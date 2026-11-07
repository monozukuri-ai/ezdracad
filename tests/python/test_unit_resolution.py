"""General unknown states must remain distinct from declared historical scales."""

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
from ezdracad.coordinate_measurement import CoordinateMeasurementContext
from ezdracad.origin_units import OriginUnitsCode
from ezdracad.unit_resolution import (
    CreationScaleProvenance,
    CreationScaleResolution,
    DeclaredCreationScale,
    PhysicalUnitResolution,
    PhysicalUnitStatus,
    PhysicalUnitUnresolvedReason,
    UnitResolution,
)

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "unit_resolution_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def contents(marker=25, code=0, factor=0.01, ratio=0.5):
    payload = bytearray(assembler.make_record_contents(marker))
    struct.pack_into("<i", payload, 0x150, code)
    struct.pack_into("<d", payload, 0x178, factor)
    struct.pack_into("<d", payload, 0x278, ratio)
    return payload


@pytest.mark.parametrize("code", [0, 1, 2, 3, -1, -(2**31), 2**31 - 1])
def test_owned_resolution_keeps_unresolved_states_source_bytes_and_geometry(
    code, tmp_path
):
    payload = contents(code=code)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "unit-resolution.mpz"
    path.write_bytes(envelope)
    scan = raw.scanfile(path)
    before = scan.line_records()
    path.unlink()
    value = scan.unit_resolution()
    assert isinstance(value, UnitResolution)
    assert value.physical_units == PhysicalUnitResolution(
        PhysicalUnitStatus.UNRESOLVED,
        PhysicalUnitUnresolvedReason.STORED_COORDINATE_UNIT_NOT_ESTABLISHED,
    )
    assert (
        value.original_creation_scale
        is CreationScaleResolution.UNAVAILABLE_FROM_DECODED_FIELDS
    )
    assert value.origin_units == OriginUnitsCode(0x150, struct.pack("<i", code), code)
    assert value.coordinate_input_unit.origin_units == value.origin_units
    assert (
        value.coordinate_input_unit._handle.metadata()
        == scan.coordinate_input_unit()._handle.metadata()
    )
    current = value.current_measurement
    assert isinstance(current, CoordinateMeasurementContext)
    assert current.paper_scale_offset == 0x178
    assert current.paper_scale_bytes == struct.pack("<d", 0.01)
    assert current.annotation_ratio_offset == 0x278
    assert current.annotation_ratio_bytes == struct.pack("<d", 0.5)
    assert (current.drawing_denominator, current.annotation_denominator) == (100, 200)
    assert current.length_on_paper_mm(1000) == 10
    assert current.length_at_current_annotation_mm(1000) == 2000
    native = _core.scan_bytes(envelope, ParseLimits()._values()).unit_resolution()
    assert native.metadata() == value._handle.metadata()
    assert native.current_measurement().metadata() == current._handle.metadata()
    assert scan.read_contents() == payload
    assert scan.line_records() == before
    assert all(line.units is None for line in before)
    with pytest.raises(FrozenInstanceError):
        value.original_creation_scale = None
    with pytest.raises(FrozenInstanceError):
        value.physical_units.reason = None


@pytest.mark.parametrize("marker", [5, 8])
def test_matching_legacy_header_fields_do_not_qualify_resolution(marker):
    scan = raw.scan_records(assembler.wrap_record_contents(contents(marker=marker)))
    with pytest.raises(UnsupportedFeatureError, match="marker 25"):
        scan.unit_resolution()
    with pytest.raises(UnsupportedFeatureError, match="marker 25"):
        scan.origin_units_code()
    assert scan.coordinate_measurement_context().drawing_denominator == 100


def test_invalid_current_context_does_not_invalidate_other_raw_readers():
    scan = raw.scan_records(assembler.wrap_record_contents(contents(factor=0)))
    with pytest.raises(InvalidFormatError, match="coordinate measurement"):
        scan.unit_resolution()
    assert scan.origin_units_code().raw_code == 0
    assert len(scan.line_records()) == 2


def test_declared_original_drawing_scale_is_required_after_current_drawing_scale_changes():
    scan = raw.scan_records(assembler.wrap_record_contents(contents(ratio=1)))
    resolution = scan.unit_resolution()
    assert resolution.current_measurement.drawing_denominator == 100
    assert resolution.current_measurement.length_at_current_annotation_mm(1000) == 1000
    declared = DeclaredCreationScale(25, 100)
    assert declared.original_drawing_denominator == 25
    assert declared.original_annotation_denominator == 100
    assert declared.original_annotation_ratio == 0.25
    assert declared.provenance is CreationScaleProvenance.CALLER_DECLARED_UNVERIFIED
    assert declared.length_mm(1000) == 4000
    assert DeclaredCreationScale(100, 100).length_mm(1000) == 1000
    assert (
        scan.unit_resolution().original_creation_scale
        is CreationScaleResolution.UNAVAILABLE_FROM_DECODED_FIELDS
    )
    assert scan.unit_resolution().physical_units == resolution.physical_units
    with pytest.raises(TypeError):
        DeclaredCreationScale(100)
    with pytest.raises(FrozenInstanceError):
        declared.original_drawing_denominator = 100


@pytest.mark.parametrize("bad", [0, -0.0, -1, math.nan, math.inf, -math.inf])
def test_declared_denominators_are_positive_finite(bad):
    with pytest.raises(InvalidFormatError, match="finite and positive"):
        DeclaredCreationScale(bad, 100)
    with pytest.raises(InvalidFormatError, match="finite and positive"):
        DeclaredCreationScale(100, bad)


@pytest.mark.parametrize("drawing,annotation", [(1e308, 1e-308), (1e-308, 1e308)])
def test_declared_unrepresentable_ratio_is_rejected(drawing, annotation):
    with pytest.raises(InvalidFormatError, match="D/A ratio"):
        DeclaredCreationScale(drawing, annotation)


@pytest.mark.parametrize("bad", [True, False, "100", None, [], object()])
def test_python_and_direct_native_constructor_reject_non_numeric_and_bool(bad):
    for constructor in [DeclaredCreationScale, _core._DeclaredCreationScaleHandle]:
        with pytest.raises(TypeError):
            constructor(bad, 100)
        with pytest.raises(TypeError):
            constructor(100, bad)


def test_declared_conversion_boundary_results_and_direct_ffi_validation():
    balanced = DeclaredCreationScale(1e308, 1e308)
    assert balanced.length_mm(1e308) == 1e308
    smallest = float.fromhex("0x0.0000000000001p-1022")
    assert balanced.length_mm(smallest) == smallest
    assert math.copysign(1, balanced.length_mm(-0.0)) == -1
    assert balanced.length_mm(0) == 0
    for bad in [-1, math.nan, math.inf, -math.inf]:
        with pytest.raises(InvalidFormatError, match="finite and nonnegative"):
            balanced.length_mm(bad)
    for bad in [True, "1000", None]:
        with pytest.raises(TypeError):
            balanced.length_mm(bad)
        with pytest.raises(TypeError):
            balanced._handle.length_mm(bad)
    with pytest.raises(InvalidFormatError, match="overflow or nonzero underflow"):
        DeclaredCreationScale(100, 200).length_mm(sys.float_info.max)
    with pytest.raises(InvalidFormatError, match="overflow or nonzero underflow"):
        DeclaredCreationScale(200, 100).length_mm(smallest)
