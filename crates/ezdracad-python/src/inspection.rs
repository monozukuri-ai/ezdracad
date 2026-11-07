use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::{cfb_row, limits, mps_row, python_error, LimitValues};

#[pyclass(name = "_ContainerHandle", frozen, module = "ezdracad._core")]
pub(crate) struct ContainerHandle {
    inner: ezdracad_core::ContainerInfo,
}

#[pymethods]
impl ContainerHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        row.set_item("format", self.inner.format.as_str())?;
        row.set_item("file_size", self.inner.file_size)?;
        row.set_item(
            "cfb",
            self.inner
                .cfb
                .as_ref()
                .map(|cfb| cfb_row(py, cfb))
                .transpose()?,
        )?;
        let payload = self
            .inner
            .payload
            .as_ref()
            .map(|payload| -> PyResult<_> {
                let value = PyDict::new(py);
                value.set_item("stream_path", &payload.stream_path)?;
                value.set_item("offset_domain", payload.offset_domain())?;
                value.set_item("size", payload.size)?;
                value.set_item("sha256", &payload.sha256)?;
                value.set_item("marker_raw", payload.marker_raw)?;
                Ok(value)
            })
            .transpose()?;
        row.set_item("payload", payload)?;
        row.set_item(
            "mps",
            self.inner
                .mps
                .as_ref()
                .map(|mps| mps_row(py, mps))
                .transpose()?,
        )?;
        let diagnostics: Vec<_> = self
            .inner
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message.as_str()))
            .collect();
        row.set_item("diagnostics", diagnostics)?;
        Ok(row)
    }

    fn contents<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self
            .inner
            .contents()
            .map_err(|error| python_error(py, error))?;
        Ok(PyBytes::new(py, bytes))
    }
}

#[pyfunction]
pub(crate) fn inspect_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    values: LimitValues,
) -> PyResult<ContainerHandle> {
    let limits = limits(values);
    limits
        .check_file_size(data.as_bytes().len() as u64)
        .map_err(|error| python_error(py, error))?;
    let owned = data.as_bytes().to_vec();
    let inner = py
        .detach(move || ezdracad_core::inspect(&owned, &limits))
        .map_err(|error| python_error(py, error))?;
    Ok(ContainerHandle { inner })
}

#[pyfunction]
pub(crate) fn inspect_file(
    py: Python<'_>,
    path: PathBuf,
    values: LimitValues,
) -> PyResult<ContainerHandle> {
    let limits = limits(values);
    let inner = py
        .detach(move || ezdracad_core::inspect_file(&path, &limits))
        .map_err(|error| python_error(py, error))?;
    Ok(ContainerHandle { inner })
}
