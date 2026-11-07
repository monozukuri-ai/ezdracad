"""Synthetic context/provenance checks, separate from native rule qualification."""

import importlib.util
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
from ezdracad.effective_styles import EffectiveBlockLineStyle, EffectiveLineStyle

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "effective_style_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def offset(data, at):
    return struct.unpack_from("<I", data, at)[0]


def header(data, at, tag, layer, values):
    struct.pack_into("<HH3B", data, at, tag, layer, *values)


def layer(data, index, values):
    suffix = offset(data, 0x50) - (7272 if offset_marker(data) == 25 else 1768)
    at = (
        suffix + {0: 96, 256: 408}[index]
        if index in (0, 256)
        else offset(data, 0x28) + (index - 1) * 64
    )
    struct.pack_into("<3H", data, at + 4, *values)


def offset_marker(data):
    return struct.unpack_from("<H", data, 0x22)[0]


def scan(data):
    return raw.scan_records(assembler.wrap_record_contents(data))


def root(marker=25):
    return bytearray(assembler.make_semantics_contents(marker))


def blocks():
    data = bytearray(assembler.make_block_contents(25))
    styles = root()
    for index in range(6):
        a, b = (offset(data, at + 4 * index) for at in (0x28, 0x2C))
        c, d = (offset(styles, at + 4 * index) for at in (0x28, 0x2C))
        assert b - a == d - c
        data[a:b] = styles[c:d]
    end, style_end = offset(data, 0x50), offset(styles, 0x50)
    data[end - 7272 : end] = styles[style_end - 7272 : style_end]
    header(data, 8192, 6, 5, (12, 13, 14))
    header(data, 8192 + 128, 6, 6, (13, 14, 15))
    data[8192 + 160 : 8192 + 192] = b"TWO\0".ljust(32, b"\0")
    library = scan(data).block_library()
    first, second = (d.record_start for d in library.definitions)
    header(data, first, 2, 1, (1, 1, 1))
    header(data, second, 0x4402, 0, (0, 255, 255))
    header(data, second + 64, 0x402, 7, (7, 8, 9))
    for index, triple in (
        (0, (15, 16, 15)),
        (5, (2, 3, 4)),
        (6, (4, 5, 6)),
        (7, (9, 10, 11)),
    ):
        layer(data, index, triple)
    return data


def triple(styles):
    return tuple(getattr(styles, key).index for key in ("color", "line_type", "width"))


def assert_sources(styles, data):
    for name in ("color", "line_type", "width"):
        resolved = getattr(styles, name)
        assert isinstance(resolved.sources, tuple)
        assert 2 <= len(resolved.sources) <= 4
        for source in resolved.sources:
            assert (
                source.raw_bytes
                == data[
                    source.payload_offset : source.payload_offset
                    + len(source.raw_bytes)
                ]
            )


def test_root_explicit_and_bylayer_retain_sources_without_aci_or_printing():
    data = root()
    header(data, 8192 + 128, 0x402, 256, (0, 32, 16))
    document = scan(data)
    result = document.effective_line_style(2)
    assert isinstance(result, EffectiveLineStyle)
    assert triple(result.styles) == (256, 32, 16)
    assert result.units is None and result.coordinate_space == "record_local"
    assert result.source.header_bytes == result.attributes.header_bytes
    assert [s.kind for s in result.styles.color.sources] == ["record", "color"]
    assert result.styles.color.sources[-1].index == 256
    assert_sources(result.styles, data)
    assert not hasattr(result, "printed_width") and not hasattr(result, "visible")
    header(data, 8192 + 128, 0x2402, 256, (0, 0, 0))
    layer(data, 256, (2, 3, 4))
    document = scan(data)
    result = document.effective_line_style(2)
    assert triple(result.styles) == (2, 3, 4)
    assert [s.kind for s in result.styles.color.sources] == ["record", "layer", "color"]
    assert result.styles.color.sources[1].index == 256
    assert len(result.styles.color.sources[1].raw_bytes) == 296
    assert document.line_records() == ()  # Existing exact-tag contract is unchanged.


def test_root_layer_zero_uses_its_own_table_without_a_block_context():
    data = root()
    header(data, 8192 + 128, 0x2402, 0, (0, 0, 0))
    layer(data, 0, (9, 10, 11))
    result = scan(data).effective_line_style(2)
    assert triple(result.styles) == (9, 10, 11)
    assert result.styles.color.sources[1].index == 0
    assert_sources(result.styles, data)


def test_missing_context_and_interior_selection_are_rejected():
    document = scan(root())
    with pytest.raises(UnsupportedFeatureError, match="ByBlock"):
        document.effective_line_style(2)
    for index in (0, 1, 3, 7, 8):
        with pytest.raises(UnsupportedFeatureError):
            document.effective_line_style(index)
    data = root()
    header(data, 8192 + 128, 0x802, 1, (1, 1, 1))
    with pytest.raises(UnsupportedFeatureError):
        scan(data).effective_line_style(2)
    header(data, 8192 + 128, 0x402, 1, (1, 1, 1))
    struct.pack_into("<H", data, 8192 + 5 * 64 + 8, 0)
    with pytest.raises(InvalidFormatError):
        scan(data).effective_line_style(2)


@pytest.mark.parametrize("values", [(0, 1, 1), (257, 1, 1), (1, 33, 1), (1, 1, 17)])
def test_undocumented_layer_sentinels_are_not_recursively_guessed(values):
    data = root()
    header(data, 8192 + 128, 0x2402, 1, (0, 0, 0))
    layer(data, 1, values)
    with pytest.raises(UnsupportedFeatureError, match="style-table reference"):
        scan(data).effective_line_style(2)


@pytest.mark.parametrize("marker", [5, 8])
def test_older_profiles_keep_explicit_only_qualification(marker):
    data = root(marker)
    header(data, 8192 + 128, 0x402, 1, (2, 3, 4))
    assert triple(scan(data).effective_line_style(2).styles) == (2, 3, 4)
    header(data, 8192 + 128, 0x2402, 1, (0, 0, 0))
    with pytest.raises(UnsupportedFeatureError, match="profile"):
        scan(data).effective_line_style(2)


def test_one_definition_multiple_placements_and_explicit_child_override():
    data = blocks()
    document = scan(data)
    library = document.block_library()
    for reference, expected in ((0, (12, 13, 14)), (2, (13, 14, 15))):
        rows = library.effective_line_styles(reference)
        assert isinstance(rows, tuple) and len(rows) == 2
        assert all(isinstance(row, EffectiveBlockLineStyle) for row in rows)
        assert all(row.definition_index == 1 for row in rows)
        assert triple(rows[0].styles) == expected
        assert triple(rows[1].styles) == (7, 8, 9)
        assert rows[0].reference_record_index == reference
        assert rows[0].coordinate_space == "package_parent" and rows[0].units is None
        assert rows[0].source.coordinate_space == "record_local"
        assert rows[0].source.header_bytes == library.record_bytes(1, 0)[:16]
        placement = rows[0].styles.color.sources[1]
        assert placement.kind == "block_placement" and placement.index == reference
        assert len(placement.raw_bytes) == 128
        assert_sources(rows[0].styles, data)
    with pytest.raises(UnsupportedFeatureError):
        library.planar_lines(1)
    with pytest.raises(UnsupportedFeatureError):
        library.placed_lines(0)
    assert document.line_candidate_count == 0


def test_block_layer_zero_bylayer_and_nonzero_layer_are_distinct():
    data = blocks()
    at = scan(data).block_library().definitions[1].record_start
    header(data, at, 0x2402, 0, (0, 0, 0))
    header(data, at + 64, 0x2402, 7, (0, 0, 0))
    library = scan(data).block_library()
    for reference, expected, selected_layer in ((0, (2, 3, 4), 5), (2, (4, 5, 6), 6)):
        rows = library.effective_line_styles(reference)
        assert triple(rows[0].styles) == expected
        assert triple(rows[1].styles) == (9, 10, 11)
        assert rows[0].styles.color.sources[2].index == selected_layer
        assert rows[1].styles.color.sources[2].index == 7
        assert_sources(rows[0].styles, data)


def test_insertion_type_width_bylayer_and_unresolved_outer_dependency():
    data = blocks()
    data[8192 + 5] = 0
    data[8192 + 6] = 0
    rows = scan(data).block_library().effective_line_styles(0)
    assert triple(rows[0].styles) == (12, 3, 4)
    assert [s.kind for s in rows[0].styles.width.sources] == [
        "record",
        "block_placement",
        "layer",
        "width",
    ]
    data[8192 + 5] = 255
    with pytest.raises(UnsupportedFeatureError, match="insertion itself"):
        scan(data).block_library().effective_line_styles(0)


def test_block_output_is_bounded_and_no_partial_results_escape():
    data = blocks()
    library = scan(data).block_library()
    with pytest.raises(LimitExceededError, match="effective_style_lines"):
        library.effective_line_styles(0, max_lines=1)
    with pytest.raises(LimitExceededError, match="effective_style_lines_limit"):
        library.effective_line_styles(0, max_lines=4097)
    with pytest.raises(UnsupportedFeatureError, match="primary"):
        library.effective_line_styles(1)
    leaf = library.definitions[1].record_start
    for at, packed in (
        (8192 + 112, struct.pack("<d", 2)),
        (8192 + 16, struct.pack("<d", 1)),
        (leaf + 48, struct.pack("<d", 1)),
        (leaf + 24, struct.pack("<d", float("nan"))),
        (leaf + 64, struct.pack("<H", 0x30)),
    ):
        changed = data[:]
        changed[at : at + len(packed)] = packed
        with pytest.raises((UnsupportedFeatureError, InvalidFormatError)):
            scan(changed).block_library().effective_line_styles(0)


def test_effective_results_remain_immutable_and_owned_after_input_removal(tmp_path):
    data = blocks()
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(assembler.wrap_record_contents(data))
    library = raw.scanfile(path).block_library()
    path.unlink()
    result = library.effective_line_styles(0)[0]
    assert triple(result.styles) == (12, 13, 14)
    assert_sources(result.styles, data)
    for obj, field, value in (
        (result, "definition_index", 7),
        (result.styles.color, "index", 8),
        (result.styles.color.sources[0], "index", 9),
    ):
        with pytest.raises(FrozenInstanceError):
            setattr(obj, field, value)
