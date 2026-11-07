"""Versioned Line2D contracts using independently authored synthetic scenes."""

import struct
from dataclasses import FrozenInstanceError

import pytest
from test_scene import assembler, fixture, inline, line, reference, scan

from ezdracad import (
    InvalidFormatError,
    LimitExceededError,
    UnsupportedFeatureError,
    raw,
)
from ezdracad.qualification import (
    LINE2D_UNRESOLVED_SEMANTICS,
    Line2D,
    Line2DProfile,
    QualifiedLineSelection,
)
from ezdracad.scene import SceneLimits

PROFILE = Line2DProfile.MARKER25_PLANAR_STORED_XY_V1


def numeric_line(start, end):
    result = line()
    struct.pack_into("<4d", result, 16, *start, *end)
    return result


@pytest.mark.parametrize("profile", list(Line2DProfile))
@pytest.mark.parametrize(
    "placement,start,end",
    [
        ((1e300, 1e300, 0, 1, 1, 0), (1, 2), (3, 4)),
        ((1e300, 0, 0, 1, 1, 0), (1, 2), (3, 4)),
        ((0, 0, 0, 1e-200, 1e-200, 0), (1e-200, 2e-200), (3e-200, 4e-200)),
        ((1e30, 0, 0, 1, 1, 0), (1e20, 2), (1e20 + 16384, 4)),
    ],
)
def test_finite_placement_cannot_qualify_lost_coordinates_or_displacements(
    profile, placement, start, end
):
    data = fixture(
        reference(b"A", placement=placement), [(b"A", [numeric_line(start, end)])]
    )
    document = scan(data)
    assert len(document.scene_lines(0)) == 1
    with pytest.raises(InvalidFormatError):
        document.qualified_lines(0, profile=profile)
    assert document.read_contents() == data


@pytest.mark.parametrize("profile", list(Line2DProfile))
@pytest.mark.parametrize(
    "angle,start,end",
    [
        (0, (100, 51), (100, 52)),
        (90, (99, 50), (98, 50)),
        (180, (100, 49), (100, 48)),
        (270, (101, 50), (102, 50)),
        (-90, (101, 50), (102, 50)),
        (450, (99, 50), (98, 50)),
    ],
)
def test_cardinal_placements_keep_exact_axes_and_all_original_bytes(
    profile, angle, start, end
):
    leaf = numeric_line((0, 1), (0, 2))
    references = reference(b"A", placement=(100, 50, 0, 1, 1, angle))
    data = fixture(references, [(b"A", [leaf])])
    document = scan(data)
    value = document.qualified_lines(0, profile=profile).lines[0]
    assert value.start == start and value.end == end
    assert value.source.start == (0, 1) and value.source.end == (0, 2)
    assert value.source.header_bytes == leaf[:16]
    assert value.ancestors[0].raw_bytes == b"".join(references)
    a, b, c, d, tx, ty = value.cumulative_transform
    assert (a * 0 + c * 1 + tx, b * 0 + d * 1 + ty) == start
    assert document.read_contents() == data


@pytest.mark.parametrize("profile", list(Line2DProfile))
@pytest.mark.parametrize(
    "start,end",
    [
        ((float.fromhex("0x1.fffffffffffffp+1023"), -1e308), (-1e308, 1e308)),
        (
            (float.fromhex("0x0.0000000000001p-1022"), 0),
            (float.fromhex("0x0.0000000000002p-1022"), 0),
        ),
    ],
)
def test_identity_root_lines_keep_finite_extremes(profile, start, end):
    value = (
        scan(fixture([numeric_line(start, end)]))
        .qualified_lines(0, profile=profile)
        .lines[0]
    )
    assert value.start == start and value.end == end


@pytest.mark.parametrize("profile", list(Line2DProfile))
def test_source_zero_length_stays_raw_and_scene_only(profile):
    data = fixture([numeric_line((3.125, -7.5), (3.125, -7.5))])
    document = scan(data)
    assert len(document.scene_lines(0)) == 1
    assert len(document.line_records(0, 1)) == 1
    with pytest.raises(UnsupportedFeatureError, match="zero-length"):
        document.qualified_lines(0, profile=profile)
    assert document.read_contents() == data


@pytest.mark.parametrize("profile", list(Line2DProfile))
@pytest.mark.parametrize(
    "angle",
    [
        float.fromhex("0x0.0000000000001p-1022"),
        -float.fromhex("0x0.0000000000001p-1022"),
    ],
)
def test_noncardinal_degree_underflow_cannot_erase_representable_rotation(
    profile, angle
):
    data = fixture(
        reference(b"A", placement=(0, 0, 0, 1, 1, angle)),
        [(b"A", [numeric_line((1e308, 0), (1.1e308, 0))])],
    )
    document = scan(data)
    assert len(document.scene_lines(0)) == 1
    with pytest.raises(InvalidFormatError, match="angle conversion underflow"):
        document.qualified_lines(0, profile=profile)
    assert document.read_contents() == data


@pytest.mark.parametrize("profile", list(Line2DProfile))
@pytest.mark.parametrize("angle", [1e-300, -1e-300])
def test_tiny_signed_angle_is_not_rounded_to_a_cardinal_orientation(profile, angle):
    data = fixture(
        reference(b"A", placement=(0, 0, 0, 1, 1, angle)),
        [(b"A", [numeric_line((1e308, 0), (1.1e308, 0))])],
    )
    value = scan(data).qualified_lines(0, profile=profile).lines[0]
    assert 1.7e6 < abs(value.start[1]) < 1.8e6
    assert 1.9e6 < abs(value.end[1]) < 2e6
    assert (value.start[1] < 0) is (angle < 0)


@pytest.mark.parametrize("profile", list(Line2DProfile))
def test_native_observed_layout_word_cannot_be_masked_into_a_model_layer(profile):
    from test_scene import slot

    model0 = numeric_line((0, 0), (10000, 0))
    model1 = numeric_line((0, 0), (0, 5000))
    paper = numeric_line((10, 15), (30, 35))
    struct.pack_into("<H", paper, 2, 0x0201)
    data = fixture(
        [
            model0,
            model1,
            slot(0x041D, 4),
            slot(0xFE, 0),
            slot(0xFE, 0),
            slot(0x52, 0),
            paper,
            slot(0x1E, 4),
            slot(0xFE, 0),
            slot(0xFE, 0),
            slot(0x51, 0),
        ]
    )
    document = scan(data)
    assert len(document.line_records(0, document.header.record_count)) == 3
    assert document.qualified_lines(0, profile=profile).lines[0].end == (10000, 0)
    assert document.qualified_lines(1, profile=profile).lines[0].end == (0, 5000)
    for index in range(2, 11):
        with pytest.raises(UnsupportedFeatureError):
            document.qualified_lines(index, profile=profile)
    assert document.read_contents() == data
    struct.pack_into("<H", model0, 2, 256)
    assert scan(fixture([model0])).qualified_lines(0, profile=profile).lines


def test_native_observed_terminal_variant_keeps_contents_without_raw_qualification():
    data = fixture([numeric_line((0, 0), (10000, 0))])
    terminal = struct.unpack_from("<I", data, 0x28)[0] - 64
    data[terminal : terminal + 16] = bytes.fromhex("ff040001010000000101010004000000")
    container = raw.inspect(assembler.wrap_record_contents(data))
    assert container.format == "mpz" and container.read_contents() == data
    with pytest.raises(InvalidFormatError, match="missing terminal record"):
        scan(data)
    assert container.read_contents() == data


def test_qualification_is_explicit_stored_xy_and_does_not_complete_unknown_objects(
    tmp_path,
):
    from test_scene import slot

    data = fixture([line(), slot(0x7777)])
    path = tmp_path / "bounded.mpz"
    path.write_bytes(assembler.wrap_record_contents(data))
    document = raw.scanfile(path)
    path.unlink()
    with pytest.raises(TypeError):
        document.qualified_lines(0)
    with pytest.raises(TypeError):
        document.qualified_lines(0, profile="marker25_planar_stored_xy_v1")
    result = document.qualified_lines(0, profile=PROFILE)
    assert isinstance(result, QualifiedLineSelection)
    assert result.profile is PROFILE and result.record_index == 0
    assert result.coordinate_semantics == "stored_xy" and result.units is None
    assert result.document_complete is False
    assert result.unresolved_semantics == LINE2D_UNRESOLVED_SEMANTICS
    value = result.lines[0]
    assert isinstance(value, Line2D)
    assert value.start == (1, 2) and value.source.start == (1, 2)
    assert value.coordinate_semantics == "stored_xy"
    assert value.coordinate_scope == "root_records" and value.units is None
    assert value.qualification_profile is PROFILE
    assert {"drawing_wcs", "physical_units", "visibility", "layout"} <= set(
        value.unresolved_semantics
    )
    assert value.attributes.material_index_raw == 19
    assert value.attributes.group_index == 1234
    assert value.source.header_bytes == data[8192:8208]
    with pytest.raises(FrozenInstanceError):
        value.coordinate_semantics = "wcs"
    with pytest.raises(FrozenInstanceError):
        result.document_complete = True
    with pytest.raises(UnsupportedFeatureError):
        document.qualified_lines(1, profile=PROFILE)


def test_supported_reference_and_inline_scopes_keep_complete_owned_sources():
    inner = reference(b"B", style=(0, 255, 255))
    inner[0][:2] = (0x4006).to_bytes(2, "little")
    data = fixture(
        reference(b"A", 5, (2, 2, 5)),
        [(b"A", inner), (b"B", [line(0x4402, 1, (0, 255, 255))])],
    )
    result = scan(data).qualified_lines(0, profile=PROFILE)
    value = result.lines[0]
    assert value.coordinate_scope == "package_parent"
    assert len(value.ancestors) == 2
    assert (
        value.styles.color.index,
        value.styles.line_type.index,
        value.styles.width.index,
    ) == (2, 2, 5)
    assert len(value.styles.color.sources) == 4
    for source in value.styles.color.sources:
        assert (
            source.raw_bytes
            == data[
                source.payload_offset : source.payload_offset + len(source.raw_bytes)
            ]
        )
    data = fixture(reference(b"A"), [(b"A", [inline(3), inline(2), line()])])
    assert len(scan(data).qualified_lines(0, profile=PROFILE).lines[0].ancestors) == 3


@pytest.mark.parametrize("marker", [5, 8])
def test_legacy_matching_line_fields_do_not_implicitly_gain_a_current_profile(marker):
    data = bytearray(assembler.make_semantics_contents(marker))
    data[8192 + 128 : 8192 + 128 + 64] = line()
    document = scan(data)
    assert document.line_records(2, 1)[0].start == (1, 2)
    with pytest.raises(UnsupportedFeatureError, match="marker25"):
        document.qualified_lines(2, profile=PROFILE)


@pytest.mark.parametrize(
    "root,definitions",
    [
        ([inline(4), inline(3), inline(2), line()], []),
        (
            reference(b"A"),
            [(b"A", reference(b"B")), (b"B", reference(b"C")), (b"C", [line()])],
        ),
        (
            reference(b"A"),
            [(b"A", reference(b"B")), (b"B", [inline(3), inline(2), line()])],
        ),
    ],
)
def test_native_v1_depths_stay_narrower_than_general_scene_traversal(root, definitions):
    document = scan(fixture(root, definitions))
    assert document.scene_lines(0)
    with pytest.raises(UnsupportedFeatureError, match="Line2D v1"):
        document.qualified_lines(0, profile=PROFILE)


def test_failed_admission_and_work_limits_leave_raw_access_unchanged():
    data = fixture([inline(2), line(0x2402, 0, (0, 0, 0))])
    document = scan(data)
    with pytest.raises(UnsupportedFeatureError):
        document.qualified_lines(0, profile=PROFILE)
    assert document.read_contents() == data
    document = scan(fixture([line()]))
    with pytest.raises(LimitExceededError):
        document.qualified_lines(0, profile=PROFILE, limits=SceneLimits(max_lines=0))
    with pytest.raises(TypeError):
        document.qualified_lines(0, profile=PROFILE, limits={})
    empty = scan(fixture(reference(b"A"), [(b"A", [])]))
    assert empty.scene_lines(0) == ()
    with pytest.raises(UnsupportedFeatureError, match="at least one"):
        empty.qualified_lines(0, profile=PROFILE)


@pytest.mark.parametrize("tag", [0x30, 0x430, 0x2030, 0x4030])
def test_inline_profile_preserves_its_identity_and_never_promotes_old_profile(tag):
    expanded = Line2DProfile.MARKER25_PLANAR_STORED_XY_INLINE_V1
    wrapper = inline(2)
    wrapper[:2] = tag.to_bytes(2, "little")
    wrapper[2:4] = (9).to_bytes(2, "little")
    wrapper[4:7] = bytes((7, 4, 9))
    if tag == 0x2030:
        wrapper[4:7] = bytes(3)
    if tag == 0x4030:
        wrapper[4:7] = bytes((0, 255, 255))
    document = scan(
        fixture(
            reference(b"A", 5, (2, 2, 5)),
            [(b"A", [wrapper, line(0x4402, 0, (0, 255, 255))])],
        )
    )
    with pytest.raises(UnsupportedFeatureError):
        document.qualified_lines(0, profile=PROFILE)
    result = document.qualified_lines(0, profile=expanded)
    assert (
        result.profile is expanded and result.lines[0].qualification_profile is expanded
    )
    assert (
        result.lines[0].styles.color.index,
        result.lines[0].styles.line_type.index,
        result.lines[0].styles.width.index,
    ) == (2, 2, 5)
    assert result.lines[0].ancestors[1].raw_bytes == wrapper
    assert result.units is None and result.document_complete is False
