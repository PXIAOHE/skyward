//! A page from NetSurf, and the frame it draws.
//!
//! NetSurf runs as a child of the recovery, not linked into it: a page that
//! runs away with its memory, or a renderer that falls over on a strange one,
//! costs a process rather than the recovery.
//!
//! Words go to the child's standard input, in the language its frontend
//! already speaks:
//!
//! ```text
//! WINDOW NEW WIN 0 X 0 Y 0 W <w> H <h>
//! WINDOW GO 0 <url>
//! WINDOW REDRAW 0
//! WINDOW CLICK WIN 0 X <x> Y <y> BUTTON l KIND <down|up>
//! ```
//!
//! Pixels come back through one file it maps: a header, the page, and a
//! counter that moves whenever the page does.

#![allow(dead_code)]

use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Where the child leaves the page it has drawn.
pub const FRAME: &str = "/tmp/skyward-frame";

const MAGIC: u32 = 0x534b_5946; // "SKYF"
const HEADER: usize = 32;

/// A page of pixels, as the child left them.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    /// Four bytes a pixel, red first.
    pub pixels: &'a [u8],
}

#[repr(C)]
struct Header {
    magic: u32,
    width: u32,
    height: u32,
    stride: u32,
    seq: u64,
}

struct Page {
    child: Child,
    mapped: *mut u8,
    length: usize,
    seen: u64,
}

// The mapping is owned here and only read while it is held.
unsafe impl Send for Page {}

impl Drop for Page {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if !self.mapped.is_null() {
            unsafe {
                libc::munmap(self.mapped.cast(), self.length);
            }
        }
    }
}

static PAGE: OnceLock<Mutex<Option<Page>>> = OnceLock::new();

fn page() -> MutexGuard<'static, Option<Page>> {
    PAGE.get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// The program that draws pages; a build stage puts it in the ramdisk.
fn renderer() -> String {
    std::env::var("SKYWARD_BROWSER").unwrap_or_else(|_| "/system/bin/nsmonkey".to_string())
}

fn say(page: &mut Page, line: &str) -> Result<()> {
    let stdin = page
        .child
        .stdin
        .as_mut()
        .context("the renderer takes no commands")?;
    stdin.write_all(line.as_bytes())?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

/// Start the renderer on an address, if it is not already running.
pub fn start(width: u32, height: u32, url: &str) -> Result<()> {
    let mut held = page();
    if held.is_some() {
        return Ok(());
    }

    let child = Command::new(renderer())
        .args([
            "-f",
            "ram",
            "-w",
            &width.to_string(),
            "-h",
            &height.to_string(),
            url,
        ])
        // The face the renderer draws text with is the one the recovery
        // carries, and the frame file is the one already mapped below.
        .env("SKYWARD_FONT", "/skyward/MiSansVF.ttf")
        .env("SKYWARD_WIDTH", width.to_string())
        .env("SKYWARD_HEIGHT", height.to_string())
        .env("SKYWARD_FRAME", FRAME)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn {}", renderer()))?;

    let length = HEADER + width as usize * height as usize * 4;
    let at = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            length,
            libc::PROT_READ,
            libc::MAP_SHARED,
            OpenOptions::new().read(true).open(FRAME)?.as_raw_fd(),
            0,
        )
    };
    if at == libc::MAP_FAILED {
        return Err(anyhow::anyhow!("map {FRAME}"));
    }

    *held = Some(Page {
        child,
        mapped: at.cast(),
        length,
        seen: 0,
    });
    Ok(())
}

/// Go somewhere: the renderer is started again on the new address.
pub fn open(width: u32, height: u32, url: &str) -> Result<()> {
    close();
    start(width, height, url)
}

/// Ask for the page again.
pub fn redraw() -> Result<()> {
    Ok(())
}

/// Where the frontend listens for a finger.
fn input_path() -> String {
    std::env::var("SKYWARD_INPUT").unwrap_or_else(|_| "/tmp/skyward-input".to_string())
}

fn say_to_renderer(line: &str) -> Result<()> {
    static STREAM: OnceLock<Mutex<Option<UnixStream>>> = OnceLock::new();
    let held = STREAM.get_or_init(|| Mutex::new(None));
    let mut slot = held.lock().unwrap_or_else(|poison| poison.into_inner());

    if slot.is_none() {
        match UnixStream::connect(input_path()) {
            Ok(stream) => *slot = Some(stream),
            Err(_) => return Ok(()), // the renderer is not listening yet
        }
    }
    if let Some(stream) = slot.as_mut() {
        if stream.write_all(line.as_bytes()).is_err() {
            *slot = None;
        }
    }
    Ok(())
}

/// A finger on the page.
pub fn press(x: i32, y: i32) -> Result<()> {
    say_to_renderer(&format!("m {x} {y}\nd {x} {y}\n"))
}

/// The finger lifted.
pub fn release(x: i32, y: i32) -> Result<()> {
    say_to_renderer(&format!("u {x} {y}\n"))
}

/// The wheel, one notch.
pub fn scroll(down: bool) -> Result<()> {
    say_to_renderer(if down { "w down\n" } else { "w up\n" })
}

/// The page as it stands, or nothing when it has not moved since last look.
pub fn frame() -> Option<Frame<'static>> {
    let mut held = page();
    let page = held.as_mut()?;

    let header = page.mapped.cast::<Header>();
    let magic = unsafe { std::ptr::read_volatile(&(*header).magic) };
    if magic != MAGIC {
        return None;
    }
    let seq = unsafe { std::ptr::read_volatile(&(*header).seq) };
    if seq == page.seen {
        return None;
    }
    let width = unsafe { std::ptr::read_volatile(&(*header).width) } as usize;
    let height = unsafe { std::ptr::read_volatile(&(*header).height) } as usize;
    let bytes = width * height * 4;
    if width == 0 || height == 0 || HEADER + bytes > page.length {
        return None;
    }

    // The counter moves after the pixels are written, so a frame is never
    // seen half drawn; it may be seen twice.
    page.seen = seq;
    let pixels = unsafe { std::slice::from_raw_parts(page.mapped.add(HEADER), bytes) };
    Some(Frame {
        width,
        height,
        pixels,
    })
}

/// Whether a renderer is running.
pub fn running() -> bool {
    page().is_some()
}

/// Stop the renderer and let go of what was mapped for it.
pub fn close() {
    let mut held = page();
    *held = None;
}

/// Make the file the child maps, before it is started.
pub fn prepare(width: u32, height: u32) -> Result<()> {
    let file = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(FRAME)
        .with_context(|| format!("create {FRAME}"))?;
    file.set_len(HEADER as u64 + width as u64 * height as u64 * 4)?;
    Ok(())
}
