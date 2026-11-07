"""Project-authored synthetic reader fixtures; never native file-format qualification proof."""

from __future__ import annotations

import argparse
import struct
from pathlib import Path

from make_test_fixture import make_semantics_contents, wrap_record_contents

ROOT = Path(__file__).resolve().parents[1]


def slot(tag: int, span: int = 1) -> bytearray:
    result = bytearray(64)
    struct.pack_into("<HH4BHH", result, 0, tag, 1, 1, 1, 1, 0, span, 1)
    return result


def line(start=(-12.5, 0.25), end=(42.0, 0.25)) -> bytearray:
    result = slot(2)
    struct.pack_into("<6d", result, 16, *start, *end, 0, 0)
    return result


def make_document_contents(records=None, definitions=()) -> bytes:
    """Rebuild the root and definition partitions with independently set counts."""
    if records is None:
        records = [line(), line((2.5, -3.75), (17.125, 8.5))]
    data = bytearray(make_semantics_contents(25))

    def get(at):
        return struct.unpack_from("<I", data, at)[0]

    suffix = data[get(0x50) - 7272 : get(0x50)]
    old_end = get(0x28)
    shift = (len(records) + 1) * 64 - (old_end - 8192)
    for at in range(0x28, 0x54, 4):
        value = get(at)
        if value:
            struct.pack_into("<I", data, at, value + shift)
    struct.pack_into("<I", data, 0x58, len(records))
    data[8192:old_end] = b"".join(records) + slot(0xFF, 0)
    data = data[: get(0x40)] + bytes(296)
    first = len(data)
    for name, children in definitions:
        header = bytearray(176)
        header[: len(name)] = name
        struct.pack_into("<I", header, 36, len(children))
        data += header + b"".join(children) + slot(0xFF, 0)
    data += suffix
    end = len(data)
    data += bytes(4)
    struct.pack_into("<II", data, 0x4C, first if definitions else 0, end)
    return bytes(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    cases = {
        "document25": make_document_contents(),
        "document-partial25": make_document_contents([line(), slot(0x7331), line()]),
    }
    for name, payload in cases.items():
        for suffix, data in (
            (".bin", payload),
            (".cfb", wrap_record_contents(payload)),
        ):
            path = ROOT / "tests/fixtures" / (name + suffix)
            if args.check:
                assert path.read_bytes() == data, path
            else:
                with path.open("xb") as stream:
                    stream.write(data)
    print("Document fixtures verified" if args.check else "Document fixtures authored")


if __name__ == "__main__":
    main()
