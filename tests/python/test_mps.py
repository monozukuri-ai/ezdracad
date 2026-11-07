"""Synthetic MPS wrapper and Python boundary checks; no native qualification."""

import copy
import hashlib
import json
import struct
import subprocess
import sys
import zlib
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest
from test_scene import assembler

import ezdracad as ez
from ezdracad import _core, raw
from ezdracad.document import OpaqueRegion, SourceRef, _report

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
PAYLOAD = (FIXTURES / "document25.bin").read_bytes()


def envelope(payload=PAYLOAD, *, prefix=None, compressed=None):
    """Author a known single-member wrapper in a separately assembled CFB.

    Level zero keeps this small synthetic fixture in the regular-stream domain;
    it still exercises zlib framing, Adler32 and exact consumed-byte validation.
    """
    if prefix is None:
        prefix = struct.pack("<3I", 1, 1, 1) + bytes(110)
    if compressed is None:
        compressed = zlib.compress(payload, level=0)
    stream = prefix + struct.pack("<2I", len(payload), len(compressed)) + compressed
    carrier = bytearray(assembler.wrap_record_contents(stream))
    carrier[640:768] = assembler.directory_entry(
        "SecureContents", 2, sector=2, size=len(stream)
    )
    return bytes(carrier), stream


def cli(command, path, *options):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", command, str(path), *options],
        capture_output=True,
        text=True,
        check=False,
    )


def test_mps_owned_decoded_bytes_have_a_separate_offset_domain(tmp_path):
    data, stream = envelope()
    path = tmp_path / "図面.native"
    path.write_bytes(data)
    info = raw.inspectfile(path)
    assert info == raw.inspect(data)
    assert info.format == "mps" and info.cfb is not None
    assert info.file_size == len(data)
    assert info.payload.stream_path is None
    assert info.payload.offset_domain == "decoded_payload"
    assert info.payload.size == len(PAYLOAD)
    assert info.payload.sha256 == hashlib.sha256(PAYLOAD).hexdigest()
    assert info.read_contents() == info.read_payload() == PAYLOAD
    path.write_bytes(b"changed after inspection")
    path.unlink()
    assert info.read_payload() == PAYLOAD
    row = info.to_dict()
    assert row["payload"]["offset_domain"] == "decoded_payload"
    assert row["units"] is None and row["coordinate_system"] is None
    assert row["geometry_status"] == "not_decoded"
    assert json.loads(json.dumps(row, allow_nan=False)) == row
    with pytest.raises(FrozenInstanceError):
        info.payload.offset_domain = "cfb_stream"
    assert len(stream) != len(PAYLOAD)
    assert isinstance(info.mps, raw.MpsInfo)
    assert info.mps.stream_path == "/SecureContents"
    assert info.mps.stream_size == len(stream)
    assert info.mps.stream_sha256 == hashlib.sha256(stream).hexdigest()
    assert info.mps.decoded_size == len(PAYLOAD)
    assert info.mps.wrapper_profile == "unprotected_base_v1"
    assert info.mps.compression == "zlib"
    assert len(info.mps.chunks) == 1
    chunk = info.mps.chunks[0]
    assert (chunk.encoded_offset, chunk.encoded_size) == (130, len(stream) - 130)
    assert (chunk.decoded_offset, chunk.decoded_size) == (0, len(PAYLOAD))
    assert row["mps"]["stream_sha256"] == info.mps.stream_sha256
    assert isinstance(row["mps"]["chunks"], list)
    with pytest.raises(FrozenInstanceError):
        chunk.encoded_offset = 0


def test_mps_scanning_and_reading_preserve_decoded_offsets_and_hash(tmp_path):
    data, _ = envelope()
    path = tmp_path / "data"
    path.write_bytes(data)
    scan = raw.scanfile(path)
    assert scan == raw.scan_records(data)
    assert scan.format == "mps" and scan.offset_domain == "decoded_payload"
    assert scan.stream_path is None and scan.file_size == len(data)
    assert scan.payload_size == len(PAYLOAD)
    assert scan.mps == raw.inspect(data).mps
    assert scan.to_dict()["mps"] == raw.inspect(data).to_dict()["mps"]
    assert scan.read_contents() == PAYLOAD
    assert scan.sha256 == hashlib.sha256(PAYLOAD).hexdigest()
    assert scan.header.payload_end_basis == "declared_field"
    assert scan.header.payload_end + 4 == len(PAYLOAD)
    assert scan.record_bytes(0) == PAYLOAD[8192:8256]
    doc = ez.readfile(path, scope="stored_xy_lines")
    path.unlink()
    assert doc.report.source_format == "mps"
    assert doc.report.offset_domain == "decoded_payload"
    assert doc.report.stream_path is None
    assert doc.report.payload_sha256 == doc.raw().sha256 == scan.sha256
    assert len(doc.entities) == 2 and doc.report.document_complete is False
    for entity in doc.entities:
        source = entity.source_ref
        assert source.offset_domain == "decoded_payload" and source.stream_path is None
        assert (
            entity.source.header_bytes
            == PAYLOAD[source.payload_offset : source.payload_offset + 16]
        )
    assert all(
        region.offset_domain == "decoded_payload" and region.stream_path is None
        for region in doc.report.opaque_regions
    )
    assert doc.raw().read_payload() == PAYLOAD
    assert doc.to_dict()["report"]["source_format"] == "mps"
    with pytest.raises(ez.IncompleteReadError) as caught:
        ez.read(data)
    assert caught.value.report.source_format == "mps"
    assert caught.value.report.offset_domain == "decoded_payload"


@pytest.mark.parametrize("call", [raw.inspect, raw.scan_records, ez.read])
def test_mps_declared_decompressed_limit_crosses_all_ffi_paths(call):
    data, _ = envelope()
    with pytest.raises(ez.LimitExceededError) as caught:
        call(data, limits=ez.ParseLimits(max_decompressed_size=len(PAYLOAD) - 1))
    assert (caught.value.resource, caught.value.limit, caught.value.actual) == (
        "decompressed_size",
        len(PAYLOAD) - 1,
        len(PAYLOAD),
    )
    assert raw.inspect(data, limits=ez.ParseLimits(max_decompressed_size=len(PAYLOAD)))


def test_mps_decompressed_limit_defaults_and_validation():
    assert ez.ParseLimits().max_decompressed_size == 128 * 1024 * 1024
    assert ez.ParseLimits()._values()[-1] == _core.DEFAULT_MAX_DECOMPRESSED_SIZE
    for value, error in [(-1, ValueError), (1 << 64, ValueError), (True, TypeError)]:
        with pytest.raises(error):
            ez.ParseLimits(max_decompressed_size=value)
    # The new limit concerns decoded MPS bytes; existing carriers do not inflate.
    assert (
        raw.inspect(
            (FIXTURES / "document25.cfb").read_bytes(),
            limits=ez.ParseLimits(max_decompressed_size=0),
        ).format
        == "mpz"
    )
    assert (
        raw.inspect(
            (FIXTURES / "mpx25.mpx").read_bytes(),
            limits=ez.ParseLimits(max_decompressed_size=0),
        ).format
        == "mpx"
    )


@pytest.mark.parametrize("call", [raw.inspect, raw.scan_records, ez.read])
def test_known_mps_checksum_error_is_never_a_partial_success(call):
    compressed = bytearray(zlib.compress(PAYLOAD, level=0))
    compressed[-1] ^= 1
    data, _ = envelope(compressed=bytes(compressed))
    with pytest.raises(ez.InvalidFormatError):
        call(data)


def test_mps_unknown_wrapper_stays_unsupported_without_payload():
    prefix = bytearray(struct.pack("<3I", 1, 1, 1) + bytes(110))
    prefix[16] = 0x80
    data, _ = envelope(prefix=bytes(prefix))
    info = raw.inspect(data)
    assert info.format == "mps_candidate" and info.payload is None
    with pytest.raises(ez.UnsupportedFormatError):
        info.read_payload()
    for call in (raw.scan_records, ez.read):
        with pytest.raises(ez.UnsupportedFormatError):
            call(data)


def test_mps_cli_metadata_extraction_limit_and_unsupported(tmp_path):
    data, _ = envelope()
    path = tmp_path / "input"
    path.write_bytes(data)
    result = cli("inspect", path, "--json")
    assert result.returncode == 0 and not result.stderr
    row = json.loads(result.stdout)
    assert row["format"] == "mps"
    assert row["payload"]["offset_domain"] == "decoded_payload"
    result = cli("inspect", path)
    assert result.returncode == 0 and "Decoded MPS payload:" in result.stdout
    result = cli("scan", path, "--json", "--records")
    assert result.returncode == 0 and json.loads(result.stdout)["format"] == "mps"
    result = cli("extract", path, "--scope", "stored_xy_lines")
    assert result.returncode == 0
    assert json.loads(result.stdout)["report"]["offset_domain"] == "decoded_payload"
    result = cli("inspect", path, "--json", "--max-decompressed-size", "1")
    assert result.returncode == 3
    assert json.loads(result.stderr)["resource"] == "decompressed_size"


def test_mps_bridge_rejects_wrong_or_unknown_domain():
    data, _ = envelope()
    doc = ez.read(data, policy="partial")
    row = _core.read_bytes(data, doc.options._values()).report()
    for change in (
        {"offset_domain": "file"},
        {"offset_domain": "cfb_stream", "stream_path": "/SecureContents"},
        {"source_format": "mpx"},
        {"stream_path": "/SecureContents"},
    ):
        with pytest.raises(ValueError):
            _report({**copy.deepcopy(row), **change})
    for kind, args in (
        (SourceRef, ("/SecureContents", "root_records", None, 0, 8192, 64)),
        (OpaqueRegion, ("/SecureContents", 0, 8192)),
    ):
        with pytest.raises(ValueError):
            kind(*args, offset_domain="decoded_payload")


@pytest.mark.parametrize(
    "fixture_name", ["mps-document25", "mps-basic2d25", "mps-block25"]
)
def test_packaged_synthetic_mps_carriers_use_the_existing_decoders(fixture_name):
    data = (FIXTURES / (fixture_name + ".mps")).read_bytes()
    info = raw.inspect(data)
    options = dict(scope="stored_xy_lines")
    if fixture_name == "mps-basic2d25":
        options = dict(
            scope="stored_xy_primitives",
            basic_profile=ez.Basic2DProfile.MARKER25_ROOT_PRIMITIVES_V1,
        )
    doc = ez.read(data, **options)
    assert doc.raw().mps == info.mps
    assert doc.raw().read_payload() == info.read_payload()
    assert doc.report.source_format == "mps"
    assert doc.report.offset_domain == "decoded_payload"
    assert doc.report.payload_sha256 == info.payload.sha256
    assert doc.report.document_complete is False
    if fixture_name == "mps-basic2d25":
        assert {entity.type for entity in doc.entities} == {
            "LINE",
            "CIRCLE",
            "ARC",
            "TEXT",
        }
    if fixture_name == "mps-block25":
        assert len(doc.entities) == 1
        entity = doc.entities[0]
        source = entity.source_ref
        assert source.table_domain == "block_definition_records"
        assert (
            source.definition_index == 0 and source.offset_domain == "decoded_payload"
        )
        assert source.stream_path is None
        decoded = info.read_payload()
        assert (
            entity.source.header_bytes
            == decoded[source.payload_offset : source.payload_offset + 16]
        )
        for resolved in (
            entity.styles.color,
            entity.styles.line_type,
            entity.styles.width,
        ):
            for evidence in resolved.sources:
                assert (
                    evidence.raw_bytes
                    == decoded[
                        evidence.payload_offset : evidence.payload_offset
                        + len(evidence.raw_bytes)
                    ]
                )


def test_mps_metadata_bridge_rejects_missing_or_mismatched_provenance():
    data, _ = envelope()
    source = _core.inspect_bytes(data, ez.ParseLimits()._values()).metadata()
    for key, value in (
        ("offset_domain", "file"),
        ("stream_path", "/SecureContents"),
    ):
        row = copy.deepcopy(source)
        row["payload"][key] = value

        class Handle:
            def metadata(self):
                return row

        with pytest.raises(ValueError):
            raw._container(Handle())
    for key, value in (
        ("stream_path", "/Other"),
        ("stream_size", 0),
        ("decoded_size", 0),
        ("compression", "unknown"),
        ("wrapper_profile", "guessed"),
        ("chunks", []),
    ):
        row = copy.deepcopy(source)
        row["mps"][key] = value

        class Handle:
            def metadata(self):
                return row

        with pytest.raises(ValueError):
            raw._container(Handle())


@pytest.mark.parametrize("source_kind", ["opaque_regions", "diagnostics"])
def test_report_source_ranges_cannot_silently_claim_another_domain(source_kind):
    data, _ = envelope()
    doc = ez.read(data, policy="partial")
    row = _core.read_bytes(data, doc.options._values()).report()
    if source_kind == "opaque_regions":
        row["opaque_regions"][0]["offset_domain"] = "file"
    else:
        row["diagnostics"].append(
            {
                "code": "synthetic",
                "message": "forged source domain",
                "source": {
                    "stream_path": None,
                    "offset_domain": "file",
                    "table_domain": "root_records",
                    "definition_index": None,
                    "record_index": 0,
                    "payload_offset": 8192,
                    "byte_length": 64,
                },
            }
        )
    with pytest.raises(ValueError, match="Document source ranges"):
        _report(row)


@pytest.mark.parametrize(
    "field,value",
    [
        ("encoded_offset", -1),
        ("encoded_size", -1),
        ("encoded_size", 1 << 64),
        ("decoded_offset", 1),
        ("decoded_size", -1),
    ],
)
def test_mps_chunk_metadata_bridge_rejects_inconsistent_ranges(field, value):
    data, _ = envelope()
    row = _core.inspect_bytes(data, ez.ParseLimits()._values()).metadata()
    row["mps"]["chunks"][0][field] = value

    class Handle:
        def metadata(self):
            return row

    with pytest.raises(ValueError):
        raw._container(Handle())
