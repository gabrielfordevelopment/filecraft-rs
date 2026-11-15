//! Independent, local image processing. Source files are only opened for reading.
//! Each run owns a new child directory and publishes complete files without replacing any path.

mod metadata;

use std::fs::{self, File};
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont, point};
use chrono::NaiveDate;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Rgba};

use crate::path_key::name_key;

pub const OUTPUT_FOLDER: &str = "edited-images";
pub const DEFAULT_MAX_DIMENSION: u32 = 2100;
const MAX_PIXELS: u64 = 50_000_000;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Font bytes are supplied by the desktop's existing system font resolver.
#[derive(Clone)]
pub struct StampFont(FontArc);

impl StampFont {
    pub fn new(bytes: Vec<u8>, index: u32) -> io::Result<Self> {
        ab_glyph::FontVec::try_from_vec_and_index(bytes, index)
            .map(|font| Self(FontArc::new(font)))
            .map_err(|_| invalid("The system font could not be read."))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub add_date: bool,
    pub resize: bool,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// Visible glyph height as a percentage of the final reference dimension.
    pub text_height_percent: f32,
    /// Both margins use the same final reference dimension as the glyph height.
    pub margin_percent: f32,
    /// Use the shorter side of the displayed image instead of its height for percentages.
    pub match_orientation: bool,
    pub corner: StampCorner,
}

/// Position in the displayed image, after applying its orientation and resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StampCorner {
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            add_date: true,
            resize: false,
            max_width: Some(DEFAULT_MAX_DIMENSION),
            max_height: Some(DEFAULT_MAX_DIMENSION),
            text_height_percent: 3.0,
            margin_percent: 5.0,
            match_orientation: true,
            corner: StampCorner::BottomLeft,
        }
    }
}

impl Options {
    pub fn validate(&self) -> io::Result<()> {
        if !self.add_date && !self.resize {
            return Err(invalid("Select date stamping, resizing, or both."));
        }
        if self.resize
            && ((self.max_width.is_none() && self.max_height.is_none())
                || self.max_width == Some(0)
                || self.max_height == Some(0))
        {
            return Err(invalid("Enter at least one positive size limit."));
        }
        if self.add_date
            && (!self.text_height_percent.is_finite()
                || !(0.1..=10.0).contains(&self.text_height_percent)
                || !self.margin_percent.is_finite()
                || !(0.0..=25.0).contains(&self.margin_percent))
        {
            return Err(invalid(
                "Text height must be 0.1-10% and margin must be 0-25%.",
            ));
        }
        Ok(())
    }
}

/// Recognizes the current file name only. No EXIF or file-time fallback is used.
/// This deliberately does not change or depend on the file renaming date policy.
pub fn date_from_name(path: &Path) -> Option<NaiveDate> {
    let stem = path.file_stem()?.to_str()?.as_bytes();
    for start in 0..stem.len() {
        if start > 0 && stem[start - 1].is_ascii_digit() {
            continue;
        }
        let tail = &stem[start..];
        let (year, month, day, length) =
            if tail.len() >= 10 && matches!(tail[4], b'_' | b'-' | b'.') && tail[7] == tail[4] {
                (&tail[..4], &tail[5..7], &tail[8..10], 10)
            } else if tail.len() >= 8 {
                (&tail[..4], &tail[4..6], &tail[6..8], 8)
            } else {
                continue;
            };
        if tail.get(length).is_some_and(u8::is_ascii_digit) {
            continue;
        }
        let number = |digits: &[u8]| {
            digits.iter().try_fold(0u32, |n, d| {
                d.is_ascii_digit().then(|| n * 10 + u32::from(d - b'0'))
            })
        };
        if let (Some(y), Some(m), Some(d)) = (number(year), number(month), number(day))
            && let Some(date) = NaiveDate::from_ymd_opt(y as i32, m, d)
            && y > 0
        {
            return Some(date);
        }
    }
    None
}

/// Fits within both limits, never enlarging. Integer rounding cannot exceed either limit.
pub fn fitted_size(width: u32, height: u32, options: &Options) -> (u32, u32) {
    if !options.resize || width == 0 || height == 0 {
        return (width, height);
    }
    let max_w = options.max_width.unwrap_or(width).min(width);
    let max_h = options.max_height.unwrap_or(height).min(height);
    if u64::from(max_w) * u64::from(height) <= u64::from(max_h) * u64::from(width) {
        (
            max_w,
            ((u64::from(height) * u64::from(max_w)) / u64::from(width)).max(1) as u32,
        )
    } else {
        (
            ((u64::from(width) * u64::from(max_h)) / u64::from(height)).max(1) as u32,
            max_h,
        )
    }
}

#[derive(Debug, Clone)]
pub struct FileResult {
    pub name: String,
    pub outcome: Result<Processed, String>,
}

#[derive(Debug, Clone)]
pub struct Processed {
    pub original_size: (u32, u32),
    pub output_size: (u32, u32),
    pub date: Option<NaiveDate>,
    pub copied: bool,
    pub notes: Vec<String>,
}

pub struct RunResult {
    pub output: PathBuf,
    pub files: Vec<FileResult>,
    pub total: usize,
    pub cancelled: bool,
}

/// Reports the initial workload once, then every completed file, including failures.
pub enum ProgressEvent {
    Listed { total: usize },
    File(FileResult),
}

/// Lists only regular JPEG/PNG files directly in the chosen source folder.
/// Links and subfolders, including outputs of previous runs, are never followed.
pub fn source_files(source: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| {
                    ["jpg", "jpeg", "png"]
                        .iter()
                        .any(|ext| s.eq_ignore_ascii_case(ext))
                })
        {
            files.push(entry.path());
        }
    }
    files.sort_by_key(|path| name_key(&path.file_name().unwrap_or_default().to_string_lossy()));
    Ok(files)
}

/// Header-only preview. It neither decodes pixels nor creates output files.
pub fn preview_folder(source: &Path, options: &Options) -> io::Result<Vec<FileResult>> {
    options.validate()?;
    source_files(source)?
        .into_iter()
        .map(|path| {
            let outcome = (|| {
                let reader = ImageReader::open(&path)?.with_guessed_format()?;
                if !matches!(reader.format(), Some(ImageFormat::Jpeg | ImageFormat::Png)) {
                    return Err(invalid("The file is not a JPEG or PNG image."));
                }
                let mut decoder = reader.into_decoder().map_err(image_error)?;
                let (mut width, mut height) = decoder.dimensions();
                let orientation = decoder.orientation().map_err(image_error)?;
                use image::metadata::Orientation;
                if matches!(
                    orientation,
                    Orientation::Rotate90
                        | Orientation::Rotate270
                        | Orientation::Rotate90FlipH
                        | Orientation::Rotate270FlipH
                ) {
                    std::mem::swap(&mut width, &mut height);
                }
                let date = if options.add_date {
                    Some(
                        date_from_name(&path)
                            .ok_or_else(|| invalid("No valid date was found in the file name."))?,
                    )
                } else {
                    None
                };
                let output_size = fitted_size(width, height, options);
                Ok(Processed {
                    original_size: (width, height),
                    output_size,
                    date,
                    copied: date.is_none() && output_size == (width, height),
                    notes: Vec::new(),
                })
            })()
            .map_err(|error: io::Error| error.to_string());
            Ok(FileResult {
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                outcome,
            })
        })
        .collect()
}

pub fn process_folder(
    source: &Path,
    options: &Options,
    font: Option<&StampFont>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(ProgressEvent),
) -> io::Result<RunResult> {
    options.validate()?;
    if options.add_date && font.is_none() {
        return Err(invalid("No system font is available for the date stamp."));
    }
    let paths = source_files(source)?;
    if paths.is_empty() {
        return Err(invalid(
            "No JPEG or PNG files were found directly in this folder.",
        ));
    }
    let total = paths.len();
    progress(ProgressEvent::Listed { total });
    let output = create_output(source)?;
    let mut files = Vec::new();
    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let result = FileResult {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            outcome: process_file(&path, &output, options, font).map_err(|e| e.to_string()),
        };
        progress(ProgressEvent::File(result.clone()));
        files.push(result);
    }
    Ok(RunResult {
        output,
        files,
        total,
        cancelled: cancel.load(Ordering::Relaxed),
    })
}

fn create_output(source: &Path) -> io::Result<PathBuf> {
    for index in 0..10_000 {
        let name = if index == 0 {
            OUTPUT_FOLDER.to_owned()
        } else {
            format!("{OUTPUT_FOLDER}-{index}")
        };
        let path = source.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(invalid("No unused output folder name is available."))
}

fn process_file(
    path: &Path,
    output: &Path,
    options: &Options,
    font: Option<&StampFont>,
) -> io::Result<Processed> {
    let date = if options.add_date {
        Some(date_from_name(path).ok_or_else(|| {
            invalid("No valid date was found in the file name. The image was skipped.")
        })?)
    } else {
        None
    };
    let source = File::open(path)?;
    if source.metadata()?.len() > MAX_FILE_BYTES {
        return Err(invalid("The image exceeds the 256 MiB input limit."));
    }
    let reader = ImageReader::new(BufReader::new(source)).with_guessed_format()?;
    let format = reader
        .format()
        .ok_or_else(|| invalid("Unknown image format."))?;
    let expected = if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };
    if format != expected {
        return Err(invalid(
            "The image format does not match its file extension.",
        ));
    }
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(invalid(
            "The image exceeds the 50 megapixel processing limit.",
        ));
    }
    let orientation = decoder.orientation().map_err(image_error)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
    image.apply_orientation(orientation);
    let original_size = image.dimensions();
    let output_size = fitted_size(original_size.0, original_size.1, options);
    let copied = date.is_none() && output_size == original_size;
    let mut notes = Vec::new();
    let mut temporary = tempfile::NamedTempFile::new_in(output)?;
    if copied {
        io::copy(&mut File::open(path)?, temporary.as_file_mut())?;
    } else {
        if output_size != original_size {
            image = image.resize_exact(
                output_size.0,
                output_size.1,
                image::imageops::FilterType::Lanczos3,
            );
        }
        if let Some(date) = date {
            stamp(
                &mut image,
                &date.format("%Y.%m.%d").to_string(),
                options,
                font.ok_or_else(|| invalid("No date stamp font is available."))?,
            )?;
        }
        notes = metadata::save(path, &image, format, temporary.as_file_mut())?;
    }
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(
            output.join(
                path.file_name()
                    .ok_or_else(|| invalid("Missing file name."))?,
            ),
        )
        .map_err(|error| error.error)?;
    Ok(Processed {
        original_size,
        output_size,
        date,
        copied,
        notes,
    })
}

/// A grayscale coverage mask keeps font colors separate from UI theme colors.
/// The exported stamp is white with a black outline in both application themes.
fn stamp(
    image: &mut DynamicImage,
    text: &str,
    options: &Options,
    font: &StampFont,
) -> io::Result<()> {
    *image = if image.color().bits_per_pixel() / u16::from(image.color().channel_count()) > 8 {
        DynamicImage::ImageRgba16(image.to_rgba16())
    } else {
        DynamicImage::ImageRgba8(image.to_rgba8())
    };
    let reference_dimension = if options.match_orientation {
        image.width().min(image.height())
    } else {
        image.height()
    } as f32;
    let target = (reference_dimension * options.text_height_percent / 100.0)
        .round()
        .max(1.0);
    let scaled = font.0.as_scaled(PxScale::from(100.0));
    let mut x = 0.0;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if id.0 == 0 {
            return Err(invalid(
                "The system font does not contain all date characters.",
            ));
        }
        glyphs.push(Glyph {
            id,
            scale: PxScale::from(100.0),
            position: point(x, scaled.ascent()),
        });
        x += scaled.h_advance(id);
    }
    let bounds = |glyphs: &[Glyph]| {
        glyphs
            .iter()
            .filter_map(|g| font.0.outline_glyph(g.clone()).map(|g| g.px_bounds()))
            .reduce(|a, b| ab_glyph::Rect {
                min: point(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                max: point(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
            })
    };
    let base = bounds(&glyphs)
        .ok_or_else(|| invalid("The date could not be rendered with the system font."))?;
    let factor = target / base.height();
    for glyph in &mut glyphs {
        glyph.scale = PxScale::from(100.0 * factor);
        glyph.position.x *= factor;
        glyph.position.y *= factor;
    }
    let rect = bounds(&glyphs).ok_or_else(|| invalid("The date has no visible glyphs."))?;
    let stroke = (target * 0.08).round().max(1.0) as i32;
    let mask_w = rect.width() as u32 + 2 * stroke as u32;
    let mask_h = rect.height() as u32 + 2 * stroke as u32;
    let margin = (reference_dimension * options.margin_percent / 100.0).round() as u32;
    if mask_w.saturating_add(margin.saturating_mul(2)) > image.width()
        || mask_h.saturating_add(margin.saturating_mul(2)) > image.height()
    {
        return Err(invalid(
            "The date stamp does not fit. Reduce its height or margin.",
        ));
    }
    let mut mask = vec![0.0f32; (mask_w * mask_h) as usize];
    for glyph in glyphs {
        if let Some(outlined) = font.0.outline_glyph(glyph) {
            let b = outlined.px_bounds();
            outlined.draw(|x, y, coverage| {
                let x = x + (b.min.x - rect.min.x) as u32 + stroke as u32;
                let y = y + (b.min.y - rect.min.y) as u32 + stroke as u32;
                let cell = &mut mask[(y * mask_w + x) as usize];
                *cell = cell.max(coverage);
            });
        }
    }
    let origin_x = match options.corner {
        StampCorner::TopLeft | StampCorner::BottomLeft => margin,
        StampCorner::TopRight | StampCorner::BottomRight => image.width() - margin - mask_w,
    };
    let origin_y = match options.corner {
        StampCorner::TopLeft | StampCorner::TopRight => margin,
        StampCorner::BottomLeft | StampCorner::BottomRight => image.height() - margin - mask_h,
    };
    let outline_mask = dilate(&mask, mask_w as usize, mask_h as usize, stroke as usize);
    for y in 0..mask_h {
        for x in 0..mask_w {
            let fill = mask[(y * mask_w + x) as usize];
            let outline = outline_mask[(y * mask_w + x) as usize];
            if outline > 0.0 {
                blend(image, origin_x + x, origin_y + y, 0.0, outline);
            }
            if fill > 0.0 {
                blend(image, origin_x + x, origin_y + y, 1.0, fill);
            }
        }
    }
    Ok(())
}

/// Separable sliding maximum, linear in mask size even for large date stamps.
fn dilate(mask: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    use std::collections::VecDeque;
    let mut horizontal = vec![0.0; mask.len()];
    let mut output = vec![0.0; mask.len()];
    for line in 0..height {
        let start = line * width;
        let mut queue = VecDeque::<usize>::new();
        let mut next = 0;
        for i in 0..width {
            while next < width && next <= i + radius {
                while queue
                    .back()
                    .is_some_and(|&j| mask[start + j] <= mask[start + next])
                {
                    queue.pop_back();
                }
                queue.push_back(next);
                next += 1;
            }
            while queue.front().is_some_and(|&j| j + radius < i) {
                queue.pop_front();
            }
            horizontal[start + i] = mask[start + queue[0]];
        }
    }
    for x in 0..width {
        let mut queue = VecDeque::<usize>::new();
        let mut next = 0;
        for y in 0..height {
            while next < height && next <= y + radius {
                while queue
                    .back()
                    .is_some_and(|&j| horizontal[j * width + x] <= horizontal[next * width + x])
                {
                    queue.pop_back();
                }
                queue.push_back(next);
                next += 1;
            }
            while queue.front().is_some_and(|&j| j + radius < y) {
                queue.pop_front();
            }
            output[y * width + x] = horizontal[queue[0] * width + x];
        }
    }
    output
}

/// Preserve 16-bit PNG samples and alpha rather than converting the entire image to RGBA8.
fn blend(image: &mut DynamicImage, x: u32, y: u32, value: f32, coverage: f32) {
    use image::Pixel;
    fn over<T: image::Primitive + Into<f64>>(
        pixel: &mut [T],
        value: f32,
        coverage: f32,
        max: f64,
        convert: impl Fn(f64) -> T,
    ) {
        let alpha = pixel[3].into() / max;
        let out_alpha = f64::from(coverage) + alpha * (1.0 - f64::from(coverage));
        for channel in &mut pixel[..3] {
            let out = (f64::from(value * coverage)
                + (*channel).into() / max * alpha * (1.0 - f64::from(coverage)))
                / out_alpha;
            *channel = convert((out * max).round());
        }
        pixel[3] = convert((out_alpha * max).round());
    }
    if let DynamicImage::ImageRgba16(buffer) = image {
        over(
            buffer.get_pixel_mut(x, y).channels_mut(),
            value,
            coverage,
            65535.0,
            |n| n as u16,
        );
    } else {
        if !matches!(image, DynamicImage::ImageRgba8(_)) {
            *image = DynamicImage::ImageRgba8(image.to_rgba8());
        }
        if let DynamicImage::ImageRgba8(buffer) = image {
            let pixel: &mut Rgba<u8> = buffer.get_pixel_mut(x, y);
            over(pixel.channels_mut(), value, coverage, 255.0, |n| n as u8);
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn image_error(error: image::ImageError) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests;
