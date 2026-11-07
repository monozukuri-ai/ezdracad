# Recorded dimension ratios

`RawScan.dimension_scale(index, profile=...)` reads a supported root numeric
dimension and its recorded measurement ratio. It does not recover original
drawing and annotation scales independently or assign physical units to nearby
geometry.

```python
from ezdracad import raw
from ezdracad.dimension_scale import DimensionScaleProfile

scan = raw.scanfile("drawing.mpz")
scale = scan.dimension_scale(
    0, profile=DimensionScaleProfile.MARKER25_ALIGNED_NUMERIC_MM_V1
)
print(scale.recorded_value_mm, scale.annotation_to_drawing_ratio)
print(scale.equivalent_annotation_denominator)
```

The profile admits restricted marker-25 aligned numeric dimensions with the
supported millimeter convention. Other dimension types, association flags,
units, layouts, non-root selections, or unsupported embedded text are rejected.
Displayed text is not parsed as a measurement.

For recorded number `N` and stored dimension-line length `L`:

```text
annotation_to_drawing_ratio = N / L
equivalent_annotation_denominator = current_drawing_denominator * (N / L)
```

The equivalent denominator uses the current drawing setting. Neither quantity
identifies immutable creation history. Editing a saved numeric value can change
the ratio without creating new geometry. `original_drawing_denominator` and
`original_annotation_denominator` remain `None`.

The result owns its raw slots, source hash, geometry, selectors, and current
scale evidence. Geometry remains record-local with unknown general units.
Invalid numeric values, arithmetic overflow/underflow, or unrepresentable
ratios reject the request. Unsupported interpretation leaves raw access available.
