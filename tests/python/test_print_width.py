"""Explicit output-policy checks; native persisted-flag qualification is separate."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError, replace
from pathlib import Path

import pytest

from ezdracad import InvalidFormatError, UnsupportedFeatureError, _core, raw
from ezdracad.effective_styles import (
    EffectiveStyleIndices,
    ResolvedStyleIndex,
    StyleSource,
)
from ezdracad.print_width import PrintWidthPolicy, resolve_nominal_print_width

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "print_width_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)


def inputs(marker=25, width=20, color_width=70):
    data = bytearray(assembler.make_semantics_contents(marker))

    def at(field):
        return struct.unpack_from("<I", data, field)[0]

    struct.pack_into("<H", data, at(0x34) + 4 * 64 + 2, width)
    struct.pack_into("<H", data, at(0x2C) + 64 + 2, color_width)
    scan = raw.scan_records(assembler.wrap_record_contents(data))
    tables = scan.style_tables()
    row = tables.width(5)
    source = StyleSource(
        "width", "width_table", 5, None, row.payload_offset, row.raw_bytes
    )
    styles = EffectiveStyleIndices(
        ResolvedStyleIndex(2, ()),
        ResolvedStyleIndex(1, ()),
        ResolvedStyleIndex(5, (source,)),
    )
    policy = PrintWidthPolicy(100, device_context="application_managed_opaque_vectors")
    return scan, tables, styles, policy


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_explicit_policy_selects_output_mm_and_retains_owned_evidence(marker):
    scan, tables, styles, p = inputs(marker)
    result = resolve_nominal_print_width(styles, tables, p)
    assert result.status == "resolved_nominal_mm"
    assert result.nominal_paper_mm == 0.2
    assert result.style_sources == styles.width.sources
    assert result.selected_row_bytes == tables.width(5).raw_bytes
    assert result.selected_row_offset == tables.width(5).payload_offset
    assert (
        scan.read_contents()[
            result.selected_row_offset : result.selected_row_offset + 64
        ]
        == result.selected_row_bytes
    )
    color = resolve_nominal_print_width(styles, tables, replace(p, width_by_color=True))
    assert color.selected_table_domain == "color_table"
    assert color.selected_index == 2
    assert color.base_pen_mm == color.nominal_paper_mm == 0.7
    assert color.selected_row_bytes == tables.color(2).raw_bytes
    with pytest.raises(FrozenInstanceError):
        result.status = "device_hairline"


def test_drawing_reduction_and_auto_fit_keep_distinct_information_requirements():
    _, tables, styles, p = inputs()
    specified = replace(p, output_scale="specified", output_denominator=200)
    assert (
        resolve_nominal_print_width(styles, tables, specified).nominal_paper_mm == 0.2
    )
    specified = replace(specified, reduction_mode=True)
    assert (
        resolve_nominal_print_width(styles, tables, specified).nominal_paper_mm == 0.1
    )
    automatic = replace(p, output_scale="automatic", reduction_mode=True)
    result = resolve_nominal_print_width(styles, tables, automatic)
    assert result.status == "held_output_scale"
    assert result.nominal_paper_mm is result.applied_scale_factor is None
    assert (
        resolve_nominal_print_width(
            styles, tables, replace(automatic, output_denominator=50)
        ).nominal_paper_mm
        == 0.4
    )
    assert (
        resolve_nominal_print_width(
            styles, tables, replace(automatic, reduction_mode=False)
        ).nominal_paper_mm
        == 0.2
    )
    assert (
        resolve_nominal_print_width(
            styles, tables, replace(p, reduction_mode=True)
        ).nominal_paper_mm
        == 0.2
    )


@pytest.mark.parametrize(
    "context,status",
    [
        ("unspecified", "held_device_context"),
        ("driver_managed", "held_driver_policy"),
        ("rasterized_transparency", "held_rasterization"),
    ],
)
def test_device_context_never_silently_promotes_to_a_final_width(context, status):
    _, tables, styles, p = inputs()
    result = resolve_nominal_print_width(
        styles, tables, replace(p, device_context=context)
    )
    assert result.status == status
    assert result.nominal_paper_mm is None
    assert result.base_pen_mm == 0.2


@pytest.mark.parametrize("by_color", [False, True])
def test_zero_is_suppression_or_device_hairline(by_color):
    _, tables, styles, p = inputs(width=0, color_width=0)
    p = replace(p, width_by_color=by_color)
    result = resolve_nominal_print_width(styles, tables, p)
    assert result.status == "device_hairline"
    assert result.nominal_paper_mm is None
    result = resolve_nominal_print_width(
        styles, tables, replace(p, suppress_zero_width=True)
    )
    assert result.status == "suppressed_zero"
    assert result.nominal_paper_mm is None


@pytest.mark.parametrize("bad", [0, -1, float("nan"), float("inf"), float("-inf")])
def test_invalid_denominators_reject_even_when_reduction_is_off(bad):
    _, tables, styles, p = inputs()
    with pytest.raises(InvalidFormatError):
        resolve_nominal_print_width(styles, tables, replace(p, drawing_denominator=bad))
    with pytest.raises(InvalidFormatError):
        resolve_nominal_print_width(
            styles, tables, replace(p, output_scale="specified", output_denominator=bad)
        )


def test_missing_indices_unknown_policy_and_type_boundaries_reject():
    _, tables, styles, p = inputs()
    for bad in [0, 17, 257]:
        changed = replace(styles, width=ResolvedStyleIndex(bad, ()))
        with pytest.raises(UnsupportedFeatureError):
            resolve_nominal_print_width(changed, tables, p)
    for policy in [
        replace(p, output_scale="unspecified"),
        replace(p, output_scale="specified"),
        replace(p, output_denominator=100),
        replace(p, device_context="assume_default"),
    ]:
        with pytest.raises(ValueError):
            resolve_nominal_print_width(styles, tables, policy)
    for kwargs in [
        dict(width_by_color=1),
        dict(drawing_denominator=True),
        dict(output_denominator=False),
    ]:
        with pytest.raises(TypeError):
            resolve_nominal_print_width(styles, tables, replace(p, **kwargs))
    for args in [
        (True, 5, p._values()),
        (2, False, p._values()),
        (2, 5, (False, False, False, True, "drawing", None, "unspecified")),
        (2, 5, (1, False, False, 100, "drawing", None, "unspecified")),
    ]:
        with pytest.raises(TypeError):
            _core.resolve_nominal_print_width(tables._handle, *args)
