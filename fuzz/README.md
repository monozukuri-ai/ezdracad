# Fuzzing

The seven targets accept arbitrary input bytes with bounded parse and read limits.
Successful parses additionally check source ranges, hash retention, and
strict/partial behavior.

| Target | Synthetic seed input |
| --- | --- |
| `scan_contents` | `tests/fixtures/records*.bin`, block/object/semantic payloads |
| `scan_mpz` | Matching `.cfb` containers |
| `read_document` | `document*.cfb`, `blocks25.cfb`, `semantics25.cfb` |
| `read_basic2d` | `basic2d25.cfb` |
| `read_mpx` | `mpx*.mpx` |
| `read_mps` | `mps-*.mps` |
| `read_best_effort` | `best-effort8.cfb`, `best-effort8.bin`, `best-effort25u.mpuw`, `basic2d25.cfb` |

Use nightly Rust and `cargo-fuzz`:

```sh
mkdir -p fuzz/corpus/read_mps
cp tests/fixtures/mps-*.mps fuzz/corpus/read_mps/
cargo +nightly fuzz run read_mps -- -runs=10000 -max_len=262144 -timeout=5
```

Use the corresponding seed directory for another target. The CI workflow shows
bounded smoke runs for all targets. Corpora must contain only independently
authored, redistributable synthetic inputs. Keep generated corpus mutations,
coverage data, and crash artifacts out of source control.

`scan_contents` is a low-level extracted-payload interface, not flat MPX
identification. `read_mps` checks the supported single-member zlib wrappers,
decoded output caps, source domains, and geometry guards. Unknown encodings
must not yield decoded drawing bytes.

The fuzz workspace has its own lockfile. Check it with:

```sh
cargo fmt --manifest-path fuzz/Cargo.toml -- --check
cargo check --manifest-path fuzz/Cargo.toml --bins --locked
```
