use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

fn source_row(py: Python<'_>, value: ezdracad_core::StyleSource) -> PyResult<Bound<'_, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("kind", value.kind.as_str())?;
    row.set_item("table_domain", value.table_domain)?;
    row.set_item("index", value.index)?;
    row.set_item("definition_index", value.definition_index)?;
    row.set_item("payload_offset", value.payload_offset)?;
    row.set_item("raw_bytes", PyBytes::new(py, &value.raw_bytes))?;
    Ok(row)
}

fn index_row(
    py: Python<'_>,
    value: ezdracad_core::ResolvedStyleIndex,
) -> PyResult<Bound<'_, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("index", value.index)?;
    let sources = value
        .sources
        .into_iter()
        .map(|source| source_row(py, source).map(Bound::unbind))
        .collect::<PyResult<Vec<_>>>()?;
    row.set_item("sources", sources)?;
    Ok(row)
}

pub(crate) fn styles_row(
    py: Python<'_>,
    value: ezdracad_core::EffectiveStyleIndices,
) -> PyResult<Bound<'_, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("color", index_row(py, value.color)?)?;
    row.set_item("line_type", index_row(py, value.line_type)?)?;
    row.set_item("width", index_row(py, value.width)?)?;
    Ok(row)
}

pub(crate) fn line_style_row(
    py: Python<'_>,
    value: ezdracad_core::EffectiveLineStyle,
) -> PyResult<Bound<'_, PyDict>> {
    let row = PyDict::new(py);
    row.set_item(
        "source",
        super::record_attribute_scanning::line_row(
            py,
            ezdracad_core::AttributedLineRecord2D {
                source: value.source,
                attributes: value.attributes,
            },
        ),
    )?;
    row.set_item("styles", styles_row(py, value.styles)?)?;
    Ok(row)
}

pub(crate) fn block_style_rows(
    py: Python<'_>,
    values: Vec<ezdracad_core::EffectiveBlockLineStyle>,
) -> PyResult<Vec<Py<PyDict>>> {
    values
        .into_iter()
        .map(|value| {
            let row = line_style_row(
                py,
                ezdracad_core::EffectiveLineStyle {
                    source: value.source,
                    attributes: value.attributes,
                    styles: value.styles,
                },
            )?;
            row.set_item("reference_record_index", value.reference_record_index)?;
            row.set_item("definition_index", value.definition_index)?;
            row.set_item("start", value.start)?;
            row.set_item("end", value.end)?;
            Ok(row.unbind())
        })
        .collect()
}
