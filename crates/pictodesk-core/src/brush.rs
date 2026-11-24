//! Painting tools — the classic toolbox's pixel pushers:
//! Brush (soft, spaced stamps), Pencil (hard), Eraser, Clone Stamp
//! (Rubber Stamp in 1.0), Paint Bucket, Gradient, and the basic
//! Blur/Sharpen/Smudge stamp tools.
//!
//! Every stamp respects the selection mask. Strokes interpolate
//! stamps along the path (spacing = 25% of diameter, the classic
//! default) and return their bounding box so the caller can wrap
//! the stroke in a tile-snapshot history command.

use crate::color::rgb_to_cmyk;

/// A CMYK pixel sample: (c, m, y, k, alpha), each 0..=65535.
type CmykSample = (u16, u16, u16, u16, u16);
use crate::document::{ColorMode, Document};
use crate::pixel::Pixel16;
use crate::selection::Selection;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushTip {
    /// Diameter in pixels.
    pub size: f32,
    /// 0.0 = fully feathered edge, 1.0 = hard edge.
    pub hardness: f32,
}

impl BrushTip {
    pub fn new(size: f32, hardness: f32) -> Self {
        BrushTip {
            size: size.max(1.0),
            hardness: hardness.clamp(0.0, 1.0),
        }
    }

    /// Falloff at distance `d` from stamp center: 1 inside the hard
    /// radius, easing to 0 at the outer radius.
    fn falloff(&self, d: f32) -> f32 {
        let r = self.size / 2.0;
        if d >= r {
            return 0.0;
        }
        let hard_r = r * self.hardness;
        if d <= hard_r {
            return 1.0;
        }
        let t = (d - hard_r) / (r - hard_r);
        // smoothstep edge
        1.0 - t * t * (3.0 - 2.0 * t)
    }

    fn bbox(&self, cx: f32, cy: f32) -> (i32, i32, i32, i32) {
        let r = self.size / 2.0 + 1.0;
        (
            (cx - r).floor() as i32,
            (cy - r).floor() as i32,
            (cx + r).ceil() as i32,
            (cy + r).ceil() as i32,
        )
    }
}

fn sel_strength(sel: Option<&Selection>, x: i32, y: i32) -> f32 {
    match sel {
        None => 1.0,
        Some(s) => s.value(x, y) as f32 / 255.0,
    }
}

/// Source-over composite of `color` onto the layer pixel at strength.
/// In a CMYK document the (RGB) color is converted to inks and
/// deposited in ink space via over_ink.
fn over_pixel(doc: &mut Document, layer: usize, x: i32, y: i32, color: Pixel16, strength: f32) {
    if strength <= 0.0 {
        return;
    }
    if doc.mode == ColorMode::Cmyk {
        let (r, g, b, a) = color.to_f32s();
        let (ic, im, iy, ik) = rgb_to_cmyk(r, g, b);
        over_ink(doc, layer, x, y, (ic, im, iy, ik), a, strength);
        return;
    }
    let dst = doc.layers[layer].get_pixel(x, y).to_f32s();
    let src = color.to_f32s();
    let sa = src.3 * strength;
    let out_a = sa + dst.3 * (1.0 - sa);
    let comp = |sc: f32, dc: f32| -> f32 {
        if out_a <= 0.0 {
            0.0
        } else {
            (sc * sa + dc * dst.3 * (1.0 - sa)) / out_a
        }
    };
    doc.layers[layer].set_pixel(
        x,
        y,
        Pixel16::from_f32s(
            comp(src.0, dst.0),
            comp(src.1, dst.1),
            comp(src.2, dst.2),
            out_a,
        ),
    );
}

/// Deposit ink over a CMYK pixel: Porter-Duff over in ink space,
/// alpha tracked on the layer's alpha plane. `ink` components and
/// `src_a` are 0.0..=1.0.
fn over_ink(
    doc: &mut Document,
    layer: usize,
    x: i32,
    y: i32,
    ink: (f32, f32, f32, f32),
    src_a: f32,
    strength: f32,
) {
    let sa = src_a * strength;
    if sa <= 0.0 {
        return;
    }
    let (dc, dm, dy, dk, da16) = doc.get_cmyk(layer, x, y);
    let da = da16 as f32 / 65535.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    let comp = |sc: f32, dcv: u16| -> u16 {
        let d = dcv as f32 / 65535.0;
        (((sc * sa + d * da * (1.0 - sa)) / out_a).clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
    };
    doc.set_cmyk(
        layer,
        x,
        y,
        (
            comp(ink.0, dc),
            comp(ink.1, dm),
            comp(ink.2, dy),
            comp(ink.3, dk),
            (out_a * 65535.0 + 0.5) as u16,
        ),
    );
}

/// One Brush stamp. Returns the stamp bbox (x0, y0, x1, y1 exclusive).
#[allow(clippy::too_many_arguments)]
pub fn stamp_brush(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    color: Pixel16,
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let s = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
            over_pixel(doc, layer, xx, yy, color, s);
        }
    }
    (x0, y0, x1, y1)
}

/// One Pencil stamp: hard round/square-free circle, no feathering.
#[allow(clippy::too_many_arguments)]
pub fn stamp_pencil(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    color: Pixel16,
    size: f32,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    let tip = BrushTip::new(size, 1.0);
    stamp_brush(doc, layer, sel, cx, cy, color, &tip, opacity)
}

/// One Eraser stamp: destination-out (reduces alpha).
pub fn stamp_eraser(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    if doc.mode == ColorMode::Cmyk {
        // Erasing a CMYK pixel reduces its alpha-plane alpha.
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
                let s = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
                if s <= 0.0 {
                    continue;
                }
                let a = doc.layers[layer].get_alpha(xx, yy);
                if a == 0 {
                    continue;
                }
                let na = (a as f32 * (1.0 - s) + 0.5) as u16;
                doc.layers[layer].set_alpha(xx, yy, na);
            }
        }
        return (x0, y0, x1, y1);
    }
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let s = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
            if s <= 0.0 {
                continue;
            }
            let p = doc.layers[layer].get_pixel(xx, yy);
            if p.a == 0 {
                continue;
            }
            let (r, g, b, a) = p.to_f32s();
            doc.layers[layer].set_pixel(xx, yy, Pixel16::from_f32s(r, g, b, a * (1.0 - s)));
        }
    }
    (x0, y0, x1, y1)
}

/// One Background Eraser stamp: erases pixels whose color is
/// close to `sample` (the background color under the crosshair).
/// Erasure strength falls off with color distance, so edges
/// between the background and a differently-colored foreground
/// survive. `tolerance` 0..=255 widens the accepted distance.
#[allow(clippy::too_many_arguments)]
pub fn stamp_erase_bg(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    opacity: f32,
    sample: Pixel16,
    tolerance: u8,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    let (sr, sg, sb, _) = sample.to_f32s();
    // Tolerance in normalized RGB distance (max possible ~1.73).
    let max_dist = (tolerance as f32 / 255.0) * 1.2;
    if max_dist <= 0.0 {
        return (x0, y0, x1, y1);
    }
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let s0 = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
            if s0 <= 0.0 {
                continue;
            }
            let p = doc.layers[layer].get_pixel(xx, yy);
            if p.a == 0 {
                continue;
            }
            let (r, g, b, a) = p.to_f32s();
            let dist = ((r - sr).powi(2) + (g - sg).powi(2) + (b - sb).powi(2)).sqrt();
            if dist > max_dist {
                continue;
            }
            let closeness = 1.0 - dist / max_dist;
            let s = s0 * closeness.clamp(0.0, 1.0);
            if doc.mode == ColorMode::Cmyk {
                let na = (a * 65535.0 * (1.0 - s) + 0.5) as u16;
                doc.layers[layer].set_alpha(xx, yy, na);
            } else {
                doc.layers[layer].set_pixel(xx, yy, Pixel16::from_f32s(r, g, b, a * (1.0 - s)));
            }
        }
    }
    (x0, y0, x1, y1)
}

/// Magic Eraser: erase layer pixels matching the clicked pixel's
/// color within `tolerance` (0..=255) — the contiguous flood when
/// `contiguous`, otherwise every matching pixel on the layer.
/// Returns the affected bbox, or an empty box when the click
/// landed on transparency.
pub fn erase_magic(
    doc: &mut Document,
    layer: usize,
    x: i32,
    y: i32,
    tolerance: u8,
    contiguous: bool,
) -> (i32, i32, i32, i32) {
    let empty = (0, 0, 0, 0);
    let target = doc.layers[layer].get_pixel(x, y);
    if target.a == 0 {
        return empty;
    }
    let (tr, tg, tb, _) = target.to_f32s();
    let max_dist = (tolerance as f32 / 255.0) * 1.2;
    let matches = |doc: &Document, xx: i32, yy: i32| -> bool {
        let p = doc.layers[layer].get_pixel(xx, yy);
        if p.a == 0 {
            return false;
        }
        let (r, g, b, _) = p.to_f32s();
        ((r - tr).powi(2) + (g - tg).powi(2) + (b - tb).powi(2)).sqrt() <= max_dist
    };
    let (w, h) = (doc.width as i32, doc.height as i32);
    let mut hits: Vec<(i32, i32)> = Vec::new();
    if contiguous {
        let mut seen = vec![false; (w * h) as usize];
        let mut stack = vec![(x, y)];
        seen[(y * w + x) as usize] = true;
        while let Some((cx, cy)) = stack.pop() {
            if !matches(doc, cx, cy) {
                continue;
            }
            hits.push((cx, cy));
            for (nx, ny) in [(cx - 1, cy), (cx + 1, cy), (cx, cy - 1), (cx, cy + 1)] {
                if nx >= 0 && ny >= 0 && nx < w && ny < h && !seen[(ny * w + nx) as usize] {
                    seen[(ny * w + nx) as usize] = true;
                    stack.push((nx, ny));
                }
            }
        }
    } else {
        for yy in 0..h {
            for xx in 0..w {
                if matches(doc, xx, yy) {
                    hits.push((xx, yy));
                }
            }
        }
    }
    if hits.is_empty() {
        return empty;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for (hx, hy) in &hits {
        x0 = x0.min(*hx);
        y0 = y0.min(*hy);
        x1 = x1.max(*hx + 1);
        y1 = y1.max(*hy + 1);
    }
    for (hx, hy) in hits {
        if doc.mode == ColorMode::Cmyk {
            doc.layers[layer].set_alpha(hx, hy, 0);
        } else {
            let p = doc.layers[layer].get_pixel(hx, hy);
            let (r, g, b, _) = p.to_f32s();
            doc.layers[layer].set_pixel(hx, hy, Pixel16::from_f32s(r, g, b, 0.0));
        }
    }
    (x0, y0, x1, y1)
}

/// One Clone Stamp: copies pixels from the same layer, where the
/// stamp center maps to `src_c` (source center). Aligned mode (the
/// caller's stroke math) keeps the source glued to the stroke.
#[allow(clippy::too_many_arguments)]
pub fn stamp_clone(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    src_c: (f32, f32),
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    // Snapshot the source area first so a stamp never reads its own writes.
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    let (dx, dy) = (src_c.0 - cx, src_c.1 - cy);
    if doc.mode == ColorMode::Cmyk {
        let mut ink_samples: Vec<(i32, i32, CmykSample)> = Vec::new();
        for yy in y0..y1 {
            for xx in x0..x1 {
                let sx = (xx as f32 + dx).round() as i32;
                let sy = (yy as f32 + dy).round() as i32;
                ink_samples.push((xx, yy, doc.get_cmyk(layer, sx, sy)));
            }
        }
        for (xx, yy, sp) in ink_samples {
            if sp.4 == 0 {
                continue;
            }
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let st = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
            let f = |v: u16| v as f32 / 65535.0;
            over_ink(
                doc,
                layer,
                xx,
                yy,
                (f(sp.0), f(sp.1), f(sp.2), f(sp.3)),
                f(sp.4),
                st,
            );
        }
        return (x0, y0, x1, y1);
    }
    let mut samples: Vec<(i32, i32, Pixel16)> = Vec::new();
    for yy in y0..y1 {
        for xx in x0..x1 {
            let sx = (xx as f32 + dx).round() as i32;
            let sy = (yy as f32 + dy).round() as i32;
            samples.push((xx, yy, doc.layers[layer].get_pixel(sx, sy)));
        }
    }
    for (xx, yy, sp) in samples {
        if sp.a == 0 {
            continue;
        }
        let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
        let s = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
        over_pixel(doc, layer, xx, yy, sp, s);
    }
    (x0, y0, x1, y1)
}

/// One Blur-tool stamp: blends each pixel toward its 3x3 neighborhood
/// average. `strength` 0..=1.
pub fn stamp_blur(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    strength: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    if doc.mode == ColorMode::Cmyk {
        let mut ink_writes: Vec<(i32, i32, CmykSample)> = Vec::new();
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
                let s = tip.falloff(d) * strength * sel_strength(sel, xx, yy);
                if s <= 0.0 {
                    continue;
                }
                let mut acc = [0.0f32; 4];
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        let p = doc.get_cmyk(layer, xx + ox, yy + oy);
                        acc[0] += p.0 as f32 / 65535.0;
                        acc[1] += p.1 as f32 / 65535.0;
                        acc[2] += p.2 as f32 / 65535.0;
                        acc[3] += p.3 as f32 / 65535.0;
                    }
                }
                let cur = doc.get_cmyk(layer, xx, yy);
                let mix = |c: u16, a: f32| -> u16 {
                    let cf = c as f32 / 65535.0;
                    ((cf + (a / 9.0 - cf) * s).clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
                };
                ink_writes.push((
                    xx,
                    yy,
                    (
                        mix(cur.0, acc[0]),
                        mix(cur.1, acc[1]),
                        mix(cur.2, acc[2]),
                        mix(cur.3, acc[3]),
                        cur.4,
                    ),
                ));
            }
        }
        for (x, y, v) in ink_writes {
            doc.set_cmyk(layer, x, y, v);
        }
        return (x0, y0, x1, y1);
    }
    let mut writes: Vec<(i32, i32, Pixel16)> = Vec::new();
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let s = tip.falloff(d) * strength * sel_strength(sel, xx, yy);
            if s <= 0.0 {
                continue;
            }
            let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for oy in -1..=1 {
                for ox in -1..=1 {
                    let p = doc.layers[layer].get_pixel(xx + ox, yy + oy).to_f32s();
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                    acc.3 += p.3;
                }
            }
            let avg = Pixel16::from_f32s(acc.0 / 9.0, acc.1 / 9.0, acc.2 / 9.0, acc.3 / 9.0);
            let cur = doc.layers[layer].get_pixel(xx, yy).to_f32s();
            let mix = |c: f32, a: f32| c + (a - c) * s;
            writes.push((
                xx,
                yy,
                Pixel16::from_f32s(
                    mix(cur.0, acc.0 / 9.0),
                    mix(cur.1, acc.1 / 9.0),
                    mix(cur.2, acc.2 / 9.0),
                    cur.3,
                ),
            ));
            let _ = avg;
        }
    }
    for (x, y, p) in writes {
        doc.layers[layer].set_pixel(x, y, p);
    }
    (x0, y0, x1, y1)
}

/// One Sharpen-tool stamp: pushes each pixel away from its 3x3
/// neighborhood average (unsharp at stamp scale).
pub fn stamp_sharpen(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    strength: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    if doc.mode == ColorMode::Cmyk {
        let mut ink_writes: Vec<(i32, i32, CmykSample)> = Vec::new();
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
                let s = tip.falloff(d) * strength * sel_strength(sel, xx, yy);
                if s <= 0.0 {
                    continue;
                }
                let mut acc = [0.0f32; 4];
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        let p = doc.get_cmyk(layer, xx + ox, yy + oy);
                        acc[0] += p.0 as f32 / 65535.0;
                        acc[1] += p.1 as f32 / 65535.0;
                        acc[2] += p.2 as f32 / 65535.0;
                        acc[3] += p.3 as f32 / 65535.0;
                    }
                }
                let cur = doc.get_cmyk(layer, xx, yy);
                let push = |c: u16, a: f32| -> u16 {
                    let cf = c as f32 / 65535.0;
                    ((cf + (cf - a / 9.0) * s).clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
                };
                ink_writes.push((
                    xx,
                    yy,
                    (
                        push(cur.0, acc[0]),
                        push(cur.1, acc[1]),
                        push(cur.2, acc[2]),
                        push(cur.3, acc[3]),
                        cur.4,
                    ),
                ));
            }
        }
        for (x, y, v) in ink_writes {
            doc.set_cmyk(layer, x, y, v);
        }
        return (x0, y0, x1, y1);
    }
    let mut writes: Vec<(i32, i32, Pixel16)> = Vec::new();
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let s = tip.falloff(d) * strength * sel_strength(sel, xx, yy);
            if s <= 0.0 {
                continue;
            }
            let mut acc = (0.0f32, 0.0f32, 0.0f32);
            for oy in -1..=1 {
                for ox in -1..=1 {
                    let p = doc.layers[layer].get_pixel(xx + ox, yy + oy).to_f32s();
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                }
            }
            let cur = doc.layers[layer].get_pixel(xx, yy).to_f32s();
            let push = |c: f32, a: f32| c + (c - a / 9.0) * s;
            writes.push((
                xx,
                yy,
                Pixel16::from_f32s(
                    push(cur.0, acc.0),
                    push(cur.1, acc.1),
                    push(cur.2, acc.2),
                    cur.3,
                ),
            ));
        }
    }
    for (x, y, p) in writes {
        doc.layers[layer].set_pixel(x, y, p);
    }
    (x0, y0, x1, y1)
}

/// One Smudge stamp: blends each pixel toward `carry` (the color the
/// stroke picked up at its start / previous position — the caller
/// updates it between stamps, which is what drags color along).
#[allow(clippy::too_many_arguments)]
pub fn stamp_smudge(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    carry: Pixel16,
    tip: &BrushTip,
    strength: f32,
) -> (i32, i32, i32, i32) {
    if doc.mode == ColorMode::Cmyk {
        // The carry's slots hold ink values (see sample_average).
        let (x0, y0, x1, y1) = tip.bbox(cx, cy);
        let ink = {
            let f = carry.to_f32s();
            (f.0, f.1, f.2, f.3)
        };
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
                let s =
                    tip.falloff(d) * strength.clamp(0.0, 1.0) * 0.85 * sel_strength(sel, xx, yy);
                over_ink(doc, layer, xx, yy, ink, 1.0, s);
            }
        }
        return (x0, y0, x1, y1);
    }
    stamp_brush(
        doc,
        layer,
        sel,
        cx,
        cy,
        carry,
        tip,
        strength.clamp(0.0, 1.0) * 0.85,
    )
}

/// Sample the average color under a point (for Smudge pickup and the
/// Eyedropper's averaged mode).
pub fn sample_average(doc: &Document, layer: usize, cx: i32, cy: i32, radius: i32) -> Pixel16 {
    if doc.mode == ColorMode::Cmyk {
        // Average ink values; the returned pixel's slots hold
        // (c, m, y, k) — the Smudge carry convention in CMYK mode.
        let mut acc = [0.0f32; 4];
        let mut n = 0.0f32;
        for yy in cy - radius..=cy + radius {
            for xx in cx - radius..=cx + radius {
                let p = doc.get_cmyk(layer, xx, yy);
                acc[0] += p.0 as f32 / 65535.0;
                acc[1] += p.1 as f32 / 65535.0;
                acc[2] += p.2 as f32 / 65535.0;
                acc[3] += p.3 as f32 / 65535.0;
                n += 1.0;
            }
        }
        return Pixel16::from_f32s(acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n);
    }
    let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut n = 0.0f32;
    for yy in cy - radius..=cy + radius {
        for xx in cx - radius..=cx + radius {
            let p = doc.layers[layer].get_pixel(xx, yy).to_f32s();
            acc.0 += p.0;
            acc.1 += p.1;
            acc.2 += p.2;
            acc.3 += p.3;
            n += 1.0;
        }
    }
    Pixel16::from_f32s(acc.0 / n, acc.1 / n, acc.2 / n, acc.3 / n)
}

/// Interpolate stamp centers along a polyline at `spacing` intervals.
pub fn stroke_centers(points: &[(f32, f32)], spacing: f32) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let spacing = spacing.max(1.0);
    let mut centers = vec![points[0]];
    let mut carry = 0.0f32;
    for w in points.windows(2) {
        let (ax, ay) = w[0];
        let (bx, by) = w[1];
        let len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        if len < 1e-6 {
            continue;
        }
        let mut d = spacing - carry;
        while d < len {
            let t = d / len;
            centers.push((ax + (bx - ax) * t, ay + (by - ay) * t));
            d += spacing;
        }
        carry = len - (d - spacing);
    }
    centers
}

/// A full Brush stroke along `points`. Returns the stroke bbox
/// (x, y, w, h) in document pixels, for history capture.
pub fn stroke_brush(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    points: &[(f32, f32)],
    color: Pixel16,
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, u32, u32) {
    let spacing = (tip.size * 0.25).max(1.0);
    let mut bbox: Option<(i32, i32, i32, i32)> = None;
    for (cx, cy) in stroke_centers(points, spacing) {
        let b = stamp_brush(doc, layer, sel, cx, cy, color, tip, opacity);
        bbox = Some(match bbox {
            None => b,
            Some((x0, y0, x1, y1)) => (x0.min(b.0), y0.min(b.1), x1.max(b.2), y1.max(b.3)),
        });
    }
    match bbox {
        Some((x0, y0, x1, y1)) => (x0, y0, (x1 - x0) as u32, (y1 - y0) as u32),
        None => (0, 0, 0, 0),
    }
}

/// A full Clone stroke. `src_start` is the source point aligned with
/// the stroke's first point (the classic "Aligned" mode).
pub fn stroke_clone(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    points: &[(f32, f32)],
    src_start: (f32, f32),
    tip: &BrushTip,
    opacity: f32,
) -> (i32, i32, u32, u32) {
    if points.is_empty() {
        return (0, 0, 0, 0);
    }
    let spacing = (tip.size * 0.25).max(1.0);
    let mut bbox: Option<(i32, i32, i32, i32)> = None;
    for (cx, cy) in stroke_centers(points, spacing) {
        let src = (
            src_start.0 + (cx - points[0].0),
            src_start.1 + (cy - points[0].1),
        );
        let b = stamp_clone(doc, layer, sel, cx, cy, src, tip, opacity);
        bbox = Some(match bbox {
            None => b,
            Some((x0, y0, x1, y1)) => (x0.min(b.0), y0.min(b.1), x1.max(b.2), y1.max(b.3)),
        });
    }
    match bbox {
        Some((x0, y0, x1, y1)) => (x0, y0, (x1 - x0) as u32, (y1 - y0) as u32),
        None => (0, 0, 0, 0),
    }
}

/// Edit > Fill / Paint Bucket core: source-over fill of `color`
/// through the selection mask.
pub fn fill_selection(doc: &mut Document, layer: usize, sel: &Selection, color: Pixel16) {
    if let Some((bx, by, bw, bh)) = sel.bounds() {
        for yy in by..by + bh as i32 {
            for xx in bx..bx + bw as i32 {
                let s = sel.value(xx, yy) as f32 / 255.0;
                over_pixel(doc, layer, xx, yy, color, s);
            }
        }
    }
}

/// Paint Bucket: flood the contiguous (or global) similar-color
/// region under (x, y) with `color`, classic-style.
pub fn paint_bucket(
    doc: &mut Document,
    layer: usize,
    x: i32,
    y: i32,
    tolerance: u8,
    contiguous: bool,
    color: Pixel16,
) {
    let region = Selection::wand(doc, x, y, tolerance, contiguous);
    fill_selection(doc, layer, &region, color);
}

/// Gradient geometry (Phase 5 completes the classic set).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GradientShape {
    Linear,
    Radial,
    Angle,
    Diamond,
    Reflected,
}

/// The Gradient tool: gradient from `fg` at `from` to `bg` at
/// `to`, drawn through the selection (or whole layer). Pixels beyond
/// the endpoints clamp to the end colors, as in the classics.
#[allow(clippy::too_many_arguments)]
pub fn gradient(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    from: (f32, f32),
    to: (f32, f32),
    fg: Pixel16,
    bg: Pixel16,
    opacity: f32,
) {
    gradient_shaped(
        doc,
        layer,
        sel,
        from,
        to,
        fg,
        bg,
        opacity,
        GradientShape::Linear,
    );
}

/// The Gradient tool with a choice of geometry. The drag vector
/// defines the gradient's frame for every shape: length = the
/// fg→bg distance (radius for Radial/Diamond), direction = the
/// starting angle for Angle.
#[allow(clippy::too_many_arguments)]
pub fn gradient_shaped(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    from: (f32, f32),
    to: (f32, f32),
    fg: Pixel16,
    bg: Pixel16,
    opacity: f32,
    shape: GradientShape,
) {
    let (ax, ay) = from;
    let (dx, dy) = (to.0 - ax, to.1 - ay);
    let len2 = dx * dx + dy * dy;
    if len2 < 1.0 {
        return;
    }
    let len = len2.sqrt();
    let bounds = match sel.and_then(|s| s.bounds()) {
        Some(b) => b,
        None => (0, 0, doc.width, doc.height),
    };
    let (f, b) = (fg.to_f32s(), bg.to_f32s());
    for yy in bounds.1..bounds.1 + bounds.3 as i32 {
        for xx in bounds.0..bounds.0 + bounds.2 as i32 {
            let proj = ((xx as f32 - ax) * dx + (yy as f32 - ay) * dy) / len2;
            let t = match shape {
                GradientShape::Linear => proj.clamp(0.0, 1.0),
                GradientShape::Reflected => proj.abs().clamp(0.0, 1.0),
                GradientShape::Radial => {
                    let d = ((xx as f32 - ax).powi(2) + (yy as f32 - ay).powi(2)).sqrt();
                    (d / len).clamp(0.0, 1.0)
                }
                GradientShape::Diamond => {
                    // Manhattan distance in the drag frame.
                    let (ux, uy) = (dx / len, dy / len);
                    let (vx, vy) = (-uy, ux);
                    let (px, py) = (xx as f32 - ax, yy as f32 - ay);
                    let d = (px * ux + py * uy).abs() + (px * vx + py * vy).abs();
                    (d / len).clamp(0.0, 1.0)
                }
                GradientShape::Angle => {
                    let base = dy.atan2(dx);
                    let a = (yy as f32 - ay).atan2(xx as f32 - ax);
                    let mut sweep = (a - base) / (2.0 * std::f32::consts::PI);
                    sweep -= sweep.floor();
                    sweep
                }
            };
            let c = Pixel16::from_f32s(
                f.0 + (b.0 - f.0) * t,
                f.1 + (b.1 - f.1) * t,
                f.2 + (b.2 - f.2) * t,
                f.3 + (b.3 - f.3) * t,
            );
            let s = opacity * sel_strength(sel, xx, yy);
            over_pixel(doc, layer, xx, yy, c, s);
        }
    }
}

/// One stamp whose color comes from a sampler function of the
/// document coordinates — the shared engine for the History Brush
/// (sampler reads a frozen state) and the Pattern Stamp (sampler
/// reads the repeating pattern). Returns the stamp bbox.
#[allow(clippy::too_many_arguments)]
pub fn stamp_sample(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    opacity: f32,
    sampler: &dyn Fn(i32, i32) -> Pixel16,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let s = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
            if s > 0.0 {
                over_pixel(doc, layer, xx, yy, sampler(xx, yy), s);
            }
        }
    }
    (x0, y0, x1, y1)
}

/// One Pattern Stamp: paints the document's defined pattern,
/// aligned to the document origin so strokes tile seamlessly.
#[allow(clippy::too_many_arguments)]
pub fn stamp_pattern(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    opacity: f32,
) -> Option<(i32, i32, i32, i32)> {
    let pat = doc.pattern.clone()?;
    if pat.width == 0 || pat.height == 0 {
        return None;
    }
    let (w, h) = (pat.width as i32, pat.height as i32);
    let sampler = |x: i32, y: i32| {
        let px = ((x % w) + w) % w;
        let py = ((y % h) + h) % h;
        pat.px[(py * w + px) as usize]
    };
    Some(stamp_sample(
        doc, layer, sel, cx, cy, tip, opacity, &sampler,
    ))
}

/// Fill the selection with another document's flattened image,
/// sampled at the same coordinates (Edit > Fill > History, and
/// the History Brush's fill cousin). `opacity` scales the source
/// alpha; the fill composites over the existing pixels.
pub fn fill_from_document(
    doc: &mut Document,
    layer: usize,
    sel: &Selection,
    src: &Document,
    opacity: f32,
) {
    let Some((bx, by, bw, bh)) = sel.bounds() else {
        return;
    };
    for yy in by..by + bh as i32 {
        for xx in bx..bx + bw as i32 {
            let s = sel_strength(Some(sel), xx, yy);
            if s <= 0.0 {
                continue;
            }
            let mut p = src.render_pixel(xx, yy);
            p.a = ((p.a as f32 * opacity) + 0.5) as u16;
            if p.a > 0 {
                over_pixel(doc, layer, xx, yy, p, s);
            }
        }
    }
}

/// Which toning tool a stamp belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToneKind {
    Dodge,
    Burn,
    Sponge,
}

/// The tonal range a dodge/burn stamp leans on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToneRange {
    Shadows,
    Midtones,
    Highlights,
}

impl ToneRange {
    /// Weight 0.0..=1.0 for a pixel of the given luminance: the
    /// stamp acts strongest inside its range and fades out of it.
    fn weight(self, lum: f32) -> f32 {
        match self {
            ToneRange::Shadows => (1.0 - lum) * (1.0 - lum),
            ToneRange::Midtones => 1.0 - (2.0 * lum - 1.0) * (2.0 * lum - 1.0),
            ToneRange::Highlights => lum * lum,
        }
    }
}

/// One Dodge / Burn / Sponge stamp. Dodge lightens multiplicatively
/// toward white, Burn darkens toward black, Sponge pushes color
/// away from (saturate) or toward (desaturate) its own gray.
/// `exposure` is 0.0..=1.0 per stamp; repeated stamps build, as
/// with the classic tools. Works in RGB space; CMYK documents
/// round-trip each pixel through RGB, alpha untouched.
#[allow(clippy::too_many_arguments)]
pub fn stamp_tone(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    kind: ToneKind,
    range: ToneRange,
    exposure: f32,
    saturate: bool,
    opacity: f32,
) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    let cmyk = doc.mode == ColorMode::Cmyk;
    let mut writes: Vec<(i32, i32, Pixel16)> = Vec::new();
    let mut ink_writes: Vec<(i32, i32, CmykSample)> = Vec::new();
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
            let cov = tip.falloff(d) * opacity * sel_strength(sel, xx, yy);
            if cov <= 0.0 {
                continue;
            }
            let (mut r, mut g, mut b, alpha) = if cmyk {
                let (c, m, y2, k, a) = doc.get_cmyk(layer, xx, yy);
                if a == 0 {
                    continue;
                }
                let f = |v: u16| v as f32 / 65535.0;
                let (r, g, b) = crate::color::cmyk_to_rgb(f(c), f(m), f(y2), f(k));
                (r, g, b, a)
            } else {
                let p = doc.layers[layer].get_pixel(xx, yy);
                if p.a == 0 {
                    continue;
                }
                let (r, g, b, _) = p.to_f32s();
                (r, g, b, p.a)
            };
            let lum = 0.3 * r + 0.59 * g + 0.11 * b;
            let s = cov * exposure * range.weight(lum);
            match kind {
                ToneKind::Dodge => {
                    r = 1.0 - (1.0 - r) * (1.0 - s);
                    g = 1.0 - (1.0 - g) * (1.0 - s);
                    b = 1.0 - (1.0 - b) * (1.0 - s);
                }
                ToneKind::Burn => {
                    r *= 1.0 - s;
                    g *= 1.0 - s;
                    b *= 1.0 - s;
                }
                ToneKind::Sponge => {
                    let push = |v: f32| {
                        if saturate {
                            lum + (v - lum) * (1.0 + cov * exposure)
                        } else {
                            v + (lum - v) * cov * exposure
                        }
                    };
                    r = push(r);
                    g = push(g);
                    b = push(b);
                }
            }
            if cmyk {
                let (c, m, y2, k) = crate::color::rgb_to_cmyk(
                    r.clamp(0.0, 1.0),
                    g.clamp(0.0, 1.0),
                    b.clamp(0.0, 1.0),
                );
                let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                ink_writes.push((xx, yy, (e(c), e(m), e(y2), e(k), alpha)));
            } else {
                writes.push((xx, yy, Pixel16::from_f32s(r, g, b, alpha as f32 / 65535.0)));
            }
        }
    }
    for (x, y, p) in writes {
        doc.layers[layer].set_pixel(x, y, p);
    }
    for (x, y, v) in ink_writes {
        doc.set_cmyk(layer, x, y, v);
    }
    (x0, y0, x1, y1)
}

/// Stamp the brush tip into a raw channel buffer (alpha/spot
/// channels): pixels move toward `value` by tip coverage x opacity.
/// This is how painting works when a channel, not the image, is the
/// paint target.
#[allow(clippy::too_many_arguments)]
pub fn stamp_channel(
    data: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    value: u8,
    opacity: f32,
) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    for y in y0..y1 {
        for x in x0..x1 {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let cov = tip.falloff(d) * opacity;
            if cov <= 0.0 {
                continue;
            }
            let i = (y as u32 * width + x as u32) as usize;
            let cur = data[i] as f32;
            data[i] = (cur + (value as f32 - cur) * cov).round() as u8;
        }
    }
}

/// One brush stamp into a layer mask (Phase 4): the same falloff
/// math as `stamp_channel`, but the mask is a sparse tile plane on
/// the layer rather than a flat buffer.
#[allow(clippy::too_many_arguments)]
pub fn stamp_mask(
    layer: &mut crate::layer::Layer,
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    tip: &BrushTip,
    value: u8,
    opacity: f32,
) {
    let (x0, y0, x1, y1) = tip.bbox(cx, cy);
    for y in y0..y1 {
        for x in x0..x1 {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let cov = tip.falloff(d) * opacity;
            if cov <= 0.0 {
                continue;
            }
            let cur = layer.mask_value(x, y) as f32;
            layer.set_mask_value(x, y, (cur + (value as f32 - cur) * cov).round() as u8);
        }
    }
}

/// Flood a layer mask from (x, y): contiguous pixels within
/// `tolerance` of the start value are set to `value`. The Paint
/// Bucket on a mask target.
pub fn flood_mask(
    layer: &mut crate::layer::Layer,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    value: u8,
    tolerance: u8,
) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 || layer.mask.is_none() {
        return;
    }
    let (w, h) = (width as i32, height as i32);
    let start = layer.mask_value(x, y) as i32;
    let tol = tolerance as i32;
    let mut seen = vec![false; (w * h) as usize];
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        let i = (cy * w + cx) as usize;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        if (layer.mask_value(cx, cy) as i32 - start).abs() > tol {
            continue;
        }
        layer.set_mask_value(cx, cy, value);
        for (nx, ny) in [(cx - 1, cy), (cx + 1, cy), (cx, cy - 1), (cx, cy + 1)] {
            if nx >= 0 && ny >= 0 && nx < w && ny < h {
                stack.push((nx, ny));
            }
        }
    }
}

/// Flood a channel buffer from (x, y): contiguous pixels within
/// `tolerance` of the start value are set to `value`. The Paint
/// Bucket on a channel target.
pub fn flood_channel(
    data: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    value: u8,
    tolerance: u8,
) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let w = width as i32;
    let start = data[(y * w + x) as usize] as i32;
    let tol = tolerance as i32;
    let mut seen = vec![false; data.len()];
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        let i = (cy * w + cx) as usize;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        if (data[i] as i32 - start).abs() > tol {
            continue;
        }
        data[i] = value;
        for (nx, ny) in [(cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)] {
            if nx >= 0 && ny >= 0 && nx < w && ny < height as i32 {
                stack.push((nx, ny));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(w: u32, h: u32) -> Document {
        Document::new(w, h)
    }

    #[test]
    fn brush_stamp_paints_center_and_fades_edge() {
        let mut doc = blank(40, 40);
        let tip = BrushTip::new(11.0, 0.0);
        stamp_brush(
            &mut doc,
            0,
            None,
            20.0,
            20.0,
            Pixel16::from_rgba8(255, 0, 0, 255),
            &tip,
            1.0,
        );
        // A fully soft brush (hardness 0) peaks just under full opacity
        // at the center pixel — the falloff is a smoothstep from the
        // exact center point. Color is full; alpha ~243/255.
        let center = doc.layers[0].get_pixel(20, 20).to_rgba8();
        assert_eq!((center.0, center.1, center.2), (255, 0, 0));
        assert!(center.3 > 230, "soft center alpha {}", center.3);
        let edge = doc.layers[0].get_pixel(24, 20);
        assert!(edge.a > 0 && edge.a < 20000, "soft edge alpha {}", edge.a);
        assert_eq!(doc.layers[0].get_pixel(30, 20), Pixel16::TRANSPARENT);
    }

    #[test]
    fn eraser_reduces_alpha() {
        let mut doc = blank(20, 20);
        doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(0, 0, 0, 255));
        let tip = BrushTip::new(9.0, 1.0);
        stamp_eraser(&mut doc, 0, None, 10.0, 10.0, &tip, 1.0);
        assert_eq!(doc.layers[0].get_pixel(10, 10), Pixel16::TRANSPARENT);
        assert_eq!(doc.layers[0].get_pixel(0, 0).to_rgba8().3, 255);
    }

    #[test]
    fn clone_stamp_copies_source() {
        let mut doc = blank(40, 20);
        doc.layers[0].fill_rect(2, 2, 6, 6, Pixel16::from_rgba8(0, 200, 0, 255));
        let tip = BrushTip::new(7.0, 1.0);
        stamp_clone(&mut doc, 0, None, 25.0, 10.0, (5.0, 5.0), &tip, 1.0);
        assert_eq!(doc.layers[0].get_pixel(25, 10).to_rgba8(), (0, 200, 0, 255));
    }

    #[test]
    fn stroke_interpolates_between_points() {
        let mut doc = blank(60, 20);
        let tip = BrushTip::new(5.0, 1.0);
        let bbox = stroke_brush(
            &mut doc,
            0,
            None,
            &[(5.0, 10.0), (55.0, 10.0)],
            Pixel16::from_rgba8(0, 0, 255, 255),
            &tip,
            1.0,
        );
        assert!(bbox.2 > 40);
        // midpoint must be painted even though no input point was there
        assert_eq!(doc.layers[0].get_pixel(30, 10).to_rgba8().2, 255);
    }

    #[test]
    fn paint_respects_selection() {
        let mut doc = blank(20, 20);
        let sel = Selection::rect(20, 20, 0, 0, 10, 20);
        let tip = BrushTip::new(9.0, 1.0);
        stamp_brush(
            &mut doc,
            0,
            Some(&sel),
            10.0,
            10.0,
            Pixel16::from_rgba8(255, 0, 0, 255),
            &tip,
            1.0,
        );
        assert_eq!(doc.layers[0].get_pixel(15, 10), Pixel16::TRANSPARENT);
        assert!(doc.layers[0].get_pixel(8, 10).a > 0);
    }

    #[test]
    fn gradient_runs_fg_to_bg() {
        let mut doc = blank(100, 4);
        gradient(
            &mut doc,
            0,
            None,
            (0.0, 2.0),
            (99.0, 2.0),
            Pixel16::from_rgba8(0, 0, 0, 255),
            Pixel16::from_rgba8(255, 255, 255, 255),
            1.0,
        );
        assert!(doc.layers[0].get_pixel(2, 2).to_rgba8().0 < 20);
        assert!(doc.layers[0].get_pixel(97, 2).to_rgba8().0 > 235);
        let mid = doc.layers[0].get_pixel(50, 2).to_rgba8().0;
        assert!((110..=145).contains(&mid), "mid {mid}");
    }

    #[test]
    fn bucket_fills_region_only() {
        let mut doc = blank(20, 10);
        doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(255, 255, 255, 255));
        doc.layers[0].fill_rect(10, 0, 10, 10, Pixel16::from_rgba8(0, 0, 0, 255));
        paint_bucket(
            &mut doc,
            0,
            2,
            2,
            10,
            true,
            Pixel16::from_rgba8(255, 0, 0, 255),
        );
        assert_eq!(doc.layers[0].get_pixel(5, 5).to_rgba8().0, 255);
        assert_eq!(doc.layers[0].get_pixel(15, 5).to_rgba8().0, 0);
    }
}
