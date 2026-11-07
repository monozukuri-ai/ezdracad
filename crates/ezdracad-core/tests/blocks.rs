use ezdracad_core::{scan_contents, DracadError, ParseLimits};

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn record(tag: u16, span: u16) -> [u8; 64] {
    let mut data = [0; 64];
    data[..2].copy_from_slice(&tag.to_le_bytes());
    data[8..10].copy_from_slice(&span.to_le_bytes());
    data
}

fn doubles(data: &mut [u8], values: [f64; 6]) {
    for (index, value) in values.into_iter().enumerate() {
        data[16 + index * 8..24 + index * 8].copy_from_slice(&value.to_le_bytes());
    }
}

fn line() -> [u8; 64] {
    let mut data = record(2, 1);
    doubles(&mut data, [1.25, -2.5, 101.75, 48.125, 0.0, 0.0]);
    data
}

fn reference(name: &[u8], placement: [f64; 6]) -> Vec<[u8; 64]> {
    let mut primary = record(6, 2);
    primary[32..64].fill(0xa5);
    primary[32..32 + name.len()].copy_from_slice(name);
    primary[32 + name.len()] = 0;
    let mut continuation = record(0xfe, 0);
    doubles(&mut continuation, placement);
    vec![primary, continuation]
}

fn fixture(marker: u16) -> Vec<u8> {
    let mut root = reference(b"TWO", [-25.0, 75.0, 0.0, 1.0, 1.0, 0.0]);
    root.extend(reference(b"BASE", [100.0, -50.0, 0.0, 2.0, 2.0, 90.0]));
    let mut terminal = record(0xff, 0);
    terminal[16..24].copy_from_slice(b"terminal");
    root.push(terminal);
    let mut boundaries = vec![8192 + root.len() * 64];
    for delta in [
        16320,
        16320,
        2048,
        1024,
        if marker == 5 { 1656 } else { 3312 },
        4096,
    ] {
        boundaries.push(boundaries.last().unwrap() + delta);
    }
    let definitions_start = boundaries[6] + 296;
    let definitions = [
        (
            b"BASE".as_slice(),
            vec![line(), record(0x33, 2), record(0xfe, 0), terminal],
        ),
        (b"TWO".as_slice(), vec![line(), line(), terminal]),
    ];
    let definitions_size: usize = definitions
        .iter()
        .map(|(_, records)| 176 + records.len() * 64)
        .sum();
    let suffix_size = if marker == 25 { 7272 } else { 1768 };
    let payload_end = definitions_start + definitions_size + suffix_size;
    let mut data = vec![0; payload_end + 4];
    data[..18].copy_from_slice(b"DRACAD for Windows");
    data[34..36].copy_from_slice(&marker.to_le_bytes());
    put(&mut data, 36, 8192);
    for (index, offset) in boundaries.into_iter().enumerate() {
        put(&mut data, 40 + index * 4, offset as u32);
    }
    put(&mut data, 76, definitions_start as u32);
    put(&mut data, 80, payload_end as u32);
    put(&mut data, 88, root.len() as u32 - 1);
    for (index, record) in root.iter().enumerate() {
        data[8192 + index * 64..8192 + (index + 1) * 64].copy_from_slice(record);
    }
    let mut cursor = definitions_start;
    for (name, records) in definitions {
        data[cursor..cursor + 32].fill(0xee);
        data[cursor..cursor + name.len()].copy_from_slice(name);
        data[cursor + name.len()] = 0;
        put(&mut data, cursor + 32, 0x073d_bf98); // opaque, never a length.
        put(&mut data, cursor + 36, records.len() as u32 - 1);
        cursor += 176;
        for record in records {
            data[cursor..cursor + 64].copy_from_slice(&record);
            cursor += 64;
        }
    }
    data[cursor..payload_end].fill(0xab); // one shared uninterpreted suffix.
    put(&mut data, payload_end, 0x1234_abcd); // unrelated trailing word stays raw.
    data
}

#[test]
fn multiple_definitions_resolve_by_internal_name_not_reference_order() {
    for marker in [5, 8, 25] {
        let data = fixture(marker);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let library = doc.block_library().unwrap();
        let definitions = library.definitions();
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].name_bytes, b"BASE");
        assert_eq!(definitions[1].name_bytes, b"TWO");
        assert_eq!(definitions[0].record_end, definitions[1].header_offset);
        assert_eq!(library.suffix_offset(), Some(definitions[1].record_end));
        let suffix = library.suffix_bytes().unwrap();
        assert_eq!(suffix.len(), if marker == 25 { 7272 } else { 1768 });
        assert!(suffix.iter().all(|byte| *byte == 0xab));
        let second = library.placed_line_records(0).unwrap();
        assert_eq!(second.len(), 2);
        assert!(second.iter().all(|line| line.definition_index == 1));
        assert_eq!(second[0].start, [-23.75, 72.5]);
        let first = library.placed_line_records(2).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].definition_index, 0);
        assert!((first[0].start[0] - 105.0).abs() < 1e-12);
        assert!((first[0].end[1] - 153.5).abs() < 1e-12);
        assert_eq!(first[0].coordinate_space(), "package_parent");
        assert_eq!(first[0].source.record_index, 0);
        assert_eq!(first[0].source.payload_offset, definitions[0].record_start);
        assert!(library.record(0, 99).is_none());
        assert!(library.record(u64::MAX, 0).is_none());
        assert!(library.resolve(b"missing").is_err());
    }
}

#[test]
fn all_allocations_and_combined_record_work_are_bounded() {
    let data = fixture(25);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.block_library_with_limit(1),
        Err(DracadError::LimitExceeded {
            resource: "block_definitions",
            ..
        })
    ));
    let limits = ParseLimits {
        max_records: 11,
        ..ParseLimits::default()
    };
    let doc = scan_contents(&data, &limits).unwrap();
    assert!(matches!(
        doc.block_library(),
        Err(DracadError::LimitExceeded {
            resource: "records",
            actual: 12,
            ..
        })
    ));
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    assert!(matches!(
        library.planar_line_records_with_limit(1, 1),
        Err(DracadError::LimitExceeded {
            resource: "block_lines",
            ..
        })
    ));
}

#[test]
fn malformed_bounds_and_unknown_layouts_fail_without_resynchronization() {
    let data = fixture(5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    let def = &library.definitions()[0];
    for (offset, value) in [
        (def.header_offset as usize + 36, u32::MAX),
        (def.record_start as usize, 0xff),
        (def.record_end as usize - 64, 2),
    ] {
        let mut bad = data.clone();
        put(&mut bad, offset, value);
        let doc = scan_contents(&bad, &ParseLimits::default()).unwrap();
        assert!(doc.block_library().is_err());
    }
    let mut bad = data.clone();
    let suffix = library.suffix_offset().unwrap();
    put(&mut bad, 68, suffix as u32);
    assert!(matches!(
        scan_contents(&bad, &ParseLimits::default())
            .unwrap()
            .block_library(),
        Err(DracadError::UnsupportedFeature(_))
    ));
    let mut bad = data.clone();
    bad[doc.header().section_offsets[6] as usize] = 1;
    assert!(matches!(
        scan_contents(&bad, &ParseLimits::default())
            .unwrap()
            .block_library(),
        Err(DracadError::UnsupportedFeature(_))
    ));
    let mut bad = data.clone();
    bad[def.header_offset as usize..def.header_offset as usize + 32].fill(b'x');
    assert!(matches!(
        scan_contents(&bad, &ParseLimits::default())
            .unwrap()
            .block_library(),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn ambiguous_names_and_unknown_compounds_are_not_flattened() {
    let data = fixture(5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    let first = &library.definitions()[0];
    let second = &library.definitions()[1];
    let mut ambiguous = data.clone();
    let start = second.header_offset as usize;
    ambiguous[start..start + 32].copy_from_slice(&first.header_bytes[..32]);
    let doc = scan_contents(&ambiguous, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    assert_eq!(library.definitions().len(), 2);
    assert!(matches!(
        library.resolve(b"BASE"),
        Err(DracadError::UnsupportedFeature(_))
    ));
    let mut unknown = data.clone();
    let start = second.record_start as usize;
    unknown[start..start + 2].copy_from_slice(&0x30_u16.to_le_bytes());
    unknown[start + 8..start + 10].copy_from_slice(&2_u16.to_le_bytes());
    let doc = scan_contents(&unknown, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    assert!(library
        .record(1, 1)
        .unwrap()
        .line_record()
        .unwrap()
        .is_some());
    assert!(matches!(
        library.placed_line_records(0),
        Err(DracadError::UnsupportedFeature(_))
    ));
    for (offset, bytes) in [
        (second.header_offset as usize + 40, 1_u64.to_le_bytes()),
        (second.record_start as usize + 48, 12.5_f64.to_le_bytes()),
    ] {
        let mut bad = data.clone();
        bad[offset..offset + 8].copy_from_slice(&bytes);
        let doc = scan_contents(&bad, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.block_library().unwrap().placed_line_records(0),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let mut bad = data.clone();
    bad[second.record_start as usize + 56..second.record_start as usize + 64]
        .copy_from_slice(&f64::NAN.to_le_bytes());
    let doc = scan_contents(&bad, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.block_library().unwrap().placed_line_records(0),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn empty_definitions_do_not_bypass_placement_guards() {
    let mut data = fixture(5);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    let definition = &library.definitions()[1];
    data.drain(definition.record_start as usize..definition.record_end as usize - 64);
    put(&mut data, definition.header_offset as usize + 36, 0);
    let payload_end = data.len() as u32 - 4;
    put(&mut data, 80, payload_end);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    assert!(library.planar_line_records(1).unwrap().is_empty());
    assert!(library.placed_line_records(0).unwrap().is_empty());
    data[8192 + 64 + 48..8192 + 64 + 56].copy_from_slice(&2_f64.to_le_bytes());
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.block_library().unwrap().placed_line_records(0),
        Err(DracadError::UnsupportedFeature(_))
    ));
}
