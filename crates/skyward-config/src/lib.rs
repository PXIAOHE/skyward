use anyhow::{ensure, Context, Result};
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
    /// The scheduling profiles libprocessgroup reads.
    pub const TASK_PROFILES: &str = "system/etc/task_profiles.json";
    /// The init rules that set up file transfer's USB function.
    pub const MTP_RC: &str = "system/etc/init/skyward-mtp.rc";

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
    #[serde(default)]
    pub wipe: Wipe,
    #[serde(default)]
    pub backup: Backup,
    /// The phone's WLAN, on phones whose driver the recovery brings up.
    #[serde(default)]
    pub wlan: Option<Wlan>,
    /// Where the phone maker answers which system is the newest, on phones
    /// whose server the recovery knows.
    #[serde(default)]
    pub update: Option<Update>,
}

/// What brings the phone's WLAN up inside a recovery, which loads none of
/// it: the driver, what it reads as it starts, and the phone's own
/// wpa_supplicant, which joins networks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Wlan {
    /// The driver's kernel modules; what they depend on loads first.
    pub modules: Vec<String>,
    /// What is written once they are in, each value to its file, such as
    /// what tells a platform driver it may start the chip.
    #[serde(default)]
    pub writes: Vec<Write>,
    /// Files the driver asks the kernel's firmware loader for that are not
    /// where the loader looks in a recovery.
    #[serde(default)]
    pub firmware: Vec<Firmware>,
    /// The network interface the driver makes.
    pub interface: String,
    /// The phone's own wpa_supplicant.
    pub supplicant: String,
    /// Directories for its LD_LIBRARY_PATH, searched in order and before
    /// the libraries of the vendor interface the phone was built against.
    #[serde(default)]
    pub libraries: Vec<String>,
    /// The service it registers with the service directory as it starts,
    /// which the directory takes only from a program the running system
    /// describes.
    pub service: String,
}

/// A value written to a file, such as a switch in sysfs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Write {
    pub path: String,
    pub value: String,
}

/// A file a driver asks the firmware loader for, and where it is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Firmware {
    /// The name the driver asks for, from the top of the loader's folders.
    pub name: String,
    /// Where the file is: a path in the running recovery, or from the top
    /// of `partition`.
    pub path: String,
    /// A partition the recovery does not keep mounted, which is mounted
    /// read-only while the file is read.
    #[serde(default)]
    pub partition: Option<String>,
}

/// The phone maker's update server, which the stock updater asks about the
/// installed system.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Update {
    /// Where the questions are sent.
    pub server: String,
}

/// What a backup takes in and leaves out beyond what every Android phone
/// keeps the same.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Backup {
    /// The partitions that keep the baseband's settings, with the phone's
    /// own identity, and the sensors' calibration.
    #[serde(default)]
    pub efs: Vec<String>,
    /// Folders in the data partition, from its top, that the phone's own
    /// secure world keeps in step with itself, as its own storage and the
    /// fingerprint templates: an older copy written back would not open.
    /// A `*` stands for any one name.
    #[serde(default)]
    pub data_skipped: Vec<String>,
}

/// What erasing on the wipe page takes in beyond the places every Android
/// phone keeps the same.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Wipe {
    /// Folders in the data partition where the phone's own programs keep
    /// their logs and dumps: their files go with the logs and crash
    /// records, the folders staying.
    #[serde(default)]
    pub logs: Vec<String>,
}

/// The data partition, which the folders erasing takes in are in.
const DATA: &str = "/data";

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
    /// How round the screen's corners are, as a radius in pixels; a page
    /// sliding over another rounds its leading corners to it.
    #[serde(default)]
    pub corner_radius: u32,
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
        let cfg: DeviceConfig = toml::from_str(text).context("parse device config")?;
        for log in &cfg.wipe.logs {
            let path = Path::new(log);
            let plain = path.components().all(|c| matches!(c, std::path::Component::RootDir | std::path::Component::Normal(_)));
            ensure!(
                path.is_absolute() && plain && path.starts_with(DATA) && path != Path::new(DATA),
                "[wipe] logs: {log} is not a folder inside {DATA}"
            );
        }
        for name in &cfg.backup.efs {
            ensure!(!name.is_empty() && !name.contains('/'), "[backup] efs: {name} is not a partition's name");
        }
        for folder in &cfg.backup.data_skipped {
            let path = Path::new(folder);
            let plain = path.components().all(|c| matches!(c, std::path::Component::Normal(_)));
            ensure!(plain && !folder.is_empty(), "[backup] data_skipped: {folder} is not a path from the top of {DATA}");
        }
        if let Some(wlan) = &cfg.wlan {
            for file in &wlan.firmware {
                let name = Path::new(&file.name);
                let plain = name.components().all(|c| matches!(c, std::path::Component::Normal(_)));
                ensure!(plain && !file.name.is_empty(), "[wlan] firmware: {} is not a name from the top of the loader's folders", file.name);
                let path = Path::new(&file.path);
                match &file.partition {
                    Some(partition) => {
                        let plain = path.components().all(|c| matches!(c, std::path::Component::Normal(_)));
                        ensure!(
                            !partition.is_empty() && !partition.contains('/'),
                            "[wlan] firmware: {partition} is not a partition's name"
                        );
                        ensure!(plain && !file.path.is_empty(), "[wlan] firmware: {} is not a path from the top of {partition}", file.path);
                    }
                    None => ensure!(path.is_absolute(), "[wlan] firmware: {} is not an absolute path", file.path),
                }
            }
        }
        Ok(cfg)
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
                "/vendor/bin/hw/android.hardware.boot@1.2-service",
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
        assert_eq!(cfg.services[6].ready_hardware_service.as_deref(), Some("android.hardware.boot@1.2::IBootControl/default"));
        assert_eq!(cfg.backup.efs, ["modemst1", "modemst2", "fsg", "fsc", "persist"]);
        assert!(cfg.backup.data_skipped.contains(&"vendor/tzstorage".to_string()));
    }

    #[test]
    fn what_a_backup_takes_in_is_named_plainly() {
        let base = "[device]\ncodename = \"x\"\n";
        assert!(DeviceConfig::parse(&format!("{base}[backup]\nefs = [\"by-name/fsg\"]\n")).is_err());
        assert!(DeviceConfig::parse(&format!("{base}[backup]\ndata_skipped = [\"/data/vendor\"]\n")).is_err());
        assert!(DeviceConfig::parse(&format!("{base}[backup]\ndata_skipped = [\"vendor/../system\"]\n")).is_err());
        assert!(DeviceConfig::parse(&format!("{base}[backup]\ndata_skipped = [\"vendor_de/*/fpdata\"]\n")).is_ok());
    }

    #[test]
    fn the_phones_own_logs_are_folders_inside_the_data_partition() {
        let cfg = DeviceConfig::parse(include_str!("../../../devices/ishtar.toml")).unwrap();
        assert!(cfg.wipe.logs.contains(&"/data/vendor/diag".to_string()));
        assert!(DeviceConfig::parse("[device]\ncodename = \"x\"\n").unwrap().wipe.logs.is_empty(), "a phone may have none");
        for bad in ["/data", "/data/", "/metadata/logs", "data/log", "/data/../system"] {
            let text = format!("[device]\ncodename = \"x\"\n[wipe]\nlogs = [\"{bad}\"]\n");
            assert!(DeviceConfig::parse(&text).is_err(), "{bad} is refused");
        }
    }

    #[test]
    fn ishtar_brings_its_wlan_up_from_its_own_partitions() {
        let cfg = DeviceConfig::parse(include_str!("../../../devices/ishtar.toml")).unwrap();
        let wlan = cfg.wlan.expect("the 13 Ultra's WLAN is described");
        assert_eq!(wlan.modules, ["qca_cld3_kiwi_v2"]);
        assert_eq!(wlan.writes, [Write { path: "/sys/kernel/cnss/fs_ready".into(), value: "1".into() }]);
        assert_eq!(wlan.firmware[0].partition, None);
        assert_eq!(wlan.firmware[1].partition.as_deref(), Some("persist"));
        assert_eq!(wlan.firmware[1].path, "kiwi_v2/wlan_mac.bin");
        assert_eq!(wlan.interface, "wlan0");
        assert_eq!(wlan.service, "android.hardware.wifi.supplicant.ISupplicant/default");
        assert_eq!(cfg.update.expect("its update server is known").server, "https://update.miui.com/updates/miotaV3.php");
        let nezha = DeviceConfig::parse(include_str!("../../../devices/nezha.toml")).unwrap();
        assert_eq!((nezha.wlan, nezha.update), (None, None), "a phone may have neither");
    }

    #[test]
    fn firmware_is_named_from_the_loaders_folders_and_found_plainly() {
        let base = "[device]\ncodename = \"x\"\n[wlan]\nmodules = []\ninterface = \"wlan0\"\nsupplicant = \"/s\"\nservice = \"a/b\"\n";
        let with = |file: &str| DeviceConfig::parse(&format!("{base}firmware = [{file}]\n"));
        assert!(with("{ name = \"wlan/a.ini\", path = \"/vendor/etc/a.ini\" }").is_ok());
        assert!(with("{ name = \"wlan/mac.bin\", partition = \"persist\", path = \"kiwi/mac.bin\" }").is_ok());
        assert!(with("{ name = \"/etc/firmware/a.ini\", path = \"/vendor/etc/a.ini\" }").is_err());
        assert!(with("{ name = \"../a.ini\", path = \"/vendor/etc/a.ini\" }").is_err());
        assert!(with("{ name = \"a.ini\", path = \"vendor/etc/a.ini\" }").is_err());
        assert!(with("{ name = \"mac.bin\", partition = \"persist\", path = \"/kiwi/mac.bin\" }").is_err());
        assert!(with("{ name = \"mac.bin\", partition = \"by-name/persist\", path = \"mac.bin\" }").is_err());
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
