"""Synthetic 3D raw-record contracts; native evidence is checked separately."""

import struct
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

import ezdracad
from ezdracad import _core, raw
from ezdracad.line3d import LineRecord3D

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
VALUES = (1.25, -2.5, 12.5, 101.75, 48.125, -3.75)


def fixture(marker=5):
    data = bytearray((FIXTURES / f"records{marker}.cfb").read_bytes())
    for index in (0, 1):
        offset = 1536 + 8192 + index * 64
        data[offset : offset + 16] = bytes([0xA5]) * 16
        struct.pack_into("<H", data, offset, 0x0013)
        struct.pack_into("<6d", data, offset + 16, *VALUES)
    return data


@pytest.mark.parametrize("marker", [5, 25])
def test_records_preserve_xyz_order_header_duplicates_and_owned_snapshot(
    marker, tmp_path
):
    data = fixture(marker)
    path = tmp_path / "raw3d.mpz"
    path.write_bytes(data)
    scan = raw.scanfile(path)
    path.unlink()
    lines = scan.line_records_3d()
    assert len(lines) == 2 and all(isinstance(line, LineRecord3D) for line in lines)
    for index, line in enumerate(lines):
        assert line.record_index == index and line.payload_offset == 8192 + 64 * index
        assert line.tag_raw == 0x0013
        assert (
            line.header_bytes
            == data[1536 + line.payload_offset : 1552 + line.payload_offset]
        )
        assert line.start == VALUES[:3] and line.end == VALUES[3:]
        assert line.coordinate_space == "record_local"
        assert line.units is line.parent_context is None
    assert scan.line_records_3d(1, 1) == lines[1:]
    assert scan.line_records_3d(2, 1) == ()
    assert scan.line_records_3d(0, 0) == ()
    assert scan.line_records_3d(scan.header.record_count) == ()
    assert len(scan.line_records()) == 1
    assert scan.to_dict()["coordinate_system"] is None
    with pytest.raises(FrozenInstanceError):
        lines[0].start = (0, 0, 0)
    handle = _core.scan_bytes(bytes(data), ezdracad.ParseLimits()._values())
    native = handle.line_records_3d(0, 1)[0]
    assert native[3] == lines[0].header_bytes
    assert native[4] == list(VALUES[:3]) and native[5] == list(VALUES[3:])


@pytest.mark.parametrize("marker", [8])
def test_unobserved_profile_is_refused_even_for_an_empty_page(marker):
    scan = raw.scan_records(bytes(fixture(marker)))
    with pytest.raises(ezdracad.UnsupportedFeatureError):
        scan.line_records_3d()
    with pytest.raises(ezdracad.UnsupportedFeatureError):
        scan.line_records_3d(0, 0)
    assert scan.records()


@pytest.mark.parametrize("offset", [16, 24, 32, 40, 48, 56])
@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
def test_nonfinite_fields_fail_only_the_requested_page(offset, value):
    data = fixture()
    struct.pack_into("<d", data, 1536 + 8192 + 64 + offset, value)
    scan = raw.scan_records(bytes(data))
    assert len(scan.line_records_3d(0, 1)) == 1
    with pytest.raises(ezdracad.InvalidFormatError):
        scan.line_records_3d(0, 2)
    assert scan.line_records_3d(2, 1) == ()
    assert scan.read_contents() == data[1536 : 1536 + len(scan.read_contents())]


@pytest.mark.parametrize("tag", [0x0413, 0x0813, 0x2013, 0x4013, 0x8013, 0x0012])
def test_unknown_tag_variants_are_opaque_without_flag_masking(tag):
    data = fixture()
    struct.pack_into("<H14x6d", data, 1536 + 8192, tag, *([float("nan")] * 6))
    scan = raw.scan_records(bytes(data))
    assert [line.record_index for line in scan.line_records_3d()] == [1]


def test_page_limits_and_argument_types_use_the_existing_bounded_window():
    scan = raw.scan_records(bytes(fixture()))
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        scan.line_records_3d(0, 4097)
    assert (caught.value.resource, caught.value.limit, caught.value.actual) == (
        "records_per_page",
        4096,
        4097,
    )
    with pytest.raises(IndexError):
        scan.line_records_3d(6)
    for value, error in (
        (True, TypeError),
        (1.5, TypeError),
        (-1, ValueError),
        (1 << 64, ValueError),
    ):
        with pytest.raises(error):
            scan.line_records_3d(value, 0)
        with pytest.raises(error):
            scan.line_records_3d(0, value)
