# Saved workplane point context

`RawScan.workplane_context(profile=...)` exposes an owned point-conversion
context under `marker25_saved_workplane_v1`. It does not infer a raw entity's
world frame or attach physical units to geometry.

```python
from ezdracad import raw
from ezdracad.workplane import WorkplaneProfile

scan = raw.scanfile("drawing.mpz")
context = scan.workplane_context(
    profile=WorkplaneProfile.MARKER25_SAVED_WORKPLANE_V1,
)
plane_point = context.from_world_stored_xyz((113.0, -43.0, 36.0))
world_point = context.to_world_stored_xyz(plane_point)
```

Admission requires marker 25, the supported millimeter-input setting, disabled
Draft with its supported selectors, and finite positive current scale fields.
The origin and axes must be finite. Free mode requires a right-handed
orthonormal basis within absolute tolerance `1e-12`. Supported preset directions
use their selector-derived basis; stored axes remain available as source data.

`origin`, `direction`, `stored_axes`, `effective_axes`, `basis_source`, and
`evidence` expose the admitted saved state. Stored preset axes can be stale and
do not identify a previous orientation or immutable history.

The maps accept stored-plane displacements and stored-world points. They do
not apply current input-unit conversion, annotation scale, camera projection,
reference placement, or a page/printer transform. Both frames retain unknown
general units. Literal numeric-input conversion is a separate operation.

Conversions use checked Rust arithmetic and reject nonfinite input, overflow,
nonzero underflow, or displacement lost during translation. Floating-point
roundtrips are numerical rather than universally bitwise reversible.
