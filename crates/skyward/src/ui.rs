use crate::haptics::Haptics;
use crate::input::{monotonic_ms, Event, Inputs};
use crate::services;
use anyhow::{anyhow, ensure, Context, Result};
use skyward_config::{DeviceConfig, Service};
use skyward_info::DataState;
use skyward_install::{Detach, Installer};
use skyward_sys::Power;
use skyward_ui::app::{App, Effect, FileKind, FileRow, Keys, MountFailure, Screen, Target};
use skyward_ui::surface::kms::KmsSurface;
use skyward_ui::{Canvas, Fonts, Surface};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
        Target::Bootloader => Power::Bootloader,
        Target::Edl => Power::Edl,
        Target::PowerOff => Power::Off,
        Target::Fastboot => return Err(anyhow!("userspace fastboot is not wired up")),
    })
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
/// A user who has set no screen lock opens with no credential at all.
fn open_user_files(credential: Option<&[u8]>, users: &[u32]) -> Result<Vec<(u32, String)>> {
    let binder = skyward_binder::driver::Binder::open()?;
    let mut service = skyward_crypto::keymint::KeyMint::open(&binder)?;
    let mut closed = Vec::new();
    for &user in users {
        match skyward_crypto::data::open_user_files(&mut service, user, credential) {
            Ok(()) => skyward_sys::kmsg(&format!("the files of user {user} can be read"))?,
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

/// Wait until a device or the installer has something, or `timeout_ms`
/// passes.
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

pub fn run(cfg: &DeviceConfig, font: &Path, mono: &Path, start: Screen, dump: Option<&Path>) -> Result<()> {
    if cfg.display.density == 0 {
        return Err(anyhow!("{}: [display] density is required to lay out the UI", cfg.device.codename));
    }
    let mut info = skyward_info::collect(cfg)?;
    let fonts = Fonts::load(font, 0, mono)?;
    let mut surface = KmsSurface::open("/dev/dri/card0")?;
    let (w, h) = surface.size();
    eprintln!("display {}", surface.describe());
    let mut inputs = Inputs::open(w, h)?;
    let mut haptics = cfg.haptics.as_ref().map(Haptics::open).transpose()?;
    let mut backlight = Backlight::open()?;
    let mut app = App::new(start, w, h, cfg.display.density as f32 / 160.0);
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
    loop {
        let now = monotonic_ms();
        app.tick(now);
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
        wait(&fds, timeout)?;
        let now = monotonic_ms();
        // What the installer told comes first, so a touch acts on the page
        // as it now stands.
        if let Some(i) = &installer {
            for ev in i.events() {
                let finished = matches!(ev, skyward_install::Event::Finished(_));
                eprintln!("install {ev:?}");
                app.install(ev, now);
                if finished {
                    installer = None;
                }
            }
        }
        for ev in inputs.read()? {
            let effects = match ev {
                Event::Touch(t, at) => app.touch(t, &fonts, at),
                Event::Key(k) => app.key(k, now),
            };
            for fx in effects {
                eprintln!("effect {fx:?}");
                match fx {
                    Effect::Reboot(target) => power(target)?,
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
                    Effect::ReadDir(folder) => {
                        let files = read_folder(Path::new(&folder)).map_err(|e| format!("{e:#}"));
                        let listed = SystemTime::now().duration_since(UNIX_EPOCH).context("the clock is before 1970")?.as_secs();
                        app.set_files(folder, files, listed as i64);
                    }
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
                }
            }
        }
    }
}
