"""Raw code preservation is separate from native physical-unit interpretation."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError, asdict
from pathlib import Path

import pytest

from ezdracad import ParseLimits, UnsupportedFeatureError, _core, raw
from ezdracad.origin_units import OriginUnitsCode

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "origin_units_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def contents(marker=25, code=0):
    payload = bytearray(assembler.make_record_contents(marker))
    payload[0x14C:0x150] = bytes([0xA5]) * 4
    struct.pack_into("<i", payload, 0x150, code)
    payload[0x154:0x158] = bytes([0x5A]) * 4
    return payload


@pytest.mark.parametrize("code", [0, 1, 2, 3, -1, -(2**31), 2**31 - 1, 0x01020304])
def test_raw_code_snapshot_survives_source_removal_and_does_not_transform_lines(
    code, tmp_path
):
    payload = contents(code=code)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "raw-origin-code.mpz"
    path.write_bytes(envelope)
    scan = raw.scanfile(path)
    lines = scan.line_records()
    path.unlink()
    value = scan.origin_units_code()
    assert isinstance(value, OriginUnitsCode)
    assert value == OriginUnitsCode(0x150, struct.pack("<i", code), code)
    assert asdict(value) == {
        "payload_offset": 0x150,
        "selector_bytes": bytes(payload[0x150:0x154]),
        "raw_code": code,
    }
    assert scan.read_contents() == payload
    assert scan.line_records() == lines
    assert all(line.units is None for line in lines)
    native = _core.scan_bytes(envelope, ParseLimits()._values()).origin_units_code()
    assert OriginUnitsCode(*native) == value
    with pytest.raises(FrozenInstanceError):
        value.raw_code = 0


@pytest.mark.parametrize("marker", [5, 8])
def test_legacy_codes_require_independent_native_field_evidence(marker):
    payload = contents(marker=marker, code=1)
    scan = raw.scan_records(assembler.wrap_record_contents(payload))
    with pytest.raises(UnsupportedFeatureError, match="marker 25"):
        scan.origin_units_code()
    assert scan.read_contents() == payload
    assert len(scan.line_records()) == 2
