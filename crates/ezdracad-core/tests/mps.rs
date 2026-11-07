//! Authored wrapper/corruption contracts. These are not native format evidence.
use std::io::{Cursor, Read, Write};

use ezdracad_core::{
    inspect, inspect_file, read, read_file, scan_file, scan_records, Basic2DProfile, DracadError,
    DracadFormat, Entity, ParseLimits, ReadOptions, ReadPolicy, ReadScope, ReadStatus,
    DRACAD_CLSID,
};
use flate2::{write::ZlibEncoder, Compression};
use sha2::{Digest, Sha256};

const MPS: &[u8] = include_bytes!("../../../tests/fixtures/mps-document25.mps");
const MIXED: &[u8] = include_bytes!("../../../tests/fixtures/mps-basic2d25.mps");
const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/document25.bin");
const MPZ: &[u8] = include_bytes!("../../../tests/fixtures/document25.cfb");
const EXTENSION: &[u8] = &[
    0, 0, 0, 0, 0x18, 0xfc, 0xff, 0xff, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x24, 0x40, 1, 0, 0, 0,
];

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
fn wrap_stream_named(stream: &[u8], name: &str) -> Vec<u8> {
    let mut file =
        cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new())).unwrap();
    file.set_storage_clsid("/", uuid::Uuid::parse_str(DRACAD_CLSID).unwrap())
        .unwrap();
    if name.starts_with("/Nested/") {
        file.create_storage("/Nested").unwrap();
    }
    file.create_stream(name).unwrap().write_all(stream).unwrap();
    file.into_inner().into_inner()
}
fn wrap_stream(stream: &[u8]) -> Vec<u8> {
    wrap_stream_named(stream, "/SecureContents")
}
fn wrapper(contents: &[u8]) -> Vec<u8> {
    let mut output = vec![0; 122];
    for at in [0, 4, 8] {
        put(&mut output, at, 1);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(contents).unwrap();
    let encoded = encoder.finish().unwrap();
    output.extend_from_slice(&(contents.len() as u32).to_le_bytes());
    output.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    output.extend_from_slice(&encoded);
    output
}
fn wrapped(contents: &[u8]) -> Vec<u8> {
    wrap_stream(&wrapper(contents))
}
fn invalid(bytes: &[u8]) {
    assert!(matches!(
        inspect(bytes, &ParseLimits::default()),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn decoded_payload_identity_and_source_domains_are_not_compressed_stream_offsets() {
    let info = inspect(MPS, &ParseLimits::default()).unwrap();
    assert_eq!(info.format, DracadFormat::Mps);
    assert_eq!(info.file_size, MPS.len() as u64);
    assert!(info.cfb.is_some() && info.mps.is_some());
    assert_eq!(info.contents().unwrap(), CONTENTS);
    let payload = info.payload.as_ref().unwrap();
    assert_eq!(payload.stream_path, None);
    assert_eq!(payload.offset_domain(), "decoded_payload");
    assert_eq!(payload.size, CONTENTS.len() as u64);
    assert_eq!(payload.sha256, format!("{:x}", Sha256::digest(CONTENTS)));
    assert_eq!(payload.marker_raw, 25);
    let mps = info.mps.as_ref().unwrap();
    let mut carrier = cfb::CompoundFile::open(Cursor::new(MPS)).unwrap();
    let mut secure = Vec::new();
    carrier
        .open_stream("/SecureContents")
        .unwrap()
        .read_to_end(&mut secure)
        .unwrap();
    assert_eq!(mps.stream_path, "/SecureContents");
    assert_eq!(mps.stream_size, secure.len() as u64);
    assert_eq!(mps.stream_sha256, format!("{:x}", Sha256::digest(&secure)));
    assert_ne!(mps.stream_sha256, payload.sha256);
    assert_ne!(mps.stream_sha256, format!("{:x}", Sha256::digest(MPS)));
    assert_eq!(mps.wrapper_profile, "unprotected_base_v1");
    assert_eq!(mps.compression.as_str(), "zlib");
    assert_eq!(mps.decoded_size, CONTENTS.len() as u64);
    assert_eq!(mps.chunks.len(), 1);
    assert_eq!(mps.chunks[0].encoded_offset, 130);
    assert_eq!(mps.chunks[0].encoded_size, secure.len() as u64 - 130);
    assert_eq!(mps.chunks[0].decoded_offset, 0);
    assert_eq!(mps.chunks[0].decoded_size, CONTENTS.len() as u64);
    let raw = scan_records(MPS, &ParseLimits::default()).unwrap();
    let uncompressed = scan_records(MPZ, &ParseLimits::default()).unwrap();
    assert_eq!(raw.contents(), CONTENTS);
    assert_eq!(raw.sha256(), uncompressed.sha256());
    assert_eq!(raw.header(), uncompressed.header());
    for (decoded, plain) in raw.records().zip(uncompressed.records()) {
        assert_eq!(decoded.payload_offset, plain.payload_offset);
        assert_eq!(decoded.bytes(), plain.bytes());
    }
}

#[test]
fn strict_drawing_and_explicit_stored_scope_keep_existing_semantic_boundaries() {
    let err = read(MPS, &ReadOptions::default()).unwrap_err();
    assert!(matches!(err.error(), DracadError::UnsupportedFeature(_)));
    let report = err.report().unwrap();
    assert_eq!(report.source_format, DracadFormat::Mps);
    assert_eq!(report.offset_domain(), "decoded_payload");
    assert_eq!(report.stream_path, None);
    assert_eq!(report.status, ReadStatus::Partial);
    assert_eq!(report.entity_count, 2);
    assert!(!report.document_complete());
    assert_eq!(report.units(), None);
    assert_eq!(report.unresolved_semantics().len(), 8);
    let doc = read(MPS, &narrow()).unwrap();
    assert_eq!(doc.report().status, ReadStatus::Complete);
    assert_eq!(doc.report().payload_sha256(), doc.raw().sha256());
    for (index, entity) in doc.entities().iter().enumerate() {
        let source = entity.source_ref();
        assert_eq!(source.offset_domain(), "decoded_payload");
        assert_eq!(source.stream_path, None);
        assert_eq!(source.payload_offset, 8192 + index as u64 * 64);
        let at = source.payload_offset as usize;
        assert_eq!(
            entity
                .as_line()
                .unwrap()
                .geometry
                .scene()
                .source
                .header_bytes,
            CONTENTS[at..at + 16]
        );
    }
    for region in &doc.report().opaque_regions {
        assert_eq!(region.offset_domain(), "decoded_payload");
        assert_eq!(region.stream_path, None);
        assert!(region.payload_offset + region.byte_length <= CONTENTS.len() as u64);
    }
}

#[test]
fn source_ownership_survives_replacement_deletion_and_non_mps_suffix() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ezdracad-p7-mps-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("圧縮図面.payload");
    std::fs::write(&path, MPS).unwrap();
    let info = inspect_file(&path, &ParseLimits::default()).unwrap();
    let raw = scan_file(&path, &ParseLimits::default()).unwrap();
    let doc = read_file(&path, &narrow()).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(info.contents().unwrap(), CONTENTS);
    assert_eq!(raw.contents(), CONTENTS);
    assert_eq!(doc.raw().contents(), CONTENTS);
    assert_eq!(
        doc.query("LINE", 0, 1).unwrap()[0]
            .source_ref()
            .offset_domain(),
        "decoded_payload"
    );
    assert!(matches!(
        read_file(&path, &narrow()).unwrap_err().error(),
        DracadError::Io(_)
    ));
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn decompressed_budget_is_independent_and_checked_on_declared_size() {
    assert_eq!(
        ParseLimits::default().max_decompressed_size,
        128 * 1024 * 1024
    );
    for limit in [0, 1, CONTENTS.len() as u64 - 1] {
        let limits = ParseLimits {
            max_decompressed_size: limit,
            ..ParseLimits::default()
        };
        assert!(
            matches!(inspect(MPS, &limits), Err(DracadError::LimitExceeded {
            resource: "decompressed_size", actual, limit: bound,
        }) if actual == CONTENTS.len() as u64 && bound == limit)
        );
    }
    let exact = ParseLimits {
        max_decompressed_size: CONTENTS.len() as u64,
        ..ParseLimits::default()
    };
    assert!(inspect(MPS, &exact).is_ok());
    assert!(read(
        MPS,
        &ReadOptions {
            parse_limits: exact,
            ..narrow()
        }
    )
    .is_ok());
    // A decompression-only limit cannot restrict uncompressed MPZ input.
    assert!(inspect(
        MPZ,
        &ParseLimits {
            max_decompressed_size: 0,
            ..exact
        }
    )
    .is_ok());
    let mut huge = wrapper(CONTENTS);
    put(&mut huge, 122, u32::MAX);
    assert!(matches!(
        inspect(&wrap_stream(&huge), &ParseLimits::default()),
        Err(DracadError::LimitExceeded { .. })
    ));
}

#[test]
fn falsely_small_output_declaration_never_lets_actual_output_escape_the_budget() {
    let mut stream = wrapper(CONTENTS);
    put(&mut stream, 122, 1);
    let result = inspect(
        &wrap_stream(&stream),
        &ParseLimits {
            max_decompressed_size: 1024,
            ..ParseLimits::default()
        },
    );
    assert!(matches!(
        result,
        Err(DracadError::InvalidFormat(_)) | Err(DracadError::LimitExceeded { .. })
    ));
    let mut mismatch = wrapper(CONTENTS);
    put(&mut mismatch, 122, CONTENTS.len() as u32 - 1);
    invalid(&wrap_stream(&mismatch));
    put(&mut mismatch, 122, CONTENTS.len() as u32 + 1);
    invalid(&wrap_stream(&mismatch));
}

#[test]
fn all_cfb_and_input_budgets_still_apply_to_the_original_carrier() {
    let defaults = ParseLimits::default();
    for limits in [
        ParseLimits {
            max_file_size: MPS.len() as u64 - 1,
            ..defaults
        },
        ParseLimits {
            max_stream_size: 1,
            ..defaults
        },
        ParseLimits {
            max_total_stream_size: 1,
            ..defaults
        },
        ParseLimits {
            max_cfb_entries: 0,
            ..defaults
        },
        ParseLimits {
            max_cfb_metadata_size: 0,
            ..defaults
        },
        ParseLimits {
            max_cfb_directory_slots: 0,
            ..defaults
        },
        ParseLimits {
            max_cfb_depth: 0,
            ..defaults
        },
    ] {
        assert!(matches!(
            inspect(MPS, &limits),
            Err(DracadError::LimitExceeded { .. })
        ));
    }
    let error = scan_records(
        MPS,
        &ParseLimits {
            max_records: 2,
            ..defaults
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        DracadError::LimitExceeded {
            resource: "records",
            ..
        }
    ));
}

#[test]
fn checksums_truncation_and_declared_member_lengths_are_exact() {
    let stream = wrapper(CONTENTS);
    for length in [122, 123, 126, 129, 130, stream.len() - 1] {
        invalid(&wrap_stream(&stream[..length]));
    }
    for delta in [-1i64, 1] {
        let mut changed = stream.clone();
        put(
            &mut changed,
            126,
            ((stream.len() - 130) as i64 + delta) as u32,
        );
        invalid(&wrap_stream(&changed));
    }
    let mut checksum = stream.clone();
    *checksum.last_mut().unwrap() ^= 1;
    invalid(&wrap_stream(&checksum));
    let mut encoded = stream;
    encoded[132] ^= 0xff;
    invalid(&wrap_stream(&encoded));
}

#[test]
fn unused_bytes_and_concatenated_members_do_not_become_successful_drawings() {
    for suffix in [
        b"trailing bytes".to_vec(),
        wrapper(CONTENTS)[130..].to_vec(),
    ] {
        let mut stream = wrapper(CONTENTS);
        stream.extend_from_slice(&suffix);
        let count = stream.len() as u32 - 130;
        put(&mut stream, 126, count);
        invalid(&wrap_stream(&stream));
    }
}

#[test]
fn dictionary_required_or_non_zlib_bodies_are_not_decoded_as_plain_or_raw_deflate() {
    let mut required_dictionary = vec![0x78, 0x20, 0, 0, 0, 1];
    required_dictionary.extend_from_slice(&wrapper(CONTENTS)[132..]);
    let mut stream = wrapper(CONTENTS);
    stream.truncate(130);
    put(&mut stream, 126, required_dictionary.len() as u32);
    stream.extend_from_slice(&required_dictionary);
    invalid(&wrap_stream(&stream));
    for replacement in [CONTENTS.to_vec(), wrapper(CONTENTS)[132..].to_vec()] {
        let mut stream = wrapper(CONTENTS);
        stream.truncate(130);
        put(&mut stream, 126, replacement.len() as u32);
        stream.extend_from_slice(&replacement);
        invalid(&wrap_stream(&stream));
    }
}

#[test]
fn unknown_wrapper_security_and_extension_fields_fail_closed_without_header_search() {
    for at in [0, 4, 8, 12, 14, 16, 18, 20, 40, 80, 120, 121] {
        let mut stream = wrapper(CONTENTS);
        stream[at] = 2;
        let bytes = wrap_stream(&stream);
        let info = inspect(&bytes, &ParseLimits::default()).unwrap();
        assert_eq!(info.format, DracadFormat::MpsCandidate, "+{at}");
        assert!(info.payload.is_none());
        assert!(info.contents().is_err());
        assert!(matches!(
            scan_records(&bytes, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_))
        ));
        assert!(read(&bytes, &partial()).unwrap_err().report().is_none());
    }
    let mut shifted = b"opaque-prefix".to_vec();
    shifted.extend_from_slice(&wrapper(CONTENTS));
    let info = inspect(&wrap_stream(&shifted), &ParseLimits::default()).unwrap();
    assert_eq!(info.format, DracadFormat::MpsCandidate);
    assert!(info.payload.is_none());
}

#[test]
fn observed_extension_profile_does_not_turn_unknown_extension_values_into_supported_data() {
    let extended = include_bytes!("../../../tests/fixtures/mps-extended25.mps");
    let extended_info = inspect(extended, &ParseLimits::default()).unwrap();
    assert_eq!(extended_info.contents().unwrap(), CONTENTS);
    assert_eq!(
        extended_info.mps.as_ref().unwrap().wrapper_profile,
        "unprotected_extended_v1"
    );
    assert_eq!(
        extended_info.mps.as_ref().unwrap().chunks[0].encoded_offset,
        154
    );
    let mut stream = wrapper(CONTENTS);
    stream[18] = 1;
    stream.splice(122..122, EXTENSION.iter().copied());
    let bytes = wrap_stream(&stream);
    assert_eq!(
        inspect(&bytes, &ParseLimits::default())
            .unwrap()
            .contents()
            .unwrap(),
        CONTENTS
    );
    stream[125] ^= 1;
    let unknown = inspect(&wrap_stream(&stream), &ParseLimits::default()).unwrap();
    assert_eq!(unknown.format, DracadFormat::MpsCandidate);
    assert!(unknown.payload.is_none());
}

#[test]
fn root_stream_identity_and_ambiguous_sources_are_checked_before_decoding() {
    let stream = wrapper(CONTENTS);
    assert_eq!(
        inspect(
            &wrap_stream_named(&stream, "/securecontents"),
            &ParseLimits::default()
        )
        .unwrap()
        .format,
        DracadFormat::Mps
    );
    assert_eq!(
        inspect(
            &wrap_stream_named(&stream, "/Nested/SecureContents"),
            &ParseLimits::default()
        )
        .unwrap()
        .format,
        DracadFormat::UnknownCfb
    );
    let mut file = cfb::CompoundFile::open(Cursor::new(wrap_stream(&stream))).unwrap();
    file.create_stream("/Contents")
        .unwrap()
        .write_all(CONTENTS)
        .unwrap();
    invalid(&file.into_inner().into_inner());
    invalid(&MPS[..MPS.len() - 1]);
}

#[test]
fn valid_compression_does_not_qualify_unsupported_payload_markers_or_body_corruption() {
    for marker in [10u16, 22, 99] {
        let mut contents = CONTENTS.to_vec();
        contents[34..36].copy_from_slice(&marker.to_le_bytes());
        let bytes = wrapped(&contents);
        let info = inspect(&bytes, &ParseLimits::default()).unwrap();
        assert_eq!(info.payload.as_ref().unwrap().marker_raw, marker);
        assert_eq!(info.contents().unwrap(), contents);
        assert!(matches!(
            scan_records(&bytes, &ParseLimits::default()),
            Err(DracadError::UnsupportedFormat(_))
        ));
    }
    for at in [0, 18, 31] {
        let mut contents = CONTENTS.to_vec();
        contents[at] ^= 1;
        invalid(&wrapped(&contents));
    }
    let mut contents = CONTENTS.to_vec();
    contents[8192 + 128] = 2; // Recompressed valid zlib, invalid terminal record.
    let bytes = wrapped(&contents);
    assert!(inspect(&bytes, &ParseLimits::default()).is_ok());
    assert!(matches!(
        scan_records(&bytes, &ParseLimits::default()),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn unknown_roots_fence_following_geometry_and_keep_decoded_diagnostic_ranges() {
    let mut contents = CONTENTS.to_vec();
    contents[8192..8194].copy_from_slice(&0x7331u16.to_le_bytes());
    let bytes = wrapped(&contents);
    let doc = read(&bytes, &partial()).unwrap();
    assert_eq!(doc.report().entity_count, 0);
    assert_eq!(doc.report().unsupported_counts.get(&0x7331), Some(&1));
    assert_eq!(doc.report().context_blocked_counts.get(&2), Some(&1));
    for diagnostic in &doc.report().diagnostics {
        if let Some(source) = &diagnostic.source {
            assert_eq!(source.offset_domain(), "decoded_payload");
            assert_eq!(source.stream_path, None);
            assert!(source.payload_offset + source.byte_length <= contents.len() as u64);
        }
    }
    let error = read(&bytes, &narrow()).unwrap_err();
    assert_eq!(
        error.report().unwrap().unsupported_counts,
        doc.report().unsupported_counts
    );
    // Later structural corruption remains fatal even behind an unknown root.
    contents[8192 + 64 + 8..8192 + 64 + 10].fill(0);
    let error = read(&wrapped(&contents), &partial()).unwrap_err();
    assert!(matches!(error.error(), DracadError::InvalidFormat(_)));
    assert!(error.report().is_none());
}

#[test]
fn mixed_and_definition_sources_refer_to_complete_decoded_original_bodies() {
    let options = ReadOptions {
        policy: ReadPolicy::Partial,
        scope: ReadScope::StoredXyPrimitives,
        basic_profile: Some(Basic2DProfile::Marker25RootPrimitivesV1),
        ..ReadOptions::default()
    };
    let doc = read(MIXED, &options).unwrap();
    for entity in doc.entities() {
        let source = entity.source_ref();
        assert_eq!(source.offset_domain(), "decoded_payload");
        let at = source.payload_offset as usize;
        let body = &doc.raw().contents()[at..at + source.byte_length as usize];
        match entity {
            Entity::Line(e) => assert_eq!(e.geometry.scene().source.header_bytes, body[..16]),
            Entity::Circle(e) => assert_eq!(e.geometry.raw_bytes, body),
            Entity::Arc(e) => assert_eq!(e.geometry.raw_bytes, body),
            Entity::Text(e) => assert_eq!(e.geometry.raw_bytes, body),
        }
    }
    let block = include_bytes!("../../../tests/fixtures/mps-block25.mps");
    let doc = read(block, &narrow()).unwrap();
    assert_eq!(doc.report().entity_count, 1);
    let line = doc.query("LINE", 0, 1).unwrap()[0].as_line().unwrap();
    assert_eq!(line.geometry.scene().start, [105.0, -47.5]);
    assert_eq!(line.geometry.scene().end, [83.75, -40.5]);
    assert_eq!(line.source_ref.definition_index, Some(0));
    assert_eq!(line.source_ref.offset_domain(), "decoded_payload");
    assert_eq!(
        line.source_ref.table_domain.as_str(),
        "block_definition_records"
    );
    let at = line.source_ref.payload_offset as usize;
    assert_eq!(
        line.geometry.scene().source.header_bytes,
        doc.raw().contents()[at..at + 16]
    );
}

#[test]
fn regular_fat_compressed_streams_do_not_depend_on_minifat_layout() {
    let mut contents = CONTENTS.to_vec();
    let mut state = 0x8c93_719du32;
    for _ in 0..12_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        contents.push(state as u8);
    }
    let end = contents.len() as u32 - 4;
    put(&mut contents, 0x50, end);
    let stream = wrapper(&contents);
    assert!(stream.len() > 4096);
    let info = inspect(&wrap_stream(&stream), &ParseLimits::default()).unwrap();
    assert_eq!(info.contents().unwrap(), contents);
    assert_eq!(
        info.payload.as_ref().unwrap().offset_domain(),
        "decoded_payload"
    );
}

#[test]
fn decompression_preserves_nonplanar_and_opaque_curve_tail_holds_across_carriers() {
    let mut line_contents = CONTENTS.to_vec();
    line_contents[8192 + 48..8192 + 56].copy_from_slice(&12.5f64.to_le_bytes());
    for contents in [line_contents, {
        let mut bytes = include_bytes!("../../../tests/fixtures/basic2d25.bin").to_vec();
        // Locate the first ordinary CIRCLE by the already validated root spans.
        let source = scan_records(MIXED, &ParseLimits::default()).unwrap();
        let at = source
            .object_spans()
            .map(Result::unwrap)
            .find(|span| span.tag_raw == 3)
            .unwrap()
            .payload_offset as usize;
        bytes[at + 12..at + 16].copy_from_slice(&[0, 3, 0, 0]);
        bytes
    }] {
        let mps = wrapped(&contents);
        let mpz = wrap_stream_named(&contents, "/Contents");
        let options = ReadOptions {
            policy: ReadPolicy::Partial,
            scope: ReadScope::StoredXyPrimitives,
            basic_profile: Some(Basic2DProfile::Marker25RootPrimitivesV1),
            ..ReadOptions::default()
        };
        let decoded = read(&mps, &options).unwrap();
        let plain = read(&mpz, &options).unwrap();
        assert_eq!(decoded.report().entity_count, plain.report().entity_count);
        assert_eq!(
            decoded.report().unsupported_counts,
            plain.report().unsupported_counts
        );
        assert_eq!(
            decoded.report().context_blocked_counts,
            plain.report().context_blocked_counts
        );
        assert_eq!(decoded.report().status, ReadStatus::Partial);
        assert_eq!(decoded.raw().contents(), plain.raw().contents());
        assert!(!decoded.report().unsupported_counts.is_empty());
        assert_eq!(decoded.report().offset_domain(), "decoded_payload");
        assert_eq!(plain.report().offset_domain(), "cfb_stream");
    }
}
