//! Bounded marker25 root TEXT literals. The body is the saved native string,
//! not a font layout, rendered outline, physical unit, or WCS claim.

use crate::limits::check_limit;
use crate::preflight::invalid;
use crate::{DracadError, ObservedProfile, RawDocument, RawObjectSpan, Result};

/// Limits for saved string bytes and decoded UTF-8 bytes, checked before copying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLimits {
    pub max_encoded_bytes: u64,
    pub max_output_bytes: u64,
}

impl Default for TextLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: u16::MAX.into(),
            max_output_bytes: 262_144,
        }
    }
}

/// An owned, planar root TEXT with its serialized bytes retained.
///
/// `body_bytes` excludes the NUL terminator and includes native Unicode escape
/// syntax. `text` expands only the observed BMP Unicode escapes. Native text
/// codes, styling, multiline layout, numeric dimensions, and rendering remain
/// outside this contract. Size values are saved values, without physical units.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRecord2D {
    pub record_index: u64,
    pub payload_offset: u64,
    pub raw_bytes: Vec<u8>,
    pub basepoint: [f64; 2],
    pub width: f64,
    pub height: f64,
    pub spacing: f64,
    pub rotation_degrees: f64,
    pub origin: u16,
    pub font_name: String,
    /// UTF-16LE name bytes excluding the two-byte terminator.
    pub font_bytes: Vec<u8>,
    pub serialized_text: String,
    pub body_bytes: Vec<u8>,
    pub text: String,
    pub text_flags: [u8; 2],
    pub charset: u8,
    pub family: u8,
    pub scale_flag: u32,
}

impl TextRecord2D {
    pub const fn coordinate_scope(&self) -> &'static str {
        "root_records"
    }
    pub const fn units(&self) -> Option<&'static str> {
        None
    }
    pub const fn body_encoding(&self) -> &'static str {
        "ascii_native_bmp_unicode_escapes"
    }
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(format!("root TEXT: {message}"))
}

fn number(bytes: &[u8], offset: usize) -> Result<f64> {
    let value = f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid("nonfinite root TEXT field"))
    }
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn literal(bytes: &[u8], limits: &TextLimits) -> Result<String> {
    let mut result = String::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let (value, consumed) = if bytes[cursor] == b'{' {
            let escape = bytes
                .get(cursor..cursor + 12)
                .ok_or_else(|| unsupported("unknown or incomplete native text escape"))?;
            if &escape[..6] != b"{\\uni\"" || &escape[10..] != b"\"}" {
                return Err(unsupported(
                    "formatting or dynamic text code is not qualified",
                ));
            }
            let mut scalar = 0_u32;
            for digit in &escape[6..10] {
                let value = match digit {
                    b'0'..=b'9' => digit - b'0',
                    b'a'..=b'f' => digit - b'a' + 10,
                    b'A'..=b'F' => digit - b'A' + 10,
                    _ => return Err(unsupported("invalid native Unicode escape")),
                };
                scalar = scalar * 16 + u32::from(value);
            }
            let value = char::from_u32(scalar)
                .filter(|value| !value.is_control())
                .ok_or_else(|| {
                    unsupported("surrogate or control Unicode escape is not qualified")
                })?;
            (value, 12)
        } else {
            let value = bytes[cursor];
            if !(0x20..=0x7e).contains(&value) || value == b'}' {
                return Err(unsupported(
                    "non-ASCII body, multiline, or unknown escape is not qualified",
                ));
            }
            (char::from(value), 1)
        };
        check_limit(
            "text_output_bytes",
            result.len() as u64 + value.len_utf8() as u64,
            limits.max_output_bytes,
        )?;
        result.push(value);
        cursor += consumed;
    }
    Ok(result)
}

fn decode_bytes(
    bytes: &[u8],
    record_index: u64,
    payload_offset: u64,
    limits: &TextLimits,
) -> Result<TextRecord2D> {
    if bytes.len() < 256 || bytes.len() % 64 != 0 {
        return Err(unsupported("unqualified TEXT span length"));
    }
    if word(bytes, 0) != 0x0005 || usize::from(word(bytes, 8)) != bytes.len() / 64 {
        return Err(unsupported("unqualified ordinary TEXT tag or span"));
    }
    // Known numeric fields remain fatal when an unsupported style/layout state
    // later fences interpretation. They must never become partial reads.
    let basepoint = [number(bytes, 16)?, number(bytes, 24)?];
    let elevation = number(bytes, 32)?;
    let width = number(bytes, 40)?;
    let height = number(bytes, 48)?;
    let spacing = number(bytes, 56)?;
    let rotation_degrees = number(bytes, 144)?;
    if bytes[12..16] != [0, 1, 0, 0] {
        return Err(unsupported("text id, style, or 3D state is not qualified"));
    }
    for slot in bytes[64..].chunks_exact(64) {
        if word(slot, 0) != 0xfe {
            return Err(invalid("TEXT continuation tag is not FE"));
        }
    }
    let first = &bytes[64..128];
    let font = &bytes[128..192];
    let body = &bytes[192..];
    if first[2..8].iter().any(|value| *value != 0)
        || first[8..16] != [1, 0, 0, 0, 1, 0, 0, 0]
        || first[20..].iter().any(|value| *value != 0)
    {
        return Err(unsupported(
            "multiline, scaling, or unknown text continuation state",
        ));
    }
    let encoded_length = usize::from(word(first, 16));
    if encoded_length == 0 {
        return Err(unsupported(
            "empty TEXT body has no qualified native geometry",
        ));
    }
    check_limit(
        "text_encoded_bytes",
        encoded_length as u64,
        limits.max_encoded_bytes,
    )?;
    let body_slots = (encoded_length + 1).div_ceil(48);
    if body_slots * 64 != body.len() {
        return Err(invalid("TEXT length and continuation span disagree"));
    }
    if body[..16] != [0xfe, 0, 44, 46, 3, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
        return Err(unsupported("numeric or unknown TEXT body state"));
    }
    for slot in body.chunks_exact(64).skip(1) {
        if slot[2..16].iter().any(|value| *value != 0) {
            return Err(unsupported("unknown TEXT body continuation state"));
        }
    }
    if font[2..16] != [19, 0, 0, 0, 53, 0, 1, 0, 0, 0, 0, 0, 0, 0]
        || font[24..32].iter().any(|value| *value != 0)
    {
        return Err(unsupported(
            "font, formatting, unit, or slant state is not qualified",
        ));
    }
    let origin = word(first, 18);
    if origin != 8 {
        return Err(unsupported("unqualified TEXT insertion origin"));
    }
    if elevation != 0.0 {
        return Err(unsupported("nonzero TEXT Z is not qualified"));
    }
    if width <= 0.0 || height <= 0.0 || spacing != 0.0 {
        return Err(unsupported(
            "nonpositive text size or nonzero spacing is not qualified",
        ));
    }
    let font_units: Vec<u16> = font[32..]
        .chunks_exact(2)
        .map(|pair| word(pair, 0))
        .collect();
    let font_end = font_units
        .iter()
        .position(|value| *value == 0)
        .ok_or_else(|| invalid("TEXT font is not terminated UTF-16LE"))?;
    if font_end == 0 || font_units[font_end..].iter().any(|value| *value != 0) {
        return Err(unsupported(
            "empty font or nonzero font padding is not qualified",
        ));
    }
    let font_name = String::from_utf16(&font_units[..font_end])
        .map_err(|_| invalid("invalid UTF-16LE font name"))?;
    let mut body_bytes = Vec::with_capacity(encoded_length);
    let mut seen = 0;
    for slot in body.chunks_exact(64) {
        for value in &slot[16..] {
            if seen < encoded_length {
                if *value == 0 {
                    return Err(invalid("TEXT body terminates before declared byte length"));
                }
                body_bytes.push(*value);
            } else if *value != 0 {
                return Err(invalid("TEXT body terminator or padding is nonzero"));
            }
            seen += 1;
        }
    }
    let text = literal(&body_bytes, limits)?;
    let serialized_text = String::from_utf8(body_bytes.clone())
        .map_err(|_| unsupported("unqualified body encoding"))?;
    Ok(TextRecord2D {
        record_index,
        payload_offset,
        raw_bytes: bytes.to_vec(),
        basepoint,
        width,
        height,
        spacing,
        rotation_degrees,
        origin,
        font_name,
        font_bytes: font[32..32 + font_end * 2].to_vec(),
        serialized_text,
        body_bytes,
        text,
        text_flags: [font[6], font[7]],
        charset: font[8],
        family: font[9],
        scale_flag: 1,
    })
}

/// Decode a span already validated as a primary in the root table.
pub(crate) fn decode_root_text(
    raw: &RawDocument,
    span: &RawObjectSpan,
    limits: &TextLimits,
) -> Result<TextRecord2D> {
    if raw.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported("requires observed marker25"));
    }
    // Limit the span before any owned copies. The saved length field is u16.
    if span.record_count > 3 + (u64::from(u16::MAX) + 1).div_ceil(48) {
        return Err(unsupported("TEXT span exceeds saved length range"));
    }
    let at = usize::try_from(span.payload_offset).map_err(|_| invalid("TEXT offset too large"))?;
    let length = usize::try_from(
        span.record_count
            .checked_mul(64)
            .ok_or_else(|| invalid("TEXT span overflow"))?,
    )
    .map_err(|_| invalid("TEXT span too large"))?;
    let end = at
        .checked_add(length)
        .ok_or_else(|| invalid("TEXT offset overflow"))?;
    let bytes = raw
        .contents()
        .get(at..end)
        .ok_or_else(|| invalid("truncated TEXT span"))?;
    decode_bytes(bytes, span.record_index, span.payload_offset, limits)
}

/// Select an ordinary planar root TEXT; interior and terminal records fail.
pub fn root_text_2d(raw: &RawDocument, index: u64, limits: &TextLimits) -> Result<TextRecord2D> {
    let mut selected = None;
    for span in raw.object_spans() {
        let span = span?;
        if span.record_index == index {
            selected = Some(span);
        }
    }
    let span = selected.ok_or_else(|| unsupported("selection must be a primary root object"))?;
    decode_root_text(raw, &span, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic hostile-input vectors preserve the original serialized text.
    fn fixture(body: &[u8]) -> Vec<u8> {
        let count = 3 + (body.len() + 1).div_ceil(48);
        let mut bytes = vec![0; count * 64];
        bytes[..16].copy_from_slice(&[
            5,
            0,
            1,
            0,
            1,
            1,
            1,
            0,
            count as u8,
            (count >> 8) as u8,
            1,
            0,
            0,
            1,
            0,
            0,
        ]);
        for (at, value) in [(16, 3.125_f64), (24, -7.5), (40, 3.0), (48, 3.0)] {
            bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        for offset in (64..bytes.len()).step_by(64) {
            bytes[offset] = 0xfe;
        }
        bytes[72] = 1;
        bytes[76] = 1;
        bytes[80..82].copy_from_slice(&(body.len() as u16).to_le_bytes());
        bytes[82] = 8;
        bytes[130] = 19;
        bytes[134] = 53;
        bytes[136] = 1;
        let font: Vec<u8> = "ＭＳ ゴシック"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        bytes[160..160 + font.len()].copy_from_slice(&font);
        bytes[194..198].copy_from_slice(&[44, 46, 3, 2]);
        for (index, value) in body.iter().enumerate() {
            bytes[208 + index / 48 * 64 + index % 48] = *value;
        }
        bytes
    }

    #[test]
    fn exact_serialized_sources_and_cross_slot_unicode_are_retained() {
        let native =
            b"{\\uni\"65e5\"}{\\uni\"672c\"}{\\uni\"8a9e\"}ABC{\\uni\"9ad9\"}{\\uni\"2460\"}";
        let bytes = fixture(native);
        let text = decode_bytes(&bytes, 28, 9984, &TextLimits::default()).unwrap();
        assert_eq!(text.text, "日本語ABC髙①");
        assert_eq!(text.body_bytes, native);
        assert_eq!(text.raw_bytes, bytes);
        assert_eq!(text.basepoint, [3.125, -7.5]);
        assert_eq!(text.font_name, "ＭＳ ゴシック");
        assert_eq!(text.record_index, 28);
        assert_eq!(text.units(), None);
    }

    #[test]
    fn lengths_terminators_headers_and_font_fail_closed() {
        assert!(decode_bytes(&fixture(b""), 0, 8192, &TextLimits::default()).is_err());
        for length in [1, 47, 48, 49, 95, 96, 97] {
            let data = vec![b'A'; length];
            assert_eq!(
                decode_bytes(&fixture(&data), 0, 8192, &TextLimits::default())
                    .unwrap()
                    .body_bytes,
                data
            );
        }
        for offset in [
            12,
            65,
            72,
            76,
            84,
            130,
            134,
            135,
            136,
            138,
            152,
            194,
            200,
            208 + 11,
        ] {
            let mut bytes = fixture(b"P5_ASCII_01");
            bytes[offset] ^= 1;
            assert!(
                decode_bytes(&bytes, 0, 8192, &TextLimits::default()).is_err(),
                "offset {offset}"
            );
        }
        let mut bytes = fixture(b"A");
        bytes[160..162].copy_from_slice(&0xd800_u16.to_le_bytes());
        assert!(decode_bytes(&bytes, 0, 8192, &TextLimits::default()).is_err());
    }

    #[test]
    fn limits_and_unqualified_markup_do_not_return_partial_text() {
        let bytes = fixture(b"{\\uni\"65e5\"}");
        assert!(matches!(
            decode_bytes(
                &bytes,
                0,
                8192,
                &TextLimits {
                    max_encoded_bytes: 11,
                    max_output_bytes: 20
                }
            ),
            Err(DracadError::LimitExceeded { .. })
        ));
        assert!(matches!(
            decode_bytes(
                &bytes,
                0,
                8192,
                &TextLimits {
                    max_encoded_bytes: 12,
                    max_output_bytes: 2
                }
            ),
            Err(DracadError::LimitExceeded { .. })
        ));
        for body in [
            b"{TCODE:UserName}".as_slice(),
            b"{\\b\"A\"}",
            b"{\\uni\"d800\"}",
            b"{\\uni\"0000\"}",
            b"A\r\nB",
            b"\x82\xa0",
            b"{\\uni\"1f600\"}",
            b"}",
        ] {
            assert!(
                decode_bytes(&fixture(body), 0, 8192, &TextLimits::default()).is_err(),
                "body {body:?}"
            );
        }
    }
}
