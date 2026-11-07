//! Synthetic contract/numerical checks, separate from native workplane proof.
use ezdracad_core::{
    scan_contents, workplane_context, DracadError, ParseLimits, RawDocument, WorkplaneBasisSource,
    WorkplaneDirection, WorkplaneProfile,
};

const PROFILE: WorkplaneProfile = WorkplaneProfile::Marker25SavedWorkplaneV1;
const IDENTITY: [[f64; 3]; 3] = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];

fn number(data: &mut [u8], at: usize, value: f64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn fixture(direction: i32, axes: [[f64; 3]; 3]) -> Vec<u8> {
    let mut data = include_bytes!("../../../tests/fixtures/semantics25.bin").to_vec();
    data[0x140..0x154].fill(0);
    data[0x1ac..0x1b8].fill(0);
    data[0x274..0x278].fill(0);
    number(&mut data, 0x178, 0.01);
    number(&mut data, 0x278, 0.5);
    for (i, value) in [100., -50., 25.].into_iter().enumerate() {
        number(&mut data, 0x450 + i * 8, value);
    }
    data[0x480..0x484].copy_from_slice(&direction.to_le_bytes());
    // Opaque neighbors stay present; no semantics are assigned to these bits.
    data[0x468..0x480].fill(0xa5);
    data[0x484..0x488].copy_from_slice(&0x12345678u32.to_le_bytes());
    for (i, axis) in axes.into_iter().enumerate() {
        for (j, value) in axis.into_iter().enumerate() {
            number(&mut data, 0x488 + (i * 3 + j) * 8, value);
        }
    }
    data
}

fn scan(data: &[u8]) -> RawDocument {
    scan_contents(data, &ParseLimits::default()).unwrap()
}

fn close(actual: [f64; 3], expected: [f64; 3]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-10, "{a} != {e}");
    }
}

#[test]
fn source_ranges_and_owned_context_survive_caller_byte_changes() {
    let mut data = fixture(1, IDENTITY);
    let doc = scan(&data);
    let context = workplane_context(&doc, PROFILE).unwrap();
    assert_eq!(context.profile(), PROFILE);
    assert_eq!(context.direction(), WorkplaneDirection::Plan);
    assert_eq!(context.direction_raw(), 1);
    assert_eq!(context.origin(), [100., -50., 25.]);
    assert_eq!(context.evidence().contents_sha256, doc.sha256());
    assert_eq!(context.evidence().header_offset, 0x450);
    assert_eq!(context.evidence().header_bytes, data[0x450..0x4d0]);
    assert_eq!(context.evidence().coordinate_guard.header_offset, 0x140);
    assert_eq!(
        context.evidence().coordinate_guard.header_bytes,
        data[0x140..0x280]
    );
    assert_eq!(context.units(), None);
    data[0x450..0x4d0].fill(0);
    assert_eq!(context.origin(), [100., -50., 25.]);
    assert_eq!(
        context.to_world_stored_xyz([7., 11., 13.]).unwrap(),
        [107., -39., 38.]
    );
}

#[test]
fn preset_maps_use_direction_not_the_retained_free_axis_matrix() {
    let inactive_free = [[1., 0., 0.], [0., 0.8, -0.6], [0., 0.6, 0.8]];
    for (direction, expected) in [
        (1, [107., -39., 38.]),
        (2, [113., -43., 36.]),
        (3, [107., -63., 36.]),
        (4, [87., -57., 36.]),
        (5, [93., -37., 36.]),
    ] {
        let context = scan(&fixture(direction, inactive_free))
            .workplane_context(PROFILE)
            .unwrap();
        assert_eq!(context.stored_axes(), inactive_free);
        assert_eq!(context.basis_source(), WorkplaneBasisSource::DerivedPreset);
        assert_eq!(
            context.to_world_stored_xyz([7., 11., 13.]).unwrap(),
            expected
        );
        assert_eq!(
            context.from_world_stored_xyz(expected).unwrap(),
            [7., 11., 13.]
        );
    }
}

#[test]
fn free_axes_and_negative_points_use_a_numerical_affine_inverse() {
    let axes = [[1., 0., 0.], [0., 0.8, -0.6], [0., 0.6, 0.8]];
    let context = scan(&fixture(0, axes)).workplane_context(PROFILE).unwrap();
    assert_eq!(context.direction(), WorkplaneDirection::Free);
    assert_eq!(context.effective_axes(), axes);
    assert_eq!(
        context.basis_source(),
        WorkplaneBasisSource::StoredAxesForFree
    );
    close(
        context.to_world_stored_xyz([7., 11., 13.]).unwrap(),
        [107., -33.4, 28.8],
    );
    for point in [[7., 11., 13.], [-12.5, 0.25, -7.75], [0., 0., 0.]] {
        close(
            context
                .from_world_stored_xyz(context.to_world_stored_xyz(point).unwrap())
                .unwrap(),
            point,
        );
    }
}

#[test]
fn unknown_profiles_units_directions_draft_and_legacy_are_rejected_on_demand() {
    assert!(matches!(
        WorkplaneProfile::from_id("wcs"),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert_eq!(
        WorkplaneProfile::from_id(PROFILE.as_str()).unwrap(),
        PROFILE
    );
    for direction in [-1, 6, i32::MAX] {
        let doc = scan(&fixture(direction, IDENTITY));
        assert!(matches!(
            doc.workplane_context(PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert_eq!(doc.contents()[0x480..0x484], direction.to_le_bytes());
    }
    for (at, bytes) in [
        (0x150, 2i32.to_le_bytes().to_vec()),
        (0x274, 1i32.to_le_bytes().to_vec()),
        (0x1ac, 1i32.to_le_bytes().to_vec()),
        (0x1b0, 30f64.to_le_bytes().to_vec()),
    ] {
        let mut data = fixture(1, IDENTITY);
        data[at..at + bytes.len()].copy_from_slice(&bytes);
        let doc = scan(&data);
        assert!(matches!(
            doc.workplane_context(PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert_eq!(doc.contents(), data);
    }
    for bytes in [
        include_bytes!("../../../tests/fixtures/semantics5.bin").as_slice(),
        include_bytes!("../../../tests/fixtures/semantics8.bin").as_slice(),
    ] {
        assert!(matches!(
            scan(bytes).workplane_context(PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn degenerate_skew_and_left_handed_free_axes_are_unsupported() {
    for axes in [
        [[0., 0., 0.]; 3],
        [[2., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        [[1., 0., 0.], [1., 1., 0.], [0., 0., 1.]],
        [[1., 0., 0.], [0., 1., 0.], [0., 0., -1.]],
    ] {
        assert!(matches!(
            scan(&fixture(0, axes)).workplane_context(PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
}

#[test]
fn nonfinite_sources_points_overflow_and_lost_translation_fail_without_partial_values() {
    for at in [0x450, 0x488, 0x178, 0x278] {
        let mut data = fixture(0, IDENTITY);
        number(&mut data, at, f64::NAN);
        assert!(matches!(
            scan(&data).workplane_context(PROFILE),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let context = scan(&fixture(1, IDENTITY))
        .workplane_context(PROFILE)
        .unwrap();
    for point in [[f64::NAN, 0., 0.], [0., f64::INFINITY, 0.]] {
        assert!(context.to_world_stored_xyz(point).is_err());
        assert!(context.from_world_stored_xyz(point).is_err());
    }
    let mut data = fixture(1, IDENTITY);
    number(&mut data, 0x450, 1e308);
    let context = scan(&data).workplane_context(PROFILE).unwrap();
    assert!(context.to_world_stored_xyz([1e308, 0., 0.]).is_err());
    assert!(context.from_world_stored_xyz([-1e308, 0., 0.]).is_err());
    assert!(context.to_world_stored_xyz([1., 0., 0.]).is_err());
    let (sine, cosine) = std::f64::consts::FRAC_PI_6.sin_cos();
    let data = fixture(0, [[cosine, sine, 0.], [-sine, cosine, 0.], [0., 0., 1.]]);
    let context = scan(&data).workplane_context(PROFILE).unwrap();
    assert!(context
        .to_world_stored_xyz([f64::from_bits(1), 0., 0.])
        .is_err());
}
