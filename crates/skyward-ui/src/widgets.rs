use crate::canvas::Canvas;
use crate::color::{mix, Rgba};
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
    Sliders,
    Vibrate,
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
        Icon::Sliders => {
            for (dy, knob) in [(-0.27f32, 0.14f32), (0.0, -0.16), (0.27, 0.06)] {
                c.line(cx - 0.4 * g, cy + dy * g, cx + 0.4 * g, cy + dy * g, sw * 0.85, fg);
                c.fill_circle(cx + knob * g, cy + dy * g, 0.12 * g, fg);
                c.fill_circle(cx + knob * g, cy + dy * g, 0.05 * g, bg);
            }
        }
        Icon::Vibrate => {
            c.fill_round_rect(cx - 0.17 * g, cy - 0.38 * g, 0.34 * g, 0.76 * g, 0.08 * g, fg);
            c.fill_round_rect(cx - 0.09 * g, cy - 0.28 * g, 0.18 * g, 0.5 * g, 0.03 * g, bg);
            for side in [-1.0f32, 1.0] {
                let x = cx + side * 0.3 * g;
                c.line(x, cy - 0.2 * g, x + side * 0.07 * g, cy - 0.07 * g, sw * 0.8, fg);
                c.line(x + side * 0.07 * g, cy - 0.07 * g, x, cy + 0.07 * g, sw * 0.8, fg);
                c.line(x, cy + 0.07 * g, x + side * 0.07 * g, cy + 0.2 * g, sw * 0.8, fg);
            }
        }
    }
}

pub fn tile(c: &mut Canvas, t: &Theme, x: f32, y: f32, color: Rgba, icon: Icon) {
    let s = t.m.icon;
    c.fill_round_rect(x, y, s, s, t.m.icon_radius, color);
    glyph(c, icon, x + s / 2.0, y + s / 2.0, s, Rgba::rgb(255, 255, 255), color);
}

/// The colours a card is drawn in: the page's own, or light on dark over the
/// About backdrop.
#[derive(Clone, Copy)]
pub struct Tone {
    pub card: Rgba,
    pub text: Rgba,
    pub sub: Rgba,
    pub value: Rgba,
    pub chevron: Rgba,
    pub caption: Rgba,
}

impl Tone {
    pub fn page(t: &Theme) -> Tone {
        let p = &t.pal;
        Tone { card: p.card, text: p.text, sub: p.subtitle, value: p.value, chevron: p.chevron, caption: p.spec_label }
    }

    pub fn hero(t: &Theme) -> Tone {
        let h = &t.hero;
        Tone { card: h.card, text: h.title, sub: h.value, value: h.value, chevron: h.chevron, caption: h.value }
    }
}

fn chevron(c: &mut Canvas, t: &Theme, right: f32, cy: f32, color: Rgba) {
    let m = &t.m;
    let (w, h, sw) = (m.chevron_w, m.chevron_h, m.chevron_stroke);
    let x0 = right - w + sw / 2.0;
    let x1 = right - sw / 2.0;
    c.line(x0, cy - h / 2.0 + sw / 2.0, x1, cy, sw, color);
    c.line(x1, cy, x0, cy + h / 2.0 - sw / 2.0, sw, color);
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

/// Status-bar contents: the clock and, when a fuel gauge is present, the
/// battery level and whether it is charging.
pub struct Bar<'a> {
    pub clock: &'a str,
    pub battery: Option<(u8, bool)>,
}

/// Clock on the left; on the right the battery capsule with its level, a
/// grey cap, and a bolt while charging. `ink` colours the clock and bolt.
pub fn status_bar(c: &mut Canvas, t: &Theme, f: &Fonts, bar: &Bar, ink: Rgba) {
    let m = &t.m;
    let cy = m.status_cy;
    // Digits carry no descender, so centre on half their height (≈0.72 em).
    c.text(&f.status, bar.clock, m.status_x, cy + m.status_px * 0.36, m.status_px, ink, None);
    let Some((batt, charging)) = bar.battery else { return };

    let mut right = m.w as f32 - m.status_right;
    if charging {
        let (bw, bh) = (m.dp(5.0), m.dp(11.3));
        let x = right - bw;
        c.fill_convex(&[(x + bw * 0.7, cy - bh / 2.0), (x, cy + bh * 0.1), (x + bw * 0.52, cy + bh * 0.1)], ink);
        c.fill_convex(&[(x + bw * 0.3, cy + bh / 2.0), (x + bw, cy - bh * 0.1), (x + bw * 0.48, cy - bh * 0.1)], ink);
        right = x - m.dp(3.0);
    }
    let (cap_w, cap_h) = (m.dp(1.0), m.dp(4.2));
    c.fill_round_rect(right - cap_w, cy - cap_h / 2.0, cap_w, cap_h, cap_w / 2.0, t.pal.battery_cap);
    let (bw, bh) = (m.dp(21.3), m.dp(12.3));
    let bx = right - cap_w - m.dp(0.7) - bw;
    c.fill_round_rect(bx, cy - bh / 2.0, bw, bh, m.dp(4.0), t.pal.battery);
    c.text_center(&f.strong, &batt.to_string(), bx + bw / 2.0, cy, m.dp(9.7), Rgba::rgb(255, 255, 255));
}

/// The `<` back control.
pub fn back_arrow(c: &mut Canvas, t: &Theme, color: Rgba) {
    let m = &t.m;
    let (x, cy, w, h, sw) = (m.back_x, m.back_cy, m.back_w, m.back_h, m.back_stroke);
    let (tip, end) = (x + sw / 2.0, x + w - sw / 2.0);
    c.line(tip, cy, end, cy - h / 2.0 + sw / 2.0, sw, color);
    c.line(tip, cy, end, cy + h / 2.0 - sw / 2.0, sw, color);
}

/// Status bar, optional `<`, and the large page title. Returns where the page
/// content starts.
pub fn header(c: &mut Canvas, t: &Theme, f: &Fonts, bar: &Bar, title: &str, back: bool) -> f32 {
    let m = &t.m;
    status_bar(c, t, f, bar, t.pal.status);
    if back {
        back_arrow(c, t, t.pal.text);
    }
    c.text_top(&f.title, title, m.title_x, m.title_top, m.title_px, t.pal.text, None);
    m.content_top
}

pub fn search_bar(c: &mut Canvas, t: &Theme, f: &Fonts, y: f32, placeholder: &str) -> f32 {
    let m = &t.m;
    let (x, w, h) = (m.margin, m.w as f32 - m.margin * 2.0, m.search_h);
    let cy = y + h / 2.0;
    c.fill_round_rect(x, y, w, h, h / 2.0, t.pal.search);
    let (icx, icy, sw) = (x + m.dp(22.3), cy - m.dp(2.0), m.dp(1.45));
    let ring = m.dp(6.6);
    c.stroke_circle(icx, icy, ring, sw, t.pal.search_icon);
    let k = ring * std::f32::consts::FRAC_1_SQRT_2;
    c.line(icx + k, icy + k, icx + m.dp(9.9), icy + m.dp(10.0), sw, t.pal.search_icon);
    // The placeholder is set at the row-title size and weight, in grey.
    c.text_vc(&f.heading, placeholder, x + m.search_text_inset, cy, m.row_title_px, t.pal.placeholder, None);
    y + h + m.after_search
}

/// Section caption between two cards. `prev_bottom` is the bottom of the card
/// above (or the content top for the first section). Returns the next card top.
pub fn section(c: &mut Canvas, t: &Theme, f: &Fonts, prev_bottom: f32, label: &str) -> f32 {
    let m = &t.m;
    let ink_top = prev_bottom + m.section_label_top;
    c.text_top(&f.label, label, m.margin + m.inner, ink_top, m.label_px, t.pal.section, None);
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

fn draw_row(c: &mut Canvas, t: &Theme, f: &Fonts, r: &Row, lay: &Layout, top: f32, tone: &Tone) {
    let m = &t.m;
    let card_x = m.margin;
    let right = m.w as f32 - m.margin;
    let cy = top + lay.h / 2.0;
    if let Some((color, icon)) = r.icon {
        tile(c, t, card_x + m.inner, cy - m.icon / 2.0, color, icon);
    }
    let tx = card_x + lay.text_x;
    let color = r.tint.unwrap_or(tone.text);
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
            c.text_top(&f.body, line, tx, st_top, m.sub_px, tone.sub, None);
            st_top += m.sub_pitch;
        }
    }
    match r.trail {
        Trail::None => {}
        Trail::Arrow => chevron(c, t, right - m.chevron_inset, cy, tone.chevron),
        Trail::Value(s) => {
            let chev_left = right - m.chevron_inset - m.chevron_w;
            chevron(c, t, right - m.chevron_inset, cy, tone.chevron);
            let w = c.text_width(&f.value, s, m.sub_px);
            c.text_vc(&f.value, s, chev_left - m.value_gap - w, cy, m.sub_px, tone.value, None);
        }
        Trail::Text(s) => {
            let w = c.text_width(&f.value, s, m.sub_px);
            c.text_vc(&f.value, s, right - m.inner - w, cy, m.sub_px, tone.value, None);
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
    list_toned(c, t, f, top, rows, &Tone::page(t))
}

/// [`list`] in the given colours.
pub fn list_toned(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, rows: &[Row], tone: &Tone) -> f32 {
    let m = &t.m;
    let lays: Vec<Layout> = rows.iter().map(|r| layout(c, t, f, r)).collect();
    let h: f32 = lays.iter().map(|l| l.h).sum();
    c.fill_round_rect(m.margin, top, m.w as f32 - m.margin * 2.0, h, m.card_radius, tone.card);
    let mut y = top;
    for (r, l) in rows.iter().zip(&lays) {
        draw_row(c, t, f, r, l, y, tone);
        y += l.h;
    }
    top + h
}

/// Account-style card: avatar, name and detail, then a strip of key
/// readings (value over caption, spread edge to edge), then optional rows.
#[allow(clippy::too_many_arguments)]
pub fn device_card(
    c: &mut Canvas,
    t: &Theme,
    f: &Fonts,
    top: f32,
    title: &str,
    sub: &str,
    stats: &[(&str, &str)],
    rows: &[Row],
) -> f32 {
    let m = &t.m;
    let lays: Vec<Layout> = rows.iter().map(|r| layout(c, t, f, r)).collect();
    let strip_h = if stats.is_empty() { 0.0 } else { m.dp(62.0) };
    let h = m.account_row_h + strip_h + lays.iter().map(|l| l.h).sum::<f32>();
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, t.pal.card);

    let strip_top = top + m.account_row_h;
    let widths: Vec<f32> = stats
        .iter()
        .map(|(v, cap)| c.text_width(&f.heading, v, m.row_title_px).max(c.text_width(&f.body, cap, m.sub_px)))
        .collect();
    let span = right - card_x - m.inner * 2.0;
    let gap = (span - widths.iter().sum::<f32>()) / (stats.len().max(2) - 1) as f32;
    let mut x = card_x + m.inner;
    let base = strip_top + m.dp(20.0);
    for ((value, caption), w) in stats.iter().zip(&widths) {
        c.text(&f.heading, value, x, base, m.row_title_px, t.pal.text, None);
        c.text(&f.body, caption, x, base + m.dp(20.0), m.sub_px, t.pal.subtitle, None);
        x += w + gap;
    }

    let cy = top + m.account_row_h / 2.0;
    let av = m.avatar;
    let acx = card_x + m.avatar_inset + av / 2.0;
    c.fill_circle(acx, cy, av / 2.0, t.pal.avatar);
    glyph(c, Icon::Phone, acx, cy, av * 0.86, t.pal.avatar_glyph, t.pal.avatar);

    let tx = card_x + m.inner + m.icon + m.icon_gap;
    let (tt, tb) = f.heading.ink(m.account_title_px);
    let (st, sb) = f.body.ink(m.sub_px);
    let block = (tb - tt) + m.sub_gap + (sb - st);
    let ink_top = cy - block / 2.0;
    c.text_top(&f.heading, title, tx, ink_top, m.account_title_px, t.pal.text, None);
    c.text_top(&f.body, sub, tx, ink_top + (tb - tt) + m.sub_gap, m.sub_px, t.pal.subtitle, None);
    chevron(c, t, right - m.chevron_inset, cy, t.pal.chevron);

    let tone = Tone::page(t);
    let mut y = strip_top + strip_h;
    for (r, l) in rows.iter().zip(&lays) {
        draw_row(c, t, f, r, l, y, &tone);
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
    c.fill_circle(x + fw - h / 2.0, top + h / 2.0, m.slider_knob / 2.0, Rgba::rgb(255, 255, 255));
}

/// Card with a monochrome icon caption, a slider, and an optional row below.
pub fn slider_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str, value: f32, below: Option<Row>) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let below_lay = below.as_ref().map(|r| layout(c, t, f, r));
    let slider_top = top + m.dp(55.0);
    let slider_bottom = slider_top + m.slider_h;
    let below_gap = m.dp(16.3);
    let h = match &below_lay {
        Some(l) => slider_bottom - top + below_gap + l.h,
        None => slider_bottom - top + m.dp(18.0),
    };
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, t.pal.card);

    let lcy = top + m.dp(31.2);
    let scx = card_x + m.dp(28.0);
    c.fill_circle(scx, lcy, m.dp(5.7), t.pal.mono_icon);
    for k in 0..8 {
        let a = k as f32 * FRAC_PI_4;
        let (ux, uy) = (a.cos(), a.sin());
        c.line(scx + ux * m.dp(8.3), lcy + uy * m.dp(8.3), scx + ux * m.dp(8.9), lcy + uy * m.dp(8.9), m.dp(2.0), t.pal.mono_icon);
    }
    // The caption is set like the account name, in the softer text colour.
    c.text_vc(&f.heading, label, card_x + m.dp(53.7), lcy, m.account_title_px, t.pal.info, None);
    slider(c, t, card_x + m.inner, slider_top, right - card_x - m.inner * 2.0, value);
    if let (Some(r), Some(l)) = (below, below_lay) {
        draw_row(c, t, f, &r, &l, slider_bottom + below_gap, &Tone::page(t));
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

/// Grey line icons of the parameters page, drawn on a 24 dp grid.
#[derive(Clone, Copy)]
pub enum Outline {
    Phone,
    Battery,
    Chip,
    Memory,
    Resolution,
    Screen,
    Storage,
    Tag,
}

pub fn outline(c: &mut Canvas, t: &Theme, icon: Outline, cx: f32, cy: f32) {
    let m = &t.m;
    let d = |v: f32| m.dp(v);
    let sw = d(1.7);
    let col = t.pal.outline;
    // Rounded box whose outer edge spans (x, y, w, h) in dp from the centre.
    let boxed = |c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32| {
        c.stroke_round_rect(cx + d(x) + sw / 2.0, cy + d(y) + sw / 2.0, d(w) - sw, d(h) - sw, d(r) - sw / 2.0, sw, col);
    };
    let seg = |c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32| c.line(cx + d(x0), cy + d(y0), cx + d(x1), cy + d(y1), sw, col);
    let ring = |c: &mut Canvas, pts: &[(f32, f32)]| {
        for (i, &(x0, y0)) in pts.iter().enumerate() {
            let (x1, y1) = pts[(i + 1) % pts.len()];
            seg(c, x0, y0, x1, y1);
        }
    };
    match icon {
        Outline::Phone => {
            boxed(c, -7.7, -10.0, 15.4, 20.0, 3.4);
            seg(c, -3.1, 5.9, 3.1, 5.9);
        }
        Outline::Battery => {
            boxed(c, -10.9, -7.15, 19.0, 14.3, 3.4);
            c.fill_round_rect(cx + d(9.2), cy - d(2.85), d(1.7), d(5.7), d(0.85), col);
        }
        Outline::Chip => {
            boxed(c, -7.0, -7.0, 14.0, 14.0, 3.0);
            for k in [-2.6f32, 2.6] {
                seg(c, k, -9.6, k, -7.4);
                seg(c, k, 7.4, k, 9.6);
                seg(c, -9.6, k, -7.4, k);
                seg(c, 7.4, k, 9.6, k);
            }
        }
        Outline::Memory => {
            boxed(c, -8.0, -8.0, 16.0, 16.0, 4.0);
            for k in [-3.4f32, 0.0, 3.4] {
                seg(c, k, -10.3, k, -8.2);
                seg(c, k, 8.2, k, 10.3);
            }
        }
        Outline::Resolution => {
            boxed(c, -9.5, -9.5, 19.0, 19.0, 4.5);
            for k in 0..4 {
                let s = -4.4 + k as f32 * 2.95;
                seg(c, s, s, s + 0.9, s + 0.9);
            }
        }
        Outline::Screen => {
            boxed(c, -9.5, -9.5, 19.0, 19.0, 4.5);
            seg(c, -4.6, -1.4, -4.6, -4.6);
            seg(c, -4.6, -4.6, -1.4, -4.6);
            seg(c, 4.6, 1.4, 4.6, 4.6);
            seg(c, 4.6, 4.6, 1.4, 4.6);
        }
        Outline::Storage => {
            ring(c, &[(-2.6, -8.65), (5.95, -8.65), (5.95, 8.65), (-5.95, 8.65), (-5.95, -5.3)]);
            for x in [-0.6f32, 1.6, 3.8] {
                seg(c, x, -5.6, x, -3.4);
            }
        }
        Outline::Tag => {
            ring(c, &[(-8.6, 0.0), (-4.3, -5.95), (8.65, -5.95), (8.65, 5.95), (-4.3, 5.95)]);
            c.fill_circle(cx - d(3.3), cy, d(1.6), col);
        }
    }
}

/// The About page backdrop over the whole canvas: soft colour fields over a
/// vertical blend that keeps its bottom colour past the first screen.
pub fn hero_backdrop(c: &mut Canvas, t: &Theme) {
    let hero = t.hero;
    let (w, sh) = (t.m.w as f32, t.m.h as f32);
    c.shade_rows(0, c.h, |x, y| {
        let fy = y / sh;
        let base = if fy < 0.5 { mix(hero.top, hero.mid, fy / 0.5) } else { mix(hero.mid, hero.bottom, (fy - 0.5) / 0.5) };
        let mut acc = [base.r as f32, base.g as f32, base.b as f32];
        let mut sum = 1.0;
        for &(bx, by, br, col) in &hero.blobs {
            let dx = (x - bx * w) / (br * w);
            let dy = (y - by * sh) / (br * w);
            let k = 3.0 * (-(dx * dx + dy * dy)).exp();
            acc[0] += k * col.r as f32;
            acc[1] += k * col.g as f32;
            acc[2] += k * col.b as f32;
            sum += k;
        }
        let ch = |v: f32| (v / sum).round() as u8;
        Rgba::rgb(ch(acc[0]), ch(acc[1]), ch(acc[2]))
    });
}

/// Centred gradient lettering with its cap line at `cap_top`.
pub fn wordmark(c: &mut Canvas, t: &Theme, f: &Fonts, text: &str, cap_top: f32, px: f32) {
    let face = &f.display;
    let w = c.text_width(face, text, px);
    let x = (t.m.w as f32 - w) / 2.0;
    let ink = t.hero.ink;
    let paint = |gx: f32, _: f32| {
        let u = ((gx - x) / w).clamp(0.0, 1.0);
        if u < 0.5 {
            mix(ink[0], ink[1], u / 0.5)
        } else {
            mix(ink[1], ink[2], (u - 0.5) / 0.5)
        }
    };
    c.text_paint(face, text, x, cap_top + face.cap(px), px, &paint, None);
}

/// A row of the About summary card: title on the left, value on the right,
/// and a chevron when the row opens a page.
pub struct Fact<'a> {
    pub title: &'a str,
    pub value: String,
    pub link: bool,
}

/// The About summary card: rows a little shorter than settings rows.
pub fn facts(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, rows: &[Fact], tone: &Tone) -> f32 {
    let m = &t.m;
    let row_h = m.dp(51.0);
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let h = row_h * rows.len() as f32;
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, tone.card);
    for (i, r) in rows.iter().enumerate() {
        let cy = top + row_h * (i as f32 + 0.5);
        let tx = card_x + m.inner;
        let tw = c.text_vc(&f.heading, r.title, tx, cy, m.row_title_px, tone.text, None);
        let value_right = if r.link {
            chevron(c, t, right - m.chevron_inset, cy, tone.chevron);
            right - m.chevron_inset - m.chevron_w - m.value_gap
        } else {
            right - m.inner
        };
        let room = value_right - (tx + tw + m.dp(16.0));
        let w = c.text_width(&f.value, &r.value, m.sub_px).min(room);
        c.text_vc(&f.value, &r.value, value_right - w, cy, m.sub_px, tone.value, Some(room));
    }
    top + h
}

/// The About page hardware card: the model in large type (`title` may hold
/// several lines separated by `\n`), then values with small captions.
pub fn spec_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, title: &str, items: &[(&str, String)], tone: &Tone) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let x = card_x + m.inner;
    let room = right - m.inner - x;
    let title_px = m.dp(24.0);
    let lines: Vec<String> = title.split('\n').flat_map(|l| wrap(c, &f.heading, l, title_px, room)).collect();
    let first_title = top + m.dp(23.0) + f.heading.cap(title_px);
    let title_pitch = m.dp(33.0);
    let first = first_title + title_pitch * lines.len().saturating_sub(1) as f32 + m.dp(39.4);
    let (pitch, drop) = (m.dp(45.7), m.dp(15.5));
    let last_caption = first + pitch * items.len().saturating_sub(1) as f32 + drop;
    let bottom = last_caption + m.dp(19.7);
    c.fill_round_rect(card_x, top, right - card_x, bottom - top, m.card_radius, tone.card);
    for (k, line) in lines.iter().enumerate() {
        c.text(&f.heading, line, x, first_title + title_pitch * k as f32, title_px, tone.text, Some(room));
    }
    for (i, (caption, value)) in items.iter().enumerate() {
        let base = first + pitch * i as f32;
        c.text(&f.heading, value, x, base, m.dp(14.1), tone.text, Some(room));
        c.text(&f.body, caption, x, base + drop, m.dp(11.0), tone.caption, None);
    }
    bottom
}

/// One cell of the parameters grid. `value` may hold several lines separated
/// by `\n`; each is wrapped to the column.
pub struct Spec<'a> {
    pub icon: Outline,
    pub label: &'a str,
    pub value: String,
}

/// Two-column grid of icon, caption and value, as on the stock「全部参数」.
pub fn spec_grid(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, items: &[Spec]) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let cols = [card_x + m.inner, card_x + (right - card_x) * 0.507];
    let widths = [cols[1] - cols[0] - m.inner, right - m.inner - cols[1]];
    let (value_px, pitch) = (m.dp(16.0), m.dp(21.5));
    let (to_label, to_value) = (m.dp(30.8), m.dp(23.0));
    let cells: Vec<Vec<String>> = items
        .iter()
        .enumerate()
        .map(|(i, s)| s.value.split('\n').flat_map(|l| wrap(c, &f.heading, l, value_px, widths[i % 2])).collect())
        .collect();
    let mut centres = Vec::new();
    let mut cy = top + m.dp(38.2);
    let mut last = cy;
    for pair in cells.chunks(2) {
        centres.push(cy);
        let lines = pair.iter().map(Vec::len).max().unwrap_or(1).max(1);
        last = cy + to_label + to_value + pitch * (lines - 1) as f32;
        cy = last + m.dp(28.3);
    }
    let bottom = last + m.dp(19.7);
    c.fill_round_rect(card_x, top, right - card_x, bottom - top, m.card_radius, t.pal.card);
    for (i, (s, lines)) in items.iter().zip(&cells).enumerate() {
        let (x, icy) = (cols[i % 2], centres[i / 2]);
        outline(c, t, s.icon, x + m.dp(11.5), icy);
        let label_base = icy + to_label;
        c.text(&f.body, s.label, x, label_base, m.sub_px, t.pal.subtitle, Some(widths[i % 2]));
        for (k, l) in lines.iter().enumerate() {
            c.text(&f.heading, l, x, label_base + to_value + pitch * k as f32, value_px, t.pal.info, None);
        }
    }
    bottom
}

/// Parameters-page card of title-over-value entries; values wrap to the card.
pub fn info_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, items: &[(&str, String)]) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let x = card_x + m.inner;
    let width = right - m.inner - x;
    let (ink_top, _) = f.heading.ink(m.row_title_px);
    let (to_value, pitch) = (m.dp(20.2), m.dp(19.0));
    let blocks: Vec<Vec<String>> = items.iter().map(|(_, v)| wrap(c, &f.body, v, m.sub_px, width)).collect();
    let mut bases = Vec::new();
    let mut title_top = top + m.dp(23.5);
    let mut last = title_top;
    for lines in &blocks {
        let base = title_top - ink_top;
        bases.push(base);
        last = base + to_value + pitch * lines.len().saturating_sub(1) as f32;
        title_top = last + m.dp(33.0);
    }
    let bottom = last + m.dp(24.0);
    c.fill_round_rect(card_x, top, right - card_x, bottom - top, m.card_radius, t.pal.card);
    for ((title, _), (lines, base)) in items.iter().zip(blocks.iter().zip(&bases)) {
        c.text(&f.heading, title, x, *base, m.row_title_px, t.pal.info, Some(width));
        for (k, l) in lines.iter().enumerate() {
            c.text(&f.body, l, x, base + to_value + pitch * k as f32, m.sub_px, t.pal.subtitle, None);
        }
    }
    bottom
}

/// Light and dark previews side by side; the active one carries an accent
/// ring and an accent caption. Returns the bottom of the captions.
pub fn theme_picker(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32) -> f32 {
    let m = &t.m;
    let gap = m.dp(4.0);
    let slot_w = (m.w as f32 - m.margin * 2.0 - gap) / 2.0;
    let (ring, inset) = (m.dp(3.0), m.dp(6.0));
    let tile = slot_w - inset * 2.0;
    let label_px = m.dp(15.9);
    let (ink_top, ink_bottom) = f.heading.ink(label_px);
    let caption_base = top + slot_w + m.dp(11.0) - ink_top;
    for (i, (label, dark)) in [("浅色模式", false), ("深色模式", true)].into_iter().enumerate() {
        let sx = m.margin + (slot_w + gap) * i as f32;
        let on = t.dark == dark;
        if on {
            let r = m.dp(26.0) - ring / 2.0;
            c.stroke_round_rect(sx + ring / 2.0, top + ring / 2.0, slot_w - ring, slot_w - ring, r, ring, t.pal.accent);
        }
        theme_tile(c, t, sx + inset, top + inset, tile, tile, dark);
        let w = c.text_width(&f.heading, label, label_px);
        let color = if on { t.pal.accent } else { t.pal.info };
        c.text(&f.heading, label, sx + (slot_w - w) / 2.0, caption_base, label_px, color, None);
    }
    caption_base + ink_bottom
}

/// Miniature settings page: a blue backdrop with two cards of placeholder
/// lines, light cards for the light theme and dark ones for the dark theme.
fn theme_tile(c: &mut Canvas, t: &Theme, x: f32, y: f32, w: f32, h: f32, dark: bool) {
    let (top, bottom) = (Rgba::rgb(77, 113, 246), Rgba::rgb(115, 184, 249));
    let grad = |_: f32, py: f32| mix(top, bottom, (py - y) / h);
    c.fill_round_rect_paint(x, y, w, h, t.m.dp(20.0), &grad);
    let card = if dark { Rgba::rgb(0, 0, 0).with_alpha(51) } else { Rgba::rgb(255, 255, 255).with_alpha(51) };
    let lw = h * 0.027;
    for (c0, c1) in [(0.218f32, 0.472f32), (0.523, 0.779)] {
        c.fill_round_rect(x + w * 0.069, y + h * c0, w * 0.86, h * (c1 - c0), w * 0.045, card);
        for (at, end) in [(0.37f32, 0.841f32), (0.64, 0.622)] {
            let ly = y + h * (c0 + (c1 - c0) * at);
            let base = grad(0.0, ly);
            let ink = if dark { mix(base, Rgba::rgb(180, 190, 190), 0.45) } else { mix(base, Rgba::rgb(0, 0, 0), 0.14) };
            c.line(x + w * 0.162 + lw / 2.0, ly, x + w * end - lw / 2.0, ly, lw, ink);
        }
    }
}
