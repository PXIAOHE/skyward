use crate::app::{Action, Confirm, Damage, Frame, HeroArt, Hit, Install, Pick, Screen, State, Switch, Target, SCREEN_OFF, TARGETS};
use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::fmt;
use crate::font::Fonts;
use crate::geom::Rect;
use crate::theme::Theme;
use crate::widgets::{self, Fact, Icon, Outline, Row, Spec, Tone, Trail};
use skyward_info::{Charge, DataState, DeviceInfo, VERSION};
use skyward_install::{Outcome, Stage};
use skyward_vab::Slot;

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
        Screen::Mount => mount(cx),
        Screen::Install => install(cx),
        Screen::Sideload => sideload(cx),
        Screen::About => about(cx),
        Screen::Specs => specs(cx),
        Screen::ScreenOff => screen_off(cx),
    }
}

/// Label and colour of each swipe-to-confirm control.
pub fn confirm_label(which: Confirm) -> (&'static str, bool) {
    match which {
        Confirm::Reboot => ("滑动以重启", false),
        Confirm::Wipe => ("滑动以清除", true),
        Confirm::Flash => ("滑动以刷入", true),
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
    let mut y = t.m.content_top;
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
        Action::Open(Screen::Install),
        Action::Unavailable("备份还没实现"),
        Action::Unavailable("恢复还没实现"),
        Action::Open(Screen::Mount),
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
    Frame { canvas: c, hits: out, title: "Skyward", back: false, header: true, content_top: search_top, hero: None, pattern: None }
}

/// Preferences, opening with the stock「显示与亮度」theme previews.
pub fn settings(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(t);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let themes = widgets::theme_picker(&mut c, t, f, top + t.m.dp(8.9));
    out.extend(hits(&themes.rows, &[Action::Theme { dark: false }, Action::Theme { dark: true }]));
    let mut y = widgets::section(&mut c, t, f, themes.bottom, "亮度");
    let off = fmt::screen_off(SCREEN_OFF[s.screen_off]);
    let slider = widgets::slider_card(&mut c, t, f, y, "屏幕亮度", s.brightness, Some(Row::new("息屏时间").trail(Trail::Value(&off))));
    out.push(Hit { rect: slider.track, action: Action::Brightness });
    if let Some(r) = slider.below {
        out.push(Hit { rect: r, action: Action::Open(Screen::ScreenOff) });
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
    let debug = widgets::list(&mut c, t, f, y, &[Row::new("终端").icon(SLATE, Icon::Terminal)]);
    out.extend(hits(&debug.rows, &[Action::Unavailable("终端还没实现")]));
    y = widgets::section(&mut c, t, f, debug.bottom, "关于");
    let about = widgets::list(&mut c, t, f, y, &[Row::new("关于本机").trail(Trail::Value(VERSION))]);
    out.extend(hits(&about.rows, &[Action::Open(Screen::About)]));
    Frame { canvas: fit(c, t, about.bottom), hits: out, title: "设置", back: true, header: true, content_top: top, hero: None, pattern: None }
}

/// How long the screen stays on with nothing touched, opened from the
/// settings.
pub fn screen_off(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(t);
    let top = t.m.content_top;
    let labels = SCREEN_OFF.map(fmt::screen_off);
    let rows: Vec<Row> = labels.iter().enumerate().map(|(k, label)| Row::new(label).trail(Trail::Check(s.screen_off == k))).collect();
    let times = widgets::list(&mut c, t, f, top, &rows);
    let picks: Vec<Action> = (0..SCREEN_OFF.len()).map(Action::ScreenOff).collect();
    let y = widgets::footnote(
        &mut c,
        t,
        f,
        times.bottom,
        "超过所选时间没有操作，屏幕会先变暗，再熄灭。熄灭后轻触屏幕或按任意键即可点亮；需要选择分区或安装结束时，屏幕也会自动点亮。",
    );
    if SCREEN_OFF[s.screen_off].is_none() {
        widgets::footnote(&mut c, t, f, y, "屏幕会一直亮着，同一画面显示太久会留下残影。");
    }
    Frame { canvas: c, hits: hits(&times.rows, &picks), title: "息屏时间", back: true, header: true, content_top: top, hero: None, pattern: None }
}

/// Mount manager: the slot's partitions, each with a switch to mount it
/// read-only or unmount it; one that programs are using says which, and
/// stays. While an update waits to merge, nothing more is mounted.
pub fn mount(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(t);
    let mut out = Vec::new();
    let top = t.m.content_top;

    let pending = fmt::snapshot_mount_note(&cx.info.snapshot);
    let y = match pending {
        Some(note) => {
            let warning = Row::new(fmt::snapshot(&cx.info.snapshot)).icon(ORANGE, Icon::Layers).subtitle(note).trail(Trail::None);
            let y = widgets::list(&mut c, t, f, top, &[warning]).bottom;
            widgets::section(&mut c, t, f, y, "分区")
        }
        None => widgets::section_first(&mut c, t, f, top, "分区"),
    };
    let bottom = match &s.mounts {
        Err(why) => {
            let y = widgets::list(&mut c, t, f, y, &[Row::new("读不到分区").trail(Trail::None)]).bottom;
            widgets::footnote(&mut c, t, f, y, why)
        }
        Ok(mounts) if mounts.is_empty() => widgets::list(&mut c, t, f, y, &[Row::new("没有可挂载的分区").trail(Trail::None)]).bottom,
        Ok(mounts) => {
            let subs: Vec<String> = mounts
                .iter()
                .map(|m| if m.holders.is_empty() { m.point.clone() } else { format!("{} · {} 正在使用", m.point, m.holders.join("、")) })
                .collect();
            let rows: Vec<Row> = mounts
                .iter()
                .zip(&subs)
                .map(|(m, sub)| {
                    let trail = if m.holders.is_empty() { Trail::Switch(m.mounted) } else { Trail::Text("使用中") };
                    Row::new(&m.base).subtitle(sub).trail(trail)
                })
                .collect();
            let list = widgets::list(&mut c, t, f, y, &rows);
            out.extend(list.rows.iter().zip(mounts).enumerate().map(|(i, (&rect, m))| {
                let action = match (m.holders.is_empty(), m.mounted, pending) {
                    (false, _, _) => Action::Unavailable("正在使用的分区不能卸载"),
                    (true, false, Some(note)) => Action::Unavailable(note),
                    _ => Action::MountToggle(i),
                };
                Hit { rect, action }
            }));
            let y = match &s.mount_failure {
                Some(failure) => widgets::footnote(&mut c, t, f, list.bottom, &fmt::mount_failure(failure)),
                None => list.bottom,
            };
            let about = match cx.info.slot {
                Some(slot) => format!("这里列出当前槽位 {} 在 super 里的分区，都以只读方式挂载。", slot.letter()),
                None => "这里列出 super 里的分区，都以只读方式挂载。".into(),
            };
            widgets::footnote(&mut c, t, f, y, &about)
        }
    };

    Frame { canvas: fit(c, t, bottom), hits: out, title: "挂载", back: true, header: true, content_top: top, hero: None, pattern: None }
}

/// About this device, modelled on the stock「我的设备」: the wordmark and
/// version over a moving colour field, and the summary, hardware and link
/// cards scrolling over it. Only the cards are drawn here; the canvas is
/// clear wherever the field shows.
pub fn about(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let m = &t.m;
    let tone = Tone::hero(t);
    let mut c = Canvas::new(m.w, m.h * 3);
    let mut out = Vec::new();

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
    let canvas = fit(c, t, links.bottom);
    let art = HeroArt {
        wordmark: widgets::wordmark(t, f, "Skyward", m.dp(233.7), m.dp(37.8)),
        version: widgets::version_line(t, f, VERSION),
        runs: canvas.runs(),
    };
    Frame { canvas, hits: out, title: "关于本机", back: true, header: false, content_top: summary.rows[0].y, hero: Some(art), pattern: None }
}

/// Every reading the recovery has, modelled on the stock「全部参数与信息」.
pub fn specs(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let gap = t.m.card_gap;
    let mut c = long_page(t);
    let mut out = Vec::new();
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
    Frame { canvas: fit(c, t, links.bottom), hits: out, title: "全部参数与信息", back: true, header: true, content_top: top, hero: None, pattern: None }
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
    Frame { canvas: c, hits: out, title: "解密", back: false, header: false, content_top: 0.0, hero: None, pattern: Some((pcx, pcy, span)) }
}

/// Reboot target picker with a swipe-to-confirm control.
pub fn reboot(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(t);
    let mut out = Vec::new();
    let top = t.m.content_top;
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
    let picks: Vec<Action> = TARGETS
        .iter()
        .enumerate()
        .map(|(k, target)| match target {
            Target::Fastboot => Action::Unavailable("用户空间 fastboot 还没接入"),
            _ => Action::RebootTarget(k),
        })
        .collect();
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
    Frame { canvas: c, hits: out, title: "重启", back: true, header: true, content_top: top, hero: None, pattern: None }
}

/// Installing, opened from the home page: the system on the phone now, and
/// where a package can come from.
pub fn install(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let mut c = page(t);
    let top = t.m.content_top;
    let mut facts = Vec::new();
    if let Some(v) = &i.os_version {
        facts.push(("当前系统", v.clone()));
    }
    if let Some(p) = &i.security_patch {
        facts.push(("安全补丁", fmt::patch(p)));
    }
    if let Some(s) = i.slot {
        facts.push(("当前槽位", format!("{}，系统更新会安装到 {}", s.letter(), s.other().letter())));
    }
    let y = if facts.is_empty() {
        widgets::section_first(&mut c, t, f, top, "安装方式")
    } else {
        let y = widgets::section_first(&mut c, t, f, top, "这台手机");
        let y = widgets::info_card(&mut c, t, f, y, &facts);
        widgets::section(&mut c, t, f, y, "安装方式")
    };
    let mut rows = Vec::new();
    if let Some(note) = fmt::snapshot_note(&i.snapshot) {
        rows.push(Row::new(fmt::snapshot(&i.snapshot)).icon(ORANGE, Icon::Layers).subtitle(note).trail(Trail::None));
    }
    rows.push(Row::new("通过电脑安装").icon(GREEN, Icon::Usb).subtitle("在电脑上使用 adb sideload 发送安装包"));
    let methods = widgets::list(&mut c, t, f, y, &rows);
    let out = vec![Hit { rect: *methods.rows.last().expect("the list holds the sideload row"), action: Action::Sideload }];
    widgets::footnote(
        &mut c,
        t,
        f,
        methods.bottom,
        "能安装完整的 A/B 系统更新包，以及 boot、init_boot、vendor_boot、recovery、dtbo、vbmeta 分区镜像。系统更新会安装到另一个槽位，安装完成后从那个槽位启动。",
    );
    Frame { canvas: c, hits: out, title: "安装", back: true, header: true, content_top: top, hero: None, pattern: None }
}

/// A one-line card on how things stand.
fn standing<'a>(title: &'a str, color: Rgba, icon: Icon, detail: &'a str) -> Row<'a> {
    Row::new(title).icon(color, icon).subtitle(detail).trail(Trail::None)
}

fn share(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64) as f32
    }
}

fn percent(share: f32) -> u32 {
    (share * 100.0).floor() as u32
}

/// Where an update stands, and the share of its step done when known.
fn update_stage(stage: Option<&Stage>) -> (String, Option<f32>) {
    match stage {
        Some(Stage::Writing { done, total }) => {
            let s = share(*done, *total);
            (format!("第 1 步，共 3 步：写入新系统，{}%", percent(s)), Some(s))
        }
        Some(Stage::Verifying { index, count, partition }) => {
            (format!("第 2 步，共 3 步：校验 {partition}（{}/{count}）", index + 1), Some(*index as f32 / *count as f32))
        }
        Some(Stage::Finishing) => ("第 3 步，共 3 步：收尾".into(), None),
        Some(Stage::Flashing { .. }) | None => ("正在准备".into(), None),
    }
}

fn pick_title(pick: Pick, current: Option<Slot>) -> String {
    let letter = |s: Option<Slot>| s.map(|s| format!(" {}", s.letter())).unwrap_or_default();
    match pick {
        Pick::Current => format!("当前槽位{}", letter(current)),
        Pick::Other => format!("另一个槽位{}", letter(current.map(Slot::other))),
        Pick::Both => "两个槽位".into(),
    }
}

/// Receiving what the computer sends with `adb sideload`, and installing
/// it, step by step.
pub fn sideload(cx: &Ctx) -> Frame {
    let (t, f, s, i) = (cx.t, cx.f, cx.state, cx.info);
    let mut c = long_page(t);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let bottom = match &s.install {
        Install::Idle | Install::Starting | Install::Ending => {
            let row = standing("正在准备 USB", GREEN, Icon::Usb, "USB 正在从 adb 调试切换到 sideload 模式");
            widgets::list(&mut c, t, f, top, &[row]).bottom
        }
        Install::Waiting => {
            let row = standing("等待电脑发送", GREEN, Icon::Usb, "用数据线把手机连接到电脑，再在电脑上运行下面的命令");
            let y = widgets::list(&mut c, t, f, top, &[row]).bottom;
            let y = widgets::section(&mut c, t, f, y, "在电脑上运行");
            let y = widgets::info_card(&mut c, t, f, y, &[(
                "adb sideload 文件名",
                "文件名替换成需要安装的系统更新包或分区镜像，比如 boot.img".into(),
            )]);
            widgets::footnote(&mut c, t, f, y, "电脑上需要安装 Android SDK Platform-Tools。返回会停止等待，USB 切换回 adb 调试。")
        }
        Install::Checking { size } => {
            let detail = format!("{}，正在识别和检查", fmt::bytes(*size));
            let y = widgets::list(&mut c, t, f, top, &[standing("正在读取安装包", GREEN, Icon::Install, &detail)]).bottom;
            widgets::footnote(&mut c, t, f, y, "读取时不要断开数据线。")
        }
        Install::Choosing(choice) => {
            let (name, holds) = fmt::image_kind(choice.kind);
            let detail = format!("{}，{holds}", fmt::bytes(choice.size));
            let y = widgets::list(&mut c, t, f, top, &[standing(name, GREEN, Icon::Install, &detail)]).bottom;
            let y = widgets::section(&mut c, t, f, y, "写入分区");
            let rows: Vec<Row> = choice
                .partitions
                .iter()
                .enumerate()
                .map(|(k, p)| Row::new(p).subtitle(fmt::partition_role(p)).trail(Trail::Check(k == choice.partition)))
                .collect();
            let parts = widgets::list(&mut c, t, f, y, &rows);
            let choose: Vec<Action> = (0..choice.partitions.len()).map(Action::FlashPartition).collect();
            out.extend(hits(&parts.rows, &choose));
            let y = widgets::section(&mut c, t, f, parts.bottom, "槽位");
            let titles = Pick::ALL.map(|p| pick_title(p, i.slot));
            let rows: Vec<Row> =
                Pick::ALL.iter().zip(&titles).map(|(p, title)| Row::new(title).trail(Trail::Check(*p == choice.pick))).collect();
            let picks = widgets::list(&mut c, t, f, y, &rows);
            out.extend(hits(&picks.rows, &Pick::ALL.map(Action::FlashSlots)));
            let (label, danger) = confirm_label(Confirm::Flash);
            let track = widgets::confirm(&mut c, t, f, picks.bottom + t.m.dp(28.0), label, danger, s.confirm);
            out.push(Hit { rect: track, action: Action::Confirm(Confirm::Flash) });
            widgets::footnote(&mut c, t, f, track.y + track.h, "刷入会覆盖分区里原来的内容。返回会取消，电脑上的 adb sideload 随之结束。")
        }
        Install::Updating { version, stage, overwriting } => {
            let title = match version {
                Some(v) => format!("正在安装 {v}"),
                None => "正在安装系统更新".into(),
            };
            let (detail, done) = update_stage(stage.as_ref());
            let y = widgets::progress_card(&mut c, t, f, top, &title, &detail, done);
            let target = i.slot.map(|s| format!("新系统安装到槽位 {}。", s.other().letter())).unwrap_or_default();
            let y = widgets::footnote(&mut c, t, f, y, &format!("安装完成前不要断开数据线，也不要关机。{target}"));
            if *overwriting {
                widgets::footnote(&mut c, t, f, y, "super 分区放不下两套系统，当前系统正在被覆盖，安装完成前重启会无法进入系统。")
            } else {
                y
            }
        }
        Install::Flashing { partition, stage } => {
            let (detail, done) = match stage {
                Some(Stage::Flashing { done, total }) => {
                    let s = share(*done, *total);
                    let what = if done * 2 < *total { "正在写入" } else { "正在核对写入的内容" };
                    (format!("{what}，{}%", percent(s)), Some(s))
                }
                _ => ("正在准备".into(), None),
            };
            let y = widgets::progress_card(&mut c, t, f, top, &format!("正在刷入 {partition}"), &detail, done);
            widgets::footnote(&mut c, t, f, y, "刷入完成前不要断开数据线。")
        }
        Install::Finished { outcome, damage } => finished(&mut c, cx, top, outcome, *damage, &mut out),
    };
    Frame { canvas: fit(c, t, bottom), hits: out, title: "通过电脑安装", back: true, header: true, content_top: top, hero: None, pattern: None }
}

/// How an install ended, and where to go from there.
fn finished(c: &mut Canvas, cx: &Ctx, top: f32, outcome: &Outcome, damage: Option<Damage>, out: &mut Vec<Hit>) -> f32 {
    let (t, f) = (cx.t, cx.f);
    let gap = t.m.card_gap;
    let again = Row::new("再发送一个").icon(GREEN, Icon::Usb);
    let restart = Row::new("重启到系统").icon(ORANGE, Icon::Power);
    match outcome {
        Outcome::Installed { slot } => {
            let detail = format!("新系统安装在槽位 {}，重启后从这个槽位启动。", slot.letter());
            let y = widgets::list(c, t, f, top, &[standing("安装完成", GREEN, Icon::Install, &detail)]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[restart]);
            out.extend(hits(&next.rows, &[Action::RebootTo(Target::System)]));
            next.bottom
        }
        Outcome::Flashed { partition, slots } => {
            let detail = format!("{partition} 已写入{}。", fmt::slots(slots));
            let y = widgets::list(c, t, f, top, &[standing("刷入完成", GREEN, Icon::Install, &detail)]).bottom;
            let mut rows = vec![restart];
            let mut acts = vec![Action::RebootTo(Target::System)];
            if *partition == "recovery" {
                rows.push(Row::new("重启到恢复模式").icon(ORANGE, Icon::Power));
                acts.push(Action::RebootTo(Target::Recovery));
            }
            rows.push(again);
            acts.push(Action::Sideload);
            let next = widgets::list(c, t, f, y + gap, &rows);
            out.extend(hits(&next.rows, &acts));
            next.bottom
        }
        Outcome::Cancelled => {
            let y = widgets::list(c, t, f, top, &[standing("已取消", SLATE, Icon::Install, "没有写入任何内容。")]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[again]);
            out.extend(hits(&next.rows, &[Action::Sideload]));
            next.bottom
        }
        Outcome::Refused(r) => {
            let reason = fmt::refusal(r, &cx.info.codename);
            let y = widgets::list(c, t, f, top, &[standing("无法安装", ORANGE, Icon::Install, &reason)]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[again]);
            out.extend(hits(&next.rows, &[Action::Sideload]));
            widgets::footnote(c, t, f, next.bottom, "没有写入任何内容。")
        }
        Outcome::Failed(failure) => {
            let (what, detail) = fmt::failure(failure);
            let y = widgets::list(c, t, f, top, &[standing("安装失败", RED, Icon::Install, &what)]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[again, restart]);
            out.extend(hits(&next.rows, &[Action::Sideload, Action::RebootTo(Target::System)]));
            let warning = match damage {
                Some(Damage::System) => {
                    "当前系统已经开始被覆盖，现在重启会无法进入系统。请再发送一次完整的更新包，安装完成后再重启。".to_string()
                }
                Some(Damage::Partition(p)) => format!("{p} 只写入了一部分，现在重启可能出问题。请再发送一次镜像，刷入完成后再重启。"),
                None => "当前系统没有受到影响。".into(),
            };
            let mut y = widgets::footnote(c, t, f, next.bottom, &warning);
            if let Some(d) = detail {
                y = widgets::footnote(c, t, f, y, &d);
            }
            y
        }
    }
}

/// Destructive wipe with the partitions to clear.
pub fn wipe(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(t);
    let mut out = Vec::new();
    let top = t.m.content_top;
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
    Frame { canvas: c, hits: out, title: "清除数据", back: true, header: true, content_top: top, hero: None, pattern: None }
}
