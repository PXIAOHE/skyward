//! Fetching one file from an address typed on the download page: on a thread
//! of its own, so the page keeps up with how far it has come.

use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// How much is read and written at a time.
const CHUNK: usize = 1 << 16;

/// How a fetch ended.
pub enum Ended {
    Done { path: PathBuf, bytes: u64 },
    Failed(String),
    Stopped,
}

pub struct Worker {
    ended: Receiver<Ended>,
    /// Readable once the fetch has ended.
    wake: OwnedFd,
    stop: Arc<AtomicBool>,
    done: Arc<AtomicU64>,
    /// How long the file is, or 0 where the server did not say.
    total: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(url: String, path: PathBuf, servers: skyward_net::http::Servers) -> Result<Worker> {
        let mut fds = [0; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return Err(std::io::Error::last_os_error()).context("make the fetching thread's wake pipe");
        }
        let (wake, signal) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        let (tx, ended) = mpsc::channel();
        let (stop_as, done_as, total_as) = (stop.clone(), done.clone(), total.clone());
        let thread = thread::Builder::new()
            .name("fetch".into())
            .spawn(move || {
                let ended_as = match fetch(&url, &path, &stop_as, &done_as, &total_as, servers) {
                    Ok(()) => Ended::Done { path: path.clone(), bytes: done_as.load(Ordering::Relaxed) },
                    Err(_) if stop_as.load(Ordering::Relaxed) => Ended::Stopped,
                    Err(e) => Ended::Failed(format!("{e:#}")),
                };
                let _ = tx.send(ended_as);
                unsafe { libc::write(signal.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
            })
            .context("start the fetching thread")?;
        Ok(Worker { ended, wake, stop, done, total, thread: Some(thread) })
    }

    pub fn fd(&self) -> RawFd {
        self.wake.as_raw_fd()
    }

    /// How many bytes have come, and how many the server named where it did.
    pub fn progress(&self) -> (u64, Option<u64>) {
        let total = self.total.load(Ordering::Relaxed);
        (self.done.load(Ordering::Relaxed), (total > 0).then_some(total))
    }

    /// How the fetch ended, once it has.
    pub fn ended(&self) -> Option<Ended> {
        let mut drain = [0u8; 8];
        while unsafe { libc::read(self.wake.as_raw_fd(), drain.as_mut_ptr().cast(), drain.len()) } > 0 {}
        self.ended.try_recv().ok()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The name a file from `url` is saved under: its last path segment, without
/// the query, and made of characters a filesystem takes.
pub fn name_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let last = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("");
    let cleaned: String = last
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    match cleaned.trim().is_empty() {
        true => "download.bin".to_string(),
        false => cleaned,
    }
}

fn fetch(
    url: &str,
    path: &PathBuf,
    stop: &AtomicBool,
    done: &AtomicU64,
    total: &AtomicU64,
    servers: skyward_net::http::Servers,
) -> Result<()> {
    // The names in the address are looked up with the servers the network the
    // phone joined gave: an empty list leaves nothing to ask.
    let agent = skyward_net::http::agent(servers);
    let head = agent.head(url).call().context("ask the server about the file")?;
    let status = head.status().as_u16();
    if status != 200 {
        bail!("the server answered {status}");
    }
    if let Some(length) = head
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
    {
        total.store(length, Ordering::Relaxed);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
    }
    let mut response = agent.get(url).call().context("start the download")?;
    let status = response.status().as_u16();
    if status != 200 {
        bail!("the server answered {status}");
    }
    let mut file = std::fs::File::create(path).with_context(|| format!("make {}", path.display()))?;
    let mut body = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; CHUNK];
    let mut have = 0u64;
    loop {
        if stop.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(path);
            bail!("stopped");
        }
        let n = body.read(&mut buf).context("download the file")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).with_context(|| format!("write {}", path.display()))?;
        have += n as u64;
        done.store(have, Ordering::Relaxed);
    }
    file.sync_all().with_context(|| format!("write {} through", path.display()))?;
    if have == 0 {
        drop(file);
        let _ = std::fs::remove_file(path);
        bail!("the server sent nothing");
    }
    Ok(())
}
