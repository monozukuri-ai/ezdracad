"""Synthetic API contracts, independent from native workplane captures."""

import math
import struct
from dataclasses import FrozenInstanceError

import pytest
from test_coordinate_space import assembler, context_data, scan

from ezdracad import InvalidFormatError, UnsupportedFeatureError, raw
from ezdracad.workplane import (
    WorkplaneBasisSource,
    WorkplaneContext,
    WorkplaneDirection,
    WorkplaneProfile,
)

PROFILE = WorkplaneProfile.MARKER25_SAVED_WORKPLANE_V1
IDENTITY = ((1, 0, 0), (0, 1, 0), (0, 0, 1))
TILTED = ((1, 0, 0), (0, 0.8, -0.6), (0, 0.6, 0.8))


def plane_data(direction=1, axes=IDENTITY):
    data = context_data()
    struct.pack_into("<3d", data, 0x450, 100, -50, 25)
    data[0x468:0x480] = b"\xa5" * 24
    struct.pack_into("<ii", data, 0x480, direction, 0x12345678)
    struct.pack_into("<9d", data, 0x488, *(x for axis in axes for x in axis))
    return data


def context(direction=1, axes=IDENTITY):
    return scan(plane_data(direction, axes)).workplane_context(profile=PROFILE)


def test_owned_context_keeps_exact_plane_and_guard_headers_after_file_removal(tmp_path):
    payload = plane_data()
    path = tmp_path / "plane.mpz"
    path.write_bytes(assembler.wrap_record_contents(payload))
    document = raw.scanfile(path)
    path.unlink()
    with pytest.raises(TypeError):
        document.workplane_context()
    with pytest.raises(TypeError):
        document.workplane_context(PROFILE)
    value = document.workplane_context(profile=PROFILE)
    assert isinstance(value, WorkplaneContext)
    assert value.profile is PROFILE
    assert value.direction is WorkplaneDirection.PLAN
    assert value.direction_raw == 1
    assert value.origin == (100, -50, 25)
    assert value.origin_offset == 0x450
    assert value.origin_bytes == payload[0x450:0x468]
    assert value.direction_offset == 0x480
    assert value.direction_bytes == payload[0x480:0x484]
    assert value.stored_axes_offset == 0x488
    assert value.stored_axes_bytes == payload[0x488:0x4D0]
    assert value.evidence.contents_sha256 == document.sha256
    assert value.evidence.header_offset == 0x450
    assert value.evidence.header_bytes == payload[0x450:0x4D0]
    assert value.evidence.coordinate_guard.header_offset == 0x140
    assert value.evidence.coordinate_guard.header_bytes == payload[0x140:0x280]
    assert value.units is None
    assert value.to_world_stored_xyz([7, 11, 13]) == (107, -39, 38)
    with pytest.raises(FrozenInstanceError):
        value.evidence.header_bytes = b""
    with pytest.raises(FrozenInstanceError):
        value._handle = None


@pytest.mark.parametrize(
    "direction,label,expected",
    [
        (1, "plan", (107, -39, 38)),
        (2, "east", (113, -43, 36)),
        (3, "south", (107, -63, 36)),
        (4, "west", (87, -57, 36)),
        (5, "north", (93, -37, 36)),
    ],
)
def test_presets_derive_active_axes_and_do_not_apply_retained_free_matrix(
    direction, label, expected
):
    value = context(direction, TILTED)
    assert value.direction.value == label
    assert value.stored_axes == TILTED
    assert value.basis_source is WorkplaneBasisSource.DERIVED_PRESET
    assert value.to_world_stored_xyz((7, 11, 13)) == expected
    assert value.from_world_stored_xyz(expected) == (7, 11, 13)


def test_free_affine_map_and_inverse_keep_units_and_source_unchanged():
    document = scan(plane_data(0, TILTED))
    before = document.line_records()
    value = document.workplane_context(profile=PROFILE.value)
    assert value.direction is WorkplaneDirection.FREE
    assert value.basis_source is WorkplaneBasisSource.STORED_AXES_FOR_FREE
    assert value.effective_axes == TILTED
    assert value.to_world_stored_xyz((7, 11, 13)) == pytest.approx((107, -33.4, 28.8))
    for point in [(7, 11, 13), (-12.5, 0.25, -7.75), (0, 0, 0)]:
        assert value.from_world_stored_xyz(
            value.to_world_stored_xyz(point)
        ) == pytest.approx(point)
    assert document.line_records() == before
    assert all(line.units is None for line in before)


@pytest.mark.parametrize("profile", ["wcs", "marker5_saved_workplane_v1", ""])
def test_unknown_profile_has_no_fallback(profile):
    with pytest.raises(UnsupportedFeatureError):
        scan(plane_data()).workplane_context(profile=profile)


@pytest.mark.parametrize("profile", [None, 1, True, object()])
def test_profile_requires_explicit_enum_or_string(profile):
    with pytest.raises(TypeError):
        scan(plane_data()).workplane_context(profile=profile)


@pytest.mark.parametrize(
    "point", [(), (1, 2), (1, 2, 3, 4), "1,2,3", (True, 1, 2), (1, "2", 3)]
)
def test_point_shape_and_numeric_type_are_checked_in_python_and_ffi(point):
    value = context()
    for method in [
        value.from_world_stored_xyz,
        value.to_world_stored_xyz,
        value._handle.from_world_stored_xyz,
        value._handle.to_world_stored_xyz,
    ]:
        with pytest.raises(TypeError):
            method(point)


@pytest.mark.parametrize(
    "point", [(math.nan, 0, 0), (0, math.inf, 0), (0, 0, -math.inf)]
)
def test_nonfinite_points_fail_in_rust(point):
    value = context()
    for method in [value.from_world_stored_xyz, value.to_world_stored_xyz]:
        with pytest.raises(InvalidFormatError):
            method(point)


@pytest.mark.parametrize("direction", [-1, 6, 2**31 - 1])
def test_unknown_direction_retains_raw_bytes_but_rejects_context(direction):
    data = plane_data(direction)
    document = scan(data)
    with pytest.raises(UnsupportedFeatureError):
        document.workplane_context(profile=PROFILE)
    assert document.read_contents() == data


@pytest.mark.parametrize("marker", [5, 8])
def test_legacy_not_promoted_by_matching_header(marker):
    data = bytearray(assembler.make_semantics_contents(marker))
    data[0x140:0x280] = plane_data()[0x140:0x280]
    data[0x450:0x4D0] = plane_data()[0x450:0x4D0]
    with pytest.raises(UnsupportedFeatureError):
        scan(data).workplane_context(profile=PROFILE)


@pytest.mark.parametrize(
    "offset,packed",
    [
        (0x150, struct.pack("<i", 2)),
        (0x274, struct.pack("<i", 1)),
        (0x1AC, struct.pack("<i", 1)),
        (0x1B0, struct.pack("<d", 30)),
    ],
)
def test_conservative_input_and_draft_guards_do_not_rewrite_the_source(offset, packed):
    data = plane_data()
    data[offset : offset + len(packed)] = packed
    document = scan(data)
    with pytest.raises(UnsupportedFeatureError):
        document.workplane_context(profile=PROFILE)
    assert document.read_contents() == data


@pytest.mark.parametrize("offset", [0x450, 0x488, 0x178, 0x278])
def test_nonfinite_source_fields_fail_only_the_requested_context(offset):
    data = plane_data()
    struct.pack_into("<d", data, offset, math.nan)
    document = scan(data)
    with pytest.raises(InvalidFormatError):
        document.workplane_context(profile=PROFILE)
    assert document.read_contents() == data


@pytest.mark.parametrize(
    "axes",
    [
        ((0, 0, 0),) * 3,
        ((2, 0, 0), (0, 1, 0), (0, 0, 1)),
        ((1, 0, 0), (1, 1, 0), (0, 0, 1)),
        ((1, 0, 0), (0, 1, 0), (0, 0, -1)),
    ],
)
def test_degenerate_skew_or_left_handed_free_axes_are_not_supported(axes):
    with pytest.raises(UnsupportedFeatureError):
        context(0, axes)


def test_translation_overflow_and_lost_displacement_are_rejected():
    data = plane_data()
    struct.pack_into("<d", data, 0x450, 1e308)
    value = scan(data).workplane_context(profile=PROFILE)
    for point in [(1e308, 0, 0), (1, 0, 0)]:
        with pytest.raises(InvalidFormatError):
            value.to_world_stored_xyz(point)
    with pytest.raises(InvalidFormatError):
        value.from_world_stored_xyz((-1e308, 0, 0))
