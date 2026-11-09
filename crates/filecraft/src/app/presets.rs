//! Setting presets, path presets and the folder tree tools of the Options tab.

use filecraft_core::path_convert::{convert_save_data_paths, relative_path};
use filecraft_core::presets::{PresetSortCriteria, PresetStore, Presets};
use filecraft_core::settings_store::LoadSource;

use super::{ActiveDialog, DialogPurpose, FileCraftApp};
use crate::dialog::{Dialog, DialogResult, TabSummary};
use crate::icons;
use crate::preset_list::{PresetList, PresetListAction, PresetListItem};
use crate::preset_windows::{
    LoadSummaryWindow, PathPresetsEvent, PathPresetsWindow, PresetDetailsWindow,
};
use crate::state::TreeTab;
use crate::strings;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What a preset dialog was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PresetDialog {
    SaveName,
    SaveNameTaken(String),
    Load(i32),
    Overwrite(i32),
    Delete(i32),
    Rename(i32),
    PathName,
    PathNameTaken(String),
    PathLoad(i32),
    PathOverwrite(i32),
    PathDelete(i32),
    PathRename(i32),
    CopyTree(TreeTab, TreeTab),
}

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

impl FileCraftApp {
    /// Reads the stored presets. Returns an error message when they cannot be read. The file is
    /// then left as it is, and changing presets reports the error again.
    pub(super) fn load_presets(&mut self) -> Option<String> {
        let store = self.store.as_ref()?;
        let preset_store = PresetStore::new(store.directory());
        let result = match preset_store.load() {
            Ok(presets) => {
                self.presets = presets;
                None
            }
            Err(error) => Some(strings::presets_load_error(&error.to_string())),
        };
        self.preset_store = Some(preset_store);
        result
    }

    /// Reads the stored presets, applies `change` and writes them back. The stored file is read
    /// again first, so a file that cannot be read is never overwritten.
    fn update_presets<R>(&mut self, change: impl FnOnce(&mut Presets) -> R) -> Result<R, String> {
        let Some(store) = &self.preset_store else {
            return Err(strings::SETTINGS_FOLDER_UNAVAILABLE.to_owned());
        };
        let mut presets = store.load().map_err(|error| error.to_string())?;
        let result = change(&mut presets);
        store.save(&presets).map_err(|error| error.to_string())?;
        self.presets = presets;
        Ok(result)
    }

    pub(super) fn save_preset_items(&self) -> Vec<PresetListItem> {
        self.presets
            .save_presets
            .iter()
            .map(|preset| PresetListItem {
                id: preset.id,
                name: preset.name.clone(),
                last_modified: preset.last_modified,
                description: preset.description.clone(),
            })
            .collect()
    }

    fn path_preset_items(&self) -> Vec<PresetListItem> {
        self.presets
            .path_presets
            .iter()
            .map(|preset| PresetListItem {
                id: preset.id,
                name: preset.name.clone(),
                last_modified: preset.last_modified,
                description: strings::path_preset_description(preset.file_paths.len()),
            })
            .collect()
    }

    fn source_is_valid(&self) -> bool {
        let source = self.shared.source_path.trim();
        !source.is_empty() && std::path::Path::new(source).is_dir()
    }

    fn show(&mut self, dialog: Dialog) {
        self.dialog = Some(ActiveDialog::message(dialog));
    }

    fn ask(&mut self, dialog: Dialog, purpose: PresetDialog) {
        self.dialog = Some(ActiveDialog {
            dialog,
            purpose: DialogPurpose::Preset(purpose),
        });
    }

    fn warning(&mut self, message: &str) {
        self.show(Dialog::notification(
            strings::COMMON_WARNING_TITLE,
            message.to_owned(),
            icons::WARNING,
        ));
    }

    fn success(&mut self, title: &str, message: String) {
        self.toasts
            .push(crate::toast::Toast::success(title, message));
    }

    fn error(&mut self, message: String) {
        self.show(Dialog::notification(
            strings::COMMON_ERROR_TITLE,
            message,
            icons::ERROR,
        ));
    }

    fn ask_save_name(&mut self, name: String) {
        self.ask(
            Dialog::line_input(
                strings::PATH_PRESET_NEW_PRESET_TITLE,
                strings::PATH_PRESET_NAME_PROMPT.to_owned(),
                name,
            ),
            PresetDialog::SaveName,
        );
    }

    /// The current state with paths relative to the source folder, named for a preset.
    fn preset_data(&self, name: &str) -> filecraft_core::settings::SaveData {
        let mut data = self.snapshot();
        data.preset_name = name.to_owned();
        convert_save_data_paths(&data, &self.shared.source_path, true)
    }

    pub(super) fn handle_preset_action(&mut self, action: PresetListAction) {
        match action {
            PresetListAction::SaveNew => {
                if !self.source_is_valid() {
                    self.warning(strings::PRESET_SELECT_SOURCE_FIRST);
                    return;
                }
                self.ask_save_name(String::new());
            }
            PresetListAction::Load(id) => {
                if !self.source_is_valid() {
                    self.warning(strings::PRESET_LOAD_SELECT_SOURCE_FIRST);
                    return;
                }
                let Some(preset) = self.presets.save_preset(id) else {
                    return;
                };
                let dialog = Dialog::confirmation(
                    strings::PRESET_LOAD_CONFIRM_TITLE,
                    strings::format1(strings::PRESET_LOAD_CONFIRM_MESSAGE, &preset.name),
                    icons::INFO,
                );
                self.ask(dialog, PresetDialog::Load(id));
            }
            PresetListAction::Overwrite(id) => {
                if !self.source_is_valid() {
                    self.warning(strings::PRESET_SELECT_SOURCE_FIRST);
                    return;
                }
                let Some(preset) = self.presets.save_preset(id) else {
                    return;
                };
                let dialog = Dialog::confirmation(
                    strings::PRESET_OVERWRITE_CURRENT_CONFIRM_TITLE,
                    strings::format1(
                        strings::PRESET_OVERWRITE_CURRENT_CONFIRM_MESSAGE,
                        &preset.name,
                    ),
                    icons::WARNING,
                );
                self.ask(dialog, PresetDialog::Overwrite(id));
            }
            PresetListAction::Details(id) => {
                if let Some(preset) = self.presets.save_preset(id) {
                    self.details = Some(PresetDetailsWindow::for_save_preset(preset));
                }
            }
            PresetListAction::Rename(id) => {
                let Some(preset) = self.presets.save_preset(id) else {
                    return;
                };
                let dialog = Dialog::line_input(
                    strings::RENAME_PRESET_TITLE,
                    strings::format1(strings::RENAME_PRESET_PROMPT, &preset.name),
                    preset.name.clone(),
                );
                self.ask(dialog, PresetDialog::Rename(id));
            }
            PresetListAction::Delete(id) => {
                let Some(preset) = self.presets.save_preset(id) else {
                    return;
                };
                let dialog = Dialog::confirmation(
                    strings::PRESET_DELETE_CONFIRM_TITLE,
                    strings::format1(strings::PRESET_DELETE_CONFIRM_MESSAGE, &preset.name),
                    icons::WARNING,
                );
                self.ask(dialog, PresetDialog::Delete(id));
            }
            PresetListAction::SortChanged => self.persist_sort(),
        }
    }

    fn handle_path_preset_action(&mut self, action: PresetListAction) {
        match action {
            PresetListAction::SaveNew => {
                if self.check_path_selection() {
                    self.ask(
                        Dialog::line_input(
                            strings::PATH_PRESET_NEW_PRESET_TITLE,
                            strings::PATH_PRESET_NAME_PROMPT.to_owned(),
                            String::new(),
                        ),
                        PresetDialog::PathName,
                    );
                }
            }
            PresetListAction::Load(id) => {
                let Some(preset) = self.presets.path_preset(id) else {
                    return;
                };
                let dialog = Dialog::confirmation(
                    strings::PATH_PRESET_LOAD_CONFIRM_TITLE,
                    strings::format1(strings::PATH_PRESET_LOAD_CONFIRM_MESSAGE, &preset.name),
                    icons::INFO,
                );
                self.ask(dialog, PresetDialog::PathLoad(id));
            }
            PresetListAction::Overwrite(id) => {
                let Some(name) = self
                    .presets
                    .path_preset(id)
                    .map(|preset| preset.name.clone())
                else {
                    return;
                };
                if self.check_path_selection() {
                    let dialog = Dialog::confirmation(
                        strings::PRESET_OVERWRITE_CURRENT_CONFIRM_TITLE,
                        strings::format1(strings::PRESET_OVERWRITE_CURRENT_CONFIRM_MESSAGE, &name),
                        icons::WARNING,
                    );
                    self.ask(dialog, PresetDialog::PathOverwrite(id));
                }
            }
            PresetListAction::Details(id) => {
                if let Some(preset) = self.presets.path_preset(id) {
                    self.details = Some(PresetDetailsWindow::for_path_preset(preset));
                }
            }
            PresetListAction::Rename(id) => {
                let Some(preset) = self.presets.path_preset(id) else {
                    return;
                };
                let dialog = Dialog::line_input(
                    strings::PATH_PRESET_RENAME_TITLE,
                    strings::format1(strings::PATH_PRESET_RENAME_PROMPT, &preset.name),
                    preset.name.clone(),
                );
                self.ask(dialog, PresetDialog::PathRename(id));
            }
            PresetListAction::Delete(id) => {
                let Some(preset) = self.presets.path_preset(id) else {
                    return;
                };
                let dialog = Dialog::confirmation(
                    strings::PATH_PRESET_DELETE_CONFIRM_TITLE,
                    strings::format1(strings::PATH_PRESET_DELETE_CONFIRM_MESSAGE, &preset.name),
                    icons::WARNING,
                );
                self.ask(dialog, PresetDialog::PathDelete(id));
            }
            PresetListAction::SortChanged => {
                if let Some(window) = &self.path_presets {
                    self.path_sort = (window.list.sort_by, window.list.descending);
                }
                self.persist_sort();
            }
        }
    }

    /// A valid source folder and selected files are needed to store a path preset.
    fn check_path_selection(&mut self) -> bool {
        if !self.source_is_valid() {
            self.warning(strings::PRESET_SELECT_SOURCE_FIRST);
            return false;
        }
        if self.file_content.selected_file_paths().is_empty() {
            self.warning(strings::PATH_PRESET_NO_SELECTION_ERROR);
            return false;
        }
        true
    }

    /// Stores the selected files under `name`, relative to the source folder.
    fn save_path_preset(&mut self, name: &str) {
        let source = self.shared.source_path.clone();
        let paths: Vec<String> = self
            .file_content
            .selected_file_paths()
            .iter()
            .map(|path| relative_path(&source, path))
            .collect();
        match self.update_presets(|presets| presets.save_path_preset(name, paths, now())) {
            Ok(()) => self.success(strings::TOAST_PATH_PRESET_SAVED, strings::quoted(name)),
            Err(error) => self.error(strings::format1(strings::PRESET_SAVE_ERROR, &error)),
        }
    }

    pub(super) fn handle_preset_dialog(
        &mut self,
        purpose: PresetDialog,
        result: DialogResult,
        input: Option<String>,
    ) {
        if result != DialogResult::Primary {
            return;
        }
        let input = input.unwrap_or_default();
        match purpose {
            PresetDialog::SaveName => {
                if input.trim().is_empty() {
                    return;
                }
                if self.presets.save_preset_name_exists(&input, None) {
                    let dialog = Dialog::confirmation(
                        strings::COMMON_WARNING_TITLE,
                        strings::PRESET_NAME_EXISTS_USE_DIFFERENT.to_owned(),
                        icons::WARNING,
                    );
                    self.ask(dialog, PresetDialog::SaveNameTaken(input));
                    return;
                }
                let data = self.preset_data(&input);
                match self.update_presets(|presets| {
                    presets.add_save_preset(&input, data, now(), APP_VERSION)
                }) {
                    Ok(_) => self.success(strings::TOAST_PRESET_SAVED, strings::quoted(&input)),
                    Err(error) => self.error(strings::format1(strings::PRESET_SAVE_ERROR, &error)),
                }
            }
            PresetDialog::SaveNameTaken(name) => self.ask_save_name(name),
            PresetDialog::Load(id) => {
                let Some(preset) = self.presets.save_preset(id).cloned() else {
                    return;
                };
                let data = convert_save_data_paths(&preset.data, &self.shared.source_path, false);
                self.apply(&data);
                self.success(strings::TOAST_PRESET_LOADED, strings::quoted(&preset.name));
            }
            PresetDialog::Overwrite(id) => {
                let Some(name) = self
                    .presets
                    .save_preset(id)
                    .map(|preset| preset.name.clone())
                else {
                    return;
                };
                let data = self.preset_data(&name);
                match self.update_presets(|presets| {
                    presets.update_save_preset_data(id, data, now(), APP_VERSION)
                }) {
                    Ok(_) => self.success(strings::TOAST_PRESET_SAVED, strings::quoted(&name)),
                    Err(error) => self.error(strings::format1(strings::PRESET_SAVE_ERROR, &error)),
                }
            }
            PresetDialog::Delete(id) => {
                let Some(name) = self
                    .presets
                    .save_preset(id)
                    .map(|preset| preset.name.clone())
                else {
                    return;
                };
                match self.update_presets(|presets| presets.delete_save_preset(id)) {
                    Ok(()) => self.success(strings::TOAST_PRESET_DELETED, strings::quoted(&name)),
                    Err(error) => {
                        self.error(strings::format1(strings::PRESET_DELETE_ERROR, &error))
                    }
                }
            }
            PresetDialog::Rename(id) => {
                let Some(old) = self
                    .presets
                    .save_preset(id)
                    .map(|preset| preset.name.clone())
                else {
                    return;
                };
                if input.trim().is_empty() || input == old {
                    return;
                }
                if self.presets.save_preset_name_exists(&input, Some(id)) {
                    self.error(strings::PRESET_NAME_EXISTS.to_owned());
                    return;
                }
                if let Err(error) =
                    self.update_presets(|presets| presets.rename_save_preset(id, &input, now()))
                {
                    self.error(strings::format1(strings::PRESET_SAVE_ERROR, &error));
                }
            }
            PresetDialog::PathName => {
                if input.trim().is_empty() {
                    return;
                }
                if self.presets.path_preset_name_exists(&input) {
                    let dialog = Dialog::confirmation(
                        strings::COMMON_WARNING_TITLE,
                        strings::format1(strings::PRESET_OVERWRITE_MESSAGE, &input),
                        icons::WARNING,
                    );
                    self.ask(dialog, PresetDialog::PathNameTaken(input));
                    return;
                }
                self.save_path_preset(&input);
            }
            PresetDialog::PathNameTaken(name) => self.save_path_preset(&name),
            PresetDialog::PathLoad(id) => {
                let Some(paths) = self
                    .presets
                    .path_preset(id)
                    .map(|preset| preset.file_paths.clone())
                else {
                    return;
                };
                let source = self.shared.source_path.clone();
                let result = self.file_content.load_path_preset(&paths, &source);
                self.load_summary = Some(LoadSummaryWindow::new(result));
            }
            PresetDialog::PathOverwrite(id) => {
                if let Some(name) = self
                    .presets
                    .path_preset(id)
                    .map(|preset| preset.name.clone())
                {
                    self.save_path_preset(&name);
                }
            }
            PresetDialog::PathDelete(id) => {
                if let Err(error) = self.update_presets(|presets| presets.delete_path_preset(id)) {
                    self.error(strings::format1(strings::PRESET_DELETE_ERROR, &error));
                }
            }
            PresetDialog::PathRename(id) => {
                let Some(old) = self
                    .presets
                    .path_preset(id)
                    .map(|preset| preset.name.clone())
                else {
                    return;
                };
                if input.trim().is_empty() || input == old {
                    return;
                }
                if let Err(error) =
                    self.update_presets(|presets| presets.rename_path_preset(id, &input, now()))
                {
                    self.error(strings::format1(strings::PRESET_SAVE_ERROR, &error));
                }
            }
            PresetDialog::CopyTree(source, destination) => {
                self.shared.copy_tree(source, destination);
                self.options.clear_tab_selection();
                self.success(
                    strings::TOAST_TREE_COPIED,
                    strings::tree_copied(source.title(), destination.title()),
                );
            }
        }
    }

    /// Asks before copying the folder tree of one tab to another.
    pub(super) fn confirm_copy_tree(&mut self, source: TreeTab, destination: TreeTab) {
        let summary = |tab: TreeTab| TabSummary {
            icon: tab.icon(),
            name: tab.title(),
            folders: self
                .shared
                .tree(tab)
                .map_or(0, filecraft_core::folder_tree::FolderTree::selected_count),
        };
        let dialog = Dialog::copy_tree_confirmation(
            strings::OPTIONS_COPY_TREE_TITLE,
            summary(source),
            summary(destination),
        );
        self.ask(dialog, PresetDialog::CopyTree(source, destination));
    }

    pub(super) fn open_path_presets(&mut self) {
        let mut list = PresetList::new("path_presets");
        (list.sort_by, list.descending) = self.path_sort;
        self.path_presets = Some(PathPresetsWindow::new(list));
    }

    /// Shows the preset windows above the main window. Dialogs are shown above them afterwards.
    pub(super) fn preset_windows(&mut self, ctx: &egui::Context) {
        let topmost =
            self.details.is_none() && self.load_summary.is_none() && self.dialog.is_none();
        let items = self.path_preset_items();
        let event = match &mut self.path_presets {
            Some(window) => window.show(ctx, &items, topmost),
            None => None,
        };
        match event {
            Some(PathPresetsEvent::Close) => self.path_presets = None,
            Some(PathPresetsEvent::Action(action)) if self.dialog.is_none() => {
                self.handle_path_preset_action(action);
            }
            _ => {}
        }
        if let Some(window) = &mut self.details
            && window.show(ctx)
        {
            self.details = None;
        }
        if let Some(window) = &mut self.load_summary
            && window.show(ctx)
        {
            self.load_summary = None;
        }
    }

    /// Writes the sort settings of both preset lists into the save file right away. Without a save
    /// file they are written with the next save.
    fn persist_sort(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        match store.load() {
            Ok((mut data, LoadSource::SaveFile)) => {
                self.write_sort(&mut data);
                if let Err(error) = store.save(&data) {
                    eprintln!("FileCraft: the preset sort order could not be saved: {error}");
                }
            }
            Ok(_) => {}
            Err(error) => eprintln!("FileCraft: the preset sort order could not be saved: {error}"),
        }
    }

    /// Puts the sort settings of both preset lists into `data`.
    pub(super) fn write_sort(&self, data: &mut filecraft_core::settings::SaveData) {
        data.save_preset_sort_by = self.options.presets.sort_by.number();
        data.save_preset_is_descending = self.options.presets.descending;
        data.path_preset_sort_by = self.path_sort.0.number();
        data.path_preset_is_descending = self.path_sort.1;
    }

    /// Takes the sort settings of both preset lists from a loaded save file.
    pub(super) fn read_sort(&mut self, data: &filecraft_core::settings::SaveData) {
        self.options.presets.sort_by = PresetSortCriteria::from_number(data.save_preset_sort_by);
        self.options.presets.descending = data.save_preset_is_descending;
        self.path_sort = (
            PresetSortCriteria::from_number(data.path_preset_sort_by),
            data.path_preset_is_descending,
        );
    }
}
