//! Healing (Phase 7): texture transfer. The healed pixel
//! takes the source's detail (its high-frequency texture)
//! added onto the destination's lighting (its low-frequency
//! shading), so a repair borrows texture without importing
//! the source's brightness or color cast.
//!
//! healed = source − blur(source) + blur(destination),
//! computed per channel in 16-bit space. Alpha comes from
//! the destination; fully transparent destinations are left
//! alone. RGB and Grayscale documents (the app routes other
//! modes away — named in the phase delivery).

use crate::brush::BrushTip;
use crate::document::Document;
use crate::pixel::Pixel16;
use crate::selection::Selection;

type Grid = Vec<[f32; 3]>;

fn read_grid(doc: &Document, layer: usize, x0: i32, y0: i32, w: i32, h: i32) -> Grid {
    let l = &doc.layers[layer];
    let mut g = Vec::with_capacity((w * h) as usize);
    for yy in 0..h {
        for xx in 0..w {
            let p = l.get_pixel(x0 + xx, y0 + yy);
            g.push([p.r as f32, p.g as f32, p.b as f32]);
        }
    }
    g
}

/// Separable box blur over an RGB grid (clamped edges).
fn box_blur(src: &Grid, w: i32, h: i32, radius: i32) -> Grid {
    if radius <= 0 || w <= 0 || h <= 0 {
        return src.clone();
    }
    let (w, h) = (w as usize, h as usize);
    let r = radius;
    let mut tmp = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 3];
            let mut n = 0f32;
            for k in -r..=r {
                let sx = (x as i32 + k).clamp(0, w as i32 - 1) as usize;
                let v = src[y * w + sx];
                acc[0] += v[0];
                acc[1] += v[1];
                acc[2] += v[2];
                n += 1.0;
            }
            tmp[y * w + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
        }
    }
    let mut out = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 3];
            let mut n = 0f32;
            for k in -r..=r {
                let sy = (y as i32 + k).clamp(0, h as i32 - 1) as usize;
                let v = tmp[sy * w + x];
                acc[0] += v[0];
                acc[1] += v[1];
                acc[2] += v[2];
                n += 1.0;
            }
            out[y * w + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
        }
    }
    out
}

fn clamp16(v: f32) -> u16 {
    v.clamp(0.0, 65535.0) as u16
}

/// One Healing Brush stamp. `src_c` is the sampled center
/// (document coords); the stamp lands at (cx, cy). Returns
/// the touched bbox.
#[allow(clippy::too_many_arguments)]
pub fn stamp_heal(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    src_c: (f32, f32),
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0 || h <= 0 {
        return (x0, y0, x1, y1);
    }
    let (dx, dy) = ((src_c.0 - cx).round() as i32, (src_c.1 - cy).round() as i32);
    let src = read_grid(doc, layer, x0 + dx, y0 + dy, w, h);
    heal_with_src_grid(doc, layer, sel, cx, cy, tip, opacity, src, (x0, y0, x1, y1))
}

/// Healing Brush with Source = Pattern: the texture comes
/// from the document pattern tiled over document coords.
#[allow(clippy::too_many_arguments)]
pub fn stamp_heal_pattern(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0 || h <= 0 {
        return (x0, y0, x1, y1);
    }
    let Some(pat) = doc.pattern.clone() else {
        return (x0, y0, x1, y1);
    };
    let mut src: Grid = Vec::with_capacity((w * h) as usize);
    for yy in 0..h {
        for xx in 0..w {
            let tx = (x0 + xx).rem_euclid(pat.width as i32) as usize;
            let ty = (y0 + yy).rem_euclid(pat.height as i32) as usize;
            let p = pat.px[ty * pat.width as usize + tx];
            src.push([p.r as f32, p.g as f32, p.b as f32]);
        }
    }
    heal_with_src_grid(doc, layer, sel, cx, cy, tip, opacity, src, (x0, y0, x1, y1))
}

#[allow(clippy::too_many_arguments)]
fn heal_with_src_grid(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    opacity: f32,
    src: Grid,
    bbox: (i32, i32, i32, i32),
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = bbox;
    let (w, h) = (x1 - x0, y1 - y0);
    let radius = ((tip.size / 2.0) as i32).clamp(1, 32);
    let src_blur = box_blur(&src, w, h, radius);
    // Padded destination grid: the blur kernel must see past
    // the stamp's own footprint.
    let (pw, ph) = (w + radius * 2, h + radius * 2);
    let dst_pad = read_grid(doc, layer, x0 - radius, y0 - radius, pw, ph);
    let dst_blur_pad = box_blur(&dst_pad, pw, ph, radius);
    let mut dst_blur: Grid = Vec::with_capacity((w * h) as usize);
    for yy in 0..h {
        for xx in 0..w {
            dst_blur.push(dst_blur_pad[((yy + radius) * pw + (xx + radius)) as usize]);
        }
    }
    for yy in 0..h {
        for xx in 0..w {
            let (px, py) = (x0 + xx, y0 + yy);
            let d = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt();
            let mut st = tip.strength(px, py, cx, cy, d) * opacity;
            if let Some(s) = sel {
                st *= s.value(px, py) as f32 / 255.0;
            }
            if st <= 0.0 {
                continue;
            }
            let cur = doc.layers[layer].get_pixel(px, py);
            if cur.a == 0 {
                continue;
            }
            let i = (yy * w + xx) as usize;
            let healed = Pixel16 {
                r: clamp16(src[i][0] - src_blur[i][0] + dst_blur[i][0]),
                g: clamp16(src[i][1] - src_blur[i][1] + dst_blur[i][1]),
                b: clamp16(src[i][2] - src_blur[i][2] + dst_blur[i][2]),
                a: cur.a,
            };
            let mix = |a: u16, b: u16| -> u16 {
                (a as f32 * (1.0 - st) + b as f32 * st).clamp(0.0, 65535.0) as u16
            };
            doc.layers[layer].set_pixel(
                px,
                py,
                Pixel16 {
                    r: mix(cur.r, healed.r),
                    g: mix(cur.g, healed.g),
                    b: mix(cur.b, healed.b),
                    a: cur.a,
                },
            );
        }
    }
    (x0, y0, x1, y1)
}

/// Patch tool: heal the whole selection at once, sampling
/// from the selection shifted by (dx, dy) (where the user
/// dragged the selection to). The selection's own values
/// feather the blend.
pub fn heal_patch(doc: &mut Document, layer: usize, sel: &Selection, dx: i32, dy: i32) {
    let Some((x0, y0, w, h)) = sel.bounds() else {
        return;
    };
    let (w, h) = (w as i32, h as i32);
    if w <= 0 || h <= 0 {
        return;
    }
    let radius = (w.max(h) / 2).clamp(1, 48);
    let src = read_grid(doc, layer, x0 + dx, y0 + dy, w, h);
    let src_blur = box_blur(&src, w, h, radius);
    let (pw, ph) = (w + radius * 2, h + radius * 2);
    let dst_pad = read_grid(doc, layer, x0 - radius, y0 - radius, pw, ph);
    let dst_blur_pad = box_blur(&dst_pad, pw, ph, radius);
    let mut dst_blur: Grid = Vec::with_capacity((w * h) as usize);
    for yy in 0..h {
        for xx in 0..w {
            dst_blur.push(dst_blur_pad[((yy + radius) * pw + (xx + radius)) as usize]);
        }
    }
    for yy in 0..h {
        for xx in 0..w {
            let (px, py) = (x0 + xx, y0 + yy);
            let st = sel.value(px, py) as f32 / 255.0;
            if st <= 0.0 {
                continue;
            }
            let cur = doc.layers[layer].get_pixel(px, py);
            if cur.a == 0 {
                continue;
            }
            let i = (yy * w + xx) as usize;
            let healed = Pixel16 {
                r: clamp16(src[i][0] - src_blur[i][0] + dst_blur[i][0]),
                g: clamp16(src[i][1] - src_blur[i][1] + dst_blur[i][1]),
                b: clamp16(src[i][2] - src_blur[i][2] + dst_blur[i][2]),
                a: cur.a,
            };
            let mix = |a: u16, b: u16| -> u16 {
                (a as f32 * (1.0 - st) + b as f32 * st).clamp(0.0, 65535.0) as u16
            };
            doc.layers[layer].set_pixel(
                px,
                py,
                Pixel16 {
                    r: mix(cur.r, healed.r),
                    g: mix(cur.g, healed.g),
                    b: mix(cur.b, healed.b),
                    a: cur.a,
                },
            );
        }
    }
}
