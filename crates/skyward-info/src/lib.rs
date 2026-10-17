use anyhow::{anyhow, bail, Context, Result};
use skyward_config::DeviceConfig;
use skyward_vab::{Slot, SnapshotState};
use std::ffi::{CStr, CString};
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;

/// Version of this build, shown on the About page.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const CPU: &str = "/sys/devices/system/cpu";
const POWER_SUPPLY: &str = "/sys/class/power_supply";
const SYS_BLOCK: &str = "/sys/class/block";

/// What the recovery shows about the device. A source that may legitimately
/// be absent (no battery driver loaded, build properties missing from the
/// recovery ramdisk) is an `Option`; a source that exists but cannot be read
/// or parsed is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub model: String,
    pub codename: String,
    pub soc: Option<String>,
    pub cpu: Cpu,
    /// `MemTotal` in KiB.
    pub ram_kib: u64,
    /// Panel resolution as width × height in pixels.
    pub panel: Option<(u32, u32)>,
    pub os_version: Option<String>,
    pub android: Option<String>,
    pub security_patch: Option<String>,
    pub kernel: String,
    pub storage: Storage,
    pub battery: Option<Battery>,
    pub slot: Option<Slot>,
    pub snapshot: SnapshotState,
    pub boot: Boot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cpu {
    pub cores: u32,
    /// Highest `cpuinfo_max_freq` across cores in kHz; absent until the
    /// cpufreq driver is loaded.
    pub max_khz: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    /// Size of the userdata partition in bytes.
    pub total: u64,
    /// Free bytes on /data, counted like `File.getFreeSpace()`, while /data
    /// is mounted.
    pub free: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    Charging,
    Discharging,
    NotCharging,
    Full,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Battery {
    pub level: u8,
    pub charge: Charge,
    /// Tenths of a degree Celsius.
    pub temp: Option<i32>,
    /// Kernel `POWER_SUPPLY_HEALTH` text such as `Good`.
    pub health: Option<String>,
    pub cycles: Option<u32>,
    /// Design capacity in µAh.
    pub design: Option<u64>,
    /// Present full-charge capacity in µAh.
    pub full: Option<u64>,
    /// Voltage in µV.
    pub voltage: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boot {
    /// From `ro.boot.vbmeta.device_state`.
    pub unlocked: Option<bool>,
    /// `ro.boot.verifiedbootstate`: green, yellow, orange or red.
    pub verified: Option<String>,
}

/// Where userdata stands in the unlock flow. The recovery sets this itself:
/// only the code that installed the CE keys knows whether it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataState {
    Unmounted,
    Locked,
    Decrypted,
}

pub fn collect(cfg: &DeviceConfig) -> Result<DeviceInfo> {
    let d = &cfg.display;
    let unlocked = match prop("ro.boot.vbmeta.device_state").as_deref() {
        None => None,
        Some("locked") => Some(false),
        Some("unlocked") => Some(true),
        Some(other) => bail!("ro.boot.vbmeta.device_state {other:?}"),
    };
    Ok(DeviceInfo {
        model: cfg.device.model.clone(),
        codename: cfg.device.codename.clone(),
        soc: cfg.device.soc.clone(),
        cpu: cpu(Path::new(CPU))?,
        ram_kib: mem_total(&read("/proc/meminfo")?)?,
        panel: (d.width > 0 && d.height > 0).then_some((d.width, d.height)),
        os_version: prop(&cfg.device.os_version_prop),
        android: prop("ro.build.version.release"),
        security_patch: prop("ro.build.version.security_patch"),
        kernel: kernel_release()?,
        storage: storage()?,
        battery: battery(Path::new(POWER_SUPPLY))?,
        slot: skyward_vab::current_slot(),
        snapshot: skyward_vab::snapshot_state(),
        boot: Boot { unlocked, verified: prop("ro.boot.verifiedbootstate") },
    })
}

/// A system property, or `None` when it is unset.
#[cfg(target_os = "android")]
pub fn prop(name: &str) -> Option<String> {
    let key = CString::new(name).expect("property names contain no NUL");
    let mut buf = [0 as libc::c_char; libc::PROP_VALUE_MAX as usize];
    let n = unsafe { libc::__system_property_get(key.as_ptr(), buf.as_mut_ptr()) };
    if n <= 0 {
        return None;
    }
    Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
}

/// Hosts have no property area.
#[cfg(not(target_os = "android"))]
pub fn prop(_name: &str) -> Option<String> {
    None
}

fn read(path: impl AsRef<Path>) -> Result<String> {
    let p = path.as_ref();
    Ok(fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?.trim().to_string())
}

/// `None` when the file does not exist; any other failure is an error.
fn read_opt(path: impl AsRef<Path>) -> Result<Option<String>> {
    let p = path.as_ref();
    match fs::read_to_string(p) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", p.display())),
    }
}

fn parse<T: FromStr>(s: &str, what: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    s.parse().map_err(|e| anyhow!("{what}: {s:?}: {e}"))
}

fn parse_opt<T: FromStr>(path: impl AsRef<Path>, what: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    read_opt(path)?.map(|s| parse(&s, what)).transpose()
}

fn cpu(root: &Path) -> Result<Cpu> {
    let ids = cpulist(&read(root.join("possible"))?)?;
    let mut max_khz: Option<u32> = None;
    for id in &ids {
        let path = root.join(format!("cpu{id}/cpufreq/cpuinfo_max_freq"));
        if let Some(khz) = parse_opt::<u32>(&path, "cpuinfo_max_freq")? {
            max_khz = Some(max_khz.map_or(khz, |m| m.max(khz)));
        }
    }
    Ok(Cpu { cores: ids.len() as u32, max_khz })
}

/// Kernel cpulist syntax: comma-separated ids and inclusive ranges (`0-3,6`).
fn cpulist(s: &str) -> Result<Vec<u32>> {
    let mut ids = Vec::new();
    for part in s.split(',') {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (parse(a, "cpulist")?, parse(b, "cpulist")?);
                if a > b {
                    bail!("cpulist range {part:?} runs backwards");
                }
                ids.extend(a..=b);
            }
            None => ids.push(parse(part, "cpulist")?),
        }
    }
    Ok(ids)
}

fn mem_total(meminfo: &str) -> Result<u64> {
    let line = meminfo
        .lines()
        .find(|l| l.starts_with("MemTotal:"))
        .ok_or_else(|| anyhow!("MemTotal missing from /proc/meminfo"))?;
    let kib = line["MemTotal:".len()..]
        .trim()
        .strip_suffix("kB")
        .ok_or_else(|| anyhow!("MemTotal is not in kB: {line:?}"))?;
    parse(kib.trim(), "MemTotal")
}

fn kernel_release() -> Result<String> {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return Err(std::io::Error::last_os_error()).context("uname");
    }
    let release = unsafe { CStr::from_ptr(u.release.as_ptr()) };
    Ok(release.to_str().context("kernel release is not UTF-8")?.to_string())
}

fn storage() -> Result<Storage> {
    let link = Path::new(skyward_block::BY_NAME).join("userdata");
    let dev = fs::canonicalize(&link).with_context(|| format!("resolve {}", link.display()))?;
    let name = dev.file_name().ok_or_else(|| anyhow!("{} has no device name", dev.display()))?;
    let sectors: u64 = parse(&read(Path::new(SYS_BLOCK).join(name).join("size"))?, "block size")?;
    let free = if is_mountpoint(Path::new("/data"))? { Some(free_bytes("/data")?) } else { None };
    Ok(Storage { total: sectors * 512, free })
}

/// A directory is a mountpoint when it lives on a different filesystem from
/// its parent.
fn is_mountpoint(path: &Path) -> Result<bool> {
    let parent = path.parent().ok_or_else(|| anyhow!("{} has no parent", path.display()))?;
    let here = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let up = fs::metadata(parent).with_context(|| format!("stat {}", parent.display()))?;
    Ok(here.dev() != up.dev())
}

fn free_bytes(path: &str) -> Result<u64> {
    let c = CString::new(path)?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| format!("statvfs {path}"));
    }
    // `fsblkcnt_t` is 32 bits on some hosts and 64 bits on Android.
    #[allow(clippy::useless_conversion)]
    let blocks = u64::from(s.f_bfree);
    Ok(blocks * s.f_frsize)
}

/// The first power supply whose type is `Battery`; `None` while no fuel-gauge
/// driver is loaded.
fn battery(root: &Path) -> Result<Option<Battery>> {
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("read {}", root.display())),
    };
    let mut dirs: Vec<_> = entries.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.sort();
    for dir in dirs {
        if read_opt(dir.join("type"))?.as_deref() != Some("Battery") {
            continue;
        }
        let charge = match read(dir.join("status"))?.as_str() {
            "Charging" => Charge::Charging,
            "Discharging" => Charge::Discharging,
            "Not charging" => Charge::NotCharging,
            "Full" => Charge::Full,
            "Unknown" => Charge::Unknown,
            other => bail!("{}: status {other:?}", dir.display()),
        };
        return Ok(Some(Battery {
            level: parse(&read(dir.join("capacity"))?, "capacity")?,
            charge,
            temp: parse_opt(dir.join("temp"), "temp")?,
            health: read_opt(dir.join("health"))?,
            cycles: parse_opt(dir.join("cycle_count"), "cycle_count")?,
            design: parse_opt(dir.join("charge_full_design"), "charge_full_design")?,
            full: parse_opt(dir.join("charge_full"), "charge_full")?,
            voltage: parse_opt(dir.join("voltage_now"), "voltage_now")?,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpulist_ranges_and_singles() {
        assert_eq!(cpulist("0-7").unwrap(), (0..8).collect::<Vec<_>>());
        assert_eq!(cpulist("0-3,6,8-9").unwrap(), vec![0, 1, 2, 3, 6, 8, 9]);
        assert_eq!(cpulist("0").unwrap(), vec![0]);
        assert!(cpulist("3-1").is_err());
        assert!(cpulist("").is_err());
    }

    #[test]
    fn mem_total_from_meminfo() {
        // Values reported by a Xiaomi 17 Ultra.
        let text = "MemTotal:       15472464 kB\nMemFree:         2381608 kB\n";
        assert_eq!(mem_total(text).unwrap(), 15_472_464);
        assert!(mem_total("MemFree: 1 kB\n").is_err());
    }

    #[test]
    fn kernel_release_is_readable() {
        assert!(!kernel_release().unwrap().is_empty());
    }
}
