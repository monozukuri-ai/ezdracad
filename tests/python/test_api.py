import subprocess
import sys
from dataclasses import FrozenInstanceError
from importlib.metadata import version
from pathlib import Path

import pytest
from packaging.version import Version

import ezdracad
from ezdracad import _core

FIXTURE = Path(__file__).resolve().parents[1] / "fixtures/minimal.cfb"


def test_installed_version_and_cli():
    assert ezdracad.__version__ == version("ezdracad")
    assert Version(ezdracad.__version__) == Version(_core.core_version())
    result = subprocess.run(
        [sys.executable, "-m", "ezdracad", "--version"],
        capture_output=True,
        text=True,
        check=True,
    )
    assert result.stdout.strip() == f"ezdracad {ezdracad.__version__}"


def test_independent_fixture_through_rust_binding():
    info = ezdracad.raw.inspect_cfb(FIXTURE.read_bytes())
    assert info.version == 3
    assert info.root_clsid == "00000000-0000-0000-0000-000000000000"
    assert info.total_stream_size == 11
    assert [(entry.path, entry.kind, entry.size) for entry in info.entries] == [
        ("/Example", "storage", 0),
        ("/Example/Note", "stream", 11),
    ]
    with pytest.raises(FrozenInstanceError):
        info.version = 4


def test_rust_exception_hierarchy_and_limits():
    with pytest.raises(ezdracad.InvalidFormatError):
        ezdracad.raw.inspect_cfb(b"invalid")
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        ezdracad.raw.inspect_cfb(b"x", limits=ezdracad.ParseLimits(max_file_size=0))
    assert isinstance(caught.value, ezdracad.DracadError)
    assert (caught.value.resource, caught.value.limit, caught.value.actual) == (
        "file_size",
        0,
        1,
    )
    for error in [
        ezdracad.InvalidFormatError,
        ezdracad.UnsupportedFormatError,
        ezdracad.UnsupportedFeatureError,
        ezdracad.LimitExceededError,
        ezdracad.DracadIoError,
    ]:
        assert issubclass(error, ezdracad.DracadError)


@pytest.mark.parametrize(
    "name", ["max_cfb_entries", "max_stream_size", "max_total_stream_size"]
)
def test_inventory_limits_cross_the_ffi_boundary(name):
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        ezdracad.raw.inspect_cfb(
            FIXTURE.read_bytes(), limits=ezdracad.ParseLimits(**{name: 0})
        )
    assert caught.value.resource == name.removeprefix("max_")


@pytest.mark.parametrize(
    "value,error",
    [(-1, ValueError), (1 << 64, ValueError), (True, TypeError), (1.5, TypeError)],
)
def test_invalid_limit_values(value, error):
    with pytest.raises(error):
        ezdracad.ParseLimits(max_file_size=value)


def test_paths_and_mutable_buffers_are_not_implicitly_bytes():
    for value in [FIXTURE, str(FIXTURE), bytearray(FIXTURE.read_bytes())]:
        with pytest.raises(TypeError, match="data must be bytes"):
            ezdracad.raw.inspect_cfb(value)
