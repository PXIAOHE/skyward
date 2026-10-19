use crate::canvas::{Canvas, Run};
use crate::color::Rgba;
use crate::fling::{Fling, Tracker};
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
use skyward_install::{Event, ImageKind, Outcome, Stage};
use skyward_vab::{Hold, Slot};

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
    /// Receiving what the computer sends with `adb sideload`, and
    /// installing it.
    Sideload,
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
    /// Compress a backup.
    Compress,
    /// Read a backup back once it is written.
    Verify,
}

/// What a backup can take, in the order the backup page lists it.
pub const BACKUP_PARTS: [&str; 4] = ["启动分区", "系统分区", "数据", "基带与 EFS"];

/// Where a backup can go, in the order the backup page lists it.
pub const BACKUP_PLACES: [&str; 2] = ["内部存储", "U 盘"];

/// A backup found on storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupEntry {
    /// When it was made, in Unix seconds.
    pub made: i64,
    pub size: u64,
    /// Where it is, by its place in [`BACKUP_PLACES`].
    pub place: usize,
    /// Which of [`BACKUP_PARTS`] it holds.
    pub parts: [bool; 4],
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
    /// Install an update that erases all user data after it.
    InstallWipe,
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
    /// An update that erases all user data after it waits for the user to
    /// agree; the version it brings.
    Confirming { version: Option<String> },
    /// `overwriting` says whether the update writes over the running
    /// system.
    Updating { version: Option<String>, stage: Option<Stage>, overwriting: bool },
    Flashing { partition: &'static str, stage: Option<Stage> },
    /// `damage` is what a failure may have left broken.
    Finished { outcome: Outcome, damage: Option<Damage> },
    /// The page was left and the installer is handing USB back.
    Ending,
}

/// What a failed install may have left broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// The update had begun writing over the running system.
    System,
    /// The image had begun going into this partition.
    Partition(&'static str),
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
/// how far a computer has the port, as the runtime last read them, and
/// which of [`FASTBOOT_CHOICES`] the keys are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fastboot {
    pub running: bool,
    pub link: UsbLink,
    pub focus: usize,
}

/// How long the screen stays on with nothing touched, in milliseconds, in
/// the order the page lists them; `None` keeps it on.
pub const SCREEN_OFF: [Option<u64>; 7] = [Some(15_000), Some(30_000), Some(60_000), Some(120_000), Some(300_000), Some(600_000), None];

/// What a control does when touched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Open(Screen),
    /// Leave the unlock page for the home page without keeping it behind.
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
    /// Tick or untick a part of the backup, by its place in
    /// [`BACKUP_PARTS`].
    BackupPart(usize),
    /// Send the backup to a place in [`BACKUP_PLACES`].
    BackupTo(usize),
    /// Open a backup found, by its place in the list, to restore it.
    OpenBackup(usize),
    /// Tick or untick a part of the backup being restored.
    RestorePart(usize),
    /// Tick or untick a log to export: the recovery's, the kernel's.
    LogPart(usize),
    /// Save the logs to /tmp, or to internal storage.
    LogsTo(usize),
    /// Start next from this slot; `None` is the one running now.
    PickSlot(Option<Slot>),
    /// A key of the on-screen keyboard.
    Type(Cap),
    /// A key of the terminal's own row, above the keyboard.
    TermKey(TermKey),
    /// Empty the search field.
    ClearSearch,
    /// Open the file manager's entry at this place in the folder.
    OpenEntry(usize),
    /// Go up to the folder this many levels below the root, by its place in
    /// the path shown over the list.
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
    /// Ask to delete the entry the file page shows; a swipe then confirms.
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
}

/// A strip held at the bottom of the screen over the page, such as the
/// file manager's choice of where a picked entry goes.
pub struct Dock {
    pub canvas: Canvas,
    /// In the strip's own coordinates.
    pub hits: Vec<Hit>,
}

/// The rows of a list drawn as they come into view, the way a list view
/// binds its rows: where each lies on the page and what touching one does.
pub struct Rows {
    /// Page y of each row's top, then of the last row's bottom; whole
    /// pixels, so the rows meet without seams.
    pub edges: Vec<f32>,
    pub action: fn(usize) -> Action,
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
        }
    }

    /// The topmost control under a point in page coordinates.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        if let Some(h) = self.hits.iter().rev().find(|h| h.rect.contains(x, y)) {
            return Some(*h);
        }
        let rows = self.rows.as_ref()?;
        let i = rows.at(y)?;
        Some(Hit { rect: rows.rect(i, self.canvas.w as f32), action: (rows.action)(i) })
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
    pub vibrate: bool,
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
    /// The mount page's partitions, as the runtime last read them, or why
    /// it could not.
    pub mounts: Result<Vec<MountRow>, String>,
    pub mount_failure: Option<MountFailure>,
    /// Which of [`BACKUP_PARTS`] a backup takes.
    pub backup: [bool; 4],
    /// Where a backup goes, by its place in [`BACKUP_PLACES`].
    pub backup_to: usize,
    pub compress: bool,
    pub verify: bool,
    /// The backups found, newest first.
    pub backups: Vec<BackupEntry>,
    /// The backup being restored, by its place in `backups`, and which of
    /// its parts to restore.
    pub restore: Option<(usize, [bool; 4])>,
    /// Which logs to export: the recovery's, the kernel's.
    pub logs: [bool; 2],
    /// Where the logs go: 0 for /tmp, 1 for internal storage.
    pub logs_to: usize,
    /// The slot chosen to start from next; `None` keeps the running one.
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
    /// The folder the file manager shows, what is in it as the runtime read
    /// it or why it could not, and when it was read, in Unix seconds: dates
    /// in the list are told against that day.
    pub folder: String,
    pub files: Result<Vec<FileRow>, String>,
    pub listed: i64,
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
    /// Deleting the entry the file page shows was asked for; a swipe
    /// confirms it.
    pub asking_delete: bool,
    /// A copy or move waiting on what to do with the names taken where it
    /// goes, and those names.
    pub clash: Option<(Clip, String, Vec<String>)>,
    /// Work on files under way, or how it last ended.
    pub job: Option<FileJob>,
}

impl Default for State {
    fn default() -> Self {
        State {
            dark: false,
            brightness: 0.5,
            // Two minutes.
            screen_off: 3,
            vibrate: true,
            reboot: 0,
            wipe: [false; WIPES.len()],
            cache_partition: false,
            phone_logs: 0,
            pattern: Vec::new(),
            confirm: 0.0,
            install: Install::Idle,
            mounts: Ok(Vec::new()),
            mount_failure: None,
            backup: [true, true, false, true],
            backup_to: 0,
            compress: false,
            verify: true,
            backups: Vec::new(),
            restore: None,
            logs: [true, true],
            logs_to: 0,
            next_slot: None,
            data: Data::Closed,
            code: String::new(),
            search: String::new(),
            layer: Layer::Letters,
            pressed: None,
            folder: "/".into(),
            files: Ok(Vec::new()),
            listed: 0,
            file: None,
            fastboot: Fastboot { running: false, link: UsbLink::Unplugged, focus: 0 },
            erase: Erase::default(),
            asked: Asked::default(),
            clip: None,
            new_name: String::new(),
            asking_delete: false,
            clash: None,
            job: None,
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
    /// Start receiving a package over USB.
    Sideload,
    /// Write the image the installer holds to `partition` in the slots
    /// `pick` means.
    Flash { partition: &'static str, pick: Pick },
    /// Stop waiting for the computer, or for where an image goes.
    Cancel,
    /// Read the slot's partitions for the mount page, which has just opened.
    ReadMounts,
    /// Mount the slot's partition with this base name, such as `system`.
    Mount(String),
    /// Unmount the slot's partition with this base name.
    Unmount(String),
    /// List the folder at this path for the file manager.
    ReadDir(String),
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
    /// Install the update held for the user to agree to erasing after it.
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

const TOAST_MS: u64 = 2000;
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
/// While the backlight eases, it steps once a frame of a 120 Hz panel.
const BACKLIGHT_STEP_MS: u64 = 8;
/// Just after the panel lights, a touch or key only keeps it awake: the
/// second tap of a wake habit, or the hand reaching for a phone whose
/// screen an install just lit, should not act. Android's double-tap window.
const WAKE_GUARD_MS: u64 = 400;
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

/// Lay the sprite `blurs` keeps, moved by (`dx`, `dy`), in `color` at
/// `level` of its strength, blurred by `blur` pixels.
fn draw_faded(out: &mut Canvas, blurs: &mut Blurs, dx: f32, dy: f32, color: Rgba, level: f32, blur: f32) {
    if level > 0.0 {
        blurs.get(blur).draw(out, dx, dy, color, level);
    }
}

pub struct App {
    pub state: State,
    stack: Vec<(Screen, f32)>,
    frame: Option<Frame>,
    /// The rows of the page's long list drawn so far, by their place in it;
    /// those far off screen are let go, their canvases kept spare for the
    /// next rows to be drawn on.
    drawn: Vec<(usize, Canvas)>,
    spare: Vec<Canvas>,
    /// The file manager's folders left for one inside them, outermost
    /// first, with how far each was scrolled.
    left: Vec<(String, f32)>,
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
    dirty: bool,
    w: u32,
    h: u32,
    px_per_dp: f32,
}

impl App {
    pub fn new(start: Screen, w: u32, h: u32, px_per_dp: f32) -> Self {
        App {
            state: State::default(),
            stack: vec![(start, 0.0)],
            frame: None,
            drawn: Vec::new(),
            spare: Vec::new(),
            left: Vec::new(),
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

    fn top(&self) -> &(Screen, f32) {
        self.stack.last().expect("the page stack always holds a page")
    }

    fn top_mut(&mut self) -> &mut (Screen, f32) {
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
        let moving = self.display_on
            && (self.fling.is_some()
                || self.bounce.is_some()
                || self.cue.is_some()
                || self.title.moving(now)
                || self.bar.moving(now)
                || self.glowing());
        if moving {
            return Some(0);
        }
        let easing = self.bright.moving(now).then_some(now + BACKLIGHT_STEP_MS);
        let held = match self.drag {
            Drag::Press { action: Some(Action::OpenEntry(_)), at, .. } => Some(at + LONG_PRESS_MS),
            _ => None,
        };
        [self.toast.as_ref().map(|t| t.1), easing, self.idle_due(), held].into_iter().flatten().min()
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

    /// Let time pass: the panel dims and goes off once left alone long
    /// enough, a gliding page moves on, a page past its end springs back,
    /// the bar and its title follow the scroll, and a message past its
    /// deadline goes away.
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        // An entry of the file manager held long enough opens what can be
        // done with it, a folder as much as a file, with a tap of the motor.
        if let Drag::Press { action: Some(Action::OpenEntry(i)), at, .. } = self.drag {
            if now >= at + LONG_PRESS_MS {
                self.drag = Drag::Held;
                let entry = self.state.files.as_ref().expect("an entry held is one the page lists")[i].clone();
                self.state.file = Some(entry);
                self.state.asking_delete = false;
                self.open(Screen::FileInfo);
                if self.state.vibrate {
                    fx.push(Effect::Haptic);
                }
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
        self.fling = None;
        self.bounce = None;
        self.pull = 0.0;
        self.cue = None;
    }

    /// Something laid over the page changed (the clock): compose again.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The page's readings changed (battery, storage): render it again.
    pub fn refresh(&mut self) {
        self.frame = None;
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
        let now = Fastboot { running, link, ..self.state.fastboot };
        if self.state.fastboot != now {
            self.state.fastboot = now;
            self.refresh();
        }
    }

    /// Fastboot has stopped and the recovery is up: its pages start again
    /// from the home page, which nothing can go back from to fastboot.
    pub fn enter_recovery(&mut self) {
        self.stack = vec![(Screen::Home, 0.0)];
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
        term.feed(format!("\r\n[shell 已结束，{how}。按回车重新开始]\r\n").as_bytes());
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
        let said = format!("这个文件夹里已经有「{}」", self.state.new_name);
        self.say(&said, now);
    }

    /// The system asked for sideload before the recovery started: the page
    /// opens over the home page, already receiving.
    pub fn start_sideload(&mut self) {
        self.state.install = Install::Starting;
        self.open(Screen::Sideload);
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
        self.stack = vec![(Screen::Erase, 0.0)];
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

    /// The runtime could not mount or unmount a partition: the mount page
    /// says so, and why.
    pub fn mount_failed(&mut self, failure: MountFailure) {
        self.state.mount_failure = Some(failure);
        self.refresh();
    }

    /// The runtime listed a folder for the file manager at `listed`, in Unix
    /// seconds, or could not: it becomes the folder shown. A folder gone back
    /// up to shows where it was left, as the stock file manager does; any
    /// other starts from its top.
    pub fn set_files(&mut self, folder: String, files: Result<Vec<FileRow>, String>, listed: i64) {
        if folder != self.state.folder && self.screen() == Screen::Files {
            let kept = self.left.iter().position(|(path, _)| *path == folder);
            let scroll = kept.map_or(0.0, |k| self.left[k].1);
            self.left.truncate(kept.unwrap_or(self.left.len()));
            self.top_mut().1 = scroll;
            self.fling = None;
            self.bounce = None;
            self.pull = 0.0;
            self.snap = true;
        }
        self.state.folder = folder;
        self.state.files = files;
        self.state.listed = listed;
        self.refresh();
    }

    /// Render the current page if what it shows has changed.
    pub fn frame(&mut self, f: &Fonts, info: &DeviceInfo) -> &Frame {
        if self.frame.is_none() {
            let t = self.theme();
            let screen = self.screen();
            let cx = Ctx { t: &t, f, info, state: &self.state, term: self.term.as_ref() };
            let frame = pages::render(screen, &cx);
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
        let t = self.theme();
        self.frame.as_ref().map_or(0.0, |f| (f.height(&t) - self.h as f32).max(0.0))
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
        let t = self.theme();
        let m = t.m;
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
        let screen = self.screen();
        let scroll = self.top().1;
        let offset = (scroll + self.overscroll(now)).round();
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
        let bar = self.bar.level(now);
        let strip = frame.pinned.as_ref().map(|p| (p, pinned_top(&t, p, offset)));
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
        }
        if let Some(d) = &frame.dock {
            out.put(&d.canvas, (out.h - d.canvas.h) as i32);
        }
        if frame.back {
            let (_, chip, x, y) = self.chip.as_ref().expect("drawn above");
            out.lay(chip, *x, *y, bar);
            widgets::back_arrow(out, &t, t.pal.text);
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

    /// Put the page back where it rests, for a page just opened or returned
    /// to; its first frame shows the bar as its scroll has it.
    fn settle_page(&mut self) {
        self.fling = None;
        self.pull = 0.0;
        self.bounce = None;
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

    fn say(&mut self, text: &str, now: u64) {
        self.toast = Some((text.to_string(), now + TOAST_MS));
        self.dirty = true;
    }

    fn open(&mut self, screen: Screen) {
        self.stack.push((screen, 0.0));
        self.settle_page();
        self.refresh();
    }

    /// Go back a page. Leaving the sideload page stops what can be stopped;
    /// while a package is being read or written, the page stays. In the
    /// file manager, back climbs a folder until it reaches the root.
    fn back(&mut self, now: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.stack.len() <= 1 {
            return fx;
        }
        match self.screen() {
            Screen::Mount => self.state.mount_failure = None,
            Screen::Files => {
                if let Some(up) = std::path::Path::new(&self.state.folder).parent() {
                    return vec![Effect::ReadDir(up.to_string_lossy().into_owned())];
                }
                // Leaving the file manager puts a picked entry down.
                self.state.clip = None;
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
                    self.say("完成或停止之后才能返回", now);
                    return fx;
                }
                self.state.job = None;
            }
            Screen::RestoreBackup => self.state.restore = None,
            _ => {}
        }
        if self.screen() == Screen::Sideload {
            match self.state.install {
                Install::Checking { .. } => {
                    self.say("正在读取安装包，读取完成后才能返回", now);
                    return fx;
                }
                Install::Updating { .. } | Install::Flashing { .. } => {
                    self.say("正在安装，安装完成后才能返回", now);
                    return fx;
                }
                Install::Starting | Install::Waiting | Install::Choosing(_) | Install::Confirming { .. } => {
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
        fx
    }

    /// Hear from the installer. Once the sideload page is left, only its
    /// end matters. An image waiting for its partition, an update waiting
    /// to be agreed to, and the end light a dark panel: all want the user.
    pub fn install(&mut self, ev: Event, now: u64) {
        if self.state.install == Install::Ending {
            if matches!(ev, Event::Finished(_)) {
                self.state.install = Install::Idle;
            }
            return;
        }
        if matches!(ev, Event::Image { .. } | Event::Wipes { .. } | Event::Finished(_)) {
            self.wake(now);
        }
        let install = &mut self.state.install;
        match ev {
            Event::Waiting => *install = Install::Waiting,
            Event::Checking { size } => *install = Install::Checking { size },
            Event::Wipes { version } => *install = Install::Confirming { version },
            Event::Installing { version } => *install = Install::Updating { version, stage: None, overwriting: false },
            Event::Overwriting => {
                if let Install::Updating { overwriting, .. } = install {
                    *overwriting = true;
                }
            }
            Event::Image { kind, size, partitions } => {
                *install = Install::Choosing(Choice { kind, size, partitions, partition: 0, pick: Pick::Current })
            }
            Event::Progress(stage) => match install {
                Install::Updating { stage: shown, .. } | Install::Flashing { stage: shown, .. } => *shown = Some(stage),
                _ => {}
            },
            Event::Finished(outcome) => {
                let damage = match (&outcome, &*install) {
                    (Outcome::Failed(_), Install::Updating { overwriting: true, .. }) => Some(Damage::System),
                    (Outcome::Failed(_), Install::Flashing { partition, stage: Some(Stage::Flashing { done, .. }) }) if *done > 0 => {
                        Some(Damage::Partition(partition))
                    }
                    _ => None,
                };
                *install = Install::Finished { outcome, damage };
            }
        }
        if self.screen() == Screen::Sideload {
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
            Screen::Files => vec![Effect::ReadDir(self.state.folder.clone())],
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
            _ => Vec::new(),
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
                self.open(s);
                return self.arrive_at(s);
            }
            Action::OpenData => return self.open_data(),
            Action::Skip => {
                *self.top_mut() = (Screen::Home, 0.0);
                self.settle_page();
                self.refresh();
            }
            Action::Back => return self.back(now),
            Action::Theme { dark } => {
                if self.state.dark != dark {
                    self.state.dark = dark;
                    self.refresh();
                }
            }
            Action::Toggle(switch) => {
                let on = match switch {
                    Switch::Vibrate => &mut self.state.vibrate,
                    Switch::Compress => &mut self.state.compress,
                    Switch::Verify => &mut self.state.verify,
                };
                *on = !*on;
                self.refresh();
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
            Action::Confirm(_) => self.say("按住圆形按钮向右滑动", now),
            Action::Brightness | Action::Pattern => {}
            Action::Sideload => match self.state.install {
                Install::Idle | Install::Finished { .. } => {
                    self.state.install = Install::Starting;
                    if self.screen() == Screen::Sideload {
                        self.refresh();
                    } else {
                        self.open(Screen::Sideload);
                    }
                    return vec![Effect::Sideload];
                }
                _ => self.say("上一次安装还在收尾，请稍后再尝试", now),
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
                self.stack = vec![(Screen::Home, 0.0)];
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
                let (path, folder) = (std::path::Path::new(&clip.path), std::path::Path::new(&self.state.folder));
                let name = path.file_name().map_or_else(|| clip.path.clone(), |n| n.to_string_lossy().into_owned());
                if folder.starts_with(path) {
                    self.say(&format!("不能把「{name}」放进它自己里面"), now);
                } else if clip.op == FileOp::Move && path.parent() == Some(folder) {
                    self.say(&format!("「{name}」已经在这个文件夹里"), now);
                } else {
                    return vec![Effect::Paste { op: clip.op, path: clip.path, into: self.state.folder.clone(), clash: None }];
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
            Action::BackupPart(k) => {
                self.state.backup[k] = !self.state.backup[k];
                self.refresh();
            }
            Action::BackupTo(k) => {
                self.state.backup_to = k;
                self.refresh();
            }
            Action::OpenBackup(i) => {
                self.state.restore = Some((i, self.state.backups[i].parts));
                self.open(Screen::RestoreBackup);
            }
            Action::RestorePart(k) => {
                let (_, parts) = self.state.restore.as_mut().expect("the restore page shows a backup");
                parts[k] = !parts[k];
                self.refresh();
            }
            Action::LogPart(k) => {
                self.state.logs[k] = !self.state.logs[k];
                self.refresh();
            }
            Action::LogsTo(k) => {
                self.state.logs_to = k;
                self.refresh();
            }
            Action::PickSlot(slot) => {
                self.state.next_slot = slot;
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
                self.refresh();
            }
            Action::OpenEntry(i) => {
                let entry = self.state.files.as_ref().expect("an entry opened is one the page lists")[i].clone();
                match entry.kind {
                    FileKind::Folder { .. } | FileKind::Link { folder: true, .. } => {
                        // The folder left keeps its place for coming back.
                        let scroll = self.top().1;
                        self.left.push((self.state.folder.clone(), scroll));
                        let path = std::path::Path::new(&self.state.folder).join(&entry.name);
                        return vec![Effect::ReadDir(path.to_string_lossy().into_owned())];
                    }
                    FileKind::File | FileKind::Link { folder: false, .. } | FileKind::Special => {
                        self.state.file = Some(entry);
                        self.open(Screen::FileInfo);
                    }
                }
            }
            Action::OpenAncestor(depth) => {
                let parts = std::path::Path::new(&self.state.folder).components();
                if depth + 1 < parts.clone().count() {
                    let path: std::path::PathBuf = parts.take(depth + 1).collect();
                    return vec![Effect::ReadDir(path.to_string_lossy().into_owned())];
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
        std::path::Path::new(&self.state.folder).join(&entry.name).to_string_lossy().into_owned()
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
                if shifted {
                    self.state.layer = Layer::Letters;
                }
            }
            Cap::Space => self.typing().push(' '),
            Cap::Backspace => {
                self.typing().pop();
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
                Screen::Decrypt if self.state.code.is_empty() => self.say("先输入锁屏密码", now),
                Screen::Decrypt => {
                    let code = std::mem::take(&mut self.state.code);
                    return self.unlock(code);
                }
                // A name the folder cannot take is turned down here, in
                // the user's words; an unchanged one leaves the page.
                Screen::Rename => {
                    let name = self.state.new_name.clone();
                    let current = &self.state.file.as_ref().expect("the file page shows an entry").name;
                    let taken = self.state.files.as_ref().is_ok_and(|entries| entries.iter().any(|e| e.name == name));
                    match name.as_str() {
                        "" => self.say("先输入名称", now),
                        "." | ".." => self.say("名称不能是「.」或「..」", now),
                        _ if name.contains('/') => self.say("名称里不能有「/」", now),
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
                    let done = self.state.confirm > 0.9;
                    self.state.confirm = 0.0;
                    self.repaint_confirm(&t, f, which, track);
                    if done {
                        match which {
                            Confirm::Reboot => fx.push(Effect::Reboot(TARGETS[self.state.reboot])),
                            Confirm::Wipe => {
                                let chosen: Vec<Wipe> = WIPES.iter().zip(self.state.wipe).filter(|(_, on)| *on).map(|(&w, _)| w).collect();
                                match chosen.is_empty() {
                                    true => self.say("请选择要清除的内容", now),
                                    false => fx.push(Effect::Wipe(chosen)),
                                }
                            }
                            Confirm::InstallWipe => {
                                if let Install::Confirming { version } = &self.state.install {
                                    self.state.install = Install::Updating { version: version.clone(), stage: None, overwriting: false };
                                    fx.push(Effect::Install);
                                    self.refresh();
                                }
                            }
                            Confirm::Backup => self.say("备份还没实现", now),
                            Confirm::Restore => self.say("恢复还没实现", now),
                            Confirm::Slot => match self.state.next_slot {
                                None => self.say("已经从这个槽位启动", now),
                                Some(_) => self.say("切换槽位还没实现", now),
                            },
                            Confirm::FactoryReset => fx.push(Effect::FactoryReset),
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
                    }
                }
                Drag::Pattern => {
                    // A pattern reads as the number of each dot it joins,
                    // counted from one across the rows.
                    let drawn = std::mem::take(&mut self.state.pattern);
                    self.repaint_pattern(&t);
                    match drawn.len() >= 4 {
                        true => fx.extend(self.unlock(drawn.iter().map(|i| (b'1' + *i as u8) as char).collect())),
                        false => self.say("至少连接 4 个点", now),
                    }
                }
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

    pub fn key(&mut self, key: Key, now: u64) -> Vec<Effect> {
        // Any button stops a gliding page.
        self.fling = None;
        if !self.display_on {
            // Any button lights the dark panel, and does only that.
            self.wake(now);
            return Vec::new();
        }
        // A button pressed just after the panel lit only keeps it awake, so
        // a power press meant to wake it does not turn it back off.
        if self.just_woke(now) {
            self.stir(now);
            return Vec::new();
        }
        // The touchscreen may run from the partitions fastboot leaves out,
        // so its page is worked with the keys: volume moves between the
        // choices, power takes the one it is on.
        if self.screen() == Screen::Fastboot {
            self.stir(now);
            let (n, focus) = (FASTBOOT_CHOICES.len(), self.state.fastboot.focus);
            self.state.fastboot.focus = match key {
                Key::VolumeUp => (focus + n - 1) % n,
                Key::VolumeDown => (focus + 1) % n,
                Key::Power => return self.act(FASTBOOT_CHOICES[focus].1, now),
                Key::Back => focus,
            };
            if self.state.fastboot.focus != focus {
                self.refresh();
            }
            return Vec::new();
        }
        if key == Key::Power {
            self.sleep();
            return Vec::new();
        }
        self.stir(now);
        let step = self.h as f32 / 3.0;
        match key {
            Key::Back => self.act(Action::Back, now),
            Key::VolumeUp => {
                self.scroll_by(-step);
                Vec::new()
            }
            Key::VolumeDown => {
                self.scroll_by(step);
                Vec::new()
            }
            Key::Power => unreachable!("handled above"),
        }
    }

    fn scroll_by(&mut self, d: f32) {
        let max = self.max_scroll();
        let top = self.top_mut();
        let next = (top.1 + d).clamp(0.0, max);
        if next != top.1 {
            top.1 = next;
            self.dirty = true;
        }
    }

    /// Whether the page is gliding or springing back at `now`.
    fn in_motion(&self, now: u64) -> bool {
        self.fling.is_some_and(|fl| !fl.at(now).1) || self.bounce.is_some_and(|b| !b.at(now).1)
    }

    /// Stop a page gliding or springing back where it has got to, for a
    /// finger to hold: one past its end stays out there as the finger's
    /// pull. Whether there was one.
    fn catch(&mut self, now: u64) -> bool {
        let glide = self.fling.take().is_some_and(|fl| !fl.at(now).1);
        let spring = self.bounce.take().map(|b| b.at(now)).filter(|&(_, done)| !done);
        if let Some((offset, _)) = spring {
            self.pull = motion::unrubber(offset, self.h as f32);
        }
        glide || spring.is_some()
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
        self.state.brightness = ((x - track.x) / track.w).clamp(MIN_BRIGHTNESS, 1.0);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.card);
            widgets::slider(&mut fr.canvas, t, track.x, track.y, track.w, self.state.brightness);
        }
        self.dirty = true;
    }

    fn repaint_confirm(&mut self, t: &Theme, f: &Fonts, which: Confirm, track: Rect) {
        let (label, danger) = pages::confirm_label(which);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.bg);
            widgets::confirm(&mut fr.canvas, t, f, track.y, label, danger, self.state.confirm);
        }
        self.dirty = true;
    }

    /// Add the dot under the finger to the pattern being drawn.
    fn trace(&mut self, t: &Theme, x: f32, y: f32) {
        let Some((cx, cy, span)) = self.frame.as_ref().and_then(|fr| fr.pattern) else { return };
        let reach = t.m.dp(32.0);
        let dots = widgets::pattern_dots(cx, cy, span);
        let near = dots.iter().position(|&(dx, dy)| (dx - x).hypot(dy - y) < reach);
        if let Some(i) = near {
            if !self.state.pattern.contains(&i) {
                self.state.pattern.push(i);
                self.repaint_pattern(t);
            }
        }
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
        let k = (0..rows.count()).find(|&k| (rows.action)(k) == action).expect("control on the page");
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
    fn an_unreadable_partition_table_leaves_nothing_to_toggle() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Mount, &f, &i);
        app.set_mounts(Err("read the metadata in super: Input/output error (os error 5)".into()));
        assert!(app.frame(&f, &i).hits.is_empty());
    }

    #[test]
    fn nothing_mounts_while_an_update_waits_to_merge() {
        let (f, mut i) = (fonts(), info());
        i.snapshot = SnapshotState::Snapshotted;
        let mut app = opened(Screen::Mount, &f, &i);
        app.set_mounts(Ok(vec![
            MountRow { base: "vendor".into(), point: "/vendor".into(), mounted: false, holders: Vec::new() },
            MountRow { base: "odm".into(), point: "/odm".into(), mounted: true, holders: Vec::new() },
        ]));
        let frame = app.frame(&f, &i);
        assert!(matches!(frame.hits[0].action, Action::Unavailable(_)), "vendor stays unmounted");
        assert_eq!(frame.hits[1].action, Action::MountToggle(1), "odm can still come off");
        let refused = frame.hits[0].action;
        assert!(tap(&mut app, &f, &i, refused).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says why");
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
        tap(&mut app, &f, &i, Action::Unavailable("这种语言还没有翻译"));
        assert_eq!(app.deadline(0), Some(TOAST_MS));
        app.tick(TOAST_MS);
        assert_eq!(app.deadline(TOAST_MS), app.idle_due());
    }

    /// The sideload page, opened from the installing page, with the
    /// installer started and waiting for the computer.
    fn sideloading(f: &Fonts, i: &DeviceInfo) -> App {
        let mut app = opened(Screen::Install, f, i);
        assert_eq!(tap(&mut app, f, i, Action::Sideload), vec![Effect::Sideload]);
        assert_eq!(app.screen(), Screen::Sideload);
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
        assert_eq!(app.screen(), Screen::Sideload, "a partition being written keeps the page");
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
        assert_eq!(app.screen(), Screen::Sideload);
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says why");
        app.install(Event::Finished(Outcome::Failed(Failure::Engine(9))), 0);
        assert!(matches!(app.state.install, Install::Finished { damage: Some(Damage::System), .. }));
        // Sending again starts over on the same page, past the wake guard.
        assert_eq!(tap_at(&mut app, &f, &i, Action::Sideload, 1000), vec![Effect::Sideload]);
        assert_eq!(app.state.install, Install::Starting);
        assert_eq!(app.screen(), Screen::Sideload);
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

    #[test]
    fn the_fastboot_page_is_worked_with_the_keys() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Fastboot, W, H, PPD);
        app.frame(&f, &i);
        assert_eq!(app.state.fastboot.focus, 0);
        assert!(app.key(Key::VolumeDown, 0).is_empty());
        assert_eq!(app.state.fastboot.focus, 1, "down moves to the next choice");
        assert_eq!(app.key(Key::Power, 0), vec![Effect::EnterRecovery], "power takes it");
        assert!(app.display_on(), "power leaves the panel on here");
        app.key(Key::VolumeUp, 0);
        app.key(Key::VolumeUp, 0);
        assert_eq!(app.state.fastboot.focus, FASTBOOT_CHOICES.len() - 1, "up from the first goes round to the last");
        assert_eq!(app.key(Key::Power, 0), vec![Effect::Reboot(Target::PowerOff)]);
        assert_eq!(tap(&mut app, &f, &i, Action::RebootTo(Target::Bootloader)), vec![Effect::Reboot(Target::Bootloader)], "a touch takes a choice too");
        assert!(app.key(Key::Back, 0).is_empty());
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
        assert!(app.key(Key::Power, 0).is_empty());
        assert!(!app.display_on(), "power turns the panel off again");
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
        app.install(Event::Wipes { version: Some("OS3.0.306.0.WMACNXM".into()) }, 0);
        assert_eq!(app.state.install, Install::Confirming { version: Some("OS3.0.306.0.WMACNXM".into()) });
        // The question lit the panel, so the user swipes past the wake guard.
        let track = app.frame(&f, &i).hits.iter().find(|h| h.action == Action::Confirm(Confirm::InstallWipe)).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 1000);
        app.touch(Touch::Move(track.x + track.w, y), &f, 1000);
        assert_eq!(app.touch(Touch::Up, &f, 1000), vec![Effect::Install]);
        assert_eq!(app.state.install, Install::Updating { version: Some("OS3.0.306.0.WMACNXM".into()), stage: None, overwriting: false });
        app.key(Key::Back, 1000);
        assert_eq!(app.screen(), Screen::Sideload, "an update being written keeps the page");

        let mut app = sideloading(&f, &i);
        app.install(Event::Wipes { version: None }, 0);
        assert_eq!(app.key(Key::Back, 1000), vec![Effect::Cancel], "turning it down cancels before anything is written");
        assert_eq!(app.state.install, Install::Ending);
        assert_eq!(app.screen(), Screen::Install);
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
    fn a_backup_takes_what_is_ticked_to_where_it_is_sent() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Backup, &f, &i);
        assert_eq!(app.state.backup, [true, true, false, true]);
        tap(&mut app, &f, &i, Action::BackupPart(2));
        tap(&mut app, &f, &i, Action::BackupPart(0));
        assert_eq!(app.state.backup, [false, true, true, true]);
        tap(&mut app, &f, &i, Action::BackupTo(1));
        assert_eq!(app.state.backup_to, 1);
        tap(&mut app, &f, &i, Action::Toggle(Switch::Compress));
        tap(&mut app, &f, &i, Action::Toggle(Switch::Verify));
        assert!(app.state.compress && !app.state.verify);
        assert!(swipe(&mut app, &f, &i, Confirm::Backup).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says backing up is not done yet");
    }

    #[test]
    fn a_backup_found_opens_to_choose_what_to_restore() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Restore, &f, &i);
        assert!(app.frame(&f, &i).hits.is_empty(), "nothing found, nothing to open");
        app.state.backups = vec![
            BackupEntry { made: 1_792_348_260, size: 6_200_000_000, place: 0, parts: [true, true, false, true] },
            BackupEntry { made: 1_792_000_000, size: 120_000_000, place: 1, parts: [true, false, false, false] },
        ];
        app.refresh();
        tap(&mut app, &f, &i, Action::OpenBackup(0));
        assert_eq!(app.screen(), Screen::RestoreBackup);
        assert_eq!(app.state.restore, Some((0, [true, true, false, true])));
        let parts: Vec<Action> =
            app.frame(&f, &i).hits.iter().map(|h| h.action).filter(|a| matches!(a, Action::RestorePart(_))).collect();
        assert_eq!(parts, [Action::RestorePart(0), Action::RestorePart(1), Action::RestorePart(3)], "only what the backup holds");
        tap(&mut app, &f, &i, Action::RestorePart(1));
        assert_eq!(app.state.restore, Some((0, [true, false, false, true])));
        assert!(swipe(&mut app, &f, &i, Confirm::Restore).is_empty());
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Restore);
        assert_eq!(app.state.restore, None);
    }

    #[test]
    fn the_slot_page_picks_the_other_slot_and_the_reboot_page_says_so() {
        let (f, i) = (fonts(), DeviceInfo { slot: Some(Slot::B), ..info() });
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Slot));
        assert!(swipe(&mut app, &f, &i, Confirm::Slot).is_empty());
        assert_eq!(app.deadline(0), Some(TOAST_MS), "already starting from B");
        tap(&mut app, &f, &i, Action::PickSlot(Some(Slot::A)));
        assert_eq!(app.state.next_slot, Some(Slot::A));
        tap(&mut app, &f, &i, Action::PickSlot(None));
        assert_eq!(app.state.next_slot, None, "picking the running slot keeps it");
        tap(&mut app, &f, &i, Action::PickSlot(Some(Slot::A)));
        assert!(swipe(&mut app, &f, &i, Confirm::Slot).is_empty(), "switching is not done yet");
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
        app.term_ended("退出码 0");
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
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("不能把「sdcard」放进它自己里面"));
        assert!(paste(&mut app, FileOp::Move, "/sdcard/a.zip").is_empty());
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("「a.zip」已经在这个文件夹里"));
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
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some("这个文件夹里已经有「ab」"), "as the folder compares names");
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
            [("", "先输入名称"), ("..", "名称不能是「.」或「..」"), ("x/y", "名称里不能有「/」"), ("DCIM", "这个文件夹里已经有「DCIM」")];
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
        assert_eq!(app.titles.as_ref().map(|((title, _), _, _)| *title), Some("复制没有完成"));
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
        let rows = Rows { edges: vec![100.0, 352.0, 682.0, 934.0], action: Action::OpenEntry };
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
    fn logs_are_chosen_and_their_place_picked() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Logs, W, H, PPD);
        tap(&mut app, &f, &i, Action::LogPart(1));
        assert_eq!(app.state.logs, [true, false]);
        tap(&mut app, &f, &i, Action::LogsTo(1));
        assert_eq!(app.state.logs_to, 1);
        tap(&mut app, &f, &i, Action::Unavailable("导出日志还没实现"));
        assert_eq!(app.deadline(0), Some(TOAST_MS));
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
        assert_eq!(app.screen(), Screen::Sideload, "the stray tap did not reboot");
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
