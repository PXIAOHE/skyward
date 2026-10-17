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
    pub mono_icon: Rgba,
    pub danger: Rgba,
    pub danger_soft: Rgba,
}

/// Layout metrics measured from the stock Settings and expressed in dp, then
/// converted at the panel's density. The 17 Ultra is 1200 px wide at 3.0 px/dp.
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
    pub back_x: f32,
    pub back_cy: f32,
    pub back_w: f32,
    pub back_h: f32,
    pub stroke: f32,
    pub content_top: f32,
    pub search_h: f32,
    pub search_text_inset: f32,
    pub after_search: f32,
    pub row_title_px: f32,
    pub sub_px: f32,
    pub sub_gap: f32,
    pub sub_pitch: f32,
    pub account_title_px: f32,
    pub account_row_h: f32,
    pub avatar: f32,
    pub badge_px: f32,
    pub badge_w: f32,
    pub badge_h: f32,
    pub chevron_w: f32,
    pub chevron_h: f32,
    pub chevron_inset: f32,
    pub value_gap: f32,
    pub switch_w: f32,
    pub switch_h: f32,
    pub knob: f32,
    pub slider_h: f32,
    pub status_x: f32,
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
    let px_per_dp = 3.0 * w as f32 / 1200.0;
    let dp = |v: f32| v * px_per_dp;
    Metrics {
        w,
        h,
        px_per_dp,
        margin: dp(12.57),
        card_radius: dp(15.7),
        card_gap: dp(12.57),
        section_block: dp(46.86),
        section_label_top: dp(23.43),
        row_h: dp(58.57),
        row_pad_v: dp(18.0),
        inner: dp(16.7),
        icon: dp(29.43),
        icon_radius: dp(8.0),
        icon_gap: dp(17.14),
        title_px: dp(29.5),
        title_x: dp(28.6),
        title_top: dp(109.4),
        back_x: dp(29.4),
        back_cy: dp(70.3),
        back_w: dp(24.6),
        back_h: dp(17.7),
        stroke: dp(2.0),
        content_top: dp(158.0),
        search_h: dp(46.0),
        search_text_inset: dp(46.9),
        after_search: dp(13.4),
        row_title_px: dp(16.0),
        sub_px: dp(13.0),
        sub_gap: dp(6.6),
        sub_pitch: dp(18.3),
        account_title_px: dp(17.5),
        account_row_h: dp(68.0),
        avatar: dp(35.7),
        badge_px: dp(14.5),
        badge_w: dp(72.9),
        badge_h: dp(28.9),
        chevron_w: dp(8.0),
        chevron_h: dp(14.3),
        chevron_inset: dp(17.7),
        value_gap: dp(10.6),
        switch_w: dp(51.1),
        switch_h: dp(28.3),
        knob: dp(20.9),
        slider_h: dp(29.4),
        status_x: dp(28.3),
        status_cy: dp(20.9),
        status_px: dp(17.0),
    }
}

// Sampled from the stock Settings.
const LIGHT: Palette = Palette {
    bg: Rgba::rgb(247, 247, 247),
    card: Rgba::rgb(255, 255, 255),
    text: Rgba::rgb(0, 0, 0),
    subtitle: Rgba::rgb(102, 102, 102),
    value: Rgba::rgb(153, 153, 153),
    section: Rgba::rgb(141, 147, 174),
    chevron: Rgba::rgb(178, 178, 178),
    accent: Rgba::rgb(74, 128, 247),
    track: Rgba::rgb(240, 240, 240),
    switch_off: Rgba::rgb(230, 230, 230),
    search: Rgba::rgb(232, 232, 232),
    search_icon: Rgba::rgb(163, 163, 163),
    placeholder: Rgba::rgb(162, 162, 162),
    badge: Rgba::rgb(235, 242, 254),
    avatar: Rgba::rgb(221, 223, 226),
    avatar_glyph: Rgba::rgb(255, 255, 255),
    status: Rgba::rgb(62, 62, 62),
    battery: Rgba::rgb(45, 200, 86),
    mono_icon: Rgba::rgb(179, 179, 179),
    danger: Rgba::rgb(230, 66, 60),
    danger_soft: Rgba::rgb(253, 236, 235),
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
    battery: Rgba::rgb(45, 200, 86),
    mono_icon: Rgba::rgb(110, 110, 110),
    danger: Rgba::rgb(255, 92, 84),
    danger_soft: Rgba::rgb(52, 24, 24),
};
