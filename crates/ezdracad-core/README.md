# ezdracad-core

A read-only Rust parser for supported DRA-CAD MPZ, MPX, and unprotected MPS
encodings. The crate provides container inspection, raw record access, and
conservative partial geometry decoding. It does not require DRA-CAD at runtime.

## Read everything that is recognized

```rust,no_run
use ezdracad_core::{read_best_effort_file, BestEffortLimits, ParseLimits, Result};
use std::path::Path;

fn summary(path: &Path) -> Result<()> {
    let drawing =
        read_best_effort_file(path, &ParseLimits::default(), &BestEffortLimits::default())?;
    for entity in &drawing.entities {
        println!("{} on layer {}", entity.kind(), entity.attributes.layer);
    }
    println!("{:?}", drawing.report.unsupported_counts);
    Ok(())
}
```

`read_best_effort` decodes every object whose saved layout is known, in the
raw markers 3 to 25 and in both record layouts, including MPUW input without a
password, and counts the rest by object kind. It
returns stored coordinates and file-local attribute references, applies no saved
setting, and makes no claim of completeness. An object that does not fit its
layout is skipped and counted; malformed tables and exhausted budgets are errors.

## Read with qualification

```rust,no_run
use ezdracad_core::{read_file, ReadOptions, ReadPolicy, ReadResult};
use std::path::Path;

fn lines(path: &Path) -> ReadResult<()> {
    let doc = read_file(path, &ReadOptions {
        policy: ReadPolicy::Partial,
        ..ReadOptions::default()
    })?;
    for entity in doc.query("LINE", 0, 256)? {
        let line = entity.as_line().expect("LINE query");
        println!("{:?} -> {:?}", line.geometry.scene().start, line.geometry.scene().end);
    }
    println!("{:?}", doc.report().unsupported_counts);
    Ok(())
}
```

`ReadOptions` defaults to strict drawing semantics. An incomplete request
returns a `ReadError` with a structured report. Partial policy returns admitted
entities and their limitations; malformed data and exhausted budgets remain
errors. An unsupported object can block later root objects.

Lines use an explicit stored-XY profile. Root circles, arcs, and literal text
require `Basic2DProfile::Marker25RootPrimitivesV1`. These profiles do not provide
complete drawing semantics, general physical units, WCS, visibility, or rendering.
`ReadScope::StoredXyLines` and `ReadScope::StoredXyPrimitives` narrow the requested
meaning; a complete narrow scope still has `document_complete() == false`.

## Inspect bytes

```rust,no_run
use ezdracad_core::{inspect_file, ParseLimits, Result};
use std::path::Path;

fn inspect(path: &Path) -> Result<()> {
    let info = inspect_file(path, &ParseLimits::default())?;
    println!("{}", info.format.as_str());
    if let Some(payload) = &info.payload {
        println!("{} {}", payload.sha256, payload.offset_domain());
    }
    Ok(())
}
```

`scan_file` / `scan_records` retain exact payload bytes and expose observed raw
marker-5, marker-8, and marker-25 layouts. A marker is an uninterpreted header
value, not an application version. Low-level line, style, block, coordinate,
and print-setting APIs have their own explicit profiles and error boundaries.

## Bounds and source locations

`ParseLimits` bounds input, CFB metadata and streams, record counts, and MPS
output. The default decompressed-size limit is 128 MiB. MPS decoding checks
lengths, zlib EOF, checksum, and exact consumption, using the Rust
`miniz_oxide` backend. Only the two supported unprotected wrappers are admitted;
other compression and protection forms are unsupported. The two unprotected
MPUW wrappers are decoded by `read_best_effort` only.

Payload and entity offsets use `PayloadOffsetDomain`: `CfbStream` for MPZ,
`File` for flat MPX, and `DecodedPayload` for MPS. MPS retains the wrapped-stream
hash separately from the decoded payload hash. A decoded offset is not a
compressed byte offset.

Read limits additionally bound objects, entities, traversal work, provenance,
and diagnostics. Unknown bytes remain available from the owned raw snapshot.
A successful inspection or raw scan is not a successful geometry decode.

## License

PolyForm Noncommercial License 1.0.0. Use is restricted to the purposes permitted
by that license; commercial use outside those grants requires separate permission.
See `LICENSE` and `THIRD_PARTY_NOTICES.txt`. DRA-CAD is a product name of its
respective owner; this is not an official vendor SDK.
