# Documents, policies, and queries

`ezdracad.read` and `readfile` create an owned, read-only `Document`. The default
entity profile decodes restricted stored-XY lines and supported placements.
An explicit [basic profile](basic2d-profile.md) adds root CIRCLE, ARC, and TEXT.

```python
import ezdracad

doc = ezdracad.readfile("drawing.mpz", policy="partial")
for line in doc.query("LINE"):
    print(line.start, line.end, line.source_ref)
print(doc.report.to_dict())
```

## Policies and scope

| Scope | Meaning requested |
| --- | --- |
| `drawing` | Drawing semantics, including units, WCS, and visibility |
| `stored_xy_lines` | Only the supported stored-XY line and style-index contract |
| `stored_xy_primitives` | The supported primitive contract; requires the basic profile |

`strict` raises `IncompleteReadError` for an incomplete request and attaches a
report. `partial` returns admitted entities with that report. Current support
does not complete drawing semantics. Completing a narrower scope still leaves
`document_complete=False` and unresolved semantics in the report.

Unknown objects or state can affect later geometry. The first unsupported root
object is held, and later root objects are blocked. Supported earlier objects
remain queryable. A selected subtree is atomic: unsupported descendants reject
that selection. Known corruption, nonfinite numbers, or exhausted budgets are
fatal even after a state boundary has blocked further output.

## Entities and provenance

`doc.query("LINE")` and `doc.query("*")` return immutable sequences of decoded
entities. Iteration fetches bounded pages; explicit pages contain at most 4096
items. Entity IDs are ordinals in the snapshot, not original drawing handles.
Separate placements of one definition remain separate entities.

`SourceRef` identifies the source table, optional definition index, record index,
payload offset, and byte length. Interpret offsets using `offset_domain`:
`cfb_stream` for MPZ, `file` for MPX, and `decoded_payload` for MPS. MPS decoded
ranges are not compressed-stream offsets. The payload hash binds source ranges
to exact retained bytes. Use `doc.raw()` for low-level access.

`Document.to_dict()` and entity `to_dict()` methods return JSON-compatible
values without copying raw byte bodies into the result. Unknown semantics and
unsupported/blocked counts remain explicit. Additional container entries are
inventoried without interpreting or executing their contents.

## Budgets

`ParseLimits` bounds input, CFB, record, and MPS decoded sizes. `ReadLimits`
independently bounds root objects, generated entities, traversal work, retained
provenance, and diagnostics. Text limits apply when the basic profile is enabled.
Truncated diagnostic reporting is counted and never changes the completeness
policy. See [raw access](../raw-api.md) and [compatibility](../compatibility.md).
