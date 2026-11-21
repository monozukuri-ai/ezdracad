//! Tests of the best-effort reader over independently authored payloads.
//!
//! Every payload is assembled here from the documented layout. None is a
//! drawing saved by the application.

use std::io::{Cursor, Write};

use super::*;
use crate::DracadError;

type Record = [u8; 64];

fn head(tag: u16, span: u16) -> Record {
    let mut record = [0; 64];
    record[..2].copy_from_slice(&tag.to_le_bytes());
    record[2..4].copy_from_slice(&3u16.to_le_bytes());
    record[4] = 7;
    record[5] = 2;
    record[6] = 4;
    record[8..10].copy_from_slice(&span.to_le_bytes());
    record[10..12].copy_from_slice(&9u16.to_le_bytes());
    record
}

fn continuation() -> Record {
    let mut record = [0; 64];
    record[0] = 0xfe;
    record
}

fn put(record: &mut Record, at: usize, values: &[f64]) {
    for (index, value) in values.iter().enumerate() {
        record[at + index * 8..at + index * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
}

fn line(tag: u16, start: [f64; 2], end: [f64; 2]) -> Vec<Record> {
    let mut record = head(tag, 1);
    put(
        &mut record,
        16,
        &[start[0], start[1], end[0], end[1], 0.0, 0.0],
    );
    vec![record]
}

fn curve(tag: u16, center: [f64; 2], tail_values: &[f64]) -> Vec<Record> {
    let mut record = head(tag, 2);
    put(&mut record, 16, &center);
    let mut tail = continuation();
    put(&mut tail, 16, tail_values);
    vec![record, tail]
}

fn ellipse(tag: u16, center: [f64; 2], radius: [f64; 2], tail_values: &[f64]) -> Vec<Record> {
    let mut records = curve(tag, center, tail_values);
    put(&mut records[0], 32, &radius);
    records
}

/// Text with a Shift_JIS or UTF-16 font name, as the generation requires.
fn text(tag: u16, body: &[u8], font: &[u8], origin: u16) -> Vec<Record> {
    let body_records = (body.len() + 1).div_ceil(48);
    let mut records = vec![head(tag, 3 + body_records as u16)];
    put(&mut records[0], 16, &[10.0, 20.0, 0.0, 2.5, 4.0, 0.25]);
    let mut layout = continuation();
    layout[16..18].copy_from_slice(&(body.len() as u16).to_le_bytes());
    layout[18..20].copy_from_slice(&origin.to_le_bytes());
    let mut style = continuation();
    style[2..4].copy_from_slice(&19u16.to_le_bytes());
    style[4..6].copy_from_slice(&1u16.to_le_bytes());
    style[6..8].copy_from_slice(&37u16.to_le_bytes());
    style[8] = 128;
    style[9] = 49;
    put(&mut style, 16, &[30.0, 5.0]);
    style[32..32 + font.len()].copy_from_slice(font);
    records.extend([layout, style]);
    for index in 0..body_records {
        let mut record = continuation();
        if index == 0 {
            record[2..6].copy_from_slice(&[44, 46, 3, 2]);
            put(&mut record, 8, &[1234.5]);
        }
        let chunk = &body[(index * 48).min(body.len())..body.len().min(index * 48 + 48)];
        record[16..16 + chunk.len()].copy_from_slice(chunk);
        records.push(record);
    }
    records
}

fn group(tag: u16, body: &[u8], members: Vec<Vec<Record>>) -> Vec<Record> {
    let members: Vec<Record> = members.into_iter().flatten().collect();
    let mut record = head(tag, 1 + members.len() as u16);
    record[16..16 + body.len()].copy_from_slice(body);
    let mut records = vec![record];
    records.extend(members);
    records
}

fn part(tag: u8, values: &[f64]) -> Vec<Record> {
    let mut record = [0; 64];
    record[0] = tag;
    put(&mut record, 16, values);
    vec![record]
}

fn point(position: [f64; 2], point_type: u32) -> Vec<Record> {
    let mut record = head(1, 1);
    put(&mut record, 16, &position);
    record[32..36].copy_from_slice(&point_type.to_le_bytes());
    vec![record]
}

fn arrowhead(position: [f64; 2]) -> Vec<Record> {
    let mut record = head(0x34, 1);
    put(
        &mut record,
        16,
        &[position[0], position[1], 1.0, 3.0, 270.0],
    );
    record[56] = 5;
    record[57] = 0x80;
    vec![record]
}

fn insert(name: &[u8], position: [f64; 2]) -> Vec<Record> {
    let mut record = head(6, 2);
    record[32..32 + name.len()].copy_from_slice(name);
    let mut tail = continuation();
    put(
        &mut tail,
        16,
        &[position[0], position[1], -10.0, 2.0, 3.0, 45.0],
    );
    vec![record, tail]
}

#[derive(Default)]
struct Parts {
    marker: u16,
    records: Vec<Record>,
    blocks: Vec<(Vec<u8>, Vec<Record>)>,
    flat_end: bool,
    without_tail: bool,
}

fn terminal() -> Record {
    let mut record = [0; 64];
    record[0] = 0xff;
    record
}

fn put_bytes(bytes: &mut [u8], at: usize, values: &[f64]) {
    for (index, value) in values.iter().enumerate() {
        bytes[at + index * 8..at + index * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
}

fn build(parts: &Parts) -> Vec<u8> {
    let mut bytes = vec![0u8; 8192];
    bytes[..18].copy_from_slice(b"DRACAD for Windows");
    bytes[0x22..0x24].copy_from_slice(&parts.marker.to_le_bytes());
    put_bytes(&mut bytes, 0x140, &[5.0, -6.0]);
    put_bytes(&mut bytes, 0x178, &[0.02]);
    put_bytes(&mut bytes, 0x188, &[420.0, 297.0, -100.0, 14850.0]);
    for record in parts.records.iter().chain([&terminal()]) {
        bytes.extend_from_slice(record);
    }
    let mut sections = [bytes.len() as u32; 7];
    // Layer 3 is named and hidden; color 7, line type 2 and width 4 carry values.
    let mut layers = vec![0u8; 16320];
    let row = &mut layers[2 * 64..3 * 64];
    row[1] = 0x05;
    row[4] = 6;
    row[6] = 2;
    row[8] = 4;
    row[32..36].copy_from_slice(&[0x95, 0x94, 0x95, 0x69]);
    let mut colors = vec![0u8; 16320];
    colors[6 * 64 + 2..6 * 64 + 7].copy_from_slice(&[35, 0, 10, 20, 30]);
    let mut line_types = vec![0u8; 2048];
    line_types[64 + 2..64 + 8].copy_from_slice(&[1, 0, 0x2c, 0x01, 0x64, 0x00]);
    let mut widths = vec![0u8; 1024];
    widths[3 * 64 + 2..3 * 64 + 4].copy_from_slice(&50u16.to_le_bytes());
    let tables = [
        layers,
        colors,
        line_types,
        widths,
        vec![0; 3312],
        vec![0; 4096],
    ];
    for (index, table) in tables.iter().enumerate() {
        bytes.extend_from_slice(table);
        sections[index + 1] = bytes.len() as u32;
    }
    bytes.extend_from_slice(&[0; 296]);
    let block_start = if parts.blocks.is_empty() {
        0
    } else {
        bytes.len() as u32
    };
    for (name, records) in &parts.blocks {
        let mut header = [0u8; 176];
        header[..name.len()].copy_from_slice(name);
        header[36..40].copy_from_slice(&(records.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&header);
        for record in records.iter().chain([&terminal()]) {
            bytes.extend_from_slice(record);
        }
    }
    if !parts.without_tail {
        let mut tail = vec![0u8; 1768];
        tail[..16].copy_from_slice(&[2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0]);
        tail[16 + 4..16 + 7].copy_from_slice(&[1, 2, 3]);
        tail[96 + 32..96 + 36].copy_from_slice(b"ZERO");
        tail[408 + 32..408 + 35].copy_from_slice(b"TOP");
        bytes.extend_from_slice(&tail);
    }
    let end = bytes.len() as u32;
    for (index, value) in sections.iter().enumerate() {
        bytes[0x28 + index * 4..0x2c + index * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x24..0x28].copy_from_slice(&8192u32.to_le_bytes());
    bytes[0x4c..0x50].copy_from_slice(&block_start.to_le_bytes());
    bytes[0x58..0x5c].copy_from_slice(&(parts.records.len() as u32).to_le_bytes());
    if !parts.flat_end {
        bytes[0x50..0x54].copy_from_slice(&end.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
    }
    bytes
}

fn read_parts(parts: &Parts) -> BestEffortDrawing {
    read_best_effort(
        &build(parts),
        &ParseLimits::default(),
        &BestEffortLimits::default(),
    )
    .unwrap()
}

fn read(marker: u16, records: Vec<Vec<Record>>) -> BestEffortDrawing {
    read_parts(&Parts {
        marker,
        records: records.into_iter().flatten().collect(),
        ..Parts::default()
    })
}

fn kinds(entities: &[BestEffortEntity]) -> Vec<&'static str> {
    entities.iter().map(BestEffortEntity::kind).collect()
}

fn codes(drawing: &BestEffortDrawing) -> Vec<&'static str> {
    drawing
        .report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn primitives_keep_their_stored_values_and_attributes() {
    let mut line_3d = head(0x13, 1);
    put(&mut line_3d, 16, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let drawing = read(
        5,
        vec![
            line(2, [1.0, 2.0], [3.0, 4.0]),
            curve(3, [5.0, 6.0], &[7.0, 0.0, 360.0]),
            curve(4, [8.0, 9.0], &[2.0, 30.0, 120.0]),
            ellipse(7, [1.5, 2.5], [4.0, 2.0], &[15.0, 0.0, 360.0]),
            ellipse(8, [1.5, 2.5], [4.0, 2.0], &[15.0, 10.0, 80.0]),
            point([9.0, 8.0], 3),
            vec![line_3d],
        ],
    );
    assert!(!drawing.report.lossy());
    assert_eq!((drawing.marker_raw, drawing.marker_observed), (5, true));
    assert_eq!(
        kinds(&drawing.entities),
        [
            "LINE",
            "CIRCLE",
            "ARC",
            "ELLIPSE",
            "ELLIPTICAL_ARC",
            "POINT",
            "LINE3D"
        ]
    );
    let first = &drawing.entities[0];
    assert_eq!(
        first.geometry,
        BestEffortGeometry::Line {
            start: [1.0, 2.0],
            end: [3.0, 4.0],
            elevation: 0.0,
            thickness: 0.0
        }
    );
    let attributes = &first.attributes;
    assert_eq!(
        (
            attributes.layer,
            attributes.color,
            attributes.line_type,
            attributes.width,
            attributes.group
        ),
        (3, 7, 2, 4, 9)
    );
    assert_eq!(
        first.source,
        BestEffortSource {
            table: BestEffortTable::Root,
            record_index: 0,
            payload_offset: 8192,
            record_count: 1
        }
    );
    assert_eq!(
        drawing.entities[1].geometry,
        BestEffortGeometry::Circle {
            center: [5.0, 6.0],
            radius: 7.0,
            elevation: 0.0,
            fill_state: [0; 16]
        }
    );
    assert_eq!(drawing.entities[2].source.record_index, 3);
    assert_eq!(
        drawing.entities[2].geometry,
        BestEffortGeometry::Arc {
            center: [8.0, 9.0],
            radius: 2.0,
            start_degrees: 30.0,
            end_degrees: 120.0,
            elevation: 0.0
        }
    );
    assert_eq!(
        drawing.entities[3].geometry,
        BestEffortGeometry::Ellipse {
            center: [1.5, 2.5],
            radius: [4.0, 2.0],
            rotation_degrees: 15.0
        }
    );
    assert_eq!(
        drawing.entities[4].geometry,
        BestEffortGeometry::EllipticalArc {
            center: [1.5, 2.5],
            radius: [4.0, 2.0],
            rotation_degrees: 15.0,
            start_degrees: 10.0,
            end_degrees: 80.0
        }
    );
    assert_eq!(
        drawing.entities[5].geometry,
        BestEffortGeometry::Point {
            position: [9.0, 8.0],
            point_type: 3
        }
    );
    assert_eq!(
        drawing.entities[6].geometry,
        BestEffortGeometry::Line3D {
            start: [1.0, 2.0, 3.0],
            end: [4.0, 5.0, 6.0]
        }
    );
    assert_eq!(drawing.report.decoded_counts["LINE"], 1);
    // Eleven object records and the terminal.
    assert_eq!(drawing.record_count, 12);
}

#[test]
fn dependent_attributes_and_layout_membership_are_distinguished() {
    let mut records = Vec::new();
    for (tag, color, line_type, width, layer) in [
        (0x0402u16, 0u8, 0u8, 255u8, 0x0201u16),
        (0x2402, 0, 255, 0, 0x0100),
        (0x4402, 0, 1, 1, 0),
        (0x0402, 9, 32, 16, 5),
        (0x0a02, 1, 1, 1, 1),
    ] {
        let mut record = line(tag, [0.0, 0.0], [1.0, 1.0]).remove(0);
        record[2..4].copy_from_slice(&layer.to_le_bytes());
        record[4] = color;
        record[5] = line_type;
        record[6] = width;
        records.push(vec![record]);
    }
    let drawing = read(25, records);
    let values: Vec<_> = drawing
        .entities
        .iter()
        .map(|entity| {
            let a = &entity.attributes;
            (a.color, a.line_type, a.width, a.layer, a.segment)
        })
        .collect();
    assert_eq!(
        values,
        [
            (256, 33, 18, 1, 1),
            (257, 34, 17, 256, 0),
            (258, 1, 1, 0, 0),
            (9, 32, 16, 5, 0),
            (1, 1, 1, 1, 0),
        ]
    );
    let last = &drawing.entities[4].attributes;
    assert!(last.hidden() && last.polyline_member() && !last.reversed_arc());
    assert_eq!((last.tag_raw, last.flags_raw), (0x0a02, 0x0a));
}

#[test]
fn text_is_decoded_per_generation() {
    // Two Shift_JIS characters, then an escaped character outside that set.
    let mut body = vec![0x90, 0x7d, 0x96, 0xca];
    body.extend_from_slice(b"{\\uni\"9ad9\"} A\rB");
    let old = read(8, vec![text(5, &body, &[0x82, 0x6c, 0x82, 0x72, 0], 9)]);
    let BestEffortGeometry::Text(value) = &old.entities[0].geometry else {
        panic!("text expected");
    };
    assert_eq!(value.text, "\u{56f3}\u{9762}\u{9ad9} A\nB");
    assert_eq!(value.text_bytes, body);
    assert_eq!(value.font_name, "\u{ff2d}\u{ff33}");
    assert_eq!(
        (value.position, value.width, value.height),
        ([10.0, 20.0], 2.5, 4.0)
    );
    assert_eq!(
        (value.spacing, value.rotation_degrees, value.slant_degrees),
        (0.25, 30.0, 5.0)
    );
    assert_eq!(
        (value.origin, value.flags_raw, value.charset, value.family),
        (9, 37, 128, 49)
    );
    assert_eq!((value.unit_raw, value.item_raw), (19, 1));
    assert!(!value.numeric && value.number.is_none());
    assert!(!value.lossy && !value.format_codes);

    let font: Vec<u8> = "Font".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let new = read(25, vec![text(20, &[b'x'; 100], &font, 1)]);
    let BestEffortGeometry::Text(value) = &new.entities[0].geometry else {
        panic!("text expected");
    };
    assert_eq!(value.text.len(), 100);
    assert_eq!(value.font_name, "Font");
    assert!(value.numeric);
    assert_eq!(value.number, Some(1234.5));
    assert_eq!(new.entities[0].source.record_count, 6);

    let empty = read(5, vec![text(5, b"", b"F\0", 0)]);
    let BestEffortGeometry::Text(value) = &empty.entities[0].geometry else {
        panic!("text expected");
    };
    assert!(value.text.is_empty() && value.text_bytes.is_empty());
}

#[test]
fn polylines_keep_members_and_evaluate_cubic_splines() {
    let members = |points: &[[f64; 2]]| -> Vec<Vec<Record>> {
        points
            .windows(2)
            .map(|pair| line(0x0802, pair[0], pair[1]))
            .collect()
    };
    let square = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0], [0.0, 0.0]];
    let mut with_arc = members(&square[..3]);
    with_arc.push(curve(0x0804, [2.0, 4.0], &[2.0, 0.0, 180.0]));
    let mut broken = members(&square[..3]);
    broken.push(line(0x0802, [9.0, 9.0], [8.0, 8.0]));
    let drawing = read(
        5,
        vec![
            group(10, &[0, 0x05, 4, 0], with_arc),
            group(10, &[3, 0, 0, 10], members(&square[..4])),
            group(10, &[3, 1, 0, 5], members(&square)),
            group(10, &[2, 0, 0, 10], members(&square[..3])),
            group(10, &[3, 0, 0, 10], broken),
            group(18, &[0, 0, 0, 0], vec![]),
        ],
    );
    let plain = &drawing.entities[0];
    assert_eq!(
        plain.geometry,
        BestEffortGeometry::Polyline {
            curve_kind: 0,
            flags_raw: 5,
            fill_color: 4,
            divisions: 0,
            three_d: false,
            curve_points: None
        }
    );
    assert_eq!(kinds(&plain.children), ["LINE", "LINE", "ARC"]);
    assert!(plain.children[0].attributes.polyline_member());
    assert_eq!(plain.children[0].source.record_index, 1);
    assert_eq!(plain.source.record_count, 5);
    let BestEffortGeometry::Polyline {
        curve_points: Some(open),
        ..
    } = &drawing.entities[1].geometry
    else {
        panic!("open spline expected");
    };
    assert_eq!(open.len(), 3 * 10 + 1);
    assert_eq!(open[0], [0.0, 0.0]);
    assert!(open[30][0].abs() < 1e-12 && (open[30][1] - 4.0).abs() < 1e-12);
    let BestEffortGeometry::Polyline {
        curve_points: Some(closed),
        ..
    } = &drawing.entities[2].geometry
    else {
        panic!("closed spline expected");
    };
    assert_eq!(closed.len(), 4 * 5 + 1);
    assert_eq!(closed[0], closed[20]);
    // Other curve kinds and unconnected members return what is stored.
    assert!(matches!(
        drawing.entities[3].geometry,
        BestEffortGeometry::Polyline {
            curve_kind: 2,
            curve_points: None,
            ..
        }
    ));
    assert!(matches!(
        drawing.entities[4].geometry,
        BestEffortGeometry::Polyline {
            curve_kind: 3,
            curve_points: None,
            ..
        }
    ));
    assert!(matches!(
        drawing.entities[5].geometry,
        BestEffortGeometry::Polyline { three_d: true, .. }
    ));
    assert_eq!(
        codes(&drawing),
        [
            "spline_kind_not_evaluated",
            "spline_control_polygon_unusable"
        ]
    );
    assert_eq!(drawing.report.decoded_counts["POLYLINE"], 6);
    assert_eq!(drawing.report.decoded_counts["LINE"], 2 + 3 + 4 + 2 + 3);
    assert!(!drawing.report.lossy());
}

#[test]
fn hatches_refer_to_their_region_by_identifier() {
    let ring = vec![
        line(0x0802, [0.0, 0.0], [10.0, 0.0]),
        line(0x0802, [10.0, 0.0], [10.0, 5.0]),
        line(0x0802, [10.0, 5.0], [0.0, 0.0]),
    ];
    let mut region = [0u8; 16];
    region[..3].copy_from_slice(&[0, 5, 6]);
    region[4..8].copy_from_slice(&1u32.to_le_bytes());
    region[12..16].copy_from_slice(&0x098du32.to_le_bytes());
    let mut hatch = [0u8; 6];
    hatch[2..6].copy_from_slice(&0x098du32.to_le_bytes());
    let mut pattern = [1u8, 0, 0, 0, 0, 0];
    pattern[2..6].copy_from_slice(&0x05b1u32.to_le_bytes());
    let drawing = read(
        22,
        vec![
            group(12, &region, vec![group(10, &[0, 1, 0, 0], ring)]),
            group(
                13,
                &hatch,
                vec![
                    part(0x4b, &[1.0, 2.0, 0.5, 0.0]),
                    part(0x4b, &[1.0, 2.0, 0.5, 90.0]),
                ],
            ),
            group(
                13,
                &pattern,
                vec![part(0x4c, &[3.0, 4.0, 0.4, 360.0]), part(0x4d, &[0.005])],
            ),
        ],
    );
    assert_eq!(
        drawing.entities[0].geometry,
        BestEffortGeometry::ExtendedPolygon {
            polygon_kind: 0,
            flags_raw: 5,
            fill_color: 6,
            loop_count: 1,
            link_id: 0x098d
        }
    );
    assert_eq!(drawing.entities[0].children[0].children.len(), 3);
    assert_eq!(
        drawing.entities[1].geometry,
        BestEffortGeometry::Hatch {
            hatch_type: 0,
            link_id: 0x098d,
            lines: vec![
                BestEffortHatchLine {
                    origin: [1.0, 2.0],
                    pitch: 0.5,
                    angle_degrees: 0.0
                },
                BestEffortHatchLine {
                    origin: [1.0, 2.0],
                    pitch: 0.5,
                    angle_degrees: 90.0
                },
            ],
            pattern_origin: None
        }
    );
    assert_eq!(
        drawing.entities[2].geometry,
        BestEffortGeometry::Hatch {
            hatch_type: 1,
            link_id: 0x05b1,
            lines: Vec::new(),
            pattern_origin: Some([3.0, 4.0])
        }
    );
    assert_eq!(
        codes(&drawing),
        ["hatch_pattern_not_decoded", "hatch_parts_ignored"]
    );
    assert!(!drawing.report.lossy());
}

#[test]
fn dimensions_and_their_drawn_packages_share_a_link() {
    let linked = |mut records: Vec<Record>| {
        records[0][14..16].copy_from_slice(&104u16.to_le_bytes());
        records
    };
    let drawing = read(
        10,
        vec![
            linked(group(
                26,
                &[2, 33, 0, 3, 1, 3, 1, 51],
                vec![
                    part(0x41, &[0.0, 30.0, 100.0, 30.0]),
                    part(0x43, &[0.0, 10.0]),
                    part(0x43, &[100.0, 10.0]),
                    part(0x44, &[30.0, 30.0, 30.0, 30.0]),
                    linked(text(20, b"100", b"Font\0", 1)),
                    part(0x50, &[1.0, 3.0, 1.0, 3.0]),
                    part(0x5f, &[0.0]),
                ],
            )),
            group(
                48,
                &[3, 0, 0, 0, 7, 0, 0, 0],
                vec![
                    linked(line(2, [0.0, 30.0], [100.0, 30.0])),
                    point([0.0, 10.0], 1),
                    arrowhead([0.0, 30.0]),
                    linked(text(20, b"100", b"Font\0", 1)),
                ],
            ),
        ],
    );
    let dimension = &drawing.entities[0];
    assert_eq!(
        dimension.geometry,
        BestEffortGeometry::Dimension {
            dimension_type: 2,
            flags_raw: 33,
            flags2_raw: 0,
            arrow_styles: [3, 1, 3, 1],
            text_position: 51,
            line: Some([[0.0, 30.0], [100.0, 30.0]]),
            extension_origins: vec![[0.0, 10.0], [100.0, 10.0]],
            arrow_sizes: Some([30.0; 4]),
            arrow_paper_sizes: Some([1.0, 3.0, 1.0, 3.0])
        }
    );
    assert_eq!(dimension.attributes.link_id, 104);
    assert_eq!(kinds(&dimension.children), ["TEXT"]);
    let package = &drawing.entities[1];
    assert_eq!(
        package.geometry,
        BestEffortGeometry::Package {
            package_kind: 3,
            flags_raw: 0,
            reference_number: 7
        }
    );
    assert_eq!(
        kinds(&package.children),
        ["LINE", "POINT", "ARROWHEAD", "TEXT"]
    );
    assert_eq!(package.children[0].attributes.link_id, 104);
    assert_eq!(
        package.children[2].geometry,
        BestEffortGeometry::Arrowhead {
            position: [0.0, 30.0],
            size: [1.0, 3.0],
            angle_degrees: 270.0,
            arrow_type: 5,
            flags_raw: 0x80
        }
    );
    assert_eq!(codes(&drawing), ["dimension_parts_ignored"]);
    assert_eq!(drawing.report.decoded_counts["TEXT"], 2);
    assert!(!drawing.report.lossy());
}

#[test]
fn block_definitions_and_references_are_read() {
    let drawing = read_parts(&Parts {
        marker: 5,
        records: insert(b"SYM", [100.0, 200.0]),
        blocks: vec![
            (b"SYM".to_vec(), line(2, [0.0, 0.0], [1.0, 0.0])),
            (vec![0x95, 0x94, 0x95, 0x69], Vec::new()),
        ],
        ..Parts::default()
    });
    assert_eq!(
        drawing.entities[0].geometry,
        BestEffortGeometry::Insert {
            name: "SYM".into(),
            name_bytes: b"SYM".to_vec(),
            position: [100.0, 200.0],
            elevation: -10.0,
            scale: [2.0, 3.0],
            rotation_degrees: 45.0
        }
    );
    assert_eq!(drawing.blocks.len(), 2);
    let block = &drawing.blocks[0];
    assert_eq!((block.index, block.name.as_str()), (0, "SYM"));
    let member = &block.entities[0];
    assert_eq!(member.source.table, BestEffortTable::Block(0));
    assert_eq!(member.source.record_index, 0);
    assert_eq!(member.source.payload_offset, block.header_offset + 176);
    assert_eq!(drawing.blocks[1].name, "\u{90e8}\u{54c1}");
    assert!(drawing.blocks[1].entities.is_empty());
    // The special rows after the definitions are still found.
    assert_eq!(drawing.layers.len(), 257);
    assert!(!drawing.report.lossy());
}

#[test]
fn tables_and_settings_are_reported_without_being_applied() {
    let drawing = read(8, vec![line(2, [1.0, 2.0], [3.0, 4.0])]);
    assert_eq!(drawing.layers.len(), 257);
    assert_eq!(
        (drawing.layers[0].index, drawing.layers[0].name.as_str()),
        (0, "ZERO")
    );
    assert_eq!(
        (drawing.layers[256].index, drawing.layers[256].name.as_str()),
        (256, "TOP")
    );
    let layer = &drawing.layers[3];
    assert_eq!(
        (layer.index, layer.name.as_str(), layer.hidden),
        (3, "\u{90e8}\u{54c1}", true)
    );
    assert_eq!(
        (layer.color, layer.line_type, layer.width, layer.flags_raw),
        (6, 2, 4, 0x0500)
    );
    assert!(!drawing.layers[1].hidden);
    assert_eq!(drawing.colors.len(), 256);
    let color = &drawing.colors[6];
    assert_eq!(
        (color.index, color.rgb, color.pen_width),
        (7, [10, 20, 30], 0.35)
    );
    assert_eq!(
        (drawing.colors[255].index, drawing.colors[255].rgb),
        (256, [1, 2, 3])
    );
    assert_eq!(drawing.line_types.len(), 32);
    assert_eq!(drawing.line_types[1].pattern, [3.0, 1.0]);
    assert_eq!(drawing.line_types[1].pen_style, 1);
    assert!(drawing.line_types[0].pattern.is_empty());
    assert_eq!((drawing.widths.len(), drawing.widths[3].width), (16, 0.5));
    assert_eq!(
        drawing.settings,
        Some(BestEffortSettings {
            drawing_origin: [5.0, -6.0],
            drawing_scale: 50.0,
            paper_size: [420.0, 297.0],
            paper_origin: [-100.0, 14850.0]
        })
    );
    // Settings do not move the stored coordinates.
    assert!(matches!(
        drawing.entities[0].geometry,
        BestEffortGeometry::Line {
            start: [1.0, 2.0],
            ..
        }
    ));
}

#[test]
fn unknown_and_misfit_objects_are_counted_and_do_not_hide_later_ones() {
    let mut broken_line = line(2, [0.0, 0.0], [1.0, 1.0]);
    put(&mut broken_line[0], 32, &[f64::NAN]);
    let mut lone_circle = curve(3, [0.0, 0.0], &[1.0]);
    lone_circle.truncate(1);
    lone_circle[0][8] = 1;
    let mut short_text = text(5, b"abc", b"F\0", 0);
    short_text[1][16] = 200;
    let property = vec![head(0x33, 2), continuation()];
    let drawing = read(
        5,
        vec![
            property,
            part(0x19, &[0.0]),
            group(0x1b, &[], vec![line(2, [0.0, 0.0], [1.0, 1.0])]),
            broken_line,
            lone_circle,
            curve(4, [0.0, 0.0], &[-1.0, 0.0, 90.0]),
            short_text,
            line(2, [7.0, 7.0], [8.0, 8.0]),
        ],
    );
    assert_eq!(drawing.entities.len(), 1);
    assert_eq!(
        drawing.entities[0].geometry,
        BestEffortGeometry::Line {
            start: [7.0, 7.0],
            end: [8.0, 8.0],
            elevation: 0.0,
            thickness: 0.0
        }
    );
    let report = &drawing.report;
    assert!(report.lossy());
    assert_eq!(report.property_records, 1);
    assert_eq!(
        report.unsupported_counts.iter().collect::<Vec<_>>(),
        [(&0x19, &1), (&0x1b, &1)]
    );
    assert_eq!(
        report.failed_counts.iter().collect::<Vec<_>>(),
        [(&2, &1), (&3, &1), (&4, &1), (&5, &1)]
    );
    assert_eq!(codes(&drawing), ["object_layout_mismatch"; 4]);
    assert_eq!(report.diagnostics[0].record_index, Some(5));
    assert_eq!(report.diagnostics[0].tag_raw, Some(2));
    assert_eq!(report.diagnostics[0].table, Some(BestEffortTable::Root));
    assert_eq!(report.decoded_counts.get("LINE"), Some(&1));
}

#[test]
fn stray_continuations_and_overlong_objects_are_reported() {
    let mut overlong = group(
        10,
        &[0, 0, 0, 0],
        vec![line(0x0802, [0.0, 0.0], [1.0, 0.0])],
    );
    overlong[0][8] = 9;
    let drawing = read(
        5,
        vec![
            vec![continuation()],
            line(2, [0.0, 0.0], [1.0, 1.0]),
            vec![continuation()],
            overlong,
        ],
    );
    assert_eq!(drawing.report.orphan_records, 1);
    assert_eq!(drawing.report.clamped_spans, 1);
    assert!(drawing.report.lossy());
    // A continuation written after a single-record object stays with it.
    assert_eq!(drawing.entities[0].source.record_count, 2);
    assert_eq!(drawing.entities[1].children.len(), 1);
}

#[test]
fn unchecked_generations_and_flat_files_are_read_with_a_note() {
    let drawing = read(77, vec![line(2, [0.0, 0.0], [1.0, 1.0])]);
    assert!(!drawing.marker_observed);
    assert_eq!(codes(&drawing), ["generation_not_observed"]);
    assert_eq!(drawing.entities.len(), 1);
    assert!(!drawing.report.lossy());

    let flat = read_parts(&Parts {
        marker: 8,
        records: line(2, [0.0, 0.0], [1.0, 1.0]),
        flat_end: true,
        ..Parts::default()
    });
    assert_eq!(flat.format, DracadFormat::Mpx);
    assert_eq!(flat.entities.len(), 1);
    assert_eq!(flat.payload_sha256.len(), 64);
    assert!(codes(&flat).is_empty());
}

#[test]
fn a_missing_tail_only_costs_the_special_rows() {
    let drawing = read_parts(&Parts {
        marker: 22,
        records: line(2, [0.0, 0.0], [1.0, 1.0]),
        without_tail: true,
        ..Parts::default()
    });
    assert_eq!(drawing.entities.len(), 1);
    assert_eq!((drawing.layers.len(), drawing.colors.len()), (255, 255));
    assert_eq!(codes(&drawing), ["block_table_end_not_found"]);
}

#[test]
fn budgets_are_errors() {
    let records: Vec<Record> = (0..4)
        .flat_map(|_| line(2, [0.0, 0.0], [1.0, 1.0]))
        .collect();
    let bytes = build(&Parts {
        marker: 5,
        records,
        ..Parts::default()
    });
    assert!(matches!(
        read_best_effort(
            &bytes,
            &ParseLimits::default(),
            &BestEffortLimits {
                max_entities: 3,
                ..BestEffortLimits::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "best_effort_entities",
            limit: 3,
            actual: 4
        })
    ));
    assert!(matches!(
        read_best_effort(
            &bytes,
            &ParseLimits {
                max_records: 4,
                ..ParseLimits::default()
            },
            &BestEffortLimits::default()
        ),
        Err(DracadError::LimitExceeded {
            resource: "records",
            ..
        })
    ));
    assert!(matches!(
        read_best_effort(
            &bytes,
            &ParseLimits {
                max_file_size: 100,
                ..ParseLimits::default()
            },
            &BestEffortLimits::default()
        ),
        Err(DracadError::LimitExceeded {
            resource: "file_size",
            ..
        })
    ));

    let nested = group(48, &[], vec![group(48, &[], vec![group(48, &[], vec![])])]);
    let bytes = build(&Parts {
        marker: 5,
        records: nested,
        ..Parts::default()
    });
    assert!(matches!(
        read_best_effort(
            &bytes,
            &ParseLimits::default(),
            &BestEffortLimits {
                max_depth: 1,
                ..BestEffortLimits::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "best_effort_depth",
            limit: 1,
            actual: 2
        })
    ));

    let spline = group(
        10,
        &[3, 0, 0, 200],
        vec![
            line(0x0802, [0.0, 0.0], [1.0, 0.0]),
            line(0x0802, [1.0, 0.0], [1.0, 1.0]),
        ],
    );
    let bytes = build(&Parts {
        marker: 5,
        records: spline,
        ..Parts::default()
    });
    assert!(matches!(
        read_best_effort(
            &bytes,
            &ParseLimits::default(),
            &BestEffortLimits {
                max_curve_points: 400,
                ..BestEffortLimits::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "best_effort_curve_points",
            limit: 400,
            actual: 401
        })
    ));

    let blocks = (0..3).map(|_| (b"B".to_vec(), Vec::new())).collect();
    let bytes = build(&Parts {
        marker: 5,
        blocks,
        ..Parts::default()
    });
    assert!(matches!(
        read_best_effort(
            &bytes,
            &ParseLimits::default(),
            &BestEffortLimits {
                max_block_definitions: 2,
                ..BestEffortLimits::default()
            }
        ),
        Err(DracadError::LimitExceeded {
            resource: "block_definitions",
            limit: 2,
            actual: 3
        })
    ));

    let noisy: Vec<Record> = (0..5).flat_map(|_| curve(4, [0.0, 0.0], &[-1.0])).collect();
    let bytes = build(&Parts {
        marker: 5,
        records: noisy,
        ..Parts::default()
    });
    let drawing = read_best_effort(
        &bytes,
        &ParseLimits::default(),
        &BestEffortLimits {
            max_diagnostics: 2,
            ..BestEffortLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        (
            drawing.report.diagnostics.len(),
            drawing.report.diagnostics_suppressed
        ),
        (2, 3)
    );
    assert_eq!(drawing.report.failed_counts[&4], 5);
}

#[test]
fn damaged_tables_are_errors_not_partial_results() {
    let good = build(&Parts {
        marker: 5,
        records: line(2, [0.0, 0.0], [1.0, 1.0]),
        ..Parts::default()
    });
    let read = |bytes: &[u8]| {
        read_best_effort(bytes, &ParseLimits::default(), &BestEffortLimits::default())
    };
    let mutate = |change: &dyn Fn(&mut Vec<u8>)| {
        let mut bytes = good.clone();
        change(&mut bytes);
        read(&bytes)
    };
    assert!(read(&good).is_ok());
    for result in [
        mutate(&|bytes| bytes[0] = b'X'),
        mutate(&|bytes| bytes[0x58] = 7),
        mutate(&|bytes| bytes[8192 + 64] = 0x02),
        mutate(&|bytes| bytes.truncate(bytes.len() - 1)),
        mutate(&|bytes| bytes[0x2c..0x30].copy_from_slice(&1u32.to_le_bytes())),
        mutate(&|bytes| bytes[0x4c] = 1),
        read(&good[..0x40]),
    ] {
        assert!(
            matches!(result, Err(DracadError::InvalidFormat(_))),
            "{result:?}"
        );
    }
    // A terminal with flag bits is still a terminal.
    assert!(mutate(&|bytes| bytes[8192 + 64 + 1] = 0x04).is_ok());
    // Tables of another size are skipped, not guessed.
    let resized = mutate(&|bytes| {
        let value = u32::from_le_bytes(bytes[0x2c..0x30].try_into().unwrap()) - 64;
        bytes[0x2c..0x30].copy_from_slice(&value.to_le_bytes());
    })
    .unwrap();
    assert!(resized.layers.is_empty() && resized.colors.is_empty());
    assert_eq!(resized.entities.len(), 1);
    assert!(codes(&resized).contains(&"style_tables_not_read"));
}

#[test]
fn compound_containers_are_identified_by_content() {
    let payload = build(&Parts {
        marker: 5,
        records: line(2, [0.0, 0.0], [1.0, 1.0]),
        ..Parts::default()
    });
    let wrap = |clsid: &str, stream: &str| {
        let mut compound = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        compound
            .set_storage_clsid("/", clsid.parse().unwrap())
            .unwrap();
        compound
            .create_stream(stream)
            .unwrap()
            .write_all(&payload)
            .unwrap();
        compound.flush().unwrap();
        compound.into_inner().into_inner()
    };
    let read = |bytes: &[u8]| {
        read_best_effort(bytes, &ParseLimits::default(), &BestEffortLimits::default())
    };
    let drawing = read(&wrap(crate::DRACAD_CLSID, "/Contents")).unwrap();
    assert_eq!(drawing.format, DracadFormat::Mpz);
    assert_eq!(drawing.entities.len(), 1);
    assert_eq!(drawing.payload_size, payload.len() as u64);
    for bytes in [
        wrap("00000000-0000-0000-0000-000000000001", "/Contents"),
        wrap(crate::DRACAD_CLSID, "/Other"),
        wrap(crate::DRACAD_CLSID, "/SecureContents"),
    ] {
        assert!(matches!(
            read(&bytes),
            Err(DracadError::UnsupportedFormat(_))
        ));
    }
    assert!(matches!(
        read(b"not a drawing"),
        Err(DracadError::InvalidFormat(_))
    ));
}
