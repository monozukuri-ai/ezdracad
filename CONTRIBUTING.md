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

## Releases

[The release workflow](.github/workflows/release.yml) runs the complete CI suite,
then builds CPython 3.10 ABI3 wheels for Linux x86-64/AArch64 (glibc 2.28 or
later), macOS Intel/Apple Silicon, and Windows x86-64. Each wheel is installed
and tested on its native runner. The source archive is rebuilt and checked
on Python 3.10. The publication bundle contains these six distributions and
`SHA256SUMS`; contents, metadata, license notices, and hashes are checked again
before publication.

Manual `workflow_dispatch` runs produce a verified `publication-bundle` artifact
for review. A pushed `v<workspace.package.version>` tag publishes the same bundle
to PyPI and attaches its distributions and checksums to a GitHub Release.
Alpha, beta, and release-candidate tags create GitHub prereleases.

Before the first publication, configure a PyPI
[Trusted Publisher](https://docs.pypi.org/trusted-publishers/adding-a-publisher/)
with project `ezdracad`, owner `monozukuri-ai`, repository `ezdracad`, workflow
`release.yml`, and environment `pypi`. If the project does not exist yet, register
a [pending publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/)
with those values. Create the matching GitHub environment and choose its
deployment rules. Publishing uses GitHub OIDC, with no PyPI API token stored
in repository secrets.

Update the shared version and lockfiles, commit the change, and push the matching
tag:

```sh
python scripts/bump_version.py X.Y.Z
```

Supported prereleases include `X.Y.Z-alpha.1`, `X.Y.Z-beta.1`, and `X.Y.Z-rc.1`.
For example, Cargo version `0.1.0-alpha.1` uses tag `v0.1.0-alpha.1` and Python
package version `0.1.0a1`.
