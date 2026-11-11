#![allow(dead_code)]

//! The screen lock a user chose to have kept, so that the data partition
//! opens as the recovery starts rather than waiting for it to be typed.
//!
//! It is kept where the recovery keeps its own settings, readable by root
//! alone. A factory reset formats that partition, and so does the way out of
//! the decrypt page that forgets it.

use anyhow::{Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// The partition the file is on. The recovery mounts it read-only, as the
/// phone's own recovery does, so it is made writable for as long as this
/// takes and then read-only again.
const METADATA: &str = "/metadata";

/// Where it is kept: beside the recovery's own settings.
pub const FILE: &str = "/metadata/skyward/credential";

/// The screen lock kept from before, where one was kept.
pub fn read() -> Option<Vec<u8>> {
    std::fs::read(FILE).ok().filter(|kept| !kept.is_empty())
}

/// Do `work` with the partition writable, and read-only again afterwards.
fn writable<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    let at = Path::new(METADATA);
    let mounted = skyward_block::mount::is_mount_point(at)?;
    if mounted {
        skyward_block::mount::remount_writable(at, true).context("make /metadata writable")?;
    }
    let done = work();
    if mounted {
        let _ = skyward_block::mount::remount_writable(at, false);
    }
    done
}

/// Keep `credential` for the starts that follow.
pub fn write(credential: &[u8]) -> Result<()> {
    writable(|| {
        let at = Path::new(FILE);
        if let Some(dir) = at.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
        }
        std::fs::write(at, credential).with_context(|| format!("write {}", at.display()))?;
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("keep {} to root alone", at.display()))?;
        Ok(())
    })
}

/// Forget the screen lock kept from before, if there is one.
pub fn forget() -> Result<()> {
    writable(|| match std::fs::remove_file(FILE) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("remove {FILE}")),
    })
}
