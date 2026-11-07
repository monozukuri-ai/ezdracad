"""Synthetic named-frame contracts; native input evidence is checked separately."""

import struct
from dataclasses import FrozenInstanceError

import pytest
from test_scene import assembler, fixture, line, reference, scan

from ezdracad import InvalidFormatError, UnsupportedFeatureError, raw
from ezdracad.coordinate_space import (
    CoordinateFrame,
    CoordinateSpaceContext,
    CoordinateSpaceProfile,
    FramedLineSelection,
)
from ezdracad.qualification import Line2DProfile

SPACE = CoordinateSpaceProfile.MARKER25_STORED_AXES_V1
LINE = Line2DProfile.MARKER25_PLANAR_STORED_XY_V1
FRAME = CoordinateFrame.CURRENT_ANNOTATION_MM_FROM_DRAWING_ORIGIN


def context_data(data=None):
    if data is None:
        data = fixture([line()])
    data = bytearray(data)
    struct.pack_into("<2di", data, 0x140, 100, -50, 0)
    struct.pack_into("<d", data, 0x178, 0.01)
    struct.pack_into("<d", data, 0x278, 0.5)
    return data


def test_context_requires_explicit_profile_and_owns_exact_header_after_source_removal(
    tmp_path,
):
    data = context_data()
    path = tmp_path / "context.mpz"
    path.write_bytes(assembler.wrap_record_contents(data))
    document = raw.scanfile(path)
    path.unlink()
    with pytest.raises(TypeError):
        document.coordinate_space_context()
    with pytest.raises(TypeError):
        document.coordinate_space_context(SPACE)
    context = document.coordinate_space_context(profile=SPACE)
    assert isinstance(context, CoordinateSpaceContext)
    assert context.profile is SPACE
    assert context.drawing_origin == (100, -50)
    assert context.drawing_origin_offset == 0x140
    assert context.drawing_origin_bytes == struct.pack("<2d", 100, -50)
    assert context.origin_units_offset == 0x150
    assert context.origin_units_raw == 0 and context.origin_units_bytes == bytes(4)
    assert context.evidence.header_offset == 0x140
    assert context.evidence.header_bytes == data[0x140:0x280]
    assert context.paper_scale_factor == 0.01
    assert context.annotation_ratio == 0.5
    assert context.drawing_denominator == 100
    assert context.annotation_denominator == 200
    assert context.draft.angle_offset == 0x1B0
    assert context.draft.object_offset == 0x1AC
    assert context.draft.enable_offset == 0x274
    assert context.draft.angle_degrees == 0
    assert context.draft.object_raw == context.draft.enable_raw == 0
    with pytest.raises(FrozenInstanceError):
        context.evidence.header_bytes = b""


def test_explicit_named_maps_and_inverse_keep_input_context_separate_from_source_units():
    context = scan(context_data()).coordinate_space_context(profile=SPACE)
    for frame, value in (
        (CoordinateFrame.ROOT_STORED_XY, (110, -45)),
        (CoordinateFrame.DRAWING_ORIGIN_RELATIVE_STORED_XY, (10, 5)),
        (FRAME, (20, 10)),
        (CoordinateFrame.CONFIGURED_PAPER_MM_OFFSET_FROM_DRAWING_ORIGIN, (0.1, 0.05)),
    ):
        assert context.from_root_stored_xy((110, -45), frame=frame) == value
        assert context.to_root_stored_xy(value, frame=frame) == (110, -45)
    assert CoordinateFrame.ROOT_STORED_XY.units is None
    assert FRAME.units == "mm"
    with pytest.raises(TypeError):
        context.from_root_stored_xy((110, -45))
    with pytest.raises(UnsupportedFeatureError):
        context.from_root_stored_xy((110, -45), frame="wcs")


def test_same_snapshot_qualification_places_first_and_retains_every_source():
    data = context_data(
        fixture(
            reference(b"A", 5, (2, 2, 5), (100, 50, 0, 2, 2, 90)),
            [(b"A", [line()])],
        )
    )
    document = scan(data)
    selection = document.qualified_lines_in_coordinate_space(
        0, line_profile=LINE, space_profile=SPACE, frame=FRAME
    )
    assert isinstance(selection, FramedLineSelection)
    source = document.qualified_lines(0, profile=LINE)
    assert selection.source == source
    assert not selection.document_complete
    assert selection.units == "mm" and selection.frame is FRAME
    value = selection.lines[0]
    assert value.source == source.lines[0]
    assert value.source.start == pytest.approx((96, 52))
    assert value.source.source.start == (1, 2)
    assert value.start == pytest.approx((-8, 204))
    assert value.end == pytest.approx((-16, 212))
    assert value.source.units is None
    assert {"physical_units", "drawing_wcs", "layout", "visibility"} <= set(
        value.unresolved_semantics
    )
    assert value.source.attributes.material_index_raw == 19
    assert value.source.attributes.group_index == 1234
    assert len(value.source.ancestors) == 1
    assert value.source.ancestors[0].raw_bytes == data[8192:8320]
    with pytest.raises(TypeError):
        document.qualified_lines_in_coordinate_space(0, line_profile=LINE, frame=FRAME)


@pytest.mark.parametrize("point", [None, "1,2", [1], [1, 2, 3], [True, 1], [1, "2"]])
def test_point_shape_and_boolean_coordinates_are_rejected(point):
    context = scan(context_data()).coordinate_space_context(profile=SPACE)
    with pytest.raises(TypeError):
        context.from_root_stored_xy(point, frame=FRAME)
    with pytest.raises(TypeError):
        context.to_root_stored_xy(point, frame=FRAME)


@pytest.mark.parametrize(
    "at,value",
    [
        (0x140, float("nan")),
        (0x148, float("inf")),
        (0x178, 0),
        (0x178, -0.1),
        (0x278, 0),
        (0x278, float("nan")),
        (0x178, 5e-324),
        (0x1B0, float("nan")),
    ],
)
def test_nonfinite_and_nonpositive_metadata_never_construct_context(at, value):
    data = context_data()
    struct.pack_into("<d", data, at, value)
    with pytest.raises(InvalidFormatError):
        scan(data).coordinate_space_context(profile=SPACE)


def test_unknown_units_and_profiles_do_not_create_contextual_physical_claims():
    for units in (-1, 7, 2147483647):
        data = context_data()
        struct.pack_into("<i", data, 0x150, units)
        with pytest.raises(UnsupportedFeatureError):
            scan(data).coordinate_space_context(profile=SPACE)
    for marker in (5, 8):
        data = context_data(assembler.make_semantics_contents(marker))
        with pytest.raises(UnsupportedFeatureError):
            scan(data).coordinate_space_context(profile=SPACE)
        context = scan(data).coordinate_space_context(
            profile=f"marker{marker}_stored_axes_v1"
        )
        assert context.from_root_stored_xy((110, -45), frame=FRAME) == (20, 10)
        with pytest.raises(UnsupportedFeatureError):
            scan(data).qualified_lines_in_coordinate_space(
                0,
                line_profile=LINE,
                space_profile=f"marker{marker}_stored_axes_v1",
                frame=FRAME,
            )
    with pytest.raises(UnsupportedFeatureError):
        scan(context_data()).coordinate_space_context(profile="automatic")


def test_point_arithmetic_rejects_overflow_underflow_and_lost_translation():
    context = scan(context_data()).coordinate_space_context(profile=SPACE)
    for value in ((float("nan"), 0), (1, float("inf")), (1.7976931348623157e308, 0)):
        with pytest.raises(InvalidFormatError):
            context.from_root_stored_xy(value, frame=FRAME)
    with pytest.raises(InvalidFormatError):
        context.to_root_stored_xy((5e-324, 0), frame=FRAME)
    with pytest.raises(InvalidFormatError):
        context.to_root_stored_xy(
            (2e-16, 0), frame=CoordinateFrame.DRAWING_ORIGIN_RELATIVE_STORED_XY
        )


def test_unqualified_saved_axes_fail_before_any_normalized_selection_is_returned():
    for offset, value, encoding in (
        (0x1AC, 2, "<i"),
        (0x274, 2, "<i"),
        (0x1B0, 361, "<d"),
    ):
        data = context_data()
        struct.pack_into(encoding, data, offset, value)
        document = scan(data)
        with pytest.raises(UnsupportedFeatureError):
            document.coordinate_space_context(profile=SPACE)
        with pytest.raises(UnsupportedFeatureError):
            document.qualified_lines_in_coordinate_space(
                0, line_profile=LINE, space_profile=SPACE, frame=FRAME
            )


@pytest.mark.parametrize("angle,mode", [(30, 0), (30, 1), (90, 0)])
def test_active_drafter_input_coordinates_are_separate_from_stored_axis_metric(
    angle, mode
):
    data = context_data()
    struct.pack_into("<i", data, 0x1AC, mode)
    struct.pack_into("<d", data, 0x1B0, angle)
    struct.pack_into("<i", data, 0x274, 1)
    context = scan(data).coordinate_space_context(profile=SPACE)
    stored = (95, -40) if angle == 90 else (106.16025403784439, -40.66987298107781)
    target = CoordinateFrame.CURRENT_ABSOLUTE_INPUT_MM_FROM_DRAWING_ORIGIN
    assert context.from_root_stored_xy(stored, frame=target) == pytest.approx((20, 10))
    assert context.to_root_stored_xy((20, 10), frame=target) == pytest.approx(stored)
    assert context.from_root_stored_xy(stored, frame=FRAME) != pytest.approx((20, 10))


def test_known_input_units_preserve_stored_geometry_and_expose_literal_conversion():
    data = context_data()
    struct.pack_into("<i", data, 0x150, 1)
    document = scan(data)
    context = document.coordinate_space_context(profile=SPACE)
    assert context.input_unit.origin_units.raw_code == 1
    assert context.input_unit.origin_units.selector_bytes == struct.pack("<i", 1)
    assert context.input_unit.unit.value == "cm"
    assert context.input_unit.native_display_factor == 0.1
    target = CoordinateFrame.CURRENT_ABSOLUTE_INPUT_MM_FROM_DRAWING_ORIGIN
    native_mm = context.from_root_stored_xy((0, -100), frame=target)
    assert native_mm == (-200, -100)
    assert tuple(context.input_unit.from_native_mm(x) for x in native_mm) == (-20, -10)
    assert context.to_root_stored_xy(native_mm, frame=target) == (0, -100)
    result = document.qualified_lines_in_coordinate_space(
        0, line_profile=LINE, space_profile=SPACE, frame=target
    )
    assert result.source.lines[0].source.start == (1, 2)
    assert result.source.lines[0].units is None
    legacy = context_data(assembler.make_semantics_contents(8))
    struct.pack_into("<i", legacy, 0x150, 1)
    with pytest.raises(UnsupportedFeatureError):
        scan(legacy).coordinate_space_context(profile="marker8_stored_axes_v1")
