//! Owned named-frame contexts; point arithmetic and admission stay in Rust.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyTuple};

#[pyclass(name = "_CoordinateSpaceHandle", frozen, module = "ezdracad._core")]
pub(crate) struct CoordinateSpaceHandle {
    pub(crate) inner: ezdracad_core::CoordinateSpaceContext,
}

fn point(value: &Bound<'_, PyAny>) -> PyResult<[f64; 2]> {
    if !value.is_instance_of::<PyTuple>() && !value.is_instance_of::<PyList>() {
        return Err(PyTypeError::new_err(
            "point must be a two-number tuple or list",
        ));
    }
    if value.len()? != 2 {
        return Err(PyTypeError::new_err(
            "point must contain exactly two numbers",
        ));
    }
    let items = value.extract::<Vec<Bound<'_, PyAny>>>()?;
    let mut result = [0.0; 2];
    for (i, value) in items.iter().enumerate() {
        if value.is_instance_of::<PyBool>() {
            return Err(PyTypeError::new_err("point coordinate must not be bool"));
        }
        result[i] = value.extract()?;
    }
    Ok(result)
}

#[pymethods]
impl CoordinateSpaceHandle {
    fn input_unit(
        &self,
    ) -> super::coordinate_input_units_scanning::NativeCoordinateInputUnitHandle {
        super::coordinate_input_units_scanning::NativeCoordinateInputUnitHandle {
            inner: *self.inner.input_unit(),
        }
    }

    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        let evidence = self.inner.evidence();
        let measurement = self.inner.measurement();
        let draft = self.inner.draft();
        row.set_item("profile", self.inner.profile().as_str())?;
        row.set_item("contents_sha256", &evidence.contents_sha256)?;
        row.set_item("observed_profile", evidence.observed_profile.as_str())?;
        row.set_item("header_offset", evidence.header_offset)?;
        row.set_item("header_bytes", PyBytes::new(py, &evidence.header_bytes))?;
        row.set_item("drawing_origin_offset", 0x140u64)?;
        row.set_item("drawing_origin", self.inner.drawing_origin())?;
        row.set_item(
            "drawing_origin_bytes",
            PyBytes::new(py, &evidence.header_bytes[..16]),
        )?;
        row.set_item("origin_units_offset", 0x150u64)?;
        row.set_item("origin_units_raw", self.inner.origin_units_raw())?;
        row.set_item(
            "origin_units_bytes",
            PyBytes::new(py, &evidence.header_bytes[16..20]),
        )?;
        row.set_item("paper_scale_offset", measurement.paper_scale_offset)?;
        row.set_item(
            "paper_scale_bytes",
            PyBytes::new(py, &measurement.paper_scale_bytes),
        )?;
        row.set_item("paper_scale_factor", measurement.paper_scale_factor)?;
        row.set_item(
            "annotation_ratio_offset",
            measurement.annotation_ratio_offset,
        )?;
        row.set_item(
            "annotation_ratio_bytes",
            PyBytes::new(py, &measurement.annotation_ratio_bytes),
        )?;
        row.set_item("annotation_ratio", measurement.annotation_ratio)?;
        row.set_item("drawing_denominator", measurement.drawing_denominator)?;
        row.set_item("annotation_denominator", measurement.annotation_denominator)?;
        row.set_item("draft_angle_offset", draft.angle_offset)?;
        row.set_item("draft_angle_bytes", PyBytes::new(py, &draft.angle_bytes))?;
        row.set_item("draft_angle_degrees", draft.angle_degrees)?;
        row.set_item("draft_object_offset", draft.object_offset)?;
        row.set_item("draft_object_bytes", PyBytes::new(py, &draft.object_bytes))?;
        row.set_item("draft_object_raw", draft.object_raw)?;
        row.set_item("draft_enable_offset", draft.enable_offset)?;
        row.set_item("draft_enable_bytes", PyBytes::new(py, &draft.enable_bytes))?;
        row.set_item("draft_enable_raw", draft.enable_raw)?;
        Ok(row)
    }

    // Directional conversion under this owned coordinate context, not a constructor.
    #[allow(clippy::wrong_self_convention)]
    fn from_root_stored_xy(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        frame: &str,
    ) -> PyResult<[f64; 2]> {
        let frame = ezdracad_core::CoordinateFrame::from_id(frame)
            .map_err(|error| super::python_error(py, error))?;
        self.inner
            .from_root_stored_xy(point(value)?, frame)
            .map_err(|error| super::python_error(py, error))
    }

    fn to_root_stored_xy(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        frame: &str,
    ) -> PyResult<[f64; 2]> {
        let frame = ezdracad_core::CoordinateFrame::from_id(frame)
            .map_err(|error| super::python_error(py, error))?;
        self.inner
            .to_root_stored_xy(point(value)?, frame)
            .map_err(|error| super::python_error(py, error))
    }
}

pub(crate) fn selection_row(
    py: Python<'_>,
    selection: ezdracad_core::FramedLineSelection,
) -> PyResult<Bound<'_, PyDict>> {
    let (source, context, frame, coordinates) = selection.into_parts();
    let row = PyDict::new(py);
    row.set_item(
        "source",
        super::qualification_scanning::selection_row(py, source)?,
    )?;
    row.set_item(
        "context",
        Py::new(py, CoordinateSpaceHandle { inner: context })?,
    )?;
    row.set_item("frame", frame.as_str())?;
    row.set_item("units", frame.units())?;
    row.set_item("document_complete", false)?;
    let points = PyList::empty(py);
    for coordinate in coordinates {
        let point = PyDict::new(py);
        point.set_item("start", coordinate.start)?;
        point.set_item("end", coordinate.end)?;
        points.append(point)?;
    }
    row.set_item("coordinates", points)?;
    Ok(row)
}
