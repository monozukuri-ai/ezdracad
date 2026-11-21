# Best-effort reading

`ezdracad.best_effort` reads every object whose saved layout is known and counts
the rest. Use it to display or convert a drawing. The qualified reader in
[documents and queries](document-reader.md) makes the opposite trade: it stops at
the first object it cannot establish.

```python
from ezdracad import best_effort

drawing = best_effort.readfile("drawing.mpz")
for entity in drawing.walk():
    if entity.type == "LINE":
        print(entity.layer, entity.data["start"], entity.data["end"])
    elif entity.type == "TEXT":
        print(entity.data["text"], entity.data["position"])

report = drawing.report
print(report.decoded_counts, report.unsupported_counts, report.failed_counts)
```

`read(bytes)` accepts in-memory bytes. Both functions identify the input by
content:

| Input | `drawing.format` | Records |
| --- | --- | --- |
| MPZ | `mpz` | 64 bytes, Shift_JIS strings |
| Flat MPX | `mpx` | 64 bytes, Shift_JIS strings |
| MPS without a password | `mps` | 64 bytes, Shift_JIS strings |
| MPUW without a password | `mpuw` | 80 bytes, UTF-16 strings |

The two record layouts decode to the same entities, tables and settings. Every
record generation that the application can save, raw markers 3 to 25, is read
with the same layouts. A marker outside that range is read the same way and
reported with `marker_observed=False`. Password-protected MPS and MPUW files are
not read.

## What a result means

A result describes what the file stores. It is not a statement that the drawing
is complete or that it matches the printed sheet.

- Coordinates are the stored values. `drawing.settings` holds the saved origin,
  paper scale, paper size and paper corner; none of them is applied.
- `color`, `line_type` and `width` are indices into `drawing.colors`,
  `drawing.line_types` and `drawing.widths`. The `BY_LAYER_*` and `BY_BLOCK_*`
  constants mark values that defer to the layer or to the enclosing block.
- A layer with `hidden=True` and an entity with `hidden=True` are still returned.
  Hiding them is left to the consumer.
- Text keeps its saved character cell height and width. Font metrics and line
  layout are left to the consumer.
- The oldest generations cannot store every object. The application converts a
  dimension to lines and text when it saves such a file, and the reader returns
  what was saved.

Malformed containers, inconsistent record tables and exceeded limits raise. One
object that does not fit its layout is skipped and counted; the objects after it
are still read.

## Entities

Every `Entity` has `type`, `layer`, `color`, `line_type`, `width`, `flags`,
`group`, `link_id`, a source location (`table`, `block_index`, `record_index`,
`payload_offset`, `record_count`), a read-only `data` mapping and `children`.
Angles are degrees, counterclockwise.

| `type` | `data` keys | `children` |
| --- | --- | --- |
| `POINT` | `position`, `point_type` | |
| `LINE` | `start`, `end`, `elevation`, `thickness` | |
| `LINE3D` | `start`, `end` with three coordinates | |
| `CIRCLE` | `center`, `radius`, `elevation`, `fill` | |
| `ARC` | `center`, `radius`, `start_degrees`, `end_degrees`, `elevation` | |
| `ELLIPSE` | `center`, `radius` (two values), `rotation_degrees`, `fill` | |
| `ELLIPTICAL_ARC` | as `ELLIPSE` without `fill`, with `start_degrees`, `end_degrees` | |
| `TEXT` | `position`, `elevation`, `text`, `text_bytes`, `height`, `width`, `spacing`, `rotation_degrees`, `slant_degrees`, `origin`, `flags`, `font_name`, `charset`, `family`, `numeric`, `number`, `unit`, `item`, `format_codes`, `lossy` | |
| `POLYLINE` | `curve_kind`, `flags`, `closed`, `divisions`, `three_d`, `fill`, `curve_points` | Member lines and arcs |
| `EXTENDED_POLYGON` | `polygon_kind`, `flags`, `fill`, `reference_number`, `hole_count`, `region_id` | Boundary polylines |
| `HATCH` | `hatch_type`, `region_id`, `lines`, `pattern` | |
| `DIMENSION` | `dimension_type`, `flags`, `flags2`, `arrow_styles`, `text_position`, `line`, `arc`, `extension_origins`, `arrow_sizes`, `arrow_paper_sizes` | The numeric text |
| `LEADER` | `leader_type`, `flags`, `flags2`, `arrow_style`, `lines`, `arrow_size`, `arrow_paper_size` | The note text |
| `PACKAGE` | `package_kind`, `flags`, `reference_number` | Member entities |
| `ARROWHEAD` | `position`, `size`, `angle_degrees`, `arrow_type`, `flags` | |
| `INSERT` | `name`, `name_bytes`, `reference_number`, `position`, `elevation`, `scale`, `rotation_degrees` | |
| `IMAGE` | `file_name`, `directory`, `position`, `elevation`, `scale`, `size`, `rotation_degrees` | |
| `EXTERNAL_REFERENCE` | `file_name`, `directory`, `relative_directory`, `position`, `elevation`, `scale`, `rotation_degrees` | |

### Fills

`fill` is `None` for a shape that is not filled. Otherwise it is a mapping:

| Key | Meaning |
| --- | --- |
| `color` | 1 to 256: an index into `drawing.colors`. `RGB_FILL_COLOR` (257): use `rgb`. `BACKGROUND_FILL_COLOR` (258): the background color of the view |
| `rgb` | Three numbers. The fill color with `RGB_FILL_COLOR`; the background color at the time of saving with `BACKGROUND_FILL_COLOR` |
| `transparency` | The saved percentage; 0 is opaque |
| `outline` | Whether the outline is drawn in addition to the fill |

Circles, ellipses, polylines and extended polygons carry a fill. A polyline is
filled as the area enclosed by its members.

### Text

`text` is decoded from `text_bytes`. Files with 64-byte records store Shift_JIS
bytes, with characters outside that set as escapes; MPUW stores UTF-16. Saved
escapes are expanded and formatting codes are removed (`format_codes=True`
then). A line feed separates lines. `origin` holds the horizontal position in
its low two bits (0 left, 1 center, 2 right) and the vertical position in the
next two (0 bottom, 1 middle, 2 top). A `numeric` text stores a measured `number`
next to the string it displays; draw the string.

### Polylines and curves

Members of a polyline are its children, in stored order. `curve_kind` selects
how they are read:

| `curve_kind` | Children | `curve_points` |
| --- | --- | --- |
| 0 | The polyline itself | `None` |
| 1 | A saved evaluation of the curve that precedes it | `None` |
| 2 | Control polygon of a quadratic spline | Evaluated curve |
| 3 | Control polygon of a cubic spline, or its through points when `flags` has `0x10` | Evaluated curve |
| 4 | Alternating anchors and handles of a Bezier curve | Evaluated curve |

An evaluated curve has `divisions` pieces per span. MPUW files store a kind 1
polyline after each curve; the older layout does not. Draw `curve_points` and
skip kind 1 polylines, or the curve is drawn twice. `flags` bit `0x01` closes the
polyline or curve.

An arc always stores its angles counterclockwise. A member arc with
`reversed=True` is followed by the polyline from its end angle to its start
angle. Use it to chain the members into one path, for example to fill a
polyline. An arc outside a polyline is never reversed.

### Regions and hatches

An `EXTENDED_POLYGON` is a region. Its children are closed polylines: the first
is the outer boundary and `hole_count` more cut holes. It can be filled, hatched
or both. `flags` bit `0x02` hides the boundary lines.

A hatch names its region by `region_id`: the `EXTENDED_POLYGON` with the same
value. `reference_number` numbers the hatched regions of a drawing and is zero
for a region without a hatch.

- `hatch_type` 0: `lines` lists families of parallel lines, each with `origin`,
  `pitch`, `angle_degrees` and its own `color`, `line_type` and `width`.
  Clipping them to the region is left to the consumer.
- `hatch_type` 1: `pattern` holds the `name` of a pattern of the application's
  pattern library, its `origin`, the saved numeric `values` and the attribute
  references. The lines of the pattern are not stored in the drawing, so they
  cannot be returned.
- `hatch_type` 2 is a gradation; only the type is returned.

### Dimensions and leaders

A `DIMENSION` holds the measured `line`, the extension origins, the arrow sizes
and the numeric text. `dimension_type` is 1 aligned, 2 linear, 3 angle, 4 radius
or 5 diameter. An angle dimension also has `arc`, with `center`, `radius`,
`start_degrees` and `end_degrees`. A `LEADER` holds its `lines`, each a pair of
points, and the note text.

The lines, arcs, arrowheads and text that draw a dimension or leader are the
members of a `PACKAGE` with `package_kind` 3 or 4. The object and those members
share a `link_id`. To draw a dimension or leader, draw the package and skip the
`DIMENSION` or `LEADER`.

### Blocks

`drawing.blocks` lists the named definitions with their own entities. An `INSERT`
refers to a definition by `name_bytes` and places it with `scale`, then
`rotation_degrees`, then `position`. `drawing.block(name)` returns the first
definition with a name.

MPUW files additionally store a `PACKAGE` with `package_kind` 1 for each placed
block. Its members are the entities of the definition, already placed, and its
`reference_number` equals that of the `INSERT`. Draw either the insert or that
package, not both.

### Images and external references

An `IMAGE` names a picture file by `file_name` and `directory`; `size` is the
drawn width and height. An `EXTERNAL_REFERENCE` names another drawing. The file
contents are not part of the drawing and are not read.

## Report

| Field | Meaning |
| --- | --- |
| `decoded_counts` | Decoded entities by type, at every nesting level |
| `unsupported_counts` | Objects of a kind that is not decoded, by object kind number |
| `failed_counts` | Objects of a known kind that did not fit its layout |
| `property_records` | Non-graphic property records, which hold no geometry |
| `orphan_records` | Continuation records that belonged to no object |
| `clamped_spans` | Objects whose declared length ran past their parent |
| `lossy` | Whether anything was skipped or cut: one of `unsupported_counts`, `failed_counts`, `orphan_records` and `clamped_spans` is nonzero |
| `diagnostics` | Notes with a code, a message and a record location |

Object kinds that are counted and not decoded include layouts, viewports, and
the three-dimensional solid, symbol, camera and light objects.

## Limits

`BestEffortLimits` bounds decoded entities, nesting depth, generated curve
points, block definitions and retained diagnostics. `ParseLimits` bounds the
input as for the other readers. Exceeding a limit raises `LimitExceededError`;
only diagnostics are cut silently, and the cut is counted.

```python
from ezdracad import ParseLimits, best_effort

drawing = best_effort.readfile(
    "drawing.mps",
    limits=ParseLimits(max_decompressed_size=64 * 1024 * 1024),
    read_limits=best_effort.BestEffortLimits(max_entities=500_000),
)
```
