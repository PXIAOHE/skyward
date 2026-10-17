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
    let fonts = Fonts::load_single(&font_path, 0)?;
    eprintln!("font: {}", font_path.display());

    let light = Theme::light(W, H);
    let dark = Theme::dark(W, H);

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
    for p in [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ] {
        if Path::new(p).exists() {
            return Ok(PathBuf::from(p));
        }
    }
    Err(anyhow!("no CJK font found; set SKYWARD_FONT to a .ttf/.ttc path"))
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
