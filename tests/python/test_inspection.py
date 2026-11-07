import hashlib
import json
import subprocess
import sys
from pathlib import Path

import pytest

import ezdracad

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
ENVELOPE = FIXTURES / "mpz-envelope.cfb"
CONTENTS = (FIXTURES / "mpz-contents.bin").read_bytes()


def test_bytes_file_and_owned_contents_snapshot(tmp_path):
    path = tmp_path / "図面.without_mpz_extension"
    path.write_bytes(ENVELOPE.read_bytes())
    info = ezdracad.raw.inspectfile(path)
    assert info == ezdracad.raw.inspect(ENVELOPE.read_bytes())
    assert info.format == "mpz"
    assert info.file_size == ENVELOPE.stat().st_size
    assert info.payload.marker_raw == 0x3456
    assert info.payload.sha256 == hashlib.sha256(CONTENTS).hexdigest()
    assert info.payload.size == len(CONTENTS)
    assert info.read_contents() == CONTENTS
    path.write_bytes(b"source changed after inspection")
    assert info.read_contents() == CONTENTS
    metadata = info.to_dict()
    assert metadata["schema_version"] == 1
    assert metadata["geometry_status"] == "not_decoded"
    assert metadata["units"] is None and metadata["coordinate_system"] is None
    assert metadata["payload"]["offset_domain"] == "cfb_stream"
    assert "Synthetic test payload" not in json.dumps(metadata)
    assert json.loads(json.dumps(metadata)) == metadata


def test_generic_cfb_is_not_dra_even_with_mpz_extension(tmp_path):
    path = tmp_path / "unrelated.mpz"
    path.write_bytes((FIXTURES / "minimal.cfb").read_bytes())
    info = ezdracad.raw.inspectfile(path)
    assert info.format == "unknown_cfb" and info.payload is None
    with pytest.raises(ezdracad.UnsupportedFormatError):
        info.read_contents()


def test_io_and_file_byte_limit(tmp_path):
    with pytest.raises(ezdracad.DracadIoError):
        ezdracad.raw.inspectfile(tmp_path / "missing.mpz")
    with pytest.raises(ezdracad.DracadIoError):
        ezdracad.raw.inspectfile(tmp_path)
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        ezdracad.raw.inspectfile(
            ENVELOPE,
            limits=ezdracad.ParseLimits(max_file_size=ENVELOPE.stat().st_size - 1),
        )
    assert caught.value.resource == "file_size"
    assert caught.value.actual == ENVELOPE.stat().st_size
    with pytest.raises(TypeError):
        ezdracad.raw.inspectfile(bytes(ENVELOPE))
    with pytest.raises(ezdracad.UnsupportedFormatError):
        ezdracad.raw.inspect(CONTENTS)


@pytest.mark.parametrize(
    "name", ["max_cfb_metadata_size", "max_cfb_directory_slots", "max_cfb_depth"]
)
def test_preflight_limits_cross_ffi(name):
    with pytest.raises(ezdracad.LimitExceededError) as caught:
        ezdracad.raw.inspect(
            ENVELOPE.read_bytes(), limits=ezdracad.ParseLimits(**{name: 0})
        )
    assert caught.value.resource == name.removeprefix("max_")


def run_cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "ezdracad", "inspect", *map(str, args), "--json"],
        capture_output=True,
        text=True,
    )


def test_cli_reports_metadata_and_unsupported_cfb():
    result = run_cli(ENVELOPE)
    assert result.returncode == 0 and result.stderr == ""
    metadata = json.loads(result.stdout)
    assert metadata["format"] == "mpz"
    assert metadata["payload"]["sha256"] == hashlib.sha256(CONTENTS).hexdigest()
    assert metadata["geometry_status"] == "not_decoded"
    result = run_cli(FIXTURES / "minimal.cfb")
    assert result.returncode == 2
    assert json.loads(result.stdout)["format"] == "unknown_cfb"


def test_cli_error_codes_and_structured_errors(tmp_path):
    malformed = tmp_path / "bad.mpz"
    malformed.write_bytes(b"not a compound file")
    result = run_cli(malformed)
    assert result.returncode == 1 and result.stdout == ""
    assert json.loads(result.stderr)["error"] == "InvalidFormatError"
    result = run_cli(tmp_path / "missing")
    assert result.returncode == 1
    assert json.loads(result.stderr)["error"] == "DracadIoError"
    result = run_cli(ENVELOPE, "--max-file-size", 1)
    assert result.returncode == 3
    assert json.loads(result.stderr)["resource"] == "file_size"
    result = run_cli(FIXTURES / "mpz-contents.bin")
    assert result.returncode == 2
    assert json.loads(result.stderr)["error"] == "UnsupportedFormatError"
