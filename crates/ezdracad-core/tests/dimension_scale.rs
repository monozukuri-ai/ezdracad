use ezdracad_core::{scan_contents, DimensionScaleProfile, DracadError, ParseLimits};

const START: usize = 8192;
const PROFILE: DimensionScaleProfile = DimensionScaleProfile::Marker25AlignedNumericMmV1;

fn fixture() -> Vec<u8> {
    include_bytes!("../../../tests/fixtures/dimension25.bin").to_vec()
}

fn parse(data: &[u8]) -> ezdracad_core::RawDocument {
    scan_contents(data, &ParseLimits::default()).unwrap()
}

#[test]
fn retains_exact_owned_sources_without_claiming_history_or_geometry_units() {
    let mut data = fixture();
    data[START + 48] = 0xa5; // An opaque dimension field must survive unchanged.
    data[START + 8 * 64 + 16] = b'X'; // Display text does not replace the saved number.
    let document = parse(&data);
    let result = document.dimension_scale(0, PROFILE).unwrap();
    assert_eq!(result.record_bytes, data[START..START + 640]);
    assert_eq!(result.contents_sha256, document.sha256());
    assert_eq!(result.recorded_value_mm, 2000.0);
    assert_eq!(result.stored_span_length, 1000.0);
    assert_eq!(result.annotation_to_drawing_ratio, 2.0);
    assert_eq!(result.equivalent_annotation_denominator, 200.0);
    assert_eq!(result.coordinate_space(), "record_local");
    assert_eq!(result.geometry_units(), None);
    assert!(!result.document_complete());
    assert_eq!(
        result.original_scale_resolution(),
        "ratio_only_original_denominators_unavailable"
    );
    assert_eq!(result.original_drawing_denominator(), None);
    assert_eq!(result.original_annotation_denominator(), None);
    for (offset, bytes) in [
        (result.number_offset, result.number_bytes.as_slice()),
        (result.geometry_offset, result.geometry_bytes.as_slice()),
        (
            result.numeric_flag_offset,
            result.numeric_flag_bytes.as_slice(),
        ),
        (result.unit_offset, result.unit_bytes.as_slice()),
        (
            result.current_drawing_scale_offset,
            result.current_drawing_scale_bytes.as_slice(),
        ),
    ] {
        assert_eq!(&data[offset as usize..offset as usize + bytes.len()], bytes);
    }
    drop(document);
    drop(data);
    assert_eq!(result.number_bytes, 2000.0_f64.to_le_bytes());
}

#[test]
fn current_annotation_is_not_used_and_current_d_is_not_called_original_d() {
    let mut data = fixture();
    data[0x278..0x280].copy_from_slice(&f64::NAN.to_le_bytes());
    let first = parse(&data).dimension_scale(0, PROFILE).unwrap();
    data[0x178..0x180].copy_from_slice(&0.04_f64.to_le_bytes());
    let second = parse(&data).dimension_scale(0, PROFILE).unwrap();
    assert_eq!(
        first.annotation_to_drawing_ratio,
        second.annotation_to_drawing_ratio
    );
    assert_eq!(second.equivalent_annotation_denominator, 50.0);
    assert_eq!(second.original_drawing_denominator(), None);
    assert!(parse(&data).coordinate_measurement_context().is_err());
}

#[test]
fn exact_observed_tag_variants_are_preserved_without_masking_unknown_flags() {
    for tag in [0x1a_u16, 0x41a] {
        let mut data = fixture();
        data[START..START + 2].copy_from_slice(&tag.to_le_bytes());
        let value = parse(&data).dimension_scale(0, PROFILE).unwrap();
        assert_eq!(value.tag_raw, tag);
        assert_eq!(&value.record_bytes[..2], &tag.to_le_bytes());
        assert_eq!(value.annotation_to_drawing_ratio, 2.0);
    }
    let mut data = fixture();
    data[START..START + 2].copy_from_slice(&0x81a_u16.to_le_bytes());
    assert!(matches!(
        parse(&data).dimension_scale(0, PROFILE),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn unknown_profile_flags_units_and_child_layout_fail_on_demand() {
    for (offset, value) in [
        (0x22, 8),
        (START, 0x1b),
        (START + 16, 2),
        (START + 17, 129),
        (START + 18, 3),
        (START + 64, 0x42),
        (START + 5 * 64 + 8, 3),
        (START + 6 * 64 + 12, 0),
        (START + 7 * 64 + 2, 17),
        (START + 7 * 64 + 4, 2),
    ] {
        let mut data = fixture();
        data[offset] = value;
        let document = parse(&data);
        assert!(
            matches!(
                document.dimension_scale(0, PROFILE),
                Err(DracadError::UnsupportedFeature(_))
            ),
            "offset{offset}"
        );
        assert_eq!(document.contents(), data);
        assert!(document.record(0).is_some());
    }
    assert!(DimensionScaleProfile::from_id("guess").is_err());
}

#[test]
fn interior_terminal_and_absent_indices_are_not_promoted_to_root_dimensions() {
    let data = fixture();
    let document = parse(&data);
    for index in [1, 5, 9, 10, 11, u64::MAX] {
        assert!(matches!(
            document.dimension_scale(index, PROFILE),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    // Put a fully admissible synthetic DIM inside an opaque root package. Its
    // bytes would pass every dimension-field guard without primary membership.
    let mut nested = fixture();
    nested.splice(START..START, [0_u8; 64]);
    for offset in (0x28..0x44).step_by(4).chain([0x4c, 0x50]) {
        let old = u32::from_le_bytes(nested[offset..offset + 4].try_into().unwrap());
        nested[offset..offset + 4].copy_from_slice(&(old + 64).to_le_bytes());
    }
    nested[0x58..0x5c].copy_from_slice(&11_u32.to_le_bytes());
    nested[START..START + 2].copy_from_slice(&0x30_u16.to_le_bytes());
    nested[START + 8..START + 10].copy_from_slice(&11_u16.to_le_bytes());
    assert_eq!(parse(&nested).record(1).unwrap().tag, 0x1a);
    assert!(parse(&nested).dimension_scale(1, PROFILE).is_err());
}

#[test]
fn numerical_failures_do_not_return_partial_or_nonfinite_ratios() {
    for (offset, value) in [
        (START + 64 + 16, f64::NAN),
        (START + 64 + 32, f64::INFINITY),
        (START + 8 * 64 + 8, f64::NAN),
        (START + 8 * 64 + 8, 0.0),
        (START + 8 * 64 + 8, -1.0),
        (START + 8 * 64 + 8, f64::from_bits(1)),
        (0x178, 0.0),
        (0x178, 1e-308),
    ] {
        let mut data = fixture();
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        assert!(
            parse(&data).dimension_scale(0, PROFILE).is_err(),
            "offset{offset}"
        );
    }
    let mut data = fixture();
    data[START + 64 + 48..START + 64 + 56].copy_from_slice(&1.0_f64.to_le_bytes());
    assert!(matches!(
        parse(&data).dimension_scale(0, PROFILE),
        Err(DracadError::UnsupportedFeature(_))
    ));
    let mut data = fixture();
    data[START + 64 + 32..START + 64 + 40].copy_from_slice(&10.0_f64.to_le_bytes());
    assert!(parse(&data).dimension_scale(0, PROFILE).is_err());
}

#[test]
fn scaled_lengths_preserve_large_small_and_subnormal_diagonals_and_finite_ratios() {
    let smallest = f64::from_bits(1);
    for (dx, dy, number, expected_length, expected_ratio) in [
        (3.0, 4.0, 10.0, 5.0, 2.0),
        (-3.0, 4.0, 10.0, 5.0, 2.0),
        (
            1e308,
            1e308,
            1e308,
            1e308 * std::f64::consts::SQRT_2,
            std::f64::consts::FRAC_1_SQRT_2,
        ),
        (3e-308, 4e-308, 1e-307, 5e-308, 2.0),
        (
            3.0 * smallest,
            4.0 * smallest,
            10.0 * smallest,
            5.0 * smallest,
            2.0,
        ),
        (1e308, smallest, 1e308, 1e308, 1.0),
    ] {
        let mut data = fixture();
        for (offset, value) in [
            (START + 64 + 16, 0.0),
            (START + 64 + 24, 0.0),
            (START + 64 + 32, dx),
            (START + 64 + 40, dy),
            (START + 8 * 64 + 8, number),
        ] {
            data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        let result = parse(&data).dimension_scale(0, PROFILE).unwrap();
        assert!(result.stored_span_length.is_finite() && result.stored_span_length > 0.0);
        assert!((result.stored_span_length / expected_length - 1.0).abs() < 1e-14);
        assert!(result.annotation_to_drawing_ratio.is_finite());
        assert!((result.annotation_to_drawing_ratio / expected_ratio - 1.0).abs() < 1e-14);
        assert!(result.equivalent_annotation_denominator.is_finite());
    }
    let mut data = fixture();
    for offset in [START + 64 + 32, START + 64 + 40] {
        data[offset..offset + 8].copy_from_slice(&1.4e308_f64.to_le_bytes());
    }
    assert!(parse(&data).dimension_scale(0, PROFILE).is_err());
}
