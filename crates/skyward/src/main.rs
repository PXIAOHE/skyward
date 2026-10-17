use anyhow::Result;
use clap::{Parser, Subcommand};
use skyward_config::DeviceConfig;
use skyward_vab::SnapshotState;

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
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Info { config } => info(&config),
        Cmd::Slot => slot(),
        Cmd::Modules { source } => modules(&source),
    }
}

fn info(config: &str) -> Result<()> {
    let cfg = DeviceConfig::load(config)?;
    let i = skyward_info::collect(&cfg)?;
    println!("model         = {} ({})", i.model, i.codename);
    println!("soc           = {:?}", i.soc);
    println!("cpu           = {} cores, max {:?} kHz", i.cpu.cores, i.cpu.max_khz);
    println!("ram           = {} KiB", i.ram_kib);
    println!("panel         = {:?}", i.panel);
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
    if snap != SnapshotState::None {
        println!("  -> dynamic-partition writes are refused while a snapshot is outstanding");
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
