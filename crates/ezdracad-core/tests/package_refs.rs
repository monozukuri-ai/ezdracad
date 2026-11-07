use ezdracad_core::{scan_contents, DracadError, PackageReference2D, ParseLimits};

const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/records25.bin");

fn package(data: &mut [u8], index: usize, name: &[u8], fields: [f64; 6]) {
    let offset = 8192 + index * 64;
    data[offset..offset + 64].fill(0xa5);
    data[offset..offset + 2].copy_from_slice(&6u16.to_le_bytes());
    data[offset + 8..offset + 10].copy_from_slice(&2u16.to_le_bytes());
    data[offset + 16..offset + 32].fill(0);
    data[offset + 32..offset + 32 + name.len()].copy_from_slice(name);
    data[offset + 32 + name.len()] = 0;
    if index + 1 < 4 {
        let offset = offset + 64;
        data[offset..offset + 64].fill(0x11);
        data[offset..offset + 2].copy_from_slice(&0xfeu16.to_le_bytes());
        for (index, value) in fields.into_iter().enumerate() {
            data[offset + 16 + index * 8..offset + 24 + index * 8]
                .copy_from_slice(&value.to_le_bytes());
        }
    }
}

fn sample() -> Vec<u8> {
    let mut data = CONTENTS.to_vec();
    package(
        &mut data,
        0,
        b"native-~KKSrpX",
        [100.0, -50.0, 0.0, 2.0, 2.0, 90.0],
    );
    data
}

fn reference() -> PackageReference2D {
    scan_contents(&sample(), &ParseLimits::default())
        .unwrap()
        .package_reference(0)
        .unwrap()
        .unwrap()
}

#[test]
fn exact_reference_fields_and_garbage_after_nul_are_retained() {
    let data = sample();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let reference = doc.package_reference(0).unwrap().unwrap();
    assert_eq!(reference.record_index, 0);
    assert_eq!(reference.payload_offset, 8192);
    assert_eq!(reference.continuation_index, 1);
    assert_eq!(reference.continuation_offset, 8256);
    assert_eq!(reference.tag_raw, 6);
    assert_eq!(reference.header_bytes, data[8192..8208]);
    assert_eq!(reference.name_field_bytes, data[8224..8256]);
    assert_eq!(reference.name_key(), b"native-~KKSrpX");
    assert!(reference.name_field_bytes[14..].contains(&0xa5));
    assert_eq!(reference.continuation_header_bytes, data[8256..8272]);
    assert_eq!(reference.anchor_raw, [0.0, 0.0]);
    assert_eq!(reference.placement_raw, [100.0, -50.0, 0.0, 2.0, 2.0, 90.0]);
    assert_eq!(reference.coordinate_space(), "package_parent");
    assert!(doc.package_reference(1).unwrap().is_none());
    assert!(doc.package_reference(5).unwrap().is_none());
    assert!(doc.package_reference(u64::MAX).unwrap().is_none());
    assert_eq!(doc.package_references().count(), 1);
    assert_eq!(doc.line_candidate_count(), 1);
    // Names are byte keys, including when no text encoding is known.
    let mut data = sample();
    package(&mut data, 0, &[0x82, 0xa0], [0.0, 0.0, 0.0, 1.0, 1.0, 0.0]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(
        doc.package_reference(0).unwrap().unwrap().name_key(),
        &[0x82, 0xa0]
    );
}

#[test]
fn separate_reference_occurrences_preserve_slot_order() {
    let mut data = sample();
    package(
        &mut data,
        2,
        b"native-~KKSrpX",
        [100.0, -50.0, 0.0, 2.0, 2.0, 90.0],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let references = doc
        .package_references()
        .collect::<ezdracad_core::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        references
            .iter()
            .map(|r| r.record_index)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(references[0].name_key(), references[1].name_key());
    assert_eq!(references[0].placement_raw, references[1].placement_raw);
}

#[test]
fn unsupported_spans_and_malformed_continuations_or_names_fail_on_request() {
    for span in [0u16, 1, 3, u16::MAX] {
        let mut data = sample();
        data[8200..8202].copy_from_slice(&span.to_le_bytes());
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.package_reference(0),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    for tag in [0u16, 0xfd, 0x04fe] {
        let mut data = sample();
        data[8256..8258].copy_from_slice(&tag.to_le_bytes());
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.package_reference(0),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    for empty in [false, true] {
        let mut data = sample();
        data[8224..8256].fill(if empty { 0 } else { b'X' });
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.package_reference(0),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let mut data = sample();
    package(&mut data, 3, b"TRUNCATED", [0.0; 6]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.package_reference(0).unwrap().is_some());
    assert!(matches!(
        doc.package_reference(3),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn flags_are_not_masked_and_numeric_checks_do_not_change_raw_scan() {
    for tag in [0x0406u16, 0x0806, 0x0013] {
        let mut data = sample();
        data[8192..8194].copy_from_slice(&tag.to_le_bytes());
        data[8208..8224].fill(0xff);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(doc.package_reference(0).unwrap().is_none());
        assert_eq!(doc.package_references().count(), 0);
    }
    for offset in [8208, 8216, 8272, 8280, 8288, 8296, 8304, 8312] {
        for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut data = sample();
            data[offset..offset + 8].copy_from_slice(&number.to_le_bytes());
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            assert!(matches!(
                doc.package_reference(0),
                Err(DracadError::InvalidFormat(_))
            ));
            assert!(doc.package_references().next().unwrap().is_err());
            assert_eq!(doc.line_candidate_count(), 1);
        }
    }
}

#[test]
fn verified_planar_calculation_has_known_results_but_rejects_other_states() {
    let reference = reference();
    for (point, wanted) in [
        ([1.25, -2.5], [105.0, -47.5]),
        ([101.75, 48.125], [3.75, 153.5]),
    ] {
        let actual = reference.apply_xy(point).unwrap();
        assert!(actual
            .into_iter()
            .zip(wanted)
            .all(|(a, b)| (a - b).abs() < 1e-12));
    }
    let mut rotated = reference;
    rotated.placement_raw[5] = 45.0;
    let actual = rotated.apply_xy([1.0, 0.0]).unwrap();
    assert!((actual[0] - (100.0 + 2.0f64.sqrt())).abs() < 1e-12);
    assert!((actual[1] - (-50.0 + 2.0f64.sqrt())).abs() < 1e-12);
    for axis in 0..2 {
        let mut other = reference;
        other.anchor_raw[axis] = 1.0;
        assert!(matches!(
            other.apply_xy([0.0, 0.0]),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    for fields in [
        [100.0, -50.0, -10.0, 2.0, 2.0, 90.0],
        [100.0, -50.0, 0.0, 2.0, 3.0, 90.0],
        [100.0, -50.0, 0.0, 0.0, 0.0, 90.0],
        [100.0, -50.0, 0.0, -2.0, -2.0, 90.0],
    ] {
        let mut other = reference;
        other.placement_raw = fields;
        assert!(matches!(
            other.apply_xy([0.0, 0.0]),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let mut large = reference;
    large.placement_raw[5] = 0.0;
    assert!(matches!(
        large.apply_xy([f64::MAX, 0.0]),
        Err(DracadError::InvalidFormat(_))
    ));
    for point in [
        [f64::NAN, 0.0],
        [0.0, f64::INFINITY],
        [f64::NEG_INFINITY, 0.0],
    ] {
        assert!(matches!(
            reference.apply_xy(point),
            Err(DracadError::InvalidFormat(_))
        ));
    }
}
