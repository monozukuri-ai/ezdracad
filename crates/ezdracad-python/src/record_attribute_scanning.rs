use pyo3::prelude::*;
use pyo3::types::PyBytes;

type ReferenceRow = (&'static str, Option<u16>, u8);
pub(crate) type AttributeRow = (
    u16,
    Py<PyBytes>,
    u16,
    ReferenceRow,
    ReferenceRow,
    ReferenceRow,
    u8,
    u16,
    u8,
    Py<PyBytes>,
);
type SourceRow = (u64, u64, u16, Py<PyBytes>, [f64; 2], [f64; 2], f64, f64);
pub(crate) type AttributedLineRow = (SourceRow, AttributeRow);

fn reference_row(value: ezdracad_core::AttributeReference) -> ReferenceRow {
    (value.kind.as_str(), value.index, value.raw_code)
}

pub(crate) fn attribute_row(
    py: Python<'_>,
    value: ezdracad_core::RecordAttributes,
) -> AttributeRow {
    (
        value.tag_raw,
        PyBytes::new(py, &value.header_bytes).unbind(),
        value.layer_index,
        reference_row(value.color),
        reference_row(value.line_type),
        reference_row(value.width),
        value.material_index_raw,
        value.group_index,
        value.flags_raw,
        PyBytes::new(py, &value.opaque_tail_bytes).unbind(),
    )
}

pub(crate) fn line_row(
    py: Python<'_>,
    value: ezdracad_core::AttributedLineRecord2D,
) -> AttributedLineRow {
    let source = value.source;
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
        attribute_row(py, value.attributes),
    )
}

#[pyfunction]
pub(crate) fn decode_record_attributes(
    py: Python<'_>,
    header: &Bound<'_, PyBytes>,
) -> PyResult<AttributeRow> {
    let attributes = ezdracad_core::decode_record_attributes(header.as_bytes())
        .map_err(|error| super::python_error(py, error))?;
    Ok(attribute_row(py, attributes))
}
