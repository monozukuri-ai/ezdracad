//! Container-level identification. No record/geometry/version interpretation.

use std::sync::Arc;

use crate::{CfbInfo, DracadError, Result};

pub const DRACAD_CLSID: &str = "0d4c9b60-7843-11cf-1234-56789abcdeff";
pub(crate) const DRAWING_MAGIC: &[u8] = b"DRACAD for Windows";

/// Whether `bytes` begin with the drawing identifier.
///
/// The identifier is a terminated string in a 32-byte field. Most drawings
/// zero the rest of the field, which `strict` requires; saved drawings exist
/// where the bytes after the terminator are leftovers.
pub(crate) fn has_drawing_identifier(bytes: &[u8], strict: bool) -> bool {
    bytes.len() >= 32
        && bytes.starts_with(DRAWING_MAGIC)
        && bytes[DRAWING_MAGIC.len()] == 0
        && (!strict || bytes[DRAWING_MAGIC.len()..32].iter().all(|byte| *byte == 0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DracadFormat {
    Mpz,
    Mpx,
    Mps,
    /// Unicode drawing format. Only the best-effort reader produces it; the
    /// other entry points report such a file as `MpsCandidate`.
    Mpuw,
    MpsCandidate,
    UnknownCfb,
}

impl DracadFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mpz => "mpz",
            Self::Mpx => "mpx",
            Self::Mps => "mps",
            Self::Mpuw => "mpuw",
            Self::MpsCandidate => "mps_candidate",
            Self::UnknownCfb => "unknown_cfb",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
}

/// Coordinate system for byte offsets, independent of the carrier stream name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadOffsetDomain {
    File,
    CfbStream,
    DecodedPayload,
}
impl PayloadOffsetDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::CfbStream => "cfb_stream",
            Self::DecodedPayload => "decoded_payload",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadInfo {
    pub domain: PayloadOffsetDomain,
    pub stream_path: Option<String>,
    pub size: u64,
    pub sha256: String,
    /// Raw field at Contents-relative offset 0x22, not an application version.
    pub marker_raw: u16,
}

impl PayloadInfo {
    /// MPS offsets refer to decoded bytes, never compressed stream positions.
    pub fn offset_domain(&self) -> &'static str {
        self.domain.as_str()
    }
}

/// Successful container inspection is not successful drawing decoding.
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub format: DracadFormat,
    pub file_size: u64,
    pub cfb: Option<CfbInfo>,
    pub payload: Option<PayloadInfo>,
    pub mps: Option<crate::MpsInfo>,
    pub diagnostics: Vec<Diagnostic>,
    pub(crate) contents: Option<Arc<[u8]>>,
}

impl ContainerInfo {
    pub(crate) fn contents_arc(&self) -> Result<Arc<[u8]>> {
        self.contents.clone().ok_or_else(|| {
            DracadError::UnsupportedFormat(format!(
                "{} has no supported drawing payload",
                self.format.as_str()
            ))
        })
    }
    /// Exact MPZ Contents, original flat MPX, or decoded MPS bytes; no rewritten header/footer.
    pub fn contents(&self) -> Result<&[u8]> {
        self.contents.as_deref().ok_or_else(|| {
            DracadError::UnsupportedFormat(format!(
                "{} has no supported drawing payload",
                self.format.as_str()
            ))
        })
    }
}
