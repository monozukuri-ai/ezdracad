//! Observed record-header references, separate from effective/rendered styles.

use crate::preflight::invalid;
use crate::{DracadError, LineRecord2D, ObservedProfile, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeReferenceKind {
    Index,
    ByLayer,
    ByBlock,
}

impl AttributeReferenceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::ByLayer => "by_layer",
            Self::ByBlock => "by_block",
        }
    }
}

/// File-local index or unresolved inheritance selector, not a DXF value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributeReference {
    pub kind: AttributeReferenceKind,
    pub index: Option<u16>,
    pub raw_code: u8,
}

impl AttributeReference {
    fn index(raw_code: u8, index: u16) -> Self {
        Self {
            kind: AttributeReferenceKind::Index,
            index: Some(index),
            raw_code,
        }
    }

    fn dependency(raw_code: u8, kind: AttributeReferenceKind) -> Self {
        Self {
            kind,
            index: None,
            raw_code,
        }
    }
}

/// Exact observed header fields; flags/material/tail keep their raw values.
/// Layer 0, ByLayer and ByBlock do not imply a resolved effective style here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAttributes {
    pub tag_raw: u16,
    pub header_bytes: [u8; 16],
    pub layer_index: u16,
    pub color: AttributeReference,
    pub line_type: AttributeReference,
    pub width: AttributeReference,
    pub material_index_raw: u8,
    pub group_index: u16,
    pub flags_raw: u8,
    pub opaque_tail_bytes: [u8; 4],
}

impl RecordAttributes {
    #[must_use]
    pub fn has_dependencies(&self) -> bool {
        [self.color, self.line_type, self.width]
            .iter()
            .any(|reference| reference.kind != AttributeReferenceKind::Index)
    }

    pub(crate) fn ensure_profile(&self, profile: ObservedProfile) -> Result<()> {
        if profile != ObservedProfile::Marker25 && self.has_dependencies() {
            return Err(DracadError::UnsupportedFeature(
                "attribute dependency selectors are unqualified in this observed profile".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributedLineRecord2D {
    pub source: LineRecord2D,
    pub attributes: RecordAttributes,
}

fn indexed_reference(raw_code: u8, maximum: u8, field: &str) -> Result<AttributeReference> {
    match raw_code {
        0 => Ok(AttributeReference::dependency(
            raw_code,
            AttributeReferenceKind::ByLayer,
        )),
        255 => Ok(AttributeReference::dependency(
            raw_code,
            AttributeReferenceKind::ByBlock,
        )),
        code if code <= maximum => Ok(AttributeReference::index(code, u16::from(code))),
        _ => Err(DracadError::UnsupportedFeature(format!(
            "unqualified {field} reference code {raw_code}"
        ))),
    }
}

/// Decode an explicit 16-byte observed current-format header.
///
/// This helper has no surrounding profile. Document APIs separately reject
/// dependency selectors in marker-5/8 files, whose old-format save conversion
/// has not qualified those sentinel encodings. No flags are generally masked.
pub fn decode_record_attributes(header: &[u8]) -> Result<RecordAttributes> {
    let bytes: [u8; 16] = header
        .try_into()
        .map_err(|_| invalid("record attribute header must be exactly 16 bytes"))?;
    let tag_raw = u16::from_le_bytes([bytes[0], bytes[1]]);
    if !matches!(tag_raw, 2 | 0x402 | 0x2402 | 0x4402 | 0x30 | 0x430 | 6) {
        return Err(DracadError::UnsupportedFeature(format!(
            "unqualified attribute header tag/flags 0x{tag_raw:04x}"
        )));
    }
    let layer_index = u16::from_le_bytes([bytes[2], bytes[3]]);
    if layer_index > 256 {
        return Err(DracadError::UnsupportedFeature(format!(
            "unqualified layer index {layer_index}"
        )));
    }
    let color = match tag_raw {
        0x2402 | 0x4402 => {
            if bytes[4] != 0 {
                return Err(DracadError::UnsupportedFeature(
                    "color dependency flags require the observed zero color code".into(),
                ));
            }
            AttributeReference::dependency(
                0,
                if tag_raw == 0x2402 {
                    AttributeReferenceKind::ByLayer
                } else {
                    AttributeReferenceKind::ByBlock
                },
            )
        }
        _ => AttributeReference::index(
            bytes[4],
            if bytes[4] == 0 {
                256
            } else {
                u16::from(bytes[4])
            },
        ),
    };
    Ok(RecordAttributes {
        tag_raw,
        header_bytes: bytes,
        layer_index,
        color,
        line_type: indexed_reference(bytes[5], 32, "line-type")?,
        width: indexed_reference(bytes[6], 16, "width")?,
        material_index_raw: bytes[7],
        group_index: u16::from_le_bytes([bytes[10], bytes[11]]),
        flags_raw: bytes[1],
        opaque_tail_bytes: bytes[12..16]
            .try_into()
            .map_err(|_| invalid("truncated attribute tail"))?,
    })
}
