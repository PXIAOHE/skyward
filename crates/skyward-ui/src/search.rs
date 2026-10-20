use crate::app::Screen;

/// A feature or partition the search finds, and the page it opens.
pub struct Entry {
    pub title: &'static str,
    /// Where it lives, shown under the title.
    pub place: &'static str,
    pub screen: Screen,
    /// What it answers to besides its title: pinyin in full and by
    /// initials, and English, since the keyboard types Latin letters.
    pub keys: &'static [&'static str],
}

const fn entry(title: &'static str, place: &'static str, screen: Screen, keys: &'static [&'static str]) -> Entry {
    Entry { title, place, screen, keys }
}

pub const ENTRIES: &[Entry] = &[
    entry("安装", "主页", Screen::Install, &["anzhuang", "az", "install", "shuaji", "sj"]),
    entry("从内部存储安装", "安装", Screen::Install, &["congneibucunchuanzhuang", "cnbccaz", "neicun", "sdcard", "zip"]),
    entry("通过电脑安装", "安装", Screen::Install, &["tongguodiannaoanzhuang", "tgdnaz", "sideload", "adb"]),
    entry("备份", "主页", Screen::Backup, &["beifen", "bf", "backup"]),
    entry("恢复", "主页", Screen::Restore, &["huifu", "hf", "restore"]),
    entry("挂载", "主页", Screen::Mount, &["guazai", "gz", "mount", "umount"]),
    entry("清除数据", "主页", Screen::Wipe, &["qingchushuju", "qcsj", "wipe", "format", "geshihua", "gsh"]),
    entry("高级", "主页", Screen::Advanced, &["gaoji", "gj", "advanced"]),
    entry("文件管理", "高级", Screen::Files, &["wenjianguanli", "wjgl", "files", "file"]),
    entry("终端", "高级", Screen::Terminal, &["zhongduan", "zd", "terminal", "shell"]),
    entry("导出日志", "高级", Screen::Logs, &["daochurizhi", "dcrz", "rizhi", "rz", "logs", "dmesg"]),
    entry("设置", "主页", Screen::Settings, &["shezhi", "sz", "settings"]),
    entry("深色模式", "设置", Screen::Settings, &["shensemoshi", "ssms", "dark", "theme"]),
    entry("屏幕亮度", "设置", Screen::Settings, &["pingmuliangdu", "pmld", "liangdu", "ld", "brightness"]),
    entry("息屏时间", "设置", Screen::ScreenOff, &["xipingshijian", "xpsj", "timeout"]),
    entry("触摸振动", "设置", Screen::Settings, &["chumozhendong", "cmzd", "zhendong", "vibrate", "haptic"]),
    entry("语言", "设置", Screen::Language, &["yuyan", "yy", "language"]),
    entry("时区", "设置", Screen::TimeZone, &["shiqu", "sq", "timezone"]),
    entry("重启", "主页", Screen::Reboot, &["chongqi", "cq", "reboot", "fastboot", "bootloader", "edl", "guanji"]),
    entry("启动槽位", "重启", Screen::Slot, &["qidongcaowei", "qdcw", "caowei", "cw", "slot"]),
    entry("关于本机", "设置", Screen::About, &["guanyubenji", "gybj", "about", "banben", "version"]),
    entry("全部参数与信息", "设置 > 关于本机", Screen::Specs, &["quanbucanshuyuxinxi", "qbcsyxx", "canshu", "specs"]),
    entry("开源许可", "设置 > 关于本机 > 全部参数与信息", Screen::License, &["kaiyuanxuke", "kyxk", "license", "gpl"]),
    entry("界面字体", "设置 > 关于本机 > 全部参数与信息", Screen::FontLicense, &["jiemianziti", "jmzt", "ziti", "font", "misans", "kirkmono", "dengkuan"]),
    entry("boot", "安装", Screen::Install, &[]),
    entry("init_boot", "安装", Screen::Install, &[]),
    entry("vendor_boot", "安装", Screen::Install, &[]),
    entry("recovery", "安装", Screen::Install, &[]),
    entry("dtbo", "安装", Screen::Install, &[]),
    entry("vbmeta", "安装", Screen::Install, &[]),
    entry("system", "挂载", Screen::Mount, &[]),
    entry("system_ext", "挂载", Screen::Mount, &[]),
    entry("product", "挂载", Screen::Mount, &[]),
    entry("vendor", "挂载", Screen::Mount, &[]),
    entry("odm", "挂载", Screen::Mount, &[]),
    entry("vendor_dlkm", "挂载", Screen::Mount, &[]),
    entry("system_dlkm", "挂载", Screen::Mount, &[]),
    entry("odm_dlkm", "挂载", Screen::Mount, &[]),
];

/// The entries `query` finds, best first: a title holding it, then a key
/// starting with it, then a key holding it.
pub fn find(query: &str) -> Vec<&'static Entry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let rank = |e: &Entry| {
        if e.title.to_lowercase().contains(&q) {
            Some(0)
        } else if e.keys.iter().any(|k| k.starts_with(&q)) {
            Some(1)
        } else if e.keys.iter().any(|k| k.contains(&q)) {
            Some(2)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, usize, &Entry)> = ENTRIES.iter().enumerate().filter_map(|(k, e)| rank(e).map(|r| (r, k, e))).collect();
    found.sort_by_key(|&(r, k, _)| (r, k));
    found.into_iter().map(|(_, _, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(query: &str) -> Vec<&'static str> {
        find(query).iter().map(|e| e.title).collect()
    }

    #[test]
    fn pinyin_initials_and_english_all_find_a_feature() {
        assert_eq!(titles("bf"), ["备份"]);
        assert_eq!(titles("beifen"), ["备份"]);
        assert_eq!(titles("Backup"), ["备份"]);
        assert_eq!(titles("xpsj"), ["息屏时间"]);
    }

    #[test]
    fn a_title_holding_the_query_comes_before_keys_that_do() {
        // vendor_boot holds "boot" in its title; the reboot page only in a key.
        let found = titles("boot");
        assert_eq!(&found[..3], ["boot", "init_boot", "vendor_boot"]);
        assert!(found.contains(&"重启"), "{found:?}");
    }

    #[test]
    fn chinese_titles_match_as_typed() {
        assert_eq!(titles("挂载"), ["挂载"]);
    }

    #[test]
    fn nothing_typed_finds_nothing() {
        assert!(find("").is_empty());
        assert!(find("   ").is_empty());
        assert!(find("zzzz").is_empty());
    }
}
