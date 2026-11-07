"""Synthetic cross-language guards; native visual evidence is tested separately."""

import importlib.util
import struct
from pathlib import Path

import pytest

from ezdracad import UnsupportedFeatureError
from ezdracad.scene import SceneLimits

_spec = importlib.util.spec_from_file_location(
    "inline_scene_fixture", Path(__file__).with_name("test_scene.py")
)
fixture = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fixture)


def document(wrapper_tag, leaf_tag):
    outer = fixture.inline(4)
    inner = fixture.inline(2)
    struct.pack_into("<H", outer, 0, wrapper_tag)
    if wrapper_tag in (0x2030, 0x4030):
        outer[4:7] = bytes((0, 0, 0) if wrapper_tag == 0x2030 else (0, 255, 255))
    inner[2:7] = bytes((7, 0, 8, 9, 10))
    child = fixture.line(
        leaf_tag, 0, (0, 255, 255) if leaf_tag == 0x4402 else (0, 0, 0)
    )
    return fixture.scan(
        fixture.fixture(
            fixture.reference(
                b"A", layer=5, style=(12, 13, 14), placement=(100, 50, 0, 2, 2, 90)
            ),
            ((b"A", [outer, inner, child, child]),),
        )
    )


@pytest.mark.parametrize("wrapper_tag", [0x30, 0x430, 0x2030, 0x4030])
@pytest.mark.parametrize(
    "leaf_tag,expected", [(0x4402, (12, 13, 14)), (0x2402, (2, 3, 4))]
)
def test_explicit_inline_opt_in_preserves_true_scope_and_geometry(
    wrapper_tag, leaf_tag, expected
):
    drawing = document(wrapper_tag, leaf_tag)
    with pytest.raises(UnsupportedFeatureError):
        drawing.scene_lines(0)
    rows = drawing.scene_lines_with_inline_scopes(0)
    assert len(rows) == 2
    for index, row in enumerate(rows):
        assert fixture.triple(row) == expected
        assert row.start == pytest.approx((96, 52))
        assert row.end == pytest.approx((92, 56))
        assert row.coordinate_scope == "package_parent" and row.units is None
        assert row.style_layer_index == 5
        assert [a.kind for a in row.ancestors] == ["block_reference"] + [
            "inline_package"
        ] * (2 if index == 0 else 1)
        assert int.from_bytes(row.ancestors[1].raw_bytes[:2], "little") == wrapper_tag
        assert all(
            source.kind != "inline_package"
            for field in (row.styles.color, row.styles.line_type, row.styles.width)
            for source in field.sources
        )


def test_inline_wrapper_api_arguments_keep_existing_limit_checks():
    drawing = document(0x2030, 0x4402)
    with pytest.raises(TypeError, match="limits"):
        drawing.scene_lines_with_inline_scopes(0, limits={})
    with pytest.raises(ValueError):
        drawing.scene_lines_with_inline_scopes(-1)
    from ezdracad import LimitExceededError

    with pytest.raises(LimitExceededError):
        drawing.scene_lines_with_inline_scopes(0, limits=SceneLimits(max_lines=1))
