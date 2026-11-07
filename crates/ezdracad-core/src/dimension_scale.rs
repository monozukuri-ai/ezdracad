//! Recorded numeric dimension ratios, not an entity's original D and A.

use crate::preflight::invalid;
use crate::{DracadError, ObservedProfile, RawDocument, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionScaleProfile {
    Marker25AlignedNumericMmV1,
}

impl DimensionScaleProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Marker25AlignedNumericMmV1 => "marker25_aligned_numeric_mm_v1",
        }
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "marker25_aligned_numeric_mm_v1" => Ok(Self::Marker25AlignedNumericMmV1),
            _ => Err(unsupported("unknown explicit profile")),
        }
    }
}

/// One admitted root-table dimension's owned, record-local evidence.
///
/// The ratio is derived from its saved numeric value and its own stored span.
/// It does not identify original D/A separately, apply parent placements, give
/// neighboring geometry physical units, or certify a complete drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionScale {
    pub profile: DimensionScaleProfile,
    pub record_index: u64,
    pub payload_offset: u64,
    pub tag_raw: u16,
    pub contents_sha256: String,
    pub record_bytes: Vec<u8>,
    pub dimid: u8,
    pub dimflag: u8,
    pub dimflag2: u8,
    pub geometry_offset: u64,
    pub geometry_bytes: [u8; 48],
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub stored_span_length: f64,
    pub numeric_flag_offset: u64,
    pub numeric_flag_bytes: [u8; 4],
    pub unit_offset: u64,
    pub unit_bytes: [u8; 4],
    pub number_offset: u64,
    pub number_bytes: [u8; 8],
    pub recorded_value_mm: f64,
    pub annotation_to_drawing_ratio: f64,
    pub current_drawing_scale_offset: u64,
    pub current_drawing_scale_bytes: [u8; 8],
    pub current_drawing_denominator: f64,
    pub equivalent_annotation_denominator: f64,
}

impl DimensionScale {
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "record_local"
    }

    #[must_use]
    pub const fn ratio_provenance(&self) -> &'static str {
        "derived_saved_dimension_number_and_geometry"
    }

    #[must_use]
    pub const fn geometry_units(&self) -> Option<&'static str> {
        None
    }

    #[must_use]
    pub const fn original_scale_resolution(&self) -> &'static str {
        "ratio_only_original_denominators_unavailable"
    }

    #[must_use]
    pub const fn document_complete(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn original_drawing_denominator(&self) -> Option<f64> {
        None
    }

    #[must_use]
    pub const fn original_annotation_denominator(&self) -> Option<f64> {
        None
    }
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(format!("dimension scale: {message}"))
}

fn finite(value: f64) -> Result<f64> {
    if !value.is_finite() {
        return Err(invalid(
            "dimension scale contains a nonfinite numeric field",
        ));
    }
    Ok(value)
}

fn derived_positive(value: f64) -> Result<f64> {
    if !value.is_finite() || value <= 0.0 {
        return Err(invalid(
            "dimension scale arithmetic overflow, nonzero underflow or unrepresentable ratio",
        ));
    }
    Ok(value)
}

fn scaled_planar_length(dx: f64, dy: f64) -> Result<f64> {
    let ax = finite(dx)?.abs();
    let ay = finite(dy)?.abs();
    let maximum = ax.max(ay);
    if maximum == 0.0 {
        return Ok(0.0);
    }
    // Square only a normalized value in [0,1]. Squaring the coordinates would
    // overflow for large finite spans and underflow for subnormal spans.
    // An extremely small normalized component may round to zero; the norm
    // still retains the nonzero maximum component. sqrt lowers without a
    // platform hypot symbol, preserving the existing Linux wheel baseline.
    let ratio = ax.min(ay) / maximum;
    finite(maximum * (1.0 + ratio * ratio).sqrt())
}

impl RawDocument {
    /// Read only the explicit profile's root-table, associated mm dimension.
    pub fn dimension_scale(
        &self,
        index: u64,
        profile: DimensionScaleProfile,
    ) -> Result<DimensionScale> {
        if self.header().profile != ObservedProfile::Marker25 {
            return Err(unsupported("unqualified source marker"));
        }
        let mut selected = None;
        for span in self.object_spans() {
            let span = span?;
            if span.record_index == index {
                selected = Some(span);
                break;
            }
            if span.record_index > index {
                break;
            }
        }
        let span = selected.ok_or_else(|| unsupported("selection is not a root primary span"))?;
        if !matches!(span.tag_raw, 0x1a | 0x41a) || span.record_count != 10 {
            return Err(unsupported("unqualified dimension tag or span"));
        }
        let mut bytes = Vec::with_capacity(640);
        for (slot, tag) in [
            span.tag_raw,
            0x41,
            0x43,
            0x43,
            0x44,
            0x14,
            0xfe,
            0xfe,
            0xfe,
            0x50,
        ]
        .into_iter()
        .enumerate()
        {
            let record = self
                .record(index + slot as u64)
                .ok_or_else(|| invalid("dimension scale truncated record span"))?;
            if record.tag != tag {
                return Err(unsupported("unqualified dimension child sequence"));
            }
            bytes.extend_from_slice(record.bytes());
        }
        if bytes[16..19] != [1, 128, 2] {
            return Err(unsupported(
                "unqualified dimension kind or association flags",
            ));
        }
        if u16::from_le_bytes(bytes[5 * 64 + 8..5 * 64 + 10].try_into().unwrap()) != 4 {
            return Err(unsupported("unqualified embedded text span"));
        }
        let numeric_flag_offset = 6 * 64 + 12;
        let numeric_flag_bytes: [u8; 4] = bytes[numeric_flag_offset..numeric_flag_offset + 4]
            .try_into()
            .unwrap();
        if u32::from_le_bytes(numeric_flag_bytes) != 1 {
            return Err(unsupported("text is not an admitted numeric dimension"));
        }
        let unit_offset = 7 * 64 + 2;
        let unit_bytes: [u8; 4] = bytes[unit_offset..unit_offset + 4].try_into().unwrap();
        if unit_bytes != [19, 0, 1, 0] {
            return Err(unsupported("unqualified numeric dimension unit or item"));
        }
        let geometry_offset = 64 + 16;
        let geometry_bytes: [u8; 48] = bytes[geometry_offset..geometry_offset + 48]
            .try_into()
            .unwrap();
        let mut fields = [0.0; 6];
        for (value, raw) in fields.iter_mut().zip(geometry_bytes.chunks_exact(8)) {
            *value = finite(f64::from_le_bytes(raw.try_into().unwrap()))?;
        }
        if fields[4] != 0.0 || fields[5] != 0.0 {
            return Err(unsupported("nonplanar dimension geometry"));
        }
        let dx = finite(fields[2] - fields[0])?;
        let dy = finite(fields[3] - fields[1])?;
        let length = scaled_planar_length(dx, dy)?;
        if length <= 0.0 {
            return Err(unsupported("zero dimension span cannot identify a ratio"));
        }
        let number_offset = 8 * 64 + 8;
        let number_bytes: [u8; 8] = bytes[number_offset..number_offset + 8].try_into().unwrap();
        let number = finite(f64::from_le_bytes(number_bytes))?;
        if number <= 0.0 {
            return Err(unsupported(
                "nonpositive dimension value cannot identify a ratio",
            ));
        }
        let ratio = derived_positive(number / length)?;
        // The current annotation setting is deliberately not used here.
        let drawing = self.drawing_settings()?.drawing_scale;
        let equivalent = derived_positive(drawing * ratio)?;
        Ok(DimensionScale {
            profile,
            record_index: index,
            payload_offset: span.payload_offset,
            tag_raw: span.tag_raw,
            contents_sha256: self.sha256().into(),
            dimid: bytes[16],
            dimflag: bytes[17],
            dimflag2: bytes[18],
            geometry_offset: span.payload_offset + geometry_offset as u64,
            geometry_bytes,
            start: [fields[0], fields[1]],
            end: [fields[2], fields[3]],
            stored_span_length: length,
            numeric_flag_offset: span.payload_offset + numeric_flag_offset as u64,
            numeric_flag_bytes,
            unit_offset: span.payload_offset + unit_offset as u64,
            unit_bytes,
            number_offset: span.payload_offset + number_offset as u64,
            number_bytes,
            recorded_value_mm: number,
            annotation_to_drawing_ratio: ratio,
            current_drawing_scale_offset: 0x178,
            current_drawing_scale_bytes: self.contents()[0x178..0x180].try_into().unwrap(),
            current_drawing_denominator: drawing,
            equivalent_annotation_denominator: equivalent,
            record_bytes: bytes,
        })
    }
}
