#![no_main]
//! Bounded compressed-carrier and decoded-source contracts. Synthetic seeds only.
use ezdracad_core::{
    inspect, read, scan_records, Basic2DProfile, DracadFormat, Entity, ParseLimits, ReadLimits,
    ReadOptions, ReadPolicy, ReadScope, ReadStatus, SceneLimits, TextLimits,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = ParseLimits {
        max_file_size: 262144,
        max_stream_size: 262144,
        max_total_stream_size: 262144,
        max_cfb_metadata_size: 65536,
        max_cfb_directory_slots: 128,
        max_cfb_entries: 32,
        max_cfb_depth: 8,
        max_records: 2048,
        max_decompressed_size: 1048576,
    };
    let inspected = inspect(data, &limits);
    if let Ok(info) = &inspected {
        if info.format != DracadFormat::Mps {
            // Other carriers and unsupported MPS wrappers have their own targets.
            if info.format == DracadFormat::MpsCandidate {
                assert!(info.payload.is_none());
                assert!(info.contents().is_err());
            }
            return;
        }
        assert!(info.cfb.is_some());
        let bytes = info.contents().unwrap();
        assert!(bytes.len() as u64 <= limits.max_decompressed_size);
        let payload = info.payload.as_ref().unwrap();
        assert_eq!(payload.stream_path, None);
        assert_eq!(payload.offset_domain(), "decoded_payload");
        assert_eq!(payload.size, bytes.len() as u64);
        let mps = info.mps.as_ref().unwrap();
        assert_eq!(mps.decoded_size, bytes.len() as u64);
        let mut decoded_end = 0;
        for chunk in &mps.chunks {
            assert_eq!(chunk.decoded_offset, decoded_end);
            decoded_end = chunk
                .decoded_offset
                .checked_add(chunk.decoded_size)
                .unwrap();
            assert!(decoded_end <= bytes.len() as u64);
            assert!(
                chunk
                    .encoded_offset
                    .checked_add(chunk.encoded_size)
                    .unwrap()
                    <= mps.stream_size
            );
        }
        assert_eq!(decoded_end, bytes.len() as u64);
    }
    if let Ok(raw) = scan_records(data, &limits) {
        let carrier = raw.container().unwrap();
        assert_eq!(carrier.format, DracadFormat::Mps);
        assert_eq!(
            raw.contents(),
            inspected.as_ref().unwrap().contents().unwrap()
        );
        assert_eq!(raw.records().count() as u64, raw.header().record_count);
        assert_eq!(
            raw.tag_counts().values().sum::<u64>(),
            raw.header().record_count
        );
        assert_eq!(raw.header().payload_end + 4, raw.contents().len() as u64);
        assert_eq!(raw.header().payload_end_basis.as_str(), "declared_field");
    }
    let options = ReadOptions {
        policy: ReadPolicy::Partial,
        scope: ReadScope::StoredXyPrimitives,
        basic_profile: Some(Basic2DProfile::Marker25RootPrimitivesV1),
        parse_limits: limits,
        scene_limits: SceneLimits {
            max_depth: 8,
            max_nodes: 32,
            max_lines: 32,
        },
        text_limits: TextLimits {
            max_encoded_bytes: 1024,
            max_output_bytes: 4096,
        },
        read_limits: ReadLimits {
            max_objects: 64,
            max_entities: 32,
            max_nodes: 256,
            max_provenance_bytes: 131072,
            max_diagnostics: 16,
        },
        ..ReadOptions::default()
    };
    if let Ok(doc) = read(data, &options) {
        let report = doc.report();
        let bytes = doc.raw().contents();
        assert_eq!(report.source_format, DracadFormat::Mps);
        assert_eq!(report.stream_path, None);
        assert_eq!(report.offset_domain(), "decoded_payload");
        assert_eq!(bytes, inspected.as_ref().unwrap().contents().unwrap());
        assert_eq!(report.payload_sha256(), doc.raw().sha256());
        assert_eq!(report.units(), None);
        assert!(!report.document_complete());
        assert_eq!(report.entity_count, doc.entities().len() as u64);
        assert!(report.entity_count <= 32 && report.scene_nodes_visited <= 256);
        assert!(report.diagnostics.len() <= 16);
        assert_eq!(
            report.primary_object_count,
            report.decoded_object_count
                + report.handled_auxiliary_object_count
                + report.unsupported_counts.values().sum::<u64>()
                + report.context_blocked_counts.values().sum::<u64>()
        );
        for (id, entity) in doc.entities().iter().enumerate() {
            assert_eq!(entity.entity_id(), id as u64);
            let source = entity.source_ref();
            assert_eq!(source.stream_path, None);
            assert_eq!(source.offset_domain(), "decoded_payload");
            let at = source.payload_offset as usize;
            let body = &bytes[at..at + source.byte_length as usize];
            let styles = match entity {
                Entity::Line(e) => {
                    assert_eq!(&body[..16], e.geometry.scene().source.header_bytes);
                    assert!(e
                        .geometry
                        .scene()
                        .start
                        .into_iter()
                        .chain(e.geometry.scene().end)
                        .all(f64::is_finite));
                    &e.geometry.scene().styles
                }
                Entity::Circle(e) => {
                    assert_eq!(body, e.geometry.raw_bytes);
                    assert!(e
                        .geometry
                        .center
                        .into_iter()
                        .chain([e.geometry.radius])
                        .all(f64::is_finite));
                    assert!(e.geometry.radius > 0.0);
                    &e.style.styles
                }
                Entity::Arc(e) => {
                    assert_eq!(body, e.geometry.raw_bytes);
                    assert!(e
                        .geometry
                        .center
                        .into_iter()
                        .chain([
                            e.geometry.radius,
                            e.geometry.start_angle_degrees,
                            e.geometry.end_angle_degrees,
                        ])
                        .all(f64::is_finite));
                    assert!(e.geometry.radius > 0.0);
                    &e.style.styles
                }
                Entity::Text(e) => {
                    assert_eq!(body, e.geometry.raw_bytes);
                    assert!(e
                        .geometry
                        .basepoint
                        .into_iter()
                        .chain([
                            e.geometry.width,
                            e.geometry.height,
                            e.geometry.rotation_degrees,
                        ])
                        .all(f64::is_finite));
                    assert!(e.geometry.body_bytes.len() <= 1024 && e.geometry.text.len() <= 4096);
                    &e.style.styles
                }
            };
            for property in [&styles.color, &styles.line_type, &styles.width] {
                for evidence in &property.sources {
                    let at = evidence.payload_offset as usize;
                    assert_eq!(
                        &bytes[at..at + evidence.raw_bytes.len()],
                        evidence.raw_bytes
                    );
                }
            }
        }
        for diagnostic in &report.diagnostics {
            if let Some(source) = &diagnostic.source {
                assert_eq!(source.stream_path, None);
                assert_eq!(source.offset_domain(), "decoded_payload");
                assert!(source.payload_offset + source.byte_length <= bytes.len() as u64);
            }
        }
        let strict = read(
            data,
            &ReadOptions {
                policy: ReadPolicy::Strict,
                ..options
            },
        );
        if report.status == ReadStatus::Complete {
            assert!(strict.is_ok());
        } else {
            let error = strict.unwrap_err();
            let other = error
                .report()
                .expect("strict cannot turn partial input into corruption");
            assert_eq!(other.entity_count, report.entity_count);
            assert_eq!(other.unsupported_counts, report.unsupported_counts);
            assert_eq!(other.offset_domain(), "decoded_payload");
        }
    }
});
