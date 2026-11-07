# Command-line reference

```sh
ezdracad --version
ezdracad inspect drawing.mpz --json
ezdracad scan drawing.mpx --json
ezdracad scan drawing.mpz --json --records --start 0 --count 256
ezdracad extract drawing.mps --entities LINE --partial --json
```

`inspect` reports carrier and payload metadata. `scan` adds raw record counts
and optional bounded record or candidate pages. `extract` returns admitted
entities and a read report.

## Extraction options

| Option | Meaning |
| --- | --- |
| `--entities LINE` | Query decoded lines; the default |
| `--entities '*'` | Query all admitted entity types |
| `--basic-profile marker25_root_primitives_v1` | Opt in to restricted root CIRCLE, ARC, and TEXT decoding |
| `--policy strict` | Fail if the requested scope is incomplete; the default |
| `--partial` | Return admitted output with an incomplete report |
| `--scope drawing` | Request drawing semantics; the default |
| `--scope stored_xy_lines` | Request the narrower stored-XY line contract |
| `--scope stored_xy_primitives` | Request the basic primitive contract; requires the basic profile |

Run `ezdracad extract --help` for line-profile and object, entity, text, work,
and diagnostic limits. Every command accepts parse limits such as
`--max-file-size N` and `--max-decompressed-size N`, in bytes.

## Exit status and JSON

| Status | Meaning |
| --- | --- |
| `0` | Supported inspection/scan, or complete requested extraction scope |
| `1` | Malformed input or I/O error |
| `2` | Unsupported format, partial extraction, or invalid command arguments |
| `3` | Resource limit exceeded |

Successful and partial result JSON is written to stdout. Error JSON is written
to stderr. Partial extraction can emit useful output with status `2`; do not
interpret status `0` as complete drawing support. JSON omits raw byte bodies.
