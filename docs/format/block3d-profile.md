# Direct-line 3D block placement

`RawScan.placed_block_lines_3d(index, profile=..., max_lines=4096)` returns one
atomic selection under `marker25_direct_line3d_v1`. It supports restricted
root references to definitions containing direct 3D lines and admitted metadata.
Nested references, inline packages, other geometry, ambiguous definitions,
unsupported flags, or malformed children reject the complete selection.

```python
from ezdracad import raw
from ezdracad.block3d import Block3DProfile

scan = raw.scanfile("drawing.mpz")
selection = scan.placed_block_lines_3d(
    0, profile=Block3DProfile.MARKER25_DIRECT_LINE3D_V1
)
for line in selection.lines:
    print(line.source.start, line.start, line.coordinate_space)
```

For definition-local point `p`, the supported transform is:

```text
p_parent = translation + Rz(z) * Ry(y) * Rx(x) * diag(scale) * p
```

Angles are saved degrees with right-handed rotations: scale first, then X, Y,
Z rotations, then translation. Positive finite scales are required.
`selection.reference.apply_xyz(point)` applies the same arithmetic without
reopening a file. Overflow, nonfinite values, lost nonzero displacement, and
nonzero underflow are errors.

The reference, definition, original local lines, saved bounds, source bytes,
and payload locations remain available. Saved bounds are not recomputed geometry.
Output coordinates belong to `block_parent_stored_xyz`, with unknown general
units and parent context. This is not a complete 3D scene, WCS, arbitrary parent
chain, visibility, effective-style, or rendering API.
