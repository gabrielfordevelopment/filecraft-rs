//! The File Rename tab: renames the files of one folder by date or by number, or gives the files of
//! an earlier renaming their original names back. It shows a preview of the first files before
//! renaming, of all files on request, and the outcome afterwards. The renaming itself lives in
//! `filecraft_core::file_rename`, the undo in `filecraft_core::rename_undo`.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use egui::{Align, Align2, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use filecraft_core::file_rename::{
    DateSource, FileResult, Outcome, Preview, RenameMode, RunFiles, SkipReason, rename_files,
};
use filecraft_core::rename_log::LogMode;
use filecraft_core::rename_undo::{RenameRun, find_runs, read_run, undo_files, undo_preview};

use crate::background::{Repaint, TaskFailed};
use crate::dialog::Dialog;
use crate::export_footer::{self, FooterButton, Stat, StatTone};
use crate::icons::{self, Icon, Tone};
use crate::requests::{ConfirmAction, TabRequest};
use crate::strings;
use crate::theme;
use crate::toast::Toast;
use crate::widgets::{self, ButtonKind};

/// What the tab does with the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    ByDate,
    Numbering,
    /// Gives the files of an earlier renaming their original names back.
    Undo,
}

impl Mode {
    fn rename_mode(self) -> Option<RenameMode> {
        match self {
            Self::ByDate => Some(RenameMode::ByDate),
            Self::Numbering => Some(RenameMode::Numbering),
            Self::Undo => None,
        }
    }
}

const MODES: [Mode; 3] = [Mode::ByDate, Mode::Numbering, Mode::Undo];
/// The widest the list of earlier renamings gets.
const RUN_COMBO_WIDTH: f32 = 440.0;
/// How many files a preview reads unless all of them are asked for. Reading a file can be slow on
/// network drives, so large folders are not read in full by default.
const PREVIEW_LIMIT: usize = 100;
const REFRESH: Icon = Icon {
    glyph: '\u{e5d5}',
    tone: Tone::Text,
};

/// What the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    /// What renaming would do.
    Preview,
    /// What the last run did.
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Files,
    Log,
}

enum RunEvent {
    Message(String),
    File(FileResult),
    Finished(io::Result<RunFiles>),
}

enum PreviewEvent {
    /// The number of files in the folder.
    Listed(usize),
    File(FileResult),
    /// The reading stopped at the end, at the limit or when asked to. The preview can go on from
    /// here.
    Paused(Box<Preview>),
    /// Undo: the renamings logged in the folder and the one the preview is for.
    Runs {
        runs: Vec<RenameRun>,
        chosen: Option<String>,
    },
    /// Undo: the preview is complete.
    Done,
    Failed(io::Error),
}

/// A preview reading files on a background thread. Dropping it stops the thread.
struct Previewing {
    receiver: Receiver<PreviewEvent>,
    stop: Arc<AtomicBool>,
}

impl Previewing {
    /// Asks the thread to stop before the next file. It hands the preview back when it stops.
    fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Previewing {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The folder and mode a preview was made for, and the log of the renaming an undo is for.
type PreviewKey = (PathBuf, Mode, Option<String>);

pub struct FileRenameTab {
    folder: Option<PathBuf>,
    mode: Mode,
    /// Undo: the renamings logged in the folder, newest first.
    runs: Vec<RenameRun>,
    /// Undo: the log of the renaming to undo.
    run: Option<String>,
    rows: Vec<FileResult>,
    list: ListKind,
    view: View,
    log: String,
    /// The preview reads every file instead of the first [`PREVIEW_LIMIT`]. Choosing another
    /// folder turns it off.
    show_all: bool,
    /// The number of files in the folder, known once the folder is listed.
    total: Option<usize>,
    /// The preview the rows belong to.
    shown_for: Option<PreviewKey>,
    previewing: Option<Previewing>,
    /// The preview between two readings, to go on from when more files are wanted.
    paused: Option<Box<Preview>>,
    preview_error: Option<String>,
    running: Option<Receiver<RunEvent>>,
}

impl Default for FileRenameTab {
    fn default() -> Self {
        Self {
            folder: None,
            mode: Mode::ByDate,
            runs: Vec::new(),
            run: None,
            rows: Vec::new(),
            list: ListKind::Preview,
            view: View::Files,
            log: String::new(),
            show_all: false,
            total: None,
            shown_for: None,
            previewing: None,
            paused: None,
            preview_error: None,
            running: None,
        }
    }
}

/// Counts of a preview or a result.
#[derive(Default)]
struct Counts {
    renamed: usize,
    skipped: usize,
    failed: usize,
}

impl FileRenameTab {
    fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for row in &self.rows {
            match row.outcome {
                Outcome::Renamed { .. } => counts.renamed += 1,
                Outcome::Skipped { .. } => counts.skipped += 1,
                Outcome::Failed { .. } => counts.failed += 1,
            }
        }
        counts
    }

    /// Starts a new preview when the folder or the mode changed, or when `force` is set. Goes on
    /// with the current preview when all files are wanted.
    fn update_preview(&mut self, ctx: &egui::Context, force: bool) {
        if self.running.is_some() {
            return;
        }
        let Some(folder) = self.folder.clone() else {
            self.rows.clear();
            self.total = None;
            self.shown_for = None;
            self.previewing = None;
            self.paused = None;
            return;
        };
        let run = (self.mode == Mode::Undo)
            .then(|| self.run.clone())
            .flatten();
        let key = (folder.clone(), self.mode, run.clone());
        if force || self.shown_for.as_ref() != Some(&key) {
            self.shown_for = Some(key);
            self.rows.clear();
            self.total = None;
            self.paused = None;
            self.list = ListKind::Preview;
            self.preview_error = None;
            match self.mode.rename_mode() {
                Some(mode) => self.read_files(ctx, move || {
                    Preview::new(&folder, mode, chrono::Local::now().naive_local()).map(Box::new)
                }),
                None => self.read_undo(ctx, folder, run),
            }
        } else if self.show_all
            && self.list == ListKind::Preview
            && self.previewing.is_none()
            && self.preview_error.is_none()
            && self.total.is_some_and(|total| self.rows.len() < total)
            && let Some(preview) = self.paused.take()
        {
            self.read_files(ctx, move || Ok(preview));
        }
    }

    /// Reads files for the preview `start` gives on a background thread: the first
    /// [`PREVIEW_LIMIT`] files, or all the rest when all files are wanted.
    fn read_files(
        &mut self,
        ctx: &egui::Context,
        start: impl FnOnce() -> io::Result<Box<Preview>> + Send + 'static,
    ) {
        let limit = (!self.show_all).then_some(PREVIEW_LIMIT);
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let repaint = Repaint::new(ctx);
        thread::spawn(move || {
            // The receiver is gone only when the preview is no longer wanted, which also sets the
            // stop flag.
            let finished = match start() {
                Ok(mut preview) => {
                    let _ = sender.send(PreviewEvent::Listed(preview.file_count()));
                    repaint.now();
                    preview.advance(limit, &thread_stop, &mut |row| {
                        let _ = sender.send(PreviewEvent::File(row));
                        repaint.soon();
                    });
                    PreviewEvent::Paused(preview)
                }
                Err(error) => PreviewEvent::Failed(error),
            };
            let _ = sender.send(finished);
            repaint.now();
        });
        self.previewing = Some(Previewing { receiver, stop });
    }

    /// Lists the renamings logged in `folder` and works out on a background thread what undoing
    /// `wanted`, or else the newest one, would do.
    fn read_undo(&mut self, ctx: &egui::Context, folder: PathBuf, wanted: Option<String>) {
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let repaint = Repaint::new(ctx);
        thread::spawn(move || {
            // The receiver is gone only when the preview is no longer wanted, which also sets the
            // stop flag.
            let result = (|| -> io::Result<()> {
                let runs = find_runs(&folder)?;
                let chosen = wanted
                    .filter(|name| runs.iter().any(|run| run.log_name == *name))
                    .or_else(|| runs.first().map(|run| run.log_name.clone()));
                let _ = sender.send(PreviewEvent::Runs {
                    runs,
                    chosen: chosen.clone(),
                });
                repaint.now();
                let Some(chosen) = chosen else {
                    return Ok(());
                };
                let log = read_run(&folder, &chosen)?;
                let _ = sender.send(PreviewEvent::Listed(log.renamed().count()));
                undo_preview(&folder, &log, &thread_stop, &mut |row| {
                    let _ = sender.send(PreviewEvent::File(row));
                    repaint.soon();
                });
                Ok(())
            })();
            let _ = sender.send(match result {
                Ok(()) => PreviewEvent::Done,
                Err(error) => PreviewEvent::Failed(error),
            });
            repaint.now();
        });
        self.previewing = Some(Previewing { receiver, stop });
    }

    fn poll_preview(&mut self) {
        let Some(previewing) = &self.previewing else {
            return;
        };
        let mut finished = false;
        loop {
            match previewing.receiver.try_recv() {
                Ok(PreviewEvent::Listed(total)) => self.total = Some(total),
                Ok(PreviewEvent::File(row)) => self.rows.push(row),
                Ok(PreviewEvent::Paused(preview)) => {
                    self.paused = Some(preview);
                    finished = true;
                    break;
                }
                Ok(PreviewEvent::Runs { runs, chosen }) => {
                    self.runs = runs;
                    // The preview is for the run it chose, so choosing nothing new is no change.
                    if let Some(key) = &mut self.shown_for {
                        key.2.clone_from(&chosen);
                    }
                    self.run = chosen;
                }
                Ok(PreviewEvent::Done) => {
                    finished = true;
                    break;
                }
                Ok(PreviewEvent::Failed(error)) => {
                    self.preview_error = Some(error.to_string());
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.preview_error = Some(TaskFailed.to_string());
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.previewing = None;
        }
    }

    /// Stops reading all files. The rows read so far stay, and "Show all files" goes on from
    /// there.
    fn stop_preview(&mut self) {
        self.show_all = false;
        if let Some(previewing) = &self.previewing {
            previewing.stop();
        }
    }

    /// The preview shows every file of the folder.
    fn preview_complete(&self) -> bool {
        self.previewing.is_none() && self.total == Some(self.rows.len())
    }

    /// Renames the files, or gives them their original names back, on a background thread. The
    /// list and the log follow the progress.
    pub fn start(&mut self, ctx: &egui::Context) {
        let Some(folder) = self.folder.clone() else {
            return;
        };
        let mode = self.mode.rename_mode();
        let run = self.run.clone();
        if mode.is_none() && run.is_none() {
            return;
        }
        self.rows.clear();
        self.log.clear();
        self.list = ListKind::Result;
        self.paused = None;
        let (sender, receiver) = mpsc::channel();
        let repaint = Repaint::new(ctx);
        thread::spawn(move || {
            let now = chrono::Local::now().naive_local();
            // The receiver is gone only when the result is no longer wanted.
            let send = |event: RunEvent| {
                let _ = sender.send(event);
                repaint.soon();
            };
            let mut show = |message: &str| send(RunEvent::Message(message.to_owned()));
            let mut on_file = |file: &FileResult| send(RunEvent::File(file.clone()));
            let result = match (mode, run) {
                (Some(mode), _) => rename_files(&folder, mode, now, &mut show, &mut on_file),
                (None, Some(run)) => undo_files(&folder, &run, now, &mut show, &mut on_file),
                (None, None) => unreachable!("checked before the thread starts"),
            };
            send(RunEvent::Finished(result));
            repaint.now();
        });
        self.running = Some(receiver);
    }

    /// Takes the progress of a run. Returns the notification or error once it is over.
    fn poll_run(&mut self) -> Option<TabRequest> {
        let receiver = self.running.as_ref()?;
        let mut finished = None;
        loop {
            match receiver.try_recv() {
                Ok(RunEvent::Message(message)) => {
                    self.log.push_str(&message);
                    self.log.push('\n');
                }
                Ok(RunEvent::File(file)) => self.rows.push(file),
                Ok(RunEvent::Finished(result)) => {
                    finished = Some(result.map_err(|error| error.to_string()));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err(strings::FILE_RENAME_STOPPED.to_owned()));
                    break;
                }
            }
        }
        let result = finished?;
        self.running = None;
        let undo = self.mode == Mode::Undo;
        if !undo {
            // An undo offers the newest renaming first, which is this one now.
            self.run = None;
        }
        Some(match result {
            Ok(files) => {
                let counts = self.counts();
                let mut facts = vec![if undo {
                    strings::restored_count(counts.renamed)
                } else {
                    strings::renamed_count(counts.renamed)
                }];
                if counts.skipped > 0 {
                    facts.push(strings::skipped_count(counts.skipped));
                }
                if counts.failed > 0 {
                    facts.push(strings::failed_count(counts.failed));
                }
                let title = if undo {
                    strings::TOAST_NAMES_RESTORED
                } else {
                    strings::TOAST_FILES_RENAMED
                };
                TabRequest::Notify(
                    Toast::success(title, String::new())
                        .with_file(files.report.unwrap_or(files.log))
                        .with_facts(facts),
                )
            }
            Err(message) => TabRequest::ShowDialog(Dialog::notification(
                strings::COMMON_ERROR_TITLE,
                if undo {
                    strings::file_rename_undo_error(&message)
                } else {
                    strings::file_rename_error(&message)
                },
                icons::ERROR,
            )),
        })
    }

    /// Why the rename button is disabled, or `None` when renaming can start.
    fn blocker(&self) -> Option<&'static str> {
        if self.folder.is_none() {
            Some(strings::FILE_RENAME_NEEDS_FOLDER)
        } else if self.previewing.is_some() {
            Some(strings::FILE_RENAME_WAIT_FOR_PREVIEW)
        } else if self.list == ListKind::Result {
            Some(strings::FILE_RENAME_REFRESH_FIRST)
        } else if self.preview_error.is_some() {
            Some(if self.mode == Mode::Undo {
                strings::FILE_RENAME_UNDO_PREVIEW_FAILED
            } else {
                strings::FILE_RENAME_PREVIEW_FAILED
            })
        } else if self.mode == Mode::Undo && self.run.is_none() {
            Some(strings::FILE_RENAME_UNDO_NO_RUN)
        } else if self.mode == Mode::Undo && !self.preview_complete() {
            Some(strings::FILE_RENAME_REFRESH_FIRST)
        } else if self.preview_complete() && self.counts().renamed == 0 {
            Some(if self.mode == Mode::Undo {
                strings::FILE_RENAME_UNDO_NOTHING
            } else {
                strings::FILE_RENAME_NOTHING_TO_RENAME
            })
        } else {
            None
        }
    }

    /// The renaming an undo is for.
    fn chosen_run(&self) -> Option<&RenameRun> {
        let name = self.run.as_ref()?;
        self.runs.iter().find(|run| run.log_name == *name)
    }

    fn confirmation(&self) -> Option<TabRequest> {
        let folder = self.folder.as_ref()?.display().to_string();
        let count = self.counts().renamed;
        let total = self.total.unwrap_or_default();
        let shown = self.rows.len();
        let message = match (self.mode, self.preview_complete()) {
            (Mode::ByDate, true) => strings::file_rename_by_date_confirm(count),
            (Mode::Numbering, true) => strings::file_rename_numbering_confirm(count),
            (Mode::ByDate, false) => strings::file_rename_by_date_confirm_partial(total, shown),
            (Mode::Numbering, false) => {
                strings::file_rename_numbering_confirm_partial(total, shown)
            }
            (Mode::Undo, _) => {
                let started = self
                    .chosen_run()
                    .and_then(|run| run.details.as_ref().ok())
                    .map_or("", |details| details.started.as_str());
                let dialog = Dialog::confirmation(
                    strings::FILE_RENAME_UNDO_CONFIRM_TITLE,
                    strings::file_rename_undo_confirm(started, count),
                    icons::WARNING,
                )
                .with_primary(strings::FILE_RENAME_UNDO_CONFIRM_BUTTON, ButtonKind::Danger)
                .with_emphasis(strings::FILE_RENAME_CANNOT_UNDO)
                .with_detail(folder);
                return Some(TabRequest::Confirm(dialog, ConfirmAction::RenameFiles));
            }
        };
        let dialog =
            Dialog::confirmation(strings::FILE_RENAME_CONFIRM_TITLE, message, icons::WARNING)
                .with_primary(strings::FILE_RENAME_CONFIRM_BUTTON, ButtonKind::Danger)
                .with_detail(folder);
        Some(TabRequest::Confirm(dialog, ConfirmAction::RenameFiles))
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Option<TabRequest> {
        let ctx = ui.ctx().clone();
        self.poll_preview();
        let mut request = self.poll_run();
        self.update_preview(&ctx, false);

        let full = ui.available_rect_before_wrap();
        let running = self.running.is_some();
        let mut refresh = false;

        // Folder and mode.
        let folder_rect = Rect::from_min_size(
            full.min + vec2(5.0, 5.0),
            vec2(full.width() - 10.0, widgets::floating_box_height(ui)),
        );
        // Bottom aligned, so the button lines up with the box under its label.
        let mut folder_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(folder_rect)
                .layout(Layout::right_to_left(Align::Max)),
        );
        let select_folder = widgets::button(
            &mut folder_ui,
            strings::BROWSE_BUTTON,
            ButtonKind::Secondary,
            !running,
            egui::Vec2::ZERO,
        )
        .clicked()
            && !running;
        folder_ui.add_space(10.0);
        let text = self
            .folder
            .as_ref()
            .map(|folder| folder.display().to_string())
            .unwrap_or_default();
        let width = folder_ui.available_width();
        widgets::floating_path_box(
            &mut folder_ui,
            &text,
            width,
            strings::FILE_RENAME_FOLDER_LABEL,
        );

        // The card ends below its content, so the height is only an upper bound.
        let mode_rect = Rect::from_min_max(
            pos2(folder_rect.left(), folder_rect.bottom() + 10.0),
            pos2(folder_rect.right(), full.bottom()),
        );
        let busy = self.previewing.is_some() || running;
        let (_, mode_frame) = widgets::card_in(
            ui,
            mode_rect,
            false,
            |ui| {
                widgets::card_title(ui, strings::FILE_RENAME_MODE_HEADER);
            },
            |_| {},
            |ui| {
                ui.horizontal(|ui| {
                    let options = [
                        strings::FILE_RENAME_BY_DATE_OPTION,
                        strings::FILE_RENAME_NUMBERING_OPTION,
                        strings::FILE_RENAME_UNDO_OPTION,
                    ];
                    let current = MODES
                        .iter()
                        .position(|mode| *mode == self.mode)
                        .unwrap_or(0);
                    if let Some(index) = widgets::segmented(ui, &options, current)
                        && !running
                    {
                        self.mode = MODES[index];
                    }
                    ui.add_space(12.0);
                    let description = match self.mode {
                        Mode::ByDate => strings::FILE_RENAME_BY_DATE_DESCRIPTION,
                        Mode::Numbering => strings::FILE_RENAME_NUMBERING_DESCRIPTION,
                        Mode::Undo => {
                            if self.folder.is_some() && !self.runs.is_empty() {
                                if let Some(run) = self.run_combo(ui, !busy) {
                                    self.run = Some(run);
                                }
                                ui.add_space(12.0);
                            }
                            strings::FILE_RENAME_UNDO_DESCRIPTION
                        }
                    };
                    let galley = widgets::ellipsized(
                        ui,
                        description,
                        theme::body(theme::BODY_SIZE),
                        theme::palette().text_muted,
                        ui.available_width(),
                    );
                    let elided = galley.elided;
                    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
                    ui.painter()
                        .galley(rect.min, galley, theme::palette().text_muted);
                    if elided {
                        response.on_hover_text(description);
                    }
                });
            },
        );

        // Footer: counts and the rename button.
        let counts = self.counts();
        let button_height = export_footer::button_height(ui);
        let footer_height = button_height + 20.0;
        let footer_area =
            Rect::from_min_max(pos2(full.left(), full.bottom() - footer_height), full.max);
        export_footer::separator(ui, footer_area);
        let footer = footer_area.shrink(10.0);
        let mut footer_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(footer)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let undo = self.mode == Mode::Undo;
        let (button_text, running_text) = if undo {
            (
                strings::FILE_RENAME_UNDO_BUTTON,
                strings::FILE_RENAME_UNDOING,
            )
        } else {
            (strings::FILE_RENAME_BUTTON, strings::FILE_RENAME_RUNNING)
        };
        let button = FooterButton {
            text: button_text,
            disabled_reason: self.blocker(),
            busy_text: running.then_some(running_text),
        };
        // The widest of all texts, so the button keeps its size when the mode changes.
        let width = [
            strings::FILE_RENAME_BUTTON,
            strings::FILE_RENAME_RUNNING,
            strings::FILE_RENAME_UNDO_BUTTON,
            strings::FILE_RENAME_UNDOING,
        ]
        .iter()
        .map(|text| widgets::button_width(ui, text))
        .fold(0.0, f32::max)
            + 20.0;
        if export_footer::export_button(&mut footer_ui, &button, width, button_height) {
            request = self.confirmation().or(request);
        }
        let stats = match self.list {
            ListKind::Preview => vec![
                match self.total {
                    Some(total) if total != self.rows.len() => Stat {
                        value: strings::shown_of(self.rows.len(), total),
                        label: strings::FILE_RENAME_FILES_LABEL,
                        tone: StatTone::Neutral,
                    },
                    _ => Stat::count(
                        self.rows.len(),
                        strings::FILE_RENAME_FILES_LABEL,
                        StatTone::Neutral,
                    ),
                },
                Stat::count(
                    counts.renamed,
                    if undo {
                        strings::FILE_RENAME_TO_RESTORE_LABEL
                    } else {
                        strings::FILE_RENAME_TO_RENAME_LABEL
                    },
                    StatTone::Highlight,
                ),
                Stat::count(
                    counts.skipped,
                    strings::FILE_RENAME_SKIPPED_LABEL,
                    StatTone::Neutral,
                ),
            ],
            ListKind::Result => vec![
                Stat::count(
                    counts.renamed,
                    if undo {
                        strings::FILE_RENAME_RESTORED_LABEL
                    } else {
                        strings::FILE_RENAME_RENAMED_LABEL
                    },
                    StatTone::Highlight,
                ),
                Stat::count(
                    counts.skipped,
                    strings::FILE_RENAME_SKIPPED_LABEL,
                    StatTone::Neutral,
                ),
                Stat::count(
                    counts.failed,
                    strings::FILE_RENAME_ERRORS_LABEL,
                    if counts.failed == 0 {
                        StatTone::Neutral
                    } else {
                        StatTone::Warning
                    },
                ),
            ],
        };
        export_footer::paint_stats(ui, footer.left(), footer.center().y, &stats);

        // The list or the log fills the rest.
        let list_rect = Rect::from_min_max(
            pos2(folder_rect.left(), mode_frame.bottom() + 10.0),
            pos2(folder_rect.right(), footer.top() - 10.0),
        );
        let title = match self.list {
            ListKind::Preview => strings::FILE_RENAME_PREVIEW_HEADER,
            ListKind::Result => strings::FILE_RENAME_RESULT_HEADER,
        };
        let mut view = self.view;
        let has_folder = self.folder.is_some();
        let mut list_action = None;
        widgets::card_in(
            ui,
            list_rect,
            true,
            |ui| {
                widgets::card_title(ui, title);
                ui.add_space(10.0);
                let views = [
                    strings::FILE_RENAME_FILES_VIEW,
                    strings::FILE_RENAME_LOG_VIEW,
                ];
                let current = usize::from(view == View::Log);
                if let Some(index) = widgets::segmented(ui, &views, current) {
                    view = if index == 0 { View::Files } else { View::Log };
                }
                ui.add_space(6.0);
                let can_refresh = !busy && has_folder;
                refresh = widgets::icon_button(ui, REFRESH, 28.0, 20.0, can_refresh)
                    .on_hover_text(strings::FILE_RENAME_REFRESH_TOOLTIP)
                    .clicked()
                    && can_refresh;
                if busy {
                    ui.add_space(10.0);
                    let text = if running {
                        running_text
                    } else {
                        strings::FILE_RENAME_READING
                    };
                    widgets::busy_label(ui, text);
                }
            },
            |_| {},
            |ui| match self.view {
                View::Files => list_action = self.files_ui(ui),
                View::Log => self.log_ui(ui),
            },
        );
        ui.advance_cursor_after_rect(full);
        self.view = view;

        match list_action {
            Some(ListAction::ShowAll) => self.show_all = true,
            Some(ListAction::Stop) => self.stop_preview(),
            None => {}
        }
        if refresh {
            self.update_preview(&ctx, true);
        }
        if select_folder
            && let Some(folder) = rfd::FileDialog::new()
                .set_title(strings::FILE_RENAME_SELECT_FOLDER_TITLE)
                .pick_folder()
        {
            self.folder = Some(folder);
            self.show_all = false;
            self.log.clear();
            self.view = View::Files;
            self.runs.clear();
            self.run = None;
        }
        request
    }

    /// The list of the renamings logged in the folder. Returns the log of the one picked.
    fn run_combo(&self, ui: &mut Ui, enabled: bool) -> Option<String> {
        let width = RUN_COMBO_WIDTH.min(ui.available_width() * 0.6);
        let selected = self.chosen_run().map(run_label).unwrap_or_default();
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                widgets::combo_value(ui, width, None, &selected, None)
            })
            .inner;
        let response = match self.chosen_run() {
            Some(run) => response.on_hover_text(strings::file_rename_run_tooltip(&run.log_name)),
            None => response,
        };
        let labels: Vec<String> = self.runs.iter().map(run_label).collect();
        let items: Vec<widgets::ComboItem<'_>> = labels
            .iter()
            .map(|text| widgets::ComboItem {
                icon: None,
                text,
                trailing: None,
            })
            .collect();
        widgets::combo_popup(&response, width, &items)
            .map(|index| self.runs[index].log_name.clone())
    }

    /// The table of files: current name, new name and date source. Returns the button clicked in
    /// its last row.
    fn files_ui(&self, ui: &mut Ui) -> Option<ListAction> {
        let colors = theme::palette();
        let body = ui.max_rect();
        let undo = self.mode == Mode::Undo;
        let empty = if self.folder.is_none() {
            Some(if undo {
                strings::FILE_RENAME_UNDO_NEEDS_FOLDER
            } else {
                strings::FILE_RENAME_EMPTY_NO_FOLDER
            })
        } else if let Some(error) = &self.preview_error {
            Some(error.as_str())
        } else if self.rows.is_empty() && self.previewing.is_none() && self.running.is_none() {
            Some(if undo {
                strings::FILE_RENAME_UNDO_EMPTY
            } else {
                strings::FILE_RENAME_EMPTY_FOLDER
            })
        } else {
            None
        };
        if let Some(message) = empty {
            let mut job = egui::text::LayoutJob::simple(
                message.to_owned(),
                theme::body(theme::BODY_SIZE),
                colors.text_muted,
                (body.width() - 40.0).max(100.0),
            );
            job.halign = Align::Center;
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(
                pos2(body.center().x, body.top() + 22.0),
                galley,
                colors.text_muted,
            );
            return None;
        }

        let by_date = self.mode == Mode::ByDate;
        let source_width = if by_date { 110.0 } else { 0.0 };
        let name_width = ((body.width() - source_width) / 2.0).max(60.0);
        let columns = [
            (strings::FILE_RENAME_CURRENT_NAME_COLUMN, name_width),
            (
                if undo {
                    strings::FILE_RENAME_ORIGINAL_NAME_COLUMN
                } else {
                    strings::FILE_RENAME_NEW_NAME_COLUMN
                },
                name_width,
            ),
            (strings::FILE_RENAME_DATE_COLUMN, source_width),
        ];

        let header_height = widgets::text_height(ui, theme::semibold(theme::BODY_SIZE)) + 8.0;
        let (header, _) = ui.allocate_exact_size(vec2(body.width(), header_height), Sense::hover());
        let mut x = header.left() + 6.0;
        for (title, width) in columns {
            if width > 0.0 {
                ui.painter().text(
                    pos2(x, header.center().y),
                    Align2::LEFT_CENTER,
                    title,
                    theme::semibold(theme::BODY_SIZE),
                    colors.text_muted,
                );
            }
            x += width;
        }
        ui.painter().line_segment(
            [
                pos2(header.left(), header.bottom() - 0.5),
                pos2(header.right(), header.bottom() - 0.5),
            ],
            egui::Stroke::new(1.0, colors.border_subtle),
        );

        let row_height = widgets::control_height(ui);
        let end = self.list_end();
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("file_rename_rows")
            .auto_shrink([false, false])
            .stick_to_bottom(self.running.is_some())
            .show_rows(
                ui,
                row_height,
                self.rows.len() + usize::from(end.is_some()),
                |ui, range| {
                    for index in range {
                        match (self.rows.get(index), &end) {
                            (Some(row), _) => self.row_ui(ui, row, &columns, row_height),
                            (None, Some(end)) => action = end_row_ui(ui, end, row_height),
                            (None, None) => {}
                        }
                    }
                },
            );
        action
    }

    /// What the last row of the preview says, if the preview does not show every file.
    fn list_end(&self) -> Option<ListEnd> {
        // An undo preview always reads the whole log.
        if self.list != ListKind::Preview || self.mode == Mode::Undo {
            return None;
        }
        let total = self.total?;
        match &self.previewing {
            Some(_) if self.show_all => Some(ListEnd::Reading {
                read: self.rows.len(),
                total,
            }),
            None if self.rows.len() < total && self.preview_error.is_none() => {
                Some(ListEnd::Limited {
                    shown: self.rows.len(),
                    total,
                })
            }
            _ => None,
        }
    }

    /// One file: its name, its new name, or why it keeps its name, and the date source.
    fn row_ui(&self, ui: &mut Ui, row: &FileResult, columns: &[(&str, f32); 3], height: f32) {
        let colors = theme::palette();
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        if response.hovered() {
            ui.painter().rect_filled(rect, 3, colors.list_hover);
        }
        let (name_color, new_name, new_name_color, source) = match &row.outcome {
            Outcome::Renamed {
                new_name, source, ..
            } => (colors.text, new_name.clone(), colors.text, *source),
            Outcome::Skipped { source, reason } => (
                colors.text_muted,
                skip_label(reason),
                colors.text_muted,
                *source,
            ),
            Outcome::Failed { error } => (
                colors.text,
                strings::file_rename_error_label(error),
                colors.danger,
                None,
            ),
        };
        let cells = [
            (row.name.as_str(), name_color),
            (new_name.as_str(), new_name_color),
            (source.map_or("-", source_name), colors.text_muted),
        ];
        let mut x = rect.left() + 6.0;
        for ((text, color), (_, width)) in cells.iter().zip(columns.iter()) {
            if *width > 0.0 {
                let galley = widgets::ellipsized(
                    ui,
                    text,
                    theme::body(theme::BODY_SIZE),
                    *color,
                    width - 12.0,
                );
                ui.painter().galley(
                    pos2(x, rect.center().y - galley.size().y / 2.0),
                    galley,
                    *color,
                );
            }
            x += width;
        }
        response.on_hover_text(self.row_tooltip(row));
    }

    /// A sentence about what happens, or happened, to a file.
    fn row_tooltip(&self, row: &FileResult) -> String {
        let name = &row.name;
        let undo = self.mode == Mode::Undo;
        match (&row.outcome, self.list) {
            (Outcome::Renamed { new_name, .. }, ListKind::Preview) if undo => {
                strings::file_rename_will_restore(name, new_name)
            }
            (Outcome::Renamed { new_name, .. }, ListKind::Result) if undo => {
                strings::file_rename_was_restored(name, new_name)
            }
            (Outcome::Renamed { new_name, .. }, ListKind::Preview) => {
                strings::file_rename_will_rename(name, new_name)
            }
            (Outcome::Renamed { new_name, .. }, ListKind::Result) => {
                strings::file_rename_was_renamed(name, new_name)
            }
            (Outcome::Skipped { reason, .. }, _) => {
                strings::file_rename_skipped(name, &reason.describe())
            }
            (Outcome::Failed { error }, _) => strings::file_rename_failed(name, error),
        }
    }

    /// The read-only log of the last run, scrolled to the newest message.
    fn log_ui(&self, ui: &mut Ui) {
        let colors = theme::palette();
        let rect = ui.max_rect();
        ui.painter().rect(
            rect,
            4,
            colors.surface_alt,
            egui::Stroke::new(1.0, colors.border),
            egui::StrokeKind::Inside,
        );
        let mut inner = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(6.0))
                .layout(Layout::top_down(Align::Min)),
        );
        egui::ScrollArea::vertical()
            .id_salt("file_rename_log")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(&mut inner, |ui| {
                let (text, color) = if self.log.is_empty() {
                    (strings::FILE_RENAME_EMPTY_LOG, colors.text_muted)
                } else {
                    (self.log.as_str(), colors.text)
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .font(theme::body(theme::BODY_SIZE))
                            .color(color),
                    )
                    .wrap()
                    .selectable(true),
                );
            });
    }
}

/// A button in the last row of the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListAction {
    ShowAll,
    Stop,
}

/// The last row of a preview that does not show every file.
enum ListEnd {
    /// Only the first files are shown.
    Limited { shown: usize, total: usize },
    /// All files are being read.
    Reading { read: usize, total: usize },
}

/// The last row of the preview: how many files it shows, with a button to show all of them or to
/// stop reading.
fn end_row_ui(ui: &mut Ui, end: &ListEnd, height: f32) -> Option<ListAction> {
    let colors = theme::palette();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().line_segment(
        [
            pos2(rect.left(), rect.top() + 0.5),
            pos2(rect.right(), rect.top() + 0.5),
        ],
        egui::Stroke::new(1.0, colors.border_subtle),
    );
    let (text, button, tooltip, action) = match *end {
        ListEnd::Limited { shown, total } => (
            strings::file_rename_showing_first(shown, total),
            strings::FILE_RENAME_SHOW_ALL_BUTTON,
            strings::FILE_RENAME_SHOW_ALL_TOOLTIP,
            ListAction::ShowAll,
        ),
        ListEnd::Reading { read, total } => (
            strings::file_rename_reading_all(read, total),
            strings::FILE_RENAME_STOP_BUTTON,
            strings::FILE_RENAME_STOP_TOOLTIP,
            ListAction::Stop,
        ),
    };
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    if matches!(end, ListEnd::Reading { .. }) {
        widgets::busy_label(&mut row, &text);
    } else {
        widgets::label(
            &mut row,
            &text,
            theme::body(theme::BODY_SIZE),
            colors.text_muted,
        );
    }
    row.add_space(10.0);
    let mut style = widgets::ButtonStyle::new(ButtonKind::Secondary, egui::Vec2::ZERO);
    style.height = Some(height - 6.0);
    widgets::styled_button(&mut row, button, style, true)
        .on_hover_text(tooltip)
        .clicked()
        .then_some(action)
}

fn source_name(source: DateSource) -> &'static str {
    match source {
        DateSource::Exif => strings::FILE_RENAME_SOURCE_METADATA,
        DateSource::FileName => strings::FILE_RENAME_SOURCE_FILE_NAME,
        DateSource::LastModified => strings::FILE_RENAME_SOURCE_MODIFIED,
    }
}

/// A short reason shown instead of the new name of a skipped file.
fn skip_label(reason: &SkipReason) -> String {
    match reason {
        SkipReason::UnsupportedType => strings::FILE_RENAME_SKIP_TYPE.to_owned(),
        SkipReason::TextFile => strings::FILE_RENAME_SKIP_TEXT.to_owned(),
        SkipReason::NameTaken(name) => strings::file_rename_skip_taken(name),
        SkipReason::AlreadyNamed => strings::FILE_RENAME_SKIP_SAME.to_owned(),
        SkipReason::AlreadyRestored => strings::FILE_RENAME_SKIP_RESTORED.to_owned(),
        SkipReason::Changed => strings::FILE_RENAME_SKIP_CHANGED.to_owned(),
        SkipReason::Missing => strings::FILE_RENAME_SKIP_MISSING.to_owned(),
        SkipReason::NotVerifiable => strings::FILE_RENAME_SKIP_UNCHECKED.to_owned(),
    }
}

/// A renaming in the list of an undo: when it ran, how and how many files it renamed.
fn run_label(run: &RenameRun) -> String {
    match &run.details {
        Ok(details) => strings::file_rename_run_label(
            &details.started,
            details.mode == LogMode::ByDate,
            details.renamed,
            details.finished,
            run.undone,
        ),
        Err(_) => strings::file_rename_damaged_run_label(&run.log_name),
    }
}
