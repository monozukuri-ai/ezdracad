//! Object decoders of the best-effort reader.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{ParseLimits, Result, RECORD_SIZE};

use super::payload::{self, Carrier, Header};
use super::tree::{self, Node, TreeIssues};
use super::{
    curve, text, BestEffortAttributes, BestEffortBlock, BestEffortColor, BestEffortDiagnostic,
    BestEffortDrawing, BestEffortEntity, BestEffortGeometry, BestEffortHatchLine, BestEffortLayer,
    BestEffortLimits, BestEffortLineType, BestEffortReport, BestEffortSettings, BestEffortSource,
    BestEffortTable, BestEffortText, BestEffortWidth,
};

const RECORD: usize = RECORD_SIZE as usize;

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
const DIMENSION: u8 = 26;
const PACKAGE: u8 = 48;
const PROPERTY: u8 = 51;
const ARROWHEAD: u8 = 52;
// Parts that only occur inside their owning object.
const DIMENSION_LINE: u8 = 0x41;
const DIMENSION_EXTENSION: u8 = 0x43;
const DIMENSION_ARROWS: u8 = 0x44;
const DIMENSION_PAPER_ARROWS: u8 = 0x50;
const HATCH_LINE: u8 = 0x4b;
const HATCH_PATTERN: u8 = 0x4c;

const CUBIC_SPLINE: u8 = 3;
const POLYLINE_CLOSED: u8 = 0x01;

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

fn attributes(head: &[u8]) -> BestEffortAttributes {
    let tag_raw = short(head, 0);
    let flags_raw = (tag_raw >> 8) as u8;
    let layer_raw = short(head, 2);
    // The color byte is extended by three tag bits: 0x04 alone selects entry
    // 256, with 0x20 the layer's color and with 0x40 the block's color.
    let color = match (head[4], flags_raw & 0x64) {
        (0, 0x24) => 257,
        (0, 0x44) => 258,
        (0, 0x04) => 256,
        (value, _) => u16::from(value),
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
        line_type: dependent(head[5], 33, 34),
        width: dependent(head[6], 17, 18),
        material: head[7],
        group: short(head, 10),
        link_id: short(head, 14),
    }
}

struct Reader<'a> {
    marker_raw: u16,
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

    fn failed(&mut self, node: &Node<'_>, table: BestEffortTable, reason: &str) {
        *self.report.failed_counts.entry(node.tag as u8).or_default() += 1;
        self.note(
            "object_layout_mismatch",
            format!("{reason}; the object was skipped"),
            Some(table),
            Some(node),
        );
    }

    fn unsupported(&mut self, node: &Node<'_>) {
        *self
            .report
            .unsupported_counts
            .entry(node.tag as u8)
            .or_default() += 1;
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
        let kind = node.tag as u8;
        let decoded: std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &str> =
            match kind {
                POINT => point_geometry(node),
                LINE => line(node),
                LINE_3D => line_3d(node),
                CIRCLE => circle(node),
                ARC => arc(node),
                ELLIPSE => ellipse(node, false),
                ELLIPTICAL_ARC => ellipse(node, true),
                TEXT | NUMERIC_TEXT => self.text(node),
                BLOCK_REFERENCE => insert(node),
                ARROWHEAD => arrowhead(node),
                POLYLINE | POLYLINE_3D => self.polyline(node, table, base)?,
                EXTENDED_POLYGON => self.extended_polygon(node, table, base)?,
                HATCH => self.hatch(node, table),
                DIMENSION => self.dimension(node, table, base)?,
                PACKAGE => self.package(node, table, base)?,
                PROPERTY => {
                    self.report.property_records += 1;
                    return Ok(None);
                }
                _ => {
                    self.unsupported(node);
                    return Ok(None);
                }
            };
        let (geometry, children) = match decoded {
            Ok(value) => value,
            Err(reason) => {
                self.failed(node, table, reason);
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
            attributes: attributes(node.head()),
            source: BestEffortSource {
                table,
                record_index: node.index,
                payload_offset: base + node.index * RECORD_SIZE,
                record_count: node.record_count,
            },
            geometry,
            children,
        }))
    }

    fn text(
        &mut self,
        node: &Node<'_>,
    ) -> std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str> {
        const SHORT: &str = "text has fewer continuation records than its layout";
        const NUMBER: &str = "text has a non-finite field";
        let head = node.head();
        let layout = node.continuation(0).ok_or(SHORT)?;
        let font = node.continuation(1).ok_or(SHORT)?;
        let first_body = node.continuation(2).ok_or(SHORT)?;
        let position = point(head, 16).ok_or(NUMBER)?;
        let length = usize::from(short(layout, 16));
        let mut text_bytes = Vec::with_capacity(length);
        'body: for index in 2..node.continuations() {
            let Some(record) = node.continuation(index) else {
                break;
            };
            for byte in &record[16..] {
                if text_bytes.len() == length || *byte == 0 {
                    break 'body;
                }
                text_bytes.push(*byte);
            }
        }
        if text_bytes.len() != length {
            return Err("text body is shorter than its declared length");
        }
        let decoded = text::body(&text_bytes);
        // Newer generations store the font name as UTF-16, older ones as Shift_JIS.
        let (font_name, _) = if self.marker_raw >= 25 {
            text::utf16(&font[32..])
        } else {
            text::shift_jis(&font[32..])
        };
        let numeric = node.tag as u8 == NUMERIC_TEXT;
        Ok((
            BestEffortGeometry::Text(Box::new(BestEffortText {
                position,
                elevation: number(head, 32).ok_or(NUMBER)?,
                text: decoded.text,
                text_bytes,
                width: number(head, 40).ok_or(NUMBER)?,
                height: number(head, 48).ok_or(NUMBER)?,
                spacing: number(head, 56).ok_or(NUMBER)?,
                rotation_degrees: number(font, 16).ok_or(NUMBER)?,
                slant_degrees: number(font, 24).ok_or(NUMBER)?,
                origin: short(layout, 18),
                flags_raw: short(font, 6),
                font_name,
                charset: font[8],
                family: font[9],
                numeric,
                number: if numeric { number(first_body, 8) } else { None },
                unit_raw: short(font, 2),
                item_raw: short(font, 4),
                format_codes: decoded.format_codes,
                lossy: decoded.lossy,
            })),
            Vec::new(),
        ))
    }

    #[allow(clippy::type_complexity)]
    fn polyline(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
        base: u64,
    ) -> Result<std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str>>
    {
        let head = node.head();
        let (curve_kind, flags_raw, fill_color, divisions) =
            (head[16], head[17], head[18], head[19]);
        let children = self.entities(&node.children, table, base)?;
        let mut curve_points = None;
        if curve_kind == CUBIC_SPLINE && divisions != 0 {
            if let Some(control) = control_polygon(&children) {
                let spans = control.len().saturating_sub(1) as u64;
                self.curve_points = self
                    .curve_points
                    .saturating_add(spans.saturating_mul(u64::from(divisions)) + 1);
                check_limit(
                    "best_effort_curve_points",
                    self.curve_points,
                    self.limits.max_curve_points,
                )?;
                curve_points = Some(curve::cubic_b_spline(
                    &control,
                    usize::from(divisions),
                    flags_raw & POLYLINE_CLOSED != 0,
                ));
            } else {
                self.note(
                    "spline_control_polygon_unusable",
                    "spline members are not one connected run of lines; no curve was evaluated",
                    Some(table),
                    Some(node),
                );
            }
        } else if curve_kind != 0 {
            self.note(
                "spline_kind_not_evaluated",
                "only cubic splines are evaluated; the control polygon is returned",
                Some(table),
                Some(node),
            );
        }
        Ok(Ok((
            BestEffortGeometry::Polyline {
                curve_kind,
                flags_raw,
                fill_color,
                divisions,
                three_d: node.tag as u8 == POLYLINE_3D,
                curve_points,
            },
            children,
        )))
    }

    #[allow(clippy::type_complexity)]
    fn extended_polygon(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
        base: u64,
    ) -> Result<std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str>>
    {
        let head = node.head();
        let children = self.entities(&node.children, table, base)?;
        Ok(Ok((
            BestEffortGeometry::ExtendedPolygon {
                polygon_kind: head[16],
                flags_raw: head[17],
                fill_color: head[18],
                loop_count: long(head, 20),
                link_id: long(head, 28),
            },
            children,
        )))
    }

    fn hatch(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
    ) -> std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str> {
        let head = node.head();
        let mut lines = Vec::new();
        let mut pattern_origin = None;
        let mut unknown_parts = 0u64;
        for child in &node.children {
            let record = child.head();
            match child.tag as u8 {
                HATCH_LINE => lines.push(BestEffortHatchLine {
                    origin: point(record, 16).ok_or("hatch line has a non-finite field")?,
                    pitch: number(record, 32).ok_or("hatch line has a non-finite field")?,
                    angle_degrees: number(record, 40).ok_or("hatch line has a non-finite field")?,
                }),
                HATCH_PATTERN => {
                    pattern_origin =
                        Some(point(record, 16).ok_or("hatch pattern has a non-finite field")?);
                }
                _ => unknown_parts += 1,
            }
        }
        if pattern_origin.is_some() {
            self.note(
                "hatch_pattern_not_decoded",
                "a named hatch pattern keeps its origin only; its lines are not decoded",
                Some(table),
                Some(node),
            );
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
                hatch_type: short(head, 16),
                link_id: long(head, 18),
                lines,
                pattern_origin,
            },
            Vec::new(),
        ))
    }

    #[allow(clippy::type_complexity)]
    fn dimension(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
        base: u64,
    ) -> Result<std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str>>
    {
        const NUMBER: &str = "dimension part has a non-finite field";
        let head = node.head();
        let mut line = None;
        let mut extension_origins = Vec::new();
        let mut arrow_sizes = None;
        let mut arrow_paper_sizes = None;
        let mut children = Vec::new();
        let mut unknown_parts = 0u64;
        for child in &node.children {
            let record = child.head();
            match child.tag as u8 {
                DIMENSION_LINE => match (point(record, 16), point(record, 32)) {
                    (Some(start), Some(end)) => line = Some([start, end]),
                    _ => return Ok(Err(NUMBER)),
                },
                DIMENSION_EXTENSION => match point(record, 16) {
                    Some(origin) => extension_origins.push(origin),
                    None => return Ok(Err(NUMBER)),
                },
                kind @ (DIMENSION_ARROWS | DIMENSION_PAPER_ARROWS) => {
                    match (point(record, 16), point(record, 32)) {
                        (Some(first), Some(second)) => {
                            let sizes = Some([first[0], first[1], second[0], second[1]]);
                            if kind == DIMENSION_ARROWS {
                                arrow_sizes = sizes;
                            } else {
                                arrow_paper_sizes = sizes;
                            }
                        }
                        _ => return Ok(Err(NUMBER)),
                    }
                }
                TEXT | NUMERIC_TEXT => {
                    if let Some(entity) = self.entity(child, table, base)? {
                        children.push(entity);
                    }
                }
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
                dimension_type: head[16],
                flags_raw: head[17],
                flags2_raw: head[18],
                arrow_styles: [head[19], head[20], head[21], head[22]],
                text_position: head[23],
                line,
                extension_origins,
                arrow_sizes,
                arrow_paper_sizes,
            },
            children,
        )))
    }

    #[allow(clippy::type_complexity)]
    fn package(
        &mut self,
        node: &Node<'_>,
        table: BestEffortTable,
        base: u64,
    ) -> Result<std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str>>
    {
        let head = node.head();
        let children = self.entities(&node.children, table, base)?;
        Ok(Ok((
            BestEffortGeometry::Package {
                package_kind: head[16],
                flags_raw: head[17],
                reference_number: long(head, 20),
            },
            children,
        )))
    }
}

type Decoded = std::result::Result<(BestEffortGeometry, Vec<BestEffortEntity>), &'static str>;

fn point_geometry(node: &Node<'_>) -> Decoded {
    let head = node.head();
    Ok((
        BestEffortGeometry::Point {
            position: point(head, 16).ok_or("point has a non-finite coordinate")?,
            point_type: long(head, 32),
        },
        Vec::new(),
    ))
}

fn line(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "line has a non-finite field";
    let head = node.head();
    Ok((
        BestEffortGeometry::Line {
            start: point(head, 16).ok_or(NUMBER)?,
            end: point(head, 32).ok_or(NUMBER)?,
            elevation: number(head, 48).ok_or(NUMBER)?,
            thickness: number(head, 56).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn line_3d(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "3D line has a non-finite coordinate";
    let head = node.head();
    let triple = |at| -> Option<[f64; 3]> {
        Some([
            number(head, at)?,
            number(head, at + 8)?,
            number(head, at + 16)?,
        ])
    };
    Ok((
        BestEffortGeometry::Line3D {
            start: triple(16).ok_or(NUMBER)?,
            end: triple(40).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn circle(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "circle has a non-finite field";
    let head = node.head();
    let tail = node
        .continuation(0)
        .ok_or("circle has no continuation record")?;
    let radius = number(tail, 16).ok_or(NUMBER)?;
    if radius <= 0.0 {
        return Err("circle radius is not positive");
    }
    let mut fill_state = [0; 16];
    fill_state.copy_from_slice(&head[32..48]);
    Ok((
        BestEffortGeometry::Circle {
            center: point(head, 16).ok_or(NUMBER)?,
            radius,
            elevation: number(head, 48).ok_or(NUMBER)?,
            fill_state,
        },
        Vec::new(),
    ))
}

fn arc(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "arc has a non-finite field";
    let head = node.head();
    let tail = node
        .continuation(0)
        .ok_or("arc has no continuation record")?;
    let radius = number(tail, 16).ok_or(NUMBER)?;
    if radius <= 0.0 {
        return Err("arc radius is not positive");
    }
    Ok((
        BestEffortGeometry::Arc {
            center: point(head, 16).ok_or(NUMBER)?,
            radius,
            start_degrees: number(tail, 24).ok_or(NUMBER)?,
            end_degrees: number(tail, 32).ok_or(NUMBER)?,
            elevation: number(head, 48).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn ellipse(node: &Node<'_>, partial: bool) -> Decoded {
    const NUMBER: &str = "ellipse has a non-finite field";
    let head = node.head();
    let tail = node
        .continuation(0)
        .ok_or("ellipse has no continuation record")?;
    let center = point(head, 16).ok_or(NUMBER)?;
    let radius = point(head, 32).ok_or(NUMBER)?;
    if radius[0] <= 0.0 || radius[1] <= 0.0 {
        return Err("ellipse radius is not positive");
    }
    let rotation_degrees = number(tail, 16).ok_or(NUMBER)?;
    let geometry = if partial {
        BestEffortGeometry::EllipticalArc {
            center,
            radius,
            rotation_degrees,
            start_degrees: number(tail, 24).ok_or(NUMBER)?,
            end_degrees: number(tail, 32).ok_or(NUMBER)?,
        }
    } else {
        BestEffortGeometry::Ellipse {
            center,
            radius,
            rotation_degrees,
        }
    };
    Ok((geometry, Vec::new()))
}

fn insert(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "block reference has a non-finite field";
    let head = node.head();
    let tail = node
        .continuation(0)
        .ok_or("block reference has no continuation record")?;
    let name_field = &head[32..];
    let name_end = name_field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name_field.len());
    let (name, _) = text::shift_jis(name_field);
    Ok((
        BestEffortGeometry::Insert {
            name,
            name_bytes: name_field[..name_end].to_vec(),
            position: point(tail, 16).ok_or(NUMBER)?,
            elevation: number(tail, 32).ok_or(NUMBER)?,
            scale: point(tail, 40).ok_or(NUMBER)?,
            rotation_degrees: number(tail, 56).ok_or(NUMBER)?,
        },
        Vec::new(),
    ))
}

fn arrowhead(node: &Node<'_>) -> Decoded {
    const NUMBER: &str = "arrowhead has a non-finite field";
    let head = node.head();
    Ok((
        BestEffortGeometry::Arrowhead {
            position: point(head, 16).ok_or(NUMBER)?,
            size: point(head, 32).ok_or(NUMBER)?,
            angle_degrees: number(head, 48).ok_or(NUMBER)?,
            arrow_type: head[56],
            flags_raw: head[57],
        },
        Vec::new(),
    ))
}

/// Points of a polyline whose members are one connected run of 2D lines.
fn control_polygon(members: &[BestEffortEntity]) -> Option<Vec<[f64; 2]>> {
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

fn rows(bytes: &[u8], start: u64, count: usize) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    bytes.get(start..start.checked_add(count.checked_mul(RECORD)?)?)
}

fn layer(index: u16, row: &[u8]) -> BestEffortLayer {
    let (name, _) = text::shift_jis(&row[32..64]);
    let flags_raw = short(row, 0);
    BestEffortLayer {
        index,
        name,
        flags_raw,
        hidden: flags_raw & 0x0100 != 0,
        color: short(row, 4),
        line_type: short(row, 6),
        width: short(row, 8),
    }
}

fn color(index: u16, row: &[u8]) -> BestEffortColor {
    BestEffortColor {
        index,
        rgb: [row[4], row[5], row[6]],
        pen_width: f64::from(short(row, 2)) / 100.0,
    }
}

struct Tables {
    layers: Vec<BestEffortLayer>,
    colors: Vec<BestEffortColor>,
    line_types: Vec<BestEffortLineType>,
    widths: Vec<BestEffortWidth>,
}

fn tables(bytes: &[u8], header: &Header, tail: Option<u64>) -> Option<Tables> {
    let [layer_start, color_start, line_type_start, width_start] = header.tables?;
    let layer_rows = rows(bytes, layer_start, 255)?;
    let color_rows = rows(bytes, color_start, 255)?;
    let line_type_rows = rows(bytes, line_type_start, 32)?;
    let width_rows = rows(bytes, width_start, 16)?;
    // The tail extensions hold color 256 and the layers 0 and 256.
    let extension = |offset: u64| tail.and_then(|tail| rows(bytes, tail + offset, 1));
    let mut layers = Vec::with_capacity(257);
    if let Some(row) = extension(96) {
        layers.push(layer(0, row));
    }
    layers.extend(
        layer_rows
            .chunks_exact(RECORD)
            .zip(1u16..)
            .map(|(row, index)| layer(index, row)),
    );
    if let Some(row) = extension(408) {
        layers.push(layer(256, row));
    }
    let mut colors: Vec<_> = color_rows
        .chunks_exact(RECORD)
        .zip(1u16..)
        .map(|(row, index)| color(index, row))
        .collect();
    if let Some(row) = extension(16) {
        colors.push(color(256, row));
    }
    let line_types = line_type_rows
        .chunks_exact(RECORD)
        .zip(1u16..)
        .map(|(row, index)| {
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
    let widths = width_rows
        .chunks_exact(RECORD)
        .zip(1u16..)
        .map(|(row, index)| BestEffortWidth {
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

fn settings(bytes: &[u8]) -> Option<BestEffortSettings> {
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

fn table_records(bytes: &[u8], start: u64, count: u64) -> Result<&[u8]> {
    let size = count
        .checked_mul(RECORD_SIZE)
        .ok_or_else(|| invalid("record table extent overflow"))?;
    let end = start
        .checked_add(size)
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
    let root = table_records(bytes, header.record_start, header.record_count)?;
    let (root, terminal) = root.split_at(root.len() - RECORD);
    if terminal[0] != 0xff {
        return Err(invalid("record table has no terminal record"));
    }
    let mut reader = Reader {
        marker_raw: header.marker_raw,
        limits,
        report: BestEffortReport::default(),
        entities: 0,
        curve_points: 0,
    };
    if !header.marker_observed {
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
        0,
        root.len() / RECORD,
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
    if unbounded {
        reader.note(
            "block_table_end_not_found",
            "the block table did not end at the expected tail; later definitions and the special layer and color rows are unavailable",
            None,
            None,
        );
    }
    let mut blocks = Vec::with_capacity(ranges.len());
    for (index, range) in ranges.iter().enumerate() {
        let table = BestEffortTable::Block(index as u64);
        let records = table_records(bytes, range.record_start, range.record_count)?;
        let nodes = tree::parse(
            records,
            0,
            records.len() / RECORD,
            0,
            limits.max_depth,
            &mut issues,
        )?;
        let (name, _) = text::shift_jis(&range.name_bytes);
        blocks.push(BestEffortBlock {
            index: index as u64,
            name,
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
            "the layer, color, line-type and width tables do not have the known sizes",
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
        marker_observed: header.marker_observed,
        payload_size: bytes.len() as u64,
        payload_sha256: carrier.sha256.clone(),
        record_count: header.record_count,
        settings: settings(bytes),
        layers: tables.layers,
        colors: tables.colors,
        line_types: tables.line_types,
        widths: tables.widths,
        blocks,
        entities,
        report: reader.report,
    })
}
