"""Reader policies and ownership, independent of native decoder qualification."""

import importlib.util
import json
import struct
import subprocess
import sys
import uuid
from dataclasses import FrozenInstanceError, fields
from pathlib import Path

import pytest
from test_scene import fixture, inline, line, reference, slot

import ezdracad as ez
from ezdracad import _core, raw
from ezdracad.document import ReadLimits, ReadOptions, ReadPolicy, ReadScope, ReadStatus
from ezdracad.qualification import Line2D, Line2DProfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "reader_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)
MPZ = ROOT / "tests/fixtures/document25.cfb"
PARTIAL = ROOT / "tests/fixtures/document-partial25.cfb"


def read_payload(payload, **options):
    return ez.read(assembler.wrap_record_contents(payload), policy="partial", **options)


def test_default_strict_and_explicit_stored_scope_are_distinct_from_complete_drawing():
    with pytest.raises(ez.IncompleteReadError) as caught:
        ez.read(MPZ.read_bytes())
    report = caught.value.report
    assert isinstance(caught.value, ez.UnsupportedFeatureError)
    assert isinstance(report, ez.ReadReport)
    assert (
        report.status == ReadStatus.PARTIAL
        and report.geometry_status == ReadStatus.COMPLETE
    )
    assert report.entity_count == 2 and report.scope == ReadScope.DRAWING
    assert report.document_complete is False and report.units is None
    assert not hasattr(caught.value, "_read_report")
    doc = ez.readfile(MPZ, scope="stored_xy_lines")
    assert doc.report.status == ReadStatus.COMPLETE
    assert doc.report.document_status == ReadStatus.PARTIAL
    assert doc.report.document_complete is False and len(doc.entities) == 2
    assert (
        doc.report.unresolved_semantics
        == ez.readfile(MPZ, policy="partial").report.unresolved_semantics
    )


def test_owned_snapshot_queries_preserve_all_p3_geometry_and_evidence(tmp_path):
    path = tmp_path / "図面.native"
    data = MPZ.read_bytes()
    path.write_bytes(data)
    doc = ez.readfile(path, policy=ReadPolicy.PARTIAL)
    snapshot = raw.scan_records(data)
    path.write_bytes(b"replaced")
    path.unlink()
    query = doc.query("LINE")
    assert isinstance(query, ez.EntityQuery) and len(query) == 2
    assert query[0].start == (-12.5, 0.25) and query[-1].end == (17.125, 8.5)
    assert tuple(query) == query[:] and query[::-1] == (query[1], query[0])
    assert query[99:100] == () and query[1:0:-1] == (query[1],)
    assert query.page(2) == () and query.page(0, 0) == ()
    for index, entity in enumerate(query):
        old = snapshot.qualified_lines(
            index, profile=Line2DProfile.MARKER25_PLANAR_STORED_XY_V1
        ).lines[0]
        for item in fields(Line2D):
            assert getattr(entity, item.name) == getattr(old, item.name)
        assert (
            entity.type == "LINE"
            and entity.entity_id == entity.root_record_index == index
        )
        assert entity.source_ref.payload_offset == 8192 + index * 64
        assert (
            entity.source_ref.byte_length == 64
            and entity.source_ref.offset_domain == "cfb_stream"
        )
        assert entity.source.header_bytes == doc.raw().record_bytes(index)[:16]
    assert doc.raw().read_contents() == snapshot.read_contents()
    assert doc.raw().sha256 == doc.report.contents_sha256
    with pytest.raises(FrozenInstanceError):
        doc.report = None
    with pytest.raises(FrozenInstanceError):
        query[0].entity_id = 2
    with pytest.raises(TypeError):
        doc.report.unsupported_counts[0] = 5
    with pytest.raises(IndexError):
        query[2]
    with pytest.raises(ez.UnsupportedFeatureError):
        doc.query("CIRCLE")
    with pytest.raises(ez.LimitExceededError):
        query.page(0, 4097)
    json.dumps(doc.to_dict(), allow_nan=False)
    assert "entities" not in doc.to_dict(include_entities=False)


def test_unknown_state_does_not_silently_disappear_or_normalize_later_lines():
    doc = ez.readfile(PARTIAL, policy="partial")
    assert len(doc.entities) == 1
    assert dict(doc.report.unsupported_counts) == {0x7331: 1}
    assert dict(doc.report.context_blocked_counts) == {2: 1}
    assert doc.report.primary_object_count == 3 and doc.report.decoded_object_count == 1
    errors = [d for d in doc.report.diagnostics if d.source]
    assert [(d.code, d.source.record_index) for d in errors] == [
        ("unsupported_object", 1),
        ("context_blocked", 2),
    ]
    assert doc.raw().record_bytes(1)[:2] == bytes.fromhex("3173")
    with pytest.raises(ez.UnsupportedFeatureError) as caught:
        ez.readfile(PARTIAL, scope="stored_xy_lines")
    assert caught.value.report.unsupported_counts == doc.report.unsupported_counts


def test_an_unsupported_subtree_is_atomic_and_never_leaks_a_supported_child():
    payload = fixture([line(), inline(3), line(), slot(0x7331), line()])
    doc = read_payload(payload)
    assert len(doc.entities) == 1
    assert doc.report.primary_object_count == 3
    assert doc.report.interior_record_count == 2
    assert doc.report.auxiliary_record_count == 0
    assert dict(doc.report.unsupported_counts) == {0x30: 1}
    assert dict(doc.report.context_blocked_counts) == {2: 1}
    assert doc.raw().read_contents() == payload


@pytest.mark.parametrize("tag", [2, 0x402, 0x2402, 0x4402, 0x13])
@pytest.mark.parametrize("field", [16, 48, 56])
def test_nonfinite_known_fields_after_unknown_state_are_fatal(tag, field):
    leaf = line(tag)
    struct.pack_into("<d", leaf, field, float("nan"))
    with pytest.raises(ez.InvalidFormatError) as caught:
        read_payload(fixture([line(), slot(0x7331), leaf]))
    assert not hasattr(caught.value, "report")


@pytest.mark.parametrize(
    "mutation",
    [
        "orphan",
        "zero_span",
        "crossing",
        "nested_crossing",
        "missing_FE",
        "unused_definition_NaN",
        "unused_definition_span",
    ],
)
def test_structural_corruption_is_not_downgraded_to_partial(mutation):
    records = [line(), slot(0x7331), line()]
    definitions = []
    if mutation == "orphan":
        records[-1] = slot(0xFE)
    elif mutation == "zero_span":
        struct.pack_into("<H", records[-1], 8, 0)
    elif mutation == "crossing":
        struct.pack_into("<H", records[-1], 8, 2)
    elif mutation == "nested_crossing":
        records = [line(), slot(0x7331), inline(2), inline(2)]
    elif mutation == "missing_FE":
        records = [line(), slot(0x7331), *reference(b"MISSING")]
        records[-1][:2] = b"\x02\x00"
    elif mutation == "unused_definition_NaN":
        leaf = line(0x402)
        struct.pack_into("<d", leaf, 48, float("nan"))
        definitions = [(b"UNUSED", [leaf])]
    else:
        leaf = line()
        struct.pack_into("<H", leaf, 8, 2)
        definitions = [(b"UNUSED", [leaf])]
    with pytest.raises(ez.InvalidFormatError):
        read_payload(fixture(records, definitions))


def test_repeated_instances_keep_distinct_roots_and_exact_definition_sources():
    refs = reference(b"A", placement=(100, 50, 0, 2, 2, 90)) + reference(
        b"A", placement=(-10, 20, 0, 1, 1, 0)
    )
    payload = fixture(refs, [(b"A", [line()])])
    doc = read_payload(payload)
    rows = tuple(doc.query())
    assert len(rows) == 2 and [e.entity_id for e in rows] == [0, 1]
    assert [e.root_record_index for e in rows] == [0, 2]
    assert rows[0].start == (96, 52) and rows[1].start == (-9, 22)
    assert rows[0].source_ref == rows[1].source_ref
    assert rows[0].source_ref.table_domain == "block_definition_records"
    assert rows[0].ancestors[0].raw_bytes != rows[1].ancestors[0].raw_bytes
    assert doc.report.scene_nodes_visited == 4


@pytest.mark.parametrize(
    "name,value,resource",
    [
        ("max_objects", 1, "document_objects"),
        ("max_entities", 1, "document_entities"),
        ("max_nodes", 1, "document_nodes"),
        ("max_provenance_bytes", 0, "document_provenance_bytes"),
    ],
)
def test_aggregate_limits_are_fatal(name, value, resource):
    with pytest.raises(ez.LimitExceededError) as caught:
        ez.readfile(MPZ, policy="partial", read_limits=ReadLimits(**{name: value}))
    assert caught.value.resource == resource and not hasattr(caught.value, "report")


def test_diagnostic_budget_reports_omitted_counts_and_never_changes_coverage():
    base = ez.readfile(PARTIAL, policy="partial")
    limited = ez.readfile(
        PARTIAL, policy="partial", read_limits=ReadLimits(max_diagnostics=0)
    )
    assert limited.report.diagnostics == ()
    assert limited.report.diagnostics_suppressed == len(base.report.diagnostics)
    assert limited.report.unsupported_counts == base.report.unsupported_counts
    assert tuple(limited.entities) == tuple(base.entities)


@pytest.mark.parametrize("marker", [5, 8])
@pytest.mark.parametrize("empty", [False, True])
def test_marker25_profile_is_not_inferred_from_empty_or_old_tables(marker, empty):
    payload = bytearray(fixture([] if empty else [line()]))
    struct.pack_into("<H", payload, 34, marker)
    if marker == 5:
        with pytest.raises(
            ez.UnsupportedFeatureError, match="post-record table layout"
        ):
            read_payload(payload)
        return
    doc = read_payload(payload)
    assert len(doc.entities) == 0 and doc.report.status == ReadStatus.PARTIAL
    with pytest.raises(ez.UnsupportedFeatureError):
        ez.read(assembler.wrap_record_contents(payload), scope="stored_xy_lines")


def test_options_validate_inputs_without_guessing_or_silently_overriding(tmp_path):
    for data in (bytearray(), str(MPZ), MPZ):
        with pytest.raises(TypeError):
            ez.read(data)
    with pytest.raises(TypeError):
        ez.readfile(bytes(MPZ))
    with pytest.raises(ez.DracadIoError):
        ez.readfile(tmp_path / "missing")
    with pytest.raises(ez.DracadIoError):
        ez.readfile(tmp_path)
    with pytest.raises(ValueError):
        ReadOptions(policy="ignore")
    with pytest.raises(ValueError):
        ReadOptions(scope="WCS")
    with pytest.raises(ValueError):
        ReadLimits(max_entities=-1)
    with pytest.raises(TypeError):
        ReadLimits(max_nodes=True)
    with pytest.raises(TypeError):
        ReadOptions(parse_limits={})
    with pytest.raises(ValueError):
        ez.readfile(MPZ, options=ReadOptions(), policy="partial")
    doc = ez.readfile(MPZ, options=ReadOptions(policy="partial"))
    assert len(doc.entities) == 2
    row = _core.read_file(str(MPZ), doc.options._values()).report()
    assert ez.document._report(row) == doc.report


def cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", "extract", *map(str, args)],
        capture_output=True,
        text=True,
    )


def large_envelope(contents):
    """Independent regular-stream CFB with several FAT sectors, for paging."""
    sectors = (len(contents) + 511) // 512
    fats = 1
    while sectors + fats + 1 > 128 * fats:
        fats += 1
    assert fats <= 109 and len(contents) >= 4096
    first = fats + 1
    header = bytearray(512)
    header[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    struct.pack_into("<HHHHH", header, 24, 0x3E, 3, 0xFFFE, 9, 6)
    struct.pack_into(
        "<9I", header, 40, 0, fats, 0, 0, 4096, assembler.END, 0, assembler.END, 0
    )
    struct.pack_into(
        "<109I", header, 76, *range(1, first), *([assembler.FREE] * (109 - fats))
    )
    root = assembler.directory_entry("Root Entry", 5, child=1, sector=assembler.END)
    root[80:96] = uuid.UUID("0d4c9b60-7843-11cf-1234-56789abcdeff").bytes_le
    directory = (
        root
        + assembler.directory_entry("Contents", 2, sector=first, size=len(contents))
        + bytes(256)
    )
    entries = (
        [assembler.END]
        + [assembler.FAT] * fats
        + list(range(first + 1, first + sectors))
        + [assembler.END]
    )
    fat = struct.pack(
        f"<{128 * fats}I", *entries, *([assembler.FREE] * (128 * fats - len(entries)))
    )
    return bytes(header) + directory + fat + contents.ljust(sectors * 512, b"\0")


def test_extract_cli_status_json_error_report_and_exit_codes(tmp_path):
    result = cli(MPZ, "--entities", "LINE", "--partial")
    assert result.returncode == 2 and not result.stderr
    data = json.loads(result.stdout)
    assert len(data["entities"]) == 2
    assert data["report"]["document_complete"] is False
    assert "header_bytes" not in result.stdout and "raw_bytes" not in result.stdout
    result = cli(MPZ, "--scope", "stored_xy_lines")
    assert (
        result.returncode == 0
        and json.loads(result.stdout)["report"]["status"] == "complete"
    )
    result = cli(MPZ)
    assert result.returncode == 2 and not result.stdout
    assert json.loads(result.stderr)["report"]["geometry_status"] == "complete"
    result = cli(PARTIAL, "--policy", "partial", "--max-diagnostics", 0)
    assert (
        result.returncode == 2
        and json.loads(result.stdout)["report"]["diagnostics_suppressed"] > 0
    )
    result = cli(MPZ, "--partial", "--max-entities", 1)
    assert (
        result.returncode == 3
        and json.loads(result.stderr)["resource"] == "document_entities"
    )
    bad = tmp_path / "bad.mpz"
    bad.write_bytes(b"broken")
    for path in (bad, tmp_path / "missing"):
        result = cli(path, "--partial")
        assert result.returncode == 1 and not result.stdout
    result = cli(MPZ, "--entities", "CIRCLE")
    assert result.returncode == 2 and not result.stdout


def test_more_than_one_page_uses_shared_tables_and_keeps_stable_ordinals():
    doc = ez.read(large_envelope(fixture([line()] * 5000)), policy="partial")
    query = doc.query()
    assert len(query) == doc.report.entity_count == 5000
    assert doc.report.scene_nodes_visited == 5000
    assert query[4096].entity_id == 4096 and query[-1].entity_id == 4999
    assert [entity.entity_id for entity in query.page(4094, 4)] == list(
        range(4094, 4098)
    )
    assert [entity.entity_id for entity in query[4999:4995:-2]] == [4999, 4997]
    assert (
        tuple(getattr(ReadLimits(), f.name) for f in fields(ReadLimits))
        == _core.read_limit_defaults()
    )


def test_old_profile_unknown_3d_fields_are_opaque_instead_of_guessed_numbers():
    unknown = line(0x13)
    struct.pack_into("<d", unknown, 16, float("nan"))
    data = bytearray(fixture([unknown]))
    struct.pack_into("<H", data, 34, 8)
    doc = read_payload(data)
    assert len(doc.entities) == 0
    assert doc.report.status == ReadStatus.PARTIAL
    assert doc.raw().read_contents() == data
