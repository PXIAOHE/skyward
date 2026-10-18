//! The table a phone keeps of its own filesystems: where each one is, what
//! it holds, and how it is encrypted.

use anyhow::{Context, Result};
use std::path::Path;

/// Where the phone's own partitions keep the table.
pub const FILES: [&str; 2] = ["/vendor/etc/fstab.qcom", "/odm/etc/fstab.qcom"];

/// One filesystem the phone knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub device: String,
    pub mount_point: String,
    pub filesystem: String,
    /// What the phone says about the filesystem beyond how to mount it,
    /// such as how it is encrypted.
    pub extras: Vec<String>,
}

impl Entry {
    /// The value of an extra written as `name=value`.
    pub fn extra(&self, name: &str) -> Option<&str> {
        let prefix = format!("{name}=");
        self.extras.iter().find_map(|e| e.strip_prefix(&prefix))
    }
}

/// Every filesystem a table names. A phone may name the same mount point
/// more than once, once per filesystem it might hold.
pub fn entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [device, mount_point, filesystem, _flags, extras] = fields[..] else { continue };
        out.push(Entry {
            device: device.to_string(),
            mount_point: mount_point.to_string(),
            filesystem: filesystem.to_string(),
            extras: extras.split(',').map(str::to_string).collect(),
        });
    }
    out
}

/// What the phone's own table says about `mount_point`, read from wherever
/// the table is.
pub fn read(mount_point: &str) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for file in FILES {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("read {file}")),
        };
        out.extend(entries(&text).into_iter().filter(|e| e.mount_point == mount_point));
    }
    Ok(out)
}

/// The partition an entry names, as it is reachable now. A phone's table
/// points at its storage controller by a path a recovery need not have.
pub fn device(entry: &Entry) -> Result<String> {
    if Path::new(&entry.device).exists() {
        return Ok(entry.device.clone());
    }
    let name = Path::new(&entry.device).file_name().with_context(|| format!("{} names no partition", entry.device))?;
    let by_name = Path::new(crate::BY_NAME).join(name);
    if by_name.exists() {
        return Ok(by_name.to_string_lossy().into_owned());
    }
    anyhow::bail!("neither {} nor {} is here", entry.device, by_name.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
# a comment
/dev/block/bootdevice/by-name/metadata /metadata f2fs noatime,nosuid wait,check,formattable
/dev/block/bootdevice/by-name/userdata /data f2fs noatime,discard latemount,wait,fileencryption=aes-256-xts:aes-256-cts:v2,keydirectory=/metadata/vold/metadata_encryption,metadata_encryption=aes-256-xts:wrappedkey_v0,quota
/dev/block/bootdevice/by-name/userdata /data mifs noatime,discard latemount,wait,keydirectory=/metadata/vold/metadata_encryption
";

    #[test]
    fn a_mount_point_can_be_named_once_per_filesystem_it_might_hold() {
        let data: Vec<Entry> = entries(TABLE).into_iter().filter(|e| e.mount_point == "/data").collect();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].filesystem, "f2fs");
        assert_eq!(data[1].filesystem, "mifs");
        assert_eq!(data[0].device, "/dev/block/bootdevice/by-name/userdata");
    }

    #[test]
    fn what_the_phone_says_about_encryption_is_read_off_the_entry() {
        let data = &entries(TABLE)[1];
        assert_eq!(data.extra("metadata_encryption"), Some("aes-256-xts:wrappedkey_v0"));
        assert_eq!(data.extra("keydirectory"), Some("/metadata/vold/metadata_encryption"));
        assert_eq!(data.extra("quota"), None, "a word on its own has no value");
        assert_eq!(data.extra("keydir"), None, "a shorter name is another extra");
    }

    #[test]
    fn comments_and_short_lines_are_passed_over() {
        assert!(entries("# nothing\n\n/dev/x /y f2fs rw\n").is_empty());
        assert_eq!(entries(TABLE).len(), 3);
    }
}
