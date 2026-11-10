//! Xiaomi touch v2 ABI from miro's installed xiaomi_touch.ko BTF and code.
//! This is not the older Xiaomi int[256] ioctl protocol.

use anyhow::{ensure, Context, Result};
use std::fs::{self, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, Instant};

const SELECT_TOUCH: libc::c_ulong = 0x5403;
const COMMON_DATA: libc::c_ulong = 0xc4085400;
const DOUBLE_TAP: u16 = 14;
const SET_CURRENT: u8 = 0;
const GET_CURRENT: u8 = 1;
const GESTURE_MODE: &str = "/sys/bus/spi/devices/spi0.0/fts_gesture_mode";

#[repr(C)]
struct Packet {
    touch_id: u8,
    command: u8,
    mode: u16,
    count: u16,
    reserved: u16,
    data: [i32; 256],
}

impl Packet {
    fn new(command: u8, value: i32) -> Self {
        let mut packet = Self { touch_id: 0, command, mode: DOUBLE_TAP, count: 1, reserved: 0, data: [0; 256] };
        packet.data[0] = value;
        packet
    }
}

fn set_and_check(enabled: bool, mut exchange: impl FnMut(&mut Packet) -> Result<()>) -> Result<()> {
    let desired = i32::from(enabled);
    exchange(&mut Packet::new(SET_CURRENT, desired)).context("set touch double-tap mode")?;
    let mut read = Packet::new(GET_CURRENT, 0);
    exchange(&mut read).context("read touch double-tap mode back")?;
    ensure!(read.mode == DOUBLE_TAP && read.count >= 1 && read.data[0] == desired, "touch double-tap readback disagrees");
    Ok(())
}

fn gesture_matches(text: &str, enabled: bool) -> bool {
    text.lines().any(|line| line.trim() == if enabled { "Gesture Mode:On" } else { "Gesture Mode:Off" })
}

/// Configure while the display is lit, before the panel's suspend callback.
/// A successful ioctl is not enough: the asynchronous FocalTech callback
/// must also have applied the gesture-support flag before blanking.
pub fn xiaomi_v2_double_tap(enabled: bool) -> Result<()> {
    // The FocalTech gesture node miro checks for does not exist on a Goodix
    // panel: the ioctl is tried on its own here, and the readback decides.
    let file = OpenOptions::new().read(true).write(true).open("/dev/xiaomi-touch").context("open Xiaomi touch control")?;
    // The selected touch ID belongs to this fd. The argument is a value,
    // not a pointer, even though COMMON_DATA below takes a full packet.
    if unsafe { libc::ioctl(file.as_raw_fd(), SELECT_TOUCH as _, 0 as libc::c_ulong) } < 0 {
        return Err(std::io::Error::last_os_error()).context("select the primary touch controller");
    }
    set_and_check(enabled, |packet| {
        if unsafe { libc::ioctl(file.as_raw_fd(), COMMON_DATA as _, packet as *mut Packet) } < 0 {
            return Err(std::io::Error::last_os_error()).context("exchange Xiaomi touch v2 packet");
        }
        Ok(())
    })?;
    let until = Instant::now() + Duration::from_millis(500);
    loop {
        let text = fs::read_to_string(GESTURE_MODE).context("read FocalTech gesture support")?;
        if gesture_matches(&text, enabled) {
            return Ok(());
        }
        ensure!(Instant::now() < until, "FocalTech did not apply the requested gesture mode");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_matches_the_installed_modules_btf() {
        assert_eq!(std::mem::size_of::<Packet>(), 1032);
        assert_eq!(std::mem::offset_of!(Packet, command), 1);
        assert_eq!(std::mem::offset_of!(Packet, mode), 2);
        assert_eq!(std::mem::offset_of!(Packet, count), 4);
        assert_eq!(std::mem::offset_of!(Packet, data), 8);
        assert_eq!(COMMON_DATA, (3 << 30) | (1032 << 16) | (u64::from(b'T') << 8));
        assert_eq!(SELECT_TOUCH, (u64::from(b'T') << 8) | 3);
    }

    #[test]
    fn sets_only_double_tap_then_checks_current_value() {
        for enabled in [false, true] {
            let mut calls = Vec::new();
            set_and_check(enabled, |packet| {
                assert_eq!((packet.touch_id, packet.mode, packet.count, packet.reserved), (0, 14, 1, 0));
                assert!(packet.data[1..].iter().all(|v| *v == 0));
                calls.push(packet.command);
                if packet.command == SET_CURRENT {
                    assert_eq!(packet.data[0], i32::from(enabled));
                } else {
                    packet.data[0] = i32::from(enabled);
                }
                Ok(())
            }).unwrap();
            assert_eq!(calls, [SET_CURRENT, GET_CURRENT]);
        }
    }

    #[test]
    fn refuses_failed_or_disagreeing_readback() {
        assert!(set_and_check(true, |_| Ok(())).is_err());
        for fail_at in [SET_CURRENT, GET_CURRENT] {
            assert!(set_and_check(true, |packet| {
                ensure!(packet.command != fail_at, "injected ioctl failure");
                Ok(())
            }).is_err());
        }
    }

    #[test]
    fn checks_the_support_flag_not_the_lit_panels_register() {
        assert!(gesture_matches("Gesture Mode:On\nReg(0xD0)=0\n", true));
        assert!(!gesture_matches("Gesture Mode:Off\nReg(0xD0)=1\n", true));
        assert!(gesture_matches("Gesture Mode:Off\nReg(0xD0)=0\n", false));
        assert!(!gesture_matches("", false));
    }
}
