use crate::{snapshot_state, Slot, SnapshotState, METADATA};
use anyhow::{ensure, Context, Result};
use skyward_block::{devnum, dm, mount, BY_NAME};
use skyward_lp::{Extent, Metadata, Partition, ATTR_DISABLED};
use std::fs;
use std::path::{Path, PathBuf};

/// A logical partition of super, mapped and mounted read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mounted {
    /// The name in super, such as vendor_dlkm_b; also the device-mapper name.
    pub partition: String,
    pub device: PathBuf,
    pub mount_point: PathBuf,
    pub filesystem: String,
}

#[derive(Debug, Clone, Default)]
pub struct Mounts {
    pub partitions: Vec<Mounted>,
    /// Damaged metadata copies in super that their backups stood in for.
    pub damaged: Vec<String>,
}

/// Where a logical partition goes in recovery. System goes to /mnt/system,
/// since /system holds recovery itself; a partition whose mount point
/// directory is in the ramdisk root uses it, as /vendor or /odm; any other
/// goes under /mnt.
pub fn mount_point(base: &str) -> PathBuf {
    let root = Path::new("/").join(base);
    if base != "system" && fs::symlink_metadata(&root).is_ok_and(|m| m.is_dir()) {
        root
    } else {
        Path::new("/mnt").join(base)
    }
}

fn super_device() -> PathBuf {
    Path::new(BY_NAME).join("super")
}

/// The slot's partitions that hold data. Copy-on-write partitions such as
/// odm_b-cow end in -cow and stay out.
fn slot_partitions(md: &Metadata, slot: Slot) -> Vec<&Partition> {
    md.partitions
        .iter()
        .filter(|p| p.name.ends_with(slot.suffix()) && !p.extents.is_empty() && p.attributes & ATTR_DISABLED == 0)
        .collect()
}

fn base(p: &Partition, slot: Slot) -> &str {
    &p.name[..p.name.len() - slot.suffix().len()]
}

/// Mount /metadata, then map the slot's logical partitions from super and
/// mount them, all read-only. Partitions are mapped only while no update
/// snapshot is outstanding, since a snapshotted partition reads correctly
/// only through its snapshot.
pub fn mount_partitions(slot: Slot) -> Result<Mounts> {
    let metadata = Path::new(METADATA);
    if !mount::is_mount_point(metadata)? {
        mount::mount_ro(&Path::new(BY_NAME).join("metadata"), metadata)?;
    }
    let state = snapshot_state()?;
    ensure!(state == SnapshotState::None, "an update snapshot is outstanding ({state:?})");

    let super_dev = super_device();
    let md = skyward_lp::read(&super_dev, slot.number())?;
    let devices = md
        .block_devices
        .iter()
        .enumerate()
        .map(|(i, d)| if i == 0 { devnum(&super_dev) } else { devnum(Path::new(BY_NAME).join(&d.partition_name)) })
        .collect::<Result<Vec<_>>>()?;

    let mut mounts = Mounts { partitions: Vec::new(), damaged: md.damaged.clone() };
    for p in slot_partitions(&md, slot) {
        let targets: Vec<dm::Target> = p
            .extents
            .iter()
            .map(|e| match *e {
                Extent::Linear { sectors, device, sector } => {
                    dm::Target::Linear { sectors, device: devices[device], offset: sector }
                }
                Extent::Zero { sectors } => dm::Target::Zero { sectors },
            })
            .collect();
        let device = dm::create(&p.name, &targets, true)?;
        let mount_point = mount_point(base(p, slot));
        fs::create_dir_all(&mount_point).with_context(|| format!("create {}", mount_point.display()))?;
        let filesystem = mount::mount_ro_over(&device, &mount_point)
            .with_context(|| format!("mount {}", p.name))?
            .to_string();
        mounts.partitions.push(Mounted { partition: p.name.clone(), device, mount_point, filesystem });
    }
    Ok(mounts)
}

/// The slot's partitions that are mapped and show on their mount points, in
/// the order they were mapped.
pub fn mounted_partitions(slot: Slot) -> Result<Vec<Mounted>> {
    let mut found = Vec::new();
    for entry in fs::read_dir("/sys/block").context("read /sys/block")? {
        let entry = entry?;
        let node = entry.file_name().to_string_lossy().into_owned();
        let Some(minor) = node.strip_prefix("dm-") else { continue };
        let name_file = entry.path().join("dm/name");
        let name = fs::read_to_string(&name_file).with_context(|| format!("read {}", name_file.display()))?;
        let name = name.trim();
        let Some(base) = name.strip_suffix(slot.suffix()) else { continue };
        let device = Path::new("/dev/block").join(&node);
        let mount_point = mount_point(base);
        let Some(shown) = mount::visible(&mount_point)? else { continue };
        if shown.device != devnum(&device)? {
            continue;
        }
        let m = Mounted { partition: name.to_string(), device, mount_point, filesystem: shown.filesystem };
        found.push((minor.parse::<u32>()?, m));
    }
    found.sort_by_key(|(minor, _)| *minor);
    Ok(found.into_iter().map(|(_, m)| m).collect())
}

/// Undo `mount_partitions`: unmount the slot's partitions where their own
/// device is what shows, remove their device-mapper devices, and unmount
/// /metadata.
pub fn unmount_partitions(slot: Slot) -> Result<()> {
    let md = skyward_lp::read(&super_device(), slot.number())?;
    for p in slot_partitions(&md, slot).into_iter().rev() {
        let Some(dev) = dm::status(&p.name)? else { continue };
        let mount_point = mount_point(base(p, slot));
        if mount::visible(&mount_point)?.is_some_and(|v| v.device == dev) {
            mount::umount_over(&mount_point)?;
        }
        dm::remove(&p.name)?;
    }
    let metadata = Path::new(METADATA);
    let metadata_dev = devnum(Path::new(BY_NAME).join("metadata"))?;
    if mount::visible(metadata)?.is_some_and(|v| v.device == metadata_dev) {
        mount::umount(metadata)?;
    }
    Ok(())
}
