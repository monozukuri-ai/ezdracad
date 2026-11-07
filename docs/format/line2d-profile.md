# Stored-XY line profiles

The default `marker25_planar_stored_xy_v1` profile is a bounded interpretation
of stored planar line geometry and supported block placements. The separate
`marker25_planar_stored_xy_inline_v1` profile additionally admits supported
inline wrappers. Select profiles by their exact names; there is no general
fallback for unknown flags or nesting forms.

```python
import ezdracad

doc = ezdracad.readfile(
    "drawing.mpz",
    policy="partial",
    line_profile="marker25_planar_stored_xy_inline_v1",
)
for line in doc.query("LINE"):
    print(line.start, line.end, line.attributes, line.source_ref)
```

Returned coordinates belong to the explicitly named stored-XY contract.
`units=None` remains meaningful. The profiles do not establish general physical
units, WCS, visibility, page placement, or final printed appearance. A complete
`stored_xy_lines` scope is narrower than a complete drawing.

Lines retain source records, raw attributes, definition and ancestor evidence,
and supported placement transforms. Effective color, line-type, and width
values are file-local style-table indices, not rendered values. Supported
ByLayer, ByBlock, and layer-zero inheritance requires an admitted context;
missing context is not filled in with guessed defaults.

Inline wrappers and true references have different inheritance rules. Unknown
wrappers, ambiguous names, cycles, unsupported descendants, nonplanar geometry,
or exhausted budgets reject the selected subtree. The high-level reader also
blocks later root objects after unsupported state. See
[document behavior](document-reader.md).
