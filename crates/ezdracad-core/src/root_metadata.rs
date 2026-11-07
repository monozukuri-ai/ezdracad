//! Exact supported marker25 auxiliary-property forms.
//!
//! This is deliberately not a generic object-property decoder. A recognized
//! property may be passed when selecting stored root geometry under the basic 2D
//! profile; it does not establish drawing completeness, future-edit behavior,
//! visibility, units, or that arbitrary properties cannot affect geometry.

use crate::preflight::invalid;
use crate::{ObservedProfile, RawDocument, RawObjectSpan, Result};

const IDENTITY: &[u8] =
    br#"{\uni"56f3"}{\uni"9762"}{\uni"306e"}{\uni"8b58"}{\uni"5225"}{\uni"756a"}{\uni"53f7"}"#;
const HISTORY: &[u8] = br#"{\uni"56f3"}{\uni"9762"}{\uni"306e"}{\uni"8b58"}{\uni"5225"}{\uni"756a"}{\uni"53f7"}{\uni"5c65"}{\uni"6b74"}"#;
const REFERENCE_HISTORY: &[u8] = br#"{\uni"53c2"}{\uni"7167"}{\uni"56f3"}{\uni"9762"}{\uni"306e"}{\uni"8b58"}{\uni"5225"}{\uni"756a"}{\uni"53f7"}{\uni"5c65"}{\uni"6b74"}"#;
const ANALYSIS: &[u8] =
    br#"{\uni"56f3"}{\uni"9762"}{\uni"306e"}{\uni"89e3"}{\uni"6790"}{\uni"60c5"}{\uni"5831"}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootMetadataKind {
    CurrentCategoryEmpty,
    DrawingIdentity,
    DrawingIdentityHistory,
    ReferenceIdentityHistoryEmpty,
    DrawingAnalysisEmpty,
}

impl RootMetadataKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CurrentCategoryEmpty => "current_category_empty",
            Self::DrawingIdentity => "drawing_identity",
            Self::DrawingIdentityHistory => "drawing_identity_history",
            Self::ReferenceIdentityHistoryEmpty => "reference_identity_history_empty",
            Self::DrawingAnalysisEmpty => "drawing_analysis_empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootMetadata {
    pub record_index: u64,
    pub payload_offset: u64,
    pub raw_bytes: Vec<u8>,
    pub kind: RootMetadataKind,
    pub property_name: String,
    pub property_value: String,
}

fn uuid(bytes: &[u8]) -> bool {
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(at, value)| {
            if matches!(at, 8 | 13 | 18 | 23) {
                *value == b'-'
            } else {
                value.is_ascii_digit() || (b'A'..=b'F').contains(value)
            }
        })
}

fn nul_field(bytes: &[u8]) -> Option<&[u8]> {
    let end = bytes.iter().position(|byte| *byte == 0)?;
    bytes[end..]
        .iter()
        .all(|byte| *byte == 0)
        .then_some(&bytes[..end])
}

fn classify_bytes(bytes: &[u8]) -> Option<RootMetadata> {
    let count = bytes.len() / 64;
    if bytes.len() % 64 != 0 || !(2..=6).contains(&count) {
        return None;
    }
    let expected: [u8; 16] = [0x33, 0, 0, 0, 0, 0, 0, 0, count as u8, 0, 0, 0, 0, 0, 0, 0];
    if bytes[..16] != expected
        || bytes[16..22]
            .iter()
            .chain(&bytes[24..32])
            .any(|byte| *byte != 0)
    {
        return None;
    }
    let size = usize::from(u16::from_le_bytes([bytes[22], bytes[23]]));
    let owner = nul_field(&bytes[32..64])?;
    // All continuation headers are checked, including reserved attribute bytes.
    let mut body = Vec::with_capacity((count - 1) * 48);
    for slot in bytes[64..].chunks_exact(64) {
        if slot[..2] != [0xfe, 0] || slot[2..16].iter().any(|byte| *byte != 0) {
            return None;
        }
        body.extend_from_slice(&slot[16..]);
    }
    let end_name = body.iter().position(|byte| *byte == 0)?;
    let name = &body[..end_name];
    let value_start = end_name + 1;
    let value_end = value_start.checked_add(size)?;
    let value = body.get(value_start..value_end)?;
    // A full NUL and all saved padding must remain zero. Extra all-zero slots
    // are also outside the exact observed form rather than silently discarded.
    if body.get(value_end..)?.is_empty()
        || body[value_end..].iter().any(|byte| *byte != 0)
        || 1 + (value_end + 1).div_ceil(48) != count
    {
        return None;
    }
    let kind = if owner == b"DRACAD_SYSTEM_ADDING_MASTER" {
        if name != b"CUR_CATEGORY" || !value.is_empty() {
            return None;
        }
        RootMetadataKind::CurrentCategoryEmpty
    } else if owner == b"KOZO_SYSTEM_PLAN" {
        if name == IDENTITY {
            if value.len() != 42
                || &value[..5] != b"1.00\t"
                || !uuid(&value[5..41])
                || value[41] != b'\t'
            {
                return None;
            }
            RootMetadataKind::DrawingIdentity
        } else if name == HISTORY {
            if !matches!(value.len(), 74 | 111)
                || !value
                    .chunks_exact(37)
                    .all(|token| uuid(&token[..36]) && token[36] == b'\t')
            {
                return None;
            }
            RootMetadataKind::DrawingIdentityHistory
        } else if name == REFERENCE_HISTORY && value.is_empty() {
            RootMetadataKind::ReferenceIdentityHistoryEmpty
        } else if name == ANALYSIS && matches!(value, b"1.00\t\t" | b"1.00\t0\t") {
            RootMetadataKind::DrawingAnalysisEmpty
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(RootMetadata {
        record_index: 0,
        payload_offset: 0,
        raw_bytes: bytes.to_vec(),
        kind,
        property_name: String::from_utf8(name.to_vec()).ok()?,
        property_value: String::from_utf8(value.to_vec()).ok()?,
    })
}

/// Recognize only the exact auxiliary-property forms supported by the basic 2D profile
/// controls. `None` means the normal unknown-state fence still applies. The
/// caller supplies a structurally validated primary root span; interior records
/// must never be passed as root properties.
pub(crate) fn classify_root_metadata(
    raw: &RawDocument,
    span: &RawObjectSpan,
) -> Result<Option<RootMetadata>> {
    if raw.header().profile != ObservedProfile::Marker25
        || span.tag_raw != 0x33
        || !(2..=6).contains(&span.record_count)
    {
        return Ok(None);
    }
    let at =
        usize::try_from(span.payload_offset).map_err(|_| invalid("metadata offset too large"))?;
    let end = at
        .checked_add(span.record_count as usize * 64)
        .ok_or_else(|| invalid("metadata end overflow"))?;
    let bytes = raw
        .contents()
        .get(at..end)
        .ok_or_else(|| invalid("truncated root metadata"))?;
    let mut result = classify_bytes(bytes);
    if let Some(value) = result.as_mut() {
        value.record_index = span.record_index;
        value.payload_offset = span.payload_offset;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Authored hostile-input vectors. These are not native evidence.
    fn fixture(owner: &[u8], name: &[u8], value: &[u8]) -> Vec<u8> {
        let count = 1 + (name.len() + 1 + value.len() + 1).div_ceil(48);
        let mut bytes = vec![0; count * 64];
        bytes[0] = 0x33;
        bytes[8] = count as u8;
        bytes[22..24].copy_from_slice(&(value.len() as u16).to_le_bytes());
        bytes[32..32 + owner.len()].copy_from_slice(owner);
        let body: Vec<u8> = name
            .iter()
            .copied()
            .chain([0])
            .chain(value.iter().copied())
            .chain([0])
            .collect();
        for offset in (64..bytes.len()).step_by(64) {
            bytes[offset] = 0xfe;
        }
        for (at, byte) in body.iter().enumerate() {
            bytes[80 + at / 48 * 64 + at % 48] = *byte;
        }
        bytes
    }

    #[test]
    fn exact_auxiliary_forms_retain_every_original_byte() {
        let id = b"01234567-89AB-CDEF-0123-456789ABCDEF";
        let identity: Vec<_> = b"1.00\t".iter().chain(id).chain(b"\t").copied().collect();
        let history: Vec<_> = id
            .iter()
            .chain(b"\t")
            .chain(id)
            .chain(b"\t")
            .copied()
            .collect();
        for (owner, name, value, kind) in [
            (
                &b"DRACAD_SYSTEM_ADDING_MASTER"[..],
                &b"CUR_CATEGORY"[..],
                &b""[..],
                RootMetadataKind::CurrentCategoryEmpty,
            ),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                IDENTITY,
                &identity,
                RootMetadataKind::DrawingIdentity,
            ),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                HISTORY,
                &history,
                RootMetadataKind::DrawingIdentityHistory,
            ),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                REFERENCE_HISTORY,
                &b""[..],
                RootMetadataKind::ReferenceIdentityHistoryEmpty,
            ),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                ANALYSIS,
                &b"1.00\t\t"[..],
                RootMetadataKind::DrawingAnalysisEmpty,
            ),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                ANALYSIS,
                &b"1.00\t0\t"[..],
                RootMetadataKind::DrawingAnalysisEmpty,
            ),
        ] {
            let bytes = fixture(owner, name, value);
            let metadata = classify_bytes(&bytes).unwrap();
            assert_eq!(metadata.kind, kind);
            assert_eq!(metadata.raw_bytes, bytes);
            assert_eq!(metadata.property_name.as_bytes(), name);
            assert_eq!(metadata.property_value.as_bytes(), value);
        }
    }

    #[test]
    fn future_owner_property_state_flags_and_padding_remain_unknown() {
        for (owner, name, value) in [
            (&b"UNKNOWN"[..], &b"CUR_CATEGORY"[..], &b""[..]),
            (
                &b"DRACAD_SYSTEM_ADDING_MASTER"[..],
                &b"CUR_CATEGORY"[..],
                &b"1"[..],
            ),
            (&b"KOZO_SYSTEM_PLAN"[..], ANALYSIS, &b"1.00\t1\t"[..]),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                REFERENCE_HISTORY,
                &b"REFERENCE"[..],
            ),
            (&b"KOZO_SYSTEM_PLAN"[..], &b"unknown_name"[..], &b""[..]),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                IDENTITY,
                &b"1.00\t01234567-89ab-cdef-0123-456789abcdef\t"[..],
            ),
            (
                &b"KOZO_SYSTEM_PLAN"[..],
                HISTORY,
                &b"01234567-89AB-CDEF-0123-456789ABCDEF\t"[..],
            ),
        ] {
            assert!(classify_bytes(&fixture(owner, name, value)).is_none());
        }
        let bytes = fixture(b"KOZO_SYSTEM_PLAN", ANALYSIS, b"1.00\t\t");
        for offset in [
            1,
            2,
            6,
            10,
            12,
            16,
            20,
            22,
            24,
            32,
            63,
            64,
            66,
            78,
            80,
            bytes.len() - 1,
        ] {
            let mut mutated = bytes.clone();
            mutated[offset] ^= 1;
            assert!(classify_bytes(&mutated).is_none(), "offset {offset}");
        }
        let mut extra = bytes.clone();
        extra.extend_from_slice(&[0; 64]);
        extra[8] += 1;
        extra[bytes.len()] = 0xfe;
        assert!(classify_bytes(&extra).is_none());
        for end in 0..bytes.len() {
            assert!(classify_bytes(&bytes[..end]).is_none());
        }
    }
}
