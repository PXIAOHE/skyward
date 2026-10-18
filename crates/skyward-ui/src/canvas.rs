use crate::color::{blend_over, Rgba};
use crate::font::Face;
use ab_glyph::{point, Font, GlyphId, PxScale, ScaleFont};

/// A straight-alpha RGBA8 raster target. All primitives are antialiased via
/// signed-distance coverage so the same code produces crisp output on the
/// device framebuffer and in host PNG previews.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

/// How a layer covers a stretch of pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    Clear,
    Solid,
    Partial,
}

/// Pixels `x0..x1` of a row, all covered the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub x0: u16,
    pub x1: u16,
    pub cover: Cover,
}

// Drawing primitives take geometry plus style as plain arguments.
#[allow(clippy::too_many_arguments)]
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
        for px in self.data.as_chunks_mut::<4>().0 {
            *px = [color.r, color.g, color.b, color.a];
        }
    }

    /// Overwrite a pixel-aligned rectangle with `color`.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let x0 = (x.floor().max(0.0) as u32).min(self.w);
        let y0 = (y.floor().max(0.0) as u32).min(self.h);
        let x1 = ((x + w).ceil().max(0.0) as u32).min(self.w);
        let y1 = ((y + h).ceil().max(0.0) as u32).min(self.h);
        let px = [color.r, color.g, color.b, color.a];
        for row in y0..y1 {
            let start = ((row * self.w + x0) * 4) as usize;
            let end = ((row * self.w + x1) * 4) as usize;
            for p in self.data[start..end].as_chunks_mut::<4>().0 {
                *p = px;
            }
        }
    }

    /// Paint rows `y0..y1` with a colour computed per pixel centre.
    pub fn shade_rows(&mut self, y0: u32, y1: u32, paint: impl Fn(f32, f32) -> Rgba) {
        for y in y0..y1.min(self.h) {
            for x in 0..self.w {
                let c = paint(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x as i32, y as i32, c, 1.0);
            }
        }
    }

    /// Overwrite row `y` with `paint(x)` for every column `x`.
    pub fn paint_row(&mut self, y: u32, paint: impl Fn(usize) -> Rgba) {
        let start = (y * self.w * 4) as usize;
        let row = &mut self.data[start..start + (self.w * 4) as usize];
        for (x, p) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let c = paint(x);
            *p = [c.r, c.g, c.b, c.a];
        }
    }

    /// Lay `layer`, whose top-left corner goes at (`x`, `y`), over this
    /// opaque canvas at `alpha` of its strength.
    pub fn lay(&mut self, layer: &Canvas, x: i32, y: i32, alpha: f32) {
        if alpha <= 0.0 {
            return;
        }
        let strength = (alpha * 256.0).round() as u32;
        let (lw, sw) = (layer.w as i32, self.w as i32);
        let (x0, x1) = ((-x).max(0), (sw - x).min(lw));
        for ly in 0..layer.h as i32 {
            let py = y + ly;
            if py < 0 || py >= self.h as i32 || x0 >= x1 {
                continue;
            }
            let src = &layer.data[((ly * lw + x0) * 4) as usize..((ly * lw + x1) * 4) as usize];
            let start = ((py * sw + x + x0) * 4) as usize;
            let dst = &mut self.data[start..start + src.len()];
            for (s, d) in src.as_chunks::<4>().0.iter().zip(dst.as_chunks_mut::<4>().0) {
                if s[3] == 0 {
                    continue;
                }
                let a = ((s[3] as u32 + (s[3] as u32 >> 7)) * strength) >> 8;
                for k in 0..3 {
                    d[k] = ((s[k] as u32 * a + d[k] as u32 * (256 - a) + 128) >> 8) as u8;
                }
            }
        }
    }

    /// Copy the opaque `strip`, as wide as this canvas, over it with its top
    /// row at `y`; rows past either edge are left out.
    pub fn put(&mut self, strip: &Canvas, y: i32) {
        assert_eq!(strip.w, self.w, "a strip spans the canvas");
        let first = (-y).clamp(0, strip.h as i32);
        let last = (self.h as i32 - y).clamp(first, strip.h as i32);
        if first == last {
            return;
        }
        let row = self.w as usize * 4;
        let dst = (y + first) as usize * row;
        let n = (last - first) as usize * row;
        self.data[dst..dst + n].copy_from_slice(&strip.data[first as usize * row..first as usize * row + n]);
    }

    /// Each row as runs of clear, solid and partly covered pixels, for
    /// laying the canvas over something that keeps changing.
    pub fn runs(&self) -> Vec<Vec<Run>> {
        let w = self.w as usize;
        let cover = |a: u8| match a {
            0 => Cover::Clear,
            255 => Cover::Solid,
            _ => Cover::Partial,
        };
        self.data
            .chunks_exact(w * 4)
            .map(|row| {
                let mut runs = Vec::new();
                let (mut start, mut now) = (0, cover(row[3]));
                for x in 1..w {
                    let c = cover(row[x * 4 + 3]);
                    if c != now {
                        runs.push(Run { x0: start as u16, x1: x as u16, cover: now });
                        (start, now) = (x, c);
                    }
                }
                runs.push(Run { x0: start as u16, x1: w as u16, cover: now });
                runs
            })
            .collect()
    }

    /// Drop everything below row `h`, for pages measured after drawing.
    pub fn crop_height(mut self, h: u32) -> Canvas {
        let h = h.min(self.h);
        self.data.truncate((self.w * h * 4) as usize);
        self.h = h;
        self
    }

    pub fn fill_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: Rgba) {
        self.fill_round_rect_paint(x, y, w, h, r, &|_, _| color);
    }

    /// [`Canvas::fill_round_rect`] with the colour chosen per pixel.
    pub fn fill_round_rect_paint(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, paint: &dyn Fn(f32, f32) -> Rgba) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0, h / 2.0);
        let r = r.min(hx).min(hy);
        let x0 = (x - 1.0).floor() as i32;
        let y0 = (y - 1.0).floor() as i32;
        let x1 = (x + w + 1.0).ceil() as i32;
        let y1 = (y + h + 1.0).ceil() as i32;
        for py in y0..y1 {
            let fy = py as f32 + 0.5;
            // Pixels at least half a pixel inside the straight edges are fully
            // covered: the whole width between the corners' rows, the part
            // between the corners above and below them.
            let dy = (fy - cy).abs();
            let reach = if dy <= hy - r {
                hx - 0.5
            } else if dy <= hy - 0.5 {
                hx - r
            } else {
                -1.0
            };
            for px in x0..x1 {
                let fx = px as f32 + 0.5;
                if (fx - cx).abs() <= reach {
                    self.blend(px, py, paint(fx, fy), 1.0);
                    continue;
                }
                let d = rr_sdf(fx, fy, cx, cy, hx, hy, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, paint(fx, fy), cov);
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

    /// Round-capped arc stroke. Angles are in radians in screen space (0 points
    /// right, positive turns clockwise because y grows downward); the arc runs
    /// from `start` through `sweep` radians.
    pub fn stroke_arc(&mut self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, t: f32, color: Rgba) {
        let tau = std::f32::consts::TAU;
        let (sx, sy) = (cx + r * start.cos(), cy + r * start.sin());
        let end = start + sweep;
        let (ex, ey) = (cx + r * end.cos(), cy + r * end.sin());
        let x0 = (cx - r - t).floor() as i32;
        let y0 = (cy - r - t).floor() as i32;
        let x1 = (cx + r + t).ceil() as i32;
        let y1 = (cy + r + t).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let (dx, dy) = (x - cx, y - cy);
                let rel = (dy.atan2(dx) - start).rem_euclid(tau);
                let d = if rel <= sweep {
                    ((dx * dx + dy * dy).sqrt() - r).abs()
                } else {
                    let a = ((x - sx).powi(2) + (y - sy).powi(2)).sqrt();
                    let b = ((x - ex).powi(2) + (y - ey).powi(2)).sqrt();
                    a.min(b)
                };
                let cov = (0.5 - (d - t / 2.0)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    /// Ellipse outline using a first-order distance estimate, which is accurate
    /// for the thin strokes icons use.
    pub fn stroke_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, t: f32, color: Rgba) {
        let x0 = (cx - rx - t).floor() as i32;
        let y0 = (cy - ry - t).floor() as i32;
        let x1 = (cx + rx + t).ceil() as i32;
        let y1 = (cy + ry + t).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let (x, y) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let f = (x / rx).powi(2) + (y / ry).powi(2) - 1.0;
                let gx = 2.0 * x / (rx * rx);
                let gy = 2.0 * y / (ry * ry);
                let d = f.abs() / (gx * gx + gy * gy).sqrt().max(1e-6);
                let cov = (0.5 - (d - t / 2.0)).clamp(0.0, 1.0);
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

    /// Antialiased fill of a convex polygon (points in either winding).
    pub fn fill_convex(&mut self, pts: &[(f32, f32)], color: Rgba) {
        if pts.len() < 3 {
            return;
        }
        let n = pts.len() as f32;
        let (cx, cy) = pts.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x / n, sy + y / n));
        // Per edge: unit normal pointing out of the polygon and its offset.
        let edges: Vec<(f32, f32, f32)> = (0..pts.len())
            .map(|i| {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % pts.len()];
                let (ex, ey) = (bx - ax, by - ay);
                let len = (ex * ex + ey * ey).sqrt().max(1e-6);
                let (mut nx, mut ny) = (ey / len, -ex / len);
                if nx * (cx - ax) + ny * (cy - ay) > 0.0 {
                    nx = -nx;
                    ny = -ny;
                }
                (nx, ny, nx * ax + ny * ay)
            })
            .collect();
        let minx = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor() as i32 - 1;
        let maxx = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i32 + 1;
        let miny = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32 - 1;
        let maxy = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32 + 1;
        for py in miny..maxy {
            for px in minx..maxx {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let d = edges.iter().map(|&(nx, ny, c)| nx * x + ny * y - c).fold(f32::MIN, f32::max);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(px, py, color, cov);
                }
            }
        }
    }

    pub fn text_width(&self, face: &Face, text: &str, px: f32) -> f32 {
        let sf = face.font.as_scaled(face.scale(px));
        let mut w = 0.0;
        let mut prev: Option<GlyphId> = None;
        for ch in text.chars() {
            let id = face.font.glyph_id(ch);
            if let Some(p) = prev {
                w += sf.kern(p, id);
            }
            w += sf.h_advance(id);
            prev = Some(id);
        }
        w
    }

    /// Draw a run of text with its baseline at `(x, baseline)`. `px` is the em
    /// size. Returns the advance width; truncates with an ellipsis past `max_w`.
    pub fn text(
        &mut self,
        face: &Face,
        text: &str,
        x: f32,
        baseline: f32,
        px: f32,
        color: Rgba,
        max_w: Option<f32>,
    ) -> f32 {
        self.text_paint(face, text, x, baseline, px, &|_, _| color, max_w)
    }

    /// [`Canvas::text`] with the ink colour chosen per pixel, for gradient
    /// lettering.
    pub fn text_paint(
        &mut self,
        face: &Face,
        text: &str,
        x: f32,
        baseline: f32,
        px: f32,
        paint: &dyn Fn(f32, f32) -> Rgba,
        max_w: Option<f32>,
    ) -> f32 {
        let scale = face.scale(px);
        let sf = face.font.as_scaled(scale);
        let ell = face.font.glyph_id('…');
        let ell_w = sf.h_advance(ell);
        let limit = max_w.map(|m| x + m);
        let chars: Vec<char> = text.chars().collect();
        let mut pen = x;
        let mut prev: Option<GlyphId> = None;
        for (i, &ch) in chars.iter().enumerate() {
            let id = face.font.glyph_id(ch);
            if let Some(p) = prev {
                pen += sf.kern(p, id);
            }
            let aw = sf.h_advance(id);
            if let Some(lim) = limit {
                let more = i + 1 < chars.len();
                if pen + aw + if more { ell_w } else { 0.0 } > lim {
                    self.glyph(face, ell, scale, pen, baseline, paint);
                    pen += ell_w;
                    break;
                }
            }
            self.glyph(face, id, scale, pen, baseline, paint);
            pen += aw;
            prev = Some(id);
        }
        pen - x
    }

    /// Text whose ink is vertically centred on `cy` (by the reference ideograph).
    pub fn text_vc(&mut self, face: &Face, text: &str, x: f32, cy: f32, px: f32, color: Rgba, max_w: Option<f32>) -> f32 {
        let (top, bottom) = face.ink(px);
        self.text(face, text, x, cy - (top + bottom) / 2.0, px, color, max_w)
    }

    /// Text whose ink top sits at `top_y`.
    pub fn text_top(&mut self, face: &Face, text: &str, x: f32, top_y: f32, px: f32, color: Rgba, max_w: Option<f32>) -> f32 {
        let (top, _) = face.ink(px);
        self.text(face, text, x, top_y - top, px, color, max_w)
    }

    /// Text centred horizontally on `cx` and vertically on `cy`.
    pub fn text_center(&mut self, face: &Face, text: &str, cx: f32, cy: f32, px: f32, color: Rgba) {
        let w = self.text_width(face, text, px);
        self.text_vc(face, text, cx - w / 2.0, cy, px, color, None);
    }

    fn glyph(&mut self, face: &Face, id: GlyphId, scale: PxScale, x: f32, baseline: f32, paint: &dyn Fn(f32, f32) -> Rgba) {
        let g = id.with_scale_and_position(scale, point(x, baseline));
        if let Some(og) = face.font.outline_glyph(g) {
            let b = og.px_bounds();
            let (ox, oy) = (b.min.x as i32, b.min.y as i32);
            og.draw(|gx, gy, cov| {
                if cov > 0.0 {
                    let (px, py) = (ox + gx as i32, oy + gy as i32);
                    self.blend(px, py, paint(px as f32 + 0.5, py as f32 + 0.5), cov);
                }
            });
        }
    }
}

fn rr_sdf(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let dx = (px - cx).abs() - (hx - r);
    let dy = (py - cy).abs() - (hy - r);
    let ox = dx.max(0.0);
    let oy = dy.max(0.0);
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - r
}

