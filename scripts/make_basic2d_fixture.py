"""Independently authored root-primitive fixtures, never native evidence."""

from __future__ import annotations

import argparse
import struct
from pathlib import Path

from make_document_fixture import make_document_contents, slot
from make_test_fixture import wrap_record_contents

ROOT = Path(__file__).resolve().parents[1]


def primary(tag, span=1):
    """Explicit file-local indices and saved opaque tail for the new profile."""
    result = slot(tag, span)
    struct.pack_into("<HBBB", result, 2, 2, 3, 2, 4)
    result[12:16] = b"\0\1\0\0"
    return result


def line(start=(-1.125, 2.25), end=(3.5, 4.75)):
    result = primary(2)
    struct.pack_into("<6d", result, 16, *start, *end, 0, 0)
    return [result]


def curve(kind="circle", center=(10.25, -20.5), radius=3.75, angles=None):
    assert kind in ("circle", "arc")
    result = primary(3 if kind == "circle" else 4, 2)
    struct.pack_into("<6d", result, 16, *center, 0, 0, 0, 0)
    tail = bytearray(64)
    tail[0] = 0xFE
    start, end = angles or ((0, 360) if kind == "circle" else (270, 15))
    struct.pack_into("<6d", tail, 16, radius, start, end, 0, 0, 0)
    return [result, tail]


def saved_literal(value):
    """Serialize authored BMP characters without copying any native string."""
    return "".join(
        character
        if 0x20 <= ord(character) <= 0x7E
        else '{\\uni"%04x"}' % ord(character)
        for character in value
    ).encode("ascii")


def text(value, *, width=2.5, height=4.0, rotation=17.0):
    encoded = saved_literal(value)
    count = 3 + (len(encoded) + 1 + 47) // 48
    result = [primary(5, count)] + [bytearray(64) for _ in range(count - 1)]
    struct.pack_into("<6d", result[0], 16, 6.125, -8.25, 0, width, height, 0)
    for row in result[1:]:
        row[0] = 0xFE
    struct.pack_into("<IIHH", result[1], 8, 1, 1, len(encoded), 8)
    result[2][2:16] = bytes((19, 0, 0, 0, 53, 0, 1, 0, 0, 0, 0, 0, 0, 0))
    struct.pack_into("<d", result[2], 16, rotation)
    font = "Synthetic Font".encode("utf-16le")
    result[2][32 : 32 + len(font)] = font
    result[3][2:6] = bytes((44, 46, 3, 2))
    for at, byte in enumerate(encoded):
        result[3 + at // 48][16 + at % 48] = byte
    return result


def metadata(name="CUR_CATEGORY", value="", owner="DRACAD_SYSTEM_ADDING_MASTER"):
    name_bytes, value_bytes = saved_literal(name), value.encode("ascii")
    body = name_bytes + b"\0" + value_bytes + b"\0"
    count = 1 + (len(body) + 47) // 48
    result = [bytearray(64) for _ in range(count)]
    struct.pack_into("<H6xH", result[0], 0, 0x33, count)
    struct.pack_into("<H", result[0], 22, len(value_bytes))
    result[0][32 : 32 + len(owner)] = owner.encode("ascii")
    for row in result[1:]:
        row[0] = 0xFE
    for at, byte in enumerate(body):
        result[1 + at // 48][16 + at % 48] = byte
    return result


def basic_records():
    return [
        *metadata(),
        *line(),
        *curve(),
        *curve("arc", center=(-3.0, 7.0), radius=9.0),
        *text("合成図面β"),
        *text("A" * 95 + "Z", width=1.75, height=5.25, rotation=-23.5),
        *curve(),  # Authored duplicate: neither geometry nor provenance is deduplicated.
        *line(),
        *metadata(
            "図面の識別番号",
            "1.00\t01234567-89AB-CDEF-0123-456789ABCDEF\t",
            "KOZO_SYSTEM_PLAN",
        ),
    ]


def make_basic2d_contents(records=None):
    return make_document_contents(basic_records() if records is None else records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    contents = make_basic2d_contents()
    for suffix, value in (("bin", contents), ("cfb", wrap_record_contents(contents))):
        path = ROOT / "tests/fixtures" / ("basic2d25." + suffix)
        if args.check:
            assert path.read_bytes() == value, path
        else:
            with path.open("xb") as stream:
                stream.write(value)
    print("Basic2D fixtures verified" if args.check else "Basic2D fixtures authored")


if __name__ == "__main__":
    main()
