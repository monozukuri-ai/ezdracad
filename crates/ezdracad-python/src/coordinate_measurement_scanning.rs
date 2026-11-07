//! Owned measurement metadata and Rust-only contextual length conversions.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict};

#[pyclass(
    name = "_CoordinateMeasurementHandle",
    frozen,
    module = "ezdracad._core"
)]
pub(crate) struct CoordinateMeasurementHandle {
    pub(crate) inner: ezdracad_core::CoordinateMeasurementContext,
}

fn length(value: &Bound<'_, PyAny>) -> PyResult<f64> {
    if value.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(
            "raw length must be a number, not bool",
        ));
    }
    value.extract()
}

#[pymethods]
impl CoordinateMeasurementHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        row.set_item("paper_scale_offset", self.inner.paper_scale_offset)?;
        row.set_item(
            "paper_scale_bytes",
            PyBytes::new(py, &self.inner.paper_scale_bytes),
        )?;
        row.set_item("paper_scale_factor", self.inner.paper_scale_factor)?;
        row.set_item(
            "annotation_ratio_offset",
            self.inner.annotation_ratio_offset,
        )?;
        row.set_item(
            "annotation_ratio_bytes",
            PyBytes::new(py, &self.inner.annotation_ratio_bytes),
        )?;
        row.set_item("annotation_ratio", self.inner.annotation_ratio)?;
        row.set_item("drawing_denominator", self.inner.drawing_denominator)?;
        row.set_item("annotation_denominator", self.inner.annotation_denominator)?;
        Ok(row)
    }

    fn length_on_paper_mm(&self, py: Python<'_>, raw_length: &Bound<'_, PyAny>) -> PyResult<f64> {
        self.inner
            .length_on_paper_mm(length(raw_length)?)
            .map_err(|error| super::python_error(py, error))
    }

    fn length_at_current_annotation_mm(
        &self,
        py: Python<'_>,
        raw_length: &Bound<'_, PyAny>,
    ) -> PyResult<f64> {
        self.inner
            .length_at_current_annotation_mm(length(raw_length)?)
            .map_err(|error| super::python_error(py, error))
    }
}
