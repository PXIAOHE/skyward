//! Installing a package that a computer sends with `adb sideload`, or that
//! is a file on the phone: A/B update packages, which the phone's own
//! update_engine applies, zips that install themselves by running the
//! script inside them, and partition images, which are written here.

pub mod adb;
pub mod fuse;
pub mod image;
pub mod keep;
pub mod ota;
pub mod package;
pub mod script;

#[cfg(target_os = "android")]
mod ffs;
#[cfg(target_os = "android")]
mod run;

pub use image::{ImageKind, Ramdisk};
#[cfg(target_os = "android")]
pub use run::{Detach, Installer};
use skyward_vab::{Slot, SnapshotState};

/// Where the installer keeps what update_engine wrote while it last
/// installed an update.
pub const ENGINE_LOG: &str = "/tmp/update_engine.log";

/// Where the installer keeps what the install script it last ran showed
/// and asked for, with what it and the programs it ran wrote.
pub const SCRIPT_LOG: &str = "/tmp/install-script.log";

/// What the installer tells the screen, in the order it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// USB is ready for `adb sideload`.
    Waiting,
    /// A package of this many bytes is being looked at: one the computer
    /// started sending, or a file on the phone.
    Checking { size: u64 },
    /// An update package passed the checks and waits for
    /// [`Command::Install`] before anything is written: the version it
    /// brings, and whether all user data is to be erased before that
    /// system starts. One that erases always waits, and so does one from a
    /// file on the phone, which a single tap picks.
    Ready { version: Option<String>, wipe: bool },
    /// An update package passed the checks and is being installed; the
    /// version it brings.
    Installing { version: Option<String> },
    /// Super has no room to keep the running system beside the new one, so
    /// the update writes over it: until it finishes, the phone may not
    /// start.
    Overwriting,
    /// A partition image, held until [`Command::Flash`] says where it goes:
    /// the partitions of this phone it fits, the likeliest first.
    Image { kind: ImageKind, size: u64, partitions: Vec<&'static str> },
    /// A zip that installs itself by running the script inside it, held
    /// until [`Command::Install`]: a script may write over any partition,
    /// so it always waits.
    Script,
    /// The install script has started.
    Running,
    /// A line the install script shows.
    Print(String),
    Progress(Stage),
    Finished(Outcome),
}

/// How far the work is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// update_engine writes the new system: the bytes of its payload read
    /// so far.
    Writing { done: u64, total: u64 },
    /// It reads back what it wrote, one partition after another.
    Verifying { index: usize, count: usize, partition: String },
    /// It runs the package's own last steps and switches slots.
    Finishing,
    /// An image is written and read back: bytes so far, of all.
    Flashing { done: u64, total: u64 },
    /// The install script runs, as far as it says it is, in thousandths.
    Script { thousandths: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The update is in `slot`, which starts next; `wipe` when all user data
    /// is to be erased before it does.
    Installed { slot: Slot, wipe: bool },
    /// The image is in the partition of these slots.
    Flashed { partition: &'static str, slots: Vec<Slot> },
    /// The install script ran to its end; `wipe_cache` when it asked for
    /// the cache partition to be erased after.
    Ran { wipe_cache: bool },
    /// Stopped before anything was written.
    Cancelled,
    /// Not installed, for this reason; nothing was written.
    Refused(Refusal),
    Failed(Failure),
}

/// Why a package is not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A file this recovery cannot install.
    Unknown,
    /// An update package that is not for A/B phones; the type it names.
    NotAb(String),
    /// Made for other devices: the ones it names.
    OtherDevice(String),
    /// Made for particular phones, not this one.
    OtherSerial,
    /// An incremental package, which needs snapshots that recovery cannot
    /// make on a Virtual A/B phone.
    Incremental,
    /// Older than the installed system: both build times.
    Older { package: i64, installed: i64 },
    /// Its security patch is older than the installed one, which leaves
    /// the data partition unreadable unless it is erased.
    OlderPatch { package: String, installed: String },
    /// An image for partitions this phone does not have.
    NoPartition,
    /// Too little charge to install safely: the level, and whether a
    /// charger is in.
    Battery { level: u8, charging: bool },
    /// An earlier update is still in flight; writing over it would break
    /// it.
    Pending(SnapshotState),
    /// The computer's adb is too old to send a package a block at a time.
    OldHost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// update_engine stopped with this error code.
    Engine(i32),
    /// The install script ended with this status, or 128 and the signal
    /// that ended it, as a shell tells it.
    Script(i32),
    /// The computer stopped sending: the cable came out or `adb sideload`
    /// was stopped.
    Disconnected,
    /// Reading the file on the phone failed, for this reason: the USB
    /// drive came out, or the storage failed.
    Unreadable(String),
    /// The package changed where it is while it was being read: on the
    /// computer as it was sent, or the file on the phone.
    Changed,
    /// Anything else: the error and its causes.
    Error(String),
}

/// What the screen tells the installer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Write the image held in [`Event::Image`] to `partition` in these
    /// slots.
    Flash { partition: &'static str, slots: Vec<Slot> },
    /// Install the update held in [`Event::Ready`], or run the script held
    /// in [`Event::Script`].
    Install,
    /// Stop without installing anything.
    Cancel,
}
