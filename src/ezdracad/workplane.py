"""Explicit saved 3D workplane maps in unresolved stored coordinate units.

Preset directions select fixed axes; the saved axis matrix applies only to
direction FREE and may contain cached state under presets. These bytes do not
recover a previous free-mode orientation. Bare supplied points carry no native
entity provenance.
Admission and all coordinate arithmetic occur in Rust on the owned snapshot.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from . import _core
from .coordinate_space import CoordinateSpaceEvidence


class WorkplaneProfile(str, Enum):
    MARKER25_SAVED_WORKPLANE_V1 = "marker25_saved_workplane_v1"


class WorkplaneDirection(str, Enum):
    FREE = "free"
    PLAN = "plan"
    EAST = "east"
    SOUTH = "south"
    WEST = "west"
    NORTH = "north"


class WorkplaneBasisSource(str, Enum):
    STORED_AXES_FOR_FREE = "stored_axes_for_free"
    DERIVED_PRESET = "derived_preset"


def str_value(profile: WorkplaneProfile | str) -> str:
    if isinstance(profile, WorkplaneProfile):
        return profile.value
    if not isinstance(profile, str):
        raise TypeError("profile must be a WorkplaneProfile or an explicit string")
    return profile


def _point(
    value: tuple[float, float, float] | list[float],
) -> tuple[float, float, float]:
    if not isinstance(value, (tuple, list)) or len(value) != 3:
        raise TypeError("point must be a three-number tuple or list")
    if any(not isinstance(x, (int, float)) or isinstance(x, bool) for x in value):
        raise TypeError("point coordinates must be int or float, not bool")
    return float(value[0]), float(value[1]), float(value[2])


@dataclass(frozen=True)
class WorkplaneEvidence:
    """Exact plane and admission-guard headers; opaque gaps remain uninterpreted."""

    contents_sha256: str
    observed_profile: str
    header_offset: int
    header_bytes: bytes
    coordinate_guard: CoordinateSpaceEvidence


@dataclass(frozen=True, eq=False)
class WorkplaneContext:
    """A Rust-decoded, explicit saved point frame; no physical-unit declaration.

    origin and axis vectors use stored world coordinates. This context neither
    qualifies a geometry selection nor decodes camera, layout or creation scale.
    """

    _handle: _core._WorkplaneHandle = field(repr=False, compare=False)

    @property
    def profile(self) -> WorkplaneProfile:
        return WorkplaneProfile(self._handle.metadata()["profile"])

    @property
    def direction(self) -> WorkplaneDirection:
        return WorkplaneDirection(self._handle.metadata()["direction"])

    @property
    def direction_raw(self) -> int:
        return self._handle.metadata()["direction_raw"]

    @property
    def direction_offset(self) -> int:
        return self._handle.metadata()["direction_offset"]

    @property
    def direction_bytes(self) -> bytes:
        return self._handle.metadata()["direction_bytes"]

    @property
    def origin(self) -> tuple[float, float, float]:
        return tuple(self._handle.metadata()["origin"])

    @property
    def origin_offset(self) -> int:
        return self._handle.metadata()["origin_offset"]

    @property
    def origin_bytes(self) -> bytes:
        return self._handle.metadata()["origin_bytes"]

    @property
    def stored_axes(self) -> tuple[tuple[float, float, float], ...]:
        return tuple(tuple(axis) for axis in self._handle.metadata()["stored_axes"])

    @property
    def stored_axes_offset(self) -> int:
        return self._handle.metadata()["stored_axes_offset"]

    @property
    def stored_axes_bytes(self) -> bytes:
        return self._handle.metadata()["stored_axes_bytes"]

    @property
    def effective_axes(self) -> tuple[tuple[float, float, float], ...]:
        """Workplane +X,+Y,+Z unit vectors in stored world axes."""
        return tuple(tuple(axis) for axis in self._handle.metadata()["effective_axes"])

    @property
    def basis_source(self) -> WorkplaneBasisSource:
        return WorkplaneBasisSource(self._handle.metadata()["basis_source"])

    @property
    def units(self) -> None:
        return None

    @property
    def evidence(self) -> WorkplaneEvidence:
        row = self._handle.metadata()
        return WorkplaneEvidence(
            row["contents_sha256"],
            row["observed_profile"],
            row["header_offset"],
            row["header_bytes"],
            CoordinateSpaceEvidence(
                row["contents_sha256"],
                row["observed_profile"],
                row["guard_header_offset"],
                row["guard_header_bytes"],
            ),
        )

    def from_world_stored_xyz(
        self,
        point: tuple[float, float, float] | list[float],
    ) -> tuple[float, float, float]:
        """Convert a supplied stored-world point into the saved workplane."""
        return tuple(self._handle.from_world_stored_xyz(_point(point)))

    def to_world_stored_xyz(
        self,
        point: tuple[float, float, float] | list[float],
    ) -> tuple[float, float, float]:
        """Invert the saved map without changing/reopening the source."""
        return tuple(self._handle.to_world_stored_xyz(_point(point)))
