use crate::{DracadError, Result};

pub const DEFAULT_MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;
pub const DEFAULT_MAX_CFB_ENTRIES: u64 = 4096;
pub const DEFAULT_MAX_STREAM_SIZE: u64 = 64 * 1024 * 1024;
pub const DEFAULT_MAX_TOTAL_STREAM_SIZE: u64 = 128 * 1024 * 1024;
pub const DEFAULT_MAX_CFB_METADATA_SIZE: u64 = 8 * 1024 * 1024;
pub const DEFAULT_MAX_CFB_DIRECTORY_SLOTS: u64 = 16384;
pub const DEFAULT_MAX_CFB_DEPTH: u64 = 64;
pub const DEFAULT_MAX_DECOMPRESSED_SIZE: u64 = 128 * 1024 * 1024;
pub const DEFAULT_MAX_RECORDS: u64 = 1_000_000;

/// Size limits in bytes; the entry count excludes the CFB root storage.
/// Zero limits are permitted (for example, to reject all nonempty streams).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_file_size: u64,
    pub max_cfb_entries: u64,
    pub max_stream_size: u64,
    pub max_total_stream_size: u64,
    /// On-disk metadata bytes, not a heap quota.
    pub max_cfb_metadata_size: u64,
    /// Includes root and unallocated directory slots, before backend parsing.
    pub max_cfb_directory_slots: u64,
    /// Root children have depth one; siblings do not increase depth.
    pub max_cfb_depth: u64,
    /// Includes the terminal record; this is not a drawing entity count.
    pub max_records: u64,
    /// Aggregate decoded MPS bytes; checked before allocation and during decoding.
    pub max_decompressed_size: u64,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_cfb_entries: DEFAULT_MAX_CFB_ENTRIES,
            max_stream_size: DEFAULT_MAX_STREAM_SIZE,
            max_total_stream_size: DEFAULT_MAX_TOTAL_STREAM_SIZE,
            max_cfb_metadata_size: DEFAULT_MAX_CFB_METADATA_SIZE,
            max_cfb_directory_slots: DEFAULT_MAX_CFB_DIRECTORY_SLOTS,
            max_cfb_depth: DEFAULT_MAX_CFB_DEPTH,
            max_records: DEFAULT_MAX_RECORDS,
            max_decompressed_size: DEFAULT_MAX_DECOMPRESSED_SIZE,
        }
    }
}

impl ParseLimits {
    /// Check before allocating an owned copy of input at an FFI boundary.
    pub fn check_file_size(&self, size: u64) -> Result<()> {
        check_limit("file_size", size, self.max_file_size)
    }
}

pub(crate) fn check_limit(resource: &'static str, actual: u64, limit: u64) -> Result<()> {
    if actual > limit {
        Err(DracadError::LimitExceeded {
            resource,
            limit,
            actual,
        })
    } else {
        Ok(())
    }
}
