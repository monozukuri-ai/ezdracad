//! Structural walk of a record table: objects, their continuations and members.

use crate::{DracadError, Result};

pub(crate) const CONTINUATION: u16 = 0x00fe;
const TERMINAL: u16 = 0x00ff;

/// Record layout of a payload.
///
/// The older family uses 64-byte records with a 16-byte header. The Unicode
/// family uses 80-byte records: objects and attributed parts have a 32-byte
/// header, continuations and plain parts keep the 16-byte one and gain body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    pub record: usize,
    pub unicode: bool,
}

impl Layout {
    pub const NARROW: Self = Self {
        record: 64,
        unicode: false,
    };
    pub const WIDE: Self = Self {
        record: 80,
        unicode: true,
    };

    /// Length of the header of an object or attributed part.
    pub const fn header(self) -> usize {
        if self.unicode {
            32
        } else {
            16
        }
    }

    /// Declared number of records of the object that starts with `record`.
    fn span(self, record: &[u8]) -> usize {
        if self.unicode {
            u32::from_le_bytes([record[12], record[13], record[14], record[15]]) as usize
        } else {
            usize::from(u16::from_le_bytes([record[8], record[9]]))
        }
    }
}

/// One object: a primary record, the continuation records that follow it
/// directly, and the member objects inside its declared length.
pub(crate) struct Node<'a> {
    pub tag: u16,
    /// Index of the primary record within the table.
    pub index: u64,
    /// Primary record and its own continuations.
    own: &'a [u8],
    layout: Layout,
    /// Records of the whole object.
    pub record_count: u64,
    pub children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    /// The primary record with its header.
    #[must_use]
    pub fn record(&self) -> &'a [u8] {
        &self.own[..self.layout.record]
    }
    /// Body of an object or attributed part: 48 bytes in both layouts.
    #[must_use]
    pub fn body(&self) -> &'a [u8] {
        &self.own[self.layout.header()..self.layout.record]
    }
    /// Body of a plain part, which has the short header in both layouts.
    #[must_use]
    pub fn plain(&self) -> &'a [u8] {
        &self.own[16..self.layout.record]
    }
    /// The `index`-th continuation record of the primary record, from zero.
    #[must_use]
    pub fn continuation(&self, index: usize) -> Option<&'a [u8]> {
        let start = (index + 1).checked_mul(self.layout.record)?;
        self.own.get(start..start + self.layout.record)
    }
    /// Body of the `index`-th continuation record.
    #[must_use]
    pub fn part(&self, index: usize) -> Option<&'a [u8]> {
        self.continuation(index).map(|record| &record[16..])
    }
    #[must_use]
    pub fn continuations(&self) -> usize {
        self.own.len() / self.layout.record - 1
    }
}

#[derive(Default)]
pub(crate) struct TreeIssues {
    pub orphan_records: u64,
    pub clamped_spans: u64,
}

/// Parse the records `start..end` of `table` into objects.
///
/// `table` holds whole records and excludes the table terminal. A declared
/// length that runs past `end` is cut there; a continuation that follows no
/// object is skipped. Both are counted in `issues`.
pub(crate) fn parse<'a>(
    table: &'a [u8],
    layout: Layout,
    start: usize,
    end: usize,
    depth: u64,
    max_depth: u64,
    issues: &mut TreeIssues,
) -> Result<Vec<Node<'a>>> {
    if depth > max_depth {
        return Err(DracadError::LimitExceeded {
            resource: "best_effort_depth",
            limit: max_depth,
            actual: depth,
        });
    }
    let size = layout.record;
    let tag_at = |index: usize| u16::from_le_bytes([table[index * size], table[index * size + 1]]);
    let mut nodes = Vec::new();
    let mut index = start;
    while index < end {
        let tag = tag_at(index);
        if tag == CONTINUATION || tag & 0x00ff == TERMINAL {
            issues.orphan_records += 1;
            index += 1;
            continue;
        }
        let at = index * size;
        let declared = layout.span(&table[at..at + size]);
        let mut stop = index.saturating_add(declared.max(1));
        if stop > end {
            issues.clamped_spans += 1;
            stop = end;
        }
        if declared <= 1 {
            // A single-record object keeps the continuations written after it.
            while stop < end && tag_at(stop) == CONTINUATION {
                stop += 1;
            }
        }
        let mut own = 1;
        while index + own < stop && tag_at(index + own) == CONTINUATION {
            own += 1;
        }
        let children = if index + own < stop {
            parse(
                table,
                layout,
                index + own,
                stop,
                depth + 1,
                max_depth,
                issues,
            )?
        } else {
            Vec::new()
        };
        nodes.push(Node {
            tag,
            index: index as u64,
            own: &table[at..at + own * size],
            layout,
            record_count: (stop - index) as u64,
            children,
        });
        index = stop;
    }
    Ok(nodes)
}
