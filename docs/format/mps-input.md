# Unprotected MPS input

Two supported unprotected SecureContents wrappers use the existing inspection,
raw scan, and document APIs. Detection checks the CFB identity and exact wrapper
profile. Passwords are not accepted. Unknown protection, other compression
modes, multiple chunks, and MPUW remain unsupported.

```python
from ezdracad import ParseLimits, raw

info = raw.inspectfile(
    "drawing.mps",
    limits=ParseLimits(max_decompressed_size=32 * 1024 * 1024),
)
if info.format == "mps":
    print(info.mps.stream_sha256, info.payload.sha256)
    assert info.payload.offset_domain == "decoded_payload"
    original_decoded_bytes = info.read_payload()
```

The original file size and CFB inventory remain container metadata. `info.mps`
identifies the wrapped stream, its hash, and encoded/decoded chunk extents.
`info.payload` identifies exact decoded bytes and their separate hash. Its
`stream_path` is `None`, because decoded offsets do not address the compressed
stream. Entity, record, diagnostic, and opaque-region offsets all use
`decoded_payload`.

The parser checks the supported wrapper before decoding a single zlib member.
It validates the size declaration, actual output, EOF, Adler32, and exact input
consumption. Trailing data, concatenated members, and preset dictionaries are
rejected. The Rust backend uses bounded output buffers and does not search for
compression signatures.

`max_decompressed_size` defaults to 128 MiB and is independent of file and stream
budgets. Unsupported wrapper candidates have no decoded payload. A successfully
decoded payload may still have an unsupported raw marker or geometry layout;
decompression does not broaden the reader's compatibility contract.
