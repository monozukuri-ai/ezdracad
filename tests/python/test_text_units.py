"""Text-size selectors are independent of physical units and geometry."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError, asdict
from pathlib import Path

import pytest

from ezdracad import ParseLimits, UnsupportedFeatureError, _core, raw

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "text_unit_fixture_assembler", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(assembler)


def contents(marker=25, selector=0):
    payload = bytearray(assembler.make_record_contents(marker))
    struct.pack_into("<H", payload, 0x9AC, selector)
    payload[0x9AA:0x9AC] = b"\xa5\xa5"
    payload[0x9AE:0x9B0] = b"\xff\xff"
    return payload


@pytest.mark.parametrize("selector,unit", [(0, "mm"), (1, "pt")])
def test_text_size_snapshot_source_bytes_and_geometry_are_independent(
    selector, unit, tmp_path
):
    payload = contents(selector=selector)
    envelope = assembler.wrap_record_contents(payload)
    path = tmp_path / "synthetic.mpz"
    path.write_bytes(envelope)
    scan = raw.scanfile(path)
    before = scan.line_records()
    path.unlink()
    info = scan.text_size_unit()
    assert isinstance(info, raw.TextSizeUnitInfo)
    assert asdict(info) == {
        "payload_offset": 0x9AC,
        "selector_bytes": selector.to_bytes(2, "little"),
        "raw_selector": selector,
        "unit": unit,
    }
    native = _core.scan_bytes(envelope, ParseLimits()._values()).text_size_unit()
    assert info == raw.TextSizeUnitInfo(*native)
    assert scan.read_contents() == payload
    assert scan.line_records() == before
    assert before[0].start == (-12.5, 0.25)
    assert all(
        line.units is None and line.coordinate_space == "record_local"
        for line in before
    )
    assert not hasattr(info, "geometry_unit") and not hasattr(info, "origin_units")
    with pytest.raises(FrozenInstanceError):
        info.unit = "mm"


@pytest.mark.parametrize("selector", [2, 255, 256, 0x8000, 0xFFFF])
def test_unknown_selectors_fail_on_demand_without_masking_or_normalization(selector):
    scan = raw.scan_records(assembler.wrap_record_contents(contents(selector=selector)))
    assert scan.line_candidate_count == 2
    with pytest.raises(UnsupportedFeatureError, match="text-size unit selector"):
        scan.text_size_unit()
    assert len(scan.line_records()) == 2
    assert scan.read_contents()[0x9AC:0x9AE] == selector.to_bytes(2, "little")


@pytest.mark.parametrize("marker", [5, 8])
@pytest.mark.parametrize("selector", [0, 1])
def test_matching_selector_does_not_qualify_legacy_profiles(marker, selector):
    scan = raw.scan_records(assembler.wrap_record_contents(contents(marker, selector)))
    with pytest.raises(UnsupportedFeatureError, match="only for observed marker 25"):
        scan.text_size_unit()
    assert len(scan.line_records()) == 2
    assert all(line.units is None for line in scan.line_records())
