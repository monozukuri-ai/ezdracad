"""Project-authored synthetic MPS wrapper vectors; never native acquisition evidence."""

from __future__ import annotations

import argparse
import hashlib
import struct
import uuid
import zlib
from pathlib import Path

from make_basic2d_fixture import basic_records
from make_document_fixture import line, make_document_contents, slot
from make_test_fixture import END, FAT, FREE, directory_entry

ROOT = Path(__file__).resolve().parents[1]
# Observed public extension, retained verbatim for the grammar test profile.
EXTENSION = bytes.fromhex("0000000018fcffff01000000000000000000244001000000")


def make_wrapper(contents: bytes, *, extension: bool = False) -> bytes:
    """An authored one-member zlib wrapper, not a relabelled native extraction."""
    prefix = bytearray(122)
    struct.pack_into("<III", prefix, 0, 1, 1, 1)
    if extension:
        struct.pack_into("<H", prefix, 18, 1)
    encoded = zlib.compress(contents)
    return (
        bytes(prefix)
        + (EXTENSION if extension else b"")
        + struct.pack("<II", len(contents), len(encoded))
        + encoded
    )


def wrap_secure_contents(stream: bytes, *, name: str = "SecureContents") -> bytes:
    """Independently assemble bounded one-stream CFB v3, mini or regular FAT."""
    assert 0 < len(stream) < 60_000
    regular = len(stream) >= 4096
    mini_count = (len(stream) + 63) // 64
    sectors = (len(stream) + 511) // 512
    root_size = 0 if regular else mini_count * 64
    mini_fat_at = END if regular else 2 + sectors
    entries = [END, FAT, *range(3, 2 + sectors), END]
    if not regular:
        entries.append(END)
    assert len(entries) <= 128
    header = bytearray(512)
    header[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    struct.pack_into("<HHHHH", header, 24, 0x3E, 3, 0xFFFE, 9, 6)
    struct.pack_into(
        "<IIIIIIIII",
        header,
        40,
        0,
        1,
        0,
        0,
        4096,
        mini_fat_at,
        0 if regular else 1,
        END,
        0,
    )
    struct.pack_into("<109I", header, 76, 1, *([FREE] * 108))
    root = directory_entry(
        "Root Entry",
        5,
        child=1,
        sector=END if regular else 2,
        size=root_size,
    )
    root[80:96] = uuid.UUID("0d4c9b60-7843-11cf-1234-56789abcdeff").bytes_le
    directory = (
        root
        + directory_entry(
            name,
            2,
            sector=2 if regular else 0,
            size=len(stream),
        )
        + bytes(256)
    )
    fat = struct.pack("<128I", *entries, *([FREE] * (128 - len(entries))))
    result = bytes(header) + directory + fat + stream.ljust(sectors * 512, b"\0")
    if not regular:
        entries = [*range(1, mini_count), END]
        result += struct.pack("<128I", *entries, *([FREE] * (128 - mini_count)))
    return result


def make_block_contents() -> bytes:
    primary = slot(6, 2)
    primary[32:38] = b"P7UNIT"
    tail = bytearray(64)
    tail[0] = 0xFE
    struct.pack_into("<6d", tail, 16, 100, -50, 0, 2, 2, 90)
    child = line((1.25, -2.5), (4.75, 8.125))
    return make_document_contents([primary, tail], [(b"P7UNIT", [child])])


def fixture_payloads() -> dict[str, bytes]:
    return {
        "mps-document25": make_document_contents(),
        "mps-extended25": make_document_contents(),
        "mps-basic2d25": make_document_contents(basic_records()),
        "mps-block25": make_block_contents(),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for name, payload in fixture_payloads().items():
        wrapper = make_wrapper(payload, extension=name == "mps-extended25")
        data = wrap_secure_contents(wrapper)
        path = ROOT / "tests/fixtures" / f"{name}.mps"
        if args.check:
            assert path.read_bytes() == data, path
        else:
            with path.open("xb") as output:
                output.write(data)
        print(name, len(data), hashlib.sha256(payload).hexdigest())


if __name__ == "__main__":
    main()
