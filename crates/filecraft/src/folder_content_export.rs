//! The Folder Content Export tab.

use std::io;
use std::path::PathBuf;

use egui::{Align, Layout, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use filecraft_core::directory::IgnoredFolderNames;
use filecraft_core::folder_content::{
    COLUMNS, ColumnGroup, FolderExportRequest, collect_all_files, export_folder_contents,
};
use filecraft_core::folder_tree::CheckState;
use filecraft_core::settings::{
    DEFAULT_FOLDER_CONTENTS_NAME, FolderContentExportSettings, FolderState,
};

use crate::background::Task;
use crate::dialog::Dialog;
use crate::export_footer::{self, ExportOptions, FooterButton, FooterCheck, Stat, StatTone};
use crate::folder_tree_view::{self, FoldersAction};
use crate::icons;
use crate::ignored_comments::master_state;
use crate::requests::{ConfirmAction, TabRequest};
use crate::state::{SharedState, TreeTab};
use crate::strings;
use crate::theme;
use crate::toast::Toast;
use crate::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fullscreen {
    None,
    Folders,
    Details,
}

impl Fullscreen {
    fn toggled(self, target: Self) -> Self {
        if self == target { Self::None } else { target }
    }
}

/// What the affected file count was computed for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CountKey {
    folders: Vec<PathBuf>,
    ignored: Vec<String>,
}

pub struct FolderContentExportTab {
    options: ExportOptions,
    /// Selection of each column, by position in `COLUMNS`.
    selected: [bool; COLUMNS.len()],
    protect_spreadsheet_formulas: bool,
    /// Expansion of each column group, by position in `ColumnGroup::ALL`.
    expanded: [bool; ColumnGroup::ALL.len()],
    fullscreen: Fullscreen,
    affected_files: usize,
    count_key: Option<CountKey>,
    counting: Option<Task<usize>>,
    exporting: Option<Task<io::Result<PathBuf>>>,
    /// The file name chosen when the export was requested, used once it is confirmed.
    pending_file_name: Option<String>,
}

impl Default for FolderContentExportTab {
    fn default() -> Self {
        Self {
            options: ExportOptions::new(DEFAULT_FOLDER_CONTENTS_NAME),
            selected: COLUMNS.map(|column| column.default_selected),
            protect_spreadsheet_formulas: false,
            expanded: ColumnGroup::ALL.map(|group| group != ColumnGroup::Diagnostics),
            fullscreen: Fullscreen::None,
            affected_files: 0,
            count_key: None,
            counting: None,
            exporting: None,
            pending_file_name: None,
        }
    }
}

impl FolderContentExportTab {
    /// The tab's part of the saved state.
    pub fn settings(
        &self,
        folder_tree_state: Vec<FolderState>,
        base: &FolderContentExportSettings,
    ) -> FolderContentExportSettings {
        FolderContentExportSettings {
            selected_columns: self.selected_column_ids(),
            folder_tree_state,
            protect_spreadsheet_formulas: self.protect_spreadsheet_formulas,
            output_file_name: self.options.output_file_name.clone(),
            append_timestamp: self.options.append_timestamp,
            extra: base.extra.clone(),
        }
    }

    /// Applies saved settings. Unknown column identifiers are dropped.
    pub fn apply_settings(&mut self, settings: &FolderContentExportSettings) {
        self.options.output_file_name = settings.output_file_name.clone();
        self.options.append_timestamp = settings.append_timestamp;
        self.protect_spreadsheet_formulas = settings.protect_spreadsheet_formulas;
        self.selected =
            COLUMNS.map(|column| settings.selected_columns.iter().any(|id| id == column.id));
    }

    /// Selected column identifiers in column order.
    fn selected_column_ids(&self) -> Vec<String> {
        COLUMNS
            .iter()
            .zip(self.selected)
            .filter(|(_, selected)| *selected)
            .map(|(column, _)| column.id.to_owned())
            .collect()
    }

    /// Why the export button is disabled, or `None` when an export can start.
    fn export_blocker(&self, shared: &SharedState) -> Option<&'static str> {
        self.options.blocker(shared).or_else(|| {
            if Self::checked_folders(shared).is_empty() {
                Some(strings::NEEDS_FOLDERS)
            } else if !self.selected.contains(&true) {
                Some(strings::FOLDER_CONTENT_NEEDS_COLUMNS)
            } else {
                None
            }
        })
    }

    fn checked_folders(shared: &SharedState) -> Vec<PathBuf> {
        shared
            .tree(TreeTab::FolderContentExport)
            .map(|tree| tree.checked_folders())
            .unwrap_or_default()
    }

    /// Counts the files of the selected folders in the background whenever the selection or the
    /// ignored folders change.
    fn update_count(&mut self, shared: &SharedState, ctx: &egui::Context) {
        if let Some(result) = self.counting.as_ref().and_then(Task::poll) {
            self.counting = None;
            self.affected_files = result.unwrap_or_else(|failed| {
                eprintln!("FileCraft: counting the selected files failed: {failed}");
                0
            });
        }
        let key = CountKey {
            folders: Self::checked_folders(shared),
            ignored: shared.ignored_folders().to_vec(),
        };
        if self.count_key.as_ref() == Some(&key) {
            return;
        }
        self.count_key = Some(key.clone());
        if key.folders.is_empty() {
            self.counting = None;
            self.affected_files = 0;
        } else {
            let ignored = IgnoredFolderNames::new(&key.ignored);
            self.counting = Some(Task::spawn(ctx, move || {
                collect_all_files(&key.folders, &ignored).len()
            }));
        }
    }

    /// Checks the selection, then asks before exporting. The question
    /// warns when the export would replace a file.
    fn request_export(&mut self, shared: &SharedState) -> TabRequest {
        let info = |message: &str| {
            TabRequest::ShowDialog(Dialog::notification(
                strings::COMMON_INFO_TITLE,
                message.to_owned(),
                icons::INFO,
            ))
        };
        if Self::checked_folders(shared).is_empty() {
            return info(strings::FOLDER_CONTENT_NO_FOLDERS_SELECTED);
        }
        if !self.selected.contains(&true) {
            return info(strings::FOLDER_CONTENT_NO_COLUMNS_SELECTED);
        }
        let (file_name, path) = self.options.target(&shared.destination_path);
        self.pending_file_name = Some(file_name);
        let dialog = Dialog::export_confirmation(
            strings::FOLDER_CONTENT_EXPORT_TITLE,
            strings::FOLDER_CONTENT_CONFIRM_EXPORT_MESSAGE,
            &path,
            (
                self.affected_files,
                strings::CONFIRMATION_FILES_AFFECTED_LABEL,
            ),
        );
        TabRequest::Confirm(dialog, ConfirmAction::ExportFolderContents)
    }

    /// Writes the export file on a background thread.
    pub fn start_export(&mut self, shared: &SharedState, ctx: &egui::Context) {
        let file_name = self
            .pending_file_name
            .take()
            .unwrap_or_else(|| self.options.target(&shared.destination_path).0);
        let request = FolderExportRequest {
            destination_path: PathBuf::from(&shared.destination_path),
            file_name,
            folders: Self::checked_folders(shared),
            column_ids: self.selected_column_ids(),
            protect_spreadsheet_formulas: self.protect_spreadsheet_formulas,
            ignored: shared.ignored_folder_names(),
        };
        self.exporting = Some(Task::spawn(ctx, move || export_folder_contents(&request)));
    }

    fn poll_export(&mut self) -> Option<TabRequest> {
        let result = self.exporting.as_ref()?.poll()?;
        self.exporting = None;
        let dialog = match result {
            Ok(Ok(path)) => {
                let toast = Toast::success(strings::TOAST_FOLDER_CONTENTS_EXPORTED, String::new())
                    .with_file(path)
                    .with_facts(vec![strings::count_of(
                        self.affected_files,
                        "file",
                        "files",
                    )]);
                return Some(TabRequest::Notify(toast));
            }
            Ok(Err(error)) => error_dialog(&error.to_string()),
            Err(failed) => error_dialog(&failed.to_string()),
        };
        Some(TabRequest::ShowDialog(dialog))
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut SharedState) -> Option<TabRequest> {
        let mut request = self.poll_export();
        self.update_count(shared, ui.ctx());

        let full = ui.available_rect_before_wrap();
        let stats = [if self.counting.is_some() {
            Stat {
                value: strings::COUNTING_LABEL.to_owned(),
                label: "",
                tone: StatTone::Neutral,
            }
        } else {
            Stat::count(
                self.affected_files,
                strings::FOLDER_CONTENT_FILES_AFFECTED_LABEL,
                StatTone::needed(self.affected_files),
            )
        }];
        let button = FooterButton {
            text: strings::FOLDER_CONTENT_EXPORT_BUTTON,
            disabled_reason: self.export_blocker(shared),
            busy_text: self
                .exporting
                .is_some()
                .then_some(strings::EXPORTING_BUTTON),
        };
        let bottom_height = if self.fullscreen == Fullscreen::None {
            export_footer::footer_height(
                ui,
                full.width(),
                &stats,
                Some(strings::FOLDER_CONTENT_PROTECT_SPREADSHEET_FORMULAS),
                &button,
            )
        } else {
            0.0
        };
        let top = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - bottom_height));
        let (folders_rect, details_rect) = match self.fullscreen {
            Fullscreen::None => {
                let split = top.left() + top.width() * 0.7;
                (
                    Some(Rect::from_min_max(top.min, pos2(split, top.bottom()))),
                    Some(Rect::from_min_max(pos2(split, top.top()), top.max)),
                )
            }
            Fullscreen::Folders => (Some(top), None),
            Fullscreen::Details => (None, Some(top)),
        };

        let linked = shared.linked_peers(TreeTab::FolderContentExport);
        if let Some(rect) = folders_rect {
            match folder_tree_view::folders_group(
                ui,
                rect.shrink(5.0),
                "folder_content_folders",
                shared.tree_mut(TreeTab::FolderContentExport),
                &linked,
                self.fullscreen == Fullscreen::Folders,
            ) {
                Some(FoldersAction::ToggleFullscreen) => {
                    self.fullscreen = self.fullscreen.toggled(Fullscreen::Folders);
                }
                Some(FoldersAction::ChooseSource) => {
                    request = Some(TabRequest::ChooseSourceFolder);
                }
                None => {}
            }
        }
        if let Some(rect) = details_rect {
            self.details_ui(ui, rect.shrink(5.0));
        }
        if bottom_height > 0.0 {
            let bottom = Rect::from_min_max(pos2(full.left(), top.bottom()), full.max);
            if export_footer::footer_ui(
                ui,
                bottom,
                "folder_content_file_name",
                &mut self.options,
                &stats,
                Some(FooterCheck {
                    value: &mut self.protect_spreadsheet_formulas,
                    text: strings::FOLDER_CONTENT_PROTECT_SPREADSHEET_FORMULAS,
                    tooltip: strings::FOLDER_CONTENT_PROTECT_SPREADSHEET_FORMULAS_TOOLTIP,
                }),
                button,
            ) {
                request = Some(self.request_export(shared));
            }
        }
        ui.advance_cursor_after_rect(full);
        request
    }

    /// The "Select details" card: a summary row that selects or clears every column, then
    /// the column groups.
    fn details_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let mut toggle_fullscreen = false;
        let fullscreen_active = self.fullscreen == Fullscreen::Details;
        widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                widgets::card_title(ui, strings::FOLDER_CONTENT_DETAILS_HEADER);
            },
            |ui| toggle_fullscreen = widgets::fullscreen_header_toggle(ui, fullscreen_active),
            |ui| {
                let selected = self.selected.iter().filter(|selected| **selected).count();
                let all = selected == COLUMNS.len();
                let link = if all {
                    strings::COMMON_CLEAR_ALL
                } else {
                    strings::COMMON_SELECT_ALL
                };
                if summary_row(
                    ui,
                    &strings::folder_content_columns_selected(selected, COLUMNS.len()),
                    link,
                ) {
                    self.selected = [!all; COLUMNS.len()];
                }
                widgets::separator(ui);
                egui::ScrollArea::vertical()
                    .id_salt("folder_content_columns")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, group) in ColumnGroup::ALL.into_iter().enumerate() {
                            self.group_ui(ui, index, group);
                            ui.add_space(GROUP_GAP);
                        }
                    });
            },
        );
        if toggle_fullscreen {
            self.fullscreen = self.fullscreen.toggled(Fullscreen::Details);
        }
    }

    /// A column group: a header row with a three-state check box, the selected count and a
    /// chevron, then an indented row per column while the group is expanded.
    fn group_ui(&mut self, ui: &mut Ui, index: usize, group: ColumnGroup) {
        let columns: Vec<usize> = (0..COLUMNS.len())
            .filter(|column| COLUMNS[*column].group == group)
            .collect();
        let state = master_state(columns.iter().map(|column| self.selected[*column]));
        let selected = columns
            .iter()
            .filter(|column| self.selected[**column])
            .count();
        let header = group_header(
            ui,
            state,
            group_name(group),
            &format!("{selected}/{}", columns.len()),
            self.expanded[index],
        );
        if header.check {
            let select = state != CheckState::Checked;
            for column in &columns {
                self.selected[*column] = select;
            }
        } else if header.toggle {
            self.expanded[index] = !self.expanded[index];
        }
        if !self.expanded[index] {
            return;
        }
        let top = ui.cursor().top();
        for column in &columns {
            if column_row(ui, self.selected[*column], COLUMNS[*column].display_name).clicked() {
                self.selected[*column] = !self.selected[*column];
            }
        }
        // A guide line under the group check box ties the rows to their group.
        let x = ui.max_rect().left() + CHECK_BOX_INSET + CHECK_BOX_SIZE / 2.0;
        ui.painter().line_segment(
            [pos2(x, top + 2.0), pos2(x, ui.cursor().top() - 2.0)],
            Stroke::new(1.0, theme::palette().border_subtle),
        );
    }
}

const GROUP_GAP: f32 = 2.0;
const GROUP_ROW_HEIGHT: f32 = 24.0;
const COLUMN_ROW_HEIGHT: f32 = 20.0;
/// How far column rows are indented under their group.
const COLUMN_INDENT: f32 = 18.0;
const CHECK_BOX_SIZE: f32 = 15.0;
/// The space left of a check box in its row.
const CHECK_BOX_INSET: f32 = 4.0;
const CHECK_BOX_TEXT_GAP: f32 = 6.0;

/// A row with muted text and a link on the right. Returns true when the link was clicked.
fn summary_row(ui: &mut Ui, text: &str, link: &str) -> bool {
    ui.horizontal(|ui| {
        ui.add_space(CHECK_BOX_INSET);
        widgets::label(
            ui,
            text,
            theme::body(theme::BODY_SIZE),
            theme::palette().text_muted,
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(CHECK_BOX_INSET);
            widgets::text_link(ui, link).clicked()
        })
        .inner
    })
    .inner
}

/// What was clicked in a column group header.
struct GroupHeader {
    /// The check box, which selects or clears the whole group.
    check: bool,
    /// The rest of the row, which expands or collapses the group.
    toggle: bool,
}

/// The header row of a column group. It highlights on hover like a list row.
fn group_header(
    ui: &mut Ui,
    state: CheckState,
    title: &str,
    count: &str,
    expanded: bool,
) -> GroupHeader {
    let (rect, row) =
        ui.allocate_exact_size(vec2(ui.available_width(), GROUP_ROW_HEIGHT), Sense::click());
    let box_rect = check_box_rect(rect, rect.left() + CHECK_BOX_INSET);
    // Interacted with after the row, so it takes the clicks on the check box.
    let check = ui
        .interact(box_rect.expand(2.0), row.id.with("check"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let row = row.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = widgets::hover_amount(ui, &row, true);
    if ui.is_rect_visible(rect) {
        let colors = theme::palette();
        widgets::paint_row_hover(ui, rect, hover);
        widgets::paint_check_box(ui, box_rect, state, check.hovered());
        let chevron = pos2(rect.right() - 12.0, rect.center().y);
        let direction = if expanded { -1.0 } else { 1.0 };
        let points = [vec2(-4.0, -2.0), vec2(0.0, 2.0), vec2(4.0, -2.0)]
            .map(|offset| chevron + vec2(offset.x, offset.y * direction));
        ui.painter().add(Shape::line(
            points.to_vec(),
            Stroke::new(1.5, colors.text_muted),
        ));
        let count = ui.painter().layout_no_wrap(
            count.to_owned(),
            theme::body(theme::BODY_SIZE),
            colors.text_muted,
        );
        let count_left = chevron.x - 12.0 - count.size().x;
        ui.painter().galley(
            pos2(count_left, rect.center().y - count.size().y / 2.0),
            count,
            colors.text_muted,
        );
        let text_left = box_rect.right() + CHECK_BOX_TEXT_GAP;
        let title = widgets::ellipsized(
            ui,
            title,
            theme::semibold(theme::BODY_SIZE),
            colors.text,
            count_left - 8.0 - text_left,
        );
        ui.painter().galley(
            pos2(text_left, rect.center().y - title.size().y / 2.0),
            title,
            colors.text,
        );
    }
    GroupHeader {
        check: check.clicked(),
        toggle: row.clicked(),
    }
}

/// An indented row with a check box and the column name. Clicking anywhere on it toggles it.
fn column_row(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), COLUMN_ROW_HEIGHT),
        Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = widgets::hover_amount(ui, &response, true);
    if ui.is_rect_visible(rect) {
        let colors = theme::palette();
        let row = Rect::from_min_max(pos2(rect.left() + COLUMN_INDENT, rect.top()), rect.max);
        widgets::paint_row_hover(ui, row, hover);
        let box_rect = check_box_rect(row, row.left() + CHECK_BOX_INSET);
        let state = if selected {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        };
        widgets::paint_check_box(ui, box_rect, state, response.hovered());
        let text_left = box_rect.right() + CHECK_BOX_TEXT_GAP;
        let text = widgets::ellipsized(
            ui,
            text,
            theme::body(theme::BODY_SIZE),
            colors.text,
            row.right() - 4.0 - text_left,
        );
        ui.painter().galley(
            pos2(text_left, row.center().y - text.size().y / 2.0),
            text,
            colors.text,
        );
    }
    response
}

/// The check box of a row, vertically centered, with its left edge at `left`.
fn check_box_rect(row: Rect, left: f32) -> Rect {
    Rect::from_min_size(
        pos2(left, row.center().y - CHECK_BOX_SIZE / 2.0),
        Vec2::splat(CHECK_BOX_SIZE),
    )
}

fn group_name(group: ColumnGroup) -> &'static str {
    match group {
        ColumnGroup::FileDetails => strings::FOLDER_CONTENT_GROUP_FILE_DETAILS,
        ColumnGroup::AudioTags => strings::FOLDER_CONTENT_GROUP_AUDIO_TAGS,
        ColumnGroup::AudioProperties => strings::FOLDER_CONTENT_GROUP_AUDIO_PROPERTIES,
        ColumnGroup::Diagnostics => strings::FOLDER_CONTENT_GROUP_DIAGNOSTICS,
    }
}

fn error_dialog(message: &str) -> Dialog {
    Dialog::notification(
        strings::COMMON_ERROR_TITLE,
        strings::folder_content_error(message),
        icons::ERROR,
    )
}
