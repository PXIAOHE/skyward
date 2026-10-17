use anyhow::{anyhow, Result};
use fontdue::{Font, FontSettings};
use std::path::Path;

/// The UI uses two weights: a regular face for body text and a heavier face for
/// titles. On the device both are loaded from `/twres`; a single file may back
/// both faces when only one weight is shipped.
pub struct Fonts {
    pub regular: Font,
    pub title: Font,
}

impl Fonts {
    pub fn load(regular: impl AsRef<Path>, title: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            regular: load_face(regular.as_ref(), 0)?,
            title: load_face(title.as_ref(), 0)?,
        })
    }

    pub fn load_single(path: impl AsRef<Path>, index: u32) -> Result<Self> {
        let f = load_face(path.as_ref(), index)?;
        let g = load_face(path.as_ref(), index)?;
        Ok(Self { regular: f, title: g })
    }
}

fn load_face(path: &Path, index: u32) -> Result<Font> {
    let bytes = std::fs::read(path).map_err(|e| anyhow!("read font {}: {e}", path.display()))?;
    let settings = FontSettings { collection_index: index, ..FontSettings::default() };
    Font::from_bytes(bytes, settings).map_err(|e| anyhow!("parse font {}: {e}", path.display()))
}
