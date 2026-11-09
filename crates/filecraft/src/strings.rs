//! User-facing text.

use crate::numbers::format_grouped;

pub const APP_TITLE: &str = "FileCraft";

pub const COMMON_INFO_TITLE: &str = "Information";
pub const COMMON_WARNING_TITLE: &str = "Warning";
pub const COMMON_LOAD_BUTTON: &str = "Load";
pub const COMMON_CLOSE_BUTTON: &str = "Close";
pub const COMMON_ERROR_TITLE: &str = "Error";
pub const COMMON_OK_BUTTON: &str = "OK";
pub const COMMON_YES_BUTTON: &str = "Yes";
pub const COMMON_CANCEL_BUTTON: &str = "Cancel";
pub const COMMON_FOLDERS_HEADER: &str = "Folders to include";

pub const UNDO_TOOLTIP: &str = "Undo (Ctrl+Z)";
pub const REDO_TOOLTIP: &str = "Redo (Ctrl+Y)";
pub const SAVE_SETTINGS_TOOLTIP: &str = "Save current settings (Ctrl+S)";
pub const THEME_DARK_TOOLTIP: &str = "Switch to dark theme";
pub const THEME_LIGHT_TOOLTIP: &str = "Switch to light theme";

/// Shown when the preferences cannot be read at startup.
pub fn preferences_load_error(message: &str) -> String {
    format!("The preferences could not be loaded, the system theme is used.\n\n{message}")
}

/// Shown when the chosen theme cannot be written.
pub fn preferences_save_error(message: &str) -> String {
    format!("The theme could not be saved.\n\n{message}")
}

pub const SOURCE_FOLDER_LABEL: &str = "Source folder";
pub const DESTINATION_FOLDER_LABEL: &str = "Destination folder";
pub const BROWSE_BUTTON: &str = "Browse...";
pub const CLEAR_BUTTON: &str = "Clear";
pub const CLEAR_PATHS_TOOLTIP: &str = "Clear both source and destination paths.";
pub const SELECT_SOURCE_FOLDER: &str = "Select the common source folder";
pub const SELECT_DESTINATION_FOLDER: &str = "Select the common destination folder";

pub const TAB_FILE_CONTENT_EXPORT: &str = "File Content Export";
pub const TAB_TREE_GENERATOR: &str = "Tree Generator";
pub const TAB_FOLDER_CONTENT_EXPORT: &str = "Folder Content Export";
pub const TAB_FILE_RENAME: &str = "File Rename";
pub const TAB_OPTIONS: &str = "Options";

pub const FILE_RENAME_FOLDER_LABEL: &str = "Folder to rename";
pub const FILE_RENAME_SELECT_FOLDER_TITLE: &str =
    "Select the folder whose files you want to rename";
pub const FILE_RENAME_MODE_HEADER: &str = "Rename mode";
pub const FILE_RENAME_BY_DATE_OPTION: &str = "By date";
pub const FILE_RENAME_NUMBERING_OPTION: &str = "Numbering";
pub const FILE_RENAME_UNDO_OPTION: &str = "Undo";
pub const FILE_RENAME_BY_DATE_DESCRIPTION: &str = "Photos and videos get names like 2024_05_31_001.jpg. The date comes from the photo, the file name or the last change, in this order.";
pub const FILE_RENAME_NUMBERING_DESCRIPTION: &str =
    "All files except .txt files get names like 00001.jpg, in file name order.";
pub const FILE_RENAME_PREVIEW_HEADER: &str = "Preview";
pub const FILE_RENAME_RESULT_HEADER: &str = "Result";
pub const FILE_RENAME_FILES_VIEW: &str = "Files";
pub const FILE_RENAME_LOG_VIEW: &str = "Log";
pub const FILE_RENAME_REFRESH_TOOLTIP: &str = "Read the folder again";
pub const FILE_RENAME_READING: &str = "Reading the folder...";
pub const FILE_RENAME_RUNNING: &str = "Renaming...";
pub const FILE_RENAME_CURRENT_NAME_COLUMN: &str = "Current name";
pub const FILE_RENAME_NEW_NAME_COLUMN: &str = "New name";
pub const FILE_RENAME_DATE_COLUMN: &str = "Date from";
pub const FILE_RENAME_SKIP_TYPE: &str = "Skipped, not a photo or video";
pub const FILE_RENAME_SKIP_TEXT: &str = "Skipped, text file";
pub const FILE_RENAME_SKIP_SAME: &str = "Skipped, already named";
pub const FILE_RENAME_SKIP_RESTORED: &str = "Skipped, already has its original name";
pub const FILE_RENAME_SKIP_CHANGED: &str = "Skipped, changed since the renaming";
pub const FILE_RENAME_SKIP_MISSING: &str = "Skipped, no longer in the folder";
pub const FILE_RENAME_SKIP_UNCHECKED: &str = "Skipped, cannot be checked";
pub const FILE_RENAME_UNDO_DESCRIPTION: &str =
    "Restores the original file names. Changed or moved files are skipped.";
pub const FILE_RENAME_UNDO_NEEDS_FOLDER: &str = "Select a folder to see what can be undone.";
pub const FILE_RENAME_UNDO_EMPTY: &str = "There is nothing to undo in this folder.";
pub const FILE_RENAME_ORIGINAL_NAME_COLUMN: &str = "Original name";
pub const FILE_RENAME_TO_RESTORE_LABEL: &str = "to restore";
pub const FILE_RENAME_RESTORED_LABEL: &str = "restored";
pub const FILE_RENAME_UNDO_BUTTON: &str = "Restore names";
pub const FILE_RENAME_UNDOING: &str = "Restoring...";
pub const FILE_RENAME_UNDO_NO_RUN: &str = "There is no renaming to undo in this folder.";
pub const FILE_RENAME_UNDO_NOTHING: &str = "No file would get its original name back.";
pub const FILE_RENAME_UNDO_PREVIEW_FAILED: &str = "The log of this renaming could not be read.";
pub const FILE_RENAME_UNDO_CONFIRM_TITLE: &str = "Restore original names";
pub const FILE_RENAME_UNDO_CONFIRM_BUTTON: &str = "Restore";
pub const TOAST_NAMES_RESTORED: &str = "Original names restored";

/// A renaming in the list of an undo, such as `2026-11-09 14:30:12 - by date, 10 files`.
pub fn file_rename_run_label(
    started: &str,
    by_date: bool,
    renamed: usize,
    finished: bool,
    undone: bool,
) -> String {
    let mut label = format!(
        "{started} - {}, {}",
        if by_date { "by date" } else { "numbering" },
        grouped_count_of(renamed, "file", "files")
    );
    if !finished {
        label.push_str(", interrupted");
    }
    if undone {
        label.push_str(", undone before");
    }
    label
}

/// A log in the list of an undo that cannot be read.
pub fn file_rename_damaged_run_label(log_name: &str) -> String {
    format!("{log_name} - cannot be read")
}

/// The tooltip of the chosen renaming of an undo.
pub fn file_rename_run_tooltip(log_name: &str) -> String {
    format!("Log: {log_name}")
}

/// Asks before giving the files of a renaming their original names back.
pub fn file_rename_undo_confirm(started: &str, count: usize) -> String {
    format!(
        "The files renamed on {started} will get their original names back.\n\n{} will be renamed.",
        grouped_count_of(count, "file", "files")
    )
}

/// Shown when an undo stops because of an error.
pub fn file_rename_undo_error(message: &str) -> String {
    format!("The undo stopped because of an error.\n\n{message}")
}

pub fn file_rename_will_restore(name: &str, original: &str) -> String {
    format!("'{name}' will get its original name '{original}' back.")
}

pub fn file_rename_was_restored(name: &str, original: &str) -> String {
    format!("'{name}' got its original name '{original}' back.")
}

pub fn restored_count(count: usize) -> String {
    format!("{count} restored")
}

/// Shown instead of the new name when that name is taken.
pub fn file_rename_skip_taken(name: &str) -> String {
    format!("Skipped, {name} already exists")
}

/// Shown instead of the new name when the file could not be renamed.
pub fn file_rename_error_label(error: &str) -> String {
    format!("Error: {error}")
}

pub const FILE_RENAME_SOURCE_METADATA: &str = "Metadata";
pub const FILE_RENAME_SOURCE_FILE_NAME: &str = "File name";
pub const FILE_RENAME_SOURCE_MODIFIED: &str = "Last modified";
pub const FILE_RENAME_EMPTY_NO_FOLDER: &str =
    "Select a folder to see how its files will be renamed.";
pub const FILE_RENAME_EMPTY_FOLDER: &str = "This folder has no files.";
pub const FILE_RENAME_EMPTY_LOG: &str = "The log of the renaming appears here.";
pub const FILE_RENAME_FILES_LABEL: &str = "files";
pub const FILE_RENAME_TO_RENAME_LABEL: &str = "to rename";
pub const FILE_RENAME_RENAMED_LABEL: &str = "renamed";
pub const FILE_RENAME_SKIPPED_LABEL: &str = "skipped";
pub const FILE_RENAME_ERRORS_LABEL: &str = "errors";
pub const FILE_RENAME_BUTTON: &str = "Rename files";
pub const FILE_RENAME_NEEDS_FOLDER: &str = "Select a folder first.";
pub const FILE_RENAME_WAIT_FOR_PREVIEW: &str = "Wait until the folder is read.";
pub const FILE_RENAME_REFRESH_FIRST: &str = "Refresh the preview before renaming again.";
pub const FILE_RENAME_NOTHING_TO_RENAME: &str = "No file would be renamed.";
pub const FILE_RENAME_CONFIRM_TITLE: &str = "Rename files";
pub const FILE_RENAME_CONFIRM_BUTTON: &str = "Rename";
pub const FILE_RENAME_STOPPED: &str = "The renaming stopped unexpectedly.";
pub const FILE_RENAME_PREVIEW_FAILED: &str =
    "The folder could not be read. Refresh the preview to try again.";
pub const FILE_RENAME_SHOW_ALL_BUTTON: &str = "Show all files";
pub const FILE_RENAME_SHOW_ALL_TOOLTIP: &str =
    "Reads every file of the folder. In a large folder this can take a while.";
pub const FILE_RENAME_STOP_BUTTON: &str = "Stop";
pub const FILE_RENAME_STOP_TOOLTIP: &str = "Keep the files read so far";
pub const TOAST_FILES_RENAMED: &str = "Files renamed";

/// The end of a preview that shows only the first files of the folder.
pub fn file_rename_showing_first(shown: usize, total: usize) -> String {
    format!(
        "Showing the first {} of {}.",
        format_grouped(shown as u64),
        grouped_count_of(total, "file", "files")
    )
}

/// The progress of a preview of all files.
pub fn file_rename_reading_all(read: usize, total: usize) -> String {
    format!(
        "Reading all files... {} of {}",
        format_grouped(read as u64),
        format_grouped(total as u64)
    )
}

/// A count out of a total, such as `100 of 2 340`.
pub fn shown_of(shown: usize, total: usize) -> String {
    format!(
        "{} of {}",
        format_grouped(shown as u64),
        format_grouped(total as u64)
    )
}

/// Asks before renaming by date when the preview showed only the first files.
pub fn file_rename_by_date_confirm_partial(total: usize, shown: usize) -> String {
    format!(
        "{FILE_RENAME_BY_DATE_SCOPE}\n\n{}\n\n{FILE_RENAME_UNDO_HINT}",
        file_rename_partial_note(total, shown)
    )
}

/// Asks before numbering when the preview showed only the first files.
pub fn file_rename_numbering_confirm_partial(total: usize, shown: usize) -> String {
    format!(
        "{FILE_RENAME_NUMBERING_SCOPE}\n\n{}\n\n{FILE_RENAME_UNDO_HINT}",
        file_rename_partial_note(total, shown)
    )
}

/// Asks before renaming the photos and videos of a folder by date.
pub fn file_rename_by_date_confirm(count: usize) -> String {
    format!(
        "{FILE_RENAME_BY_DATE_SCOPE}\n\n{}\n\n{FILE_RENAME_UNDO_HINT}",
        file_rename_will_rename_count(count)
    )
}

/// Asks before numbering the files of a folder.
pub fn file_rename_numbering_confirm(count: usize) -> String {
    format!(
        "{FILE_RENAME_NUMBERING_SCOPE}\n\n{}\n\n{FILE_RENAME_UNDO_HINT}",
        file_rename_will_rename_count(count)
    )
}

const FILE_RENAME_BY_DATE_SCOPE: &str =
    "Photos and videos in this folder will be renamed based on their dates.";
const FILE_RENAME_NUMBERING_SCOPE: &str =
    "All files in this folder except .txt files will be renamed to numbers.";
pub const FILE_RENAME_CANNOT_UNDO: &str = "This action cannot be undone.";
const FILE_RENAME_UNDO_HINT: &str = "The original names can be restored later in the Undo mode, as long as the files are not changed or moved.";

fn file_rename_partial_note(total: usize, shown: usize) -> String {
    format!(
        "This folder contains {}, but the preview showed only the first {}. Any remaining eligible files will also be renamed.",
        grouped_count_of(total, "file", "files"),
        format_grouped(shown as u64)
    )
}

fn file_rename_will_rename_count(count: usize) -> String {
    format!(
        "{} will be renamed.",
        grouped_count_of(count, "file", "files")
    )
}

/// Shown when a run stops because of an error.
pub fn file_rename_error(message: &str) -> String {
    format!("The renaming stopped because of an error.\n\n{message}")
}

pub fn file_rename_will_rename(name: &str, new_name: &str) -> String {
    format!("'{name}' will be renamed to '{new_name}'.")
}

pub fn file_rename_was_renamed(name: &str, new_name: &str) -> String {
    format!("'{name}' was renamed to '{new_name}'.")
}

pub fn file_rename_skipped(name: &str, reason: &str) -> String {
    format!("'{name}' is skipped, {reason}.")
}

pub fn file_rename_failed(name: &str, error: &str) -> String {
    format!("'{name}' could not be renamed: {error}")
}

pub fn renamed_count(count: usize) -> String {
    format!("{count} renamed")
}

pub fn skipped_count(count: usize) -> String {
    format!("{count} skipped")
}

pub fn failed_count(count: usize) -> String {
    count_of(count, "error", "errors")
}

pub const FULLSCREEN_TOOLTIP: &str = "Fullscreen";
pub const EXIT_FULLSCREEN_TOOLTIP: &str = "Exit Fullscreen";

pub const TREE_GEN_FOLDERS_AFFECTED_LABEL: &str = "folders affected";
pub const TREE_GEN_GENERATE_BUTTON: &str = "Generate tree structure";
pub const TREE_GEN_CONFIRM_GENERATE_MESSAGE: &str =
    "Are you sure you want to generate the tree structure to the following path?";
pub const TREE_GEN_GENERATE_TITLE: &str = "Generate Tree Structure";
pub const TREE_GEN_GENERATING_BUTTON: &str = "Generating...";
pub const TREE_GEN_GENERATE_AS_HEADER: &str = "Generate as";
pub const TREE_GEN_STRUCTURED_TREE_OPTION: &str = "Structured tree";
pub const TREE_GEN_PATHS_ONLY_OPTION: &str = "Paths only";
/// What a structured tree looks like, shown on its card.
pub const TREE_GEN_STRUCTURED_SAMPLE: [&str; 4] =
    ["Project", "├── src", "│   └── main.rs", "└── README.md"];
/// What a paths only list looks like, shown on its card. `/` stands for the path separator.
pub const TREE_GEN_PATHS_ONLY_SAMPLE: [&str; 3] = ["src", "src/main.rs", "README.md"];

/// Formats `TreeGen_ErrorMessage`.
pub fn tree_gen_error(message: &str) -> String {
    format!("An unexpected error occurred:\n{message}")
}

pub const FOLDER_CONTENT_DETAILS_HEADER: &str = "Select details";
pub const FOLDER_CONTENT_FILES_AFFECTED_LABEL: &str = "files affected";
pub const FOLDER_CONTENT_PROTECT_SPREADSHEET_FORMULAS: &str = "Protect spreadsheet formulas";
pub const FOLDER_CONTENT_PROTECT_SPREADSHEET_FORMULAS_TOOLTIP: &str =
    "Prefixes values that could be interpreted as spreadsheet formulas.";
pub const FOLDER_CONTENT_EXPORT_BUTTON: &str = "Export folder contents";
pub const FOLDER_CONTENT_NO_FOLDERS_SELECTED: &str =
    "No folders were selected. Please select at least one folder.";
pub const FOLDER_CONTENT_NO_COLUMNS_SELECTED: &str =
    "No columns were selected. Please select at least one column to export.";
pub const FOLDER_CONTENT_CONFIRM_EXPORT_MESSAGE: &str =
    "Are you sure you want to export folder contents to the following path?";
pub const FOLDER_CONTENT_EXPORT_TITLE: &str = "Export Folder Contents";
pub const FOLDER_CONTENT_NEEDS_COLUMNS: &str = "Select at least one detail to export.";
pub const FOLDER_CONTENT_GROUP_FILE_DETAILS: &str = "File details";
pub const FOLDER_CONTENT_GROUP_AUDIO_TAGS: &str = "Audio tags";
pub const FOLDER_CONTENT_GROUP_AUDIO_PROPERTIES: &str = "Audio properties";
pub const FOLDER_CONTENT_GROUP_DIAGNOSTICS: &str = "Diagnostics";

pub fn folder_content_columns_selected(selected: usize, total: usize) -> String {
    format!("{selected} of {total} columns")
}

/// Formats `FolderContent_ErrorMessage`.
pub fn folder_content_error(message: &str) -> String {
    format!("An unexpected error occurred:\n{message}")
}

pub const EXPORT_OPTIONS_FILE_NAME_LABEL: &str = "File name";
pub const EXPORT_OPTIONS_APPEND_TIMESTAMP_LABEL: &str = "Timestamp";
pub const EXPORT_OPTIONS_APPEND_TIMESTAMP_TOOLTIP: &str =
    "Adds _yyyy_MM_dd_HH_mm_ss to the file name.";

pub const CONFIRMATION_FILES_AFFECTED_LABEL: &str = "files affected";
pub const CONFIRMATION_FOLDERS_AFFECTED_LABEL: &str = "folders affected";

pub const REPLACE_FILE_BUTTON: &str = "Replace";

/// Asks before an export replaces an existing file. The folder is shown below it.
pub fn replace_file_message(file_name: &str) -> String {
    format!("{file_name} already exists in this folder. Do you want to replace it?")
}

pub const EXPORTING_BUTTON: &str = "Exporting...";
pub const COUNTING_LABEL: &str = "Counting...";
pub const SCANNING_LABEL: &str = "Scanning folders...";
pub const NEEDS_SOURCE_FOLDER: &str = "Choose a source folder first.";
pub const NEEDS_DESTINATION_FOLDER: &str = "Choose a destination folder first.";
pub const NEEDS_FILE_NAME: &str = "Enter a file name.";
pub const NEEDS_FOLDERS: &str = "Select at least one folder.";
pub const WAIT_FOR_SCAN: &str = "Wait until the folders are scanned.";

pub const FOLDERS_EMPTY_MESSAGE: &str = "Choose a source folder to see its folders here.";
pub const CHOOSE_SOURCE_FOLDER_BUTTON: &str = "Choose source folder";

pub const TOAST_FILE_CONTENTS_EXPORTED: &str = "File contents exported";
pub const TOAST_FOLDER_CONTENTS_EXPORTED: &str = "Folder contents exported";
pub const TOAST_TREE_CREATED: &str = "Tree structure created";
pub const TOAST_SETTINGS_SAVED: &str = "Settings saved";
pub const TOAST_SETTINGS_RESET: &str = "Settings reset";
pub const TOAST_SETTINGS_RESET_MESSAGE: &str = "All settings are back to their defaults.";
pub const TOAST_PRESET_SAVED: &str = "Preset saved";
pub const TOAST_PRESET_LOADED: &str = "Preset loaded";
pub const TOAST_PRESET_DELETED: &str = "Preset deleted";
pub const TOAST_PATH_PRESET_SAVED: &str = "Path preset saved";
pub const TOAST_TREE_COPIED: &str = "Folder tree copied";

/// A count with its unit, such as "1 file" or "12 files".
pub fn count_of(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

/// A count with its unit and grouped thousands, such as "2 340 files".
pub fn grouped_count_of(count: usize, singular: &str, plural: &str) -> String {
    format!(
        "{} {}",
        format_grouped(count as u64),
        if count == 1 { singular } else { plural }
    )
}

pub const CHARACTERS_UNIT: &str = "characters";
pub const LINES_UNIT: &str = "lines";

pub const TOAST_REMOVED_COMMENTS_CAPTION: &str = "Removed '///' comments";

pub fn percent_smaller(percent: &str) -> String {
    format!("{percent}% smaller")
}

/// Which folder tree was copied where.
pub fn tree_copied(from: &str, to: &str) -> String {
    format!("From {from} to {to}.")
}

/// A preset name shown in a notification.
pub fn quoted(name: &str) -> String {
    format!("'{name}'")
}

pub const TOAST_OPEN_FOLDER_BUTTON: &str = "Open folder";
pub const TOAST_COPY_PATH_BUTTON: &str = "Copy path";
pub const TOAST_PATH_COPIED: &str = "Copied";
pub const TOAST_CLOSE_TOOLTIP: &str = "Close";

/// Shown when the folder of an exported file cannot be opened.
pub fn open_folder_error(message: &str) -> String {
    format!("The folder could not be opened.\n\n{message}")
}

pub const COMMON_SAVE_BUTTON: &str = "Save";
pub const COMMON_VERSION_LABEL: &str = "Version";

pub const UNSAVED_CHANGES_TITLE: &str = "Unsaved changes";
pub const UNSAVED_CHANGES_MESSAGE: &str =
    "You have unsaved changes. Do you want to save before quitting?";
pub const EXIT_CONFIRMATION_DONT_SAVE_BUTTON: &str = "Don't save";

/// Shown when writing the save file fails.
pub fn settings_save_error(message: &str) -> String {
    format!("The settings could not be saved.\n\n{message}")
}

/// Shown when the save file cannot be loaded at startup.
pub fn settings_load_error(message: &str) -> String {
    format!("The saved settings could not be loaded, default settings are used.\n\n{message}")
}

pub const MANAGE_PRESETS_HEADER: &str = "Manage presets";
pub const OPEN_SAVE_FOLDER_BUTTON: &str = "Open save folder";
pub const DELETE_CURRENT_SAVE_BUTTON: &str = "Delete current save";
pub const COPY_FOLDER_TREE_CONTENT_HEADER: &str = "Copy folder tree content";
pub const IGNORE_FOLDERS_HEADER: &str = "Ignore folders";
pub const EDIT_FOLDERS_BUTTON: &str = "Edit folders";
pub const IGNORED_FOLDERS_NONE: &str = "No folders are ignored.";
pub const EDIT_IGNORED_TITLE: &str = "Edit ignored folders";
pub const EDIT_IGNORED_INSTRUCTION: &str =
    "Enter the names of the folders to ignore, separated by commas.";

pub const OPEN_SAVE_ERROR_NOT_FOUND: &str = "The save folder could not be found.";

/// Formats `Options_OpenSaveError_Exception`.
pub fn open_save_error(message: &str) -> String {
    format!("An error occurred while trying to open the folder:\n{message}")
}

pub const RESET_CONFIRM_TITLE: &str = "Delete current save";
pub const RESET_CONFIRM_MESSAGE: &str = "Are you sure you want to delete all current settings and reset to default? This action cannot be undone.";

/// Formats `Reset_ErrorMessage`.
pub fn reset_error(message: &str) -> String {
    format!("Failed to delete save data.\n\n{message}")
}

pub const SETTINGS_FOLDER_UNAVAILABLE: &str =
    "The settings folder of this user could not be determined.";

pub const COMMON_SELECT_ALL: &str = "Select all";
pub const COMMON_CLEAR_ALL: &str = "Clear all";
pub const COMMON_FILTER_PLACEHOLDER: &str = "Filter by path...";

pub const FILE_CONTENT_EXTENSIONS_HEADER: &str = "File extensions to include";
pub const FILE_CONTENT_EXTENSIONS_PLACEHOLDER: &str = "Select extensions";
pub const FILE_CONTENT_IGNORE_COMMENTS_BUTTON: &str = "Ignore comment on these files";
pub const FILE_CONTENT_IGNORE_COMMENTS_TOOLTIP: &str = "Select files to ignore '///' comments.";
pub const FILE_CONTENT_FILES_HEADER: &str = "Files to export";
pub const FILE_CONTENT_BULK_SEARCH_BUTTON: &str = "Bulk search";
pub const FILE_CONTENT_FILTER_ALL: &str = "All";
pub const FILE_CONTENT_FILTER_SELECTED: &str = "Selected";
pub const FILE_CONTENT_FILTER_UNSELECTED: &str = "Unselected";
pub const FILE_CONTENT_CLEAR_FILTER_BUTTON: &str = "Clear filter";
pub const FILE_CONTENT_TOTAL_FILES_LABEL: &str = "total";
pub const FILE_CONTENT_FILTERED_FILES_LABEL: &str = "shown";
pub const FILE_CONTENT_SELECTED_FILES_LABEL: &str = "selected";

/// The export button of the File Content Export tab, with the number of selected files.
pub fn file_content_export_button(selected: usize) -> String {
    match selected {
        0 => "Export files".to_owned(),
        1 => "Export 1 file".to_owned(),
        count => format!("Export {} files", format_grouped(count as u64)),
    }
}
pub const FILE_CONTENT_NO_EXTENSIONS_SELECTED: &str = "No extensions selected.";
pub const FILE_CONTENT_NO_IGNORED_FILES: &str = "No files selected to ignore comments.";
pub const FILE_CONTENT_CONFIRM_EXPORT_MESSAGE: &str =
    "Are you sure you want to export file contents to the following path?";
pub const FILE_CONTENT_EXPORT_TITLE: &str = "Export File Contents";
pub const FILE_CONTENT_REMOVE_EXTENSION_TOOLTIP: &str = "Remove";
pub const FILE_CONTENT_EXTENSION_SEARCH_PLACEHOLDER: &str = "Search extensions...";
pub const FILE_CONTENT_NO_EXTENSIONS_FOUND: &str = "No files in the selected folders.";
pub const FILE_CONTENT_NEEDS_FILES: &str = "Select at least one file to export.";
pub const FILE_CONTENT_EMPTY_NO_FOLDERS: &str = "Select folders to list their files.";
pub const FILE_CONTENT_EMPTY_NO_EXTENSIONS: &str = "Select file extensions to list matching files.";
pub const FILE_CONTENT_EMPTY_NO_MATCH: &str = "No files match the filter.";

/// The closed extension picker: how many of the found extensions are selected.
pub fn extensions_selected(selected: usize, total: usize) -> String {
    format!("{selected} of {total} extensions selected")
}
pub const FILE_CONTENT_MANAGE_PATH_PRESETS_BUTTON: &str = "Manage path presets";

/// Formats `FileContent_ExportErrorMessage`.
pub fn file_content_export_error(message: &str) -> String {
    format!("An unexpected error occurred during export:\n{message}")
}

/// Replaces the `{0}` placeholder of a message.
pub fn format1(template: &str, value: &str) -> String {
    template.replace("{0}", value)
}

pub const IGNORED_COMMENTS_TITLE: &str = "Select files to ignore '///' comments";
pub const IGNORED_COMMENTS_CHARS_HEADER: &str = "Chars";
pub const IGNORED_COMMENTS_TOTAL_LABEL: &str = "Total:";
pub const IGNORED_COMMENTS_STATUS_NA: &str = "N/A";
pub const IGNORED_COMMENTS_STATUS_ERR: &str = "Err";

pub const BULK_SEARCH_TITLE: &str = "Bulk Search";
pub const BULK_SEARCH_INPUT_PATHS_LABEL: &str = "Input paths";
pub const BULK_SEARCH_ONE_PER_LINE: &str = "one per line";
pub const BULK_SEARCH_FOUND_PATHS_LABEL: &str = "Found paths";
pub const BULK_SEARCH_LINES_MATCHED_LABEL: &str = "lines matched";
pub const BULK_SEARCH_FOUND_LABEL: &str = "found";
pub const BULK_SEARCH_CHANGED_LABEL: &str = "changed";
pub const BULK_SEARCH_REMOVE_WHITESPACES_BUTTON: &str = "Remove whitespaces";
pub const BULK_SEARCH_BREAK_INTO_LINES_BUTTON: &str = "Break into lines";
pub const BULK_SEARCH_BREAK_BY_LABEL: &str = "by";
pub const BULK_SEARCH_SPACE_OPTION: &str = "Space";
pub const BULK_SEARCH_SEMICOLON_OPTION: &str = ";";

pub const BULK_SEARCH_APPLY_BUTTON: &str = "Apply";
pub const BULK_SEARCH_APPLY_TITLE: &str = "Apply Bulk Search";
pub const BULK_SEARCH_APPLY_MESSAGE: &str = "Apply bulk search changes?";

pub const DELETE_PRESET_TOOLTIP: &str = "Delete this preset";
pub const LAST_MODIFIED_LABEL: &str = "Last modified:";

pub const COPY_FROM_TAB_LABEL: &str = "Copy from this tab:";
pub const COPY_TO_TAB_LABEL: &str = "Copy to this tab:";
pub const COPY_FOLDER_TREE_BUTTON: &str = "Copy folder tree";
pub const CONFIRMATION_COPY_TREE_PART1: &str = "Are you sure you want to copy the folder tree of";
pub const CONFIRMATION_COPY_TREE_PART2: &str = "to";
pub const OPTIONS_TAB_NONE: &str = "-- None --";
pub const OPTIONS_COPY_TREE_TITLE: &str = "Copy folder tree";
pub const OPTIONS_LINK_FOLDER_TREES_BUTTON: &str = "Link folder trees";
pub const OPTIONS_UNLINK_TOOLTIP: &str = "Click to unlink this group of folder trees.";

/// Formats the folder count of the copy tree confirmation.
pub fn folder_count(count: usize) -> String {
    format!("({count} folders)")
}

pub const RENAME_PRESET_TITLE: &str = "Edit preset name";
pub const RENAME_PRESET_PROMPT: &str = "Preset {0} name:";

/// Formats the error shown when the stored presets cannot be read.
pub fn presets_load_error(message: &str) -> String {
    format!(
        "The saved presets could not be loaded. They are kept unchanged.
{message}"
    )
}
pub const PRESET_SELECT_SOURCE_FIRST: &str =
    "Please select a valid source folder before saving a preset.";
pub const PRESET_NAME_EXISTS_USE_DIFFERENT: &str =
    "A preset with this name already exists. Do you want to use a different name?";
pub const PRESET_NAME_EXISTS: &str = "A preset with this name already exists.";
pub const PRESET_OVERWRITE_MESSAGE: &str =
    "Are you sure you want to overwrite the preset '{0}' with the current settings?";
pub const PRESET_SAVE_ERROR: &str = "Failed to save the preset.\n\n{0}";
pub const PRESET_LOAD_SELECT_SOURCE_FIRST: &str =
    "To load a preset, please first select the root source folder for your project.";
pub const PRESET_LOAD_CONFIRM_MESSAGE: &str = "Are you sure you want to load ALL settings from the preset '{0}'?\nThis will overwrite your entire current configuration.";
pub const PRESET_LOAD_CONFIRM_TITLE: &str = "Load preset";
pub const PRESET_DELETE_CONFIRM_MESSAGE: &str = "You are about to delete the preset '{0}'.\nThis action cannot be undone.\n\nAre you sure you want to continue?";
pub const PRESET_DELETE_CONFIRM_TITLE: &str = "Delete preset";
pub const PRESET_DELETE_ERROR: &str = "Failed to delete the preset.\n\n{0}";
pub const PRESET_OVERWRITE_CURRENT_CONFIRM_TITLE: &str = "Overwrite Preset";
pub const PRESET_OVERWRITE_CURRENT_CONFIRM_MESSAGE: &str =
    "Are you sure you want to overwrite preset '{0}' with your current settings?";

pub const PATH_PRESET_TITLE: &str = "Path Presets Manager";
pub const PATH_PRESET_SAVE_CURRENT_BUTTON: &str = "Save selection as new preset";
pub const PATH_PRESET_DELETE_CONFIRM_TITLE: &str = "Delete Path Preset";
pub const PATH_PRESET_DELETE_CONFIRM_MESSAGE: &str =
    "Are you sure you want to delete path preset '{0}'?";
pub const PATH_PRESET_NAME_PROMPT: &str = "Enter preset name:";
pub const PATH_PRESET_NEW_PRESET_TITLE: &str = "New Path Preset";
pub const PATH_PRESET_RENAME_TITLE: &str = "Rename Path Preset";
pub const PATH_PRESET_RENAME_PROMPT: &str = "Rename preset '{0}' to:";
pub const PATH_PRESET_NO_SELECTION_ERROR: &str = "Please select files in the main window first.";
pub const PATH_PRESET_LOAD_CONFIRM_TITLE: &str = "Load Path Preset";
pub const PATH_PRESET_LOAD_CONFIRM_MESSAGE: &str =
    "Are you sure you want to load paths from preset '{0}'?";

/// The description of a path preset in the list.
pub fn path_preset_description(count: usize) -> String {
    format!("{count} files")
}

pub const PRESET_SUMMARY_TITLE: &str = "Preset Load Summary";
pub const PRESET_SUMMARY_TOTAL: &str = "Total paths:";
pub const PRESET_SUMMARY_LOADED: &str = "Successfully loaded:";
pub const PRESET_SUMMARY_CHANGED: &str = "Changed:";
pub const PRESET_SUMMARY_UNCHANGED: &str = "Unchanged:";
pub const PRESET_SUMMARY_NOT_FOUND: &str = "Not found:";

pub const PRESET_LIST_SORT_BY_LABEL: &str = "Sort by:";
pub const PRESET_LIST_SEARCH_PLACEHOLDER: &str = "Search presets...";
pub const PRESET_LIST_DETAILS_BUTTON: &str = "Details";
pub const PRESET_LIST_ASCENDING_TOOLTIP: &str = "Ascending";
pub const PRESET_LIST_DESCENDING_TOOLTIP: &str = "Descending";
pub const PRESET_LIST_OVERWRITE_BUTTON: &str = "Save current settings to this preset";

/// Formats the number of presets below the list.
pub fn preset_count(count: usize) -> String {
    format!("Count: {count}")
}

/// The date of a preset with the time since then.
pub fn preset_date(last_modified: chrono::NaiveDateTime, now: chrono::NaiveDateTime) -> String {
    let elapsed = now - last_modified;
    let plural =
        |count: i64, unit: &str| format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" });
    let relative = if elapsed.num_minutes() < 1 {
        "Just now".to_owned()
    } else if elapsed.num_hours() < 1 {
        plural(elapsed.num_minutes(), "minute")
    } else if elapsed.num_days() < 1 {
        plural(elapsed.num_hours(), "hour")
    } else {
        plural(elapsed.num_days(), "day")
    };
    format!(
        "{} \u{2022} {relative}",
        last_modified.format("%Y.%m.%d %H:%M")
    )
}

pub const DETAILS_BACK: &str = "Back";
pub const DETAILS_SWITCH_TO_TREE: &str = "Switch to Tree view";
pub const DETAILS_SWITCH_TO_LIST: &str = "Switch to List view";
pub const DETAILS_FOLDERS_TITLE: &str = "Included Folders";
pub const DETAILS_FILES_TITLE: &str = "Included Files";
pub const DETAILS_TOTAL_FILES: &str = "Total Files";
pub const DETAILS_APP_VERSION: &str = "App Version";
pub const DETAILS_FOLDERS_CONFIGURED: &str = "Folders Configured";
pub const DETAILS_EXPLICIT_FILES: &str = "Explicit Files";
pub const DETAILS_INCLUDED_FILES_HEADER: &str = "Included Files";
pub const DETAILS_CONFIGURATION_SUMMARY_HEADER: &str = "Configuration Summary";
