use ezdracad_core::{scan_contents, DracadError, ParseLimits, RawStyleTables};

fn word(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn dword(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn layer(index: u16, size: usize) -> Vec<u8> {
    let mut row = vec![0xA5; size];
    for (offset, value) in [(0, 0x400), (2, 0x7788), (4, 7), (6, 2), (8, 5), (10, index)] {
        word(&mut row, offset, value);
    }
    row
}

fn color(index: u16) -> [u8; 64] {
    let mut row = [0xAA; 64];
    word(&mut row, 0, 0x1234);
    word(&mut row, 2, index * 2);
    row[4..7].copy_from_slice(&[(index % 256) as u8, 17, 250]);
    row[7] = 2;
    row[8..11].copy_from_slice(&[11, 22, 33]);
    row[11] = 0x9A; // Encoding remains uninterpreted, not an RGB-policy guess.
    row
}

/// Independently assembled style tables; these bytes are not a native drawing.
fn fixture(marker: u16) -> Vec<u8> {
    let mut bounds = vec![8192 + 64];
    for delta in [
        16320,
        16320,
        2048,
        1024,
        if marker == 5 { 1656 } else { 3312 },
        4096,
    ] {
        bounds.push(bounds.last().unwrap() + delta);
    }
    let suffix = bounds[6] + 296;
    let end = suffix + if marker == 25 { 7272 } else { 1768 };
    let mut data = vec![0; end + 4];
    data[..18].copy_from_slice(b"DRACAD for Windows");
    word(&mut data, 0x22, marker);
    dword(&mut data, 0x24, 8192);
    for (i, value) in bounds.iter().enumerate() {
        dword(&mut data, 0x28 + i * 4, *value as u32);
    }
    dword(&mut data, 0x50, end as u32);
    word(&mut data, 8192, 255);
    for i in 1u16..=255 {
        let offset = bounds[0] + usize::from(i - 1) * 64;
        data[offset..offset + 64].copy_from_slice(&layer(i, 64));
        let offset = bounds[1] + usize::from(i - 1) * 64;
        data[offset..offset + 64].copy_from_slice(&color(i));
    }
    for i in 1u16..=32 {
        let offset = bounds[2] + usize::from(i - 1) * 64;
        word(&mut data, offset, 0x43);
        word(&mut data, offset + 2, i - 1);
        word(&mut data, offset + 4, 125);
        word(&mut data, offset + 8, 250); // Internal zero is retained.
    }
    for i in 1u16..=16 {
        let offset = bounds[3] + usize::from(i - 1) * 64;
        data[offset..offset + 64].fill(0x7F);
        word(&mut data, offset, 0x9876);
        word(&mut data, offset + 2, i * 25);
    }
    for (relative, words) in [
        (0, [2, 256, 0, 64]),
        (80, [1, 0, 0, 296]),
        (392, [1, 256, 0, 296]),
    ] {
        for (i, value) in words.into_iter().enumerate() {
            dword(&mut data, suffix + relative + 4 * i, value);
        }
    }
    data[suffix + 16..suffix + 80].copy_from_slice(&color(256));
    data[suffix + 96..suffix + 392].copy_from_slice(&layer(0, 296));
    data[suffix + 408..suffix + 704].copy_from_slice(&layer(256, 296));
    data
}

fn parse(data: &[u8]) -> RawStyleTables {
    RawStyleTables::parse(&scan_contents(data, &ParseLimits::default()).unwrap()).unwrap()
}

#[test]
fn complete_tables_preserve_source_indices_offsets_and_special_payloads() {
    for marker in [5, 8, 25] {
        let data = fixture(marker);
        let document = scan_contents(&data, &ParseLimits::default()).unwrap();
        let bounds = document.header().section_offsets;
        let tables = RawStyleTables::parse(&document).unwrap();
        assert_eq!(tables.layers.len(), 257);
        assert_eq!(tables.colors.len(), 256);
        assert_eq!(tables.line_types.len(), 32);
        assert_eq!(tables.widths.len(), 16);
        assert_eq!(
            tables.layers.iter().map(|e| e.index).collect::<Vec<_>>(),
            (0..=256).collect::<Vec<_>>()
        );
        assert_eq!(
            tables.colors.iter().map(|e| e.index).collect::<Vec<_>>(),
            (1..=256).collect::<Vec<_>>()
        );
        for row in &tables.layers {
            let offset = row.payload_offset as usize;
            assert_eq!(row.raw_bytes, data[offset..offset + row.raw_bytes.len()]);
            assert_eq!(row.stored_index_raw, row.index);
            assert_eq!(
                (
                    row.color_index_raw,
                    row.line_type_index_raw,
                    row.width_index_raw
                ),
                (7, 2, 5)
            );
            assert_eq!((row.flags_raw, row.reserved_word_raw), (0x400, 0x7788));
        }
        assert_eq!(tables.layers[1].payload_offset, bounds[0]);
        assert_eq!(tables.layers[0].raw_bytes.len(), 296);
        assert_eq!(tables.layers[256].raw_bytes.len(), 296);
        assert_eq!(&tables.layers[0].raw_bytes[64..], &[0xA5; 232]);
        assert_eq!(tables.colors[0].payload_offset, bounds[1]);
        assert_eq!(
            tables.colors[255].payload_offset,
            tables.shared_suffix_offset + 16
        );
        assert_eq!(
            tables.shared_suffix_bytes.len(),
            if marker == 25 { 7272 } else { 1768 }
        );
        assert_eq!(
            tables.extension_header_offsets,
            [
                tables.shared_suffix_offset,
                tables.shared_suffix_offset + 80,
                tables.shared_suffix_offset + 392
            ]
        );
        assert_eq!(
            tables.extension_header_bytes[0],
            [2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0]
        );
        assert_eq!(document.contents(), data);
    }
}

#[test]
fn raw_and_scaled_values_stay_distinct_and_unknown_flags_are_not_evaluated() {
    let tables = parse(&fixture(25));
    let c = &tables.colors[255];
    assert_eq!(
        (c.index, c.pen_width_hundredths_raw, c.pen_width_value),
        (256, 512, 5.12)
    );
    assert_eq!((c.rgb, c.output_rgb), ([0, 17, 250], [11, 22, 33]));
    assert_eq!(
        (c.flags_raw, c.rgb_encoding_raw, c.output_rgb_encoding_raw),
        (0x1234, 2, 0x9A)
    );
    assert_eq!(c.raw_bytes, color(256));
    for (i, row) in tables.line_types.iter().enumerate() {
        assert_eq!(
            (row.index, row.pen_style_raw, row.flags_raw),
            (i as u16 + 1, i as u16, 0x43)
        );
        assert_eq!(&row.pen_definition_hundredths_raw[..4], &[125, 0, 250, 0]);
        assert_eq!(&row.pen_definition_values[..4], &[1.25, 0.0, 2.5, 0.0]);
        assert_eq!(row.pen_definition_values.len(), 30);
        assert_eq!(row.raw_bytes[4..6], 125u16.to_le_bytes());
        assert_eq!(
            row.payload_offset,
            tables.line_types[0].payload_offset + i as u64 * 64
        );
    }
    for row in &tables.widths {
        assert_eq!(row.pen_width_hundredths_raw, row.index * 25);
        assert_eq!(row.pen_width_value, f64::from(row.index) / 4.0);
        assert_eq!(row.flags_raw, 0x9876);
        assert_eq!(&row.raw_bytes[4..], &[0x7F; 60]);
        assert_eq!(
            row.payload_offset,
            tables.line_types[0].payload_offset + 2048 + u64::from(row.index - 1) * 64
        );
    }
}

#[test]
fn unqualified_suffix_headers_and_data_reject_the_whole_request() {
    for relative in [0usize, 4, 8, 12, 80, 84, 92, 392, 396, 404, 704, 1767] {
        let mut data = fixture(5);
        let suffix = parse(&data).shared_suffix_offset as usize;
        data[suffix + relative] ^= 1;
        let document = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(
            matches!(
                RawStyleTables::parse(&document),
                Err(DracadError::UnsupportedFeature(_))
            ),
            "relative {relative}"
        );
        assert_eq!(document.records().count(), 1);
    }
}

#[test]
fn layer_identity_mismatch_and_unqualified_optional_tail_fail_closed() {
    for special in [None, Some(0usize), Some(256usize)] {
        let mut data = fixture(25);
        let good = parse(&data);
        let offset = special.map_or(good.layers[13].payload_offset, |index| {
            good.layers[index].payload_offset
        }) as usize;
        word(&mut data, offset + 10, 65535);
        let document = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            RawStyleTables::parse(&document),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let mut data = fixture(25);
    let suffix = parse(&data).shared_suffix_offset;
    dword(&mut data, 0x44, suffix as u32);
    let document = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        RawStyleTables::parse(&document),
        Err(DracadError::UnsupportedFeature(_))
    ));
}
