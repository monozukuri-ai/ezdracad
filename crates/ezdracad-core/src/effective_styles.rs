//! Bounded native attribute-index resolution for explicitly admitted contexts.
//!
//! Effective indices are file-local references. This module evaluates neither
//! visibility nor printing, and it never creates an inline-package ByBlock scope.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{
    AttributeReference, AttributeReferenceKind, BlockLibrary, DracadError, LineRecord2D,
    RawDocument, RawRecord, RawStyleTables, RecordAttributes, Result,
};

/// A hard cap on copied output/provenance, independently of input-record limits.
pub const MAX_EFFECTIVE_STYLE_LINES: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleSourceKind {
    Record,
    BlockPlacement,
    Layer,
    Color,
    LineType,
    Width,
}

impl StyleSourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::BlockPlacement => "block_placement",
            Self::Layer => "layer",
            Self::Color => "color",
            Self::LineType => "line_type",
            Self::Width => "width",
        }
    }
}

/// Immutable source evidence. Offsets are Contents-relative; `index` is local
/// to `table_domain`. Block placement sources retain both complete 64-byte slots.
/// A block-placement step also records scope when an explicit child overrides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSource {
    pub kind: StyleSourceKind,
    pub table_domain: &'static str,
    pub index: u64,
    pub definition_index: Option<u64>,
    pub payload_offset: u64,
    pub raw_bytes: Vec<u8>,
}

/// One resolved native index plus its source/context chain, never a DXF ACI,
/// printed width, visibility result or normalized layer ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStyleIndex {
    pub index: u16,
    pub sources: Vec<StyleSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveStyleIndices {
    pub color: ResolvedStyleIndex,
    pub line_type: ResolvedStyleIndex,
    pub width: ResolvedStyleIndex,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveLineStyle {
    pub source: LineRecord2D,
    pub attributes: RecordAttributes,
    pub styles: EffectiveStyleIndices,
}

/// A direct, planar definition line under one selected primary root reference.
/// Placement guards match the older direct-only placement subset. Source stays
/// definition-local; `start`/`end` are the selected reference's parent coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveBlockLineStyle {
    pub reference_record_index: u64,
    pub definition_index: u64,
    pub source: LineRecord2D,
    pub attributes: RecordAttributes,
    pub styles: EffectiveStyleIndices,
    pub start: [f64; 2],
    pub end: [f64; 2],
}

impl EffectiveBlockLineStyle {
    #[must_use]
    pub const fn coordinate_space(&self) -> &'static str {
        "package_parent"
    }
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

    fn maximum(self) -> u16 {
        match self {
            Self::Color => 256,
            Self::LineType => 32,
            Self::Width => 16,
        }
    }
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn primary_record(document: &RawDocument, selected: u64) -> Result<RawRecord<'_>> {
    let mut found = None;
    // Consume the entire one-level table: selecting a known object must not
    // conceal a malformed later span. No unknown interior is interpreted.
    for span in document.object_spans() {
        let span = span?;
        if span.record_index == selected {
            found = document.record(selected);
        }
    }
    found.ok_or_else(|| unsupported("effective style requires a primary nonterminal root record"))
}

fn source(
    kind: StyleSourceKind,
    table_domain: &'static str,
    index: u64,
    definition_index: Option<u64>,
    payload_offset: u64,
    raw_bytes: &[u8],
) -> StyleSource {
    StyleSource {
        kind,
        table_domain,
        index,
        definition_index,
        payload_offset,
        raw_bytes: raw_bytes.to_vec(),
    }
}

fn index_checked(property: Property, index: u16) -> Result<u16> {
    if index == 0 || index > property.maximum() {
        return Err(unsupported("unqualified effective style-table reference"));
    }
    Ok(index)
}

fn layer_index(
    tables: &RawStyleTables,
    property: Property,
    layer_index: u16,
    sources: &mut Vec<StyleSource>,
) -> Result<u16> {
    let layer = tables
        .layers
        .get(usize::from(layer_index))
        .filter(|layer| layer.index == layer_index)
        .ok_or_else(|| unsupported("unqualified effective layer row"))?;
    sources.push(source(
        StyleSourceKind::Layer,
        "layer_table",
        u64::from(layer.index),
        None,
        layer.payload_offset,
        &layer.raw_bytes,
    ));
    index_checked(
        property,
        match property {
            Property::Color => layer.color_index_raw,
            Property::LineType => layer.line_type_index_raw,
            Property::Width => layer.width_index_raw,
        },
    )
}

fn target_source(tables: &RawStyleTables, property: Property, index: u16) -> Result<StyleSource> {
    let slot = usize::from(index_checked(property, index)? - 1);
    let (kind, domain, offset, bytes) = match property {
        Property::Color => {
            let row = tables
                .colors
                .get(slot)
                .filter(|row| row.index == index)
                .ok_or_else(|| unsupported("missing effective color table row"))?;
            (
                StyleSourceKind::Color,
                "color_table",
                row.payload_offset,
                &row.raw_bytes,
            )
        }
        Property::LineType => {
            let row = tables
                .line_types
                .get(slot)
                .filter(|row| row.index == index)
                .ok_or_else(|| unsupported("missing effective line-type table row"))?;
            (
                StyleSourceKind::LineType,
                "line_type_table",
                row.payload_offset,
                &row.raw_bytes,
            )
        }
        Property::Width => {
            let row = tables
                .widths
                .get(slot)
                .filter(|row| row.index == index)
                .ok_or_else(|| unsupported("missing effective width table row"))?;
            (
                StyleSourceKind::Width,
                "width_table",
                row.payload_offset,
                &row.raw_bytes,
            )
        }
    };
    Ok(source(kind, domain, u64::from(index), None, offset, bytes))
}

struct BlockContext {
    attributes: RecordAttributes,
    source: StyleSource,
}

fn resolve_property(
    tables: &RawStyleTables,
    attributes: &RecordAttributes,
    record_source: &StyleSource,
    block: Option<&BlockContext>,
    property: Property,
) -> Result<ResolvedStyleIndex> {
    let mut sources = vec![record_source.clone()];
    if let Some(block) = block {
        sources.push(block.source.clone());
    }
    let reference = property.reference(attributes);
    let index = match reference.kind {
        AttributeReferenceKind::Index => index_checked(
            property,
            reference
                .index
                .ok_or_else(|| invalid("explicit style reference lacks an index"))?,
        )?,
        AttributeReferenceKind::ByLayer => {
            let selected_layer = if attributes.layer_index == 0 {
                block.map_or(0, |block| block.attributes.layer_index)
            } else {
                attributes.layer_index
            };
            layer_index(tables, property, selected_layer, &mut sources)?
        }
        AttributeReferenceKind::ByBlock => {
            let block = block.ok_or_else(|| {
                unsupported("ByBlock style has no qualified block-insertion context")
            })?;
            let inherited = property.reference(&block.attributes);
            match inherited.kind {
                AttributeReferenceKind::Index => index_checked(
                    property,
                    inherited
                        .index
                        .ok_or_else(|| invalid("explicit insertion style lacks an index"))?,
                )?,
                AttributeReferenceKind::ByLayer => {
                    layer_index(tables, property, block.attributes.layer_index, &mut sources)?
                }
                AttributeReferenceKind::ByBlock => {
                    return Err(unsupported(
                        "root block insertion itself has unresolved ByBlock attributes",
                    ));
                }
            }
        }
    };
    sources.push(target_source(tables, property, index)?);
    Ok(ResolvedStyleIndex { index, sources })
}

fn resolve(
    tables: &RawStyleTables,
    attributes: &RecordAttributes,
    record_source: &StyleSource,
    block: Option<&BlockContext>,
) -> Result<EffectiveStyleIndices> {
    Ok(EffectiveStyleIndices {
        color: resolve_property(tables, attributes, record_source, block, Property::Color)?,
        line_type: resolve_property(tables, attributes, record_source, block, Property::LineType)?,
        width: resolve_property(tables, attributes, record_source, block, Property::Width)?,
    })
}

pub(crate) fn effective_line_style(
    document: &RawDocument,
    record_index: u64,
) -> Result<EffectiveLineStyle> {
    let record = primary_record(document, record_index)?;
    if u16::from_le_bytes([record.bytes()[8], record.bytes()[9]]) != 1 {
        return Err(unsupported(
            "effective root style requires a direct line span",
        ));
    }
    let line = document
        .attributed_line_record(record_index)?
        .ok_or_else(|| unsupported("effective root style requires a qualified line tag"))?;
    let tables = RawStyleTables::parse(document)?;
    let evidence = source(
        StyleSourceKind::Record,
        "root_records",
        record.index,
        None,
        record.payload_offset,
        record.bytes(),
    );
    Ok(EffectiveLineStyle {
        source: line.source,
        attributes: line.attributes,
        styles: resolve(&tables, &line.attributes, &evidence, None)?,
    })
}

pub(crate) fn effective_block_line_styles(
    document: &RawDocument,
    library: &BlockLibrary,
    reference_index: u64,
    max_lines: u64,
) -> Result<Vec<EffectiveBlockLineStyle>> {
    check_limit(
        "effective_style_lines_limit",
        max_lines,
        MAX_EFFECTIVE_STYLE_LINES,
    )?;
    let primary = primary_record(document, reference_index)?;
    let reference = document
        .package_reference(reference_index)?
        .ok_or_else(|| unsupported("effective style requires a qualified block reference"))?;
    // The guards apply even to empty or entirely explicit definitions.
    reference.apply_xy([0.0, 0.0])?;
    let attributes = document
        .record_attributes(reference_index)?
        .ok_or_else(|| invalid("missing block-reference attributes"))?;
    let next = document
        .record(reference.continuation_index)
        .ok_or_else(|| invalid("missing block-reference continuation"))?;
    let mut reference_bytes = primary.bytes().to_vec();
    reference_bytes.extend(next.bytes());
    let block = BlockContext {
        attributes,
        source: source(
            StyleSourceKind::BlockPlacement,
            "root_records",
            reference_index,
            None,
            primary.payload_offset,
            &reference_bytes,
        ),
    };
    let definition = library.resolve(reference.name_key())?;
    if definition.header_bytes[40..112]
        .iter()
        .chain(&definition.header_bytes[160..176])
        .any(|byte| *byte != 0)
    {
        return Err(unsupported(
            "unqualified block-definition coordinate fields",
        ));
    }
    let tables = RawStyleTables::parse(document)?;
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor + 1 < definition.record_count {
        let record = library
            .record(definition.index, cursor)
            .ok_or_else(|| invalid("missing bounded effective-style block record"))?;
        let span = u64::from(u16::from_le_bytes([record.bytes()[8], record.bytes()[9]]));
        let end = cursor
            .checked_add(span)
            .filter(|end| *end < definition.record_count)
            .ok_or_else(|| invalid("effective-style block span exceeds terminal"))?;
        if span == 0 {
            return Err(invalid("zero effective-style block span"));
        }
        if record.tag == 0x33 {
            for index in cursor + 1..end {
                if library.record(definition.index, index).map(|r| r.tag) != Some(0xfe) {
                    return Err(unsupported(
                        "unqualified metadata continuation inside block",
                    ));
                }
            }
        } else {
            if span != 1 {
                return Err(unsupported(
                    "effective block styles require direct line leaves",
                ));
            }
            let line = record
                .attributed_line_record()?
                .ok_or_else(|| unsupported("unqualified effective-style block leaf"))?;
            line.attributes.ensure_profile(document.header().profile)?;
            if line.source.elevation != 0.0 || line.source.thickness != 0.0 {
                return Err(unsupported(
                    "nonzero block line elevation/thickness has no qualified placement",
                ));
            }
            check_limit("effective_style_lines", result.len() as u64 + 1, max_lines)?;
            let evidence = source(
                StyleSourceKind::Record,
                "block_definition_records",
                record.index,
                Some(definition.index),
                record.payload_offset,
                record.bytes(),
            );
            result.push(EffectiveBlockLineStyle {
                reference_record_index: reference_index,
                definition_index: definition.index,
                source: line.source,
                attributes: line.attributes,
                styles: resolve(&tables, &line.attributes, &evidence, Some(&block))?,
                start: reference.apply_xy(line.source.start)?,
                end: reference.apply_xy(line.source.end)?,
            });
        }
        cursor = end;
    }
    Ok(result)
}
