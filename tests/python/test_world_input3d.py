"""Synthetic loaded-file3D context contracts, separate from native controls."""

import math
import struct
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

from ezdracad import InvalidFormatError, UnsupportedFeatureError, _core, raw
from ezdracad.options import ParseLimits
from ezdracad.world_input3d import WorldInput3DContext, WorldInput3DProfile

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
PROFILE = WorldInput3DProfile.MARKER25_LOADED_WORLD_INPUT_V1


def fixture(marker=25, *, code=0, ratio=0.5):
    data = bytearray((FIXTURES / f"records{marker}.cfb").read_bytes())
    for offset, value in (
        (0x140, 100.0),
        (0x148, -50.0),
        (0x178, 0.01),
        (0x1B0, 0.0),
        (0x278, ratio),
    ):
        struct.pack_into("<d", data, 1536 + offset, value)
    for offset, value in ((0x150, code), (0x154, 0), (0x1AC, 0), (0x274, 0)):
        struct.pack_into("<i", data, 1536 + offset, value)
    return data


def context(data=None):
    return raw.scan_records(
        bytes(fixture() if data is None else data)
    ).world_input_3d_context(profile=PROFILE)


def test_explicit_profile_owned_sources_and_zero_z_convention(tmp_path):
    data = fixture()
    struct.pack_into("<2d", data, 1536 + 0x158, 9876.25, -43.5)
    path = tmp_path / "loaded-world.mpz"
    path.write_bytes(data)
    scan = raw.scanfile(path)
    path.unlink()
    with pytest.raises(TypeError):
        scan.world_input_3d_context()
    with pytest.raises(TypeError):
        scan.world_input_3d_context(PROFILE)
    value = scan.world_input_3d_context(profile=PROFILE)
    assert isinstance(value, WorldInput3DContext) and value.profile is PROFILE
    assert value.loaded_world_origin == (100, -50, 0)
    assert value.origin_z_source == "loaded_file_zero_z_convention"
    assert value.draw_origin_offset == 0x154
    assert value.draw_origin_bytes == bytes(4) and value.draw_origin_raw == 0
    assert value.annotation_ratio == 0.5
    assert value.drawing_denominator == 100 and value.annotation_denominator == 200
    assert (
        value.units == "mm"
        and value.coordinate_semantics == "loaded_world_absolute_input_mm"
    )
    assert value.evidence == value.coordinate_context.evidence
    assert value.evidence.header_bytes == scan.read_contents()[0x140:0x280]
    with pytest.raises(FrozenInstanceError):
        value._handle = None
    assert value.input_mm_from_world_stored_xyz((110, -45, 2.5)) == (20, 10, 5)
    assert value.world_stored_xyz_from_input_mm((20, 10, 5)) == (110, -45, 2.5)
    assert (
        scan.world_input_3d_context(profile=PROFILE.value).loaded_world_origin
        == value.loaded_world_origin
    )


@pytest.mark.parametrize(
    "ratio,stored", [(0.5, (110, -45, 2.5)), (0.25, (105, -47.5, 1.25))]
)
def test_observed_ratio_maps_all_three_components(ratio, stored):
    value = context(fixture(ratio=ratio))
    assert value.input_mm_from_world_stored_xyz(stored) == (20, 10, 5)
    assert value.world_stored_xyz_from_input_mm((20, 10, 5)) == stored


def test_native_input_meters_are_separate_from_contextual_mm_components():
    value = context(fixture(code=2))
    assert value.input_unit.unit.value == "m"
    mm = tuple(value.input_unit.to_native_mm(v) for v in (20, 10, 5))
    assert mm == (20000, 10000, 5000)
    stored = value.world_stored_xyz_from_input_mm(mm)
    assert stored == (10100, 4950, 2500)
    recovered = value.input_mm_from_world_stored_xyz(stored)
    assert tuple(value.input_unit.from_native_mm(v) for v in recovered) == (20, 10, 5)


@pytest.mark.parametrize("grid_x", [0, 75, -1234.5, float("nan")])
def test_grid_origin_does_not_replace_loaded_world_origin_z(grid_x):
    data = fixture()
    struct.pack_into("<d", data, 1536 + 0x158, grid_x)
    value = context(data)
    assert value.loaded_world_origin[2] == 0
    assert value.input_mm_from_world_stored_xyz((110, -45, 2.5)) == (20, 10, 5)
    assert value.evidence.header_bytes == data[1536 + 0x140 : 1536 + 0x280]


@pytest.mark.parametrize("marker", [5, 8])
def test_unobserved_file_profiles_remain_raw_inspectable(marker):
    scan = raw.scan_records(bytes(fixture(marker)))
    with pytest.raises(UnsupportedFeatureError):
        scan.world_input_3d_context(profile=PROFILE)
    assert len(scan.line_records()) == 2


@pytest.mark.parametrize(
    "offset,value",
    [
        (0x150, 1),
        (0x150, 3),
        (0x150, 4),
        (0x150, 5),
        (0x150, 6),
        (0x150, -1),
        (0x154, 1),
        (0x154, -1),
        (0x1AC, 1),
        (0x274, 1),
    ],
)
def test_unobserved_selectors_and_active_draft_are_rejected(offset, value):
    data = fixture()
    struct.pack_into("<i", data, 1536 + offset, value)
    scan = raw.scan_records(bytes(data))
    with pytest.raises(UnsupportedFeatureError):
        scan.world_input_3d_context(profile=PROFILE)
    assert len(scan.line_records()) == 2


def test_retained_disabled_draft_angle_is_also_outside_this_profile():
    data = fixture()
    struct.pack_into("<d", data, 1536 + 0x1B0, 30)
    with pytest.raises(UnsupportedFeatureError):
        context(data)


@pytest.mark.parametrize(
    "offset,value",
    [
        (0x140, float("nan")),
        (0x148, float("inf")),
        (0x178, 0),
        (0x278, -1),
        (0x278, float("nan")),
    ],
)
def test_invalid_current_context_does_not_prevent_raw_scanning(offset, value):
    data = fixture()
    struct.pack_into("<d", data, 1536 + offset, value)
    scan = raw.scan_records(bytes(data))
    with pytest.raises(InvalidFormatError):
        scan.world_input_3d_context(profile=PROFILE)
    assert len(scan.records()) == 5


@pytest.mark.parametrize(
    "point",
    [True, 1, "1,2,3", (1, 2), (1, 2, 3, 4), (True, 2, 3), (1, "2", 3), (1, None, 3)],
)
def test_point_shape_and_scalar_types_are_checked_on_python_and_ffi_boundaries(point):
    value = context()
    for method in (
        value.input_mm_from_world_stored_xyz,
        value.world_stored_xyz_from_input_mm,
    ):
        with pytest.raises(TypeError):
            method(point)
    handle = _core.scan_bytes(
        bytes(fixture()), ParseLimits()._values()
    ).world_input_3d_context(PROFILE.value)
    for method in (
        handle.input_mm_from_world_stored_xyz,
        handle.world_stored_xyz_from_input_mm,
    ):
        with pytest.raises(TypeError):
            method(point)


@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
@pytest.mark.parametrize("axis", [0, 1, 2])
def test_nonfinite_xyz_rejects_the_whole_conversion(value, axis):
    plane = context()
    point = [0, 0, 0]
    point[axis] = value
    with pytest.raises(InvalidFormatError):
        plane.input_mm_from_world_stored_xyz(point)
    with pytest.raises(InvalidFormatError):
        plane.world_stored_xyz_from_input_mm(point)


def test_signed_zero_z_overflow_underflow_and_xy_translation_guards():
    value = context()
    assert (
        math.copysign(1, value.input_mm_from_world_stored_xyz((100, -50, -0.0))[2])
        == -1
    )
    assert math.copysign(1, value.world_stored_xyz_from_input_mm((0, 0, -0.0))[2]) == -1
    with pytest.raises(InvalidFormatError):
        value.input_mm_from_world_stored_xyz(
            (100, -50, float.fromhex("0x1.fffffffffffffp+1023"))
        )
    with pytest.raises(InvalidFormatError):
        value.world_stored_xyz_from_input_mm(
            (0, 0, float.fromhex("0x0.0000000000001p-1022"))
        )
    with pytest.raises(InvalidFormatError):
        value.world_stored_xyz_from_input_mm((1e-300, 0, 0))


def test_profile_type_and_unknown_id_are_explicit():
    scan = raw.scan_records(bytes(fixture()))
    with pytest.raises(TypeError):
        scan.world_input_3d_context(profile=True)
    with pytest.raises(UnsupportedFeatureError):
        scan.world_input_3d_context(profile="automatic")
