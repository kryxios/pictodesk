//! Painting tools — the classic toolbox's pixel pushers:
//! Brush (soft, spaced stamps), Pencil (hard), Eraser, Clone Stamp
//! (Rubber Stamp in 1.0), Paint Bucket, Gradient, and the basic
//! Blur/Sharpen/Smudge stamp tools.
//!
//! Every stamp respects the selection mask. Strokes interpolate
//! stamps along the path (spacing = 25% of diameter, the classic
//! default) and return their bounding box so the caller can wrap
//! the stroke in a tile-snapshot history command.

use crate::document::Document;
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
fn over_pixel(doc: &mut Document, layer: usize, x: i32, y: i32, color: Pixel16, strength: f32) {
    if strength <= 0.0 {
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

/// The Gradient tool: linear gradient from `fg` at `from` to `bg` at
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
    let (ax, ay) = from;
    let (dx, dy) = (to.0 - ax, to.1 - ay);
    let len2 = dx * dx + dy * dy;
    if len2 < 1.0 {
        return;
    }
    let bounds = match sel.and_then(|s| s.bounds()) {
        Some(b) => b,
        None => (0, 0, doc.width, doc.height),
    };
    let (f, b) = (fg.to_f32s(), bg.to_f32s());
    for yy in bounds.1..bounds.1 + bounds.3 as i32 {
        for xx in bounds.0..bounds.0 + bounds.2 as i32 {
            let t = (((xx as f32 - ax) * dx + (yy as f32 - ay) * dy) / len2).clamp(0.0, 1.0);
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
