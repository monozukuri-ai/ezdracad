import hashlib
import json
import struct
import subprocess
import sys
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

import ezdracad
from ezdracad import _core

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"


@pytest.mark.parametrize("marker", [5, 8, 25])
def test_scan_snapshot_pages_opaque_records_and_xy(marker, tmp_path):
    envelope = (FIXTURES / f"records{marker}.cfb").read_bytes()
    contents = (FIXTURES / f"records{marker}.bin").read_bytes()
    path = tmp_path / "図面.data"
    path.write_bytes(envelope)
    scan = ezdracad.raw.scanfile(path)
    assert scan == ezdracad.raw.scan_records(envelope)
    path.write_bytes(b"changed")
    assert scan.read_contents() == contents
    assert scan.sha256 == hashlib.sha256(contents).hexdigest()
    assert scan.header.record_count == 5
    assert scan.header.declared_nonterminal_records == 4
    assert scan.header.marker_raw == marker
    assert scan.header.profile == f"observed_marker{marker}_stride64"
    assert scan.tag_counts == {2: 2, 254: 1, 255: 1, 2050: 1}
    assert scan.opaque_record_count == 2 and scan.line_candidate_count == 2
    assert scan.records(1, 2) == (
        ezdracad.raw.RawRecord(1, 8256, 2050),
        ezdracad.raw.RawRecord(2, 8320, 2),
    )
    assert scan.records(4, 4096)[0].tag == 255
    assert scan.records(5) == ()
    assert scan.records(0, 0) == ()
    assert scan.line_candidates(1, 1) == ()
    assert [x.record_index for x in scan.line_candidates()] == [0, 2]
    first = scan.line_candidates()[0]
    assert first.start == (-12.5, 0.25) and first.end == (42.0, -3.125)
    assert scan.record_bytes(1) == contents[8256:8320]
    for index in (5, 1 << 63):
        with pytest.raises(IndexError):
            scan.record_bytes(index)
        if index > 5:
            with pytest.raises(IndexError):
                scan.records(index)
    with pytest.raises(FrozenInstanceError):
        scan.header.record_count = 0
    with pytest.raises(TypeError):
        scan.tag_counts[2] = 0
    report = scan.to_dict()
    assert json.loads(json.dumps(report, allow_nan=False)) == report
    assert report["geometry_status"] == "not_decoded"
    assert report["units"] is None and report["coordinate_system"] is None
    assert report["payload"]["offset_domain"] == "cfb_stream"
    assert "records" not in report and "line_candidates" not in report
    # Native rows and the typed public wrapper describe the same owned result.
    native = _core.scan_bytes(envelope, ezdracad.ParseLimits()._values())
    assert native.metadata()["tag_counts"] == scan.tag_counts
    assert native.line_candidates(0, 5)[0][2] == list(first.start)


@pytest.mark.parametrize("marker", [24, 26, 65535])
def test_marker25_does_not_enable_other_markers(marker):
    envelope = bytearray((FIXTURES / "records25.cfb").read_bytes())
    struct.pack_into("<H", envelope, 1536 + 34, marker)
    with pytest.raises(ezdracad.UnsupportedFormatError):
        ezdracad.raw.scan_records(bytes(envelope))


@pytest.mark.parametrize(
    "offset,value,error",
    [
        (0x24, 8191, ezdracad.UnsupportedFeatureError),
        (0x58, 3, ezdracad.InvalidFormatError),
        (0x4C, 1, ezdracad.InvalidFormatError),
        (0x20, 0x00190001, ezdracad.UnsupportedFeatureError),
    ],
)
def test_marker25_retains_record_origin_count_tail_and_flag_checks(
    offset, value, error
):
    envelope = bytearray((FIXTURES / "records25.cfb").read_bytes())
    struct.pack_into("<I", envelope, 1536 + offset, value)
    with pytest.raises(error):
        ezdracad.raw.scan_records(bytes(envelope))


def test_record_limits_and_page_limits_are_structured():
    envelope = (FIXTURES / "records5.cfb").read_bytes()
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        ezdracad.raw.scan_records(envelope, limits=ezdracad.ParseLimits(max_records=4))
    assert (caught.value.resource, caught.value.limit, caught.value.actual) == (
        "records",
        4,
        5,
    )
    scan = ezdracad.raw.scan_records(envelope)
    for page in (scan.records, scan.line_candidates, scan.line_records):
        with pytest.raises(ezdracad.LimitExceededError) as caught:
            page(0, 4097)
        assert (caught.value.resource, caught.value.limit, caught.value.actual) == (
            "records_per_page",
            4096,
            4097,
        )
        for value, error in [
            (-1, ValueError),
            (1 << 64, ValueError),
            (True, TypeError),
            (1.5, TypeError),
        ]:
            with pytest.raises(error):
                page(value, 0)


def test_line_records_preserve_record_local_fields_duplicates_and_raw_header(tmp_path):
    data = bytearray((FIXTURES / "records5.cfb").read_bytes())
    values = (-12.5, 0.25, 42, -3.125, 12.5, 2.5)
    for index, tag in ((0, 0x0002), (1, 0x0402)):
        offset = 1536 + 8192 + index * 64
        data[offset : offset + 16] = b"\xa5" * 16
        struct.pack_into("<H", data, offset, tag)
        struct.pack_into("<6d", data, offset + 16, *values)
    struct.pack_into("<H14x6d", data, 1536 + 8320, 0x0802, *([float("nan")] * 6))
    path = tmp_path / "local.mpz"
    path.write_bytes(data)
    scan = ezdracad.raw.scanfile(path)
    path.write_bytes(b"changed")
    lines = scan.line_records(0, 4096)
    assert len(lines) == 2
    assert [line.record_index for line in lines] == [0, 1]
    assert [line.tag_raw for line in lines] == [0x0002, 0x0402]
    assert scan.line_candidate_count == 1 and scan.opaque_record_count == 3
    for index, line in enumerate(lines):
        assert line.payload_offset == 8192 + index * 64
        assert line.header_bytes == bytes(
            data[1536 + line.payload_offset : 1536 + line.payload_offset + 16]
        )
        assert line.start == (-12.5, 0.25) and line.end == (42, -3.125)
        assert line.elevation == 12.5 and line.thickness == 2.5
        assert line.coordinate_space == "record_local"
        assert line.units is None and line.parent_context is None
    assert scan.line_records(1, 1) == (lines[1],)
    assert scan.line_records(2, 2) == ()
    assert scan.line_records(4, 4096) == ()
    assert scan.line_records(5) == ()
    assert scan.line_records(0, 0) == ()
    with pytest.raises(IndexError):
        scan.line_records(6)
    with pytest.raises(FrozenInstanceError):
        lines[0].coordinate_space = "world"
    assert scan.to_dict()["geometry_status"] == "not_decoded"
    native = _core.scan_bytes(bytes(data), ezdracad.ParseLimits()._values())
    assert native.line_records(0, 1)[0][3] == lines[0].header_bytes


@pytest.mark.parametrize(
    "tag,offset", [(2, 48), (2, 56), *[(0x0402, o) for o in (16, 24, 32, 40, 48, 56)]]
)
@pytest.mark.parametrize("value", [float("nan"), float("inf"), -float("inf")])
def test_line_record_nonfinite_fields_fail_only_the_requested_page(tag, offset, value):
    data = bytearray((FIXTURES / "records5.cfb").read_bytes())
    struct.pack_into("<H", data, 1536 + 8192, tag)
    struct.pack_into("<d", data, 1536 + 8192 + offset, value)
    scan = ezdracad.raw.scan_records(bytes(data))
    assert len(scan.line_candidates(0, 1)) == (0 if tag == 0x0402 else 1)
    with pytest.raises(ezdracad.InvalidFormatError, match="non-finite"):
        scan.line_records(0, 1)
    assert scan.line_records(0, 0) == ()
    assert scan.line_records(2, 1)[0].record_index == 2


def test_line_record_page_does_not_return_a_valid_prefix_before_a_later_error():
    data = bytearray((FIXTURES / "records5.cfb").read_bytes())
    struct.pack_into("<H14x6d", data, 1536 + 8256, 0x0402, 1, 2, 3, 4, 5, float("nan"))
    scan = ezdracad.raw.scan_records(bytes(data))
    assert scan.line_records(0, 1)[0].record_index == 0
    with pytest.raises(ezdracad.InvalidFormatError, match="non-finite"):
        scan.line_records(0, 2)
    assert scan.line_records(2, 1)[0].record_index == 2


def test_nonzero_terminal_body_is_preserved_with_an_uninterpreted_diagnostic():
    data = bytearray((FIXTURES / "records5.cfb").read_bytes())
    body = bytes(range(62))
    data[1536 + 8450 : 1536 + 8512] = body
    scan = ezdracad.raw.scan_records(bytes(data))
    assert scan.record_bytes(4) == b"\xff\0" + body
    assert scan.line_candidate_count == 2
    assert len(scan.line_records()) == 2
    assert any(item.code == "terminal_data_uninterpreted" for item in scan.diagnostics)


def test_scan_preserves_actual_case_of_cfb_stream_path():
    data = bytearray((FIXTURES / "records5.cfb").read_bytes())
    name = "cOnTeNtS\0".encode("utf-16le")
    data[640 : 640 + len(name)] = name
    scan = ezdracad.raw.scan_records(bytes(data))
    assert scan.stream_path == "/cOnTeNtS"
    assert scan.to_dict()["payload"]["stream_path"] == "/cOnTeNtS"


def test_malformed_and_unsupported_cross_native_boundary():
    # This independent fixture has one regular FAT stream starting at file +1536.
    envelope = bytearray((FIXTURES / "records5.cfb").read_bytes())
    struct.pack_into("<d", envelope, 1536 + 8192 + 16, float("nan"))
    with pytest.raises(ezdracad.InvalidFormatError, match="non-finite"):
        ezdracad.raw.scan_records(bytes(envelope))
    with pytest.raises(ezdracad.UnsupportedFormatError):
        ezdracad.raw.scanfile(FIXTURES / "mpz-envelope.cfb")
    with pytest.raises(ezdracad.UnsupportedFormatError):
        ezdracad.raw.scanfile(FIXTURES / "minimal.cfb")
    with pytest.raises(ezdracad.UnsupportedFormatError):
        ezdracad.raw.scan_records((FIXTURES / "records5.bin").read_bytes())
    with pytest.raises(TypeError):
        ezdracad.raw.scan_records(envelope)
    with pytest.raises(TypeError):
        ezdracad.raw.scanfile(bytes(FIXTURES))


def cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", "scan", *map(str, args), "--json"],
        capture_output=True,
        text=True,
    )


def test_scan_cli_summary_pages_and_failures(tmp_path):
    path = FIXTURES / "records5.cfb"
    result = cli(path)
    assert result.returncode == 0 and result.stderr == ""
    assert json.loads(result.stdout) == ezdracad.raw.scanfile(path).to_dict()
    result = cli(path, "--records", "--line-candidates", "--start", 1, "--count", 2)
    report = json.loads(result.stdout)
    assert report["page"]["record_end"] == 3
    assert [x["index"] for x in report["records"]] == [1, 2]
    assert [x["record_index"] for x in report["line_candidates"]] == [2]
    result = cli(path, "--max-records", 4)
    assert result.returncode == 3 and json.loads(result.stderr)["resource"] == "records"
    assert cli(FIXTURES / "mpz-envelope.cfb").returncode == 2
    assert cli(tmp_path / "missing").returncode == 1
    for args in [("--count", 4097), ("--count", -1), ("--start", -1), ("--start", 6)]:
        result = cli(path, *args)
        assert result.returncode == 2 and "Traceback" not in result.stderr
