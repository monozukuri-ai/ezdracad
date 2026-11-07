use ezdracad_core::{
    scan_contents, Block3DProfile, BlockLineSelection3D, DracadError, ParseLimits,
};

const PROFILE: Block3DProfile = Block3DProfile::Marker25DirectLine3dV1;

fn fixture() -> Vec<u8> {
    include_bytes!("../../../tests/fixtures/block3d25.bin").to_vec()
}

fn select(data: &[u8]) -> ezdracad_core::Result<BlockLineSelection3D> {
    scan_contents(data, &ParseLimits::default())?.placed_block_lines_3d(0, PROFILE, 4096)
}

fn put16(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put64(data: &mut [u8], at: usize, value: f64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn definition(data: &[u8]) -> usize {
    let first = u32::from_le_bytes(data[76..80].try_into().unwrap()) as usize;
    let count = u32::from_le_bytes(data[first + 36..first + 40].try_into().unwrap()) as usize;
    first + 176 + (count + 1) * 64
}

fn approx(actual: [f64; 3], expected: [f64; 3]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!(
            (a - e).abs() < 1e-10,
            "actual {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn atomic_selection_preserves_complete_sources_and_neutral_semantics() {
    let data = fixture();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let selection = doc.placed_block_lines_3d(0, PROFILE, 2).unwrap();
    assert_eq!(selection.profile(), PROFILE);
    assert_eq!(selection.contents_sha256(), doc.sha256());
    assert!(!selection.document_complete());
    assert_eq!(selection.units(), None);
    assert_eq!(selection.definition().index, 1);
    assert_eq!(selection.definition_flag_raw(), 1);
    assert_eq!(
        selection.saved_bounds(),
        [101.75, 48.125, 12.5, -12.5, -2.5, -3.75]
    );
    let source = selection.definition();
    assert_eq!(
        source.header_bytes,
        data[source.header_offset as usize..source.header_offset as usize + 176]
    );
    let reference = selection.reference();
    assert_eq!(reference.name_key(), b"TWO");
    assert_eq!(reference.name_field_bytes(), &data[8224..8256]);
    assert_eq!(reference.translation(), [100., -50., 25.]);
    assert_eq!(reference.scale(), [2., 3., 0.5]);
    assert_eq!(reference.rotation_degrees(), [0.; 3]);
    assert_eq!(reference.units(), None);
    let evidence = reference.evidence();
    assert_eq!(evidence.contents_sha256, doc.sha256());
    assert_eq!(evidence.record_indices, [0, 1, 2]);
    assert_eq!(evidence.payload_offsets, [8192, 8256, 8320]);
    for i in 0..3 {
        assert_eq!(evidence.records[i], data[8192 + i * 64..8256 + i * 64]);
    }
    assert_eq!(selection.lines().len(), 2);
    let line = selection.lines()[0];
    assert_eq!(line.source.start, [1.25, -2.5, 12.5]);
    assert_eq!(line.source.end, [101.75, 48.125, -3.75]);
    assert_eq!(line.start, [102.5, -57.5, 31.25]);
    assert_eq!(line.end, [303.5, 94.375, 23.125]);
    assert_eq!(line.coordinate_space(), "block_parent_stored_xyz");
    assert_eq!(line.source.coordinate_space(), "record_local");
    assert_eq!(line.units(), None);
    assert_eq!(line.source.payload_offset, source.record_start);
    assert_eq!(
        line.source.header_bytes,
        data[source.record_start as usize..source.record_start as usize + 16]
    );
    assert!(doc.block_library().unwrap().placed_line_records(0).is_err());
    assert_eq!(
        doc.block_library()
            .unwrap()
            .planar_line_records(0)
            .unwrap()
            .len(),
        1
    );
    drop(doc);
    assert_eq!(
        reference.apply_xyz([7., 11., 13.]).unwrap(),
        [114., -17., 31.5]
    );
}

#[test]
fn exact_dirty_tag_and_euler_order_follow_the_native_field_contract() {
    let mut data = fixture();
    put16(&mut data, 8192, 0x406);
    put64(&mut data, 8272, -70.);
    put64(&mut data, 8280, 35.);
    put64(&mut data, 8288, -15.);
    put64(&mut data, 8312, 43.);
    put64(&mut data, 8336, 17.);
    put64(&mut data, 8344, -29.);
    let selection = select(&data).unwrap();
    approx(
        selection.lines()[0].start,
        [-63.6048648050769, 28.6581477626273, -10.47831270011],
    );
    approx(
        selection.lines()[0].end,
        [-48.6958607713266, 244.398111701582, 119.009218778176],
    );
    assert_eq!(selection.reference().rotation_degrees(), [17., -29., 43.]);
    for (axis, expected) in [
        (0, [102., -50., 28.]),
        (1, [100., -50., 23.]),
        (2, [100., -48., 25.]),
    ] {
        let mut data = fixture();
        put64(&mut data, [8336, 8344, 8312][axis], 90.);
        let point = if axis == 0 {
            [1., 1., 0.]
        } else {
            [1., 0., 0.]
        };
        approx(
            select(&data).unwrap().reference().apply_xyz(point).unwrap(),
            expected,
        );
    }
}

#[test]
fn unrecognized_reference_shapes_and_nonprimary_selection_are_not_masked() {
    for (offset, value) in [
        (8192, 0x806),
        (8200, 2),
        (8200, 4),
        (8256, 0xfd),
        (8320, 0xfd),
    ] {
        let mut data = fixture();
        put16(&mut data, offset, value);
        assert!(select(&data).is_err(), "offset {offset}");
    }
    for offset in [8204, 8208, 8223, 8258, 8271, 8322, 8360, 8383] {
        let mut data = fixture();
        data[offset] ^= 1;
        assert!(select(&data).is_err(), "offset {offset}");
    }
    let data = fixture();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for index in [1, 2, 3, 4, u64::MAX] {
        assert!(doc.placed_block_lines_3d(index, PROFILE, 4096).is_err());
    }
    let mut data = fixture();
    put16(&mut data, 8192, 0x30);
    put16(&mut data, 8192 + 64, 6);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(doc.placed_block_lines_3d(1, PROFILE, 4096).is_err());
}

#[test]
fn unknown_definition_fields_and_late_unsupported_children_fail_atomically() {
    let data = fixture();
    let header = definition(&data);
    for offset in [32, 40, 44, 111, 160, 175] {
        let mut bad = data.clone();
        bad[header + offset] ^= 1;
        assert!(select(&bad).is_err(), "offset {offset}");
    }
    for tag in [0x413, 2, 6, 0x30, 0xfe, 0x7f] {
        let mut bad = data.clone();
        put16(&mut bad, header + 176 + 64, tag);
        assert!(select(&bad).is_err(), "tag {tag}");
    }
    for span in [0, 2, 0xffff] {
        let mut bad = data.clone();
        put16(&mut bad, header + 176 + 64 + 8, span);
        assert!(select(&bad).is_err());
    }
    for offset in 12..16 {
        let mut bad = data.clone();
        bad[header + 176 + 64 + offset] = 1;
        assert!(select(&bad).is_err());
    }
    let mut metadata = data.clone();
    put16(&mut metadata, header + 176 + 64, 0x33);
    assert_eq!(select(&metadata).unwrap().lines().len(), 1);
    let mut metadata = data;
    put16(&mut metadata, header + 176, 0x33);
    put16(&mut metadata, header + 184, 2);
    assert!(select(&metadata).is_err()); // second slot is not a metadata FE
}

#[test]
fn name_resolution_profile_and_limits_remain_explicit() {
    for marker in [5u16, 8] {
        let mut data = fixture();
        put16(&mut data, 34, marker);
        assert!(select(&data).is_err());
    }
    assert!(Block3DProfile::from_id("marker25_direct_line3d_v1").is_ok());
    assert!(Block3DProfile::from_id("auto").is_err());
    for value in [
        b"MISS\0".as_slice(),
        b"\0",
        b"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    ] {
        let mut data = fixture();
        data[8224..8224 + value.len()].copy_from_slice(value);
        assert!(select(&data).is_err());
    }
    let mut data = fixture();
    let first = u32::from_le_bytes(data[76..80].try_into().unwrap()) as usize;
    data[first..first + 4].copy_from_slice(b"TWO\0");
    assert!(select(&data).is_err());
    let data = fixture();
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    for limit in [0, 1] {
        assert!(matches!(
            doc.placed_block_lines_3d(0, PROFILE, limit),
            Err(DracadError::LimitExceeded {
                resource: "block_lines_3d",
                ..
            })
        ));
    }
    let doc = scan_contents(
        &data,
        &ParseLimits {
            max_records: 6,
            ..ParseLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        doc.placed_block_lines_3d(0, PROFILE, 4096),
        Err(DracadError::LimitExceeded {
            resource: "records",
            ..
        })
    ));
}

#[test]
fn nonfinite_inputs_fields_and_unrepresentable_math_fail_without_losing_raw_bytes() {
    let data = fixture();
    let header = definition(&data);
    for offset in (8272..8320)
        .step_by(8)
        .chain((8336..8360).step_by(8))
        .chain((header + 112..header + 160).step_by(8))
        .chain((header + 192..header + 240).step_by(8))
    {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut bad = data.clone();
            put64(&mut bad, offset, value);
            assert!(matches!(select(&bad), Err(DracadError::InvalidFormat(_))));
            assert_eq!(
                scan_contents(&bad, &ParseLimits::default())
                    .unwrap()
                    .contents(),
                bad
            );
        }
    }
    for offset in [8296, 8304, 8352] {
        for value in [0., -0., -1.] {
            let mut bad = data.clone();
            put64(&mut bad, offset, value);
            assert!(select(&bad).is_err());
        }
    }
    let selection = select(&data).unwrap();
    for point in [
        [f64::NAN, 0., 0.],
        [0., f64::INFINITY, 0.],
        [f64::MAX, 0., 0.],
        [0., 0., f64::from_bits(1)],
        [f64::from_bits(1), 0., 0.],
    ] {
        assert!(selection.reference().apply_xyz(point).is_err());
    }
    let mut bad = data;
    put64(&mut bad, 8296, f64::MAX);
    assert!(select(&bad).is_err());
    let mut bad = fixture();
    put64(&mut bad, 8336, f64::from_bits(1));
    assert!(select(&bad).is_err());
}
