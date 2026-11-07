use ezdracad_core::{scan_contents, DracadError, ParseLimits};

fn fixture(marker: u16) -> Vec<u8> {
    let mut data = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").to_vec(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").to_vec(),
        25 => include_bytes!("../../../tests/fixtures/records25.bin").to_vec(),
        _ => unreachable!(),
    };
    for index in [0, 1] {
        let offset = 8192 + index * 64;
        data[offset..offset + 16].fill(0xa5);
        data[offset..offset + 2].copy_from_slice(&0x0013u16.to_le_bytes());
        for (value, offset) in [1.25_f64, -2.5, 12.5, 101.75, 48.125, -3.75]
            .into_iter()
            .zip((offset + 16..offset + 64).step_by(8))
        {
            data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
    }
    data
}

#[test]
fn independent_endpoint_z_and_exact_sources_preserve_raw_table_order() {
    for marker in [5, 25] {
        let data = fixture(marker);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let lines = doc
            .line_records_3d()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(lines.len(), 2);
        for (index, line) in lines.iter().enumerate() {
            assert_eq!(line.record_index, index as u64);
            assert_eq!(line.payload_offset, (8192 + index * 64) as u64);
            assert_eq!(line.tag_raw, 0x0013);
            assert_eq!(
                line.header_bytes,
                data[8192 + index * 64..8208 + index * 64]
            );
            assert_eq!(line.start, [1.25, -2.5, 12.5]);
            assert_eq!(line.end, [101.75, 48.125, -3.75]);
            assert_eq!(line.coordinate_space(), "record_local");
            assert_eq!(doc.line_record_3d(index as u64).unwrap(), Some(*line));
        }
        assert_eq!(doc.line_record_3d(2).unwrap(), None);
        assert_eq!(doc.line_record_3d(5).unwrap(), None);
        assert_eq!(doc.line_record_3d(u64::MAX).unwrap(), None);
        assert_eq!(doc.contents(), data);
        assert_eq!(doc.line_records().count(), 1);
    }
}

#[test]
fn unobserved_file_profiles_are_rejected_even_for_an_empty_request() {
    let doc = scan_contents(&fixture(8), &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.line_record_3d(u64::MAX),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        doc.line_records_3d(),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert_eq!(doc.records().count(), 5);
}

#[test]
fn nonfinite_recognized_fields_fail_on_demand_without_discarding_raw_data() {
    for offset in [16, 24, 32, 40, 48, 56] {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut data = fixture(5);
            data[8192 + offset..8200 + offset].copy_from_slice(&value.to_le_bytes());
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            assert!(matches!(
                doc.line_record_3d(0),
                Err(DracadError::InvalidFormat(_))
            ));
            assert!(doc.line_record_3d(1).unwrap().is_some());
            assert!(doc.line_record_3d(2).unwrap().is_none());
            assert_eq!(doc.contents(), data);
        }
    }
}

#[test]
fn unknown_flags_are_not_masked_and_negative_zero_is_preserved() {
    for tag in [0x0413u16, 0x0813, 0x2013, 0x4013, 0x8013, 0x0012] {
        let mut data = fixture(5);
        data[8192..8194].copy_from_slice(&tag.to_le_bytes());
        data[8208..8216].copy_from_slice(&f64::NAN.to_le_bytes());
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert_eq!(doc.line_record_3d(0).unwrap(), None);
        assert_eq!(doc.line_records_3d().unwrap().count(), 1);
    }
    let mut data = fixture(5);
    data[8208..8216].copy_from_slice(&(-0.0f64).to_le_bytes());
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.line_record_3d(0).unwrap().unwrap().start[0].is_sign_negative());
}
