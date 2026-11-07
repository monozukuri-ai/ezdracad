"""Synthetic package-slot payload tests; no drawing-context qualification."""

import math
import struct
from dataclasses import FrozenInstanceError, asdict, replace
from pathlib import Path

import pytest

import ezdracad
from ezdracad import _core, raw

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
CONTENTS_FILE_OFFSET = 1536


def write_reference(
    data, index=0, *, name=b"native-~KKSrpX", fields=(100, -50, 0, 2, 2, 90)
):
    offset = CONTENTS_FILE_OFFSET + 8192 + index * 64
    data[offset : offset + 64] = b"\xa5" * 64
    struct.pack_into("<H", data, offset, 6)
    struct.pack_into("<H", data, offset + 8, 2)
    struct.pack_into("<2d", data, offset + 16, 0, 0)
    data[offset + 32 : offset + 32 + len(name)] = name
    data[offset + 32 + len(name)] = 0
    if index + 1 < 4:
        offset += 64
        data[offset : offset + 64] = b"\x11" * 64
        struct.pack_into("<H", data, offset, 254)
        struct.pack_into("<6d", data, offset + 16, *fields)


def sample():
    data = bytearray((FIXTURES / "records25.cfb").read_bytes())
    write_reference(data)
    return data


def scan(data):
    return raw.scan_records(bytes(data))


def test_reference_snapshot_headers_alias_bytes_and_continuation_outside_page(tmp_path):
    data = sample()
    path = tmp_path / "references.mpz"
    path.write_bytes(data)
    source = raw.scanfile(path)
    path.write_bytes(b"changed")
    reference = source.package_references(0, 1)[0]
    assert isinstance(reference, raw.PackageReference2D)
    assert reference.record_index == 0 and reference.payload_offset == 8192
    assert reference.continuation_index == 1 and reference.continuation_offset == 8256
    assert reference.tag_raw == 6
    assert reference.header_bytes == source.record_bytes(0)[:16]
    assert reference.name_field_bytes == source.record_bytes(0)[32:]
    assert reference.name_key == b"native-~KKSrpX"
    assert reference.name_field_bytes[14] == 0
    assert reference.name_field_bytes[15:] == b"\xa5" * 17
    assert reference.continuation_header_bytes == source.record_bytes(1)[:16]
    assert reference.anchor_raw == (0, 0)
    assert reference.placement_raw == (100, -50, 0, 2, 2, 90)
    assert reference.coordinate_space == "package_parent"
    assert reference.units is None and reference.parent_context is None
    assert source.package_references(1, 1) == ()
    assert source.package_references(0, 0) == ()
    assert source.package_references(4, 4096) == ()
    assert source.package_references(5) == ()
    assert source.line_candidate_count == 1
    assert [line.record_index for line in source.line_records()] == [2]
    assert source.to_dict()["geometry_status"] == "not_decoded"
    assert reference.apply_xy((1.25, -2.5)) == pytest.approx((105, -47.5))
    assert reference.apply_xy((101.75, 48.125)) == pytest.approx((3.75, 153.5))
    rotated = replace(reference, placement_raw=(100, -50, 0, 2, 2, 45))
    assert rotated.apply_xy((1, 0)) == pytest.approx(
        (100 + math.sqrt(2), -50 + math.sqrt(2))
    )
    assert asdict(reference)["name_field_bytes"] == reference.name_field_bytes
    with pytest.raises(FrozenInstanceError):
        reference.coordinate_space = "world"
    native = _core.scan_bytes(bytes(data), ezdracad.ParseLimits()._values())
    assert native.package_references(0, 1)[0][6] == reference.name_key


def test_reference_duplicates_keep_occurrence_order_and_raw_names_are_not_decoded():
    data = sample()
    write_reference(data, 2)
    references = scan(data).package_references()
    assert [reference.record_index for reference in references] == [0, 2]
    assert references[0].placement_raw == references[1].placement_raw
    assert references[0].name_key == references[1].name_key
    data = sample()
    write_reference(data, name=b"\x82\xa0")
    assert scan(data).package_references(0, 1)[0].name_key == b"\x82\xa0"


@pytest.mark.parametrize("tag", [0x0406, 0x0806, 0x0013])
def test_unknown_flags_remain_opaque_even_with_arbitrary_float_bits(tag):
    data = sample()
    offset = CONTENTS_FILE_OFFSET + 8192
    struct.pack_into("<H2x", data, offset, tag)
    data[offset + 16 : offset + 32] = b"\xff" * 16
    assert scan(data).package_references() == ()


@pytest.mark.parametrize("span", [0, 1, 3, 65535])
def test_unqualified_span_fails_only_when_requested(span):
    data = sample()
    struct.pack_into("<H", data, CONTENTS_FILE_OFFSET + 8200, span)
    source = scan(data)
    assert source.package_references(1, 1) == ()
    with pytest.raises(ezdracad.UnsupportedFeatureError, match="span"):
        source.package_references(0, 1)


@pytest.mark.parametrize("tag", [0, 253, 0x04FE])
def test_continuation_must_be_the_exact_following_fe(tag):
    data = sample()
    struct.pack_into("<H", data, CONTENTS_FILE_OFFSET + 8256, tag)
    source = scan(data)
    with pytest.raises(ezdracad.InvalidFormatError, match="following 0x00fe"):
        source.package_references(0, 1)


@pytest.mark.parametrize("fill", [b"\0", b"X"])
def test_names_require_a_nonempty_bounded_nul_prefix(fill):
    data = sample()
    data[CONTENTS_FILE_OFFSET + 8224 : CONTENTS_FILE_OFFSET + 8256] = fill * 32
    source = scan(data)
    with pytest.raises(ezdracad.InvalidFormatError, match="name"):
        source.package_references(0, 1)


def test_terminal_crossing_fails_whole_requested_page_without_returning_prefix():
    data = sample()
    write_reference(data, 3, name=b"TRUNCATED")
    source = scan(data)
    assert len(source.package_references(0, 1)) == 1
    with pytest.raises(ezdracad.InvalidFormatError, match="terminal"):
        source.package_references(0, 4)
    assert source.package_references(0, 0) == ()


@pytest.mark.parametrize("offset", [8208, 8216, 8272, 8280, 8288, 8296, 8304, 8312])
@pytest.mark.parametrize("number", [float("nan"), float("inf"), -float("inf")])
def test_all_anchor_and_continuation_floats_must_be_finite_on_request(offset, number):
    data = sample()
    struct.pack_into("<d", data, CONTENTS_FILE_OFFSET + offset, number)
    source = scan(data)
    assert source.package_references(1, 1) == ()
    assert source.line_candidate_count == 1
    with pytest.raises(ezdracad.InvalidFormatError, match="non-finite"):
        source.package_references(0, 1)


@pytest.mark.parametrize(
    "anchor,placement",
    [
        ((1, 0), (100, -50, 0, 2, 2, 90)),
        ((0, 1), (100, -50, 0, 2, 2, 90)),
        ((0, 0), (100, -50, -10, 2, 2, 90)),
        ((0, 0), (100, -50, 0, 2, 3, 90)),
        ((0, 0), (100, -50, 0, 0, 0, 90)),
        ((0, 0), (100, -50, 0, -2, -2, 90)),
    ],
)
def test_unsupported_transform_states_still_expose_raw_fields(anchor, placement):
    data = sample()
    struct.pack_into("<2d", data, CONTENTS_FILE_OFFSET + 8208, *anchor)
    struct.pack_into("<6d", data, CONTENTS_FILE_OFFSET + 8272, *placement)
    reference = scan(data).package_references(0, 1)[0]
    assert reference.anchor_raw == anchor and reference.placement_raw == placement
    with pytest.raises(ezdracad.UnsupportedFeatureError):
        reference.apply_xy((0, 0))


def test_nonfinite_computation_inputs_and_results_are_rejected():
    reference = scan(sample()).package_references(0, 1)[0]
    for point in ((float("nan"), 0), (0, float("inf")), (-float("inf"), 0)):
        with pytest.raises(ezdracad.InvalidFormatError, match="non-finite"):
            reference.apply_xy(point)
    for changed in (
        replace(reference, anchor_raw=(float("nan"), 0)),
        replace(reference, placement_raw=(100, -50, 0, 2, 2, float("inf"))),
    ):
        with pytest.raises(ezdracad.InvalidFormatError, match="non-finite"):
            changed.apply_xy((0, 0))
    unrotated = replace(reference, placement_raw=(100, -50, 0, 2, 2, 0))
    with pytest.raises(ezdracad.InvalidFormatError, match="result"):
        unrotated.apply_xy((1.7976931348623157e308, 0))


def test_reference_pages_use_existing_range_type_and_count_limits():
    source = scan(sample())
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        source.package_references(0, 4097)
    assert (caught.value.resource, caught.value.limit, caught.value.actual) == (
        "records_per_page",
        4096,
        4097,
    )
    with pytest.raises(IndexError):
        source.package_references(6, 0)
    for number, error in (
        (-1, ValueError),
        (1 << 64, ValueError),
        (True, TypeError),
        (1.5, TypeError),
    ):
        with pytest.raises(error):
            source.package_references(number, 0)
        with pytest.raises(error):
            source.package_references(0, number)
