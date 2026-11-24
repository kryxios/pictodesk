//! Text Warp math (Photoshop 6.0's Warp Text): deforms a
//! rectangular raster region — in practice a rendered type
//! layer's pixels — with one of fifteen named styles, a Bend
//! amount, and Horizontal / Vertical Distortion.
//!
//! The warp is computed by inverse mapping: for each output
//! pixel we compute the source position the style's deformation
//! pulls it from, then bilinear-sample the source there.
//! Samples that fall outside the source region are transparent,
//! so warped text can vacate corners and swing past its old
//! edges cleanly (within the region — callers pass the layer's
//! painted bounds, so extreme bends clip at those bounds, the
//! same trade the era tool made with its text box).
//!
//! Coordinates: normalized `u = (x + 0.5) / w` and
//! `v = (y + 0.5) / h`, both in `0..1`, with centered forms
//! `cx = u - 0.5` and `cy = v - 0.5`. `b = bend / 100` in
//! `-1..=1`. Positive bend arcs text upward. Every style is a
//! smooth deformation and reduces to the exact identity at
//! `b == 0` (and `warp_region` short-circuits the all-zero case
//! so identity is bit-exact, not just mathematically exact).
//!
//! Distortion is applied uniformly AFTER the style mapping,
//! as a shear pair computed from the style-mapped position
//! (both components use the pre-distortion pair, so the two
//! shears never feed each other):
//! `su += (h_dist / 100) * (sv - 0.5) * 0.6` and
//! `sv += (v_dist / 100) * (su - 0.5) * 0.6`.

use crate::document::Document;
use crate::pixel::Pixel16;
use crate::transform::layer_painted_bounds;

/// The fifteen Warp Text styles, in the era dialog's order.
/// `warp_region`'s `style` parameter indexes this table; an
/// out-of-range style is an identity copy.
pub const WARP_STYLES: [&str; 15] = [
    "Arc",
    "Arc Lower",
    "Arc Upper",
    "Arch",
    "Bulge",
    "Shell Lower",
    "Shell Upper",
    "Flag",
    "Wave",
    "Fish",
    "Rise",
    "Fisheye",
    "Inflate",
    "Squeeze",
    "Twist",
];

/// Smoothstep on `0..1` (clamped), for weighting a deformation
/// toward one end of the region.
fn smooth01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bilinear-sample `src` (RGBA8, row-major `w` × `h`) at the
/// fractional pixel position `(fx, fy)`. Out-of-bounds taps
/// contribute transparent black, so edges fade to transparent
/// instead of smearing.
fn sample_bilinear(src: &[(u8, u8, u8, u8)], w: u32, h: u32, fx: f32, fy: f32) -> (u8, u8, u8, u8) {
    let get = |x: i32, y: i32| -> (f32, f32, f32, f32) {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        match src.get((y as u32 * w + x as u32) as usize) {
            Some(p) => (p.0 as f32, p.1 as f32, p.2 as f32, p.3 as f32),
            None => (0.0, 0.0, 0.0, 0.0),
        }
    };
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let c00 = get(x0, y0);
    let c10 = get(x0 + 1, y0);
    let c01 = get(x0, y0 + 1);
    let c11 = get(x0 + 1, y0 + 1);
    let mix = |a: f32, b: f32, c: f32, d: f32| -> u8 {
        let v =
            a * (1.0 - tx) * (1.0 - ty) + b * tx * (1.0 - ty) + c * (1.0 - tx) * ty + d * tx * ty;
        v.round().clamp(0.0, 255.0) as u8
    };
    (
        mix(c00.0, c10.0, c01.0, c11.0),
        mix(c00.1, c10.1, c01.1, c11.1),
        mix(c00.2, c10.2, c01.2, c11.2),
        mix(c00.3, c10.3, c01.3, c11.3),
    )
}

/// The style mapping: given a normalized output position
/// `(u, v)`, return the normalized source position `(su, sv)`
/// to sample. `b` is the bend in `-1..=1`.
fn style_source(style: u8, u: f32, v: f32, b: f32) -> (f32, f32) {
    let cx = u - 0.5;
    let cy = v - 0.5;
    // Parabolic arc profile: 0 at the left/right edges, max
    // 0.25 at the horizontal center.
    let arc = 0.25 - cx * cx;
    match style {
        // Arc: the whole block bends; the top edge arcs up in
        // the middle (source is sampled lower there, so the
        // content rises).
        0 => (u, v + b * arc),
        // Arc Lower: the arc grows toward the bottom — the
        // lower edge swings most, the top stays near-straight.
        1 => (u, v + b * arc * smooth01(v)),
        // Arc Upper: mirror of Arc Lower, weighted to the top.
        2 => (u, v + b * arc * smooth01(1.0 - v)),
        // Arch: a stronger arc plus a slight horizontal pinch
        // toward the center where the arc peaks.
        3 => (
            0.5 + cx * (1.0 - b.abs() * 0.3 * (arc * 4.0)),
            v + b * arc * 1.8,
        ),
        // Bulge: vertical scale compresses (in source space)
        // near the horizontal center, so content there reads
        // stretched taller — a vertical bulge.
        4 => {
            let f = 1.0 - b * 0.5 * (1.0 - (2.0 * cx).powi(2)).clamp(0.0, 1.0);
            (u, 0.5 + cy * f)
        }
        // Shell Lower: the fan opens toward the bottom — source
        // space compresses horizontally more and more lower
        // down, so content spreads wider toward the base.
        5 => (0.5 + cx * (1.0 - b * 0.45 * smooth01(v)), v),
        // Shell Upper: the fan opens toward the top.
        6 => (0.5 + cx * (1.0 - b * 0.45 * smooth01(1.0 - v)), v),
        // Flag: a tilt that grows along +u (one side lifts)
        // with a slight single sine ripple over it.
        7 => (
            u,
            v + b * (u * 0.3 + (u * std::f32::consts::PI).sin() * 0.08),
        ),
        // Wave: one full sine period across the width, shifting
        // rows up then down.
        8 => (u, v + b * (u * std::f32::consts::TAU).sin() * 0.12),
        // Fish: horizontal swell — source positions push away
        // from the center on a sine envelope, swelling the
        // middle of the block sideways.
        9 => (u + b * (u * std::f32::consts::PI).sin() * cx * 0.6, v),
        // Rise: a diagonal shear — sampling drops as u grows
        // and rises as v grows, so lines climb to the right.
        10 => (u, v + b * (u - v) * 0.2),
        // Fisheye: radial center magnification. The source is
        // sampled through a scale that shrinks with the radius
        // (linear in r), so the center is magnified most and
        // the corners are nearly untouched.
        11 | 12 => {
            let (nx, ny) = (2.0 * cx, 2.0 * cy);
            let r2 = ((nx * nx + ny * ny) / 2.0).clamp(0.0, 1.0);
            let scale = if style == 11 {
                1.0 - b * 0.5 * (1.0 - r2.sqrt())
            } else {
                // Inflate: a broader, gentler radial expansion —
                // the falloff is quadratic in r, so a wide
                // central area expands almost uniformly instead
                // of the Fisheye's tight center magnification.
                1.0 - b * 0.35 * (1.0 - r2)
            };
            (0.5 + cx * scale, 0.5 + cy * scale)
        }
        // Squeeze: horizontal pinch at the vertical center —
        // source space compresses horizontally near the middle
        // row band, squeezing content inward there.
        13 => {
            let f = 1.0 - b * 0.5 * (1.0 - (2.0 * cy).powi(2)).clamp(0.0, 1.0);
            (0.5 + cx * f, v)
        }
        // Twist: rotate the source position around the center
        // by an angle that fades with radius — the middle of
        // the block turns, the corners stay put. (Rotation is
        // in normalized space, so on non-square regions the
        // twist is slightly elliptical — an accepted
        // approximation, invisible at text-warp bend levels.)
        _ => {
            let (nx, ny) = (2.0 * cx, 2.0 * cy);
            let r = (nx * nx + ny * ny).sqrt().clamp(0.0, 1.0);
            let theta = b * 0.8 * (1.0 - r);
            let (s, c) = theta.sin_cos();
            (0.5 + cx * c - cy * s, 0.5 + cx * s + cy * c)
        }
    }
}

/// Warp an RGBA8 region (`src`, row-major `w` × `h`) with the
/// Warp Text `style` (an index into [`WARP_STYLES`]), `bend`
/// in `-100..=100` percent, and `h_dist` / `v_dist` distortion
/// in `-100..=100`. Returns a new `w` × `h` RGBA8 buffer.
///
/// Bend 0 with both distortions 0 is an exact copy for every
/// style, and an out-of-range `style` is always an exact copy.
pub fn warp_region(
    src: &[(u8, u8, u8, u8)],
    w: u32,
    h: u32,
    style: u8,
    bend: f32,
    h_dist: f32,
    v_dist: f32,
) -> Vec<(u8, u8, u8, u8)> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    if (style as usize) >= WARP_STYLES.len() || (bend == 0.0 && h_dist == 0.0 && v_dist == 0.0) {
        // Identity: return the source verbatim (padding with
        // transparency if the caller handed a short buffer).
        let mut out = vec![(0u8, 0u8, 0u8, 0u8); (w * h) as usize];
        let n = out.len().min(src.len());
        out[..n].copy_from_slice(&src[..n]);
        return out;
    }
    let b = (bend / 100.0).clamp(-1.0, 1.0);
    let hd = (h_dist / 100.0).clamp(-1.0, 1.0);
    let vd = (v_dist / 100.0).clamp(-1.0, 1.0);
    let (wf, hf) = (w as f32, h as f32);
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let v = (y as f32 + 0.5) / hf;
        for x in 0..w {
            let u = (x as f32 + 0.5) / wf;
            let (mut su, mut sv) = style_source(style, u, v, b);
            // Distortion shears, both computed from the
            // style-mapped pair before either is applied.
            let du = hd * (sv - 0.5) * 0.6;
            let dv = vd * (su - 0.5) * 0.6;
            su += du;
            sv += dv;
            out.push(sample_bilinear(src, w, h, su * wf - 0.5, sv * hf - 0.5));
        }
    }
    out
}

/// Warp one layer's painted region in place with
/// [`warp_region`]. The region is the layer's painted bounds;
/// it is cleared to transparent first (the warp may vacate
/// pixels) and then the warped raster is written back over
/// the same rectangle. A layer with no painted pixels, or an
/// out-of-range layer index, is left untouched.
///
/// Pixel access goes through `Layer::get_pixel` /
/// `set_pixel`, which work in document coordinates and account
/// for the layer offset themselves; the bounds from
/// `layer_painted_bounds` are document-space exclusive corners
/// `(x0, y0, x1, y1)`, so the region size is `(x1 - x0)` by
/// `(y1 - y0)`.
pub fn warp_layer(
    doc: &mut Document,
    layer: usize,
    style: u8,
    bend: f32,
    h_dist: f32,
    v_dist: f32,
) {
    if layer >= doc.layers.len() {
        return;
    }
    let Some((bx0, by0, bx1, by1)) = layer_painted_bounds(&doc.layers[layer]) else {
        return;
    };
    let (rw, rh) = ((bx1 - bx0).max(0) as u32, (by1 - by0).max(0) as u32);
    if rw == 0 || rh == 0 {
        return;
    }
    // Read the region out before touching the layer.
    let mut src = Vec::with_capacity((rw * rh) as usize);
    for yy in 0..rh as i32 {
        for xx in 0..rw as i32 {
            src.push(doc.layers[layer].get_pixel(bx0 + xx, by0 + yy).to_rgba8());
        }
    }
    let warped = warp_region(&src, rw, rh, style, bend, h_dist, v_dist);
    // Clear, then write back: vacated pixels must end
    // transparent, not smeared with the pre-warp content.
    let layer_ref = &mut doc.layers[layer];
    for yy in 0..rh as i32 {
        for xx in 0..rw as i32 {
            layer_ref.set_pixel(bx0 + xx, by0 + yy, Pixel16::TRANSPARENT);
        }
    }
    for (i, p) in warped.iter().enumerate() {
        if p.3 == 0 && *p == (0, 0, 0, 0) {
            continue; // already transparent from the clear
        }
        let xx = (i as u32 % rw) as i32;
        let yy = (i as u32 / rw) as i32;
        layer_ref.set_pixel(bx0 + xx, by0 + yy, Pixel16::from_rgba8(p.0, p.1, p.2, p.3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16×12 gradient: R = x * 16, G = y * 16, opaque.
    fn gradient(w: u32, h: u32) -> Vec<(u8, u8, u8, u8)> {
        let mut out = Vec::new();
        for y in 0..h {
            for x in 0..w {
                out.push(((x * 16) as u8, (y * 16) as u8, 128, 255));
            }
        }
        out
    }

    #[test]
    fn bend_zero_is_exact_identity_for_all_styles() {
        let src = gradient(16, 12);
        for style in 0..WARP_STYLES.len() as u8 {
            let out = warp_region(&src, 16, 12, style, 0.0, 0.0, 0.0);
            assert_eq!(out.len(), src.len(), "style {style} length");
            assert_eq!(
                out, src,
                "style {} must be identity",
                WARP_STYLES[style as usize]
            );
        }
    }

    #[test]
    fn out_of_range_style_is_identity() {
        let src = gradient(8, 8);
        let out = warp_region(&src, 8, 8, 99, 60.0, 20.0, 20.0);
        assert_eq!(out, src);
    }

    #[test]
    fn arc_bend_moves_the_middle_up() {
        let src = gradient(16, 12);
        let out = warp_region(&src, 16, 12, 0, 50.0, 0.0, 0.0);
        assert_ne!(out, src, "Arc at bend 50 must change the raster");
        // Middle column, top row: Arc samples from LOWER in the
        // source there (the content arcs up), so the green
        // channel (which encodes source y) must be larger than
        // the unwarped top row's.
        let mid_top = out[8];
        assert!(
            mid_top.1 > src[8].1,
            "arc middle-top should sample lower source rows: {mid_top:?} vs {:?}",
            src[8]
        );
        // Corners stay in-bounds and opaque: their arc shift
        // is small and the samples land inside the source, so
        // no transparent bleed appears at the top corners.
        assert_eq!(out[0].3, 255);
        assert_eq!(out[15].3, 255);
    }

    #[test]
    fn twist_and_distortion_change_the_raster() {
        let src = gradient(16, 12);
        let twist = warp_region(&src, 16, 12, 14, 60.0, 0.0, 0.0);
        assert_eq!(twist.len(), src.len());
        assert_ne!(twist, src, "Twist at bend 60 must change the raster");
        let dist = warp_region(&src, 16, 12, 0, 0.0, 50.0, -50.0);
        assert_ne!(dist, src, "distortion alone must change the raster");
    }

    #[test]
    fn every_style_at_full_bend_stays_well_formed() {
        let src = gradient(16, 12);
        for style in 0..WARP_STYLES.len() as u8 {
            let out = warp_region(&src, 16, 12, style, 100.0, 30.0, -30.0);
            assert_eq!(out.len(), src.len(), "style {style} length");
        }
    }

    #[test]
    fn warp_layer_moves_pixels_and_keeps_bounds() {
        let mut doc = Document::new(24, 24);
        doc.layers[0].fill_rect(4, 8, 16, 8, Pixel16::from_rgba8(200, 40, 40, 255));
        let before = doc.layers[0].get_pixel(12, 15);
        assert_ne!(before, Pixel16::TRANSPARENT);
        warp_layer(&mut doc, 0, 0, 80.0, 0.0, 0.0);
        // The strong upward arc pulls content up: the block's
        // bottom-middle pixel is vacated to transparent (its
        // source position falls below the region), while the
        // layer as a whole still has paint.
        let after = doc.layers[0].get_pixel(12, 15);
        assert!(layer_painted_bounds(&doc.layers[0]).is_some());
        assert_eq!(after, Pixel16::TRANSPARENT);
        // Out-of-range layer is a no-op, not a panic.
        warp_layer(&mut doc, 99, 0, 80.0, 0.0, 0.0);
    }
}
