//! Copying, moving and deleting on a thread of its own, telling the screen
//! how far it has come.

use anyhow::{Context, Result};
use skyward_files::{Cancelled, Clash, Ownership, Progress};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The most often how far the work has come is told.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

pub enum Task {
    Copy { from: PathBuf, into: PathBuf, clash: Clash },
    Move { from: PathBuf, into: PathBuf, clash: Clash },
    Delete { path: PathBuf },
}

pub enum End {
    Done,
    /// Stopped as asked.
    Stopped,
    Failed(String),
}

pub enum Event {
    Progress(Progress),
    Ended(End),
}

pub struct Worker {
    events: Receiver<Event>,
    /// Readable while events wait.
    wake: OwnedFd,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(task: Task) -> Result<Worker> {
        let mut fds = [0; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return Err(std::io::Error::last_os_error()).context("make the file worker's wake pipe");
        }
        let (wake, signal) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let thread = thread::Builder::new()
            .name("files".into())
            .spawn(move || {
                let tell = |event: Event| {
                    // The screen only stops listening once it has let the
                    // worker go.
                    let _ = tx.send(event);
                    unsafe { libc::write(signal.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
                };
                let mut told = Instant::now() - PROGRESS_EVERY;
                let mut last = None;
                let mut progress = |p: Progress| {
                    last = Some(p);
                    if told.elapsed() >= PROGRESS_EVERY {
                        told = Instant::now();
                        tell(Event::Progress(p));
                    }
                };
                let ended = match task {
                    Task::Copy { from, into, clash } => {
                        skyward_files::copy(&[from], &into, clash, Ownership::Kept, &stop, &mut progress)
                    }
                    Task::Move { from, into, clash } => {
                        skyward_files::move_into(&[from], &into, clash, Ownership::Kept, &stop, &mut progress)
                    }
                    Task::Delete { path } => skyward_files::delete(&[path], &stop, &mut progress),
                };
                // Work that stops or fails shows how far it came in the end.
                if let Some(p) = last {
                    tell(Event::Progress(p));
                }
                tell(Event::Ended(match ended {
                    Ok(()) => End::Done,
                    Err(e) if e.is::<Cancelled>() => End::Stopped,
                    Err(e) => End::Failed(format!("{e:#}")),
                }));
            })
            .context("start the file worker")?;
        Ok(Worker { events, wake, cancel, thread: Some(thread) })
    }

    pub fn fd(&self) -> RawFd {
        self.wake.as_raw_fd()
    }

    /// The events since the last call.
    pub fn events(&self) -> Vec<Event> {
        let mut drain = [0u8; 64];
        while unsafe { libc::read(self.wake.as_raw_fd(), drain.as_mut_ptr().cast(), drain.len()) } > 0 {}
        self.events.try_iter().collect()
    }

    /// Ask the work to stop; it does between two chunks of a file.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Worker {
    /// The screen lets the worker go once its work has ended.
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the file worker does not panic");
        }
    }
}
