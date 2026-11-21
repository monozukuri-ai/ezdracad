from typing import Final, Literal, TypedDict

DEFAULT_MAX_FILE_SIZE: Final[int]
DEFAULT_MAX_CFB_ENTRIES: Final[int]
DEFAULT_MAX_STREAM_SIZE: Final[int]
DEFAULT_MAX_TOTAL_STREAM_SIZE: Final[int]
DEFAULT_MAX_CFB_METADATA_SIZE: Final[int]
DEFAULT_MAX_CFB_DIRECTORY_SLOTS: Final[int]
DEFAULT_MAX_CFB_DEPTH: Final[int]
DEFAULT_MAX_RECORDS: Final[int]
DEFAULT_MAX_DECOMPRESSED_SIZE: Final[int]
MAX_RECORDS_PER_PAGE: Final[int]
MAX_INLINE_PACKAGE_DEPTH: Final[int]
MAX_INLINE_PACKAGE_PROVENANCE: Final[int]

LimitValues = tuple[int, int, int, int, int, int, int, int, int]
ReadValues = tuple[
    str,
    str,
    str,
    LimitValues,
    tuple[int, int, int],
    tuple[int, int, int, int, int],
    str | None,
    tuple[int, int],
]

class _DocumentHandle:
    def report(self) -> dict: ...
    def query_count(self, entity_type: str) -> int: ...
    def query_page(self, entity_type: str, start: int, count: int) -> list[dict]: ...
    def raw(self) -> _RawHandle: ...

def read_bytes(data: bytes, values: ReadValues) -> _DocumentHandle: ...
def read_file(path: str, values: ReadValues) -> _DocumentHandle: ...
def read_limit_defaults() -> tuple[int, int, int, int, int]: ...
def text_limit_defaults() -> tuple[int, int]: ...

BestEffortValues = tuple[int, int, int, int, int]

def best_effort_limit_defaults() -> BestEffortValues: ...
def read_best_effort_bytes(
    data: bytes, values: LimitValues, best_effort_values: BestEffortValues
) -> dict: ...
def read_best_effort_file(
    path: str, values: LimitValues, best_effort_values: BestEffortValues
) -> dict: ...

class DracadError(Exception): ...
class InvalidFormatError(DracadError): ...
class UnsupportedFormatError(DracadError): ...
class UnsupportedFeatureError(DracadError): ...

class LimitExceededError(DracadError):
    resource: str
    limit: int
    actual: int

class DracadIoError(DracadError): ...

class CfbRow(TypedDict):
    version: int
    root_clsid: str
    entries: list[tuple[str, Literal["storage", "stream"], int]]
    total_stream_size: int
    metadata_size: int
    directory_slots: int
    depth: int

class PayloadRow(TypedDict):
    stream_path: str | None
    size: int
    sha256: str
    marker_raw: int
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"]

class MpsChunkRow(TypedDict):
    encoded_offset: int
    encoded_size: int
    decoded_offset: int
    decoded_size: int

class MpsRow(TypedDict):
    stream_path: str
    stream_size: int
    stream_sha256: str
    wrapper_profile: Literal["unprotected_base_v1", "unprotected_extended_v1"]
    compression: Literal["zlib", "stored", "split_zlib"]
    decoded_size: int
    chunks: list[MpsChunkRow]

class ContainerRow(TypedDict):
    format: Literal["mpz", "mpx", "mps", "mps_candidate", "unknown_cfb"]
    file_size: int
    cfb: CfbRow | None
    payload: PayloadRow | None
    mps: MpsRow | None
    diagnostics: list[tuple[str, str]]

class _ContainerHandle:
    def metadata(self) -> ContainerRow: ...
    def contents(self) -> bytes: ...

def core_version() -> str: ...
def inspect_cfb_bytes(
    data: bytes,
    values: LimitValues,
) -> CfbRow: ...
def inspect_bytes(data: bytes, values: LimitValues) -> _ContainerHandle: ...
def inspect_file(path: str, values: LimitValues) -> _ContainerHandle: ...

class HeaderRow(TypedDict):
    profile: Literal[
        "observed_marker5_stride64",
        "observed_marker8_stride64",
        "observed_marker25_stride64",
    ]
    marker_raw: int
    record_start: int
    record_end: int
    record_count: int
    declared_nonterminal_records: int
    payload_end: int
    section_offsets: list[int]
    optional_offsets: list[int]
    payload_end_field_raw: int
    payload_end_basis: Literal["declared_field", "file_length"]

class ScanRow(TypedDict):
    header: HeaderRow
    format: Literal["mpz", "mpx", "mps"]
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"]
    file_size: int
    payload_size: int
    mps: MpsRow | None
    sha256: str
    stream_path: str | None
    line_candidate_count: int
    opaque_record_count: int
    tag_counts: dict[int, int]
    diagnostics: list[tuple[str, str]]

class _RawHandle:
    def placed_block_lines_3d(
        self, index: int, profile: str, max_lines: int
    ) -> dict: ...
    def world_input_3d_context(self, profile: str) -> _WorldInput3DHandle: ...
    def dimension_scale(self, index: int, profile: str) -> dict: ...
    def workplane_context(self, profile: str) -> _WorkplaneHandle: ...
    def coordinate_input_unit(self) -> _NativeCoordinateInputUnitHandle: ...
    def coordinate_space_context(self, profile: str) -> _CoordinateSpaceHandle: ...
    def qualified_lines_in_coordinate_space(
        self,
        index: int,
        line_profile: str,
        space_profile: str,
        frame: str,
        max_depth: int,
        max_nodes: int,
        max_lines: int,
    ) -> dict: ...
    def unit_resolution(self) -> _UnitResolutionHandle: ...
    def qualified_lines(
        self, index: int, profile: str, max_depth: int, max_nodes: int, max_lines: int
    ) -> dict: ...
    def origin_units_code(self) -> tuple[int, bytes, int]: ...
    def print_settings(self) -> _PrintSettingsHandle: ...
    def scene_lines(
        self, index: int, max_depth: int, max_nodes: int, max_lines: int
    ) -> list[dict]: ...
    def scene_lines_with_inline_scopes(
        self, index: int, max_depth: int, max_nodes: int, max_lines: int
    ) -> list[dict]: ...
    def coordinate_measurement_context(self) -> _CoordinateMeasurementHandle: ...
    def effective_line_style(self, index: int) -> EffectiveLineStyleRow: ...
    def record_attributes(self, index: int) -> RecordAttributeRow: ...
    def attributed_line_records(
        self, start: int, count: int
    ) -> list[tuple[LineRecordRow, RecordAttributeRow]]: ...
    def style_tables(self) -> _StyleHandle: ...
    def inline_package_lines(
        self, index: int, max_depth: int, max_nodes: int, max_lines: int
    ) -> list[InlineLineRow]: ...
    def drawing_settings(self) -> tuple[int, bytes, list[float], float, float]: ...
    def text_size_unit(self) -> tuple[int, bytes, int, Literal["mm", "pt"]]: ...
    def block_library(self, max_definitions: int) -> _BlockHandle: ...
    def metadata(self) -> ScanRow: ...
    def contents(self) -> bytes: ...
    def records(self, start: int, count: int) -> list[tuple[int, int, int]]: ...
    def object_spans(
        self, max_objects: int
    ) -> list[tuple[int, int, int, bytes, int, int]]: ...
    def record_bytes(self, index: int) -> bytes: ...
    def line_candidates(
        self, start: int, count: int
    ) -> list[tuple[int, int, list[float], list[float]]]: ...
    def line_records(
        self, start: int, count: int
    ) -> list[tuple[int, int, int, bytes, list[float], list[float], float, float]]: ...
    def line_records_3d(
        self, start: int, count: int
    ) -> list[tuple[int, int, int, bytes, list[float], list[float]]]: ...
    def package_references(
        self, start: int, count: int
    ) -> list[
        tuple[int, int, int, int, bytes, bytes, bytes, list[float], bytes, list[float]]
    ]: ...
    @staticmethod
    def apply_package_xy(
        anchor_raw: tuple[float, float],
        placement_raw: tuple[float, float, float, float, float, float],
        point: tuple[float, float],
    ) -> list[float]: ...

def scan_bytes(data: bytes, values: LimitValues) -> _RawHandle: ...
def scan_file(path: str, values: LimitValues) -> _RawHandle: ...

class _PrintSettingsHandle:
    def metadata(self) -> dict: ...
    def width_policy(
        self, device_context: str, automatic_output_denominator: float | None = None
    ) -> dict: ...

class _BlockReference3DHandle:
    def metadata(self) -> dict: ...
    def apply_xyz(
        self, point: tuple[float, float, float] | list[float]
    ) -> list[float]: ...

def resolve_nominal_print_width(
    tables: _StyleHandle,
    color_index: int,
    width_index: int,
    values: tuple[bool, bool, bool, float, str, float | None, str],
) -> dict: ...

LineRecordRow = tuple[int, int, int, bytes, list[float], list[float], float, float]
AttributeReferenceRow = tuple[Literal["index", "by_layer", "by_block"], int | None, int]
RecordAttributeRow = tuple[
    int,
    bytes,
    int,
    AttributeReferenceRow,
    AttributeReferenceRow,
    AttributeReferenceRow,
    int,
    int,
    int,
    bytes,
]

def decode_record_attributes(header: bytes) -> RecordAttributeRow: ...

InlineAncestorRow = tuple[int, int, int, bytes, bytes]
InlineLineRow = tuple[
    LineRecordRow,
    list[InlineAncestorRow],
    Literal["root_records", "block_definition_records"],
    int | None,
]

class BlockDefinitionRow(TypedDict):
    index: int
    header_offset: int
    header_bytes: bytes
    name_bytes: bytes
    record_start: int
    record_end: int
    declared_nonterminal_records: int
    record_count: int

class BlockLibraryRow(TypedDict):
    definitions: list[BlockDefinitionRow]
    suffix_offset: int | None
    suffix_size: int

class _BlockHandle:
    def effective_line_styles(
        self, reference_record_index: int, max_lines: int
    ) -> list[EffectiveBlockLineStyleRow]: ...
    def inline_package_lines(
        self,
        definition: int,
        index: int,
        max_depth: int,
        max_nodes: int,
        max_lines: int,
    ) -> list[InlineLineRow]: ...
    def metadata(self) -> BlockLibraryRow: ...
    def suffix_bytes(self) -> bytes: ...
    def resolve(self, name: bytes) -> int: ...
    def record_bytes(self, definition: int, index: int) -> bytes: ...
    def records(
        self, definition: int, start: int, count: int
    ) -> list[tuple[int, int, int]]: ...
    def planar_lines(self, definition: int, max_lines: int) -> list[LineRecordRow]: ...
    def placed_lines(
        self, reference: int, max_lines: int
    ) -> list[tuple[int, int, LineRecordRow, list[float], list[float]]]: ...

class CoordinateMeasurementRow(TypedDict):
    paper_scale_offset: int
    paper_scale_bytes: bytes
    paper_scale_factor: float
    annotation_ratio_offset: int
    annotation_ratio_bytes: bytes
    annotation_ratio: float
    drawing_denominator: float
    annotation_denominator: float

class _CoordinateMeasurementHandle:
    def metadata(self) -> CoordinateMeasurementRow: ...
    def length_on_paper_mm(self, raw_length: float) -> float: ...
    def length_at_current_annotation_mm(self, raw_length: float) -> float: ...

class StyleSourceRow(TypedDict):
    kind: Literal["record", "block_placement", "layer", "color", "line_type", "width"]
    table_domain: str
    index: int
    definition_index: int | None
    payload_offset: int
    raw_bytes: bytes

class ResolvedStyleIndexRow(TypedDict):
    index: int
    sources: list[StyleSourceRow]

class EffectiveStyleIndicesRow(TypedDict):
    color: ResolvedStyleIndexRow
    line_type: ResolvedStyleIndexRow
    width: ResolvedStyleIndexRow

class EffectiveLineStyleRow(TypedDict):
    source: tuple[LineRecordRow, RecordAttributeRow]
    styles: EffectiveStyleIndicesRow

class EffectiveBlockLineStyleRow(EffectiveLineStyleRow):
    reference_record_index: int
    definition_index: int
    start: list[float]
    end: list[float]

class LayerStyleRow(TypedDict):
    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    reserved_word_raw: int
    color_index_raw: int
    line_type_index_raw: int
    width_index_raw: int
    stored_index_raw: int

class ColorStyleRow(TypedDict):
    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    pen_width_hundredths_raw: int
    pen_width_value: float
    rgb: list[int]
    rgb_encoding_raw: int
    output_rgb: list[int]
    output_rgb_encoding_raw: int

class LineTypeStyleRow(TypedDict):
    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    pen_style_raw: int
    pen_definition_hundredths_raw: list[int]
    pen_definition_values: list[float]

class WidthStyleRow(TypedDict):
    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    pen_width_hundredths_raw: int
    pen_width_value: float

class StyleTablesRow(TypedDict):
    layer_count: int
    color_count: int
    line_type_count: int
    width_count: int
    shared_suffix_offset: int
    shared_suffix_size: int
    extension_header_offsets: list[int]
    extension_header_bytes: list[bytes]

class _StyleHandle:
    def metadata(self) -> StyleTablesRow: ...
    def suffix_bytes(self) -> bytes: ...
    def layer(self, index: int) -> LayerStyleRow: ...
    def color(self, index: int) -> ColorStyleRow: ...
    def line_type(self, index: int) -> LineTypeStyleRow: ...
    def width(self, index: int) -> WidthStyleRow: ...

class UnitResolutionRow(TypedDict):
    physical_unit_status: Literal["unresolved"]
    physical_unit_reason: Literal["stored_coordinate_unit_not_established"]
    original_creation_scale: Literal["unavailable_from_decoded_fields"]
    origin_units_offset: int
    origin_units_bytes: bytes
    origin_units_code: int

class _UnitResolutionHandle:
    def metadata(self) -> UnitResolutionRow: ...
    def current_measurement(self) -> _CoordinateMeasurementHandle: ...
    def coordinate_input_unit(self) -> _NativeCoordinateInputUnitHandle: ...

class DeclaredCreationScaleRow(TypedDict):
    original_drawing_denominator: float
    original_annotation_denominator: float
    original_annotation_ratio: float
    provenance: Literal["caller_declared_unverified"]

class _DeclaredCreationScaleHandle:
    def __init__(
        self,
        original_drawing_denominator: float,
        original_annotation_denominator: float,
    ) -> None: ...
    def metadata(self) -> DeclaredCreationScaleRow: ...
    def length_mm(self, stored_length: float) -> float: ...

class _CoordinateSpaceHandle:
    def metadata(self) -> dict: ...
    def input_unit(self) -> _NativeCoordinateInputUnitHandle: ...
    def from_root_stored_xy(
        self, value: tuple[float, float] | list[float], frame: str
    ) -> list[float]: ...
    def to_root_stored_xy(
        self, value: tuple[float, float] | list[float], frame: str
    ) -> list[float]: ...

class _WorkplaneHandle:
    def metadata(self) -> dict: ...
    def from_world_stored_xyz(
        self, value: tuple[float, float, float] | list[float]
    ) -> list[float]: ...
    def to_world_stored_xyz(
        self, value: tuple[float, float, float] | list[float]
    ) -> list[float]: ...

class _WorldInput3DHandle:
    def metadata(self) -> dict: ...
    def coordinate_context(self) -> _CoordinateSpaceHandle: ...
    def input_mm_from_world_stored_xyz(
        self, value: tuple[float, float, float] | list[float]
    ) -> list[float]: ...
    def world_stored_xyz_from_input_mm(
        self, value: tuple[float, float, float] | list[float]
    ) -> list[float]: ...

class NativeCoordinateInputUnitRow(TypedDict):
    origin_units_offset: int
    origin_units_bytes: bytes
    origin_units_code: int
    unit: Literal["mm", "cm", "m", "km", "inch", "feet", "yard", "unknown"]
    native_display_factor: float | None

class _NativeCoordinateInputUnitHandle:
    def metadata(self) -> NativeCoordinateInputUnitRow: ...
    def to_native_mm(self, value: float) -> float: ...
    def from_native_mm(self, value: float) -> float: ...
