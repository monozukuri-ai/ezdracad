#![no_main]
//! Flat-carrier safety and source identity, with synthetic seeds only.
use ezdracad_core::{
    inspect, read, scan_records, Basic2DProfile, DracadFormat, Entity, ParseLimits, ReadLimits,
    ReadOptions, ReadPolicy, ReadScope, ReadStatus, SceneLimits, TextLimits,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = ParseLimits {
        max_file_size: 262144,
        // Flat files have no CFB streams or metadata. These zero budgets must
        // not fabricate or constrain a nonexistent CFB envelope.
        max_stream_size: 0,
        max_total_stream_size: 0,
        max_cfb_metadata_size: 0,
        max_cfb_directory_slots: 0,
        max_cfb_entries: 0,
        max_cfb_depth: 0,
        max_records: 2048,
        max_decompressed_size: 0,
    };
    if let Ok(info) = inspect(data, &limits) {
        assert_eq!(info.format, DracadFormat::Mpx);
        assert!(info.cfb.is_none());
        assert_eq!(info.contents().unwrap(), data);
        let payload = info.payload.as_ref().unwrap();
        assert_eq!(payload.stream_path, None);
        assert_eq!(payload.offset_domain(), "file");
        assert_eq!(payload.size, data.len() as u64);
    }
    if let Ok(raw) = scan_records(data, &limits) {
        assert_eq!(raw.contents(), data);
        let carrier = raw.container().unwrap();
        assert_eq!(carrier.format, DracadFormat::Mpx);
        assert!(carrier.cfb.is_none());
        assert_eq!(raw.header().payload_end, data.len() as u64);
        assert_eq!(raw.header().payload_end_field_raw, 0);
        assert_eq!(raw.header().payload_end_basis.as_str(), "file_length");
        assert_eq!(raw.records().count() as u64, raw.header().record_count);
        assert_eq!(
            raw.tag_counts().values().sum::<u64>(),
            raw.header().record_count
        );
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
        assert_eq!(doc.raw().contents(), data);
        assert_eq!(report.source_format, DracadFormat::Mpx);
        assert_eq!(report.stream_path, None);
        assert_eq!(report.offset_domain(), "file");
        assert_eq!(report.contents_sha256, doc.raw().sha256());
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
        let mut selected = 0;
        for kind in report.requested_entity_types() {
            let count = doc.query_count(kind).unwrap();
            selected += count;
            let page = doc.query(kind, 0, 32).unwrap();
            assert_eq!(page.len() as u64, count);
            assert!(page.iter().all(|e| e.entity_type() == *kind));
        }
        assert_eq!(selected, report.entity_count);
        for (id, entity) in doc.entities().iter().enumerate() {
            assert_eq!(entity.entity_id(), id as u64);
            let source = entity.source_ref();
            assert_eq!(source.stream_path, None);
            assert_eq!(source.offset_domain(), "file");
            let at = source.payload_offset as usize;
            let bytes = &data[at..at + source.byte_length as usize];
            let styles = match entity {
                Entity::Line(e) => {
                    assert_eq!(&bytes[..16], e.geometry.scene().source.header_bytes);
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
                    assert_eq!(bytes, e.geometry.raw_bytes);
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
                    assert_eq!(bytes, e.geometry.raw_bytes);
                    assert!(e
                        .geometry
                        .center
                        .into_iter()
                        .chain([
                            e.geometry.radius,
                            e.geometry.start_angle_degrees,
                            e.geometry.end_angle_degrees
                        ])
                        .all(f64::is_finite));
                    assert!(e.geometry.radius > 0.0);
                    &e.style.styles
                }
                Entity::Text(e) => {
                    assert_eq!(bytes, e.geometry.raw_bytes);
                    assert!(e
                        .geometry
                        .basepoint
                        .into_iter()
                        .chain([
                            e.geometry.width,
                            e.geometry.height,
                            e.geometry.rotation_degrees
                        ])
                        .all(f64::is_finite));
                    assert!(e.geometry.body_bytes.len() <= 1024 && e.geometry.text.len() <= 4096);
                    &e.style.styles
                }
            };
            for property in [&styles.color, &styles.line_type, &styles.width] {
                for evidence in &property.sources {
                    let start = evidence.payload_offset as usize;
                    assert_eq!(
                        &data[start..start + evidence.raw_bytes.len()],
                        evidence.raw_bytes
                    );
                }
            }
        }
        for diagnostic in &report.diagnostics {
            if let Some(source) = &diagnostic.source {
                assert_eq!(source.stream_path, None);
                assert_eq!(source.offset_domain(), "file");
                assert!(source.payload_offset + source.byte_length <= data.len() as u64);
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
            let r = error
                .report()
                .expect("strict alone cannot turn partial input into corruption");
            assert_eq!(r.entity_count, report.entity_count);
            assert_eq!(r.unsupported_counts, report.unsupported_counts);
            assert_eq!(r.offset_domain(), "file");
        }
    }
});
