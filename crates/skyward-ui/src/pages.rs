use crate::app::{Action, Confirm, Frame, Hit, Screen, State, Switch};
use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::fmt;
use crate::font::Fonts;
use crate::geom::Rect;
use crate::theme::Theme;
use crate::widgets::{self, Fact, Icon, Outline, Row, Spec, Tone, Trail};
use skyward_info::{Charge, DataState, DeviceInfo, VERSION};

// Tile colours sampled from the stock Settings, plus orange and red from the
// same family for the destructive and reboot entries.
const BLUE: Rgba = Rgba::rgb(74, 128, 247);
const GREEN: Rgba = Rgba::rgb(97, 202, 81);
const PURPLE: Rgba = Rgba::rgb(116, 104, 240);
const SLATE: Rgba = Rgba::rgb(143, 156, 174);
const ORANGE: Rgba = Rgba::rgb(255, 146, 46);
const RED: Rgba = Rgba::rgb(242, 83, 75);

/// What every page draws from: style, typefaces, the device readings, where
/// userdata stands, and the choices made on the pages.
pub struct Ctx<'a> {
    pub t: &'a Theme,
    pub f: &'a Fonts,
    pub info: &'a DeviceInfo,
    pub data: DataState,
    pub state: &'a State,
}

pub fn render(screen: Screen, cx: &Ctx) -> Frame {
    match screen {
        Screen::Decrypt => decrypt(cx),
        Screen::Home => home(cx),
        Screen::Settings => settings(cx),
        Screen::Reboot => reboot(cx),
        Screen::Wipe => wipe(cx),
        Screen::About => about(cx),
        Screen::Specs => specs(cx),
    }
}

/// Label and colour of each swipe-to-confirm control.
pub fn confirm_label(which: Confirm) -> (&'static str, bool) {
    match which {
        Confirm::Reboot => ("滑动以重启", false),
        Confirm::Wipe => ("滑动以清除", true),
    }
}

const REBOOT_TARGETS: [(&str, Option<&str>); 6] = [
    ("系统", None),
    ("恢复模式", None),
    ("Fastboot", Some("用户空间 fastbootd")),
    ("Bootloader", None),
    ("EDL", Some("高通紧急下载模式")),
    ("关机", None),
];

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

fn hits(rects: &[Rect], actions: &[Action]) -> Vec<Hit> {
    rects.iter().zip(actions).map(|(&rect, &action)| Hit { rect, action }).collect()
}

/// Recovery home, laid out like the Settings root: title, search, a device
/// card with the readings that matter before flashing, then grouped entries.
pub fn home(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let mut c = page(t);
    let mut out = Vec::new();
    let mut y = widgets::header(&mut c, t, f, "Skyward", false);
    let search_top = y;
    y = widgets::search_bar(&mut c, t, f, y, "搜索功能与分区");
    out.push(Hit { rect: Rect::new(t.m.margin, search_top, t.m.w as f32 - t.m.margin * 2.0, t.m.search_h), action: Action::Unavailable("搜索还没实现") });

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
    if let Some(note) = fmt::snapshot_note(&i.snapshot) {
        rows.push(Row::new(fmt::snapshot(&i.snapshot)).icon(ORANGE, Icon::Layers).subtitle(note).trail(Trail::None));
    }
    let card = widgets::device_card(&mut c, t, f, y, &i.model, &sub, &stats, &rows);
    out.push(Hit { rect: card.rows[0], action: Action::Open(Screen::About) });
    y = card.bottom + t.m.card_gap;
    let tools = widgets::list(&mut c, t, f, y, &[
        Row::new("安装").icon(GREEN, Icon::Install),
        Row::new("备份").icon(BLUE, Icon::Backup),
        Row::new("恢复").icon(PURPLE, Icon::Restore),
        Row::new("挂载").icon(SLATE, Icon::Mount),
    ]);
    out.extend(hits(&tools.rows, &[
        Action::Unavailable("安装还没实现"),
        Action::Unavailable("备份还没实现"),
        Action::Unavailable("恢复还没实现"),
        Action::Unavailable("挂载还没实现"),
    ]));
    y = tools.bottom + t.m.card_gap;
    let target = REBOOT_TARGETS[cx.state.reboot].0;
    let system = widgets::list(&mut c, t, f, y, &[
        Row::new("清除数据").icon(RED, Icon::Wipe),
        Row::new("高级").icon(SLATE, Icon::Sliders),
        Row::new("设置").icon(SLATE, Icon::Gear),
        Row::new("重启").icon(ORANGE, Icon::Power).trail(Trail::Value(target)),
    ]);
    out.extend(hits(&system.rows, &[
        Action::Open(Screen::Wipe),
        Action::Unavailable("高级功能还没实现"),
        Action::Open(Screen::Settings),
        Action::Open(Screen::Reboot),
    ]));
    Frame { canvas: c, hits: out, title: "Skyward", back: false, hero: false, pattern: None }
}

/// Preferences, opening with the stock「显示与亮度」theme previews.
pub fn settings(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(t);
    let mut out = vec![Hit { rect: widgets::back_rect(t), action: Action::Back }];
    let top = widgets::header(&mut c, t, f, "设置", true);
    let themes = widgets::theme_picker(&mut c, t, f, top + t.m.dp(8.9));
    out.extend(hits(&themes.rows, &[Action::Theme { dark: false }, Action::Theme { dark: true }]));
    let mut y = widgets::section(&mut c, t, f, themes.bottom, "亮度");
    let slider = widgets::slider_card(&mut c, t, f, y, "屏幕亮度", s.brightness, Some(Row::new("息屏时间").trail(Trail::Value("2 分钟"))));
    out.push(Hit { rect: slider.track, action: Action::Brightness });
    if let Some(r) = slider.below {
        out.push(Hit { rect: r, action: Action::Unavailable("息屏时间还不能修改") });
    }
    y = widgets::section(&mut c, t, f, slider.bottom, "通用");
    let general = widgets::list(&mut c, t, f, y, &[
        Row::new("语言").icon(BLUE, Icon::Globe).trail(Trail::Value("简体中文")),
        Row::new("时区").icon(ORANGE, Icon::Clock).trail(Trail::Value("中国标准时间")),
        Row::new("触摸振动").icon(PURPLE, Icon::Vibrate).trail(Trail::Switch(s.vibrate)),
    ]);
    out.extend(hits(&general.rows, &[
        Action::Unavailable("语言还不能修改"),
        Action::Unavailable("时区还不能修改"),
        Action::Toggle(Switch::Vibrate),
    ]));
    y = widgets::section(&mut c, t, f, general.bottom, "调试");
    let debug = widgets::list(&mut c, t, f, y, &[
        Row::new("ADB Sideload").icon(GREEN, Icon::Usb).subtitle("通过电脑上的 adb 推送安装包").trail(Trail::Switch(s.sideload)),
        Row::new("终端").icon(SLATE, Icon::Terminal),
    ]);
    out.extend(hits(&debug.rows, &[Action::Toggle(Switch::Sideload), Action::Unavailable("终端还没实现")]));
    y = widgets::section(&mut c, t, f, debug.bottom, "关于");
    let about = widgets::list(&mut c, t, f, y, &[Row::new("关于本机").trail(Trail::Value(VERSION))]);
    out.extend(hits(&about.rows, &[Action::Open(Screen::About)]));
    Frame { canvas: fit(c, t, about.bottom), hits: out, title: "设置", back: true, hero: false, pattern: None }
}

/// About this device, modelled on the stock「我的设备」at rest: the wordmark
/// over a deep blue backdrop, then translucent summary, hardware and link
/// cards.
pub fn about(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let m = &t.m;
    let tone = Tone::hero(t);
    let mut c = long_page(t);
    let mut out = vec![Hit { rect: widgets::back_rect(t), action: Action::Back }];
    widgets::hero_backdrop(&mut c, t);
    widgets::back_arrow(&mut c, t, t.hero.title);
    widgets::wordmark(&mut c, t, f, "Skyward", m.dp(233.7), m.dp(37.8));
    let version_px = m.dp(13.3);
    let w = c.text_width(&f.body, VERSION, version_px);
    let base = m.dp(296.3) + f.body.cap(version_px);
    c.text(&f.body, VERSION, (m.w as f32 - w) / 2.0, base, version_px, t.hero.caption, None);

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
    let summary = widgets::facts(&mut c, t, f, m.dp(490.7), &facts, &tone);
    out.push(Hit { rect: summary.rows[1], action: Action::Open(Screen::Specs) });

    let mut specs = vec![("处理器", i.soc.clone().unwrap_or_else(|| fmt::cpu(&i.cpu))), ("运行内存", fmt::ram(i.ram_kib))];
    if let Some(design) = i.battery.as_ref().and_then(|b| b.design) {
        specs.push(("电池容量", fmt::mah(design)));
    }
    if let Some(d) = i.screen {
        specs.push(("屏幕尺寸", fmt::inches(d)));
    }
    if let Some(p) = i.panel {
        specs.push(("分辨率", fmt::panel(p)));
    }
    let y = widgets::spec_card(&mut c, t, f, summary.bottom + m.card_gap, &fmt::model_lines(&i.model), &specs, &tone);
    let links = widgets::list_toned(&mut c, t, f, y + m.card_gap, &[Row::new("全部参数与信息"), Row::new("导出日志")], &tone);
    out.extend(hits(&links.rows, &[Action::Open(Screen::Specs), Action::Unavailable("导出日志还没实现")]));
    Frame { canvas: fit(c, t, links.bottom), hits: out, title: "关于本机", back: true, hero: true, pattern: None }
}

/// Every reading the recovery has, modelled on the stock「全部参数与信息」.
pub fn specs(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let gap = t.m.card_gap;
    let mut c = long_page(t);
    let mut out = vec![Hit { rect: widgets::back_rect(t), action: Action::Back }];
    widgets::header(&mut c, t, f, "全部参数与信息", true);
    // This page leaves more room under the title than the settings pages.
    let top = t.m.dp(188.0);

    let mut grid = vec![Spec { icon: Outline::Phone, label: "设备型号", value: fmt::model_lines(&i.model) }];
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
    if let Some(d) = i.screen {
        grid.push(Spec { icon: Outline::Screen, label: "屏幕尺寸", value: fmt::inches(d) });
    }
    grid.push(Spec { icon: Outline::Storage, label: "存储容量", value: fmt::capacity(i.storage.total) });
    grid.push(Spec { icon: Outline::Tag, label: "设备代号", value: i.codename.clone() });
    let mut y = widgets::spec_grid(&mut c, t, f, top, &grid);

    if let Some(v) = &i.os_version {
        y = widgets::info_card(&mut c, t, f, y + gap, &[("OS 版本", v.clone())]);
    }
    if let Some(r) = &i.android {
        y = widgets::info_card(&mut c, t, f, y + gap, &[("Android 版本", fmt::android(r, i.security_patch.as_ref()))]);
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
    let links = widgets::list(&mut c, t, f, y + gap, &[
        Row::new("开源许可").trail(Trail::Value("GPL-3.0")),
        Row::new("界面字体").trail(Trail::Value("MiSans")),
    ]);
    out.extend(hits(&links.rows, &[Action::Unavailable("许可证全文还没放进来"), Action::Unavailable("字体许可还没放进来")]));
    Frame { canvas: fit(c, t, links.bottom), hits: out, title: "全部参数与信息", back: true, hero: false, pattern: None }
}

/// File-based-encryption unlock.
pub fn decrypt(cx: &Ctx) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(t);
    let m = &t.m;
    let x = m.w as f32 / 2.0;
    let lock_cy = m.dp(150.0);
    c.fill_circle(x, lock_cy, m.dp(30.0), t.pal.card);
    widgets::glyph(&mut c, Icon::Lock, x, lock_cy, m.dp(40.0), t.pal.accent, t.pal.card);
    c.text_center(&f.heading, "输入锁屏图案", x, lock_cy + m.dp(64.0), m.dp(24.0), t.pal.text);
    c.text_center(&f.body, "解锁后才能读取「数据」分区", x, lock_cy + m.dp(98.0), m.sub_px, t.pal.subtitle);
    let (pcx, pcy, span) = (x, m.h as f32 * 0.6, m.dp(190.0));
    widgets::pattern_lock(&mut c, t, pcx, pcy, span, &cx.state.pattern);
    let skip_cy = m.h as f32 - m.dp(64.0);
    c.text_center(&f.heading, "暂不解密", x, skip_cy, m.row_title_px, t.pal.accent);
    let reach = span / 2.0 + m.dp(40.0);
    let out = vec![
        Hit { rect: Rect::new(pcx - reach, pcy - reach, reach * 2.0, reach * 2.0), action: Action::Pattern },
        Hit { rect: Rect::new(x - m.dp(80.0), skip_cy - m.dp(24.0), m.dp(160.0), m.dp(48.0)), action: Action::Skip },
    ];
    Frame { canvas: c, hits: out, title: "解密", back: false, hero: false, pattern: Some((pcx, pcy, span)) }
}

/// Reboot target picker with a swipe-to-confirm control.
pub fn reboot(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(t);
    let mut out = vec![Hit { rect: widgets::back_rect(t), action: Action::Back }];
    let top = widgets::header(&mut c, t, f, "重启", true);
    let mut y = widgets::section_first(&mut c, t, f, top, "重启到");
    let rows: Vec<Row> = REBOOT_TARGETS
        .iter()
        .enumerate()
        .map(|(k, &(title, sub))| {
            let r = Row::new(title).trail(Trail::Check(s.reboot == k));
            match sub {
                Some(text) => r.subtitle(text),
                None => r,
            }
        })
        .collect();
    let targets = widgets::list(&mut c, t, f, y, &rows);
    let picks: Vec<Action> = (0..rows.len()).map(Action::RebootTarget).collect();
    out.extend(hits(&targets.rows, &picks));
    y = targets.bottom;
    if let Some(slot) = cx.info.slot {
        let next = format!("{}（当前）", slot.letter());
        y = widgets::section(&mut c, t, f, y, "槽位");
        let slots = widgets::list(&mut c, t, f, y, &[Row::new("下次启动").trail(Trail::Value(&next))]);
        out.extend(hits(&slots.rows, &[Action::Unavailable("切换槽位还没实现")]));
        y = slots.bottom;
    }
    let (label, danger) = confirm_label(Confirm::Reboot);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(28.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::Reboot) });
    Frame { canvas: c, hits: out, title: "重启", back: true, hero: false, pattern: None }
}

/// Destructive wipe with the partitions to clear.
pub fn wipe(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(t);
    let mut out = vec![Hit { rect: widgets::back_rect(t), action: Action::Back }];
    let top = widgets::header(&mut c, t, f, "清除数据", true);
    let y = widgets::section_first(&mut c, t, f, top, "选择要清除的内容");
    let items = widgets::list(&mut c, t, f, y, &[
        Row::new("用户数据").subtitle("应用、账户与设置").trail(Trail::Check(s.wipe[0])),
        Row::new("内部存储").subtitle("照片、视频与下载的文件").trail(Trail::Check(s.wipe[1])),
        Row::new("ART 缓存").subtitle("应用的预编译代码，开机时重新生成").trail(Trail::Check(s.wipe[2])),
        Row::new("Metadata").subtitle("加密元数据，清除后「数据」分区将无法解密").trail(Trail::Check(s.wipe[3])),
    ]);
    out.extend(hits(&items.rows, &[Action::WipeItem(0), Action::WipeItem(1), Action::WipeItem(2), Action::WipeItem(3)]));
    let y = widgets::footnote(&mut c, t, f, items.bottom, "清除后无法恢复。内部存储里的照片和文件不会一起删除，除非你勾选了它。");
    let (label, danger) = confirm_label(Confirm::Wipe);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::Wipe) });
    Frame { canvas: c, hits: out, title: "清除数据", back: true, hero: false, pattern: None }
}
