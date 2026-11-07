"""Explicit contextual length measurements; no general geometry-unit inference."""

from __future__ import annotations

from dataclasses import dataclass, field

from . import _core


def _length(value: float) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise TypeError("raw length must be an int or float, not bool")
    return float(value)


@dataclass(frozen=True, eq=False)
class CoordinateMeasurementContext:
    """Owned scale snapshot for the checked native measurement convention.

    Methods operate on an explicitly supplied nonnegative raw length. They do
    not normalize coordinates or declare an entity's immutable physical units.
    Current-annotation results change with the saved active annotation context.
    Paper results exclude printer settings and output overrides. Rust performs
    all decoding, finite-value and conversion validation; no source is reopened.
    """

    _handle: _core._CoordinateMeasurementHandle = field(repr=False, compare=False)

    @property
    def paper_scale_offset(self) -> int:
        return self._handle.metadata()["paper_scale_offset"]

    @property
    def paper_scale_bytes(self) -> bytes:
        return self._handle.metadata()["paper_scale_bytes"]

    @property
    def paper_scale_factor(self) -> float:
        return self._handle.metadata()["paper_scale_factor"]

    @property
    def annotation_ratio_offset(self) -> int:
        return self._handle.metadata()["annotation_ratio_offset"]

    @property
    def annotation_ratio_bytes(self) -> bytes:
        return self._handle.metadata()["annotation_ratio_bytes"]

    @property
    def annotation_ratio(self) -> float:
        return self._handle.metadata()["annotation_ratio"]

    @property
    def drawing_denominator(self) -> float:
        return self._handle.metadata()["drawing_denominator"]

    @property
    def annotation_denominator(self) -> float:
        return self._handle.metadata()["annotation_denominator"]

    def length_on_paper_mm(self, raw_length: float) -> float:
        """Length at configured drawing output scale; no printer override."""
        return self._handle.length_on_paper_mm(_length(raw_length))

    def length_at_current_annotation_mm(self, raw_length: float) -> float:
        """Length under the current annotation context; no creation-scale claim."""
        return self._handle.length_at_current_annotation_mm(_length(raw_length))
