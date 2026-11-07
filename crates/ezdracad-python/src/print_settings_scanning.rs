//! Owned native print-policy fields and checked typed-policy conversion.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict};

#[pyclass(name = "_PrintSettingsHandle", frozen, module = "ezdracad._core")]
pub(crate) struct PrintSettingsHandle {
    pub(crate) inner: ezdracad_core::RawPrintSettings,
}

#[pymethods]
impl PrintSettingsHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        row.set_item("marker_raw", 25)?;
        row.set_item(
            "drawing_paper_scale_factor",
            self.inner.drawing_paper_scale_factor(),
        )?;
        row.set_item("printer_settings_raw", self.inner.printer_settings_raw())?;
        row.set_item("output_scale_mode_raw", self.inner.output_scale_mode_raw())?;
        row.set_item("width_by_color_raw", self.inner.width_by_color_raw())?;
        row.set_item("print_scale_factor", self.inner.print_scale_factor())?;
        row.set_item("reduction_mode_raw", self.inner.reduction_mode_raw())?;
        row.set_item(
            "suppress_zero_width_raw",
            self.inner.suppress_zero_width_raw(),
        )?;
        let fields = self
            .inner
            .fields()
            .iter()
            .map(|field| {
                let f = PyDict::new(py);
                f.set_item("name", field.name)?;
                f.set_item("payload_offset", field.payload_offset)?;
                f.set_item("raw_bytes", PyBytes::new(py, &field.raw_bytes))?;
                Ok(f.unbind())
            })
            .collect::<PyResult<Vec<_>>>()?;
        row.set_item("fields", fields)?;
        Ok(row)
    }

    #[pyo3(signature = (device_context, automatic_output_denominator=None))]
    fn width_policy<'py>(
        &self,
        py: Python<'py>,
        device_context: &str,
        automatic_output_denominator: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        use ezdracad_core::{PrintDeviceContext, PrintOutputScale};
        let context = match device_context {
            "unspecified" => PrintDeviceContext::Unspecified,
            "application_managed_opaque_vectors" => {
                PrintDeviceContext::ApplicationManagedOpaqueVectors
            }
            "driver_managed" => PrintDeviceContext::DriverManaged,
            "rasterized_transparency" => PrintDeviceContext::RasterizedTransparency,
            _ => return Err(PyValueError::new_err("unknown print device context")),
        };
        let denominator = automatic_output_denominator
            .map(|value| {
                if value.is_instance_of::<PyBool>() {
                    return Err(PyTypeError::new_err(
                        "automatic output denominator must be a number, not bool",
                    ));
                }
                value.extract::<f64>()
            })
            .transpose()?;
        let p = self
            .inner
            .width_policy(context, denominator)
            .map_err(|error| super::python_error(py, error))?;
        let row = PyDict::new(py);
        row.set_item("drawing_denominator", p.drawing_denominator)?;
        row.set_item("width_by_color", p.width_by_color)?;
        row.set_item("suppress_zero_width", p.suppress_zero_width)?;
        row.set_item("reduction_mode", p.reduction_mode)?;
        let (mode, denominator) = match p.output_scale {
            PrintOutputScale::Drawing => ("drawing", None),
            PrintOutputScale::Specified { denominator } => ("specified", Some(denominator)),
            PrintOutputScale::Automatic {
                resolved_denominator,
            } => ("automatic", resolved_denominator),
        };
        row.set_item("output_scale", mode)?;
        row.set_item("output_denominator", denominator)?;
        row.set_item(
            "device_context",
            match p.device_context {
                PrintDeviceContext::Unspecified => "unspecified",
                PrintDeviceContext::ApplicationManagedOpaqueVectors => {
                    "application_managed_opaque_vectors"
                }
                PrintDeviceContext::DriverManaged => "driver_managed",
                PrintDeviceContext::RasterizedTransparency => "rasterized_transparency",
            },
        )?;
        Ok(row)
    }
}
