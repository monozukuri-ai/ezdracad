//! Owned resolution snapshots and Rust-only externally declared conversions.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt};

#[pyclass(name = "_UnitResolutionHandle", frozen, module = "ezdracad._core")]
pub(crate) struct UnitResolutionHandle {
    pub(crate) inner: ezdracad_core::UnitResolution,
}

#[pymethods]
impl UnitResolutionHandle {
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        match self.inner.physical_units {
            ezdracad_core::PhysicalUnitResolution::Unresolved(
                ezdracad_core::PhysicalUnitUnresolvedReason::StoredCoordinateUnitNotEstablished,
            ) => {
                row.set_item("physical_unit_status", "unresolved")?;
                row.set_item(
                    "physical_unit_reason",
                    "stored_coordinate_unit_not_established",
                )?;
            }
        }
        match self.inner.original_creation_scale {
            ezdracad_core::CreationScaleResolution::UnavailableFromDecodedFields => {
                row.set_item("original_creation_scale", "unavailable_from_decoded_fields")?;
            }
        }
        row.set_item(
            "origin_units_offset",
            self.inner.origin_units.payload_offset,
        )?;
        row.set_item(
            "origin_units_bytes",
            PyBytes::new(py, &self.inner.origin_units.selector_bytes),
        )?;
        row.set_item("origin_units_code", self.inner.origin_units.raw_code)?;
        Ok(row)
    }

    fn current_measurement(
        &self,
    ) -> super::coordinate_measurement_scanning::CoordinateMeasurementHandle {
        super::coordinate_measurement_scanning::CoordinateMeasurementHandle {
            inner: self.inner.current_measurement,
        }
    }

    fn coordinate_input_unit(
        &self,
    ) -> super::coordinate_input_units_scanning::NativeCoordinateInputUnitHandle {
        super::coordinate_input_units_scanning::NativeCoordinateInputUnitHandle {
            inner: self.inner.coordinate_input_unit,
        }
    }
}

fn number(value: &Bound<'_, PyAny>, label: &str) -> PyResult<f64> {
    if value.is_instance_of::<PyBool>()
        || !(value.is_instance_of::<PyFloat>() || value.is_instance_of::<PyInt>())
    {
        return Err(PyTypeError::new_err(format!(
            "{label} must be an int or float, not bool"
        )));
    }
    value.extract()
}

#[pyclass(
    name = "_DeclaredCreationScaleHandle",
    frozen,
    module = "ezdracad._core"
)]
pub(crate) struct DeclaredCreationScaleHandle {
    inner: ezdracad_core::DeclaredCreationScale,
}

#[pymethods]
impl DeclaredCreationScaleHandle {
    #[new]
    fn new(
        py: Python<'_>,
        original_drawing_denominator: &Bound<'_, PyAny>,
        original_annotation_denominator: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let inner = ezdracad_core::DeclaredCreationScale::new(
            number(original_drawing_denominator, "original drawing denominator")?,
            number(
                original_annotation_denominator,
                "original annotation denominator",
            )?,
        )
        .map_err(|error| super::python_error(py, error))?;
        Ok(Self { inner })
    }

    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        row.set_item(
            "original_drawing_denominator",
            self.inner.original_drawing_denominator(),
        )?;
        row.set_item(
            "original_annotation_denominator",
            self.inner.original_annotation_denominator(),
        )?;
        row.set_item(
            "original_annotation_ratio",
            self.inner.original_annotation_ratio(),
        )?;
        match self.inner.provenance() {
            ezdracad_core::CreationScaleProvenance::CallerDeclaredUnverified => {
                row.set_item("provenance", "caller_declared_unverified")?;
            }
        }
        Ok(row)
    }

    fn length_mm(&self, py: Python<'_>, stored_length: &Bound<'_, PyAny>) -> PyResult<f64> {
        self.inner
            .length_mm(number(stored_length, "stored length")?)
            .map_err(|error| super::python_error(py, error))
    }
}
