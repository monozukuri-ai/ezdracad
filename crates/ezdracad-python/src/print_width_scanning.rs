//! Thin explicit print-policy boundary. No native flag or layout inference.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyTuple};

type PolicyValues = (bool, bool, bool, f64, String, Option<f64>, String);

fn index(value: &Bound<'_, PyAny>) -> PyResult<u16> {
    if value.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(
            "style index must be an integer, not bool",
        ));
    }
    value.extract()
}

fn number(value: &Bound<'_, PyAny>) -> PyResult<f64> {
    if value.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(
            "print denominator must be a number, not bool",
        ));
    }
    value.extract()
}

fn policy_values(values: &Bound<'_, PyTuple>) -> PyResult<PolicyValues> {
    if values.len() != 7 {
        return Err(PyValueError::new_err("print policy requires seven values"));
    }
    let boolean = |at| {
        let value = values.get_item(at)?;
        if !value.is_instance_of::<PyBool>() {
            return Err(PyTypeError::new_err("print policy switches must be bool"));
        }
        value.extract::<bool>()
    };
    let denominator = values.get_item(5)?;
    Ok((
        boolean(0)?,
        boolean(1)?,
        boolean(2)?,
        number(&values.get_item(3)?)?,
        values.get_item(4)?.extract()?,
        if denominator.is_none() {
            None
        } else {
            Some(number(&denominator)?)
        },
        values.get_item(6)?.extract()?,
    ))
}

#[pyfunction]
pub(crate) fn resolve_nominal_print_width<'py>(
    py: Python<'py>,
    tables: PyRef<'_, super::style_scanning::StyleHandle>,
    color_index: &Bound<'py, PyAny>,
    width_index: &Bound<'py, PyAny>,
    values: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyDict>> {
    use ezdracad_core::{PrintDeviceContext, PrintOutputScale};
    let color_index = index(color_index)?;
    let width_index = index(width_index)?;
    let values = policy_values(values)?;
    let output_scale = match (values.4.as_str(), values.5) {
        ("drawing", None) => PrintOutputScale::Drawing,
        ("specified", Some(denominator)) => PrintOutputScale::Specified { denominator },
        ("automatic", resolved_denominator) => PrintOutputScale::Automatic {
            resolved_denominator,
        },
        _ => {
            return Err(PyValueError::new_err(
                "invalid print output scale/denominator",
            ))
        }
    };
    let device_context = match values.6.as_str() {
        "unspecified" => PrintDeviceContext::Unspecified,
        "application_managed_opaque_vectors" => PrintDeviceContext::ApplicationManagedOpaqueVectors,
        "driver_managed" => PrintDeviceContext::DriverManaged,
        "rasterized_transparency" => PrintDeviceContext::RasterizedTransparency,
        _ => return Err(PyValueError::new_err("unknown print device context")),
    };
    let policy = ezdracad_core::PrintWidthPolicy {
        width_by_color: values.0,
        suppress_zero_width: values.1,
        reduction_mode: values.2,
        drawing_denominator: values.3,
        output_scale,
        device_context,
    };
    let index = |index| ezdracad_core::ResolvedStyleIndex {
        index,
        sources: vec![],
    };
    let styles = ezdracad_core::EffectiveStyleIndices {
        color: index(color_index),
        line_type: index(1),
        width: index(width_index),
    };
    let value = ezdracad_core::resolve_nominal_print_width(&styles, &tables.inner, &policy)
        .map_err(|error| super::python_error(py, error))?;
    let row = PyDict::new(py);
    row.set_item("status", value.status.as_str())?;
    row.set_item("selected_table_domain", value.selected_table_domain)?;
    row.set_item("selected_index", value.selected_index)?;
    row.set_item("selected_row_offset", value.selected_row_offset)?;
    row.set_item(
        "selected_row_bytes",
        PyBytes::new(py, &value.selected_row_bytes),
    )?;
    row.set_item("pen_width_hundredths_raw", value.pen_width_hundredths_raw)?;
    row.set_item("base_pen_mm", value.base_pen_mm)?;
    row.set_item("applied_scale_factor", value.applied_scale_factor)?;
    row.set_item("nominal_paper_mm", value.nominal_paper_mm)?;
    Ok(row)
}
