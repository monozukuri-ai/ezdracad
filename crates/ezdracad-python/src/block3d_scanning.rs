//! Thin owned-reference boundary for atomic direct 3D block selections.
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyTuple};

#[pyclass(name = "_BlockReference3DHandle", frozen, module = "ezdracad._core")]
pub(crate) struct BlockReference3DHandle {
    pub(crate) inner: ezdracad_core::BlockReference3D,
}

#[pymethods]
impl BlockReference3DHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        let evidence = self.inner.evidence();
        row.set_item("profile", self.inner.profile().as_str())?;
        row.set_item(
            "name_field_bytes",
            PyBytes::new(py, self.inner.name_field_bytes()),
        )?;
        row.set_item("name_key", PyBytes::new(py, self.inner.name_key()))?;
        row.set_item("translation", self.inner.translation())?;
        row.set_item("scale", self.inner.scale())?;
        row.set_item("rotation_degrees", self.inner.rotation_degrees())?;
        row.set_item("contents_sha256", &evidence.contents_sha256)?;
        row.set_item("record_indices", evidence.record_indices)?;
        row.set_item("payload_offsets", evidence.payload_offsets)?;
        row.set_item(
            "tag_raw",
            u16::from_le_bytes(evidence.records[0][..2].try_into().unwrap()),
        )?;
        let records = PyList::empty(py);
        for record in &evidence.records {
            records.append(PyBytes::new(py, record))?;
        }
        row.set_item("records", records)?;
        row.set_item("units", self.inner.units())?;
        row.set_item("record_scope", "root_records")?;
        Ok(row)
    }

    fn apply_xyz(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<[f64; 3]> {
        if (!value.is_instance_of::<PyTuple>() && !value.is_instance_of::<PyList>())
            || value.len()? != 3
        {
            return Err(PyTypeError::new_err(
                "point must be a three-number tuple or list",
            ));
        }
        let items = value.extract::<Vec<Bound<'_, PyAny>>>()?;
        let mut point = [0.; 3];
        for (i, item) in items.iter().enumerate() {
            if item.is_instance_of::<PyBool>() {
                return Err(PyTypeError::new_err("point coordinate must not be bool"));
            }
            point[i] = item.extract()?;
        }
        self.inner
            .apply_xyz(point)
            .map_err(|error| super::python_error(py, error))
    }
}

pub(crate) fn selection_row(
    py: Python<'_>,
    selection: ezdracad_core::BlockLineSelection3D,
) -> PyResult<Bound<'_, PyDict>> {
    let flag_raw = selection.definition_flag_raw();
    let saved_bounds = selection.saved_bounds();
    let (profile, digest, reference, definition, lines) = selection.into_parts();
    let row = PyDict::new(py);
    row.set_item("profile", profile.as_str())?;
    row.set_item("contents_sha256", digest)?;
    row.set_item(
        "reference",
        Py::new(py, BlockReference3DHandle { inner: reference })?,
    )?;
    let source = PyDict::new(py);
    source.set_item("index", definition.index)?;
    source.set_item("header_offset", definition.header_offset)?;
    source.set_item("header_bytes", PyBytes::new(py, &definition.header_bytes))?;
    source.set_item("name_bytes", PyBytes::new(py, &definition.name_bytes))?;
    source.set_item("record_start", definition.record_start)?;
    source.set_item("record_end", definition.record_end)?;
    source.set_item(
        "declared_nonterminal_records",
        definition.declared_nonterminal_records,
    )?;
    source.set_item("record_count", definition.record_count)?;
    source.set_item("flag_raw", flag_raw)?;
    source.set_item("saved_bounds", saved_bounds)?;
    row.set_item("definition", source)?;
    let output = PyList::empty(py);
    for line in lines {
        let item = PyDict::new(py);
        item.set_item("definition_index", line.definition_index)?;
        item.set_item("source", super::line3d_scanning::row(py, line.source))?;
        item.set_item("start", line.start)?;
        item.set_item("end", line.end)?;
        item.set_item("coordinate_space", line.coordinate_space())?;
        item.set_item("units", line.units())?;
        output.append(item)?;
    }
    row.set_item("lines", output)?;
    row.set_item("document_complete", false)?;
    row.set_item("units", py.None())?;
    Ok(row)
}
