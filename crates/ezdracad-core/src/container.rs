use std::io::Cursor;

use crate::limits::check_limit;
use crate::preflight::preflight;
use crate::{DracadError, ParseLimits, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfbEntryKind {
    Storage,
    Stream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfbEntry {
    /// Absolute CFB path with forward slashes on every platform.
    pub path: String,
    pub kind: CfbEntryKind,
    /// Declared stream length; zero for a storage entry.
    pub size: u64,
}

/// Container metadata only: no DRA-CAD identity or stream integrity claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfbInfo {
    pub version: u16,
    pub root_clsid: String,
    /// Sorted by path. The root storage is represented by root_clsid, not an entry.
    pub entries: Vec<CfbEntry>,
    pub total_stream_size: u64,
    pub metadata_size: u64,
    pub directory_slots: u64,
    pub depth: u64,
}

/// Inspect CFB metadata using strict backend validation, without reading payloads.
/// Allocation preflight runs before backend metadata parsing.
pub fn inspect_cfb(data: &[u8], limits: &ParseLimits) -> Result<CfbInfo> {
    open_cfb(data, limits).map(|(_, info)| info)
}

type OpenedCfb<'a> = (cfb::CompoundFile<Cursor<&'a [u8]>>, CfbInfo);

pub(crate) fn open_cfb<'a>(data: &'a [u8], limits: &ParseLimits) -> Result<OpenedCfb<'a>> {
    limits.check_file_size(data.len() as u64)?;
    let guarded = preflight(data, limits)?;
    let compound = cfb::CompoundFile::open_strict(Cursor::new(data))
        .map_err(|error| DracadError::InvalidFormat(error.to_string()))?;
    let version = match compound.version() {
        cfb::Version::V3 => 3,
        cfb::Version::V4 => 4,
    };
    let mut entries = Vec::new();
    let mut total_stream_size = 0_u64;
    for entry in compound.walk().filter(|entry| !entry.is_root()) {
        check_limit(
            "cfb_entries",
            entries.len() as u64 + 1,
            limits.max_cfb_entries,
        )?;
        let (kind, size) = if entry.is_stream() {
            check_limit("stream_size", entry.len(), limits.max_stream_size)?;
            total_stream_size = total_stream_size.checked_add(entry.len()).ok_or_else(|| {
                DracadError::InvalidFormat("total declared stream size overflows u64".into())
            })?;
            check_limit(
                "total_stream_size",
                total_stream_size,
                limits.max_total_stream_size,
            )?;
            (CfbEntryKind::Stream, entry.len())
        } else {
            (CfbEntryKind::Storage, 0)
        };
        entries.push(CfbEntry {
            path: entry.path().to_string_lossy().replace('\\', "/"),
            kind,
            size,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let info = CfbInfo {
        version,
        root_clsid: compound.root_entry().clsid().to_string(),
        entries,
        total_stream_size,
        metadata_size: guarded.metadata_size,
        directory_slots: guarded.directory_slots,
        depth: guarded.depth,
    };
    Ok((compound, info))
}
