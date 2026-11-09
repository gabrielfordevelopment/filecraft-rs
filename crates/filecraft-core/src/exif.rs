//! Reads the date a photo was taken from its EXIF metadata, with the same results as Pillow's
//! `Image.open(path)._getexif()` followed by the date parsing of the original renaming script.
//!
//! Pillow offers `_getexif()` only for JPEG (including MPO), PNG and WebP images. Other files,
//! TIFF images among them, have no EXIF date here either. The format is recognized by the file
//! content, not by the extension. The compressed "Raw profile type exif" text of some PNG files is
//! not read.
//!
//! Only the headers are read: the image data is skipped, so a large photo costs a few kilobytes
//! of reading. Unlike Pillow, the image data itself is not decoded, so a PNG or WebP file whose
//! image data is broken can still give a date here.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use chrono::NaiveDate;

/// `DateTimeOriginal` of the Exif sub-directory.
const TAG_DATE_TIME_ORIGINAL: u16 = 0x9003;
/// `DateTime` of the main directory.
const TAG_DATE_TIME: u16 = 0x0132;
/// Pointer to the Exif sub-directory.
const TAG_EXIF_IFD: u16 = 0x8769;

const EXIF_HEADER: &[u8] = b"Exif\0\0";
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Pillow's `Image.open` refuses images with more pixels than twice `Image.MAX_IMAGE_PIXELS`, as
/// possible decompression bombs.
const MAX_PIXELS: u64 = 2 * (1024 * 1024 * 1024 / 4 / 3);

/// Skips up to this many bytes are read through the buffer, longer ones seek.
const SHORT_SKIP: u64 = 8 * 1024;

/// The date the photo at `path` was taken, or `None` when the file has no readable EXIF date.
pub fn exif_date(path: &Path) -> Option<NaiveDate> {
    let file = File::open(path).ok()?;
    let block = exif_block(&mut Reader::new(BufReader::new(file)).ok()?)?;
    let entries = merged_entries(&block)?;
    let text = date_text(&entries)?;
    parse_exif_date(&text)
}

/// A file read from the start. Parts that are not needed are skipped instead of read. Reading
/// past the end, or a failed read, gives `None`, like the exception that stops Pillow.
struct Reader<R> {
    inner: R,
    pos: u64,
    len: u64,
}

impl<R: BufRead + Seek> Reader<R> {
    fn new(mut inner: R) -> io::Result<Self> {
        let len = inner.seek(SeekFrom::End(0))?;
        inner.seek(SeekFrom::Start(0))?;
        Ok(Self { inner, pos: 0, len })
    }

    fn remaining(&self) -> u64 {
        self.len.saturating_sub(self.pos)
    }

    /// The next `count` bytes.
    fn bytes(&mut self, count: u64) -> Option<Vec<u8>> {
        if count > self.remaining() {
            return None;
        }
        let mut data = vec![0; usize::try_from(count).ok()?];
        self.inner.read_exact(&mut data).ok()?;
        self.pos += count;
        Some(data)
    }

    fn byte(&mut self) -> Option<u8> {
        if self.remaining() == 0 {
            return None;
        }
        let mut byte = [0];
        self.inner.read_exact(&mut byte).ok()?;
        self.pos += 1;
        Some(byte[0])
    }

    /// Skips `count` bytes, which must all be there.
    fn skip(&mut self, count: u64) -> Option<()> {
        if count > self.remaining() {
            return None;
        }
        if count <= SHORT_SKIP {
            let skipped = io::copy(&mut (&mut self.inner).take(count), &mut io::sink()).ok()?;
            (skipped == count).then_some(())?;
        } else {
            self.inner
                .seek(SeekFrom::Current(i64::try_from(count).ok()?))
                .ok()?;
        }
        self.pos += count;
        Some(())
    }

    fn rewind(&mut self) -> Option<()> {
        self.inner.seek(SeekFrom::Start(0)).ok()?;
        self.pos = 0;
        Some(())
    }
}

/// The raw EXIF data of a JPEG, PNG or WebP file, as Pillow stores it in `info["exif"]`. Other
/// files give `None` after their first bytes, so large videos are not read.
fn exif_block<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<Vec<u8>> {
    let head = file.bytes(file.remaining().min(12))?;
    file.rewind()?;
    if head.starts_with(b"\xFF\xD8\xFF") {
        jpeg_exif(file)
    } else if head.starts_with(PNG_SIGNATURE) {
        png_exif(file)
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP".as_slice()) {
        webp_exif(file)
    } else {
        None
    }
}

/// An image Pillow can open: it has a width and a height, and not more pixels than its limit.
fn openable_size(width: u64, height: u64) -> Option<()> {
    (width > 0 && height > 0 && width * height <= MAX_PIXELS).then_some(())
}

/// The APP1 "Exif" segments of a JPEG file, read like Pillow's `JpegImageFile._open`: the header
/// is read marker by marker up to the start of the image data, and bytes between markers are
/// skipped. Further Exif segments are appended to the first one without their header. A header
/// Pillow cannot read gives `None`: one that is cut off, has an unknown marker, a broken segment
/// or no usable frame, or an image larger than Pillow's pixel limit.
fn jpeg_exif<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<Vec<u8>> {
    file.skip(2)?;
    let mut exif: Option<Vec<u8>> = None;
    let mut size = None;
    let mut icc_parts: Vec<Vec<u8>> = Vec::new();
    let mut byte = file.byte()?;
    loop {
        if byte != 0xFF {
            byte = file.byte()?;
            continue;
        }
        let marker = file.byte()?;
        match marker {
            // Padding: the second 0xFF starts the marker.
            0xFF => continue,
            // An escaped 0xFF in data.
            0x00 => {
                byte = file.byte()?;
                continue;
            }
            // Markers without a segment.
            0xC8 | 0xD0..=0xD9 | 0xF0..=0xFD => {}
            // Segments Pillow skips. The image data follows the start of scan (0xDA).
            0xC4 | 0xCC | 0xDA | 0xDC | 0xDD | 0xDF => {
                let length = segment_length(file)?;
                file.skip(length)?;
            }
            // Frame headers.
            0xC0..=0xCF | 0xDE => size = Some(frame_size(&segment(file)?, &mut icc_parts)?),
            0xDB => check_quantization_tables(&segment(file)?)?,
            0xE0..=0xEF => app_segment(marker, segment(file)?, &mut exif, &mut icc_parts)?,
            // Comment.
            0xFE => {
                segment(file)?;
            }
            _ => return None,
        }
        if marker == 0xDA {
            break;
        }
        byte = file.byte()?;
    }
    let (width, height) = size?;
    openable_size(width, height)?;
    exif
}

/// The length of the segment after a marker, without its two length bytes. Pillow reads nothing
/// when the length is below two.
fn segment_length<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<u64> {
    let bytes = file.bytes(2)?;
    Some(u64::from(u16::from_be_bytes([bytes[0], bytes[1]])).saturating_sub(2))
}

fn segment<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<Vec<u8>> {
    let length = segment_length(file)?;
    file.bytes(length)
}

/// The width and height of a frame header, checked like Pillow's `SOF`: 8 bit samples, 1, 3 or 4
/// components and complete component entries. ICC profile parts collected before it must hold
/// their count, which Pillow reads from the first part.
fn frame_size(segment: &[u8], icc_parts: &mut Vec<Vec<u8>>) -> Option<(u64, u64)> {
    let height = u16::from_be_bytes([*segment.get(1)?, *segment.get(2)?]);
    let width = u16::from_be_bytes([*segment.get(3)?, *segment.get(4)?]);
    if *segment.first()? != 8 || !matches!(*segment.get(5)?, 1 | 3 | 4) {
        return None;
    }
    if !icc_parts.is_empty() {
        icc_parts.sort();
        icc_parts.first()?.get(13)?;
        icc_parts.clear();
    }
    if !(segment.len() - 6).is_multiple_of(3) {
        return None;
    }
    Some((u64::from(width), u64::from(height)))
}

/// Checks a quantization table segment like Pillow's `DQT`: every table must be complete.
fn check_quantization_tables(mut tables: &[u8]) -> Option<()> {
    while let Some(&first) = tables.first() {
        let precision = if first / 16 == 0 { 1 } else { 2 };
        tables = tables.get(1 + precision * 64..)?;
    }
    Some(())
}

/// An application segment, read like Pillow's `APP`. Exif segments are collected. JFIF and Adobe
/// segments must be long enough for the values Pillow reads without a check.
fn app_segment(
    marker: u8,
    segment: Vec<u8>,
    exif: &mut Option<Vec<u8>>,
    icc_parts: &mut Vec<Vec<u8>>,
) -> Option<()> {
    match marker {
        0xE0 if segment.starts_with(b"JFIF") => {
            segment.get(5..7)?;
        }
        0xE1 if segment.starts_with(EXIF_HEADER) => match exif {
            Some(data) => data.extend_from_slice(&segment[EXIF_HEADER.len()..]),
            None => *exif = Some(segment),
        },
        0xE2 if segment.starts_with(b"ICC_PROFILE\0") => icc_parts.push(segment),
        0xED if segment.starts_with(b"Photoshop 3.0\0") => check_photoshop_resources(&segment)?,
        0xEE if segment.starts_with(b"Adobe") => {
            segment.get(5..7)?;
        }
        _ => {}
    }
    Some(())
}

/// Walks the resource blocks of a Photoshop segment like Pillow does. A block cut off right after
/// its code makes Pillow fail. Other cut off blocks only end the walk.
fn check_photoshop_resources(segment: &[u8]) -> Option<()> {
    let be_u32 = |pos: usize| -> Option<usize> {
        let bytes: [u8; 4] = segment.get(pos..pos + 4)?.try_into().ok()?;
        usize::try_from(u32::from_be_bytes(bytes)).ok()
    };
    let mut offset = 14;
    while segment.get(offset..offset + 4) == Some(b"8BIM".as_slice()) {
        offset += 4;
        let Some(code) = segment.get(offset..offset + 2) else {
            return Some(());
        };
        let code = u16::from_be_bytes([code[0], code[1]]);
        offset += 2;
        let name_length = usize::from(*segment.get(offset)?);
        offset += 1 + name_length;
        offset += offset & 1;
        let Some(size) = be_u32(offset) else {
            return Some(());
        };
        offset += 4;
        // The resolution values are read from the block, and a short block ends the walk.
        let data_end = offset.saturating_add(size).min(segment.len());
        if code == 0x03ED
            && segment
                .get(offset..data_end)
                .is_none_or(|data| data.len() < 14)
        {
            return Some(());
        }
        offset = offset.saturating_add(size);
        offset += offset & 1;
    }
    Some(())
}

/// The name and data length of the next PNG chunk. `None` when the file ends or the name is not
/// four letters, digits or underscores, which Pillow treats as a broken file.
fn png_chunk_header<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<([u8; 4], u64)> {
    let header = file.bytes(8)?;
    let kind: [u8; 4] = header[4..8].try_into().ok()?;
    if !kind
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return None;
    }
    let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    Some((kind, u64::from(length)))
}

/// The `eXIf` chunk of a PNG file, read like Pillow. The chunks before the image data must have
/// correct names and checksums and a usable header (IHDR), and EXIF found there is used without
/// reading further. Otherwise Pillow loads the image and takes the last `eXIf` chunk after the
/// image data, up to the end or, in an animation, the next frame. A chunk cut off there makes
/// loading fail.
fn png_exif<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<Vec<u8>> {
    file.skip(PNG_SIGNATURE.len() as u64)?;
    let mut exif = None;
    let mut size = None;
    let mut has_mode = false;
    let mut animated = false;
    let image_length = loop {
        let (kind, length) = png_chunk_header(file)?;
        match &kind {
            b"IDAT" => break Some(length),
            b"IEND" => break None,
            _ => {}
        }
        let data = file.bytes(length)?;
        let stored = file.bytes(4)?;
        if crc32(&[&kind[..], &data]) != u32::from_be_bytes(stored.try_into().ok()?) {
            return None;
        }
        match &kind {
            b"IHDR" => {
                let width = u32::from_be_bytes(data.get(0..4)?.try_into().ok()?);
                let height = u32::from_be_bytes(data.get(4..8)?.try_into().ok()?);
                if data.len() < 13 || data[11] != 0 {
                    return None;
                }
                size = Some((u64::from(width), u64::from(height)));
                has_mode |= matches!(
                    (data[8], data[9]),
                    (1 | 2 | 4 | 8 | 16, 0) | (8 | 16, 2 | 4 | 6) | (1 | 2 | 4 | 8, 3)
                );
            }
            b"eXIf" => exif = Some(data),
            b"acTL" if data.len() >= 4 => {
                animated = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) > 1;
            }
            _ => {}
        }
    };
    let (width, height) = size?;
    if !has_mode {
        return None;
    }
    openable_size(width, height)?;
    let Some(image_length) = image_length else {
        return exif;
    };
    if exif.is_some() {
        return exif;
    }

    file.skip(image_length)?;
    loop {
        // The checksum of the previous chunk is not checked here.
        if file.skip(4).is_none() {
            break;
        }
        let Some((kind, length)) = png_chunk_header(file) else {
            break;
        };
        match &kind {
            b"IEND" => break,
            b"fcTL" if animated => break,
            b"eXIf" => exif = Some(file.bytes(length)?),
            _ => file.skip(length)?,
        }
    }
    exif
}

/// The CRC-32 of the PNG chunk checksums over the given parts.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in parts.iter().flat_map(|part| part.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The `EXIF` chunk of a WebP file. An extended file (VP8X) whose canvas is larger than Pillow's
/// pixel limit gives `None`.
fn webp_exif<R: BufRead + Seek>(file: &mut Reader<R>) -> Option<Vec<u8>> {
    file.skip(12)?;
    loop {
        let header = file.bytes(8)?;
        let size = u64::from(u32::from_le_bytes([
            header[4], header[5], header[6], header[7],
        ]));
        match &header[0..4] {
            b"EXIF" => {
                let data = file.bytes(size)?;
                return (!data.is_empty()).then_some(data);
            }
            b"VP8X" => {
                let data = file.bytes(size)?;
                if let Some(canvas) = data.get(4..10) {
                    let width = u32::from_le_bytes([canvas[0], canvas[1], canvas[2], 0]) + 1;
                    let height = u32::from_le_bytes([canvas[3], canvas[4], canvas[5], 0]) + 1;
                    openable_size(u64::from(width), u64::from(height))?;
                }
                file.skip(size % 2)?;
            }
            _ => file.skip(size + size % 2)?,
        }
    }
}

/// A tag value: ASCII text or any other type, which the date parsing cannot use.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Text(String),
    Number(u32),
    Other,
}

/// Byte order and contents of the TIFF structure inside the EXIF data.
struct Tiff<'a> {
    data: &'a [u8],
    little_endian: bool,
}

impl Tiff<'_> {
    fn u16_at(&self, pos: usize) -> Option<u16> {
        let bytes = [*self.data.get(pos)?, *self.data.get(pos + 1)?];
        Some(if self.little_endian {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    }

    fn u32_at(&self, pos: usize) -> Option<u32> {
        let bytes: [u8; 4] = self.data.get(pos..pos + 4)?.try_into().ok()?;
        Some(if self.little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    /// The tags of the directory at `offset`. Like Pillow, entries whose data is cut off or of an
    /// unknown type are skipped, and a directory cut off in the middle keeps the entries read so
    /// far.
    fn directory(&self, offset: usize) -> Vec<(u16, Value)> {
        let mut entries = Vec::new();
        let Some(count) = self.u16_at(offset) else {
            return entries;
        };
        for index in 0..usize::from(count) {
            let entry = offset + 2 + index * 12;
            let (Some(tag), Some(kind), Some(count)) = (
                self.u16_at(entry),
                self.u16_at(entry + 2),
                self.u32_at(entry + 4),
            ) else {
                break;
            };
            if self.data.get(entry + 8..entry + 12).is_none() {
                break;
            }
            let Some(unit) = type_size(kind) else {
                continue;
            };
            let Some(size) = (count as usize).checked_mul(unit) else {
                continue;
            };
            let value_pos = if size > 4 {
                match self.u32_at(entry + 8) {
                    Some(pointer) => pointer as usize,
                    None => continue,
                }
            } else {
                entry + 8
            };
            let Some(value) = value_pos
                .checked_add(size)
                .and_then(|end| self.data.get(value_pos..end))
            else {
                continue;
            };
            if value.is_empty() {
                continue;
            }
            entries.push((tag, self.value(kind, count, value)));
        }
        entries
    }

    fn value(&self, kind: u16, count: u32, data: &[u8]) -> Value {
        match (kind, count) {
            // ASCII, read like Pillow: one trailing NUL removed, decoded as Latin-1.
            (2, _) => {
                let data = data.strip_suffix(b"\0").unwrap_or(data);
                Value::Text(data.iter().map(|byte| char::from(*byte)).collect())
            }
            (3, 1) => self.u16_at_slice(data).map_or(Value::Other, Value::Number),
            (4 | 13, 1) => self.u32_at_slice(data).map_or(Value::Other, Value::Number),
            _ => Value::Other,
        }
    }

    fn u16_at_slice(&self, data: &[u8]) -> Option<u32> {
        Tiff {
            data,
            little_endian: self.little_endian,
        }
        .u16_at(0)
        .map(u32::from)
    }

    fn u32_at_slice(&self, data: &[u8]) -> Option<u32> {
        Tiff {
            data,
            little_endian: self.little_endian,
        }
        .u32_at(0)
    }
}

/// Size in bytes of one value of a TIFF field type, for the types Pillow reads.
fn type_size(kind: u16) -> Option<usize> {
    match kind {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 | 13 => Some(4),
        5 | 10 | 12 => Some(8),
        16..=18 => Some(8),
        _ => None,
    }
}

/// The tags of the main directory merged with the Exif sub-directory, like Pillow's
/// `_get_merged_dict`. Later entries with the same tag replace earlier ones.
fn merged_entries(block: &[u8]) -> Option<Vec<(u16, Value)>> {
    let mut data = block;
    while let Some(rest) = data.strip_prefix(EXIF_HEADER) {
        data = rest;
    }
    let little_endian = match data.get(0..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let tiff = Tiff {
        data,
        little_endian,
    };
    let mut entries = tiff.directory(tiff.u32_at(4)? as usize);
    let exif_offset = entries.iter().rev().find_map(|(tag, value)| match value {
        Value::Number(offset) if *tag == TAG_EXIF_IFD => Some(*offset as usize),
        _ => None,
    });
    if let Some(offset) = exif_offset {
        entries.extend(tiff.directory(offset));
    }
    (!entries.is_empty()).then_some(entries)
}

/// `exif.get(DateTimeOriginal) or exif.get(DateTime)`: the original date unless it is missing or
/// empty text. A value that is not text gives no date.
fn date_text(entries: &[(u16, Value)]) -> Option<String> {
    let lookup = |tag: u16| {
        entries
            .iter()
            .rev()
            .find(|(entry_tag, _)| *entry_tag == tag)
            .map(|(_, value)| value)
    };
    let chosen = match lookup(TAG_DATE_TIME_ORIGINAL) {
        Some(Value::Text(text)) if text.is_empty() => lookup(TAG_DATE_TIME),
        Some(Value::Number(0)) | None => lookup(TAG_DATE_TIME),
        other => other,
    };
    match chosen? {
        Value::Text(text) => Some(text.clone()),
        Value::Number(_) | Value::Other => None,
    }
}

/// The date part of an EXIF date such as `2021:03:04 05:06:07`: the text up to the first space as a
/// four-digit year, a month and a day separated by colons.
pub fn parse_exif_date(text: &str) -> Option<NaiveDate> {
    let date = text.split(' ').next()?;
    let mut parts = date.split(':');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if year.len() != 4 || !digits(year) || !digits(month) || !digits(day) {
        return None;
    }
    // The month is 1 to 9, 01 to 09 or 10 to 12, the day is 1 to 9, 01 to 09 or 10 to 31.
    let field = |part: &str, max: u32| -> Option<u32> {
        let value: u32 = part.parse().ok()?;
        let valid = match part.len() {
            1 => (1..=9).contains(&value),
            2 => (1..=max).contains(&value),
            _ => false,
        };
        valid.then_some(value)
    };
    let year: i32 = year.parse().ok()?;
    if year < 1 {
        return None;
    }
    NaiveDate::from_ymd_opt(year, field(month, 12)?, field(day, 31)?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A little-endian TIFF structure with `DateTime` in the main directory and
    /// `DateTimeOriginal` in the Exif sub-directory, when given.
    pub(crate) fn tiff(date_time: Option<&str>, original: Option<&str>) -> Vec<u8> {
        let mut main: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        if let Some(text) = date_time {
            main.push((TAG_DATE_TIME, 2, format!("{text}\0").into_bytes()));
        }
        let mut sub: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        if let Some(text) = original {
            sub.push((TAG_DATE_TIME_ORIGINAL, 2, format!("{text}\0").into_bytes()));
        }
        // Layout: header (8), main directory, sub directory, then the values.
        let directory_size = |count: usize| 2 + count * 12 + 4;
        let main_count = main.len() + usize::from(!sub.is_empty());
        let main_offset = 8;
        let sub_offset = main_offset + directory_size(main_count);
        let mut values_offset = if sub.is_empty() {
            sub_offset
        } else {
            sub_offset + directory_size(sub.len())
        };
        let mut out = b"II*\0".to_vec();
        out.extend_from_slice(&(main_offset as u32).to_le_bytes());
        let mut values = Vec::new();
        let mut write_directory =
            |out: &mut Vec<u8>, entries: &[(u16, u16, Vec<u8>)], pointer: Option<u32>| {
                let count = entries.len() + usize::from(pointer.is_some());
                out.extend_from_slice(&(count as u16).to_le_bytes());
                for (tag, kind, data) in entries {
                    out.extend_from_slice(&tag.to_le_bytes());
                    out.extend_from_slice(&kind.to_le_bytes());
                    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
                    if data.len() <= 4 {
                        // Short values are stored in the entry itself.
                        let mut inline = data.clone();
                        inline.resize(4, 0);
                        out.extend_from_slice(&inline);
                    } else {
                        out.extend_from_slice(&(values_offset as u32).to_le_bytes());
                        values.extend_from_slice(data);
                        values_offset += data.len();
                    }
                }
                if let Some(pointer) = pointer {
                    out.extend_from_slice(&TAG_EXIF_IFD.to_le_bytes());
                    out.extend_from_slice(&4u16.to_le_bytes());
                    out.extend_from_slice(&1u32.to_le_bytes());
                    out.extend_from_slice(&pointer.to_le_bytes());
                }
                out.extend_from_slice(&0u32.to_le_bytes());
            };
        let pointer = (!sub.is_empty()).then_some(sub_offset as u32);
        write_directory(&mut out, &main, pointer);
        if !sub.is_empty() {
            write_directory(&mut out, &sub, None);
        }
        out.extend_from_slice(&values);
        out
    }

    /// A JPEG header with the TIFF structure in an APP1 segment.
    pub(crate) fn jpeg(tiff: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        // A one pixel grayscale frame, so Pillow can open the file.
        out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 8, 0, 1, 0, 1, 1, 1, 0x11, 0]);
        out.extend_from_slice(&[0xFF, 0xE1]);
        let length = (2 + EXIF_HEADER.len() + tiff.len()) as u16;
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(EXIF_HEADER);
        out.extend_from_slice(tiff);
        out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0xFF, 0xD9]);
        out
    }

    fn date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(year, month, day)
    }

    fn date_of(bytes: &[u8]) -> Option<NaiveDate> {
        let block = exif_block(&mut Reader::new(io::Cursor::new(bytes.to_vec())).ok()?)?;
        let entries = merged_entries(&block)?;
        parse_exif_date(&date_text(&entries)?)
    }

    #[test]
    fn computes_png_checksums() {
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn needs_a_frame_and_the_image_data_in_jpeg_files() {
        let data = tiff(None, Some("2018:12:24 10:00:00"));
        let full = jpeg(&data);
        assert_eq!(date_of(&full), date(2018, 12, 24));
        let without_frame: Vec<u8> = [&full[..2], &full[15..]].concat();
        assert_eq!(date_of(&without_frame), None);
        assert_eq!(date_of(&full[..full.len() - 6]), None);
    }

    #[test]
    fn parses_dates_like_strptime() {
        assert_eq!(parse_exif_date("2021:03:04 05:06:07"), date(2021, 3, 4));
        assert_eq!(parse_exif_date("2021:3:4"), date(2021, 3, 4));
        assert_eq!(parse_exif_date("2021:02:30"), None);
        assert_eq!(parse_exif_date("0000:00:00 00:00:00"), None);
        assert_eq!(parse_exif_date("    :  :     "), None);
        assert_eq!(parse_exif_date(" 2021:03:04"), None);
        assert_eq!(parse_exif_date("2021:03:04:05"), None);
        assert_eq!(parse_exif_date("21:03:04"), None);
        assert_eq!(parse_exif_date("2021:003:04"), None);
    }

    #[test]
    fn prefers_the_original_date() {
        let both = tiff(Some("2020:01:01 00:00:00"), Some("2019:05:06 07:08:09"));
        assert_eq!(date_of(&jpeg(&both)), date(2019, 5, 6));
        let only_main = tiff(Some("2020:01:01 00:00:00"), None);
        assert_eq!(date_of(&jpeg(&only_main)), date(2020, 1, 1));
        let empty_original = tiff(Some("2020:01:01 00:00:00"), Some(""));
        assert_eq!(date_of(&jpeg(&empty_original)), date(2020, 1, 1));
        let invalid_original = tiff(Some("2020:01:01 00:00:00"), Some("unknown"));
        assert_eq!(date_of(&jpeg(&invalid_original)), None);
    }

    /// A PNG file with the given chunks and correct checksums.
    fn png(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = PNG_SIGNATURE.to_vec();
        for (kind, body) in chunks {
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(*kind);
            out.extend_from_slice(body);
            out.extend_from_slice(&crc32(&[*kind, body]).to_be_bytes());
        }
        out
    }

    /// A PNG image header of the given size, 8-bit RGB.
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut header = width.to_be_bytes().to_vec();
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 2, 0, 0, 0]);
        header
    }

    #[test]
    fn reads_png_and_webp() {
        let data = tiff(None, Some("2018:12:24 10:00:00"));
        let after_image = png(&[
            (b"IHDR", png_header(1, 1)),
            (b"IDAT", vec![0u8; 3]),
            (b"eXIf", data.clone()),
            (b"IEND", Vec::new()),
        ]);
        assert_eq!(date_of(&after_image), date(2018, 12, 24));
        let mut broken = after_image.clone();
        broken[PNG_SIGNATURE.len() + 8 + 13] ^= 1;
        assert_eq!(date_of(&broken), None);

        let mut chunks = Vec::new();
        chunks.extend_from_slice(b"VP8X");
        chunks.extend_from_slice(&10u32.to_le_bytes());
        chunks.extend_from_slice(&[0; 10]);
        chunks.extend_from_slice(b"EXIF");
        chunks.extend_from_slice(&(data.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&data);
        if data.len() % 2 == 1 {
            chunks.push(0);
        }
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(&chunks);
        assert_eq!(date_of(&webp), date(2018, 12, 24));
    }

    #[test]
    fn opens_png_files_like_pillow() {
        let data = tiff(None, Some("2018:12:24 10:00:00"));
        let cut_off_chunk = {
            let mut chunk = 100u32.to_be_bytes().to_vec();
            chunk.extend_from_slice(b"tEXt");
            chunk.extend_from_slice(b"short");
            chunk
        };
        // EXIF before the image data is used without reading the rest.
        let mut before = png(&[
            (b"IHDR", png_header(1, 1)),
            (b"eXIf", data.clone()),
            (b"IDAT", vec![0u8; 3]),
        ]);
        before.extend_from_slice(&cut_off_chunk);
        assert_eq!(date_of(&before), date(2018, 12, 24));
        // After the image data, a cut off chunk makes loading the image fail.
        let mut after = png(&[
            (b"IHDR", png_header(1, 1)),
            (b"IDAT", vec![0u8; 3]),
            (b"eXIf", data.clone()),
        ]);
        assert_eq!(date_of(&after), date(2018, 12, 24));
        after.extend_from_slice(&cut_off_chunk);
        assert_eq!(date_of(&after), None);
        // Without an image header, with a bad chunk name or too many pixels Pillow cannot open
        // the file.
        let no_header = png(&[(b"eXIf", data.clone()), (b"IDAT", vec![0u8; 3])]);
        assert_eq!(date_of(&no_header), None);
        let bad_name = png(&[
            (b"IHDR", png_header(1, 1)),
            (b"eX-f", Vec::new()),
            (b"eXIf", data.clone()),
            (b"IDAT", vec![0u8; 3]),
        ]);
        assert_eq!(date_of(&bad_name), None);
        let huge = png(&[
            (b"IHDR", png_header(20_000, 10_000)),
            (b"eXIf", data),
            (b"IDAT", vec![0u8; 3]),
        ]);
        assert_eq!(date_of(&huge), None);
    }

    #[test]
    fn opens_jpeg_files_like_pillow() {
        let data = tiff(None, Some("2018:12:24 10:00:00"));
        let exif = {
            let mut segment = vec![0xFF, 0xE1];
            segment.extend_from_slice(&((2 + EXIF_HEADER.len() + data.len()) as u16).to_be_bytes());
            segment.extend_from_slice(EXIF_HEADER);
            segment.extend_from_slice(&data);
            segment
        };
        let frame = |bits: u8, height: u16, width: u16| {
            let mut segment = vec![0xFF, 0xC0, 0x00, 0x0B, bits];
            segment.extend_from_slice(&height.to_be_bytes());
            segment.extend_from_slice(&width.to_be_bytes());
            segment.extend_from_slice(&[1, 1, 0x11, 0]);
            segment
        };
        let file = |parts: &[&[u8]]| {
            let mut out = vec![0xFF, 0xD8];
            for part in parts {
                out.extend_from_slice(part);
            }
            out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0xFF, 0xD9]);
            out
        };
        let good = frame(8, 1, 1);
        assert_eq!(date_of(&file(&[&exif, &good])), date(2018, 12, 24));
        // Markers without a segment and padding are passed over.
        assert_eq!(
            date_of(&file(&[&exif, &[0xFF, 0xD9, 0xFF, 0xFF], &good])),
            date(2018, 12, 24)
        );
        // 12-bit samples, an empty or a huge image.
        assert_eq!(date_of(&file(&[&exif, &frame(12, 1, 1)])), None);
        assert_eq!(date_of(&file(&[&exif, &frame(8, 0, 1)])), None);
        assert_eq!(date_of(&file(&[&exif, &frame(8, 20_000, 10_000)])), None);
        // An unknown marker, a short quantization table and a short JFIF segment.
        assert_eq!(date_of(&file(&[&exif, &[0xFF, 0x01], &good])), None);
        assert_eq!(
            date_of(&file(&[
                &exif,
                &[0xFF, 0xDB, 0x00, 0x04, 0x00, 0x00],
                &good
            ])),
            None
        );
        assert_eq!(
            date_of(&file(&[
                &[0xFF, 0xE0, 0x00, 0x06, b'J', b'F', b'I', b'F'],
                &exif,
                &good
            ])),
            None
        );
    }

    #[test]
    fn ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let tiff_file = dir.path().join("photo.tiff");
        std::fs::write(&tiff_file, tiff(None, Some("2018:12:24 10:00:00"))).unwrap();
        assert_eq!(exif_date(&tiff_file), None);
        let jpeg_file = dir.path().join("photo.png");
        std::fs::write(&jpeg_file, jpeg(&tiff(None, Some("2018:12:24 10:00:00")))).unwrap();
        assert_eq!(exif_date(&jpeg_file), date(2018, 12, 24));
        assert_eq!(exif_date(&dir.path().join("missing.jpg")), None);
    }
}
