use crate::color::Rgba;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Rgba,
    pub card: Rgba,
    pub text: Rgba,
    pub subtitle: Rgba,
    pub value: Rgba,
    pub section: Rgba,
    pub chevron: Rgba,
    pub accent: Rgba,
    pub track: Rgba,
    pub switch_off: Rgba,
    pub search: Rgba,
    pub search_icon: Rgba,
    pub placeholder: Rgba,
    pub badge: Rgba,
    pub avatar: Rgba,
    pub avatar_glyph: Rgba,
    pub status: Rgba,
    pub battery: Rgba,
    pub battery_cap: Rgba,
    pub mono_icon: Rgba,
    pub danger: Rgba,
    pub danger_soft: Rgba,
    /// Titles and values on the parameters page and captions inside cards, a
    /// step softer than `text`.
    pub info: Rgba,
    /// Line icons on the parameters page.
    pub outline: Rgba,
    /// Captions under the About page's spec values.
    pub spec_label: Rgba,
}

/// The About page backdrop, the same in both themes: deep blue with a violet
/// edge, and translucent cards on top.
#[derive(Clone, Copy)]
pub struct Hero {
    /// Backdrop at the top, middle and bottom of the first screen.
    pub top: Rgba,
    pub mid: Rgba,
    pub bottom: Rgba,
    /// Soft colour fields: centre as fractions of the screen width and
    /// height, radius as a fraction of the width.
    pub blobs: [(f32, f32, f32, Rgba); 6],
    /// Wordmark gradient, left, centre and right.
    pub ink: [Rgba; 3],
    pub caption: Rgba,
    /// Cards are white at low opacity over the backdrop.
    pub card: Rgba,
    pub title: Rgba,
    pub value: Rgba,
    pub chevron: Rgba,
    pub status: Rgba,
}

/// Layout metrics measured on a Xiaomi 17 Ultra's stock settings, in dp,
/// converted at the panel's density (1200 px wide at 3.0 px/dp).
#[derive(Clone, Copy)]
pub struct Metrics {
    pub w: u32,
    pub h: u32,
    pub px_per_dp: f32,
    pub margin: f32,
    pub card_radius: f32,
    pub card_gap: f32,
    pub section_block: f32,
    pub section_label_top: f32,
    pub row_h: f32,
    pub row_pad_v: f32,
    pub inner: f32,
    pub icon: f32,
    pub icon_radius: f32,
    pub icon_gap: f32,
    pub title_px: f32,
    pub title_x: f32,
    pub title_top: f32,
    /// Back chevron: tip x, centre y, width, height and stroke.
    pub back_x: f32,
    pub back_cy: f32,
    pub back_w: f32,
    pub back_h: f32,
    pub back_stroke: f32,
    pub stroke: f32,
    pub content_top: f32,
    pub search_h: f32,
    pub search_text_inset: f32,
    pub after_search: f32,
    pub row_title_px: f32,
    pub sub_px: f32,
    pub sub_gap: f32,
    pub sub_pitch: f32,
    pub label_px: f32,
    pub account_title_px: f32,
    pub account_row_h: f32,
    pub avatar: f32,
    pub avatar_inset: f32,
    pub badge_px: f32,
    pub badge_w: f32,
    pub badge_h: f32,
    pub chevron_w: f32,
    pub chevron_h: f32,
    pub chevron_inset: f32,
    pub chevron_stroke: f32,
    pub value_gap: f32,
    pub switch_w: f32,
    pub switch_h: f32,
    pub knob: f32,
    pub slider_h: f32,
    pub slider_knob: f32,
    pub status_x: f32,
    pub status_right: f32,
    pub status_cy: f32,
    pub status_px: f32,
}

impl Metrics {
    pub fn dp(&self, v: f32) -> f32 {
        v * self.px_per_dp
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub pal: Palette,
    pub hero: Hero,
    pub m: Metrics,
    pub dark: bool,
}

impl Theme {
    pub fn light(w: u32, h: u32) -> Self {
        Theme { pal: LIGHT, hero: HERO, m: metrics(w, h), dark: false }
    }

    pub fn dark(w: u32, h: u32) -> Self {
        Theme { pal: DARK, hero: HERO, m: metrics(w, h), dark: true }
    }
}

fn metrics(w: u32, h: u32) -> Metrics {
    let px_per_dp = 3.0 * w as f32 / 1200.0;
    let dp = |v: f32| v * px_per_dp;
    Metrics {
        w,
        h,
        px_per_dp,
        margin: dp(12.0),
        card_radius: dp(20.0),
        card_gap: dp(12.0),
        section_block: dp(45.67),
        section_label_top: dp(23.0),
        row_h: dp(55.67),
        row_pad_v: dp(17.0),
        inner: dp(16.0),
        icon: dp(28.0),
        icon_radius: dp(8.0),
        icon_gap: dp(20.67),
        title_px: dp(32.0),
        title_x: dp(28.6),
        title_top: dp(117.7),
        back_x: dp(26.3),
        back_cy: dp(75.7),
        back_w: dp(11.7),
        back_h: dp(21.0),
        back_stroke: dp(1.8),
        stroke: dp(2.0),
        content_top: dp(167.7),
        search_h: dp(43.7),
        search_text_inset: dp(43.7),
        after_search: dp(12.7),
        row_title_px: dp(16.9),
        sub_px: dp(14.0),
        sub_gap: dp(6.7),
        sub_pitch: dp(18.7),
        label_px: dp(13.2),
        account_title_px: dp(17.1),
        account_row_h: dp(69.7),
        avatar: dp(34.0),
        avatar_inset: dp(12.8),
        badge_px: dp(14.5),
        badge_w: dp(72.9),
        badge_h: dp(28.9),
        chevron_w: dp(7.7),
        chevron_h: dp(13.3),
        chevron_inset: dp(17.3),
        chevron_stroke: dp(1.15),
        value_gap: dp(11.3),
        switch_w: dp(48.7),
        switch_h: dp(28.0),
        knob: dp(19.3),
        slider_h: dp(27.5),
        slider_knob: dp(18.7),
        status_x: dp(37.0),
        status_right: dp(40.0),
        status_cy: dp(25.7),
        status_px: dp(14.1),
    }
}

// Sampled from the stock Settings on a Xiaomi 17 Ultra.
const LIGHT: Palette = Palette {
    bg: Rgba::rgb(243, 243, 243),
    card: Rgba::rgb(255, 255, 255),
    text: Rgba::rgb(0, 0, 0),
    subtitle: Rgba::rgb(102, 102, 102),
    value: Rgba::rgb(153, 153, 153),
    section: Rgba::rgb(143, 156, 174),
    chevron: Rgba::rgb(178, 178, 178),
    accent: Rgba::rgb(74, 128, 247),
    track: Rgba::rgb(240, 240, 240),
    switch_off: Rgba::rgb(229, 229, 229),
    search: Rgba::rgb(229, 229, 229),
    search_icon: Rgba::rgb(161, 161, 161),
    placeholder: Rgba::rgb(160, 160, 160),
    badge: Rgba::rgb(235, 242, 254),
    avatar: Rgba::rgb(221, 223, 226),
    avatar_glyph: Rgba::rgb(255, 255, 255),
    status: Rgba::rgb(61, 61, 61),
    battery: Rgba::rgb(29, 205, 58),
    battery_cap: Rgba::rgb(171, 171, 171),
    mono_icon: Rgba::rgb(179, 179, 179),
    danger: Rgba::rgb(230, 66, 60),
    danger_soft: Rgba::rgb(253, 236, 235),
    info: Rgba::rgb(51, 51, 51),
    outline: Rgba::rgb(102, 102, 102),
    spec_label: Rgba::rgb(127, 127, 127),
};

const DARK: Palette = Palette {
    bg: Rgba::rgb(0, 0, 0),
    card: Rgba::rgb(28, 28, 28),
    text: Rgba::rgb(237, 237, 237),
    subtitle: Rgba::rgb(140, 140, 140),
    value: Rgba::rgb(128, 128, 128),
    section: Rgba::rgb(107, 113, 134),
    chevron: Rgba::rgb(77, 77, 77),
    accent: Rgba::rgb(84, 136, 248),
    track: Rgba::rgb(46, 46, 46),
    switch_off: Rgba::rgb(58, 58, 58),
    search: Rgba::rgb(28, 28, 28),
    search_icon: Rgba::rgb(110, 110, 110),
    placeholder: Rgba::rgb(110, 110, 110),
    badge: Rgba::rgb(29, 40, 64),
    avatar: Rgba::rgb(51, 51, 51),
    avatar_glyph: Rgba::rgb(140, 140, 140),
    status: Rgba::rgb(224, 224, 224),
    battery: Rgba::rgb(29, 205, 58),
    battery_cap: Rgba::rgb(110, 110, 110),
    mono_icon: Rgba::rgb(110, 110, 110),
    danger: Rgba::rgb(255, 92, 84),
    danger_soft: Rgba::rgb(52, 24, 24),
    info: Rgba::rgb(214, 214, 214),
    outline: Rgba::rgb(140, 140, 140),
    spec_label: Rgba::rgb(128, 128, 128),
};

// Sampled from the stock「我的设备」page at rest.
const HERO: Hero = Hero {
    top: Rgba::rgb(16, 25, 60),
    mid: Rgba::rgb(20, 33, 90),
    bottom: Rgba::rgb(10, 20, 66),
    blobs: [
        (0.25, 0.26, 0.36, Rgba::rgb(34, 50, 112)),
        (1.05, 0.22, 0.38, Rgba::rgb(66, 30, 82)),
        (1.05, -0.02, 0.25, Rgba::rgb(38, 16, 46)),
        (-0.10, 1.05, 0.50, Rgba::rgb(0, 2, 12)),
        (0.60, 0.75, 0.45, Rgba::rgb(14, 30, 90)),
        (-0.10, 0.30, 0.30, Rgba::rgb(18, 28, 66)),
    ],
    ink: [Rgba::rgb(127, 152, 240), Rgba::rgb(125, 144, 241), Rgba::rgb(160, 136, 238)],
    caption: Rgba::rgb(164, 169, 193),
    card: Rgba::rgb(255, 255, 255).with_alpha(36),
    title: Rgba::rgb(244, 245, 248),
    value: Rgba::rgb(157, 161, 188),
    chevron: Rgba::rgb(118, 122, 160),
    status: Rgba::rgb(255, 255, 255),
};
