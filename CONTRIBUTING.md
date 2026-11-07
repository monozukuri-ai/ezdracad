# Contributing

## Development environment

Use Python 3.10 or later and Rust 1.83 or later. Install the development tools
and build the extension from the repository root:

```sh
uv sync --locked --no-install-project
uv run --no-sync maturin develop --release --locked
```

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo doc -p ezdracad-core --no-deps --locked
uv run --no-sync ruff check src tests scripts
uv run --no-sync ruff format --check src tests scripts
uv run --no-sync pytest
python scripts/check_public_surface.py
```

See [the fuzzing guide](fuzz/README.md) for bounded fuzz runs. Test fixtures
must be independently authored and redistributable. Include the relevant
license and construction instructions; do not submit third-party drawings,
installers, credentials, or local environment data.

Contributions of project-authored code and test data should use the project's
PolyForm Noncommercial License 1.0.0. Preserve any third-party notices.

## Parser changes

Keep format parsing, bounds checking, and geometry decisions in Rust. Python
should provide typed interfaces without duplicating the parser. Preserve
unknown bytes and source locations. Do not reinterpret an unknown flag as
neutral or turn a malformed input into a successful partial result.

A format change should include meaningful tests for accepted data, unsupported
forms, corrupt input, and resource limits. Keep stored coordinates, physical
units, coordinate frames, visibility, and print behavior distinct. Update the
English user documentation when public behavior changes.

## Distribution checks

```sh
uv run --no-sync python scripts/build_distributions.py --out dist --sdist
python scripts/verify_distribution.py dist/package.whl
python scripts/verify_distribution.py dist/package.tar.gz
```

Replace the artifact filenames with the generated paths. The build helper remaps local Rust source paths. Verification checks
package contents and installs the artifact into an isolated environment.
Version consistency can be checked with `python scripts/bump_version.py --check`.
