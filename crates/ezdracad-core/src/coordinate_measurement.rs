//! Explicit lengths in the observed paper/current-annotation measurement context.
//!
//! This is not a world-space or entity physical-unit declaration. It does not
//! normalize points, apply the drawing origin, infer object creation scales or
//! evaluate dimension-figure text.

use crate::preflight::invalid;
use crate::{ObservedProfile, Result};

/// The two retained scale fields used by the tested native measurement context.
///
/// Raw line coordinates remain unchanged. Paper lengths describe the drawing's
/// configured output scale, not printer overrides. Current-annotation lengths
/// describe the active DRA-CAD measurement convention, not an immutable design
/// length intrinsic to every entity. Source offsets are relative to Contents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateMeasurementContext {
    pub paper_scale_offset: u64,
    pub paper_scale_bytes: [u8; 8],
    pub paper_scale_factor: f64,
    pub annotation_ratio_offset: u64,
    pub annotation_ratio_bytes: [u8; 8],
    /// Stored drawing denominator / current annotation denominator.
    pub annotation_ratio: f64,
    pub drawing_denominator: f64,
    pub annotation_denominator: f64,
}

fn positive(value: f64, label: &str) -> Result<f64> {
    if !value.is_finite() || value <= 0.0 {
        return Err(invalid(format!(
            "coordinate measurement {label} must be finite and positive"
        )));
    }
    Ok(value)
}

fn denominators(paper_scale_factor: f64, annotation_ratio: f64) -> Result<(f64, f64)> {
    positive(paper_scale_factor, "paper-scale factor")?;
    positive(annotation_ratio, "annotation ratio")?;
    let drawing = positive(1.0 / paper_scale_factor, "drawing denominator")?;
    let annotation_inverse = positive(
        paper_scale_factor * annotation_ratio,
        "inverse annotation denominator",
    )?;
    let annotation = positive(1.0 / annotation_inverse, "annotation denominator")?;
    Ok((drawing, annotation))
}

fn converted_length(raw_length: f64, result: f64) -> Result<f64> {
    if !raw_length.is_finite() || raw_length < 0.0 {
        return Err(invalid(
            "coordinate measurement raw length must be finite and nonnegative",
        ));
    }
    if !result.is_finite() || result < 0.0 {
        return Err(invalid(
            "non-finite coordinate measurement converted length",
        ));
    }
    if raw_length > 0.0 && result == 0.0 {
        return Err(invalid("coordinate measurement nonzero length underflow"));
    }
    Ok(result)
}

impl CoordinateMeasurementContext {
    /// Apply the configured drawing scale, excluding printer scale/overrides.
    pub fn length_on_paper_mm(&self, raw_length: f64) -> Result<f64> {
        denominators(self.paper_scale_factor, self.annotation_ratio)?;
        converted_length(raw_length, raw_length * self.paper_scale_factor)
    }

    /// Express a raw length in the current annotation measurement convention.
    ///
    /// Changing the active annotation scale changes this result even when the
    /// line's stored coordinates are unchanged. No creation scale is inferred.
    pub fn length_at_current_annotation_mm(&self, raw_length: f64) -> Result<f64> {
        denominators(self.paper_scale_factor, self.annotation_ratio)?;
        converted_length(raw_length, raw_length / self.annotation_ratio)
    }
}

pub(crate) fn decode(
    contents: &[u8],
    profile: ObservedProfile,
) -> Result<CoordinateMeasurementContext> {
    match profile {
        ObservedProfile::Marker5 | ObservedProfile::Marker8 | ObservedProfile::Marker25 => {}
    }
    let paper_scale_bytes = contents
        .get(0x178..0x180)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated coordinate measurement paper-scale field"))?;
    let annotation_ratio_bytes = contents
        .get(0x278..0x280)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated coordinate measurement annotation-ratio field"))?;
    let paper_scale_factor = f64::from_le_bytes(paper_scale_bytes);
    let annotation_ratio = f64::from_le_bytes(annotation_ratio_bytes);
    let (drawing_denominator, annotation_denominator) =
        denominators(paper_scale_factor, annotation_ratio)?;
    Ok(CoordinateMeasurementContext {
        paper_scale_offset: 0x178,
        paper_scale_bytes,
        paper_scale_factor,
        annotation_ratio_offset: 0x278,
        annotation_ratio_bytes,
        annotation_ratio,
        drawing_denominator,
        annotation_denominator,
    })
}
