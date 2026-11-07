"""Native display-attribute indices and source evidence, without print policy."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from .raw import LineRecord2D, RecordAttributes


@dataclass(frozen=True)
class StyleSource:
    """Drawing-payload-relative evidence; index is local to the named source table.

    Placement sources also establish scope for explicit child overrides. Their
    presence does not itself mean an attribute was inherited from the placement.
    Native row bytes remain owned and immutable after the input file is removed.
    """

    kind: Literal["record", "block_placement", "layer", "color", "line_type", "width"]
    table_domain: Literal[
        "root_records",
        "block_definition_records",
        "layer_table",
        "color_table",
        "line_type_table",
        "width_table",
    ]
    index: int
    definition_index: int | None
    payload_offset: int
    raw_bytes: bytes


@dataclass(frozen=True)
class ResolvedStyleIndex:
    """A native index with its ordered source/context chain; never a DXF ACI."""

    index: int
    sources: tuple[StyleSource, ...]


@dataclass(frozen=True)
class EffectiveStyleIndices:
    """Native drawing-property indices; visibility and printing stay separate."""

    color: ResolvedStyleIndex
    line_type: ResolvedStyleIndex
    width: ResolvedStyleIndex


@dataclass(frozen=True)
class EffectiveLineStyle:
    """One primary root line with raw attributes and resolved native indices."""

    source: LineRecord2D
    attributes: RecordAttributes
    styles: EffectiveStyleIndices
    units: None = field(default=None, init=False)

    @property
    def coordinate_space(self) -> Literal["record_local"]:
        return "record_local"


@dataclass(frozen=True)
class EffectiveBlockLineStyle:
    """Direct definition leaf under a selected, admitted root block insertion.

    Source coordinates stay definition-local. Start/end use the same restricted
    package-parent placement subset as the existing direct placement API.
    Material/group remain raw; visibility and printed appearance are not solved.
    """

    reference_record_index: int
    definition_index: int
    source: LineRecord2D
    attributes: RecordAttributes
    styles: EffectiveStyleIndices
    start: tuple[float, float]
    end: tuple[float, float]
    coordinate_space: Literal["package_parent"] = field(
        default="package_parent", init=False
    )
    units: None = field(default=None, init=False)


def _indices(row: dict) -> EffectiveStyleIndices:
    def index(value: dict) -> ResolvedStyleIndex:
        return ResolvedStyleIndex(
            value["index"], tuple(StyleSource(**source) for source in value["sources"])
        )

    return EffectiveStyleIndices(
        *(index(row[key]) for key in ("color", "line_type", "width"))
    )


def _source(row: dict) -> tuple[LineRecord2D, RecordAttributes]:
    # Imported lazily so raw.py can expose these dataclasses without a cycle.
    from .raw import LineRecord2D, _record_attributes

    source, attributes = row["source"]
    index, offset, tag, header, start, end, elevation, thickness = source
    return (
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


def _line_style(row: dict) -> EffectiveLineStyle:
    source, attributes = _source(row)
    return EffectiveLineStyle(source, attributes, _indices(row["styles"]))


def _block_styles(rows: list[dict]) -> tuple[EffectiveBlockLineStyle, ...]:
    result = []
    for row in rows:
        source, attributes = _source(row)
        result.append(
            EffectiveBlockLineStyle(
                row["reference_record_index"],
                row["definition_index"],
                source,
                attributes,
                _indices(row["styles"]),
                tuple(row["start"]),
                tuple(row["end"]),
            )
        )
    return tuple(result)
