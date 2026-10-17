use skyward_info::{Battery, Charge, Cpu, DataState, Storage};
use skyward_vab::SnapshotState;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Marketed RAM size: the smallest standard module size that holds `MemTotal`.
pub fn ram(kib: u64) -> String {
    const SIZES: [f64; 16] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 18.0, 20.0, 24.0, 32.0, 48.0, 64.0];
    let gib = kib as f64 / (1024.0 * 1024.0);
    let size = SIZES.iter().copied().find(|&s| s >= gib).unwrap_or(gib.ceil());
    format!("{size:.1}GB")
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

pub fn panel((w, h): (u32, u32)) -> String {
    format!("{}×{}", w.max(h), w.min(h))
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

pub fn android(release: &str, patch: Option<&str>) -> String {
    match patch {
        Some(p) => format!("{release} | Android 安全更新：{p}"),
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
        SnapshotState::Unknown => "无法读取",
    }
}

pub fn data(d: DataState) -> &'static str {
    match d {
        DataState::Unmounted => "未挂载",
        DataState::Locked => "未解密",
        DataState::Decrypted => "已解密",
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
        assert_eq!(ram(15_556_972), "16.0GB");
        assert_eq!(ram(15_472_464), "16.0GB");
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

    // Xiaomi 13 Ultra build and panel.
    #[test]
    fn versions_and_states() {
        assert_eq!(android("16", Some("2026-04-01")), "16 | Android 安全更新：2026-04-01");
        assert_eq!(panel((1440, 3200)), "3200×1440");
        assert_eq!(verified("orange"), "橙色 · 设备已解锁，不校验系统");
        assert_eq!(health("Watchdog timer expire"), "Watchdog timer expire");
    }
}
