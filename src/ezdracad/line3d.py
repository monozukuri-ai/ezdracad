"""Observed 3D record fields, without WCS, workplane or parent interpretation."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal


@dataclass(frozen=True)
class LineRecord3D:
    """An exact tag-0x0013 source with independent endpoint XYZ coordinates.

    These are record-local stored values. Raw-table membership does not prove
    logical root membership. No origin, scale, workplane or placement transform
    is applied; physical units, parent context and drawing completeness remain
    unresolved. Header bytes retain all uninterpreted flags and attributes.
    """

    record_index: int
    payload_offset: int
    tag_raw: int
    header_bytes: bytes
    start: tuple[float, float, float]
    end: tuple[float, float, float]
    coordinate_space: Literal["record_local"] = field(
        default="record_local", init=False
    )
    units: None = field(default=None, init=False)
    parent_context: None = field(default=None, init=False)
