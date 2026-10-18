use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use skyward_config::ramdisk as paths;
use std::io::Write;
use std::path::{Path, PathBuf};

mod image;
mod ramdisk;

use image::BootImage;
use ramdisk::Ramdisk;

/// Builds Skyward recovery images from a device's own stock images.
#[derive(Parser)]
#[command(name = "skyward-pack")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print an image's header and the files in its ramdisk.
    List { image: PathBuf },
    /// Write one ramdisk file to stdout.
    Cat { image: PathBuf, path: String },
    /// Unpack and pack an image's ramdisk unchanged, to check that a rebuilt
    /// archive keeps every entry.
    Repack { image: PathBuf, out: PathBuf },
    /// Join a kernel and another image's ramdisk into one boot image.
    Boot {
        /// Image holding the kernel (the device's boot partition).
        #[arg(long)]
        kernel: PathBuf,
        /// Image holding the ramdisk.
        #[arg(long)]
        ramdisk: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Build a Skyward recovery image on top of the device's stock recovery:
    /// its init, adbd, SELinux policy and libraries stay, the recovery
    /// service becomes Skyward.
    Recovery {
        /// The device's stock recovery image.
        #[arg(long)]
        stock: PathBuf,
        /// Skyward built for the device (aarch64-linux-android).
        #[arg(long)]
        binary: PathBuf,
        /// The device configuration, devices/<codename>.toml.
        #[arg(long)]
        config: PathBuf,
        /// The UI typeface.
        #[arg(long)]
        font: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::List { image } => list(&image),
        Cmd::Cat { image, path } => {
            let rd = Ramdisk::decode(&BootImage::read(&image)?.ramdisk)?;
            let e = rd.find(&path).with_context(|| format!("{path} is not in the ramdisk"))?;
            std::io::stdout().write_all(&e.data)?;
            Ok(())
        }
        Cmd::Repack { image, out } => {
            let mut img = BootImage::read(&image)?;
            img.ramdisk = Ramdisk::decode(&img.ramdisk)?.encode()?;
            write(&out, &img)
        }
        Cmd::Boot { kernel, ramdisk, out } => {
            let mut img = BootImage::read(&ramdisk)?;
            img.kernel = BootImage::read(&kernel)?.kernel;
            write(&out, &img)
        }
        Cmd::Recovery { stock, binary, config, font, out } => {
            let mut img = BootImage::read(&stock)?;
            let mut rd = Ramdisk::decode(&img.ramdisk)?;
            // A stock recovery ramdisk has no third-party tree; refuse to build
            // on top of another custom recovery.
            if let Some(e) = rd.entries.iter().find(|e| e.name == "FFiles" || e.name.starts_with("twres")) {
                anyhow::bail!("{} holds {}, which is not a stock recovery", stock.display(), e.name);
            }
            let read = |p: &PathBuf| std::fs::read(p).with_context(|| format!("read {}", p.display()));
            skyward_config::DeviceConfig::load(&config)?;
            rd.replace(paths::RECOVERY, read(&binary)?)?;
            rd.add(paths::CONFIG, 0o644, read(&config)?)?;
            rd.add(paths::FONT, 0o644, read(&font)?)?;
            // On an unlocked device adbd keeps root when ro.secure is 0 and
            // lets any computer in without authorization when ro.adb.secure is
            // 0; init lets a later line of a property file override an earlier
            // one. update_engine refuses a package built before the recovery
            // itself unless both of the last two are set; the recovery checks
            // packages against the installed system instead.
            let mut props = rd
                .find(paths::PROP_DEFAULT)
                .with_context(|| format!("{} is not in the ramdisk", paths::PROP_DEFAULT))?
                .data
                .clone();
            props.extend_from_slice(b"\nro.secure=0\nro.adb.secure=0\nro.debuggable=1\nro.ota.allow_downgrade=true\n");
            rd.replace(paths::PROP_DEFAULT, props)?;
            img.ramdisk = rd.encode()?;
            write(&out, &img)
        }
    }
}

fn write(out: &Path, img: &BootImage) -> Result<()> {
    std::fs::write(out, img.to_bytes()?).with_context(|| format!("write {}", out.display()))?;
    println!("wrote {} ({} bytes kernel, {} bytes ramdisk)", out.display(), img.kernel.len(), img.ramdisk.len());
    Ok(())
}

fn list(image: &Path) -> Result<()> {
    let img = BootImage::read(image)?;
    let h = &img.header;
    println!(
        "header v{}  kernel {} bytes  ramdisk {} bytes  {:?}",
        h.header_version(),
        img.kernel.len(),
        img.ramdisk.len(),
        h.osversionpatch
    );
    if img.ramdisk.is_empty() {
        return Ok(());
    }
    let rd = Ramdisk::decode(&img.ramdisk)?;
    for e in &rd.entries {
        println!("{:06o} {:>5}:{:<5} {:>10}  {}", e.mode, e.uid, e.gid, e.data.len(), e.name);
    }
    println!("{} entries", rd.entries.len());
    Ok(())
}
