//! Stored strings: Shift_JIS bytes with saved escape codes.

use encoding_rs::SHIFT_JIS;

pub(crate) struct Decoded {
    pub text: String,
    /// Formatting codes were present; only their content was kept.
    pub format_codes: bool,
    /// Some bytes had no Shift_JIS meaning.
    pub lossy: bool,
}

/// Bytes up to the first NUL, decoded as Shift_JIS.
pub(crate) fn shift_jis(bytes: &[u8]) -> (String, bool) {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let (text, lossy) = SHIFT_JIS.decode_without_bom_handling(&bytes[..end]);
    (text.into_owned(), lossy)
}

/// UTF-16LE code units up to the first NUL unit.
pub(crate) fn utf16(bytes: &[u8]) -> (String, bool) {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    match String::from_utf16(&units) {
        Ok(text) => (text, false),
        Err(_) => (String::from_utf16_lossy(&units), true),
    }
}

/// Decode a saved string body.
///
/// `{\uni"XXXX"}`, with four to six hexadecimal digits, becomes the character
/// it names. Any other `{\name"content"}` code keeps its content and drops the
/// formatting it asked for. A styled run `{\Fname;\H1.8;text}` keeps its text,
/// with `\n` as a line break, and drops its style settings. Text that only
/// resembles a code is kept as written. Line breaks become line feeds.
pub(crate) fn body(bytes: &[u8]) -> Decoded {
    let (source, lossy) = SHIFT_JIS.decode_without_bom_handling(bytes);
    let mut format_codes = false;
    let text = expand(&source, &mut format_codes, 0)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Decoded {
        text,
        format_codes,
        lossy,
    }
}

/// Decode a string body saved as UTF-16 code units. Escape and formatting
/// codes are handled as in [`body`].
pub(crate) fn wide_body(bytes: &[u8]) -> Decoded {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let (source, lossy) = match String::from_utf16(&units) {
        Ok(text) => (text, false),
        Err(_) => (String::from_utf16_lossy(&units), true),
    };
    let mut format_codes = false;
    let text = expand(&source, &mut format_codes, 0)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Decoded {
        text,
        format_codes,
        lossy,
    }
}

/// How many characters of `text` are usual in a font name.
fn usual(text: &str) -> usize {
    text.chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, ' ' | '-' | '_' | '.' | '(' | ')' | '@')
                || matches!(
                    *character as u32,
                    0x3000 | 0x3040..=0x30ff | 0xff01..=0xff5e
                )
        })
        .count()
}

/// Decode a saved font name.
///
/// Generations before 25 store Shift_JIS. From generation 25 on, a name is
/// stored as UTF-16 or as Shift_JIS without a field that says which, so the
/// reading with the larger share of characters usual in font names wins.
pub(crate) fn font_name(bytes: &[u8], marker: u16) -> String {
    let (narrow, narrow_lossy) = shift_jis(bytes);
    if marker < 25 {
        return narrow;
    }
    let (wide, wide_lossy) = utf16(bytes);
    let share = |text: &str, lossy: bool| -> (usize, usize) {
        let total = text.chars().count();
        if lossy || total == 0 {
            (0, 1)
        } else {
            (usual(text), total)
        }
    };
    let (narrow_usual, narrow_total) = share(&narrow, narrow_lossy);
    let (wide_usual, wide_total) = share(&wide, wide_lossy);
    // Compare the shares as fractions; equal shares go to the longer reading,
    // then to UTF-16, which is the usual form in these generations.
    let left = narrow_usual * wide_total;
    let right = wide_usual * narrow_total;
    if left > right || (left == right && narrow_usual > wide_usual) {
        narrow
    } else {
        wide
    }
}

fn expand(source: &str, format_codes: &mut bool, depth: u8) -> String {
    let mut result = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("{\\") {
        result.push_str(&rest[..at]);
        let code = &rest[at..];
        match parse_code(code) {
            Some((name, content, length)) => {
                if name == "uni" {
                    match unicode_escape(content) {
                        Some(character) => result.push(character),
                        None => result.push_str(&code[..length]),
                    }
                } else {
                    *format_codes = true;
                    if depth < 8 {
                        result.push_str(&expand(content, format_codes, depth + 1));
                    } else {
                        result.push_str(content);
                    }
                }
                rest = &code[length..];
            }
            None => match parse_styled_run(code) {
                Some((content, length)) => {
                    *format_codes = true;
                    result.push_str(&content.replace("\\n", "\n"));
                    rest = &code[length..];
                }
                None => {
                    result.push_str("{\\");
                    rest = &code[2..];
                }
            },
        }
    }
    result.push_str(rest);
    result
}

/// Split `{\Fname;\H1.8;text}` into its text and total length in bytes.
///
/// A run needs at least one style setting: a backslash, an upper-case letter
/// and arguments up to a semicolon.
fn parse_styled_run(code: &str) -> Option<(&str, usize)> {
    let close = code.find('}')?;
    let mut body = &code[1..close];
    let mut styles = 0;
    while let Some(after) = body.strip_prefix('\\') {
        if !after.starts_with(|letter: char| letter.is_ascii_uppercase()) {
            break;
        }
        let Some(end) = after.find(';') else {
            break;
        };
        if after[..end].contains('\\') {
            break;
        }
        body = &after[end + 1..];
        styles += 1;
    }
    (styles > 0).then_some((body, close + 1))
}

/// Split `{\name"content"}` into its name, content and total length in bytes.
fn parse_code(code: &str) -> Option<(&str, &str, usize)> {
    let after = code.strip_prefix("{\\")?;
    let quote = after.find('"')?;
    let name = &after[..quote];
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    let content_start = quote + 1;
    let content_length = after[content_start..].find("\"}")?;
    let content = &after[content_start..content_start + content_length];
    Some((name, content, 2 + content_start + content_length + 2))
}

fn unicode_escape(content: &str) -> Option<char> {
    if !(4..=6).contains(&content.len()) || !content.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(content, 16).ok()?;
    char::from_u32(value).filter(|character| !character.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_jis_and_unicode_escapes_are_decoded() {
        // "BP" + U+4E38 U+92FC in Shift_JIS, then an escaped U+9AD9.
        let mut bytes = vec![0x42, 0x50, 0x8a, 0xdb, 0x8d, 0x7c];
        bytes.extend_from_slice(b"{\\uni\"9ad9\"}");
        let decoded = body(&bytes);
        assert_eq!(decoded.text, "BP\u{4e38}\u{92fc}\u{9ad9}");
        assert!(!decoded.format_codes && !decoded.lossy);
    }

    #[test]
    fn formatting_codes_keep_their_content() {
        let decoded = body(b"a{\\b\"BOLD\"}z");
        assert_eq!(decoded.text, "aBOLDz");
        assert!(decoded.format_codes);
    }

    #[test]
    fn styled_runs_keep_their_text_and_line_breaks() {
        let decoded =
            body(b"{\\FFont|128|49;\\H1.87;\\C016;Head}{\\FFont|128|49;\\H1.8;\\na\\nb}!");
        assert_eq!(decoded.text, "Head\na\nb!");
        assert!(decoded.format_codes);
        // A brace group without a style setting is ordinary text.
        assert_eq!(body(b"{\\nabc}").text, "{\\nabc}");
        assert!(!body(b"{\\Fopen;text").format_codes);
    }

    #[test]
    fn line_breaks_become_line_feeds() {
        assert_eq!(body(b"a\rb\r\nc").text, "a\nb\nc");
    }

    #[test]
    fn text_that_is_not_a_code_is_kept() {
        for value in [
            "{\\",
            "{\\uni\"12\"}",
            "{\\uni\"0007\"}",
            "{\\uni\"1234567\"}",
            "{\\b\"open",
            "{\\\"x\"}",
        ] {
            assert_eq!(body(value.as_bytes()).text, value);
        }
    }

    #[test]
    fn invalid_bytes_are_replaced_and_reported() {
        let decoded = body(&[0x41, 0x81]);
        assert!(decoded.lossy);
        assert!(decoded.text.starts_with('A'));
    }

    #[test]
    fn wide_bodies_expand_the_same_codes() {
        let bytes: Vec<u8> = "a{\\uni\"9ad9\"}{\\b\"B\"}\rz"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let decoded = wide_body(&bytes);
        assert_eq!(decoded.text, "a\u{9ad9}B\nz");
        assert!(decoded.format_codes && !decoded.lossy);
        assert!(wide_body(&[0x00, 0xd8]).lossy);
    }

    #[test]
    fn font_name_encoding_is_chosen_by_plausibility() {
        let wide = |text: &str| -> Vec<u8> {
            let mut bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
            bytes.resize(32, 0);
            bytes
        };
        let narrow = |bytes: &[u8]| -> Vec<u8> {
            let mut field = bytes.to_vec();
            field.resize(32, 0);
            field
        };
        // Fullwidth "MS" and katakana in Shift_JIS.
        let gothic = [
            0x82, 0x6c, 0x82, 0x72, 0x20, 0x83, 0x53, 0x83, 0x56, 0x83, 0x62, 0x83, 0x4e,
        ];
        let name = "\u{ff2d}\u{ff33} \u{30b4}\u{30b7}\u{30c3}\u{30af}";
        assert_eq!(font_name(&narrow(&gothic), 25), name);
        assert_eq!(font_name(&wide(name), 25), name);
        assert_eq!(font_name(&wide("Arial"), 25), "Arial");
        assert_eq!(font_name(&narrow(b"Century"), 25), "Century");
        assert_eq!(
            font_name(&wide("\u{6e38}\u{30b4}\u{30b7}\u{30c3}\u{30af}"), 25)
                .chars()
                .count(),
            5
        );
        // Older generations never store UTF-16.
        assert_eq!(font_name(&narrow(b"Century"), 22), "Century");
        assert_eq!(font_name(&narrow(&gothic), 5), name);
        assert_eq!(font_name(&[0; 32], 25), "");
    }

    #[test]
    fn font_names_stop_at_the_terminator() {
        assert_eq!(shift_jis(b"Font\0junk").0, "Font");
        assert_eq!(
            utf16(&[0x2d, 0xff, 0x33, 0xff, 0, 0, 0x41, 0]).0,
            "\u{ff2d}\u{ff33}"
        );
    }
}
