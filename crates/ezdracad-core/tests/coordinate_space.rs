//! Synthetic arithmetic/provenance checks, separate from native frame evidence.
use ezdracad_core::{
    coordinate_space_context, qualify_lines_in_coordinate_space, scan_contents, CoordinateFrame,
    CoordinateSpaceProfile, DracadError, Line2DProfile, ParseLimits, SceneLimits,
};

const PROFILE: CoordinateSpaceProfile = CoordinateSpaceProfile::Marker25StoredAxesV1;
const LINE: Line2DProfile = Line2DProfile::Marker25PlanarStoredXyV1;

fn number(data: &mut [u8], at: usize, value: f64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn metadata(data: &mut [u8]) {
    number(data, 0x140, 100.0);
    number(data, 0x148, -50.0);
    data[0x150..0x154].fill(0);
    number(data, 0x178, 0.01);
    number(data, 0x278, 0.5);
}

fn fixture() -> Vec<u8> {
    let mut data = include_bytes!("../../../tests/fixtures/semantics25.bin").to_vec();
    metadata(&mut data);
    data[8192..8256].fill(0);
    data[8192..8194].copy_from_slice(&2u16.to_le_bytes());
    data[8194..8196].copy_from_slice(&1u16.to_le_bytes());
    data[8196..8199].copy_from_slice(&[1, 1, 1]);
    data[8199] = 19;
    data[8200..8202].copy_from_slice(&1u16.to_le_bytes());
    data[8202..8204].copy_from_slice(&1234u16.to_le_bytes());
    for (i, value) in [110.0, -45.0, 130.0, -35.0, 0.0, 0.0]
        .into_iter()
        .enumerate()
    {
        number(&mut data, 8208 + i * 8, value);
    }
    data
}

fn scan(data: &[u8]) -> ezdracad_core::RawDocument {
    scan_contents(data, &ParseLimits::default()).unwrap()
}

#[test]
fn explicit_frames_roundtrip_and_retain_exact_owned_source_context() {
    let mut data = fixture();
    let doc = scan(&data);
    let context = coordinate_space_context(&doc, PROFILE).unwrap();
    assert_eq!(context.evidence().contents_sha256, doc.sha256());
    assert_eq!(context.evidence().header_offset, 0x140);
    assert_eq!(context.evidence().header_bytes, data[0x140..0x280]);
    assert_eq!(context.drawing_origin(), [100.0, -50.0]);
    assert_eq!(context.origin_units_raw(), 0);
    assert_eq!(context.draft().angle_offset, 0x1b0);
    assert_eq!(context.draft().object_offset, 0x1ac);
    assert_eq!(context.draft().enable_offset, 0x274);
    assert_eq!(context.draft().angle_degrees, 0.0);
    assert_eq!(context.draft().object_raw, 0);
    assert_eq!(context.draft().enable_raw, 0);
    for (frame, expected) in [
        (CoordinateFrame::RootStoredXy, [110.0, -45.0]),
        (CoordinateFrame::DrawingOriginRelativeStoredXy, [10.0, 5.0]),
        (
            CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin,
            [20.0, 10.0],
        ),
        (
            CoordinateFrame::ConfiguredPaperMmOffsetFromDrawingOrigin,
            [0.1, 0.05],
        ),
    ] {
        assert_eq!(
            context.from_root_stored_xy([110.0, -45.0], frame).unwrap(),
            expected
        );
        assert_eq!(
            context.to_root_stored_xy(expected, frame).unwrap(),
            [110.0, -45.0]
        );
    }
    data[0x140..0x280].fill(0xa5);
    assert_eq!(context.drawing_origin(), [100.0, -50.0]);
    assert_eq!(context.measurement().annotation_ratio, 0.5);
    assert_eq!(CoordinateFrame::RootStoredXy.units(), None);
    assert_eq!(
        CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin.units(),
        Some("mm")
    );
}

#[test]
fn same_document_selection_retains_qualified_source_units_and_unknown_semantics() {
    let doc = scan(&fixture());
    let result = qualify_lines_in_coordinate_space(
        &doc,
        0,
        LINE,
        PROFILE,
        CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin,
        &SceneLimits::default(),
    )
    .unwrap();
    assert_eq!(
        result.source().lines(),
        doc.qualified_lines(0, LINE, &SceneLimits::default())
            .unwrap()
            .lines()
    );
    assert_eq!(result.coordinates()[0].start, [20.0, 10.0]);
    assert_eq!(result.coordinates()[0].end, [60.0, 30.0]);
    assert_eq!(result.source().lines()[0].units(), None);
    assert!(result.source().lines()[0]
        .unresolved_semantics()
        .iter()
        .any(|x| x.as_str() == "drawing_wcs"));
    assert!(!result.document_complete());
    assert_eq!(
        result.source().lines()[0]
            .scene()
            .attributes
            .material_index_raw,
        19
    );
}

#[test]
fn placement_occurs_before_origin_subtraction_and_context_scaling() {
    let mut data = include_bytes!("../../../tests/fixtures/blocks25.bin").to_vec();
    metadata(&mut data);
    // This block-layout fixture intentionally has an opaque suffix. Replace
    // only that style table with the independent qualified synthetic table.
    let styles = include_bytes!("../../../tests/fixtures/semantics25.bin");
    let source_end = u32::from_le_bytes(styles[0x50..0x54].try_into().unwrap()) as usize;
    let target_end = u32::from_le_bytes(data[0x50..0x54].try_into().unwrap()) as usize;
    data[target_end - 7272..target_end].copy_from_slice(&styles[source_end - 7272..source_end]);
    for field in (0x28..0x40).step_by(4) {
        let start = u32::from_le_bytes(data[field..field + 4].try_into().unwrap()) as usize;
        let end = u32::from_le_bytes(data[field + 4..field + 8].try_into().unwrap()) as usize;
        let source_start =
            u32::from_le_bytes(styles[field..field + 4].try_into().unwrap()) as usize;
        data[start..end].copy_from_slice(&styles[source_start..source_start + end - start]);
    }
    // Independently assembled block fixture: root reference TWO translates
    // (1.25,-2.5) by (-25,75). Set explicit styles; names/placements stay intact.
    data[8194..8196].copy_from_slice(&1u16.to_le_bytes());
    data[8196..8199].copy_from_slice(&[1, 1, 1]);
    let first = u32::from_le_bytes(data[0x4c..0x50].try_into().unwrap()) as usize;
    let count = u32::from_le_bytes(data[first + 36..first + 40].try_into().unwrap()) as usize;
    let second = first + 176 + (count + 1) * 64;
    for at in [second + 176, second + 240] {
        data[at + 2..at + 4].copy_from_slice(&1u16.to_le_bytes());
        data[at + 4..at + 7].copy_from_slice(&[1, 1, 1]);
    }
    let result = qualify_lines_in_coordinate_space(
        &scan(&data),
        0,
        LINE,
        PROFILE,
        CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin,
        &SceneLimits::default(),
    )
    .unwrap();
    assert_eq!(result.coordinates().len(), 2);
    let source = &result.source().lines()[0].scene();
    assert_eq!(source.start, [-23.75, 72.5]);
    assert_eq!(source.source.start, [1.25, -2.5]);
    assert_eq!(result.coordinates()[0].start, [-247.5, 245.0]);
    assert_eq!(source.ancestors.len(), 1);
    assert_eq!(source.ancestors[0].raw_bytes.len(), 128);
}

#[test]
fn invalid_metadata_unknown_profiles_and_raw_unit_codes_fail_closed() {
    for (at, value) in [
        (0x140, f64::NAN),
        (0x148, f64::INFINITY),
        (0x178, 0.0),
        (0x178, -0.1),
        (0x178, f64::INFINITY),
        (0x278, 0.0),
        (0x278, f64::NAN),
        (0x178, f64::from_bits(1)),
        (0x1b0, f64::NAN),
    ] {
        let mut data = fixture();
        number(&mut data, at, value);
        assert!(coordinate_space_context(&scan(&data), PROFILE).is_err());
    }
    for value in [-1i32, 7, i32::MAX] {
        let mut data = fixture();
        data[0x150..0x154].copy_from_slice(&value.to_le_bytes());
        assert!(matches!(
            coordinate_space_context(&scan(&data), PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    for field in [0x1ac, 0x274] {
        for value in [-1i32, 2, i32::MAX] {
            let mut data = fixture();
            data[field..field + 4].copy_from_slice(&value.to_le_bytes());
            assert!(matches!(
                coordinate_space_context(&scan(&data), PROFILE),
                Err(DracadError::UnsupportedFeature(_))
            ));
        }
    }
    let mut rotated = fixture();
    number(&mut rotated, 0x1b0, 361.0);
    assert!(matches!(
        coordinate_space_context(&scan(&rotated), PROFILE),
        Err(DracadError::UnsupportedFeature(_))
    ));
    for (fixture, matching_profile) in [
        (
            include_bytes!("../../../tests/fixtures/semantics5.bin").as_slice(),
            CoordinateSpaceProfile::Marker5StoredAxesV1,
        ),
        (
            include_bytes!("../../../tests/fixtures/semantics8.bin").as_slice(),
            CoordinateSpaceProfile::Marker8StoredAxesV1,
        ),
    ] {
        let mut data = fixture.to_vec();
        metadata(&mut data);
        let doc = scan(&data);
        assert!(matches!(
            coordinate_space_context(&doc, PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
        let context = coordinate_space_context(&doc, matching_profile).unwrap();
        assert_eq!(
            context
                .from_root_stored_xy(
                    [110., -45.],
                    CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin
                )
                .unwrap(),
            [20., 10.]
        );
    }
    assert!(CoordinateFrame::from_id("wcs").is_err());
    assert!(CoordinateSpaceProfile::from_id("automatic").is_err());
}

#[test]
fn input_unit_codes_affect_literal_digits_without_rescaling_existing_stored_axes() {
    let mut data = fixture();
    data[0x150..0x154].copy_from_slice(&1i32.to_le_bytes());
    let doc = scan(&data);
    let context = coordinate_space_context(&doc, PROFILE).unwrap();
    assert_eq!(
        context.input_unit().origin_units().selector_bytes,
        1i32.to_le_bytes()
    );
    assert_eq!(context.input_unit().native_display_factor(), Some(0.1));
    let millimeters = context
        .from_root_stored_xy(
            [0.0, -100.0],
            CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
        )
        .unwrap();
    assert_eq!(millimeters, [-200.0, -100.0]);
    assert_eq!(
        context.input_unit().from_native_mm(millimeters[0]).unwrap(),
        -20.0
    );
    assert_eq!(
        context.input_unit().from_native_mm(millimeters[1]).unwrap(),
        -10.0
    );
    let selected = qualify_lines_in_coordinate_space(
        &doc,
        0,
        LINE,
        PROFILE,
        CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
        &SceneLimits::default(),
    )
    .unwrap();
    assert_eq!(selected.source().lines()[0].scene().start, [110.0, -45.0]);
    assert_eq!(selected.source().lines()[0].units(), None);
    assert_eq!(selected.coordinates()[0].start, [20.0, 10.0]);
    let mut legacy = include_bytes!("../../../tests/fixtures/semantics8.bin").to_vec();
    metadata(&mut legacy);
    legacy[0x150..0x154].copy_from_slice(&1i32.to_le_bytes());
    assert!(matches!(
        coordinate_space_context(&scan(&legacy), CoordinateSpaceProfile::Marker8StoredAxesV1),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn saved_axes_metric_and_active_absolute_input_frame_are_distinct() {
    for (angle, object) in [(30.0f64, 0i32), (30.0, 1), (90.0, 0)] {
        let mut data = fixture();
        number(&mut data, 0x1b0, angle);
        data[0x1ac..0x1b0].copy_from_slice(&object.to_le_bytes());
        data[0x274..0x278].copy_from_slice(&1i32.to_le_bytes());
        let context = coordinate_space_context(&scan(&data), PROFILE).unwrap();
        // Independently calculated stored displacement for input (20,10), r=.5.
        let stored = if angle == 90.0 {
            [95.0, -40.0]
        } else {
            [106.16025403784438, -40.66987298107781]
        };
        let input = context
            .from_root_stored_xy(
                stored,
                CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
            )
            .unwrap();
        assert!((input[0] - 20.0).abs() < 1e-12 && (input[1] - 10.0).abs() < 1e-12);
        let metric = context
            .from_root_stored_xy(
                stored,
                CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin,
            )
            .unwrap();
        assert_ne!(metric, input);
        let roundtrip = context
            .to_root_stored_xy(
                input,
                CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
            )
            .unwrap();
        assert!(
            (roundtrip[0] - stored[0]).abs() < 1e-12 && (roundtrip[1] - stored[1]).abs() < 1e-12
        );
    }
    let mut legacy = include_bytes!("../../../tests/fixtures/semantics8.bin").to_vec();
    metadata(&mut legacy);
    legacy[0x274..0x278].copy_from_slice(&1i32.to_le_bytes());
    assert!(matches!(
        coordinate_space_context(&scan(&legacy), CoordinateSpaceProfile::Marker8StoredAxesV1),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn arithmetic_guards_nonfinite_overflow_underflow_and_lost_translation() {
    let context = coordinate_space_context(&scan(&fixture()), PROFILE).unwrap();
    for point in [[f64::NAN, 1.0], [1.0, f64::INFINITY]] {
        for frame in [
            CoordinateFrame::RootStoredXy,
            CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin,
        ] {
            assert!(context.from_root_stored_xy(point, frame).is_err());
            assert!(context.to_root_stored_xy(point, frame).is_err());
        }
    }
    assert!(context
        .from_root_stored_xy(
            [f64::MAX, 0.0],
            CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin
        )
        .is_err());
    assert!(context
        .to_root_stored_xy(
            [f64::from_bits(1), 0.0],
            CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin
        )
        .is_err());
    assert!(context
        .to_root_stored_xy(
            [f64::EPSILON, 0.0],
            CoordinateFrame::DrawingOriginRelativeStoredXy
        )
        .is_err());
    let mut data = fixture();
    number(&mut data, 0x140, 0.0);
    let context = coordinate_space_context(&scan(&data), PROFILE).unwrap();
    assert!(context
        .from_root_stored_xy(
            [f64::from_bits(1), 0.0],
            CoordinateFrame::ConfiguredPaperMmOffsetFromDrawingOrigin
        )
        .is_err());
}
