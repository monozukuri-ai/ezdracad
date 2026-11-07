"""Check English public documentation and repository/distribution boundaries."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
PRIVATE_ROOTS = (
    ".internal",
    "samples",
    "docs/research",
    "docs/validation",
    "docs/implementation-plan.md",
    "tests/research",
)
TEXT_SUFFIXES = {
    ".md",
    ".rs",
    ".py",
    ".pyi",
    ".toml",
    ".json",
    ".yml",
    ".yaml",
    ".lock",
}
GUARD_FILES = {
    "scripts/check_public_surface.py",
    "scripts/verify_distribution.py",
    "tests/python/test_public_surface.py",
}
HISTORY = re.compile(r"\bP[0-9]\b|native-controls|\b(?:Wine|Trial)\b|2026-\d\d-\d\d")
JAPANESE = re.compile(r"[\u3040-\u30ff\u3400-\u9fff]")
PRIVATE_TEXT = (
    "docs/research/",
    "docs/validation/",
    "chatgpt.com/share/",
    ".internal/native-",
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def public_name(name):
    require("\\" not in name and not name.startswith("/"), f"nonportable path: {name}")
    parts = PurePosixPath(name).parts
    require(".." not in parts, f"escaping path: {name}")
    require(
        not any(name == p or name.startswith(p + "/") for p in PRIVATE_ROOTS),
        f"private path: {name}",
    )
    require(".internal" not in parts, f"private path: {name}")
    require(
        not any(p in {"__pycache__", ".venv", "target", ".git"} for p in parts),
        f"generated path: {name}",
    )
    require(not name.endswith((".exe", ".msi", ".chm")), f"vendor artifact: {name}")


def check_text(name, text, *, root=ROOT):
    if name not in GUARD_FILES:
        for marker in PRIVATE_TEXT:
            require(marker not in text, f"private reference in {name}: {marker}")
    for marker in (str(root.resolve()), str(Path.home())):
        require(marker not in text, f"local filesystem path in {name}")
    if name.endswith(".md"):
        require(".internal" not in text, f"private documentation reference: {name}")
        require(not HISTORY.search(text), f"development history in {name}")
        prose = re.sub(r"```[^\n]*\n.*?```", "", text, flags=re.S)
        require(not JAPANESE.search(prose), f"non-English default prose: {name}")


def check_document(path, *, root=ROOT):
    text = path.read_text(encoding="utf-8")
    check_text(path.relative_to(root).as_posix(), text, root=root)
    count = 0
    for target in re.findall(r"!?\[[^\]]*\]\(([^)]+)\)", text):
        target = target.strip().strip("<>")
        url = urlsplit(target)
        if url.scheme or url.netloc:
            require(
                url.scheme in {"https", "http", "mailto"}, f"unsupported link: {target}"
            )
            continue
        linked = (
            (path.parent / unquote(url.path)).resolve() if url.path else path.resolve()
        )
        require(
            linked.is_relative_to(root.resolve()),
            f"link leaves repository: {path}: {target}",
        )
        require(linked.exists(), f"broken link: {path}: {target}")
        public_name(linked.relative_to(root.resolve()).as_posix())
        if url.fragment and linked.is_file() and linked.suffix == ".md":
            headings = re.findall(
                r"^#{1,6}\s+(.+)$", linked.read_text(encoding="utf-8"), re.M
            )
            anchors = {
                re.sub(r"[^\w -]", "", h.lower()).replace(" ", "-") for h in headings
            }
            require(
                unquote(url.fragment) in anchors, f"broken anchor: {path}: {target}"
            )
        count += 1
    return count


def check_archive(members, *, kind, root=ROOT):
    """Inspect actual archive names and contents, including compiled paths."""
    require(kind in {"wheel", "sdist", "crate"}, "unknown archive kind")
    for name, data in members.items():
        public_name(name)
        if kind == "wheel":
            require(
                name.startswith("ezdracad/") or ".dist-info/" in name,
                f"unexpected wheel member: {name}",
            )
        for marker in (str(root.resolve()).encode(), str(Path.home()).encode()):
            require(marker not in data, f"local build path in archive member: {name}")
            require(
                marker.decode().encode("utf-16-le") not in data,
                f"local UTF-16 path in archive member: {name}",
            )
        if PurePosixPath(name).suffix in TEXT_SUFFIXES or PurePosixPath(name).name in {
            "METADATA",
            "PKG-INFO",
            "LICENSE",
        }:
            check_text(name, data.decode("utf-8"), root=root)
    return len(members)


def check_repository(root=ROOT):
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
        capture_output=True,
        check=True,
    )
    names = sorted(set(filter(None, result.stdout.decode().split("\0"))))
    docs, links = 0, 0
    for name in names:
        public_name(name)
        path = root / name
        require(path.exists(), f"missing public file: {name}")
        require(not path.is_symlink(), f"public symlink: {name}")
        if path.suffix in TEXT_SUFFIXES:
            check_text(name, path.read_text(encoding="utf-8"), root=root)
        if path.suffix == ".md":
            docs += 1
            links += check_document(path, root=root)
    return {
        "status": "passed",
        "public_files": len(names),
        "English_documents": docs,
        "local_links": links,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    print(json.dumps(check_repository(), sort_keys=True))


if __name__ == "__main__":
    main()
