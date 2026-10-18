use skyward_info::{Battery, Charge, Cpu, DataState, PatchLevel, Storage};
use skyward_install::ota::Trouble;
use skyward_install::{Failure, ImageKind, Ramdisk, Refusal};
use skyward_vab::{Slot, SnapshotState};

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

pub fn data(d: DataState) -> &'static str {
    match d {
        DataState::Unmounted => "未挂载",
        DataState::Locked => "未解密",
        DataState::Decrypted => "已解密",
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
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}年{month}月{day}日")
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
        Refusal::Wipe => "这个包要求安装后清除全部数据，清除功能还没实现。".into(),
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
}
