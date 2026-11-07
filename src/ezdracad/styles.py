"""Observed raw style tables, without effective inheritance or unit conversion."""

from __future__ import annotations

from dataclasses import dataclass, field

from . import _core


def _index(value: int) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise TypeError("style index must be an integer")
    if not 0 <= value < 1 << 16:
        raise ValueError("style index must fit an unsigned 16-bit integer")
    return value


@dataclass(frozen=True)
class RawLayerStyle:
    """One file-local layer row; special rows retain their entire 296 bytes."""

    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    reserved_word_raw: int
    color_index_raw: int
    line_type_index_raw: int
    width_index_raw: int
    stored_index_raw: int


@dataclass(frozen=True)
class RawColorStyle:
    """Palette RGB and output RGB bytes, without ACI or appearance conversion."""

    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    pen_width_hundredths_raw: int
    pen_width_value: float
    rgb: tuple[int, int, int]
    rgb_encoding_raw: int
    output_rgb: tuple[int, int, int]
    output_rgb_encoding_raw: int


@dataclass(frozen=True)
class RawLineTypeStyle:
    """Thirty stored pattern slots. Zero-tail removal and length inference are absent."""

    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    pen_style_raw: int
    pen_definition_hundredths_raw: tuple[int, ...]
    pen_definition_values: tuple[float, ...]


@dataclass(frozen=True)
class RawWidthStyle:
    """Stored table value, not effective or color-based printed line width."""

    index: int
    payload_offset: int
    raw_bytes: bytes
    flags_raw: int
    pen_width_hundredths_raw: int
    pen_width_value: float


@dataclass(frozen=True, eq=False)
class StyleTables:
    """Owned immutable Rust tables. Later lookups do not reopen the source.

    Values and indices remain local to the file. ByLayer/ByBlock inheritance,
    printing policy, physical geometry units and effective styles are unresolved.
    Unknown encoding/row bytes remain observable. Parsing happens in Rust only.
    """

    _handle: _core._StyleHandle = field(repr=False, compare=False)

    @property
    def layer_count(self) -> int:
        return self._handle.metadata()["layer_count"]

    @property
    def color_count(self) -> int:
        return self._handle.metadata()["color_count"]

    @property
    def line_type_count(self) -> int:
        return self._handle.metadata()["line_type_count"]

    @property
    def width_count(self) -> int:
        return self._handle.metadata()["width_count"]

    @property
    def shared_suffix_offset(self) -> int:
        return self._handle.metadata()["shared_suffix_offset"]

    @property
    def shared_suffix_size(self) -> int:
        return self._handle.metadata()["shared_suffix_size"]

    @property
    def extension_header_offsets(self) -> tuple[int, ...]:
        return tuple(self._handle.metadata()["extension_header_offsets"])

    @property
    def extension_header_bytes(self) -> tuple[bytes, ...]:
        return tuple(self._handle.metadata()["extension_header_bytes"])

    def suffix_bytes(self) -> bytes:
        return self._handle.suffix_bytes()

    def layer(self, index: int) -> RawLayerStyle:
        """Look up index 0..256; other representable indices raise IndexError."""
        return RawLayerStyle(**self._handle.layer(_index(index)))

    def color(self, index: int) -> RawColorStyle:
        """Look up palette index 1..256; no inherited color is resolved."""
        row = self._handle.color(_index(index))
        row["rgb"] = tuple(row["rgb"])
        row["output_rgb"] = tuple(row["output_rgb"])
        return RawColorStyle(**row)

    def line_type(self, index: int) -> RawLineTypeStyle:
        row = self._handle.line_type(_index(index))
        row["pen_definition_hundredths_raw"] = tuple(
            row["pen_definition_hundredths_raw"]
        )
        row["pen_definition_values"] = tuple(row["pen_definition_values"])
        return RawLineTypeStyle(**row)

    def width(self, index: int) -> RawWidthStyle:
        return RawWidthStyle(**self._handle.width(_index(index)))
