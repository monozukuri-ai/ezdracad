"""Independent synthetic nesting checks; native qualification lives separately."""

import importlib.util
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
from ezdracad.scene import SceneLimits, SceneLine2D

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "scene_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def offset(data, at):
    return struct.unpack_from("<I", data, at)[0]


def slot(tag, span=1, layer=1, style=(1, 1, 1)):
    data = bytearray(64)
    struct.pack_into("<HH4BHH", data, 0, tag, layer, *style, 19, span, 1234)
    return data


def line(tag=2, layer=1, style=(1, 1, 1)):
    data = slot(tag, layer=layer, style=style)
    struct.pack_into("<6d", data, 16, 1, 2, 3, 4, 0, 0)
    return data


def inline(span):
    data = slot(0x30, span, 7, (9, 10, 11))
    struct.pack_into("<I", data, 16, 0x400)
    return data


def reference(name, layer=0, style=(1, 1, 1), placement=(0, 0, 0, 1, 1, 0)):
    data = slot(6, 2, layer, style)
    data[32 : 32 + len(name)] = name
    tail = slot(0xFE, 0)
    struct.pack_into("<6d", tail, 16, *placement)
    return [data, tail]


def fixture(root, definitions=()):
    original = bytearray(assembler.make_semantics_contents(25))
    old_end = offset(original, 0x28)
    suffix = original[offset(original, 0x50) - 7272 : offset(original, 0x50)]
    data = bytearray(original)
    shift = (len(root) + 1) * 64 - (old_end - 8192)
    for at in range(0x28, 0x54, 4):
        old = offset(data, at)
        if old:
            struct.pack_into("<I", data, at, old + shift)
    struct.pack_into("<I", data, 0x58, len(root))
    data[8192:old_end] = b"".join(root) + slot(0xFF, 0)
    data = data[: offset(data, 0x40)] + bytearray(296)
    start = len(data)
    for name, records in definitions:
        header = bytearray(176)
        header[: len(name)] = name
        struct.pack_into("<I", header, 36, len(records))
        data += header + b"".join(records) + slot(0xFF, 0)
    data += suffix
    end = len(data)
    data += bytes(4)
    struct.pack_into("<II", data, 0x4C, start if definitions else 0, end)
    for index, values in ((0, (15, 16, 15)), (5, (2, 3, 4)), (7, (9, 10, 11))):
        at = end - 7272 + 96 if index == 0 else offset(data, 0x28) + (index - 1) * 64
        struct.pack_into("<3H", data, at + 4, *values)
    return data


def nested():
    outer = reference(b"INNER", style=(7, 255, 255), placement=(100, 50, 0, 2, 2, 90))
    outer += reference(
        b"INNER", layer=7, style=(8, 9, 10), placement=(-10, 20, 0, 1, 1, 0)
    )
    return fixture(
        reference(
            b"OUTER", layer=5, style=(12, 13, 14), placement=(-150, 200, 0, 3, 3, 30)
        ),
        (
            (b"OUTER", outer),
            (
                b"INNER",
                [
                    inline(2),
                    line(2, 0, (9, 10, 11)),
                    line(0x4402, 0, (0, 255, 255)),
                    line(0x2402, 0, (0, 0, 0)),
                ],
            ),
        ),
    )


def scan(data):
    return raw.scan_records(assembler.wrap_record_contents(data))


def triple(row):
    return tuple(
        getattr(row.styles, name).index for name in ("color", "line_type", "width")
    )


def test_recursive_placements_styles_and_owned_evidence_survive_input_removal(tmp_path):
    data = nested()
    path = tmp_path / "scene.mpz"
    path.write_bytes(assembler.wrap_record_contents(data))
    document = raw.scanfile(path)
    path.unlink()
    rows = document.scene_lines(0)
    assert isinstance(rows, tuple) and len(rows) == 6
    assert all(isinstance(row, SceneLine2D) for row in rows)
    assert [triple(row) for row in rows] == [
        (9, 10, 11),
        (7, 13, 14),
        (2, 3, 4),
        (9, 10, 11),
        (8, 9, 10),
        (9, 10, 11),
    ]
    assert rows[0].start == pytest.approx(
        (-228 + 144 * math.sqrt(3), 344 + 78 * math.sqrt(3))
    )
    assert rows[3].start == pytest.approx(
        (-183 - 13.5 * math.sqrt(3), 186.5 + 33 * math.sqrt(3))
    )
    assert rows[0].source.start == (1, 2)
    assert rows[0].coordinate_scope == "package_parent" and rows[0].units is None
    assert rows[0].style_layer_index == 5 and rows[3].style_layer_index == 7
    assert rows[0].attributes.layer_index == 0
    assert rows[0].attributes.material_index_raw == 19
    assert rows[0].attributes.group_index == 1234
    assert [a.kind for a in rows[0].ancestors] == [
        "block_reference",
        "block_reference",
        "inline_package",
    ]
    assert [s.kind for s in rows[0].styles.width.sources] == [
        "record",
        "block_placement",
        "block_placement",
        "width",
    ]
    for row in rows:
        for ancestor in row.ancestors:
            assert (
                ancestor.raw_bytes
                == data[
                    ancestor.payload_offset : ancestor.payload_offset
                    + len(ancestor.raw_bytes)
                ]
            )
            if ancestor.target_definition:
                target = ancestor.target_definition
                assert (
                    target.header_bytes
                    == data[target.payload_offset : target.payload_offset + 176]
                )
        for name in ("color", "line_type", "width"):
            for source in getattr(row.styles, name).sources:
                assert (
                    source.raw_bytes
                    == data[
                        source.payload_offset : source.payload_offset
                        + len(source.raw_bytes)
                    ]
                )
    with pytest.raises(FrozenInstanceError):
        rows[0].style_layer_index = 99
    with pytest.raises(FrozenInstanceError):
        rows[0].ancestors[0].record_index = 99


def test_inline_dependent_attribute_scopes_fail_until_native_qualification():
    row = scan(fixture([inline(2), line(2, 0, (1, 1, 1))])).scene_lines(0)[0]
    assert triple(row) == (1, 1, 1)
    assert row.start == (1, 2) and row.coordinate_scope == "root_records"
    assert row.cumulative_transform == (1, 0, 0, 1, 0, 0)
    for child in [line(0x4402, 0, (0, 255, 255)), line(0x2402, 0, (0, 0, 0))]:
        with pytest.raises(UnsupportedFeatureError, match="inline-dependent"):
            scan(fixture([inline(2), child])).scene_lines(0)
        with pytest.raises(UnsupportedFeatureError, match="inline-dependent"):
            scan(
                fixture(reference(b"A", 5, (2, 2, 5)), [(b"A", [inline(2), child])])
            ).scene_lines(0)


def test_exact_color_byblock_reference_stays_scoped_and_retains_original_flagged_bytes():
    data = nested()
    inner = offset(data, 0x4C) + 176
    struct.pack_into("<H", data, inner, 0x4006)
    data[inner + 4] = 0
    row = scan(data).scene_lines(0)[1]
    assert triple(row) == (12, 13, 14)
    assert row.ancestors[1].raw_bytes[:2] == bytes.fromhex("0640")
    assert [source.kind for source in row.styles.color.sources] == [
        "record",
        "block_placement",
        "block_placement",
        "color",
    ]
    direct = reference(b"A", style=(0, 255, 255))
    struct.pack_into("<H", direct[0], 0, 0x4006)
    document = scan(fixture(direct, [(b"A", [line(0x4402, 1, (0, 255, 255))])]))
    with pytest.raises(UnsupportedFeatureError, match="ByBlock"):
        document.scene_lines(0)
    assert document.package_references() == ()
    with pytest.raises(UnsupportedFeatureError):
        document.record_attributes(0)


def test_exact_bylayer_reference_resolves_root_zero_and_nested_insertion_layers():
    data = nested()
    inner = offset(data, 0x4C) + 176
    struct.pack_into("<H", data, inner, 0x2006)
    data[inner + 4 : inner + 7] = bytes(3)
    row = scan(data).scene_lines(0)[1]
    assert triple(row) == (2, 3, 4)
    assert row.styles.color.sources[1].raw_bytes[:2] == bytes.fromhex("0620")
    assert row.styles.color.sources[3].index == 5
    direct = reference(b"A", style=(0, 0, 0))
    struct.pack_into("<H", direct[0], 0, 0x2006)
    document = scan(fixture(direct, [(b"A", [line(0x4402, 1, (0, 255, 255))])]))
    assert triple(document.scene_lines(0)[0]) == (15, 16, 15)
    assert document.package_references() == ()
    with pytest.raises(UnsupportedFeatureError):
        document.record_attributes(0)


@pytest.mark.parametrize(
    "definitions",
    [
        [(b"A", reference(b"A"))],
        [(b"A", reference(b"B")), (b"B", reference(b"A"))],
        [(b"A", [line()]), (b"A", [line()])],
        [],
    ],
)
def test_cycles_ambiguous_and_missing_definition_fail_without_results(definitions):
    with pytest.raises((UnsupportedFeatureError, InvalidFormatError)):
        scan(fixture(reference(b"A"), definitions)).scene_lines(0)


@pytest.mark.parametrize(
    "limits",
    [SceneLimits(max_depth=2), SceneLimits(max_nodes=2), SceneLimits(max_lines=3)],
)
def test_each_traversal_budget_is_enforced(limits):
    with pytest.raises(LimitExceededError):
        scan(nested()).scene_lines(0, limits=limits)


def test_unknown_child_and_malformed_later_root_fail_the_whole_selected_request():
    with pytest.raises(UnsupportedFeatureError):
        scan(fixture([inline(3), line(), slot(0x7777)])).scene_lines(0)
    with pytest.raises(InvalidFormatError):
        scan(fixture([line(), slot(0x7777, 0)])).scene_lines(0)
    for index in (1, 2, 99):
        with pytest.raises(UnsupportedFeatureError):
            scan(nested()).scene_lines(index)
