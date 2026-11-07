//! Explicit bounded placement of directly contained 3D block lines.
//!
//! This does not broaden the existing planar block/scene APIs. Only an admitted
//! primary root reference and a wholly supported direct-line definition return
//! a selection. Units, effective styles, general WCS and completeness remain
//! unresolved. Controlled native placements establish scale then Rx, Ry, Rz,
//! then translation for the admitted fields.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{BlockDefinition, DracadError, LineRecord3D, ObservedProfile, RawDocument, Result};

pub const DEFAULT_MAX_BLOCK_LINES_3D: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block3DProfile {
    Marker25DirectLine3dV1,
}

impl Block3DProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "marker25_direct_line3d_v1"
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "marker25_direct_line3d_v1" => Ok(Self::Marker25DirectLine3dV1),
            _ => Err(unsupported("unknown 3D block profile")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockReferenceEvidence3D {
    pub contents_sha256: String,
    pub record_indices: [u64; 3],
    pub payload_offsets: [u64; 3],
    pub records: [[u8; 64]; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockReference3D {
    profile: Block3DProfile,
    name_field_bytes: [u8; 32],
    translation: [f64; 3],
    scale: [f64; 3],
    rotation_degrees: [f64; 3],
    evidence: BlockReferenceEvidence3D,
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn finite(value: f64, label: &str) -> Result<f64> {
    if !value.is_finite() {
        return Err(invalid(format!("non-finite 3D block {label}")));
    }
    Ok(value)
}

fn product(a: f64, b: f64) -> Result<f64> {
    let value = finite(a * b, "product")?;
    if a != 0. && b != 0. && value == 0. {
        return Err(invalid("3D block nonzero product underflow"));
    }
    Ok(value)
}

fn floats<const N: usize>(data: &[u8], start: usize) -> Result<[f64; N]> {
    let mut result = [0.; N];
    for (i, value) in result.iter_mut().enumerate() {
        let at = start + i * 8;
        let bytes = data
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| invalid("truncated 3D block numeric field"))?;
        *value = finite(f64::from_le_bytes(bytes), "numeric field")?;
    }
    Ok(result)
}

impl BlockReference3D {
    #[must_use]
    pub const fn profile(&self) -> Block3DProfile {
        self.profile
    }

    #[must_use]
    pub const fn name_field_bytes(&self) -> &[u8; 32] {
        &self.name_field_bytes
    }

    #[must_use]
    pub fn name_key(&self) -> &[u8] {
        let end = self
            .name_field_bytes
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(32);
        &self.name_field_bytes[..end]
    }

    #[must_use]
    pub const fn translation(&self) -> [f64; 3] {
        self.translation
    }

    #[must_use]
    pub const fn scale(&self) -> [f64; 3] {
        self.scale
    }

    /// Saved right-handed X/Y/Z rotation angles, in degrees.
    #[must_use]
    pub const fn rotation_degrees(&self) -> [f64; 3] {
        self.rotation_degrees
    }

    #[must_use]
    pub const fn evidence(&self) -> &BlockReferenceEvidence3D {
        &self.evidence
    }

    #[must_use]
    pub const fn units(&self) -> Option<&'static str> {
        None
    }

    /// Apply positive XYZ scale, then right-handed Rx, Ry, Rz, then translation.
    /// Bare supplied points have no native entity provenance.
    pub fn apply_xyz(&self, point: [f64; 3]) -> Result<[f64; 3]> {
        let mut displacement = [0.; 3];
        for i in 0..3 {
            finite(point[i], "input point")?;
            displacement[i] = product(self.scale[i], point[i])?;
        }
        for (axis, angle) in self.rotation_degrees.into_iter().enumerate() {
            let reduced = angle % 360.;
            if reduced == 0. {
                continue;
            }
            let (s, c) = match reduced {
                90. | -270. => (1., 0.),
                -90. | 270. => (-1., 0.),
                180. | -180. => (0., -1.),
                _ => {
                    let radians = reduced.to_radians();
                    if radians == 0. {
                        return Err(invalid("3D block nonzero rotation angle underflow"));
                    }
                    radians.sin_cos()
                }
            };
            let (a, b) = match axis {
                0 => (1, 2),
                1 => (2, 0),
                _ => (0, 1),
            };
            let left = finite(
                product(c, displacement[a])? - product(s, displacement[b])?,
                "rotated point",
            )?;
            let right = finite(
                product(s, displacement[a])? + product(c, displacement[b])?,
                "rotated point",
            )?;
            displacement[a] = left;
            displacement[b] = right;
        }
        let mut result = [0.; 3];
        for i in 0..3 {
            result[i] = finite(self.translation[i] + displacement[i], "translated point")?;
            if displacement[i] != 0. && result[i] == self.translation[i] {
                return Err(invalid("3D block displacement lost in translation"));
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedBlockLine3D {
    pub definition_index: u64,
    pub source: LineRecord3D,
    pub start: [f64; 3],
    pub end: [f64; 3],
}

impl PlacedBlockLine3D {
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "block_parent_stored_xyz"
    }

    #[must_use]
    pub const fn units(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockLineSelection3D {
    profile: Block3DProfile,
    contents_sha256: String,
    reference: BlockReference3D,
    definition: BlockDefinition,
    saved_bounds: [f64; 6],
    lines: Vec<PlacedBlockLine3D>,
}

impl BlockLineSelection3D {
    #[must_use]
    pub const fn profile(&self) -> Block3DProfile {
        self.profile
    }

    #[must_use]
    pub fn contents_sha256(&self) -> &str {
        &self.contents_sha256
    }

    #[must_use]
    pub const fn reference(&self) -> &BlockReference3D {
        &self.reference
    }

    #[must_use]
    pub const fn definition(&self) -> &BlockDefinition {
        &self.definition
    }

    #[must_use]
    pub const fn definition_flag_raw(&self) -> u32 {
        1
    }

    /// Retained six saved bounds components; this is not a recomputed box.
    #[must_use]
    pub const fn saved_bounds(&self) -> [f64; 6] {
        self.saved_bounds
    }

    #[must_use]
    pub fn lines(&self) -> &[PlacedBlockLine3D] {
        &self.lines
    }

    #[must_use]
    pub const fn document_complete(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn units(&self) -> Option<&'static str> {
        None
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Block3DProfile,
        String,
        BlockReference3D,
        BlockDefinition,
        Vec<PlacedBlockLine3D>,
    ) {
        (
            self.profile,
            self.contents_sha256,
            self.reference,
            self.definition,
            self.lines,
        )
    }
}

fn reference(
    document: &RawDocument,
    index: u64,
    profile: Block3DProfile,
) -> Result<BlockReference3D> {
    if index >= document.header().record_count.saturating_sub(1) {
        return Err(invalid(
            "3D block reference index exceeds nonterminal records",
        ));
    }
    let mut primary = None;
    for item in document.object_spans() {
        let span = item?;
        if span.record_index == index {
            primary = Some(span);
            break;
        }
        if span.record_index > index {
            break;
        }
    }
    let span =
        primary.ok_or_else(|| unsupported("3D block selection requires a primary root span"))?;
    if !matches!(span.tag_raw, 6 | 0x406) || span.record_count != 3 {
        return Err(unsupported(
            "3D block profile requires exact tag6/0406 and three slots",
        ));
    }
    let mut records = [[0u8; 64]; 3];
    let mut offsets = [0u64; 3];
    for i in 0..3 {
        let slot = document
            .record(index + i as u64)
            .ok_or_else(|| invalid("missing 3D reference slot"))?;
        records[i].copy_from_slice(slot.bytes());
        offsets[i] = slot.payload_offset;
    }
    let mut prefix = [0u8; 16];
    prefix[1] = 1;
    if records[0][16..32] != prefix || records[0][12..16] != [0, 1, 0, 0] {
        return Err(unsupported("unqualified 3D block primary source fields"));
    }
    let mut continuation = [0u8; 16];
    continuation[0] = 0xfe;
    if records[1][..16] != continuation || records[2][..16] != continuation {
        return Err(unsupported("unqualified 3D block continuation headers"));
    }
    if records[2][40..64].iter().any(|b| *b != 0) {
        return Err(unsupported(
            "unqualified 3D block reserved continuation fields",
        ));
    }
    let name_field_bytes: [u8; 32] = records[0][32..64].try_into().unwrap();
    let end = name_field_bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| invalid("3D block name lacks a bounded NUL terminator"))?;
    if end == 0 {
        return Err(invalid("3D block name is empty"));
    }
    let first = floats::<6>(&records[1], 16)?;
    let second = floats::<3>(&records[2], 16)?;
    let scale = [first[3], first[4], second[2]];
    if scale.iter().any(|s| *s <= 0.) {
        return Err(unsupported(
            "3D block placement requires positive XYZ scale",
        ));
    }
    let rotation_degrees = [second[0], second[1], first[5]];
    Ok(BlockReference3D {
        profile,
        name_field_bytes,
        translation: [first[0], first[1], first[2]],
        scale,
        rotation_degrees,
        evidence: BlockReferenceEvidence3D {
            contents_sha256: document.sha256().into(),
            record_indices: [index, index + 1, index + 2],
            payload_offsets: offsets,
            records,
        },
    })
}

/// Select and place a supported entire direct-line definition atomically.
/// This function does not expand inline packages, nested references or a scene.
pub fn placed_block_lines_3d(
    document: &RawDocument,
    index: u64,
    profile: Block3DProfile,
    max_lines: u64,
) -> Result<BlockLineSelection3D> {
    if document.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported("3D block profile requires observed marker25"));
    }
    let reference = reference(document, index, profile)?;
    let library = document.block_library()?;
    let definition = library.resolve(reference.name_key())?.clone();
    let h = &definition.header_bytes;
    if h[32..36].iter().any(|b| *b != 0)
        || h[40..44] != 1u32.to_le_bytes()
        || h[44..112].iter().chain(&h[160..176]).any(|b| *b != 0)
    {
        return Err(unsupported(
            "unqualified 3D block definition flags or coordinate fields",
        ));
    }
    let saved_bounds = floats::<6>(h, 112)?;
    let mut lines = Vec::new();
    let mut cursor = 0u64;
    while cursor + 1 < definition.record_count {
        let record = library
            .record(definition.index, cursor)
            .ok_or_else(|| invalid("missing 3D block definition record"))?;
        let bytes = record.bytes();
        let span = u64::from(u16::from_le_bytes(bytes[8..10].try_into().unwrap()));
        if span == 0 {
            return Err(invalid("zero 3D block definition object span"));
        }
        let end = cursor
            .checked_add(span)
            .filter(|end| *end < definition.record_count)
            .ok_or_else(|| invalid("3D block object span crosses terminal"))?;
        match record.tag {
            0x13 if span == 1 => {
                if bytes[12..16] != [0; 4] {
                    return Err(unsupported("unqualified direct 3D line source tail"));
                }
                let values = floats::<6>(bytes, 16)?;
                let source = LineRecord3D {
                    record_index: cursor,
                    payload_offset: record.payload_offset,
                    tag_raw: record.tag,
                    header_bytes: bytes[..16].try_into().unwrap(),
                    start: [values[0], values[1], values[2]],
                    end: [values[3], values[4], values[5]],
                };
                let start = reference.apply_xyz(source.start)?;
                let end = reference.apply_xyz(source.end)?;
                check_limit("block_lines_3d", lines.len() as u64 + 1, max_lines)?;
                lines.push(PlacedBlockLine3D {
                    definition_index: definition.index,
                    source,
                    start,
                    end,
                });
            }
            0x33 => {
                for i in cursor + 1..end {
                    if library.record(definition.index, i).map(|r| r.tag) != Some(0xfe) {
                        return Err(unsupported("unqualified 3D block metadata continuation"));
                    }
                }
            }
            _ => {
                return Err(unsupported(
                    "3D block definition contains an unsupported object or span",
                ))
            }
        }
        cursor = end;
    }
    if cursor + 1 != definition.record_count || lines.is_empty() {
        return Err(unsupported(
            "3D block definition has no complete supported direct lines",
        ));
    }
    Ok(BlockLineSelection3D {
        profile,
        contents_sha256: document.sha256().into(),
        reference,
        definition,
        saved_bounds,
        lines,
    })
}

impl RawDocument {
    pub fn placed_block_lines_3d(
        &self,
        index: u64,
        profile: Block3DProfile,
        max_lines: u64,
    ) -> Result<BlockLineSelection3D> {
        placed_block_lines_3d(self, index, profile, max_lines)
    }
}
