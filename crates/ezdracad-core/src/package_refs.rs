//! Exact two-slot package-reference payloads; no logical object membership.

use crate::preflight::invalid;
use crate::{DracadError, RawRecord, Result, RECORD_SIZE};

/// Fields observed in an exact root-slot tag 0x0006 followed by 0x00fe.
///
/// Root-slot enumeration does not establish logical root-object membership,
/// units, a resolved parent scope or drawing completeness. Names are opaque
/// internal byte keys, which need not equal names exported to DXF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackageReference2D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub continuation_index: u64,
    pub continuation_offset: u64,
    pub tag_raw: u16,
    pub header_bytes: [u8; 16],
    /// Entire fixed-width name field, including any bytes after the first NUL.
    pub name_field_bytes: [u8; 32],
    pub anchor_raw: [f64; 2],
    pub continuation_header_bytes: [u8; 16],
    /// Exact six floating fields at continuation +16, +24, +32, +40, +48, +56.
    pub placement_raw: [f64; 6],
}

impl PackageReference2D {
    /// The validated first-NUL prefix; no character encoding is assumed.
    #[must_use]
    pub fn name_key(&self) -> &[u8] {
        let end = self
            .name_field_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.name_field_bytes.len());
        &self.name_field_bytes[..end]
    }

    /// Destination of the explicit placement calculation, not a WCS claim.
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "package_parent"
    }

    /// Apply only the verified zero-anchor, positive uniform-scale XY subset.
    /// Units, parent scope and any additional drawing transform stay unresolved.
    pub fn apply_xy(&self, point: [f64; 2]) -> Result<[f64; 2]> {
        Self::apply_xy_fields(self.anchor_raw, self.placement_raw, point)
    }

    /// The same bounded calculation for thin bindings with owned field values.
    /// This does not identify an object or qualify its drawing context.
    pub fn apply_xy_fields(
        anchor_raw: [f64; 2],
        placement_raw: [f64; 6],
        point: [f64; 2],
    ) -> Result<[f64; 2]> {
        if !anchor_raw
            .into_iter()
            .chain(placement_raw)
            .chain(point)
            .all(f64::is_finite)
        {
            return Err(invalid("non-finite package placement input"));
        }
        if anchor_raw != [0.0, 0.0] || placement_raw[2] != 0.0 {
            return Err(DracadError::UnsupportedFeature(
                "unqualified package anchor or third continuation field".into(),
            ));
        }
        let [tx, ty, _, sx, sy, degrees] = placement_raw;
        if sx <= 0.0 || sx != sy {
            return Err(DracadError::UnsupportedFeature(
                "package placement requires a positive uniform scale".into(),
            ));
        }
        let (sin, cos) = degrees.to_radians().sin_cos();
        let [x, y] = point;
        let output = [tx + sx * (cos * x - sin * y), ty + sx * (sin * x + cos * y)];
        if !output.into_iter().all(f64::is_finite) {
            return Err(invalid("non-finite package placement result"));
        }
        Ok(output)
    }
}

fn field<const N: usize>(bytes: &[u8], start: usize) -> Result<[u8; N]> {
    bytes
        .get(start..start + N)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| invalid("truncated package-reference field"))
}

fn floats<const N: usize>(bytes: &[u8]) -> Result<[f64; N]> {
    let mut values = [0.0; N];
    for (index, value) in values.iter_mut().enumerate() {
        *value = f64::from_le_bytes(field(bytes, 16 + index * 8)?);
        if !value.is_finite() {
            return Err(invalid("non-finite package-reference field"));
        }
    }
    Ok(values)
}

pub(crate) fn decode_reference(
    primary: RawRecord<'_>,
    continuation: Option<RawRecord<'_>>,
    record_count: u64,
) -> Result<Option<PackageReference2D>> {
    if primary.tag != 0x0006 {
        return Ok(None);
    }
    let bytes = primary.bytes();
    if u16::from_le_bytes(field(bytes, 8)?) != 2 {
        return Err(DracadError::UnsupportedFeature(
            "unqualified package-reference slot span".into(),
        ));
    }
    let next_index = primary
        .index
        .checked_add(1)
        .ok_or_else(|| invalid("package continuation index overflow"))?;
    if next_index >= record_count.saturating_sub(1) {
        return Err(invalid("package continuation crosses the terminal slot"));
    }
    let next = continuation.ok_or_else(|| invalid("missing package continuation"))?;
    let next_offset = primary
        .payload_offset
        .checked_add(RECORD_SIZE)
        .ok_or_else(|| invalid("package continuation offset overflow"))?;
    if next.index != next_index || next.payload_offset != next_offset || next.tag != 0x00fe {
        return Err(invalid(
            "package continuation is not the following 0x00fe slot",
        ));
    }
    let name_field_bytes: [u8; 32] = field(bytes, 32)?;
    let name_end = name_field_bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| invalid("package name lacks a bounded NUL terminator"))?;
    if name_end == 0 {
        return Err(invalid("empty package name key"));
    }
    Ok(Some(PackageReference2D {
        record_index: primary.index,
        payload_offset: primary.payload_offset,
        continuation_index: next.index,
        continuation_offset: next.payload_offset,
        tag_raw: primary.tag,
        header_bytes: field(bytes, 0)?,
        name_field_bytes,
        anchor_raw: floats(bytes)?,
        continuation_header_bytes: field(next.bytes(), 0)?,
        placement_raw: floats(next.bytes())?,
    }))
}
