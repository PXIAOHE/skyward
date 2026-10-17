//! Installing what a computer sends with `adb sideload`: A/B update
//! packages, which the phone's own update_engine applies, and partition
//! images, which are written here.

pub mod image;
pub mod package;

pub use image::{ImageKind, Ramdisk};
use skyward_vab::{Slot, SnapshotState};

/// What the installer tells the screen, in the order it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// USB is ready for `adb sideload`.
    Waiting,
    /// The computer started sending a package of this many bytes, which is
    /// being looked at.
    Checking { size: u64 },
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The update is in `slot`, which starts next.
    Installed { slot: Slot },
    /// The image is in the partition of these slots.
    Flashed { partition: &'static str, slots: Vec<Slot> },
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
    /// A zip that installs itself by running the script inside it.
    Script,
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
    /// the data partition unreadable.
    OlderPatch { package: String, installed: String },
    /// It asks for the data partition to be wiped afterwards.
    Wipe,
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
    /// The computer stopped sending: the cable came out or `adb sideload`
    /// was stopped.
    Disconnected,
    /// The file changed on the computer while it was being sent.
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
    /// Leave sideload without installing anything.
    Cancel,
}
