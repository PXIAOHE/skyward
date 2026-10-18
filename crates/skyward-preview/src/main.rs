use anyhow::{anyhow, Result};
use skyward_config::DeviceConfig;
use skyward_info::{Battery, Boot, Charge, Cpu, DataState, DeviceInfo, PatchLevel, Storage};
use skyward_install::{Failure, ImageKind, Outcome, Ramdisk, Refusal, Stage};
use skyward_ui::app::{Action, App, Choice, Damage, Install, MountFailure, MountRow, Pick, Screen, Touch, SCREEN_OFF};
use skyward_ui::widgets::{self, Bar};
use skyward_ui::{Canvas, Fonts, Theme};
use skyward_vab::{Slot, SnapshotState};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

mod matcher;

const CLOCK: &str = "23:46";

/// File name, page, dark theme, readings, userdata state, pattern drawn.
type Shot<'a> = (&'a str, Screen, bool, &'a DeviceInfo, DataState, &'a [usize]);

/// File name, dark theme, readings, a mount that failed, and the partitions
/// left unmounted.
type MountShot<'a> = (&'a str, bool, &'a DeviceInfo, Option<MountFailure>, &'a [&'a str]);

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("preview-out"));
    std::fs::create_dir_all(&out)?;

    let font_path = font_path()?;
    let fonts = Fonts::load(&font_path, 0)?;
    eprintln!("font: {}", font_path.display());
    for (tag, min, def, max) in fonts.axes() {
        eprintln!("axis {tag}: {min}..{max} (default {def})");
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = DeviceConfig::load(root.join("devices/nezha.toml"))?;
    let (w, h) = (cfg.display.width, cfg.display.height);
    let ppd = cfg.display.density as f32 / 160.0;
    let light = Theme::light(w, h, ppd);

    match args.get(2).map(String::as_str) {
        Some("--calibrate") => {
            let path = out.join("calibrate.png");
            write_png(&path, &calibrate(&fonts, &light)?)?;
            println!("wrote {}", path.display());
            return Ok(());
        }
        Some("--match") => {
            let probes = args.get(3).ok_or_else(|| anyhow!("--match needs a probe file"))?;
            return matcher::run(&fonts, Path::new(probes));
        }
        _ => {}
    }

    let info = nezha(&cfg);
    let pending = DeviceInfo { snapshot: SnapshotState::Snapshotted, ..info.clone() };
    let open = DataState::Decrypted;
    // Whole pages, the way a long screenshot shows them. The About page's
    // field is drawn only on screen, so it appears among the moments below.
    let shots: [Shot; 12] = [
        ("01-decrypt-dark", Screen::Decrypt, true, &info, DataState::Locked, &[0, 3, 4, 5, 8]),
        ("02-home-light", Screen::Home, false, &info, open, &[]),
        ("03-home-dark-update-pending", Screen::Home, true, &pending, open, &[]),
        ("04-settings-light", Screen::Settings, false, &info, open, &[]),
        ("05-settings-dark", Screen::Settings, true, &info, open, &[]),
        ("06-reboot-dark", Screen::Reboot, true, &info, open, &[]),
        ("07-wipe-light", Screen::Wipe, false, &info, open, &[]),
        ("09-specs-light", Screen::Specs, false, &info, open, &[]),
        ("10-specs-dark", Screen::Specs, true, &info, open, &[]),
        ("11-install-light", Screen::Install, false, &info, open, &[]),
        ("12-install-dark-update-pending", Screen::Install, true, &pending, open, &[]),
        ("13-screen-off-light", Screen::ScreenOff, false, &info, open, &[]),
    ];
    for (name, screen, dark, info, data, pattern) in shots {
        let mut app = App::new(screen, w, h, ppd);
        app.state.dark = dark;
        app.state.pattern = pattern.to_vec();
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, info, data))?;
        println!("wrote {}", path.display());
    }

    // The screen kept on, which warns about burn-in.
    let mut app = App::new(Screen::ScreenOff, w, h, ppd);
    app.state.dark = true;
    app.state.screen_off = SCREEN_OFF.iter().position(Option::is_none).expect("one choice keeps the screen on");
    let path = out.join("14-screen-off-never-dark.png");
    write_png(&path, &whole_page(&mut app, &fonts, &info, open))?;
    println!("wrote {}", path.display());

    // The sideload page through an install, as the installer tells it.
    let recovery = ImageKind::Boot { kernel: false, ramdisk: Ramdisk::Recovery };
    let installing: [(&str, bool, Install); 11] = [
        ("30-sideload-waiting", false, Install::Waiting),
        ("31-sideload-checking", false, Install::Checking { size: 7_103_043_868 }),
        (
            "32-sideload-choosing",
            false,
            Install::Choosing(Choice { kind: recovery, size: 43_257_856, partitions: vec!["recovery"], partition: 0, pick: Pick::Current }),
        ),
        (
            "33-sideload-writing-dark",
            true,
            Install::Updating {
                version: Some("OS4.0.5.0.XPACNXM".into()),
                stage: Some(Stage::Writing { done: 2_934_000_000, total: 7_103_041_280 }),
                overwriting: true,
            },
        ),
        (
            "34-sideload-verifying",
            false,
            Install::Updating {
                version: Some("OS4.0.5.0.XPACNXM".into()),
                stage: Some(Stage::Verifying { index: 6, count: 21, partition: "system".into() }),
                overwriting: false,
            },
        ),
        ("35-sideload-flashing", false, Install::Flashing { partition: "boot", stage: Some(Stage::Flashing { done: 300, total: 800 }) }),
        ("36-sideload-installed", false, Install::Finished { outcome: Outcome::Installed { slot: Slot::A }, damage: None }),
        (
            "37-sideload-flashed-recovery-dark",
            true,
            Install::Finished { outcome: Outcome::Flashed { partition: "recovery", slots: vec![Slot::A, Slot::B] }, damage: None },
        ),
        (
            "38-sideload-refused",
            false,
            Install::Finished { outcome: Outcome::Refused(Refusal::OtherDevice("ishtar".into())), damage: None },
        ),
        (
            "39-sideload-failed",
            false,
            Install::Finished { outcome: Outcome::Failed(Failure::Engine(9)), damage: Some(Damage::System) },
        ),
        (
            "40-sideload-error-dark",
            true,
            Install::Finished {
                outcome: Outcome::Failed(Failure::Error("set USB up for sideload: open ep0: Device or resource busy (os error 16)".into())),
                damage: None,
            },
        ),
    ];
    for (name, dark, install) in installing {
        let mut app = App::new(Screen::Sideload, w, h, ppd);
        app.state.dark = dark;
        app.state.install = install;
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, &info, open))?;
        println!("wrote {}", path.display());
    }

    // The mount page as it opens after boot, after an unmount that failed,
    // and while an update waits to merge, with some partitions unmounted.
    let busy = MountFailure {
        mounting: false,
        base: "vendor".into(),
        detail: "unmount /vendor: Device or resource busy (os error 16)".into(),
    };
    let mounting: [MountShot; 3] = [
        ("15-mount-light", false, &info, None, &[]),
        ("16-mount-dark-unmount-failed", true, &info, Some(busy), &[]),
        ("17-mount-light-update-pending", false, &pending, None, &["product", "system_ext"]),
    ];
    for (name, dark, info, failure, unmounted) in mounting {
        let mut app = App::new(Screen::Mount, w, h, ppd);
        app.state.dark = dark;
        app.set_mounts(Ok(sample_mounts(unmounted)));
        if let Some(failure) = failure {
            app.mount_failed(failure);
        }
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, info, open))?;
        println!("wrote {}", path.display());
    }

    // The panel mid-gesture, to set beside recordings of the same moves.
    let moments: [Moment; 12] = [
        ("08-about-light", Screen::About, false, &|_, _| 2000),
        ("08-about-dark", Screen::About, true, &|_, _| 2000),
        ("08-about-light-later", Screen::About, false, &|_, _| 9000),
        ("21-settings-scrolled", Screen::Settings, false, &|app, f| drag(app, f, 1900.0, 1300.0)),
        ("22-settings-scrolled-dark", Screen::Settings, true, &|app, f| drag(app, f, 1900.0, 1300.0)),
        ("23-specs-title-fading", Screen::Specs, false, &|app, f| drag(app, f, 1900.0, 1870.0)),
        ("24-specs-title-gone", Screen::Specs, false, &|app, f| drag(app, f, 1900.0, 1820.0)),
        ("25-specs-under-bar", Screen::Specs, false, &|app, f| drag(app, f, 1900.0, 1450.0)),
        ("26-reboot-pulled-down", Screen::Reboot, false, &|app, f| hold(app, f, 900.0, 1700.0)),
        ("27-back-cue-short", Screen::Home, false, &|app, f| swipe_in(app, f, &info, 133.0)),
        ("28-back-cue-armed", Screen::Home, false, &|app, f| swipe_in(app, f, &info, 323.0)),
        ("29-about-scrolled", Screen::About, false, &|app, f| drag(app, f, 1900.0, 1500.0)),
    ];
    for (name, screen, dark, act) in moments {
        let mut app = App::new(screen, w, h, ppd);
        app.state.dark = dark;
        app.frame(&fonts, &info, open);
        app.tick(0);
        let now = act(&mut app, &fonts);
        // Time passes frame by frame, as on the panel.
        for ms in (0..now).step_by(16) {
            app.tick(ms);
        }
        app.tick(now);
        let mut c = Canvas::new(w, h);
        app.compose(&mut c, &fonts, &info, open, CLOCK, now);
        let path = out.join(format!("{name}.png"));
        write_png(&path, &c)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// File name, page, dark theme, and the touches leading up to the moment,
/// which return the time it is shown at.
type Moment<'a> = (&'a str, Screen, bool, &'a dyn Fn(&mut App, &Fonts) -> u64);

/// Drag from `y0` to `y1` at x = 600 and rest before lifting, so the page
/// stays where the finger left it; returns a time after the bar has
/// settled.
fn drag(app: &mut App, f: &Fonts, y0: f32, y1: f32) -> u64 {
    app.touch(Touch::Down(600.0, y0), f, 0);
    for k in 1..=10u64 {
        app.touch(Touch::Move(600.0 + (k % 2) as f32, y0 + (y1 - y0) * k as f32 / 10.0), f, 40 * k);
    }
    app.touch(Touch::Up, f, 800);
    for ms in (800..2000).step_by(8) {
        app.tick(ms);
    }
    2000
}

/// Pull from `y0` to `y1` at x = 600 and keep the finger down.
fn hold(app: &mut App, f: &Fonts, y0: f32, y1: f32) -> u64 {
    app.touch(Touch::Down(600.0, y0), f, 0);
    for k in 1..=10u64 {
        app.touch(Touch::Move(600.0, y0 + (y1 - y0) * k as f32 / 10.0), f, 40 * k);
    }
    400
}

/// Open the reboot page from the home page, which leaves a page to go back
/// to, then swipe in from the left edge at y = 1300 to `x` and keep the
/// finger down, the way the 17U recordings were made.
fn swipe_in(app: &mut App, f: &Fonts, info: &DeviceInfo, x: f32) -> u64 {
    let hits = &app.frame(f, info, DataState::Decrypted).hits;
    let (rx, ry) = hits.iter().find(|h| h.action == Action::Open(Screen::Reboot)).expect("the home page opens the reboot page").rect.centre();
    app.touch(Touch::Down(rx, ry), f, 0);
    app.touch(Touch::Up, f, 0);
    app.touch(Touch::Down(3.0, 1300.0), f, 0);
    for k in 1..=8u64 {
        app.touch(Touch::Move(3.0 + (x - 3.0) * k as f32 / 8.0, 1300.0 + k as f32), f, 8 * k);
    }
    64
}

/// The slot's partitions on a Xiaomi 13 Ultra, in the order super lists
/// them and where the recovery mounts them, all mounted but `unmounted`.
/// The touch programs run from odm.
fn sample_mounts(unmounted: &[&str]) -> Vec<MountRow> {
    let partitions = [
        ("odm", "/odm"),
        ("product", "/mnt/product"),
        ("system", "/mnt/system"),
        ("system_dlkm", "/system_dlkm"),
        ("system_ext", "/mnt/system_ext"),
        ("vendor", "/vendor"),
        ("vendor_dlkm", "/vendor_dlkm"),
        ("mi_ext", "/mnt/mi_ext"),
    ];
    partitions
        .iter()
        .map(|&(base, point)| {
            let mounted = !unmounted.contains(&base);
            let holders = if mounted && base == "odm" { vec!["touch_report".into(), "touchsensor".into()] } else { Vec::new() };
            MountRow { base: base.into(), point: point.into(), mounted, holders }
        })
        .collect()
}

/// The full length of a page with the status bar over its top, the way a
/// long screenshot shows it.
fn whole_page(app: &mut App, f: &Fonts, info: &DeviceInfo, data: DataState) -> Canvas {
    let t = app.theme();
    let frame = app.frame(f, info, data);
    let mut c = Canvas { w: frame.canvas.w, h: frame.canvas.h, data: frame.canvas.data.clone() };
    let battery = info.battery.as_ref().map(|b| (b.level, b.charge == Charge::Charging));
    widgets::status_bar(&mut c, &t, f, &Bar { clock: CLOCK, battery }, t.pal.status);
    c
}

/// Xiaomi 17 Ultra by Leica as `skyward info` read it on 2026-10-18 (the
/// config supplies the SoC name, panel and screen size).
fn nezha(cfg: &DeviceConfig) -> DeviceInfo {
    DeviceInfo {
        model: "Xiaomi 17 Ultra by Leica".into(),
        codename: cfg.device.codename.clone(),
        soc: cfg.device.soc.clone(),
        cpu: Cpu { cores: 8, max_khz: Some(4_608_000) },
        ram_kib: 15_472_464,
        panel: Some((cfg.display.width, cfg.display.height)),
        screen: cfg.display.diagonal,
        os_version: Some("OS4.0.4.0.XPACNXM".into()),
        android: Some("17".into()),
        security_patch: Some(PatchLevel { year: 2026, month: 10, day: 1 }),
        kernel: "6.12.69-android16-6-g43b5ab7e247d-abogki561813157-4k".into(),
        storage: Storage { total: 1_039_535_157_248, free: Some(620_652_802_048) },
        battery: Some(Battery {
            level: 95,
            charge: Charge::Charging,
            temp: Some(350),
            health: Some("Good".into()),
            cycles: Some(300),
            design: Some(6_800_000),
            full: Some(6_543_000),
            voltage: Some(4_442_000),
        }),
        slot: Some(Slot::B),
        snapshot: SnapshotState::None,
        boot: Boot { unlocked: Some(true), verified: Some("orange".into()) },
    }
}

/// Weight ladders for matching stroke widths against device screenshots: the
/// settings strings at their sizes, and a capital H at the wordmark size.
fn calibrate(fonts: &Fonts, t: &Theme) -> Result<Canvas> {
    let m = &t.m;
    let weights = [330.0, 380.0, 420.0, 450.0, 480.0, 520.0, 560.0, 600.0, 620.0, 650.0, 700.0];
    let mut c = Canvas::new(m.w, 200 * weights.len() as u32);
    c.clear(t.pal.bg);
    let ink = t.pal.text;
    for (i, w) in weights.iter().enumerate() {
        let face = fonts.face(*w)?;
        let top = 10.0 + 200.0 * i as f32;
        c.text_top(&face, "设置", 10.0, top, m.title_px, ink, None);
        c.text_top(&face, "蓝牙", 210.0, top, m.row_title_px, ink, None);
        c.text_top(&face, "亮度", 340.0, top, m.sub_px, ink, None);
        c.text_top(&face, "登录小米账号", 440.0, top, m.account_title_px, ink, None);
        let b = top + 110.0;
        c.text_top(&face, "享受更多小米服务", 10.0, b, m.sub_px, ink, None);
        c.text_top(&face, "已开启", 340.0, b, m.sub_px, ink, None);
        c.text_top(&face, "搜索系统设置项", 480.0, b, m.row_title_px, ink, None);
        c.text(&face, "15:29", 840.0, b + m.status_px * 0.72, m.status_px, ink, None);
        c.text_top(&face, "新版本", 1010.0, b, m.badge_px, ink, None);
        // The stock wordmark is set at a 136.5 px em on the 3.5 px/dp
        // reference panel.
        c.text(&face, "H", 1100.0, top + 105.0, 136.5, ink, None);
    }
    Ok(c)
}

fn font_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SKYWARD_FONT") {
        return Ok(PathBuf::from(p));
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/fonts/MiSansVF.ttf");
    if bundled.exists() {
        return Ok(bundled);
    }
    for p in ["/System/Library/Fonts/STHeiti Medium.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc"] {
        if Path::new(p).exists() {
            return Ok(PathBuf::from(p));
        }
    }
    Err(anyhow!("no font found; set SKYWARD_FONT to a .ttf/.ttc path"))
}

fn write_png(path: &Path, canvas: &Canvas) -> Result<()> {
    let file = File::create(path)?;
    let w = BufWriter::new(file);
    let mut enc = png::Encoder::new(w, canvas.w, canvas.h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&canvas.data)?;
    Ok(())
}
