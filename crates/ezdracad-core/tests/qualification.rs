//! Synthetic contract tests, separate from native decoder-qualification proof.
use ezdracad_core::{
    qualification::{qualify_lines, Line2DProfile, UnresolvedLineSemantics},
    scan_contents, DracadError, ParseLimits, SceneCoordinateScope, SceneLimits,
};

const PROFILE: Line2DProfile = Line2DProfile::Marker25PlanarStoredXyV1;

const PROFILES: [Line2DProfile; 2] = [PROFILE, Line2DProfile::Marker25PlanarStoredXyInlineV1];

fn offset(data: &[u8], at: usize) -> usize {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize
}
fn put(data: &mut [u8], at: usize, value: usize) {
    data[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn slot(tag: u16, span: u16) -> [u8; 64] {
    let mut result = [0; 64];
    result[..2].copy_from_slice(&tag.to_le_bytes());
    result[2..4].copy_from_slice(&1u16.to_le_bytes());
    result[4..8].copy_from_slice(&[1, 1, 1, 17]);
    result[8..10].copy_from_slice(&span.to_le_bytes());
    result[10..12].copy_from_slice(&301u16.to_le_bytes());
    result
}
fn line(tag: u16) -> [u8; 64] {
    let mut result = slot(tag, 1);
    for (index, number) in [-12.5f64, 0.25, 42., 0.25, 0., 0.].into_iter().enumerate() {
        result[16 + index * 8..24 + index * 8].copy_from_slice(&number.to_le_bytes());
    }
    if tag == 0x2402 {
        result[4..7].fill(0);
    }
    if tag == 0x4402 {
        result[4..7].copy_from_slice(&[0, 255, 255]);
    }
    result
}
fn inline(span: u16) -> [u8; 64] {
    let mut result = slot(0x30, span);
    result[16..20].copy_from_slice(&0x400u32.to_le_bytes());
    result
}
fn reference(tag: u16, name: &[u8]) -> Vec<[u8; 64]> {
    let mut primary = slot(tag, 2);
    primary[32..32 + name.len()].copy_from_slice(name);
    if tag == 0x4006 {
        primary[4..7].copy_from_slice(&[0, 255, 255]);
    }
    let mut next = slot(0xfe, 0);
    for (i, value) in [100f64, 50., 0., 2., 2., 90.].into_iter().enumerate() {
        next[16 + i * 8..24 + i * 8].copy_from_slice(&value.to_le_bytes());
    }
    vec![primary, next]
}
fn fixture(marker: u16, records: &[[u8; 64]], definitions: &[(&[u8], Vec<[u8; 64]>)]) -> Vec<u8> {
    let original = match marker {
        5 => include_bytes!("../../../tests/fixtures/semantics5.bin").as_slice(),
        8 => include_bytes!("../../../tests/fixtures/semantics8.bin").as_slice(),
        _ => include_bytes!("../../../tests/fixtures/semantics25.bin").as_slice(),
    };
    let suffix_size = if marker == 25 { 7272 } else { 1768 };
    let suffix = original[offset(original, 0x50) - suffix_size..offset(original, 0x50)].to_vec();
    let mut data = original.to_vec();
    let old_end = offset(&data, 0x28);
    let shift = (records.len() + 1) as i64 * 64 - (old_end - 8192) as i64;
    for at in (0x28..=0x50).step_by(4) {
        let old = offset(&data, at);
        if old != 0 {
            put(&mut data, at, (old as i64 + shift) as usize);
        }
    }
    put(&mut data, 0x58, records.len());
    let mut all = records.concat();
    all.extend(slot(0xff, 0));
    data.splice(8192..old_end, all);
    data.truncate(offset(&data, 0x40));
    data.extend([0; 296]);
    let first = data.len();
    for (name, records) in definitions {
        let mut header = [0; 176];
        header[..name.len()].copy_from_slice(name);
        put(&mut header, 36, records.len());
        data.extend(header);
        data.extend(records.concat());
        data.extend(slot(0xff, 0));
    }
    data.extend(suffix);
    let end = data.len();
    data.extend([0; 4]);
    put(
        &mut data,
        0x4c,
        if definitions.is_empty() { 0 } else { first },
    );
    put(&mut data, 0x50, end);
    let layer1 = offset(&data, 0x28);
    for (i, v) in [3u16, 4, 7].into_iter().enumerate() {
        data[layer1 + 4 + i * 2..layer1 + 6 + i * 2].copy_from_slice(&v.to_le_bytes());
    }
    data
}

fn numeric_line(start: [f64; 2], end: [f64; 2]) -> [u8; 64] {
    let mut result = line(2);
    for (index, value) in start.into_iter().chain(end).enumerate() {
        result[16 + index * 8..24 + index * 8].copy_from_slice(&value.to_le_bytes());
    }
    result
}

fn numeric_reference(placement: [f64; 6]) -> Vec<[u8; 64]> {
    let mut result = reference(6, b"A");
    for (index, value) in placement.into_iter().enumerate() {
        result[1][16 + index * 8..24 + index * 8].copy_from_slice(&value.to_le_bytes());
    }
    result
}

#[test]
fn finite_placements_must_not_qualify_lost_coordinates_or_endpoint_displacements() {
    for (placement, start, end) in [
        ([1e300, 1e300, 0., 1., 1., 0.], [1., 2.], [3., 4.]),
        ([1e300, 0., 0., 1., 1., 0.], [1., 2.], [3., 4.]),
        (
            [0., 0., 0., 1e-200, 1e-200, 0.],
            [1e-200, 2e-200],
            [3e-200, 4e-200],
        ),
        ([1e30, 0., 0., 1., 1., 0.], [1e20, 2.], [1e20 + 16384., 4.]),
    ] {
        let data = fixture(
            25,
            &numeric_reference(placement),
            &[(b"A", vec![numeric_line(start, end)])],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        // The legacy scene API remains accessible, including its old arithmetic.
        let scene = doc.scene_lines(0, &SceneLimits::default()).unwrap();
        assert_eq!(scene.len(), 1);
        for profile in PROFILES {
            assert!(matches!(
                qualify_lines(&doc, 0, profile, &SceneLimits::default()),
                Err(DracadError::InvalidFormat(_))
            ));
        }
        assert_eq!(doc.contents(), data);
    }
}

#[test]
fn exact_cardinal_placements_do_not_invent_displacements_or_change_source_bytes() {
    for (angle, start, end) in [
        (0., [100., 51.], [100., 52.]),
        (90., [99., 50.], [98., 50.]),
        (180., [100., 49.], [100., 48.]),
        (270., [101., 50.], [102., 50.]),
        (-90., [101., 50.], [102., 50.]),
        (450., [99., 50.], [98., 50.]),
    ] {
        let leaf = numeric_line([0., 1.], [0., 2.]);
        let references = numeric_reference([100., 50., 0., 1., 1., angle]);
        let data = fixture(25, &references, &[(b"A", vec![leaf])]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        for profile in PROFILES {
            let result = qualify_lines(&doc, 0, profile, &SceneLimits::default()).unwrap();
            let value = result.lines()[0].scene();
            assert_eq!(value.start, start);
            assert_eq!(value.end, end);
            assert_eq!(value.source.start, [0., 1.]);
            assert_eq!(value.source.end, [0., 2.]);
            assert_eq!(value.source.header_bytes, leaf[..16]);
            assert_eq!(value.ancestors[0].raw_bytes, references.concat());
            let [a, b, c, d, tx, ty] = value.cumulative_transform;
            assert_eq!([a * 0. + c * 1. + tx, b * 0. + d * 1. + ty], start);
        }
        assert_eq!(doc.contents(), data);
    }
}

#[test]
fn identity_root_lines_preserve_finite_extremes_without_unneeded_arithmetic() {
    for (start, end) in [
        ([f64::MAX, -f64::MAX], [-f64::MAX, f64::MAX]),
        ([f64::from_bits(1), 0.], [f64::from_bits(2), 0.]),
    ] {
        let data = fixture(25, &[numeric_line(start, end)], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        for profile in PROFILES {
            let result = qualify_lines(&doc, 0, profile, &SceneLimits::default()).unwrap();
            assert_eq!(result.lines()[0].scene().start, start);
            assert_eq!(result.lines()[0].scene().end, end);
        }
    }
}

#[test]
fn source_zero_length_is_unqualified_without_disabling_raw_or_scene_access() {
    let data = fixture(25, &[numeric_line([3.125, -7.5], [3.125, -7.5])], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(
        doc.scene_lines(0, &SceneLimits::default()).unwrap().len(),
        1
    );
    assert_eq!(doc.line_records().count(), 1);
    for profile in PROFILES {
        assert!(matches!(
            qualify_lines(&doc, 0, profile, &SceneLimits::default()),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    assert_eq!(doc.contents(), data);
}

#[test]
fn noncardinal_degree_conversion_cannot_erase_a_representable_rotation_effect() {
    for angle in [f64::from_bits(1), -f64::from_bits(1)] {
        let data = fixture(
            25,
            &numeric_reference([0., 0., 0., 1., 1., angle]),
            &[(b"A", vec![numeric_line([1e308, 0.], [1.1e308, 0.])])],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert_eq!(
            doc.scene_lines(0, &SceneLimits::default()).unwrap().len(),
            1
        );
        for profile in PROFILES {
            assert!(matches!(
                qualify_lines(&doc, 0, profile, &SceneLimits::default()),
                Err(DracadError::InvalidFormat(_))
            ));
        }
        assert_eq!(doc.contents(), data);
    }
    // Euclidean remainder would round the negative angle to 360. Its tiny
    // rotation still moves these large points by millions of stored units.
    for angle in [1e-300, -1e-300] {
        let data = fixture(
            25,
            &numeric_reference([0., 0., 0., 1., 1., angle]),
            &[(b"A", vec![numeric_line([1e308, 0.], [1.1e308, 0.])])],
        );
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        for profile in PROFILES {
            let value = qualify_lines(&doc, 0, profile, &SceneLimits::default()).unwrap();
            let line = value.lines()[0].scene();
            assert!((1.7e6..1.8e6).contains(&line.start[1].abs()));
            assert!((1.9e6..2e6).contains(&line.end[1].abs()));
            assert_eq!(line.start[1].is_sign_negative(), angle.is_sign_negative());
        }
    }
}

#[test]
fn observed_layout_layer_word_is_not_masked_into_a_model_layer() {
    let model0 = numeric_line([0., 0.], [10000., 0.]);
    let model1 = numeric_line([0., 0.], [0., 5000.]);
    let mut paper = numeric_line([10., 15.], [30., 35.]);
    // Native layout-live: MPXL layer1/segment1 has this complete raw word.
    // No general bit-field decoding or layout coordinate conversion is implied.
    paper[2..4].copy_from_slice(&0x0201u16.to_le_bytes());
    let records = [
        model0,
        model1,
        slot(0x041d, 4),
        slot(0xfe, 0),
        slot(0xfe, 0),
        slot(0x52, 0),
        paper,
        slot(0x1e, 4),
        slot(0xfe, 0),
        slot(0xfe, 0),
        slot(0x51, 0),
    ];
    let data = fixture(25, &records, &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(doc.line_records().count(), 3);
    for profile in PROFILES {
        for (index, expected) in [(0, [10000., 0.]), (1, [0., 5000.])] {
            let value = qualify_lines(&doc, index, profile, &SceneLimits::default()).unwrap();
            assert_eq!(value.lines()[0].scene().end, expected);
        }
        for index in 2..11 {
            assert!(matches!(
                qualify_lines(&doc, index, profile, &SceneLimits::default()),
                Err(DracadError::UnsupportedFeature(_))
            ));
        }
    }
    assert_eq!(doc.contents(), data);
    // The supported layer256 boundary is distinct from the observed 0x0201.
    let mut layer256 = model0;
    layer256[2..4].copy_from_slice(&256u16.to_le_bytes());
    let data = fixture(25, &[layer256], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for profile in PROFILES {
        assert!(qualify_lines(&doc, 0, profile, &SceneLimits::default()).is_ok());
    }
}

#[test]
fn native_observed_unknown_terminal_flags_do_not_gain_a_raw_or_qualified_profile() {
    let mut data = fixture(25, &[numeric_line([0., 0.], [10000., 0.])], &[]);
    let terminal = offset(&data, 0x28) - 64;
    data[terminal..terminal + 16].copy_from_slice(&[
        0xff, 0x04, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x00, 0x04, 0x00, 0x00,
        0x00,
    ]);
    assert!(matches!(
        scan_contents(&data, &ParseLimits::default()),
        Err(DracadError::InvalidFormat(_))
    ));
    assert_eq!(&data[terminal..terminal + 2], &0x04ffu16.to_le_bytes());
}

#[test]
fn explicit_opt_in_keeps_stored_xy_source_unknowns_and_unrelated_objects() {
    let data = fixture(25, &[line(2), slot(0x7777, 1)], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let result = qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()).unwrap();
    assert_eq!(result.profile(), PROFILE);
    assert_eq!(result.record_index(), 0);
    assert_eq!(result.coordinate_semantics(), "stored_xy");
    assert!(!result.document_complete());
    let line = &result.lines()[0];
    assert_eq!(line.coordinate_semantics(), "stored_xy");
    assert_eq!(line.units(), None);
    assert!(line
        .unresolved_semantics()
        .contains(&UnresolvedLineSemantics::Visibility));
    assert!(line
        .unresolved_semantics()
        .contains(&UnresolvedLineSemantics::Layout));
    assert!(line
        .unresolved_semantics()
        .contains(&UnresolvedLineSemantics::DocumentCompleteness));
    assert_eq!(line.scene().start, [-12.5, 0.25]);
    assert_eq!(
        line.scene().coordinate_scope,
        SceneCoordinateScope::RootRecords
    );
    assert_eq!(line.scene().attributes.material_index_raw, 17);
    assert_eq!(line.scene().attributes.group_index, 301);
    assert_eq!(doc.contents(), data);
    assert!(qualify_lines(&doc, 1, PROFILE, &SceneLimits::default()).is_err());
    assert_eq!(Line2DProfile::from_id(PROFILE.as_str()).unwrap(), PROFILE);
    assert!(Line2DProfile::from_id("wcs_mm").is_err());
}

#[test]
fn bylayer_and_known_two_reference_byblock_scope_are_admitted() {
    let data = fixture(25, &[line(0x2402)], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let lines = qualify_lines(&doc, 0, PROFILE, &SceneLimits::default())
        .unwrap()
        .into_lines();
    assert_eq!(lines[0].scene().styles.color.index, 3);
    assert_eq!(lines[0].scene().styles.width.index, 7);
    let data = fixture(
        25,
        &reference(6, b"A"),
        &[(b"A", reference(0x4006, b"B")), (b"B", vec![line(0x4402)])],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let value = qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()).unwrap();
    assert_eq!(value.lines()[0].scene().ancestors.len(), 2);
    assert_eq!(value.lines()[0].scene().styles.color.sources.len(), 4);
    assert_eq!(
        value.lines()[0].scene().coordinate_scope,
        SceneCoordinateScope::PackageParent
    );
}

#[test]
fn explicit_mixed_inline_path_is_admitted_but_v1_depth_cannot_drift_with_scene() {
    let data = fixture(
        25,
        &reference(6, b"A"),
        &[(b"A", vec![inline(3), inline(2), line(2)])],
    );
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert_eq!(
        qualify_lines(&doc, 0, PROFILE, &SceneLimits::default())
            .unwrap()
            .lines()[0]
            .scene()
            .ancestors
            .len(),
        3
    );
    for (root, definitions) in [
        (vec![inline(4), inline(3), inline(2), line(2)], vec![]),
        (
            reference(6, b"A"),
            vec![
                (b"A".as_slice(), reference(6, b"B")),
                (b"B".as_slice(), reference(6, b"C")),
                (b"C".as_slice(), vec![line(2)]),
            ],
        ),
        (
            reference(6, b"A"),
            vec![
                (b"A".as_slice(), reference(6, b"B")),
                (b"B".as_slice(), vec![inline(3), inline(2), line(2)]),
            ],
        ),
    ] {
        let data = fixture(25, &root, &definitions);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(doc.scene_lines(0, &SceneLimits::default()).is_ok());
        assert!(matches!(
            qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn old_profile_matching_coordinates_do_not_gain_v1_qualification() {
    for marker in [5, 8] {
        let data = fixture(marker, &[line(2)], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert_eq!(
            doc.line_records().next().unwrap().unwrap().start,
            [-12.5, 0.25]
        );
        assert!(matches!(
            qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn unsupported_path_nonplanar_and_work_limits_fail_without_partial_output() {
    for mut bad in [line(0x802), line(0x402)] {
        if bad[..2] == 0x402u16.to_le_bytes() {
            bad[48..56].copy_from_slice(&2f64.to_le_bytes());
        }
        let data = fixture(25, &[inline(3), line(2), bad], &[]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()).is_err());
    }
    let data = fixture(25, &[inline(2), line(0x2402)], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()).is_err());
    let data = fixture(25, &reference(6, b"A"), &[(b"A", vec![])]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc
        .scene_lines(0, &SceneLimits::default())
        .unwrap()
        .is_empty());
    assert!(qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()).is_err());
    let data = fixture(25, &[line(2)], &[]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        qualify_lines(
            &doc,
            0,
            PROFILE,
            &SceneLimits {
                max_lines: 0,
                ..Default::default()
            }
        ),
        Err(DracadError::LimitExceeded { .. })
    ));
    assert_eq!(doc.contents(), data);
}

#[test]
fn inline_profile_is_separate_preserves_actual_profile_and_ignores_wrapper_styles() {
    let expanded = Line2DProfile::Marker25PlanarStoredXyInlineV1;
    assert_eq!(Line2DProfile::from_id(expanded.as_str()).unwrap(), expanded);
    for tag in [0x30u16, 0x430, 0x2030, 0x4030] {
        for child_tag in [0x2402, 0x4402] {
            let mut wrapper = inline(2);
            wrapper[..2].copy_from_slice(&tag.to_le_bytes());
            wrapper[2..4].copy_from_slice(&9u16.to_le_bytes());
            wrapper[4..7].copy_from_slice(&[7, 4, 9]);
            if tag == 0x2030 {
                wrapper[4..7].fill(0);
            }
            if tag == 0x4030 {
                wrapper[4..7].copy_from_slice(&[0, 255, 255]);
            }
            let mut child = line(child_tag);
            child[2..4].fill(0);
            let data = fixture(25, &reference(6, b"A"), &[(b"A", vec![wrapper, child])]);
            let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
            assert!(qualify_lines(&doc, 0, PROFILE, &SceneLimits::default()).is_err());
            let selected = qualify_lines(&doc, 0, expanded, &SceneLimits::default()).unwrap();
            assert_eq!(selected.profile(), expanded);
            assert_eq!(selected.lines()[0].profile(), expanded);
            let scene = selected.lines()[0].scene();
            assert_eq!(
                [
                    scene.styles.color.index,
                    scene.styles.line_type.index,
                    scene.styles.width.index
                ],
                if child_tag == 0x2402 {
                    [3, 4, 7]
                } else {
                    [1, 1, 1]
                }
            );
            assert_eq!(scene.ancestors[1].raw_bytes, wrapper);
            assert_eq!(
                scene
                    .styles
                    .color
                    .sources
                    .iter()
                    .filter(|s| s.kind == ezdracad_core::StyleSourceKind::BlockPlacement)
                    .count(),
                1
            );
        }
    }
    for tag in [0x4430u16, 0x6030, 0x4406] {
        let mut bad = inline(2);
        bad[..2].copy_from_slice(&tag.to_le_bytes());
        let data = fixture(25, &reference(6, b"A"), &[(b"A", vec![bad, line(2)])]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(qualify_lines(&doc, 0, expanded, &SceneLimits::default()).is_err());
    }
}
