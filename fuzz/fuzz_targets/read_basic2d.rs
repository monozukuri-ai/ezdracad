#![no_main]
use ezdracad_core::{
    read, Basic2DProfile, Entity, ParseLimits, ReadLimits, ReadOptions, ReadPolicy, ReadScope,
    ReadStatus, TextLimits,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let options = ReadOptions {
        policy: ReadPolicy::Partial,
        scope: ReadScope::StoredXyPrimitives,
        basic_profile: Some(Basic2DProfile::Marker25RootPrimitivesV1),
        text_limits: TextLimits {
            max_encoded_bytes: 4096,
            max_output_bytes: 4096,
        },
        parse_limits: ParseLimits {
            max_file_size: 262144,
            max_stream_size: 131072,
            max_total_stream_size: 262144,
            max_cfb_metadata_size: 65536,
            max_cfb_directory_slots: 64,
            max_cfb_entries: 32,
            max_cfb_depth: 8,
            max_records: 2048,
            max_decompressed_size: 1048576,
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
        assert!(!report.document_complete());
        assert_eq!(report.units(), None);
        let payload = doc.raw().container().unwrap().payload.as_ref().unwrap();
        assert_eq!(report.offset_domain(), payload.offset_domain());
        assert_eq!(report.stream_path, payload.stream_path);
        assert_eq!(report.source_format, doc.raw().container().unwrap().format);
        assert_eq!(report.entity_count, doc.entities().len() as u64);
        assert_eq!(
            report.raw_record_count,
            1 + report.primary_object_count + report.interior_record_count
        );
        assert_eq!(
            report.primary_object_count,
            report.decoded_object_count
                + report.handled_auxiliary_object_count
                + report.unsupported_counts.values().sum::<u64>()
                + report.context_blocked_counts.values().sum::<u64>()
        );
        assert!(report.diagnostics.len() <= 16 && report.scene_nodes_visited <= 256);
        assert_eq!(
            report.requested_entity_types(),
            ["LINE", "CIRCLE", "ARC", "TEXT"]
        );
        let mut selected = 0;
        for kind in report.requested_entity_types() {
            let count = doc.query_count(kind).unwrap();
            selected += count;
            let page = doc.query(kind, 0, 32).unwrap();
            assert_eq!(page.len() as u64, count);
            for entity in page {
                assert_eq!(entity.entity_type(), *kind);
            }
        }
        assert_eq!(selected, report.entity_count);
        for (id, entity) in doc.entities().iter().enumerate() {
            assert_eq!(entity.entity_id(), id as u64);
            let source = entity.source_ref();
            assert_eq!(source.offset_domain(), report.offset_domain());
            assert_eq!(source.stream_path, report.stream_path);
            let at = source.payload_offset as usize;
            let raw = &doc.raw().contents()[at..at + source.byte_length as usize];
            match entity {
                Entity::Line(e) => {
                    assert_eq!(&raw[..16], e.geometry.scene().source.header_bytes);
                    assert!(e
                        .geometry
                        .scene()
                        .start
                        .into_iter()
                        .chain(e.geometry.scene().end)
                        .all(f64::is_finite));
                }
                Entity::Circle(e) => {
                    assert_eq!(raw, e.geometry.raw_bytes);
                    assert!(e
                        .geometry
                        .center
                        .into_iter()
                        .chain([e.geometry.radius])
                        .all(f64::is_finite));
                    assert!(e.geometry.radius > 0.0);
                }
                Entity::Arc(e) => {
                    assert_eq!(raw, e.geometry.raw_bytes);
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
                }
                Entity::Text(e) => {
                    assert_eq!(raw, e.geometry.raw_bytes);
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
                    assert!(e.geometry.body_bytes.len() <= 4096 && e.geometry.text.len() <= 4096);
                }
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
            assert_eq!(report.geometry_status, ReadStatus::Complete);
            assert!(
                report.unsupported_counts.is_empty() && report.context_blocked_counts.is_empty()
            );
            assert!(strict.is_ok());
        } else {
            let error = strict.unwrap_err();
            let strict_report = error
                .report()
                .expect("strict policy alone cannot corrupt an admitted partial input");
            assert_eq!(strict_report.entity_count, report.entity_count);
            assert_eq!(strict_report.unsupported_counts, report.unsupported_counts);
            assert_eq!(
                strict_report.handled_auxiliary_object_count,
                report.handled_auxiliary_object_count
            );
        }
    }
});
