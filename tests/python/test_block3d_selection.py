"""Project-authored synthetic parser contracts; native qualification has an independent gate."""

import hashlib
import math
import struct
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

from ezdracad import (
    InvalidFormatError,
    LimitExceededError,
    UnsupportedFeatureError,
    raw,
)
from ezdracad.block3d import Block3DProfile, BlockLineSelection3D
from ezdracad.options import ParseLimits

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
PROFILE = Block3DProfile.MARKER25_DIRECT_LINE3D_V1
PAYLOAD_AT = 1536


def fixture():
    data = bytearray((FIXTURES / "block3d25.cfb").read_bytes())
    payload = (FIXTURES / "block3d25.bin").read_bytes()
    assert data[PAYLOAD_AT : PAYLOAD_AT + len(payload)] == payload
    return data


def write(data, at, fmt, *values):
    struct.pack_into(fmt, data, PAYLOAD_AT + at, *values)


def selection(data=None, **kwargs):
    return raw.scan_records(
        bytes(fixture() if data is None else data)
    ).placed_block_lines_3d(0, profile=PROFILE, **kwargs)


def definition(data):
    first = struct.unpack_from("<I", data, PAYLOAD_AT + 76)[0]
    count = struct.unpack_from("<I", data, PAYLOAD_AT + first + 36)[0] + 1
    return first + 176 + count * 64


def test_owned_atomic_selection_preserves_every_source_and_explicit_scope(tmp_path):
    data = fixture()
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(data)
    scan = raw.scanfile(path)
    path.unlink()
    value = scan.placed_block_lines_3d(0, profile=PROFILE)
    assert isinstance(value, BlockLineSelection3D) and value.profile is PROFILE
    assert value.document_complete is False and value.units is None
    assert value.contents_sha256 == hashlib.sha256(scan.read_contents()).hexdigest()
    ref = value.reference
    assert ref.name_key == b"TWO" and ref.name_field_bytes == b"TWO\0" + b"\xa5" * 28
    assert ref.translation == (100, -50, 25) and ref.scale == (2, 3, 0.5)
    assert ref.rotation_degrees == (0, 0, 0) and ref.tag_raw == 6
    assert ref.record_scope == "root_records" and ref.units is None
    assert ref.evidence.contents_sha256 == value.contents_sha256
    assert ref.evidence.record_indices == (0, 1, 2)
    assert ref.evidence.payload_offsets == (8192, 8256, 8320)
    assert ref.evidence.records == tuple(scan.record_bytes(i) for i in range(3))
    source = value.definition
    assert source.index == 1 and source.name_bytes == b"TWO" and source.flag_raw == 1
    assert (
        source.header_bytes
        == scan.read_contents()[source.header_offset : source.header_offset + 176]
    )
    assert source.saved_bounds == (101.75, 48.125, 12.5, -12.5, -2.5, -3.75)
    assert len(value.lines) == 2
    line = value.lines[0]
    assert line.definition_index == 1 and line.source.record_index == 0
    assert line.source.payload_offset == source.record_start
    assert line.source.start == (1.25, -2.5, 12.5)
    assert line.source.end == (101.75, 48.125, -3.75)
    assert line.source.coordinate_space == "record_local"
    assert (
        line.source.header_bytes
        == scan.read_contents()[source.record_start : source.record_start + 16]
    )
    assert line.start == (102.5, -57.5, 31.25) and line.end == (303.5, 94.375, 23.125)
    assert line.coordinate_space == "block_parent_stored_xyz"
    assert line.units is None and line.parent_context is None
    del scan
    assert ref.apply_xyz([7, 11, 13]) == (114, -17, 31.5)
    with pytest.raises(FrozenInstanceError):
        value.lines = ()
    with pytest.raises(FrozenInstanceError):
        ref._handle = None


def test_explicit_profile_and_old_planar_api_are_unchanged():
    scan = raw.scan_records(bytes(fixture()))
    with pytest.raises(TypeError):
        scan.placed_block_lines_3d(0)
    with pytest.raises(TypeError):
        scan.placed_block_lines_3d(0, PROFILE)
    with pytest.raises(TypeError):
        scan.placed_block_lines_3d(0, profile=None)
    with pytest.raises(UnsupportedFeatureError):
        scan.placed_block_lines_3d(0, profile="auto")
    assert scan.placed_block_lines_3d(0, profile=PROFILE.value).profile is PROFILE
    assert len(scan.block_library().planar_lines(0)) == 1
    with pytest.raises(UnsupportedFeatureError):
        scan.block_library().placed_lines(0)


def test_dirty_tag_and_noncommuting_combined_rotation_order():
    data = fixture()
    write(data, 8192, "<H", 0x406)
    write(data, 8272, "<6d", -70, 35, -15, 2, 3, 43)
    write(data, 8336, "<3d", 17, -29, 0.5)
    value = selection(data)
    assert value.reference.tag_raw == 0x406
    assert value.reference.rotation_degrees == (17, -29, 43)
    assert value.lines[0].start == pytest.approx(
        (-63.6048648050769, 28.6581477626273, -10.47831270011), abs=1e-10
    )
    assert value.lines[0].end == pytest.approx(
        (-48.6958607713266, 244.398111701582, 119.009218778176), abs=1e-10
    )


@pytest.mark.parametrize(
    "axis,point,expected",
    [
        (0, (1, 1, 0), (102, -50, 28)),
        (1, (1, 0, 0), (100, -50, 23)),
        (2, (1, 0, 0), (100, -48, 25)),
    ],
)
def test_right_handed_quarter_turns_have_no_spurious_translation_loss(
    axis, point, expected
):
    data = fixture()
    write(data, [8336, 8344, 8312][axis], "<d", 90)
    assert selection(data).reference.apply_xyz(point) == expected


@pytest.mark.parametrize(
    "offset,fmt,value",
    [
        (8192, "<H", 0x806),
        (8200, "<H", 2),
        (8200, "<H", 4),
        (8256, "<H", 0xFD),
        (8320, "<H", 0xFD),
        (8208, "<B", 1),
        (8223, "<B", 1),
        (8258, "<B", 1),
        (8360, "<B", 1),
    ],
)
def test_unknown_reference_shapes_fail_without_masking(offset, fmt, value):
    data = fixture()
    write(data, offset, fmt, value)
    scan = raw.scan_records(bytes(data))
    with pytest.raises((UnsupportedFeatureError, InvalidFormatError)):
        scan.placed_block_lines_3d(0, profile=PROFILE)
    assert len(scan.record_bytes(0)) == 64


@pytest.mark.parametrize("index", [1, 2, 3, 4, 2**64 - 1])
def test_only_primary_root_reference_can_be_selected(index):
    with pytest.raises((UnsupportedFeatureError, InvalidFormatError)):
        raw.scan_records(bytes(fixture())).placed_block_lines_3d(index, profile=PROFILE)


@pytest.mark.parametrize("offset", [32, 40, 44, 111, 160, 175])
def test_unknown_definition_fields_fail_closed(offset):
    data = fixture()
    data[PAYLOAD_AT + definition(data) + offset] ^= 1
    with pytest.raises(UnsupportedFeatureError):
        selection(data)


@pytest.mark.parametrize("tag", [2, 6, 0x30, 0xFE, 0x413, 0x7F])
def test_late_unsupported_child_never_returns_partial_lines(tag):
    data = fixture()
    write(data, definition(data) + 176 + 64, "<H", tag)
    with pytest.raises(UnsupportedFeatureError):
        selection(data)


def test_metadata_span_and_continuation_guards():
    data = fixture()
    write(data, definition(data) + 176 + 64, "<H", 0x33)
    assert len(selection(data).lines) == 1
    data = fixture()
    write(data, definition(data) + 176, "<H", 0x33)
    write(data, definition(data) + 184, "<H", 2)
    with pytest.raises(UnsupportedFeatureError):
        selection(data)


@pytest.mark.parametrize("offset", [12, 13, 14, 15])
def test_line_source_tail_is_independently_guarded_from_reference(offset):
    data = fixture()
    write(data, definition(data) + 176 + 64 + offset, "<B", 1)
    with pytest.raises(UnsupportedFeatureError):
        selection(data)


def test_bounded_name_and_unique_resolution_guards():
    for name in [b"MISS\0", b"\0", b"x" * 32]:
        data = fixture()
        data[PAYLOAD_AT + 8224 : PAYLOAD_AT + 8224 + len(name)] = name
        with pytest.raises((UnsupportedFeatureError, InvalidFormatError)):
            selection(data)
    data = fixture()
    first = struct.unpack_from("<I", data, PAYLOAD_AT + 76)[0]
    data[PAYLOAD_AT + first : PAYLOAD_AT + first + 4] = b"TWO\0"
    with pytest.raises(UnsupportedFeatureError):
        selection(data)


@pytest.mark.parametrize("marker", [5, 8])
def test_other_observed_markers_are_not_promoted(marker):
    data = fixture()
    write(data, 34, "<H", marker)
    with pytest.raises(UnsupportedFeatureError):
        selection(data)


@pytest.mark.parametrize("limit", [0, 1])
def test_line_limit_is_atomic_and_has_its_own_resource(limit):
    with pytest.raises(LimitExceededError) as error:
        selection(max_lines=limit)
    assert error.value.resource == "block_lines_3d"
    assert error.value.limit == limit and error.value.actual == limit + 1
    scan = raw.scan_records(bytes(fixture()), limits=ParseLimits(max_records=6))
    with pytest.raises(LimitExceededError) as error:
        scan.placed_block_lines_3d(0, profile=PROFILE)
    assert error.value.resource == "records"


@pytest.mark.parametrize("argument", [True, -1, 1.5, "0", 2**64])
def test_index_and_budget_types_remain_strict(argument):
    scan = raw.scan_records(bytes(fixture()))
    with pytest.raises((TypeError, ValueError, OverflowError)):
        scan.placed_block_lines_3d(argument, profile=PROFILE)
    with pytest.raises((TypeError, ValueError, OverflowError)):
        scan.placed_block_lines_3d(0, profile=PROFILE, max_lines=argument)


@pytest.mark.parametrize(
    "offset", [8272, 8280, 8288, 8296, 8304, 8312, 8336, 8344, 8352]
)
def test_nonfinite_native_fields_remain_raw_inspectable(offset):
    data = fixture()
    write(data, offset, "<d", math.nan)
    scan = raw.scan_records(bytes(data))
    with pytest.raises(InvalidFormatError):
        scan.placed_block_lines_3d(0, profile=PROFILE)
    assert math.isnan(struct.unpack_from("<d", scan.read_contents(), offset)[0])


@pytest.mark.parametrize(
    "point",
    [
        (math.nan, 0, 0),
        (0, math.inf, 0),
        (1e308, 0, 0),
        (0, 0, math.ulp(0.0)),
        (math.ulp(0.0), 0, 0),
    ],
)
def test_numerical_range_failures_raise(point):
    with pytest.raises(InvalidFormatError):
        selection().reference.apply_xyz(point)


def test_nonzero_angle_that_cannot_be_represented_in_radians_is_rejected():
    data = fixture()
    write(data, 8336, "<d", math.ulp(0.0))
    with pytest.raises(InvalidFormatError):
        selection(data)


@pytest.mark.parametrize(
    "point", [None, (1, 2), [1, 2, 3, 4], "123", [True, 0, 0], ["1", 2, 3]]
)
def test_public_and_private_point_boundaries_reject_bad_shapes(point):
    reference = selection().reference
    with pytest.raises(TypeError):
        reference.apply_xyz(point)
    with pytest.raises(TypeError):
        reference._handle.apply_xyz(point)
