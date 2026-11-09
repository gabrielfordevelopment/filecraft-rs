//! Saved application state in `save.json`.
//!
//! Field names use PascalCase and enums are stored as numbers.
//! Fields that this application does not know yet are kept in `extra` maps and written back unchanged.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::tree_generator::TreeGenerationMode;

pub const DEFAULT_FILE_CONTENTS_NAME: &str = "FileContents";
pub const DEFAULT_FOLDER_CONTENTS_NAME: &str = "FolderContents";
pub const DEFAULT_TREE_STRUCTURE_NAME: &str = "TreeStructure";
pub const DEFAULT_IGNORED_FOLDERS: [&str; 2] = ["bin", "obj"];

/// Folder export columns selected by default, in column order.
pub const DEFAULT_FOLDER_EXPORT_COLUMNS: [&str; 10] = [
    "file.name",
    "file.sizeBytes",
    "file.creationTime",
    "file.lastWriteTime",
    "file.lastAccessTime",
    "file.isReadOnly",
    "file.attributes",
    "file.fullPath",
    "file.parent",
    "file.extension",
];

/// Reads `null` as the default value, so a missing list does not fail the whole file.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SaveData {
    #[serde(deserialize_with = "null_as_default")]
    pub preset_name: String,
    pub last_modified: Option<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub source_path: String,
    #[serde(deserialize_with = "null_as_default")]
    pub destination_path: String,
    pub selected_tab_index: i32,
    #[serde(deserialize_with = "null_as_default")]
    pub file_content_export: FileContentExportSettings,
    #[serde(deserialize_with = "null_as_default")]
    pub folder_content_export: FolderContentExportSettings,
    #[serde(deserialize_with = "null_as_default")]
    pub tree_generator: TreeGeneratorSettings,
    #[serde(deserialize_with = "null_as_default")]
    pub settings_page: SettingsPageSettings,
    /// `PresetSortCriteria`: 0 is name, 1 is date modified.
    pub path_preset_sort_by: i32,
    pub path_preset_is_descending: bool,
    pub save_preset_sort_by: i32,
    pub save_preset_is_descending: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for SaveData {
    fn default() -> Self {
        Self {
            preset_name: String::new(),
            last_modified: None,
            source_path: String::new(),
            destination_path: String::new(),
            selected_tab_index: 0,
            file_content_export: FileContentExportSettings::default(),
            folder_content_export: FolderContentExportSettings::default(),
            tree_generator: TreeGeneratorSettings::default(),
            settings_page: SettingsPageSettings::default(),
            path_preset_sort_by: 1,
            path_preset_is_descending: true,
            save_preset_sort_by: 1,
            save_preset_is_descending: true,
            extra: Map::new(),
        }
    }
}

/// Selection and expansion of one folder. Only folders that are selected, partially selected or
/// expanded are stored.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct FolderState {
    #[serde(deserialize_with = "null_as_default")]
    pub full_path: String,
    /// `None` means partially selected.
    pub is_selected: Option<bool>,
    pub is_expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct FileContentExportSettings {
    #[serde(deserialize_with = "null_as_default")]
    pub selected_extensions: Vec<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub selected_file_paths: Vec<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub ignored_comment_file_paths: Vec<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub folder_tree_state: Vec<FolderState>,
    #[serde(deserialize_with = "null_as_default")]
    pub output_file_name: String,
    pub append_timestamp: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for FileContentExportSettings {
    fn default() -> Self {
        Self {
            selected_extensions: Vec::new(),
            selected_file_paths: Vec::new(),
            ignored_comment_file_paths: Vec::new(),
            folder_tree_state: Vec::new(),
            output_file_name: DEFAULT_FILE_CONTENTS_NAME.to_owned(),
            append_timestamp: false,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct FolderContentExportSettings {
    #[serde(deserialize_with = "null_as_default")]
    pub selected_columns: Vec<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub folder_tree_state: Vec<FolderState>,
    pub protect_spreadsheet_formulas: bool,
    #[serde(deserialize_with = "null_as_default")]
    pub output_file_name: String,
    pub append_timestamp: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for FolderContentExportSettings {
    fn default() -> Self {
        Self {
            selected_columns: DEFAULT_FOLDER_EXPORT_COLUMNS
                .iter()
                .map(|id| (*id).to_owned())
                .collect(),
            folder_tree_state: Vec::new(),
            protect_spreadsheet_formulas: false,
            output_file_name: DEFAULT_FOLDER_CONTENTS_NAME.to_owned(),
            append_timestamp: false,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct TreeGeneratorSettings {
    #[serde(deserialize_with = "null_as_default")]
    pub folder_tree_state: Vec<FolderState>,
    #[serde(with = "generation_mode")]
    pub generation_mode: TreeGenerationMode,
    #[serde(deserialize_with = "null_as_default")]
    pub output_file_name: String,
    pub append_timestamp: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for TreeGeneratorSettings {
    fn default() -> Self {
        Self {
            folder_tree_state: Vec::new(),
            generation_mode: TreeGenerationMode::Structured,
            output_file_name: DEFAULT_TREE_STRUCTURE_NAME.to_owned(),
            append_timestamp: false,
            extra: Map::new(),
        }
    }
}

/// The tree generation mode as a number. Any value other than paths-only is treated as a
/// structured tree.
mod generation_mode {
    use super::*;

    const STRUCTURED: i32 = 0;
    const PATHS_ONLY: i32 = 1;

    pub fn serialize<S: Serializer>(
        mode: &TreeGenerationMode,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(match mode {
            TreeGenerationMode::Structured => STRUCTURED,
            TreeGenerationMode::PathsOnly => PATHS_ONLY,
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<TreeGenerationMode, D::Error> {
        Ok(match Option::<i32>::deserialize(deserializer)? {
            Some(PATHS_ONLY) => TreeGenerationMode::PathsOnly,
            _ => TreeGenerationMode::Structured,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SettingsPageSettings {
    #[serde(deserialize_with = "null_as_default")]
    pub ignored_folders: Vec<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub linked_folder_tree_groups: Vec<Vec<String>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for SettingsPageSettings {
    fn default() -> Self {
        Self {
            ignored_folders: DEFAULT_IGNORED_FOLDERS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            linked_folder_tree_groups: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// Parses the comma separated text of the ignored folders dialog: entries are trimmed, empty and
/// duplicate (case-insensitive) entries are dropped, and the result is sorted by name.
pub fn parse_ignored_folders(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in text
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if !names
            .iter()
            .any(|existing| existing.to_lowercase() == name.to_lowercase())
        {
            names.push(name.to_owned());
        }
    }
    names.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    names
}

/// Formats ignored folders for display and editing.
pub fn format_ignored_folders(names: &[String]) -> String {
    names.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SAVE: &str = r#"{
  "PresetName": "",
  "LastModified": null,
  "SourcePath": "E:\\project",
  "DestinationPath": "C:\\out",
  "SelectedTabIndex": 1,
  "FileContentExport": {
    "SelectedExtensions": [".cs"],
    "SelectedFilePaths": [],
    "IgnoredCommentFilePaths": [],
    "FolderTreeState": [{ "FullPath": "E:\\project", "IsSelected": null, "IsExpanded": true }],
    "OutputFileName": "FileContents",
    "AppendTimestamp": false
  },
  "TreeGenerator": {
    "FolderTreeState": [],
    "GenerationMode": 1,
    "OutputFileName": "Tree",
    "AppendTimestamp": true
  },
  "SettingsPage": { "IgnoredFolders": ["bin"], "LinkedFolderTreeGroups": [] },
  "PathPresetSortBy": 1,
  "PathPresetIsDescending": true,
  "SavePresetSortBy": 0,
  "SavePresetIsDescending": false,
  "FutureSetting": { "Value": 42 }
}"#;

    #[test]
    fn reads_save_file() {
        let data: SaveData = serde_json::from_str(SAMPLE_SAVE).unwrap();
        assert_eq!(data.source_path, "E:\\project");
        assert_eq!(data.selected_tab_index, 1);
        assert_eq!(
            data.tree_generator.generation_mode,
            TreeGenerationMode::PathsOnly
        );
        assert!(data.tree_generator.append_timestamp);
        assert_eq!(
            data.file_content_export.folder_tree_state[0].is_selected,
            None
        );
        assert_eq!(data.settings_page.ignored_folders, ["bin"]);
        assert_eq!(data.save_preset_sort_by, 0);
        // Sections missing from older files get the defaults.
        assert_eq!(
            data.folder_content_export.output_file_name,
            DEFAULT_FOLDER_CONTENTS_NAME
        );
    }

    #[test]
    fn keeps_unknown_fields() {
        let data: SaveData = serde_json::from_str(SAMPLE_SAVE).unwrap();
        let written = serde_json::to_value(&data).unwrap();
        assert_eq!(written["FutureSetting"]["Value"], 42);
        assert_eq!(written["TreeGenerator"]["GenerationMode"], 1);
        let reread: SaveData = serde_json::from_value(written).unwrap();
        assert_eq!(reread, data);
    }

    #[test]
    fn null_lists_read_as_empty() {
        let data: SaveData =
            serde_json::from_str(r#"{ "SettingsPage": { "IgnoredFolders": null } }"#).unwrap();
        assert!(data.settings_page.ignored_folders.is_empty());
    }

    #[test]
    fn default_values() {
        let data = SaveData::default();
        assert_eq!(data.settings_page.ignored_folders, ["bin", "obj"]);
        assert_eq!(
            data.tree_generator.output_file_name,
            DEFAULT_TREE_STRUCTURE_NAME
        );
        assert_eq!(data.folder_content_export.selected_columns.len(), 10);
    }

    #[test]
    fn parses_ignored_folders() {
        assert_eq!(
            parse_ignored_folders(" obj, bin,,Bin , node_modules ,"),
            ["bin", "node_modules", "obj"]
        );
        assert!(parse_ignored_folders(" , ").is_empty());
    }
}
