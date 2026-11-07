//! Explicit unresolved unit/history states and caller-declared creation scales.

use crate::preflight::invalid;
use crate::{
    CoordinateMeasurementContext, NativeCoordinateInputUnitInfo, OriginUnitsCode, RawDocument,
    Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalUnitUnresolvedReason {
    StoredCoordinateUnitNotEstablished,
}

/// No physical-unit name for stored geometry is inferred from Origin.Units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalUnitResolution {
    Unresolved(PhysicalUnitUnresolvedReason),
}

/// This describes the decoded fields, not an absence from the entire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationScaleResolution {
    UnavailableFromDecodedFields,
}

/// An owned snapshot of what is known and unresolved about coordinate units.
///
/// Current drawing/annotation scales are document context, not entity history.
/// Marker 25 is required by the independently qualified Origin.Units reader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitResolution {
    pub physical_units: PhysicalUnitResolution,
    pub original_creation_scale: CreationScaleResolution,
    pub current_measurement: CoordinateMeasurementContext,
    pub origin_units: OriginUnitsCode,
    pub coordinate_input_unit: NativeCoordinateInputUnitInfo,
}

impl RawDocument {
    /// Preserve raw sources while explicitly reporting unresolved meanings.
    pub fn unit_resolution(&self) -> Result<UnitResolution> {
        let origin_units = self.origin_units_code()?;
        let current_measurement = self.coordinate_measurement_context()?;
        let coordinate_input_unit =
            NativeCoordinateInputUnitInfo::from_origin_units_code(origin_units);
        Ok(UnitResolution {
            physical_units: PhysicalUnitResolution::Unresolved(
                PhysicalUnitUnresolvedReason::StoredCoordinateUnitNotEstablished,
            ),
            original_creation_scale: CreationScaleResolution::UnavailableFromDecodedFields,
            current_measurement,
            origin_units,
            coordinate_input_unit,
        })
    }
}

/// The caller supplied these scales; they were not recovered from a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationScaleProvenance {
    CallerDeclaredUnverified,
}

/// External knowledge of both scales at the time a figure was created.
///
/// This is an explicit caller declaration, not native metadata or an inference.
/// Both original denominators are required: substituting the current drawing
/// denominator after it has changed can produce an incorrect original length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeclaredCreationScale {
    original_drawing_denominator: f64,
    original_annotation_denominator: f64,
    original_annotation_ratio: f64,
}

impl DeclaredCreationScale {
    pub fn new(
        original_drawing_denominator: f64,
        original_annotation_denominator: f64,
    ) -> Result<Self> {
        for (label, value) in [
            ("original drawing denominator", original_drawing_denominator),
            (
                "original annotation denominator",
                original_annotation_denominator,
            ),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(invalid(format!(
                    "declared creation scale {label} must be finite and positive"
                )));
            }
        }
        let original_annotation_ratio =
            original_drawing_denominator / original_annotation_denominator;
        if !original_annotation_ratio.is_finite() || original_annotation_ratio <= 0.0 {
            return Err(invalid(
                "declared creation scale original D/A ratio must be finite and positive",
            ));
        }
        Ok(Self {
            original_drawing_denominator,
            original_annotation_denominator,
            original_annotation_ratio,
        })
    }

    pub fn original_drawing_denominator(&self) -> f64 {
        self.original_drawing_denominator
    }

    pub fn original_annotation_denominator(&self) -> f64 {
        self.original_annotation_denominator
    }

    pub fn original_annotation_ratio(&self) -> f64 {
        self.original_annotation_ratio
    }

    pub fn provenance(&self) -> CreationScaleProvenance {
        CreationScaleProvenance::CallerDeclaredUnverified
    }

    /// Return stored_length * original_A / original_D in declared millimeters.
    ///
    /// The result is conditional on the caller's declaration. It does not
    /// resolve the file's physical-unit code or infer any entity creation scale.
    pub fn length_mm(&self, stored_length: f64) -> Result<f64> {
        if !stored_length.is_finite() || stored_length < 0.0 {
            return Err(invalid(
                "declared creation scale stored length must be finite and nonnegative",
            ));
        }
        if stored_length == 0.0 {
            return Ok(stored_length);
        }
        // Retain the checked original D/A ratio so the raw*A multiplication
        // cannot overflow before division restores a representable result.
        let result = stored_length / self.original_annotation_ratio;
        if result.is_finite() && result > 0.0 {
            return Ok(result);
        }
        Err(invalid(
            "declared creation scale length overflow or nonzero underflow",
        ))
    }
}
