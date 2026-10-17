// The table encoding serves the Android ioctls and the host tests.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use anyhow::{ensure, Result};
use std::mem::size_of;
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout};

/// One target of a device-mapper table, in 512-byte sectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// `sectors` sectors of block device `device` (major, minor), starting
    /// at its sector `offset`.
    Linear { sectors: u64, device: (u32, u32), offset: u64 },
    Zero { sectors: u64 },
}

impl Target {
    pub fn sectors(&self) -> u64 {
        match *self {
            Target::Linear { sectors, .. } | Target::Zero { sectors } => sectors,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Target::Linear { .. } => "linear",
            Target::Zero { .. } => "zero",
        }
    }

    fn params(&self) -> String {
        match *self {
            Target::Linear { device: (major, minor), offset, .. } => format!("{major}:{minor} {offset}"),
            Target::Zero { .. } => String::new(),
        }
    }
}

// From <linux/dm-ioctl.h>.
const DM_NAME_LEN: usize = 128;
const DM_UUID_LEN: usize = 129;
const DM_MAX_TYPE_NAME: usize = 16;
const DM_READONLY_FLAG: u32 = 1 << 0;
/// The oldest interface version with every command used here.
const DM_VERSION: [u32; 3] = [4, 0, 0];

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
struct DmIoctl {
    version: [u32; 3],
    data_size: u32,
    data_start: u32,
    target_count: u32,
    open_count: i32,
    flags: u32,
    event_nr: u32,
    padding: u32,
    dev: u64,
    name: [u8; DM_NAME_LEN],
    uuid: [u8; DM_UUID_LEN],
    data: [u8; 7],
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
struct DmTargetSpec {
    sector_start: u64,
    length: u64,
    status: i32,
    /// Bytes from this spec to the next one.
    next: u32,
    target_type: [u8; DM_MAX_TYPE_NAME],
}

const _: () = {
    assert!(size_of::<DmIoctl>() == 312);
    assert!(size_of::<DmTargetSpec>() == 40);
};

/// `_IOWR(DM_IOCTL, nr, struct dm_ioctl)`.
const fn request(nr: u32) -> u32 {
    (3 << 30) | ((size_of::<DmIoctl>() as u32) << 16) | (0xfd << 8) | nr
}

const DM_DEV_CREATE: u32 = request(3);
const DM_DEV_REMOVE: u32 = request(4);
const DM_DEV_SUSPEND: u32 = request(6);
const DM_DEV_STATUS: u32 = request(7);
const DM_TABLE_LOAD: u32 = request(9);

impl DmIoctl {
    fn new(name: &str) -> Result<DmIoctl> {
        ensure!(!name.is_empty() && name.len() < DM_NAME_LEN, "device-mapper name {name:?}");
        let mut io = DmIoctl::new_zeroed();
        io.version = DM_VERSION;
        io.data_size = size_of::<DmIoctl>() as u32;
        io.name[..name.len()].copy_from_slice(name.as_bytes());
        Ok(io)
    }
}

/// The DM_TABLE_LOAD argument: the ioctl header, then each target's spec and
/// NUL-terminated parameters, padded to 8 bytes.
fn table_load(name: &str, targets: &[Target], readonly: bool) -> Result<Vec<u8>> {
    ensure!(!targets.is_empty(), "{name} has no targets");
    let mut body = Vec::new();
    let mut start = 0;
    for t in targets {
        let params = t.params();
        let len = (size_of::<DmTargetSpec>() + params.len() + 1).next_multiple_of(8);
        let mut spec = DmTargetSpec::new_zeroed();
        spec.sector_start = start;
        spec.length = t.sectors();
        spec.next = len as u32;
        spec.target_type[..t.kind().len()].copy_from_slice(t.kind().as_bytes());
        let at = body.len();
        body.extend_from_slice(spec.as_bytes());
        body.extend_from_slice(params.as_bytes());
        body.resize(at + len, 0);
        start += t.sectors();
    }
    let mut io = DmIoctl::new(name)?;
    io.data_start = size_of::<DmIoctl>() as u32;
    io.data_size = (size_of::<DmIoctl>() + body.len()) as u32;
    io.target_count = targets.len() as u32;
    if readonly {
        io.flags |= DM_READONLY_FLAG;
    }
    let mut buf = io.as_bytes().to_vec();
    buf.extend_from_slice(&body);
    Ok(buf)
}

#[cfg(target_os = "android")]
mod sys {
    use super::*;
    use anyhow::{bail, Context};
    use std::fs::{File, OpenOptions};
    use std::os::unix::io::AsRawFd;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    const CONTROL: &str = "/dev/device-mapper";
    /// How long ueventd gets to make the node of a new device.
    const NODE_TIMEOUT: Duration = Duration::from_secs(5);

    fn control() -> Result<File> {
        OpenOptions::new().read(true).write(true).open(CONTROL).with_context(|| format!("open {CONTROL}"))
    }

    fn call(ctl: &File, request: u32, buf: &mut [u8]) -> std::io::Result<()> {
        // bionic's ioctl request is a c_int; the bits are reinterpreted.
        if unsafe { libc::ioctl(ctl.as_raw_fd(), request as libc::c_int, buf.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Create device `name` from `targets` and wait for ueventd to make its
    /// node; returns /dev/block/dm-N. A device that fails to come up is
    /// removed again.
    pub fn create(name: &str, targets: &[Target], readonly: bool) -> Result<PathBuf> {
        let ctl = control()?;
        let mut io = DmIoctl::new(name)?;
        call(&ctl, DM_DEV_CREATE, io.as_mut_bytes()).with_context(|| format!("create device-mapper device {name}"))?;
        let node = PathBuf::from(format!("/dev/block/dm-{}", decode(io.dev).1));
        let up = (|| -> Result<()> {
            let mut table = table_load(name, targets, readonly)?;
            call(&ctl, DM_TABLE_LOAD, &mut table).with_context(|| format!("load the table of {name}"))?;
            let mut resume = DmIoctl::new(name)?;
            call(&ctl, DM_DEV_SUSPEND, resume.as_mut_bytes()).with_context(|| format!("activate {name}"))?;
            wait_for(&node)
        })();
        match up {
            Ok(()) => Ok(node),
            Err(e) => match remove(name) {
                Ok(()) => Err(e),
                Err(r) => Err(e.context(format!("removing {name} afterwards failed too: {r:#}"))),
            },
        }
    }

    pub fn remove(name: &str) -> Result<()> {
        let mut io = DmIoctl::new(name)?;
        call(&control()?, DM_DEV_REMOVE, io.as_mut_bytes()).with_context(|| format!("remove device-mapper device {name}"))
    }

    /// The (major, minor) of device `name`, or `None` when there is none.
    pub fn status(name: &str) -> Result<Option<(u32, u32)>> {
        let mut io = DmIoctl::new(name)?;
        match call(&control()?, DM_DEV_STATUS, io.as_mut_bytes()) {
            Ok(()) => Ok(Some(decode(io.dev))),
            Err(e) if e.raw_os_error() == Some(libc::ENXIO) => Ok(None),
            Err(e) => Err(e).with_context(|| format!("look up device-mapper device {name}")),
        }
    }

    fn decode(dev: u64) -> (u32, u32) {
        let dev = dev as libc::dev_t;
        (libc::major(dev) as u32, libc::minor(dev) as u32)
    }

    fn wait_for(node: &Path) -> Result<()> {
        let start = Instant::now();
        while !node.exists() {
            if start.elapsed() > NODE_TIMEOUT {
                bail!("ueventd made no {} within {:?}", node.display(), NODE_TIMEOUT);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}

#[cfg(not(target_os = "android"))]
mod sys {
    use super::Target;
    use anyhow::{bail, Result};
    use std::path::PathBuf;

    pub fn create(name: &str, _targets: &[Target], _readonly: bool) -> Result<PathBuf> {
        bail!("this host has no device-mapper to create {name}")
    }

    pub fn remove(name: &str) -> Result<()> {
        bail!("this host has no device-mapper to remove {name}")
    }

    pub fn status(name: &str) -> Result<Option<(u32, u32)>> {
        bail!("this host has no device-mapper to look up {name}")
    }
}

pub use sys::{create, remove, status};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_codes_match_the_kernel_header() {
        assert_eq!(DM_DEV_CREATE, 0xc138_fd03);
        assert_eq!(DM_TABLE_LOAD, 0xc138_fd09);
    }

    #[test]
    fn table_lists_targets_back_to_back() {
        let targets = [
            Target::Linear { sectors: 4_078_120, device: (259, 29), offset: 2048 },
            Target::Zero { sectors: 8 },
        ];
        let buf = table_load("odm_b", &targets, true).unwrap();
        let (io, rest) = DmIoctl::read_from_prefix(&buf).unwrap();
        assert_eq!(&io.name[..6], b"odm_b\0");
        assert_eq!((io.data_start, io.data_size as usize, io.target_count), (312, buf.len(), 2));
        assert_eq!(io.flags, DM_READONLY_FLAG);

        let (first, params) = DmTargetSpec::read_from_prefix(rest).unwrap();
        assert_eq!((first.sector_start, first.length), (0, 4_078_120));
        assert_eq!(&first.target_type[..7], b"linear\0");
        assert_eq!(&params[..16], b"259:29 2048\0\0\0\0\0");
        assert_eq!(first.next, 56);

        let (second, tail) = DmTargetSpec::read_from_prefix(&rest[first.next as usize..]).unwrap();
        assert_eq!((second.sector_start, second.length), (4_078_120, 8));
        assert_eq!(&second.target_type[..5], b"zero\0");
        assert_eq!((second.next, tail.len()), (48, 8));
    }
}
