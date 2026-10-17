use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use skyward_config::{ramdisk, DeviceConfig};
use skyward_sys::{bcb, Power};
use skyward_vab::SnapshotState;
use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
mod input;
#[cfg(target_os = "android")]
mod ui;

#[derive(Parser)]
#[command(name = "skyward", version, about = "Skyward recovery")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print what the About page shows, plus a partition summary.
    Info {
        /// Device configuration, e.g. devices/nezha.toml.
        #[arg(long)]
        config: String,
    },
    /// Print the active slot suffix.
    Slot,
    /// List modules in dependency load order from a source directory.
    Modules {
        #[arg(long, default_value = "/vendor/lib/modules")]
        source: String,
    },
    /// Load kernel modules and their dependencies from the configured
    /// sources; with no names, the configuration's own list.
    Load {
        #[arg(long, default_value = "/skyward/device.toml")]
        config: String,
        names: Vec<String>,
    },
    /// Run as the recovery service (what init starts as /system/bin/recovery).
    Recovery,
    /// Reboot through init: system, recovery, bootloader, edl, or off.
    Reboot { target: String },
    /// Print the bootloader control block in misc.
    Bcb,
    /// Print the logical partition metadata in super.
    Lp {
        /// The super partition, or an image of its start.
        #[arg(long = "super", default_value = "/dev/block/by-name/super")]
        super_dev: PathBuf,
        /// Metadata slot: 0 for _a, 1 for _b. Defaults to the booted slot.
        #[arg(long)]
        slot: Option<u32>,
    },
    /// Map the booted slot's partitions from super and mount them read-only.
    Mount,
    /// Unmount what `mount` mounted and remove its device-mapper devices.
    Umount,
    /// List the display connectors, their modes, CRTCs and planes.
    #[cfg(target_os = "android")]
    Kms {
        #[arg(long, default_value = "/dev/dri/card0")]
        card: String,
    },
    /// Drive the panel and touchscreen with the recovery UI.
    #[cfg(target_os = "android")]
    Ui {
        #[arg(long, default_value = "/skyward/device.toml")]
        config: std::path::PathBuf,
        #[arg(long, default_value = "/skyward/MiSansVF.ttf")]
        font: std::path::PathBuf,
        /// Open on the unlock page instead of the home page.
        #[arg(long)]
        unlock: bool,
        /// After each frame, also write it as raw RGBA to this file.
        #[arg(long)]
        dump: Option<std::path::PathBuf>,
    },
}

fn main() -> Result<()> {
    // init starts the recovery service by path with no arguments.
    let started_as = std::env::args().next().unwrap_or_default();
    if Path::new(&started_as).file_name().is_some_and(|n| n == "recovery") {
        return recovery();
    }
    match Cli::parse().cmd {
        Cmd::Info { config } => info(&config),
        Cmd::Slot => slot(),
        Cmd::Modules { source } => modules(&source),
        Cmd::Load { config, names } => {
            let cfg = DeviceConfig::load(&config)?;
            let names = if names.is_empty() { cfg.modules.load.clone() } else { names };
            for path in load_modules(&cfg, &names)? {
                println!("{}", path.display());
            }
            Ok(())
        }
        Cmd::Recovery => recovery(),
        Cmd::Reboot { target } => {
            let p = Power::parse(&target).ok_or_else(|| anyhow!("unknown target {target:?}"))?;
            skyward_sys::power(p)
        }
        Cmd::Bcb => {
            let b = bcb::read(&bcb::misc())?;
            println!("command = {:?}", b.command);
            println!("args    = {:?}", b.args);
            Ok(())
        }
        Cmd::Lp { super_dev, slot } => lp(&super_dev, slot),
        Cmd::Mount => {
            let mounts = skyward_vab::mount_partitions(booted_slot()?)?;
            for d in &mounts.damaged {
                println!("damaged: {d}");
            }
            for m in &mounts.partitions {
                println!("{:<14} {:<16} {:<18} {}", m.partition, m.device.display(), m.mount_point.display(), m.filesystem);
            }
            Ok(())
        }
        Cmd::Umount => skyward_vab::unmount_partitions(booted_slot()?),
        #[cfg(target_os = "android")]
        Cmd::Kms { card } => {
            print!("{}", skyward_ui::surface::kms::report(&card)?);
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Ui { config, font, unlock, dump } => {
            let start = if unlock { skyward_ui::app::Screen::Decrypt } else { skyward_ui::app::Screen::Home };
            ui::run(&config, &font, start, dump.as_deref())
        }
    }
}

fn booted_slot() -> Result<skyward_vab::Slot> {
    skyward_vab::current_slot()?.context("not an A/B device")
}

fn lp(super_dev: &Path, slot: Option<u32>) -> Result<()> {
    let slot = match slot {
        Some(n) => n,
        None => booted_slot()?.number(),
    };
    let md = skyward_lp::read(super_dev, slot)?;
    let g = md.geometry;
    println!("slot {slot}: version {}.{}, flags {:#x}", md.major_version, md.minor_version, md.header_flags);
    println!(
        "geometry: {} metadata slots of {} bytes, logical block {} bytes",
        g.metadata_slot_count, g.metadata_max_size, g.logical_block_size
    );
    for d in &md.damaged {
        println!("damaged: {d}");
    }
    for d in &md.block_devices {
        println!(
            "device {}: first sector {}, {} bytes, alignment {} offset {}",
            d.partition_name, d.first_logical_sector, d.size, d.alignment, d.alignment_offset
        );
    }
    for gr in &md.groups {
        println!("group {}: max {} bytes", gr.name, gr.maximum_size);
    }
    for p in &md.partitions {
        println!(
            "partition {} group {} attributes {:#x}, {} bytes",
            p.name, md.groups[p.group].name, p.attributes, p.bytes()
        );
        let mut start = 0;
        for e in &p.extents {
            let end = start + e.sectors() - 1;
            match *e {
                skyward_lp::Extent::Linear { device, sector, .. } => {
                    println!("  {start} .. {end} linear {} {sector}", md.block_devices[device].partition_name)
                }
                skyward_lp::Extent::Zero { .. } => println!("  {start} .. {end} zero"),
            }
            start = end + 1;
        }
    }
    Ok(())
}

/// Load `names` from the module sources the configuration lists.
fn load_modules(cfg: &DeviceConfig, names: &[String]) -> Result<Vec<PathBuf>> {
    let dirs = cfg.modules.sources.iter().map(|s| skyward_modules::source_dir(s)).collect::<Result<Vec<_>>>()?;
    skyward_modules::load(&dirs, names)
}

/// The recovery service. adb comes up first so a failure later on can be
/// inspected, then the boot request in misc is taken so that any reboot from
/// here boots the system; failures go to the kernel log and still end the
/// process.
fn recovery() -> Result<()> {
    let run = || -> Result<()> {
        skyward_sys::set_prop("sys.usb.config", "adb")?;
        let request = bcb::take(&bcb::misc())?;
        let cfg = DeviceConfig::load(Path::new("/").join(ramdisk::CONFIG))?;
        skyward_sys::kmsg(&format!(
            "{} {} recovery up, misc command {:?} args {:?}",
            cfg.device.codename,
            skyward_info::VERSION,
            request.command,
            request.args
        ))?;
        let slot = booted_slot()?;
        // Recovery has to stay usable when super cannot be mounted, as after
        // it is wiped, so the reason goes to the kernel log instead.
        match skyward_vab::mount_partitions(slot) {
            Ok(mounts) => {
                for d in &mounts.damaged {
                    skyward_sys::kmsg(&format!("super: {d}"))?;
                }
                for m in &mounts.partitions {
                    skyward_sys::kmsg(&format!(
                        "mounted {} on {} ({})",
                        m.partition,
                        m.mount_point.display(),
                        m.filesystem
                    ))?;
                }
            }
            Err(e) => skyward_sys::kmsg(&format!("partitions not mounted: {e:#}"))?,
        }
        // init restarts a service that exits, so the service stays resident.
        loop {
            std::thread::park();
        }
    };
    run().inspect_err(|e| {
        let _ = skyward_sys::kmsg(&format!("recovery failed: {e:#}"));
    })
}

fn info(config: &str) -> Result<()> {
    let cfg = DeviceConfig::load(config)?;
    let i = skyward_info::collect(&cfg)?;
    println!("model         = {} ({})", i.model, i.codename);
    println!("soc           = {:?}", i.soc);
    println!("cpu           = {} cores, max {:?} kHz", i.cpu.cores, i.cpu.max_khz);
    println!("ram           = {} KiB", i.ram_kib);
    println!("panel         = {:?}", i.panel);
    println!("screen        = {:?} in", i.screen);
    println!("os_version    = {:?}", i.os_version);
    println!("android       = {:?}", i.android);
    println!("patch         = {:?}", i.security_patch);
    println!("kernel        = {}", i.kernel);
    println!("storage       = {} bytes, free {:?}", i.storage.total, i.storage.free);
    println!("battery       = {:?}", i.battery);
    println!("boot          = {:?}", i.boot);
    println!("recovery      = {}", skyward_info::VERSION);
    match i.slot {
        Some(s) => println!("current_slot  = {}", s.suffix()),
        None => println!("current_slot  = (not A/B)"),
    }
    let snap = i.snapshot;
    println!("snapshot      = {snap:?}");
    match snap {
        SnapshotState::None => {}
        SnapshotState::Unknown => println!("  -> /metadata is not mounted; dynamic-partition writes stay refused"),
        _ => println!("  -> dynamic-partition writes are refused while a snapshot is outstanding"),
    }
    let parts = skyward_block::list_by_name(skyward_block::BY_NAME)?;
    println!("partitions    = {} under {}", parts.len(), skyward_block::BY_NAME);
    Ok(())
}

fn slot() -> Result<()> {
    match skyward_vab::current_slot()? {
        Some(s) => println!("{}", s.suffix()),
        None => println!("(not A/B)"),
    }
    Ok(())
}

fn modules(source: &str) -> Result<()> {
    let graph = skyward_modules::parse_dep(source)?;
    let order = graph.load_order(&[]);
    println!("{} modules in dependency order", order.len());
    for m in &order {
        println!("  {m}");
    }
    Ok(())
}
