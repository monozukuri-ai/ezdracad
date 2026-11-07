//! Saved native print-policy fields qualified by independent J209 changes.
//!
//! Marker-25 only. MPXL omits the observed reduction and printer-override
//! switches; their absence must never be treated as a default native policy.

use crate::preflight::invalid;
use crate::{
    DracadError, ObservedProfile, PrintDeviceContext, PrintOutputScale, PrintWidthPolicy,
    RawDocument, Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPrintSettingField {
    pub name: &'static str,
    pub payload_offset: u64,
    pub raw_bytes: Vec<u8>,
}

/// Owned source fields. Reading raw values permits unknown switches/modes;
/// `width_policy` separately rejects every unqualified value. This snapshot
/// does not contain a printer DEVMODE, printable area, or visibility decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPrintSettings {
    fields: [RawPrintSettingField; 7],
}

// Boolean/mode contrasts identify the low byte. Four bytes are retained and
// checked as a conservative guard window: the observed upper three bytes are
// zero. This is not a declaration of the native C/C++ field's ABI width.
const FIELDS: [(&str, usize, usize); 7] = [
    ("drawing_paper_scale_factor", 0x178, 8),
    ("printer_settings", 0x1d0, 4),
    ("output_scale_mode", 0x1fc, 4),
    ("width_by_color", 0x200, 4),
    ("print_scale_factor", 0x208, 8),
    ("reduction_mode", 0x2d8, 4),
    ("suppress_zero_width", 0x788, 4),
];

fn positive(value: f64, label: &str) -> Result<f64> {
    if !value.is_finite() || value <= 0.0 {
        return Err(invalid(format!(
            "print settings {label} must be finite and positive"
        )));
    }
    Ok(value)
}

fn switch(raw: u32, label: &str) -> Result<bool> {
    match raw {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(DracadError::UnsupportedFeature(format!(
            "unqualified native print {label} switch {raw}"
        ))),
    }
}

impl RawPrintSettings {
    pub fn parse(document: &RawDocument) -> Result<Self> {
        if document.header().profile != ObservedProfile::Marker25 {
            return Err(DracadError::UnsupportedFeature(
                "native print-setting offsets are qualified only for marker 25".into(),
            ));
        }
        let mut fields = Vec::with_capacity(FIELDS.len());
        for (name, offset, size) in FIELDS {
            let bytes = document
                .contents()
                .get(offset..offset + size)
                .ok_or_else(|| invalid("truncated native print-setting field"))?;
            fields.push(RawPrintSettingField {
                name,
                payload_offset: offset as u64,
                raw_bytes: bytes.to_vec(),
            });
        }
        Ok(Self {
            fields: fields
                .try_into()
                .map_err(|_| invalid("incomplete native print-setting snapshot"))?,
        })
    }

    #[must_use]
    pub const fn fields(&self) -> &[RawPrintSettingField; 7] {
        &self.fields
    }

    fn u32(&self, slot: usize) -> u32 {
        // Exact field sizes are established once in parse; fields are private.
        let bytes = &self.fields[slot].raw_bytes;
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn f64(&self, slot: usize) -> f64 {
        let bytes = &self.fields[slot].raw_bytes;
        f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    #[must_use]
    pub fn drawing_paper_scale_factor(&self) -> f64 {
        self.f64(0)
    }
    #[must_use]
    pub fn printer_settings_raw(&self) -> u32 {
        self.u32(1)
    }
    #[must_use]
    pub fn output_scale_mode_raw(&self) -> u32 {
        self.u32(2)
    }
    #[must_use]
    pub fn width_by_color_raw(&self) -> u32 {
        self.u32(3)
    }
    #[must_use]
    pub fn print_scale_factor(&self) -> f64 {
        self.f64(4)
    }
    #[must_use]
    pub fn reduction_mode_raw(&self) -> u32 {
        self.u32(5)
    }
    #[must_use]
    pub fn suppress_zero_width_raw(&self) -> u32 {
        self.u32(6)
    }

    /// Construct a typed nominal-width policy. A saved printer override always
    /// forces the driver hold, even if the caller requests application-managed
    /// vectors. Driver state, rasterisation and layout are not silently assumed.
    pub fn width_policy(
        &self,
        device_context: PrintDeviceContext,
        automatic_output_denominator: Option<f64>,
    ) -> Result<PrintWidthPolicy> {
        let driver = switch(self.printer_settings_raw(), "printer-settings")?;
        let width_by_color = switch(self.width_by_color_raw(), "width-by-color")?;
        let suppress_zero_width = switch(self.suppress_zero_width_raw(), "suppress-zero-width")?;
        let reduction_mode = switch(self.reduction_mode_raw(), "reduction-mode")?;
        let drawing_denominator = positive(
            1.0 / positive(
                self.drawing_paper_scale_factor(),
                "drawing paper-scale factor",
            )?,
            "drawing denominator",
        )?;
        if let Some(value) = automatic_output_denominator {
            positive(value, "automatic output denominator")?;
            if self.output_scale_mode_raw() != 1 {
                return Err(invalid(
                    "automatic output denominator supplied for a nonautomatic scale",
                ));
            }
        }
        let output_scale = match self.output_scale_mode_raw() {
            0 => PrintOutputScale::Drawing,
            1 => PrintOutputScale::Automatic {
                resolved_denominator: automatic_output_denominator,
            },
            2 => PrintOutputScale::Specified {
                denominator: positive(
                    1.0 / positive(self.print_scale_factor(), "specified print-scale factor")?,
                    "specified print denominator",
                )?,
            },
            other => {
                return Err(DracadError::UnsupportedFeature(format!(
                    "unqualified native print output-scale mode {other}"
                )))
            }
        };
        Ok(PrintWidthPolicy {
            width_by_color,
            suppress_zero_width,
            reduction_mode,
            drawing_denominator,
            output_scale,
            device_context: if driver {
                PrintDeviceContext::DriverManaged
            } else {
                device_context
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        resolve_nominal_print_width, scan_contents, EffectiveStyleIndices, ParseLimits,
        PrintWidthStatus, RawStyleTables, ResolvedStyleIndex,
    };

    fn fixture() -> Vec<u8> {
        let mut bytes = include_bytes!("../../../tests/fixtures/semantics25.bin").to_vec();
        for (_, at, size) in FIELDS {
            bytes[at..at + size].fill(0);
        }
        bytes[0x178..0x180].copy_from_slice(&0.01f64.to_le_bytes());
        bytes[0x208..0x210].copy_from_slice(&0.005f64.to_le_bytes());
        bytes[0x170..0x178].fill(0xa5);
        bytes[0x1d4..0x1d8].fill(0xc7);
        bytes[0x204..0x208].fill(0xd8);
        bytes[0x2dc..0x2e0].fill(0xe9);
        bytes[0x78c..0x790].fill(0xfa);
        bytes
    }

    fn parse(bytes: &[u8]) -> RawPrintSettings {
        RawPrintSettings::parse(&scan_contents(bytes, &ParseLimits::default()).unwrap()).unwrap()
    }

    #[test]
    fn exact_owned_fields_and_typed_scale_modes_do_not_consume_neighbor_bytes() {
        let mut bytes = fixture();
        let raw = parse(&bytes);
        for field in raw.fields() {
            let at = field.payload_offset as usize;
            assert_eq!(field.raw_bytes, bytes[at..at + field.raw_bytes.len()]);
        }
        let p = raw
            .width_policy(PrintDeviceContext::Unspecified, None)
            .unwrap();
        assert_eq!(p.drawing_denominator, 100.0);
        assert_eq!(p.output_scale, PrintOutputScale::Drawing);
        bytes[0x1fc..0x200].copy_from_slice(&2u32.to_le_bytes());
        bytes[0x2d8..0x2dc].copy_from_slice(&1u32.to_le_bytes());
        let p = parse(&bytes)
            .width_policy(PrintDeviceContext::ApplicationManagedOpaqueVectors, None)
            .unwrap();
        assert_eq!(
            p.output_scale,
            PrintOutputScale::Specified { denominator: 200.0 }
        );
        assert!(p.reduction_mode);
        bytes[0x1fc..0x200].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            parse(&bytes)
                .width_policy(PrintDeviceContext::Unspecified, None)
                .unwrap()
                .output_scale,
            PrintOutputScale::Automatic {
                resolved_denominator: None
            }
        );
        assert_eq!(
            parse(&bytes)
                .width_policy(PrintDeviceContext::Unspecified, Some(75.0))
                .unwrap()
                .output_scale,
            PrintOutputScale::Automatic {
                resolved_denominator: Some(75.0)
            }
        );
        assert_eq!(raw.output_scale_mode_raw(), 0); // owned, never reopens bytes
    }

    #[test]
    fn all_unknown_switch_bits_and_scale_modes_remain_raw_but_fail_policy_conversion() {
        for at in [0x1d0, 0x200, 0x2d8, 0x788] {
            for value in [2u32, 256, 0x8000_0001] {
                let mut bytes = fixture();
                bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
                let raw = parse(&bytes);
                assert!(matches!(
                    raw.width_policy(PrintDeviceContext::Unspecified, None),
                    Err(DracadError::UnsupportedFeature(_))
                ));
                assert_eq!(
                    raw.fields()
                        .iter()
                        .find(|f| f.payload_offset == at as u64)
                        .unwrap()
                        .raw_bytes,
                    value.to_le_bytes()
                );
            }
        }
        for value in [3u32, 256, u32::MAX] {
            let mut bytes = fixture();
            bytes[0x1fc..0x200].copy_from_slice(&value.to_le_bytes());
            let raw = parse(&bytes);
            assert_eq!(raw.output_scale_mode_raw(), value);
            assert!(matches!(
                raw.width_policy(PrintDeviceContext::Unspecified, None),
                Err(DracadError::UnsupportedFeature(_))
            ));
        }
    }

    #[test]
    fn invalid_used_scale_fields_fail_but_inactive_print_scale_stays_observable() {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::from_bits(1)] {
            let mut bytes = fixture();
            bytes[0x178..0x180].copy_from_slice(&value.to_le_bytes());
            assert!(parse(&bytes)
                .width_policy(PrintDeviceContext::Unspecified, None)
                .is_err());
            let mut bytes = fixture();
            bytes[0x208..0x210].copy_from_slice(&value.to_le_bytes());
            let raw = parse(&bytes);
            assert!(raw
                .width_policy(PrintDeviceContext::Unspecified, None)
                .is_ok());
            bytes[0x1fc..0x200].copy_from_slice(&2u32.to_le_bytes());
            assert!(parse(&bytes)
                .width_policy(PrintDeviceContext::Unspecified, None)
                .is_err());
        }
        assert!(parse(&fixture())
            .width_policy(PrintDeviceContext::Unspecified, Some(100.0))
            .is_err());
    }

    #[test]
    fn persisted_driver_override_cannot_be_bypassed_by_caller_device_context() {
        let mut bytes = fixture();
        bytes[0x1d0..0x1d4].copy_from_slice(&1u32.to_le_bytes());
        let doc = scan_contents(&bytes, &ParseLimits::default()).unwrap();
        let raw = RawPrintSettings::parse(&doc).unwrap();
        let policy = raw
            .width_policy(PrintDeviceContext::ApplicationManagedOpaqueVectors, None)
            .unwrap();
        assert_eq!(policy.device_context, PrintDeviceContext::DriverManaged);
        let index = |index| ResolvedStyleIndex {
            index,
            sources: vec![],
        };
        let styles = EffectiveStyleIndices {
            color: index(2),
            line_type: index(1),
            width: index(5),
        };
        let result =
            resolve_nominal_print_width(&styles, &RawStyleTables::parse(&doc).unwrap(), &policy)
                .unwrap();
        assert_eq!(result.status, PrintWidthStatus::HeldDriverPolicy);
        assert_eq!(result.nominal_paper_mm, None);
    }

    #[test]
    fn legacy_offsets_have_not_been_promoted_without_native_policy_holdouts() {
        for marker in [5, 8] {
            let b = match marker {
                5 => include_bytes!("../../../tests/fixtures/semantics5.bin").as_slice(),
                _ => include_bytes!("../../../tests/fixtures/semantics8.bin").as_slice(),
            };
            let doc = scan_contents(b, &ParseLimits::default()).unwrap();
            assert!(matches!(
                RawPrintSettings::parse(&doc),
                Err(DracadError::UnsupportedFeature(_))
            ));
        }
    }
}
