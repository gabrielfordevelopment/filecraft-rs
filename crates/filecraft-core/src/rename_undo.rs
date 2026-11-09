//! Gives the files of an earlier File Rename run their original names back, using the log the run
//! wrote into the folder.
//!
//! The renamings are reverted from the last one to the first, which exactly reverses the run even
//! when a file took a name another file gave up earlier in the same run. A file only gets its old
//! name back when its size and modification time still match the log and the old name is free.
//! Files that were edited, moved or replaced since are skipped, and nothing is ever replaced.
//! [`undo_preview`] runs the same checks on a simulated folder, and the undo checks every file
//! again right before renaming it.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use chrono::NaiveDateTime;

use crate::file_rename::{FileResult, Outcome, RunFiles, SkipReason};
use crate::path_key::name_key;
use crate::rename_log::{
    Fingerprint, LogMode, LogWriter, ParsedLog, UNDO_LOG_PREFIX, looks_like_log, read_log,
    unique_stamp,
};

/// A run found in a folder that can be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRun {
    /// The name of its log in the folder.
    pub log_name: String,
    /// What the log says, or why it cannot be read.
    pub details: Result<RunDetails, String>,
    /// An undo of this run was done before. It may have skipped some files.
    pub undone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDetails {
    pub mode: LogMode,
    /// When the run started, `YYYY-MM-DD HH:MM:SS`.
    pub started: String,
    /// The number of renamed files.
    pub renamed: usize,
    /// The run went through to the end.
    pub finished: bool,
}

/// The runs logged in `folder` that renamed at least one file, newest first. Logs that look like
/// FileCraft rename logs but cannot be read are listed too, with the reason.
pub fn find_runs(folder: &Path) -> io::Result<Vec<RenameRun>> {
    // Each run with the time its log was last written, which orders runs started in the same
    // second.
    let mut runs: Vec<(RenameRun, Option<SystemTime>)> = Vec::new();
    let mut undone = HashSet::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let written = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        // A text file that cannot be opened is not known to be a log, so it is not listed.
        if !name.to_lowercase().ends_with(".txt") || !looks_like_log(&entry.path()).unwrap_or(false)
        {
            continue;
        }
        match read_log(&entry.path()) {
            Ok(log) if log.mode == LogMode::Undo => {
                if let Some(run) = log.undoes {
                    undone.insert(name_key(&run));
                }
            }
            Ok(log) => {
                let renamed = log.renamed().count();
                if renamed > 0 {
                    let run = RenameRun {
                        log_name: name,
                        details: Ok(RunDetails {
                            mode: log.mode,
                            started: log.started,
                            renamed,
                            finished: log.finished,
                        }),
                        undone: false,
                    };
                    runs.push((run, written));
                }
            }
            Err(error) => {
                let run = RenameRun {
                    log_name: name,
                    details: Err(error.to_string()),
                    undone: false,
                };
                runs.push((run, written));
            }
        }
    }
    let started = |run: &RenameRun| {
        run.details
            .as_ref()
            .map(|details| details.started.clone())
            .unwrap_or_default()
    };
    runs.sort_by(|(a, a_written), (b, b_written)| {
        started(b)
            .cmp(&started(a))
            .then_with(|| b_written.cmp(a_written))
            .then_with(|| b.log_name.cmp(&a.log_name))
    });
    Ok(runs
        .into_iter()
        .map(|(mut run, _)| {
            run.undone = undone.contains(&name_key(&run.log_name));
            run
        })
        .collect())
}

/// Reads the log `log_name` of `folder` for an undo. Fails for the log of an undo.
pub fn read_run(folder: &Path, log_name: &str) -> io::Result<ParsedLog> {
    let log = read_log(&folder.join(log_name))?;
    if log.mode == LogMode::Undo {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the log of an undo cannot be undone",
        ));
    }
    Ok(log)
}

/// What a name in the folder holds right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameState {
    Free,
    File(Fingerprint),
    /// A folder, a broken link or anything else that is not a readable file.
    Other,
}

trait UndoFolder {
    fn state(&self, name: &str) -> io::Result<NameState>;
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
}

struct RealFolder<'a>(&'a Path);

impl UndoFolder for RealFolder<'_> {
    fn state(&self, name: &str) -> io::Result<NameState> {
        let path = self.0.join(name);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(NameState::Free),
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        match Fingerprint::of(&path) {
            Ok(Some(fingerprint)) => Ok(NameState::File(fingerprint)),
            Ok(None) => Ok(NameState::Other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(NameState::Other),
            Err(error) => Err(error),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        fs::rename(self.0.join(from), self.0.join(to))
    }
}

/// The real folder with the renamings of the preview applied on top, compared like the file
/// system compares names.
struct PlannedFolder<'a> {
    real: RealFolder<'a>,
    changed: HashMap<String, NameState>,
}

impl UndoFolder for PlannedFolder<'_> {
    fn state(&self, name: &str) -> io::Result<NameState> {
        match self.changed.get(&name_key(name)) {
            Some(state) => Ok(*state),
            None => self.real.state(name),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        let moved = self.state(from)?;
        self.changed.insert(name_key(from), NameState::Free);
        self.changed.insert(name_key(to), moved);
        Ok(())
    }
}

/// Reverts one renaming: `original` was renamed to `renamed`, which had `recorded` then.
fn undo_step(
    folder: &mut dyn UndoFolder,
    original: &str,
    renamed: &str,
    recorded: Option<Fingerprint>,
) -> FileResult {
    let skipped = |reason| Outcome::Skipped {
        source: None,
        reason,
    };
    let outcome = match recorded {
        None => skipped(SkipReason::NotVerifiable),
        Some(recorded) => (|| -> io::Result<Outcome> {
            let current = folder.state(renamed)?;
            if matches!(current, NameState::File(print) if print.matches(&recorded)) {
                return Ok(if folder.state(original)? == NameState::Free {
                    folder.rename(renamed, original)?;
                    Outcome::Renamed {
                        new_name: original.to_owned(),
                        source: None,
                        mismatch: None,
                    }
                } else {
                    skipped(SkipReason::NameTaken(original.to_owned()))
                });
            }
            // The file is not under its new name. It may have its old name already, also when
            // another file took the new name since.
            Ok(match folder.state(original)? {
                NameState::File(print) if print.matches(&recorded) => {
                    skipped(SkipReason::AlreadyRestored)
                }
                _ if current == NameState::Free => skipped(SkipReason::Missing),
                _ => skipped(SkipReason::Changed),
            })
        })()
        .unwrap_or_else(|error| Outcome::Failed {
            error: error.to_string(),
        }),
    };
    FileResult {
        name: renamed.to_owned(),
        outcome,
    }
}

/// What undoing the run of `log` in `folder` would do, file by file in the order the undo goes,
/// without changing anything. Stops early, before a file, once `stop` is set.
pub fn undo_preview(
    folder: &Path,
    log: &ParsedLog,
    stop: &AtomicBool,
    on_file: &mut dyn FnMut(FileResult),
) {
    let mut planned = PlannedFolder {
        real: RealFolder(folder),
        changed: HashMap::new(),
    };
    for (original, renamed, recorded) in log.renamed().rev() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        on_file(undo_step(&mut planned, original, renamed, recorded));
    }
}

/// Gives the files renamed by the run logged in `log_name` their original names back, writes an
/// undo log into `folder`, passes messages to `show` and every file's outcome to `on_file` as they
/// happen. Returns the undo log, or the error that stopped the undo.
pub fn undo_files(
    folder: &Path,
    log_name: &str,
    now: NaiveDateTime,
    show: &mut dyn FnMut(&str),
    on_file: &mut dyn FnMut(&FileResult),
) -> io::Result<RunFiles> {
    let result = undo_run(folder, log_name, now, show, on_file);
    if let Err(error) = &result {
        show(&format!("\nThe undo stopped because of an error: {error}"));
    }
    result
}

fn undo_run(
    folder: &Path,
    log_name: &str,
    now: NaiveDateTime,
    show: &mut dyn FnMut(&str),
    on_file: &mut dyn FnMut(&FileResult),
) -> io::Result<RunFiles> {
    let run = read_run(folder, log_name)?;
    let stamp = unique_stamp(folder, now, &[UNDO_LOG_PREFIX]);
    let undo_name = format!("{UNDO_LOG_PREFIX}{stamp}.txt");
    let mut log = LogWriter::create(
        &folder.join(&undo_name),
        LogMode::Undo,
        folder,
        now,
        Some(log_name),
    )?;
    let (mut restored, mut skipped) = (0usize, 0usize);
    let result = (|| -> io::Result<()> {
        show("--- Undo started ---");
        show(&format!("Folder: {}", folder.display()));
        show(&format!(
            "The files renamed on {} get their original names back, from the last renamed file to \
             the first. The renaming is described in '{log_name}'.\n",
            run.started
        ));
        let mut real = RealFolder(folder);
        for (original, renamed, recorded) in run.renamed().rev() {
            let result = undo_step(&mut real, original, renamed, recorded);
            match &result.outcome {
                Outcome::Renamed { .. } => {
                    restored += 1;
                    show(&format!("Renamed '{renamed}' back to '{original}'."));
                    let fingerprint = Fingerprint::of(&folder.join(original))
                        .ok()
                        .flatten()
                        .or(recorded);
                    log.renamed(renamed, original, fingerprint, &[])
                        .map_err(|error| revert_unlogged(folder, renamed, original, error))?;
                }
                Outcome::Skipped { reason, .. } => {
                    skipped += 1;
                    let reason = reason.describe();
                    show(&format!("Skipped '{renamed}', {reason}."));
                    log.skipped(renamed, &reason)?;
                }
                Outcome::Failed { error } => {
                    skipped += 1;
                    show(&format!("Could not rename '{renamed}' back: {error}"));
                    log.failed(renamed, error)?;
                }
            }
            on_file(&result);
        }
        show(&format!(
            "\n--- Finished ---\nFiles with their original names back: {restored}\nSkipped files: \
             {skipped}"
        ));
        Ok(())
    })();
    log.end(result)?;
    Ok(RunFiles {
        log: folder.join(undo_name),
        report: None,
    })
}

/// Called when a renaming happened but its log line could not be written. Renames the file from
/// `after` back to `before` if that name is still free, so the log stays complete, and returns the
/// error to stop the run with.
pub fn revert_unlogged(folder: &Path, before: &str, after: &str, error: io::Error) -> io::Error {
    let free = matches!(
        fs::symlink_metadata(folder.join(before)),
        Err(error) if error.kind() == io::ErrorKind::NotFound
    );
    let message = match free.then(|| fs::rename(folder.join(after), folder.join(before))) {
        Some(Ok(())) => format!(
            "The log could not be written, so '{after}' was renamed back to '{before}': {error}"
        ),
        Some(Err(revert_error)) => format!(
            "The log could not be written: {error}. '{before}' is now named '{after}', and \
             renaming it back failed: {revert_error}"
        ),
        None => format!(
            "The log could not be written: {error}. '{before}' is now named '{after}', which is \
             not in the log."
        ),
    };
    io::Error::new(error.kind(), message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_rename::{RenameMode, rename_files};
    use crate::rename_log::{BY_DATE_LOG_PREFIX, NUMBERING_LOG_PREFIX};
    use chrono::{DateTime, NaiveDate, Utc};
    use std::time::{Duration, SystemTime};

    fn at(minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 7)
            .unwrap()
            .and_hms_opt(9, minute, 0)
            .unwrap()
    }

    /// The names and contents of the files in `folder`, without the logs.
    fn files(folder: &Path) -> Vec<(String, String)> {
        let mut files: Vec<(String, String)> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    String::from_utf8_lossy(&fs::read(entry.path()).unwrap()).into_owned(),
                )
            })
            .filter(|(name, _)| {
                !name.starts_with(BY_DATE_LOG_PREFIX)
                    && !name.starts_with(NUMBERING_LOG_PREFIX)
                    && !name.starts_with(UNDO_LOG_PREFIX)
                    && !name.starts_with("database_")
            })
            .collect();
        files.sort();
        files
    }

    fn owned(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files
            .iter()
            .map(|(name, content)| ((*name).to_owned(), (*content).to_owned()))
            .collect()
    }

    fn write(folder: &Path, files: &[(&str, &str)]) {
        for (name, content) in files {
            fs::write(folder.join(name), content).unwrap();
        }
    }

    fn rename(folder: &Path, mode: RenameMode, now: NaiveDateTime) -> String {
        let files = rename_files(folder, mode, now, &mut |_| {}, &mut |_| {}).unwrap();
        files
            .log
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn plan(folder: &Path, log_name: &str) -> Vec<FileResult> {
        let log = read_run(folder, log_name).unwrap();
        let mut rows = Vec::new();
        undo_preview(folder, &log, &AtomicBool::new(false), &mut |row| {
            rows.push(row);
        });
        rows
    }

    /// Undoes the run and checks that the preview made just before matched what happened.
    fn undo(folder: &Path, log_name: &str, now: NaiveDateTime) -> Vec<FileResult> {
        let planned = plan(folder, log_name);
        let mut done = Vec::new();
        undo_files(folder, log_name, now, &mut |_| {}, &mut |row| {
            done.push(row.clone());
        })
        .unwrap();
        assert_eq!(planned, done);
        done
    }

    /// Each file with the reason it was skipped, or `None` when it got its name back.
    fn reasons(rows: &[FileResult]) -> Vec<(&str, Option<SkipReason>)> {
        rows.iter()
            .map(|row| {
                let reason = match &row.outcome {
                    Outcome::Skipped { reason, .. } => Some(reason.clone()),
                    Outcome::Renamed { .. } => None,
                    Outcome::Failed { error } => panic!("{} failed: {error}", row.name),
                };
                (row.name.as_str(), reason)
            })
            .collect()
    }

    fn set_modified(path: &Path, time: SystemTime) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    fn modified(path: &Path) -> SystemTime {
        fs::metadata(path).unwrap().modified().unwrap()
    }

    #[test]
    fn restores_a_numbering_with_a_name_chain() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        // "00002.jpg" becomes "00001.jpg", then "a.jpg" takes the freed "00002.jpg". Only undoing
        // from the last renaming to the first gives both their names back.
        let before = [
            ("00002.jpg", "two"),
            ("a.jpg", "a"),
            ("b.png", "b"),
            ("notes.txt", "notes"),
        ];
        write(folder, &before);
        let log_name = rename(folder, RenameMode::Numbering, at(0));
        assert_eq!(
            files(folder),
            owned(&[
                ("00001.jpg", "two"),
                ("00002.jpg", "a"),
                ("00003.png", "b"),
                ("notes.txt", "notes"),
            ])
        );
        let done = undo(folder, &log_name, at(1));
        assert_eq!(
            reasons(&done),
            [
                ("00003.png", None),
                ("00002.jpg", None),
                ("00001.jpg", None)
            ]
        );
        assert_eq!(files(folder), owned(&before));

        // The undo log marks the run, and a second undo finds nothing to do.
        let runs = find_runs(folder).unwrap();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].undone);
        let again = undo(folder, &log_name, at(2));
        assert!(
            reasons(&again)
                .iter()
                .all(|(_, reason)| *reason == Some(SkipReason::AlreadyRestored))
        );
        assert_eq!(files(folder), owned(&before));
    }

    #[test]
    fn restores_a_renaming_by_date() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        let before = [
            ("Anna's photo 2020-01-02.jpg", "anna"),
            ("IMG 2020-01-02.PNG", "img"),
            ("clip 2019-05-06.mp4", "clip"),
            ("readme.doc", "readme"),
        ];
        write(folder, &before);
        let log_name = rename(folder, RenameMode::ByDate, at(0));
        assert_eq!(log_name, "rename_log_2026-03-07_090000.txt");
        assert_eq!(
            files(folder),
            owned(&[
                ("2019_05_06_001.mp4", "clip"),
                ("2020_01_02_001.jpg", "anna"),
                ("2020_01_02_002.png", "img"),
                ("readme.doc", "readme"),
            ])
        );
        undo(folder, &log_name, at(1));
        assert_eq!(files(folder), owned(&before));
    }

    #[test]
    fn unusual_names_survive_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        let before = [
            ("  spaced  name.png", "1"),
            ("#001 RENAMED.jpg", "2"),
            ("a - to - b.jpg", "3"),
            ("key=value size=1.jpg", "4"),
            ("semi;colon 'quoted'.jpg", "5"),
            (
                "\u{e1}rv\u{ed}zt\u{171}r\u{151} t\u{fc}k\u{f6}rf\u{fa}r\u{f3}g\u{e9}p.jpg",
                "6",
            ),
            ("emoji \u{1f600}.jpg", "7"),
            ("no extension", "8"),
        ];
        write(folder, &before);
        let log_name = rename(folder, RenameMode::Numbering, at(0));
        assert!(
            files(folder)
                .iter()
                .all(|(name, _)| name.starts_with("0000"))
        );
        let done = undo(folder, &log_name, at(1));
        assert_eq!(done.len(), before.len());
        assert!(reasons(&done).iter().all(|(_, reason)| reason.is_none()));
        let mut expected = owned(&before);
        expected.sort();
        assert_eq!(files(folder), expected);
    }

    #[test]
    fn skips_files_that_changed_or_moved_and_never_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(
            folder,
            &[
                ("00002.jpg", "two"),
                ("a.jpg", "a"),
                ("b.png", "b"),
                ("c.png", "c"),
                ("d.png", "d"),
                ("e.png", "e"),
            ],
        );
        let log_name = rename(folder, RenameMode::Numbering, at(0));
        // 00002.jpg -> 00001.jpg, a.jpg -> 00002.jpg, b.png -> 00003.png, c.png -> 00004.png,
        // d.png -> 00005.png, e.png -> 00006.png.

        // The renamed "a.jpg" is edited. "00001.jpg" then cannot get "00002.jpg" back.
        fs::write(folder.join("00002.jpg"), "a edited").unwrap();
        // The renamed "b.png" is moved away.
        let away = tempfile::tempdir().unwrap();
        fs::rename(folder.join("00003.png"), away.path().join("b.png")).unwrap();
        // A new file takes the old name of "c.png".
        fs::write(folder.join("c.png"), "new c").unwrap();
        // A folder takes the new name of "d.png" after the file was deleted.
        fs::remove_file(folder.join("00005.png")).unwrap();
        fs::create_dir(folder.join("00005.png")).unwrap();
        // "e.png" got its name back by hand.
        fs::rename(folder.join("00006.png"), folder.join("e.png")).unwrap();

        let done = undo(folder, &log_name, at(1));
        assert_eq!(
            reasons(&done),
            [
                ("00006.png", Some(SkipReason::AlreadyRestored)),
                ("00005.png", Some(SkipReason::Changed)),
                ("00004.png", Some(SkipReason::NameTaken("c.png".to_owned()))),
                ("00003.png", Some(SkipReason::Missing)),
                ("00002.jpg", Some(SkipReason::Changed)),
                (
                    "00001.jpg",
                    Some(SkipReason::NameTaken("00002.jpg".to_owned()))
                ),
            ]
        );
        assert_eq!(
            files(folder),
            owned(&[
                ("00001.jpg", "two"),
                ("00002.jpg", "a edited"),
                ("00004.png", "c"),
                ("c.png", "new c"),
                ("e.png", "e"),
            ])
        );
        assert!(folder.join("00005.png").is_dir());
    }

    #[test]
    fn a_skipped_file_does_not_block_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(
            folder,
            &[("00002.jpg", "two"), ("a.jpg", "a"), ("b.png", "b")],
        );
        let log_name = rename(folder, RenameMode::Numbering, at(0));
        // The first renamed file is edited. The later ones still get their names back.
        fs::write(folder.join("00001.jpg"), "two edited").unwrap();
        let done = undo(folder, &log_name, at(1));
        assert_eq!(
            reasons(&done),
            [
                ("00003.png", None),
                ("00002.jpg", None),
                ("00001.jpg", Some(SkipReason::Changed)),
            ]
        );
        assert_eq!(
            files(folder),
            owned(&[("00001.jpg", "two edited"), ("a.jpg", "a"), ("b.png", "b")])
        );
    }

    #[test]
    fn modification_times_may_differ_by_two_seconds() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(folder, &[("a.jpg", "a"), ("b.jpg", "b")]);
        let log_name = rename(folder, RenameMode::Numbering, at(0));
        let first = folder.join("00001.jpg");
        set_modified(&first, modified(&first) + Duration::from_millis(1900));
        let second = folder.join("00002.jpg");
        set_modified(&second, modified(&second) + Duration::from_secs(5));
        let done = undo(folder, &log_name, at(1));
        assert_eq!(
            reasons(&done),
            [
                ("00002.jpg", Some(SkipReason::Changed)),
                ("00001.jpg", None)
            ]
        );
    }

    #[test]
    fn a_later_run_must_be_undone_first() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        let before = [("x 2020-01-02.jpg", "x"), ("y 2020-01-02.jpg", "y")];
        write(folder, &before);
        // Both runs start in the same second. The later one is still listed first.
        let by_date = rename(folder, RenameMode::ByDate, at(0));
        std::thread::sleep(Duration::from_millis(50));
        let numbering = rename(folder, RenameMode::Numbering, at(0));
        let runs = find_runs(folder).unwrap();
        assert_eq!(
            runs.iter()
                .map(|run| run.log_name.as_str())
                .collect::<Vec<_>>(),
            [numbering.as_str(), by_date.as_str()]
        );
        // The files renamed by the first run have other names now.
        assert!(
            reasons(&plan(folder, &by_date))
                .iter()
                .all(|(_, reason)| *reason == Some(SkipReason::Missing))
        );
        undo(folder, &numbering, at(2));
        undo(folder, &by_date, at(3));
        assert_eq!(files(folder), owned(&before));
    }

    #[test]
    fn finds_only_readable_rename_logs() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(folder, &[("a.doc", "a")]);
        let numbering = rename(folder, RenameMode::Numbering, at(0));
        // A run that renamed nothing is not listed.
        rename(folder, RenameMode::ByDate, at(1));
        write(
            folder,
            &[
                // The old log format, other text files and a damaged log.
                (
                    "rename_log_2026-03-06.txt",
                    "[2026-03-06 10:00:00] Renamed 'a' to 'b'.",
                ),
                ("notes.txt", "FileCraft"),
                ("short.TXT", ""),
                (
                    "rename_log_damaged.txt",
                    "FileCraft rename log\nFormat: 1\nMode: Simple numbering\n\n#001 RENAMED \"a",
                ),
                ("FileCraft rename log.jpg", "FileCraft rename log"),
            ],
        );
        fs::create_dir(folder.join("folder.txt")).unwrap();
        let runs = find_runs(folder).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].log_name, numbering);
        assert_eq!(
            runs[0].details,
            Ok(RunDetails {
                mode: LogMode::Numbering,
                started: "2026-03-07 09:00:00".to_owned(),
                renamed: 1,
                finished: true,
            })
        );
        assert!(!runs[0].undone);
        assert_eq!(runs[1].log_name, "rename_log_damaged.txt");
        assert_eq!(
            runs[1].details,
            Err("line 5 of the log is damaged".to_owned())
        );

        let done = undo(folder, &numbering, at(2));
        assert_eq!(reasons(&done), [("00001.doc", None)]);
        assert!(find_runs(folder).unwrap()[0].undone);
        let undo_log = "undo_log_2026-03-07_090200.txt";
        assert_eq!(
            read_run(folder, undo_log).unwrap_err().to_string(),
            "the log of an undo cannot be undone"
        );
        let log = read_log(&folder.join(undo_log)).unwrap();
        assert_eq!(log.undoes.as_deref(), Some(numbering.as_str()));
        assert!(log.finished);
        assert_eq!(
            log.renamed()
                .map(|(from, to, _)| (from, to))
                .collect::<Vec<_>>(),
            [("00001.doc", "a.doc")]
        );
    }

    #[test]
    fn undoes_interrupted_runs_and_skips_unchecked_lines() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(folder, &[("00001.jpg", "a"), ("00002.jpg", "b")]);
        let print = Fingerprint::of(&folder.join("00001.jpg")).unwrap().unwrap();
        let modified = DateTime::<Utc>::from_timestamp_millis(print.modified_ms)
            .unwrap()
            .to_rfc3339();
        // A run that stopped after two files, the second without a size and time.
        fs::write(
            folder.join("numbering_log_x.txt"),
            format!(
                "FileCraft rename log\r\nFormat: 1\r\nMode: Simple numbering\r\nStarted: \
                 2026-03-07 08:00:00\r\n\r\n#001 RENAMED \"a.jpg\" -> \"00001.jpg\" size={} \
                 modified={modified}\r\n#002 RENAMED \"b.jpg\" -> \"00002.jpg\"\r\n",
                print.size
            ),
        )
        .unwrap();
        let runs = find_runs(folder).unwrap();
        assert!(matches!(&runs[0].details, Ok(details) if !details.finished));
        let done = undo(folder, "numbering_log_x.txt", at(0));
        assert_eq!(
            reasons(&done),
            [
                ("00002.jpg", Some(SkipReason::NotVerifiable)),
                ("00001.jpg", None),
            ]
        );
        assert_eq!(files(folder), owned(&[("00002.jpg", "b"), ("a.jpg", "a")]));
    }

    #[test]
    fn unlogged_renamings_are_reverted() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(folder, &[("new.jpg", "a")]);
        let error = revert_unlogged(folder, "old.jpg", "new.jpg", io::Error::other("disk full"));
        assert!(error.to_string().contains("renamed back to 'old.jpg'"));
        assert_eq!(files(folder), owned(&[("old.jpg", "a")]));
        // A name taken in the meantime is never replaced.
        write(folder, &[("new.jpg", "b")]);
        let error = revert_unlogged(folder, "old.jpg", "new.jpg", io::Error::other("disk full"));
        assert!(error.to_string().contains("which is not in the log"));
        assert_eq!(files(folder), owned(&[("new.jpg", "b"), ("old.jpg", "a")]));
    }
}
