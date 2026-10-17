use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::ops::Range;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// The bootloader message at the start of misc: command[32], status[32],
/// recovery[768], stage[32] and reserved[1184].
const SIZE: usize = 2048;
const COMMAND: Range<usize> = 0..32;
const RECOVERY: Range<usize> = 64..832;

/// The misc partition, shared by the bootloader, Android and recovery.
pub fn misc() -> PathBuf {
    Path::new(skyward_block::BY_NAME).join("misc")
}

/// What the bootloader control block asks for: the command the bootloader
/// acts on, such as "boot-recovery", and the arguments recovery should run
/// with, such as "--wipe_data".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bcb {
    pub command: String,
    pub args: Vec<String>,
}

impl Bcb {
    pub fn parse(raw: &[u8]) -> Result<Bcb> {
        if raw.len() < SIZE {
            bail!("a bootloader message takes {SIZE} bytes, got {}", raw.len());
        }
        let command = text(&raw[COMMAND]).context("misc command")?.to_string();
        let recovery = text(&raw[RECOVERY]).context("misc recovery message")?;
        let mut lines = recovery.split('\n');
        let args = match lines.next() {
            None | Some("") => Vec::new(),
            Some("recovery") => lines.filter(|l| !l.is_empty()).map(str::to_string).collect(),
            Some(_) => bail!("misc holds an unknown recovery message {recovery:?}"),
        };
        Ok(Bcb { command, args })
    }
}

/// A NUL-terminated field; bytes after the first NUL are leftovers of an
/// earlier, longer value.
fn text(field: &[u8]) -> Result<&str> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..end]).context("not UTF-8")
}

pub fn read(misc: &Path) -> Result<Bcb> {
    let mut raw = [0u8; SIZE];
    File::open(misc)
        .and_then(|f| f.read_exact_at(&mut raw, 0))
        .with_context(|| format!("read {}", misc.display()))?;
    Bcb::parse(&raw)
}

/// Zero the bootloader message, as recovery does before it reboots, so the
/// next boot follows the reboot target instead of a pending command.
pub fn clear(misc: &Path) -> Result<()> {
    let f = OpenOptions::new().write(true).open(misc).with_context(|| format!("open {}", misc.display()))?;
    f.write_all_at(&[0u8; SIZE], 0).with_context(|| format!("write {}", misc.display()))?;
    f.sync_all().with_context(|| format!("sync {}", misc.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(command: &[u8], recovery: &[u8]) -> Vec<u8> {
        let mut raw = vec![0u8; SIZE];
        raw[..command.len()].copy_from_slice(command);
        raw[RECOVERY.start..RECOVERY.start + recovery.len()].copy_from_slice(recovery);
        raw
    }

    #[test]
    fn reads_recovery_arguments() {
        let raw = message(b"boot-recovery", b"recovery\n--wipe_data\n--locale=zh_CN\n");
        let bcb = Bcb::parse(&raw).unwrap();
        assert_eq!(bcb.command, "boot-recovery");
        assert_eq!(bcb.args, ["--wipe_data", "--locale=zh_CN"]);
    }

    #[test]
    fn ignores_leftovers_after_nul() {
        // The 13 Ultra's misc after the bootloader wrote boot-recovery over
        // bootonce-bootloader.
        let bcb = Bcb::parse(&message(b"boot-recovery\0oader", b"")).unwrap();
        assert_eq!(bcb, Bcb { command: "boot-recovery".into(), args: Vec::new() });
    }

    #[test]
    fn empty_message_asks_for_nothing() {
        assert_eq!(Bcb::parse(&[0u8; SIZE]).unwrap(), Bcb::default());
    }

    #[test]
    fn refuses_unknown_recovery_message() {
        assert!(Bcb::parse(&message(b"boot-recovery", b"--wipe_data\n")).is_err());
    }

    #[test]
    fn clear_zeroes_only_the_message() {
        let path = std::env::temp_dir().join(format!("skyward-bcb-{}", std::process::id()));
        std::fs::write(&path, [0xffu8; 2 * SIZE]).unwrap();
        clear(&path).unwrap();
        let after = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(after[..SIZE].iter().all(|&b| b == 0));
        assert!(after[SIZE..].iter().all(|&b| b == 0xff));
    }
}
