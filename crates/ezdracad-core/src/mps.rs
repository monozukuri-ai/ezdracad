//! Whitelisted unprotected SecureContents envelopes. Never search for zlib magic.
use flate2::{Decompress, FlushDecompress, Status};
use sha2::{Digest, Sha256};

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{ParseLimits, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpsCompression {
    Zlib,
    Stored,
    SplitZlib,
}
impl MpsCompression {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zlib => "zlib",
            Self::Stored => "stored",
            Self::SplitZlib => "split_zlib",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpsChunk {
    /// Byte range in the original SecureContents stream.
    pub encoded_offset: u64,
    pub encoded_size: u64,
    /// Byte range in the logical decoded payload.
    pub decoded_offset: u64,
    pub decoded_size: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpsInfo {
    pub stream_path: String,
    pub stream_size: u64,
    pub stream_sha256: String,
    pub wrapper_profile: &'static str,
    pub compression: MpsCompression,
    pub decoded_size: u64,
    pub chunks: Vec<MpsChunk>,
}
const EXTENSION: [u8; 24] = [
    0, 0, 0, 0, 0x18, 0xfc, 0xff, 0xff, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x24, 0x40, 1, 0, 0, 0,
];

/// None means an unqualified protection/envelope profile; no data is decoded.
///
/// With `unicode` set, the two envelopes of the Unicode drawing format are
/// admitted too. Their payload uses another record layout, which only the
/// best-effort reader understands.
pub(crate) fn decode_envelope(
    bytes: &[u8],
    path: String,
    limits: &ParseLimits,
    unicode: bool,
) -> Result<Option<(MpsInfo, Vec<u8>)>> {
    if bytes.len() < 122 {
        return Ok(None);
    }
    let mut prefix = [0u8; 122];
    for at in [0, 4, 8] {
        prefix[at] = 1;
    }
    let kind = u16::from_le_bytes([bytes[18], bytes[19]]);
    let (extended, wide) = match kind {
        0 => (false, false),
        1 => (true, false),
        2 if unicode => (false, true),
        3 if unicode => (true, true),
        _ => return Ok(None),
    };
    prefix[18] = kind as u8;
    // Some saved drawings set a further switch here; they open like the others.
    if unicode && bytes[16] == 1 {
        prefix[16] = 1;
    }
    if bytes[..122] != prefix {
        return Ok(None);
    }
    let length_at = if extended {
        if bytes.len() < 146 {
            return Err(invalid("MPS extension truncated"));
        }
        if bytes[122..146] != EXTENSION {
            return Ok(None);
        }
        146
    } else {
        122
    };
    if bytes.len() < length_at + 8 {
        return Err(invalid("MPS length fields truncated"));
    }
    let decoded = u32::from_le_bytes(
        bytes[length_at..length_at + 4]
            .try_into()
            .map_err(|_| invalid("MPS size field"))?,
    ) as u64;
    let encoded = u32::from_le_bytes(
        bytes[length_at + 4..length_at + 8]
            .try_into()
            .map_err(|_| invalid("MPS size field"))?,
    ) as u64;
    check_limit("decompressed_size", decoded, limits.max_decompressed_size)?;
    let start = length_at + 8;
    if encoded != (bytes.len() - start) as u64 || encoded < 6 {
        return Err(invalid(
            "MPS compressed extent differs from size declaration",
        ));
    }
    let output = inflate(&bytes[start..], decoded, limits.max_decompressed_size)?;
    if output.len() < 84 || !crate::format::has_drawing_identifier(&output, !unicode) {
        return Err(invalid(
            "MPS decoded payload has no supported DRA-CAD prefix",
        ));
    }
    let end = u32::from_le_bytes(
        output[80..84]
            .try_into()
            .map_err(|_| invalid("MPS payload end field"))?,
    ) as u64;
    if end.checked_add(4) != Some(decoded) {
        return Err(invalid("MPS decoded payload is not a Contents envelope"));
    }
    let info = MpsInfo {
        stream_path: path,
        stream_size: bytes.len() as u64,
        stream_sha256: format!("{:x}", Sha256::digest(bytes)),
        wrapper_profile: match (extended, wide) {
            (false, false) => "unprotected_base_v1",
            (true, false) => "unprotected_extended_v1",
            (false, true) => "unprotected_unicode_base_v1",
            (true, true) => "unprotected_unicode_extended_v1",
        },
        compression: MpsCompression::Zlib,
        decoded_size: decoded,
        chunks: vec![MpsChunk {
            encoded_offset: start as u64,
            encoded_size: encoded,
            decoded_offset: 0,
            decoded_size: decoded,
        }],
    };
    Ok(Some((info, output)))
}

fn inflate(input: &[u8], declared: u64, limit: u64) -> Result<Vec<u8>> {
    let cmf = input[0];
    let flg = input[1];
    if cmf & 15 != 8
        || cmf >> 4 > 7
        || (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0
        || flg & 0x20 != 0
    {
        return Err(invalid(
            "MPS zlib header/dictionary is unsupported or malformed",
        ));
    }
    let mut decoder = Decompress::new(true);
    let mut output = Vec::new();
    let mut block = [0u8; 8192];
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let consumed =
            usize::try_from(before_in).map_err(|_| invalid("MPS input position overflow"))?;
        // One extra byte detects a falsely small declaration, without reserving
        // the advertised allocation or permitting unbounded decoder output.
        let room = declared
            .saturating_sub(before_out)
            .saturating_add(1)
            .min(block.len() as u64) as usize;
        let status = decoder
            .decompress(
                &input[consumed..],
                &mut block[..room],
                FlushDecompress::None,
            )
            .map_err(|error| invalid(format!("MPS zlib/checksum: {error}")))?;
        let produced = decoder.total_out() - before_out;
        check_limit("decompressed_size", decoder.total_out(), limit)?;
        if decoder.total_out() > declared {
            return Err(invalid("MPS decoded size exceeds declaration"));
        }
        output
            .try_reserve(produced as usize)
            .map_err(|_| invalid("MPS decoded allocation failed"))?;
        output.extend_from_slice(&block[..produced as usize]);
        if status == Status::StreamEnd {
            if decoder.total_in() != input.len() as u64 {
                return Err(invalid("MPS zlib has trailing or concatenated input"));
            }
            if decoder.total_out() != declared {
                return Err(invalid("MPS decoded size differs from declaration"));
            }
            return Ok(output);
        }
        if before_in == decoder.total_in() && before_out == decoder.total_out() {
            return Err(invalid("MPS zlib truncated or made no progress"));
        }
    }
}
