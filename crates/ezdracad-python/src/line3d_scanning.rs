//! Tuple boundary for record-local 3D lines; no Python-side binary parsing.

use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub(crate) type LineRecord3DRow = (u64, u64, u16, Py<PyBytes>, [f64; 3], [f64; 3]);

pub(crate) fn row(py: Python<'_>, source: ezdracad_core::LineRecord3D) -> LineRecord3DRow {
    (
        source.record_index,
        source.payload_offset,
        source.tag_raw,
        PyBytes::new(py, &source.header_bytes).unbind(),
        source.start,
        source.end,
    )
}
