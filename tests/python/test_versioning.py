import importlib.util
import shutil
import subprocess
from pathlib import Path

import pytest
from packaging.version import Version

import ezdracad
from ezdracad import _core

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "bump_version", ROOT / "scripts/bump_version.py"
)
bump = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bump)


@pytest.mark.parametrize(
    "version", ["0.1.0", "1.2.3-alpha.1", "1.2.3-beta.2", "1.2.3-rc.3"]
)
def test_cargo_and_python_version_relationship(version):
    assert Version(bump.python_version(version)) == Version(version)


@pytest.mark.parametrize(
    "version", ["v1.2.3", "01.2.3", "1.2", "1.2.3-alpha", "1.2.3-rc.01"]
)
def test_invalid_release_versions(version):
    with pytest.raises(ValueError):
        bump.python_version(version)


def test_manifests_and_lockfiles_share_one_version():
    current = bump.check_versions(ROOT)
    assert _core.core_version() == current, (
        "rebuild the extension after changing Cargo versions"
    )
    assert Version(ezdracad.__version__) == Version(current)
    assert (ROOT / "LICENSE").read_bytes() == (
        ROOT / "crates/ezdracad-core/LICENSE"
    ).read_bytes()
    assert bump.check_versions(ROOT, f"v{current}") == current
    assert bump.check_versions(ROOT, f"v{bump.python_version(current)}") == current
    with pytest.raises(ValueError, match="tag"):
        bump.check_versions(ROOT, "unrelated-tag")


@pytest.fixture
def release_root(tmp_path):
    current = bump.check_versions(ROOT)

    def create(version):
        for name in (
            "Cargo.toml",
            "Cargo.lock",
            "uv.lock",
            "fuzz/Cargo.lock",
            "pyproject.toml",
            "crates/ezdracad-core/Cargo.toml",
            "crates/ezdracad-python/Cargo.toml",
        ):
            path = tmp_path / name
            path.parent.mkdir(parents=True, exist_ok=True)
            text = (ROOT / name).read_text(encoding="utf-8")
            text = text.replace(f'"{current}"', f'"{version}"')
            text = text.replace(
                f'"{bump.python_version(current)}"', f'"{bump.python_version(version)}"'
            )
            path.write_text(text, encoding="utf-8")
        return tmp_path

    return create


@pytest.mark.parametrize(
    ("version", "tag"),
    [
        ("1.2.3", "v1.2.3"),
        ("1.2.3-alpha.1", "v1.2.3-alpha.1"),
        ("1.2.3-alpha.1", "v1.2.3a1"),
        ("1.2.3-beta.2", "v1.2.3-beta.2"),
        ("1.2.3-beta.2", "v1.2.3b2"),
        ("1.2.3-rc.3", "v1.2.3-rc.3"),
        ("1.2.3-rc.3", "v1.2.3rc3"),
    ],
)
def test_release_tags_accept_cargo_and_python_spellings(release_root, version, tag):
    assert bump.check_versions(release_root(version), tag) == version


@pytest.mark.parametrize(
    "tag",
    [
        "v1.2.3",
        "v1.2.3a2",
        "v1.2.3b1",
        "v1.2.3-alpha.2",
        "v1.2.4a1",
        "v1.2.3a01",
        "v1.2.3A1",
        "1.2.3a1",
    ],
)
def test_release_tags_reject_other_versions_and_aliases(release_root, tag):
    with pytest.raises(ValueError, match="tag"):
        bump.check_versions(release_root("1.2.3-alpha.1"), tag)


def test_failed_update_restores_manifests_and_lockfiles(tmp_path, monkeypatch):
    names = ["Cargo.toml", "Cargo.lock", "uv.lock", "fuzz/Cargo.lock"]
    for name in names:
        (tmp_path / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, tmp_path / name)
    originals = {name: (tmp_path / name).read_bytes() for name in names}

    def fail(command, **kwargs):
        (tmp_path / "Cargo.lock").write_text("partial update")
        (tmp_path / "fuzz/Cargo.lock").write_text("partial fuzz update")
        raise subprocess.CalledProcessError(1, command)

    monkeypatch.setattr(bump.subprocess, "run", fail)
    with pytest.raises(subprocess.CalledProcessError):
        bump.bump_version(tmp_path, "9.8.7-rc.1")
    assert {name: (tmp_path / name).read_bytes() for name in names} == originals


def test_independent_fuzz_lock_version_cannot_go_stale(tmp_path):
    for name in (
        "Cargo.toml",
        "Cargo.lock",
        "uv.lock",
        "fuzz/Cargo.lock",
        "crates/ezdracad-core/Cargo.toml",
        "crates/ezdracad-python/Cargo.toml",
    ):
        (tmp_path / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, tmp_path / name)
    shutil.copyfile(ROOT / "pyproject.toml", tmp_path / "pyproject.toml")
    path = tmp_path / "fuzz/Cargo.lock"
    current = bump.check_versions(tmp_path)
    path.write_text(
        path.read_text().replace(f'version = "{current}"', 'version = "9.8.7"')
    )
    with pytest.raises(ValueError, match="fuzz/Cargo.lock is stale"):
        bump.check_versions(tmp_path)
