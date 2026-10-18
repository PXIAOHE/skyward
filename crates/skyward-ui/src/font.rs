use ab_glyph::{Font, FontRef, PxScale, VariableFont};
use anyhow::{anyhow, Result};
use std::path::Path;

/// One weight instance of a typeface. MiSans ships as a variable font, so
/// every UI weight is the same file with a different `wght` coordinate.
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

    /// Whether the typeface draws `ch` itself.
    pub fn has(&self, ch: char) -> bool {
        self.font.glyph_id(ch).0 != 0
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

/// `wght` per text role, fitted on Xiaomi 17 Ultra screenshots:
/// for each string, the size and weight whose rendering matches the ink
/// width, height and colour-normalised coverage (`skyward-preview --match`).
pub struct Weights {
    pub title: f32,
    pub heading: f32,
    pub label: f32,
    pub body: f32,
    pub value: f32,
    pub badge: f32,
    pub status: f32,
    pub strong: f32,
    pub display: f32,
}

pub const STOCK_WEIGHTS: Weights = Weights {
    title: 322.0,
    heading: 365.0,
    label: 370.0,
    body: 315.0,
    value: 320.0,
    badge: 505.0,
    status: 485.0,
    strong: 700.0,
    display: 480.0,
};

pub struct Fonts {
    /// Large page title.
    pub title: Face,
    /// Row, card and grid-value titles.
    pub heading: Face,
    /// Section captions.
    pub label: Face,
    /// Subtitles, placeholders and explanatory text.
    pub body: Face,
    /// Trailing values.
    pub value: Face,
    pub badge: Face,
    pub status: Face,
    /// Digits inside the battery pill.
    pub strong: Face,
    /// The About page wordmark.
    pub display: Face,
    /// The terminal's monospaced face, for the characters it has.
    pub mono: Face,
    /// What the UI typeface says about itself.
    pub info: FontInfo,
    /// What the monospaced typeface says about itself.
    pub mono_info: FontInfo,
    data: &'static [u8],
    index: u32,
}

/// A typeface's own name, version and copyright, from its name table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontInfo {
    pub family: String,
    pub version: String,
    /// The copyright notice, when the font carries one; some leave it to the
    /// licence file that comes with them.
    pub copyright: Option<String>,
}

impl FontInfo {
    fn read(data: &[u8], index: u32) -> Result<FontInfo> {
        let face = ttf_parser::Face::parse(data, index).map_err(|e| anyhow!("parse font: {e}"))?;
        let name = |id: u16| -> Option<String> {
            face.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string())
        };
        let needed = |id: u16| name(id).ok_or_else(|| anyhow!("the font names no entry {id}"));
        let version = needed(ttf_parser::name_id::VERSION)?;
        Ok(FontInfo {
            family: needed(ttf_parser::name_id::FAMILY)?,
            // "Version 4.009" names the version twice in a row labelled 版本.
            version: version.strip_prefix("Version ").unwrap_or(&version).to_string(),
            copyright: name(ttf_parser::name_id::COPYRIGHT_NOTICE),
        })
    }
}

/// Read a font file for the life of the process: the faces borrow its bytes.
fn leak(path: &Path) -> Result<&'static [u8]> {
    let bytes = std::fs::read(path).map_err(|e| anyhow!("read font {}: {e}", path.display()))?;
    Ok(Box::leak(bytes.into_boxed_slice()))
}

impl Fonts {
    /// The UI typeface from `path` (face `index` of a collection) and the
    /// terminal's monospaced one from `mono`.
    pub fn load(path: impl AsRef<Path>, index: u32, mono: impl AsRef<Path>) -> Result<Self> {
        Self::load_with(path, index, mono, &STOCK_WEIGHTS)
    }

    pub fn load_with(path: impl AsRef<Path>, index: u32, mono: impl AsRef<Path>, w: &Weights) -> Result<Self> {
        // One copy of the bytes backs every weight instance.
        let data = leak(path.as_ref())?;
        let mono_data = leak(mono.as_ref())?;
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
            display: face(w.display)?,
            // A single-weight file: the coordinate asks for the weight it is.
            mono: Face::new(mono_data, 0, 400.0)?,
            info: FontInfo::read(data, index)?,
            mono_info: FontInfo::read(mono_data, 0)?,
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

/// The typefaces the recovery image carries, from the repository.
#[cfg(test)]
pub(crate) fn bundled() -> Fonts {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/fonts");
    Fonts::load(format!("{dir}/MiSansVF.ttf"), 0, format!("{dir}/KirkMono-Regular.ttf")).unwrap()
}
