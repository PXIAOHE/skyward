use anyhow::{anyhow, Context, Result};
use evdev::raw_stream::RawDevice;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

// linux/input-event-codes.h
const EV_FF: u16 = 0x15;
const FF_PERIODIC: u16 = 0x51;
const FF_CUSTOM: u16 = 0x5d;

/// The vibration motor, played through the kernel's force-feedback
/// events. Its driver keeps waveforms of its own and plays one until told
/// to stop, so each click is stopped once the configured length has
/// passed.
pub struct Haptics {
    dev: File,
    slot: i16,
    length: Duration,
    /// Counts clicks, so a stop meant for one does not cut the next short.
    clicks: Arc<AtomicU64>,
}

impl Haptics {
    /// The force-feedback device with the configured click loaded.
    pub fn open(cfg: &skyward_config::Haptics) -> Result<Haptics> {
        let (path, mut dev) = motor()?;
        let slot = upload(&dev, cfg.click).with_context(|| format!("load waveform {} on {}", cfg.click, path.display()))?;
        let length = Duration::from_millis(cfg.click_ms);
        eprintln!("haptics {}: click is waveform {} in slot {slot}, {} ms", path.display(), cfg.click, cfg.click_ms);
        // Nothing may be left playing from before.
        play(&mut dev, slot, 0).context("quiet the motor")?;
        Ok(Haptics { dev, slot, length, clicks: Arc::new(AtomicU64::new(0)) })
    }

    /// One short click.
    pub fn click(&mut self) -> Result<()> {
        play(&mut self.dev, self.slot, 1).context("play the click")?;
        let this = self.clicks.fetch_add(1, Ordering::SeqCst) + 1;
        let mut dev = self.dev.try_clone().context("share the motor")?;
        let (clicks, slot, length) = (self.clicks.clone(), self.slot, self.length);
        std::thread::spawn(move || {
            std::thread::sleep(length);
            if clicks.load(Ordering::SeqCst) == this {
                if let Err(e) = play(&mut dev, slot, 0) {
                    eprintln!("stop the click: {e}");
                }
            }
        });
        Ok(())
    }
}

/// Load waveform `effect`, play it once and stop it after `stop_ms`, for
/// trying out a motor's waveforms and their lengths.
pub fn probe(effect: i16, stop_ms: u64) -> Result<()> {
    let (path, mut dev) = motor()?;
    let slot = upload(&dev, effect).context("upload")?;
    play(&mut dev, slot, 1).context("play")?;
    std::thread::sleep(Duration::from_millis(stop_ms));
    play(&mut dev, slot, 0).context("stop")?;
    println!("{}: played waveform {effect} for {stop_ms} ms", path.display());
    Ok(())
}

/// The first input device under /dev/input that takes periodic
/// force-feedback effects, opened for writing.
fn motor() -> Result<(PathBuf, File)> {
    for (path, dev) in evdev::raw_stream::enumerate() {
        if takes_periodic(&dev) {
            let file = OpenOptions::new().read(true).write(true).open(&path).with_context(|| format!("open {}", path.display()))?;
            return Ok((path, file));
        }
    }
    Err(anyhow!("no input device takes force-feedback effects"))
}

fn takes_periodic(dev: &RawDevice) -> bool {
    dev.supported_ff().is_some_and(|ff| ff.contains(evdev::FFEffectCode::FF_PERIODIC))
}

/// Load the driver's waveform `id` as a custom periodic effect; returns the
/// slot the kernel gave it.
fn upload(dev: &File, id: i16) -> io::Result<i16> {
    // The driver reads the waveform number from the first of three values.
    let mut data: [i16; 3] = [id, 0, 0];
    let periodic = libc::ff_periodic_effect {
        waveform: FF_CUSTOM,
        period: 0,
        magnitude: i16::MAX,
        offset: 0,
        phase: 0,
        envelope: libc::ff_envelope { attack_length: 0, attack_level: 0, fade_length: 0, fade_level: 0 },
        custom_len: data.len() as u32,
        custom_data: data.as_mut_ptr(),
    };
    let mut effect = libc::ff_effect {
        type_: FF_PERIODIC,
        id: -1,
        direction: 0,
        trigger: libc::ff_trigger { button: 0, interval: 0 },
        replay: libc::ff_replay { length: 0, delay: 0 },
        u: [0; 4],
    };
    // The union holds the periodic effect; it is exactly as large.
    const _: () = assert!(std::mem::size_of::<libc::ff_periodic_effect>() == std::mem::size_of::<[u64; 4]>());
    effect.u = unsafe { std::mem::transmute::<libc::ff_periodic_effect, [u64; 4]>(periodic) };
    // EVIOCSFF: _IOW('E', 0x80, struct ff_effect)
    let size = std::mem::size_of::<libc::ff_effect>() as u32;
    let request = (1u32 << 30) | (size << 16) | (u32::from(b'E') << 8) | 0x80;
    if unsafe { libc::ioctl(dev.as_raw_fd(), request as _, &mut effect) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(effect.id)
}

/// Start (`value` 1) or stop (0) the loaded effect in slot `id`.
fn play(dev: &mut File, id: i16, value: i32) -> io::Result<()> {
    let event = libc::input_event { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: EV_FF, code: id as u16, value };
    let bytes = unsafe { std::slice::from_raw_parts((&event as *const libc::input_event).cast::<u8>(), std::mem::size_of::<libc::input_event>()) };
    dev.write_all(bytes)
}
