//! Thin owned style-table lookups; no effective drawing attributes.

use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

#[pyclass(name = "_StyleHandle", frozen, module = "ezdracad._core")]
pub(crate) struct StyleHandle {
    pub(crate) inner: ezdracad_core::RawStyleTables,
}

fn one_based(index: u16) -> PyResult<usize> {
    index
        .checked_sub(1)
        .map(usize::from)
        .ok_or_else(|| PyIndexError::new_err("style index is out of range"))
}

#[pymethods]
impl StyleHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        row.set_item("layer_count", self.inner.layers.len())?;
        row.set_item("color_count", self.inner.colors.len())?;
        row.set_item("line_type_count", self.inner.line_types.len())?;
        row.set_item("width_count", self.inner.widths.len())?;
        row.set_item("shared_suffix_offset", self.inner.shared_suffix_offset)?;
        row.set_item("shared_suffix_size", self.inner.shared_suffix_bytes.len())?;
        row.set_item(
            "extension_header_offsets",
            self.inner.extension_header_offsets,
        )?;
        let headers: Vec<_> = self
            .inner
            .extension_header_bytes
            .iter()
            .map(|bytes| PyBytes::new(py, bytes).unbind())
            .collect();
        row.set_item("extension_header_bytes", headers)?;
        Ok(row)
    }

    fn suffix_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.shared_suffix_bytes)
    }

    fn layer<'py>(&self, py: Python<'py>, index: u16) -> PyResult<Bound<'py, PyDict>> {
        let entry = self
            .inner
            .layers
            .get(usize::from(index))
            .ok_or_else(|| PyIndexError::new_err("layer style index is out of range"))?;
        let row = PyDict::new(py);
        row.set_item("index", entry.index)?;
        row.set_item("payload_offset", entry.payload_offset)?;
        row.set_item("raw_bytes", PyBytes::new(py, &entry.raw_bytes))?;
        row.set_item("flags_raw", entry.flags_raw)?;
        row.set_item("reserved_word_raw", entry.reserved_word_raw)?;
        row.set_item("color_index_raw", entry.color_index_raw)?;
        row.set_item("line_type_index_raw", entry.line_type_index_raw)?;
        row.set_item("width_index_raw", entry.width_index_raw)?;
        row.set_item("stored_index_raw", entry.stored_index_raw)?;
        Ok(row)
    }

    fn color<'py>(&self, py: Python<'py>, index: u16) -> PyResult<Bound<'py, PyDict>> {
        let entry = self
            .inner
            .colors
            .get(one_based(index)?)
            .ok_or_else(|| PyIndexError::new_err("color style index is out of range"))?;
        let row = PyDict::new(py);
        row.set_item("index", entry.index)?;
        row.set_item("payload_offset", entry.payload_offset)?;
        row.set_item("raw_bytes", PyBytes::new(py, &entry.raw_bytes))?;
        row.set_item("flags_raw", entry.flags_raw)?;
        row.set_item("pen_width_hundredths_raw", entry.pen_width_hundredths_raw)?;
        row.set_item("pen_width_value", entry.pen_width_value)?;
        row.set_item("rgb", entry.rgb)?;
        row.set_item("rgb_encoding_raw", entry.rgb_encoding_raw)?;
        row.set_item("output_rgb", entry.output_rgb)?;
        row.set_item("output_rgb_encoding_raw", entry.output_rgb_encoding_raw)?;
        Ok(row)
    }

    fn line_type<'py>(&self, py: Python<'py>, index: u16) -> PyResult<Bound<'py, PyDict>> {
        let entry = self
            .inner
            .line_types
            .get(one_based(index)?)
            .ok_or_else(|| PyIndexError::new_err("line-type style index is out of range"))?;
        let row = PyDict::new(py);
        row.set_item("index", entry.index)?;
        row.set_item("payload_offset", entry.payload_offset)?;
        row.set_item("raw_bytes", PyBytes::new(py, &entry.raw_bytes))?;
        row.set_item("flags_raw", entry.flags_raw)?;
        row.set_item("pen_style_raw", entry.pen_style_raw)?;
        row.set_item(
            "pen_definition_hundredths_raw",
            entry.pen_definition_hundredths_raw,
        )?;
        row.set_item("pen_definition_values", entry.pen_definition_values)?;
        Ok(row)
    }

    fn width<'py>(&self, py: Python<'py>, index: u16) -> PyResult<Bound<'py, PyDict>> {
        let entry = self
            .inner
            .widths
            .get(one_based(index)?)
            .ok_or_else(|| PyIndexError::new_err("width style index is out of range"))?;
        let row = PyDict::new(py);
        row.set_item("index", entry.index)?;
        row.set_item("payload_offset", entry.payload_offset)?;
        row.set_item("raw_bytes", PyBytes::new(py, &entry.raw_bytes))?;
        row.set_item("flags_raw", entry.flags_raw)?;
        row.set_item("pen_width_hundredths_raw", entry.pen_width_hundredths_raw)?;
        row.set_item("pen_width_value", entry.pen_width_value)?;
        Ok(row)
    }
}
