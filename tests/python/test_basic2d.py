"""Opt-in mixed Document APIs; synthetic vectors are not native evidence."""

import importlib.util
import json
import struct
import sys
from dataclasses import FrozenInstanceError, fields
from pathlib import Path

import pytest

import ezdracad as ez
from ezdracad import _core

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
try:
    spec = importlib.util.spec_from_file_location(
        "basic2d_fixture", ROOT / "scripts/make_basic2d_fixture.py"
    )
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
finally:
    sys.path.pop(0)

MPZ = ROOT / "tests/fixtures/basic2d25.cfb"
PROFILE = ez.Basic2DProfile.MARKER25_ROOT_PRIMITIVES_V1


def read_records(records, **options):
    return ez.read(
        fixture.wrap_record_contents(fixture.make_basic2d_contents(records)),
        basic_profile=PROFILE,
        scope="stored_xy_primitives",
        **options,
    )


def test_opt_in_scope_and_drawing_completeness_remain_explicit():
    with pytest.raises(ez.IncompleteReadError):
        ez.readfile(MPZ)
    old = ez.readfile(MPZ, policy="partial")
    assert old.report.basic_profile is None
    assert old.report.requested_entity_types == ("LINE",)
    with pytest.raises(ez.UnsupportedFeatureError):
        old.query("CIRCLE")
    for options in [
        dict(scope="stored_xy_primitives"),
        dict(scope="stored_xy_lines", basic_profile=PROFILE),
    ]:
        with pytest.raises(ez.UnsupportedFeatureError):
            ez.readfile(MPZ, **options)
    with pytest.raises(ez.IncompleteReadError) as caught:
        ez.readfile(MPZ, basic_profile=PROFILE)
    assert caught.value.report.geometry_status == ez.ReadStatus.COMPLETE
    document = ez.readfile(MPZ, basic_profile=PROFILE, scope="stored_xy_primitives")
    assert document.report.status == ez.ReadStatus.COMPLETE
    assert document.report.document_status == ez.ReadStatus.PARTIAL
    assert document.report.document_complete is False and document.report.units is None
    assert document.report.requested_entity_types == ("LINE", "CIRCLE", "ARC", "TEXT")
    assert document.report.basic_profile == PROFILE
    assert document.report.handled_auxiliary_object_count == 2


def test_mixed_entities_paging_filtering_and_snapshot_sources(tmp_path):
    path = tmp_path / "mixed.mpz"
    path.write_bytes(MPZ.read_bytes())
    document = ez.readfile(path, basic_profile=PROFILE, scope="stored_xy_primitives")
    path.unlink()
    entities = tuple(document.entities)
    assert tuple(entity.type for entity in entities) == (
        "LINE",
        "CIRCLE",
        "ARC",
        "TEXT",
        "TEXT",
        "CIRCLE",
        "LINE",
    )
    assert [entity.entity_id for entity in entities] == list(range(7))
    assert [entity.root_record_index for entity in entities] == [2, 3, 5, 7, 12, 18, 20]
    assert isinstance(entities[0], ez.LineEntity)
    assert isinstance(entities[1], ez.CircleEntity)
    assert isinstance(entities[2], ez.ArcEntity)
    assert isinstance(entities[3], ez.TextEntity)
    assert document.entities[::-2] == entities[::-2]
    assert document.entities.page(2, 3) == entities[2:5]
    assert document.query("TEXT").page(1, 1) == (entities[4],)
    assert [entity.entity_id for entity in document.query("CIRCLE")] == [1, 5]
    assert document.query("ARC")[0] == entities[2]
    assert document.query("TEXT").page(2) == ()
    for entity in entities[1:6]:
        if entity.type == "LINE":
            continue
        source = entity.source_ref
        start = source.payload_offset
        assert (
            entity.raw_bytes
            == document.raw().read_contents()[start : start + source.byte_length]
        )
        assert source.table_domain == "root_records" and source.definition_index is None
        assert source.offset_domain == "cfb_stream"
        assert entity.attributes.header_bytes == entity.raw_bytes[:16]
        assert entity.styles.color.index == 3 and entity.styles.line_type.index == 2
        assert entity.styles.width.index == 4 and entity.style_layer_index == 2
        assert entity.styles.color.sources[0].raw_bytes == entity.raw_bytes
        with pytest.raises(FrozenInstanceError):
            entity.entity_id = 100
    assert entities[1].center == (10.25, -20.5) and entities[1].radius == 3.75
    assert entities[2].center == (-3, 7) and entities[2].radius == 9
    assert (entities[2].start_angle_degrees, entities[2].end_angle_degrees) == (270, 15)
    assert entities[1].raw_bytes == entities[5].raw_bytes
    assert entities[1].source_ref != entities[5].source_ref
    encoded = document.to_dict()
    assert encoded["report"]["basic_profile"] == PROFILE.value
    assert encoded["report"]["requested_entity_types"] == [
        "LINE",
        "CIRCLE",
        "ARC",
        "TEXT",
    ]
    assert "raw_bytes" not in json.dumps(encoded, allow_nan=False)


def test_text_body_font_geometry_and_serialized_sources_are_distinct():
    document = ez.readfile(MPZ, basic_profile=PROFILE, scope="stored_xy_primitives")
    first, second = document.query("TEXT")
    assert first.text == "合成図面β" and first.basepoint == (6.125, -8.25)
    assert first.body_bytes == first.serialized_text.encode("ascii")
    assert first.body_bytes != first.text.encode("utf-8")
    assert first.font_name == "Synthetic Font"
    assert first.font_bytes == first.font_name.encode("utf-16le")
    assert (first.width, first.height, first.rotation_degrees) == (2.5, 4, 17)
    assert first.text_flags == (53, 0) and first.scale_flag == 1 and first.origin == 8
    assert (
        first.units is None
        and first.body_encoding == "ascii_native_bmp_unicode_escapes"
    )
    assert second.text == "A" * 95 + "Z" and len(second.body_bytes) == 96
    assert second.source_ref.byte_length == 384
    assert (second.width, second.height, second.rotation_degrees) == (1.75, 5.25, -23.5)
    assert second.to_dict()["encoded_byte_length"] == 96


@pytest.mark.parametrize(
    "body", ["", '{\\b"A"}', "{TCODE:UserName}", '{\\uni"1f600"}', "A\nB"]
)
def test_unqualified_text_is_reported_without_partial_text_leaks(body):
    document = read_records(fixture.text(body), policy="partial")
    assert len(document.query("TEXT")) == 0
    assert document.report.unsupported_counts == {5: 1}
    assert document.report.status == ez.ReadStatus.PARTIAL
    assert document.raw().record_bytes(0)[0] == 5
    with pytest.raises(ez.IncompleteReadError):
        read_records(fixture.text(body))


def test_text_limits_and_nonfinite_values_are_fatal_across_unsupported_fences():
    for limits, resource in [
        (ez.TextLimits(max_encoded_bytes=59), "text_encoded_bytes"),
        (ez.TextLimits(max_output_bytes=13), "text_output_bytes"),
    ]:
        with pytest.raises(ez.LimitExceededError) as caught:
            ez.readfile(
                MPZ, policy="partial", basic_profile=PROFILE, text_limits=limits
            )
        assert caught.value.resource == resource
    for field_offset in [16, 24, 32, 40, 48, 56, 144]:
        records = fixture.text("A")
        struct.pack_into(
            "<d", records[field_offset // 64], field_offset % 64, float("nan")
        )
        records[2][6] = 0  # Unknown formatting must not hide known numeric corruption.
        with pytest.raises(ez.InvalidFormatError):
            read_records(records, policy="partial")


def test_options_and_limit_values_match_the_rust_boundary():
    limits = ez.TextLimits()
    assert (
        tuple(getattr(limits, item.name) for item in fields(limits))
        == _core.text_limit_defaults()
    )
    options = ez.ReadOptions(
        basic_profile=PROFILE, text_limits=limits, scope="stored_xy_primitives"
    )
    assert options._values()[-2:] == (PROFILE.value, limits._values())
    for value in [-1, 1 << 64]:
        with pytest.raises(ValueError):
            ez.TextLimits(max_output_bytes=value)
    with pytest.raises(TypeError):
        ez.TextLimits(max_encoded_bytes=True)
    with pytest.raises(TypeError):
        ez.ReadOptions(text_limits={})
    with pytest.raises(TypeError):
        ez.ReadOptions(basic_profile=123)
    with pytest.raises(ValueError):
        ez.ReadOptions(basic_profile="guess")
    with pytest.raises(ValueError):
        ez.readfile(MPZ, options=options, text_limits=limits)


@pytest.mark.parametrize("kind", ["curve", "text"])
def test_failed_primitive_selection_consumes_aggregate_work(kind):
    if kind == "curve":
        records = fixture.curve()
        struct.pack_into("<d", records[1], 16, 0.0)
    else:
        records = fixture.text("{TCODE:UserName}")
    document = read_records(records, policy="partial")
    assert len(document.entities) == 0
    assert document.report.scene_nodes_visited == 1
    with pytest.raises(ez.LimitExceededError) as caught:
        read_records(records, policy="partial", read_limits=ez.ReadLimits(max_nodes=0))
    assert caught.value.resource == "document_nodes"
    assert not hasattr(caught.value, "report")
