# ezdracad

A read-only DRA-CAD parser with a Rust core and Python bindings.

`ezdracad` inspects MPZ, flat MPX, and two supported unprotected MPS encodings.
It offers two ways to read a drawing. The best-effort reader decodes every
object whose saved layout is known, in older and newer record generations, and
counts what it skipped. The qualified reader returns a conservative subset of
lines, block placements, root circles, arcs, and literal text, and stops where
it cannot establish the meaning. DRA-CAD is not required at runtime.

This is alpha software with partial format support. A successful inspection
or extraction does not establish complete drawing semantics, physical units,
WCS coordinates, visibility, or print fidelity. Unsupported state is reported;
malformed data and resource-limit violations raise errors.

## Installation

Python 3.10 or later is required. Building from source also requires Rust 1.83
or later:

```sh
python -m pip install .
```

The Python package has no runtime Python dependencies. The native extension
contains the parser; MPS decompression uses a Rust backend.

## Read everything that is recognized

```python
from ezdracad import best_effort

drawing = best_effort.readfile("drawing.mpz")
for entity in drawing.walk():
    print(entity.type, entity.layer, entity.data)

print(drawing.report.decoded_counts)
print(drawing.report.unsupported_counts, drawing.report.failed_counts)
```

Lines, circles, arcs, ellipses, text, polylines with spline curves, hatches,
dimensions, packages, and block placements are returned with their layer, color,
line-type, and width references. Objects of other kinds are counted, not
guessed. Coordinates are the stored values; saved settings are reported and not
applied. See [best-effort reading](docs/format/best-effort.md).

## Read with qualification

```python
import ezdracad

doc = ezdracad.readfile("drawing.mpz", policy="partial")
for line in doc.query("LINE"):
    print(line.start, line.end, line.units, line.source_ref)

print(doc.report.status)
print(doc.report.unsupported_counts)
print(doc.report.context_blocked_counts)
```

The default `strict` policy raises `IncompleteReadError` when the requested
meaning cannot be established. Its `report` explains the incomplete result.
Use `partial` to inspect supported output and its limitations. Unknown state
can block later objects, so an empty query does not prove that a drawing
contains no such geometry.

Root circles, arcs, and literal text require an explicit profile:

```python
doc = ezdracad.readfile(
    "drawing.mpz",
    policy="partial",
    basic_profile="marker25_root_primitives_v1",
)
for entity in doc.query("*"):
    print(entity.type, entity.source_ref.offset_domain)
```

## Inspect without decoding geometry

```python
from ezdracad import ParseLimits, raw

info = raw.inspectfile(
    "drawing.mps",
    limits=ParseLimits(max_decompressed_size=32 * 1024 * 1024),
)
print(info.format, info.payload)
```

Format detection uses the bytes, not the filename extension. Inspection,
record scanning, and geometry decoding have separate support boundaries.
See [compatibility](docs/compatibility.md) before interpreting results.

## Command line

```sh
ezdracad inspect drawing.mpz --json
ezdracad scan drawing.mpx --json
ezdracad extract drawing.mpz --entities LINE --partial --json
ezdracad best-effort drawing.mpz --json --entities
```

Exit status is `0` for success within the requested scope, `2` for an unsupported
or partial result, `1` for malformed data or I/O failure, and `3` for a resource
limit. Extraction scripts should accept status `2` when requesting partial output.

## Documentation

- [User guide](docs/usage.md)
- [Command-line reference](docs/cli.md)
- [Compatibility and limitations](docs/compatibility.md)
- [Raw inspection and source locations](docs/raw-api.md)
- [Best-effort reading](docs/format/best-effort.md)
- [Advanced profiles](docs/README.md)
- [Contributing](CONTRIBUTING.md)

The standalone Rust crate is documented in
[ezdracad-core](crates/ezdracad-core/README.md).

## License

The library, documentation, and independently authored test fixtures are licensed
under the [PolyForm Noncommercial License 1.0.0](LICENSE). This is source-available
software with noncommercial-use restrictions, not an OSI-approved open-source
license. Commercial use outside the license grants requires separate permission
from the copyright holder. Dependencies retain their own licenses; see
[third-party notices](THIRD_PARTY_NOTICES.txt).
DRA-CAD is a product name of its respective owner. This project is not an
official vendor SDK. See [LICENSE](LICENSE).
