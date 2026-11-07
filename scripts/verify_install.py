"""Smoke-test an installed distribution from outside the source tree (stdlib only)."""

import argparse
import hashlib
import json
import struct
import subprocess
import sys
from importlib.metadata import version
from pathlib import Path

import ezdracad
from ezdracad import _core
from ezdracad.block3d import Block3DProfile
from ezdracad.line3d import LineRecord3D
from ezdracad.print_width import PrintWidthPolicy, resolve_nominal_print_width
from ezdracad.scene import SceneLimits
from ezdracad.workplane import WorkplaneProfile
from ezdracad.world_input3d import WorldInput3DProfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--python-version", required=True)
    parser.add_argument("--rust-version", required=True)
    args = parser.parse_args()
    package = Path(ezdracad.__file__).resolve().parent
    assert package.is_relative_to(Path(sys.prefix).resolve()), package
    assert ezdracad.__version__ == version("ezdracad") == args.python_version
    assert _core.core_version() == args.rust_version
    for filename in ["py.typed", "_core.pyi"]:
        assert (package / filename).is_file(), filename
    assert any(package.glob("_core*.so")) or any(package.glob("_core*.pyd"))
    info = ezdracad.raw.inspect_cfb(args.fixture.read_bytes())
    assert info.version == 3 and info.total_stream_size == 11
    assert [(e.path, e.kind, e.size) for e in info.entries] == [
        ("/Example", "storage", 0),
        ("/Example/Note", "stream", 11),
    ]
    envelope = args.fixture.parent / "mpz-envelope.cfb"
    contents = (args.fixture.parent / "mpz-contents.bin").read_bytes()
    inspection = ezdracad.raw.inspectfile(envelope)
    assert inspection.format == "mpz"
    assert inspection.payload.sha256 == hashlib.sha256(contents).hexdigest()
    assert inspection.read_contents() == contents
    try:
        ezdracad.raw.inspect_cfb(b"bad")
    except ezdracad.InvalidFormatError:
        pass
    else:
        raise AssertionError("malformed CFB was accepted")
    result = subprocess.run(
        [sys.executable, "-m", "ezdracad", "--version"],
        capture_output=True,
        text=True,
        check=True,
    )
    assert result.stdout.strip() == f"ezdracad {args.python_version}"
    cli = Path(sys.executable).parent / (
        "ezdracad.exe" if sys.platform == "win32" else "ezdracad"
    )
    subprocess.run([str(cli), "--version"], check=True)
    report = subprocess.run(
        [str(cli), "inspect", str(envelope), "--json"],
        capture_output=True,
        text=True,
        check=True,
    )
    assert json.loads(report.stdout) == inspection.to_dict()
    record_fixture = args.fixture.parent / "records5.cfb"
    scan = ezdracad.raw.scanfile(record_fixture)
    assert scan.header.record_count == 5 and scan.opaque_record_count == 2
    assert scan.line_candidate_count == 2
    assert scan.line_candidates()[0].start == (-12.5, 0.25)
    fields = scan.line_records()
    assert [line.record_index for line in fields] == [0, 2]
    assert fields[0].end == (42.0, -3.125)
    assert fields[0].elevation == fields[0].thickness == 0
    assert fields[0].header_bytes == scan.record_bytes(0)[:16]
    assert LineRecord3D.__module__ == "ezdracad.line3d"
    assert callable(scan.line_records_3d)
    assert scan.line_records_3d() == ()
    assert (
        ezdracad.raw.scanfile(args.fixture.parent / "records25.cfb").line_records_3d()
        == ()
    )
    assert all(
        line.coordinate_space == "record_local"
        and line.units is None
        and line.parent_context is None
        for line in fields
    )
    assert (
        scan.record_bytes(1)
        == (args.fixture.parent / "records5.bin").read_bytes()[8256:8320]
    )
    report = subprocess.run(
        [str(cli), "scan", str(record_fixture), "--json"],
        capture_output=True,
        text=True,
        check=True,
    )
    assert json.loads(report.stdout) == scan.to_dict()
    for marker in (5, 8, 25):
        blocks = ezdracad.raw.scanfile(args.fixture.parent / f"blocks{marker}.cfb")
        assert blocks.header.profile == f"observed_marker{marker}_stride64"
        library = blocks.block_library()
        assert [d.name_bytes for d in library.definitions] == [b"BASE", b"TWO"]
        references = blocks.package_references(0, 5)
        assert [r.name_key for r in references] == [b"TWO", b"BASE"]
        assert library.resolve(references[0].name_key).index == 1
        local = library.planar_lines(1)
        assert local[1].coordinate_space == "block_definition_local"
        assert local[1].tag_raw == 0x0402
        placed = library.placed_lines(references[0].record_index)
        assert placed[0].start == (-23.75, 72.5)
        assert placed[1].end == (17.0, 75.25)
        assert placed[1].source.header_bytes == library.record_bytes(1, 1)[:16]
        assert placed[1].coordinate_space == "package_parent"
        assert placed[1].units is None and placed[1].parent_context is None
        assert references[1].apply_xy((1.25, -2.5)) == (105.0, -47.5)
        assert library.suffix_bytes() == bytes([0xAB]) * library.suffix_size
        objects = ezdracad.raw.scanfile(args.fixture.parent / f"objects{marker}.cfb")
        spans = objects.object_spans()
        assert [s.record_index for s in spans] == [0, 2, 3, 5]
        assert [s.record_count for s in spans] == [2, 1, 2, 2]
        assert [s.end_record_index for s in spans] == [2, 3, 5, 7]
        assert [s.tag_raw for s in spans] == [0x30, 2, 0x33, 0x7777]
        assert spans[0].header_bytes == objects.record_bytes(0)[:16]
        assert [line.record_index for line in objects.line_records()] == [1, 2]
        settings = objects.drawing_settings()
        assert settings.payload_offset == 0x140
        assert settings.drawing_origin == (-12.5, 250.25)
        assert settings.paper_scale_factor == 0.04 and settings.drawing_scale == 25
        assert settings.units is None
        assert settings.header_bytes == objects.read_contents()[0x140:0x180]
        try:
            objects.object_spans(max_objects=3)
        except ezdracad.LimitExceededError:
            pass
        else:
            raise AssertionError("installed object-span limit was ignored")
    semantic = ezdracad.raw.scanfile(args.fixture.parent / "semantics25.cfb")
    assert semantic.text_size_unit().unit == "pt"
    assert semantic.text_size_unit().raw_selector == 1
    assert semantic.text_size_unit().payload_offset == 0x9AC
    assert semantic.line_records() == ()  # Older exact-tag API stays unchanged.
    attributed = semantic.attributed_line_records()
    assert [line.source.tag_raw for line in attributed] == [0x2402, 0x4402]
    assert attributed[0].attributes.color.kind == "by_layer"
    assert attributed[1].attributes.color.kind == "by_block"
    assert attributed[0].attributes.material_index_raw == 3
    assert attributed[0].attributes.group_index == 257
    assert semantic.record_attributes(1) == attributed[0].attributes
    assert (
        ezdracad.raw.decode_record_attributes(semantic.record_bytes(1)[:16])
        == attributed[0].attributes
    )
    inline = semantic.inline_package_lines(0)
    assert len(inline) == 1 and inline[0].source.start == (1.25, -2.5)
    assert [p.record_index for p in inline[0].ancestors] == [0]
    assert inline[0].table_domain == "root_records"
    assert inline[0].definition_index is None and inline[0].source.units is None
    styles = semantic.style_tables()
    assert (
        styles.layer_count,
        styles.color_count,
        styles.line_type_count,
        styles.width_count,
    ) == (257, 256, 32, 16)
    assert styles.layer(256).index == 256 and len(styles.layer(0).raw_bytes) == 296
    assert styles.color(1).rgb == (37, 19, 11)
    assert styles.color(256).pen_width_value == 7.68
    assert styles.line_type(4).pen_definition_values[:2] == (1.25, 0.37)
    assert len(styles.line_type(4).pen_definition_values) == 30
    assert styles.width(5).pen_width_value == 0.35
    assert styles.suffix_bytes()[1768:] == bytes([0x5A]) * 5504
    measurement = ezdracad.raw.scanfile(args.fixture.parent / "measurement25.cfb")
    origin_code = measurement.origin_units_code()
    assert origin_code.payload_offset == 0x150
    assert origin_code.selector_bytes == measurement.read_contents()[0x150:0x154]
    assert origin_code.raw_code == int.from_bytes(
        origin_code.selector_bytes, "little", signed=True
    )
    for source in measurement.print_settings().fields:
        assert (
            source.raw_bytes
            == measurement.read_contents()[
                source.payload_offset : source.payload_offset + len(source.raw_bytes)
            ]
        )
    context = measurement.coordinate_measurement_context()
    assert context.paper_scale_factor == 0.04 and context.annotation_ratio == 0.5
    assert context.drawing_denominator == 25 and context.annotation_denominator == 50
    assert context.length_on_paper_mm(1000) == 40
    assert context.length_at_current_annotation_mm(1000) == 2000
    effective = measurement.effective_line_style(2)
    assert tuple(
        getattr(effective.styles, key).index for key in ("color", "line_type", "width")
    ) == (3, 4, 5)
    assert [s.kind for s in effective.styles.color.sources] == [
        "record",
        "layer",
        "color",
    ]
    from ezdracad.dimension_scale import DimensionScaleProfile

    dimension = ezdracad.raw.scanfile(args.fixture.parent / "dimension25.cfb")
    dimension_scale = dimension.dimension_scale(
        0, profile=DimensionScaleProfile.MARKER25_ALIGNED_NUMERIC_MM_V1
    )
    assert dimension_scale.recorded_value_mm == 2000
    assert dimension_scale.stored_span_length == 1000
    assert dimension_scale.annotation_to_drawing_ratio == 2
    assert dimension_scale.equivalent_annotation_denominator == 200
    assert dimension_scale.original_drawing_denominator is None
    assert dimension_scale.original_annotation_denominator is None
    assert dimension_scale.geometry_units is None
    assert dimension_scale.coordinate_space == "record_local"
    assert dimension_scale.document_complete is False
    assert dimension_scale.record_bytes == dimension.read_contents()[8192:8832]

    inherited = ezdracad.raw.scanfile(args.fixture.parent / "inheritance25.cfb")
    world_input = inherited.world_input_3d_context(
        profile=WorldInput3DProfile.MARKER25_LOADED_WORLD_INPUT_V1
    )
    assert world_input.units == "mm"
    assert world_input.origin_z_source == "loaded_file_zero_z_convention"
    assert world_input.loaded_world_origin[2] == 0
    assert world_input.evidence.header_bytes == inherited.read_contents()[0x140:0x280]
    assert world_input.input_mm_from_world_stored_xyz(
        world_input.loaded_world_origin
    ) == (0, 0, 0)
    assert world_input.input_mm_from_world_stored_xyz(
        world_input.world_stored_xyz_from_input_mm((20, 10, 5))
    ) == (20, 10, 5)
    # A memory-only synthetic preset exercises the installed owned-handle path;
    # the historical fixture file remains unchanged and contains no native data.
    plane_envelope = bytearray((args.fixture.parent / "inheritance25.cfb").read_bytes())
    plane_payload = inherited.read_contents()
    assert plane_envelope[1536 : 1536 + len(plane_payload)] == plane_payload
    struct.pack_into("<3d", plane_envelope, 1536 + 0x450, 3, -5, 7)
    struct.pack_into("<i", plane_envelope, 1536 + 0x480, 1)
    plane = ezdracad.raw.scan_records(bytes(plane_envelope)).workplane_context(
        profile=WorkplaneProfile.MARKER25_SAVED_WORKPLANE_V1
    )
    assert plane.units is None and plane.origin == (3, -5, 7)
    assert plane.from_world_stored_xyz((4, -3, 10)) == (1, 2, 3)
    assert plane.to_world_stored_xyz((1, 2, 3)) == (4, -3, 10)
    assert plane.evidence.header_bytes == plane_envelope[1536 + 0x450 : 1536 + 0x4D0]
    placed_scan = ezdracad.raw.scanfile(args.fixture.parent / "block3d25.cfb")
    placed = placed_scan.placed_block_lines_3d(
        0, profile=Block3DProfile.MARKER25_DIRECT_LINE3D_V1
    )
    assert placed.reference.scale == (2, 3, 0.5)
    assert placed.reference.record_scope == "root_records"
    assert placed.reference.apply_xyz((7, 11, 13)) == (114, -17, 31.5)
    assert placed.reference.evidence.records == tuple(
        placed_scan.record_bytes(i) for i in range(3)
    )
    assert (
        placed.definition.flag_raw == 1 and len(placed.definition.header_bytes) == 176
    )
    assert placed.lines[0].source.start == (1.25, -2.5, 12.5)
    assert placed.lines[0].start == (102.5, -57.5, 31.25)
    assert placed.lines[0].coordinate_space == "block_parent_stored_xyz"
    assert placed.units is None and placed.document_complete is False
    from ezdracad.coordinate_space import CoordinateFrame, CoordinateSpaceProfile
    from ezdracad.qualification import Line2DProfile
    from ezdracad.unit_resolution import DeclaredCreationScale

    resolution = inherited.unit_resolution()
    assert resolution.physical_units.status.value == "unresolved"
    assert resolution.original_creation_scale.value == "unavailable_from_decoded_fields"
    assert (
        resolution.physical_units.reason.value
        == "stored_coordinate_unit_not_established"
    )
    input_unit = inherited.coordinate_input_unit()
    assert input_unit.unit.value == "mm" and input_unit.native_display_factor == 1
    assert input_unit.to_native_mm(-2.5) == -2.5
    assert input_unit.from_native_mm(2.5) == 2.5
    assert resolution.coordinate_input_unit.origin_units == input_unit.origin_units
    declared = DeclaredCreationScale(25, 100)
    assert declared.length_mm(1000) == 4000
    assert declared.provenance.value == "caller_declared_unverified"
    frame_context = inherited.coordinate_space_context(
        profile=CoordinateSpaceProfile.MARKER25_STORED_AXES_V1
    )
    assert frame_context.evidence.header_bytes == inherited.read_contents()[0x140:0x280]
    assert frame_context.input_unit.origin_units == input_unit.origin_units
    point = frame_context.from_root_stored_xy(
        (10, 20), frame=CoordinateFrame.ROOT_STORED_XY
    )
    assert point == (10, 20)

    for reference, triple in ((0, (12, 13, 14)), (2, (13, 14, 15))):
        lines = inherited.block_library().effective_line_styles(reference)
        scene = inherited.scene_lines(reference, limits=SceneLimits())
        assert len(scene) == len(lines) == 2
        assert scene[0].source == lines[0].source
        assert scene[0].styles == lines[0].styles
        assert scene[0].ancestors[0].kind == "block_reference"
        assert scene[0].units is None
        qualified = inherited.qualified_lines(
            reference, profile=Line2DProfile.MARKER25_PLANAR_STORED_XY_V1
        )
        assert len(qualified.lines) == len(scene)
        assert qualified.lines[0].source == scene[0].source
        assert qualified.lines[0].styles == scene[0].styles
        assert qualified.coordinate_semantics == "stored_xy"
        assert qualified.units is None and qualified.document_complete is False
        assert "physical_units" in qualified.unresolved_semantics
        framed = inherited.qualified_lines_in_coordinate_space(
            reference,
            line_profile=Line2DProfile.MARKER25_PLANAR_STORED_XY_V1,
            space_profile=CoordinateSpaceProfile.MARKER25_STORED_AXES_V1,
            frame=CoordinateFrame.CURRENT_ABSOLUTE_INPUT_MM_FROM_DRAWING_ORIGIN,
        )
        assert framed.source == qualified and len(framed.lines) == len(scene)
        assert framed.lines[0].source is framed.source.lines[0]
        assert framed.units == "mm" and framed.lines[0].source.units is None
        width = resolve_nominal_print_width(
            scene[0].styles,
            inherited.style_tables(),
            PrintWidthPolicy(100, device_context="application_managed_opaque_vectors"),
        )
        assert width.status == "resolved_nominal_mm"
        assert width.selected_index == triple[2]
        assert (
            width.nominal_paper_mm
            == inherited.style_tables().width(triple[2]).pen_width_value
        )
        assert [
            tuple(
                getattr(line.styles, key).index
                for key in ("color", "line_type", "width")
            )
            for line in lines
        ] == [triple, (7, 8, 9)]
        assert lines[0].coordinate_space == "package_parent" and lines[0].units is None
        for property in (
            lines[0].styles.color,
            lines[0].styles.line_type,
            lines[0].styles.width,
        ):
            assert any(source.kind == "block_placement" for source in property.sources)
            for source in property.sources:
                assert (
                    source.raw_bytes
                    == inherited.read_contents()[
                        source.payload_offset : source.payload_offset
                        + len(source.raw_bytes)
                    ]
                )
    drawing = args.fixture.parent / "document25.cfb"
    document = ezdracad.readfile(drawing, policy="partial")
    assert len(document.query("LINE")) == 2
    assert document.query("LINE")[0].start == (-12.5, 0.25)
    assert document.raw().sha256 == document.report.contents_sha256
    assert document.report.document_complete is False
    try:
        ezdracad.readfile(drawing)
    except ezdracad.UnsupportedFeatureError as error:
        assert error.report.entity_count == 2
    else:
        raise AssertionError("strict drawing read accepted unresolved semantics")
    bounded = ezdracad.readfile(drawing, scope="stored_xy_lines")
    assert (
        bounded.report.status == "complete"
        and bounded.report.document_complete is False
    )
    command = subprocess.run(
        [sys.executable, "-m", "ezdracad", "extract", str(drawing), "--partial"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert command.returncode == 2 and not command.stderr
    extracted = json.loads(command.stdout)
    assert len(extracted["entities"]) == 2
    assert extracted["report"]["document_complete"] is False
    basic_path = args.fixture.parent / "basic2d25.cfb"
    basic = ezdracad.readfile(
        basic_path,
        basic_profile="marker25_root_primitives_v1",
        scope="stored_xy_primitives",
    )
    assert len(basic.entities) == 7 and basic.report.handled_auxiliary_object_count == 2
    assert basic.report.status == "complete" and not basic.report.document_complete
    assert (
        basic.report.basic_profile
        == ezdracad.Basic2DProfile.MARKER25_ROOT_PRIMITIVES_V1
    )
    assert [e.type for e in basic.entities] == [
        "LINE",
        "CIRCLE",
        "ARC",
        "TEXT",
        "TEXT",
        "CIRCLE",
        "LINE",
    ]
    assert [e.entity_id for e in basic.query("CIRCLE")] == [1, 5]
    assert basic.query("CIRCLE")[0].center == (10.25, -20.5)
    assert basic.query("ARC")[0].start_angle_degrees == 270
    text = basic.query("TEXT")[0]
    assert text.text == "合成図面β" and text.font_name == "Synthetic Font"
    assert (text.width, text.height, text.rotation_degrees) == (2.5, 4, 17)
    assert len(basic.query("TEXT")[1].body_bytes) == 96
    for e in basic.entities:
        if e.type == "LINE":
            continue
        ref = e.source_ref
        assert (
            e.raw_bytes
            == basic.raw().read_contents()[
                ref.payload_offset : ref.payload_offset + ref.byte_length
            ]
        )
        assert e.attributes.header_bytes == e.raw_bytes[:16]
        assert (
            e.styles.color.index,
            e.styles.line_type.index,
            e.styles.width.index,
        ) == (3, 2, 4)
    command = subprocess.run(
        [
            str(cli),
            "extract",
            str(basic_path),
            "--entities",
            "ARC",
            "--scope",
            "stored_xy_primitives",
            "--basic-profile",
            "marker25_root_primitives_v1",
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    mixed = json.loads(command.stdout)
    assert len(mixed["entities"]) == 1 and mixed["entities"][0]["type"] == "ARC"
    assert mixed["query"]["decoded_entity_count"] == 7
    # Flat MPX is a distinct owned file payload, not a fabricated CFB stream.
    flat_path = args.fixture.parent / "mpx25.mpx"
    flat_bytes = flat_path.read_bytes()
    flat_info = ezdracad.raw.inspectfile(flat_path)
    assert flat_info.format == "mpx" and flat_info.cfb is None
    assert flat_info.payload.stream_path is None
    assert flat_info.payload.offset_domain == "file"
    assert flat_info.read_payload() == flat_info.read_contents() == flat_bytes
    assert flat_info.payload.sha256 == hashlib.sha256(flat_bytes).hexdigest()
    assert flat_info.to_dict()["cfb"] is None
    flat_scan = ezdracad.raw.scanfile(flat_path)
    assert flat_scan.format == "mpx" and flat_scan.offset_domain == "file"
    assert flat_scan.stream_path is None
    assert flat_scan.header.payload_end_field_raw == 0
    assert flat_scan.header.payload_end_basis == "file_length"
    assert flat_scan.header.payload_end == len(flat_bytes)
    assert struct.unpack_from("<I", flat_scan.read_payload(), 0x50)[0] == 0
    flat = ezdracad.readfile(flat_path, scope="stored_xy_lines")
    assert flat.report.source_format == "mpx" and flat.report.offset_domain == "file"
    assert flat.report.stream_path is None
    assert flat.report.payload_sha256 == hashlib.sha256(flat_bytes).hexdigest()
    assert flat.report.status == "complete" and not flat.report.document_complete
    assert len(flat.entities) == 2
    assert flat.entities[0].start == (-12.5, 0.25)
    for entity in flat.entities:
        source = entity.source_ref
        assert source.stream_path is None and source.offset_domain == "file"
        assert (
            entity.source.header_bytes
            == flat_bytes[source.payload_offset : source.payload_offset + 16]
        )
    assert all(region.offset_domain == "file" for region in flat.report.opaque_regions)
    block_path = args.fixture.parent / "mpx-block25.mpx"
    block_bytes = block_path.read_bytes()
    assert len(block_bytes) % 64 == 48
    block = ezdracad.readfile(block_path, scope="stored_xy_lines")
    assert len(block.entities) == 1
    entity = block.entities[0]
    assert entity.start == (105.0, -47.5) and entity.end == (83.75, -40.5)
    source = entity.source_ref
    assert (
        source.table_domain == "block_definition_records"
        and source.definition_index == 0
    )
    assert source.stream_path is None and source.offset_domain == "file"
    assert (
        entity.source.header_bytes
        == block_bytes[source.payload_offset : source.payload_offset + 16]
    )
    mixed_mpx = args.fixture.parent / "mpx-mixed25.mpx"
    assert ezdracad.raw.scanfile(mixed_mpx).read_payload() == mixed_mpx.read_bytes()
    # CFB has retained its original stream-relative interpretation.
    assert document.report.source_format == "mpz"
    assert document.report.offset_domain == "cfb_stream"
    assert document.report.stream_path == "/Contents"
    assert document.raw().header.payload_end_basis == "declared_field"
    try:
        ezdracad.raw.inspect((args.fixture.parent / "document25.bin").read_bytes())
    except ezdracad.UnsupportedFormatError:
        pass
    else:
        raise AssertionError("Extracted Contents was misidentified as a native MPX")
    command = subprocess.run(
        [str(cli), "inspect", str(flat_path), "--json"],
        capture_output=True,
        text=True,
        check=True,
    )
    flat_metadata = json.loads(command.stdout)
    assert flat_metadata["format"] == "mpx" and flat_metadata["cfb"] is None
    assert flat_metadata["payload"]["offset_domain"] == "file"
    command = subprocess.run(
        [str(cli), "extract", str(flat_path), "--scope", "stored_xy_lines"],
        capture_output=True,
        text=True,
        check=True,
    )
    flat_metadata = json.loads(command.stdout)
    assert flat_metadata["report"]["source_format"] == "mpx"
    assert flat_metadata["report"]["offset_domain"] == "file"
    assert len(flat_metadata["entities"]) == 2
    # MPS wrapper offsets and decoded payload offsets remain distinct.
    assert ezdracad.ParseLimits().max_decompressed_size == 128 * 1024 * 1024
    assert _core.DEFAULT_MAX_DECOMPRESSED_SIZE == 128 * 1024 * 1024
    for fixture_name, count in (
        ("mps-document25", 2),
        ("mps-basic2d25", 7),
        ("mps-block25", 1),
    ):
        mps_path = args.fixture.parent / (fixture_name + ".mps")
        mps_info = ezdracad.raw.inspectfile(mps_path)
        assert mps_info.format == "mps" and mps_info.cfb is not None
        assert mps_info.payload.stream_path is None
        assert mps_info.payload.offset_domain == "decoded_payload"
        mps_payload = mps_info.read_payload()
        assert mps_info.read_contents() == mps_payload
        assert mps_info.payload.size == len(mps_payload)
        assert mps_info.payload.sha256 == hashlib.sha256(mps_payload).hexdigest()
        if fixture_name != "mps-block25":
            source_name = (
                "basic2d25.bin" if fixture_name == "mps-basic2d25" else "document25.bin"
            )
            assert mps_payload == (args.fixture.parent / source_name).read_bytes()
        wrapper = mps_info.mps
        assert wrapper.stream_path == "/SecureContents"
        assert wrapper.wrapper_profile == "unprotected_base_v1"
        assert wrapper.compression == "zlib" and wrapper.decoded_size == len(
            mps_payload
        )
        assert len(wrapper.stream_sha256) == 64
        assert any(
            entry.path == wrapper.stream_path and entry.size == wrapper.stream_size
            for entry in mps_info.cfb.entries
        )
        assert len(wrapper.chunks) == 1
        chunk = wrapper.chunks[0]
        assert chunk.encoded_offset == 130
        assert chunk.encoded_offset + chunk.encoded_size == wrapper.stream_size
        assert chunk.decoded_offset == 0 and chunk.decoded_size == len(mps_payload)
        options = dict(scope="stored_xy_lines")
        if fixture_name == "mps-basic2d25":
            options = dict(
                scope="stored_xy_primitives",
                basic_profile="marker25_root_primitives_v1",
            )
        mps = ezdracad.readfile(mps_path, **options)
        assert len(mps.entities) == count
        assert mps.report.source_format == "mps"
        assert mps.report.offset_domain == "decoded_payload"
        assert mps.report.stream_path is None
        assert mps.report.payload_sha256 == mps_info.payload.sha256
        assert mps.report.status == "complete" and not mps.report.document_complete
        assert mps.report.units is None
        assert mps.raw().mps == wrapper
        assert mps.raw().read_payload() == mps_payload
        assert mps.raw().header.payload_end_basis == "declared_field"
        assert mps.raw().header.payload_end + 4 == len(mps_payload)
        assert mps.raw().to_dict()["mps"] == mps_info.to_dict()["mps"]
        for entity in mps.entities:
            source = entity.source_ref
            assert (
                source.offset_domain == "decoded_payload" and source.stream_path is None
            )
            assert source.payload_offset + source.byte_length <= len(mps_payload)
            if entity.type == "LINE":
                assert (
                    entity.source.header_bytes
                    == mps_payload[source.payload_offset : source.payload_offset + 16]
                )
            else:
                assert (
                    entity.raw_bytes
                    == mps_payload[
                        source.payload_offset : source.payload_offset
                        + source.byte_length
                    ]
                )
        assert all(
            region.offset_domain == "decoded_payload" and region.stream_path is None
            for region in mps.report.opaque_regions
        )
        try:
            ezdracad.readfile(mps_path)
        except ezdracad.IncompleteReadError as error:
            assert error.report.source_format == "mps"
            assert error.report.offset_domain == "decoded_payload"
            assert error.report.document_complete is False
        else:
            raise AssertionError(
                "MPS strict drawing read accepted unresolved semantics"
            )
        try:
            ezdracad.raw.inspectfile(
                mps_path, limits=ezdracad.ParseLimits(max_decompressed_size=1)
            )
        except ezdracad.LimitExceededError as error:
            assert error.resource == "decompressed_size" and error.actual == len(
                mps_payload
            )
        else:
            raise AssertionError("installed MPS decompression limit was ignored")
        command = subprocess.run(
            [str(cli), "inspect", str(mps_path), "--json"],
            capture_output=True,
            text=True,
            check=True,
        )
        assert json.loads(command.stdout) == mps_info.to_dict()
    mps_block = ezdracad.readfile(
        args.fixture.parent / "mps-block25.mps", scope="stored_xy_lines"
    )
    assert mps_block.entities[0].source_ref.table_domain == "block_definition_records"
    assert mps_block.entities[0].start == (105, -47.5)
    assert mps_block.entities[0].end == (83.75, -40.5)
    # Existing primitive qualification remains opt-in after wrapper decoding.
    mps_basic_path = args.fixture.parent / "mps-basic2d25.mps"
    guarded = ezdracad.readfile(
        mps_basic_path, policy="partial", scope="stored_xy_lines"
    )
    assert (
        guarded.report.geometry_status == "partial"
        and guarded.report.unsupported_counts
    )
    assert all(entity.type == "LINE" for entity in guarded.entities)
    command = subprocess.run(
        [
            str(cli),
            "extract",
            str(args.fixture.parent / "mps-document25.mps"),
            "--scope",
            "stored_xy_lines",
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    mps_metadata = json.loads(command.stdout)
    assert mps_metadata["report"]["source_format"] == "mps"
    assert mps_metadata["report"]["offset_domain"] == "decoded_payload"
    assert len(mps_metadata["entities"]) == 2
    command = subprocess.run(
        [
            str(cli),
            "inspect",
            str(mps_basic_path),
            "--json",
            "--max-decompressed-size",
            "1",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert command.returncode == 3
    assert json.loads(command.stderr)["resource"] == "decompressed_size"
    print(f"Installed distribution verified: {package}")


if __name__ == "__main__":
    main()
