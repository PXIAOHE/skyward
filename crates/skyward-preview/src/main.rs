use anyhow::{anyhow, Result};
use skyward_ui::{pages, Canvas, Fonts, Theme};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const W: u32 = 1200;
const H: u32 = 2608;

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

    let light = Theme::light(W, H);
    let dark = Theme::dark(W, H);

    // Weight ladder used to match stroke widths against device screenshots.
    if args.get(2).map(String::as_str) == Some("--calibrate") {
        let m = &light.m;
        let weights = [330.0, 380.0, 420.0, 450.0, 480.0, 520.0, 560.0, 600.0, 650.0, 700.0];
        let mut c = Canvas::new(W, 200 * weights.len() as u32);
        c.clear(light.pal.bg);
        let ink = light.pal.text;
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
        }
        let path = out.join("calibrate.png");
        write_png(&path, &c)?;
        println!("wrote {}", path.display());
        return Ok(());
    }

    let shots: Vec<(&str, Canvas)> = vec![
        ("01-decrypt-dark", pages::decrypt(&dark, &fonts, &[0, 3, 4, 5, 8])),
        ("02-home-light", pages::home(&light, &fonts)),
        ("03-home-dark", pages::home(&dark, &fonts)),
        ("04-settings-light", pages::settings(&light, &fonts)),
        ("05-settings-dark", pages::settings(&dark, &fonts)),
        ("06-reboot-dark", pages::reboot(&dark, &fonts)),
        ("07-wipe-light", pages::wipe(&light, &fonts)),
        ("08-install-dark", pages::install_progress(&dark, &fonts)),
    ];

    for (name, canvas) in &shots {
        let path = out.join(format!("{name}.png"));
        write_png(&path, canvas)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn font_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SKYWARD_FONT") {
        return Ok(PathBuf::from(p));
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/fonts/MiSansVF.ttf");
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
