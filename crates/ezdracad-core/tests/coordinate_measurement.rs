use ezdracad_core::{scan_contents, DracadError, ParseLimits};

fn fixture(marker: u16, factor: f64, ratio: f64) -> Vec<u8> {
    let source = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
    };
    let mut data = source.to_vec();
    data[0x178..0x180].copy_from_slice(&factor.to_le_bytes());
    data[0x278..0x280].copy_from_slice(&ratio.to_le_bytes());
    data[0x170..0x178].fill(0xa5);
    data[0x270..0x278].fill(0x5a);
    data[0x280..0x288].fill(0xff);
    data
}

#[test]
fn contextual_lengths_retain_exact_fields_and_leave_raw_geometry_unchanged() {
    for (factor, ratio, drawing, annotation, paper, current) in [
        (0.01, 1.0, 100.0, 100.0, 10.0, 1000.0),
        (0.01, 0.5, 100.0, 200.0, 10.0, 2000.0),
        (0.04, 1.0, 25.0, 25.0, 40.0, 1000.0),
        (0.04, 0.25, 25.0, 100.0, 40.0, 4000.0),
        (0.01, 5.0, 100.0, 20.0, 10.0, 200.0),
    ] {
        let data = fixture(25, factor, ratio);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let before = doc.line_records().collect::<Result<Vec<_>, _>>().unwrap();
        let context = doc.coordinate_measurement_context().unwrap();
        assert_eq!(context.paper_scale_offset, 0x178);
        assert_eq!(context.paper_scale_bytes, factor.to_le_bytes());
        assert_eq!(context.paper_scale_factor, factor);
        assert_eq!(context.annotation_ratio_offset, 0x278);
        assert_eq!(context.annotation_ratio_bytes, ratio.to_le_bytes());
        assert_eq!(context.annotation_ratio, ratio);
        assert_eq!(context.drawing_denominator, drawing);
        assert_eq!(context.annotation_denominator, annotation);
        assert_eq!(context.length_on_paper_mm(1000.0).unwrap(), paper);
        assert_eq!(
            context.length_at_current_annotation_mm(1000.0).unwrap(),
            current
        );
        assert_eq!(context.length_on_paper_mm(0.0).unwrap(), 0.0);
        assert_eq!(context.length_at_current_annotation_mm(0.0).unwrap(), 0.0);
        assert_eq!(
            context.length_on_paper_mm(-0.0).unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(doc.contents(), data);
        assert_eq!(
            doc.line_records().collect::<Result<Vec<_>, _>>().unwrap(),
            before
        );
        assert_eq!(before[0].coordinate_space(), "record_local");
    }
}

#[test]
fn invalid_scale_fields_and_nonfinite_derived_values_fail_on_demand() {
    for (factor, ratio) in [
        (0.0, 1.0),
        (-0.0, 1.0),
        (-1.0, 1.0),
        (f64::NAN, 1.0),
        (f64::INFINITY, 1.0),
        (f64::NEG_INFINITY, 1.0),
        (0.01, 0.0),
        (0.01, -0.0),
        (0.01, -1.0),
        (0.01, f64::NAN),
        (0.01, f64::INFINITY),
        (0.01, f64::NEG_INFINITY),
        (f64::from_bits(1), 1.0),
        (f64::MAX, f64::MAX),
        (f64::MIN_POSITIVE, f64::MIN_POSITIVE),
        (1e-200, 1e-109),
    ] {
        let data = fixture(25, factor, ratio);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.coordinate_measurement_context(),
            Err(DracadError::InvalidFormat(_))
        ));
        assert_eq!(doc.line_records().count(), 2);
        assert_eq!(doc.line_candidate_count(), 2);
        assert_eq!(doc.contents(), data);
    }
}

#[test]
fn invalid_raw_lengths_overflow_and_nonzero_underflow_do_not_return_numbers() {
    let data = fixture(25, 0.01, 0.5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let context = doc.coordinate_measurement_context().unwrap();
    for value in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(context.length_on_paper_mm(value).is_err());
        assert!(context.length_at_current_annotation_mm(value).is_err());
    }
    let data = fixture(25, f64::MAX, 1.0);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc
        .coordinate_measurement_context()
        .unwrap()
        .length_on_paper_mm(2.0)
        .is_err());
    let data = fixture(25, 0.01, 1e-200);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc
        .coordinate_measurement_context()
        .unwrap()
        .length_at_current_annotation_mm(f64::MAX)
        .is_err());
    let data = fixture(25, 0.01, 5.0);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let context = doc.coordinate_measurement_context().unwrap();
    assert!(context.length_on_paper_mm(f64::from_bits(1)).is_err());
    assert!(context
        .length_at_current_annotation_mm(f64::from_bits(1))
        .is_err());
    let mut modified = context;
    modified.annotation_ratio = 0.0;
    assert!(modified.length_on_paper_mm(0.0).is_err());
    assert!(modified.length_at_current_annotation_mm(0.0).is_err());
}

#[test]
fn legacy_profile_contexts_match_their_separate_native_holdouts() {
    for marker in [5, 8] {
        for (factor, ratio, drawing, annotation, paper, measured) in [
            (0.01, 0.5, 100.0, 200.0, 10.0, 2000.0),
            (0.04, 1.0, 25.0, 25.0, 40.0, 1000.0),
        ] {
            let data = fixture(marker, factor, ratio);
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            let context = doc.coordinate_measurement_context().unwrap();
            assert_eq!(context.paper_scale_bytes, factor.to_le_bytes());
            assert_eq!(context.annotation_ratio_bytes, ratio.to_le_bytes());
            assert_eq!(context.drawing_denominator, drawing);
            assert_eq!(context.annotation_denominator, annotation);
            assert_eq!(context.length_on_paper_mm(1000.0).unwrap(), paper);
            assert_eq!(
                context.length_at_current_annotation_mm(1000.0).unwrap(),
                measured
            );
            assert_eq!(doc.contents(), data);
            assert_eq!(doc.line_records().count(), 2);
            assert!(doc
                .line_records()
                .all(|line| line.unwrap().coordinate_space() == "record_local"));
        }
    }
}
