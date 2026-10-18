use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// Recovery ramdisk files Skyward reads, adds or changes, as archive paths
/// (they sit under `/` once the kernel unpacks the ramdisk).
pub mod ramdisk {
    /// The device configuration, copied from `devices/<codename>.toml`.
    pub const CONFIG: &str = "skyward/device.toml";
    /// The UI typeface.
    pub const FONT: &str = "skyward/MiSansVF.ttf";
    /// The terminal's monospaced typeface.
    pub const MONO: &str = "skyward/KirkMono-Regular.ttf";
    /// The monospaced typeface's licence, which travels with every copy of
    /// it.
    pub const MONO_LICENSE: &str = "skyward/KirkMono-OFL.txt";
    /// The recovery service binary init starts.
    pub const RECOVERY: &str = "system/bin/recovery";
    /// The first property file init loads in recovery mode.
    pub const PROP_DEFAULT: &str = "prop.default";

    /// Whether the running root is Skyward's recovery ramdisk.
    pub fn in_recovery() -> bool {
        std::path::Path::new("/").join(CONFIG).exists()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceConfig {
    pub device: Device,
    #[serde(default)]
    pub display: Display,
    #[serde(default)]
    pub modules: Modules,
    #[serde(default, rename = "service")]
    pub services: Vec<Service>,
    #[serde(default)]
    pub decrypt: Decrypt,
    /// The vibration motor's click, on phones that have one.
    #[serde(default)]
    pub haptics: Option<Haptics>,
}

/// A waveform the vibration motor's driver keeps, played through the
/// kernel's force-feedback events. The driver plays it until told to stop
/// and does not say how long it is, so its length is given here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Haptics {
    /// The driver's number for its click waveform.
    pub click: i16,
    /// How long the click plays, in milliseconds.
    pub click_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Device {
    pub codename: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_true")]
    pub ab: bool,
    /// SoC marketing name shown on the About page.
    #[serde(default)]
    pub soc: Option<String>,
    /// System property that carries the vendor OS version.
    #[serde(default = "default_os_version_prop")]
    pub os_version_prop: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Display {
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub density: u32,
    /// Marketed screen diagonal in inches.
    #[serde(default)]
    pub diagonal: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Modules {
    /// Partitions the phone keeps its kernel modules on, searched in order.
    #[serde(default = "default_module_sources")]
    pub sources: Vec<String>,
    /// Modules the recovery loads at startup that first-stage init leaves
    /// out, such as the touchscreen driver. Dependencies come from the
    /// source's modules.dep and load first.
    #[serde(default)]
    pub load: Vec<String>,
}

impl Default for Modules {
    fn default() -> Self {
        Self { sources: default_module_sources(), load: Vec::new() }
    }
}

/// A program from the phone's own partitions that the recovery keeps
/// running, such as the touch processing of a panel that hands raw frames
/// to the host.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Service {
    pub path: String,
    /// Directories for LD_LIBRARY_PATH, searched in order.
    #[serde(default)]
    pub libraries: Vec<String>,
    /// A property the program sets once it is ready for the rest. Programs
    /// are started in the order they are written, and one that says it has
    /// something to wait for holds up the ones after it.
    #[serde(default)]
    pub ready_property: Option<String>,
    /// A service the program registers with the phone's service directory
    /// once it is ready.
    #[serde(default)]
    pub ready_service: Option<String>,
    /// A service the program registers with the other directory, the one
    /// the phone's hardware services of the older kind use. It is written
    /// as the interface and then which of them it is.
    #[serde(default)]
    pub ready_hardware_service: Option<String>,
}

impl Service {
    /// Whether the program, or a directory it loads libraries from, is
    /// under `dir`: without what is mounted there it cannot start again.
    pub fn uses(&self, dir: &Path) -> bool {
        Path::new(&self.path).starts_with(dir) || self.libraries.iter().any(|l| Path::new(l).starts_with(dir))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Decrypt {
    #[serde(default)]
    pub weaver: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_os_version_prop() -> String {
    "ro.build.display.id".into()
}

fn default_module_sources() -> Vec<String> {
    vec!["vendor_boot".into(), "vendor_dlkm".into(), "vendor".into()]
}

impl DeviceConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let p = path.as_ref();
        let text = std::fs::read_to_string(p)
            .with_context(|| format!("read config {}", p.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).context("parse device config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let cfg = DeviceConfig::parse("[device]\ncodename = \"nezha\"\n").unwrap();
        assert_eq!(cfg.device.codename, "nezha");
        assert!(cfg.device.ab);
        assert_eq!(cfg.device.os_version_prop, "ro.build.display.id");
        assert_eq!(cfg.modules.sources, default_module_sources());
        assert!(cfg.modules.load.is_empty());
    }

    #[test]
    fn parses_nezha() {
        let cfg = DeviceConfig::parse(include_str!("../../../devices/nezha.toml")).unwrap();
        assert_eq!(cfg.device.codename, "nezha");
        assert_eq!(cfg.device.os_version_prop, "ro.mi.os.version.incremental");
        assert_eq!(cfg.device.soc.as_deref(), Some("第五代骁龙®8至尊版移动平台"));
        assert_eq!(cfg.display.width, 1200);
        assert_eq!(cfg.display.diagonal, Some(6.9));
        assert_eq!(cfg.decrypt.weaver, vec!["goodix", "thales"]);
    }

    #[test]
    fn parses_ishtar() {
        let cfg = DeviceConfig::parse(include_str!("../../../devices/ishtar.toml")).unwrap();
        assert_eq!(cfg.device.codename, "ishtar");
        assert_eq!((cfg.display.width, cfg.display.height, cfg.display.density), (1440, 3200, 560));
        assert!(cfg.decrypt.weaver.is_empty());
        assert_eq!(cfg.modules.load, ["fts_touch_spi"]);
        let paths: Vec<&str> = cfg.services.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/odm/bin/touch_report",
                "/odm/bin/touchsensor",
                "/vendor/bin/qseecomd",
                "/vendor/bin/hw/android.hardware.security.keymint-service-qti",
                "/mnt/system_ext/bin/hwservicemanager",
                "/vendor/bin/hw/android.hardware.gatekeeper@1.0-service-qti",
            ]
        );
        assert_eq!(cfg.services[1].libraries, ["/odm/lib64", "/system/lib64"]);
        assert_eq!(cfg.services[0].ready_property, None, "the touch programs are ready as soon as they run");
        assert_eq!(cfg.services[2].ready_property.as_deref(), Some("vendor.sys.listeners.registered"));
        assert_eq!(
            cfg.services[3].ready_service.as_deref(),
            Some("android.hardware.security.keymint.IKeyMintDevice/default")
        );
        assert_eq!(
            cfg.services[4].ready_property.as_deref(),
            Some("hwservicemanager.ready"),
            "the directory for the hardware services says so itself"
        );
        assert_eq!(
            cfg.services[5].ready_hardware_service.as_deref(),
            Some("android.hardware.gatekeeper@1.0::IGatekeeper/default")
        );
    }

    #[test]
    fn services_use_the_partitions_they_run_from() {
        let cfg = DeviceConfig::parse(include_str!("../../../devices/ishtar.toml")).unwrap();
        let [report, sensor, listeners, ..] = &cfg.services[..] else { panic!("ishtar keeps several programs running") };
        assert!(report.uses(Path::new("/odm")));
        assert!(sensor.uses(Path::new("/odm")));
        assert!(!report.uses(Path::new("/odm_dlkm")), "a longer name is another partition");
        assert!(!sensor.uses(Path::new("/mnt/system")), "its /system/lib64 is the recovery's own");
        assert!(!report.uses(Path::new("/vendor")));
        assert!(listeners.uses(Path::new("/vendor")));
    }
}
