//! Renames the files of one folder by date or by number. A renaming by date also writes a report.
//! Every run writes its own log, which `rename_undo` can use to give the files their original names
//! back, see `rename_log`.
//!
//! Only the files directly in the folder are renamed, in the order of their names.
//! Existing files are never replaced. [`Preview`] runs the same steps on a simulated folder, so it
//! shows exactly what [`rename_files`] will do as long as the folder does not change in between.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime};

use crate::exif::exif_date;
use crate::path_key::name_key;
use crate::rename_log::{
    BY_DATE_LOG_PREFIX, Fingerprint, LogMode, LogWriter, NUMBERING_LOG_PREFIX, create_new,
    unique_stamp,
};
use crate::rename_undo::revert_unlogged;

/// Extensions of the photos and videos that are renamed by date, in lower case.
pub const SUPPORTED_EXTENSIONS: [&str; 10] = [
    ".jpg", ".jpeg", ".tiff", ".png", ".bmp", ".webp", ".mp4", ".mov", ".avi", ".mkv",
];

/// Years before this are not accepted as dates in file names.
pub const MIN_VALID_YEAR: i32 = 1950;

/// How the files are renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameMode {
    /// Photos and videos get `YYYY_MM_DD_NNN.ext` names.
    ByDate,
    /// Every file except `.txt` files gets `00001.ext` and so on.
    Numbering,
}

/// Why a file is not renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Only photos and videos are renamed by date.
    UnsupportedType,
    /// Text files are not numbered.
    TextFile,
    /// The new name is taken by another file.
    NameTaken(String),
    /// The file already has its new name.
    AlreadyNamed,
    /// Undo: the file already has its original name.
    AlreadyRestored,
    /// Undo: the file was changed after the renaming.
    Changed,
    /// Undo: the renamed file is not in the folder any more.
    Missing,
    /// Undo: the log has no size and time for the file, so it cannot be checked.
    NotVerifiable,
}

/// What happens, or happened, to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Renamed {
        new_name: String,
        source: Option<DateSource>,
        /// Details when the dates of the file name and the metadata differ.
        mismatch: Option<String>,
    },
    Skipped {
        source: Option<DateSource>,
        reason: SkipReason,
    },
    Failed {
        error: String,
    },
}

/// One file of the folder and its outcome, in processing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub name: String,
    pub outcome: Outcome,
}

/// Where the date of a file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateSource {
    Exif,
    FileName,
    LastModified,
}

impl DateSource {
    /// The value of the report's first column.
    fn report_name(self) -> &'static str {
        match self {
            Self::Exif => "EXIF",
            Self::FileName => "File name",
            Self::LastModified => "Last modified",
        }
    }

    /// How the date source is named in the log.
    fn description(self) -> &'static str {
        match self {
            Self::Exif => "date from the photo metadata",
            Self::FileName => "date from the file name",
            Self::LastModified => "date the file was last modified",
        }
    }
}

/// A date in the file name: `YYYY-MM-DD` with `-`, `_` or `/` between the parts, otherwise eight
/// digits in a row. Only the first match of each form is tried, and only years from
/// [`MIN_VALID_YEAR`] to `current_year` are accepted.
pub fn date_from_file_name(name: &str, current_year: i32) -> Option<NaiveDate> {
    let chars: Vec<char> = name.chars().collect();
    // The value of `count` decimal digits in a row, in any script.
    let digits = |start: usize, count: usize| -> Option<u32> {
        chars
            .get(start..start + count)?
            .iter()
            .try_fold(0u32, |value, c| Some(value * 10 + decimal_value(*c)?))
    };
    let separator = |index: usize| matches!(chars.get(index), Some('-' | '_' | '/'));
    let date = |year: u32, month: u32, day: u32| {
        let year = i32::try_from(year).ok()?;
        if !(MIN_VALID_YEAR..=current_year).contains(&year) {
            return None;
        }
        NaiveDate::from_ymd_opt(year, month, day)
    };

    let separated = (0..chars.len()).find_map(|start| {
        let year = digits(start, 4)?;
        (separator(start + 4) && separator(start + 7))
            .then_some(())
            .and_then(|()| Some((year, digits(start + 5, 2)?, digits(start + 8, 2)?)))
    });
    if let Some((year, month, day)) = separated
        && let Some(found) = date(year, month, day)
    {
        return Some(found);
    }
    let compact = (0..chars.len()).find_map(|start| {
        Some((
            digits(start, 4)?,
            digits(start + 4, 2)?,
            digits(start + 6, 2)?,
        ))
    })?;
    date(compact.0, compact.1, compact.2)
}

/// The first code point of every block of ten Unicode decimal digits (category `Nd`), as of
/// Unicode 16. A date in a file name may be written with the digits of any of these blocks.
const DECIMAL_ZEROS: [u32; 76] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x10D40, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
    0x11650, 0x116C0, 0x116D0, 0x116DA, 0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50,
    0x11DA0, 0x11F50, 0x16130, 0x16A60, 0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8,
    0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// The value of a Unicode decimal digit, such as `7` for `'7'` or `'\u{0667}'`.
fn decimal_value(c: char) -> Option<u32> {
    let code = u32::from(c);
    let index = DECIMAL_ZEROS
        .partition_point(|zero| *zero <= code)
        .checked_sub(1)?;
    let value = code - DECIMAL_ZEROS[index];
    (value < 10).then_some(value)
}

/// The extension of a file name with its dot. Leading dots do not start an extension.
pub fn extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if name[..dot].chars().any(|c| c != '.') => &name[dot..],
        _ => "",
    }
}

/// An entry of the folder: its name, its path and whether it is a file.
struct Entry {
    name: String,
    path: PathBuf,
    is_file: bool,
}

/// The entries directly in `folder`, sorted by name. Names are compared by Unicode code points, so
/// upper case letters come before lower case ones.
fn sorted_entries(folder: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        // The listing already knows the type, so only links need a look at their target.
        let file_type = entry.file_type()?;
        let is_file = if file_type.is_symlink() {
            path.is_file()
        } else {
            file_type.is_file()
        };
        entries.push(Entry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_file,
            path,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// The modification time of a file in local time.
fn modified(path: &Path) -> io::Result<NaiveDateTime> {
    let time = fs::metadata(path)?.modified()?;
    Ok(DateTime::<Local>::from(time).naive_local())
}

/// The folder the steps work on: the real one, or a set of names for the preview.
trait Folder {
    fn exists(&self, name: &str) -> bool;
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
}

struct RealFolder<'a>(&'a Path);

impl Folder for RealFolder<'_> {
    /// A name counts as taken unless the file system says it is free, so a name that cannot be
    /// checked or a broken link is never replaced.
    fn exists(&self, name: &str) -> bool {
        !matches!(
            fs::symlink_metadata(self.0.join(name)),
            Err(error) if error.kind() == io::ErrorKind::NotFound
        )
    }

    /// The target is checked again right before renaming, because on Windows a plain rename would
    /// replace it.
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        if self.exists(to) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a file with the new name already exists",
            ));
        }
        fs::rename(self.0.join(from), self.0.join(to))
    }
}

/// The names of a folder, compared like the file system compares them.
struct SimulatedFolder(HashSet<String>);

impl Folder for SimulatedFolder {
    fn exists(&self, name: &str) -> bool {
        self.0.contains(&name_key(name))
    }

    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        self.0.remove(&name_key(from));
        self.0.insert(name_key(to));
        Ok(())
    }
}

/// The renaming state carried from one file to the next. The preview and the run share it, so
/// they make the same decisions.
struct Steps {
    mode: RenameMode,
    now: NaiveDateTime,
    /// "Rename by Date": the last number given on each day.
    day_counters: HashMap<String, u32>,
    /// "Simple Numbering": the next number.
    next_number: u32,
}

impl Steps {
    fn new(mode: RenameMode, now: NaiveDateTime) -> Self {
        Self {
            mode,
            now,
            day_counters: HashMap::new(),
            next_number: 1,
        }
    }

    /// The outcome of one entry, or `None` for folders and for the names in `skip`, which are not
    /// reported.
    fn step(
        &mut self,
        entry: &Entry,
        skip: &[&str],
        folder: &mut dyn Folder,
    ) -> Option<FileResult> {
        if skip.contains(&entry.name.as_str()) || !entry.is_file {
            return None;
        }
        let outcome = match self.mode {
            RenameMode::ByDate => self.by_date(entry, folder),
            RenameMode::Numbering => self.numbering(entry, folder),
        };
        Some(FileResult {
            name: entry.name.clone(),
            outcome,
        })
    }

    /// One file of "Rename by Date".
    fn by_date(&mut self, entry: &Entry, folder: &mut dyn Folder) -> Outcome {
        let name = &entry.name;
        let file_extension = extension(name).to_lowercase();
        if !SUPPORTED_EXTENSIONS.contains(&file_extension.as_str()) {
            return Outcome::Skipped {
                source: None,
                reason: SkipReason::UnsupportedType,
            };
        }

        let outcome = (|| -> io::Result<Outcome> {
            let metadata_date = exif_date(&entry.path);
            let (date, source) = if let Some(date) = metadata_date {
                (date, DateSource::Exif)
            } else if let Some(date) = date_from_file_name(name, self.now.year()) {
                (date, DateSource::FileName)
            } else {
                (modified(&entry.path)?.date(), DateSource::LastModified)
            };
            let date_text = date.format("%Y_%m_%d").to_string();

            // A mismatch is only looked for when the date came from the file name, and it also
            // needs a metadata date that differs from it.
            let mut mismatch = None;
            if source == DateSource::FileName {
                let modified_date = modified(&entry.path)?.date();
                if let Some(metadata_date) = metadata_date
                    && date != metadata_date
                    && date != modified_date
                {
                    mismatch = Some(format!(
                        "'{name}' (file name: {date_text}, metadata: {}, last modified: {})",
                        metadata_date.format("%Y_%m_%d"),
                        modified_date.format("%Y_%m_%d"),
                    ));
                }
            }

            let counter = self.day_counters.entry(date_text.clone()).or_insert(0);
            *counter += 1;
            let new_name = format!("{date_text}_{:03}{file_extension}", *counter);
            if folder.exists(&new_name) || *name == new_name {
                let reason = if folder.exists(&new_name) {
                    SkipReason::NameTaken(new_name)
                } else {
                    SkipReason::AlreadyNamed
                };
                return Ok(Outcome::Skipped {
                    source: Some(source),
                    reason,
                });
            }
            folder.rename(name, &new_name)?;
            Ok(Outcome::Renamed {
                new_name,
                source: Some(source),
                mismatch,
            })
        })();
        outcome.unwrap_or_else(|error| Outcome::Failed {
            error: error.to_string(),
        })
    }

    /// One file of "Simple Numbering".
    fn numbering(&mut self, entry: &Entry, folder: &mut dyn Folder) -> Outcome {
        let name = &entry.name;
        if name.to_lowercase().ends_with(".txt") {
            return Outcome::Skipped {
                source: None,
                reason: SkipReason::TextFile,
            };
        }
        let new_name = format!("{:05}{}", self.next_number, extension(name));
        if folder.exists(&new_name) {
            return Outcome::Skipped {
                source: None,
                reason: SkipReason::NameTaken(new_name),
            };
        }
        match folder.rename(name, &new_name) {
            Ok(()) => {
                self.next_number += 1;
                Outcome::Renamed {
                    new_name,
                    source: None,
                    mismatch: None,
                }
            }
            Err(error) => Outcome::Failed {
                error: error.to_string(),
            },
        }
    }
}

/// Runs the steps over all entries and reports every outcome to `on_file` in order.
fn run_steps(
    entries: &[Entry],
    skip: &[&str],
    steps: &mut Steps,
    folder: &mut dyn Folder,
    on_file: &mut dyn FnMut(FileResult) -> io::Result<()>,
) -> io::Result<()> {
    for entry in entries {
        if let Some(result) = steps.step(entry, skip, folder) {
            on_file(result)?;
        }
    }
    Ok(())
}

/// What renaming a folder would do, worked out file by file without changing anything. Listing
/// the folder is cheap, the steps read the files. A preview can stop after some files and go on
/// later where it stopped. A file's outcome depends only on the files before it and on the names
/// in the folder, so the files reported so far are always exactly the start of the full preview.
pub struct Preview {
    entries: Vec<Entry>,
    /// The index of the next entry.
    next: usize,
    steps: Steps,
    folder: SimulatedFolder,
}

impl Preview {
    /// Lists `folder` for a preview of renaming it in `mode`.
    pub fn new(folder: &Path, mode: RenameMode, now: NaiveDateTime) -> io::Result<Self> {
        let entries = sorted_entries(folder)?;
        let simulated =
            SimulatedFolder(entries.iter().map(|entry| name_key(&entry.name)).collect());
        Ok(Self {
            entries,
            next: 0,
            steps: Steps::new(mode, now),
            folder: simulated,
        })
    }

    /// The number of files the full preview reports.
    pub fn file_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_file).count()
    }

    /// Reports the outcomes of the next `limit` files, or of all the rest, to `on_file`. Stops
    /// early, before a file, once `stop` is set.
    pub fn advance(
        &mut self,
        limit: Option<usize>,
        stop: &AtomicBool,
        on_file: &mut dyn FnMut(FileResult),
    ) {
        let mut reported = 0;
        while let Some(entry) = self.entries.get(self.next) {
            if limit.is_some_and(|limit| reported >= limit) || stop.load(Ordering::Relaxed) {
                break;
            }
            self.next += 1;
            if let Some(result) = self.steps.step(entry, &[], &mut self.folder) {
                reported += 1;
                on_file(result);
            }
        }
    }
}

/// What renaming all files of `folder` in `mode` would do, without changing anything.
pub fn preview(folder: &Path, mode: RenameMode, now: NaiveDateTime) -> io::Result<Vec<FileResult>> {
    let mut results = Vec::new();
    Preview::new(folder, mode, now)?.advance(None, &AtomicBool::new(false), &mut |result| {
        results.push(result);
    });
    Ok(results)
}

/// The start of the report names of "Rename by Date", followed by the time of the run and `.txt`.
const REPORT_PREFIX: &str = "database_";

/// Logs a renamed file. The size and the time are read from the renamed file, which renaming does
/// not change. When they cannot be read, the line goes without them, and an undo skips the file.
/// When the line cannot be written, the file gets its old name back, so the log never misses a
/// renaming.
fn log_renamed(
    log: &mut LogWriter,
    folder: &Path,
    name: &str,
    new_name: &str,
    fields: &[(&str, &str)],
) -> io::Result<()> {
    let fingerprint = Fingerprint::of(&folder.join(new_name)).ok().flatten();
    log.renamed(name, new_name, fingerprint, fields)
        .map_err(|error| revert_unlogged(folder, name, new_name, error))
}

/// One row of the `;` separated report: fields with a separator, a quote or a line break are put
/// in quotes, and quotes are doubled.
fn report_row(fields: &[&str]) -> String {
    let quoted: Vec<String> = fields
        .iter()
        .map(|field| {
            if field.contains([';', '"', '\r', '\n']) {
                format!("\"{}\"", field.replace('"', "\"\""))
            } else {
                (*field).to_owned()
            }
        })
        .collect();
    format!("{}\r\n", quoted.join(";"))
}

impl SkipReason {
    /// The reason as part of a sentence, such as "Skipped, {reason}.".
    pub fn describe(&self) -> String {
        match self {
            Self::UnsupportedType => "this file type is not renamed".to_owned(),
            Self::TextFile => "text files are not renamed".to_owned(),
            Self::NameTaken(name) => format!("a file named '{name}' already exists"),
            Self::AlreadyNamed => "it already has the right name".to_owned(),
            Self::AlreadyRestored => "it already has its original name".to_owned(),
            Self::Changed => "it was changed after the renaming".to_owned(),
            Self::Missing => "it is no longer in the folder".to_owned(),
            Self::NotVerifiable => "the log has no size and time to check it".to_owned(),
        }
    }
}

/// The log and report file written by a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFiles {
    pub log: PathBuf,
    /// The `database_` report of "Rename by Date".
    pub report: Option<PathBuf>,
}

/// Renames the files of `folder`, writes the log and report files into it, passes messages to
/// `show` and every file's outcome to `on_file` as they happen. Returns the written files, or the
/// error that stopped the run.
pub fn rename_files(
    folder: &Path,
    mode: RenameMode,
    now: NaiveDateTime,
    show: &mut dyn FnMut(&str),
    on_file: &mut dyn FnMut(&FileResult),
) -> io::Result<RunFiles> {
    let result = match mode {
        RenameMode::ByDate => rename_by_date(folder, now, show, on_file),
        RenameMode::Numbering => number_files(folder, now, show, on_file),
    };
    if let Err(error) = &result {
        show(&format!(
            "\nThe renaming stopped because of an error: {error}"
        ));
    }
    result
}

/// "Rename by Date": the log, the `database_` report and the summary.
fn rename_by_date(
    folder: &Path,
    now: NaiveDateTime,
    show: &mut dyn FnMut(&str),
    on_file: &mut dyn FnMut(&FileResult),
) -> io::Result<RunFiles> {
    let stamp = unique_stamp(folder, now, &[BY_DATE_LOG_PREFIX, REPORT_PREFIX]);
    let log_name = format!("{BY_DATE_LOG_PREFIX}{stamp}.txt");
    let report_name = format!("{REPORT_PREFIX}{stamp}.txt");
    let mut log = LogWriter::create(&folder.join(&log_name), LogMode::ByDate, folder, now, None)?;
    let result = (|| -> io::Result<()> {
        let mut report = create_new(&folder.join(&report_name))?;
        report.write_all(
            report_row(&["Date source", "Original name", "New name", "Result"]).as_bytes(),
        )?;

        show("--- Renaming by date started ---");
        show(&format!("Folder: {}", folder.display()));
        show(
            "The date is taken from the photo metadata (EXIF) first, then from the file name, \
             then from the date the file was last modified.\n",
        );

        let mut mismatches: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let (mut renamed_count, mut skipped_count, mut mismatch_count) = (0usize, 0usize, 0usize);
        let entries = sorted_entries(folder)?;
        let mut real = RealFolder(folder);
        run_steps(
            &entries,
            &[log_name.as_str(), report_name.as_str()],
            &mut Steps::new(RenameMode::ByDate, now),
            &mut real,
            &mut |result| {
                let name = &result.name;
                match &result.outcome {
                    Outcome::Renamed {
                        new_name,
                        source,
                        mismatch,
                    } => {
                        let source = source.unwrap_or(DateSource::LastModified);
                        renamed_count += 1;
                        let mut message = format!(
                            "Renamed '{name}' to '{new_name}' ({}).",
                            source.description()
                        );
                        let mut fields = vec![("date", source.report_name())];
                        if let Some(details) = mismatch {
                            message.push_str(
                                " The dates in the file name and in the metadata differ.",
                            );
                            fields.push((
                                "note",
                                "the dates in the file name and in the metadata differ",
                            ));
                            mismatch_count += 1;
                            mismatches.push(details.clone());
                        }
                        show(&message);
                        log_renamed(&mut log, folder, name, new_name, &fields)?;
                        report.write_all(
                            report_row(&[source.report_name(), name, new_name, "Renamed."])
                                .as_bytes(),
                        )?;
                    }
                    Outcome::Skipped { source, reason } => {
                        let reason = reason.describe();
                        skipped.push(format!("'{name}' ({reason})"));
                        log.skipped(name, &reason)?;
                        report.write_all(
                            report_row(&[
                                source.map_or("-", DateSource::report_name),
                                name,
                                "-",
                                &format!("Skipped, {reason}."),
                            ])
                            .as_bytes(),
                        )?;
                        skipped_count += 1;
                    }
                    Outcome::Failed { error } => {
                        show(&format!("Could not rename '{name}': {error}"));
                        log.failed(name, error)?;
                        skipped.push(format!("'{name}' (error: {error})"));
                        report.write_all(
                            report_row(&[
                                "-",
                                name,
                                "-",
                                &format!("Not renamed because of an error: {error}"),
                            ])
                            .as_bytes(),
                        )?;
                        skipped_count += 1;
                    }
                }
                on_file(&result);
                Ok(())
            },
        )?;

        let list = |items: &[String]| {
            if items.is_empty() {
                "None".to_owned()
            } else {
                items
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };
        show(&format!("\n{} SUMMARY {}", "=".repeat(25), "=".repeat(25)));
        show(&format!(
            "--- Files whose dates differ ({}) ---",
            mismatches.len()
        ));
        show(&list(&mismatches));
        show(&format!("\n--- Skipped files ({}) ---", skipped.len()));
        show(&list(&skipped));
        show(&format!(
            "\n--- Totals ---\nRenamed files: {renamed_count}\n  - with differing dates: {mismatch_count}\nSkipped files: {skipped_count}"
        ));
        show(&format!(
            "\nThe detailed report was saved as '{report_name}'."
        ));
        Ok(())
    })();
    log.end(result)?;
    Ok(RunFiles {
        log: folder.join(log_name),
        report: Some(folder.join(report_name)),
    })
}

/// "Simple Numbering": the log and the totals.
fn number_files(
    folder: &Path,
    now: NaiveDateTime,
    show: &mut dyn FnMut(&str),
    on_file: &mut dyn FnMut(&FileResult),
) -> io::Result<RunFiles> {
    let stamp = unique_stamp(folder, now, &[NUMBERING_LOG_PREFIX]);
    let log_name = format!("{NUMBERING_LOG_PREFIX}{stamp}.txt");
    let mut log = LogWriter::create(
        &folder.join(&log_name),
        LogMode::Numbering,
        folder,
        now,
        None,
    )?;
    let result = (|| -> io::Result<()> {
        show("--- Numbering started ---");
        show(&format!("Folder: {}\n", folder.display()));

        let (mut renamed_count, mut skipped_count) = (0usize, 0usize);
        let entries = sorted_entries(folder)?;
        let mut real = RealFolder(folder);
        let mut steps = Steps::new(RenameMode::Numbering, now);
        run_steps(
            &entries,
            &[log_name.as_str()],
            &mut steps,
            &mut real,
            &mut |result| {
                let name = &result.name;
                match &result.outcome {
                    Outcome::Renamed { new_name, .. } => {
                        renamed_count += 1;
                        show(&format!("Renamed '{name}' to '{new_name}'."));
                        log_renamed(&mut log, folder, name, new_name, &[])?;
                    }
                    Outcome::Skipped { reason, .. } => {
                        skipped_count += 1;
                        let reason = reason.describe();
                        show(&format!("Skipped '{name}', {reason}."));
                        log.skipped(name, &reason)?;
                    }
                    Outcome::Failed { error } => {
                        skipped_count += 1;
                        show(&format!("Could not rename '{name}': {error}"));
                        log.failed(name, error)?;
                    }
                }
                on_file(&result);
                Ok(())
            },
        )?;
        show(&format!(
            "\n--- Finished ---\nRenamed files: {renamed_count}\nSkipped files: {skipped_count}"
        ));
        Ok(())
    })();
    log.end(result)?;
    Ok(RunFiles {
        log: folder.join(log_name),
        report: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif::tests::{jpeg, tiff};
    use crate::rename_log::read_log;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 7)
            .unwrap()
            .and_hms_opt(9, 5, 2)
            .unwrap()
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn set_modified(path: &Path, date: NaiveDate) {
        let time = date
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .single()
            .unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time.into())
            .unwrap();
    }

    #[test]
    fn finds_dates_in_file_names() {
        let date = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
        assert_eq!(
            date_from_file_name("IMG_2021-05-06.jpg", 2026),
            date(2021, 5, 6)
        );
        assert_eq!(
            date_from_file_name("2021_05_06 trip.jpg", 2026),
            date(2021, 5, 6)
        );
        assert_eq!(
            date_from_file_name("IMG_20210506_1200.jpg", 2026),
            date(2021, 5, 6)
        );
        assert_eq!(date_from_file_name("12345-06-07.jpg", 2026), None);
        assert_eq!(date_from_file_name("1949-01-01.jpg", 2026), None);
        assert_eq!(date_from_file_name("2030-01-01.jpg", 2026), None);
        // An invalid separated date falls back to the first eight digits in a row.
        assert_eq!(
            date_from_file_name("2021-13-01 20200102.jpg", 2026),
            date(2020, 1, 2)
        );
        // Only the first run of eight digits is tried.
        assert_eq!(date_from_file_name("99999999 20200102.jpg", 2026), None);
        // Decimal digits of other scripts count as digits.
        assert_eq!(
            date_from_file_name(
                "\u{0662}\u{0660}\u{0662}\u{0660}-\u{0660}\u{0661}-\u{0660}\u{0663}.jpg",
                2026
            ),
            date(2020, 1, 3)
        );
        assert_eq!(date_from_file_name("\u{00B2}0200102.jpg", 2026), None);
    }

    #[test]
    fn splits_extensions() {
        assert_eq!(extension("photo.JPG"), ".JPG");
        assert_eq!(extension("archive.tar.gz"), ".gz");
        assert_eq!(extension(".hidden"), "");
        assert_eq!(extension("..jpg"), "");
        assert_eq!(extension("noext"), "");
    }

    #[test]
    fn quotes_report_fields() {
        assert_eq!(
            report_row(&["a", "b;c", "d\"e"]),
            "a;\"b;c\";\"d\"\"e\"\r\n"
        );
    }

    #[test]
    fn renames_by_date_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        fs::write(
            folder.join("b.JPG"),
            jpeg(&tiff(None, Some("2020:01:02 03:04:05"))),
        )
        .unwrap();
        fs::write(folder.join("a 2020-01-02.png"), b"not an image").unwrap();
        fs::write(folder.join("C.mp4"), b"video").unwrap();
        set_modified(
            &folder.join("C.mp4"),
            NaiveDate::from_ymd_opt(2019, 7, 8).unwrap(),
        );
        fs::write(folder.join("notes.doc"), b"text").unwrap();
        fs::create_dir(folder.join("sub")).unwrap();

        let planned = preview(folder, RenameMode::ByDate, now()).unwrap();
        let mut shown = Vec::new();
        let mut done = Vec::new();
        rename_files(
            folder,
            RenameMode::ByDate,
            now(),
            &mut |message| shown.push(message.to_owned()),
            &mut |result| done.push(result.clone()),
        )
        .unwrap();
        assert_eq!(planned, done);

        assert_eq!(
            names(folder),
            [
                "2019_07_08_001.mp4",
                "2020_01_02_001.png",
                "2020_01_02_002.jpg",
                "database_2026-03-07_090502.txt",
                "notes.doc",
                "rename_log_2026-03-07_090502.txt",
                "sub",
            ]
        );
        let report = fs::read_to_string(folder.join("database_2026-03-07_090502.txt")).unwrap();
        assert_eq!(
            report,
            "Date source;Original name;New name;Result\r\n\
             Last modified;C.mp4;2019_07_08_001.mp4;Renamed.\r\n\
             File name;a 2020-01-02.png;2020_01_02_001.png;Renamed.\r\n\
             EXIF;b.JPG;2020_01_02_002.jpg;Renamed.\r\n\
             -;notes.doc;-;Skipped, this file type is not renamed.\r\n"
        );
        let log = read_log(&folder.join("rename_log_2026-03-07_090502.txt")).unwrap();
        assert_eq!(log.mode, LogMode::ByDate);
        assert_eq!(log.started, "2026-03-07 09:05:02");
        assert!(log.finished);
        assert_eq!(
            log.renamed()
                .map(|(from, to, fingerprint)| (from, to, fingerprint.unwrap().size))
                .collect::<Vec<_>>(),
            [
                ("C.mp4", "2019_07_08_001.mp4", 5),
                ("a 2020-01-02.png", "2020_01_02_001.png", 12),
                (
                    "b.JPG",
                    "2020_01_02_002.jpg",
                    fs::metadata(folder.join("2020_01_02_002.jpg"))
                        .unwrap()
                        .len()
                ),
            ]
        );
        assert_eq!(log.entries.len(), 4);
        assert!(
            shown
                .iter()
                .any(|message| message.contains("Renamed files: 3"))
        );
    }

    #[test]
    fn skips_taken_names_without_replacing() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        fs::write(folder.join("1 2020-01-02.jpg"), b"first").unwrap();
        fs::write(folder.join("2020_01_02_001.jpg"), b"kept").unwrap();
        let planned = preview(folder, RenameMode::ByDate, now()).unwrap();
        let mut done = Vec::new();
        rename_files(
            folder,
            RenameMode::ByDate,
            now(),
            &mut |_| {},
            &mut |result| done.push(result.clone()),
        )
        .unwrap();
        assert_eq!(planned, done);
        // The first file wants the taken name and is skipped. The counter still moves on, so the
        // second file gets the next number.
        assert_eq!(fs::read(folder.join("1 2020-01-02.jpg")).unwrap(), b"first");
        assert_eq!(
            fs::read(folder.join("2020_01_02_002.jpg")).unwrap(),
            b"kept"
        );
        assert!(!folder.join("2020_01_02_001.jpg").exists());
    }

    #[test]
    fn numbers_files_except_text_files() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        for name in ["b.png", "A.JPG", "readme.TXT", "c"] {
            fs::write(folder.join(name), name).unwrap();
        }
        let planned = preview(folder, RenameMode::Numbering, now()).unwrap();
        let mut shown = Vec::new();
        let mut done = Vec::new();
        rename_files(
            folder,
            RenameMode::Numbering,
            now(),
            &mut |message| shown.push(message.to_owned()),
            &mut |result| done.push(result.clone()),
        )
        .unwrap();
        assert_eq!(planned, done);
        assert_eq!(
            names(folder),
            [
                "00001.JPG",
                "00002.png",
                "00003",
                "numbering_log_2026-03-07_090502.txt",
                "readme.TXT",
            ]
        );
        assert_eq!(fs::read(folder.join("00001.JPG")).unwrap(), b"A.JPG");
        let log = read_log(&folder.join("numbering_log_2026-03-07_090502.txt")).unwrap();
        assert_eq!(log.mode, LogMode::Numbering);
        assert_eq!(
            log.renamed()
                .map(|(from, to, _)| (from, to))
                .collect::<Vec<_>>(),
            [
                ("A.JPG", "00001.JPG"),
                ("b.png", "00002.png"),
                ("c", "00003")
            ]
        );
        // The text file is logged as skipped.
        assert_eq!(log.entries.len(), 4);
        assert!(shown.last().unwrap().contains("Renamed files: 3"));
    }

    /// The files a preview reports in one step.
    fn advance(preview: &mut Preview, limit: Option<usize>) -> Vec<FileResult> {
        let mut rows = Vec::new();
        preview.advance(limit, &AtomicBool::new(false), &mut |row| rows.push(row));
        rows
    }

    #[test]
    fn preview_goes_on_where_it_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        for name in [
            "0 2020-01-02.jpg",
            "1 2020-01-02.jpg",
            "2020_01_02_001.jpg",
            "z 2020-01-02.jpg",
        ] {
            fs::write(folder.join(name), name).unwrap();
        }
        fs::create_dir(folder.join("1 sub")).unwrap();

        for mode in [RenameMode::ByDate, RenameMode::Numbering] {
            let full = preview(folder, mode, now()).unwrap();
            assert_eq!(full.len(), 4);
            for limit in 0..=5 {
                let mut listed = Preview::new(folder, mode, now()).unwrap();
                assert_eq!(listed.file_count(), 4);
                let mut rows = advance(&mut listed, Some(limit));
                assert_eq!(rows, full[..limit.min(full.len())], "{mode:?} {limit}");
                rows.extend(advance(&mut listed, None));
                assert_eq!(rows, full, "{mode:?} {limit}");
            }
            let mut listed = Preview::new(folder, mode, now()).unwrap();
            let mut rows = Vec::new();
            listed.advance(None, &AtomicBool::new(true), &mut |row| rows.push(row));
            assert!(rows.is_empty());
            while rows.len() < full.len() {
                rows.extend(advance(&mut listed, Some(1)));
            }
            assert_eq!(rows, full);
        }
        // The first file already sees the name taken by a file further down the list.
        let first = advance(
            &mut Preview::new(folder, RenameMode::ByDate, now()).unwrap(),
            Some(1),
        );
        assert!(matches!(
            &first[0].outcome,
            Outcome::Skipped {
                reason: SkipReason::NameTaken(name),
                ..
            } if name == "2020_01_02_001.jpg"
        ));
    }

    #[test]
    fn earlier_logs_are_listed_and_never_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        fs::write(folder.join("a 2020-01-02.jpg"), b"photo").unwrap();
        fs::write(
            folder.join("rename_log_2026-03-07_090502.txt"),
            b"earlier run",
        )
        .unwrap();
        fs::write(
            folder.join("database_2026-03-07_090502.txt"),
            b"earlier report",
        )
        .unwrap();
        let listed = Preview::new(folder, RenameMode::ByDate, now()).unwrap();
        assert_eq!(listed.file_count(), 3);
        let planned = preview(folder, RenameMode::ByDate, now()).unwrap();
        let mut done = Vec::new();
        let files = rename_files(
            folder,
            RenameMode::ByDate,
            now(),
            &mut |_| {},
            &mut |result| done.push(result.clone()),
        )
        .unwrap();
        assert_eq!(planned, done);
        assert_eq!(done.len(), 3);
        // A run in the same second gets new names and leaves the earlier files alone.
        assert_eq!(files.log, folder.join("rename_log_2026-03-07_090502_2.txt"));
        assert_eq!(
            files.report,
            Some(folder.join("database_2026-03-07_090502_2.txt"))
        );
        assert_eq!(
            fs::read(folder.join("rename_log_2026-03-07_090502.txt")).unwrap(),
            b"earlier run"
        );
        assert_eq!(
            fs::read(folder.join("database_2026-03-07_090502.txt")).unwrap(),
            b"earlier report"
        );
    }

    #[test]
    fn a_missing_folder_fails_before_writing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let error = rename_files(
            &dir.path().join("missing"),
            RenameMode::Numbering,
            now(),
            &mut |_| {},
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
