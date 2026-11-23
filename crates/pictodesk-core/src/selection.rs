//! Selections, Photoshop-style: a grayscale mask over the document.
//! 255 = fully selected, 0 = not selected, feathered edges land in
//! between. Every paint, adjustment, and filter in Phase 1 respects
//! this mask — that's what made Photoshop 1.0 a retouching tool
//! rather than a paint program.
//!
//! Constructors mirror the 1.0 selection tools: Rectangular and
//! Elliptical Marquee, Lasso (freehand polygon), Magic Wand (color
//! flood with tolerance). Select > Feather blurs the mask edge;
//! Select > Inverse flips it.

use crate::document::Document;
use crate::tile::TileCoord;

#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub width: u32,
    pub height: u32,
    mask: Vec<u8>,
}

impl Selection {
    pub fn none(width: u32, height: u32) -> Self {
        Selection {
            width,
            height,
            mask: vec![0; (width * height) as usize],
        }
    }

    pub fn all(width: u32, height: u32) -> Self {
        Selection {
            width,
            height,
            mask: vec![255; (width * height) as usize],
        }
    }

    pub fn rect(width: u32, height: u32, x: i32, y: i32, w: u32, h: u32) -> Self {
        let mut s = Selection::none(width, height);
        for yy in y.max(0)..(y + h as i32).min(height as i32) {
            for xx in x.max(0)..(x + w as i32).min(width as i32) {
                s.set(xx, yy, 255);
            }
        }
        s
    }

    pub fn ellipse(width: u32, height: u32, x: i32, y: i32, w: u32, h: u32) -> Self {
        let mut s = Selection::none(width, height);
        if w == 0 || h == 0 {
            return s;
        }
        let (cx, cy) = (x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0);
        let (rx, ry) = (w as f32 / 2.0, h as f32 / 2.0);
        for yy in y.max(0)..(y + h as i32).min(height as i32) {
            for xx in x.max(0)..(x + w as i32).min(width as i32) {
                let dx = (xx as f32 + 0.5 - cx) / rx;
                let dy = (yy as f32 + 0.5 - cy) / ry;
                if dx * dx + dy * dy <= 1.0 {
                    s.set(xx, yy, 255);
                }
            }
        }
        s
    }

    /// Lasso: fill the closed polygon through `points` (even-odd rule).
    pub fn polygon(width: u32, height: u32, points: &[(f32, f32)]) -> Self {
        let mut s = Selection::none(width, height);
        if points.len() < 3 {
            return s;
        }
        let (mut miny, mut maxy) = (f32::MAX, f32::MIN);
        for p in points {
            miny = miny.min(p.1);
            maxy = maxy.max(p.1);
        }
        let y0 = (miny.floor() as i32).max(0);
        let y1 = (maxy.ceil() as i32).min(height as i32 - 1);
        let n = points.len();
        for yy in y0..=y1 {
            let cy = yy as f32 + 0.5;
            let mut nodes: Vec<f32> = Vec::new();
            for i in 0..n {
                let (x1, y1p) = points[i];
                let (x2, y2p) = points[(i + 1) % n];
                if (cy >= y1p && cy < y2p) || (cy >= y2p && cy < y1p) {
                    nodes.push(x1 + (cy - y1p) / (y2p - y1p) * (x2 - x1));
                }
            }
            nodes.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in nodes.as_chunks::<2>().0 {
                let xa = (pair[0].ceil() as i32).max(0);
                let xb = (pair[1].floor() as i32).min(width as i32 - 1);
                for xx in xa..=xb {
                    s.set(xx, yy, 255);
                }
            }
        }
        s
    }

    /// Magic Wand: select pixels whose flattened color is within
    /// `tolerance` (0..=255, max per-channel difference vs. the clicked
    /// pixel). Contiguous = flood from the click; otherwise global.
    pub fn wand(doc: &Document, x: i32, y: i32, tolerance: u8, contiguous: bool) -> Self {
        let (w, h) = (doc.width, doc.height);
        let mut s = Selection::none(w, h);
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return s;
        }
        let img = doc.render_full_rgba8();
        let at = |px: i32, py: i32| -> (u8, u8, u8) {
            let i = ((py as u32 * w + px as u32) * 4) as usize;
            (img[i], img[i + 1], img[i + 2])
        };
        let target = at(x, y);
        let tol = tolerance as i32;
        let matches = |px: i32, py: i32| -> bool {
            let c = at(px, py);
            (c.0 as i32 - target.0 as i32).abs() <= tol
                && (c.1 as i32 - target.1 as i32).abs() <= tol
                && (c.2 as i32 - target.2 as i32).abs() <= tol
        };
        if !contiguous {
            for yy in 0..h as i32 {
                for xx in 0..w as i32 {
                    if matches(xx, yy) {
                        s.set(xx, yy, 255);
                    }
                }
            }
            return s;
        }
        // Flood fill (iterative, scanline-ish via stack of points).
        let mut stack = vec![(x, y)];
        s.set(x, y, 255);
        while let Some((cx, cy)) = stack.pop() {
            for (nx, ny) in [(cx - 1, cy), (cx + 1, cy), (cx, cy - 1), (cx, cy + 1)] {
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                if s.value(nx, ny) == 0 && matches(nx, ny) {
                    s.set(nx, ny, 255);
                    stack.push((nx, ny));
                }
            }
        }
        s
    }

    #[inline]
    pub fn value(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.mask[(y as u32 * self.width + x as u32) as usize]
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, v: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as u32 * self.width + x as u32) as usize;
        self.mask[i] = v;
    }

    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|&v| v == 0)
    }

    /// Bounding box of selected (>0) pixels: (x, y, w, h).
    pub fn bounds(&self) -> Option<(i32, i32, u32, u32)> {
        let (mut x0, mut y0) = (i32::MAX, i32::MAX);
        let (mut x1, mut y1) = (i32::MIN, i32::MIN);
        for yy in 0..self.height as i32 {
            for xx in 0..self.width as i32 {
                if self.value(xx, yy) > 0 {
                    x0 = x0.min(xx);
                    y0 = y0.min(yy);
                    x1 = x1.max(xx);
                    y1 = y1.max(yy);
                }
            }
        }
        if x1 < x0 {
            None
        } else {
            Some((x0, y0, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32))
        }
    }

    pub fn invert(&mut self) {
        for v in &mut self.mask {
            *v = 255 - *v;
        }
    }

    /// Select > Feather: blur the mask edge by `radius` pixels
    /// (two separable box-blur passes approximate Photoshop's falloff).
    pub fn feather(&mut self, radius: u32) {
        if radius == 0 || self.is_empty() {
            return;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let r = radius as i32;
        for _ in 0..2 {
            // horizontal
            let src = self.mask.clone();
            for yy in 0..h {
                let mut sum: u32 = 0;
                for xx in 0..w as i32 {
                    if xx == 0 {
                        sum = (0..=r)
                            .map(|k| src[yy * w + (k as usize).min(w - 1)] as u32)
                            .sum();
                    }
                    self.mask[yy * w + xx as usize] = (sum / (2 * r + 1) as u32).min(255) as u8;
                    let out_i = (xx - r).max(0) as usize;
                    let in_i = (xx + r + 1).min(w as i32 - 1) as usize;
                    sum = sum - src[yy * w + out_i] as u32 + src[yy * w + in_i] as u32;
                }
            }
            // vertical
            let src = self.mask.clone();
            for xx in 0..w {
                let mut sum: u32 = 0;
                for yy in 0..h as i32 {
                    if yy == 0 {
                        sum = (0..=r)
                            .map(|k| src[(k as usize).min(h - 1) * w + xx] as u32)
                            .sum();
                    }
                    self.mask[yy as usize * w + xx] = (sum / (2 * r + 1) as u32).min(255) as u8;
                    let out_i = (yy - r).max(0) as usize;
                    let in_i = (yy + r + 1).min(h as i32 - 1) as usize;
                    sum = sum - src[out_i * w + xx] as u32 + src[in_i * w + xx] as u32;
                }
            }
        }
    }

    /// Tile coordinates intersecting a pixel-space rect — the set a
    /// history command must snapshot to cover an edit in that rect.
    pub fn tiles_in_rect(x: i32, y: i32, w: u32, h: u32) -> Vec<TileCoord> {
        if w == 0 || h == 0 {
            return Vec::new();
        }
        let (tx0, ty0) = (x.div_euclid(256), y.div_euclid(256));
        let (tx1, ty1) = (
            (x + w as i32 - 1).div_euclid(256),
            (y + h as i32 - 1).div_euclid(256),
        );
        let mut out = Vec::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                out.push((tx, ty));
            }
        }
        out
    }

    pub fn raw_mask(&self) -> &[u8] {
        &self.mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Pixel16;

    #[test]
    fn rect_selection_bounds_and_values() {
        let s = Selection::rect(100, 80, 10, 10, 20, 15);
        assert_eq!(s.value(15, 15), 255);
        assert_eq!(s.value(9, 15), 0);
        assert_eq!(s.bounds(), Some((10, 10, 20, 15)));
    }

    #[test]
    fn ellipse_excludes_corners() {
        let s = Selection::ellipse(100, 100, 0, 0, 100, 100);
        assert_eq!(s.value(50, 50), 255);
        assert_eq!(s.value(2, 2), 0);
    }

    #[test]
    fn polygon_triangle_fills_inside_only() {
        let s = Selection::polygon(40, 40, &[(5.0, 5.0), (35.0, 5.0), (5.0, 35.0)]);
        assert_eq!(s.value(8, 8), 255);
        assert_eq!(s.value(30, 30), 0);
    }

    #[test]
    fn wand_selects_matching_region_contiguously() {
        let mut doc = Document::new(20, 10);
        doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(200, 0, 0, 255));
        doc.layers[0].fill_rect(10, 0, 10, 10, Pixel16::from_rgba8(0, 0, 200, 255));
        // an island of red inside the blue side
        doc.layers[0].fill_rect(15, 4, 2, 2, Pixel16::from_rgba8(200, 0, 0, 255));
        let s = Selection::wand(&doc, 2, 2, 10, true);
        assert_eq!(s.value(5, 5), 255);
        assert_eq!(s.value(15, 5), 0, "island must not join a contiguous wand");
        let s2 = Selection::wand(&doc, 2, 2, 10, false);
        assert_eq!(s2.value(15, 5), 255, "non-contiguous wand finds the island");
    }

    #[test]
    fn feather_softens_edges() {
        let mut s = Selection::rect(30, 30, 10, 10, 10, 10);
        s.feather(2);
        let edge = s.value(9, 15);
        assert!(edge > 0 && edge < 255, "feathered edge was {edge}");
        assert_eq!(s.value(15, 15), 255, "interior stays fully selected");
    }

    #[test]
    fn invert_flips_mask() {
        let mut s = Selection::rect(10, 10, 0, 0, 5, 10);
        s.invert();
        assert_eq!(s.value(2, 5), 0);
        assert_eq!(s.value(7, 5), 255);
    }

    #[test]
    fn tiles_in_rect_spans_boundaries() {
        let tiles = Selection::tiles_in_rect(250, 250, 20, 20);
        assert_eq!(tiles.len(), 4);
    }
}
