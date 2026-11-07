"""Owned native print-setting evidence; only independently controlled fields."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

from . import _core
from .print_width import PrintWidthPolicy


@dataclass(frozen=True)
class RawPrintSettingField:
    """One exact Drawing-payload-relative source field, including unqualified raw bits."""

    name: str
    payload_offset: int
    raw_bytes: bytes


@dataclass(frozen=True, eq=False)
class PrintSettings:
    """Marker-25 native saved print policy, independent of MPXL defaults.

    Unknown raw switches/modes remain inspectable. Conversion to a width policy
    fails on unqualified values. Printer override and reduction are retained
    from native binary fields; observed MPXL exports omit these switches.
    """

    _handle: _core._PrintSettingsHandle = field(repr=False, compare=False)

    @property
    def marker_raw(self) -> int:
        return self._handle.metadata()["marker_raw"]

    @property
    def fields(self) -> tuple[RawPrintSettingField, ...]:
        return tuple(
            RawPrintSettingField(**row) for row in self._handle.metadata()["fields"]
        )

    @property
    def drawing_paper_scale_factor(self) -> float:
        return self._handle.metadata()["drawing_paper_scale_factor"]

    @property
    def printer_settings_raw(self) -> int:
        return self._handle.metadata()["printer_settings_raw"]

    @property
    def output_scale_mode_raw(self) -> int:
        return self._handle.metadata()["output_scale_mode_raw"]

    @property
    def width_by_color_raw(self) -> int:
        return self._handle.metadata()["width_by_color_raw"]

    @property
    def print_scale_factor(self) -> float:
        return self._handle.metadata()["print_scale_factor"]

    @property
    def reduction_mode_raw(self) -> int:
        return self._handle.metadata()["reduction_mode_raw"]

    @property
    def suppress_zero_width_raw(self) -> int:
        return self._handle.metadata()["suppress_zero_width_raw"]

    def width_policy(
        self,
        *,
        device_context: Literal[
            "unspecified",
            "application_managed_opaque_vectors",
            "driver_managed",
            "rasterized_transparency",
        ] = "unspecified",
        automatic_output_denominator: float | None = None,
    ) -> PrintWidthPolicy:
        """Convert qualified saved switches into explicit nominal-width policy.

        A saved driver override forces a driver hold regardless of the supplied
        context. A separately resolved denominator is required for automatic
        reduction. This method neither fits a page nor measures physical ink.
        """
        if automatic_output_denominator is not None and (
            not isinstance(automatic_output_denominator, (int, float))
            or isinstance(automatic_output_denominator, bool)
        ):
            raise TypeError("automatic output denominator must be a number, not bool")
        row = self._handle.width_policy(device_context, automatic_output_denominator)
        return PrintWidthPolicy(**row)
