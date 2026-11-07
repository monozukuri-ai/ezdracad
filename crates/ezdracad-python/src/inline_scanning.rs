use pyo3::prelude::*;
use pyo3::types::PyBytes;

type LineRow = (u64, u64, u16, Py<PyBytes>, [f64; 2], [f64; 2], f64, f64);
type AncestorRow = (u64, u64, u64, Py<PyBytes>, Py<PyBytes>);
pub(crate) type InlineLineRow = (LineRow, Vec<AncestorRow>, &'static str, Option<u64>);

pub(crate) fn rows(
    py: Python<'_>,
    lines: Vec<ezdracad_core::ScopedInlineLine2D>,
) -> Vec<InlineLineRow> {
    lines
        .into_iter()
        .map(|line| {
            let source = line.source;
            (
                (
                    source.record_index,
                    source.payload_offset,
                    source.tag_raw,
                    PyBytes::new(py, &source.header_bytes).unbind(),
                    source.start,
                    source.end,
                    source.elevation,
                    source.thickness,
                ),
                line.ancestors
                    .into_iter()
                    .map(|ancestor| {
                        (
                            ancestor.record_index,
                            ancestor.payload_offset,
                            ancestor.record_count,
                            PyBytes::new(py, &ancestor.header_bytes).unbind(),
                            PyBytes::new(py, &ancestor.body_bytes).unbind(),
                        )
                    })
                    .collect(),
                line.table_domain.as_str(),
                line.definition_index,
            )
        })
        .collect()
}
