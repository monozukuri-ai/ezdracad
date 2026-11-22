use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::container::open_cfb;
use crate::format::DRAWING_MAGIC;
use crate::preflight::{invalid, CFB_MAGIC};
use crate::{
    CfbEntryKind, ContainerInfo, Diagnostic, DracadError, DracadFormat, ParseLimits, PayloadInfo,
    Result, DRACAD_CLSID,
};

/// Root storage class identifier of a compound file that never had one set.
const NIL_CLSID: &str = "00000000-0000-0000-0000-000000000000";

/// Identify a supported carrier and retain exact MPZ Contents or flat MPX bytes.
pub fn inspect(data: &[u8], limits: &ParseLimits) -> Result<ContainerInfo> {
    inspect_with(data, limits, false)
}

/// As [`inspect`]; with `unicode` set, the envelopes of the Unicode drawing
/// format are decoded too and reported as [`DracadFormat::Mpuw`], and a
/// compound file without a class identifier on its root storage is accepted
/// when its drawing stream identifies itself. Older releases saved such files.
pub(crate) fn inspect_with(
    data: &[u8],
    limits: &ParseLimits,
    unicode: bool,
) -> Result<ContainerInfo> {
    limits.check_file_size(data.len() as u64)?;
    if !data.starts_with(CFB_MAGIC) && data.starts_with(DRAWING_MAGIC) {
        // A real flat MPX is not MPZ Contents with a different filename. The
        // carrier has a zero end word and EOF-derived end; keep every byte.
        let header = crate::header::parse_mpx_header(data, limits)?;
        return Ok(ContainerInfo {
            format:DracadFormat::Mpx,file_size:data.len() as u64,cfb:None,
            payload:Some(PayloadInfo {domain:crate::PayloadOffsetDomain::File,stream_path:None,size:data.len() as u64,sha256:format!("{:x}",Sha256::digest(data)),marker_raw:header.marker_raw}),
            diagnostics:vec![Diagnostic {code:"mpx_payload_retained",message:"Observed flat marker25 MPX envelope; original file bytes retained. Inspection does not decode geometry or establish opaque-tail completeness".into()}],
            contents:Some(Arc::from(data)),mps:None,
        });
    }
    let (mut compound, cfb) = open_cfb(data, limits)?;
    let mut info = ContainerInfo {
        format: DracadFormat::UnknownCfb,
        file_size: data.len() as u64,
        cfb: Some(cfb),
        payload: None,
        mps: None,
        diagnostics: Vec::new(),
        contents: None,
    };
    let root_clsid = &info
        .cfb
        .as_ref()
        .ok_or_else(|| invalid("missing CFB metadata"))?
        .root_clsid;
    let anonymous = unicode && root_clsid == NIL_CLSID;
    if root_clsid != DRACAD_CLSID && !anonymous {
        info.diagnostics.push(Diagnostic {
            code: "unrecognized_clsid",
            message: "CFB root CLSID is not the observed DRA-CAD CLSID; no drawing identified"
                .into(),
        });
        return Ok(info);
    }
    let contents = info
        .cfb
        .as_ref()
        .ok_or_else(|| invalid("missing CFB metadata"))?
        .entries
        .iter()
        .find(|entry| {
            entry.path.eq_ignore_ascii_case("/Contents") && entry.kind == CfbEntryKind::Stream
        });
    let secure = info
        .cfb
        .as_ref()
        .ok_or_else(|| invalid("missing CFB metadata"))?
        .entries
        .iter()
        .find(|entry| {
            entry.path.eq_ignore_ascii_case("/SecureContents") && entry.kind == CfbEntryKind::Stream
        });
    if secure.is_some() && contents.is_some() {
        return Err(invalid("both Contents and SecureContents exist at root"));
    }
    if let Some(entry) = secure {
        info.format = DracadFormat::MpsCandidate;
        let mut stream = compound
            .open_stream(&entry.path)
            .map_err(|error| invalid(error.to_string()))?;
        let bytes = read_limited(&mut stream, entry.size, ReadDomain::Contents)?;
        if bytes.len() as u64 != entry.size {
            return Err(invalid("SecureContents length differs from declared size"));
        }
        if let Some((wrapper, decoded)) =
            crate::mps::decode_envelope(&bytes, entry.path.clone(), limits, unicode)?
        {
            info.format = if wrapper.wrapper_profile.starts_with("unprotected_unicode") {
                DracadFormat::Mpuw
            } else {
                DracadFormat::Mps
            };
            info.payload = Some(PayloadInfo {
                domain: crate::PayloadOffsetDomain::DecodedPayload,
                stream_path: None,
                size: decoded.len() as u64,
                sha256: format!("{:x}", Sha256::digest(&decoded)),
                marker_raw: u16::from_le_bytes([decoded[34], decoded[35]]),
            });
            info.mps = Some(wrapper);
            info.contents = Some(Arc::from(decoded));
            info.diagnostics.push(Diagnostic{code:"mps_payload_decoded",message:"Whitelisted unprotected MPS envelope decoded; all byte offsets refer to decoded_payload, not compressed stream positions. Geometry qualification remains separate".into()});
        } else {
            info.diagnostics.push(Diagnostic{code:"mps_wrapper_unsupported",message:"SecureContents protection or wrapper profile is unqualified; no decoding attempted".into()});
        }
        return Ok(info);
    }
    let Some(entry) = contents else {
        info.diagnostics.push(Diagnostic {
            code: "missing_drawing_stream",
            message: "DRA-CAD CLSID without a root Contents or SecureContents stream".into(),
        });
        return Ok(info);
    };
    let path = entry.path.clone();
    let declared_size = entry.size;
    let mut stream = compound
        .open_stream(&path)
        .map_err(|error| invalid(error.to_string()))?;
    let bytes = read_limited(&mut stream, declared_size, ReadDomain::Contents)?;
    if bytes.len() as u64 != declared_size {
        return Err(invalid("Contents length differs from declared stream size"));
    }
    if bytes.len() < 0x24 || !crate::format::has_drawing_identifier(&bytes, !unicode) {
        if anonymous {
            // Other programs name a stream Contents too; without the class
            // identifier this is simply not a drawing.
            info.diagnostics.push(Diagnostic {
                code: "unrecognized_clsid",
                message: "CFB root has no class identifier and Contents is not a drawing".into(),
            });
            return Ok(info);
        }
        return Err(invalid(
            "Contents has no complete supported DRA-CAD identifier/header prefix",
        ));
    }
    info.format = DracadFormat::Mpz;
    info.payload = Some(PayloadInfo {
        domain: crate::PayloadOffsetDomain::CfbStream,
        stream_path: Some(path),
        size: declared_size,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        marker_raw: u16::from_le_bytes([bytes[0x22], bytes[0x23]]),
    });
    info.contents = Some(Arc::from(bytes));
    info.diagnostics.push(Diagnostic { code: "geometry_not_decoded", message: "MPZ container recognized; marker meaning, geometry and coordinate systems are not decoded".into() });
    if info
        .cfb
        .as_ref()
        .ok_or_else(|| invalid("missing CFB metadata"))?
        .entries
        .len()
        > 1
    {
        info.diagnostics.push(Diagnostic {
            code: "additional_cfb_entries",
            message: "Additional CFB entries are inventoried, not interpreted or executed".into(),
        });
    }
    Ok(info)
}

/// Inspect a regular file with metadata and actual-byte limits before parsing.
pub fn inspect_file(path: &Path, limits: &ParseLimits) -> Result<ContainerInfo> {
    inspect(&read_file_limited(path, limits)?, limits)
}

/// Read a regular file, checking its metadata and its actual bytes against the limit.
pub(crate) fn read_file_limited(path: &Path, limits: &ParseLimits) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(non_file());
    }
    limits.check_file_size(metadata.len())?;
    let mut file = File::open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() {
        return Err(non_file());
    }
    limits.check_file_size(opened.len())?;
    read_limited(&mut file, limits.max_file_size, ReadDomain::File)
}

fn non_file() -> DracadError {
    DracadError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "input must be a regular file",
    ))
}

#[derive(Clone, Copy)]
enum ReadDomain {
    File,
    Contents,
}

fn read_limited(reader: &mut impl Read, limit: u64, domain: ReadDomain) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut capped = reader.take(limit.saturating_add(1));
    let mut buffer = [0; 8192];
    loop {
        let count = match capped.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if matches!(domain, ReadDomain::File) => return Err(DracadError::Io(error)),
            Err(error) => return Err(invalid(error.to_string())),
        };
        let next = (output.len() as u64)
            .checked_add(count as u64)
            .ok_or_else(|| invalid("input byte count overflow"))?;
        if next > limit {
            if matches!(domain, ReadDomain::File) {
                return Err(DracadError::LimitExceeded {
                    resource: "file_size",
                    limit,
                    actual: next,
                });
            }
            return Err(invalid("Contents produced more bytes than declared"));
        }
        output.try_reserve(count).map_err(|error| {
            invalid(format!("unable to allocate bounded input buffer: {error}"))
        })?;
        output.extend_from_slice(&buffer[..count]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn actual_bytes_are_limited_without_relying_on_file_metadata() {
        let error =
            read_limited(&mut Cursor::new(b"growing input"), 2, ReadDomain::File).unwrap_err();
        assert!(matches!(
            error,
            DracadError::LimitExceeded {
                resource: "file_size",
                limit: 2,
                ..
            }
        ));
    }
}
