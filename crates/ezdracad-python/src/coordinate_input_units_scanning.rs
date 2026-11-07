//! Owned input-unit metadata and native scalar conversions; no geometry units.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt};

#[pyclass(
    name = "_NativeCoordinateInputUnitHandle",
    frozen,
    module = "ezdracad._core"
)]
pub(crate) struct NativeCoordinateInputUnitHandle {
    pub(crate) inner: ezdracad_core::NativeCoordinateInputUnitInfo,
}

fn scalar(value: &Bound<'_, PyAny>) -> PyResult<f64> {
    if value.is_instance_of::<PyBool>()
        || !(value.is_instance_of::<PyFloat>() || value.is_instance_of::<PyInt>())
    {
        return Err(PyTypeError::new_err(
            "native coordinate input scalar must be an int or float, not bool",
        ));
    }
    value.extract()
}

#[pymethods]
impl NativeCoordinateInputUnitHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        let source = self.inner.origin_units();
        row.set_item("origin_units_offset", source.payload_offset)?;
        row.set_item(
            "origin_units_bytes",
            PyBytes::new(py, &source.selector_bytes),
        )?;
        row.set_item("origin_units_code", source.raw_code)?;
        row.set_item("unit", self.inner.unit().label())?;
        row.set_item("native_display_factor", self.inner.native_display_factor())?;
        Ok(row)
    }

    fn to_native_mm(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<f64> {
        self.inner
            .to_native_mm(scalar(value)?)
            .map_err(|e| super::python_error(py, e))
    }

    // Directional conversion under this owned unit snapshot, not a constructor.
    #[allow(clippy::wrong_self_convention)]
    fn from_native_mm(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<f64> {
        self.inner
            .from_native_mm(scalar(value)?)
            .map_err(|e| super::python_error(py, e))
    }
}
