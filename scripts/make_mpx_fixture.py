"""Project-authored synthetic flat MPX grammar fixtures, never native acquisition evidence."""

from __future__ import annotations

import argparse
import struct
from pathlib import Path

from make_basic2d_fixture import basic_records
from make_document_fixture import line, make_document_contents, slot

ROOT = Path(__file__).resolve().parents[1]


def make_mpx(contents: bytes) -> bytes:
    """Assemble the distinct observed flat envelope over authored test records.

    The source is generated synthetic data, not a native MPZ extraction or an
    acquired .bin relabelled as a native MPX. Original test bytes are untouched.
    """
    assert contents.startswith(b"DRACAD for Windows")
    assert struct.unpack_from("<H", contents, 34)[0] == 25
    assert struct.unpack_from("<I", contents, 0x50)[0] == len(contents) - 4
    assert contents[-4:] == bytes(4)
    result = bytearray(contents[:-4])
    struct.pack_into("<I", result, 0x50, 0)
    return bytes(result)


def make_mpx_records(records=None) -> bytes:
    return make_mpx(make_document_contents(records))


def make_mixed_mpx() -> bytes:
    records = basic_records()
    index = 0
    while index < len(records):
        tag, span = struct.unpack_from("<H6xH", records[index])
        if tag in (2, 3, 4, 5):
            # Independent hostile/API vector. This is not evidence that all
            # native primitive kinds use this otherwise opaque saved tail.
            records[index][12:16] = bytes(4)
        index += span
    return make_mpx_records(records)


def make_block_mpx() -> bytes:
    primary = slot(6, 2)
    primary[32:38] = b"P6UNIT"
    tail = bytearray(64)
    tail[0] = 0xFE
    struct.pack_into("<6d", tail, 16, 100, -50, 0, 2, 2, 90)
    child = line((1.25, -2.5), (4.75, 8.125))
    contents = make_document_contents([primary, tail], [(b"P6UNIT", [child])])
    result = make_mpx(contents)
    assert len(result) % 64 == 48  # The 176-byte definition header is not 64 bytes.
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    fixtures = {
        "mpx25.mpx": make_mpx_records(),
        "mpx-mixed25.mpx": make_mixed_mpx(),
        "mpx-block25.mpx": make_block_mpx(),
    }
    for name, value in fixtures.items():
        path = ROOT / "tests/fixtures" / name
        if args.check:
            assert path.read_bytes() == value, path
        else:
            with path.open("xb") as stream:
                stream.write(value)
    print("MPX fixtures verified" if args.check else "MPX fixtures authored")


if __name__ == "__main__":
    main()
