use anyhow::{anyhow, Result};
use skyward_config::DeviceConfig;
use skyward_info::{Battery, Boot, Charge, Cpu, DataState, DeviceInfo, PatchLevel, Storage};
use skyward_ui::{pages, Canvas, Ctx, Fonts, Theme};
use skyward_vab::{Slot, SnapshotState};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

mod matcher;

const CLOCK: &str = "23:46";

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
    let light = Theme::light(w, h);
    let dark = Theme::dark(w, h);

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
    let f = &fonts;
    let open = DataState::Decrypted;
    let shots: Vec<(&str, Canvas)> = vec![
        ("01-decrypt-dark", pages::decrypt(&ctx(&dark, f, &info, DataState::Locked), &[0, 3, 4, 5, 8])),
        ("02-home-light", pages::home(&ctx(&light, f, &info, open))),
        ("03-home-dark-update-pending", pages::home(&ctx(&dark, f, &pending, open))),
        ("04-settings-light", pages::settings(&ctx(&light, f, &info, open))),
        ("05-settings-dark", pages::settings(&ctx(&dark, f, &info, open))),
        ("06-reboot-dark", pages::reboot(&ctx(&dark, f, &info, open))),
        ("07-wipe-light", pages::wipe(&ctx(&light, f, &info, open))),
        ("08-install-dark", pages::install_progress(&ctx(&dark, f, &info, open))),
        // The About backdrop is the same in both themes.
        ("09-about", pages::about(&ctx(&light, f, &info, open))),
        ("10-specs-light", pages::specs(&ctx(&light, f, &info, open))),
        ("11-specs-dark", pages::specs(&ctx(&dark, f, &info, open))),
    ];
    for (name, canvas) in &shots {
        let path = out.join(format!("{name}.png"));
        write_png(&path, canvas)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn ctx<'a>(t: &'a Theme, f: &'a Fonts, info: &'a DeviceInfo, data: DataState) -> Ctx<'a> {
    Ctx { t, f, info, data, clock: CLOCK }
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
