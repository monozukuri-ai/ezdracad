//! Explicit root-only primitive decoding with exact supported forms.
//! Coordinates are stored XY, never universal WCS or physical millimeters.
use crate::preflight::invalid;
use crate::{DracadError, RawDocument, RawObjectSpan, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basic2DProfile {
    Marker25RootPrimitivesV1,
}
impl Basic2DProfile {
    pub const fn as_str(self) -> &'static str {
        "marker25_root_primitives_v1"
    }
    pub fn from_id(value: &str) -> Result<Self> {
        if value == "marker25_root_primitives_v1" {
            Ok(Self::Marker25RootPrimitivesV1)
        } else {
            Err(DracadError::UnsupportedFeature(
                "unknown root primitive profile".into(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Circle2D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub raw_bytes: [u8; 128],
    pub center: [f64; 2],
    pub radius: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Arc2D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub raw_bytes: [u8; 128],
    pub center: [f64; 2],
    pub radius: f64,
    pub start_angle_degrees: f64,
    pub end_angle_degrees: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Curve2D {
    Circle(Circle2D),
    Arc(Arc2D),
}
impl Curve2D {
    pub fn raw_bytes(&self) -> &[u8; 128] {
        match self {
            Self::Circle(c) => &c.raw_bytes,
            Self::Arc(a) => &a.raw_bytes,
        }
    }
    pub fn record_index(&self) -> u64 {
        match self {
            Self::Circle(c) => c.record_index,
            Self::Arc(a) => a.record_index,
        }
    }
    pub fn payload_offset(&self) -> u64 {
        match self {
            Self::Circle(c) => c.payload_offset,
            Self::Arc(a) => a.payload_offset,
        }
    }
    pub const fn coordinate_scope(&self) -> &'static str {
        "root_records"
    }
    pub const fn units(&self) -> Option<&'static str> {
        None
    }
}
fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}
fn number(bytes: &[u8], at: usize) -> Result<f64> {
    let value = f64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .and_then(|x| x.try_into().ok())
            .ok_or_else(|| invalid("truncated curve field"))?,
    );
    if !value.is_finite() {
        return Err(invalid(format!("non-finite curve field +{at}")));
    }
    Ok(value)
}

pub(crate) fn decode_curve_span(
    raw: &RawDocument,
    span: &RawObjectSpan,
    _profile: Basic2DProfile,
) -> Result<Curve2D> {
    if raw.header().profile != crate::ObservedProfile::Marker25 {
        return Err(unsupported("root curves require observed marker25"));
    }
    if !matches!(span.tag_raw, 3 | 4) || span.record_count != 2 {
        return Err(unsupported(
            "curve tag/flags/span outside root primitive profile",
        ));
    }
    let at = usize::try_from(span.payload_offset).map_err(|_| invalid("curve offset too large"))?;
    let bytes: [u8; 128] = raw
        .contents()
        .get(at..at + 128)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("truncated root curve"))?;
    if bytes[64..66] != 0xfeu16.to_le_bytes() {
        return Err(invalid("root curve missing FE continuation"));
    }
    let center = [number(&bytes, 16)?, number(&bytes, 24)?];
    let radius = number(&bytes, 80)?;
    let start = number(&bytes, 88)?;
    let end = number(&bytes, 96)?;
    let height = number(&bytes, 48)?;
    let thickness = number(&bytes, 56)?;
    if bytes[12..16] != [0, 1, 0, 0] {
        return Err(unsupported("unknown curve opaque tail"));
    }
    if bytes[32..48]
        .iter()
        .chain(&bytes[66..80])
        .chain(&bytes[104..])
        .any(|b| *b != 0)
        || height != 0.0
        || thickness != 0.0
    {
        return Err(unsupported(
            "filled/nonplanar/unknown curve state is not qualified",
        ));
    }
    if radius <= 0.0 {
        return Err(unsupported(
            "non-positive radius has no qualified native geometry",
        ));
    }
    if span.tag_raw == 3 {
        if start != 0.0 || end != 360.0 {
            return Err(unsupported(
                "circle angle fields outside observed full-circle form",
            ));
        }
        Ok(Curve2D::Circle(Circle2D {
            record_index: span.record_index,
            payload_offset: span.payload_offset,
            raw_bytes: bytes,
            center,
            radius,
        }))
    } else {
        if !(0.0..360.0).contains(&start) || !(0.0..360.0).contains(&end) || start == end {
            return Err(unsupported("arc angles outside nondegenerate degree form"));
        }
        Ok(Curve2D::Arc(Arc2D {
            record_index: span.record_index,
            payload_offset: span.payload_offset,
            raw_bytes: bytes,
            center,
            radius,
            start_angle_degrees: start,
            end_angle_degrees: end,
        }))
    }
}

pub fn root_curve_2d(raw: &RawDocument, index: u64, profile: Basic2DProfile) -> Result<Curve2D> {
    let mut selected = None;
    for span in raw.object_spans() {
        let span = span?;
        if span.record_index == index {
            selected = Some(span);
        }
    }
    let span =
        selected.ok_or_else(|| unsupported("curve selection must be a primary root object"))?;
    decode_curve_span(raw, &span, profile)
}

/// Native file-local explicit attribute indices; no visibility/print-width claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootPrimitiveStyle {
    pub attributes: crate::RecordAttributes,
    pub style_layer_index: u16,
    pub styles: crate::EffectiveStyleIndices,
}
