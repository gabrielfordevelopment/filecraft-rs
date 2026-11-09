//! Audio tags and properties for the folder export.

use std::path::Path;
use std::time::Duration;

use lofty::config::ParseOptions;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::Tag;

use crate::directory::display_name;
use crate::file_content::{extension_of, ignore_case_key};

/// Extensions that are read as audio files, compared ignoring case.
const SUPPORTED_EXTENSIONS: [&str; 15] = [
    ".aac", ".aif", ".aifc", ".aiff", ".flac", ".m4a", ".m4b", ".m4p", ".mp3", ".mp4", ".oga",
    ".ogg", ".opus", ".wav", ".wma",
];

pub const STATUS_NOT_AUDIO: &str = "Not audio";
pub const STATUS_OK: &str = "OK";
pub const STATUS_ERROR: &str = "Error";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub genre: String,
    pub track_number: String,
    pub duration: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMetadataReadResult {
    pub is_supported_audio_file: bool,
    pub success: bool,
    pub metadata: AudioMetadata,
    pub status: &'static str,
    pub error_message: String,
}

impl AudioMetadataReadResult {
    fn not_audio() -> Self {
        Self {
            is_supported_audio_file: false,
            success: false,
            metadata: AudioMetadata::default(),
            status: STATUS_NOT_AUDIO,
            error_message: String::new(),
        }
    }

    fn error(message: String) -> Self {
        Self {
            is_supported_audio_file: true,
            success: false,
            metadata: AudioMetadata::default(),
            status: STATUS_ERROR,
            error_message: message,
        }
    }
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    let extension = ignore_case_key(&extension_of(&display_name(path)));
    SUPPORTED_EXTENSIONS.contains(&extension.as_str())
}

/// Reads the tags and the duration of an audio file. Files that are not audio files are not
/// opened. A file without tags is read successfully with empty values.
pub fn read_audio_metadata(path: &Path) -> AudioMetadataReadResult {
    if !is_supported_audio_file(path) {
        return AudioMetadataReadResult::not_audio();
    }
    match read_tagged_file(path) {
        Ok(metadata) => AudioMetadataReadResult {
            is_supported_audio_file: true,
            success: true,
            metadata,
            status: STATUS_OK,
            error_message: String::new(),
        },
        Err(message) => AudioMetadataReadResult::error(message),
    }
}

fn read_tagged_file(path: &Path) -> Result<AudioMetadata, String> {
    let file = Probe::open(path)
        .map_err(|error| error_message(&error))?
        .options(ParseOptions::new().read_cover_art(false))
        .guess_file_type()
        .map_err(|error| error.to_string())?
        .read()
        .map_err(|error| error_message(&error))?;
    let duration = format_duration(file.properties().duration());
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Ok(AudioMetadata {
            duration,
            ..AudioMetadata::default()
        });
    };
    Ok(AudioMetadata {
        title: joined(tag, ItemKey::TrackTitle),
        artist: joined(tag, ItemKey::TrackArtist),
        album: joined(tag, ItemKey::AlbumTitle),
        album_artist: joined(tag, ItemKey::AlbumArtist),
        year: year(tag),
        genre: joined(tag, ItemKey::Genre),
        track_number: tag
            .track()
            .filter(|track| *track > 0)
            .map(|track| track.to_string())
            .unwrap_or_default(),
        duration,
    })
}

/// The year of the recording date, or of a plain year item that is not a full date.
fn year(tag: &Tag) -> String {
    if let Some(date) = tag.date() {
        return date.year.to_string();
    }
    tag.get_string(ItemKey::Year)
        .map(str::trim)
        .filter(|year| !year.is_empty() && year.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_owned)
        .unwrap_or_default()
}

/// All values of a tag item, trimmed and joined with ", " like the Shell's multi-value strings.
fn joined(tag: &Tag, key: ItemKey) -> String {
    tag.get_strings(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The error with its cause, since the parse error alone only names the file type.
fn error_message(error: &(dyn std::error::Error + 'static)) -> String {
    match error.source() {
        Some(source) => format!("{error}: {source}"),
        None => error.to_string(),
    }
}

/// Formats as `h:mm:ss` from one hour on, else as `m:ss`. Zero is empty.
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if duration.is_zero() {
        return String::new();
    }
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours >= 1 {
        format!("{}:{minutes:02}:{seconds:02}", hours % 24)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_audio_extensions_ignoring_case() {
        assert!(is_supported_audio_file(Path::new("song.MP3")));
        assert!(is_supported_audio_file(Path::new("a/b.flac")));
        assert!(!is_supported_audio_file(Path::new("notes.txt")));
        assert!(!is_supported_audio_file(Path::new("mp3")));
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::ZERO), "");
        assert_eq!(format_duration(Duration::from_millis(999)), "0:00");
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_secs(3599)), "59:59");
        assert_eq!(
            format_duration(Duration::from_secs(3600 + 2 * 60 + 3)),
            "1:02:03"
        );
    }

    #[test]
    fn non_audio_files_are_not_opened() {
        let result = read_audio_metadata(Path::new("missing.txt"));
        assert_eq!(result.status, STATUS_NOT_AUDIO);
        assert!(!result.is_supported_audio_file);
    }

    /// A silent 8 kHz mono 8-bit PCM WAV file of the given length.
    fn wav(seconds: u32) -> Vec<u8> {
        let data_len = 8000 * seconds;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(bytes.len() + data_len as usize, 0x80);
        bytes
    }

    #[test]
    fn reads_tags_and_duration() {
        use lofty::config::WriteOptions;
        use lofty::tag::TagType;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Song.WAV");
        std::fs::write(&path, wav(65)).unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title(" Title ".to_owned());
        tag.set_artist("Artist".to_owned());
        tag.set_album("Album".to_owned());
        tag.insert_text(ItemKey::AlbumArtist, "Album Artist".to_owned());
        tag.insert_text(ItemKey::RecordingDate, "2021-05-01".to_owned());
        tag.set_genre("Genre".to_owned());
        tag.set_track(3);
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        let result = read_audio_metadata(&path);
        assert_eq!(result.status, STATUS_OK, "{}", result.error_message);
        assert!(result.success);
        assert_eq!(
            result.metadata,
            AudioMetadata {
                title: "Title".to_owned(),
                artist: "Artist".to_owned(),
                album: "Album".to_owned(),
                album_artist: "Album Artist".to_owned(),
                year: "2021".to_owned(),
                genre: "Genre".to_owned(),
                track_number: "3".to_owned(),
                duration: "1:05".to_owned(),
            }
        );
    }

    #[test]
    fn untagged_audio_files_have_only_a_duration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.wav");
        std::fs::write(&path, wav(2)).unwrap();
        let result = read_audio_metadata(&path);
        assert_eq!(result.status, STATUS_OK);
        assert_eq!(result.metadata.title, "");
        assert_eq!(result.metadata.duration, "0:02");
    }

    #[test]
    fn unreadable_audio_files_report_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.mp3");
        std::fs::write(&path, b"not really audio").unwrap();
        let result = read_audio_metadata(&path);
        assert!(result.is_supported_audio_file);
        assert_eq!(result.status, STATUS_ERROR);
        assert!(!result.error_message.is_empty());
    }
}
