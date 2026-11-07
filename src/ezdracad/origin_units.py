"""Raw Origin.Units metadata; the physical-unit enum remains unresolved."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class OriginUnitsCode:
    """Source-preserving marker-25 code associated with MPXL Origin.Units.

    Unknown and negative signed codes retain their exact source bytes. This
    metadata does not supply a physical-unit name, convert coordinates, or
    identify the scale at which a figure was originally authored. Rust owns
    the source snapshot and performs profile and field-boundary checks.
    """

    payload_offset: int
    selector_bytes: bytes
    raw_code: int
