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
MPUW = FIXTURES / "best-effort25u.mpuw"
EARLY = FIXTURES / "best-effort1.cfb"
TYPES = [
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
    "EXTENDED_POLYGON",
    "EXTENDED_POLYGON",
    "HATCH",
    "DIMENSION",
    "PACKAGE",
    "DIMENSION",
    "LEADER",
    "PACKAGE",
    "INSERT",
    "IMAGE",
    "EXTERNAL_REFERENCE",
    "LINE3D",
    "EXTENDED_POLYGON",
    "HATCH",
    "VIEWPORT",
    "LINE",
    "LAYOUT",
    "PACKAGE",
    "LINE",
]


def cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", "best-effort", *map(str, args)],
        capture_output=True,
        text=True,
        check=False,
    )


@pytest.fixture(scope="module", params=["older", "unicode"])
def drawing(request):
    """The same content in the older and in the Unicode record layout."""
    return best_effort.readfile(MPZ if request.param == "older" else MPUW)


def one(drawing, kind):
    (entity,) = (entity for entity in drawing.entities if entity.type == kind)
    return entity


def test_every_decoded_kind_keeps_its_stored_values(drawing):
    unicode = drawing.format == "mpuw"
    assert (drawing.marker, drawing.marker_observed) == (
        (25, True) if unicode else (8, True)
    )
    assert [entity.type for entity in drawing.entities] == TYPES
    first = drawing.entities[0]
    assert first.data == {
        "start": [1.0, 2.0],
        "end": [3.0, 4.0],
        "elevation": 0.0,
        "thickness": 0.0,
    }
    assert (first.layer, first.color, first.line_type, first.width) == (3, 7, 2, 4)
    assert (first.group, first.link_id, first.segment) == (9, 0, 0)
    assert (first.table, first.block_index, first.record_index) == ("root", None, 2)
    record = 80 if unicode else 64
    assert first.payload_offset == 8192 + 2 * record and first.record_count == 1
    assert first.children == ()
    arc = one(drawing, "ARC").data
    assert (arc["center"], arc["radius"]) == ([8.0, 9.0], 2.0)
    assert (arc["start_degrees"], arc["end_degrees"]) == (30.0, 120.0)
    partial = one(drawing, "ELLIPTICAL_ARC").data
    assert (partial["rotation_degrees"], partial["end_degrees"]) == (15.0, 80.0)
    assert one(drawing, "POINT").data == {"position": [9.0, 8.0], "point_type": 3}
    assert one(drawing, "LINE3D").data == {
        "start": [1.0, 2.0, 3.0],
        "end": [4.0, 5.0, 6.0],
    }
    last = drawing.entities[-1]
    assert (last.color, last.line_type, last.width) == (
        best_effort.BY_LAYER_COLOR,
        best_effort.BY_LAYER_LINE_TYPE,
        best_effort.BY_BLOCK_WIDTH,
    )


def test_fills_are_explicit(drawing):
    circle = one(drawing, "CIRCLE").data
    assert (circle["center"], circle["radius"]) == ([5.0, 6.0], 7.0)
    assert circle["fill"] == {
        "color": best_effort.RGB_FILL_COLOR,
        "rgb": [10, 20, 30],
        "transparency": 40,
        "outline": True,
    }
    ellipse = one(drawing, "ELLIPSE").data
    assert ellipse["radius"] == [4.0, 2.0]
    assert ellipse["fill"] == {
        "color": 6,
        "rgb": [0, 0, 0],
        "transparency": 0,
        "outline": False,
    }
    plain, spline = (e for e in drawing.entities if e.type == "POLYLINE")
    assert plain.data["fill"]["color"] == 4 and plain.data["fill"]["outline"]
    assert spline.data["fill"] is None
    hatched, filled, patterned, area = (
        e.data for e in drawing.entities if e.type == "EXTENDED_POLYGON"
    )
    # A transparency runs from 0, opaque, to 255.
    assert area["fill"] == {
        "color": 4,
        "rgb": [0, 0, 0],
        "transparency": 204,
        "outline": True,
    }
    assert hatched["fill"] is None and patterned["fill"] is None
    assert filled["fill"]["color"] == 6 and filled["flags"] == 5


def test_text_is_decoded_from_the_saved_bytes(drawing):
    unicode = drawing.format == "mpuw"
    plain, styled = (e.data for e in drawing.entities if e.type == "TEXT")
    assert plain["text"] == "図面 A髙\n2"
    saved = "図面 A髙\r2".encode("utf-16-le") if unicode else None
    assert plain["text_bytes"] == saved or plain["text_bytes"].startswith(
        "図面 A".encode("cp932") + b"{\\uni"
    )
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
    assert not plain["output_size"]
    assert styled["text"] == "見出し\n本文" and styled["format_codes"]


def test_polylines_and_hatched_regions(drawing):
    plain, spline = (e for e in drawing.entities if e.type == "POLYLINE")
    assert [child.type for child in plain.children] == ["LINE", "LINE", "ARC"]
    assert plain.data["closed"] and plain.data["curve_kind"] == 0
    assert plain.data["curve_points"] is None and plain.children[0].flags == 0x08
    assert [child.reversed for child in plain.children] == [False, False, True]
    assert not one(drawing, "ARC").reversed
    assert (spline.data["curve_kind"], spline.data["divisions"]) == (3, 10)
    curve = spline.data["curve_points"]
    assert len(curve) == 3 * 10 + 1 and curve[0] == [0.0, 0.0]
    assert curve[-1] == pytest.approx([0.0, 40.0])

    hatched, filled, patterned, _ = (
        e for e in drawing.entities if e.type == "EXTENDED_POLYGON"
    )
    lines, pattern, _ = (e for e in drawing.entities if e.type == "HATCH")
    assert hatched.data["region_id"] == lines.data["region_id"] == 0x098D
    assert patterned.data["region_id"] == pattern.data["region_id"] == 0x098E
    assert [e.data["reference_number"] for e in (hatched, filled, patterned)] == [
        1,
        0,
        2,
    ]
    assert (filled.data["region_id"], filled.data["hole_count"]) == (0, 1)
    assert [len(loop.children) for loop in filled.children] == [4, 4]
    assert [len(loop.children) for loop in hatched.children] == [4]
    assert lines.data["hatch_type"] == 0 and lines.data["pattern"] is None
    assert lines.data["lines"] == [
        {
            "origin": [0.0, 0.0],
            "pitch": 5.0,
            "angle_degrees": angle,
            "color": 7,
            "line_type": 2,
            "width": 4,
        }
        for angle in (45.0, 135.0)
    ]
    assert pattern.data["hatch_type"] == 1 and pattern.data["lines"] == []
    assert pattern.data["pattern"] == {
        "name": "断熱材2",
        "origin": [3.0, 4.0],
        "values": [0.38, 360.0, 1.0, 90.0, 0.005, 0.005, 360.0],
        "color": 7,
        "line_type": 2,
        "width": 4,
    }


def test_dimensions_and_leaders_come_with_their_packages(drawing):
    linear, angle = (e for e in drawing.entities if e.type == "DIMENSION")
    drawn, pointer, _ = (e for e in drawing.entities if e.type == "PACKAGE")
    assert linear.data["dimension_type"] == 2 and linear.data["arc"] is None
    assert linear.data["line"] == [[0.0, 60.0], [40.0, 60.0]]
    assert linear.data["extension_origins"] == [[0.0, 40.0], [40.0, 40.0]]
    assert linear.data["arrow_styles"] == [3, 1, 3, 1]
    assert linear.data["arrow_sizes"] == [3.0, 3.0, 3.0, 3.0]
    assert linear.data["arrow_paper_sizes"] is None
    (number,) = linear.children
    assert number.data["numeric"] and number.data["number"] == 1234.5
    assert number.data["output_size"] and not angle.children[0].data["output_size"]
    assert [child.data["output_size"] for child in drawn.children[3:5]] == [True] * 2
    assert number.data["text"] == "40"
    assert (drawn.data["package_kind"], drawn.data["reference_number"]) == (3, 1)
    assert [child.type for child in drawn.children] == [
        "LINE",
        "LINE",
        "POINT",
        "ARROWHEAD",
        "ARROWHEAD",
        "TEXT",
    ]
    linked = [child.link_id for child in drawn.children]
    assert linear.link_id == 104 and linked == [104, 0, 0, 0, 0, 104]
    assert drawn.children[3].data["arrow_type"] == 5
    assert len(list(drawn.walk())) == 7

    assert angle.data["dimension_type"] == 3
    assert angle.data["arc"] == {
        "center": [0.0, 0.0],
        "radius": 10.0,
        "start_degrees": 0.0,
        "end_degrees": 58.0,
    }
    assert angle.data["arrow_paper_sizes"] == [1.0, 3.0, 1.0, 3.0]
    assert angle.children[0].data["text"] == "58°"

    leader = one(drawing, "LEADER")
    assert leader.data == {
        "leader_type": 1,
        "flags": 0x80,
        "flags2": 0,
        "arrow_style": 7,
        "lines": [[[10.0, 10.0], [21.5, 30.0]], [[21.5, 30.0], [60.0, 30.0]]],
        "arrow_size": [100.0, 300.0],
        "arrow_paper_size": [1.0, 3.0],
    }
    assert [child.data["text"] for child in leader.children] == ["注記"]
    assert (pointer.data["package_kind"], pointer.data["reference_number"]) == (4, 2)
    assert [child.type for child in pointer.children] == [
        "LINE",
        "LINE",
        "ARROWHEAD",
        "TEXT",
    ]
    assert leader.link_id == pointer.children[0].link_id == 106
    # The members of a group are counted but not repeated at the top level.
    total = sum(drawing.report.decoded_counts.values())
    assert sum(1 for _ in drawing.walk()) == total - 2


def test_blocks_and_placed_files(drawing):
    unicode = drawing.format == "mpuw"
    reference = one(drawing, "INSERT")
    name_bytes = "SYM".encode("utf-16-le" if unicode else "ascii")
    assert reference.data["name"] == "SYM"
    assert reference.data["name_bytes"] == name_bytes
    assert reference.data["reference_number"] == 0
    assert reference.data["position"] == [100.0, 200.0]
    assert reference.data["scale"] == [2.0, 2.0]
    assert reference.data["rotation_degrees"] == 90.0
    block = drawing.block("SYM")
    assert block is drawing.block(name_bytes) and drawing.block("missing") is None
    assert [entity.type for entity in block.entities] == ["LINE", "CIRCLE"]
    assert (block.entities[0].table, block.entities[0].block_index) == ("block", 0)
    assert block.entities[1].data["fill"] is None

    assert one(drawing, "IMAGE").data == {
        "file_name": "写真.tif",
        "directory": "E:\\Photo\\",
        "position": [335.0, 107.0],
        "elevation": 0.0,
        "scale": [0.4, 0.4],
        "size": [69.0, 49.0],
        "rotation_degrees": 90.0,
    }
    assert one(drawing, "EXTERNAL_REFERENCE").data == {
        "symbol": False,
        "file_name": "図面枠.mps",
        "directory": "D:\\Work\\",
        "relative_directory": ".\\",
        "position": [1.0, 2.0],
        "elevation": 0.0,
        "scale": [1.0, 1.0],
        "rotation_degrees": 30.0,
    }


def test_tables_and_settings_have_the_same_ranges_in_both_layouts(drawing):
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
        paper_type=4,
        paper_portrait=True,
        scale_ratio=0.5,
        comment="改修工事 A棟",
        width_by_color=True,
    )
    assert drawing.settings.paper_name == "A3"
    # The scale the drawing states in its scale field: 1:50 over the ratio.
    assert drawing.settings.stated_scale == 100.0


def test_report_counts_what_was_not_decoded(drawing):
    report = drawing.report
    assert report.lossy
    assert dict(report.unsupported_counts) == {0x2E: 1}
    assert dict(report.failed_counts) == {4: 1}
    assert (report.property_records, report.orphan_records, report.clamped_spans) == (
        2,
        0,
        0,
    )
    # An erased object keeps its records; it is neither decoded nor missing.
    assert report.erased_objects == 1
    assert report.decoded_counts["LINE"] == 33 and report.decoded_counts["TEXT"] == 8
    assert report.decoded_counts["LEADER"] == report.decoded_counts["IMAGE"] == 1
    (diagnostic,) = report.diagnostics
    assert diagnostic.code == "object_layout_mismatch"
    # Strings take more records as UTF-16 than as Shift_JIS.
    assert (diagnostic.table, diagnostic.tag, diagnostic.record_index) == (
        "root",
        4,
        168 if drawing.format == "mpuw" else 166,
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
    assert row["schema_version"] == 2
    assert len(row["entities"]) == len(drawing.entities)
    assert (
        row["entities"][6]["data"]["text_bytes"]
        == drawing.entities[6].data["text_bytes"].hex()
    )
    assert row["entities"][1]["data"]["fill"]["rgb"] == [10, 20, 30]
    assert row["blocks"][0]["name_bytes"] == drawing.blocks[0].name_bytes.hex()
    assert row["report"]["unsupported_counts"] == {"46": 1}
    assert row["settings"]["comment"] == "改修工事 A棟"
    assert row["properties"][0]["name"] == "会社名"
    assert row["settings"]["paper_size"] == [420.0, 297.0]
    summary = drawing.to_dict(include_entities=False)
    assert "entities" not in summary and "blocks" not in summary


def test_properties_hold_the_drawing_property_set(drawing):
    first, second = drawing.properties
    assert (first.owner, first.name, first.value) == (
        best_effort.DRAWING_PROPERTY_SET,
        "会社名",
        "設計室 A",
    )
    assert (first.table, first.block_index, first.record_index) == ("root", None, 0)
    assert not first.lossy and first.payload_offset == 8192
    # The first releases tagged properties with another object kind.
    assert (second.owner, second.name, second.value) == (
        "DRACAD_SYSTEM",
        "PSET_LIST",
        "会社名",
    )
    assert drawing.property_set() == {"会社名": "設計室 A"}
    with pytest.raises(FrozenInstanceError):
        first.value = ""


def test_sheets_named_areas_and_gradations(drawing):
    viewport = one(drawing, "VIEWPORT")
    assert viewport.segment == 1 and viewport.data == {
        "flags": 0x80,
        "paper_point": [148.5, 105.0],
        "model_point": [-4484.0, 9311.0],
        "scale": [0.02, 0.02],
        "rotation_degrees": 0.0,
        "center": [148.5, 105.0],
        "half_size": [128.5, 85.0],
    }
    layout = one(drawing, "LAYOUT")
    assert layout.segment == 0 and layout.data == {
        "number": 1,
        "name": "配置図",
        "paper_type": 5,
        "paper_size": [297.0, 210.0],
    }
    # Objects drawn on the sheet name it; everything else is the model.
    on_sheet = [e for e in drawing.entities if e.segment == 1]
    assert [e.type for e in on_sheet] == ["VIEWPORT", "LINE"]
    assert on_sheet[1].data["end"] == [100.0, 50.0] and on_sheet[1].layer == 3

    area = [e for e in drawing.entities if e.type == "EXTENDED_POLYGON"][3]
    assert area.data["name"] == "居室" and area.data["polygon_kind"] == 1
    # The attribute parts are not members; the label is.
    assert [child.type for child in area.children] == ["POLYLINE", "TEXT"]
    assert area.children[1].data["text"] == "居室"
    assert area.children[1].data["position"] == [20.0, 20.0]

    gradation = [e for e in drawing.entities if e.type == "HATCH"][2]
    assert gradation.data["hatch_type"] == 2 and gradation.data["lines"] == []
    assert gradation.data["gradation"] == {
        "start": [0.0, 40.0],
        "end": [0.0, 0.0],
        "rgb_start": [130, 224, 255],
        "rgb_end": [255, 255, 255],
        "gradation_type": 0,
    }


def test_model_parts_are_read(drawing):
    model = [e for e in drawing.entities if e.type == "PACKAGE"][2]
    face, part, camera, light = model.children
    assert face.type == "POLYGON_3D" and face.data == {"hole_count": 1}
    assert [loop.type for loop in face.children] == ["POLYLINE", "POLYLINE"]
    assert face.children[1].data["three_d"]
    assert face.children[1].children[0].data["start"] == [0.0, 0.0, 5.0]
    assert part.type == "EXTERNAL_REFERENCE" and part.data == {
        "symbol": True,
        "file_name": "椅子.mps",
        # Longer than one record of names holds.
        "directory": "C:\\Work\\Projects\\Building A\\Parts\\",
        "relative_directory": "",
        "position": [10.0, 20.0],
        "elevation": 275.0,
        "scale": [1.0, 1.0],
        "rotation_degrees": 270.0,
    }
    assert camera.type == "CAMERA" and camera.data == {
        "position": [1.0, 2.0, 3.0],
        "target": [4.0, 5.0, 6.0],
    }
    assert light.type == "LIGHT" and light.data["target"] == [7.0, 8.0, 0.0]


def test_a_drawing_of_the_first_releases_is_read():
    drawing = best_effort.readfile(EARLY)
    assert (drawing.format, drawing.marker, drawing.marker_observed) == (
        "mpz",
        1,
        True,
    )
    assert [e.type for e in drawing.entities] == [
        "LINE",
        "CIRCLE",
        "ARC",
        "TEXT",
        "POINT",
    ]
    # Leftover bytes in a continuation header do not hide the arc.
    assert one(drawing, "ARC").data["radius"] == 2.0
    report = drawing.report
    assert not report.lossy and report.diagnostics == ()
    assert report.erased_objects == 1
    # Fewer rows than later, and no layer widths or names in them.
    assert (len(drawing.layers), len(drawing.colors)) == (200, 32)
    assert (len(drawing.line_types), len(drawing.widths)) == (16, 16)
    assert drawing.layers[0] == best_effort.Layer(1, "", 0x0400, False, 1, 1, 0)
    assert drawing.colors[8] == best_effort.Color(9, (0, 0, 128), 0.1)
    assert drawing.line_types[1].pattern == (1.0, 2.0, 2.0, 2.0, 1.0)
    assert drawing.line_types[2].pattern == ()
    assert drawing.widths[2] == best_effort.Width(3, 0.1)
    settings = drawing.settings
    assert (settings.paper_name, settings.paper_portrait) == ("A2", False)
    # The comment sits in the short field alone.
    assert settings.comment == "300x300mmタイル"
    assert (settings.scale_ratio, settings.stated_scale) == (1.0, 50.0)
    line = drawing.entities[0]
    assert (line.layer, line.color, line.line_type, line.width) == (30, 9, 1, 1)
    # The root of this file has no class identifier; the other reader asks for one.
    assert ez.raw.inspectfile(EARLY).format == "unknown_cfb"


def test_both_layouts_hold_the_same_content():
    def content(entity):
        data = {
            key: value
            for key, value in entity.data.items()
            if key not in ("text_bytes", "name_bytes")
        }
        attributes = (entity.type, entity.layer, entity.color, entity.line_type)
        return attributes, data, [content(child) for child in entity.children]

    older, unicode = best_effort.readfile(MPZ), best_effort.readfile(MPUW)
    assert (older.format, unicode.format) == ("mpz", "mpuw")
    assert [content(e) for e in older.entities] == [
        content(e) for e in unicode.entities
    ]
    assert older.report.decoded_counts == unicode.report.decoded_counts
    assert (older.layers, older.colors) == (unicode.layers, unicode.colors)
    assert (older.line_types, older.widths) == (unicode.line_types, unicode.widths)
    assert [(p.owner, p.name, p.value) for p in older.properties] == [
        (p.owner, p.name, p.value) for p in unicode.properties
    ]
    assert older.settings == unicode.settings
    assert older.record_count < unicode.record_count
    assert older.payload_size == PAYLOAD.stat().st_size < unicode.payload_size


def test_carriers_are_identified_by_content():
    flat = best_effort.read(PAYLOAD.read_bytes())
    assert flat.format == "mpx" and len(flat.entities) == len(TYPES)
    assert flat.payload_sha256 == best_effort.readfile(MPZ).payload_sha256
    assert best_effort.read(MPUW.read_bytes()).format == "mpuw"
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
    # Only the best-effort reader decodes the Unicode envelope.
    with pytest.raises(ez.UnsupportedFormatError, match="mps_candidate"):
        ez.readfile(MPUW)


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
    with pytest.raises(ez.LimitExceededError) as caught:
        best_effort.readfile(MPUW, limits=ez.ParseLimits(max_decompressed_size=4096))
    assert caught.value.resource == "decompressed_size"
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
    assert "LINE: 33" in result.stdout
    assert "not decoded, object kind 46: 1" in result.stdout
    assert "skipped, object kind 4: 1" in result.stdout
    result = cli(MPUW)
    assert result.returncode == 2 and "Format: mpuw, raw marker 25" in result.stdout
    result = cli(MPZ, "--json")
    row = json.loads(result.stdout)
    assert result.returncode == 2 and "entities" not in row
    assert row["report"]["decoded_counts"]["TEXT"] == 8
    result = cli(MPZ, "--json", "--entities")
    assert len(json.loads(result.stdout)["entities"]) == len(TYPES)
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
