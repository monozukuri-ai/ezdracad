#![no_main]
use ezdracad_core::{read_best_effort, BestEffortEntity, BestEffortLimits, ParseLimits};
use libfuzzer_sys::fuzz_target;

fn count(entities: &[BestEffortEntity]) -> u64 {
    entities
        .iter()
        .map(|entity| 1 + count(&entity.children))
        .sum()
}

fuzz_target!(|data: &[u8]| {
    let parse_limits = ParseLimits {
        max_file_size: 262144,
        max_stream_size: 131072,
        max_total_stream_size: 262144,
        max_cfb_metadata_size: 65536,
        max_cfb_directory_slots: 64,
        max_cfb_entries: 32,
        max_cfb_depth: 8,
        max_records: 2048,
        max_decompressed_size: 1048576,
    };
    let limits = BestEffortLimits {
        max_entities: 1024,
        max_depth: 8,
        max_curve_points: 4096,
        max_block_definitions: 16,
        max_diagnostics: 16,
    };
    let Ok(drawing) = read_best_effort(data, &parse_limits, &limits) else {
        return;
    };
    // Every returned entity is counted once, and no budget is exceeded.
    let decoded: u64 = drawing.report.decoded_counts.values().sum();
    let returned = count(&drawing.entities)
        + drawing
            .blocks
            .iter()
            .map(|block| count(&block.entities))
            .sum::<u64>();
    assert_eq!(decoded, returned);
    assert!(decoded <= limits.max_entities);
    assert!(drawing.blocks.len() as u64 <= limits.max_block_definitions);
    assert!(drawing.report.diagnostics.len() as u64 <= limits.max_diagnostics);
    assert!(drawing.record_count <= parse_limits.max_records);
    assert_eq!(drawing.payload_sha256.len(), 64);
});
