use crate::color::{blend_over, Rgba};
use fontdue::Font;

/// A straight-alpha RGBA8 raster target. All primitives are antialiased via
/// signed-distance coverage so the same code produces crisp output on the
/// device framebuffer and in host PNG previews.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, data: vec![0; (w * h * 4) as usize] }
    }

    #[inline]
    fn px(&mut self, x: i32, y: i32) -> Option<&mut [u8]> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return None;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        Some(&mut self.data[i..i + 4])
    }

    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, color: Rgba, cov: f32) {
        if let Some(p) = self.px(x, y) {
            blend_over(p, color, cov);
        }
    }

    pub fn clear(&mut self, color: Rgba) {
        for px in self.data.chunks_exact_mut(4) {
            px[0] = color.r;
            px[1] = color.g;
            px[2] = color.b;
            px[3] = color.a;
        }
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x + w).ceil() as i32;
        let y1 = (y + h).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend(px, py, color, 1.0);
            }
        }
    }

    /// Vertical linear gradient from `top` to `bottom`.
    pub fn vgradient(&mut self, x: f32, y: f32, w: f32, h: f32, top: Rgba, bottom: Rgba) {
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x + w).ceil() as i32;
        let y1 = (y + h).ceil() as i32;
        let span = (h).max(1.0);
        for py in y0..y1 {
            let t = ((py as f32 - y) / span).clamp(0.0, 1.0);
            let c = top.mix(bottom, t);
            for px in x0..x1 {
                self.blend(px, py, c, 1.0);
            }
        }
    }

    pub fn fill_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: Rgba) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0, h / 2.0);
        let r = r.min(hx).min(hy);
        let x0 = (x - 1.0).floor() as i32;
        let y0 = (y - 1.0).floor() as i32;
        let x1 = (x + w + 1.0).ceil() as i32;
        let y1 = (y + h + 1.0).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let d = rr_sdf(px as f32 + 0.5, py as f32 + 0.5, cx, cy, hx, hy, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    pub fn stroke_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, t: f32, color: Rgba) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0, h / 2.0);
        let r = r.min(hx).min(hy);
        let pad = t + 1.0;
        let x0 = (x - pad).floor() as i32;
        let y0 = (y - pad).floor() as i32;
        let x1 = (x + w + pad).ceil() as i32;
        let y1 = (y + h + pad).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let d = rr_sdf(px as f32 + 0.5, py as f32 + 0.5, cx, cy, hx, hy, r);
                let cov = (0.5 - (d.abs() - t / 2.0)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    /// Soft drop shadow for a rounded rect: a blurred alpha mask of the shape,
    /// offset and tinted. `blur` is the box-blur radius in pixels.
    pub fn round_rect_shadow(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, dy: f32, color: Rgba) {
        let pad = (blur * 2.0 + 2.0).ceil() as i32;
        let mw = (w.ceil() as i32 + pad * 2).max(1) as usize;
        let mh = (h.ceil() as i32 + pad * 2).max(1) as usize;
        let mut mask = vec![0f32; mw * mh];
        let (cx, cy) = (pad as f32 + w / 2.0, pad as f32 + h / 2.0);
        let (hx, hy) = (w / 2.0, h / 2.0);
        let rr = r.min(hx).min(hy);
        for my in 0..mh {
            for mx in 0..mw {
                let d = rr_sdf(mx as f32 + 0.5, my as f32 + 0.5, cx, cy, hx, hy, rr);
                mask[my * mw + mx] = (0.5 - d).clamp(0.0, 1.0);
            }
        }
        box_blur(&mut mask, mw, mh, blur.round().max(1.0) as usize);
        let ox = x.floor() as i32 - pad;
        let oy = y.floor() as i32 - pad + dy.round() as i32;
        for my in 0..mh {
            for mx in 0..mw {
                let a = mask[my * mw + mx];
                if a > 0.003 {
                    self.blend(ox + mx as i32, oy + my as i32, color, a);
                }
            }
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgba) {
        let x0 = (cx - r - 1.0).floor() as i32;
        let y0 = (cy - r - 1.0).floor() as i32;
        let x1 = (cx + r + 1.0).ceil() as i32;
        let y1 = (cy + r + 1.0).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, t: f32, color: Rgba) {
        let x0 = (cx - r - t).floor() as i32;
        let y0 = (cy - r - t).floor() as i32;
        let x1 = (cx + r + t).ceil() as i32;
        let y1 = (cy + r + t).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - (d.abs() - t / 2.0)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, t: f32, color: Rgba) {
        let minx = (x0.min(x1) - t).floor() as i32;
        let miny = (y0.min(y1) - t).floor() as i32;
        let maxx = (x0.max(x1) + t).ceil() as i32;
        let maxy = (y0.max(y1) + t).ceil() as i32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len2 = (vx * vx + vy * vy).max(1e-6);
        for py in miny..maxy {
            for px in minx..maxx {
                let wx = px as f32 + 0.5 - x0;
                let wy = py as f32 + 0.5 - y0;
                let s = ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0);
                let dx = wx - vx * s;
                let dy = wy - vy * s;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (0.5 - (d - t / 2.0)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    pub fn text_width(&self, font: &Font, text: &str, px: f32) -> f32 {
        text.chars().map(|c| font.metrics(c, px).advance_width).sum()
    }

    /// Draw a run of text with its baseline at `(x, baseline)`. Returns the
    /// advance width. Truncates with an ellipsis when `max_w` is exceeded.
    pub fn text(
        &mut self,
        font: &Font,
        text: &str,
        x: f32,
        baseline: f32,
        px: f32,
        color: Rgba,
        max_w: Option<f32>,
    ) -> f32 {
        let mut pen = x;
        let limit = max_w.map(|m| x + m);
        let ell_w = font.metrics('…', px).advance_width;
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if let Some(lim) = limit {
                let aw = font.metrics(ch, px).advance_width;
                let more = chars.peek().is_some();
                if pen + aw + if more { ell_w } else { 0.0 } > lim {
                    self.glyph(font, '…', pen, baseline, px, color);
                    pen += ell_w;
                    break;
                }
            }
            pen += self.glyph(font, ch, pen, baseline, px, color);
        }
        pen - x
    }

    pub fn text_center(&mut self, font: &Font, text: &str, cx: f32, baseline: f32, px: f32, color: Rgba) {
        let w = self.text_width(font, text, px);
        self.text(font, text, cx - w / 2.0, baseline, px, color, None);
    }

    fn glyph(&mut self, font: &Font, ch: char, pen: f32, baseline: f32, px: f32, color: Rgba) -> f32 {
        let (m, bitmap) = font.rasterize(ch, px);
        let gx = (pen + m.xmin as f32).round() as i32;
        let gy = (baseline - (m.ymin as f32 + m.height as f32)).round() as i32;
        for j in 0..m.height {
            for i in 0..m.width {
                let cov = bitmap[j * m.width + i] as f32 / 255.0;
                if cov > 0.0 {
                    self.blend(gx + i as i32, gy + j as i32, color, cov);
                }
            }
        }
        m.advance_width
    }
}

fn rr_sdf(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let dx = (px - cx).abs() - (hx - r);
    let dy = (py - cy).abs() - (hy - r);
    let ox = dx.max(0.0);
    let oy = dy.max(0.0);
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - r
}

fn box_blur(buf: &mut [f32], w: usize, h: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let mut tmp = vec![0f32; w * h];
    let norm = 1.0 / (2 * radius + 1) as f32;
    // Horizontal pass.
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let mut acc = 0.0;
            for k in 0..=2 * radius {
                let sx = (x + k).saturating_sub(radius).min(w - 1);
                acc += buf[row + sx];
            }
            tmp[row + x] = acc * norm;
        }
    }
    // Vertical pass.
    for x in 0..w {
        for y in 0..h {
            let mut acc = 0.0;
            for k in 0..=2 * radius {
                let sy = (y + k).saturating_sub(radius).min(h - 1);
                acc += tmp[sy * w + x];
            }
            buf[y * w + x] = acc * norm;
        }
    }
}
