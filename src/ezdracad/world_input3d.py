"""Explicit loaded-file absolute 3D input coordinates in contextual native mm.

This is saved current input context, not immutable source-geometry units or a
history decoder. Origin Z is the tested loaded-file zero convention; a stored
2D grid-origin field is never substituted for it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from . import _core
from .coordinate_input_units import NativeCoordinateInputUnitInfo
from .coordinate_space import CoordinateSpaceContext, CoordinateSpaceEvidence


class WorldInput3DProfile(str, Enum):
    MARKER25_LOADED_WORLD_INPUT_V1 = "marker25_loaded_world_input_v1"


def str_value(profile: WorldInput3DProfile | str) -> str:
    if isinstance(profile, WorldInput3DProfile):
        return profile.value
    if not isinstance(profile, str):
        raise TypeError("profile must be a WorldInput3DProfile or an explicit string")
    return profile


def _point(
    value: tuple[float, float, float] | list[float],
) -> tuple[float, float, float]:
    if not isinstance(value, (tuple, list)) or len(value) != 3:
        raise TypeError("point must be a three-number tuple or list")
    if any(not isinstance(x, (int, float)) or isinstance(x, bool) for x in value):
        raise TypeError("point coordinates must be int or float, not bool")
    return float(value[0]), float(value[1]), float(value[2])


@dataclass(frozen=True, eq=False)
class WorldInput3DContext:
    """A Rust-decoded loaded-file convention on bare stored-world points.

    No source entity, parent transform or scene is qualified by this point map.
    Native input codes 0/2 are admitted; use input_unit for literal digits rather
    than changing these mm-equivalent map components. Transient live state after
    a GUI origin edit is not recoverable from these saved source bytes.
    """

    _handle: _core._WorldInput3DHandle = field(repr=False, compare=False)

    @property
    def profile(self) -> WorldInput3DProfile:
        return WorldInput3DProfile(self._handle.metadata()["profile"])

    @property
    def coordinate_context(self) -> CoordinateSpaceContext:
        return CoordinateSpaceContext(self._handle.coordinate_context())

    @property
    def evidence(self) -> CoordinateSpaceEvidence:
        return self.coordinate_context.evidence

    @property
    def input_unit(self) -> NativeCoordinateInputUnitInfo:
        return self.coordinate_context.input_unit

    @property
    def loaded_world_origin(self) -> tuple[float, float, float]:
        return tuple(self._handle.metadata()["loaded_world_origin"])

    @property
    def origin_z_source(self) -> str:
        return self._handle.metadata()["origin_z_source"]

    @property
    def draw_origin_offset(self) -> int:
        return self._handle.metadata()["draw_origin_offset"]

    @property
    def draw_origin_bytes(self) -> bytes:
        return self._handle.metadata()["draw_origin_bytes"]

    @property
    def draw_origin_raw(self) -> int:
        return self._handle.metadata()["draw_origin_raw"]

    @property
    def annotation_ratio(self) -> float:
        return self.coordinate_context.annotation_ratio

    @property
    def drawing_denominator(self) -> float:
        return self.coordinate_context.drawing_denominator

    @property
    def annotation_denominator(self) -> float:
        return self.coordinate_context.annotation_denominator

    @property
    def units(self) -> str:
        """Units of the named input convention, not of any source geometry."""
        return self._handle.metadata()["units"]

    @property
    def coordinate_semantics(self) -> str:
        return self._handle.metadata()["coordinate_semantics"]

    def input_mm_from_world_stored_xyz(
        self, point: tuple[float, float, float] | list[float]
    ) -> tuple[float, float, float]:
        return tuple(self._handle.input_mm_from_world_stored_xyz(_point(point)))

    def world_stored_xyz_from_input_mm(
        self, point: tuple[float, float, float] | list[float]
    ) -> tuple[float, float, float]:
        return tuple(self._handle.world_stored_xyz_from_input_mm(_point(point)))
