"""Check package contents/metadata and install an artifact in a clean environment."""

import argparse
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from email.parser import Parser
from pathlib import Path

from bump_version import ROOT, check_versions, python_version, read_toml
from check_public_surface import check_archive


def inspect_artifact(artifact, expected_version):
    if artifact.suffix == ".whl":
        with zipfile.ZipFile(artifact) as archive:
            names = archive.namelist()
            metadata = [n for n in names if n.endswith(".dist-info/METADATA")]
            assert len(metadata) == 1
            text = archive.read(metadata[0]).decode("utf-8")
            for filename in ("LICENSE", "THIRD_PARTY_NOTICES.txt"):
                member = next(
                    n for n in names if n.endswith(".dist-info/licenses/" + filename)
                )
                assert archive.read(member) == (ROOT / filename).read_bytes()

            check_archive(
                {n: archive.read(n) for n in names if not n.endswith("/")}, kind="wheel"
            )
        assert "ezdracad/_core.pyi" in names and "ezdracad/py.typed" in names
        assert "ezdracad/line3d.py" in names
        assert "ezdracad/workplane.py" in names
        assert "ezdracad/world_input3d.py" in names
        assert "ezdracad/block3d.py" in names
        assert "ezdracad/document.py" in names
        assert "ezdracad/basic2d.py" in names
        assert "ezdracad/raw.py" in names
        assert any(
            n.startswith("ezdracad/_core") and n.endswith((".so", ".pyd"))
            for n in names
        )
    else:
        with tarfile.open(artifact, "r:gz") as archive:
            names = archive.getnames()
            metadata = sorted((n for n in names if n.endswith("/PKG-INFO")), key=len)
            assert metadata
            stream = archive.extractfile(metadata[0])
            assert stream is not None
            text = stream.read().decode("utf-8")
            members = {}
            for member in archive.getmembers():
                if member.isfile():
                    name = member.name.split("/", 1)[1]
                    members[name] = archive.extractfile(member).read()
            check_archive(members, kind="sdist")
            for filename in ("LICENSE", "THIRD_PARTY_NOTICES.txt"):
                assert members[filename] == (ROOT / filename).read_bytes()
                assert (
                    members["crates/ezdracad-core/" + filename]
                    == (ROOT / filename).read_bytes()
                )
        for suffix in [
            "/crates/ezdracad-core/src/inspect.rs",
            "/crates/ezdracad-core/src/format.rs",
            "/crates/ezdracad-core/src/header.rs",
            "/crates/ezdracad-core/src/mps.rs",
            "/crates/ezdracad-core/src/limits.rs",
            "/tests/fixtures/mps-document25.mps",
            "/tests/fixtures/mps-basic2d25.mps",
            "/tests/fixtures/mps-block25.mps",
            "/tests/fixtures/mps-README.md",
            "/fuzz/fuzz_targets/read_mps.rs",
            "/tests/fixtures/mpx25.mpx",
            "/tests/fixtures/mpx-mixed25.mpx",
            "/tests/fixtures/mpx-block25.mpx",
            "/tests/fixtures/mpx25.README.md",
            "/crates/ezdracad-core/src/basic2d.rs",
            "/crates/ezdracad-core/src/text2d.rs",
            "/crates/ezdracad-core/src/root_metadata.rs",
            "/src/ezdracad/basic2d.py",
            "/tests/fixtures/basic2d25.cfb",
            "/tests/fixtures/basic2d25.bin",
            "/fuzz/fuzz_targets/read_basic2d.rs",
            "/docs/README.md",
            "/docs/usage.md",
            "/docs/compatibility.md",
            "/Cargo.lock",
            "/crates/ezdracad-core/src/lib.rs",
            "/crates/ezdracad-core/src/document.rs",
            "/crates/ezdracad-python/src/document_reading.rs",
            "/src/ezdracad/document.py",
            "/tests/fixtures/document25.cfb",
            "/tests/fixtures/document-partial25.cfb",
            "/fuzz/fuzz_targets/read_document.rs",
            "/crates/ezdracad-core/LICENSE",
            "/crates/ezdracad-core/README.md",
            "/crates/ezdracad-python/src/lib.rs",
            "/src/ezdracad/py.typed",
            "/tests/fixtures/records5.cfb",
            "/tests/fixtures/records5.bin",
            "/tests/fixtures/records25.cfb",
            "/tests/fixtures/blocks5.cfb",
            "/tests/fixtures/blocks8.cfb",
            "/tests/fixtures/blocks25.cfb",
            "/crates/ezdracad-core/src/blocks.rs",
            "/crates/ezdracad-core/src/block3d.rs",
            "/crates/ezdracad-python/src/block3d_scanning.rs",
            "/src/ezdracad/block3d.py",
            "/tests/fixtures/block3d25.bin",
            "/tests/fixtures/block3d25.cfb",
            "/tests/fixtures/block3d25.README.md",
            "/crates/ezdracad-core/src/package_refs.rs",
            "/crates/ezdracad-python/src/block_scanning.rs",
            "/tests/fixtures/objects5.cfb",
            "/tests/fixtures/objects8.cfb",
            "/tests/fixtures/objects25.cfb",
            "/crates/ezdracad-core/src/objects.rs",
            "/crates/ezdracad-core/src/drawing_settings.rs",
            "/tests/fixtures/semantics25.cfb",
            "/tests/fixtures/measurement25.cfb",
            "/tests/fixtures/inheritance25.cfb",
            "/crates/ezdracad-core/src/coordinate_measurement.rs",
            "/crates/ezdracad-core/src/effective_styles.rs",
            "/crates/ezdracad-core/src/scene.rs",
            "/crates/ezdracad-core/src/print_width.rs",
            "/crates/ezdracad-core/src/print_settings.rs",
            "/crates/ezdracad-core/src/origin_units.rs",
            "/crates/ezdracad-core/src/qualification.rs",
            "/crates/ezdracad-core/src/unit_resolution.rs",
            "/crates/ezdracad-core/src/coordinate_space.rs",
            "/crates/ezdracad-core/src/coordinate_input_units.rs",
            "/crates/ezdracad-core/src/dimension_scale.rs",
            "/crates/ezdracad-core/src/line3d.rs",
            "/crates/ezdracad-core/src/workplane.rs",
            "/crates/ezdracad-core/src/world_input3d.rs",
            "/crates/ezdracad-python/src/scene_scanning.rs",
            "/crates/ezdracad-python/src/print_width_scanning.rs",
            "/crates/ezdracad-python/src/print_settings_scanning.rs",
            "/crates/ezdracad-python/src/origin_units_scanning.rs",
            "/crates/ezdracad-python/src/qualification_scanning.rs",
            "/crates/ezdracad-python/src/unit_resolution_scanning.rs",
            "/crates/ezdracad-python/src/coordinate_space_scanning.rs",
            "/crates/ezdracad-python/src/coordinate_input_units_scanning.rs",
            "/crates/ezdracad-python/src/dimension_scale_scanning.rs",
            "/crates/ezdracad-python/src/line3d_scanning.rs",
            "/crates/ezdracad-python/src/workplane_scanning.rs",
            "/crates/ezdracad-python/src/world_input3d_scanning.rs",
            "/src/ezdracad/scene.py",
            "/src/ezdracad/print_width.py",
            "/src/ezdracad/print_settings.py",
            "/src/ezdracad/origin_units.py",
            "/src/ezdracad/qualification.py",
            "/src/ezdracad/unit_resolution.py",
            "/src/ezdracad/coordinate_space.py",
            "/src/ezdracad/coordinate_input_units.py",
            "/src/ezdracad/dimension_scale.py",
            "/tests/fixtures/dimension25.bin",
            "/tests/fixtures/dimension25.cfb",
            "/src/ezdracad/line3d.py",
            "/src/ezdracad/workplane.py",
            "/src/ezdracad/world_input3d.py",
            "/crates/ezdracad-python/src/coordinate_measurement_scanning.rs",
            "/crates/ezdracad-python/src/effective_style_scanning.rs",
            "/src/ezdracad/coordinate_measurement.py",
            "/src/ezdracad/effective_styles.py",
            "/crates/ezdracad-core/src/inline_packages.rs",
            "/crates/ezdracad-core/src/record_attributes.rs",
            "/crates/ezdracad-core/src/attributes.rs",
            "/crates/ezdracad-core/src/text_units.rs",
            "/crates/ezdracad-python/src/style_scanning.rs",
            "/crates/ezdracad-python/src/record_attribute_scanning.rs",
            "/src/ezdracad/styles.py",
            "/fuzz/Cargo.lock",
            "/fuzz/fuzz_targets/scan_contents.rs",
        ]:
            assert any(n.endswith(suffix) for n in names), suffix
    for name in names:
        for forbidden in [
            ".internal/",
            "samples/",
            "docs/research/",
            "docs/validation/",
            ".aws/",
            ".codex/",
            ".agents/",
            "target/",
        ]:
            assert not name.startswith(forbidden) and f"/{forbidden}" not in name, name
    metadata = Parser().parsestr(text)
    assert (
        metadata.get_payload().rstrip().splitlines()
        == (ROOT / "README.md").read_text(encoding="utf-8").rstrip().splitlines()
    )
    assert metadata["Name"] == "ezdracad"
    assert (
        read_toml(ROOT / "Cargo.toml")["workspace"]["package"]["license"]
        == read_toml(ROOT / "pyproject.toml")["project"]["license"]
    )
    assert metadata["Version"] == expected_version
    assert (
        metadata["License-Expression"]
        == read_toml(ROOT / "pyproject.toml")["project"]["license"]
    )
    assert set(metadata.get_all("License-File")) == {
        "LICENSE",
        "THIRD_PARTY_NOTICES.txt",
    }
    assert (
        metadata["Requires-Python"]
        == read_toml(ROOT / "pyproject.toml")["project"]["requires-python"]
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    args = parser.parse_args()
    artifact = args.artifact.resolve()
    rust_version = check_versions(ROOT)
    expected_version = python_version(rust_version)
    inspect_artifact(artifact, expected_version)
    with tempfile.TemporaryDirectory(prefix="ezdracad-install-") as temp:
        directory = Path(temp)
        environment = directory / "venv"
        subprocess.run(
            ["uv", "venv", "--python", sys.executable, str(environment)], check=True
        )
        python = environment / (
            "Scripts/python.exe" if sys.platform == "win32" else "bin/python"
        )
        command = ["uv", "pip", "install", "--python", str(python), "--no-deps"]
        if artifact.suffix == ".whl":
            command.append("--no-index")
        subprocess.run([*command, str(artifact)], cwd=directory, check=True)
        subprocess.run(
            [
                str(python),
                str(ROOT / "scripts/verify_install.py"),
                "--fixture",
                str(ROOT / "tests/fixtures/minimal.cfb"),
                "--python-version",
                expected_version,
                "--rust-version",
                rust_version,
            ],
            cwd=directory,
            check=True,
        )
    print(f"Artifact verified: {artifact.name}")


if __name__ == "__main__":
    main()
