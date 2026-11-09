//! The footer of the export tabs: statistics as pills on the left, then the file name, the
//! timestamp check box, an optional check box and the export button on the right. It is one row
//! and wraps when the tab is too narrow.

use std::path::{Path, PathBuf};

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Layout, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
use filecraft_core::file_name::{final_file_name, output_path};

use crate::numbers::format_grouped;
use crate::state::SharedState;
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind};

/// File name and timestamp option shared by all exports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub output_file_name: String,
    pub append_timestamp: bool,
}

impl ExportOptions {
    pub fn new(output_file_name: &str) -> Self {
        Self {
            output_file_name: output_file_name.to_owned(),
            append_timestamp: false,
        }
    }
}

impl ExportOptions {
    /// The file name an export started now writes, without extension, and its full path.
    pub fn target(&self, destination: &str) -> (String, PathBuf) {
        let name = final_file_name(
            &self.output_file_name,
            self.append_timestamp,
            chrono::Local::now().naive_local(),
        );
        let path = output_path(Path::new(destination), &name);
        (name, path)
    }

    /// Why an export cannot start because of the folders or the file name.
    pub fn blocker(&self, shared: &SharedState) -> Option<&'static str> {
        if shared.source_path.trim().is_empty() {
            Some(strings::NEEDS_SOURCE_FOLDER)
        } else if shared.destination_path.trim().is_empty() {
            Some(strings::NEEDS_DESTINATION_FOLDER)
        } else if self.output_file_name.trim().is_empty() {
            Some(strings::NEEDS_FILE_NAME)
        } else {
            None
        }
    }
}

/// How a statistic pill is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatTone {
    Neutral,
    /// The number the action works on, such as the selected files.
    Highlight,
    /// A number that needs attention, such as errors.
    Warning,
}

impl StatTone {
    /// Highlights a number the action needs, or warns when it is zero.
    pub fn needed(count: usize) -> Self {
        if count == 0 {
            Self::Warning
        } else {
            Self::Highlight
        }
    }
}

/// A number with a short label, shown as a pill such as "24 selected".
pub struct Stat<'a> {
    pub value: String,
    pub label: &'a str,
    pub tone: StatTone,
}

impl<'a> Stat<'a> {
    pub fn count(count: usize, label: &'a str, tone: StatTone) -> Self {
        Self {
            value: format_grouped(count as u64),
            label,
            tone,
        }
    }
}

/// A check box shown between the timestamp check box and the button, with its tooltip.
pub struct FooterCheck<'a> {
    pub value: &'a mut bool,
    pub text: &'a str,
    pub tooltip: &'a str,
}

/// The export button and its state.
pub struct FooterButton<'a> {
    pub text: &'a str,
    /// Why the button is disabled, shown as its tooltip. `None` enables it.
    pub disabled_reason: Option<&'a str>,
    /// Replaces the text and shows a spinner while the export runs.
    pub busy_text: Option<&'a str>,
}

/// Space between the footer and the tab border.
const MARGIN: f32 = 10.0;
const GAP: f32 = 12.0;
const STATS_GAP: f32 = 20.0;
const ROW_GAP: f32 = 8.0;
const FILE_NAME_WIDTH: f32 = 190.0;
/// Room for the spinner of a busy button.
const SPINNER_SPACE: f32 = 20.0;
const PILL_HEIGHT: f32 = 22.0;
const PILL_PADDING: f32 = 9.0;
const PILL_GAP: f32 = 6.0;

/// Height of the export button, as high as the file name box with its floating label.
pub fn button_height(ui: &Ui) -> f32 {
    widgets::floating_box_height(ui)
}

/// How the footer is arranged in a given width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrangement {
    /// Everything on one row.
    Row,
    /// Statistics on one line above the other controls.
    StatsAbove,
    /// Statistics, then the check box, then the other controls.
    Stacked,
}

struct Measure {
    stats: f32,
    options: f32,
    check: f32,
    button: f32,
}

fn measure(ui: &Ui, stats: &[Stat], check_text: Option<&str>, button: &FooterButton) -> Measure {
    let body = theme::body(theme::BODY_SIZE);
    // The button keeps the width of its longer text, so it does not jump while busy.
    let button_width = [Some(button.text), button.busy_text]
        .into_iter()
        .flatten()
        .map(|text| widgets::button_width(ui, text))
        .fold(0.0, f32::max)
        + SPINNER_SPACE;
    let check_width = |text: &str| 15.0 + 5.0 + widgets::text_width(ui, text, body.clone());
    Measure {
        stats: stats_width(ui, stats),
        options: FILE_NAME_WIDTH
            + GAP
            + check_width(strings::EXPORT_OPTIONS_APPEND_TIMESTAMP_LABEL),
        check: check_text.map_or(0.0, |text| check_width(text) + GAP),
        button: button_width,
    }
}

fn arrangement(measure: &Measure, width: f32) -> Arrangement {
    let controls = measure.options + GAP + measure.check + measure.button;
    if measure.stats + STATS_GAP + controls <= width {
        Arrangement::Row
    } else if controls <= width {
        Arrangement::StatsAbove
    } else {
        Arrangement::Stacked
    }
}

/// Height of the footer in a tab of `width`, including its margin.
pub fn footer_height(
    ui: &Ui,
    width: f32,
    stats: &[Stat],
    check_text: Option<&str>,
    button: &FooterButton,
) -> f32 {
    let measure = measure(ui, stats, check_text, button);
    let row = button_height(ui);
    let line = ui.text_style_height(&egui::TextStyle::Body);
    let content = match arrangement(&measure, width - MARGIN * 2.0) {
        Arrangement::Row => row,
        Arrangement::StatsAbove => PILL_HEIGHT + ROW_GAP + row,
        Arrangement::Stacked => PILL_HEIGHT + ROW_GAP + line + ROW_GAP + row,
    };
    content + MARGIN * 2.0
}

/// Draws the footer inside `rect`, the area measured by [`footer_height`] including its margin.
/// Returns true when the enabled button was clicked.
pub fn footer_ui(
    ui: &mut Ui,
    rect: Rect,
    id_salt: &str,
    options: &mut ExportOptions,
    stats: &[Stat],
    check: Option<FooterCheck<'_>>,
    button: FooterButton<'_>,
) -> bool {
    separator(ui, rect);
    let rect = rect.shrink(MARGIN);
    let measure = measure(ui, stats, check.as_ref().map(|check| check.text), &button);
    let arrangement = arrangement(&measure, rect.width());
    let row_height = button_height(ui);
    let line = ui.text_style_height(&egui::TextStyle::Body);

    let controls_rect = Rect::from_min_max(pos2(rect.left(), rect.bottom() - row_height), rect.max);
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(controls_rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    let clicked = export_button(&mut row, &button, measure.button, row_height);
    row.add_space(GAP);
    let check = match (arrangement, check) {
        (Arrangement::Stacked, Some(check)) => {
            let check_rect = Rect::from_min_size(
                pos2(rect.left(), rect.top() + PILL_HEIGHT + ROW_GAP),
                vec2(rect.width(), line),
            );
            check_ui(ui, check_rect, check);
            None
        }
        (_, check) => check,
    };
    if let Some(check) = check {
        let (check_rect, _) =
            row.allocate_exact_size(vec2(measure.check - GAP, row_height), Sense::hover());
        check_ui(&mut row, check_rect, check);
        row.add_space(GAP);
    }
    options_ui(&mut row, id_salt, options);

    let stats_center = match arrangement {
        Arrangement::Row => controls_rect.center().y,
        Arrangement::StatsAbove | Arrangement::Stacked => rect.top() + PILL_HEIGHT / 2.0,
    };
    paint_stats(ui, rect.left(), stats_center, stats);
    clicked
}

/// The thin line between the footer and the content above it.
pub fn separator(ui: &Ui, rect: Rect) {
    let y = rect.top().round() + 0.5;
    ui.painter().line_segment(
        [pos2(rect.left() + 5.0, y), pos2(rect.right() - 5.0, y)],
        Stroke::new(1.0, theme::palette().border_subtle),
    );
}

/// The colors of a pill: fill, border, value and label.
fn pill_colors(tone: StatTone) -> [egui::Color32; 4] {
    let colors = theme::palette();
    match tone {
        StatTone::Neutral => [
            colors.badge_background,
            colors.badge_pressed,
            colors.text,
            colors.text_muted,
        ],
        StatTone::Highlight => [
            colors.accent_soft,
            widgets::lerp_color(colors.accent_soft, colors.accent, 0.35),
            colors.accent,
            colors.accent,
        ],
        StatTone::Warning => [
            colors.warning_soft,
            colors.warning_border,
            colors.warning,
            colors.warning,
        ],
    }
}

fn pill_galley(ui: &Ui, stat: &Stat) -> std::sync::Arc<egui::Galley> {
    let [_, _, value, label] = pill_colors(stat.tone);
    let mut job = LayoutJob::default();
    job.append(
        &stat.value,
        0.0,
        TextFormat::simple(theme::semibold(theme::BODY_SIZE), value),
    );
    if !stat.label.is_empty() {
        job.append(
            stat.label,
            4.0,
            TextFormat::simple(theme::body(theme::BODY_SIZE), label),
        );
    }
    ui.painter().layout_job(job)
}

/// Width of the pills of `stats` side by side.
pub fn stats_width(ui: &Ui, stats: &[Stat]) -> f32 {
    let pills: f32 = stats
        .iter()
        .map(|stat| pill_galley(ui, stat).size().x + PILL_PADDING * 2.0)
        .sum();
    pills + PILL_GAP * stats.len().saturating_sub(1) as f32
}

/// Lays out `stats` as pills, for example in a card header laid out from the right.
pub fn stats_ui(ui: &mut Ui, stats: &[Stat]) {
    let width = stats_width(ui, stats);
    let (rect, _) = ui.allocate_exact_size(vec2(width, PILL_HEIGHT), Sense::hover());
    paint_stats(ui, rect.left(), rect.center().y, stats);
}

/// Paints `stats` as pills from `left`, vertically centered on `center_y`.
pub fn paint_stats(ui: &Ui, left: f32, center_y: f32, stats: &[Stat]) {
    let mut x = left;
    for stat in stats {
        let [fill, border, _, _] = pill_colors(stat.tone);
        let galley = pill_galley(ui, stat);
        let pill = Rect::from_min_size(
            pos2(x, center_y - PILL_HEIGHT / 2.0),
            vec2(galley.size().x + PILL_PADDING * 2.0, PILL_HEIGHT),
        );
        ui.painter().rect(
            pill,
            PILL_HEIGHT / 2.0,
            fill,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        ui.painter().galley(
            pos2(
                pill.left() + PILL_PADDING,
                pill.center().y - galley.size().y / 2.0,
            ),
            galley,
            fill,
        );
        x = pill.right() + PILL_GAP;
    }
}

pub fn export_button(ui: &mut Ui, button: &FooterButton, width: f32, height: f32) -> bool {
    let enabled = button.disabled_reason.is_none() && button.busy_text.is_none();
    let text = button.busy_text.unwrap_or(button.text);
    let response = widgets::button(ui, text, ButtonKind::Primary, enabled, vec2(width, height));
    if button.busy_text.is_some() {
        let text_width = widgets::text_width(ui, text, theme::semibold(theme::BODY_SIZE));
        let spinner = Rect::from_center_size(
            pos2(
                response.rect.center().x - text_width / 2.0 - 12.0,
                response.rect.center().y,
            ),
            egui::Vec2::splat(12.0),
        );
        egui::Spinner::new()
            .size(12.0)
            .color(theme::palette().text_hint)
            .paint_at(ui, spinner);
    }
    let clicked = response.clicked() && enabled;
    if let Some(reason) = button.disabled_reason
        && button.busy_text.is_none()
    {
        response.on_hover_text(reason);
    }
    clicked
}

fn check_ui(ui: &mut Ui, rect: Rect, check: FooterCheck<'_>) {
    let mut check_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    widgets::labeled_check_box(&mut check_ui, check.value, check.text).on_hover_text(check.tooltip);
}

/// The file name box and the timestamp check box, laid out from the right.
fn options_ui(row: &mut Ui, id_salt: &str, options: &mut ExportOptions) {
    let body = theme::body(theme::BODY_SIZE);
    let timestamp_width = 15.0
        + 5.0
        + widgets::text_width(
            row,
            strings::EXPORT_OPTIONS_APPEND_TIMESTAMP_LABEL,
            body.clone(),
        );
    let (timestamp_rect, _) = row.allocate_exact_size(
        vec2(timestamp_width, row.available_height()),
        Sense::hover(),
    );
    check_ui(
        row,
        timestamp_rect,
        FooterCheck {
            value: &mut options.append_timestamp,
            text: strings::EXPORT_OPTIONS_APPEND_TIMESTAMP_LABEL,
            tooltip: strings::EXPORT_OPTIONS_APPEND_TIMESTAMP_TOOLTIP,
        },
    );
    row.add_space(GAP);
    widgets::floating_text_box(
        row,
        id_salt,
        &mut options.output_file_name,
        FILE_NAME_WIDTH,
        strings::EXPORT_OPTIONS_FILE_NAME_LABEL,
        ".txt",
    );
}
