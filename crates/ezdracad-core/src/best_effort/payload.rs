//! Carrier and header of a best-effort read.
//!
//! The record table relations are still validated; only the generation marker
//! and the size of the tail after the tables are left open.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::tree::Layout;
use crate::format::DRAWING_MAGIC;
use crate::limits::check_limit;
use crate::preflight::{invalid, CFB_MAGIC};
use crate::{DracadError, DracadFormat, ParseLimits, Result};

/// First extension header of the tail that follows the block definitions.
const TAIL_SIGNATURE: [u8; 16] = [2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0];

pub(crate) struct Carrier {
    pub format: DracadFormat,
    pub payload: Arc<[u8]>,
    pub sha256: String,
}

/// Rows of one style table: `first` is the index the first row stands for.
#[derive(Clone, Copy)]
pub(crate) struct TableSpec {
    pub start: u64,
    pub rows: usize,
    pub row: usize,
    pub first: u16,
}

pub(crate) struct TableSpecs {
    pub layers: TableSpec,
    pub colors: TableSpec,
    pub line_types: TableSpec,
    pub widths: TableSpec,
    /// Offset and length of the name field of a layer row.
    pub layer_name: (usize, usize),
    /// The tables of the first releases: fewer rows, no layer widths, and
    /// leftover bytes wherever a row has no field.
    pub early: bool,
}

pub(crate) struct Header {
    pub marker_raw: u16,
    pub layout: Layout,
    pub record_start: u64,
    /// Records of the root table, terminal included.
    pub record_count: u64,
    pub payload_end: u64,
    /// Style tables, when their sizes match a known layout.
    pub tables: Option<TableSpecs>,
    pub tail_start: u64,
    pub block_start: u64,
}

pub(crate) struct BlockRange {
    pub header_offset: u64,
    pub name_bytes: Vec<u8>,
    pub record_start: u64,
    /// Records of the definition table, terminal excluded.
    pub record_count: u64,
}

/// Identify the carrier and retain its drawing payload.
pub(crate) fn open(data: &[u8], limits: &ParseLimits) -> Result<Carrier> {
    limits.check_file_size(data.len() as u64)?;
    if !data.starts_with(CFB_MAGIC) && data.starts_with(DRAWING_MAGIC) {
        check_limit("stream_size", data.len() as u64, limits.max_stream_size)?;
        return Ok(Carrier {
            format: DracadFormat::Mpx,
            payload: Arc::from(data),
            sha256: format!("{:x}", Sha256::digest(data)),
        });
    }
    let info = crate::inspect::inspect_with(data, limits, true)?;
    match info.format {
        DracadFormat::Mpz | DracadFormat::Mps | DracadFormat::Mpuw => {
            let payload = info.contents_arc()?;
            let sha256 = info
                .payload
                .as_ref()
                .map(|payload| payload.sha256.clone())
                .ok_or_else(|| invalid("drawing payload metadata is absent"))?;
            Ok(Carrier {
                format: info.format,
                payload,
                sha256,
            })
        }
        DracadFormat::MpsCandidate => Err(DracadError::UnsupportedFormat(
            "protected or unrecognized MPS or MPUW encoding; the drawing payload cannot be decoded"
                .into(),
        )),
        DracadFormat::Mpx | DracadFormat::UnknownCfb => Err(DracadError::UnsupportedFormat(
            "compound file is not a DRA-CAD drawing container".into(),
        )),
    }
}

pub(crate) fn word(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u64::from(u32::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3],
    ])))
}

fn table_specs(sections: &[u64; 7], layout: Layout) -> Option<TableSpecs> {
    let sizes: Vec<u64> = sections[..5]
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    let spec = |index: usize, rows: usize, row: usize, first: u16| TableSpec {
        start: sections[index],
        rows,
        row,
        first,
    };
    if !layout.unicode && sizes == [16320, 16320, 2048, 1024] {
        Some(TableSpecs {
            layers: spec(0, 255, 64, 1),
            colors: spec(1, 255, 64, 1),
            line_types: spec(2, 32, 64, 1),
            widths: spec(3, 16, 64, 1),
            layer_name: (32, 32),
            early: false,
        })
    } else if !layout.unicode && sizes == [12800, 2048, 1024, 1024] {
        Some(TableSpecs {
            layers: spec(0, 200, 64, 1),
            colors: spec(1, 32, 64, 1),
            line_types: spec(2, 16, 64, 1),
            widths: spec(3, 16, 64, 1),
            layer_name: (32, 32),
            early: true,
        })
    } else if layout.unicode && sizes == [282_624, 24_768, 141_312, 24_576] {
        Some(TableSpecs {
            layers: spec(0, 512, 552, 0),
            colors: spec(1, 258, 96, 0),
            line_types: spec(2, 256, 552, 0),
            widths: spec(3, 256, 96, 0),
            layer_name: (32, 520),
            early: false,
        })
    } else {
        None
    }
}

pub(crate) fn parse_header(
    bytes: &[u8],
    format: DracadFormat,
    limits: &ParseLimits,
) -> Result<Header> {
    if bytes.len() < 0x5c || !crate::format::has_drawing_identifier(bytes, false) {
        return Err(invalid("payload lacks a complete DRA-CAD header"));
    }
    let length = bytes.len() as u64;
    let marker_raw = u16::from_le_bytes([bytes[0x22], bytes[0x23]]);
    let field = |offset| word(bytes, offset).ok_or_else(|| invalid("truncated header field"));
    let start = field(0x24)?;
    let end = field(0x28)?;
    if start < 0x5c || start > end || end > length {
        return Err(invalid("record boundaries are outside the payload"));
    }
    let record_count = field(0x58)?
        .checked_add(1)
        .ok_or_else(|| invalid("record count overflow"))?;
    check_limit("records", record_count, limits.max_records)?;
    let layout = [Layout::NARROW, Layout::WIDE]
        .into_iter()
        .find(|layout| record_count.checked_mul(layout.record as u64) == Some(end - start))
        .ok_or_else(|| invalid("record count/stride/end relation is inconsistent"))?;
    let declared_end = field(0x50)?;
    let payload_end = if declared_end == 0 && format == DracadFormat::Mpx {
        length
    } else if declared_end.checked_add(4) == Some(length) {
        declared_end
    } else {
        return Err(invalid("declared payload end differs from payload length"));
    };
    let mut sections = [0; 7];
    for (index, value) in sections.iter_mut().enumerate() {
        *value = field(0x28 + index * 4)?;
    }
    if sections.windows(2).any(|pair| pair[1] < pair[0]) || sections[6] > payload_end {
        return Err(invalid("post-record boundaries are inconsistent"));
    }
    let block_start = field(0x4c)?;
    if block_start != 0 && !(sections[6]..=payload_end).contains(&block_start) {
        return Err(invalid("block table start is outside the payload tail"));
    }
    Ok(Header {
        marker_raw,
        layout,
        record_start: start,
        record_count,
        payload_end,
        tables: table_specs(&sections, layout),
        tail_start: sections[6],
        block_start,
    })
}

fn slice(bytes: &[u8], start: u64, size: u64) -> Option<&[u8]> {
    let end = start.checked_add(size)?;
    bytes.get(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
}

/// Name bytes of a definition header: up to the first NUL byte, or NUL unit
/// in the Unicode layout.
fn block_name(field: &[u8], unicode: bool) -> Option<Vec<u8>> {
    let end = if unicode {
        field
            .chunks_exact(2)
            .position(|unit| unit == [0, 0])
            .map(|units| units * 2)?
    } else {
        field.iter().position(|byte| *byte == 0)?
    };
    (end != 0).then(|| field[..end].to_vec())
}

/// Block definition tables, walked until the tail.
///
/// The second value is the offset of the tail extensions when they were
/// found; the third says whether the walk of the older layout ended anywhere
/// else. The Unicode layout has no such extensions and simply ends.
pub(crate) fn block_ranges(
    bytes: &[u8],
    header: &Header,
    max_definitions: u64,
    max_records: u64,
) -> Result<(Vec<BlockRange>, Option<u64>, bool)> {
    let unicode = header.layout.unicode;
    let record = header.layout.record as u64;
    let (prefix, header_size, name_size, count_at) = if unicode {
        (1096, 656, 520, 524)
    } else {
        (296, 176, 32, 36)
    };
    let mut ranges = Vec::new();
    if unicode && header.block_start == 0 {
        return Ok((ranges, None, false));
    }
    let mut cursor = if header.block_start != 0 {
        header.block_start
    } else {
        // Without definitions the tail follows a fixed empty prefix.
        match header.tail_start.checked_add(prefix) {
            Some(cursor) => cursor,
            None => return Ok((ranges, None, true)),
        }
    };
    let mut total = header.record_count;
    loop {
        if !unicode {
            match slice(bytes, cursor, 16) {
                Some(signature) if signature == TAIL_SIGNATURE => {
                    return Ok((ranges, Some(cursor), false))
                }
                Some(_) if header.block_start != 0 => {}
                _ => return Ok((ranges, None, true)),
            }
        }
        let Some(head) = slice(bytes, cursor, header_size) else {
            return Ok((ranges, None, !unicode));
        };
        let Some(name_bytes) = block_name(&head[..name_size as usize], unicode) else {
            return Ok((ranges, None, !unicode));
        };
        let record_count = word(head, count_at).unwrap_or(0);
        let record_start = cursor + header_size;
        let Some(table_end) = record_count
            .checked_add(1)
            .and_then(|count| count.checked_mul(record))
            .and_then(|extent| record_start.checked_add(extent))
            .filter(|end| *end <= header.payload_end)
        else {
            return Ok((ranges, None, !unicode));
        };
        // The terminal must sit exactly where the declared count puts it.
        match slice(bytes, table_end - record, 2) {
            Some([0xff, _]) => {}
            _ => return Ok((ranges, None, !unicode)),
        }
        check_limit(
            "block_definitions",
            ranges.len() as u64 + 1,
            max_definitions,
        )?;
        total = total
            .checked_add(record_count + 1)
            .ok_or_else(|| invalid("combined record count overflow"))?;
        check_limit("records", total, max_records)?;
        ranges.push(BlockRange {
            header_offset: cursor,
            name_bytes,
            record_start,
            record_count,
        });
        cursor = table_end;
    }
}
