#![no_main]

use ezdracad_core::{scan_records, Block3DProfile, DracadFormat, ParseLimits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = ParseLimits {
        max_file_size: 256 * 1024,
        max_stream_size: 128 * 1024,
        max_total_stream_size: 256 * 1024,
        max_cfb_metadata_size: 64 * 1024,
        max_cfb_directory_slots: 64,
        max_cfb_entries: 32,
        max_cfb_depth: 8,
        max_records: 2048,
        max_decompressed_size: 1048576,
    };
    if let Ok(doc) = scan_records(data, &limits) {
        for index in 0..doc.header().record_count.min(4) {
            if let Ok(selection) =
                doc.placed_block_lines_3d(index, Block3DProfile::Marker25DirectLine3dV1, 8)
            {
                assert_eq!(selection.contents_sha256(), doc.sha256());
                assert!(!selection.document_complete());
                assert_eq!(selection.units(), None);
                assert!(selection.lines().len() <= 8);
                for (record, offset) in selection
                    .reference()
                    .evidence()
                    .records
                    .iter()
                    .zip(selection.reference().evidence().payload_offsets)
                {
                    assert_eq!(
                        record,
                        &doc.contents()[offset as usize..offset as usize + 64]
                    );
                }
                for line in selection.lines() {
                    assert!(line.start.into_iter().chain(line.end).all(f64::is_finite));
                }
            }
        }
        let carrier = doc.container().unwrap();
        let payload = carrier.payload.as_ref().unwrap();
        match carrier.format {
            DracadFormat::Mpx => {
                assert!(carrier.cfb.is_none());
                assert_eq!(payload.stream_path, None);
                assert_eq!(payload.offset_domain(), "file");
                assert_eq!(doc.contents(), data);
            }
            DracadFormat::Mpz => {
                assert!(carrier.cfb.is_some());
                assert!(payload.stream_path.is_some());
                assert_eq!(payload.offset_domain(), "cfb_stream");
            }
            DracadFormat::Mps => {
                assert!(carrier.cfb.is_some() && carrier.mps.is_some());
                assert_eq!(payload.stream_path, None);
                assert_eq!(payload.offset_domain(), "decoded_payload");
                assert!(doc.contents().len() <= 1048576);
            }
            _ => panic!("unsupported carrier cannot produce raw records"),
        }
        assert_eq!(doc.records().count() as u64, doc.header().record_count);
        assert_eq!(
            doc.tag_counts().values().sum::<u64>(),
            doc.header().record_count
        );
        assert_eq!(
            doc.line_candidates().count() as u64,
            doc.line_candidate_count()
        );
    }
});
