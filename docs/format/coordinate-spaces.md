# Saved coordinate frames

Explicit coordinate profiles convert root stored XY using saved current
settings. They do not recover an entity's immutable physical units or original
creation scale.

```python
from ezdracad import raw
from ezdracad.coordinate_space import CoordinateFrame, CoordinateSpaceProfile

scan = raw.scanfile("drawing.mpz")
context = scan.coordinate_space_context(
    profile=CoordinateSpaceProfile.MARKER25_STORED_AXES_V1,
)
frame = CoordinateFrame.CURRENT_ABSOLUTE_INPUT_MM_FROM_DRAWING_ORIGIN
point = context.from_root_stored_xy((110.0, -45.0), frame=frame)
stored = context.to_root_stored_xy(point, frame=frame)
literal_digits = tuple(context.input_unit.from_native_mm(x) for x in point)
```

## Profiles

`marker25_stored_axes_v1` admits supported input-unit codes 0 through 6 and
supported Draft selectors/angles. `marker5_stored_axes_v1` and
`marker8_stored_axes_v1` require the narrower millimeter-input, disabled-Draft
state. All profiles require finite origins and angles and finite positive
scale fields. Out-of-profile settings are unsupported, not necessarily invalid
files. Point-profile admission does not broaden line-decoder admission.

## Named frames

Let `s` be root stored XY, `O` the saved drawing origin, `p=1/D` the paper
factor, `r=D/A` the current drawing/annotation ratio, and `R(a)` a planar
counterclockwise rotation by the saved Draft angle.

| Frame | Map from root stored XY | Units |
| --- | --- | --- |
| `root_stored_xy` | `s` | Unknown |
| `drawing_origin_relative_stored_xy` | `s-O` | Unknown |
| `current_annotation_mm_from_drawing_origin` | `(s-O)/r` | Contextual mm |
| `current_absolute_input_mm_from_drawing_origin` | `R(-a)*(s-O)/r` with active Draft, otherwise `(s-O)/r` | Contextual mm |
| `configured_paper_mm_offset_from_drawing_origin` | `p*(s-O)` | Configured paper-mm offset |

The absolute-input target returns native-mm equivalents. Use `input_unit`
conversion to obtain literal input digits. Its rounded imperial coefficients
are preserved application conventions, not exact-SI conversions. Configured
paper offsets are not page positions or printer-output mappings.

For a decoded selection, `qualified_lines_in_coordinate_space` requires explicit
`line_profile`, `space_profile`, and `frame` arguments. The whole subtree is
admitted and placed before its endpoints are mapped; failure returns no partial
selection. Original local geometry, transforms, attributes, and source evidence
remain available through the result's `source` fields.

Conversions retain exact saved context bytes and use checked Rust arithmetic.
Unknown selectors, nonfinite numbers, overflow, nonzero underflow, or lost
translation displacement are rejected. Numerical roundtrips do not promise
bitwise reversibility. A bare supplied point has no entity provenance.
