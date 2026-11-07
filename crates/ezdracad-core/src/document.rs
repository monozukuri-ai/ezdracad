//! Read-only, explicitly partial documents built from the supported stored-XY decoder.

use std::collections::BTreeMap;
use std::path::Path;

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{
    DracadError, InlineTableDomain, Line2D, Line2DProfile, ParseLimits, RawDocument, RawObjectSpan,
    RawRecord, Result, SceneLimits,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPolicy {
    Strict,
    Partial,
}
impl ReadPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Partial => "partial",
        }
    }
    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "strict" => Ok(Self::Strict),
            "partial" => Ok(Self::Partial),
            _ => Err(DracadError::UnsupportedFeature(
                "unknown read policy".into(),
            )),
        }
    }
}

/// Drawing requires resolved drawing meaning. StoredXyLines explicitly requests
/// only the stored-XY contract, with unknown units/visibility/WCS retained as unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadScope {
    Drawing,
    StoredXyLines,
    StoredXyPrimitives,
}
impl ReadScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Drawing => "drawing",
            Self::StoredXyLines => "stored_xy_lines",
            Self::StoredXyPrimitives => "stored_xy_primitives",
        }
    }
    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "drawing" => Ok(Self::Drawing),
            "stored_xy_lines" => Ok(Self::StoredXyLines),
            "stored_xy_primitives" => Ok(Self::StoredXyPrimitives),
            _ => Err(DracadError::UnsupportedFeature("unknown read scope".into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadStatus {
    Complete,
    Partial,
}
impl ReadStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
        }
    }
}

/// Aggregate budgets across every selection, including failed traversal work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    pub max_objects: u64,
    pub max_entities: u64,
    pub max_nodes: u64,
    pub max_provenance_bytes: u64,
    pub max_diagnostics: u64,
}
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_objects: 100_000,
            max_entities: 100_000,
            max_nodes: 1_000_000,
            max_provenance_bytes: crate::MAX_SCENE_PROVENANCE_BYTES,
            max_diagnostics: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOptions {
    pub policy: ReadPolicy,
    pub scope: ReadScope,
    pub line_profile: Line2DProfile,
    pub basic_profile: Option<crate::Basic2DProfile>,
    pub text_limits: crate::TextLimits,
    pub parse_limits: ParseLimits,
    pub scene_limits: SceneLimits,
    pub read_limits: ReadLimits,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            policy: ReadPolicy::Strict,
            scope: ReadScope::Drawing,
            line_profile: Line2DProfile::Marker25PlanarStoredXyV1,
            basic_profile: None,
            text_limits: crate::TextLimits::default(),
            parse_limits: ParseLimits::default(),
            scene_limits: SceneLimits::default(),
            read_limits: ReadLimits::default(),
        }
    }
}

/// A range in an MPZ stream, flat MPX file, or decoded MPS payload.
/// Always inspect offset_domain(); stream_path is None for MPX and decoded MPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub domain: crate::PayloadOffsetDomain,
    pub stream_path: Option<String>,
    pub table_domain: InlineTableDomain,
    pub definition_index: Option<u64>,
    pub record_index: u64,
    pub payload_offset: u64,
    pub byte_length: u64,
}
impl SourceRef {
    pub fn offset_domain(&self) -> &'static str {
        self.domain.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueRegion {
    pub domain: crate::PayloadOffsetDomain,
    pub stream_path: Option<String>,
    pub payload_offset: u64,
    pub byte_length: u64,
}

impl OpaqueRegion {
    pub fn offset_domain(&self) -> &'static str {
        self.domain.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadReport {
    pub domain: crate::PayloadOffsetDomain,
    pub status: ReadStatus,
    pub geometry_status: ReadStatus,
    pub document_status: ReadStatus,
    pub policy: ReadPolicy,
    pub scope: ReadScope,
    pub line_profile: Line2DProfile,
    pub basic_profile: Option<crate::Basic2DProfile>,
    pub contents_sha256: String,
    pub source_format: crate::DracadFormat,
    pub stream_path: Option<String>,
    pub raw_record_count: u64,
    pub primary_object_count: u64,
    pub interior_record_count: u64,
    /// Raw exact-FE slot count, not a count of all non-geometric metadata.
    pub auxiliary_record_count: u64,
    /// Exact recognized non-geometric root metadata, retained in raw().
    pub handled_auxiliary_object_count: u64,
    pub decoded_object_count: u64,
    pub entity_count: u64,
    pub unsupported_counts: BTreeMap<u16, u64>,
    pub context_blocked_counts: BTreeMap<u16, u64>,
    pub diagnostics: Vec<ReadDiagnostic>,
    pub diagnostics_suppressed: u64,
    pub opaque_regions: Vec<OpaqueRegion>,
    pub definition_structure_validated: bool,
    pub scene_nodes_visited: u64,
}
impl ReadReport {
    pub fn offset_domain(&self) -> &'static str {
        self.domain.as_str()
    }
    /// Carrier-neutral alias; contents_sha256 is retained for compatibility.
    pub fn payload_sha256(&self) -> &str {
        &self.contents_sha256
    }

    pub fn requested_entity_types(&self) -> &'static [&'static str] {
        if self.basic_profile.is_some() {
            &["LINE", "CIRCLE", "ARC", "TEXT"]
        } else {
            &["LINE"]
        }
    }
    pub const fn document_complete(&self) -> bool {
        false
    }
    pub const fn coordinate_semantics(&self) -> &'static str {
        "stored_xy"
    }
    pub const fn units(&self) -> Option<&'static str> {
        None
    }
    pub const fn unresolved_semantics(&self) -> &'static [crate::UnresolvedLineSemantics] {
        &crate::LINE2D_UNRESOLVED_SEMANTICS
    }
    fn diagnostic(
        &mut self,
        options: &ReadOptions,
        code: &'static str,
        message: String,
        source: Option<SourceRef>,
    ) {
        if (self.diagnostics.len() as u64) < options.read_limits.max_diagnostics {
            self.diagnostics.push(ReadDiagnostic {
                code,
                message,
                source,
            });
        } else {
            self.diagnostics_suppressed += 1;
        }
    }
}

/// Strict rejection keeps the same typed cause as partial's unsupported report.
/// Fatal parse/IO/limit errors have no successful partial Document attached.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct ReadError {
    #[source]
    error: DracadError,
    report: Option<Box<ReadReport>>,
}
impl From<DracadError> for ReadError {
    fn from(error: DracadError) -> Self {
        Self {
            error,
            report: None,
        }
    }
}
impl ReadError {
    pub fn error(&self) -> &DracadError {
        &self.error
    }
    pub fn report(&self) -> Option<&ReadReport> {
        self.report.as_deref()
    }
    pub fn into_parts(self) -> (DracadError, Option<Box<ReadReport>>) {
        (self.error, self.report)
    }
}
pub type ReadResult<T> = std::result::Result<T, ReadError>;

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentLine {
    pub entity_id: u64,
    pub root_record_index: u64,
    pub source_ref: SourceRef,
    pub geometry: Line2D,
}
impl DocumentLine {
    pub const fn entity_type(&self) -> &'static str {
        "LINE"
    }
}

/// A root-only primitive; styles are native file-local explicit indices.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentPrimitive<T> {
    pub entity_id: u64,
    pub root_record_index: u64,
    pub source_ref: SourceRef,
    pub geometry: T,
    pub style: crate::RootPrimitiveStyle,
}
pub type DocumentCircle = DocumentPrimitive<crate::Circle2D>;
pub type DocumentArc = DocumentPrimitive<crate::Arc2D>;
pub type DocumentText = DocumentPrimitive<crate::TextRecord2D>;

#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
    Line(DocumentLine),
    Circle(DocumentCircle),
    Arc(DocumentArc),
    Text(DocumentText),
}
impl Entity {
    pub const fn entity_type(&self) -> &'static str {
        match self {
            Self::Line(_) => "LINE",
            Self::Circle(_) => "CIRCLE",
            Self::Arc(_) => "ARC",
            Self::Text(_) => "TEXT",
        }
    }
    pub fn entity_id(&self) -> u64 {
        match self {
            Self::Line(e) => e.entity_id,
            Self::Circle(e) => e.entity_id,
            Self::Arc(e) => e.entity_id,
            Self::Text(e) => e.entity_id,
        }
    }
    pub fn root_record_index(&self) -> u64 {
        match self {
            Self::Line(e) => e.root_record_index,
            Self::Circle(e) => e.root_record_index,
            Self::Arc(e) => e.root_record_index,
            Self::Text(e) => e.root_record_index,
        }
    }
    pub fn source_ref(&self) -> &SourceRef {
        match self {
            Self::Line(e) => &e.source_ref,
            Self::Circle(e) => &e.source_ref,
            Self::Arc(e) => &e.source_ref,
            Self::Text(e) => &e.source_ref,
        }
    }
    pub fn as_line(&self) -> Option<&DocumentLine> {
        if let Self::Line(e) = self {
            Some(e)
        } else {
            None
        }
    }
    fn styles(&self) -> &crate::EffectiveStyleIndices {
        match self {
            Self::Line(e) => &e.geometry.scene().styles,
            Self::Circle(e) => &e.style.styles,
            Self::Arc(e) => &e.style.styles,
            Self::Text(e) => &e.style.styles,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    raw: RawDocument,
    report: ReadReport,
    entities: Vec<Entity>,
}
impl Document {
    pub fn report(&self) -> &ReadReport {
        &self.report
    }
    pub fn raw(&self) -> &RawDocument {
        &self.raw
    }
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
    pub fn query_count(&self, entity_type: &str) -> Result<u64> {
        self.check_query(entity_type)?;
        Ok(self
            .entities
            .iter()
            .filter(|e| entity_type == "*" || e.entity_type() == entity_type)
            .count() as u64)
    }
    fn check_query(&self, entity_type: &str) -> Result<()> {
        if entity_type != "*" && !self.report.requested_entity_types().contains(&entity_type) {
            return Err(DracadError::UnsupportedFeature(
                "query type is outside selected profile; unavailable types are not known absent"
                    .into(),
            ));
        }
        Ok(())
    }
    /// Pages are relative to the selected type; entity IDs remain global ordinals.
    pub fn query(&self, entity_type: &str, start: u64, count: u64) -> Result<Vec<&Entity>> {
        let total = self.query_count(entity_type)?;
        check_limit("entities_per_page", count, crate::MAX_RECORDS_PER_PAGE)?;
        if start > total {
            return Err(invalid("entity page start exceeds decoded count"));
        }
        Ok(self
            .entities
            .iter()
            .filter(|e| entity_type == "*" || e.entity_type() == entity_type)
            .skip(start as usize)
            .take(count as usize)
            .collect())
    }
}

pub fn read(data: &[u8], options: &ReadOptions) -> ReadResult<Document> {
    read_raw(crate::scan_records(data, &options.parse_limits)?, options)
}
pub fn read_file(path: &Path, options: &ReadOptions) -> ReadResult<Document> {
    read_raw(crate::scan_file(path, &options.parse_limits)?, options)
}

fn finite(record: RawRecord<'_>, offsets: &[usize]) -> Result<()> {
    for &at in offsets {
        let bytes = record
            .bytes()
            .get(at..at + 8)
            .ok_or_else(|| invalid("truncated known numeric field"))?;
        let value = f64::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| invalid("truncated numeric field"))?,
        );
        if !value.is_finite() {
            return Err(invalid(format!(
                "non-finite known field +{at} at Contents offset {}",
                record.payload_offset
            )));
        }
    }
    Ok(())
}

/// Validate known structural/numeric fields even in later held or unused ranges.
/// Unknown compound interiors stay opaque; this is not arbitrary resynchronizing.
fn validate_table(
    contents: &[u8],
    table_start: u64,
    count: u64,
    profile: crate::ObservedProfile,
) -> Result<()> {
    let mut pending = vec![(0, count.saturating_sub(1))];
    while let Some((mut index, end)) = pending.pop() {
        while index < end {
            let offset = table_start
                .checked_add(
                    index
                        .checked_mul(64)
                        .ok_or_else(|| invalid("record offset overflow"))?,
                )
                .ok_or_else(|| invalid("record offset overflow"))?;
            let r = RawRecord::at(contents, index, offset)
                .ok_or_else(|| invalid("record outside validated table"))?;
            if matches!(r.tag, 0xfe | 0xff) {
                return Err(invalid(
                    "orphan continuation or terminal in primary structure",
                ));
            }
            let span = u64::from(u16::from_le_bytes([r.bytes()[8], r.bytes()[9]]));
            let next = index
                .checked_add(span)
                .filter(|n| span != 0 && *n <= end)
                .ok_or_else(|| invalid("zero or crossing primary span"))?;
            match r.tag {
                2 | 0x402 | 0x2402 | 0x4402 => finite(r, &[16, 24, 32, 40, 48, 56])?,
                0x13 | 0x413 if profile != crate::ObservedProfile::Marker8 => {
                    finite(r, &[16, 24, 32, 40, 48, 56])?
                }
                6 | 0x2006 | 0x4006 | 0x406
                    if (span == 2 || span == 3 && profile == crate::ObservedProfile::Marker25)
                        && (r.tag != 0x406 || profile == crate::ObservedProfile::Marker25) =>
                {
                    finite(r, &[16, 24])?;
                    for child in index + 1..next {
                        let tail = RawRecord::at(contents, child, table_start + child * 64)
                            .ok_or_else(|| invalid("missing reference continuation"))?;
                        if tail.tag != 0xfe {
                            return Err(invalid("known reference lacks FE continuation"));
                        }
                        finite(
                            tail,
                            if child == index + 1 {
                                &[16, 24, 32, 40, 48, 56]
                            } else {
                                &[16, 24, 32]
                            },
                        )?;
                    }
                }
                0x30 | 0x430 | 0x2030 | 0x4030
                    if span >= 2
                        && r.bytes()[16..20] == 0x400u32.to_le_bytes()
                        && r.bytes()[20..].iter().all(|b| *b == 0) =>
                {
                    // Iterative descent bounds stack and preserves parent ends.
                    pending.push((index + 1, next));
                }
                0x33 => {
                    for child in index + 1..next {
                        if RawRecord::at(contents, child, table_start + child * 64)
                            .ok_or_else(|| invalid("missing metadata continuation"))?
                            .tag
                            != 0xfe
                        {
                            return Err(invalid("known metadata lacks FE continuation"));
                        }
                    }
                }
                _ => {}
            }
            index = next;
        }
    }
    Ok(())
}

fn root_source(
    span: &RawObjectSpan,
    stream: &Option<String>,
    domain: crate::PayloadOffsetDomain,
) -> SourceRef {
    SourceRef {
        domain,
        stream_path: stream.clone(),
        table_domain: InlineTableDomain::RootRecords,
        definition_index: None,
        record_index: span.record_index,
        payload_offset: span.payload_offset,
        byte_length: span.record_count * 64,
    }
}

fn read_raw(raw: RawDocument, options: &ReadOptions) -> ReadResult<Document> {
    if (options.scope == ReadScope::StoredXyPrimitives && options.basic_profile.is_none())
        || (options.scope == ReadScope::StoredXyLines && options.basic_profile.is_some())
    {
        return Err(DracadError::UnsupportedFeature("stored_xy_primitives requires basic_profile; stored_xy_lines retains its LINE-only contract".into()).into());
    }
    check_limit(
        "scene_nodes_limit",
        options.scene_limits.max_nodes,
        crate::MAX_RECORDS_PER_PAGE,
    )?;
    check_limit(
        "scene_lines_limit",
        options.scene_limits.max_lines,
        crate::MAX_RECORDS_PER_PAGE,
    )?;
    // Validate the entire root before retaining any output, not only its prefix.
    let mut spans = Vec::new();
    for span in raw.object_spans() {
        let span = span?;
        check_limit(
            "document_objects",
            spans.len() as u64 + 1,
            options.read_limits.max_objects,
        )?;
        spans.push(span);
    }
    validate_table(
        raw.contents(),
        raw.header().record_start,
        raw.header().record_count,
        raw.header().profile,
    )?;
    if let Some(profile) = options.basic_profile {
        for span in &spans {
            let checked = match span.tag_raw {
                3 | 4 => crate::basic2d::decode_curve_span(&raw, span, profile).map(|_| ()),
                5 => crate::text2d::decode_root_text(&raw, span, &options.text_limits).map(|_| ()),
                _ => Ok(()),
            };
            match checked {
                Ok(()) | Err(DracadError::UnsupportedFeature(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    let library = match raw.block_library() {
        Ok(library) => Some(library),
        Err(DracadError::UnsupportedFeature(_)) => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(library) = &library {
        for definition in library.definitions() {
            validate_table(
                raw.contents(),
                definition.record_start,
                definition.record_count,
                raw.header().profile,
            )?;
        }
    }
    let input = raw
        .container()
        .ok_or_else(|| invalid("Document has no input carrier"))?;
    let source_format = input.format;
    let domain = input
        .payload
        .as_ref()
        .ok_or_else(|| invalid("Document has no payload metadata"))?
        .domain;
    let stream = input
        .payload
        .as_ref()
        .ok_or_else(|| invalid("Document has no payload metadata"))?
        .stream_path
        .clone();
    let mut report = ReadReport {
        domain,
        status: ReadStatus::Partial,
        geometry_status: ReadStatus::Partial,
        document_status: ReadStatus::Partial,
        policy: options.policy,
        scope: options.scope,
        line_profile: options.line_profile,
        basic_profile: options.basic_profile,
        contents_sha256: raw.sha256().into(),
        source_format,
        stream_path: stream.clone(),
        raw_record_count: raw.header().record_count,
        primary_object_count: spans.len() as u64,
        interior_record_count: raw.header().record_count - 1 - spans.len() as u64,
        auxiliary_record_count: raw.tag_counts().get(&0xfe).copied().unwrap_or(0),
        handled_auxiliary_object_count: 0,
        decoded_object_count: 0,
        entity_count: 0,
        unsupported_counts: BTreeMap::new(),
        context_blocked_counts: BTreeMap::new(),
        diagnostics: Vec::new(),
        diagnostics_suppressed: 0,
        definition_structure_validated: library.is_some(),
        scene_nodes_visited: 0,
        opaque_regions: vec![
            OpaqueRegion {
                domain,
                stream_path: stream.clone(),
                payload_offset: 0,
                byte_length: raw.header().record_start,
            },
            OpaqueRegion {
                domain,
                stream_path: stream.clone(),
                payload_offset: raw.header().record_end,
                byte_length: raw.contents().len() as u64 - raw.header().record_end,
            },
        ],
    };
    for diagnostic in raw.diagnostics() {
        report.diagnostic(options, diagnostic.code, diagnostic.message.clone(), None);
    }
    for meaning in crate::LINE2D_UNRESOLVED_SEMANTICS {
        report.diagnostic(
            options,
            "unresolved_semantics",
            meaning.as_str().into(),
            None,
        );
    }
    if !report.definition_structure_validated {
        report.diagnostic(options,"opaque_definition_structure","Unrecognized optional table layout is retained; definition structure is not fully validated".into(),None);
    }
    if let Some(cfb) = raw.container().and_then(|container| container.cfb.as_ref()) {
        for entry in &cfb.entries {
            if Some(&entry.path) != stream.as_ref()
                && input.mps.as_ref().map(|mps| &mps.stream_path) != Some(&entry.path)
            {
                report.diagnostic(
                    options,
                    "uninterpreted_cfb_entry",
                    format!("{} ({:?}, {} bytes)", entry.path, entry.kind, entry.size),
                    None,
                );
            }
        }
    }
    let prepared = if raw.header().profile == crate::ObservedProfile::Marker25 {
        crate::scene::DecodeContext::new(&raw, library, options.read_limits.max_nodes)
    } else {
        Err(DracadError::UnsupportedFeature(
            "Line2D Document requires observed marker25, including empty tables".into(),
        ))
    };
    let mut context = match prepared {
        Ok(context) => Some(context),
        Err(DracadError::UnsupportedFeature(message)) => {
            report.diagnostic(options, "unsupported_decoder_context", message, None);
            None
        }
        Err(error) => return Err(error.into()),
    };
    let mut barrier = context.is_none();
    let mut entities = Vec::new();
    let mut source_bytes = 0u64;
    let mut source_count = 0u64;
    for span in spans {
        let source = root_source(&span, &stream, domain);
        if barrier {
            *report
                .context_blocked_counts
                .entry(span.tag_raw)
                .or_default() += 1;
            report.diagnostic(
                options,
                "context_blocked",
                "Earlier unsupported state prevents automatic recognition of this object".into(),
                Some(source),
            );
            continue;
        }
        if options.basic_profile.is_some() && span.tag_raw == 0x33 {
            if let Some(property) = crate::root_metadata::classify_root_metadata(&raw, &span)? {
                context
                    .as_mut()
                    .ok_or_else(|| invalid("missing metadata context"))?
                    .visit_root()?;
                report.handled_auxiliary_object_count += 1;
                report.diagnostic(
                    options,
                    "recognized_auxiliary_property",
                    format!(
                        "{}; original property bytes retained in raw()",
                        property.kind.as_str()
                    ),
                    Some(source),
                );
                continue;
            }
        }
        // One selected object remains atomic. No supported leaves leak from an
        // unsupported subtree, and unknown state fences all following roots.
        let selected: Result<Vec<Entity>> = (|| {
            let context = context
                .as_mut()
                .ok_or_else(|| invalid("missing document decoder context"))?;
            if let Some(profile) = options.basic_profile {
                if matches!(span.tag_raw, 3 | 4) {
                    context.visit_root()?;
                    let curve = crate::basic2d::decode_curve_span(&raw, &span, profile)?;
                    let style = context.root_style(
                        curve.raw_bytes(),
                        span.record_index,
                        span.payload_offset,
                    )?;
                    let id = entities.len() as u64;
                    return Ok(vec![match curve {
                        crate::Curve2D::Circle(geometry) => Entity::Circle(DocumentPrimitive {
                            entity_id: id,
                            root_record_index: span.record_index,
                            source_ref: source.clone(),
                            geometry,
                            style,
                        }),
                        crate::Curve2D::Arc(geometry) => Entity::Arc(DocumentPrimitive {
                            entity_id: id,
                            root_record_index: span.record_index,
                            source_ref: source.clone(),
                            geometry,
                            style,
                        }),
                    }]);
                }
                if span.tag_raw == 5 {
                    context.visit_root()?;
                    let geometry =
                        crate::text2d::decode_root_text(&raw, &span, &options.text_limits)?;
                    let style = context.root_style(
                        &geometry.raw_bytes,
                        span.record_index,
                        span.payload_offset,
                    )?;
                    return Ok(vec![Entity::Text(DocumentPrimitive {
                        entity_id: entities.len() as u64,
                        root_record_index: span.record_index,
                        source_ref: source.clone(),
                        geometry,
                        style,
                    })]);
                }
            }
            let selection = crate::qualification::qualify_selected(
                &raw,
                &span,
                options.line_profile,
                &options.scene_limits,
                context,
            )?;
            Ok(selection
                .into_lines()
                .into_iter()
                .enumerate()
                .map(|(n, geometry)| {
                    let line = geometry.scene();
                    let source_ref = SourceRef {
                        domain,
                        stream_path: stream.clone(),
                        table_domain: line.table_domain,
                        definition_index: line.definition_index,
                        record_index: line.source.record_index,
                        payload_offset: line.source.payload_offset,
                        byte_length: 64,
                    };
                    Entity::Line(DocumentLine {
                        entity_id: entities.len() as u64 + n as u64,
                        root_record_index: span.record_index,
                        source_ref,
                        geometry,
                    })
                })
                .collect())
        })();
        match selected {
            Ok(selection) => {
                check_limit(
                    "document_entities",
                    entities.len() as u64 + selection.len() as u64,
                    options.read_limits.max_entities,
                )?;
                for entity in &selection {
                    let ancestors = entity
                        .as_line()
                        .map(|e| e.geometry.scene().ancestors.as_slice())
                        .unwrap_or(&[]);
                    let (count, bytes) = crate::scene::output_cost(ancestors, entity.styles());
                    source_count = source_count
                        .checked_add(count)
                        .ok_or_else(|| invalid("document provenance overflow"))?;
                    source_bytes = source_bytes
                        .checked_add(bytes)
                        .ok_or_else(|| invalid("document provenance bytes overflow"))?;
                    check_limit(
                        "document_provenance",
                        source_count,
                        crate::MAX_SCENE_PROVENANCE,
                    )?;
                    check_limit(
                        "document_provenance_bytes",
                        source_bytes,
                        options.read_limits.max_provenance_bytes,
                    )?;
                }
                entities.extend(selection);
                report.decoded_object_count += 1;
            }
            Err(DracadError::UnsupportedFeature(message)) => {
                *report.unsupported_counts.entry(span.tag_raw).or_default() += 1;
                report.diagnostic(options, "unsupported_object", message, Some(source));
                barrier = true;
            }
            Err(error) => return Err(error.into()),
        }
    }
    report.scene_nodes_visited = context.as_ref().map_or(0, |c| c.nodes);
    report.entity_count = entities.len() as u64;
    if report.decoded_object_count + report.handled_auxiliary_object_count
        == report.primary_object_count
        && context.is_some()
    {
        report.geometry_status = ReadStatus::Complete;
    }
    if report.geometry_status == ReadStatus::Complete
        && matches!(
            options.scope,
            ReadScope::StoredXyLines | ReadScope::StoredXyPrimitives
        )
    {
        report.status = ReadStatus::Complete;
    }
    if options.policy == ReadPolicy::Strict && report.status != ReadStatus::Complete {
        return Err(ReadError { error:DracadError::UnsupportedFeature("strict read requires complete requested scope; use partial to inspect the qualified subset and report".into()),report:Some(Box::new(report)) });
    }
    Ok(Document {
        raw,
        report,
        entities,
    })
}
