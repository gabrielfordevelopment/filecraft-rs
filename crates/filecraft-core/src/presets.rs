//! Saved setting presets and path presets, stored in `presets.json` next to `save.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::file_content::ignore_case_key;
use crate::settings::SaveData;

pub const PRESETS_FILE_NAME: &str = "presets.json";

/// How preset lists are sorted, stored as `SavePresetSortBy` and `PathPresetSortBy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetSortCriteria {
    Name,
    DateModified,
}

impl PresetSortCriteria {
    pub const ALL: [Self; 2] = [Self::Name, Self::DateModified];

    pub fn from_number(value: i32) -> Self {
        if value == 0 {
            Self::Name
        } else {
            Self::DateModified
        }
    }

    pub fn number(self) -> i32 {
        match self {
            Self::Name => 0,
            Self::DateModified => 1,
        }
    }

    /// The name the sort box shows.
    pub fn name(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::DateModified => "DateModified",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct PresetStatistics {
    pub folder_count: i64,
    pub file_count: i64,
    pub total_size: i64,
    pub total_line_count: i64,
    pub total_character_count: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PresetStatistics {
    /// Folder states of all trees and the explicitly selected files.
    pub fn of(data: &SaveData) -> Self {
        Self {
            folder_count: (data.file_content_export.folder_tree_state.len()
                + data.folder_content_export.folder_tree_state.len()
                + data.tree_generator.folder_tree_state.len()) as i64,
            file_count: data.file_content_export.selected_file_paths.len() as i64,
            ..Self::default()
        }
    }
}

/// A complete saved state with paths relative to the source folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SavePreset {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub created_at: NaiveDateTime,
    pub last_modified: NaiveDateTime,
    pub app_version: String,
    pub statistics: PresetStatistics,
    pub data: SaveData,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for SavePreset {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            description: String::new(),
            created_at: NaiveDateTime::default(),
            last_modified: NaiveDateTime::default(),
            app_version: String::new(),
            statistics: PresetStatistics::default(),
            data: SaveData::default(),
            extra: Map::new(),
        }
    }
}

/// File selections of the File Content Export, relative to the source folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct PathPreset {
    pub id: i32,
    pub name: String,
    pub file_paths: Vec<String>,
    pub last_modified: NaiveDateTime,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Presets {
    pub save_presets: Vec<SavePreset>,
    pub path_presets: Vec<PathPreset>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn same_name(a: &str, b: &str) -> bool {
    ignore_case_key(a) == ignore_case_key(b)
}

fn next_id<T>(items: &[T], id: impl Fn(&T) -> i32) -> i32 {
    items.iter().map(id).max().unwrap_or(0) + 1
}

impl Presets {
    pub fn save_preset(&self, id: i32) -> Option<&SavePreset> {
        self.save_presets.iter().find(|preset| preset.id == id)
    }

    pub fn path_preset(&self, id: i32) -> Option<&PathPreset> {
        self.path_presets.iter().find(|preset| preset.id == id)
    }

    /// Whether another save preset than `except` has this name, ignoring case.
    pub fn save_preset_name_exists(&self, name: &str, except: Option<i32>) -> bool {
        self.save_presets
            .iter()
            .any(|preset| Some(preset.id) != except && same_name(&preset.name, name))
    }

    /// Adds a save preset and returns its id.
    pub fn add_save_preset(
        &mut self,
        name: &str,
        data: SaveData,
        now: NaiveDateTime,
        app_version: &str,
    ) -> i32 {
        let id = next_id(&self.save_presets, |preset| preset.id);
        self.save_presets.push(SavePreset {
            id,
            name: name.to_owned(),
            created_at: now,
            last_modified: now,
            app_version: app_version.to_owned(),
            statistics: PresetStatistics::of(&data),
            data,
            ..SavePreset::default()
        });
        id
    }

    /// Replaces the saved state of a preset.
    pub fn update_save_preset_data(
        &mut self,
        id: i32,
        data: SaveData,
        now: NaiveDateTime,
        app_version: &str,
    ) -> bool {
        let Some(preset) = self.save_presets.iter_mut().find(|preset| preset.id == id) else {
            return false;
        };
        preset.statistics = PresetStatistics::of(&data);
        preset.data = data;
        preset.last_modified = now;
        preset.app_version = app_version.to_owned();
        true
    }

    pub fn rename_save_preset(&mut self, id: i32, name: &str, now: NaiveDateTime) -> bool {
        let Some(preset) = self.save_presets.iter_mut().find(|preset| preset.id == id) else {
            return false;
        };
        preset.name = name.to_owned();
        preset.last_modified = now;
        true
    }

    pub fn delete_save_preset(&mut self, id: i32) {
        self.save_presets.retain(|preset| preset.id != id);
    }

    pub fn path_preset_name_exists(&self, name: &str) -> bool {
        self.path_presets
            .iter()
            .any(|preset| same_name(&preset.name, name))
    }

    /// Stores file paths under a name. A preset with the same name, ignoring case, is replaced and
    /// keeps its id.
    pub fn save_path_preset(&mut self, name: &str, file_paths: Vec<String>, now: NaiveDateTime) {
        if let Some(preset) = self
            .path_presets
            .iter_mut()
            .find(|preset| same_name(&preset.name, name))
        {
            preset.name = name.to_owned();
            preset.file_paths = file_paths;
            preset.last_modified = now;
            return;
        }
        let id = next_id(&self.path_presets, |preset| preset.id);
        self.path_presets.push(PathPreset {
            id,
            name: name.to_owned(),
            file_paths,
            last_modified: now,
            extra: Map::new(),
        });
    }

    /// Renames a path preset. Another preset that already has the new name is removed.
    pub fn rename_path_preset(&mut self, id: i32, name: &str, now: NaiveDateTime) -> bool {
        if self.path_preset(id).is_none() {
            return false;
        }
        self.path_presets
            .retain(|preset| preset.id == id || !same_name(&preset.name, name));
        if let Some(preset) = self.path_presets.iter_mut().find(|preset| preset.id == id) {
            preset.name = name.to_owned();
            preset.last_modified = now;
        }
        true
    }

    pub fn delete_path_preset(&mut self, id: i32) {
        self.path_presets.retain(|preset| preset.id != id);
    }
}

#[derive(Debug)]
pub enum PresetStoreError {
    Io(io::Error),
    /// The file exists but is not valid. It is kept unchanged.
    Malformed(serde_json::Error),
}

impl std::fmt::Display for PresetStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Malformed(error) => write!(f, "The presets file is not valid: {error}"),
        }
    }
}

impl std::error::Error for PresetStoreError {}

impl From<io::Error> for PresetStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads and writes `presets.json` in the settings folder.
#[derive(Debug, Clone)]
pub struct PresetStore {
    path: PathBuf,
}

impl PresetStore {
    pub fn new(directory: &Path) -> Self {
        Self {
            path: directory.join(PRESETS_FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The stored presets, or none when the file does not exist yet.
    pub fn load(&self) -> Result<Presets, PresetStoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Presets::default()),
            Err(error) => return Err(error.into()),
        };
        let text = String::from_utf8_lossy(&bytes);
        serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(PresetStoreError::Malformed)
    }

    /// Writes the presets through a temporary file, so a failed write keeps the previous file.
    pub fn save(&self, presets: &Presets) -> Result<(), PresetStoreError> {
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }
        let json = serde_json::to_string_pretty(presets).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(&temporary, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(minute: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 1, 2)
            .unwrap()
            .and_hms_opt(3, minute, 0)
            .unwrap()
    }

    #[test]
    fn save_presets_get_ids_and_statistics() {
        let mut presets = Presets::default();
        let mut data = SaveData::default();
        data.file_content_export.selected_file_paths = vec!["a".to_owned(), "b".to_owned()];
        let first = presets.add_save_preset("One", data.clone(), time(1), "0.1.0");
        let second = presets.add_save_preset("Two", SaveData::default(), time(2), "0.1.0");
        assert_eq!((first, second), (1, 2));
        assert_eq!(presets.save_preset(1).unwrap().statistics.file_count, 2);
        assert!(presets.save_preset_name_exists("one", None));
        assert!(!presets.save_preset_name_exists("one", Some(1)));

        assert!(presets.rename_save_preset(2, "Renamed", time(5)));
        assert_eq!(presets.save_preset(2).unwrap().last_modified, time(5));
        assert!(presets.update_save_preset_data(1, SaveData::default(), time(6), "0.2.0"));
        let updated = presets.save_preset(1).unwrap();
        assert_eq!(updated.statistics.file_count, 0);
        assert_eq!(updated.created_at, time(1));
        assert_eq!(updated.app_version, "0.2.0");

        presets.delete_save_preset(1);
        assert_eq!(
            presets.add_save_preset("Three", SaveData::default(), time(7), ""),
            3
        );
    }

    #[test]
    fn path_presets_are_unique_by_name() {
        let mut presets = Presets::default();
        presets.save_path_preset("Docs", vec!["a.md".to_owned()], time(1));
        presets.save_path_preset("docs", vec!["b.md".to_owned()], time(2));
        assert_eq!(presets.path_presets.len(), 1);
        assert_eq!(presets.path_presets[0].name, "docs");
        assert_eq!(presets.path_presets[0].file_paths, ["b.md"]);

        presets.save_path_preset("Code", vec![], time(3));
        assert!(presets.rename_path_preset(2, "DOCS", time(4)));
        assert_eq!(presets.path_presets.len(), 1);
        assert_eq!(presets.path_presets[0].name, "DOCS");
        assert!(presets.rename_path_preset(2, "docs", time(5)));
        assert_eq!(presets.path_presets.len(), 1);
    }

    #[test]
    fn store_round_trips_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = PresetStore::new(dir.path());
        assert_eq!(store.load().unwrap(), Presets::default());

        let mut presets: Presets = serde_json::from_str(
            r#"{ "Future": 1, "PathPresets": [ { "Id": 4, "Name": "x", "Extra": true } ] }"#,
        )
        .unwrap();
        presets.add_save_preset("p", SaveData::default(), time(1), "0.1.0");
        store.save(&presets).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, presets);
        let json: Value = serde_json::from_str(&fs::read_to_string(store.path()).unwrap()).unwrap();
        assert_eq!(json["Future"], 1);
        assert_eq!(json["PathPresets"][0]["Extra"], true);
        assert_eq!(
            json["SavePresets"][0]["LastModified"],
            "2026-01-02T03:01:00"
        );
    }

    #[test]
    fn malformed_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(PRESETS_FILE_NAME), "{ broken").unwrap();
        assert!(matches!(
            PresetStore::new(dir.path()).load(),
            Err(PresetStoreError::Malformed(_))
        ));
    }
}
