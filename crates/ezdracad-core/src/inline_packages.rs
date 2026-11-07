//! Explicit, bounded decoding of the observed inline-package subset.
//!
//! No placement, unit conversion or effective-attribute inheritance is applied.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{DracadError, LineRecord2D, ObservedProfile, RawRecord, Result};

pub const DEFAULT_MAX_INLINE_PACKAGE_DEPTH: u64 = 32;
pub const MAX_INLINE_PACKAGE_DEPTH: u64 = 128;
/// Bounds copies of ancestor descriptors even with explicitly raised outputs.
pub const MAX_INLINE_PACKAGE_PROVENANCE: u64 = 131_072;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlinePackageLimits {
    pub max_depth: u64,
    pub max_nodes: u64,
    pub max_lines: u64,
}

impl Default for InlinePackageLimits {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_INLINE_PACKAGE_DEPTH,
            max_nodes: crate::MAX_RECORDS_PER_PAGE,
            max_lines: crate::MAX_RECORDS_PER_PAGE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineTableDomain {
    RootRecords,
    BlockDefinitionRecords,
}

impl InlineTableDomain {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RootRecords => "root_records",
            Self::BlockDefinitionRecords => "block_definition_records",
        }
    }
}

/// Source index is local to the selected table; offset is relative to Contents.
/// Body bytes are retained, not exposed as an inferred MPXL package flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlinePackageRecord {
    pub record_index: u64,
    pub payload_offset: u64,
    pub record_count: u64,
    pub header_bytes: [u8; 16],
    pub body_bytes: [u8; 48],
}

/// A record-local line and its complete outer-to-inner inline-package ancestry.
/// This is not normalized geometry or a completeness/visibility certificate.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedInlineLine2D {
    pub source: LineRecord2D,
    pub ancestors: Vec<InlinePackageRecord>,
    pub table_domain: InlineTableDomain,
    pub definition_index: Option<u64>,
}

pub(crate) struct TableIdentity {
    pub domain: InlineTableDomain,
    pub definition_index: Option<u64>,
    pub profile: ObservedProfile,
}

struct Table<'a> {
    contents: &'a [u8],
    start: u64,
    nonterminal_count: u64,
}

impl Table<'_> {
    fn record(&self, index: u64) -> Result<RawRecord<'_>> {
        let offset = index
            .checked_mul(64)
            .and_then(|relative| self.start.checked_add(relative))
            .ok_or_else(|| invalid("inline-package source offset overflow"))?;
        RawRecord::at(self.contents, index, offset)
            .ok_or_else(|| invalid("missing bounded inline-package record"))
    }

    fn span(&self, index: u64, end: u64) -> Result<(RawRecord<'_>, u64)> {
        let record = self.record(index)?;
        if matches!(record.tag, 0xfe | 0xff) {
            return Err(invalid("orphan inline-package continuation/terminal"));
        }
        let bytes = record.bytes();
        let span = u64::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        if span == 0 {
            return Err(invalid("zero inline-package object span"));
        }
        let next = index
            .checked_add(span)
            .filter(|next| *next <= end)
            .ok_or_else(|| invalid("inline-package object span exceeds its parent"))?;
        Ok((record, next))
    }

    fn selected_primary(&self, selected: u64) -> Result<()> {
        if selected >= self.nonterminal_count {
            return Err(invalid(
                "inline-package selection is outside nonterminal records",
            ));
        }
        let mut cursor = 0;
        while cursor < selected {
            let (_, end) = self.span(cursor, self.nonterminal_count)?;
            if end > selected {
                return Err(DracadError::UnsupportedFeature(
                    "inline-package selection is inside another primary span".into(),
                ));
            }
            cursor = end;
        }
        Ok(())
    }
}

fn package(
    record: RawRecord<'_>,
    end: u64,
    profile: ObservedProfile,
) -> Result<InlinePackageRecord> {
    let data = record.bytes();
    if !matches!(record.tag, 0x30 | 0x430)
        || data.get(16..20) != Some(0x400u32.to_le_bytes().as_slice())
        || data[20..].iter().any(|byte| *byte != 0)
        || end - record.index < 2
    {
        return Err(DracadError::UnsupportedFeature(
            "unqualified inline-package tag, state body or empty variant".into(),
        ));
    }
    record.record_attributes()?.ensure_profile(profile)?;
    Ok(InlinePackageRecord {
        record_index: record.index,
        payload_offset: record.payload_offset,
        record_count: end - record.index,
        header_bytes: data[..16]
            .try_into()
            .map_err(|_| invalid("truncated inline-package header"))?,
        body_bytes: data[16..]
            .try_into()
            .map_err(|_| invalid("truncated inline-package body"))?,
    })
}

struct Frame {
    next: u64,
    end: u64,
}

pub(crate) fn decode(
    contents: &[u8],
    table_start: u64,
    record_count: u64,
    selected: u64,
    identity: TableIdentity,
    limits: &InlinePackageLimits,
) -> Result<Vec<ScopedInlineLine2D>> {
    let table = Table {
        contents,
        start: table_start,
        nonterminal_count: record_count
            .checked_sub(1)
            .ok_or_else(|| invalid("inline-package table lacks a terminal"))?,
    };
    table.selected_primary(selected)?;
    let (record, end) = table.span(selected, table.nonterminal_count)?;
    let root = package(record, end, identity.profile)?;
    let depth_limit = limits.max_depth.min(MAX_INLINE_PACKAGE_DEPTH);
    check_limit("inline_package_depth", 1, depth_limit)?;
    check_limit("inline_package_nodes", 1, limits.max_nodes)?;
    let mut nodes = 1u64;
    let mut provenance = 0u64;
    let mut ancestors = vec![root];
    let mut stack = vec![Frame {
        next: selected + 1,
        end,
    }];
    let mut lines = Vec::new();
    while let Some(frame) = stack.last() {
        if frame.next == frame.end {
            stack.pop();
            ancestors.pop();
            continue;
        }
        let index = frame.next;
        let (record, next) = table.span(index, frame.end)?;
        // Advance the parent's cursor before pushing a child frame.
        if let Some(parent) = stack.last_mut() {
            parent.next = next;
        }
        nodes += 1;
        check_limit("inline_package_nodes", nodes, limits.max_nodes)?;
        match record.tag {
            2 | 0x402 | 0x2402 | 0x4402 if next == index + 1 => {
                let attributed = record
                    .attributed_line_record()?
                    .ok_or_else(|| invalid("missing bounded inline line fields"))?;
                attributed.attributes.ensure_profile(identity.profile)?;
                check_limit(
                    "inline_package_lines",
                    lines.len() as u64 + 1,
                    limits.max_lines,
                )?;
                provenance = provenance
                    .checked_add(ancestors.len() as u64)
                    .ok_or_else(|| invalid("inline-package provenance count overflow"))?;
                check_limit(
                    "inline_package_provenance",
                    provenance,
                    MAX_INLINE_PACKAGE_PROVENANCE,
                )?;
                lines.push(ScopedInlineLine2D {
                    source: attributed.source,
                    ancestors: ancestors.clone(),
                    table_domain: identity.domain,
                    definition_index: identity.definition_index,
                });
            }
            0x30 | 0x430 => {
                let child = package(record, next, identity.profile)?;
                check_limit(
                    "inline_package_depth",
                    ancestors.len() as u64 + 1,
                    depth_limit,
                )?;
                ancestors.push(child);
                stack.push(Frame {
                    next: index + 1,
                    end: next,
                });
            }
            _ => {
                return Err(DracadError::UnsupportedFeature(format!(
                    "unqualified inline-package child tag 0x{:04x} or span {}",
                    record.tag,
                    next - index,
                )))
            }
        }
    }
    Ok(lines)
}
