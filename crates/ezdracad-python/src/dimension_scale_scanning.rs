//! One owned record-local numeric dimension ratio; arithmetic stays in Rust.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub(crate) fn row(
    py: Python<'_>,
    value: ezdracad_core::DimensionScale,
) -> PyResult<Bound<'_, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("profile", value.profile.as_str())?;
    row.set_item("record_index", value.record_index)?;
    row.set_item("payload_offset", value.payload_offset)?;
    row.set_item("tag_raw", value.tag_raw)?;
    row.set_item("contents_sha256", value.contents_sha256)?;
    row.set_item("record_bytes", PyBytes::new(py, &value.record_bytes))?;
    row.set_item("dimid", value.dimid)?;
    row.set_item("dimflag", value.dimflag)?;
    row.set_item("dimflag2", value.dimflag2)?;
    row.set_item("geometry_offset", value.geometry_offset)?;
    row.set_item("geometry_bytes", PyBytes::new(py, &value.geometry_bytes))?;
    row.set_item("start", value.start)?;
    row.set_item("end", value.end)?;
    row.set_item("stored_span_length", value.stored_span_length)?;
    row.set_item("numeric_flag_offset", value.numeric_flag_offset)?;
    row.set_item(
        "numeric_flag_bytes",
        PyBytes::new(py, &value.numeric_flag_bytes),
    )?;
    row.set_item("unit_offset", value.unit_offset)?;
    row.set_item("unit_bytes", PyBytes::new(py, &value.unit_bytes))?;
    row.set_item("number_offset", value.number_offset)?;
    row.set_item("number_bytes", PyBytes::new(py, &value.number_bytes))?;
    row.set_item("recorded_value_mm", value.recorded_value_mm)?;
    row.set_item(
        "annotation_to_drawing_ratio",
        value.annotation_to_drawing_ratio,
    )?;
    row.set_item(
        "current_drawing_scale_offset",
        value.current_drawing_scale_offset,
    )?;
    row.set_item(
        "current_drawing_scale_bytes",
        PyBytes::new(py, &value.current_drawing_scale_bytes),
    )?;
    row.set_item(
        "current_drawing_denominator",
        value.current_drawing_denominator,
    )?;
    row.set_item(
        "equivalent_annotation_denominator",
        value.equivalent_annotation_denominator,
    )?;
    Ok(row)
}
