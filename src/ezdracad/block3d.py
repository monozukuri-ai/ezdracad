"""Explicit atomic direct-line 3D block placement, with exact source records.

The source is an admitted primary root reference, not arbitrary nesting. Units,
effective attributes, WCS, visibility and document completeness stay unresolved.
All parsing, qualification and point arithmetic run in the Rust core.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Literal

from . import _core
from .line3d import LineRecord3D


class Block3DProfile(str, Enum):
    MARKER25_DIRECT_LINE3D_V1 = "marker25_direct_line3d_v1"


def str_value(profile: Block3DProfile | str) -> str:
    if isinstance(profile, Block3DProfile):
        return profile.value
    if not isinstance(profile, str):
        raise TypeError("profile must be a Block3DProfile or an explicit string")
    return profile


@dataclass(frozen=True)
class BlockReferenceEvidence3D:
    contents_sha256: str
    record_indices: tuple[int, int, int]
    payload_offsets: tuple[int, int, int]
    records: tuple[bytes, bytes, bytes]


@dataclass(frozen=True, eq=False)
class BlockReference3D:
    """A privately Rust-decoded reference belonging to a qualified selection."""

    _handle: _core._BlockReference3DHandle = field(repr=False, compare=False)

    @property
    def profile(self) -> Block3DProfile:
        return Block3DProfile(self._handle.metadata()["profile"])

    @property
    def tag_raw(self) -> int:
        return self._handle.metadata()["tag_raw"]

    @property
    def name_key(self) -> bytes:
        return self._handle.metadata()["name_key"]

    @property
    def name_field_bytes(self) -> bytes:
        return self._handle.metadata()["name_field_bytes"]

    @property
    def translation(self) -> tuple[float, float, float]:
        return tuple(self._handle.metadata()["translation"])

    @property
    def scale(self) -> tuple[float, float, float]:
        return tuple(self._handle.metadata()["scale"])

    @property
    def rotation_degrees(self) -> tuple[float, float, float]:
        return tuple(self._handle.metadata()["rotation_degrees"])

    @property
    def evidence(self) -> BlockReferenceEvidence3D:
        row = self._handle.metadata()
        return BlockReferenceEvidence3D(
            row["contents_sha256"],
            tuple(row["record_indices"]),
            tuple(row["payload_offsets"]),
            tuple(row["records"]),
        )

    @property
    def units(self) -> None:
        return None

    @property
    def record_scope(self) -> Literal["root_records"]:
        return self._handle.metadata()["record_scope"]

    def apply_xyz(
        self,
        point: tuple[float, float, float] | list[float],
    ) -> tuple[float, float, float]:
        """Explicit mathematical point application; no source is reopened."""
        if not isinstance(point, (tuple, list)) or len(point) != 3:
            raise TypeError("point must be a three-number tuple or list")
        if any(not isinstance(x, (int, float)) or isinstance(x, bool) for x in point):
            raise TypeError("point coordinates must be int or float, not bool")
        return tuple(self._handle.apply_xyz(tuple(float(x) for x in point)))


@dataclass(frozen=True)
class BlockDefinitionSource3D:
    index: int
    header_offset: int
    header_bytes: bytes
    name_bytes: bytes
    record_start: int
    record_end: int
    declared_nonterminal_records: int
    record_count: int
    flag_raw: int
    saved_bounds: tuple[float, float, float, float, float, float]


@dataclass(frozen=True)
class PlacedBlockLine3D:
    definition_index: int
    source: LineRecord3D
    start: tuple[float, float, float]
    end: tuple[float, float, float]
    coordinate_space: Literal["block_parent_stored_xyz"] = field(
        default="block_parent_stored_xyz", init=False
    )
    units: None = field(default=None, init=False)
    parent_context: None = field(default=None, init=False)


@dataclass(frozen=True)
class BlockLineSelection3D:
    profile: Block3DProfile
    contents_sha256: str
    reference: BlockReference3D
    definition: BlockDefinitionSource3D
    lines: tuple[PlacedBlockLine3D, ...]
    document_complete: Literal[False] = field(default=False, init=False)
    units: None = field(default=None, init=False)


def selection(row: dict) -> BlockLineSelection3D:
    if row["document_complete"] is not False or row["units"] is not None:
        raise RuntimeError("inconsistent 3D block selection semantics")
    lines = []
    for item in row["lines"]:
        if (
            item["coordinate_space"] != "block_parent_stored_xyz"
            or item["units"] is not None
        ):
            raise RuntimeError("inconsistent 3D block line semantics")
        index, offset, tag, header, start, end = item["source"]
        source = LineRecord3D(index, offset, tag, header, tuple(start), tuple(end))
        lines.append(
            PlacedBlockLine3D(
                item["definition_index"],
                source,
                tuple(item["start"]),
                tuple(item["end"]),
            )
        )
    definition = dict(row["definition"])
    definition["saved_bounds"] = tuple(definition["saved_bounds"])
    return BlockLineSelection3D(
        Block3DProfile(row["profile"]),
        row["contents_sha256"],
        BlockReference3D(row["reference"]),
        BlockDefinitionSource3D(**definition),
        tuple(lines),
    )
