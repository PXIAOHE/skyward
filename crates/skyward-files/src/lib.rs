//! Copying, moving, renaming and deleting files and folders the way a file
//! manager does: folders whole, links as links, each entry's permissions,
//! owner, times and extended attributes kept where the filesystem it goes
//! to keeps them, and how far the work has come told as it goes.

use anyhow::{bail, ensure, Context, Result};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// How much of a file is copied between two reports of how far it is.
const CHUNK: usize = 1 << 20;

/// What to do when an entry of the same name is already where one goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clash {
    /// Write nothing, and say which name is taken.
    Refuse,
    /// Put the new entry in place of the old one, which goes first.
    Replace,
    /// Leave the old entry, and leave out the new one.
    Skip,
    /// Give the new entry a name of its own, as "name (1).txt".
    KeepBoth,
}

/// How far a piece of work has come.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub bytes: u64,
    pub total_bytes: u64,
    pub entries: u64,
    pub total_entries: u64,
}

/// The work was stopped, as asked, before it was done.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stopped before it was done")
    }
}

impl std::error::Error for Cancelled {}

/// What a filesystem keeps of an entry beyond its contents and times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Keeps {
    owner: bool,
    mode: bool,
    xattrs: bool,
}

/// Whether the work goes on, or was asked to stop.
fn going(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Cancelled.into());
    }
    Ok(())
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("look at {}", path.display())),
    }
}

fn name(path: &Path) -> Result<&OsStr> {
    path.file_name().with_context(|| format!("{} names no entry", path.display()))
}

/// Where an entry really is: its folder with every link in the way
/// followed, and its own name, the entry itself not followed if a link.
fn real(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let folder = fs::canonicalize(parent).with_context(|| format!("find {}", parent.display()))?;
    Ok(folder.join(name(path)?))
}

/// Whether a folder is where a filesystem is mounted: it sits on another
/// device than the folder it is in.
fn mounted_at(path: &Path, meta: &fs::Metadata) -> Result<bool> {
    let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) else { return Ok(true) };
    let up = fs::metadata(parent).with_context(|| format!("look at {}", parent.display()))?;
    Ok(up.dev() != meta.dev())
}

/// The bytes the files of `paths` hold and how many entries they are,
/// folders whole. With `one_filesystem`, a folder that has another
/// filesystem mounted on or inside it is refused: the work would reach
/// into that filesystem.
fn measure(paths: &[PathBuf], one_filesystem: bool) -> Result<(u64, u64)> {
    fn walk(path: &Path, dev: Option<u64>, one_filesystem: bool, sum: &mut (u64, u64)) -> Result<()> {
        let meta = fs::symlink_metadata(path).with_context(|| format!("look at {}", path.display()))?;
        if one_filesystem && meta.is_dir() && dev.is_some_and(|d| d != meta.dev()) {
            bail!("another filesystem is mounted at {}; unmount it first", path.display());
        }
        sum.1 += 1;
        if meta.is_file() {
            sum.0 += meta.len();
        } else if meta.is_dir() {
            for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
                let entry = entry.with_context(|| format!("read {}", path.display()))?;
                walk(&entry.path(), Some(meta.dev()), one_filesystem, sum)?;
            }
        }
        Ok(())
    }
    let mut sum = (0, 0);
    for path in paths {
        if one_filesystem {
            let meta = fs::symlink_metadata(path).with_context(|| format!("look at {}", path.display()))?;
            if meta.is_dir() && mounted_at(path, &meta)? {
                bail!("a filesystem is mounted at {}; unmount it first", path.display());
            }
        }
        walk(path, None, one_filesystem, &mut sum)?;
    }
    Ok(sum)
}

/// The names that are taken: where entries of `sources` would go in the
/// folder `into` and an entry of that name is already there.
pub fn clashes(sources: &[PathBuf], into: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for source in sources {
        let target = into.join(name(source)?);
        if exists(&target)? && real(&target)? != real(source)? {
            out.push(target);
        }
    }
    Ok(out)
}

/// A name in `into` for a new entry named `name` that is not taken:
/// "name (1).txt", "name (2).txt" and on.
fn free_name(into: &Path, name: &OsStr) -> Result<PathBuf> {
    let as_path = Path::new(name);
    let stem = as_path.file_stem().unwrap_or(name).to_string_lossy();
    let extension = as_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 1.. {
        let candidate = into.join(format!("{stem} ({n}){extension}"));
        if !exists(&candidate)? {
            return Ok(candidate);
        }
    }
    unreachable!("some number gives a name that is not taken")
}

/// Where each of `sources` goes in the folder `into` under `clash`, with
/// what cannot be done refused before anything is written. An entry moved
/// to the folder it is in already stays as it is; one copied there is a
/// second one beside it, under a name of its own.
fn plan(sources: &[PathBuf], into: &Path, clash: Clash, moving: bool) -> Result<Vec<(PathBuf, PathBuf)>> {
    let folder = fs::metadata(into).with_context(|| format!("look at {}", into.display()))?;
    ensure!(folder.is_dir(), "{} is not a folder", into.display());
    let into_real = fs::canonicalize(into).with_context(|| format!("find {}", into.display()))?;
    let mut out = Vec::new();
    for source in sources {
        let meta = fs::symlink_metadata(source).with_context(|| format!("look at {}", source.display()))?;
        let source_real = real(source)?;
        if meta.is_dir() && into_real.starts_with(&source_real) {
            bail!("{} cannot go inside itself", source.display());
        }
        let mut target = into.join(name(source)?);
        let itself = into_real.join(name(source)?) == source_real;
        if moving && itself {
            continue;
        }
        if itself {
            target = free_name(into, name(source)?)?;
        } else if exists(&target)? {
            match clash {
                Clash::Refuse => bail!("{} is already there", target.display()),
                Clash::Replace => {}
                Clash::Skip => continue,
                Clash::KeepBoth => target = free_name(into, name(source)?)?,
            }
        }
        out.push((source.clone(), target));
    }
    Ok(out)
}

/// What the filesystem holding the folder `into` keeps of an entry: one
/// made for other systems keeps neither owners, permissions nor extended
/// attributes.
#[cfg(target_os = "android")]
fn keeps(into: &Path) -> Result<Keeps> {
    use std::os::unix::ffi::OsStrExt;
    const EXFAT_SUPER_MAGIC: u64 = 0x2011_bab0;
    const SDCARDFS_SUPER_MAGIC: u64 = 0x5dca_2df5;
    let c = std::ffi::CString::new(into.as_os_str().as_bytes()).with_context(|| format!("{} holds a NUL", into.display()))?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| format!("look at the filesystem of {}", into.display()));
    }
    let foreign = [libc::MSDOS_SUPER_MAGIC as u64, EXFAT_SUPER_MAGIC, libc::FUSE_SUPER_MAGIC as u64, SDCARDFS_SUPER_MAGIC];
    let native = !foreign.contains(&st.f_type);
    Ok(Keeps { owner: native, mode: native, xattrs: native })
}

/// A computer the tests run on keeps owners and permissions; extended
/// attributes are copied only on the phone.
#[cfg(not(target_os = "android"))]
fn keeps(_into: &Path) -> Result<Keeps> {
    Ok(Keeps { owner: true, mode: true, xattrs: false })
}

/// Copy each of `sources` into the folder `into`.
pub fn copy(sources: &[PathBuf], into: &Path, clash: Clash, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<()> {
    let plan = plan(sources, into, clash, false)?;
    let froms: Vec<PathBuf> = plan.iter().map(|(from, _)| from.clone()).collect();
    let (total_bytes, total_entries) = measure(&froms, false)?;
    let mut p = Progress { total_bytes, total_entries, ..Progress::default() };
    progress(p);
    let keeps = keeps(into)?;
    for (from, to) in plan {
        if exists(&to)? {
            remove(&to, cancel, &mut Progress::default(), &mut |_| {})?;
        }
        copy_entry(&from, &to, keeps, cancel, &mut p, progress)?;
    }
    Ok(())
}

/// Move each of `sources` into the folder `into`. Within one filesystem an
/// entry is moved at once; to another, it is copied and then deleted.
pub fn move_into(sources: &[PathBuf], into: &Path, clash: Clash, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<()> {
    let plan = plan(sources, into, clash, true)?;
    let froms: Vec<PathBuf> = plan.iter().map(|(from, _)| from.clone()).collect();
    let (total_bytes, total_entries) = measure(&froms, true)?;
    let mut p = Progress { total_bytes, total_entries, ..Progress::default() };
    progress(p);
    let keeps = keeps(into)?;
    for (from, to) in plan {
        going(cancel)?;
        if exists(&to)? {
            remove(&to, cancel, &mut Progress::default(), &mut |_| {})?;
        }
        let (bytes, entries) = measure(std::slice::from_ref(&from), true)?;
        match fs::rename(&from, &to) {
            Ok(()) => {
                p.bytes += bytes;
                p.entries += entries;
                progress(p);
            }
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                copy_entry(&from, &to, keeps, cancel, &mut p, progress)?;
                remove(&from, cancel, &mut Progress::default(), &mut |_| {})?;
            }
            Err(e) => return Err(e).with_context(|| format!("move {} to {}", from.display(), to.display())),
        }
    }
    Ok(())
}

/// Give the entry at `path` the name `new`, in the folder it is in, and
/// say where it now is.
pub fn rename(path: &Path, new: &str) -> Result<PathBuf> {
    ensure!(
        !new.is_empty() && new != "." && new != ".." && !new.contains(['/', '\0']),
        "{new:?} cannot be the name of a file or folder"
    );
    let to = path.parent().with_context(|| format!("{} is in no folder", path.display()))?.join(new);
    if to == path {
        return Ok(to);
    }
    ensure!(!exists(&to)?, "{} is already there", to.display());
    fs::rename(path, &to).with_context(|| format!("rename {} to {new}", path.display()))?;
    Ok(to)
}

/// Delete each of `paths`, folders with all they hold. A folder that has a
/// filesystem mounted on or inside it is refused before anything goes.
pub fn delete(paths: &[PathBuf], cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<()> {
    let (total_bytes, total_entries) = measure(paths, true)?;
    let mut p = Progress { total_bytes, total_entries, ..Progress::default() };
    progress(p);
    for path in paths {
        remove(path, cancel, &mut p, progress)?;
    }
    Ok(())
}

fn remove(path: &Path, cancel: &AtomicBool, p: &mut Progress, progress: &mut dyn FnMut(Progress)) -> Result<()> {
    going(cancel)?;
    let meta = fs::symlink_metadata(path).with_context(|| format!("look at {}", path.display()))?;
    if meta.is_dir() {
        for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
            let entry = entry.with_context(|| format!("read {}", path.display()))?;
            remove(&entry.path(), cancel, p, progress)?;
        }
        fs::remove_dir(path).with_context(|| format!("delete {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("delete {}", path.display()))?;
    }
    if meta.is_file() {
        p.bytes += meta.len();
    }
    p.entries += 1;
    progress(*p);
    Ok(())
}

fn copy_entry(from: &Path, to: &Path, keeps: Keeps, cancel: &AtomicBool, p: &mut Progress, progress: &mut dyn FnMut(Progress)) -> Result<()> {
    going(cancel)?;
    let meta = fs::symlink_metadata(from).with_context(|| format!("look at {}", from.display()))?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        let target = fs::read_link(from).with_context(|| format!("read the link {}", from.display()))?;
        std::os::unix::fs::symlink(&target, to).with_context(|| format!("make the link {}", to.display()))?;
    } else if kind.is_dir() {
        fs::create_dir(to).with_context(|| format!("make {}", to.display()))?;
        let mut entries: Vec<PathBuf> = fs::read_dir(from)
            .with_context(|| format!("read {}", from.display()))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<_>>()
            .with_context(|| format!("read {}", from.display()))?;
        entries.sort();
        for entry in entries {
            copy_entry(&entry, &to.join(name(&entry)?), keeps, cancel, p, progress)?;
        }
    } else if kind.is_file() {
        copy_file(from, to, cancel, p, progress)?;
    } else if kind.is_block_device() || kind.is_char_device() || kind.is_fifo() {
        make_node(to, meta.mode(), meta.rdev())?;
    } else {
        bail!("{} is a socket, which only the program that made it can answer", from.display());
    }
    keep(from, to, &meta, keeps)?;
    p.entries += 1;
    progress(*p);
    Ok(())
}

/// Copy a file's contents, telling how far each chunk takes the work. A
/// file stopped halfway is not left behind.
fn copy_file(from: &Path, to: &Path, cancel: &AtomicBool, p: &mut Progress, progress: &mut dyn FnMut(Progress)) -> Result<()> {
    let mut source = File::open(from).with_context(|| format!("open {}", from.display()))?;
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)
        .with_context(|| format!("make {}", to.display()))?;
    let mut buf = vec![0u8; CHUNK];
    let copied = (|| -> Result<()> {
        loop {
            going(cancel)?;
            let n = source.read(&mut buf).with_context(|| format!("read {}", from.display()))?;
            if n == 0 {
                return target.sync_all().with_context(|| format!("write {}", to.display()));
            }
            target.write_all(&buf[..n]).with_context(|| format!("write {}", to.display()))?;
            p.bytes += n as u64;
            progress(*p);
        }
    })();
    if copied.is_err() {
        drop(target);
        fs::remove_file(to).with_context(|| format!("take away the part of {} written", to.display()))?;
    }
    copied
}

fn make_node(to: &Path, mode: u32, rdev: u64) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(to.as_os_str().as_bytes()).with_context(|| format!("{} holds a NUL", to.display()))?;
    if unsafe { libc::mknod(c.as_ptr(), mode as libc::mode_t, rdev as libc::dev_t) } != 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| format!("make {}", to.display()));
    }
    Ok(())
}

/// Give a copy what the original has beyond its contents, as far as the
/// filesystem it is on keeps it; the times go last, as writing the rest
/// would change them.
fn keep(from: &Path, to: &Path, meta: &fs::Metadata, keeps: Keeps) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let link = meta.file_type().is_symlink();
    if keeps.xattrs {
        copy_xattrs(from, to)?;
    }
    if keeps.owner {
        std::os::unix::fs::lchown(to, Some(meta.uid()), Some(meta.gid())).with_context(|| format!("give {} its owner", to.display()))?;
    }
    if keeps.mode && !link {
        fs::set_permissions(to, fs::Permissions::from_mode(meta.mode() & 0o7777))
            .with_context(|| format!("give {} its permissions", to.display()))?;
    }
    let c = std::ffi::CString::new(to.as_os_str().as_bytes()).with_context(|| format!("{} holds a NUL", to.display()))?;
    let times = [
        libc::timespec { tv_sec: meta.atime() as _, tv_nsec: meta.atime_nsec() as _ },
        libc::timespec { tv_sec: meta.mtime() as _, tv_nsec: meta.mtime_nsec() as _ },
    ];
    if unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), libc::AT_SYMLINK_NOFOLLOW) } != 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| format!("give {} its times", to.display()));
    }
    Ok(())
}

/// Copy every extended attribute, the SELinux label among them, from one
/// entry to another, a link's own rather than its target's, as far as the
/// filesystem of `to` keeps them: one that keeps no attribute of a kind,
/// as the recovery's own root keeps no access lists, says so for each.
#[cfg(target_os = "android")]
fn copy_xattrs(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let cstr = |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes()).with_context(|| format!("{} holds a NUL", p.display()));
    let (f, t) = (cstr(from)?, cstr(to)?);
    let size = unsafe { libc::llistxattr(f.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| format!("list the attributes of {}", from.display()));
    }
    let mut names = vec![0u8; size as usize];
    let size = unsafe { libc::llistxattr(f.as_ptr(), names.as_mut_ptr().cast(), names.len()) };
    if size < 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| format!("list the attributes of {}", from.display()));
    }
    for attr in names[..size as usize].split(|&b| b == 0).filter(|n| !n.is_empty()) {
        let n = std::ffi::CString::new(attr).expect("a name split at NULs holds none");
        let len = unsafe { libc::lgetxattr(f.as_ptr(), n.as_ptr(), std::ptr::null_mut(), 0) };
        if len < 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| format!("read {:?} of {}", n, from.display()));
        }
        let mut value = vec![0u8; len as usize];
        let len = unsafe { libc::lgetxattr(f.as_ptr(), n.as_ptr(), value.as_mut_ptr().cast(), value.len()) };
        if len < 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| format!("read {:?} of {}", n, from.display()));
        }
        if unsafe { libc::lsetxattr(t.as_ptr(), n.as_ptr(), value.as_ptr().cast(), len as usize, 0) } != 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EOPNOTSUPP) {
                continue;
            }
            return Err(e).with_context(|| format!("give {} {:?}", to.display(), n));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn copy_xattrs(_from: &Path, _to: &Path) -> Result<()> {
    unreachable!("only the phone copies extended attributes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A folder of its own for a test, empty, under the system's temporary
    /// folder.
    fn scratch(test: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("skyward-files-{test}-{}", std::process::id()));
        if exists(&at).unwrap() {
            fs::remove_dir_all(&at).unwrap();
        }
        fs::create_dir_all(&at).unwrap();
        at
    }

    /// A tree to copy: a file, a folder with a file in it, and a link.
    fn tree(at: &Path) -> PathBuf {
        let root = at.join("photos");
        fs::create_dir_all(root.join("2026")).unwrap();
        fs::write(root.join("a.jpg"), vec![7u8; 3 * CHUNK + 5]).unwrap();
        fs::write(root.join("2026/b.txt"), b"hello").unwrap();
        symlink("2026/b.txt", root.join("latest")).unwrap();
        fs::set_permissions(root.join("2026/b.txt"), fs::Permissions::from_mode(0o640)).unwrap();
        root
    }

    fn quiet() -> impl FnMut(Progress) {
        |_| {}
    }

    #[test]
    fn a_folder_is_copied_whole_with_links_as_links() {
        let at = scratch("copy");
        let root = tree(&at);
        fs::create_dir(at.join("into")).unwrap();
        let mut last = Progress::default();
        copy(std::slice::from_ref(&root), &at.join("into"), Clash::Refuse, &AtomicBool::new(false), &mut |p| last = p).unwrap();
        let copied = at.join("into/photos");
        assert_eq!(fs::read(copied.join("a.jpg")).unwrap(), vec![7u8; 3 * CHUNK + 5]);
        assert_eq!(fs::read(copied.join("2026/b.txt")).unwrap(), b"hello");
        assert_eq!(fs::read_link(copied.join("latest")).unwrap(), Path::new("2026/b.txt"));
        assert_eq!(fs::metadata(copied.join("2026/b.txt")).unwrap().mode() & 0o777, 0o640, "permissions are kept");
        let (before, after) = (fs::metadata(root.join("a.jpg")).unwrap(), fs::metadata(copied.join("a.jpg")).unwrap());
        assert_eq!((after.mtime(), after.mtime_nsec()), (before.mtime(), before.mtime_nsec()), "times are kept");
        assert_eq!(last, Progress { bytes: 3 * CHUNK as u64 + 10, total_bytes: 3 * CHUNK as u64 + 10, entries: 5, total_entries: 5 });
        assert!(root.join("a.jpg").exists(), "the original stays");
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn a_taken_name_is_refused_replaced_skipped_or_kept_beside() {
        let at = scratch("clash");
        fs::create_dir(at.join("into")).unwrap();
        fs::write(at.join("note.txt"), b"new").unwrap();
        fs::write(at.join("into/note.txt"), b"old").unwrap();
        let (cancel, sources) = (AtomicBool::new(false), [at.join("note.txt")]);
        assert_eq!(clashes(&sources, &at.join("into")).unwrap(), [at.join("into/note.txt")]);
        assert!(copy(&sources, &at.join("into"), Clash::Refuse, &cancel, &mut quiet()).is_err());
        copy(&sources, &at.join("into"), Clash::Skip, &cancel, &mut quiet()).unwrap();
        assert_eq!(fs::read(at.join("into/note.txt")).unwrap(), b"old");
        copy(&sources, &at.join("into"), Clash::KeepBoth, &cancel, &mut quiet()).unwrap();
        copy(&sources, &at.join("into"), Clash::KeepBoth, &cancel, &mut quiet()).unwrap();
        assert_eq!(fs::read(at.join("into/note (1).txt")).unwrap(), b"new");
        assert_eq!(fs::read(at.join("into/note (2).txt")).unwrap(), b"new");
        copy(&sources, &at.join("into"), Clash::Replace, &cancel, &mut quiet()).unwrap();
        assert_eq!(fs::read(at.join("into/note.txt")).unwrap(), b"new");
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn a_copy_into_its_own_folder_is_a_second_one() {
        let at = scratch("duplicate");
        fs::write(at.join(".profile"), b"x").unwrap();
        let cancel = AtomicBool::new(false);
        assert!(clashes(&[at.join(".profile")], &at).unwrap().is_empty(), "an entry does not clash with itself");
        for clash in [Clash::Refuse, Clash::Replace] {
            copy(&[at.join(".profile")], &at, clash, &cancel, &mut quiet()).unwrap();
        }
        assert_eq!(fs::read(at.join(".profile")).unwrap(), b"x", "the original stays");
        assert_eq!(fs::read(at.join(".profile (1)")).unwrap(), b"x");
        assert_eq!(fs::read(at.join(".profile (2)")).unwrap(), b"x", "whatever is chosen for names taken by others");
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn a_folder_cannot_go_inside_itself() {
        let at = scratch("inside");
        let root = tree(&at);
        let cancel = AtomicBool::new(false);
        assert!(copy(std::slice::from_ref(&root), &root.join("2026"), Clash::Refuse, &cancel, &mut quiet()).is_err());
        assert!(move_into(std::slice::from_ref(&root), &root, Clash::Refuse, &cancel, &mut quiet()).is_err());
        assert!(!root.join("2026/photos").exists(), "nothing was written");
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn moving_takes_the_entries_away_from_where_they_were() {
        let at = scratch("move");
        let root = tree(&at);
        fs::create_dir(at.join("into")).unwrap();
        let cancel = AtomicBool::new(false);
        let mut last = Progress::default();
        move_into(std::slice::from_ref(&root), &at.join("into"), Clash::Refuse, &cancel, &mut |p| last = p).unwrap();
        assert!(!root.exists());
        assert_eq!(fs::read(at.join("into/photos/2026/b.txt")).unwrap(), b"hello");
        assert_eq!((last.entries, last.total_entries), (5, 5));
        move_into(&[at.join("into/photos")], &at.join("into"), Clash::Refuse, &cancel, &mut quiet()).unwrap();
        assert!(at.join("into/photos").exists(), "moving to the folder it is in leaves it as it is");
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn renaming_keeps_to_names_that_can_be() {
        let at = scratch("rename");
        fs::write(at.join("a.txt"), b"a").unwrap();
        fs::write(at.join("b.txt"), b"b").unwrap();
        assert_eq!(rename(&at.join("a.txt"), "c.txt").unwrap(), at.join("c.txt"));
        assert_eq!(fs::read(at.join("c.txt")).unwrap(), b"a");
        assert!(rename(&at.join("c.txt"), "b.txt").is_err(), "a name taken stays with its own entry");
        for bad in ["", ".", "..", "x/y"] {
            assert!(rename(&at.join("c.txt"), bad).is_err(), "{bad:?}");
        }
        assert_eq!(rename(&at.join("c.txt"), "c.txt").unwrap(), at.join("c.txt"), "the same name changes nothing");
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn deleting_takes_folders_whole_and_counts_each_entry() {
        let at = scratch("delete");
        let root = tree(&at);
        fs::write(at.join("loose.txt"), b"x").unwrap();
        let mut seen = Vec::new();
        delete(&[root.clone(), at.join("loose.txt")], &AtomicBool::new(false), &mut |p| seen.push(p.entries)).unwrap();
        assert!(!root.exists() && !at.join("loose.txt").exists());
        assert_eq!(seen, [0, 1, 2, 3, 4, 5, 6], "every entry is told as it goes");
        fs::remove_dir_all(&at).unwrap();
    }

    #[cfg(target_os = "android")]
    fn label(path: &Path) -> String {
        use std::os::unix::ffi::OsStrExt;
        let (p, n) = (std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap(), c"security.selinux");
        let mut value = vec![0u8; 256];
        let len = unsafe { libc::lgetxattr(p.as_ptr(), n.as_ptr(), value.as_mut_ptr().cast(), value.len()) };
        assert!(len > 0, "{} has a label", path.display());
        String::from_utf8_lossy(&value[..len as usize]).trim_end_matches('\0').to_string()
    }

    /// On the phone, as root: labels go with a copy, and a move to another
    /// filesystem, /dev beside the temporary folder, copies and deletes.
    #[cfg(target_os = "android")]
    #[test]
    fn on_the_phone_labels_go_with_a_copy_and_moves_cross_filesystems() {
        use std::os::unix::ffi::OsStrExt;
        let at = scratch("phone");
        let root = tree(&at);
        let file = root.join("2026/b.txt");
        let (p, n, v) = (std::ffi::CString::new(file.as_os_str().as_bytes()).unwrap(), c"security.selinux", b"u:object_r:system_file:s0\0");
        assert_eq!(unsafe { libc::lsetxattr(p.as_ptr(), n.as_ptr(), v.as_ptr().cast(), v.len(), 0) }, 0, "{}", std::io::Error::last_os_error());
        fs::create_dir(at.join("into")).unwrap();
        let cancel = AtomicBool::new(false);
        copy(std::slice::from_ref(&root), &at.join("into"), Clash::Refuse, &cancel, &mut quiet()).unwrap();
        assert_eq!(label(&at.join("into/photos/2026/b.txt")), "u:object_r:system_file:s0");
        let other = Path::new("/dev").join(format!("skyward-files-{}", std::process::id()));
        fs::create_dir(&other).unwrap();
        assert_ne!(fs::metadata(&other).unwrap().dev(), fs::metadata(&at).unwrap().dev(), "two filesystems");
        let mut last = Progress::default();
        move_into(std::slice::from_ref(&root), &other, Clash::Refuse, &cancel, &mut |p| last = p).unwrap();
        assert!(!root.exists(), "the original goes once copied");
        assert_eq!(fs::read(other.join("photos/a.jpg")).unwrap(), vec![7u8; 3 * CHUNK + 5]);
        assert_eq!(label(&other.join("photos/2026/b.txt")), "u:object_r:system_file:s0");
        assert_eq!((last.bytes, last.entries), (last.total_bytes, last.total_entries));
        assert!(delete(&[Path::new("/dev").to_path_buf()], &cancel, &mut quiet()).is_err(), "a filesystem's own folder is not deleted");
        delete(&[other], &cancel, &mut quiet()).unwrap();
        fs::remove_dir_all(&at).unwrap();
    }

    /// On the phone, as root, with the temporary folder on the data
    /// partition: an access list, which the recovery's own root keeps none
    /// of, is left behind, and the copy is made all the same.
    #[cfg(target_os = "android")]
    #[test]
    fn on_the_phone_what_a_filesystem_keeps_none_of_is_left_behind() {
        use std::os::unix::ffi::OsStrExt;
        let at = scratch("phone-acl");
        let file = at.join("listed.txt");
        fs::write(&file, b"x").unwrap();
        // Version 2, then the entries of the owner, the group, group 1000,
        // the mask and everyone else: tag, permissions, id.
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, perm, id) in [(0x01u16, 6u16, u32::MAX), (0x04, 4, u32::MAX), (0x08, 4, 1000), (0x10, 4, u32::MAX), (0x20, 0, u32::MAX)] {
            acl.extend(tag.to_le_bytes());
            acl.extend(perm.to_le_bytes());
            acl.extend(id.to_le_bytes());
        }
        let p = std::ffi::CString::new(file.as_os_str().as_bytes()).unwrap();
        let set = unsafe { libc::lsetxattr(p.as_ptr(), c"system.posix_acl_access".as_ptr(), acl.as_ptr().cast(), acl.len(), 0) };
        assert_eq!(set, 0, "{}", std::io::Error::last_os_error());
        let into = Path::new("/").join(format!("skyward-files-acl-{}", std::process::id()));
        fs::create_dir(&into).unwrap();
        copy(&[file], &into, Clash::Refuse, &AtomicBool::new(false), &mut quiet()).unwrap();
        assert_eq!(fs::read(into.join("listed.txt")).unwrap(), b"x");
        fs::remove_dir_all(&into).unwrap();
        fs::remove_dir_all(&at).unwrap();
    }

    #[test]
    fn work_stopped_halfway_leaves_no_part_of_a_file() {
        let at = scratch("cancel");
        let root = tree(&at);
        fs::create_dir(at.join("into")).unwrap();
        let cancel = AtomicBool::new(false);
        let stopped = copy(&[root.join("a.jpg")], &at.join("into"), Clash::Refuse, &cancel, &mut |p| {
            if p.bytes >= CHUNK as u64 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(stopped.unwrap_err().is::<Cancelled>());
        assert!(!at.join("into/a.jpg").exists());
        fs::remove_dir_all(&at).unwrap();
    }
}
