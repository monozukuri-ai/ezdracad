use ezdracad_core::{scan_contents, DracadError, OriginUnitsCode, ParseLimits};

fn fixture(marker: u16, code: i32) -> Vec<u8> {
    let source = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
    };
    let mut data = source.to_vec();
    data[0x14c..0x150].fill(0xa5);
    data[0x150..0x154].copy_from_slice(&code.to_le_bytes());
    data[0x154..0x158].fill(0x5a);
    data
}

#[test]
fn raw_unknown_and_signed_codes_keep_exact_source_and_geometry() {
    for code in [0, 1, 2, 3, -1, i32::MIN, i32::MAX, 0x0102_0304] {
        let data = fixture(25, code);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let before = doc.line_records().collect::<Result<Vec<_>, _>>().unwrap();
        let value = doc.origin_units_code().unwrap();
        assert_eq!(
            value,
            OriginUnitsCode {
                payload_offset: 0x150,
                selector_bytes: code.to_le_bytes(),
                raw_code: code,
            }
        );
        assert_eq!(&doc.contents()[0x14c..0x150], &[0xa5; 4]);
        assert_eq!(&doc.contents()[0x154..0x158], &[0x5a; 4]);
        assert_eq!(doc.contents(), data);
        assert_eq!(
            before,
            doc.line_records().collect::<Result<Vec<_>, _>>().unwrap()
        );
    }
}

#[test]
fn matching_byte_values_do_not_qualify_legacy_profiles() {
    for marker in [5, 8] {
        let data = fixture(marker, 1);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.origin_units_code(),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert_eq!(doc.contents(), data);
        assert_eq!(doc.line_candidate_count(), 2);
    }
}
