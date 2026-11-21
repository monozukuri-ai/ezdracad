//! Carrier and header of a best-effort read.
//!
//! The record table relations are still validated; only the generation marker
//! and the size of the tail after the tables are left open.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::format::DRAWING_MAGIC;
use crate::limits::check_limit;
use crate::preflight::{invalid, CFB_MAGIC};
use crate::{DracadError, DracadFormat, ParseLimits, Result, RECORD_SIZE};

/// Generations the layouts in this module were checked against.
const OBSERVED_MARKERS: [u16; 5] = [5, 8, 10, 22, 25];
const TABLE_SIZES: [u64; 4] = [16320, 16320, 2048, 1024];
/// First extension header of the tail that follows the block definitions.
const TAIL_SIGNATURE: [u8; 16] = [2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0];
const BLOCK_HEADER_SIZE: u64 = 176;

pub(crate) struct Carrier {
    pub format: DracadFormat,
    pub payload: Arc<[u8]>,
    pub sha256: String,
}

pub(crate) struct Header {
    pub marker_raw: u16,
    pub marker_observed: bool,
    pub record_start: u64,
    /// Records of the root table, terminal included.
    pub record_count: u64,
    pub payload_end: u64,
    /// Starts of the layer, color, line-type and width tables when their sizes
    /// match the observed layout.
    pub tables: Option<[u64; 4]>,
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
    let info = crate::inspect(data, limits)?;
    match info.format {
        DracadFormat::Mpz | DracadFormat::Mps => {
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
            "protected or unrecognized MPS encoding; the drawing payload cannot be decoded".into(),
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

pub(crate) fn parse_header(
    bytes: &[u8],
    format: DracadFormat,
    limits: &ParseLimits,
) -> Result<Header> {
    if bytes.len() < 0x5c
        || !bytes.starts_with(DRAWING_MAGIC)
        || bytes[DRAWING_MAGIC.len()..32].iter().any(|byte| *byte != 0)
    {
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
    if record_count.checked_mul(RECORD_SIZE) != Some(end - start) {
        return Err(invalid("record count/stride/end relation is inconsistent"));
    }
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
    let tables = sections[..5]
        .windows(2)
        .zip(TABLE_SIZES)
        .all(|(pair, size)| pair[1] - pair[0] == size)
        .then(|| [sections[0], sections[1], sections[2], sections[3]]);
    let block_start = field(0x4c)?;
    if block_start != 0 && !(sections[6]..=payload_end).contains(&block_start) {
        return Err(invalid("block table start is outside the payload tail"));
    }
    Ok(Header {
        marker_raw,
        marker_observed: OBSERVED_MARKERS.contains(&marker_raw),
        record_start: start,
        record_count,
        payload_end,
        tables,
        tail_start: sections[6],
        block_start,
    })
}

fn slice(bytes: &[u8], start: u64, size: u64) -> Option<&[u8]> {
    let end = start.checked_add(size)?;
    bytes.get(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
}

/// Block definition tables, walked until the tail signature.
///
/// The second value is the offset of the tail extensions when it was found;
/// the third says whether the walk ended somewhere else.
pub(crate) fn block_ranges(
    bytes: &[u8],
    header: &Header,
    max_definitions: u64,
    max_records: u64,
) -> Result<(Vec<BlockRange>, Option<u64>, bool)> {
    let mut ranges = Vec::new();
    let mut cursor = if header.block_start != 0 {
        header.block_start
    } else {
        // Without definitions the tail follows a fixed empty prefix.
        match header.tail_start.checked_add(296) {
            Some(cursor) => cursor,
            None => return Ok((ranges, None, true)),
        }
    };
    let mut total = header.record_count;
    loop {
        match slice(bytes, cursor, 16) {
            Some(signature) if signature == TAIL_SIGNATURE => {
                return Ok((ranges, Some(cursor), false))
            }
            Some(_) if header.block_start != 0 => {}
            _ => return Ok((ranges, None, true)),
        }
        let Some(head) = slice(bytes, cursor, BLOCK_HEADER_SIZE) else {
            return Ok((ranges, None, true));
        };
        let Some(name_end) = head[..32].iter().position(|byte| *byte == 0) else {
            return Ok((ranges, None, true));
        };
        if name_end == 0 {
            return Ok((ranges, None, true));
        }
        let record_count = u64::from(u32::from_le_bytes([head[36], head[37], head[38], head[39]]));
        let record_start = cursor + BLOCK_HEADER_SIZE;
        let Some(table_end) = record_count
            .checked_add(1)
            .and_then(|count| count.checked_mul(RECORD_SIZE))
            .and_then(|extent| record_start.checked_add(extent))
            .filter(|end| *end <= header.payload_end)
        else {
            return Ok((ranges, None, true));
        };
        // The terminal must sit exactly where the declared count puts it.
        match slice(bytes, table_end - RECORD_SIZE, 2) {
            Some([0xff, _]) => {}
            _ => return Ok((ranges, None, true)),
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
            name_bytes: head[..name_end].to_vec(),
            record_start,
            record_count,
        });
        cursor = table_end;
    }
}
