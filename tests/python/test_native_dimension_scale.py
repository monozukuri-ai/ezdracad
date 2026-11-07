"""Admission, source retention and ratio/history separation for synthetic DIMs."""

import importlib.util
import math
import struct
import sys
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

from ezdracad import InvalidFormatError, UnsupportedFeatureError, raw
from ezdracad.dimension_scale import DimensionScale, DimensionScaleProfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location(
    "dimension_fixture", ROOT / "scripts/make_dimension_fixture.py"
)
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
PROFILE = DimensionScaleProfile.MARKER25_ALIGNED_NUMERIC_MM_V1
START = 8192


def scan(data):
    return raw.scan_records(fixture.wrap_record_contents(bytes(data)))


def test_owned_raw_sources_and_explicit_ratio_leave_general_units_and_history_unknown(
    tmp_path,
):
    data = fixture.make_dimension_contents()
    path = tmp_path / "dimension.mpz"
    path.write_bytes(fixture.wrap_record_contents(data))
    document = raw.scanfile(path)
    value = document.dimension_scale(0, profile=PROFILE)
    path.unlink()
    assert isinstance(value, DimensionScale)
    assert value.record_bytes == data[START : START + 640]
    assert value.stored_span_length == 1000
    assert value.recorded_value_mm == 2000
    assert value.annotation_to_drawing_ratio == 2
    assert value.current_drawing_denominator == 100
    assert value.equivalent_annotation_denominator == 200
    assert value.original_drawing_denominator is None
    assert value.original_annotation_denominator is None
    assert value.geometry_units is None and value.numeric_unit == "mm"
    assert value.coordinate_space == "record_local"
    assert value.document_complete is False
    assert document.unit_resolution().physical_units.status.value == "unresolved"
    for offset, source in (
        (value.geometry_offset, value.geometry_bytes),
        (value.number_offset, value.number_bytes),
        (value.numeric_flag_offset, value.numeric_flag_bytes),
        (value.unit_offset, value.unit_bytes),
        (value.current_drawing_scale_offset, value.current_drawing_scale_bytes),
    ):
        assert data[offset : offset + len(source)] == source
    with pytest.raises(FrozenInstanceError):
        value.recorded_value_mm = 4


def test_current_annotation_is_irrelevant_and_current_d_is_explicit():
    data = fixture.make_dimension_contents(current_ratio=math.nan)
    first = scan(data).dimension_scale(0, profile=PROFILE.value)
    other = scan(
        fixture.make_dimension_contents(drawing=25, current_ratio=0.25)
    ).dimension_scale(0, profile=PROFILE)
    assert first.annotation_to_drawing_ratio == other.annotation_to_drawing_ratio == 2
    assert other.equivalent_annotation_denominator == 50
    assert other.original_annotation_denominator is None


def test_custom_display_string_is_opaque_and_does_not_replace_saved_numeric_ratio():
    data = bytearray(fixture.make_dimension_contents())
    data[START + 8 * 64 + 16 : START + 8 * 64 + 64] = b"X".ljust(48, b"\0")
    value = scan(data).dimension_scale(0, profile=PROFILE)
    assert value.recorded_value_mm == 2000
    assert value.annotation_to_drawing_ratio == 2
    assert value.record_bytes[8 * 64 + 16 : 8 * 64 + 64] == b"X".ljust(48, b"\0")
    assert value.original_drawing_denominator is None


@pytest.mark.parametrize("tag", [0x1A, 0x41A])
def test_exact_native_tag_variants_are_retained(tag):
    data = bytearray(fixture.make_dimension_contents())
    struct.pack_into("<H", data, START, tag)
    value = scan(data).dimension_scale(0, profile=PROFILE)
    assert value.tag_raw == tag
    assert value.record_bytes[:2] == struct.pack("<H", tag)
    struct.pack_into("<H", data, START, 0x81A)
    with pytest.raises(UnsupportedFeatureError):
        scan(data).dimension_scale(0, profile=PROFILE)


@pytest.mark.parametrize(
    "offset,value",
    [
        (START, 0x1B),
        (START + 16, 2),
        (START + 17, 129),
        (START + 18, 3),
        (START + 64, 0x42),
        (START + 5 * 64 + 8, 3),
        (START + 6 * 64 + 12, 0),
        (START + 7 * 64 + 2, 17),
        (START + 7 * 64 + 4, 2),
    ],
)
def test_unqualified_semantics_fail_only_the_requested_reader(offset, value):
    data = bytearray(fixture.make_dimension_contents())
    data[offset] = value
    document = scan(data)
    with pytest.raises(UnsupportedFeatureError):
        document.dimension_scale(0, profile=PROFILE)
    assert document.read_contents() == data
    assert document.records(0, 1)


@pytest.mark.parametrize("value", [math.nan, math.inf, -1.0, 0.0, math.ulp(0.0)])
def test_nonfinite_nonpositive_and_unrepresentable_ratios_return_no_result(value):
    document = scan(fixture.make_dimension_contents(number=value))
    with pytest.raises((InvalidFormatError, UnsupportedFeatureError)):
        document.dimension_scale(0, profile=PROFILE)


@pytest.mark.parametrize("index", [1, 5, 9, 10, 11, 2**64 - 1])
def test_nonprimary_and_absent_selections_are_rejected(index):
    with pytest.raises(UnsupportedFeatureError):
        scan(fixture.make_dimension_contents()).dimension_scale(index, profile=PROFILE)


def test_profile_keyword_types_and_index_types_are_explicit():
    document = scan(fixture.make_dimension_contents())
    with pytest.raises(TypeError):
        document.dimension_scale(0)
    for value in (True, None, 3):
        with pytest.raises(TypeError):
            document.dimension_scale(0, profile=value)
    for value in (True, 1.5):
        with pytest.raises(TypeError):
            document.dimension_scale(value, profile=PROFILE)
    with pytest.raises(UnsupportedFeatureError):
        document.dimension_scale(0, profile="unknown")
    for marker in (5, 8):
        with pytest.raises(UnsupportedFeatureError):
            scan(fixture.make_dimension_contents(marker)).dimension_scale(
                0, profile=PROFILE
            )


def test_geometry_nonfinite_or_zero_span_and_equivalent_scale_overflow_are_rejected():
    for offset, value in (
        (START + 64 + 16, math.nan),
        (START + 64 + 32, 10.0),
        (0x178, 1e-308),
    ):
        data = bytearray(fixture.make_dimension_contents())
        struct.pack_into("<d", data, offset, value)
        with pytest.raises((InvalidFormatError, UnsupportedFeatureError)):
            scan(data).dimension_scale(0, profile=PROFILE)


@pytest.mark.parametrize(
    "dx,dy,number,expected_length,expected_ratio",
    [
        (3.0, 4.0, 10.0, 5.0, 2.0),
        (-3.0, 4.0, 10.0, 5.0, 2.0),
        (1e308, 1e308, 1e308, 1e308 * math.sqrt(2), 1 / math.sqrt(2)),
        (3e-308, 4e-308, 1e-307, 5e-308, 2.0),
        (
            3 * math.ulp(0.0),
            4 * math.ulp(0.0),
            10 * math.ulp(0.0),
            5 * math.ulp(0.0),
            2.0,
        ),
        (1e308, math.ulp(0.0), 1e308, 1e308, 1.0),
    ],
)
def test_scaled_norm_keeps_extreme_diagonals_and_recorded_ratios_finite(
    dx, dy, number, expected_length, expected_ratio
):
    data = bytearray(fixture.make_dimension_contents(number=number))
    struct.pack_into("<4d", data, START + 64 + 16, 0, 0, dx, dy)
    value = scan(data).dimension_scale(0, profile=PROFILE)
    assert math.isfinite(value.stored_span_length) and value.stored_span_length > 0
    assert value.stored_span_length / expected_length == pytest.approx(1, rel=1e-14)
    assert math.isfinite(value.annotation_to_drawing_ratio)
    assert value.annotation_to_drawing_ratio == pytest.approx(expected_ratio, rel=1e-14)
    assert math.isfinite(value.equivalent_annotation_denominator)
