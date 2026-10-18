use crate::app::{Action, FileKind, FileRow, Hit, Pinned, Rows};
use crate::canvas::Canvas;
use crate::color::{mix, Rgba};
use crate::fmt;
use crate::font::{Face, Fonts};
use crate::geom::Rect;
use crate::theme::Theme;
use crate::widgets;
use std::path::{Component, Path};

// The file manager's list as the stock file manager draws a folder: rows
// laid straight on the page, under a path of capsules. Sizes are dp,
// measured on a Xiaomi 17 Ultra.

/// Where the path strip starts on an unscrolled page; scrolled up to the
/// bar, it stays right under it.
pub const STRIP_TOP_DP: f32 = 164.0;
const STRIP_DP: f32 = 51.0;
const PILL_TOP_DP: f32 = 13.0;
const PILL_DP: f32 = 33.0;
const PILL_PAD_DP: f32 = 14.0;
const PILL_TEXT_DP: f32 = 10.9;
/// Space either side of the arrow between two capsules.
const ARROW_GAP_DP: f32 = 6.0;
const ARROW_DP: f32 = 14.0;
/// Where the first row starts on an unscrolled page.
pub const ROWS_TOP_DP: f32 = 223.7;
const ROW_DP: f32 = 72.0;
/// What a second line of name adds to a row.
const NAME_LINE_DP: f32 = 22.33;
const SIDE_DP: f32 = 28.0;
const ICON_DP: f32 = 40.0;
const TEXT_X_DP: f32 = 84.0;
/// Room the text leaves at the right, for the chevron's column.
const TEXT_RIGHT_DP: f32 = 65.7;
const NAME_DP: f32 = 16.9;
/// The top of a name's ink in its row.
const NAME_TOP_DP: f32 = 19.0;
const LINE_DP: f32 = 11.7;
/// The baseline of the line under a one-line name.
const LINE_BASE_DP: f32 = 53.33;
/// Where the message in place of an empty or unreadable list sits, below
/// where the rows would start.
const NOTE_DP: f32 = 140.0;
const NOTE_TEXT_DP: f32 = 17.0;

/// The list's colours: the page, the text, and the path's capsules, the
/// folder shown in amber.
struct Palette {
    page: Rgba,
    name: Rgba,
    line: Rgba,
    note: Rgba,
    chevron: Rgba,
    pill: Rgba,
    pill_text: Rgba,
    here: Rgba,
    here_text: Rgba,
    arrow: Rgba,
}

fn palette(t: &Theme) -> Palette {
    if t.dark {
        Palette {
            page: Rgba::rgb(0, 0, 0),
            name: Rgba::rgb(242, 242, 242),
            line: Rgba::rgb(128, 128, 128),
            note: Rgba::rgb(128, 128, 128),
            chevron: Rgba::rgb(128, 128, 128),
            pill: Rgba::rgb(38, 38, 38),
            pill_text: Rgba::rgb(136, 140, 154),
            here: Rgba::rgb(51, 37, 0),
            here_text: Rgba::rgb(204, 150, 0),
            arrow: Rgba::rgb(51, 51, 51),
        }
    } else {
        Palette {
            page: Rgba::rgb(255, 255, 255),
            name: Rgba::rgb(0, 0, 0),
            line: Rgba::rgb(153, 153, 153),
            note: Rgba::rgb(102, 102, 102),
            chevron: Rgba::rgb(153, 153, 153),
            pill: Rgba::rgb(243, 244, 247),
            pill_text: Rgba::rgb(136, 140, 154),
            here: Rgba::rgb(255, 245, 217),
            here_text: Rgba::rgb(255, 178, 29),
            arrow: Rgba::rgb(217, 217, 217),
        }
    }
}

/// The colour of the page under the list.
pub fn page(t: &Theme) -> Rgba {
    palette(t).page
}

fn name_width(t: &Theme) -> f32 {
    t.m.w as f32 - t.m.dp(TEXT_X_DP) - t.m.dp(TEXT_RIGHT_DP)
}

/// A name as the stock file manager sets it in `w` pixels: on one line if
/// it fits, on two if they hold it; longer, the first line full and the
/// second keeping the extension, with the middle taken out.
pub fn name_lines(face: &Face, px: f32, name: &str, w: f32) -> Vec<String> {
    let c = Canvas::new(0, 0);
    let width = |s: &str| c.text_width(face, s, px);
    if width(name) <= w {
        return vec![name.to_string()];
    }
    let lines = widgets::wrap(&c, face, name, px, w);
    if lines.len() <= 2 {
        return lines;
    }
    let cut = name.char_indices().map(|(at, ch)| at + ch.len_utf8()).take_while(|&end| width(&name[..end]) <= w).last().unwrap_or(0);
    let (first, rest) = name.split_at(cut);
    let second = match rest.rfind('.') {
        Some(dot) if width(&rest[dot..]) < w => {
            let ext = &rest[dot..];
            format!("{}{ext}", middle(&c, face, px, &rest[..dot], w - width(ext)))
        }
        _ => middle(&c, face, px, rest, w),
    };
    vec![first.to_string(), second]
}

/// `text` cut down to `w` pixels by taking characters out of its middle:
/// the end keeps up to half the room, the start the rest.
fn middle(c: &Canvas, face: &Face, px: f32, text: &str, w: f32) -> String {
    let width = |s: &[char]| c.text_width(face, &s.iter().collect::<String>(), px);
    let chars: Vec<char> = text.chars().collect();
    if width(&chars) <= w {
        return text.to_string();
    }
    let room = w - c.text_width(face, "…", px);
    let mut right = chars.len();
    while right > 0 && width(&chars[right - 1..]) <= room / 2.0 {
        right -= 1;
    }
    let left_room = room - width(&chars[right..]);
    let mut left = 0;
    while left < right && width(&chars[..=left]) <= left_room {
        left += 1;
    }
    format!("{}…{}", chars[..left].iter().collect::<String>(), chars[right..].iter().collect::<String>())
}

/// The list's rows for `entries`, the first starting where an unscrolled
/// page puts it; a row grows by a line for a name on two.
pub fn rows(t: &Theme, f: &Fonts, entries: &[FileRow]) -> Rows {
    let m = &t.m;
    let w = name_width(t);
    let mut y = m.dp(ROWS_TOP_DP).round();
    let mut edges = Vec::with_capacity(entries.len() + 1);
    edges.push(y);
    for e in entries {
        let lines = name_lines(&f.heading, m.dp(NAME_DP), &e.name, w).len();
        y += (m.dp(ROW_DP) + m.dp(NAME_LINE_DP) * (lines - 1) as f32).round();
        edges.push(y);
    }
    Rows { edges, action: Action::OpenEntry }
}

/// Entry `e`'s row over the whole of `c`, sized for it: its icon, its name
/// on up to two lines, the line under it with dates told against `now`,
/// and a chevron for what opens as a folder.
pub fn row(c: &mut Canvas, t: &Theme, f: &Fonts, e: &FileRow, now: i64) {
    let m = &t.m;
    let p = palette(t);
    c.clear(p.page);
    let cy = c.h as f32 / 2.0;
    let size = m.dp(ICON_DP);
    let folder = matches!(e.kind, FileKind::Folder { .. } | FileKind::Link { folder: true, .. });
    let (ix, iy) = (m.dp(SIDE_DP), cy - size / 2.0);
    if folder {
        folder_icon(c, ix, iy, size);
    } else {
        file_icon(c, ix, iy, size, p.page);
    }
    let (x, w) = (m.dp(TEXT_X_DP), name_width(t));
    let lines = name_lines(&f.heading, m.dp(NAME_DP), &e.name, w);
    for (k, line) in lines.iter().enumerate() {
        c.text_top(&f.heading, line, x, m.dp(NAME_TOP_DP + NAME_LINE_DP * k as f32), m.dp(NAME_DP), p.name, None);
    }
    let base = m.dp(LINE_BASE_DP + NAME_LINE_DP * (lines.len() - 1) as f32);
    c.text(&f.value, &fmt::file_line(e, now), x, base, m.dp(LINE_DP), p.line, Some(w));
    if folder {
        chevron(c, t, cy, p.chevron);
    }
}

/// A folder 40 units square scaled to `size`: an amber back with its tab,
/// a sheet of paper, and a yellow front shading darker towards its foot,
/// in the colours the stock list shows its folders in.
fn folder_icon(c: &mut Canvas, x: f32, y: f32, size: f32) {
    let u = size / 40.0;
    let at = |v: f32, w: f32| (x + v * u, y + w * u);
    let (tab, under) = (Rgba::rgb(243, 175, 64), Rgba::rgb(238, 164, 59));
    let (b0, b1) = (y + 6.34 * u, y + 12.92 * u);
    let back = |_: f32, py: f32| mix(tab, under, (py - b0) / (b1 - b0));
    c.fill_round_rect_paint(x + 3.0 * u, b0, 12.6 * u, 6.0 * u, 1.9 * u, &back);
    c.fill_convex(&[at(13.0, 6.34), at(14.6, 6.34), at(17.9, 8.8), at(13.0, 8.8)], tab);
    c.fill_round_rect_paint(x + 3.0 * u, y + 8.8 * u, 34.0 * u, 24.86 * u, 4.7 * u, &back);
    c.fill_round_rect(x + 5.4 * u, y + 11.1 * u, 29.4 * u, 20.6 * u, 1.2 * u, Rgba::rgb(255, 255, 255));
    let (top, foot) = (Rgba::rgb(248, 215, 83), Rgba::rgb(241, 176, 67));
    let (y0, y1) = (y + 12.92 * u, y + 33.66 * u);
    let front = |_: f32, py: f32| mix(top, foot, (py - y0) / (y1 - y0));
    c.fill_round_rect_paint(x + 3.0 * u, y0, 34.0 * u, y1 - y0, 4.7 * u, &front);
    c.fill_round_rect_paint(x + 3.0 * u, y0, 34.0 * u, 10.0 * u, 1.45 * u, &front);
}

/// A plain document 40 units square scaled to `size`, in slate with its top
/// right corner folded over; `page` is the colour around it.
fn file_icon(c: &mut Canvas, x: f32, y: f32, size: f32, page: Rgba) {
    let u = size / 40.0;
    let at = |v: f32, w: f32| (x + v * u, y + w * u);
    let sheet = Rgba::rgb(140, 157, 176);
    c.fill_round_rect(x + 4.0 * u, y, 32.0 * u, 40.0 * u, 7.0 * u, sheet);
    // The corner cut away, and the flap folded down over the sheet.
    c.fill_convex(&[at(25.0, -0.5), at(36.5, -0.5), at(36.5, 11.0)], page);
    c.fill_convex(&[at(25.0, 0.0), at(25.0, 11.0), at(36.0, 11.0)], mix(sheet, Rgba::rgb(255, 255, 255), 0.3));
}

/// The list's chevron, 10 by 16 dp at the right margin.
fn chevron(c: &mut Canvas, t: &Theme, cy: f32, color: Rgba) {
    let m = &t.m;
    let (x0, y0) = (m.w as f32 - m.dp(SIDE_DP) - m.dp(10.0), cy - m.dp(8.0));
    let at = |v: f32, w: f32| (x0 + m.dp(v), y0 + m.dp(w));
    let (a, b, d) = (at(2.23, 1.94), at(8.28, 8.0), at(2.23, 14.06));
    let sw = m.dp(1.43);
    c.line(a.0, a.1, b.0, b.1, sw, color);
    c.line(b.0, b.1, d.0, d.1, sw, color);
}

/// The small arrow between two capsules, its 14 dp box starting at `x`.
fn arrow(c: &mut Canvas, t: &Theme, x: f32, cy: f32, color: Rgba) {
    let m = &t.m;
    let at = |v: f32, w: f32| (x + m.dp(v), cy + m.dp(w - 7.0));
    let (a, b, d) = (at(4.4, 2.11), at(9.2, 7.0), at(4.4, 11.89));
    let sw = m.dp(1.4);
    c.line(a.0, a.1, b.0, b.1, sw, color);
    c.line(b.0, b.1, d.0, d.1, sw, color);
}

/// The path over the list as capsules, the root first and the folder shown
/// last in amber; each earlier one goes up to its folder. A path longer
/// than the strip shows its end, as the stock strip does scrolled to it.
pub fn strip(t: &Theme, f: &Fonts, folder: &str) -> Pinned {
    let m = &t.m;
    let p = palette(t);
    let mut c = Canvas::new(m.w, m.dp(STRIP_DP).round() as u32);
    c.clear(p.page);
    let names: Vec<String> = Path::new(folder)
        .components()
        .map(|part| match part {
            Component::RootDir => "根目录".to_string(),
            other => other.as_os_str().to_string_lossy().into_owned(),
        })
        .collect();
    let px = m.dp(PILL_TEXT_DP);
    let widths: Vec<f32> = names.iter().map(|n| c.text_width(&f.value, n, px) + m.dp(PILL_PAD_DP) * 2.0).collect();
    let step = m.dp(ARROW_GAP_DP) * 2.0 + m.dp(ARROW_DP);
    let total = widths.iter().sum::<f32>() + step * (names.len() - 1) as f32;
    let mut x = m.dp(SIDE_DP).min(m.w as f32 - m.dp(SIDE_DP) - total);
    let (top, h) = (m.dp(PILL_TOP_DP), m.dp(PILL_DP));
    let mut hits = Vec::new();
    for (k, (name, &w)) in names.iter().zip(&widths).enumerate() {
        let here = k + 1 == names.len();
        let (fill, ink) = if here { (p.here, p.here_text) } else { (p.pill, p.pill_text) };
        c.fill_round_rect(x, top, w, h, h / 2.0, fill);
        c.text_center(&f.value, name, x + w / 2.0, top + h / 2.0, px, ink);
        if !here {
            hits.push(Hit { rect: Rect::new(x, top, w, h), action: Action::OpenAncestor(k) });
            arrow(&mut c, t, x + w + m.dp(ARROW_GAP_DP), top + h / 2.0, p.arrow);
        }
        x += w + step;
    }
    Pinned { canvas: c, top: m.dp(STRIP_TOP_DP), hits }
}

/// What the page says in place of the list, centred below the path, with
/// the reason under it when there is one.
pub fn note(c: &mut Canvas, t: &Theme, f: &Fonts, title: &str, why: Option<&str>) {
    let m = &t.m;
    let p = palette(t);
    let cx = m.w as f32 / 2.0;
    let cy = m.dp(ROWS_TOP_DP + NOTE_DP);
    c.text_center(&f.body, title, cx, cy, m.dp(NOTE_TEXT_DP), p.note);
    let Some(why) = why else { return };
    let lines = widgets::wrap(c, &f.body, why, m.sub_px, m.w as f32 - m.dp(SIDE_DP) * 2.0);
    for (k, line) in lines.iter().enumerate() {
        c.text_center(&f.body, line, cx, cy + m.dp(32.0) + m.sub_pitch * k as f32, m.sub_px, p.line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_keeps_its_start_and_its_extension() {
        let f = crate::font::bundled();
        let (px, w) = (50.0, 750.0);
        let short = name_lines(&f.heading, px, "init.rc", w);
        assert_eq!(short, ["init.rc"]);
        let two = name_lines(&f.heading, px, "1d9bf122-130e-4db7-ae55-460c84819f0e.pdf", w);
        assert_eq!(two.len(), 2, "{two:?}");
        assert_eq!(two.concat(), "1d9bf122-130e-4db7-ae55-460c84819f0e.pdf", "nothing is left out of two lines");
        let long = "2021一带一路暨金砖大赛之物联网技术及其在智慧城市与工业互联网领域的应用创新赛表彰文件.pdf";
        let lines = name_lines(&f.heading, px, long, w);
        let c = Canvas::new(0, 0);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(long.starts_with(&lines[0]), "the first line is the start: {lines:?}");
        assert!(lines[1].contains('…') && lines[1].ends_with("文件.pdf"), "the middle goes, the end stays: {lines:?}");
        for line in &lines {
            assert!(c.text_width(&f.heading, line, px) <= w, "{line:?} fits");
        }
    }

    #[test]
    fn the_path_goes_up_by_its_capsules_and_shows_its_end_when_long() {
        let f = crate::font::bundled();
        let t = Theme::light(1440, 3200, 3.5);
        let path = strip(&t, &f, "/system/bin");
        let ups: Vec<Action> = path.hits.iter().map(|h| h.action).collect();
        assert_eq!(ups, [Action::OpenAncestor(0), Action::OpenAncestor(1)], "the root and system go up; bin is where we are");
        let deep = strip(&t, &f, "/data/media/0/Android/data/com.example.something.long/files/cache/images");
        let last = deep.hits.last().expect("the capsules before the folder shown");
        assert!(last.rect.x + last.rect.w < t.m.w as f32, "the end shows");
        assert!(deep.hits[0].rect.x < 0.0, "the start runs off the left edge");
    }
}
