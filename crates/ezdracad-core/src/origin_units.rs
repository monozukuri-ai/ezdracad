//! Source-preserving MPXL Origin.Units code, without a physical-unit enum.

use crate::preflight::invalid;
use crate::{DracadError, ObservedProfile, RawDocument, Result};

/// The raw signed integer associated with MPXL `Header/Origin@Units`.
///
/// Native read/export controls associate these four Contents bytes with that
/// XML attribute for observed marker 25. Code meanings and physical-unit names
/// remain unresolved. Every bit pattern is retained, including unknown and
/// negative codes. Reading this metadata applies no coordinate transformation
/// and does not identify an entity's creation scale or physical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginUnitsCode {
    /// Contents-relative offset of the four-byte code.
    pub payload_offset: u64,
    pub selector_bytes: [u8; 4],
    pub raw_code: i32,
}

fn decode(contents: &[u8], profile: ObservedProfile) -> Result<OriginUnitsCode> {
    if profile != ObservedProfile::Marker25 {
        return Err(DracadError::UnsupportedFeature(
            "Origin.Units code is qualified only for observed marker 25".into(),
        ));
    }
    let selector_bytes = contents
        .get(0x150..0x154)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated Origin.Units code"))?;
    Ok(OriginUnitsCode {
        payload_offset: 0x150,
        selector_bytes,
        raw_code: i32::from_le_bytes(selector_bytes),
    })
}

impl RawDocument {
    /// Read the raw marker-25 Origin.Units code from this owned snapshot.
    ///
    /// This does not decode a physical-unit enum or convert geometry. Other
    /// profiles fail explicitly; unknown code values remain available as raw
    /// signed integers and exact source bytes.
    pub fn origin_units_code(&self) -> Result<OriginUnitsCode> {
        decode(self.contents(), self.header().profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_code_never_returns_partial_metadata() {
        for length in [0, 0x14f, 0x150, 0x151, 0x152, 0x153] {
            assert!(matches!(
                decode(&vec![0; length], ObservedProfile::Marker25),
                Err(DracadError::InvalidFormat(_))
            ));
        }
    }
}
