//! Object decoders of the best-effort reader.
//!
//! Offsets are relative to the body of a record, which starts after its
//! header: [`Node::body`] for objects and attributed parts, [`Node::plain`]
//! and [`Node::part`] for plain parts and continuations.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{ParseLimits, Result};

use super::curve::{self, CurveKind};
use super::payload::{self, Carrier, Header, TableSpec};
use super::tree::{self, Layout, Node, TreeIssues};
use super::{
    text, BestEffortAttributes, BestEffortBlock, BestEffortColor, BestEffortDiagnostic,
    BestEffortDimensionArc, BestEffortDrawing, BestEffortEntity, BestEffortFill,
    BestEffortGeometry, BestEffortHatchLine, BestEffortHatchPattern, BestEffortLayer,
    BestEffortLimits, BestEffortLineType, BestEffortReport, BestEffortSettings, BestEffortSource,
    BestEffortTable, BestEffortText, BestEffortWidth,
};

// Object kinds, the low byte of a primary tag.
const POINT: u8 = 1;
const LINE: u8 = 2;
const CIRCLE: u8 = 3;
const ARC: u8 = 4;
const TEXT: u8 = 5;
const BLOCK_REFERENCE: u8 = 6;
const ELLIPSE: u8 = 7;
const ELLIPTICAL_ARC: u8 = 8;
const POLYLINE: u8 = 10;
const EXTENDED_POLYGON: u8 = 12;
const HATCH: u8 = 13;
const POLYLINE_3D: u8 = 18;
const LINE_3D: u8 = 19;
const NUMERIC_TEXT: u8 = 20;
const OVERLAY: u8 = 24;
const IMAGE: u8 = 25;
const DIMENSION: u8 = 26;
const LEADER: u8 = 27;
const PACKAGE: u8 = 48;
const PROPERTY: u8 = 51;
const ARROWHEAD: u8 = 52;
// Parts that only occur inside their owning object.
const DIMENSION_LINE: u8 = 0x41;
const DIMENSION_ARC: u8 = 0x42;
const DIMENSION_EXTENSION: u8 = 0x43;
const DIMENSION_ARROWS: u8 = 0x44;
const LEADER_LINE: u8 = 0x46;
const FILE_NAMES: u8 = 0x4a;
const LONG_NAME: u8 = 0x59;
const HATCH_LINE: u8 = 0x4b;
const HATCH_PATTERN: u8 = 0x4c;
const HATCH_PATTERN_VALUES: u8 = 0x4d;
const HATCH_PATTERN_NAME: u8 = 0x4e;
const DIMENSION_PAPER_ARROWS: u8 = 0x50;

const CLOSED: u8 = 0x01;
const FILL_ONLY: u8 = 0x02;
const FILLED: u8 = 0x04;
const CUSTOM_FILL: u8 = 0x08;
const THROUGH_POINTS: u8 = 0x10;
const BACKGROUND_FILL: u8 = 0x20;
/// Fill color byte that stands for an explicit RGB value.
const RGB_FILL: u8 = 0xfd;

type Decoded = std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str>;

fn number(bytes: &[u8], at: usize) -> Option<f64> {
    let raw: [u8; 8] = bytes.get(at..at + 8)?.try_into().ok()?;
    let value = f64::from_le_bytes(raw);
    value.is_finite().then_some(value)
}

fn point(bytes: &[u8], at: usize) -> Option<[f64; 2]> {
    Some([number(bytes, at)?, number(bytes, at + 8)?])
}

fn short(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn long(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn attributes(record: &[u8], layout: Layout) -> BestEffortAttributes {
    let tag_raw = short(record, 0);
    let flags_raw = (tag_raw >> 8) as u8;
    let (layer_raw, color_raw, line_type, width, material, group, link_id) = if layout.unicode {
        (
            short(record, 10),
            long(record, 4).min(u32::from(u16::MAX)) as u16,
            record[8],
            record[9],
            0,
            short(record, 16),
            short(record, 20),
        )
    } else {
        (
            short(record, 2),
            u16::from(record[4]),
            record[5],
            record[6],
            record[7],
            short(record, 10),
            short(record, 14),
        )
    };
    // A zero color defers: to the layer with tag bit 0x20, to the block with
    // 0x40, and otherwise stands for palette entry 256.
    let color = match color_raw {
        0 if flags_raw & 0x20 != 0 => 257,
        0 if flags_raw & 0x40 != 0 => 258,
        0 => 256,
        value => value,
    };
    let dependent = |value: u8, by_layer: u16, by_block: u16| match value {
        0 => by_layer,
        255 => by_block,
        value => u16::from(value),
    };
    BestEffortAttributes {
        tag_raw,
        flags_raw,
        layer: layer_raw & 0x01ff,
        segment: (layer_raw >> 9) as u8,
        color,
        line_type: dependent(line_type, 33, 34),
        width: dependent(width, 17, 18),
        material,
        group,
        link_id,
    }
}

/// Fill fields in their shared order: color, transparency, flags and RGB.
///
/// A shape is filled when its fill flag is set. Generations up to 4 have no
/// such flag; there a fill color alone fills. An explicit RGB value is marked
/// by a flag, by the reserved color byte, or by both.
fn fill(color: u8, transparency: u8, flags: u8, rgb: &[u8], marker: u16) -> Option<BestEffortFill> {
    let filled = flags & FILLED != 0 || (marker <= 4 && color != 0);
    filled.then(|| BestEffortFill {
        color: match color {
            _ if flags & BACKGROUND_FILL != 0 => 258,
            _ if flags & CUSTOM_FILL != 0 => 257,
            RGB_FILL => 257,
            0 => 256,
            value => u16::from(value),
        },
        rgb: [rgb[0], rgb[1], rgb[2]],
        transparency,
        outline: flags & FILL_ONLY == 0,
    })
}

/// Fill of a circle or ellipse, saved after the angles of its continuation.
fn shape_fill(tail: &[u8], marker: u16) -> Option<BestEffortFill> {
    fill(tail[32], tail[34], tail[35], &tail[36..39], marker)
}

struct Reader<'a> {
    marker_raw: u16,
    layout: Layout,
    limits: &'a BestEffortLimits,
    report: BestEffortReport,
    entities: u64,
    curve_points: u64,
}

impl Reader<'_> {
    fn note(
        &mut self,
        code: &'static str,
        message: impl Into<String>,
        table: Option<BestEffortTable>,
        node: Option<&Node<'_>>,
    ) {
        if (self.report.diagnostics.len() as u64) < self.limits.max_diagnostics {
            self.report.diagnostics.push(BestEffortDiagnostic {
                code,
                message: message.into(),
                table,
                record_index: node.map(|node| node.index),
                tag_raw: node.map(|node| node.tag),
            });
        } else {
            self.report.diagnostics_suppressed += 1;
        }
    }

    fn string(&self, bytes: &[u8]) -> String {
        if self.layout.unicode {
            text::utf16(bytes).0
        } else {
            text::shift_jis(bytes).0
        }
    }

    /// NUL-separated strings of a part and the continuations written after it.
    fn strings(&self, node: &Node<'_>) -> Vec<String> {
        let mut bytes = node.plain().to_vec();
        for index in 0..node.continuations() {
            if let Some(part) = node.part(index) {
                bytes.extend_from_slice(part);
            }
        }
        let unit = if self.layout.unicode { 2 } else { 1 };
        let mut result = Vec::new();
        let mut start = 0;
        for at in (0..bytes.len() / unit).map(|index| index * unit) {
            if bytes[at..at + unit].iter().all(|byte| *byte == 0) {
                if at == start {
                    break;
                }
                result.push(self.string(&bytes[start..at]));
                start = at + unit;
            }
        }
        result
    }

    fn entities(
        &mut self,
        nodes: &[Node<'_>],
        table: BestEffortTable,
        base: u64,
    ) -> Result<Vec<BestEffortEntity>> {
        let mut result = Vec::with_capacity(nodes.len());
        for node in nodes {
            if let Some(entity) = self.entity(node, table, base)? {
                result.push(entity);
            }
        }
        Ok(result)
    }

    fn entity(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
        base: u64,
    ) -> Result<Option<BestEffortEntity>> {
        let decoded: Decoded = match node.tag as u8 {
            POINT => point_geometry(node),
            LINE => line(node),
            LINE_3D => line_3d(node),
            CIRCLE => circle(node, self.marker_raw),
            ARC => arc(node),
            ELLIPSE => ellipse(node, false, self.marker_raw),
            ELLIPTICAL_ARC => ellipse(node, true, self.marker_raw),
            TEXT | NUMERIC_TEXT => self.text(node),
            BLOCK_REFERENCE => self.insert(node),
            ARROWHEAD => arrowhead(node),
            IMAGE => self.placed_file(node, true),
            OVERLAY => self.placed_file(node, false),
            POLYLINE | POLYLINE_3D => self.polyline(node, table, base)?,
            EXTENDED_POLYGON => self.extended_polygon(node, table, base)?,
            HATCH => self.hatch(node, table),
            DIMENSION => self.dimension(node, table, base)?,
            LEADER => self.leader(node, table, base)?,
            PACKAGE => self.package(node, table, base)?,
            PROPERTY => {
                self.report.property_records += 1;
                return Ok(None);
            }
            kind => {
                *self.report.unsupported_counts.entry(kind).or_default() += 1;
                return Ok(None);
            }
        };
        let (geometry, children) = match decoded {
            Ok(value) => value,
            Err(reason) => {
                *self.report.failed_counts.entry(node.tag as u8).or_default() += 1;
                self.note(
                    "object_layout_mismatch",
                    format!("{reason}; the object was skipped"),
                    Some(table),
                    Some(node),
                );
                return Ok(None);
            }
        };
        self.entities += 1;
        check_limit(
            "best_effort_entities",
            self.entities,
            self.limits.max_entities,
        )?;
        *self
            .report
            .decoded_counts
            .entry(geometry.kind())
            .or_default() += 1;
        Ok(Some(BestEffortEntity {
            attributes: attributes(node.record(), self.layout),
            source: BestEffortSource {
                table,
                record_index: node.index,
                payload_offset: base + node.index * self.layout.record as u64,
                record_count: node.record_count,
            },
            geometry,
            children,
        }))
    }

    fn text(&mut self, node: &Node<'_>) -> Decoded {
        const SHORT: &str = "text has fewer continuation records than its layout";
        const NUMBER: &str = "text has a non-finite field";
        let body = node.body();
        let layout = node.part(0).ok_or(SHORT)?;
        let font = node.part(1).ok_or(SHORT)?;
        let font_header = node.continuation(1).ok_or(SHORT)?;
        let number_record = node.continuation(2).ok_or(SHORT)?;
        let unit = if self.layout.unicode { 2 } else { 1 };
        let length = usize::from(short(layout, 0)) * unit;
        let mut text_bytes = Vec::with_capacity(length);
        'body: for index in 2..node.continuations() {
            let Some(part) = node.part(index) else {
                break;
            };
            for unit_bytes in part.chunks_exact(unit) {
                if text_bytes.len() == length || unit_bytes.iter().all(|byte| *byte == 0) {
                    break 'body;
                }
                text_bytes.extend_from_slice(unit_bytes);
            }
        }
        if text_bytes.len() != length {
            return Err("text body is shorter than its declared length");
        }
        let decoded = if self.layout.unicode {
            text::wide_body(&text_bytes)
        } else {
            text::body(&text_bytes)
        };
        let numeric = node.tag as u8 == NUMERIC_TEXT;
        Ok((
            BestEffortGeometry::Text(Box::new(BestEffortText {
                position: point(body, 0).ok_or(NUMBER)?,
                elevation: number(body, 16).ok_or(NUMBER)?,
                text: decoded.text,
                text_bytes,
                width: number(body, 24).ok_or(NUMBER)?,
                height: number(body, 32).ok_or(NUMBER)?,
                spacing: number(body, 40).ok_or(NUMBER)?,
                rotation_degrees: number(font, 0).ok_or(NUMBER)?,
                slant_degrees: number(font, 8).ok_or(NUMBER)?,
                origin: short(layout, 2),
                flags_raw: short(font_header, 6),
                font_name: text::font_name(&font[16..], self.marker_raw),
                charset: font_header[8],
                family: font_header[9],
                numeric,
                number: if numeric {
                    number(number_record, 8)
                } else {
                    None
                },
                unit_raw: short(font_header, 2),
                item_raw: short(font_header, 4),
                format_codes: decoded.format_codes,
                lossy: decoded.lossy,
            })),
            Vec::new(),
        ))
    }

    fn insert(&mut self, node: &Node<'_>) -> Decoded {
        const NUMBER: &str = "block reference has a non-finite field";
        let tail = node
            .part(0)
            .ok_or("block reference has no continuation record")?;
        let field = &node.body()[16..];
        let unit = if self.layout.unicode { 2 } else { 1 };
        let end = field
            .chunks_exact(unit)
            .position(|bytes| bytes.iter().all(|byte| *byte == 0))
            .map_or(field.len(), |units| units * unit);
        // A name too long for its field is saved in full in a part of its own;
        // the field then holds an alias.
        let long_name = node
            .children
            .iter()
            .find(|child| child.tag as u8 == LONG_NAME)
            .map(|child| {
                let bytes = child.plain();
                let end = bytes
                    .chunks_exact(unit)
                    .position(|unit| unit.iter().all(|byte| *byte == 0))
                    .map_or(bytes.len(), |units| units * unit);
                &bytes[..end]
            });
        let name_bytes = long_name.unwrap_or(&field[..end]);
        Ok((
            BestEffortGeometry::Insert {
                name: self.string(name_bytes),
                name_bytes: name_bytes.to_vec(),
                reference_number: long(node.body(), 4),
                position: point(tail, 0).ok_or(NUMBER)?,
                elevation: number(tail, 16).ok_or(NUMBER)?,
                scale: point(tail, 24).ok_or(NUMBER)?,
                rotation_degrees: number(tail, 40).ok_or(NUMBER)?,
            },
            Vec::new(),
        ))
    }

    /// An image or an external drawing: a placement and the names of a file.
    fn placed_file(&mut self, node: &Node<'_>, image: bool) -> Decoded {
        const SHORT: &str = "placed file has fewer continuation records than its layout";
        const NUMBER: &str = "placed file has a non-finite field";
        let placement = node.part(0).ok_or(SHORT)?;
        let extent = node.part(1).ok_or(SHORT)?;
        let mut names = node
            .children
            .iter()
            .find(|child| child.tag as u8 == FILE_NAMES)
            .map(|child| self.strings(child))
            .unwrap_or_default()
            .into_iter();
        let file_name = names.next().unwrap_or_default();
        let directory = names.next().unwrap_or_default();
        let position = point(placement, 0).ok_or(NUMBER)?;
        let elevation = number(placement, 16).ok_or(NUMBER)?;
        let scale = point(placement, 24).ok_or(NUMBER)?;
        let geometry = if image {
            BestEffortGeometry::Image {
                file_name,
                directory,
                position,
                elevation,
                scale,
                size: point(extent, 0).ok_or(NUMBER)?,
                rotation_degrees: number(extent, 16).ok_or(NUMBER)?,
            }
        } else {
            BestEffortGeometry::ExternalReference {
                file_name,
                directory,
                relative_directory: names.next().unwrap_or_default(),
                position,
                elevation,
                scale,
                // Rotations about three axes are saved; the last is in the plane.
                rotation_degrees: number(extent, 16).ok_or(NUMBER)?,
            }
        };
        Ok((geometry, Vec::new()))
    }

    fn polyline(&mut self, node: &Node<'_>, table: BestEffortTable, base: u64) -> Result<Decoded> {
        let body = node.body();
        let (curve_kind, flags_raw, divisions) = (body[0], body[1], body[3]);
        let children = self.entities(&node.children, table, base)?;
        let kind = match curve_kind {
            0 => None,
            2 => Some(CurveKind::Quadratic),
            3 if flags_raw & THROUGH_POINTS != 0 => Some(CurveKind::CubicThrough),
            3 => Some(CurveKind::Cubic),
            4 => Some(CurveKind::Bezier),
            // A saved evaluation of the spline next to it.
            1 => None,
            _ => {
                self.note(
                    "curve_kind_not_evaluated",
                    "the curve kind of this polyline is unknown; its stored points are returned",
                    Some(table),
                    Some(node),
                );
                None
            }
        };
        let mut curve_points = None;
        if let (Some(kind), true) = (kind, divisions != 0) {
            if let Some(stored) = stored_points(&children) {
                let closed = flags_raw & CLOSED != 0;
                let repeated = stored.first() == stored.last();
                let spans = curve::spans(kind, stored.len(), closed, repeated) as u64;
                self.curve_points = self
                    .curve_points
                    .saturating_add(spans.saturating_mul(u64::from(divisions)) + 1);
                check_limit(
                    "best_effort_curve_points",
                    self.curve_points,
                    self.limits.max_curve_points,
                )?;
                curve_points = Some(curve::evaluate(
                    kind,
                    &stored,
                    usize::from(divisions),
                    closed,
                ));
            } else {
                self.note(
                    "curve_points_unusable",
                    "curve members are not one connected run of lines; no curve was evaluated",
                    Some(table),
                    Some(node),
                );
            }
        }
        Ok(Ok((
            BestEffortGeometry::Polyline {
                curve_kind,
                flags_raw,
                divisions,
                three_d: node.tag as u8 == POLYLINE_3D,
                fill: fill(body[2], body[6], flags_raw, &body[8..11], self.marker_raw),
                curve_points,
            },
            children,
        )))
    }

    fn extended_polygon(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
        base: u64,
    ) -> Result<Decoded> {
        let body = node.body();
        let children = self.entities(&node.children, table, base)?;
        let flags_raw = body[1];
        Ok(Ok((
            BestEffortGeometry::ExtendedPolygon {
                polygon_kind: body[0],
                flags_raw,
                fill: fill(body[2], body[6], flags_raw, &body[8..11], self.marker_raw),
                reference_number: u32::from(short(body, 4)),
                hole_count: short(body, 14),
                link_id: u32::from(short(body, 12)),
            },
            children,
        )))
    }

    fn hatch(&mut self, node: &Node<'_>, table: BestEffortTable) -> Decoded {
        const NUMBER: &str = "hatch part has a non-finite field";
        let body = node.body();
        let mut lines = Vec::new();
        let mut pattern = None;
        let mut unknown_parts = 0u64;
        for child in &node.children {
            let part = child.body();
            let style = attributes(child.record(), self.layout);
            match child.tag as u8 {
                HATCH_LINE => lines.push(BestEffortHatchLine {
                    origin: point(part, 0).ok_or(NUMBER)?,
                    pitch: number(part, 16).ok_or(NUMBER)?,
                    angle_degrees: number(part, 24).ok_or(NUMBER)?,
                    color: style.color,
                    line_type: style.line_type,
                    width: style.width,
                }),
                HATCH_PATTERN => {
                    let mut values = Vec::new();
                    for at in [16, 24, 32, 40] {
                        values.push(number(part, at).ok_or(NUMBER)?);
                    }
                    let mut name = String::new();
                    for detail in &child.children {
                        match detail.tag as u8 {
                            HATCH_PATTERN_VALUES => {
                                for at in [0, 8, 16] {
                                    values.push(number(detail.plain(), at).ok_or(NUMBER)?);
                                }
                            }
                            HATCH_PATTERN_NAME => {
                                name = self.strings(detail).into_iter().next().unwrap_or_default();
                            }
                            _ => unknown_parts += 1,
                        }
                    }
                    pattern = Some(BestEffortHatchPattern {
                        name,
                        origin: point(part, 0).ok_or(NUMBER)?,
                        values,
                        color: style.color,
                        line_type: style.line_type,
                        width: style.width,
                    });
                }
                _ => unknown_parts += 1,
            }
        }
        if unknown_parts != 0 {
            self.note(
                "hatch_parts_ignored",
                format!("{unknown_parts} hatch part(s) of an unknown kind were ignored"),
                Some(table),
                Some(node),
            );
        }
        Ok((
            BestEffortGeometry::Hatch {
                hatch_type: short(body, 0),
                link_id: u32::from(short(body, 2)),
                lines,
                pattern,
            },
            Vec::new(),
        ))
    }

    fn dimension(&mut self, node: &Node<'_>, table: BestEffortTable, base: u64) -> Result<Decoded> {
        const NUMBER: &str = "dimension part has a non-finite field";
        let body = node.body();
        let mut line = None;
        let mut arc = None;
        let mut extension_origins = Vec::new();
        let mut arrow_sizes = None;
        let mut arrow_paper_sizes = None;
        let mut children = Vec::new();
        let mut unknown_parts = 0u64;
        for child in &node.children {
            let kind = child.tag as u8;
            if matches!(kind, TEXT | NUMERIC_TEXT) {
                if let Some(entity) = self.entity(child, table, base)? {
                    children.push(entity);
                }
                continue;
            }
            let part = child.plain();
            let pair = || Some((point(part, 0)?, point(part, 16)?));
            match kind {
                DIMENSION_LINE => match pair() {
                    Some((start, end)) => line = Some([start, end]),
                    None => return Ok(Err(NUMBER)),
                },
                DIMENSION_ARC => match (pair(), number(part, 32)) {
                    (Some((center, [radius, start_degrees])), Some(end_degrees)) => {
                        arc = Some(BestEffortDimensionArc {
                            center,
                            radius,
                            start_degrees,
                            end_degrees,
                        });
                    }
                    _ => return Ok(Err(NUMBER)),
                },
                DIMENSION_EXTENSION => match point(part, 0) {
                    Some(origin) => extension_origins.push(origin),
                    None => return Ok(Err(NUMBER)),
                },
                DIMENSION_ARROWS | DIMENSION_PAPER_ARROWS => match pair() {
                    Some((first, second)) => {
                        let sizes = Some([first[0], first[1], second[0], second[1]]);
                        if kind == DIMENSION_ARROWS {
                            arrow_sizes = sizes;
                        } else {
                            arrow_paper_sizes = sizes;
                        }
                    }
                    None => return Ok(Err(NUMBER)),
                },
                _ => unknown_parts += 1,
            }
        }
        if unknown_parts != 0 {
            self.note(
                "dimension_parts_ignored",
                format!("{unknown_parts} dimension part(s) of an unknown kind were ignored"),
                Some(table),
                Some(node),
            );
        }
        Ok(Ok((
            BestEffortGeometry::Dimension {
                dimension_type: body[0],
                flags_raw: body[1],
                flags2_raw: body[2],
                arrow_styles: [body[3], body[4], body[5], body[6]],
                text_position: body[7],
                line,
                arc,
                extension_origins,
                arrow_sizes,
                arrow_paper_sizes,
            },
            children,
        )))
    }

    fn leader(&mut self, node: &Node<'_>, table: BestEffortTable, base: u64) -> Result<Decoded> {
        const NUMBER: &str = "leader part has a non-finite field";
        let body = node.body();
        let mut lines = Vec::new();
        let mut children = Vec::new();
        let mut unknown_parts = 0u64;
        for child in &node.children {
            match child.tag as u8 {
                TEXT | NUMERIC_TEXT => {
                    if let Some(entity) = self.entity(child, table, base)? {
                        children.push(entity);
                    }
                }
                LEADER_LINE => match (point(child.plain(), 0), point(child.plain(), 16)) {
                    (Some(start), Some(end)) => lines.push([start, end]),
                    _ => return Ok(Err(NUMBER)),
                },
                _ => unknown_parts += 1,
            }
        }
        if unknown_parts != 0 {
            self.note(
                "leader_parts_ignored",
                format!("{unknown_parts} leader part(s) of an unknown kind were ignored"),
                Some(table),
                Some(node),
            );
        }
        let (Some(arrow_paper_size), Some(arrow_size)) = (point(body, 16), point(body, 32)) else {
            return Ok(Err(NUMBER));
        };
        Ok(Ok((
            BestEffortGeometry::Leader {
                leader_type: body[0],
                flags_raw: body[1],
                flags2_raw: body[2],
                arrow_style: body[3],
                lines,
                arrow_size,
                arrow_paper_size,
            },
            children,
        )))
    }

    fn package(&mut self, node: &Node<'_>, table: BestEffortTable, base: u64) -> Result<Decoded> {
        let body = node.body();
        let children = self.entities(&node.children, table, base)?;
        Ok(Ok((
            BestEffortGeometry::Package {
                package_kind: body[0],
                flags_raw: body[1],
                reference_number: long(body, 4),
            },
            children,
        )))
    }
}

fn point_geometry(node: &Node<'_>) -> Decoded {
    let body = node.body();
    Ok((
        BestEffortGeometry::Point {
            position: point(body, 0).ok_or("point has a non-finite coordinate")?,
            point_type: long(body, 16),
        },
        Vec::new(),
    ))
}

fn line(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "line has a non-finite field";
    let body = node.body();
    Ok((
        BestEffortGeometry::Line {
            start: point(body, 0).ok_or(NUMBER)?,
            end: point(body, 16).ok_or(NUMBER)?,
            elevation: number(body, 32).ok_or(NUMBER)?,
            thickness: number(body, 40).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn line_3d(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "3D line has a non-finite coordinate";
    let body = node.body();
    let triple = |at| -> Option<[f64; 3]> {
        Some([
            number(body, at)?,
            number(body, at + 8)?,
            number(body, at + 16)?,
        ])
    };
    Ok((
        BestEffortGeometry::Line3D {
            start: triple(0).ok_or(NUMBER)?,
            end: triple(24).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn circle(node: &Node<'_>, marker: u16) -> Decoded {
    const NUMBER: &str = "circle has a non-finite field";
    let body = node.body();
    let tail = node.part(0).ok_or("circle has no continuation record")?;
    let radius = number(tail, 0).ok_or(NUMBER)?;
    if radius <= 0.0 {
        return Err("circle radius is not positive");
    }
    Ok((
        BestEffortGeometry::Circle {
            center: point(body, 0).ok_or(NUMBER)?,
            radius,
            elevation: number(body, 32).ok_or(NUMBER)?,
            fill: shape_fill(tail, marker),
        },
        Vec::new(),
    ))
}

fn arc(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "arc has a non-finite field";
    let body = node.body();
    let tail = node.part(0).ok_or("arc has no continuation record")?;
    let radius = number(tail, 0).ok_or(NUMBER)?;
    if radius <= 0.0 {
        return Err("arc radius is not positive");
    }
    Ok((
        BestEffortGeometry::Arc {
            center: point(body, 0).ok_or(NUMBER)?,
            radius,
            start_degrees: number(tail, 8).ok_or(NUMBER)?,
            end_degrees: number(tail, 16).ok_or(NUMBER)?,
            elevation: number(body, 32).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn ellipse(node: &Node<'_>, partial: bool, marker: u16) -> Decoded {
    const NUMBER: &str = "ellipse has a non-finite field";
    let body = node.body();
    let tail = node.part(0).ok_or("ellipse has no continuation record")?;
    let center = point(body, 0).ok_or(NUMBER)?;
    let radius = point(body, 16).ok_or(NUMBER)?;
    if radius[0] <= 0.0 || radius[1] <= 0.0 {
        return Err("ellipse radius is not positive");
    }
    let rotation_degrees = number(tail, 0).ok_or(NUMBER)?;
    let geometry = if partial {
        BestEffortGeometry::EllipticalArc {
            center,
            radius,
            rotation_degrees,
            start_degrees: number(tail, 8).ok_or(NUMBER)?,
            end_degrees: number(tail, 16).ok_or(NUMBER)?,
        }
    } else {
        BestEffortGeometry::Ellipse {
            center,
            radius,
            rotation_degrees,
            fill: shape_fill(tail, marker),
        }
    };
    Ok((geometry, Vec::new()))
}

fn arrowhead(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "arrowhead has a non-finite field";
    let body = node.body();
    Ok((
        BestEffortGeometry::Arrowhead {
            position: point(body, 0).ok_or(NUMBER)?,
            size: point(body, 16).ok_or(NUMBER)?,
            angle_degrees: number(body, 32).ok_or(NUMBER)?,
            arrow_type: body[40],
            flags_raw: body[41],
        },
        Vec::new(),
    ))
}

/// Points of a polyline whose members are one connected run of 2D lines.
fn stored_points(members: &[BestEffortEntity]) -> Option<Vec<[f64; 2]>> {
    let mut points = Vec::with_capacity(members.len() + 1);
    for member in members {
        let BestEffortGeometry::Line { start, end, .. } = member.geometry else {
            return None;
        };
        match points.last() {
            None => points.push(start),
            Some(last) if *last == start => {}
            Some(_) => return None,
        }
        points.push(end);
    }
    (points.len() >= 2).then_some(points)
}

fn rows(bytes: &[u8], spec: TableSpec) -> Option<impl Iterator<Item = (u16, &[u8])>> {
    let start = usize::try_from(spec.start).ok()?;
    let table = bytes.get(start..start.checked_add(spec.rows.checked_mul(spec.row)?)?)?;
    Some(
        table
            .chunks_exact(spec.row)
            .enumerate()
            .map(move |(index, row)| (spec.first + index as u16, row)),
    )
}

struct Tables {
    layers: Vec<BestEffortLayer>,
    colors: Vec<BestEffortColor>,
    line_types: Vec<BestEffortLineType>,
    widths: Vec<BestEffortWidth>,
}

fn tables(bytes: &[u8], header: &Header, tail: Option<u64>) -> Option<Tables> {
    let specs = header.tables.as_ref()?;
    let unicode = header.layout.unicode;
    let (name_at, name_size) = specs.layer_name;
    let layer = |index: u16, row: &[u8]| {
        let field = &row[name_at..name_at + name_size];
        let flags_raw = short(row, 0);
        BestEffortLayer {
            index,
            name: if unicode {
                text::utf16(field).0
            } else {
                text::shift_jis(field).0
            },
            flags_raw,
            hidden: flags_raw & 0x0100 != 0,
            color: short(row, 4),
            line_type: short(row, 6),
            width: short(row, 8),
        }
    };
    let color = |index: u16, row: &[u8]| BestEffortColor {
        index,
        rgb: [row[4], row[5], row[6]],
        pen_width: f64::from(short(row, 2)) / 100.0,
    };
    // The older layout keeps color 256 and the layers 0 and 256 in its tail.
    let extension = |offset: u64| {
        tail.and_then(|tail| {
            let start = usize::try_from(tail + offset).ok()?;
            bytes.get(start..start + 64)
        })
    };
    let mut layers = Vec::with_capacity(257);
    if let Some(row) = extension(96) {
        layers.push(layer(0, row));
    }
    layers.extend(
        rows(bytes, specs.layers)?
            .filter(|(index, _)| *index <= 256)
            .map(|(index, row)| layer(index, row)),
    );
    if let Some(row) = extension(408) {
        layers.push(layer(256, row));
    }
    // The Unicode layout reserves more rows than the application uses; both
    // layouts return 256 colors, 32 line types and 16 widths.
    let mut colors: Vec<_> = rows(bytes, specs.colors)?
        .filter(|(index, _)| (1..=256).contains(index))
        .map(|(index, row)| color(index, row))
        .collect();
    if let Some(row) = extension(16) {
        colors.push(color(256, row));
    }
    let line_types = rows(bytes, specs.line_types)?
        .filter(|(index, _)| (1..=32).contains(index))
        .map(|(index, row)| {
            let mut pattern: Vec<f64> = (0..30)
                .map(|slot| f64::from(short(row, 4 + slot * 2)) / 100.0)
                .collect();
            while pattern.last() == Some(&0.0) {
                pattern.pop();
            }
            BestEffortLineType {
                index,
                pen_style: short(row, 2),
                pattern,
            }
        })
        .collect();
    let widths = rows(bytes, specs.widths)?
        .filter(|(index, _)| (1..=16).contains(index))
        .map(|(index, row)| BestEffortWidth {
            index,
            width: f64::from(short(row, 2)) / 100.0,
        })
        .collect();
    Some(Tables {
        layers,
        colors,
        line_types,
        widths,
    })
}

fn settings(bytes: &[u8], layout: Layout) -> Option<BestEffortSettings> {
    // The Unicode layout has the same fields 64 bytes further on.
    let bytes = bytes.get(if layout.unicode { 0x40 } else { 0 }..)?;
    let factor = number(bytes, 0x178)?;
    if factor <= 0.0 {
        return None;
    }
    let drawing_scale = 1.0 / factor;
    drawing_scale.is_finite().then_some(BestEffortSettings {
        drawing_origin: point(bytes, 0x140)?,
        drawing_scale,
        paper_size: point(bytes, 0x188)?,
        paper_origin: point(bytes, 0x198)?,
    })
}

fn table_records(bytes: &[u8], start: u64, count: u64, record: usize) -> Result<&[u8]> {
    let end = count
        .checked_mul(record as u64)
        .and_then(|size| start.checked_add(size))
        .ok_or_else(|| invalid("record table extent overflow"))?;
    let range = usize::try_from(start).map_err(|_| invalid("record table offset is too large"))?
        ..usize::try_from(end).map_err(|_| invalid("record table offset is too large"))?;
    bytes
        .get(range)
        .ok_or_else(|| invalid("record table exceeds the payload"))
}

pub(crate) fn drawing(
    carrier: &Carrier,
    parse_limits: &ParseLimits,
    limits: &BestEffortLimits,
) -> Result<BestEffortDrawing> {
    let bytes: &[u8] = &carrier.payload;
    let header = payload::parse_header(bytes, carrier.format, parse_limits)?;
    let layout = header.layout;
    let root = table_records(
        bytes,
        header.record_start,
        header.record_count,
        layout.record,
    )?;
    let (root, terminal) = root.split_at(root.len() - layout.record);
    if terminal[0] != 0xff {
        return Err(invalid("record table has no terminal record"));
    }
    // Generations 3 to 25 are the ones the application can save.
    let marker_observed = (3..=25).contains(&header.marker_raw);
    let mut reader = Reader {
        marker_raw: header.marker_raw,
        layout,
        limits,
        report: BestEffortReport::default(),
        entities: 0,
        curve_points: 0,
    };
    if !marker_observed {
        reader.note(
            "generation_not_observed",
            format!(
                "record generation {} has not been checked; objects are decoded with the known layouts",
                header.marker_raw
            ),
            None,
            None,
        );
    }
    let mut issues = TreeIssues::default();
    let nodes = tree::parse(
        root,
        layout,
        0,
        root.len() / layout.record,
        0,
        limits.max_depth,
        &mut issues,
    )?;
    let entities = reader.entities(&nodes, BestEffortTable::Root, header.record_start)?;
    drop(nodes);

    let (ranges, tail, unbounded) = payload::block_ranges(
        bytes,
        &header,
        limits.max_block_definitions,
        parse_limits.max_records,
    )?;
    if unbounded && header.block_start != 0 && ranges.is_empty() {
        reader.note(
            "block_definitions_not_read",
            "the block table does not start with a definition of the known layout",
            None,
            None,
        );
    }
    let mut blocks = Vec::with_capacity(ranges.len());
    for (index, range) in ranges.iter().enumerate() {
        let table = BestEffortTable::Block(index as u64);
        let records = table_records(bytes, range.record_start, range.record_count, layout.record)?;
        let nodes = tree::parse(
            records,
            layout,
            0,
            records.len() / layout.record,
            0,
            limits.max_depth,
            &mut issues,
        )?;
        blocks.push(BestEffortBlock {
            index: index as u64,
            name: reader.string(&range.name_bytes),
            name_bytes: range.name_bytes.clone(),
            header_offset: range.header_offset,
            entities: reader.entities(&nodes, table, range.record_start)?,
        });
    }
    reader.report.orphan_records = issues.orphan_records;
    reader.report.clamped_spans = issues.clamped_spans;

    let tables = tables(bytes, &header, tail);
    if tables.is_none() {
        reader.note(
            "style_tables_not_read",
            "the layer, color, line-type and width tables do not have a known size",
            None,
            None,
        );
    }
    let tables = tables.unwrap_or(Tables {
        layers: Vec::new(),
        colors: Vec::new(),
        line_types: Vec::new(),
        widths: Vec::new(),
    });
    Ok(BestEffortDrawing {
        format: carrier.format,
        marker_raw: header.marker_raw,
        marker_observed,
        payload_size: bytes.len() as u64,
        payload_sha256: carrier.sha256.clone(),
        record_count: header.record_count,
        settings: settings(bytes, layout),
        layers: tables.layers,
        colors: tables.colors,
        line_types: tables.line_types,
        widths: tables.widths,
        blocks,
        entities,
        report: reader.report,
    })
}
