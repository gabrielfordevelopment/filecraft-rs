//! Folder Content Export: one delimited line per file with the selected columns.

use std::fs::{self, File, Metadata};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::audio_metadata::{AudioMetadataReadResult, read_audio_metadata};
use crate::directory::{IgnoredFolderNames, display_name};
use crate::file_content::{compare_paths, extension_of};
use crate::file_name::output_path;
use crate::text::{LINE_ENDING, UTF8_BOM};

pub const FILE_NAME: &str = "file.name";
pub const FILE_SIZE_BYTES: &str = "file.sizeBytes";
pub const FILE_CREATION_TIME: &str = "file.creationTime";
pub const FILE_LAST_WRITE_TIME: &str = "file.lastWriteTime";
pub const FILE_LAST_ACCESS_TIME: &str = "file.lastAccessTime";
pub const FILE_IS_READ_ONLY: &str = "file.isReadOnly";
pub const FILE_ATTRIBUTES: &str = "file.attributes";
pub const FILE_FULL_PATH: &str = "file.fullPath";
pub const FILE_PARENT: &str = "file.parent";
pub const FILE_EXTENSION: &str = "file.extension";
pub const AUDIO_TITLE: &str = "audio.title";
pub const AUDIO_ARTIST: &str = "audio.artist";
pub const AUDIO_ALBUM: &str = "audio.album";
pub const AUDIO_ALBUM_ARTIST: &str = "audio.albumArtist";
pub const AUDIO_YEAR: &str = "audio.year";
pub const AUDIO_GENRE: &str = "audio.genre";
pub const AUDIO_TRACK_NUMBER: &str = "audio.trackNumber";
pub const AUDIO_DURATION: &str = "audio.duration";
pub const AUDIO_IS_AUDIO_FILE: &str = "audio.isAudioFile";
pub const AUDIO_METADATA_STATUS: &str = "audio.metadataStatus";
pub const AUDIO_METADATA_ERROR: &str = "audio.metadataError";

/// The field delimiter of the export file.
pub const DELIMITER: char = ';';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnGroup {
    FileDetails,
    AudioTags,
    AudioProperties,
    Diagnostics,
}

impl ColumnGroup {
    pub const ALL: [Self; 4] = [
        Self::FileDetails,
        Self::AudioTags,
        Self::AudioProperties,
        Self::Diagnostics,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnDefinition {
    /// Stable identifier stored in `SelectedColumns`.
    pub id: &'static str,
    /// Header text in the export file and the label in the column list.
    pub display_name: &'static str,
    pub group: ColumnGroup,
    pub default_selected: bool,
    pub requires_audio_metadata: bool,
}

const fn file(id: &'static str, display_name: &'static str) -> ColumnDefinition {
    ColumnDefinition {
        id,
        display_name,
        group: ColumnGroup::FileDetails,
        default_selected: true,
        requires_audio_metadata: false,
    }
}

const fn audio(
    id: &'static str,
    display_name: &'static str,
    group: ColumnGroup,
) -> ColumnDefinition {
    ColumnDefinition {
        id,
        display_name,
        group,
        default_selected: false,
        requires_audio_metadata: true,
    }
}

/// All columns in display and export order.
pub const COLUMNS: [ColumnDefinition; 21] = [
    file(FILE_NAME, "Name"),
    file(FILE_SIZE_BYTES, "Size (byte)"),
    file(FILE_CREATION_TIME, "Creation time"),
    file(FILE_LAST_WRITE_TIME, "Last write time"),
    file(FILE_LAST_ACCESS_TIME, "Last access time"),
    file(FILE_IS_READ_ONLY, "Is read-only"),
    file(FILE_ATTRIBUTES, "Attributes"),
    file(FILE_FULL_PATH, "Full path"),
    file(FILE_PARENT, "Parent"),
    file(FILE_EXTENSION, "Format"),
    audio(AUDIO_TITLE, "Audio title", ColumnGroup::AudioTags),
    audio(AUDIO_ARTIST, "Audio artist", ColumnGroup::AudioTags),
    audio(AUDIO_ALBUM, "Album", ColumnGroup::AudioTags),
    audio(AUDIO_ALBUM_ARTIST, "Album artist", ColumnGroup::AudioTags),
    audio(AUDIO_YEAR, "Year", ColumnGroup::AudioTags),
    audio(AUDIO_GENRE, "Genre", ColumnGroup::AudioTags),
    audio(AUDIO_TRACK_NUMBER, "Track number", ColumnGroup::AudioTags),
    audio(AUDIO_DURATION, "Duration", ColumnGroup::AudioProperties),
    audio(
        AUDIO_IS_AUDIO_FILE,
        "Is audio file",
        ColumnGroup::Diagnostics,
    ),
    audio(
        AUDIO_METADATA_STATUS,
        "Metadata status",
        ColumnGroup::Diagnostics,
    ),
    audio(
        AUDIO_METADATA_ERROR,
        "Metadata error",
        ColumnGroup::Diagnostics,
    ),
];

pub fn column(id: &str) -> Option<&'static ColumnDefinition> {
    COLUMNS.iter().find(|column| column.id == id)
}

/// Formats one field: values that a spreadsheet could read
/// as a formula get a leading `'` when protection is on, and values with the delimiter, a quote or
/// a line break are quoted with doubled quotes.
pub fn format_field(value: &str, protect_spreadsheet_formulas: bool) -> String {
    let mut value = value.to_owned();
    if protect_spreadsheet_formulas
        && value
            .chars()
            .next()
            .is_some_and(|first| matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r' | '\n'))
    {
        value.insert(0, '\'');
    }
    let must_quote = value.contains(DELIMITER)
        || value.contains('"')
        || value.contains('\r')
        || value.contains('\n');
    if must_quote {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// Lists every file below the folders, skipping ignored folder names at every level including the
/// folders themselves. Unreadable folders are skipped. The order is not defined.
pub fn collect_all_files(folders: &[PathBuf], ignored: &IgnoredFolderNames) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for folder in folders {
        if !folder.is_dir() {
            continue;
        }
        let mut stack = vec![folder.clone()];
        while let Some(directory) = stack.pop() {
            if ignored.contains(&directory) {
                continue;
            }
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push(path);
                }
            }
        }
    }
    files
}

#[derive(Debug, Clone)]
pub struct FolderExportRequest {
    pub destination_path: PathBuf,
    /// Output file name without extension.
    pub file_name: String,
    /// The selected folders, each exported with all of its subfolders.
    pub folders: Vec<PathBuf>,
    /// Selected column identifiers. Unknown identifiers are skipped.
    pub column_ids: Vec<String>,
    pub protect_spreadsheet_formulas: bool,
    pub ignored: IgnoredFolderNames,
}

/// Writes the header line and one line per file, ordered by full path, and returns the output path.
pub fn export_folder_contents(request: &FolderExportRequest) -> io::Result<PathBuf> {
    if request.folders.is_empty() {
        return Err(io::Error::other("No folders were selected to export from."));
    }
    let columns: Vec<&ColumnDefinition> = request
        .column_ids
        .iter()
        .filter_map(|id| column(id))
        .collect();
    if columns.is_empty() {
        return Err(io::Error::other(
            "No known columns were selected for export.",
        ));
    }
    let mut files = collect_all_files(&request.folders, &request.ignored);
    if files.is_empty() {
        return Err(io::Error::other(
            "The selected folders contain no files to export.",
        ));
    }
    files.sort_by(|a, b| compare_paths(&a.to_string_lossy(), &b.to_string_lossy()));

    let output_path = output_path(&request.destination_path, &request.file_name);
    let mut writer = BufWriter::new(File::create(&output_path)?);
    writer.write_all(UTF8_BOM)?;
    let protect = request.protect_spreadsheet_formulas;
    write_record(
        &mut writer,
        columns.iter().map(|column| column.display_name.to_owned()),
        protect,
    )?;

    let needs_audio = columns.iter().any(|column| column.requires_audio_metadata);
    for path in &files {
        let row = Row {
            path,
            metadata: fs::metadata(path)?,
            audio: needs_audio.then(|| read_audio_metadata(path)),
        };
        write_record(
            &mut writer,
            columns.iter().map(|column| row.value(column.id)),
            protect,
        )?;
    }
    writer.flush()?;
    Ok(output_path)
}

fn write_record(
    writer: &mut impl Write,
    fields: impl Iterator<Item = String>,
    protect: bool,
) -> io::Result<()> {
    let line = fields
        .map(|field| format_field(&field, protect))
        .collect::<Vec<_>>()
        .join(&DELIMITER.to_string());
    writer.write_all(line.as_bytes())?;
    writer.write_all(LINE_ENDING.as_bytes())
}

struct Row<'a> {
    path: &'a Path,
    metadata: Metadata,
    audio: Option<AudioMetadataReadResult>,
}

impl Row<'_> {
    fn value(&self, column_id: &str) -> String {
        let audio = self.audio.as_ref();
        let tag = |read: fn(&AudioMetadataReadResult) -> &String| {
            audio.map(|audio| read(audio).clone()).unwrap_or_default()
        };
        match column_id {
            FILE_NAME => display_name(self.path),
            FILE_SIZE_BYTES => self.metadata.len().to_string(),
            FILE_CREATION_TIME => format_time(self.metadata.created().or(self.metadata.modified())),
            FILE_LAST_WRITE_TIME => format_time(self.metadata.modified()),
            FILE_LAST_ACCESS_TIME => format_time(self.metadata.accessed()),
            FILE_IS_READ_ONLY => bool_text(attributes::is_read_only(&self.metadata)),
            FILE_ATTRIBUTES => attributes::describe(self.path, &self.metadata),
            FILE_FULL_PATH => self.path.to_string_lossy().into_owned(),
            FILE_PARENT => self.path.parent().map(display_name).unwrap_or_default(),
            FILE_EXTENSION => extension_of(&display_name(self.path)),
            AUDIO_TITLE => tag(|audio| &audio.metadata.title),
            AUDIO_ARTIST => tag(|audio| &audio.metadata.artist),
            AUDIO_ALBUM => tag(|audio| &audio.metadata.album),
            AUDIO_ALBUM_ARTIST => tag(|audio| &audio.metadata.album_artist),
            AUDIO_YEAR => tag(|audio| &audio.metadata.year),
            AUDIO_GENRE => tag(|audio| &audio.metadata.genre),
            AUDIO_TRACK_NUMBER => tag(|audio| &audio.metadata.track_number),
            AUDIO_DURATION => tag(|audio| &audio.metadata.duration),
            AUDIO_IS_AUDIO_FILE => {
                bool_text(audio.is_some_and(|audio| audio.is_supported_audio_file))
            }
            AUDIO_METADATA_STATUS => audio
                .map(|audio| audio.status.to_owned())
                .unwrap_or_default(),
            AUDIO_METADATA_ERROR => tag(|audio| &audio.error_message),
            _ => String::new(),
        }
    }
}

/// `True` or `False`.
fn bool_text(value: bool) -> String {
    if value { "True" } else { "False" }.to_owned()
}

/// Local time as `yyyy-MM-dd HH:mm:ss`. Times the platform cannot provide are empty.
fn format_time(time: io::Result<SystemTime>) -> String {
    time.map(|time| {
        chrono::DateTime::<chrono::Local>::from(time)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
    .unwrap_or_default()
}

#[cfg(windows)]
mod attributes {
    use std::fs::Metadata;
    use std::os::windows::fs::MetadataExt;
    use std::path::Path;

    /// Windows file attribute names with their values, in ascending order.
    const NAMES: [(u32, &str); 16] = [
        (0x1, "ReadOnly"),
        (0x2, "Hidden"),
        (0x4, "System"),
        (0x10, "Directory"),
        (0x20, "Archive"),
        (0x40, "Device"),
        (0x80, "Normal"),
        (0x100, "Temporary"),
        (0x200, "SparseFile"),
        (0x400, "ReparsePoint"),
        (0x800, "Compressed"),
        (0x1000, "Offline"),
        (0x2000, "NotContentIndexed"),
        (0x4000, "Encrypted"),
        (0x8000, "IntegrityStream"),
        (0x20000, "NoScrubData"),
    ];

    pub fn is_read_only(metadata: &Metadata) -> bool {
        metadata.file_attributes() & 0x1 != 0
    }

    pub fn describe(_path: &Path, metadata: &Metadata) -> String {
        format_flags(metadata.file_attributes())
    }

    /// The flag names joined with ", ", or the number when a bit has no name.
    pub fn format_flags(value: u32) -> String {
        let known = NAMES.iter().fold(0, |bits, (flag, _)| bits | flag);
        if value == 0 || value & !known != 0 {
            return value.to_string();
        }
        NAMES
            .iter()
            .filter(|(flag, _)| value & flag != 0)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Attributes on other platforms: read-only without write permission, hidden
/// for names starting with a dot, otherwise normal.
#[cfg(not(windows))]
mod attributes {
    use std::fs::Metadata;
    use std::path::Path;

    use crate::directory::display_name;

    pub fn is_read_only(metadata: &Metadata) -> bool {
        metadata.permissions().readonly()
    }

    pub fn describe(path: &Path, metadata: &Metadata) -> String {
        let mut names = Vec::new();
        if is_read_only(metadata) {
            names.push("ReadOnly");
        }
        if display_name(path).starts_with('.') {
            names.push("Hidden");
        }
        if names.is_empty() {
            names.push("Normal");
        }
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_fields_like_delimited_text_writer() {
        assert_eq!(format_field("plain", false), "plain");
        assert_eq!(format_field("a;b", false), "\"a;b\"");
        assert_eq!(format_field("say \"hi\"", false), "\"say \"\"hi\"\"\"");
        assert_eq!(format_field("two\nlines", false), "\"two\nlines\"");
        assert_eq!(format_field("=SUM(A1)", false), "=SUM(A1)");
        assert_eq!(format_field("=SUM(A1)", true), "'=SUM(A1)");
        assert_eq!(format_field("-1", true), "'-1");
        assert_eq!(format_field("=a;b", true), "\"'=a;b\"");
        assert_eq!(format_field("", true), "");
    }

    #[test]
    fn columns_have_unique_ids_and_known_defaults() {
        let defaults: Vec<&str> = COLUMNS
            .iter()
            .filter(|column| column.default_selected)
            .map(|column| column.id)
            .collect();
        assert_eq!(defaults, crate::settings::DEFAULT_FOLDER_EXPORT_COLUMNS);
        for (index, column) in COLUMNS.iter().enumerate() {
            assert!(
                COLUMNS[index + 1..]
                    .iter()
                    .all(|other| other.id != column.id)
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn formats_attribute_flags() {
        assert_eq!(attributes::format_flags(0x20), "Archive");
        assert_eq!(attributes::format_flags(0x21), "ReadOnly, Archive");
        assert_eq!(attributes::format_flags(0x80), "Normal");
        assert_eq!(attributes::format_flags(0x400020), "4194336");
        assert_eq!(attributes::format_flags(0), "0");
    }

    #[test]
    fn collects_files_below_selected_folders_skipping_ignored_ones() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/bin")).unwrap();
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        fs::write(dir.path().join("root.txt"), "").unwrap();
        fs::write(dir.path().join("a/one.txt"), "").unwrap();
        fs::write(dir.path().join("a/bin/skip.dll"), "").unwrap();
        fs::write(dir.path().join("bin/skip.dll"), "").unwrap();
        let ignored = IgnoredFolderNames::new(["bin"]);
        let mut files = collect_all_files(&[dir.path().join("a")], &ignored);
        files.sort();
        assert_eq!(files, [dir.path().join("a").join("one.txt")]);
        assert!(collect_all_files(&[dir.path().join("bin")], &ignored).is_empty());
        assert_eq!(
            collect_all_files(&[dir.path().to_path_buf()], &ignored).len(),
            2
        );
    }

    #[test]
    fn exports_header_and_sorted_rows() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        fs::create_dir_all(source.join("sub")).unwrap();
        fs::write(source.join("b;c.txt"), "12345").unwrap();
        fs::write(source.join("A.md"), "").unwrap();
        fs::write(source.join("sub").join("song.mp3"), "x").unwrap();
        let output = export_folder_contents(&FolderExportRequest {
            destination_path: dir.path().to_path_buf(),
            file_name: "out".to_owned(),
            folders: vec![source.clone()],
            column_ids: vec![
                FILE_NAME.to_owned(),
                "unknown".to_owned(),
                FILE_SIZE_BYTES.to_owned(),
                FILE_PARENT.to_owned(),
                FILE_EXTENSION.to_owned(),
                AUDIO_IS_AUDIO_FILE.to_owned(),
                AUDIO_METADATA_STATUS.to_owned(),
            ],
            protect_spreadsheet_formulas: false,
            ignored: IgnoredFolderNames::default(),
        })
        .unwrap();
        assert_eq!(output, dir.path().join("out.txt"));
        let bytes = fs::read(&output).unwrap();
        assert!(bytes.starts_with(UTF8_BOM));
        let text = String::from_utf8(bytes[UTF8_BOM.len()..].to_vec()).unwrap();
        let lines: Vec<&str> = text.split(LINE_ENDING).collect();
        assert_eq!(
            lines,
            [
                "Name;Size (byte);Parent;Format;Is audio file;Metadata status",
                "A.md;0;src;.md;False;Not audio",
                "\"b;c.txt\";5;src;.txt;False;Not audio",
                "song.mp3;1;sub;.mp3;True;Error",
                "",
            ]
        );
    }

    #[test]
    fn rejects_empty_selections() {
        let dir = tempfile::tempdir().unwrap();
        let request = FolderExportRequest {
            destination_path: dir.path().to_path_buf(),
            file_name: "out".to_owned(),
            folders: vec![dir.path().to_path_buf()],
            column_ids: vec![FILE_NAME.to_owned()],
            protect_spreadsheet_formulas: false,
            ignored: IgnoredFolderNames::default(),
        };
        let error = export_folder_contents(&request).unwrap_err();
        assert_eq!(
            error.to_string(),
            "The selected folders contain no files to export."
        );
        let no_columns = FolderExportRequest {
            column_ids: vec!["unknown".to_owned()],
            ..request
        };
        assert!(export_folder_contents(&no_columns).is_err());
    }
}
