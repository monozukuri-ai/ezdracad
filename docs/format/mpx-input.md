# MPX input

Observed marker-25 flat MPX uses the existing inspection, raw scan, and document
entry points. The input is the original flat file; an extracted MPZ Contents
payload is not an MPX file merely because its extension is changed.

```python
from ezdracad import raw

info = raw.inspectfile("drawing.mpx")
assert info.format == "mpx" and info.cfb is None
assert info.payload.offset_domain == "file"
original = info.read_payload()
```

MPX has a distinct end-of-file convention from MPZ Contents. The parser keeps
all original bytes and does not insert or remove a trailer for parser reuse.
The payload hash covers the entire original file. Raw, entity, diagnostic, and
opaque-region offsets are file-relative, with `stream_path=None`.

Inspection and raw scanning do not establish geometry support. Existing
geometry profiles, state boundaries, and read policies apply to MPX independently
of successful carrier identification. Only the supported marker-25 flat
encoding is admitted.

MPX lacks a complete payload checksum in this supported envelope. Validated
bounds cannot detect every modification within an opaque suffix. Parsing is
not an authenticity or complete-integrity check.
