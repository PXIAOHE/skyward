use crate::input::{monotonic_ms, Event, Inputs};
use anyhow::{anyhow, Context, Result};
use skyward_config::DeviceConfig;
use skyward_info::DataState;
use skyward_sys::Power;
use skyward_ui::app::{App, Effect, Screen, Target};
use skyward_ui::surface::kms::KmsSurface;
use skyward_ui::{Canvas, Fonts, Surface};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The panel backlight under /sys/class/backlight.
struct Backlight {
    path: PathBuf,
    max: u32,
    /// The level found at startup, as the bootloader left it.
    start: u32,
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
        let max = read("max_brightness")?;
        let start = read("brightness")?;
        Ok(Backlight { path: dir.join("brightness"), max, start })
    }

    fn set(&self, level: f32) -> Result<()> {
        let v = ((level.clamp(0.0, 1.0) * self.max as f32).round() as u32).max(1);
        fs::write(&self.path, v.to_string()).with_context(|| format!("write {}", self.path.display()))
    }

    fn off(&self) -> Result<()> {
        fs::write(&self.path, "0").with_context(|| format!("write {}", self.path.display()))
    }
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

/// Local wall-clock time as `HH:MM`.
fn clock() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// The recovery does not mount /data itself yet, so a mounted /data was
/// mounted (and unlocked) by Android.
fn data_state() -> Result<DataState> {
    let here = fs::metadata("/data").context("stat /data")?;
    let up = fs::metadata("/").context("stat /")?;
    Ok(if here.dev() != up.dev() { DataState::Decrypted } else { DataState::Unmounted })
}

/// Wait until a device has input or `timeout_ms` passes.
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

pub fn run(cfg: &DeviceConfig, font: &Path, start: Screen, dump: Option<&Path>) -> Result<()> {
    if cfg.display.density == 0 {
        return Err(anyhow!("{}: [display] density is required to lay out the UI", cfg.device.codename));
    }
    let mut info = skyward_info::collect(cfg)?;
    let fonts = Fonts::load(font, 0)?;
    let mut surface = KmsSurface::open("/dev/dri/card0")?;
    let (w, h) = surface.size();
    eprintln!("display {}", surface.describe());
    let mut inputs = Inputs::open(w, h)?;
    let backlight = Backlight::open()?;
    let mut app = App::new(start, w, h, cfg.display.density as f32 / 160.0);
    // Keep the level the boot splash had, so the panel does not jump; a
    // dark panel gets the slider's default.
    if backlight.start == 0 {
        backlight.set(app.state.brightness)?;
    } else {
        app.state.brightness = backlight.start as f32 / backlight.max as f32;
    }
    let data = data_state()?;

    let mut screen = Canvas::new(w, h);
    let mut shown_clock = String::new();
    let mut read_at = Instant::now();
    loop {
        let now = monotonic_ms();
        app.tick(now);
        let time = clock();
        if time != shown_clock {
            shown_clock = time;
            app.redraw();
        }
        if read_at.elapsed().as_secs() >= 30 {
            info = skyward_info::collect(cfg)?;
            read_at = Instant::now();
            app.refresh();
        }
        if app.take_dirty() && app.display_on() {
            let t0 = Instant::now();
            app.compose(&mut screen, &fonts, &info, data, &shown_clock, now);
            surface.present(&screen)?;
            eprintln!("frame {:?} in {} ms", app.screen(), t0.elapsed().as_millis());
            if let Some(path) = dump {
                fs::write(path, &screen.data).with_context(|| format!("write {}", path.display()))?;
            }
        }
        let until_due = app.deadline().map(|d| d.saturating_sub(now) as i32);
        let timeout = until_due.map_or(1000, |t| t.clamp(1, 1000));
        wait(&inputs.fds(), timeout)?;
        let now = monotonic_ms();
        for ev in inputs.read()? {
            let effects = match ev {
                Event::Touch(t, at) => app.touch(t, &fonts, at),
                Event::Key(k) => app.key(k, now),
            };
            for fx in effects {
                eprintln!("effect {fx:?}");
                match fx {
                    Effect::Backlight(v) => backlight.set(v)?,
                    Effect::Display(true) => backlight.set(app.state.brightness)?,
                    Effect::Display(false) => backlight.off()?,
                    Effect::Reboot(target) => power(target)?,
                }
            }
        }
    }
}
