use ezdracad_core::{
    scan_contents, DracadError, ParseLimits, StyleSourceKind, MAX_EFFECTIVE_STYLE_LINES,
};

fn word(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn offset(data: &[u8], at: usize) -> usize {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize
}

fn header(data: &mut [u8], at: usize, tag: u16, layer: u16, values: [u8; 3]) {
    word(data, at, tag);
    word(data, at + 2, layer);
    data[at + 4..at + 7].copy_from_slice(&values);
}

fn root() -> Vec<u8> {
    include_bytes!("../../../tests/fixtures/semantics25.bin").to_vec()
}

fn triple(style: &ezdracad_core::EffectiveStyleIndices) -> [u16; 3] {
    [style.color.index, style.line_type.index, style.width.index]
}

fn set_layer(data: &mut [u8], index: usize, values: [u16; 3]) {
    let layer_start = offset(data, 0x28);
    let suffix = offset(data, 0x50) - 7272;
    let at = match index {
        0 => suffix + 96,
        256 => suffix + 408,
        _ => layer_start + (index - 1) * 64,
    };
    for (i, value) in values.into_iter().enumerate() {
        word(data, at + 4 + i * 2, value);
    }
}

/// Independent synthetic definitions plus separately authored style rows. It is
/// not a native application drawing or qualification for native inheritance.
fn blocks() -> Vec<u8> {
    let mut data = include_bytes!("../../../tests/fixtures/blocks25.bin").to_vec();
    let styles = root();
    for index in 0..6 {
        let a = offset(&data, 0x28 + index * 4);
        let b = offset(&data, 0x2c + index * 4);
        let c = offset(&styles, 0x28 + index * 4);
        let d = offset(&styles, 0x2c + index * 4);
        assert_eq!(b - a, d - c);
        data[a..b].copy_from_slice(&styles[c..d]);
    }
    let end = offset(&data, 0x50);
    let styles_end = offset(&styles, 0x50);
    data[end - 7272..end].copy_from_slice(&styles[styles_end - 7272..styles_end]);
    header(&mut data, 8192, 6, 5, [12, 13, 14]);
    header(&mut data, 8192 + 128, 6, 6, [13, 14, 15]);
    data[8192 + 128 + 32..8192 + 128 + 64].fill(0);
    data[8192 + 128 + 32..8192 + 128 + 35].copy_from_slice(b"TWO");
    let first = offset(&data, 0x4c) + 176;
    header(&mut data, first, 2, 1, [1, 1, 1]);
    let second = first + 128 + 176;
    header(&mut data, second, 0x4402, 0, [0, 255, 255]);
    header(&mut data, second + 64, 0x402, 7, [7, 8, 9]);
    set_layer(&mut data, 0, [15, 16, 15]);
    set_layer(&mut data, 5, [2, 3, 4]);
    set_layer(&mut data, 6, [4, 5, 6]);
    set_layer(&mut data, 7, [9, 10, 11]);
    data
}

#[test]
fn root_explicit_and_layer_references_retain_exact_source_rows() {
    let mut data = root();
    header(&mut data, 8192 + 128, 0x402, 1, [0, 32, 16]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = doc.effective_line_style(2).unwrap();
    assert_eq!(triple(&row.styles), [256, 32, 16]);
    assert_eq!(row.styles.color.sources.len(), 2);
    assert_eq!(row.styles.color.sources[0].kind, StyleSourceKind::Record);
    assert_eq!(row.styles.color.sources[1].kind, StyleSourceKind::Color);
    assert_eq!(row.styles.color.sources[1].index, 256);
    assert_eq!(row.source.header_bytes, row.attributes.header_bytes);
    assert_eq!(row.source.start, [-12.5, 0.25]);
    for s in &row.styles.color.sources {
        let at = s.payload_offset as usize;
        assert_eq!(s.raw_bytes, data[at..at + s.raw_bytes.len()]);
    }
    header(&mut data, 8192 + 128, 0x2402, 1, [0, 0, 0]);
    set_layer(&mut data, 1, [256, 32, 16]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = doc.effective_line_style(2).unwrap();
    assert_eq!(triple(&row.styles), [256, 32, 16]);
    assert_eq!(row.styles.color.sources.len(), 3);
    assert_eq!(row.styles.color.sources[1].kind, StyleSourceKind::Layer);
    assert_eq!(row.styles.color.sources[1].index, 1);
    assert_eq!(doc.line_records().count(), 0); // Legacy API is unchanged.
}

#[test]
fn standalone_byblock_unknown_flags_and_interior_selection_fail_closed() {
    let data = root();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.effective_line_style(2),
        Err(DracadError::UnsupportedFeature(_))
    ));
    for index in [0, 1, 3, 7, u64::MAX] {
        assert!(doc.effective_line_style(index).is_err());
    }
    let mut data = data;
    header(&mut data, 8192 + 128, 0x802, 1, [1, 1, 1]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.effective_line_style(2).is_err());
    header(&mut data, 8192 + 128, 2, 1, [1, 1, 1]);
    word(&mut data, 8192 + 5 * 64 + 8, 0);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        doc.effective_line_style(2),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn layer_references_do_not_become_undocumented_sentinels_or_print_width() {
    for values in [[0, 1, 1], [257, 1, 1], [1, 33, 1], [1, 1, 17]] {
        let mut data = root();
        header(&mut data, 8192 + 128, 0x2402, 1, [0, 0, 0]);
        set_layer(&mut data, 1, values);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            doc.effective_line_style(2),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let mut data = root();
    header(&mut data, 8192 + 128, 0x2402, 0, [0, 0, 0]);
    set_layer(&mut data, 0, [2, 3, 4]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = doc.effective_line_style(2).unwrap();
    assert_eq!(triple(&row.styles), [2, 3, 4]);
    assert_eq!(row.styles.width.sources[1].raw_bytes.len(), 296);
    assert_eq!(row.styles.width.sources.last().unwrap().index, 4);
    assert_ne!(
        row.styles.width.sources.last().unwrap().kind,
        StyleSourceKind::Color
    );
}

#[test]
fn one_definition_resolves_by_selected_insertion_and_explicit_children_override() {
    let data = blocks();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    for (reference, expected) in [(0, [12, 13, 14]), (2, [13, 14, 15])] {
        let rows = library.effective_line_styles(reference, 4096).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].definition_index, 1);
        assert_eq!(triple(&rows[0].styles), expected);
        assert_eq!(triple(&rows[1].styles), [7, 8, 9]);
        assert_eq!(rows[0].styles.color.sources[0].definition_index, Some(1));
        assert_eq!(
            rows[0].styles.color.sources[1].kind,
            StyleSourceKind::BlockPlacement
        );
        assert_eq!(rows[0].styles.color.sources[1].index, reference);
        assert_eq!(rows[0].styles.color.sources[1].raw_bytes.len(), 128);
        assert_eq!(rows[0].coordinate_space(), "package_parent");
        for property in [
            &rows[0].styles.color,
            &rows[0].styles.line_type,
            &rows[0].styles.width,
        ] {
            assert!(property.sources.len() <= 4);
            for source in &property.sources {
                let at = source.payload_offset as usize;
                assert_eq!(source.raw_bytes, data[at..at + source.raw_bytes.len()]);
            }
        }
    }
    assert!(library.planar_line_records(1).is_err());
    assert!(library.placed_line_records(0).is_err());
    assert_eq!(doc.line_candidate_count(), 0);
}

#[test]
fn layer_zero_by_layer_uses_insertion_layer_while_nonzero_child_keeps_own_layer() {
    let mut data = blocks();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    let at = library.definitions()[1].record_start as usize;
    header(&mut data, at, 0x2402, 0, [0, 0, 0]);
    header(&mut data, at + 64, 0x2402, 7, [0, 0, 0]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    for (reference, expected, layer) in [(0, [2, 3, 4], 5), (2, [4, 5, 6], 6)] {
        let rows = library.effective_line_styles(reference, 4096).unwrap();
        assert_eq!(triple(&rows[0].styles), expected);
        assert_eq!(triple(&rows[1].styles), [9, 10, 11]);
        assert_eq!(rows[0].styles.color.sources[2].index, layer);
        assert_eq!(rows[0].styles.color.sources[2].kind, StyleSourceKind::Layer);
        assert_eq!(rows[1].styles.color.sources[2].index, 7);
    }
}

#[test]
fn output_limits_placement_guards_unknown_children_and_nonfinite_values_are_preserved() {
    let data = blocks();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let library = doc.block_library().unwrap();
    assert!(matches!(
        library.effective_line_styles(0, 1),
        Err(DracadError::LimitExceeded {
            resource: "effective_style_lines",
            ..
        })
    ));
    assert!(matches!(
        library.effective_line_styles(0, MAX_EFFECTIVE_STYLE_LINES + 1),
        Err(DracadError::LimitExceeded {
            resource: "effective_style_lines_limit",
            ..
        })
    ));
    assert!(library.effective_line_styles(1, 4096).is_err());
    let leaf = library.definitions()[1].record_start as usize;
    for (at, bytes) in [
        (8192 + 80 + 32, 2f64.to_le_bytes().to_vec()), // nonuniform scale
        (8192 + 16, 1f64.to_le_bytes().to_vec()),      // nonzero anchor
        (leaf + 48, 1f64.to_le_bytes().to_vec()),      // nonplanar source
        (leaf + 24, f64::NAN.to_le_bytes().to_vec()),
        (leaf, 0x30u16.to_le_bytes().to_vec()), // no generic package inheritance
    ] {
        let mut changed = data.clone();
        changed[at..at + bytes.len()].copy_from_slice(&bytes);
        let doc = scan_contents(&changed, &ParseLimits::default()).unwrap();
        assert!(doc
            .block_library()
            .unwrap()
            .effective_line_styles(0, 4096)
            .is_err());
    }
}
