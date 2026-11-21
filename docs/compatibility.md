# Compatibility and limitations

## Input support

| Input | Supported scope |
| --- | --- |
| MPZ | Recognized CFB identity and Contents payload; observed raw markers 5, 8, and 25 |
| MPX | Observed marker-25 flat-file envelope, with original file offsets |
| MPS | Two supported unprotected, single-zlib-member wrappers; separate wrapped and decoded source metadata |
| Generic CFB | Metadata inventory without drawing interpretation |
| MPUW, protected MPS, other MPS compression modes | Unsupported |

Format detection uses content, not the extension. Container inspection, raw
record support, and geometry support are separate. An MPS payload can be
successfully decompressed while its record marker remains unsupported.
A raw marker is not an application release number.

## Geometry support

Two readers have different boundaries. The [best-effort reader](format/best-effort.md)
decodes points, lines, circles, arcs, ellipses, text, polylines, cubic splines,
extended polygons, hatches by line families, dimensions, packages, arrowheads,
and block placements in the record generations 5, 8, 10, 22, and 25, including
flat MPX of those generations. It counts images, external references, layouts,
viewports, three-dimensional solids, and any other object kind without decoding
them. Named hatch patterns, quadratic and Bezier curves, and the fill of circles
and ellipses are reported without being evaluated. Its output is the stored
state, with no claim of completeness.

The qualified high-level reader is conservative and partial. The default line profile
admits bounded planar stored-XY line and placement forms. An optional profile
adds root circles, arcs, and literal text. Unsupported objects or context can
block subsequent root objects. Re-saving a file can change opaque fields and
make an otherwise similar object unsupported; unknown flags are not ignored.

There is no complete modelspace/layout, visibility, editable-document, general
3D, rendering, or native writing API. Saved values, current settings, drawing
history, physical units, WCS, and final print appearance are distinct concepts.
A style width index is not a final printed width.

General original drawing scale and physical units cannot be recovered from
current settings alone. Coordinate and workplane APIs use explicit profiles;
their supported transformations do not establish general WCS geometry.
Unknown bytes and diagnostic source locations remain available for inspection.

## Integrity boundaries

MPS decoding validates declared and actual output size, checksum, EOF, and
exact input consumption. Unsupported encodings are not tried as alternate
compression or password formats.

The observed MPX envelope uses EOF rather than a complete payload checksum.
Bounds validation cannot detect every change inside an opaque suffix.
Successful parsing is not proof of file authenticity or complete integrity.

## Platform requirements

Python requires version 3.10 or later. Building the Rust code requires Rust 1.83
or later. The parser has no DRA-CAD runtime dependency and the Python package has
no runtime Python dependencies. Installations need a compatible native extension.
