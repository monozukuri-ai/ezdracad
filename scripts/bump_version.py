"""Update/check the shared Cargo and Python release version."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 development environment
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-(alpha|beta|rc)\.(0|[1-9][0-9]*))?"
)


def python_version(version: str) -> str:
    match = VERSION.fullmatch(version)
    if match is None:
        raise ValueError("use X.Y.Z or X.Y.Z-{alpha,beta,rc}.N, without a v prefix")
    major, minor, patch, stage, number = match.groups()
    suffix = {"alpha": "a", "beta": "b", "rc": "rc"}
    return f"{major}.{minor}.{patch}" + (f"{suffix[stage]}{number}" if stage else "")


def read_toml(path: Path) -> dict:
    with path.open("rb") as stream:
        return tomllib.load(stream)


def check_versions(root: Path, tag: str | None = None) -> str:
    workspace = read_toml(root / "Cargo.toml")["workspace"]
    version = workspace["package"]["version"]
    expected_python = python_version(version)
    project = read_toml(root / "pyproject.toml")["project"]
    if "version" in project or "version" not in project.get("dynamic", []):
        raise ValueError("pyproject.toml must use dynamic version metadata")
    locked = read_toml(root / "Cargo.lock")["package"]
    for member in workspace["members"]:
        package = read_toml(root / member / "Cargo.toml")["package"]
        if package["version"] != {"workspace": True}:
            raise ValueError(f"{member} must inherit version.workspace = true")
        versions = [p["version"] for p in locked if p["name"] == package["name"]]
        if versions != [version]:
            raise ValueError(f"Cargo.lock is stale for {package['name']}")
    fuzz_locked = read_toml(root / "fuzz/Cargo.lock")["package"]
    if [p["version"] for p in fuzz_locked if p["name"] == "ezdracad-core"] != [version]:
        raise ValueError("fuzz/Cargo.lock is stale for ezdracad-core")
    local = [
        p
        for p in read_toml(root / "uv.lock")["package"]
        if p["name"] == project["name"]
    ]
    if len(local) != 1 or local[0].get("source") != {"editable": "."}:
        raise ValueError("uv.lock must contain the local editable project")
    # uv can omit the version for local projects with dynamic metadata.
    if local[0].get("version", expected_python) != expected_python:
        raise ValueError("uv.lock contains an outdated project version")
    expected_tags = {f"v{version}", f"v{expected_python}"}
    if tag is not None and tag not in expected_tags:
        raise ValueError(
            f"tag {tag!r} does not match {' or '.join(sorted(expected_tags))}"
        )
    return version


def bump_version(root: Path, version: str) -> None:
    python_version(version)
    paths = [
        root / name
        for name in ("Cargo.toml", "Cargo.lock", "uv.lock", "fuzz/Cargo.lock")
    ]
    originals = {path: path.read_bytes() for path in paths}
    manifest = originals[paths[0]].decode("utf-8")
    updated, count = re.subn(
        r'(?ms)(^\[workspace\.package\]\r?\n(?:(?!^\[).)*?^version\s*=\s*")[^"]+("[^\n]*)',
        lambda match: f"{match[1]}{version}{match[2]}",
        manifest,
    )
    if count != 1:
        raise ValueError("expected one workspace.package.version")
    try:
        paths[0].write_bytes(updated.encode("utf-8"))
        subprocess.run(["cargo", "update", "--workspace"], cwd=root, check=True)
        subprocess.run(
            ["cargo", "update", "--manifest-path", "fuzz/Cargo.toml", "--workspace"],
            cwd=root,
            check=True,
        )
        subprocess.run(["uv", "lock"], cwd=root, check=True)
        check_versions(root)
    except BaseException:
        for path, data in originals.items():
            path.write_bytes(data)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("version", nargs="?")
    action.add_argument("--check", action="store_true")
    parser.add_argument(
        "--tag", help="expected v<Cargo or Python version> tag; only with --check"
    )
    args = parser.parse_args()
    if args.tag and not args.check:
        parser.error("--tag requires --check")
    try:
        if args.check:
            version = check_versions(ROOT, args.tag)
            subprocess.run(["uv", "lock", "--check"], cwd=ROOT, check=True)
            print(f"Versions and locks are consistent: {version}")
        else:
            bump_version(ROOT, args.version)
            print(f"Updated shared version to {args.version}")
    except (OSError, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
