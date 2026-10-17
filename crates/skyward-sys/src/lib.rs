use anyhow::{bail, Result};
use std::fs::OpenOptions;
use std::io::Write;

pub mod bcb;

/// A system property, or `None` when it is unset.
#[cfg(target_os = "android")]
pub fn prop(name: &str) -> Option<String> {
    use std::ffi::{CStr, CString};
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

/// Ask init to set a property; init refuses names the caller's SELinux
/// domain may not set.
#[cfg(target_os = "android")]
pub fn set_prop(name: &str, value: &str) -> Result<()> {
    use std::ffi::CString;
    let (k, v) = (CString::new(name)?, CString::new(value)?);
    if unsafe { libc::__system_property_set(k.as_ptr(), v.as_ptr()) } != 0 {
        bail!("init refused {name}={value}");
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
pub fn set_prop(name: &str, _value: &str) -> Result<()> {
    bail!("this host has no property service to set {name}")
}

/// Where a power request sends the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    System,
    Recovery,
    Bootloader,
    /// Qualcomm emergency download mode.
    Edl,
    Off,
}

impl Power {
    pub fn parse(name: &str) -> Option<Power> {
        Some(match name {
            "system" => Power::System,
            "recovery" => Power::Recovery,
            "bootloader" => Power::Bootloader,
            "edl" => Power::Edl,
            "off" => Power::Off,
            _ => return None,
        })
    }

    /// The `sys.powerctl` value; init syncs and unmounts before it acts.
    pub fn command(self) -> &'static str {
        match self {
            Power::System => "reboot,",
            Power::Recovery => "reboot,recovery",
            Power::Bootloader => "reboot,bootloader",
            Power::Edl => "reboot,edl",
            Power::Off => "shutdown,",
        }
    }
}

pub fn power(target: Power) -> Result<()> {
    set_prop("sys.powerctl", target.command())
}

/// Append a line to the kernel log, where it survives until the next boot
/// and shows in `dmesg`.
pub fn kmsg(line: &str) -> Result<()> {
    let mut f = OpenOptions::new().write(true).open("/dev/kmsg")?;
    f.write_all(format!("<6>skyward: {line}\n").as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_names_round_trip() {
        for (name, cmd) in [
            ("system", "reboot,"),
            ("recovery", "reboot,recovery"),
            ("bootloader", "reboot,bootloader"),
            ("edl", "reboot,edl"),
            ("off", "shutdown,"),
        ] {
            assert_eq!(Power::parse(name).unwrap().command(), cmd);
        }
        assert_eq!(Power::parse("fastboot"), None);
    }
}
