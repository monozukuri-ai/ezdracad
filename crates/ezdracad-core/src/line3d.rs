//! Observed 3D line fields without coordinate-system or parent interpretation.

use crate::preflight::invalid;
use crate::{DracadError, ObservedProfile, RawDocument, RawRecord, Result};

/// One exact observed tag-0x0013 record, retaining independent endpoint Z.
///
/// These are record-local stored coordinates. This source does not establish
/// root membership, physical units, WCS, workplane state or parent placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineRecord3D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub tag_raw: u16,
    pub header_bytes: [u8; 16],
    pub start: [f64; 3],
    pub end: [f64; 3],
}

impl LineRecord3D {
    /// No drawing-origin, input-unit or parent transform has been applied.
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "record_local"
    }
}

fn decode(record: RawRecord<'_>) -> Result<Option<LineRecord3D>> {
    if record.tag != 0x0013 {
        return Ok(None);
    }
    let mut values = [0.0; 6];
    for (value, offset) in values.iter_mut().zip([16, 24, 32, 40, 48, 56]) {
        let raw = record
            .bytes()
            .get(offset..offset + 8)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| invalid("truncated 3D line field"))?;
        *value = f64::from_le_bytes(raw);
        if !value.is_finite() {
            return Err(invalid(format!(
                "non-finite 3D line field +{offset} in record {} at Contents offset {}",
                record.index, record.payload_offset
            )));
        }
    }
    let header_bytes = record
        .bytes()
        .get(..16)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated 3D line header"))?;
    Ok(Some(LineRecord3D {
        record_index: record.index,
        payload_offset: record.payload_offset,
        tag_raw: record.tag,
        header_bytes,
        start: [values[0], values[1], values[2]],
        end: [values[3], values[4], values[5]],
    }))
}

impl RawDocument {
    fn ensure_line3d_profile(&self) -> Result<()> {
        if !matches!(
            self.header().profile,
            ObservedProfile::Marker5 | ObservedProfile::Marker25
        ) {
            return Err(DracadError::UnsupportedFeature(
                "3D line record decoding requires observed marker5 or marker25".into(),
            ));
        }
        Ok(())
    }

    /// Read an exact tag-0x0013 raw slot in an independently observed file profile.
    ///
    /// Unknown tags and absent indices return `None`; nonfinite recognized fields
    /// fail on demand. Profile support is checked even for an absent index.
    pub fn line_record_3d(&self, index: u64) -> Result<Option<LineRecord3D>> {
        self.ensure_line3d_profile()?;
        self.record(index)
            .map(decode)
            .transpose()
            .map(Option::flatten)
    }

    /// Preserve raw table order and duplicates, without asserting logical roots.
    ///
    /// File-profile admission is eager. Consuming every fallible iterator item
    /// checks all recognized 3D lines, while raw scan admission remains unchanged.
    pub fn line_records_3d(&self) -> Result<impl Iterator<Item = Result<LineRecord3D>> + '_> {
        self.ensure_line3d_profile()?;
        Ok(self
            .records()
            .filter_map(|record| decode(record).transpose()))
    }
}
