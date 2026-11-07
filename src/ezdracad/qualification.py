"""Explicit, versioned admission of planar stored-XY lines.

This qualifies the decoder subset, not input authorship or a complete drawing.
Coordinates retain their declared root/package-parent scope and units=None.
"""

from __future__ import annotations

from dataclasses import dataclass, field, fields
from enum import Enum
from typing import Literal

from .scene import SceneLine2D, _scene_lines


class Line2DProfile(str, Enum):
    MARKER25_PLANAR_STORED_XY_V1 = "marker25_planar_stored_xy_v1"
    MARKER25_PLANAR_STORED_XY_INLINE_V1 = "marker25_planar_stored_xy_inline_v1"


LINE2D_V1_MAX_REFERENCE_DEPTH = 2
LINE2D_V1_MAX_INLINE_DEPTH = 2
LINE2D_V1_MAX_ANCESTORS = 3
LINE2D_UNRESOLVED_SEMANTICS = (
    "physical_units",
    "drawing_wcs",
    "visibility",
    "layout",
    "material_meaning",
    "group_meaning",
    "final_printed_appearance",
    "document_completeness",
)


@dataclass(frozen=True)
class Line2D(SceneLine2D):
    """A planar primitive admitted by the explicit v1 stored-XY contract.

    The inherited source, attributes, full ancestors and style evidence remain
    owned. Start/end are in coordinate_scope, with no WCS/mm conversion. Unknown
    visibility/layout and the raw material/group fields remain unresolved.
    Qualified placements use exact cardinal axes and reject nonfinite results,
    nonzero product underflow and lost coordinate/displacement components.
    Source zero-length lines remain unqualified; direct root lines retain their
    finite coordinates without calculating a length or endpoint difference.
    """

    qualification_profile: Line2DProfile
    coordinate_semantics: Literal["stored_xy"] = field(default="stored_xy", init=False)
    unresolved_semantics: tuple[str, ...] = field(
        default=LINE2D_UNRESOLVED_SEMANTICS, init=False
    )


@dataclass(frozen=True)
class QualifiedLineSelection:
    """All admitted lines of one selected primary object, never a Document.

    A successful call requires at least one supported line. Unsupported paths
    fail the whole request; no partial selection or complete drawing is implied.
    """

    record_index: int
    profile: Line2DProfile
    lines: tuple[Line2D, ...]
    coordinate_semantics: Literal["stored_xy"] = field(default="stored_xy", init=False)
    units: None = field(default=None, init=False)
    document_complete: Literal[False] = field(default=False, init=False)
    unresolved_semantics: tuple[str, ...] = field(
        default=LINE2D_UNRESOLVED_SEMANTICS, init=False
    )


def _qualified_selection(row: dict) -> QualifiedLineSelection:
    profile = Line2DProfile(row["profile"])
    if (
        row["coordinate_semantics"] != "stored_xy"
        or row["units"] is not None
        or row["document_complete"] is not False
        or tuple(row["unresolved_semantics"]) != LINE2D_UNRESOLVED_SEMANTICS
    ):
        raise ValueError("qualified Line2D contract differs across language boundary")
    scenes = _scene_lines(row["lines"])
    names = tuple(item.name for item in fields(SceneLine2D) if item.init)
    lines = tuple(
        Line2D(
            qualification_profile=profile,
            **{name: getattr(scene, name) for name in names},
        )
        for scene in scenes
    )
    return QualifiedLineSelection(row["record_index"], profile, lines)
