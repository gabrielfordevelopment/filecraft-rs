//! The log a File Rename run writes into the renamed folder. People can read it, and FileCraft
//! reads it to give the renamed files their original names back.
//!
//! ```text
//! FileCraft rename log
//! Format: 1
//! Mode: Rename by date
//! Folder: E:\Photos\2024 Summer
//! Started: 2026-11-09 14:30:12
//!
//! #001 RENAMED "IMG_4501.jpg" -> "2024_07_14_001.jpg" size=3481152 modified=2024-07-14T06:12:44.120Z sample=9f86d081884c7d65 date=EXIF
//! #002 FAILED "VID_0002.mp4" error="The process cannot access the file."
//! #003 SKIPPED "notes.txt" reason="this file type is not renamed"
//!
//! Finished: 2026-11-09 14:30:15
//! Totals: 1 renamed, 1 skipped, 1 failed
//! ```
//!
//! Names and text values are written in double quotes. A Windows file name never contains `"`,
//! `\` or a control character, so Windows names appear unchanged. Elsewhere these characters are
//! escaped with a backslash. Every line is written as soon as its file is done, so the log of an
//! interrupted run is complete up to the last file and only lacks the `Finished:` line. Readers
//! skip `key=value` fields they do not know, so fields can be added without a new format version.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path};

use chrono::{DateTime, Local, NaiveDateTime, Utc};

use crate::text::{LINE_ENDING, UTF8_BOM};

/// The first line of every log.
pub const HEADER: &str = "FileCraft rename log";
/// The format version this code writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// How far the recorded modification time may be from the current one and still count as the
/// same. FAT and exFAT drives store the time in steps of up to two seconds, so a copied folder
/// can differ by that much.
pub const MODIFIED_TOLERANCE_MS: i64 = 2000;

/// How many bytes from the start and from the end of a file go into its sample.
const SAMPLE_PART: u64 = 4 * 1024;

/// The start of the log names, each followed by the time of the run and `.txt`.
pub const BY_DATE_LOG_PREFIX: &str = "rename_log_";
pub const NUMBERING_LOG_PREFIX: &str = "numbering_log_";
pub const UNDO_LOG_PREFIX: &str = "undo_log_";

const EXPLANATION: &str = "One line per file, in the order FileCraft processed them. FileCraft can \
use this log to give the renamed files their original names back.\nSizes are in bytes, times are \
in UTC.";
const UNDO_EXPLANATION: &str = "One line per file, in the order FileCraft processed them. Sizes \
are in bytes, times are in UTC.";

/// What a run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMode {
    ByDate,
    Numbering,
    /// Gave the files of an earlier run their original names back.
    Undo,
}

impl LogMode {
    fn text(self) -> &'static str {
        match self {
            Self::ByDate => "Rename by date",
            Self::Numbering => "Simple numbering",
            Self::Undo => "Undo",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Self::ByDate, Self::Numbering, Self::Undo]
            .into_iter()
            .find(|mode| mode.text() == text)
    }
}

/// What identifies a file apart from its name. Renaming changes none of these, editing the file
/// changes the time and usually the size. The sample tells apart files that happen to have the same
/// size and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    pub size: u64,
    /// Milliseconds since 1970-01-01 UTC.
    pub modified_ms: i64,
    /// A hash of the first and the last 4 KiB of the file. Missing when the file could not be
    /// read.
    pub sample: Option<u64>,
}

impl Fingerprint {
    /// The fingerprint of the file at `path`, following links like the renaming does. `None` for
    /// anything but a file.
    pub fn of(path: &Path) -> io::Result<Option<Self>> {
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Ok(None);
        }
        Ok(Some(Self {
            size: metadata.len(),
            modified_ms: DateTime::<Utc>::from(metadata.modified()?).timestamp_millis(),
            // A file that cannot be read, such as one opened exclusively by another program, is
            // still told apart by its size and time.
            sample: sample(path, metadata.len()).ok(),
        }))
    }

    /// Whether `other` is most likely the same file: the same size, times at most
    /// [`MODIFIED_TOLERANCE_MS`] apart and the same sample when both have one.
    pub fn matches(&self, other: &Self) -> bool {
        self.size == other.size
            && self.modified_ms.abs_diff(other.modified_ms) <= MODIFIED_TOLERANCE_MS.unsigned_abs()
            && match (self.sample, other.sample) {
                (Some(own), Some(other)) => own == other,
                _ => true,
            }
    }

    fn modified_text(&self) -> String {
        DateTime::<Utc>::from_timestamp_millis(self.modified_ms).map_or_else(
            || self.modified_ms.to_string(),
            |time| time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        )
    }
}

/// The 64-bit FNV-1a hash of the first and the last [`SAMPLE_PART`] bytes of the file at `path`,
/// which is `size` bytes long. Small files are hashed whole. The hash is written into logs, so it
/// must never change.
fn sample(path: &Path, size: u64) -> io::Result<u64> {
    let mut file = File::open(path)?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut add = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    let mut buffer = Vec::new();
    (&mut file).take(SAMPLE_PART).read_to_end(&mut buffer)?;
    add(&buffer);
    if size > SAMPLE_PART {
        let tail = (size - SAMPLE_PART).min(SAMPLE_PART);
        file.seek(SeekFrom::Start(size - tail))?;
        buffer.clear();
        file.take(tail).read_to_end(&mut buffer)?;
        add(&buffer);
    }
    Ok(hash)
}

/// A time stamp for new log names that no file in `folder` uses yet with any of `prefixes`:
/// `2026-11-09_143012`, or `2026-11-09_143012_2` and so on when two runs start in the same second.
pub fn unique_stamp(folder: &Path, now: NaiveDateTime, prefixes: &[&str]) -> String {
    let base = now.format("%Y-%m-%d_%H%M%S").to_string();
    (1u32..)
        .map(|number| {
            if number == 1 {
                base.clone()
            } else {
                format!("{base}_{number}")
            }
        })
        .find(|stamp| {
            prefixes.iter().all(|prefix| {
                fs::symlink_metadata(folder.join(format!("{prefix}{stamp}.txt"))).is_err()
            })
        })
        .unwrap_or(base)
}

/// Creates `path`, failing instead of replacing a file that already exists.
pub fn create_new(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Writes a log line by line. Each line goes to the file at once.
pub struct LogWriter {
    file: File,
    step: usize,
    renamed: usize,
    skipped: usize,
    failed: usize,
}

impl LogWriter {
    /// Creates the log at `path` and writes its header. `undoes` names the log of the run an undo
    /// reverts.
    pub fn create(
        path: &Path,
        mode: LogMode,
        folder: &Path,
        started: NaiveDateTime,
        undoes: Option<&str>,
    ) -> io::Result<Self> {
        let mut writer = Self {
            file: create_new(path)?,
            step: 0,
            renamed: 0,
            skipped: 0,
            failed: 0,
        };
        let mut header = format!(
            "{HEADER}\nFormat: {FORMAT_VERSION}\nMode: {}\n",
            mode.text()
        );
        if let Some(undoes) = undoes {
            let _ = writeln!(header, "Undoes: {}", quote(undoes));
        }
        let _ = write!(
            header,
            "Folder: {}\nStarted: {}\n\n{}\n\n",
            plain(&folder.display().to_string()),
            started.format("%Y-%m-%d %H:%M:%S"),
            if mode == LogMode::Undo {
                UNDO_EXPLANATION
            } else {
                EXPLANATION
            },
        );
        writer.write(&header)?;
        Ok(writer)
    }

    /// A renamed file. `extra` holds further `key=value` fields, such as the date source.
    pub fn renamed(
        &mut self,
        from: &str,
        to: &str,
        fingerprint: Option<Fingerprint>,
        extra: &[(&str, &str)],
    ) -> io::Result<()> {
        let mut line = format!(
            "{} RENAMED {} -> {}",
            self.next_step(),
            quote(from),
            quote(to)
        );
        if let Some(fingerprint) = fingerprint {
            let _ = write!(
                line,
                " size={} modified={}",
                fingerprint.size,
                fingerprint.modified_text()
            );
            if let Some(sample) = fingerprint.sample {
                let _ = write!(line, " sample={sample:016x}");
            }
        }
        push_fields(&mut line, extra);
        self.renamed += 1;
        self.write_line(&line)
    }

    /// A file that keeps its name.
    pub fn skipped(&mut self, name: &str, reason: &str) -> io::Result<()> {
        let mut line = format!("{} SKIPPED {}", self.next_step(), quote(name));
        push_fields(&mut line, &[("reason", reason)]);
        self.skipped += 1;
        self.write_line(&line)
    }

    /// A file that could not be renamed.
    pub fn failed(&mut self, name: &str, error: &str) -> io::Result<()> {
        let mut line = format!("{} FAILED {}", self.next_step(), quote(name));
        push_fields(&mut line, &[("error", error)]);
        self.failed += 1;
        self.write_line(&line)
    }

    /// Ends the log of a run that went through, or of one that stopped because of `error`, and
    /// makes sure everything is on the disk.
    pub fn finish(&mut self, ended: NaiveDateTime, error: Option<&str>) -> io::Result<()> {
        let mut footer = String::from("\n");
        let time = ended.format("%Y-%m-%d %H:%M:%S");
        match error {
            None => {
                let _ = writeln!(footer, "Finished: {time}");
            }
            Some(error) => {
                let _ = writeln!(footer, "Stopped: {time}\nError: {}", quote(error));
            }
        }
        let _ = writeln!(
            footer,
            "Totals: {} renamed, {} skipped, {} failed",
            self.renamed, self.skipped, self.failed
        );
        self.write(&footer)?;
        self.file.sync_all()
    }

    /// Ends the log with the outcome of the run, see [`Self::finish`]. Returns the error that
    /// stopped the run, or the error of ending the log.
    pub fn end(mut self, result: io::Result<()>) -> io::Result<()> {
        let ended = Local::now().naive_local();
        match result {
            Ok(()) => self.finish(ended, None),
            Err(error) => match self.finish(ended, Some(&error.to_string())) {
                Ok(()) => Err(error),
                Err(log_error) => Err(io::Error::new(
                    error.kind(),
                    format!("{error} The log could not be completed either: {log_error}"),
                )),
            },
        }
    }

    fn next_step(&mut self) -> String {
        self.step += 1;
        format!("#{:03}", self.step)
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.write(&format!("{line}\n"))
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        self.file
            .write_all(text.replace('\n', LINE_ENDING).as_bytes())
    }
}

fn push_fields(line: &mut String, fields: &[(&str, &str)]) {
    for (key, value) in fields {
        let _ = write!(line, " {key}={}", field_value(value));
    }
}

/// A field value: bare when it has only letters, digits and `.:+-_`, otherwise quoted.
fn field_value(value: &str) -> String {
    let bare = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:+-_".contains(c));
    if bare { value.to_owned() } else { quote(value) }
}

/// `text` in double quotes, with `"`, `\` and control characters escaped.
pub fn quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(quoted, "\\u{{{:x}}}", u32::from(c));
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// `text` with control characters replaced, for the header lines that are only read by people.
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// One file line of a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEntry {
    Renamed {
        from: String,
        to: String,
        /// Missing when the size or the time could not be read during the run.
        fingerprint: Option<Fingerprint>,
    },
    Skipped {
        name: String,
    },
    Failed {
        name: String,
    },
}

/// A log read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLog {
    pub mode: LogMode,
    /// The run start as written, `YYYY-MM-DD HH:MM:SS`.
    pub started: String,
    /// The log name of the run an undo reverted.
    pub undoes: Option<String>,
    pub entries: Vec<LogEntry>,
    /// The run went through to the end.
    pub finished: bool,
}

impl ParsedLog {
    /// The renamed files in log order: their original and their new names and fingerprints.
    pub fn renamed(&self) -> impl DoubleEndedIterator<Item = (&str, &str, Option<Fingerprint>)> {
        self.entries.iter().filter_map(|entry| match entry {
            LogEntry::Renamed {
                from,
                to,
                fingerprint,
            } => Some((from.as_str(), to.as_str(), *fingerprint)),
            _ => None,
        })
    }
}

/// Whether the file at `path` starts like a log. Reads only the first bytes, so other text files
/// of the folder are cheap to rule out.
pub fn looks_like_log(path: &Path) -> io::Result<bool> {
    let mut start = [0u8; UTF8_BOM.len() + HEADER.len()];
    let mut file = File::open(path)?;
    let mut read = 0;
    while read < start.len() {
        match file.read(&mut start[read..])? {
            0 => break,
            count => read += count,
        }
    }
    let start = &start[..read];
    Ok(start
        .strip_prefix(UTF8_BOM)
        .unwrap_or(start)
        .starts_with(HEADER.as_bytes()))
}

/// Reads the log at `path`. Fails with [`io::ErrorKind::InvalidData`] when it is not a log of a
/// known format or when a file line is damaged, because acting on part of a log could leave files
/// half restored.
pub fn read_log(path: &Path) -> io::Result<ParsedLog> {
    let bytes = fs::read(path)?;
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(&bytes);
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid("the log is not valid UTF-8 text".to_owned()))?;
    parse_log(text)
}

/// Parses the text of a log, see [`read_log`].
pub fn parse_log(text: &str) -> io::Result<ParsedLog> {
    let mut lines = text.lines().enumerate();
    if lines.next().map(|(_, line)| line) != Some(HEADER) {
        return Err(invalid("this is not a FileCraft rename log".to_owned()));
    }
    let mut format = None;
    let mut mode = None;
    let mut started = String::new();
    let mut undoes = None;
    let mut entries = Vec::new();
    let mut finished = false;
    let mut in_header = true;
    for (index, line) in lines {
        let number = index + 1;
        if line.is_empty() {
            in_header = false;
            continue;
        }
        if in_header {
            let Some((key, value)) = line.split_once(": ") else {
                return Err(damaged(number));
            };
            match key {
                "Format" => {
                    let version: u32 = value.parse().map_err(|_| damaged(number))?;
                    if version > FORMAT_VERSION {
                        return Err(invalid(
                            "the log was written by a newer version of FileCraft".to_owned(),
                        ));
                    }
                    format = Some(version);
                }
                "Mode" => mode = Some(LogMode::parse(value).ok_or_else(|| damaged(number))?),
                "Started" => value.clone_into(&mut started),
                "Undoes" => {
                    let mut cursor = Cursor::new(value);
                    let name = cursor.quoted().ok_or_else(|| damaged(number))?;
                    if !cursor.at_end() {
                        return Err(damaged(number));
                    }
                    undoes = Some(name);
                }
                _ => {}
            }
            continue;
        }
        if line.starts_with('#') {
            entries.push(parse_entry(line).ok_or_else(|| damaged(number))?);
        } else if line.starts_with("Finished: ") {
            finished = true;
        }
    }
    if format.is_none() {
        return Err(invalid("the log has no format version".to_owned()));
    }
    let mode = mode.ok_or_else(|| invalid("the log does not say what the run did".to_owned()))?;
    Ok(ParsedLog {
        mode,
        started,
        undoes,
        entries,
        finished,
    })
}

/// One `#001 ACTION "name" ...` line.
fn parse_entry(line: &str) -> Option<LogEntry> {
    let mut cursor = Cursor::new(line.strip_prefix('#')?);
    let step = cursor.word()?;
    if step.is_empty() || !step.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let action = cursor.word()?;
    let name = cursor.quoted()?;
    if !is_plain_file_name(&name) {
        return None;
    }
    let entry = match action {
        "RENAMED" => {
            if cursor.word()? != "->" {
                return None;
            }
            let to = cursor.quoted()?;
            if !is_plain_file_name(&to) {
                return None;
            }
            let fields = cursor.fields()?;
            let field = |key: &str| {
                fields
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.as_str())
            };
            let sample = match field("sample") {
                Some(sample) if sample.len() == 16 => Some(u64::from_str_radix(sample, 16).ok()?),
                Some(_) => return None,
                None => None,
            };
            let fingerprint = match (field("size"), field("modified")) {
                (Some(size), Some(modified)) => Some(Fingerprint {
                    size: size.parse().ok()?,
                    modified_ms: DateTime::parse_from_rfc3339(modified)
                        .ok()?
                        .timestamp_millis(),
                    sample,
                }),
                (None, None) if sample.is_none() => None,
                _ => return None,
            };
            LogEntry::Renamed {
                from: name,
                to,
                fingerprint,
            }
        }
        "SKIPPED" => {
            cursor.fields()?;
            LogEntry::Skipped { name }
        }
        "FAILED" => {
            cursor.fields()?;
            LogEntry::Failed { name }
        }
        _ => return None,
    };
    Some(entry)
}

/// Whether `name` is a single file name, so acting on it can never reach outside the folder.
pub fn is_plain_file_name(name: &str) -> bool {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.chars().any(char::is_control) {
        return false;
    }
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(part)), None) if part == name
    )
}

/// Reads the parts of a file line.
struct Cursor<'a> {
    rest: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { rest: text }
    }

    fn at_end(&self) -> bool {
        self.rest.is_empty()
    }

    /// Drops the single space before the next part. Fails when there is none.
    fn space(&mut self) -> Option<()> {
        self.rest = self.rest.strip_prefix(' ')?;
        Some(())
    }

    /// The text up to the next space or the end, with the space dropped.
    fn word(&mut self) -> Option<&'a str> {
        let end = self.rest.find(' ').unwrap_or(self.rest.len());
        let (word, rest) = self.rest.split_at(end);
        self.rest = rest;
        if !self.rest.is_empty() {
            self.space()?;
        }
        Some(word)
    }

    /// A quoted string with its escapes resolved, with the space after it dropped.
    fn quoted(&mut self) -> Option<String> {
        let mut chars = self.rest.strip_prefix('"')?.char_indices();
        let mut text = String::new();
        let end = loop {
            let (index, c) = chars.next()?;
            match c {
                '"' => break index + 2,
                '\\' => match chars.next()?.1 {
                    '"' => text.push('"'),
                    '\\' => text.push('\\'),
                    'n' => text.push('\n'),
                    'r' => text.push('\r'),
                    't' => text.push('\t'),
                    'u' => {
                        if chars.next()?.1 != '{' {
                            return None;
                        }
                        let mut hex = String::new();
                        loop {
                            match chars.next()?.1 {
                                '}' => break,
                                digit if digit.is_ascii_hexdigit() && hex.len() < 6 => {
                                    hex.push(digit);
                                }
                                _ => return None,
                            }
                        }
                        text.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    _ => return None,
                },
                c => text.push(c),
            }
        };
        self.rest = &self.rest[end..];
        if !self.rest.is_empty() {
            self.space()?;
        }
        Some(text)
    }

    /// The `key=value` fields up to the end of the line.
    fn fields(&mut self) -> Option<Vec<(String, String)>> {
        let mut fields = Vec::new();
        while !self.at_end() {
            let equals = self.rest.find('=')?;
            let key = &self.rest[..equals];
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return None;
            }
            self.rest = &self.rest[equals + 1..];
            let value = if self.rest.starts_with('"') {
                self.quoted()?
            } else {
                let value = self.word()?;
                if value.is_empty() {
                    return None;
                }
                value.to_owned()
            };
            fields.push((key.to_owned(), value));
        }
        Some(fields)
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn damaged(line: usize) -> io::Error {
    invalid(format!("line {line} of the log is damaged"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn time(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 9)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    const PRINT: Fingerprint = Fingerprint {
        size: 3_481_152,
        modified_ms: 1_720_937_564_120,
        sample: Some(0x9f86_d081_884c_7d65),
    };

    #[test]
    fn writes_a_log_that_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rename_log_2026-11-09_143012.txt");
        let mut log = LogWriter::create(
            &path,
            LogMode::ByDate,
            Path::new(r"E:\Photos\2024 Summer"),
            time(14, 30, 12),
            None,
        )
        .unwrap();
        log.renamed(
            "Anna's \"best\" photo.jpg",
            "2024_07_14_001.jpg",
            Some(PRINT),
            &[("date", "Last modified")],
        )
        .unwrap();
        log.renamed(
            "IMG_4502.jpg",
            "2024_07_14_002.jpg",
            None,
            &[("date", "EXIF")],
        )
        .unwrap();
        log.failed("VID 1.mp4", "The file is in use.").unwrap();
        log.skipped("notes.txt", "this file type is not renamed")
            .unwrap();
        log.finish(time(14, 30, 15), None).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        let expected = "FileCraft rename log\nFormat: 1\nMode: Rename by date\n\
             Folder: E:\\Photos\\2024 Summer\nStarted: 2026-11-09 14:30:12\n\n\
             One line per file, in the order FileCraft processed them. FileCraft can use this log \
             to give the renamed files their original names back.\n\
             Sizes are in bytes, times are in UTC.\n\n\
             #001 RENAMED \"Anna's \\\"best\\\" photo.jpg\" -> \"2024_07_14_001.jpg\" size=3481152 \
             modified=2024-07-14T06:12:44.120Z sample=9f86d081884c7d65 date=\"Last modified\"\n\
             #002 RENAMED \"IMG_4502.jpg\" -> \"2024_07_14_002.jpg\" date=EXIF\n\
             #003 FAILED \"VID 1.mp4\" error=\"The file is in use.\"\n\
             #004 SKIPPED \"notes.txt\" reason=\"this file type is not renamed\"\n\n\
             Finished: 2026-11-09 14:30:15\nTotals: 2 renamed, 1 skipped, 1 failed\n"
            .replace('\n', LINE_ENDING);
        assert_eq!(text, expected);

        let log = read_log(&path).unwrap();
        assert_eq!(log.mode, LogMode::ByDate);
        assert_eq!(log.started, "2026-11-09 14:30:12");
        assert!(log.finished);
        assert_eq!(log.undoes, None);
        assert_eq!(
            log.renamed().collect::<Vec<_>>(),
            [
                (
                    "Anna's \"best\" photo.jpg",
                    "2024_07_14_001.jpg",
                    Some(PRINT)
                ),
                ("IMG_4502.jpg", "2024_07_14_002.jpg", None),
            ]
        );
        assert_eq!(log.entries.len(), 4);
        assert!(looks_like_log(&path).unwrap());
    }

    #[test]
    fn reads_undo_logs_and_stopped_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("undo.txt");
        let mut log = LogWriter::create(
            &path,
            LogMode::Undo,
            dir.path(),
            time(15, 0, 0),
            Some("rename_log_2026-11-09_143012.txt"),
        )
        .unwrap();
        log.renamed("b.jpg", "a.jpg", Some(PRINT), &[]).unwrap();
        log.finish(time(15, 0, 1), Some("Disk full")).unwrap();
        let log = read_log(&path).unwrap();
        assert_eq!(log.mode, LogMode::Undo);
        assert_eq!(
            log.undoes.as_deref(),
            Some("rename_log_2026-11-09_143012.txt")
        );
        assert!(!log.finished);
        assert_eq!(log.renamed().count(), 1);
    }

    #[test]
    fn escapes_names_that_only_other_systems_allow() {
        let name = "a\"b\\c\nd\u{1}e.jpg";
        let quoted = quote(name);
        assert_eq!(quoted, "\"a\\\"b\\\\c\\nd\\u{1}e.jpg\"");
        let mut cursor = Cursor::new(&quoted);
        assert_eq!(cursor.quoted().as_deref(), Some(name));
        assert!(cursor.at_end());
    }

    fn log_with(lines: &str) -> String {
        format!("{HEADER}\nFormat: 1\nMode: Simple numbering\nStarted: x\n\n{lines}\n")
    }

    #[test]
    fn refuses_damaged_or_unknown_logs() {
        let error = |text: &str| parse_log(text).unwrap_err().to_string();
        assert_eq!(
            error("[2026-11-09 14:30:12] --- Renaming by date started ---"),
            "this is not a FileCraft rename log"
        );
        assert_eq!(
            error(&format!("{HEADER}\nFormat: 2\nMode: Undo\n")),
            "the log was written by a newer version of FileCraft"
        );
        assert_eq!(
            error(&format!("{HEADER}\nMode: Undo\n")),
            "the log has no format version"
        );
        for line in [
            "#001 RENAMED \"a.jpg\" -> \"b.jpg",
            "#001 RENAMED \"a.jpg\" \"b.jpg\"",
            "#001 RENAMED \"a.jpg\" -> \"b.jpg\" size=1",
            "#001 RENAMED \"a.jpg\" -> \"b.jpg\" size=x modified=2024-07-14T06:12:44.120Z",
            "#001 RENAMED \"a.jpg\" -> \"b.jpg\" sample=0000000000000001",
            "#001 RENAMED \"a.jpg\" -> \"b.jpg\" size=1 modified=2024-07-14T06:12:44Z sample=1",
            "#001 RENAMED \"..\\\\a.jpg\" -> \"b.jpg\"",
            "#001 RENAMED \"a.jpg\" -> \"sub/b.jpg\"",
            "#001 RENAMED \"a.jpg\" -> \"..\"",
            "#001 RENAMED \"a.jpg\" -> \"C:b.jpg\"",
            "#001 RENAMED \"a.jpg\" -> \"\"",
            "#001 MOVED \"a.jpg\"",
            "#x RENAMED \"a.jpg\" -> \"b.jpg\"",
            "#001 SKIPPED \"a.jpg\" reason",
            "#001 SKIPPED \"a.jpg\"  reason=x",
            "#001 SKIPPED \"a.jpg\" reason=\"\\q\"",
        ] {
            assert_eq!(
                error(&log_with(line)),
                "line 6 of the log is damaged",
                "{line}"
            );
        }
        // Unknown fields and lines without a `#` are left alone.
        let log = parse_log(&log_with(
            "Some note.\n#001 RENAMED \"a.jpg\" -> \"b.jpg\" later=\"x y\" more=1",
        ))
        .unwrap();
        assert_eq!(
            log.renamed().collect::<Vec<_>>(),
            [("a.jpg", "b.jpg", None)]
        );
    }

    #[test]
    fn plain_file_names_stay_in_the_folder() {
        for name in ["a.jpg", "Anna's photo.jpg", ".hidden", "a..b"] {
            assert!(is_plain_file_name(name), "{name}");
        }
        for name in ["", ".", "..", "a/b", "a\\b", "C:a", "/a", "a\n"] {
            assert!(!is_plain_file_name(name), "{name}");
        }
    }

    #[test]
    fn fingerprints_allow_coarse_file_system_times() {
        let later = |ms| Fingerprint {
            modified_ms: PRINT.modified_ms + ms,
            ..PRINT
        };
        assert!(PRINT.matches(&later(2000)));
        assert!(PRINT.matches(&later(-2000)));
        assert!(!PRINT.matches(&later(2001)));
        assert!(!PRINT.matches(&Fingerprint { size: 1, ..PRINT }));
        assert!(!PRINT.matches(&Fingerprint {
            sample: Some(1),
            ..PRINT
        }));
        assert!(PRINT.matches(&Fingerprint {
            sample: None,
            ..PRINT
        }));
    }

    #[test]
    fn samples_the_start_and_the_end_of_files() {
        let dir = tempfile::tempdir().unwrap();
        let print = |content: &[u8]| {
            let path = dir.path().join("file");
            fs::write(&path, content).unwrap();
            Fingerprint::of(&path).unwrap().unwrap().sample.unwrap()
        };
        // FNV-1a of nothing and of "a".
        assert_eq!(print(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(print(b"a"), 0xaf63_dc4c_8601_ec8c);
        let big = vec![7u8; 20 * 1024];
        let base = print(&big);
        let mut start = big.clone();
        start[10] = 8;
        let mut end = big.clone();
        end[20 * 1024 - 10] = 8;
        let mut middle = big.clone();
        middle[10 * 1024] = 8;
        assert_ne!(print(&start), base);
        assert_ne!(print(&end), base);
        // The middle of a big file is not read.
        assert_eq!(print(&middle), base);
        assert_eq!(Fingerprint::of(dir.path()).unwrap(), None);
    }

    #[test]
    fn stamps_never_reuse_a_log_name() {
        let dir = tempfile::tempdir().unwrap();
        let now = time(14, 30, 12);
        let prefixes = [BY_DATE_LOG_PREFIX, "database_"];
        assert_eq!(
            unique_stamp(dir.path(), now, &prefixes),
            "2026-11-09_143012"
        );
        fs::write(dir.path().join("database_2026-11-09_143012.txt"), "").unwrap();
        assert_eq!(
            unique_stamp(dir.path(), now, &prefixes),
            "2026-11-09_143012_2"
        );
        fs::write(dir.path().join("rename_log_2026-11-09_143012_2.txt"), "").unwrap();
        assert_eq!(
            unique_stamp(dir.path(), now, &prefixes),
            "2026-11-09_143012_3"
        );
        let path = dir.path().join("database_2026-11-09_143012.txt");
        assert_eq!(
            create_new(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }
}
