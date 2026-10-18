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

/// Where Android keeps the memory tagging the bootloader is to turn on: in
/// the system's own space of misc, which starts at 32 KiB, after the 64
/// bytes of the virtual A/B message.
const MEMTAG_MESSAGE: Range<u64> = 32 * 1024 + 64..32 * 1024 + 128;

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

    /// What the message asks the recovery to do, read the way Android's
    /// own recovery reads it: of several pieces of work it does the first
    /// in this order, fastboot, an update package, erasing data, asking
    /// whether to erase data, erasing the cache, erasing by a wipe package,
    /// sideload, rescue, unsharing blocks, leaving at once. A command with
    /// no arguments can ask for fastboot or rescue by itself.
    pub fn request(&self) -> Request {
        let mut r = Request::default();
        let (mut fastboot, mut wipe_data, mut prompt, mut wipe_cache, mut exit) = (false, false, false, false, false);
        let mut sideload = None;
        // Work of its own the recovery does not do, at the place of each in
        // the order above.
        let (mut update, mut reformat, mut wipe_ab, mut rescue, mut unshare) = (None, None, None, None, None);
        for arg in &self.args {
            let Some(option) = arg.strip_prefix("--") else {
                r.unknown.push(arg.clone());
                continue;
            };
            let (name, value) = match option.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (option, None),
            };
            match name {
                "fastboot" => fastboot = true,
                // The second is how the stock system of some phones asks
                // for a factory reset.
                "wipe_data" | "format_data" => wipe_data = true,
                "prompt_and_wipe_data" => prompt = true,
                "wipe_cache" => wipe_cache = true,
                "sideload" => sideload = sideload.or(Some(false)),
                "sideload_auto_reboot" => sideload = Some(true),
                "just_exit" => exit = true,
                "reason" => r.reason = value.map(str::to_string),
                "locale" => r.locale = value.map(str::to_string),
                "shutdown_after" => r.shutdown_after = true,
                "keep_memtag_mode" => r.keep_memtag_mode = true,
                "update_package" => update = update.or(Some(arg.clone())),
                "reformat_data" => reformat = reformat.or(Some(arg.clone())),
                "wipe_ab" => wipe_ab = wipe_ab.or(Some(arg.clone())),
                "rescue" => rescue = rescue.or(Some(arg.clone())),
                "fsck_unshare_blocks" => unshare = unshare.or(Some(arg.clone())),
                // What only shapes work of those kinds, or how the text of
                // Android's own recovery shows.
                "install_with_fuse" | "retry_count" | "security" | "update_wipe" | "export_validate" | "wipe_package_size"
                | "show_text" => {}
                _ => r.unknown.push(arg.clone()),
            }
        }
        r.work = if fastboot {
            Work::Fastboot
        } else if let Some(arg) = update {
            Work::Unsupported(arg)
        } else if let Some(arg) = reformat {
            Work::Unsupported(arg)
        } else if wipe_data {
            Work::WipeData
        } else if prompt {
            Work::PromptWipeData
        } else if wipe_cache {
            Work::WipeCache
        } else if let Some(arg) = wipe_ab {
            Work::Unsupported(arg)
        } else if let Some(auto_reboot) = sideload {
            Work::Sideload { auto_reboot }
        } else if let Some(arg) = rescue.or(unshare) {
            Work::Unsupported(arg)
        } else if exit {
            Work::Exit
        } else if !r.unknown.is_empty() {
            Work::Unsupported(r.unknown.join(" "))
        } else if self.args.is_empty() && self.command == "boot-fastboot" {
            Work::Fastboot
        } else if self.args.is_empty() && self.command == "boot-rescue" {
            Work::Unsupported(self.command.clone())
        } else {
            Work::Menu
        };
        r
    }
}

/// What a boot message asks the recovery to do: one piece of work, and how
/// to go about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    pub work: Work,
    /// Why the system asked, as it says it, such as RescueParty.
    pub reason: Option<String>,
    /// The language the system runs in.
    pub locale: Option<String>,
    /// Power off once the work is done, instead of starting the system.
    pub shutdown_after: bool,
    /// Erasing data leaves alone the memory tagging asked of the
    /// bootloader.
    pub keep_memtag_mode: bool,
    /// Arguments the recovery knows nothing of.
    pub unknown: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Work {
    /// Nothing: the recovery's own pages.
    #[default]
    Menu,
    /// The recovery's userspace fastboot.
    Fastboot,
    /// Erase all user data, as a factory reset does.
    WipeData,
    /// Say that Android cannot start, and offer to erase all user data.
    PromptWipeData,
    WipeCache,
    /// Wait for `adb sideload`; with `auto_reboot`, start the system once
    /// the package is in or has failed.
    Sideload { auto_reboot: bool },
    /// Start the system at once.
    Exit,
    /// Work the recovery does not do, as it was asked.
    Unsupported(String),
}

impl Work {
    /// Whether the work goes on past the recovery's start. Its request
    /// then stays in misc until it is done, so a restart halfway comes back
    /// to it, as Android's own recovery does.
    pub fn lasts(&self) -> bool {
        matches!(self, Work::WipeData | Work::PromptWipeData | Work::WipeCache | Work::Sideload { .. })
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

/// Read the boot request and clear it. The caller keeps the request; any
/// later reboot (adb reboot, the power key, a crash) then boots what its
/// reboot reason names instead of coming back to recovery.
pub fn take(misc: &Path) -> Result<Bcb> {
    let bcb = read(misc)?;
    clear(misc)?;
    Ok(bcb)
}

/// Zero the bootloader message so the next boot follows its reboot reason
/// instead of a pending command.
pub fn clear(misc: &Path) -> Result<()> {
    let f = OpenOptions::new().write(true).open(misc).with_context(|| format!("open {}", misc.display()))?;
    f.write_all_at(&[0u8; SIZE], 0).with_context(|| format!("write {}", misc.display()))?;
    f.sync_all().with_context(|| format!("sync {}", misc.display()))
}

/// Leave the memory tagging message empty, as erasing data does unless it
/// is asked to keep it: the bootloader then turns on no tagging it was
/// asked for before.
pub fn forget_memtag_mode(misc: &Path) -> Result<()> {
    let f = OpenOptions::new().write(true).open(misc).with_context(|| format!("open {}", misc.display()))?;
    let empty = [0u8; (MEMTAG_MESSAGE.end - MEMTAG_MESSAGE.start) as usize];
    f.write_all_at(&empty, MEMTAG_MESSAGE.start).with_context(|| format!("write {}", misc.display()))?;
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

    fn asked(command: &str, args: &[&str]) -> Request {
        Bcb { command: command.into(), args: args.iter().map(|a| a.to_string()).collect() }.request()
    }

    #[test]
    fn a_factory_reset_from_the_settings_erases_data() {
        let r = asked("boot-recovery", &["--format_data", "--locale=zh-CN", "--reason=factroy_reset(1217)"]);
        assert_eq!(r.work, Work::WipeData);
        assert_eq!(r.locale.as_deref(), Some("zh-CN"));
        assert_eq!(r.reason.as_deref(), Some("factroy_reset(1217)"));
        assert!(!r.shutdown_after && !r.keep_memtag_mode && r.unknown.is_empty());
    }

    #[test]
    fn a_wipe_asked_by_the_system_says_how_to_end() {
        let reason = "--reason=MasterClearConfirm,2026-10-20T01:00:00Z";
        let r = asked("boot-recovery", &["--wipe_data", "--shutdown_after", reason, "--keep_memtag_mode", "--locale=en-US"]);
        assert_eq!(r.work, Work::WipeData);
        assert!(r.shutdown_after && r.keep_memtag_mode);
        assert_eq!(r.reason.as_deref(), Some("MasterClearConfirm,2026-10-20T01:00:00Z"));
    }

    #[test]
    fn each_kind_of_work_is_read() {
        assert_eq!(asked("boot-recovery", &["--prompt_and_wipe_data", "--reason=RescueParty"]).work, Work::PromptWipeData);
        assert_eq!(asked("boot-recovery", &["--wipe_cache"]).work, Work::WipeCache);
        assert_eq!(asked("boot-recovery", &["--sideload"]).work, Work::Sideload { auto_reboot: false });
        assert_eq!(asked("boot-recovery", &["--sideload_auto_reboot"]).work, Work::Sideload { auto_reboot: true });
        assert_eq!(asked("boot-recovery", &["--fastboot"]).work, Work::Fastboot);
        assert_eq!(asked("boot-recovery", &["--just_exit"]).work, Work::Exit);
        assert_eq!(asked("boot-recovery", &[]).work, Work::Menu);
        assert_eq!(asked("boot-recovery", &["--locale=zh-CN"]).work, Work::Menu, "a language alone asks for nothing");
    }

    #[test]
    fn the_first_work_in_order_is_the_one_done() {
        assert_eq!(asked("boot-recovery", &["--sideload", "--wipe_data"]).work, Work::WipeData);
        assert_eq!(asked("boot-recovery", &["--wipe_data", "--fastboot"]).work, Work::Fastboot);
        assert_eq!(
            asked("boot-recovery", &["--wipe_data", "--update_package=/data/ota_package/update.zip"]).work,
            Work::Unsupported("--update_package=/data/ota_package/update.zip".into())
        );
        assert_eq!(asked("boot-recovery", &["--wipe_ab", "--wipe_package_size=4096", "--sideload"]).work, Work::Unsupported("--wipe_ab".into()));
        assert_eq!(asked("boot-recovery", &["--wipe_cache", "--wipe_ab"]).work, Work::WipeCache);
    }

    #[test]
    fn a_command_alone_can_ask_for_fastboot() {
        assert_eq!(asked("boot-fastboot", &[]).work, Work::Fastboot);
        assert_eq!(asked("boot-fastboot", &["--wipe_data"]).work, Work::WipeData, "arguments come first");
        assert_eq!(asked("boot-rescue", &[]).work, Work::Unsupported("boot-rescue".into()));
    }

    #[test]
    fn arguments_the_recovery_does_not_know() {
        let alone = asked("boot-recovery", &["--wipe_efs"]);
        assert_eq!(alone.work, Work::Unsupported("--wipe_efs".into()), "asked for on their own, they are shown");
        let beside = asked("boot-recovery", &["--wipe_data", "--keep_photos", "stray"]);
        assert_eq!(beside.work, Work::WipeData, "beside work the recovery knows, they change nothing");
        assert_eq!(beside.unknown, ["--keep_photos", "stray"]);
    }

    #[test]
    fn work_that_goes_on_keeps_its_request() {
        for work in [Work::WipeData, Work::PromptWipeData, Work::WipeCache, Work::Sideload { auto_reboot: true }] {
            assert!(work.lasts(), "{work:?}");
        }
        for work in [Work::Menu, Work::Fastboot, Work::Exit, Work::Unsupported("--rescue".into())] {
            assert!(!work.lasts(), "{work:?}");
        }
    }

    #[test]
    fn forgetting_memtag_mode_empties_only_its_message() {
        let path = std::env::temp_dir().join(format!("skyward-bcb-memtag-{}", std::process::id()));
        std::fs::write(&path, [0xffu8; 64 * 1024]).unwrap();
        forget_memtag_mode(&path).unwrap();
        let after = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let (start, end) = (MEMTAG_MESSAGE.start as usize, MEMTAG_MESSAGE.end as usize);
        assert_eq!((start, end - start), (32832, 64));
        assert!(after[start..end].iter().all(|&b| b == 0));
        assert!(after[..start].iter().chain(&after[end..]).all(|&b| b == 0xff));
    }

    #[test]
    fn clear_zeroes_only_the_message() {
        let path = std::env::temp_dir().join(format!("skyward-bcb-clear-{}", std::process::id()));
        std::fs::write(&path, [0xffu8; 2 * SIZE]).unwrap();
        clear(&path).unwrap();
        let after = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(after[..SIZE].iter().all(|&b| b == 0));
        assert!(after[SIZE..].iter().all(|&b| b == 0xff));
    }

    #[test]
    fn take_returns_the_request_and_clears_it() {
        let path = std::env::temp_dir().join(format!("skyward-bcb-take-{}", std::process::id()));
        let mut misc = message(b"boot-recovery", b"recovery\n--fastboot\n");
        misc.extend_from_slice(&[0xffu8; SIZE]);
        std::fs::write(&path, &misc).unwrap();
        let bcb = take(&path).unwrap();
        let after = read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(bcb, Bcb { command: "boot-recovery".into(), args: vec!["--fastboot".into()] });
        assert_eq!(after, Bcb::default());
    }
}
