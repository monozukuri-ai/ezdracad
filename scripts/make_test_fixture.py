"""Create an independently assembled MS-CFB v3 fixture, not a DRA-CAD file."""

import struct
import uuid
from pathlib import Path

FREE = 0xFFFFFFFF
END = 0xFFFFFFFE
FAT = 0xFFFFFFFD
ROOT = Path(__file__).resolve().parents[1]


def directory_entry(name, kind, *, child=FREE, sector=0, size=0):
    entry = bytearray(128)
    encoded = (name + "\0").encode("utf-16le")
    entry[: len(encoded)] = encoded
    struct.pack_into("<HBBIII", entry, 64, len(encoded), kind, 1, FREE, FREE, child)
    struct.pack_into("<IQ", entry, 116, sector, size)
    return entry


def make_fixture():
    header = bytearray(512)
    header[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    struct.pack_into("<HHHHH", header, 24, 0x3E, 3, 0xFFFE, 9, 6)
    struct.pack_into("<IIIIIIIII", header, 40, 0, 1, 0, 0, 4096, 3, 1, END, 0)
    struct.pack_into("<109I", header, 76, 1, *([FREE] * 108))
    directory = (
        directory_entry("Root Entry", 5, child=1, sector=2, size=64)
        + directory_entry("Example", 1, child=2)
        + directory_entry("Note", 2, sector=0, size=11)
        + bytes(128)
    )
    fat = struct.pack("<128I", END, FAT, END, END, *([FREE] * 124))
    mini_stream = b"foundation\n".ljust(512, b"\0")
    mini_fat = struct.pack("<128I", END, *([FREE] * 127))
    return bytes(header) + directory + fat + mini_stream + mini_fat


def make_mpz_fixture():
    """Synthetic MPZ-shaped envelope; payload is not a valid native drawing."""
    contents = bytearray(96)
    contents[:18] = b"DRACAD for Windows"
    struct.pack_into("<H", contents, 0x22, 0x3456)
    contents[36:] = (
        b"Synthetic test payload; geometry intentionally not encoded.\n".ljust(
            60, b"\0"
        )
    )
    data = bytearray(make_fixture())
    root = directory_entry("Root Entry", 5, child=1, sector=2, size=128)
    root[80:96] = uuid.UUID("0d4c9b60-7843-11cf-1234-56789abcdeff").bytes_le
    data[512:640] = root
    data[640:768] = directory_entry("Contents", 2, sector=0, size=len(contents))
    data[768:896] = bytes(128)
    data[1536:2048] = bytes(contents).ljust(512, b"\0")
    data[2048:2560] = struct.pack("<128I", 1, END, *([FREE] * 126))
    return bytes(data), bytes(contents)


def make_record_contents(marker=5):
    """Synthetic observed layout, independently authored candidate XY constants.

    This exercises bounds, not native drawing semantics or application validity.
    """
    start = 8192
    end = start + 5 * 64
    deltas = [16320, 16320, 2048, 1024, 1656 if marker == 5 else 3312, 4096]
    boundaries = [end]
    for delta in deltas:
        boundaries.append(boundaries[-1] + delta)
    payload_end = boundaries[-1] + 64
    data = bytearray(payload_end + 4)
    data[:18] = b"DRACAD for Windows"
    struct.pack_into("<H", data, 0x22, marker)
    struct.pack_into("<I", data, 0x24, start)
    struct.pack_into("<7I", data, 0x28, *boundaries)
    struct.pack_into("<I", data, 0x4C, boundaries[-1] + 8)
    struct.pack_into("<I", data, 0x50, payload_end)
    struct.pack_into("<I", data, 0x58, 4)
    struct.pack_into("<H", data, start, 2)
    struct.pack_into("<4d", data, start + 16, -12.5, 0.25, 42.0, -3.125)
    struct.pack_into("<H", data, start + 64, 0x0802)
    data[start + 66 : start + 128] = bytes(range(62))
    struct.pack_into("<H", data, start + 128, 2)
    struct.pack_into("<4d", data, start + 144, 0.0, -0.0, 0.0, -0.0)
    struct.pack_into("<H", data, start + 192, 0xFE)
    data[start + 194 : start + 256] = b"Opaque continuation; semantics unknown".ljust(
        62, b"!"
    )
    struct.pack_into("<H", data, end - 64, 0xFF)
    data[128:160] = b"Uninterpreted header bytes".ljust(32, b"!")
    data[payload_end - 32 : payload_end] = b"Uninterpreted trailing bytes".ljust(
        32, b"!"
    )
    return bytes(data)


def wrap_record_contents(contents):
    """Independent CFB v3 assembler, one FAT sector and one regular stream."""
    assert len(contents) >= 4096
    sectors = (len(contents) + 511) // 512
    assert sectors + 2 <= 128
    header = bytearray(512)
    header[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    struct.pack_into("<HHHHH", header, 24, 0x3E, 3, 0xFFFE, 9, 6)
    struct.pack_into("<IIIIIIIII", header, 40, 0, 1, 0, 0, 4096, END, 0, END, 0)
    struct.pack_into("<109I", header, 76, 1, *([FREE] * 108))
    root = directory_entry("Root Entry", 5, child=1, sector=END)
    root[80:96] = uuid.UUID("0d4c9b60-7843-11cf-1234-56789abcdeff").bytes_le
    directory = (
        root + directory_entry("Contents", 2, sector=2, size=len(contents)) + bytes(256)
    )
    entries = [END, FAT, *range(3, sectors + 2), END]
    fat = struct.pack("<128I", *entries, *([FREE] * (128 - len(entries))))
    return bytes(header) + directory + fat + contents.ljust(sectors * 512, b"\0")


def _block_line(start=(1.25, -2.5), end=(101.75, 48.125), tag=2):
    data = bytearray(64)
    struct.pack_into("<H6xH", data, 0, tag, 1)
    struct.pack_into("<6d", data, 16, *start, *end, 0, 0)
    return data


def make_block_contents(marker=5, *, compound=False):
    data = bytearray(make_record_contents(marker))
    start, end = struct.unpack_from("<2I", data, 36)
    for index in range(5):
        data[start + index * 64 : start + (index + 1) * 64] = bytes(64)
    for index, name, fields in (
        (0, b"TWO", (-25, 75, 0, 1, 1, 0)),
        (2, b"BASE", (100, -50, 0, 2, 2, 90)),
    ):
        offset = start + index * 64
        struct.pack_into("<H6xH", data, offset, 6, 2)
        data[offset + 32 : offset + 64] = bytes([0xA5]) * 32
        data[offset + 32 : offset + 32 + len(name)] = name
        data[offset + 32 + len(name)] = 0
        struct.pack_into("<H", data, offset + 64, 254)
        struct.pack_into("<6d", data, offset + 80, *fields)
    struct.pack_into("<H", data, end - 64, 255)
    section_end = struct.unpack_from("<I", data, 64)[0]
    data = data[:section_end] + bytearray(296)
    definitions_start = len(data)
    definitions = (
        (b"BASE", [_block_line()]),
        (b"TWO", [_block_line(), _block_line((-12.5, 0.25), (42, 0.25), 0x0402)]),
    )
    for name, records in definitions:
        header = bytearray(176)
        header[:32] = bytes([0xEE]) * 32
        header[: len(name)] = name
        header[len(name)] = 0
        struct.pack_into("<2I", header, 32, 0x073DBF98, len(records))
        data.extend(header)
        if compound and name == b"TWO":
            struct.pack_into("<H6xH", records[0], 0, 0x30, 2)
        for record in records:
            data.extend(record)
        data.extend(struct.pack("<H", 255) + bytes(62))
    data.extend(bytes([0xAB]) * (7272 if marker == 25 else 1768))
    data.extend(b"tail")
    struct.pack_into("<I", data, 76, definitions_start)
    struct.pack_into("<I", data, 80, len(data) - 4)
    return data


def make_object_contents(marker=5):
    """Independent primary/child spans and settings; not a native drawing."""
    start, count = 8192, 8
    boundaries = [start + count * 64]
    for delta in (16320, 16320, 2048, 1024, 1656 if marker == 5 else 3312, 4096):
        boundaries.append(boundaries[-1] + delta)
    payload_end = boundaries[-1] + 296 + (7272 if marker == 25 else 1768)
    data = bytearray(payload_end + 4)
    data[:18] = b"DRACAD for Windows"
    struct.pack_into("<H", data, 0x22, marker)
    struct.pack_into("<I7I", data, 0x24, start, *boundaries)
    struct.pack_into("<I", data, 0x50, payload_end)
    struct.pack_into("<I", data, 0x58, count - 1)
    data[0x150:0x178] = b"Opaque drawing-settings gap".ljust(40, b"!")
    struct.pack_into("<2d", data, 0x140, -12.5, 250.25)
    struct.pack_into("<d", data, 0x178, 0.04)
    for index, tag, span in (
        (0, 0x30, 2),
        (1, 2, 1),
        (2, 2, 1),
        (3, 0x33, 2),
        (4, 0xFE, 0),
        (5, 0x7777, 2),
        (6, 0xFE, 0),
        (7, 0xFF, 0),
    ):
        struct.pack_into("<H6xH", data, start + index * 64, tag, span)
    struct.pack_into("<I", data, start + 16, 0x400)
    struct.pack_into("<6d", data, start + 64 + 16, 1.25, -2.5, 101.75, 48.125, 0, 0)
    struct.pack_into("<6d", data, start + 128 + 16, -12.5, 0.25, 42, 0.25, 0, 0)
    data[-4:] = b"tail"
    return bytes(data)


def make_semantics_contents(marker=25):
    """Independent style, attribute, unit and package API fixture; not native."""
    data = bytearray(make_object_contents(marker))
    sections = struct.unpack_from("<7I", data, 0x28)
    payload_end = struct.unpack_from("<I", data, 0x50)[0]
    suffix = payload_end - (7272 if marker == 25 else 1768)

    def layer_row(index):
        row = bytearray(64)
        struct.pack_into("<6H", row, 0, 0x123, 0x456, 3, 4, 5, index)
        return row

    def color_row(index):
        row = bytearray(64)
        struct.pack_into("<2H", row, 0, 0x123, index * 3)
        row[4:7] = bytes(((index * 37) % 256, (index * 19) % 256, (index * 11) % 256))
        row[8:11] = bytes(((index * 7) % 256, (index * 3) % 256, index % 256))
        return row

    for index in range(1, 256):
        pos = sections[0] + (index - 1) * 64
        data[pos : pos + 64] = layer_row(index)
        pos = sections[1] + (index - 1) * 64
        data[pos : pos + 64] = color_row(index)
    for index in range(1, 33):
        pos = sections[2] + (index - 1) * 64
        struct.pack_into("<32H", data, pos, 0x123, index, 125, 37, *([0] * 28))
    for index in range(1, 17):
        pos = sections[3] + (index - 1) * 64
        struct.pack_into("<2H", data, pos, 0x123, index * 7)
    for relative, kind, index, size in (
        (0, 2, 256, 64),
        (80, 1, 0, 296),
        (392, 1, 256, 296),
    ):
        struct.pack_into("<4I", data, suffix + relative, kind, index, 0, size)
    data[suffix + 16 : suffix + 80] = color_row(256)
    for relative, index in ((96, 0), (408, 256)):
        data[suffix + relative : suffix + relative + 64] = layer_row(index)
        data[suffix + relative + 64 : suffix + relative + 296] = bytes([0xA5]) * 232
    if marker == 25:
        data[suffix + 1768 : payload_end] = bytes([0x5A]) * 5504
    struct.pack_into("<H", data, 0x9AC, 1)
    for index, tag, layer, color, ltype, width, material, group in (
        (0, 0x30, 1, 2, 1, 1, 0, 1),
        (1, 0x2402, 2, 0, 0, 0, 3, 257),
        (2, 0x4402, 1, 0, 255, 255, 0, 1),
    ):
        pos = 8192 + index * 64
        span = struct.unpack_from("<H", data, pos + 8)[0]
        struct.pack_into(
            "<HHBBBBHH",
            data,
            pos,
            tag,
            layer,
            color,
            ltype,
            width,
            material,
            span,
            group,
        )
    return bytes(data)


def make_measurement_contents():
    """Independent contextual length/root-style API fixture, never native proof."""
    data = bytearray(make_semantics_contents(25))
    struct.pack_into("<d", data, 0x278, 0.5)
    # One primary root ByLayer line alongside the existing opaque/inline cases.
    struct.pack_into("<HH3B", data, 8192 + 128, 0x2402, 1, 0, 0, 0)
    return bytes(data)


def make_inheritance_contents():
    """Independent one-level block property chains and explicit overrides."""
    data = bytearray(make_block_contents(25))
    styles = make_semantics_contents(25)
    for index in range(6):
        a, b = (
            struct.unpack_from("<I", data, at + 4 * index)[0] for at in (0x28, 0x2C)
        )
        c, d = (
            struct.unpack_from("<I", styles, at + 4 * index)[0] for at in (0x28, 0x2C)
        )
        assert b - a == d - c
        data[a:b] = styles[c:d]
    end, style_end = (
        struct.unpack_from("<I", item, 0x50)[0] for item in (data, styles)
    )
    data[end - 7272 : end] = styles[style_end - 7272 : style_end]
    struct.pack_into("<d", data, 0x178, 0.04)
    struct.pack_into("<d", data, 0x278, 0.5)
    for index, layer, color, ltype, width in ((0, 5, 12, 13, 14), (2, 6, 13, 14, 15)):
        struct.pack_into(
            "<HH3B", data, 8192 + index * 64, 6, layer, color, ltype, width
        )
        data[8192 + index * 64 + 32 : 8192 + index * 64 + 64] = b"TWO\0".ljust(
            32, b"\0"
        )
    # BASE definition: 176-byte header, one 64-byte line, terminal; then TWO.
    first = struct.unpack_from("<I", data, 0x4C)[0] + 176
    second = first + 64 + 64 + 176
    struct.pack_into("<HH3B", data, first, 2, 1, 1, 1, 1)
    struct.pack_into("<HH3B", data, second, 0x4402, 0, 0, 255, 255)
    struct.pack_into("<HH3B", data, second + 64, 0x402, 7, 7, 8, 9)
    return bytes(data)


if __name__ == "__main__":
    output = ROOT / "tests/fixtures/minimal.cfb"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(make_fixture())
    print(output)
    envelope, contents = make_mpz_fixture()
    (output.parent / "mpz-envelope.cfb").write_bytes(envelope)
    (output.parent / "mpz-contents.bin").write_bytes(contents)
    for marker in (5, 8, 25):
        contents = make_record_contents(marker)
        (output.parent / f"records{marker}.bin").write_bytes(contents)
        (output.parent / f"records{marker}.cfb").write_bytes(
            wrap_record_contents(contents)
        )

        contents = make_block_contents(marker)
        (output.parent / f"blocks{marker}.bin").write_bytes(contents)
        (output.parent / f"blocks{marker}.cfb").write_bytes(
            wrap_record_contents(contents)
        )
        contents = make_object_contents(marker)
        (output.parent / f"objects{marker}.bin").write_bytes(contents)
        (output.parent / f"objects{marker}.cfb").write_bytes(
            wrap_record_contents(contents)
        )

        contents = make_semantics_contents(marker)
        (output.parent / f"semantics{marker}.bin").write_bytes(contents)
        (output.parent / f"semantics{marker}.cfb").write_bytes(
            wrap_record_contents(contents)
        )
    for name, contents in (
        ("measurement25", make_measurement_contents()),
        ("inheritance25", make_inheritance_contents()),
    ):
        (output.parent / f"{name}.bin").write_bytes(contents)
        (output.parent / f"{name}.cfb").write_bytes(wrap_record_contents(contents))
