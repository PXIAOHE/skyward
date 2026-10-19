use crate::filework;
use crate::haptics::Haptics;
use crate::input::{monotonic_ms, Event, Inputs};
use crate::pty;
use crate::services;
use crate::wipe::{self, Eraser, Step};
use anyhow::{anyhow, ensure, Context, Result};
use skyward_config::{DeviceConfig, Service};
use skyward_info::DataState;
use skyward_install::{Detach, Installer};
use skyward_sys::bcb::{self, Work};
use skyward_sys::control::{Control, Request};
use skyward_sys::Power;
use skyward_ui::app::{
    App, Asked, ClashChoice, Effect, FileKind, FileOp, FileRow, FileTask, JobEnd, Keys, MountFailure, Part, Screen, Target, UsbLink,
};
use skyward_ui::surface::kms::KmsSurface;
use skyward_ui::{Canvas, Fonts, Surface};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long init may take to hand the USB port to another function: the
/// program behind the new one has to start and get ready first.
const USB_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the page shows that the work the system asked for is done
/// before the phone starts the system or powers off.
const LEAVE_AFTER_MS: u64 = 1500;

/// The panel backlight under /sys/class/backlight.
struct Backlight {
    path: PathBuf,
    /// The raw level at the top of the slider: the panel's normal maximum.
    top: u32,
    /// The raw level last written; at first the one the bootloader left.
    level: u32,
}

impl Backlight {
    fn open() -> Result<Self> {
        let mut dirs: Vec<PathBuf> = fs::read_dir("/sys/class/backlight")
            .context("read /sys/class/backlight")?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<_>>()?;
        dirs.sort();
        // Qualcomm panels expose panel0-backlight; take it when present.
        let dir = dirs
            .iter()
            .find(|d| d.file_name().is_some_and(|n| n == "panel0-backlight"))
            .or(dirs.first())
            .ok_or_else(|| anyhow!("no backlight device"))?
            .clone();
        let read = |name: &str| -> Result<u32> {
            fs::read_to_string(dir.join(name))
                .with_context(|| format!("read {}/{name}", dir.display()))?
                .trim()
                .parse()
                .with_context(|| format!("parse {name}"))
        };
        let top = normal_max(read("max_brightness")?)?;
        let level = read("brightness")?;
        Ok(Backlight { path: dir.join("brightness"), top, level })
    }

    /// Light the panel at `level` of its normal maximum, or turn it off
    /// with `None`. The file is written only when the raw level changes.
    fn show(&mut self, level: Option<f32>) -> Result<()> {
        let raw = level.map_or(0, |v| ((v.clamp(0.0, 1.0) * self.top as f32).round() as u32).max(1));
        if raw != self.level {
            fs::write(&self.path, raw.to_string()).with_context(|| format!("write {}", self.path.display()))?;
            eprintln!("backlight {raw}");
            self.level = raw;
        }
        Ok(())
    }
}

/// The raw backlight level, out of `max`, at the panel's normal maximum.
/// Above it the panel runs in high brightness mode, which the stock system
/// keeps for strong sunlight and its slider does not reach. The panel the
/// display driver booted with gives that maximum in the device tree, on a
/// finer brightness scale of its own that spans the same light as the
/// backlight's range.
fn normal_max(max: u32) -> Result<u32> {
    let param = "/sys/module/msm_drm/parameters/dsi_display0";
    let boot = fs::read_to_string(param).with_context(|| format!("read {param}"))?;
    // The panel's node, then its configuration after a colon, as in
    // "qcom,mdss_dsi_m1_42_02_0a_dsc_cmd:"; the driver takes no name
    // without the colon either.
    let (name, _) = boot.trim().split_once(':').with_context(|| format!("{param} names no panel: {boot:?}"))?;
    let soc = Path::new("/proc/device-tree/soc");
    let node = fs::read_dir(soc)
        .with_context(|| format!("read {}", soc.display()))?
        .map(|e| e.map(|e| e.path().join(name)))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .find(|p| p.is_dir())
        .ok_or_else(|| anyhow!("panel {name} is not in the device tree"))?;
    let cell = |prop: &str| -> Result<u32> {
        let path = node.join(prop);
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let cell: [u8; 4] = bytes.as_slice().try_into().map_err(|_| anyhow!("{} is not one cell", path.display()))?;
        Ok(u32::from_be_bytes(cell))
    };
    let normal = cell("mi,normal-max-brightness-clone")?;
    let full = cell("mi,max-brightness-clone")?;
    ensure!(normal > 0 && normal <= full, "panel {name}: normal maximum {normal} out of {full}");
    Ok((f64::from(max) * f64::from(normal) / f64::from(full)).round() as u32)
}

/// Leave recovery for the chosen target.
fn power(target: Target) -> Result<()> {
    skyward_sys::power(match target {
        Target::System => Power::System,
        Target::Recovery => Power::Recovery,
        Target::Fastboot => Power::Fastboot,
        Target::Bootloader => Power::Bootloader,
        Target::Edl => Power::Edl,
        Target::PowerOff => Power::Off,
    })
}

/// Hand the USB port to `function`, "adb" or "fastboot", through init:
/// through "none" first, which stops the program serving the port now,
/// then init starts the one behind the new function and sets the port up
/// once that program is ready.
fn usb(function: &str) -> Result<()> {
    for state in ["none", function] {
        skyward_sys::set_prop("sys.usb.config", state)?;
        skyward_sys::wait_prop("sys.usb.state", state, USB_TIMEOUT)?;
    }
    Ok(())
}

/// Whether the phone's fastbootd runs, as init tells it, and how far a
/// computer has the port, as the USB controller tells it.
fn fastboot_status() -> Result<(bool, UsbLink)> {
    let running = skyward_sys::prop("init.svc.fastbootd").as_deref() == Some("running");
    let controller = skyward_sys::prop("sys.usb.controller").context("init names no USB controller")?;
    let path = Path::new("/sys/class/udc").join(controller).join("state");
    let state = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let link = match state.trim() {
        "configured" => UsbLink::Connected,
        "not attached" => UsbLink::Unplugged,
        _ => UsbLink::Connecting,
    };
    Ok((running, link))
}

/// What a request from misc shows on the pages that answer it.
fn asked(request: &bcb::Request) -> Asked {
    Asked {
        reason: request.reason.clone(),
        shutdown_after: request.shutdown_after,
        unhandled: match &request.work {
            Work::Unsupported(what) => Some(what.clone()),
            _ => None,
        },
    }
}

/// Read a folder for the file manager, as it is now.
fn list(app: &mut App, folder: String) -> Result<()> {
    let files = read_folder(Path::new(&folder)).map_err(|e| format!("{e:#}"));
    let listed = SystemTime::now().duration_since(UNIX_EPOCH).context("the clock is before 1970")?.as_secs();
    app.set_files(folder, files, listed as i64);
    Ok(())
}

/// What a choice on the clash page asks of the work on files.
fn clash_rule(choice: ClashChoice) -> skyward_files::Clash {
    match choice {
        ClashChoice::Replace => skyward_files::Clash::Replace,
        ClashChoice::Skip => skyward_files::Clash::Skip,
        ClashChoice::KeepBoth => skyward_files::Clash::KeepBoth,
    }
}

/// The page's name for a part erasing goes through.
fn part(step: Step) -> Part {
    match step {
        Step::Data => Part::Data,
        Step::Cache => Part::Cache,
        Step::Metadata => Part::Metadata,
    }
}

/// Start erasing all user data, its page following it; why it cannot
/// start ends it at once, on the same page.
fn factory_reset(app: &mut App, keep_memtag_mode: bool) -> Result<Eraser> {
    match wipe::factory_reset() {
        Ok(plan) => {
            app.erase_started(true, plan.steps().into_iter().map(part).collect());
            Eraser::start(move |at| plan.carry_out(keep_memtag_mode, at))
        }
        Err(e) => {
            app.erase_started(true, Vec::new());
            Eraser::start(move |_| Err(e))
        }
    }
}

/// Start erasing the cache partition, its page following it.
fn cache_wipe(app: &mut App) -> Result<Eraser> {
    match wipe::cache() {
        Ok(Some(cache)) => {
            app.erase_started(false, vec![Part::Cache]);
            Eraser::start(move |at| {
                at(Step::Cache);
                wipe::remake(&cache)
            })
        }
        Ok(None) => {
            app.erase_started(false, Vec::new());
            Eraser::start(|_| Err(anyhow!("the phone has no cache partition to erase")))
        }
        Err(e) => {
            app.erase_started(false, Vec::new());
            Eraser::start(move |_| Err(e))
        }
    }
}

/// What the pages act on: a touch or a key, or what a program asked on the
/// recovery socket.
enum Incoming {
    Input(Event),
    Asked(Request),
}

/// What a request on the recovery socket comes to. adbd asks for fastboot
/// while the slot's partitions are up, which fastbootd cannot work with, so
/// the phone restarts into fastboot as from the reboot page; fastbootd asks
/// for the recovery it runs in. A request that finds the recovery already
/// where it asks to be, as when the keys left fastboot first, needs nothing.
fn answer(request: Request, fastboot: bool) -> Vec<Effect> {
    match (request, fastboot) {
        (Request::Fastboot, false) => vec![Effect::Reboot(Target::Fastboot)],
        (Request::Recovery, true) => vec![Effect::EnterRecovery],
        _ => Vec::new(),
    }
}

/// The time of day in China for the status bar: the recovery carries no
/// time zone files, and China's is the time zone the settings offer.
fn clock() -> Result<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).context("the clock is before 1970")?;
    Ok(skyward_ui::fmt::clock(now.as_secs() as i64))
}

/// The recovery does not mount /data itself yet, so a mounted /data was
/// mounted (and unlocked) by Android.
fn data_state() -> Result<DataState> {
    let here = fs::metadata("/data").context("stat /data")?;
    let up = fs::metadata("/").context("stat /")?;
    Ok(if here.dev() != up.dev() { DataState::Decrypted } else { DataState::Unmounted })
}

/// Open the data partition and let in every key the phone keeps for it
/// that needs nothing a user knows, and give the users the phone keeps
/// keys for.
fn open_data() -> Result<Vec<u32>> {
    use skyward_crypto::data;
    let binder = skyward_binder::driver::Binder::open()?;
    let service = skyward_crypto::keymint::KeyMint::open(&binder)?;
    if !skyward_block::mount::is_mount_point(Path::new(data::MOUNT))? {
        let (mapped, filesystem) = data::open(&service)?;
        skyward_sys::kmsg(&format!("{} holds {filesystem} at {}", mapped.display(), data::MOUNT))?;
    }
    data::open_shared(&service)?;
    data::open_users(&service)
}

/// Let in the keys for the own files of every one of `users` that
/// `credential` opens, and give the ones whose files stay closed and why.
/// A user who has set no screen lock opens with no credential at all, and
/// one the phone made a credential up for opens behind the owner's, so the
/// owner is taken first and the note their credential gave is kept for the
/// rest.
fn open_user_files(credential: Option<&[u8]>, users: &[u32]) -> Result<Vec<(u32, String)>> {
    let binder = skyward_binder::driver::Binder::open()?;
    let service = skyward_crypto::keymint::KeyMint::open(&binder)?;
    let mut owner = None;
    let mut closed = Vec::new();
    for &user in users {
        match skyward_crypto::data::open_user_files(&service, user, credential, owner.as_ref()) {
            Ok(note) => {
                skyward_sys::kmsg(&format!("the files of user {user} can be read"))?;
                owner = owner.or(note);
            }
            Err(e) => {
                skyward_sys::kmsg(&format!("the files of user {user} stay closed: {e:#}"))?;
                closed.push((user, format!("{e:#}")));
            }
        }
    }
    Ok(closed)
}

/// Which control answers the screen lock the first of these users set.
fn keys_for(users: &[u32]) -> Result<Keys> {
    use skyward_crypto::user::Lock;
    let Some(&user) = users.first() else { return Ok(Keys::None) };
    Ok(match skyward_crypto::data::protector(user)?.lock.map(|l| l.kind) {
        Some(Lock::PATTERN) => Keys::Pattern,
        Some(Lock::PIN) => Keys::Digits,
        _ => Keys::Letters,
    })
}

/// What the file manager lists for a folder: its entries, folders and
/// links to folders first, then by name ignoring case, each folder with how
/// many entries it holds. An entry gone by the time it is looked at, as
/// processes in /proc go, is left out.
fn read_folder(path: &Path) -> Result<Vec<FileRow>> {
    let gone = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let mut rows = Vec::new();
    for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
        let full = entry.with_context(|| format!("read {}", path.display()))?.path();
        let meta = match fs::symlink_metadata(&full) {
            Ok(meta) => meta,
            Err(e) if gone(&e) => continue,
            Err(e) => return Err(e).with_context(|| format!("stat {}", full.display())),
        };
        let kind = if meta.file_type().is_symlink() {
            let target = fs::read_link(&full).with_context(|| format!("read the link {}", full.display()))?;
            // A link to nothing leads to no folder.
            let folder = match fs::metadata(&full) {
                Ok(to) => to.is_dir(),
                Err(e) if gone(&e) => false,
                Err(e) => return Err(e).with_context(|| format!("follow {}", full.display())),
            };
            FileKind::Link { target: target.to_string_lossy().into_owned(), folder }
        } else if meta.is_dir() {
            // A folder that may not be read shows that in place of a count.
            let items = match fs::read_dir(&full) {
                Ok(inside) => Some(inside.count()),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => None,
                Err(e) if gone(&e) => continue,
                Err(e) => return Err(e).with_context(|| format!("read {}", full.display())),
            };
            FileKind::Folder { items }
        } else if meta.is_file() {
            FileKind::File
        } else {
            FileKind::Special
        };
        let name = full.file_name().with_context(|| format!("{} has no name", full.display()))?.to_string_lossy().into_owned();
        rows.push(FileRow { name, kind, size: meta.len(), modified: meta.mtime(), mode: meta.mode(), uid: meta.uid(), gid: meta.gid() });
    }
    rows.sort_by_key(|r| (!matches!(r.kind, FileKind::Folder { .. } | FileKind::Link { folder: true, .. }), r.name.to_lowercase()));
    Ok(rows)
}

/// The slot's logical partitions and how each shows now, for the mount page.
/// A mounted partition is in use when a program holds it, or when one of
/// `services` runs from it: those are started again from there whenever
/// they end.
fn read_mounts(services: &[Service]) -> Result<Vec<skyward_ui::app::MountRow>> {
    use skyward_ui::app::MountRow;
    let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
    let shown = skyward_vab::mounted_partitions(slot)?;
    let holders = skyward_block::mount::holders()?;
    let mut rows = Vec::new();
    for base in skyward_vab::slot_bases(slot)? {
        let point = skyward_vab::mount_point(&base);
        let name = format!("{base}{}", slot.suffix());
        let mounted = shown.iter().find(|m| m.partition == name);
        let mut using = BTreeSet::new();
        if let Some(m) = mounted {
            using.extend(holders.get(&skyward_block::devnum(&m.device)?).into_iter().flatten().cloned());
            for s in services.iter().filter(|s| s.uses(&point)) {
                let program = Path::new(&s.path).file_name().with_context(|| format!("{} names no program", s.path))?;
                using.insert(program.to_string_lossy().into_owned());
            }
        }
        rows.push(MountRow {
            base,
            point: point.display().to_string(),
            mounted: mounted.is_some(),
            holders: using.into_iter().collect(),
        });
    }
    Ok(rows)
}

/// What has to happen before an update writes over the installed system's
/// partitions: the files the phone's own programs run from stay in memory,
/// and the booted slot's partitions come off.
fn detach(services: Vec<Service>) -> Detach {
    Box::new(move || {
        let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
        let mounted = skyward_vab::mounted_partitions(slot)?;
        let busy = services::pin(&services, &mounted)?;
        skyward_vab::release_partitions(slot, &busy)
    })
}

/// Wait until a device, the installer or the recovery socket has something,
/// or `timeout_ms` passes.
fn wait(fds: &[i32], timeout_ms: i32) -> Result<()> {
    let mut polls: Vec<libc::pollfd> = fds.iter().map(|&fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
    let n = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout_ms) };
    if n < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err).context("poll input devices");
        }
    }
    Ok(())
}

/// Run the pages. What `request`, read from misc, asks for decides the
/// first of them; asking for nothing, they start at `start`.
pub fn run(cfg: &DeviceConfig, font: &Path, mono: &Path, request: bcb::Request, start: Screen, dump: Option<&Path>) -> Result<()> {
    if cfg.display.density == 0 {
        return Err(anyhow!("{}: [display] density is required to lay out the UI", cfg.device.codename));
    }
    let mut info = skyward_info::collect(cfg)?;
    let fonts = Fonts::load(font, 0, mono)?;
    let mut surface = KmsSurface::open("/dev/dri/card0")?;
    let (w, h) = surface.size();
    eprintln!("display {}", surface.describe());
    let mut inputs = Inputs::open(w, h)?;
    // adbd and fastbootd ask over it to go over to fastboot and back.
    let control = Control::open()?;
    let mut haptics = cfg.haptics.as_ref().map(Haptics::open).transpose()?;
    let mut backlight = Backlight::open()?;
    let first = match &request.work {
        Work::Fastboot => Screen::Fastboot,
        Work::PromptWipeData => Screen::CantStart,
        Work::Unsupported(_) => Screen::Unhandled,
        Work::Menu | Work::Exit | Work::WipeData | Work::WipeCache | Work::Sideload { .. } => start,
    };
    let mut app = App::new(first, w, h, cfg.display.density as f32 / 160.0);
    app.set_asked(asked(&request));
    // Keep the level the boot splash had, so the panel does not jump, or
    // the slider's top when it was brighter; a dark panel gets the
    // slider's default.
    if backlight.level > 0 {
        app.state.brightness = (backlight.level as f32 / backlight.top as f32).min(1.0);
    }
    let mut data = data_state()?;
    // Which users' own files are still closed, once the partition itself
    // is open. Each is closed with what that user types.
    let mut locked: Vec<u32> = Vec::new();
    // The mount page shows the partitions as they are when it opens and
    // after each change; why they cannot be read is the page's to say.
    let mounts = || read_mounts(&cfg.services).map_err(|e| format!("{e:#}"));

    let mut screen = Canvas::new(w, h);
    let mut shown_clock = String::new();
    // The readings are taken again every 30 seconds, the battery every
    // second, so plugging the cable in or pulling it shows at once.
    let mut read_at = Instant::now();
    let mut battery_at = read_at;
    let mut installer: Option<Installer> = None;
    // Whether the panel's frame memory has been blacked out for an off
    // panel; cleared once a real frame is shown again.
    let mut blanked = false;
    // Started for fastboot, the port goes to the phone's own fastbootd. The
    // page says whether it came up and how far a computer has the port;
    // why it did not come up goes to the kernel log and on screen.
    let mut fastboot = first == Screen::Fastboot;
    let mut port_at = Instant::now();
    if fastboot {
        if let Err(e) = usb("fastboot") {
            skyward_sys::kmsg(&format!("fastbootd has not taken the port: {e:#}"))?;
            app.report(&format!("{e:#}"), monotonic_ms());
        }
        let (running, link) = fastboot_status()?;
        app.set_fastboot(running, link);
    }
    // Work the request asks for that goes on keeps the request in misc,
    // so a restart halfway comes back to it, until it is done or the user
    // leaves it for somewhere else.
    let misc = bcb::misc();
    let mut kept = request.work.lasts();
    let mut eraser: Option<Eraser> = None;
    match &request.work {
        Work::WipeData => eraser = Some(factory_reset(&mut app, request.keep_memtag_mode)?),
        Work::WipeCache => eraser = Some(cache_wipe(&mut app)?),
        Work::Sideload { .. } => {
            app.start_sideload();
            installer = Some(Installer::start(detach(cfg.services.clone()))?);
        }
        Work::Menu | Work::Exit | Work::Fastboot | Work::PromptWipeData | Work::Unsupported(_) => {}
    }
    // Once the work the system asked for is done, the page shows it for a
    // moment; then the phone starts the system, or powers off where the
    // request says to.
    let leave = if request.shutdown_after { Target::PowerOff } else { Target::System };
    let mut leave_at: Option<u64> = None;
    // The terminal page's shell, from when the page first asks for one.
    let mut shell: Option<pty::Shell> = None;
    // Copying, moving or deleting under way.
    let mut worker: Option<filework::Worker> = None;
    loop {
        let now = monotonic_ms();
        if leave_at.is_some_and(|at| now >= at) {
            leave_at = None;
            power(leave)?;
        }
        for fx in app.tick(now) {
            match fx {
                Effect::Haptic => {
                    if let Some(h) = &mut haptics {
                        h.click()?;
                    }
                }
                other => unreachable!("time passing asks for nothing but a tap of the motor, not {other:?}"),
            }
        }
        let time = clock()?;
        if time != shown_clock {
            shown_clock = time;
            app.redraw();
        }
        if read_at.elapsed().as_secs() >= 30 {
            info = skyward_info::collect(cfg)?;
            read_at = Instant::now();
            battery_at = read_at;
            app.refresh();
        } else if battery_at.elapsed().as_secs() >= 1 {
            let battery = skyward_info::battery_now()?;
            battery_at = Instant::now();
            if battery != info.battery {
                info.battery = battery;
                app.refresh();
            }
        }
        // In fastboot the page follows fastbootd and the cable each second.
        if fastboot && port_at.elapsed().as_secs() >= 1 {
            let (running, link) = fastboot_status()?;
            app.set_fastboot(running, link);
            port_at = Instant::now();
        }
        let panel = app.panel(now);
        let mut shown = false;
        if panel.is_some() {
            if app.take_dirty() {
                let t0 = Instant::now();
                app.compose(&mut screen, &fonts, &info, data, &shown_clock, now);
                surface.present(&screen)?;
                eprintln!("frame {:?} in {} ms", app.screen(), t0.elapsed().as_millis());
                if let Some(path) = dump {
                    fs::write(path, &screen.data).with_context(|| format!("write {}", path.display()))?;
                }
                shown = true;
            }
            blanked = false;
        } else if !blanked {
            // Black out the panel's frame memory before the backlight goes
            // to 0, so an OLED that floors the backlight above zero rather
            // than cutting it shows nothing and cannot burn in.
            screen.data.fill(0);
            surface.present(&screen)?;
            blanked = true;
        }
        // After the frame, so a panel coming back on lights with what it
        // should show now, never with what it showed when it went off.
        backlight.show(panel)?;
        // A frame shown has waited for the panel, which keeps the loop to
        // its pace; otherwise the CPU gets at least a millisecond.
        let floor = if shown { 0 } else { 1 };
        let until_due = app.deadline(now).map(|d| d.saturating_sub(now) as i32);
        let timeout = until_due.map_or(1000, |t| t.clamp(floor, 1000));
        let mut fds = inputs.fds();
        if let Some(i) = &installer {
            fds.push(i.fd());
        }
        if let Some(c) = &control {
            fds.push(c.fd());
        }
        if let Some(e) = &eraser {
            fds.push(e.fd());
        }
        if let Some(sh) = &shell {
            fds.extend(sh.fds());
        }
        if let Some(w) = &worker {
            fds.push(w.fd());
        }
        wait(&fds, timeout)?;
        let now = monotonic_ms();
        // What the installer and the eraser told comes first, so a touch
        // acts on the page as it now stands.
        if let Some(i) = &installer {
            for ev in i.events() {
                let finished = matches!(ev, skyward_install::Event::Finished(_));
                eprintln!("install {ev:?}");
                app.install(ev, now);
                if finished {
                    installer = None;
                    // A sideload the system asked for is done with however
                    // it went; asked to, the phone then leaves on its own.
                    if let Work::Sideload { auto_reboot } = request.work {
                        if kept {
                            bcb::clear(&misc)?;
                            kept = false;
                        }
                        if auto_reboot {
                            leave_at = Some(now + LEAVE_AFTER_MS);
                        }
                    }
                }
            }
        }
        let mut erased = false;
        if let Some(e) = &eraser {
            for ev in e.events() {
                match ev {
                    wipe::Event::At(step) => app.erase_at(part(step)),
                    wipe::Event::Ended(outcome) => {
                        skyward_sys::kmsg(&format!("erasing ended: {outcome:?}"))?;
                        // As with the phone's own recovery, the request is
                        // done with either way, so a restart starts the
                        // system rather than erasing again.
                        if kept {
                            bcb::clear(&misc)?;
                            kept = false;
                        }
                        if outcome.is_ok() {
                            leave_at = Some(now + LEAVE_AFTER_MS);
                        }
                        app.erase_ended(outcome, now);
                        erased = true;
                    }
                }
            }
        }
        if erased {
            eraser = None;
        }
        let mut worked = false;
        if let Some(w) = &worker {
            for ev in w.events() {
                match ev {
                    filework::Event::Progress(p) => app.file_progress((p.bytes, p.total_bytes), (p.entries, p.total_entries)),
                    filework::Event::Ended(end) => {
                        let end = match end {
                            filework::End::Done => JobEnd::Done,
                            filework::End::Stopped => JobEnd::Stopped,
                            filework::End::Failed(why) => JobEnd::Failed(why),
                        };
                        app.file_job_ended(end, now);
                        worked = true;
                    }
                }
            }
        }
        // The folder shown is read again for what the work changed.
        if worked {
            worker = None;
            let folder = app.state.folder.clone();
            list(&mut app, folder)?;
        }
        if let Some(sh) = &mut shell {
            let wrote = sh.read()?;
            if !wrote.is_empty() {
                app.term_output(&wrote);
            }
            if let Some(status) = sh.ended()? {
                // What it wrote last shows before its end does.
                let last = sh.read()?;
                if !last.is_empty() {
                    app.term_output(&last);
                }
                app.term_ended(&pty::how(status));
                shell = None;
            }
        }
        // Then what programs asked on the recovery socket, each answered as
        // the recovery stands when its turn comes.
        let requests = match &control {
            Some(c) => c.requests()?,
            None => Vec::new(),
        };
        let incoming = requests.into_iter().map(Incoming::Asked).chain(inputs.read()?.into_iter().map(Incoming::Input));
        for came in incoming {
            let effects = match came {
                Incoming::Asked(asked) => {
                    skyward_sys::kmsg(&format!("asked on the recovery socket: {asked:?}"))?;
                    answer(asked, fastboot)
                }
                Incoming::Input(Event::Touch(t, at)) => app.touch(t, &fonts, at),
                Incoming::Input(Event::Key(k)) => app.key(k, now),
            };
            for fx in effects {
                eprintln!("effect {fx:?}");
                match fx {
                    // Leaving is the user's answer to whatever the request
                    // still kept in misc asked.
                    Effect::Reboot(target) => {
                        if kept {
                            bcb::clear(&misc)?;
                            kept = false;
                        }
                        power(target)?
                    }
                    Effect::Dismiss => {
                        if kept {
                            bcb::clear(&misc)?;
                            kept = false;
                        }
                    }
                    Effect::FactoryReset => {
                        ensure!(eraser.is_none(), "erasing is under way already");
                        eraser = Some(factory_reset(&mut app, request.keep_memtag_mode)?);
                    }
                    // A shell that will not start, or will not take keys,
                    // is the page's to report; the recovery carries on.
                    Effect::StartShell => {
                        ensure!(shell.is_none(), "a shell runs already");
                        let (rows, cols) = app.term_size(&fonts);
                        match pty::Shell::start(rows, cols, &cfg.device.codename) {
                            Ok(sh) => {
                                shell = Some(sh);
                                app.term_started(rows, cols);
                            }
                            Err(e) => {
                                skyward_sys::kmsg(&format!("no shell for the terminal: {e:#}"))?;
                                app.report(&format!("{e:#}"), now);
                            }
                        }
                    }
                    Effect::TermInput(bytes) => {
                        let sh = shell.as_mut().context("keys went to a shell that has ended")?;
                        if let Err(e) = sh.write(&bytes) {
                            app.report(&format!("{e:#}"), now);
                        }
                    }
                    Effect::Haptic => {
                        if let Some(h) = &mut haptics {
                            h.click()?;
                        }
                    }
                    Effect::Sideload => {
                        ensure!(installer.is_none(), "an installer is running already");
                        installer = Some(Installer::start(detach(cfg.services.clone()))?);
                    }
                    Effect::Flash { partition, pick } => {
                        let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
                        installer.as_ref().context("no installer holds an image")?.flash(partition, pick.slots(slot));
                    }
                    Effect::Cancel => installer.as_ref().context("no installer to cancel")?.cancel()?,
                    Effect::ReadMounts => app.set_mounts(mounts()),
                    Effect::Decrypt => match open_data().and_then(|users| open_user_files(None, &users)) {
                        Ok(closed) => {
                            locked = closed.iter().map(|(user, _)| *user).collect();
                            data = if locked.is_empty() { DataState::Decrypted } else { DataState::Locked };
                            app.set_locked(locked.clone(), keys_for(&locked)?);
                            app.refresh();
                            skyward_sys::kmsg(&format!("the data partition is open, with users {locked:?} still closed"))?;
                        }
                        Err(e) => {
                            skyward_sys::kmsg(&format!("the data partition stays closed: {e:#}"))?;
                            app.report(&format!("{e:#}"), now);
                        }
                    },
                    Effect::Unlock(code) => match open_user_files(Some(code.as_bytes()), &locked) {
                        Ok(closed) => {
                            locked = closed.iter().map(|(user, _)| *user).collect();
                            data = if locked.is_empty() { DataState::Decrypted } else { DataState::Locked };
                            app.set_locked(locked.clone(), keys_for(&locked)?);
                            app.refresh();
                            if let Some((_, why)) = closed.first() {
                                app.report(why, now);
                            }
                        }
                        Err(e) => {
                            skyward_sys::kmsg(&format!("the files of the phone's users stay closed: {e:#}"))?;
                            app.report(&format!("{e:#}"), now);
                        }
                    },
                    Effect::ReadDir(folder) => list(&mut app, folder)?,
                    // A name taken where an entry goes is the user's to
                    // decide on before anything is written.
                    Effect::Paste { op, path, into, clash } => {
                        ensure!(worker.is_none(), "work on files is under way already");
                        let clash = match clash {
                            Some(choice) => clash_rule(choice),
                            None => match skyward_files::clashes(&[PathBuf::from(&path)], Path::new(&into)) {
                                Ok(taken) if taken.is_empty() => skyward_files::Clash::Refuse,
                                Ok(taken) => {
                                    let names = taken.iter().map(|t| t.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())).collect();
                                    app.file_clash(op, path, into, names);
                                    continue;
                                }
                                Err(e) => {
                                    app.report(&format!("{e:#}"), now);
                                    continue;
                                }
                            },
                        };
                        let (from, to) = (PathBuf::from(&path), PathBuf::from(&into));
                        let (task, shown) = match op {
                            FileOp::Copy => (filework::Task::Copy { from, into: to, clash }, FileTask::Copy { path, into }),
                            FileOp::Move => (filework::Task::Move { from, into: to, clash }, FileTask::Move { path, into }),
                        };
                        worker = Some(filework::Worker::start(task)?);
                        app.file_job_started(shown);
                    }
                    Effect::Delete(path) => {
                        ensure!(worker.is_none(), "work on files is under way already");
                        worker = Some(filework::Worker::start(filework::Task::Delete { path: PathBuf::from(&path) })?);
                        app.file_job_started(FileTask::Delete { path });
                    }
                    Effect::StopJob => worker.as_ref().context("there is no work on files to stop")?.stop(),
                    Effect::Rename { path, name } => match skyward_files::rename(Path::new(&path), &name) {
                        Ok(_) => {
                            app.renamed(Ok(()), now);
                            let folder = app.state.folder.clone();
                            list(&mut app, folder)?;
                        }
                        Err(e) => app.renamed(Err(format!("{e:#}")), now),
                    },
                    // A partition that will not mount, or is in use, is the
                    // page's to report; the recovery carries on.
                    Effect::Mount(base) => {
                        let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
                        if let Err(e) = skyward_vab::mount_one(slot, &base) {
                            app.mount_failed(MountFailure { mounting: true, base, detail: format!("{e:#}") });
                        }
                        app.set_mounts(mounts());
                    }
                    Effect::Unmount(base) => {
                        let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
                        if let Err(e) = skyward_vab::unmount_one(slot, &base) {
                            app.mount_failed(MountFailure { mounting: false, base, detail: format!("{e:#}") });
                        }
                        app.set_mounts(mounts());
                    }
                    // init stops fastbootd as the port goes through "none";
                    // then the slot's partitions come up as at boot.
                    Effect::EnterRecovery => {
                        usb("adb")?;
                        let slot = skyward_vab::current_slot()?.context("not an A/B phone")?;
                        crate::prepare(cfg, slot)?;
                        info = skyward_info::collect(cfg)?;
                        data = data_state()?;
                        fastboot = false;
                        app.enter_recovery();
                    }
                }
            }
        }
    }
}
