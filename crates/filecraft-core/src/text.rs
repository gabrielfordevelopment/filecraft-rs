//! Text file conventions shared by the exports.

use std::fs;
use std::io;
use std::path::Path;

/// Exported files start with a UTF-8 byte order mark.
pub const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Exported files use the platform line ending.
pub const LINE_ENDING: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Length in UTF-16 code units.
pub fn utf16_len(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

/// Reads a text file into lines: the encoding is taken from a UTF-8, UTF-16
/// or UTF-32 byte order mark and defaults to UTF-8, invalid bytes become U+FFFD, and lines end at
/// `\r\n`, `\n` or `\r`.
pub fn read_text_lines(path: &Path) -> io::Result<Vec<String>> {
    Ok(split_lines(&decode(&fs::read(path)?)))
}

fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        String::from_utf8_lossy(rest).into_owned()
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE\x00\x00") {
        decode_utf32(rest, u32::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(b"\x00\x00\xFE\xFF") {
        decode_utf32(rest, u32::from_be_bytes)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        decode_utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        decode_utf16(rest, u16::from_be_bytes)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units = bytes.chunks(2).map(|pair| match pair {
        [a, b] => unit([*a, *b]),
        _ => 0xFFFD,
    });
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn decode_utf32(bytes: &[u8], unit: fn([u8; 4]) -> u32) -> String {
    bytes
        .chunks(4)
        .map(|chunk| match chunk {
            [a, b, c, d] => {
                char::from_u32(unit([*a, *b, *c, *d])).unwrap_or(char::REPLACEMENT_CHARACTER)
            }
            _ => char::REPLACEMENT_CHARACTER,
        })
        .collect()
}

/// Splits text into lines. A final line ending does not start another line.
pub fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines.push(std::mem::take(&mut current));
            }
            '\n' => lines.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_all_line_endings() {
        assert_eq!(split_lines("a\r\nb\nc\rd"), ["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\n"), ["a"]);
        assert_eq!(split_lines("a\n\n"), ["a", ""]);
        assert!(split_lines("").is_empty());
    }

    #[test]
    fn decodes_byte_order_marks() {
        assert_eq!(decode(b"\xEF\xBB\xBFhi"), "hi");
        assert_eq!(decode(b"\xFF\xFEh\x00i\x00"), "hi");
        assert_eq!(decode(b"\xFE\xFF\x00h\x00i"), "hi");
        assert_eq!(decode(b"\xFF\xFE\x00\x00h\x00\x00\x00"), "h");
        assert_eq!(decode(b"ok\xFF"), "ok\u{FFFD}");
    }

    #[test]
    fn counts_utf16_units() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("\u{1F600}"), 2);
    }
}
