//! Independently authored synthetic traversal/style fixtures. These exercise
//! algorithms and rejection boundaries; they are not native semantic evidence.
use ezdracad_core::{
    scan_contents, scene, DracadError, ParseLimits, SceneAncestorKind, SceneCoordinateScope,
    SceneLimits, StyleSourceKind, MAX_SCENE_DEPTH, MAX_SCENE_PROVENANCE,
};

fn u32_at(data: &[u8], at: usize) -> usize {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize
}
fn put32(data: &mut [u8], at: usize, value: usize) {
    data[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn slot(tag: u16, span: u16, layer: u16, style: [u8; 3]) -> [u8; 64] {
    let mut out = [0; 64];
    out[..2].copy_from_slice(&tag.to_le_bytes());
    out[2..4].copy_from_slice(&layer.to_le_bytes());
    out[4..7].copy_from_slice(&style);
    out[7] = 19; // opaque material
    out[8..10].copy_from_slice(&span.to_le_bytes());
    out[10..12].copy_from_slice(&1234u16.to_le_bytes());
    out
}
fn line(tag: u16, layer: u16, style: [u8; 3]) -> [u8; 64] {
    let mut out = slot(tag, 1, layer, style);
    for (i, n) in [1f64, 2., 3., 4., 0., 0.].into_iter().enumerate() {
        out[16 + i * 8..24 + i * 8].copy_from_slice(&n.to_le_bytes());
    }
    out
}
fn inline(span: u16) -> [u8; 64] {
    let mut out = slot(0x30, span, 7, [9, 10, 11]);
    out[16..20].copy_from_slice(&0x400u32.to_le_bytes());
    out
}
fn reference(name: &[u8], layer: u16, style: [u8; 3], placement: [f64; 6]) -> Vec<[u8; 64]> {
    assert!(name.len() < 32);
    let mut primary = slot(6, 2, layer, style);
    primary[32..32 + name.len()].copy_from_slice(name);
    let mut continuation = slot(0xfe, 0, 0, [0, 0, 0]);
    for (i, n) in placement.into_iter().enumerate() {
        continuation[16 + i * 8..24 + i * 8].copy_from_slice(&n.to_le_bytes());
    }
    vec![primary, continuation]
}
const UNIT: [f64; 6] = [0., 0., 0., 1., 1., 0.];

fn fixture(root: &[[u8; 64]], definitions: &[(&[u8], Vec<[u8; 64]>)]) -> Vec<u8> {
    let original = include_bytes!("../../../tests/fixtures/semantics25.bin");
    let suffix = original[u32_at(original, 0x50) - 7272..u32_at(original, 0x50)].to_vec();
    let mut data = original.to_vec();
    let old_end = u32_at(&data, 0x28);
    let old_count = (old_end - 8192) / 64;
    let shift = (root.len() as i64 + 1 - old_count as i64) * 64;
    for at in (0x28..=0x50).step_by(4) {
        let offset = u32_at(&data, at);
        if offset != 0 {
            put32(&mut data, at, (offset as i64 + shift) as usize);
        }
    }
    put32(&mut data, 0x58, root.len());
    let mut records = root.concat();
    records.extend(slot(0xff, 0, 0, [0, 0, 0]));
    data.splice(8192..old_end, records);
    data.truncate(u32_at(&data, 0x40));
    data.extend([0; 296]);
    let definition_start = data.len();
    for (name, records) in definitions {
        let mut header = [0; 176];
        header[..name.len()].copy_from_slice(name);
        put32(&mut header, 36, records.len());
        data.extend(header);
        data.extend(records.concat());
        data.extend(slot(0xff, 0, 0, [0, 0, 0]));
    }
    data.extend(suffix);
    let end = data.len();
    data.extend([0; 4]);
    put32(
        &mut data,
        0x4c,
        if definitions.is_empty() {
            0
        } else {
            definition_start
        },
    );
    put32(&mut data, 0x50, end);
    for (index, values) in [(0, [15u16, 16, 15]), (5, [2, 3, 4]), (7, [9, 10, 11])] {
        let at = if index == 0 {
            end - 7272 + 96
        } else {
            u32_at(&data, 0x28) + (index - 1) * 64
        };
        for (i, n) in values.into_iter().enumerate() {
            data[at + 4 + i * 2..at + 6 + i * 2].copy_from_slice(&n.to_le_bytes());
        }
    }
    data
}

fn nested() -> Vec<u8> {
    let mut outer = reference(b"INNER", 0, [7, 255, 255], [100., 50., 0., 2., 2., 90.]);
    outer.extend(reference(
        b"INNER",
        7,
        [8, 9, 10],
        [-10., 20., 0., 1., 1., 0.],
    ));
    fixture(
        &reference(b"OUTER", 5, [12, 13, 14], [-150., 200., 0., 3., 3., 30.]),
        &[
            (b"OUTER", outer),
            (
                b"INNER",
                vec![
                    inline(2),
                    line(2, 0, [9, 10, 11]),
                    line(0x4402, 0, [0, 255, 255]),
                    line(0x2402, 0, [0, 0, 0]),
                ],
            ),
        ],
    )
}
fn triple(row: &ezdracad_core::SceneLine2D) -> [u16; 3] {
    [
        row.styles.color.index,
        row.styles.line_type.index,
        row.styles.width.index,
    ]
}
fn near(actual: [f64; 2], expected: [f64; 2]) {
    for i in 0..2 {
        assert!(
            (actual[i] - expected[i]).abs() < 1e-10,
            "{actual:?} != {expected:?}"
        );
    }
}

#[test]
fn repeated_definitions_compose_placements_and_retain_complete_scope_and_source() {
    let data = nested();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let rows = scene::decode(&doc, 0, &SceneLimits::default()).unwrap();
    assert_eq!(rows.len(), 6);
    // First inner point is (96,52); outer S3/R30/T(-150,200).
    let sqrt3 = 3f64.sqrt();
    near(
        rows[0].start,
        [-150. + 144. * sqrt3 - 78., 200. + 144. + 78. * sqrt3],
    );
    // The same definition is emitted again for its second active path.
    near(
        rows[3].start,
        [-150. - 13.5 * sqrt3 - 33., 200. - 13.5 + 33. * sqrt3],
    );
    assert_eq!(triple(&rows[0]), [9, 10, 11]);
    assert_eq!(triple(&rows[1]), [7, 13, 14]);
    assert_eq!(triple(&rows[2]), [2, 3, 4]);
    assert_eq!(triple(&rows[3]), [9, 10, 11]);
    assert_eq!(triple(&rows[4]), [8, 9, 10]);
    assert_eq!(triple(&rows[5]), [9, 10, 11]);
    assert_eq!(rows[0].style_layer_index, 5);
    assert_eq!(rows[3].style_layer_index, 7);
    assert_eq!(rows[0].attributes.layer_index, 0);
    assert_eq!(rows[0].attributes.material_index_raw, 19);
    assert_eq!(rows[0].attributes.group_index, 1234);
    assert_eq!(rows[0].source.start, [1., 2.]);
    assert_eq!(
        rows[0].coordinate_scope,
        SceneCoordinateScope::PackageParent
    );
    assert_eq!(rows[0].definition_index, Some(1));
    assert_eq!(
        rows[0].ancestors.iter().map(|a| a.kind).collect::<Vec<_>>(),
        [
            SceneAncestorKind::BlockReference,
            SceneAncestorKind::BlockReference,
            SceneAncestorKind::InlinePackage
        ]
    );
    assert_eq!(
        rows[0]
            .styles
            .width
            .sources
            .iter()
            .map(|s| s.kind)
            .collect::<Vec<_>>(),
        [
            StyleSourceKind::Record,
            StyleSourceKind::BlockPlacement,
            StyleSourceKind::BlockPlacement,
            StyleSourceKind::Width
        ]
    );
    for row in rows {
        for ancestor in row.ancestors {
            let at = ancestor.payload_offset as usize;
            assert_eq!(ancestor.raw_bytes, data[at..at + ancestor.raw_bytes.len()]);
            if let Some(target) = ancestor.target_definition {
                let at = target.payload_offset as usize;
                assert_eq!(target.header_bytes, data[at..at + 176]);
            }
        }
        for property in [&row.styles.color, &row.styles.line_type, &row.styles.width] {
            for source in &property.sources {
                let at = source.payload_offset as usize;
                assert_eq!(source.raw_bytes, data[at..at + source.raw_bytes.len()]);
            }
        }
    }
}

#[test]
fn exact_color_byblock_reference_has_scoped_recursive_sources_without_broadening_raw_apis() {
    let mut inner = reference(b"INNER", 0, [0, 255, 255], [100., 50., 0., 2., 2., 90.]);
    inner[0][..2].copy_from_slice(&0x4006u16.to_le_bytes());
    let data = fixture(
        &reference(b"OUTER", 5, [2, 2, 5], [-150., 200., 0., 3., 3., 30.]),
        &[
            (b"OUTER", inner.clone()),
            (b"INNER", vec![line(0x4402, 1, [0, 255, 255])]),
        ],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = scene::decode(&doc, 0, &SceneLimits::default())
        .unwrap()
        .remove(0);
    assert_eq!(triple(&row), [2, 2, 5]);
    assert_eq!(row.ancestors[1].raw_bytes[..2], 0x4006u16.to_le_bytes());
    for property in [&row.styles.color, &row.styles.line_type, &row.styles.width] {
        assert_eq!(property.sources.len(), 4);
        assert_eq!(property.sources[1].raw_bytes[..2], 0x4006u16.to_le_bytes());
    }
    let standalone = fixture(&inner, &[(b"INNER", vec![line(0x4402, 1, [0, 255, 255])])]);
    let doc = scan_contents(&standalone, &ParseLimits::default()).unwrap();
    assert!(
        matches!(scene::decode(&doc,0,&SceneLimits::default()), Err(DracadError::UnsupportedFeature(message)) if message.contains("ByBlock"))
    );
    assert!(doc.package_reference(0).unwrap().is_none());
    assert!(doc.record_attributes(0).is_err());
    for (at, bytes) in [
        (4, vec![1]),
        (0, 0x8006u16.to_le_bytes().to_vec()),
        (8, 1u16.to_le_bytes().to_vec()),
        (16, 1f64.to_le_bytes().to_vec()),
    ] {
        let mut bad = standalone.clone();
        bad[8192 + at..8192 + at + bytes.len()].copy_from_slice(&bytes);
        let doc = scan_contents(&bad, &ParseLimits::default()).unwrap();
        assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    }
}

#[test]
fn exact_bylayer_reference_uses_root_layer_zero_or_the_outer_insertion_layer() {
    let mut inner = reference(b"INNER", 0, [0, 0, 0], [100., 50., 0., 2., 2., 90.]);
    inner[0][..2].copy_from_slice(&0x2006u16.to_le_bytes());
    let standalone = fixture(&inner, &[(b"INNER", vec![line(0x4402, 1, [0, 255, 255])])]);
    let doc = scan_contents(&standalone, &ParseLimits::default()).unwrap();
    let row = scene::decode(&doc, 0, &SceneLimits::default())
        .unwrap()
        .remove(0);
    assert_eq!(triple(&row), [15, 16, 15]);
    assert_eq!(
        row.styles.color.sources[1].raw_bytes[..2],
        0x2006u16.to_le_bytes()
    );
    assert_eq!(row.styles.color.sources[2].index, 0);
    assert!(doc.package_reference(0).unwrap().is_none());
    assert!(doc.record_attributes(0).is_err());
    let data = fixture(
        &reference(b"OUTER", 5, [2, 2, 5], UNIT),
        &[
            (b"OUTER", inner),
            (b"INNER", vec![line(0x4402, 1, [0, 255, 255])]),
        ],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = scene::decode(&doc, 0, &SceneLimits::default())
        .unwrap()
        .remove(0);
    assert_eq!(triple(&row), [2, 3, 4]);
    assert_eq!(row.style_layer_index, 1); // leaf layer remains explicit1
    assert_eq!(row.styles.color.sources.len(), 5);
    assert_eq!(row.styles.color.sources[3].index, 5);
}

#[test]
fn inline_dependent_scopes_fail_until_native_qualification() {
    let data = fixture(&[inline(2), line(2, 0, [1, 1, 1])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = scene::decode(&doc, 0, &SceneLimits::default())
        .unwrap()
        .remove(0);
    assert_eq!(triple(&row), [1, 1, 1]);
    assert_eq!(row.coordinate_scope, SceneCoordinateScope::RootRecords);
    near(row.start, [1., 2.]);
    for child in [line(0x4402, 0, [0, 255, 255]), line(0x2402, 0, [0, 0, 0])] {
        let data = fixture(&[inline(2), child], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(
            matches!(scene::decode(&doc,0,&SceneLimits::default()), Err(DracadError::UnsupportedFeature(message)) if message.contains("inline-dependent"))
        );
        let data = fixture(
            &reference(b"A", 5, [2, 2, 5], UNIT),
            &[(b"A", vec![inline(2), child])],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(
            matches!(scene::decode(&doc,0,&SceneLimits::default()), Err(DracadError::UnsupportedFeature(message)) if message.contains("inline-dependent"))
        );
    }
    let mut inner = reference(b"B", 0, [0, 255, 255], UNIT);
    inner[0][..2].copy_from_slice(&0x4006u16.to_le_bytes());
    let data = fixture(
        &[vec![inline(3)], inner].concat(),
        &[(b"B", vec![line(2, 1, [1, 1, 1])])],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(
        matches!(scene::decode(&doc,0,&SceneLimits::default()), Err(DracadError::UnsupportedFeature(message)) if message.contains("inline-dependent"))
    );
}

#[test]
fn opted_in_inline_wrappers_are_transparent_to_byblock_and_placement() {
    let mut outer = inline(4);
    outer[2..4].copy_from_slice(&5u16.to_le_bytes());
    outer[4..7].copy_from_slice(&[3, 4, 7]);
    let mut inner = inline(2);
    inner[2..4].copy_from_slice(&9u16.to_le_bytes());
    inner[4..7].copy_from_slice(&[6, 3, 9]);
    let leaf = line(0x4402, 1, [0, 255, 255]);
    let data = fixture(
        &reference(b"A", 5, [2, 2, 5], [100., 50., 0., 2., 2., 90.]),
        &[(b"A", vec![outer, inner, leaf, leaf])],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.scene_lines(0, &SceneLimits::default()).is_err());
    let rows = doc
        .scene_lines_with_inline_scopes(0, &SceneLimits::default())
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(triple(row), [2, 2, 5]);
        assert_eq!(row.style_layer_index, 1);
        assert_eq!(row.coordinate_scope, SceneCoordinateScope::PackageParent);
        near(row.start, [96., 52.]);
        near(row.end, [92., 56.]);
        assert_eq!(row.styles.color.sources.len(), 3);
        assert_eq!(row.styles.color.sources[0].kind, StyleSourceKind::Record);
        assert_eq!(
            row.styles.color.sources[1].kind,
            StyleSourceKind::BlockPlacement
        );
        assert_eq!(row.styles.color.sources[2].kind, StyleSourceKind::Color);
    }
    assert_eq!(rows[0].ancestors.len(), 3);
    assert_eq!(rows[1].ancestors.len(), 2);
    assert_eq!(rows[0].ancestors[1].raw_bytes, outer);
    assert_eq!(rows[0].ancestors[2].raw_bytes, inner);
    // Unused inline style fields must not be reported as inherited contributors.
    assert!(rows[0]
        .styles
        .width
        .sources
        .iter()
        .all(|source| { source.table_domain != "block_definition_records" || source.index == 2 }));
}

#[test]
fn opted_in_layer0_bylayer_uses_true_reference_layer_not_inline_layers() {
    let mut outer = inline(4);
    outer[2..4].copy_from_slice(&7u16.to_le_bytes());
    let leaf = line(0x2402, 0, [0, 0, 0]);
    let data = fixture(
        &reference(b"A", 5, [12, 13, 14], UNIT),
        &[(b"A", vec![outer, inline(2), leaf, leaf])],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let rows = scene::decode_with_inline_scopes(&doc, 0, &SceneLimits::default()).unwrap();
    for row in rows {
        assert_eq!(triple(&row), [2, 3, 4]);
        assert_eq!(row.style_layer_index, 5);
        assert_eq!(row.styles.color.sources[2].kind, StyleSourceKind::Layer);
        assert_eq!(row.styles.color.sources[2].index, 5);
    }
}

#[test]
fn opted_in_exact_dependent_wrappers_are_preserved_without_becoming_scopes() {
    for (tag, style) in [
        (0x2030u16, [0, 0, 0]),
        (0x4030, [0, 255, 255]),
        (0x430, [6, 3, 9]),
    ] {
        let mut wrapper = inline(2);
        wrapper[..2].copy_from_slice(&tag.to_le_bytes());
        wrapper[2..4].fill(0);
        wrapper[4..7].copy_from_slice(&style);
        let data = fixture(
            &reference(b"A", 5, [12, 13, 14], UNIT),
            &[(b"A", vec![wrapper, line(0x4402, 1, [0, 255, 255])])],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let row = scene::decode_with_inline_scopes(&doc, 0, &SceneLimits::default())
            .unwrap()
            .remove(0);
        assert_eq!(triple(&row), [12, 13, 14]);
        assert_eq!(row.ancestors[1].raw_bytes, wrapper);
        assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    }
    // No change to the separate exact-tag raw attribute API.
    let mut wrapper = inline(2);
    wrapper[..2].copy_from_slice(&0x2030u16.to_le_bytes());
    wrapper[4..7].fill(0);
    let data = fixture(&[wrapper, line(2, 1, [1, 1, 1])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.record_attributes(0).is_err());
}

#[test]
fn opted_in_inline_without_true_reference_does_not_create_byblock_context() {
    let data = fixture(&[inline(2), line(0x4402, 0, [0, 255, 255])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(
        matches!(scene::decode_with_inline_scopes(&doc, 0, &SceneLimits::default()),
        Err(DracadError::UnsupportedFeature(message)) if message.contains("no block-insertion"))
    );
    let data = fixture(&[inline(2), line(0x2402, 0, [0, 0, 0])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let row = scene::decode_with_inline_scopes(&doc, 0, &SceneLimits::default())
        .unwrap()
        .remove(0);
    assert_eq!(row.style_layer_index, 0);
    assert_eq!(triple(&row), [15, 16, 15]);
    assert_eq!(row.coordinate_scope, SceneCoordinateScope::RootRecords);
}

#[test]
fn opted_in_inline_requires_marker25_and_keeps_exact_state_and_resource_guards() {
    for source in [
        include_bytes!("../../../tests/fixtures/records5.bin").as_slice(),
        include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
    ] {
        let doc = scan_contents(source, &ParseLimits::default()).unwrap();
        assert!(
            matches!(scene::decode_with_inline_scopes(&doc, 0, &SceneLimits::default()),
            Err(DracadError::UnsupportedFeature(message)) if message.contains("requires marker25"))
        );
    }
    for damage in [0, 1, 2, 3] {
        let mut wrapper = inline(2);
        match damage {
            0 => wrapper[..2].copy_from_slice(&0x4430u16.to_le_bytes()),
            1 => wrapper[20] = 1,
            2 => {
                wrapper[..2].copy_from_slice(&0x2030u16.to_le_bytes());
                wrapper[4] = 3;
            }
            _ => wrapper[5] = 35,
        }
        let data = fixture(&[wrapper, line(2, 1, [1, 1, 1])], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(scene::decode_with_inline_scopes(&doc, 0, &SceneLimits::default()).is_err());
    }
    let data = fixture(&[inline(2), line(0x2402, 0, [0, 0, 0])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        scene::decode_with_inline_scopes(
            &doc,
            0,
            &SceneLimits {
                max_nodes: 1,
                ..SceneLimits::default()
            }
        ),
        Err(DracadError::LimitExceeded { .. })
    ));
}

#[test]
fn cycles_ambiguity_missing_targets_and_interior_selection_fail_whole_requests() {
    for definitions in [
        vec![(b"A".as_slice(), reference(b"A", 0, [1, 1, 1], UNIT))],
        vec![
            (b"A".as_slice(), reference(b"B", 0, [1, 1, 1], UNIT)),
            (b"B".as_slice(), reference(b"A", 0, [1, 1, 1], UNIT)),
        ],
        vec![
            (b"A".as_slice(), vec![line(2, 1, [1, 1, 1])]),
            (b"A".as_slice(), vec![line(2, 1, [1, 1, 1])]),
        ],
        vec![],
    ] {
        let data = fixture(&reference(b"A", 1, [1, 1, 1], UNIT), &definitions);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    }
    let data = nested();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for selected in [1, 2, u64::MAX] {
        assert!(scene::decode(&doc, selected, &SceneLimits::default()).is_err());
    }
}

#[test]
fn unqualified_states_nonplanar_and_nonfinite_transforms_are_not_partially_returned() {
    for leaf in [
        slot(0x7777, 1, 1, [1, 1, 1]),
        slot(0x4006, 1, 0, [0, 255, 255]),
        slot(0x2006, 1, 0, [0, 0, 0]),
        slot(0x802, 1, 0, [1, 1, 1]),
        inline(1),
    ] {
        let data = fixture(&[inline(3), line(2, 1, [1, 1, 1]), leaf], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    }
    for (at, value) in [(16, f64::NAN), (48, 1.), (56, 1.)] {
        // The legacy exact-tag2 scan itself rejects non-finite XY. Variant402
        // leaves defer all six checks until this request.
        let mut leaf = line(0x402, 1, [1, 1, 1]);
        leaf[at..at + 8].copy_from_slice(&value.to_le_bytes());
        let data = fixture(&[inline(3), line(2, 1, [1, 1, 1]), leaf], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    }
    for placement in [
        [0., 0., 0., 1., 2., 0.],
        [0., 0., 0., -1., -1., 0.],
        [0., 0., 1., 1., 1., 0.],
        [0., 0., 0., 1., 1., f64::NAN],
    ] {
        let data = fixture(
            &reference(b"A", 1, [1, 1, 1], placement),
            &[(b"A", vec![line(2, 1, [1, 1, 1])])],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    }
}

#[test]
fn later_root_corruption_state_bodies_definition_metadata_and_transform_overflow() {
    let mut body = inline(2);
    body[24] = 1;
    let data = fixture(&[body, line(2, 1, [1, 1, 1])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    let data = fixture(&[line(2, 1, [1, 1, 1]), slot(0x7777, 0, 0, [0, 0, 0])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        scene::decode(&doc, 0, &SceneLimits::default()),
        Err(DracadError::InvalidFormat(_))
    ));
    let metadata = slot(0x33, 2, 0, [0, 0, 0]);
    let continuation = slot(0xfe, 0, 0, [0, 0, 0]);
    let definitions = [(
        b"A".as_slice(),
        vec![metadata, continuation, line(2, 1, [1, 1, 1])],
    )];
    let mut data = fixture(&reference(b"A", 1, [1, 1, 1], UNIT), &definitions);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(
        scene::decode(&doc, 0, &SceneLimits::default())
            .unwrap()
            .len(),
        1
    );
    let at = u32_at(&data, 0x4c);
    data[at + 40] = 1;
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(scene::decode(&doc, 0, &SceneLimits::default()).is_err());
    for scale in [1e200, 1e-200] {
        let data = fixture(
            &reference(b"A", 1, [1, 1, 1], [0., 0., 0., scale, scale, 0.]),
            &[
                (
                    b"A",
                    reference(b"B", 0, [1, 1, 1], [0., 0., 0., scale, scale, 0.]),
                ),
                (b"B", vec![line(2, 1, [1, 1, 1])]),
            ],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            scene::decode(&doc, 0, &SceneLimits::default()),
            Err(DracadError::InvalidFormat(_))
        ));
    }
}

#[test]
fn empty_definitions_still_validate_placement_and_requested_hard_limits() {
    let definitions = [(b"A".as_slice(), vec![])];
    let data = fixture(&reference(b"A", 1, [1, 1, 1], UNIT), &definitions);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(scene::decode(&doc, 0, &SceneLimits::default())
        .unwrap()
        .is_empty());
    for limits in [
        SceneLimits {
            max_nodes: 4097,
            ..Default::default()
        },
        SceneLimits {
            max_lines: 4097,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            scene::decode(&doc, 0, &limits),
            Err(DracadError::LimitExceeded { .. })
        ));
    }
    let mut bad = data;
    bad[8192 + 16..8192 + 24].copy_from_slice(&1f64.to_le_bytes());
    let doc = scan_contents(&bad, &ParseLimits::default()).unwrap();
    assert!(matches!(
        scene::decode(&doc, 0, &SceneLimits::default()),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn depth_nodes_lines_and_all_copied_provenance_are_bounded() {
    let data = nested();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for (resource, limits) in [
        (
            "scene_depth",
            SceneLimits {
                max_depth: 2,
                ..Default::default()
            },
        ),
        (
            "scene_nodes",
            SceneLimits {
                max_nodes: 2,
                ..Default::default()
            },
        ),
        (
            "scene_lines",
            SceneLimits {
                max_lines: 3,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(scene::decode(&doc, 0, &limits), Err(DracadError::LimitExceeded { resource: name, .. }) if name == resource)
        );
    }
    let mut root = (2..=130).rev().map(inline).collect::<Vec<_>>();
    root.push(line(2, 1, [1, 1, 1]));
    let data = fixture(&root, &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        scene::decode(
            &doc,
            0,
            &SceneLimits {
                max_depth: 1000,
                ..Default::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "scene_depth",
            limit: MAX_SCENE_DEPTH,
            actual: 129
        })
    ));
    // 128 ancestor descriptors plus separately retained leaf and three property
    // chains per output. Counting ancestors alone would incorrectly pass 1000.
    let mut root = (1001..=1128).rev().map(inline).collect::<Vec<_>>();
    root.extend(vec![line(2, 1, [1, 1, 1]); 1000]);
    let data = fixture(&root, &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        scene::decode(
            &doc,
            0,
            &SceneLimits {
                max_depth: 128,
                ..Default::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "scene_provenance",
            limit: MAX_SCENE_PROVENANCE,
            ..
        })
    ));
}

#[test]
fn provenance_counts_all_three_recursive_property_chains_and_definition_headers() {
    let names = (0..128)
        .map(|i| format!("D{i}").into_bytes())
        .collect::<Vec<_>>();
    let mut definitions = Vec::new();
    for index in 0..127 {
        // Ref6 already qualifies line-type/width ByBlock sentinel codes. Color
        // remains explicit until a new exact reference tag is native-qualified.
        definitions.push((
            names[index].as_slice(),
            reference(&names[index + 1], 0, [1, 255, 255], UNIT),
        ));
    }
    definitions.push((
        names[127].as_slice(),
        vec![line(0x4402, 0, [0, 255, 255]); 512],
    ));
    let data = fixture(&reference(&names[0], 5, [1, 1, 1], UNIT), &definitions);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        scene::decode(
            &doc,
            0,
            &SceneLimits {
                max_depth: 128,
                ..Default::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "scene_provenance",
            limit: MAX_SCENE_PROVENANCE,
            ..
        })
    ));
}
