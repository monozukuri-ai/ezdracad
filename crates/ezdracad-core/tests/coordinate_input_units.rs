use ezdracad_core::{scan_contents, DracadError, NativeCoordinateInputUnit, ParseLimits};

fn fixture(marker: u16, code: i32) -> Vec<u8> {
    let bytes = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
    };
    let mut result = bytes.to_vec();
    result[0x150..0x154].copy_from_slice(&code.to_le_bytes());
    result
}

#[test]
fn input_units_preserve_codes_and_native_factors_without_changing_geometry() {
    for (code, unit, factor) in [
        (0, NativeCoordinateInputUnit::Millimeters, 1.0),
        (1, NativeCoordinateInputUnit::Centimeters, 0.1),
        (2, NativeCoordinateInputUnit::Meters, 0.001),
        (3, NativeCoordinateInputUnit::Kilometers, 0.000001),
        (4, NativeCoordinateInputUnit::Inches, 0.03937),
        (5, NativeCoordinateInputUnit::Feet, 0.003281),
        (6, NativeCoordinateInputUnit::Yards, 0.0010936),
    ] {
        let bytes = fixture(25, code);
        let doc = scan_contents(&bytes, &ParseLimits::default()).unwrap();
        let before = doc.line_records().collect::<Result<Vec<_>, _>>().unwrap();
        let info = doc.coordinate_input_unit().unwrap();
        assert_eq!(info.origin_units(), doc.origin_units_code().unwrap());
        assert_eq!(info.origin_units().selector_bytes, code.to_le_bytes());
        assert_eq!(info.unit(), unit);
        assert_eq!(info.native_display_factor(), Some(factor));
        for value in [-60.0, -20.0, 0.0, 20.0, 60.0] {
            assert_eq!(info.to_native_mm(value).unwrap(), value / factor);
            assert_eq!(info.from_native_mm(value).unwrap(), value * factor);
        }
        assert_eq!(doc.contents(), bytes);
        assert_eq!(
            doc.line_records().collect::<Result<Vec<_>, _>>().unwrap(),
            before
        );
    }
}

#[test]
fn imperial_constants_are_native_values_not_exact_si_replacements() {
    let doc = scan_contents(&fixture(25, 4), &ParseLimits::default()).unwrap();
    let info = doc.coordinate_input_unit().unwrap();
    assert_eq!(info.to_native_mm(1.0).unwrap(), 1.0 / 0.03937);
    assert_ne!(info.to_native_mm(1.0).unwrap(), 25.4);
    assert_eq!(
        100.0 + 0.5 * info.to_native_mm(-20.0).unwrap(),
        -154.000508001016
    );
    assert_eq!(
        info.from_native_mm(-0.0).unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
}

#[test]
fn unknown_signed_codes_keep_their_sources_but_refuse_conversion() {
    for code in [-1, i32::MIN, i32::MAX, 7, 0x01020304] {
        let doc = scan_contents(&fixture(25, code), &ParseLimits::default()).unwrap();
        let info = doc.coordinate_input_unit().unwrap();
        assert_eq!(info.unit(), NativeCoordinateInputUnit::Unknown(code));
        assert_eq!(info.origin_units().selector_bytes, code.to_le_bytes());
        assert_eq!(info.native_display_factor(), None);
        assert!(matches!(
            info.to_native_mm(1.0),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert!(matches!(
            info.from_native_mm(-1.0),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn unsupported_profiles_and_nonfinite_or_unrepresentable_scalars_fail_explicitly() {
    for marker in [5, 8] {
        let doc = scan_contents(&fixture(marker, 0), &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.coordinate_input_unit(),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let doc = scan_contents(&fixture(25, 1), &ParseLimits::default()).unwrap();
    let info = doc.coordinate_input_unit().unwrap();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(info.to_native_mm(value).is_err());
        assert!(info.from_native_mm(value).is_err());
    }
    assert!(info.to_native_mm(f64::MAX).is_err());
    assert!(info.to_native_mm(-f64::MAX).is_err());
    assert!(info.from_native_mm(f64::from_bits(1)).is_err());
    assert!(info.from_native_mm(-f64::from_bits(1)).is_err());
    assert_eq!(
        info.to_native_mm(-0.0).unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    let doc = scan_contents(&fixture(25, 0), &ParseLimits::default()).unwrap();
    let info = doc.coordinate_input_unit().unwrap();
    assert_eq!(info.to_native_mm(f64::MAX).unwrap(), f64::MAX);
    assert_eq!(info.from_native_mm(f64::from_bits(1)).unwrap().to_bits(), 1);
}
