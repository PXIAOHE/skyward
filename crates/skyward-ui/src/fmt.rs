use crate::app::{BackupRefusal, Data, EraseStop, FileKind, FileRow, MountFailure, Part, Place, Source, BACKUP_PARTS, TIME_ZONES};
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::Timestamp;
use skyward_info::{Battery, Charge, Cpu, PatchLevel, Storage};
use skyward_install::ota::Trouble;
use skyward_install::{Failure, ImageKind, Ramdisk, Refusal};
use skyward_vab::{Hold, Slot, SnapshotState, Standing};
use std::sync::OnceLock;

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
        "green" => "绿色 · 已通过官方密钥验证",
        "yellow" => "黄色 · 已通过自定义密钥验证",
        "orange" => "橙色 · 设备已解锁，不验证系统",
        "red" => "红色 · 验证失败",
        other => return other.to_string(),
    }
    .to_string()
}

pub fn snapshot(s: &SnapshotState) -> &'static str {
    match s {
        SnapshotState::None => "暂无待合并的更新",
        SnapshotState::Snapshotted => "更新已写入，等待合并",
        SnapshotState::Merging => "正在合并更新",
        SnapshotState::MergeFailed => "更新合并失败",
        SnapshotState::Unknown => "无法读取更新状态",
    }
}

/// Why super is off limits while an update is in flight; `None` when it is not.
pub fn snapshot_note(s: &SnapshotState) -> Option<&'static str> {
    match s {
        SnapshotState::None => None,
        SnapshotState::Snapshotted | SnapshotState::Merging => Some("更新合并完成前，暂不刷写 super 中的分区"),
        SnapshotState::MergeFailed => Some("在系统中处理完成前，暂不刷写 super 中的分区"),
        SnapshotState::Unknown => Some("读取到更新状态前，暂不刷写 super 中的分区"),
    }
}

/// Why the slot to start next stays as an update in flight set it; `None`
/// when none is.
pub fn snapshot_slot_note(s: &SnapshotState) -> Option<&'static str> {
    match s {
        SnapshotState::None => None,
        SnapshotState::Snapshotted | SnapshotState::Merging => Some("更新合并完成前，暂不切换启动槽位。"),
        SnapshotState::MergeFailed => Some("在系统中处理完成前，暂不切换启动槽位。"),
        SnapshotState::Unknown => Some("读取到更新状态前，暂不切换启动槽位。"),
    }
}

/// What an install script that writes to super does to an update in
/// flight; `None` when none is known to be.
pub fn snapshot_script_note(s: &SnapshotState) -> Option<&'static str> {
    match s {
        SnapshotState::None | SnapshotState::Unknown => None,
        SnapshotState::Snapshotted | SnapshotState::Merging => Some("系统更新尚未完成合并。刷机包改写 super 中的分区，会导致这次更新无法完成。"),
        SnapshotState::MergeFailed => Some("系统更新合并失败，需在系统中处理。刷机包改写 super 中的分区，会导致这次更新无法完成。"),
    }
}

/// Why the slot's partitions in super are not backed up while an update is
/// in flight; `None` when none is.
pub fn snapshot_backup_note(s: &SnapshotState) -> Option<&'static str> {
    match s {
        SnapshotState::None => None,
        SnapshotState::Snapshotted | SnapshotState::Merging => Some("更新合并完成前，暂不备份系统分区"),
        SnapshotState::MergeFailed => Some("在系统中处理完成前，暂不备份系统分区"),
        SnapshotState::Unknown => Some("读取到更新状态前，暂不备份系统分区"),
    }
}

/// Why backing up, or restoring, did not start.
pub fn backup_refusal(r: &BackupRefusal, restore: bool) -> String {
    match r {
        BackupRefusal::Room { needed, free } => format!("存储空间不足。备份需要 {}，可用空间为 {}。", gb(*needed), gb(*free)),
        BackupRefusal::Update(state) => {
            let what = if restore { "恢复启动分区和系统分区" } else { "备份系统分区" };
            match state {
                SnapshotState::Snapshotted | SnapshotState::Merging => format!("更新合并完成前，暂不{what}。你可以在「高级 > 系统更新」中查看。"),
                SnapshotState::MergeFailed => format!("更新合并失败。在系统中处理完成前，暂不{what}。"),
                SnapshotState::Unknown => format!("无法读取更新状态，暂不{what}。"),
                SnapshotState::None => unreachable!("no update in flight holds anything back"),
            }
        }
        BackupRefusal::OtherPhone(device) => format!("该备份来自其他设备（{device}），无法恢复到本机。"),
        BackupRefusal::Resized(name) => format!("{name} 的大小与备份时不同，系统可能已更新。该备份中的系统分区无法恢复到当前系统。"),
        BackupRefusal::DataReset => "备份后 data 分区已重新创建，例如恢复过出厂设置，该备份中的 data 分区无法恢复。".into(),
    }
}

/// What the update page says of an update standing so, and the line under
/// it on what comes next; `next` is the slot the bootloader starts next,
/// where the boot control could tell.
pub fn update(s: &Standing, next: Option<Slot>) -> (&'static str, Option<String>) {
    match s {
        Standing::None => ("暂无待合并的更新", None),
        Standing::Unknown => ("无法读取更新状态", Some("无法读取 /metadata 中的更新记录".into())),
        Standing::Writing => ("更新未写入完成", Some("启动系统后将继续安装".into())),
        Standing::Installed(slot) => {
            let detail = match next {
                Some(next) if next == *slot => format!("重启后将从槽位 {} 启动新系统，启动后自动合并", slot.letter()),
                Some(_) => format!("需在系统的「系统更新」中重启，才会从槽位 {} 启动新系统", slot.letter()),
                None => format!("从槽位 {} 启动新系统后将自动合并", slot.letter()),
            };
            ("更新已安装", Some(detail))
        }
        Standing::Waiting => ("更新等待合并", Some("启动新系统后将自动合并，也可以在此合并".into())),
        Standing::Compressed => ("更新等待合并", Some("此类更新需在系统中合并，启动系统后将自动合并".into())),
        Standing::Merging(_) => ("正在合并更新", None),
        Standing::Failed(code) => ("更新合并失败", Some(format!("重启后将继续合并（{code:?}）"))),
        Standing::Merged => ("更新已合并", Some("下次启动系统时将清理更新留下的文件".into())),
    }
}

/// Where an update standing so is, beside the entry of its page.
pub fn update_value(s: &Standing) -> &'static str {
    match s {
        Standing::None => "暂无",
        Standing::Unknown => "无法读取",
        Standing::Writing => "未写入完成",
        Standing::Installed(_) => "已安装",
        Standing::Waiting | Standing::Compressed => "等待合并",
        Standing::Merging(_) => "正在合并",
        Standing::Failed(_) => "合并失败",
        Standing::Merged => "已合并",
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
        Part::AppData => "应用数据",
        Part::Media => "内部存储",
        Part::AppCaches => "应用缓存",
        Part::SystemCaches => "系统缓存",
        Part::Art => "ART 缓存",
        Part::Logs => "日志",
        Part::Updates => "下载的更新包",
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

/// Users by how the pages call them: the owner, then the others by number,
/// as「机主、用户 999」.
pub fn users(users: &[u32]) -> String {
    users.iter().map(|&u| if u == 0 { "机主".to_string() } else { format!("用户 {u}") }).collect::<Vec<_>>().join("、")
}

/// Why erasing did not start or stopped short, and what the system said of
/// it when there is that.
pub fn erase_stop(s: &EraseStop) -> (String, Option<String>) {
    match s {
        EraseStop::Update(Hold::Unconfirmed) => {
            ("当前槽位的系统更新尚未完成，暂无法清除。你可以在「高级 > 系统更新」中合并后重试。".into(), None)
        }
        EraseStop::Update(Hold::Merging) => ("系统更新正在合并，暂无法清除。你可以在「高级 > 系统更新」中查看，合并完成后重试。".into(), None),
        EraseStop::Update(Hold::MergeFailed) => ("系统更新合并失败，暂无法清除。请先启动系统处理后重试。".into(), None),
        EraseStop::NotRebuilt => ("本机系统开机时不会重新创建已清空的 data 分区，无法清除。".into(), None),
        EraseStop::NoCache => ("本机没有 cache 分区。".into(), None),
        EraseStop::Erased => ("data 分区已清空，下次开机时将重新创建，暂无可清除的内容。".into(), None),
        EraseStop::Closed(why) => ("无法打开 data 分区，未清除任何内容。".into(), Some(why.clone())),
        EraseStop::Locked(locked) => {
            (spaced("", &users(locked), "的数据尚未解密，未清除任何内容。请先在「解密」中输入锁屏密码后重试。"), None)
        }
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

/// The rules of each of [`TIME_ZONES`], read once from the ones the
/// recovery carries.
pub fn rules(zone: usize) -> &'static TimeZone {
    static RULES: OnceLock<Vec<TimeZone>> = OnceLock::new();
    let all = RULES.get_or_init(|| {
        TIME_ZONES.iter().map(|&(_, _, name)| TimeZone::get(name).expect("the recovery carries the rules of every zone listed")).collect()
    });
    &all[zone]
}

/// A Unix time on the calendar and clock of the zone at `zone` in
/// [`TIME_ZONES`]. A time past either end of the calendar, as a damaged
/// file's can be, is taken as that end.
fn local(unix: i64, zone: usize) -> DateTime {
    let unix = unix.clamp(Timestamp::MIN.as_second(), Timestamp::MAX.as_second());
    let at = Timestamp::from_second(unix).expect("a time within the calendar");
    at.to_zoned(rules(zone).clone()).datetime()
}

/// The day a Unix time falls on in the zone at `zone` in [`TIME_ZONES`],
/// `2026年5月8日`.
pub fn date(unix: i64, zone: usize) -> String {
    let d = local(unix, zone);
    format!("{}年{}月{}日", d.year(), d.month(), d.day())
}

/// When a list entry last changed, as the stock file manager tells it
/// against `now`, when the folder was read: the time for today, month and
/// day for a file from this year, and the whole date for a folder from this
/// year or anything older; all in the zone at `zone` in [`TIME_ZONES`].
pub fn file_time(unix: i64, folder: bool, now: i64, zone: usize) -> String {
    let (day, today) = (local(unix, zone).date(), local(now, zone).date());
    if day == today {
        clock(unix, zone)
    } else if day.year() == today.year() && !folder {
        format!("{:02}/{:02}", day.month(), day.day())
    } else {
        format!("{}/{:02}/{:02}", day.year(), day.month(), day.day())
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

/// A Unix time in the zone at `zone` in [`TIME_ZONES`], to the minute:
/// `2026年10月19日 02:31`.
pub fn datetime(unix: i64, zone: usize) -> String {
    format!("{} {}", date(unix, zone), clock(unix, zone))
}

/// The time of day in the zone at `zone` in [`TIME_ZONES`], `HH:MM`, as
/// the status bar shows it.
pub fn clock(unix: i64, zone: usize) -> String {
    let t = local(unix, zone);
    format!("{:02}:{:02}", t.hour(), t.minute())
}

/// A Unix time in the zone at `zone` in [`TIME_ZONES`], to the second, as
/// the name of what is saved then, which sorts by it: `20261021-133000`.
pub fn stamp(unix: i64, zone: usize) -> String {
    let t = local(unix, zone);
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.year(), t.month(), t.day(), t.hour(), t.minute(), t.second())
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

/// The line under an entry's name in the file list, laid out as the stock
/// file manager lays it out, with dates told against `now`, when the folder
/// was read, in the zone at `zone` in [`TIME_ZONES`].
pub fn file_line(f: &FileRow, now: i64, zone: usize) -> String {
    const DIVIDER: &str = "  |  ";
    match &f.kind {
        FileKind::Folder { items: Some(n) } => format!("{}{DIVIDER}{n} 项", file_time(f.modified, true, now, zone)),
        FileKind::Folder { items: None } => "无法读取".to_string(),
        FileKind::File => format!("{}{DIVIDER}{}", file_time(f.modified, false, now, zone), file_size(f.size)),
        FileKind::Link { target, .. } => format!("链接到 {target}"),
        FileKind::Special => {
            let what = match f.mode & 0o170_000 {
                0o020_000 => "字符设备",
                0o060_000 => "块设备",
                0o010_000 => "管道",
                _ => "套接字",
            };
            format!("{}{DIVIDER}{what}", file_time(f.modified, false, now, zone))
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
        ImageKind::Boot { kernel: true, ramdisk: Ramdisk::Recovery } => ("恢复模式镜像", "包含内核和恢复模式 ramdisk"),
        ImageKind::Boot { kernel: false, ramdisk: Ramdisk::Recovery } => ("恢复模式镜像", "仅包含恢复模式 ramdisk"),
        ImageKind::Boot { kernel: true, ramdisk: Ramdisk::Generic } => ("启动镜像", "包含内核和通用 ramdisk"),
        ImageKind::Boot { kernel: true, ramdisk: Ramdisk::None } => ("启动镜像", "仅包含内核"),
        ImageKind::Boot { kernel: false, .. } => ("启动镜像", "仅包含通用 ramdisk"),
        ImageKind::VendorBoot => ("vendor_boot 镜像", "包含厂商 ramdisk 和设备树"),
        ImageKind::Dtbo => ("dtbo 镜像", "包含设备树叠加层"),
        ImageKind::Vbmeta => ("vbmeta 镜像", "包含启动验证数据"),
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

/// How another package is given to the installer from where this one came
/// from: the computer sends it, or another file is chosen.
fn give(source: Source) -> &'static str {
    match source {
        Source::Computer => "发送",
        Source::File(_) => "选择",
    }
}

/// Why a package from `source` was not installed, for a phone with this
/// codename, its dates in the zone at `zone` in [`TIME_ZONES`].
pub fn refusal(r: &Refusal, codename: &str, source: Source, zone: usize) -> String {
    let give = give(source);
    match r {
        Refusal::Unknown => format!("无法识别该文件，请{give} A/B 完整更新包、刷机包或分区镜像。"),
        Refusal::NotAb(kind) if kind.is_empty() => format!("该更新包未注明类型，请{give} A/B 完整更新包。"),
        Refusal::NotAb(kind) => format!("该更新包的类型为 {kind}，请{give} A/B 完整更新包。"),
        Refusal::OtherDevice(devices) => format!("该更新包适用于 {devices}，本机为 {codename}。"),
        Refusal::OtherSerial => "该更新包仅适用于指定序列号的设备，不包括本机。".into(),
        Refusal::Incremental => format!("无法安装增量更新包，请{give}完整更新包。"),
        Refusal::Older { package, installed } => {
            format!("该更新包的构建日期为 {}，早于当前系统的 {}。", date(*package, zone), date(*installed, zone))
        }
        Refusal::OlderPatch { package, installed } => {
            format!("该更新包的安全补丁为 {package}，早于当前系统的 {installed}。")
        }
        Refusal::NoPartition => "本机没有可刷入该镜像的分区。".into(),
        Refusal::Battery { level, charging: true } => format!("电量仅剩 {level}%，请充电至 15% 以上后重试。"),
        Refusal::Battery { level, charging: false } => format!("电量仅剩 {level}%，请充电至 20% 以上后重试。"),
        Refusal::Pending(state) => format!("{}。请重启到系统，完成上一次更新后重试。", snapshot(state)),
        Refusal::OldHost => "电脑上的 adb 版本过低，请更新 Android SDK Platform-Tools 后重试。".into(),
    }
}

/// What went wrong installing a package from `source`, and the details
/// behind it when there are any.
pub fn failure(f: &Failure, source: Source) -> (String, Option<String>) {
    match f {
        Failure::Engine(code) => {
            let what = match (Trouble::of(*code), source) {
                (Trouble::Signature, _) => format!("更新包签名验证失败，请{}官方更新包。", give(source)),
                (Trouble::Damaged, _) => "更新包已损坏，请重新下载。".into(),
                (Trouble::Older, _) => "更新包的版本过旧。".into(),
                (Trouble::NoSpace, _) => "super 分区空间不足。".into(),
                (Trouble::Transfer, Source::Computer) => "读取更新包失败，请检查数据线连接后重新发送。".into(),
                (Trouble::Transfer, Source::File(Place::Storage)) => "读取更新包失败。".into(),
                (Trouble::Transfer, Source::File(Place::UsbDrive)) => "读取更新包失败，请检查 U 盘连接后重试。".into(),
                (Trouble::Write, _) => "写入分区失败。".into(),
                (Trouble::Verify, _) => "分区校验失败。".into(),
                (Trouble::PostInstall, _) => "完成安装失败。".into(),
                (Trouble::Unsupported, _) => "暂不支持该更新包的格式。".into(),
                (Trouble::Other, _) => "更新程序出错。".into(),
            };
            (what, Some(format!("错误代码：{code}")))
        }
        Failure::Script(code) => (format!("安装脚本运行失败，退出码为 {code}。"), None),
        Failure::Disconnected => ("电脑已停止发送，请检查数据线连接后重新发送。".into(), None),
        Failure::Unreadable(why) => match source {
            Source::File(Place::UsbDrive) => ("读取文件失败，请检查 U 盘连接后重试。".into(), Some(why.clone())),
            _ => ("读取文件失败。".into(), Some(why.clone())),
        },
        Failure::Changed => match source {
            Source::Computer => ("文件在发送过程中被修改，请重新发送。".into(), None),
            Source::File(_) => ("文件在读取过程中被修改，请重新选择。".into(), None),
        },
        Failure::Error(detail) => ("发生意外错误。".into(), Some(detail.clone())),
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
        assert_eq!(verified("orange"), "橙色 · 设备已解锁，不验证系统");
        assert_eq!(health("Watchdog timer expire"), "Watchdog timer expire");
    }

    #[test]
    fn sizes_and_dates() {
        assert_eq!(bytes(7_103_041_280), "7.1GB");
        assert_eq!(bytes(100_663_296), "100.7MB");
        assert_eq!(bytes(131_072), "131KB");
        assert_eq!(bytes(64), "64B");
        // The Xiaomi 13 Ultra's installed system, by its ro.build.date.utc.
        assert_eq!(date(1_778_243_655, 0), "2026年5月8日");
        assert_eq!(date(0, 0), "1970年1月1日");
        // Midnight on a leap day in UTC is eight in the morning in China.
        assert_eq!(date(1_709_164_800, 0), "2024年2月29日");
        assert_eq!(date(1_709_164_800 - 8 * 3600 - 1, 0), "2024年2月28日");
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
        assert_eq!(datetime(1_792_348_260, 0), "2026年10月19日 02:31");
        assert_eq!(clock(1_792_348_260, 0), "02:31");
        // Midnight in Beijing is 16:00 UTC the day before.
        assert_eq!(clock(1_792_339_200, 0), "00:00");
        assert_eq!(stamp(1_792_348_265, 0), "20261019-023105");
    }

    /// Where a zone named in the tz database is in [`TIME_ZONES`].
    fn zone(name: &str) -> usize {
        TIME_ZONES.iter().position(|&(_, _, n)| n == name).expect("a zone listed")
    }

    #[test]
    fn zones_keep_their_own_summer_time() {
        // 2026-07-01 12:00 UTC, and 2026-01-15 12:00 UTC.
        let (summer, winter) = (1_782_907_200, 1_768_478_400);
        assert_eq!(clock(summer, zone("UTC")), "12:00");
        assert_eq!(clock(summer, zone("Europe/London")), "13:00", "British Summer Time");
        assert_eq!(clock(winter, zone("Europe/London")), "12:00");
        assert_eq!(clock(summer, zone("Europe/Berlin")), "14:00");
        assert_eq!(clock(winter, zone("Europe/Berlin")), "13:00");
        assert_eq!(clock(summer, zone("America/New_York")), "08:00");
        assert_eq!(clock(winter, zone("America/New_York")), "07:00");
        assert_eq!(datetime(summer, zone("America/Los_Angeles")), "2026年7月1日 05:00");
        assert_eq!(clock(summer, zone("Asia/Tokyo")), "21:00");
        assert_eq!(clock(winter, zone("Asia/Shanghai")), "20:00", "China keeps no summer time");
        // New York goes over to summer time at 2:00 on 2026-03-08.
        let before = 1_772_953_200 - 60;
        assert_eq!((clock(before, zone("America/New_York")), clock(before + 60, zone("America/New_York"))), ("01:59".into(), "03:00".into()));
    }

    #[test]
    fn a_time_past_the_calendar_shows_as_its_end() {
        assert_eq!(date(i64::MAX, 0), "9999年12月31日");
        assert_eq!(date(i64::MIN, zone("UTC")), "-9999年1月2日");
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
        assert_eq!(file_line(&row(FileKind::Folder { items: Some(6) }, 4096, 0o040_755), later, 0), "2026/10/19  |  6 项");
        assert_eq!(file_line(&row(FileKind::Folder { items: None }, 4096, 0o040_700), later, 0), "无法读取");
        assert_eq!(file_line(&row(FileKind::File, 26_950, 0o100_644), later, 0), "10/19  |  26.95 KB");
        let link = FileKind::Link { target: "/storage/self/primary".into(), folder: true };
        assert_eq!(file_line(&row(link, 21, 0o120_777), later, 0), "链接到 /storage/self/primary");
        assert_eq!(file_line(&row(FileKind::Special, 0, 0o060_600), later, 0), "10/19  |  块设备");
    }

    #[test]
    fn list_dates_are_told_against_the_day_the_folder_was_read() {
        // 2026-10-19 02:31 in Beijing.
        let at = 1_792_348_260;
        assert_eq!(file_time(at, false, at + 3600, 0), "02:31", "today: the time");
        assert_eq!(file_time(at, true, at + 3600, 0), "02:31", "a folder today too");
        assert_eq!(file_time(at, false, at + 86_400, 0), "10/19", "this year: a file's month and day");
        assert_eq!(file_time(at, true, at + 86_400, 0), "2026/10/19", "this year: a folder's whole date");
        assert_eq!(file_time(at, false, at + 90 * 86_400, 0), "2026/10/19", "another year: the whole date");
        // 2026-10-18 23:59 UTC is already the 19th in Beijing.
        assert_eq!(file_time(1_792_367_940, false, at, 0), "07:59");
        // In London the file changed on the evening of the 18th, the day
        // before.
        assert_eq!(file_time(1_792_348_260, false, 1_792_367_940, zone("Europe/London")), "10/18");
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
        let pc = Source::Computer;
        assert_eq!(refusal(&Refusal::OtherDevice("nezha".into()), "ishtar", pc, 0), "该更新包适用于 nezha，本机为 ishtar。");
        assert_eq!(
            refusal(&Refusal::Older { package: 1_770_000_000, installed: 1_778_243_655 }, "ishtar", pc, 0),
            "该更新包的构建日期为 2026年2月2日，早于当前系统的 2026年5月8日。"
        );
        assert_eq!(refusal(&Refusal::Battery { level: 12, charging: true }, "ishtar", pc, 0), "电量仅剩 12%，请充电至 15% 以上后重试。");
        assert_eq!(
            refusal(&Refusal::Pending(SnapshotState::Snapshotted), "ishtar", pc, 0),
            "更新已写入，等待合并。请重启到系统，完成上一次更新后重试。"
        );
        assert_eq!(failure(&Failure::Engine(26), pc), ("更新包签名验证失败，请发送官方更新包。".into(), Some("错误代码：26".into())));
        assert_eq!(failure(&Failure::Error("open ep0: busy".into()), pc).1.as_deref(), Some("open ep0: busy"));
        assert_eq!(failure(&Failure::Script(1), pc), ("安装脚本运行失败，退出码为 1。".into(), None));
    }

    #[test]
    fn a_file_chosen_is_answered_with_choosing_another() {
        let file = Source::File(Place::Storage);
        assert_eq!(refusal(&Refusal::Unknown, "ishtar", file, 0), "无法识别该文件，请选择 A/B 完整更新包、刷机包或分区镜像。");
        assert_eq!(refusal(&Refusal::Incremental, "ishtar", file, 0), "无法安装增量更新包，请选择完整更新包。");
        assert_eq!(failure(&Failure::Engine(26), file).0, "更新包签名验证失败，请选择官方更新包。");
        assert_eq!(failure(&Failure::Changed, file).0, "文件在读取过程中被修改，请重新选择。");
        let gone = "read /sdcard/Download/ota.zip: Input/output error (os error 5)";
        assert_eq!(failure(&Failure::Unreadable(gone.into()), file), ("读取文件失败。".into(), Some(gone.into())));
        let pulled = failure(&Failure::Unreadable(gone.into()), Source::File(Place::UsbDrive));
        assert_eq!(pulled.0, "读取文件失败，请检查 U 盘连接后重试。", "a drive that came out is checked first");
    }

    #[test]
    fn erasing_that_stops_says_what_was_left_and_why() {
        assert_eq!(users(&[0]), "机主");
        assert_eq!(users(&[0, 999]), "机主、用户 999");
        assert!(erase_stop(&EraseStop::Locked(vec![0, 999])).0.starts_with("机主、用户 999 的数据尚未解密，"));
        assert!(erase_stop(&EraseStop::Locked(vec![0])).0.starts_with("机主的数据尚未解密，"));
        assert_eq!(erase_stop(&EraseStop::Failed(Part::Art, "x".into())).0, "清除 ART 缓存失败。", "a Latin name stands apart");
        assert_eq!(erase_stop(&EraseStop::Failed(Part::Data, "x".into())).0, "清除 data 分区失败。");
        assert_eq!(erase_stop(&EraseStop::Failed(Part::Logs, "x".into())), ("清除日志失败。".into(), Some("x".into())));
    }

    #[test]
    fn an_installed_update_says_whether_its_slot_starts_next() {
        let installed = Standing::Installed(Slot::A);
        assert_eq!(update(&installed, Some(Slot::A)).1.unwrap(), "重启后将从槽位 A 启动新系统，启动后自动合并");
        assert_eq!(
            update(&installed, Some(Slot::B)).1.unwrap(),
            "需在系统的「系统更新」中重启，才会从槽位 A 启动新系统",
            "the system's updater has not had slot A start next yet"
        );
        assert_eq!(update(&installed, None).1.unwrap(), "从槽位 A 启动新系统后将自动合并", "the boot control could not tell");
    }
}
