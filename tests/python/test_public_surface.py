"""Publication guards reject private material in source and actual archives."""

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "public_surface", ROOT / "scripts/check_public_surface.py"
)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


@pytest.mark.parametrize(
    "name",
    [
        ".internal/notes.md",
        "docs/research/proof.json",
        "docs/validation/run.md",
        "samples/drawing.mpz",
        "tests/research/test_native.py",
        "../secret",
        "/secret",
        "pkg/.internal/source.py",
        "vendor/setup.exe",
    ],
)
def test_private_or_escaping_archive_paths_are_rejected(name):
    with pytest.raises(ValueError):
        checker.check_archive({name: b""}, kind="sdist")


@pytest.mark.parametrize("kind", ["wheel", "sdist", "crate"])
def test_embedded_build_paths_are_rejected_in_binary_members(kind):
    name = "ezdracad/_core.so" if kind == "wheel" else "src/source.bin"
    for value in (str(ROOT).encode(), str(ROOT).encode("utf-16-le")):
        with pytest.raises(ValueError, match="path in archive"):
            checker.check_archive({name: b"binary\0" + value + b"\0"}, kind=kind)


def test_wheel_does_not_accept_unrelated_top_level_data():
    with pytest.raises(ValueError, match="unexpected wheel"):
        checker.check_archive({"private-data.json": b"{}"}, kind="wheel")


def test_document_language_links_and_private_references(tmp_path):
    document = tmp_path / "README.md"
    for value in (
        "# Guide\n\n内部の説明\n",
        "# Guide\n\n[missing](missing.md)\n",
        "# Guide\n\n[private](.internal/notes.md)\n",
    ):
        document.write_text(value)
        with pytest.raises(ValueError):
            checker.check_document(document, root=tmp_path)
    document.write_text(
        "# Guide\n\n[section](#section)\n\n## Section\n\nEnglish prose.\n"
    )
    assert checker.check_document(document, root=tmp_path) == 1
