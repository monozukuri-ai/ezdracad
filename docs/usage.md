# User guide

## Choose a reader

`ezdracad.best_effort.readfile` decodes every object whose saved layout is known
and reports the rest by count. It reads every record generation the application
can save, and MPUW files without a password, and is the reader to use for
display or conversion.

```python
from ezdracad import best_effort

drawing = best_effort.readfile("drawing.mpz")
print(drawing.report.decoded_counts, drawing.report.lossy)
for entity in drawing.walk():
    print(entity.type, entity.data)
```

`ezdracad.readfile` is the qualified reader described below. It returns only
objects whose meaning it can establish and blocks the objects after an unknown
one. Use it when a wrong interpretation is worse than a missing one. The two
readers are independent; see [best-effort reading](format/best-effort.md).

## Read policies

```python
import ezdracad

doc = ezdracad.readfile("drawing.mpz", policy="partial")
print(doc.report.status, doc.report.document_complete)
print(doc.report.unsupported_counts, doc.report.context_blocked_counts)
for line in doc.query("LINE"):
    print(line.start, line.end, line.source_ref)
```

`read(bytes, ...)` accepts in-memory bytes; `readfile(path, ...)` reads a regular
file. Both retain an owned snapshot, so subsequent queries do not reopen the
original input. Query objects support iteration, indexing, slicing, and bounded
pages.

The default `strict` policy raises `IncompleteReadError` when the requested
scope is incomplete. Read `error.report` for coverage details. `partial` returns
admitted entities with the report, but it still rejects malformed fields,
nonfinite numbers, corrupt structure, and resource-limit violations.

Unsupported state can affect later geometry. The reader therefore holds the
unsupported object and blocks later root objects instead of guessing its effect.
An empty result does not prove that the file contains no geometry.

## Explicit primitive support

```python
doc = ezdracad.readfile(
    "drawing.mpz",
    policy="partial",
    basic_profile="marker25_root_primitives_v1",
)
for entity in doc.query("*"):
    print(entity.type, entity.source_ref)
```

The basic profile adds restricted root CIRCLE, ARC, and literal TEXT support.
It does not expand their block, nesting, fill, 3D, or text-formatting support.
See [the primitive contract](format/basic2d-profile.md).

## Narrow scopes

The default `scope="drawing"` requests drawing meaning, including units,
coordinate systems, and visibility; this meaning is not currently complete.
`scope="stored_xy_lines"` narrows the request to the supported stored-XY line
contract. `scope="stored_xy_primitives"` requires the basic profile and narrows
the request to its supported primitive contract.

A narrow scope can have `status="complete"` while `document_complete=False`.
Inspect the scope, unsupported counts, blocked counts, and unresolved semantics
before consuming output. Stored coordinates do not automatically have physical
units or WCS meaning.

## Limits and errors

```python
from ezdracad import ParseLimits

limits = ParseLimits(
    max_file_size=32 * 1024 * 1024,
    max_decompressed_size=64 * 1024 * 1024,
)
info = ezdracad.raw.inspectfile("drawing.mps", limits=limits)
```

`ParseLimits` bounds input, container metadata, streams, records, and MPS output.
`ReadLimits` bounds objects, entities, traversal work, source evidence, and
reported diagnostics. These are independent budgets. A limit is an error under
both read policies; suppressed diagnostics never imply complete coverage.

The public error hierarchy includes `InvalidFormatError`,
`UnsupportedFormatError`, `UnsupportedFeatureError`, `LimitExceededError`, and
`DracadIoError`. `IncompleteReadError` carries a read report.
