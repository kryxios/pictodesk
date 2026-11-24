//! Liquify (Phase 6): the mesh-warp filter that debuted in the
//! 6.0 generation — forward warp, two twirls, pucker, bloat,
//! shift pixels, reflection, freeze/thaw, and reconstruct.
//! The model is a displacement field over the document: one
//! (dx, dy) source offset per pixel. Painting with a Liquify
//! brush adds to the field; applying it resamples the layer
//! once, sampling each output pixel from (x + dx, y + dy).
//! Stamps use a cosine falloff inside the brush radius and
//! never touch frozen pixels (the Freeze mask, > 0 = frozen).

use crate::document::Document;
use crate::pixel::Pixel16;
use crate::selection::Selection;

/// A Liquify displacement field, row-major over `w * h` pixels.
/// Displacements are in pixels, in document space.
#[derive(Clone, Debug, PartialEq)]
pub struct LiquifyField {
    pub w: u32,
    pub h: u32,
    pub disp: Vec<(f32, f32)>,
}

/// Cosine brush falloff: 1 at the center, 0 at the rim.
fn falloff(dist: f32, radius: f32) -> f32 {
    if radius <= 0.0 || dist >= radius {
        return 0.0;
    }
    let t = dist / radius;
    0.5 + 0.5 * (t * std::f32::consts::PI).cos()
}

fn is_frozen(frozen: &[u8], i: usize) -> bool {
    frozen.get(i).copied().unwrap_or(0) > 0
}

impl LiquifyField {
    pub fn new(w: u32, h: u32) -> Self {
        LiquifyField {
            w,
            h,
            disp: vec![(0.0, 0.0); (w * h) as usize],
        }
    }

    /// True when no pixel is displaced (the untouched mesh).
    pub fn is_identity(&self) -> bool {
        self.disp.iter().all(|d| *d == (0.0, 0.0))
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        (y as u32 * self.w + x as u32) as usize
    }

    /// Pixels inside the disc at `c`: (index, falloff, rel x,
    /// rel y), where rel is the pixel's offset from `c`.
    fn disc_offsets(&self, c: (f32, f32), radius: f32) -> Vec<(usize, f32, f32, f32)> {
        let mut out = Vec::new();
        if radius <= 0.0 {
            return out;
        }
        let x0 = (c.0 - radius).floor().max(0.0) as i32;
        let x1 = (c.0 + radius).ceil().min(self.w as f32 - 1.0) as i32;
        let y0 = (c.1 - radius).floor().max(0.0) as i32;
        let y1 = (c.1 + radius).ceil().min(self.h as f32 - 1.0) as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (rx, ry) = (x as f32 - c.0, y as f32 - c.1);
                let dist = (rx * rx + ry * ry).sqrt();
                let f = falloff(dist, radius);
                if f > 0.0 {
                    out.push((self.idx(x, y), f, rx, ry));
                }
            }
        }
        out
    }

    /// Pixels within `radius` of the stroke segment from..to:
    /// (index, falloff, pixel x, pixel y, closest point x,
    /// closest point y on the segment).
    fn stroke_offsets(
        &self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
    ) -> Vec<(usize, f32, f32, f32, f32, f32)> {
        let mut out = Vec::new();
        if radius <= 0.0 {
            return out;
        }
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len2 = dx * dx + dy * dy;
        let x0 = (from.0.min(to.0) - radius).floor().max(0.0) as i32;
        let x1 = (from.0.max(to.0) + radius).ceil().min(self.w as f32 - 1.0) as i32;
        let y0 = (from.1.min(to.1) - radius).floor().max(0.0) as i32;
        let y1 = (from.1.max(to.1) + radius).ceil().min(self.h as f32 - 1.0) as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32, y as f32);
                let t = if len2 > 0.0 {
                    (((px - from.0) * dx + (py - from.1) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (qx, qy) = (from.0 + t * dx, from.1 + t * dy);
                let dist = ((px - qx) * (px - qx) + (py - qy) * (py - qy)).sqrt();
                let f = falloff(dist, radius);
                if f > 0.0 {
                    out.push((self.idx(x, y), f, px, py, qx, qy));
                }
            }
        }
        out
    }

    /// Warp: pixels near the stroke path are pushed along the
    /// stroke direction, strongest at the path.
    pub fn stamp_warp(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        frozen: &[u8],
    ) {
        let (mx, my) = ((to.0 - from.0) * pressure, (to.1 - from.1) * pressure);
        for (i, f, _, _, _, _) in self.stroke_offsets(from, to, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            self.disp[i].0 += mx * f;
            self.disp[i].1 += my * f;
        }
    }

    /// Twirl: rotate the sample points around the center by an
    /// angle that peaks at the center and fades at the rim.
    pub fn stamp_twirl(
        &mut self,
        c: (f32, f32),
        radius: f32,
        pressure: f32,
        clockwise: bool,
        frozen: &[u8],
    ) {
        let dir = if clockwise { 1.0 } else { -1.0 };
        for (i, f, rx, ry) in self.disc_offsets(c, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            // Screen space is y-down, so a positive angle here
            // turns visually clockwise.
            let a = dir * f * pressure * 0.35;
            let (s, co) = (a.sin(), a.cos());
            let (nx, ny) = (rx * co - ry * s, rx * s + ry * co);
            self.disp[i].0 += nx - rx;
            self.disp[i].1 += ny - ry;
        }
    }

    /// Pucker: pull sample points toward the center.
    pub fn stamp_pucker(&mut self, c: (f32, f32), radius: f32, pressure: f32, frozen: &[u8]) {
        for (i, f, rx, ry) in self.disc_offsets(c, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            let dist = (rx * rx + ry * ry).sqrt();
            if dist < 0.001 {
                continue;
            }
            let amt = f * pressure * radius * 0.1 / dist;
            self.disp[i].0 -= rx * amt;
            self.disp[i].1 -= ry * amt;
        }
    }

    /// Bloat: push sample points away from the center.
    pub fn stamp_bloat(&mut self, c: (f32, f32), radius: f32, pressure: f32, frozen: &[u8]) {
        for (i, f, rx, ry) in self.disc_offsets(c, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            let dist = (rx * rx + ry * ry).sqrt();
            if dist < 0.001 {
                continue;
            }
            let amt = f * pressure * radius * 0.1 / dist;
            self.disp[i].0 += rx * amt;
            self.disp[i].1 += ry * amt;
        }
    }

    /// Shift Pixels: move sample points perpendicular to the
    /// stroke, to the left of the direction of travel.
    pub fn stamp_shift(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        frozen: &[u8],
    ) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 {
            return;
        }
        // Left of travel in screen space (y down).
        let (lx, ly) = (dy / len, -dx / len);
        let amt = len * 0.5 * pressure;
        for (i, f, _, _, _, _) in self.stroke_offsets(from, to, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            self.disp[i].0 += lx * amt * f;
            self.disp[i].1 += ly * amt * f;
        }
    }

    /// Reflection: mirror sample points across the stroke line,
    /// pulled toward their mirrored position by the falloff.
    pub fn stamp_reflect(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        frozen: &[u8],
    ) {
        for (i, f, px, py, qx, qy) in self.stroke_offsets(from, to, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            // The mirrored point is 2q - p, where q is the
            // pixel's projection on the stroke segment.
            let (mx, my) = (2.0 * qx - px, 2.0 * qy - py);
            let k = f * pressure;
            self.disp[i].0 += (mx - px) * k;
            self.disp[i].1 += (my - py) * k;
        }
    }

    /// Reconstruct: scale every non-frozen displacement toward
    /// zero. Modes follow the 6.0 dialog's reconstruct modes:
    /// 0 Rigid (keep the mesh as-is), 1 Stiff (quarter), 2
    /// Smooth (half), 3 Loose (full restore of unfrozen areas).
    pub fn reconstruct(&mut self, mode: u8, frozen: &[u8]) {
        let keep = match mode {
            0 => return,
            1 => 0.25,
            2 => 0.5,
            _ => 0.0,
        };
        for i in 0..self.disp.len() {
            if is_frozen(frozen, i) {
                continue;
            }
            self.disp[i].0 *= keep;
            self.disp[i].1 *= keep;
        }
    }

    /// Restore the whole mesh to identity, frozen or not.
    pub fn restore_all(&mut self) {
        for d in self.disp.iter_mut() {
            *d = (0.0, 0.0);
        }
    }
}

/// Apply a Liquify field to a layer: each document pixel in
/// the field's bounds is resampled from (x + dx, y + dy) with
/// bilinear interpolation over the layer's current pixels.
/// Only pixels inside the selection (when one is given) are
/// written; feathered edges blend by the selection value. The
/// layer's own get/set_pixel handle its offset, so sampling
/// and writing both happen in document space.
pub fn apply_liquify(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    field: &LiquifyField,
) {
    if layer >= doc.layers.len() {
        return;
    }
    let w = doc.width.min(field.w);
    let h = doc.height.min(field.h);
    if w == 0 || h == 0 {
        return;
    }
    // Snapshot the source first: writes must not feed back
    // into samples taken later in the pass.
    let mut src: Vec<Pixel16> = Vec::with_capacity((w * h) as usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            src.push(doc.layers[layer].get_pixel(x, y));
        }
    }
    let at = |x: i32, y: i32| -> (f32, f32, f32, f32) {
        let cx = x.clamp(0, w as i32 - 1);
        let cy = y.clamp(0, h as i32 - 1);
        src[(cy as u32 * w + cx as u32) as usize].to_f32s()
    };
    let sample = |fx: f32, fy: f32| -> (f32, f32, f32, f32) {
        let x0 = fx.floor() as i32;
        let y0 = fy.floor() as i32;
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let (a, b, c, d) = (
            at(x0, y0),
            at(x0 + 1, y0),
            at(x0, y0 + 1),
            at(x0 + 1, y0 + 1),
        );
        let mix = |u: f32, v: f32, s: f32, t: f32| {
            u + (v - u) * tx + (s - u) * ty + (u - v - s + t) * tx * ty
        };
        (
            mix(a.0, b.0, c.0, d.0),
            mix(a.1, b.1, c.1, d.1),
            mix(a.2, b.2, c.2, d.2),
            mix(a.3, b.3, c.3, d.3),
        )
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as u32 * field.w + x as u32) as usize;
            let d = field.disp[i];
            if d == (0.0, 0.0) {
                continue;
            }
            let sv = match sel {
                Some(s) => s.value(x, y),
                None => 255,
            };
            if sv == 0 {
                continue;
            }
            let smp = sample(x as f32 + d.0, y as f32 + d.1);
            let out = if sv == 255 {
                Pixel16::from_f32s(smp.0, smp.1, smp.2, smp.3)
            } else {
                let k = sv as f32 / 255.0;
                let o = src[(y as u32 * w + x as u32) as usize].to_f32s();
                Pixel16::from_f32s(
                    o.0 + (smp.0 - o.0) * k,
                    o.1 + (smp.1 - o.1) * k,
                    o.2 + (smp.2 - o.2) * k,
                    o.3 + (smp.3 - o.3) * k,
                )
            };
            doc.layers[layer].set_pixel(x, y, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_starts_identity() {
        let f = LiquifyField::new(8, 8);
        assert!(f.is_identity());
        assert_eq!(f.disp.len(), 64);
    }

    #[test]
    fn warp_moves_the_center_and_leaves_the_corners() {
        let mut f = LiquifyField::new(21, 21);
        f.stamp_warp((10.0, 10.0), (14.0, 10.0), 6.0, 1.0, &[]);
        assert!(!f.is_identity());
        let center = f.disp[f.idx(10, 10)];
        assert!(center.0 > 1.0 && center.1.abs() < 0.001);
        assert_eq!(f.disp[f.idx(0, 0)], (0.0, 0.0));
        assert_eq!(f.disp[f.idx(20, 20)], (0.0, 0.0));
    }

    #[test]
    fn pucker_and_bloat_pull_opposite_ways() {
        let mut pk = LiquifyField::new(21, 21);
        pk.stamp_pucker((10.0, 10.0), 8.0, 1.0, &[]);
        let mut bl = LiquifyField::new(21, 21);
        bl.stamp_bloat((10.0, 10.0), 8.0, 1.0, &[]);
        // A pixel right of center: pucker displaces it left
        // (toward center), bloat right (away).
        let p = pk.disp[pk.idx(14, 10)];
        let b = bl.disp[bl.idx(14, 10)];
        assert!(p.0 < 0.0);
        assert!(b.0 > 0.0);
        assert!((p.0 + b.0).abs() < 0.001);
    }

    #[test]
    fn frozen_pixels_never_change() {
        let mut f = LiquifyField::new(21, 21);
        let mut frozen = vec![0u8; 21 * 21];
        let ci = f.idx(10, 10);
        frozen[ci] = 255;
        f.stamp_warp((10.0, 10.0), (16.0, 10.0), 6.0, 1.0, &frozen);
        assert_eq!(f.disp[ci], (0.0, 0.0));
        // Reconstruct must also respect the freeze.
        f.disp[ci] = (3.0, 0.0);
        f.reconstruct(3, &frozen);
        assert_eq!(f.disp[ci], (3.0, 0.0));
        assert!(f
            .disp
            .iter()
            .enumerate()
            .all(|(i, d)| i == ci || *d == (0.0, 0.0)));
    }

    #[test]
    fn reconstruct_loose_zeroes_and_restore_all_zeroes() {
        let mut f = LiquifyField::new(11, 11);
        f.stamp_bloat((5.0, 5.0), 4.0, 1.0, &[]);
        assert!(!f.is_identity());
        f.reconstruct(3, &[]);
        assert!(f.is_identity());
        f.stamp_pucker((5.0, 5.0), 4.0, 1.0, &[]);
        f.restore_all();
        assert!(f.is_identity());
    }

    #[test]
    fn twirl_and_shift_and_reflect_displace() {
        let mut f = LiquifyField::new(21, 21);
        f.stamp_twirl((10.0, 10.0), 8.0, 1.0, true, &[]);
        assert!(!f.is_identity());
        let mut g = LiquifyField::new(21, 21);
        g.stamp_shift((4.0, 10.0), (16.0, 10.0), 5.0, 1.0, &[]);
        // Traveling right, shift pushes sample points up
        // (left of travel in screen space).
        assert!(g.disp[g.idx(10, 10)].1 < 0.0);
        let mut r = LiquifyField::new(21, 21);
        r.stamp_reflect((4.0, 10.0), (16.0, 10.0), 5.0, 1.0, &[]);
        // Below the line, reflection displaces upward toward
        // the mirrored position; above it, downward.
        assert!(r.disp[r.idx(10, 13)].1 < 0.0);
        assert!(r.disp[r.idx(10, 7)].1 > 0.0);
    }

    #[test]
    fn apply_shifts_a_painted_block() {
        let mut doc = Document::new(16, 16);
        let red = Pixel16::from_rgba8(220, 20, 20, 255);
        doc.layers[0].fill_rect(2, 2, 4, 4, red);
        // Every sample pulled 2 px left: the block's image
        // lands 2 px to the right.
        let mut f = LiquifyField::new(16, 16);
        for d in f.disp.iter_mut() {
            *d = (-2.0, 0.0);
        }
        apply_liquify(&mut doc, 0, None, &f);
        assert_eq!(doc.layers[0].get_pixel(7, 3).to_rgba8().0, 220);
        assert_eq!(doc.layers[0].get_pixel(3, 3).to_rgba8().3, 0);
    }

    #[test]
    fn apply_respects_the_selection() {
        let mut doc = Document::new(16, 16);
        let red = Pixel16::from_rgba8(220, 20, 20, 255);
        doc.layers[0].fill_rect(2, 2, 4, 4, red);
        let mut f = LiquifyField::new(16, 16);
        for d in f.disp.iter_mut() {
            *d = (-4.0, 0.0);
        }
        // Select only the right half: the left half must not
        // be written even though the field displaces it.
        let sel = Selection::rect(16, 16, 8, 0, 8, 16);
        apply_liquify(&mut doc, 0, Some(&sel), &f);
        assert_eq!(doc.layers[0].get_pixel(3, 3).to_rgba8().0, 220);
        assert_eq!(doc.layers[0].get_pixel(7, 3).to_rgba8().3, 0);
        // Inside the selection the shift did land: (8, 3)
        // sampled the block at (4, 3).
        assert_eq!(doc.layers[0].get_pixel(8, 3).to_rgba8().0, 220);
    }
}

/// Smooth value noise in [0, 1] over a hashed integer lattice.
fn noise2(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (xf, yf) = (x - xi, y - yi);
    let h = |ix: i32, iy: i32| -> f32 {
        let mut n = (ix as u32).wrapping_mul(374_761_393) ^ (iy as u32).wrapping_mul(668_265_263);
        n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
        ((n ^ (n >> 16)) & 0xFFFF) as f32 / 65535.0
    };
    let (u, v) = (xf * xf * (3.0 - 2.0 * xf), yf * yf * (3.0 - 2.0 * yf));
    let (a, b) = (h(xi as i32, yi as i32), h(xi as i32 + 1, yi as i32));
    let (c, dd) = (h(xi as i32, yi as i32 + 1), h(xi as i32 + 1, yi as i32 + 1));
    a + (b - a) * u + (c - a) * v + (a - b - c + dd) * u * v
}

impl LiquifyField {
    /// Turbulence (7.0): displace sample points along a smooth
    /// pseudo-random flow field — the smoky, marbled distortion.
    pub fn stamp_turbulence(
        &mut self,
        c: (f32, f32),
        radius: f32,
        pressure: f32,
        frozen: &[u8],
        seed: u32,
    ) {
        let s = seed as f32;
        for (i, f, rx, ry) in self.disc_offsets(c, radius) {
            if is_frozen(frozen, i) {
                continue;
            }
            let (px, py) = (c.0 + rx, c.1 + ry);
            let nx = noise2(px * 0.045 + s * 13.7, py * 0.045 + 3.1) - 0.5;
            let ny = noise2(px * 0.045 + 71.3, py * 0.045 + s * 7.9) - 0.5;
            self.disp[i].0 += nx * f * pressure * radius * 0.5;
            self.disp[i].1 += ny * f * pressure * radius * 0.5;
        }
    }
}

/// Serialize a mesh: "PDMESH1" + w + h + f32 LE pairs.
pub fn field_to_bytes(field: &LiquifyField) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + field.disp.len() * 8);
    out.extend_from_slice(b"PDMESH1\0");
    out.extend_from_slice(&field.w.to_le_bytes());
    out.extend_from_slice(&field.h.to_le_bytes());
    for (dx, dy) in &field.disp {
        out.extend_from_slice(&dx.to_le_bytes());
        out.extend_from_slice(&dy.to_le_bytes());
    }
    out
}

/// Parse a mesh written by `field_to_bytes`.
pub fn field_from_bytes(bytes: &[u8]) -> Option<LiquifyField> {
    if bytes.len() < 16 || &bytes[0..8] != b"PDMESH1\0" {
        return None;
    }
    let w = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let h = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
    let n = (w as usize).checked_mul(h as usize)?;
    if bytes.len() != 16 + n * 8 || n > 268_435_456 {
        return None;
    }
    let mut disp = Vec::with_capacity(n);
    for i in 0..n {
        let o = 16 + i * 8;
        disp.push((
            f32::from_le_bytes(bytes[o..o + 4].try_into().ok()?),
            f32::from_le_bytes(bytes[o + 4..o + 8].try_into().ok()?),
        ));
    }
    Some(LiquifyField { w, h, disp })
}
