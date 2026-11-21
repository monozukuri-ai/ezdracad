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

`read(bytes)` accepts in-memory bytes. Both functions identify MPZ, flat MPX and
unprotected MPS input by content, and accept the record generations 5, 8, 10, 22
and 25 that the layouts were checked on. Another generation is read with the same
layouts and reported with `marker_observed=False`.

## What a result means

A result describes what the file stores. It is not a statement that the drawing
is complete or that it matches the printed sheet.

- Coordinates are the stored values. `drawing.settings` holds the saved origin,
  paper scale, paper size and paper corner; none of them is applied.
- `color`, `line_type` and `width` are indices into `drawing.colors`,
  `drawing.line_types` and `drawing.widths`. The `BY_LAYER_*` and `BY_BLOCK_*`
  constants mark values that defer to the layer or to the enclosing block.
- A layer with `hidden=True` and an entity with `hidden=True` are still returned.
- Text keeps its saved character cell height and width. Font metrics and line
  layout are left to the consumer.

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
| `CIRCLE` | `center`, `radius`, `elevation`, `fill_state` | |
| `ARC` | `center`, `radius`, `start_degrees`, `end_degrees`, `elevation` | |
| `ELLIPSE` | `center`, `radius` (two values), `rotation_degrees` | |
| `ELLIPTICAL_ARC` | as `ELLIPSE`, with `start_degrees`, `end_degrees` | |
| `TEXT` | `position`, `text`, `text_bytes`, `height`, `width`, `spacing`, `rotation_degrees`, `slant_degrees`, `origin`, `flags`, `font_name`, `charset`, `family`, `numeric`, `number`, `unit`, `item`, `format_codes`, `lossy` | |
| `POLYLINE` | `curve_kind`, `flags`, `closed`, `filled`, `fill_color`, `divisions`, `three_d`, `curve_points` | Member lines and arcs |
| `EXTENDED_POLYGON` | `polygon_kind`, `flags`, `filled`, `fill_color`, `loop_count`, `region_id` | Boundary polylines |
| `HATCH` | `hatch_type`, `region_id`, `lines`, `pattern_origin` | |
| `DIMENSION` | `dimension_type`, `flags`, `flags2`, `arrow_styles`, `text_position`, `line`, `extension_origins`, `arrow_sizes`, `arrow_paper_sizes` | The numeric text |
| `PACKAGE` | `package_kind`, `flags`, `reference_number` | Member entities |
| `ARROWHEAD` | `position`, `size`, `angle_degrees`, `arrow_type`, `flags` | |
| `INSERT` | `name`, `name_bytes`, `position`, `elevation`, `scale`, `rotation_degrees` | |

### Text

`text` is decoded from `text_bytes`: Shift_JIS bytes, with saved Unicode escapes
expanded and formatting codes removed (`format_codes=True` then). A line feed
separates lines. `origin` holds the horizontal position in its low two bits
(0 left, 1 center, 2 right) and the vertical position in the next two (0 bottom,
1 middle, 2 top). A `numeric` text stores a measured `number` next to the string
it displays; draw the string.

### Polylines and splines

Members of a polyline are its children, in stored order. `curve_kind` is 0 for a
plain polyline, 2 for a quadratic spline, 3 for a cubic spline and 4 for a Bezier
curve. For a cubic spline the children are the control polygon and
`curve_points` is the evaluated curve, `divisions` pieces per span. Other curve
kinds return their control polygon only.

### Hatches

A hatch names its region by `region_id`: the `EXTENDED_POLYGON` with the same
value, whose child polylines bound the area. `lines` lists families of parallel
lines by `origin`, `pitch` and `angle_degrees`; clipping them to the region is
left to the consumer. A named pattern (`hatch_type` 1) keeps its
`pattern_origin` only.

### Dimensions

A `DIMENSION` holds the measured line, the extension origins, the arrow sizes and
the numeric text. The lines, arrowheads and text that draw it are the members of
a `PACKAGE` with `package_kind` 3; the dimension and those members share a
`link_id`. To draw a dimension, draw the package and skip the `DIMENSION`.

### Blocks

`drawing.blocks` lists the named definitions with their own entities. An `INSERT`
refers to a definition by `name_bytes` and places it with `scale`, then
`rotation_degrees`, then `position`. `drawing.block(name)` returns the first
definition with a name.

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

Object kinds that are not decoded include images, external references, leaders
without a package, layouts and viewports, and the three-dimensional solid, camera
and light objects. Fill colors of circles and ellipses are returned as raw bytes
in `fill_state`.

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
