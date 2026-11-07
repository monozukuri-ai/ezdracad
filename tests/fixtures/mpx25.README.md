# Synthetic flat MPX fixtures

The `mpx25.mpx`, `mpx-mixed25.mpx`, and `mpx-block25.mpx` fixtures are authored by
`scripts/make_mpx_fixture.py`. They exercise the distinct flat-file envelope,
EOF-derived bounds, record and definition offsets, byte/hash ownership, and read
policies. They are not renamed native extractions.

`mpx-block25.mpx` deliberately has a whole-file length that is not divisible by
64; individual record tables retain their own stride. Opaque bytes remain
opaque. Run `python scripts/make_mpx_fixture.py --check` to compare generated
bytes without overwriting files. All fixtures use the project's
PolyForm Noncommercial License 1.0.0.
