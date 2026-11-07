"""Independent synthetic style tables; native XML comparisons are opt-in research."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

from ezdracad import UnsupportedFeatureError, raw

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "style_fixture_assembler", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_complete_tables_special_rows_unknown_bytes_and_snapshot(marker, tmp_path):
    payload = assembler.make_semantics_contents(marker)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(assembler.wrap_record_contents(payload))
    scan = raw.scanfile(path)
    tables = scan.style_tables()
    path.unlink()
    assert (
        tables.layer_count,
        tables.color_count,
        tables.line_type_count,
        tables.width_count,
    ) == (257, 256, 32, 16)
    assert tables.layer(2).color_index_raw == 3
    assert tables.layer(2).line_type_index_raw == 4
    assert tables.layer(2).width_index_raw == 5
    assert tables.layer(0).raw_bytes[64:] == bytes([0xA5]) * 232
    assert len(tables.layer(256).raw_bytes) == 296
    assert tables.color(1).rgb == (37, 19, 11)
    assert tables.color(1).output_rgb == (7, 3, 1)
    assert tables.color(256).rgb == (0, 0, 0)
    assert tables.color(256).pen_width_hundredths_raw == 768
    assert tables.color(256).pen_width_value == 7.68
    assert tables.width(5).pen_width_hundredths_raw == 35
    assert tables.width(5).pen_width_value == 0.35
    assert tables.line_type(4).pen_definition_hundredths_raw == (125, 37) + (0,) * 28
    assert tables.line_type(4).pen_definition_values == (1.25, 0.37) + (0.0,) * 28
    assert (
        tables.suffix_bytes()
        == payload[tables.shared_suffix_offset : scan.header.payload_end]
    )
    assert (
        len(tables.extension_header_offsets) == len(tables.extension_header_bytes) == 3
    )
    for index, data in zip(
        tables.extension_header_offsets, tables.extension_header_bytes
    ):
        assert data == payload[index : index + 16]
    for getter, indices in (
        (tables.layer, range(257)),
        (tables.color, range(1, 257)),
        (tables.line_type, range(1, 33)),
        (tables.width, range(1, 17)),
    ):
        for index in indices:
            row = getter(index)
            assert row.index == index
            assert (
                row.raw_bytes
                == payload[row.payload_offset : row.payload_offset + len(row.raw_bytes)]
            )
    if marker == 25:
        assert tables.suffix_bytes()[1768:] == bytes([0x5A]) * 5504
    else:
        assert tables.shared_suffix_size == 1768
    with pytest.raises(FrozenInstanceError):
        tables.color(1).index = 2
    assert tables != scan.style_tables()  # Immutable views are separate snapshots.


@pytest.mark.parametrize(
    "damage", ["extension_type", "extension_length", "padding", "layer_number"]
)
def test_unqualified_layout_is_not_silently_normalized(damage):
    payload = bytearray(assembler.make_semantics_contents())
    scan = raw.scan_records(assembler.wrap_record_contents(payload))
    tables = scan.style_tables()
    if damage == "extension_type":
        struct.pack_into("<I", payload, tables.shared_suffix_offset, 999)
    elif damage == "extension_length":
        struct.pack_into("<I", payload, tables.shared_suffix_offset + 12, 65)
    elif damage == "padding":
        payload[tables.shared_suffix_offset + 704] = 1
    else:
        struct.pack_into("<H", payload, tables.layer(2).payload_offset + 10, 0)
    with pytest.raises(UnsupportedFeatureError):
        raw.scan_records(assembler.wrap_record_contents(payload)).style_tables()


def test_lookup_indices_keep_explicit_type_and_domain_boundaries():
    tables = raw.scan_records(
        assembler.wrap_record_contents(assembler.make_semantics_contents())
    ).style_tables()
    for getter in (tables.layer, tables.color, tables.line_type, tables.width):
        for value in (True, "1", None):
            with pytest.raises(TypeError):
                getter(value)
        for value in (-1, 1 << 16):
            with pytest.raises(ValueError):
                getter(value)
    for getter, value in (
        (tables.layer, 257),
        (tables.color, 0),
        (tables.line_type, 33),
        (tables.width, 17),
    ):
        with pytest.raises(IndexError):
            getter(value)
