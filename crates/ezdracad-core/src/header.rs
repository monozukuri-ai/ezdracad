//! Observed payload/flat-carrier layouts, not every DRA-CAD generation.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{DracadError, ParseLimits, Result};

pub const RECORD_SIZE: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedProfile {
    Marker5,
    Marker8,
    Marker25,
}

impl ObservedProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Marker5 => "observed_marker5_stride64",
            Self::Marker8 => "observed_marker8_stride64",
            Self::Marker25 => "observed_marker25_stride64",
        }
    }
}

/// Source of the validated drawing-payload end, without changing original bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadEndBasis {
    DeclaredField,
    FileLength,
}
impl PayloadEndBasis {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeclaredField => "declared_field",
            Self::FileLength => "file_length",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadHeader {
    pub profile: ObservedProfile,
    pub marker_raw: u16,
    pub record_start: u64,
    pub record_end: u64,
    pub declared_nonterminal_records: u32,
    pub record_count: u64,
    pub payload_end: u64,
    pub payload_end_field_raw: u32,
    pub payload_end_basis: PayloadEndBasis,
    /// Raw boundaries from +0x28..+0x40; their table semantics remain unresolved.
    pub section_offsets: [u64; 7],
    /// +0x44, +0x48, +0x4c; zero means absent in the observed layouts.
    pub optional_offsets: [u64; 3],
}

fn word(bytes: &[u8], offset: usize) -> u64 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]) as u64
}

pub(crate) fn parse_header(bytes: &[u8], limits: &ParseLimits) -> Result<PayloadHeader> {
    parse_header_with_basis(bytes, limits, PayloadEndBasis::DeclaredField)
}

pub(crate) fn parse_mpx_header(bytes: &[u8], limits: &ParseLimits) -> Result<PayloadHeader> {
    parse_header_with_basis(bytes, limits, PayloadEndBasis::FileLength)
}

fn parse_header_with_basis(
    bytes: &[u8],
    limits: &ParseLimits,
    basis: PayloadEndBasis,
) -> Result<PayloadHeader> {
    if bytes.len() < 0x5c
        || !bytes.starts_with(b"DRACAD for Windows")
        || bytes[18..32].iter().any(|byte| *byte != 0)
    {
        return Err(invalid("Contents lacks a complete supported header"));
    }
    let marker_raw = u16::from_le_bytes([bytes[34], bytes[35]]);
    let profile = match marker_raw {
        5 => ObservedProfile::Marker5,
        8 => ObservedProfile::Marker8,
        25 => ObservedProfile::Marker25,
        _ => {
            return Err(DracadError::UnsupportedFormat(format!(
                "unqualified raw record marker {marker_raw}"
            )))
        }
    };
    if basis == PayloadEndBasis::FileLength && profile != ObservedProfile::Marker25 {
        return Err(DracadError::UnsupportedFormat(
            "flat MPX requires the observed marker25 carrier profile".into(),
        ));
    }
    let start = word(bytes, 0x24);
    let end = word(bytes, 0x28);
    if start < 0x5c || start > end || end > bytes.len() as u64 {
        return Err(invalid("record boundaries are outside Contents"));
    }
    if start != 8192 || bytes[32..34] != [0, 0] {
        return Err(DracadError::UnsupportedFeature(
            "unqualified record origin/header flags".into(),
        ));
    }
    let count = word(bytes, 0x58);
    let record_count = count
        .checked_add(1)
        .ok_or_else(|| invalid("record count overflow"))?;
    check_limit("records", record_count, limits.max_records)?;
    let extent = record_count
        .checked_mul(RECORD_SIZE)
        .ok_or_else(|| invalid("record extent overflow"))?;
    if end - start != extent {
        return Err(invalid("record count/stride/end relation is inconsistent"));
    }
    let raw_end = word(bytes, 0x50);
    let payload_end = match basis {
        PayloadEndBasis::DeclaredField => {
            if raw_end.checked_add(4) != Some(bytes.len() as u64) {
                return Err(invalid("declared payload end differs from Contents length"));
            }
            raw_end
        }
        PayloadEndBasis::FileLength => {
            if raw_end != 0 {
                return Err(DracadError::UnsupportedFormat("flat input has a Contents end pointer, not the observed native MPX envelope; use scan_contents for extracted payloads".into()));
            }
            bytes.len() as u64
        }
    };
    if end > payload_end {
        return Err(invalid("record table exceeds drawing payload end"));
    }
    let mut section_offsets = [0; 7];
    for (index, value) in section_offsets.iter_mut().enumerate() {
        *value = word(bytes, 0x28 + index * 4);
        if *value < end || *value > payload_end {
            return Err(invalid("post-record boundary outside Contents"));
        }
    }
    let deltas = [
        16320,
        16320,
        2048,
        1024,
        match profile {
            ObservedProfile::Marker5 => 1656,
            ObservedProfile::Marker8 | ObservedProfile::Marker25 => 3312,
        },
        4096,
    ];
    for (pair, delta) in section_offsets.windows(2).zip(deltas) {
        if pair[1] < pair[0] {
            return Err(invalid("post-record boundaries are not monotonic"));
        }
        if pair[1] - pair[0] != delta {
            return Err(DracadError::UnsupportedFeature(
                "unqualified post-record table layout".into(),
            ));
        }
    }
    let optional_offsets = [word(bytes, 0x44), word(bytes, 0x48), word(bytes, 0x4c)];
    for value in optional_offsets {
        if value != 0 && !(section_offsets[6]..=payload_end).contains(&value) {
            return Err(invalid(
                "optional tail offset is outside its bounded region",
            ));
        }
    }
    Ok(PayloadHeader {
        profile,
        marker_raw,
        record_start: start,
        record_end: end,
        declared_nonterminal_records: count as u32,
        record_count,
        payload_end,
        payload_end_field_raw: raw_end as u32,
        payload_end_basis: basis,
        section_offsets,
        optional_offsets,
    })
}
