"""Native input coefficients are separate from stored physical-unit claims."""

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
    UnsupportedFeatureError,
    _core,
    raw,
)
from ezdracad.coordinate_input_units import (
    NativeCoordinateInputUnit,
    NativeCoordinateInputUnitInfo,
)

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "coordinate_input_unit_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def contents(marker=25, code=0):
    result = bytearray(assembler.make_record_contents(marker))
    struct.pack_into("<i", result, 0x150, code)
    return result


@pytest.mark.parametrize(
    "code,label,factor",
    [
        (0, "mm", 1.0),
        (1, "cm", 0.1),
        (2, "m", 0.001),
        (3, "km", 0.000001),
        (4, "inch", 0.03937),
        (5, "feet", 0.003281),
        (6, "yard", 0.0010936),
    ],
)
def test_native_factors_source_snapshot_and_signed_scalar_conversions(
    code, label, factor, tmp_path
):
    payload = contents(code=code)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "input-unit.mpz"
    path.write_bytes(envelope)
    scan = raw.scanfile(path)
    before = scan.line_records()
    path.unlink()
    info = scan.coordinate_input_unit()
    assert isinstance(info, NativeCoordinateInputUnitInfo)
    assert info.unit is NativeCoordinateInputUnit(label)
    assert info.native_display_factor == factor
    assert info.origin_units == scan.origin_units_code()
    assert info.origin_units.selector_bytes == struct.pack("<i", code)
    for value in [-60, -20, 0, 20, 60]:
        assert info.to_native_mm(value) == value / factor
        assert info.from_native_mm(value) == value * factor
    native = _core.scan_bytes(envelope, ParseLimits()._values()).coordinate_input_unit()
    assert native.metadata() == info._handle.metadata()
    assert scan.read_contents() == payload and scan.line_records() == before
    assert all(line.units is None for line in before)
    with pytest.raises(FrozenInstanceError):
        info.native_display_factor = 1.0


@pytest.mark.parametrize("code", [-1, -(2**31), 2**31 - 1, 7, 0x01020304])
def test_unknown_signed_codes_are_readable_but_not_convertible(code):
    scan = raw.scan_records(assembler.wrap_record_contents(contents(code=code)))
    info = scan.coordinate_input_unit()
    assert info.unit is NativeCoordinateInputUnit.UNKNOWN
    assert info.native_display_factor is None
    assert info.origin_units.raw_code == code
    assert info.origin_units.selector_bytes == struct.pack("<i", code)
    for method in [info.to_native_mm, info.from_native_mm]:
        with pytest.raises(UnsupportedFeatureError, match="input unit code"):
            method(-1)


@pytest.mark.parametrize("marker", [5, 8])
def test_standalone_input_unit_reader_keeps_legacy_source_guard(marker):
    scan = raw.scan_records(assembler.wrap_record_contents(contents(marker=marker)))
    with pytest.raises(UnsupportedFeatureError, match="marker 25"):
        scan.coordinate_input_unit()


def test_native_imperial_coefficient_is_not_replaced_by_exact_si_and_zero_sign_survives():
    info = raw.scan_records(
        assembler.wrap_record_contents(contents(code=4))
    ).coordinate_input_unit()
    assert info.to_native_mm(1) == 1 / 0.03937
    assert info.to_native_mm(1) != 25.4
    assert 100 + 0.5 * info.to_native_mm(-20) == -154.000508001016
    assert math.copysign(1, info.to_native_mm(-0.0)) == -1
    assert math.copysign(1, info.from_native_mm(-0.0)) == -1


@pytest.mark.parametrize("bad", [True, False, "1", None, [], object()])
def test_wrapper_and_direct_ffi_reject_non_numeric_and_bool(bad):
    info = raw.scan_records(
        assembler.wrap_record_contents(contents())
    ).coordinate_input_unit()
    for target in [info, info._handle]:
        for method in [target.to_native_mm, target.from_native_mm]:
            with pytest.raises(TypeError):
                method(bad)


def test_nonfinite_overflow_and_nonzero_underflow_are_not_silently_returned():
    info = raw.scan_records(
        assembler.wrap_record_contents(contents(code=1))
    ).coordinate_input_unit()
    for value in [math.nan, math.inf, -math.inf]:
        for method in [info.to_native_mm, info.from_native_mm]:
            with pytest.raises(InvalidFormatError, match="must be finite"):
                method(value)
    for value in [sys.float_info.max, -sys.float_info.max]:
        with pytest.raises(InvalidFormatError, match="overflow"):
            info.to_native_mm(value)
    smallest = float.fromhex("0x0.0000000000001p-1022")
    for value in [smallest, -smallest]:
        with pytest.raises(InvalidFormatError, match="underflow"):
            info.from_native_mm(value)
    mm = raw.scan_records(
        assembler.wrap_record_contents(contents())
    ).coordinate_input_unit()
    assert mm.to_native_mm(sys.float_info.max) == sys.float_info.max
    assert mm.from_native_mm(smallest) == smallest
