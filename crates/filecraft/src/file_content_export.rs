//! The File Content Export tab.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use filecraft_core::directory::IgnoredFolderNames;
use filecraft_core::file_content::{
    FileEntry, FileExportRequest, FileExportResult, PathFilter, available_extensions,
    collect_files, export_file_contents, ignore_case_key,
};
use filecraft_core::folder_tree::{CheckState, FolderScope};
use filecraft_core::path_convert::{absolute_path, relative_path};
use filecraft_core::settings::{
    DEFAULT_FILE_CONTENTS_NAME, FileContentExportSettings, FolderState,
};

use crate::background::Task;
use crate::bulk_search::{BulkSearchDialog, BulkSearchOutcome};
use crate::dialog::Dialog;
use crate::export_footer::{self, ExportOptions, FooterButton, Stat, StatTone};
use crate::folder_tree_view::{self, FoldersAction};
use crate::icons;
use crate::ignored_comments::IgnoredCommentsDialog;
use crate::numbers::{format_grouped, format_percent};
use crate::preset_windows::PathPresetLoadResult;
use crate::requests::{ConfirmAction, TabRequest};
use crate::state::{SharedState, TreeTab};
use crate::strings;
use crate::theme;
use crate::toast::Toast;
use crate::widgets::{self, ButtonKind};

/// Height of the "Files to export" row when no panel is in fullscreen and there is enough room.
const FILES_ROW_HEIGHT: f32 = 240.0;
const FILES_ROW_MIN_HEIGHT: f32 = 170.0;
/// Height the folders and extensions row keeps before the files row gets smaller.
const TOP_ROW_MIN_HEIGHT: f32 = 210.0;
const SELECTED_EXTENSIONS_MAX_HEIGHT: f32 = 80.0;
const EXTENSION_LIST_MAX_HEIGHT: f32 = 240.0;
const CHIP_GAP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fullscreen {
    None,
    Folders,
    Extensions,
    Files,
}

impl Fullscreen {
    fn toggled(self, target: Self) -> Self {
        if self == target { Self::None } else { target }
    }
}

struct FileItem {
    entry: FileEntry,
    selected: bool,
}

/// A file extension found in the selected folders.
struct ExtensionItem {
    name: String,
    selected: bool,
    /// Number of scanned files with this extension.
    count: usize,
}

/// Which files the list shows, by selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionFilter {
    All,
    Selected,
    Unselected,
}

/// Everything a scan depends on. A new scan starts whenever it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanKey {
    source: String,
    scopes: Vec<FolderScope>,
    ignored: Vec<String>,
}

pub struct FileContentExportTab {
    options: ExportOptions,
    fullscreen: Fullscreen,
    /// All files of the selected folders, whatever their extension.
    scanned: Vec<FileEntry>,
    extensions: Vec<ExtensionItem>,
    extension_search: String,
    /// Files with a selected extension, ordered by full path.
    files: Vec<FileItem>,
    /// Indices into `files` that pass the search filter.
    filtered: Vec<usize>,
    filter_dirty: bool,
    search: String,
    selection_filter: SelectionFilter,
    /// Relative paths whose `///` comments are removed during export.
    ignored_comments: Vec<String>,
    /// Saved settings waiting for the next scan, like `_pendingSettings`.
    pending: Option<FileContentExportSettings>,
    scan_key: Option<ScanKey>,
    scan: Option<Task<Vec<FileEntry>>>,
    exporting: Option<(usize, Task<io::Result<FileExportResult>>)>,
    /// The file name chosen when the export was requested, used once it is confirmed.
    pending_file_name: Option<String>,
    ignored_dialog: Option<IgnoredCommentsDialog>,
    bulk_dialog: Option<BulkSearchDialog>,
}

impl Default for FileContentExportTab {
    fn default() -> Self {
        Self {
            options: ExportOptions::new(DEFAULT_FILE_CONTENTS_NAME),
            fullscreen: Fullscreen::None,
            scanned: Vec::new(),
            extensions: Vec::new(),
            extension_search: String::new(),
            files: Vec::new(),
            filtered: Vec::new(),
            filter_dirty: false,
            search: String::new(),
            selection_filter: SelectionFilter::All,
            ignored_comments: Vec::new(),
            pending: None,
            scan_key: None,
            scan: None,
            exporting: None,
            pending_file_name: None,
            ignored_dialog: None,
            bulk_dialog: None,
        }
    }
}

impl FileContentExportTab {
    /// The tab's part of the saved state. While saved settings are still loading they are
    /// returned unchanged.
    pub fn settings(
        &self,
        folder_tree_state: Vec<FolderState>,
        base: &FileContentExportSettings,
    ) -> FileContentExportSettings {
        let (selected_extensions, selected_file_paths, ignored_comment_file_paths) =
            match &self.pending {
                Some(pending) => (
                    pending.selected_extensions.clone(),
                    pending.selected_file_paths.clone(),
                    pending.ignored_comment_file_paths.clone(),
                ),
                None => (
                    self.selected_extensions(),
                    self.files
                        .iter()
                        .filter(|file| file.selected)
                        .map(|file| file.entry.full_path.clone())
                        .collect(),
                    self.ignored_comments.clone(),
                ),
            };
        FileContentExportSettings {
            selected_extensions,
            selected_file_paths,
            ignored_comment_file_paths,
            folder_tree_state,
            output_file_name: self.options.output_file_name.clone(),
            append_timestamp: self.options.append_timestamp,
            extra: base.extra.clone(),
        }
    }

    /// Applies saved settings. Selections are restored after the next scan.
    pub fn apply_settings(&mut self, settings: &FileContentExportSettings) {
        self.options.output_file_name = settings.output_file_name.clone();
        self.options.append_timestamp = settings.append_timestamp;
        self.pending = Some(settings.clone());
        self.scan_key = None;
    }

    fn selected_extensions(&self) -> Vec<String> {
        self.extensions
            .iter()
            .filter(|extension| extension.selected)
            .map(|extension| extension.name.clone())
            .collect()
    }

    fn selected_count(&self) -> usize {
        self.files.iter().filter(|file| file.selected).count()
    }

    /// Why the export button is disabled, or `None` when an export can start.
    fn export_blocker(&self, shared: &SharedState) -> Option<&'static str> {
        self.options.blocker(shared).or_else(|| {
            if self.scan.is_some() {
                Some(strings::WAIT_FOR_SCAN)
            } else if self.selected_count() == 0 {
                Some(strings::FILE_CONTENT_NEEDS_FILES)
            } else {
                None
            }
        })
    }

    fn can_clear_filter(&self) -> bool {
        self.selection_filter != SelectionFilter::All || !self.search.trim().is_empty()
    }

    /// Starts a scan when the selected folders or ignored folders changed and collects finished
    /// scans. Returns true when pending saved settings were applied.
    fn update_scan(&mut self, shared: &SharedState, ctx: &egui::Context) -> bool {
        let mut resolved = false;
        if let Some(result) = self.scan.as_ref().and_then(Task::poll) {
            self.scan = None;
            let files = result.unwrap_or_else(|failed| {
                eprintln!("FileCraft: scanning the selected folders failed: {failed}");
                Vec::new()
            });
            resolved = self.apply_scan(files);
        }

        let key = ScanKey {
            source: shared.source_path.clone(),
            scopes: shared
                .tree(TreeTab::FileContentExport)
                .map(|tree| tree.selection_scopes())
                .unwrap_or_default(),
            ignored: shared.ignored_folders().to_vec(),
        };
        if self.scan_key.as_ref() != Some(&key) {
            self.scan_key = Some(key.clone());
            if key.scopes.is_empty() {
                self.scan = None;
                resolved |= self.apply_scan(Vec::new());
            } else {
                let ignored = IgnoredFolderNames::new(&key.ignored);
                self.scan = Some(Task::spawn(ctx, move || {
                    collect_files(Path::new(&key.source), &key.scopes, &ignored)
                }));
            }
        }
        resolved
    }

    /// Rebuilds extensions and files from a finished scan, keeping current or pending selections.
    fn apply_scan(&mut self, scanned: Vec<FileEntry>) -> bool {
        let pending = self.pending.take();
        let selected_extensions: HashSet<String> = match &pending {
            Some(settings) => settings
                .selected_extensions
                .iter()
                .map(|e| ignore_case_key(e))
                .collect(),
            None => self
                .selected_extensions()
                .iter()
                .map(|e| ignore_case_key(e))
                .collect(),
        };
        let selected_files: HashSet<String> = match &pending {
            Some(settings) => settings
                .selected_file_paths
                .iter()
                .map(|p| ignore_case_key(p))
                .collect(),
            None => self.selected_file_keys(),
        };
        if let Some(settings) = &pending {
            self.ignored_comments = settings.ignored_comment_file_paths.clone();
        }
        self.scanned = scanned;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in &self.scanned {
            *counts.entry(ignore_case_key(&entry.extension)).or_default() += 1;
        }
        self.extensions = available_extensions(&self.scanned)
            .into_iter()
            .map(|name| {
                let key = ignore_case_key(&name);
                ExtensionItem {
                    selected: selected_extensions.contains(&key),
                    count: counts.get(&key).copied().unwrap_or(0),
                    name,
                }
            })
            .collect();
        self.rebuild_files(&selected_files);
        pending.is_some()
    }

    /// Full paths of the selected files.
    pub fn selected_file_paths(&self) -> Vec<String> {
        match &self.pending {
            Some(pending) => pending.selected_file_paths.clone(),
            None => self
                .files
                .iter()
                .filter(|file| file.selected)
                .map(|file| file.entry.full_path.clone())
                .collect(),
        }
    }

    /// Selects the listed files of a path preset. Paths are relative to
    /// `source_path`. Files that are not in the list are reported as not found.
    pub fn load_path_preset(
        &mut self,
        relative_paths: &[String],
        source_path: &str,
    ) -> PathPresetLoadResult {
        let mut result = PathPresetLoadResult {
            total: relative_paths.len(),
            ..PathPresetLoadResult::default()
        };
        if source_path.trim().is_empty() {
            return result;
        }
        let lookup: HashMap<String, usize> = self
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| (ignore_case_key(&file.entry.full_path), index))
            .collect();
        for relative in relative_paths {
            let full_path = absolute_path(source_path, relative);
            match lookup.get(&ignore_case_key(&full_path)) {
                Some(index) => {
                    let file = &mut self.files[*index];
                    if file.selected {
                        result.unchanged += 1;
                    } else {
                        file.selected = true;
                        result.changed += 1;
                    }
                    result.loaded.push(relative.clone());
                }
                None => result
                    .not_found
                    .push(relative_path(source_path, &full_path)),
            }
        }
        self.filter_dirty = true;
        result
    }

    fn selected_file_keys(&self) -> HashSet<String> {
        self.files
            .iter()
            .filter(|file| file.selected)
            .map(|file| ignore_case_key(&file.entry.full_path))
            .collect()
    }

    /// Lists the scanned files whose extension is selected.
    fn rebuild_files(&mut self, selected_files: &HashSet<String>) {
        let extensions: HashSet<String> = self
            .selected_extensions()
            .iter()
            .map(|extension| ignore_case_key(extension))
            .collect();
        self.files = self
            .scanned
            .iter()
            .filter(|entry| extensions.contains(&ignore_case_key(&entry.extension)))
            .map(|entry| FileItem {
                selected: selected_files.contains(&ignore_case_key(&entry.full_path)),
                entry: entry.clone(),
            })
            .collect();
        self.filter_dirty = true;
    }

    fn update_filter(&mut self) {
        if !self.filter_dirty {
            return;
        }
        self.filter_dirty = false;
        let filter = PathFilter::new(&self.search);
        self.filtered = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| {
                let shown = match self.selection_filter {
                    SelectionFilter::All => true,
                    SelectionFilter::Selected => file.selected,
                    SelectionFilter::Unselected => !file.selected,
                };
                shown && filter.matches(&file.entry.full_path)
            })
            .map(|(index, _)| index)
            .collect();
    }

    fn set_file_selected(&mut self, index: usize, selected: bool) {
        self.files[index].selected = selected;
        if self.selection_filter != SelectionFilter::All {
            self.filter_dirty = true;
        }
    }

    fn master_state(&self) -> CheckState {
        let selected = self
            .filtered
            .iter()
            .filter(|index| self.files[**index].selected)
            .count();
        check_state(selected, self.filtered.len())
    }

    /// Asks before exporting. The question warns when the export would replace a file.
    fn request_export(&mut self, shared: &SharedState) -> TabRequest {
        let (file_name, path) = self.options.target(&shared.destination_path);
        self.pending_file_name = Some(file_name);
        let dialog = Dialog::export_confirmation(
            strings::FILE_CONTENT_EXPORT_TITLE,
            strings::FILE_CONTENT_CONFIRM_EXPORT_MESSAGE,
            &path,
            (
                self.selected_count(),
                strings::CONFIRMATION_FILES_AFFECTED_LABEL,
            ),
        );
        TabRequest::Confirm(dialog, ConfirmAction::ExportFileContents)
    }

    /// Starts writing the export file on a background thread.
    pub fn start_export(&mut self, shared: &SharedState, ctx: &egui::Context) {
        let files: Vec<FileEntry> = self
            .files
            .iter()
            .filter(|file| file.selected)
            .map(|file| file.entry.clone())
            .collect();
        let count = files.len();
        let file_name = self
            .pending_file_name
            .take()
            .unwrap_or_else(|| self.options.target(&shared.destination_path).0);
        let request = FileExportRequest {
            destination_path: PathBuf::from(&shared.destination_path),
            file_name,
            files,
            ignore_comments_for: self.ignored_comments.clone(),
        };
        self.exporting = Some((
            count,
            Task::spawn(ctx, move || export_file_contents(&request)),
        ));
    }

    fn poll_export(&mut self) -> Option<TabRequest> {
        let (count, task) = self.exporting.as_ref()?;
        let result = task.poll()?;
        let count = *count;
        self.exporting = None;
        let dialog = match result {
            Ok(Ok(result)) => {
                let toast = export_toast(count, result);
                return Some(TabRequest::Notify(toast));
            }
            Ok(Err(error)) => export_error(&error.to_string()),
            Err(failed) => export_error(&failed.to_string()),
        };
        Some(TabRequest::ShowDialog(dialog))
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut SharedState) -> Option<TabRequest> {
        let ctx = ui.ctx().clone();
        let mut request = self.poll_export();
        if self.update_scan(shared, &ctx) {
            request = request.or(Some(TabRequest::SettingsResolved));
        }
        self.update_filter();

        let full = ui.available_rect_before_wrap();
        let selected = self.selected_count();
        let stats = [
            Stat::count(
                self.files.len(),
                strings::FILE_CONTENT_TOTAL_FILES_LABEL,
                StatTone::Neutral,
            ),
            Stat::count(
                self.filtered.len(),
                strings::FILE_CONTENT_FILTERED_FILES_LABEL,
                StatTone::Neutral,
            ),
            Stat::count(
                selected,
                strings::FILE_CONTENT_SELECTED_FILES_LABEL,
                StatTone::needed(selected),
            ),
        ];
        let button_text = strings::file_content_export_button(selected);
        let button = FooterButton {
            text: &button_text,
            disabled_reason: self.export_blocker(shared),
            busy_text: self
                .exporting
                .is_some()
                .then_some(strings::EXPORTING_BUTTON),
        };
        let footer = if self.fullscreen == Fullscreen::None {
            export_footer::footer_height(ui, full.width(), &stats, None, &button)
        } else {
            0.0
        };
        let files_height = match self.fullscreen {
            Fullscreen::None => FILES_ROW_HEIGHT
                .min(full.height() - footer - TOP_ROW_MIN_HEIGHT)
                .max(FILES_ROW_MIN_HEIGHT),
            Fullscreen::Files => full.height(),
            Fullscreen::Folders | Fullscreen::Extensions => 0.0,
        };
        let top = Rect::from_min_max(
            full.min,
            pos2(full.right(), full.bottom() - footer - files_height),
        );

        let (folders_rect, extensions_rect) = match self.fullscreen {
            Fullscreen::None => {
                let split = top.left() + top.width() * 0.6;
                (
                    Some(Rect::from_min_max(top.min, pos2(split, top.bottom()))),
                    Some(Rect::from_min_max(pos2(split, top.top()), top.max)),
                )
            }
            Fullscreen::Folders => (Some(top), None),
            Fullscreen::Extensions => (None, Some(top)),
            Fullscreen::Files => (None, None),
        };
        let linked = shared.linked_peers(TreeTab::FileContentExport);
        if let Some(rect) = folders_rect {
            match folder_tree_view::folders_group(
                ui,
                rect.shrink(5.0),
                "file_content_folders",
                shared.tree_mut(TreeTab::FileContentExport),
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
        if let Some(rect) = extensions_rect
            && self.extensions_ui(ui, rect.shrink(5.0))
        {
            request = Some(TabRequest::OpenPathPresets);
        }
        if files_height > 0.0 {
            let rect = Rect::from_min_max(
                pos2(full.left(), top.bottom()),
                pos2(full.right(), top.bottom() + files_height),
            );
            self.files_ui(ui, rect.shrink(5.0));
        }
        if footer > 0.0 {
            let rect = Rect::from_min_max(pos2(full.left(), full.bottom() - footer), full.max);
            if export_footer::footer_ui(
                ui,
                rect,
                "file_content_file_name",
                &mut self.options,
                &stats,
                None,
                button,
            ) {
                request = Some(self.request_export(shared));
            }
        }
        ui.advance_cursor_after_rect(full);

        self.dialogs_ui(&ctx);
        request
    }

    /// Returns true when "Manage path presets" was clicked.
    fn extensions_ui(&mut self, ui: &mut Ui, rect: Rect) -> bool {
        let mut toggle = false;
        let mut open_path_presets = false;
        let active = self.fullscreen == Fullscreen::Extensions;
        let mut open_ignored = false;
        widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                widgets::card_title(ui, strings::FILE_CONTENT_EXTENSIONS_HEADER);
            },
            |ui| toggle = widgets::fullscreen_header_toggle(ui, active),
            |ui| {
                let body = ui.max_rect();
                let control = widgets::control_height(ui);
                let buttons_height = control * 2.0 + 5.0;
                // The chips get the room that is left after the picker, the buttons and a
                // one-line ignored files box.
                let fixed = 5.0 + control + 10.0 + control + 5.0 + buttons_height;
                let chips_max = (body.height() - fixed)
                    .min(SELECTED_EXTENSIONS_MAX_HEIGHT)
                    .max(control);
                let chips_height = self.chips_height(ui, body.width()).min(chips_max);
                let chips_rect = Rect::from_min_size(body.min, vec2(body.width(), chips_height));
                self.chips_ui(ui, chips_rect);

                let combo_top = chips_rect.bottom() + 5.0;
                let mut combo_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_size(
                            pos2(body.left(), combo_top),
                            vec2(body.width(), control),
                        ))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                self.extension_picker(&mut combo_ui, body.width());

                let buttons_top = (body.bottom() - buttons_height).max(combo_top + control + 5.0);
                let ignored_rect = Rect::from_min_max(
                    pos2(body.left(), combo_top + control + 10.0),
                    pos2(body.right(), buttons_top - 5.0),
                );
                if ignored_rect.height() >= control {
                    widgets::info_box(
                        ui,
                        ignored_rect,
                        "ignored_comment_files",
                        &self.ignored_files_text(),
                    );
                }

                let mut buttons = ui.new_child(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_max(
                            pos2(body.left(), buttons_top),
                            pos2(body.right(), buttons_top + buttons_height),
                        ))
                        .layout(Layout::top_down(Align::Min)),
                );
                let full_width = vec2(body.width(), 0.0);
                let has_selection = self.selected_count() > 0;
                open_ignored = widgets::button(
                    &mut buttons,
                    strings::FILE_CONTENT_IGNORE_COMMENTS_BUTTON,
                    ButtonKind::Secondary,
                    has_selection,
                    full_width,
                )
                .on_hover_text(strings::FILE_CONTENT_IGNORE_COMMENTS_TOOLTIP)
                .clicked()
                    && has_selection;
                buttons.add_space(5.0);
                open_path_presets = widgets::button(
                    &mut buttons,
                    strings::FILE_CONTENT_MANAGE_PATH_PRESETS_BUTTON,
                    ButtonKind::Secondary,
                    true,
                    full_width,
                )
                .clicked();
            },
        );
        if toggle {
            self.fullscreen = self.fullscreen.toggled(Fullscreen::Extensions);
        }
        if open_ignored {
            let selected: Vec<FileEntry> = self
                .files
                .iter()
                .filter(|file| file.selected)
                .map(|file| file.entry.clone())
                .collect();
            self.ignored_dialog = Some(IgnoredCommentsDialog::new(
                ui.ctx(),
                &selected,
                &self.ignored_comments,
            ));
        }
        open_path_presets
    }

    /// Width of the chip of a selected extension.
    fn chip_width(ui: &Ui, name: &str) -> f32 {
        8.0 + widgets::text_width(ui, name, theme::body(theme::BODY_SIZE)) + 4.0 + 14.0 + 4.0
    }

    /// Height the selected extension chips need when wrapped at `width`.
    fn chips_height(&self, ui: &Ui, width: f32) -> f32 {
        let inner = width - 2.0 - CHIP_GAP * 2.0;
        let mut lines = 1;
        let mut x = 0.0;
        for extension in self
            .extensions
            .iter()
            .filter(|extension| extension.selected)
        {
            let chip = Self::chip_width(ui, &extension.name);
            if x > 0.0 && x + chip > inner {
                lines += 1;
                x = 0.0;
            }
            x += chip + CHIP_GAP;
        }
        2.0 + CHIP_GAP + lines as f32 * (22.0 + CHIP_GAP)
    }

    /// The selected extensions as removable chips in a read-only box.
    fn chips_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let colors = theme::palette();
        ui.painter().rect(
            rect,
            4,
            colors.surface_alt,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        let inner_rect = rect.shrink(1.0 + CHIP_GAP);
        let mut inner = ui.new_child(
            UiBuilder::new()
                .max_rect(inner_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        let mut removed = None;
        egui::ScrollArea::vertical()
            .id_salt("selected_extensions")
            .auto_shrink([false, false])
            .show(&mut inner, |ui| {
                if !self.extensions.iter().any(|extension| extension.selected) {
                    let (line, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
                    ui.painter().text(
                        pos2(line.left() + 4.0, line.center().y),
                        egui::Align2::LEFT_CENTER,
                        strings::FILE_CONTENT_NO_EXTENSIONS_SELECTED,
                        theme::body(theme::BODY_SIZE),
                        colors.text_muted,
                    );
                    return;
                }
                ui.spacing_mut().item_spacing = vec2(CHIP_GAP, CHIP_GAP);
                ui.horizontal_wrapped(|ui| {
                    for (index, extension) in self.extensions.iter().enumerate() {
                        if extension.selected
                            && widgets::chip(
                                ui,
                                &extension.name,
                                strings::FILE_CONTENT_REMOVE_EXTENSION_TOOLTIP,
                            )
                        {
                            removed = Some(index);
                        }
                    }
                });
            });
        if let Some(index) = removed {
            self.extensions[index].selected = false;
            let keep = self.selected_file_keys();
            self.rebuild_files(&keep);
        }
    }

    /// The drop-down list of extensions with a search box, a select-all row and file counts.
    fn extension_picker(&mut self, ui: &mut Ui, width: f32) {
        let selected = self
            .extensions
            .iter()
            .filter(|extension| extension.selected)
            .count();
        let text = if self.extensions.is_empty() {
            strings::FILE_CONTENT_NO_EXTENSIONS_FOUND.to_owned()
        } else if selected == 0 {
            strings::FILE_CONTENT_EXTENSIONS_PLACEHOLDER.to_owned()
        } else {
            strings::extensions_selected(selected, self.extensions.len())
        };
        let response = widgets::combo_field(ui, width, &text);
        let mut changed = false;
        let search = &mut self.extension_search;
        let extensions = &mut self.extensions;
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(width)
            .show(|ui| {
                ui.set_min_width(width - 12.0);
                widgets::search_box(
                    ui,
                    "extension_search",
                    search,
                    width - 12.0,
                    strings::FILE_CONTENT_EXTENSION_SEARCH_PLACEHOLDER,
                );
                ui.add_space(4.0);
                let filter = search.trim().to_lowercase();
                let visible: Vec<usize> = (0..extensions.len())
                    .filter(|index| {
                        filter.is_empty()
                            || extensions[*index].name.to_lowercase().contains(&filter)
                    })
                    .collect();
                let master = check_state(
                    visible
                        .iter()
                        .filter(|index| extensions[**index].selected)
                        .count(),
                    visible.len(),
                );
                if !visible.is_empty()
                    && widgets::check_row(ui, master, strings::COMMON_SELECT_ALL, true).clicked()
                {
                    let select = master != CheckState::Checked;
                    for index in &visible {
                        extensions[*index].selected = select;
                    }
                    changed = true;
                }
                widgets::separator(ui);
                // The list height follows the visible rows, so it grows again when the search is
                // cleared instead of keeping the height of the last filtered list.
                let row_height = widgets::check_row_height(ui);
                let list_height =
                    (row_height * visible.len() as f32).min(EXTENSION_LIST_MAX_HEIGHT);
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .min_scrolled_height(list_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for index in visible {
                            let extension = &mut extensions[index];
                            let state = if extension.selected {
                                CheckState::Checked
                            } else {
                                CheckState::Unchecked
                            };
                            let row = widgets::check_row(ui, state, &extension.name, false);
                            ui.painter().text(
                                pos2(row.rect.right() - 6.0, row.rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                extension.count.to_string(),
                                theme::body(theme::BODY_SIZE),
                                theme::palette().text_hint,
                            );
                            if row.clicked() {
                                extension.selected = !extension.selected;
                                changed = true;
                            }
                        }
                    });
            });
        if changed {
            let keep = self.selected_file_keys();
            self.rebuild_files(&keep);
        }
    }

    fn ignored_files_text(&self) -> String {
        if self.ignored_comments.is_empty() {
            return strings::FILE_CONTENT_NO_IGNORED_FILES.to_owned();
        }
        let full = self.fullscreen == Fullscreen::Extensions;
        let mut paths: Vec<&String> = self.ignored_comments.iter().collect();
        paths.sort_by(|a, b| filecraft_core::file_content::compare_paths(a, b));
        paths
            .iter()
            .map(|path| {
                if full {
                    (*path).clone()
                } else {
                    Path::new(path.as_str())
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| (*path).clone())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn files_ui(&mut self, ui: &mut Ui, rect: Rect) {
        // The extensions panel above can change the files in this frame.
        self.update_filter();
        let mut toggle = false;
        let active = self.fullscreen == Fullscreen::Files;
        let scanning = self.scan.is_some();
        widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                widgets::card_title(ui, strings::FILE_CONTENT_FILES_HEADER);
                if scanning {
                    ui.add_space(10.0);
                    widgets::busy_label(ui, strings::SCANNING_LABEL);
                }
            },
            |ui| toggle = widgets::fullscreen_header_toggle(ui, active),
            |ui| {
                self.filter_row(ui);
                ui.add_space(2.0);
                let state = self.master_state();
                ui.horizontal(|ui| {
                    if widgets::check_row(ui, state, strings::COMMON_SELECT_ALL, true).clicked() {
                        let select = state != CheckState::Checked;
                        for index in self.filtered.clone() {
                            self.set_file_selected(index, select);
                        }
                    }
                });
                widgets::separator(ui);
                self.file_list(ui);
            },
        );
        if toggle {
            self.fullscreen = self.fullscreen.toggled(Fullscreen::Files);
        }
    }

    /// Search box on the left, then bulk search, the selection filter and clear filter, laid out
    /// from the right.
    fn filter_row(&mut self, ui: &mut Ui) {
        let control = widgets::control_height(ui);
        let row_height = control + 4.0;
        let (row, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
        let row = row.shrink2(vec2(5.0, 2.0));

        let filter_options = [
            strings::FILE_CONTENT_FILTER_ALL,
            strings::FILE_CONTENT_FILTER_SELECTED,
            strings::FILE_CONTENT_FILTER_UNSELECTED,
        ];
        let clear_width = widgets::button_width(ui, strings::FILE_CONTENT_CLEAR_FILTER_BUTTON);
        let segmented_width = widgets::segmented_width(ui, &filter_options);
        let bulk_width = widgets::button_width(ui, strings::FILE_CONTENT_BULK_SEARCH_BUTTON);
        let mut right = row.right();
        let mut take = |width: f32, gap: f32| {
            let rect =
                Rect::from_min_max(pos2(right - width, row.top()), pos2(right, row.bottom()));
            right -= width + gap;
            rect
        };
        let clear_rect = take(clear_width, 10.0);
        let segmented_rect = take(segmented_width, 10.0);
        let bulk_rect = take(bulk_width, 10.0);
        let search_rect = Rect::from_min_max(row.min, pos2(right, row.bottom()));

        let centered = |ui: &mut Ui, rect: Rect| {
            ui.new_child(
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
            )
        };

        let mut search_ui = centered(ui, search_rect);
        let width = search_ui.available_width().max(40.0);
        if widgets::search_box(
            &mut search_ui,
            "file_content_search",
            &mut self.search,
            width,
            strings::COMMON_FILTER_PLACEHOLDER,
        )
        .changed()
        {
            self.filter_dirty = true;
        }

        let has_files = !self.files.is_empty();
        if widgets::button(
            &mut centered(ui, bulk_rect),
            strings::FILE_CONTENT_BULK_SEARCH_BUTTON,
            ButtonKind::Secondary,
            has_files,
            egui::Vec2::ZERO,
        )
        .clicked()
            && has_files
        {
            let paths: Vec<(String, bool)> = self
                .files
                .iter()
                .map(|file| (file.entry.relative_path.clone(), file.selected))
                .collect();
            self.bulk_dialog = Some(BulkSearchDialog::new(paths));
        }

        let filters = [
            SelectionFilter::All,
            SelectionFilter::Selected,
            SelectionFilter::Unselected,
        ];
        let current = filters
            .iter()
            .position(|filter| *filter == self.selection_filter)
            .unwrap_or(0);
        if let Some(index) =
            widgets::segmented(&mut centered(ui, segmented_rect), &filter_options, current)
            && filters[index] != self.selection_filter
        {
            self.selection_filter = filters[index];
            self.filter_dirty = true;
        }

        let can_clear = self.can_clear_filter();
        if widgets::button(
            &mut centered(ui, clear_rect),
            strings::FILE_CONTENT_CLEAR_FILTER_BUTTON,
            ButtonKind::Secondary,
            can_clear,
            egui::Vec2::ZERO,
        )
        .clicked()
            && can_clear
        {
            self.search.clear();
            self.selection_filter = SelectionFilter::All;
            self.filter_dirty = true;
        }
    }

    /// Why the file list is empty.
    fn empty_message(&self) -> &'static str {
        if self.scan.is_some() {
            strings::SCANNING_LABEL
        } else if self
            .scan_key
            .as_ref()
            .is_none_or(|key| key.scopes.is_empty())
        {
            strings::FILE_CONTENT_EMPTY_NO_FOLDERS
        } else if self.scanned.is_empty() {
            strings::FILE_CONTENT_NO_EXTENSIONS_FOUND
        } else if self.files.is_empty() {
            strings::FILE_CONTENT_EMPTY_NO_EXTENSIONS
        } else {
            strings::FILE_CONTENT_EMPTY_NO_MATCH
        }
    }

    fn file_list(&mut self, ui: &mut Ui) {
        if self.filtered.is_empty() {
            let rect = ui.available_rect_before_wrap();
            ui.painter().text(
                pos2(rect.center().x, rect.top() + 30.0),
                egui::Align2::CENTER_CENTER,
                self.empty_message(),
                theme::body(theme::BODY_SIZE),
                theme::palette().text_muted,
            );
            return;
        }
        let row_height = ui.text_style_height(&egui::TextStyle::Body).max(15.0) + 4.0;
        let mut toggled = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("file_content_files")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, self.filtered.len(), |ui, range| {
                for position in range {
                    let index = self.filtered[position];
                    let file = &self.files[index];
                    let state = if file.selected {
                        CheckState::Checked
                    } else {
                        CheckState::Unchecked
                    };
                    ui.add_space(1.0);
                    if widgets::path_check_row(ui, state, &file.entry.relative_path).clicked() {
                        toggled.push(index);
                    }
                    ui.add_space(1.0);
                }
            });
        for index in toggled {
            let selected = !self.files[index].selected;
            self.set_file_selected(index, selected);
        }
    }

    fn dialogs_ui(&mut self, ctx: &egui::Context) {
        if let Some(dialog) = &mut self.ignored_dialog
            && let Some(result) = dialog.show(ctx)
        {
            if let Some(paths) = result {
                self.ignored_comments = paths;
            }
            self.ignored_dialog = None;
        }
        if let Some(dialog) = &mut self.bulk_dialog
            && let Some(outcome) = dialog.show(ctx)
        {
            if let BulkSearchOutcome::Apply(changes) = outcome {
                for (index, selected) in changes {
                    self.files[index].selected = selected;
                }
                self.filter_dirty = true;
            }
            self.bulk_dialog = None;
        }
    }
}

/// The results of an export as badges: files, characters and lines, then what removing `///`
/// comments saved.
fn export_toast(count: usize, result: FileExportResult) -> Toast {
    let units = |value: u64, unit: &str| format!("{} {unit}", format_grouped(value));
    let total = result.exported_characters + result.ignored_characters;
    let savings = if total > 0 {
        result.ignored_characters as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    Toast::success(strings::TOAST_FILE_CONTENTS_EXPORTED, String::new())
        .with_facts(vec![
            strings::count_of(count, "file", "files"),
            units(result.exported_characters, strings::CHARACTERS_UNIT),
            units(result.exported_lines, strings::LINES_UNIT),
        ])
        .with_fact_group(
            strings::TOAST_REMOVED_COMMENTS_CAPTION,
            vec![
                strings::count_of(result.files_with_ignored_comments as usize, "file", "files"),
                units(result.ignored_characters, strings::CHARACTERS_UNIT),
                units(result.ignored_lines, strings::LINES_UNIT),
                strings::percent_smaller(&format_percent(savings)),
            ],
        )
        .with_file(result.output_path)
}

fn export_error(message: &str) -> Dialog {
    Dialog::notification(
        strings::COMMON_ERROR_TITLE,
        strings::file_content_export_error(message),
        icons::ERROR,
    )
}

/// The state of a select-all check box over `total` items of which `selected` are checked.
fn check_state(selected: usize, total: usize) -> CheckState {
    if selected == 0 {
        CheckState::Unchecked
    } else if selected == total {
        CheckState::Checked
    } else {
        CheckState::Indeterminate
    }
}
