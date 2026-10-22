use crate::canvas::{Canvas, Run};
use crate::color::Rgba;
use crate::fling::{Fling, Tracker};
use crate::focus::{self, Group, Spot, Stop};
use crate::font::Fonts;
use crate::geom::Rect;
use crate::glow::{self, GlowClock};
use crate::motion::{self, Spring};
use crate::pages::{self, Ctx};
use crate::sprite::{Blurs, Sprite};
use crate::search;
use crate::term::{self, Term, TermKey};
use crate::theme::Theme;
use crate::widgets::{self, Bar, Cap, Layer};
use skyward_info::DeviceInfo;
use skyward_install::{Event, ImageKind, Outcome, Reflash, Stage};
use skyward_vab::{BootSlots, Hold, Slot, SnapshotState, Standing};

/// The pages of the recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Decrypt,
    Home,
    Settings,
    Reboot,
    Wipe,
    /// Mounting and unmounting the slot's partitions.
    Mount,
    Install,
    /// A place's folders and the files in them that can be installed, to
    /// choose one.
    Choose(Place),
    /// Installing a package, which the computer sends with `adb sideload`
    /// or which is a file chosen in a place, and how it went.
    Installing,
    About,
    Specs,
    /// How long the screen stays on with nothing touched.
    ScreenOff,
    /// Finding a feature or partition by name, typed on the screen's own
    /// keyboard.
    Search,
    Backup,
    /// The backups found, to restore one.
    Restore,
    /// One backup, and what to restore from it.
    RestoreBackup,
    /// Backing up or restoring under way, and how it ended.
    BackupJob,
    /// Tools for the experienced: files, a terminal, the logs.
    Advanced,
    Files,
    /// One file, and what can be done with it.
    FileInfo,
    Terminal,
    Language,
    TimeZone,
    /// Saving the logs, as a bug report asks for them.
    Logs,
    /// Where an update the system installed stands, and merging it into
    /// the slot it went to.
    Update,
    /// Skyward's own licence.
    License,
    /// The interface typeface and the terms it is used under.
    FontLicense,
    /// Which slot the phone starts from next.
    Slot,
    /// The phone's own fastbootd serving the computer's fastboot, and where
    /// to go from it. Its only page, worked with the keys: the partitions
    /// the touchscreen runs from are left out while fastbootd rewrites
    /// super.
    Fastboot,
    /// Erasing what the system asked to erase before the recovery started,
    /// all user data or the cache alone: how far it has come, and where to
    /// go if it stops.
    Erase,
    /// The system could not start Android and asks whether to erase all
    /// user data, which may be what keeps it from starting.
    CantStart,
    /// Something the system asked of the recovery that it does not do.
    Unhandled,
    /// A new name for the entry the file page shows, typed on the screen's
    /// keyboard.
    Rename,
    /// The folder an entry is copied or moved to already holds that name;
    /// what to do is the user's to choose.
    Clash,
    /// Copying, moving or deleting under way, and how it ended.
    FileJob,
}

/// Where a file to install is chosen from, and where backups go and are
/// found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The owner's internal storage, which opens with the data partition.
    Storage,
    /// A USB drive plugged into the port, which the runtime shows read
    /// only.
    UsbDrive,
}

/// The USB drive plugged in, as the runtime found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbDrive {
    /// Shown, for a file on it to be chosen.
    Shown,
    /// Nothing on it could be read: why.
    Unreadable(String),
}

/// Where the package being installed comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// The computer, with `adb sideload`.
    #[default]
    Computer,
    /// A file chosen in a place.
    File(Place),
}

/// What can be done with an entry picked in the file manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOp {
    Copy,
    Move,
}

/// An entry picked to copy or move, while the file manager looks for the
/// folder it goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub op: FileOp,
    /// Where the entry is.
    pub path: String,
}

/// What to do with an entry of the same name already where one goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClashChoice {
    /// The new entry takes the old one's place.
    Replace,
    /// The old entry stays, and the new one is left out.
    Skip,
    /// The new entry gets a name of its own beside the old one.
    KeepBoth,
}

/// Work on files as its page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileTask {
    Copy { path: String, into: String },
    Move { path: String, into: String },
    Delete { path: String },
}

/// Why an entry was not renamed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameRefused {
    /// Another entry in the folder has the name, as the folder compares
    /// names: shared storage takes names in any case.
    Taken,
    /// What went wrong, as the system says it.
    Failed(String),
}

/// How work on files ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobEnd {
    Done,
    /// Stopped as asked, with what was done by then left as it is.
    Stopped,
    Failed(String),
}

/// Work on files under way: what it is, how far it has come, and how it
/// ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileJob {
    pub task: FileTask,
    /// Bytes and entries, each as done and out of how many; none while the
    /// work still measures what it covers.
    pub progress: Option<((u64, u64), (u64, u64))>,
    pub outcome: Option<JobEnd>,
    /// Stopping was asked for and is under way.
    pub stopping: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Vibrate,
    /// Offer internal storage to a computer over USB.
    FileTransfer,
    /// Compress a backup.
    Compress,
    /// Read a backup back once it is written.
    Verify,
    /// Write Skyward back into the recovery partitions an install writes
    /// over.
    KeepRecovery,
}

/// What a backup can take, in the order the backup page lists it.
pub const BACKUP_PARTS: [&str; 4] = ["启动分区", "系统分区", "data 分区", "基带与 EFS"];

/// A backup found in a place's Skyward/backups folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupEntry {
    pub place: Place,
    /// Its folder's name: when it was begun.
    pub name: String,
    /// What it holds, as its manifest records it, or why that cannot be
    /// read: a backup that did not finish has none.
    pub held: Result<Held, String>,
}

/// What a backup holds, and where it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    /// When it was made, in Unix seconds.
    pub made: i64,
    /// The bytes its files take.
    pub size: u64,
    /// Which of [`BACKUP_PARTS`] it holds.
    pub parts: [bool; 4],
    /// The phone it was made on, by its codename, the system it ran, as
    /// its version reads, and the slot it was made from.
    pub device: String,
    pub system: Option<String>,
    pub slot: Option<Slot>,
}

/// Backing up or restoring, as its page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupJob {
    /// Restoring a backup, rather than making one.
    pub restore: bool,
    /// Where the backup goes, or comes from.
    pub place: Place,
    /// Each part is read back to check it: all once they are written, for a
    /// backup; all before any is written, for a restore.
    pub checks: bool,
    /// The parts it goes through, by name with their bytes; none while it
    /// still measures them.
    pub plan: Vec<(String, u64)>,
    pub at: Option<BackupAt>,
    pub outcome: Option<BackupEnd>,
    /// Stopping was asked for and is under way.
    pub stopping: bool,
}

/// The part under way, by its place in the plan, and how far it has come:
/// bytes done of how many in this pass over it, and whether this is the
/// pass that checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackupAt {
    pub item: usize,
    pub done: u64,
    pub of: u64,
    pub checking: bool,
}

/// How backing up or restoring ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupEnd {
    /// A backup made, in the folder of this name.
    Saved(String),
    /// A backup restored, but for what it held in the folders of users gone
    /// since, which only the system makes: where those were.
    Restored { unmade: Vec<String> },
    /// Stopped as asked: a backup stopped leaves nothing of itself, and a
    /// restore stops only while nothing is written.
    Stopped,
    Refused(BackupRefusal),
    /// What went wrong, as the system says it.
    Failed(String),
}

/// Why backing up or restoring did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupRefusal {
    /// Too little room where the backup goes: the bytes it takes, and the
    /// bytes free.
    Room { needed: u64, free: u64 },
    /// An update is in flight, and the slot's partitions in super are its
    /// to change.
    Update(SnapshotState),
    /// The backup was made on another phone, by its codename.
    OtherPhone(String),
    /// This partition is not the size it was backed up at, as after an
    /// update.
    Resized(String),
    /// The data partition has been made afresh since the backup, as a
    /// factory reset makes it: the files backed up would not open in it.
    DataReset,
}

/// An entry of a folder, as the file manager lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub name: String,
    pub kind: FileKind,
    /// Bytes; for a link, the link's own.
    pub size: u64,
    /// Last changed, in Unix seconds.
    pub modified: i64,
    /// Type and permission bits, as `st_mode` holds them.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A folder, and how many entries it holds when it can be read.
    Folder { items: Option<usize> },
    File,
    /// A symbolic link: where it points, and whether that is a folder.
    Link { target: String, folder: bool },
    /// A device, socket or pipe.
    Special,
}

/// A folder as a list of its entries shows it: where it is, what is in it
/// as the runtime read it or why it could not, and when it was read, in
/// Unix seconds: dates in the list are told against that day.
#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub path: String,
    pub entries: Result<Vec<FileRow>, String>,
    pub listed: i64,
    /// The folders left for one inside them, outermost first, with how far
    /// each was scrolled.
    left: Vec<(String, f32)>,
}

impl Folder {
    /// The folder at `path`, not read yet.
    pub fn new(path: &str) -> Folder {
        Folder { path: path.into(), entries: Ok(Vec::new()), listed: 0, left: Vec::new() }
    }

    /// The entry at this place in the list.
    fn entry(&self, i: usize) -> &FileRow {
        &self.entries.as_ref().expect("an entry touched is one the list shows")[i]
    }

    /// Where the entry of this name in the folder is.
    fn inside(&self, name: &str) -> String {
        std::path::Path::new(&self.path).join(name).to_string_lossy().into_owned()
    }

    /// The folder this one is in; none for the root.
    fn up(&self) -> Option<String> {
        std::path::Path::new(&self.path).parent().map(|p| p.to_string_lossy().into_owned())
    }

    /// The folder this many levels below the root, by its place in the
    /// path shown over the list; none for this folder itself.
    fn ancestor(&self, depth: usize) -> Option<String> {
        let path = std::path::Path::new(&self.path);
        let names: Vec<_> = path.components().filter(|c| matches!(c, std::path::Component::Normal(_))).collect();
        if depth >= names.len() {
            return None;
        }
        let mut up = if path.has_root() { std::path::PathBuf::from("/") } else { std::path::PathBuf::new() };
        up.extend(&names[..depth]);
        Some(up.to_string_lossy().into_owned())
    }

    /// Going into a folder inside this one: this one keeps how far it was
    /// scrolled, for coming back.
    fn leave(&mut self, scroll: f32) {
        self.left.push((self.path.clone(), scroll));
    }

    /// How far `path` is scrolled as it takes this folder's place: where it
    /// was left, for a folder gone back up to; its top for any other.
    fn scroll_for(&mut self, path: &str) -> f32 {
        let kept = self.left.iter().position(|(left, _)| left == path);
        let scroll = kept.map_or(0.0, |k| self.left[k].1);
        self.left.truncate(kept.unwrap_or(self.left.len()));
        scroll
    }
}

/// Whether a list in a place shows an entry: a folder, or a file the
/// installer may take by its name, an update package or a partition
/// image. Hidden entries are left out, as the stock file manager leaves
/// them out.
fn installable(e: &FileRow) -> bool {
    if e.name.starts_with('.') {
        return false;
    }
    match e.kind {
        FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => true,
        FileKind::File | FileKind::Link { folder: false, .. } => std::path::Path::new(&e.name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("img")),
        FileKind::Special => false,
    }
}

/// A partition the mount page lists: its name, where it sits, and how it
/// shows now. The runtime fills these in; the page draws them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountRow {
    /// The partition's base name in super, such as `system`: the row's
    /// title, and what the runtime acts on.
    pub base: String,
    pub point: String,
    pub mounted: bool,
    /// The programs using it, which keep it from being unmounted.
    pub holders: Vec<String>,
}

/// A mount or unmount the runtime could not carry out. The mount page says
/// so until the next try, or until it is left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountFailure {
    /// Mounting, rather than unmounting.
    pub mounting: bool,
    pub base: String,
    /// The error and its causes.
    pub detail: String,
}

/// Which control answers a user's screen lock, by the kind of lock it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keys {
    Pattern,
    Digits,
    Letters,
}

/// Where the data partition stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Data {
    /// Not opened; the decrypt page opens it as it shows.
    #[default]
    Closed,
    /// Being opened as far as it opens with nothing a user knows, which is
    /// also when each user's screen lock is read.
    Opening,
    /// Open, with this user's own files waiting on their screen lock,
    /// answered with `keys`; `checking` while what they entered is checked.
    Locked { user: u32, keys: Keys, checking: bool },
    /// Open, every user's own files with it.
    Open,
    /// Not opened, and why.
    Failed(String),
    /// Erased whole: nothing opens until the system makes it afresh, as it
    /// next starts.
    Erased,
}

impl Data {
    /// Whether the owner's internal storage can be read: the partition is
    /// open, and so are the owner's own files, which open first.
    pub fn storage_open(&self) -> bool {
        match self {
            Data::Open => true,
            // The owner is user 0.
            Data::Locked { user, .. } => *user != 0,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    Reboot,
    Wipe,
    Flash,
    Backup,
    Restore,
    /// Start next from the slot chosen on the slot page.
    Slot,
    /// Erase all user data, as the system offered when it could not start.
    FactoryReset,
    /// Delete the entry the file page shows.
    DeleteEntry,
    /// Delete the backup the restore page shows.
    DeleteBackup,
    /// Install the update that waits to be agreed to.
    Install,
    /// Install an update that erases all user data after it.
    InstallWipe,
    /// Run the install script of the zip that waits to be agreed to.
    RunScript,
    /// Merge the update that waits in the slot the phone runs from.
    Merge,
}

/// What the wipe page erases: everything a factory reset erases, or any of
/// the parts after it on their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wipe {
    All,
    /// Each app's own data, for every user.
    AppData,
    /// The files users share between apps.
    Media,
    /// Each app's caches, for every user.
    AppCaches,
    SystemCaches,
    /// What ART compiled ahead of time.
    Art,
    /// The cache partition, on phones that have one.
    Cache,
    Logs,
    /// The updates the system downloaded.
    Updates,
}

/// What the wipe page offers, in the order it lists them.
pub const WIPES: [Wipe; 9] =
    [Wipe::All, Wipe::AppData, Wipe::Media, Wipe::AppCaches, Wipe::SystemCaches, Wipe::Art, Wipe::Logs, Wipe::Updates, Wipe::Cache];

/// A part of the phone that erasing goes through, in the order it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The data partition, whole.
    Data,
    Cache,
    /// /metadata, which holds the key the data partition is opened with.
    Metadata,
    /// The parts of the data partition the wipe page names.
    AppData,
    Media,
    AppCaches,
    SystemCaches,
    Art,
    Logs,
    Updates,
}

/// Erasing as its page shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Erase {
    /// Everything users keep on the phone, as a factory reset erases it.
    pub reset: bool,
    /// The system asked for it before the recovery started; once it is
    /// done, the phone leaves on its own.
    pub asked: bool,
    pub parts: Vec<Part>,
    /// How many of `parts` are erased; the one after them is under way.
    pub done: usize,
    /// How far the part under way has come, entries of how many, where it
    /// tells.
    pub progress: Option<(u64, u64)>,
    /// How erasing ended: done, or why it stopped.
    pub outcome: Option<Result<(), EraseStop>>,
}

/// Why erasing did not start, or stopped short.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EraseStop {
    /// An update has to be finished by the system first.
    Update(Hold),
    /// The system does not make an emptied data partition afresh as it
    /// starts, so it is not erased this way.
    NotRebuilt,
    /// The phone has no cache partition.
    NoCache,
    /// The data partition is erased already, and holds nothing until the
    /// system makes it afresh.
    Erased,
    /// The data partition did not open, so nothing in it was erased: why.
    Closed(String),
    /// These users' own files are locked, and what was chosen is partly in
    /// them: nothing was erased.
    Locked(Vec<u32>),
    /// Erasing this part went wrong: what the system said.
    Failed(Part, String),
}

/// What the system asked of the recovery before it started, as the pages
/// that answer it show it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Asked {
    /// Why, as the system put it.
    pub reason: Option<String>,
    /// Power off once the work is done, instead of starting the system.
    pub shutdown_after: bool,
    /// Work the recovery does not do, as it was asked.
    pub unhandled: Option<String>,
}

/// Which slots an image goes to, by the slot the phone runs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Current,
    Other,
    Both,
}

impl Pick {
    pub const ALL: [Pick; 3] = [Pick::Current, Pick::Other, Pick::Both];

    /// The slots this means on a phone running from `current`.
    pub fn slots(self, current: Slot) -> Vec<Slot> {
        match self {
            Pick::Current => vec![current],
            Pick::Other => vec![current.other()],
            Pick::Both => vec![Slot::A, Slot::B],
        }
    }
}

/// Where installing stands, as the installer last told.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Install {
    /// No installer runs.
    #[default]
    Idle,
    /// USB is being set up for sideload.
    Starting,
    Waiting,
    Checking { size: u64 },
    /// An image waits to be told where it goes.
    Choosing(Choice),
    /// An update waits for the user to agree: the version it brings, and
    /// whether all user data is erased after it.
    Confirming { version: Option<String>, wipe: bool },
    /// `overwriting` says whether the update writes over the running
    /// system.
    Updating { version: Option<String>, stage: Option<Stage>, overwriting: bool },
    Flashing { partition: &'static str, stage: Option<Stage> },
    /// A zip with an install script waits for the user to agree to run it.
    ConfirmingScript,
    /// Its install script is made ready and then, once `running`, runs; how
    /// far it says it is, once it says.
    Scripting { stage: Option<Stage>, running: bool },
    /// `damage` is what a failure may have left broken.
    Finished { outcome: Outcome, damage: Option<Damage> },
    /// The page was left and the installer is stopping, handing USB back
    /// if it had it.
    Ending,
}

/// What a failed install may have left broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// The update had begun writing over the running system.
    System,
    /// The image had begun going into this partition.
    Partition(&'static str),
    /// The install script had begun to run, and may have written to any
    /// partition.
    Script,
}

/// An image, and where it goes so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub kind: ImageKind,
    pub size: u64,
    /// The partitions it fits, the likeliest first.
    pub partitions: Vec<&'static str>,
    /// Which of them is chosen.
    pub partition: usize,
    pub pick: Pick,
}

/// Where the reboot page sends the device, in the order it lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    System,
    Recovery,
    Fastboot,
    Bootloader,
    Edl,
    PowerOff,
}

pub const TARGETS: [Target; 6] = [Target::System, Target::Recovery, Target::Fastboot, Target::Bootloader, Target::Edl, Target::PowerOff];

/// Where the fastboot page goes, in the order it lists them: the volume
/// keys move between them and the power key takes one.
pub const FASTBOOT_CHOICES: [(&str, Action); 4] = [
    ("重启到系统", Action::RebootTo(Target::System)),
    ("进入恢复模式", Action::EnterRecovery),
    ("重启到 Bootloader", Action::RebootTo(Target::Bootloader)),
    ("关机", Action::RebootTo(Target::PowerOff)),
];

/// How far a computer has the USB port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbLink {
    /// No cable, or nothing answering at its other end.
    Unplugged,
    /// Plugged in, with the computer still setting the port up.
    Connecting,
    /// Set up by the computer, which can send commands.
    Connected,
}

/// Fastboot as its page shows it: whether the phone's fastbootd runs and
/// how far a computer has the port, as the runtime last read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fastboot {
    pub running: bool,
    pub link: UsbLink,
}

/// How long the screen stays on with nothing touched, in milliseconds, in
/// the order the page lists them; `None` keeps it on.
pub const SCREEN_OFF: [Option<u64>; 7] = [Some(15_000), Some(30_000), Some(60_000), Some(120_000), Some(300_000), Some(600_000), None];

/// The time zones the clock and the times on the pages can keep, in the
/// order their page lists them: the name, the cities it covers, and the
/// zone's name in the tz database.
pub const TIME_ZONES: [(&str, &str, &str); 9] = [
    ("中国标准时间", "北京、上海", "Asia/Shanghai"),
    ("香港时间", "香港", "Asia/Hong_Kong"),
    ("台北时间", "台北", "Asia/Taipei"),
    ("日本标准时间", "东京", "Asia/Tokyo"),
    ("协调世界时", "UTC", "UTC"),
    ("英国时间", "伦敦", "Europe/London"),
    ("中欧时间", "柏林、巴黎", "Europe/Berlin"),
    ("美国东部时间", "纽约", "America/New_York"),
    ("美国太平洋时间", "洛杉矶", "America/Los_Angeles"),
];

/// What a control does when touched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Open(Screen),
    /// Leave the decrypt page for the page it was opened from; the page
    /// the recovery started on, for the home page without keeping it
    /// behind.
    Skip,
    /// Try opening the data partition again, after it did not open.
    OpenData,
    Back,
    Theme { dark: bool },
    Toggle(Switch),
    RebootTarget(usize),
    /// Choose or leave out what the wipe page erases.
    WipeItem(Wipe),
    /// The brightness track: touching or dragging it sets the level.
    Brightness,
    /// Mount or unmount the mount page's partition at this index.
    MountToggle(usize),
    /// A swipe-to-confirm track; dragging the knob to the far end confirms.
    Confirm(Confirm),
    /// The pattern grid on the unlock page.
    Pattern,
    /// Start receiving a package over USB.
    Sideload,
    /// Send the image to one of the partitions it fits.
    FlashPartition(usize),
    /// Send the image to these slots.
    FlashSlots(Pick),
    /// Leave the recovery for this target at once.
    RebootTo(Target),
    /// Stop fastboot and bring the recovery's own pages up, without
    /// restarting.
    EnterRecovery,
    /// Leave what the system asked of the recovery for the recovery's own
    /// pages.
    Dismiss,
    /// Keep the screen on for this choice in [`SCREEN_OFF`].
    ScreenOff(usize),
    /// Keep the time zone at this place in [`TIME_ZONES`].
    TimeZone(usize),
    /// Tick or untick a part of the backup, by its place in
    /// [`BACKUP_PARTS`].
    BackupPart(usize),
    /// Send the backup to this place.
    BackupTo(Place),
    /// Open a backup found, by its place in the list, to restore it.
    OpenBackup(usize),
    /// Tick or untick a part of the backup being restored.
    RestorePart(usize),
    /// Stop backing up, or checking a backup before it is restored.
    StopBackup,
    /// Tick or untick a log to export: the recovery's, the kernel's.
    LogPart(usize),
    /// Save the logs to /tmp, or to internal storage.
    LogsTo(usize),
    /// Save the logs ticked where they are to go.
    ExportLogs,
    /// Pick this slot to start next; a swipe then switches to it.
    PickSlot(Slot),
    /// A key of the on-screen keyboard.
    Type(Cap),
    /// A key of the terminal's own row, above the keyboard.
    TermKey(TermKey),
    /// Empty the search field.
    ClearSearch,
    /// Leave the search, by the round button beside its field.
    CloseSearch,
    /// Open the file manager's entry at this place in the folder.
    OpenEntry(usize),
    /// Open the entry at this place in a place's folder: a folder to look
    /// in, or the file to install.
    ChooseEntry(usize),
    /// Go up to the folder this many levels below the top of the list
    /// shown, by its place in the path shown over the list.
    OpenAncestor(usize),
    /// Pick the entry the file page shows to copy or move, and look for
    /// where it goes.
    PickEntry(FileOp),
    /// Copy or move the entry picked into the folder shown.
    Paste,
    /// Put the entry picked down again.
    Unpick,
    /// Give the entry the file page shows a new name.
    RenameEntry,
    /// Ask to delete the entry the file page shows, or the backup the
    /// restore page shows; a swipe then confirms.
    AskDelete,
    /// How to go on when a name is taken where an entry goes.
    Resolve(ClashChoice),
    /// Stop copying, moving or deleting.
    StopJob,
    /// A control whose work is not wired up yet; touching it says so.
    Unavailable(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

/// A rendered page in page coordinates. The status-bar band at its top is
/// left empty, and the `<`, the titles and the bar are laid over the page
/// as it scrolls.
pub struct Frame {
    pub canvas: Canvas,
    pub hits: Vec<Hit>,
    pub title: &'static str,
    pub back: bool,
    /// The page opens with its title in large type, scrolling away with it.
    pub header: bool,
    /// Where the page's content starts; once it slides under the top of
    /// the screen, the bar comes up over it.
    pub content_top: f32,
    /// The page sits on the About page's moving field.
    pub hero: Option<HeroArt>,
    /// Centre and span of the pattern grid, on the unlock page.
    pub pattern: Option<(f32, f32, f32)>,
    /// The colour of the page itself, past the canvas and under the bar.
    pub bg: Rgba,
    /// A list too long to draw whole: its rows are drawn as they scroll
    /// into view, below the canvas's content and past its end.
    pub rows: Option<Rows>,
    /// A strip that scrolls with the page until it reaches the bar, then
    /// stays under it.
    pub pinned: Option<Pinned>,
    /// A terminal's screen and the height of its rows: a drag on it scrolls
    /// back through what the shell wrote, rather than moving the page.
    pub scrollback: Option<(Rect, f32)>,
    /// A strip held at the bottom of the screen over the page; the page
    /// scrolls far enough for nothing to stay under it.
    pub dock: Option<Dock>,
    /// A text field's blinking cursor, in page coordinates.
    pub caret: Option<widgets::Caret>,
}

/// A strip held at the bottom of the screen over the page, such as the
/// file manager's choice of where a picked entry goes.
pub struct Dock {
    pub canvas: Canvas,
    /// In the strip's own coordinates.
    pub hits: Vec<Hit>,
}

/// The rows of a list drawn as they come into view, the way a list view
/// binds its rows: where each lies on the page and what touching one does,
/// for rows that answer touch.
pub struct Rows {
    /// Page y of each row's top, then of the last row's bottom; whole
    /// pixels, so the rows meet without seams.
    pub edges: Vec<f32>,
    pub action: Option<fn(usize) -> Action>,
}

impl Rows {
    pub fn count(&self) -> usize {
        self.edges.len() - 1
    }

    pub fn bottom(&self) -> f32 {
        *self.edges.last().expect("a list has its top edge at least")
    }

    /// The row under page height `y`.
    pub fn at(&self, y: f32) -> Option<usize> {
        if y < self.edges[0] || y >= self.bottom() {
            return None;
        }
        Some(self.edges.partition_point(|&e| e <= y) - 1)
    }

    /// The rows reaching into the band from `y0` to `y1`.
    pub fn within(&self, y0: f32, y1: f32) -> std::ops::Range<usize> {
        let first = self.edges.partition_point(|&e| e <= y0).saturating_sub(1);
        let end = self.edges.partition_point(|&e| e < y1).min(self.count());
        first.min(end)..end
    }

    /// Where row `i` lies on the page.
    pub fn rect(&self, i: usize, w: f32) -> Rect {
        Rect::new(0.0, self.edges[i], w, self.edges[i + 1] - self.edges[i])
    }
}

/// A strip drawn once and laid over the page at `top`, or just under the
/// bar once the page has scrolled it that far; its controls answer where
/// it shows.
pub struct Pinned {
    pub canvas: Canvas,
    pub top: f32,
    /// In the strip's own coordinates.
    pub hits: Vec<Hit>,
}

/// What the About page lays over its field: the wordmark and version, which
/// stay put while the cards scroll, and the cards' coverage row by row.
pub struct HeroArt {
    pub wordmark: Sprite,
    pub version: Sprite,
    pub runs: Vec<Vec<Run>>,
}

impl Frame {
    /// A page of cards on the grey page colour that opens with its title in
    /// large type and goes back with `<`.
    pub fn new(t: &Theme, canvas: Canvas, hits: Vec<Hit>, title: &'static str, content_top: f32) -> Frame {
        Frame {
            canvas,
            hits,
            title,
            back: true,
            header: true,
            content_top,
            hero: None,
            pattern: None,
            bg: t.pal.bg,
            rows: None,
            pinned: None,
            scrollback: None,
            dock: None,
            caret: None,
        }
    }

    /// The topmost control under a point in page coordinates.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if let Some(h) = self.hits.iter().rev().find(|h| h.rect.contains(x, y)) {
            return Some(*h);
        }
        let rows = self.rows.as_ref()?;
        let action = rows.action?;
        let i = rows.at(y)?;
        Some(Hit { rect: rows.rect(i, self.canvas.w as f32), action: action(i) })
    }

    /// How tall the page is: its canvas, or its rows and the margin after
    /// them if they reach further.
    pub fn height(&self, t: &Theme) -> f32 {
        let h = self.canvas.h as f32;
        let h = self.rows.as_ref().map_or(h, |r| h.max((r.bottom() + t.m.margin * 2.0).ceil()));
        h + self.dock.as_ref().map_or(0.0, |d| d.canvas.h as f32)
    }
}

/// Choices the pages show and the user changes, and where installing
/// stands.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub dark: bool,
    pub brightness: f32,
    /// How long the screen stays on with nothing touched, by its place in
    /// [`SCREEN_OFF`].
    pub screen_off: usize,
    /// The time zone the clock and the times on the pages keep, by its
    /// place in [`TIME_ZONES`].
    pub time_zone: usize,
    pub vibrate: bool,
    /// A computer connected over USB is offered internal storage.
    pub file_transfer: bool,
    pub reboot: usize,
    /// What the wipe page has chosen, by place in [`WIPES`].
    pub wipe: [bool; WIPES.len()],
    /// The phone has a cache partition, which the wipe page then offers.
    pub cache_partition: bool,
    /// How many folders of its own the phone keeps logs in on the data
    /// partition, which erasing the logs takes in.
    pub phone_logs: usize,
    pub pattern: Vec<usize>,
    /// How far the swipe-to-confirm knob is dragged, 0 to 1.
    pub confirm: f32,
    pub install: Install,
    /// Where the package being installed comes from.
    pub source: Source,
    /// The name of the file being installed, when it is one on the phone.
    pub package_name: Option<String>,
    /// What the install script last run showed, its latest lines.
    pub script_output: Vec<String>,
    /// The phone keeps its recovery in a partition of each slot, which the
    /// install page then offers to keep Skyward in.
    pub recovery_partitions: bool,
    /// Skyward goes back into the recovery partitions an install writes
    /// over.
    pub keep_recovery: bool,
    /// How Skyward went back into the recovery partitions the install
    /// wrote over, where it did.
    pub reflash: Option<Reflash>,
    /// The mount page's partitions, as the runtime last read them, or why
    /// it could not.
    pub mounts: Result<Vec<MountRow>, String>,
    pub mount_failure: Option<MountFailure>,
    /// Which of [`BACKUP_PARTS`] a backup takes.
    pub backup: [bool; 4],
    /// Where a backup goes.
    pub backup_to: Place,
    pub compress: bool,
    pub verify: bool,
    /// The backups found, newest first, those that cannot be read last.
    pub backups: Vec<BackupEntry>,
    /// The backup the restore page shows, as it was found, and which of its
    /// parts to restore.
    pub restore: Option<(BackupEntry, [bool; 4])>,
    /// Backing up or restoring under way, or how it last ended.
    pub backup_job: Option<BackupJob>,
    /// Which logs to export: the recovery's, the kernel's.
    pub logs: [bool; 2],
    /// Where the logs go: 0 for /tmp, 1 for internal storage.
    pub logs_to: usize,
    /// What the phone's boot control holds of the slots, as the runtime
    /// last read it for the slot or reboot page, or why it could not.
    pub boot_slots: Option<Result<BootSlots, String>>,
    /// The slot picked on the slot page to start next; with none picked,
    /// the page shows the one the boot control starts next.
    pub next_slot: Option<Slot>,
    /// Where the data partition stands, and what is typed at the screen
    /// lock its users' own files wait on.
    pub data: Data,
    pub code: String,
    /// What is typed in the search field.
    pub search: String,
    /// The on-screen keyboard's layer, and the key held down on it.
    pub layer: Layer,
    pub pressed: Option<Cap>,
    /// The folder the file manager shows.
    pub files: Folder,
    /// The folder of internal storage a file to install is chosen in, its
    /// path from the top of the storage.
    pub storage: Folder,
    /// The USB drive plugged in, as the runtime last found it, and the
    /// folder of it a file to install is chosen in, its path from the top
    /// of the drive.
    pub usb_drive: Option<UsbDrive>,
    pub drive: Folder,
    /// The entry the file page describes.
    pub file: Option<FileRow>,
    pub fastboot: Fastboot,
    pub erase: Erase,
    pub asked: Asked,
    /// The entry picked to copy or move, while a folder for it is looked
    /// for.
    pub clip: Option<Clip>,
    /// What is typed as an entry's new name.
    pub new_name: String,
    /// Deleting the entry the file page shows, or the backup the restore
    /// page shows, was asked for; a swipe confirms it.
    pub asking_delete: bool,
    /// A copy or move waiting on what to do with the names taken where it
    /// goes, and those names.
    pub clash: Option<(Clip, String, Vec<String>)>,
    /// Work on files under way, or how it last ended.
    pub job: Option<FileJob>,
    /// Where an update the system installed stands, as the runtime last
    /// read it.
    pub update: Standing,
}

impl Default for State {
    fn default() -> Self {
        State {
            dark: false,
            brightness: 0.5,
            // Two minutes.
            screen_off: 3,
            time_zone: 0,
            vibrate: true,
            file_transfer: true,
            reboot: 0,
            wipe: [false; WIPES.len()],
            cache_partition: false,
            phone_logs: 0,
            pattern: Vec::new(),
            confirm: 0.0,
            install: Install::Idle,
            source: Source::Computer,
            package_name: None,
            script_output: Vec::new(),
            recovery_partitions: false,
            keep_recovery: true,
            reflash: None,
            mounts: Ok(Vec::new()),
            mount_failure: None,
            backup: [true, true, false, true],
            backup_to: Place::Storage,
            compress: false,
            verify: true,
            backups: Vec::new(),
            restore: None,
            backup_job: None,
            logs: [true, true],
            logs_to: 0,
            boot_slots: None,
            next_slot: None,
            data: Data::Closed,
            code: String::new(),
            search: String::new(),
            layer: Layer::Letters,
            pressed: None,
            files: Folder::new("/"),
            storage: Folder::new(""),
            usb_drive: None,
            drive: Folder::new(""),
            file: None,
            fastboot: Fastboot { running: false, link: UsbLink::Unplugged },
            erase: Erase::default(),
            asked: Asked::default(),
            clip: None,
            new_name: String::new(),
            asking_delete: false,
            clash: None,
            job: None,
            update: Standing::None,
        }
    }
}

impl State {
    /// The folder the list on page `screen` shows: the file manager's, or
    /// a place's to choose a file to install in.
    pub fn folder(&self, screen: Screen) -> &Folder {
        match screen {
            Screen::Files => &self.files,
            Screen::Choose(Place::Storage) => &self.storage,
            Screen::Choose(Place::UsbDrive) => &self.drive,
            other => unreachable!("{other:?} lists no folder"),
        }
    }

    fn folder_mut(&mut self, screen: Screen) -> &mut Folder {
        match screen {
            Screen::Files => &mut self.files,
            Screen::Choose(Place::Storage) => &mut self.storage,
            Screen::Choose(Place::UsbDrive) => &mut self.drive,
            other => unreachable!("{other:?} lists no folder"),
        }
    }

    /// Whether the files of `place` can be read: internal storage once it
    /// is open, a USB drive once the runtime shows it.
    pub fn place_open(&self, place: Place) -> bool {
        match place {
            Place::Storage => self.data.storage_open(),
            Place::UsbDrive => self.usb_drive == Some(UsbDrive::Shown),
        }
    }
}

/// One finger, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Touch {
    Down(f32, f32),
    Move(f32, f32),
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Power,
    VolumeUp,
    VolumeDown,
    Back,
}

/// Work the runtime carries out on the UI's behalf.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Reboot(Target),
    /// A short tap of the vibration motor.
    Haptic,
    /// Offer internal storage to a computer over USB, or stop.
    FileTransfer(bool),
    /// Start receiving a package over USB.
    Sideload,
    /// Start installing the file at this path from the top of a place,
    /// telling how it goes as [`Effect::Sideload`] does.
    InstallFile(Place, String),
    /// Write the image the installer holds to `partition` in the slots
    /// `pick` means.
    Flash { partition: &'static str, pick: Pick },
    /// Stop waiting for the computer, for where an image goes, or for an
    /// update to be agreed to.
    Cancel,
    /// Read the slot's partitions for the mount page, which has just opened.
    ReadMounts,
    /// Read what the boot control holds of the slots, telling it with
    /// [`App::set_boot_slots`].
    ReadSlots,
    /// Have the bootloader start this slot next, telling how it went with
    /// [`App::slot_switched`].
    SetSlot(Slot),
    /// Mount the slot's partition with this base name, such as `system`.
    Mount(String),
    /// Unmount the slot's partition with this base name.
    Unmount(String),
    /// List the folder at this path for the file manager.
    ReadDir(String),
    /// List the folder at this path from the top of a place, to choose a
    /// file to install in, telling what it holds with [`App::set_place`].
    ReadPlace(Place, String),
    /// Open the data partition as far as it opens with nothing a user
    /// knows, telling what waits with [`App::data_waiting`] or why it did
    /// not open with [`App::data_failed`].
    OpenData,
    /// Open the own files of `user` with what they entered at their screen
    /// lock, telling what still waits with [`App::data_waiting`] or why it
    /// did not open with [`App::unlock_failed`].
    Unlock { user: u32, code: String },
    /// Hand the USB port back from fastboot to adb, bring up what the
    /// recovery runs from the slot's partitions, then show its pages with
    /// [`App::enter_recovery`].
    EnterRecovery,
    /// Erase all user data, as a factory reset does, telling how it goes
    /// with [`App::erase_at`] and [`App::erase_ended`].
    FactoryReset,
    /// Erase what the wipe page has chosen, [`Wipe::All`] as a factory
    /// reset does, telling how it goes with [`App::erase_at`],
    /// [`App::erase_progress`] and [`App::erase_ended`].
    Wipe(Vec<Wipe>),
    /// Install the update held for the user to agree to.
    Install,
    /// Forget what the system asked of the recovery, so that the next start
    /// goes where its reboot reason says.
    Dismiss,
    /// Start a shell for the terminal page, on a screen of the size
    /// [`App::term_size`] gives, telling of it with [`App::term_started`].
    StartShell,
    /// Send these bytes to the shell, as typed.
    TermInput(Vec<u8>),
    /// Copy or move an entry into a folder; with no choice made yet, a name
    /// taken there is told with [`App::file_clash`] instead of going on.
    Paste { op: FileOp, path: String, into: String, clash: Option<ClashChoice> },
    /// Give an entry a new name; how it went comes back with
    /// [`App::renamed`].
    Rename { path: String, name: String },
    /// Delete an entry, folders whole.
    Delete(String),
    /// Stop the work on files under way.
    StopJob,
    /// Save the logs chosen, the recovery's own and the kernel's, into a
    /// folder of their own in internal storage or the temporary folder,
    /// saying where with [`App::report`].
    ExportLogs { recovery: bool, kernel: bool, storage: bool },
    /// Back up the parts ticked, by their place in [`BACKUP_PARTS`], into
    /// a folder of their own in `place`, telling how it goes with
    /// [`App::backup_started`], [`App::backup_plan`], [`App::backup_at`]
    /// and [`App::backup_ended`].
    Backup { parts: [bool; 4], place: Place, compress: bool, verify: bool },
    /// Restore the parts ticked of the backup in the folder `name` of
    /// `place`, telling how it goes as a backup does.
    Restore { place: Place, name: String, parts: [bool; 4] },
    /// Look for backups in the places that can be read, telling what is
    /// found with [`App::set_backups`].
    FindBackups { storage: bool, usb: bool },
    /// Delete the backup in the folder `name` of `place`, telling how it
    /// went with [`App::backup_deleted`].
    DeleteBackup { place: Place, name: String },
    /// Stop the backup under way, or the check before a restore.
    StopBackup,
    /// Read where the update stands, telling it with [`App::set_update`].
    ReadUpdate,
    /// Merge the update that waits in the slot the phone runs from,
    /// telling how it goes with [`App::set_update`].
    Merge,
}

enum Drag {
    Idle,
    /// Finger down and not yet past the touch slop. `edge` is set when the
    /// press began at a screen edge (true for the left one).
    /// `at` is when it came down: held long enough over an entry of the file
    /// manager, it opens what can be done with that entry.
    Press { x: f32, y: f32, action: Option<Action>, edge: Option<bool>, at: u64 },
    /// A press held long enough to have done its work; nothing it does
    /// counts until it lifts.
    Held,
    Scroll { last: f32 },
    /// Swiping in from a screen edge, the finger now `x` across; the cue
    /// keeps to height `cy`, set by where the swipe began. Past the
    /// threshold, lifting goes back.
    Edge { start: f32, from_left: bool, armed: bool, x: f32, cy: f32 },
    Slider(Rect),
    Confirm { which: Confirm, track: Rect, grab: f32 },
    /// Scrolling a terminal's screen back through what the shell wrote,
    /// `carry` being the part of a row not yet scrolled.
    History { last: f32, carry: f32, pitch: f32 },
    Pattern,
    /// The finger that lit the dark panel; nothing it does counts until it
    /// lifts.
    Waking,
}

/// Where the keys are on a page.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    /// Nowhere yet: the first control in view lights once the keys work
    /// the page.
    Unset,
    /// On no control, none showing where the keys scrolled the page.
    Off,
    /// On this control, as it was when the page was last drawn.
    On(Spot),
}

/// A button held down, and what holding it does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Press {
    /// A volume key, moving the light down the page or up it, and again at
    /// `next`.
    Volume { down: bool, next: u64 },
    /// The power key on the control lit, from `at`, which it works as it
    /// comes up; held long enough over an entry of the file manager, it
    /// opens what can be done with the entry, as a long press does.
    Power { spot: Spot, at: u64 },
    /// The power key on a swipe-to-confirm control, whose knob slides along
    /// its track at `track` from `at` while the key is held; let go once
    /// it has slid far enough, the control confirms.
    Slide { which: Confirm, track: Rect, at: u64 },
    /// Done with: nothing more comes of it until it is let go.
    Spent,
}

const TOAST_MS: u64 = 2000;
/// How long a text field's cursor shows and then hides, as the stock
/// fields blink it.
const BLINK_MS: u64 = 500;
/// The spring the home page's search bar moves to the search page's field
/// on, and back: critically damped, fitted to frames of the stock settings
/// on a Xiaomi 17 Ultra.
const MORPH_PERIOD: f32 = 0.292;
/// Along the move, from 0 at the home page to 1 at the search page: where
/// the large title has faded away, and the stretch over which the button
/// leaving the search comes in.
const MORPH_TITLE_GONE: f32 = 0.6;
const MORPH_CANCEL: (f32, f32) = (0.45, 0.95);
/// How blurred that button is as it starts to show, in dp.
const MORPH_CANCEL_BLUR_DP: f32 = 8.0;
/// When what the search page shows under its field comes in after the move
/// starts, and how long it takes, in ms.
const MORPH_BODY_MS: (u64, u64) = (380, 120);
/// The most lines of what an install script shows that its page keeps,
/// the latest; the log keeps them all.
const SCRIPT_LINES: usize = 500;
/// How long a finger rests on an entry before it counts as held, as the
/// system's long press does.
const LONG_PRESS_MS: u64 = 400;
/// A finger lifted slower than this, in dp per second, leaves the page
/// where it is.
const MIN_FLING_DP: f32 = 50.0;
/// The fastest a page starts gliding, in dp per second.
const MAX_FLING_DP: f32 = 8000.0;
/// A glide running into a page's end faster than this, in dp per second,
/// swings out on the slower spring.
const HARD_BOUNCE_DP: f32 = 1667.0;
/// Scroll, in dp, over which a large title fades out.
const TITLE_FADE_DP: f32 = 20.0;
/// Blur, in dp, of a title faded out completely.
const TITLE_BLUR_DP: f32 = 25.0 / 3.0;
/// How far, in dp, a swipe from an edge has to come in to go back.
const BACK_SWIPE_DP: f32 = 56.0;
/// The back cue sits this far, in dp, above where the finger came down,
/// and stays at that height however the finger then wanders.
const CUE_LIFT_DP: f32 = 9.0;
/// On the About page, where the wordmark's area ends, in dp from the top:
/// the version has faded out by the time it passes under the bar.
const ABOUT_LOGO_DP: f32 = 160.0;
/// Scroll, in dp, over which the wordmark then shrinks and fades.
const ABOUT_LOGO_FADE_DP: f32 = 40.0;
/// Scroll, in dp, over which the About page's field fades out; the field's
/// discs sit in the band this far below the bar.
const ABOUT_FIELD_DP: f32 = 389.0;
/// The field is worked out every this many pixels and filled in between.
const FIELD_CELL: usize = 24;
/// Before the panel goes off it stays dim this long, in milliseconds, or
/// a fifth of the screen-off time when that is shorter.
const DIM_MS: u64 = 7000;
/// The dim level is this much below the brightness set, no brighter than
/// the ceiling, and never under the slider's floor unless the brightness
/// set is.
const DIM_DROP: f32 = 0.04;
const DIM_CEILING: f32 = 0.05;
/// The lowest brightness the slider sets.
const MIN_BRIGHTNESS: f32 = 0.02;
/// How much of the slider's span a volume key turns the brightness up or
/// down.
const BRIGHTNESS_STEP: f32 = 0.1;
/// While the backlight eases, it steps once a frame of a 120 Hz panel.
const BACKLIGHT_STEP_MS: u64 = 8;
/// Just after the panel lights, a touch or key only keeps it awake: the
/// second tap of a wake habit, or the hand reaching for a phone whose
/// screen an install just lit, should not act. Android's double-tap window.
const WAKE_GUARD_MS: u64 = 400;
/// A volume key held this long moves the light again, then once every
/// [`REPEAT_MS`], as the system repeats a key held.
const REPEAT_DELAY_MS: u64 = 400;
const REPEAT_MS: u64 = 50;
/// How far along its track the knob of a swipe-to-confirm control has to
/// be let go to confirm.
const SLID: f32 = 0.9;
/// How long the power key held takes the knob the whole way along.
const SLIDE_MS: u64 = 700;
/// The response, in seconds, of the spring the page scrolls on to show
/// the control the keys moved to.
const SEEK_PERIOD: f32 = 0.35;
/// How strongly the light tints the control the keys are on, and the one
/// the power key is held down on.
const LIGHT_ALPHA: u8 = 36;
const LIGHT_HELD_ALPHA: u8 = 72;
/// How far past the screen, in screens, a long list keeps its rows drawn.
const NEAR_SCREENS: f32 = 0.5;

/// How a part of the screen eases between shown and hidden.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ease {
    /// A critically damped spring of this period, in seconds.
    Spring(f32),
    /// A straight run over this many milliseconds.
    Linear(u64),
}

impl Ease {
    fn progress(self, ms: u64) -> f32 {
        match self {
            Ease::Spring(period) => motion::settle(ms, period),
            Ease::Linear(span) => (ms as f32 / span as f32).min(1.0),
        }
    }
}

/// A part of the screen fading in or out: where it is headed, since when,
/// how far in it was then, and how it eases each way.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fade {
    on: bool,
    since: u64,
    from: f32,
    show: Ease,
    hide: Ease,
}

impl Fade {
    fn hidden(show: Ease, hide: Ease) -> Fade {
        Fade { on: false, since: 0, from: 0.0, show, hide }
    }

    fn shown(show: Ease, hide: Ease) -> Fade {
        Fade { on: true, since: 0, from: 1.0, show, hide }
    }

    /// How far in, 0 to 1, at `now`.
    fn level(&self, now: u64) -> f32 {
        let (target, ease) = if self.on { (1.0, self.show) } else { (0.0, self.hide) };
        self.from + (target - self.from) * ease.progress(now.saturating_sub(self.since))
    }

    fn set(&mut self, on: bool, now: u64) {
        if on != self.on {
            self.from = self.level(now);
            self.since = now;
            self.on = on;
        }
    }

    /// Stand fully in or out at once.
    fn jump(&mut self, on: bool) {
        self.on = on;
        self.from = if on { 1.0 } else { 0.0 };
    }

    fn moving(&self, now: u64) -> bool {
        self.level(now) != if self.on { 1.0 } else { 0.0 }
    }
}

/// How the bar's title comes and goes: sharpening in on a spring and
/// blurring away on a quicker one, or on the About page, fading in a
/// straight line together with the bar.
fn title_fade(hero: bool) -> Fade {
    if hero {
        Fade::hidden(Ease::Linear(150), Ease::Linear(150))
    } else {
        Fade::hidden(Ease::Spring(0.3), Ease::Spring(0.15))
    }
}

fn bar_fade(hero: bool) -> Fade {
    if hero {
        Fade::hidden(Ease::Linear(150), Ease::Linear(150))
    } else {
        Fade::hidden(Ease::Linear(100), Ease::Linear(100))
    }
}

/// The panel at full brightness: it dims over half a second and comes
/// back quickly when touched.
fn full_brightness() -> Fade {
    Fade::shown(Ease::Linear(150), Ease::Linear(500))
}

/// The level the panel dims to shortly before it goes off, for the
/// brightness set.
fn dim_level(full: f32) -> f32 {
    (full - DIM_DROP).clamp(MIN_BRIGHTNESS, DIM_CEILING).min(full)
}

/// How long the panel stays dim before going off, for a screen-off time.
fn dim_span(after: u64) -> u64 {
    DIM_MS.min(after / 5)
}

/// Where a pinned strip shows on a page scrolled to `offset`: with the page,
/// or just under the bar once the page has carried it that far.
fn pinned_top(t: &Theme, p: &Pinned, offset: f32) -> f32 {
    (p.top - offset).max(widgets::bar_bottom(t))
}

/// Whether `r` reaches into the band of the screen from `top` to `bottom`.
fn within(r: Rect, (top, bottom): (f32, f32)) -> bool {
    r.y < bottom && r.y + r.h > top
}

/// Lay the sprite `blurs` keeps, moved by (`dx`, `dy`), in `color` at
/// `level` of its strength, blurred by `blur` pixels.
fn draw_faded(out: &mut Canvas, blurs: &mut Blurs, dx: f32, dy: f32, color: Rgba, level: f32, blur: f32) {
    if level > 0.0 {
        blurs.get(blur).draw(out, dx, dy, color, level);
    }
}

/// The home page's search bar moving up to become the search page's
/// field, or back down, as the stock settings move it: the home page's
/// content goes at once on the way up and comes along under the bar on
/// the way down; the large title fades and blurs as it goes up with the
/// bar; the button leaving the search fades and sharpens in; the keyboard
/// rises with the bar; and what the search page shows under its field
/// comes in last.
struct Morph {
    /// Up to the search page.
    into: bool,
    /// How far along it is, from 0 at the home page to 1 at the search
    /// page, as a spring in thousandths settling on the end it goes to.
    spring: Spring,
    start: u64,
    /// How far the home page was scrolled, where its bar stood.
    scroll: f32,
    /// The search page's keyboard, and where it rests.
    keyboard: Option<(Canvas, i32)>,
    /// What the search page shows between its field and the keyboard, and
    /// where.
    body: Option<(Canvas, i32)>,
    /// The home page's large title, and the button's disc and cross.
    title: Option<Blurs>,
    cancel: Option<(Blurs, Blurs)>,
}

impl Morph {
    /// How far along it is at `now`, and whether it has settled.
    fn at(&self, now: u64) -> (f32, bool) {
        let (x, done) = self.spring.at(now);
        (if self.into { 1.0 } else { 0.0 } + x / 1000.0, done)
    }
}

/// The keyboard at the foot of a typing page's `frame`, and where it rests.
fn keyboard_strip(t: &Theme, frame: &Frame) -> (Canvas, i32) {
    let top = (t.m.h as f32 - widgets::keyboard_height(t)).floor() as i32;
    let mut strip = Canvas::new(frame.canvas.w, (frame.canvas.h as i32 - top).max(0) as u32);
    strip.put_rows(&frame.canvas, top, frame.canvas.h as i32, 0);
    (strip, top)
}

pub struct App {
    pub state: State,
    /// The pages open, the one shown last: each with how far it is
    /// scrolled and where the keys are on it.
    stack: Vec<(Screen, f32, Focus)>,
    frame: Option<Frame>,
    /// The controls of the page drawn, in the order the keys go through
    /// them.
    spots: Option<Vec<Spot>>,
    /// The keys are working the pages: the control they are on is lit. A
    /// touch puts the light out, and a volume key brings it back.
    keys: bool,
    /// The button held down, and what holding it does.
    button: Option<(Key, Press)>,
    /// The page scrolling of its own accord to show the control the keys
    /// moved to: where it comes to rest, and the spring taking it there.
    seek: Option<(f32, Spring)>,
    /// A swipe-to-confirm control whose knob the power key moved, to draw
    /// again with the page.
    knob: Option<(Confirm, Rect)>,
    /// The brightness slider lit takes the volume keys, until the power
    /// key lets it go.
    adjusting: bool,
    /// How tall the page was when last drawn: one being drawn again, as
    /// work under way tells how far it has come, scrolls as far meanwhile.
    height: f32,
    /// The rows of the page's long list drawn so far, by their place in it;
    /// those far off screen are let go, their canvases kept spare for the
    /// next rows to be drawn on.
    drawn: Vec<(usize, Canvas)>,
    spare: Vec<Canvas>,
    /// The current page's large and bar titles, for their words and the
    /// theme, with their blurred versions as they are needed. A page can
    /// change its title in place, as work on files does once it ends.
    titles: Option<((&'static str, bool), Blurs, Blurs)>,
    /// The `<` chip for the theme (dark or not), and where it goes.
    chip: Option<(bool, Canvas, i32, i32)>,
    /// The About page field's dither, made for the screen once.
    dither: Option<glow::Dither>,
    toast: Option<(String, u64)>,
    drag: Drag,
    tracker: Tracker,
    /// The page gliding on after a flick.
    fling: Option<Fling>,
    /// How far a dragging finger has pulled the page past an end:
    /// negative past the top, positive past the bottom.
    pull: f32,
    /// The page springing back after being pulled or thrown past an end.
    bounce: Option<Spring>,
    /// The back cue shrinking away after a swipe that did not go back:
    /// its edge, where it sits, and its depth.
    cue: Option<(bool, f32, Spring)>,
    title: Fade,
    bar: Fade,
    /// A page was put up, or the file manager's folder changed: once it is
    /// rendered, the bar and its title stand as its scroll has them,
    /// without easing in or out.
    snap: bool,
    /// The About page's moving field, while that page shows.
    glow: Option<GlowClock>,
    /// The terminal page's shell, from when it is first opened; it goes on
    /// while other pages show.
    term: Option<Term>,
    display_on: bool,
    /// When the screen was last touched or a key pressed; set on the first
    /// tick, so the time before the UI started does not count.
    active: Option<u64>,
    /// When the panel last lit from dark, for the brief guard that keeps a
    /// stray touch or key from acting on a page just put up.
    woke_at: Option<u64>,
    /// The panel at full brightness, or easing to the dim level it takes
    /// shortly before going off.
    bright: Fade,
    /// While a text field's cursor is up: the page, how many edits had
    /// been typed, and when it came up whole on them. It shows for the
    /// first half of each blink from then on, so it is whole again at each
    /// edit and on each page it comes up on.
    blink: Option<(Screen, u64, u64)>,
    /// Edits typed into any text field so far.
    edits: u64,
    /// Whether the frame last composed showed the cursor.
    caret_shown: bool,
    /// The home page's search bar on its way to the search page's field,
    /// or back.
    morph: Option<Morph>,
    dirty: bool,
    w: u32,
    h: u32,
    px_per_dp: f32,
}

impl App {
    pub fn new(start: Screen, w: u32, h: u32, px_per_dp: f32) -> Self {
        App {
            state: State::default(),
            stack: vec![(start, 0.0, Focus::Unset)],
            frame: None,
            spots: None,
            // The touchscreen may run from the partitions fastboot leaves
            // out, so its page starts worked with the keys.
            keys: start == Screen::Fastboot,
            button: None,
            seek: None,
            knob: None,
            adjusting: false,
            height: 0.0,
            drawn: Vec::new(),
            spare: Vec::new(),
            titles: None,
            chip: None,
            dither: None,
            toast: None,
            drag: Drag::Idle,
            tracker: Tracker::default(),
            fling: None,
            pull: 0.0,
            bounce: None,
            cue: None,
            title: title_fade(start == Screen::About),
            bar: bar_fade(start == Screen::About),
            snap: false,
            glow: None,
            term: None,
            display_on: true,
            active: None,
            woke_at: None,
            bright: full_brightness(),
            blink: None,
            edits: 0,
            caret_shown: false,
            morph: None,
            dirty: true,
            w,
            h,
            px_per_dp,
        }
    }

    pub fn theme(&self) -> Theme {
        if self.state.dark {
            Theme::dark(self.w, self.h, self.px_per_dp)
        } else {
            Theme::light(self.w, self.h, self.px_per_dp)
        }
    }

    pub fn screen(&self) -> Screen {
        self.top().0
    }

    fn top(&self) -> &(Screen, f32, Focus) {
        self.stack.last().expect("the page stack always holds a page")
    }

    fn top_mut(&mut self) -> &mut (Screen, f32, Focus) {
        self.stack.last_mut().expect("the page stack always holds a page")
    }

    /// Whether the panel needs a new frame since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// When time next has to pass through [`App::tick`]: at once while
    /// something on the lit screen moves on its own, a frame on while the
    /// backlight eases, else when a shown message should disappear or the
    /// panel should dim or go off.
    pub fn deadline(&self, now: u64) -> Option<u64> {
        let sliding = matches!(self.button, Some((_, Press::Slide { .. }))) && self.state.confirm < 1.0;
        let moving = self.display_on
            && (self.fling.is_some()
                || self.bounce.is_some()
                || self.seek.is_some()
                || sliding
                || self.cue.is_some()
                || self.title.moving(now)
                || self.bar.moving(now)
                || self.morph.is_some()
                || self.glowing());
        if moving {
            return Some(0);
        }
        let easing = self.bright.moving(now).then_some(now + BACKLIGHT_STEP_MS);
        let held = match self.drag {
            Drag::Press { action: Some(Action::OpenEntry(_)), at, .. } => Some(at + LONG_PRESS_MS),
            _ => None,
        };
        let repeat = match self.button {
            Some((_, Press::Volume { next, .. })) => Some(next),
            Some((_, Press::Power { spot: Spot { stop: Stop::Hit(Action::OpenEntry(_)), .. }, at })) => Some(at + LONG_PRESS_MS),
            _ => None,
        };
        let blink = self.blink_at(now).filter(|_| self.display_on).map(|(_, next)| next);
        [self.toast.as_ref().map(|t| t.1), easing, self.idle_due(), held, repeat, blink].into_iter().flatten().min()
    }

    /// Whether a text field's cursor shows at `now`, and when it next turns;
    /// `None` while no cursor is up.
    fn blink_at(&self, now: u64) -> Option<(bool, u64)> {
        let (_, _, since) = self.blink?;
        let n = now.saturating_sub(since) / BLINK_MS;
        Some((n.is_multiple_of(2), since + (n + 1) * BLINK_MS))
    }

    /// When sitting idle next changes the lit panel: when it dims, or once
    /// it is dimming, when it goes off.
    fn idle_due(&self) -> Option<u64> {
        let (active, after) = (self.active?, SCREEN_OFF[self.state.screen_off]?);
        if !self.display_on {
            return None;
        }
        Some(if self.bright.on { active + after - dim_span(after) } else { active + after })
    }

    /// Open what can be done with entry `i` of the folder shown, held long
    /// enough to, with a tap of the motor.
    fn held_entry(&mut self, i: usize) -> Vec<Effect> {
        let entry = self.state.files.entry(i).clone();
        self.state.file = Some(entry);
        self.state.asking_delete = false;
        self.open(Screen::FileInfo);
        if self.state.vibrate {
            vec![Effect::Haptic]
        } else {
            Vec::new()
        }
    }

    /// Let time pass: the panel dims and goes off once left alone long
    /// enough, a gliding page moves on, a page past its end springs back,
    /// the bar and its title follow the scroll, and a message past its
    /// deadline goes away.
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        // An entry of the file manager held long enough, under a finger or
        // the power key, opens what can be done with it, a folder as much as
        // a file, with a tap of the motor.
        if let Drag::Press { action: Some(Action::OpenEntry(i)), at, .. } = self.drag {
            if now >= at + LONG_PRESS_MS {
                self.drag = Drag::Held;
                fx.extend(self.held_entry(i));
            }
        }
        if let Some((key, Press::Power { spot: Spot { stop: Stop::Hit(Action::OpenEntry(i)), .. }, at })) = self.button {
            if now >= at + LONG_PRESS_MS {
                self.button = Some((key, Press::Spent));
                fx.extend(self.held_entry(i));
            }
        }
        let active = *self.active.get_or_insert(now);
        if let (true, Some(after)) = (self.display_on, SCREEN_OFF[self.state.screen_off]) {
            let idle = now.saturating_sub(active);
            if idle >= after {
                self.sleep();
            } else if idle >= after - dim_span(after) {
                self.bright.set(false, now);
            }
        }
        if self.toast.as_ref().is_some_and(|m| now >= m.1) {
            self.toast = None;
            self.dirty = true;
        }
        if self.blink_at(now).is_some_and(|(on, _)| on != self.caret_shown) || self.morph.is_some() {
            self.dirty = true;
        }
        if let Some(fling) = self.fling {
            let (y, done) = fling.at(now);
            let next = y.clamp(0.0, self.max_scroll());
            let top = self.top_mut();
            if next != top.1 {
                top.1 = next;
                self.dirty = true;
            }
            if next != y {
                // The glide ran into an end: the speed it had left throws
                // the page past it, and it springs back.
                let speed = fling.velocity(now);
                let period = if speed.abs() <= self.theme().m.dp(HARD_BOUNCE_DP) { 0.4 } else { 0.55 };
                self.bounce = Some(Spring::new(now, 0.0, speed, period));
                self.fling = None;
            } else if done {
                self.fling = None;
            }
        }
        if let Some(b) = self.bounce {
            self.dirty = true;
            if b.at(now).1 {
                self.bounce = None;
            }
        }
        if let Some((target, spring)) = self.seek {
            let (off, done) = spring.at(now);
            let at = if done { target } else { (target + off).clamp(0.0, self.max_scroll()) };
            let top = self.top_mut();
            if top.1 != at {
                top.1 = at;
                self.dirty = true;
            }
            if done {
                self.seek = None;
            }
        }
        // The power key held on a swipe-to-confirm control slides its knob.
        if let Some((_, Press::Slide { which, track, at })) = self.button {
            let along = (now.saturating_sub(at) as f32 / SLIDE_MS as f32).min(1.0);
            if along != self.state.confirm {
                self.state.confirm = along;
                self.knob = Some((which, track));
                self.dirty = true;
            }
        }
        // A volume key held moves the light on and on, once the page it
        // last moved on has been drawn.
        if let Some((key, Press::Volume { down, next })) = self.button {
            if now >= next && self.frame.is_some() {
                self.button = Some((key, Press::Volume { down, next: now + REPEAT_MS }));
                self.volume(down, now);
            }
        }
        if let Some((_, _, s)) = self.cue {
            self.dirty = true;
            if s.at(now).1 {
                self.cue = None;
            }
        }
        if self.screen() == Screen::About {
            let look = self.theme().hero.glow;
            match &mut self.glow {
                Some(g) if std::ptr::eq(g.look(), look) => g.advance(now),
                _ => self.glow = Some(GlowClock::new(look, now)),
            }
            if self.glowing() {
                self.dirty = true;
            }
        }
        self.follow_scroll(now);
        fx
    }

    /// Whether the About page's field is on screen, moving.
    fn glowing(&self) -> bool {
        self.glow.is_some() && self.display_on && self.top().1 < self.theme().m.dp(ABOUT_FIELD_DP)
    }

    /// Whether the bar's title and the bar belong over the rendered page
    /// where it is scrolled: the title once the large one has faded, the bar
    /// once the content slides under it. On the About page both come once
    /// the version has faded out.
    fn bar_wanted(&self) -> Option<(bool, bool)> {
        let fr = self.frame.as_ref()?;
        let t = self.theme();
        let scroll = self.top().1;
        Some(if fr.hero.is_some() {
            let gone = scroll >= t.m.dp(ABOUT_LOGO_DP) - widgets::bar_bottom(&t);
            (gone, gone)
        } else {
            let under = (fr.content_top - widgets::bar_bottom(&t) - t.m.dp(20.0)).max(0.0);
            (fr.header && scroll >= t.m.dp(TITLE_FADE_DP), scroll > under)
        })
    }

    /// Bring the bar and its title in or out as the page scrolls.
    fn follow_scroll(&mut self, now: u64) {
        let Some((title, bar)) = self.bar_wanted() else { return };
        self.title.set(title, now);
        self.bar.set(bar, now);
        if self.title.moving(now) || self.bar.moving(now) {
            self.dirty = true;
        }
    }

    /// How far past its end the page shows at `now`: negative past the
    /// top, positive past the bottom.
    fn overscroll(&self, now: u64) -> f32 {
        if self.pull != 0.0 {
            motion::rubber(self.pull, self.h as f32)
        } else {
            self.bounce.map_or(0.0, |b| b.at(now).0)
        }
    }

    pub fn display_on(&self) -> bool {
        self.display_on
    }

    /// The backlight at `now`: off, or this share of the brightness at the
    /// slider's top.
    pub fn panel(&self, now: u64) -> Option<f32> {
        if !self.display_on {
            return None;
        }
        let (full, level) = (self.state.brightness, self.bright.level(now));
        Some(full * level + dim_level(full) * (1.0 - level))
    }

    /// Something was touched or pressed: the panel stays at full
    /// brightness for the screen-off time from now.
    fn stir(&mut self, now: u64) {
        self.active = Some(now);
        self.bright.set(true, now);
    }

    /// Light the panel, as if just touched.
    fn wake(&mut self, now: u64) {
        if !self.display_on {
            self.display_on = true;
            self.dirty = true;
        }
        self.woke_at = Some(now);
        self.stir(now);
    }

    /// Whether the panel lit so recently that a touch or key should only
    /// keep it awake.
    fn just_woke(&self, now: u64) -> bool {
        self.woke_at.is_some_and(|w| now.saturating_sub(w) < WAKE_GUARD_MS)
    }

    /// Turn the panel off. A finger on the glass lets go of what it held,
    /// and the page stops where it is.
    fn sleep(&mut self) {
        self.display_on = false;
        self.bright = full_brightness();
        match std::mem::replace(&mut self.drag, Drag::Idle) {
            Drag::Confirm { .. } => {
                self.state.confirm = 0.0;
                self.refresh();
            }
            Drag::Pattern => {
                self.state.pattern.clear();
                self.refresh();
            }
            _ => {}
        }
        self.drop_button();
        self.adjusting = false;
        self.fling = None;
        self.bounce = None;
        self.seek = None;
        self.pull = 0.0;
        self.cue = None;
        self.morph = None;
    }

    /// Something laid over the page changed (the clock): compose again.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The page's readings changed (battery, storage): render it again.
    pub fn refresh(&mut self) {
        self.frame = None;
        self.spots = None;
        self.knob = None;
        self.spare.extend(self.drawn.drain(..).map(|(_, c)| c));
        self.dirty = true;
    }

    /// The data partition is open: every user's own files with it, or
    /// those of `waiting`, the user and the control their screen lock is
    /// answered with, still closed.
    pub fn data_waiting(&mut self, waiting: Option<(u32, Keys)>) {
        self.state.data = match waiting {
            Some((user, keys)) => Data::Locked { user, keys, checking: false },
            None => Data::Open,
        };
        self.state.code.clear();
        self.refresh();
    }

    /// The data partition did not open, and why.
    pub fn data_failed(&mut self, why: String) {
        self.state.data = Data::Failed(why);
        self.refresh();
    }

    /// What the user entered did not open their own files: the page asks
    /// again and says why.
    pub fn unlock_failed(&mut self, why: String, now: u64) {
        if let Data::Locked { checking, .. } = &mut self.state.data {
            *checking = false;
        }
        self.refresh();
        self.report(&why, now);
    }

    /// The runtime read the mount table again, or could not: keep it and
    /// show it if it changed.
    pub fn set_mounts(&mut self, mounts: Result<Vec<MountRow>, String>) {
        if self.state.mounts != mounts {
            self.state.mounts = mounts;
            self.refresh();
        }
    }

    /// The runtime read whether fastbootd runs and how far a computer has
    /// the port: keep it and show it if it changed.
    pub fn set_fastboot(&mut self, running: bool, link: UsbLink) {
        let now = Fastboot { running, link };
        if self.state.fastboot != now {
            self.state.fastboot = now;
            self.refresh();
        }
    }

    /// Fastboot has stopped and the recovery is up: its pages start again
    /// from the home page, which nothing can go back from to fastboot.
    pub fn enter_recovery(&mut self) {
        self.stack = vec![(Screen::Home, 0.0, Focus::Unset)];
        self.settle_page();
        self.refresh();
    }

    /// How many rows and columns the terminal's screen holds.
    pub fn term_size(&self, f: &Fonts) -> (u16, u16) {
        let t = self.theme();
        let l = term::layout(&t, f, t.m.content_top);
        (l.rows, l.cols)
    }

    /// A shell has started for the terminal page, on a blank screen of
    /// `rows` by `cols`.
    pub fn term_started(&mut self, rows: u16, cols: u16) {
        self.term = Some(Term::new(rows, cols));
        self.refresh();
    }

    /// What the shell wrote.
    pub fn term_output(&mut self, bytes: &[u8]) {
        let term = self.term.as_mut().expect("only a shell that started writes");
        term.feed(bytes);
        if self.screen() == Screen::Terminal {
            self.refresh();
        }
    }

    /// The shell has ended, `how` saying how; its screen says so, and that
    /// Enter starts another.
    pub fn term_ended(&mut self, how: &str) {
        let term = self.term.as_mut().expect("only a shell that started ends");
        term.feed(format!("\r\n[{how}。按「回车」重新启动]\r\n").as_bytes());
        term.running = false;
        if self.screen() == Screen::Terminal {
            self.refresh();
        }
    }

    /// Names are taken in `into` where a copy or move of `path` goes: what
    /// to do with them is the user's to choose.
    pub fn file_clash(&mut self, op: FileOp, path: String, into: String, taken: Vec<String>) {
        self.state.clash = Some((Clip { op, path }, into, taken));
        self.open(Screen::Clash);
    }

    /// Work on files has begun: its page shows how far it comes, in place
    /// of the page of an entry being deleted.
    pub fn file_job_started(&mut self, task: FileTask) {
        if matches!(task, FileTask::Copy { .. } | FileTask::Move { .. }) {
            self.state.clip = None;
        }
        self.state.job = Some(FileJob { task, progress: None, outcome: None, stopping: false });
        if self.screen() == Screen::FileInfo {
            self.state.file = None;
            self.state.asking_delete = false;
            self.stack.pop();
        }
        self.open(Screen::FileJob);
    }

    /// How far work on files has come, in bytes and in entries.
    pub fn file_progress(&mut self, bytes: (u64, u64), entries: (u64, u64)) {
        let job = self.state.job.as_mut().expect("only work under way moves on");
        job.progress = Some((bytes, entries));
        if self.screen() == Screen::FileJob {
            self.refresh();
        }
    }

    /// Work on files has ended. Done, its page goes and says so; stopped
    /// or failed, the page stays and says why, lit for the user to read.
    pub fn file_job_ended(&mut self, end: JobEnd, now: u64) {
        let job = self.state.job.as_mut().expect("only work under way ends");
        job.stopping = false;
        let done = match (&end, &job.task) {
            (JobEnd::Done, FileTask::Copy { .. }) => Some("已复制"),
            (JobEnd::Done, FileTask::Move { .. }) => Some("已移动"),
            (JobEnd::Done, FileTask::Delete { .. }) => Some("已删除"),
            _ => None,
        };
        job.outcome = Some(end);
        match done {
            Some(said) => {
                self.state.job = None;
                if self.screen() == Screen::FileJob {
                    self.stack.pop();
                    self.settle_page();
                }
                self.refresh();
                self.say(said, now);
            }
            None => {
                self.wake(now);
                self.refresh();
            }
        }
    }

    /// How renaming the entry the file page shows went: done, the folder
    /// comes back with the entry under its new name; refused, why.
    pub fn renamed(&mut self, outcome: Result<(), RenameRefused>, now: u64) {
        match outcome {
            Ok(()) => {
                while matches!(self.screen(), Screen::Rename | Screen::FileInfo) {
                    self.stack.pop();
                }
                self.state.file = None;
                self.settle_page();
                self.refresh();
                self.say("已重命名", now);
            }
            Err(RenameRefused::Taken) => self.say_taken(now),
            Err(RenameRefused::Failed(why)) => self.report(&why, now),
        }
    }

    /// The name typed for an entry is another entry's in the folder.
    fn say_taken(&mut self, now: u64) {
        let said = format!("该文件夹中已有\"{}\"", self.state.new_name);
        self.say(&said, now);
    }

    /// The system asked for sideload before the recovery started: the page
    /// opens over the home page, already receiving.
    pub fn start_sideload(&mut self) {
        self.begin_install(Install::Starting, Source::Computer, None);
        self.open(Screen::Installing);
    }

    /// Start an install from `source` afresh, of the file named `name` when
    /// it is one on the phone: nothing the last install showed stays.
    fn begin_install(&mut self, install: Install, source: Source, name: Option<String>) {
        let s = &mut self.state;
        (s.install, s.source, s.package_name) = (install, source, name);
        s.script_output.clear();
        s.reflash = None;
    }

    /// What the system asked of the recovery before it started.
    pub fn set_asked(&mut self, asked: Asked) {
        self.state.asked = asked;
        self.refresh();
    }

    /// Erasing has begun, going through `parts`: its page is the only one
    /// until it ends. `reset` for a factory reset, `asked` when the system
    /// asked for it before the recovery started.
    pub fn erase_started(&mut self, reset: bool, asked: bool, parts: Vec<Part>) {
        self.state.erase = Erase { reset, asked, parts, done: 0, progress: None, outcome: None };
        self.stack = vec![(Screen::Erase, 0.0, Focus::Unset)];
        self.settle_page();
        self.refresh();
    }

    /// Erasing has come to `part`.
    pub fn erase_at(&mut self, part: Part) {
        let erase = &mut self.state.erase;
        erase.done = erase.parts.iter().position(|&p| p == part).expect("erasing comes only to the parts it set out with");
        erase.progress = None;
        self.refresh();
    }

    /// The part under way has come `done` entries of `total`.
    pub fn erase_progress(&mut self, done: u64, total: u64) {
        self.state.erase.progress = Some((done, total));
        self.refresh();
    }

    /// Erasing has ended. What comes next is the user's to choose and the
    /// panel lights for it, unless the system asked for the erasing and it
    /// went well: the phone then leaves on its own. A data partition
    /// erased whole opens no more until the system makes it afresh.
    pub fn erase_ended(&mut self, outcome: Result<(), EraseStop>, now: u64) {
        let erase = &mut self.state.erase;
        if outcome.is_ok() {
            erase.done = erase.parts.len();
        }
        erase.progress = None;
        let data_gone = erase.parts.iter().position(|&p| p == Part::Data).is_some_and(|k| k < erase.done);
        if outcome.is_err() || !erase.asked {
            self.wake(now);
        }
        if data_gone {
            self.state.data = Data::Erased;
        }
        self.state.erase.outcome = Some(outcome);
        self.refresh();
    }

    /// What the boot control holds of the slots, as the runtime read it, or
    /// why it could not.
    pub fn set_boot_slots(&mut self, slots: Result<BootSlots, String>) {
        self.state.boot_slots = Some(slots);
        self.refresh();
    }

    /// How switching the slot to start next went: what the boot control
    /// holds of the slots now, or why it did not switch.
    pub fn slot_switched(&mut self, outcome: Result<BootSlots, String>, now: u64) {
        match outcome {
            Ok(slots) => {
                self.state.next_slot = None;
                self.state.boot_slots = Some(Ok(slots));
                self.say(&format!("下次将从槽位 {} 启动", slots.active.letter()), now);
                self.refresh();
            }
            Err(why) => self.say(&format!("切换失败：{why}"), now),
        }
    }

    /// What was found in the places looked in for backups: the newest
    /// first, by when each was made, and those that cannot be read after
    /// them, by their folder's name, which is when each was begun.
    pub fn set_backups(&mut self, mut backups: Vec<BackupEntry>) {
        let key = |e: &BackupEntry| (e.held.is_ok(), e.held.as_ref().map_or(0, |h| h.made), e.name.clone());
        backups.sort_by_key(|e| std::cmp::Reverse(key(e)));
        self.state.backups = backups;
        self.refresh();
    }

    /// Look for backups in the places that can be read now.
    fn find_backups(&self) -> Effect {
        Effect::FindBackups { storage: self.state.place_open(Place::Storage), usb: self.state.place_open(Place::UsbDrive) }
    }

    /// Backing up, or restoring, has begun: its page shows how far it comes.
    /// `checks` when each part is read back to check it.
    pub fn backup_started(&mut self, restore: bool, place: Place, checks: bool) {
        self.state.backup_job = Some(BackupJob { restore, place, checks, plan: Vec::new(), at: None, outcome: None, stopping: false });
        self.open(Screen::BackupJob);
    }

    /// The parts the work goes through, once they are measured.
    pub fn backup_plan(&mut self, plan: Vec<(String, u64)>) {
        self.state.backup_job.as_mut().expect("only work under way is measured").plan = plan;
        if self.screen() == Screen::BackupJob {
            self.refresh();
        }
    }

    /// How far the work has come.
    pub fn backup_at(&mut self, at: BackupAt) {
        self.state.backup_job.as_mut().expect("only work under way moves on").at = Some(at);
        if self.screen() == Screen::BackupJob {
            self.refresh();
        }
    }

    /// Backing up or restoring has ended: its page says how, lit for the
    /// user to read.
    pub fn backup_ended(&mut self, end: BackupEnd, now: u64) {
        let job = self.state.backup_job.as_mut().expect("only work under way ends");
        job.stopping = false;
        job.outcome = Some(end);
        self.wake(now);
        self.refresh();
    }

    /// How deleting the backup the restore page shows went: deleted, the
    /// page goes, and the backups left are looked for; not, why.
    pub fn backup_deleted(&mut self, outcome: Result<(), String>, now: u64) -> Vec<Effect> {
        match outcome {
            Ok(()) => {
                self.state.restore = None;
                self.state.asking_delete = false;
                if self.screen() == Screen::RestoreBackup {
                    self.stack.pop();
                    self.settle_page();
                }
                self.refresh();
                self.say("已删除", now);
                vec![self.find_backups()]
            }
            Err(why) => {
                self.report(&format!("删除失败：{why}"), now);
                Vec::new()
            }
        }
    }

    /// The runtime could not mount or unmount a partition: the mount page
    /// says so, and why.
    pub fn mount_failed(&mut self, failure: MountFailure) {
        self.state.mount_failure = Some(failure);
        self.refresh();
    }

    /// The runtime listed a folder for the file manager at `listed`, in Unix
    /// seconds, or could not: it becomes the folder shown.
    pub fn set_files(&mut self, folder: String, files: Result<Vec<FileRow>, String>, listed: i64) {
        self.show_folder(Screen::Files, folder, files, listed);
    }

    /// The runtime listed a folder of `place` at `listed`, its path from
    /// the place's top, or could not: it becomes the folder shown to choose
    /// a file to install in, with what it holds that may be installed.
    pub fn set_place(&mut self, place: Place, folder: String, files: Result<Vec<FileRow>, String>, listed: i64) {
        let files = files.map(|entries| entries.into_iter().filter(installable).collect());
        self.show_folder(Screen::Choose(place), folder, files, listed);
    }

    /// The runtime found the USB drive plugged in, shown or not, or found
    /// none. Another drive, or none, starts its folders over from the top,
    /// and one shown while its page is up is read at once. The backups
    /// listed are looked for again, and a backup on a drive gone leaves its
    /// page.
    pub fn set_usb_drive(&mut self, drive: Option<UsbDrive>) -> Vec<Effect> {
        if drive == self.state.usb_drive {
            return Vec::new();
        }
        self.state.usb_drive = drive;
        self.state.drive = Folder::new("");
        self.refresh();
        let gone = !self.state.place_open(Place::UsbDrive) && self.state.restore.as_ref().is_some_and(|(e, _)| e.place == Place::UsbDrive);
        match self.screen() {
            Screen::RestoreBackup if gone => {
                self.state.restore = None;
                self.state.asking_delete = false;
                self.state.confirm = 0.0;
                self.stack.pop();
                self.settle_page();
                return vec![self.find_backups()];
            }
            Screen::Restore => return vec![self.find_backups()],
            _ => {}
        }
        let page = Screen::Choose(Place::UsbDrive);
        if self.screen() != page {
            return Vec::new();
        }
        *self.top_mut() = (page, 0.0, Focus::Unset);
        self.fling = None;
        self.bounce = None;
        self.seek = None;
        self.pull = 0.0;
        self.snap = true;
        self.arrive_at(page)
    }

    /// `folder` becomes the one the list on page `screen` shows. A folder
    /// gone back up to shows where it was left, as the stock file manager
    /// does; any other starts from its top.
    fn show_folder(&mut self, screen: Screen, folder: String, files: Result<Vec<FileRow>, String>, listed: i64) {
        let left = &self.state.folder(screen).path;
        if folder != *left && self.screen() == screen {
            // Climbing out of a folder, the keys come back to it; any other
            // folder starts them at its top.
            let came_from = std::path::Path::new(left).strip_prefix(&folder).ok().and_then(|rest| rest.components().next());
            let name = came_from.map(|c| c.as_os_str().to_string_lossy().into_owned());
            let k = name.and_then(|name| files.as_ref().ok()?.iter().position(|e| e.name == name));
            let row = if screen == Screen::Files { Action::OpenEntry } else { Action::ChooseEntry };
            self.top_mut().2 = match k {
                Some(k) => Focus::On(Spot { stop: Stop::Hit(row(k)), group: Group::Page, rect: Rect::new(0.0, 0.0, 0.0, 0.0) }),
                None => Focus::Unset,
            };
            self.top_mut().1 = self.state.folder_mut(screen).scroll_for(&folder);
            self.fling = None;
            self.bounce = None;
            self.seek = None;
            self.pull = 0.0;
            self.snap = true;
        }
        let shown = self.state.folder_mut(screen);
        shown.path = folder;
        shown.entries = files;
        shown.listed = listed;
        self.refresh();
    }

    /// Render the current page if what it shows has changed.
    pub fn frame(&mut self, f: &Fonts, info: &DeviceInfo) -> &Frame {
        if self.frame.is_none() {
            let t = self.theme();
            let screen = self.screen();
            let cx = Ctx { t: &t, f, info, state: &self.state, term: self.term.as_ref() };
            let frame = pages::render(screen, &cx);
            self.height = frame.height(&t);
            self.frame = Some(frame);
            self.clamp_scroll();
            if std::mem::take(&mut self.snap) {
                let (title, bar) = self.bar_wanted().expect("rendered above");
                self.title.jump(title);
                self.bar.jump(bar);
            }
        }
        self.frame.as_ref().expect("rendered above")
    }

    fn max_scroll(&self) -> f32 {
        (self.height - self.h as f32).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_scroll();
        let top = self.top_mut();
        top.1 = top.1.clamp(0.0, max);
    }

    /// Draw what the panel shows now: the visible part of the page, pulled
    /// past an end if it is, its large title fading as it scrolls, the bar
    /// and its title over a scrolled page, the `<`, the status bar, any
    /// message, and the back cue while a swipe from an edge is under way.
    pub fn compose(&mut self, out: &mut Canvas, f: &Fonts, info: &DeviceInfo, clock: &str, now: u64) {
        self.frame(f, info);
        let screen = self.screen();
        let caret_on = match self.frame.as_ref().is_some_and(|fr| fr.caret.is_some()) {
            true => self.caret_up(screen, now),
            false => {
                self.blink = None;
                self.caret_shown = false;
                false
            }
        };
        if let Some(m) = &self.morph {
            if !m.at(now).1 {
                return self.compose_morph(out, f, info, clock, now, caret_on);
            }
            self.morph = None;
        }
        let t = self.theme();
        let m = t.m;
        if let Some((which, track)) = self.knob.take() {
            self.paint_knob(&t, f, which, track);
        }
        if self.toast.as_ref().is_some_and(|m| now >= m.1) {
            self.toast = None;
        }
        let title = self.frame.as_ref().expect("rendered above").title;
        let key = (title, self.state.dark);
        if self.titles.as_ref().is_none_or(|(k, _, _)| *k != key) {
            self.titles = Some((key, Blurs::new(widgets::big_title(&t, f, title)), Blurs::new(widgets::small_title(&t, f, title))));
        }
        if self.chip.as_ref().is_none_or(|c| c.0 != t.dark) {
            let (art, x, y) = widgets::back_chip(&t);
            self.chip = Some((t.dark, art, x, y));
        }
        let scroll = self.top().1;
        let offset = (scroll + self.overscroll(now)).round();
        let light = self.light(offset);
        // The light goes over its control, in the part of the screen the
        // control is drawn in, under whatever is laid over that part.
        let shine = |out: &mut Canvas, part: Group| {
            if let Some((g, r, radius, tint)) = light {
                if g == part {
                    out.fill_round_rect(r.x, r.y, r.w, r.h, radius, tint);
                }
            }
        };
        let frame = self.frame.as_ref().expect("rendered above");
        let (_, big, small) = self.titles.as_mut().expect("drawn above");

        if let Some(art) = &frame.hero {
            // The field fills the screen and fades as the page scrolls; the
            // cards scroll over it. The version fades and shrinks away
            // first, then the wordmark.
            let glow = self.glow.as_ref().expect("the About page keeps its field");
            let fade = (1.0 - scroll / m.dp(ABOUT_FIELD_DP)).clamp(0.0, 1.0);
            let top = (widgets::bar_bottom(&t) + m.dp(ABOUT_FIELD_DP)) / m.h as f32;
            let field = glow::sample(glow, out.w as usize, out.h as usize, FIELD_CELL, top, t.pal.bg, fade);
            let dither = self.dither.get_or_insert_with(|| glow::Dither::new(out.w as usize, out.h as usize));
            glow::paint(out, &field, fade, &glow::Layer { canvas: &frame.canvas, runs: &art.runs, offset: offset as isize }, dither);
            let range = m.dp(ABOUT_LOGO_DP) - widgets::bar_bottom(&t);
            let v = (scroll / range).clamp(0.0, 1.0);
            art.version.draw_scaled(out, 0.0, 0.0, 1.0 - 0.1 * v, t.hero.version, 1.0 - v);
            let l = ((scroll - range) / m.dp(ABOUT_LOGO_FADE_DP)).clamp(0.0, 1.0);
            glow::ink_wordmark(out, &field, &glow.look().ink, &art.wordmark, 1.0 - 0.1 * l, 1.0 - l, fade, dither);
        } else {
            // The page rows on screen above a long list's rows, those rows,
            // and the page colour past the ends; every pixel is written
            // once.
            let (row, h) = (out.w as usize * 4, out.h as f32);
            let span = frame.rows.as_ref().map(|r| ((r.edges[0] - offset).clamp(0.0, h), (r.bottom() - offset).clamp(0.0, h)));
            let (list_top, list_end) = span.unwrap_or((h, h));
            let first = (-offset).clamp(0.0, h) as usize;
            let last = (frame.canvas.h as f32 - offset).clamp(first as f32, list_top) as usize;
            if first < last {
                let from = (offset as isize + first as isize) as usize * row;
                out.data[first * row..last * row].copy_from_slice(&frame.canvas.data[from..from + (last - first) * row]);
            }
            out.fill_rect(0.0, 0.0, out.w as f32, first as f32, frame.bg);
            out.fill_rect(0.0, last as f32, out.w as f32, list_top - last as f32, frame.bg);
            let below = list_end.max(last as f32);
            out.fill_rect(0.0, below, out.w as f32, h - below, frame.bg);
            if let Some(rows) = &frame.rows {
                // A long list's rows on screen, each drawn when it first
                // shows. Those more than half a screen away are let go, and
                // their canvases drawn over again for the rows coming in, as
                // a list view recycles its rows.
                let near = rows.within(offset - h * NEAR_SCREENS, offset + h * (1.0 + NEAR_SCREENS));
                self.spare.extend(self.drawn.extract_if(.., |(i, _)| !near.contains(i)).map(|(_, c)| c));
                for i in rows.within(offset, offset + h) {
                    let k = match self.drawn.iter().position(|(j, _)| *j == i) {
                        Some(k) => k,
                        None => {
                            let mut c = self.spare.pop().unwrap_or_else(|| Canvas::new(out.w, 0));
                            c.h = (rows.edges[i + 1] - rows.edges[i]) as u32;
                            c.data.resize((c.w * c.h * 4) as usize, 0);
                            let cx = Ctx { t: &t, f, info, state: &self.state, term: self.term.as_ref() };
                            pages::row(screen, &cx, i, &mut c);
                            self.drawn.push((i, c));
                            self.drawn.len() - 1
                        }
                    };
                    out.put(&self.drawn[k].1, (rows.edges[i] - offset) as i32);
                }
            } else {
                // Off a long list, its rows' canvases are not kept.
                self.spare.clear();
            }
            if frame.header {
                let level = (1.0 - scroll / m.dp(TITLE_FADE_DP)).clamp(0.0, 1.0);
                draw_faded(out, big, 0.0, -offset, t.pal.text, level, (1.0 - level) * m.dp(TITLE_BLUR_DP));
            }
        }
        if let Some(caret) = frame.caret.as_ref().filter(|_| caret_on) {
            out.lay(&caret.art, caret.x, caret.y - offset as i32, 1.0);
        }
        let bar = self.bar.level(now);
        let strip = frame.pinned.as_ref().map(|p| (p, pinned_top(&t, p, offset)));
        shine(out, Group::Page);
        if bar > 0.0 {
            match strip {
                // The strip under the bar is veiled with it, and the list
                // thins out below the strip.
                Some((p, top)) => widgets::strip_mask(out, &t, frame.bg, bar, top.round(), top.round() + p.canvas.h as f32),
                None => {
                    let stops = if frame.hero.is_some() { &widgets::ABOUT_BAR_MASK[..] } else { &widgets::BAR_MASK[..] };
                    widgets::bar_mask(out, &t, stops, frame.bg, bar);
                }
            }
        }
        if let Some((p, top)) = strip {
            out.put(&p.canvas, top.round() as i32);
            shine(out, Group::Strip);
        }
        if let Some(d) = &frame.dock {
            out.put(&d.canvas, (out.h - d.canvas.h) as i32);
            shine(out, Group::Dock);
        }
        if frame.back {
            let (_, chip, x, y) = self.chip.as_ref().expect("drawn above");
            out.lay(chip, *x, *y, bar);
            widgets::back_arrow(out, &t, t.pal.text);
            shine(out, Group::Fixed);
        }
        let level = self.title.level(now);
        let blur = if frame.hero.is_some() { 0.0 } else { (1.0 - level) * m.dp(TITLE_BLUR_DP) };
        draw_faded(out, small, 0.0, 0.0, t.pal.text, level, blur);

        let battery = info.battery.as_ref().map(|b| (b.level, b.plugged()));
        widgets::status_bar(out, &t, f, &Bar { clock, battery }, t.pal.status);
        if let Some((text, _)) = &self.toast {
            widgets::toast(out, &t, f, text);
        }
        match self.drag {
            Drag::Edge { start, from_left, armed, x, cy } => {
                let travel = if from_left { x - start } else { start - x };
                widgets::back_cue(out, &t, from_left, cy, widgets::back_cue_depth(&t, travel), armed);
            }
            _ => {
                if let Some((from_left, cy, s)) = self.cue {
                    widgets::back_cue(out, &t, from_left, cy, s.at(now).0.max(0.0), false);
                }
            }
        }
    }

    /// Whether the cursor of the text field on `screen` shows at `now`: it
    /// comes up whole on each page it shows on and at each edit, then
    /// blinks.
    fn caret_up(&mut self, screen: Screen, now: u64) -> bool {
        let key = (screen, self.edits);
        if self.blink.is_none_or(|(s, e, _)| (s, e) != key) {
            self.blink = Some((screen, self.edits, now));
        }
        self.caret_shown = self.blink_at(now).is_some_and(|(on, _)| on);
        self.caret_shown
    }

    /// Draw the search bar on its way between the home page and the search
    /// page; see [`Morph`].
    fn compose_morph(&mut self, out: &mut Canvas, f: &Fonts, info: &DeviceInfo, clock: &str, now: u64, caret_on: bool) {
        let t = self.theme();
        let m = t.m;
        let frame = self.frame.as_ref().expect("rendered above");
        let mut morph = self.morph.take().expect("composing it");
        let (q, _) = morph.at(now);
        let (field, _) = widgets::search_layout(&t);
        let home = Rect::new(m.margin, m.content_top - morph.scroll, m.w as f32 - m.margin * 2.0, m.search_h);
        let along = |a: f32, b: f32| a + (b - a) * q;
        let bar = Rect::new(m.margin, along(home.y, field.y), along(home.w, field.w), m.search_h);
        let rise = bar.y - home.y;
        out.fill_rect(0.0, 0.0, out.w as f32, out.h as f32, frame.bg);
        if morph.into {
            // The search page's keyboard and what it shows under its
            // field, from its frame; that comes in once the bar is nearly
            // there.
            let (_, keys_top) = *morph.keyboard.get_or_insert_with(|| keyboard_strip(&t, frame));
            let (body, top) = morph.body.get_or_insert_with(|| {
                let top = (field.y + field.h).ceil() as i32;
                let mut body = Canvas::new(frame.canvas.w, (keys_top - top).max(0) as u32);
                body.put_rows(&frame.canvas, top, keys_top, 0);
                (body, top)
            });
            let (delay, span) = MORPH_BODY_MS;
            out.lay(body, 0, *top, (now.saturating_sub(morph.start + delay) as f32 / span as f32).min(1.0));
        } else {
            // The home page under its bar comes down with it.
            let below = m.content_top + m.search_h;
            out.put_rows(&frame.canvas, below.floor() as i32, frame.canvas.h as i32, (below - morph.scroll + rise).floor() as i32);
        }
        let title = morph.title.get_or_insert_with(|| Blurs::new(widgets::big_title(&t, f, pages::HOME_TITLE)));
        let level = (1.0 - q / MORPH_TITLE_GONE).clamp(0.0, 1.0);
        draw_faded(out, title, 0.0, rise - morph.scroll, t.pal.text, level, (1.0 - level) * m.dp(TITLE_BLUR_DP));
        if let Some((keys, top)) = &morph.keyboard {
            out.put(keys, (*top as f32 + (1.0 - q) * keys.h as f32).round() as i32);
        }
        widgets::search_capsule(out, &t, f, bar, pages::SEARCH_HINT);
        // The cursor shows from the start, going up with the field.
        if let Some(caret) = frame.caret.as_ref().filter(|_| morph.into && caret_on) {
            out.lay(&caret.art, caret.x, caret.y + (bar.y - field.y).round() as i32, 1.0);
        }
        let (disc, cross) = morph.cancel.get_or_insert_with(|| {
            let (disc, cross) = widgets::cancel_art(&t);
            (Blurs::new(disc), Blurs::new(cross))
        });
        let shown = ((q - MORPH_CANCEL.0) / (MORPH_CANCEL.1 - MORPH_CANCEL.0)).clamp(0.0, 1.0);
        let blur = (1.0 - shown) * m.dp(MORPH_CANCEL_BLUR_DP);
        draw_faded(out, disc, 0.0, 0.0, t.pal.search, shown, blur);
        draw_faded(out, cross, 0.0, 0.0, t.pal.text, shown, blur);
        self.morph = Some(morph);
        let battery = info.battery.as_ref().map(|b| (b.level, b.plugged()));
        widgets::status_bar(out, &t, f, &Bar { clock, battery }, t.pal.status);
        if let Some((text, _)) = &self.toast {
            widgets::toast(out, &t, f, text);
        }
    }

    /// Put the page back where it rests, for a page just opened or returned
    /// to; its first frame shows the bar as its scroll has it.
    fn settle_page(&mut self) {
        self.height = 0.0;
        self.fling = None;
        self.pull = 0.0;
        self.bounce = None;
        self.seek = None;
        self.adjusting = false;
        self.titles = None;
        self.glow = None;
        let hero = self.screen() == Screen::About;
        self.title = title_fade(hero);
        self.bar = bar_fade(hero);
        self.snap = true;
    }

    /// Show the user what the runtime has to say about work it was asked
    /// to do.
    pub fn report(&mut self, text: &str, now: u64) {
        self.say(text, now);
    }

    /// File transfer could not be turned on or off as the switch says: it
    /// shows how it stands, and why.
    pub fn file_transfer_failed(&mut self, on: bool, why: &str, now: u64) {
        self.state.file_transfer = on;
        self.refresh();
        self.say(why, now);
    }

    fn say(&mut self, text: &str, now: u64) {
        self.toast = Some((text.to_string(), now + TOAST_MS));
        self.dirty = true;
    }

    fn open(&mut self, screen: Screen) {
        self.stack.push((screen, 0.0, Focus::Unset));
        self.settle_page();
        self.refresh();
    }

    /// Set the home page's search bar moving up to the search page's field,
    /// or back down, before the page changes: from where a move not yet
    /// done has it, at the speed it has.
    fn start_morph(&mut self, into: bool, now: u64) {
        let (q, speed, title, cancel) = match self.morph.take() {
            Some(m) => {
                let (q, _) = m.at(now);
                let speed = (q - m.at(now.saturating_sub(8)).0) / 0.008;
                (q, speed, m.title, m.cancel)
            }
            None => (if into { 0.0 } else { 1.0 }, 0.0, None, None),
        };
        let target = if into { 1.0 } else { 0.0 };
        // The home page is the page shown, or the one under the search.
        let home = self.stack.len() - if into { 1 } else { 2 };
        // On the way down the keyboard goes with the search page's frame.
        let keyboard = if into { None } else { self.frame.as_ref().map(|fr| keyboard_strip(&self.theme(), fr)) };
        self.morph = Some(Morph {
            into,
            spring: Spring::new(now, (q - target) * 1000.0, speed * 1000.0, MORPH_PERIOD),
            start: now,
            scroll: self.stack[home].1,
            keyboard,
            body: None,
            title,
            cancel,
        });
        self.dirty = true;
    }

    /// Go back a page. Leaving the installing page stops what can be
    /// stopped; while a package is being read or written, the page stays.
    /// In the file manager, back climbs a folder until it reaches the root,
    /// and in a place, until it reaches the place's top.
    fn back(&mut self, now: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.stack.len() <= 1 {
            return fx;
        }
        if self.screen() == Screen::Search && self.stack[self.stack.len() - 2].0 == Screen::Home {
            self.start_morph(false, now);
        }
        match self.screen() {
            Screen::Mount => self.state.mount_failure = None,
            // A slot picked and not switched to is put down.
            Screen::Slot => self.state.next_slot = None,
            Screen::Files => {
                if let Some(up) = self.state.files.up() {
                    return vec![Effect::ReadDir(up)];
                }
                // Leaving the file manager puts a picked entry down.
                self.state.clip = None;
            }
            Screen::Choose(place) => {
                if let Some(up) = self.state.folder(Screen::Choose(place)).up() {
                    return vec![Effect::ReadPlace(place, up)];
                }
            }
            Screen::FileInfo => {
                self.state.file = None;
                self.state.asking_delete = false;
            }
            // Turning the choice down leaves the entry picked, for another
            // folder.
            Screen::Clash => self.state.clash = None,
            Screen::FileJob => {
                if self.state.job.as_ref().is_some_and(|j| j.outcome.is_none()) {
                    self.say("请等待完成或停止后再返回", now);
                    return fx;
                }
                self.state.job = None;
            }
            Screen::RestoreBackup => {
                self.state.restore = None;
                self.state.asking_delete = false;
            }
            // A restore that has begun writing cannot be stopped.
            Screen::BackupJob => {
                let job = self.state.backup_job.as_ref().expect("the backup page shows work");
                if job.outcome.is_none() {
                    let writing = job.restore && job.at.is_some_and(|a| !a.checking);
                    self.say(if writing { "正在恢复，请等待恢复完成" } else { "请等待完成或停止后再返回" }, now);
                    return fx;
                }
                self.state.backup_job = None;
            }
            _ => {}
        }
        if self.screen() == Screen::Installing {
            match self.state.install {
                Install::Checking { .. } => {
                    self.say("正在读取文件，请稍后再试", now);
                    return fx;
                }
                Install::Updating { .. } => {
                    self.say("正在安装，请等待安装完成", now);
                    return fx;
                }
                Install::Flashing { .. } => {
                    self.say("正在刷入，请等待刷入完成", now);
                    return fx;
                }
                Install::Scripting { .. } => {
                    self.say("正在安装，请等待安装完成", now);
                    return fx;
                }
                Install::Starting | Install::Waiting | Install::Choosing(_) | Install::Confirming { .. } | Install::ConfirmingScript => {
                    self.state.install = Install::Ending;
                    fx.push(Effect::Cancel);
                }
                Install::Finished { .. } => self.state.install = Install::Idle,
                Install::Idle | Install::Ending => {}
            }
        }
        self.stack.pop();
        self.state.confirm = 0.0;
        self.settle_page();
        self.refresh();
        // Coming back to the list of backups looks for them again: one may
        // have been made, deleted or opened up to be read since.
        if self.screen() == Screen::Restore {
            fx.push(self.find_backups());
        }
        fx
    }

    /// Hear from the installer. Once the installing page is left, only its
    /// end matters. An image waiting for its partition, an update or a
    /// script waiting to be agreed to, and the end light a dark panel: all
    /// want the user.
    pub fn install(&mut self, ev: Event, now: u64) {
        if self.state.install == Install::Ending {
            if matches!(ev, Event::Finished(_)) {
                self.state.install = Install::Idle;
            }
            return;
        }
        if matches!(ev, Event::Image { .. } | Event::Ready { .. } | Event::Script | Event::Finished(_)) {
            self.wake(now);
        }
        let install = &mut self.state.install;
        match ev {
            Event::Waiting => *install = Install::Waiting,
            Event::Checking { size } => *install = Install::Checking { size },
            Event::Ready { version, wipe } => *install = Install::Confirming { version, wipe },
            Event::Installing { version } => *install = Install::Updating { version, stage: None, overwriting: false },
            Event::Overwriting => {
                if let Install::Updating { overwriting, .. } = install {
                    *overwriting = true;
                }
            }
            Event::Image { kind, size, partitions } => {
                *install = Install::Choosing(Choice { kind, size, partitions, partition: 0, pick: Pick::Current })
            }
            Event::Script => *install = Install::ConfirmingScript,
            Event::Running => {
                *install = Install::Scripting { stage: None, running: true };
                self.state.script_output.clear();
            }
            Event::Print(line) => {
                let shown = &mut self.state.script_output;
                if shown.len() == SCRIPT_LINES {
                    shown.remove(0);
                }
                shown.push(line);
            }
            Event::Progress(stage) => match install {
                Install::Updating { stage: shown, .. } | Install::Flashing { stage: shown, .. } | Install::Scripting { stage: shown, .. } => {
                    *shown = Some(stage)
                }
                _ => {}
            },
            Event::Reflashed(reflash) => self.state.reflash = Some(reflash),
            Event::Finished(outcome) => {
                let damage = match (&outcome, &*install) {
                    (Outcome::Failed(_), Install::Updating { overwriting: true, .. }) => Some(Damage::System),
                    (Outcome::Failed(_), Install::Flashing { partition, stage: Some(Stage::Flashing { done, .. }) }) if *done > 0 => {
                        Some(Damage::Partition(partition))
                    }
                    (Outcome::Failed(_), Install::Scripting { running: true, .. }) => Some(Damage::Script),
                    _ => None,
                };
                *install = Install::Finished { outcome, damage };
            }
        }
        if self.screen() == Screen::Installing {
            self.refresh();
        }
    }

    /// What the page shown first asks of the runtime as it shows.
    pub fn arrive(&mut self) -> Vec<Effect> {
        self.arrive_at(self.screen())
    }

    /// What a page asks of the runtime as it opens: what it shows read, or
    /// started. The decrypt page opens the data partition, which is also
    /// how it learns which control a user's screen lock is answered with.
    fn arrive_at(&mut self, s: Screen) -> Vec<Effect> {
        match s {
            Screen::Mount => vec![Effect::ReadMounts],
            Screen::Slot | Screen::Reboot => vec![Effect::ReadSlots],
            Screen::Restore => vec![self.find_backups()],
            Screen::Files => vec![Effect::ReadDir(self.state.files.path.clone())],
            Screen::Choose(place) if self.state.place_open(place) => vec![Effect::ReadPlace(place, self.state.folder(s).path.clone())],
            Screen::Search => {
                self.state.search.clear();
                self.state.layer = Layer::Letters;
                Vec::new()
            }
            Screen::Terminal => {
                self.state.layer = Layer::Letters;
                if self.term.is_none() {
                    vec![Effect::StartShell]
                } else {
                    Vec::new()
                }
            }
            Screen::Decrypt if matches!(self.state.data, Data::Closed | Data::Failed(_)) => self.open_data(),
            Screen::Update => vec![Effect::ReadUpdate],
            _ => Vec::new(),
        }
    }

    /// Where the update stands, as the runtime read it or a merge has come.
    pub fn set_update(&mut self, standing: Standing) {
        if self.state.update != standing {
            self.state.update = standing;
            self.refresh();
        }
    }

    /// Open the data partition, the decrypt page saying so meanwhile.
    fn open_data(&mut self) -> Vec<Effect> {
        self.state.data = Data::Opening;
        self.refresh();
        vec![Effect::OpenData]
    }

    /// Carry out a tapped control.
    fn act(&mut self, action: Action, now: u64) -> Vec<Effect> {
        match action {
            Action::Open(s) => {
                if s == Screen::Search && self.screen() == Screen::Home {
                    self.start_morph(true, now);
                }
                self.open(s);
                return self.arrive_at(s);
            }
            Action::OpenData => return self.open_data(),
            Action::Skip => {
                if self.stack.len() > 1 {
                    self.stack.pop();
                } else {
                    *self.top_mut() = (Screen::Home, 0.0, Focus::Unset);
                }
                self.settle_page();
                self.refresh();
            }
            Action::Back | Action::CloseSearch => return self.back(now),
            Action::Theme { dark } => {
                if self.state.dark != dark {
                    self.state.dark = dark;
                    self.refresh();
                }
            }
            Action::Toggle(switch) => {
                let on = match switch {
                    Switch::Vibrate => &mut self.state.vibrate,
                    Switch::FileTransfer => &mut self.state.file_transfer,
                    Switch::Compress => &mut self.state.compress,
                    Switch::Verify => &mut self.state.verify,
                    Switch::KeepRecovery => &mut self.state.keep_recovery,
                };
                *on = !*on;
                let now_on = *on;
                self.refresh();
                if switch == Switch::FileTransfer {
                    return vec![Effect::FileTransfer(now_on)];
                }
            }
            Action::MountToggle(i) => {
                if self.state.mount_failure.take().is_some() {
                    self.refresh();
                }
                let row = &self.state.mounts.as_ref().expect("a partition toggled is one the page lists")[i];
                let base = row.base.clone();
                return vec![if row.mounted { Effect::Unmount(base) } else { Effect::Mount(base) }];
            }
            Action::RebootTarget(i) => {
                self.state.reboot = i;
                self.refresh();
            }
            // Everything takes in every part, so choosing it leaves the
            // parts out, and choosing a part on its own leaves it out.
            Action::WipeItem(item) => {
                let k = WIPES.iter().position(|&w| w == item).expect("the page offers only what it lists");
                let on = !self.state.wipe[k];
                match (on, item) {
                    (true, Wipe::All) => {
                        self.state.wipe = [false; WIPES.len()];
                        self.state.wipe[k] = true;
                    }
                    (true, _) => {
                        self.state.wipe[0] = false;
                        self.state.wipe[k] = true;
                    }
                    (false, _) => self.state.wipe[k] = false,
                }
                self.refresh();
            }
            Action::Confirm(_) => self.say("请按住圆形按钮向右滑动", now),
            Action::Brightness | Action::Pattern => {}
            Action::Sideload => match self.state.install {
                Install::Idle | Install::Finished { .. } => {
                    self.begin_install(Install::Starting, Source::Computer, None);
                    if self.screen() == Screen::Installing {
                        self.refresh();
                    } else {
                        self.open(Screen::Installing);
                    }
                    return vec![Effect::Sideload];
                }
                _ => self.say("上一次安装尚未结束，请稍后再试", now),
            },
            Action::FlashPartition(k) => {
                if let Install::Choosing(choice) = &mut self.state.install {
                    choice.partition = k;
                    self.refresh();
                }
            }
            Action::FlashSlots(pick) => {
                if let Install::Choosing(choice) = &mut self.state.install {
                    choice.pick = pick;
                    self.refresh();
                }
            }
            Action::RebootTo(target) => return vec![Effect::Reboot(target)],
            Action::EnterRecovery => return vec![Effect::EnterRecovery],
            Action::Dismiss => {
                self.stack = vec![(Screen::Home, 0.0, Focus::Unset)];
                self.settle_page();
                self.refresh();
                return vec![Effect::Dismiss];
            }
            Action::PickEntry(op) => {
                let path = self.entry_path();
                self.state.clip = Some(Clip { op, path });
                return self.back(now);
            }
            // A folder cannot go inside itself, and an entry moved to the
            // folder it is in stays put: the entry stays picked for another
            // folder.
            Action::Paste => {
                let clip = self.state.clip.clone().expect("only a picked entry is pasted");
                let (path, folder) = (std::path::Path::new(&clip.path), std::path::Path::new(&self.state.files.path));
                let name = path.file_name().map_or_else(|| clip.path.clone(), |n| n.to_string_lossy().into_owned());
                if folder.starts_with(path) {
                    self.say(&format!("无法将\"{name}\"放入其自身中"), now);
                } else if clip.op == FileOp::Move && path.parent() == Some(folder) {
                    self.say(&format!("\"{name}\"已在该文件夹中"), now);
                } else {
                    return vec![Effect::Paste { op: clip.op, path: clip.path, into: self.state.files.path.clone(), clash: None }];
                }
            }
            Action::Unpick => {
                self.state.clip = None;
                self.refresh();
            }
            Action::RenameEntry => {
                self.state.new_name = self.state.file.as_ref().expect("the file page shows an entry").name.clone();
                self.state.layer = Layer::Letters;
                self.open(Screen::Rename);
            }
            Action::AskDelete => {
                self.state.asking_delete = true;
                self.state.confirm = 0.0;
                self.refresh();
            }
            Action::Resolve(choice) => {
                let (clip, into, _) = self.state.clash.take().expect("a choice answers a clash");
                self.stack.pop();
                self.settle_page();
                self.refresh();
                // Skipping the one entry picked leaves nothing to do.
                if choice == ClashChoice::Skip {
                    self.state.clip = None;
                    return Vec::new();
                }
                return vec![Effect::Paste { op: clip.op, path: clip.path, into, clash: Some(choice) }];
            }
            Action::StopJob => {
                let job = self.state.job.as_mut().expect("only work under way is stopped");
                if job.outcome.is_none() && !job.stopping {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopJob];
                }
            }
            Action::ScreenOff(k) => {
                self.state.screen_off = k;
                self.refresh();
            }
            Action::TimeZone(k) => {
                self.state.time_zone = k;
                self.refresh();
            }
            Action::BackupPart(k) => {
                self.state.backup[k] = !self.state.backup[k];
                self.refresh();
            }
            Action::BackupTo(place) => {
                self.state.backup_to = place;
                self.refresh();
            }
            // All a backup holds is ticked to restore at first.
            Action::OpenBackup(i) => {
                let entry = self.state.backups[i].clone();
                let parts = entry.held.as_ref().map_or([false; 4], |h| h.parts);
                self.state.restore = Some((entry, parts));
                self.state.asking_delete = false;
                self.open(Screen::RestoreBackup);
            }
            Action::RestorePart(k) => {
                let (_, parts) = self.state.restore.as_mut().expect("the restore page shows a backup");
                parts[k] = !parts[k];
                self.refresh();
            }
            Action::StopBackup => {
                let job = self.state.backup_job.as_mut().expect("only work under way is stopped");
                if job.outcome.is_none() && !job.stopping {
                    job.stopping = true;
                    self.refresh();
                    return vec![Effect::StopBackup];
                }
            }
            Action::LogPart(k) => {
                self.state.logs[k] = !self.state.logs[k];
                self.refresh();
            }
            Action::LogsTo(k) => {
                self.state.logs_to = k;
                self.refresh();
            }
            Action::ExportLogs => {
                let ([recovery, kernel], storage) = (self.state.logs, self.state.logs_to == 1);
                if !recovery && !kernel {
                    self.say("请选择要导出的日志", now);
                } else if storage && !self.state.data.storage_open() {
                    self.say("请先完成「解密」", now);
                } else {
                    return vec![Effect::ExportLogs { recovery, kernel, storage }];
                }
            }
            Action::PickSlot(slot) => {
                self.state.next_slot = Some(slot);
                self.refresh();
            }
            Action::Type(cap) if self.screen() == Screen::Terminal => return self.term_typed(cap),
            Action::Type(cap) => return self.type_key(cap, now),
            Action::TermKey(key) => {
                let Some(term) = self.term.as_mut() else { return Vec::new() };
                let bytes = term.key(key);
                let running = term.running;
                self.refresh();
                if running && !bytes.is_empty() {
                    return vec![Effect::TermInput(bytes)];
                }
            }
            Action::ClearSearch => {
                self.state.search.clear();
                self.edits += 1;
                self.refresh();
            }
            Action::OpenEntry(i) => {
                let entry = self.state.files.entry(i).clone();
                match entry.kind {
                    FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => {
                        let scroll = self.top().1;
                        self.state.files.leave(scroll);
                        return vec![Effect::ReadDir(self.state.files.inside(&entry.name))];
                    }
                    FileKind::File | FileKind::Link { folder: false, .. } | FileKind::Special => {
                        self.state.file = Some(entry);
                        self.open(Screen::FileInfo);
                    }
                }
            }
            Action::ChooseEntry(i) => {
                let screen = self.screen();
                let Screen::Choose(place) = screen else { unreachable!("only a place's folder is chosen in") };
                let folder = self.state.folder(screen);
                let entry = folder.entry(i).clone();
                let path = folder.inside(&entry.name);
                match entry.kind {
                    FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => {
                        let scroll = self.top().1;
                        self.state.folder_mut(screen).leave(scroll);
                        return vec![Effect::ReadPlace(place, path)];
                    }
                    FileKind::File | FileKind::Link { folder: false, .. } => return self.install_file(place, path, entry.size, now),
                    FileKind::Special => unreachable!("a place lists no special files"),
                }
            }
            Action::OpenAncestor(depth) => {
                let screen = self.screen();
                if let Some(path) = self.state.folder(screen).ancestor(depth) {
                    return vec![match screen {
                        Screen::Choose(place) => Effect::ReadPlace(place, path),
                        _ => Effect::ReadDir(path),
                    }];
                }
            }
            Action::Unavailable(text) => self.say(text, now),
        }
        Vec::new()
    }

    /// The text the keyboard types into on this page.
    fn typing(&mut self) -> &mut String {
        match self.screen() {
            Screen::Search => &mut self.state.search,
            Screen::Decrypt => &mut self.state.code,
            Screen::Rename => &mut self.state.new_name,
            s => panic!("{s:?} has no text field"),
        }
    }

    /// Where the entry the file page shows is.
    fn entry_path(&self) -> String {
        let entry = self.state.file.as_ref().expect("the file page shows an entry");
        self.state.files.inside(&entry.name)
    }

    /// Install the file of `size` bytes at `path` in `place`, its page
    /// following how it goes; one install at a time.
    fn install_file(&mut self, place: Place, path: String, size: u64, now: u64) -> Vec<Effect> {
        match self.state.install {
            Install::Idle | Install::Finished { .. } => {
                let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned());
                self.begin_install(Install::Checking { size }, Source::File(place), name);
                self.open(Screen::Installing);
                vec![Effect::InstallFile(place, path)]
            }
            _ => {
                self.say("上一次安装尚未结束，请稍后再试", now);
                Vec::new()
            }
        }
    }

    /// A key of the keyboard on the terminal page. The layer keys change
    /// the keyboard; every other key goes to the shell, and once the shell
    /// has ended, Enter starts another.
    fn term_typed(&mut self, cap: Cap) -> Vec<Effect> {
        match cap {
            Cap::Shift => self.state.layer = if self.state.layer == Layer::Shifted { Layer::Letters } else { Layer::Shifted },
            Cap::Show(layer) => self.state.layer = layer,
            _ => {}
        }
        let shifted = self.state.layer == Layer::Shifted;
        if shifted && matches!(cap, Cap::Char(_)) {
            self.state.layer = Layer::Letters;
        }
        self.refresh();
        let Some(term) = self.term.as_mut() else { return Vec::new() };
        if !term.running {
            return if cap == Cap::Enter { vec![Effect::StartShell] } else { Vec::new() };
        }
        let bytes = term.typed(cap, shifted);
        if bytes.is_empty() {
            Vec::new()
        } else {
            vec![Effect::TermInput(bytes)]
        }
    }

    /// Send what was entered at the screen lock of the user whose own
    /// files wait. It is taken as it is sent, so the dots clear while it is
    /// checked, and nothing more is entered until it has been.
    fn unlock(&mut self, code: String) -> Vec<Effect> {
        let Data::Locked { user, keys, .. } = self.state.data else { unreachable!("only a screen lock waiting takes what is entered") };
        self.state.data = Data::Locked { user, keys, checking: true };
        self.refresh();
        vec![Effect::Unlock { user, code }]
    }

    /// Scroll the terminal's screen as a finger goes from `last` to `y`: a
    /// row for each row's height, and down shows what is older.
    fn scroll_history(&mut self, last: f32, y: f32, carry: f32, pitch: f32) -> Drag {
        let carry = carry + (y - last);
        let rows = (carry / pitch).trunc();
        if rows != 0.0 {
            if let Some(term) = self.term.as_mut() {
                term.scroll(rows as isize);
                self.refresh();
            }
        }
        Drag::History { last: y, carry: carry - rows * pitch, pitch }
    }

    /// A key of the on-screen keyboard. A capital lasts one letter; return
    /// opens the first result of a search, and runs a terminal's command.
    fn type_key(&mut self, cap: Cap, now: u64) -> Vec<Effect> {
        match cap {
            Cap::Char(ch) => {
                let shifted = self.state.layer == Layer::Shifted;
                self.typing().push(if shifted { ch.to_ascii_uppercase() } else { ch });
                self.edits += 1;
                if shifted {
                    self.state.layer = Layer::Letters;
                }
            }
            Cap::Space => {
                self.typing().push(' ');
                self.edits += 1;
            }
            Cap::Backspace => {
                self.typing().pop();
                self.edits += 1;
            }
            Cap::Shift => {
                self.state.layer = if self.state.layer == Layer::Shifted { Layer::Letters } else { Layer::Shifted };
            }
            Cap::Show(layer) => self.state.layer = layer,
            Cap::Enter => match self.screen() {
                Screen::Search => {
                    if let Some(first) = search::find(&self.state.search).first() {
                        return self.act(Action::Open(first.screen), now);
                    }
                }
                Screen::Decrypt if self.state.code.is_empty() => self.say("请输入锁屏密码", now),
                Screen::Decrypt => {
                    let code = std::mem::take(&mut self.state.code);
                    return self.unlock(code);
                }
                // A name the folder cannot take is turned down here, in
                // the user's words; an unchanged one leaves the page.
                Screen::Rename => {
                    let name = self.state.new_name.clone();
                    let current = &self.state.file.as_ref().expect("the file page shows an entry").name;
                    let taken = self.state.files.entries.as_ref().is_ok_and(|entries| entries.iter().any(|e| e.name == name));
                    match name.as_str() {
                        "" => self.say("请输入名称", now),
                        "." | ".." => self.say("名称不能为\".\"或\"..\"", now),
                        _ if name.contains('/') => self.say("名称不能包含\"/\"", now),
                        _ if name == *current => return self.back(now),
                        _ if taken => self.say_taken(now),
                        _ => return vec![Effect::Rename { path: self.entry_path(), name }],
                    }
                }
                other => unreachable!("{other:?} shows no keyboard"),
            },
        }
        self.refresh();
        Vec::new()
    }

    pub fn touch(&mut self, ev: Touch, f: &Fonts, now: u64) -> Vec<Effect> {
        if !self.display_on {
            // A finger on the dark panel lights it, and does nothing else
            // until it lifts.
            if matches!(ev, Touch::Down(..)) {
                self.wake(now);
                self.drag = Drag::Waking;
            }
            return Vec::new();
        }
        // A finger landing just after the panel lit only keeps it awake.
        if matches!(ev, Touch::Down(..)) && self.just_woke(now) {
            self.stir(now);
            self.drag = Drag::Waking;
            return Vec::new();
        }
        // A finger on the glass puts the keys' light out, and whatever a
        // key held down was doing comes to nothing.
        if matches!(ev, Touch::Down(..)) {
            if self.keys {
                self.keys = false;
                self.adjusting = false;
                self.dirty = true;
            }
            self.drop_button();
        }
        self.stir(now);
        let t = self.theme();
        let m = t.m;
        let (slop, edge) = (m.dp(8.0), m.dp(20.0));
        let scroll = self.top().1;
        let mut fx = Vec::new();
        match ev {
            Touch::Down(x, y) => {
                self.tracker.clear();
                self.tracker.add(now, y);
                let side = if x < edge {
                    Some(true)
                } else if x > m.w as f32 - edge {
                    Some(false)
                } else {
                    None
                };
                // The first page has nothing to go back to: its edges are
                // like the rest of it.
                let at_edge = side.filter(|_| self.stack.len() > 1);
                // A swipe in from an edge goes back over whatever the page is
                // doing: a finger landing there leaves a gliding or springing
                // page alone until it drags the page, and presses nothing.
                if at_edge.is_some() && self.in_motion(now) {
                    self.drag = Drag::Press { x, y, action: None, edge: at_edge, at: now };
                    return fx;
                }
                // Anywhere else it stops such a page and holds it; the
                // control under the finger is not pressed.
                if self.catch(now) {
                    self.drag = Drag::Scroll { last: y };
                    return fx;
                }
                // The `<` stays put while the page scrolls; under the bar
                // only it answers. A pinned strip answers where it shows,
                // over the rows beneath it, and so does a docked one.
                let back = widgets::back_rect(&t);
                let strip = self.frame.as_ref().and_then(|fr| fr.pinned.as_ref()).map(|p| (p, pinned_top(&t, p, scroll)));
                let on_strip = strip.filter(|&(p, top)| y >= top && y < top + p.canvas.h as f32);
                let dock = self.frame.as_ref().and_then(|fr| fr.dock.as_ref()).map(|d| (d, self.h as f32 - d.canvas.h as f32));
                let on_dock = dock.filter(|&(_, top)| y >= top);
                let hit = if self.frame.as_ref().is_some_and(|fr| fr.back) && back.contains(x, y) {
                    Some((Action::Back, back))
                } else if let Some((d, top)) = on_dock {
                    d.hits.iter().find(|h| h.rect.contains(x, y - top)).map(|h| (h.action, Rect { y: h.rect.y + top, ..h.rect }))
                } else if let Some((p, top)) = on_strip {
                    p.hits.iter().find(|h| h.rect.contains(x, y - top)).map(|h| (h.action, Rect { y: h.rect.y + top, ..h.rect }))
                } else if self.bar.on && y < widgets::bar_bottom(&t) {
                    None
                } else {
                    self.frame.as_ref().and_then(|fr| fr.hit(x, y + scroll)).map(|h| (h.action, h.rect))
                };
                let action = hit.map(|h| h.0);
                self.drag = match hit {
                    Some((Action::Brightness, track)) => {
                        self.set_brightness(&t, track, x);
                        Drag::Slider(track)
                    }
                    Some((Action::Confirm(which), track)) if self.on_knob(track, x) => {
                        let knob_x = track.x + (track.w - track.h) * self.state.confirm;
                        Drag::Confirm { which, track, grab: x - knob_x }
                    }
                    Some((Action::Pattern, _)) => {
                        self.state.pattern.clear();
                        self.trace(&t, x, y + scroll);
                        Drag::Pattern
                    }
                    _ => Drag::Press { x, y, action, edge: at_edge, at: now },
                };
            }
            Touch::Move(x, y) => match std::mem::replace(&mut self.drag, Drag::Idle) {
                Drag::Press { x: x0, y: y0, action, edge: at_edge, at } => {
                    self.tracker.add(now, y);
                    let (dx, dy) = (x - x0, y - y0);
                    self.drag = match at_edge {
                        Some(from_left) if dx.abs() > slop && dx.abs() > dy.abs() => {
                            self.dirty = true;
                            Drag::Edge { start: x0, from_left, armed: false, x, cy: y0 - m.dp(CUE_LIFT_DP) }
                        }
                        _ if dy.abs() > slop => {
                            let history = self.frame.as_ref().and_then(|fr| fr.scrollback).filter(|(r, _)| r.contains(x0, y0 + scroll));
                            if let Some((_, pitch)) = history {
                                self.scroll_history(y0, y, 0.0, pitch)
                            } else {
                                // A page still gliding or springing back is
                                // caught where it has got to; one at rest
                                // follows the finger from where it came down.
                                if !self.catch(now) {
                                    self.drag_by(y0 - y);
                                }
                                Drag::Scroll { last: y }
                            }
                        }
                        _ => Drag::Press { x: x0, y: y0, action, edge: at_edge, at },
                    };
                }
                Drag::Scroll { last } => {
                    self.tracker.add(now, y);
                    self.drag_by(last - y);
                    self.drag = Drag::Scroll { last: y };
                }
                Drag::Edge { start, from_left, armed, cy, .. } => {
                    let travel = if from_left { x - start } else { start - x };
                    let ready = travel > m.dp(BACK_SWIPE_DP);
                    // A tap of the motor as the `<` appears, as the system
                    // gives.
                    if ready && !armed && self.state.vibrate {
                        fx.push(Effect::Haptic);
                    }
                    self.drag = Drag::Edge { start, from_left, armed: ready, x, cy };
                    self.dirty = true;
                }
                Drag::Slider(track) => {
                    self.set_brightness(&t, track, x);
                    self.drag = Drag::Slider(track);
                }
                Drag::Confirm { which, track, grab } => {
                    let travel = (track.w - track.h).max(1.0);
                    self.state.confirm = ((x - grab - track.x) / travel).clamp(0.0, 1.0);
                    self.repaint_confirm(&t, f, which, track);
                    self.drag = Drag::Confirm { which, track, grab };
                }
                Drag::Pattern => {
                    self.trace(&t, x, y + scroll);
                    self.drag = Drag::Pattern;
                }
                Drag::History { last, carry, pitch } => self.drag = self.scroll_history(last, y, carry, pitch),
                Drag::Waking => self.drag = Drag::Waking,
                Drag::Held => self.drag = Drag::Held,
                Drag::Idle => {}
            },
            Touch::Up => match std::mem::replace(&mut self.drag, Drag::Idle) {
                Drag::Press { action: Some(a), .. } => fx.extend(self.act(a, now)),
                Drag::Scroll { .. } if self.pull != 0.0 => {
                    // A page let go past its end springs back.
                    self.bounce = Some(Spring::new(now, motion::rubber(self.pull, self.h as f32), 0.0, 0.4));
                    self.pull = 0.0;
                    self.dirty = true;
                }
                Drag::Scroll { .. } => {
                    // A finger lifted while still moving sends the page gliding on.
                    let speed = -self.tracker.velocity(now);
                    if speed.abs() >= m.dp(MIN_FLING_DP) {
                        let speed = speed.clamp(-m.dp(MAX_FLING_DP), m.dp(MAX_FLING_DP));
                        self.fling = Some(Fling::new(now, scroll, speed, self.px_per_dp * 160.0));
                    }
                }
                Drag::Edge { armed: true, .. } => fx.extend(self.back(now)),
                Drag::Edge { start, from_left, x, cy, .. } => {
                    // Short of going back, the cue sinks back into the edge.
                    let travel = if from_left { x - start } else { start - x };
                    let depth = widgets::back_cue_depth(&t, travel);
                    self.cue = Some((from_left, cy, Spring::new(now, depth, 0.0, 0.15)));
                    self.dirty = true;
                }
                Drag::Confirm { which, track, .. } => {
                    let done = self.state.confirm > SLID;
                    self.state.confirm = 0.0;
                    self.repaint_confirm(&t, f, which, track);
                    if done {
                        fx.extend(self.confirmed(which, now));
                    }
                }
                Drag::Pattern => fx.extend(self.drawn(now)),
                _ => {}
            },
        }
        // A key of the on-screen keyboard shows held down while the finger
        // rests on it.
        let held = match self.drag {
            Drag::Press { action: Some(Action::Type(cap)), .. } => Some(cap),
            _ => None,
        };
        if held != self.state.pressed {
            self.state.pressed = held;
            self.refresh();
        }
        fx
    }

    /// No touchscreen answers: the keys work the pages, their light shown
    /// from now on.
    pub fn touchscreen_missing(&mut self) {
        self.keys = true;
        self.dirty = true;
    }

    /// A button went down. With the keys working the page, the volume keys
    /// move the light from control to control and the power key works the
    /// one lit as it comes back up; otherwise the first volume press lights
    /// a control, and the power key turns the panel off.
    pub fn key(&mut self, key: Key, now: u64) -> Vec<Effect> {
        // Any button stops a gliding page.
        self.fling = None;
        if !self.display_on {
            // Any button lights the dark panel, and does only that.
            self.wake(now);
            self.button = Some((key, Press::Spent));
            return Vec::new();
        }
        // A button pressed just after the panel lit only keeps it awake, so
        // a power press meant to wake it does not turn it back off.
        if self.just_woke(now) {
            self.stir(now);
            self.button = Some((key, Press::Spent));
            return Vec::new();
        }
        self.stir(now);
        match key {
            Key::Back => {
                self.button = Some((key, Press::Spent));
                self.act(Action::Back, now)
            }
            Key::VolumeUp | Key::VolumeDown => {
                let down = key == Key::VolumeDown;
                self.button = Some((key, Press::Volume { down, next: now + REPEAT_DELAY_MS }));
                self.volume(down, now);
                Vec::new()
            }
            // A press before the page it lands on is drawn is lost, rather
            // than taken for one on nothing.
            Key::Power if self.keys && self.frame.is_none() => {
                self.button = Some((key, Press::Spent));
                Vec::new()
            }
            Key::Power => {
                match self.keys.then(|| self.lit()).flatten() {
                    // Held on a swipe-to-confirm control, the key slides
                    // its knob along as a finger would.
                    Some(Spot { stop: Stop::Hit(Action::Confirm(which)), rect, .. }) => {
                        self.button = Some((key, Press::Slide { which, track: rect, at: now }))
                    }
                    Some(spot) => self.button = Some((key, Press::Power { spot, at: now })),
                    None => {
                        self.button = Some((key, Press::Spent));
                        self.sleep();
                    }
                }
                self.dirty = true;
                Vec::new()
            }
        }
    }

    /// A button came back up: the power key works the control it went down
    /// on, or confirms with the knob it slid far enough.
    pub fn key_up(&mut self, key: Key, now: u64) -> Vec<Effect> {
        let Some((down, press)) = self.button else { return Vec::new() };
        if down != key {
            return Vec::new();
        }
        self.button = None;
        self.dirty = true;
        match press {
            Press::Power { spot, .. } => self.work(spot, now),
            Press::Slide { which, track, .. } => {
                let done = self.state.confirm > SLID;
                self.let_knob_go(which, track);
                if done {
                    return self.confirmed(which, now);
                }
                self.say("请按住电源键", now);
                Vec::new()
            }
            Press::Volume { .. } | Press::Spent => Vec::new(),
        }
    }

    /// Whatever the button held down was doing comes to nothing until it is
    /// let go: a knob it slid goes back.
    fn drop_button(&mut self) {
        let Some((key, press)) = self.button else { return };
        if let Press::Slide { which, track, .. } = press {
            self.let_knob_go(which, track);
        }
        self.button = Some((key, Press::Spent));
    }

    /// Let go of a swipe-to-confirm control's knob, which goes back to the
    /// start of its track.
    fn let_knob_go(&mut self, which: Confirm, track: Rect) {
        self.state.confirm = 0.0;
        self.knob = Some((which, track));
        self.dirty = true;
    }

    /// What a swipe-to-confirm control slid to its end does.
    fn confirmed(&mut self, which: Confirm, now: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        match which {
            Confirm::Reboot => fx.push(Effect::Reboot(TARGETS[self.state.reboot])),
            Confirm::Wipe => {
                let chosen: Vec<Wipe> = WIPES.iter().zip(self.state.wipe).filter(|(_, on)| *on).map(|(&w, _)| w).collect();
                match chosen.is_empty() {
                    true => self.say("请选择要清除的内容", now),
                    false => fx.push(Effect::Wipe(chosen)),
                }
            }
            Confirm::Install | Confirm::InstallWipe => {
                if let Install::Confirming { version, .. } = &self.state.install {
                    self.state.install = Install::Updating { version: version.clone(), stage: None, overwriting: false };
                    fx.push(Effect::Install);
                    self.refresh();
                }
            }
            Confirm::RunScript => {
                if self.state.install == Install::ConfirmingScript {
                    self.state.install = Install::Scripting { stage: None, running: false };
                    fx.push(Effect::Install);
                    self.refresh();
                }
            }
            Confirm::Backup => {
                let s = &self.state;
                let refused = if !s.backup.contains(&true) {
                    Some("请选择要备份的内容")
                } else if s.backup[2] && s.data != Data::Open {
                    Some("备份 data 分区需先完成「解密」")
                } else if s.place_open(s.backup_to) {
                    None
                } else {
                    Some(match (s.backup_to, &s.usb_drive) {
                        (Place::Storage, _) if s.data == Data::Erased => "data 分区已清除",
                        (Place::Storage, _) => "请先完成「解密」",
                        (Place::UsbDrive, Some(UsbDrive::Unreadable(_))) => "无法读取该 U 盘",
                        (Place::UsbDrive, _) => "请通过 OTG 连接 U 盘",
                    })
                };
                let backup = Effect::Backup { parts: s.backup, place: s.backup_to, compress: s.compress, verify: s.verify };
                match refused {
                    Some(why) => self.say(why, now),
                    None => fx.push(backup),
                }
            }
            Confirm::Restore => {
                let (entry, parts) = self.state.restore.as_ref().expect("the restore page shows a backup");
                let restore = Effect::Restore { place: entry.place, name: entry.name.clone(), parts: *parts };
                if !parts.contains(&true) {
                    self.say("请选择要恢复的内容", now);
                } else if parts[2] && self.state.data != Data::Open {
                    self.say("恢复 data 分区需先完成「解密」", now);
                } else {
                    fx.push(restore);
                }
            }
            Confirm::DeleteBackup => {
                let (entry, _) = self.state.restore.as_ref().expect("the restore page shows a backup");
                fx.push(Effect::DeleteBackup { place: entry.place, name: entry.name.clone() });
            }
            Confirm::Slot => match (&self.state.boot_slots, self.state.next_slot) {
                (Some(Ok(slots)), Some(pick)) if pick != slots.active => fx.push(Effect::SetSlot(pick)),
                (Some(Ok(slots)), _) => {
                    let said = format!("下次已从槽位 {} 启动", slots.active.letter());
                    self.say(&said, now);
                }
                _ => self.say("无法读取启动槽位", now),
            },
            Confirm::FactoryReset => fx.push(Effect::FactoryReset),
            Confirm::Merge => {
                if self.state.update == Standing::Waiting {
                    self.state.update = Standing::Merging(0.0);
                    self.refresh();
                    fx.push(Effect::Merge);
                }
            }
            Confirm::DeleteEntry => fx.push(Effect::Delete(self.entry_path())),
            Confirm::Flash => {
                if let Install::Choosing(choice) = &self.state.install {
                    let partition = choice.partitions[choice.partition];
                    fx.push(Effect::Flash { partition, pick: choice.pick });
                    self.state.install = Install::Flashing { partition, stage: None };
                    self.refresh();
                }
            }
        }
        fx
    }

    /// Work the control the keys are on, as touching it does. The
    /// brightness slider takes the volume keys until the power key lets it
    /// go, and a pattern's dots are joined one press at a time, a second
    /// press on the last one joined ending the pattern.
    fn work(&mut self, spot: Spot, now: u64) -> Vec<Effect> {
        match spot.stop {
            Stop::Back => self.act(Action::Back, now),
            Stop::Hit(Action::Brightness) => {
                self.adjusting = !self.adjusting;
                if self.adjusting {
                    self.say("按音量键调节亮度，按电源键完成", now);
                }
                Vec::new()
            }
            Stop::Hit(action) => self.act(action, now),
            Stop::Dot(i) if self.state.pattern.last() == Some(&i) => self.drawn(now),
            Stop::Dot(i) => {
                if self.state.pattern.is_empty() {
                    self.say("连接完成后，在最后一个点上再按一次电源键", now);
                }
                self.join(&self.theme(), i);
                Vec::new()
            }
        }
    }

    /// A pattern drawn to its end: four dots or more are sent to be
    /// checked, read as the number of each dot counted from one across the
    /// rows; fewer are wiped and asked for again.
    fn drawn(&mut self, now: u64) -> Vec<Effect> {
        let drawn = std::mem::take(&mut self.state.pattern);
        self.repaint_pattern(&self.theme());
        if drawn.len() < 4 {
            self.say("请至少连接 4 个点", now);
            return Vec::new();
        }
        self.unlock(drawn.iter().map(|i| (b'1' + *i as u8) as char).collect())
    }

    /// A volume key: the first lights the control the keys were last on
    /// if it shows, or else the first in view; each after moves the light
    /// down the page or up it, or turns the brightness up or down while
    /// its slider takes the keys.
    fn volume(&mut self, down: bool, now: u64) {
        if self.frame.is_none() {
            return;
        }
        if self.adjusting {
            if let Some(Spot { stop: Stop::Hit(Action::Brightness), rect, .. }) = self.lit() {
                let level = self.state.brightness + if down { -BRIGHTNESS_STEP } else { BRIGHTNESS_STEP };
                self.set_level(&self.theme(), rect, level);
            }
            return;
        }
        if self.keys {
            return self.step(down, now);
        }
        self.keys = true;
        self.dirty = true;
        let scroll = self.resting();
        let shows = self.lit().is_some_and(|spot| self.shows(&spot, scroll));
        if !shows {
            self.top_mut().2 = Focus::Unset;
            self.lit();
        }
    }

    /// Move the light to the next control down the page, or up it. A
    /// control far off is scrolled to half a screen at a time, the light
    /// staying on its control while that shows; past the last control the
    /// light goes round to the first, and past the first to the last.
    fn step(&mut self, down: bool, now: u64) {
        let scroll = self.resting();
        let current = self.lit();
        if self.spots().is_none() {
            return;
        }
        let spots = self.spots.clone().expect("made above");
        let cur = current.and_then(|c| focus::find(&spots, &c));
        let (top, bottom) = self.band(scroll);
        let (max, n) = (self.max_scroll(), spots.len());
        let page = |k: usize| spots[k].group == Group::Page;
        let held = |k: usize| matches!(spots[k].group, Group::Fixed | Group::Strip);
        let docked = |k: usize| spots[k].group == Group::Dock;
        // The page's own controls not wholly above the band, or below it.
        let from_top = |k: usize| page(k) && self.place(&spots[k], scroll).y + spots[k].rect.h > top;
        let from_bottom = |k: usize| page(k) && self.place(&spots[k], scroll).y < bottom;
        enum To {
            Spot(usize),
            Scroll,
            Round,
        }
        let to = if down {
            // Past the page's own: the rest of the page, then the bottom
            // strip, then round.
            let past = || {
                if scroll < max {
                    To::Scroll
                } else {
                    (0..n).find(|&k| docked(k)).map_or(To::Round, To::Spot)
                }
            };
            match cur {
                Some(i) if page(i) => (i + 1..n).find(|&k| page(k)).map_or_else(past, To::Spot),
                Some(i) if docked(i) => (i + 1..n).find(|&k| docked(k)).map_or(if scroll < max { To::Scroll } else { To::Round }, To::Spot),
                Some(i) => (i + 1..n).find(|&k| held(k)).or_else(|| (0..n).find(|&k| from_top(k))).map_or_else(past, To::Spot),
                None => (0..n).find(|&k| from_top(k)).map_or_else(past, To::Spot),
            }
        } else {
            // Before the page's own: the rest of the page, then the `<` and
            // the strip at the top, then round.
            let before = || {
                if scroll > 0.0 {
                    To::Scroll
                } else {
                    (0..n).rev().find(|&k| held(k)).map_or(To::Round, To::Spot)
                }
            };
            match cur {
                Some(i) if page(i) => (0..i).rev().find(|&k| page(k)).map_or_else(before, To::Spot),
                Some(i) if docked(i) => (0..i).rev().find(|&k| docked(k)).or_else(|| (0..n).rev().find(|&k| from_bottom(k))).map_or_else(before, To::Spot),
                Some(i) => (0..i).rev().find(|&k| held(k)).map_or(if scroll > 0.0 { To::Scroll } else { To::Round }, To::Spot),
                None => (0..n).rev().find(|&k| from_bottom(k)).map_or_else(before, To::Spot),
            }
        };
        match to {
            To::Spot(k) => {
                let s = spots[k];
                if s.group == Group::Page {
                    let target = self.reveal(&s, scroll);
                    if (target - scroll).abs() > (bottom - top) / 2.0 {
                        return self.scroll_on(down, &spots, cur, now);
                    }
                    self.seek_to(target, now);
                }
                self.top_mut().2 = Focus::On(s);
            }
            To::Scroll => return self.scroll_on(down, &spots, cur, now),
            To::Round if n == 0 => return,
            // Going round, the page goes to its top or its end, as far as
            // its control there shows.
            To::Round => {
                let s = spots[if down { 0 } else { n - 1 }];
                let end = if down { 0.0 } else { max };
                let target = if self.shows(&s, end) { end } else { self.reveal(&s, end) };
                self.seek_to(target, now);
                self.top_mut().2 = Focus::On(s);
            }
        }
        self.dirty = true;
    }

    /// Scroll the page half a screen down or up for the keys. The light
    /// stays on its control while that shows, and goes to the first in view
    /// the way the page went when it does not.
    fn scroll_on(&mut self, down: bool, spots: &[Spot], cur: Option<usize>, now: u64) {
        let scroll = self.resting();
        let (top, bottom) = self.band(scroll);
        let half = (bottom - top) / 2.0;
        let target = if down { (scroll + half).min(self.max_scroll()) } else { (scroll - half).max(0.0) };
        self.seek_to(target, now);
        if cur.is_some_and(|i| self.shows(&spots[i], target)) {
            return;
        }
        let band = self.band(target);
        let in_view = |k: &usize| spots[*k].group == Group::Page && within(self.place(&spots[*k], target), band);
        let next = if down { (0..spots.len()).find(in_view) } else { (0..spots.len()).rev().find(in_view) };
        self.top_mut().2 = next.map_or(Focus::Off, |k| Focus::On(spots[k]));
    }

    /// The page's controls in the order the keys go through them, once the
    /// page is drawn.
    fn spots(&mut self) -> Option<&[Spot]> {
        if self.spots.is_none() {
            let t = self.theme();
            self.spots = Some(focus::spots(&t, self.frame.as_ref()?));
        }
        self.spots.as_deref()
    }

    /// The control the keys are on, once the page is drawn: the one they
    /// were on, found again, or the first in view when they have not been
    /// anywhere on the page yet or theirs has gone from it.
    fn lit(&mut self) -> Option<Spot> {
        let scroll = self.resting();
        self.spots()?;
        let spots = self.spots.as_deref().expect("made above");
        let found = match self.top().2 {
            Focus::Off => return None,
            Focus::On(at) => focus::find(spots, &at),
            Focus::Unset => None,
        };
        let spot = spots[found.or_else(|| self.first_in_view(spots, scroll))?];
        self.stack.last_mut().expect("the page stack always holds a page").2 = Focus::On(spot);
        Some(spot)
    }

    /// The first of the page's own controls in view at `scroll`, or else
    /// the bottom strip's first, or else the `<` or the top strip's first.
    fn first_in_view(&self, spots: &[Spot], scroll: f32) -> Option<usize> {
        let band = self.band(scroll);
        let k = spots.iter().position(|s| s.group == Group::Page && within(self.place(s, scroll), band));
        k.or_else(|| spots.iter().position(|s| s.group == Group::Dock))
            .or_else(|| spots.iter().position(|s| matches!(s.group, Group::Fixed | Group::Strip)))
    }

    /// Where on screen `s` is with the page `offset` pixels down.
    fn place(&self, s: &Spot, offset: f32) -> Rect {
        let fr = self.frame.as_ref().expect("a control is placed on the page drawn");
        let dy = match s.group {
            Group::Fixed => 0.0,
            Group::Strip => pinned_top(&self.theme(), fr.pinned.as_ref().expect("the strip holds its controls"), offset),
            Group::Page => -offset,
            Group::Dock => self.h as f32 - fr.dock.as_ref().expect("the dock holds its controls").canvas.h as f32,
        };
        Rect { y: s.rect.y + dy, ..s.rect }
    }

    /// The band of the screen the page's own controls show in with the page
    /// at `scroll`: under the bar, or under the strip held there, down to
    /// the strip held at the bottom.
    fn band(&self, scroll: f32) -> (f32, f32) {
        let t = self.theme();
        let fr = self.frame.as_ref().expect("the band is the page drawn's");
        let top = match &fr.pinned {
            Some(p) => pinned_top(&t, p, scroll) + p.canvas.h as f32,
            None => widgets::bar_bottom(&t),
        };
        (top, self.h as f32 - fr.dock.as_ref().map_or(0.0, |d| d.canvas.h as f32))
    }

    /// Whether `s` shows, in part at least, with the page at `scroll`.
    fn shows(&self, s: &Spot, scroll: f32) -> bool {
        s.group != Group::Page || within(self.place(s, scroll), self.band(scroll))
    }

    /// How far down to scroll the page from `scroll` to show `s` whole, a
    /// margin clear of the band's edges, or as much of it from its top as
    /// fits.
    fn reveal(&self, s: &Spot, scroll: f32) -> f32 {
        let margin = self.theme().m.margin;
        let (top, bottom) = self.band(scroll);
        let (top, bottom) = (top + margin, bottom - margin);
        let r = self.place(s, scroll);
        let d = if r.y < top {
            r.y - top
        } else if r.y + r.h > bottom {
            (r.y + r.h - bottom).min(r.y - top)
        } else {
            0.0
        };
        (scroll + d).clamp(0.0, self.max_scroll())
    }

    /// Where the page comes to rest: where the keys are scrolling it to, or
    /// where it is.
    fn resting(&self) -> f32 {
        self.seek.map_or(self.top().1, |(target, _)| target)
    }

    /// Scroll the page to `target` for the keys, easing there.
    fn seek_to(&mut self, target: f32, now: u64) {
        let at = self.top().1;
        self.fling = None;
        self.seek = (target != at).then(|| (target, Spring::new(now, at - target, 0.0, SEEK_PERIOD)));
        self.dirty = true;
    }

    /// The light on the control the keys are on while they work the page:
    /// the part of the screen its control is drawn in, where the light
    /// falls with the page `offset` pixels down, how round it is, and its
    /// tint, stronger while the power key is held down on it.
    fn light(&mut self, offset: f32) -> Option<(Group, Rect, f32, Rgba)> {
        if !self.keys {
            return None;
        }
        let spot = self.lit()?;
        let t = self.theme();
        let keypad = self.screen() == Screen::Decrypt && matches!(self.state.data, Data::Locked { keys: Keys::Digits, .. });
        let (r, radius) = focus::lit(&t, spot.stop, self.place(&spot, offset), keypad);
        let held = matches!(self.button, Some((_, Press::Power { .. } | Press::Slide { .. })));
        let alpha = if held || self.adjusting { LIGHT_HELD_ALPHA } else { LIGHT_ALPHA };
        Some((spot.group, r, radius, t.pal.accent.with_alpha(alpha)))
    }

    /// Whether the page is gliding, springing back, or easing to where the
    /// keys moved at `now`.
    fn in_motion(&self, now: u64) -> bool {
        self.fling.is_some_and(|fl| !fl.at(now).1) || self.bounce.is_some_and(|b| !b.at(now).1) || self.seek.is_some_and(|(_, s)| !s.at(now).1)
    }

    /// Stop a page gliding, springing back or easing to where the keys
    /// moved, where it has got to, for a finger to hold: one past its end
    /// stays out there as the finger's pull. Whether there was one.
    fn catch(&mut self, now: u64) -> bool {
        let glide = self.fling.take().is_some_and(|fl| !fl.at(now).1);
        let seeking = self.seek.take().is_some_and(|(_, s)| !s.at(now).1);
        let spring = self.bounce.take().map(|b| b.at(now)).filter(|&(_, done)| !done);
        if let Some((offset, _)) = spring {
            self.pull = motion::unrubber(offset, self.h as f32);
        }
        glide || seeking || spring.is_some()
    }

    /// Move the page `d` pixels under a dragging finger (positive moves it
    /// up). Past an end the finger's travel goes into the pull, which the
    /// page follows with growing resistance; coming back undoes the pull
    /// before the page scrolls again.
    fn drag_by(&mut self, d: f32) {
        self.dirty = true;
        let mut d = d;
        if self.pull != 0.0 {
            let pull = self.pull + d;
            if pull * self.pull > 0.0 {
                self.pull = pull;
                return;
            }
            self.pull = 0.0;
            d = pull;
        }
        let max = self.max_scroll();
        let top = self.top_mut();
        let next = top.1 + d;
        top.1 = next.clamp(0.0, max);
        self.pull = next - top.1;
    }

    fn on_knob(&self, track: Rect, x: f32) -> bool {
        let knob_x = track.x + (track.w - track.h) * self.state.confirm;
        x >= knob_x && x <= knob_x + track.h
    }

    fn set_brightness(&mut self, t: &Theme, track: Rect, x: f32) {
        self.set_level(t, track, (x - track.x) / track.w);
    }

    /// Set the brightness to `level` of the slider's top, no lower than
    /// its floor, and draw the slider at `track` on the page again.
    fn set_level(&mut self, t: &Theme, track: Rect, level: f32) {
        self.state.brightness = level.clamp(MIN_BRIGHTNESS, 1.0);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.card);
            widgets::slider(&mut fr.canvas, t, track.x, track.y, track.w, self.state.brightness);
        }
        self.dirty = true;
    }

    fn repaint_confirm(&mut self, t: &Theme, f: &Fonts, which: Confirm, track: Rect) {
        self.paint_knob(t, f, which, track);
        self.dirty = true;
    }

    /// Draw the swipe-to-confirm control at `track` on the page again, its
    /// knob where it has come to.
    fn paint_knob(&mut self, t: &Theme, f: &Fonts, which: Confirm, track: Rect) {
        let (label, danger) = pages::confirm_label(which);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.bg);
            widgets::confirm(&mut fr.canvas, t, f, track.y, label, danger, self.state.confirm);
        }
    }

    /// Add the dot under the finger to the pattern being drawn.
    fn trace(&mut self, t: &Theme, x: f32, y: f32) {
        let Some((cx, cy, span)) = self.frame.as_ref().and_then(|fr| fr.pattern) else { return };
        let reach = t.m.dp(widgets::PATTERN_REACH_DP);
        let dots = widgets::pattern_dots(cx, cy, span);
        if let Some(i) = dots.iter().position(|&(dx, dy)| (dx - x).hypot(dy - y) < reach) {
            self.join(t, i);
        }
    }

    /// Join dot `i` to the pattern being drawn, after the dot it passes
    /// straight over on its way from the last one, as the system's pattern
    /// lock joins that one too. A dot joined once is not joined again.
    fn join(&mut self, t: &Theme, i: usize) {
        let drawn = &mut self.state.pattern;
        if drawn.contains(&i) {
            return;
        }
        if let Some(&last) = drawn.last() {
            let (row, col) = ((last / 3) as i32, (last % 3) as i32);
            let (rows, cols) = ((i / 3) as i32 - row, (i % 3) as i32 - col);
            // Two rows or columns away, and not a knight's move.
            let row = if rows.abs() == 2 && cols.abs() != 1 { row + rows.signum() } else { row };
            let col = if cols.abs() == 2 && rows.abs() != 1 { col + cols.signum() } else { col };
            let over = (row * 3 + col) as usize;
            if !drawn.contains(&over) {
                drawn.push(over);
            }
        }
        drawn.push(i);
        self.repaint_pattern(t);
    }

    fn repaint_pattern(&mut self, t: &Theme) {
        let Some(fr) = self.frame.as_mut() else { return };
        let Some((cx, cy, span)) = fr.pattern else { return };
        let r = span / 2.0 + t.m.dp(16.0);
        fr.canvas.fill_rect(cx - r, cy - r, r * 2.0, r * 2.0, t.pal.bg);
        widgets::pattern_lock(&mut fr.canvas, t, cx, cy, span, &self.state.pattern);
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt;
    use skyward_info::{Boot, Charge, Cpu, Storage};
    use skyward_install::{Failure, Refusal};
    use skyward_vab::SnapshotState;

    // A Xiaomi 13 Ultra panel: 1440 × 3200 at 560 dpi.
    const W: u32 = 1440;
    const H: u32 = 3200;
    const PPD: f32 = 3.5;

    fn fonts() -> Fonts {
        crate::font::bundled()
    }

    fn info() -> DeviceInfo {
        DeviceInfo {
            model: "Xiaomi 13 Ultra".into(),
            codename: "ishtar".into(),
            soc: None,
            cpu: Cpu { cores: 8, max_khz: Some(3_187_200) },
            ram_kib: 15_556_972,
            panel: Some((W, H)),
            screen: Some(6.73),
            os_version: None,
            android: None,
            security_patch: None,
            kernel: "5.15.178".into(),
            storage: Storage { total: 496_550_785_024, free: None },
            battery: None,
            slot: None,
            snapshot: SnapshotState::None,
            boot: Boot { unlocked: None, verified: None },
        }
    }

    /// Where on screen the first control doing `action` sits.
    /// Where on screen the control doing `action` is: among the page's
    /// controls, on its pinned strip, or a row of its long list.
    fn spot(app: &mut App, f: &Fonts, i: &DeviceInfo, action: Action) -> (f32, f32) {
        let (t, scroll) = (app.theme(), app.top().1);
        let fr = app.frame(f, i);
        if let Some(h) = fr.hits.iter().find(|h| h.action == action) {
            let (x, y) = h.rect.centre();
            return (x, y - scroll);
        }
        if let Some(p) = &fr.pinned {
            if let Some(h) = p.hits.iter().find(|h| h.action == action) {
                let (x, y) = h.rect.centre();
                return (x, y + pinned_top(&t, p, scroll));
            }
        }
        if let Some(d) = &fr.dock {
            if let Some(h) = d.hits.iter().find(|h| h.action == action) {
                let (x, y) = h.rect.centre();
                return (x, y + H as f32 - d.canvas.h as f32);
            }
        }
        let rows = fr.rows.as_ref().expect("control on the page");
        let answer = rows.action.expect("control on the page");
        let k = (0..rows.count()).find(|&k| answer(k) == action).expect("control on the page");
        let (x, y) = rows.rect(k, fr.canvas.w as f32).centre();
        (x, y - scroll)
    }

    fn tap(app: &mut App, f: &Fonts, i: &DeviceInfo, action: Action) -> Vec<Effect> {
        tap_at(app, f, i, action, 0)
    }

    fn tap_at(app: &mut App, f: &Fonts, i: &DeviceInfo, action: Action, now: u64) -> Vec<Effect> {
        let (x, y) = spot(app, f, i, action);
        let mut fx = app.touch(Touch::Down(x, y), f, now);
        fx.extend(app.touch(Touch::Up, f, now));
        fx
    }

    /// Tap the `<`, which stays at the top of the screen.
    fn tap_back(app: &mut App, f: &Fonts, i: &DeviceInfo, now: u64) {
        app.frame(f, i);
        let (x, y) = widgets::back_rect(&app.theme()).centre();
        app.touch(Touch::Down(x, y), f, now);
        app.touch(Touch::Up, f, now);
    }

    #[test]
    fn rows_open_pages_and_back_returns() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Settings));
        assert_eq!(app.screen(), Screen::Settings);
        tap_back(&mut app, &f, &i, 0);
        assert_eq!(app.screen(), Screen::Home);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home, "back on the first page stays there");
    }

    #[test]
    fn the_back_control_stays_put_while_the_page_scrolls() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.frame(&f, &i);
        app.touch(Touch::Down(700.0, 2400.0), &f, 0);
        app.touch(Touch::Move(700.0, 2200.0), &f, 0);
        app.touch(Touch::Move(700.0, 2100.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.top().1, 300.0);
        tap_back(&mut app, &f, &i, 0);
        assert_eq!(app.screen(), Screen::Home);
    }

    /// `screen` opened from the home page, so there is a page to go back
    /// to.
    fn opened(screen: Screen, f: &Fonts, i: &DeviceInfo) -> App {
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, f, i, Action::Open(screen));
        app
    }

    #[test]
    fn home_opens_the_mount_page() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Mount)), vec![Effect::ReadMounts], "the page shows the partitions as they are now");
        assert_eq!(app.screen(), Screen::Mount);
    }

    #[test]
    fn a_home_page_grown_by_an_update_notice_scrolls_down_to_its_last_row() {
        let f = fonts();
        let plain = info();
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert_eq!(app.frame(&f, &plain).canvas.h, H, "without the notice the page fits the screen");
        let i = DeviceInfo { snapshot: SnapshotState::Snapshotted, ..info() };
        let mut app = App::new(Screen::Home, W, H, PPD);
        let fr = app.frame(&f, &i);
        let reboot = fr.hits.iter().find(|h| h.action == Action::Open(Screen::Reboot)).expect("a reboot row").rect;
        assert!(reboot.y + reboot.h > H as f32, "the notice pushes the last row below the screen");
        assert!(reboot.y + reboot.h <= fr.canvas.h as f32, "the page is drawn down to it");
        app.touch(Touch::Down(700.0, 2800.0), &f, 0);
        app.touch(Touch::Move(700.0, 2000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1200.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        for ms in (0..3000).step_by(8) {
            app.tick(ms);
        }
        let (_, y) = spot(&mut app, &f, &i, Action::Open(Screen::Reboot));
        assert!(y < H as f32, "scrolled up into view");
        tap_at(&mut app, &f, &i, Action::Open(Screen::Reboot), 3000);
        assert_eq!(app.screen(), Screen::Reboot);
    }

    #[test]
    fn the_cursor_blinks_and_comes_up_whole_at_each_edit() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Search, &f, &i);
        let mut screen = Canvas::new(W, H);
        let mut shows = |app: &mut App, now: u64| {
            app.compose(&mut screen, &f, &i, "12:00", now);
            app.caret_shown
        };
        assert!(shows(&mut app, 1000), "it comes up whole on the page");
        assert_eq!(app.deadline(1200), Some(1500), "the loop wakes when it turns");
        assert!(!shows(&mut app, 1500), "half a second on it hides");
        assert!(shows(&mut app, 2000));
        assert!(!shows(&mut app, 2600));
        tap(&mut app, &f, &i, Action::Type(Cap::Char('b')));
        assert!(shows(&mut app, 2700), "an edit brings it back whole at once");
        assert!(!shows(&mut app, 3200));
        tap(&mut app, &f, &i, Action::CloseSearch);
        assert_eq!(app.screen(), Screen::Home, "the round button leaves the search");
        assert!(!shows(&mut app, 3300), "no cursor off the page");
        assert!(app.deadline(3300).is_none_or(|d| d > 10_000), "nor a wake for it, only for the screen going off");
    }

    #[test]
    fn the_search_bar_moves_up_into_the_search_page_and_back_down() {
        let (f, i) = (fonts(), info());
        let t = Theme::light(W, H, PPD);
        let m = t.m;
        let mut app = App::new(Screen::Home, W, H, PPD);
        let mut screen = Canvas::new(W, H);
        // The bar's top along a column through it, clear of the title and
        // the placeholder: the first row in the field's grey.
        let x = m.dp(300.0) as u32;
        let top = |screen: &Canvas| {
            (m.dp(40.0) as u32..H).find(|&y| screen.data[((y * W + x) * 4) as usize] == t.pal.search.r).map(|y| y as f32)
        };
        let grey = |screen: &Canvas, y: f32| screen.data[((y as u32 * W + W / 2) * 4) as usize];
        let (home, field) = (m.content_top, widgets::search_layout(&t).0.y);
        // The device card under the home page's bar.
        let card = m.content_top + m.search_h + m.after_search + m.dp(20.0);
        app.compose(&mut screen, &f, &i, "12:00", 0);
        assert_eq!(grey(&screen, card), t.pal.card.r);

        tap_at(&mut app, &f, &i, Action::Open(Screen::Search), 1000);
        assert_eq!(app.screen(), Screen::Search);
        assert_eq!(app.deadline(1000), Some(0), "it moves frame by frame");
        app.compose(&mut screen, &f, &i, "12:00", 1067);
        let along = crate::motion::settle(67, MORPH_PERIOD);
        let expected = home + (field - home) * along;
        let at = top(&screen).expect("the bar is on screen");
        assert!((at - expected).abs() <= 2.0, "up {along} of the way after 67 ms: at {at}, not {expected}");
        assert_eq!(grey(&screen, card), t.pal.bg.r, "the home page's cards go at once");
        app.compose(&mut screen, &f, &i, "12:00", 2500);
        assert!(app.morph.is_none(), "settled, the search page shows as itself");
        assert_eq!(top(&screen), Some(field.ceil()));

        tap_at(&mut app, &f, &i, Action::CloseSearch, 3000);
        assert_eq!(app.screen(), Screen::Home);
        app.compose(&mut screen, &f, &i, "12:00", 3067);
        let expected = field + (home - field) * along;
        let at = top(&screen).expect("the bar is on screen");
        assert!((at - expected).abs() <= 2.0, "down {along} of the way after 67 ms: at {at}, not {expected}");
        assert_eq!(grey(&screen, card + (at - home)), t.pal.card.r, "the cards come down under the bar");
        app.compose(&mut screen, &f, &i, "12:00", 4500);
        assert!(app.morph.is_none());
        assert_eq!(top(&screen), Some(home.ceil()));
    }

    #[test]
    fn an_unreadable_partition_table_leaves_nothing_to_toggle() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Mount, &f, &i);
        app.set_mounts(Err("read the metadata in super: Input/output error (os error 5)".into()));
        assert!(app.frame(&f, &i).hits.is_empty());
    }

    #[test]
    fn partitions_mount_while_an_update_waits_to_merge() {
        let (f, mut i) = (fonts(), info());
        i.snapshot = SnapshotState::Snapshotted;
        let mut app = opened(Screen::Mount, &f, &i);
        app.set_mounts(Ok(vec![
            MountRow { base: "vendor".into(), point: "/vendor".into(), mounted: false, holders: Vec::new() },
            MountRow { base: "odm".into(), point: "/odm".into(), mounted: true, holders: Vec::new() },
        ]));
        let frame = app.frame(&f, &i);
        assert_eq!(frame.hits.iter().map(|h| h.action).collect::<Vec<_>>(), [Action::MountToggle(0), Action::MountToggle(1)]);
        assert_eq!(tap(&mut app, &f, &i, Action::MountToggle(0)), vec![Effect::Mount("vendor".into())], "through its snapshot");
    }

    #[test]
    fn an_update_waiting_in_the_slot_merges_with_a_swipe_and_shows_how_far_it_has_come() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        app.set_update(Standing::Waiting);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Update)), vec![Effect::ReadUpdate], "the page reads where the update stands");
        assert_eq!(app.screen(), Screen::Update);
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Merge), vec![Effect::Merge]);
        assert_eq!(app.state.update, Standing::Merging(0.0), "the page turns to the merge at once");
        assert!(app.frame(&f, &i).hits.is_empty(), "nothing to swipe while it merges");
        app.set_update(Standing::Merging(0.5));
        assert!(app.frame(&f, &i).hits.is_empty());
        app.set_update(Standing::Merged);
        assert!(app.frame(&f, &i).hits.is_empty(), "merged, nothing is left to do here");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Advanced);
        app.set_update(Standing::Installed(Slot::A));
        tap(&mut app, &f, &i, Action::Open(Screen::Update));
        assert!(app.frame(&f, &i).hits.is_empty(), "an update in the other slot merges once that slot has started");
    }

    #[test]
    fn the_home_pages_word_on_a_waiting_update_opens_its_page() {
        let (f, mut i) = (fonts(), info());
        i.snapshot = SnapshotState::Snapshotted;
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Update)), vec![Effect::ReadUpdate]);
        assert_eq!(app.screen(), Screen::Update);
    }

    #[test]
    fn the_mount_page_toggles_each_partition() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Mount, &f, &i);
        app.set_mounts(Ok(vec![
            MountRow { base: "system".into(), point: "/mnt/system".into(), mounted: true, holders: Vec::new() },
            MountRow { base: "vendor".into(), point: "/vendor".into(), mounted: false, holders: Vec::new() },
        ]));
        let frame = app.frame(&f, &i);
        assert!(frame.hits.iter().any(|h| h.action == Action::MountToggle(0)), "the first partition has a toggle");
        assert!(frame.hits.iter().any(|h| h.action == Action::MountToggle(1)), "the second partition has a toggle");
        assert!(!frame.hits.iter().any(|h| matches!(h.action, Action::Toggle(_))), "partitions mount read-only");
        // A mounted partition unmounts, an unmounted one mounts.
        assert_eq!(app.act(Action::MountToggle(0), 0), vec![Effect::Unmount("system".into())]);
        assert_eq!(app.act(Action::MountToggle(1), 0), vec![Effect::Mount("vendor".into())]);
    }

    #[test]
    fn a_partition_in_use_cannot_be_unmounted() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Mount, &f, &i);
        let held = vec!["touch_report".to_string(), "touchsensor".to_string()];
        app.set_mounts(Ok(vec![
            MountRow { base: "odm".into(), point: "/odm".into(), mounted: true, holders: held },
            MountRow { base: "vendor".into(), point: "/vendor".into(), mounted: true, holders: Vec::new() },
        ]));
        let frame = app.frame(&f, &i);
        assert!(matches!(frame.hits[0].action, Action::Unavailable(_)), "the programs using odm keep it");
        assert_eq!(frame.hits[1].action, Action::MountToggle(1));
        let busy = frame.hits[0].action;
        assert!(tap(&mut app, &f, &i, busy).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says why");
    }

    #[test]
    fn a_failed_mount_stays_on_the_page_until_the_next_try() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Mount, &f, &i);
        app.set_mounts(Ok(vec![MountRow { base: "vendor".into(), point: "/vendor".into(), mounted: false, holders: Vec::new() }]));
        let failure = MountFailure { mounting: true, base: "vendor".into(), detail: "mount vendor_b: no filesystem".into() };
        app.mount_failed(failure.clone());
        assert_eq!(app.state.mount_failure, Some(failure.clone()));
        assert_eq!(app.act(Action::MountToggle(0), 0), vec![Effect::Mount("vendor".into())]);
        assert_eq!(app.state.mount_failure, None, "trying again clears it");
        app.mount_failed(failure);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home);
        assert_eq!(app.state.mount_failure, None, "leaving the page clears it");
    }

    #[test]
    fn the_mount_page_notes_when_there_is_nothing_to_mount() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Mount, &f, &i);
        let frame = app.frame(&f, &i);
        assert!(frame.hits.is_empty(), "no partition rows, and nothing else to touch");
    }

    #[test]
    fn swipe_from_the_left_edge_goes_back() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(90.0, 1605.0), &f, 0);
        app.touch(Touch::Move(320.0, 1610.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn a_short_edge_swipe_does_not_go_back() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.screen(), Screen::Reboot);
    }

    #[test]
    fn the_first_page_has_no_edge_swipe_back() {
        let f = fonts();
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 8);
        assert!(app.touch(Touch::Move(400.0, 1600.0), &f, 16).is_empty(), "no tap of the motor");
        assert!(!matches!(app.drag, Drag::Edge { .. }), "no cue");
        app.touch(Touch::Up, &f, 24);
        assert_eq!(app.screen(), Screen::Home);
        assert_eq!(app.deadline(24), app.idle_due(), "nothing left on screen to settle");
    }

    #[test]
    fn the_back_cue_taps_once_as_it_arms_and_sinks_when_let_go() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        let ready = app.theme().m.dp(BACK_SWIPE_DP);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        assert!(app.touch(Touch::Move(80.0, 1600.0), &f, 8).is_empty());
        assert_eq!(app.touch(Touch::Move(20.0 + ready, 1600.0), &f, 16), vec![Effect::Haptic]);
        assert!(app.touch(Touch::Move(60.0 + ready, 1600.0), &f, 24).is_empty(), "one tap while armed");
        app.touch(Touch::Move(40.0, 1600.0), &f, 32);
        app.touch(Touch::Up, &f, 40);
        assert_eq!(app.screen(), Screen::Reboot, "pulled back in, it does not go back");
        assert_eq!(app.deadline(40), Some(0), "the cue is sinking back");
        app.tick(1000);
        assert_eq!(app.deadline(1000), app.idle_due());
    }

    #[test]
    fn the_back_cue_stays_quiet_with_touch_vibration_off() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.state.vibrate = false;
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 8);
        assert!(app.touch(Touch::Move(400.0, 1600.0), &f, 16).is_empty());
    }

    #[test]
    fn the_back_cue_keeps_to_the_height_the_swipe_began_at() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        let at = 1600.0 - app.theme().m.dp(CUE_LIFT_DP);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1610.0), &f, 8);
        app.touch(Touch::Move(200.0, 1300.0), &f, 16);
        app.touch(Touch::Move(150.0, 1900.0), &f, 24);
        let Drag::Edge { cy, .. } = app.drag else { panic!("still swiping in from the edge") };
        assert_eq!(cy, at, "the finger wandered up and down");
        app.touch(Touch::Move(40.0, 2100.0), &f, 32);
        app.touch(Touch::Up, &f, 40);
        assert_eq!(app.cue.as_ref().map(|c| c.1), Some(at), "it sinks back where it was");
    }

    #[test]
    fn pulling_past_the_top_resists_and_springs_back() {
        let f = fonts();
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        app.touch(Touch::Down(700.0, 1000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1300.0), &f, 100);
        app.touch(Touch::Move(700.0, 1800.0), &f, 600);
        let shown = app.overscroll(600);
        assert!(shown < 0.0 && shown > -700.0, "the page follows less than the finger: {shown}");
        assert_eq!(app.top().1, 0.0);
        // Dragging back first undoes the pull.
        app.touch(Touch::Move(700.0, 1400.0), &f, 1000);
        assert!(app.overscroll(1000) > shown);
        app.touch(Touch::Up, &f, 1400);
        assert!(app.overscroll(1400) < 0.0);
        assert_eq!(app.deadline(1400), Some(0));
        app.tick(1500);
        assert!(app.overscroll(1500) > app.overscroll(1400), "on its way back");
        app.tick(3000);
        assert_eq!(app.overscroll(3000), 0.0);
        assert_eq!(app.deadline(3000), app.idle_due());
    }

    #[test]
    fn a_page_pulled_past_its_end_can_be_caught() {
        let f = fonts();
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        app.touch(Touch::Down(700.0, 1000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1500.0), &f, 500);
        app.touch(Touch::Up, &f, 1000);
        app.tick(1050);
        let held = app.overscroll(1050);
        app.touch(Touch::Down(700.0, 1200.0), &f, 1050);
        app.tick(1500);
        assert!((app.overscroll(1500) - held).abs() < 1.0, "the finger holds it where it was");
        app.touch(Touch::Up, &f, 1600);
        app.tick(4000);
        assert_eq!(app.overscroll(4000), 0.0);
    }

    #[test]
    fn scrolling_back_down_past_the_top_pulls_from_the_top() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i);
        app.touch(Touch::Down(700.0, 2000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1700.0), &f, 100);
        assert_eq!(app.top().1, 300.0);
        app.touch(Touch::Move(700.0, 2100.0), &f, 500);
        assert_eq!(app.top().1, 0.0);
        assert!(app.overscroll(500) < 0.0, "the last 100 px pull the page down");
    }

    #[test]
    fn dragging_scrolls_and_does_not_tap() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let (x, y) = spot(&mut app, &f, &i, Action::Toggle(Switch::Vibrate));
        assert!(y < H as f32, "the switch is on the first screen");
        app.touch(Touch::Down(x, y), &f, 0);
        app.touch(Touch::Move(x, y - 100.0), &f, 0);
        app.touch(Touch::Move(x, y - 300.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert!(app.state.vibrate, "the switch was not flipped");
        assert_eq!(app.top().1, 300.0);
    }

    #[test]
    fn the_file_transfer_switch_has_the_runtime_follow_it() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        assert!(app.state.file_transfer, "on until turned off");
        let (_, y) = spot(&mut app, &f, &i, Action::Toggle(Switch::FileTransfer));
        if y > H as f32 * 0.9 {
            scroll_to(&mut app, &f, y - H as f32 * 0.8, 0);
        }
        assert_eq!(tap_at(&mut app, &f, &i, Action::Toggle(Switch::FileTransfer), 2000), [Effect::FileTransfer(false)]);
        assert!(!app.state.file_transfer);
        assert_eq!(tap_at(&mut app, &f, &i, Action::Toggle(Switch::FileTransfer), 3000), [Effect::FileTransfer(true)]);
        app.file_transfer_failed(false, "无法开启文件传输", 4000);
        assert!(!app.state.file_transfer, "the switch shows how it stands");
    }

    /// Flick the page up from `start`: the finger travels `step` px every
    /// 8 ms five times and lifts 8 ms after its last move. Returns when it
    /// lifted.
    fn flick(app: &mut App, f: &Fonts, start: u64, step: f32) -> u64 {
        app.touch(Touch::Down(700.0, 2400.0), f, start);
        for k in 1..=5u64 {
            app.touch(Touch::Move(700.0, 2400.0 - step * k as f32), f, start + 8 * k);
        }
        let lift = start + 48;
        app.touch(Touch::Up, f, lift);
        lift
    }

    #[test]
    fn a_flick_glides_on_and_comes_to_rest() {
        let f = fonts();
        // Every reading there is, for a page long enough to glide on.
        let battery = skyward_info::Battery {
            level: 80,
            charge: Charge::Discharging,
            temp: Some(300),
            health: Some("Good".into()),
            cycles: Some(169),
            design: Some(5_500_000),
            full: Some(5_095_000),
            voltage: Some(4_200_000),
        };
        let i = DeviceInfo {
            os_version: Some("OS3.0.305.0.WMACNXM".into()),
            android: Some("16".into()),
            battery: Some(battery),
            ..info()
        };
        let mut app = App::new(Screen::Specs, W, H, PPD);
        app.frame(&f, &i);
        let lift = flick(&mut app, &f, 1000, 20.0);
        let lifted = app.top().1;
        assert_eq!(lifted, 100.0);
        assert_eq!(app.deadline(lift), Some(0), "the page moves on after the lift");
        app.tick(lift + 50);
        assert!(app.top().1 > lifted);
        for ms in (100..5000).step_by(8) {
            app.tick(lift + ms);
        }
        assert_eq!(app.deadline(lift + 5000), app.idle_due(), "the glide comes to rest");
        let rest = app.top().1;
        assert!(rest > lifted + 300.0 && rest < app.max_scroll(), "{rest}");
    }

    #[test]
    fn a_page_drawn_again_as_work_goes_on_keeps_its_scroll() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.backup_started(false, Place::Storage, true);
        app.backup_plan((0..20).map(|k| (format!("part{k}"), 1_000_000)).collect());
        let at = |done| BackupAt { item: 0, done, of: 1_000_000, checking: false };
        app.backup_at(at(0));
        app.frame(&f, &i);
        // A finger drags the page up while the work tells how far it has
        // come, each time drawing the page again.
        app.touch(Touch::Down(700.0, 2400.0), &f, 0);
        for k in 1..=4u64 {
            app.touch(Touch::Move(700.0, 2400.0 - 200.0 * k as f32), &f, 8 * k);
        }
        let dragged = app.top().1;
        assert!(dragged > 0.0);
        app.backup_at(at(500_000));
        app.touch(Touch::Move(700.0, 1400.0), &f, 40);
        assert_eq!((app.top().1, app.pull), (dragged + 200.0, 0.0), "the page goes on from where it was");
        app.touch(Touch::Up, &f, 48);
        let lifted = app.top().1;
        app.backup_at(at(600_000));
        app.tick(98);
        assert!(app.top().1 > lifted, "a glide goes on through the page being drawn again");
        assert_eq!(app.overscroll(98), 0.0, "and does not spring back from the top");
    }

    #[test]
    fn a_glide_into_the_end_swings_past_it_and_back() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i);
        let lift = flick(&mut app, &f, 1000, 80.0);
        let mut furthest: f32 = 0.0;
        for ms in (8..2000).step_by(8) {
            app.tick(lift + ms);
            furthest = furthest.max(app.overscroll(lift + ms));
        }
        assert_eq!(app.top().1, app.max_scroll());
        assert!(furthest > app.theme().m.dp(8.0), "it ran on past the end: {furthest}");
        assert_eq!(app.overscroll(lift + 2000), 0.0);
        assert_eq!(app.deadline(lift + 2000), app.idle_due());
    }

    #[test]
    fn a_finger_that_stops_before_lifting_leaves_the_page_still() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i);
        app.touch(Touch::Down(700.0, 2400.0), &f, 1000);
        app.touch(Touch::Move(700.0, 2200.0), &f, 1008);
        app.touch(Touch::Move(700.0, 2000.0), &f, 1016);
        app.touch(Touch::Up, &f, 1100);
        assert!(app.fling.is_none());
        assert_eq!(app.top().1, 400.0);
    }

    #[test]
    fn touching_a_gliding_page_stops_it_without_pressing() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i);
        let lift = flick(&mut app, &f, 1000, 20.0);
        app.tick(lift + 30);
        assert!(app.fling.is_some(), "the page is gliding");
        let held = app.top().1;
        let (x, y) = spot(&mut app, &f, &i, Action::Toggle(Switch::Vibrate));
        assert!(y > 0.0 && y < H as f32, "the switch is on screen");
        app.touch(Touch::Down(x, y), &f, lift + 40);
        app.tick(lift + 500);
        assert_eq!(app.top().1, held, "the page stopped under the finger");
        app.touch(Touch::Up, &f, lift + 520);
        assert!(app.state.vibrate, "the switch was not flipped");
        assert!(app.fling.is_none());
    }

    #[test]
    fn an_edge_swipe_goes_back_over_a_gliding_page() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        app.frame(&f, &i);
        let lift = flick(&mut app, &f, 1000, 20.0);
        app.tick(lift + 30);
        app.touch(Touch::Down(10.0, 1600.0), &f, lift + 40);
        app.touch(Touch::Move(90.0, 1605.0), &f, lift + 48);
        let under = app.top().1;
        app.tick(lift + 64);
        assert!(app.top().1 > under, "the page glides on under the swipe");
        app.touch(Touch::Move(400.0, 1610.0), &f, lift + 72);
        app.touch(Touch::Up, &f, lift + 80);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn a_finger_dragging_from_the_edge_catches_a_gliding_page() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        app.frame(&f, &i);
        let lift = flick(&mut app, &f, 1000, 20.0);
        app.tick(lift + 30);
        app.touch(Touch::Down(10.0, 1600.0), &f, lift + 40);
        app.tick(lift + 48);
        assert!(app.fling.is_some(), "landing at the edge leaves the glide going");
        app.touch(Touch::Move(12.0, 1500.0), &f, lift + 56);
        assert!(app.fling.is_none(), "dragging the page catches it");
        let held = app.top().1;
        app.touch(Touch::Move(12.0, 1450.0), &f, lift + 64);
        assert_eq!(app.top().1, held + 50.0, "and it follows the finger from there");
    }

    #[test]
    fn a_button_stops_a_gliding_page() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i);
        let lift = flick(&mut app, &f, 1000, 20.0);
        assert!(app.fling.is_some(), "the page is gliding");
        app.key(Key::Back, lift + 20);
        assert!(app.fling.is_none());
    }

    /// Drag the page to scroll `to`, resting before lifting so it does not
    /// glide, and let the time pass. The finger first goes 100 px, past the
    /// touch slop, and comes back to the mark.
    fn scroll_to(app: &mut App, f: &Fonts, to: f32, now: u64) {
        let from = app.top().1;
        app.touch(Touch::Down(700.0, 2400.0), f, now);
        app.touch(Touch::Move(700.0, 2300.0), f, now + 200);
        app.touch(Touch::Move(700.0, 2400.0 - (to - from)), f, now + 400);
        app.touch(Touch::Up, f, now + 800);
        app.tick(now + 800);
        assert!((app.top().1 - to).abs() < 0.01, "{} {to}", app.top().1);
    }

    #[test]
    fn the_bar_title_comes_after_the_large_title_fades_and_the_bar_once_content_slides_under() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let fr = app.frame(&f, &i);
        let content_top = fr.content_top;
        let m = app.theme().m;
        scroll_to(&mut app, &f, m.dp(TITLE_FADE_DP) - 2.0, 0);
        assert!(!app.title.on && !app.bar.on);
        scroll_to(&mut app, &f, m.dp(TITLE_FADE_DP) + 2.0, 1000);
        assert!(app.title.on, "the large title is gone");
        assert!(!app.bar.on, "the content is not under the bar yet");
        assert!(app.title.level(1900) < 1.0 && app.title.level(1900) > 0.0, "it eases in");
        assert_eq!(app.title.level(3000), 1.0);
        let under = content_top - widgets::bar_bottom(&app.theme()) - m.dp(20.0);
        scroll_to(&mut app, &f, under + 10.0, 4000);
        assert!(app.bar.on);
        assert_eq!(app.bar.level(4800 + 100), 1.0, "the bar comes in 100 ms");
        scroll_to(&mut app, &f, 0.0, 6000);
        assert!(!app.title.on && !app.bar.on);
    }

    #[test]
    fn the_about_page_brings_title_and_bar_together_once_the_version_fades() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::About, W, H, PPD);
        app.frame(&f, &i);
        let t = app.theme();
        let gone = t.m.dp(ABOUT_LOGO_DP) - widgets::bar_bottom(&t);
        scroll_to(&mut app, &f, gone - 4.0, 0);
        assert!(!app.title.on && !app.bar.on);
        scroll_to(&mut app, &f, gone + 4.0, 1000);
        assert!(app.title.on && app.bar.on);
        assert!((app.bar.level(1800 + 75) - 0.5).abs() < 0.01, "a straight 150 ms fade");
    }

    #[test]
    fn a_scrolled_page_gone_back_to_has_its_bar_from_its_first_frame() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.frame(&f, &i);
        let end = app.max_scroll();
        scroll_to(&mut app, &f, end, 0);
        app.tick(2000);
        assert_eq!((app.title.level(2000), app.bar.level(2000)), (1.0, 1.0), "scrolled down, the bar is up");
        tap_at(&mut app, &f, &i, Action::Open(Screen::Language), 3000);
        assert_eq!(app.screen(), Screen::Language);
        tap_back(&mut app, &f, &i, 4000);
        assert_eq!(app.screen(), Screen::Settings);
        let mut c = Canvas::new(W, H);
        app.tick(4008);
        app.compose(&mut c, &f, &i, "12:00", 4008);
        assert_eq!(app.top().1, end, "the page is where it was left");
        assert_eq!((app.title.level(4008), app.bar.level(4008)), (1.0, 1.0), "its bar is up from the first frame");
    }

    #[test]
    fn theme_previews_switch_the_theme() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        tap(&mut app, &f, &i, Action::Theme { dark: true });
        assert!(app.state.dark);
        assert!(app.theme().dark);
        tap(&mut app, &f, &i, Action::Theme { dark: false });
        assert!(!app.state.dark);
    }

    #[test]
    fn dragging_the_track_sets_brightness() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Brightness).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        assert!(app.touch(Touch::Down(track.x + track.w * 0.25, y), &f, 0).is_empty());
        assert_eq!(app.panel(0), Some(0.25));
        assert!(app.touch(Touch::Move(track.x + track.w * 0.75, y + 40.0), &f, 0).is_empty());
        assert_eq!(app.panel(0), Some(0.75));
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.state.brightness, 0.75);
    }

    #[test]
    fn swiping_the_knob_reboots_to_the_chosen_target() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        tap(&mut app, &f, &i, Action::RebootTarget(3));
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Reboot)).unwrap().rect;
        let y = track.centre().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w, y), &f, 0);
        let fx = app.touch(Touch::Up, &f, 0);
        assert_eq!(fx, vec![Effect::Reboot(Target::Bootloader)]);
        assert_eq!(app.state.confirm, 0.0, "the knob springs back");
    }

    #[test]
    fn letting_go_halfway_does_not_reboot() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Reboot)).unwrap().rect;
        let y = track.centre().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w / 2.0, y), &f, 0);
        assert!(app.touch(Touch::Up, &f, 0).is_empty());
    }

    #[test]
    fn unfinished_features_explain_themselves() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Language, W, H, PPD);
        tap(&mut app, &f, &i, Action::Unavailable("暂不支持该语言"));
        assert_eq!(app.deadline(0), Some(TOAST_MS));
        app.tick(TOAST_MS);
        assert_eq!(app.deadline(TOAST_MS), app.idle_due());
    }

    /// The installing page, opened from the install page, with the
    /// installer started and waiting for the computer.
    fn sideloading(f: &Fonts, i: &DeviceInfo) -> App {
        let mut app = opened(Screen::Install, f, i);
        assert_eq!(tap(&mut app, f, i, Action::Sideload), vec![Effect::Sideload]);
        assert_eq!(app.screen(), Screen::Installing);
        assert_eq!(app.state.install, Install::Starting);
        app.install(Event::Waiting, 0);
        app
    }

    #[test]
    fn leaving_while_waiting_cancels_and_the_end_clears_it() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        assert_eq!(app.state.install, Install::Waiting);
        app.frame(&f, &i);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(90.0, 1600.0), &f, 8);
        app.touch(Touch::Move(400.0, 1600.0), &f, 16);
        assert_eq!(app.touch(Touch::Up, &f, 24), vec![Effect::Cancel]);
        assert_eq!(app.screen(), Screen::Install);
        assert_eq!(app.state.install, Install::Ending);
        // Whatever the installer was in the middle of, only its end counts.
        app.install(Event::Checking { size: 4096 }, 0);
        assert_eq!(app.state.install, Install::Ending);
        tap(&mut app, &f, &i, Action::Sideload);
        assert_eq!(app.screen(), Screen::Install, "no second installer while the first is ending");
        app.install(Event::Finished(Outcome::Cancelled), 0);
        assert_eq!(app.state.install, Install::Idle);
    }

    #[test]
    fn an_image_goes_where_it_is_sent_once_swiped() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 201_326_592 }, 0);
        let kind = ImageKind::Boot { kernel: true, ramdisk: skyward_install::Ramdisk::Recovery };
        app.install(Event::Image { kind, size: 201_326_592, partitions: vec!["recovery", "boot"] }, 0);
        // The image lit the panel, so the user looks and taps past the wake
        // guard.
        tap_at(&mut app, &f, &i, Action::FlashPartition(1), 1000);
        tap_at(&mut app, &f, &i, Action::FlashSlots(Pick::Both), 1000);
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Flash)).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 1000);
        app.touch(Touch::Move(track.x + track.w, y), &f, 1000);
        assert_eq!(app.touch(Touch::Up, &f, 1000), vec![Effect::Flash { partition: "boot", pick: Pick::Both }]);
        assert_eq!(app.state.install, Install::Flashing { partition: "boot", stage: None });
        app.install(Event::Progress(Stage::Flashing { done: 10, total: 40 }), 1000);
        assert_eq!(app.state.install, Install::Flashing { partition: "boot", stage: Some(Stage::Flashing { done: 10, total: 40 }) });
        app.key(Key::Back, 1000);
        assert_eq!(app.screen(), Screen::Installing, "a partition being written keeps the page");
        app.install(Event::Finished(Outcome::Flashed { partition: "boot", slots: vec![Slot::A, Slot::B] }), 1000);
        assert!(matches!(app.state.install, Install::Finished { damage: None, .. }), "a flash that worked broke nothing");
        assert_eq!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 2000), vec![Effect::Reboot(Target::System)]);
        app.key(Key::Back, 2000);
        assert_eq!(app.screen(), Screen::Install);
        assert_eq!(app.state.install, Install::Idle);
    }

    #[test]
    fn an_update_shows_its_steps_and_cannot_be_left() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 7_103_043_868 }, 0);
        app.install(Event::Installing { version: Some("OS3.0.306.0.WMACNXM".into()) }, 0);
        app.install(Event::Overwriting, 0);
        app.install(Event::Progress(Stage::Writing { done: 1 << 30, total: 7 << 30 }), 0);
        assert!(matches!(app.state.install, Install::Updating { overwriting: true, stage: Some(_), .. }));
        let shown = app.frame(&f, &i).hits.len();
        assert_eq!(shown, 0, "nothing on the page to tap while installing");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Installing);
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says why");
        app.install(Event::Finished(Outcome::Failed(Failure::Engine(9))), 0);
        assert!(matches!(app.state.install, Install::Finished { damage: Some(Damage::System), .. }));
        // Sending again starts over on the same page, past the wake guard.
        assert_eq!(tap_at(&mut app, &f, &i, Action::Sideload, 1000), vec![Effect::Sideload]);
        assert_eq!(app.state.install, Install::Starting);
        assert_eq!(app.screen(), Screen::Installing);
    }

    #[test]
    fn a_refused_package_or_an_update_that_kept_the_system_broke_nothing() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 10 }, 0);
        app.install(Event::Finished(Outcome::Refused(Refusal::Unknown)), 0);
        assert_eq!(app.state.install, Install::Finished { outcome: Outcome::Refused(Refusal::Unknown), damage: None });
        tap_at(&mut app, &f, &i, Action::Sideload, 1000);
        app.install(Event::Waiting, 1000);
        app.install(Event::Installing { version: None }, 1000);
        app.install(Event::Finished(Outcome::Failed(Failure::Engine(21))), 1000);
        assert!(matches!(app.state.install, Install::Finished { damage: None, .. }));
    }

    #[test]
    fn an_image_that_failed_part_way_names_its_partition() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Image { kind: ImageKind::Dtbo, size: 1 << 20, partitions: vec!["dtbo"] }, 0);
        app.state.install = Install::Flashing { partition: "dtbo", stage: None };
        app.install(Event::Progress(Stage::Flashing { done: 1 << 20, total: 4 << 20 }), 0);
        app.install(Event::Finished(Outcome::Failed(Failure::Disconnected)), 0);
        assert!(matches!(app.state.install, Install::Finished { damage: Some(Damage::Partition("dtbo")), .. }));
    }

    #[test]
    fn fastboot_is_chosen_and_restarted_to_like_any_target() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        let fastboot = TARGETS.iter().position(|t| *t == Target::Fastboot).unwrap();
        tap(&mut app, &f, &i, Action::RebootTarget(fastboot));
        assert_eq!(app.state.reboot, fastboot);
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Reboot), vec![Effect::Reboot(Target::Fastboot)]);
    }

    /// Press `key` and let it go.
    fn press(app: &mut App, key: Key, now: u64) -> Vec<Effect> {
        let mut fx = app.key(key, now);
        fx.extend(app.key_up(key, now));
        fx
    }

    /// The control the keys are on, on the page drawn.
    fn lit(app: &mut App, f: &Fonts, i: &DeviceInfo) -> Option<Stop> {
        app.frame(f, i);
        app.lit().map(|s| s.stop)
    }

    /// Move the light down the page until it is on `stop`.
    fn light_on(app: &mut App, f: &Fonts, i: &DeviceInfo, stop: Stop) {
        for _ in 0..100 {
            press(app, Key::VolumeDown, 0);
            if app.keys && lit(app, f, i) == Some(stop) {
                return;
            }
        }
        panic!("the keys never came to {stop:?}");
    }

    #[test]
    fn the_keys_reach_the_last_row_of_a_home_page_grown_by_an_update_notice() {
        let f = fonts();
        let i = DeviceInfo { snapshot: SnapshotState::Snapshotted, ..info() };
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.frame(&f, &i);
        light_on(&mut app, &f, &i, Stop::Hit(Action::Open(Screen::Reboot)));
        for ms in (0..3000).step_by(8) {
            app.tick(ms);
        }
        let (_, y) = spot(&mut app, &f, &i, Action::Open(Screen::Reboot));
        assert!(y < H as f32, "the page scrolled the lit row into view");
        press(&mut app, Key::Power, 3000);
        assert_eq!(app.screen(), Screen::Reboot);
    }

    #[test]
    fn the_fastboot_page_is_worked_with_the_keys() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Fastboot, W, H, PPD);
        let choice = |k: usize| Some(Stop::Hit(FASTBOOT_CHOICES[k].1));
        assert_eq!(lit(&mut app, &f, &i), choice(0), "the first choice is lit as the page opens");
        assert!(press(&mut app, Key::VolumeDown, 0).is_empty());
        assert_eq!(lit(&mut app, &f, &i), choice(1), "down moves to the next choice");
        assert!(app.key(Key::Power, 0).is_empty(), "power takes it as it comes up");
        assert_eq!(app.key_up(Key::Power, 0), vec![Effect::EnterRecovery]);
        assert!(app.display_on(), "power leaves the panel on here");
        press(&mut app, Key::VolumeUp, 0);
        press(&mut app, Key::VolumeUp, 0);
        assert_eq!(lit(&mut app, &f, &i), choice(FASTBOOT_CHOICES.len() - 1), "up from the first goes round to the last");
        assert_eq!(press(&mut app, Key::Power, 0), vec![Effect::Reboot(Target::PowerOff)]);
        assert_eq!(tap(&mut app, &f, &i, Action::RebootTo(Target::Bootloader)), vec![Effect::Reboot(Target::Bootloader)], "a touch takes a choice too");
        assert!(press(&mut app, Key::Back, 0).is_empty());
        assert_eq!(app.screen(), Screen::Fastboot, "there is nothing to go back to");
    }

    #[test]
    fn the_recovery_comes_up_from_fastboot_at_the_home_page() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Fastboot, W, H, PPD);
        app.frame(&f, &i);
        app.enter_recovery();
        assert_eq!(app.screen(), Screen::Home);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home, "fastboot is not behind it");
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Search))), "the keys go on working the pages");
        app.touch(Touch::Down(W as f32 / 2.0, 200.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert!(app.key(Key::Power, 0).is_empty());
        assert!(!app.display_on(), "with the light put out, power turns the panel off");
    }

    #[test]
    fn without_a_touchscreen_the_first_control_is_lit_at_once() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.frame(&f, &i);
        assert_eq!(app.light(0.0), None, "a page opens to the touch");
        app.touchscreen_missing();
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Search))));
        assert!(app.light(0.0).is_some(), "lit before any key is pressed");
        assert!(app.take_dirty(), "and shown");
    }

    #[test]
    fn the_volume_keys_light_the_controls_down_the_page_and_up_it() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.frame(&f, &i);
        assert!(!app.keys, "a page opens to the touch");
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Search))), "the first press lights the first control in view");
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::About))));
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Install))));
        press(&mut app, Key::VolumeUp, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::About))));
        press(&mut app, Key::VolumeUp, 0);
        press(&mut app, Key::VolumeUp, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Reboot))), "up from the first goes round to the last");
        app.tick(5000);
        let reboot = app.lit().expect("lit");
        assert!(app.shows(&reboot, app.top().1), "scrolled into view");
        press(&mut app, Key::VolumeDown, 6000);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Search))), "down from the last goes round to the first");
        app.tick(10_000);
        assert_eq!(app.top().1, 0.0, "at the top of the page again");
    }

    #[test]
    fn the_power_key_works_the_control_lit_as_it_comes_up() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.frame(&f, &i);
        light_on(&mut app, &f, &i, Stop::Hit(Action::Open(Screen::Settings)));
        assert!(app.key(Key::Power, 0).is_empty());
        assert_eq!(app.screen(), Screen::Home, "nothing happens as the key goes down");
        app.key_up(Key::Power, 0);
        assert_eq!(app.screen(), Screen::Settings, "the row opens its page as the key comes up");
        let first = lit(&mut app, &f, &i);
        assert!(first.is_some() && first != Some(Stop::Back), "the page's first control is lit, not the `<`: {first:?}");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Settings))), "back on the page, the keys are where they left it");
        light_on(&mut app, &f, &i, Stop::Hit(Action::Open(Screen::Settings)));
        press(&mut app, Key::Power, 0);
        assert_eq!(lit(&mut app, &f, &i), first);
        press(&mut app, Key::VolumeUp, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Back), "up from the page's first control comes to the `<`");
        press(&mut app, Key::Power, 0);
        assert_eq!(app.screen(), Screen::Home, "the `<` goes back");
    }

    #[test]
    fn past_the_last_control_the_keys_scroll_on_half_a_screen_a_press() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Installing, W, H, PPD);
        app.state.install = Install::Finished { outcome: Outcome::Ran { wipe_cache: false }, damage: None };
        app.state.script_output = (0..300).map(|k| format!("- line {k}")).collect();
        app.frame(&f, &i);
        let (max, (top, bottom)) = (app.max_scroll(), app.band(0.0));
        let half = (bottom - top) / 2.0;
        assert!(max > half * 4.0, "the script's output runs on below the controls: {max}");
        let last = *app.spots().expect("drawn").last().expect("controls on the page");
        light_on(&mut app, &f, &i, last.stop);
        let mut at = app.resting();
        let mut presses = 0;
        while at < max {
            press(&mut app, Key::VolumeDown, 0);
            assert_eq!(app.resting(), (at + half).min(max), "half a screen a press");
            at = app.resting();
            presses += 1;
            assert!(presses < 40);
            if !app.shows(&last, at) {
                assert_eq!(lit(&mut app, &f, &i), None, "with no control in view, nothing is lit");
            }
        }
        press(&mut app, Key::VolumeDown, 0);
        let first = app.spots().expect("drawn")[0].stop;
        assert_eq!((lit(&mut app, &f, &i), app.resting()), (Some(first), 0.0), "past the end the light goes round to the top");
        press(&mut app, Key::VolumeUp, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(last.stop), "and up from it round to the last control");
        assert!(app.shows(&last, app.resting()), "which shows");
        let resting = app.resting();
        app.tick(5000);
        assert_eq!(app.top().1, resting, "where the page eased to");
    }

    #[test]
    fn the_power_key_held_slides_a_swipe_control_and_confirms_it_let_go() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        app.frame(&f, &i);
        light_on(&mut app, &f, &i, Stop::Hit(Action::Confirm(Confirm::Reboot)));
        assert!(press(&mut app, Key::Power, 0).is_empty());
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("请按住电源键"), "a short press says how");
        app.key(Key::Power, 1000);
        assert_eq!(app.deadline(1000), Some(0), "the knob moves while the key is held");
        app.tick(1000 + SLIDE_MS / 2);
        assert!((app.state.confirm - 0.5).abs() < 0.01, "half way after half the time: {}", app.state.confirm);
        assert!(app.key_up(Key::Power, 1000 + SLIDE_MS / 2).is_empty(), "let go short of the end, nothing happens");
        assert_eq!(app.state.confirm, 0.0, "and the knob goes back");
        app.key(Key::Power, 3000);
        app.tick(3000 + SLIDE_MS * 2);
        assert_eq!(app.state.confirm, 1.0, "the knob stops at the end");
        assert_eq!(app.key_up(Key::Power, 3000 + SLIDE_MS * 2), vec![Effect::Reboot(TARGETS[0])], "let go there, it confirms");
        assert_eq!(app.state.confirm, 0.0);
        app.key(Key::Power, 6000);
        app.tick(6000 + SLIDE_MS);
        app.touch(Touch::Down(W as f32 / 2.0, 200.0), &f, 6000 + SLIDE_MS);
        assert_eq!(app.state.confirm, 0.0, "a touch lets the knob go");
        assert!(app.key_up(Key::Power, 6000 + SLIDE_MS).is_empty(), "and the key comes up to nothing");
    }

    #[test]
    fn the_brightness_slider_takes_the_volume_keys_until_the_power_key_lets_it_go() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i);
        light_on(&mut app, &f, &i, Stop::Hit(Action::Brightness));
        let level = app.state.brightness;
        press(&mut app, Key::Power, 0);
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("按音量键调节亮度，按电源键完成"));
        press(&mut app, Key::VolumeUp, 0);
        assert!((app.state.brightness - (level + BRIGHTNESS_STEP)).abs() < 1e-5, "up turns it up");
        press(&mut app, Key::VolumeDown, 0);
        press(&mut app, Key::VolumeDown, 0);
        assert!((app.state.brightness - (level - BRIGHTNESS_STEP)).abs() < 1e-5, "down turns it down");
        for _ in 0..20 {
            press(&mut app, Key::VolumeDown, 0);
        }
        assert_eq!(app.state.brightness, MIN_BRIGHTNESS, "no lower than the slider's floor");
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Brightness)), "the light stays on the slider");
        press(&mut app, Key::Power, 0);
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(app.state.brightness, MIN_BRIGHTNESS, "let go, the slider keeps its level");
        assert_ne!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Brightness)), "and the keys move the light again");
    }

    #[test]
    fn the_keys_join_a_patterns_dots_and_end_it_on_the_last_one_joined() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.data_waiting(Some((0, Keys::Pattern)));
        app.frame(&f, &i);
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Dot(0)), "the grid's first dot is lit first");
        assert!(press(&mut app, Key::Power, 0).is_empty());
        assert_eq!(app.state.pattern, [0]);
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("连接完成后，在最后一个点上再按一次电源键"));
        press(&mut app, Key::Power, 0);
        assert!(app.state.pattern.is_empty(), "ended at one dot, the pattern is wiped");
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("请至少连接 4 个点"));
        press(&mut app, Key::Power, 0);
        press(&mut app, Key::VolumeDown, 0);
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Dot(2)));
        press(&mut app, Key::Power, 0);
        assert_eq!(app.state.pattern, [0, 1, 2], "the dot passed straight over is joined on the way");
        for _ in 0..6 {
            press(&mut app, Key::VolumeDown, 0);
        }
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Dot(8)));
        press(&mut app, Key::Power, 0);
        assert_eq!(app.state.pattern, [0, 1, 2, 5, 8]);
        press(&mut app, Key::VolumeUp, 0);
        press(&mut app, Key::VolumeUp, 0);
        press(&mut app, Key::Power, 0);
        assert_eq!(app.state.pattern, [0, 1, 2, 5, 8, 7, 6], "two along the row joins the one between first");
        press(&mut app, Key::VolumeDown, 0);
        assert!(press(&mut app, Key::Power, 0).is_empty(), "a dot joined already is not joined again");
        press(&mut app, Key::VolumeUp, 0);
        assert_eq!(press(&mut app, Key::Power, 0), vec![Effect::Unlock { user: 0, code: "1236987".into() }], "the last one joined ends it");
    }

    #[test]
    fn the_keys_type_a_screen_locks_code_on_its_keypad() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.data_waiting(Some((0, Keys::Digits)));
        app.frame(&f, &i);
        for ch in ['2', '5', '8', '0'] {
            light_on(&mut app, &f, &i, Stop::Hit(Action::Type(Cap::Char(ch))));
            press(&mut app, Key::Power, 0);
        }
        assert_eq!(app.state.code, "2580");
        light_on(&mut app, &f, &i, Stop::Hit(Action::Type(Cap::Enter)));
        assert_eq!(press(&mut app, Key::Power, 0), vec![Effect::Unlock { user: 0, code: "2580".into() }]);
    }

    #[test]
    fn the_power_key_held_on_an_entry_opens_what_can_be_done_with_it() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Files));
        app.set_files("/".into(), Ok(vec![entry("vendor", folder(12)), entry("init.rc", FileKind::File)]), LISTED);
        app.frame(&f, &i);
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::OpenEntry(0))));
        assert!(app.key(Key::Power, 1000).is_empty());
        assert_eq!(app.deadline(1000), Some(1000 + LONG_PRESS_MS), "the press is timed");
        assert!(app.tick(1000 + LONG_PRESS_MS - 1).is_empty());
        assert_eq!(app.tick(1000 + LONG_PRESS_MS), vec![Effect::Haptic], "held long enough, the folder's page opens");
        assert_eq!(app.screen(), Screen::FileInfo);
        assert_eq!(app.state.file.as_ref().map(|e| e.name.as_str()), Some("vendor"));
        assert!(app.key_up(Key::Power, 1000 + LONG_PRESS_MS + 100).is_empty(), "and the key comes up to nothing");
        app.key(Key::Back, 2000);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::OpenEntry(0))));
        assert_eq!(press(&mut app, Key::Power, 3000), vec![Effect::ReadDir("/vendor".into())], "a short press goes into the folder");
    }

    #[test]
    fn a_touch_puts_the_light_out_and_a_volume_key_brings_it_back() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.frame(&f, &i);
        light_on(&mut app, &f, &i, Stop::Hit(Action::Open(Screen::About)));
        let mut c = Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
        assert!(app.light(0.0).is_some(), "lit while the keys work the page");
        app.touch(Touch::Down(W as f32 / 2.0, 200.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert!(app.light(0.0).is_none(), "a touch puts the light out");
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::About))), "the keys come back where they were");
        app.touch(Touch::Down(W as f32 / 2.0, 200.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        press(&mut app, Key::Power, 0);
        assert!(!app.display_on(), "with the light out, power turns the panel off");
        press(&mut app, Key::VolumeDown, 1000);
        assert!(app.display_on(), "a button lights the panel");
        assert!(!app.keys, "and does only that");
    }

    #[test]
    fn a_volume_key_held_moves_the_light_on_and_on() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.frame(&f, &i);
        app.key(Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Search))));
        assert_eq!(app.deadline(0), Some(REPEAT_DELAY_MS));
        app.tick(REPEAT_DELAY_MS - 1);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Search))), "not held long enough yet");
        app.tick(REPEAT_DELAY_MS);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::About))));
        app.tick(REPEAT_DELAY_MS + REPEAT_MS);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Install))));
        app.key_up(Key::VolumeDown, REPEAT_DELAY_MS + REPEAT_MS + 10);
        app.tick(REPEAT_DELAY_MS + REPEAT_MS * 5);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::Open(Screen::Install))), "let go, it stops");
    }

    #[test]
    fn climbing_out_of_a_folder_the_keys_come_back_to_it() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Files));
        let folders: Vec<FileRow> = (0..300).map(|k| entry(&format!("dir{k:03}"), folder(k))).collect();
        app.set_files("/".into(), Ok(folders.clone()), LISTED);
        app.frame(&f, &i);
        press(&mut app, Key::VolumeDown, 0);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::OpenEntry(0))), "the first entry, below the path");
        for _ in 0..40 {
            press(&mut app, Key::VolumeDown, 0);
        }
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::OpenEntry(40))));
        app.tick(5000);
        let row = app.lit().expect("lit");
        assert!(app.shows(&row, app.top().1), "scrolled into view");
        assert_eq!(press(&mut app, Key::Power, 5000), vec![Effect::ReadDir("/dir040".into())]);
        app.set_files("/dir040".into(), Ok(vec![entry("a", FileKind::File)]), LISTED);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::OpenEntry(0))), "a folder opened lights its first entry");
        assert_eq!(app.key(Key::Back, 6000), vec![Effect::ReadDir("/".into())]);
        app.set_files("/".into(), Ok(folders), LISTED);
        assert_eq!(lit(&mut app, &f, &i), Some(Stop::Hit(Action::OpenEntry(40))), "back up, the folder left is lit");
    }

    #[test]
    fn the_cant_start_page_retries_looks_around_or_erases() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::CantStart, W, H, PPD);
        app.set_asked(Asked { reason: Some("RescueParty".into()), ..Asked::default() });
        assert_eq!(tap(&mut app, &f, &i, Action::RebootTo(Target::System)), vec![Effect::Reboot(Target::System)]);
        assert_eq!(swipe(&mut app, &f, &i, Confirm::FactoryReset), vec![Effect::FactoryReset]);
        assert_eq!(tap(&mut app, &f, &i, Action::Dismiss), vec![Effect::Dismiss]);
        assert_eq!(app.screen(), Screen::Home);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home, "the system's question is not behind it");
    }

    #[test]
    fn erasing_goes_part_by_part_and_offers_a_way_out_once_it_stops() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::CantStart, W, H, PPD);
        app.erase_started(true, true, vec![Part::Data, Part::Metadata]);
        assert_eq!(app.screen(), Screen::Erase);
        assert!(app.frame(&f, &i).hits.is_empty(), "nothing is to be done while erasing");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Erase, "there is nothing to go back to");
        app.erase_at(Part::Metadata);
        assert_eq!(app.state.erase.done, 1);
        app.key(Key::Power, 0);
        assert!(!app.display_on());
        app.erase_ended(Err(EraseStop::Failed(Part::Metadata, "make_f2fs ended with exit status: 1".into())), 1000);
        assert!(app.display_on(), "what comes next is the user's to choose");
        assert_eq!(app.state.data, Data::Erased, "the data partition went before /metadata failed");
        let later = 2000;
        assert_eq!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::PowerOff), later), vec![Effect::Reboot(Target::PowerOff)]);
        assert_eq!(tap_at(&mut app, &f, &i, Action::Dismiss, later), vec![Effect::Dismiss]);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn erasing_that_ends_well_leaves_nothing_to_choose() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.erase_started(false, true, vec![Part::Cache]);
        app.erase_ended(Ok(()), 0);
        assert_eq!(app.state.erase.done, 1, "every part is erased");
        assert!(app.frame(&f, &i).hits.is_empty(), "the runtime starts the system at once");
        assert_eq!(app.state.data, Data::Closed, "the data partition was not part of it");
    }

    #[test]
    fn the_wipe_page_erases_everything_or_the_parts_chosen() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Wipe, &f, &i);
        let chosen = |app: &App| WIPES.iter().zip(app.state.wipe).filter(|(_, on)| *on).map(|(&w, _)| w).collect::<Vec<_>>();
        assert!(swipe(&mut app, &f, &i, Confirm::Wipe).is_empty(), "nothing is chosen at first");
        for part in [Wipe::Updates, Wipe::Logs, Wipe::Art, Wipe::SystemCaches, Wipe::AppCaches, Wipe::Media, Wipe::AppData] {
            tap(&mut app, &f, &i, Action::WipeItem(part));
        }
        let parts = vec![Wipe::AppData, Wipe::Media, Wipe::AppCaches, Wipe::SystemCaches, Wipe::Art, Wipe::Logs, Wipe::Updates];
        assert_eq!(chosen(&app), parts, "every part the page lists can be chosen with the others");
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Wipe), vec![Effect::Wipe(parts)], "in the order listed");
        tap(&mut app, &f, &i, Action::WipeItem(Wipe::All));
        assert_eq!(chosen(&app), [Wipe::All], "everything takes the parts in");
        tap(&mut app, &f, &i, Action::WipeItem(Wipe::Art));
        assert_eq!(chosen(&app), [Wipe::Art], "a part on its own leaves everything out");
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Wipe), vec![Effect::Wipe(vec![Wipe::Art])]);
        tap(&mut app, &f, &i, Action::WipeItem(Wipe::Art));
        assert!(chosen(&app).is_empty(), "tapped again, a part is left out");
        tap(&mut app, &f, &i, Action::WipeItem(Wipe::All));
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Wipe), vec![Effect::Wipe(vec![Wipe::All])]);
    }

    #[test]
    fn the_cache_partition_is_offered_only_on_a_phone_that_has_one() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Wipe, &f, &i);
        let offered = |app: &mut App| app.frame(&f, &i).hits.iter().any(|h| h.action == Action::WipeItem(Wipe::Cache));
        assert!(!offered(&mut app));
        app.state.cache_partition = true;
        app.refresh();
        assert!(offered(&mut app));
        tap(&mut app, &f, &i, Action::WipeItem(Wipe::Cache));
        tap(&mut app, &f, &i, Action::WipeItem(Wipe::Media));
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Wipe), vec![Effect::Wipe(vec![Wipe::Media, Wipe::Cache])], "in the order listed");
    }

    #[test]
    fn erasing_the_user_chose_tells_its_progress_and_waits_for_them_at_the_end() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.erase_started(false, false, vec![Part::Media, Part::Art]);
        app.erase_progress(30, 120);
        assert_eq!(app.state.erase.progress, Some((30, 120)));
        app.erase_at(Part::Art);
        assert_eq!(app.state.erase.progress, None, "the next part starts from nothing");
        app.key(Key::Power, 0);
        assert!(!app.display_on());
        app.erase_ended(Ok(()), 1000);
        assert!(app.display_on(), "how it ended waits to be read");
        assert_eq!(app.state.data, Data::Closed, "the data partition itself stays");
        let later = 2000;
        assert_eq!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), later), vec![Effect::Reboot(Target::System)]);
        assert_eq!(tap_at(&mut app, &f, &i, Action::Dismiss, later), vec![Effect::Dismiss]);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn parts_waiting_on_a_locked_user_erase_nothing_and_send_the_user_home_to_decrypt() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.state.data = Data::Locked { user: 0, keys: Keys::Digits, checking: false };
        app.erase_started(false, false, vec![Part::AppCaches, Part::Logs]);
        app.erase_ended(Err(EraseStop::Locked(vec![0])), 0);
        assert_eq!(app.state.erase.done, 0, "nothing was erased");
        assert_eq!(app.state.data, Data::Locked { user: 0, keys: Keys::Digits, checking: false }, "the partition stays as it was");
        assert_eq!(tap_at(&mut app, &f, &i, Action::Dismiss, 1000), vec![Effect::Dismiss]);
        assert_eq!(app.screen(), Screen::Home);
        assert_eq!(tap_at(&mut app, &f, &i, Action::Open(Screen::Decrypt), 1000), Vec::new(), "the screen lock is asked for at once");
        assert_eq!(app.screen(), Screen::Decrypt);
    }

    #[test]
    fn a_data_partition_erased_whole_is_not_opened_again() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.state.data = Data::Open;
        app.erase_started(true, false, vec![Part::Data, Part::Metadata]);
        app.erase_ended(Ok(()), 0);
        assert_eq!(app.state.data, Data::Erased);
        assert_eq!(tap_at(&mut app, &f, &i, Action::Dismiss, 1000), vec![Effect::Dismiss]);
        assert!(tap_at(&mut app, &f, &i, Action::Open(Screen::Decrypt), 1000).is_empty(), "nothing opens until the system makes it afresh");
        assert_eq!(app.screen(), Screen::Decrypt);
    }

    #[test]
    fn an_update_that_erases_all_user_data_waits_to_be_agreed_to() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 7_103_043_868 }, 0);
        app.install(Event::Ready { version: Some("OS3.0.306.0.WMACNXM".into()), wipe: true }, 0);
        assert_eq!(app.state.install, Install::Confirming { version: Some("OS3.0.306.0.WMACNXM".into()), wipe: true });
        // The question lit the panel, so the user swipes past the wake guard.
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::InstallWipe)).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 1000);
        app.touch(Touch::Move(track.x + track.w, y), &f, 1000);
        assert_eq!(app.touch(Touch::Up, &f, 1000), vec![Effect::Install]);
        assert_eq!(app.state.install, Install::Updating { version: Some("OS3.0.306.0.WMACNXM".into()), stage: None, overwriting: false });
        app.key(Key::Back, 1000);
        assert_eq!(app.screen(), Screen::Installing, "an update being written keeps the page");

        let mut app = sideloading(&f, &i);
        app.install(Event::Ready { version: None, wipe: true }, 0);
        assert_eq!(app.key(Key::Back, 1000), vec![Effect::Cancel], "turning it down cancels before anything is written");
        assert_eq!(app.state.install, Install::Ending);
        assert_eq!(app.screen(), Screen::Install);
    }

    #[test]
    fn a_zip_runs_its_install_script_once_agreed_to_and_shows_what_it_says() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Install, &f, &i);
        app.data_waiting(None);
        tap(&mut app, &f, &i, Action::Open(Screen::Choose(Place::Storage)));
        app.set_place(Place::Storage, String::new(), Ok(vec![entry("rom.zip", FileKind::File)]), LISTED);
        assert_eq!(tap(&mut app, &f, &i, Action::ChooseEntry(0)), vec![Effect::InstallFile(Place::Storage, "rom.zip".into())]);
        assert_eq!(app.state.package_name.as_deref(), Some("rom.zip"), "the question names the file");
        app.install(Event::Script, 0);
        assert_eq!(app.state.install, Install::ConfirmingScript);
        // The question lit the panel, so the user swipes past the wake guard.
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::RunScript)).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 1000);
        app.touch(Touch::Move(track.x + track.w, y), &f, 1000);
        assert_eq!(app.touch(Touch::Up, &f, 1000), vec![Effect::Install]);
        assert_eq!(app.state.install, Install::Scripting { stage: None, running: false });
        app.install(Event::Running, 1000);
        for line in ["- Flashing boot", " ", "- Done"] {
            app.install(Event::Print(line.into()), 1000);
        }
        app.install(Event::Progress(Stage::Script { thousandths: 500 }), 1000);
        assert_eq!(app.state.install, Install::Scripting { stage: Some(Stage::Script { thousandths: 500 }), running: true });
        let fr = app.frame(&f, &i);
        assert!(fr.hits.is_empty(), "nothing on the page to tap while the script runs");
        assert_eq!(fr.rows.as_ref().map(Rows::count), Some(3), "a row to each line it showed");
        app.key(Key::Back, 1000);
        assert_eq!(app.screen(), Screen::Installing, "a script running keeps the page");
        assert_eq!(app.deadline(1000), Some(1000 + TOAST_MS), "it says why");
        app.install(Event::Finished(Outcome::Ran { wipe_cache: false }), 1000);
        assert_eq!(app.state.install, Install::Finished { outcome: Outcome::Ran { wipe_cache: false }, damage: None });
        let fr = app.frame(&f, &i);
        let rows = fr.rows.as_ref().expect("what it showed stays on the page");
        assert_eq!(rows.count(), 3);
        assert!(fr.hit(W as f32 / 2.0, rows.rect(1, W as f32).centre().1).is_none(), "its lines answer no touch");
        assert_eq!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 2000), vec![Effect::Reboot(Target::System)]);
    }

    #[test]
    fn a_script_turned_down_never_runs_and_one_that_failed_may_have_written_anywhere() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Script, 0);
        assert_eq!(app.key(Key::Back, 1000), vec![Effect::Cancel], "turning it down cancels before it runs");
        assert_eq!((app.screen(), &app.state.install), (Screen::Install, &Install::Ending));

        let mut app = sideloading(&f, &i);
        app.install(Event::Script, 0);
        app.state.install = Install::Scripting { stage: None, running: false };
        let full = "extract META-INF/com/google/android/update-binary: No space left on device (os error 28)";
        app.install(Event::Finished(Outcome::Failed(Failure::Error(full.into()))), 0);
        assert!(matches!(app.state.install, Install::Finished { damage: None, .. }), "a script that never ran wrote nothing");
        tap_at(&mut app, &f, &i, Action::Sideload, 1000);
        app.install(Event::Waiting, 1000);
        app.install(Event::Script, 1000);
        app.state.install = Install::Scripting { stage: None, running: false };
        app.install(Event::Running, 1000);
        app.install(Event::Print("! Could not write vendor_b".into()), 1000);
        app.install(Event::Finished(Outcome::Failed(Failure::Script(1))), 1000);
        assert_eq!(app.state.install, Install::Finished { outcome: Outcome::Failed(Failure::Script(1)), damage: Some(Damage::Script) });
        assert_eq!(app.state.script_output, ["! Could not write vendor_b"], "what it showed before it failed stays");
        assert_eq!(tap_at(&mut app, &f, &i, Action::Sideload, 2000), vec![Effect::Sideload]);
        assert!(app.state.script_output.is_empty(), "sending again starts afresh");
    }

    #[test]
    fn a_script_page_keeps_the_latest_lines_its_script_showed() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Script, 0);
        app.install(Event::Running, 0);
        for k in 0..SCRIPT_LINES + 2 {
            app.install(Event::Print(format!("line {k}")), 0);
        }
        assert_eq!(app.state.script_output.len(), SCRIPT_LINES);
        assert_eq!(app.state.script_output[0], "line 2");
        assert_eq!(app.frame(&f, &i).rows.as_ref().map(Rows::count), Some(SCRIPT_LINES));
    }

    #[test]
    fn the_install_page_offers_to_keep_skyward_where_the_phone_has_recovery_partitions() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Install, &f, &i);
        let offered = |app: &mut App| app.frame(&f, &i).hits.iter().any(|h| h.action == Action::Toggle(Switch::KeepRecovery));
        assert!(!offered(&mut app), "a phone whose recovery lives in boot");
        app.state.recovery_partitions = true;
        app.refresh();
        assert!(offered(&mut app));
        assert!(app.state.keep_recovery, "on until turned off");
        tap(&mut app, &f, &i, Action::Toggle(Switch::KeepRecovery));
        assert!(!app.state.keep_recovery);
    }

    #[test]
    fn how_skyward_went_back_stays_until_the_next_install() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Script, 0);
        app.install(Event::Running, 0);
        let writing = Stage::Reflashing { done: 1 << 20, total: 4 << 20 };
        app.install(Event::Progress(writing.clone()), 0);
        assert_eq!(app.state.install, Install::Scripting { stage: Some(writing), running: true });
        let why = "write /dev/block/by-name/recovery_a: Input/output error (os error 5)";
        app.install(Event::Reflashed(Reflash::Failed(why.into())), 0);
        app.install(Event::Finished(Outcome::Failed(Failure::Script(1))), 0);
        assert_eq!(app.state.reflash, Some(Reflash::Failed(why.into())));
        tap_at(&mut app, &f, &i, Action::Sideload, 1000);
        assert_eq!(app.state.reflash, None, "sending again starts afresh");

        app.install(Event::Waiting, 1000);
        app.install(Event::Installing { version: None }, 1000);
        app.install(Event::Progress(Stage::Reflashing { done: 1, total: 2 }), 1000);
        assert!(matches!(app.state.install, Install::Updating { stage: Some(Stage::Reflashing { .. }), .. }), "an update writes it back too");
        app.install(Event::Reflashed(Reflash::Written(vec![Slot::A])), 1000);
        app.install(Event::Finished(Outcome::Installed { slot: Slot::A, wipe: false }), 1000);
        assert_eq!(app.state.reflash, Some(Reflash::Written(vec![Slot::A])));
    }

    #[test]
    fn work_the_recovery_does_not_do_is_shown_with_where_to_go() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Unhandled, W, H, PPD);
        app.set_asked(Asked { unhandled: Some("--update_package=/data/ota_package/update.zip".into()), ..Asked::default() });
        assert_eq!(tap(&mut app, &f, &i, Action::RebootTo(Target::System)), vec![Effect::Reboot(Target::System)]);
        assert_eq!(tap(&mut app, &f, &i, Action::Dismiss), vec![Effect::Dismiss]);
        assert_eq!(app.screen(), Screen::Home);
    }

    /// Drag the knob of a swipe-to-confirm track all the way across.
    fn swipe(app: &mut App, f: &Fonts, i: &DeviceInfo, which: Confirm) -> Vec<Effect> {
        let track = app.frame(f, i).hits.iter().find(|h| h.action == Action::Confirm(which)).expect("the track is on the page").rect;
        let y = track.centre().1 - app.top().1;
        let mut fx = app.touch(Touch::Down(track.x + track.h / 2.0, y), f, 0);
        fx.extend(app.touch(Touch::Move(track.x + track.w / 2.0, y), f, 0));
        fx.extend(app.touch(Touch::Move(track.x + track.w, y), f, 0));
        fx.extend(app.touch(Touch::Up, f, 0));
        fx
    }

    #[test]
    fn every_unfinished_entry_now_opens_its_page() {
        let (f, i) = (fonts(), DeviceInfo { slot: Some(Slot::B), ..info() });
        let routes: [(Screen, Screen); 13] = [
            (Screen::Home, Screen::Search),
            (Screen::Home, Screen::Backup),
            (Screen::Home, Screen::Restore),
            (Screen::Home, Screen::Advanced),
            (Screen::Advanced, Screen::Files),
            (Screen::Advanced, Screen::Terminal),
            (Screen::Advanced, Screen::Logs),
            (Screen::Settings, Screen::Language),
            (Screen::Settings, Screen::TimeZone),
            (Screen::Settings, Screen::Terminal),
            (Screen::About, Screen::Logs),
            (Screen::Specs, Screen::License),
            (Screen::Specs, Screen::FontLicense),
        ];
        for (from, to) in routes {
            let mut app = App::new(from, W, H, PPD);
            tap(&mut app, &f, &i, Action::Open(to));
            assert_eq!(app.screen(), to, "from {from:?}");
        }
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Slot));
        assert_eq!(app.screen(), Screen::Slot);
    }

    #[test]
    fn logs_are_exported_as_ticked_where_they_can_go() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Logs, W, H, PPD);
        let both = Effect::ExportLogs { recovery: true, kernel: true, storage: false };
        assert_eq!(tap(&mut app, &f, &i, Action::ExportLogs), vec![both], "both, to the temporary folder, at first");
        tap(&mut app, &f, &i, Action::LogPart(0));
        tap(&mut app, &f, &i, Action::LogsTo(1));
        assert!(tap(&mut app, &f, &i, Action::ExportLogs).is_empty(), "internal storage waits for the data partition");
        assert_eq!(app.deadline(0), Some(TOAST_MS), "and says so");
        app.data_waiting(None);
        assert_eq!(tap(&mut app, &f, &i, Action::ExportLogs), vec![Effect::ExportLogs { recovery: false, kernel: true, storage: true }]);
        tap(&mut app, &f, &i, Action::LogPart(1));
        assert!(tap(&mut app, &f, &i, Action::ExportLogs).is_empty(), "nothing ticked, nothing saved");
    }

    #[test]
    fn a_time_zone_chosen_is_the_one_kept() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::TimeZone));
        assert_eq!(app.state.time_zone, 0, "China until another is chosen");
        let london = TIME_ZONES.iter().position(|&(_, _, name)| name == "Europe/London").expect("London is listed");
        tap(&mut app, &f, &i, Action::TimeZone(london));
        assert_eq!(app.state.time_zone, london);
        tap(&mut app, &f, &i, Action::TimeZone(0));
        assert_eq!(app.state.time_zone, 0, "and China again");
    }

    #[test]
    fn a_backup_takes_what_is_ticked_to_where_it_is_sent() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Backup, &f, &i);
        assert_eq!(app.state.backup, [true, true, false, true]);
        assert!(swipe(&mut app, &f, &i, Confirm::Backup).is_empty(), "internal storage waits for the data partition");
        assert_eq!(app.deadline(0), Some(TOAST_MS), "and says so");
        // The owner's files open first; another user's still wait.
        app.data_waiting(Some((10, Keys::Digits)));
        let first = Effect::Backup { parts: [true, true, false, true], place: Place::Storage, compress: false, verify: true };
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Backup), vec![first]);
        tap(&mut app, &f, &i, Action::BackupPart(2));
        assert!(swipe(&mut app, &f, &i, Confirm::Backup).is_empty(), "the data partition is backed up only with every user's files open");
        app.data_waiting(None);
        tap(&mut app, &f, &i, Action::BackupPart(0));
        assert_eq!(app.state.backup, [false, true, true, true]);
        tap(&mut app, &f, &i, Action::BackupTo(Place::UsbDrive));
        tap(&mut app, &f, &i, Action::Toggle(Switch::Compress));
        tap(&mut app, &f, &i, Action::Toggle(Switch::Verify));
        assert!(swipe(&mut app, &f, &i, Confirm::Backup).is_empty(), "no drive is plugged in");
        app.set_usb_drive(Some(UsbDrive::Shown));
        let second = Effect::Backup { parts: [false, true, true, true], place: Place::UsbDrive, compress: true, verify: false };
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Backup), vec![second]);
        for k in 1..4 {
            tap(&mut app, &f, &i, Action::BackupPart(k));
        }
        assert!(swipe(&mut app, &f, &i, Confirm::Backup).is_empty(), "nothing ticked, nothing backed up");
    }

    /// A backup found in `place`, made at `made`, holding `parts`.
    fn found(place: Place, made: i64, parts: [bool; 4]) -> BackupEntry {
        let held = Held {
            made,
            size: 6_200_000_000,
            parts,
            device: "ishtar".into(),
            system: Some("OS3.0.305.0.WMACNXM".into()),
            slot: Some(Slot::B),
        };
        BackupEntry { place, name: fmt::stamp(made, 0), held: Ok(held) }
    }

    #[test]
    fn backups_found_are_listed_newest_first_and_those_unread_last() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Restore)), vec![Effect::FindBackups { storage: false, usb: false }]);
        let unread = BackupEntry { place: Place::Storage, name: "20261021-150000".into(), held: Err("read backup.toml: not found".into()) };
        let (old, new) = (found(Place::UsbDrive, 1_792_000_000, [true; 4]), found(Place::Storage, 1_792_348_260, [true; 4]));
        app.set_backups(vec![unread.clone(), old.clone(), new.clone()]);
        assert_eq!(app.state.backups, [new, old, unread]);
        let opens = app.frame(&f, &i).hits.iter().filter(|h| matches!(h.action, Action::OpenBackup(_))).count();
        assert_eq!(opens, 3, "each opens, one that cannot be read to be deleted");
    }

    #[test]
    fn a_backup_found_opens_to_choose_what_to_restore() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Restore, &f, &i);
        assert!(app.frame(&f, &i).hits.is_empty(), "nothing found, nothing to open");
        app.set_backups(vec![found(Place::Storage, 1_792_348_260, [true, true, false, true])]);
        tap(&mut app, &f, &i, Action::OpenBackup(0));
        assert_eq!(app.screen(), Screen::RestoreBackup);
        let parts: Vec<Action> =
            app.frame(&f, &i).hits.iter().map(|h| h.action).filter(|a| matches!(a, Action::RestorePart(_))).collect();
        assert_eq!(parts, [Action::RestorePart(0), Action::RestorePart(1), Action::RestorePart(3)], "only what the backup holds");
        tap(&mut app, &f, &i, Action::RestorePart(1));
        assert_eq!(app.state.restore.as_ref().map(|r| r.1), Some([true, false, false, true]));
        let name = fmt::stamp(1_792_348_260, 0);
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Restore), vec![Effect::Restore {
            place: Place::Storage,
            name: name.clone(),
            parts: [true, false, false, true]
        }]);
        for k in [0, 3] {
            tap(&mut app, &f, &i, Action::RestorePart(k));
        }
        assert!(swipe(&mut app, &f, &i, Confirm::Restore).is_empty(), "nothing ticked, nothing restored");
        assert_eq!(app.key(Key::Back, 0), vec![Effect::FindBackups { storage: false, usb: false }], "back on the list, it is looked for again");
        assert_eq!((app.screen(), app.state.restore.clone()), (Screen::Restore, None));
    }

    #[test]
    fn a_data_backup_is_restored_only_with_every_users_files_open() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Restore, &f, &i);
        app.set_backups(vec![found(Place::Storage, 1_792_348_260, [false, false, true, false])]);
        tap(&mut app, &f, &i, Action::OpenBackup(0));
        assert!(swipe(&mut app, &f, &i, Confirm::Restore).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "the page says to decrypt first");
        app.data_waiting(None);
        let fx = swipe(&mut app, &f, &i, Confirm::Restore);
        assert!(matches!(&fx[..], [Effect::Restore { parts: [false, false, true, false], .. }]), "{fx:?}");
    }

    #[test]
    fn a_backup_made_on_another_phone_is_not_restored_here() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Restore, &f, &i);
        let mut other = found(Place::Storage, 1_792_348_260, [true; 4]);
        other.held.as_mut().unwrap().device = "aurora".into();
        let unread = BackupEntry { place: Place::Storage, name: "20261021-150000".into(), held: Err("read backup.toml: not found".into()) };
        app.set_backups(vec![other, unread]);
        for k in 0..2 {
            tap(&mut app, &f, &i, Action::OpenBackup(k));
            let actions: Vec<Action> = app.frame(&f, &i).hits.iter().map(|h| h.action).collect();
            assert!(!actions.contains(&Action::Confirm(Confirm::Restore)) && actions.contains(&Action::AskDelete), "{k}: {actions:?}");
            app.key(Key::Back, 0);
        }
    }

    #[test]
    fn a_backup_is_deleted_once_a_swipe_confirms_it() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Restore, &f, &i);
        app.data_waiting(None);
        let entry = found(Place::Storage, 1_792_348_260, [true; 4]);
        app.set_backups(vec![entry.clone()]);
        tap(&mut app, &f, &i, Action::OpenBackup(0));
        assert!(app.frame(&f, &i).hits.iter().all(|h| h.action != Action::Confirm(Confirm::DeleteBackup)), "asked for first");
        tap(&mut app, &f, &i, Action::AskDelete);
        assert_eq!(swipe(&mut app, &f, &i, Confirm::DeleteBackup), vec![Effect::DeleteBackup { place: Place::Storage, name: entry.name }]);
        assert!(app.backup_deleted(Err("Permission denied".into()), 0).is_empty());
        assert_eq!(app.screen(), Screen::RestoreBackup, "not deleted, the page stays and says why");
        assert_eq!(app.backup_deleted(Ok(()), 0), vec![Effect::FindBackups { storage: true, usb: false }]);
        assert_eq!((app.screen(), app.state.restore.clone()), (Screen::Restore, None));
    }

    #[test]
    fn a_backup_on_a_drive_pulled_out_leaves_its_page() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.set_usb_drive(Some(UsbDrive::Shown));
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Restore)), vec![Effect::FindBackups { storage: false, usb: true }]);
        app.set_backups(vec![found(Place::UsbDrive, 1_792_348_260, [true; 4])]);
        tap(&mut app, &f, &i, Action::OpenBackup(0));
        assert_eq!(app.set_usb_drive(None), vec![Effect::FindBackups { storage: false, usb: false }]);
        assert_eq!((app.screen(), app.state.restore.clone()), (Screen::Restore, None));
        assert_eq!(app.set_usb_drive(Some(UsbDrive::Shown)), vec![Effect::FindBackups { storage: false, usb: true }], "one plugged in is looked in");
    }

    #[test]
    fn backing_up_shows_how_far_it_has_come_and_stops_when_asked() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Backup, &f, &i);
        app.data_waiting(None);
        app.backup_started(false, Place::Storage, true);
        assert_eq!(app.screen(), Screen::BackupJob);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::BackupJob, "the page stays while the backup goes on");
        app.backup_plan(vec![("boot".into(), 200), ("system".into(), 800)]);
        app.backup_at(BackupAt { item: 1, done: 400, of: 800, checking: false });
        assert_eq!(tap(&mut app, &f, &i, Action::StopBackup), vec![Effect::StopBackup]);
        assert!(app.state.backup_job.as_ref().unwrap().stopping);
        assert!(app.frame(&f, &i).hits.iter().all(|h| h.action != Action::StopBackup), "only one stop is asked for");
        app.backup_ended(BackupEnd::Stopped, 0);
        tap_at(&mut app, &f, &i, Action::Back, 1000);
        assert_eq!((app.screen(), app.state.backup_job.clone()), (Screen::Backup, None));
    }

    #[test]
    fn a_restore_checks_its_backup_first_and_goes_on_to_its_end_once_writing() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Restore, &f, &i);
        app.set_backups(vec![found(Place::Storage, 1_792_348_260, [true; 4])]);
        tap(&mut app, &f, &i, Action::OpenBackup(0));
        app.backup_started(true, Place::Storage, true);
        app.backup_plan(vec![("boot".into(), 200), ("data".into(), 800)]);
        app.backup_at(BackupAt { item: 0, done: 100, of: 200, checking: true });
        let stops = |app: &mut App| app.frame(&f, &i).hits.iter().any(|h| h.action == Action::StopBackup);
        assert!(stops(&mut app), "nothing is written while the backup is checked");
        app.backup_at(BackupAt { item: 0, done: 0, of: 400, checking: false });
        assert!(!stops(&mut app), "what has begun to be written is not stopped");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::BackupJob);
        app.backup_ended(BackupEnd::Restored { unmade: vec!["/data/user/10".into()] }, 0);
        assert_eq!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 1000), vec![Effect::Reboot(Target::System)]);
    }

    #[test]
    fn the_slot_page_switches_to_the_slot_picked() {
        let (f, i) = (fonts(), DeviceInfo { slot: Some(Slot::B), ..info() });
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Slot)), vec![Effect::ReadSlots], "read as the page opens");
        let held = BootSlots { active: Slot::B, bootable: [false, true], successful: [true, true] };
        app.set_boot_slots(Ok(held));
        assert!(swipe(&mut app, &f, &i, Confirm::Slot).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "B starts next already");
        tap(&mut app, &f, &i, Action::PickSlot(Slot::A));
        assert_eq!(swipe(&mut app, &f, &i, Confirm::Slot), vec![Effect::SetSlot(Slot::A)]);
        let switched = BootSlots { active: Slot::A, bootable: [true, true], ..held };
        app.slot_switched(Ok(switched), 0);
        assert_eq!((app.state.boot_slots.clone(), app.state.next_slot), (Some(Ok(switched)), None), "the pick is done with");
        tap(&mut app, &f, &i, Action::PickSlot(Slot::B));
        app.key(Key::Back, 0);
        assert_eq!(app.state.next_slot, None, "a pick left unswitched is put down");
    }

    #[test]
    fn the_slot_stays_as_an_update_in_flight_set_it() {
        let (f, i) = (fonts(), DeviceInfo { slot: Some(Slot::B), snapshot: SnapshotState::Snapshotted, ..info() });
        let mut app = App::new(Screen::Slot, W, H, PPD);
        app.set_boot_slots(Ok(BootSlots { active: Slot::A, bootable: [true, true], successful: [false, true] }));
        assert!(app.frame(&f, &i).hits.iter().all(|h| !matches!(h.action, Action::Confirm(_))), "nothing to swipe");
    }

    /// Whether the page shows a key of the keypad or keyboard to tap.
    fn keys_shown(app: &mut App, f: &Fonts, i: &DeviceInfo) -> bool {
        app.frame(f, i).hits.iter().any(|h| matches!(h.action, Action::Type(_)))
    }

    #[test]
    fn the_decrypt_page_opens_the_partition_and_then_asks_with_the_users_own_control() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Decrypt)), vec![Effect::OpenData], "no swipe first");
        assert_eq!(app.state.data, Data::Opening);
        assert!(!keys_shown(&mut app, &f, &i) && app.frame(&f, &i).pattern.is_none(), "nothing to answer yet");
        app.data_waiting(Some((0, Keys::Pattern)));
        assert!(app.frame(&f, &i).pattern.is_some(), "the owner set a pattern, so the grid shows");
        app.data_waiting(Some((0, Keys::Digits)));
        assert!(keys_shown(&mut app, &f, &i), "for a code, the keypad");
        app.key(Key::Back, 0);
        assert!(tap(&mut app, &f, &i, Action::Open(Screen::Decrypt)).is_empty(), "open already, it is not opened again");
        let mut first = App::new(Screen::Decrypt, W, H, PPD);
        assert_eq!(first.arrive(), vec![Effect::OpenData], "a recovery that starts on the page opens it as well");
    }

    #[test]
    fn leaving_the_decrypt_page_goes_back_to_where_it_was_opened_from() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Decrypt, &f, &i);
        tap(&mut app, &f, &i, Action::Skip);
        assert_eq!((app.screen(), app.stack.len()), (Screen::Home, 1), "back on the home page, with no other behind it");
        let mut first = App::new(Screen::Decrypt, W, H, PPD);
        first.arrive();
        tap(&mut first, &f, &i, Action::Skip);
        assert_eq!((first.screen(), first.stack.len()), (Screen::Home, 1), "a recovery that started on the page goes on to the home page");
    }

    #[test]
    fn the_code_typed_on_the_keypad_is_sent_once_and_checked_before_more_is_taken() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.data_waiting(Some((0, Keys::Digits)));
        for ch in "1234".chars() {
            tap(&mut app, &f, &i, Action::Type(Cap::Char(ch)));
        }
        assert_eq!(app.state.code, "1234");
        tap(&mut app, &f, &i, Action::Type(Cap::Backspace));
        assert_eq!(app.state.code, "123");
        let fx = tap(&mut app, &f, &i, Action::Type(Cap::Enter));
        assert_eq!(fx, vec![Effect::Unlock { user: 0, code: "123".into() }]);
        assert_eq!(app.state.code, "", "what was typed is gone once it is sent");
        assert_eq!(app.state.data, Data::Locked { user: 0, keys: Keys::Digits, checking: true });
        assert!(!keys_shown(&mut app, &f, &i), "nothing more is taken while it is checked");
        app.unlock_failed("the secure world does not recognise that credential".into(), 0);
        assert!(keys_shown(&mut app, &f, &i), "turned down, it is asked for again");
        assert_eq!(app.deadline(0), Some(TOAST_MS), "with why");
        app.data_waiting(None);
        assert_eq!(app.state.data, Data::Open);
    }

    #[test]
    fn nothing_typed_is_not_sent() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.data_waiting(Some((0, Keys::Digits)));
        assert!(tap(&mut app, &f, &i, Action::Type(Cap::Enter)).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "the page says to type one first");
    }

    #[test]
    fn a_pattern_is_sent_as_the_numbers_of_the_dots_it_joins() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.data_waiting(Some((0, Keys::Pattern)));
        let (cx, cy, span) = app.frame(&f, &i).pattern.expect("the grid is on the page");
        let dots = widgets::pattern_dots(cx, cy, span);
        let scroll = app.top().1;
        app.touch(Touch::Down(dots[0].0, dots[0].1 - scroll), &f, 0);
        for &which in &[1usize, 4, 7] {
            app.touch(Touch::Move(dots[which].0, dots[which].1 - scroll), &f, 0);
        }
        let fx = app.touch(Touch::Up, &f, 0);
        assert_eq!(fx, vec![Effect::Unlock { user: 0, code: "1258".into() }], "the dots counted from one across the rows");
        assert!(app.state.pattern.is_empty(), "the grid is cleared once it is sent");
        assert!(app.frame(&f, &i).hits.iter().all(|h| h.action != Action::Pattern), "nothing more is drawn while it is checked");
    }

    #[test]
    fn a_pattern_too_short_to_be_one_is_not_sent() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.data_waiting(Some((0, Keys::Pattern)));
        let (cx, cy, span) = app.frame(&f, &i).pattern.expect("the grid is on the page");
        let dots = widgets::pattern_dots(cx, cy, span);
        let scroll = app.top().1;
        app.touch(Touch::Down(dots[0].0, dots[0].1 - scroll), &f, 0);
        app.touch(Touch::Move(dots[1].0, dots[1].1 - scroll), &f, 0);
        let fx = app.touch(Touch::Up, &f, 0);
        assert!(fx.is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "the page says how many dots it takes");
    }

    #[test]
    fn a_partition_that_did_not_open_says_why_and_is_tried_again() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Decrypt, W, H, PPD);
        app.arrive();
        app.data_failed("KeyMint is not running".into());
        assert_eq!(tap(&mut app, &f, &i, Action::OpenData), vec![Effect::OpenData]);
        assert_eq!(app.state.data, Data::Opening);
    }

    /// Type `text` on the keyboard of the page showing, one tap a key, in
    /// the letters layer.
    fn type_text(app: &mut App, f: &Fonts, i: &DeviceInfo, text: &str) {
        for ch in text.chars() {
            tap(app, f, i, Action::Type(Cap::Char(ch)));
        }
    }

    #[test]
    fn search_finds_by_pinyin_typed_on_its_own_keyboard() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Search, &f, &i);
        type_text(&mut app, &f, &i, "bf");
        assert_eq!(app.state.search, "bf");
        let opens: Vec<Action> = app.frame(&f, &i).hits.iter().map(|h| h.action).filter(|a| matches!(a, Action::Open(_))).collect();
        assert_eq!(opens, [Action::Open(Screen::Backup)]);
        tap(&mut app, &f, &i, Action::Type(Cap::Backspace));
        assert_eq!(app.state.search, "b");
        tap(&mut app, &f, &i, Action::ClearSearch);
        assert_eq!(app.state.search, "");
        type_text(&mut app, &f, &i, "xpsj");
        tap(&mut app, &f, &i, Action::Type(Cap::Enter));
        assert_eq!(app.screen(), Screen::ScreenOff, "return opens the first result");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Search);
    }

    #[test]
    fn shift_makes_one_capital_and_the_layers_switch() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Search, &f, &i);
        tap(&mut app, &f, &i, Action::Type(Cap::Shift));
        type_text(&mut app, &f, &i, "ab");
        assert_eq!(app.state.search, "Ab");
        tap(&mut app, &f, &i, Action::Type(Cap::Show(Layer::Digits)));
        type_text(&mut app, &f, &i, "1");
        tap(&mut app, &f, &i, Action::Type(Cap::Show(Layer::Symbols)));
        type_text(&mut app, &f, &i, "|");
        tap(&mut app, &f, &i, Action::Type(Cap::Space));
        assert_eq!(app.state.search, "Ab1| ");
        tap(&mut app, &f, &i, Action::Type(Cap::Show(Layer::Letters)));
        assert_eq!(app.state.layer, Layer::Letters);
    }

    #[test]
    fn a_key_shows_held_down_while_touched() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Search, &f, &i);
        let (x, y) = spot(&mut app, &f, &i, Action::Type(Cap::Char('q')));
        app.touch(Touch::Down(x, y), &f, 0);
        assert_eq!(app.state.pressed, Some(Cap::Char('q')));
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.state.pressed, None);
        assert_eq!(app.state.search, "q");
    }

    #[test]
    fn the_terminal_sends_every_key_to_its_shell() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Terminal)), vec![Effect::StartShell], "the page asks for a shell the first time");
        let (rows, cols) = app.term_size(&f);
        assert!(rows >= 20 && cols >= 40, "a screen of {rows} rows by {cols}");
        app.term_started(rows, cols);
        assert_eq!(tap(&mut app, &f, &i, Action::Type(Cap::Char('l'))), vec![Effect::TermInput(b"l".to_vec())]);
        assert_eq!(tap(&mut app, &f, &i, Action::Type(Cap::Enter)), vec![Effect::TermInput(b"\r".to_vec())]);
        assert!(tap(&mut app, &f, &i, Action::TermKey(TermKey::Ctrl)).is_empty());
        assert_eq!(tap(&mut app, &f, &i, Action::Type(Cap::Char('c'))), vec![Effect::TermInput(vec![0x03])]);
        assert_eq!(tap(&mut app, &f, &i, Action::TermKey(TermKey::Up)), vec![Effect::TermInput(b"\x1b[A".to_vec())]);
        app.term_output(b"ishtar:/ # ");
        tap_back(&mut app, &f, &i, 0);
        assert!(tap(&mut app, &f, &i, Action::Open(Screen::Terminal)).is_empty(), "the same shell goes on");
        app.term_ended("shell 已退出，退出码 0");
        assert!(tap(&mut app, &f, &i, Action::Type(Cap::Char('l'))).is_empty(), "nothing runs to take the keys");
        assert_eq!(tap(&mut app, &f, &i, Action::Type(Cap::Enter)), vec![Effect::StartShell]);
    }

    #[test]
    fn a_drag_on_the_terminal_scrolls_back_through_what_the_shell_wrote() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Terminal, W, H, PPD);
        let (rows, cols) = app.term_size(&f);
        app.term_started(rows, cols);
        let many: String = (0..100).map(|n| format!("line {n}\r\n")).collect();
        app.term_output(many.as_bytes());
        let (card, pitch) = app.frame(&f, &i).scrollback.expect("the terminal's screen scrolls back");
        let (x, y) = (card.x + card.w / 2.0, card.y + card.h / 3.0);
        app.touch(Touch::Down(x, y), &f, 0);
        app.touch(Touch::Move(x, y + pitch * 5.5), &f, 10);
        app.touch(Touch::Up, &f, 20);
        assert_eq!(app.term.as_ref().expect("the shell started").screen().scrollback(), 5, "down shows five older rows");
        assert_eq!(app.top().1, 0.0, "the page itself stays put");
    }

    fn entry(name: &str, kind: FileKind) -> FileRow {
        FileRow { name: name.into(), kind, size: 4096, modified: 1_792_348_260, mode: 0o040_755, uid: 0, gid: 0 }
    }

    /// When the tests list folders: a day after the entries changed.
    const LISTED: i64 = 1_792_348_260 + 86_400;

    fn folder(items: usize) -> FileKind {
        FileKind::Folder { items: Some(items) }
    }

    #[test]
    fn the_file_manager_walks_folders_and_back_up_to_the_root() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Files)), vec![Effect::ReadDir("/".into())]);
        let link = FileKind::Link { target: "/storage/self/primary".into(), folder: true };
        app.set_files("/".into(), Ok(vec![entry("sdcard", link), entry("vendor", folder(12)), entry("init.rc", FileKind::File)]), LISTED);
        assert_eq!(tap(&mut app, &f, &i, Action::OpenEntry(1)), vec![Effect::ReadDir("/vendor".into())]);
        assert_eq!(tap(&mut app, &f, &i, Action::OpenEntry(0)), vec![Effect::ReadDir("/sdcard".into())], "a link to a folder opens like one");
        app.set_files("/vendor".into(), Ok(vec![entry("etc", folder(3))]), LISTED);
        assert_eq!(app.key(Key::Back, 0), vec![Effect::ReadDir("/".into())], "back climbs a folder");
        assert_eq!(app.screen(), Screen::Files);
        app.set_files("/".into(), Ok(vec![entry("init.rc", FileKind::File)]), LISTED);
        tap(&mut app, &f, &i, Action::OpenEntry(0));
        assert_eq!(app.screen(), Screen::FileInfo);
        assert_eq!(app.state.file.as_ref().map(|e| e.name.as_str()), Some("init.rc"));
        app.key(Key::Back, 0);
        assert_eq!(app.state.file, None);
        assert!(app.key(Key::Back, 0).is_empty(), "at the root, back leaves");
        assert_eq!(app.screen(), Screen::Advanced);
    }

    #[test]
    fn a_file_in_internal_storage_is_chosen_once_the_storage_is_open() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Install, &f, &i);
        // Closed, the storage leads to the decrypt page, which comes back.
        assert_eq!(tap(&mut app, &f, &i, Action::Open(Screen::Decrypt)), vec![Effect::OpenData]);
        app.data_waiting(None);
        tap(&mut app, &f, &i, Action::Skip);
        assert_eq!(app.screen(), Screen::Install);
        let storage = Screen::Choose(Place::Storage);
        assert_eq!(tap(&mut app, &f, &i, Action::Open(storage)), vec![Effect::ReadPlace(Place::Storage, String::new())]);
        let listed = vec![
            entry(".thumbnails", folder(9)),
            entry("Download", folder(3)),
            entry("OTA.ZIP", FileKind::File),
            entry("boot.img", FileKind::File),
            entry("notes.txt", FileKind::File),
            entry(".old.zip", FileKind::File),
            entry("fifo", FileKind::Special),
        ];
        app.set_place(Place::Storage, String::new(), Ok(listed), LISTED);
        let shown: Vec<&str> = app.state.storage.entries.as_ref().expect("listed").iter().map(|e| e.name.as_str()).collect();
        assert_eq!(shown, ["Download", "OTA.ZIP", "boot.img"], "folders and what can be installed, hidden ones left out");
        assert_eq!(tap(&mut app, &f, &i, Action::ChooseEntry(0)), vec![Effect::ReadPlace(Place::Storage, "Download".into())]);
        app.set_place(Place::Storage, "Download".into(), Ok(vec![entry("ota.zip", FileKind::File)]), LISTED);
        assert_eq!(tap(&mut app, &f, &i, Action::ChooseEntry(0)), vec![Effect::InstallFile(Place::Storage, "Download/ota.zip".into())]);
        assert_eq!((app.screen(), app.state.source), (Screen::Installing, Source::File(Place::Storage)));
        assert_eq!(app.state.install, Install::Checking { size: 4096 }, "the file is read at once");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Installing, "a file being read keeps the page");
        app.install(Event::Finished(Outcome::Refused(Refusal::Unknown)), 0);
        tap_at(&mut app, &f, &i, Action::Back, 1000);
        assert_eq!((app.screen(), &app.state.install), (storage, &Install::Idle), "back to choose another");
        assert_eq!(app.key(Key::Back, 1000), vec![Effect::ReadPlace(Place::Storage, String::new())], "back climbs to the top");
        app.set_place(Place::Storage, String::new(), Ok(Vec::new()), LISTED);
        assert!(app.key(Key::Back, 1000).is_empty());
        assert_eq!(app.screen(), Screen::Install, "from the top, back leaves");
    }

    #[test]
    fn a_usb_drive_shows_its_files_as_soon_as_it_is_plugged_in() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Install, &f, &i);
        let page = Screen::Choose(Place::UsbDrive);
        assert!(tap(&mut app, &f, &i, Action::Open(page)).is_empty(), "with no drive, nothing to read");
        assert_eq!(app.screen(), page);
        assert!(app.frame(&f, &i).rows.is_none(), "the page waits for one");
        assert_eq!(app.set_usb_drive(Some(UsbDrive::Shown)), vec![Effect::ReadPlace(Place::UsbDrive, String::new())], "read once shown");
        app.set_place(Place::UsbDrive, String::new(), Ok(vec![entry("ROMs", folder(2))]), LISTED);
        assert_eq!(tap(&mut app, &f, &i, Action::ChooseEntry(0)), vec![Effect::ReadPlace(Place::UsbDrive, "ROMs".into())]);
        app.set_place(Place::UsbDrive, "ROMs".into(), Ok(vec![entry("boot.img", FileKind::File)]), LISTED);
        assert_eq!(tap(&mut app, &f, &i, Action::ChooseEntry(0)), vec![Effect::InstallFile(Place::UsbDrive, "ROMs/boot.img".into())]);
        assert_eq!(app.state.source, Source::File(Place::UsbDrive));
        app.install(Event::Finished(Outcome::Failed(Failure::Unreadable("read /mnt/usb/ROMs/boot.img: Input/output error".into()))), 0);
        tap_at(&mut app, &f, &i, Action::Back, 1000);
        assert_eq!(app.screen(), page, "back to choose another");
        assert!(app.set_usb_drive(None).is_empty());
        assert_eq!(app.state.drive.path, "", "a drive pulled out leaves nothing of its folders");
        assert!(app.frame(&f, &i).rows.is_none(), "the page waits for another");
        assert!(app.key(Key::Back, 1000).is_empty());
        assert_eq!(app.screen(), Screen::Install);
        let unreadable = Some(UsbDrive::Unreadable("sdg holds no filesystem the kernel reads".into()));
        assert!(app.set_usb_drive(unreadable).is_empty(), "another page reads nothing");
        assert!(tap_at(&mut app, &f, &i, Action::Open(page), 1000).is_empty(), "nothing on the drive can be read");
    }

    #[test]
    fn a_file_waits_for_the_install_before_it_to_end() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Install, &f, &i);
        app.data_waiting(None);
        tap(&mut app, &f, &i, Action::Open(Screen::Choose(Place::Storage)));
        app.set_place(Place::Storage, String::new(), Ok(vec![entry("ota.zip", FileKind::File)]), LISTED);
        app.state.install = Install::Ending;
        assert!(tap(&mut app, &f, &i, Action::ChooseEntry(0)).is_empty());
        assert_eq!(app.screen(), Screen::Choose(Place::Storage));
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says to wait");
    }

    /// The file manager in /sdcard, listing a folder and a file.
    fn in_sdcard(f: &Fonts, i: &DeviceInfo) -> App {
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, f, i, Action::Open(Screen::Files));
        app.set_files("/sdcard".into(), Ok(vec![entry("DCIM", folder(2)), entry("a.zip", FileKind::File)]), LISTED);
        app
    }

    #[test]
    fn an_entry_held_opens_what_can_be_done_with_it_a_folder_too() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        let (x, y) = spot(&mut app, &f, &i, Action::OpenEntry(0));
        app.touch(Touch::Down(x, y), &f, 1000);
        assert_eq!(app.deadline(1000), Some(1000 + LONG_PRESS_MS), "the press is timed");
        assert!(app.tick(1000 + LONG_PRESS_MS - 1).is_empty());
        assert_eq!(app.tick(1000 + LONG_PRESS_MS), vec![Effect::Haptic], "held long enough, it opens with a tap of the motor");
        assert_eq!(app.screen(), Screen::FileInfo);
        assert_eq!(app.state.file.as_ref().map(|e| e.name.as_str()), Some("DCIM"));
        assert!(app.touch(Touch::Up, &f, 1500).is_empty(), "lifting does not open the folder as well");
        assert_eq!(app.screen(), Screen::FileInfo);
    }

    #[test]
    fn a_picked_entry_goes_into_the_folder_the_bar_is_used_in() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        tap(&mut app, &f, &i, Action::OpenEntry(1));
        assert!(tap(&mut app, &f, &i, Action::PickEntry(FileOp::Copy)).is_empty());
        assert_eq!(app.screen(), Screen::Files, "back to the folders to pick one");
        assert_eq!(app.state.clip, Some(Clip { op: FileOp::Copy, path: "/sdcard/a.zip".into() }));
        assert!(app.frame(&f, &i).dock.is_some(), "the bar is held at the bottom");
        assert_eq!(tap(&mut app, &f, &i, Action::OpenEntry(0)), vec![Effect::ReadDir("/sdcard/DCIM".into())]);
        app.set_files("/sdcard/DCIM".into(), Ok(Vec::new()), LISTED);
        let paste = Effect::Paste { op: FileOp::Copy, path: "/sdcard/a.zip".into(), into: "/sdcard/DCIM".into(), clash: None };
        assert_eq!(tap(&mut app, &f, &i, Action::Paste), vec![paste]);
        app.file_job_started(FileTask::Copy { path: "/sdcard/a.zip".into(), into: "/sdcard/DCIM".into() });
        assert_eq!(app.state.clip, None, "the entry is put down once the copy starts");
        assert_eq!(app.screen(), Screen::FileJob);
        app.file_progress((50, 100), (0, 1));
        assert_eq!(app.key(Key::Back, 0), Vec::new());
        assert_eq!(app.screen(), Screen::FileJob, "the page stays while the copy runs");
        app.file_job_ended(JobEnd::Done, 0);
        assert_eq!(app.screen(), Screen::Files);
        assert_eq!(app.state.job, None);
    }

    #[test]
    fn a_folder_goes_nowhere_inside_itself_and_a_move_nowhere_it_is_already() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        let paste = |app: &mut App, op: FileOp, path: &str| {
            app.state.clip = Some(Clip { op, path: path.into() });
            app.toast = None;
            app.refresh();
            tap(app, &f, &i, Action::Paste)
        };
        assert!(paste(&mut app, FileOp::Copy, "/sdcard").is_empty());
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("无法将\"sdcard\"放入其自身中"));
        assert!(paste(&mut app, FileOp::Move, "/sdcard/a.zip").is_empty());
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("\"a.zip\"已在该文件夹中"));
        assert!(app.state.clip.is_some(), "the entry stays picked, for another folder");
        let copy = Effect::Paste { op: FileOp::Copy, path: "/sdcard/a.zip".into(), into: "/sdcard".into(), clash: None };
        assert_eq!(paste(&mut app, FileOp::Copy, "/sdcard/a.zip"), vec![copy], "a copy where the entry is makes a second one");
    }

    #[test]
    fn a_taken_name_waits_for_the_user_to_choose() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        app.state.clip = Some(Clip { op: FileOp::Move, path: "/tmp/a.zip".into() });
        app.file_clash(FileOp::Move, "/tmp/a.zip".into(), "/sdcard".into(), vec!["a.zip".into()]);
        assert_eq!(app.screen(), Screen::Clash);
        let paste = Effect::Paste { op: FileOp::Move, path: "/tmp/a.zip".into(), into: "/sdcard".into(), clash: Some(ClashChoice::KeepBoth) };
        assert_eq!(tap(&mut app, &f, &i, Action::Resolve(ClashChoice::KeepBoth)), vec![paste]);
        assert_eq!(app.screen(), Screen::Files);
        app.file_clash(FileOp::Move, "/tmp/a.zip".into(), "/sdcard".into(), vec!["a.zip".into()]);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Files, "turning the choice down goes back to the folder");
        assert!(app.state.clip.is_some(), "with the entry still picked, for another folder");
        app.file_clash(FileOp::Move, "/tmp/a.zip".into(), "/sdcard".into(), vec!["a.zip".into()]);
        assert!(tap(&mut app, &f, &i, Action::Resolve(ClashChoice::Skip)).is_empty(), "skipping the entry leaves nothing to do");
        assert_eq!(app.screen(), Screen::Files);
        assert_eq!(app.state.clip, None, "and nothing picked");
    }

    #[test]
    fn renaming_takes_the_name_typed() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        tap(&mut app, &f, &i, Action::OpenEntry(1));
        tap(&mut app, &f, &i, Action::RenameEntry);
        assert_eq!(app.screen(), Screen::Rename);
        assert_eq!(app.state.new_name, "a.zip", "the name it has to start from");
        for _ in 0..4 {
            tap(&mut app, &f, &i, Action::Type(Cap::Backspace));
        }
        type_text(&mut app, &f, &i, "b");
        let rename = Effect::Rename { path: "/sdcard/a.zip".into(), name: "ab".into() };
        assert_eq!(tap(&mut app, &f, &i, Action::Type(Cap::Enter)), vec![rename]);
        app.renamed(Err(RenameRefused::Failed("rename /sdcard/a.zip to ab: No space left on device (os error 28)".into())), 0);
        assert_eq!(app.screen(), Screen::Rename, "a name refused can be typed again");
        app.toast = None;
        app.renamed(Err(RenameRefused::Taken), 0);
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("该文件夹中已有\"ab\""), "as the folder compares names");
        app.renamed(Ok(()), 0);
        assert_eq!(app.screen(), Screen::Files, "renamed, the folder shows again");
    }

    #[test]
    fn a_name_the_folder_cannot_take_is_turned_down_before_renaming() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        tap(&mut app, &f, &i, Action::OpenEntry(1));
        tap(&mut app, &f, &i, Action::RenameEntry);
        let refused =
            [("", "请输入名称"), ("..", "名称不能为\".\"或\"..\""), ("x/y", "名称不能包含\"/\""), ("DCIM", "该文件夹中已有\"DCIM\"")];
        for (name, said) in refused {
            app.state.new_name = name.into();
            app.toast = None;
            assert!(tap(&mut app, &f, &i, Action::Type(Cap::Enter)).is_empty(), "{name:?} is not renamed to");
            assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some(said));
            assert_eq!(app.screen(), Screen::Rename);
        }
        app.state.new_name = "a.zip".into();
        assert!(tap(&mut app, &f, &i, Action::Type(Cap::Enter)).is_empty());
        assert_eq!(app.screen(), Screen::FileInfo, "the name left as it was, the page goes");
    }

    #[test]
    fn deleting_asks_with_a_swipe_and_runs_on_its_page() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        tap(&mut app, &f, &i, Action::OpenEntry(1));
        assert!(app.frame(&f, &i).hits.iter().all(|h| h.action != Action::Confirm(Confirm::DeleteEntry)));
        tap(&mut app, &f, &i, Action::AskDelete);
        assert_eq!(swipe(&mut app, &f, &i, Confirm::DeleteEntry), vec![Effect::Delete("/sdcard/a.zip".into())]);
        app.file_job_started(FileTask::Delete { path: "/sdcard/a.zip".into() });
        assert_eq!(app.screen(), Screen::FileJob);
        assert_eq!(tap(&mut app, &f, &i, Action::StopJob), vec![Effect::StopJob]);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::FileJob, "it stays until stopped");
        app.file_job_ended(JobEnd::Stopped, 0);
        assert_eq!(app.screen(), Screen::FileJob, "how it ended stays to be read");
        app.key(Key::Back, 1000);
        assert_eq!(app.screen(), Screen::Files, "back past the entry's page, which was for an entry being deleted");
    }

    #[test]
    fn work_that_ends_on_its_page_takes_the_title_of_how_it_ended() {
        let (f, i) = (fonts(), info());
        let mut app = in_sdcard(&f, &i);
        app.file_job_started(FileTask::Copy { path: "/sdcard/a.zip".into(), into: "/tmp".into() });
        let mut c = Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
        app.file_job_ended(JobEnd::Stopped, 0);
        app.compose(&mut c, &f, &i, "12:00", 16);
        assert_eq!(app.titles.as_ref().map(|((title, _), _, _)| *title), Some("复制未完成"));
    }

    #[test]
    fn an_unreadable_folder_offers_nothing_to_open() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Files, W, H, PPD);
        app.set_files("/data".into(), Err("read /data: Permission denied (os error 13)".into()), LISTED);
        let fr = app.frame(&f, &i);
        assert!(fr.hits.is_empty() && fr.rows.is_none());
    }

    #[test]
    fn a_long_folder_scrolls_to_its_last_entry_in_one_list() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Files, W, H, PPD);
        let many: Vec<FileRow> = (0..5000).map(|k| entry(&format!("IMG_{k:05}.jpg"), FileKind::File)).collect();
        app.set_files("/sdcard/DCIM/Camera".into(), Ok(many), LISTED);
        let rows = app.frame(&f, &i).rows.as_ref().expect("the entries are listed").count();
        assert_eq!(rows, 5000, "every entry has its row");
        let mut c = Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 0);
        let end = app.max_scroll();
        scroll_to(&mut app, &f, end, 1000);
        app.compose(&mut c, &f, &i, "12:00", 2000);
        assert!(app.drawn.len() < 40, "only the rows near the screen are kept drawn: {}", app.drawn.len());
        assert!(app.drawn.iter().any(|(k, _)| *k == 4999), "the last row shows");
        tap_at(&mut app, &f, &i, Action::OpenEntry(4999), 3000);
        assert_eq!(app.screen(), Screen::FileInfo);
        assert_eq!(app.state.file.as_ref().map(|e| e.name.as_str()), Some("IMG_04999.jpg"));
    }

    #[test]
    fn a_folder_gone_back_up_to_shows_where_it_was_left() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Files));
        let folders: Vec<FileRow> = (0..300).map(|k| entry(&format!("dir{k:03}"), folder(k))).collect();
        app.set_files("/".into(), Ok(folders.clone()), LISTED);
        app.frame(&f, &i);
        scroll_to(&mut app, &f, 9000.0, 0);
        let (_, y) = spot(&mut app, &f, &i, Action::OpenEntry(36));
        assert!(y > 0.0 && y < H as f32, "row 36 is on screen at {y}");
        assert_eq!(tap_at(&mut app, &f, &i, Action::OpenEntry(36), 2000), vec![Effect::ReadDir("/dir036".into())]);
        app.set_files("/dir036".into(), Ok(vec![entry("a", FileKind::File)]), LISTED);
        assert_eq!(app.top().1, 0.0, "a folder opened starts from its top");
        assert_eq!(app.key(Key::Back, 3000), vec![Effect::ReadDir("/".into())]);
        app.set_files("/".into(), Ok(folders), LISTED);
        assert_eq!(app.top().1, 9000.0, "back up, the folder is where it was left");
    }

    #[test]
    fn a_folder_opened_or_gone_back_up_to_has_its_bar_from_its_first_frame() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Files));
        let folders: Vec<FileRow> = (0..300).map(|k| entry(&format!("dir{k:03}"), folder(k))).collect();
        app.set_files("/".into(), Ok(folders.clone()), LISTED);
        app.frame(&f, &i);
        scroll_to(&mut app, &f, 9000.0, 0);
        app.tick(2000);
        assert_eq!((app.title.level(2000), app.bar.level(2000)), (1.0, 1.0), "scrolled down, the bar is up");
        // The runtime lists a folder as it is asked to, then goes round its
        // loop: time passes, and the next frame is the folder's first.
        let mut c = Canvas::new(W, H);
        tap_at(&mut app, &f, &i, Action::OpenEntry(36), 3000);
        app.set_files("/dir036".into(), Ok(vec![entry("a", FileKind::File)]), LISTED);
        app.tick(3008);
        app.compose(&mut c, &f, &i, "12:00", 3008);
        assert_eq!((app.title.level(3008), app.bar.level(3008)), (0.0, 0.0), "a folder opened at its top has only its large title");
        app.tick(4008);
        assert_eq!(app.key(Key::Back, 4100), vec![Effect::ReadDir("/".into())]);
        app.set_files("/".into(), Ok(folders), LISTED);
        app.tick(4108);
        app.compose(&mut c, &f, &i, "12:00", 4108);
        assert_eq!((app.title.level(4108), app.bar.level(4108)), (1.0, 1.0), "back up, the list is under the bar from the first frame");
        assert_ne!(app.deadline(4108), Some(0), "nothing eases in after it");
    }

    #[test]
    fn the_path_goes_up_from_its_capsules_even_held_under_the_bar() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Files, W, H, PPD);
        let many: Vec<FileRow> = (0..200).map(|k| entry(&format!("lib{k:03}.so"), FileKind::File)).collect();
        app.set_files("/system/lib64".into(), Ok(many), LISTED);
        assert_eq!(tap(&mut app, &f, &i, Action::OpenAncestor(1)), vec![Effect::ReadDir("/system".into())]);
        scroll_to(&mut app, &f, 6000.0, 0);
        app.tick(2000);
        let t = app.theme();
        let (_, y) = spot(&mut app, &f, &i, Action::OpenAncestor(0));
        assert!(y > widgets::bar_bottom(&t) && y < widgets::bar_bottom(&t) + t.m.dp(51.0), "the capsules stay under the bar: {y}");
        let mut c = Canvas::new(W, H);
        app.compose(&mut c, &f, &i, "12:00", 2000);
        let (x, _) = spot(&mut app, &f, &i, Action::OpenAncestor(0));
        let px = |c: &Canvas, x: f32, y: f32| c.data[(y as usize * W as usize + x as usize) * 4..][..3].to_vec();
        assert_eq!(px(&c, x - t.m.dp(20.0), y), vec![243, 244, 247], "the root's capsule shows over the list");
        assert_eq!(tap_at(&mut app, &f, &i, Action::OpenAncestor(0), 3000), vec![Effect::ReadDir("/".into())]);
        // The strip's empty end covers the rows beneath it.
        app.touch(Touch::Down(W as f32 - t.m.dp(40.0), y), &f, 4000);
        assert!(app.touch(Touch::Up, &f, 4000).is_empty());
        assert_eq!(app.screen(), Screen::Files);
    }

    #[test]
    fn rows_are_found_by_where_they_lie() {
        let rows = Rows { edges: vec![100.0, 352.0, 682.0, 934.0], action: Some(Action::OpenEntry) };
        assert_eq!(rows.count(), 3);
        assert_eq!(rows.at(99.0), None);
        assert_eq!(rows.at(100.0), Some(0));
        assert_eq!(rows.at(352.0), Some(1));
        assert_eq!(rows.at(933.0), Some(2));
        assert_eq!(rows.at(934.0), None);
        assert_eq!(rows.within(0.0, 100.0), 0..0);
        assert_eq!(rows.within(0.0, 101.0), 0..1);
        assert_eq!(rows.within(351.0, 700.0), 0..3);
        assert_eq!(rows.within(700.0, 5000.0), 2..3);
        assert_eq!(rows.within(934.0, 5000.0), 3..3);
    }

    #[test]
    fn power_key_turns_the_panel_off_and_on() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert!(app.key(Key::Power, 0).is_empty());
        assert_eq!(app.panel(0), None);
        assert!(app.key(Key::Power, 10).is_empty());
        assert_eq!(app.panel(10), Some(0.5));
    }

    /// With the default two minutes, when the panel dims and when it goes
    /// off.
    const DIMS: u64 = 113_000;
    const OFF: u64 = 120_000;

    #[test]
    fn left_alone_the_panel_dims_then_goes_off() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.tick(0);
        assert_eq!(app.deadline(0), Some(DIMS));
        app.tick(DIMS - 1);
        assert_eq!(app.panel(DIMS - 1), Some(0.5));
        app.tick(DIMS);
        assert_eq!(app.deadline(DIMS), Some(DIMS + BACKLIGHT_STEP_MS), "the backlight steps down a frame at a time");
        let midway = app.panel(DIMS + 250).unwrap();
        assert!(midway < 0.5 && midway > dim_level(0.5), "{midway}");
        assert_eq!(app.panel(DIMS + 500), Some(dim_level(0.5)));
        assert_eq!(app.deadline(DIMS + 500), Some(OFF), "dim until it goes off");
        app.tick(OFF);
        assert_eq!(app.panel(OFF), None);
        assert!(!app.display_on());
        assert_eq!(app.deadline(OFF), None, "nothing to wait for on a dark panel");
    }

    #[test]
    fn time_before_the_first_tick_does_not_count_as_idle() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        // The UI started long after boot; the idle clock begins at the
        // first tick, not at zero.
        app.tick(5_000_000);
        assert_eq!(app.panel(5_000_000), Some(0.5));
        assert_eq!(app.deadline(5_000_000), Some(5_000_000 + DIMS));
        app.tick(5_000_000 + OFF);
        assert_eq!(app.panel(5_000_000 + OFF), None);
    }

    #[test]
    fn the_dim_level_sits_below_the_brightness_set() {
        assert_eq!(dim_level(1.0), DIM_CEILING);
        assert_eq!(dim_level(0.06), MIN_BRIGHTNESS);
        assert_eq!(dim_level(0.03), MIN_BRIGHTNESS);
        assert_eq!(dim_level(0.01), 0.01, "never brighter than the level set");
        assert_eq!(dim_span(15_000), 3000);
        assert_eq!(dim_span(120_000), DIM_MS);
    }

    #[test]
    fn a_touch_while_dim_brightens_and_still_counts() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.tick(0);
        app.tick(DIMS);
        assert_eq!(app.panel(DIMS + 500), Some(dim_level(0.5)));
        let (x, y) = spot(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.touch(Touch::Down(x, y), &f, DIMS + 600);
        app.touch(Touch::Up, &f, DIMS + 650);
        assert_eq!(app.screen(), Screen::Settings, "the tap counts");
        assert_eq!(app.panel(DIMS + 800), Some(0.5), "back to full brightness");
        assert_eq!(app.deadline(DIMS + 800), Some(DIMS + 650 + DIMS), "the wait starts over");
    }

    #[test]
    fn a_touch_on_the_dark_panel_only_lights_it() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        let (x, y) = spot(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.key(Key::Power, 0);
        assert!(app.touch(Touch::Down(x, y), &f, 100).is_empty());
        assert_eq!(app.panel(100), Some(0.5));
        assert!(app.touch(Touch::Move(x, y - 300.0), &f, 120).is_empty());
        assert!(app.touch(Touch::Up, &f, 140).is_empty());
        assert_eq!(app.screen(), Screen::Home, "the waking tap pressed nothing");
        assert_eq!(app.top().1, 0.0, "nor scrolled the page");
        tap_at(&mut app, &f, &i, Action::Open(Screen::Settings), 600);
        assert_eq!(app.screen(), Screen::Settings, "a tap past the guard does");
    }

    #[test]
    fn a_key_lights_the_dark_panel_and_does_nothing_else() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        app.key(Key::Power, 0);
        assert!(app.key(Key::Back, 10).is_empty());
        assert!(app.display_on());
        assert_eq!(app.screen(), Screen::Settings, "back did not go back");
    }

    #[test]
    fn a_stray_press_just_after_the_panel_lights_only_keeps_it_awake() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.key(Key::Power, 0);
        // A key wakes the panel; the next, within the guard, does not sleep
        // it again or act.
        app.key(Key::Power, 100);
        assert!(app.display_on());
        assert!(app.key(Key::Power, 300).is_empty());
        assert!(app.display_on(), "a second power press did not turn it off");
        assert!(app.key(Key::Back, 350).is_empty());
        assert_eq!(app.screen(), Screen::Reboot, "back did not go back");
        // Past the guard, power turns it off as usual.
        app.key(Key::Power, 600);
        assert!(!app.display_on());
    }

    #[test]
    fn a_stray_touch_just_after_an_install_lights_the_panel_presses_nothing() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.key(Key::Power, 0);
        app.install(Event::Finished(Outcome::Installed { slot: Slot::A, wipe: false }), 100);
        assert!(app.display_on(), "the finished install lit the panel");
        // A finger reaching for the phone lands on 重启到系统; within the
        // guard it only keeps the panel awake.
        assert!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 200).is_empty());
        assert_eq!(app.screen(), Screen::Installing, "the stray tap did not reboot");
        // Past the guard the same tap reboots.
        assert_eq!(
            tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 700),
            vec![Effect::Reboot(Target::System)]
        );
    }

    #[test]
    fn a_finger_held_as_the_panel_goes_off_lets_go() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Reboot)).unwrap().rect;
        let y = track.centre().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w * 0.6, y), &f, 10);
        assert!(app.state.confirm > 0.0);
        app.key(Key::Power, 20);
        assert_eq!(app.state.confirm, 0.0, "the knob is back at the start");
        app.touch(Touch::Move(track.x + track.w, y), &f, 30);
        assert!(app.touch(Touch::Up, &f, 40).is_empty(), "no reboot from a dark panel");
        assert!(!app.display_on());
    }

    #[test]
    fn never_leaves_the_panel_on() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.state.screen_off = SCREEN_OFF.iter().position(Option::is_none).unwrap();
        app.tick(0);
        assert_eq!(app.deadline(0), None);
        let day = 24 * 3600 * 1000;
        app.tick(day);
        assert_eq!(app.panel(day), Some(0.5));
    }

    #[test]
    fn the_installer_lights_the_panel_when_it_needs_the_user() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.key(Key::Power, 0);
        app.install(Event::Checking { size: 1 << 20 }, 10);
        assert!(!app.display_on(), "reading the package does not light it");
        app.install(Event::Image { kind: ImageKind::Dtbo, size: 1 << 20, partitions: vec!["dtbo"] }, 20);
        assert!(app.display_on(), "an image waiting for its partition does");
        assert_eq!(app.deadline(20), Some(20 + DIMS));
        app.state.install = Install::Flashing { partition: "dtbo", stage: None };
        app.key(Key::Power, 500);
        app.install(Event::Progress(Stage::Flashing { done: 1, total: 2 }), 510);
        assert!(!app.display_on(), "progress does not");
        app.install(Event::Finished(Outcome::Flashed { partition: "dtbo", slots: vec![Slot::A] }), 520);
        assert!(app.display_on(), "the end does");
    }

    #[test]
    fn an_install_ending_after_its_page_was_left_keeps_the_panel_dark() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        assert_eq!(app.key(Key::Back, 0), vec![Effect::Cancel]);
        app.key(Key::Power, 10);
        app.install(Event::Finished(Outcome::Cancelled), 20);
        assert!(!app.display_on());
        assert_eq!(app.state.install, Install::Idle);
    }

    #[test]
    fn choosing_how_long_the_screen_stays_on() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        tap(&mut app, &f, &i, Action::Open(Screen::ScreenOff));
        assert_eq!(app.screen(), Screen::ScreenOff);
        let frame = app.frame(&f, &i);
        assert_eq!(frame.title, "息屏时间");
        assert!(frame.back);
        // The choices sit in order, top to bottom.
        let actions: Vec<Action> = frame.hits.iter().map(|h| h.action).collect();
        assert_eq!(actions, (0..SCREEN_OFF.len()).map(Action::ScreenOff).collect::<Vec<_>>());
        tap(&mut app, &f, &i, Action::ScreenOff(0));
        assert_eq!(app.state.screen_off, 0);
        assert_eq!(app.screen(), Screen::ScreenOff, "the page stays");
        // Fifteen seconds, dim for the last three.
        assert_eq!(app.deadline(0), Some(12_000));
        app.tick(15_000);
        assert!(!app.display_on());
    }
}
