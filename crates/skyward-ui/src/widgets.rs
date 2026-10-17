use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::font::Fonts;
use crate::theme::Theme;

#[derive(Clone, Copy)]
pub enum Icon {
    Gear,
    Globe,
    Clock,
    Sun,
    Moon,
    Usb,
    Terminal,
    Install,
    Restore,
    Backup,
    Wipe,
    Mount,
    Lock,
    Power,
    Reboot,
    Chevron,
}

pub fn draw_icon(c: &mut Canvas, icon: Icon, cx: f32, cy: f32, size: f32, color: Rgba, carve: Rgba) {
    let s = size;
    let t = (s * 0.085).max(2.0);
    match icon {
        Icon::Gear => {
            c.stroke_circle(cx, cy, s * 0.28, t, color);
            c.stroke_circle(cx, cy, s * 0.12, t, color);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let (dx, dy) = (a.cos(), a.sin());
                c.line(cx + dx * s * 0.3, cy + dy * s * 0.3, cx + dx * s * 0.44, cy + dy * s * 0.44, t, color);
            }
        }
        Icon::Globe => {
            c.stroke_circle(cx, cy, s * 0.38, t * 0.9, color);
            c.line(cx - s * 0.38, cy, cx + s * 0.38, cy, t * 0.8, color);
            c.line(cx, cy - s * 0.38, cx, cy + s * 0.38, t * 0.8, color);
            c.stroke_circle(cx, cy, s * 0.18, t * 0.7, color);
        }
        Icon::Clock => {
            c.stroke_circle(cx, cy, s * 0.38, t * 0.9, color);
            c.line(cx, cy, cx, cy - s * 0.24, t, color);
            c.line(cx, cy, cx + s * 0.17, cy + s * 0.02, t, color);
        }
        Icon::Sun => {
            c.stroke_circle(cx, cy, s * 0.2, t, color);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let (dx, dy) = (a.cos(), a.sin());
                c.line(cx + dx * s * 0.3, cy + dy * s * 0.3, cx + dx * s * 0.44, cy + dy * s * 0.44, t, color);
            }
        }
        Icon::Moon => {
            c.fill_circle(cx + s * 0.02, cy, s * 0.34, color);
            c.fill_circle(cx + s * 0.2, cy - s * 0.12, s * 0.32, carve);
        }
        Icon::Usb => {
            c.line(cx, cy - s * 0.3, cx, cy + s * 0.34, t, color);
            c.fill_circle(cx, cy + s * 0.34, s * 0.07, color);
            c.line(cx - s * 0.02, cy - s * 0.3, cx - s * 0.16, cy - s * 0.12, t * 0.8, color);
            c.line(cx + s * 0.02, cy - s * 0.1, cx + s * 0.18, cy + s * 0.04, t * 0.8, color);
            c.fill_circle(cx - s * 0.16, cy - s * 0.12, s * 0.06, color);
            c.stroke_round_rect(cx + s * 0.12, cy - s * 0.02, s * 0.12, s * 0.12, s * 0.02, t * 0.8, color);
            // Arrow head at top of the stem.
            c.line(cx, cy - s * 0.36, cx - s * 0.08, cy - s * 0.24, t, color);
            c.line(cx, cy - s * 0.36, cx + s * 0.08, cy - s * 0.24, t, color);
        }
        Icon::Terminal => {
            c.stroke_round_rect(cx - s * 0.4, cy - s * 0.3, s * 0.8, s * 0.6, s * 0.1, t * 0.9, color);
            c.line(cx - s * 0.2, cy - s * 0.08, cx - s * 0.06, cy + s * 0.02, t, color);
            c.line(cx - s * 0.06, cy + s * 0.02, cx - s * 0.2, cy + s * 0.12, t, color);
            c.line(cx + s * 0.02, cy + s * 0.12, cx + s * 0.2, cy + s * 0.12, t, color);
        }
        Icon::Install => {
            c.line(cx, cy - s * 0.3, cx, cy + s * 0.12, t, color);
            c.line(cx - s * 0.17, cy - s * 0.06, cx, cy + s * 0.14, t, color);
            c.line(cx + s * 0.17, cy - s * 0.06, cx, cy + s * 0.14, t, color);
            c.line(cx - s * 0.28, cy + s * 0.3, cx + s * 0.28, cy + s * 0.3, t, color);
        }
        Icon::Restore => {
            c.line(cx, cy + s * 0.18, cx, cy - s * 0.24, t, color);
            c.line(cx - s * 0.17, cy - s * 0.02, cx, cy - s * 0.26, t, color);
            c.line(cx + s * 0.17, cy - s * 0.02, cx, cy - s * 0.26, t, color);
            c.line(cx - s * 0.28, cy + s * 0.3, cx + s * 0.28, cy + s * 0.3, t, color);
        }
        Icon::Backup => {
            c.stroke_round_rect(cx - s * 0.3, cy - s * 0.26, s * 0.6, s * 0.52, s * 0.08, t, color);
            c.line(cx - s * 0.3, cy - s * 0.06, cx + s * 0.3, cy - s * 0.06, t * 0.9, color);
            c.line(cx - s * 0.1, cy - s * 0.26, cx - s * 0.1, cy - s * 0.06, t * 0.9, color);
            c.line(cx + s * 0.1, cy - s * 0.26, cx + s * 0.1, cy - s * 0.06, t * 0.9, color);
        }
        Icon::Wipe => {
            c.line(cx - s * 0.3, cy - s * 0.24, cx + s * 0.3, cy - s * 0.24, t, color);
            c.line(cx - s * 0.1, cy - s * 0.24, cx - s * 0.08, cy - s * 0.34, t * 0.9, color);
            c.line(cx + s * 0.1, cy - s * 0.24, cx + s * 0.08, cy - s * 0.34, t * 0.9, color);
            c.line(cx - s * 0.08, cy - s * 0.34, cx + s * 0.08, cy - s * 0.34, t * 0.9, color);
            c.stroke_round_rect(cx - s * 0.22, cy - s * 0.2, s * 0.44, s * 0.5, s * 0.06, t, color);
            c.line(cx - s * 0.08, cy - s * 0.08, cx - s * 0.08, cy + s * 0.2, t * 0.8, color);
            c.line(cx + s * 0.08, cy - s * 0.08, cx + s * 0.08, cy + s * 0.2, t * 0.8, color);
        }
        Icon::Mount => {
            c.stroke_round_rect(cx - s * 0.32, cy - s * 0.22, s * 0.64, s * 0.44, s * 0.08, t, color);
            c.line(cx - s * 0.18, cy - s * 0.22, cx - s * 0.18, cy - s * 0.04, t * 0.8, color);
            c.line(cx, cy - s * 0.22, cx, cy - s * 0.04, t * 0.8, color);
            c.line(cx + s * 0.18, cy - s * 0.22, cx + s * 0.18, cy - s * 0.04, t * 0.8, color);
        }
        Icon::Lock => {
            c.stroke_circle(cx, cy - s * 0.1, s * 0.18, t, color);
            c.fill_round_rect(cx - s * 0.26, cy - s * 0.06, s * 0.52, s * 0.36, s * 0.08, color);
            c.fill_circle(cx, cy + s * 0.12, s * 0.05, carve);
        }
        Icon::Power => {
            c.stroke_circle(cx, cy + s * 0.02, s * 0.32, t, color);
            c.fill_rect(cx - s * 0.1, cy - s * 0.44, s * 0.2, s * 0.22, carve);
            c.line(cx, cy - s * 0.38, cx, cy - s * 0.02, t, color);
        }
        Icon::Reboot => {
            c.stroke_circle(cx, cy, s * 0.3, t, color);
            c.fill_rect(cx + s * 0.1, cy - s * 0.44, s * 0.3, s * 0.28, carve);
            c.line(cx + s * 0.28, cy - s * 0.3, cx + s * 0.14, cy - s * 0.18, t, color);
            c.line(cx + s * 0.28, cy - s * 0.3, cx + s * 0.4, cy - s * 0.16, t, color);
        }
        Icon::Chevron => {
            c.line(cx - s * 0.1, cy - s * 0.22, cx + s * 0.14, cy, t * 0.9, color);
            c.line(cx + s * 0.14, cy, cx - s * 0.1, cy + s * 0.22, t * 0.9, color);
        }
    }
}

pub fn icon_chip(c: &mut Canvas, t: &Theme, x: f32, y: f32, size: f32, bg: Rgba, icon: Icon) {
    c.fill_round_rect(x, y, size, size, t.m.chip_radius, bg);
    draw_icon(c, icon, x + size / 2.0, y + size / 2.0, size * 0.62, Rgba::rgb(255, 255, 255), bg);
}

pub fn status_bar(c: &mut Canvas, t: &Theme, f: &Fonts, time: &str, batt: u8, charging: bool) {
    let m = &t.m;
    let y = m.status_h * 0.62;
    c.text(&f.regular, time, m.side, y, m.status_px, t.pal.text, None);
    // Battery on the right: number, then a small battery glyph.
    let gw = m.status_px * 1.5;
    let gh = m.status_px * 0.82;
    let gx = m.w as f32 - m.side - gw;
    let gy = m.status_h * 0.62 - gh * 0.78;
    c.stroke_round_rect(gx, gy, gw, gh, gh * 0.28, m.scale * 3.0, t.pal.secondary);
    c.fill_round_rect(gx + gw, gy + gh * 0.3, m.scale * 5.0, gh * 0.4, m.scale * 2.0, t.pal.secondary);
    let fill_c = if charging { Rgba::rgb(80, 200, 120) } else { t.pal.text };
    let pad = m.scale * 4.0;
    c.fill_round_rect(gx + pad, gy + pad, (gw - pad * 2.0) * (batt as f32 / 100.0), gh - pad * 2.0, gh * 0.18, fill_c);
    let label = format!("{batt}%");
    let lw = c.text_width(&f.regular, &label, m.status_px * 0.86);
    c.text(&f.regular, &label, gx - lw - m.side * 0.4, y, m.status_px * 0.86, t.pal.secondary, None);
    if charging {
        let bx = gx - lw - m.side * 0.4 - m.status_px * 0.5;
        c.line(bx, y - m.status_px * 0.4, bx - m.status_px * 0.12, y - m.status_px * 0.02, m.scale * 3.0, fill_c);
        c.line(bx - m.status_px * 0.12, y - m.status_px * 0.02, bx + m.status_px * 0.08, y - m.status_px * 0.06, m.scale * 3.0, fill_c);
    }
}

pub enum Trailing<'a> {
    None,
    Arrow,
    Text(&'a str),
    Switch(bool),
}

pub struct RowSpec<'a> {
    pub icon: Option<(Rgba, Icon)>,
    pub title: &'a str,
    pub sub: Option<&'a str>,
    pub trailing: Trailing<'a>,
}

/// A grouped settings card: rounded white (or dark) container with a soft
/// shadow, rows separated by inset hairlines. Returns the card height.
pub fn card_group(c: &mut Canvas, t: &Theme, f: &Fonts, x: f32, y: f32, w: f32, rows: &[RowSpec]) -> f32 {
    let m = &t.m;
    let h = m.row_h * rows.len() as f32;
    c.round_rect_shadow(x, y, w, h, m.card_radius, m.scale * 26.0, m.scale * 10.0, t.pal.shadow);
    c.fill_round_rect(x, y, w, h, m.card_radius, t.pal.card);

    let chip = m.chip;
    let text_left_no_icon = x + m.side * 0.9;
    for (i, r) in rows.iter().enumerate() {
        let ry = y + m.row_h * i as f32;
        let has_icon = r.icon.is_some();
        let text_x = if has_icon { x + m.side * 0.8 + chip + m.side * 0.55 } else { text_left_no_icon };
        if let Some((bg, icon)) = r.icon {
            icon_chip(c, t, x + m.side * 0.8, ry + (m.row_h - chip) / 2.0, chip, bg, icon);
        }
        let cy = ry + m.row_h / 2.0;
        match r.sub {
            Some(sub) => {
                c.text(&f.regular, r.title, text_x, cy - m.scale * 6.0, m.row_title, t.pal.text, Some(w * 0.55));
                c.text(&f.regular, sub, text_x, cy + m.sub + m.scale * 4.0, m.sub, t.pal.secondary, Some(w * 0.55));
            }
            None => {
                c.text(&f.regular, r.title, text_x, cy + m.row_title * 0.34, m.row_title, t.pal.text, Some(w * 0.6));
            }
        }
        let right = x + w - m.side * 0.9;
        match &r.trailing {
            Trailing::None => {}
            Trailing::Arrow => draw_icon(c, Icon::Chevron, right - m.scale * 14.0, cy, m.scale * 44.0, t.pal.tertiary, t.pal.card),
            Trailing::Text(s) => {
                let tw = c.text_width(&f.regular, s, m.sub * 1.05);
                c.text(&f.regular, s, right - tw - m.scale * 46.0, cy + m.sub * 0.36, m.sub * 1.05, t.pal.secondary, None);
                draw_icon(c, Icon::Chevron, right - m.scale * 14.0, cy, m.scale * 44.0, t.pal.tertiary, t.pal.card);
            }
            Trailing::Switch(on) => switch(c, t, right - m.scale * 92.0, cy, *on),
        }
        if i + 1 < rows.len() {
            let sep_x = if has_icon { text_x } else { x + m.side * 0.9 };
            c.fill_rect(sep_x, ry + m.row_h - m.scale, x + w - m.side * 0.9 - sep_x, m.scale, t.pal.separator);
        }
    }
    h
}

pub fn switch(c: &mut Canvas, t: &Theme, x: f32, cy: f32, on: bool) {
    let m = &t.m;
    let w = m.scale * 92.0;
    let h = m.scale * 54.0;
    let y = cy - h / 2.0;
    let bg = if on { t.pal.accent } else { t.pal.switch_off };
    c.fill_round_rect(x, y, w, h, h / 2.0, bg);
    let kr = h / 2.0 - m.scale * 5.0;
    let kx = if on { x + w - kr - m.scale * 5.0 } else { x + kr + m.scale * 5.0 };
    c.fill_circle(kx, cy, kr, Rgba::rgb(255, 255, 255));
}

pub fn slider(c: &mut Canvas, t: &Theme, x: f32, cy: f32, w: f32, value: f32) {
    let m = &t.m;
    let th = m.scale * 16.0;
    c.fill_round_rect(x, cy - th / 2.0, w, th, th / 2.0, t.pal.track);
    let fw = (w * value.clamp(0.0, 1.0)).max(th);
    c.fill_round_rect(x, cy - th / 2.0, fw, th, th / 2.0, t.pal.accent);
    c.fill_circle(x + fw, cy, m.scale * 22.0, Rgba::rgb(255, 255, 255));
    c.stroke_circle(x + fw, cy, m.scale * 22.0, m.scale * 2.0, t.pal.nav_border);
}

pub fn progress_bar(c: &mut Canvas, t: &Theme, x: f32, y: f32, w: f32, value: f32) {
    let m = &t.m;
    let th = m.scale * 20.0;
    c.fill_round_rect(x, y, w, th, th / 2.0, t.pal.track);
    c.fill_round_rect(x, y, (w * value.clamp(0.0, 1.0)).max(th), th, th / 2.0, t.pal.accent);
}

pub fn primary_button(c: &mut Canvas, t: &Theme, f: &Fonts, x: f32, y: f32, w: f32, h: f32, label: &str, danger: bool) {
    let bg = if danger { t.pal.danger } else { t.pal.accent };
    c.fill_round_rect(x, y, w, h, h / 2.0, bg);
    c.text_center(&f.title, label, x + w / 2.0, y + h / 2.0 + t.m.row_title * 0.36, t.m.row_title, t.pal.on_accent);
}

/// Swipe-to-confirm track used on the reboot and wipe pages.
pub fn confirm_slider(c: &mut Canvas, t: &Theme, f: &Fonts, x: f32, y: f32, w: f32, h: f32, label: &str, danger: bool) {
    let m = &t.m;
    let accent = if danger { t.pal.danger } else { t.pal.accent };
    c.fill_round_rect(x, y, w, h, h / 2.0, accent.with_alpha(40));
    c.text_center(&f.regular, label, x + w / 2.0, y + h / 2.0 + m.sub * 0.36, m.sub * 1.1, accent);
    let kr = h / 2.0 - m.scale * 6.0;
    let kcx = x + h / 2.0;
    c.fill_circle(kcx, y + h / 2.0, kr, accent);
    draw_icon(c, Icon::Chevron, kcx, y + h / 2.0, kr * 1.4, Rgba::rgb(255, 255, 255), accent);
}

/// 3x3 FBE unlock grid. `path` is the sequence of visited dot indices (0..9),
/// drawn as a connected accent trail.
pub fn pattern_lock(c: &mut Canvas, t: &Theme, cx: f32, cy: f32, span: f32, path: &[usize]) {
    let m = &t.m;
    let step = span / 2.0;
    let x0 = cx - step;
    let y0 = cy - step;
    let pos = |idx: usize| -> (f32, f32) {
        let (r, col) = (idx / 3, idx % 3);
        (x0 + col as f32 * step, y0 + r as f32 * step)
    };
    for w in path.windows(2) {
        let (ax, ay) = pos(w[0]);
        let (bx, by) = pos(w[1]);
        c.line(ax, ay, bx, by, m.scale * 10.0, t.pal.accent.with_alpha(210));
    }
    for i in 0..9 {
        let (px, py) = pos(i);
        let active = path.contains(&i);
        c.fill_circle(px, py, m.scale * 20.0, if active { t.pal.accent } else { t.pal.tertiary.with_alpha(150) });
        if active {
            c.stroke_circle(px, py, m.scale * 42.0, m.scale * 8.0, t.pal.accent.with_alpha(120));
        }
    }
}

/// Floating 1+3 bottom navigation: a standalone round back button and a pill
/// holding three destinations.
pub fn bottom_nav(c: &mut Canvas, t: &Theme, active: usize) {
    let m = &t.m;
    let d = m.scale * 128.0;
    let y = m.h as f32 - d - m.scale * 48.0;
    let back_x = m.side;
    c.round_rect_shadow(back_x, y, d, d, d / 2.0, m.scale * 22.0, m.scale * 8.0, t.pal.shadow);
    c.fill_round_rect(back_x, y, d, d, d / 2.0, t.pal.nav);
    c.stroke_round_rect(back_x, y, d, d, d / 2.0, m.scale * 2.0, t.pal.nav_border);
    // Back arrow.
    let bcx = back_x + d / 2.0;
    let bcy = y + d / 2.0;
    c.line(bcx + m.scale * 16.0, bcy - m.scale * 22.0, bcx - m.scale * 18.0, bcy, m.scale * 9.0, t.pal.text);
    c.line(bcx - m.scale * 18.0, bcy, bcx + m.scale * 16.0, bcy + m.scale * 22.0, m.scale * 9.0, t.pal.text);

    let pill_x = back_x + d + m.scale * 24.0;
    let pill_w = m.w as f32 - pill_x - m.side;
    c.round_rect_shadow(pill_x, y, pill_w, d, d / 2.0, m.scale * 22.0, m.scale * 8.0, t.pal.shadow);
    c.fill_round_rect(pill_x, y, pill_w, d, d / 2.0, t.pal.nav);
    c.stroke_round_rect(pill_x, y, pill_w, d, d / 2.0, m.scale * 2.0, t.pal.nav_border);
    let icons = [Icon::Install, Icon::Backup, Icon::Gear];
    for (i, ic) in icons.iter().enumerate() {
        let cxp = pill_x + pill_w * (i as f32 + 0.5) / 3.0;
        let col = if i == active { t.pal.accent } else { t.pal.secondary };
        draw_icon(c, *ic, cxp, y + d / 2.0, m.scale * 58.0, col, t.pal.nav);
    }
}
