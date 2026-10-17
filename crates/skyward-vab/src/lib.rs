use anyhow::{anyhow, Context, Result};
use prost::Message;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

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

const OTA_DIR: &str = "/metadata/ota";

/// libsnapshot's `SnapshotUpdateStatus` from snapshot.proto; only the state is
/// read, the other fields are skipped by the decoder.
#[derive(Clone, PartialEq, Message)]
struct SnapshotUpdateStatus {
    #[prost(enumeration = "UpdateState", tag = "1")]
    state: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
enum UpdateState {
    None = 0,
    Initiated = 1,
    Unverified = 2,
    Merging = 3,
    MergeNeedsReboot = 4,
    MergeCompleted = 5,
    MergeFailed = 6,
    Cancelled = 7,
}

pub fn snapshot_state() -> Result<SnapshotState> {
    match fs::metadata(OTA_DIR) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(SnapshotState::Unknown),
        Err(e) => return Err(e).with_context(|| format!("stat {OTA_DIR}")),
        Ok(_) => {}
    }
    let path = Path::new(OTA_DIR).join("state");
    match fs::read(&path) {
        Ok(bytes) => parse_state(&bytes),
        // libsnapshot writes the file with the first update.
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(SnapshotState::None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn parse_state(bytes: &[u8]) -> Result<SnapshotState> {
    let status = SnapshotUpdateStatus::decode(bytes).context("decode SnapshotUpdateStatus")?;
    let state = UpdateState::try_from(status.state).map_err(|e| anyhow!("update state: {e}"))?;
    Ok(match state {
        UpdateState::None | UpdateState::MergeCompleted | UpdateState::Cancelled => SnapshotState::None,
        UpdateState::Initiated | UpdateState::Unverified => SnapshotState::Snapshotted,
        UpdateState::Merging | UpdateState::MergeNeedsReboot => SnapshotState::Merging,
        UpdateState::MergeFailed => SnapshotState::MergeFailed,
    })
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
        assert_eq!(Slot::from_suffix("_c"), None);
    }

    fn encoded(state: UpdateState) -> Vec<u8> {
        SnapshotUpdateStatus { state: state as i32 }.encode_to_vec()
    }

    #[test]
    fn empty_state_file_means_no_update() {
        // A Xiaomi 17 Ultra with no update pending keeps a zero-length file.
        assert_eq!(parse_state(&[]).unwrap(), SnapshotState::None);
    }

    #[test]
    fn update_states_map_to_phases() {
        assert_eq!(parse_state(&encoded(UpdateState::Unverified)).unwrap(), SnapshotState::Snapshotted);
        assert_eq!(parse_state(&encoded(UpdateState::Initiated)).unwrap(), SnapshotState::Snapshotted);
        assert_eq!(parse_state(&encoded(UpdateState::Merging)).unwrap(), SnapshotState::Merging);
        assert_eq!(parse_state(&encoded(UpdateState::MergeNeedsReboot)).unwrap(), SnapshotState::Merging);
        assert_eq!(parse_state(&encoded(UpdateState::MergeFailed)).unwrap(), SnapshotState::MergeFailed);
        assert_eq!(parse_state(&encoded(UpdateState::MergeCompleted)).unwrap(), SnapshotState::None);
    }

    #[test]
    fn unknown_state_value_is_an_error() {
        let bytes = SnapshotUpdateStatus { state: 42 }.encode_to_vec();
        assert!(parse_state(&bytes).is_err());
    }
}
