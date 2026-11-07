use ezdracad_core::{
    decode_record_attributes, scan_contents, AttributeReferenceKind, DracadError, ParseLimits,
};

fn line(tag: u16) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    bytes[..2].copy_from_slice(&tag.to_le_bytes());
    bytes[2..4].copy_from_slice(&256u16.to_le_bytes());
    bytes[4..8].copy_from_slice(&[2, 32, 16, 3]);
    bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
    bytes[10..12].copy_from_slice(&257u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&[0xa5, 0x5a, 0x42, 0x81]);
    if matches!(tag, 0x2402 | 0x4402) {
        bytes[4] = 0;
    }
    for (i, v) in [1.25f64, -2.5, 101.75, 48.125, 12.5, 2.5]
        .iter()
        .enumerate()
    {
        bytes[16 + i * 8..24 + i * 8].copy_from_slice(&v.to_le_bytes());
    }
    bytes
}

fn contents(slots: &[[u8; 64]], marker: u16) -> Vec<u8> {
    let mut data = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").to_vec(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").to_vec(),
        _ => include_bytes!("../../../tests/fixtures/records25.bin").to_vec(),
    };
    let shift = (slots.len() as i64 + 1 - 5) * 64;
    for o in (0x28..=0x50).step_by(4) {
        let old = u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        if old != 0 {
            data[o..o + 4].copy_from_slice(&((i64::from(old) + shift) as u32).to_le_bytes());
        }
    }
    data[0x58..0x5c].copy_from_slice(&(slots.len() as u32).to_le_bytes());
    let mut bytes = slots.concat();
    let mut terminal = [0; 64];
    terminal[..2].copy_from_slice(&255u16.to_le_bytes());
    bytes.extend(terminal);
    data.splice(8192..8192 + 5 * 64, bytes);
    data
}

#[test]
fn explicit_indices_raw_fields_and_opaque_tail_are_retained() {
    for tag in [2, 0x402, 0x30, 6] {
        let record = line(tag);
        let attrs = decode_record_attributes(&record[..16]).unwrap();
        assert_eq!(attrs.tag_raw, tag);
        assert_eq!(attrs.header_bytes, record[..16]);
        assert_eq!(attrs.layer_index, 256);
        assert_eq!(attrs.color.index, Some(2));
        assert_eq!(attrs.color.raw_code, 2);
        assert_eq!(attrs.line_type.index, Some(32));
        assert_eq!(attrs.width.index, Some(16));
        assert_eq!(attrs.material_index_raw, 3);
        assert_eq!(attrs.group_index, 257);
        assert_eq!(attrs.flags_raw, (tag >> 8) as u8);
        assert_eq!(attrs.opaque_tail_bytes, [0xa5, 0x5a, 0x42, 0x81]);
        assert!(!attrs.has_dependencies());
    }
    let mut bytes = line(2);
    bytes[4] = 0;
    bytes[2..4].fill(0);
    let attrs = decode_record_attributes(&bytes[..16]).unwrap();
    assert_eq!(attrs.color.index, Some(256));
    assert_eq!(attrs.color.kind, AttributeReferenceKind::Index);
    assert_eq!(attrs.layer_index, 0);
}

#[test]
fn native_dependency_selectors_do_not_become_dxf_or_effective_values() {
    for (tag, kind) in [
        (0x2402, AttributeReferenceKind::ByLayer),
        (0x4402, AttributeReferenceKind::ByBlock),
    ] {
        let mut bytes = line(tag);
        bytes[5] = 0;
        bytes[6] = 255;
        let attrs = decode_record_attributes(&bytes[..16]).unwrap();
        assert_eq!(attrs.color.kind, kind);
        assert_eq!(attrs.color.index, None);
        assert_eq!(attrs.color.raw_code, 0);
        assert_eq!(attrs.line_type.kind, AttributeReferenceKind::ByLayer);
        assert_eq!(attrs.line_type.raw_code, 0);
        assert_eq!(attrs.width.kind, AttributeReferenceKind::ByBlock);
        assert_eq!(attrs.width.raw_code, 255);
        assert_eq!(attrs.width.kind.as_str(), "by_block");
        assert!(attrs.has_dependencies());
    }
}

#[test]
fn unknown_tags_flags_indices_and_inconsistent_color_flags_fail_closed() {
    for tag in [0x802, 0x2002, 0x4002, 0x6402, 0x2430, 0x13] {
        let record = line(tag);
        assert!(matches!(
            decode_record_attributes(&record[..16]),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    for (offset, value) in [(5, 33), (5, 254), (6, 17), (6, 254)] {
        let mut bytes = line(2);
        bytes[offset] = value;
        assert!(matches!(
            decode_record_attributes(&bytes[..16]),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let mut bytes = line(0x2402);
    bytes[4] = 2;
    assert!(matches!(
        decode_record_attributes(&bytes[..16]),
        Err(DracadError::UnsupportedFeature(_))
    ));
    bytes = line(2);
    bytes[2..4].copy_from_slice(&257u16.to_le_bytes());
    assert!(decode_record_attributes(&bytes[..16]).is_err());
    for size in [0, 15, 17, 64] {
        assert!(matches!(
            decode_record_attributes(&bytes[..size]),
            Err(DracadError::InvalidFormat(_))
        ));
    }
}

#[test]
fn broader_attributed_line_api_leaves_legacy_exact_tag_contracts_unchanged() {
    let slots = [
        line(2),
        line(0x402),
        line(0x2402),
        line(0x4402),
        line(0x802),
    ];
    let data = contents(&slots, 25);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let lines = doc
        .attributed_line_records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(lines.len(), 4);
    assert_eq!(doc.line_records().count(), 2);
    assert_eq!(doc.line_candidate_count(), 1);
    assert_eq!(lines[2].source.tag_raw, 0x2402);
    assert_eq!(lines[3].source.record_index, 3);
    for line in lines {
        assert_eq!(line.source.coordinate_space(), "record_local");
        assert_eq!(line.source.header_bytes, line.attributes.header_bytes);
        assert_eq!(line.source.start, [1.25, -2.5]);
        assert_eq!((line.source.elevation, line.source.thickness), (12.5, 2.5));
    }
    assert!(doc.attributed_line_record(5).unwrap().is_none());
    assert!(doc.attributed_line_record(6).unwrap().is_none());
    assert!(doc.record_attributes(6).unwrap().is_none());
    assert!(doc.record_attributes(5).is_err());
}

#[test]
fn older_profiles_reject_dependency_encodings_but_retain_explicit_references() {
    for marker in [5, 8] {
        let data = contents(&[line(2), line(0x2402)], marker);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert_eq!(doc.record_attributes(0).unwrap().unwrap().layer_index, 256);
        assert!(matches!(
            doc.record_attributes(1),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert!(doc
            .attributed_line_records()
            .collect::<Result<Vec<_>, _>>()
            .is_err());
        assert_eq!(doc.line_records().count(), 1);
        // The explicit standalone header decoder has no surrounding profile.
        assert!(doc
            .record(1)
            .unwrap()
            .attributed_line_record()
            .unwrap()
            .is_some());
        for offset in [5, 6] {
            let mut bytes = line(2);
            bytes[offset] = 0;
            let data = contents(&[bytes], marker);
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            assert!(doc.record_attributes(0).is_err());
            assert!(doc.attributed_line_record(0).is_err());
            assert_eq!(doc.line_records().count(), 1);
        }
    }
}

#[test]
fn all_six_fields_of_colored_line_variants_are_finite_on_request() {
    for offset in (16..64).step_by(8) {
        let mut bad = line(0x4402);
        bad[offset..offset + 8].copy_from_slice(&f64::INFINITY.to_le_bytes());
        let data = contents(&[line(2), bad], 25);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.attributed_line_record(1),
            Err(DracadError::InvalidFormat(_))
        ));
        assert!(doc
            .attributed_line_records()
            .collect::<Result<Vec<_>, _>>()
            .is_err());
        assert_eq!(doc.line_records().count(), 1);
    }
}

#[test]
fn inline_packages_accept_qualified_colored_children_without_resolving_styles() {
    let mut package = line(0x30);
    package[8..10].copy_from_slice(&2u16.to_le_bytes());
    package[16..].fill(0);
    package[16..20].copy_from_slice(&0x400u32.to_le_bytes());
    let data = contents(&[package, line(0x2402)], 25);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let source = doc.inline_package_lines(0).unwrap();
    assert_eq!(source[0].source.tag_raw, 0x2402);
    let attrs = decode_record_attributes(&source[0].source.header_bytes).unwrap();
    assert_eq!(attrs.color.kind, AttributeReferenceKind::ByLayer);
    let data = contents(&[package, line(0x2402)], 5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.inline_package_lines(0).is_err());
    let mut unknown = line(0x4402);
    unknown[5] = 254;
    let data = contents(&[package, unknown], 25);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.inline_package_lines(0).is_err());
}
