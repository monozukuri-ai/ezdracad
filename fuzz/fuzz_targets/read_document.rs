#![no_main]
use ezdracad_core::{
    read, ParseLimits, ReadLimits, ReadOptions, ReadPolicy, ReadScope, ReadStatus,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let options = ReadOptions {
        policy: ReadPolicy::Partial,
        scope: ReadScope::StoredXyLines,
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
                + report.unsupported_counts.values().sum::<u64>()
                + report.context_blocked_counts.values().sum::<u64>()
        );
        assert!(report.diagnostics.len() <= 16);
        if report.status == ReadStatus::Complete {
            assert_eq!(report.geometry_status, ReadStatus::Complete);
            assert!(
                report.unsupported_counts.is_empty() && report.context_blocked_counts.is_empty()
            );
        }
        for (index, entity) in doc.entities().iter().enumerate() {
            let entity = entity.as_line().unwrap();
            assert_eq!(entity.entity_id, index as u64);
            assert_eq!(entity.source_ref.offset_domain(), report.offset_domain());
            assert_eq!(entity.source_ref.stream_path, report.stream_path);
            let offset = entity.source_ref.payload_offset as usize;
            assert_eq!(
                &doc.raw().contents()[offset..offset + 16],
                entity.geometry.scene().source.header_bytes
            );
            assert!(entity
                .geometry
                .scene()
                .start
                .into_iter()
                .chain(entity.geometry.scene().end)
                .all(f64::is_finite));
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
                .expect("strict policy alone cannot turn a valid partial document into corruption");
            assert_eq!(r.entity_count, report.entity_count);
            assert_eq!(r.unsupported_counts, report.unsupported_counts);
        }
    }
});
