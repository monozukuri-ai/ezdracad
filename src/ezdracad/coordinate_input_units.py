"""Native numeric input units, preserving the application's own coefficients."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from . import _core
from .origin_units import OriginUnitsCode


class NativeCoordinateInputUnit(str, Enum):
    MILLIMETERS = "mm"
    CENTIMETERS = "cm"
    METERS = "m"
    KILOMETERS = "km"
    INCHES = "inch"
    FEET = "feet"
    YARDS = "yard"
    UNKNOWN = "unknown"


def _scalar(value: float) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise TypeError(
            "native coordinate input scalar must be an int or float, not bool"
        )
    return float(value)


@dataclass(frozen=True, eq=False)
class NativeCoordinateInputUnitInfo:
    """Owned Origin.Units code and numeric input convention.

    The imperial display factors intentionally retain the native rounded
    constants. to_native_mm is not an exact-SI converter and does not identify
    immutable physical units of stored geometry, apply an origin, or apply D/A.
    Unknown codes retain signed values and source bytes but reject conversions.
    Rust performs all decoding and finite/overflow/nonzero-underflow checks.
    """

    _handle: _core._NativeCoordinateInputUnitHandle = field(repr=False, compare=False)

    @property
    def origin_units(self) -> OriginUnitsCode:
        value = self._handle.metadata()
        return OriginUnitsCode(
            value["origin_units_offset"],
            value["origin_units_bytes"],
            value["origin_units_code"],
        )

    @property
    def unit(self) -> NativeCoordinateInputUnit:
        return NativeCoordinateInputUnit(self._handle.metadata()["unit"])

    @property
    def native_display_factor(self) -> float | None:
        return self._handle.metadata()["native_display_factor"]

    def to_native_mm(self, value: float) -> float:
        """Native base-mm scalar = input / native_display_factor; signed."""
        return self._handle.to_native_mm(_scalar(value))

    def from_native_mm(self, value: float) -> float:
        """Native input scalar = base-mm value * native_display_factor; signed."""
        return self._handle.from_native_mm(_scalar(value))
