use ezdracad_core::{scan_contents, DracadError, ParseLimits};

const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/records5.bin");

fn slot(tag: u16, span: u16) -> [u8; 64] {
    let mut data = [0xa5; 64];
    data[..2].copy_from_slice(&tag.to_le_bytes());
    data[8..10].copy_from_slice(&span.to_le_bytes());
    data
}

/// Independently authored slots rebased inside an existing synthetic envelope.
fn contents(slots: &[[u8; 64]]) -> Vec<u8> {
    let mut data = CONTENTS.to_vec();
    let shift = (slots.len() as i64 + 1 - 5) * 64;
    for offset in (0x28..=0x50).step_by(4) {
        let old = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        if old != 0 {
            data[offset..offset + 4]
                .copy_from_slice(&((i64::from(old) + shift) as u32).to_le_bytes());
        }
    }
    data[0x58..0x5c].copy_from_slice(&(slots.len() as u32).to_le_bytes());
    let mut records = slots.concat();
    let mut terminal = [0; 64];
    terminal[..2].copy_from_slice(&0xffu16.to_le_bytes());
    terminal[31] = 0x81; // Terminal body is retained, not a zero-padding condition.
    records.extend_from_slice(&terminal);
    data.splice(8192..8192 + 5 * 64, records);
    data
}

#[test]
fn one_level_spans_preserve_primary_headers_and_leave_interiors_opaque() {
    for marker in [5u16, 8, 25] {
        let original = match marker {
            5 => CONTENTS,
            8 => include_bytes!("../../../tests/fixtures/records8.bin").as_slice(),
            _ => include_bytes!("../../../tests/fixtures/records25.bin").as_slice(),
        };
        let mut data = original.to_vec();
        let slots = [slot(0x0030, 3), slot(0xfe, 0), slot(2, 0), slot(0x7ffe, 1)];
        for (index, record) in slots.iter().enumerate() {
            data[8192 + index * 64..8192 + (index + 1) * 64].copy_from_slice(record);
        }
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let spans = doc.object_spans().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!((spans[0].record_index, spans[0].payload_offset), (0, 8192));
        assert_eq!((spans[0].tag_raw, spans[0].record_count), (0x30, 3));
        assert_eq!(spans[0].end_record_index, 3);
        assert_eq!(spans[0].header_bytes, slots[0][..16]);
        assert_eq!((spans[1].record_index, spans[1].end_record_index), (3, 4));
        assert_eq!((spans[1].tag_raw, spans[1].record_count), (0x7ffe, 1));
        assert_eq!(spans[1].header_bytes, slots[3][..16]);
        // The old flat raw APIs retain the interior, including its zero span.
        assert_eq!(doc.line_candidate_count(), 1);
        assert_eq!(doc.line_records().count(), 1);
        assert_eq!(doc.record(1).unwrap().bytes(), slots[1]);
        assert_eq!(doc.record(2).unwrap().bytes(), slots[2]);
        assert_eq!(doc.contents(), data);
    }
}

#[test]
fn terminal_is_excluded_and_exact_boundary_and_empty_table_are_valid() {
    let data = contents(&[slot(0x4321, 4), slot(0xfe, 0), slot(0xfe, 0), slot(0xfe, 0)]);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let spans = doc.object_spans().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].end_record_index, doc.header().record_count - 1);
    assert_eq!(spans[0].record_count, 4);
    let data = contents(&[]);
    let empty = scan_contents(&data, &ParseLimits::default()).unwrap();
    let mut spans = empty.object_spans();
    assert!(spans.next().is_none());
    assert!(spans.next().is_none());
}

#[test]
fn malformed_spans_fail_once_without_resynchronization() {
    for (tag, span) in [(0x7777, 0), (0x7777, 4), (0xfe, 1)] {
        let data = contents(&[slot(0x0033, 1), slot(tag, span), slot(0x1111, 1)]);
        let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
        let mut spans = doc.object_spans();
        assert_eq!(spans.next().unwrap().unwrap().record_index, 0);
        assert!(matches!(
            spans.next(),
            Some(Err(DracadError::InvalidFormat(_)))
        ));
        assert!(spans.next().is_none());
        assert!(spans.next().is_none());
        assert_eq!(doc.records().count(), 4);
        assert!(doc.object_spans().collect::<Result<Vec<_>, _>>().is_err());
    }
}

#[test]
fn all_primary_slots_are_returned_in_order_without_tag_interpretation() {
    let slots = [
        slot(2, 1),
        slot(0x0402, 1),
        slot(0x3333, 1),
        slot(0x04fe, 1),
    ];
    let data = contents(&slots);
    let doc = scan_contents(&data, &ParseLimits::default()).unwrap();
    let spans = doc.object_spans().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(spans.len(), slots.len());
    for (i, (span, slot)) in spans.iter().zip(slots).enumerate() {
        assert_eq!(span.record_index, i as u64);
        assert_eq!(span.end_record_index, i as u64 + 1);
        assert_eq!(span.header_bytes, slot[..16]);
    }
}
