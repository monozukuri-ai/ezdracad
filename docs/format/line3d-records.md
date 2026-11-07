# Record-local 3D lines

`RawScan.line_records_3d(record_start=0, record_count=1024)` exposes supported
record-local 3D line fields. Rust provides `line_record_3d` and `line_records_3d`.
The supported form is exact tag `0x0013` in observed marker-5 and marker-25
payloads. Flagged variants are not admitted by masking the tag.

Each source record retains its complete header and six finite endpoint values:
start XYZ and end XYZ. These independent endpoint Z values differ from a 2D
line's common elevation and thickness.

`coordinate_space="record_local"` and `units=None`. Reading a raw slot does not
establish logical root membership, parent placement, workplane, WCS, or complete
drawing semantics. A line-like slot can belong to an opaque structure.

Pages use raw record indices with a 4096-record cap. Unknown tags remain opaque.
Nonfinite recognized fields reject the whole page without returning a valid
prefix. Source bytes and payload-relative offsets remain tied to the owned
snapshot and its explicit source domain.
