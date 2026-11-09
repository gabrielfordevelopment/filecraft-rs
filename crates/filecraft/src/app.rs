//! The main window.

use egui::{
    Align, Align2, Key, KeyboardShortcut, Layout, Modifiers, Rect, Sense, Shape, Stroke, Theme, Ui,
    UiBuilder, pos2, vec2,
};

use std::path::PathBuf;

use filecraft_core::history::History;
use filecraft_core::preferences::{ColorTheme, Preferences, PreferencesStore};
use filecraft_core::presets::{PresetSortCriteria, PresetStore, Presets};
use filecraft_core::settings::{SaveData, format_ignored_folders, parse_ignored_folders};
use filecraft_core::settings_store::SettingsStore;

use presets::PresetDialog;

use crate::dialog::{Dialog, DialogResult};
use crate::file_content_export::FileContentExportTab;
use crate::file_rename::FileRenameTab;
use crate::folder_content_export::FolderContentExportTab;
use crate::icons::{self, Icon};
use crate::options::{OptionsRequest, OptionsTab};
use crate::platform;
use crate::preset_windows::{LoadSummaryWindow, PathPresetsWindow, PresetDetailsWindow};
use crate::requests::{ConfirmAction, TabRequest};
use crate::state::{SharedState, TreeTab};
use crate::strings;
use crate::theme;
use crate::toast::{Toast, Toasts};
use crate::tree_generator::TreeGeneratorTab;
use crate::widgets::{self, ButtonKind};

mod presets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    FileContentExport,
    TreeGenerator,
    FolderContentExport,
    FileRename,
    Options,
}

impl Tab {
    const ALL: [Self; 5] = [
        Self::FileContentExport,
        Self::TreeGenerator,
        Self::FolderContentExport,
        Self::FileRename,
        Self::Options,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::FileContentExport => strings::TAB_FILE_CONTENT_EXPORT,
            Self::TreeGenerator => strings::TAB_TREE_GENERATOR,
            Self::FolderContentExport => strings::TAB_FOLDER_CONTENT_EXPORT,
            Self::FileRename => strings::TAB_FILE_RENAME,
            Self::Options => strings::TAB_OPTIONS,
        }
    }

    /// The `SelectedTabIndex` of the tab. Index 3 is not used, and tabs added later get new
    /// numbers.
    fn index(self) -> i32 {
        match self {
            Self::FileContentExport => 0,
            Self::TreeGenerator => 1,
            Self::FolderContentExport => 2,
            Self::Options => 4,
            Self::FileRename => 5,
        }
    }

    /// Falls back to the first tab for indexes that do not exist.
    fn from_index(index: i32) -> Self {
        Self::ALL
            .into_iter()
            .find(|tab| tab.index() == index)
            .unwrap_or(Self::FileContentExport)
    }

    fn icon(self) -> Icon {
        match self {
            Self::FileContentExport => icons::FILE_CONTENT_EXPORT,
            Self::TreeGenerator => icons::TREE_GENERATOR,
            Self::FolderContentExport => icons::FOLDER_CONTENT_EXPORT,
            Self::FileRename => icons::FILE_RENAME,
            Self::Options => icons::OPTIONS,
        }
    }

    /// The smallest content size at which the tab's panels do not overlap. Smaller windows scroll
    /// the tab content instead.
    fn min_content_size(self) -> egui::Vec2 {
        match self {
            Self::FileContentExport => vec2(640.0, 470.0),
            Self::TreeGenerator => vec2(640.0, 380.0),
            Self::FolderContentExport => vec2(640.0, 300.0),
            Self::FileRename => vec2(640.0, 300.0),
            Self::Options => vec2(640.0, 420.0),
        }
    }
}

enum DialogPurpose {
    Message,
    Confirm(ConfirmAction),
    ConfirmReset,
    EditIgnoredFolders,
    ConfirmExit,
    Preset(PresetDialog),
}

struct ActiveDialog {
    dialog: Dialog,
    purpose: DialogPurpose,
}

impl ActiveDialog {
    fn message(dialog: Dialog) -> Self {
        Self {
            dialog,
            purpose: DialogPurpose::Message,
        }
    }
}

pub struct FileCraftApp {
    store: Option<SettingsStore>,
    /// The last applied saved state. It keeps the saved values that no tab edits, so they are
    /// written back unchanged.
    base: SaveData,
    shared: SharedState,
    selected_tab: Tab,
    file_content: FileContentExportTab,
    tree_generator: TreeGeneratorTab,
    folder_content: FolderContentExportTab,
    file_rename: FileRenameTab,
    options: OptionsTab,
    history: History<SaveData>,
    preset_store: Option<PresetStore>,
    /// The stored presets as last read or written.
    presets: Presets,
    /// Sort criteria and direction of the path preset list.
    path_sort: (PresetSortCriteria, bool),
    path_presets: Option<PathPresetsWindow>,
    details: Option<PresetDetailsWindow>,
    load_summary: Option<LoadSummaryWindow>,
    dialog: Option<ActiveDialog>,
    toasts: Toasts,
    preferences_store: Option<PreferencesStore>,
    preferences: Preferences,
    /// Set once the user decided how to handle unsaved changes, so the window may close.
    allow_close: bool,
}

impl FileCraftApp {
    /// `settings_dir` replaces the platform settings folder, for isolated test sessions.
    pub fn new(cc: &eframe::CreationContext<'_>, settings_dir: Option<PathBuf>) -> Self {
        let mut app = Self {
            store: None,
            base: SaveData::default(),
            shared: SharedState::default(),
            selected_tab: Tab::FileContentExport,
            file_content: FileContentExportTab::default(),
            tree_generator: TreeGeneratorTab::default(),
            folder_content: FolderContentExportTab::default(),
            file_rename: FileRenameTab::default(),
            options: OptionsTab::default(),
            history: History::new(SaveData::default()),
            preset_store: None,
            presets: Presets::default(),
            path_sort: (PresetSortCriteria::DateModified, true),
            path_presets: None,
            details: None,
            load_summary: None,
            dialog: None,
            toasts: Toasts::default(),
            preferences_store: None,
            preferences: Preferences::default(),
            allow_close: false,
        };
        let (data, settings_error) = app.load_settings(settings_dir);
        let preferences_error = app.load_preferences();
        let theme = app.preferences.theme.map(|theme| match theme {
            ColorTheme::Light => Theme::Light,
            ColorTheme::Dark => Theme::Dark,
        });
        theme::install(&cc.egui_ctx, theme);
        let presets_error = app.load_presets();
        let error = settings_error.or(presets_error).or(preferences_error);
        app.read_sort(&data);
        app.apply(&data);
        app.history = History::new(app.snapshot());
        if let Some(message) = error {
            app.dialog = Some(ActiveDialog::message(Dialog::notification(
                strings::COMMON_ERROR_TITLE,
                message,
                icons::ERROR,
            )));
        }
        app
    }

    /// Opens the settings store and loads the saved state.
    fn load_settings(&mut self, settings_dir: Option<PathBuf>) -> (SaveData, Option<String>) {
        let store = match settings_dir.map_or_else(SettingsStore::from_platform, |dir| {
            Ok(SettingsStore::new(dir))
        }) {
            Ok(store) => store,
            Err(error) => {
                return (
                    SaveData::default(),
                    Some(format!("{}\n{error}", strings::SETTINGS_FOLDER_UNAVAILABLE)),
                );
            }
        };
        // The settings folder exists from the first start on.
        if let Err(error) = std::fs::create_dir_all(store.directory()) {
            eprintln!(
                "FileCraft: the settings folder {} could not be created: {error}",
                store.directory().display()
            );
        }
        let result = match store.load() {
            Ok((data, _source)) => (data, None),
            Err(error) => (
                SaveData::default(),
                Some(strings::settings_load_error(&error.to_string())),
            ),
        };
        self.store = Some(store);
        result
    }

    /// Reads the preferences, such as the theme. Returns an error message when they cannot be read.
    fn load_preferences(&mut self) -> Option<String> {
        let store = PreferencesStore::new(self.store.as_ref()?.directory());
        let error = match store.load() {
            Ok(preferences) => {
                self.preferences = preferences;
                None
            }
            Err(error) => Some(strings::preferences_load_error(&error.to_string())),
        };
        self.preferences_store = Some(store);
        error
    }

    /// Switches between the light and the dark theme and stores the choice right away.
    fn toggle_theme(&mut self, ctx: &egui::Context) {
        let (theme, choice) = if ctx.theme() == Theme::Dark {
            (Theme::Light, ColorTheme::Light)
        } else {
            (Theme::Dark, ColorTheme::Dark)
        };
        ctx.set_theme(theme);
        theme::sync(ctx);
        self.preferences.theme = Some(choice);
        let Some(store) = &self.preferences_store else {
            return;
        };
        if let Err(error) = store.save(&self.preferences) {
            self.show_error(strings::preferences_save_error(&error.to_string()));
        }
    }

    fn show_error(&mut self, message: String) {
        self.dialog = Some(ActiveDialog::message(Dialog::notification(
            strings::COMMON_ERROR_TITLE,
            message,
            icons::ERROR,
        )));
    }

    /// Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z) and Ctrl+S. Undo and redo are left to a focused text box,
    /// and no shortcut runs while a dialog or window is open.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let blocked = self.dialog.is_some()
            || self.path_presets.is_some()
            || self.details.is_some()
            || self.load_summary.is_some()
            || ctx.memory(|memory| memory.top_modal_layer()).is_some();
        if blocked {
            return;
        }
        let shortcut = |modifiers: Modifiers, key: Key| {
            ctx.input_mut(|input| input.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        if shortcut(Modifiers::COMMAND, Key::S) {
            if self.history.has_unsaved_changes() {
                self.save_with_feedback();
            }
            return;
        }
        if ctx.text_edit_focused() {
            return;
        }
        if shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || shortcut(Modifiers::COMMAND, Key::Y)
        {
            self.redo();
        } else if shortcut(Modifiers::COMMAND, Key::Z) {
            self.undo();
        }
    }

    /// The complete current state.
    fn snapshot(&self) -> SaveData {
        let mut data = self.base.clone();
        data.source_path = self.shared.source_path.clone();
        data.destination_path = self.shared.destination_path.clone();
        data.selected_tab_index = self.selected_tab.index();
        data.file_content_export = self.file_content.settings(
            self.shared.folder_states(TreeTab::FileContentExport),
            &self.base.file_content_export,
        );
        data.folder_content_export = self.folder_content.settings(
            self.shared.folder_states(TreeTab::FolderContentExport),
            &self.base.folder_content_export,
        );
        data.tree_generator = self.tree_generator.settings(
            self.shared.folder_states(TreeTab::TreeGenerator),
            &self.base.tree_generator,
        );
        data.settings_page.ignored_folders = self.shared.ignored_folders().to_vec();
        data.settings_page.linked_folder_tree_groups = self.shared.link_group_ids();
        self.write_sort(&mut data);
        data
    }

    /// Applies a saved state.
    fn apply(&mut self, data: &SaveData) {
        self.base = data.clone();
        self.shared.apply(
            &data.source_path,
            &data.destination_path,
            &data.settings_page.ignored_folders,
            &data.settings_page.linked_folder_tree_groups,
            [
                &data.file_content_export.folder_tree_state,
                &data.tree_generator.folder_tree_state,
                &data.folder_content_export.folder_tree_state,
            ],
        );
        self.file_content.apply_settings(&data.file_content_export);
        self.tree_generator.apply_settings(&data.tree_generator);
        self.folder_content
            .apply_settings(&data.folder_content_export);
        self.selected_tab = Tab::from_index(data.selected_tab_index);
    }

    fn save(&mut self) -> Result<(), String> {
        let Some(store) = &self.store else {
            return Err(strings::SETTINGS_FOLDER_UNAVAILABLE.to_owned());
        };
        store
            .save(&self.snapshot())
            .map_err(|error| error.to_string())?;
        self.history.mark_saved();
        Ok(())
    }

    fn save_with_feedback(&mut self) {
        match self.save() {
            Ok(()) => self
                .toasts
                .push(Toast::success(strings::TOAST_SETTINGS_SAVED, String::new())),
            Err(message) => self.show_error(strings::settings_save_error(&message)),
        }
    }

    fn undo(&mut self) {
        if let Some(previous) = self.history.undo(self.snapshot()) {
            self.apply(&previous);
            self.history.set_current(self.snapshot());
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.history.redo(self.snapshot()) {
            self.apply(&next);
            self.history.set_current(self.snapshot());
        }
    }

    /// Records user changes made during this frame as undo steps. Switching tabs alone is not a change.
    fn observe_changes(&mut self, ctx: &egui::Context) {
        let had_input = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::PointerButton { .. }
                        | egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Cut
                )
            })
        });
        if had_input {
            self.history
                .observe(self.snapshot(), same_ignoring_view_state);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        let mut toggle_theme = false;
        let mut action = None;
        let buttons = [
            (
                icons::UNDO,
                strings::UNDO_TOOLTIP,
                self.history.can_undo(),
                ToolbarAction::Undo,
            ),
            (
                icons::REDO,
                strings::REDO_TOOLTIP,
                self.history.can_redo(),
                ToolbarAction::Redo,
            ),
            (
                icons::SAVE,
                strings::SAVE_SETTINGS_TOOLTIP,
                self.history.has_unsaved_changes(),
                ToolbarAction::Save,
            ),
        ];
        ui.horizontal(|ui| {
            ui.add_space(5.0);
            for (icon, tooltip, enabled, kind) in buttons {
                ui.add_space(2.0);
                if widgets::icon_button(ui, icon, 28.0, 28.0, enabled)
                    .on_hover_text(tooltip)
                    .clicked()
                    && enabled
                {
                    action = Some(kind);
                }
                ui.add_space(2.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(5.0);
                let (icon, tooltip) = if ui.visuals().dark_mode {
                    (icons::LIGHT_MODE, strings::THEME_LIGHT_TOOLTIP)
                } else {
                    (icons::DARK_MODE, strings::THEME_DARK_TOOLTIP)
                };
                toggle_theme = widgets::icon_button(ui, icon, 32.0, 22.0, true)
                    .on_hover_text(tooltip)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked();
            });
        });
        ui.add_space(5.0);
        if toggle_theme {
            self.toggle_theme(ui.ctx());
        }
        match action {
            Some(ToolbarAction::Undo) => self.undo(),
            Some(ToolbarAction::Redo) => self.redo(),
            Some(ToolbarAction::Save) => self.save_with_feedback(),
            None => {}
        }
    }

    fn paths_row(&mut self, ui: &mut Ui) {
        let available = ui.available_rect_before_wrap();
        let row = Rect::from_min_max(
            pos2(available.left() + 5.0, available.top()),
            pos2(
                available.right() - 5.0,
                available.top() + widgets::floating_box_height(ui),
            ),
        );
        let middle = row.center().x;
        let source_rect = Rect::from_min_max(row.min, pos2(middle - 5.0, row.bottom()));
        let destination_rect = Rect::from_min_max(pos2(middle + 5.0, row.top()), row.max);
        // Bottom aligned, so the buttons line up with the boxes under their labels.
        let layout = Layout::right_to_left(Align::Max);

        let mut source = ui.new_child(UiBuilder::new().max_rect(source_rect).layout(layout));
        let select_source = widgets::button(
            &mut source,
            strings::BROWSE_BUTTON,
            ButtonKind::Secondary,
            true,
            egui::Vec2::ZERO,
        )
        .clicked();
        source.add_space(10.0);
        let width = source.available_width();
        widgets::floating_path_box(
            &mut source,
            &self.shared.source_path,
            width,
            strings::SOURCE_FOLDER_LABEL,
        );

        let can_clear = self.shared.can_clear_paths();
        let mut destination =
            ui.new_child(UiBuilder::new().max_rect(destination_rect).layout(layout));
        let clear = widgets::button(
            &mut destination,
            strings::CLEAR_BUTTON,
            ButtonKind::Secondary,
            can_clear,
            egui::Vec2::ZERO,
        )
        .on_hover_text(strings::CLEAR_PATHS_TOOLTIP)
        .clicked()
            && can_clear;
        destination.add_space(10.0);
        let select_destination = widgets::button(
            &mut destination,
            strings::BROWSE_BUTTON,
            ButtonKind::Secondary,
            true,
            egui::Vec2::ZERO,
        )
        .clicked();
        destination.add_space(10.0);
        let width = destination.available_width();
        widgets::floating_path_box(
            &mut destination,
            &self.shared.destination_path,
            width,
            strings::DESTINATION_FOLDER_LABEL,
        );
        ui.advance_cursor_after_rect(Rect::from_min_max(
            available.min,
            pos2(row.right(), row.bottom() + 5.0),
        ));

        if select_source && let Some(path) = pick_folder(strings::SELECT_SOURCE_FOLDER) {
            self.shared.set_source_path(path);
        }
        if select_destination && let Some(path) = pick_folder(strings::SELECT_DESTINATION_FOLDER) {
            self.shared.destination_path = path;
        }
        if clear {
            self.shared.clear_paths();
        }
    }

    fn tab_control(&mut self, ui: &mut Ui) {
        let area = ui.available_rect_before_wrap();
        let colors = theme::palette();
        let strip = Rect::from_min_size(area.min, vec2(area.width(), TAB_HEIGHT));
        let content = Rect::from_min_max(pos2(area.left(), strip.bottom()), area.max);
        // Painted once the selected tab of this frame is known.
        let background = ui.painter().add(Shape::Noop);
        ui.painter().line_segment(
            [
                pos2(area.left(), strip.bottom() - 0.5),
                pos2(area.right(), strip.bottom() - 0.5),
            ],
            Stroke::new(1.0, colors.border_subtle),
        );
        let mut x = strip.left() + 5.0;
        for tab in Tab::ALL {
            // Measured in the bold font, so a tab keeps its width when it is selected.
            let width = TAB_PADDING * 2.0
                + TAB_ICON_SIZE
                + TAB_ICON_GAP
                + widgets::text_width(ui, tab.title(), theme::semibold(theme::BODY_SIZE));
            let rect = Rect::from_min_size(pos2(x, strip.top()), vec2(width, TAB_HEIGHT));
            x += width;
            let response = ui
                .interact(
                    rect,
                    ui.scope_id().with(("tab", tab.title())),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                self.selected_tab = tab;
            }
            let selected = tab == self.selected_tab;
            let hover = widgets::hover_amount(ui, &response, !selected);
            paint_tab(ui, tab, rect, selected, hover);
        }
        // Options keeps its group boxes on the window background, the other tabs show cards.
        if self.selected_tab != Tab::Options {
            ui.painter().set(
                background,
                Shape::rect_filled(content, 0, colors.section_background),
            );
        }
        let content_clip = content;

        // The tab content keeps its minimum size and scrolls when the window is smaller, so the
        // panels never overlap.
        let inner = content_clip.shrink(5.0);
        let size = inner.size().max(self.selected_tab.min_content_size());
        let mut content_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        content_ui.set_clip_rect(content_clip.intersect(ui.clip_rect()));
        let mut scroll_source = egui::scroll_area::ScrollSource::ALL;
        scroll_source.drag = egui::scroll_area::ScrollSource::NONE.drag;
        let tab = self.selected_tab;
        egui::ScrollArea::both()
            .id_salt(("tab_content", tab.index()))
            .auto_shrink([false, false])
            .scroll_source(scroll_source)
            .show(&mut content_ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let mut tab_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                self.tab_ui(&mut tab_ui, tab);
            });
        ui.advance_cursor_after_rect(area);
    }

    fn tab_ui(&mut self, ui: &mut Ui, tab: Tab) {
        match tab {
            Tab::FileContentExport => {
                if let Some(request) = self.file_content.ui(ui, &mut self.shared) {
                    self.handle_tab_request(request);
                }
            }
            Tab::TreeGenerator => {
                if let Some(request) = self.tree_generator.ui(ui, &mut self.shared) {
                    self.handle_tab_request(request);
                }
            }
            Tab::FolderContentExport => {
                if let Some(request) = self.folder_content.ui(ui, &mut self.shared) {
                    self.handle_tab_request(request);
                }
            }
            Tab::FileRename => {
                if let Some(request) = self.file_rename.ui(ui) {
                    self.handle_tab_request(request);
                }
            }
            Tab::Options => {
                let presets = self.save_preset_items();
                if let Some(request) = self.options.ui(ui, &self.shared, &presets) {
                    self.handle_options_request(request);
                }
            }
        }
    }

    fn handle_tab_request(&mut self, request: TabRequest) {
        match request {
            TabRequest::ShowDialog(dialog) => {
                self.dialog = Some(ActiveDialog::message(dialog));
            }
            TabRequest::Confirm(dialog, action) => {
                self.dialog = Some(ActiveDialog {
                    dialog,
                    purpose: DialogPurpose::Confirm(action),
                });
            }
            TabRequest::Notify(toast) => self.toasts.push(toast),
            TabRequest::OpenPathPresets => self.open_path_presets(),
            TabRequest::ChooseSourceFolder => {
                if let Some(path) = pick_folder(strings::SELECT_SOURCE_FOLDER) {
                    self.shared.set_source_path(path);
                }
            }
            TabRequest::SettingsResolved => self.history.set_current(self.snapshot()),
        }
    }

    fn handle_options_request(&mut self, request: OptionsRequest) {
        match request {
            OptionsRequest::OpenSaveFolder => {
                if let Some(message) = self.open_save_folder() {
                    self.dialog = Some(ActiveDialog::message(Dialog::notification(
                        strings::COMMON_ERROR_TITLE,
                        message,
                        icons::ERROR,
                    )));
                }
            }
            OptionsRequest::DeleteCurrentSave => {
                self.dialog = Some(ActiveDialog {
                    dialog: Dialog::confirmation(
                        strings::RESET_CONFIRM_TITLE,
                        strings::RESET_CONFIRM_MESSAGE.to_owned(),
                        icons::WARNING,
                    ),
                    purpose: DialogPurpose::ConfirmReset,
                });
            }
            OptionsRequest::EditIgnoredFolders => {
                self.dialog = Some(ActiveDialog {
                    dialog: Dialog::text_input(
                        strings::EDIT_IGNORED_TITLE,
                        strings::EDIT_IGNORED_INSTRUCTION.to_owned(),
                        format_ignored_folders(self.shared.ignored_folders()),
                    ),
                    purpose: DialogPurpose::EditIgnoredFolders,
                });
            }
            OptionsRequest::Preset(action) => self.handle_preset_action(action),
            OptionsRequest::CopyTree(source, destination) => {
                self.confirm_copy_tree(source, destination);
            }
            OptionsRequest::Link(first, second) => {
                self.shared.link(first, second);
                self.options.clear_tab_selection();
            }
            OptionsRequest::Unlink(tab) => self.shared.unlink(tab),
        }
    }

    /// Returns an error message when the folder cannot be opened.
    fn open_save_folder(&self) -> Option<String> {
        let Some(store) = &self.store else {
            return Some(strings::SETTINGS_FOLDER_UNAVAILABLE.to_owned());
        };
        if !store.directory().is_dir() {
            return Some(strings::OPEN_SAVE_ERROR_NOT_FOUND.to_owned());
        }
        platform::open_folder(store.directory())
            .err()
            .map(|error| strings::open_save_error(&error.to_string()))
    }

    /// Deletes the save file and resets to defaults.
    fn reset(&mut self) {
        let before = self.snapshot();
        let result = match &self.store {
            Some(store) => store.delete().map_err(|error| error.to_string()),
            None => Err(strings::SETTINGS_FOLDER_UNAVAILABLE.to_owned()),
        };
        match result {
            Ok(()) => {
                self.apply(&SaveData::default());
                self.history.record_change(before, self.snapshot());
                self.history.mark_saved();
                self.toasts.push(Toast::success(
                    strings::TOAST_SETTINGS_RESET,
                    strings::TOAST_SETTINGS_RESET_MESSAGE.to_owned(),
                ));
            }
            Err(message) => self.show_error(strings::reset_error(&message)),
        }
    }

    /// Asks about unsaved changes before the window closes.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let requested = ctx.input(|input| input.viewport().close_requested());
        if !requested || self.allow_close || !self.history.has_unsaved_changes() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if self.dialog.is_none() {
            self.dialog = Some(ActiveDialog {
                dialog: Dialog::exit_confirmation(
                    strings::UNSAVED_CHANGES_TITLE,
                    strings::UNSAVED_CHANGES_MESSAGE.to_owned(),
                    icons::WARNING,
                ),
                purpose: DialogPurpose::ConfirmExit,
            });
        }
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(active) = &mut self.dialog else {
            return;
        };
        let Some(result) = active.dialog.show(ctx) else {
            return;
        };
        let Some(active) = self.dialog.take() else {
            return;
        };
        match (active.purpose, result) {
            (DialogPurpose::Confirm(ConfirmAction::GenerateTree), DialogResult::Primary) => {
                self.tree_generator.start_generation(&self.shared, ctx);
            }
            (DialogPurpose::Confirm(ConfirmAction::ExportFileContents), DialogResult::Primary) => {
                self.file_content.start_export(&self.shared, ctx);
            }
            (
                DialogPurpose::Confirm(ConfirmAction::ExportFolderContents),
                DialogResult::Primary,
            ) => {
                self.folder_content.start_export(&self.shared, ctx);
            }
            (DialogPurpose::Confirm(ConfirmAction::RenameFiles), DialogResult::Primary) => {
                self.file_rename.start(ctx);
            }
            (DialogPurpose::ConfirmReset, DialogResult::Primary) => self.reset(),
            (DialogPurpose::EditIgnoredFolders, DialogResult::Primary) => {
                let text = active.dialog.input.unwrap_or_default();
                self.shared
                    .set_ignored_folders(parse_ignored_folders(&text));
            }
            (DialogPurpose::ConfirmExit, DialogResult::Primary) => match self.save() {
                Ok(()) => self.close(ctx),
                Err(message) => {
                    self.dialog = Some(ActiveDialog::message(Dialog::notification(
                        strings::COMMON_ERROR_TITLE,
                        strings::settings_save_error(&message),
                        icons::ERROR,
                    )));
                }
            },
            (DialogPurpose::ConfirmExit, DialogResult::Tertiary) => self.close(ctx),
            (DialogPurpose::Preset(purpose), result) => {
                self.handle_preset_dialog(purpose, result, active.dialog.input);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ToolbarAction {
    Undo,
    Redo,
    Save,
}

/// Compares states without the selected tab and the preset list sorting, which are not recorded as
/// changes.
fn same_ignoring_view_state(a: &SaveData, b: &SaveData) -> bool {
    let view_state = |data: &SaveData| {
        (
            data.selected_tab_index,
            data.save_preset_sort_by,
            data.save_preset_is_descending,
            data.path_preset_sort_by,
            data.path_preset_is_descending,
        )
    };
    if view_state(a) == view_state(b) {
        return a == b;
    }
    let mut b = b.clone();
    b.selected_tab_index = a.selected_tab_index;
    b.save_preset_sort_by = a.save_preset_sort_by;
    b.save_preset_is_descending = a.save_preset_is_descending;
    b.path_preset_sort_by = a.path_preset_sort_by;
    b.path_preset_is_descending = a.path_preset_is_descending;
    *a == b
}

impl eframe::App for FileCraftApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::sync(&ctx);
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::palette().background)
                    .inner_margin(10),
            )
            .show(ui, |ui| {
                self.toolbar(ui);
                self.paths_row(ui);
                self.tab_control(ui);
            });
        if let Some(message) = self.toasts.show(&ctx)
            && self.dialog.is_none()
        {
            self.show_error(message);
        }
        self.preset_windows(&ctx);
        self.dialogs(&ctx);
        self.observe_changes(&ctx);
    }
}

const TAB_HEIGHT: f32 = 38.0;
const TAB_PADDING: f32 = 12.0;
const TAB_ICON_SIZE: f32 = 20.0;
const TAB_ICON_GAP: f32 = 7.0;

/// Paints a tab: its icon and title, with an accent underline when it is selected. Unselected
/// tabs darken their title and show a gray underline on hover.
fn paint_tab(ui: &Ui, tab: Tab, rect: Rect, selected: bool, hover: f32) {
    let colors = theme::palette();
    let (font, text_color, underline) = if selected {
        (
            theme::semibold(theme::BODY_SIZE),
            colors.text,
            colors.accent,
        )
    } else {
        (
            theme::body(theme::BODY_SIZE),
            widgets::lerp_color(colors.text_muted, colors.text, hover),
            widgets::lerp_color(egui::Color32::TRANSPARENT, colors.border, hover),
        )
    };
    if underline.a() > 0 {
        let line = Rect::from_min_max(
            pos2(rect.left() + 4.0, rect.bottom() - 2.0),
            pos2(rect.right() - 4.0, rect.bottom()),
        );
        ui.painter().rect_filled(line, 1, underline);
    }
    let center_y = rect.center().y - 1.0;
    let icon = tab.icon();
    widgets::paint_glyph(
        ui,
        pos2(rect.left() + TAB_PADDING + TAB_ICON_SIZE / 2.0, center_y),
        icon.glyph,
        TAB_ICON_SIZE,
        icon.color(),
    );
    ui.painter().text(
        pos2(
            rect.left() + TAB_PADDING + TAB_ICON_SIZE + TAB_ICON_GAP,
            center_y,
        ),
        Align2::LEFT_CENTER,
        tab.title(),
        font,
        text_color,
    );
}

fn pick_folder(title: &str) -> Option<String> {
    rfd::FileDialog::new()
        .set_title(title)
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}
