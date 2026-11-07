//! Bounded views of observed block-definition tables, not drawing completeness.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{DracadError, LineRecord2D, ObservedProfile, RawDocument, RawRecord, Result};

pub const BLOCK_HEADER_SIZE: u64 = 176;
pub const DEFAULT_MAX_BLOCK_DEFINITIONS: u64 = 4096;

/// Offsets are relative to Contents. Name bytes are internal keys, not DXF names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDefinition {
    pub index: u64,
    pub header_offset: u64,
    pub header_bytes: [u8; 176],
    pub name_bytes: Vec<u8>,
    pub record_start: u64,
    pub record_end: u64,
    pub declared_nonterminal_records: u32,
    pub record_count: u64,
}

/// One immutable payload and bounded descriptors; record/trailer bytes stay raw.
#[derive(Debug, Clone)]
pub struct BlockLibrary {
    root: RawDocument,
    definitions: Vec<BlockDefinition>,
    suffix_offset: Option<u64>,
    suffix_size: u64,
}

impl BlockLibrary {
    /// Resolve native style indices for direct lines of one primary root reference.
    /// This retains source chains and uses the restricted planar placement subset.
    pub fn effective_line_styles(
        &self,
        reference_record_index: u64,
        max_lines: u64,
    ) -> Result<Vec<crate::EffectiveBlockLineStyle>> {
        crate::effective_styles::effective_block_line_styles(
            &self.root,
            self,
            reference_record_index,
            max_lines,
        )
    }

    /// Explicit inline-package subtree in one definition, retaining local scope.
    /// This does not change the direct-only planar/placed APIs.
    pub fn inline_package_lines(
        &self,
        definition_index: u64,
        index: u64,
    ) -> Result<Vec<crate::ScopedInlineLine2D>> {
        self.inline_package_lines_with_limits(
            definition_index,
            index,
            &crate::InlinePackageLimits::default(),
        )
    }

    pub fn inline_package_lines_with_limits(
        &self,
        definition_index: u64,
        index: u64,
        limits: &crate::InlinePackageLimits,
    ) -> Result<Vec<crate::ScopedInlineLine2D>> {
        let definition = usize::try_from(definition_index)
            .ok()
            .and_then(|index| self.definitions.get(index))
            .ok_or_else(|| invalid("block definition index is out of range"))?;
        crate::inline_packages::decode(
            self.root.contents(),
            definition.record_start,
            definition.record_count,
            index,
            crate::inline_packages::TableIdentity {
                domain: crate::InlineTableDomain::BlockDefinitionRecords,
                definition_index: Some(definition_index),
                profile: self.root.header().profile,
            },
            limits,
        )
    }

    pub(crate) fn parse(root: RawDocument, max_records: u64, max_definitions: u64) -> Result<Self> {
        let header = root.header().clone();
        let mut result = Self {
            root,
            definitions: Vec::new(),
            suffix_offset: None,
            suffix_size: 0,
        };
        let start = header.optional_offsets[2];
        let suffix_size = match header.profile {
            ObservedProfile::Marker5 => 1768,
            ObservedProfile::Marker25 => 7272,
            ObservedProfile::Marker8 => 1768,
        };
        // Other optional tables may establish a different tail partition.
        if header.optional_offsets[..2]
            .iter()
            .any(|offset| *offset != 0)
        {
            return Err(DracadError::UnsupportedFeature(
                "combined optional block-tail tables are unqualified".into(),
            ));
        }
        let definitions_start = header.section_offsets[6]
            .checked_add(296)
            .ok_or_else(|| invalid("block table prefix overflow"))?;
        let definitions_end = header
            .payload_end
            .checked_sub(suffix_size)
            .filter(|end| *end >= definitions_start)
            .ok_or_else(|| invalid("block table has no bounded definitions/shared suffix"))?;
        if result
            .slice(header.section_offsets[6], 296)?
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(DracadError::UnsupportedFeature(
                "unqualified block table prefix".into(),
            ));
        }
        if (start == 0 && definitions_start != definitions_end)
            || (start != 0 && (start != definitions_start || start == definitions_end))
        {
            return Err(DracadError::UnsupportedFeature(
                "unqualified block table origin/extent".into(),
            ));
        }
        result.suffix_offset = Some(definitions_end);
        result.suffix_size = suffix_size;
        if start == 0 {
            return Ok(result);
        }
        let mut cursor = start;
        let mut total_records = header.record_count;
        while cursor < definitions_end {
            check_limit(
                "block_definitions",
                result.definitions.len() as u64 + 1,
                max_definitions,
            )?;
            let table_start = cursor
                .checked_add(BLOCK_HEADER_SIZE)
                .ok_or_else(|| invalid("block header offset overflow"))?;
            if table_start > definitions_end {
                return Err(invalid("truncated block-definition header"));
            }
            let bytes: [u8; 176] = result
                .slice(cursor, BLOCK_HEADER_SIZE)?
                .try_into()
                .map_err(|_| invalid("truncated block header"))?;
            let name_end = bytes[..32]
                .iter()
                .position(|byte| *byte == 0)
                .filter(|end| *end != 0)
                .ok_or_else(|| {
                    DracadError::UnsupportedFeature(
                        "unqualified empty or unterminated block name".into(),
                    )
                })?;
            let declared = u32::from_le_bytes(
                bytes[36..40]
                    .try_into()
                    .map_err(|_| invalid("truncated block count"))?,
            );
            let count = u64::from(declared) + 1;
            total_records = total_records
                .checked_add(count)
                .ok_or_else(|| invalid("combined root/block record count overflow"))?;
            check_limit("records", total_records, max_records)?;
            let table_end = count
                .checked_mul(64)
                .and_then(|extent| table_start.checked_add(extent))
                .ok_or_else(|| invalid("block record extent overflow"))?;
            if table_end > definitions_end {
                return Err(invalid("block records exceed the shared-suffix boundary"));
            }
            for index in 0..count {
                let offset = table_start + index * 64;
                let data = result.slice(offset, 64)?;
                let tag = u16::from_le_bytes([data[0], data[1]]);
                if (tag == 0xff) != (index + 1 == count) {
                    return Err(invalid("block terminal tag has an inconsistent position"));
                }
            }
            result.definitions.push(BlockDefinition {
                index: result.definitions.len() as u64,
                header_offset: cursor,
                header_bytes: bytes,
                name_bytes: bytes[..name_end].to_vec(),
                record_start: table_start,
                record_end: table_end,
                declared_nonterminal_records: declared,
                record_count: count,
            });
            cursor = table_end;
        }
        Ok(result)
    }

    fn slice(&self, start: u64, size: u64) -> Result<&[u8]> {
        let end = start
            .checked_add(size)
            .ok_or_else(|| invalid("block slice overflow"))?;
        let start = usize::try_from(start).map_err(|_| invalid("block offset is too large"))?;
        let end = usize::try_from(end).map_err(|_| invalid("block end is too large"))?;
        self.root
            .contents()
            .get(start..end)
            .ok_or_else(|| invalid("block slice exceeds payload"))
    }

    #[must_use]
    pub fn definitions(&self) -> &[BlockDefinition] {
        &self.definitions
    }

    /// Duplicate internal keys are retained; resolving them is ambiguous.
    pub fn resolve(&self, name: &[u8]) -> Result<&BlockDefinition> {
        let mut matches = self
            .definitions
            .iter()
            .filter(|definition| definition.name_bytes == name);
        let first = matches.next().ok_or_else(|| {
            DracadError::UnsupportedFeature("unresolved block reference name".into())
        })?;
        if matches.next().is_some() {
            return Err(DracadError::UnsupportedFeature(
                "ambiguous block reference name".into(),
            ));
        }
        Ok(first)
    }

    /// Record index is local to the selected definition; offset is in Contents.
    #[must_use]
    pub fn record(&self, definition: u64, index: u64) -> Option<RawRecord<'_>> {
        let definition = self.definitions.get(usize::try_from(definition).ok()?)?;
        if index >= definition.record_count {
            return None;
        }
        let offset = definition
            .record_start
            .checked_add(index.checked_mul(64)?)?;
        RawRecord::at(self.root.contents(), index, offset)
    }

    pub fn suffix_offset(&self) -> Option<u64> {
        self.suffix_offset
    }

    pub fn suffix_bytes(&self) -> Result<&[u8]> {
        match self.suffix_offset {
            Some(offset) => self.slice(offset, self.suffix_size),
            None => Ok(&[]),
        }
    }

    /// Decode a whole definition only when each object is a direct planar line
    /// or a bounded 0x0033 metadata object with FE continuations. Unknown
    /// compounds (including 0x0030) stop the request rather than losing parents.
    pub fn planar_line_records(&self, definition_index: u64) -> Result<Vec<LineRecord2D>> {
        self.planar_line_records_with_limit(definition_index, crate::MAX_RECORDS_PER_PAGE)
    }

    pub fn planar_line_records_with_limit(
        &self,
        definition_index: u64,
        max_lines: u64,
    ) -> Result<Vec<LineRecord2D>> {
        let definition = self
            .definitions
            .get(
                usize::try_from(definition_index)
                    .map_err(|_| invalid("block definition index is too large"))?,
            )
            .ok_or_else(|| invalid("block definition index is out of range"))?;
        if definition.header_bytes[40..112]
            .iter()
            .chain(&definition.header_bytes[160..176])
            .any(|byte| *byte != 0)
        {
            return Err(DracadError::UnsupportedFeature(
                "unqualified block-definition coordinate fields".into(),
            ));
        }
        let mut lines = Vec::new();
        let mut cursor = 0;
        while cursor + 1 < definition.record_count {
            let record = self
                .record(definition_index, cursor)
                .ok_or_else(|| invalid("missing bounded block record"))?;
            let span = u64::from(u16::from_le_bytes([record.bytes()[8], record.bytes()[9]]));
            let end = cursor
                .checked_add(span)
                .filter(|end| *end < definition.record_count)
                .ok_or_else(|| invalid("block object span exceeds nonterminal records"))?;
            if span == 0 {
                return Err(invalid("zero block object span"));
            }
            match record.tag {
                0x0002 | 0x0402 if span == 1 => {
                    let line = record
                        .line_record()?
                        .ok_or_else(|| invalid("missing line fields"))?;
                    if line.elevation != 0.0 || line.thickness != 0.0 {
                        return Err(DracadError::UnsupportedFeature(
                            "nonzero block line elevation/thickness has no qualified placement"
                                .into(),
                        ));
                    }
                    check_limit("block_lines", lines.len() as u64 + 1, max_lines)?;
                    lines.push(line);
                }
                0x0033 => {
                    for index in cursor + 1..end {
                        if self.record(definition_index, index).map(|r| r.tag) != Some(0xfe) {
                            return Err(DracadError::UnsupportedFeature(
                                "unqualified metadata continuation inside block".into(),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(DracadError::UnsupportedFeature(format!(
                        "unqualified block object tag 0x{:04x} or span {span} at record {cursor}",
                        record.tag,
                    )))
                }
            }
            cursor = end;
        }
        Ok(lines)
    }

    /// Explicit package-parent XY placement of one reference. This is not WCS,
    /// an entity query, visibility evaluation or a drawing-completeness claim.
    pub fn placed_line_records(&self, reference_index: u64) -> Result<Vec<PlacedBlockLine2D>> {
        self.placed_line_records_with_limit(reference_index, crate::MAX_RECORDS_PER_PAGE)
    }

    pub fn placed_line_records_with_limit(
        &self,
        reference_index: u64,
        max_lines: u64,
    ) -> Result<Vec<PlacedBlockLine2D>> {
        let reference = self
            .root
            .package_reference(reference_index)?
            .ok_or_else(|| {
                DracadError::UnsupportedFeature(
                    "record is not a recognized package reference".into(),
                )
            })?;
        // Validate the supported subset even when the definition has no lines.
        reference.apply_xy([0.0, 0.0])?;
        let definition = self.resolve(reference.name_key())?;
        let lines = self.planar_line_records_with_limit(definition.index, max_lines)?;
        let mut placed = Vec::with_capacity(lines.len());
        for source in lines {
            placed.push(PlacedBlockLine2D {
                reference_record_index: reference_index,
                definition_index: definition.index,
                start: reference.apply_xy(source.start)?,
                end: reference.apply_xy(source.end)?,
                source,
            });
        }
        Ok(placed)
    }
}

/// Source indices remain definition-local; payload offsets are in Contents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedBlockLine2D {
    pub reference_record_index: u64,
    pub definition_index: u64,
    pub source: LineRecord2D,
    pub start: [f64; 2],
    pub end: [f64; 2],
}

impl PlacedBlockLine2D {
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "package_parent"
    }
}
