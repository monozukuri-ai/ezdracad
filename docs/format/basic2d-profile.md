# Root circles, arcs, and text

`marker25_root_primitives_v1` is an explicit read-only profile for restricted
root CIRCLE, ARC, and literal TEXT forms. The line profile remains independent.
This profile does not extend circle, arc, or text support to arbitrary blocks,
inline nesting, or 3D geometry.

```python
import ezdracad

doc = ezdracad.readfile(
    "drawing.mpz",
    policy="partial",
    basic_profile="marker25_root_primitives_v1",
)
for entity in doc.query("*"):
    print(entity.type, entity.source_ref)
for text in doc.query("TEXT"):
    print(text.text, text.basepoint, text.serialized_text)
```

| Entity | Returned values | Supported boundary |
| --- | --- | --- |
| CIRCLE | Stored XY center and positive radius | Admitted planar, unfilled root forms |
| ARC | Stored XY center, positive radius, saved start/end degrees | Admitted nondegenerate circular arcs |
| TEXT | Literal string, serialized body, font bytes/name, basepoint, width, height, rotation | Admitted ordinary single-line text with supported flags and origin |

Unknown flags and opaque header values are not ignored. Re-saving can change
such fields and make a similar-looking object unsupported. Filled/nonplanar
curves and unsupported text states are held. Malformed spans, nonfinite numbers,
missing continuations, invalid lengths, or exhausted limits remain errors under
partial policy.

Text supports printable ASCII and admitted BMP Unicode escapes, retaining the
serialized representation. This is not a general rich-text, CP932, or MPUW
decoder. Dynamic codes, arbitrary formatting, multiline/vertical text,
surrogate escapes, and numeric dimension text are unsupported. A saved font name
does not establish font availability, glyph layout, or physical text size.

The profile returns stored values with unknown general units. Width indices are
style references, not final printed widths. `stored_xy_primitives` requires this
profile and completes only when all required root objects are admitted. Unknown
state still blocks later objects. Exact recognized auxiliary properties may be
handled separately from entities; other properties remain unsupported.
