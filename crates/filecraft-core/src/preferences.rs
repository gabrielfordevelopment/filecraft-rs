//! Application preferences that are not part of the saved state, such as the color theme. They are
//! stored in `preferences.json` in the settings folder and written as soon as they change, so undo,
//! presets and "Delete current save" leave them alone.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PREFERENCES_FILE_NAME: &str = "preferences.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Preferences {
    /// The chosen theme. `None` follows the operating system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ColorTheme>,
    /// Fields written by newer versions, kept unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct PreferencesStore {
    path: PathBuf,
}

impl PreferencesStore {
    pub fn new(directory: &Path) -> Self {
        Self {
            path: directory.join(PREFERENCES_FILE_NAME),
        }
    }

    /// Reads the preferences. A missing file gives the defaults, a malformed file is an error and
    /// is left as it is.
    pub fn load(&self) -> io::Result<Preferences> {
        match fs::read(&self.path) {
            Ok(bytes) => {
                let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
                serde_json::from_slice(bytes).map_err(io::Error::other)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Preferences::default()),
            Err(error) => Err(error),
        }
    }

    /// Writes the preferences through a temporary file, so a failed write keeps the previous file.
    pub fn save(&self, preferences: &Preferences) -> io::Result<()> {
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }
        let json = serde_json::to_string_pretty(preferences).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = PreferencesStore::new(dir.path());
        assert_eq!(store.load().unwrap(), Preferences::default());
    }

    #[test]
    fn round_trips_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(PREFERENCES_FILE_NAME),
            "\u{feff}{\"Theme\":\"Dark\",\"Future\":1}",
        )
        .unwrap();
        let store = PreferencesStore::new(dir.path());
        let mut preferences = store.load().unwrap();
        assert_eq!(preferences.theme, Some(ColorTheme::Dark));
        preferences.theme = Some(ColorTheme::Light);
        store.save(&preferences).unwrap();
        let saved = store.load().unwrap();
        assert_eq!(saved.theme, Some(ColorTheme::Light));
        assert_eq!(saved.extra.get("Future"), Some(&Value::from(1)));
    }

    #[test]
    fn malformed_file_is_an_error_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PREFERENCES_FILE_NAME);
        fs::write(&path, "{").unwrap();
        assert!(PreferencesStore::new(dir.path()).load().is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "{");
    }
}
