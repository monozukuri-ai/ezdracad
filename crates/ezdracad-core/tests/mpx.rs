//! Synthetic carrier/ownership/corruption contracts, never native MPX evidence.
use ezdracad_core::{
    inspect, inspect_cfb, inspect_file, read, read_file, scan_contents, scan_file, scan_records,
    Basic2DProfile, DracadError, DracadFormat, Entity, ParseLimits, ReadOptions, ReadPolicy,
    ReadScope, ReadStatus,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const MPX: &[u8] = include_bytes!("../../../tests/fixtures/mpx25.mpx");
const MIXED: &[u8] = include_bytes!("../../../tests/fixtures/mpx-mixed25.mpx");
const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/document25.bin");
const CFB: &[u8] = include_bytes!("../../../tests/fixtures/document25.cfb");
const START: usize = 8192;

fn narrow() -> ReadOptions {
    ReadOptions {
        scope: ReadScope::StoredXyLines,
        ..ReadOptions::default()
    }
}
fn partial() -> ReadOptions {
    ReadOptions {
        policy: ReadPolicy::Partial,
        ..narrow()
    }
}
fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn unique_directory() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("ezdracad-p6-mpx-{}-{stamp}", std::process::id()))
}

#[test]
fn flat_identification_retains_original_file_bytes_without_a_fabricated_cfb() {
    let info = inspect(MPX, &ParseLimits::default()).unwrap();
    assert_eq!(info.format, DracadFormat::Mpx);
    assert_eq!(info.file_size, MPX.len() as u64);
    assert!(info.cfb.is_none());
    assert_eq!(info.contents().unwrap(), MPX);
    let source = info.payload.as_ref().unwrap();
    assert_eq!(source.stream_path, None);
    assert_eq!(source.offset_domain(), "file");
    assert_eq!(source.size, MPX.len() as u64);
    assert_eq!(source.sha256, format!("{:x}", Sha256::digest(MPX)));
    assert_eq!(source.marker_raw, 25);
    assert!(info.diagnostics.iter().any(
        |d| d.code == "mpx_payload_retained" && d.message.contains("does not decode geometry")
    ));
    assert!(inspect_cfb(MPX, &ParseLimits::default()).is_err());
}

#[test]
fn carrier_end_rules_differ_without_altering_source_offsets_or_record_fields() {
    let flat = scan_records(MPX, &ParseLimits::default()).unwrap();
    let wrapped = scan_records(CFB, &ParseLimits::default()).unwrap();
    assert_eq!(flat.contents(), MPX);
    assert_eq!(wrapped.contents(), CONTENTS);
    assert_eq!(&MPX[..0x50], &CONTENTS[..0x50]);
    assert_eq!(&MPX[0x54..], &CONTENTS[0x54..CONTENTS.len() - 4]);
    assert_eq!(flat.header().payload_end, MPX.len() as u64);
    assert_eq!(flat.header().payload_end_field_raw, 0);
    assert_eq!(flat.header().payload_end_basis.as_str(), "file_length");
    assert_eq!(wrapped.header().payload_end, CONTENTS.len() as u64 - 4);
    assert_eq!(
        wrapped.header().payload_end_field_raw,
        (CONTENTS.len() - 4) as u32
    );
    assert_eq!(
        wrapped.header().payload_end_basis.as_str(),
        "declared_field"
    );
    assert_eq!(flat.header().record_count, wrapped.header().record_count);
    assert_eq!(
        flat.header().section_offsets,
        wrapped.header().section_offsets
    );
    assert_eq!(flat.tag_counts(), wrapped.tag_counts());
    for (a, b) in flat.records().zip(wrapped.records()) {
        assert_eq!(a.payload_offset, b.payload_offset);
        assert_eq!(a.bytes(), b.bytes());
    }
    assert_ne!(flat.sha256(), wrapped.sha256());
    assert!(scan_contents(MPX, &ParseLimits::default()).is_err());
    assert!(scan_contents(CONTENTS, &ParseLimits::default()).is_ok());
}

#[test]
fn extracted_contents_are_not_automatically_promoted_to_a_native_flat_carrier() {
    for data in [
        CONTENTS,
        include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
        include_bytes!("../../../tests/fixtures/basic2d25.bin").as_slice(),
    ] {
        assert!(matches!(
            inspect(data, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            scan_records(data, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_))
        ));
    }
    let directory = unique_directory();
    std::fs::create_dir(&directory).unwrap();
    let fake = directory.join("renamed.mpx");
    std::fs::write(&fake, CONTENTS).unwrap();
    assert!(inspect_file(&fake, &ParseLimits::default()).is_err());
    assert!(read_file(&fake, &partial()).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn drawing_policy_stays_partial_while_explicit_stored_scope_can_complete() {
    let error = read(MPX, &ReadOptions::default()).unwrap_err();
    assert!(matches!(error.error(), DracadError::UnsupportedFeature(_)));
    let report = error.report().unwrap();
    assert_eq!(report.source_format, DracadFormat::Mpx);
    assert_eq!(report.stream_path, None);
    assert_eq!(report.offset_domain(), "file");
    assert_eq!(report.status, ReadStatus::Partial);
    assert_eq!(report.geometry_status, ReadStatus::Complete);
    assert_eq!(report.entity_count, 2);
    assert!(!report.document_complete());
    assert_eq!(report.units(), None);
    assert_eq!(report.unresolved_semantics().len(), 8);
    let doc = read(MPX, &narrow()).unwrap();
    assert_eq!(doc.report().status, ReadStatus::Complete);
    assert_eq!(doc.report().document_status, ReadStatus::Partial);
    assert_eq!(doc.raw().contents(), MPX);
    assert_eq!(
        doc.report().contents_sha256,
        format!("{:x}", Sha256::digest(MPX))
    );
    for (id, entity) in doc.query("LINE", 0, 4096).unwrap().iter().enumerate() {
        let line = entity.as_line().unwrap();
        assert_eq!(line.entity_id, id as u64);
        let source = &line.source_ref;
        assert_eq!(source.stream_path, None);
        assert_eq!(source.offset_domain(), "file");
        assert_eq!(source.payload_offset, 8192 + id as u64 * 64);
        let at = source.payload_offset as usize;
        assert_eq!(line.geometry.scene().source.header_bytes, MPX[at..at + 16]);
    }
    assert_eq!(doc.query("LINE", 1, 1).unwrap()[0].entity_id(), 1);
    assert!(doc.query("LINE", 2, 256).unwrap().is_empty());
    assert!(doc.query("CIRCLE", 0, 1).is_err());
    for region in &doc.report().opaque_regions {
        assert_eq!(region.stream_path, None);
        assert_eq!(region.offset_domain(), "file");
        assert!(region.payload_offset + region.byte_length <= MPX.len() as u64);
    }
}

#[test]
fn file_suffix_is_irrelevant_and_owned_queries_survive_source_replacement_and_deletion() {
    let directory = unique_directory();
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("図面.payload");
    std::fs::write(&path, MPX).unwrap();
    let info = inspect_file(&path, &ParseLimits::default()).unwrap();
    let raw = scan_file(&path, &ParseLimits::default()).unwrap();
    let doc = read_file(&path, &narrow()).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(info.contents().unwrap(), MPX);
    assert_eq!(raw.contents(), MPX);
    assert_eq!(doc.raw().contents(), MPX);
    let first = doc.query("LINE", 0, 1).unwrap()[0].as_line().unwrap();
    assert_eq!(first.geometry.scene().start, [-12.5, 0.25]);
    assert_eq!(first.geometry.scene().end, [42.0, 0.25]);
    assert_eq!(first.source_ref.offset_domain(), "file");
    let error = read_file(&path, &narrow()).unwrap_err();
    assert!(matches!(error.error(), DracadError::Io(_)));
    assert!(error.report().is_none());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn file_and_record_budgets_apply_but_cfb_only_budgets_do_not_apply_to_flat_inputs() {
    let limits = ParseLimits {
        max_stream_size: 0,
        max_total_stream_size: 0,
        max_cfb_entries: 0,
        max_cfb_metadata_size: 0,
        max_cfb_directory_slots: 0,
        max_cfb_depth: 0,
        ..ParseLimits::default()
    };
    assert!(inspect(MPX, &limits).is_ok());
    assert!(scan_records(MPX, &limits).is_ok());
    assert!(read(
        MPX,
        &ReadOptions {
            parse_limits: limits,
            ..narrow()
        }
    )
    .is_ok());
    assert!(inspect(CFB, &limits).is_err());
    let limit = MPX.len() as u64 - 1;
    assert!(matches!(
        inspect(MPX, &ParseLimits { max_file_size: limit, ..limits }),
        Err(DracadError::LimitExceeded { resource: "file_size", actual, limit: bound })
            if actual==MPX.len() as u64 && bound==limit
    ));
    assert!(matches!(
        scan_records(
            MPX,
            &ParseLimits {
                max_records: 2,
                ..limits
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "records",
            ..
        })
    ));
    let mut opts = narrow();
    opts.read_limits.max_entities = 1;
    assert!(matches!(
        read(MPX, &opts).unwrap_err().error(),
        DracadError::LimitExceeded {
            resource: "document_entities",
            ..
        }
    ));
}

#[test]
fn unsupported_flat_profiles_flags_and_fake_container_prefixes_never_fall_back() {
    for marker in [5u16, 8, 24, 26, u16::MAX] {
        let mut bytes = MPX.to_vec();
        bytes[34..36].copy_from_slice(&marker.to_le_bytes());
        assert!(matches!(
            inspect(&bytes, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_))
        ));
    }
    let mut flags = MPX.to_vec();
    flags[32] = 1;
    assert!(inspect(&flags, &ParseLimits::default()).is_err());
    let mut fake_cfb = MPX.to_vec();
    fake_cfb[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    assert!(inspect(&fake_cfb, &ParseLimits::default()).is_err());
    let mut prefix = b"unrecognized-prefix".to_vec();
    prefix.extend_from_slice(MPX);
    assert!(inspect(&prefix, &ParseLimits::default()).is_err());
    let mut padding = MPX.to_vec();
    padding[24] = 1;
    assert!(inspect(&padding, &ParseLimits::default()).is_err());
}

#[test]
fn known_truncated_crossing_and_overflowing_bounds_are_controlled_errors() {
    let end = u32::from_le_bytes(MPX[0x40..0x44].try_into().unwrap()) as usize;
    // EOF is the carrier boundary. Unknown suffix bytes are not promised a
    // checksum, so only truncation through known required regions is asserted.
    for length in [0, 18, 35, 91, 8192, end - 1] {
        assert!(scan_records(&MPX[..length], &ParseLimits::default()).is_err());
    }
    for (at, value) in [
        (0x24, 0),
        (0x24, u32::MAX),
        (0x28, 8191),
        (0x28, u32::MAX),
        (0x58, 0),
        (0x58, u32::MAX),
        (0x2c, 0),
        (0x30, u32::MAX),
        (0x4c, 1),
    ] {
        let mut bytes = MPX.to_vec();
        put(&mut bytes, at, value);
        assert!(
            scan_records(&bytes, &ParseLimits::default()).is_err(),
            "+{at:#x}"
        );
    }
    let mut declared_end = MPX.to_vec();
    let n = declared_end.len() as u32;
    put(&mut declared_end, 0x50, n);
    assert!(matches!(
        scan_records(&declared_end, &ParseLimits::default()),
        Err(DracadError::UnsupportedFormat(_))
    ));
}

#[test]
fn terminal_numeric_and_later_structure_errors_cannot_become_successful_partial_reads() {
    let mut terminal = MPX.to_vec();
    terminal[START + 2 * 64] = 2;
    assert!(scan_records(&terminal, &ParseLimits::default()).is_err());
    let mut premature = MPX.to_vec();
    premature[START] = 0xff;
    assert!(scan_records(&premature, &ParseLimits::default()).is_err());
    let mut bad_xy = MPX.to_vec();
    bad_xy[START + 16..START + 24].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(scan_records(&bad_xy, &ParseLimits::default()).is_err());
    for (at, value) in [
        (START + 64 + 8, 0u16.to_le_bytes().to_vec()),
        (START + 64 + 48, f64::NAN.to_le_bytes().to_vec()),
        (START + 64, 0xfeu16.to_le_bytes().to_vec()),
    ] {
        let mut bytes = MPX.to_vec();
        bytes[START..START + 2].copy_from_slice(&0x7331u16.to_le_bytes());
        bytes[at..at + value.len()].copy_from_slice(&value);
        let error = read(&bytes, &partial()).unwrap_err();
        assert!(matches!(error.error(), DracadError::InvalidFormat(_)));
        assert!(error.report().is_none());
    }
}

#[test]
fn unsupported_roots_fence_the_following_flat_objects_with_real_file_source_ranges() {
    let mut bytes = MPX.to_vec();
    bytes[START..START + 2].copy_from_slice(&0x7331u16.to_le_bytes());
    let doc = read(&bytes, &partial()).unwrap();
    let report = doc.report();
    assert_eq!(report.entity_count, 0);
    assert_eq!(report.unsupported_counts.get(&0x7331), Some(&1));
    assert_eq!(report.context_blocked_counts.get(&2), Some(&1));
    assert_eq!(report.scene_nodes_visited, 1);
    for diagnostic in report.diagnostics.iter().filter(|d| d.source.is_some()) {
        let source = diagnostic.source.as_ref().unwrap();
        assert_eq!(source.offset_domain(), "file");
        assert_eq!(source.stream_path, None);
        assert!(source.payload_offset + source.byte_length <= bytes.len() as u64);
    }
    let strict = read(&bytes, &narrow()).unwrap_err();
    assert!(strict.report().is_some());
    assert_eq!(
        strict.report().unwrap().unsupported_counts,
        report.unsupported_counts
    );
}

#[test]
fn mixed_flat_records_retain_complete_unknown_bodies_and_report_type_scope() {
    let raw = scan_records(MIXED, &ParseLimits::default()).unwrap();
    assert_eq!(raw.contents(), MIXED);
    assert_eq!(raw.header().record_count, 26);
    let doc = read(
        MIXED,
        &ReadOptions {
            policy: ReadPolicy::Partial,
            scope: ReadScope::StoredXyPrimitives,
            basic_profile: Some(Basic2DProfile::Marker25RootPrimitivesV1),
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(doc.report().source_format, DracadFormat::Mpx);
    assert_eq!(doc.report().offset_domain(), "file");
    assert_eq!(
        doc.report().requested_entity_types(),
        ["LINE", "CIRCLE", "ARC", "TEXT"]
    );
    for entity in doc.entities() {
        assert_eq!(entity.source_ref().offset_domain(), "file");
        let at = entity.source_ref().payload_offset as usize;
        match entity {
            Entity::Line(line) => assert_eq!(
                line.geometry.scene().source.header_bytes,
                MIXED[at..at + 16]
            ),
            Entity::Circle(circle) => assert_eq!(circle.geometry.raw_bytes, MIXED[at..at + 128]),
            Entity::Arc(arc) => assert_eq!(arc.geometry.raw_bytes, MIXED[at..at + 128]),
            Entity::Text(text) => assert_eq!(
                text.geometry.raw_bytes,
                MIXED[at..at + text.geometry.raw_bytes.len()]
            ),
        }
    }
    assert_eq!(doc.raw().contents(), MIXED);
    // This test does not decide which unobserved native primitive tails qualify.
    // The native pair oracle establishes admission separately from this vector.
}

#[test]
fn block_headers_need_no_global_eof_alignment_and_keep_definition_file_provenance() {
    let bytes = include_bytes!("../../../tests/fixtures/mpx-block25.mpx");
    assert_eq!(bytes.len() % 64, 48);
    let raw = scan_records(bytes, &ParseLimits::default()).unwrap();
    let library = raw.block_library().unwrap();
    assert_eq!(library.definitions().len(), 1);
    assert_eq!(library.definitions()[0].name_bytes, b"P6UNIT");
    assert_eq!(library.definitions()[0].record_count, 2);
    let doc = read(bytes, &narrow()).unwrap();
    assert_eq!(doc.report().entity_count, 1);
    let line = doc.query("LINE", 0, 1).unwrap()[0].as_line().unwrap();
    assert_eq!(line.geometry.scene().start, [105.0, -47.5]);
    assert_eq!(line.geometry.scene().end, [83.75, -40.5]);
    assert_eq!(line.source_ref.stream_path, None);
    assert_eq!(line.source_ref.offset_domain(), "file");
    assert_eq!(line.source_ref.definition_index, Some(0));
    assert_eq!(
        line.source_ref.table_domain.as_str(),
        "block_definition_records"
    );
    let offset = line.source_ref.payload_offset as usize;
    assert_eq!(
        line.geometry.scene().source.header_bytes,
        bytes[offset..offset + 16]
    );
    assert_eq!(line.root_record_index, 0);
    assert_eq!(doc.raw().contents(), bytes);
}
