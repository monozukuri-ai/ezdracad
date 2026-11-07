"""Read-only Documents, paged queries and explicit partial-read contracts."""

from __future__ import annotations

import operator
from collections.abc import Iterator, Mapping, Sequence
from dataclasses import asdict, dataclass, field, fields
from enum import Enum
from os import PathLike, fspath
from types import MappingProxyType
from typing import Literal, overload

from . import _core, raw
from .basic2d import (
    ArcEntity,
    Basic2DProfile,
    CircleEntity,
    TextEntity,
    TextLimits,
    _primitive_entity,
)
from .options import ParseLimits
from .qualification import LINE2D_UNRESOLVED_SEMANTICS, Line2D, Line2DProfile
from .scene import SceneLimits, SceneLine2D, _scene_lines

_READ_LIMIT_DEFAULTS = _core.read_limit_defaults()


class ReadPolicy(str, Enum):
    STRICT = "strict"
    PARTIAL = "partial"


class ReadScope(str, Enum):
    DRAWING = "drawing"
    STORED_XY_LINES = "stored_xy_lines"
    STORED_XY_PRIMITIVES = "stored_xy_primitives"


class ReadStatus(str, Enum):
    COMPLETE = "complete"
    PARTIAL = "partial"


def _uint(value: int, name: str) -> int:
    if type(value) is not int:
        raise TypeError(f"{name} must be an integer")
    if not 0 <= value <= (1 << 64) - 1:
        raise ValueError(f"{name} must fit an unsigned 64-bit integer")
    return value


@dataclass(frozen=True)
class ReadLimits:
    """Aggregate budgets, separate from input and per-subtree SceneLimits."""

    max_objects: int = _READ_LIMIT_DEFAULTS[0]
    max_entities: int = _READ_LIMIT_DEFAULTS[1]
    max_nodes: int = _READ_LIMIT_DEFAULTS[2]
    max_provenance_bytes: int = _READ_LIMIT_DEFAULTS[3]
    max_diagnostics: int = _READ_LIMIT_DEFAULTS[4]

    def __post_init__(self) -> None:
        for item in fields(self):
            _uint(getattr(self, item.name), item.name)


@dataclass(frozen=True)
class ReadOptions:
    """Strict drawing reads are default; current drawing meaning is partial.

    stored_xy_lines explicitly narrows the requested meaning to the named stored-XY
    profile. Complete within that scope still means document_complete=False.
    basic_profile opts into root CIRCLE/ARC/literal TEXT; stored_xy_primitives
    narrows completeness to those primitives plus the selected LINE profile.
    """

    policy: ReadPolicy | str = ReadPolicy.STRICT
    scope: ReadScope | str = ReadScope.DRAWING
    line_profile: Line2DProfile | str = Line2DProfile.MARKER25_PLANAR_STORED_XY_V1
    parse_limits: ParseLimits = field(default_factory=ParseLimits)
    scene_limits: SceneLimits = field(default_factory=SceneLimits)
    read_limits: ReadLimits = field(default_factory=ReadLimits)
    basic_profile: Basic2DProfile | str | None = None
    text_limits: TextLimits = field(default_factory=TextLimits)

    def __post_init__(self) -> None:
        for name, kind in (
            ("policy", ReadPolicy),
            ("scope", ReadScope),
            ("line_profile", Line2DProfile),
        ):
            value = getattr(self, name)
            if not isinstance(value, (str, kind)):
                raise TypeError(f"{name} must be {kind.__name__} or str")
            object.__setattr__(self, name, kind(value))
        if self.basic_profile is not None:
            if not isinstance(self.basic_profile, (str, Basic2DProfile)):
                raise TypeError("basic_profile must be Basic2DProfile, str or None")
            object.__setattr__(
                self, "basic_profile", Basic2DProfile(self.basic_profile)
            )
        for name, kind in (
            ("parse_limits", ParseLimits),
            ("scene_limits", SceneLimits),
            ("read_limits", ReadLimits),
            ("text_limits", TextLimits),
        ):
            if not isinstance(getattr(self, name), kind):
                raise TypeError(f"{name} must be {kind.__name__}")
        for name in ("max_depth", "max_nodes", "max_lines"):
            _uint(getattr(self.scene_limits, name), name)

    def _values(self) -> _core.ReadValues:
        return (
            self.policy.value,
            self.scope.value,
            self.line_profile.value,
            self.parse_limits._values(),
            (
                self.scene_limits.max_depth,
                self.scene_limits.max_nodes,
                self.scene_limits.max_lines,
            ),
            tuple(getattr(self.read_limits, f.name) for f in fields(ReadLimits)),
            self.basic_profile.value if self.basic_profile is not None else None,
            self.text_limits._values(),
        )


@dataclass(frozen=True)
class SourceRef:
    """Source byte range; file offsets and CFB stream offsets stay distinct."""

    stream_path: str | None
    table_domain: Literal["root_records", "block_definition_records"]
    definition_index: int | None
    record_index: int
    payload_offset: int
    byte_length: int
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"] = "cfb_stream"

    def __post_init__(self) -> None:
        raw._source_domain(self.stream_path, self.offset_domain)


@dataclass(frozen=True)
class ReadDiagnostic:
    code: str
    message: str
    source: SourceRef | None


@dataclass(frozen=True)
class OpaqueRegion:
    """Retained region whose meaning is incomplete, including partly read tables."""

    stream_path: str | None
    payload_offset: int
    byte_length: int
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"] = "cfb_stream"

    def __post_init__(self) -> None:
        raw._source_domain(self.stream_path, self.offset_domain)


@dataclass(frozen=True)
class ReadReport:
    status: ReadStatus
    geometry_status: ReadStatus
    document_status: ReadStatus
    policy: ReadPolicy
    scope: ReadScope
    line_profile: Line2DProfile
    basic_profile: Basic2DProfile | None
    source_format: Literal["mpz", "mpx", "mps"]
    offset_domain: Literal["cfb_stream", "file", "decoded_payload"]
    contents_sha256: str
    stream_path: str | None
    raw_record_count: int
    primary_object_count: int
    interior_record_count: int
    auxiliary_record_count: int
    handled_auxiliary_object_count: int
    decoded_object_count: int
    entity_count: int
    unsupported_counts: Mapping[int, int]
    context_blocked_counts: Mapping[int, int]
    diagnostics: tuple[ReadDiagnostic, ...]
    diagnostics_suppressed: int
    opaque_regions: tuple[OpaqueRegion, ...]
    definition_structure_validated: bool
    scene_nodes_visited: int
    schema_version: Literal[1] = field(default=1, init=False)
    document_complete: Literal[False] = field(default=False, init=False)
    coordinate_semantics: Literal["stored_xy"] = field(default="stored_xy", init=False)
    units: None = field(default=None, init=False)
    requested_entity_types: tuple[str, ...] = ("LINE",)
    unresolved_semantics: tuple[str, ...] = field(
        default=LINE2D_UNRESOLVED_SEMANTICS, init=False
    )

    @property
    def payload_sha256(self) -> str:
        """Hash of the retained drawing payload, including decoded MPS bytes."""
        return self.contents_sha256

    def to_dict(self) -> dict:
        result = {f.name: getattr(self, f.name) for f in fields(self)}
        for name in (
            "status",
            "geometry_status",
            "document_status",
            "policy",
            "scope",
            "line_profile",
        ):
            result[name] = result[name].value
        result["basic_profile"] = (
            self.basic_profile.value if self.basic_profile is not None else None
        )
        for name in ("unsupported_counts", "context_blocked_counts"):
            result[name] = dict(result[name])
        for name in ("requested_entity_types", "unresolved_semantics"):
            result[name] = list(result[name])
        result["diagnostics"] = [asdict(d) for d in self.diagnostics]
        result["opaque_regions"] = [asdict(r) for r in self.opaque_regions]
        return result


def _report(row: dict) -> ReadReport:
    row = dict(row)
    for key, value in (
        ("schema_version", 1),
        ("document_complete", False),
        ("coordinate_semantics", "stored_xy"),
        ("units", None),
    ):
        if row.pop(key) != value:
            raise ValueError("Document report differs across language boundary")
    if {"mpz": "cfb_stream", "mpx": "file", "mps": "decoded_payload"}.get(
        row["source_format"]
    ) != row["offset_domain"]:
        raise ValueError("Document input carrier differs across language boundary")
    raw._source_domain(row["stream_path"], row["offset_domain"])
    profile = row["basic_profile"]
    requested = ("LINE", "CIRCLE", "ARC", "TEXT") if profile is not None else ("LINE",)
    if (
        tuple(row.pop("unresolved_semantics")) != LINE2D_UNRESOLVED_SEMANTICS
        or tuple(row["requested_entity_types"]) != requested
    ):
        raise ValueError(
            "Document requested/unresolved scope differs across language boundary"
        )
    row["requested_entity_types"] = requested
    row["basic_profile"] = Basic2DProfile(profile) if profile is not None else None
    for name, kind in (
        ("status", ReadStatus),
        ("geometry_status", ReadStatus),
        ("document_status", ReadStatus),
        ("policy", ReadPolicy),
        ("scope", ReadScope),
        ("line_profile", Line2DProfile),
    ):
        row[name] = kind(row[name])
    if (row["scope"] == ReadScope.STORED_XY_PRIMITIVES and profile is None) or (
        row["scope"] == ReadScope.STORED_XY_LINES and profile is not None
    ):
        raise ValueError(
            "Document primitive profile/scope differs across language boundary"
        )
    if row["document_status"] != ReadStatus.PARTIAL or (
        row["scope"] == ReadScope.DRAWING and row["status"] != ReadStatus.PARTIAL
    ):
        raise ValueError("Document completeness differs across language boundary")
    row["diagnostics"] = tuple(
        ReadDiagnostic(
            d["code"],
            d["message"],
            SourceRef(**d["source"]) if d["source"] is not None else None,
        )
        for d in row["diagnostics"]
    )
    row["opaque_regions"] = tuple(OpaqueRegion(**r) for r in row["opaque_regions"])
    sources = [d.source for d in row["diagnostics"] if d.source is not None]
    sources.extend(row["opaque_regions"])
    if any(
        source.offset_domain != row["offset_domain"]
        or source.stream_path != row["stream_path"]
        for source in sources
    ):
        raise ValueError("Document source ranges differ across language boundary")
    for name in ("unsupported_counts", "context_blocked_counts"):
        row[name] = MappingProxyType(dict(row[name]))
    return ReadReport(**row)


class IncompleteReadError(_core.UnsupportedFeatureError):
    """Strict request rejected, with its typed coverage report attached."""

    report: ReadReport

    def __init__(self, message: str, report: ReadReport) -> None:
        super().__init__(message)
        self.report = report


@dataclass(frozen=True)
class LineEntity(Line2D):
    """A qualified line instance. IDs are ordinals in this decoded snapshot."""

    entity_id: int
    root_record_index: int
    source_ref: SourceRef
    type: Literal["LINE"] = field(default="LINE", init=False)

    def to_dict(self) -> dict:
        """JSON geometry and source locations, excluding copied raw byte bodies."""
        return {
            "type": self.type,
            "entity_id": self.entity_id,
            "root_record_index": self.root_record_index,
            "source_ref": asdict(self.source_ref),
            "start": list(self.start),
            "end": list(self.end),
            "units": None,
            "coordinate_semantics": self.coordinate_semantics,
            "coordinate_scope": self.coordinate_scope,
            "qualification_profile": self.qualification_profile.value,
            "unresolved_semantics": list(self.unresolved_semantics),
            "cumulative_transform": list(self.cumulative_transform),
            "attributes": {
                "layer_index_raw": self.attributes.layer_index,
                "style_layer_index": self.style_layer_index,
                "color_index": self.styles.color.index,
                "line_type_index": self.styles.line_type.index,
                "width_index": self.styles.width.index,
                "material_index_raw": self.attributes.material_index_raw,
                "group_index_raw": self.attributes.group_index,
            },
            "ancestors": [
                {
                    "kind": a.kind,
                    "table_domain": a.table_domain,
                    "definition_index": a.definition_index,
                    "record_index": a.record_index,
                    "payload_offset": a.payload_offset,
                    "byte_length": len(a.raw_bytes),
                    "target_definition_index": a.target_definition.index
                    if a.target_definition
                    else None,
                }
                for a in self.ancestors
            ],
        }


Entity = LineEntity | CircleEntity | ArcEntity | TextEntity


def _entities(rows: list[dict]) -> tuple[Entity, ...]:
    names = tuple(f.name for f in fields(SceneLine2D) if f.init)
    result = []
    for row in rows:
        if row["type"] != "LINE":
            result.append(_primitive_entity(row, SourceRef(**row["source_ref"])))
            continue
        scene = _scene_lines([row])[0]
        result.append(
            LineEntity(
                **{n: getattr(scene, n) for n in names},
                qualification_profile=Line2DProfile(row["qualification_profile"]),
                entity_id=row["entity_id"],
                root_record_index=row["root_record_index"],
                source_ref=SourceRef(**row["source_ref"]),
            )
        )
    return tuple(result)


@dataclass(frozen=True)
class EntityQuery(Sequence[Entity]):
    """Decoded entities only, fetched in batches. Empty is not proof of absence."""

    _handle: _core._DocumentHandle = field(repr=False)
    entity_type: str
    _count: int = field(repr=False)

    def __len__(self) -> int:
        return self._count

    def page(self, start: int = 0, count: int = 256) -> tuple[Entity, ...]:
        _uint(start, "start")
        _uint(count, "count")
        return _entities(self._handle.query_page(self.entity_type, start, count))

    def _range(self, start: int, stop: int) -> Iterator[Entity]:
        for at in range(start, stop, 256):
            yield from self.page(at, min(256, stop - at))

    def __iter__(self) -> Iterator[Entity]:
        return self._range(0, self._count)

    @overload
    def __getitem__(self, key: int) -> Entity: ...
    @overload
    def __getitem__(self, key: slice) -> tuple[Entity, ...]: ...
    def __getitem__(self, key: int | slice) -> Entity | tuple[Entity, ...]:
        if isinstance(key, slice):
            start, stop, step = key.indices(self._count)
            if step > 0:
                return tuple(self._range(start, stop))[::step]
            return tuple(self._range(stop + 1, start + 1))[::step]
        index = operator.index(key)
        if index < 0:
            index += self._count
        if not 0 <= index < self._count:
            raise IndexError("decoded entity index out of range")
        return self.page(index, 1)[0]


@dataclass(frozen=True)
class Document:
    """One owned Rust snapshot; no modelspace or universal WCS claim."""

    report: ReadReport
    options: ReadOptions
    _handle: _core._DocumentHandle = field(repr=False, compare=False)

    def query(self, entity_type: str = "LINE") -> EntityQuery:
        if not isinstance(entity_type, str):
            raise TypeError("entity_type must be str")
        return EntityQuery(
            self._handle, entity_type, self._handle.query_count(entity_type)
        )

    @property
    def entities(self) -> EntityQuery:
        return self.query("*")

    def raw(self) -> raw.RawScan:
        """Access retained bytes, unsupported records and the exact source hash."""
        return raw._scan(self._handle.raw())

    def to_dict(self, *, include_entities: bool = True) -> dict:
        result = {
            "schema_version": 1,
            "kind": "ezdracad.document",
            "report": self.report.to_dict(),
        }
        if include_entities:
            result["entities"] = [entity.to_dict() for entity in self.entities]
        return result


def _options(
    options,
    policy,
    scope,
    line_profile,
    limits,
    scene_limits,
    read_limits,
    basic_profile,
    text_limits,
) -> ReadOptions:
    overrides = {
        "policy": policy,
        "scope": scope,
        "line_profile": line_profile,
        "parse_limits": limits,
        "scene_limits": scene_limits,
        "read_limits": read_limits,
        "basic_profile": basic_profile,
        "text_limits": text_limits,
    }
    if options is not None:
        if not isinstance(options, ReadOptions):
            raise TypeError("options must be ReadOptions")
        if any(v is not None for v in overrides.values()):
            raise ValueError("options cannot be combined with individual read options")
        return options
    return ReadOptions(**{k: v for k, v in overrides.items() if v is not None})


def _read(call, value, options: ReadOptions) -> Document:
    try:
        handle = call(value, options._values())
    except _core.DracadError as error:
        row = getattr(error, "_read_report", None)
        if row is not None:
            del error._read_report
            raise IncompleteReadError(str(error), _report(row)) from error
        raise
    return Document(_report(handle.report()), options, handle)


def read(
    data: bytes,
    *,
    options: ReadOptions | None = None,
    policy: ReadPolicy | str | None = None,
    scope: ReadScope | str | None = None,
    line_profile: Line2DProfile | str | None = None,
    limits: ParseLimits | None = None,
    scene_limits: SceneLimits | None = None,
    read_limits: ReadLimits | None = None,
    basic_profile: Basic2DProfile | str | None = None,
    text_limits: TextLimits | None = None,
) -> Document:
    """Read qualified MPZ/MPX/MPS bytes; strict drawing meaning remains the default."""
    if not isinstance(data, bytes):
        raise TypeError("data must be bytes")
    return _read(
        _core.read_bytes,
        data,
        _options(
            options,
            policy,
            scope,
            line_profile,
            limits,
            scene_limits,
            read_limits,
            basic_profile,
            text_limits,
        ),
    )


def readfile(
    path: str | PathLike[str],
    *,
    options: ReadOptions | None = None,
    policy: ReadPolicy | str | None = None,
    scope: ReadScope | str | None = None,
    line_profile: Line2DProfile | str | None = None,
    limits: ParseLimits | None = None,
    scene_limits: SceneLimits | None = None,
    read_limits: ReadLimits | None = None,
    basic_profile: Basic2DProfile | str | None = None,
    text_limits: TextLimits | None = None,
) -> Document:
    """Read a regular file once in Rust; later queries never reopen the path."""
    value = fspath(path)
    if not isinstance(value, str):
        raise TypeError("path must be str or PathLike[str]")
    return _read(
        _core.read_file,
        value,
        _options(
            options,
            policy,
            scope,
            line_profile,
            limits,
            scene_limits,
            read_limits,
            basic_profile,
            text_limits,
        ),
    )
