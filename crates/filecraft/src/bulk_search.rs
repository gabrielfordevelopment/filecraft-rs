//! The bulk search dialog. The input lines
//! and the found paths have the same row height and start at the same height, so they line up.

use egui::{Align, Align2, Layout, Rect, Ui, UiBuilder, pos2, vec2};
use filecraft_core::bulk_search::{BulkMatch, break_into_lines, match_lines, remove_whitespaces};
use filecraft_core::folder_tree::CheckState;

use crate::dialog::{Dialog, DialogResult, ModalLayer};
use crate::export_footer::{self, Stat, StatTone};
use crate::icons;
use crate::ignored_comments::{dialog_frame, master_state};
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind};

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 600.0;
const BUTTON_HEIGHT: f32 = 32.0;
const CARD_GAP: f32 = 12.0;
/// Space between the list and the row of controls below it in each card.
const LIST_GAP: f32 = 8.0;
/// Space between the input box border and its text.
const INPUT_PADDING: f32 = 8.0;
/// Space above the first line of the input box and of the found paths.
const LIST_TOP: f32 = 5.0;
/// Width of the column with the match count of each input line.
const COUNTS_WIDTH: f32 = 40.0;
/// Space between the match counts and the input box.
const COUNTS_GAP: f32 = 6.0;

pub enum BulkSearchOutcome {
    /// File indices with their new selection.
    Apply(Vec<(usize, bool)>),
    Cancel,
}

pub struct BulkSearchDialog {
    relative_paths: Vec<String>,
    original: Vec<bool>,
    selected: Vec<bool>,
    input: String,
    matched: BulkMatch,
    break_by_space: bool,
    confirm: Option<Dialog>,
    input_focused: bool,
    layer: ModalLayer,
}

impl BulkSearchDialog {
    /// `files` are the relative paths of the export files with their current selection.
    pub fn new(files: Vec<(String, bool)>) -> Self {
        let (relative_paths, original): (Vec<String>, Vec<bool>) = files.into_iter().unzip();
        let matched = match_lines("", &relative_paths);
        Self {
            selected: original.clone(),
            relative_paths,
            original,
            input: String::new(),
            matched,
            break_by_space: true,
            confirm: None,
            input_focused: false,
            layer: ModalLayer::new(),
        }
    }

    fn set_input(&mut self, input: String) {
        self.input = input;
        self.matched = match_lines(&self.input, &self.relative_paths);
    }

    fn changes(&self) -> Vec<(usize, bool)> {
        (0..self.selected.len())
            .filter(|index| self.selected[*index] != self.original[*index])
            .map(|index| (index, self.selected[index]))
            .collect()
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<BulkSearchOutcome> {
        let screen = ctx.content_rect();
        let size = vec2(
            WIDTH.min(screen.width() - 40.0),
            HEIGHT.min(screen.height() - 40.0),
        );
        let modal = crate::dialog::stacked_modal(ctx, self.layer)
            .frame(dialog_frame().fill(theme::palette().section_background))
            .show(ctx, |ui| {
                ui.set_width(size.x - 30.0);
                ui.set_height(size.y - 30.0);
                self.contents(ui)
            });

        if let Some(confirm) = &mut self.confirm {
            if let Some(result) = confirm.show(ctx) {
                self.confirm = None;
                if result == DialogResult::Primary {
                    return Some(BulkSearchOutcome::Apply(self.changes()));
                }
            }
            return None;
        }
        if let Some(outcome) = modal.inner {
            return Some(outcome);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            return Some(BulkSearchOutcome::Cancel);
        }
        None
    }

    /// The title, the input card and the found paths card side by side, and the buttons.
    fn contents(&mut self, ui: &mut Ui) -> Option<BulkSearchOutcome> {
        let area = ui.max_rect();
        widgets::label(
            ui,
            strings::BULK_SEARCH_TITLE,
            theme::semibold(14.0),
            theme::palette().text,
        );
        ui.add_space(12.0);
        let top = ui.cursor().top();
        let cards_bottom = area.bottom() - BUTTON_HEIGHT - CARD_GAP;
        let middle = area.center().x;
        let input_card = Rect::from_min_max(
            pos2(area.left(), top),
            pos2(middle - CARD_GAP / 2.0, cards_bottom),
        );
        let found_card = Rect::from_min_max(
            pos2(middle + CARD_GAP / 2.0, top),
            pos2(area.right(), cards_bottom),
        );
        self.input_card_ui(ui, input_card);
        self.found_card_ui(ui, found_card);

        let footer_rect =
            Rect::from_min_max(pos2(area.left(), area.bottom() - BUTTON_HEIGHT), area.max);
        self.footer_ui(ui, footer_rect)
    }

    /// The match counts and the input box, with the text tools below them.
    fn input_card_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let stats = [Stat::count(
            self.matched.lines_with_matches,
            strings::BULK_SEARCH_LINES_MATCHED_LABEL,
            StatTone::Neutral,
        )];
        widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                widgets::card_title(ui, strings::BULK_SEARCH_INPUT_PATHS_LABEL);
                ui.add_space(6.0);
                widgets::label(
                    ui,
                    strings::BULK_SEARCH_ONE_PER_LINE,
                    theme::body(theme::BODY_SIZE),
                    theme::palette().text_hint,
                );
            },
            |ui| export_footer::stats_ui(ui, &stats),
            |ui| {
                let (list, controls) = split_card_body(ui);
                self.input_ui(ui, list);
                self.tools_ui(ui, controls);
            },
        );
    }

    /// The match count of every line next to the input box. The lines do not wrap, so every line
    /// keeps the height of a found path row.
    fn input_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let colors = theme::palette();
        let counts_rect =
            Rect::from_min_max(rect.min, pos2(rect.left() + COUNTS_WIDTH, rect.bottom()));
        ui.painter().rect(
            counts_rect,
            4,
            colors.surface_alt,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        let input_rect =
            Rect::from_min_max(pos2(counts_rect.right() + COUNTS_GAP, rect.top()), rect.max);
        ui.painter().rect(
            input_rect,
            4,
            colors.surface,
            egui::Stroke::new(1.0, colors.accent),
            egui::StrokeKind::Inside,
        );
        let inner = input_rect.shrink(1.0);
        let row_height = widgets::check_row_height(ui);
        let font = egui::FontId::monospace(theme::BODY_SIZE);
        let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, _wrap_width: f32| {
            let mut job = egui::text::LayoutJob::simple(
                buffer.as_str().to_owned(),
                font.clone(),
                colors.text,
                f32::INFINITY,
            );
            for section in &mut job.sections {
                section.format.line_height = Some(row_height);
            }
            ui.painter().layout_job(job)
        };
        let area = egui::ScrollArea::both()
            .id_salt("bulk_search_input")
            .auto_shrink([false, false]);
        let mut input = self.input.clone();
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        let output = area.show(&mut child, |ui| {
            let margin = egui::Margin {
                left: INPUT_PADDING as i8,
                right: INPUT_PADDING as i8,
                top: LIST_TOP as i8,
                bottom: LIST_TOP as i8,
            };
            let edit = egui::TextEdit::multiline(&mut input)
                .id_salt("bulk_search_text")
                .font(font.clone())
                .frame(egui::Frame::new().inner_margin(margin))
                .min_size(inner.size())
                .desired_width(inner.width() - INPUT_PADDING * 2.0)
                .text_color(colors.text)
                .layouter(&mut layouter)
                .show(ui);
            if !self.input_focused {
                edit.response.request_focus();
                self.input_focused = true;
            }
        });
        if input != self.input {
            self.set_input(input);
        }
        self.paint_counts(ui, counts_rect, output.state.clamped_offset().y, row_height);
    }

    /// Paints the match count of every visible input line, level with the line at the scroll
    /// position `offset` of the input box.
    fn paint_counts(&self, ui: &Ui, rect: Rect, offset: f32, row_height: f32) {
        let clip = rect.shrink(1.0);
        let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
        let font = egui::FontId::monospace(theme::BODY_SIZE);
        let first_top = clip.top() + LIST_TOP - offset;
        for (line, count) in self.matched.counts.iter().enumerate() {
            let top = first_top + row_height * line as f32;
            if top > clip.bottom() {
                break;
            }
            if let Some(count) = count
                && top + row_height >= clip.top()
            {
                painter.text(
                    pos2(clip.right() - 6.0, top + row_height / 2.0),
                    Align2::RIGHT_CENTER,
                    count.to_string(),
                    font.clone(),
                    theme::palette().text_muted,
                );
            }
        }
    }

    /// Remove whitespaces, break into lines and the delimiter to break at.
    fn tools_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        if widgets::button(
            &mut row,
            strings::BULK_SEARCH_REMOVE_WHITESPACES_BUTTON,
            ButtonKind::Secondary,
            true,
            egui::Vec2::ZERO,
        )
        .clicked()
            && !self.input.is_empty()
        {
            self.set_input(remove_whitespaces(&self.input));
        }
        row.add_space(8.0);
        if widgets::button(
            &mut row,
            strings::BULK_SEARCH_BREAK_INTO_LINES_BUTTON,
            ButtonKind::Secondary,
            true,
            egui::Vec2::ZERO,
        )
        .clicked()
            && !self.input.is_empty()
        {
            let delimiter = if self.break_by_space { ' ' } else { ';' };
            self.set_input(break_into_lines(&self.input, delimiter));
        }
        row.add_space(8.0);
        widgets::label(
            &mut row,
            strings::BULK_SEARCH_BREAK_BY_LABEL,
            theme::body(theme::BODY_SIZE),
            theme::palette().text_muted,
        );
        row.add_space(8.0);
        let options = [
            strings::BULK_SEARCH_SPACE_OPTION,
            strings::BULK_SEARCH_SEMICOLON_OPTION,
        ];
        let current = usize::from(!self.break_by_space);
        if let Some(index) = widgets::segmented(&mut row, &options, current) {
            self.break_by_space = index == 0;
        }
    }

    /// The found paths, starting level with the first input line, and "Select all" below them.
    fn found_card_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let changed = self
            .matched
            .matched
            .iter()
            .filter(|index| self.selected[**index] != self.original[**index])
            .count();
        let stats = [
            Stat::count(
                self.matched.matched.len(),
                strings::BULK_SEARCH_FOUND_LABEL,
                StatTone::Neutral,
            ),
            Stat::count(
                changed,
                strings::BULK_SEARCH_CHANGED_LABEL,
                if changed == 0 {
                    StatTone::Neutral
                } else {
                    StatTone::Highlight
                },
            ),
        ];
        widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                widgets::card_title(ui, strings::BULK_SEARCH_FOUND_PATHS_LABEL);
            },
            |ui| export_footer::stats_ui(ui, &stats),
            |ui| {
                let (list, controls) = split_card_body(ui);
                self.found_ui(ui, list);
                self.select_all_ui(ui, controls);
            },
        );
    }

    /// Every found path with its check box, ordered by path. The rows have the height of the
    /// input lines and start at the same height.
    fn found_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let colors = theme::palette();
        ui.painter().rect(
            rect,
            4,
            colors.surface,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        let inner = Rect::from_min_max(
            pos2(rect.left() + 1.0, rect.top() + 1.0 + LIST_TOP),
            pos2(rect.right() - 1.0, rect.bottom() - 1.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        let row_height = widgets::check_row_height(ui);
        let matched = self.matched.matched.clone();
        egui::ScrollArea::vertical()
            .id_salt("bulk_search_found")
            .auto_shrink([false, false])
            .show_rows(&mut child, row_height, matched.len(), |ui, range| {
                for position in range {
                    let index = matched[position];
                    let state = if self.selected[index] {
                        CheckState::Checked
                    } else {
                        CheckState::Unchecked
                    };
                    if widgets::path_check_row(ui, state, &self.relative_paths[index]).clicked() {
                        self.selected[index] = !self.selected[index];
                    }
                }
            });
    }

    fn select_all_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        row.add_space((rect.height() - widgets::check_row_height(ui)) / 2.0);
        let matched = self.matched.matched.clone();
        let master = master_state(matched.iter().map(|index| self.selected[*index]));
        if widgets::check_row(&mut row, master, strings::COMMON_SELECT_ALL, true).clicked() {
            let select = master != CheckState::Checked;
            for index in matched {
                self.selected[index] = select;
            }
        }
    }

    fn footer_ui(&mut self, ui: &mut Ui, rect: Rect) -> Option<BulkSearchOutcome> {
        let min = vec2(80.0, BUTTON_HEIGHT);
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        if widgets::button(
            &mut right,
            strings::COMMON_CANCEL_BUTTON,
            ButtonKind::Secondary,
            true,
            min,
        )
        .clicked()
        {
            return Some(BulkSearchOutcome::Cancel);
        }
        right.add_space(10.0);
        if widgets::button(
            &mut right,
            strings::BULK_SEARCH_APPLY_BUTTON,
            ButtonKind::Primary,
            true,
            min,
        )
        .clicked()
        {
            self.confirm = Some(Dialog::confirmation(
                strings::BULK_SEARCH_APPLY_TITLE,
                strings::BULK_SEARCH_APPLY_MESSAGE.to_owned(),
                icons::INFO,
            ));
        }
        None
    }
}

/// Splits the body of a card into the list and the row of controls below it. Both cards split
/// the same way, so their lists start and end at the same height.
fn split_card_body(ui: &Ui) -> (Rect, Rect) {
    let body = ui.max_rect();
    let controls_height = widgets::control_height(ui);
    let list = Rect::from_min_max(
        body.min,
        pos2(body.right(), body.bottom() - controls_height - LIST_GAP),
    );
    let controls = Rect::from_min_max(pos2(body.left(), body.bottom() - controls_height), body.max);
    (list, controls)
}
