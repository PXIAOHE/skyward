use anyhow::{anyhow, Context, Result};
use prost::Message;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

#[cfg(target_os = "android")]
pub mod boot;
mod partitions;

pub use partitions::{
    map_slot, map_whole, mount_one, mount_partitions, mount_point, mounted_partitions, release_partitions, slot_bases, slot_sizes,
    unmap_slot, unmap_whole, unmount_one, unmount_partitions, Mounted, Mounts, SlotMaps, Whole,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn suffix(self) -> &'static str {
        match self {
            Slot::A => "_a",
            Slot::B => "_b",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Slot::A => "A",
            Slot::B => "B",
        }
    }

    /// The slot's metadata copy in super.
    pub fn number(self) -> u32 {
        match self {
            Slot::A => 0,
            Slot::B => 1,
        }
    }

    pub fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    pub fn from_suffix(s: &str) -> Option<Slot> {
        match s {
            "_a" => Some(Slot::A),
            "_b" => Some(Slot::B),
            _ => None,
        }
    }
}

/// What the phone's boot control holds of the slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSlots {
    /// The slot the bootloader starts next.
    pub active: Slot,
    /// Of each slot, A first: whether the bootloader may start it, and
    /// whether its system has started fully since it was written.
    pub bootable: [bool; 2],
    pub successful: [bool; 2],
}

/// The booted slot, or `None` on devices that are not A/B.
pub fn current_slot() -> Result<Option<Slot>> {
    match skyward_block::slot_suffix()? {
        None => Ok(None),
        Some(s) => Slot::from_suffix(&s).map(Some).ok_or_else(|| anyhow!("slot suffix {s:?}")),
    }
}

/// Virtual A/B update phase as libsnapshot records it. Anything but `None`
/// means dynamic partitions have live snapshots, and writing them would
/// corrupt the pending update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotState {
    /// No update in flight.
    None,
    /// An update is being written, or is written and waits to be booted.
    Snapshotted,
    /// Snapshots are being merged back into the base partitions.
    Merging,
    /// The merge failed; Android has to recover it.
    MergeFailed,
    /// /metadata is not mounted, so the state cannot be read.
    Unknown,
}

const METADATA: &str = "/metadata";
const OTA_DIR: &str = "/metadata/ota";

/// What libsnapshot keeps in its directory on /metadata: the update's state,
/// and the slot it was installed from, written once the new slot's
/// partitions are.
const STATE: &str = "state";
const SOURCE: &str = "snapshot-boot";

/// libsnapshot's `SnapshotUpdateStatus` from snapshot.proto; only the state is
/// read, the other fields are skipped by the decoder.
#[derive(Clone, PartialEq, Message)]
struct SnapshotUpdateStatus {
    #[prost(enumeration = "UpdateState", tag = "1")]
    state: i32,
}

/// Where an update stands, as libsnapshot names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum UpdateState {
    None = 0,
    /// The new slot's partitions are being written.
    Initiated = 1,
    /// They are written, and wait for the system to start from them and
    /// confirm them.
    Unverified = 2,
    Merging = 3,
    /// Merged, with a restart left to finish cleaning up.
    MergeNeedsReboot = 4,
    MergeCompleted = 5,
    MergeFailed = 6,
    Cancelled = 7,
}

impl UpdateState {
    fn phase(self) -> SnapshotState {
        match self {
            UpdateState::None | UpdateState::MergeCompleted | UpdateState::Cancelled => SnapshotState::None,
            UpdateState::Initiated | UpdateState::Unverified => SnapshotState::Snapshotted,
            UpdateState::Merging | UpdateState::MergeNeedsReboot => SnapshotState::Merging,
            UpdateState::MergeFailed => SnapshotState::MergeFailed,
        }
    }
}

/// An update as libsnapshot keeps track of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Update {
    pub state: UpdateState,
    /// The slot the update was installed from; `None` until the new slot's
    /// partitions are written.
    pub source: Option<Slot>,
}

/// Why erasing the data partition has to wait for an update. Erasing it
/// erases /metadata too, where the update's snapshots are tracked: an update
/// being merged into the slot the phone runs from leaves that slot half old
/// and half new, and an update that slot runs on unconfirmed is lost with
/// its snapshots. Android's own recovery merges the update first, or marks
/// its slot unbootable; erasing here waits for the system to finish it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// The slot the phone runs from got an update the system has not yet
    /// confirmed.
    Unconfirmed,
    Merging,
    MergeFailed,
}

impl Update {
    /// What keeps the data partition from being erased while the phone runs
    /// from `current`, if anything. An update still being written, one the
    /// phone left for the slot it came from, and one merged with only a
    /// restart's cleaning up left go as Android's recovery lets them, with
    /// /metadata.
    pub fn holds_wipe(&self, current: Slot) -> Option<Hold> {
        match self.state {
            UpdateState::Unverified if self.source.is_some_and(|source| source != current) => Some(Hold::Unconfirmed),
            UpdateState::Merging => Some(Hold::Merging),
            UpdateState::MergeFailed => Some(Hold::MergeFailed),
            _ => None,
        }
    }
}

pub fn snapshot_state() -> Result<SnapshotState> {
    if !skyward_block::mount::is_mount_point(Path::new(METADATA))? {
        return Ok(SnapshotState::Unknown);
    }
    Ok(read_state(Path::new(OTA_DIR))?.phase())
}

/// The update libsnapshot keeps track of, where /metadata is mounted to
/// tell; `None` where it is not.
pub fn update() -> Result<Option<Update>> {
    if !skyward_block::mount::is_mount_point(Path::new(METADATA))? {
        return Ok(None);
    }
    read_update(Path::new(OTA_DIR)).map(Some)
}

fn read_update(dir: &Path) -> Result<Update> {
    let state = read_state(dir)?;
    let path = dir.join(SOURCE);
    let source = match fs::read_to_string(&path) {
        Ok(suffix) => Some(Slot::from_suffix(suffix.trim()).with_context(|| format!("{} names the slot {suffix:?}", path.display()))?),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    Ok(Update { state, source })
}

fn read_state(dir: &Path) -> Result<UpdateState> {
    let path = dir.join(STATE);
    match fs::read(&path) {
        Ok(bytes) => decode_state(&bytes),
        // libsnapshot writes the file, and /metadata/ota, with the first update.
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(UpdateState::None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn decode_state(bytes: &[u8]) -> Result<UpdateState> {
    let status = SnapshotUpdateStatus::decode(bytes).context("decode SnapshotUpdateStatus")?;
    UpdateState::try_from(status.state).map_err(|e| anyhow!("update state: {e}"))
}

/// Conservative guard: `false` only when no update is positively known to be
/// in flight. Callers writing to dynamic partitions must honour this.
pub fn merge_outstanding() -> Result<bool> {
    Ok(snapshot_state()? != SnapshotState::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_round_trip() {
        assert_eq!(Slot::from_suffix("_a"), Some(Slot::A));
        assert_eq!(Slot::A.other(), Slot::B);
        assert_eq!(Slot::B.suffix(), "_b");
        assert_eq!(Slot::B.letter(), "B");
        assert_eq!((Slot::A.number(), Slot::B.number()), (0, 1));
        assert_eq!(Slot::from_suffix("_c"), None);
    }

    fn encoded(state: UpdateState) -> Vec<u8> {
        SnapshotUpdateStatus { state: state as i32 }.encode_to_vec()
    }

    fn phase(bytes: &[u8]) -> SnapshotState {
        decode_state(bytes).unwrap().phase()
    }

    #[test]
    fn empty_state_file_means_no_update() {
        // A Xiaomi 17 Ultra with no update pending keeps a zero-length file.
        assert_eq!(phase(&[]), SnapshotState::None);
    }

    #[test]
    fn update_states_map_to_phases() {
        assert_eq!(phase(&encoded(UpdateState::Unverified)), SnapshotState::Snapshotted);
        assert_eq!(phase(&encoded(UpdateState::Initiated)), SnapshotState::Snapshotted);
        assert_eq!(phase(&encoded(UpdateState::Merging)), SnapshotState::Merging);
        assert_eq!(phase(&encoded(UpdateState::MergeNeedsReboot)), SnapshotState::Merging);
        assert_eq!(phase(&encoded(UpdateState::MergeFailed)), SnapshotState::MergeFailed);
        assert_eq!(phase(&encoded(UpdateState::MergeCompleted)), SnapshotState::None);
    }

    #[test]
    fn unknown_state_value_is_an_error() {
        let bytes = SnapshotUpdateStatus { state: 42 }.encode_to_vec();
        assert!(decode_state(&bytes).is_err());
    }

    fn scratch(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("skyward-vab-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_update_is_read_with_the_slot_it_came_from_once_that_is_written() {
        let dir = scratch("read");
        assert_eq!(read_update(&dir).unwrap(), Update { state: UpdateState::None, source: None }, "no update yet");
        fs::write(dir.join(STATE), encoded(UpdateState::Initiated)).unwrap();
        assert_eq!(read_update(&dir).unwrap(), Update { state: UpdateState::Initiated, source: None });
        fs::write(dir.join(STATE), encoded(UpdateState::Unverified)).unwrap();
        fs::write(dir.join(SOURCE), "_a").unwrap();
        assert_eq!(read_update(&dir).unwrap(), Update { state: UpdateState::Unverified, source: Some(Slot::A) });
        fs::write(dir.join(SOURCE), "_c").unwrap();
        assert!(read_update(&dir).is_err(), "a slot that is neither");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn erasing_waits_only_where_androids_recovery_has_to_merge_or_give_the_update_up() {
        let update = |state, source| Update { state, source };
        let free = [
            update(UpdateState::None, None),
            update(UpdateState::Initiated, None),
            update(UpdateState::Unverified, Some(Slot::B)),
            update(UpdateState::Unverified, None),
            update(UpdateState::MergeNeedsReboot, Some(Slot::A)),
            update(UpdateState::MergeCompleted, Some(Slot::A)),
            update(UpdateState::Cancelled, Some(Slot::A)),
        ];
        for u in free {
            assert_eq!(u.holds_wipe(Slot::B), None, "{u:?} from slot B");
        }
        assert_eq!(update(UpdateState::Unverified, Some(Slot::A)).holds_wipe(Slot::B), Some(Hold::Unconfirmed));
        assert_eq!(update(UpdateState::Merging, Some(Slot::A)).holds_wipe(Slot::B), Some(Hold::Merging));
        assert_eq!(update(UpdateState::MergeFailed, Some(Slot::A)).holds_wipe(Slot::B), Some(Hold::MergeFailed));
    }
}
