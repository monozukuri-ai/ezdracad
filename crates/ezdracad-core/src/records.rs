//! Immutable, bounded raw record access. Candidate XY is not normalized geometry.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::header::parse_header;
use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{
    inspect, inspect_file, ContainerInfo, Diagnostic, ParseLimits, PayloadHeader, Result,
    RECORD_SIZE,
};

pub const MAX_RECORDS_PER_PAGE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineCandidate2D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub start: [f64; 2],
    pub end: [f64; 2],
}

/// Decoded fields of one explicitly recognized 2D line record.
///
/// Coordinates are record-local. Units, parent scope, transforms and the meaning
/// of header/flag bytes remain unresolved; this is not a normalized drawing
/// entity and does not establish document completeness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineRecord2D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub tag_raw: u16,
    /// Exact first 16 bytes, including the tag; no attributes are applied.
    pub header_bytes: [u8; 16],
    pub start: [f64; 2],
    pub end: [f64; 2],
    /// Common height, not the start endpoint's independent Z coordinate.
    pub elevation: f64,
    /// Thickness, not the end endpoint's Z coordinate.
    pub thickness: f64,
}

impl LineRecord2D {
    /// No parent, origin, unit or placement transform has been applied.
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "record_local"
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RawRecord<'a> {
    pub index: u64,
    pub payload_offset: u64,
    pub tag: u16,
    bytes: &'a [u8],
}

impl<'a> RawRecord<'a> {
    /// Decode an explicit observed current-format header, without a file profile.
    /// Use document methods when old-profile dependency guards are required.
    pub fn record_attributes(&self) -> Result<crate::RecordAttributes> {
        crate::decode_record_attributes(&self.bytes[..16])
    }

    /// Recognize only the exact observed explicit/ByLayer/ByBlock line tags.
    /// This standalone record method has no profile; document methods add it.
    pub fn attributed_line_record(&self) -> Result<Option<crate::AttributedLineRecord2D>> {
        if !matches!(self.tag, 2 | 0x402 | 0x2402 | 0x4402) {
            return Ok(None);
        }
        Ok(Some(crate::AttributedLineRecord2D {
            attributes: self.record_attributes()?,
            source: self.decode_line_fields()?,
        }))
    }

    pub(crate) fn at(payload: &'a [u8], index: u64, offset: u64) -> Option<Self> {
        let start = usize::try_from(offset).ok()?;
        let bytes = payload.get(start..start.checked_add(64)?)?;
        Some(Self {
            index,
            payload_offset: offset,
            tag: u16::from_le_bytes([bytes[0], bytes[1]]),
            bytes,
        })
    }
    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Only tag 0x0002; units, Z, attributes, hierarchy and transforms are unresolved.
    #[must_use]
    pub fn line_candidate(&self) -> Option<LineCandidate2D> {
        if self.tag != 2 {
            return None;
        }
        let mut xy = [0.0; 4];
        for (value, field) in xy.iter_mut().zip([16, 24, 32, 40]) {
            *value = f64::from_le_bytes(self.bytes.get(field..field + 8)?.try_into().ok()?);
            if !value.is_finite() {
                return None;
            }
        }
        Some(LineCandidate2D {
            record_index: self.index,
            payload_offset: self.payload_offset,
            start: [xy[0], xy[1]],
            end: [xy[2], xy[3]],
        })
    }

    /// Decode only exact tags 0x0002 and 0x0402, without masking flags.
    ///
    /// All six fields must be finite when this method is requested. Raw scanning
    /// and the older XY-candidate API retain their separate validation contract.
    pub fn line_record(&self) -> Result<Option<LineRecord2D>> {
        if !matches!(self.tag, 0x0002 | 0x0402) {
            return Ok(None);
        }
        self.decode_line_fields().map(Some)
    }

    fn decode_line_fields(&self) -> Result<LineRecord2D> {
        let mut fields = [0.0; 6];
        for (value, field) in fields.iter_mut().zip([16, 24, 32, 40, 48, 56]) {
            let bytes = self
                .bytes
                .get(field..field + 8)
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| invalid("truncated 2D line record field"))?;
            *value = f64::from_le_bytes(bytes);
            if !value.is_finite() {
                return Err(invalid(format!(
                    "non-finite 2D line field +{field} in record {} at Contents offset {}",
                    self.index, self.payload_offset
                )));
            }
        }
        let header_bytes = self
            .bytes
            .get(..16)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| invalid("truncated 2D line record header"))?;
        Ok(LineRecord2D {
            record_index: self.index,
            payload_offset: self.payload_offset,
            tag_raw: self.tag,
            header_bytes,
            start: [fields[0], fields[1]],
            end: [fields[2], fields[3]],
            elevation: fields[4],
            thickness: fields[5],
        })
    }
}

#[derive(Debug, Clone)]
pub struct RawDocument {
    payload: Arc<[u8]>,
    header: PayloadHeader,
    source: Option<ContainerInfo>,
    sha256: String,
    tag_counts: BTreeMap<u16, u64>,
    line_count: u64,
    diagnostics: Vec<Diagnostic>,
    max_records: u64,
}

impl RawDocument {
    /// Opt into an explicit, versioned planar stored-XY line contract.
    pub fn qualified_lines(
        &self,
        index: u64,
        profile: crate::Line2DProfile,
        limits: &crate::SceneLimits,
    ) -> Result<crate::QualifiedLineSelection> {
        crate::qualify_lines(self, index, profile, limits)
    }

    /// Read qualified marker-25 saved print fields, retaining unknown raw values.
    pub fn print_settings(&self) -> Result<crate::RawPrintSettings> {
        crate::RawPrintSettings::parse(self)
    }

    /// Traverse one primary admitted planar subtree, preserving every placement and style source.
    pub fn scene_lines(
        &self,
        index: u64,
        limits: &crate::SceneLimits,
    ) -> Result<Vec<crate::SceneLine2D>> {
        crate::scene::decode(self, index, limits)
    }

    /// Opt into the marker25 transparent-inline C/T/W inheritance extension.
    ///
    /// Known inline wrappers retain source bytes but create no new insertion
    /// scope or placement. Unknown variants still fail the complete request.
    pub fn scene_lines_with_inline_scopes(
        &self,
        index: u64,
        limits: &crate::SceneLimits,
    ) -> Result<Vec<crate::SceneLine2D>> {
        crate::scene::decode_with_inline_scopes(self, index, limits)
    }

    /// Explicit current-annotation/paper length context, not entity units.
    pub fn coordinate_measurement_context(&self) -> Result<crate::CoordinateMeasurementContext> {
        crate::coordinate_measurement::decode(self.contents(), self.header.profile)
    }

    /// Resolve native property indices for one primary root line only.
    pub fn effective_line_style(&self, index: u64) -> Result<crate::EffectiveLineStyle> {
        crate::effective_styles::effective_line_style(self, index)
    }

    /// File-local header references, without resolving an effective style.
    pub fn record_attributes(&self, index: u64) -> Result<Option<crate::RecordAttributes>> {
        self.record(index)
            .map(|record| {
                let attributes = record.record_attributes()?;
                attributes.ensure_profile(self.header.profile)?;
                Ok(attributes)
            })
            .transpose()
    }

    /// Known colored line variants, preserving file-local reference selectors.
    /// Old-profile dependency encodings and all unknown flags remain unsupported.
    pub fn attributed_line_record(
        &self,
        index: u64,
    ) -> Result<Option<crate::AttributedLineRecord2D>> {
        self.record(index)
            .map(|record| {
                let line = record.attributed_line_record()?;
                if let Some(line) = &line {
                    line.attributes.ensure_profile(self.header.profile)?;
                }
                Ok(line)
            })
            .transpose()
            .map(Option::flatten)
    }

    pub fn attributed_line_records(
        &self,
    ) -> impl Iterator<Item = Result<crate::AttributedLineRecord2D>> + '_ {
        (0..self.header.record_count)
            .filter_map(|index| self.attributed_line_record(index).transpose())
    }

    /// Decode a selected primary inline package without applying any transform.
    pub fn inline_package_lines(&self, index: u64) -> Result<Vec<crate::ScopedInlineLine2D>> {
        self.inline_package_lines_with_limits(index, &crate::InlinePackageLimits::default())
    }

    pub fn inline_package_lines_with_limits(
        &self,
        index: u64,
        limits: &crate::InlinePackageLimits,
    ) -> Result<Vec<crate::ScopedInlineLine2D>> {
        crate::inline_packages::decode(
            self.contents(),
            self.header.record_start,
            self.header.record_count,
            index,
            crate::inline_packages::TableIdentity {
                domain: crate::InlineTableDomain::RootRecords,
                definition_index: None,
                profile: self.header.profile,
            },
            limits,
        )
    }

    /// Decode observed origin/paper-scale metadata without changing coordinates.
    /// Unknown header fields and physical units remain unresolved.
    pub fn drawing_settings(&self) -> Result<crate::DrawingSettings> {
        crate::drawing_settings::decode(self.contents())
    }

    /// Read the qualified text-size selector without resolving geometric units.
    pub fn text_size_unit(&self) -> Result<crate::TextSizeUnitInfo> {
        crate::text_units::decode(self.contents(), self.header.profile)
    }

    /// Parse observed definition-table bounds without resolving drawing scope.
    pub fn block_library(&self) -> Result<crate::BlockLibrary> {
        self.block_library_with_limit(crate::DEFAULT_MAX_BLOCK_DEFINITIONS)
    }

    pub fn block_library_with_limit(&self, max_definitions: u64) -> Result<crate::BlockLibrary> {
        crate::BlockLibrary::parse(self.clone(), self.max_records, max_definitions)
    }
    #[must_use]
    pub fn header(&self) -> &PayloadHeader {
        &self.header
    }
    #[must_use]
    pub fn container(&self) -> Option<&ContainerInfo> {
        self.source.as_ref()
    }
    #[must_use]
    pub fn contents(&self) -> &[u8] {
        &self.payload
    }
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    #[must_use]
    pub fn tag_counts(&self) -> &BTreeMap<u16, u64> {
        &self.tag_counts
    }
    #[must_use]
    pub fn line_candidate_count(&self) -> u64 {
        self.line_count
    }
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    #[must_use]
    pub fn opaque_record_count(&self) -> u64 {
        self.header.record_count - self.line_count - 1
    }

    #[must_use]
    pub fn record(&self, index: u64) -> Option<RawRecord<'_>> {
        if index >= self.header.record_count {
            return None;
        }
        let offset = self
            .header
            .record_start
            .checked_add(index.checked_mul(RECORD_SIZE)?)?;
        let start = usize::try_from(offset).ok()?;
        let bytes = self
            .payload
            .get(start..start.checked_add(RECORD_SIZE as usize)?)?;
        Some(RawRecord {
            index,
            payload_offset: offset,
            tag: u16::from_le_bytes([bytes[0], bytes[1]]),
            bytes,
        })
    }

    pub fn records(&self) -> impl Iterator<Item = RawRecord<'_>> {
        (0..self.header.record_count).filter_map(|index| self.record(index))
    }

    /// Walk one-level primary spans in the root raw table, excluding its terminal.
    /// This leaves span interiors, logical membership and coordinates unresolved.
    /// Consume every fallible item to validate the whole table; errors end the walk.
    #[must_use]
    pub fn object_spans(&self) -> crate::ObjectSpans<'_> {
        crate::ObjectSpans::new(self)
    }

    pub fn line_candidates(&self) -> impl Iterator<Item = LineCandidate2D> + '_ {
        self.records().filter_map(|record| record.line_candidate())
    }

    /// Recognized record-local fields, retaining table order and duplicates.
    /// Unknown tags remain opaque; errors are produced on demand, not at scan.
    pub fn line_records(&self) -> impl Iterator<Item = Result<LineRecord2D>> + '_ {
        self.records()
            .filter_map(|record| record.line_record().transpose())
    }

    /// Decode an exact two-slot reference payload, without logical-root claims.
    /// The following continuation may be outside a caller's requested page.
    pub fn package_reference(&self, index: u64) -> Result<Option<crate::PackageReference2D>> {
        let Some(primary) = self.record(index) else {
            return Ok(None);
        };
        let continuation = index.checked_add(1).and_then(|next| self.record(next));
        crate::package_refs::decode_reference(primary, continuation, self.header.record_count)
    }

    /// Root-slot payload enumeration; unknown flags/tags remain opaque.
    pub fn package_references(
        &self,
    ) -> impl Iterator<Item = Result<crate::PackageReference2D>> + '_ {
        (0..self.header.record_count).filter_map(|index| self.package_reference(index).transpose())
    }
}

fn validate(
    payload: Arc<[u8]>,
    source: Option<ContainerInfo>,
    sha256: String,
    limits: &ParseLimits,
) -> Result<RawDocument> {
    let header = if source
        .as_ref()
        .is_some_and(|s| s.format == crate::DracadFormat::Mpx)
    {
        crate::header::parse_mpx_header(&payload, limits)?
    } else {
        parse_header(&payload, limits)?
    };
    let mut doc = RawDocument {
        payload,
        header,
        source,
        sha256,
        tag_counts: BTreeMap::new(),
        line_count: 0,
        diagnostics: Vec::new(),
        max_records: limits.max_records,
    };
    // No per-record allocation; offsets refer into the one immutable payload.
    for index in 0..doc.header.record_count {
        let record = doc
            .record(index)
            .ok_or_else(|| invalid("validated record boundary is unavailable"))?;
        let tag = record.tag;
        let terminal_has_data = index + 1 == doc.header.record_count
            && record.bytes()[2..].iter().any(|byte| *byte != 0);
        if index + 1 == doc.header.record_count {
            if tag != 0xff {
                return Err(invalid("missing terminal record"));
            }
        } else if tag == 0xff {
            return Err(invalid("terminal tag appears before the record table end"));
        }
        if tag == 2 {
            if record.line_candidate().is_none() {
                return Err(invalid(format!(
                    "non-finite XY in candidate record {index} at Contents offset {}",
                    record.payload_offset
                )));
            }
            doc.line_count += 1;
        }
        if terminal_has_data {
            doc.diagnostics.push(Diagnostic {
                code: "terminal_data_uninterpreted",
                message: "Nonzero terminal body is retained without interpretation".into(),
            });
        }
        *doc.tag_counts.entry(tag).or_default() += 1;
    }
    doc.diagnostics.push(Diagnostic {
        code: "observed_record_profile",
        message: format!(
            "{} is an observed layout, not an application version",
            doc.header.profile.as_str()
        ),
    });
    doc.diagnostics.push(Diagnostic { code: "line_candidates_only", message: "Tag 0x0002 XY is provisional; units, Z, attributes, hierarchy, transforms and geometry meaning are unresolved".into() });
    doc.diagnostics.push(Diagnostic { code: "uninterpreted_regions", message: "Header fields and post-record regions are retained; raw records do not establish drawing completeness".into() });
    Ok(doc)
}

fn scan_container(source: ContainerInfo, limits: &ParseLimits) -> Result<RawDocument> {
    let payload = source.contents_arc()?;
    let sha256 = source
        .payload
        .as_ref()
        .ok_or_else(|| invalid("drawing payload metadata is absent"))?
        .sha256
        .clone();
    validate(payload, Some(source), sha256, limits)
}

/// Scan a supported MPZ container or observed flat MPX, identified by bytes.
pub fn scan_records(data: &[u8], limits: &ParseLimits) -> Result<RawDocument> {
    scan_container(inspect(data, limits)?, limits)
}

pub fn scan_file(path: &Path, limits: &ParseLimits) -> Result<RawDocument> {
    scan_container(inspect_file(path, limits)?, limits)
}

/// Low-level Contents-domain API, useful for independent tests/fuzzing.
/// It does not identify the bytes as an MPX file or a native DRA-CAD document.
pub fn scan_contents(data: &[u8], limits: &ParseLimits) -> Result<RawDocument> {
    check_limit("stream_size", data.len() as u64, limits.max_stream_size)?;
    validate(
        Arc::from(data),
        None,
        format!("{:x}", Sha256::digest(data)),
        limits,
    )
}
