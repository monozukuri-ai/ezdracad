//! Bounded traversal of the admitted planar inline/package-reference subset.
//!
//! Synthetic traversal tests establish algorithmic behavior, not qualification
//! of additional native flags, units, visibility or printed appearance.

use std::rc::Rc;

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{
    AttributeReference, AttributeReferenceKind, BlockLibrary, DracadError, EffectiveStyleIndices,
    InlineTableDomain, LineRecord2D, ObservedProfile, PackageReference2D, RawDocument, RawRecord,
    RawStyleTables, RecordAttributes, ResolvedStyleIndex, Result, StyleSource, StyleSourceKind,
};

pub const DEFAULT_MAX_SCENE_DEPTH: u64 = 32;
pub const MAX_SCENE_DEPTH: u64 = 128;
pub const MAX_SCENE_PROVENANCE: u64 = 131_072;
pub const MAX_SCENE_PROVENANCE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneLimits {
    pub max_depth: u64,
    pub max_nodes: u64,
    pub max_lines: u64,
}

impl Default for SceneLimits {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_SCENE_DEPTH,
            max_nodes: crate::MAX_RECORDS_PER_PAGE,
            max_lines: crate::MAX_RECORDS_PER_PAGE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneAncestorKind {
    InlinePackage,
    BlockReference,
}

impl SceneAncestorKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InlinePackage => "inline_package",
            Self::BlockReference => "block_reference",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneDefinitionSource {
    pub index: u64,
    pub payload_offset: u64,
    pub header_bytes: [u8; 176],
}

/// Complete source bytes for a wrapper/reference and, for references, its
/// resolved target definition header. Indices are local to the named table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAncestor {
    pub kind: SceneAncestorKind,
    pub table_domain: InlineTableDomain,
    pub definition_index: Option<u64>,
    pub record_index: u64,
    pub payload_offset: u64,
    pub raw_bytes: Vec<u8>,
    pub target_definition: Option<SceneDefinitionSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCoordinateScope {
    RootRecords,
    PackageParent,
}

impl SceneCoordinateScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RootRecords => "root_records",
            Self::PackageParent => "package_parent",
        }
    }
}

/// A planar leaf with all admitted placements composed into the selected root
/// object's parent scope. This scope is not a universal WCS or a unit claim.
/// Inline wrapper bodies contribute no additional placement. The legacy decoder
/// rejects dependent paths crossing them; the separately opted-in marker25
/// decoder qualifies the observed wrappers as transparent to C/T/W inheritance.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneLine2D {
    pub source: LineRecord2D,
    pub attributes: RecordAttributes,
    pub table_domain: InlineTableDomain,
    pub definition_index: Option<u64>,
    pub ancestors: Vec<SceneAncestor>,
    /// `[a,b,c,d,tx,ty]`: `(a*x+c*y+tx, b*x+d*y+ty)`.
    pub cumulative_transform: [f64; 6],
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub coordinate_scope: SceneCoordinateScope,
    /// Layer selected solely for style resolution; no visibility/ownership rule.
    pub style_layer_index: u16,
    pub styles: EffectiveStyleIndices,
}

#[derive(Clone, Copy)]
struct Table {
    domain: InlineTableDomain,
    definition: Option<u64>,
    start: u64,
    record_count: u64,
}

impl Table {
    fn record<'a>(&self, document: &'a RawDocument, index: u64) -> Result<RawRecord<'a>> {
        let offset = index
            .checked_mul(64)
            .and_then(|n| self.start.checked_add(n))
            .ok_or_else(|| invalid("scene record offset overflow"))?;
        if index >= self.record_count {
            return Err(invalid("scene record exceeds table"));
        }
        RawRecord::at(document.contents(), index, offset)
            .ok_or_else(|| invalid("missing bounded scene record"))
    }

    fn span<'a>(
        &self,
        document: &'a RawDocument,
        index: u64,
        end: u64,
    ) -> Result<(RawRecord<'a>, u64)> {
        let record = self.record(document, index)?;
        if matches!(record.tag, 0xfe | 0xff) {
            return Err(invalid("orphan scene continuation/terminal"));
        }
        let count = u64::from(u16::from_le_bytes([record.bytes()[8], record.bytes()[9]]));
        let next = index
            .checked_add(count)
            .filter(|n| count != 0 && *n <= end)
            .ok_or_else(|| invalid("zero or overflowing scene object span"))?;
        Ok((record, next))
    }
}

struct BlockContext {
    layer: u16,
    source: StyleSource,
    parent: Option<Rc<BlockContext>>,
    styles: EffectiveStyleIndices,
}

struct Frame {
    table: Table,
    next: u64,
    end: u64,
    transform: [f64; 6],
    block: Option<Rc<BlockContext>>,
    closes_ancestor: bool,
    active_definition: Option<u64>,
    metadata_allowed: bool,
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn ensure_style_scope(attributes: &RecordAttributes, ancestors: &[SceneAncestor]) -> Result<()> {
    if !attributes.has_dependencies() {
        return Ok(());
    }
    for ancestor in ancestors.iter().rev() {
        if ancestor.kind == SceneAncestorKind::BlockReference {
            break;
        }
        if ancestor.kind == SceneAncestorKind::InlinePackage {
            return Err(unsupported("unqualified inline-dependent attribute scope"));
        }
    }
    Ok(())
}

fn evidence(
    kind: StyleSourceKind,
    table: Table,
    record: RawRecord<'_>,
    bytes: &[u8],
) -> StyleSource {
    StyleSource {
        kind,
        table_domain: table.domain.as_str(),
        index: record.index,
        definition_index: table.definition,
        payload_offset: record.payload_offset,
        raw_bytes: bytes.to_vec(),
    }
}

/// Scene-local admission for supported exact color-dependent references.
/// The raw reference/attribute APIs
/// deliberately retain their separate exact-tag contracts. This qualifies
/// stored fields, not a standalone ByBlock result or rendered appearance.
fn reference_attributes(record: RawRecord<'_>) -> Result<RecordAttributes> {
    if record.tag == 6 {
        return record.record_attributes();
    }
    if !matches!(record.tag, 0x2006 | 0x4006) {
        return Err(unsupported("unqualified scene reference attribute flags"));
    }
    let original: [u8; 16] = record.bytes()[..16]
        .try_into()
        .map_err(|_| invalid("truncated scene reference header"))?;
    // These exact native headers encode color257/258 with zero color byte and
    // identical independent ltype/width selectors; no generic flags are masked.
    let mut color_header = original;
    let selector = if record.tag == 0x2006 {
        0x2402u16
    } else {
        0x4402u16
    };
    color_header[..2].copy_from_slice(&selector.to_le_bytes());
    let mut result = crate::decode_record_attributes(&color_header)?;
    result.tag_raw = record.tag;
    result.header_bytes = original;
    result.flags_raw = original[1];
    Ok(result)
}

/// Scene-local exact admission, separate from the frozen raw attribute API.
/// Supported wrappers create neither placement
/// nor a C/T/W or Layer0 insertion scope. Their attributes remain source data,
/// not property contributors; all 64 source bytes stay in `ancestors`.
fn inline_attributes(record: RawRecord<'_>, qualified: bool) -> Result<RecordAttributes> {
    if matches!(record.tag, 0x30 | 0x430) {
        return record.record_attributes();
    }
    if !qualified || !matches!(record.tag, 0x2030 | 0x4030) {
        return Err(unsupported("unqualified scene inline attribute flags"));
    }
    let original: [u8; 16] = record.bytes()[..16]
        .try_into()
        .map_err(|_| invalid("truncated scene inline header"))?;
    let mut canonical = original;
    let tag = if record.tag == 0x2030 {
        0x2402u16
    } else {
        0x4402u16
    };
    canonical[..2].copy_from_slice(&tag.to_le_bytes());
    let mut attributes = crate::decode_record_attributes(&canonical)?;
    attributes.tag_raw = record.tag;
    attributes.header_bytes = original;
    attributes.flags_raw = original[1];
    Ok(attributes)
}

fn scene_reference(
    record: RawRecord<'_>,
    continuation: RawRecord<'_>,
    record_count: u64,
) -> Result<PackageReference2D> {
    if record.tag == 6 {
        return crate::package_refs::decode_reference(record, Some(continuation), record_count)?
            .ok_or_else(|| invalid("missing admitted scene reference"));
    }
    if !matches!(record.tag, 0x2006 | 0x4006) {
        return Err(unsupported("unqualified scene reference tag/flags"));
    }
    let bytes = record.bytes();
    let next_index = record
        .index
        .checked_add(1)
        .ok_or_else(|| invalid("scene reference continuation index overflow"))?;
    let next_offset = record
        .payload_offset
        .checked_add(64)
        .ok_or_else(|| invalid("scene reference continuation offset overflow"))?;
    if u16::from_le_bytes([bytes[8], bytes[9]]) != 2 {
        return Err(unsupported("unqualified scene reference slot span"));
    }
    if next_index >= record_count.saturating_sub(1)
        || continuation.index != next_index
        || continuation.payload_offset != next_offset
        || continuation.tag != 0xfe
    {
        return Err(invalid(
            "scene reference lacks a following nonterminal FE continuation",
        ));
    }
    let name_field_bytes: [u8; 32] = bytes[32..64]
        .try_into()
        .map_err(|_| invalid("truncated scene reference name"))?;
    if !matches!(name_field_bytes.iter().position(|b| *b == 0), Some(1..=31)) {
        return Err(invalid(
            "scene reference name lacks a nonempty bounded NUL key",
        ));
    }
    let floats = |bytes: &[u8]| -> Result<[f64; 6]> {
        let mut values = [0.0; 6];
        for (index, value) in values.iter_mut().enumerate() {
            *value = f64::from_le_bytes(
                bytes[16 + index * 8..24 + index * 8]
                    .try_into()
                    .map_err(|_| invalid("truncated scene reference numeric field"))?,
            );
        }
        finite(values)
    };
    // Only the first two primary floating fields are numeric; the remaining
    // primary body is an opaque fixed-width name, including bytes after NUL.
    let anchor_raw = finite([
        f64::from_le_bytes(
            bytes[16..24]
                .try_into()
                .map_err(|_| invalid("truncated scene reference anchor"))?,
        ),
        f64::from_le_bytes(
            bytes[24..32]
                .try_into()
                .map_err(|_| invalid("truncated scene reference anchor"))?,
        ),
    ])?;
    Ok(PackageReference2D {
        record_index: record.index,
        payload_offset: record.payload_offset,
        continuation_index: continuation.index,
        continuation_offset: continuation.payload_offset,
        tag_raw: record.tag,
        header_bytes: bytes[..16]
            .try_into()
            .map_err(|_| invalid("truncated scene reference header"))?,
        name_field_bytes,
        anchor_raw,
        continuation_header_bytes: continuation.bytes()[..16]
            .try_into()
            .map_err(|_| invalid("truncated scene continuation header"))?,
        placement_raw: floats(continuation.bytes())?,
    })
}

#[derive(Clone, Copy)]
enum Property {
    Color,
    LineType,
    Width,
}

impl Property {
    fn reference(self, attributes: &RecordAttributes) -> AttributeReference {
        match self {
            Self::Color => attributes.color,
            Self::LineType => attributes.line_type,
            Self::Width => attributes.width,
        }
    }
    fn selected(self, styles: &EffectiveStyleIndices) -> &ResolvedStyleIndex {
        match self {
            Self::Color => &styles.color,
            Self::LineType => &styles.line_type,
            Self::Width => &styles.width,
        }
    }
    fn check(self, index: u16) -> Result<u16> {
        let max = match self {
            Self::Color => 256,
            Self::LineType => 32,
            Self::Width => 16,
        };
        if index == 0 || index > max {
            return Err(unsupported("unqualified scene style-table reference"));
        }
        Ok(index)
    }
}

fn row_source(
    kind: StyleSourceKind,
    domain: &'static str,
    index: u16,
    offset: u64,
    bytes: &[u8],
) -> StyleSource {
    StyleSource {
        kind,
        table_domain: domain,
        index: u64::from(index),
        definition_index: None,
        payload_offset: offset,
        raw_bytes: bytes.to_vec(),
    }
}

fn target(tables: &RawStyleTables, property: Property, index: u16) -> Result<StyleSource> {
    let slot = usize::from(property.check(index)? - 1);
    Ok(match property {
        Property::Color => {
            let row = tables
                .colors
                .get(slot)
                .filter(|r| r.index == index)
                .ok_or_else(|| unsupported("missing scene color row"))?;
            row_source(
                StyleSourceKind::Color,
                "color_table",
                index,
                row.payload_offset,
                &row.raw_bytes,
            )
        }
        Property::LineType => {
            let row = tables
                .line_types
                .get(slot)
                .filter(|r| r.index == index)
                .ok_or_else(|| unsupported("missing scene line-type row"))?;
            row_source(
                StyleSourceKind::LineType,
                "line_type_table",
                index,
                row.payload_offset,
                &row.raw_bytes,
            )
        }
        Property::Width => {
            let row = tables
                .widths
                .get(slot)
                .filter(|r| r.index == index)
                .ok_or_else(|| unsupported("missing scene width row"))?;
            row_source(
                StyleSourceKind::Width,
                "width_table",
                index,
                row.payload_offset,
                &row.raw_bytes,
            )
        }
    })
}

fn resolve_property(
    tables: &RawStyleTables,
    attributes: &RecordAttributes,
    source: &StyleSource,
    block: Option<&BlockContext>,
    layer: u16,
    property: Property,
) -> Result<ResolvedStyleIndex> {
    let reference = property.reference(attributes);
    let mut sources = vec![source.clone()];
    if reference.kind == AttributeReferenceKind::ByBlock {
        let inherited = property.selected(
            &block
                .ok_or_else(|| unsupported("scene ByBlock style has no block-insertion context"))?
                .styles,
        );
        sources.extend(inherited.sources.iter().cloned());
        return Ok(ResolvedStyleIndex {
            index: inherited.index,
            sources,
        });
    }
    // Explicit overrides retain insertion scope evidence without inheriting it.
    let mut scope = block;
    while let Some(context) = scope {
        sources.push(context.source.clone());
        scope = context.parent.as_deref();
    }
    let index = match reference.kind {
        AttributeReferenceKind::Index => property.check(
            reference
                .index
                .ok_or_else(|| invalid("scene explicit style lacks an index"))?,
        )?,
        AttributeReferenceKind::ByLayer => {
            let row = tables
                .layers
                .get(usize::from(layer))
                .filter(|r| r.index == layer)
                .ok_or_else(|| unsupported("missing scene layer row"))?;
            sources.push(row_source(
                StyleSourceKind::Layer,
                "layer_table",
                layer,
                row.payload_offset,
                &row.raw_bytes,
            ));
            property.check(match property {
                Property::Color => row.color_index_raw,
                Property::LineType => row.line_type_index_raw,
                Property::Width => row.width_index_raw,
            })?
        }
        AttributeReferenceKind::ByBlock => unreachable!(),
    };
    sources.push(target(tables, property, index)?);
    Ok(ResolvedStyleIndex { index, sources })
}

fn resolve(
    tables: &RawStyleTables,
    attributes: &RecordAttributes,
    source: &StyleSource,
    block: Option<&BlockContext>,
) -> Result<(u16, EffectiveStyleIndices)> {
    let layer = if attributes.layer_index == 0 {
        block.map_or(0, |b| b.layer)
    } else {
        attributes.layer_index
    };
    Ok((
        layer,
        EffectiveStyleIndices {
            color: resolve_property(tables, attributes, source, block, layer, Property::Color)?,
            line_type: resolve_property(
                tables,
                attributes,
                source,
                block,
                layer,
                Property::LineType,
            )?,
            width: resolve_property(tables, attributes, source, block, layer, Property::Width)?,
        },
    ))
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn local_transform(reference: &PackageReference2D) -> Result<[f64; 6]> {
    reference.apply_xy([0.0, 0.0])?;
    let [tx, ty, _, sx, _, degrees] = reference.placement_raw;
    let (sin, cos) = degrees.to_radians().sin_cos();
    finite([sx * cos, sx * sin, -sx * sin, sx * cos, tx, ty])
}

fn finite<const N: usize>(values: [f64; N]) -> Result<[f64; N]> {
    if !values.into_iter().all(f64::is_finite) {
        return Err(invalid("non-finite composed scene transform/result"));
    }
    Ok(values)
}

fn compose(parent: [f64; 6], local: [f64; 6]) -> Result<[f64; 6]> {
    let [a, b, c, d, x, y] = parent;
    let [e, f, g, h, u, v] = local;
    let output = finite([
        a * e + c * f,
        b * e + d * f,
        a * g + c * h,
        b * g + d * h,
        a * u + c * v + x,
        b * u + d * v + y,
    ])?;
    if output[..4].iter().all(|value| *value == 0.0) {
        return Err(invalid("composed scene scale underflow"));
    }
    Ok(output)
}

fn apply(matrix: [f64; 6], point: [f64; 2]) -> Result<[f64; 2]> {
    let [a, b, c, d, x, y] = matrix;
    finite([
        a * point[0] + c * point[1] + x,
        b * point[0] + d * point[1] + y,
    ])
}

/// Count every copied descriptor/raw-byte allocation in the output, including
/// the three independently owned property chains and target definition headers.
pub(crate) fn output_cost(
    ancestors: &[SceneAncestor],
    styles: &EffectiveStyleIndices,
) -> (u64, u64) {
    let mut entries = 2; // retained leaf source and raw attribute header
    let mut bytes = 64 + 16;
    for ancestor in ancestors {
        entries += 1;
        bytes += ancestor.raw_bytes.len() as u64;
        if ancestor.target_definition.is_some() {
            entries += 1;
            bytes += 176;
        }
    }
    for property in [&styles.color, &styles.line_type, &styles.width] {
        entries += property.sources.len() as u64;
        bytes += property
            .sources
            .iter()
            .map(|s| s.raw_bytes.len() as u64)
            .sum::<u64>();
    }
    (entries, bytes)
}

/// Traverse one complete primary root object. Unknown child semantics, cycles,
/// ambiguous/missing names and budget failures return an error for the whole
/// request. The result never contains a successful partial traversal.
pub fn decode(
    document: &RawDocument,
    selected: u64,
    limits: &SceneLimits,
) -> Result<Vec<SceneLine2D>> {
    decode_impl(document, selected, limits, false)
}

/// Explicit marker25 inline-scope extension. Exact 0030/0430/2030/4030
/// wrappers with the admitted body are
/// transparent to C/T/W and Layer0 inheritance; the nearest true reference
/// supplies insertion context. No general flags, state, unit or visibility
/// semantics are inferred. The legacy `decode` contract remains unchanged.
pub fn decode_with_inline_scopes(
    document: &RawDocument,
    selected: u64,
    limits: &SceneLimits,
) -> Result<Vec<SceneLine2D>> {
    if document.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported(
            "qualified inline inheritance requires marker25",
        ));
    }
    decode_impl(document, selected, limits, true)
}

fn decode_impl(
    document: &RawDocument,
    selected: u64,
    limits: &SceneLimits,
    qualified_inline_scopes: bool,
) -> Result<Vec<SceneLine2D>> {
    check_limit(
        "scene_nodes_limit",
        limits.max_nodes,
        crate::MAX_RECORDS_PER_PAGE,
    )?;
    check_limit(
        "scene_lines_limit",
        limits.max_lines,
        crate::MAX_RECORDS_PER_PAGE,
    )?;
    let mut selected_end = None;
    for span in document.object_spans() {
        let span = span?;
        if span.record_index == selected {
            selected_end = Some(span.end_record_index);
        }
    }
    let end = selected_end
        .ok_or_else(|| unsupported("scene requires a primary nonterminal root record"))?;
    let mut context = DecodeContext::new(document, None, u64::MAX)?;
    decode_selected(
        document,
        selected,
        end,
        limits,
        qualified_inline_scopes,
        &mut context,
    )
}

/// Shared immutable tables and an aggregate work budget for the Document reader.
/// Only the reader's already validated root span may enter this internal path.
pub(crate) struct DecodeContext {
    tables: RawStyleTables,
    library: Option<BlockLibrary>,
    pub(crate) nodes: u64,
    max_nodes: u64,
}

impl DecodeContext {
    /// Root primitive attributes reuse the proven table resolver, after an exact
    /// primitive-tag admission. Never mask flags or alter retained source bytes.
    pub(crate) fn root_style(
        &mut self,
        raw: &[u8],
        index: u64,
        offset: u64,
    ) -> Result<crate::RootPrimitiveStyle> {
        let original: [u8; 16] = raw
            .get(..16)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| invalid("primitive header truncated"))?;
        let tag = u16::from_le_bytes([original[0], original[1]]);
        if !matches!(tag, 3..=5) || original[7] != 0 || original[12..16] != [0, 1, 0, 0] {
            return Err(DracadError::UnsupportedFeature(
                "root primitive attribute flags/material/tail outside observed profile".into(),
            ));
        }
        let mut normalized = original;
        normalized[0..2].copy_from_slice(&2u16.to_le_bytes());
        let mut attributes = crate::decode_record_attributes(&normalized)?;
        // Only explicit references have new native primitive evidence. Root
        // ByBlock, and unobserved primitive dependency flags remain held.
        if attributes.has_dependencies() {
            return Err(DracadError::UnsupportedFeature(
                "unqualified primitive attribute dependency".into(),
            ));
        }
        attributes.tag_raw = tag;
        attributes.header_bytes = original;
        attributes.flags_raw = original[1];
        let source = StyleSource {
            kind: StyleSourceKind::Record,
            table_domain: "root_records",
            index,
            definition_index: None,
            payload_offset: offset,
            raw_bytes: raw.to_vec(),
        };
        let (style_layer_index, styles) = resolve(&self.tables, &attributes, &source, None)?;
        Ok(crate::RootPrimitiveStyle {
            attributes,
            style_layer_index,
            styles,
        })
    }
    pub(crate) fn visit_root(&mut self) -> Result<()> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| invalid("document nodes overflow"))?;
        check_limit("document_nodes", self.nodes, self.max_nodes)
    }
    pub(crate) fn new(
        document: &RawDocument,
        library: Option<BlockLibrary>,
        max_nodes: u64,
    ) -> Result<Self> {
        Ok(Self {
            tables: RawStyleTables::parse(document)?,
            library,
            nodes: 0,
            max_nodes,
        })
    }
}

pub(crate) fn decode_selected(
    document: &RawDocument,
    selected: u64,
    end: u64,
    limits: &SceneLimits,
    qualified_inline_scopes: bool,
    context: &mut DecodeContext,
) -> Result<Vec<SceneLine2D>> {
    check_limit(
        "scene_nodes_limit",
        limits.max_nodes,
        crate::MAX_RECORDS_PER_PAGE,
    )?;
    check_limit(
        "scene_lines_limit",
        limits.max_lines,
        crate::MAX_RECORDS_PER_PAGE,
    )?;
    let header = document.header();
    let root_table = Table {
        domain: InlineTableDomain::RootRecords,
        definition: None,
        start: header.record_start,
        record_count: header.record_count,
    };
    let tables = &context.tables;
    let library = &mut context.library;
    let mut stack = vec![Frame {
        table: root_table,
        next: selected,
        end,
        transform: IDENTITY,
        block: None,
        closes_ancestor: false,
        active_definition: None,
        metadata_allowed: false,
    }];
    let mut ancestors = Vec::new();
    let mut active_definitions = Vec::new();
    let mut lines = Vec::new();
    let mut nodes = 0u64;
    let mut provenance = 0u64;
    let mut provenance_bytes = 0u64;
    while let Some(frame) = stack.last() {
        if frame.next == frame.end {
            let frame = stack.pop().ok_or_else(|| invalid("missing scene frame"))?;
            if frame.closes_ancestor {
                ancestors.pop();
            }
            if frame.active_definition.is_some() {
                active_definitions.pop();
            }
            continue;
        }
        let table = frame.table;
        let index = frame.next;
        let (record, next) = table.span(document, index, frame.end)?;
        let transform = frame.transform;
        let block = frame.block.clone();
        let metadata_allowed = frame.metadata_allowed;
        if let Some(frame) = stack.last_mut() {
            frame.next = next;
        }
        nodes += 1;
        context.nodes = context
            .nodes
            .checked_add(1)
            .ok_or_else(|| invalid("document scene work overflow"))?;
        check_limit("document_nodes", context.nodes, context.max_nodes)?;
        check_limit("scene_nodes", nodes, limits.max_nodes)?;
        match record.tag {
            2 | 0x402 | 0x2402 | 0x4402 if next == index + 1 => {
                let line = record
                    .attributed_line_record()?
                    .ok_or_else(|| invalid("missing admitted scene line"))?;
                line.attributes.ensure_profile(header.profile)?;
                if !qualified_inline_scopes {
                    ensure_style_scope(&line.attributes, &ancestors)?;
                }
                if line.source.elevation != 0.0 || line.source.thickness != 0.0 {
                    return Err(unsupported(
                        "nonzero scene line elevation/thickness has no qualified planar placement",
                    ));
                }
                check_limit("scene_lines", lines.len() as u64 + 1, limits.max_lines)?;
                let source = evidence(StyleSourceKind::Record, table, record, record.bytes());
                let (style_layer_index, styles) =
                    resolve(tables, &line.attributes, &source, block.as_deref())?;
                let (count, bytes) = output_cost(&ancestors, &styles);
                provenance = provenance
                    .checked_add(count)
                    .ok_or_else(|| invalid("scene provenance overflow"))?;
                provenance_bytes = provenance_bytes
                    .checked_add(bytes)
                    .ok_or_else(|| invalid("scene source-byte overflow"))?;
                check_limit("scene_provenance", provenance, MAX_SCENE_PROVENANCE)?;
                check_limit(
                    "scene_provenance_bytes",
                    provenance_bytes,
                    MAX_SCENE_PROVENANCE_BYTES,
                )?;
                lines.push(SceneLine2D {
                    source: line.source,
                    attributes: line.attributes,
                    table_domain: table.domain,
                    definition_index: table.definition,
                    ancestors: ancestors.clone(),
                    cumulative_transform: transform,
                    start: apply(transform, line.source.start)?,
                    end: apply(transform, line.source.end)?,
                    coordinate_scope: if block.is_some() {
                        SceneCoordinateScope::PackageParent
                    } else {
                        SceneCoordinateScope::RootRecords
                    },
                    style_layer_index,
                    styles,
                });
            }
            tag if matches!(tag, 0x30 | 0x430)
                || qualified_inline_scopes && matches!(tag, 0x2030 | 0x4030) => {
                let bytes = record.bytes();
                if next - index < 2
                    || bytes[16..20] != 0x400u32.to_le_bytes()
                    || bytes[20..].iter().any(|b| *b != 0)
                {
                    return Err(unsupported(
                        "unqualified scene inline-package state body/empty variant",
                    ));
                }
                inline_attributes(record, qualified_inline_scopes)?.ensure_profile(header.profile)?;
                check_limit(
                    "scene_depth",
                    ancestors.len() as u64 + 1,
                    limits.max_depth.min(MAX_SCENE_DEPTH),
                )?;
                ancestors.push(SceneAncestor {
                    kind: SceneAncestorKind::InlinePackage,
                    table_domain: table.domain,
                    definition_index: table.definition,
                    record_index: index,
                    payload_offset: record.payload_offset,
                    raw_bytes: bytes.to_vec(),
                    target_definition: None,
                });
                stack.push(Frame {
                    table,
                    next: index + 1,
                    end: next,
                    transform,
                    block,
                    closes_ancestor: true,
                    active_definition: None,
                    metadata_allowed: false,
                });
            }
            6 | 0x2006 | 0x4006 => {
                let continuation = table.record(document, index + 1)?;
                let reference = scene_reference(record, continuation, table.record_count)?;
                let local = local_transform(&reference)?;
                let attributes = reference_attributes(record)?;
                attributes.ensure_profile(header.profile)?;
                if !qualified_inline_scopes {
                    ensure_style_scope(&attributes, &ancestors)?;
                }
                if library.is_none() {
                    *library = Some(document.block_library()?);
                }
                let definition = library
                    .as_ref()
                    .ok_or_else(|| invalid("missing scene library"))?
                    .resolve(reference.name_key())?;
                if definition.header_bytes[40..112]
                    .iter()
                    .chain(&definition.header_bytes[160..176])
                    .any(|b| *b != 0)
                {
                    return Err(unsupported(
                        "unqualified scene definition coordinate fields",
                    ));
                }
                if active_definitions.contains(&definition.index) {
                    return Err(unsupported("cyclic scene block-reference path"));
                }
                check_limit(
                    "scene_depth",
                    ancestors.len() as u64 + 1,
                    limits.max_depth.min(MAX_SCENE_DEPTH),
                )?;
                let mut raw_bytes = record.bytes().to_vec();
                raw_bytes.extend(continuation.bytes());
                let source = evidence(StyleSourceKind::BlockPlacement, table, record, &raw_bytes);
                let (layer, styles) = resolve(tables, &attributes, &source, block.as_deref())?;
                let context = Rc::new(BlockContext {
                    layer,
                    source,
                    parent: block,
                    styles,
                });
                ancestors.push(SceneAncestor {
                    kind: SceneAncestorKind::BlockReference,
                    table_domain: table.domain,
                    definition_index: table.definition,
                    record_index: index,
                    payload_offset: record.payload_offset,
                    raw_bytes,
                    target_definition: Some(SceneDefinitionSource {
                        index: definition.index,
                        payload_offset: definition.header_offset,
                        header_bytes: definition.header_bytes,
                    }),
                });
                active_definitions.push(definition.index);
                stack.push(Frame {
                    table: Table {
                        domain: InlineTableDomain::BlockDefinitionRecords,
                        definition: Some(definition.index),
                        start: definition.record_start,
                        record_count: definition.record_count,
                    },
                    next: 0,
                    end: definition.record_count - 1,
                    transform: compose(transform, local)?,
                    block: Some(context),
                    closes_ancestor: true,
                    active_definition: Some(definition.index),
                    metadata_allowed: true,
                });
            }
            0x33 if metadata_allowed => {
                for cursor in index + 1..next {
                    if table.record(document, cursor)?.tag != 0xfe {
                        return Err(unsupported(
                            "unqualified scene definition metadata continuation",
                        ));
                    }
                }
            }
            _ => return Err(DracadError::UnsupportedFeature(format!(
                "unqualified scene child tag/flags 0x{:04x} or span {} at {} record {} (definition {:?})",
                record.tag, next - index, table.domain.as_str(), index, table.definition
            ))),
        }
    }
    Ok(lines)
}
