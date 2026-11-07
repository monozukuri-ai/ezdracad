# Loaded-file 3D absolute-input convention

`world_input_3d_context(profile=...)` provides an explicit conversion for the
supported loaded-file absolute-input convention. It does not decode a complete
3D scene or authenticate transient live application state.

```python
from ezdracad import raw
from ezdracad.world_input3d import WorldInput3DProfile

scan = raw.scanfile("drawing.mpz")
context = scan.world_input_3d_context(
    profile=WorldInput3DProfile.MARKER25_LOADED_WORLD_INPUT_V1,
)
q_mm = context.input_mm_from_world_stored_xyz((110, -45, 2.5))
literal = tuple(context.input_unit.from_native_mm(x) for x in q_mm)
stored = context.world_stored_xyz_from_input_mm(q_mm)
```

Admission requires marker 25, supported input codes 0 (mm) or 2 (m), disabled
Draft with supported selectors, the supported origin setting, and finite
positive scales. Other states are unsupported by this profile.

Let `s` be supplied stored-world coordinates, `O=(saved_origin_x,
saved_origin_y,0)`, and `r=current_D/current_A`:

```text
q_mm = (s - O) / r
s = O + r * q_mm
```

Zero origin Z is part of this loaded-file convention, not an inferred header
field. Grid origin and workplane origin are separate quantities. The input-unit
converter maps literal keyboard digits to or from native-mm equivalents. These
contextual units do not change `LineRecord3D.units=None` or establish an entity's
physical units.

The result retains current scale, unit, origin settings, and source bytes.
Checked arithmetic rejects invalid values, overflow, and nonzero underflow.
No creation-scale, visibility, layout, arbitrary-parent, or general WCS claim
follows from converting a bare supplied point.
