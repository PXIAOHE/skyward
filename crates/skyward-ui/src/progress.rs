//! How far the phone's parts have come up, so a page can say what it is
//! waiting for instead of standing still without a word. The recovery draws
//! its first frame before any of this is done; the work itself runs behind
//! the screen and reports here as it passes each stage.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// Nothing has begun: the screen has just been drawn.
pub const BEGIN: u8 = 0;
/// The phone's own partitions are being mounted.
pub const PARTITIONS: u8 = 1;
/// The drivers the recovery needs are being loaded.
pub const DRIVERS: u8 = 2;
/// The phone's service descriptions are being published to its directory.
pub const DESCRIPTIONS: u8 = 3;
/// The phone's own services are being started.
pub const SERVICES: u8 = 4;
/// Everything the pages wait for is up.
pub const READY: u8 = 5;

static STAGE: AtomicU8 = AtomicU8::new(BEGIN);

/// Where the work has come to.
pub fn stage() -> u8 {
    STAGE.load(Ordering::Relaxed)
}

/// Say the work has come this far.
pub fn reached(stage: u8) {
    STAGE.store(stage, Ordering::Relaxed);
}

/// What a page says it is waiting for, in the order the work passes it.
pub fn waiting(stage: u8) -> [&'static str; 3] {
    match stage {
        BEGIN => ["正在准备手机的分区", "正在準備手機的分割區", "Getting the phone's partitions ready"],
        PARTITIONS => ["正在挂载分区", "正在掛載分割區", "Mounting the partitions"],
        DRIVERS => ["正在加载驱动", "正在載入驅動程式", "Loading the drivers"],
        DESCRIPTIONS => ["正在登记系统服务", "正在登記系統服務", "Registering the system services"],
        SERVICES => ["正在启动系统服务", "正在啟動系統服務", "Starting the system services"],
        _ => ["正在读取锁屏方式", "正在讀取螢幕鎖定方式", "Reading the screen lock"],
    }
}

/// Milliseconds since the screen first asked, for the ring that turns while
/// the work runs: a page is drawn again every frame it is still going.
pub fn phase_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// How much of the work is behind us, from none to all of it.
pub fn done() -> f32 {
    (stage() as f32 / READY as f32).clamp(0.0, 1.0)
}
