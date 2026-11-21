"""Independently authored fixture for best-effort reading, never native evidence.

The payload is assembled from scratch with an older record generation: its
strings are Shift_JIS and its font name is not UTF-16. It holds one object of
every kind the best-effort reader decodes, one of a kind it does not, and one
that does not fit its layout.
"""

from __future__ import annotations

import argparse
import struct
from pathlib import Path

from make_test_fixture import wrap_record_contents

ROOT = Path(__file__).resolve().parents[1]
MARKER = 8
LINK = 104
REGION = 0x098D


def head(tag, span=1, *, layer=3, color=7, line_type=2, width=4, link=0):
    record = bytearray(64)
    struct.pack_into(
        "<HHBBBBHH", record, 0, tag, layer, color, line_type, width, 0, span, 9
    )
    struct.pack_into("<H", record, 14, link)
    return record


def continuation():
    record = bytearray(64)
    record[0] = 0xFE
    return record


def line(start, end, *, tag=2, **attributes):
    record = head(tag, **attributes)
    struct.pack_into("<6d", record, 16, *start, *end, 0, 0)
    return [record]


def curve(tag, center, *tail_values, radius=None):
    record = head(tag, 2)
    struct.pack_into("<2d", record, 16, *center)
    if radius is not None:
        struct.pack_into("<2d", record, 32, *radius)
    tail = continuation()
    struct.pack_into(f"<{len(tail_values)}d", tail, 16, *tail_values)
    return [record, tail]


def point(position, kind=1):
    record = head(1)
    struct.pack_into("<2dI", record, 16, *position, kind)
    return [record]


def text(value, *, tag=5, origin=0, position=(10.0, 20.0), rotation=0.0, link=0):
    body = value if isinstance(value, bytes) else value.encode("cp932")
    count = (len(body) + 1 + 47) // 48
    records = [head(tag, 3 + count, link=link)] + [
        continuation() for _ in range(2 + count)
    ]
    struct.pack_into("<6d", records[0], 16, *position, 0, 2.5, 4.0, 0.25)
    struct.pack_into("<HH", records[1], 16, len(body), origin)
    struct.pack_into("<HHHBB", records[2], 2, 19, 1, 37, 128, 49)
    struct.pack_into("<2d", records[2], 16, rotation, 5.0)
    font = "ＭＳ ゴシック".encode("cp932")
    records[2][32 : 32 + len(font)] = font
    records[3][2:6] = bytes((44, 46, 3, 2))
    struct.pack_into("<d", records[3], 8, 1234.5)
    for at, byte in enumerate(body):
        records[3 + at // 48][16 + at % 48] = byte
    return records


def group(tag, body, members, **attributes):
    members = [record for member in members for record in member]
    record = head(tag, 1 + len(members), **attributes)
    record[16 : 16 + len(body)] = body
    return [record, *members]


def part(tag, *values):
    record = bytearray(64)
    record[0] = tag
    struct.pack_into(f"<{len(values)}d", record, 16, *values)
    return [record]


def arrowhead(position, angle):
    record = head(0x34)
    struct.pack_into("<5dBB", record, 16, *position, 1.0, 3.0, angle, 5, 0x80)
    return [record]


def insert(name, position):
    record = head(6, 2)
    record[32 : 32 + len(name)] = name
    tail = continuation()
    struct.pack_into("<6d", tail, 16, *position, 0, 2.0, 2.0, 90.0)
    return [record, tail]


def ring(points, tag=0x0802):
    return [line(a, b, tag=tag) for a, b in zip(points, points[1:])]


def records():
    square = [(0, 0), (40, 0), (40, 40), (0, 40), (0, 0)]
    property_record = head(0x33, 2, layer=0, color=0, line_type=0, width=0)
    return [
        [property_record, continuation()],
        line((1.0, 2.0), (3.0, 4.0)),
        curve(3, (5.0, 6.0), 7.0, 0.0, 360.0),
        curve(4, (8.0, 9.0), 2.0, 30.0, 120.0),
        curve(7, (1.5, 2.5), 15.0, 0.0, 360.0, radius=(4.0, 2.0)),
        curve(8, (1.5, 2.5), 15.0, 10.0, 80.0, radius=(4.0, 2.0)),
        point((9.0, 8.0), 3),
        text("図面 A".encode("cp932") + b'{\\uni"9ad9"}\r2', origin=9, rotation=30.0),
        text(
            "{\\FＭＳ ゴシック|128|49;\\H1.8;\\C016;見出し}{\\FＭＳ ゴシック|128|49;\\H1.5;\\n本文}"
        ),
        group(
            10,
            bytes((0, 5, 4, 0)),
            ring(square[:3]) + [curve(0x0804, (20.0, 40.0), 20.0, 0.0, 180.0)],
        ),
        group(10, bytes((3, 0, 0, 10)), ring(square[:4])),
        group(
            12,
            struct.pack("<BBBxI4xI", 0, 5, 6, 1, REGION),
            [group(10, bytes((0, 1, 0, 0)), ring(square))],
        ),
        group(
            13,
            struct.pack("<HI", 0, REGION),
            [part(0x4B, 0.0, 0.0, 5.0, 45.0), part(0x4B, 0.0, 0.0, 5.0, 135.0)],
        ),
        group(
            26,
            bytes((2, 33, 0, 3, 1, 3, 1, 51)),
            [
                part(0x41, 0.0, 60.0, 40.0, 60.0),
                part(0x43, 0.0, 40.0),
                part(0x43, 40.0, 40.0),
                part(0x44, 3.0, 3.0, 3.0, 3.0),
                text("40", tag=20, origin=1, position=(20.0, 60.0), link=LINK),
            ],
            link=LINK,
        ),
        group(
            48,
            struct.pack("<BBxxI", 3, 0, 1),
            [
                line((0.0, 60.0), (40.0, 60.0), link=LINK),
                line((0.0, 40.0), (0.0, 60.0)),
                point((0.0, 40.0)),
                arrowhead((0.0, 60.0), 180.0),
                arrowhead((40.0, 60.0), 0.0),
                text("40", tag=20, origin=1, position=(20.0, 60.0), link=LINK),
            ],
        ),
        insert(b"SYM", (100.0, 200.0)),
        [head(0x19)],
        curve(4, (0.0, 0.0), -1.0, 0.0, 90.0),
        line((7.0, 7.0), (8.0, 8.0), tag=0x2402, color=0, line_type=0, width=255),
    ]


def make_contents():
    root = b"".join(record for item in records() for record in item)
    data = bytearray(8192)
    data[:18] = b"DRACAD for Windows"
    struct.pack_into("<H", data, 0x22, MARKER)
    struct.pack_into("<2d", data, 0x140, 0.0, 0.0)
    struct.pack_into("<d", data, 0x178, 1 / 50)
    struct.pack_into("<4d", data, 0x188, 420.0, 297.0, -100.0, 14850.0)
    data += root + bytes([0xFF]) + bytes(63)
    sections = [len(data)]
    layers = bytearray(16320)
    layers[2 * 64 + 1] = 0x04
    struct.pack_into("<3H", layers, 2 * 64 + 4, 6, 2, 4)
    name = "寸法".encode("cp932")
    layers[2 * 64 + 32 : 2 * 64 + 32 + len(name)] = name
    layers[4 * 64 + 1] = 0x01
    colors = bytearray(16320)
    struct.pack_into("<H3B", colors, 6 * 64 + 2, 35, 10, 20, 30)
    line_types = bytearray(2048)
    struct.pack_into("<3H", line_types, 64 + 2, 1, 300, 100)
    widths = bytearray(1024)
    struct.pack_into("<H", widths, 3 * 64 + 2, 50)
    for table in (layers, colors, line_types, widths, bytes(3312), bytes(4096)):
        data += table
        sections.append(len(data))
    data += bytes(296)
    block_start = len(data)
    members = line((0.0, 0.0), (1.0, 0.0)) + curve(3, (0.5, 0.0), 0.25, 0.0, 360.0)
    header = bytearray(176)
    header[:3] = b"SYM"
    struct.pack_into("<I", header, 36, len(members))
    data += header + b"".join(members) + bytes([0xFF]) + bytes(63)
    tail = bytearray(1768)
    struct.pack_into("<4I", tail, 0, 2, 256, 0, 64)
    tail[16 + 4 : 16 + 7] = bytes((1, 2, 3))
    struct.pack_into("<4I", tail, 80, 1, 0, 0, 296)
    struct.pack_into("<4I", tail, 392, 1, 256, 0, 296)
    data += tail
    end = len(data)
    data += bytes(4)
    struct.pack_into("<I7I", data, 0x24, 8192, *sections)
    struct.pack_into("<2I", data, 0x4C, block_start, end)
    struct.pack_into("<I", data, 0x58, len(root) // 64)
    return bytes(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    contents = make_contents()
    for suffix, value in (("bin", contents), ("cfb", wrap_record_contents(contents))):
        path = ROOT / "tests/fixtures" / (f"best-effort{MARKER}." + suffix)
        if args.check:
            assert path.read_bytes() == value, path
        else:
            with path.open("xb") as stream:
                stream.write(value)
    print(
        "Best-effort fixtures verified"
        if args.check
        else "Best-effort fixtures authored"
    )


if __name__ == "__main__":
    main()
