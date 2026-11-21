#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod attributes;
mod basic2d;
mod best_effort;
mod block3d;
mod blocks;
mod container;
mod coordinate_input_units;
mod coordinate_measurement;
mod coordinate_space;
mod dimension_scale;
mod document;
mod drawing_settings;
mod effective_styles;
mod error;
mod format;
mod header;
mod inline_packages;
mod inspect;
mod limits;
mod line3d;
mod mps;
mod objects;
mod origin_units;
mod package_refs;
mod preflight;
mod print_settings;
mod print_width;
pub mod qualification;
mod record_attributes;
mod records;
mod root_metadata;
pub mod scene;
mod text2d;
mod text_units;
mod unit_resolution;
mod workplane;
mod world_input3d;

pub use attributes::{
    RawColorStyle, RawLayerStyle, RawLineTypeStyle, RawStyleTables, RawWidthStyle,
};
pub use basic2d::{root_curve_2d, Arc2D, Basic2DProfile, Circle2D, Curve2D, RootPrimitiveStyle};
pub use best_effort::{
    read_best_effort, read_best_effort_file, BestEffortAttributes, BestEffortBlock,
    BestEffortColor, BestEffortDiagnostic, BestEffortDimensionArc, BestEffortDrawing,
    BestEffortEntity, BestEffortFill, BestEffortGeometry, BestEffortHatchLine,
    BestEffortHatchPattern, BestEffortLayer, BestEffortLimits, BestEffortLineType,
    BestEffortReport, BestEffortSettings, BestEffortSource, BestEffortTable, BestEffortText,
    BestEffortWidth, DEFAULT_MAX_BEST_EFFORT_CURVE_POINTS, DEFAULT_MAX_BEST_EFFORT_DEPTH,
    DEFAULT_MAX_BEST_EFFORT_DIAGNOSTICS, DEFAULT_MAX_BEST_EFFORT_ENTITIES,
};
pub use block3d::{
    placed_block_lines_3d, Block3DProfile, BlockLineSelection3D, BlockReference3D,
    BlockReferenceEvidence3D, PlacedBlockLine3D, DEFAULT_MAX_BLOCK_LINES_3D,
};
pub use blocks::{
    BlockDefinition, BlockLibrary, PlacedBlockLine2D, BLOCK_HEADER_SIZE,
    DEFAULT_MAX_BLOCK_DEFINITIONS,
};
pub use container::{inspect_cfb, CfbEntry, CfbEntryKind, CfbInfo};
pub use coordinate_input_units::{NativeCoordinateInputUnit, NativeCoordinateInputUnitInfo};
pub use coordinate_measurement::CoordinateMeasurementContext;
pub use coordinate_space::{
    coordinate_space_context, qualify_lines_in_coordinate_space, CoordinateFrame,
    CoordinateSpaceContext, CoordinateSpaceEvidence, CoordinateSpaceProfile, FramedLineEndpoints,
    FramedLineSelection, SavedDraftState,
};
pub use dimension_scale::{DimensionScale, DimensionScaleProfile};
pub use document::{
    read, read_file, Document, DocumentArc, DocumentCircle, DocumentLine, DocumentPrimitive,
    DocumentText, Entity, OpaqueRegion, ReadDiagnostic, ReadError, ReadLimits, ReadOptions,
    ReadPolicy, ReadReport, ReadResult, ReadScope, ReadStatus, SourceRef,
};
pub use drawing_settings::DrawingSettings;
pub use effective_styles::{
    EffectiveBlockLineStyle, EffectiveLineStyle, EffectiveStyleIndices, ResolvedStyleIndex,
    StyleSource, StyleSourceKind, MAX_EFFECTIVE_STYLE_LINES,
};
pub use error::{DracadError, Result};
pub use format::{
    ContainerInfo, Diagnostic, DracadFormat, PayloadInfo, PayloadOffsetDomain, DRACAD_CLSID,
};
pub use header::{ObservedProfile, PayloadEndBasis, PayloadHeader, RECORD_SIZE};
pub use inline_packages::{
    InlinePackageLimits, InlinePackageRecord, InlineTableDomain, ScopedInlineLine2D,
    DEFAULT_MAX_INLINE_PACKAGE_DEPTH, MAX_INLINE_PACKAGE_DEPTH, MAX_INLINE_PACKAGE_PROVENANCE,
};
pub use inspect::{inspect, inspect_file};
pub use limits::{
    ParseLimits, DEFAULT_MAX_CFB_DEPTH, DEFAULT_MAX_CFB_DIRECTORY_SLOTS, DEFAULT_MAX_CFB_ENTRIES,
    DEFAULT_MAX_CFB_METADATA_SIZE, DEFAULT_MAX_DECOMPRESSED_SIZE, DEFAULT_MAX_FILE_SIZE,
    DEFAULT_MAX_RECORDS, DEFAULT_MAX_STREAM_SIZE, DEFAULT_MAX_TOTAL_STREAM_SIZE,
};
pub use line3d::LineRecord3D;
pub use mps::{MpsChunk, MpsCompression, MpsInfo};
pub use objects::{ObjectSpans, RawObjectSpan};
pub use origin_units::OriginUnitsCode;
pub use package_refs::PackageReference2D;
pub use print_settings::{RawPrintSettingField, RawPrintSettings};
pub use print_width::{
    resolve_nominal_print_width, PrintDeviceContext, PrintOutputScale, PrintWidthPolicy,
    PrintWidthResolution, PrintWidthStatus,
};
pub use qualification::{
    qualify_lines, Line2D, Line2DProfile, QualifiedLineSelection, UnresolvedLineSemantics,
    LINE2D_UNRESOLVED_SEMANTICS, LINE2D_V1_MAX_ANCESTORS, LINE2D_V1_MAX_INLINE_DEPTH,
    LINE2D_V1_MAX_REFERENCE_DEPTH,
};
pub use record_attributes::{
    decode_record_attributes, AttributeReference, AttributeReferenceKind, AttributedLineRecord2D,
    RecordAttributes,
};
pub use records::{
    scan_contents, scan_file, scan_records, LineCandidate2D, LineRecord2D, RawDocument, RawRecord,
    MAX_RECORDS_PER_PAGE,
};
pub use scene::{
    SceneAncestor, SceneAncestorKind, SceneCoordinateScope, SceneDefinitionSource, SceneLimits,
    SceneLine2D, DEFAULT_MAX_SCENE_DEPTH, MAX_SCENE_DEPTH, MAX_SCENE_PROVENANCE,
    MAX_SCENE_PROVENANCE_BYTES,
};
pub use text2d::{root_text_2d, TextLimits, TextRecord2D};
pub use text_units::{TextSizeUnit, TextSizeUnitInfo};
pub use unit_resolution::{
    CreationScaleProvenance, CreationScaleResolution, DeclaredCreationScale,
    PhysicalUnitResolution, PhysicalUnitUnresolvedReason, UnitResolution,
};
pub use workplane::{
    workplane_context, WorkplaneBasisSource, WorkplaneContext, WorkplaneDirection,
    WorkplaneEvidence, WorkplaneProfile,
};
pub use world_input3d::{world_input_3d_context, WorldInput3DContext, WorldInput3DProfile};

/// Cargo version shared by the Rust core and Python extension.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Return the Rust (SemVer) version. Python metadata uses its PEP 440 equivalent.
#[must_use]
pub const fn version() -> &'static str {
    VERSION
}
