//! Checking a filesystem before it is mounted writable, the way a phone
//! checks its own as it starts.

use anyhow::{bail, Context, Result};
use std::path::Path;

/// The program a recovery ramdisk carries for checking f2fs.
const FSCK_F2FS: &str = "/system/bin/fsck.f2fs";

/// How a check ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The program's exit status: 0 when it found nothing to fix.
    pub status: i32,
    /// What it printed.
    pub said: String,
}

/// Check `device`, which holds `filesystem`, as the phone's fs_mgr does
/// before it mounts one, and fix what the kernel marked as needing it.
pub fn check(device: &Path, filesystem: &str) -> Result<Checked> {
    let command = command(device, filesystem)?;
    let out = std::process::Command::new(&command[0])
        .args(&command[1..])
        .output()
        .with_context(|| format!("run {}", command[0]))?;
    let status = out.status.code().with_context(|| format!("{} ended with {}", command.join(" "), out.status))?;
    let said = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    Ok(Checked { status, said })
}

/// The program and its arguments, as fs_mgr runs them. The whole check
/// that the phone's own starts make once a month, timed by a clock the
/// filesystem keeps, is left to them: made here, it would move their mark.
fn command(device: &Path, filesystem: &str) -> Result<Vec<String>> {
    let device = device.to_str().with_context(|| format!("{} is not a path in UTF-8", device.display()))?;
    match filesystem {
        "f2fs" => Ok([FSCK_F2FS, "-a", "--no-kernel-check", "-c", "10000", "--debug-cache", device].map(str::to_string).to_vec()),
        other => bail!("{device} holds {other}, which the recovery does not check before writing to it"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f2fs_is_checked_as_the_phone_checks_it_less_the_monthly_whole_check() {
        let c = command(Path::new("/dev/block/dm-8"), "f2fs").unwrap();
        assert_eq!(c, ["/system/bin/fsck.f2fs", "-a", "--no-kernel-check", "-c", "10000", "--debug-cache", "/dev/block/dm-8"]);
    }

    #[test]
    fn a_filesystem_the_recovery_does_not_check_is_refused() {
        assert!(command(Path::new("/dev/x"), "ext4").is_err());
    }
}
