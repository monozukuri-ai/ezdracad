use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::python_error;

type LineRow = (u64, u64, u16, Py<PyBytes>, [f64; 2], [f64; 2], f64, f64);
type PlacedRow = (u64, u64, LineRow, [f64; 2], [f64; 2]);

fn line_row(py: Python<'_>, line: ezdracad_core::LineRecord2D) -> LineRow {
    (
        line.record_index,
        line.payload_offset,
        line.tag_raw,
        PyBytes::new(py, &line.header_bytes).unbind(),
        line.start,
        line.end,
        line.elevation,
        line.thickness,
    )
}

#[pyclass(name = "_BlockHandle", frozen, module = "ezdracad._core")]
pub(crate) struct BlockHandle {
    pub(crate) inner: ezdracad_core::BlockLibrary,
}

#[pymethods]
impl BlockHandle {
    fn effective_line_styles(
        &self,
        py: Python<'_>,
        reference_record_index: u64,
        max_lines: u64,
    ) -> PyResult<Vec<Py<PyDict>>> {
        let values = py
            .detach(|| {
                self.inner
                    .effective_line_styles(reference_record_index, max_lines)
            })
            .map_err(|error| python_error(py, error))?;
        super::effective_style_scanning::block_style_rows(py, values)
    }

    fn inline_package_lines(
        &self,
        py: Python<'_>,
        definition: u64,
        index: u64,
        max_depth: u64,
        max_nodes: u64,
        max_lines: u64,
    ) -> PyResult<Vec<super::inline_scanning::InlineLineRow>> {
        let limits = ezdracad_core::InlinePackageLimits {
            max_depth,
            max_nodes,
            max_lines,
        };
        let lines = py
            .detach(|| {
                self.inner
                    .inline_package_lines_with_limits(definition, index, &limits)
            })
            .map_err(|error| python_error(py, error))?;
        Ok(super::inline_scanning::rows(py, lines))
    }

    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let result = PyDict::new(py);
        let mut definitions = Vec::new();
        for definition in self.inner.definitions() {
            let row = PyDict::new(py);
            row.set_item("index", definition.index)?;
            row.set_item("header_offset", definition.header_offset)?;
            row.set_item("header_bytes", PyBytes::new(py, &definition.header_bytes))?;
            row.set_item("name_bytes", PyBytes::new(py, &definition.name_bytes))?;
            row.set_item("record_start", definition.record_start)?;
            row.set_item("record_end", definition.record_end)?;
            row.set_item("record_count", definition.record_count)?;
            row.set_item(
                "declared_nonterminal_records",
                definition.declared_nonterminal_records,
            )?;
            definitions.push(row);
        }
        result.set_item("definitions", definitions)?;
        result.set_item("suffix_offset", self.inner.suffix_offset())?;
        result.set_item(
            "suffix_size",
            self.inner
                .suffix_bytes()
                .map_err(|e| python_error(py, e))?
                .len(),
        )?;
        Ok(result)
    }

    fn suffix_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new(
            py,
            self.inner.suffix_bytes().map_err(|e| python_error(py, e))?,
        ))
    }

    fn resolve(&self, py: Python<'_>, name: &Bound<'_, PyBytes>) -> PyResult<u64> {
        self.inner
            .resolve(name.as_bytes())
            .map(|d| d.index)
            .map_err(|e| python_error(py, e))
    }

    fn record_bytes<'py>(
        &self,
        py: Python<'py>,
        definition: u64,
        index: u64,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let record = self
            .inner
            .record(definition, index)
            .ok_or_else(|| PyIndexError::new_err("definition or record index out of range"))?;
        Ok(PyBytes::new(py, record.bytes()))
    }

    fn records(
        &self,
        py: Python<'_>,
        definition: u64,
        start: u64,
        count: u64,
    ) -> PyResult<Vec<(u64, u64, u16)>> {
        if count > ezdracad_core::MAX_RECORDS_PER_PAGE {
            return Err(python_error(
                py,
                ezdracad_core::DracadError::LimitExceeded {
                    resource: "records_per_page",
                    limit: ezdracad_core::MAX_RECORDS_PER_PAGE,
                    actual: count,
                },
            ));
        }
        let definition_metadata = usize::try_from(definition)
            .ok()
            .and_then(|i| self.inner.definitions().get(i))
            .ok_or_else(|| PyIndexError::new_err("definition index out of range"))?;
        if start > definition_metadata.record_count {
            return Err(PyIndexError::new_err(
                "record start exceeds definition record count",
            ));
        }
        Ok((start
            ..start
                .saturating_add(count)
                .min(definition_metadata.record_count))
            .filter_map(|index| self.inner.record(definition, index))
            .map(|record| (record.index, record.payload_offset, record.tag))
            .collect())
    }

    fn planar_lines(
        &self,
        py: Python<'_>,
        definition: u64,
        max_lines: u64,
    ) -> PyResult<Vec<LineRow>> {
        let lines = py
            .detach(|| {
                self.inner
                    .planar_line_records_with_limit(definition, max_lines)
            })
            .map_err(|e| python_error(py, e))?;
        Ok(lines.into_iter().map(|line| line_row(py, line)).collect())
    }

    fn placed_lines(
        &self,
        py: Python<'_>,
        reference: u64,
        max_lines: u64,
    ) -> PyResult<Vec<PlacedRow>> {
        let lines = py
            .detach(|| {
                self.inner
                    .placed_line_records_with_limit(reference, max_lines)
            })
            .map_err(|e| python_error(py, e))?;
        Ok(lines
            .into_iter()
            .map(|line| {
                (
                    line.reference_record_index,
                    line.definition_index,
                    line_row(py, line.source),
                    line.start,
                    line.end,
                )
            })
            .collect())
    }
}
