"""Explicit native print policy and nominal vector pen widths on paper."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from . import _core
from .effective_styles import EffectiveStyleIndices, StyleSource
from .styles import StyleTables, _index


@dataclass(frozen=True)
class PrintWidthPolicy:
    """Caller-supplied settings; this class does not infer persisted native flags.

    Application-managed opaque vector output is an explicit caller assertion
    that printer-width override and transparency rasterisation are absent.
    Automatic reduction requires a separately resolved output denominator.
    """

    drawing_denominator: float
    width_by_color: bool = False
    suppress_zero_width: bool = False
    reduction_mode: bool = False
    output_scale: Literal["drawing", "specified", "automatic"] = "drawing"
    output_denominator: float | None = None
    device_context: Literal[
        "unspecified",
        "application_managed_opaque_vectors",
        "driver_managed",
        "rasterized_transparency",
    ] = "unspecified"

    def _values(self) -> tuple:
        for name in ("width_by_color", "suppress_zero_width", "reduction_mode"):
            if type(getattr(self, name)) is not bool:
                raise TypeError(f"{name} must be bool")
        for name in ("drawing_denominator", "output_denominator"):
            value = getattr(self, name)
            if value is None and name == "output_denominator":
                continue
            if not isinstance(value, (int, float)) or isinstance(value, bool):
                raise TypeError(f"{name} must be a number, not bool")
        return (
            self.width_by_color,
            self.suppress_zero_width,
            self.reduction_mode,
            float(self.drawing_denominator),
            self.output_scale,
            None if self.output_denominator is None else float(self.output_denominator),
            self.device_context,
        )


@dataclass(frozen=True)
class PrintWidthResolution:
    """Owned selected-row evidence and a nominal request, never physical ink width.

    A None nominal value is accompanied by a suppression, hairline or hold status.
    These decisions do not evaluate clipping, layer visibility or white-output
    suppression. Block geometric scale is separate from output pen width.
    """

    status: Literal[
        "resolved_nominal_mm",
        "suppressed_zero",
        "device_hairline",
        "held_device_context",
        "held_driver_policy",
        "held_rasterization",
        "held_output_scale",
    ]
    policy: PrintWidthPolicy
    selected_table_domain: Literal["width_table", "color_table"]
    selected_index: int
    selected_row_offset: int
    selected_row_bytes: bytes
    style_sources: tuple[StyleSource, ...]
    pen_width_hundredths_raw: int
    base_pen_mm: float
    applied_scale_factor: float | None
    nominal_paper_mm: float | None


def resolve_nominal_print_width(
    styles: EffectiveStyleIndices, tables: StyleTables, policy: PrintWidthPolicy
) -> PrintWidthResolution:
    """Apply explicit printing policy after native style-index inheritance.

    All table selection and arithmetic is performed in Rust over owned tables.
    No source file is reopened. Rust returns the selected row evidence; the
    already-owned effective-index source chain is retained unchanged.
    """
    if not isinstance(styles, EffectiveStyleIndices):
        raise TypeError("styles must be EffectiveStyleIndices")
    if not isinstance(tables, StyleTables):
        raise TypeError("tables must be StyleTables")
    if not isinstance(policy, PrintWidthPolicy):
        raise TypeError("policy must be PrintWidthPolicy")
    values = policy._values()
    row = _core.resolve_nominal_print_width(
        tables._handle, _index(styles.color.index), _index(styles.width.index), values
    )
    selected = styles.color if policy.width_by_color else styles.width
    return PrintWidthResolution(policy=policy, style_sources=selected.sources, **row)
