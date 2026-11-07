//! Bounded raw style-table fields, without effective style or geometry units.

use crate::{DracadError, RawDocument, Result};

/// One observed layer row. Special layers retain their full 296-byte payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLayerStyle {
    pub index: u16,
    pub payload_offset: u64,
    pub raw_bytes: Vec<u8>,
    pub flags_raw: u16,
    pub reserved_word_raw: u16,
    pub color_index_raw: u16,
    pub line_type_index_raw: u16,
    pub width_index_raw: u16,
    pub stored_index_raw: u16,
}

/// Palette entry. RGB bytes and output RGB are not DXF ACI or final appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct RawColorStyle {
    pub index: u16,
    pub payload_offset: u64,
    pub raw_bytes: [u8; 64],
    pub flags_raw: u16,
    pub pen_width_hundredths_raw: u16,
    pub pen_width_value: f64,
    pub rgb: [u8; 3],
    pub rgb_encoding_raw: u8,
    pub output_rgb: [u8; 3],
    pub output_rgb_encoding_raw: u8,
}

/// Fixed-capacity pattern values. No logical pattern length is inferred.
#[derive(Debug, Clone, PartialEq)]
pub struct RawLineTypeStyle {
    pub index: u16,
    pub payload_offset: u64,
    pub raw_bytes: [u8; 64],
    pub flags_raw: u16,
    pub pen_style_raw: u16,
    pub pen_definition_hundredths_raw: [u16; 30],
    pub pen_definition_values: [f64; 30],
}

/// Stored width-table value; not an effective or color-based printed width.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWidthStyle {
    pub index: u16,
    pub payload_offset: u64,
    pub raw_bytes: [u8; 64],
    pub flags_raw: u16,
    pub pen_width_hundredths_raw: u16,
    pub pen_width_value: f64,
}

/// Observed tables and the exact shared suffix. All offsets are in Contents.
///
/// This parses three fixed extensions, not arbitrary TLVs. Unknown extension
/// headers or nonzero padding in the shared 1768-byte prefix stop the request.
/// Marker25's additional 5504 bytes are retained without interpretation; they
/// do not qualify effective styles. Opaque row fields and encoding words remain
/// observable.
/// Numbers are file-local references; no ByLayer/ByBlock resolution, palette-to-
/// ACI conversion, print policy or physical geometry-unit conversion is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RawStyleTables {
    /// Sorted by numeric index 0..=256, including the two special suffix rows.
    pub layers: Vec<RawLayerStyle>,
    /// Sorted by numeric index 1..=256, including the special suffix color.
    pub colors: Vec<RawColorStyle>,
    pub line_types: Vec<RawLineTypeStyle>,
    pub widths: Vec<RawWidthStyle>,
    pub shared_suffix_offset: u64,
    pub shared_suffix_bytes: Vec<u8>,
    pub extension_header_offsets: [u64; 3],
    pub extension_header_bytes: [[u8; 16]; 3],
}

fn invalid(message: &str) -> DracadError {
    DracadError::InvalidFormat(message.into())
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn bounded(bytes: &[u8], offset: u64, size: u64) -> Result<&[u8]> {
    let end = offset
        .checked_add(size)
        .ok_or_else(|| invalid("style-table slice offset overflow"))?;
    let start = usize::try_from(offset).map_err(|_| invalid("style offset is too large"))?;
    let end = usize::try_from(end).map_err(|_| invalid("style end offset is too large"))?;
    bytes
        .get(start..end)
        .ok_or_else(|| invalid("style-table row exceeds Contents"))
}

fn fixed_row(bytes: &[u8], offset: u64) -> Result<[u8; 64]> {
    bounded(bytes, offset, 64)?
        .try_into()
        .map_err(|_| invalid("truncated style-table row"))
}

fn word(bytes: &[u8; 64], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn layer(index: u16, offset: u64, raw_bytes: &[u8]) -> Result<RawLayerStyle> {
    let header: [u8; 64] = raw_bytes
        .get(..64)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated layer-style row"))?;
    let stored_index_raw = word(&header, 10);
    if stored_index_raw != index {
        return Err(unsupported("unqualified style-table layer row index"));
    }
    Ok(RawLayerStyle {
        index,
        payload_offset: offset,
        raw_bytes: raw_bytes.to_vec(),
        flags_raw: word(&header, 0),
        reserved_word_raw: word(&header, 2),
        color_index_raw: word(&header, 4),
        line_type_index_raw: word(&header, 6),
        width_index_raw: word(&header, 8),
        stored_index_raw,
    })
}

fn color(index: u16, offset: u64, raw_bytes: [u8; 64]) -> RawColorStyle {
    let pen_width_hundredths_raw = word(&raw_bytes, 2);
    RawColorStyle {
        index,
        payload_offset: offset,
        flags_raw: word(&raw_bytes, 0),
        pen_width_hundredths_raw,
        pen_width_value: f64::from(pen_width_hundredths_raw) / 100.0,
        rgb: [raw_bytes[4], raw_bytes[5], raw_bytes[6]],
        rgb_encoding_raw: raw_bytes[7],
        output_rgb: [raw_bytes[8], raw_bytes[9], raw_bytes[10]],
        output_rgb_encoding_raw: raw_bytes[11],
        raw_bytes,
    }
}

fn line_type(index: u16, offset: u64, raw_bytes: [u8; 64]) -> RawLineTypeStyle {
    let mut pen_definition_hundredths_raw = [0u16; 30];
    let mut pen_definition_values = [0.0; 30];
    for (i, value) in pen_definition_hundredths_raw.iter_mut().enumerate() {
        *value = word(&raw_bytes, 4 + i * 2);
        pen_definition_values[i] = f64::from(*value) / 100.0;
    }
    RawLineTypeStyle {
        index,
        payload_offset: offset,
        flags_raw: word(&raw_bytes, 0),
        pen_style_raw: word(&raw_bytes, 2),
        pen_definition_hundredths_raw,
        pen_definition_values,
        raw_bytes,
    }
}

fn width(index: u16, offset: u64, raw_bytes: [u8; 64]) -> RawWidthStyle {
    let pen_width_hundredths_raw = word(&raw_bytes, 2);
    RawWidthStyle {
        index,
        payload_offset: offset,
        flags_raw: word(&raw_bytes, 0),
        pen_width_hundredths_raw,
        pen_width_value: f64::from(pen_width_hundredths_raw) / 100.0,
        raw_bytes,
    }
}

impl RawStyleTables {
    /// Parse only qualified observed table bounds, preserving all row bytes.
    ///
    /// BlockLibrary supplies raw tail boundaries; no block geometry is decoded.
    /// The document's existing root+definition record limits still apply.
    pub fn parse(document: &RawDocument) -> Result<Self> {
        let bounds = document.header().section_offsets;
        for (pair, expected) in bounds[..5].windows(2).zip([16320, 16320, 2048, 1024]) {
            if pair[1].checked_sub(pair[0]) != Some(expected) {
                return Err(unsupported("unqualified raw style-table bounds"));
            }
        }
        let library = document.block_library()?;
        let shared_suffix_offset = library
            .suffix_offset()
            .ok_or_else(|| unsupported("missing bounded style-table suffix"))?;
        let shared_suffix = library.suffix_bytes()?;
        let expected_size = match document.header().marker_raw {
            5 | 8 => 1768,
            25 => 7272,
            _ => return Err(unsupported("unqualified raw style-table profile")),
        };
        if shared_suffix.len() != expected_size {
            return Err(unsupported("unqualified style-table shared suffix size"));
        }
        let extension_starts = [0usize, 80, 392];
        let expected_headers = [
            (2u32, 256u32, 0u32, 64u32),
            (1, 0, 0, 296),
            (1, 256, 0, 296),
        ];
        let mut extension_header_bytes = [[0; 16]; 3];
        let mut extension_header_offsets = [0; 3];
        for (i, (start, (kind, index, reserved, size))) in extension_starts
            .into_iter()
            .zip(expected_headers)
            .enumerate()
        {
            let header: [u8; 16] = shared_suffix
                .get(start..start + 16)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| invalid("truncated style-table suffix header"))?;
            let mut expected = [0u8; 16];
            for (chunk, value) in expected
                .chunks_exact_mut(4)
                .zip([kind, index, reserved, size])
            {
                chunk.copy_from_slice(&value.to_le_bytes());
            }
            if header != expected {
                return Err(unsupported(
                    "unqualified style-table suffix extension header",
                ));
            }
            extension_header_bytes[i] = header;
            extension_header_offsets[i] = shared_suffix_offset
                .checked_add(start as u64)
                .ok_or_else(|| invalid("style-table suffix offset overflow"))?;
        }
        if shared_suffix
            .get(704..1768)
            .ok_or_else(|| invalid("truncated style-table suffix extensions"))?
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(unsupported(
                "unqualified style-table extension prefix padding",
            ));
        }
        let bytes = document.contents();
        let mut layers = Vec::with_capacity(257);
        layers.push(layer(
            0,
            shared_suffix_offset + 96,
            bounded(bytes, shared_suffix_offset + 96, 296)?,
        )?);
        for index in 1u16..=255 {
            let offset = bounds[0] + u64::from(index - 1) * 64;
            layers.push(layer(index, offset, bounded(bytes, offset, 64)?)?);
        }
        layers.push(layer(
            256,
            shared_suffix_offset + 408,
            bounded(bytes, shared_suffix_offset + 408, 296)?,
        )?);
        let mut colors = Vec::with_capacity(256);
        for index in 1u16..=255 {
            let offset = bounds[1] + u64::from(index - 1) * 64;
            colors.push(color(index, offset, fixed_row(bytes, offset)?));
        }
        colors.push(color(
            256,
            shared_suffix_offset + 16,
            fixed_row(bytes, shared_suffix_offset + 16)?,
        ));
        let mut line_types = Vec::with_capacity(32);
        for index in 1u16..=32 {
            let offset = bounds[2] + u64::from(index - 1) * 64;
            line_types.push(line_type(index, offset, fixed_row(bytes, offset)?));
        }
        let mut widths = Vec::with_capacity(16);
        for index in 1u16..=16 {
            let offset = bounds[3] + u64::from(index - 1) * 64;
            widths.push(width(index, offset, fixed_row(bytes, offset)?));
        }
        Ok(Self {
            layers,
            colors,
            line_types,
            widths,
            shared_suffix_offset,
            shared_suffix_bytes: shared_suffix.to_vec(),
            extension_header_offsets,
            extension_header_bytes,
        })
    }
}
