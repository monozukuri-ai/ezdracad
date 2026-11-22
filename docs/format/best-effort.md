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
with the same layouts, and so is marker 1, which the oldest part libraries
hold. Any other marker is read the same way and reported with
`marker_observed=False`. Password-protected MPS and MPUW files are not read,
and neither is one that forbids printing.

Older releases saved compound files without a class identifier on the root
storage. This reader accepts them when the drawing stream identifies itself;
the [raw API](../raw-api.md) and the qualified reader ask for the identifier.
Bytes that hold nothing are not always zero in saved drawings: the padding of
the identifier, the header of a continuation record and the unused part of a
style-table row can carry leftovers, and are ignored.

## What a result means

A result describes what the file stores. It is not a statement that the drawing
is complete or that it matches the printed sheet.

- Coordinates are the stored values, in drawing units. `drawing.settings` holds
  the saved origin, the drawing scale, the paper size in millimetres and
  `paper_origin`, the upper left corner of the sheet in drawing units. The sheet
  covers `paper_size` times `drawing_scale` drawing units to the right of and
  below that corner. None of this is applied.
- `settings.paper_type` is the paper size code: 0 a free size, 1 to 7 A0 to A6,
  8 to 14 B0 to B6, 15 letter and 16 legal; `settings.paper_name` is the name
  the application prints for it and `settings.paper_portrait` tells an upright
  sheet. `settings.comment` is the comment of the drawing.
- `settings.scale_ratio` is 1 in most drawings. A sheet drawn at full size for
  drawings at 1:100, such as a title block, saves 0.01: its `drawing_scale` is
  1 and the scale it states, `settings.stated_scale`, is 100. Coordinates follow
  `drawing_scale`; only the stated scale changes.
- `settings.width_by_color` is set when the drawing is printed with the pen
  width of each color, `Color.pen_width`, instead of the width of each object.
- `color`, `line_type` and `width` are indices into `drawing.colors`,
  `drawing.line_types` and `drawing.widths`. The `BY_LAYER_*` and `BY_BLOCK_*`
  constants mark values that defer to the layer or to the enclosing block.
- A line-type pattern is a list of dash and gap lengths, starting with a dash,
  in millimetres on the output sheet: multiply them by `drawing_scale` to get
  drawing units. A width is millimetres on the output sheet.
- A layer with `hidden=True` is still returned; hiding its entities is left to
  the consumer. Bit `0x0800` of a layer's `flags` marks a layer that is not
  printed.
- The first releases saved shorter style tables: 200 layers, 32 colors, 16 line
  types and 16 widths, and layers without a name or a width. A layer whose
  `width` is 0 has no attributes of its own; the application shows it with
  color 7, line type 1 and width 1.
- Some sizes are stored in millimetres on the output sheet instead of drawing
  units. A text or an arrowhead with `output_size=True` is one: multiply its
  sizes by `drawing_scale`.
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
`group`, `link_id`, `segment`, a source location (`table`, `block_index`,
`record_index`, `payload_offset`, `record_count`), a read-only `data` mapping
and `children`. Angles are degrees, counterclockwise. `segment` is zero for an
object of the model and the number of its sheet otherwise; see
[sheets](#sheets).

| `type` | `data` keys | `children` |
| --- | --- | --- |
| `POINT` | `position`, `point_type` | |
| `LINE` | `start`, `end`, `elevation`, `thickness` | |
| `LINE3D` | `start`, `end` with three coordinates | |
| `CIRCLE` | `center`, `radius`, `elevation`, `fill` | |
| `ARC` | `center`, `radius`, `start_degrees`, `end_degrees`, `elevation` | |
| `ELLIPSE` | `center`, `radius` (two values), `rotation_degrees`, `fill` | |
| `ELLIPTICAL_ARC` | as `ELLIPSE` without `fill`, with `start_degrees`, `end_degrees` | |
| `TEXT` | `position`, `elevation`, `text`, `text_bytes`, `height`, `width`, `spacing`, `output_size`, `rotation_degrees`, `slant_degrees`, `origin`, `flags`, `font_name`, `charset`, `family`, `numeric`, `number`, `unit`, `item`, `format_codes`, `lossy` | |
| `POLYLINE` | `curve_kind`, `flags`, `closed`, `divisions`, `three_d`, `fill`, `curve_points` | Member lines and arcs |
| `EXTENDED_POLYGON` | `polygon_kind`, `flags`, `fill`, `reference_number`, `hole_count`, `region_id`, `name` | Boundary polylines |
| `HATCH` | `hatch_type`, `region_id`, `lines`, `pattern`, `gradation` | |
| `DIMENSION` | `dimension_type`, `flags`, `flags2`, `arrow_styles`, `text_position`, `line`, `arc`, `extension_origins`, `arrow_sizes`, `arrow_paper_sizes` | The numeric text |
| `LEADER` | `leader_type`, `flags`, `flags2`, `arrow_style`, `lines`, `arrow_size`, `arrow_paper_size` | The note text |
| `PACKAGE` | `package_kind`, `flags`, `reference_number` | Member entities |
| `ARROWHEAD` | `position`, `size`, `angle_degrees`, `arrow_type`, `flags`, `output_size` | |
| `INSERT` | `name`, `name_bytes`, `reference_number`, `position`, `elevation`, `scale`, `rotation_degrees` | |
| `IMAGE` | `file_name`, `directory`, `position`, `elevation`, `scale`, `size`, `rotation_degrees` | |
| `EXTERNAL_REFERENCE` | `symbol`, `file_name`, `directory`, `relative_directory`, `position`, `elevation`, `scale`, `rotation_degrees` | |
| `LAYOUT` | `number`, `name`, `paper_type`, `paper_size` | |
| `VIEWPORT` | `flags`, `paper_point`, `model_point`, `scale`, `rotation_degrees`, `center`, `half_size` | |
| `POLYGON_3D` | `hole_count` | Boundary polylines in space |
| `CAMERA`, `LIGHT` | `position`, `target`, three coordinates each | |

A `POINT` is a marker of a fixed size on the screen. `point_type` names its
shape: 1 a dot, 2 a cross, 3 a diagonal cross, 4 a circle, 5 a square, 6 a
circle with a cross, 7 a square with a cross, 8 an asterisk, 9 a triangle, and
10 to 12 the filled circle, square and triangle. 0 follows a setting of the
application, which draws a dot.

### Fills

`fill` is `None` for a shape that is not filled. Otherwise it is a mapping:

| Key | Meaning |
| --- | --- |
| `color` | 1 to 256: an index into `drawing.colors`. `RGB_FILL_COLOR` (257): use `rgb`. `BACKGROUND_FILL_COLOR` (258): the background color of the view |
| `rgb` | Three numbers. The fill color with `RGB_FILL_COLOR`; the background color at the time of saving with `BACKGROUND_FILL_COLOR` |
| `transparency` | 0 opaque to 255 fully transparent |
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

`height` is the height of the character cell. `width` is the advance of one
character, so a text is `width` times its number of characters long; zero means
the natural width of the font. With `output_size=True` both are millimetres on
the output sheet, as is `spacing`.

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
for a region without a hatch. A `HATCH` is the definition of the hatching:

- `hatch_type` 0: `lines` lists families of parallel lines, each with `origin`,
  `pitch`, `angle_degrees` and its own `color`, `line_type` and `width`.
- `hatch_type` 1: `pattern` holds the `name` of a pattern of the application's
  pattern library, its `origin`, the saved numeric `values` and the attribute
  references.
- `hatch_type` 2 is a gradation: `gradation` holds its `start` and `end`
  points, the colors `rgb_start` and `rgb_end` at them and a
  `gradation_type`.

The hatching as it is drawn is stored next to its definition: a `PACKAGE` with
`package_kind` 5 whose `reference_number` equals that of the region. Its
members are the hatch lines, already cut to the region, and the boundary
polyline unless the region hides it. This holds for a named pattern too, so its
lines need no pattern library. To draw a hatched region, draw that package and
skip the `HATCH`; draw the `EXTENDED_POLYGON` itself only for its fill. The
application draws nothing for a hatch without such a package. The package of
a gradation holds filled bands of one color each.

A region with `polygon_kind` 1 is a named area, such as a room or a site.
`name` is its name, and a `TEXT` among its children draws its label.

### Dimensions and leaders

A `DIMENSION` holds the measured `line`, the extension origins, the arrow sizes
and the numeric text. `dimension_type` is 1 aligned, 2 linear, 3 angle, 4 radius
or 5 diameter. An angle dimension also has `arc`, with `center`, `radius`,
`start_degrees` and `end_degrees`. A `LEADER` holds its `lines`, each a pair of
points, and the note text.

The lines, arcs, arrowheads and text that draw a dimension or leader are the
members of a `PACKAGE` with `package_kind` 3 or 4. A dimension and the members
of its package share a `link_id`; leaders and their packages follow each other
in the same order. To draw a dimension or leader, draw the package and skip the
`DIMENSION` or `LEADER`.

An `ARROWHEAD` has its tip at `position` and points along `angle_degrees`.
`size` is its width across the line and its length along it, in millimetres on
the output sheet when `output_size` is set. `arrow_type` selects the shape:

| `arrow_type` | Shape |
| --- | --- |
| 0 | Nothing |
| 1, 2 | Circle around `position`, as wide as the width |
| 3 | Filled circle |
| 4 | Two lines from the tip back to the corners of the width and length |
| 5, 6 | Closed triangle with those corners |
| 7 | Filled triangle |
| 8, 9 | Square around `position`, as wide as the width |
| 10 | Filled square |
| 11 | Slash across `position` |
| 12 | Integral sign across `position` |
| 13 | One line from the tip back to one side, the full width away |
| 14 | Closed half arrow on that side |
| 15 | Filled half arrow |

The side of a half arrow is the left of its direction, and the right when
`angle_degrees` lies from 90 to 270. On a horizontal line both heads then stand
above the line.

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

An `IMAGE` names a picture file by `file_name` and `directory`. `position` is
its lower left corner and `size` the drawn width and height in drawing units.
An `EXTERNAL_REFERENCE` names another drawing: with `symbol=False` a whole
drawing laid under this one, with `symbol=True` a part placed like a block
reference. The file contents are not part of the drawing and are not read.

### Sheets

A drawing can hold sheets next to its model. A `LAYOUT` describes one: its
`number`, its `name` and its paper in millimetres. Objects whose `segment` is
that number are drawn on the sheet, in millimetres from its lower left corner;
objects with `segment` 0 are the model.

A `VIEWPORT` is a window of its sheet onto the model. `center` and `half_size`
give its rectangle on the sheet. The model point `model_point` appears at
`paper_point`, and a model point `p` at

    paper_point + R(rotation_degrees) * (p - model_point) * scale

so `scale` is the sheet length per model length: 0.02 for 1:50. `center` and
`half_size` are `None` for a window that is not a rectangle.

### Model objects

A `POLYGON_3D` is a face in space with holes: its first child polyline is the
outer boundary and carries the fill, the next `hole_count` cut holes. `CAMERA`
and `LIGHT` are a viewpoint and a light source for perspective views; they are
not part of what is drawn.

## Properties and text fields

`drawing.properties` lists the properties saved with the drawing, each with an
`owner`, a `name` and a `value`. The application keeps its own bookkeeping in
them. Those owned by `DRAWING_PROPERTY_SET` are the property set of the
drawing; `drawing.property_set()` returns them by name.

A text can contain fields, which the application replaces when it draws:

| Field | Prints |
| --- | --- |
| `{TCODE:DrawingName}` | The file name without its extension |
| `{TCODE:FileName}` | The file name |
| `{TCODE:Comment}` | `settings.comment` |
| `{TCODE:PaperSizeName}` | `settings.paper_name` |
| `{TCODE:PaperOrientation}` | Upright or lying, in the language of the application |
| `{TCODE:PaperScale}` | `1:` and `settings.drawing_scale` |
| `{TCODE:DrawingScale}` | `1:` and `settings.stated_scale` |
| `{PSET:name}` | The value of that name in the property set |

A name the application does not know prints as `{ERR:name}`. A cut can follow
the name of a `TCODE` field: `@"L4"` keeps the first four characters, `@"R3"`
the last three, `@"M6"` everything from the sixth on and `@"M2,3"` three from
the second. Fields for the user, the licence, dates, the folder and the file
size take their value from outside the drawing. The reader returns texts as
stored and replaces nothing.

## Report

| Field | Meaning |
| --- | --- |
| `decoded_counts` | Decoded entities by type, at every nesting level |
| `unsupported_counts` | Objects of a kind that is not decoded, by object kind number |
| `failed_counts` | Objects of a known kind that did not fit its layout |
| `property_records` | Property records, which hold no geometry; see `drawing.properties` |
| `erased_objects` | Objects the application had erased; their records stay in the file |
| `orphan_records` | Continuation records that belonged to no object |
| `clamped_spans` | Objects whose declared length ran past their parent |
| `lossy` | Whether anything was skipped or cut: one of `unsupported_counts`, `failed_counts`, `orphan_records` and `clamped_spans` is nonzero |
| `diagnostics` | Notes with a code, a message and a record location |

Erased objects and properties are neither decoded entities nor losses. The
construction lines and embedded objects of other programs that the application
can hold have not been seen in a saved drawing; they would be counted in
`unsupported_counts`.

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
