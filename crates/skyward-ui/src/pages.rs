use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::fmt;
use crate::font::Fonts;
use crate::theme::Theme;
use crate::widgets::{self, Bar, Fact, Icon, Outline, Row, Spec, Trail};
use skyward_info::{Charge, DataState, DeviceInfo, VERSION};
use skyward_vab::SnapshotState;

// Tile colours sampled from the stock Settings, plus orange and red from the
// same family for the destructive and reboot entries.
const BLUE: Rgba = Rgba::rgb(74, 128, 247);
const GREEN: Rgba = Rgba::rgb(97, 202, 81);
const PURPLE: Rgba = Rgba::rgb(116, 104, 240);
const SLATE: Rgba = Rgba::rgb(143, 156, 174);
const ORANGE: Rgba = Rgba::rgb(255, 146, 46);
const RED: Rgba = Rgba::rgb(242, 83, 75);

/// What every page draws from: style, typefaces, the device readings, where
/// userdata stands, and the clock text for the status bar.
pub struct Ctx<'a> {
    pub t: &'a Theme,
    pub f: &'a Fonts,
    pub info: &'a DeviceInfo,
    pub data: DataState,
    pub clock: &'a str,
}

impl Ctx<'_> {
    fn bar(&self) -> Bar<'_> {
        let battery = self.info.battery.as_ref().map(|b| (b.level, b.charge == Charge::Charging));
        Bar { clock: self.clock, battery }
    }
}

fn page(t: &Theme) -> Canvas {
    let mut c = Canvas::new(t.m.w, t.m.h);
    c.clear(t.pal.bg);
    c
}

/// A canvas tall enough for a scrolling page; [`fit`] trims it to the content.
fn long_page(t: &Theme) -> Canvas {
    let mut c = Canvas::new(t.m.w, t.m.h * 3);
    c.clear(t.pal.bg);
    c
}

/// Trim a long page to its content plus the bottom margin, and never shorter
/// than one screen.
fn fit(c: Canvas, t: &Theme, bottom: f32) -> Canvas {
    let h = ((bottom + t.m.margin * 2.0).ceil() as u32).max(t.m.h);
    c.crop_height(h)
}

/// Recovery home, laid out like the Settings root: title, search, a device
/// card with the readings that matter before flashing, then grouped entries.
pub fn home(cx: &Ctx) -> Canvas {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let mut c = page(t);
    let mut y = widgets::header(&mut c, t, f, &cx.bar(), "Skyward", false);
    y = widgets::search_bar(&mut c, t, f, y, "搜索功能与分区");

    let sub = match &i.os_version {
        Some(v) => format!("{} · {v}", i.codename),
        None => i.codename.clone(),
    };
    let free = i.storage.free.map(fmt::gb);
    let level = i.battery.as_ref().map(|b| format!("{}%", b.level));
    let mut stats = Vec::new();
    if let Some(s) = i.slot {
        stats.push((s.letter(), "当前槽位"));
    }
    stats.push((fmt::data(cx.data), "数据分区"));
    if let Some(v) = &free {
        stats.push((v.as_str(), "可用空间"));
    }
    if let (Some(v), Some(b)) = (&level, &i.battery) {
        let caption = match b.charge {
            Charge::Charging => "充电中",
            Charge::Full => "已充满",
            _ => "电量",
        };
        stats.push((v.as_str(), caption));
    }
    let mut rows = Vec::new();
    if matches!(i.snapshot, SnapshotState::Snapshotted | SnapshotState::Merging) {
        rows.push(
            Row::new(fmt::snapshot(&i.snapshot))
                .icon(ORANGE, Icon::Layers)
                .subtitle("合并完成前不要刷写 super 里的分区")
                .trail(Trail::None),
        );
    }
    y = widgets::device_card(&mut c, t, f, y, &i.model, &sub, &stats, &rows);
    y += t.m.card_gap;
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("安装").icon(GREEN, Icon::Install),
        Row::new("备份").icon(BLUE, Icon::Backup).trail(Trail::Value("3 个")),
        Row::new("恢复").icon(PURPLE, Icon::Restore),
        Row::new("挂载").icon(SLATE, Icon::Mount),
    ]);
    y += t.m.card_gap;
    widgets::list(&mut c, t, f, y, &[
        Row::new("清除数据").icon(RED, Icon::Wipe),
        Row::new("高级").icon(SLATE, Icon::Sliders),
        Row::new("设置").icon(SLATE, Icon::Gear),
        Row::new("重启").icon(ORANGE, Icon::Power).trail(Trail::Value("系统")),
    ]);
    c
}

/// Preferences, opening with the stock「显示与亮度」theme previews.
pub fn settings(cx: &Ctx) -> Canvas {
    let (t, f) = (cx.t, cx.f);
    let mut c = long_page(t);
    let top = widgets::header(&mut c, t, f, &cx.bar(), "设置", true);
    let mut y = widgets::theme_picker(&mut c, t, f, top + t.m.dp(8.9));
    y = widgets::section(&mut c, t, f, y, "亮度");
    y = widgets::slider_card(&mut c, t, f, y, "屏幕亮度", 0.62, Some(Row::new("息屏时间").trail(Trail::Value("2 分钟"))));
    y = widgets::section(&mut c, t, f, y, "通用");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("语言").icon(BLUE, Icon::Globe).trail(Trail::Value("简体中文")),
        Row::new("时区").icon(ORANGE, Icon::Clock).trail(Trail::Value("中国标准时间")),
        Row::new("触摸振动").icon(PURPLE, Icon::Vibrate).trail(Trail::Switch(true)),
    ]);
    y = widgets::section(&mut c, t, f, y, "调试");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("ADB Sideload").icon(GREEN, Icon::Usb).subtitle("通过电脑上的 adb 推送安装包").trail(Trail::Switch(false)),
        Row::new("终端").icon(SLATE, Icon::Terminal),
    ]);
    y = widgets::section(&mut c, t, f, y, "关于");
    y = widgets::list(&mut c, t, f, y, &[Row::new("关于本机").trail(Trail::Value(VERSION))]);
    fit(c, t, y)
}

/// About this device, modelled on the stock「我的设备」: the wordmark over a
/// coloured backdrop, a summary card and the hardware card.
pub fn about(cx: &Ctx) -> Canvas {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let m = &t.m;
    let mut c = long_page(t);
    widgets::hero_backdrop(&mut c, t);
    widgets::status_bar(&mut c, t, f, &cx.bar());
    widgets::back_arrow(&mut c, t);
    widgets::wordmark(&mut c, t, f, "Skyward", m.dp(205.1), m.dp(39.0));
    let w = c.text_width(&f.body, VERSION, m.sub_px);
    let base = m.dp(270.3) + f.body.cap(m.sub_px);
    c.text(&f.body, VERSION, (m.w as f32 - w) / 2.0, base, m.sub_px, t.pal.hero.caption, None);
    widgets::pill(&mut c, t, f, m.dp(392.6), "导出日志");

    let storage = fmt::storage_used(&i.storage).unwrap_or_else(|| fmt::capacity(i.storage.total));
    let mut facts = vec![
        Fact { title: "设备型号", value: i.model.clone(), link: false },
        Fact { title: "存储空间", value: storage, link: true },
    ];
    if let Some(v) = &i.os_version {
        facts.push(Fact { title: "OS 版本", value: v.clone(), link: false });
    }
    if let Some(r) = &i.android {
        facts.push(Fact { title: "Android 版本", value: r.clone(), link: false });
    }
    let mut y = widgets::facts(&mut c, t, f, m.dp(506.9), &facts);

    let mut specs = vec![("处理器", i.soc.clone().unwrap_or_else(|| fmt::cpu(&i.cpu))), ("运行内存", fmt::ram(i.ram_kib))];
    if let Some(design) = i.battery.as_ref().and_then(|b| b.design) {
        specs.push(("电池容量", fmt::mah(design)));
    }
    y = widgets::spec_card(&mut c, t, f, y + m.card_gap, &i.model, &specs);
    y = widgets::list(&mut c, t, f, y + m.card_gap, &[Row::new("全部参数与信息")]);
    fit(c, t, y)
}

/// Every reading the recovery has, modelled on the stock「全部参数与信息」.
pub fn specs(cx: &Ctx) -> Canvas {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let gap = t.m.card_gap;
    let mut c = long_page(t);
    widgets::header(&mut c, t, f, &cx.bar(), "全部参数与信息", true);
    // This page leaves more room under the title than the settings pages.
    let top = t.m.dp(229.1);

    let mut grid = vec![Spec { icon: Outline::Phone, label: "设备型号", value: i.model.clone() }];
    if let Some(design) = i.battery.as_ref().and_then(|b| b.design) {
        grid.push(Spec { icon: Outline::Battery, label: "电池容量", value: fmt::mah(design) });
    }
    let cpu = match &i.soc {
        Some(soc) => format!("{soc}\n{}", fmt::cpu(&i.cpu)),
        None => fmt::cpu(&i.cpu),
    };
    grid.push(Spec { icon: Outline::Chip, label: "处理器", value: cpu });
    grid.push(Spec { icon: Outline::Memory, label: "运行内存", value: fmt::ram(i.ram_kib) });
    if let Some(p) = i.panel {
        grid.push(Spec { icon: Outline::Resolution, label: "分辨率", value: fmt::panel(p) });
    }
    grid.push(Spec { icon: Outline::Storage, label: "存储容量", value: fmt::capacity(i.storage.total) });
    grid.push(Spec { icon: Outline::Tag, label: "设备代号", value: i.codename.clone() });
    let mut y = widgets::spec_grid(&mut c, t, f, top, &grid);

    if let Some(v) = &i.os_version {
        y = widgets::info_card(&mut c, t, f, y + gap, &[("OS 版本", v.clone())]);
    }
    if let Some(r) = &i.android {
        y = widgets::info_card(&mut c, t, f, y + gap, &[("Android 版本", fmt::android(r, i.security_patch.as_deref()))]);
    }
    y = widgets::info_card(&mut c, t, f, y + gap, &[
        ("Recovery 版本", format!("Skyward {VERSION}")),
        ("内核版本", i.kernel.clone()),
    ]);

    let mut boot = Vec::new();
    if let Some(s) = i.slot {
        boot.push(("当前槽位", s.letter().to_string()));
        boot.push(("系统更新", fmt::snapshot(&i.snapshot).to_string()));
    }
    if let Some(unlocked) = i.boot.unlocked {
        boot.push(("Bootloader", if unlocked { "已解锁" } else { "已锁定" }.to_string()));
    }
    if let Some(v) = &i.boot.verified {
        boot.push(("验证启动", fmt::verified(v)));
    }
    boot.push(("数据分区", fmt::data(cx.data).to_string()));
    y = widgets::info_card(&mut c, t, f, y + gap, &boot);

    if let Some(b) = &i.battery {
        let mut rows = vec![("电量", fmt::level(b))];
        if let Some(v) = b.temp {
            rows.push(("温度", fmt::celsius(v)));
        }
        if let Some(v) = fmt::wear(b) {
            rows.push(("健康", v));
        }
        if let Some(v) = b.cycles {
            rows.push(("充电循环", format!("{v} 次")));
        }
        if let Some(v) = b.voltage {
            rows.push(("电压", fmt::volts(v)));
        }
        y = widgets::info_card(&mut c, t, f, y + gap, &rows);
    }

    let storage = match i.storage.free {
        Some(free) => format!("可用存储 {} / 总存储 {}", fmt::gb(free), fmt::capacity(i.storage.total)),
        None => format!("总存储 {} · 数据分区未挂载", fmt::capacity(i.storage.total)),
    };
    y = widgets::info_card(&mut c, t, f, y + gap, &[("机身存储", storage)]);
    y = widgets::list(&mut c, t, f, y + gap, &[
        Row::new("开源许可").trail(Trail::Value("GPL-3.0")),
        Row::new("界面字体").trail(Trail::Value("MiSans")),
    ]);
    fit(c, t, y)
}

/// File-based-encryption unlock.
pub fn decrypt(cx: &Ctx, path: &[usize]) -> Canvas {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, &cx.bar());
    let x = m.w as f32 / 2.0;
    let lock_cy = m.dp(150.0);
    c.fill_circle(x, lock_cy, m.dp(30.0), t.pal.card);
    widgets::glyph(&mut c, Icon::Lock, x, lock_cy, m.dp(40.0), t.pal.accent, t.pal.card);
    c.text_center(&f.heading, "输入锁屏图案", x, lock_cy + m.dp(64.0), m.dp(24.0), t.pal.text);
    c.text_center(&f.body, "解锁后才能读取「数据」分区", x, lock_cy + m.dp(98.0), m.sub_px, t.pal.subtitle);
    widgets::pattern_lock(&mut c, t, x, m.h as f32 * 0.6, m.dp(190.0), path);
    c.text_center(&f.heading, "暂不解密", x, m.h as f32 - m.dp(64.0), m.row_title_px, t.pal.accent);
    c
}

/// Reboot target picker with a swipe-to-confirm control.
pub fn reboot(cx: &Ctx) -> Canvas {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(t);
    let top = widgets::header(&mut c, t, f, &cx.bar(), "重启", true);
    let mut y = widgets::section_first(&mut c, t, f, top, "重启到");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("系统").trail(Trail::Check(true)),
        Row::new("恢复模式").trail(Trail::Check(false)),
        Row::new("Fastboot").subtitle("用户空间 fastbootd").trail(Trail::Check(false)),
        Row::new("Bootloader").trail(Trail::Check(false)),
        Row::new("EDL").subtitle("高通紧急下载模式").trail(Trail::Check(false)),
        Row::new("关机").trail(Trail::Check(false)),
    ]);
    if let Some(s) = cx.info.slot {
        let next = format!("{}（当前）", s.letter());
        y = widgets::section(&mut c, t, f, y, "槽位");
        y = widgets::list(&mut c, t, f, y, &[Row::new("下次启动").trail(Trail::Value(&next))]);
    }
    y += t.m.dp(28.0);
    widgets::confirm(&mut c, t, f, y, "滑动以重启", false);
    c
}

/// Destructive wipe with the partitions to clear.
pub fn wipe(cx: &Ctx) -> Canvas {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(t);
    let top = widgets::header(&mut c, t, f, &cx.bar(), "清除数据", true);
    let mut y = widgets::section_first(&mut c, t, f, top, "选择要清除的内容");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("用户数据").subtitle("应用、账户与设置 · 12.4 GB").trail(Trail::Check(true)),
        Row::new("内部存储").subtitle("照片、视频与下载的文件 · 378 GB").trail(Trail::Check(false)),
        Row::new("ART 缓存").subtitle("2.1 GB").trail(Trail::Check(true)),
        Row::new("Metadata").subtitle("加密元数据，清除后「数据」分区将无法解密").trail(Trail::Check(false)),
    ]);
    y = widgets::footnote(&mut c, t, f, y, "清除后无法恢复。内部存储里的照片和文件不会一起删除，除非你勾选了它。");
    y += t.m.dp(24.0);
    widgets::confirm(&mut c, t, f, y, "滑动以清除", true);
    c
}

/// Flashing progress.
pub fn install_progress(cx: &Ctx) -> Canvas {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(t);
    let m = &t.m;
    let top = widgets::header(&mut c, t, f, &cx.bar(), "安装", true);
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let h = m.dp(118.0);
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, t.pal.card);
    let row_cy = top + m.row_h / 2.0 + m.dp(4.0);
    widgets::tile(&mut c, t, card_x + m.inner, row_cy - m.icon / 2.0, GREEN, Icon::Install);
    let tx = card_x + m.inner + m.icon + m.icon_gap;
    c.text_vc(&f.heading, "kernelsu_patched.img", tx, row_cy - m.dp(10.0), m.row_title_px, t.pal.text, Some(right - tx - m.inner));
    c.text_vc(&f.body, "正在写入 init_boot_b", tx, row_cy + m.dp(12.0), m.sub_px, t.pal.subtitle, None);
    let bar_y = top + h - m.dp(30.0);
    widgets::progress_bar(&mut c, t, card_x + m.inner, bar_y, right - card_x - m.inner * 2.0 - m.dp(46.0), 0.68);
    let pct = "68%";
    let pw = c.text_width(&f.heading, pct, m.sub_px);
    c.text_vc(&f.heading, pct, right - m.inner - pw, bar_y + m.dp(3.0), m.sub_px, t.pal.accent, None);

    let y = widgets::section(&mut c, t, f, top + h, "日志");
    widgets::list(&mut c, t, f, y, &[
        Row::new("修补 ramdisk").trail(Trail::Text("完成")),
        Row::new("写入 init_boot_a").trail(Trail::Text("完成")),
        Row::new("写入 init_boot_b").trail(Trail::Text("进行中")),
        Row::new("校验镜像").trail(Trail::Text("等待")),
    ]);
    c
}
