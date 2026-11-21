//! Structural walk of a record table: objects, their continuations and members.

use crate::{DracadError, Result};

const RECORD: usize = 64;
pub(crate) const CONTINUATION: u16 = 0x00fe;
const TERMINAL: u16 = 0x00ff;

/// One object: a primary record, the continuation records that follow it
/// directly, and the member objects inside its declared length.
pub(crate) struct Node<'a> {
    pub tag: u16,
    /// Index of the primary record within the table.
    pub index: u64,
    /// Primary record and its own continuations.
    pub own: &'a [u8],
    /// Records of the whole object.
    pub record_count: u64,
    pub children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    #[must_use]
    pub fn head(&self) -> &'a [u8] {
        &self.own[..RECORD]
    }
    /// The `index`-th continuation record of the primary record, from zero.
    #[must_use]
    pub fn continuation(&self, index: usize) -> Option<&'a [u8]> {
        let start = (index + 1).checked_mul(RECORD)?;
        self.own.get(start..start + RECORD)
    }
    #[must_use]
    pub fn continuations(&self) -> usize {
        self.own.len() / RECORD - 1
    }
}

#[derive(Default)]
pub(crate) struct TreeIssues {
    pub orphan_records: u64,
    pub clamped_spans: u64,
}

fn tag_at(table: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([table[index * RECORD], table[index * RECORD + 1]])
}

/// Parse the records `start..end` of `table` into objects.
///
/// `table` holds whole records and excludes the table terminal. A declared
/// length that runs past `end` is cut there; a continuation that follows no
/// object is skipped. Both are counted in `issues`.
pub(crate) fn parse<'a>(
    table: &'a [u8],
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
    let mut nodes = Vec::new();
    let mut index = start;
    while index < end {
        let tag = tag_at(table, index);
        if tag == CONTINUATION || tag & 0x00ff == TERMINAL {
            issues.orphan_records += 1;
            index += 1;
            continue;
        }
        let at = index * RECORD;
        let declared = usize::from(u16::from_le_bytes([table[at + 8], table[at + 9]]));
        let mut stop = index + declared.max(1);
        if stop > end {
            issues.clamped_spans += 1;
            stop = end;
        }
        if declared <= 1 {
            // A single-record object keeps the continuations written after it.
            while stop < end && tag_at(table, stop) == CONTINUATION {
                stop += 1;
            }
        }
        let mut own = 1;
        while index + own < stop && tag_at(table, index + own) == CONTINUATION {
            own += 1;
        }
        let children = if index + own < stop {
            parse(table, index + own, stop, depth + 1, max_depth, issues)?
        } else {
            Vec::new()
        };
        nodes.push(Node {
            tag,
            index: index as u64,
            own: &table[at..at + own * RECORD],
            record_count: (stop - index) as u64,
            children,
        });
        index = stop;
    }
    Ok(nodes)
}
