use ezdracad_core::{
    scan_contents, DracadError, InlinePackageLimits, InlineTableDomain, ParseLimits,
    MAX_INLINE_PACKAGE_DEPTH, MAX_INLINE_PACKAGE_PROVENANCE,
};

fn slot(tag: u16, span: u16) -> [u8; 64] {
    let mut result = [0; 64];
    result[..2].copy_from_slice(&tag.to_le_bytes());
    result[8..10].copy_from_slice(&span.to_le_bytes());
    result
}

fn package(span: u16) -> [u8; 64] {
    let mut result = slot(0x30, span);
    result[2..8].copy_from_slice(&[17, 0, 2, 3, 4, 5]);
    result[10..16].copy_from_slice(&[0x34, 0x12, 0xa5, 0x5a, 0x42, 0x81]);
    result[16..20].copy_from_slice(&0x400u32.to_le_bytes());
    result
}

fn line(tag: u16) -> [u8; 64] {
    let mut result = slot(tag, 1);
    result[2..8].copy_from_slice(&[8, 0, 9, 10, 11, 12]);
    for (index, value) in [1.25f64, -2.5, 101.75, 48.125, 12.5, 2.5]
        .into_iter()
        .enumerate()
    {
        result[16 + index * 8..24 + index * 8].copy_from_slice(&value.to_le_bytes());
    }
    result
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn contents(slots: &[[u8; 64]], marker: u16) -> Vec<u8> {
    let original: &[u8] = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin"),
        8 => include_bytes!("../../../tests/fixtures/records8.bin"),
        _ => include_bytes!("../../../tests/fixtures/records25.bin"),
    };
    let mut data = original.to_vec();
    let shift = (slots.len() as i64 + 1 - 5) * 64;
    for offset in (0x28..=0x50).step_by(4) {
        let old = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        if old != 0 {
            put(&mut data, offset, (i64::from(old) + shift) as u32);
        }
    }
    put(&mut data, 0x58, slots.len() as u32);
    let mut records = slots.concat();
    records.extend_from_slice(&slot(0xff, 0));
    data.splice(8192..8192 + 5 * 64, records);
    data
}

fn nested() -> Vec<[u8; 64]> {
    vec![package(4), package(2), line(0x402), line(2)]
}

#[test]
fn nested_lines_preserve_every_ancestor_and_all_record_local_values() {
    for marker in [5, 8, 25] {
        let slots = nested();
        let data = contents(&slots, marker);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let lines = doc.inline_package_lines(0).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].source.record_index, 2);
        assert_eq!(lines[1].source.record_index, 3);
        assert_eq!(
            lines[0]
                .ancestors
                .iter()
                .map(|a| a.record_index)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(
            lines[1]
                .ancestors
                .iter()
                .map(|a| a.record_index)
                .collect::<Vec<_>>(),
            [0]
        );
        assert_eq!(lines[0].ancestors[0].record_count, 4);
        assert_eq!(lines[0].ancestors[1].record_count, 2);
        assert_eq!(lines[0].ancestors[0].header_bytes, slots[0][..16]);
        assert_eq!(lines[0].ancestors[1].body_bytes, slots[1][16..]);
        for value in &lines {
            assert_eq!(value.table_domain, InlineTableDomain::RootRecords);
            assert_eq!(value.table_domain.as_str(), "root_records");
            assert_eq!(value.definition_index, None);
            assert_eq!(value.source.coordinate_space(), "record_local");
            assert_eq!(value.source.start, [1.25, -2.5]);
            assert_eq!(
                (value.source.elevation, value.source.thickness),
                (12.5, 2.5)
            );
            assert_eq!(
                value.source.payload_offset,
                8192 + value.source.record_index * 64
            );
        }
        // Legacy flat APIs retain their contract and complete original bytes.
        assert_eq!(doc.line_records().count(), 2);
        assert_eq!(doc.contents(), data);
    }
}

#[test]
fn selection_cannot_drop_an_ancestor_or_reinterpret_unrelated_spans() {
    let data = contents(&nested(), 5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for index in [1, 2, 3] {
        assert!(matches!(
            doc.inline_package_lines(index),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    for index in [4, 100] {
        assert!(matches!(
            doc.inline_package_lines(index),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let data = contents(
        &[
            slot(0x7777, 2),
            slot(0xfe, 0),
            package(2),
            line(2),
            slot(0x7778, 0),
        ],
        5,
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(
        doc.inline_package_lines(2).unwrap()[0].source.record_index,
        3
    );
    assert!(doc.object_spans().collect::<Result<Vec<_>, _>>().is_err());
    assert!(matches!(
        doc.inline_package_lines(1),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn unknown_children_variants_and_state_fields_fail_whole_requests() {
    let mut variants = vec![slot(0x7777, 1), slot(0x33, 1), slot(0x430, 1), package(1)];
    let mut nonzero = package(2);
    nonzero[20] = 1;
    variants.push(nonzero);
    let mut other_kind = package(2);
    other_kind[16..20].copy_from_slice(&3u32.to_le_bytes());
    variants.push(other_kind);
    for variant in variants {
        let span = u16::from_le_bytes(variant[8..10].try_into().unwrap());
        let mut slots = vec![package(2 + span), line(2), variant];
        for _ in 1..span {
            slots.push(line(2));
        }
        let data = contents(&slots, 5);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.inline_package_lines(0),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn malformed_children_and_prefixes_are_errors_without_resynchronization() {
    for child in [slot(0xfe, 0), slot(0x7777, 0), slot(0x7777, 3)] {
        let data = contents(&[package(3), line(2), child], 5);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.inline_package_lines(0),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let data = contents(&[slot(0x7777, 0), package(2), line(2)], 5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.inline_package_lines(1),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn all_six_line_fields_are_checked_on_request() {
    for offset in (16..64).step_by(8) {
        let mut bad = line(0x402);
        bad[offset..offset + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        let data = contents(&[package(3), line(2), bad], 5);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.inline_package_lines(0),
            Err(DracadError::InvalidFormat(_))
        ));
    }
}

#[test]
fn depth_node_line_and_absolute_provenance_budgets_are_independent() {
    let data = contents(&nested(), 5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for (resource, limits) in [
        (
            "inline_package_depth",
            InlinePackageLimits {
                max_depth: 1,
                ..Default::default()
            },
        ),
        (
            "inline_package_nodes",
            InlinePackageLimits {
                max_nodes: 3,
                ..Default::default()
            },
        ),
        (
            "inline_package_lines",
            InlinePackageLimits {
                max_lines: 1,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(doc.inline_package_lines_with_limits(0, &limits), Err(DracadError::LimitExceeded { resource: name, .. }) if name==resource)
        );
    }
    let mut slots = Vec::new();
    for count in (2..=130).rev() {
        slots.push(package(count));
    }
    slots.push(line(2));
    let data = contents(&slots, 5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.inline_package_lines_with_limits(
            0,
            &InlinePackageLimits {
                max_depth: 1000,
                ..Default::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "inline_package_depth",
            limit: MAX_INLINE_PACKAGE_DEPTH,
            actual: 129
        })
    ));
    let mut slots = Vec::new();
    for index in 0..128 {
        slots.push(package((128 + 1025 - index) as u16));
    }
    slots.extend(vec![line(2); 1025]);
    let data = contents(&slots, 5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.inline_package_lines_with_limits(
            0,
            &InlinePackageLimits {
                max_depth: 128,
                max_nodes: 2000,
                max_lines: 2000
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "inline_package_provenance",
            limit: MAX_INLINE_PACKAGE_PROVENANCE,
            ..
        })
    ));
}

#[test]
fn block_definition_domain_and_snapshot_keep_the_direct_only_contract() {
    let slots = [line(2), package(4), package(2), line(0x402), line(2)];
    let mut data = contents(&[line(2)], 5);
    let start = u32::from_le_bytes(data[0x40..0x44].try_into().unwrap()) as usize;
    data.truncate(start);
    data.extend(vec![0; 296]);
    let definition_start = data.len();
    let mut header = [0; 176];
    header[..4].copy_from_slice(b"PKG\0");
    put(&mut header, 36, slots.len() as u32);
    data.extend(header);
    let record_start = data.len();
    data.extend(slots.concat());
    data.extend(slot(0xff, 0));
    data.extend(vec![0xab; 1768]);
    let payload_end = data.len();
    data.extend([0; 4]);
    put(&mut data, 0x4c, definition_start as u32);
    put(&mut data, 0x50, payload_end as u32);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    let lines = library.inline_package_lines(0, 1).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0].table_domain,
        InlineTableDomain::BlockDefinitionRecords
    );
    assert_eq!(lines[0].definition_index, Some(0));
    assert_eq!(
        lines[0]
            .ancestors
            .iter()
            .map(|a| a.record_index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(lines[0].source.payload_offset, record_start as u64 + 3 * 64);
    assert!(matches!(
        library.inline_package_lines(0, 2),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        library.planar_line_records(0),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert!(library.inline_package_lines(2, 0).is_err());
}
