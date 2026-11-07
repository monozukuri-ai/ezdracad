"""An explicit dimension's saved number/span ratio, not original D/A history."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Literal


class DimensionScaleProfile(str, Enum):
    MARKER25_ALIGNED_NUMERIC_MM_V1 = "marker25_aligned_numeric_mm_v1"


def _profile_value(value: DimensionScaleProfile | str) -> str:
    if isinstance(value, DimensionScaleProfile):
        return value.value
    if isinstance(value, str):
        return value
    raise TypeError("profile must be DimensionScaleProfile or str")


@dataclass(frozen=True)
class DimensionScale:
    """Rust-decoded root-table dimension with owned exact source evidence.

    annotation_to_drawing_ratio is its recorded mm number / stored span length.
    equivalent_annotation_denominator uses current D and is not original A.
    No world placement, neighboring geometry or drawing completeness is inferred.
    Constructing this dataclass alone is not native-authorship certification.
    """

    profile: DimensionScaleProfile
    record_index: int
    payload_offset: int
    tag_raw: int
    contents_sha256: str
    record_bytes: bytes
    dimid: int
    dimflag: int
    dimflag2: int
    geometry_offset: int
    geometry_bytes: bytes
    start: tuple[float, float]
    end: tuple[float, float]
    stored_span_length: float
    numeric_flag_offset: int
    numeric_flag_bytes: bytes
    unit_offset: int
    unit_bytes: bytes
    number_offset: int
    number_bytes: bytes
    recorded_value_mm: float
    annotation_to_drawing_ratio: float
    current_drawing_scale_offset: int
    current_drawing_scale_bytes: bytes
    current_drawing_denominator: float
    equivalent_annotation_denominator: float
    coordinate_space: Literal["record_local"] = field(
        default="record_local", init=False
    )
    ratio_provenance: Literal["derived_saved_dimension_number_and_geometry"] = field(
        default="derived_saved_dimension_number_and_geometry", init=False
    )
    numeric_unit: Literal["mm"] = field(default="mm", init=False)
    geometry_units: None = field(default=None, init=False)
    original_drawing_denominator: None = field(default=None, init=False)
    original_annotation_denominator: None = field(default=None, init=False)
    original_scale_resolution: Literal[
        "ratio_only_original_denominators_unavailable"
    ] = field(default="ratio_only_original_denominators_unavailable", init=False)
    document_complete: bool = field(default=False, init=False)


def _dimension_scale(row: dict) -> DimensionScale:
    value = dict(row)
    value["profile"] = DimensionScaleProfile(value["profile"])
    value["start"] = tuple(value["start"])
    value["end"] = tuple(value["end"])
    return DimensionScale(**value)
