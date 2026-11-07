"""Author a small synthetic dimension fixture without copying native payloads."""

import argparse
import struct
from pathlib import Path

from make_test_fixture import make_record_contents, wrap_record_contents

ROOT = Path(__file__).resolve().parents[1]


def make_dimension_contents(
    marker=25, *, number=2000.0, drawing=100.0, current_ratio=1.0
):
    old = bytearray(make_record_contents(marker))
    start, end = struct.unpack_from("<2I", old, 0x24)
    count = 11
    delta = start + count * 64 - end
    data = old[:start] + bytearray(count * 64) + old[end:]
    for offset in range(0x28, 0x44, 4):
        struct.pack_into(
            "<I", data, offset, struct.unpack_from("<I", old, offset)[0] + delta
        )
    for offset in (0x44, 0x48, 0x4C):
        value = struct.unpack_from("<I", old, offset)[0]
        if value:
            struct.pack_into("<I", data, offset, value + delta)
    struct.pack_into("<I", data, 0x50, struct.unpack_from("<I", old, 0x50)[0] + delta)
    struct.pack_into("<I", data, 0x58, count - 1)
    struct.pack_into("<2d", data, 0x140, 0, 0)
    struct.pack_into("<d", data, 0x178, 1 / drawing)
    struct.pack_into("<d", data, 0x278, current_ratio)
    for index, tag in enumerate(
        (0x1A, 0x41, 0x43, 0x43, 0x44, 0x14, 0xFE, 0xFE, 0xFE, 0x50, 0xFF)
    ):
        struct.pack_into("<H", data, start + index * 64, tag)
    struct.pack_into("<H", data, start + 8, 10)
    data[start + 16 : start + 19] = bytes((1, 128, 2))
    struct.pack_into("<6d", data, start + 64 + 16, 10, 20, 1010, 20, 0, 0)
    struct.pack_into("<H", data, start + 5 * 64 + 8, 4)
    struct.pack_into("<I", data, start + 6 * 64 + 12, 1)
    struct.pack_into("<2H", data, start + 7 * 64 + 2, 19, 1)
    struct.pack_into("<d", data, start + 8 * 64 + 8, number)
    return bytes(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    payload = make_dimension_contents()
    for name, data in (
        ("dimension25.bin", payload),
        ("dimension25.cfb", wrap_record_contents(payload)),
    ):
        path = ROOT / "tests/fixtures" / name
        if args.check:
            assert path.read_bytes() == data, path
        else:
            assert not path.exists(), path
            path.write_bytes(data)
    print(
        "Synthetic dimension fixture verified"
        if args.check
        else "Synthetic dimension fixture authored"
    )


if __name__ == "__main__":
    main()
