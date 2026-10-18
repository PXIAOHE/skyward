//! Erasing a partition, and making afresh the filesystem a phone's table
//! names for one, the way a phone's own recovery formats it.

use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

/// The programs a recovery ramdisk carries for making the two filesystems.
const MAKE_F2FS: &str = "/system/bin/make_f2fs";
const MAKE_EXT4: &str = "/system/bin/mke2fs";

/// How much of a partition's start is written over with zeros once it is
/// discarded: well past where any filesystem a phone uses keeps its
/// superblocks.
#[cfg(target_os = "android")]
const ZEROED: u64 = 1 << 20;

/// Discard all a partition holds, as a phone's own recovery erases one
/// that the system opens through metadata encryption, then write zeros
/// over its start. A partition that starts with zeros is one the system
/// formats afresh, under a new key, as it mounts it; what discarded blocks
/// read back as is the storage's own choice, so the zeros are written.
#[cfg(target_os = "android")]
pub fn discard(device: &Path) -> Result<()> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::FileExt;
    use std::os::unix::io::AsRawFd;
    // _IO(0x12, 125) and _IO(0x12, 119); each takes the start and length.
    const BLKSECDISCARD: libc::c_int = 0x127d;
    const BLKDISCARD: libc::c_int = 0x1277;
    let size = crate::size_bytes(device)?;
    let f = OpenOptions::new().write(true).open(device).with_context(|| format!("open {}", device.display()))?;
    let range = [0u64, size];
    // The secure kind leaves nothing behind in spare blocks either; not
    // all storage offers it.
    if unsafe { libc::ioctl(f.as_raw_fd(), BLKSECDISCARD, range.as_ptr()) } != 0 {
        let secure = std::io::Error::last_os_error();
        if unsafe { libc::ioctl(f.as_raw_fd(), BLKDISCARD, range.as_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("discard {}, after a secure discard failed with {secure}", device.display()));
        }
    }
    let zeros = vec![0u8; ZEROED.min(size) as usize];
    f.write_all_at(&zeros, 0).with_context(|| format!("write zeros over the start of {}", device.display()))?;
    f.sync_all().with_context(|| format!("sync {}", device.display()))
}

#[cfg(not(target_os = "android"))]
pub fn discard(device: &Path) -> Result<()> {
    bail!("this host has no block device {} to discard", device.display())
}

/// Make the filesystem `filesystem` on `device`; `extras` are what the
/// phone's table says of it beyond how to mount it.
pub fn make(device: &Path, filesystem: &str, extras: &[String]) -> Result<()> {
    let command = command(device, filesystem, extras)?;
    let out = std::process::Command::new(&command[0])
        .args(&command[1..])
        .output()
        .with_context(|| format!("run {}", command[0]))?;
    ensure!(
        out.status.success(),
        "{} ended with {}: {}",
        command.join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// The program and its arguments that make a filesystem, as a phone's own
/// recovery runs them: f2fs with Android's defaults and quota by project,
/// ext4 with 4 KiB blocks and inodes wide enough for project quota, and
/// checksums over its metadata where the table asks for them.
fn command(device: &Path, filesystem: &str, extras: &[String]) -> Result<Vec<String>> {
    let mut command: Vec<&str> = match filesystem {
        "f2fs" => vec![MAKE_F2FS, "-g", "android", "-O", "project_quota,extra_attr"],
        "ext4" => {
            let mut c = vec![MAKE_EXT4, "-F", "-t", "ext4", "-b", "4096", "-I", "512"];
            if extras.iter().any(|e| e == "metadata_csum") {
                c.extend(["-O", "metadata_csum", "-O", "64bit", "-O", "extent"]);
            }
            c
        }
        other => bail!("{} is to hold {other}, which the recovery cannot make", device.display()),
    };
    let device = device.to_str().with_context(|| format!("{} is not a path in UTF-8", device.display()))?;
    command.push(device);
    Ok(command.into_iter().map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f2fs_is_made_with_androids_defaults_and_project_quota() {
        let c = command(Path::new("/dev/block/by-name/metadata"), "f2fs", &[]).unwrap();
        assert_eq!(c, ["/system/bin/make_f2fs", "-g", "android", "-O", "project_quota,extra_attr", "/dev/block/by-name/metadata"]);
    }

    #[test]
    fn ext4_takes_metadata_checksums_where_the_table_asks() {
        let plain = command(Path::new("/dev/x"), "ext4", &["check".into()]).unwrap();
        assert_eq!(plain, ["/system/bin/mke2fs", "-F", "-t", "ext4", "-b", "4096", "-I", "512", "/dev/x"]);
        let summed = command(Path::new("/dev/x"), "ext4", &["metadata_csum".into()]).unwrap();
        assert_eq!(&summed[8..], ["-O", "metadata_csum", "-O", "64bit", "-O", "extent", "/dev/x"]);
    }

    #[test]
    fn a_filesystem_the_recovery_cannot_make_is_refused() {
        assert!(command(Path::new("/dev/x"), "erofs", &[]).is_err());
    }
}
