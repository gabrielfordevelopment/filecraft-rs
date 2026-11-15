//! Separate image tools. This tab does not read or modify the file renaming state.

mod view;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use filecraft_core::image_tools::{self, FileResult, Options, ProgressEvent, RunResult};

use crate::background::{Repaint, Task};
use crate::dialog::Dialog;
use crate::icons;
use crate::requests::TabRequest;
use crate::strings;
use crate::theme;
use crate::toast::Toast;

struct Running {
    task: Task<io::Result<RunResult>>,
    progress: Receiver<ProgressEvent>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct ImageToolsTab {
    source: Option<PathBuf>,
    options: Options,
    width_enabled: bool,
    height_enabled: bool,
    width: u32,
    height: u32,
    preview: Option<Task<io::Result<Vec<FileResult>>>>,
    running: Option<Running>,
    rows: Vec<FileResult>,
    results: bool,
    output: Option<PathBuf>,
    progress_total: Option<usize>,
    stopped: bool,
}

impl Default for ImageToolsTab {
    fn default() -> Self {
        Self {
            source: None,
            options: Options::default(),
            width_enabled: true,
            height_enabled: true,
            width: image_tools::DEFAULT_MAX_DIMENSION,
            height: image_tools::DEFAULT_MAX_DIMENSION,
            preview: None,
            running: None,
            rows: Vec::new(),
            results: false,
            output: None,
            progress_total: None,
            stopped: false,
        }
    }
}

impl ImageToolsTab {
    /// Poll from the main window, including while another tab is selected.
    pub fn poll(&mut self) -> Option<TabRequest> {
        if let Some(running) = &self.running {
            for event in running.progress.try_iter() {
                match event {
                    ProgressEvent::Listed { total } => self.progress_total = Some(total),
                    ProgressEvent::File(row) => self.rows.push(row),
                }
            }
            if let Some(result) = running.task.poll() {
                self.running = None;
                self.stopped = true;
                return Some(match result {
                    Ok(Ok(run)) => {
                        self.progress_total = Some(run.total);
                        self.stopped = run.cancelled;
                        self.rows = run.files;
                        self.output = Some(run.output);
                        let saved = self.rows.iter().filter(|row| row.outcome.is_ok()).count();
                        let failed = self.rows.len() - saved;
                        let title = if run.cancelled {
                            strings::IMAGE_STOPPED
                        } else {
                            strings::IMAGE_COMPLETE
                        };
                        let summary = strings::image_summary(saved, failed);
                        if failed > 0 {
                            error_request(format!("{summary}\n\n{}", strings::IMAGE_FAILED))
                        } else {
                            TabRequest::Notify(Toast::success(title, summary))
                        }
                    }
                    Ok(Err(error)) => error_request(error.to_string()),
                    Err(error) => error_request(error.to_string()),
                });
            }
        }
        if let Some(task) = &self.preview
            && let Some(result) = task.poll()
        {
            self.preview = None;
            return match result {
                Ok(Ok(rows)) => {
                    self.rows = rows;
                    if self.rows.is_empty() {
                        Some(error_request(strings::IMAGE_NO_FILES.to_owned()))
                    } else {
                        None
                    }
                }
                Ok(Err(error)) => Some(error_request(error.to_string())),
                Err(error) => Some(error_request(error.to_string())),
            };
        }
        None
    }

    fn start(&mut self, ctx: &egui::Context, preview: bool) -> Option<TabRequest> {
        let Some(source) = self.source.clone() else {
            return Some(error_request(strings::IMAGE_NO_SOURCE.to_owned()));
        };
        if let Err(error) = self.options.validate() {
            return Some(error_request(error.to_string()));
        }
        let options = self.options.clone();
        self.rows.clear();
        self.output = None;
        self.progress_total = None;
        self.stopped = false;
        self.results = !preview;
        if preview {
            self.preview = Some(Task::spawn(ctx, move || {
                image_tools::preview_folder(&source, &options)
            }));
        } else {
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let (sender, progress) = mpsc::channel();
            let repaint = Repaint::new(ctx);
            let task = Task::spawn(ctx, move || {
                let font = if options.add_date {
                    Some(theme::image_stamp_font()?)
                } else {
                    None
                };
                image_tools::process_folder(
                    &source,
                    &options,
                    font.as_ref(),
                    &worker_cancel,
                    |event| {
                        let _ = sender.send(event);
                        repaint.soon();
                    },
                )
            });
            self.running = Some(Running {
                task,
                progress,
                cancel,
            });
        }
        None
    }
}

fn error_request(message: String) -> TabRequest {
    TabRequest::ShowDialog(Dialog::notification(
        strings::COMMON_ERROR_TITLE,
        message,
        icons::ERROR,
    ))
}
