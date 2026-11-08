"""Collect or verify the qualified wheels, source archive and SHA-256 manifest."""

from __future__ import annotations

import argparse
import hashlib
import re
import shutil
from pathlib import Path

from bump_version import ROOT, check_versions, python_version
from packaging.utils import parse_wheel_filename
from packaging.version import Version
from verify_distribution import inspect_artifact

PLATFORMS = {
    "manylinux_2_28_x86_64": "linux-x86_64",
    "manylinux_2_28_aarch64": "linux-aarch64",
    "win_amd64": "windows-x86_64",
}
EXPECTED_PLATFORMS = {*PLATFORMS.values(), "macos-x86_64", "macos-arm64"}


def inventory(directory: Path, version: str) -> list[Path]:
    """Require exactly one CPython 3.10 ABI3 wheel per platform and one sdist."""
    files = sorted(directory.iterdir())
    platforms = set()
    sdist = f"ezdracad-{version}.tar.gz"
    found_sdist = False
    for path in files:
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Expected a regular distribution: {path.name}")
        if path.name == sdist:
            found_sdist = True
            continue
        if path.suffix != ".whl":
            raise ValueError(f"Unexpected distribution: {path.name}")
        name, wheel_version, build, tags = parse_wheel_filename(path.name)
        if name != "ezdracad" or wheel_version != Version(version) or build:
            raise ValueError(f"Unexpected wheel identity: {path.name}")
        roles = set()
        for tag in tags:
            platform = PLATFORMS.get(tag.platform)
            macos = re.fullmatch(r"macosx_\d+_\d+_(x86_64|arm64)", tag.platform)
            if macos:
                platform = f"macos-{macos[1]}"
            if tag.interpreter != "cp310" or tag.abi != "abi3" or platform is None:
                raise ValueError(f"Unexpected wheel tag: {path.name}")
            roles.add(platform)
        if len(roles) != 1 or platforms & roles:
            raise ValueError(f"Duplicate or ambiguous wheel platform: {path.name}")
        platforms.update(roles)
    if platforms != EXPECTED_PLATFORMS or not found_sdist:
        raise ValueError("Release requires five platform wheels and one source archive")
    return files


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def collect(downloads: Path, output: Path) -> None:
    version = python_version(check_versions(ROOT))
    files = inventory(downloads, version)
    for path in files:
        inspect_artifact(path, version)
    if output.exists() and any(output.iterdir()):
        raise ValueError("Release bundle output must be empty")
    dist = output / "dist"
    dist.mkdir(parents=True)
    checksums = []
    for source in files:
        destination = dist / source.name
        shutil.copyfile(source, destination)
        checksums.append(f"{digest(destination)}  {destination.name}\n")
    (output / "SHA256SUMS").write_text("".join(checksums), encoding="ascii")
    verify_bundle(output)


def verify_bundle(bundle: Path) -> None:
    version = python_version(check_versions(ROOT))
    if {p.name for p in bundle.iterdir()} != {"dist", "SHA256SUMS"}:
        raise ValueError("Unexpected release bundle contents")
    files = inventory(bundle / "dist", version)
    checksums = {}
    for line in (bundle / "SHA256SUMS").read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([a-f0-9]{64})  ([A-Za-z0-9_.-]+)", line)
        if not match or match[2] in checksums:
            raise ValueError("Invalid or duplicate checksum entry")
        checksums[match[2]] = match[1]
    if checksums.keys() != {p.name for p in files}:
        raise ValueError("Release checksum inventory mismatch")
    for path in files:
        if digest(path) != checksums[path.name]:
            raise ValueError(f"Release checksum mismatch: {path.name}")
        inspect_artifact(path, version)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    prepare = actions.add_parser("collect")
    prepare.add_argument("downloads", type=Path)
    prepare.add_argument("output", type=Path)
    verify = actions.add_parser("verify")
    verify.add_argument("bundle", type=Path)
    args = parser.parse_args()
    if args.action == "collect":
        collect(args.downloads, args.output)
    else:
        verify_bundle(args.bundle)
    print("Release inventory, metadata, public contents and checksums verified")


if __name__ == "__main__":
    main()
