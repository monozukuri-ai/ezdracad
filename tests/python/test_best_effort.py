"""Best-effort reading over synthetic fixtures; never native evidence."""

import json
import subprocess
import sys
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

import ezdracad as ez
from ezdracad import best_effort

FIXTURES = Path(__file__).resolve().parents[2] / "tests/fixtures"
MPZ = FIXTURES / "best-effort8.cfb"
PAYLOAD = FIXTURES / "best-effort8.bin"


def cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", "best-effort", *map(str, args)],
        capture_output=True,
        text=True,
        check=False,
    )


@pytest.fixture(scope="module")
def drawing():
    return best_effort.readfile(MPZ)


def test_every_decoded_kind_keeps_its_stored_values(drawing):
    assert (drawing.format, drawing.marker, drawing.marker_observed) == ("mpz", 8, True)
    assert drawing.payload_size == PAYLOAD.stat().st_size
    assert [entity.type for entity in drawing.entities] == [
        "LINE",
        "CIRCLE",
        "ARC",
        "ELLIPSE",
        "ELLIPTICAL_ARC",
        "POINT",
        "TEXT",
        "TEXT",
        "POLYLINE",
        "POLYLINE",
        "EXTENDED_POLYGON",
        "HATCH",
        "DIMENSION",
        "PACKAGE",
        "INSERT",
        "LINE",
    ]
    first = drawing.entities[0]
    assert first.data == {
        "start": [1.0, 2.0],
        "end": [3.0, 4.0],
        "elevation": 0.0,
        "thickness": 0.0,
    }
    assert (first.layer, first.color, first.line_type, first.width) == (3, 7, 2, 4)
    assert (first.table, first.block_index, first.record_index) == ("root", None, 2)
    assert first.payload_offset == 8192 + 2 * 64 and first.record_count == 1
    assert not first.hidden and first.children == ()
    by_type = {entity.type: entity for entity in drawing.entities}
    assert by_type["CIRCLE"].data["center"] == [5.0, 6.0]
    assert by_type["CIRCLE"].data["radius"] == 7.0
    arc = by_type["ARC"].data
    assert (arc["radius"], arc["start_degrees"], arc["end_degrees"]) == (
        2.0,
        30.0,
        120.0,
    )
    assert by_type["ELLIPSE"].data["radius"] == [4.0, 2.0]
    partial = by_type["ELLIPTICAL_ARC"].data
    assert (partial["rotation_degrees"], partial["end_degrees"]) == (15.0, 80.0)
    assert by_type["POINT"].data == {"position": [9.0, 8.0], "point_type": 3}
    last = drawing.entities[-1]
    assert (last.color, last.line_type, last.width) == (
        best_effort.BY_LAYER_COLOR,
        best_effort.BY_LAYER_LINE_TYPE,
        best_effort.BY_BLOCK_WIDTH,
    )


def test_text_is_decoded_from_the_saved_bytes(drawing):
    plain, styled = (e.data for e in drawing.entities if e.type == "TEXT")
    assert plain["text"] == "図面 A髙\n2"
    assert plain["text_bytes"].startswith("図面 A".encode("cp932"))
    assert plain["font_name"] == "ＭＳ ゴシック"
    assert (plain["position"], plain["height"], plain["width"]) == (
        [10.0, 20.0],
        4.0,
        2.5,
    )
    assert (plain["origin"], plain["rotation_degrees"], plain["slant_degrees"]) == (
        9,
        30.0,
        5.0,
    )
    assert (plain["flags"], plain["charset"], plain["family"]) == (37, 128, 49)
    assert not plain["numeric"] and plain["number"] is None
    assert not plain["format_codes"] and not plain["lossy"]
    assert styled["text"] == "見出し\n本文" and styled["format_codes"]


def test_compound_objects_keep_their_members(drawing):
    plain, spline = (e for e in drawing.entities if e.type == "POLYLINE")
    assert [child.type for child in plain.children] == ["LINE", "LINE", "ARC"]
    assert (
        plain.data["closed"] and plain.data["filled"] and plain.data["fill_color"] == 4
    )
    assert plain.data["curve_points"] is None and plain.children[0].flags == 0x08
    assert (spline.data["curve_kind"], spline.data["divisions"]) == (3, 10)
    curve = spline.data["curve_points"]
    assert len(curve) == 3 * 10 + 1 and curve[0] == [0.0, 0.0]
    assert curve[-1] == pytest.approx([0.0, 40.0])

    region, hatch = (
        e for e in drawing.entities if e.type in ("EXTENDED_POLYGON", "HATCH")
    )
    assert region.data["region_id"] == hatch.data["region_id"] == 0x098D
    assert region.data["loop_count"] == 1 and region.data["filled"]
    assert [len(loop.children) for loop in region.children] == [4]
    assert hatch.data["hatch_type"] == 0 and hatch.data["pattern_origin"] is None
    assert hatch.data["lines"] == [
        {"origin": [0.0, 0.0], "pitch": 5.0, "angle_degrees": 45.0},
        {"origin": [0.0, 0.0], "pitch": 5.0, "angle_degrees": 135.0},
    ]

    dimension = next(e for e in drawing.entities if e.type == "DIMENSION")
    package = next(e for e in drawing.entities if e.type == "PACKAGE")
    assert dimension.data["dimension_type"] == 2
    assert dimension.data["line"] == [[0.0, 60.0], [40.0, 60.0]]
    assert dimension.data["extension_origins"] == [[0.0, 40.0], [40.0, 40.0]]
    assert dimension.data["arrow_sizes"] == [3.0, 3.0, 3.0, 3.0]
    assert dimension.data["arrow_paper_sizes"] is None
    (number,) = dimension.children
    assert number.data["numeric"] and number.data["number"] == 1234.5
    assert number.data["text"] == "40"
    assert (package.data["package_kind"], package.data["reference_number"]) == (3, 1)
    assert [child.type for child in package.children] == [
        "LINE",
        "LINE",
        "POINT",
        "ARROWHEAD",
        "ARROWHEAD",
        "TEXT",
    ]
    linked = [child.link_id for child in package.children]
    assert dimension.link_id == 104 and linked == [104, 0, 0, 0, 0, 104]
    assert package.children[3].data["arrow_type"] == 5
    assert len(list(package.walk())) == 7
    assert (
        sum(1 for _ in drawing.walk())
        == sum(drawing.report.decoded_counts.values()) - 2
    )


def test_blocks_tables_and_settings(drawing):
    reference = next(e for e in drawing.entities if e.type == "INSERT")
    assert reference.data["name"] == "SYM" and reference.data["name_bytes"] == b"SYM"
    assert reference.data["position"] == [100.0, 200.0]
    assert reference.data["scale"] == [2.0, 2.0]
    assert reference.data["rotation_degrees"] == 90.0
    block = drawing.block("SYM")
    assert block is drawing.block(b"SYM") and drawing.block("missing") is None
    assert [entity.type for entity in block.entities] == ["LINE", "CIRCLE"]
    assert (block.entities[0].table, block.entities[0].block_index) == ("block", 0)

    assert len(drawing.layers) == 257 and len(drawing.colors) == 256
    assert (len(drawing.line_types), len(drawing.widths)) == (32, 16)
    layer = drawing.layers[3]
    assert (layer.index, layer.name, layer.hidden) == (3, "寸法", False)
    assert (layer.color, layer.line_type, layer.width) == (6, 2, 4)
    assert drawing.layers[5].hidden
    assert (drawing.layers[0].index, drawing.layers[256].index) == (0, 256)
    color = drawing.colors[6]
    assert (color.index, color.rgb, color.pen_width) == (7, (10, 20, 30), 0.35)
    assert drawing.colors[255] == best_effort.Color(256, (1, 2, 3), 0.0)
    assert drawing.line_types[1].pattern == (3.0, 1.0)
    assert drawing.widths[3] == best_effort.Width(4, 0.5)
    assert drawing.settings == best_effort.Settings(
        drawing_origin=(0.0, 0.0),
        drawing_scale=50.0,
        paper_size=(420.0, 297.0),
        paper_origin=(-100.0, 14850.0),
    )


def test_report_counts_what_was_not_decoded(drawing):
    report = drawing.report
    assert report.lossy
    assert dict(report.unsupported_counts) == {0x19: 1}
    assert dict(report.failed_counts) == {4: 1}
    assert (report.property_records, report.orphan_records, report.clamped_spans) == (
        1,
        0,
        0,
    )
    assert report.decoded_counts["LINE"] == 14 and report.decoded_counts["TEXT"] == 4
    (diagnostic,) = report.diagnostics
    assert diagnostic.code == "object_layout_mismatch"
    assert (diagnostic.table, diagnostic.tag, diagnostic.record_index) == (
        "root",
        4,
        61,
    )
    assert report.diagnostics_suppressed == 0


def test_results_are_immutable_and_json_compatible(drawing):
    with pytest.raises(FrozenInstanceError):
        drawing.marker = 1
    with pytest.raises(TypeError):
        drawing.entities[0].data["start"] = None
    with pytest.raises(TypeError):
        drawing.report.decoded_counts["LINE"] = 0
    row = json.loads(json.dumps(drawing.to_dict(), allow_nan=False))
    assert row["schema_version"] == 1
    assert len(row["entities"]) == len(drawing.entities)
    assert (
        row["entities"][6]["data"]["text_bytes"]
        == drawing.entities[6].data["text_bytes"].hex()
    )
    assert row["blocks"][0]["name_bytes"] == b"SYM".hex()
    assert row["report"]["unsupported_counts"] == {"25": 1}
    assert row["settings"]["paper_size"] == [420.0, 297.0]
    summary = drawing.to_dict(include_entities=False)
    assert "entities" not in summary and "blocks" not in summary


def test_carriers_are_identified_by_content():
    flat = best_effort.read(PAYLOAD.read_bytes())
    assert flat.format == "mpx" and len(flat.entities) == 16
    assert flat.payload_sha256 == best_effort.readfile(MPZ).payload_sha256
    mps = best_effort.readfile(FIXTURES / "mps-basic2d25.mps")
    assert (mps.format, mps.marker) == ("mps", 25) and not mps.report.lossy
    assert mps.report.decoded_counts["TEXT"] == 2
    mpx = best_effort.readfile(FIXTURES / "mpx25.mpx")
    assert mpx.format == "mpx" and mpx.report.decoded_counts["LINE"] >= 1
    with pytest.raises(ez.UnsupportedFormatError):
        best_effort.readfile(FIXTURES / "minimal.cfb")
    with pytest.raises(ez.InvalidFormatError):
        best_effort.read(b"not a drawing")
    with pytest.raises(ez.DracadIoError):
        best_effort.readfile(FIXTURES / "missing.mpz")


def test_newer_generation_text_uses_unicode_escapes_and_utf16_fonts():
    doc = best_effort.readfile(FIXTURES / "basic2d25.cfb")
    assert doc.marker == 25 and not doc.report.lossy
    texts = [entity.data for entity in doc.walk() if entity.type == "TEXT"]
    assert texts[0]["text"] == "合成図面β" and texts[0]["font_name"] == "Synthetic Font"
    assert texts[1]["text"] == "A" * 95 + "Z"
    assert doc.report.property_records == 2
    strict = ez.readfile(
        FIXTURES / "basic2d25.cfb",
        policy="partial",
        basic_profile="marker25_root_primitives_v1",
    )
    assert len(strict.query("*")) == sum(doc.report.decoded_counts.values())


def test_limits_and_argument_types():
    with pytest.raises(ez.LimitExceededError) as caught:
        best_effort.readfile(
            MPZ, read_limits=best_effort.BestEffortLimits(max_entities=5)
        )
    assert (caught.value.resource, caught.value.limit) == ("best_effort_entities", 5)
    with pytest.raises(ez.LimitExceededError) as caught:
        best_effort.readfile(MPZ, read_limits=best_effort.BestEffortLimits(max_depth=0))
    assert caught.value.resource == "best_effort_depth"
    with pytest.raises(ez.LimitExceededError) as caught:
        best_effort.readfile(
            MPZ, read_limits=best_effort.BestEffortLimits(max_curve_points=30)
        )
    assert caught.value.resource == "best_effort_curve_points"
    with pytest.raises(ez.LimitExceededError) as caught:
        best_effort.readfile(MPZ, limits=ez.ParseLimits(max_records=10))
    assert caught.value.resource == "records"
    quiet = best_effort.readfile(
        MPZ, read_limits=best_effort.BestEffortLimits(max_diagnostics=0)
    )
    assert quiet.report.diagnostics == () and quiet.report.diagnostics_suppressed == 1
    for invalid in (-1, 1 << 64):
        with pytest.raises(ValueError):
            best_effort.BestEffortLimits(max_entities=invalid)
    with pytest.raises(TypeError):
        best_effort.BestEffortLimits(max_depth=True)
    with pytest.raises(TypeError):
        best_effort.read("text")
    with pytest.raises(TypeError):
        best_effort.readfile(MPZ, limits=best_effort.BestEffortLimits())
    with pytest.raises(TypeError):
        best_effort.readfile(MPZ, read_limits=ez.ParseLimits())


def test_cli_summary_json_and_exit_policy(tmp_path):
    result = cli(MPZ)
    assert result.returncode == 2 and result.stderr == ""
    assert "Format: mpz, raw marker 8" in result.stdout
    assert "LINE: 14" in result.stdout
    assert "not decoded, object kind 25: 1" in result.stdout
    assert "skipped, object kind 4: 1" in result.stdout
    result = cli(MPZ, "--json")
    row = json.loads(result.stdout)
    assert result.returncode == 2 and "entities" not in row
    assert row["report"]["decoded_counts"]["TEXT"] == 4
    result = cli(MPZ, "--json", "--entities")
    assert len(json.loads(result.stdout)["entities"]) == 16
    result = cli(FIXTURES / "basic2d25.cfb", "--json")
    assert result.returncode == 0 and not json.loads(result.stdout)["report"]["lossy"]
    result = cli(MPZ, "--json", "--max-entities", 3)
    assert result.returncode == 3 and result.stdout == ""
    assert json.loads(result.stderr)["resource"] == "best_effort_entities"
    result = cli(FIXTURES / "minimal.cfb", "--json")
    assert result.returncode == 2
    assert json.loads(result.stderr)["error"] == "UnsupportedFormatError"
    malformed = tmp_path / "bad.mpz"
    malformed.write_bytes(b"not a compound file")
    result = cli(malformed)
    assert result.returncode == 1 and result.stderr.startswith("error: ")
