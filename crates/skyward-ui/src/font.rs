use ab_glyph::{Font, FontRef, PxScale, VariableFont};
use anyhow::{anyhow, Result};
use std::path::Path;

/// One weight instance of the UI typeface. MiSans ships as a variable font, so
/// every weight is the same file with a different `wght` coordinate.
pub struct Face {
    pub(crate) font: FontRef<'static>,
    upem: f32,
    height: f32,
}

impl Face {
    fn new(data: &'static [u8], index: u32, weight: f32) -> Result<Self> {
        let mut font =
            FontRef::try_from_slice_and_index(data, index).map_err(|e| anyhow!("parse font: {e}"))?;
        font.set_variation(b"wght", weight);
        let upem = font.units_per_em().ok_or_else(|| anyhow!("font has no units-per-em"))?;
        let height = font.height_unscaled();
        Ok(Face { font, upem, height })
    }

    /// ab_glyph scales by ascent-minus-descent; the UI sizes text by the em,
    /// the same way Android `sp` values are defined.
    pub fn scale(&self, em_px: f32) -> PxScale {
        PxScale::from(em_px * self.height / self.upem)
    }

    /// Vertical ink extent of the reference ideograph 「中」 relative to the
    /// baseline (negative is above). Rows centre text on this, as the stock
    /// settings do.
    pub fn ink(&self, em_px: f32) -> (f32, f32) {
        self.bounds('中', em_px)
    }

    /// Height of a capital letter above the baseline, for Latin lettering
    /// placed by its cap line.
    pub fn cap(&self, em_px: f32) -> f32 {
        -self.bounds('H', em_px).0
    }

    fn bounds(&self, ch: char, em_px: f32) -> (f32, f32) {
        let glyph = self.font.glyph_id(ch).with_scale_and_position(self.scale(em_px), ab_glyph::point(0.0, 0.0));
        let og = self.font.outline_glyph(glyph).unwrap_or_else(|| panic!("font has no outline for {ch:?}"));
        let b = og.px_bounds();
        (b.min.y, b.max.y)
    }
}

/// `wght` per text role, matched glyph-for-glyph against the stock Settings
/// by comparing colour-normalised ink coverage with a rendered weight ladder.
pub struct Weights {
    pub title: f32,
    pub heading: f32,
    pub label: f32,
    pub body: f32,
    pub value: f32,
    pub badge: f32,
    pub status: f32,
    pub strong: f32,
}

pub const STOCK_WEIGHTS: Weights = Weights {
    title: 380.0,
    heading: 560.0,
    label: 445.0,
    body: 480.0,
    value: 455.0,
    badge: 505.0,
    status: 360.0,
    strong: 700.0,
};

pub struct Fonts {
    /// Large page title.
    pub title: Face,
    /// Row and card titles.
    pub heading: Face,
    /// Section captions.
    pub label: Face,
    /// Subtitles, placeholders, explanatory text and the About wordmark.
    pub body: Face,
    /// Trailing values.
    pub value: Face,
    pub badge: Face,
    pub status: Face,
    /// Digits inside the battery pill.
    pub strong: Face,
    data: &'static [u8],
    index: u32,
}

impl Fonts {
    pub fn load(path: impl AsRef<Path>, index: u32) -> Result<Self> {
        Self::load_with(path, index, &STOCK_WEIGHTS)
    }

    pub fn load_with(path: impl AsRef<Path>, index: u32, w: &Weights) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| anyhow!("read font {}: {e}", path.display()))?;
        // The face borrows the bytes for the life of the process; one copy
        // backs every weight instance.
        let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let face = |wght: f32| Face::new(data, index, wght);
        Ok(Fonts {
            title: face(w.title)?,
            heading: face(w.heading)?,
            label: face(w.label)?,
            body: face(w.body)?,
            value: face(w.value)?,
            badge: face(w.badge)?,
            status: face(w.status)?,
            strong: face(w.strong)?,
            data,
            index,
        })
    }

    /// An extra instance at an arbitrary `wght` coordinate.
    pub fn face(&self, weight: f32) -> Result<Face> {
        Face::new(self.data, self.index, weight)
    }

    /// `(tag, min, default, max)` for every variation axis.
    pub fn axes(&self) -> Vec<(String, f32, f32, f32)> {
        self.title
            .font
            .variations()
            .into_iter()
            .map(|a| (String::from_utf8_lossy(&a.tag).into_owned(), a.min_value, a.default_value, a.max_value))
            .collect()
    }
}
