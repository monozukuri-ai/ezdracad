use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub(crate) fn rows(
    py: Python<'_>,
    values: Vec<ezdracad_core::SceneLine2D>,
) -> PyResult<Vec<Py<PyDict>>> {
    values
        .into_iter()
        .map(|value| {
            let row = super::effective_style_scanning::line_style_row(
                py,
                ezdracad_core::EffectiveLineStyle {
                    source: value.source,
                    attributes: value.attributes,
                    styles: value.styles,
                },
            )?;
            row.set_item("table_domain", value.table_domain.as_str())?;
            row.set_item("definition_index", value.definition_index)?;
            row.set_item("cumulative_transform", value.cumulative_transform)?;
            row.set_item("start", value.start)?;
            row.set_item("end", value.end)?;
            row.set_item("coordinate_scope", value.coordinate_scope.as_str())?;
            row.set_item("style_layer_index", value.style_layer_index)?;
            let ancestors = value
                .ancestors
                .into_iter()
                .map(|ancestor| {
                    let item = PyDict::new(py);
                    item.set_item("kind", ancestor.kind.as_str())?;
                    item.set_item("table_domain", ancestor.table_domain.as_str())?;
                    item.set_item("definition_index", ancestor.definition_index)?;
                    item.set_item("record_index", ancestor.record_index)?;
                    item.set_item("payload_offset", ancestor.payload_offset)?;
                    item.set_item("raw_bytes", PyBytes::new(py, &ancestor.raw_bytes))?;
                    if let Some(definition) = ancestor.target_definition {
                        let target = PyDict::new(py);
                        target.set_item("index", definition.index)?;
                        target.set_item("payload_offset", definition.payload_offset)?;
                        target
                            .set_item("header_bytes", PyBytes::new(py, &definition.header_bytes))?;
                        item.set_item("target_definition", target)?;
                    } else {
                        item.set_item("target_definition", py.None())?;
                    }
                    Ok(item.unbind())
                })
                .collect::<PyResult<Vec<_>>>()?;
            row.set_item("ancestors", ancestors)?;
            Ok(row.unbind())
        })
        .collect()
}
