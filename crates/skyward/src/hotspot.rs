//! The phone as an access point: another device joins the phone's own network
//! when there is no router to join, and reaches the files served on it.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Where the phone's own AP configuration is kept between starts.
const KEPT: &str = "/metadata/skyward/hotspot";

/// The program that runs the access point, and the one that hands addresses
/// out on it.
const HOSTAPD: &str = "/vendor/bin/hw/hostapd";
const DNSMASQ: &str = "/system/bin/dnsmasq";

/// The address the phone takes on the hotspot, and the range it hands out.
pub const ADDRESS: &str = "192.168.43.1";
const NETMASK: &str = "255.255.255.0";
const RANGE: (&str, &str) = ("192.168.43.50", "192.168.43.200");

/// What a hotspot is called and how it is protected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub ssid: String,
    pub password: String,
    /// The band it runs on, as hostapd names it.
    pub band: String,
    pub channel: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { ssid: "Skyward".to_string(), password: String::new(), band: "2".to_string(), channel: 6 }
    }
}

impl Settings {
    /// The name a phone shows for a hotspot it has not been given one for.
    pub fn named(index: u32) -> Settings {
        Settings { ssid: format!("Skyward-{index:04}"), ..Settings::default() }
    }

    /// The password made of the phone's own, so that it is not nothing and not
    /// the same on every phone: eight characters, as WPA wants.
    pub fn with_password(mut self, seed: &str) -> Settings {
        const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
        let mut out = String::new();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in seed.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        for _ in 0..8 {
            out.push(ALPHABET[(hash % ALPHABET.len() as u64) as usize] as char);
            hash /= 7;
            hash ^= hash >> 13;
        }
        self.password = out;
        self
    }

    fn hostapd_conf(&self, interface: &str, driver: &str) -> String {
        format!(
            "interface={interface}\ndriver={driver}\nssid={}\nhw_mode=g\nchannel={}\n\
             auth_algs=1\nwpa=2\nwpa_key_mgmt=WPA-PSK\nwpa_pairwise=CCMP\nrsn_pairwise=CCMP\n\
             wpa_passphrase={}\nignore_broadcast_ssid=0\ndtim_period=2\n\
             ieee80211n=1\nhw_mode=a\nchannel={}\nht_capab=[HT40+]\n",
            self.ssid, self.channel, self.password, self.channel
        )
    }
}

/// The access point while it runs: the program that runs it, and the one
/// handing addresses out on it.
pub struct Hotspot {
    hostapd: Child,
    dnsmasq: Option<Child>,
    interface: String,
    saved: String,
}

impl Hotspot {
    /// Bring the access point up on `interface`, with `settings`.
    pub fn start(interface: &str, driver: &str, settings: &Settings, libraries: &[String]) -> Result<Hotspot> {
        let conf = PathBuf::from("/tmp/hostapd.conf");
        fs_write(&conf, settings.hostapd_conf(interface, driver).as_bytes())?;

        // The interface is taken down and given the hotspot's own address
        // before the access point comes up on it.
        let _ = Command::new("/system/bin/ip").args(["link", "set", interface, "down"]).status();
        let _ = Command::new("/system/bin/ip").args(["addr", "flush", "dev", interface]).status();
        let up = Command::new("/system/bin/ip").args(["addr", "add", &format!("{ADDRESS}/24"), "dev", interface]).status();
        if up.map(|s| !s.success()).unwrap_or(true) {
            bail!("the hotspot's address was not given to {interface}");
        }
        let _ = Command::new("/system/bin/ip").args(["link", "set", interface, "up"]).status();

        let saved = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let mut command = Command::new(HOSTAPD);
        command.arg("-B").arg(&conf);
        if !libraries.is_empty() {
            command.env("LD_LIBRARY_PATH", libraries.join(":"));
        }
        let started = command.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().context("start hostapd")?;
        let hostapd = started;
        thread::sleep(Duration::from_millis(800));

        // dnsmasq hands addresses out on the interface, and nothing else: the
        // phone is not a router for the device that joins.
        let dnsmasq = Command::new(DNSMASQ)
            .args([
                "--interface",
                interface,
                "--dhcp-range",
                &format!("{},{}", RANGE.0, RANGE.1),
                "--dhcp-option=3",
                ADDRESS,
                "--dhcp-option=6",
                ADDRESS,
                "--no-resolv",
                "--no-hosts",
                "--port=0",
                "--bind-interfaces",
                "--pid-file=/tmp/dnsmasq.pid",
                "--dhcp-leasefile=/tmp/dnsmasq.leases",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        Ok(Hotspot { hostapd, dnsmasq, interface: interface.to_string(), saved })
    }

    /// The address another device reaches the phone at once joined.
    pub fn address(&self) -> &'static str {
        ADDRESS
    }

    /// Whether the access point still runs.
    pub fn running(&mut self) -> bool {
        matches!(self.hostapd.try_wait(), Ok(None))
    }

    /// How many devices have joined.
    pub fn joined(&self) -> u32 {
        let leases = std::fs::read_to_string("/tmp/dnsmasq.leases").unwrap_or_default();
        leases.lines().filter(|line| !line.trim().is_empty()).count() as u32
    }

    pub fn stop(mut self) {
        self.shut();
    }

    fn shut(&mut self) {
        if let Some(mut d) = self.dnsmasq.take() {
            let _ = d.kill();
            let _ = d.wait();
        }
        let _ = self.hostapd.kill();
        let _ = self.hostapd.wait();
        let _ = Command::new("/system/bin/ip").args(["addr", "flush", "dev", &self.interface]).status();
        if !self.saved.is_empty() {
            std::env::set_var("LD_LIBRARY_PATH", &self.saved);
        }
    }
}

impl Drop for Hotspot {
    fn drop(&mut self) {
        self.shut();
    }
}

fn fs_write(path: &Path, body: &[u8]) -> Result<()> {
    let mut file = std::fs::File::create(path).with_context(|| format!("make {}", path.display()))?;
    file.write_all(body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// The settings kept from the starts before, if any were.
pub fn read() -> Option<Settings> {
    let text = std::fs::read_to_string(KEPT).ok()?;
    let mut out = Settings::default();
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else { continue };
        match name.trim() {
            "ssid" => out.ssid = value.trim().to_string(),
            "password" => out.password = value.trim().to_string(),
            "band" => out.band = value.trim().to_string(),
            "channel" => out.channel = value.trim().parse().unwrap_or(6),
            _ => {}
        }
    }
    Some(out)
}

/// Keep `settings` for the starts that follow.
pub fn write(settings: &Settings) -> Result<()> {
    let body = format!("ssid={}\npassword={}\nband={}\nchannel={}\n", settings.ssid, settings.password, settings.band, settings.channel);
    let metadata = Path::new("/metadata");
    if let Some(dir) = Path::new(KEPT).parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
    }
    let mounted = skyward_block::mount::is_mount_point(metadata).unwrap_or(false);
    if mounted {
        skyward_block::mount::remount_writable(metadata, true)?;
    }
    let put = fs_write(Path::new(KEPT), body.as_bytes());
    if mounted {
        let back = skyward_block::mount::remount_writable(metadata, false);
        put?;
        back?;
    } else {
        put?;
    }
    Ok(())
}

/// Whether the access point came up, waited for a moment.
pub fn came_up(interface: &str) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        let state = std::fs::read_to_string(format!("/sys/class/net/{interface}/operstate")).unwrap_or_default();
        if state.trim() == "up" {
            return true;
        }
        thread::sleep(Duration::from_millis(150));
    }
    false
}
