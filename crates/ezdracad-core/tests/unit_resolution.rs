use ezdracad_core::{
    scan_contents, CreationScaleProvenance, CreationScaleResolution, DeclaredCreationScale,
    DracadError, ParseLimits, PhysicalUnitResolution, PhysicalUnitUnresolvedReason,
};

fn fixture(marker: u16, code: i32, factor: f64, ratio: f64) -> Vec<u8> {
    let source = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
    };
    let mut data = source.to_vec();
    data[0x150..0x154].copy_from_slice(&code.to_le_bytes());
    data[0x178..0x180].copy_from_slice(&factor.to_le_bytes());
    data[0x278..0x280].copy_from_slice(&ratio.to_le_bytes());
    data
}

#[test]
fn resolution_retains_sources_without_promoting_known_or_unknown_codes() {
    for code in [0, 1, 2, 3, -1, i32::MIN, i32::MAX] {
        let data = fixture(25, code, 0.01, 0.5);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let before = doc.line_records().collect::<Result<Vec<_>, _>>().unwrap();
        let resolution = doc.unit_resolution().unwrap();
        assert_eq!(
            resolution.physical_units,
            PhysicalUnitResolution::Unresolved(
                PhysicalUnitUnresolvedReason::StoredCoordinateUnitNotEstablished
            )
        );
        assert_eq!(
            resolution.original_creation_scale,
            CreationScaleResolution::UnavailableFromDecodedFields
        );
        assert_eq!(resolution.origin_units, doc.origin_units_code().unwrap());
        assert_eq!(
            resolution.coordinate_input_unit.origin_units(),
            resolution.origin_units
        );
        assert_eq!(
            resolution.coordinate_input_unit,
            doc.coordinate_input_unit().unwrap()
        );
        assert_eq!(resolution.origin_units.selector_bytes, code.to_le_bytes());
        let current = resolution.current_measurement;
        assert_eq!(current, doc.coordinate_measurement_context().unwrap());
        assert_eq!(current.paper_scale_bytes, 0.01_f64.to_le_bytes());
        assert_eq!(current.annotation_ratio_bytes, 0.5_f64.to_le_bytes());
        assert_eq!(current.drawing_denominator, 100.0);
        assert_eq!(current.annotation_denominator, 200.0);
        assert_eq!(current.length_on_paper_mm(1000.0).unwrap(), 10.0);
        assert_eq!(
            current.length_at_current_annotation_mm(1000.0).unwrap(),
            2000.0
        );
        assert_eq!(doc.contents(), data);
        assert_eq!(
            doc.line_records().collect::<Result<Vec<_>, _>>().unwrap(),
            before
        );
    }
}

#[test]
fn unsupported_source_profiles_and_invalid_current_scales_fail_on_demand() {
    for marker in [5, 8] {
        let data = fixture(marker, 0, 0.01, 1.0);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.unit_resolution(),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert!(matches!(
            doc.origin_units_code(),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert!(doc.coordinate_measurement_context().is_ok());
        assert_eq!(doc.line_records().count(), 2);
    }
    let data = fixture(25, 0, 0.0, 1.0);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.unit_resolution(),
        Err(DracadError::InvalidFormat(_))
    ));
    assert_eq!(doc.origin_units_code().unwrap().raw_code, 0);
    assert_eq!(doc.line_records().count(), 2);
}

#[test]
fn external_creation_scale_requires_original_drawing_and_annotation_denominators() {
    let doc = scan_contents(&fixture(25, 0, 0.01, 1.0), &ParseLimits::default()).unwrap();
    let current = doc.unit_resolution().unwrap();
    assert_eq!(current.current_measurement.drawing_denominator, 100.0);
    assert_eq!(current.current_measurement.annotation_denominator, 100.0);
    assert_eq!(
        current
            .current_measurement
            .length_at_current_annotation_mm(1000.0)
            .unwrap(),
        1000.0
    );
    let declared = DeclaredCreationScale::new(25.0, 100.0).unwrap();
    assert_eq!(declared.original_drawing_denominator(), 25.0);
    assert_eq!(declared.original_annotation_denominator(), 100.0);
    assert_eq!(declared.original_annotation_ratio(), 0.25);
    assert_eq!(
        declared.provenance(),
        CreationScaleProvenance::CallerDeclaredUnverified
    );
    assert_eq!(declared.length_mm(1000.0).unwrap(), 4000.0);
    assert_eq!(
        DeclaredCreationScale::new(100.0, 100.0)
            .unwrap()
            .length_mm(1000.0)
            .unwrap(),
        1000.0
    );
    assert_eq!(doc.unit_resolution().unwrap(), current);
}

#[test]
fn invalid_external_denominators_and_unrepresentable_ratios_are_rejected() {
    for bad in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(DeclaredCreationScale::new(bad, 100.0).is_err());
        assert!(DeclaredCreationScale::new(100.0, bad).is_err());
    }
    for (drawing, annotation) in [(f64::MAX, f64::from_bits(1)), (f64::from_bits(1), f64::MAX)] {
        assert!(DeclaredCreationScale::new(drawing, annotation).is_err());
    }
}

#[test]
fn conversion_avoids_intermediate_product_overflow_and_rejects_lost_results() {
    let balanced = DeclaredCreationScale::new(1e308, 1e308).unwrap();
    assert_eq!(balanced.length_mm(1e308).unwrap(), 1e308);
    assert_eq!(balanced.length_mm(f64::from_bits(1)).unwrap().to_bits(), 1);
    assert_eq!(
        balanced.length_mm(-0.0).unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    for bad in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(balanced.length_mm(bad).is_err());
    }
    assert!(DeclaredCreationScale::new(100.0, 200.0)
        .unwrap()
        .length_mm(f64::MAX)
        .is_err());
    assert!(DeclaredCreationScale::new(200.0, 100.0)
        .unwrap()
        .length_mm(f64::from_bits(1))
        .is_err());
}
