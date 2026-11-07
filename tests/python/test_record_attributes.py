"""Synthetic header references, separate from native attribute qualification."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError, asdict
from pathlib import Path

import pytest

from ezdracad import (
    InvalidFormatError,
    LimitExceededError,
    UnsupportedFeatureError,
    raw,
)

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "record_attribute_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def line(tag=2):
    data = bytearray(64)
    struct.pack_into("<HH4BHH", data, 0, tag, 256, 2, 32, 16, 3, 1, 257)
    data[12:16] = bytes([0xA5, 0x5A, 0x42, 0x81])
    if tag in (0x2402, 0x4402):
        data[4] = 0
    struct.pack_into("<6d", data, 16, 1.25, -2.5, 101.75, 48.125, 12.5, 2.5)
    return data


def payload(slots, marker=25):
    data = bytearray(assembler.make_record_contents(marker))
    shift = (len(slots) + 1 - 5) * 64
    for offset in range(0x28, 0x54, 4):
        old = struct.unpack_from("<I", data, offset)[0]
        if old:
            struct.pack_into("<I", data, offset, old + shift)
    struct.pack_into("<I", data, 0x58, len(slots))
    terminal = bytearray(64)
    struct.pack_into("<H", terminal, 0, 255)
    data[8192 : 8192 + 5 * 64] = b"".join(slots) + terminal
    return data


def scan(slots, marker=25):
    return raw.scan_records(assembler.wrap_record_contents(payload(slots, marker)))


@pytest.mark.parametrize("tag", [2, 0x402, 0x30, 0x430, 6])
def test_explicit_indices_and_opaque_header_values_are_preserved(tag):
    data = line(tag)
    attrs = raw.decode_record_attributes(bytes(data[:16]))
    assert attrs == raw.RecordAttributes(
        tag,
        bytes(data[:16]),
        256,
        raw.AttributeReference("index", 2, 2),
        raw.AttributeReference("index", 32, 32),
        raw.AttributeReference("index", 16, 16),
        3,
        257,
        tag >> 8,
        bytes(data[12:16]),
    )
    assert not hasattr(attrs, "effective_color")
    assert asdict(attrs)["line_type"] == {"kind": "index", "index": 32, "raw_code": 32}
    with pytest.raises(FrozenInstanceError):
        attrs.layer_index = 1
    with pytest.raises(FrozenInstanceError):
        attrs.color.index = 7
    data[4] = 0
    data[2:4] = bytes(2)
    attrs = raw.decode_record_attributes(bytes(data[:16]))
    assert attrs.layer_index == 0 and attrs.color == raw.AttributeReference(
        "index", 256, 0
    )


@pytest.mark.parametrize("tag,kind", [(0x2402, "by_layer"), (0x4402, "by_block")])
def test_dependency_selectors_are_kept_without_effective_or_dxf_values(tag, kind):
    data = line(tag)
    data[5] = 0
    data[6] = 255
    attrs = raw.decode_record_attributes(bytes(data[:16]))
    assert attrs.color == raw.AttributeReference(kind, None, 0)
    assert attrs.line_type == raw.AttributeReference("by_layer", None, 0)
    assert attrs.width == raw.AttributeReference("by_block", None, 255)
    assert attrs.flags_raw == tag >> 8
    data[5] = 255
    data[6] = 0
    attrs = raw.decode_record_attributes(bytes(data[:16]))
    assert attrs.line_type.kind == "by_block" and attrs.width.kind == "by_layer"


def test_attributed_pages_have_owned_sources_and_do_not_change_old_queries(tmp_path):
    slots = [line(2), line(0x402), line(0x2402), line(0x4402), line(0x802)]
    contents = payload(slots)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(assembler.wrap_record_contents(contents))
    document = raw.scanfile(path)
    path.unlink()
    lines = document.attributed_line_records(0, 4096)
    assert isinstance(lines, tuple) and [e.source.record_index for e in lines] == [
        0,
        1,
        2,
        3,
    ]
    assert [e.source.tag_raw for e in lines] == [2, 0x402, 0x2402, 0x4402]
    assert len(document.line_records()) == 2 and document.line_candidate_count == 1
    for e in lines:
        assert e.units is None and e.coordinate_space == "record_local"
        assert e.source.start == (1.25, -2.5)
        assert (e.source.elevation, e.source.thickness) == (12.5, 2.5)
        assert (
            e.source.header_bytes
            == e.attributes.header_bytes
            == document.record_bytes(e.source.record_index)[:16]
        )
        assert e.attributes == document.record_attributes(e.source.record_index)
    assert document.attributed_line_records(2, 1) == lines[2:3]
    assert document.attributed_line_records(document.header.record_count) == ()
    assert document.attributed_line_records(0, 0) == ()
    assert document.read_contents() == contents
    with pytest.raises(IndexError):
        document.record_attributes(document.header.record_count)
    with pytest.raises(UnsupportedFeatureError):
        document.record_attributes(document.header.record_count - 1)
    with pytest.raises(IndexError):
        document.attributed_line_records(document.header.record_count + 1)
    with pytest.raises(LimitExceededError, match="records_per_page"):
        document.attributed_line_records(0, 4097)


@pytest.mark.parametrize("tag", [0x802, 0x2002, 0x4002, 0x6402, 0x2430, 0x13])
def test_unknown_flags_are_not_masked(tag):
    data = line(tag)
    with pytest.raises(UnsupportedFeatureError, match="tag/flags"):
        raw.decode_record_attributes(bytes(data[:16]))
    document = scan([data])
    assert document.attributed_line_records() == ()
    with pytest.raises(UnsupportedFeatureError):
        document.record_attributes(0)


@pytest.mark.parametrize("field,value", [(5, 33), (5, 254), (6, 17), (6, 254)])
def test_unknown_reference_codes_fail_whole_pages(field, value):
    data = line()
    data[field] = value
    document = scan([line(), data])
    with pytest.raises(UnsupportedFeatureError):
        document.attributed_line_records()
    assert len(document.line_records()) == 2
    with pytest.raises(UnsupportedFeatureError):
        raw.decode_record_attributes(bytes(data[:16]))


def test_inconsistent_color_flags_and_out_of_range_layer_are_not_corrected():
    data = line(0x2402)
    data[4] = 1
    with pytest.raises(UnsupportedFeatureError, match="zero color"):
        raw.decode_record_attributes(bytes(data[:16]))
    data = line()
    struct.pack_into("<H", data, 2, 257)
    with pytest.raises(UnsupportedFeatureError, match="layer index"):
        raw.decode_record_attributes(bytes(data[:16]))


@pytest.mark.parametrize("marker", [5, 8])
def test_dependency_encodings_in_old_profiles_remain_unqualified(marker):
    document = scan([line(), line(0x2402)], marker)
    assert document.record_attributes(0).layer_index == 256
    with pytest.raises(UnsupportedFeatureError, match="profile"):
        document.record_attributes(1)
    with pytest.raises(UnsupportedFeatureError, match="profile"):
        document.attributed_line_records()
    assert len(document.line_records()) == 1
    assert (
        raw.decode_record_attributes(document.record_bytes(1)[:16]).color.kind
        == "by_layer"
    )
    for field in (5, 6):
        data = line()
        data[field] = 0
        document = scan([data], marker)
        with pytest.raises(UnsupportedFeatureError, match="profile"):
            document.attributed_line_records()
        assert len(document.line_records()) == 1


@pytest.mark.parametrize("offset", range(16, 64, 8))
def test_all_six_fields_of_colored_lines_are_checked_on_demand(offset):
    data = line(0x4402)
    struct.pack_into("<d", data, offset, float("nan"))
    document = scan([line(), data])
    with pytest.raises(InvalidFormatError, match="non-finite"):
        document.attributed_line_records()
    assert len(document.line_records()) == 1


@pytest.mark.parametrize("size", [0, 15, 17, 64])
def test_header_helper_requires_exactly_sixteen_bytes(size):
    with pytest.raises(InvalidFormatError, match="exactly 16"):
        raw.decode_record_attributes(bytes(line()[:size]))


@pytest.mark.parametrize(
    "value", [bytearray(16), memoryview(bytes(16)), None, "header"]
)
def test_header_helper_does_not_guess_input_types(value):
    with pytest.raises(TypeError, match="header_bytes must be bytes"):
        raw.decode_record_attributes(value)


@pytest.mark.parametrize(
    "value,error",
    [
        (True, TypeError),
        (-1, ValueError),
        (1 << 64, ValueError),
        (1.5, TypeError),
        (None, TypeError),
    ],
)
def test_index_and_window_arguments_are_explicit_unsigned_integers(value, error):
    document = scan([line()])
    with pytest.raises(error, match="record_index"):
        document.record_attributes(value)
    with pytest.raises(error, match="record_start"):
        document.attributed_line_records(value)
    with pytest.raises(error, match="record_count"):
        document.attributed_line_records(0, value)


def test_inline_colored_lines_preserve_selectors_without_attribute_inheritance():
    package = line(0x30)
    package[16:] = bytes(48)
    struct.pack_into("<I", package, 16, 0x400)
    struct.pack_into("<H", package, 8, 2)
    document = scan([package, line(0x2402)])
    result = document.inline_package_lines(0)
    assert result[0].source.tag_raw == 0x2402
    assert (
        raw.decode_record_attributes(result[0].source.header_bytes).color.kind
        == "by_layer"
    )
    assert (
        raw.decode_record_attributes(result[0].ancestors[0].header_bytes).color.index
        == 2
    )
    assert result[0].units is None
    with pytest.raises(UnsupportedFeatureError, match="profile"):
        scan([package, line(0x2402)], 5).inline_package_lines(0)
    bad = line(0x4402)
    bad[5] = 254
    with pytest.raises(UnsupportedFeatureError):
        scan([package, bad]).inline_package_lines(0)
