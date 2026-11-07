//! Observed header metadata, without applying drawing-coordinate transforms.

use crate::preflight::invalid;
use crate::Result;

/// Drawing origin and inverse paper-scale fields in observed Contents headers.
///
/// These are metadata values, not an instruction to transform geometry. Units,
/// normalized coordinates and the intervening header fields remain unresolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawingSettings {
    /// Contents-relative start of the retained header range.
    pub payload_offset: u64,
    /// Exact bytes from Contents +0x140 through +0x17f, including unknown gaps.
    pub header_bytes: [u8; 64],
    pub drawing_origin: [f64; 2],
    /// The stored positive factor at +0x178, equal to 1 / drawing_scale.
    pub paper_scale_factor: f64,
    /// Reciprocal of paper_scale_factor; not a physical-unit conversion.
    pub drawing_scale: f64,
}

fn finite_field(bytes: &[u8; 64], offset: usize) -> Result<f64> {
    let raw = bytes
        .get(offset..offset + 8)
        .and_then(|field| field.try_into().ok())
        .ok_or_else(|| invalid("truncated drawing-setting field"))?;
    let value = f64::from_le_bytes(raw);
    if !value.is_finite() {
        return Err(invalid(format!(
            "non-finite drawing-setting field at Contents +0x{:x}",
            0x140 + offset
        )));
    }
    Ok(value)
}

pub(crate) fn decode(contents: &[u8]) -> Result<DrawingSettings> {
    let header_bytes = contents
        .get(0x140..0x180)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated drawing-settings header range"))?;
    let drawing_origin = [
        finite_field(&header_bytes, 0)?,
        finite_field(&header_bytes, 8)?,
    ];
    let paper_scale_factor = finite_field(&header_bytes, 56)?;
    if paper_scale_factor <= 0.0 {
        return Err(invalid("drawing paper-scale factor must be positive"));
    }
    let drawing_scale = 1.0 / paper_scale_factor;
    if !drawing_scale.is_finite() {
        return Err(invalid("non-finite reciprocal drawing scale"));
    }
    Ok(DrawingSettings {
        payload_offset: 0x140,
        header_bytes,
        drawing_origin,
        paper_scale_factor,
        drawing_scale,
    })
}
