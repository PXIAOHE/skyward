//! What a factory reset erases, erased the way a phone's own recovery
//! erases it, on a thread of its own.

use anyhow::{bail, ensure, Context, Result};
use skyward_block::fstab::{self, Entry};
use skyward_block::{dm, erase, mount};
use skyward_sys::bcb;
use skyward_vab::SnapshotState;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

const DATA: &str = "/data";
const CACHE: &str = "/cache";
const METADATA: &str = "/metadata";

/// A part erasing goes through, as it begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Data,
    Cache,
    /// /metadata, which holds the key the data partition is opened with.
    Metadata,
}

/// What a factory reset will erase on this phone, once it is known that it
/// can.
pub struct Plan {
    data: Entry,
    cache: Option<Entry>,
    metadata: Entry,
}

/// Find what a factory reset erases: the data partition, the cache
/// partition where the phone has one, and /metadata. The data partition is
/// opened through metadata encryption, and is erased only as such a
/// partition is.
pub fn factory_reset() -> Result<Plan> {
    // An update that has not finished merging keeps part of the slot the
    // phone runs from in /metadata.
    match skyward_vab::snapshot_state()? {
        SnapshotState::None => {}
        SnapshotState::Unknown => bail!("{METADATA} is not mounted, so whether an update is still being merged is not known"),
        state => bail!("an update is still being merged ({state:?}); start the system and let it finish first"),
    }
    let data = first(DATA)?;
    ensure!(
        data.extra("keydirectory").is_some(),
        "{DATA} is not opened through metadata encryption, which is the only way this recovery erases it"
    );
    Ok(Plan { data, cache: fstab::read(CACHE)?.into_iter().next(), metadata: first(METADATA)? })
}

impl Plan {
    /// The parts, in the order they are erased.
    pub fn steps(&self) -> Vec<Step> {
        let cache = self.cache.as_ref().map(|_| Step::Cache);
        std::iter::once(Step::Data).chain(cache).chain(std::iter::once(Step::Metadata)).collect()
    }

    /// Erase each part, telling `at` of each as it begins. The data
    /// partition is discarded whole, and the system makes it afresh under a
    /// new key the next time it starts. Unless `keep_memtag_mode`, the
    /// memory tagging asked of the bootloader is forgotten too.
    pub fn carry_out(self, keep_memtag_mode: bool, at: &mut dyn FnMut(Step)) -> Result<()> {
        at(Step::Data);
        close_data()?;
        erase::discard(Path::new(&fstab::device(&self.data)?))?;
        if let Some(cache) = &self.cache {
            at(Step::Cache);
            remake(cache)?;
        }
        at(Step::Metadata);
        remake(&self.metadata)?;
        if !keep_memtag_mode {
            bcb::forget_memtag_mode(&bcb::misc())?;
        }
        Ok(())
    }
}

/// The cache partition, where the phone has one.
pub fn cache() -> Result<Option<Entry>> {
    Ok(fstab::read(CACHE)?.into_iter().next())
}

/// The first entry the phone's table has for `mount_point`: the one the
/// system tries first.
fn first(mount_point: &str) -> Result<Entry> {
    fstab::read(mount_point)?.into_iter().next().with_context(|| format!("the phone's table has no {mount_point}"))
}

/// Make the filesystem an entry names afresh, with nothing mounted from it.
pub fn remake(entry: &Entry) -> Result<()> {
    let at = Path::new(&entry.mount_point);
    if mount::is_mount_point(at)? {
        mount::umount(at)?;
    }
    erase::make(Path::new(&fstab::device(entry)?), &entry.filesystem, &entry.extras)
}

/// Unmount the data partition and take away the device it was opened
/// through, where the recovery opened it.
fn close_data() -> Result<()> {
    let at = Path::new(DATA);
    if mount::is_mount_point(at)? {
        mount::umount(at)?;
    }
    if dm::status(skyward_crypto::data::MAPPED)?.is_some() {
        dm::remove(skyward_crypto::data::MAPPED)?;
    }
    Ok(())
}

/// What erasing tells the screen.
#[derive(Debug)]
pub enum Event {
    At(Step),
    /// Done, or why it stopped.
    Ended(Result<(), String>),
}

/// Erasing on a thread of its own, so the screen keeps up while it runs:
/// discarding a whole data partition can take minutes.
pub struct Eraser {
    events: Receiver<Event>,
    /// Readable while events wait.
    wake: OwnedFd,
    thread: Option<JoinHandle<()>>,
}

impl Eraser {
    /// Run `work` on its thread; what it tells `at`, and how it ends, come
    /// back as events.
    pub fn start(work: impl FnOnce(&mut dyn FnMut(Step)) -> Result<()> + Send + 'static) -> Result<Eraser> {
        let mut fds = [0; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return Err(std::io::Error::last_os_error()).context("make the eraser's wake pipe");
        }
        let (wake, signal) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        let (tx, events) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("eraser".into())
            .spawn(move || {
                let tell = |event: Event| {
                    // The screen only stops listening once it has let the
                    // eraser go.
                    let _ = tx.send(event);
                    unsafe { libc::write(signal.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
                };
                let ended = work(&mut |step| tell(Event::At(step))).map_err(|e| format!("{e:#}"));
                tell(Event::Ended(ended));
            })
            .context("start the eraser")?;
        Ok(Eraser { events, wake, thread: Some(thread) })
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
}

impl Drop for Eraser {
    /// The screen lets the eraser go once it has ended.
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the eraser does not panic");
        }
    }
}
