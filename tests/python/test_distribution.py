"""Distribution metadata must not depend on host checkout line endings."""

import importlib.util
import sys
import zipfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location(
    "distribution_check", ROOT / "scripts/verify_distribution.py"
)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


@pytest.mark.parametrize("newline", ["\n", "\r\n"])
def test_wheel_metadata_accepts_equivalent_readme_line_endings(tmp_path, newline):
    project = checker.read_toml(ROOT / "pyproject.toml")["project"]
    version = checker.python_version(checker.check_versions(ROOT))
    metadata = (
        "Metadata-Version: 2.4\n"
        "Name: ezdracad\n"
        f"Version: {version}\n"
        f"Requires-Python: {project['requires-python']}\n"
        f"License-Expression: {project['license']}\n"
        "License-File: LICENSE\n"
        "License-File: THIRD_PARTY_NOTICES.txt\n\n"
        + (ROOT / "README.md").read_text(encoding="utf-8")
    ).replace("\n", newline)
    artifact = tmp_path / "metadata.whl"
    dist_info = f"ezdracad-{version}.dist-info"
    with zipfile.ZipFile(artifact, "w") as archive:
        for source in (ROOT / "src/ezdracad").iterdir():
            if source.suffix in {".py", ".pyi"} or source.name == "py.typed":
                archive.write(source, "ezdracad/" + source.name)
        archive.writestr("ezdracad/_core.abi3.so", b"synthetic metadata-only vector")
        archive.writestr(dist_info + "/METADATA", metadata.encode("utf-8"))
        for filename in ("LICENSE", "THIRD_PARTY_NOTICES.txt"):
            archive.write(ROOT / filename, dist_info + "/licenses/" + filename)
    checker.inspect_artifact(artifact, version)
