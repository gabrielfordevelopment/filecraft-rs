//! Reads and writes `save.json` in the application data folder.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::settings::SaveData;

pub const SAVE_FILE_NAME: &str = "save.json";

/// Folder name of this application inside the platform configuration folder.
pub const APP_FOLDER_NAME: &str = "FileCraft-rs";

/// Where the saved state came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadSource {
    SaveFile,
    /// No save file existed.
    Defaults,
}

#[derive(Debug)]
pub enum SettingsError {
    /// The save file could not be parsed. It was renamed to `backup` so it is not overwritten.
    Malformed {
        backup: PathBuf,
        message: String,
    },
    Io(io::Error),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { backup, message } => write!(
                f,
                "The save file could not be read and was kept as {}.\n{message}",
                backup.display()
            ),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<io::Error> for SettingsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    directory: PathBuf,
}

impl SettingsStore {
    /// Uses the platform configuration folder, `%APPDATA%` on Windows.
    pub fn from_platform() -> io::Result<Self> {
        use etcetera::BaseStrategy;
        let strategy = etcetera::choose_base_strategy().map_err(io::Error::other)?;
        Ok(Self {
            directory: strategy.config_dir().join(APP_FOLDER_NAME),
        })
    }

    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn save_file(&self) -> PathBuf {
        self.directory.join(SAVE_FILE_NAME)
    }

    /// Loads the saved state. A missing file gives the defaults. A malformed file is renamed
    /// instead of being silently replaced.
    pub fn load(&self) -> Result<(SaveData, LoadSource), SettingsError> {
        let path = self.save_file();
        match fs::read(&path) {
            Ok(bytes) => match parse(&bytes) {
                Ok(data) => Ok((data, LoadSource::SaveFile)),
                Err(error) => {
                    let backup = self.backup_path();
                    fs::rename(&path, &backup)?;
                    Err(SettingsError::Malformed {
                        backup,
                        message: error.to_string(),
                    })
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok((SaveData::default(), LoadSource::Defaults))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Writes the save file through a temporary file, so a failed write keeps the previous file.
    pub fn save(&self, data: &SaveData) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        let json = serde_json::to_string_pretty(data).map_err(io::Error::other)?;
        let temporary = self.directory.join(format!("{SAVE_FILE_NAME}.tmp"));
        fs::write(&temporary, json)?;
        fs::rename(&temporary, self.save_file())
    }

    /// Deletes the save file. A missing file is not an error.
    pub fn delete(&self) -> io::Result<()> {
        match fs::remove_file(self.save_file()) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    fn backup_path(&self) -> PathBuf {
        let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        self.directory
            .join(format!("{SAVE_FILE_NAME}.invalid_{stamp}"))
    }
}

/// Parses save file bytes, accepting an optional UTF-8 BOM.
fn parse(bytes: &[u8]) -> serde_json::Result<SaveData> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &tempfile::TempDir) -> SettingsStore {
        SettingsStore::new(dir.path().join("app"))
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let (data, source) = store(&dir).load().unwrap();
        assert_eq!(data, SaveData::default());
        assert_eq!(source, LoadSource::Defaults);
    }

    #[test]
    fn saves_and_loads_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let data = SaveData {
            source_path: "C:\\project".to_owned(),
            ..SaveData::default()
        };
        store.save(&data).unwrap();
        let (loaded, source) = store.load().unwrap();
        assert_eq!(loaded, data);
        assert_eq!(source, LoadSource::SaveFile);
        assert!(!store.directory().join("save.json.tmp").exists());
    }

    #[test]
    fn reads_a_save_file_with_a_byte_order_mark() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        fs::create_dir_all(store.directory()).unwrap();
        fs::write(
            store.save_file(),
            b"\xEF\xBB\xBF{\r\n  \"SourcePath\": \"D:\\\\x\"\r\n}",
        )
        .unwrap();
        let (data, source) = store.load().unwrap();
        assert_eq!(data.source_path, "D:\\x");
        assert_eq!(source, LoadSource::SaveFile);
    }

    #[test]
    fn malformed_file_is_kept_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        fs::create_dir_all(store.directory()).unwrap();
        fs::write(store.save_file(), "{ not json").unwrap();
        let Err(SettingsError::Malformed { backup, .. }) = store.load() else {
            panic!("expected a malformed file error");
        };
        assert!(!store.save_file().exists());
        assert_eq!(fs::read_to_string(backup).unwrap(), "{ not json");
    }

    #[test]
    fn delete_ignores_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        store.delete().unwrap();
        store.save(&SaveData::default()).unwrap();
        store.delete().unwrap();
        assert!(!store.save_file().exists());
    }
}
