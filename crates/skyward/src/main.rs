use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use skyward_config::{ramdisk, DeviceConfig};
use skyward_sys::{bcb, Power};
use skyward_vab::SnapshotState;
use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
mod haptics;
#[cfg(target_os = "android")]
mod input;
#[cfg(target_os = "android")]
mod secure;
#[cfg(target_os = "android")]
mod services;
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
    /// Tell what a package or image is, the way the installer reads it.
    Identify { file: PathBuf },
    /// Map the booted slot's partitions from super and mount them read-only.
    Mount,
    /// Unmount what `mount` mounted and remove its device-mapper devices.
    Umount,
    /// Keep the configuration's services running until stopped.
    #[cfg(target_os = "android")]
    Services {
        #[arg(long, default_value = "/skyward/device.toml")]
        config: String,
    },
    /// List the display connectors, their modes, CRTCs and planes.
    #[cfg(target_os = "android")]
    Kms {
        #[arg(long, default_value = "/dev/dri/card0")]
        card: String,
    },
    /// Play a waveform of the vibration motor once, saying what its driver
    /// reports of it.
    #[cfg(target_os = "android")]
    Haptic {
        /// The driver's waveform number; 0 is the click.
        #[arg(default_value_t = 0)]
        effect: i16,
        /// Stop it after this many milliseconds.
        #[arg(long, default_value_t = 60)]
        stop: u64,
    },
    /// Save what the panel shows, as raw RGBA files cap-000.rgba onwards.
    #[cfg(target_os = "android")]
    Capture {
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        count: u32,
        /// Pause between captures, in milliseconds.
        #[arg(long, default_value_t = 100)]
        interval: u64,
        #[arg(long, default_value = "/dev/dri/card0")]
        card: String,
    },
    /// Set the system clock from the RTC and the offset Android keeps on
    /// persist, as the recovery does when it starts, and print the time.
    #[cfg(target_os = "android")]
    Clock,
    /// Say whether a service has registered with the phone's service
    /// directory, such as android.hardware.security.keymint.IKeyMintDevice/default.
    #[cfg(target_os = "android")]
    Service { name: String },
    /// Report the phone's key service and, given a key directory Android
    /// wrote, unwrap the key it holds and say how long it is.
    #[cfg(target_os = "android")]
    Keymint {
        #[arg(long, default_value = "/metadata/vold/metadata_encryption/key")]
        key: PathBuf,
    },
    /// Say what the phone's credential service answers, which takes no
    /// credential and changes nothing.
    #[cfg(target_os = "android")]
    Gatekeeper,
    /// Open the data partition with the key the phone keeps for it, and
    /// mount it read-only.
    #[cfg(target_os = "android")]
    Data {
        /// What a user would type at their screen lock, where one is set.
        /// A wrong one is counted against that user by the phone's secure
        /// world, the same as a wrong try on the screen itself.
        #[arg(long)]
        credential: Option<String>,
    },
    /// Drive the panel and touchscreen with the recovery UI.
    #[cfg(target_os = "android")]
    Ui {
        #[arg(long, default_value = "/skyward/device.toml")]
        config: std::path::PathBuf,
        #[arg(long, default_value = "/skyward/MiSansVF.ttf")]
        font: std::path::PathBuf,
        /// The terminal's monospaced typeface.
        #[arg(long, default_value = "/skyward/KirkMono-Regular.ttf")]
        mono: std::path::PathBuf,
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
        Cmd::Identify { file } => identify(&file),
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
        Cmd::Services { config } => {
            services::start(&DeviceConfig::load(&config)?.services, secure::libraries()?.as_deref())?;
            loop {
                std::thread::park();
            }
        }
        #[cfg(target_os = "android")]
        Cmd::Kms { card } => {
            print!("{}", skyward_ui::surface::kms::report(&card)?);
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Haptic { effect, stop } => haptics::probe(effect, stop),
        #[cfg(target_os = "android")]
        Cmd::Capture { out, count, interval, card } => {
            std::fs::create_dir_all(&out).with_context(|| format!("create {}", out.display()))?;
            for i in 0..count {
                let t0 = std::time::Instant::now();
                let c = skyward_ui::surface::kms::capture(&card)?;
                let path = out.join(format!("cap-{i:03}.rgba"));
                std::fs::write(&path, &c.data).with_context(|| format!("write {}", path.display()))?;
                println!("{} {}x{} in {} ms", path.display(), c.w, c.h, t0.elapsed().as_millis());
                std::thread::sleep(std::time::Duration::from_millis(interval));
            }
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Clock => {
            let ms = skyward_sys::clock::set_from_rtc()?;
            println!("{}", skyward_ui::fmt::datetime((ms / 1000) as i64));
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Service { name } => {
            let binder = skyward_binder::driver::Binder::open()?;
            let described = if skyward_binder::service::declared(&binder, &name)? { "described" } else { "not described" };
            let running = match skyward_binder::service::find(&binder, &name)? {
                Some(_) => "registered",
                None => "not registered",
            };
            println!("{name} is {described} and {running}");
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Keymint { key } => {
            let binder = skyward_binder::driver::Binder::open()?;
            let service = skyward_crypto::keymint::KeyMint::open(&binder)?;
            let hw = service.hardware()?;
            println!("keymint {} version {} at security level {} by {}", hw.name, hw.version, hw.security_level, hw.author);
            let stored = skyward_crypto::keystore::StoredKey::load(&key)?;
            for (tag, value) in stored.authorizations(&service)? {
                println!("  tag {tag} = {value}");
            }
            let raw = stored.unwrap(skyward_crypto::keystore::Unwrap::Hardware(&service))?;
            println!("{} holds a key of {} bytes", key.display(), raw.len());
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Gatekeeper => {
            let hardware = skyward_binder::driver::Binder::hardware()?;
            let gatekeeper = skyward_crypto::gatekeeper::Gatekeeper::open(&hardware)?;
            println!("the credential service answers for {}", gatekeeper.serves()?);
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Data { credential } => {
            let binder = skyward_binder::driver::Binder::open()?;
            let mut service = skyward_crypto::keymint::KeyMint::open(&binder)?;
            let at = Path::new(skyward_crypto::data::MOUNT);
            if skyward_block::mount::is_mount_point(at)? {
                println!("{} is open already", at.display());
            } else {
                let (mapped, filesystem) = skyward_crypto::data::open(&service)?;
                println!("{} holds {} at {}", mapped.display(), filesystem, at.display());
            }
            skyward_crypto::data::open_shared(&service)?;
            println!("the files that belong to no one user can be read");
            let users = skyward_crypto::data::open_users(&service)?;
            println!("users whose own keys are in: {users:?}");
            for user in users {
                match skyward_crypto::data::open_user_files(&mut service, user, credential.as_deref().map(str::as_bytes)) {
                    Ok(()) => println!("user {user}'s own files can be read"),
                    Err(e) => println!("user {user}'s own files stay closed: {e:#}"),
                }
            }
            Ok(())
        }
        #[cfg(target_os = "android")]
        Cmd::Ui { config, font, mono, unlock, dump } => {
            let cfg = DeviceConfig::load(&config)?;
            let start = if unlock { skyward_ui::app::Screen::Decrypt } else { skyward_ui::app::Screen::Home };
            ui::run(&cfg, &font, &mono, start, dump.as_deref())
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

/// Load the configured kernel modules and start the phone's own programs,
/// both read from the partitions just mounted: on the 13 Ultra, the
/// touchscreen driver and the touch processing.
#[cfg(target_os = "android")]
fn bring_up(cfg: &DeviceConfig, slot: skyward_vab::Slot) -> Result<()> {
    let nodes = skyward_sys::ueventd::apply()?;
    skyward_sys::kmsg(&format!("{nodes} device nodes given the owner their phone describes"))?;
    adopt_installed_version(slot)?;
    for path in load_modules(cfg, &cfg.modules.load)? {
        skyward_sys::kmsg(&format!("loaded {}", path.display()))?;
    }
    let hardware: Vec<String> = cfg.services.iter().filter_map(|s| s.ready_hardware_service.clone()).collect();
    if !hardware.is_empty() {
        secure::prepare_hardware_directory(&hardware)?;
        skyward_sys::kmsg("the directory for the phone's hardware services can run")?;
    }
    // Only a program that has to register with the service directory needs
    // the directory to know the phone describes it.
    let registering: Vec<String> = cfg.services.iter().filter_map(|s| s.ready_service.clone()).collect();
    if !registering.is_empty() {
        let described = secure::publish_descriptions(&registering)?;
        skyward_sys::kmsg(&format!("{described} of the phone's own service descriptions published"))?;
    }
    let libraries = secure::libraries()?;
    if let Some(dir) = &libraries {
        skyward_sys::kmsg(&format!("the phone's own libraries are at {}", dir.display()))?;
    }
    services::start(&cfg.services, libraries.as_deref())
}

/// What the phone's own secure services read to tell the secure world which
/// build is running. A recovery image carries its own, older build's values;
/// the secure world then sees keys the installed system made as made under
/// a build it does not know, and refuses them. The recovery's property files
/// leave these two out so that they can be set here instead.
#[cfg(target_os = "android")]
const BUILD_VERSION: [&str; 2] = ["ro.build.version.release", "ro.build.version.security_patch"];

#[cfg(target_os = "android")]
fn adopt_installed_version(slot: skyward_vab::Slot) -> Result<()> {
    let props = skyward_info::installed_props(slot)?;
    for name in BUILD_VERSION {
        let value = props.get(name).with_context(|| format!("the installed system says no {name}"))?;
        skyward_sys::set_prop(name, value)?;
        skyward_sys::kmsg(&format!("{name} is the installed system's {value}"))?;
    }
    Ok(())
}

/// The recovery service. adb comes up first so a failure later on can be
/// inspected, then the boot request in misc is taken so that any reboot from
/// here boots the system; failures go to the kernel log and still end the
/// process, which init then starts again.
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
        // The kernel starts the clock from the RTC, days after 1970 on these
        // phones; the screen and the files written still work without the
        // real time, so a failure goes to the kernel log.
        #[cfg(target_os = "android")]
        match skyward_sys::clock::set_from_rtc() {
            Ok(ms) => skyward_sys::kmsg(&format!("clock set to {} ms from the RTC and persist", ms))?,
            Err(e) => skyward_sys::kmsg(&format!("clock not set: {e:#}"))?,
        }
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
                // Without them the screen still shows and the keys still
                // work, so this reason goes to the kernel log as well.
                #[cfg(target_os = "android")]
                if let Err(e) = bring_up(&cfg, slot) {
                    skyward_sys::kmsg(&format!("devices not brought up: {e:#}"))?;
                }
            }
            Err(e) => skyward_sys::kmsg(&format!("partitions not mounted: {e:#}"))?,
        }
        #[cfg(target_os = "android")]
        {
            let root = Path::new("/");
            ui::run(&cfg, &root.join(ramdisk::FONT), &root.join(ramdisk::MONO), skyward_ui::app::Screen::Home, None)
        }
        #[cfg(not(target_os = "android"))]
        {
            anyhow::bail!("the recovery service runs only on Android")
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

fn identify(path: &Path) -> Result<()> {
    use skyward_install::package::{self, Package};
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    match package::identify(std::io::BufReader::new(file))? {
        Package::Update(u) => {
            println!("an A/B update to {}", u.version().unwrap_or("an unnamed version"));
            println!("payload.bin at byte {}, {} bytes", u.payload_offset, u.payload_size);
            for key in ["ota-type", "pre-device", "post-build", "post-timestamp", "post-security-patch-level", "pre-build"] {
                if let Some(v) = u.metadata.get(key) {
                    println!("{key:<26} {v}");
                }
            }
        }
        Package::Image(kind) => println!("{kind:?}, for {}", kind.partitions().join(" or ")),
        Package::Script => println!("a zip that installs itself with a script"),
        Package::Unknown => println!("nothing this recovery installs"),
    }
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
