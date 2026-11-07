use ezdracad_core::{scan_contents, DracadError, ParseLimits, TextSizeUnit};

fn fixture(marker: u16, selector: u16) -> Vec<u8> {
    let source = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
    };
    let mut data = source.to_vec();
    data[0x9ac..0x9ae].copy_from_slice(&selector.to_le_bytes());
    data
}

#[test]
fn text_size_selectors_preserve_bytes_and_do_not_change_line_coordinates() {
    for (selector, unit, name) in [(0, TextSizeUnit::Mm, "mm"), (1, TextSizeUnit::Pt, "pt")] {
        let mut data = fixture(25, selector);
        // The neighboring bytes are opaque, not part of the u16 selector.
        data[0x9aa..0x9ac].fill(0xa5);
        data[0x9ae..0x9b0].fill(0xff);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let before = doc.line_records().collect::<Result<Vec<_>, _>>().unwrap();
        let info = doc.text_size_unit().unwrap();
        assert_eq!(info.payload_offset, 0x9ac);
        assert_eq!(info.selector_bytes, selector.to_le_bytes());
        assert_eq!(info.raw_selector, selector);
        assert_eq!(info.unit, unit);
        assert_eq!(info.unit.as_str(), name);
        assert_eq!(doc.contents(), data);
        assert_eq!(
            before,
            doc.line_records().collect::<Result<Vec<_>, _>>().unwrap()
        );
        assert_eq!(before[0].start, [-12.5, 0.25]);
        assert_eq!(before[0].coordinate_space(), "record_local");
    }
}

#[test]
fn unqualified_text_size_codes_fail_only_the_requested_api() {
    for selector in [2, 255, 256, 0x8000, u16::MAX] {
        let data = fixture(25, selector);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.text_size_unit(),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert_eq!(doc.line_candidate_count(), 2);
        assert_eq!(doc.line_records().count(), 2);
        assert_eq!(doc.contents()[0x9ac..0x9ae], selector.to_le_bytes());
    }
}

#[test]
fn legacy_profiles_are_not_promoted_by_matching_selector_bytes() {
    for marker in [5, 8] {
        for selector in [0, 1] {
            let data = fixture(marker, selector);
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            assert!(matches!(
                doc.text_size_unit(),
                Err(DracadError::UnsupportedFeature(_))
            ));
            assert_eq!(doc.records().count(), 5);
        }
    }
}
