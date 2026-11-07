# Synthetic root primitive fixtures

`basic2d25.bin` and `basic2d25.cfb` contain authored LINE, CIRCLE, ARC, and literal
TEXT records for the explicit root-primitive profile. Tests modify spans, flags,
text encodings, numeric fields, and limits to check accepted and rejected forms.

The construction is in `scripts/make_basic2d_fixture.py`. The fixtures use the
project's PolyForm Noncommercial License 1.0.0 and contain no copied native
drawing bytes. They do not establish general units, WCS, glyph rendering, or
complete drawing support.
