"""Synthetic MPX grammar/API boundaries; native correspondence is tested separately."""

import copy
import hashlib
import json
import struct
import subprocess
import sys
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

import ezdracad as ez
from ezdracad import _core, raw
from ezdracad.document import OpaqueRegion, SourceRef, _report

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fixtures"
MPX = FIXTURES / "mpx25.mpx"
MPZ = FIXTURES / "document25.cfb"
CONTENTS = FIXTURES / "document25.bin"


def cli(command, path, *options):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", command, str(path), *options],
        capture_output=True,
        text=True,
        check=False,
    )


def test_flat_input_is_identified_by_structure_and_owned_without_a_fake_cfb(tmp_path):
    data = MPX.read_bytes()
    path = tmp_path / "図面.without_format_extension"
    path.write_bytes(data)
    info = raw.inspectfile(path)
    assert info == raw.inspect(data)
    assert info.format == "mpx" and info.cfb is None
    assert info.file_size == len(data) == info.payload.size
    assert info.payload.stream_path is None and info.payload.offset_domain == "file"
    assert info.payload.marker_raw == 25
    assert info.payload.sha256 == hashlib.sha256(data).hexdigest()
    assert info.read_payload() == info.read_contents() == data
    path.write_bytes(b"replaced after inspection")
    path.unlink()
    assert info.read_payload() == data
    row = info.to_dict()
    assert row["format"] == "mpx" and row["cfb"] is None
    assert row["payload"]["stream_path"] is None
    assert row["payload"]["offset_domain"] == "file"
    assert row["units"] is None and row["geometry_status"] == "not_decoded"
    assert json.loads(json.dumps(row, allow_nan=False)) == row
    with pytest.raises(FrozenInstanceError):
        info.payload.offset_domain = "cfb_stream"
    with pytest.raises(ez.InvalidFormatError):
        raw.inspect_cfb(data)


def test_raw_scan_retains_distinct_flat_header_eof_and_exact_file_offsets(tmp_path):
    data = MPX.read_bytes()
    path = tmp_path / "synthetic.bin"
    path.write_bytes(data)
    scan = raw.scanfile(path)
    assert scan == raw.scan_records(data)
    assert scan.format == "mpx" and scan.offset_domain == "file"
    assert scan.stream_path is None
    assert scan.header.payload_end_field_raw == 0
    assert scan.header.payload_end_basis == "file_length"
    assert scan.header.payload_end == scan.file_size == scan.payload_size == len(data)
    assert scan.header.record_count == 3 and scan.header.record_start == 8192
    assert scan.sha256 == hashlib.sha256(data).hexdigest()
    assert scan.read_payload() == data
    path.unlink()
    assert scan.read_contents() == data
    assert [record.tag for record in scan.records()] == [2, 2, 255]
    for record in scan.records():
        assert (
            scan.record_bytes(record.index)
            == data[record.payload_offset : record.payload_offset + 64]
        )
    row = scan.to_dict()
    assert row["format"] == "mpx" and row["payload"]["offset_domain"] == "file"
    assert row["payload"]["stream_path"] is None
    assert row["header"]["payload_end_field_raw"] == 0
    assert row["header"]["payload_end_basis"] == "file_length"
    assert json.loads(json.dumps(row, allow_nan=False)) == row
    native = _core.scan_bytes(data, ez.ParseLimits()._values()).metadata()
    assert native["format"] == scan.format
    assert native["offset_domain"] == scan.offset_domain
    assert native["header"]["payload_end"] == scan.header.payload_end


def test_reading_reuses_line_guards_but_never_relabels_file_sources_as_contents(
    tmp_path,
):
    data = MPX.read_bytes()
    path = tmp_path / "input.data"
    path.write_bytes(data)
    document = ez.readfile(path, scope="stored_xy_lines")
    path.write_bytes(b"replaced")
    path.unlink()
    assert document.report.source_format == "mpx"
    assert (
        document.report.stream_path is None and document.report.offset_domain == "file"
    )
    assert document.report.contents_sha256 == document.report.payload_sha256
    assert document.report.payload_sha256 == hashlib.sha256(data).hexdigest()
    assert document.report.status == ez.ReadStatus.COMPLETE
    assert document.report.document_status == ez.ReadStatus.PARTIAL
    assert not document.report.document_complete and document.report.units is None
    assert len(document.entities) == 2
    expected = [((-12.5, 0.25), (42.0, 0.25)), ((2.5, -3.75), (17.125, 8.5))]
    for entity, (start, end) in zip(document.entities, expected):
        source = entity.source_ref
        assert entity.start == start and entity.end == end and entity.units is None
        assert source.stream_path is None and source.offset_domain == "file"
        assert source.payload_offset == 8192 + 64 * source.record_index
        assert (
            entity.source.header_bytes
            == data[source.payload_offset : source.payload_offset + 16]
        )
    assert all(region.stream_path is None for region in document.report.opaque_regions)
    assert all(
        region.offset_domain == "file" for region in document.report.opaque_regions
    )
    encoded = document.to_dict()
    assert encoded["report"]["source_format"] == "mpx"
    assert all(e["source_ref"]["offset_domain"] == "file" for e in encoded["entities"])
    assert "cfb_stream" not in json.dumps(encoded)
    assert document.raw().format == "mpx" and document.raw().read_contents() == data
    with pytest.raises(ez.IncompleteReadError) as caught:
        ez.read(data)
    assert caught.value.report.source_format == "mpx"
    assert caught.value.report.entity_count == 2
    assert caught.value.report.offset_domain == "file"


def test_mpz_carrier_keeps_its_cfb_and_contents_domains():
    contents = CONTENTS.read_bytes()
    info = raw.inspectfile(MPZ)
    assert info.format == "mpz" and info.cfb is not None
    assert info.payload.offset_domain == "cfb_stream"
    assert info.payload.stream_path == "/Contents"
    assert info.read_payload() == contents
    scan = raw.scanfile(MPZ)
    assert scan.format == "mpz" and scan.offset_domain == "cfb_stream"
    assert (
        scan.header.payload_end_field_raw
        == scan.header.payload_end
        == len(contents) - 4
    )
    assert scan.header.payload_end_basis == "declared_field"
    document = ez.readfile(MPZ, scope="stored_xy_lines")
    assert document.report.source_format == "mpz"
    assert document.report.stream_path == "/Contents"
    assert document.report.offset_domain == "cfb_stream"
    for entity in document.entities:
        assert entity.source_ref.stream_path == "/Contents"
        assert entity.source_ref.offset_domain == "cfb_stream"


@pytest.mark.parametrize("operation", [raw.inspect, raw.scan_records, ez.read])
def test_extracted_contents_are_not_promoted_to_native_mpx_input(operation, tmp_path):
    contents = CONTENTS.read_bytes()
    with pytest.raises(ez.UnsupportedFormatError):
        operation(contents)
    path = tmp_path / "misleading.mpx"
    path.write_bytes(contents)
    with pytest.raises(ez.UnsupportedFormatError):
        raw.inspectfile(path)


@pytest.mark.parametrize("operation", [raw.inspect, raw.scan_records, ez.read])
def test_whole_file_limit_precedes_input_copy_or_parse(operation):
    data = MPX.read_bytes()
    with pytest.raises(ez.LimitExceededError) as caught:
        operation(data, limits=ez.ParseLimits(max_file_size=len(data) - 1))
    assert caught.value.resource == "file_size"
    assert caught.value.limit == len(data) - 1 and caught.value.actual == len(data)
    assert not hasattr(caught.value, "report")


def test_flat_input_has_no_cfb_metadata_or_stream_budgets():
    limits = ez.ParseLimits(
        max_file_size=MPX.stat().st_size,
        max_cfb_entries=0,
        max_stream_size=0,
        max_total_stream_size=0,
        max_cfb_metadata_size=0,
        max_cfb_directory_slots=0,
        max_cfb_depth=0,
    )
    scan = raw.scan_records(MPX.read_bytes(), limits=limits)
    assert scan.format == "mpx" and scan.header.record_count == 3
    with pytest.raises(ez.LimitExceededError) as caught:
        raw.scan_records(MPX.read_bytes(), limits=ez.ParseLimits(max_records=2))
    assert caught.value.resource == "records"


@pytest.mark.parametrize("marker", [5, 8, 24, 26, 65535])
def test_flat_carrier_does_not_guess_other_marker_profiles(marker):
    data = bytearray(MPX.read_bytes())
    struct.pack_into("<H", data, 34, marker)
    with pytest.raises(ez.UnsupportedFormatError):
        raw.scan_records(bytes(data))


@pytest.mark.parametrize(
    "mutation", ["record_end", "record_count", "terminal", "truncated_tables"]
)
def test_known_structural_corruption_remains_fatal_in_partial_reading(mutation):
    data = bytearray(MPX.read_bytes())
    if mutation == "record_end":
        struct.pack_into("<I", data, 0x28, 8192 + 2 * 64)
    elif mutation == "record_count":
        struct.pack_into("<I", data, 0x58, 3)
    elif mutation == "terminal":
        struct.pack_into("<H", data, 8192 + 128, 0x1234)
    else:
        data = data[: struct.unpack_from("<I", data, 0x40)[0] - 1]
    with pytest.raises(ez.InvalidFormatError) as caught:
        ez.read(bytes(data), policy="partial")
    assert not hasattr(caught.value, "report")


def test_mixed_synthetic_payload_is_retained_without_native_qualification_claim():
    data = (FIXTURES / "mpx-mixed25.mpx").read_bytes()
    scan = raw.scan_records(data)
    assert scan.format == "mpx"
    assert scan.read_contents() == data
    assert [span.tag_raw for span in scan.object_spans()] == [
        0x33,
        2,
        3,
        4,
        5,
        5,
        3,
        2,
        0x33,
    ]
    # Only parsing/provenance is asserted here; native geometry qualification is
    # a separate oracle over controlled native MPX originals.
    for span in scan.object_spans():
        at = span.payload_offset
        assert scan.record_bytes(span.record_index) == data[at : at + 64]


def test_cli_inspect_scan_extract_use_actual_file_offsets_and_exit_policy(tmp_path):
    path = tmp_path / "no-extension"
    path.write_bytes(MPX.read_bytes())
    result = cli("inspect", path, "--json")
    assert result.returncode == 0 and result.stderr == ""
    row = json.loads(result.stdout)
    assert row["format"] == "mpx" and row["cfb"] is None
    assert row["payload"]["offset_domain"] == "file"
    result = cli("inspect", path)
    assert result.returncode == 0 and "MPX file payload:" in result.stdout
    assert "Contents:" not in result.stdout
    result = cli("scan", path, "--json", "--records")
    assert result.returncode == 0 and result.stderr == ""
    row = json.loads(result.stdout)
    assert row["format"] == "mpx" and row["records"][0]["payload_offset"] == 8192
    result = cli("extract", path, "--scope", "stored_xy_lines")
    assert result.returncode == 0 and result.stderr == ""
    row = json.loads(result.stdout)
    assert row["report"]["offset_domain"] == "file"
    assert len(row["entities"]) == 2
    result = cli("extract", path)
    assert result.returncode == 2
    assert json.loads(result.stderr)["report"]["source_format"] == "mpx"
    result = cli("inspect", path, "--json", "--max-file-size", "1")
    assert result.returncode == 3
    assert json.loads(result.stderr)["resource"] == "file_size"


def test_cross_language_domain_mismatch_is_never_silently_accepted():
    document = ez.readfile(MPX, policy="partial")
    row = _core.read_file(str(MPX), document.options._values()).report()
    for fields in [
        {"offset_domain": "cfb_stream"},
        {"source_format": "mpz"},
        {"stream_path": "/Contents"},
    ]:
        changed = {**copy.deepcopy(row), **fields}
        with pytest.raises(ValueError):
            _report(changed)
    for stream_path, domain in [
        (None, "cfb_stream"),
        ("/Contents", "file"),
        (None, "unknown"),
    ]:
        with pytest.raises(ValueError):
            SourceRef(stream_path, "root_records", None, 0, 8192, 64, domain)
        with pytest.raises(ValueError):
            OpaqueRegion(stream_path, 0, 8192, domain)


@pytest.mark.parametrize("format", ["mpz", "mpx"])
def test_container_bridge_rejects_a_payload_from_the_other_offset_domain(format):
    path = MPZ if format == "mpz" else MPX
    row = _core.inspect_file(str(path), ez.ParseLimits()._values()).metadata()
    row["payload"]["offset_domain"] = "file" if format == "mpz" else "cfb_stream"
    row["payload"]["stream_path"] = None if format == "mpz" else "/Contents"

    class Handle:
        def metadata(self):
            return row

    with pytest.raises(ValueError):
        raw._container(Handle())


def test_definition_and_style_sources_keep_their_original_mpx_file_offsets():
    path = FIXTURES / "mpx-block25.mpx"
    data = path.read_bytes()
    document = ez.read(data, scope="stored_xy_lines")
    assert len(document.entities) == 1
    entity = document.entities[0]
    assert entity.start == (105.0, -47.5) and entity.end == (83.75, -40.5)
    source = entity.source_ref
    assert (
        source.table_domain == "block_definition_records"
        and source.definition_index == 0
    )
    assert source.stream_path is None and source.offset_domain == "file"
    assert (
        source.record_index == 0
        and source.payload_offset > document.raw().header.record_end
    )
    assert (
        entity.source.header_bytes
        == data[source.payload_offset : source.payload_offset + 16]
    )
    for resolved in (entity.styles.color, entity.styles.line_type, entity.styles.width):
        for evidence in resolved.sources:
            assert (
                evidence.raw_bytes
                == data[
                    evidence.payload_offset : evidence.payload_offset
                    + len(evidence.raw_bytes)
                ]
            )
    assert entity.units is None and not document.report.document_complete
