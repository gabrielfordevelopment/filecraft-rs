//! Encoding and metadata handling for the explicitly supported JPEG and PNG formats.

use std::borrow::Cow;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageFormat};
use jpeg_encoder::{ColorType, Encoder, QuantizationTableType, SamplingFactor};

use super::invalid;

// JPEG stores quantization tables in zigzag order. The encoder accepts natural order.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

pub(super) fn save(
    path: &Path,
    image: &DynamicImage,
    format: ImageFormat,
    output: &mut File,
    png_info: Option<png::Info<'static>>,
) -> io::Result<Vec<String>> {
    match format {
        ImageFormat::Jpeg => save_jpeg(path, image, output),
        ImageFormat::Png => save_png(
            image,
            output,
            png_info.ok_or_else(|| invalid("PNG metadata was not loaded."))?,
        ),
        _ => Err(invalid("Only JPEG and PNG images can be processed.")),
    }
}

#[derive(Default)]
struct JpegSettings {
    tables: [Option<[u16; 64]>; 4],
    components: Vec<(u8, u8, u8)>,
    progressive: bool,
    metadata: Vec<(u8, Vec<u8>)>,
}

/// Only headers are read here. Entropy-coded pixel data stays with the image decoder.
fn jpeg_settings(mut input: impl Read) -> io::Result<JpegSettings> {
    let mut soi = [0; 2];
    input.read_exact(&mut soi)?;
    if soi != [0xff, 0xd8] {
        return Err(invalid("Invalid JPEG header."));
    }
    let mut settings = JpegSettings::default();
    let mut total = 0usize;
    loop {
        let mut byte = [0];
        input.read_exact(&mut byte)?;
        if byte[0] != 0xff {
            return Err(invalid("Invalid JPEG marker."));
        }
        while byte[0] == 0xff {
            input.read_exact(&mut byte)?;
        }
        let marker = byte[0];
        if marker == 0xda {
            break;
        }
        if matches!(marker, 0x00 | 0xd8 | 0xd9 | 0xd0..=0xd7) {
            return Err(invalid("Unexpected JPEG marker before image data."));
        }
        let mut size = [0; 2];
        input.read_exact(&mut size)?;
        let size = usize::from(u16::from_be_bytes(size))
            .checked_sub(2)
            .ok_or_else(|| invalid("Invalid JPEG segment size."))?;
        total += size;
        if total > 32 * 1024 * 1024 {
            return Err(invalid("JPEG metadata exceeds the 32 MiB limit."));
        }
        let mut data = vec![0; size];
        input.read_exact(&mut data)?;
        match marker {
            0xdb => {
                let mut position = 0;
                while position < data.len() {
                    let table = data[position];
                    position += 1;
                    // Restrict to 8-bit tables, which the encoder can reproduce exactly.
                    if table >> 4 != 0 || table & 15 > 3 || data.len() - position < 64 {
                        return Err(invalid("Unsupported JPEG quantization table."));
                    }
                    let mut values = [0; 64];
                    for (index, &natural) in ZIGZAG.iter().enumerate() {
                        let value = data[position + index];
                        if value == 0 {
                            return Err(invalid("Invalid zero JPEG quantization value."));
                        }
                        values[natural] = u16::from(value);
                    }
                    settings.tables[usize::from(table & 15)] = Some(values);
                    position += 64;
                }
            }
            0xc0 | 0xc2 => {
                if !settings.components.is_empty() || data.len() < 6 || data[0] != 8 {
                    return Err(invalid("Unsupported JPEG frame."));
                }
                let count = usize::from(data[5]);
                if !matches!(count, 1 | 3) || data.len() != 6 + 3 * count {
                    return Err(invalid(
                        "Only grayscale and YCbCr JPEG images are supported.",
                    ));
                }
                settings.progressive = marker == 0xc2;
                for component in data[6..].as_chunks::<3>().0 {
                    settings
                        .components
                        .push((component[0], component[1], component[2]));
                }
            }
            0xc1 | 0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => {
                return Err(invalid("This JPEG coding mode cannot be preserved."));
            }
            0xe0..=0xef | 0xfe => settings.metadata.push((marker, data)),
            _ => {}
        }
    }
    Ok(settings)
}

fn save_jpeg(path: &Path, image: &DynamicImage, output: &mut File) -> io::Result<Vec<String>> {
    let settings = jpeg_settings(BufReader::new(File::open(path)?))?;
    let components = &settings.components;
    let first = components
        .first()
        .ok_or_else(|| invalid("Missing JPEG frame."))?;
    let grayscale = components.len() == 1;
    let ids = components.iter().map(|c| c.0).collect::<Vec<_>>();
    if !grayscale
        && ((!matches!(ids.as_slice(), [0, 1, 2] | [1, 2, 3]))
            || components[1].1 != 0x11
            || components[2].1 != 0x11
            || components[1].2 != components[2].2)
    {
        return Err(invalid(
            "The original JPEG color layout cannot be preserved.",
        ));
    }
    let sampling = SamplingFactor::from_factors(first.1 >> 4, first.1 & 15)
        .ok_or_else(|| invalid("The JPEG sampling factors cannot be preserved."))?;
    let table = |index: u8| {
        settings
            .tables
            .get(usize::from(index))
            .and_then(|v| *v)
            .ok_or_else(|| invalid("A JPEG quantization table is missing."))
    };
    let luma = table(first.2)?;
    let chroma = if grayscale {
        settings.tables[1].unwrap_or(luma)
    } else {
        table(components[1].2)?
    };
    let width =
        u16::try_from(image.width()).map_err(|_| invalid("The output is too wide for JPEG."))?;
    let height =
        u16::try_from(image.height()).map_err(|_| invalid("The output is too high for JPEG."))?;
    let mut encoded = Vec::new();
    let mut encoder = Encoder::new(&mut encoded, 100);
    encoder.set_sampling_factor(sampling);
    encoder.set_quantization_tables(
        QuantizationTableType::Custom(Box::new(luma)),
        QuantizationTableType::Custom(Box::new(chroma)),
    );
    encoder.set_progressive(settings.progressive);
    if grayscale {
        encoder.encode(image.to_luma8().as_raw(), width, height, ColorType::Luma)
    } else {
        encoder.encode(image.to_rgb8().as_raw(), width, height, ColorType::Rgb)
    }
    .map_err(|e| io::Error::other(e.to_string()))?;

    // Replace the encoder's JFIF header with the original metadata, preserving its density.
    output.write_all(&[0xff, 0xd8])?;
    let mut notes = Vec::new();
    for (marker, mut data) in settings.metadata {
        if marker == 0xe0 && data.starts_with(b"JFIF\0") {
            if data.len() < 14 {
                return Err(invalid("Truncated JFIF metadata."));
            }
            data[12] = 0;
            data[13] = 0;
            data.truncate(14);
        } else if marker == 0xe0 && data.starts_with(b"JFXX\0") {
            notes.push("The old JFIF preview was omitted.".to_owned());
            continue;
        } else if marker == 0xee
            && data.starts_with(b"Adobe")
            && !grayscale
            && data.last() != Some(&1)
        {
            return Err(invalid(
                "The Adobe JPEG color transform cannot be preserved.",
            ));
        } else if marker == 0xe1 && data.starts_with(b"Exif\0\0") {
            update_exif(&mut data[6..], image.dimensions())?;
        } else if marker == 0xe1
            && (data.starts_with(b"http://ns.adobe.com/xap/")
                || data.starts_with(b"http://ns.adobe.com/xmp/"))
        {
            // XMP can contain old orientation, dimensions and embedded previews.
            notes.push(
                "XMP metadata was omitted to avoid stale image dimensions or orientation."
                    .to_owned(),
            );
            continue;
        } else if marker == 0xe2 && data.starts_with(b"MPF\0") {
            return Err(invalid(
                "Multi-picture or HDR JPEG images are not supported.",
            ));
        } else if marker == 0xed {
            notes.push(
                "Photoshop/IPTC metadata was omitted because it may contain an outdated preview."
                    .to_owned(),
            );
            continue;
        }
        write_segment(output, marker, &data)?;
    }
    let mut position = 2;
    while encoded
        .get(position..position + 2)
        .is_some_and(|s| s == [0xff, 0xe0])
    {
        let length = usize::from(u16::from_be_bytes([
            encoded[position + 2],
            encoded[position + 3],
        ]));
        position += 2 + length;
    }
    output.write_all(&encoded[position..])?;
    Ok(notes)
}

fn write_segment(output: &mut impl Write, marker: u8, data: &[u8]) -> io::Result<()> {
    let length = u16::try_from(data.len() + 2)
        .map_err(|_| invalid("JPEG metadata segment is too large."))?;
    output.write_all(&[0xff, marker])?;
    output.write_all(&length.to_be_bytes())?;
    output.write_all(data)
}

/// Read through IEND so trailing EXIF is available without decoding another pixel buffer.
pub(super) fn read_png_info(path: &Path) -> io::Result<png::Info<'static>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > super::MAX_FILE_BYTES {
        return Err(invalid("The image exceeds the 256 MiB input limit."));
    }
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(|e| invalid(&e.to_string()))?;
    reader.finish().map_err(|e| invalid(&e.to_string()))?;
    let mut info = reader.info().clone();
    info.source_gamma = info.gama_chunk.or(info.source_gamma);
    info.source_chromaticities = info.chrm_chunk.or(info.source_chromaticities);
    Ok(info)
}

fn save_png(
    image: &DynamicImage,
    output: &mut File,
    mut info: png::Info<'static>,
) -> io::Result<Vec<String>> {
    if info.animation_control.is_some() {
        return Err(invalid("Animated PNG images are not supported."));
    }
    if info.coding_independent_code_points.is_some()
        || info.mastering_display_color_volume.is_some()
        || info.content_light_level.is_some()
    {
        return Err(invalid("HDR PNG images are not supported."));
    }
    info.width = image.width();
    info.height = image.height();
    info.interlaced = false;
    info.palette = None;
    info.trns = None;
    info.sbit = None;
    info.bkgd = None;
    if let Some(exif) = &mut info.exif_metadata {
        update_exif(exif.to_mut(), image.dimensions())?;
    }
    let mut notes = Vec::new();
    if !info.uncompressed_latin1_text.is_empty()
        || !info.compressed_latin1_text.is_empty()
        || !info.utf8_text.is_empty()
    {
        notes.push("PNG text metadata was omitted because it may contain outdated dimensions or orientation.".to_owned());
        info.uncompressed_latin1_text.clear();
        info.compressed_latin1_text.clear();
        info.utf8_text.clear();
    }
    let (color, depth, bytes) = png_pixels(image);
    info.color_type = color;
    info.bit_depth = depth;
    // Keep ICC, gamma, chromaticities, sRGB, density and corrected EXIF.
    let encoder =
        png::Encoder::with_info(output, info).map_err(|e| io::Error::other(e.to_string()))?;
    let mut writer = encoder
        .write_header()
        .map_err(|e| io::Error::other(e.to_string()))?;
    writer
        .write_image_data(&bytes)
        .map_err(|e| io::Error::other(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(notes)
}

fn png_pixels(image: &DynamicImage) -> (png::ColorType, png::BitDepth, Cow<'_, [u8]>) {
    use image::ColorType;
    let (color, sixteen) = match image.color() {
        ColorType::L8 => (png::ColorType::Grayscale, false),
        ColorType::La8 => (png::ColorType::GrayscaleAlpha, false),
        ColorType::Rgb8 => (png::ColorType::Rgb, false),
        ColorType::L16 => (png::ColorType::Grayscale, true),
        ColorType::La16 => (png::ColorType::GrayscaleAlpha, true),
        ColorType::Rgb16 => (png::ColorType::Rgb, true),
        ColorType::Rgba16 => (png::ColorType::Rgba, true),
        _ => (png::ColorType::Rgba, false),
    };
    if sixteen {
        // DynamicImage stores native endian u16 samples. PNG requires network order.
        let bytes = image
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|b| u16::from_ne_bytes([b[0], b[1]]).to_be_bytes())
            .collect();
        (color, png::BitDepth::Sixteen, Cow::Owned(bytes))
    } else {
        (color, png::BitDepth::Eight, Cow::Borrowed(image.as_bytes()))
    }
}

/// Update standard geometry tags while leaving other EXIF bytes and offsets intact.
/// Disconnect IFD1 so the old embedded thumbnail cannot be displayed as the edited image.
fn update_exif(data: &mut [u8], size: (u32, u32)) -> io::Result<()> {
    let little = match data.get(..4) {
        Some(b"II\x2a\0") => true,
        Some(b"MM\0\x2a") => false,
        _ => return Err(invalid("Invalid EXIF TIFF header.")),
    };
    fn read(data: &[u8], offset: usize, bytes: usize, little: bool) -> io::Result<u32> {
        let slice = data
            .get(offset..offset.saturating_add(bytes))
            .ok_or_else(|| invalid("Invalid EXIF offset."))?;
        Ok(if little {
            slice
                .iter()
                .enumerate()
                .fold(0, |n, (i, b)| n | (u32::from(*b) << (8 * i)))
        } else {
            slice.iter().fold(0, |n, b| (n << 8) | u32::from(*b))
        })
    }
    fn write(
        data: &mut [u8],
        offset: usize,
        bytes: usize,
        value: u32,
        little: bool,
    ) -> io::Result<()> {
        let target = data
            .get_mut(offset..offset.saturating_add(bytes))
            .ok_or_else(|| invalid("Invalid EXIF write offset."))?;
        let encoded = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        target.copy_from_slice(if little {
            &encoded[..bytes]
        } else {
            &encoded[4 - bytes..]
        });
        Ok(())
    }
    let root = read(data, 4, 4, little)? as usize;
    if root < 8 {
        return Err(invalid("Invalid EXIF root directory."));
    }
    let mut queue = vec![root];
    let mut seen = HashSet::new();
    while let Some(offset) = queue.pop() {
        if offset == 0 {
            continue;
        }
        if !seen.insert(offset) || seen.len() > 8 {
            return Err(invalid("Cyclic or excessive EXIF directories."));
        }
        let count = read(data, offset, 2, little)? as usize;
        let end = offset + 2 + count * 12;
        if end.saturating_add(4) > data.len() {
            return Err(invalid("Truncated EXIF directory."));
        }
        for index in 0..count {
            let entry = offset + 2 + index * 12;
            let tag = read(data, entry, 2, little)?;
            let kind = read(data, entry + 2, 2, little)?;
            let count = read(data, entry + 4, 4, little)?;
            if tag == 0x8769 && kind == 4 && count == 1 {
                queue.push(read(data, entry + 8, 4, little)? as usize);
            }
            let value = match tag {
                0x0112 => Some(1),
                0x0100 | 0xa002 => Some(size.0),
                0x0101 | 0xa003 => Some(size.1),
                _ => None,
            };
            if let Some(value) = value {
                if count != 1 || !matches!(kind, 3 | 4) {
                    return Err(invalid("Unsupported EXIF geometry tag."));
                }
                if kind == 3 && value > u32::from(u16::MAX) {
                    return Err(invalid("EXIF dimensions exceed the SHORT tag range."));
                }
                write(
                    data,
                    entry + 8,
                    if kind == 3 { 2 } else { 4 },
                    value,
                    little,
                )?;
            }
        }
        // No linked previews or extra image directories are copied into the result's EXIF tree.
        write(data, end, 4, 0, little)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_tools::{Options, process_folder};
    use std::sync::atomic::AtomicBool;

    fn exif(orientation: u16) -> Vec<u8> {
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend_from_slice(&3u16.to_le_bytes());
        for (tag, value) in [
            (0x112u16, u32::from(orientation)),
            (0x100, 120),
            (0x101, 80),
        ] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&3u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes
    }

    fn jpeg(
        path: &Path,
        sampling: SamplingFactor,
        progressive: bool,
        gray: bool,
        orientation: u16,
    ) {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, 87);
        encoder.set_sampling_factor(sampling);
        encoder.set_progressive(progressive);
        let mut exif = exif(orientation);
        exif.splice(..0, b"Exif\0\0".iter().copied());
        encoder.add_app_segment(1, exif).unwrap();
        encoder
            .add_app_segment(2, b"ICC_PROFILE\0\x01\x01test-profile".to_vec())
            .unwrap();
        let color = if gray {
            ColorType::Luma
        } else {
            ColorType::Rgb
        };
        encoder
            .encode(
                &vec![80; 120 * 80 * if gray { 1 } else { 3 }],
                120,
                80,
                color,
            )
            .unwrap();
        fs_write(path, &bytes);
    }

    fn fs_write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn jpeg_retains_quantization_sampling_progressive_mode_profile_and_density() {
        for (sampling, progressive, gray) in [
            (SamplingFactor::F_2_2, false, false),
            (SamplingFactor::F_1_1, true, false),
            (SamplingFactor::F_2_1, false, false),
            (SamplingFactor::F_1_1, false, true),
        ] {
            let folder = tempfile::tempdir().unwrap();
            let source = folder.path().join("photo.jpg");
            jpeg(&source, sampling, progressive, gray, 1);
            let before = std::fs::read(&source).unwrap();
            let options = Options {
                add_date: false,
                resize: true,
                max_height: Some(40),
                ..Options::default()
            };
            let run = process_folder(
                folder.path(),
                &options,
                None,
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            assert!(run.files[0].outcome.is_ok(), "{:?}", run.files[0]);
            let target = run.output.join("photo.jpg");
            let old = jpeg_settings(before.as_slice()).unwrap();
            let new = jpeg_settings(File::open(&target).unwrap()).unwrap();
            assert_eq!(old.tables, new.tables);
            assert_eq!(old.components, new.components);
            assert_eq!(old.progressive, new.progressive);
            for marker in [0xe0, 0xe2] {
                assert_eq!(
                    old.metadata.iter().find(|(m, _)| *m == marker),
                    new.metadata.iter().find(|(m, _)| *m == marker)
                );
            }
            assert_eq!(image::open(target).unwrap().dimensions(), (60, 40));
            assert_eq!(std::fs::read(source).unwrap(), before);
        }
    }

    #[test]
    fn rotated_jpeg_uses_displayed_dimensions_and_normalizes_exif() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("portrait.jpg");
        jpeg(&source, SamplingFactor::F_2_2, false, false, 6);
        let options = Options {
            add_date: false,
            resize: true,
            max_height: Some(60),
            ..Options::default()
        };
        let preview = crate::image_tools::preview_folder(folder.path(), &options).unwrap();
        assert_eq!(
            preview[0].outcome.as_ref().unwrap().original_size,
            (80, 120)
        );
        let run = process_folder(
            folder.path(),
            &options,
            None,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(run.files[0].outcome.is_ok(), "{:?}", run.files[0]);
        let target = run.output.join("portrait.jpg");
        assert_eq!(image::open(&target).unwrap().dimensions(), (40, 60));
        let settings = jpeg_settings(File::open(target).unwrap()).unwrap();
        let exif = &settings
            .metadata
            .iter()
            .find(|(m, _)| *m == 0xe1)
            .unwrap()
            .1[6..];
        assert_eq!(u16::from_le_bytes(exif[18..20].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(exif[30..32].try_into().unwrap()), 40);
        assert_eq!(u16::from_le_bytes(exif[42..44].try_into().unwrap()), 60);
    }

    #[test]
    fn malformed_exif_is_reported_and_geometry_and_thumbnail_are_corrected() {
        assert!(update_exif(&mut [0; 8], (60, 40)).is_err());
        let mut metadata = exif(8);
        metadata[46..50].copy_from_slice(&50u32.to_le_bytes());
        update_exif(&mut metadata, (60, 40)).unwrap();
        assert_eq!(&metadata[46..50], &[0, 0, 0, 0]);
        assert_eq!(u16::from_le_bytes(metadata[18..20].try_into().unwrap()), 1);
    }

    #[test]
    fn png_keeps_density_gamma_and_profile() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("photo.png");
        let mut info = png::Info::with_size(120, 80);
        info.color_type = png::ColorType::Rgb;
        info.pixel_dims = Some(png::PixelDimensions {
            xppu: 11811,
            yppu: 11811,
            unit: png::Unit::Meter,
        });
        info.source_gamma = Some(png::ScaledFloat::new(0.45455));
        info.icc_profile = Some(Cow::Borrowed(b"test-profile"));
        let mut writer = png::Encoder::with_info(File::create(&source).unwrap(), info)
            .unwrap()
            .write_header()
            .unwrap();
        writer.write_image_data(&vec![80; 120 * 80 * 3]).unwrap();
        writer.finish().unwrap();
        let options = Options {
            add_date: false,
            resize: true,
            max_height: Some(40),
            ..Options::default()
        };
        let run = process_folder(
            folder.path(),
            &options,
            None,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(run.files[0].outcome.is_ok(), "{:?}", run.files[0]);
        let reader = png::Decoder::new(BufReader::new(
            File::open(run.output.join("photo.png")).unwrap(),
        ))
        .read_info()
        .unwrap();
        assert_eq!(reader.info().pixel_dims.unwrap().xppu, 11811);
        assert_eq!(
            reader.info().icc_profile.as_deref(),
            Some(b"test-profile".as_slice())
        );
        assert_eq!(
            reader.info().gama_chunk,
            Some(png::ScaledFloat::new(0.45455))
        );
        assert_eq!(reader.info().size(), (60, 40));
    }

    #[test]
    fn png_orientation_before_or_after_pixels_matches_preview_and_saved_geometry() {
        for trailing in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let source = folder.path().join("photo.png");
            let mut info = png::Info::with_size(120, 80);
            info.color_type = png::ColorType::Rgb;
            if !trailing {
                info.exif_metadata = Some(Cow::Owned(exif(6)));
            }
            let mut writer = png::Encoder::with_info(File::create(&source).unwrap(), info)
                .unwrap()
                .write_header()
                .unwrap();
            let pixels: Vec<u8> = (0..80)
                .flat_map(|_| (0..120).flat_map(|x| if x < 60 { [255, 0, 0] } else { [0, 0, 255] }))
                .collect();
            writer.write_image_data(&pixels).unwrap();
            if trailing {
                writer.write_chunk(png::chunk::eXIf, &exif(6)).unwrap();
            }
            writer.finish().unwrap();
            let original = std::fs::read(&source).unwrap();
            for max_height in [60, 200] {
                let options = Options {
                    add_date: false,
                    resize: true,
                    max_width: None,
                    max_height: Some(max_height),
                    ..Options::default()
                };
                let preview = crate::image_tools::preview_folder(folder.path(), &options).unwrap();
                let expected = if max_height == 60 {
                    (40, 60)
                } else {
                    (80, 120)
                };
                let item = preview[0].outcome.as_ref().unwrap();
                assert_eq!(item.original_size, (80, 120), "trailing EXIF: {trailing}");
                assert_eq!(item.output_size, expected);
                let run = process_folder(
                    folder.path(),
                    &options,
                    None,
                    &AtomicBool::new(false),
                    |_| {},
                )
                .unwrap();
                let result = run.files[0].outcome.as_ref().unwrap();
                assert_eq!(result.original_size, (80, 120));
                assert_eq!(result.output_size, expected);
                let output = run.output.join("photo.png");
                if max_height == 60 {
                    let image = image::open(&output).unwrap().to_rgb8();
                    assert_eq!(image.dimensions(), (40, 60));
                    assert_eq!(image.get_pixel(10, 10).0, [255, 0, 0]);
                    assert_eq!(image.get_pixel(10, 50).0, [0, 0, 255]);
                    let mut decoder = image::ImageReader::open(&output)
                        .unwrap()
                        .with_guessed_format()
                        .unwrap()
                        .into_decoder()
                        .unwrap();
                    use image::ImageDecoder;
                    assert_eq!(
                        decoder.orientation().unwrap(),
                        image::metadata::Orientation::NoTransforms
                    );
                } else {
                    assert!(result.copied);
                    assert_eq!(std::fs::read(output).unwrap(), original);
                }
                assert_eq!(std::fs::read(&source).unwrap(), original);
            }
        }
    }
}
