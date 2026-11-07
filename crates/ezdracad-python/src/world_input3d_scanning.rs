//! Owned loaded-file world-input 3D context; all arithmetic remains in Rust.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyTuple};

#[pyclass(name = "_WorldInput3DHandle", frozen, module = "ezdracad._core")]
pub(crate) struct WorldInput3DHandle {
    pub(crate) inner: ezdracad_core::WorldInput3DContext,
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
    for (index, value) in items.iter().enumerate() {
        if value.is_instance_of::<PyBool>() {
            return Err(PyTypeError::new_err("point coordinate must not be bool"));
        }
        result[index] = value.extract()?;
    }
    Ok(result)
}

#[pymethods]
impl WorldInput3DHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        row.set_item("profile", self.inner.profile().as_str())?;
        row.set_item("loaded_world_origin", self.inner.loaded_world_origin())?;
        row.set_item("origin_z_source", self.inner.origin_z_source())?;
        row.set_item("draw_origin_offset", self.inner.draw_origin_offset())?;
        row.set_item(
            "draw_origin_bytes",
            PyBytes::new(py, &self.inner.draw_origin_bytes()),
        )?;
        row.set_item("draw_origin_raw", self.inner.draw_origin_raw())?;
        row.set_item("units", self.inner.units())?;
        row.set_item("coordinate_semantics", self.inner.coordinate_semantics())?;
        Ok(row)
    }

    fn coordinate_context(&self) -> super::coordinate_space_scanning::CoordinateSpaceHandle {
        super::coordinate_space_scanning::CoordinateSpaceHandle {
            inner: self.inner.coordinate_context().clone(),
        }
    }

    fn input_mm_from_world_stored_xyz(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<[f64; 3]> {
        self.inner
            .input_mm_from_world_stored_xyz(point(value)?)
            .map_err(|error| super::python_error(py, error))
    }

    fn world_stored_xyz_from_input_mm(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<[f64; 3]> {
        self.inner
            .world_stored_xyz_from_input_mm(point(value)?)
            .map_err(|error| super::python_error(py, error))
    }
}
