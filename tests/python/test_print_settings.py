"""Native saved-field decoding and explicit policy holds, using synthetic mutations."""

import importlib.util
import struct
from dataclasses import FrozenInstanceError
from pathlib import Path

import pytest

from ezdracad import InvalidFormatError, UnsupportedFeatureError, raw
from ezdracad.effective_styles import EffectiveStyleIndices, ResolvedStyleIndex
from ezdracad.print_settings import PrintSettings
from ezdracad.print_width import resolve_nominal_print_width

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "print_settings_fixture", ROOT / "scripts/make_test_fixture.py"
)
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)

FIELDS = (
    ("drawing_paper_scale_factor", 0x178, 8),
    ("printer_settings", 0x1D0, 4),
    ("output_scale_mode", 0x1FC, 4),
    ("width_by_color", 0x200, 4),
    ("print_scale_factor", 0x208, 8),
    ("reduction_mode", 0x2D8, 4),
    ("suppress_zero_width", 0x788, 4),
)


def contents(marker=25, words=(), factors=()):
    data = bytearray(assembler.make_semantics_contents(marker))
    for _, offset, size in FIELDS:
        data[offset : offset + size] = bytes(size)
    struct.pack_into("<d", data, 0x178, 0.01)
    struct.pack_into("<d", data, 0x208, 0.005)
    for offset, value in words:
        struct.pack_into("<I", data, offset, value)
    for offset, value in factors:
        struct.pack_into("<d", data, offset, value)
    for offset in (0x170, 0x1D4, 0x204, 0x2DC, 0x78C):
        data[offset : offset + 4] = b"\xa5\x5a\xc7\xe9"
    return data


def scan(data):
    return raw.scan_records(assembler.wrap_record_contents(data))


def test_raw_fields_are_owned_and_exact_without_interpreting_neighbor_bytes(tmp_path):
    data = contents(words=((0x200, 1), (0x788, 1)))
    path = tmp_path / "policy.mpz"
    path.write_bytes(assembler.wrap_record_contents(data))
    result = raw.scanfile(path).print_settings()
    assert isinstance(result, PrintSettings)
    path.unlink()
    assert result.marker_raw == 25
    assert result.drawing_paper_scale_factor == 0.01
    assert result.print_scale_factor == 0.005
    assert result.width_by_color_raw == result.suppress_zero_width_raw == 1
    assert (
        tuple((f.name, f.payload_offset, len(f.raw_bytes)) for f in result.fields)
        == FIELDS
    )
    for f in result.fields:
        assert (
            f.raw_bytes == data[f.payload_offset : f.payload_offset + len(f.raw_bytes)]
        )
    with pytest.raises(FrozenInstanceError):
        result.fields[0].raw_bytes = b""


@pytest.mark.parametrize(
    "mode,expected,denominator",
    [(0, "drawing", None), (1, "automatic", None), (2, "specified", 200)],
)
def test_saved_modes_convert_only_to_their_typed_policy(mode, expected, denominator):
    settings = scan(contents(words=((0x1FC, mode), (0x2D8, 1)))).print_settings()
    policy = settings.width_policy()
    assert policy.drawing_denominator == 100
    assert policy.output_scale == expected
    assert policy.output_denominator == denominator
    assert policy.reduction_mode
    assert policy.device_context == "unspecified"
    if mode == 1:
        assert (
            settings.width_policy(automatic_output_denominator=75).output_denominator
            == 75
        )
    else:
        with pytest.raises(InvalidFormatError):
            settings.width_policy(automatic_output_denominator=75)


@pytest.mark.parametrize("offset", [0x1D0, 0x200, 0x2D8, 0x788])
@pytest.mark.parametrize("value", [2, 256, 0x80000001])
def test_unknown_switch_bits_are_preserved_and_cannot_be_ignored(offset, value):
    settings = scan(contents(words=((offset, value),))).print_settings()
    field = next(f for f in settings.fields if f.payload_offset == offset)
    assert field.raw_bytes == struct.pack("<I", value)
    with pytest.raises(UnsupportedFeatureError):
        settings.width_policy(device_context="application_managed_opaque_vectors")


@pytest.mark.parametrize("value", [3, 256, 0xFFFFFFFF])
def test_unknown_scale_modes_remain_inspectable(value):
    settings = scan(contents(words=((0x1FC, value),))).print_settings()
    assert settings.output_scale_mode_raw == value
    with pytest.raises(UnsupportedFeatureError):
        settings.width_policy()


def test_saved_driver_override_forces_a_hold_despite_caller_vector_assertion():
    drawing = scan(contents(words=((0x1D0, 1),)))
    p = drawing.print_settings().width_policy(
        device_context="application_managed_opaque_vectors"
    )
    assert p.device_context == "driver_managed"

    def index(n):
        return ResolvedStyleIndex(n, ())

    styles = EffectiveStyleIndices(index(2), index(1), index(5))
    result = resolve_nominal_print_width(styles, drawing.style_tables(), p)
    assert result.status == "held_driver_policy"
    assert result.nominal_paper_mm is None


@pytest.mark.parametrize("bad", [0, -1, float("nan"), float("inf"), 5e-324])
def test_only_active_scale_factors_require_finite_positive_invertible_values(bad):
    settings = scan(contents(factors=((0x178, bad),))).print_settings()
    with pytest.raises(InvalidFormatError):
        settings.width_policy()
    inactive = scan(contents(factors=((0x208, bad),))).print_settings()
    assert inactive.width_policy().output_scale == "drawing"
    active = scan(
        contents(words=((0x1FC, 2),), factors=((0x208, bad),))
    ).print_settings()
    with pytest.raises(InvalidFormatError):
        active.width_policy()


@pytest.mark.parametrize("marker", [5, 8])
def test_legacy_policy_offsets_require_separate_native_control_holdouts(marker):
    drawing = scan(contents(marker))
    with pytest.raises(UnsupportedFeatureError):
        drawing.print_settings()


def test_context_unknown_values_and_bool_numerics_are_rejected_by_wrapper_and_ffi():
    settings = scan(contents(words=((0x1FC, 1),))).print_settings()
    with pytest.raises(ValueError):
        settings.width_policy(device_context="assume_defaults")
    for bad in (True, False, "100"):
        with pytest.raises(TypeError):
            settings.width_policy(automatic_output_denominator=bad)
    for bad in (True, False):
        with pytest.raises(TypeError):
            settings._handle.width_policy("unspecified", bad)
    for bad in (0, -1, float("nan"), float("inf")):
        with pytest.raises(InvalidFormatError):
            settings.width_policy(automatic_output_denominator=bad)
