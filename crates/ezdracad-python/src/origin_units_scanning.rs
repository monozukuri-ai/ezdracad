//! Thin source-preserving Origin.Units metadata projection; no unit inference.

use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub(crate) type OriginUnitsRow = (u64, Py<PyBytes>, i32);

pub(crate) fn row(py: Python<'_>, value: ezdracad_core::OriginUnitsCode) -> OriginUnitsRow {
    (
        value.payload_offset,
        PyBytes::new(py, &value.selector_bytes).unbind(),
        value.raw_code,
    )
}
