#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

/// Linear blend from `a` (at 0) to `b` (at 1).
pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgba { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }
}

/// `top` laid over `under`, both straight alpha.
pub fn over(top: Rgba, under: Rgba) -> Rgba {
    let mut px = [under.r, under.g, under.b, under.a];
    blend_over(&mut px, top, 1.0);
    Rgba { r: px[0], g: px[1], b: px[2], a: px[3] }
}

/// Source-over composite of `src` (with an extra coverage `cov` in 0..=1) onto
/// `dst` (a 4-byte RGBA slice), straight alpha.
pub fn blend_over(dst: &mut [u8], src: Rgba, cov: f32) {
    let sa = (src.a as f32 / 255.0) * cov.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    // An opaque source at full coverage comes out of the formula below as
    // itself, so write it directly.
    if sa >= 1.0 {
        dst.copy_from_slice(&[src.r, src.g, src.b, 255]);
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    let ch = |s: u8, d: u8| {
        let s = s as f32 / 255.0;
        let d = d as f32 / 255.0;
        let v = (s * sa + d * da * (1.0 - sa)) / out_a;
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    dst[0] = ch(src.r, dst[0]);
    dst[1] = ch(src.g, dst[1]);
    dst[2] = ch(src.b, dst[2]);
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}
