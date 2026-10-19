use crate::app::{Data, EraseStop, FileKind, FileRow, MountFailure, Part, BACKUP_PARTS};
use skyward_info::{Battery, Charge, Cpu, PatchLevel, Storage};
use skyward_install::ota::Trouble;
use skyward_install::{Failure, ImageKind, Ramdisk, Refusal};
use skyward_vab::{Hold, Slot, SnapshotState};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Marketed RAM size: the smallest standard module size that holds `MemTotal`.
pub fn ram(kib: u64) -> String {
    const SIZES: [f64; 16] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 18.0, 20.0, 24.0, 32.0, 48.0, 64.0];
    let gib = kib as f64 / (1024.0 * 1024.0);
    let size = SIZES.iter().copied().find(|&s| s >= gib).unwrap_or(gib.ceil());
    format!("{size:.1} GB")
}

/// Marketed storage size in GB: the smallest power of two (in GiB) that holds
/// the userdata partition.
pub fn capacity_gb(total: u64) -> u64 {
    let gib = total as f64 / GIB;
    let mut gb = 8;
    while (gb as f64) < gib {
        gb *= 2;
    }
    gb
}

pub fn capacity(total: u64) -> String {
    match capacity_gb(total) {
        gb if gb >= 1024 => format!("{}TB", gb / 1024),
        gb => format!("{gb}GB"),
    }
}

pub fn gb(bytes: u64) -> String {
    format!("{:.1}GB", bytes as f64 / 1e9)
}

/// `已用/总量` as the stock settings count it: everything that is not free
/// is used, against the marketed size.
pub fn storage_used(s: &Storage) -> Option<String> {
    let free = s.free?;
    let used = capacity_gb(s.total) as f64 - free as f64 / 1e9;
    Some(format!("{used:.1}GB/{}", capacity(s.total)))
}

fn numeral(n: u32) -> String {
    const D: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    match n {
        2 => "双".into(),
        0..=9 => D[n as usize].into(),
        10 => "十".into(),
        11..=19 => format!("十{}", D[(n % 10) as usize]),
        20..=99 if n.is_multiple_of(10) => format!("{}十", D[(n / 10) as usize]),
        20..=99 => format!("{}十{}", D[(n / 10) as usize], D[(n % 10) as usize]),
        _ => n.to_string(),
    }
}

/// `八核 最高 3.19GHz`
pub fn cpu(c: &Cpu) -> String {
    let cores = format!("{}核", numeral(c.cores));
    match c.max_khz {
        Some(khz) => format!("{cores} 最高 {:.2}GHz", khz as f64 / 1e6),
        None => cores,
    }
}

/// A marketing name laid out the way the stock settings stack it in large
/// type: an edition credit ("by Leica") starts its own line.
pub fn model_lines(name: &str) -> String {
    name.replacen(" by ", "\nby ", 1)
}

pub fn panel((w, h): (u32, u32)) -> String {
    format!("{} x {}", w.max(h), w.min(h))
}

pub fn inches(d: f32) -> String {
    format!("{d}″")
}

pub fn mah(uah: u64) -> String {
    format!("{}mAh", uah / 1000)
}

pub fn charge(c: Charge) -> &'static str {
    match c {
        Charge::Charging => "充电中",
        Charge::Discharging | Charge::NotCharging => "未充电",
        Charge::Full => "已充满",
        Charge::Unknown => "状态未知",
    }
}

/// `74% · 充电中`
pub fn level(b: &Battery) -> String {
    format!("{}% · {}", b.level, charge(b.charge))
}

pub fn celsius(tenths: i32) -> String {
    format!("{:.1}°C", tenths as f64 / 10.0)
}

pub fn volts(uv: u64) -> String {
    format!("{:.2}V", uv as f64 / 1e6)
}

/// Kernel health text in Chinese; unlisted states are shown as reported.
pub fn health(h: &str) -> String {
    match h {
        "Good" => "良好",
        "Overheat" | "Hot" => "过热",
        "Warm" => "偏热",
        "Cool" => "偏冷",
        "Cold" => "过冷",
        "Dead" => "损坏",
        "Over voltage" => "电压过高",
        "Over current" => "电流过大",
        "Unspecified failure" => "故障",
        "Calibration required" => "需要校准",
        "No battery" => "未检测到电池",
        "Unknown" => "未知",
        other => return other.to_string(),
    }
    .to_string()
}

/// Health and remaining capacity against design, e.g. `良好 · 剩余容量 93%`.
pub fn wear(b: &Battery) -> Option<String> {
    let state = b.health.as_deref().map(health);
    let left = match (b.full, b.design) {
        (Some(full), Some(design)) if design > 0 => Some(format!("剩余容量 {}%", (full * 100 + design / 2) / design)),
        _ => None,
    };
    match (state, left) {
        (Some(s), Some(l)) => Some(format!("{s} · {l}")),
        (s, l) => s.or(l),
    }
}

pub fn android(release: &str, patch: Option<&PatchLevel>) -> String {
    match patch {
        Some(p) => format!("{release} | Android 安全更新：{}年{}月{}日", p.year, p.month, p.day),
        None => release.to_string(),
    }
}

pub fn verified(state: &str) -> String {
    match state {
        "green" => "绿色 · 已用官方密钥验证",
        "yellow" => "黄色 · 已用自定义密钥验证",
        "orange" => "橙色 · 设备已解锁，不校验系统",
        "red" => "红色 · 验证失败",
        other => return other.to_string(),
    }
    .to_string()
}

pub fn snapshot(s: &SnapshotState) -> &'static str {
    match s {
        SnapshotState::None => "没有待合并的更新",
        SnapshotState::Snapshotted => "更新已写入快照，等待合并",
        SnapshotState::Merging => "正在合并更新",
        SnapshotState::MergeFailed => "更新合并失败",
        SnapshotState::Unknown => "读不到更新状态",
    }
}

/// Why super is off limits while an update is in flight; `None` when it is not.
pub fn snapshot_note(s: &SnapshotState) -> Option<&'static str> {
    match s {
        SnapshotState::None => None,
        SnapshotState::Snapshotted | SnapshotState::Merging => Some("合并完成前不会刷写 super 里的分区"),
        SnapshotState::MergeFailed => Some("回到系统处理完之前不会刷写 super 里的分区"),
        SnapshotState::Unknown => Some("读到更新状态之前不会刷写 super 里的分区"),
    }
}

/// Why the mount page mounts nothing from super while an update is in
/// flight; `None` when it is not.
pub fn snapshot_mount_note(s: &SnapshotState) -> Option<&'static str> {
    match s {
        SnapshotState::None => None,
        SnapshotState::Snapshotted | SnapshotState::Merging => Some("合并完成前不能挂载 super 里的分区"),
        SnapshotState::MergeFailed => Some("回到系统处理完之前不能挂载 super 里的分区"),
        SnapshotState::Unknown => Some("读到更新状态之前不能挂载 super 里的分区"),
    }
}

pub fn data(d: &Data) -> &'static str {
    match d {
        Data::Closed | Data::Opening | Data::Failed(_) => "未挂载",
        Data::Locked { .. } => "未解密",
        Data::Open => "已解密",
        Data::Erased => "已清除",
    }
}

/// A part of the phone erasing goes through, as its page names it.
pub fn part(p: Part) -> &'static str {
    match p {
        Part::Data => "data 分区",
        Part::Cache => "cache 分区",
        Part::Metadata => "metadata 分区",
        Part::Media => "内部存储",
        Part::Art => "ART 缓存",
    }
}

/// `before`, `inner` and `after` joined, with a space between Chinese and
/// the Latin letters or digits at either end of `inner`, as the pages write
/// them: 清除 ART 缓存失败.
fn spaced(before: &str, inner: &str, after: &str) -> String {
    let open = if inner.starts_with(|c: char| c.is_ascii()) && !before.ends_with(|c: char| c.is_ascii()) { " " } else { "" };
    let close = if inner.ends_with(|c: char| c.is_ascii()) && !after.starts_with(|c: char| c.is_ascii()) { " " } else { "" };
    format!("{before}{open}{inner}{close}{after}")
}

/// Why erasing did not start or stopped short, and what the system said of
/// it when there is that.
pub fn erase_stop(s: &EraseStop) -> (String, Option<String>) {
    match s {
        EraseStop::Update(Hold::Unconfirmed) => (
            "当前槽位的系统更新尚未完成，暂无法清除。请先启动系统完成更新后重试。若系统无法启动，多次启动失败后手机会切回原槽位，之后即可清除。".into(),
            None,
        ),
        EraseStop::Update(Hold::Merging) => ("系统更新正在合并，暂无法清除。请先启动系统，合并完成后重试。".into(), None),
        EraseStop::Update(Hold::MergeFailed) => ("系统更新合并失败，暂无法清除。请先启动系统处理后重试。".into(), None),
        EraseStop::NotRebuilt => ("本机系统开机时不会重建已清空的 data 分区，无法清除。".into(), None),
        EraseStop::NoCache => ("本机没有 cache 分区。".into(), None),
        EraseStop::Erased => ("data 分区已清空，下次开机时将重新建立，暂无可清除的内容。".into(), None),
        EraseStop::Closed(why) => ("无法打开 data 分区，未清除任何内容。".into(), Some(why.clone())),
        EraseStop::Failed(p, why) => (spaced("清除", part(*p), "失败。"), Some(why.clone())),
    }
}

/// A file size, `7.1GB` or `96.0MB`.
pub fn bytes(n: u64) -> String {
    match n {
        n if n >= 1_000_000_000 => format!("{:.1}GB", n as f64 / 1e9),
        n if n >= 1_000_000 => format!("{:.1}MB", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0}KB", n as f64 / 1e3),
        n => format!("{n}B"),
    }
}

/// `2026年4月1日`
pub fn patch(p: &PatchLevel) -> String {
    format!("{}年{}月{}日", p.year, p.month, p.day)
}

/// The day a Unix time falls on in China, `2026年5月8日`.
pub fn date(unix: i64) -> String {
    let (year, month, day) = day_in_china(unix);
    format!("{year}年{month}月{day}日")
}

/// The calendar day of a Unix time in China: year, month, day.
fn day_in_china(unix: i64) -> (i64, i64, i64) {
    // Days since 1970 to a calendar date, counting in 400-year eras that
    // start on 1 March.
    let z = (unix + 8 * 3600).div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// When a list entry last changed, as the stock file manager tells it
/// against `now`, when the folder was read: the time for today, month and
/// day for a file from this year, and the whole date for a folder from this
/// year or anything older.
pub fn file_time(unix: i64, folder: bool, now: i64) -> String {
    let (year, month, day) = day_in_china(unix);
    let today = day_in_china(now);
    if (year, month, day) == today {
        clock(unix)
    } else if year == today.0 && !folder {
        format!("{month:02}/{day:02}")
    } else {
        format!("{year}/{month:02}/{day:02}")
    }
}

/// A file's size as the stock file manager writes it: in steps of 1000,
/// going up a unit past 900, with at most two decimals and no trailing
/// zeros.
pub fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let (mut v, mut unit) = (bytes as f64, 0);
    while v > 900.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    let hundredths = (v * 100.0).round() as u64;
    let n = match hundredths {
        h if h % 100 == 0 => format!("{v:.0}"),
        h if h % 10 == 0 => format!("{v:.1}"),
        _ => format!("{v:.2}"),
    };
    format!("{n} {}", UNITS[unit])
}

/// A Unix time in China, to the minute: `2026年10月19日 02:31`.
pub fn datetime(unix: i64) -> String {
    format!("{} {}", date(unix), clock(unix))
}

/// The time of day in China, `HH:MM`, as the status bar shows it.
pub fn clock(unix: i64) -> String {
    let minutes = (unix + 8 * 3600).rem_euclid(86_400) / 60;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// A file's type and permissions as `ls -l` prints them: `drwxr-xr-x`.
pub fn mode(mode: u32) -> String {
    let kind = match mode & 0o170_000 {
        0o040_000 => 'd',
        0o120_000 => 'l',
        0o020_000 => 'c',
        0o060_000 => 'b',
        0o010_000 => 'p',
        0o140_000 => 's',
        _ => '-',
    };
    let mut out = String::from(kind);
    // The set-user, set-group and sticky bits show in each class's x.
    for (shift, special, set, unset) in [(6, 0o4000, 's', 'S'), (3, 0o2000, 's', 'S'), (0, 0o1000, 't', 'T')] {
        let bits = (mode >> shift) & 7;
        out.push(if bits & 4 != 0 { 'r' } else { '-' });
        out.push(if bits & 2 != 0 { 'w' } else { '-' });
        out.push(match (bits & 1 != 0, mode & special != 0) {
            (true, true) => set,
            (false, true) => unset,
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    out
}

/// What the file manager says under an entry's name.
/// The line under an entry's name in the file list, in the stock file
/// manager's words, with dates told against `now`, when the folder was read.
pub fn file_line(f: &FileRow, now: i64) -> String {
    const DIVIDER: &str = "  |  ";
    match &f.kind {
        FileKind::Folder { items: Some(n) } => format!("{}{DIVIDER}{n}项", file_time(f.modified, true, now)),
        FileKind::Folder { items: None } => "访问限制".to_string(),
        FileKind::File => format!("{}{DIVIDER}{}", file_time(f.modified, false, now), file_size(f.size)),
        FileKind::Link { target, .. } => format!("链接到 {target}"),
        FileKind::Special => {
            let what = match f.mode & 0o170_000 {
                0o020_000 => "字符设备",
                0o060_000 => "块设备",
                0o010_000 => "管道",
                _ => "套接字",
            };
            format!("{}{DIVIDER}{what}", file_time(f.modified, false, now))
        }
    }
}

/// The parts a backup holds: `启动分区、系统分区`.
pub fn parts(held: &[bool; 4]) -> String {
    BACKUP_PARTS.iter().zip(held).filter(|(_, &on)| on).map(|(name, _)| *name).collect::<Vec<_>>().join("、")
}

/// What an image is, and what it holds.
pub fn image_kind(kind: ImageKind) -> (&'static str, &'static str) {
    match kind {
        ImageKind::Boot { kernel: true, ramdisk: Ramdisk::Recovery } => ("恢复模式镜像", "内核和恢复模式 ramdisk"),
        ImageKind::Boot { kernel: false, ramdisk: Ramdisk::Recovery } => ("恢复模式镜像", "恢复模式 ramdisk"),
        ImageKind::Boot { kernel: true, ramdisk: Ramdisk::Generic } => ("启动镜像", "内核和通用 ramdisk"),
        ImageKind::Boot { kernel: true, ramdisk: Ramdisk::None } => ("启动镜像", "只有内核"),
        ImageKind::Boot { kernel: false, .. } => ("启动镜像", "只有通用 ramdisk"),
        ImageKind::VendorBoot => ("vendor_boot 镜像", "厂商 ramdisk 和设备树"),
        ImageKind::Dtbo => ("dtbo 镜像", "设备树叠加层"),
        ImageKind::Vbmeta => ("vbmeta 镜像", "启动验证数据"),
    }
}

/// What a partition an image can go to is for.
pub fn partition_role(name: &str) -> &'static str {
    match name {
        "boot" => "内核",
        "init_boot" => "通用 ramdisk",
        "recovery" => "恢复模式",
        "vendor_boot" => "厂商 ramdisk 和设备树",
        "dtbo" => "设备树叠加层",
        "vbmeta" => "启动验证数据",
        "vbmeta_system" => "系统分区的验证数据",
        "vbmeta_vendor" => "厂商分区的验证数据",
        _ => "分区",
    }
}

/// `槽位 B`, `槽位 A 和 B`
pub fn slots(slots: &[Slot]) -> String {
    let letters: Vec<&str> = slots.iter().map(|s| s.letter()).collect();
    format!("槽位 {}", letters.join(" 和 "))
}

/// How long the screen stays on with nothing touched: `30 秒`, `2 分钟`,
/// or `永不`.
pub fn screen_off(after_ms: Option<u64>) -> String {
    match after_ms {
        None => "永不".into(),
        Some(ms) if ms < 60_000 => format!("{} 秒", ms / 1000),
        Some(ms) => format!("{} 分钟", ms / 60_000),
    }
}

/// A mount or unmount that failed, with why: `挂载 vendor 失败：…`
pub fn mount_failure(m: &MountFailure) -> String {
    let what = if m.mounting { "挂载" } else { "卸载" };
    format!("{what} {} 失败：{}", m.base, m.detail)
}

/// Why a package was not installed, for a phone with this codename.
pub fn refusal(r: &Refusal, codename: &str) -> String {
    match r {
        Refusal::Unknown => "无法识别这个文件。能安装的是 A/B 系统更新包和分区镜像。".into(),
        Refusal::Script => "这是需要运行安装脚本的刷机包，这里只安装 A/B 系统更新包和分区镜像。".into(),
        Refusal::NotAb(kind) if kind.is_empty() => "这不是 A/B 系统更新包，包里没有注明类型。".into(),
        Refusal::NotAb(kind) => format!("这不是 A/B 系统更新包，包里注明的类型是 {kind}。"),
        Refusal::OtherDevice(devices) => format!("这个包适用于 {devices}，这台手机是 {codename}。"),
        Refusal::OtherSerial => "这个包只能安装在指定的几台手机上，不包括这台。".into(),
        Refusal::Incremental => "这是增量更新包，只能在系统里安装。请改用完整包。".into(),
        Refusal::Older { package, installed } => {
            format!("这个包的构建日期（{}）早于手机上的系统（{}）。", date(*package), date(*installed))
        }
        Refusal::OlderPatch { package, installed } => {
            format!("这个包的安全补丁（{package}）早于手机上的（{installed}），安装后会无法读取数据分区。")
        }
        Refusal::NoPartition => "这台手机没有能写入这个镜像的分区。".into(),
        Refusal::Battery { level, charging: true } => format!("电量只有 {level}%，充电时至少需要 15%。"),
        Refusal::Battery { level, charging: false } => format!("电量只有 {level}%，没有充电时至少需要 20%。"),
        Refusal::Pending(state) => format!("{}。请先回到系统完成上一次更新，再来安装。", snapshot(state)),
        Refusal::OldHost => "电脑上的 adb 版本过旧，无法分块发送。请更新 Android SDK Platform-Tools。".into(),
    }
}

/// What went wrong, and the details behind it when there are any.
pub fn failure(f: &Failure) -> (String, Option<String>) {
    match f {
        Failure::Engine(code) => {
            let what = match Trouble::of(*code) {
                Trouble::Signature => "安装包的签名不匹配，这台手机只接受官方签名的包。",
                Trouble::Damaged => "安装包已经损坏，请重新下载。",
                Trouble::Older => "安装包早于恢复模式允许的版本。",
                Trouble::NoSpace => "super 分区的空间不够存放新系统。",
                Trouble::Transfer => "读取安装包失败，数据线可能断开了。",
                Trouble::Write => "写入分区失败。",
                Trouble::Verify => "写入的分区没有通过校验。",
                Trouble::PostInstall => "安装包最后的步骤失败了。",
                Trouble::Unsupported => "这个包的格式比手机上的更新程序新，无法识别。",
                Trouble::Other => "更新程序报错。",
            };
            (what.into(), Some(format!("update_engine 错误代码 {code}")))
        }
        Failure::Disconnected => ("电脑停止了发送：数据线断开了，或者 adb sideload 被中断。".into(), None),
        Failure::Changed => ("电脑上的文件在发送途中被改动了，请重新发送。".into(), None),
        Failure::Error(detail) => ("出现了意外的错误。".into(), Some(detail.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Xiaomi 13 Ultra (ishtar, 16 GB + 512 GB): readings from `skyward info`
    // and the strings its stock About pages print for them.
    // Xiaomi 17 Ultra (nezha, 16 GB + 1 TB): MemTotal from its boot log and
    // the userdata size from `fastboot getvar all`.
    #[test]
    fn ram_rounds_to_module_size() {
        assert_eq!(ram(15_556_972), "16.0 GB");
        assert_eq!(ram(15_472_464), "16.0 GB");
    }

    #[test]
    fn storage_matches_the_stock_settings() {
        let ishtar = Storage { total: 496_550_785_024, free: Some(61_300_000_000) };
        assert_eq!(capacity(ishtar.total), "512GB");
        assert_eq!(storage_used(&ishtar).unwrap(), "450.7GB/512GB");
        assert_eq!(capacity(0xF2_091F_B000), "1TB");
        assert_eq!(storage_used(&Storage { total: 1, free: None }), None);
    }

    #[test]
    fn cpu_summary() {
        assert_eq!(cpu(&Cpu { cores: 8, max_khz: Some(3_187_200) }), "八核 最高 3.19GHz");
        assert_eq!(cpu(&Cpu { cores: 8, max_khz: Some(4_608_000) }), "八核 最高 4.61GHz");
        assert_eq!(cpu(&Cpu { cores: 2, max_khz: None }), "双核");
        assert_eq!(numeral(12), "十二");
        assert_eq!(numeral(20), "二十");
    }

    // Xiaomi 13 Ultra battery as `skyward info` read it.
    #[test]
    fn battery_lines() {
        let b = Battery {
            level: 100,
            charge: Charge::Full,
            temp: Some(338),
            health: Some("Good".into()),
            cycles: Some(169),
            design: Some(5_500_000),
            full: Some(5_095_000),
            voltage: Some(4_482_000),
        };
        assert_eq!(level(&b), "100% · 已充满");
        assert_eq!(celsius(b.temp.unwrap()), "33.8°C");
        assert_eq!(wear(&b).unwrap(), "良好 · 剩余容量 93%");
        assert_eq!(mah(b.design.unwrap()), "5500mAh");
        assert_eq!(volts(b.voltage.unwrap()), "4.48V");
    }

    // Xiaomi 17 Ultra build and panel, as its stock settings print them.
    #[test]
    fn versions_and_states() {
        let patch = PatchLevel { year: 2026, month: 10, day: 1 };
        assert_eq!(android("17", Some(&patch)), "17 | Android 安全更新：2026年10月1日");
        assert_eq!(panel((1200, 2608)), "2608 x 1200");
        assert_eq!(model_lines("Xiaomi 17 Ultra by Leica"), "Xiaomi 17 Ultra\nby Leica");
        assert_eq!(model_lines("Xiaomi 17 Ultra"), "Xiaomi 17 Ultra");
        assert_eq!(inches(6.9), "6.9″");
        assert_eq!(verified("orange"), "橙色 · 设备已解锁，不校验系统");
        assert_eq!(health("Watchdog timer expire"), "Watchdog timer expire");
    }

    #[test]
    fn sizes_and_dates() {
        assert_eq!(bytes(7_103_041_280), "7.1GB");
        assert_eq!(bytes(100_663_296), "100.7MB");
        assert_eq!(bytes(131_072), "131KB");
        assert_eq!(bytes(64), "64B");
        // The Xiaomi 13 Ultra's installed system, by its ro.build.date.utc.
        assert_eq!(date(1_778_243_655), "2026年5月8日");
        assert_eq!(date(0), "1970年1月1日");
        // Midnight on a leap day in UTC is eight in the morning in China.
        assert_eq!(date(1_709_164_800), "2024年2月29日");
        assert_eq!(date(1_709_164_800 - 8 * 3600 - 1), "2024年2月28日");
        assert_eq!(patch(&PatchLevel { year: 2026, month: 4, day: 1 }), "2026年4月1日");
        assert_eq!(slots(&[Slot::B]), "槽位 B");
        assert_eq!(slots(&[Slot::A, Slot::B]), "槽位 A 和 B");
    }

    #[test]
    fn screen_off_times() {
        let labels: Vec<String> = crate::app::SCREEN_OFF.iter().map(|&ms| screen_off(ms)).collect();
        assert_eq!(labels, ["15 秒", "30 秒", "1 分钟", "2 分钟", "5 分钟", "10 分钟", "永不"]);
    }

    #[test]
    fn times_to_the_minute_in_china() {
        // 2026-10-18 18:31 UTC is 02:31 the next morning in Beijing.
        assert_eq!(datetime(1_792_348_260), "2026年10月19日 02:31");
        assert_eq!(clock(1_792_348_260), "02:31");
        // Midnight in Beijing is 16:00 UTC the day before.
        assert_eq!(clock(1_792_339_200), "00:00");
    }

    #[test]
    fn modes_read_as_ls_prints_them() {
        assert_eq!(mode(0o040_755), "drwxr-xr-x");
        assert_eq!(mode(0o100_644), "-rw-r--r--");
        assert_eq!(mode(0o120_777), "lrwxrwxrwx");
        assert_eq!(mode(0o041_777), "drwxrwxrwt", "the sticky /tmp");
        assert_eq!(mode(0o104_755), "-rwsr-xr-x", "set-user on an executable");
        assert_eq!(mode(0o020_600), "crw-------");
    }

    #[test]
    fn entries_say_what_they_are() {
        // Changed at 2026-10-19 02:31 in Beijing; listed a week later.
        let row = |kind, size, mode| FileRow { name: "x".into(), kind, size, modified: 1_792_348_260, mode, uid: 0, gid: 0 };
        let later = 1_792_348_260 + 7 * 86_400;
        assert_eq!(file_line(&row(FileKind::Folder { items: Some(6) }, 4096, 0o040_755), later), "2026/10/19  |  6项");
        assert_eq!(file_line(&row(FileKind::Folder { items: None }, 4096, 0o040_700), later), "访问限制");
        assert_eq!(file_line(&row(FileKind::File, 26_950, 0o100_644), later), "10/19  |  26.95 KB");
        let link = FileKind::Link { target: "/storage/self/primary".into(), folder: true };
        assert_eq!(file_line(&row(link, 21, 0o120_777), later), "链接到 /storage/self/primary");
        assert_eq!(file_line(&row(FileKind::Special, 0, 0o060_600), later), "10/19  |  块设备");
    }

    #[test]
    fn list_dates_are_told_against_the_day_the_folder_was_read() {
        // 2026-10-19 02:31 in Beijing.
        let at = 1_792_348_260;
        assert_eq!(file_time(at, false, at + 3600), "02:31", "today: the time");
        assert_eq!(file_time(at, true, at + 3600), "02:31", "a folder today too");
        assert_eq!(file_time(at, false, at + 86_400), "10/19", "this year: a file's month and day");
        assert_eq!(file_time(at, true, at + 86_400), "2026/10/19", "this year: a folder's whole date");
        assert_eq!(file_time(at, false, at + 90 * 86_400), "2026/10/19", "another year: the whole date");
        // 2026-10-18 23:59 UTC is already the 19th in Beijing.
        assert_eq!(file_time(1_792_367_940, false, at), "07:59");
    }

    #[test]
    fn sizes_go_up_a_unit_past_900_with_two_decimals_at_most() {
        assert_eq!(file_size(0), "0 B");
        assert_eq!(file_size(900), "900 B");
        assert_eq!(file_size(901), "0.9 KB");
        assert_eq!(file_size(42_633), "42.63 KB");
        assert_eq!(file_size(26_950), "26.95 KB");
        assert_eq!(file_size(1_090_000), "1.09 MB");
        assert_eq!(file_size(634_960), "634.96 KB");
        assert_eq!(file_size(2_000_000_000), "2 GB");
        assert_eq!(file_size(1_500_000), "1.5 MB");
    }

    #[test]
    fn a_backup_lists_its_parts() {
        assert_eq!(parts(&[true, true, false, true]), "启动分区、系统分区、基带与 EFS");
    }

    #[test]
    fn mount_failures_name_the_partition_and_why() {
        let unmounting = MountFailure {
            mounting: false,
            base: "vendor".into(),
            detail: "unmount /vendor: Device or resource busy (os error 16)".into(),
        };
        assert_eq!(mount_failure(&unmounting), "卸载 vendor 失败：unmount /vendor: Device or resource busy (os error 16)");
        let mounting = MountFailure {
            mounting: true,
            base: "odm".into(),
            detail: "mount odm_b: /dev/block/dm-8 holds none of erofs, ext4, f2fs".into(),
        };
        assert_eq!(mount_failure(&mounting), "挂载 odm 失败：mount odm_b: /dev/block/dm-8 holds none of erofs, ext4, f2fs");
    }

    #[test]
    fn reasons_read_as_sentences() {
        assert_eq!(refusal(&Refusal::OtherDevice("nezha".into()), "ishtar"), "这个包适用于 nezha，这台手机是 ishtar。");
        assert_eq!(
            refusal(&Refusal::Older { package: 1_770_000_000, installed: 1_778_243_655 }, "ishtar"),
            "这个包的构建日期（2026年2月2日）早于手机上的系统（2026年5月8日）。"
        );
        assert_eq!(refusal(&Refusal::Battery { level: 12, charging: true }, "ishtar"), "电量只有 12%，充电时至少需要 15%。");
        assert_eq!(
            failure(&Failure::Engine(26)),
            ("安装包的签名不匹配，这台手机只接受官方签名的包。".into(), Some("update_engine 错误代码 26".into()))
        );
        assert_eq!(failure(&Failure::Error("open ep0: busy".into())).1.as_deref(), Some("open ep0: busy"));
    }

    #[test]
    fn erasing_that_stops_says_what_was_left_and_why() {
        assert_eq!(erase_stop(&EraseStop::Failed(Part::Art, "x".into())).0, "清除 ART 缓存失败。", "a Latin name stands apart");
        assert_eq!(erase_stop(&EraseStop::Failed(Part::Data, "x".into())).0, "清除 data 分区失败。");
    }
}
