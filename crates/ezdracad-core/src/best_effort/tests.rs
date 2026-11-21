//! Tests of the best-effort reader over independently authored payloads.
//!
//! Every payload is assembled here from the documented layout, in the older
//! 64-byte record layout or in the Unicode 80-byte one. None is a drawing
//! saved by the application.

use std::io::{Cursor, Write};

use super::*;
use crate::DracadError;

type Record = Vec<u8>;

/// Record builder for one of the two layouts.
#[derive(Clone, Copy)]
struct Shape {
    wide: bool,
}

const NARROW: Shape = Shape { wide: false };
const WIDE: Shape = Shape { wide: true };

fn floats(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

impl Shape {
    fn size(self) -> usize {
        if self.wide {
            80
        } else {
            64
        }
    }

    /// An object or attributed part: layer 3, color 7, line type 2, width 4,
    /// group 9 and a 48-byte body.
    fn object(self, tag: u16, span: u16, body: &[u8]) -> Record {
        let mut record = vec![0; self.size()];
        record[..2].copy_from_slice(&tag.to_le_bytes());
        if self.wide {
            record[4..8].copy_from_slice(&7u32.to_le_bytes());
            record[8] = 2;
            record[9] = 4;
            record[10..12].copy_from_slice(&3u16.to_le_bytes());
            record[12..16].copy_from_slice(&u32::from(span).to_le_bytes());
            record[16..18].copy_from_slice(&9u16.to_le_bytes());
            record[32..32 + body.len()].copy_from_slice(body);
        } else {
            record[2..4].copy_from_slice(&3u16.to_le_bytes());
            record[4] = 7;
            record[5] = 2;
            record[6] = 4;
            record[8..10].copy_from_slice(&span.to_le_bytes());
            record[10..12].copy_from_slice(&9u16.to_le_bytes());
            record[16..16 + body.len()].copy_from_slice(body);
        }
        record
    }

    /// Set the layer, color, line type and width fields of an object record.
    fn style(self, record: &mut Record, layer: u16, color: u8, line_type: u8, width: u8) {
        if self.wide {
            record[4..8].copy_from_slice(&u32::from(color).to_le_bytes());
            record[8] = line_type;
            record[9] = width;
            record[10..12].copy_from_slice(&layer.to_le_bytes());
        } else {
            record[2..4].copy_from_slice(&layer.to_le_bytes());
            record[4] = color;
            record[5] = line_type;
            record[6] = width;
        }
    }

    fn link(self, mut records: Vec<Record>, link: u16) -> Vec<Record> {
        let at = if self.wide { 20 } else { 14 };
        records[0][at..at + 2].copy_from_slice(&link.to_le_bytes());
        records
    }

    /// A continuation or plain part: a 16-byte header and the rest as body.
    fn plain(self, tag: u8, header: &[u8], body: &[u8]) -> Record {
        let mut record = vec![0; self.size()];
        record[0] = tag;
        record[2..2 + header.len()].copy_from_slice(header);
        record[16..16 + body.len()].copy_from_slice(body);
        record
    }

    fn continuation(self, body: &[u8]) -> Record {
        self.plain(0xfe, &[], body)
    }

    fn line(self, tag: u16, start: [f64; 2], end: [f64; 2]) -> Vec<Record> {
        vec![self.object(
            tag,
            1,
            &floats(&[start[0], start[1], end[0], end[1], 0.0, 0.0]),
        )]
    }

    /// Circle, arc, ellipse or elliptical arc: a head and one continuation.
    fn curve(self, tag: u16, head: &[f64], tail: &[f64]) -> Vec<Record> {
        vec![
            self.object(tag, 2, &floats(head)),
            self.continuation(&floats(tail)),
        ]
    }

    fn encode(self, text: &str) -> Vec<u8> {
        if self.wide {
            text.encode_utf16().flat_map(u16::to_le_bytes).collect()
        } else {
            // Tests of the older layout pass ASCII or ready-made Shift_JIS bytes.
            text.as_bytes().to_vec()
        }
    }

    fn text_bytes(self, tag: u16, body: &[u8], font: &[u8], origin: u16) -> Vec<Record> {
        let unit = if self.wide { 2 } else { 1 };
        let capacity = self.size() - 16;
        let body_records = (body.len() + unit).div_ceil(capacity);
        let mut records = vec![self.object(
            tag,
            3 + body_records as u16,
            &floats(&[10.0, 20.0, 0.0, 2.5, 4.0, 0.25]),
        )];
        let mut layout = [0u8; 4];
        layout[..2].copy_from_slice(&((body.len() / unit) as u16).to_le_bytes());
        layout[2..].copy_from_slice(&origin.to_le_bytes());
        records.push(self.continuation(&layout));
        let mut style = floats(&[30.0, 5.0]);
        style.extend_from_slice(font);
        // Unit 19, item 1, flags 37, character set 128, family 49.
        records.push(self.plain(0xfe, &[19, 0, 1, 0, 37, 0, 128, 49], &style));
        for index in 0..body_records {
            let chunk =
                &body[(index * capacity).min(body.len())..body.len().min((index + 1) * capacity)];
            let mut header = vec![0u8; 14];
            if index == 0 {
                header[..4].copy_from_slice(&[44, 46, 3, 2]);
                header[6..14].copy_from_slice(&1234.5f64.to_le_bytes());
            }
            records.push(self.plain(0xfe, &header, chunk));
        }
        records
    }

    fn text(self, tag: u16, value: &str, origin: u16) -> Vec<Record> {
        self.text_bytes(tag, &self.encode(value), b"Font\0", origin)
    }

    fn group(self, tag: u16, body: &[u8], members: Vec<Vec<Record>>) -> Vec<Record> {
        let members: Vec<Record> = members.into_iter().flatten().collect();
        let mut records = vec![self.object(tag, 1 + members.len() as u16, body)];
        records.extend(members);
        records
    }

    fn part(self, tag: u8, values: &[f64]) -> Vec<Record> {
        vec![self.plain(tag, &[], &floats(values))]
    }

    /// A part that carries attribute references, such as a hatch line.
    fn attributed_part(self, tag: u16, span: u16, values: &[f64]) -> Record {
        self.object(tag, span, &floats(values))
    }

    fn point(self, position: [f64; 2], point_type: u32) -> Vec<Record> {
        let mut body = floats(&position);
        body.extend_from_slice(&point_type.to_le_bytes());
        vec![self.object(1, 1, &body)]
    }

    fn arrowhead(self, position: [f64; 2]) -> Vec<Record> {
        let mut body = floats(&[position[0], position[1], 1.0, 3.0, 270.0]);
        body.extend_from_slice(&[5, 0x80]);
        vec![self.object(0x34, 1, &body)]
    }

    fn insert(self, name: &str, reference: u32, position: [f64; 2]) -> Vec<Record> {
        let mut body = vec![0u8; 16];
        body[4..8].copy_from_slice(&reference.to_le_bytes());
        body.extend_from_slice(&self.encode(name));
        vec![
            self.object(6, 2, &body),
            self.continuation(&floats(&[position[0], position[1], -10.0, 2.0, 3.0, 45.0])),
        ]
    }

    /// File names of an image or external reference, with one continuation.
    fn names(self, values: &[&str]) -> Vec<Record> {
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&self.encode(value));
            bytes.resize(bytes.len() + if self.wide { 2 } else { 1 }, 0);
        }
        let capacity = self.size() - 16;
        bytes.resize(2 * capacity, 0);
        vec![
            self.plain(0x4a, &[], &bytes[..capacity]),
            self.continuation(&bytes[capacity..]),
        ]
    }
}

#[derive(Default)]
struct Parts {
    marker: u16,
    wide: bool,
    records: Vec<Record>,
    blocks: Vec<(Vec<u8>, Vec<Record>)>,
    flat_end: bool,
    without_tail: bool,
}

fn put(bytes: &mut [u8], at: usize, values: &[f64]) {
    bytes[at..at + values.len() * 8].copy_from_slice(&floats(values));
}

fn build(parts: &Parts) -> Vec<u8> {
    let shape = Shape { wide: parts.wide };
    let size = shape.size();
    let shift = if parts.wide { 0x40 } else { 0 };
    let mut terminal = vec![0u8; size];
    terminal[0] = 0xff;
    let mut bytes = vec![0u8; 8192];
    bytes[..18].copy_from_slice(b"DRACAD for Windows");
    bytes[0x22..0x24].copy_from_slice(&parts.marker.to_le_bytes());
    put(&mut bytes, 0x140 + shift, &[5.0, -6.0]);
    put(&mut bytes, 0x178 + shift, &[0.02]);
    put(&mut bytes, 0x188 + shift, &[420.0, 297.0, -100.0, 14850.0]);
    for record in parts.records.iter().chain([&terminal]) {
        assert_eq!(record.len(), size);
        bytes.extend_from_slice(record);
    }
    let mut sections = [bytes.len() as u32; 7];
    // Layer 3 is named and hidden; color 7, line type 2 and width 4 carry values.
    let (layer_row, color_row, name_at, rows) = if parts.wide {
        (552, 96, 32, [512usize, 258, 256, 256])
    } else {
        (64, 64, 32, [255, 255, 32, 16])
    };
    let first = usize::from(!parts.wide);
    let mut layers = vec![0u8; rows[0] * layer_row];
    let row = &mut layers[(3 - first) * layer_row..(4 - first) * layer_row];
    row[1] = 0x05;
    row[4] = 6;
    row[6] = 2;
    row[8] = 4;
    let name: Vec<u8> = if parts.wide {
        "\u{90e8}\u{54c1}"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect()
    } else {
        vec![0x95, 0x94, 0x95, 0x69]
    };
    row[name_at..name_at + name.len()].copy_from_slice(&name);
    let mut colors = vec![0u8; rows[1] * color_row];
    let at = (7 - first) * color_row;
    colors[at + 2..at + 7].copy_from_slice(&[35, 0, 10, 20, 30]);
    let mut line_types = vec![0u8; rows[2] * layer_row];
    let at = (2 - first) * layer_row;
    line_types[at + 2..at + 8].copy_from_slice(&[1, 0, 0x2c, 0x01, 0x64, 0x00]);
    let mut widths = vec![0u8; rows[3] * color_row];
    let at = (4 - first) * color_row;
    widths[at + 2..at + 4].copy_from_slice(&50u16.to_le_bytes());
    let extra = if parts.wide {
        [13824, 6400]
    } else {
        [3312, 4096]
    };
    let tables = [
        layers,
        colors,
        line_types,
        widths,
        vec![0; extra[0]],
        vec![0; extra[1]],
    ];
    for (index, table) in tables.iter().enumerate() {
        bytes.extend_from_slice(table);
        sections[index + 1] = bytes.len() as u32;
    }
    let (prefix, header_size, count_at) = if parts.wide {
        (1096, 656, 524)
    } else {
        (296, 176, 36)
    };
    bytes.resize(bytes.len() + prefix, 0);
    let block_start = if parts.blocks.is_empty() {
        0
    } else {
        bytes.len() as u32
    };
    for (name, records) in &parts.blocks {
        let mut header = vec![0u8; header_size];
        header[..name.len()].copy_from_slice(name);
        header[count_at..count_at + 4].copy_from_slice(&(records.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&header);
        for record in records.iter().chain([&terminal]) {
            bytes.extend_from_slice(record);
        }
    }
    if parts.wide {
        // The Unicode layout ends with a tail that holds no table rows.
        bytes.resize(bytes.len() + 1024, 0);
    } else if !parts.without_tail {
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

fn read_bytes(bytes: &[u8]) -> Result<BestEffortDrawing> {
    read_best_effort(bytes, &ParseLimits::default(), &BestEffortLimits::default())
}

fn read_parts(parts: &Parts) -> BestEffortDrawing {
    read_bytes(&build(parts)).unwrap()
}

fn read(shape: Shape, marker: u16, records: Vec<Vec<Record>>) -> BestEffortDrawing {
    read_parts(&Parts {
        marker,
        wide: shape.wide,
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

fn text_of(entity: &BestEffortEntity) -> &BestEffortText {
    match &entity.geometry {
        BestEffortGeometry::Text(text) => text,
        other => panic!("text expected, found {}", other.kind()),
    }
}

/// Entities with their record locations cleared, to compare two layouts.
fn shapes(entities: &[BestEffortEntity]) -> Vec<BestEffortEntity> {
    entities
        .iter()
        .map(|entity| {
            let mut entity = entity.clone();
            entity.source = BestEffortSource {
                table: BestEffortTable::Root,
                record_index: 0,
                payload_offset: 0,
                record_count: 0,
            };
            entity.attributes.tag_raw &= 0x00ff;
            if let BestEffortGeometry::Text(text) = &mut entity.geometry {
                text.text_bytes.clear();
            }
            if let BestEffortGeometry::Insert { name_bytes, .. } = &mut entity.geometry {
                name_bytes.clear();
            }
            entity.children = shapes(&entity.children);
            entity
        })
        .collect()
}

fn sample(shape: Shape) -> Vec<Vec<Record>> {
    let ring = |points: &[[f64; 2]]| -> Vec<Vec<Record>> {
        points
            .windows(2)
            .map(|pair| shape.line(0x0802, pair[0], pair[1]))
            .collect()
    };
    let square = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0], [0.0, 0.0]];
    let mut region = [0u8; 16];
    region[..3].copy_from_slice(&[0, 5, 6]);
    region[4..6].copy_from_slice(&1u16.to_le_bytes());
    region[6] = 40;
    region[12..14].copy_from_slice(&0x098du16.to_le_bytes());
    region[14..16].copy_from_slice(&2u16.to_le_bytes());
    let mut hatch = [0u8; 4];
    hatch[2..4].copy_from_slice(&0x098du16.to_le_bytes());
    let mut leader = vec![1u8, 0x80, 0, 7, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
    leader.extend_from_slice(&floats(&[1.0, 3.0, 100.0, 300.0]));
    let mut fill_tail = floats(&[7.0, 0.0, 360.0, 0.0]);
    fill_tail.extend_from_slice(&[0xfd, 0, 40, 0x0c, 10, 20, 30, 0]);
    vec![
        shape.line(2, [1.0, 2.0], [3.0, 4.0]),
        vec![
            shape.object(3, 2, &floats(&[5.0, 6.0])),
            shape.continuation(&fill_tail),
        ],
        shape.curve(4, &[8.0, 9.0], &[2.0, 30.0, 120.0]),
        shape.curve(7, &[1.5, 2.5, 4.0, 2.0], &[15.0, 0.0, 360.0]),
        shape.curve(8, &[1.5, 2.5, 4.0, 2.0], &[15.0, 10.0, 80.0]),
        shape.point([9.0, 8.0], 3),
        shape.text(5, "A{\\uni\"9ad9\"}\rB", 9),
        shape.group(10, &[0, 0x0d, 0xfd, 0, 0, 0, 25, 0, 1, 2, 3], ring(&square)),
        shape.group(10, &[3, 0, 0, 10], ring(&square[..4])),
        shape.group(
            12,
            &region,
            vec![shape.group(10, &[0, 1, 0, 0], ring(&square))],
        ),
        shape.group(
            13,
            &hatch,
            vec![vec![shape.attributed_part(0x4b, 0, &[1.0, 2.0, 0.5, 45.0])]],
        ),
        shape.link(
            shape.group(
                26,
                &[3, 0x80, 2, 5, 1, 5, 1, 19],
                vec![
                    shape.part(0x41, &[10.0, 0.0, 5.0, 8.0]),
                    shape.part(0x42, &[0.0, 0.0, 10.0, 0.0, 58.0]),
                    shape.part(0x43, &[10.0, 0.0]),
                    shape.part(0x43, &[5.0, 8.0]),
                    shape.part(0x44, &[100.0, 300.0, 100.0, 300.0]),
                    shape.link(shape.text(20, "58", 1), 104),
                    shape.part(0x50, &[1.0, 3.0, 1.0, 3.0]),
                ],
            ),
            104,
        ),
        shape.group(
            27,
            &leader,
            vec![
                shape.part(0x46, &[10.0, 10.0, 21.5, 30.0]),
                shape.part(0x46, &[21.5, 30.0, 60.0, 30.0]),
                shape.text(5, "NOTE", 0),
            ],
        ),
        shape.group(
            48,
            &[4, 0, 0, 0, 7, 0, 0, 0],
            vec![
                shape.line(2, [10.0, 10.0], [21.5, 30.0]),
                shape.arrowhead([10.0, 10.0]),
            ],
        ),
        shape.insert("SYM", 2, [100.0, 200.0]),
        {
            let mut records = vec![
                shape.object(25, 5, &[]),
                shape.continuation(&floats(&[335.0, 107.0, 0.0, 0.4, 0.4, 1.0])),
                shape.continuation(&floats(&[69.0, 49.0, 90.0])),
            ];
            records.extend(shape.names(&["photo.tif", "E:\\Photo\\"]));
            records
        },
        {
            let mut records = vec![
                shape.object(24, 5, &[]),
                shape.continuation(&floats(&[1.0, 2.0, 0.0, 1.0, 1.0, 1.0])),
                shape.continuation(&floats(&[0.0, 0.0, 30.0])),
            ];
            records.extend(shape.names(&["frame.mps", "D:\\Work\\", ".\\"]));
            records
        },
    ]
}

#[test]
fn every_decoded_kind_keeps_its_stored_values() {
    let drawing = read(NARROW, 22, sample(NARROW));
    assert!(!drawing.report.lossy(), "{:?}", drawing.report);
    assert_eq!((drawing.marker_raw, drawing.marker_observed), (22, true));
    assert_eq!(
        kinds(&drawing.entities),
        [
            "LINE",
            "CIRCLE",
            "ARC",
            "ELLIPSE",
            "ELLIPTICAL_ARC",
            "POINT",
            "TEXT",
            "POLYLINE",
            "POLYLINE",
            "EXTENDED_POLYGON",
            "HATCH",
            "DIMENSION",
            "LEADER",
            "PACKAGE",
            "INSERT",
            "IMAGE",
            "EXTERNAL_REFERENCE"
        ]
    );
    let entities = &drawing.entities;
    assert_eq!(
        entities[0].geometry,
        BestEffortGeometry::Line {
            start: [1.0, 2.0],
            end: [3.0, 4.0],
            elevation: 0.0,
            thickness: 0.0
        }
    );
    let attributes = &entities[0].attributes;
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
        entities[0].source,
        BestEffortSource {
            table: BestEffortTable::Root,
            record_index: 0,
            payload_offset: 8192,
            record_count: 1
        }
    );
    assert_eq!(
        entities[1].geometry,
        BestEffortGeometry::Circle {
            center: [5.0, 6.0],
            radius: 7.0,
            elevation: 0.0,
            fill: Some(BestEffortFill {
                color: 257,
                rgb: [10, 20, 30],
                transparency: 40,
                outline: true
            })
        }
    );
    assert_eq!(entities[2].source.record_index, 3);
    assert_eq!(
        entities[2].geometry,
        BestEffortGeometry::Arc {
            center: [8.0, 9.0],
            radius: 2.0,
            start_degrees: 30.0,
            end_degrees: 120.0,
            elevation: 0.0
        }
    );
    assert_eq!(
        entities[3].geometry,
        BestEffortGeometry::Ellipse {
            center: [1.5, 2.5],
            radius: [4.0, 2.0],
            rotation_degrees: 15.0,
            fill: None
        }
    );
    assert_eq!(
        entities[4].geometry,
        BestEffortGeometry::EllipticalArc {
            center: [1.5, 2.5],
            radius: [4.0, 2.0],
            rotation_degrees: 15.0,
            start_degrees: 10.0,
            end_degrees: 80.0
        }
    );
    assert_eq!(
        entities[5].geometry,
        BestEffortGeometry::Point {
            position: [9.0, 8.0],
            point_type: 3
        }
    );
    let text = text_of(&entities[6]);
    assert_eq!(text.text, "A\u{9ad9}\nB");
    assert_eq!(text.font_name, "Font");
    assert_eq!(
        (text.position, text.width, text.height, text.spacing),
        ([10.0, 20.0], 2.5, 4.0, 0.25)
    );
    assert_eq!(
        (text.rotation_degrees, text.slant_degrees, text.origin),
        (30.0, 5.0, 9)
    );
    assert_eq!(
        (
            text.flags_raw,
            text.charset,
            text.family,
            text.unit_raw,
            text.item_raw
        ),
        (37, 128, 49, 19, 1)
    );
    assert!(!text.numeric && text.number.is_none() && !text.lossy && !text.format_codes);
    assert_eq!(
        entities[7].geometry,
        BestEffortGeometry::Polyline {
            curve_kind: 0,
            flags_raw: 0x0d,
            divisions: 0,
            three_d: false,
            fill: Some(BestEffortFill {
                color: 257,
                rgb: [1, 2, 3],
                transparency: 25,
                outline: true
            }),
            curve_points: None
        }
    );
    assert_eq!(kinds(&entities[7].children), ["LINE"; 4]);
    assert!(entities[7].children[0].attributes.polyline_member());
    assert!(matches!(
        &entities[8].geometry,
        BestEffortGeometry::Polyline { curve_kind: 3, curve_points: Some(points), .. } if points.len() == 31
    ));
    assert_eq!(
        entities[9].geometry,
        BestEffortGeometry::ExtendedPolygon {
            polygon_kind: 0,
            flags_raw: 5,
            fill: Some(BestEffortFill {
                color: 6,
                rgb: [0; 3],
                transparency: 40,
                outline: true
            }),
            reference_number: 1,
            hole_count: 2,
            link_id: 0x098d
        }
    );
    assert_eq!(entities[9].children[0].children.len(), 4);
    assert_eq!(
        entities[10].geometry,
        BestEffortGeometry::Hatch {
            hatch_type: 0,
            link_id: 0x098d,
            lines: vec![BestEffortHatchLine {
                origin: [1.0, 2.0],
                pitch: 0.5,
                angle_degrees: 45.0,
                color: 7,
                line_type: 2,
                width: 4
            }],
            pattern: None
        }
    );
    assert_eq!(
        entities[11].geometry,
        BestEffortGeometry::Dimension {
            dimension_type: 3,
            flags_raw: 0x80,
            flags2_raw: 2,
            arrow_styles: [5, 1, 5, 1],
            text_position: 19,
            line: Some([[10.0, 0.0], [5.0, 8.0]]),
            arc: Some(BestEffortDimensionArc {
                center: [0.0, 0.0],
                radius: 10.0,
                start_degrees: 0.0,
                end_degrees: 58.0
            }),
            extension_origins: vec![[10.0, 0.0], [5.0, 8.0]],
            arrow_sizes: Some([100.0, 300.0, 100.0, 300.0]),
            arrow_paper_sizes: Some([1.0, 3.0, 1.0, 3.0])
        }
    );
    assert_eq!(entities[11].attributes.link_id, 104);
    let number = text_of(&entities[11].children[0]);
    assert!(number.numeric);
    assert_eq!((number.text.as_str(), number.number), ("58", Some(1234.5)));
    assert_eq!(entities[11].children[0].attributes.link_id, 104);
    assert_eq!(
        entities[12].geometry,
        BestEffortGeometry::Leader {
            leader_type: 1,
            flags_raw: 0x80,
            flags2_raw: 0,
            arrow_style: 7,
            lines: vec![[[10.0, 10.0], [21.5, 30.0]], [[21.5, 30.0], [60.0, 30.0]]],
            arrow_size: [100.0, 300.0],
            arrow_paper_size: [1.0, 3.0]
        }
    );
    assert_eq!(text_of(&entities[12].children[0]).text, "NOTE");
    assert_eq!(
        entities[13].geometry,
        BestEffortGeometry::Package {
            package_kind: 4,
            flags_raw: 0,
            reference_number: 7
        }
    );
    assert_eq!(kinds(&entities[13].children), ["LINE", "ARROWHEAD"]);
    assert_eq!(
        entities[13].children[1].geometry,
        BestEffortGeometry::Arrowhead {
            position: [10.0, 10.0],
            size: [1.0, 3.0],
            angle_degrees: 270.0,
            arrow_type: 5,
            flags_raw: 0x80
        }
    );
    assert_eq!(
        entities[14].geometry,
        BestEffortGeometry::Insert {
            name: "SYM".into(),
            name_bytes: b"SYM".to_vec(),
            reference_number: 2,
            position: [100.0, 200.0],
            elevation: -10.0,
            scale: [2.0, 3.0],
            rotation_degrees: 45.0
        }
    );
    assert_eq!(
        entities[15].geometry,
        BestEffortGeometry::Image {
            file_name: "photo.tif".into(),
            directory: "E:\\Photo\\".into(),
            position: [335.0, 107.0],
            elevation: 0.0,
            scale: [0.4, 0.4],
            size: [69.0, 49.0],
            rotation_degrees: 90.0
        }
    );
    assert_eq!(
        entities[16].geometry,
        BestEffortGeometry::ExternalReference {
            file_name: "frame.mps".into(),
            directory: "D:\\Work\\".into(),
            relative_directory: ".\\".into(),
            position: [1.0, 2.0],
            elevation: 0.0,
            scale: [1.0, 1.0],
            rotation_degrees: 30.0
        }
    );
    assert_eq!(drawing.report.decoded_counts["LINE"], 1 + 4 + 3 + 4 + 1);
    assert_eq!(drawing.report.decoded_counts["TEXT"], 3);
}

#[test]
fn the_unicode_layout_decodes_to_the_same_content() {
    let narrow = read(NARROW, 25, sample(NARROW));
    let wide = read(WIDE, 25, sample(WIDE));
    assert!(!wide.report.lossy(), "{:?}", wide.report);
    assert_eq!(shapes(&narrow.entities), shapes(&wide.entities));
    assert_eq!(narrow.report.decoded_counts, wide.report.decoded_counts);
    // Record locations follow the wider stride.
    assert_eq!(wide.entities[1].source.payload_offset, 8192 + 80);
    assert_eq!(wide.entities[2].source.record_index, 3);
    // Text is saved as UTF-16 code units; a character outside Shift_JIS needs no escape.
    let direct = read(WIDE, 25, vec![WIDE.text(5, "\u{9ad9}\u{2460} wide", 0)]);
    let text = text_of(&direct.entities[0]);
    assert_eq!(text.text, "\u{9ad9}\u{2460} wide");
    assert_eq!(text.text_bytes.len(), 2 * 7);
    let long = "x".repeat(70);
    let drawing = read(WIDE, 25, vec![WIDE.text(5, &long, 0)]);
    assert_eq!(text_of(&drawing.entities[0]).text, long);
    assert_eq!(drawing.entities[0].source.record_count, 3 + 3);
}

#[test]
fn unicode_tables_settings_and_blocks_are_read() {
    let name = |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let mut reference = WIDE.insert("alias", 0, [1.0, 2.0]);
    reference[0][12] = 3;
    reference.push(WIDE.plain(0x59, &[], &name("a long definition name")));
    let drawing = read_parts(&Parts {
        marker: 25,
        wide: true,
        records: reference,
        blocks: vec![
            (
                name("a long definition name"),
                WIDE.line(2, [0.0, 0.0], [1.0, 0.0]),
            ),
            (name("\u{90e8}\u{54c1}"), Vec::new()),
        ],
        ..Parts::default()
    });
    assert!(!drawing.report.lossy() && codes(&drawing).is_empty());
    assert!(matches!(
        &drawing.entities[0].geometry,
        BestEffortGeometry::Insert { name, name_bytes, .. }
            if name == "a long definition name" && *name_bytes == drawing.blocks[0].name_bytes
    ));
    assert_eq!(drawing.blocks.len(), 2);
    assert_eq!(drawing.blocks[1].name, "\u{90e8}\u{54c1}");
    let member = &drawing.blocks[0].entities[0];
    assert_eq!(member.source.table, BestEffortTable::Block(0));
    assert_eq!(
        member.source.payload_offset,
        drawing.blocks[0].header_offset + 656
    );
    assert_eq!(drawing.layers.len(), 257);
    let layer = &drawing.layers[3];
    assert_eq!(
        (
            layer.index,
            layer.name.as_str(),
            layer.hidden,
            layer.color,
            layer.line_type,
            layer.width
        ),
        (3, "\u{90e8}\u{54c1}", true, 6, 2, 4)
    );
    // The larger tables of this layout return the same index ranges.
    assert_eq!(drawing.colors.len(), 256);
    assert_eq!(
        (drawing.colors[6].index, drawing.colors[6].rgb),
        (7, [10, 20, 30])
    );
    assert_eq!((drawing.line_types.len(), drawing.widths.len()), (32, 16));
    assert_eq!(drawing.line_types[1].pattern, [3.0, 1.0]);
    assert_eq!((drawing.widths[3].index, drawing.widths[3].width), (4, 0.5));
    assert_eq!(
        drawing.settings,
        Some(BestEffortSettings {
            drawing_origin: [5.0, -6.0],
            drawing_scale: 50.0,
            paper_size: [420.0, 297.0],
            paper_origin: [-100.0, 14850.0]
        })
    );
}

#[test]
fn dependent_attributes_and_layout_membership_are_distinguished() {
    for shape in [NARROW, WIDE] {
        let mut records = Vec::new();
        for (tag, color, line_type, width, layer) in [
            (0x0402u16, 0u8, 0u8, 255u8, 0x0201u16),
            (0x2402, 0, 255, 0, 0x0100),
            (0x4402, 0, 1, 1, 0),
            (0x0002, 0, 32, 16, 5),
            (0x0a02, 9, 1, 1, 1),
        ] {
            let mut record = shape.line(tag, [0.0, 0.0], [1.0, 1.0]).remove(0);
            shape.style(&mut record, layer, color, line_type, width);
            records.push(vec![record]);
        }
        let drawing = read(shape, 25, records);
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
                (256, 32, 16, 5, 0),
                (9, 1, 1, 1, 0),
            ]
        );
        let last = &drawing.entities[4].attributes;
        assert!(last.hidden() && last.polyline_member() && !last.reversed_member());
        assert_eq!((last.tag_raw, last.flags_raw), (0x0a02, 0x0a));
        // The direction flag only counts on a member of a polyline.
        let flags = |flags_raw| BestEffortAttributes { flags_raw, ..*last };
        assert!(flags(0x18).reversed_member());
        assert!(!flags(0x10).reversed_member() && !flags(0x08).reversed_member());
    }
}

#[test]
fn text_bytes_are_decoded_per_generation() {
    // Two Shift_JIS characters, then an escaped character outside that set.
    let mut body = vec![0x90, 0x7d, 0x96, 0xca];
    body.extend_from_slice(b"{\\uni\"9ad9\"} A\rB");
    let old = read(
        NARROW,
        8,
        vec![NARROW.text_bytes(5, &body, &[0x82, 0x6c, 0x82, 0x72, 0], 9)],
    );
    let text = text_of(&old.entities[0]);
    assert_eq!(text.text, "\u{56f3}\u{9762}\u{9ad9} A\nB");
    assert_eq!(text.text_bytes, body);
    assert_eq!(text.font_name, "\u{ff2d}\u{ff33}");

    let font: Vec<u8> = "Wide".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let new = read(
        NARROW,
        25,
        vec![NARROW.text_bytes(20, &[b'x'; 100], &font, 1)],
    );
    let text = text_of(&new.entities[0]);
    assert_eq!((text.text.len(), text.font_name.as_str()), (100, "Wide"));
    assert_eq!(text.number, Some(1234.5));
    assert_eq!(new.entities[0].source.record_count, 6);

    let empty = read(NARROW, 5, vec![NARROW.text(5, "", 0)]);
    let text = text_of(&empty.entities[0]);
    assert!(text.text.is_empty() && text.text_bytes.is_empty());

    let styled = read(
        NARROW,
        22,
        vec![NARROW.text(
            5,
            "{\\FFont|128|49;\\H1.8;Head}{\\FFont|128|49;\\H1.5;\\nbody}",
            0,
        )],
    );
    let text = text_of(&styled.entities[0]);
    assert_eq!(text.text, "Head\nbody");
    assert!(text.format_codes);
}

#[test]
fn every_curve_kind_is_evaluated() {
    let members = |points: &[[f64; 2]]| -> Vec<Vec<Record>> {
        points
            .windows(2)
            .map(|pair| NARROW.line(0x0802, pair[0], pair[1]))
            .collect()
    };
    let points = [
        [0.0, 0.0],
        [10.0, 20.0],
        [30.0, 25.0],
        [50.0, 5.0],
        [70.0, 30.0],
    ];
    let mut closed = points[..4].to_vec();
    closed.push(points[0]);
    let mut broken = members(&points[..3]);
    broken.push(NARROW.line(0x0802, [9.0, 9.0], [8.0, 8.0]));
    let drawing = read(
        NARROW,
        5,
        vec![
            NARROW.group(10, &[2, 0, 0, 10], members(&points)),
            NARROW.group(10, &[2, 1, 0, 10], members(&closed)),
            NARROW.group(10, &[3, 0, 0, 10], members(&points)),
            NARROW.group(10, &[3, 1, 0, 5], members(&closed)),
            NARROW.group(10, &[3, 0x10, 0, 10], members(&points)),
            NARROW.group(10, &[3, 0x11, 0, 10], members(&closed)),
            NARROW.group(10, &[4, 0, 0, 10], members(&points)),
            NARROW.group(10, &[1, 0, 0, 0], members(&points)),
            NARROW.group(10, &[9, 0, 0, 10], members(&points)),
            NARROW.group(10, &[3, 0, 0, 10], broken),
            NARROW.group(10, &[3, 0, 0, 0], members(&points)),
            NARROW.group(18, &[0, 0, 0, 0], vec![]),
        ],
    );
    let curve = |index: usize| -> Option<&Vec<[f64; 2]>> {
        match &drawing.entities[index].geometry {
            BestEffortGeometry::Polyline { curve_points, .. } => curve_points.as_ref(),
            other => panic!("polyline expected, found {}", other.kind()),
        }
    };
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
    // Quadratic: three spans, the first around the second point.
    let quadratic = curve(0).unwrap();
    assert_eq!(quadratic.len(), 3 * 10 + 1);
    assert!(near(quadratic[1], [2.0, 3.825]) && near(quadratic[30], points[4]));
    assert_eq!(curve(1).unwrap().len(), 4 * 10 + 1);
    // Cubic: approaches its interior points.
    let cubic = curve(2).unwrap();
    assert_eq!(cubic.len(), 4 * 10 + 1);
    assert!(near(cubic[0], points[0]) && !near(cubic[10], points[1]));
    assert_eq!(curve(3).unwrap().len(), 4 * 5 + 1);
    // Through its points.
    let through = curve(4).unwrap();
    assert!(near(through[10], points[1]) && near(through[30], points[3]));
    let ring = curve(5).unwrap();
    assert!(near(ring[10], points[1]) && near(ring[40], points[0]));
    // Bezier: anchors and handles alternate.
    let bezier = curve(6).unwrap();
    assert_eq!(bezier.len(), 2 * 10 + 1);
    assert!(near(bezier[1], [2.73, 5.425]) && near(bezier[10], points[2]));
    // A saved evaluation, an unknown kind, unconnected members and no divisions.
    for index in 7..11 {
        assert!(curve(index).is_none(), "{index}");
    }
    assert!(matches!(
        drawing.entities[11].geometry,
        BestEffortGeometry::Polyline { three_d: true, .. }
    ));
    assert_eq!(
        codes(&drawing),
        ["curve_kind_not_evaluated", "curve_points_unusable"]
    );
    assert!(!drawing.report.lossy());
}

#[test]
fn fills_follow_the_generation() {
    let circle = |shape: Shape, fill: [u8; 8]| -> Vec<Record> {
        let mut tail = floats(&[5.0, 0.0, 360.0, 0.0]);
        tail.extend_from_slice(&fill);
        vec![
            shape.object(3, 2, &floats(&[0.0, 0.0])),
            shape.continuation(&tail),
        ]
    };
    let fill_of =
        |drawing: &BestEffortDrawing, index: usize| match &drawing.entities[index].geometry {
            BestEffortGeometry::Circle { fill, .. } | BestEffortGeometry::Polyline { fill, .. } => {
                *fill
            }
            other => panic!("filled shape expected, found {}", other.kind()),
        };
    let records = |shape: Shape| {
        vec![
            circle(shape, [0; 8]),
            circle(shape, [4, 0, 0, 0x04, 0, 0, 0, 0]),
            circle(shape, [0xfd, 0, 1, 0x0c, 10, 20, 30, 0]),
            circle(shape, [0xfd, 0, 0, 0x2c, 255, 255, 255, 0]),
            // An explicit color may be marked by the flag alone.
            circle(shape, [0, 0, 0, 0x0c, 54, 92, 60, 0]),
            circle(shape, [6, 0, 2, 0x06, 0, 0, 0, 0]),
            shape.group(10, &[0, 0x05, 0, 0], vec![]),
            shape.group(10, &[0, 0x01, 9, 0], vec![]),
        ]
    };
    for shape in [NARROW, WIDE] {
        let drawing = read(shape, 25, records(shape));
        let fill = |color, rgb, transparency, outline| {
            Some(BestEffortFill {
                color,
                rgb,
                transparency,
                outline,
            })
        };
        assert_eq!(fill_of(&drawing, 0), None);
        assert_eq!(fill_of(&drawing, 1), fill(4, [0; 3], 0, true));
        assert_eq!(fill_of(&drawing, 2), fill(257, [10, 20, 30], 1, true));
        assert_eq!(fill_of(&drawing, 3), fill(258, [255; 3], 0, true));
        assert_eq!(fill_of(&drawing, 4), fill(257, [54, 92, 60], 0, true));
        assert_eq!(fill_of(&drawing, 5), fill(6, [0; 3], 2, false));
        assert_eq!(fill_of(&drawing, 6), fill(256, [0; 3], 0, true));
        // A fill color without the fill flag does not fill ...
        assert_eq!(fill_of(&drawing, 7), None);
    }
    // ... except in the oldest generations, which have no such flag.
    let old = read(
        NARROW,
        4,
        vec![
            circle(NARROW, [4, 0, 0, 0, 0, 0, 0, 0]),
            NARROW.group(10, &[0, 0x01, 9, 0], vec![]),
            circle(NARROW, [0; 8]),
        ],
    );
    assert_eq!(fill_of(&old, 0).map(|fill| fill.color), Some(4));
    assert_eq!(fill_of(&old, 1).map(|fill| fill.color), Some(9));
    assert_eq!(fill_of(&old, 2), None);
}

#[test]
fn named_hatch_patterns_keep_their_name_and_numbers() {
    for shape in [NARROW, WIDE] {
        let mut body = [1u8, 0, 0, 0];
        body[2..4].copy_from_slice(&0x05b1u16.to_le_bytes());
        let mut name = shape.encode("Insulation 2");
        name.resize(shape.size() - 16, 0);
        let drawing = read(
            shape,
            10,
            vec![shape.group(
                13,
                &body,
                vec![vec![
                    shape.attributed_part(0x4c, 3, &[3.0, 4.0, 0.38, 360.0, 1.0, 90.0]),
                    shape.plain(0x4d, &[], &floats(&[0.005, 0.005, 360.0])),
                    shape.plain(0x4e, &[], &name),
                    shape.plain(0x4f, &[], &[]),
                ]],
            )],
        );
        assert_eq!(
            drawing.entities[0].geometry,
            BestEffortGeometry::Hatch {
                hatch_type: 1,
                link_id: 0x05b1,
                lines: Vec::new(),
                pattern: Some(BestEffortHatchPattern {
                    name: "Insulation 2".into(),
                    origin: [3.0, 4.0],
                    values: vec![0.38, 360.0, 1.0, 90.0, 0.005, 0.005, 360.0],
                    color: 7,
                    line_type: 2,
                    width: 4
                })
            }
        );
        assert_eq!(codes(&drawing), ["hatch_parts_ignored"]);
        assert!(!drawing.report.lossy());
    }
}

#[test]
fn block_definitions_and_references_are_read() {
    let mut long = NARROW.insert("alias", 0, [1.0, 2.0]);
    long[0][8] = 3;
    long.push(NARROW.plain(0x59, &[], b"a long definition name"));
    let mut records = NARROW.insert("SYM", 0, [100.0, 200.0]);
    records.extend(long);
    let drawing = read_parts(&Parts {
        marker: 5,
        records,
        blocks: vec![
            (b"SYM".to_vec(), NARROW.line(2, [0.0, 0.0], [1.0, 0.0])),
            (vec![0x95, 0x94, 0x95, 0x69], Vec::new()),
        ],
        ..Parts::default()
    });
    assert!(matches!(
        &drawing.entities[0].geometry,
        BestEffortGeometry::Insert { name, reference_number: 0, .. } if name == "SYM"
    ));
    assert!(matches!(
        &drawing.entities[1].geometry,
        BestEffortGeometry::Insert { name, .. } if name == "a long definition name"
    ));
    assert_eq!(drawing.entities[1].source.record_count, 3);
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
    let drawing = read(NARROW, 8, vec![NARROW.line(2, [1.0, 2.0], [3.0, 4.0])]);
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
    let mut broken_line = NARROW.line(2, [0.0, 0.0], [1.0, 1.0]);
    broken_line[0][32..40].copy_from_slice(&f64::NAN.to_le_bytes());
    let mut lone_circle = NARROW.curve(3, &[0.0, 0.0], &[1.0]);
    lone_circle.truncate(1);
    lone_circle[0][8] = 1;
    let mut short_text = NARROW.text(5, "abc", 0);
    short_text[1][16] = 200;
    let property = vec![NARROW.object(0x33, 2, &[]), NARROW.continuation(&[])];
    let drawing = read(
        NARROW,
        5,
        vec![
            property,
            vec![NARROW.object(0x16, 1, &[])],
            NARROW.group(0x1d, &[], vec![NARROW.line(2, [0.0, 0.0], [1.0, 1.0])]),
            broken_line,
            lone_circle,
            NARROW.curve(4, &[0.0, 0.0], &[-1.0, 0.0, 90.0]),
            short_text,
            vec![NARROW.object(25, 1, &[])],
            NARROW.line(2, [7.0, 7.0], [8.0, 8.0]),
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
        [(&0x16, &1), (&0x1d, &1)]
    );
    assert_eq!(
        report.failed_counts.iter().collect::<Vec<_>>(),
        [(&2, &1), (&3, &1), (&4, &1), (&5, &1), (&25, &1)]
    );
    assert_eq!(codes(&drawing), ["object_layout_mismatch"; 5]);
    assert_eq!(report.diagnostics[0].record_index, Some(5));
    assert_eq!(report.diagnostics[0].tag_raw, Some(2));
    assert_eq!(report.diagnostics[0].table, Some(BestEffortTable::Root));
    assert_eq!(report.decoded_counts.get("LINE"), Some(&1));
}

#[test]
fn stray_continuations_and_overlong_objects_are_reported() {
    let mut overlong = NARROW.group(
        10,
        &[0, 0, 0, 0],
        vec![NARROW.line(0x0802, [0.0, 0.0], [1.0, 0.0])],
    );
    overlong[0][8] = 9;
    let drawing = read(
        NARROW,
        5,
        vec![
            vec![NARROW.continuation(&[])],
            NARROW.line(2, [0.0, 0.0], [1.0, 1.0]),
            vec![NARROW.continuation(&[])],
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
fn every_saved_generation_is_known_and_others_are_read_with_a_note() {
    for marker in [3, 4, 5, 8, 9, 10, 12, 13, 14, 15, 17, 19, 20, 22, 25] {
        let drawing = read(NARROW, marker, vec![NARROW.line(2, [0.0, 0.0], [1.0, 1.0])]);
        assert!(
            drawing.marker_observed && codes(&drawing).is_empty(),
            "{marker}"
        );
    }
    let drawing = read(NARROW, 77, vec![NARROW.line(2, [0.0, 0.0], [1.0, 1.0])]);
    assert!(!drawing.marker_observed);
    assert_eq!(codes(&drawing), ["generation_not_observed"]);
    assert_eq!(drawing.entities.len(), 1);
    assert!(!drawing.report.lossy());

    let flat = read_parts(&Parts {
        marker: 8,
        records: NARROW.line(2, [0.0, 0.0], [1.0, 1.0]),
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
        marker: 4,
        records: NARROW.line(2, [0.0, 0.0], [1.0, 1.0]),
        without_tail: true,
        ..Parts::default()
    });
    assert_eq!(drawing.entities.len(), 1);
    assert_eq!((drawing.layers.len(), drawing.colors.len()), (255, 255));
    assert!(codes(&drawing).is_empty() && !drawing.report.lossy());
}

#[test]
fn budgets_are_errors() {
    let limited = |bytes: &[u8], limits: BestEffortLimits| {
        read_best_effort(bytes, &ParseLimits::default(), &limits)
    };
    let records: Vec<Record> = (0..4)
        .flat_map(|_| NARROW.line(2, [0.0, 0.0], [1.0, 1.0]))
        .collect();
    let bytes = build(&Parts {
        marker: 5,
        records,
        ..Parts::default()
    });
    assert!(matches!(
        limited(
            &bytes,
            BestEffortLimits {
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

    let nested = NARROW.group(
        48,
        &[],
        vec![NARROW.group(48, &[], vec![NARROW.group(48, &[], vec![])])],
    );
    let bytes = build(&Parts {
        marker: 5,
        records: nested,
        ..Parts::default()
    });
    assert!(matches!(
        limited(
            &bytes,
            BestEffortLimits {
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

    let spline = NARROW.group(
        10,
        &[3, 0, 0, 200],
        vec![
            NARROW.line(0x0802, [0.0, 0.0], [1.0, 0.0]),
            NARROW.line(0x0802, [1.0, 0.0], [1.0, 1.0]),
        ],
    );
    let bytes = build(&Parts {
        marker: 5,
        records: spline,
        ..Parts::default()
    });
    assert!(matches!(
        limited(
            &bytes,
            BestEffortLimits {
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
        limited(
            &bytes,
            BestEffortLimits {
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

    let noisy: Vec<Record> = (0..5)
        .flat_map(|_| NARROW.curve(4, &[0.0, 0.0], &[-1.0]))
        .collect();
    let bytes = build(&Parts {
        marker: 5,
        records: noisy,
        ..Parts::default()
    });
    let drawing = limited(
        &bytes,
        BestEffortLimits {
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
        records: NARROW.line(2, [0.0, 0.0], [1.0, 1.0]),
        ..Parts::default()
    });
    let mutate = |change: &dyn Fn(&mut Vec<u8>)| {
        let mut bytes = good.clone();
        change(&mut bytes);
        read_bytes(&bytes)
    };
    assert!(read_bytes(&good).is_ok());
    for result in [
        mutate(&|bytes| bytes[0] = b'X'),
        mutate(&|bytes| bytes[0x58] = 7),
        mutate(&|bytes| bytes[8192 + 64] = 0x02),
        mutate(&|bytes| bytes.truncate(bytes.len() - 1)),
        mutate(&|bytes| bytes[0x2c..0x30].copy_from_slice(&1u32.to_le_bytes())),
        mutate(&|bytes| bytes[0x4c] = 1),
        read_bytes(&good[..0x40]),
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
    assert_eq!(codes(&resized), ["style_tables_not_read"]);
    // A block table that does not start with a definition is noted.
    let odd = build(&Parts {
        marker: 5,
        blocks: vec![(b"B".to_vec(), Vec::new())],
        ..Parts::default()
    });
    let start = u32::from_le_bytes(odd[0x4c..0x50].try_into().unwrap()) as usize;
    let mut bytes = odd.clone();
    bytes[start] = 0;
    let drawing = read_bytes(&bytes).unwrap();
    assert!(drawing.blocks.is_empty());
    assert_eq!(codes(&drawing), ["block_definitions_not_read"]);
}

fn compound(clsid: &str, stream: &str, contents: &[u8]) -> Vec<u8> {
    let mut compound = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    compound
        .set_storage_clsid("/", clsid.parse().unwrap())
        .unwrap();
    compound
        .create_stream(stream)
        .unwrap()
        .write_all(contents)
        .unwrap();
    compound.flush().unwrap();
    compound.into_inner().into_inner()
}

/// The compressed envelope of the security formats: kind 0 and 1 for the
/// older layout, 2 and 3 for the Unicode one; odd kinds carry an extension.
fn envelope(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(payload).unwrap();
    let encoded = encoder.finish().unwrap();
    let mut bytes = vec![0u8; 122];
    for at in [0, 4, 8] {
        bytes[at] = 1;
    }
    bytes[18] = kind;
    if kind & 1 != 0 {
        bytes.extend_from_slice(&[
            0, 0, 0, 0, 0x18, 0xfc, 0xff, 0xff, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x24, 0x40, 1, 0, 0,
            0,
        ]);
    }
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&encoded);
    bytes
}

#[test]
fn compound_containers_are_identified_by_content() {
    let payload = build(&Parts {
        marker: 5,
        records: NARROW.line(2, [0.0, 0.0], [1.0, 1.0]),
        ..Parts::default()
    });
    let drawing = read_bytes(&compound(crate::DRACAD_CLSID, "/Contents", &payload)).unwrap();
    assert_eq!(drawing.format, DracadFormat::Mpz);
    assert_eq!(drawing.entities.len(), 1);
    assert_eq!(drawing.payload_size, payload.len() as u64);
    for bytes in [
        compound(
            "00000000-0000-0000-0000-000000000001",
            "/Contents",
            &payload,
        ),
        compound(crate::DRACAD_CLSID, "/Other", &payload),
        compound(crate::DRACAD_CLSID, "/SecureContents", &payload),
    ] {
        assert!(matches!(
            read_bytes(&bytes),
            Err(DracadError::UnsupportedFormat(_))
        ));
    }
    assert!(matches!(
        read_bytes(b"not a drawing"),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn compressed_envelopes_of_both_layouts_are_read() {
    let narrow = build(&Parts {
        marker: 22,
        records: sample(NARROW).into_iter().flatten().collect(),
        ..Parts::default()
    });
    let wide = build(&Parts {
        marker: 25,
        wide: true,
        records: sample(WIDE).into_iter().flatten().collect(),
        ..Parts::default()
    });
    for (kind, payload, format) in [
        (0, &narrow, DracadFormat::Mps),
        (1, &narrow, DracadFormat::Mps),
        (2, &wide, DracadFormat::Mpuw),
        (3, &wide, DracadFormat::Mpuw),
    ] {
        let file = compound(
            crate::DRACAD_CLSID,
            "/SecureContents",
            &envelope(kind, payload),
        );
        let drawing = read_bytes(&file).unwrap();
        assert_eq!(drawing.format, format, "{kind}");
        assert_eq!(drawing.entities.len(), 17);
        assert_eq!(drawing.payload_size, payload.len() as u64);
        assert!(!drawing.report.lossy());
    }
    // The other entry points do not decode the Unicode envelopes.
    let file = compound(crate::DRACAD_CLSID, "/SecureContents", &envelope(3, &wide));
    let info = crate::inspect(&file, &ParseLimits::default()).unwrap();
    assert_eq!(info.format, DracadFormat::MpsCandidate);
    assert!(info.payload.is_none());
    // An unknown envelope kind and a damaged stream stay unread.
    let unknown = compound(crate::DRACAD_CLSID, "/SecureContents", &envelope(4, &wide));
    assert!(matches!(
        read_bytes(&unknown),
        Err(DracadError::UnsupportedFormat(_))
    ));
    let mut damaged = envelope(2, &wide);
    let last = damaged.len() - 1;
    damaged[last] ^= 0xff;
    let damaged = compound(crate::DRACAD_CLSID, "/SecureContents", &damaged);
    assert!(matches!(
        read_bytes(&damaged),
        Err(DracadError::InvalidFormat(_))
    ));
    assert!(matches!(
        read_best_effort(
            &file,
            &ParseLimits {
                max_decompressed_size: 1000,
                ..ParseLimits::default()
            },
            &BestEffortLimits::default()
        ),
        Err(DracadError::LimitExceeded {
            resource: "decompressed_size",
            ..
        })
    ));
}
