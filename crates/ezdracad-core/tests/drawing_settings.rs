use ezdracad_core::{scan_contents, DracadError, ParseLimits};

fn fixture(marker: u16, origin: [f64; 2], factor: f64) -> Vec<u8> {
    let source = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
    };
    let mut data = source.to_vec();
    data[0x140..0x180].fill(0xa5);
    for (offset, value) in [(0x140, origin[0]), (0x148, origin[1]), (0x178, factor)] {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    data
}

#[test]
fn observed_settings_preserve_opaque_header_bytes_and_do_not_transform_lines() {
    for marker in [5, 8, 25] {
        let data = fixture(marker, [-12.5, 250.25], 0.04);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let before = doc.line_records().collect::<Result<Vec<_>, _>>().unwrap();
        let settings = doc.drawing_settings().unwrap();
        assert_eq!(settings.payload_offset, 0x140);
        assert_eq!(settings.drawing_origin, [-12.5, 250.25]);
        assert_eq!(settings.paper_scale_factor, 0.04);
        assert_eq!(settings.drawing_scale, 25.0);
        assert_eq!(settings.header_bytes, data[0x140..0x180]);
        assert_eq!(settings.header_bytes[16..56], [0xa5; 40]);
        assert_eq!(
            before,
            doc.line_records().collect::<Result<Vec<_>, _>>().unwrap()
        );
        assert_eq!(doc.contents(), data);
    }
}

#[test]
fn invalid_origin_and_scale_fields_fail_on_request_without_changing_scan() {
    for (offset, values) in [
        (0x140, vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY]),
        (0x148, vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY]),
        (
            0x178,
            vec![
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                0.0,
                -0.0,
                -0.01,
                f64::from_bits(1), // Finite and positive, but reciprocal overflows.
            ],
        ),
    ] {
        for value in values {
            let mut data = fixture(5, [100.0, -50.0], 0.01);
            data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            assert_eq!(doc.line_candidate_count(), 2);
            assert!(matches!(
                doc.drawing_settings(),
                Err(DracadError::InvalidFormat(_))
            ));
            assert_eq!(doc.records().count(), 5);
        }
    }
}

#[test]
fn signed_zero_origin_and_extreme_finite_reciprocals_are_retained() {
    for factor in [f64::MIN_POSITIVE, f64::MAX] {
        let data = fixture(25, [-0.0, 0.0], factor);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let settings = doc.drawing_settings().unwrap();
        assert!(settings.drawing_origin[0].is_sign_negative());
        assert!(settings.drawing_origin[1].is_sign_positive());
        assert_eq!(settings.paper_scale_factor, factor);
        assert!(settings.drawing_scale.is_finite() && settings.drawing_scale > 0.0);
    }
}
