//! Explicit print-policy evaluation of a resolved native style.
//!
//! This returns the application's nominal vector pen request on paper. It is
//! never a measurement of physical ink, a visibility result, or a decoder for
//! as-yet-unqualified persisted print flags. The caller supplies the print policy.

use crate::preflight::invalid;
use crate::{DracadError, EffectiveStyleIndices, RawStyleTables, Result, StyleSource};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrintOutputScale {
    Drawing,
    Specified {
        denominator: f64,
    },
    /// Fit depends on printable area and margins. Supply a separately determined
    /// output denominator only when that complete layout context is known.
    Automatic {
        resolved_denominator: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintDeviceContext {
    #[default]
    Unspecified,
    /// The caller has checked that printer-width override is off and this
    /// output path preserves opaque vector pens. Hardware rasterisation remains
    /// outside this assertion and outside the result.
    ApplicationManagedOpaqueVectors,
    DriverManaged,
    RasterizedTransparency,
}

/// Explicit policy, suitable for constructing from separately qualified native
/// print settings. No mode is inferred from missing MPXL attributes or RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintWidthPolicy {
    pub width_by_color: bool,
    pub suppress_zero_width: bool,
    pub reduction_mode: bool,
    pub drawing_denominator: f64,
    pub output_scale: PrintOutputScale,
    pub device_context: PrintDeviceContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintWidthStatus {
    ResolvedNominalMm,
    SuppressedZero,
    DeviceHairline,
    HeldDeviceContext,
    HeldDriverPolicy,
    HeldRasterization,
    HeldOutputScale,
}

impl PrintWidthStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResolvedNominalMm => "resolved_nominal_mm",
            Self::SuppressedZero => "suppressed_zero",
            Self::DeviceHairline => "device_hairline",
            Self::HeldDeviceContext => "held_device_context",
            Self::HeldDriverPolicy => "held_driver_policy",
            Self::HeldRasterization => "held_rasterization",
            Self::HeldOutputScale => "held_output_scale",
        }
    }
}

/// An owned decision with the selected native row and effective-style evidence.
/// `nominal_paper_mm` is present only for a resolved positive vector request.
/// A suppressed primitive and a device hairline deliberately do not return 0 mm
/// as though it were a positive measurable physical stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintWidthResolution {
    pub status: PrintWidthStatus,
    pub policy: PrintWidthPolicy,
    pub selected_table_domain: &'static str,
    pub selected_index: u16,
    pub selected_row_offset: u64,
    pub selected_row_bytes: [u8; 64],
    pub style_sources: Vec<StyleSource>,
    pub pen_width_hundredths_raw: u16,
    pub base_pen_mm: f64,
    pub applied_scale_factor: Option<f64>,
    pub nominal_paper_mm: Option<f64>,
}

fn positive(value: f64, name: &str) -> Result<f64> {
    if !value.is_finite() || value <= 0.0 {
        return Err(invalid(format!("{name} must be finite and positive")));
    }
    Ok(value)
}

fn scale(policy: &PrintWidthPolicy) -> Result<Option<f64>> {
    let drawing = positive(policy.drawing_denominator, "drawing denominator")?;
    let output = match policy.output_scale {
        PrintOutputScale::Drawing => return Ok(Some(1.0)),
        PrintOutputScale::Specified { denominator } => {
            Some(positive(denominator, "print denominator")?)
        }
        PrintOutputScale::Automatic {
            resolved_denominator,
        } => resolved_denominator
            .map(|value| positive(value, "resolved automatic print denominator"))
            .transpose()?,
    };
    if !policy.reduction_mode {
        return Ok(Some(1.0));
    }
    output
        .map(|output| positive(drawing / output, "print-width scale factor"))
        .transpose()
}

/// Apply explicit native print policy after effective ByLayer/ByBlock indices
/// have been resolved. The print scale changes pen width only in reduction mode;
/// block geometric scale is intentionally not an input to this output quantity.
pub fn resolve_nominal_print_width(
    styles: &EffectiveStyleIndices,
    tables: &RawStyleTables,
    policy: &PrintWidthPolicy,
) -> Result<PrintWidthResolution> {
    let applied_scale_factor = scale(policy)?;
    let selected = if policy.width_by_color {
        &styles.color
    } else {
        &styles.width
    };
    let slot = selected
        .index
        .checked_sub(1)
        .map(usize::from)
        .ok_or_else(|| {
            DracadError::UnsupportedFeature(
                "print width requires a resolved native style index".into(),
            )
        })?;
    let (domain, offset, bytes, hundredths) = if policy.width_by_color {
        let row = tables
            .colors
            .get(slot)
            .filter(|row| row.index == selected.index)
            .ok_or_else(|| {
                DracadError::UnsupportedFeature("missing native color-width row".into())
            })?;
        (
            "color_table",
            row.payload_offset,
            row.raw_bytes,
            row.pen_width_hundredths_raw,
        )
    } else {
        let row = tables
            .widths
            .get(slot)
            .filter(|row| row.index == selected.index)
            .ok_or_else(|| DracadError::UnsupportedFeature("missing native width row".into()))?;
        (
            "width_table",
            row.payload_offset,
            row.raw_bytes,
            row.pen_width_hundredths_raw,
        )
    };
    let base_pen_mm = f64::from(hundredths) / 100.0;
    let mut result = PrintWidthResolution {
        status: PrintWidthStatus::HeldDeviceContext,
        policy: *policy,
        selected_table_domain: domain,
        selected_index: selected.index,
        selected_row_offset: offset,
        selected_row_bytes: bytes,
        style_sources: selected.sources.clone(),
        pen_width_hundredths_raw: hundredths,
        base_pen_mm,
        applied_scale_factor,
        nominal_paper_mm: None,
    };
    result.status = match policy.device_context {
        PrintDeviceContext::Unspecified => PrintWidthStatus::HeldDeviceContext,
        PrintDeviceContext::DriverManaged => PrintWidthStatus::HeldDriverPolicy,
        PrintDeviceContext::RasterizedTransparency => PrintWidthStatus::HeldRasterization,
        PrintDeviceContext::ApplicationManagedOpaqueVectors => {
            if hundredths == 0 {
                if policy.suppress_zero_width {
                    PrintWidthStatus::SuppressedZero
                } else {
                    PrintWidthStatus::DeviceHairline
                }
            } else if let Some(factor) = applied_scale_factor {
                let width = positive(base_pen_mm * factor, "nominal paper pen width")?;
                result.nominal_paper_mm = Some(width);
                PrintWidthStatus::ResolvedNominalMm
            } else {
                PrintWidthStatus::HeldOutputScale
            }
        }
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scan_contents, ParseLimits, ResolvedStyleIndex};

    fn inputs() -> (EffectiveStyleIndices, RawStyleTables, PrintWidthPolicy) {
        let doc = scan_contents(
            include_bytes!("../../../tests/fixtures/semantics25.bin"),
            &ParseLimits::default(),
        )
        .unwrap();
        let mut tables = RawStyleTables::parse(&doc).unwrap();
        tables.widths[4].pen_width_hundredths_raw = 20;
        tables.colors[1].pen_width_hundredths_raw = 70;
        let index = |index| ResolvedStyleIndex {
            index,
            sources: vec![],
        };
        (
            EffectiveStyleIndices {
                color: index(2),
                line_type: index(1),
                width: index(5),
            },
            tables,
            PrintWidthPolicy {
                width_by_color: false,
                suppress_zero_width: false,
                reduction_mode: false,
                drawing_denominator: 100.0,
                output_scale: PrintOutputScale::Drawing,
                device_context: PrintDeviceContext::ApplicationManagedOpaqueVectors,
            },
        )
    }

    #[test]
    fn selects_file_native_width_or_color_row_before_print_scaling() {
        let (styles, tables, mut p) = inputs();
        let r = resolve_nominal_print_width(&styles, &tables, &p).unwrap();
        assert_eq!(r.status, PrintWidthStatus::ResolvedNominalMm);
        assert_eq!(r.selected_table_domain, "width_table");
        assert_eq!(r.selected_index, 5);
        assert_eq!(r.nominal_paper_mm, Some(0.2));
        assert_eq!(r.selected_row_bytes, tables.widths[4].raw_bytes);
        p.width_by_color = true;
        let r = resolve_nominal_print_width(&styles, &tables, &p).unwrap();
        assert_eq!(r.selected_table_domain, "color_table");
        assert_eq!(r.selected_index, 2);
        assert_eq!(r.nominal_paper_mm, Some(0.7));
        assert_eq!(r.selected_row_bytes, tables.colors[1].raw_bytes);
    }

    #[test]
    fn fixed_automatic_and_drawing_scale_follow_reduction_policy() {
        let (styles, tables, mut p) = inputs();
        p.output_scale = PrintOutputScale::Specified { denominator: 200.0 };
        assert_eq!(
            resolve_nominal_print_width(&styles, &tables, &p)
                .unwrap()
                .nominal_paper_mm,
            Some(0.2)
        );
        p.reduction_mode = true;
        assert_eq!(
            resolve_nominal_print_width(&styles, &tables, &p)
                .unwrap()
                .nominal_paper_mm,
            Some(0.1)
        );
        p.output_scale = PrintOutputScale::Drawing;
        assert_eq!(
            resolve_nominal_print_width(&styles, &tables, &p)
                .unwrap()
                .nominal_paper_mm,
            Some(0.2)
        );
        p.output_scale = PrintOutputScale::Automatic {
            resolved_denominator: None,
        };
        assert_eq!(
            resolve_nominal_print_width(&styles, &tables, &p)
                .unwrap()
                .status,
            PrintWidthStatus::HeldOutputScale
        );
        p.reduction_mode = false;
        assert_eq!(
            resolve_nominal_print_width(&styles, &tables, &p)
                .unwrap()
                .nominal_paper_mm,
            Some(0.2)
        );
        p.reduction_mode = true;
        p.output_scale = PrintOutputScale::Automatic {
            resolved_denominator: Some(50.0),
        };
        assert_eq!(
            resolve_nominal_print_width(&styles, &tables, &p)
                .unwrap()
                .nominal_paper_mm,
            Some(0.4)
        );
    }

    #[test]
    fn device_uncertainty_never_returns_a_nominal_number() {
        let (styles, tables, mut p) = inputs();
        for (context, status) in [
            (
                PrintDeviceContext::Unspecified,
                PrintWidthStatus::HeldDeviceContext,
            ),
            (
                PrintDeviceContext::DriverManaged,
                PrintWidthStatus::HeldDriverPolicy,
            ),
            (
                PrintDeviceContext::RasterizedTransparency,
                PrintWidthStatus::HeldRasterization,
            ),
        ] {
            p.device_context = context;
            let r = resolve_nominal_print_width(&styles, &tables, &p).unwrap();
            assert_eq!(r.status, status);
            assert_eq!(r.nominal_paper_mm, None);
            assert_eq!(r.base_pen_mm, 0.2);
        }
    }

    #[test]
    fn zero_width_preserves_suppression_and_hairline_distinction_in_both_tables() {
        let (styles, mut tables, mut p) = inputs();
        tables.widths[4].pen_width_hundredths_raw = 0;
        tables.colors[1].pen_width_hundredths_raw = 0;
        for color in [false, true] {
            p.width_by_color = color;
            p.suppress_zero_width = false;
            assert_eq!(
                resolve_nominal_print_width(&styles, &tables, &p)
                    .unwrap()
                    .status,
                PrintWidthStatus::DeviceHairline
            );
            p.suppress_zero_width = true;
            let r = resolve_nominal_print_width(&styles, &tables, &p).unwrap();
            assert_eq!(r.status, PrintWidthStatus::SuppressedZero);
            assert_eq!(r.nominal_paper_mm, None);
        }
    }

    #[test]
    fn invalid_denominators_and_numerical_overflow_underflow_are_rejected() {
        let (styles, tables, mut p) = inputs();
        for bad in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            p.drawing_denominator = bad;
            assert!(resolve_nominal_print_width(&styles, &tables, &p).is_err());
            p.drawing_denominator = 100.0;
            p.output_scale = PrintOutputScale::Specified { denominator: bad };
            assert!(resolve_nominal_print_width(&styles, &tables, &p).is_err());
            p.output_scale = PrintOutputScale::Automatic {
                resolved_denominator: Some(bad),
            };
            assert!(resolve_nominal_print_width(&styles, &tables, &p).is_err());
        }
        p.reduction_mode = true;
        p.drawing_denominator = f64::MAX;
        p.output_scale = PrintOutputScale::Specified {
            denominator: f64::from_bits(1),
        };
        assert!(resolve_nominal_print_width(&styles, &tables, &p).is_err());
        p.drawing_denominator = f64::from_bits(1);
        p.output_scale = PrintOutputScale::Specified {
            denominator: f64::MAX,
        };
        assert!(resolve_nominal_print_width(&styles, &tables, &p).is_err());
        p.output_scale = PrintOutputScale::Specified { denominator: 1.0 };
        assert!(resolve_nominal_print_width(&styles, &tables, &p).is_err());
    }
}
