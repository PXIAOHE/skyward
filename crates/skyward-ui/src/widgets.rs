use crate::canvas::Canvas;
use crate::color::{blend_over, mix, Rgba};
use crate::font::{Face, Fonts};
use crate::geom::Rect;
use crate::sprite::Sprite;
use crate::theme::Theme;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

/// Where a drawn card ends and where each of its rows lies, for hit testing.
pub struct Block {
    pub bottom: f32,
    pub rows: Vec<Rect>,
}

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
    Folder,
    File,
    /// A page of text lines.
    Log,
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
        Icon::Folder => {
            c.fill_round_rect(cx - 0.42 * g, cy - 0.34 * g, 0.38 * g, 0.2 * g, 0.06 * g, fg);
            c.fill_round_rect(cx - 0.42 * g, cy - 0.24 * g, 0.84 * g, 0.62 * g, 0.08 * g, fg);
            c.line(cx - 0.42 * g, cy - 0.1 * g, cx + 0.42 * g, cy - 0.1 * g, 0.06 * g, bg);
        }
        Icon::File => {
            let fold = 0.2 * g;
            c.fill_convex(&[
                (cx - 0.3 * g, cy - 0.42 * g),
                (cx + 0.3 * g - fold, cy - 0.42 * g),
                (cx + 0.3 * g, cy - 0.42 * g + fold),
                (cx + 0.3 * g, cy + 0.42 * g),
                (cx - 0.3 * g, cy + 0.42 * g),
            ], fg);
            c.fill_convex(&[(cx + 0.3 * g - fold, cy - 0.42 * g), (cx + 0.3 * g - fold, cy - 0.42 * g + fold), (cx + 0.3 * g, cy - 0.42 * g + fold)], bg);
        }
        Icon::Log => {
            c.fill_round_rect(cx - 0.3 * g, cy - 0.42 * g, 0.6 * g, 0.84 * g, 0.08 * g, fg);
            for (k, len) in [0.36f32, 0.36, 0.22].iter().enumerate() {
                let y = cy - 0.18 * g + k as f32 * 0.18 * g;
                c.line(cx - 0.18 * g, y, cx - 0.18 * g + len * g, y, 0.07 * g, bg);
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

/// Punctuation that never starts a line; at the end of a full line it hangs
/// past the edge instead.
const NO_LINE_START: &str = "，。、；：！？）」』》%,.;:!?)]>";

/// Opening punctuation that never ends a line; it moves down with what it
/// opens.
const NO_LINE_END: &str = "（「『《([<";

pub(crate) fn wrap(c: &Canvas, face: &Face, text: &str, px: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for ch in text.chars() {
        let mut probe = line.clone();
        probe.push(ch);
        if !line.is_empty() && c.text_width(face, &probe, px) > max_w {
            if NO_LINE_START.contains(ch) {
                line.push(ch);
                continue;
            }
            // A Latin word, file names and links among them, moves to the
            // next line whole, unless it fills the line alone; so does an
            // opening bracket in front of what moves.
            let word = |r: char| r.is_ascii_alphanumeric() || matches!(r, '_' | '-' | '.' | '/' | ':');
            let mut split = line.len();
            if word(ch) {
                let start = line.char_indices().rev().take_while(|&(_, r)| word(r)).last().map(|(at, _)| at);
                if let Some(at) = start.filter(|&at| at > 0) {
                    split = at;
                }
            }
            if let Some((at, before)) = line[..split].char_indices().last() {
                if at > 0 && NO_LINE_END.contains(before) {
                    split = at;
                }
            }
            let rest = line.split_off(split);
            lines.push(line.trim_end().to_string());
            line = rest;
            if !(line.is_empty() && ch == ' ') {
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
/// battery level and whether the cable is in, charging or full.
pub struct Bar<'a> {
    pub clock: &'a str,
    pub battery: Option<(u8, bool)>,
}

/// Below this level the battery fills red while the cable is out.
pub const LOW_BATTERY: u8 = 20;

/// Clock on the left; on the right the battery as the stock status bar draws
/// it, with a bolt past it while the cable is in. `ink` colours the clock.
pub fn status_bar(c: &mut Canvas, t: &Theme, f: &Fonts, bar: &Bar, ink: Rgba) {
    let m = &t.m;
    let cy = m.status_cy;
    // Digits carry no descender, so centre on half their height (≈0.72 em).
    c.text(&f.status, bar.clock, m.status_x, cy + m.status_px * 0.36, m.status_px, ink, None);
    let Some((level, plugged)) = bar.battery else { return };
    let mut right = m.w as f32 - m.status_right;
    if plugged {
        let x = m.w as f32 - m.status_bolt_right - m.dp(BOLT_W_DP);
        let (art, ox, oy) = bolt(t, x, cy);
        c.lay(&art, ox, oy, 1.0);
        right = x - m.dp(2.75);
    }
    let (art, ox, oy) = battery(t, f, level, plugged, right, cy);
    c.lay(&art, ox, oy, 1.0);
}

const BOLT_W_DP: f32 = 5.67;

/// The bolt from `x`, centred on `cy`, as its own canvas and where it goes.
fn bolt(t: &Theme, x: f32, cy: f32) -> (Canvas, i32, i32) {
    let m = &t.m;
    let (bw, bh) = (m.dp(BOLT_W_DP), m.dp(11.3));
    let (ox, oy) = ((x - 1.0).floor(), (cy - bh / 2.0 - 1.0).floor());
    let (x, cy) = (x - ox, cy - oy);
    let mut art = Canvas::new((x + bw + 1.0).ceil() as u32, (cy + bh / 2.0 + 1.0).ceil() as u32);
    // Two strokes meeting in the middle, drawn as coverage first so their
    // overlap is not inked twice.
    let white = Rgba::rgb(255, 255, 255);
    art.fill_convex(&[(x + bw * 0.7, cy - bh / 2.0), (x, cy + bh * 0.1), (x + bw * 0.52, cy + bh * 0.1)], white);
    art.fill_convex(&[(x + bw * 0.3, cy + bh / 2.0), (x + bw, cy - bh * 0.1), (x + bw * 0.48, cy - bh * 0.1)], white);
    let ink = t.pal.battery.bolt;
    for px in art.data.as_chunks_mut::<4>().0 {
        let a = (px[3] as u32 * ink.a as u32 + 127) / 255;
        *px = [ink.r, ink.g, ink.b, a as u8];
    }
    (art, ox as i32, oy as i32)
}

/// The battery ending at `right`, centred on `cy`, as its own canvas and
/// where it goes: body and cap, the level's share filled from the left, the
/// digits in white over a charging or low fill and cut out of any other.
fn battery(t: &Theme, f: &Fonts, level: u8, plugged: bool, right: f32, cy: f32) -> (Canvas, i32, i32) {
    let m = &t.m;
    let ink = t.pal.battery;
    let (bw, bh, cap_w, cap_h) = (m.dp(21.75), m.dp(12.1), m.dp(1.3), m.dp(4.25));
    let left = right - bw - cap_w;
    let (ox, oy) = ((left - 1.0).floor(), (cy - bh / 2.0 - 1.0).floor());
    let (w, h) = ((right - ox + 1.0).ceil() as u32, (cy + bh / 2.0 - oy + 1.0).ceil() as u32);
    let (x, top, mid) = (left - ox, cy - bh / 2.0 - oy, cy - oy);
    // Coverage of the body with its cap, and of the digits.
    let white = Rgba::rgb(255, 255, 255);
    let mut shape = Canvas::new(w, h);
    shape.fill_round_rect(x, top, bw, bh, m.dp(3.7), white);
    shape.fill_round_rect(x + bw - cap_w, mid - cap_h / 2.0, cap_w * 2.0, cap_h, cap_w, white);
    let mut digits = Canvas::new(w, h);
    digits.text_center(&f.strong, &level.to_string(), x + bw / 2.0, mid, m.dp(9.7), white);
    // The level fills its share of the whole width, the cap's included, so
    // a full battery fills the cap too.
    let edge = x + (bw + cap_w) * level as f32 / 100.0;
    let low = !plugged && level < LOW_BATTERY;
    let fill = if plugged {
        ink.charging
    } else if low {
        ink.low
    } else {
        ink.level
    };
    let mut art = Canvas::new(w, h);
    for (i, px) in art.data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let a = shape.data[i * 4 + 3] as f32 / 255.0;
        if a == 0.0 {
            continue;
        }
        let k = (edge - (i as u32 % w) as f32).clamp(0.0, 1.0);
        let base = mix(ink.empty, fill, k);
        *px = [base.r, base.g, base.b, base.a];
        let d = digits.data[i * 4 + 3] as f32 / 255.0;
        if plugged || low {
            blend_over(px, white, d);
        } else {
            px[3] = (px[3] as f32 * (1.0 - d)).round() as u8;
        }
        px[3] = (px[3] as f32 * a).round() as u8;
    }
    (art, ox as i32, oy as i32)
}

/// The `<` back control.
pub fn back_arrow(c: &mut Canvas, t: &Theme, color: Rgba) {
    let m = &t.m;
    let (x, cy, w, h, sw) = (m.back_x, m.back_cy, m.back_w, m.back_h, m.back_stroke);
    let (tip, end) = (x + sw / 2.0, x + w - sw / 2.0);
    c.line(tip, cy, end, cy - h / 2.0 + sw / 2.0, sw, color);
    c.line(tip, cy, end, cy + h / 2.0 - sw / 2.0, sw, color);
}

/// The area that answers a tap on the `<` control.
pub fn back_rect(t: &Theme) -> Rect {
    let m = &t.m;
    Rect::new(0.0, m.back_cy - m.dp(28.0), m.back_x + m.back_w + m.dp(24.0), m.dp(56.0))
}

/// How deep the back cue bulges for a finger `travel` pixels in from the
/// edge: about 0.31 of the travel at first, flattening out at 42 dp.
pub fn back_cue_depth(t: &Theme, travel: f32) -> f32 {
    let d = (travel / t.m.px_per_dp).clamp(0.0, 272.6);
    t.m.dp(0.3096 * d - 5.679e-4 * d * d)
}

/// The back cue swelling from a screen edge while a finger swipes in from
/// it: a dark wave `depth` pixels deep at its crest around `cy`, edged in
/// light blue, holding a white `<` once lifting there would go back. The
/// `<` points back on either edge.
pub fn back_cue(c: &mut Canvas, t: &Theme, from_left: bool, cy: f32, depth: f32, arrow: bool) {
    const FILL: Rgba = Rgba::rgb(48, 48, 48);
    const RIM: Rgba = Rgba::rgb(117, 170, 246);
    let m = &t.m;
    if depth <= 0.0 {
        return;
    }
    // A Gaussian bell narrowed to nothing past 93 dp from its crest; the
    // rim hugs it and runs on along the edge as a thin line a little
    // further.
    let sigma = m.dp(60.7);
    let rim = m.dp(0.77);
    let taper = |dy: f32, from: f32| {
        let k = ((dy.abs() - from) / m.dp(24.0)).clamp(0.0, 1.0);
        1.0 - k * k * (3.0 - 2.0 * k)
    };
    let (fill_from, rim_from) = (m.dp(93.0), m.dp(111.0));
    let reach = rim_from + m.dp(24.0);
    let span = |c: &mut Canvas, y: i32, w: f32, color: Rgba| {
        let full = w.floor() as i32;
        for i in 0..=full {
            let cov = if i < full { 1.0 } else { w - full as f32 };
            let x = if from_left { i } else { m.w as i32 - 1 - i };
            c.blend(x, y, color, cov);
        }
    };
    for y in (cy - reach).floor() as i32..(cy + reach).ceil() as i32 {
        let dy = y as f32 + 0.5 - cy;
        let fill = depth * (-(dy / sigma).powi(2)).exp() * taper(dy, fill_from);
        span(c, y, fill + rim * taper(dy, rim_from), RIM);
        span(c, y, fill, FILL);
    }
    if arrow {
        let (w, h, sw) = (m.dp(6.0), m.dp(12.7), m.dp(2.1));
        let cx = if from_left { 0.53 * depth } else { m.w as f32 - 0.53 * depth };
        let (tip, end) = (cx - w / 2.0 + sw / 2.0, cx + w / 2.0 - sw / 2.0);
        let white = Rgba::rgb(255, 255, 255);
        c.line(tip, cy, end, cy - h / 2.0 + sw / 2.0, sw, white);
        c.line(tip, cy, end, cy + h / 2.0 - sw / 2.0, sw, white);
    }
}

/// Room left around title ink for it to blur into.
fn blur_room(t: &Theme) -> f32 {
    t.m.dp(14.0)
}

/// The large title a page opens with, as ink that fades and blurs away
/// while the page scrolls; it sits where the page starts and scrolls with
/// it.
pub fn big_title(t: &Theme, f: &Fonts, title: &str) -> Sprite {
    let m = &t.m;
    let pad = blur_room(t);
    let w = Canvas::new(0, 0).text_width(&f.title, title, m.title_px);
    let (x, y) = (m.title_x - pad, m.title_top - pad);
    let size = ((w + pad * 2.0).ceil() as u32, (m.title_px * 1.4 + pad * 2.0).ceil() as u32);
    Sprite::capture(x, y, size.0, size.1, |c, dx, dy| {
        c.text_top(&f.title, title, m.title_x + dx, m.title_top + dy, m.title_px, Rgba::rgb(0, 0, 0), None);
    })
}

/// The page title centred in the bar over a scrolled page, as ink that
/// fades and sharpens in.
pub fn small_title(t: &Theme, f: &Fonts, title: &str) -> Sprite {
    let m = &t.m;
    let (px, pad) = (m.account_title_px, blur_room(t));
    let w = Canvas::new(0, 0).text_width(&f.heading, title, px);
    let h = px * 1.5;
    let (x, y) = ((m.w as f32 - w) / 2.0 - pad, m.back_cy - h / 2.0 - pad);
    Sprite::capture(x, y, (w + pad * 2.0).ceil() as u32, (h + pad * 2.0).ceil() as u32, |c, dx, dy| {
        c.text_center(&f.heading, title, m.w as f32 / 2.0 + dx, m.back_cy + dy, px, Rgba::rgb(0, 0, 0));
    })
}

/// Where the bar over a scrolled page ends: the status bar and the 56 dp
/// title bar under it.
pub fn bar_bottom(t: &Theme) -> f32 {
    t.m.back_cy + t.m.dp(28.0)
}

/// Strength of the bar's mask from its top (0) to 20 dp below the bar (1).
pub const BAR_MASK: [(f32, f32); 17] = [
    (0.0, 0.98),
    (0.5, 0.98),
    (0.555, 0.97),
    (0.605, 0.94),
    (0.65, 0.91),
    (0.69, 0.86),
    (0.725, 0.8),
    (0.76, 0.74),
    (0.785, 0.67),
    (0.815, 0.59),
    (0.84, 0.51),
    (0.865, 0.42),
    (0.89, 0.34),
    (0.915, 0.25),
    (0.94, 0.16),
    (0.97, 0.08),
    (1.0, 0.0),
];

/// The About page's mask, solid at its top.
pub const ABOUT_BAR_MASK: [(f32, f32); 5] = [(0.0, 1.0), (0.52, 0.98), (0.74, 0.8), (0.85, 0.5), (1.0, 0.0)];

/// The mask over the top of a scrolled page: `color`, nearly solid behind
/// the status bar and title, thinning out by 20 dp below them, with an
/// ordered dither against banding. `level` brings it in and out.
pub fn bar_mask(c: &mut Canvas, t: &Theme, stops: &[(f32, f32)], color: Rgba, level: f32) {
    mask(c, bar_bottom(t) + t.m.dp(20.0), stops, color, level, 0..0);
}

/// How far below a pinned strip the list thins out from under the mask, as
/// the stock file manager's does.
const STRIP_FADE_DP: f32 = 40.0;

/// The mask over a scrolled page with an opaque strip from `top` to
/// `bottom` held under the bar: as solid as the bar's own down to the strip,
/// then thinning out below it the way the bar's mask does. The strip's own
/// rows are left to it.
pub fn strip_mask(c: &mut Canvas, t: &Theme, color: Rgba, level: f32, top: f32, bottom: f32) {
    let h = bottom + t.m.dp(STRIP_FADE_DP);
    let s = bottom / h;
    // The bar mask from where it starts to thin, stretched over the fade.
    let (solid, _) = BAR_MASK.iter().rev().find(|&&(_, a)| a == BAR_MASK[0].1).expect("the bar mask starts solid");
    let mut stops = vec![(0.0, BAR_MASK[0].1)];
    stops.extend(BAR_MASK.iter().filter(|&&(p, _)| p >= *solid).map(|&(p, a)| (s + (p - solid) / (1.0 - solid) * (1.0 - s), a)));
    mask(c, h, &stops, color, level, top.max(0.0) as u32..bottom.max(0.0) as u32);
}

/// `color` over the top `h` pixels at the strength `stops` give from top (0)
/// to bottom (1), with an ordered dither against banding; the rows in `skip`
/// are left as they are.
fn mask(c: &mut Canvas, h: f32, stops: &[(f32, f32)], color: Rgba, level: f32, skip: std::ops::Range<u32>) {
    const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let ink = [color.r as u32, color.g as u32, color.b as u32];
    let row_len = c.w as usize * 4;
    for y in (0..(h.ceil() as u32).min(c.h)).filter(|y| !skip.contains(y)) {
        let f = (y as f32 + 0.5) / h;
        let a = ramp(stops, f) * level * 255.0;
        if a <= 0.0 {
            continue;
        }
        // The dither shifts the opacity by under half a step, in a 4×4
        // pattern. Opacities run 0 to 256 so a shift divides by them.
        let k: [u32; 4] = std::array::from_fn(|i| {
            let k = (a + BAYER[y as usize % 4][i] as f32 / 16.0 - 0.5).clamp(0.0, 255.0).round() as u32;
            k + (k >> 7)
        });
        let lit: [[u32; 3]; 4] = k.map(|k| ink.map(|v| v * k + 128));
        let start = y as usize * row_len;
        for quad in c.data[start..start + row_len].as_chunks_mut::<16>().0 {
            for (i, px) in quad.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let keep = 256 - k[i];
                for ch in 0..3 {
                    px[ch] = ((lit[i][ch] + px[ch] as u32 * keep) >> 8) as u8;
                }
            }
        }
    }
}

/// The piecewise-linear value of `stops` at `f`.
fn ramp(stops: &[(f32, f32)], f: f32) -> f32 {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if f <= first.0 {
        return first.1;
    }
    if f >= last.0 {
        return last.1;
    }
    let i = stops.iter().position(|s| s.0 > f).expect("f lies inside the stops");
    let ((p0, a0), (p1, a1)) = (stops[i - 1], stops[i]);
    a0 + (a1 - a0) * (f - p0) / (p1 - p0)
}

/// The round chip behind `<` while the bar shows: a pale disc with a
/// bright rim and a faint shadow. Drawn once per theme on its own canvas,
/// returned with where that canvas's top-left corner goes.
pub fn back_chip(t: &Theme) -> (Canvas, i32, i32) {
    let m = &t.m;
    let (cx, cy, r) = (m.back_x + m.back_w / 2.0 + m.dp(1.55), m.back_cy, m.dp(21.7));
    let (fill, rim) = if t.dark { (Rgba::rgb(38, 38, 38), Rgba::rgb(62, 62, 62)) } else { (Rgba::rgb(248, 248, 248), Rgba::rgb(255, 255, 255)) };
    let spread = m.dp(7.0);
    let reach = r + spread;
    let (x0, y0) = ((cx - reach).floor() as i32 - 1, (cy - reach).floor() as i32 - 1);
    let size = (reach * 2.0).ceil() as u32 + 3;
    let mut c = Canvas::new(size, size);
    let (cx, cy) = (cx - x0 as f32, cy - y0 as f32);
    for py in 0..size as i32 {
        for px in 0..size as i32 {
            let d = (px as f32 + 0.5 - cx).hypot(py as f32 + 0.5 - cy) - r;
            if d > 0.0 && d < spread {
                let k = 1.0 - d / spread;
                c.blend(px, py, Rgba::rgb(0, 0, 0), 0.04 * k * k);
            }
        }
    }
    c.fill_circle(cx, cy, r, fill);
    let rim_w = m.dp(0.8);
    c.stroke_circle(cx, cy, r - rim_w / 2.0, rim_w, rim);
    (c, x0, y0)
}

/// A short message in a dark capsule above the bottom edge.
pub fn toast(c: &mut Canvas, t: &Theme, f: &Fonts, text: &str) {
    let m = &t.m;
    let px = m.sub_px;
    let w = c.text_width(&f.heading, text, px) + m.dp(40.0);
    let h = m.dp(44.0);
    let (x, y) = ((m.w as f32 - w) / 2.0, m.h as f32 - m.dp(120.0));
    c.fill_round_rect(x, y, w, h, h / 2.0, Rgba::rgb(30, 30, 30).with_alpha(235));
    c.text_center(&f.heading, text, m.w as f32 / 2.0, y + h / 2.0, px, Rgba::rgb(255, 255, 255));
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

/// What a search field shows and where its parts landed.
pub struct SearchField {
    pub field: Rect,
    /// The clear button, while there is text to clear.
    pub clear: Option<Rect>,
}

/// The search bar opened for typing, from `x` across `w` and centred on
/// `cy`: the text typed so far with the cursor after it, or the placeholder,
/// and a button that clears the text.
#[allow(clippy::too_many_arguments)]
pub fn search_field(c: &mut Canvas, t: &Theme, f: &Fonts, x: f32, w: f32, cy: f32, text: &str, placeholder: &str) -> SearchField {
    let m = &t.m;
    let h = m.search_h;
    let y = cy - h / 2.0;
    c.fill_round_rect(x, y, w, h, h / 2.0, t.pal.search);
    let (icx, icy, sw) = (x + m.dp(22.3), cy - m.dp(2.0), m.dp(1.45));
    let ring = m.dp(6.6);
    c.stroke_circle(icx, icy, ring, sw, t.pal.search_icon);
    let k = ring * std::f32::consts::FRAC_1_SQRT_2;
    c.line(icx + k, icy + k, icx + m.dp(9.9), icy + m.dp(10.0), sw, t.pal.search_icon);
    let tx = x + m.search_text_inset;
    let clear_d = m.dp(18.0);
    let room = w - m.search_text_inset - clear_d - m.dp(24.0);
    let (ink_top, ink_bottom) = f.heading.ink(m.row_title_px);
    let cursor_x = if text.is_empty() {
        c.text_vc(&f.heading, placeholder, tx, cy, m.row_title_px, t.pal.placeholder, None);
        tx
    } else {
        // A long query shows its end, where the typing is.
        let mut shown = text;
        while c.text_width(&f.heading, shown, m.row_title_px) > room {
            let mut chars = shown.chars();
            chars.next();
            shown = chars.as_str();
        }
        tx + c.text_vc(&f.heading, shown, tx, cy, m.row_title_px, t.pal.text, None)
    };
    c.line(cursor_x + m.dp(1.5), cy + ink_top - m.dp(2.0), cursor_x + m.dp(1.5), cy + ink_bottom + m.dp(2.0), m.dp(2.0), t.pal.accent);
    let clear = (!text.is_empty()).then(|| {
        let (ccx, r) = (x + w - m.dp(12.0) - clear_d / 2.0, clear_d / 2.0);
        c.fill_circle(ccx, cy, r, t.pal.search_icon);
        let s = r * 0.42;
        c.line(ccx - s, cy - s, ccx + s, cy + s, m.dp(1.6), t.pal.search);
        c.line(ccx - s, cy + s, ccx + s, cy - s, m.dp(1.6), t.pal.search);
        Rect::new(ccx - h / 2.0, y, h, h)
    });
    SearchField { field: Rect::new(x, y, w, h), clear }
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

/// Body text set on the page itself, as a licence or notice reads,
/// wrapped to the content width; a blank line starts a new paragraph.
/// Returns where the text ends.
pub fn paragraph(c: &mut Canvas, t: &Theme, f: &Fonts, y: f32, text: &str) -> f32 {
    let m = &t.m;
    let x = m.title_x + m.dp(0.9);
    let max_w = m.w as f32 - x - m.title_x;
    let mut top = y + m.dp(12.0);
    for (k, para) in text.split("\n\n").enumerate() {
        if k > 0 {
            top += m.dp(12.0);
        }
        let lines: Vec<String> = para.lines().flat_map(|l| wrap(c, &f.body, l, m.sub_px, max_w)).collect();
        for line in &lines {
            c.text_top(&f.body, line, x, top, m.sub_px, t.pal.info, None);
            top += m.sub_pitch;
        }
    }
    top
}

/// A full-width button in the accent colour, for a page's one action that
/// destroys nothing. Returns its rect.
pub fn button(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str) -> Rect {
    let m = &t.m;
    let (x, w, h) = (m.margin, m.w as f32 - m.margin * 2.0, m.dp(48.0));
    c.fill_round_rect(x, top, w, h, h / 2.0, t.pal.accent);
    c.text_center(&f.heading, label, x + w / 2.0, top + h / 2.0, m.row_title_px, Rgba::rgb(255, 255, 255));
    Rect::new(x, top, w, h)
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

/// A flat settings card holding `rows`.
pub fn list(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, rows: &[Row]) -> Block {
    list_toned(c, t, f, top, rows, &Tone::page(t))
}

/// [`list`] in the given colours.
pub fn list_toned(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, rows: &[Row], tone: &Tone) -> Block {
    let m = &t.m;
    let lays: Vec<Layout> = rows.iter().map(|r| layout(c, t, f, r)).collect();
    let h: f32 = lays.iter().map(|l| l.h).sum();
    let w = m.w as f32 - m.margin * 2.0;
    c.fill_round_rect(m.margin, top, w, h, m.card_radius, tone.card);
    let mut y = top;
    let mut rects = Vec::new();
    for (r, l) in rows.iter().zip(&lays) {
        draw_row(c, t, f, r, l, y, tone);
        rects.push(Rect::new(m.margin, y, w, l.h));
        y += l.h;
    }
    Block { bottom: top + h, rows: rects }
}

/// Account-style card: avatar, name and detail, then a strip of key
/// readings (value over caption, spread edge to edge), then optional rows.
/// The first rect returned covers the name row.
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
) -> Block {
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
    let mut rects = vec![Rect::new(card_x, top, right - card_x, m.account_row_h)];
    let mut y = strip_top + strip_h;
    for (r, l) in rows.iter().zip(&lays) {
        draw_row(c, t, f, r, l, y, &tone);
        rects.push(Rect::new(card_x, y, right - card_x, l.h));
        y += l.h;
    }
    Block { bottom: top + h, rows: rects }
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

/// Where a slider card ends, its track, and the row under the slider.
pub struct SliderBlock {
    pub bottom: f32,
    pub track: Rect,
    pub below: Option<Rect>,
}

/// Card with a monochrome icon caption, a slider, and an optional row below.
pub fn slider_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str, value: f32, below: Option<Row>) -> SliderBlock {
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
    let track = Rect::new(card_x + m.inner, slider_top, right - card_x - m.inner * 2.0, m.slider_h);
    slider(c, t, track.x, track.y, track.w, value);
    let mut below_rect = None;
    if let (Some(r), Some(l)) = (below, below_lay) {
        let y = slider_bottom + below_gap;
        draw_row(c, t, f, &r, &l, y, &Tone::page(t));
        below_rect = Some(Rect::new(card_x, y, right - card_x, l.h));
    }
    SliderBlock { bottom: top + h, track, below: below_rect }
}

/// Swipe-to-confirm control in the thick-slider style; `progress` (0 to 1)
/// is how far the knob has been dragged. Returns the track.
pub fn confirm(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, label: &str, danger: bool, progress: f32) -> Rect {
    let m = &t.m;
    let accent = if danger { t.pal.danger } else { t.pal.accent };
    let (x, w, h) = (m.margin, m.w as f32 - m.margin * 2.0, m.dp(56.0));
    c.fill_round_rect(x, top, w, h, h / 2.0, if danger { t.pal.danger_soft } else { t.pal.badge });
    c.text_center(&f.heading, label, x + w / 2.0 + h / 4.0, top + h / 2.0, m.row_title_px, accent);
    let k = h - m.dp(8.0);
    let kcx = x + h / 2.0 + (w - h) * progress.clamp(0.0, 1.0);
    let kcy = top + h / 2.0;
    if progress > 0.0 {
        c.fill_round_rect(x, top, kcx + h / 2.0 - x, h, h / 2.0, accent.with_alpha(60));
    }
    c.fill_circle(kcx, kcy, k / 2.0, accent);
    let s = k * 0.3;
    c.line(kcx - s * 0.5, kcy, kcx + s * 0.5, kcy, m.stroke, Rgba::rgb(255, 255, 255));
    c.line(kcx + s * 0.5, kcy, kcx + s * 0.05, kcy - s * 0.45, m.stroke, Rgba::rgb(255, 255, 255));
    c.line(kcx + s * 0.5, kcy, kcx + s * 0.05, kcy + s * 0.45, m.stroke, Rgba::rgb(255, 255, 255));
    Rect::new(x, top, w, h)
}

pub fn progress_bar(c: &mut Canvas, t: &Theme, x: f32, y: f32, w: f32, value: f32) {
    let m = &t.m;
    let h = m.dp(6.0);
    c.fill_round_rect(x, y, w, h, h / 2.0, t.pal.track);
    c.fill_round_rect(x, y, (w * value.clamp(0.0, 1.0)).max(h), h, h / 2.0, t.pal.accent);
}

/// A card for a long job: what is under way, a line on where it stands,
/// and a bar when the share done is known. Returns the card's bottom.
pub fn progress_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, title: &str, detail: &str, done: Option<f32>) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let x = card_x + m.inner;
    let width = right - m.inner - x;
    let (ink_top, _) = f.heading.ink(m.row_title_px);
    let title_base = top + m.dp(23.5) - ink_top;
    let detail_base = title_base + m.dp(20.2);
    let bar_top = detail_base + m.dp(18.0);
    let bottom = match done {
        Some(_) => bar_top + m.dp(6.0) + m.dp(22.0),
        None => detail_base + m.dp(24.0),
    };
    c.fill_round_rect(card_x, top, right - card_x, bottom - top, m.card_radius, t.pal.card);
    c.text(&f.heading, title, x, title_base, m.row_title_px, t.pal.text, Some(width));
    c.text(&f.body, detail, x, detail_base, m.sub_px, t.pal.subtitle, Some(width));
    if let Some(v) = done {
        progress_bar(c, t, x, bar_top, width, v);
    }
    bottom
}

/// Centres of the nine pattern dots, row by row.
pub fn pattern_dots(cx: f32, cy: f32, span: f32) -> [(f32, f32); 9] {
    let step = span / 2.0;
    std::array::from_fn(|i| (cx - step + (i % 3) as f32 * step, cy - step + (i / 3) as f32 * step))
}

pub fn pattern_lock(c: &mut Canvas, t: &Theme, cx: f32, cy: f32, span: f32, path: &[usize]) {
    let m = &t.m;
    let dots = pattern_dots(cx, cy, span);
    let pos = |idx: usize| dots[idx];
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

/// The round keys a screen lock's code is typed on.
const PAD_KEY_DP: f32 = 76.0;
const PAD_GAP_X_DP: f32 = 30.0;
const PAD_GAP_Y_DP: f32 = 18.0;
const PAD_TOP_DP: f32 = 10.0;
/// Room under the last row for the gesture area of the screen's edge.
const PAD_BOTTOM_DP: f32 = 26.0;

/// What each key of the keypad types, read as rows of three.
const PAD_KEYS: [Cap; 12] = [
    Cap::Char('1'),
    Cap::Char('2'),
    Cap::Char('3'),
    Cap::Char('4'),
    Cap::Char('5'),
    Cap::Char('6'),
    Cap::Char('7'),
    Cap::Char('8'),
    Cap::Char('9'),
    Cap::Backspace,
    Cap::Char('0'),
    Cap::Enter,
];

/// How tall the keypad is on this theme's screen.
pub fn keypad_height(t: &Theme) -> f32 {
    t.m.dp(PAD_TOP_DP + 4.0 * PAD_KEY_DP + 3.0 * PAD_GAP_Y_DP + PAD_BOTTOM_DP)
}

/// The keypad a screen lock's code is typed on, its top at `top`: four rows
/// of three round keys, `pressed` drawn held down, and the last row erasing
/// and confirming. Returns where each key answers a touch.
pub fn keypad(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, pressed: Option<Cap>) -> Vec<(Rect, Cap)> {
    let m = &t.m;
    let (size, gx, gy) = (m.dp(PAD_KEY_DP), m.dp(PAD_GAP_X_DP), m.dp(PAD_GAP_Y_DP));
    let left = (m.w as f32 - (3.0 * size + 2.0 * gx)) / 2.0;
    let mut keys = Vec::with_capacity(PAD_KEYS.len());
    for (i, &cap) in PAD_KEYS.iter().enumerate() {
        let (row, col) = (i / 3, i % 3);
        let r = Rect::new(left + col as f32 * (size + gx), top + m.dp(PAD_TOP_DP) + row as f32 * (size + gy), size, size);
        keys.push((r, cap));
    }
    for &(r, cap) in &keys {
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        // Erasing and confirming carry their own words, and sit on the
        // page rather than on a key of their own.
        let bare = matches!(cap, Cap::Backspace | Cap::Enter);
        let base = match cap {
            Cap::Enter => t.pal.accent,
            _ if bare => t.pal.bg,
            _ => t.pal.card,
        };
        let fill = if pressed == Some(cap) { mix(base, t.pal.text, 0.16) } else { base };
        if !bare || cap == Cap::Enter {
            c.fill_circle(cx, cy, r.w / 2.0, fill);
        }
        match cap {
            Cap::Char(ch) => c.text_center(&f.heading, &ch.to_string(), cx, cy, m.dp(34.0), t.pal.text),
            Cap::Backspace => c.text_center(&f.body, "删除", cx, cy, m.row_title_px, t.pal.accent),
            _ => c.text_center(&f.heading, "解密", cx, cy, m.row_title_px, Rgba::rgb(255, 255, 255)),
        }
    }
    keys
}

/// The code typed so far, as a dot for each character. Nothing is drawn
/// for a code not started.
pub fn code_dots(c: &mut Canvas, t: &Theme, cx: f32, cy: f32, typed: usize) {
    let m = &t.m;
    let pitch = m.dp(26.0);
    let left = cx - (typed.saturating_sub(1) as f32) * pitch / 2.0;
    for i in 0..typed {
        c.fill_circle(left + i as f32 * pitch, cy, m.dp(7.0), t.pal.accent);
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

/// Centred lettering with its cap line at `cap_top`, as ink whose colour
/// comes from the field under it.
pub fn wordmark(t: &Theme, f: &Fonts, text: &str, cap_top: f32, px: f32) -> Sprite {
    let face = &f.display;
    let w = Canvas::new(0, 0).text_width(face, text, px);
    let x = (t.m.w as f32 - w) / 2.0;
    let pad = t.m.dp(4.0);
    let (left, top) = (x - pad, cap_top - px * 0.4);
    Sprite::capture(left, top, (w + pad * 2.0).ceil() as u32, (px * 1.6).ceil() as u32, |c, dx, dy| {
        c.text(face, text, x + dx, cap_top + face.cap(px) + dy, px, Rgba::rgb(0, 0, 0), None);
    })
}

/// The version line centred under the About wordmark.
pub fn version_line(t: &Theme, f: &Fonts, text: &str) -> Sprite {
    let m = &t.m;
    let px = m.dp(13.3);
    let w = Canvas::new(0, 0).text_width(&f.body, text, px);
    let base = m.dp(296.3) + f.body.cap(px);
    let (left, top) = ((m.w as f32 - w) / 2.0 - m.dp(2.0), base - px * 1.2);
    Sprite::capture(left, top, (w + m.dp(4.0)).ceil() as u32, (px * 1.6).ceil() as u32, |c, dx, dy| {
        c.text(&f.body, text, (m.w as f32 - w) / 2.0 + dx, base + dy, px, Rgba::rgb(0, 0, 0), None);
    })
}

/// A row of the About summary card: title on the left, value on the right,
/// and a chevron when the row opens a page.
pub struct Fact<'a> {
    pub title: &'a str,
    pub value: String,
    pub link: bool,
}

/// The About summary card: rows a little shorter than settings rows.
pub fn facts(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, rows: &[Fact], tone: &Tone) -> Block {
    let m = &t.m;
    let row_h = m.dp(51.0);
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let h = row_h * rows.len() as f32;
    c.fill_round_rect(card_x, top, right - card_x, h, m.card_radius, tone.card);
    let mut rects = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        rects.push(Rect::new(card_x, top + row_h * i as f32, right - card_x, row_h));
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
    Block { bottom: top + h, rows: rects }
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

/// Parameters-page card of title-over-value entries; values keep their own
/// line breaks and wrap to the card.
pub fn info_card(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, items: &[(&str, String)]) -> f32 {
    let m = &t.m;
    let (card_x, right) = (m.margin, m.w as f32 - m.margin);
    let x = card_x + m.inner;
    let width = right - m.inner - x;
    let (ink_top, _) = f.heading.ink(m.row_title_px);
    let (to_value, pitch) = (m.dp(20.2), m.dp(19.0));
    let blocks: Vec<Vec<String>> =
        items.iter().map(|(_, v)| v.lines().flat_map(|l| wrap(c, &f.body, l, m.sub_px, width)).collect()).collect();
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
/// ring and an accent caption. The rows are the light and the dark choice.
pub fn theme_picker(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32) -> Block {
    let m = &t.m;
    let gap = m.dp(4.0);
    let slot_w = (m.w as f32 - m.margin * 2.0 - gap) / 2.0;
    let (ring, inset) = (m.dp(3.0), m.dp(6.0));
    let tile = slot_w - inset * 2.0;
    let label_px = m.dp(15.9);
    let (ink_top, ink_bottom) = f.heading.ink(label_px);
    let caption_base = top + slot_w + m.dp(11.0) - ink_top;
    let bottom = caption_base + ink_bottom;
    let mut rects = Vec::new();
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
        rects.push(Rect::new(sx, top, slot_w, bottom - top));
    }
    Block { bottom, rows: rects }
}

/// Characters a terminal gives two cells: the CJK blocks and the
/// full-width forms.
fn wide(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x20000..=0x3FFFD
    )
}

/// Lines of a terminal on a card, in a grid of equal cells wrapped to the
/// card's inner width, `top` being where the first line's ink starts; with
/// `cursor`, a block after the last character. The monospaced face draws
/// what it has; the UI face draws the rest, such as Chinese, two cells wide.
/// Returns where the text ends.
pub fn mono_text(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, lines: &[String], cursor: bool) -> f32 {
    let m = &t.m;
    let px = m.dp(13.0);
    let cell = c.text_width(&f.mono, "0", px);
    let pitch = m.dp(19.0);
    let x0 = m.margin + m.inner;
    let cols = ((m.w as f32 - x0 * 2.0) / cell).floor() as usize;
    // Every line sits on the baseline the UI face would give it.
    let base = top - f.body.ink(px).0;
    let (mut row, mut col) = (0usize, 0usize);
    for (k, line) in lines.iter().enumerate() {
        if k > 0 {
            row += 1;
            col = 0;
        }
        for ch in line.chars() {
            let cells = if wide(ch) { 2 } else { 1 };
            if col + cells > cols {
                row += 1;
                col = 0;
            }
            let face = if f.mono.has(ch) { &f.mono } else { &f.body };
            let glyph = ch.to_string();
            let gw = c.text_width(face, &glyph, px);
            let x = x0 + col as f32 * cell + (cells as f32 * cell - gw) / 2.0;
            c.text(face, &glyph, x, base + row as f32 * pitch, px, t.pal.text, None);
            col += cells;
        }
    }
    if cursor {
        if col >= cols {
            row += 1;
            col = 0;
        }
        c.fill_rect(x0 + col as f32 * cell, top + row as f32 * pitch - m.dp(2.0), cell, m.dp(15.0), t.pal.accent);
    }
    top + (row + 1) as f32 * pitch
}

/// Which keys the on-screen keyboard shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Letters,
    /// Capitals, for the next letter typed.
    Shifted,
    /// Digits and the common symbols.
    Digits,
    /// The rest of the ASCII symbols.
    Symbols,
}

/// A key of the on-screen keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Char(char),
    Shift,
    Backspace,
    Space,
    Enter,
    /// Show another layer.
    Show(Layer),
}

/// The keys between the side keys of each row, per layer. Together the
/// digit and symbol layers hold every printable ASCII character.
fn rows(layer: Layer) -> [&'static str; 3] {
    match layer {
        Layer::Letters | Layer::Shifted => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        Layer::Digits => ["1234567890", "-/:;()$&@\"", ".,?!'"],
        Layer::Symbols => ["[]{}#%^*+=", "_\\|~<>`", ".,?!'"],
    }
}

const KEY_H_DP: f32 = 44.0;
const KEY_GAP_X_DP: f32 = 5.5;
const KEY_GAP_Y_DP: f32 = 11.0;
const KEYBOARD_PAD_X_DP: f32 = 3.5;
const KEYBOARD_TOP_DP: f32 = 9.0;
/// Room under the last row for the gesture area of the screen's edge.
const KEYBOARD_BOTTOM_DP: f32 = 26.0;

/// How tall the keyboard is on this theme's screen.
pub fn keyboard_height(t: &Theme) -> f32 {
    let m = &t.m;
    m.dp(KEYBOARD_TOP_DP + 4.0 * KEY_H_DP + 3.0 * KEY_GAP_Y_DP + KEYBOARD_BOTTOM_DP)
}

/// The on-screen keyboard across the bottom of the screen, its top at
/// `top`: four rows of keys in `layer`, `pressed` drawn held down, and the
/// return key saying what it does. Returns where each key answers a touch,
/// the gaps between keys shared out among their neighbours.
pub fn keyboard(c: &mut Canvas, t: &Theme, f: &Fonts, top: f32, layer: Layer, pressed: Option<Cap>, enter: &str) -> Vec<(Rect, Cap)> {
    let m = &t.m;
    let w = m.w as f32;
    let (panel, key, side, ink, shadow) = if t.dark {
        (Rgba::rgb(18, 18, 18), Rgba::rgb(58, 58, 58), Rgba::rgb(38, 38, 38), Rgba::rgb(237, 237, 237), Rgba::rgb(0, 0, 0))
    } else {
        (Rgba::rgb(224, 226, 231), Rgba::rgb(255, 255, 255), Rgba::rgb(193, 198, 207), Rgba::rgb(0, 0, 0), Rgba::rgb(138, 141, 147))
    };
    c.fill_rect(0.0, top, w, keyboard_height(t), panel);
    let (pad, gx, gy, kh) = (m.dp(KEYBOARD_PAD_X_DP), m.dp(KEY_GAP_X_DP), m.dp(KEY_GAP_Y_DP), m.dp(KEY_H_DP));
    let kw = (w - pad * 2.0 - gx * 9.0) / 10.0;
    let side_w = kw * 1.35;
    let row_top = |r: usize| top + m.dp(KEYBOARD_TOP_DP) + r as f32 * (kh + gy);
    let mut keys: Vec<(Rect, Cap)> = Vec::new();
    let [first, second, third] = rows(layer);
    // Rows of plain keys are centred: the letters' second row sits half a
    // key in, as on any phone keyboard.
    let centred = |n: usize, width: f32| (w - (n as f32 * width + (n as f32 - 1.0) * gx)) / 2.0;
    for (r, row) in [first, second].iter().enumerate() {
        let n = row.chars().count();
        let x0 = centred(n, kw);
        for (k, ch) in row.chars().enumerate() {
            keys.push((Rect::new(x0 + k as f32 * (kw + gx), row_top(r), kw, kh), Cap::Char(ch)));
        }
    }
    let (left, right) = (pad, w - pad - side_w);
    let shift = match layer {
        Layer::Letters | Layer::Shifted => Cap::Shift,
        Layer::Digits => Cap::Show(Layer::Symbols),
        Layer::Symbols => Cap::Show(Layer::Digits),
    };
    keys.push((Rect::new(left, row_top(2), side_w, kh), shift));
    let n = third.chars().count();
    let room = right - gx * 2.0 - (left + side_w + gx * 2.0);
    let mid_w = ((room - (n as f32 - 1.0) * gx) / n as f32).min(kw * 1.4);
    let x0 = centred(n, mid_w);
    for (k, ch) in third.chars().enumerate() {
        keys.push((Rect::new(x0 + k as f32 * (mid_w + gx), row_top(2), mid_w, kh), Cap::Char(ch)));
    }
    keys.push((Rect::new(right, row_top(2), side_w, kh), Cap::Backspace));
    let switch = match layer {
        Layer::Letters | Layer::Shifted => Cap::Show(Layer::Digits),
        Layer::Digits | Layer::Symbols => Cap::Show(Layer::Letters),
    };
    let enter_w = kw * 2.2;
    let space_x = left + side_w + gx + kw + gx;
    let space_w = w - pad - enter_w - gx - kw - gx - space_x;
    keys.push((Rect::new(left, row_top(3), side_w, kh), switch));
    keys.push((Rect::new(left + side_w + gx, row_top(3), kw, kh), Cap::Char(',')));
    keys.push((Rect::new(space_x, row_top(3), space_w, kh), Cap::Space));
    keys.push((Rect::new(space_x + space_w + gx, row_top(3), kw, kh), Cap::Char('.')));
    keys.push((Rect::new(w - pad - enter_w, row_top(3), enter_w, kh), Cap::Enter));

    let radius = m.dp(6.0);
    let glyph_px = m.dp(21.0);
    for &(r, cap) in &keys {
        let plain = matches!(cap, Cap::Char(_) | Cap::Space);
        let base = match cap {
            Cap::Enter => t.pal.accent,
            Cap::Shift if layer == Layer::Shifted => key,
            _ if plain => key,
            _ => side,
        };
        let fill = if pressed == Some(cap) { mix(base, ink, 0.16) } else { base };
        if !t.dark {
            c.fill_round_rect(r.x, r.y + m.dp(1.0), r.w, r.h, radius, shadow.with_alpha(110));
        }
        c.fill_round_rect(r.x, r.y, r.w, r.h, radius, fill);
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        match cap {
            Cap::Char(ch) => {
                let label = if layer == Layer::Shifted { ch.to_ascii_uppercase() } else { ch };
                c.text_center(&f.heading, &label.to_string(), cx, cy, glyph_px, ink);
            }
            Cap::Space => c.text_center(&f.body, "空格", cx, cy, m.sub_px, ink),
            Cap::Enter => c.text_center(&f.heading, enter, cx, cy, m.sub_px, Rgba::rgb(255, 255, 255)),
            Cap::Show(Layer::Digits) => c.text_center(&f.heading, "123", cx, cy, m.sub_px, ink),
            Cap::Show(Layer::Symbols) => c.text_center(&f.heading, "#+=", cx, cy, m.sub_px, ink),
            Cap::Show(_) => c.text_center(&f.heading, "abc", cx, cy, m.sub_px, ink),
            Cap::Shift => {
                // An upward arrow, solid while the next letter is a capital.
                let s = m.dp(9.0);
                let pts = [
                    (cx, cy - s),
                    (cx + s, cy),
                    (cx + s * 0.45, cy),
                    (cx + s * 0.45, cy + s * 0.9),
                    (cx - s * 0.45, cy + s * 0.9),
                    (cx - s * 0.45, cy),
                    (cx - s, cy),
                ];
                if layer == Layer::Shifted {
                    c.fill_convex(&pts[..3], ink);
                    c.fill_convex(&[pts[0], pts[6], pts[5]], ink);
                    c.fill_rect(cx - s * 0.45, cy - m.dp(0.5), s * 0.9, s * 0.9 + m.dp(0.5), ink);
                } else {
                    for k in 0..pts.len() {
                        let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                        c.line(a.0, a.1, b.0, b.1, m.dp(1.6), ink);
                    }
                }
            }
            Cap::Backspace => {
                // A key cap pointing left, with a cross in it.
                let (hw, hh) = (m.dp(11.0), m.dp(8.0));
                let pts = [(cx - hw, cy), (cx - hw * 0.45, cy - hh), (cx + hw, cy - hh), (cx + hw, cy + hh), (cx - hw * 0.45, cy + hh)];
                for k in 0..pts.len() {
                    let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                    c.line(a.0, a.1, b.0, b.1, m.dp(1.6), ink);
                }
                let (xc, s) = (cx + hw * 0.22, m.dp(3.6));
                c.line(xc - s, cy - s, xc + s, cy + s, m.dp(1.6), ink);
                c.line(xc - s, cy + s, xc + s, cy - s, m.dp(1.6), ink);
            }
        }
    }
    // A touch between keys belongs to the nearest one.
    keys.into_iter().map(|(r, cap)| (Rect::new(r.x - gx / 2.0, r.y - gy / 2.0, r.w + gx, r.h + gy), cap)).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The battery's pixel at (`x`, `y`) on screen.
    fn pixel(art: &(Canvas, i32, i32), x: f32, y: f32) -> [u8; 4] {
        let (c, ox, oy) = art;
        let i = ((y as i32 - oy) as usize * c.w as usize + (x as i32 - ox) as usize) * 4;
        c.data[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn the_battery_fills_its_level_in_the_colour_of_its_state() {
        let f = crate::font::bundled();
        let t = Theme::light(1200, 2608, 3.0);
        let ink = t.pal.battery;
        let (right, cy) = (1000.0, 77.0);
        // 69 px wide from 930.85; the row 15 px above the middle is clear
        // of the digits.
        let rgba = |c: Rgba| [c.r, c.g, c.b, c.a];
        let half = battery(&t, &f, 50, false, right, cy);
        assert_eq!(pixel(&half, 948.0, cy - 15.0), rgba(ink.level), "a quarter in is filled");
        assert_eq!(pixel(&half, 983.0, cy - 15.0), rgba(ink.empty), "three quarters in is empty");
        let charging = battery(&t, &f, 50, true, right, cy);
        assert_eq!(pixel(&charging, 948.0, cy - 15.0), rgba(ink.charging), "the cable in fills green");
        let low = battery(&t, &f, 15, false, right, cy);
        assert_eq!(pixel(&low, 935.0, cy), rgba(ink.low), "low and unplugged fills red");
        assert_eq!(pixel(&low, 948.0, cy - 15.0), rgba(ink.empty), "past 15% it is empty");
        let low_charging = battery(&t, &f, 15, true, right, cy);
        assert_eq!(pixel(&low_charging, 935.0, cy), rgba(ink.charging), "low but charging stays green");
        let full = battery(&t, &f, 100, false, right, cy);
        assert_eq!(pixel(&full, right - 1.5, cy), rgba(ink.level), "a full battery fills its cap");
    }

    #[test]
    fn digits_are_cut_out_of_a_plain_fill_and_white_over_a_coloured_one() {
        let f = crate::font::bundled();
        let t = Theme::dark(1200, 2608, 3.0);
        let (right, cy) = (1000.0, 77.0);
        // Inside the body, clear of its rounded ends and edges.
        let holes = |art: &(Canvas, i32, i32)| -> usize {
            let mut n = 0;
            for y in (cy - 15.0) as i32..(cy + 15.0) as i32 {
                for x in 945..985 {
                    n += usize::from(pixel(art, x as f32, y as f32)[3] < 40);
                }
            }
            n
        };
        assert!(holes(&battery(&t, &f, 100, false, right, cy)) > 60, "the digits show the page through");
        assert_eq!(holes(&battery(&t, &f, 100, true, right, cy)), 0, "white digits over green");
        assert_eq!(holes(&battery(&t, &f, 15, false, right, cy)), 0, "white digits over red");
    }

    #[test]
    fn wrapping_keeps_brackets_punctuation_and_words_with_their_text() {
        let f = crate::font::bundled();
        let c = Canvas::new(1, 1);
        let px = 40.0;
        let texts = [
            "Skyward 是自由软件：你可以按照自由软件基金会发布的 GNU 通用公共许可证第 3 版，或者（由你选择）任何更新的版本，重新发布和修改它。",
            "You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.",
            "Skyward 的界面文字使用小米的 MiSans 字体，按《MiSans 字体知识产权许可协议》免费使用。",
            "boot、init_boot、vendor_boot、dtbo、vbmeta 和 recovery",
            "Copyright 2026 The Kirk Mono Project Authors (https://github.com/kirklin/kirk-mono)",
        ];
        let latin = |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':');
        for text in texts {
            for ems in 6..30 {
                let max_w = ems as f32 * px;
                let lines = wrap(&c, &f.body, text, px, max_w);
                for l in &lines {
                    assert!(!(l.chars().count() > 1 && l.ends_with(|ch| NO_LINE_END.contains(ch))), "{l:?} ends on an opening bracket: {lines:?}");
                    assert!(!l.starts_with(|ch| NO_LINE_START.contains(ch)), "{l:?} starts with closing punctuation: {lines:?}");
                }
                let squeezed = |s: &str| s.chars().filter(|ch| *ch != ' ').collect::<String>();
                assert_eq!(squeezed(&lines.concat()), squeezed(text), "nothing lost at {ems} em");
                for word in text.split(|ch: char| !latin(ch)).filter(|w| !w.is_empty()) {
                    if c.text_width(&f.body, word, px) <= max_w {
                        assert!(lines.iter().any(|l| l.contains(word)), "{word:?} broken at {ems} em: {lines:?}");
                    }
                }
            }
        }
    }
}
