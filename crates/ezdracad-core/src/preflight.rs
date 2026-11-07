//! CFB allocation preflight; `cfb` remains the structural parser.
//! Field locations: MS-CFB 2.2, 2.3, 2.4 and 2.6.1.

use std::collections::HashSet;

use crate::limits::check_limit;
use crate::{DracadError, ParseLimits, Result};

pub(crate) const CFB_MAGIC: &[u8; 8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";
const FREE: u32 = 0xffff_ffff;
const END: u32 = 0xffff_fffe;

pub(crate) fn invalid(message: impl Into<String>) -> DracadError {
    DracadError::InvalidFormat(message.into())
}

fn word(bytes: &[u8], offset: usize) -> Result<u32> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated CFB field"))?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

pub(crate) struct PreflightInfo {
    pub metadata_size: u64,
    pub directory_slots: u64,
    pub depth: u64,
}

struct Layout<'a> {
    data: &'a [u8],
    sector_size: usize,
    sectors: usize,
    fat: Vec<u32>,
    metadata_size: u64,
    metadata_sectors: HashSet<u32>,
    limits: &'a ParseLimits,
}

impl Layout<'_> {
    fn sector(&self, id: u32) -> Result<&[u8]> {
        let index =
            usize::try_from(id).map_err(|_| invalid("sector index exceeds platform size"))?;
        if index >= self.sectors {
            return Err(invalid("CFB metadata sector is outside the file"));
        }
        let start = (index + 1)
            .checked_mul(self.sector_size)
            .ok_or_else(|| invalid("sector offset overflow"))?;
        self.data
            .get(start..start + self.sector_size)
            .ok_or_else(|| invalid("truncated CFB sector"))
    }

    fn claim(&mut self, id: u32) -> Result<()> {
        self.sector(id)?;
        let next = self
            .metadata_size
            .checked_add(self.sector_size as u64)
            .ok_or_else(|| invalid("metadata size overflow"))?;
        check_limit("cfb_metadata_size", next, self.limits.max_cfb_metadata_size)?;
        if !self.metadata_sectors.insert(id) {
            return Err(invalid(
                "CFB metadata chain cycles or overlaps another metadata chain",
            ));
        }
        self.metadata_size = next;
        Ok(())
    }

    fn append_fat(&mut self, id: u32, expected: usize) -> Result<()> {
        if id != FREE {
            if self.fat.len() >= expected {
                return Err(invalid("more FAT sectors than declared"));
            }
            self.claim(id)?;
            self.fat.push(id);
        }
        Ok(())
    }

    fn next(&self, id: u32) -> Result<u32> {
        self.sector(id)?;
        let per_sector = self.sector_size / 4;
        let index = id as usize;
        let fat_id = *self
            .fat
            .get(index / per_sector)
            .ok_or_else(|| invalid("FAT does not cover sector index"))?;
        word(self.sector(fat_id)?, (index % per_sector) * 4)
    }

    fn chain(&mut self, first: u32, directory: bool) -> Result<Vec<u32>> {
        let mut chain = Vec::new();
        let mut current = first;
        while current != END {
            self.claim(current)?;
            if directory {
                check_limit(
                    "cfb_directory_slots",
                    (chain.len() as u64 + 1) * (self.sector_size as u64 / 128),
                    self.limits.max_cfb_directory_slots,
                )?;
            }
            chain.push(current);
            current = self.next(current)?;
        }
        Ok(chain)
    }

    fn directory_entry<'a>(&'a self, sectors: &[u32], id: u32) -> Result<&'a [u8]> {
        let per_sector = self.sector_size / 128;
        let index = id as usize;
        let sector = *sectors
            .get(index / per_sector)
            .ok_or_else(|| invalid("directory link is outside directory array"))?;
        let offset = index % per_sector * 128;
        Ok(&self.sector(sector)?[offset..offset + 128])
    }

    fn directory_tree(&self, sectors: &[u32]) -> Result<u64> {
        let mut stack = vec![(0, 0)];
        let mut seen = HashSet::new();
        let mut depth_max = 0;
        while let Some((id, depth)) = stack.pop() {
            check_limit("cfb_depth", depth, self.limits.max_cfb_depth)?;
            if !seen.insert(id) {
                return Err(invalid("cyclic or multiply referenced CFB directory entry"));
            }
            check_limit(
                "cfb_entries",
                seen.len() as u64 - 1,
                self.limits.max_cfb_entries,
            )?;
            let entry = self.directory_entry(sectors, id)?;
            if (id == 0 && entry[66] != 5) || (id != 0 && !matches!(entry[66], 1 | 2)) {
                return Err(invalid("invalid reachable CFB directory object type"));
            }
            depth_max = depth_max.max(depth);
            for offset in [68, 72] {
                let next = word(entry, offset)?;
                if next != FREE {
                    stack.push((next, depth));
                }
            }
            let child = word(entry, 76)?;
            if child != FREE {
                if entry[66] == 2 {
                    return Err(invalid("CFB stream has directory children"));
                }
                stack.push((child, depth + 1));
            }
        }
        Ok(depth_max)
    }
}

pub(crate) fn preflight(data: &[u8], limits: &ParseLimits) -> Result<PreflightInfo> {
    limits.check_file_size(data.len() as u64)?;
    if data.len() < 512 || !data.starts_with(CFB_MAGIC) {
        return Err(invalid("not a complete CFB header"));
    }
    let sector_size = match (
        u16::from_le_bytes([data[26], data[27]]),
        u16::from_le_bytes([data[30], data[31]]),
    ) {
        (3, 9) => 512,
        (4, 12) => 4096,
        _ => return Err(invalid("unsupported CFB version/sector shift combination")),
    };
    if data.len() < 3 * sector_size || data.len() % sector_size != 0 {
        return Err(invalid("CFB file has missing or incomplete sectors"));
    }
    check_limit(
        "cfb_metadata_size",
        sector_size as u64,
        limits.max_cfb_metadata_size,
    )?;
    let mut layout = Layout {
        data,
        sector_size,
        sectors: data.len() / sector_size - 1,
        fat: Vec::new(),
        metadata_size: sector_size as u64,
        metadata_sectors: HashSet::new(),
        limits,
    };
    let expected_fat = word(data, 44)? as usize;
    for offset in (76..512).step_by(4) {
        layout.append_fat(word(data, offset)?, expected_fat)?;
    }
    let mut difat = word(data, 68)?;
    let mut difat_count = 0;
    while difat != END {
        layout.claim(difat)?;
        difat_count += 1;
        if difat_count > word(data, 72)? {
            return Err(invalid("DIFAT chain exceeds declared length"));
        }
        for offset in (0..sector_size - 4).step_by(4) {
            let id = word(layout.sector(difat)?, offset)?;
            layout.append_fat(id, expected_fat)?;
        }
        difat = word(layout.sector(difat)?, sector_size - 4)?;
    }
    if layout.fat.len() != expected_fat || difat_count != word(data, 72)? {
        return Err(invalid("FAT/DIFAT count does not match header"));
    }
    let directory = layout.chain(word(data, 48)?, true)?;
    if sector_size == 4096 && directory.len() as u64 != word(data, 40)? as u64 {
        return Err(invalid("directory chain length does not match header"));
    }
    let depth = layout.directory_tree(&directory)?;
    let mini = layout.chain(word(data, 60)?, false)?;
    if mini.len() as u64 != word(data, 64)? as u64 {
        return Err(invalid("MiniFAT length does not match header"));
    }
    Ok(PreflightInfo {
        metadata_size: layout.metadata_size,
        directory_slots: directory.len() as u64 * (sector_size as u64 / 128),
        depth,
    })
}
