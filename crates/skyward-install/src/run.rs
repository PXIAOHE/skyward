//! The installer: a thread that takes USB over for sideload, receives one
//! package, installs it, and hands USB back to adb, telling the screen how
//! it goes.

use crate::adb::{self, Connection, Offer};
use crate::ffs::{Endpoints, Shared, Usb};
use crate::fuse::{self, Fault, Mount};
use crate::image::{self, ImageKind};
use crate::ota::{self, Log, Status};
use crate::package::{self, Build, Package, Phone, Update};
use crate::{Command, Event, Failure, Outcome, Refusal, Stage};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use skyward_info::Charge;
use skyward_sys::{kmsg, prop};
use skyward_vab::{Slot, SnapshotState};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// What runs right before update_engine starts writing. It may write over
/// the installed system's partitions, so whatever still runs from them has
/// to move off first.
pub type Detach = Box<dyn FnOnce() -> Result<()> + Send>;

/// update_engine's log, kept for looking into an install afterwards.
const ENGINE_LOG: &str = "/tmp/update_engine.log";

const METADATA: &str = "/metadata";

/// The least charge to install on, without and with a charger, as
/// Android's recovery asks.
const BATTERY_ALONE: u8 = 20;
const BATTERY_CHARGING: u8 = 15;

/// The most often progress is told.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

pub struct Installer {
    events: Receiver<Event>,
    commands: Sender<Command>,
    /// Readable while events wait.
    wake: OwnedFd,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Installer {
    /// Take USB over and wait for `adb sideload`, from a thread of its own.
    /// `detach` runs before an update package is written.
    pub fn start(detach: Detach) -> Result<Installer> {
        let (wake, signal) = pipe(libc::O_NONBLOCK)?;
        let (event_tx, events) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let shared = Arc::new(Shared::default());
        let mut ctx = Context { events: event_tx, signal, commands: command_rx, shared: shared.clone(), detach: Some(detach) };
        let thread = thread::Builder::new()
            .name("installer".into())
            .spawn(move || {
                let outcome = run(&mut ctx);
                let _ = kmsg(&format!("install: {outcome:?}"));
                ctx.send(Event::Finished(outcome));
            })
            .context("start the installer")?;
        Ok(Installer { events, commands, wake, shared, thread: Some(thread) })
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

    /// Write the image the installer holds to `partition` in `slots`.
    pub fn flash(&self, partition: &'static str, slots: Vec<Slot>) {
        // Only a finished installer stops listening, and then there is
        // nothing left to write.
        let _ = self.commands.send(Command::Flash { partition, slots });
    }

    /// Install the update the installer holds, which erases all user data
    /// after it.
    pub fn install(&self) {
        let _ = self.commands.send(Command::Install);
    }

    /// Stop waiting for the computer, or for where an image goes.
    pub fn cancel(&self) -> Result<()> {
        let _ = self.commands.send(Command::Cancel);
        if self.shared.cancel() {
            // A read the computer never answers ends when the function goes
            // away.
            skyward_sys::set_prop("sys.usb.config", "none")?;
        }
        Ok(())
    }
}

impl Drop for Installer {
    /// The screen lets the installer go once it has finished.
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the installer does not panic");
        }
    }
}

struct Context {
    events: Sender<Event>,
    /// The write end of the wake pipe.
    signal: OwnedFd,
    commands: Receiver<Command>,
    shared: Arc<Shared>,
    detach: Option<Detach>,
}

impl Context {
    fn send(&self, event: Event) {
        if self.events.send(event).is_ok() {
            // A full pipe already wakes the screen.
            unsafe { libc::write(self.signal.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
        }
    }
}

fn run(ctx: &mut Context) -> Outcome {
    let (usb, endpoints) = match Usb::open(ctx.shared.clone()) {
        Ok(usb) => usb,
        Err(e) => return failed(e.context("set USB up for sideload")),
    };
    ctx.send(Event::Waiting);
    let mut conn = Connection::new(endpoints, banner());
    ctx.shared.accepting(true);
    let accepted = conn.accept();
    ctx.shared.accepting(false);
    let (outcome, conn) = match accepted {
        // Cancelled as the computer started sending.
        Ok(_) if ctx.shared.cancelled() => {
            finish(&mut conn);
            (Outcome::Cancelled, conn)
        }
        Ok(offer) => receive(ctx, conn, offer),
        Err(adb::Error::Interrupted) => (Outcome::Cancelled, conn),
        Err(adb::Error::OldHost) => (Outcome::Refused(Refusal::OldHost), conn),
        Err(e) => (failed(anyhow!(e).context("wait for adb sideload")), conn),
    };
    if let Err(e) = usb.close(conn.into_link()) {
        let _ = kmsg(&format!("install: USB not handed back to adb: {e:#}"));
    }
    // Cancelling while the transfer starts cuts it off.
    if ctx.shared.cancelled() && matches!(outcome, Outcome::Failed(_)) {
        return Outcome::Cancelled;
    }
    outcome
}

/// What installing came to while the package was still served.
enum Done {
    Decided(Outcome),
    /// update_engine failed with this code.
    Engine(i32),
    Error(anyhow::Error),
}

fn receive(ctx: &mut Context, mut conn: Connection<Endpoints>, offer: Offer) -> (Outcome, Connection<Endpoints>) {
    let _ = kmsg(&format!("install: {} bytes offered in blocks of {}", offer.size, offer.block_size));
    if let Err(e) = offer.check() {
        finish(&mut conn);
        return (failed(anyhow!(e)), conn);
    }
    ctx.send(Event::Checking { size: offer.size });
    let mounted = match fuse::mount(offer) {
        Ok(mounted) => mounted,
        Err(e) => {
            finish(&mut conn);
            return (failed(e), conn);
        }
    };
    let mount = mounted.serve(conn);
    let done = install(ctx, &mount, offer.size).unwrap_or_else(Done::Error);
    let (mut conn, served) = mount.unmount();
    finish(&mut conn);
    (conclude(done, served), conn)
}

/// Tell the computer the package has been read, so `adb sideload` ends.
fn finish(conn: &mut Connection<Endpoints>) {
    if let Err(e) = conn.finish() {
        let _ = kmsg(&format!("install: the computer did not take the end of the transfer: {e}"));
    }
}

/// The outcome, given how serving the package went: a package that stopped
/// being readable explains whatever failed after.
fn conclude(done: Done, served: Result<Option<Fault>>) -> Outcome {
    match (done, served) {
        (Done::Decided(outcome), served) => {
            if let Err(e) = served {
                let _ = kmsg(&format!("install: after the package was read: {e:#}"));
            }
            outcome
        }
        (_, Err(e)) => failed(e.context("serve the package")),
        (_, Ok(Some(Fault::Transfer(_)))) => Outcome::Failed(Failure::Disconnected),
        (_, Ok(Some(Fault::Changed(_)))) => Outcome::Failed(Failure::Changed),
        (Done::Engine(code), Ok(None)) => Outcome::Failed(Failure::Engine(code)),
        (Done::Error(e), Ok(None)) => failed(e),
    }
}

fn install(ctx: &mut Context, mount: &Mount<Endpoints>, size: u64) -> Result<Done> {
    let file = File::open(fuse::PACKAGE).context("open the package")?;
    let package = package::identify(BufReader::with_capacity(1 << 16, file))?;
    let _ = kmsg(&format!("install: {}", describe(&package)));
    match package {
        Package::Update(update) => update_system(ctx, mount, &update),
        Package::Image(kind) => flash_image(ctx, kind, size),
        Package::Script => Ok(Done::Decided(Outcome::Refused(Refusal::Script))),
        Package::Unknown => Ok(Done::Decided(Outcome::Refused(Refusal::Unknown))),
    }
}

fn describe(package: &Package) -> String {
    match package {
        Package::Update(u) => format!("an update to {}", u.version().unwrap_or("an unnamed version")),
        Package::Image(kind) => format!("an image of {kind:?}"),
        Package::Script => "a zip with an install script".into(),
        Package::Unknown => "a file of no known kind".into(),
    }
}

fn update_system(ctx: &mut Context, mount: &Mount<Endpoints>, update: &Update) -> Result<Done> {
    let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
    if let Err(refusal) = package::check(update, &phone(slot)?) {
        return Ok(Done::Decided(Outcome::Refused(refusal)));
    }
    if let Some(refusal) = battery()? {
        return Ok(Done::Decided(Outcome::Refused(refusal)));
    }
    let state = skyward_vab::snapshot_state()?;
    if state != SnapshotState::None {
        return Ok(Done::Decided(Outcome::Refused(Refusal::Pending(state))));
    }
    // The screen may have left the page before it heard the package came.
    if ctx.shared.cancelled() {
        return Ok(Done::Decided(Outcome::Cancelled));
    }
    let version = update.version().map(str::to_string);
    let wipe = update.wipes();
    // Erasing all user data is the user's to agree to before the system
    // they would come back to is written over.
    if wipe {
        ctx.send(Event::Wipes { version: version.clone() });
        match ctx.commands.recv() {
            Ok(Command::Install) => {}
            Ok(Command::Cancel) | Err(_) => return Ok(Done::Decided(Outcome::Cancelled)),
            Ok(other) => bail!("an update waiting to be agreed to was told {other:?}"),
        }
    }
    ctx.send(Event::Installing { version });
    let detach = ctx.detach.take().expect("an installer takes one package");
    detach().context("move off the installed system")?;
    // update_engine mounts /metadata itself, as in the stock recovery, keeps
    // the update's state there, and unmounts it when done. The recovery
    // reads the state from it read-only again afterwards.
    let metadata = Path::new(METADATA);
    if skyward_block::mount::is_mount_point(metadata)? {
        skyward_block::mount::umount(metadata)?;
    }
    let code = apply(ctx, mount, update);
    if let Err(e) = remount_metadata() {
        let _ = kmsg(&format!("install: /metadata not mounted again: {e:#}"));
    }
    Ok(match code? {
        0 => Done::Decided(Outcome::Installed { slot: slot.other(), wipe }),
        code => Done::Engine(code),
    })
}

fn remount_metadata() -> Result<()> {
    let metadata = Path::new(METADATA);
    if !skyward_block::mount::is_mount_point(metadata)? {
        skyward_block::mount::mount_ro(&Path::new(skyward_block::BY_NAME).join("metadata"), metadata)?;
    }
    Ok(())
}

/// The phone, and the system on it as its mounted partitions tell.
fn phone(slot: Slot) -> Result<Phone> {
    let props = skyward_info::installed_props(slot)?;
    let installed = match (props.get("ro.build.date.utc"), props.get("ro.build.version.security_patch")) {
        (Some(time), Some(patch)) => Some(Build {
            timestamp: time.parse().with_context(|| format!("ro.build.date.utc {time:?}"))?,
            security_patch: patch.clone(),
        }),
        _ => None,
    };
    Ok(Phone {
        device: prop("ro.product.device").context("ro.product.device is unset")?,
        serial: prop("ro.serialno").unwrap_or_default(),
        installed,
    })
}

/// Why there is too little charge to install, if there is.
fn battery() -> Result<Option<Refusal>> {
    let Some(b) = skyward_info::battery_now()? else { return Ok(None) };
    let charging = matches!(b.charge, Charge::Charging | Charge::Full | Charge::NotCharging);
    let least = if charging { BATTERY_CHARGING } else { BATTERY_ALONE };
    Ok((b.level < least).then_some(Refusal::Battery { level: b.level, charging }))
}

/// A line update_engine wrote that matters here.
enum Line {
    Status(Status),
    Log(Log),
}

/// Run update_engine on the package, telling the screen how far it is,
/// and give its exit code.
fn apply(ctx: &Context, mount: &Mount<Endpoints>, update: &Update) -> Result<i32> {
    let (status_read, status_write) = pipe(0)?;
    let status_fd = status_write.as_raw_fd();
    let mut command = std::process::Command::new(ota::PROGRAM);
    command
        .args(ota::arguments(fuse::PACKAGE, update, status_fd))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // The program keeps its end of the status pipe across exec.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(status_fd, libc::F_SETFD, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().with_context(|| format!("start {}", ota::PROGRAM))?;
    drop(status_write);
    let stderr = child.stderr.take().expect("stderr is piped");
    let mut log = File::create(ENGINE_LOG).with_context(|| format!("create {ENGINE_LOG}"))?;

    let (tx, rx) = mpsc::channel();
    let status_tx = tx.clone();
    let status = thread::spawn(move || {
        lines(File::from(status_read), |line| {
            if let Some(s) = ota::status(line) {
                let _ = status_tx.send(Line::Status(s));
            }
            Ok(())
        })
    });
    let logged = thread::spawn(move || {
        lines(stderr, |line| {
            if let Some(l) = ota::log(line) {
                let _ = tx.send(Line::Log(l));
            }
            writeln!(log, "{line}")
        })
    });

    let total = update.payload_size;
    let mut stage = Stage::Writing { done: 0, total };
    ctx.send(Event::Progress(stage.clone()));
    let mut told = Instant::now();
    let (mut partitions, mut verifying, mut finishing, mut error) = (0, None, false, None);
    loop {
        match rx.recv_timeout(PROGRESS_EVERY) {
            Ok(Line::Status(Status::Step(step))) => finishing = step == 2,
            Ok(Line::Status(Status::Error(code))) => error = Some(code),
            Ok(Line::Log(Log::Partition(_))) => partitions += 1,
            Ok(Line::Log(Log::Overwriting)) => ctx.send(Event::Overwriting),
            Ok(Line::Log(Log::Hashing { index, partition })) => verifying = Some((index, partition)),
            Err(RecvTimeoutError::Timeout) => {}
            // Both outputs closed: the program has ended.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let now = if finishing {
            Stage::Finishing
        } else if let Some((index, partition)) = &verifying {
            Stage::Verifying { index: *index, count: partitions.max(index + 1), partition: partition.clone() }
        } else {
            Stage::Writing { done: mount.served().saturating_sub(update.payload_offset).min(total), total }
        };
        let moved_on = std::mem::discriminant(&now) != std::mem::discriminant(&stage);
        if now != stage && (moved_on || told.elapsed() >= PROGRESS_EVERY) {
            stage = now;
            ctx.send(Event::Progress(stage.clone()));
            told = Instant::now();
        }
    }
    let ended = child.wait().context("wait for update_engine")?;
    status.join().expect("the status reader does not panic").context("read update_engine's status")?;
    logged.join().expect("the log reader does not panic").context("read update_engine's log")?;
    match (ended.code(), error) {
        (_, Some(code)) => Ok(code),
        (Some(code), None) => Ok(code),
        (None, None) => bail!("update_engine was ended by {ended}"),
    }
}

/// Hand each line of `from` to `each`, until the writer closes it.
fn lines(from: impl Read, mut each: impl FnMut(&str) -> io::Result<()>) -> io::Result<()> {
    let mut reader = BufReader::new(from);
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        each(String::from_utf8_lossy(&line).trim_end())?;
    }
}

fn flash_image(ctx: &Context, kind: ImageKind, size: u64) -> Result<Done> {
    let dir = Path::new(skyward_block::BY_NAME);
    let partitions = image::partitions(kind, dir)?;
    if partitions.is_empty() {
        return Ok(Done::Decided(Outcome::Refused(Refusal::NoPartition)));
    }
    ctx.send(Event::Image { kind, size, partitions: partitions.clone() });
    let (partition, slots) = match ctx.commands.recv() {
        Ok(Command::Flash { partition, slots }) => (partition, slots),
        Ok(Command::Cancel) | Err(_) => return Ok(Done::Decided(Outcome::Cancelled)),
        Ok(Command::Install) => bail!("an image waiting for its partition was told to install an update"),
    };
    ensure!(partitions.contains(&partition), "{partition} is not among {partitions:?}");
    ensure!(!slots.is_empty(), "no slot chosen for {partition}");
    let devices: Vec<_> = slots.iter().map(|&slot| image::device(dir, partition, slot)).collect();
    let mut file = BufReader::with_capacity(1 << 20, File::open(fuse::PACKAGE).context("open the package")?);
    let mut told: Option<Instant> = None;
    image::flash(&mut file, size, &devices, &mut |done, total| {
        if done == total || told.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
            ctx.send(Event::Progress(Stage::Flashing { done, total }));
            told = Some(Instant::now());
        }
    })?;
    Ok(Done::Decided(Outcome::Flashed { partition, slots }))
}

/// What the phone tells the computer it is.
fn banner() -> Vec<u8> {
    let p = |name| prop(name).unwrap_or_default();
    adb::banner(&p("ro.product.name"), &p("ro.product.model"), &p("ro.product.device"))
}

fn failed(e: anyhow::Error) -> Outcome {
    Outcome::Failed(Failure::Error(format!("{e:#}")))
}

/// A pipe, both ends closed on exec, with `flags` added.
fn pipe(flags: libc::c_int) -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | flags) } != 0 {
        return Err(io::Error::last_os_error()).context("make a pipe");
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}
