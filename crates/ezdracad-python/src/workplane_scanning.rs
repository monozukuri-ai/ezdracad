//! Owned saved-workplane handles; all point arithmetic remains in Rust.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyTuple};

#[pyclass(name = "_WorkplaneHandle", frozen, module = "ezdracad._core")]
pub(crate) struct WorkplaneHandle {
    pub(crate) inner: ezdracad_core::WorkplaneContext,
}

fn point(value: &Bound<'_, PyAny>) -> PyResult<[f64; 3]> {
    if (!value.is_instance_of::<PyTuple>() && !value.is_instance_of::<PyList>())
        || value.len()? != 3
    {
        return Err(PyTypeError::new_err(
            "point must be a three-number tuple or list",
        ));
    }
    let items = value.extract::<Vec<Bound<'_, PyAny>>>()?;
    let mut result = [0.; 3];
    for (i, value) in items.iter().enumerate() {
        if value.is_instance_of::<PyBool>() {
            return Err(PyTypeError::new_err("point coordinate must not be bool"));
        }
        result[i] = value.extract()?;
    }
    Ok(result)
}

#[pymethods]
impl WorkplaneHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        let evidence = self.inner.evidence();
        row.set_item("profile", self.inner.profile().as_str())?;
        row.set_item("direction", self.inner.direction().as_str())?;
        row.set_item("direction_raw", self.inner.direction_raw())?;
        row.set_item("origin", self.inner.origin())?;
        row.set_item("stored_axes", self.inner.stored_axes())?;
        row.set_item("effective_axes", self.inner.effective_axes())?;
        row.set_item("basis_source", self.inner.basis_source().as_str())?;
        row.set_item("units", self.inner.units())?;
        row.set_item("contents_sha256", &evidence.contents_sha256)?;
        row.set_item("observed_profile", evidence.observed_profile.as_str())?;
        row.set_item("header_offset", evidence.header_offset)?;
        row.set_item("header_bytes", PyBytes::new(py, &evidence.header_bytes))?;
        row.set_item("origin_offset", 0x450u64)?;
        row.set_item(
            "origin_bytes",
            PyBytes::new(py, &evidence.header_bytes[..24]),
        )?;
        row.set_item("direction_offset", 0x480u64)?;
        row.set_item(
            "direction_bytes",
            PyBytes::new(py, &evidence.header_bytes[0x30..0x34]),
        )?;
        row.set_item("stored_axes_offset", 0x488u64)?;
        row.set_item(
            "stored_axes_bytes",
            PyBytes::new(py, &evidence.header_bytes[0x38..0x80]),
        )?;
        row.set_item(
            "guard_header_offset",
            evidence.coordinate_guard.header_offset,
        )?;
        row.set_item(
            "guard_header_bytes",
            PyBytes::new(py, &evidence.coordinate_guard.header_bytes),
        )?;
        Ok(row)
    }

    // This is a directional map under an owned frame, not a constructor.
    #[allow(clippy::wrong_self_convention)]
    fn from_world_stored_xyz(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<[f64; 3]> {
        self.inner
            .from_world_stored_xyz(point(value)?)
            .map_err(|error| super::python_error(py, error))
    }

    fn to_world_stored_xyz(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<[f64; 3]> {
        self.inner
            .to_world_stored_xyz(point(value)?)
            .map_err(|error| super::python_error(py, error))
    }
}
