"""Project-authored synthetic 3D block fixtures; never copies vendor/native control bytes."""

import struct
from pathlib import Path

from make_test_fixture import make_block_contents, wrap_record_contents

ROOT = Path(__file__).resolve().parents[1]


def make_block3d_contents(marker=25, tag=6):
    data = bytearray(make_block_contents(marker))
    data[8192 : 8192 + 256] = bytes(256)
    struct.pack_into("<HH4BHH4B", data, 8192, tag, 1, 1, 1, 1, 0, 3, 1, 0, 1, 0, 0)
    data[8192 + 17] = 1
    data[8192 + 32 : 8192 + 64] = b"TWO\0" + b"\xa5" * 28
    struct.pack_into("<H", data, 8192 + 64, 0xFE)
    struct.pack_into("<6d", data, 8192 + 80, 100, -50, 25, 2, 3, 0)
    struct.pack_into("<H", data, 8192 + 128, 0xFE)
    struct.pack_into("<6d", data, 8192 + 144, 0, 0, 0.5, 0, 0, 0)
    struct.pack_into("<HH4BHH", data, 8192 + 192, 0x33, 0, 0, 0, 0, 0, 1, 0)
    first = struct.unpack_from("<I", data, 0x4C)[0]
    count = struct.unpack_from("<I", data, first + 36)[0] + 1
    header = first + 176 + count * 64
    data[header + 32 : header + 36] = bytes(4)
    data[header + 40 : header + 112] = bytes(72)
    struct.pack_into("<I", data, header + 40, 1)
    data[header + 160 : header + 176] = bytes(16)
    points = [
        (1.25, -2.5, 12.5, 101.75, 48.125, -3.75),
        (-12.5, 0.25, 5, 42, 0.25, 7.5),
    ]
    struct.pack_into(
        "<6d", data, header + 112, 101.75, 48.125, 12.5, -12.5, -2.5, -3.75
    )
    for i, values in enumerate(points):
        at = header + 176 + i * 64
        data[at : at + 64] = bytes(64)
        struct.pack_into(
            "<HH4BHH4B", data, at, 0x13, 1, 1, 1, 1, 0, 1, i + 1, 0, 0, 0, 0
        )
        struct.pack_into("<6d", data, at + 16, *values)
    return bytes(data)


if __name__ == "__main__":
    payload = make_block3d_contents()
    for name, data in [
        ("block3d25.bin", payload),
        ("block3d25.cfb", wrap_record_contents(payload)),
    ]:
        target = ROOT / "tests/fixtures" / name
        with target.open("xb") as f:
            f.write(data)
        print(name, len(data))
