"""Admitted planar nesting with immutable native source and style evidence."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Literal

from .effective_styles import EffectiveStyleIndices, _indices, _source

if TYPE_CHECKING:
    from .raw import LineRecord2D, RecordAttributes

DEFAULT_MAX_SCENE_DEPTH = 32
MAX_SCENE_DEPTH = 128
MAX_SCENE_PROVENANCE = 131_072
MAX_SCENE_PROVENANCE_BYTES = 32 * 1024 * 1024


@dataclass(frozen=True)
class SceneLimits:
    max_depth: int = DEFAULT_MAX_SCENE_DEPTH
    max_nodes: int = 4096
    max_lines: int = 4096


@dataclass(frozen=True)
class SceneDefinitionSource:
    index: int
    payload_offset: int
    header_bytes: bytes


@dataclass(frozen=True)
class SceneAncestor:
    """One outer-to-inner wrapper/reference; indices use its named source table."""

    kind: Literal["inline_package", "block_reference"]
    table_domain: Literal["root_records", "block_definition_records"]
    definition_index: int | None
    record_index: int
    payload_offset: int
    raw_bytes: bytes
    target_definition: SceneDefinitionSource | None


@dataclass(frozen=True)
class SceneLine2D:
    """Planar leaf after admitted recursive placements, with original raw values.

    Root/package-parent scopes do not establish a universal WCS or units. Inline
    wrappers contribute no extra placement; dependent attribute scopes crossing
    them remain unqualified and those requests fail explicitly.
    Native property indices preserve their complete ordered source chains;
    material/group remain raw and printing/visibility are separate questions.
    """

    source: LineRecord2D
    attributes: RecordAttributes
    table_domain: Literal["root_records", "block_definition_records"]
    definition_index: int | None
    ancestors: tuple[SceneAncestor, ...]
    # (a,b,c,d,tx,ty): (a*x+c*y+tx, b*x+d*y+ty).
    cumulative_transform: tuple[float, float, float, float, float, float]
    start: tuple[float, float]
    end: tuple[float, float]
    coordinate_scope: Literal["root_records", "package_parent"]
    style_layer_index: int
    styles: EffectiveStyleIndices
    units: None = field(default=None, init=False)


def _scene_lines(rows: list[dict]) -> tuple[SceneLine2D, ...]:
    result = []
    for row in rows:
        source, attributes = _source(row)
        ancestors = []
        for item in row["ancestors"]:
            item = dict(item)
            if item["target_definition"] is not None:
                item["target_definition"] = SceneDefinitionSource(
                    **item["target_definition"]
                )
            ancestors.append(SceneAncestor(**item))
        result.append(
            SceneLine2D(
                source,
                attributes,
                row["table_domain"],
                row["definition_index"],
                tuple(ancestors),
                tuple(row["cumulative_transform"]),
                tuple(row["start"]),
                tuple(row["end"]),
                row["coordinate_scope"],
                row["style_layer_index"],
                _indices(row["styles"]),
            )
        )
    return tuple(result)
