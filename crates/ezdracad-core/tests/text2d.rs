//! Synthetic root-membership and hostile-field checks, not native evidence.
use ezdracad_core::{root_text_2d, scan_contents, DracadError, ParseLimits, TextLimits};

const START: usize = 8192;

fn text() -> Vec<u8> {
    let mut bytes = vec![0; 256];
    bytes[..16].copy_from_slice(&[5, 0, 1, 0, 1, 1, 1, 0, 4, 0, 1, 0, 0, 1, 0, 0]);
    for (at, value) in [(16, 3.125_f64), (24, -7.5), (40, 3.0), (48, 3.0)] {
        bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes[64] = 0xfe;
    bytes[72] = 1;
    bytes[76] = 1;
    bytes[80] = 11;
    bytes[82] = 8;
    bytes[128] = 0xfe;
    bytes[130] = 19;
    bytes[134] = 53;
    bytes[136] = 1;
    let font: Vec<u8> = "ＭＳ ゴシック"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    bytes[160..160 + font.len()].copy_from_slice(&font);
    bytes[192..198].copy_from_slice(&[0xfe, 0, 44, 46, 3, 2]);
    bytes[208..219].copy_from_slice(b"P5_ASCII_01");
    bytes
}

fn contents(records: &[u8]) -> Vec<u8> {
    assert_eq!(records.len() % 64, 0);
    let template = include_bytes!("../../../tests/fixtures/dimension25.bin");
    let old_end = u32::from_le_bytes(template[0x28..0x2c].try_into().unwrap()) as usize;
    let new_end = START + records.len() + 64;
    let delta = new_end as i64 - old_end as i64;
    let mut result = template[..START].to_vec();
    result.extend_from_slice(records);
    result.push(0xff);
    result.resize(new_end, 0);
    result.extend_from_slice(&template[old_end..]);
    for at in (0x28..0x44).step_by(4).chain([0x44, 0x48, 0x4c, 0x50]) {
        let old = u32::from_le_bytes(template[at..at + 4].try_into().unwrap());
        if old != 0 {
            result[at..at + 4].copy_from_slice(&((i64::from(old) + delta) as u32).to_le_bytes());
        }
    }
    result[0x58..0x5c].copy_from_slice(&((records.len() / 64) as u32).to_le_bytes());
    result
}

fn decode(records: &[u8], index: u64) -> ezdracad_core::Result<ezdracad_core::TextRecord2D> {
    let raw = scan_contents(&contents(records), &ParseLimits::default()).unwrap();
    root_text_2d(&raw, index, &TextLimits::default())
}

#[test]
fn owned_sources_survive_document_and_nonzero_root_index() {
    let mut records = text();
    records.extend_from_slice(&text());
    let value = decode(&records, 4).unwrap();
    assert_eq!(value.record_index, 4);
    assert_eq!(value.payload_offset, (START + 256) as u64);
    assert_eq!(value.raw_bytes, records[256..]);
    assert_eq!(value.text, "P5_ASCII_01");
    assert_eq!(value.serialized_text.as_bytes(), value.body_bytes);
    assert_eq!(value.basepoint, [3.125, -7.5]);
    assert_eq!(value.font_name, "ＭＳ ゴシック");
    assert_eq!(value.coordinate_scope(), "root_records");
    assert_eq!(value.units(), None);
}

#[test]
fn interiors_terminals_absent_and_embedded_text_are_not_root_entities() {
    for index in [1, 2, 3, 4, 5, u64::MAX] {
        assert!(matches!(
            decode(&text(), index),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let mut records = vec![0; 64];
    records[0] = 0x30;
    records[8] = 5;
    records.extend_from_slice(&text());
    assert!(matches!(
        decode(&records, 1),
        Err(DracadError::UnsupportedFeature(_))
    ));
}

#[test]
fn older_marker_numeric_text_unknown_flags_and_nonfinite_fields_stay_raw() {
    for tag in [0x0014_u16, 0x0405, 0x0805, 0x2005] {
        let mut bytes = text();
        bytes[..2].copy_from_slice(&tag.to_le_bytes());
        assert!(matches!(
            decode(&bytes, 0),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    for offset in [16, 24, 32, 40, 48, 56, 144] {
        let mut bytes = text();
        bytes[offset..offset + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(decode(&bytes, 0).is_err(), "field +{offset}");
        bytes[134] = 0;
        assert!(matches!(
            decode(&bytes, 0),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let mut data = contents(&text());
    data[34] = 8;
    let raw = scan_contents(&data, &ParseLimits::default()).unwrap();
    assert!(matches!(
        root_text_2d(&raw, 0, &TextLimits::default()),
        Err(DracadError::UnsupportedFeature(_))
    ));
    data[34] = 5;
    // Marker5 has a different table length: changing just its marker is held by
    // the raw layout validator before any TEXT interpretation is attempted.
    assert!(scan_contents(&data, &ParseLimits::default()).is_err());
}
