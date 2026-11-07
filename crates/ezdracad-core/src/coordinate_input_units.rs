//! Native numeric coordinate-input units, not immutable stored-geometry units.

use crate::preflight::invalid;
use crate::{DracadError, OriginUnitsCode, RawDocument, Result};

/// Names declared by the shipped plugin and checked by native J3 input replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCoordinateInputUnit {
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
    Inches,
    Feet,
    Yards,
    Unknown(i32),
}

impl NativeCoordinateInputUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Millimeters => "mm",
            Self::Centimeters => "cm",
            Self::Meters => "m",
            Self::Kilometers => "km",
            Self::Inches => "inch",
            Self::Feet => "feet",
            Self::Yards => "yard",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Source-preserving numeric input convention of the native application.
///
/// The display factors are the application's constants, including its rounded
/// imperial values. They must not be replaced with exact SI conversion values.
/// This metadata does not declare physical units of arbitrary saved entities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeCoordinateInputUnitInfo {
    origin_units: OriginUnitsCode,
    unit: NativeCoordinateInputUnit,
    native_display_factor: Option<f64>,
}

impl NativeCoordinateInputUnitInfo {
    pub fn from_origin_units_code(origin_units: OriginUnitsCode) -> Self {
        let (unit, native_display_factor) = match origin_units.raw_code {
            0 => (NativeCoordinateInputUnit::Millimeters, Some(1.0)),
            1 => (NativeCoordinateInputUnit::Centimeters, Some(0.1)),
            2 => (NativeCoordinateInputUnit::Meters, Some(0.001)),
            3 => (NativeCoordinateInputUnit::Kilometers, Some(0.000001)),
            4 => (NativeCoordinateInputUnit::Inches, Some(0.03937)),
            5 => (NativeCoordinateInputUnit::Feet, Some(0.003281)),
            6 => (NativeCoordinateInputUnit::Yards, Some(0.0010936)),
            code => (NativeCoordinateInputUnit::Unknown(code), None),
        };
        Self {
            origin_units,
            unit,
            native_display_factor,
        }
    }

    pub fn origin_units(&self) -> OriginUnitsCode {
        self.origin_units
    }

    pub fn unit(&self) -> NativeCoordinateInputUnit {
        self.unit
    }

    pub fn native_display_factor(&self) -> Option<f64> {
        self.native_display_factor
    }

    fn factor(&self) -> Result<f64> {
        self.native_display_factor.ok_or_else(|| {
            DracadError::UnsupportedFeature(format!(
                "unqualified native coordinate input unit code {}",
                self.origin_units.raw_code
            ))
        })
    }

    fn converted(value: f64, result: f64) -> Result<f64> {
        if !value.is_finite() {
            return Err(invalid("native coordinate input scalar must be finite"));
        }
        if !result.is_finite() {
            return Err(invalid("native coordinate input conversion overflow"));
        }
        if value != 0.0 && result == 0.0 {
            return Err(invalid("native coordinate input nonzero underflow"));
        }
        Ok(result)
    }

    /// Convert a signed input scalar using the native application's multiplier.
    ///
    /// This is the base-mm scalar used by the native input convention. It does
    /// not apply drawing origin, drawing/annotation scales or a point transform.
    pub fn to_native_mm(&self, value: f64) -> Result<f64> {
        let factor = self.factor()?;
        Self::converted(value, value / factor)
    }

    /// Convert a base-mm scalar to the native numeric input/display convention.
    pub fn from_native_mm(&self, value: f64) -> Result<f64> {
        let factor = self.factor()?;
        Self::converted(value, value * factor)
    }
}

impl RawDocument {
    /// Read marker-25 numeric coordinate-input units from this owned snapshot.
    pub fn coordinate_input_unit(&self) -> Result<NativeCoordinateInputUnitInfo> {
        Ok(NativeCoordinateInputUnitInfo::from_origin_units_code(
            self.origin_units_code()?,
        ))
    }
}
