use ezdracad_core::{
    scan_contents, world_input_3d_context, DracadError, ParseLimits, WorldInput3DProfile,
};

const PROFILE: WorldInput3DProfile = WorldInput3DProfile::Marker25LoadedWorldInputV1;

fn fixture(marker: u16) -> Vec<u8> {
    let mut data = match marker {
        5 => include_bytes!("../../../tests/fixtures/records5.bin").to_vec(),
        8 => include_bytes!("../../../tests/fixtures/records8.bin").to_vec(),
        25 => include_bytes!("../../../tests/fixtures/records25.bin").to_vec(),
        _ => unreachable!(),
    };
    for (offset, value) in [
        (0x140, 100_f64),
        (0x148, -50.),
        (0x178, 0.01),
        (0x1b0, 0.),
        (0x278, 0.5),
    ] {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    for offset in [0x150, 0x154, 0x1ac, 0x274] {
        data[offset..offset + 4].copy_from_slice(&0i32.to_le_bytes());
    }
    data
}

fn context(data: &[u8]) -> ezdracad_core::WorldInput3DContext {
    let document = scan_contents(data, &ParseLimits::default()).unwrap();
    world_input_3d_context(&document, PROFILE).unwrap()
}

#[test]
fn source_snapshot_and_loaded_zero_z_convention_are_explicit() {
    let mut data = fixture(25);
    data[0x158..0x160].copy_from_slice(&9876.25_f64.to_le_bytes());
    data[0x160..0x168].copy_from_slice(&(-43.5_f64).to_le_bytes());
    let value = context(&data);
    assert_eq!(value.profile(), PROFILE);
    assert_eq!(value.profile().as_str(), "marker25_loaded_world_input_v1");
    assert_eq!(value.loaded_world_origin(), [100., -50., 0.]);
    assert_eq!(value.origin_z_source(), "loaded_file_zero_z_convention");
    assert_eq!(value.draw_origin_offset(), 0x154);
    assert_eq!(value.draw_origin_bytes(), [0; 4]);
    assert_eq!(value.draw_origin_raw(), 0);
    assert_eq!(value.evidence().header_bytes, data[0x140..0x280]);
    assert_eq!(value.evidence().header_offset, 0x140);
    assert_eq!(
        value.coordinate_context().measurement().annotation_ratio,
        0.5
    );
    assert_eq!(value.units(), "mm");
    assert_eq!(
        value.coordinate_semantics(),
        "loaded_world_absolute_input_mm"
    );
    assert_eq!(
        value
            .world_stored_xyz_from_input_mm([20., 10., 5.])
            .unwrap(),
        [110., -45., 2.5]
    );
}

#[test]
fn observed_scale_and_numeric_input_conventions_keep_literal_units_separate() {
    for (ratio, expected) in [(0.5_f64, [110., -45., 2.5]), (0.25, [105., -47.5, 1.25])] {
        let mut data = fixture(25);
        data[0x278..0x280].copy_from_slice(&ratio.to_le_bytes());
        let value = context(&data);
        assert_eq!(
            value
                .world_stored_xyz_from_input_mm([20., 10., 5.])
                .unwrap(),
            expected
        );
        assert_eq!(
            value.input_mm_from_world_stored_xyz(expected).unwrap(),
            [20., 10., 5.]
        );
    }
    let mut data = fixture(25);
    data[0x150..0x154].copy_from_slice(&2i32.to_le_bytes());
    let value = context(&data);
    let literal = [20., 10., 5.];
    let mm = literal.map(|component| value.input_unit().to_native_mm(component).unwrap());
    assert_eq!(mm, [20000., 10000., 5000.]);
    let stored = value.world_stored_xyz_from_input_mm(mm).unwrap();
    assert_eq!(stored, [10100., 4950., 2500.]);
    let recovered = value.input_mm_from_world_stored_xyz(stored).unwrap();
    assert_eq!(
        recovered.map(|component| value.input_unit().from_native_mm(component).unwrap()),
        literal
    );
}

#[test]
fn unrelated_grid_origin_values_never_become_world_origin_z() {
    let data = fixture(25);
    let expected = context(&data)
        .input_mm_from_world_stored_xyz([110., -45., 2.5])
        .unwrap();
    for grid_x in [0., 75., -1234.5, f64::NAN] {
        let mut changed = data.clone();
        changed[0x158..0x160].copy_from_slice(&grid_x.to_le_bytes());
        let value = context(&changed);
        assert_eq!(
            value
                .input_mm_from_world_stored_xyz([110., -45., 2.5])
                .unwrap(),
            expected
        );
        assert_eq!(value.evidence().header_bytes, changed[0x140..0x280]);
    }
}

#[test]
fn unobserved_markers_selectors_and_active_draft_states_fail_on_demand() {
    for marker in [5, 8] {
        let document = scan_contents(&fixture(marker), &ParseLimits::default()).unwrap();
        assert!(matches!(
            document.world_input_3d_context(PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert!(document.line_records().next().unwrap().is_ok());
    }
    for (offset, raw) in [
        (0x150, 1i32),
        (0x150, 3),
        (0x150, 4),
        (0x150, 5),
        (0x150, 6),
        (0x150, -1),
        (0x154, 1),
        (0x154, -1),
        (0x1ac, 1),
        (0x274, 1),
    ] {
        let mut data = fixture(25);
        data[offset..offset + 4].copy_from_slice(&raw.to_le_bytes());
        let document = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            document.world_input_3d_context(PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
        assert_eq!(document.contents(), data);
    }
    let mut data = fixture(25);
    data[0x1b0..0x1b8].copy_from_slice(&30f64.to_le_bytes());
    let document = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        document.world_input_3d_context(PROFILE),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        WorldInput3DProfile::from_id("automatic"),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn finite_and_positive_current_context_is_required_without_losing_raw_sources() {
    for (offset, raw) in [
        (0x140, f64::NAN),
        (0x148, f64::INFINITY),
        (0x178, 0.),
        (0x278, -1.),
        (0x278, f64::NAN),
    ] {
        let mut data = fixture(25);
        data[offset..offset + 8].copy_from_slice(&raw.to_le_bytes());
        let document = scan_contents(&data, &ParseLimits::default()).unwrap();
        assert!(matches!(
            document.world_input_3d_context(PROFILE),
            Err(DracadError::InvalidFormat(_))
        ));
        assert_eq!(document.contents(), data);
    }
}

#[test]
fn checked_xyz_arithmetic_preserves_signed_z_zero_and_rejects_lost_values() {
    let value = context(&fixture(25));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for axis in 0..3 {
            let mut point = [0.; 3];
            point[axis] = invalid;
            assert!(value.input_mm_from_world_stored_xyz(point).is_err());
            assert!(value.world_stored_xyz_from_input_mm(point).is_err());
        }
    }
    assert!(value
        .input_mm_from_world_stored_xyz([100., -50., -0.])
        .unwrap()[2]
        .is_sign_negative());
    assert!(value.world_stored_xyz_from_input_mm([0., 0., -0.]).unwrap()[2].is_sign_negative());
    assert!(value
        .input_mm_from_world_stored_xyz([100., -50., f64::MAX])
        .is_err());
    assert!(value
        .world_stored_xyz_from_input_mm([0., 0., f64::from_bits(1)])
        .is_err());
    let mut data = fixture(25);
    data[0x278..0x280].copy_from_slice(&2f64.to_le_bytes());
    let value = context(&data);
    assert!(value
        .world_stored_xyz_from_input_mm([0., 0., f64::MAX])
        .is_err());
    assert!(value
        .input_mm_from_world_stored_xyz([100., -50., f64::from_bits(1)])
        .is_err());
    assert!(value
        .world_stored_xyz_from_input_mm([1e-300, 0., 0.])
        .is_err());
}
