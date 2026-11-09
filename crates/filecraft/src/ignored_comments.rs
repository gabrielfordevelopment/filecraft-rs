//! The dialog that selects files whose `///` comments are removed.

use std::path::Path;

use egui::{Align, Layout, Rect, Ui, UiBuilder, pos2, vec2};
use filecraft_core::file_content::{
    FileEntry, compare_paths, count_xml_comment_chars, ignore_case_key,
};
use filecraft_core::folder_tree::CheckState;

use crate::background::Task;
use crate::dialog::ModalLayer;
use crate::numbers::format_grouped;
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind};

const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 600.0;
const COUNT_COLUMN: f32 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentCount {
    Counting,
    Chars(u64),
    NotFound,
    Failed,
}

impl CommentCount {
    fn value(self) -> u64 {
        match self {
            Self::Chars(count) => count,
            _ => 0,
        }
    }

    fn display(self) -> String {
        match self {
            Self::Counting => "...".to_owned(),
            Self::Chars(count) => count.to_string(),
            Self::NotFound => strings::IGNORED_COMMENTS_STATUS_NA.to_owned(),
            Self::Failed => strings::IGNORED_COMMENTS_STATUS_ERR.to_owned(),
        }
    }
}

struct IgnoredFile {
    relative_path: String,
    selected: bool,
    count: CommentCount,
}

pub struct IgnoredCommentsDialog {
    files: Vec<IgnoredFile>,
    search: String,
    counting: Option<Task<Vec<CommentCount>>>,
    layer: ModalLayer,
}

impl IgnoredCommentsDialog {
    /// Lists the selected export files and starts counting their `///` comment characters.
    pub fn new(ctx: &egui::Context, selected_files: &[FileEntry], ignored: &[String]) -> Self {
        let ignored: std::collections::HashSet<String> =
            ignored.iter().map(|path| ignore_case_key(path)).collect();
        let mut files: Vec<&FileEntry> = selected_files.iter().collect();
        files.sort_by(|a, b| compare_paths(&a.relative_path, &b.relative_path));
        let full_paths: Vec<String> = files.iter().map(|file| file.full_path.clone()).collect();
        let files = files
            .iter()
            .map(|file| IgnoredFile {
                selected: ignored.contains(&ignore_case_key(&file.relative_path)),
                relative_path: file.relative_path.clone(),
                count: CommentCount::Counting,
            })
            .collect();
        let counting = Task::spawn(ctx, move || {
            full_paths
                .iter()
                .map(|path| {
                    let path = Path::new(path);
                    if !path.is_file() {
                        return CommentCount::NotFound;
                    }
                    match count_xml_comment_chars(path) {
                        Ok(count) => CommentCount::Chars(count),
                        Err(_) => CommentCount::Failed,
                    }
                })
                .collect()
        });
        Self {
            files,
            search: String::new(),
            counting: Some(counting),
            layer: ModalLayer::new(),
        }
    }

    /// Applies finished counts and orders the files by comment size, largest first.
    fn poll_counts(&mut self) {
        let Some(result) = self.counting.as_ref().and_then(Task::poll) else {
            return;
        };
        self.counting = None;
        let counts = result.unwrap_or_else(|_| vec![CommentCount::Failed; self.files.len()]);
        for (file, count) in self.files.iter_mut().zip(counts) {
            file.count = count;
        }
        self.files
            .sort_by_key(|file| std::cmp::Reverse(file.count.value()));
    }

    fn visible(&self) -> Vec<usize> {
        let search = ignore_case_key(&self.search);
        (0..self.files.len())
            .filter(|index| {
                search.trim().is_empty()
                    || ignore_case_key(&self.files[*index].relative_path).contains(&search)
            })
            .collect()
    }

    fn total(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| file.selected)
            .map(|file| file.count.value())
            .sum()
    }

    /// Shows the dialog. Returns `Some(Some(paths))` on save, `Some(None)` on cancel.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Option<Vec<String>>> {
        self.poll_counts();
        let screen = ctx.content_rect();
        let size = vec2(
            WIDTH.min(screen.width() - 40.0),
            HEIGHT.min(screen.height() - 40.0),
        );
        let modal = crate::dialog::stacked_modal(ctx, self.layer)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(size.x - 30.0);
                ui.set_height(size.y - 30.0);
                self.contents(ui)
            });
        if let Some(result) = modal.inner {
            return Some(result);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            return Some(None);
        }
        None
    }

    fn contents(&mut self, ui: &mut Ui) -> Option<Option<Vec<String>>> {
        let area = ui.max_rect();
        widgets::label(
            ui,
            strings::IGNORED_COMMENTS_TITLE,
            theme::semibold(14.0),
            theme::palette().text,
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let width = ui.available_width();
            widgets::search_box(
                ui,
                "ignored_comments_search",
                &mut self.search,
                width,
                strings::COMMON_FILTER_PLACEHOLDER,
            );
        });
        ui.add_space(10.0);

        let visible = self.visible();
        let master = master_state(visible.iter().map(|index| self.files[*index].selected));
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let (rect, _) = ui.allocate_exact_size(vec2(COUNT_COLUMN, 17.0), egui::Sense::hover());
            ui.painter().text(
                pos2(rect.right() - 10.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                strings::IGNORED_COMMENTS_CHARS_HEADER,
                theme::semibold(11.0),
                theme::palette().text_muted,
            );
            if widgets::check_row(ui, master, strings::COMMON_SELECT_ALL, true).clicked() {
                let select = master != CheckState::Checked;
                for index in &visible {
                    self.files[*index].selected = select;
                }
            }
        });
        widgets::separator(ui);

        let button_row = widgets::control_height(ui);
        let list_bottom = area.bottom() - button_row - 15.0;
        let list_rect = Rect::from_min_max(
            pos2(area.left(), ui.cursor().top()),
            pos2(area.right(), list_bottom),
        );
        ui.painter().rect_stroke(
            list_rect,
            4,
            egui::Stroke::new(1.0, theme::palette().border),
            egui::StrokeKind::Inside,
        );
        let mut list_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(list_rect.shrink(2.0))
                .layout(Layout::top_down(Align::Min)),
        );
        let row_height = 19.0;
        egui::ScrollArea::vertical()
            .id_salt("ignored_comments_list")
            .auto_shrink([false, false])
            .show_rows(&mut list_ui, row_height, visible.len(), |ui, range| {
                for position in range {
                    let index = visible[position];
                    let file = &mut self.files[index];
                    ui.horizontal(|ui| {
                        ui.set_height(row_height);
                        ui.add_space(4.0);
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(COUNT_COLUMN, row_height),
                            egui::Sense::hover(),
                        );
                        ui.painter().text(
                            pos2(rect.right() - 15.0, rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            file.count.display(),
                            egui::FontId::monospace(theme::BODY_SIZE),
                            theme::palette().text_muted,
                        );
                        let state = if file.selected {
                            CheckState::Checked
                        } else {
                            CheckState::Unchecked
                        };
                        if widgets::check_row(ui, state, &file.relative_path, false).clicked() {
                            file.selected = !file.selected;
                        }
                    });
                }
            });

        let mut footer = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(area.left(), area.bottom() - button_row),
                    area.max,
                ))
                .layout(Layout::right_to_left(Align::Center)),
        );
        let min = vec2(80.0, 0.0);
        let mut result = None;
        if widgets::button(
            &mut footer,
            strings::COMMON_CANCEL_BUTTON,
            ButtonKind::Secondary,
            true,
            min,
        )
        .clicked()
        {
            result = Some(None);
        }
        footer.add_space(10.0);
        let save = widgets::button(
            &mut footer,
            strings::COMMON_SAVE_BUTTON,
            ButtonKind::Primary,
            true,
            min,
        )
        .clicked()
            || footer.input(|input| input.key_pressed(egui::Key::Enter));
        if save {
            result = Some(Some(
                self.files
                    .iter()
                    .filter(|file| file.selected)
                    .map(|file| file.relative_path.clone())
                    .collect(),
            ));
        }
        footer.with_layout(Layout::left_to_right(Align::Center), |ui| {
            widgets::label(
                ui,
                &format!(
                    "{} {}",
                    strings::IGNORED_COMMENTS_TOTAL_LABEL,
                    format_grouped(self.total())
                ),
                theme::bold(14.0),
                theme::palette().text,
            );
        });
        result
    }
}

/// The select-all state of a list: unchecked when nothing or no item is selected.
pub fn master_state(selected: impl Iterator<Item = bool>) -> CheckState {
    let (mut total, mut count) = (0, 0);
    for value in selected {
        total += 1;
        count += usize::from(value);
    }
    if count == 0 {
        CheckState::Unchecked
    } else if count == total {
        CheckState::Checked
    } else {
        CheckState::Indeterminate
    }
}

/// The frame of the large list dialogs.
pub fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::palette().surface)
        .stroke(egui::Stroke::new(1.0, theme::palette().border))
        .corner_radius(4)
        .inner_margin(15)
}
