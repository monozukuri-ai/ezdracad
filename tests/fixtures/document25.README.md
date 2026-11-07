# Synthetic document fixtures

`document25.bin` and `document25.cfb` contain independently authored marker-25
line records. The partial variant contains unsupported state to exercise strict
and partial policies, diagnostics, source ranges, and blocking of later roots.

The construction is in `scripts/make_document_fixture.py`. These project-authored fixtures
verify implementation behavior, not complete drawing or rendering semantics.
