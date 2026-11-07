"""MPZ/MPX/MPS inspection and restricted payload-local/package-parent decoding."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import asdict, dataclass, field
from os import PathLike, fspath
from types import MappingProxyType
from typing import Literal

from . import _core
from .block3d import Block3DProfile, BlockLineSelection3D
from .block3d import selection as block3d_selection
from .block3d import str_value as block3d_profile_value
from .coordinate_input_units import NativeCoordinateInputUnitInfo
from .coordinate_measurement import CoordinateMeasurementContext
from .coordinate_space import (
    CoordinateFrame,
    CoordinateSpaceContext,
    CoordinateSpaceProfile,
    FramedLineSelection,
    _framed_selection,
    str_value,
)
from .dimension_scale import (
    DimensionScale,
    DimensionScaleProfile,
    _dimension_scale,
)
from .dimension_scale import (
    _profile_value as dimension_scale_profile_value,
)
from .effective_styles import (
    EffectiveBlockLineStyle,
    EffectiveLineStyle,
    _block_styles,
    _line_style,
)
from .line3d import LineRecord3D
from .options import ParseLimits
from .origin_units import OriginUnitsCode
from .print_settings import PrintSettings
from .qualification import Line2DProfile, QualifiedLineSelection, _qualified_selection
from .scene import SceneLimits, SceneLine2D, _scene_lines
from .styles import StyleTables
from .unit_resolution import UnitResolution
from .workplane import (
    WorkplaneContext,
    WorkplaneProfile,
)
from .workplane import (
    str_value as workplane_profile_value,
)
from .world_input3d import WorldInput3DContext, WorldInput3DProfile
from .world_input3d import str_value as world_input_3d_profile_value


@dataclass(frozen=True)
class CfbEntry:
    path: str
    kind: Literal["storage", "stream"]
    size: int


@dataclass(frozen=True)
class CfbInfo:
    """Declared CFB metadata. Stream payloads have not been validated."""

    version: int
    root_clsid: str
    entries: tuple[CfbEntry, ...]
    total_stream_size: int
    metadata_size: int
    directory_slots: int
    depth: int


def inspect_cfb(data: bytes, *, limits: ParseLimits | None = None) -> CfbInfo:
    """Inspect bytes through the Rust CFB backend, without classifying a drawing.

    This generic CFB API does not identify a DRA-CAD drawing.
    """
    if not isinstance(data, bytes):
        raise TypeError("data must be bytes")
    if limits is None:
        limits = ParseLimits()
    if not isinstance(limits, ParseLimits):
        raise TypeError("limits must be ParseLimits")
    row = _core.inspect_cfb_bytes(data, limits._values())
    return _cfb_info(row)


def _cfb_info(row: _core.CfbRow) -> CfbInfo:
    return CfbInfo(
        version=row["version"],
        root_clsid=row["root_clsid"],
        entries=tuple(CfbEntry(*entry) for entry in row["entries"]),
        total_stream_size=row["total_stream_size"],
        metadata_size=row["metadata_size"],
        directory_slots=row["directory_slots"],
        depth=row["depth"],
    )


@dataclass(frozen=True)
class Diagnostic:
    code: str
    message: str


@dataclass(frozen=True)
class PayloadInfo:
    stream_path: str | None
    size: int
    sha256: str
    marker_raw: int
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"] = "cfb_stream"

    def __post_init__(self) -> None:
        _source_domain(self.stream_path, self.offset_domain)


@dataclass(frozen=True)
class MpsChunk:
    """One encoded range and its decoded range; offsets use separate domains."""

    encoded_offset: int
    encoded_size: int
    decoded_offset: int
    decoded_size: int


@dataclass(frozen=True)
class MpsInfo:
    """Verified unprotected wrapper provenance, separate from decoded offsets.

    encoded_offset refers to the source CFB stream. decoded_offset refers to
    the drawing payload returned by read_payload(). Neither is a file offset.
    """

    stream_path: str
    stream_size: int
    stream_sha256: str
    wrapper_profile: Literal["unprotected_base_v1", "unprotected_extended_v1"]
    compression: Literal["zlib", "stored", "split_zlib"]
    decoded_size: int
    chunks: tuple[MpsChunk, ...]


def _mps_info(row: _core.MpsRow | None) -> MpsInfo | None:
    if row is None:
        return None
    if (
        not isinstance(row["stream_path"], str)
        or row["stream_path"].lower() != "/securecontents"
        or row["wrapper_profile"]
        not in ("unprotected_base_v1", "unprotected_extended_v1")
        or row["compression"] not in ("zlib", "stored", "split_zlib")
    ):
        raise ValueError("MPS wrapper provenance differs across language boundary")
    chunks = tuple(MpsChunk(**chunk) for chunk in row["chunks"])
    decoded_end = 0
    encoded_end = 0
    for chunk in chunks:
        if (
            chunk.encoded_offset < encoded_end
            or chunk.encoded_size < 0
            or chunk.encoded_offset + chunk.encoded_size > row["stream_size"]
            or chunk.decoded_offset != decoded_end
            or chunk.decoded_size < 0
        ):
            raise ValueError("MPS chunk ranges differ across language boundary")
        encoded_end = chunk.encoded_offset + chunk.encoded_size
        decoded_end += chunk.decoded_size
    if not chunks or decoded_end != row["decoded_size"]:
        raise ValueError("MPS decoded size differs across language boundary")
    return MpsInfo(**{**row, "chunks": chunks})


def _mps_dict(info: MpsInfo) -> dict:
    row = asdict(info)
    row["chunks"] = list(row["chunks"])
    return row


@dataclass(frozen=True)
class ContainerInfo:
    """Input identity and optional CFB metadata, with one owned Rust payload."""

    format: Literal["mpz", "mpx", "mps", "mps_candidate", "unknown_cfb"]
    file_size: int
    cfb: CfbInfo | None
    payload: PayloadInfo | None
    diagnostics: tuple[Diagnostic, ...]
    _handle: _core._ContainerHandle = field(repr=False, compare=False)
    mps: MpsInfo | None = None

    def read_contents(self) -> bytes:
        """Return retained MPZ/MPX bytes or a verified decoded MPS payload.

        The source file is not reopened. Check payload.offset_domain before
        interpreting offsets. Unknown/protected MPS wrappers remain unsupported.
        """
        return self._handle.contents()

    def read_payload(self) -> bytes:
        """Format-neutral alias of read_contents(); the owned snapshot stays unchanged."""
        return self.read_contents()

    def to_dict(self) -> dict:
        """JSON-ready metadata without payload bytes or the native handle."""
        cfb = None if self.cfb is None else asdict(self.cfb)
        if cfb is not None:
            cfb["entries"] = list(cfb["entries"])
        return {
            "schema_version": 1,
            "format": self.format,
            "file_size": self.file_size,
            "cfb": cfb,
            "payload": None if self.payload is None else asdict(self.payload),
            "geometry_status": "not_decoded",
            "coordinate_system": None,
            "units": None,
            "diagnostics": [asdict(item) for item in self.diagnostics],
            **({"mps": _mps_dict(self.mps)} if self.mps is not None else {}),
        }


def _options(limits: ParseLimits | None) -> ParseLimits:
    if limits is None:
        return ParseLimits()
    if not isinstance(limits, ParseLimits):
        raise TypeError("limits must be ParseLimits")
    return limits


def _source_domain(stream_path: str | None, offset_domain: str) -> None:
    if offset_domain in ("file", "decoded_payload") and stream_path is None:
        return
    if offset_domain == "cfb_stream" and isinstance(stream_path, str) and stream_path:
        return
    raise ValueError("Payload source domain differs across language boundary")


def _container(handle: _core._ContainerHandle) -> ContainerInfo:
    row = handle.metadata()
    if row["format"] == "mpx":
        if row["cfb"] is not None or row["payload"] is None:
            raise ValueError("MPX input must have a file payload and no CFB metadata")
        if row["payload"]["offset_domain"] != "file":
            raise ValueError("MPX offsets must refer to the file")
    elif row["format"] == "mpz":
        if row["cfb"] is None or row["payload"] is None:
            raise ValueError("MPZ input must have CFB metadata and a stream payload")
        if row["payload"]["offset_domain"] != "cfb_stream":
            raise ValueError("MPZ offsets must refer to a CFB stream")
    elif row["format"] == "mps":
        if row["cfb"] is None or row["payload"] is None or row["mps"] is None:
            raise ValueError(
                "MPS input must have CFB, wrapper and decoded payload metadata"
            )
        if row["payload"]["offset_domain"] != "decoded_payload":
            raise ValueError("MPS offsets must refer to the decoded payload")
        if row["mps"]["decoded_size"] != row["payload"]["size"]:
            raise ValueError(
                "MPS wrapper and payload sizes differ across language boundary"
            )
        if not any(
            path == row["mps"]["stream_path"]
            and kind == "stream"
            and size == row["mps"]["stream_size"]
            for path, kind, size in row["cfb"]["entries"]
        ):
            raise ValueError("MPS source stream differs across language boundary")
    elif row["format"] in ("mps_candidate", "unknown_cfb"):
        if row["cfb"] is None or row["payload"] is not None:
            raise ValueError("Unsupported CFB input has inconsistent metadata")
    else:
        raise ValueError("Unknown input carrier across language boundary")
    if row["format"] != "mps" and row["mps"] is not None:
        raise ValueError("Unexpected MPS provenance for a different input carrier")
    return ContainerInfo(
        format=row["format"],
        file_size=row["file_size"],
        cfb=None if row["cfb"] is None else _cfb_info(row["cfb"]),
        payload=None if row["payload"] is None else PayloadInfo(**row["payload"]),
        diagnostics=tuple(Diagnostic(*item) for item in row["diagnostics"]),
        _handle=handle,
        mps=_mps_info(row["mps"]),
    )


def inspect(data: bytes, *, limits: ParseLimits | None = None) -> ContainerInfo:
    """Identify supported MPZ/MPX/MPS bytes and retain the drawing payload."""
    if not isinstance(data, bytes):
        raise TypeError("data must be bytes")
    return _container(_core.inspect_bytes(data, _options(limits)._values()))


def inspectfile(
    path: str | PathLike[str], *, limits: ParseLimits | None = None
) -> ContainerInfo:
    """Inspect a regular file with metadata and actual-byte limits in Rust."""
    value = fspath(path)
    if not isinstance(value, str):
        raise TypeError("path must be str or PathLike[str]")
    return _container(_core.inspect_file(value, _options(limits)._values()))


MAX_RECORDS_PER_PAGE = _core.MAX_RECORDS_PER_PAGE
MAX_INLINE_PACKAGE_DEPTH = _core.MAX_INLINE_PACKAGE_DEPTH
MAX_INLINE_PACKAGE_PROVENANCE = _core.MAX_INLINE_PACKAGE_PROVENANCE


@dataclass(frozen=True)
class PayloadHeader:
    """Payload-relative boundaries; MPX EOF and MPZ declared end stay distinct."""

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
    section_offsets: tuple[int, ...]
    optional_offsets: tuple[int, ...]
    payload_end_field_raw: int
    payload_end_basis: Literal["declared_field", "file_length"]


@dataclass(frozen=True)
class RawRecord:
    index: int
    payload_offset: int
    tag: int


@dataclass(frozen=True)
class DrawingSettings:
    """Observed header metadata, without applying origin or scale to geometry.

    The exact payload-relative +0x140..+0x17f range includes opaque gap bytes.
    drawing_scale is the reciprocal of the stored paper_scale_factor. Units,
    upstream context and normalized drawing coordinates remain unresolved.
    """

    payload_offset: int
    header_bytes: bytes
    drawing_origin: tuple[float, float]
    paper_scale_factor: float
    drawing_scale: float
    units: None = field(default=None, init=False)


@dataclass(frozen=True)
class TextSizeUnitInfo:
    """Observed font-size unit, independently of geometry or MPXL Origin.Units.

    This controls text size input/display, applies no conversion, and preserves
    the exact selector bytes. Physical drawing units remain unresolved.
    """

    payload_offset: int
    selector_bytes: bytes
    raw_selector: int
    unit: Literal["mm", "pt"]


@dataclass(frozen=True)
class RawObjectSpan:
    """One primary header and its opaque interior slots in the root raw table.

    Payload-relative offset and exclusive end record index. The terminal is
    excluded. This is one-level structure, without interpreting child slots,
    logical root membership, coordinates, units or document completeness.
    """

    record_index: int
    payload_offset: int
    tag_raw: int
    header_bytes: bytes
    record_count: int
    end_record_index: int


@dataclass(frozen=True)
class LineCandidate2D:
    """Provisional tag 0x0002 XY, without resolved units, Z or transforms."""

    record_index: int
    payload_offset: int
    start: tuple[float, float]
    end: tuple[float, float]


@dataclass(frozen=True)
class LineRecord2D:
    """Recognized record-local fields; units and parent context are unresolved.

    line_records() recognizes exact tags 0x0002/0x0402. The separate attributed
    API can return other explicitly qualified colored-line variants. Header
    bytes stay exact and no effective attributes are applied to this source.
    Elevation is a common height and thickness is not an endpoint Z. No origin,
    unit or parent placement transform is applied; this is not a drawing entity.
    """

    record_index: int
    payload_offset: int
    tag_raw: int
    header_bytes: bytes
    start: tuple[float, float]
    end: tuple[float, float]
    elevation: float
    thickness: float
    coordinate_space: Literal["record_local"] = field(
        default="record_local", init=False
    )
    units: None = field(default=None, init=False)
    parent_context: None = field(default=None, init=False)


@dataclass(frozen=True)
class AttributeReference:
    """A file-local style index or an unresolved inheritance selector."""

    kind: Literal["index", "by_layer", "by_block"]
    index: int | None
    raw_code: int


@dataclass(frozen=True)
class RecordAttributes:
    """Observed header fields; no effective or print/render style is resolved.

    Indices belong to this drawing, not DXF ACI/lineweight. Flags, material and
    tail bytes remain raw. Layer zero does not by itself resolve inheritance.
    """

    tag_raw: int
    header_bytes: bytes
    layer_index: int
    color: AttributeReference
    line_type: AttributeReference
    width: AttributeReference
    material_index_raw: int
    group_index: int
    flags_raw: int
    opaque_tail_bytes: bytes


@dataclass(frozen=True)
class AttributedLineRecord2D:
    """Record-local finite line fields with file-local attribute references."""

    source: LineRecord2D
    attributes: RecordAttributes
    units: None = field(default=None, init=False)
    coordinate_space: Literal["record_local"] = field(
        default="record_local", init=False
    )


def _record_attributes(row):
    tag, header, layer, color, line_type, width, material, group, flags, tail = row
    return RecordAttributes(
        tag,
        header,
        layer,
        AttributeReference(*color),
        AttributeReference(*line_type),
        AttributeReference(*width),
        material,
        group,
        flags,
        tail,
    )


def decode_record_attributes(header_bytes: bytes) -> RecordAttributes:
    """Explicitly decode a 16-byte observed current-format header in Rust.

    No file/profile is supplied here. Document methods add profile checks and
    reject dependency sentinels in older marker-5/8 layouts. Unknown tags/flags
    and reference codes fail instead of being masked or mapped to DXF values.
    """
    if not isinstance(header_bytes, bytes):
        raise TypeError("header_bytes must be bytes")
    return _record_attributes(_core.decode_record_attributes(header_bytes))


@dataclass(frozen=True)
class PackageReference2D:
    """Exact root-slot tag-6/FE payload, without logical root membership.

    Internal names are opaque byte keys, not necessarily DXF names. Raw headers,
    including bytes after the name's first NUL, remain intact. Units and parent
    context are unresolved. apply_xy() is an explicit restricted calculation
    into a package-parent frame, not normalized WCS or document geometry.
    """

    record_index: int
    payload_offset: int
    continuation_index: int
    continuation_offset: int
    header_bytes: bytes
    name_field_bytes: bytes
    name_key: bytes
    anchor_raw: tuple[float, float]
    continuation_header_bytes: bytes
    placement_raw: tuple[float, float, float, float, float, float]
    tag_raw: Literal[6] = field(default=6, init=False)
    coordinate_space: Literal["package_parent"] = field(
        default="package_parent", init=False
    )
    units: None = field(default=None, init=False)
    parent_context: None = field(default=None, init=False)

    def apply_xy(self, point: tuple[float, float]) -> tuple[float, float]:
        """Apply only zero raw anchors/third field and positive uniform scale.

        Unsupported states fail instead of guessing a transform. All input and
        output numbers must be finite. The computation is performed in Rust.
        """
        result = _core._RawHandle.apply_package_xy(
            self.anchor_raw, self.placement_raw, point
        )
        return result[0], result[1]


@dataclass(frozen=True)
class InlinePackageRecord:
    """One qualified source package; index is table-local, offset is Contents.

    Header/body bytes stay exact. No MPXL packageflag, effective attributes or
    package transform is inferred from them.
    """

    record_index: int
    payload_offset: int
    record_count: int
    header_bytes: bytes
    body_bytes: bytes


@dataclass(frozen=True)
class ScopedInlineLine2D:
    """A record-local line with complete outer-to-inner package provenance.

    The table domain and optional definition index disambiguate local indices.
    No origin/scale/placement/unit or effective-attribute transform is applied.
    """

    source: LineRecord2D
    ancestors: tuple[InlinePackageRecord, ...]
    table_domain: Literal["root_records", "block_definition_records"]
    definition_index: int | None
    units: None = field(default=None, init=False)
    coordinate_space: Literal["record_local"] = field(
        default="record_local", init=False
    )


def _inline_lines(rows):
    result = []
    for source, ancestors, domain, definition_index in rows:
        index, offset, tag, header, start, end, elevation, thickness = source
        result.append(
            ScopedInlineLine2D(
                LineRecord2D(
                    index,
                    offset,
                    tag,
                    header,
                    (start[0], start[1]),
                    (end[0], end[1]),
                    elevation,
                    thickness,
                ),
                tuple(InlinePackageRecord(*row) for row in ancestors),
                domain,
                definition_index,
            )
        )
    return tuple(result)


def _unsigned(value: int, name: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise TypeError(f"{name} must be an integer")
    if not 0 <= value < 1 << 64:
        raise ValueError(f"{name} must fit an unsigned 64-bit integer")
    return value


@dataclass(frozen=True)
class BlockDefinition:
    """A bounded definition-table entry, with an opaque internal byte name."""

    index: int
    header_offset: int
    header_bytes: bytes
    name_bytes: bytes
    record_start: int
    record_end: int
    declared_nonterminal_records: int
    record_count: int


@dataclass(frozen=True)
class BlockLineRecord2D:
    """A direct zero-height/thickness line in one definition's local frame."""

    definition_index: int
    record_index: int
    payload_offset: int
    tag_raw: int
    header_bytes: bytes
    start: tuple[float, float]
    end: tuple[float, float]
    elevation: float
    thickness: float
    coordinate_space: Literal["block_definition_local"] = field(
        default="block_definition_local", init=False
    )
    units: None = field(default=None, init=False)


@dataclass(frozen=True)
class PlacedBlockLine2D:
    """One explicit placement; upstream context, styles and units stay unresolved."""

    reference_record_index: int
    source: BlockLineRecord2D
    start: tuple[float, float]
    end: tuple[float, float]
    coordinate_space: Literal["package_parent"] = field(
        default="package_parent", init=False
    )
    units: None = field(default=None, init=False)
    parent_context: None = field(default=None, init=False)


def _block_line(definition_index, row):
    index, offset, tag, header, start, end, elevation, thickness = row
    return BlockLineRecord2D(
        definition_index,
        index,
        offset,
        tag,
        header,
        (start[0], start[1]),
        (end[0], end[1]),
        elevation,
        thickness,
    )


@dataclass(frozen=True)
class BlockLibrary:
    """An immutable Contents snapshot. No file is reopened for later requests.

    Definition/header/suffix bytes are retained. Name bytes are internal keys,
    not necessarily DXF display names. Planar/placed requests reject unknown
    compounds and unsupported coordinate states rather than flattening them.
    This is not a normalized drawing or a document-completeness certificate.
    """

    definitions: tuple[BlockDefinition, ...]
    suffix_offset: int | None
    suffix_size: int
    _handle: _core._BlockHandle = field(repr=False, compare=False)

    def effective_line_styles(
        self, reference_record_index: int, *, max_lines: int = MAX_RECORDS_PER_PAGE
    ) -> tuple[EffectiveBlockLineStyle, ...]:
        """Resolve direct block-leaf native indices with complete source chains.

        Supports one primary root reference and the restricted planar placement
        subset. Inline compounds, visibility and print policy are not resolved.
        """
        return _block_styles(
            self._handle.effective_line_styles(
                _unsigned(reference_record_index, "reference_record_index"),
                _unsigned(max_lines, "max_lines"),
            )
        )

    def inline_package_lines(
        self,
        definition_index: int,
        record_index: int,
        *,
        max_depth: int = 32,
        max_nodes: int = MAX_RECORDS_PER_PAGE,
        max_lines: int = MAX_RECORDS_PER_PAGE,
    ) -> tuple[ScopedInlineLine2D, ...]:
        """Read one primary package subtree; no block placement is applied.

        Local indices are in this definition. Unknown child variants fail the
        whole request; direct-only planar_lines()/placed_lines() stay unchanged.
        """
        return _inline_lines(
            self._handle.inline_package_lines(
                _unsigned(definition_index, "definition_index"),
                _unsigned(record_index, "record_index"),
                _unsigned(max_depth, "max_depth"),
                _unsigned(max_nodes, "max_nodes"),
                _unsigned(max_lines, "max_lines"),
            )
        )

    def suffix_bytes(self) -> bytes:
        return self._handle.suffix_bytes()

    def resolve(self, name_bytes: bytes) -> BlockDefinition:
        if not isinstance(name_bytes, bytes):
            raise TypeError("name_bytes must be bytes; no encoding is guessed")
        return self.definitions[self._handle.resolve(name_bytes)]

    def record_bytes(self, definition_index: int, record_index: int) -> bytes:
        return self._handle.record_bytes(
            _unsigned(definition_index, "definition_index"),
            _unsigned(record_index, "record_index"),
        )

    def records(
        self, definition_index: int, start: int = 0, count: int = 1024
    ) -> tuple[RawRecord, ...]:
        """A capped raw page. Record indices are local to the named definition."""
        return tuple(
            RawRecord(*row)
            for row in self._handle.records(
                _unsigned(definition_index, "definition_index"),
                _unsigned(start, "start"),
                _unsigned(count, "count"),
            )
        )

    def planar_lines(
        self, definition_index: int, *, max_lines: int = MAX_RECORDS_PER_PAGE
    ) -> tuple[BlockLineRecord2D, ...]:
        rows = self._handle.planar_lines(
            _unsigned(definition_index, "definition_index"),
            _unsigned(max_lines, "max_lines"),
        )
        return tuple(_block_line(definition_index, row) for row in rows)

    def placed_lines(
        self, reference_record_index: int, *, max_lines: int = MAX_RECORDS_PER_PAGE
    ) -> tuple[PlacedBlockLine2D, ...]:
        """Resolve an actual root-slot reference and apply the supported XY subset.

        Zero raw anchor/third field, positive uniform scale and direct zero-Z,
        zero-thickness lines are required. All output is package_parent, not WCS.
        The whole request fails on an unsupported definition object or state.
        """
        rows = self._handle.placed_lines(
            _unsigned(reference_record_index, "reference_record_index"),
            _unsigned(max_lines, "max_lines"),
        )
        return tuple(
            PlacedBlockLine2D(
                ref,
                _block_line(definition, source),
                (start[0], start[1]),
                (end[0], end[1]),
            )
            for ref, definition, source, start, end in rows
        )


@dataclass(frozen=True)
class RawScan:
    """Validated raw table with paged access to one immutable Rust payload.

    The final 0x00ff record is included in record_count/tag_counts. Other tags
    are retained as opaque bytes unless exactly 0x0002 for the candidate API.
    Additional record-local fields are decoded only on request by line_records.
    No drawing completeness or normalized geometry is established by a scan.
    """

    header: PayloadHeader
    format: Literal["mpz", "mpx", "mps"]
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"]
    file_size: int
    payload_size: int
    sha256: str
    stream_path: str | None
    line_candidate_count: int
    opaque_record_count: int
    tag_counts: Mapping[int, int]
    diagnostics: tuple[Diagnostic, ...]
    _handle: _core._RawHandle = field(repr=False, compare=False)
    mps: MpsInfo | None = None

    def record_attributes(self, record_index: int) -> RecordAttributes:
        """Read one qualified header, preserving unresolved dependency selectors."""
        return _record_attributes(
            self._handle.record_attributes(_unsigned(record_index, "record_index"))
        )

    def attributed_line_records(
        self,
        record_start: int = 0,
        record_count: int = 1024,
    ) -> tuple[AttributedLineRecord2D, ...]:
        """Known line variants in a capped raw window; no style is applied.

        Exact 0x0002/0x0402/0x2402/0x4402 headers only. All six source floats are
        checked; any bad header/field fails this whole page. Dependency
        encodings are qualified only for the current marker-25 profile.
        Legacy line_records() and tag-2 candidate contracts remain unchanged.
        """
        rows = self._handle.attributed_line_records(
            _unsigned(record_start, "record_start"),
            _unsigned(record_count, "record_count"),
        )
        result = []
        for source, attributes in rows:
            index, offset, tag, header, start, end, elevation, thickness = source
            result.append(
                AttributedLineRecord2D(
                    LineRecord2D(
                        index,
                        offset,
                        tag,
                        header,
                        (start[0], start[1]),
                        (end[0], end[1]),
                        elevation,
                        thickness,
                    ),
                    _record_attributes(attributes),
                )
            )
        return tuple(result)

    def style_tables(self) -> StyleTables:
        """Read bounded file-local palette/style tables, without effective styles.

        Rust preserves rows, special suffix entries and opaque bytes. Unknown
        layouts fail the whole request; indices are not DXF ACI/lineweight.
        """
        return StyleTables(self._handle.style_tables())

    def effective_line_style(self, record_index: int) -> EffectiveLineStyle:
        """Resolve explicit/ByLayer native indices for a primary root line.

        A standalone ByBlock selector and an interior slot fail explicitly.
        Results do not resolve visibility, printed width or general appearance.
        """
        return _line_style(
            self._handle.effective_line_style(_unsigned(record_index, "record_index"))
        )

    def coordinate_measurement_context(self) -> CoordinateMeasurementContext:
        """Read the checked current-annotation/paper measurement convention.

        This is an explicit contextual conversion API. Existing coordinate
        results retain units=None, and printer overrides are not applied.
        """
        return CoordinateMeasurementContext(
            self._handle.coordinate_measurement_context()
        )

    def print_settings(self) -> PrintSettings:
        """Read saved marker-25 print policy with the original field bytes.

        Unknown switches remain inspectable; width-policy conversion rejects
        them. Native driver override always keeps the output width unresolved.
        """
        return PrintSettings(self._handle.print_settings())

    def origin_units_code(self) -> OriginUnitsCode:
        """Read the raw marker-25 Origin.Units code without a physical-unit mapping."""
        return OriginUnitsCode(*self._handle.origin_units_code())

    def unit_resolution(self) -> UnitResolution:
        """Inspect current scales and the unresolved physical unit/creation history."""
        return UnitResolution(self._handle.unit_resolution())

    def coordinate_input_unit(self) -> NativeCoordinateInputUnitInfo:
        """Read the current numeric input unit, retaining native conversion factors."""
        return NativeCoordinateInputUnitInfo(self._handle.coordinate_input_unit())

    def dimension_scale(
        self, record_index: int, *, profile: DimensionScaleProfile | str
    ) -> DimensionScale:
        """Read one numeric mm dimension's local ratio, retaining original D/A unknown."""
        return _dimension_scale(
            self._handle.dimension_scale(
                _unsigned(record_index, "record_index"),
                dimension_scale_profile_value(profile),
            )
        )

    def coordinate_space_context(
        self, *, profile: CoordinateSpaceProfile | str
    ) -> CoordinateSpaceContext:
        """Select a named saved-context point map, without declaring general WCS."""
        return CoordinateSpaceContext(
            self._handle.coordinate_space_context(str_value(profile))
        )

    def workplane_context(self, *, profile: WorkplaneProfile | str) -> WorkplaneContext:
        """Map explicit stored-world/workplane points without declaring physical units."""
        return WorkplaneContext(
            self._handle.workplane_context(workplane_profile_value(profile))
        )

    def placed_block_lines_3d(
        self,
        index: int,
        *,
        profile: Block3DProfile | str,
        max_lines: int = 4096,
    ) -> BlockLineSelection3D:
        """Atomically place a supported primary root 3D block's direct lines.

        The explicit profile admits positive XYZ scale and Rx→Ry→Rz rotation.
        Units, effective styles, general WCS and completeness remain unresolved.
        """
        return block3d_selection(
            self._handle.placed_block_lines_3d(
                _unsigned(index, "record_index"),
                block3d_profile_value(profile),
                _unsigned(max_lines, "max_lines"),
            )
        )

    def world_input_3d_context(
        self, *, profile: WorldInput3DProfile | str
    ) -> WorldInput3DContext:
        """Select the explicit loaded-file current 3D absolute-input convention."""
        return WorldInput3DContext(
            self._handle.world_input_3d_context(world_input_3d_profile_value(profile))
        )

    def qualified_lines_in_coordinate_space(
        self,
        record_index: int,
        *,
        line_profile: Line2DProfile,
        space_profile: CoordinateSpaceProfile | str,
        frame: CoordinateFrame | str,
        limits: SceneLimits | None = None,
    ) -> FramedLineSelection:
        """Place and qualify in the same snapshot before contextual point mapping.

        Source Line2D values and their unresolved physical units remain intact.
        No definition-local point is implicitly treated as a root-stored point.
        """
        if not isinstance(line_profile, Line2DProfile):
            raise TypeError("line_profile must be Line2DProfile")
        if limits is None:
            limits = SceneLimits()
        if not isinstance(limits, SceneLimits):
            raise TypeError("limits must be SceneLimits")
        return _framed_selection(
            self._handle.qualified_lines_in_coordinate_space(
                _unsigned(record_index, "record_index"),
                line_profile.value,
                str_value(space_profile),
                str_value(frame),
                _unsigned(limits.max_depth, "max_depth"),
                _unsigned(limits.max_nodes, "max_nodes"),
                _unsigned(limits.max_lines, "max_lines"),
            )
        )

    def qualified_lines(
        self,
        record_index: int,
        *,
        profile: Line2DProfile,
        limits: SceneLimits | None = None,
    ) -> QualifiedLineSelection:
        """Admit a selected subtree under an explicit stored-XY decoder profile.

        Units, WCS, visibility and drawing completeness remain unresolved.
        Unsupported conditions reject the entire selection.
        """
        if not isinstance(profile, Line2DProfile):
            raise TypeError("profile must be Line2DProfile")
        if limits is None:
            limits = SceneLimits()
        if not isinstance(limits, SceneLimits):
            raise TypeError("limits must be SceneLimits")
        return _qualified_selection(
            self._handle.qualified_lines(
                _unsigned(record_index, "record_index"),
                profile.value,
                _unsigned(limits.max_depth, "max_depth"),
                _unsigned(limits.max_nodes, "max_nodes"),
                _unsigned(limits.max_lines, "max_lines"),
            )
        )

    def scene_lines(
        self, record_index: int, *, limits: SceneLimits | None = None
    ) -> tuple[SceneLine2D, ...]:
        """Expand one admitted planar subtree with bounded nesting and provenance.

        Unknown states, cycles and limits fail the whole request. Inline packages
        add no placement transform; unqualified dependent inline scopes fail.
        Coordinates retain units=None.
        """
        if limits is None:
            limits = SceneLimits()
        if not isinstance(limits, SceneLimits):
            raise TypeError("limits must be SceneLimits")
        return _scene_lines(
            self._handle.scene_lines(
                _unsigned(record_index, "record_index"),
                _unsigned(limits.max_depth, "max_depth"),
                _unsigned(limits.max_nodes, "max_nodes"),
                _unsigned(limits.max_lines, "max_lines"),
            )
        )

    def scene_lines_with_inline_scopes(
        self, record_index: int, *, limits: SceneLimits | None = None
    ) -> tuple[SceneLine2D, ...]:
        """Opt into the marker25 inline attribute rules verified by native controls.

        Admitted inline wrappers do not establish an insertion attribute scope;
        dependent leaf attributes retain the nearest real reference context.
        All geometry/state/provenance limits still apply. Units remain unknown.
        """
        if limits is None:
            limits = SceneLimits()
        if not isinstance(limits, SceneLimits):
            raise TypeError("limits must be SceneLimits")
        return _scene_lines(
            self._handle.scene_lines_with_inline_scopes(
                _unsigned(record_index, "record_index"),
                _unsigned(limits.max_depth, "max_depth"),
                _unsigned(limits.max_nodes, "max_nodes"),
                _unsigned(limits.max_lines, "max_lines"),
            )
        )

    def read_contents(self) -> bytes:
        """Return the exact payload, including uninterpreted header/tail regions."""
        return self._handle.contents()

    def read_payload(self) -> bytes:
        """Return the exact drawing payload; consult offset_domain for its carrier."""
        return self.read_contents()

    def inline_package_lines(
        self,
        record_index: int,
        *,
        max_depth: int = 32,
        max_nodes: int = MAX_RECORDS_PER_PAGE,
        max_lines: int = MAX_RECORDS_PER_PAGE,
    ) -> tuple[ScopedInlineLine2D, ...]:
        """Decode one primary package and retain every ancestor source header.

        Exact line tags and qualified 0x0030/0x0430 package bodies only. An interior
        slot is rejected. Depth is capped at 128, and copied ancestor entries at
        MAX_INLINE_PACKAGE_PROVENANCE. Unknown/invalid/over-budget subtrees fail
        without partial output. No coordinate or attribute transform is applied.
        """
        return _inline_lines(
            self._handle.inline_package_lines(
                _unsigned(record_index, "record_index"),
                _unsigned(max_depth, "max_depth"),
                _unsigned(max_nodes, "max_nodes"),
                _unsigned(max_lines, "max_lines"),
            )
        )

    def drawing_settings(self) -> DrawingSettings:
        """Read finite origin/positive paper-scale fields; apply no transform.

        Rust validates the three stored fields and the reciprocal. Other header
        values, physical units and geometry interpretation remain unresolved.
        """
        offset, header, origin, factor, scale = self._handle.drawing_settings()
        return DrawingSettings(offset, header, (origin[0], origin[1]), factor, scale)

    def text_size_unit(self) -> TextSizeUnitInfo:
        """Decode marker-25 text-size units on request; resolve no geometry unit.

        Only selector 0 (mm) and 1 (pt) are qualified. Other selectors/profiles
        fail explicitly; line coordinates and line.units remain unchanged.
        """
        return TextSizeUnitInfo(*self._handle.text_size_unit())

    def block_library(self, *, max_definitions: int = 4096) -> BlockLibrary:
        """Inspect observed block-table bounds under root+definition record limits.

        Parsing is performed in Rust. Known fixed headers and the shared suffix
        remain opaque. Unsupported optional tables/layouts stop this request.
        """
        handle = self._handle.block_library(
            _unsigned(max_definitions, "max_definitions")
        )
        row = handle.metadata()
        return BlockLibrary(
            tuple(BlockDefinition(**item) for item in row["definitions"]),
            row["suffix_offset"],
            row["suffix_size"],
            handle,
        )

    def records(self, start: int = 0, count: int = 1024) -> tuple[RawRecord, ...]:
        """Read a raw-record window, capped at MAX_RECORDS_PER_PAGE (4096).

        EOF clips the window. start == record_count returns empty; a larger
        start raises IndexError. Counts include opaque and terminal records.
        """
        return tuple(
            RawRecord(*row)
            for row in self._handle.records(
                _unsigned(start, "start"), _unsigned(count, "count")
            )
        )

    def object_spans(self, *, max_objects: int = 4096) -> tuple[RawObjectSpan, ...]:
        """Validate and return all one-level root-table spans, excluding terminal.

        Rust checks primary span fields and bounds over the whole table before
        returning anything. A malformed span or exceeded output bound fails the
        entire request. Unknown tags/interior bytes stay opaque; structural
        spans do not establish logical root membership or normalized geometry.
        """
        return tuple(
            RawObjectSpan(*row)
            for row in self._handle.object_spans(_unsigned(max_objects, "max_objects"))
        )

    def record_bytes(self, index: int) -> bytes:
        """Return exactly 64 bytes; an absent record raises IndexError."""
        return self._handle.record_bytes(_unsigned(index, "index"))

    def line_candidates(
        self, record_start: int = 0, record_count: int = 1024
    ) -> tuple[LineCandidate2D, ...]:
        """Return candidates within a raw-record window (not candidate indices)."""
        rows = self._handle.line_candidates(
            _unsigned(record_start, "record_start"),
            _unsigned(record_count, "record_count"),
        )
        return tuple(
            LineCandidate2D(index, offset, (a[0], a[1]), (b[0], b[1]))
            for index, offset, a, b in rows
        )

    def line_records(
        self, record_start: int = 0, record_count: int = 1024
    ) -> tuple[LineRecord2D, ...]:
        """Decode known record-local line fields within a raw-record window.

        The page is bounded as for records(), preserves order/duplicates, and
        includes only exact tags 0x0002/0x0402. All six float fields must be finite;
        a bad record fails the requested page. Candidate counts stay unchanged.
        Units, flags, attributes, parent scopes and placement remain unresolved.
        """
        rows = self._handle.line_records(
            _unsigned(record_start, "record_start"),
            _unsigned(record_count, "record_count"),
        )
        return tuple(
            LineRecord2D(
                index,
                offset,
                tag,
                header,
                (a[0], a[1]),
                (b[0], b[1]),
                elevation,
                thickness,
            )
            for index, offset, tag, header, a, b, elevation, thickness in rows
        )

    def line_records_3d(
        self, record_start: int = 0, record_count: int = 1024
    ) -> tuple[LineRecord3D, ...]:
        """Read exact observed 3D-line fields in a capped raw-record window.

        Recognizes only tag 0x0013 in independently observed file profiles.
        All XYZ fields must be finite; a failure rejects the requested page.
        These are record-local values with unresolved parent scope and units.
        This does not promote the result to WCS or a complete drawing entity.
        """
        rows = self._handle.line_records_3d(
            _unsigned(record_start, "record_start"),
            _unsigned(record_count, "record_count"),
        )
        return tuple(
            LineRecord3D(index, offset, tag, header, tuple(start), tuple(end))
            for index, offset, tag, header, start, end in rows
        )

    def package_references(
        self, record_start: int = 0, record_count: int = 1024
    ) -> tuple[PackageReference2D, ...]:
        """Decode exact two-slot tag-6/FE payloads whose primary is in the page.

        The following FE may lie outside the requested page, but cannot cross
        the terminal slot. Unknown flags are not masked. This enumerates root
        table slots and does not establish logical object membership or resolve
        units, parents or drawing completeness. Bad payloads fail the whole page.
        """
        rows = self._handle.package_references(
            _unsigned(record_start, "record_start"),
            _unsigned(record_count, "record_count"),
        )
        return tuple(
            PackageReference2D(
                index,
                offset,
                continuation_index,
                continuation_offset,
                header,
                name_field,
                name_key,
                (anchor[0], anchor[1]),
                continuation_header,
                tuple(placement),
            )
            for index, offset, continuation_index, continuation_offset, header, name_field, name_key, anchor, continuation_header, placement in rows
        )

    def to_dict(self) -> dict:
        """JSON-ready summary; raw bytes and record pages are requested separately."""
        header = asdict(self.header)
        for name in ("section_offsets", "optional_offsets"):
            header[name] = list(header[name])
        return {
            "schema_version": 1,
            "format": self.format,
            "file_size": self.file_size,
            "payload": {
                "stream_path": self.stream_path,
                "size": self.payload_size,
                "sha256": self.sha256,
                "offset_domain": self.offset_domain,
            },
            "header": header,
            "record_size": 64,
            "tag_counts": {
                f"0x{tag:04x}": count for tag, count in self.tag_counts.items()
            },
            "line_candidate_count": self.line_candidate_count,
            "opaque_record_count": self.opaque_record_count,
            "geometry_status": "not_decoded",
            "coordinate_system": None,
            "units": None,
            "diagnostics": [asdict(item) for item in self.diagnostics],
            **({"mps": _mps_dict(self.mps)} if self.mps is not None else {}),
        }


def _scan(handle: _core._RawHandle) -> RawScan:
    row = handle.metadata()
    header = row["header"]
    _source_domain(row["stream_path"], row["offset_domain"])
    if {"mpz": "cfb_stream", "mpx": "file", "mps": "decoded_payload"}.get(
        row["format"]
    ) != row["offset_domain"]:
        raise ValueError("Raw input carrier differs across language boundary")
    if (row["format"] == "mps") != (row["mps"] is not None):
        raise ValueError("Raw MPS provenance differs across language boundary")
    if row["mps"] is not None and row["mps"]["decoded_size"] != row["payload_size"]:
        raise ValueError("Raw MPS decoded size differs across language boundary")
    return RawScan(
        header=PayloadHeader(
            **{
                **header,
                "section_offsets": tuple(header["section_offsets"]),
                "optional_offsets": tuple(header["optional_offsets"]),
            }
        ),
        format=row["format"],
        offset_domain=row["offset_domain"],
        file_size=row["file_size"],
        payload_size=row["payload_size"],
        sha256=row["sha256"],
        stream_path=row["stream_path"],
        line_candidate_count=row["line_candidate_count"],
        opaque_record_count=row["opaque_record_count"],
        tag_counts=MappingProxyType(row["tag_counts"]),
        diagnostics=tuple(Diagnostic(*item) for item in row["diagnostics"]),
        _handle=handle,
        mps=_mps_info(row["mps"]),
    )


def scan_records(data: bytes, *, limits: ParseLimits | None = None) -> RawScan:
    """Scan MPZ/MPX or verified unprotected MPS; unqualified carriers stay rejected."""
    if not isinstance(data, bytes):
        raise TypeError("data must be bytes")
    return _scan(_core.scan_bytes(data, _options(limits)._values()))


def scanfile(
    path: str | PathLike[str], *, limits: ParseLimits | None = None
) -> RawScan:
    """Read a qualified MPZ/MPX/MPS file once; pages retain the decoded snapshot."""
    value = fspath(path)
    if not isinstance(value, str):
        raise TypeError("path must be str or PathLike[str]")
    return _scan(_core.scan_file(value, _options(limits)._values()))
