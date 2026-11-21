//! Best-effort reading: decode every recognized object and count the rest.
//!
//! The qualified readers stop at the first object whose state they cannot
//! establish. This module makes the opposite trade: it walks the whole record
//! table of any observed record generation, decodes the objects whose layout is
//! known, and reports everything else by tag. A result is a description of what
//! the file stores, not a statement that the drawing is complete, that the
//! coordinates have physical units, or that it matches the printed appearance.
//!
//! Malformed containers, inconsistent record tables and exhausted budgets are
//! still errors. A single object that does not fit its known layout is skipped
//! and counted; it does not hide the objects after it.

mod curve;
mod decode;
mod payload;
mod text;
mod tree;

use std::collections::BTreeMap;
use std::path::Path;

use crate::{DracadFormat, ParseLimits, Result};

pub const DEFAULT_MAX_BEST_EFFORT_ENTITIES: u64 = 2_000_000;
pub const DEFAULT_MAX_BEST_EFFORT_DEPTH: u64 = 32;
pub const DEFAULT_MAX_BEST_EFFORT_CURVE_POINTS: u64 = 4_000_000;
pub const DEFAULT_MAX_BEST_EFFORT_DIAGNOSTICS: u64 = 256;

/// Budgets of a best-effort read. Exceeding one is an error, never a silent cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BestEffortLimits {
    /// Decoded entities at every nesting level, block definitions included.
    pub max_entities: u64,
    /// Nesting of objects inside packages, polylines and other compounds.
    pub max_depth: u64,
    /// Points generated for spline polylines, summed over the drawing.
    pub max_curve_points: u64,
    pub max_block_definitions: u64,
    /// Retained diagnostics; further ones are only counted.
    pub max_diagnostics: u64,
}

impl Default for BestEffortLimits {
    fn default() -> Self {
        Self {
            max_entities: DEFAULT_MAX_BEST_EFFORT_ENTITIES,
            max_depth: DEFAULT_MAX_BEST_EFFORT_DEPTH,
            max_curve_points: DEFAULT_MAX_BEST_EFFORT_CURVE_POINTS,
            max_block_definitions: crate::DEFAULT_MAX_BLOCK_DEFINITIONS,
            max_diagnostics: DEFAULT_MAX_BEST_EFFORT_DIAGNOSTICS,
        }
    }
}

/// Record table an entity was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BestEffortTable {
    Root,
    /// Index into [`BestEffortDrawing::blocks`].
    Block(u64),
}

/// Location of the primary record of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BestEffortSource {
    pub table: BestEffortTable,
    /// Index of the primary record within its table.
    pub record_index: u64,
    /// Offset of the primary record in the drawing payload.
    pub payload_offset: u64,
    /// Records of the object, continuations and children included.
    pub record_count: u64,
}

/// File-local attribute references of an object header.
///
/// `color` uses 1..=256 for palette entries, 257 for the layer's color and 258
/// for the enclosing block's color. `line_type` uses 1..=32, 33 for the layer
/// and 34 for the block; `width` uses 1..=16, 17 for the layer and 18 for the
/// block. These are table indices, not rendered colors or printed widths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BestEffortAttributes {
    pub tag_raw: u16,
    /// High byte of the tag: 0x02 hidden, 0x08 polyline member, 0x10 reversed arc.
    pub flags_raw: u8,
    /// Layer number 0..=256.
    pub layer: u16,
    /// Nonzero for objects that belong to a layout rather than the drawing.
    pub segment: u8,
    pub color: u16,
    pub line_type: u16,
    pub width: u16,
    pub material: u8,
    pub group: u16,
    /// Shared by a dimension and the members of the package that draws it.
    pub link_id: u16,
}

impl BestEffortAttributes {
    #[must_use]
    pub const fn hidden(&self) -> bool {
        self.flags_raw & 0x02 != 0
    }
    #[must_use]
    pub const fn polyline_member(&self) -> bool {
        self.flags_raw & 0x08 != 0
    }
    #[must_use]
    pub const fn reversed_arc(&self) -> bool {
        self.flags_raw & 0x10 != 0
    }
}

/// Stored text with its saved size, placement and font fields.
#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortText {
    pub position: [f64; 2],
    pub elevation: f64,
    /// Decoded string: Shift_JIS bytes and saved Unicode escapes expanded,
    /// formatting codes removed. A line feed separates lines.
    pub text: String,
    /// Stored bytes without the terminator.
    pub text_bytes: Vec<u8>,
    /// Character cell height and width as saved, without physical units.
    pub height: f64,
    pub width: f64,
    pub spacing: f64,
    pub rotation_degrees: f64,
    pub slant_degrees: f64,
    /// Horizontal position in the low two bits (0 left, 1 center, 2 right),
    /// vertical position in the next two (0 bottom, 1 middle, 2 top).
    pub origin: u16,
    pub flags_raw: u16,
    pub font_name: String,
    pub charset: u8,
    pub family: u8,
    /// A numeric text holds a measured value next to its displayed string.
    pub numeric: bool,
    pub number: Option<f64>,
    pub unit_raw: u16,
    pub item_raw: u16,
    /// The string contained formatting codes; only their content was kept.
    pub format_codes: bool,
    /// Some bytes had no Shift_JIS meaning and were replaced.
    pub lossy: bool,
}

/// One family of parallel hatch lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BestEffortHatchLine {
    pub origin: [f64; 2],
    pub pitch: f64,
    pub angle_degrees: f64,
}

/// Geometry of one decoded object. Coordinates are the stored values.
#[derive(Debug, Clone, PartialEq)]
pub enum BestEffortGeometry {
    Point {
        position: [f64; 2],
        point_type: u32,
    },
    Line {
        start: [f64; 2],
        end: [f64; 2],
        elevation: f64,
        thickness: f64,
    },
    Line3D {
        start: [f64; 3],
        end: [f64; 3],
    },
    Circle {
        center: [f64; 2],
        radius: f64,
        elevation: f64,
        /// Bytes that hold fill state in filled circles; zero when unfilled.
        fill_state: [u8; 16],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start_degrees: f64,
        end_degrees: f64,
        elevation: f64,
    },
    Ellipse {
        center: [f64; 2],
        radius: [f64; 2],
        rotation_degrees: f64,
    },
    EllipticalArc {
        center: [f64; 2],
        radius: [f64; 2],
        rotation_degrees: f64,
        start_degrees: f64,
        end_degrees: f64,
    },
    Text(Box<BestEffortText>),
    /// A placement of a block definition, resolved by name.
    Insert {
        name: String,
        name_bytes: Vec<u8>,
        position: [f64; 2],
        elevation: f64,
        scale: [f64; 2],
        rotation_degrees: f64,
    },
    /// Members are the entity's children, in stored order.
    Polyline {
        /// 0 plain, 2 quadratic spline, 3 cubic spline, 4 Bezier.
        curve_kind: u8,
        /// 0x01 closed, 0x02 fill only, 0x04 filled, 0x08 custom fill,
        /// 0x10 spline through its points.
        flags_raw: u8,
        fill_color: u8,
        divisions: u8,
        three_d: bool,
        /// Evaluated curve of a cubic spline; the children are its control polygon.
        curve_points: Option<Vec<[f64; 2]>>,
    },
    /// A region bounded by its child polylines; a hatch refers to it by `link_id`.
    ExtendedPolygon {
        polygon_kind: u8,
        /// 0x01 closed, 0x02 fill only, 0x04 filled, 0x08 custom fill.
        flags_raw: u8,
        fill_color: u8,
        loop_count: u32,
        link_id: u32,
    },
    /// Hatching of the extended polygon with the same `link_id`.
    Hatch {
        /// 0 line families, 1 named pattern, 2 gradation.
        hatch_type: u16,
        link_id: u32,
        lines: Vec<BestEffortHatchLine>,
        /// Origin of a named pattern; the pattern itself is not decoded.
        pattern_origin: Option<[f64; 2]>,
    },
    /// The measured line and points. The numeric text is a child; the drawn
    /// lines, arrowheads and text are members of a package with `link_id`.
    Dimension {
        /// 1 aligned, 2 linear, 3 angle, 4 radius, 5 diameter.
        dimension_type: u8,
        flags_raw: u8,
        flags2_raw: u8,
        arrow_styles: [u8; 4],
        text_position: u8,
        line: Option<[[f64; 2]; 2]>,
        extension_origins: Vec<[f64; 2]>,
        /// Arrow sizes of both ends in drawing coordinates.
        arrow_sizes: Option<[f64; 4]>,
        /// Arrow sizes of both ends as they are output on paper, when saved.
        arrow_paper_sizes: Option<[f64; 4]>,
    },
    /// Members are the entity's children.
    Package {
        /// 0 plain, 1 reference, 2 property, 3 dimension, 4 leader, 5 extended polygon.
        package_kind: u8,
        /// 0x01 three-dimensional, 0x02 closed, 0x04 named.
        flags_raw: u8,
        reference_number: u32,
    },
    Arrowhead {
        position: [f64; 2],
        size: [f64; 2],
        angle_degrees: f64,
        /// 0 none, 1 origin mark, 2 and 3 dots, 4 to 7 arrows, 8 to 10 boxes,
        /// 11 slash, 12 integral sign.
        arrow_type: u8,
        flags_raw: u8,
    },
}

impl BestEffortGeometry {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Point { .. } => "POINT",
            Self::Line { .. } => "LINE",
            Self::Line3D { .. } => "LINE3D",
            Self::Circle { .. } => "CIRCLE",
            Self::Arc { .. } => "ARC",
            Self::Ellipse { .. } => "ELLIPSE",
            Self::EllipticalArc { .. } => "ELLIPTICAL_ARC",
            Self::Text(_) => "TEXT",
            Self::Insert { .. } => "INSERT",
            Self::Polyline { .. } => "POLYLINE",
            Self::ExtendedPolygon { .. } => "EXTENDED_POLYGON",
            Self::Hatch { .. } => "HATCH",
            Self::Dimension { .. } => "DIMENSION",
            Self::Package { .. } => "PACKAGE",
            Self::Arrowhead { .. } => "ARROWHEAD",
        }
    }
}

/// One decoded object with the objects it contains.
#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortEntity {
    pub attributes: BestEffortAttributes,
    pub source: BestEffortSource,
    pub geometry: BestEffortGeometry,
    pub children: Vec<BestEffortEntity>,
}

impl BestEffortEntity {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        self.geometry.kind()
    }
}

/// A named block definition and the objects of its own record table.
#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortBlock {
    pub index: u64,
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub header_offset: u64,
    pub entities: Vec<BestEffortEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestEffortLayer {
    pub index: u16,
    pub name: String,
    pub flags_raw: u16,
    pub hidden: bool,
    pub color: u16,
    pub line_type: u16,
    pub width: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortColor {
    pub index: u16,
    pub rgb: [u8; 3],
    /// Pen width saved with the color, in the stored hundredths as a value.
    pub pen_width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortLineType {
    pub index: u16,
    pub pen_style: u16,
    /// Stored dash values up to the last nonzero one; no pattern length is implied.
    pub pattern: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortWidth {
    pub index: u16,
    pub width: f64,
}

/// Saved drawing settings. They are not applied to any coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BestEffortSettings {
    pub drawing_origin: [f64; 2],
    /// Denominator of the saved paper scale (100 for 1:100).
    pub drawing_scale: f64,
    /// Paper width and height as saved.
    pub paper_size: [f64; 2],
    /// Top-left corner of the paper in stored coordinates.
    pub paper_origin: [f64; 2],
}

/// A note about one object or table. `tag_raw` is absent for table notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestEffortDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub table: Option<BestEffortTable>,
    pub record_index: Option<u64>,
    pub tag_raw: Option<u16>,
}

/// What was decoded and what was not. Counts cover every nesting level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BestEffortReport {
    pub decoded_counts: BTreeMap<&'static str, u64>,
    /// Objects of a kind this reader does not decode, by the low byte of the tag.
    pub unsupported_counts: BTreeMap<u8, u64>,
    /// Objects of a known kind whose records did not fit the known layout.
    pub failed_counts: BTreeMap<u8, u64>,
    /// Non-graphic property records, which carry no geometry.
    pub property_records: u64,
    /// Continuation records that belonged to no object.
    pub orphan_records: u64,
    /// Objects whose declared length ran past their parent and was cut there.
    pub clamped_spans: u64,
    pub diagnostics: Vec<BestEffortDiagnostic>,
    pub diagnostics_suppressed: u64,
}

impl BestEffortReport {
    /// Whether any object was skipped, cut or left undecoded.
    #[must_use]
    pub fn lossy(&self) -> bool {
        !self.unsupported_counts.is_empty()
            || !self.failed_counts.is_empty()
            || self.orphan_records != 0
            || self.clamped_spans != 0
    }
}

/// A best-effort view of one drawing payload.
#[derive(Debug, Clone, PartialEq)]
pub struct BestEffortDrawing {
    pub format: DracadFormat,
    /// Raw generation field of the payload header, not an application version.
    pub marker_raw: u16,
    /// Whether this generation is one the reader has been checked against.
    pub marker_observed: bool,
    pub payload_size: u64,
    pub payload_sha256: String,
    pub record_count: u64,
    pub settings: Option<BestEffortSettings>,
    pub layers: Vec<BestEffortLayer>,
    pub colors: Vec<BestEffortColor>,
    pub line_types: Vec<BestEffortLineType>,
    pub widths: Vec<BestEffortWidth>,
    pub blocks: Vec<BestEffortBlock>,
    pub entities: Vec<BestEffortEntity>,
    pub report: BestEffortReport,
}

/// Read every recognized object of an MPZ, flat MPX or unprotected MPS input.
pub fn read_best_effort(
    data: &[u8],
    parse_limits: &ParseLimits,
    limits: &BestEffortLimits,
) -> Result<BestEffortDrawing> {
    let carrier = payload::open(data, parse_limits)?;
    decode::drawing(&carrier, parse_limits, limits)
}

/// Read a regular file with the same size limits as [`crate::inspect_file`].
pub fn read_best_effort_file(
    path: &Path,
    parse_limits: &ParseLimits,
    limits: &BestEffortLimits,
) -> Result<BestEffortDrawing> {
    let data = crate::inspect::read_file_limited(path, parse_limits)?;
    read_best_effort(&data, parse_limits, limits)
}

#[cfg(test)]
mod tests;
