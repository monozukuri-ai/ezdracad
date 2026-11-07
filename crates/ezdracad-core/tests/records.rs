use ezdracad_core::{scan_contents, scan_records, DracadError, ObservedProfile, ParseLimits};

const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/records5.bin");
const MPZ: &[u8] = include_bytes!("../../../tests/fixtures/records5.cfb");

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn observed_profiles_preserve_opaque_bytes_and_provisional_xy() {
    for (data, profile) in [
        (CONTENTS, ObservedProfile::Marker5),
        (
            include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
            ObservedProfile::Marker8,
        ),
        (
            include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
            ObservedProfile::Marker25,
        ),
    ] {
        let doc = scan_contents(data, &ParseLimits::default()).unwrap();
        assert_eq!(doc.header().profile, profile);
        assert_eq!(doc.header().record_count, 5);
        assert_eq!(doc.header().declared_nonterminal_records, 4);
        assert_eq!(doc.contents(), data);
        assert_eq!(doc.opaque_record_count(), 2);
        assert_eq!(
            doc.tag_counts()
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect::<Vec<_>>(),
            [(2, 2), (254, 1), (255, 1), (2050, 1)]
        );
        let lines: Vec<_> = doc.line_candidates().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].start, [-12.5, 0.25]);
        assert_eq!(lines[0].end, [42.0, -3.125]);
        assert_eq!(lines[0].payload_offset, 8192);
        assert_eq!(lines[1].record_index, 2);
        assert_eq!(doc.record(1).unwrap().bytes(), &data[8256..8320]);
        assert!(doc.record(1).unwrap().line_candidate().is_none());
        assert!(doc.record(5).is_none());
        assert!(doc.record(u64::MAX).is_none());
        assert_eq!(doc.records().count(), 5);
        assert!(doc.container().is_none());
    }
    let doc = scan_records(MPZ, &ParseLimits::default()).unwrap();
    assert!(doc.container().is_some());
    assert_eq!(doc.contents(), CONTENTS);
    assert_eq!(
        doc.container().unwrap().payload.as_ref().unwrap().sha256,
        doc.sha256()
    );
}

#[test]
fn marker25_is_a_distinct_observed_layout_with_the_same_bounds_contract() {
    let contents = include_bytes!("../../../tests/fixtures/records25.bin");
    let envelope = include_bytes!("../../../tests/fixtures/records25.cfb");
    let doc = scan_records(envelope, &ParseLimits::default()).unwrap();
    assert_eq!(doc.header().profile, ObservedProfile::Marker25);
    assert_eq!(doc.header().profile.as_str(), "observed_marker25_stride64");
    assert_eq!(doc.header().marker_raw, 25);
    assert_eq!(doc.contents(), contents);
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| d.code == "uninterpreted_regions"));
    for marker in [24u16, 26, u16::MAX] {
        let mut data = contents.to_vec();
        data[34..36].copy_from_slice(&marker.to_le_bytes());
        assert!(matches!(
            scan_contents(&data, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_))
        ));
    }
    for (offset, value) in [(0x24, 8191), (0x58, 3), (0x4c, 1)] {
        let mut data = contents.to_vec();
        put(&mut data, offset, value);
        assert!(matches!(
            scan_contents(&data, &ParseLimits::default()),
            Err(DracadError::InvalidFormat(_) | DracadError::UnsupportedFeature(_))
        ));
    }
    for (offset, value) in [
        (0x20, 0x00190001),
        (0x3c, doc.header().section_offsets[5] as u32 + 1),
    ] {
        let mut data = contents.to_vec();
        put(&mut data, offset, value);
        assert!(matches!(
            scan_contents(&data, &ParseLimits::default()),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let limits = ParseLimits {
        max_records: 4,
        ..ParseLimits::default()
    };
    assert!(matches!(
        scan_contents(contents, &limits),
        Err(DracadError::LimitExceeded {
            resource: "records",
            actual: 5,
            limit: 4
        })
    ));
}

#[test]
fn truncated_and_out_of_bounds_tables_fail_closed() {
    for length in [0, 18, 35, 91, 8192, CONTENTS.len() - 1] {
        assert!(scan_contents(&CONTENTS[..length], &ParseLimits::default()).is_err());
    }
    for (offset, value) in [
        (0x24, 0),
        (0x24, u32::MAX),
        (0x28, 8191),
        (0x28, u32::MAX),
        (0x50, u32::MAX),
        (0x58, 3),
        (0x2c, 0),
        (0x30, u32::MAX),
        (0x4c, 1),
    ] {
        let mut data = CONTENTS.to_vec();
        put(&mut data, offset, value);
        assert!(
            matches!(
                scan_contents(&data, &ParseLimits::default()),
                Err(DracadError::InvalidFormat(_))
            ),
            "offset {offset:x}"
        );
    }
}

#[test]
fn unqualified_profiles_are_distinct_from_malformed_data() {
    for (offset, value) in [(0x20, 0x00090000), (0x20, 0x00050001), (0x2c, 24833)] {
        let mut data = CONTENTS.to_vec();
        put(&mut data, offset, value);
        assert!(matches!(
            scan_contents(&data, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_) | DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn terminal_tag_must_be_last_while_its_body_is_retained() {
    for (offset, value) in [(8192, 0xff), (8448, 2)] {
        let mut data = CONTENTS.to_vec();
        data[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
        assert!(matches!(
            scan_contents(&data, &ParseLimits::default()),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let mut data = CONTENTS.to_vec();
    data[8450..8512].fill(0xab);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(doc.record(4).unwrap().bytes(), &data[8448..8512]);
    assert_eq!(doc.line_candidate_count(), 2);
    assert_eq!(
        doc.line_records()
            .collect::<ezdracad_core::Result<Vec<_>>>()
            .unwrap()
            .len(),
        2
    );
    assert!(doc
        .diagnostics()
        .iter()
        .any(|item| item.code == "terminal_data_uninterpreted"));
}

#[test]
fn record_local_lines_preserve_exact_tags_headers_height_and_thickness() {
    let mut data = CONTENTS.to_vec();
    let fields = [-12.5f64, 0.25, 42.0, -3.125, 12.5, 2.5];
    for (index, tag) in [(0, 0x0002u16), (1, 0x0402)] {
        let start = 8192 + index * 64;
        data[start..start + 16].fill(0xa5);
        data[start..start + 2].copy_from_slice(&tag.to_le_bytes());
        for (field, value) in fields.iter().enumerate() {
            data[start + 16 + field * 8..start + 24 + field * 8]
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    // A different flagged tag must not be accepted by masking its low byte.
    data[8320..8322].copy_from_slice(&0x0802u16.to_le_bytes());
    data[8336..8384].fill(0xff);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(doc.line_candidate_count(), 1);
    assert_eq!(doc.opaque_record_count(), 3);
    let lines = doc
        .line_records()
        .collect::<ezdracad_core::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(lines.len(), 2);
    for (index, line) in lines.iter().enumerate() {
        assert_eq!(line.record_index, index as u64);
        assert_eq!(line.payload_offset, (8192 + index * 64) as u64);
        assert_eq!(
            line.header_bytes,
            data[8192 + index * 64..8208 + index * 64]
        );
        assert_eq!(line.tag_raw, if index == 0 { 2 } else { 0x0402 });
        assert_eq!(line.start, [-12.5, 0.25]);
        assert_eq!(line.end, [42.0, -3.125]);
        assert_eq!(line.elevation, 12.5);
        assert_eq!(line.thickness, 2.5);
        assert_eq!(line.coordinate_space(), "record_local");
    }
    assert!(doc.record(1).unwrap().line_candidate().is_none());
    assert!(doc.record(2).unwrap().line_record().unwrap().is_none());
    data[8320..8322].copy_from_slice(&0x0013u16.to_le_bytes());
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.record(2).unwrap().line_record().unwrap().is_none());
}

#[test]
fn line_field_validation_is_on_demand_without_changing_raw_xy_validation() {
    for tag in [0x0002u16, 0x0402] {
        for offset in [16, 24, 32, 40, 48, 56] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut data = CONTENTS.to_vec();
                data[8192..8194].copy_from_slice(&tag.to_le_bytes());
                data[8192 + offset..8200 + offset].copy_from_slice(&value.to_le_bytes());
                let result = scan_contents(&data, &ParseLimits::default());
                if tag == 2 && offset < 48 {
                    assert!(matches!(result, Err(DracadError::InvalidFormat(_))));
                } else {
                    let doc = result.unwrap();
                    assert!(matches!(
                        doc.record(0).unwrap().line_record(),
                        Err(DracadError::InvalidFormat(_))
                    ));
                    assert!(doc.line_records().next().unwrap().is_err());
                    // Unrelated records stay readable after a field-decoder failure.
                    assert!(doc.record(2).unwrap().line_record().unwrap().is_some());
                    assert_eq!(doc.line_candidate_count(), if tag == 2 { 2 } else { 1 });
                }
            }
        }
    }
}

#[test]
fn nonfinite_xy_is_rejected_only_in_exact_tag_two() {
    for offset in [8208, 8216, 8224, 8232] {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut data = CONTENTS.to_vec();
            data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            assert!(matches!(
                scan_contents(&data, &ParseLimits::default()),
                Err(DracadError::InvalidFormat(_))
            ));
            // Flagged/unknown tags remain opaque, including arbitrary floating bits.
            data[8192..8194].copy_from_slice(&0x0802u16.to_le_bytes());
            assert!(scan_contents(&data, &ParseLimits::default()).is_ok());
        }
    }
}

#[test]
fn counts_are_limited_before_scanning_or_allocating_records() {
    let limits = ParseLimits {
        max_records: 4,
        ..ParseLimits::default()
    };
    assert!(matches!(
        scan_contents(CONTENTS, &limits),
        Err(DracadError::LimitExceeded {
            resource: "records",
            limit: 4,
            actual: 5
        })
    ));
    let mut data = CONTENTS.to_vec();
    put(&mut data, 0x58, u32::MAX);
    assert!(matches!(
        scan_contents(&data, &ParseLimits::default()),
        Err(DracadError::LimitExceeded {
            actual: 4_294_967_296,
            ..
        })
    ));
    let limits = ParseLimits {
        max_records: u64::MAX,
        ..ParseLimits::default()
    };
    assert!(matches!(
        scan_contents(&data, &limits),
        Err(DracadError::InvalidFormat(_))
    ));
    let limits = ParseLimits {
        max_stream_size: 1,
        ..ParseLimits::default()
    };
    assert!(matches!(
        scan_contents(CONTENTS, &limits),
        Err(DracadError::LimitExceeded {
            resource: "stream_size",
            ..
        })
    ));
}

#[test]
fn low_level_contents_are_not_mpx_file_support() {
    assert!(matches!(
        scan_records(CONTENTS, &ParseLimits::default()),
        Err(DracadError::UnsupportedFormat(_) | DracadError::InvalidFormat(_))
    ));
}
