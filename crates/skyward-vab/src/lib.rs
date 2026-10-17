use std::fs;

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

pub fn current_slot() -> Option<Slot> {
    skyward_block::slot_suffix().and_then(|s| Slot::from_suffix(&s))
}

/// Virtual A/B snapshot phase. A pending or active snapshot means a system
/// update is mid-flight; destructive operations on dynamic partitions must be
/// refused until the merge completes under Android.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotState {
    None,
    Snapshotted,
    Merging,
    Unknown,
}

pub fn snapshot_state() -> SnapshotState {
    for p in ["/metadata/ota/state", "/metadata/ota/snapshot-boot"] {
        if let Ok(s) = fs::read_to_string(p) {
            let s = s.trim().to_ascii_lowercase();
            if s.contains("merg") {
                return SnapshotState::Merging;
            }
            if s.contains("snapshot") {
                return SnapshotState::Snapshotted;
            }
        }
    }
    // An active COW mapping means snapshots are live even without a state file.
    if let Ok(rd) = fs::read_dir("/dev/block/mapper") {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().contains("-cow") {
                return SnapshotState::Snapshotted;
            }
        }
    }
    if fs::metadata("/metadata/ota").is_err() {
        return SnapshotState::Unknown;
    }
    SnapshotState::None
}

/// Conservative guard: only `false` when we positively confirm no snapshot is
/// outstanding. Callers writing to dynamic partitions must honour this.
pub fn merge_outstanding() -> bool {
    !matches!(snapshot_state(), SnapshotState::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_round_trip() {
        assert_eq!(Slot::from_suffix("_a"), Some(Slot::A));
        assert_eq!(Slot::A.other(), Slot::B);
        assert_eq!(Slot::B.suffix(), "_b");
        assert_eq!(Slot::from_suffix("_c"), None);
    }
}
