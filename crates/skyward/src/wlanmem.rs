//! The WLAN a user chose to have kept: whether it is opened as the recovery
//! starts, whether the password of the network joined last is kept, and that
//! network with its password.
//!
//! Kept beside the recovery's own settings, readable by root alone. Turning
//! the password off takes the network away with it, and so does a factory
//! reset.

// A recovery-only module: the host build quotes none of it.
#![allow(dead_code)]

use anyhow::{Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// The partition the file is on, made writable for as long as a write takes.
const METADATA: &str = "/metadata";

/// Where it is kept: beside the recovery's own settings.
pub const FILE: &str = "/metadata/skyward/wlan";

/// A network kept, with the password it was joined with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub ssid: Vec<u8>,
    pub password: String,
}

/// What the file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kept {
    /// Open the WLAN as the recovery starts.
    pub auto: bool,
    /// Keep the password of the network joined last.
    pub keep: bool,
    /// That network, where one is kept.
    pub saved: Option<Saved>,
}

/// What is kept, as the file holds it.
pub fn read() -> Kept {
    let text = match std::fs::read_to_string(FILE) {
        Ok(text) => text,
        Err(_) => return Kept::default(),
    };
    let mut kept = Kept::default();
    let (mut ssid, mut password) = (None, None);
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "auto" => kept.auto = value.trim() == "1",
            "keep" => kept.keep = value.trim() == "1",
            "ssid" => ssid = unhex(value.trim()),
            "password" => password = Some(value.to_string()),
            _ => {}
        }
    }
    if kept.keep {
        if let (Some(ssid), Some(password)) = (ssid, password) {
            if !ssid.is_empty() && !password.is_empty() {
                kept.saved = Some(Saved { ssid, password });
            }
        }
    }
    kept
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

fn store(text: &str) -> Result<()> {
    writable(|| {
        let at = Path::new(FILE);
        if let Some(dir) = at.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
        }
        std::fs::write(at, text).with_context(|| format!("write {}", at.display()))?;
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("keep {} to root alone", at.display()))?;
        Ok(())
    })
}

fn render(kept: &Kept) -> String {
    let mut text = format!("auto={}\nkeep={}\n", u8::from(kept.auto), u8::from(kept.keep));
    // A network kept without the setting is not kept at all: the password goes
    // with the setting.
    if let Some(saved) = kept.saved.as_ref().filter(|_| kept.keep) {
        text.push_str(&format!("ssid={}\npassword={}\n", hex(&saved.ssid), saved.password));
    }
    text
}

/// Set whether the WLAN opens as the recovery starts, leaving the rest as it
/// was.
pub fn set_auto(on: bool) -> Result<()> {
    let mut kept = read();
    kept.auto = on;
    store(&render(&kept))
}

/// Set whether the password is kept. Turning it off takes the network away.
pub fn set_keep(on: bool) -> Result<()> {
    let mut kept = read();
    kept.keep = on;
    if !on {
        kept.saved = None;
    }
    store(&render(&kept))
}

/// Keep `ssid` and the password it was joined with.
pub fn save(ssid: &[u8], password: &str) -> Result<()> {
    let mut kept = read();
    kept.keep = true;
    kept.saved = Some(Saved { ssid: ssid.to_vec(), password: password.to_string() });
    store(&render(&kept))
}

/// Keep the settings but forget the network, as after a password the access
/// point refused.
pub fn forget_network() -> Result<()> {
    let mut kept = read();
    kept.saved = None;
    store(&render(&kept))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_goes_round_the_file_as_bytes() {
        assert_eq!(unhex(&hex("Skyward客厅".as_bytes())).as_deref(), Some("Skyward客厅".as_bytes()));
    }

    #[test]
    fn a_password_keeps_what_it_holds() {
        assert_eq!(unhex(&hex(b"pa ss=word")), Some(b"pa ss=word".to_vec()));
    }

    #[test]
    fn half_a_byte_is_no_name() {
        assert_eq!(unhex("abc"), None);
    }

    #[test]
    fn what_is_written_is_what_is_read() {
        let kept = Kept { auto: true, keep: true, saved: Some(Saved { ssid: b"Sarva5".to_vec(), password: "p=w d".into() }) };
        let text = render(&kept);
        assert_eq!(text, "auto=1\nkeep=1\nssid=536172766135\npassword=p=w d\n");
    }

    #[test]
    fn a_network_kept_without_the_setting_is_not_kept() {
        let mut kept = Kept { auto: true, keep: false, saved: Some(Saved { ssid: b"x".to_vec(), password: "y".into() }) };
        assert_eq!(render(&kept), "auto=1\nkeep=0\n");
        kept.saved = None;
        assert_eq!(render(&kept), "auto=1\nkeep=0\n");
    }
}
