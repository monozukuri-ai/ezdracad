"""Typed unknown unit/history states and explicit external creation-scale input."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from . import _core
from .coordinate_input_units import NativeCoordinateInputUnitInfo
from .coordinate_measurement import CoordinateMeasurementContext
from .origin_units import OriginUnitsCode


class PhysicalUnitStatus(str, Enum):
    UNRESOLVED = "unresolved"


class PhysicalUnitUnresolvedReason(str, Enum):
    STORED_COORDINATE_UNIT_NOT_ESTABLISHED = "stored_coordinate_unit_not_established"


@dataclass(frozen=True)
class PhysicalUnitResolution:
    status: PhysicalUnitStatus
    reason: PhysicalUnitUnresolvedReason


class CreationScaleResolution(str, Enum):
    """Availability from qualified decoded fields, not the entire file format."""

    UNAVAILABLE_FROM_DECODED_FIELDS = "unavailable_from_decoded_fields"


class CreationScaleProvenance(str, Enum):
    CALLER_DECLARED_UNVERIFIED = "caller_declared_unverified"


@dataclass(frozen=True, eq=False)
class UnitResolution:
    """Owned marker-25 sources with explicit unresolved unit/history status.

    Current scales describe current document measurement context. They are not
    the original scales of an arbitrary entity. Stored geometry retains its
    unresolved physical units; numeric input units are reported separately.
    """

    _handle: _core._UnitResolutionHandle = field(repr=False, compare=False)

    @property
    def physical_units(self) -> PhysicalUnitResolution:
        value = self._handle.metadata()
        return PhysicalUnitResolution(
            PhysicalUnitStatus(value["physical_unit_status"]),
            PhysicalUnitUnresolvedReason(value["physical_unit_reason"]),
        )

    @property
    def original_creation_scale(self) -> CreationScaleResolution:
        return CreationScaleResolution(
            self._handle.metadata()["original_creation_scale"]
        )

    @property
    def current_measurement(self) -> CoordinateMeasurementContext:
        return CoordinateMeasurementContext(self._handle.current_measurement())

    @property
    def origin_units(self) -> OriginUnitsCode:
        value = self._handle.metadata()
        return OriginUnitsCode(
            value["origin_units_offset"],
            value["origin_units_bytes"],
            value["origin_units_code"],
        )

    @property
    def coordinate_input_unit(self) -> NativeCoordinateInputUnitInfo:
        return NativeCoordinateInputUnitInfo(self._handle.coordinate_input_unit())


def _number(value: float, label: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise TypeError(f"{label} must be an int or float, not bool")
    return float(value)


@dataclass(frozen=True, eq=False, init=False)
class DeclaredCreationScale:
    """Both original scale denominators supplied by an external caller.

    This declaration is unverified and is never recovered automatically from
    the file. length_mm returns stored_length * original_A / original_D under
    that declaration. Supplying only original_A with current_D is insufficient.
    Rust validates both positive finite denominators and all conversion results.
    """

    _handle: _core._DeclaredCreationScaleHandle = field(repr=False, compare=False)

    def __init__(
        self,
        original_drawing_denominator: float,
        original_annotation_denominator: float,
    ) -> None:
        object.__setattr__(
            self,
            "_handle",
            _core._DeclaredCreationScaleHandle(
                _number(original_drawing_denominator, "original drawing denominator"),
                _number(
                    original_annotation_denominator,
                    "original annotation denominator",
                ),
            ),
        )

    @property
    def original_drawing_denominator(self) -> float:
        return self._handle.metadata()["original_drawing_denominator"]

    @property
    def original_annotation_denominator(self) -> float:
        return self._handle.metadata()["original_annotation_denominator"]

    @property
    def original_annotation_ratio(self) -> float:
        return self._handle.metadata()["original_annotation_ratio"]

    @property
    def provenance(self) -> CreationScaleProvenance:
        return CreationScaleProvenance(self._handle.metadata()["provenance"])

    def length_mm(self, stored_length: float) -> float:
        """Declared original length, conditional on the supplied original D/A."""
        return self._handle.length_mm(_number(stored_length, "stored length"))
