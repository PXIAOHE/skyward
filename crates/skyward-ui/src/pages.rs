use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::font::Fonts;
use crate::theme::Theme;
use crate::widgets::{self, Icon, Row, Trail};

// Tile colours sampled from the stock Settings, plus orange and red from the
// same family for the destructive and reboot entries.
const BLUE: Rgba = Rgba::rgb(74, 128, 247);
const GREEN: Rgba = Rgba::rgb(97, 202, 81);
const PURPLE: Rgba = Rgba::rgb(116, 104, 240);
const SLATE: Rgba = Rgba::rgb(143, 156, 174);
const ORANGE: Rgba = Rgba::rgb(255, 146, 46);
const RED: Rgba = Rgba::rgb(242, 83, 75);

fn page(t: &Theme) -> Canvas {
    let mut c = Canvas::new(t.m.w, t.m.h);
    c.clear(t.pal.bg);
    c
}

/// Recovery home, laid out like the Settings root: title, search, a device
/// card, then grouped entries.
pub fn home(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let mut y = widgets::header(&mut c, t, f, "Skyward", false);
    y = widgets::search_bar(&mut c, t, f, y, "搜索功能与分区");
    y = widgets::device_card(&mut c, t, f, y, "Xiaomi 17 Ultra", "OS4.0.4.0 · BL 已解锁", &[
        Row::new("当前槽位").icon(PURPLE, Icon::Layers).trail(Trail::Badge("B")),
    ]);
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
        Row::new("高级").icon(SLATE, Icon::Gear),
        Row::new("重启").icon(ORANGE, Icon::Power).trail(Trail::Value("系统")),
    ]);
    c
}

/// Settings sub-page modelled on the stock「显示与亮度」.
pub fn settings(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let top = widgets::header(&mut c, t, f, "设置", true);
    let mut y = widgets::section_first(&mut c, t, f, top, "通用");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("语言").icon(BLUE, Icon::Globe).trail(Trail::Value("简体中文")),
        Row::new("时区").icon(ORANGE, Icon::Clock).trail(Trail::Value("中国标准时间")),
    ]);
    y = widgets::section(&mut c, t, f, y, "显示");
    y = widgets::slider_card(&mut c, t, f, y, "屏幕亮度", 0.62, Some(Row::new("深色模式").trail(Trail::Switch(t.dark))));
    y = widgets::section(&mut c, t, f, y, "调试");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("ADB Sideload").icon(GREEN, Icon::Usb).subtitle("通过电脑上的 adb 推送安装包").trail(Trail::Switch(false)),
        Row::new("终端").icon(SLATE, Icon::Terminal),
    ]);
    y = widgets::section(&mut c, t, f, y, "关于");
    widgets::list(&mut c, t, f, y, &[Row::new("界面字体").trail(Trail::Text("MiSans"))]);
    c
}

/// File-based-encryption unlock.
pub fn decrypt(t: &Theme, f: &Fonts, path: &[usize]) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);
    let cx = m.w as f32 / 2.0;
    let lock_cy = m.dp(150.0);
    c.fill_circle(cx, lock_cy, m.dp(30.0), t.pal.card);
    widgets::glyph(&mut c, Icon::Lock, cx, lock_cy, m.dp(40.0), t.pal.accent, t.pal.card);
    c.text_center(&f.heading, "输入锁屏图案", cx, lock_cy + m.dp(64.0), m.dp(24.0), t.pal.text);
    c.text_center(&f.body, "解锁后才能读取「数据」分区", cx, lock_cy + m.dp(98.0), m.sub_px, t.pal.subtitle);
    widgets::pattern_lock(&mut c, t, cx, m.h as f32 * 0.6, m.dp(190.0), path);
    c.text_center(&f.heading, "暂不解密", cx, m.h as f32 - m.dp(64.0), m.row_title_px, t.pal.accent);
    c
}

/// Reboot target picker with a swipe-to-confirm control.
pub fn reboot(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let top = widgets::header(&mut c, t, f, "重启", true);
    let mut y = widgets::section_first(&mut c, t, f, top, "重启到");
    y = widgets::list(&mut c, t, f, y, &[
        Row::new("系统").trail(Trail::Check(true)),
        Row::new("恢复模式").trail(Trail::Check(false)),
        Row::new("Fastboot").subtitle("用户空间 fastbootd").trail(Trail::Check(false)),
        Row::new("Bootloader").trail(Trail::Check(false)),
        Row::new("EDL").subtitle("高通紧急下载模式").trail(Trail::Check(false)),
        Row::new("关机").trail(Trail::Check(false)),
    ]);
    y = widgets::section(&mut c, t, f, y, "槽位");
    y = widgets::list(&mut c, t, f, y, &[Row::new("下次启动").trail(Trail::Value("B（当前）"))]);
    y += t.m.dp(28.0);
    widgets::confirm(&mut c, t, f, y, "滑动以重启", false);
    c
}

/// Destructive wipe with the partitions to clear.
pub fn wipe(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let top = widgets::header(&mut c, t, f, "清除数据", true);
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
pub fn install_progress(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    let top = widgets::header(&mut c, t, f, "安装", true);
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
