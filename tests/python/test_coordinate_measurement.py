"""Current measurement context is explicit and leaves raw geometry unchanged."""

import importlib.util
import math
import struct
import sys
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

from ezdracad import (
    InvalidFormatError,
    ParseLimits,
    _core,
    raw,
)
from ezdracad.coordinate_measurement import CoordinateMeasurementContext

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "coordinate_measurement_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def contents(marker=25, factor=0.01, ratio=0.5):
    payload = bytearray(assembler.make_record_contents(marker))
    struct.pack_into("<d", payload, 0x178, factor)
    struct.pack_into("<d", payload, 0x278, ratio)
    payload[0x170:0x178] = bytes([0xA5]) * 8
    payload[0x270:0x278] = bytes([0x5A]) * 8
    payload[0x280:0x288] = bytes([0xFF]) * 8
    return payload


def context(marker=25, factor=0.01, ratio=0.5):
    scan = raw.scan_records(
        assembler.wrap_record_contents(contents(marker, factor, ratio))
    )
    return scan, scan.coordinate_measurement_context()


@pytest.mark.parametrize(
    "factor,ratio,drawing,annotation,paper,current",
    [
        (0.01, 1.0, 100, 100, 10, 1000),
        (0.01, 0.5, 100, 200, 10, 2000),
        (0.04, 1.0, 25, 25, 40, 1000),
        (0.04, 0.25, 25, 100, 40, 4000),
        (0.01, 5.0, 100, 20, 10, 200),
    ],
)
def test_contextual_lengths_preserve_sources_and_keep_geometry_unqualified(
    factor, ratio, drawing, annotation, paper, current, tmp_path
):
    payload = contents(factor=factor, ratio=ratio)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "measurement.mpz"
    path.write_bytes(envelope)
    scan = raw.scanfile(path)
    before = scan.line_records()
    path.unlink()
    value = scan.coordinate_measurement_context()
    assert isinstance(value, CoordinateMeasurementContext)
    assert value.paper_scale_offset == 0x178
    assert value.paper_scale_bytes == struct.pack("<d", factor)
    assert value.paper_scale_factor == factor
    assert value.annotation_ratio_offset == 0x278
    assert value.annotation_ratio_bytes == struct.pack("<d", ratio)
    assert value.annotation_ratio == ratio
    assert value.drawing_denominator == drawing
    assert value.annotation_denominator == annotation
    assert value.length_on_paper_mm(1000) == paper
    assert value.length_at_current_annotation_mm(1000) == current
    assert value.length_on_paper_mm(0.0) == 0.0
    assert value.length_at_current_annotation_mm(0.0) == 0.0
    assert math.copysign(1, value.length_on_paper_mm(-0.0)) == -1
    native = _core.scan_bytes(
        envelope, ParseLimits()._values()
    ).coordinate_measurement_context()
    assert native.metadata() == value._handle.metadata()
    assert scan.line_records() == before
    assert scan.read_contents() == payload
    assert all(
        line.units is None and line.coordinate_space == "record_local"
        for line in before
    )
    assert not hasattr(value, "geometry_unit") and not hasattr(value, "origin_units")
    with pytest.raises(FrozenInstanceError):
        value.paper_scale_factor = 1


@pytest.mark.parametrize(
    "factor,ratio",
    [
        (0.0, 1.0),
        (-0.0, 1.0),
        (-1.0, 1.0),
        (float("nan"), 1.0),
        (float("inf"), 1.0),
        (float("-inf"), 1.0),
        (0.01, 0.0),
        (0.01, -0.0),
        (0.01, -1.0),
        (0.01, float("nan")),
        (0.01, float("inf")),
        (0.01, float("-inf")),
        (float.fromhex("0x0.0000000000001p-1022"), 1),
        (sys.float_info.max, sys.float_info.max),
        (sys.float_info.min, sys.float_info.min),
        (1e-200, 1e-109),
    ],
)
def test_invalid_context_is_rejected_on_demand(factor, ratio):
    payload = contents(factor=factor, ratio=ratio)
    scan = raw.scan_records(assembler.wrap_record_contents(payload))
    with pytest.raises(InvalidFormatError, match="coordinate measurement"):
        scan.coordinate_measurement_context()
    assert scan.line_candidate_count == 2
    assert len(scan.line_records()) == 2
    assert scan.read_contents() == payload


@pytest.mark.parametrize("value", [-1, float("nan"), float("inf"), float("-inf")])
def test_invalid_raw_length_does_not_produce_a_measurement(value):
    _, info = context()
    with pytest.raises(InvalidFormatError, match="raw length"):
        info.length_on_paper_mm(value)
    with pytest.raises(InvalidFormatError, match="raw length"):
        info.length_at_current_annotation_mm(value)


@pytest.mark.parametrize("value", [True, False, "1000", b"1000", None, [], object()])
def test_numeric_types_and_direct_binding_bool_are_not_coerced(value):
    _, info = context()
    with pytest.raises(TypeError):
        info.length_on_paper_mm(value)
    with pytest.raises(TypeError):
        info.length_at_current_annotation_mm(value)
    if isinstance(value, bool):
        with pytest.raises(TypeError, match="bool"):
            info._handle.length_on_paper_mm(value)
        with pytest.raises(TypeError, match="bool"):
            info._handle.length_at_current_annotation_mm(value)


def test_conversion_overflow_nonzero_underflow_and_huge_integer_are_explicit():
    _, info = context(factor=sys.float_info.max, ratio=1)
    with pytest.raises(InvalidFormatError, match="converted length"):
        info.length_on_paper_mm(2)
    _, info = context(factor=0.01, ratio=1e-200)
    with pytest.raises(InvalidFormatError, match="converted length"):
        info.length_at_current_annotation_mm(sys.float_info.max)
    _, info = context(factor=0.01, ratio=5)
    smallest = float.fromhex("0x0.0000000000001p-1022")
    with pytest.raises(InvalidFormatError, match="underflow"):
        info.length_on_paper_mm(smallest)
    with pytest.raises(InvalidFormatError, match="underflow"):
        info.length_at_current_annotation_mm(smallest)
    with pytest.raises(OverflowError):
        info.length_on_paper_mm(10**1000)


@pytest.mark.parametrize("marker", [5, 8])
@pytest.mark.parametrize(
    "factor,ratio,drawing,annotation,paper,measured",
    [(0.01, 0.5, 100, 200, 10, 2000), (0.04, 1.0, 25, 25, 40, 1000)],
)
def test_legacy_contexts_match_separate_native_holdouts_without_changing_entity_units(
    marker, factor, ratio, drawing, annotation, paper, measured
):
    payload = contents(marker=marker, factor=factor, ratio=ratio)
    scan = raw.scan_records(assembler.wrap_record_contents(payload))
    info = scan.coordinate_measurement_context()
    assert info.paper_scale_bytes == struct.pack("<d", factor)
    assert info.annotation_ratio_bytes == struct.pack("<d", ratio)
    assert info.drawing_denominator == drawing
    assert info.annotation_denominator == annotation
    assert info.length_on_paper_mm(1000) == paper
    assert info.length_at_current_annotation_mm(1000) == measured
    assert len(scan.line_records()) == 2
    assert all(line.units is None for line in scan.line_records())
    assert scan.read_contents() == payload
