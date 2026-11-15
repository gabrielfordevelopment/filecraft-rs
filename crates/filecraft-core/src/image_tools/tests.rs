use super::*;
use ab_glyph::{GlyphId, Outline, OutlineCurve};
use image::{ImageBuffer, RgbaImage};

fn resize_options(width: Option<u32>, height: Option<u32>) -> Options {
    Options {
        add_date: false,
        resize: true,
        max_width: width,
        max_height: height,
        ..Options::default()
    }
}

fn make_png(path: &Path, width: u32, height: u32) {
    RgbaImage::from_pixel(width, height, Rgba([41, 103, 167, 255]))
        .save(path)
        .unwrap();
}

fn run(source: &Path, options: &Options) -> RunResult {
    process_folder(source, options, None, &AtomicBool::new(false), |_| {}).unwrap()
}

#[test]
fn date_policy_is_independent_and_validates_calendar_dates() {
    for name in [
        "2026_04_05_002.jpg",
        "IMG-2026-04-05.jpeg",
        "2026.04.05.png",
        "IMG_20260405_001.jpg",
        "holiday_2026-02-30_2026-04-05.jpg",
    ] {
        assert_eq!(
            date_from_name(Path::new(name)),
            NaiveDate::from_ymd_opt(2026, 4, 5),
            "{name}"
        );
    }
    for name in [
        "no-date.jpg",
        "2026_02_30.jpg",
        "2026_04-05.jpg",
        "120260405.jpg",
        "202604051.jpg",
        "2026.04.jpg",
    ] {
        assert_eq!(date_from_name(Path::new(name)), None, "{name}");
    }
}

#[test]
fn fitting_handles_single_limits_landscape_portrait_and_no_upscaling() {
    assert_eq!(
        fitted_size(3000, 4000, &resize_options(None, Some(1500))),
        (1125, 1500)
    );
    assert_eq!(
        fitted_size(4000, 3000, &resize_options(Some(1500), Some(1500))),
        (1500, 1125)
    );
    assert_eq!(
        fitted_size(3000, 4000, &resize_options(Some(1500), Some(1500))),
        (1125, 1500)
    );
    assert_eq!(
        fitted_size(400, 300, &resize_options(Some(1500), Some(1500))),
        (400, 300)
    );
    assert_eq!(
        fitted_size(1, 4000, &resize_options(None, Some(1500))),
        (1, 1500)
    );
    assert_eq!(
        fitted_size(4000, 1, &resize_options(Some(1500), None)),
        (1500, 1)
    );
}

#[test]
fn invalid_options_do_not_create_an_output_folder() {
    let source = tempfile::tempdir().unwrap();
    let options = resize_options(None, None);
    assert!(
        process_folder(
            source.path(),
            &options,
            None,
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(source.path()).unwrap().count(), 0);
    let options = Options {
        text_height_percent: f32::NAN,
        ..Options::default()
    };
    assert!(options.validate().is_err());
}

#[test]
fn resizing_preserves_original_bytes_and_uses_new_child_folders() {
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("photo.png");
    make_png(&path, 120, 160);
    let original = fs::read(&path).unwrap();
    let original_time = fs::metadata(&path).unwrap().modified().unwrap();
    let options = resize_options(None, Some(80));
    let first = run(source.path(), &options);
    assert!(first.files[0].outcome.is_ok());
    assert_eq!(
        image::open(first.output.join("photo.png"))
            .unwrap()
            .dimensions(),
        (60, 80)
    );
    let first_bytes = fs::read(first.output.join("photo.png")).unwrap();
    let second = run(source.path(), &options);
    assert_eq!(first.output.parent(), Some(source.path()));
    assert_ne!(first.output, second.output);
    assert_eq!(first.files.len(), 1);
    assert_eq!(second.files.len(), 1);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        original_time
    );
    assert_eq!(
        fs::read(first.output.join("photo.png")).unwrap(),
        first_bytes
    );
}

#[test]
fn no_resize_needed_is_an_exact_file_copy() {
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("small.png");
    make_png(&path, 40, 30);
    let result = run(source.path(), &resize_options(Some(100), Some(100)));
    assert!(result.files[0].outcome.as_ref().unwrap().copied);
    assert_eq!(
        fs::read(path).unwrap(),
        fs::read(result.output.join("small.png")).unwrap()
    );
}

#[test]
fn cancellation_never_publishes_an_incomplete_file() {
    let source = tempfile::tempdir().unwrap();
    for name in ["a.png", "b.png"] {
        make_png(&source.path().join(name), 40, 30);
    }
    let cancel = AtomicBool::new(false);
    let result = process_folder(
        source.path(),
        &resize_options(None, Some(15)),
        None,
        &cancel,
        |_| cancel.store(true, Ordering::Relaxed),
    )
    .unwrap();
    assert!(result.cancelled);
    assert_eq!(result.files.len(), 1);
    assert_eq!(fs::read_dir(result.output).unwrap().count(), 1);
    assert_eq!(fs::read_dir(source.path()).unwrap().count(), 3);
}

#[test]
fn bad_images_do_not_stop_other_files_or_leave_partial_outputs() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("a.png"), b"not a PNG").unwrap();
    make_png(&source.path().join("b.png"), 40, 30);
    let result = run(source.path(), &resize_options(None, Some(15)));
    assert!(result.files[0].outcome.is_err());
    assert!(result.files[1].outcome.is_ok());
    assert_eq!(fs::read_dir(result.output).unwrap().count(), 1);
}

#[test]
fn listing_is_not_recursive_and_excludes_non_images() {
    let source = tempfile::tempdir().unwrap();
    fs::create_dir(source.path().join("nested")).unwrap();
    make_png(&source.path().join("nested").join("nested.png"), 40, 30);
    fs::write(source.path().join("notes.txt"), b"hello").unwrap();
    make_png(&source.path().join("image.PNG"), 40, 30);
    assert_eq!(
        source_files(source.path()).unwrap(),
        vec![source.path().join("image.PNG")]
    );
}

#[test]
fn preview_reads_headers_without_creating_output() {
    let source = tempfile::tempdir().unwrap();
    make_png(&source.path().join("2026_04_05_001.png"), 120, 160);
    let mut options = resize_options(None, Some(80));
    options.add_date = true;
    let rows = preview_folder(source.path(), &options).unwrap();
    let item = rows[0].outcome.as_ref().unwrap();
    assert_eq!(item.output_size, (60, 80));
    assert_eq!(item.date, NaiveDate::from_ymd_opt(2026, 4, 5));
    assert_eq!(fs::read_dir(source.path()).unwrap().count(), 1);
}

/// Synthetic rectangle glyphs exercise placement and compositing without distributing a font.
struct RectangleFont;
impl Font for RectangleFont {
    fn units_per_em(&self) -> Option<f32> {
        Some(100.0)
    }
    fn ascent_unscaled(&self) -> f32 {
        100.0
    }
    fn descent_unscaled(&self) -> f32 {
        0.0
    }
    fn line_gap_unscaled(&self) -> f32 {
        0.0
    }
    fn glyph_id(&self, _: char) -> GlyphId {
        GlyphId(1)
    }
    fn h_advance_unscaled(&self, _: GlyphId) -> f32 {
        60.0
    }
    fn h_side_bearing_unscaled(&self, _: GlyphId) -> f32 {
        0.0
    }
    fn v_advance_unscaled(&self, _: GlyphId) -> f32 {
        100.0
    }
    fn v_side_bearing_unscaled(&self, _: GlyphId) -> f32 {
        0.0
    }
    fn kern_unscaled(&self, _: GlyphId, _: GlyphId) -> f32 {
        0.0
    }
    fn glyph_count(&self) -> usize {
        2
    }
    fn codepoint_ids(&self) -> ab_glyph::CodepointIdIter<'_> {
        unreachable!("Stamp rendering does not enumerate code points")
    }
    fn glyph_raster_image2(&self, _: GlyphId, _: u16) -> Option<ab_glyph::v2::GlyphImage<'_>> {
        None
    }
    fn outline(&self, _: GlyphId) -> Option<Outline> {
        let points = [
            point(0.0, 0.0),
            point(50.0, 0.0),
            point(50.0, 100.0),
            point(0.0, 100.0),
        ];
        Some(Outline {
            bounds: ab_glyph::Rect {
                min: point(0.0, 100.0),
                max: point(50.0, 0.0),
            },
            curves: (0..4)
                .map(|i| OutlineCurve::Line(points[i], points[(i + 1) % 4]))
                .collect(),
        })
    }
}

fn font() -> StampFont {
    StampFont(FontArc::new(RectangleFont))
}

#[test]
fn stamp_changes_only_its_bottom_left_region_and_keeps_originals() {
    let source = tempfile::tempdir().unwrap();
    let name = "2026_04_05_001.png";
    let path = source.path().join(name);
    make_png(&path, 1000, 1000);
    let before = fs::read(&path).unwrap();
    let result = process_folder(
        source.path(),
        &Options::default(),
        Some(&font()),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(result.files[0].outcome.is_ok(), "{:?}", result.files[0]);
    let image = image::open(result.output.join(name)).unwrap().to_rgba8();
    let changed: Vec<_> = image
        .enumerate_pixels()
        .filter(|(_, _, p)| **p != Rgba([41, 103, 167, 255]))
        .collect();
    assert!(!changed.is_empty());
    assert_eq!(changed.iter().map(|(x, _, _)| *x).min(), Some(40));
    assert_eq!(changed.iter().map(|(_, y, _)| *y).max(), Some(959));
    assert!(changed.iter().all(|(x, y, _)| *x < 200 && *y > 930));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn resize_then_stamp_uses_final_dimensions() {
    let source = tempfile::tempdir().unwrap();
    let name = "2026_04_05_001.png";
    make_png(&source.path().join(name), 2000, 2000);
    let options = Options {
        resize: true,
        max_height: Some(1000),
        ..Options::default()
    };
    let result = process_folder(
        source.path(),
        &options,
        Some(&font()),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let output = image::open(result.output.join(name)).unwrap().to_rgba8();
    assert_eq!(output.dimensions(), (1000, 1000));
    let changed: Vec<_> = output
        .enumerate_pixels()
        .filter(|(_, _, p)| **p != Rgba([41, 103, 167, 255]))
        .collect();
    assert_eq!(changed.iter().map(|(x, _, _)| *x).min(), Some(40));
    assert_eq!(changed.iter().map(|(_, y, _)| *y).max(), Some(959));
}

#[test]
fn missing_date_is_reported_without_fallback_or_output() {
    let source = tempfile::tempdir().unwrap();
    make_png(&source.path().join("photo.png"), 200, 200);
    let result = process_folder(
        source.path(),
        &Options::default(),
        Some(&font()),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(
        result.files[0]
            .outcome
            .as_ref()
            .unwrap_err()
            .contains("No valid date")
    );
    assert_eq!(fs::read_dir(result.output).unwrap().count(), 0);
}

#[test]
fn stamping_keeps_16_bit_samples_and_transparency_outside_the_stamp() {
    let source = tempfile::tempdir().unwrap();
    let name = "2026_04_05.png";
    let image = DynamicImage::ImageRgba16(ImageBuffer::from_pixel(
        1000,
        1000,
        Rgba([12345u16, 23456, 34567, 45678]),
    ));
    image.save(source.path().join(name)).unwrap();
    let result = process_folder(
        source.path(),
        &Options::default(),
        Some(&font()),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let output = image::open(result.output.join(name)).unwrap();
    assert_eq!(output.color(), image::ColorType::Rgba16);
    assert_eq!(
        output.to_rgba16().get_pixel(500, 500).0,
        [12345, 23456, 34567, 45678]
    );
}

#[test]
fn small_images_fail_cleanly_when_the_stamp_cannot_fit() {
    let source = tempfile::tempdir().unwrap();
    make_png(&source.path().join("2026_04_05.png"), 1, 1);
    let result = process_folder(
        source.path(),
        &Options::default(),
        Some(&font()),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(result.files[0].outcome.is_err());
    assert_eq!(fs::read_dir(result.output).unwrap().count(), 0);
}

#[test]
fn mask_dilation_handles_edges_without_wrapping_rows() {
    let mask = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    assert_eq!(
        dilate(&mask, 3, 3, 1),
        vec![1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn publishing_never_replaces_an_existing_destination() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = source.path().join("photo.png");
    make_png(&path, 120, 80);
    let target = output.path().join("photo.png");
    fs::write(&target, b"existing destination").unwrap();
    assert!(process_file(&path, output.path(), &resize_options(None, Some(40)), None).is_err());
    assert_eq!(fs::read(target).unwrap(), b"existing destination");
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
}

#[test]
fn combined_jpeg_processing_preserves_extension_and_source_bytes() {
    let source = tempfile::tempdir().unwrap();
    let name = "2026_04_05_001.jpeg";
    let path = source.path().join(name);
    let mut bytes = Vec::new();
    jpeg_encoder::Encoder::new(&mut bytes, 90)
        .encode(
            &vec![80; 1000 * 800 * 3],
            1000,
            800,
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
    fs::write(&path, &bytes).unwrap();
    let options = Options {
        resize: true,
        max_height: Some(400),
        ..Options::default()
    };
    let result = process_folder(
        source.path(),
        &options,
        Some(&font()),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(result.files[0].outcome.is_ok(), "{:?}", result.files[0]);
    let output = result.output.join(name);
    assert_eq!(
        image::ImageReader::open(&output)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .format(),
        Some(ImageFormat::Jpeg)
    );
    assert_eq!(image::open(output).unwrap().dimensions(), (500, 400));
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn all_stamp_corners_use_the_same_margins_and_preserve_other_pixels() {
    assert_eq!(Options::default().corner, StampCorner::BottomLeft);
    let original = Rgba([41, 103, 167, 255]);
    for corner in [
        StampCorner::TopLeft,
        StampCorner::TopRight,
        StampCorner::BottomLeft,
        StampCorner::BottomRight,
    ] {
        let mut image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1000, 1000, original));
        let options = Options {
            corner,
            ..Options::default()
        };
        stamp(&mut image, "2026.04.05", &options, &font()).unwrap();
        let buffer = image.to_rgba8();
        let changed: Vec<_> = buffer
            .enumerate_pixels()
            .filter(|(_, _, pixel)| **pixel != original)
            .collect();
        let min_x = changed.iter().map(|(x, _, _)| *x).min().unwrap();
        let max_x = changed.iter().map(|(x, _, _)| *x).max().unwrap();
        let min_y = changed.iter().map(|(_, y, _)| *y).min().unwrap();
        let max_y = changed.iter().map(|(_, y, _)| *y).max().unwrap();
        match corner {
            StampCorner::TopLeft | StampCorner::BottomLeft => assert_eq!(min_x, 40),
            StampCorner::TopRight | StampCorner::BottomRight => assert_eq!(max_x, 959),
        }
        match corner {
            StampCorner::TopLeft | StampCorner::TopRight => assert_eq!(min_y, 40),
            StampCorner::BottomLeft | StampCorner::BottomRight => assert_eq!(max_y, 959),
        }
        assert_eq!(buffer.get_pixel(500, 500), &original);
    }
}
