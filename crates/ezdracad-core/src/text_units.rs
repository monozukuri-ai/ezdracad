//! Observed text-size display/input selector, not a geometric unit declaration.

use crate::preflight::invalid;
use crate::{DracadError, ObservedProfile, Result};

/// Font-size output units observed in the marker-25 drawing settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSizeUnit {
    Mm,
    Pt,
}

impl TextSizeUnit {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Pt => "pt",
        }
    }
}

/// The exact two-byte selector at Contents +0x9ac, decoded on request.
///
/// This setting controls text size, not stored geometry, physical drawing
/// units, or the separate MPXL Origin.Units value. It applies no conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSizeUnitInfo {
    pub payload_offset: u64,
    pub selector_bytes: [u8; 2],
    pub raw_selector: u16,
    pub unit: TextSizeUnit,
}

pub(crate) fn decode(contents: &[u8], profile: ObservedProfile) -> Result<TextSizeUnitInfo> {
    if profile != ObservedProfile::Marker25 {
        return Err(DracadError::UnsupportedFeature(
            "text-size unit selector is qualified only for observed marker 25".into(),
        ));
    }
    let selector_bytes = contents
        .get(0x9ac..0x9ae)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated text-size unit selector"))?;
    let raw_selector = u16::from_le_bytes(selector_bytes);
    let unit = match raw_selector {
        0 => TextSizeUnit::Mm,
        1 => TextSizeUnit::Pt,
        _ => {
            return Err(DracadError::UnsupportedFeature(format!(
                "unqualified text-size unit selector {raw_selector}"
            )))
        }
    };
    Ok(TextSizeUnitInfo {
        payload_offset: 0x9ac,
        selector_bytes,
        raw_selector,
        unit,
    })
}
