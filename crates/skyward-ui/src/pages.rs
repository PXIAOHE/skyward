use crate::app::{
    Action, BackupAt, BackupEnd, BackupJob, ClashChoice, Confirm, Damage, Data, Dock, EraseStop, Field, FileKind, FileOp, FileTask,
    Folder, Frame, HeroArt, Hit, Install, JobEnd, Keys, Pick, Place, Rows, Screen, Source, State, Switch, Target, UsbDrive, UsbLink, Wipe,
    ADDABLE, BACKUP_PARTS, FASTBOOT_CHOICES, SCREEN_OFF, TARGETS, TIME_ZONES, WIPES,
};
use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::editor;
use crate::files;
use crate::fmt;
use crate::font::Fonts;
use crate::geom::Rect;
use crate::search;
use crate::term::{self, Term};
use crate::theme::Theme;
use crate::widgets::{self, Detail, Fact, Icon, Outline, Row, Spec, Tone, Trail};
use skyward_info::{Charge, DeviceInfo, VERSION};
use skyward_install::{Outcome, Reflash, Stage};
use skyward_net::supplicant::Security;
use skyward_net::wlan::{Bands, Network, Power, Stage as Joining};
use skyward_vab::{Slot, Standing};
use std::path::Path;

// Tile colours sampled from the stock Settings, plus orange and red from the
// same family for the destructive and reboot entries.
const BLUE: Rgba = Rgba::rgb(74, 128, 247);
const GREEN: Rgba = Rgba::rgb(97, 202, 81);
const PURPLE: Rgba = Rgba::rgb(116, 104, 240);
const SLATE: Rgba = Rgba::rgb(143, 156, 174);
const ORANGE: Rgba = Rgba::rgb(255, 146, 46);
const RED: Rgba = Rgba::rgb(242, 83, 75);

/// What every page draws from: style, typefaces, the device readings, and
/// what the pages hold, from the choices made on them to where the data
/// partition stands.
pub struct Ctx<'a> {
    pub t: &'a Theme,
    pub f: &'a Fonts,
    pub info: &'a DeviceInfo,
    pub state: &'a State,
    /// The shell the terminal page shows, once one has started.
    pub term: Option<&'a Term>,
    /// The part of the page to draw; all of it if none is given. The page
    /// is laid out whole either way.
    pub band: Option<Band>,
}

/// Rows `rows` of a page, and of them the columns `cols`, each from the
/// first to one past the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    pub rows: (u32, u32),
    pub cols: (u32, u32),
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
        Screen::Choose(place) => choose(cx, place),
        Screen::Installing => installing(cx),
        Screen::About => about(cx),
        Screen::Specs => specs(cx),
        Screen::ScreenOff => screen_off(cx),
        Screen::Search => search(cx),
        Screen::Backup => backup(cx),
        Screen::Restore => restore(cx),
        Screen::RestoreBackup => restore_backup(cx),
        Screen::BackupJob => backup_job(cx),
        Screen::Advanced => advanced(cx),
        Screen::Files => files(cx),
        Screen::FileInfo => file_info(cx),
        Screen::Permissions => permissions(cx),
        Screen::Editor => editor(cx),
        Screen::Terminal => terminal(cx),
        Screen::Relabel => relabel(cx),
        Screen::Language => language(cx),
        Screen::TimeZone => time_zone(cx),
        Screen::Logs => logs(cx),
        Screen::Update => update(cx),
        Screen::License => license(cx),
        Screen::FontLicense => font_license(cx),
        Screen::Slot => slot(cx),
        Screen::Fastboot => fastboot(cx),
        Screen::Erase => erase(cx),
        Screen::CantStart => cant_start(cx),
        Screen::Unhandled => unhandled(cx),
        Screen::Rename => rename(cx),
        Screen::Clash => clash(cx),
        Screen::FileJob => file_job(cx),
        Screen::Wlan => wlan(cx),
        Screen::WlanPassword => wlan_password(cx),
        Screen::WlanNetwork => wlan_network(cx),
        Screen::WlanAdd => wlan_add(cx),
    }
}

/// Label and colour of each swipe-to-confirm control.
pub fn confirm_label(which: Confirm) -> (&'static str, bool) {
    match which {
        Confirm::Reboot => ("滑动以重启", false),
        Confirm::Wipe => ("滑动以清除", true),
        Confirm::Flash => ("滑动以刷入", true),
        Confirm::Backup => ("滑动以备份", false),
        Confirm::Restore => ("滑动以恢复", true),
        Confirm::Slot => ("滑动以切换", true),
        Confirm::FactoryReset => ("滑动以恢复出厂设置", true),
        Confirm::DeleteEntry | Confirm::DeleteBackup => ("滑动以删除", true),
        Confirm::Install => ("滑动以安装", false),
        Confirm::InstallWipe => ("滑动以安装并清除", true),
        Confirm::RunScript => ("滑动以安装", true),
        Confirm::Merge => ("滑动以合并", true),
        Confirm::Relabel => ("滑动以修复", true),
    }
}

/// The home page's large title, and what its search bar and the search
/// page's field say while nothing is typed.
pub const HOME_TITLE: &str = "Skyward";
pub const SEARCH_HINT: &str = "搜索功能与分区";

const REBOOT_TARGETS: [(&str, Option<&str>); 6] = [
    ("系统", None),
    ("恢复模式", None),
    ("Fastboot", Some("用户空间 fastbootd")),
    ("Bootloader", None),
    ("EDL", Some("高通紧急下载模式")),
    ("关机", None),
];

/// A canvas for a page `h` tall in the colour `bg`, holding the rows `cx`
/// asks to be drawn.
fn sheet(cx: &Ctx, h: u32, bg: Rgba) -> Canvas {
    let w = cx.t.m.w;
    let Band { rows, cols } = cx.band.unwrap_or(Band { rows: (0, h), cols: (0, w) });
    Canvas::band(w, h, rows.0..rows.1, cols.0..cols.1, bg)
}

fn page(cx: &Ctx) -> Canvas {
    sheet(cx, cx.t.m.h, cx.t.pal.bg)
}

/// A canvas tall enough for a scrolling page; [`fit`] trims it to the content.
fn long_page(cx: &Ctx) -> Canvas {
    sheet(cx, cx.t.m.h * 3, cx.t.pal.bg)
}

/// How tall a long page whose content ends at `bottom` is: to the bottom
/// margin after it, and never shorter than one screen.
fn fitted(t: &Theme, bottom: f32) -> u32 {
    ((bottom + t.m.margin * 2.0).ceil() as u32).max(t.m.h)
}

/// Trim a long page to its content plus the bottom margin, and never shorter
/// than one screen.
fn fit(c: Canvas, t: &Theme, bottom: f32) -> Canvas {
    c.crop_height(fitted(t, bottom))
}

fn hits(rects: &[Rect], actions: &[Action]) -> Vec<Hit> {
    rects.iter().zip(actions).map(|(&rect, &action)| Hit { rect, action }).collect()
}

/// Recovery home, laid out like the Settings root: title, search, a device
/// card with the readings that matter before flashing, then grouped entries.
pub fn home(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let mut y = t.m.content_top;
    let search_top = y;
    y = widgets::search_bar(&mut c, t, f, y, SEARCH_HINT);
    out.push(Hit { rect: Rect::new(t.m.margin, search_top, t.m.w as f32 - t.m.margin * 2.0, t.m.search_h), action: Action::Open(Screen::Search) });

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
    stats.push((fmt::data(&cx.state.data), "data 分区"));
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
        rows.push(Row::new(fmt::snapshot(&i.snapshot)).icon(ORANGE, Icon::Layers).subtitle(note).trail(Trail::Arrow));
    }
    let card = widgets::device_card(&mut c, t, f, y, &i.model, &sub, &stats, &rows);
    out.push(Hit { rect: card.rows[0], action: Action::Open(Screen::About) });
    if let Some(&rect) = card.rows.get(1) {
        out.push(Hit { rect, action: Action::Open(Screen::Update) });
    }
    y = card.bottom + t.m.card_gap;
    // On a phone whose WLAN the recovery brings up, the WLAN comes first
    // under the device, as on the stock settings home.
    if let Some(w) = &cx.state.wlan {
        let wlan = widgets::list(&mut c, t, f, y, &[Row::new("WLAN").icon(BLUE, Icon::Wlan).trail(Trail::Value(fmt::wlan_value(w)))]);
        out.extend(hits(&wlan.rows, &[Action::Open(Screen::Wlan)]));
        y = wlan.bottom + t.m.card_gap;
    }
    let tools = widgets::list(&mut c, t, f, y, &[
        Row::new("安装").icon(GREEN, Icon::Install),
        Row::new("备份").icon(BLUE, Icon::Backup),
        Row::new("恢复").icon(PURPLE, Icon::Restore),
        Row::new("挂载").icon(SLATE, Icon::Mount),
        Row::new("解密").icon(BLUE, Icon::Lock).trail(Trail::Value(fmt::data(&cx.state.data))),
    ]);
    out.extend(hits(&tools.rows, &[
        Action::Open(Screen::Install),
        Action::Open(Screen::Backup),
        Action::Open(Screen::Restore),
        Action::Open(Screen::Mount),
        Action::Open(Screen::Decrypt),
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
        Action::Open(Screen::Advanced),
        Action::Open(Screen::Settings),
        Action::Open(Screen::Reboot),
    ]));
    Frame { back: false, ..Frame::new(t, fit(c, t, system.bottom), out, HOME_TITLE, search_top) }
}

/// Preferences, opening with the stock「显示与亮度」theme previews.
pub fn settings(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(cx);
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
        Row::new("时区").icon(ORANGE, Icon::Clock).trail(Trail::Value(TIME_ZONES[s.time_zone].0)),
        Row::new("触摸振动").icon(PURPLE, Icon::Vibrate).trail(Trail::Switch(s.vibrate)),
    ]);
    out.extend(hits(&general.rows, &[
        Action::Open(Screen::Language),
        Action::Open(Screen::TimeZone),
        Action::Toggle(Switch::Vibrate),
    ]));
    y = widgets::section(&mut c, t, f, general.bottom, "USB");
    let usb = widgets::list(&mut c, t, f, y, &[Row::new("文件传输").icon(BLUE, Icon::Usb).trail(Trail::Switch(s.file_transfer))]);
    out.extend(hits(&usb.rows, &[Action::Toggle(Switch::FileTransfer)]));
    y = widgets::footnote(&mut c, t, f, usb.bottom, "开启后，可在电脑上管理内部存储中的文件，需先完成「解密」。");
    y = widgets::section(&mut c, t, f, y, "调试");
    let debug = widgets::list(&mut c, t, f, y, &[Row::new("终端").icon(SLATE, Icon::Terminal)]);
    out.extend(hits(&debug.rows, &[Action::Open(Screen::Terminal)]));
    y = widgets::section(&mut c, t, f, debug.bottom, "关于");
    let about = widgets::list(&mut c, t, f, y, &[Row::new("关于本机").trail(Trail::Value(VERSION))]);
    out.extend(hits(&about.rows, &[Action::Open(Screen::About)]));
    Frame::new(t, fit(c, t, about.bottom), out, "设置", top)
}

/// How long the screen stays on with nothing touched, opened from the
/// settings.
pub fn screen_off(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let top = t.m.content_top;
    let labels = SCREEN_OFF.map(fmt::screen_off);
    let rows: Vec<Row> = labels.iter().enumerate().map(|(k, label)| Row::new(label).trail(Trail::Check(s.screen_off == k))).collect();
    let times = widgets::list(&mut c, t, f, top, &rows);
    let picks: Vec<Action> = (0..SCREEN_OFF.len()).map(Action::ScreenOff).collect();
    let y = widgets::footnote(&mut c, t, f, times.bottom, "超过所选时间未操作，屏幕将变暗并熄灭，轻触屏幕或按任意键即可点亮。");
    if SCREEN_OFF[s.screen_off].is_none() {
        widgets::footnote(&mut c, t, f, y, "屏幕将保持常亮，长时间显示同一画面可能留下残影。");
    }
    Frame::new(t, c, hits(&times.rows, &picks), "息屏时间", top)
}

/// Mount manager: the slot's partitions, each with a switch to mount it
/// read-only or unmount it; one that programs are using says which, and
/// stays.
pub fn mount(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;

    let y = widgets::section_first(&mut c, t, f, top, "分区");
    let bottom = match &s.mounts {
        Err(why) => {
            let y = widgets::list(&mut c, t, f, y, &[Row::new("无法读取分区").trail(Trail::None)]).bottom;
            widgets::footnote(&mut c, t, f, y, why)
        }
        Ok(mounts) if mounts.is_empty() => widgets::list(&mut c, t, f, y, &[Row::new("暂无可挂载的分区").trail(Trail::None)]).bottom,
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
                let action = if m.holders.is_empty() { Action::MountToggle(i) } else { Action::Unavailable("无法卸载正在使用的分区") };
                Hit { rect, action }
            }));
            let y = match &s.mount_failure {
                Some(failure) => widgets::footnote(&mut c, t, f, list.bottom, &fmt::mount_failure(failure)),
                None => list.bottom,
            };
            let about = match cx.info.slot {
                Some(slot) => format!("仅显示当前槽位 {} 在 super 中的分区，挂载后均为只读。", slot.letter()),
                None => "仅显示 super 中的分区，挂载后均为只读。".into(),
            };
            widgets::footnote(&mut c, t, f, y, &about)
        }
    };

    Frame::new(t, fit(c, t, bottom), out, "挂载", top)
}

/// About this device, modelled on the stock「我的设备」: the wordmark and
/// version over a moving colour field, and the summary, hardware and link
/// cards scrolling over it. Only the cards are drawn here; the canvas is
/// clear wherever the field shows.
pub fn about(cx: &Ctx) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let m = &t.m;
    // The page is drawn whole, under the field: laid out first to learn
    // how tall it comes to, then drawn that tall.
    let (_, _, bottom) = about_cards(cx, &mut Canvas::band(m.w, m.h * 3, 0..0, 0..0, Rgba { r: 0, g: 0, b: 0, a: 0 }));
    let mut c = Canvas::new(m.w, fitted(t, bottom));
    let (out, summary_top, bottom) = about_cards(cx, &mut c);
    let canvas = fit(c, t, bottom);
    let art = HeroArt {
        wordmark: widgets::wordmark(t, f, "Skyward", m.dp(233.7), m.dp(37.8)),
        version: widgets::version_line(t, f, VERSION),
        runs: canvas.runs(),
    };
    Frame { header: false, hero: Some(art), ..Frame::new(t, canvas, out, "关于本机", summary_top) }
}

/// The About page's cards on `c`: what touching them does, where the
/// first starts, and where the last ends.
fn about_cards(cx: &Ctx, c: &mut Canvas) -> (Vec<Hit>, f32, f32) {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let m = &t.m;
    let tone = Tone::hero(t);
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
    let summary = widgets::facts(c, t, f, m.dp(490.7), &facts, &tone);
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
    let y = widgets::spec_card(c, t, f, summary.bottom + m.card_gap, &fmt::model_lines(&i.model), &specs, &tone);
    let links = widgets::list_toned(c, t, f, y + m.card_gap, &[Row::new("全部参数与信息"), Row::new("导出日志")], &tone);
    out.extend(hits(&links.rows, &[Action::Open(Screen::Specs), Action::Open(Screen::Logs)]));
    (out, summary.rows[0].y, links.bottom)
}

/// Every reading the recovery has, modelled on the stock「全部参数与信息」.
pub fn specs(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let gap = t.m.card_gap;
    let mut c = long_page(cx);
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
    boot.push(("data 分区", fmt::data(&cx.state.data).to_string()));
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
        None => format!("总存储 {} · data 分区未挂载", fmt::capacity(i.storage.total)),
    };
    y = widgets::info_card(&mut c, t, f, y + gap, &[("机身存储", storage)]);
    let links = widgets::list(&mut c, t, f, y + gap, &[
        Row::new("开源许可").trail(Trail::Value("GPL-3.0")),
        Row::new("界面字体").trail(Trail::Value("MiSans")),
    ]);
    out.extend(hits(&links.rows, &[Action::Open(Screen::License), Action::Open(Screen::FontLicense)]));
    Frame::new(t, fit(c, t, links.bottom), out, "全部参数与信息", top)
}

/// The data partition and its users' own files. The page opens the
/// partition as it shows, which is how it learns each user's screen lock;
/// a user whose files wait on one is asked with the control that lock is
/// answered with, from the start.
pub fn decrypt(cx: &Ctx) -> Frame {
    let c = page(cx);
    match &cx.state.data {
        Data::Locked { user, keys, checking } => code(cx, c, *user, *keys, *checking),
        Data::Closed | Data::Opening => data_status(cx, c, Icon::Lock, "解密 data 分区", Some("正在读取锁屏方式"), None),
        Data::Open => data_status(cx, c, Icon::Folder, "data 分区已解密", Some("你可以在「文件管理」中查看本机文件"), None),
        Data::Failed(why) => data_status(cx, c, Icon::Lock, "未能打开 data 分区", None, Some(why)),
        Data::Erased => data_status(cx, c, Icon::Wipe, "data 分区已清除", Some("下次开机时将重新创建"), None),
    }
}

/// Where the partition stands, under its sign: a line on it, or why it
/// did not open and a way to try again; then the way off the page.
fn data_status(cx: &Ctx, mut c: Canvas, icon: Icon, title: &str, note: Option<&str>, why: Option<&str>) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let m = &t.m;
    let x = m.w as f32 / 2.0;
    let sign_cy = m.dp(150.0);
    c.fill_circle(x, sign_cy, m.dp(30.0), t.pal.card);
    widgets::glyph(&mut c, icon, x, sign_cy, m.dp(40.0), t.pal.accent, t.pal.card);
    c.text_center(&f.heading, title, x, sign_cy + m.dp(64.0), m.dp(24.0), t.pal.text);
    if let Some(note) = note {
        c.text_center(&f.body, note, x, sign_cy + m.dp(98.0), m.sub_px, t.pal.subtitle);
    }
    let mut out = Vec::new();
    if let Some(why) = why {
        widgets::paragraph(&mut c, t, f, sign_cy + m.dp(88.0), why);
        let again = widgets::button(&mut c, t, f, m.h as f32 * 0.62, "重试");
        out.push(Hit { rect: again, action: Action::OpenData });
    }
    let leave_cy = m.h as f32 - m.dp(64.0);
    let leave = if matches!(cx.state.data, Data::Open | Data::Erased) { "完成" } else { "暂不解密" };
    c.text_center(&f.heading, leave, x, leave_cy, m.row_title_px, t.pal.accent);
    out.push(Hit { rect: Rect::new(x - m.dp(80.0), leave_cy - m.dp(24.0), m.dp(160.0), m.dp(48.0)), action: Action::Skip });
    Frame { header: false, ..Frame::new(t, c, out, "解密", 0.0) }
}

/// A user's own files wait on their screen lock: the control it is
/// answered with, which takes nothing while what was entered is checked.
fn code(cx: &Ctx, mut c: Canvas, user: u32, keys: Keys, checking: bool) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let m = &t.m;
    let x = m.w as f32 / 2.0;
    let mut out = Vec::new();
    let (what, lock) = if keys == Keys::Pattern { ("绘制", "锁屏图案") } else { ("输入", "锁屏密码") };
    let title = match user {
        0 => format!("{what}{lock}"),
        other => format!("{what}用户 {other} 的{lock}"),
    };
    let note = if checking { "正在验证" } else { "解密后可查看本机文件" };
    c.text_center(&f.heading, &title, x, m.dp(130.0), m.dp(24.0), t.pal.text);
    c.text_center(&f.body, note, x, m.dp(166.0), m.sub_px, t.pal.subtitle);
    let mut pattern = None;
    let bottom = match keys {
        Keys::Letters => {
            let top = m.h as f32 - widgets::keyboard_height(t);
            let caps = widgets::keyboard(&mut c, t, f, top, s.layer, s.key_held(), "解密");
            if !checking {
                out.extend(caps.into_iter().map(|(rect, cap)| Hit { rect, action: Action::Type(cap) }));
            }
            top
        }
        Keys::Pattern => {
            let span = m.dp(300.0);
            let cy = m.dp(300.0) + span / 2.0;
            widgets::pattern_lock(&mut c, t, x, cy, span, &s.pattern);
            if !checking {
                out.push(Hit { rect: Rect::new(x - span, cy - span, span * 2.0, span * 2.0), action: Action::Pattern });
                pattern = Some((x, cy, span));
            }
            m.h as f32 - m.dp(26.0)
        }
        Keys::Digits => {
            let top = m.h as f32 - widgets::keypad_height(t);
            let caps = widgets::keypad(&mut c, t, f, top, s.key_held());
            if !checking {
                out.extend(caps.into_iter().map(|(rect, cap)| Hit { rect, action: Action::Type(cap) }));
            }
            top
        }
    };
    // The dots sit just above whatever types into them; a pattern is its
    // own showing of what has been drawn.
    if keys != Keys::Pattern {
        widgets::code_dots(&mut c, t, x, bottom - m.dp(92.0), s.code.chars().count());
    }
    let skip_cy = bottom - m.dp(34.0);
    c.text_center(&f.heading, "暂不解密", x, skip_cy, m.row_title_px, t.pal.accent);
    out.push(Hit { rect: Rect::new(x - m.dp(80.0), skip_cy - m.dp(24.0), m.dp(160.0), m.dp(48.0)), action: Action::Skip });
    Frame { header: false, pattern, ..Frame::new(t, c, out, "解密", 0.0) }
}

/// Reboot target picker with a swipe-to-confirm control.
pub fn reboot(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
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
    let picks: Vec<Action> = (0..TARGETS.len()).map(Action::RebootTarget).collect();
    out.extend(hits(&targets.rows, &picks));
    y = targets.bottom;
    if let Some(slot) = cx.info.slot {
        let next = match &s.boot_slots {
            Some(Ok(slots)) if slots.active == slot => format!("{}（当前）", slot.letter()),
            Some(Ok(slots)) => slots.active.letter().to_string(),
            Some(Err(_)) => "无法读取".to_string(),
            None => String::new(),
        };
        y = widgets::section(&mut c, t, f, y, "槽位");
        let slots = widgets::list(&mut c, t, f, y, &[Row::new("下次启动").trail(Trail::Value(&next))]);
        out.extend(hits(&slots.rows, &[Action::Open(Screen::Slot)]));
        y = slots.bottom;
    }
    let (label, danger) = confirm_label(Confirm::Reboot);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(28.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::Reboot) });
    Frame::new(t, c, out, "重启", top)
}

/// Installing, opened from the home page: the system on the phone now, and
/// where a package can come from.
pub fn install(cx: &Ctx) -> Frame {
    let (t, f, i) = (cx.t, cx.f, cx.info);
    let mut c = long_page(cx);
    let top = t.m.content_top;
    let mut facts = Vec::new();
    if let Some(v) = &i.os_version {
        facts.push(("当前系统", v.clone()));
    }
    if let Some(p) = &i.security_patch {
        facts.push(("安全补丁", fmt::patch(p)));
    }
    if let Some(s) = i.slot {
        facts.push(("当前槽位", format!("{}，系统更新将安装到槽位 {}", s.letter(), s.other().letter())));
    }
    let y = if facts.is_empty() {
        widgets::section_first(&mut c, t, f, top, "安装方式")
    } else {
        let y = widgets::section_first(&mut c, t, f, top, "本机");
        let y = widgets::info_card(&mut c, t, f, y, &facts);
        widgets::section(&mut c, t, f, y, "安装方式")
    };
    let mut rows = Vec::new();
    if let Some(note) = fmt::snapshot_note(&i.snapshot) {
        rows.push(Row::new(fmt::snapshot(&i.snapshot)).icon(ORANGE, Icon::Layers).subtitle(note).trail(Trail::None));
    }
    // Internal storage opens with the data partition, which the decrypt
    // page opens or tells why it does not.
    let data = &cx.state.data;
    let (storage, from_storage) = if data.storage_open() {
        ("选择内部存储中的更新包或镜像", Action::Open(Screen::Choose(Place::Storage)))
    } else if *data == Data::Erased {
        ("data 分区已清除", Action::Open(Screen::Decrypt))
    } else {
        ("需先完成「解密」", Action::Open(Screen::Decrypt))
    };
    // The drive's page waits for one to be plugged in, and says why one
    // that is cannot be read.
    let usb = match &cx.state.usb_drive {
        Some(UsbDrive::Shown) => "选择 U 盘中的更新包或镜像",
        Some(UsbDrive::Unreadable(_)) => "无法读取该 U 盘",
        None => "请通过 OTG 连接 U 盘",
    };
    rows.push(Row::new(install_title(Source::File(Place::Storage))).icon(BLUE, Icon::Phone).subtitle(storage));
    rows.push(Row::new(install_title(Source::File(Place::UsbDrive))).icon(PURPLE, Icon::UsbDrive).subtitle(usb));
    rows.push(Row::new(install_title(Source::Computer)).icon(GREEN, Icon::Usb).subtitle("在电脑上运行 adb sideload"));
    let methods = widgets::list(&mut c, t, f, y, &rows);
    let from_usb = Action::Open(Screen::Choose(Place::UsbDrive));
    let mut out = hits(&methods.rows[methods.rows.len() - 3..], &[from_storage, from_usb, Action::Sideload]);
    let y = widgets::footnote(
        &mut c,
        t,
        f,
        methods.bottom,
        "支持安装 A/B 完整更新包和带安装脚本的刷机包，以及刷入 boot、init_boot、vendor_boot、recovery、dtbo、vbmeta 分区镜像。系统更新将安装到另一个槽位，重启后从该槽位启动。选择文件时仅列出文件夹以及 .zip、.img 文件。",
    );
    let y = widgets::section(&mut c, t, f, y, "选项");
    let signature = widgets::list(&mut c, t, f, y, &[Row::new("验证刷机包签名").trail(Trail::Switch(cx.state.check_signature))]);
    out.extend(hits(&signature.rows, &[Action::Toggle(Switch::Signature)]));
    let mut y = widgets::footnote(&mut c, t, f, signature.bottom, "开启后，仅安装带有本机官方签名的刷机包。");
    if cx.state.recovery_partitions {
        let row = Row::new("自动重新刷入 Skyward").trail(Trail::Switch(cx.state.keep_recovery));
        let options = widgets::list(&mut c, t, f, y + t.m.card_gap, &[row]);
        out.extend(hits(&options.rows, &[Action::Toggle(Switch::KeepRecovery)]));
        y = widgets::footnote(&mut c, t, f, options.bottom, "开启后，安装系统更新或刷机包时被覆盖的 recovery 分区将重新刷入 Skyward。");
    }
    Frame::new(t, fit(c, t, y), out, "安装", top)
}

/// How installing from `source` is named: on the install page, and as the
/// title of the page that installs.
fn install_title(source: Source) -> &'static str {
    match source {
        Source::Computer => "通过电脑安装",
        Source::File(Place::Storage) => "从内部存储安装",
        Source::File(Place::UsbDrive) => "从 U 盘安装",
    }
}

/// What a place is called at the top of its folders.
fn place_name(place: Place) -> &'static str {
    match place {
        Place::Storage => "内部存储",
        Place::UsbDrive => "U 盘",
    }
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
            (format!("第 1 步，共 3 步：写入新系统 {}%", percent(s)), Some(s))
        }
        Some(Stage::Verifying { index, count, partition }) => {
            (format!("第 2 步，共 3 步：校验 {partition}（{}/{count}）", index + 1), Some(*index as f32 / *count as f32))
        }
        Some(Stage::Finishing) => ("第 3 步，共 3 步：完成安装".into(), None),
        Some(Stage::Reflashing { done, total }) => (REFLASHING.into(), Some(*done as f32 / *total as f32)),
        Some(Stage::Flashing { .. } | Stage::Script { .. } | Stage::Signature { .. }) | None => ("正在准备".into(), None),
    }
}

const REFLASHING: &str = "正在重新刷入 Skyward";

/// What the card says while an install script runs, and how far it is: the
/// last line the script showed and how far it says it is, then Skyward
/// going back into the recovery partitions it wrote over.
fn script_stage<'a>(stage: Option<&Stage>, said: &'a [String]) -> (&'a str, Option<f32>) {
    let last = said.iter().rev().map(|line| line.trim()).find(|line| !line.is_empty()).unwrap_or("正在运行安装脚本");
    match stage {
        Some(Stage::Reflashing { done, total }) => (REFLASHING, Some(*done as f32 / *total as f32)),
        Some(Stage::Script { thousandths }) => (last, Some(f32::from(*thousandths) / 1000.0)),
        _ => (last, None),
    }
}

/// How Skyward went back into the recovery partitions the install wrote
/// over, where it did.
fn reflashed(c: &mut Canvas, cx: &Ctx, y: f32) -> f32 {
    let (t, f) = (cx.t, cx.f);
    match &cx.state.reflash {
        None => y,
        Some(Reflash::Written(slots)) => {
            widgets::footnote(c, t, f, y, &format!("已将 Skyward 重新刷入{} 的 recovery 分区。", fmt::slots(slots)))
        }
        Some(Reflash::Failed(why)) => {
            let y = widgets::footnote(c, t, f, y, "未能将 Skyward 重新刷入 recovery 分区。");
            widgets::footnote(c, t, f, y, why)
        }
    }
}

/// What the install script showed, where there is any: its heading, and
/// under it the lines as the page's long list, the last thing on the page.
fn script_output(c: &mut Canvas, cx: &Ctx, y: f32, rows: &mut Option<Rows>) -> f32 {
    let lines = &cx.state.script_output;
    if lines.is_empty() {
        return y;
    }
    let y = widgets::section(c, cx.t, cx.f, y, "安装脚本输出");
    *rows = Some(Rows { edges: widgets::output_edges(c, cx.t, cx.f, y, lines), action: None });
    y
}

fn pick_title(pick: Pick, current: Option<Slot>) -> String {
    let letter = |s: Option<Slot>| s.map(|s| format!(" {}", s.letter())).unwrap_or_default();
    match pick {
        Pick::Current => format!("当前槽位{}", letter(current)),
        Pick::Other => format!("另一个槽位{}", letter(current.map(Slot::other))),
        Pick::Both => "两个槽位".into(),
    }
}

/// Installing a package step by step: receiving what the computer sends
/// with `adb sideload`, or reading the file chosen, then installing it.
pub fn installing(cx: &Ctx) -> Frame {
    let (t, f, s, i) = (cx.t, cx.f, cx.state, cx.info);
    let mut c = long_page(cx);
    let (mut out, mut rows) = (Vec::new(), None);
    let top = t.m.content_top;
    let computer = s.source == Source::Computer;
    let bottom = match &s.install {
        Install::Idle | Install::Starting | Install::Ending if computer => {
            let row = standing("正在切换 USB 模式", GREEN, Icon::Usb, "从 adb 调试切换到 sideload 模式");
            widgets::list(&mut c, t, f, top, &[row]).bottom
        }
        Install::Idle | Install::Starting | Install::Ending => {
            widgets::list(&mut c, t, f, top, &[Row::new("正在读取文件").icon(GREEN, Icon::Install).trail(Trail::None)]).bottom
        }
        Install::Waiting => {
            let row = standing("等待电脑发送", GREEN, Icon::Usb, "请用数据线连接电脑，并在电脑上运行以下命令");
            let y = widgets::list(&mut c, t, f, top, &[row]).bottom;
            let y = widgets::section(&mut c, t, f, y, "在电脑上运行");
            let y = widgets::info_card(&mut c, t, f, y, &[(
                "adb sideload 文件名",
                "将文件名替换为系统更新包、刷机包或分区镜像，例如 boot.img".into(),
            )]);
            widgets::footnote(&mut c, t, f, y, "需在电脑上安装 Android SDK Platform-Tools。返回后将停止等待，并将 USB 切换回 adb 调试。")
        }
        Install::Checking { stage: Some(Stage::Signature { done, total }), .. } => {
            let read = share(*done, *total);
            let y = widgets::progress_card(&mut c, t, f, top, "正在验证签名", &format!("已读取 {}%", percent(read)), Some(read));
            let note = match s.source {
                Source::Computer => "读取完成前，请不要断开数据线。返回将取消安装，并结束电脑上的 adb sideload。",
                Source::File(Place::Storage) => "返回将取消安装。",
                Source::File(Place::UsbDrive) => "读取完成前，请不要拔出 U 盘。返回将取消安装。",
            };
            widgets::footnote(&mut c, t, f, y, note)
        }
        Install::Checking { size, .. } => {
            let detail = format!("文件大小 {}", fmt::bytes(*size));
            let y = widgets::list(&mut c, t, f, top, &[standing("正在读取文件", GREEN, Icon::Install, &detail)]).bottom;
            match s.source {
                Source::Computer => widgets::footnote(&mut c, t, f, y, "读取完成前，请不要断开数据线。"),
                Source::File(Place::Storage) => y,
                Source::File(Place::UsbDrive) => widgets::footnote(&mut c, t, f, y, "读取完成前，请不要拔出 U 盘。"),
            }
        }
        Install::Choosing(choice) => {
            let (name, holds) = fmt::image_kind(choice.kind);
            let detail = format!("{}，{holds}", fmt::bytes(choice.size));
            let y = widgets::list(&mut c, t, f, top, &[standing(name, GREEN, Icon::Install, &detail)]).bottom;
            let y = widgets::section(&mut c, t, f, y, "分区");
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
            let note = if computer {
                "刷入将覆盖分区中的原有内容。返回将取消刷入，并结束电脑上的 adb sideload。"
            } else {
                "刷入将覆盖分区中的原有内容。返回将取消刷入。"
            };
            widgets::footnote(&mut c, t, f, track.y + track.h, note)
        }
        Install::Confirming { version, wipe } => {
            let (title, color, icon, detail, which) = if *wipe {
                let what = match version {
                    Some(v) => format!("安装 {v} 后"),
                    None => "安装该更新后".into(),
                };
                ("需清除全部数据".to_string(), ORANGE, Icon::Wipe, format!("{what}将{}。", factory_reset(s)), Confirm::InstallWipe)
            } else {
                let title = match version {
                    Some(v) => format!("安装 {v}"),
                    None => "安装系统更新".into(),
                };
                let detail = match i.slot {
                    Some(slot) => format!("新系统将安装到槽位 {}，重启后将从该槽位启动。", slot.other().letter()),
                    None => "重启后将从新系统启动。".into(),
                };
                (title, GREEN, Icon::Install, detail, Confirm::Install)
            };
            let y = widgets::list(&mut c, t, f, top, &[standing(&title, color, icon, &detail)]).bottom;
            let (label, danger) = confirm_label(which);
            let track = widgets::confirm(&mut c, t, f, y + t.m.dp(28.0), label, danger, s.confirm);
            out.push(Hit { rect: track, action: Action::Confirm(which) });
            widgets::footnote(&mut c, t, f, track.y + track.h, "返回将取消安装，不会写入任何内容。")
        }
        Install::Updating { version, stage, overwriting } => {
            let title = match version {
                Some(v) => format!("正在安装 {v}"),
                None => "正在安装系统更新".into(),
            };
            let (detail, done) = update_stage(stage.as_ref());
            let y = widgets::progress_card(&mut c, t, f, top, &title, &detail, done);
            let target = i.slot.map(|s| format!("新系统将安装到槽位 {}。", s.other().letter())).unwrap_or_default();
            let y = widgets::footnote(&mut c, t, f, y, &format!("{}{target}", until_installed(s.source)));
            if *overwriting {
                widgets::footnote(&mut c, t, f, y, "正在覆盖当前系统，安装完成前重启将无法进入系统。")
            } else {
                y
            }
        }
        Install::Flashing { partition, stage } => {
            let (detail, done) = match stage {
                Some(Stage::Flashing { done, total }) => {
                    let s = share(*done, *total);
                    let what = if done * 2 < *total { "正在写入" } else { "正在校验" };
                    (format!("{what} {}%", percent(s)), Some(s))
                }
                _ => ("正在准备".into(), None),
            };
            let y = widgets::progress_card(&mut c, t, f, top, &format!("正在刷入 {partition}"), &detail, done);
            let keep = match s.source {
                Source::Computer => "刷入完成前，请不要断开数据线。",
                Source::File(Place::Storage) => "刷入完成前，请不要关机。",
                Source::File(Place::UsbDrive) => "刷入完成前，请不要拔出 U 盘。",
            };
            widgets::footnote(&mut c, t, f, y, keep)
        }
        Install::ConfirmingScript => {
            let name = s.package_name.as_deref().unwrap_or("刷机包");
            let detail = "该刷机包将运行其中的安装脚本，安装脚本可以改写本机的任何分区。";
            let mut y = widgets::list(&mut c, t, f, top, &[standing(name, ORANGE, Icon::Install, detail)]).bottom;
            if let Some(note) = fmt::snapshot_script_note(&i.snapshot) {
                y = widgets::footnote(&mut c, t, f, y, note);
            }
            let (label, danger) = confirm_label(Confirm::RunScript);
            let track = widgets::confirm(&mut c, t, f, y + t.m.dp(28.0), label, danger, s.confirm);
            out.push(Hit { rect: track, action: Action::Confirm(Confirm::RunScript) });
            widgets::footnote(&mut c, t, f, track.y + track.h, "返回将取消安装，不会运行安装脚本。")
        }
        Install::Scripting { stage, .. } => {
            let (detail, done) = script_stage(stage.as_ref(), &s.script_output);
            let y = widgets::progress_card(&mut c, t, f, top, "正在安装", detail, done);
            let y = widgets::footnote(&mut c, t, f, y, until_installed(s.source));
            script_output(&mut c, cx, y, &mut rows)
        }
        Install::Finished { outcome, damage } => finished(&mut c, cx, top, outcome, *damage, &mut out, &mut rows),
    };
    Frame { rows, ..Frame::new(t, fit(c, t, bottom), out, install_title(s.source), top) }
}

/// What not to do while a package from `source` is installed.
fn until_installed(source: Source) -> &'static str {
    match source {
        Source::Computer => "安装完成前，请不要断开数据线或关机。",
        Source::File(Place::Storage) => "安装完成前，请不要关机。",
        Source::File(Place::UsbDrive) => "安装完成前，请不要拔出 U 盘或关机。",
    }
}

/// How an install ended, and where to go from there: another package from
/// the computer, or back to choose another file.
fn finished(
    c: &mut Canvas,
    cx: &Ctx,
    top: f32,
    outcome: &Outcome,
    damage: Option<Damage>,
    out: &mut Vec<Hit>,
    rows: &mut Option<Rows>,
) -> f32 {
    let (t, f, source) = (cx.t, cx.f, cx.state.source);
    let gap = t.m.card_gap;
    let (again, another) = match source {
        Source::Computer => (Row::new("再次发送").icon(GREEN, Icon::Usb), Action::Sideload),
        Source::File(_) => (Row::new("选择其他文件").icon(GREEN, Icon::Folder), Action::Back),
    };
    let restart = Row::new("重启到系统").icon(ORANGE, Icon::Power);
    match outcome {
        Outcome::Installed { slot, .. } => {
            let detail = format!("新系统已安装到槽位 {}，重启后将从该槽位启动。", slot.letter());
            let y = widgets::list(c, t, f, top, &[standing("安装成功", GREEN, Icon::Install, &detail)]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[restart]);
            out.extend(hits(&next.rows, &[Action::RebootTo(Target::System)]));
            reflashed(c, cx, next.bottom)
        }
        Outcome::Flashed { partition, slots } => {
            let detail = format!("{partition} 已刷入{}。", fmt::slots(slots));
            let y = widgets::list(c, t, f, top, &[standing("刷入成功", GREEN, Icon::Install, &detail)]).bottom;
            let mut rows = vec![restart];
            let mut acts = vec![Action::RebootTo(Target::System)];
            if *partition == "recovery" {
                rows.push(Row::new("重启到恢复模式").icon(ORANGE, Icon::Power));
                acts.push(Action::RebootTo(Target::Recovery));
            }
            rows.push(again);
            acts.push(another);
            let next = widgets::list(c, t, f, y + gap, &rows);
            out.extend(hits(&next.rows, &acts));
            next.bottom
        }
        Outcome::Ran { .. } => {
            let y = widgets::list(c, t, f, top, &[standing("安装成功", GREEN, Icon::Install, "安装脚本已运行完成。")]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[restart, again]);
            out.extend(hits(&next.rows, &[Action::RebootTo(Target::System), another]));
            let y = reflashed(c, cx, next.bottom);
            script_output(c, cx, y, rows)
        }
        Outcome::Cancelled => {
            let y = widgets::list(c, t, f, top, &[standing("已取消", SLATE, Icon::Install, "未写入任何内容。")]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[again]);
            out.extend(hits(&next.rows, &[another]));
            next.bottom
        }
        Outcome::Refused(r) => {
            let reason = fmt::refusal(r, &cx.info.codename, source, cx.state.time_zone);
            let y = widgets::list(c, t, f, top, &[standing("无法安装", ORANGE, Icon::Install, &reason)]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[again]);
            out.extend(hits(&next.rows, &[another]));
            widgets::footnote(c, t, f, next.bottom, "未写入任何内容。")
        }
        Outcome::Failed(failure) => {
            let (what, detail) = fmt::failure(failure, source);
            let y = widgets::list(c, t, f, top, &[standing("安装失败", RED, Icon::Install, &what)]).bottom;
            let next = widgets::list(c, t, f, y + gap, &[again, restart]);
            out.extend(hits(&next.rows, &[another, Action::RebootTo(Target::System)]));
            let redo = match source {
                Source::Computer => ("重新发送", "重新发送"),
                Source::File(_) => ("重新安装", "重新刷入"),
            };
            let warning = match damage {
                Some(Damage::System) => format!("当前系统已被部分覆盖，重启后将无法进入系统。请{}完整更新包，安装完成后再重启。", redo.0),
                Some(Damage::Partition(p)) => format!("{p} 仅写入了部分内容，重启后可能无法正常启动。请{}该镜像，刷入完成后再重启。", redo.1),
                Some(Damage::Script) => format!("本机的分区可能已被部分改写，重启后可能无法正常启动。请{}该刷机包，安装完成后再重启。", redo.0),
                None => "当前系统未受影响。".into(),
            };
            let mut y = widgets::footnote(c, t, f, next.bottom, &warning);
            if let Some(d) = detail {
                y = widgets::footnote(c, t, f, y, &d);
            }
            let y = reflashed(c, cx, y);
            script_output(c, cx, y, rows)
        }
    }
}

/// What the wipe page lists of [`WIPES`] beyond everything, under the
/// partition each is in.
const WIPE_GROUPS: [(&str, &[Wipe]); 2] = [
    ("data 分区", &[Wipe::AppData, Wipe::Media, Wipe::AppCaches, Wipe::SystemCaches, Wipe::Art, Wipe::Logs, Wipe::Updates]),
    ("cache 分区", &[Wipe::Cache]),
];

/// What a factory reset erases, as the wipe page and an update that asks
/// for one tell it.
fn factory_reset(s: &State) -> String {
    let also = if s.cache_partition { "metadata 和 cache 分区" } else { "metadata 分区" };
    format!("清空 data 分区，包括应用、账户、设置和内部存储，并格式化 {also}")
}

/// A choice of the wipe page as it lists it: its title, and exactly what it
/// erases.
fn wipe_row(w: Wipe, s: &State) -> (&'static str, String) {
    match w {
        Wipe::All => ("全部数据", factory_reset(s)),
        Wipe::AppData => (
            "应用数据",
            "/data/data、/data/user、/data/user_de，以及内部存储中的 Android/data、Android/media，应用将恢复到初始状态".into(),
        ),
        Wipe::Media => ("内部存储", "/data/media，即 /sdcard 中的照片、视频和文件".into()),
        Wipe::AppCaches => ("应用缓存", "各应用数据中的 cache、code_cache，以及 Android/data 中的 cache，应用将按需重新生成".into()),
        Wipe::SystemCaches => ("系统缓存", "/data/system/package_cache、/data/resource-cache、/data/cache，下次开机时重新生成".into()),
        Wipe::Art => (
            "ART 缓存",
            "/data/dalvik-cache、/data/misc/apexdata/com.android.art/dalvik-cache、/data/ota，下次开机时重新编译，启动时间较长".into(),
        ),
        Wipe::Logs => {
            let own = match s.phone_logs {
                0 => String::new(),
                n => format!("，以及本机的 {n} 个日志文件夹"),
            };
            ("日志", format!("/data/tombstones、/data/anr、/data/system/dropbox、/data/misc/logd 等系统日志和崩溃记录，各用户的错误报告{own}"))
        }
        Wipe::Updates => ("下载的更新包", "/data/ota_package 中的系统更新包".into()),
        Wipe::Cache => ("全部内容", "格式化 cache 分区，保留 recovery 文件夹中的日志".into()),
    }
}

/// What to erase: everything, as a factory reset does, or parts of it on
/// their own, the cache partition among them where the phone has one; then
/// a swipe erases what is chosen.
pub fn wipe(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let chosen = |w: Wipe| s.wipe[WIPES.iter().position(|&x| x == w).expect("every choice is one of WIPES")];
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let y = widgets::section_first(&mut c, t, f, top, "恢复出厂设置");
    let (name, sub) = wipe_row(Wipe::All, s);
    let all = widgets::list(&mut c, t, f, y, &[Row::new(name).subtitle(&sub).trail(Trail::Check(chosen(Wipe::All)))]);
    out.extend(hits(&all.rows, &[Action::WipeItem(Wipe::All)]));
    let mut y = widgets::footnote(&mut c, t, f, all.bottom, "系统和已安装的更新不受影响，清除后需重新设置手机。");
    for (title, items) in WIPE_GROUPS {
        let shown: Vec<Wipe> = items.iter().copied().filter(|&w| w != Wipe::Cache || s.cache_partition).collect();
        if shown.is_empty() {
            continue;
        }
        let below = widgets::section(&mut c, t, f, y, title);
        let texts: Vec<(&str, String)> = shown.iter().map(|&w| wipe_row(w, s)).collect();
        let rows: Vec<Row> =
            shown.iter().zip(&texts).map(|(&w, (name, sub))| Row::new(name).subtitle(sub).trail(Trail::Check(chosen(w)))).collect();
        let list = widgets::list(&mut c, t, f, below, &rows);
        out.extend(hits(&list.rows, &shown.iter().map(|&w| Action::WipeItem(w)).collect::<Vec<_>>()));
        y = list.bottom;
    }
    let y = widgets::footnote(&mut c, t, f, y, "清除应用数据和应用缓存前，需先完成「解密」。清除后无法恢复。");
    let (label, danger) = confirm_label(Confirm::Wipe);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::Wipe) });
    Frame::new(t, fit(c, t, track.y + track.h), out, "清除数据", top)
}

/// A page laid out above the on-screen keyboard: where the keyboard's top
/// is, and its keys as touch targets.
fn keyboard_hits(c: &mut Canvas, t: &Theme, f: &Fonts, s: &State, enter: &str, out: &mut Vec<Hit>) -> f32 {
    let top = t.m.h as f32 - widgets::keyboard_height(t);
    let keys = widgets::keyboard(c, t, f, top, s.layer, s.key_held(), enter);
    out.extend(keys.into_iter().map(|(rect, cap)| Hit { rect, action: Action::Type(cap) }));
    top
}

/// Finding a feature or partition: the field where the title would be, the
/// button that leaves the search beside it, what it finds, and the
/// keyboard.
pub fn search(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let m = &t.m;
    let mut c = page(cx);
    let mut out = Vec::new();
    let (at, cancel) = widgets::search_layout(t);
    let field = widgets::search_field(&mut c, t, f, at.x, at.w, at.y + at.h / 2.0, &s.search, SEARCH_HINT);
    widgets::cancel_button(&mut c, t, cancel);
    out.push(Hit { rect: cancel, action: Action::CloseSearch });
    let top = field.field.y + field.field.h + m.dp(12.0);
    let kb_top = m.h as f32 - widgets::keyboard_height(t);
    if s.search.trim().is_empty() {
        widgets::footnote(&mut c, t, f, top, "输入功能或分区名称，支持拼音全拼和首字母，例如输入 bf 可找到「备份」。");
    } else {
        let found = search::find_on(&s.search, s.wlan.is_some());
        if found.is_empty() {
            let none = format!("未找到\"{}\"", s.search.trim());
            widgets::list(&mut c, t, f, top, &[Row::new(&none).trail(Trail::None)]);
        } else {
            let rows: Vec<Row> = found.iter().map(|e| Row::new(e.title).subtitle(e.place)).collect();
            let list = widgets::list(&mut c, t, f, top, &rows);
            // The keyboard covers what does not fit above it.
            let shown = list.rows.iter().zip(&found).filter(|(r, _)| r.y + r.h <= kb_top);
            out.extend(shown.map(|(&rect, e)| Hit { rect, action: Action::Open(e.screen) }));
        }
    }
    keyboard_hits(&mut c, t, f, s, "搜索", &mut out);
    if let Some(rect) = field.clear {
        out.push(Hit { rect, action: Action::ClearSearch });
    }
    Frame { back: false, header: false, caret: Some(field.caret), ..Frame::new(t, c, out, "搜索", 0.0) }
}

/// Backing up: what to take, where it goes, and how.
pub fn backup(cx: &Ctx) -> Frame {
    let (t, f, s, i) = (cx.t, cx.f, cx.state, cx.info);
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let y = widgets::section_first(&mut c, t, f, top, "备份内容");
    let system = fmt::snapshot_backup_note(&i.snapshot).unwrap_or("当前槽位在 super 中的分区");
    let data = if s.data == Data::Open {
        "应用、账户和设置，不包括内部存储、锁屏密码和指纹"
    } else {
        "应用、账户和设置，不包括内部存储、锁屏密码和指纹，需先完成「解密」"
    };
    let subs = ["boot、init_boot、vendor_boot、dtbo、vbmeta 和 recovery", system, data, "IMEI、基带设置和传感器校准数据"];
    let rows: Vec<Row> =
        BACKUP_PARTS.iter().zip(subs).enumerate().map(|(k, (name, sub))| Row::new(name).subtitle(sub).trail(Trail::Check(s.backup[k]))).collect();
    let parts = widgets::list(&mut c, t, f, y, &rows);
    out.extend(hits(&parts.rows, &[0, 1, 2, 3].map(Action::BackupPart)));
    let y = widgets::section(&mut c, t, f, parts.bottom, "存放位置");
    let storage = match (s.data.storage_open(), i.storage.free) {
        (true, Some(free)) => format!("剩余 {}", fmt::gb(free)),
        (true, None) => "无法读取剩余空间".to_string(),
        (false, _) if s.data == Data::Erased => "data 分区已清除".to_string(),
        (false, _) => "需先完成「解密」".to_string(),
    };
    let usb = match &s.usb_drive {
        Some(UsbDrive::Shown) => "已连接",
        Some(UsbDrive::Unreadable(_)) => "无法读取该 U 盘",
        None => "请通过 OTG 连接 U 盘",
    };
    let places = widgets::list(&mut c, t, f, y, &[
        Row::new(place_name(Place::Storage)).subtitle(&storage).trail(Trail::Check(s.backup_to == Place::Storage)),
        Row::new(place_name(Place::UsbDrive)).subtitle(usb).trail(Trail::Check(s.backup_to == Place::UsbDrive)),
    ]);
    out.extend(hits(&places.rows, &[Action::BackupTo(Place::Storage), Action::BackupTo(Place::UsbDrive)]));
    let y = widgets::section(&mut c, t, f, places.bottom, "选项");
    let options = widgets::list(&mut c, t, f, y, &[
        Row::new("压缩").subtitle("备份更小，耗时更长").trail(Trail::Switch(s.compress)),
        Row::new("完成后校验").subtitle("确认备份文件完整").trail(Trail::Switch(s.verify)),
    ]);
    out.extend(hits(&options.rows, &[Action::Toggle(Switch::Compress), Action::Toggle(Switch::Verify)]));
    let y = widgets::footnote(&mut c, t, f, options.bottom, "备份将保存在 Skyward/backups 文件夹中，以开始备份的时间命名。");
    let (label, danger) = confirm_label(Confirm::Backup);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::Backup) });
    Frame::new(t, fit(c, t, track.y + track.h), out, "备份", top)
}

/// The backups found, newest first, to restore one; those that cannot be
/// read are listed after them, to be deleted.
pub fn restore(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let y = widgets::section_first(&mut c, t, f, top, "备份");
    let bottom = if s.backups.is_empty() {
        widgets::list(&mut c, t, f, y, &[Row::new("暂无备份").trail(Trail::None)]).bottom
    } else {
        let texts: Vec<(String, String)> = s
            .backups
            .iter()
            .map(|b| match &b.held {
                Ok(h) => (fmt::datetime(h.made, s.time_zone), format!("{} · {} · {}", fmt::parts(&h.parts), fmt::bytes(h.size), place_name(b.place))),
                Err(_) => (b.name.clone(), format!("无法读取 · {}", place_name(b.place))),
            })
            .collect();
        let rows: Vec<Row> = texts.iter().map(|(title, sub)| Row::new(title).subtitle(sub)).collect();
        let list = widgets::list(&mut c, t, f, y, &rows);
        out.extend(list.rows.iter().enumerate().map(|(k, &rect)| Hit { rect, action: Action::OpenBackup(k) }));
        list.bottom
    };
    let bottom = widgets::footnote(&mut c, t, f, bottom, "将在内部存储和 U 盘中的 Skyward/backups 文件夹查找备份，查找内部存储需先完成「解密」。");
    Frame::new(t, fit(c, t, bottom), out, "恢复", top)
}

/// One backup: when and where it was made, and which of its parts to
/// restore. One made on another phone is not restored here, and one that
/// cannot be read can only be deleted.
pub fn restore_backup(cx: &Ctx) -> Frame {
    let (t, f, s, i) = (cx.t, cx.f, cx.state, cx.info);
    let (entry, chosen) = s.restore.as_ref().expect("the restore page shows a backup");
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let y = match &entry.held {
        Err(why) => {
            let y = widgets::info_card(&mut c, t, f, top, &[("文件夹", entry.name.clone()), ("位置", place_name(entry.place).to_string())]);
            widgets::footnote(&mut c, t, f, y, &format!("无法读取该备份，备份可能未完成或已损坏。{why}"))
        }
        Ok(held) => {
            let mut facts = vec![
                ("备份时间", fmt::datetime(held.made, s.time_zone)),
                ("大小", fmt::bytes(held.size)),
                ("位置", place_name(entry.place).to_string()),
            ];
            if let Some(system) = &held.system {
                facts.push(("系统版本", system.clone()));
            }
            if let Some(slot) = held.slot {
                facts.push(("槽位", slot.letter().to_string()));
            }
            let y = widgets::info_card(&mut c, t, f, top, &facts);
            if held.device != i.codename {
                widgets::footnote(&mut c, t, f, y, &format!("该备份来自其他设备（{}），无法恢复到本机。", held.device))
            } else {
                let y = widgets::section(&mut c, t, f, y, "恢复内容");
                let kept: Vec<usize> = (0..BACKUP_PARTS.len()).filter(|&k| held.parts[k]).collect();
                let rows: Vec<Row> = kept.iter().map(|&k| Row::new(BACKUP_PARTS[k]).trail(Trail::Check(chosen[k]))).collect();
                let list = widgets::list(&mut c, t, f, y, &rows);
                out.extend(list.rows.iter().zip(&kept).map(|(&rect, &k)| Hit { rect, action: Action::RestorePart(k) }));
                let mut y = widgets::footnote(&mut c, t, f, list.bottom, "恢复后，所选内容的现有数据将被备份覆盖。");
                if chosen[2] {
                    y = widgets::footnote(
                        &mut c,
                        t,
                        f,
                        y,
                        "data 分区将恢复应用、账户和设置，内部存储、锁屏密码和指纹保持不变。恢复出厂设置后，无法再恢复此前的 data 分区备份。",
                    );
                }
                // The boot partitions' vbmeta checks the system's: one of
                // another version without the other may not start.
                if let (Some(was), Some(now)) = (&held.system, &i.os_version) {
                    if was != now && chosen[0] != chosen[1] {
                        let warn = format!("该备份来自 {was}，本机当前为 {now}。仅恢复启动分区或系统分区中的一项，可能导致无法开机。");
                        y = widgets::footnote(&mut c, t, f, y, &warn);
                    }
                }
                let (label, danger) = confirm_label(Confirm::Restore);
                let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
                out.push(Hit { rect: track, action: Action::Confirm(Confirm::Restore) });
                track.y + track.h
            }
        }
    };
    let delete = widgets::list(&mut c, t, f, y + t.m.dp(28.0), &[Row::new("删除备份").tint(t.pal.danger).trail(Trail::None)]);
    out.extend(hits(&delete.rows, &[Action::AskDelete]));
    let mut bottom = delete.bottom;
    if s.asking_delete {
        let y = widgets::footnote(&mut c, t, f, delete.bottom, "删除该备份？删除后无法恢复。");
        let (label, danger) = confirm_label(Confirm::DeleteBackup);
        let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
        out.push(Hit { rect: track, action: Action::Confirm(Confirm::DeleteBackup) });
        bottom = track.y + track.h;
    }
    Frame::new(t, fit(c, t, bottom), out, "恢复备份", top)
}

/// The share of its pass the part under way has done.
fn part_share(a: &BackupAt) -> f32 {
    if a.of == 0 {
        1.0
    } else {
        share(a.done, a.of).min(1.0)
    }
}

/// How far backing up or restoring has come over all, 0 to 1, and the
/// bytes of the parts the pass under way has gone through, the part under
/// way by its share. Each part counts by its bytes in each pass: a restore
/// checks all its parts first, and a backup checks them last.
fn job_progress(job: &BackupJob, a: &BackupAt) -> (f32, u64) {
    let total: u64 = job.plan.iter().map(|(_, n)| n).sum();
    let before: u64 = job.plan[..a.item].iter().map(|(_, n)| n).sum();
    let passed = before as f64 + f64::from(part_share(a)) * job.plan[a.item].1 as f64;
    let second = job.checks && a.checking != job.restore;
    let passes = if job.checks { 2.0 } else { 1.0 };
    let over = (if second { total as f64 } else { 0.0 } + passed) / (passes * total.max(1) as f64);
    (over as f32, passed as u64)
}

/// Where part `k` of a backup or restore stands, as its row says it.
fn part_state(job: &BackupJob, k: usize) -> String {
    use std::cmp::Ordering;
    let (did, doing) = if job.restore { ("已恢复", "正在恢复") } else { ("已备份", "正在备份") };
    let ended = job.outcome.is_some();
    let a = match (&job.outcome, job.at) {
        (Some(BackupEnd::Saved(_)), _) => return if job.checks { "已校验" } else { did }.to_string(),
        (Some(BackupEnd::Restored { .. }), _) => return did.to_string(),
        (_, None) => return if ended { "未开始" } else { "等待中" }.to_string(),
        (_, Some(a)) => a,
    };
    match k.cmp(&a.item) {
        Ordering::Less if a.checking => "已校验".to_string(),
        Ordering::Less => did.to_string(),
        Ordering::Equal if ended => "未完成".to_string(),
        Ordering::Equal => format!("{} {}%", if a.checking { "正在校验" } else { doing }, percent(part_share(&a))),
        // Ahead of the pass under way: a backup checks its parts once all
        // are written, and a restore writes them once all are checked.
        Ordering::Greater => match (a.checking, job.restore, ended) {
            (true, false, false) => did,
            (true, false, true) => "未校验",
            (false, false, false) | (true, true, false) => "等待中",
            (false, false, true) => "未备份",
            (false, true, false) => "已校验",
            (_, true, true) => "未恢复",
        }
        .to_string(),
    }
}

/// Backing up or restoring: how far it has come over all, each part and
/// where it stands, and a way to stop while stopping breaks nothing. Once
/// it has ended, how, and where to go.
pub fn backup_job(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let job = s.backup_job.as_ref().expect("the backup page shows work");
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let (did, doing, icon) = if job.restore { ("已恢复", "正在恢复", Icon::Restore) } else { ("已备份", "正在备份", Icon::Backup) };
    let writing = job.at.is_some_and(|a| !a.checking);
    let mut y = match &job.outcome {
        None => {
            let (title, detail, done) = match job.at {
                Some(a) => {
                    let (over, passed) = job_progress(job, &a);
                    let total: u64 = job.plan.iter().map(|(_, n)| n).sum();
                    let (verb, past) = if a.checking { ("正在校验", "已校验") } else { (doing, did) };
                    (format!("{verb} {}", job.plan[a.item].0), format!("{past} {}，共 {}", fmt::bytes(passed), fmt::bytes(total)), over)
                }
                None if job.restore => ("正在读取备份".to_string(), "正在核对备份与本机".to_string(), 0.0),
                None => ("正在准备".to_string(), "正在计算备份大小".to_string(), 0.0),
            };
            widgets::progress_card(&mut c, t, f, top, &title, &detail, Some(done))
        }
        Some(end) => {
            let gone = if job.restore { "未写入任何内容。" } else { "未保存任何备份。" };
            let (title, color, detail) = match end {
                BackupEnd::Saved(name) => ("备份完成", GREEN, format!("已保存到{}的 Skyward/backups/{name}", place_name(job.place))),
                BackupEnd::Restored { .. } => ("恢复完成", GREEN, "重启手机后，将使用恢复的内容。".to_string()),
                BackupEnd::Stopped => ("已停止", SLATE, gone.to_string()),
                BackupEnd::Refused(r) => (if job.restore { "无法恢复" } else { "无法备份" }, ORANGE, fmt::backup_refusal(r, job.restore)),
                BackupEnd::Failed(_) if job.restore && writing => {
                    ("恢复失败", RED, "部分内容已写入，重启后可能无法正常启动。请重新恢复，完成后再重启。".to_string())
                }
                BackupEnd::Failed(_) => (if job.restore { "恢复失败" } else { "备份失败" }, RED, gone.to_string()),
            };
            widgets::list(&mut c, t, f, top, &[standing(title, color, icon, &detail)]).bottom
        }
    };
    let keep = match job.place {
        Place::Storage => "请不要关机。",
        Place::UsbDrive => "请不要拔出 U 盘或关机。",
    };
    match &job.outcome {
        None => {
            let note = match (job.restore, job.at) {
                (false, _) => format!("备份完成前，{keep}停止后将删除未完成的备份。"),
                (true, Some(a)) if !a.checking => format!("恢复完成前，{keep}"),
                (true, _) => "校验备份完成前，不会写入任何内容。".to_string(),
            };
            y = widgets::footnote(&mut c, t, f, y, &note);
            // A restore that has begun writing goes on to its end.
            if !(job.restore && writing) {
                let label = if job.stopping { "正在停止" } else { "停止" };
                let stop = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new(label).trail(Trail::None)]);
                if !job.stopping {
                    out.extend(hits(&stop.rows, &[Action::StopBackup]));
                }
                y = stop.bottom;
            }
        }
        Some(end) => {
            match end {
                BackupEnd::Restored { unmade } if !unmade.is_empty() => {
                    let note = format!("以下文件夹所属的用户已不存在，其中的内容未恢复：{}", unmade.join("、"));
                    y = widgets::footnote(&mut c, t, f, y, &note);
                }
                BackupEnd::Failed(why) => y = widgets::footnote(&mut c, t, f, y, why),
                _ => {}
            }
            let (rows, acts): (Vec<Row>, Vec<Action>) = match end {
                BackupEnd::Restored { .. } => (
                    vec![Row::new("重启到系统").icon(ORANGE, Icon::Power), Row::new("完成").trail(Trail::None)],
                    vec![Action::RebootTo(Target::System), Action::Back],
                ),
                _ => (vec![Row::new("完成").trail(Trail::None)], vec![Action::Back]),
            };
            let next = widgets::list(&mut c, t, f, y + t.m.card_gap, &rows);
            out.extend(hits(&next.rows, &acts));
            y = next.bottom;
        }
    }
    // The parts come after what can be done, which a long list would push
    // off the screen.
    if !job.plan.is_empty() {
        let y0 = widgets::section(&mut c, t, f, y, if job.restore { "恢复内容" } else { "备份内容" });
        let states: Vec<String> = (0..job.plan.len()).map(|k| part_state(job, k)).collect();
        let rows: Vec<Row> = job.plan.iter().zip(&states).map(|((name, _), state)| Row::new(name).trail(Trail::Text(state))).collect();
        y = widgets::list(&mut c, t, f, y0, &rows).bottom;
    }
    let title = match (&job.outcome, job.restore) {
        (None, false) => "正在备份",
        (None, true) => "正在恢复",
        (Some(BackupEnd::Saved(_)), _) => "备份完成",
        (Some(BackupEnd::Restored { .. }), _) => "恢复完成",
        (Some(_), false) => "备份未完成",
        (Some(_), true) => "恢复未完成",
    };
    Frame { back: job.outcome.is_some(), ..Frame::new(t, fit(c, t, y), out, title, top) }
}

/// Tools for the experienced.
pub fn advanced(cx: &Ctx) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(cx);
    let top = t.m.content_top;
    let tools = widgets::list(&mut c, t, f, top, &[
        Row::new("文件管理").icon(BLUE, Icon::Folder),
        Row::new("终端").icon(SLATE, Icon::Terminal),
        Row::new("修复 SELinux 上下文").icon(PURPLE, Icon::Shield),
        Row::new("导出日志").icon(ORANGE, Icon::Log),
        Row::new("系统更新").icon(GREEN, Icon::Layers).trail(Trail::Value(fmt::update_value(&cx.state.update))),
    ]);
    let out = hits(&tools.rows, &[
        Action::Open(Screen::Files),
        Action::Open(Screen::Terminal),
        Action::Open(Screen::Relabel),
        Action::Open(Screen::Logs),
        Action::Open(Screen::Update),
    ]);
    Frame::new(t, c, out, "高级", top)
}

/// Giving the data partition's files the SELinux labels the installed
/// system's rules give them: what it does, and a swipe to start it; then
/// how far it has come, a way to stop it, and how it ended.
pub fn relabel(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let title = "修复 SELinux 上下文";
    let Some(job) = &s.relabel else {
        let mut y = widgets::paragraph(&mut c, t, f, top, "按已安装系统的规则，重新设置 data 分区中文件的 SELinux 上下文，应用数据保持不变。");
        let note = match s.data {
            Data::Open | Data::Locked { .. } => None,
            Data::Erased => Some("data 分区已清除。"),
            Data::Closed | Data::Opening | Data::Failed(_) => Some("暂无法修复，需先在「解密」中打开 data 分区。"),
        };
        if let Some(note) = note {
            y = widgets::footnote(&mut c, t, f, y, note);
        }
        let (label, danger) = confirm_label(Confirm::Relabel);
        let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
        out.push(Hit { rect: track, action: Action::Confirm(Confirm::Relabel) });
        return Frame::new(t, c, out, title, top);
    };
    let heading = match &job.outcome {
        None => "正在修复",
        Some(JobEnd::Done) => "修复完成",
        Some(JobEnd::Stopped) => "已停止",
        Some(JobEnd::Failed(_)) => "修复失败",
    };
    let detail = match job.total {
        0 if job.outcome.is_none() => "正在准备".to_string(),
        total => format!("已检查 {} 项，共 {total} 项", job.checked),
    };
    let y = widgets::progress_card(&mut c, t, f, top, heading, &detail, Some(share(job.checked, job.total)));
    match &job.outcome {
        None => {
            let label = if job.stopping { "正在停止" } else { "停止" };
            let stop = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new(label).trail(Trail::None)]);
            if !job.stopping {
                out.extend(hits(&stop.rows, &[Action::StopRelabel]));
            }
        }
        Some(end) => {
            let note = match end {
                JobEnd::Done if job.changed == 0 => "全部文件的 SELinux 上下文都正确，无需修复。".to_string(),
                JobEnd::Done => format!("已按规则重新设置 {} 项的 SELinux 上下文。", job.changed),
                JobEnd::Stopped => format!("已停止，已重新设置 {} 项。", job.changed),
                JobEnd::Failed(why) => format!("修复失败：{why}"),
            };
            let y = widgets::footnote(&mut c, t, f, y, &note);
            let finish = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new("完成").trail(Trail::None)]);
            out.extend(hits(&finish.rows, &[Action::Back]));
        }
    }
    Frame { back: job.outcome.is_some(), ..Frame::new(t, c, out, title, top) }
}

/// Where an update the system installed stands, and merging it into the
/// slot it went to: how far a merge under way has come, and a swipe to
/// merge one that waits there for the system to start it.
pub fn update(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let top = t.m.content_top;
    let mut out = Vec::new();
    if let Standing::Merging(done) = s.update {
        let share = done as f32;
        let y = widgets::progress_card(&mut c, t, f, top, "正在合并更新", &format!("已完成 {}%", percent(share)), Some(share));
        widgets::footnote(&mut c, t, f, y, "合并完成前，暂不刷写 super 中的分区。返回后将继续合并。");
        return Frame::new(t, c, out, "系统更新", top);
    }
    let next = match &s.boot_slots {
        Some(Ok(slots)) => Some(slots.active),
        _ => None,
    };
    let (title, detail) = fmt::update(&s.update, next);
    let color = match s.update {
        Standing::None => SLATE,
        Standing::Merged => GREEN,
        Standing::Failed(_) | Standing::Unknown => RED,
        _ => ORANGE,
    };
    let mut row = Row::new(title).icon(color, Icon::Layers).trail(Trail::None);
    if let Some(detail) = &detail {
        row = row.subtitle(detail);
    }
    let y = widgets::list(&mut c, t, f, top, &[row]).bottom;
    if s.update == Standing::Waiting {
        let (label, danger) = confirm_label(Confirm::Merge);
        let track = widgets::confirm(&mut c, t, f, y + t.m.dp(28.0), label, danger, s.confirm);
        out.push(Hit { rect: track, action: Action::Confirm(Confirm::Merge) });
        widgets::footnote(&mut c, t, f, track.y + track.h, "合并后无法回到更新前的系统。");
    }
    Frame::new(t, c, out, "系统更新", top)
}

/// The WLAN switch; then the network chosen and how joining it goes, and
/// the networks in range, strongest first.
pub fn wlan(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let w = s.wlan.as_ref().expect("the WLAN page shows on a phone whose WLAN is brought up");
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let on = !matches!(w.power, Power::Off | Power::Failed(_));
    let switch = widgets::list(&mut c, t, f, top, &[Row::new("WLAN").trail(Trail::Switch(on))]);
    out.extend(hits(&switch.rows, &[Action::Toggle(Switch::Wlan)]));
    let mut y = switch.bottom;
    match &w.power {
        Power::Off => y = widgets::footnote(&mut c, t, f, y, "开启后，可连接 WLAN 下载系统更新。"),
        Power::Failed(why) => y = widgets::footnote(&mut c, t, f, y, &format!("无法开启 WLAN：{why}")),
        Power::Starting => y = widgets::footnote(&mut c, t, f, y, "正在开启 WLAN"),
        Power::On => {
            let locked = |s: Security| !matches!(s, Security::Open | Security::Owe);
            // The network chosen stands out in the accent, as the stock
            // page shows the one it is on.
            if let Some(j) = &w.joined {
                let found = w.networks.iter().find(|n| n.ssid == j.ssid);
                let (signal, bands) = match (&j.stage, found) {
                    (Joining::Connected(link), found) => (Some(link.signal), found.map_or(Bands::of(link.frequency), |n| n.bands)),
                    (_, Some(n)) => (Some(n.signal), n.bands),
                    (_, None) => (None, Bands::default()),
                };
                let tag = fmt::band_tag(bands);
                let mut row = Row::new(&j.name).fan(signal.map_or(0, fmt::signal_level)).subtitle(fmt::joining(&j.stage));
                row = row.trail(Trail::Network { locked: locked(j.security) });
                if let Some(tag) = &tag {
                    row = row.tag(tag);
                }
                let joined = widgets::list_toned(&mut c, t, f, y + t.m.card_gap, &[row], &Tone::lit(t));
                out.extend(hits(&joined.rows, &[Action::OpenJoined]));
                out.push(Hit { rect: widgets::round_button(t, joined.rows[0]), action: Action::OpenJoined });
                y = joined.bottom;
            }
            let others: Vec<(usize, &Network)> =
                w.networks.iter().enumerate().filter(|(_, n)| w.joined.as_ref().is_none_or(|j| j.ssid != n.ssid)).collect();
            let (below, rescan) = widgets::section_action(&mut c, t, f, y, "选取附近的 WLAN", "刷新");
            out.push(Hit { rect: rescan, action: Action::Rescan });
            y = below;
            if others.is_empty() {
                y = widgets::footnote(&mut c, t, f, y - t.m.dp(10.0), "正在搜索附近的 WLAN") + t.m.card_gap;
            }
            let tags: Vec<Option<String>> = others.iter().map(|(_, n)| fmt::band_tag(n.bands)).collect();
            let mut rows: Vec<Row> = others
                .iter()
                .zip(&tags)
                .map(|((_, n), tag)| {
                    let row = Row::new(&n.name).fan(fmt::signal_level(n.signal)).trail(Trail::Network { locked: locked(n.security) });
                    match tag {
                        Some(tag) => row.tag(tag),
                        None => row,
                    }
                })
                .collect();
            // The list ends with adding a network by hand, as the stock one
            // does.
            rows.push(Row::new("添加网络").tint(t.pal.accent).trail(Trail::None));
            let list = widgets::list(&mut c, t, f, y, &rows);
            let mut picks: Vec<Action> = others.iter().map(|(k, _)| Action::JoinNetwork(*k)).collect();
            picks.push(Action::AddNetwork);
            out.extend(hits(&list.rows, &picks));
            // The round buttons come after the rows they sit on, and a touch
            // finds the last control under it.
            for (rect, (k, _)) in list.rows.iter().zip(&others) {
                out.push(Hit { rect: widgets::round_button(t, *rect), action: Action::NetworkDetails(*k) });
            }
            y = list.bottom;
        }
    }
    Frame::new(t, fit(c, t, y), out, "WLAN", top)
}

/// The password of the network chosen: its name, what is typed, a switch
/// to show it, and the keyboard.
pub fn wlan_password(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let network = s.joining.as_ref().expect("the password page is for a network");
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let y = widgets::section_first(&mut c, t, f, top, &network.name);
    let shown = if s.show_password { s.password.clone() } else { "•".repeat(s.password.chars().count()) };
    let (y, caret) = widgets::text_field(&mut c, t, f, y, &shown);
    let show = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new("显示密码").trail(Trail::Switch(s.show_password))]);
    out.extend(hits(&show.rows, &[Action::Toggle(Switch::ShowPassword)]));
    keyboard_hits(&mut c, t, f, s, "连接", &mut out);
    Frame { caret: Some(caret), ..Frame::new(t, c, out, "输入密码", top) }
}

/// A network added by hand: its name, how it is protected, its password
/// where it takes one with the switch that shows it, and the keyboard,
/// which types into the field touched. The protections open over the page
/// from their row, which alone then answers a touch.
pub fn wlan_add(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let m = &t.m;
    let a = s.adding.as_ref().expect("the page adds a network");
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = m.content_top;
    let field = |y: f32| Rect::new(m.margin, y, m.w as f32 - m.margin * 2.0, m.dp(56.0));
    let y = widgets::section_first(&mut c, t, f, top, "网络名称");
    let (below, mut caret) = widgets::text_field(&mut c, t, f, y, &a.name);
    out.push(Hit { rect: field(y), action: Action::TypeInto(Field::Name) });
    // Held down while its list is open, as the stock row stays.
    let tone = if a.choosing { Tone::page(&t.pressed()) } else { Tone::page(t) };
    let protection = [Row::new("安全性").trail(Trail::Choice(fmt::security(a.security)))];
    let security = widgets::list_toned(&mut c, t, f, below + m.card_gap, &protection, &tone);
    out.extend(hits(&security.rows, &[Action::ChooseSecurity]));
    if a.security.password() {
        let y = widgets::section(&mut c, t, f, security.bottom, "密码");
        let shown = if s.show_password { s.password.clone() } else { "•".repeat(s.password.chars().count()) };
        let (below, typed) = widgets::text_field(&mut c, t, f, y, &shown);
        out.push(Hit { rect: field(y), action: Action::TypeInto(Field::Password) });
        if a.field == Field::Password {
            caret = typed;
        }
        let show = widgets::list(&mut c, t, f, below + m.card_gap, &[Row::new("显示密码").trail(Trail::Switch(s.show_password))]);
        out.extend(hits(&show.rows, &[Action::Toggle(Switch::ShowPassword)]));
    }
    keyboard_hits(&mut c, t, f, s, "连接", &mut out);
    if !a.choosing {
        return Frame { caret: Some(caret), ..Frame::new(t, c, out, "添加网络", top) };
    }
    // A touch anywhere off the list closes it.
    out.clear();
    out.push(Hit { rect: Rect::new(0.0, 0.0, m.w as f32, m.h as f32), action: Action::CloseChoices });
    let names = ADDABLE.map(fmt::security);
    let chosen = ADDABLE.iter().position(|&p| p == a.security);
    let held = ADDABLE.iter().position(|&p| s.held == Some(Action::PickSecurity(p)));
    let list = widgets::choices(&mut c, t, f, security.rows[0], &names, chosen, held);
    out.extend(hits(&list, &ADDABLE.map(Action::PickSecurity)));
    Frame::new(t, c, out, "添加网络", top)
}

/// A network's details, in the stock page's grid: for the one chosen, how
/// joining it goes and what it gave, and forgetting it; for any other, how
/// strong and how protected it is.
pub fn wlan_network(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let network = s.details.as_ref().expect("the details page is of a network");
    let joined = s.wlan.as_ref().and_then(|w| w.joined.as_ref()).filter(|j| j.ssid == network.ssid);
    let y = widgets::section_first(&mut c, t, f, top, &network.name);
    let detail = |icon, title, value: String| Detail { icon, title, value };
    let security = detail(Outline::Shield, "安全性", fmt::security(network.security).to_string());
    let Some(j) = joined else {
        let items = [
            detail(Outline::Wifi, "频段", fmt::bands(network.bands)),
            detail(Outline::Signal, "信号强度", fmt::signal(network.signal).to_string()),
            security,
        ];
        let y = widgets::detail_grid(&mut c, t, f, y, &items);
        return Frame::new(t, fit(c, t, y), out, "网络详情", top);
    };
    let mut items = vec![detail(Outline::Info, "状态信息", fmt::joining(&j.stage).to_string())];
    match &j.stage {
        Joining::Connected(link) => {
            if let Some(n) = link.generation {
                items.push(detail(Outline::Wifi, "技术标准", fmt::generation(n)));
            }
            if let Some(speed) = link.speed {
                items.push(detail(Outline::Speed, "连接速度", format!("{speed} Mbps")));
            }
            items.push(detail(Outline::Signal, "信号强度", fmt::signal(link.signal).to_string()));
            items.push(security);
            items.push(detail(Outline::Address, "IP 地址", link.address.to_string()));
            items.push(detail(Outline::Mask, "子网掩码", link.mask.to_string()));
            if let Some(router) = link.router {
                items.push(detail(Outline::Router, "路由器", router.to_string()));
            }
        }
        _ => {
            items.push(detail(Outline::Signal, "信号强度", fmt::signal(network.signal).to_string()));
            items.push(security);
        }
    }
    let y = widgets::detail_grid(&mut c, t, f, y, &items);
    let forget = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new("删除网络").tint(t.pal.danger).trail(Trail::None)]);
    out.extend(hits(&forget.rows, &[Action::LeaveNetwork]));
    Frame::new(t, fit(c, t, forget.bottom), out, "网络详情", top)
}

/// The file manager, every entry of the folder shown, folders first.
pub fn files(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    // An entry picked to copy or move goes to the folder shown, from the
    // bar held at the bottom.
    let dock = s.clip.as_ref().map(|clip| {
        let go = match clip.op {
            FileOp::Copy => "复制到此处",
            FileOp::Move => "移动到此处",
        };
        let (canvas, rects) = widgets::dock_bar(t, f, files::page(t), &["取消", go]);
        Dock { canvas, hits: hits(&rects, &[Action::Unpick, Action::Paste]) }
    });
    let button = Some((widgets::Glyph::Terminal, Action::TerminalHere));
    Frame { dock, button, ..listing(cx, &s.files, "根目录", None, "暂无文件", Action::OpenEntry, "文件管理") }
}

/// A place's folders and the files in them that can be installed, to
/// choose one. A USB drive's page waits for one to be plugged in, and the
/// drive's files show as soon as it is.
pub fn choose(cx: &Ctx, place: Place) -> Frame {
    let s = cx.state;
    let unreadable;
    let absent = match (place, &s.usb_drive) {
        (Place::UsbDrive, None) => Some(("未连接 U 盘", "通过 OTG 连接 U 盘后，将在此显示其中的文件。")),
        (Place::UsbDrive, Some(UsbDrive::Unreadable(why))) => {
            unreadable = format!("请使用 FAT32 或 exFAT 格式的 U 盘。{why}");
            Some(("无法读取该 U 盘", unreadable.as_str()))
        }
        _ => None,
    };
    listing(cx, s.folder(Screen::Choose(place)), place_name(place), absent, "暂无可安装的文件", Action::ChooseEntry, "选择文件")
}

/// A folder laid out as the stock file manager lays one out: the path as
/// capsules from `top`, held under the bar once scrolled there, then the
/// entries in one list on the plain page, each row drawn as it scrolls into
/// view and touched for `action`. In place of the list, `absent` says why
/// nothing of the folder can be shown, and `empty` that it holds nothing.
fn listing(
    cx: &Ctx,
    folder: &Folder,
    top: &str,
    absent: Option<(&str, &str)>,
    empty: &str,
    action: fn(usize) -> Action,
    title: &'static str,
) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let bg = files::page(t);
    let mut c = sheet(cx, t.m.h, bg);
    let rows = match (absent, &folder.entries) {
        (Some((note, why)), _) => {
            files::note(&mut c, t, f, note, Some(why));
            None
        }
        (None, Err(why)) => {
            files::note(&mut c, t, f, "无法读取该文件夹", Some(why));
            None
        }
        (None, Ok(entries)) if entries.is_empty() => {
            files::note(&mut c, t, f, empty, None);
            None
        }
        (None, Ok(entries)) => Some(files::rows(t, f, entries, action)),
    };
    let pinned = Some(files::strip(t, f, top, &folder.path));
    Frame { bg, rows, pinned, ..Frame::new(t, c, Vec::new(), title, t.m.dp(files::STRIP_TOP_DP)) }
}

/// Row `i` of the page's long list over the whole of `c`, sized for it,
/// drawn as the row comes into view.
pub fn row(screen: Screen, cx: &Ctx, i: usize, c: &mut Canvas) {
    match screen {
        Screen::Files | Screen::Choose(_) => {
            let folder = cx.state.folder(screen);
            let entries = folder.entries.as_ref().expect("a folder listed draws its rows");
            files::row(c, cx.t, cx.f, &entries[i], folder.listed, cx.state.time_zone);
        }
        Screen::Installing => widgets::output_row(c, cx.t, cx.f, &cx.state.script_output, i),
        other => unreachable!("{other:?} draws no list row by row"),
    }
}

/// One entry of a folder, a file or a folder: what it is, and what can be
/// done with it. Deleting asks first, with a swipe.
pub fn file_info(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let e = s.file.as_ref().expect("the file page shows an entry");
    let mut c = long_page(cx);
    let top = t.m.content_top;
    let folder = matches!(e.kind, FileKind::Folder { .. });
    let mut facts = vec![("名称", e.name.clone()), ("位置", Path::new(&s.files.path).join(&e.name).to_string_lossy().into_owned())];
    match &e.kind {
        FileKind::Link { target, .. } => facts.push(("指向", target.clone())),
        FileKind::Folder { items: Some(n) } => facts.push(("内容", format!("{n} 项"))),
        FileKind::Folder { items: None } => facts.push(("内容", "无法读取".to_string())),
        FileKind::File | FileKind::Special => facts.push(("大小", format!("{}（{} 字节）", fmt::file_size(e.size), e.size))),
    }
    facts.push(("修改时间", fmt::datetime(e.modified, s.time_zone)));
    facts.push(("权限", format!("{}，所有者 {}，用户组 {}", fmt::mode(e.mode), e.uid, e.gid)));
    let y = widgets::info_card(&mut c, t, f, top, &facts);
    let mut rows = Vec::new();
    let mut acts = Vec::new();
    if e.kind == FileKind::File {
        rows.push(Row::new("编辑"));
        acts.push(Action::EditEntry);
    }
    rows.extend([Row::new("复制"), Row::new("移动"), Row::new("重命名")]);
    acts.extend([Action::PickEntry(FileOp::Copy), Action::PickEntry(FileOp::Move), Action::RenameEntry]);
    // A link's own permissions mean nothing; its target is changed where
    // it is.
    if !matches!(e.kind, FileKind::Link { .. }) {
        rows.push(Row::new("修改权限"));
        acts.push(Action::OpenPermissions);
    }
    rows.push(Row::new("删除").tint(t.pal.danger).trail(Trail::None));
    acts.push(Action::AskDelete);
    let actions = widgets::list(&mut c, t, f, y + t.m.card_gap, &rows);
    let mut out = hits(&actions.rows, &acts);
    let mut bottom = actions.bottom;
    if s.asking_delete {
        let what = if folder {
            format!("删除\"{}\"及其中的全部内容？删除后无法恢复。", e.name)
        } else {
            format!("删除\"{}\"？删除后无法恢复。", e.name)
        };
        let y = widgets::footnote(&mut c, t, f, actions.bottom, &what);
        let (label, danger) = confirm_label(Confirm::DeleteEntry);
        let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
        out.push(Hit { rect: track, action: Action::Confirm(Confirm::DeleteEntry) });
        bottom = track.y + track.h;
    }
    let title = if folder { "文件夹信息" } else { "文件信息" };
    Frame::new(t, fit(c, t, bottom), out, title, top)
}

/// The permission bits of the entry the file page shows, a switch each that
/// takes effect at once: reading, writing and running for its owner, its
/// group and everyone else, then the set-ID and sticky bits.
pub fn permissions(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let e = s.file.as_ref().expect("the permissions page shows an entry");
    let mut c = long_page(cx);
    let top = t.m.content_top;
    let mode = e.mode & 0o7777;
    let mut y = widgets::info_card(&mut c, t, f, top, &[("名称", e.name.clone()), ("权限", format!("{}，{mode:04o}", fmt::mode(e.mode)))]);
    let mut out = Vec::new();
    let classes = [("所有者", 6), ("用户组", 3), ("其他用户", 0)];
    let groups = classes.map(|(title, shift)| (title, [("读取", 4 << shift), ("写入", 2 << shift), ("执行", 1 << shift)]));
    for (title, bits) in groups.into_iter().chain([("特殊权限", [("SUID", 0o4000), ("SGID", 0o2000), ("粘滞位", 0o1000)])]) {
        y = widgets::section(&mut c, t, f, y, title);
        let rows = bits.map(|(name, bit)| Row::new(name).trail(Trail::Switch(mode & bit != 0)));
        let list = widgets::list(&mut c, t, f, y, &rows);
        out.extend(hits(&list.rows, &bits.map(|(_, bit)| Action::ModeBit(bit))));
        y = list.bottom;
    }
    Frame::new(t, fit(c, t, y), out, "修改权限", top)
}

/// A new name for the entry the file page shows, typed on the keyboard.
pub fn rename(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let (y, caret) = widgets::text_field(&mut c, t, f, top, &s.new_name);
    widgets::footnote(&mut c, t, f, y, "名称不能包含\"/\"。点击「完成」保存。");
    keyboard_hits(&mut c, t, f, s, "完成", &mut out);
    Frame { caret: Some(caret), ..Frame::new(t, c, out, "重命名", top) }
}

/// The folder an entry goes to already holds its name: what to do.
pub fn clash(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let (clip, into, _) = s.clash.as_ref().expect("the clash page shows a clash");
    let name = Path::new(&clip.path).file_name().map_or_else(|| clip.path.clone(), |n| n.to_string_lossy().into_owned());
    let verb = match clip.op {
        FileOp::Copy => "复制",
        FileOp::Move => "移动",
    };
    let mut c = page(cx);
    let top = t.m.content_top;
    let y = widgets::paragraph(&mut c, t, f, top, &format!("\"{into}\"中已有同名的\"{name}\"。"));
    let skip = format!("保留已有的同名项，不{verb}该项");
    let choices = widgets::list(&mut c, t, f, y + t.m.dp(12.0), &[
        Row::new("替换").subtitle("覆盖已有的同名项"),
        Row::new("跳过").subtitle(&skip),
        Row::new("保留两者").subtitle("自动为该项重新命名"),
    ]);
    let out = hits(&choices.rows, &[
        Action::Resolve(ClashChoice::Replace),
        Action::Resolve(ClashChoice::Skip),
        Action::Resolve(ClashChoice::KeepBoth),
    ]);
    Frame::new(t, c, out, "名称已存在", top)
}

/// Copying, moving or deleting: how far it has come and a way to stop it,
/// then, if it did not finish, why.
pub fn file_job(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let job = s.job.as_ref().expect("the job page shows work on files");
    let (verb, path, titles, stopped) = match &job.task {
        FileTask::Copy { path, .. } => ("复制", path, ("正在复制", "复制未完成"), "已停止，部分内容可能已复制到目标文件夹。"),
        FileTask::Move { path, .. } => ("移动", path, ("正在移动", "移动未完成"), "已停止，部分内容可能已移动到目标文件夹。"),
        FileTask::Delete { path } => ("删除", path, ("正在删除", "删除未完成"), "已停止，部分内容可能已删除。"),
    };
    let name = Path::new(path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
    let share = |(done, total): (u64, u64)| if total == 0 { 0.0 } else { done as f32 / total as f32 };
    // The bar stays while the work measures, so that the button under it
    // does not move. Deleting, and copying entries that hold no bytes, is
    // counted in entries.
    let (detail, done) = match (job.progress, &job.task) {
        (None, _) => ("正在准备".to_string(), 0.0),
        (Some((bytes, _)), FileTask::Copy { .. } | FileTask::Move { .. }) if bytes.1 > 0 => {
            (format!("已{verb} {}，共 {}", fmt::file_size(bytes.0), fmt::file_size(bytes.1)), share(bytes))
        }
        (Some((_, entries)), _) => (format!("已{verb} {} 项，共 {} 项", entries.0, entries.1), share(entries)),
    };
    let mut c = page(cx);
    let top = t.m.content_top;
    let y = widgets::progress_card(&mut c, t, f, top, &name, &detail, Some(done));
    let mut out = Vec::new();
    match &job.outcome {
        None => {
            let label = if job.stopping { "正在停止" } else { "停止" };
            let stop = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new(label).trail(Trail::None)]);
            if !job.stopping {
                out.extend(hits(&stop.rows, &[Action::StopJob]));
            }
        }
        Some(end) => {
            let why = match end {
                JobEnd::Stopped => stopped.to_string(),
                JobEnd::Failed(why) => format!("{verb}失败：{why}"),
                JobEnd::Done => unreachable!("work done leaves its page"),
            };
            let y = widgets::footnote(&mut c, t, f, y, &why);
            let finish = widgets::list(&mut c, t, f, y + t.m.card_gap, &[Row::new("完成").trail(Trail::None)]);
            out.extend(hits(&finish.rows, &[Action::Back]));
        }
    }
    let title = if job.outcome.is_none() { titles.0 } else { titles.1 };
    Frame { back: job.outcome.is_some(), ..Frame::new(t, c, out, title, top) }
}

/// A shell on a terminal: the screen it writes, the keys a phone keyboard
/// lacks, and the keyboard, every key of which goes to the shell.
pub fn terminal(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    keyboard_hits(&mut c, t, f, s, "回车", &mut out);
    let l = term::layout(t, f, top);
    term::draw(&mut c, t, f, &l, cx.term);
    let held = cx.term.is_some_and(|term| term.ctrl);
    let keys = term::keys(&mut c, t, f, &l, &term::TERM_KEYS, held);
    out.extend(keys.into_iter().map(|(rect, key)| Hit { rect, action: Action::TermKey(key) }));
    Frame { scrollback: Some((l.card, l.pitch)), ..Frame::new(t, c, out, "终端", top) }
}

/// A text file being edited: where it is, its lines wrapped on a card, the
/// keys a phone keyboard lacks for moving the cursor, and the keyboard.
/// Once it is changed, the round button saves it.
pub fn editor(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let ed = s.editor.as_ref().expect("the editor page shows a file");
    let mut c = page(cx);
    let mut out = Vec::new();
    keyboard_hits(&mut c, t, f, s, "换行", &mut out);
    editor::caption(&mut c, t, f, &ed.path);
    let l = editor::layout(t, f);
    let caret = editor::draw(&mut c, t, f, &l, ed);
    let keys = term::keys(&mut c, t, f, &l, &editor::EDITOR_KEYS, false);
    out.extend(keys.into_iter().map(|(rect, key)| Hit { rect, action: Action::TermKey(key) }));
    let button = ed.changed.then_some((widgets::Glyph::Done, Action::SaveText));
    Frame { scrollback: Some((l.card, l.pitch)), caret, button, ..Frame::new(t, c, out, "编辑", t.m.content_top) }
}

/// The interface's languages; only Simplified Chinese is written yet.
pub fn language(cx: &Ctx) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let mut c = page(cx);
    let top = t.m.content_top;
    let list = widgets::list(&mut c, t, f, top, &[
        Row::new("简体中文").trail(Trail::Check(true)),
        Row::new("繁體中文").subtitle("暂未翻译").trail(Trail::Check(false)),
        Row::new("English").subtitle("Not translated yet").trail(Trail::Check(false)),
    ]);
    let out = hits(&list.rows[1..], &[Action::Unavailable("暂不支持该语言"), Action::Unavailable("暂不支持该语言")]);
    widgets::footnote(&mut c, t, f, list.bottom, "目前仅支持简体中文。");
    Frame::new(t, c, out, "语言", top)
}

/// The time zone the status bar's clock and the times on the pages keep.
pub fn time_zone(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = long_page(cx);
    let top = t.m.content_top;
    let rows: Vec<Row> =
        TIME_ZONES.iter().enumerate().map(|(k, &(name, cities, _))| Row::new(name).subtitle(cities).trail(Trail::Check(k == s.time_zone))).collect();
    let list = widgets::list(&mut c, t, f, top, &rows);
    let picks: Vec<Action> = (0..TIME_ZONES.len()).map(Action::TimeZone).collect();
    let out = hits(&list.rows, &picks);
    let bottom = widgets::footnote(&mut c, t, f, list.bottom, "状态栏和各页面中的时间按所选时区显示。");
    Frame::new(t, fit(c, t, bottom), out, "时区", top)
}

/// Saving the logs, as a bug report asks for them.
pub fn logs(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let y = widgets::section_first(&mut c, t, f, top, "日志内容");
    let parts = widgets::list(&mut c, t, f, y, &[
        Row::new("Recovery 日志").subtitle("界面、安装和挂载的记录").trail(Trail::Check(s.logs[0])),
        Row::new("内核日志").subtitle("本次开机以来的内核消息").trail(Trail::Check(s.logs[1])),
    ]);
    out.extend(hits(&parts.rows, &[Action::LogPart(0), Action::LogPart(1)]));
    let y = widgets::section(&mut c, t, f, parts.bottom, "保存到");
    let storage = if cx.state.data.storage_open() { "Skyward/logs 文件夹" } else { "需先完成「解密」" };
    let places = widgets::list(&mut c, t, f, y, &[
        Row::new("临时文件夹").subtitle("/tmp/skyward-logs，可通过 adb pull 导出到电脑").trail(Trail::Check(s.logs_to == 0)),
        Row::new("内部存储").subtitle(storage).trail(Trail::Check(s.logs_to == 1)),
    ]);
    out.extend(hits(&places.rows, &[Action::LogsTo(0), Action::LogsTo(1)]));
    let y = widgets::footnote(&mut c, t, f, places.bottom, "每次导出保存在以导出时间命名的文件夹中。重启后将清空临时文件夹。");
    let export = widgets::button(&mut c, t, f, y + t.m.dp(24.0), "导出");
    out.push(Hit { rect: export, action: Action::ExportLogs });
    Frame::new(t, c, out, "导出日志", top)
}

/// Skyward's licence, in Chinese and in the words the licence gives for a
/// program's notice.
const LICENSE_NOTICE: &str = "Skyward 是自由软件：你可以依据自由软件基金会发布的 GNU 通用公共许可证第 3 版，或（由你选择）任何更新的版本，重新发布和修改它。\n\n\
发布 Skyward 是希望它有用，但不附带任何担保，也不担保其适销性或适用于特定用途。详见 GNU 通用公共许可证。";

const LICENSE_ORIGINAL: &str = "Skyward\nCopyright (C) 2026 Kirk Lin\n\n\
This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.\n\n\
This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.\n\n\
You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.";

/// Skyward's own licence.
pub fn license(cx: &Ctx) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let mut c = long_page(cx);
    let top = t.m.content_top;
    let y = widgets::info_card(&mut c, t, f, top, &[("许可证", "GNU 通用公共许可证第 3 版或更新的版本（GPL-3.0-or-later）".to_string())]);
    let y = widgets::paragraph(&mut c, t, f, y + t.m.dp(4.0), LICENSE_NOTICE);
    let y = widgets::section(&mut c, t, f, y, "原文");
    let bottom = widgets::paragraph(&mut c, t, f, y - t.m.dp(16.0), LICENSE_ORIGINAL);
    Frame::new(t, fit(c, t, bottom), Vec::new(), "开源许可", top)
}

/// The monospaced typeface's licence, which names the copyright holders on
/// its first lines; the image carries the same file beside the font.
const MONO_LICENSE: &str = include_str!("../../../resources/fonts/KirkMono-OFL.txt");

/// The typefaces, as they name themselves, and the terms each is used under.
pub fn font_license(cx: &Ctx) -> Frame {
    let (t, f) = (cx.t, cx.f);
    let mut c = long_page(cx);
    let top = t.m.content_top;
    let y = widgets::section_first(&mut c, t, f, top, "界面文字");
    let y = widgets::info_card(&mut c, t, f, y, &[
        ("字体", f.info.family.clone()),
        ("版本", f.info.version.clone()),
        ("版权", f.info.copyright.clone().expect("MiSans names its copyright holder")),
    ]);
    let y = widgets::paragraph(
        &mut c,
        t,
        f,
        y + t.m.dp(4.0),
        "界面文字使用小米 MiSans 字体，依据《MiSans 字体知识产权许可协议》免费使用。字体文件未经修改，仅随 Skyward 提供，不单独分发。\n\n协议全文可在 MiSans 官方网站查看。",
    );
    let y = widgets::section(&mut c, t, f, y, "终端文字");
    let holders: Vec<&str> = MONO_LICENSE.lines().take_while(|l| l.starts_with("Copyright")).collect();
    let y = widgets::info_card(&mut c, t, f, y, &[
        ("字体", f.mono_info.family.clone()),
        ("版本", f.mono_info.version.clone()),
        ("版权", holders.join("\n")),
        ("许可证", "SIL 开源字体许可证 1.1 版（OFL-1.1）".to_string()),
    ]);
    let bottom = widgets::paragraph(
        &mut c,
        t,
        f,
        y + t.m.dp(4.0),
        "终端文字使用 Kirk Mono 等宽字体。该许可证允许自由使用、修改和再发布字体，但不允许单独出售字体。\n\n许可证全文位于 /skyward/KirkMono-OFL.txt，也可在 openfontlicense.org 查看。",
    );
    Frame::new(t, fit(c, t, bottom), Vec::new(), "界面字体", top)
}

/// Which slot the phone starts from next, as the boot control holds it,
/// and what it holds of each; picking the other and swiping switches.
/// While an update is in flight the slot stays as the update set it.
pub fn slot(cx: &Ctx) -> Frame {
    let (t, f, s, i) = (cx.t, cx.f, cx.state, cx.info);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let Some(current) = i.slot else {
        widgets::list(&mut c, t, f, top, &[Row::new("本机没有 A/B 槽位").trail(Trail::None)]);
        return Frame::new(t, c, out, "启动槽位", top);
    };
    let slots = match &s.boot_slots {
        Some(Ok(slots)) => *slots,
        Some(Err(why)) => {
            let list = widgets::list(&mut c, t, f, top, &[Row::new("无法读取启动槽位").trail(Trail::None)]);
            widgets::footnote(&mut c, t, f, list.bottom, why);
            return Frame::new(t, c, out, "启动槽位", top);
        }
        None => return Frame::new(t, c, out, "启动槽位", top),
    };
    let chosen = s.next_slot.unwrap_or(slots.active);
    let y = widgets::section_first(&mut c, t, f, top, "下次启动");
    let titles = [Slot::A, Slot::B].map(|slot| format!("槽位 {}", slot.letter()));
    let subs = [Slot::A, Slot::B].map(|slot| {
        let k = slot.number() as usize;
        let which = if slot == current { "当前运行的系统" } else { "另一个槽位" };
        match (slots.bootable[k], slots.successful[k]) {
            (false, _) => format!("{which}，已标记为无法启动"),
            (true, false) => format!("{which}，尚未完整启动过"),
            (true, true) => which.to_string(),
        }
    });
    let rows: Vec<Row> =
        [Slot::A, Slot::B].iter().enumerate().map(|(k, &slot)| Row::new(&titles[k]).subtitle(&subs[k]).trail(Trail::Check(slot == chosen))).collect();
    let list = widgets::list(&mut c, t, f, y, &rows);
    out.extend(hits(&list.rows, &[Slot::A, Slot::B].map(Action::PickSlot)));
    if let Some(note) = fmt::snapshot_slot_note(&i.snapshot) {
        widgets::footnote(&mut c, t, f, list.bottom, note);
        return Frame::new(t, c, out, "启动槽位", top);
    }
    let y = widgets::footnote(&mut c, t, f, list.bottom, "另一个槽位中没有完整的系统时，切换后将无法开机。");
    let (label, danger) = confirm_label(Confirm::Slot);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::Slot) });
    Frame::new(t, c, out, "启动槽位", top)
}

/// The phone's own fastbootd serving the computer: whether it runs, how far
/// the computer has the port, and where to go from here, chosen with the
/// keys and lit where they are.
pub fn fastboot(cx: &Ctx) -> Frame {
    let (t, f, s, i) = (cx.t, cx.f, cx.state, cx.info);
    let mut c = page(cx);
    let top = t.m.content_top;
    let running = if s.fastboot.running { "正在运行" } else { "未运行" };
    let link = match s.fastboot.link {
        UsbLink::Connected => "已连接电脑",
        UsbLink::Connecting => "正在连接",
        UsbLink::Unplugged => "未连接电脑",
    };
    let slot = i.slot.map(|s| s.letter());
    let mut facts = vec![
        Row::new("fastbootd").trail(Trail::Text(running)),
        Row::new("USB").trail(Trail::Text(link)),
        Row::new("设备代号").trail(Trail::Text(&i.codename)),
    ];
    if let Some(letter) = slot {
        facts.push(Row::new("当前槽位").trail(Trail::Text(letter)));
    }
    let y = widgets::section_first(&mut c, t, f, top, "状态");
    let state = widgets::list(&mut c, t, f, y, &facts);
    let y = widgets::footnote(&mut c, t, f, state.bottom, "你可以在电脑上使用 fastboot 命令刷写分区，包括 super 中的逻辑分区。");
    let y = widgets::section(&mut c, t, f, y, "退出 fastboot");
    let rows: Vec<Row> = FASTBOOT_CHOICES.iter().map(|&(title, _)| Row::new(title).trail(Trail::None)).collect();
    let choices = widgets::list(&mut c, t, f, y, &rows);
    let actions: Vec<Action> = FASTBOOT_CHOICES.iter().map(|&(_, action)| action).collect();
    let out = hits(&choices.rows, &actions);
    widgets::footnote(&mut c, t, f, choices.bottom, "按音量键选择，按电源键确认。");
    Frame { back: false, ..Frame::new(t, c, out, "Fastboot", top) }
}

/// Where to go once what the system asked for has stopped or cannot be
/// done, in the order the pages list them.
const AFTERWARDS: [(&str, Action); 3] = [
    ("重启到系统", Action::RebootTo(Target::System)),
    ("进入恢复模式", Action::Dismiss),
    ("关机", Action::RebootTo(Target::PowerOff)),
];

/// The list of [`AFTERWARDS`] at `y`, and where it ends. Work the user
/// chose in the recovery goes back to its home page, rather than into it.
fn afterwards(c: &mut Canvas, t: &Theme, f: &Fonts, y: f32, asked: bool, out: &mut Vec<Hit>) -> f32 {
    let rows: Vec<Row> = AFTERWARDS
        .iter()
        .map(|&(title, action)| Row::new(if action == Action::Dismiss && !asked { "返回主页" } else { title }).trail(Trail::None))
        .collect();
    let choices = widgets::list(c, t, f, y, &rows);
    out.extend(hits(&choices.rows, &AFTERWARDS.map(|(_, action)| action)));
    choices.bottom
}

/// Erasing: each part and how far it has come, and once erasing has
/// stopped, why. What the system asked for leaves on its own once it is
/// done, and offers where to go only if it stops; what the user chose
/// offers where to go once it ends either way.
pub fn erase(cx: &Ctx) -> Frame {
    use std::cmp::Ordering;
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let e = &s.erase;
    let mut c = long_page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let failed = matches!(e.outcome, Some(Err(_)));
    // Only a part that went wrong was under way; whatever else stopped
    // erasing stopped it before any part began.
    let begun = !failed || matches!(e.outcome, Some(Err(EraseStop::Failed(..))));
    let heading = match &e.outcome {
        None => "正在清除",
        Some(Ok(())) => "已清除",
        Some(Err(_)) => "清除未完成",
    };
    let mut y = widgets::section_first(&mut c, t, f, top, heading);
    if !e.parts.is_empty() {
        let states: Vec<String> = (0..e.parts.len())
            .map(|k| match (k.cmp(&e.done), failed) {
                (Ordering::Less, _) => "已清除".into(),
                (Ordering::Equal, false) => match e.progress {
                    Some((done, total)) if total > 0 => format!("正在清除 {}%", percent(share(done, total))),
                    _ => "正在清除".into(),
                },
                (Ordering::Equal, true) if begun => "未完成".into(),
                (Ordering::Greater, false) => "等待中".into(),
                (Ordering::Equal | Ordering::Greater, true) => "未清除".into(),
            })
            .collect();
        let rows: Vec<Row> = e.parts.iter().zip(&states).map(|(&part, state)| Row::new(fmt::part(part)).trail(Trail::Text(state))).collect();
        y = widgets::list(&mut c, t, f, y, &rows).bottom;
    }
    let end = if s.asked.shutdown_after { "关机" } else { "重启" };
    let notes = match (&e.outcome, e.asked, e.reset) {
        (Some(Err(stop)), _, _) => {
            let (what, detail) = fmt::erase_stop(stop);
            std::iter::once(what).chain(detail).collect()
        }
        (_, true, true) => vec![format!("本机的全部数据将被清除，完成后将自动{end}。")],
        (_, true, false) => vec![format!("完成后将自动{end}。")],
        (None, false, _) => vec!["清除完成前，请不要关机。".to_string()],
        (Some(Ok(())), false, true) => vec!["下次开机时将重新创建 data 分区，启动时间较长。".to_string()],
        (Some(Ok(())), false, false) => Vec::new(),
    };
    for note in &notes {
        y = widgets::footnote(&mut c, t, f, y, note);
    }
    if let (true, Some(reason)) = (e.asked, &s.asked.reason) {
        y = widgets::info_card(&mut c, t, f, y + t.m.dp(12.0), &[("原因", reason.clone())]);
    }
    if failed || (!e.asked && e.outcome.is_some()) {
        let below = widgets::section(&mut c, t, f, y, "操作");
        y = afterwards(&mut c, t, f, below, e.asked, &mut out);
    }
    let title = match (e.reset, e.asked) {
        (true, _) => "恢复出厂设置",
        (false, true) => "清除缓存",
        (false, false) => "清除数据",
    };
    Frame { back: false, ..Frame::new(t, fit(c, t, y), out, title, top) }
}

/// The system could not start Android and asks whether to erase all user
/// data: start it again, look around the recovery first, or erase.
pub fn cant_start(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let mut y = widgets::paragraph(
        &mut c,
        t,
        f,
        top,
        "无法启动系统，本机数据可能已损坏。如果每次启动都停留在此页面，可恢复出厂设置，这将清除本机的全部数据。",
    );
    if let Some(reason) = &s.asked.reason {
        y = widgets::info_card(&mut c, t, f, y + t.m.dp(12.0), &[("原因", reason.clone())]);
    }
    y = widgets::section(&mut c, t, f, y, "操作");
    let rows = [
        Row::new("重试").subtitle("重新启动系统").trail(Trail::None),
        Row::new("进入恢复模式").subtitle("不清除任何内容").trail(Trail::None),
    ];
    let choices = widgets::list(&mut c, t, f, y, &rows);
    out.extend(hits(&choices.rows, &[Action::RebootTo(Target::System), Action::Dismiss]));
    let y = widgets::footnote(&mut c, t, f, choices.bottom, "恢复出厂设置后，数据无法恢复。");
    let (label, danger) = confirm_label(Confirm::FactoryReset);
    let track = widgets::confirm(&mut c, t, f, y + t.m.dp(24.0), label, danger, s.confirm);
    out.push(Hit { rect: track, action: Action::Confirm(Confirm::FactoryReset) });
    Frame { back: false, ..Frame::new(t, c, out, "无法启动系统", top) }
}

/// Something the system asked of the recovery that it does not do: what
/// was asked, and where to go instead.
pub fn unhandled(cx: &Ctx) -> Frame {
    let (t, f, s) = (cx.t, cx.f, cx.state);
    let mut c = page(cx);
    let mut out = Vec::new();
    let top = t.m.content_top;
    let mut items = vec![("请求", s.asked.unhandled.clone().unwrap_or_default())];
    if let Some(reason) = &s.asked.reason {
        items.push(("原因", reason.clone()));
    }
    let y = widgets::info_card(&mut c, t, f, top, &items);
    let y = widgets::footnote(&mut c, t, f, y, "Skyward 暂不支持该请求，本机未做任何改动。");
    let y = widgets::section(&mut c, t, f, y, "操作");
    afterwards(&mut c, t, f, y, true, &mut out);
    Frame { back: false, ..Frame::new(t, c, out, "无法处理该请求", top) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::BackupRefusal;

    fn job(restore: bool, checks: bool) -> BackupJob {
        let plan = vec![("boot".into(), 200), ("system".into(), 800)];
        BackupJob { restore, place: Place::Storage, checks, plan, at: None, outcome: None, stopping: false }
    }

    fn at(item: usize, done: u64, of: u64, checking: bool) -> BackupAt {
        BackupAt { item, done, of, checking }
    }

    #[test]
    fn a_script_shows_its_last_line_until_skyward_goes_back() {
        let said = ["- Flashing boot".to_string(), "  ".to_string()];
        assert_eq!(script_stage(None, &[]), ("正在运行安装脚本", None));
        assert_eq!(script_stage(Some(&Stage::Script { thousandths: 250 }), &said), ("- Flashing boot", Some(0.25)), "past a blank line");
        assert_eq!(script_stage(Some(&Stage::Reflashing { done: 1, total: 4 }), &said), (REFLASHING, Some(0.25)));
        assert_eq!(update_stage(Some(&Stage::Reflashing { done: 3, total: 4 })), (REFLASHING.to_string(), Some(0.75)));
    }

    #[test]
    fn a_backup_checks_its_parts_last_and_a_restore_first() {
        // Halfway through writing system, 600 of the 1000 bytes: the first
        // of two passes, then the second.
        assert_eq!(job_progress(&job(false, true), &at(1, 400, 800, false)), (0.3, 600));
        assert_eq!(job_progress(&job(false, true), &at(1, 400, 800, true)), (0.8, 600));
        assert_eq!(job_progress(&job(false, false), &at(1, 400, 800, false)), (0.6, 600), "without a check, writing is all");
        assert_eq!(job_progress(&job(true, true), &at(0, 100, 200, true)), (0.05, 100));
        assert_eq!(job_progress(&job(true, true), &at(0, 100, 200, false)), (0.55, 100));
    }

    #[test]
    fn each_part_says_where_it_stands() {
        let states = |j: &BackupJob| (0..2).map(|k| part_state(j, k)).collect::<Vec<_>>();
        let mut b = job(false, true);
        assert_eq!(states(&b), ["等待中", "等待中"]);
        b.at = Some(at(1, 400, 800, false));
        assert_eq!(states(&b), ["已备份", "正在备份 50%"]);
        b.at = Some(at(0, 50, 200, true));
        assert_eq!(states(&b), ["正在校验 25%", "已备份"]);
        b.outcome = Some(BackupEnd::Failed("the backup does not hold what was stored".into()));
        assert_eq!(states(&b), ["未完成", "未校验"]);
        b.outcome = Some(BackupEnd::Saved("20261021-141639".into()));
        assert_eq!(states(&b), ["已校验", "已校验"]);
        let mut r = job(true, true);
        r.at = Some(at(1, 0, 800, true));
        assert_eq!(states(&r), ["已校验", "正在校验 0%"]);
        r.at = Some(at(0, 200, 400, false));
        assert_eq!(states(&r), ["正在恢复 50%", "已校验"]);
        r.outcome = Some(BackupEnd::Failed("write /dev/block/by-name/boot_b".into()));
        assert_eq!(states(&r), ["未完成", "未恢复"]);
        r.at = None;
        r.outcome = Some(BackupEnd::Refused(BackupRefusal::DataReset));
        assert_eq!(states(&r), ["未开始", "未开始"]);
    }
}
