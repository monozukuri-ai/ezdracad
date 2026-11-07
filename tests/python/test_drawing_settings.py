"""Independent synthetic header values; native meaning is checked separately."""

import importlib.util
import math
import struct
from dataclasses import FrozenInstanceError, asdict
from pathlib import Path

import pytest

from ezdracad import InvalidFormatError, ParseLimits, _core, raw

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "settings_fixture_assembler", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def contents(marker=5, origin=(-12.5, 250.25), factor=0.04):
    payload = bytearray(assembler.make_record_contents(marker))
    payload[0x140:0x180] = bytes([0xA5]) * 64
    struct.pack_into("<2d", payload, 0x140, *origin)
    struct.pack_into("<d", payload, 0x178, factor)
    return payload


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_metadata_snapshot_frames_opaque_gaps_and_native_binding_agree(
    marker, tmp_path
):
    payload = contents(marker)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(envelope)
    scan = raw.scanfile(path)
    before = scan.line_records()
    path.unlink()
    settings = scan.drawing_settings()
    assert settings == raw.DrawingSettings(
        0x140, bytes(payload[0x140:0x180]), (-12.5, 250.25), 0.04, 25
    )
    assert settings.units is None
    assert settings.header_bytes[16:56] == bytes([0xA5]) * 40
    assert asdict(settings) == {
        "payload_offset": 0x140,
        "header_bytes": bytes(payload[0x140:0x180]),
        "drawing_origin": (-12.5, 250.25),
        "paper_scale_factor": 0.04,
        "drawing_scale": 25,
        "units": None,
    }
    native = _core.scan_bytes(envelope, ParseLimits()._values()).drawing_settings()
    assert (
        raw.DrawingSettings(native[0], native[1], tuple(native[2]), *native[3:])
        == settings
    )
    assert scan.read_contents() == payload and scan.line_records() == before
    assert before[0].start == (-12.5, 0.25)
    assert all(
        line.coordinate_space == "record_local" and line.units is None
        for line in before
    )
    with pytest.raises(FrozenInstanceError):
        settings.drawing_scale = 1


@pytest.mark.parametrize("offset", [0x140, 0x148, 0x178])
@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
def test_nonfinite_metadata_is_rejected_on_request(offset, value):
    payload = contents()
    struct.pack_into("<d", payload, offset, value)
    scan = raw.scan_records(assembler.wrap_record_contents(payload))
    assert scan.line_candidate_count == 2
    with pytest.raises(InvalidFormatError, match="non-finite"):
        scan.drawing_settings()
    assert len(scan.line_records()) == 2


@pytest.mark.parametrize("factor", [0.0, -0.0, -0.01, math.ulp(0.0)])
def test_nonpositive_and_reciprocal_overflow_scale_are_rejected(factor):
    scan = raw.scan_records(assembler.wrap_record_contents(contents(factor=factor)))
    with pytest.raises(InvalidFormatError):
        scan.drawing_settings()
    assert scan.header.record_count == 5


def test_unknown_header_bytes_are_not_parsed_as_other_settings():
    payload = contents(origin=(-0.0, 0.0))
    struct.pack_into("<d", payload, 0x158, float("nan"))
    struct.pack_into("<d", payload, 0x208, float("inf"))
    scan = raw.scan_records(assembler.wrap_record_contents(payload))
    settings = scan.drawing_settings()
    assert math.copysign(1, settings.drawing_origin[0]) == -1
    assert settings.drawing_origin[1] == 0
    assert settings.header_bytes == payload[0x140:0x180]
    assert not hasattr(settings, "annotation_scale")
    assert not hasattr(settings, "paper_origin")
