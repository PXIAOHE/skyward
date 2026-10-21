//! Keeping this recovery in the recovery partitions an install writes
//! over: this recovery as its partition holds it, and which slots get it
//! back.

use crate::image;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use skyward_vab::Slot;
use std::fs::File;
use std::io::{self, Cursor};
use std::path::Path;

/// This recovery as the recovery partition at `path` holds it, the whole
/// partition; refused when the partition holds something else.
pub fn own(path: &Path) -> Result<Vec<u8>> {
    let image = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    ensure!(image::is_skyward(&mut Cursor::new(&image))?, "{} does not hold Skyward", path.display());
    Ok(image)
}

/// The SHA-256 of all `path` holds.
pub fn digest(path: &Path) -> Result<[u8; 32]> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut sha = Sha256::new();
    io::copy(&mut file, &mut sha).with_context(|| format!("read {}", path.display()))?;
    Ok(sha.finalize().into())
}

/// The slots whose recovery partition gets this recovery written back
/// once an install is done: each one the install wrote over, and the slot
/// an update went to, which starts next; but none that holds this recovery
/// already. `before` and `after` are what each slot's partition held, as
/// digests, slot A first, and `own` is this recovery's.
pub fn restore(before: [[u8; 32]; 2], after: [[u8; 32]; 2], own: [u8; 32], updated: Option<Slot>) -> Vec<Slot> {
    [Slot::A, Slot::B]
        .into_iter()
        .filter(|&slot| {
            let k = slot.number() as usize;
            after[k] != own && (after[k] != before[k] || updated == Some(slot))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKYWARD: [u8; 32] = [1; 32];
    const STOCK: [u8; 32] = [2; 32];
    const OLD: [u8; 32] = [3; 32];

    #[test]
    fn an_update_gets_this_recovery_in_the_slot_it_went_to() {
        assert_eq!(restore([SKYWARD, OLD], [SKYWARD, STOCK], SKYWARD, Some(Slot::B)), [Slot::B]);
        assert_eq!(restore([SKYWARD, OLD], [SKYWARD, OLD], SKYWARD, Some(Slot::B)), [Slot::B], "an older recovery there, left by the update");
        assert!(restore([SKYWARD, SKYWARD], [SKYWARD, SKYWARD], SKYWARD, Some(Slot::A)).is_empty(), "this recovery is there already");
    }

    #[test]
    fn a_script_gets_it_back_only_where_it_wrote() {
        assert!(restore([SKYWARD, OLD], [SKYWARD, OLD], SKYWARD, None).is_empty(), "a script that left recovery alone, such as a root installer");
        assert_eq!(restore([SKYWARD, OLD], [STOCK, OLD], SKYWARD, None), [Slot::A]);
        assert_eq!(restore([SKYWARD, OLD], [STOCK, STOCK], SKYWARD, None), [Slot::A, Slot::B]);
        assert!(restore([OLD, OLD], [SKYWARD, OLD], SKYWARD, None).is_empty(), "a script that wrote this recovery itself");
    }

    #[test]
    fn a_partition_holding_something_else_is_refused_and_digested_whole() {
        let dir = tempfile::tempdir().unwrap();
        let erased = dir.path().join("recovery_b");
        std::fs::write(&erased, vec![0u8; 8192]).unwrap();
        assert!(own(&erased).is_err());
        assert_eq!(digest(&erased).unwrap(), <[u8; 32]>::from(Sha256::digest(vec![0u8; 8192])));
    }
}
