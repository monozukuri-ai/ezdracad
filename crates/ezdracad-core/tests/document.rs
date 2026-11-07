use ezdracad_core::{read, read_file, DracadError, ReadOptions, ReadPolicy, ReadScope, ReadStatus};
use std::io::{Cursor, Write};

const MPZ: &[u8] = include_bytes!("../../../tests/fixtures/document25.cfb");
const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/document25.bin");
const PARTIAL: &[u8] = include_bytes!("../../../tests/fixtures/document-partial25.cfb");
const PARTIAL_CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/document-partial25.bin");
fn options() -> ReadOptions {
    ReadOptions {
        policy: ReadPolicy::Partial,
        ..ReadOptions::default()
    }
}
fn wrap(contents: &[u8]) -> Vec<u8> {
    let mut compound = cfb::CompoundFile::open(Cursor::new(MPZ.to_vec())).unwrap();
    {
        let mut stream = compound.open_stream("/Contents").unwrap();
        stream.set_len(contents.len() as u64).unwrap();
        stream.write_all(contents).unwrap();
    }
    compound.into_inner().into_inner()
}
#[test]
fn strict_drawing_error_retains_structured_report_and_same_typed_cause() {
    let error = read(MPZ, &ReadOptions::default()).unwrap_err();
    assert!(matches!(error.error(), DracadError::UnsupportedFeature(_)));
    let report = error.report().unwrap();
    assert_eq!(report.status, ReadStatus::Partial);
    assert_eq!(report.geometry_status, ReadStatus::Complete);
    assert_eq!(report.entity_count, 2);
    assert!(!report.document_complete());
    assert_eq!(report.units(), None);
    assert_eq!(report.unresolved_semantics().len(), 8);
}
#[test]
fn partial_and_explicit_stored_scope_preserve_geometry_sources_and_duplicates() {
    let doc = read(MPZ, &options()).unwrap();
    assert_eq!(doc.report().status, ReadStatus::Partial);
    assert_eq!(doc.raw().contents(), CONTENTS);
    assert_eq!(doc.query_count("LINE").unwrap(), 2);
    let selected = doc.query("LINE", 0, 4096).unwrap();
    let lines: Vec<_> = selected.iter().map(|e| e.as_line().unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].geometry.scene().start, [-12.5, 0.25]);
    assert_eq!(lines[1].geometry.scene().end, [17.125, 8.5]);
    for (index, line) in lines.iter().enumerate() {
        assert_eq!(line.entity_id, index as u64);
        assert_eq!(line.source_ref.payload_offset, 8192 + 64 * index as u64);
        assert_eq!(line.source_ref.offset_domain(), "cfb_stream");
        assert_eq!(
            line.geometry.scene().source.header_bytes,
            &CONTENTS[8192 + 64 * index..8208 + 64 * index]
        );
    }
    let doc = read(
        MPZ,
        &ReadOptions {
            scope: ReadScope::StoredXyLines,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(doc.report().status, ReadStatus::Complete);
    assert_eq!(doc.report().document_status, ReadStatus::Partial);
    assert!(!doc.report().document_complete());
    assert!(matches!(
        doc.query("CIRCLE", 0, 5),
        Err(DracadError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        doc.query("LINE", 0, 4097),
        Err(DracadError::LimitExceeded {
            resource: "entities_per_page",
            ..
        })
    ));
}
#[test]
fn unknown_primary_is_atomic_and_fences_following_state() {
    let doc = read(PARTIAL, &options()).unwrap();
    let r = doc.report();
    assert_eq!(r.primary_object_count, 3);
    assert_eq!(r.decoded_object_count, 1);
    assert_eq!(r.entity_count, 1);
    assert_eq!(r.unsupported_counts.get(&0x7331), Some(&1));
    assert_eq!(r.context_blocked_counts.get(&2), Some(&1));
    assert_eq!(doc.raw().contents(), PARTIAL_CONTENTS);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "unsupported_object" && d.source.as_ref().unwrap().record_index == 1));
    assert!(read(
        PARTIAL,
        &ReadOptions {
            scope: ReadScope::StoredXyLines,
            ..ReadOptions::default()
        }
    )
    .is_err());
}
#[test]
fn later_corruption_and_nonfinite_fields_are_fatal_even_after_unknown_state() {
    for (at, bytes) in [
        (8192 + 128 + 8, 0u16.to_le_bytes().to_vec()),
        (8192 + 128 + 48, f64::NAN.to_le_bytes().to_vec()),
        (8192 + 128, 0xfeu16.to_le_bytes().to_vec()),
    ] {
        let mut c = PARTIAL_CONTENTS.to_vec();
        c[at..at + bytes.len()].copy_from_slice(&bytes);
        let err = read(&wrap(&c), &options()).unwrap_err();
        assert!(matches!(err.error(), DracadError::InvalidFormat(_)));
        assert!(err.report().is_none());
    }
}
#[test]
fn aggregate_limits_never_degrade_into_partial_success() {
    for (key, value) in [
        ("objects", 1),
        ("entities", 1),
        ("nodes", 1),
        ("provenance", 0),
    ] {
        let mut o = options();
        match key {
            "objects" => o.read_limits.max_objects = value,
            "entities" => o.read_limits.max_entities = value,
            "nodes" => o.read_limits.max_nodes = value,
            _ => o.read_limits.max_provenance_bytes = value,
        }
        let err = read(MPZ, &o).unwrap_err();
        assert!(matches!(err.error(), DracadError::LimitExceeded { .. }));
        assert!(err.report().is_none());
    }
    let mut o = options();
    o.read_limits.max_diagnostics = 0;
    let doc = read(PARTIAL, &o).unwrap();
    assert!(doc.report().diagnostics.is_empty());
    assert!(doc.report().diagnostics_suppressed >= 10);
    assert_eq!(doc.report().unsupported_counts[&0x7331], 1);
}
#[test]
fn old_or_unknown_profiles_never_fall_back_to_line2d() {
    {
        let marker = 8u16;
        let mut c = CONTENTS.to_vec();
        c[34..36].copy_from_slice(&marker.to_le_bytes());
        let doc = read(&wrap(&c), &options()).unwrap();
        assert!(doc.entities().is_empty());
        assert_eq!(doc.report().status, ReadStatus::Partial);
        assert!(doc
            .report()
            .diagnostics
            .iter()
            .any(|d| d.code == "unsupported_decoder_context"));
    }
    let mut c = CONTENTS.to_vec();
    c[34..36].copy_from_slice(&5u16.to_le_bytes());
    assert!(matches!(
        read(&wrap(&c), &options()).unwrap_err().error(),
        DracadError::UnsupportedFeature(_)
    ));
    c[34..36].copy_from_slice(&99u16.to_le_bytes());
    assert!(matches!(
        read(&wrap(&c), &options()).unwrap_err().error(),
        DracadError::UnsupportedFormat(_)
    ));
}
#[test]
fn source_io_is_fatal_and_not_reclassified_as_unsupported() {
    let e = read_file(
        std::path::Path::new("/definitely-missing-ezdracad-document.mpz"),
        &options(),
    )
    .unwrap_err();
    assert!(matches!(e.error(), DracadError::Io(_)));
    assert!(e.report().is_none());
}
