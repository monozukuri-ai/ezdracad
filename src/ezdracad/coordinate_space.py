"""Explicit saved-context point frames, separate from stored-XY qualification.

Current-annotation values use stored axes. The separate absolute-input frame
also applies the admitted saved drafter rotation and returns native mm equivalents.
Use input_unit.from_native_mm for literal input digits. Neither frame declares
entity creation scales, general WCS or property-panel units.
All admission checks and arithmetic are performed by Rust on an owned snapshot.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Literal

from . import _core
from .coordinate_input_units import NativeCoordinateInputUnitInfo
from .qualification import Line2D, QualifiedLineSelection, _qualified_selection


class CoordinateSpaceProfile(str, Enum):
    MARKER5_STORED_AXES_V1 = "marker5_stored_axes_v1"
    MARKER8_STORED_AXES_V1 = "marker8_stored_axes_v1"
    MARKER25_STORED_AXES_V1 = "marker25_stored_axes_v1"


class CoordinateFrame(str, Enum):
    ROOT_STORED_XY = "root_stored_xy"
    DRAWING_ORIGIN_RELATIVE_STORED_XY = "drawing_origin_relative_stored_xy"
    CURRENT_ANNOTATION_MM_FROM_DRAWING_ORIGIN = (
        "current_annotation_mm_from_drawing_origin"
    )
    CURRENT_ABSOLUTE_INPUT_MM_FROM_DRAWING_ORIGIN = (
        "current_absolute_input_mm_from_drawing_origin"
    )
    CONFIGURED_PAPER_MM_OFFSET_FROM_DRAWING_ORIGIN = (
        "configured_paper_mm_offset_from_drawing_origin"
    )

    @property
    def units(self) -> Literal["mm"] | None:
        if self in (
            CoordinateFrame.CURRENT_ANNOTATION_MM_FROM_DRAWING_ORIGIN,
            CoordinateFrame.CURRENT_ABSOLUTE_INPUT_MM_FROM_DRAWING_ORIGIN,
            CoordinateFrame.CONFIGURED_PAPER_MM_OFFSET_FROM_DRAWING_ORIGIN,
        ):
            return "mm"
        return None


def _point(value: tuple[float, float] | list[float]) -> tuple[float, float]:
    if not isinstance(value, (tuple, list)) or len(value) != 2:
        raise TypeError("point must be a two-number tuple or list")
    if any(not isinstance(x, (int, float)) or isinstance(x, bool) for x in value):
        raise TypeError("point coordinates must be int or float, not bool")
    return float(value[0]), float(value[1])


@dataclass(frozen=True)
class CoordinateSpaceEvidence:
    """Exact retained Contents header; gaps are preserved, not interpreted."""

    contents_sha256: str
    observed_profile: str
    header_offset: int
    header_bytes: bytes


@dataclass(frozen=True)
class SavedDraftState:
    """Passive saved source fields; the explicit input frame applies rotation.

    Reading this state does not transform a point. Only the admitted absolute-
    input target applies its enable/angle values during coordinate conversion.
    """

    angle_offset: int
    angle_bytes: bytes
    angle_degrees: float
    object_offset: int
    object_bytes: bytes
    object_raw: int
    enable_offset: int
    enable_bytes: bytes
    enable_raw: int


@dataclass(frozen=True, eq=False)
class CoordinateSpaceContext:
    """Private Rust-decoded context with explicit profile and point-map targets.

    Bare point conversion is mathematical and carries no native entity source.
    Framed selections qualify and place within the same document snapshot first.
    Contextual mm values are not an entity's immutable physical-unit declaration.
    """

    _handle: _core._CoordinateSpaceHandle = field(repr=False, compare=False)

    @property
    def profile(self) -> CoordinateSpaceProfile:
        return CoordinateSpaceProfile(self._handle.metadata()["profile"])

    @property
    def evidence(self) -> CoordinateSpaceEvidence:
        row = self._handle.metadata()
        return CoordinateSpaceEvidence(
            row["contents_sha256"],
            row["observed_profile"],
            row["header_offset"],
            row["header_bytes"],
        )

    @property
    def drawing_origin(self) -> tuple[float, float]:
        return tuple(self._handle.metadata()["drawing_origin"])

    @property
    def drawing_origin_offset(self) -> int:
        return self._handle.metadata()["drawing_origin_offset"]

    @property
    def drawing_origin_bytes(self) -> bytes:
        return self._handle.metadata()["drawing_origin_bytes"]

    @property
    def origin_units_raw(self) -> int:
        return self._handle.metadata()["origin_units_raw"]

    @property
    def origin_units_offset(self) -> int:
        return self._handle.metadata()["origin_units_offset"]

    @property
    def origin_units_bytes(self) -> bytes:
        return self._handle.metadata()["origin_units_bytes"]

    @property
    def input_unit(self) -> NativeCoordinateInputUnitInfo:
        """Current input/display convention; no whole-file physical-unit claim."""
        return NativeCoordinateInputUnitInfo(self._handle.input_unit())

    @property
    def draft(self) -> SavedDraftState:
        row = self._handle.metadata()
        return SavedDraftState(
            row["draft_angle_offset"],
            row["draft_angle_bytes"],
            row["draft_angle_degrees"],
            row["draft_object_offset"],
            row["draft_object_bytes"],
            row["draft_object_raw"],
            row["draft_enable_offset"],
            row["draft_enable_bytes"],
            row["draft_enable_raw"],
        )

    @property
    def paper_scale_factor(self) -> float:
        return self._handle.metadata()["paper_scale_factor"]

    @property
    def paper_scale_offset(self) -> int:
        return self._handle.metadata()["paper_scale_offset"]

    @property
    def paper_scale_bytes(self) -> bytes:
        return self._handle.metadata()["paper_scale_bytes"]

    @property
    def annotation_ratio(self) -> float:
        return self._handle.metadata()["annotation_ratio"]

    @property
    def annotation_ratio_offset(self) -> int:
        return self._handle.metadata()["annotation_ratio_offset"]

    @property
    def annotation_ratio_bytes(self) -> bytes:
        return self._handle.metadata()["annotation_ratio_bytes"]

    @property
    def drawing_denominator(self) -> float:
        return self._handle.metadata()["drawing_denominator"]

    @property
    def annotation_denominator(self) -> float:
        return self._handle.metadata()["annotation_denominator"]

    def from_root_stored_xy(
        self,
        point: tuple[float, float] | list[float],
        *,
        frame: CoordinateFrame | str,
    ) -> tuple[float, float]:
        """Convert explicitly supplied root-stored XY; no source is reopened."""
        return tuple(self._handle.from_root_stored_xy(_point(point), str_value(frame)))

    def to_root_stored_xy(
        self,
        point: tuple[float, float] | list[float],
        *,
        frame: CoordinateFrame | str,
    ) -> tuple[float, float]:
        """Invert a named contextual map without modifying any source."""
        return tuple(self._handle.to_root_stored_xy(_point(point), str_value(frame)))


def str_value(value: CoordinateFrame | CoordinateSpaceProfile | str) -> str:
    if isinstance(value, (CoordinateFrame, CoordinateSpaceProfile)):
        return value.value
    if not isinstance(value, str):
        raise TypeError("profile/frame must be its enum or an explicit string")
    return value


@dataclass(frozen=True)
class FramedLine2D:
    """Named endpoints beside the untouched stored-XY qualified source line."""

    source: Line2D
    start: tuple[float, float]
    end: tuple[float, float]
    frame: CoordinateFrame

    @property
    def units(self) -> Literal["mm"] | None:
        return self.frame.units

    @property
    def coordinate_semantics(self) -> str:
        return self.frame.value

    @property
    def unresolved_semantics(self) -> tuple[str, ...]:
        return self.source.unresolved_semantics


@dataclass(frozen=True)
class FramedLineSelection:
    """Same-snapshot bounded source selection and explicit contextual endpoints."""

    source: QualifiedLineSelection
    context: CoordinateSpaceContext
    frame: CoordinateFrame
    lines: tuple[FramedLine2D, ...]
    document_complete: Literal[False] = field(default=False, init=False)

    @property
    def units(self) -> Literal["mm"] | None:
        return self.frame.units


def _framed_selection(row: dict) -> FramedLineSelection:
    source = _qualified_selection(row["source"])
    context = CoordinateSpaceContext(row["context"])
    frame = CoordinateFrame(row["frame"])
    coordinates = row["coordinates"]
    if (
        row["units"] != frame.units
        or row["document_complete"] is not False
        or len(coordinates) != len(source.lines)
    ):
        raise ValueError("coordinate-space contract differs across language boundary")
    lines = tuple(
        FramedLine2D(line, tuple(points["start"]), tuple(points["end"]), frame)
        for line, points in zip(source.lines, coordinates, strict=True)
    )
    return FramedLineSelection(source, context, frame, lines)
