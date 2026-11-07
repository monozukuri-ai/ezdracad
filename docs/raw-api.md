# Raw inspection and source locations

```python
from ezdracad import raw

info = raw.inspectfile("drawing.mpz")
print(info.format, info.file_size, info.cfb, info.payload)
if info.payload is not None:
    original_payload = info.read_payload()
```

`raw.inspect(bytes)` has the same inspection contract. `raw.inspect_cfb(bytes)`
only inventories the compound container. Unknown CFB identities and unsupported
MPS wrappers have no decoded drawing payload. Inspection does not decode geometry.

## Source domains

| Carrier | Offset domain | Payload stream path | Payload hash |
| --- | --- | --- | --- |
| MPZ | `cfb_stream` | Actual Contents stream | Exact stream bytes |
| MPX | `file` | `None` | Entire original flat file |
| MPS | `decoded_payload` | `None` | Exact decoded bytes |

MPS `info.mps` additionally describes the original SecureContents stream and its
hash. Its chunk ranges describe encoded and decoded extents; a decoded byte
offset is not a direct compressed-stream byte offset.

The legacy `read_contents()` method is an alias for retained payload access.
Snapshots do not reopen their source files.

## Records

```python
scan = raw.scanfile("drawing.mpz")
print(scan.header.profile, scan.tag_counts)
for record in scan.records(start=0, count=256):
    print(record.index, record.payload_offset, record.tag)
first_record_bytes = scan.record_bytes(0)
```

`raw.scan_records(bytes)` scans a carrier in memory. `raw.scan_contents(bytes)`
is a separate low-level entry point for an already extracted Contents payload;
renaming extracted bytes does not make them a native MPX input.

Record counts include terminal records. A record is not necessarily an entity:
one object can occupy multiple records, and records can describe state or
metadata. Pagination is bounded, and unknown records retain their exact bytes.
Raw XY candidates are provisional fields, not normalized drawing geometry.

The raw snapshot also exposes line fields, styles, block definitions, scene
selections, saved coordinate metadata, and print-setting APIs. Each decoder has
its own supported profile. Unsupported interpretation does not remove the
underlying raw bytes.
