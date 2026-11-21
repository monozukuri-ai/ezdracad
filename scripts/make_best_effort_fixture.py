"""Independently authored fixtures for best-effort reading, never native evidence.

Two payloads are assembled from scratch with the same content. `best-effort8`
uses the older record layout: 64-byte records, Shift_JIS strings and a font
name that is not UTF-16. `best-effort25u` uses the Unicode record layout:
80-byte records and UTF-16 strings, inside a compressed envelope. Each holds one
object of every kind the best-effort reader decodes, one of a kind it does not,
and one that does not fit its layout.
"""

from __future__ import annotations

import argparse
import struct
import zlib
from pathlib import Path

from make_mps_fixtures import wrap_secure_contents
from make_test_fixture import wrap_record_contents

ROOT = Path(__file__).resolve().parents[1]
LINK = 104
REGION = 0x098D


class Layout:
    """Record builder for the older (64-byte) or the Unicode (80-byte) layout."""

    def __init__(self, wide):
        self.wide = wide
        self.size = 80 if wide else 64

    def encode(self, value):
        return value.encode("utf-16-le" if self.wide else "cp932")

    def head(self, tag, span=1, *, body=b"", **attributes):
        """An object or attributed part: attribute references and a 48-byte body."""
        layer = attributes.get("layer", 3)
        color = attributes.get("color", 7)
        line_type = attributes.get("line_type", 2)
        width = attributes.get("width", 4)
        link = attributes.get("link", 0)
        record = bytearray(self.size)
        if self.wide:
            struct.pack_into(
                "<H2xIBBHIH2xH",
                record,
                0,
                tag,
                color,
                line_type,
                width,
                layer,
                span,
                9,
                link,
            )
        else:
            struct.pack_into(
                "<HHBBBxHH2xH",
                record,
                0,
                tag,
                layer,
                color,
                line_type,
                width,
                span,
                9,
                link,
            )
        assert len(body) <= 48
        at = self.size - 48
        record[at : at + len(body)] = body
        return record

    def plain(self, tag, body=b"", header=b""):
        """A continuation or plain part: a 16-byte header, the rest is body."""
        record = bytearray(self.size)
        record[0] = tag
        record[2 : 2 + len(header)] = header
        assert len(body) <= self.size - 16
        record[16 : 16 + len(body)] = body
        return record

    def line(self, start, end, *, tag=2, **attributes):
        body = struct.pack("<6d", *start, *end, 0, 0)
        return [self.head(tag, body=body, **attributes)]

    def line_3d(self, start, end):
        return [self.head(19, body=struct.pack("<6d", *start, *end))]

    def curve(self, tag, center, *tail_values, radius=(), fill=b""):
        """Circle, arc, ellipse or elliptical arc: a head and one continuation."""
        body = struct.pack(f"<{2 + len(radius)}d", *center, *radius)
        tail = struct.pack(f"<{len(tail_values)}d", *tail_values).ljust(32, b"\0")
        return [self.head(tag, 2, body=body), self.plain(0xFE, tail + fill)]

    def point(self, position, kind=1):
        return [self.head(1, body=struct.pack("<2dI", *position, kind))]

    def text(self, value, *, tag=5, origin=0, position=(10.0, 20.0), **attributes):
        body = value if isinstance(value, bytes) else self.encode(value)
        unit = 2 if self.wide else 1
        capacity = self.size - 16
        count = (len(body) + unit + capacity - 1) // capacity
        cell = struct.pack("<6d", *position, 0, 2.5, 4.0, 0.25)
        style = struct.pack("<2d", attributes.pop("rotation", 0.0), 5.0)
        records = [
            self.head(tag, 3 + count, body=cell, **attributes),
            self.plain(0xFE, struct.pack("<HH", len(body) // unit, origin)),
            self.plain(
                0xFE,
                style + self.encode("ＭＳ ゴシック"),
                struct.pack("<HHHBB", 19, 1, 37, 128, 49),
            ),
        ]
        number = bytes((44, 46, 3, 2, 0, 0)) + struct.pack("<d", 1234.5)
        for index in range(count):
            chunk = body[index * capacity : (index + 1) * capacity]
            records.append(self.plain(0xFE, chunk, number if index == 0 else b""))
        return records

    def group(self, tag, body, members, **attributes):
        members = [record for member in members for record in member]
        head = self.head(tag, 1 + len(members), body=body, **attributes)
        return [head, *members]

    def part(self, tag, *values):
        return [self.plain(tag, struct.pack(f"<{len(values)}d", *values))]

    def arrowhead(self, position, angle):
        body = struct.pack("<5dBB", *position, 1.0, 3.0, angle, 5, 0x80)
        return [self.head(0x34, body=body)]

    def insert(self, name, position):
        return [
            self.head(6, 2, body=bytes(16) + self.encode(name)),
            self.plain(0xFE, struct.pack("<6d", *position, 0, 2.0, 2.0, 90.0)),
        ]

    def placed_file(self, tag, first, second, *names):
        """An image or external reference: two continuations and its file names."""
        capacity = self.size - 16
        data = b"".join(self.encode(name + "\0") for name in names)
        assert len(data) <= 2 * capacity
        return [
            self.head(tag, 5),
            self.plain(0xFE, struct.pack("<6d", *first)),
            self.plain(0xFE, struct.pack("<3d", *second)),
            self.plain(0x4A, data[:capacity]),
            self.plain(0xFE, data[capacity:]),
        ]

    def ring(self, points, tag=0x0802):
        return [self.line(a, b, tag=tag) for a, b in zip(points, points[1:])]

    def region(self, flags, color, serial, region, loops):
        """A region bounded by closed polylines; the first is the outer one."""
        body = struct.pack(
            "<BBBxH6xHH", 0, flags, color, serial, region, len(loops) - 1
        )
        loops = [self.group(10, bytes((0, 1, 0, 0)), self.ring(loop)) for loop in loops]
        return self.group(12, body, loops)

    def text_values(self):
        """The same strings, saved the way each layout saves them."""
        if self.wide:
            return "図面 A髙\r2", "58°"
        return "図面 A".encode("cp932") + b'{\\uni"9ad9"}\r2', "58°"

    def records(self):
        square = [(0, 0), (40, 0), (40, 40), (0, 40), (0, 0)]
        hole = [(10, 10), (30, 10), (30, 30), (10, 30), (10, 10)]
        mixed, degrees = self.text_values()
        note = {"position": (60.0, 30.0)}
        property_record = self.head(0x33, 2, layer=0, color=0, line_type=0, width=0)
        return [
            [property_record, self.plain(0xFE)],
            self.line((1.0, 2.0), (3.0, 4.0)),
            # Filled with an explicit color, 40% transparent.
            self.curve(
                3,
                (5.0, 6.0),
                7.0,
                0.0,
                360.0,
                fill=bytes((0xFD, 0, 40, 0x0C, 10, 20, 30)),
            ),
            self.curve(4, (8.0, 9.0), 2.0, 30.0, 120.0),
            # Filled with palette color 6, without its outline.
            self.curve(
                7,
                (1.5, 2.5),
                15.0,
                0.0,
                360.0,
                radius=(4.0, 2.0),
                fill=bytes((6, 0, 0, 0x06)),
            ),
            self.curve(8, (1.5, 2.5), 15.0, 10.0, 80.0, radius=(4.0, 2.0)),
            self.point((9.0, 8.0), 3),
            self.text(mixed, origin=9, rotation=30.0),
            self.text(
                "{\\FＭＳ ゴシック|128|49;\\H1.8;\\C016;見出し}"
                "{\\FＭＳ ゴシック|128|49;\\H1.5;\\n本文}"
            ),
            self.group(
                10,
                bytes((0, 5, 4, 0)),
                self.ring(square[:3])
                # The path runs from (40, 40) back to (0, 40): against the angles.
                + [self.curve(0x1804, (20.0, 40.0), 20.0, 180.0, 360.0)],
            ),
            self.group(10, bytes((3, 0, 0, 10)), self.ring(square[:4])),
            # A region hatched with two line families ...
            self.region(1, 0, 1, REGION, [square]),
            self.group(
                13,
                struct.pack("<HH", 0, REGION),
                [
                    [self.head(0x4B, body=struct.pack("<4d", 0.0, 0.0, 5.0, 45.0))],
                    [self.head(0x4B, body=struct.pack("<4d", 0.0, 0.0, 5.0, 135.0))],
                ],
            ),
            # ... a filled one with a hole, and one hatched with a named pattern.
            self.region(5, 6, 0, 0, [square, hole]),
            self.region(3, 0, 2, REGION + 1, [hole]),
            self.group(
                13,
                struct.pack("<HH", 1, REGION + 1),
                [
                    [
                        self.head(
                            0x4C,
                            3,
                            body=struct.pack("<6d", 3.0, 4.0, 0.38, 360.0, 1.0, 90.0),
                        ),
                        self.plain(0x4D, struct.pack("<3d", 0.005, 0.005, 360.0)),
                        self.plain(0x4E, self.encode("断熱材2")),
                    ]
                ],
            ),
            # A linear dimension and the package that draws it.
            self.group(
                26,
                bytes((2, 33, 0, 3, 1, 3, 1, 51)),
                [
                    self.part(0x41, 0.0, 60.0, 40.0, 60.0),
                    self.part(0x43, 0.0, 40.0),
                    self.part(0x43, 40.0, 40.0),
                    self.part(0x44, 3.0, 3.0, 3.0, 3.0),
                    self.text("40", tag=20, origin=1, position=(20.0, 60.0), link=LINK),
                ],
                link=LINK,
            ),
            self.group(
                48,
                struct.pack("<BBxxI", 3, 0, 1),
                [
                    self.line((0.0, 60.0), (40.0, 60.0), link=LINK),
                    self.line((0.0, 40.0), (0.0, 60.0)),
                    self.point((0.0, 40.0)),
                    self.arrowhead((0.0, 60.0), 180.0),
                    self.arrowhead((40.0, 60.0), 0.0),
                    self.text("40", tag=20, origin=1, position=(20.0, 60.0), link=LINK),
                ],
            ),
            # An angle dimension keeps its arc.
            self.group(
                26,
                bytes((3, 0x80, 2, 5, 1, 5, 1, 19)),
                [
                    self.part(0x41, 10.0, 0.0, 5.3, 8.5),
                    self.part(0x42, 0.0, 0.0, 10.0, 0.0, 58.0),
                    self.part(0x43, 10.0, 0.0),
                    self.part(0x43, 5.3, 8.5),
                    self.part(0x44, 100.0, 300.0, 100.0, 300.0),
                    self.text(degrees, tag=20, origin=1, link=LINK + 1),
                    self.part(0x50, 1.0, 3.0, 1.0, 3.0),
                ],
                link=LINK + 1,
            ),
            # A leader and the package that draws it.
            self.group(
                27,
                bytes((1, 0x80, 0, 7)) + bytes(12) + struct.pack("<4d", 1, 3, 100, 300),
                [
                    self.part(0x46, 10.0, 10.0, 21.5, 30.0),
                    self.part(0x46, 21.5, 30.0, 60.0, 30.0),
                    self.text("注記", link=LINK + 2, **note),
                ],
                link=LINK + 2,
            ),
            self.group(
                48,
                struct.pack("<BBxxI", 4, 0, 2),
                [
                    self.line((10.0, 10.0), (21.5, 30.0), link=LINK + 2),
                    self.line((21.5, 30.0), (60.0, 30.0)),
                    self.arrowhead((10.0, 10.0), 240.0),
                    self.text("注記", link=LINK + 2, **note),
                ],
            ),
            self.insert("SYM", (100.0, 200.0)),
            self.placed_file(
                25,
                (335.0, 107.0, 0.0, 0.4, 0.4, 1.0),
                (69.0, 49.0, 90.0),
                "写真.tif",
                "E:\\Photo\\",
            ),
            self.placed_file(
                24,
                (1.0, 2.0, 0.0, 1.0, 1.0, 1.0),
                (0.0, 0.0, 30.0),
                "図面枠.mps",
                "D:\\Work\\",
                ".\\",
            ),
            self.line_3d((1.0, 2.0, 3.0), (4.0, 5.0, 6.0)),
            [self.head(0x16)],
            self.curve(4, (0.0, 0.0), -1.0, 0.0, 90.0),
            self.line(
                (7.0, 7.0), (8.0, 8.0), tag=0x2402, color=0, line_type=0, width=255
            ),
        ]

    def block_members(self):
        return self.line((0.0, 0.0), (1.0, 0.0)) + self.curve(
            3, (0.5, 0.0), 0.25, 0.0, 360.0
        )


def make_contents(layout, marker):
    wide = layout.wide
    root = b"".join(record for item in layout.records() for record in item)
    shift = 0x40 if wide else 0
    data = bytearray(8192)
    data[:18] = b"DRACAD for Windows"
    struct.pack_into("<H", data, 0x22, marker)
    struct.pack_into("<2d", data, 0x140 + shift, 0.0, 0.0)
    struct.pack_into("<d", data, 0x178 + shift, 1 / 50)
    struct.pack_into("<4d", data, 0x188 + shift, 420.0, 297.0, -100.0, 14850.0)
    data += root + layout.plain(0xFF)
    sections = [len(data)]
    # Rows of the Unicode tables are longer and start at index zero.
    long_row, short_row, first = (552, 96, 0) if wide else (64, 64, 1)
    counts = (512, 258, 256, 256) if wide else (255, 255, 32, 16)
    layers = bytearray(counts[0] * long_row)
    at = (3 - first) * long_row
    layers[at + 1] = 0x04
    struct.pack_into("<3H", layers, at + 4, 6, 2, 4)
    name = layout.encode("寸法")
    layers[at + 32 : at + 32 + len(name)] = name
    layers[(5 - first) * long_row + 1] = 0x01
    colors = bytearray(counts[1] * short_row)
    struct.pack_into("<H3B", colors, (7 - first) * short_row + 2, 35, 10, 20, 30)
    line_types = bytearray(counts[2] * long_row)
    struct.pack_into("<3H", line_types, (2 - first) * long_row + 2, 1, 300, 100)
    widths = bytearray(counts[3] * short_row)
    struct.pack_into("<H", widths, (4 - first) * short_row + 2, 50)
    extra = (bytes(13824), bytes(6400)) if wide else (bytes(3312), bytes(4096))
    if wide:
        # Color 256 is a table row here; the older layout keeps it in its tail.
        colors[256 * short_row + 4 : 256 * short_row + 7] = bytes((1, 2, 3))
    for table in (layers, colors, line_types, widths, *extra):
        data += table
        sections.append(len(data))
    data += bytes(1096 if wide else 296)
    block_start = len(data)
    members = layout.block_members()
    header = bytearray(656 if wide else 176)
    name = layout.encode("SYM")
    header[: len(name)] = name
    struct.pack_into("<I", header, 524 if wide else 36, len(members))
    data += header + b"".join(members) + layout.plain(0xFF)
    if wide:
        data += bytes(1024)
    else:
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
    struct.pack_into("<I", data, 0x58, len(root) // layout.size)
    return bytes(data)


def unicode_envelope(contents):
    """An authored one-member compressed envelope of the Unicode layout."""
    prefix = bytearray(122)
    struct.pack_into("<III", prefix, 0, 1, 1, 1)
    struct.pack_into("<H", prefix, 18, 2)
    encoded = zlib.compress(contents, 9)
    return bytes(prefix) + struct.pack("<II", len(contents), len(encoded)) + encoded


def fixtures():
    narrow = make_contents(Layout(False), 8)
    wide = make_contents(Layout(True), 25)
    return {
        "best-effort8.bin": narrow,
        "best-effort8.cfb": wrap_record_contents(narrow),
        "best-effort25u.mpuw": wrap_secure_contents(unicode_envelope(wide)),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for name, value in fixtures().items():
        path = ROOT / "tests/fixtures" / name
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
