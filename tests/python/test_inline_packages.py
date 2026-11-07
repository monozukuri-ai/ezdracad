"""Independent synthetic inline-package trees; native evidence is separate."""

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
    "inline_package_fixture_assembler", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def slot(tag, span):
    data = bytearray(64)
    struct.pack_into("<H6xH", data, 0, tag, span)
    return data


def package(span):
    data = slot(0x30, span)
    data[2:8] = bytes([17, 0, 2, 3, 4, 5])
    data[10:16] = bytes([0x34, 0x12, 0xA5, 0x5A, 0x42, 0x81])
    struct.pack_into("<I", data, 16, 0x400)
    return data


def line(tag=2):
    data = slot(tag, 1)
    data[2:8] = bytes([8, 0, 9, 10, 11, 12])
    struct.pack_into("<6d", data, 16, 1.25, -2.5, 101.75, 48.125, 12.5, 2.5)
    return data


def nested():
    return [package(4), package(2), line(0x402), line()]


def contents(slots, marker=5):
    data = bytearray(assembler.make_record_contents(marker))
    shift = (len(slots) + 1 - 5) * 64
    for offset in range(0x28, 0x54, 4):
        old = struct.unpack_from("<I", data, offset)[0]
        if old:
            struct.pack_into("<I", data, offset, old + shift)
    struct.pack_into("<I", data, 0x58, len(slots))
    data[8192 : 8192 + 5 * 64] = b"".join(slots) + slot(0xFF, 0)
    return data


def scan(slots, marker=5):
    return raw.scan_records(assembler.wrap_record_contents(contents(slots, marker)))


def definition_contents(marker=5):
    data = contents([line()], marker)
    section_end = struct.unpack_from("<I", data, 0x40)[0]
    data = data[:section_end] + bytes(296)
    definition_start = len(data)
    header = bytearray(176)
    header[:4] = b"PKG\0"
    records = [line(), *nested()]
    struct.pack_into("<I", header, 36, len(records))
    data.extend(header)
    data.extend(b"".join(records) + slot(0xFF, 0))
    data.extend(bytes([0xAB]) * (7272 if marker == 25 else 1768))
    data.extend(b"tail")
    struct.pack_into("<I", data, 0x4C, definition_start)
    struct.pack_into("<I", data, 0x50, len(data) - 4)
    return data


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_complete_ancestry_local_values_snapshot_and_immutable_provenance(
    marker, tmp_path
):
    slots = nested()
    payload = contents(slots, marker)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(assembler.wrap_record_contents(payload))
    document = raw.scanfile(path)
    flat = document.line_records()
    path.unlink()
    lines = document.inline_package_lines(0)
    assert isinstance(lines, tuple) and len(lines) == 2
    assert lines[0].source == flat[0] and lines[1].source == flat[1]
    assert [[a.record_index for a in e.ancestors] for e in lines] == [[0, 1], [0]]
    assert [a.record_count for a in lines[0].ancestors] == [4, 2]
    assert lines[0].ancestors[0] == raw.InlinePackageRecord(
        0, 8192, 4, bytes(slots[0][:16]), bytes(slots[0][16:])
    )
    for e in lines:
        assert e.table_domain == "root_records" and e.definition_index is None
        assert e.units is None and e.coordinate_space == "record_local"
        assert e.source.start == (1.25, -2.5)
        assert (e.source.elevation, e.source.thickness) == (12.5, 2.5)
    assert asdict(lines[0])["ancestors"][1]["header_bytes"] == bytes(slots[1][:16])
    assert not hasattr(lines[0].ancestors[1], "packageflag")
    assert not hasattr(lines[0], "effective_attributes")
    with pytest.raises(FrozenInstanceError):
        lines[0].ancestors[0].record_count = 1
    with pytest.raises(FrozenInstanceError):
        lines[0].source = flat[1]
    assert document.read_contents() == payload and document.line_records() == flat


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_definition_domain_is_distinct_and_direct_planar_contract_is_unchanged(
    marker, tmp_path
):
    payload = definition_contents(marker)
    path = tmp_path / "definition.mpz"
    path.write_bytes(assembler.wrap_record_contents(payload))
    library = raw.scanfile(path).block_library()
    path.unlink()
    lines = library.inline_package_lines(0, 1)
    assert [[a.record_index for a in e.ancestors] for e in lines] == [[1, 2], [1]]
    for e in lines:
        assert e.table_domain == "block_definition_records" and e.definition_index == 0
        assert (
            e.source.payload_offset
            == library.definitions[0].record_start + e.source.record_index * 64
        )
        assert (
            e.source.header_bytes == library.record_bytes(0, e.source.record_index)[:16]
        )
        assert e.source.coordinate_space == "record_local" and e.units is None
    with pytest.raises(UnsupportedFeatureError):
        library.planar_lines(0)
    with pytest.raises(UnsupportedFeatureError, match="inside"):
        library.inline_package_lines(0, 2)
    with pytest.raises(InvalidFormatError, match="definition index"):
        library.inline_package_lines(1, 0)


def test_selected_subtree_requires_a_primary_and_does_not_claim_the_rest_of_the_table():
    document = scan(nested())
    for interior in (1, 2, 3):
        with pytest.raises(UnsupportedFeatureError, match="inside"):
            document.inline_package_lines(interior)
    with pytest.raises(InvalidFormatError, match="outside"):
        document.inline_package_lines(4)
    document = scan(
        [slot(0x7777, 2), slot(0xFE, 0), package(2), line(), slot(0x7778, 0)]
    )
    assert document.inline_package_lines(2)[0].source.record_index == 3
    with pytest.raises(InvalidFormatError):
        document.object_spans()
    with pytest.raises(UnsupportedFeatureError, match="inside"):
        document.inline_package_lines(1)
    with pytest.raises(InvalidFormatError, match="zero"):
        scan([slot(0x7777, 0), package(2), line()]).inline_package_lines(1)


@pytest.mark.parametrize(
    "kind",
    [
        "unknown",
        "metadata",
        "flagged_package",
        "empty",
        "state",
        "other_kind",
        "line_span",
    ],
)
def test_unknown_child_after_a_valid_line_fails_the_whole_request(kind):
    data = package(2)
    if kind == "unknown":
        data = slot(0x7777, 1)
    elif kind == "metadata":
        data = slot(0x33, 1)
    elif kind == "flagged_package":
        data = slot(0x430, 1)
    elif kind == "empty":
        data = package(1)
    elif kind == "state":
        data[20] = 1
    elif kind == "other_kind":
        struct.pack_into("<I", data, 16, 3)
    else:
        data = line()
        struct.pack_into("<H", data, 8, 2)
    span = struct.unpack_from("<H", data, 8)[0]
    document = scan(
        [package(2 + span), line(), data, *[line() for _ in range(1, span)]]
    )
    with pytest.raises(UnsupportedFeatureError):
        document.inline_package_lines(0)


@pytest.mark.parametrize("tag,span", [(0xFE, 0), (0x7777, 0), (0x7777, 3)])
def test_malformed_child_after_a_valid_line_is_not_returned_partially(tag, span):
    document = scan([package(3), line(), slot(tag, span)])
    with pytest.raises(InvalidFormatError):
        document.inline_package_lines(0)


@pytest.mark.parametrize("offset", range(16, 64, 8))
def test_all_six_float_fields_are_checked_on_demand(offset):
    bad = line(0x402)
    struct.pack_into("<d", bad, offset, float("inf"))
    document = scan([package(3), line(), bad])
    with pytest.raises(InvalidFormatError, match="non-finite"):
        document.inline_package_lines(0)


@pytest.mark.parametrize(
    "keyword,bound,actual",
    [("max_depth", 1, 2), ("max_nodes", 3, 4), ("max_lines", 1, 2)],
)
def test_budgets_count_depth_all_nodes_and_lines_independently(keyword, bound, actual):
    document = scan(nested())
    with pytest.raises(LimitExceededError) as error:
        document.inline_package_lines(0, **{keyword: bound})
    assert error.value.resource == "inline_package_" + keyword.removeprefix("max_")
    assert error.value.limit == bound and error.value.actual == actual
    assert len(document.inline_package_lines(0)) == 2


def test_absolute_depth_budget_cannot_be_raised_by_a_large_argument():
    slots = [package(count) for count in range(130, 1, -1)] + [line()]
    with pytest.raises(LimitExceededError) as error:
        scan(slots).inline_package_lines(0, max_depth=(1 << 64) - 1)
    assert error.value.resource == "inline_package_depth"
    assert error.value.limit == raw.MAX_INLINE_PACKAGE_DEPTH == 128
    assert error.value.actual == 129
    assert raw.MAX_INLINE_PACKAGE_PROVENANCE == 131072


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
def test_integer_arguments_do_not_accept_bools_negative_or_overflow(value, error):
    document = scan(nested())
    library = raw.scan_records(
        assembler.wrap_record_contents(definition_contents())
    ).block_library()
    with pytest.raises(error, match="record_index"):
        document.inline_package_lines(value)
    for keyword in ("max_depth", "max_nodes", "max_lines"):
        with pytest.raises(error, match=keyword):
            document.inline_package_lines(0, **{keyword: value})
        with pytest.raises(error, match=keyword):
            library.inline_package_lines(0, 1, **{keyword: value})
    with pytest.raises(error, match="definition_index"):
        library.inline_package_lines(value, 1)
    with pytest.raises(error, match="record_index"):
        library.inline_package_lines(0, value)
    with pytest.raises(TypeError):
        document.inline_package_lines(0, 32)


def test_observed_moved_wrapper_retains_already_transformed_child_coordinates():
    slots = nested()
    struct.pack_into("<H", slots[0], 0, 0x430)
    struct.pack_into("<6d", slots[2], 16, 101.25, -52.5, 201.75, -1.875, 0, 0)
    struct.pack_into("<6d", slots[3], 16, 70, -70, 160, -70, 0, 0)
    document = scan(slots, marker=25)
    result = document.inline_package_lines(0)
    assert [(e.source.start, e.source.end) for e in result] == [
        ((101.25, -52.5), (201.75, -1.875)),
        ((70, -70), (160, -70)),
    ]
    assert [a.record_index for a in result[0].ancestors] == [0, 1]
    assert result[0].ancestors[0].header_bytes[:2] == b"\x30\x04"
    assert result[0].source.units is None
    struct.pack_into("<H", slots[0], 0, 0x830)
    with pytest.raises(UnsupportedFeatureError):
        scan(slots, marker=25).inline_package_lines(0)
