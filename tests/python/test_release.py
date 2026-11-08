"""Release bundles must have complete, qualified and unchanged distributions."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import prepare_release as release


@pytest.fixture
def candidate(tmp_path):
    version = release.python_version(release.check_versions(release.ROOT))
    directory = tmp_path / "candidate"
    directory.mkdir()
    for platform in (
        "manylinux_2_28_x86_64",
        "manylinux_2_28_aarch64",
        "macosx_10_13_x86_64",
        "macosx_11_0_arm64",
        "win_amd64",
    ):
        (directory / f"ezdracad-{version}-cp310-abi3-{platform}.whl").write_bytes(
            b"synthetic wheel for bundle transport tests"
        )
    (directory / f"ezdracad-{version}.tar.gz").write_bytes(b"synthetic source archive")
    return directory, version


def test_release_inventory_is_complete(candidate):
    directory, version = candidate
    assert len(release.inventory(directory, version)) == 6


@pytest.mark.parametrize("pattern", ["*win_amd64.whl", "*.tar.gz"])
def test_release_rejects_missing_distribution(candidate, pattern):
    directory, version = candidate
    next(directory.glob(pattern)).unlink()
    with pytest.raises(ValueError, match="five platform wheels"):
        release.inventory(directory, version)


@pytest.mark.parametrize(
    "replacement",
    [
        "ezdracad-{version}-cp311-abi3-win_amd64.whl",
        "ezdracad-{version}-cp310-cp310-win_amd64.whl",
        "ezdracad-{version}-cp310-abi3-linux_x86_64.whl",
        "ezdracad-{version}-cp310-abi3-macosx_11_0_universal2.whl",
        "another_package-{version}-cp310-abi3-win_amd64.whl",
        "ezdracad-9.9.9-cp310-abi3-win_amd64.whl",
        "ezdracad-{version}-1-cp310-abi3-win_amd64.whl",
    ],
)
def test_release_rejects_wrong_wheel_identity_or_tags(candidate, replacement):
    directory, version = candidate
    next(directory.glob("*win_amd64.whl")).rename(
        directory / replacement.format(version=version)
    )
    with pytest.raises(ValueError, match="Unexpected wheel"):
        release.inventory(directory, version)


def test_release_rejects_duplicate_platform(candidate):
    directory, version = candidate
    (directory / f"ezdracad-{version}-cp310-abi3-macosx_12_0_arm64.whl").write_bytes(
        b"duplicate architecture with a different deployment target"
    )
    with pytest.raises(ValueError, match="Duplicate or ambiguous"):
        release.inventory(directory, version)


def test_release_rejects_unexpected_file(candidate):
    directory, version = candidate
    (directory / "notes.txt").write_text("unreviewed payload", encoding="utf-8")
    with pytest.raises(ValueError, match="Unexpected distribution"):
        release.inventory(directory, version)


@pytest.fixture
def bundle(candidate, tmp_path, monkeypatch):
    # Metadata/content inspection has separate archive tests. These synthetic
    # payloads exercise inventory and transport integrity without native builds.
    monkeypatch.setattr(release, "inspect_artifact", lambda path, version: None)
    directory, _ = candidate
    output = tmp_path / "bundle"
    release.collect(directory, output)
    return output


def test_collected_release_can_be_verified(bundle):
    release.verify_bundle(bundle)


def test_release_rejects_modified_distribution(bundle):
    next((bundle / "dist").glob("*.whl")).write_bytes(b"modified after collection")
    with pytest.raises(ValueError, match="checksum mismatch"):
        release.verify_bundle(bundle)


@pytest.mark.parametrize("change", ["duplicate", "missing", "escaping"])
def test_release_rejects_malformed_manifest(bundle, change):
    manifest = bundle / "SHA256SUMS"
    lines = manifest.read_text(encoding="ascii").splitlines(keepends=True)
    if change == "duplicate":
        lines.append(lines[0])
    elif change == "missing":
        lines.pop()
    else:
        lines[0] = "0" * 64 + "  ../outside.whl\n"
    manifest.write_text("".join(lines), encoding="ascii")
    with pytest.raises(ValueError, match="checksum"):
        release.verify_bundle(bundle)


def test_release_propagates_artifact_inspection_failure(
    candidate, tmp_path, monkeypatch
):
    def reject(path, version):
        raise ValueError("Rejected artifact metadata or private contents")

    monkeypatch.setattr(release, "inspect_artifact", reject)
    output = tmp_path / "bundle"
    with pytest.raises(ValueError, match="Rejected artifact"):
        release.collect(candidate[0], output)
    assert not output.exists()


def test_release_rechecks_transferred_metadata(bundle, monkeypatch):
    def reject(path, version):
        raise ValueError("Transferred metadata mismatch")

    monkeypatch.setattr(release, "inspect_artifact", reject)
    with pytest.raises(ValueError, match="Transferred metadata"):
        release.verify_bundle(bundle)
