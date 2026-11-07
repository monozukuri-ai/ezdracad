"""Opt-in root CIRCLE, ARC and literal TEXT with owned native evidence."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field, fields
from enum import Enum
from typing import TYPE_CHECKING, Literal

from . import _core
from .effective_styles import EffectiveStyleIndices, _indices
from .qualification import LINE2D_UNRESOLVED_SEMANTICS

if TYPE_CHECKING:
    from .document import SourceRef
    from .raw import RecordAttributes

_TEXT_DEFAULTS = _core.text_limit_defaults()


class Basic2DProfile(str, Enum):
    MARKER25_ROOT_PRIMITIVES_V1 = "marker25_root_primitives_v1"


@dataclass(frozen=True)
class TextLimits:
    """Saved body bytes and decoded UTF-8 bytes, checked before copying."""

    max_encoded_bytes: int = _TEXT_DEFAULTS[0]
    max_output_bytes: int = _TEXT_DEFAULTS[1]

    def __post_init__(self) -> None:
        for item in fields(self):
            value = getattr(self, item.name)
            if type(value) is not int:
                raise TypeError(f"{item.name} must be an integer")
            if not 0 <= value <= (1 << 64) - 1:
                raise ValueError(f"{item.name} must fit an unsigned 64-bit integer")

    def _values(self) -> tuple[int, int]:
        return self.max_encoded_bytes, self.max_output_bytes


@dataclass(frozen=True)
class _RootPrimitiveEntity:
    entity_id: int
    root_record_index: int
    source_ref: SourceRef
    qualification_profile: Basic2DProfile
    raw_bytes: bytes
    attributes: RecordAttributes
    style_layer_index: int
    styles: EffectiveStyleIndices
    coordinate_scope: Literal["root_records"] = field(
        default="root_records", init=False
    )
    coordinate_semantics: Literal["stored_xy"] = field(default="stored_xy", init=False)
    units: None = field(default=None, init=False)
    unresolved_semantics: tuple[str, ...] = field(
        default=LINE2D_UNRESOLVED_SEMANTICS, init=False
    )

    def _base_dict(self) -> dict:
        return {
            "type": self.type,
            "entity_id": self.entity_id,
            "root_record_index": self.root_record_index,
            "source_ref": asdict(self.source_ref),
            "qualification_profile": self.qualification_profile.value,
            "coordinate_scope": self.coordinate_scope,
            "coordinate_semantics": self.coordinate_semantics,
            "units": None,
            "unresolved_semantics": list(self.unresolved_semantics),
            "attributes": {
                "layer_index_raw": self.attributes.layer_index,
                "style_layer_index": self.style_layer_index,
                "color_index": self.styles.color.index,
                "line_type_index": self.styles.line_type.index,
                "width_index": self.styles.width.index,
                "material_index_raw": self.attributes.material_index_raw,
                "group_index_raw": self.attributes.group_index,
            },
        }


@dataclass(frozen=True)
class CircleEntity(_RootPrimitiveEntity):
    """Stored planar center and radius; no WCS or physical unit conversion."""

    center: tuple[float, float]
    radius: float
    type: Literal["CIRCLE"] = field(default="CIRCLE", init=False)

    def to_dict(self) -> dict:
        return {**self._base_dict(), "center": list(self.center), "radius": self.radius}


@dataclass(frozen=True)
class ArcEntity(_RootPrimitiveEntity):
    """Stored planar circular arc; angles retain native degrees."""

    center: tuple[float, float]
    radius: float
    start_angle_degrees: float
    end_angle_degrees: float
    type: Literal["ARC"] = field(default="ARC", init=False)

    def to_dict(self) -> dict:
        return {
            **self._base_dict(),
            "center": list(self.center),
            "radius": self.radius,
            "start_angle_degrees": self.start_angle_degrees,
            "end_angle_degrees": self.end_angle_degrees,
        }


@dataclass(frozen=True)
class TextEntity(_RootPrimitiveEntity):
    """Literal native TEXT, preserving its serialized body and font bytes.

    Decoding a string does not certify glyph layout, font metrics or text size
    units. Styling escapes, dynamic text codes and multiline layout remain held.
    """

    basepoint: tuple[float, float]
    width: float
    height: float
    spacing: float
    rotation_degrees: float
    origin: int
    font_name: str
    font_bytes: bytes
    serialized_text: str
    body_bytes: bytes
    text: str
    text_flags: tuple[int, int]
    charset: int
    family: int
    scale_flag: int
    body_encoding: Literal["ascii_native_bmp_unicode_escapes"]
    type: Literal["TEXT"] = field(default="TEXT", init=False)

    def to_dict(self) -> dict:
        return {
            **self._base_dict(),
            "basepoint": list(self.basepoint),
            **{
                name: getattr(self, name)
                for name in (
                    "width",
                    "height",
                    "spacing",
                    "rotation_degrees",
                    "origin",
                    "font_name",
                    "serialized_text",
                    "text",
                    "charset",
                    "family",
                    "scale_flag",
                    "body_encoding",
                )
            },
            "text_flags": list(self.text_flags),
            "encoded_byte_length": len(self.body_bytes),
        }


def _primitive_entity(
    row: dict, source_ref: SourceRef
) -> CircleEntity | ArcEntity | TextEntity:
    # Lazy import avoids a cycle through raw's public dataclass exports.
    from .raw import _record_attributes

    if row["units"] is not None or row["coordinate_scope"] != "root_records":
        raise ValueError("root primitive scope differs across language boundary")
    common = dict(
        entity_id=row["entity_id"],
        root_record_index=row["root_record_index"],
        source_ref=source_ref,
        qualification_profile=Basic2DProfile(row["qualification_profile"]),
        raw_bytes=row["raw_bytes"],
        attributes=_record_attributes(row["attributes"]),
        style_layer_index=row["style_layer_index"],
        styles=_indices(row["styles"]),
    )
    if row["type"] in ("CIRCLE", "ARC"):
        common.update(center=tuple(row["center"]), radius=row["radius"])
        if row["type"] == "CIRCLE":
            return CircleEntity(**common)
        return ArcEntity(
            **common,
            start_angle_degrees=row["start_angle_degrees"],
            end_angle_degrees=row["end_angle_degrees"],
        )
    if row["type"] == "TEXT":
        if row["body_encoding"] != "ascii_native_bmp_unicode_escapes":
            raise ValueError("TEXT body encoding differs across language boundary")
        values = {
            item.name: row[item.name]
            for item in fields(TextEntity)
            if item.init and item.name not in common
        }
        values["basepoint"] = tuple(values["basepoint"])
        values["text_flags"] = tuple(values["text_flags"])
        return TextEntity(**common, **values)
    raise ValueError("unknown primitive type across language boundary")
