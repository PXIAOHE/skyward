use crate::color::Rgba;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Rgba,
    pub card: Rgba,
    pub card_pressed: Rgba,
    pub text: Rgba,
    pub secondary: Rgba,
    pub tertiary: Rgba,
    pub separator: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub switch_off: Rgba,
    pub track: Rgba,
    pub nav: Rgba,
    pub nav_border: Rgba,
    pub shadow: Rgba,
    pub danger: Rgba,
}

#[derive(Clone, Copy)]
pub struct Metrics {
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    pub side: f32,
    pub card_radius: f32,
    pub row_h: f32,
    pub group_gap: f32,
    pub status_h: f32,
    pub big_title: f32,
    pub page_title: f32,
    pub row_title: f32,
    pub sub: f32,
    pub status_px: f32,
    pub chip: f32,
    pub chip_radius: f32,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub pal: Palette,
    pub m: Metrics,
    pub dark: bool,
}

impl Theme {
    pub fn light(w: u32, h: u32) -> Self {
        Theme { pal: LIGHT, m: metrics(w, h), dark: false }
    }

    pub fn dark(w: u32, h: u32) -> Self {
        Theme { pal: DARK, m: metrics(w, h), dark: true }
    }
}

fn metrics(w: u32, h: u32) -> Metrics {
    // The phone baseline is a 1200px short edge; everything scales from there.
    let scale = w as f32 / 1200.0;
    let s = |v: f32| v * scale;
    Metrics {
        w,
        h,
        scale,
        side: s(32.0),
        card_radius: s(34.0),
        row_h: s(132.0),
        group_gap: s(44.0),
        status_h: s(96.0),
        big_title: s(104.0),
        page_title: s(58.0),
        row_title: s(46.0),
        sub: s(34.0),
        status_px: s(40.0),
        chip: s(78.0),
        chip_radius: s(22.0),
    }
}

const LIGHT: Palette = Palette {
    bg: Rgba::rgb(244, 244, 247),
    card: Rgba::rgb(255, 255, 255),
    card_pressed: Rgba::rgb(236, 236, 239),
    text: Rgba::rgb(23, 23, 25),
    secondary: Rgba::rgb(122, 122, 128),
    tertiary: Rgba::rgb(176, 176, 182),
    separator: Rgba::rgb(237, 237, 240),
    accent: Rgba::rgb(60, 124, 255),
    on_accent: Rgba::rgb(255, 255, 255),
    switch_off: Rgba::rgb(215, 215, 220),
    track: Rgba::rgb(230, 230, 234),
    nav: Rgba::rgba(255, 255, 255, 230),
    nav_border: Rgba::rgba(0, 0, 0, 18),
    shadow: Rgba::rgba(0, 0, 0, 42),
    danger: Rgba::rgb(240, 74, 74),
};

const DARK: Palette = Palette {
    bg: Rgba::rgb(11, 11, 12),
    card: Rgba::rgb(26, 26, 28),
    card_pressed: Rgba::rgb(38, 38, 41),
    text: Rgba::rgb(242, 242, 244),
    secondary: Rgba::rgb(154, 154, 160),
    tertiary: Rgba::rgb(106, 106, 112),
    separator: Rgba::rgb(40, 40, 43),
    accent: Rgba::rgb(76, 134, 255),
    on_accent: Rgba::rgb(255, 255, 255),
    switch_off: Rgba::rgb(58, 58, 62),
    track: Rgba::rgb(42, 42, 46),
    nav: Rgba::rgba(30, 30, 33, 235),
    nav_border: Rgba::rgba(255, 255, 255, 20),
    shadow: Rgba::rgba(0, 0, 0, 110),
    danger: Rgba::rgb(255, 92, 92),
};
