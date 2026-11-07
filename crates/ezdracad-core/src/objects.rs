//! One-level structural spans in the root raw-record table.
//!
//! A span's interior stays opaque. This does not establish logical drawing
//! membership, child semantics, coordinate systems, units or completeness.

use std::iter::FusedIterator;

use crate::preflight::invalid;
use crate::{RawDocument, Result};

/// One primary record and its bounded following slots in the root table.
///
/// The terminal is excluded. Offsets are relative to Contents; the end index
/// is exclusive. The exact primary header is retained without interpreting
/// attributes, flags or the interior slots as independent objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawObjectSpan {
    pub record_index: u64,
    pub payload_offset: u64,
    pub tag_raw: u16,
    pub header_bytes: [u8; 16],
    pub record_count: u64,
    pub end_record_index: u64,
}

/// Fallible, allocation-free walk of the root table's primary span fields.
///
/// Each request checks the next complete span. A malformed span produces one
/// error and ends iteration; consuming the whole iterator validates the whole
/// table. Unknown primary tags and all interior bytes remain uninterpreted.
pub struct ObjectSpans<'a> {
    document: &'a RawDocument,
    next_index: u64,
    finished: bool,
}

impl<'a> ObjectSpans<'a> {
    pub(crate) fn new(document: &'a RawDocument) -> Self {
        Self {
            document,
            next_index: 0,
            finished: false,
        }
    }

    fn read_next(&self) -> Result<RawObjectSpan> {
        let index = self.next_index;
        let record = self
            .document
            .record(index)
            .ok_or_else(|| invalid("missing primary object-span record"))?;
        if matches!(record.tag, 0x00fe | 0x00ff) {
            return Err(invalid(format!(
                "continuation or terminal tag starts an object span at record {index}"
            )));
        }
        let header_bytes: [u8; 16] = record
            .bytes()
            .get(..16)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| invalid("truncated object-span header"))?;
        let count = u64::from(u16::from_le_bytes([header_bytes[8], header_bytes[9]]));
        if count == 0 {
            return Err(invalid(format!("zero object span at record {index}")));
        }
        let end = index
            .checked_add(count)
            .filter(|end| *end < self.document.header().record_count)
            .ok_or_else(|| {
                invalid(format!(
                    "object span crosses the terminal at record {index}"
                ))
            })?;
        Ok(RawObjectSpan {
            record_index: index,
            payload_offset: record.payload_offset,
            tag_raw: record.tag,
            header_bytes,
            record_count: count,
            end_record_index: end,
        })
    }
}

impl Iterator for ObjectSpans<'_> {
    type Item = Result<RawObjectSpan>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.next_index >= self.document.header().record_count.saturating_sub(1)
        {
            return None;
        }
        let next = self.read_next();
        match &next {
            Ok(span) => self.next_index = span.end_record_index,
            Err(_) => self.finished = true,
        }
        Some(next)
    }
}

impl FusedIterator for ObjectSpans<'_> {}
