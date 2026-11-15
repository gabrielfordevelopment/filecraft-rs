//! Headless checks do not open a window or interact with the desktop.

use super::*;

fn texts(shape: &egui::Shape, output: &mut Vec<(String, Rect)>) {
    match shape {
        egui::Shape::Text(text) => output.push((
            text.galley.text().to_owned(),
            Rect::from_min_size(text.pos, text.galley.size()),
        )),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                texts(shape, output);
            }
        }
        _ => {}
    }
}

fn render(tab: &mut ImageToolsTab, size: egui::Vec2, theme: egui::Theme) -> Vec<(String, Rect)> {
    let ctx = egui::Context::default();
    theme::install(&ctx, Some(theme));
    let mut labels = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |root| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(root, |ui| {
                        tab.ui(ui);
                    });
            },
        );
        labels.clear();
        for shape in &output.shapes {
            texts(&shape.shape, &mut labels);
        }
        output.textures_delta.clear();
    }
    labels
}

fn rect(labels: &[(String, Rect)], text: &str) -> Rect {
    labels
        .iter()
        .find(|(label, _)| label == text)
        .unwrap_or_else(|| panic!("Missing UI label: {text}"))
        .1
}

fn input_frame(
    ctx: &egui::Context,
    tab: &mut ImageToolsTab,
    events: Vec<egui::Event>,
) -> Vec<(String, Rect)> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(680.0, 500.0))),
            events,
            ..Default::default()
        },
        |root| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(root, |ui| {
                    tab.ui(ui);
                });
        },
    );
    let mut labels = Vec::new();
    for shape in &output.shapes {
        texts(&shape.shape, &mut labels);
    }
    output.textures_delta.clear();
    labels
}

fn click(ctx: &egui::Context, tab: &mut ImageToolsTab, pos: egui::Pos2) -> Vec<(String, Rect)> {
    for pressed in [true, false] {
        input_frame(
            ctx,
            tab,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    input_frame(ctx, tab, Vec::new())
}

#[test]
fn shared_controls_toggle_options_select_corners_and_disable_processing_without_a_source() {
    let ctx = egui::Context::default();
    theme::install(&ctx, Some(egui::Theme::Light));
    let mut tab = ImageToolsTab::default();
    assert!(tab.width_enabled && tab.height_enabled);
    assert_eq!((tab.width, tab.height), (2100, 2100));
    assert_eq!(
        (tab.options.text_height_percent, tab.options.margin_percent),
        (3.0, 5.0)
    );
    assert!(tab.options.match_orientation);
    input_frame(&ctx, &mut tab, Vec::new());
    let labels = input_frame(&ctx, &mut tab, Vec::new());
    let labels = click(
        &ctx,
        &mut tab,
        rect(&labels, strings::IMAGE_CORNERS[2]).center(),
    );
    let labels = click(
        &ctx,
        &mut tab,
        rect(&labels, strings::IMAGE_CORNERS[1]).center(),
    );
    assert_eq!(tab.options.corner, StampCorner::TopRight);
    let labels = click(
        &ctx,
        &mut tab,
        rect(&labels, strings::IMAGE_MATCH_ORIENTATION).center(),
    );
    assert!(!tab.options.match_orientation);
    let labels = click(
        &ctx,
        &mut tab,
        rect(&labels, strings::IMAGE_RESIZE).center(),
    );
    assert!(tab.options.resize);
    let labels = click(
        &ctx,
        &mut tab,
        rect(&labels, strings::IMAGE_MAX_WIDTH).center(),
    );
    assert_eq!(tab.options.max_width, None);
    assert_eq!(tab.options.max_height, Some(2100));
    let labels = click(&ctx, &mut tab, rect(&labels, strings::IMAGE_STAMP).center());
    assert!(!tab.options.add_date);
    click(
        &ctx,
        &mut tab,
        rect(&labels, strings::IMAGE_PROCESS).center(),
    );
    assert!(tab.running.is_none());
    assert!(tab.preview.is_none());
}

#[test]
fn compact_layout_aligns_source_controls_and_keeps_action_in_footer() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        for size in [vec2(680.0, 500.0), vec2(1000.0, 700.0)] {
            let mut tab = ImageToolsTab::default();
            let labels = render(&mut tab, size, theme);
            let browse = rect(&labels, strings::BROWSE_BUTTON);
            let source = rect(&labels, strings::IMAGE_SOURCE);
            assert!(
                (browse.center().y - source.center().y).abs() < 2.0,
                "Source controls should share a baseline: {browse:?} {source:?}"
            );
            let process = rect(&labels, strings::IMAGE_PROCESS);
            assert!(
                process.center().y > size.y - 60.0,
                "Primary action should be in the footer"
            );
            let results = rect(&labels, strings::IMAGE_PREVIEW_TITLE);
            assert!(
                results.top() < 300.0,
                "Settings should leave room for results: {results:?}"
            );
            assert!(rect(&labels, strings::IMAGE_PREVIEW).bottom() < process.top());
            assert!(rect(&labels, strings::IMAGE_POSITION).bottom() < results.top());
            assert!(rect(&labels, strings::IMAGE_MARGIN).bottom() < results.top());
            assert!(rect(&labels, strings::IMAGE_MATCH_ORIENTATION).bottom() < results.top());
            assert!(rect(&labels, strings::IMAGE_RESIZE_HINT).bottom() < results.top());
            for results in [false, true] {
                tab.rows = vec![FileResult {
                    name: "2026_04_05_001.jpg".to_owned(),
                    outcome: Ok(image_tools::Processed {
                        original_size: (4000, 3000),
                        output_size: (2000, 1500),
                        date: None,
                        copied: false,
                        notes: Vec::new(),
                    }),
                }];
                tab.results = results;
                let labels = render(&mut tab, size, theme);
                assert!(
                    rect(&labels, strings::IMAGE_STATUS_COLUMN).left() > size.x * 0.65,
                    "Both preview and results columns should span the panel width"
                );
            }
        }
    }
}

#[test]
fn progress_keeps_stopped_counts_and_shows_completion_in_the_footer() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let row = FileResult {
            name: "photo.png".to_owned(),
            outcome: Err("Test failure".to_owned()),
        };
        let mut tab = ImageToolsTab {
            results: true,
            progress_total: Some(10),
            stopped: true,
            rows: vec![row.clone(); 2],
            ..ImageToolsTab::default()
        };
        let size = vec2(680.0, 500.0);
        let labels = render(&mut tab, size, theme);
        let stopped = strings::image_progress(strings::IMAGE_PROGRESS_STOPPED, 2, 10);
        assert!(stopped.ends_with("2 / 10 (20%)"));
        let progress = rect(&labels, &stopped);
        assert!(progress.bottom() < rect(&labels, strings::IMAGE_PROCESS).top());
        assert!(progress.top() > rect(&labels, strings::IMAGE_RESULTS).bottom());
        tab.stopped = false;
        tab.rows = vec![row; 10];
        let labels = render(&mut tab, size, theme);
        let completed = strings::image_progress(strings::IMAGE_PROGRESS_COMPLETE, 10, 10);
        assert!(completed.ends_with("10 / 10 (100%)"));
        rect(&labels, &completed);
    }
}

#[test]
fn shared_busy_buttons_keep_padding_around_spinner_and_text() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let ctx = egui::Context::default();
        theme::install(&ctx, Some(theme));
        for text in [strings::IMAGE_PROCESSING, strings::FILE_RENAME_RUNNING] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(680.0, 100.0))),
                    time: Some(std::f64::consts::FRAC_PI_2),
                    ..Default::default()
                },
                |ui| {
                    let button = FooterButton {
                        text,
                        disabled_reason: None,
                        busy_text: Some(text),
                    };
                    let width = widgets::button_width(ui, text) + 20.0;
                    assert!(!export_footer::export_button(
                        ui,
                        &button,
                        width,
                        widgets::control_height(ui)
                    ));
                },
            );
            let button = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) => Some(rect.rect),
                    _ => None,
                })
                .unwrap();
            let spinner = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(_) => Some(shape.shape.visual_bounding_rect()),
                    _ => None,
                })
                .unwrap();
            let mut labels = Vec::new();
            for shape in &output.shapes {
                texts(&shape.shape, &mut labels);
            }
            let caption = rect(&labels, text);
            assert!(spinner.left() - button.left() >= 10.0);
            assert!(button.right() - caption.right() >= 10.0);
            assert!(spinner.right() < caption.left());
            output.textures_delta.clear();
        }
    }
}
