//! Work that runs on a background thread and reports back to the UI thread.

use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

/// How often progress repaints the UI at most.
const PROGRESS_REPAINT_INTERVAL: Duration = Duration::from_millis(30);

/// A running background computation. Polling returns its result once it is done.
pub struct Task<T> {
    receiver: Receiver<T>,
}

impl<T: Send + 'static> Task<T> {
    /// Runs `work` on a new thread and repaints the UI when it finishes.
    pub fn spawn(ctx: &egui::Context, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            // The receiver is gone only when the result is no longer wanted.
            let _ = sender.send(work());
            ctx.request_repaint();
        });
        Self { receiver }
    }

    /// Returns the result once, or `Err` when the thread ended without a result.
    pub fn poll(&self) -> Option<Result<T, TaskFailed>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(Ok(result)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(TaskFailed)),
        }
    }
}

/// The background thread stopped without a result, for example after a panic.
#[derive(Debug, Clone, Copy)]
pub struct TaskFailed;

impl std::fmt::Display for TaskFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The background operation stopped unexpectedly.")
    }
}

/// Repaints the UI from a background thread. Progress reported for every item of a fast loop
/// repaints only every few milliseconds, so the thread does not keep the UI busy.
pub struct Repaint {
    ctx: egui::Context,
    last: Cell<Option<Instant>>,
}

impl Repaint {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            ctx: ctx.clone(),
            last: Cell::new(None),
        }
    }

    /// Repaints for progress, unless a repaint was asked for a moment ago.
    pub fn soon(&self) {
        let now = Instant::now();
        let recent = self
            .last
            .get()
            .is_some_and(|last| now.duration_since(last) < PROGRESS_REPAINT_INTERVAL);
        if !recent {
            self.last.set(Some(now));
            self.ctx.request_repaint();
        }
    }

    /// Repaints right away, for example when the work is done.
    pub fn now(&self) {
        self.ctx.request_repaint();
    }
}
