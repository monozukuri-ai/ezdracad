"""Independent synthetic root-slot spans; not native-format semantic evidence."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError, asdict
from pathlib import Path

import pytest

from ezdracad import InvalidFormatError, LimitExceededError, ParseLimits, _core, raw

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "object_fixture_assembler", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def slot(tag, span):
    result = bytearray([0xA5] * 64)
    struct.pack_into("<H", result, 0, tag)
    struct.pack_into("<H", result, 8, span)
    return result


def contents(slots, marker=5):
    data = bytearray(assembler.make_record_contents(marker))
    shift = (len(slots) + 1 - 5) * 64
    for offset in range(0x28, 0x54, 4):
        old = struct.unpack_from("<I", data, offset)[0]
        if old:
            struct.pack_into("<I", data, offset, old + shift)
    struct.pack_into("<I", data, 0x58, len(slots))
    terminal = bytearray(64)
    struct.pack_into("<H", terminal, 0, 0xFF)
    terminal[31] = 0x81
    data[8192 : 8192 + 5 * 64] = b"".join(slots) + terminal
    return bytes(data)


def scan(slots, marker=5):
    return raw.scan_records(assembler.wrap_record_contents(contents(slots, marker)))


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_spans_keep_exact_bounds_headers_order_and_opaque_interiors(marker, tmp_path):
    slots = [slot(0x30, 3), slot(0xFE, 0), slot(2, 0), slot(0x7FFE, 1)]
    payload = contents(slots, marker)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(envelope)
    document = raw.scanfile(path)
    path.unlink()
    objects = document.object_spans()
    assert isinstance(objects, tuple)
    assert objects == (
        raw.RawObjectSpan(0, 8192, 0x30, bytes(slots[0][:16]), 3, 3),
        raw.RawObjectSpan(3, 8384, 0x7FFE, bytes(slots[3][:16]), 1, 4),
    )
    assert asdict(objects[0]) == {
        "record_index": 0,
        "payload_offset": 8192,
        "tag_raw": 0x30,
        "header_bytes": bytes(slots[0][:16]),
        "record_count": 3,
        "end_record_index": 3,
    }
    with pytest.raises(FrozenInstanceError):
        objects[0].record_count = 1
    assert document.read_contents() == payload
    assert document.record_bytes(1) == bytes(slots[1])
    assert document.record_bytes(2) == bytes(slots[2])
    assert document.line_candidate_count == len(document.line_records()) == 1
    assert document.header.record_count == 5
    assert document.to_dict()["geometry_status"] == "not_decoded"
    assert document.to_dict()["units"] is None
    native = _core.scan_bytes(envelope, ParseLimits()._values()).object_spans(2)
    assert [raw.RawObjectSpan(*row) for row in native] == list(objects)


def test_output_budget_counts_primary_spans_and_does_not_return_partial_results():
    document = scan([slot(0x30, 3), slot(0xFE, 0), slot(0x0402, 0), slot(0x7777, 1)])
    assert len(document.object_spans(max_objects=2)) == 2
    for bound, actual in ((0, 1), (1, 2)):
        with pytest.raises(LimitExceededError) as error:
            document.object_spans(max_objects=bound)
        assert error.value.resource == "object_spans"
        assert error.value.limit == bound and error.value.actual == actual
    assert len(document.object_spans(max_objects=(1 << 64) - 1)) == 2


@pytest.mark.parametrize("tag,span", [(0x7777, 0), (0x7777, 3), (0xFE, 1)])
def test_late_malformed_span_fails_whole_request_without_resynchronization(tag, span):
    document = scan([slot(0x33, 1), slot(tag, span), slot(0x7778, 1)])
    assert document.records()[0].tag == 0x33
    with pytest.raises(InvalidFormatError):
        document.object_spans()
    with pytest.raises(InvalidFormatError):
        document.object_spans()
    assert document.records()[2].tag == 0x7778


def test_terminal_excluded_exact_boundary_empty_table_and_unknown_tags():
    document = scan([slot(0x4321, 4), slot(0xFE, 0), slot(0xFE, 0), slot(0xFE, 0)])
    assert [(e.record_index, e.end_record_index) for e in document.object_spans()] == [
        (0, 4)
    ]
    assert document.tag_counts[255] == 1
    assert scan([]).object_spans(max_objects=0) == ()
    singles = scan([slot(2, 1), slot(0x0402, 1), slot(0x3333, 1), slot(0x04FE, 1)])
    assert [e.tag_raw for e in singles.object_spans()] == [2, 0x0402, 0x3333, 0x04FE]


@pytest.mark.parametrize(
    "value,error",
    [
        (True, TypeError),
        (False, TypeError),
        (-1, ValueError),
        (1 << 64, ValueError),
        (1.5, TypeError),
        (None, TypeError),
    ],
)
def test_object_budget_rejects_invalid_integer_inputs(value, error):
    document = scan([slot(0x7777, 1)])
    with pytest.raises(error, match="max_objects"):
        document.object_spans(max_objects=value)
    with pytest.raises(TypeError):
        document.object_spans(1)


def test_new_span_validation_does_not_change_existing_raw_fixture_contract():
    document = raw.scan_records((ROOT / "tests/fixtures/records5.cfb").read_bytes())
    assert document.line_candidate_count == 2 and len(document.line_records()) == 2
    with pytest.raises(InvalidFormatError, match="zero object span"):
        document.object_spans()
    assert document.header.record_count == 5
