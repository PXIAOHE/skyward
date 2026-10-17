use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::font::{Face, Fonts};
use crate::theme::Theme;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

#[derive(Clone, Copy)]
pub enum Icon {
    Install,
    Backup,
    Restore,
    Mount,
    Wipe,
    Gear,
    Power,
    Globe,
    Clock,
    Moon,
    Terminal,
    Usb,
    Layers,
    Phone,
    Lock,
}

/// White solid glyph centred on `(cx, cy)` inside a tile of size `tile`.
/// `bg` is the tile colour, used to cut details out of filled shapes.
pub fn glyph(c: &mut Canvas, icon: Icon, cx: f32, cy: f32, tile: f32, fg: Rgba, bg: Rgba) {
    let g = tile * 0.58;
    let sw = tile * 0.085;
    match icon {
        Icon::Install => {
            c.line(cx, cy - 0.34 * g, cx, cy + 0.02 * g, sw, fg);
            c.fill_convex(&[(cx - 0.24 * g, cy - 0.04 * g), (cx + 0.24 * g, cy - 0.04 * g), (cx, cy + 0.22 * g)], fg);
            c.line(cx - 0.4 * g, cy + 0.16 * g, cx - 0.4 * g, cy + 0.38 * g, sw, fg);
            c.line(cx - 0.4 * g, cy + 0.38 * g, cx + 0.4 * g, cy + 0.38 * g, sw, fg);
            c.line(cx + 0.4 * g, cy + 0.38 * g, cx + 0.4 * g, cy + 0.16 * g, sw, fg);
        }
        Icon::Backup => {
            c.fill_round_rect(cx - 0.44 * g, cy - 0.38 * g, 0.88 * g, 0.22 * g, 0.06 * g, fg);
            c.fill_round_rect(cx - 0.37 * g, cy - 0.1 * g, 0.74 * g, 0.48 * g, 0.08 * g, fg);
            c.fill_round_rect(cx - 0.13 * g, cy + 0.01 * g, 0.26 * g, 0.09 * g, 0.045 * g, bg);
        }
        Icon::Restore => {
            let r = 0.34 * g;
            let start = 200f32.to_radians();
            c.stroke_arc(cx, cy, r, start, 290f32.to_radians(), sw, fg);
            let (hx, hy) = (cx + r * start.cos(), cy + r * start.sin());
            c.fill_convex(&[(hx - 0.2 * g, hy - 0.06 * g), (hx + 0.12 * g, hy - 0.02 * g), (hx - 0.02 * g, hy + 0.24 * g)], fg);
            c.line(cx, cy - 0.16 * g, cx, cy + 0.02 * g, sw * 0.9, fg);
            c.line(cx, cy + 0.02 * g, cx + 0.13 * g, cy + 0.1 * g, sw * 0.9, fg);
        }
        Icon::Mount => {
            c.fill_convex(&[
                (cx - 0.32 * g, cy - 0.2 * g),
                (cx - 0.1 * g, cy - 0.42 * g),
                (cx + 0.32 * g, cy - 0.42 * g),
                (cx + 0.32 * g, cy + 0.42 * g),
                (cx - 0.32 * g, cy + 0.42 * g),
            ], fg);
            for dx in [-0.04f32, 0.09, 0.22] {
                c.line(cx + dx * g, cy - 0.3 * g, cx + dx * g, cy - 0.14 * g, 0.075 * g, bg);
            }
        }
        Icon::Wipe => {
            c.fill_round_rect(cx - 0.13 * g, cy - 0.42 * g, 0.26 * g, 0.14 * g, 0.05 * g, fg);
            c.line(cx - 0.38 * g, cy - 0.25 * g, cx + 0.38 * g, cy - 0.25 * g, sw, fg);
            c.fill_convex(&[(cx - 0.3 * g, cy - 0.14 * g), (cx + 0.3 * g, cy - 0.14 * g), (cx + 0.23 * g, cy + 0.42 * g), (cx - 0.23 * g, cy + 0.42 * g)], fg);
            c.line(cx - 0.09 * g, cy - 0.02 * g, cx - 0.08 * g, cy + 0.28 * g, 0.065 * g, bg);
            c.line(cx + 0.09 * g, cy - 0.02 * g, cx + 0.08 * g, cy + 0.28 * g, 0.065 * g, bg);
        }
        Icon::Gear => {
            for k in 0..8 {
                let a = k as f32 * FRAC_PI_4;
                let (ux, uy) = (a.cos(), a.sin());
                let (vx, vy) = (-uy, ux);
                let (r0, r1, hw) = (0.24 * g, 0.44 * g, 0.09 * g);
                c.fill_convex(&[
                    (cx + ux * r0 - vx * hw, cy + uy * r0 - vy * hw),
                    (cx + ux * r1 - vx * hw, cy + uy * r1 - vy * hw),
                    (cx + ux * r1 + vx * hw, cy + uy * r1 + vy * hw),
                    (cx + ux * r0 + vx * hw, cy + uy * r0 + vy * hw),
                ], fg);
            }
            c.fill_circle(cx, cy, 0.33 * g, fg);
            c.fill_circle(cx, cy, 0.13 * g, bg);
        }
        Icon::Power => {
            let gap = 38f32.to_radians();
            c.stroke_arc(cx, cy + 0.04 * g, 0.34 * g, -FRAC_PI_2 + gap, TAU - 2.0 * gap, sw, fg);
            c.line(cx, cy - 0.42 * g, cx, cy - 0.02 * g, sw, fg);
        }
        Icon::Globe => {
            c.stroke_circle(cx, cy, 0.38 * g, sw * 0.85, fg);
            c.stroke_ellipse(cx, cy, 0.16 * g, 0.38 * g, sw * 0.8, fg);
            c.line(cx - 0.38 * g, cy, cx + 0.38 * g, cy, sw * 0.8, fg);
        }
        Icon::Clock => {
            c.fill_circle(cx, cy, 0.4 * g, fg);
            c.line(cx, cy + 0.02 * g, cx, cy - 0.24 * g, sw * 0.9, bg);
            c.line(cx, cy + 0.02 * g, cx + 0.16 * g, cy + 0.08 * g, sw * 0.9, bg);
        }
        Icon::Moon => {
            c.fill_circle(cx - 0.02 * g, cy + 0.02 * g, 0.36 * g, fg);
            c.fill_circle(cx + 0.17 * g, cy - 0.13 * g, 0.29 * g, bg);
        }
        Icon::Terminal => {
            c.stroke_round_rect(cx - 0.42 * g, cy - 0.34 * g, 0.84 * g, 0.68 * g, 0.12 * g, sw * 0.85, fg);
            c.line(cx - 0.22 * g, cy - 0.1 * g, cx - 0.06 * g, cy + 0.03 * g, sw * 0.85, fg);
            c.line(cx - 0.06 * g, cy + 0.03 * g, cx - 0.22 * g, cy + 0.16 * g, sw * 0.85, fg);
            c.line(cx + 0.02 * g, cy + 0.16 * g, cx + 0.22 * g, cy + 0.16 * g, sw * 0.85, fg);
        }
        Icon::Usb => {
            c.fill_round_rect(cx - 0.18 * g, cy - 0.44 * g, 0.36 * g, 0.28 * g, 0.05 * g, fg);
            c.fill_round_rect(cx - 0.1 * g, cy - 0.36 * g, 0.06 * g, 0.1 * g, 0.02 * g, bg);
            c.fill_round_rect(cx + 0.04 * g, cy - 0.36 * g, 0.06 * g, 0.1 * g, 0.02 * g, bg);
            c.fill_round_rect(cx - 0.27 * g, cy - 0.16 * g, 0.54 * g, 0.3 * g, 0.09 * g, fg);
            c.line(cx, cy + 0.14 * g, cx, cy + 0.44 * g, sw, fg);
        }
        Icon::Layers => {
            c.line(cx - 0.4 * g, cy + 0.1 * g, cx, cy + 0.3 * g, sw, fg);
            c.line(cx, cy + 0.3 * g, cx + 0.4 * g, cy + 0.1 * g, sw, fg);
            c.fill_convex(&[(cx, cy - 0.38 * g), (cx + 0.42 * g, cy - 0.12 * g), (cx, cy + 0.12 * g), (cx - 0.42 * g, cy - 0.12 * g)], fg);
        }
        Icon::Phone => {
            c.fill_round_rect(cx - 0.24 * g, cy - 0.42 * g, 0.48 * g, 0.84 * g, 0.11 * g, fg);
            c.fill_round_rect(cx - 0.08 * g, cy - 0.33 * g, 0.16 * g, 0.05 * g, 0.025 * g, bg);
        }
        Icon::Lock => {
            c.stroke_arc(cx, cy - 0.1 * g, 0.22 * g, std::f32::consts::PI, std::f32::consts::PI, sw, fg);
            c.line(cx - 0.22 * g, cy - 0.1 * g, cx - 0.22 * g, cy + 0.02 * g, sw, fg);
            c.line(cx + 0.22 * g, cy - 0.1 * g, cx + 0.22 * g, cy + 0.02 * g, sw, fg);
            c.fill_round_rect(cx - 0.36 * g, cy - 0.02 * g, 0.72 * g, 0.46 * g, 0.1 * g, fg);
            c.fill_circle(cx, cy + 0.2 * g, 0.06 * g, bg);
        }
    }
}

pub fn tile(c: &mut Canvas, t: &Theme, x: f32, y: f32, color: Rgba, icon: Icon) {
    let s = t.m.icon;
    c.fill_round_rect(x, y, s, s, t.m.icon_radius, color);
    glyph(c, icon, x + s / 2.0, y + s / 2.0, s, Rgba::rgb(255, 255, 255), color);
}

fn chevron(c: &mut Canvas, t: &Theme, right: f32, cy: f32) {
    let m = &t.m;
    let (w, h) = (m.chevron_w, m.chevron_h);
    let sw = m.dp(1.7);
    let x0 = right - w + sw / 2.0;
    let x1 = right - sw / 2.0;
    c.line(x0, cy - h / 2.0 + sw / 2.0, x1, cy, sw, t.pal.chevron);
    c.line(x1, cy, x0, cy + h / 2.0 - sw / 2.0, sw, t.pal.chevron);
}

fn wrap(c: &Canvas, face: &Face, text: &str, px: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for ch in text.chars() {
        let mut probe = line.clone();
        probe.push(ch);
        if !line.is_empty() && c.text_width(face, &probe, px) > max_w {
            // Prefer breaking Latin runs at the last space.
            if ch != ' ' {
                if let Some(sp) = line.rfind(' ') {
                    let rest = line[sp + 1..].to_string();
                    if rest.chars().all(|r| r.is_ascii_alphanumeric()) && !rest.is_empty() {
                        lines.push(line[..sp].to_string());
                        line = rest;
                        line.push(ch);
                        continue;
                    }
                }
            }
            lines.push(std::mem::take(&mut line));
            if ch != ' ' {
                line.push(ch);
            }
        } else {
            line.push(ch);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn status_bar(c: &mut Canvas, t: &Theme, f: &Fonts, time: &str, batt: u8, charging: bool) {
    let m = &t.m;
    let cy = m.status_cy;
    // Digits carry no descender, so centre on half their height (≈0.72 em).
    c.text(&f.status, time, m.status_x, cy + m.status_px * 0.36, m.status_px, t.pal.status, None);

    let right = m.w as f32 - m.status_x;
    let bolt_w = if charging { m.dp(6.0) } else { 0.0 };
    let (bw, bh) = (m.dp(25.0), m.dp(12.6));
    let bx = right - bolt_w - m.dp(2.0) - m.dp(1.8) - bw;
    let by = cy - bh / 2.0;
    c.fill_round_rect(bx, by, bw, bh, m.dp(4.2), t.pal.battery);
    c.fill_round_rect(bx + bw + m.dp(0.6), cy - m.dp(2.6), m.dp(1.6), m.dp(5.2), m.dp(0.8), t.pal.battery);
    c.text_center(&f.strong, &batt.to_string(), bx + bw / 2.0, cy, m.dp(10.5), Rgba::rgb(255, 255, 255));
    if charging {
        let x = right - bolt_w;
        let h = m.dp(11.0);
        c.fill_convex(&[(x + bolt_w * 0.65, cy - h / 2.0), (x + bolt_w * 0.05, cy + h * 0.08), (x + bolt_w * 0.5, cy + h * 0.08)], t.pal.status);
        c.fill_convex(&[(x + bolt_w * 0.35, cy + h / 2.0), (x + bolt_w * 0.95, cy - h * 0.08), (x + bolt_w * 0.5, cy - h * 0.08)], t.pal.status);
    }
}

/// Status bar, optional `←`, and the large page title. Returns where the page
/// content starts.
pub fn header(c: &mut Canvas, t: &Theme, f: &Fonts, title: &str, back: bool) -> f32 {
    let m = &t.m;
    status_bar(c, t, f, "23:46", 74, true);
    if back {
        let (x, cy, w, h, sw) = (m.back_x, m.back_cy, m.back_w, m.back_h, m.stroke);
        let tip = x + sw / 2.0;
        c.line(tip, cy, x + w - sw / 2.0, cy, sw, t.pal.text);
        c.line(tip, cy, tip + h / 2.0, cy - h / 2.0 + sw / 2.0, sw, t.pal.text);
        c.line(tip, cy, tip + h / 2.0, cy + h / 2.0 - sw / 2.0, sw, t.pal.text);
    }
    c.text_top(&f.title, title, m.title_x, m.title_top, m.title_px, t.pal.text, None);
    m.content_top
}

pub fn search_bar(c: &mut Canvas, t: &Theme, f: &Fonts, y: f32, placeholder: &str) -> f32 {
    let m = &t.m;
    let (x, w, h) = (m.margin, m.w as f32 - m.margin * 2.0, m.search_h);
    let cy = y + h / 2.0;
    c.fill_round_rect(x, y, w, h, h / 2.0, t.pal.search);
    let (icx, icy, sw) = (x + m.dp(23.3), cy - m.dp(2.3), m.dp(1.45));
    let ring = m.dp(6.9);
    c.stroke_circle(icx, icy, ring, sw, t.pal.search_icon);
    let k = ring * std::f32::consts::FRAC_1_SQRT_2;
    c.line(icx + k, icy + k, icx + m.dp(10.4), icy + m.dp(10.6), sw, t.pal.search_icon);
    c.text_vc(&f.body, placeholder, x + m.search_text_inset, cy, m.row_title_px, t.pal.placeholder, None);
    y + h + m.after_search
}

/// Section caption between two cards. `prev_bottom` is the bottom of the card
/// above (or the content top for the first section). Returns the next card top.
pub fn section(c: &mut Canvas, t: &Theme, f: &Fonts, prev_bottom: f32, label: &str) -> f32 {
    let m = &t.m;
    let ink_top = prev_bottom + m.section_label_top;
    c.text_top(&f.label, label, m.title_x + m.dp(0.9), ink_top, m.sub_px, t.pal.section, None);
    prev_bottom + m.section_block
}

/// First section on a page, directly under the content top.
pub fn section_first(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str) -> f32 {
    let m = &t.m;
    section(c, t, f, top - m.section_label_top + m.dp(2.0), label)
}

/// Grey explanatory text below a card, wrapped to the content width.
pub fn footnote(c: &mut Canvas, t: &Theme, f: &Fonts, y: f32, text: &str) -> f32 {
    let m = &t.m;
    let x = m.title_x + m.dp(0.9);
    let max_w = m.w as f32 - x - m.title_x;
    let lines = wrap(c, &f.value, text, m.sub_px, max_w);
    let mut top = y + m.dp(10.0);
    for line in &lines {
        c.text_top(&f.value, line, x, top, m.sub_px, t.pal.value, None);
        top += m.sub_pitch;
    }
    top
}

#[derive(Clone, Copy)]
pub enum Trail<'a> {
    None,
    Arrow,
    Value(&'a str),
    /// Right-aligned grey text with no chevron (read-only information).
    Text(&'a str),
    Switch(bool),
    Badge(&'a str),
    Check(bool),
}

#[derive(Clone, Copy)]
pub struct Row<'a> {
    pub icon: Option<(Rgba, Icon)>,
    pub title: &'a str,
    pub sub: Option<&'a str>,
    pub trail: Trail<'a>,
    pub tint: Option<Rgba>,
}

impl<'a> Row<'a> {
    pub fn new(title: &'a str) -> Self {
        Row { icon: None, title, sub: None, trail: Trail::Arrow, tint: None }
    }
    pub fn icon(mut self, color: Rgba, icon: Icon) -> Self {
        self.icon = Some((color, icon));
        self
    }
    pub fn subtitle(mut self, sub: &'a str) -> Self {
        self.sub = Some(sub);
        self
    }
    pub fn trail(mut self, trail: Trail<'a>) -> Self {
        self.trail = trail;
        self
    }
    pub fn tint(mut self, color: Rgba) -> Self {
        self.tint = Some(color);
        self
    }
}

struct Layout {
    text_x: f32,
    text_max: f32,
    sub_lines: Vec<String>,
    h: f32,
}

fn layout(c: &Canvas, t: &Theme, f: &Fonts, r: &Row) -> Layout {
    let m = &t.m;
    let card_w = m.w as f32 - m.margin * 2.0;
    let text_x = m.inner + if r.icon.is_some() { m.icon + m.icon_gap } else { 0.0 };
    let trail_w = match r.trail {
        Trail::None => m.inner,
        Trail::Arrow => m.chevron_inset + m.chevron_w + m.value_gap,
        Trail::Value(s) => m.chevron_inset + m.chevron_w + m.value_gap * 2.0 + c.text_width(&f.value, s, m.sub_px),
        Trail::Text(s) => m.inner + m.value_gap + c.text_width(&f.value, s, m.sub_px),
        Trail::Switch(_) => m.inner + m.switch_w + m.value_gap,
        Trail::Badge(_) => m.inner + m.badge_w + m.value_gap,
        Trail::Check(_) => m.inner + m.dp(24.0) + m.value_gap,
    };
    let text_max = card_w - text_x - trail_w;
    let sub_lines = r.sub.map(|s| wrap(c, &f.body, s, m.sub_px, text_max)).unwrap_or_default();
    let h = if sub_lines.is_empty() {
        m.row_h
    } else {
        let (tt, tb) = f.heading.ink(m.row_title_px);
        let (st, sb) = f.body.ink(m.sub_px);
        let block = (tb - tt) + m.sub_gap + (sub_lines.len() as f32 - 1.0) * m.sub_pitch + (sb - st);
        (block + m.row_pad_v * 2.0).max(m.row_h)
    };
    Layout { text_x, text_max, sub_lines, h }
}

fn draw_row(c: &mut Canvas, t: &Theme, f: &Fonts, r: &Row, lay: &Layout, top: f32) {
    let m = &t.m;
    let card_x = m.margin;
    let right = m.w as f32 - m.margin;
    let cy = top + lay.h / 2.0;
    if let Some((color, icon)) = r.icon {
        tile(c, t, card_x + m.inner, cy - m.icon / 2.0, color, icon);
    }
    let tx = card_x + lay.text_x;
    let color = r.tint.unwrap_or(t.pal.text);
    if lay.sub_lines.is_empty() {
        c.text_vc(&f.heading, r.title, tx, cy, m.row_title_px, color, Some(lay.text_max));
    } else {
        let (tt, tb) = f.heading.ink(m.row_title_px);
        let (st, sb) = f.body.ink(m.sub_px);
        let block = (tb - tt) + m.sub_gap + (lay.sub_lines.len() as f32 - 1.0) * m.sub_pitch + (sb - st);
        let ink_top = cy - block / 2.0;
        c.text_top(&f.heading, r.title, tx, ink_top, m.row_title_px, color, Some(lay.text_max));
        let mut st_top = ink_top + (tb - tt) + m.sub_gap;
        for line in &lay.sub_lines {
            c.text_top(&f.body, line, tx, st_top, m.sub_px, t.pal.subtitle, None);
            st_top += m.sub_pitch;
        }
    }
    match r.trail {
        Trail::None => {}
        Trail::Arrow => chevron(c, t, right - m.chevron_inset, cy),
        Trail::Value(s) => {
            let chev_left = right - m.chevron_inset - m.chevron_w;
            chevron(c, t, right - m.chevron_inset, cy);
            let w = c.text_width(&f.value, s, m.sub_px);
            c.text_vc(&f.value, s,chev_left - m.value_gap - w, cy, m.sub_px, t.pal.value, None);
        }
        Trail::Text(s) => {
            let w = c.text_width(&f.value, s, m.sub_px);
            c.text_vc(&f.value, s,right - m.inner - w, cy, m.sub_px, t.pal.value, None);
        }
        Trail::Switch(on) => switch(c, t, right - m.inner, cy, on),
        Trail::Badge(s) => {
            let (bw, bh) = (m.badge_w, m.badge_h);
            let bx = right - m.inner - bw;
            c.fill_round_rect(bx, cy - bh / 2.0, bw, bh, bh / 2.0, t.pal.badge);
            c.text_center(&f.badge, s, bx + bw / 2.0, cy, m.badge_px, t.pal.accent);
        }
        Trail::Check(on) => {
            let d = m.dp(24.0);
            let ccx = right - m.inner - d / 2.0;
            if on {
                c.fill_circle(ccx, cy, d / 2.0, t.pal.accent);
                let s = d * 0.5;
                c.line(ccx - s * 0.42, cy + s * 0.02, ccx - s * 0.1, cy + s * 0.32, m.stroke, Rgba::rgb(255, 255, 255));
                c.line(ccx - s * 0.1, cy + s * 0.32, ccx + s * 0.44, cy - s * 0.3, m.stroke, Rgba::rgb(255, 255, 255));
            } else {
                c.stroke_circle(ccx, cy, d / 2.0 - m.dp(1.0), m.dp(2.0), t.pal.switch_off);
            }
        }
    }
}

/// A flat settings card holding `rows`. Returns the card's bottom.
pub fn list(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, rows: &[Row]) -> f32 {
    let m = &t.m;
    let lays: Vec<Layout> = rows.iter().map(|r| layout(c, t, f, r)).collect();
    let h: f32 = lays.iter().map(|l| l.h).sum();
    c.fill_round_rect(m.margin, top, m.w as f32 - m.margin * 2.0, h, m.card_radius, t.pal.card);
    let mut y = top;
    for (r, l) in rows.iter().zip(&lays) {
        draw_row(c, t, f, r, l, y);
        y += l.h;
    }
    top + h
}

/// Account-style card: avatar, name and detail, optionally followed by rows.
pub fn device_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, title: &str, sub: &str, rows: &[Row]) -> f32 {
    let m = &t.m;
    let lays: Vec<Layout> = rows.iter().map(|r| layout(c, t, f, r)).collect();
    let h = m.account_row_h + lays.iter().map(|l| l.h).sum::<f32>();
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, t.pal.card);

    let cy = top + m.account_row_h / 2.0;
    let av = m.avatar;
    let acx = card_x + m.dp(13.7) + av / 2.0;
    c.fill_circle(acx, cy, av / 2.0, t.pal.avatar);
    glyph(c, Icon::Phone, acx, cy, av * 0.86, t.pal.avatar_glyph, t.pal.avatar);

    let tx = card_x + m.inner + m.icon + m.icon_gap;
    let (tt, tb) = f.heading.ink(m.account_title_px);
    let (st, sb) = f.body.ink(m.sub_px);
    let block = (tb - tt) + m.sub_gap + (sb - st);
    let ink_top = cy - block / 2.0;
    c.text_top(&f.heading, title, tx, ink_top, m.account_title_px, t.pal.text, None);
    c.text_top(&f.body, sub, tx, ink_top + (tb - tt) + m.sub_gap, m.sub_px, t.pal.subtitle, None);
    chevron(c, t, right - m.chevron_inset, cy);

    let mut y = top + m.account_row_h;
    for (r, l) in rows.iter().zip(&lays) {
        draw_row(c, t, f, r, l, y);
        y += l.h;
    }
    top + h
}

pub fn switch(c: &mut Canvas, t: &Theme, right: f32, cy: f32, on: bool) {
    let m = &t.m;
    let (w, h, k) = (m.switch_w, m.switch_h, m.knob);
    let x = right - w;
    c.fill_round_rect(x, cy - h / 2.0, w, h, h / 2.0, if on { t.pal.accent } else { t.pal.switch_off });
    let kx = if on { right - h / 2.0 } else { x + h / 2.0 };
    c.fill_circle(kx, cy, k / 2.0, Rgba::rgb(255, 255, 255));
}

/// The stock thick slider: a capsule track with the fill and a white knob
/// that sits inside the fill's rounded end.
pub fn slider(c: &mut Canvas, t: &Theme, x: f32, top: f32, w: f32, value: f32) {
    let m = &t.m;
    let h = m.slider_h;
    c.fill_round_rect(x, top, w, h, h / 2.0, t.pal.track);
    let fw = (w * value.clamp(0.0, 1.0)).max(h);
    c.fill_round_rect(x, top, fw, h, h / 2.0, t.pal.accent);
    c.fill_circle(x + fw - h / 2.0, top + h / 2.0, m.knob / 2.0, Rgba::rgb(255, 255, 255));
}

/// Card with a monochrome icon caption, a slider, and an optional row below.
pub fn slider_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str, value: f32, below: Option<Row>) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let below_lay = below.as_ref().map(|r| layout(c, t, f, r));
    let slider_top = top + m.dp(56.3);
    let slider_bottom = slider_top + m.slider_h;
    let h = match &below_lay {
        Some(l) => slider_bottom - top + m.dp(17.4) + l.h,
        None => slider_bottom - top + m.dp(18.0),
    };
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, t.pal.card);

    let lcy = top + m.dp(32.1);
    let scx = card_x + m.dp(29.6);
    c.fill_circle(scx, lcy, m.dp(5.7), t.pal.mono_icon);
    for k in 0..8 {
        let a = k as f32 * FRAC_PI_4;
        let (ux, uy) = (a.cos(), a.sin());
        c.line(scx + ux * m.dp(8.3), lcy + uy * m.dp(8.3), scx + ux * m.dp(8.9), lcy + uy * m.dp(8.9), m.dp(2.0), t.pal.mono_icon);
    }
    c.text_vc(&f.heading, label, card_x + m.dp(55.4), lcy, m.row_title_px, t.pal.text, None);
    slider(c, t, card_x + m.inner, slider_top, right - card_x - m.inner * 2.0, value);
    if let (Some(r), Some(l)) = (below, below_lay) {
        draw_row(c, t, f, &r, &l, slider_bottom + m.dp(17.4));
    }
    top + h
}

/// Swipe-to-confirm control in the thick-slider style.
pub fn confirm(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str, danger: bool) -> f32 {
    let m = &t.m;
    let accent = if danger { t.pal.danger } else { t.pal.accent };
    let (x, w, h) = (m.margin, m.w as f32 - m.margin * 2.0, m.dp(56.0));
    c.fill_round_rect(x, top, w, h, h / 2.0, if danger { t.pal.danger_soft } else { t.pal.badge });
    c.text_center(&f.heading, label, x + w / 2.0 + h / 4.0, top + h / 2.0, m.row_title_px, accent);
    let k = h - m.dp(8.0);
    let kcx = x + h / 2.0;
    let kcy = top + h / 2.0;
    c.fill_circle(kcx, kcy, k / 2.0, accent);
    let s = k * 0.3;
    c.line(kcx - s * 0.5, kcy, kcx + s * 0.5, kcy, m.stroke, Rgba::rgb(255, 255, 255));
    c.line(kcx + s * 0.5, kcy, kcx + s * 0.05, kcy - s * 0.45, m.stroke, Rgba::rgb(255, 255, 255));
    c.line(kcx + s * 0.5, kcy, kcx + s * 0.05, kcy + s * 0.45, m.stroke, Rgba::rgb(255, 255, 255));
    top + h
}

pub fn progress_bar(c: &mut Canvas, t: &Theme, x: f32, y: f32, w: f32, value: f32) {
    let m = &t.m;
    let h = m.dp(6.0);
    c.fill_round_rect(x, y, w, h, h / 2.0, t.pal.track);
    c.fill_round_rect(x, y, (w * value.clamp(0.0, 1.0)).max(h), h, h / 2.0, t.pal.accent);
}

pub fn pattern_lock(c: &mut Canvas, t: &Theme, cx: f32, cy: f32, span: f32, path: &[usize]) {
    let m = &t.m;
    let step = span / 2.0;
    let pos = |idx: usize| -> (f32, f32) { (cx - step + (idx % 3) as f32 * step, cy - step + (idx / 3) as f32 * step) };
    for w in path.windows(2) {
        let (ax, ay) = pos(w[0]);
        let (bx, by) = pos(w[1]);
        c.line(ax, ay, bx, by, m.dp(3.0), t.pal.accent.with_alpha(200));
    }
    for i in 0..9 {
        let (px, py) = pos(i);
        if path.contains(&i) {
            c.fill_circle(px, py, m.dp(14.0), t.pal.accent.with_alpha(60));
            c.fill_circle(px, py, m.dp(6.0), t.pal.accent);
        } else {
            c.fill_circle(px, py, m.dp(5.0), t.pal.value);
        }
    }
}
