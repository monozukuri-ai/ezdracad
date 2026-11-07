"""Independent synthetic block-library data; no vendor/native fixture bytes."""

import importlib.util
import struct
from dataclasses import asdict
from pathlib import Path

import pytest

from ezdracad import (
    InvalidFormatError,
    LimitExceededError,
    ParseLimits,
    UnsupportedFeatureError,
    raw,
)

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "block_fixture_assembler", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)


fixture = assembler.make_block_contents


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_library_snapshot_names_frames_and_independent_order(marker, tmp_path):
    data = fixture(marker)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(assembler.wrap_record_contents(data))
    scan = raw.scanfile(path)
    library = scan.block_library()
    path.unlink()  # Later requests must retain one owned Rust snapshot.
    assert [d.name_bytes for d in library.definitions] == [b"BASE", b"TWO"]
    assert len(library.definitions[0].header_bytes) == 176
    assert library.definitions[0].record_end == library.definitions[1].header_offset
    assert library.suffix_offset == library.definitions[1].record_end
    assert library.suffix_bytes() == bytes([0xAB]) * library.suffix_size
    references = scan.package_references(0, 5)
    assert references[0].name_key == b"TWO"
    assert references[0].name_field_bytes[4:] == bytes([0xA5]) * 28
    assert library.resolve(references[0].name_key).index == 1
    assert library.resolve(references[1].name_key).index == 0
    local = library.planar_lines(1)
    assert [(e.definition_index, e.record_index) for e in local] == [(1, 0), (1, 1)]
    assert [e.tag_raw for e in local] == [2, 0x0402]
    assert (
        local[1].coordinate_space == "block_definition_local" and local[1].units is None
    )
    assert local[1].payload_offset == library.definitions[1].record_start + 64
    placed = library.placed_lines(references[0].record_index)
    assert placed[0].start == (-23.75, 72.5)
    assert placed[1].end == (17.0, 75.25)
    assert placed[1].source.header_bytes == library.record_bytes(1, 1)[:16]
    assert all(
        e.coordinate_space == "package_parent"
        and e.units is None
        and e.parent_context is None
        for e in placed
    )
    rotated = library.placed_lines(references[1].record_index)
    assert rotated[0].start == pytest.approx((105, -47.5))
    assert rotated[0].end == pytest.approx((3.75, 153.5))
    assert asdict(rotated[0])["source"]["definition_index"] == 0
    assert len(library.records(1, 2, 1)) == 1
    assert library.records(1, 3) == ()
    assert library.records(1, 0, 0) == ()
    with pytest.raises(IndexError):
        library.records(1, 4)
    with pytest.raises(IndexError):
        library.record_bytes(3, 0)
    with pytest.raises(UnsupportedFeatureError, match="unresolved"):
        library.resolve(b"missing")


def test_limits_types_unknown_objects_and_nonfinite_fields():
    data = fixture()
    scan = raw.scan_records(assembler.wrap_record_contents(data))
    with pytest.raises(LimitExceededError, match="block_definitions"):
        scan.block_library(max_definitions=1)
    with pytest.raises(LimitExceededError, match="records"):
        raw.scan_records(
            assembler.wrap_record_contents(data), limits=ParseLimits(max_records=9)
        ).block_library()
    library = scan.block_library()
    with pytest.raises(LimitExceededError, match="block_lines"):
        library.placed_lines(0, max_lines=1)
    with pytest.raises(LimitExceededError, match="records_per_page"):
        library.records(0, 0, 4097)
    for value in (-1, True, 1 << 64):
        with pytest.raises((TypeError, ValueError)):
            scan.block_library(max_definitions=value)
        with pytest.raises((TypeError, ValueError)):
            library.planar_lines(value)
        with pytest.raises((TypeError, ValueError)):
            library.placed_lines(0, max_lines=value)
    with pytest.raises(TypeError, match="encoding"):
        library.resolve("BASE")
    compound = raw.scan_records(
        assembler.wrap_record_contents(fixture(compound=True))
    ).block_library()
    assert compound.records(1, 1, 1)[0].tag == 0x0402
    with pytest.raises(UnsupportedFeatureError, match="0030"):
        compound.placed_lines(0)
    definition = library.definitions[1]
    bad = bytearray(data)
    struct.pack_into("<d", bad, definition.record_start + 56, float("nan"))
    catalog = raw.scan_records(assembler.wrap_record_contents(bad)).block_library()
    with pytest.raises(InvalidFormatError, match="non-finite"):
        catalog.placed_lines(0)
