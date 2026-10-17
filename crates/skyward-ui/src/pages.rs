use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::font::Fonts;
use crate::theme::Theme;
use crate::widgets::{self, Icon, RowSpec, Trailing};

// Icon chip colours in the style of the stock settings.
const C_GLOBE: Rgba = Rgba::rgb(45, 130, 245);
const C_CLOCK: Rgba = Rgba::rgb(255, 149, 54);
const C_SUN: Rgba = Rgba::rgb(255, 196, 46);
const C_MOON: Rgba = Rgba::rgb(104, 110, 214);
const C_USB: Rgba = Rgba::rgb(38, 190, 170);
const C_TERM: Rgba = Rgba::rgb(96, 102, 112);
const C_INSTALL: Rgba = Rgba::rgb(54, 190, 110);
const C_BACKUP: Rgba = Rgba::rgb(45, 130, 245);
const C_RESTORE: Rgba = Rgba::rgb(38, 190, 170);
const C_WIPE: Rgba = Rgba::rgb(240, 74, 74);
const C_MOUNT: Rgba = Rgba::rgb(150, 110, 220);
const C_ADV: Rgba = Rgba::rgb(118, 124, 134);

fn page(t: &Theme) -> Canvas {
    let mut c = Canvas::new(t.m.w, t.m.h);
    c.clear(t.pal.bg);
    c
}

fn page_title(c: &mut Canvas, t: &Theme, f: &Fonts, title: &str) -> f32 {
    let m = &t.m;
    let y = m.status_h + m.big_title * 0.5;
    c.text(&f.title, title, m.side, y + m.page_title * 0.3, m.page_title, t.pal.text, None);
    y + m.page_title
}

/// FBE unlock screen — the first thing recovery shows when `/data` is encrypted.
pub fn decrypt(t: &Theme, f: &Fonts, path: &[usize]) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);
    let cx = m.w as f32 / 2.0;

    let lock_cy = m.status_h + m.scale * 180.0;
    c.fill_circle(cx, lock_cy, m.scale * 96.0, t.pal.card);
    c.round_rect_shadow(cx - m.scale * 96.0, lock_cy - m.scale * 96.0, m.scale * 192.0, m.scale * 192.0, m.scale * 96.0, m.scale * 24.0, m.scale * 8.0, t.pal.shadow);
    c.fill_circle(cx, lock_cy, m.scale * 96.0, t.pal.card);
    widgets::draw_icon(&mut c, Icon::Lock, cx, lock_cy, m.scale * 150.0, t.pal.accent, t.pal.card);

    c.text_center(&f.title, "输入图案以解密数据", cx, lock_cy + m.scale * 190.0, m.page_title, t.pal.text);
    c.text_center(&f.regular, "Xiaomi 17 Ultra · 槽位 B", cx, lock_cy + m.scale * 190.0 + m.sub * 1.6, m.sub, t.pal.secondary);

    widgets::pattern_lock(&mut c, t, cx, m.h as f32 * 0.6, m.scale * 560.0, path);

    c.text_center(&f.regular, "解锁后其余用户的数据随之可用", cx, m.h as f32 - m.scale * 140.0, m.sub, t.pal.tertiary);
    c
}

/// Recovery home — big title, device line, and the action grid.
pub fn home(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);

    let ty = m.status_h + m.big_title * 0.9;
    c.text(&f.title, "Skyward", m.side, ty, m.big_title, t.pal.text, None);
    c.text(&f.regular, "Xiaomi 17 Ultra · 槽位 B · 已解锁", m.side, ty + m.sub * 1.7, m.sub, t.pal.secondary, None);

    let items = [
        (C_INSTALL, Icon::Install, "安装", "刷入安装包"),
        (C_BACKUP, Icon::Backup, "备份", "保存当前系统"),
        (C_RESTORE, Icon::Restore, "恢复", "还原备份"),
        (C_WIPE, Icon::Wipe, "清除", "清除数据"),
        (C_MOUNT, Icon::Mount, "挂载", "挂载分区"),
        (C_ADV, Icon::Gear, "高级", "工具与设置"),
    ];
    let gap = m.group_gap;
    let cw = (m.w as f32 - m.side * 2.0 - gap) / 2.0;
    let ch = m.scale * 300.0;
    let top = ty + m.sub * 3.4;
    for (i, (col, icon, title, sub)) in items.iter().enumerate() {
        let (r, cc) = (i / 2, i % 2);
        let x = m.side + cc as f32 * (cw + gap);
        let y = top + r as f32 * (ch + gap);
        c.round_rect_shadow(x, y, cw, ch, m.card_radius, m.scale * 26.0, m.scale * 10.0, t.pal.shadow);
        c.fill_round_rect(x, y, cw, ch, m.card_radius, t.pal.card);
        let chip = m.chip * 1.5;
        widgets::icon_chip(&mut c, t, x + m.side, y + m.side * 1.2, chip, *col, *icon);
        c.text(&f.title, title, x + m.side, y + ch - m.scale * 92.0, m.row_title * 1.08, t.pal.text, None);
        c.text(&f.regular, sub, x + m.side, y + ch - m.scale * 38.0, m.sub, t.pal.secondary, None);
    }
    widgets::bottom_nav(&mut c, t, 0);
    c
}

/// Grouped settings list with a brightness slider and toggles.
pub fn settings(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);
    let mut y = page_title(&mut c, t, f, "设置");
    y += m.group_gap * 0.6;
    let x = m.side;
    let w = m.w as f32 - m.side * 2.0;

    y += widgets::card_group(&mut c, t, f, x, y, w, &[
        RowSpec { icon: Some((C_GLOBE, Icon::Globe)), title: "语言", sub: None, trailing: Trailing::Text("简体中文") },
        RowSpec { icon: Some((C_CLOCK, Icon::Clock)), title: "时区", sub: None, trailing: Trailing::Text("GMT+8") },
    ]);
    y += m.group_gap;

    // Brightness slider card.
    let bh = m.row_h * 1.15;
    c.round_rect_shadow(x, y, w, bh, m.card_radius, m.scale * 26.0, m.scale * 10.0, t.pal.shadow);
    c.fill_round_rect(x, y, w, bh, m.card_radius, t.pal.card);
    widgets::icon_chip(&mut c, t, x + m.side * 0.8, y + (bh - m.chip) / 2.0, m.chip, C_SUN, Icon::Sun);
    c.text(&f.regular, "屏幕亮度", x + m.side * 0.8 + m.chip + m.side * 0.55, y + m.row_title * 1.2, m.row_title, t.pal.text, None);
    widgets::slider(&mut c, t, x + m.side * 0.8 + m.chip + m.side * 0.55, y + bh * 0.66, w - (m.chip + m.side * 2.6), 0.6);
    y += bh + m.group_gap;

    y += widgets::card_group(&mut c, t, f, x, y, w, &[
        RowSpec { icon: Some((C_MOON, Icon::Moon)), title: "深色模式", sub: None, trailing: Trailing::Switch(t.dark) },
        RowSpec { icon: Some((C_USB, Icon::Usb)), title: "ADB Sideload", sub: Some("通过 adb 推送安装包"), trailing: Trailing::Switch(true) },
        RowSpec { icon: Some((C_TERM, Icon::Terminal)), title: "终端", sub: None, trailing: Trailing::Arrow },
    ]);
    let _ = y;
    widgets::bottom_nav(&mut c, t, 2);
    c
}

/// Reboot targets with a swipe-to-confirm track. Targets use centered text and
/// no icons, matching the device's reboot panel.
pub fn reboot(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);
    let mut y = page_title(&mut c, t, f, "重启");
    y += m.group_gap * 0.6;
    let x = m.side;
    let w = m.w as f32 - m.side * 2.0;

    let targets = ["系统", "关机", "恢复模式", "Fastboot", "Bootloader", "EDL"];
    let rh = m.row_h * 0.86;
    let ch = rh * targets.len() as f32;
    c.round_rect_shadow(x, y, w, ch, m.card_radius, m.scale * 26.0, m.scale * 10.0, t.pal.shadow);
    c.fill_round_rect(x, y, w, ch, m.card_radius, t.pal.card);
    for (i, tgt) in targets.iter().enumerate() {
        let ry = y + rh * i as f32;
        let col = if i == 0 { t.pal.accent } else { t.pal.text };
        c.text_center(&f.regular, tgt, x + w / 2.0, ry + rh / 2.0 + m.row_title * 0.36, m.row_title, col);
        if i + 1 < targets.len() {
            c.fill_rect(x + m.side, ry + rh - m.scale, w - m.side * 2.0, m.scale, t.pal.separator);
        }
    }
    y += ch + m.group_gap * 1.4;
    widgets::confirm_slider(&mut c, t, f, x, y, w, m.row_h * 0.95, "滑动确认重启到系统", false);
    c
}

/// Destructive wipe confirmation.
pub fn wipe(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);
    let mut y = page_title(&mut c, t, f, "清除数据");
    y += m.group_gap * 0.6;
    let x = m.side;
    let w = m.w as f32 - m.side * 2.0;

    let wh = m.row_h * 1.1;
    c.fill_round_rect(x, y, w, wh, m.card_radius, t.pal.danger.with_alpha(28));
    widgets::draw_icon(&mut c, Icon::Wipe, x + m.side * 1.4, y + wh / 2.0, m.chip, t.pal.danger, t.pal.bg);
    c.text(&f.title, "这会清除全部用户数据", x + m.side * 2.4 + m.scale * 20.0, y + wh / 2.0 - m.scale * 2.0, m.row_title * 0.96, t.pal.danger, Some(w * 0.7));
    c.text(&f.regular, "包括照片、应用和账户，无法撤销", x + m.side * 2.4 + m.scale * 20.0, y + wh / 2.0 + m.sub * 1.5, m.sub, t.pal.danger.with_alpha(220), Some(w * 0.72));
    y += wh + m.group_gap;

    y += widgets::card_group(&mut c, t, f, x, y, w, &[
        RowSpec { icon: None, title: "Data（用户数据）", sub: None, trailing: Trailing::Text("12.4 GB") },
        RowSpec { icon: None, title: "Internal Storage", sub: None, trailing: Trailing::Text("378 GB") },
        RowSpec { icon: None, title: "Dalvik / ART Cache", sub: None, trailing: Trailing::Text("2.1 GB") },
    ]);
    y += m.group_gap * 1.4;
    widgets::confirm_slider(&mut c, t, f, x, y, w, m.row_h * 0.95, "滑动确认清除", true);
    c
}

/// Install progress with a flashing package and a short log tail.
pub fn install_progress(t: &Theme, f: &Fonts) -> Canvas {
    let mut c = page(t);
    let m = &t.m;
    widgets::status_bar(&mut c, t, f, "23:46", 74, true);
    let mut y = page_title(&mut c, t, f, "安装");
    y += m.group_gap * 0.6;
    let x = m.side;
    let w = m.w as f32 - m.side * 2.0;

    let ph = m.row_h * 2.0;
    c.round_rect_shadow(x, y, w, ph, m.card_radius, m.scale * 26.0, m.scale * 10.0, t.pal.shadow);
    c.fill_round_rect(x, y, w, ph, m.card_radius, t.pal.card);
    widgets::icon_chip(&mut c, t, x + m.side * 0.9, y + m.side * 1.1, m.chip, C_INSTALL, Icon::Install);
    c.text(&f.title, "kernelsu_patched.img", x + m.side * 0.9 + m.chip + m.side * 0.55, y + m.side * 1.1 + m.row_title * 0.9, m.row_title, t.pal.text, Some(w * 0.62));
    c.text(&f.regular, "正在刷入 init_boot", x + m.side * 0.9 + m.chip + m.side * 0.55, y + m.side * 1.1 + m.row_title * 0.9 + m.sub * 1.6, m.sub, t.pal.secondary, None);
    widgets::progress_bar(&mut c, t, x + m.side, y + ph - m.scale * 110.0, w - m.side * 2.0, 0.68);
    c.text(&f.regular, "68%", x + m.side, y + ph - m.scale * 40.0, m.sub, t.pal.accent, None);
    y += ph + m.group_gap;

    let log = [
        "Patching ramdisk",
        "Writing init_boot_a",
        "Writing init_boot_b",
        "Verifying image",
    ];
    for (i, line) in log.iter().enumerate() {
        c.text(&f.regular, line, x + m.side * 0.4, y + m.sub * (1.6 * i as f32 + 1.0), m.sub, t.pal.secondary, None);
    }
    c
}
