use anyhow::Result;
use clap::{Parser, Subcommand};
use skyward_vab::SnapshotState;

#[derive(Parser)]
#[command(name = "skyward", version, about = "Skyward recovery")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print device, slot, snapshot and partition summary.
    Info,
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
        Cmd::Info => info(),
        Cmd::Slot => slot(),
        Cmd::Modules { source } => modules(&source),
    }
}

fn info() -> Result<()> {
    match skyward_block::slot_suffix() {
        Some(s) => println!("slot_suffix   = {s}"),
        None => println!("slot_suffix   = (not A/B)"),
    }
    match skyward_vab::current_slot() {
        Some(s) => println!("current_slot  = {}", s.suffix()),
        None => println!("current_slot  = (unknown)"),
    }
    let snap = skyward_vab::snapshot_state();
    println!("snapshot      = {snap:?}");
    if snap != SnapshotState::None {
        println!("  -> dynamic-partition writes are refused while a snapshot is outstanding");
    }
    match skyward_block::list_by_name(skyward_block::BY_NAME) {
        Ok(parts) => println!("partitions    = {} under {}", parts.len(), skyward_block::BY_NAME),
        Err(e) => println!("partitions    = (unavailable: {e})"),
    }
    Ok(())
}

fn slot() -> Result<()> {
    match skyward_vab::current_slot() {
        Some(s) => println!("{}", s.suffix()),
        None => println!("(unknown)"),
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
