/// Filesystems Android puts on logical and metadata partitions, in the order
/// a device's fstab lists alternatives for one mount point.
pub const FILESYSTEMS: [&str; 3] = ["erofs", "ext4", "f2fs"];

/// The mount that shows at a mount point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visible {
    /// (major, minor) of the mounted device.
    pub device: (u32, u32),
    pub filesystem: String,
}

#[cfg(target_os = "android")]
mod sys {
    use super::{Visible, FILESYSTEMS};
    use anyhow::{bail, Context, Result};
    use procfs::process::{all_processes, MMapPath, MountInfo, MountOptFields, Process};
    use procfs::{ProcError, ProcResult};
    use std::collections::{BTreeSet, HashMap, HashSet};
    use std::ffi::CString;
    use std::fs;
    use std::io::ErrorKind;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::ptr::null;

    /// Where mounts wait while the mount point above them is covered or
    /// uncovered. It is on rootfs, which is private during the moves.
    const PARK: &str = "/skyward/park";

    fn cstr(p: &Path) -> Result<CString> {
        CString::new(p.as_os_str().as_bytes()).with_context(|| format!("{} holds a NUL", p.display()))
    }

    fn os_error() -> std::io::Error {
        std::io::Error::last_os_error()
    }

    /// Mount `device` read-only at `target` with the first of [`FILESYSTEMS`]
    /// the kernel accepts, and return its name. Only the kernel's answers for
    /// "not this filesystem" move on to the next one.
    pub fn mount_ro(device: &Path, target: &Path) -> Result<&'static str> {
        let (src, dst) = (cstr(device)?, cstr(target)?);
        let mut refused = Vec::new();
        for fs in FILESYSTEMS {
            let ty = CString::new(fs).expect("filesystem names hold no NUL");
            if unsafe { libc::mount(src.as_ptr(), dst.as_ptr(), ty.as_ptr(), libc::MS_RDONLY, null()) } == 0 {
                return Ok(fs);
            }
            let e = os_error();
            match e.raw_os_error() {
                Some(libc::EINVAL | libc::ENODEV) => refused.push(format!("{fs}: {e}")),
                _ => {
                    return Err(e).with_context(|| format!("mount {} on {} as {fs}", device.display(), target.display()))
                }
            }
        }
        bail!("{} holds none of {} ({})", device.display(), FILESYSTEMS.join(", "), refused.join("; "))
    }

    pub fn umount(target: &Path) -> Result<()> {
        if unsafe { libc::umount2(cstr(target)?.as_ptr(), 0) } != 0 {
            return Err(os_error()).with_context(|| format!("unmount {}", target.display()));
        }
        Ok(())
    }

    /// Take the mount at `target` out of the tree at once; whatever still
    /// has files open on it keeps them until it closes them.
    pub fn umount_detach(target: &Path) -> Result<()> {
        if unsafe { libc::umount2(cstr(target)?.as_ptr(), libc::MNT_DETACH) } != 0 {
            return Err(os_error()).with_context(|| format!("detach {}", target.display()));
        }
        Ok(())
    }

    fn mounts() -> Result<Vec<MountInfo>> {
        Ok(Process::myself()?.mountinfo()?.0)
    }

    pub fn is_mount_point(path: &Path) -> Result<bool> {
        Ok(mounts()?.iter().any(|m| m.mount_point == path))
    }

    /// What is visibly mounted at `path`, the last of any mounts stacked there.
    pub fn visible(path: &Path) -> Result<Option<Visible>> {
        let Some(m) = mounts()?.into_iter().rev().find(|m| m.mount_point == path) else {
            return Ok(None);
        };
        let (major, minor) = m.majmin.split_once(':').with_context(|| format!("device number {:?}", m.majmin))?;
        Ok(Some(Visible { device: (major.parse()?, minor.parse()?), filesystem: m.fs_type }))
    }

    fn remount(target: &Path, flags: libc::c_ulong, what: &str) -> Result<()> {
        if unsafe { libc::mount(null(), cstr(target)?.as_ptr(), null(), flags, null()) } != 0 {
            return Err(os_error()).with_context(|| format!("make {} {what}", target.display()));
        }
        Ok(())
    }

    fn move_mount(from: &Path, to: &Path) -> Result<()> {
        if unsafe { libc::mount(cstr(from)?.as_ptr(), cstr(to)?.as_ptr(), null(), libc::MS_MOVE, null()) } != 0 {
            return Err(os_error()).with_context(|| format!("move the mount on {} to {}", from.display(), to.display()));
        }
        Ok(())
    }

    #[derive(Default)]
    struct Parked {
        /// Original mount point and where it waits.
        moved: Vec<(PathBuf, PathBuf)>,
        /// Shared mounts made private for the moves.
        privatized: Vec<PathBuf>,
    }

    /// Move the mounts directly below `target` to the park. The kernel moves
    /// no mount whose parent is shared, so their parents and rootfs become
    /// private until `unpark`.
    fn park(target: &Path) -> Result<Parked> {
        let all = mounts()?;
        let below = |m: &MountInfo| m.mount_point.starts_with(target) && m.mount_point != target;
        let nested: HashSet<i32> = all.iter().filter(|m| below(m)).map(|m| m.mnt_id).collect();
        // `pid` is the parent mount's id.
        let top: Vec<&MountInfo> = all.iter().filter(|m| below(m) && !nested.contains(&m.pid)).collect();
        let mut parked = Parked::default();
        if top.is_empty() {
            return Ok(parked);
        }
        let rootfs = all.iter().rev().find(|m| m.mount_point == Path::new("/")).map(|m| m.mnt_id);
        let parents: HashSet<i32> = top.iter().map(|m| m.pid).chain(rootfs).collect();
        for m in all.iter().filter(|m| parents.contains(&m.mnt_id)) {
            if m.opt_fields.iter().any(|f| matches!(f, MountOptFields::Shared(_))) {
                remount(&m.mount_point, libc::MS_PRIVATE, "private")?;
                parked.privatized.push(m.mount_point.clone());
            }
        }
        for (i, m) in top.iter().enumerate() {
            let at = Path::new(PARK).join(i.to_string());
            fs::create_dir_all(&at).with_context(|| format!("create {}", at.display()))?;
            move_mount(&m.mount_point, &at)?;
            parked.moved.push((m.mount_point.clone(), at));
        }
        Ok(parked)
    }

    fn unpark(parked: Parked) -> Result<()> {
        for (home, at) in &parked.moved {
            move_mount(at, home)?;
            fs::remove_dir(at).with_context(|| format!("remove {}", at.display()))?;
        }
        if !parked.moved.is_empty() {
            fs::remove_dir(PARK).with_context(|| format!("remove {PARK}"))?;
        }
        for p in &parked.privatized {
            remount(p, libc::MS_SHARED, "shared")?;
        }
        Ok(())
    }

    /// Mount `device` read-only over `target` and keep the mounts below it on
    /// their paths: they move aside, `target` is covered, and they move back
    /// onto the same paths in the new filesystem.
    pub fn mount_ro_over(device: &Path, target: &Path) -> Result<&'static str> {
        let parked = park(target)?;
        let fs = match mount_ro(device, target) {
            Ok(fs) => fs,
            Err(e) => {
                unpark(parked).with_context(|| format!("after {e:#}"))?;
                return Err(e);
            }
        };
        if let Some((home, _)) = parked.moved.iter().find(|(home, _)| !home.is_dir()) {
            let home = home.clone();
            umount(target)?;
            unpark(parked)?;
            bail!("{} has no {} to keep the mount there", device.display(), home.display());
        }
        unpark(parked)?;
        Ok(fs)
    }

    /// Unmount `target` and keep the mounts below it on their paths.
    pub fn umount_over(target: &Path) -> Result<()> {
        under_parked(target, umount)
    }

    /// [`umount_detach`] `target` and keep the mounts below it on their
    /// paths.
    pub fn umount_detach_over(target: &Path) -> Result<()> {
        under_parked(target, umount_detach)
    }

    fn under_parked(target: &Path, unmount: fn(&Path) -> Result<()>) -> Result<()> {
        let parked = park(target)?;
        if let Err(e) = unmount(target) {
            unpark(parked).with_context(|| format!("after {e:#}"))?;
            return Err(e);
        }
        unpark(parked)
    }

    /// The programs that hold each filesystem, keyed by the (major, minor)
    /// of its device: those with one of its files mapped or open, or one of
    /// its directories as their working or root directory, named by their
    /// executable. A filesystem a program holds does not unmount.
    pub fn holders() -> Result<HashMap<(u32, u32), BTreeSet<String>>> {
        let mut found: HashMap<(u32, u32), BTreeSet<String>> = HashMap::new();
        for p in all_processes().context("list the processes")? {
            let Some(p) = present(p)? else { continue };
            // Kernel threads have no executable and hold no files.
            let Some(exe) = present(p.exe())? else { continue };
            let name = exe.file_name().with_context(|| format!("{} has no file name", exe.display()))?.to_string_lossy();
            let proc_dir = Path::new("/proc").join(p.pid().to_string());
            let mut devices = Vec::new();
            if let Some(maps) = present(p.maps())? {
                let files = maps.into_iter().filter(|m| matches!(m.pathname, MMapPath::Path(_)));
                devices.extend(files.map(|m| (m.dev.0 as u32, m.dev.1 as u32)));
            }
            if let Some(fds) = present(p.fd())? {
                for fd in fds {
                    let Some(fd) = present(fd)? else { continue };
                    devices.extend(device_of(&proc_dir.join("fd").join(fd.fd.to_string()))?);
                }
            }
            for dir in ["cwd", "root"] {
                devices.extend(device_of(&proc_dir.join(dir))?);
            }
            for d in devices {
                found.entry(d).or_default().insert(name.to_string());
            }
        }
        Ok(found)
    }

    /// `None` for what went away while it was read: a process that ended,
    /// a file it closed.
    fn present<T>(read: ProcResult<T>) -> Result<Option<T>> {
        match read {
            Ok(v) => Ok(Some(v)),
            Err(ProcError::NotFound(_)) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// The device of the filesystem `path` is on, following it if it is a
    /// link; `None` when it went away.
    fn device_of(path: &Path) -> Result<Option<(u32, u32)>> {
        match fs::metadata(path) {
            Ok(m) => {
                let dev = m.dev() as libc::dev_t;
                Ok(Some((libc::major(dev) as u32, libc::minor(dev) as u32)))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("stat {}", path.display())),
        }
    }
}

#[cfg(not(target_os = "android"))]
mod sys {
    use super::Visible;
    use anyhow::{bail, Result};
    use std::collections::{BTreeSet, HashMap};
    use std::path::Path;

    pub fn mount_ro(device: &Path, _target: &Path) -> Result<&'static str> {
        bail!("this host does not mount {}", device.display())
    }

    pub fn mount_ro_over(device: &Path, _target: &Path) -> Result<&'static str> {
        bail!("this host does not mount {}", device.display())
    }

    pub fn umount(target: &Path) -> Result<()> {
        bail!("this host does not unmount {}", target.display())
    }

    pub fn umount_over(target: &Path) -> Result<()> {
        bail!("this host does not unmount {}", target.display())
    }

    pub fn umount_detach(target: &Path) -> Result<()> {
        bail!("this host does not unmount {}", target.display())
    }

    pub fn umount_detach_over(target: &Path) -> Result<()> {
        bail!("this host does not unmount {}", target.display())
    }

    pub fn is_mount_point(path: &Path) -> Result<bool> {
        bail!("this host has no mount table to look up {}", path.display())
    }

    pub fn visible(path: &Path) -> Result<Option<Visible>> {
        bail!("this host has no mount table to look up {}", path.display())
    }

    pub fn holders() -> Result<HashMap<(u32, u32), BTreeSet<String>>> {
        bail!("this host does not look up which programs hold a filesystem")
    }
}

pub use sys::{
    holders, is_mount_point, mount_ro, mount_ro_over, umount, umount_detach, umount_detach_over, umount_over, visible,
};
