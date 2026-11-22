"""Best-effort reading: every recognized object, with a count of the rest.

The qualified reader in :mod:`ezdracad.document` stops at the first object it
cannot establish. This module walks the whole drawing instead, decodes the
objects whose saved layout is known, and reports what it skipped. Use it to
display or convert a drawing; use the report to judge what is missing.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping
from dataclasses import dataclass, fields
from os import PathLike, fspath
from types import MappingProxyType
from typing import Any

from . import _core
from .options import ParseLimits

__all__ = [
    "BACKGROUND_FILL_COLOR",
    "BY_BLOCK_COLOR",
    "BY_BLOCK_LINE_TYPE",
    "BY_BLOCK_WIDTH",
    "BY_LAYER_COLOR",
    "BY_LAYER_LINE_TYPE",
    "BY_LAYER_WIDTH",
    "BestEffortLimits",
    "Block",
    "Color",
    "DRAWING_PROPERTY_SET",
    "Diagnostic",
    "Drawing",
    "Entity",
    "Layer",
    "LineType",
    "PAPER_NAMES",
    "Property",
    "RGB_FILL_COLOR",
    "Report",
    "Settings",
    "Width",
    "read",
    "readfile",
]

#: Attribute values that defer to the layer or to the enclosing block.
BY_LAYER_COLOR = 257
BY_BLOCK_COLOR = 258
BY_LAYER_LINE_TYPE = 33
BY_BLOCK_LINE_TYPE = 34
BY_LAYER_WIDTH = 17
BY_BLOCK_WIDTH = 18
#: ``color`` of a fill that uses its own ``rgb`` value, or the background color.
RGB_FILL_COLOR = 257
BACKGROUND_FILL_COLOR = 258
#: ``owner`` of the properties that hold the drawing's property set.
DRAWING_PROPERTY_SET = "DRACAD_SYSTEM_DRAWING_PSET"
#: Names the application prints for ``Settings.paper_type``.
PAPER_NAMES = MappingProxyType(
    {
        0: "Free",
        **{1 + size: f"A{size}" for size in range(7)},
        **{8 + size: f"B{size}" for size in range(7)},
        15: "Letter",
        16: "Legal",
    }
)

_DEFAULTS = _core.best_effort_limit_defaults()


@dataclass(frozen=True)
class BestEffortLimits:
    """Budgets of a best-effort read. Exceeding one raises, never truncates."""

    max_entities: int = _DEFAULTS[0]
    max_depth: int = _DEFAULTS[1]
    max_curve_points: int = _DEFAULTS[2]
    max_block_definitions: int = _DEFAULTS[3]
    max_diagnostics: int = _DEFAULTS[4]

    def __post_init__(self) -> None:
        for field in fields(self):
            value = getattr(self, field.name)
            if not isinstance(value, int) or isinstance(value, bool):
                raise TypeError(f"{field.name} must be an integer")
            if not 0 <= value <= (1 << 64) - 1:
                raise ValueError(f"{field.name} must fit an unsigned 64-bit integer")

    def _values(self) -> tuple[int, int, int, int, int]:
        return (
            self.max_entities,
            self.max_depth,
            self.max_curve_points,
            self.max_block_definitions,
            self.max_diagnostics,
        )


def _plain(value: Any) -> Any:
    """JSON-compatible copy; byte strings become hexadecimal text."""
    if isinstance(value, (bytes, bytearray)):
        return bytes(value).hex()
    if isinstance(value, Mapping):
        return {key: _plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(item) for item in value]
    return value


@dataclass(frozen=True)
class Entity:
    """One decoded object.

    ``type`` selects the keys of ``data``; see the best-effort guide. ``color``,
    ``line_type`` and ``width`` are file-local table indices, with the
    ``BY_LAYER_*`` and ``BY_BLOCK_*`` values deferring to the layer or block.
    ``children`` holds polyline members, package members, the loops of an
    extended polygon and the text of a dimension or leader.
    """

    type: str
    layer: int
    color: int
    line_type: int
    width: int
    flags: int
    segment: int
    material: int
    group: int
    link_id: int
    tag: int
    table: str
    block_index: int | None
    record_index: int
    payload_offset: int
    record_count: int
    data: Mapping[str, Any]
    children: tuple[Entity, ...]

    @property
    def reversed(self) -> bool:
        """Whether a polyline follows this member arc from its end angle to its start."""
        return self.flags & 0x18 == 0x18

    def walk(self) -> Iterator[Entity]:
        """This entity and every entity below it, parents first."""
        yield self
        for child in self.children:
            yield from child.walk()

    def to_dict(self) -> dict:
        row = {
            field.name: getattr(self, field.name)
            for field in fields(self)
            if field.name not in ("data", "children")
        }
        row["data"] = _plain(self.data)
        row["children"] = [child.to_dict() for child in self.children]
        return row


def _entities(rows: list[dict]) -> tuple[Entity, ...]:
    return tuple(
        Entity(
            type=row["type"],
            layer=row["layer"],
            color=row["color"],
            line_type=row["line_type"],
            width=row["width"],
            flags=row["flags"],
            segment=row["segment"],
            material=row["material"],
            group=row["group"],
            link_id=row["link_id"],
            tag=row["tag"],
            table=row["table"],
            block_index=row["block_index"],
            record_index=row["record_index"],
            payload_offset=row["payload_offset"],
            record_count=row["record_count"],
            data=MappingProxyType(row["data"]),
            children=_entities(row["children"]),
        )
        for row in rows
    )


@dataclass(frozen=True)
class Layer:
    index: int
    name: str
    flags: int
    hidden: bool
    color: int
    line_type: int
    width: int


@dataclass(frozen=True)
class Color:
    """Palette entry. The RGB value is the saved one, not a DXF color index."""

    index: int
    rgb: tuple[int, int, int]
    pen_width: float


@dataclass(frozen=True)
class LineType:
    index: int
    pen_style: int
    pattern: tuple[float, ...]


@dataclass(frozen=True)
class Width:
    index: int
    width: float


@dataclass(frozen=True)
class Settings:
    """Saved drawing settings; none of them is applied to a coordinate."""

    drawing_origin: tuple[float, float]
    drawing_scale: float
    paper_size: tuple[float, float]
    paper_origin: tuple[float, float]
    #: Paper size code: 0 a free size, 1 to 7 A0 to A6, 8 to 14 B0 to B6,
    #: 15 letter, 16 legal.
    paper_type: int = 0
    paper_portrait: bool = False
    #: Factor between the paper scale and the scale the drawing states in its
    #: scale field; 1 in most drawings.
    scale_ratio: float = 1.0
    comment: str = ""
    #: Whether the drawing is printed with the pen width of each color
    #: (``Color.pen_width``) instead of the width of each object.
    width_by_color: bool = False

    @property
    def stated_scale(self) -> float:
        """Denominator of the scale the drawing states in its scale field."""
        return self.drawing_scale / self.scale_ratio

    @property
    def paper_name(self) -> str:
        """Name of the paper size as the application prints it, ``""`` if unknown."""
        return PAPER_NAMES.get(self.paper_type, "")


@dataclass(frozen=True)
class Property:
    """A property saved with the drawing: no geometry, three strings.

    The application keeps its own bookkeeping here. Properties whose owner is
    ``DRAWING_PROPERTY_SET`` are the values that property-set fields print.
    """

    table: str
    block_index: int | None
    record_index: int
    payload_offset: int
    owner: str
    name: str
    value: str
    lossy: bool


@dataclass(frozen=True)
class Block:
    """A named definition; ``INSERT`` entities refer to it by ``name_bytes``."""

    index: int
    name: str
    name_bytes: bytes
    header_offset: int
    entities: tuple[Entity, ...]


@dataclass(frozen=True)
class Diagnostic:
    code: str
    message: str
    table: str | None
    block_index: int | None
    record_index: int | None
    tag: int | None


@dataclass(frozen=True)
class Report:
    """What was decoded and what was not, counted at every nesting level."""

    lossy: bool
    decoded_counts: Mapping[str, int]
    unsupported_counts: Mapping[int, int]
    failed_counts: Mapping[int, int]
    property_records: int
    orphan_records: int
    clamped_spans: int
    diagnostics: tuple[Diagnostic, ...]
    diagnostics_suppressed: int
    #: Objects the application had erased; their records stay in the file.
    erased_objects: int = 0

    def to_dict(self) -> dict:
        row = {field.name: getattr(self, field.name) for field in fields(self)}
        for name in ("decoded_counts", "unsupported_counts", "failed_counts"):
            row[name] = {str(key): value for key, value in row[name].items()}
        row["diagnostics"] = [
            {field.name: getattr(item, field.name) for field in fields(item)}
            for item in self.diagnostics
        ]
        return row


@dataclass(frozen=True)
class Drawing:
    """A best-effort view of one drawing.

    ``marker`` is the raw generation field of the payload, not an application
    version. Coordinates are the stored values; ``settings`` is not applied.
    """

    format: str
    marker: int
    marker_observed: bool
    payload_size: int
    payload_sha256: str
    record_count: int
    settings: Settings | None
    layers: tuple[Layer, ...]
    colors: tuple[Color, ...]
    line_types: tuple[LineType, ...]
    widths: tuple[Width, ...]
    blocks: tuple[Block, ...]
    entities: tuple[Entity, ...]
    report: Report
    properties: tuple[Property, ...] = ()

    def walk(self) -> Iterator[Entity]:
        """Every entity of the drawing's own table, parents before members."""
        for entity in self.entities:
            yield from entity.walk()

    def property_set(self) -> dict[str, str]:
        """Values of the drawing's property set by name, as its fields print them."""
        return {
            item.name: item.value
            for item in self.properties
            if item.owner == DRAWING_PROPERTY_SET and item.table == "root"
        }

    def block(self, name: str | bytes) -> Block | None:
        """The first definition with this name, or ``None``."""
        for block in self.blocks:
            if name in (block.name, block.name_bytes):
                return block
        return None

    def to_dict(self, *, include_entities: bool = True) -> dict:
        row: dict[str, Any] = {
            "schema_version": 2,
            "format": self.format,
            "marker": self.marker,
            "marker_observed": self.marker_observed,
            "payload_size": self.payload_size,
            "payload_sha256": self.payload_sha256,
            "record_count": self.record_count,
            "settings": None
            if self.settings is None
            else {
                field.name: list(value) if isinstance(value, tuple) else value
                for field in fields(self.settings)
                for value in (getattr(self.settings, field.name),)
            },
            "properties": [
                {field.name: getattr(item, field.name) for field in fields(item)}
                for item in self.properties
            ],
            "layers": [
                {field.name: getattr(item, field.name) for field in fields(item)}
                for item in self.layers
            ],
            "colors": [
                {
                    "index": item.index,
                    "rgb": list(item.rgb),
                    "pen_width": item.pen_width,
                }
                for item in self.colors
            ],
            "line_types": [
                {
                    "index": item.index,
                    "pen_style": item.pen_style,
                    "pattern": list(item.pattern),
                }
                for item in self.line_types
            ],
            "widths": [
                {"index": item.index, "width": item.width} for item in self.widths
            ],
            "report": self.report.to_dict(),
        }
        if include_entities:
            row["blocks"] = [
                {
                    "index": block.index,
                    "name": block.name,
                    "name_bytes": block.name_bytes.hex(),
                    "header_offset": block.header_offset,
                    "entities": [entity.to_dict() for entity in block.entities],
                }
                for block in self.blocks
            ]
            row["entities"] = [entity.to_dict() for entity in self.entities]
        return row


def _drawing(row: dict) -> Drawing:
    settings = row["settings"]
    report = row["report"]
    return Drawing(
        format=row["format"],
        marker=row["marker"],
        marker_observed=row["marker_observed"],
        payload_size=row["payload_size"],
        payload_sha256=row["payload_sha256"],
        record_count=row["record_count"],
        settings=None
        if settings is None
        else Settings(
            drawing_origin=tuple(settings["drawing_origin"]),
            drawing_scale=settings["drawing_scale"],
            paper_size=tuple(settings["paper_size"]),
            paper_origin=tuple(settings["paper_origin"]),
            paper_type=settings["paper_type"],
            paper_portrait=settings["paper_portrait"],
            scale_ratio=settings["scale_ratio"],
            comment=settings["comment"],
            width_by_color=settings["width_by_color"],
        ),
        layers=tuple(Layer(*item) for item in row["layers"]),
        colors=tuple(
            Color(index, tuple(rgb), pen_width)
            for index, rgb, pen_width in row["colors"]
        ),
        line_types=tuple(
            LineType(index, pen_style, tuple(pattern))
            for index, pen_style, pattern in row["line_types"]
        ),
        widths=tuple(Width(*item) for item in row["widths"]),
        blocks=tuple(
            Block(
                index=block["index"],
                name=block["name"],
                name_bytes=block["name_bytes"],
                header_offset=block["header_offset"],
                entities=_entities(block["entities"]),
            )
            for block in row["blocks"]
        ),
        entities=_entities(row["entities"]),
        report=Report(
            lossy=report["lossy"],
            decoded_counts=MappingProxyType(report["decoded_counts"]),
            unsupported_counts=MappingProxyType(report["unsupported_counts"]),
            failed_counts=MappingProxyType(report["failed_counts"]),
            property_records=report["property_records"],
            orphan_records=report["orphan_records"],
            clamped_spans=report["clamped_spans"],
            diagnostics=tuple(Diagnostic(**item) for item in report["diagnostics"]),
            diagnostics_suppressed=report["diagnostics_suppressed"],
            erased_objects=report["erased_objects"],
        ),
        properties=tuple(Property(**item) for item in row["properties"]),
    )


def _limits(
    limits: ParseLimits | None, read_limits: BestEffortLimits | None
) -> tuple[tuple[int, ...], tuple[int, ...]]:
    limits = ParseLimits() if limits is None else limits
    read_limits = BestEffortLimits() if read_limits is None else read_limits
    if not isinstance(limits, ParseLimits):
        raise TypeError("limits must be a ParseLimits")
    if not isinstance(read_limits, BestEffortLimits):
        raise TypeError("read_limits must be a BestEffortLimits")
    return limits._values(), read_limits._values()


def read(
    data: bytes,
    *,
    limits: ParseLimits | None = None,
    read_limits: BestEffortLimits | None = None,
) -> Drawing:
    """Read every recognized object of MPZ, MPX, MPS or MPUW bytes.

    MPS and MPUW input must be saved without a password.
    """
    if not isinstance(data, bytes):
        raise TypeError("data must be bytes")
    return _drawing(_core.read_best_effort_bytes(data, *_limits(limits, read_limits)))


def readfile(
    path: str | PathLike[str],
    *,
    limits: ParseLimits | None = None,
    read_limits: BestEffortLimits | None = None,
) -> Drawing:
    """Read every recognized object of a regular MPZ, MPX, MPS or MPUW file."""
    return _drawing(
        _core.read_best_effort_file(fspath(path), *_limits(limits, read_limits))
    )
