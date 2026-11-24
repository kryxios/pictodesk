//! Image menu geometry: Image Size (resample), Canvas Size, Crop,
//! Rotate, Flip. These are structural edits — wrap them in
//! `history::DocumentCommand` for undo. Layer offsets are baked into
//! the tiles by every transform here, so a layer comes out of a
//! transform with offset (0,0) and its content where it appeared.

use crate::document::Document;
use crate::layer::Layer;
use crate::pixel::Pixel16;

/// Rebuild a layer by mapping every painted pixel through `map`
/// (old doc coords -> new doc coords). Unpainted stays unpainted.
/// When `clip` is (w, h), pixels landing outside the new canvas are
/// dropped instead of kept.
fn rebuild_layer(
    layer: &Layer,
    clip: Option<(u32, u32)>,
    map: impl Fn(i32, i32) -> (i32, i32),
) -> Layer {
    let mut out = Layer::new(layer.name.clone());
    out.opacity = layer.opacity;
    out.visible = layer.visible;
    out.blend_mode = layer.blend_mode;
    out.linked = layer.linked;
    out.adjustment = layer.adjustment.clone();
    if layer.mask.is_some() {
        out.add_mask(layer.mask_fill);
        out.mask_enabled = layer.mask_enabled;
        // Remap the mask plane through the same mapping; only
        // non-fill values need writing (the rest stay default).
        let mcoords: Vec<_> = layer.mask.iter().flat_map(|m| m.keys().copied()).collect();
        for coord in mcoords {
            for ly in 0..256i32 {
                for lx in 0..256i32 {
                    let (dx, dy) = (
                        coord.0 * 256 + lx + layer.offset.0,
                        coord.1 * 256 + ly + layer.offset.1,
                    );
                    let v = layer.mask_value(dx, dy);
                    if v == layer.mask_fill {
                        continue;
                    }
                    let (nx, ny) = map(dx, dy);
                    if let Some((cw, ch)) = clip {
                        if nx < 0 || ny < 0 || nx >= cw as i32 || ny >= ch as i32 {
                            continue;
                        }
                    }
                    out.set_mask_value(nx, ny, v);
                }
            }
        }
    }
    let has_alpha = !layer.cmyk_alpha.is_empty();
    let mut coords: Vec<_> = layer.tiles.keys().copied().collect();
    if has_alpha {
        for c in layer.cmyk_alpha.keys() {
            if !coords.contains(c) {
                coords.push(*c);
            }
        }
    }
    for coord in coords {
        for ly in 0..256i32 {
            for lx in 0..256i32 {
                let (dx, dy) = (
                    coord.0 * 256 + lx + layer.offset.0,
                    coord.1 * 256 + ly + layer.offset.1,
                );
                let p = layer.get_pixel(dx, dy);
                let a = if has_alpha {
                    layer.get_alpha(dx, dy)
                } else {
                    0
                };
                // A pixel is present if it has color/ink or, in CMYK
                // mode, any alpha (white-ink pixels have zero inks).
                if p == Pixel16::TRANSPARENT && a == 0 {
                    continue;
                }
                if !has_alpha && p == Pixel16::TRANSPARENT {
                    continue;
                }
                let (nx, ny) = map(dx, dy);
                if let Some((cw, ch)) = clip {
                    if nx < 0 || ny < 0 || nx >= cw as i32 || ny >= ch as i32 {
                        continue;
                    }
                }
                out.set_pixel(nx, ny, p);
                if has_alpha {
                    out.set_alpha(nx, ny, a);
                }
            }
        }
    }
    out
}

fn rebuild_all(doc: &mut Document, map: impl Fn(i32, i32) -> (i32, i32) + Copy) {
    for i in 0..doc.layers.len() {
        doc.layers[i] = rebuild_layer(&doc.layers[i], None, map);
    }
}

fn rebuild_all_clipped(
    doc: &mut Document,
    clip: (u32, u32),
    map: impl Fn(i32, i32) -> (i32, i32) + Copy,
) {
    for i in 0..doc.layers.len() {
        doc.layers[i] = rebuild_layer(&doc.layers[i], Some(clip), map);
    }
}

/// Image > Flip Horizontal.
pub fn flip_horizontal(doc: &mut Document) {
    let w = doc.width as i32;
    rebuild_all(doc, move |x, y| (w - 1 - x, y));
}

/// Image > Flip Vertical.
pub fn flip_vertical(doc: &mut Document) {
    let h = doc.height as i32;
    rebuild_all(doc, move |x, y| (x, h - 1 - y));
}

/// Image > Rotate 180°.
pub fn rotate_180(doc: &mut Document) {
    let (w, h) = (doc.width as i32, doc.height as i32);
    rebuild_all(doc, move |x, y| (w - 1 - x, h - 1 - y));
}

/// Image > Rotate 90° CW.
pub fn rotate_90_cw(doc: &mut Document) {
    let h = doc.height as i32;
    rebuild_all(doc, move |x, y| (h - 1 - y, x));
    std::mem::swap(&mut doc.width, &mut doc.height);
}

/// Image > Rotate 90° CCW.
pub fn rotate_90_ccw(doc: &mut Document) {
    let w = doc.width as i32;
    rebuild_all(doc, move |x, y| (y, w - 1 - x));
    std::mem::swap(&mut doc.width, &mut doc.height);
}

/// Image > Canvas Size. Content is anchored: `anchor` is (x, y) with
/// each in -1 (left/top), 0 (center), 1 (right/bottom) — the 3x3
/// anchor grid in the classic dialog. Content outside the new canvas
/// is cropped away; new area is transparent.
pub fn canvas_size(doc: &mut Document, new_w: u32, new_h: u32, anchor: (i32, i32)) {
    let dx = ((new_w as i32 - doc.width as i32) * (anchor.0 + 1)) / 2;
    let dy = ((new_h as i32 - doc.height as i32) * (anchor.1 + 1)) / 2;
    rebuild_all_clipped(doc, (new_w, new_h), move |x, y| (x + dx, y + dy));
    doc.width = new_w;
    doc.height = new_h;
}

/// Image > Crop: keep only the given rect (clamped to the document).
pub fn crop(doc: &mut Document, x: i32, y: i32, w: u32, h: u32) {
    let x0 = x.clamp(0, doc.width as i32);
    let y0 = y.clamp(0, doc.height as i32);
    let x1 = (x + w as i32).clamp(x0, doc.width as i32);
    let y1 = (y + h as i32).clamp(y0, doc.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (nw, nh) = ((x1 - x0) as u32, (y1 - y0) as u32);
    rebuild_all_clipped(doc, (nw, nh), move |px, py| (px - x0, py - y0));
    doc.width = nw;
    doc.height = nh;
}

/// Layer > Move: translate one layer's content by (dx, dy).
/// Off-canvas content is kept (tiles support negative coords).
pub fn translate_layer(doc: &mut Document, layer: usize, dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }
    let src = doc.layers[layer].clone();
    doc.layers[layer] = rebuild_layer(&src, None, |x, y| (x + dx, y + dy));
}

/// Free Transform v1 for one layer: scale, rotate, and skew about
/// the center of the layer's painted bounds, bilinear sampling.
/// `skew_x_deg` shears x by tan(angle) * (y - cy). Content that
/// lands outside the canvas is kept (layers are unbounded).
fn grow_bounds(b: &mut Option<(i32, i32, i32, i32)>, x: i32, y: i32) {
    *b = Some(match *b {
        None => (x, y, x + 1, y + 1),
        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
    });
}

/// The painted bounds of one layer in document coordinates
/// (ink pixels plus, for CMYK, pixels with alpha-plane coverage).
pub fn layer_painted_bounds(layer: &crate::layer::Layer) -> Option<(i32, i32, i32, i32)> {
    let src = layer;
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    let grow = grow_bounds;
    for (coord, tile) in &src.tiles {
        for ly in 0..256i32 {
            for lx in 0..256i32 {
                if tile.get(lx as u32, ly as u32) != Pixel16::TRANSPARENT {
                    grow(
                        &mut bounds,
                        coord.0 * 256 + lx + src.offset.0,
                        coord.1 * 256 + ly + src.offset.1,
                    );
                }
            }
        }
    }
    if !src.cmyk_alpha.is_empty() {
        for (coord, data) in &src.cmyk_alpha {
            for (i, &a) in data.iter().enumerate() {
                if a > 0 {
                    grow(
                        &mut bounds,
                        coord.0 * 256 + (i % 256) as i32 + src.offset.0,
                        coord.1 * 256 + (i / 256) as i32 + src.offset.1,
                    );
                }
            }
        }
    }
    bounds
}

pub fn transform_layer(
    doc: &mut Document,
    layer: usize,
    scale_x: f32,
    scale_y: f32,
    rotate_deg: f32,
    skew_x_deg: f32,
) {
    let src = doc.layers[layer].clone();
    let Some((bx0, by0, bx1, by1)) = layer_painted_bounds(&src) else {
        return;
    };
    let (cx, cy) = ((bx0 + bx1) as f32 / 2.0, (by0 + by1) as f32 / 2.0);
    let th = rotate_deg.to_radians();
    let (sin, cos) = (th.sin(), th.cos());
    let sh = skew_x_deg.to_radians().tan();
    // Forward matrix F = T(c) * R * Sh * S * T(-c), mapping src->dst:
    // [a b tx; c d ty] with x' = a*x + b*y + tx, y' = c*x + d*y + ty.
    // R*Sh*S = [[cos, cos*sh - sin],[sin, sin*sh + cos]] * diag(sx, sy)
    let (a, b) = (cos * scale_x, (cos * sh - sin) * scale_y);
    let (c, d) = (sin * scale_x, (sin * sh + cos) * scale_y);
    let (tx, ty) = (cx - (a * cx + b * cy), cy - (c * cx + d * cy));
    let det = a * d - b * c;
    if det.abs() < 1e-9 {
        return;
    }
    // Destination bounds: forward-transform the painted corners.
    let mut db: Option<(i32, i32, i32, i32)> = None;
    for (px, py) in [(bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1)] {
        let fx = a * px as f32 + b * py as f32 + tx;
        let fy = c * px as f32 + d * py as f32 + ty;
        grow_bounds(&mut db, fx.floor() as i32, fy.floor() as i32);
    }
    let (dx0, dy0, dx1, dy1) = db.unwrap();
    let cmyk = doc.mode == crate::document::ColorMode::Cmyk;
    let mut out = Layer::new(src.name.clone());
    out.opacity = src.opacity;
    out.visible = src.visible;
    out.blend_mode = src.blend_mode;
    out.linked = src.linked;
    out.adjustment = src.adjustment.clone();
    out.effects = src.effects;
    out.text = src.text.clone();
    let has_mask = src.mask.is_some();
    if has_mask {
        out.add_mask(src.mask_fill);
        out.mask_enabled = src.mask_enabled;
    }
    for dy in dy0..dy1 {
        for dx in dx0..dx1 {
            // Inverse map dest pixel center -> source coords.
            let (ex, ey) = (dx as f32 + 0.5 - tx, dy as f32 + 0.5 - ty);
            let sx = (d * ex - b * ey) / det - 0.5;
            let sy = (-c * ex + a * ey) / det - 0.5;
            let (x0, y0) = (sx.floor() as i32, sy.floor() as i32);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            if cmyk {
                let smp = |x: i32, y: i32| -> [f32; 5] {
                    let p = src.get_pixel(x, y).to_f32s();
                    [p.0, p.1, p.2, p.3, src.get_alpha(x, y) as f32 / 65535.0]
                };
                let (sa, sb, sc, sd) = (
                    smp(x0, y0),
                    smp(x0 + 1, y0),
                    smp(x0, y0 + 1),
                    smp(x0 + 1, y0 + 1),
                );
                let mut v = [0.0f32; 5];
                for i in 0..5 {
                    let top = sa[i] + (sb[i] - sa[i]) * fx;
                    let bot = sc[i] + (sd[i] - sc[i]) * fx;
                    v[i] = top + (bot - top) * fy;
                }
                if v[4] > 0.001 {
                    out.set_pixel(dx, dy, Pixel16::from_f32s(v[0], v[1], v[2], v[3]));
                    out.set_alpha(dx, dy, (v[4].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16);
                }
            } else {
                let smp = |x: i32, y: i32| src.get_pixel(x, y).to_f32s();
                let (sa, sb, sc, sd) = (
                    smp(x0, y0),
                    smp(x0 + 1, y0),
                    smp(x0, y0 + 1),
                    smp(x0 + 1, y0 + 1),
                );
                let lerp = |p: (f32, f32, f32, f32), q: (f32, f32, f32, f32), t: f32| {
                    (
                        p.0 + (q.0 - p.0) * t,
                        p.1 + (q.1 - p.1) * t,
                        p.2 + (q.2 - p.2) * t,
                        p.3 + (q.3 - p.3) * t,
                    )
                };
                let top = lerp(sa, sb, fx);
                let bot = lerp(sc, sd, fx);
                let v = lerp(top, bot, fy);
                if v.3 > 0.001 {
                    out.set_pixel(dx, dy, Pixel16::from_f32s(v.0, v.1, v.2, v.3));
                }
            }
            if has_mask {
                let mv = src.mask_value(sx.round() as i32, sy.round() as i32);
                out.set_mask_value(dx, dy, mv);
            }
        }
    }
    doc.layers[layer] = out;
}

/// A 3x3 projective mapping (homography) through four point
/// correspondences — the math behind Distort and Perspective,
/// where straight lines stay straight but parallels may converge.
pub struct Homography {
    pub m: [f32; 9],
}

impl Homography {
    /// The homography taking src corners to dst corners (order:
    /// TL, TR, BR, BL). None when the quad is degenerate.
    pub fn through(src: [(f32, f32); 4], dst: [(f32, f32); 4]) -> Option<Homography> {
        // Solve for h (h33 = 1) with dst ~ H * src, 8 equations.
        let mut a = [[0f32; 9]; 8];
        for i in 0..4 {
            let (x, y) = src[i];
            let (u, v) = dst[i];
            a[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            a[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        // Gaussian elimination with partial pivoting.
        for col in 0..8 {
            let mut piv = col;
            for r in col + 1..8 {
                if a[r][col].abs() > a[piv][col].abs() {
                    piv = r;
                }
            }
            if a[piv][col].abs() < 1e-9 {
                return None;
            }
            a.swap(col, piv);
            let d = a[col][col];
            for v in &mut a[col][col..] {
                *v /= d;
            }
            let prow = a[col];
            for (r, row) in a.iter_mut().enumerate() {
                if r != col && row[col] != 0.0 {
                    let f = row[col];
                    for (j, v) in row.iter_mut().enumerate().skip(col) {
                        *v -= f * prow[j];
                    }
                }
            }
        }
        let h: Vec<f32> = (0..8).map(|i| a[i][8]).collect();
        Some(Homography {
            m: [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0],
        })
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let w = self.m[6] * x + self.m[7] * y + self.m[8];
        (
            (self.m[0] * x + self.m[1] * y + self.m[2]) / w,
            (self.m[3] * x + self.m[4] * y + self.m[5]) / w,
        )
    }

    pub fn inverse(&self) -> Option<Homography> {
        let m = &self.m;
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6]);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = [
            (m[4] * m[8] - m[5] * m[7]) / det,
            (m[2] * m[7] - m[1] * m[8]) / det,
            (m[1] * m[5] - m[2] * m[4]) / det,
            (m[5] * m[6] - m[3] * m[8]) / det,
            (m[0] * m[8] - m[2] * m[6]) / det,
            (m[2] * m[3] - m[0] * m[5]) / det,
            (m[3] * m[7] - m[4] * m[6]) / det,
            (m[1] * m[6] - m[0] * m[7]) / det,
            (m[0] * m[4] - m[1] * m[3]) / det,
        ];
        Some(Homography { m: inv })
    }
}

/// Free Transform with the four corners dragged freely (Distort /
/// Perspective): the layer's painted bounds corners land on
/// `corners` (TL, TR, BR, BL) via a projective warp.
pub fn transform_layer_quad(doc: &mut Document, layer: usize, corners: [(f32, f32); 4]) {
    let src = doc.layers[layer].clone();
    let Some((bx0, by0, bx1, by1)) = layer_painted_bounds(&src) else {
        return;
    };
    let src_corners = [
        (bx0 as f32, by0 as f32),
        (bx1 as f32, by0 as f32),
        (bx1 as f32, by1 as f32),
        (bx0 as f32, by1 as f32),
    ];
    let Some(fwd) = Homography::through(src_corners, corners) else {
        return;
    };
    let Some(inv) = fwd.inverse() else { return };
    let mut db: Option<(i32, i32, i32, i32)> = None;
    for (px, py) in corners {
        grow_bounds(&mut db, px.floor() as i32, py.floor() as i32);
    }
    let (dx0, dy0, dx1, dy1) = db.unwrap();
    let cmyk = doc.mode == crate::document::ColorMode::Cmyk;
    let mut out = Layer::new(src.name.clone());
    out.opacity = src.opacity;
    out.visible = src.visible;
    out.blend_mode = src.blend_mode;
    out.linked = src.linked;
    out.adjustment = src.adjustment.clone();
    out.effects = src.effects;
    out.text = src.text.clone();
    let has_mask = src.mask.is_some();
    if has_mask {
        out.add_mask(src.mask_fill);
        out.mask_enabled = src.mask_enabled;
    }
    // The warp only paints inside the destination quad — the
    // homography extrapolates past its edges, and those pixels
    // must stay untouched.
    let in_quad = |px: f32, py: f32| -> bool {
        let tri = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| -> bool {
            let s =
                |p: (f32, f32), q: (f32, f32)| (px - q.0) * (p.1 - q.1) - (p.0 - q.0) * (py - q.1);
            let d1 = s(a, b);
            let d2 = s(b, c);
            let d3 = s(c, a);
            !(((d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0)) && ((d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0)))
        };
        tri(corners[0], corners[1], corners[2]) || tri(corners[0], corners[2], corners[3])
    };
    for dy in dy0..dy1 {
        for dx in dx0..dx1 {
            if !in_quad(dx as f32 + 0.5, dy as f32 + 0.5) {
                continue;
            }
            let (sx, sy) = inv.apply(dx as f32 + 0.5, dy as f32 + 0.5);
            let (sx, sy) = (sx - 0.5, sy - 0.5);
            let (x0, y0) = (sx.floor() as i32, sy.floor() as i32);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            if cmyk {
                let smp = |x: i32, y: i32| -> [f32; 5] {
                    let p = src.get_pixel(x, y).to_f32s();
                    [p.0, p.1, p.2, p.3, src.get_alpha(x, y) as f32 / 65535.0]
                };
                let (sa, sb, sc, sd) = (
                    smp(x0, y0),
                    smp(x0 + 1, y0),
                    smp(x0, y0 + 1),
                    smp(x0 + 1, y0 + 1),
                );
                let mut v = [0.0f32; 5];
                for i in 0..5 {
                    let top = sa[i] + (sb[i] - sa[i]) * fx;
                    let bot = sc[i] + (sd[i] - sc[i]) * fx;
                    v[i] = top + (bot - top) * fy;
                }
                if v[4] > 0.001 {
                    out.set_pixel(dx, dy, Pixel16::from_f32s(v[0], v[1], v[2], v[3]));
                    out.set_alpha(dx, dy, (v[4].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16);
                }
            } else {
                let smp = |x: i32, y: i32| src.get_pixel(x, y).to_f32s();
                let (sa, sb, sc, sd) = (
                    smp(x0, y0),
                    smp(x0 + 1, y0),
                    smp(x0, y0 + 1),
                    smp(x0 + 1, y0 + 1),
                );
                let lerp = |p: (f32, f32, f32, f32), q: (f32, f32, f32, f32), t: f32| {
                    (
                        p.0 + (q.0 - p.0) * t,
                        p.1 + (q.1 - p.1) * t,
                        p.2 + (q.2 - p.2) * t,
                        p.3 + (q.3 - p.3) * t,
                    )
                };
                let v = lerp(lerp(sa, sb, fx), lerp(sc, sd, fx), fy);
                if v.3 > 0.001 {
                    out.set_pixel(dx, dy, Pixel16::from_f32s(v.0, v.1, v.2, v.3));
                }
            }
            if has_mask {
                let mv = src.mask_value(sx.round() as i32, sy.round() as i32);
                out.set_mask_value(dx, dy, mv);
            }
        }
    }
    doc.layers[layer] = out;
}

/// Select > Transform Selection: warp the selection mask itself
/// (pixels untouched) by the same projective mapping, from the
/// mask's current bounds corners to `corners` (TL, TR, BR, BL).
pub fn transform_selection_quad(sel: &mut crate::selection::Selection, corners: [(f32, f32); 4]) {
    let Some((bx, by, bw, bh)) = sel.bounds() else {
        return;
    };
    let (bx1, by1) = (bx + bw as i32, by + bh as i32);
    let src_corners = [
        (bx as f32, by as f32),
        (bx1 as f32, by as f32),
        (bx1 as f32, by1 as f32),
        (bx as f32, by1 as f32),
    ];
    let Some(fwd) = Homography::through(src_corners, corners) else {
        return;
    };
    let Some(inv) = fwd.inverse() else { return };
    let old = sel.clone();
    let mut db: Option<(i32, i32, i32, i32)> = None;
    for (px, py) in corners {
        grow_bounds(&mut db, px.floor() as i32, py.floor() as i32);
    }
    let (dx0, dy0, dx1, dy1) = db.unwrap();
    // Rewrite the union of old and new bounds so shrinking a
    // selection clears what it leaves behind.
    let (ux0, uy0) = (bx.min(dx0), by.min(dy0));
    let (ux1, uy1) = (bx1.max(dx1), by1.max(dy1));
    for dy in uy0..uy1 {
        for dx in ux0..ux1 {
            let (sx, sy) = inv.apply(dx as f32 + 0.5, dy as f32 + 0.5);
            let v = old.value(sx.floor() as i32, sy.floor() as i32);
            sel.set(dx, dy, v);
        }
    }
}

/// Image > Image Size: resample every layer to the new dimensions
/// (bilinear). This is the Phase 1 resampler; fancier kernels are a
/// later refinement.
pub fn resize_image(doc: &mut Document, new_w: u32, new_h: u32) {
    if new_w == 0 || new_h == 0 || (new_w == doc.width && new_h == doc.height) {
        return;
    }
    let (ow, oh) = (doc.width, doc.height);
    let sx = ow as f32 / new_w as f32;
    let sy = oh as f32 / new_h as f32;
    let cmyk = doc.mode == crate::document::ColorMode::Cmyk;
    for li in 0..doc.layers.len() {
        let src = doc.layers[li].clone();
        let mut out = Layer::new(src.name.clone());
        out.opacity = src.opacity;
        out.visible = src.visible;
        out.blend_mode = src.blend_mode;
        out.adjustment = src.adjustment.clone();
        if src.mask.is_some() {
            out.add_mask(src.mask_fill);
            out.mask_enabled = src.mask_enabled;
            for yy in 0..new_h as i32 {
                for xx in 0..new_w as i32 {
                    let fx = ((xx as f32 + 0.5) * sx - 0.5).round() as i32;
                    let fy = ((yy as f32 + 0.5) * sy - 0.5).round() as i32;
                    let mv = src.mask_value(fx.clamp(0, ow as i32 - 1), fy.clamp(0, oh as i32 - 1));
                    out.set_mask_value(xx, yy, mv);
                }
            }
        }
        if cmyk {
            // Bilinear over the five CMYK values (inks + alpha).
            for yy in 0..new_h as i32 {
                for xx in 0..new_w as i32 {
                    let fx = (xx as f32 + 0.5) * sx - 0.5;
                    let fy = (yy as f32 + 0.5) * sy - 0.5;
                    let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
                    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                    let smp = |x: i32, y: i32| -> [f32; 5] {
                        let vx = x.clamp(0, ow as i32 - 1);
                        let vy = y.clamp(0, oh as i32 - 1);
                        let p = src.get_pixel(vx, vy).to_f32s();
                        let a = src.get_alpha(vx, vy) as f32 / 65535.0;
                        [p.0, p.1, p.2, p.3, a]
                    };
                    let (sa, sb, sc, sd) = (
                        smp(x0, y0),
                        smp(x0 + 1, y0),
                        smp(x0, y0 + 1),
                        smp(x0 + 1, y0 + 1),
                    );
                    let mut v = [0.0f32; 5];
                    for c in 0..5 {
                        let top = sa[c] + (sb[c] - sa[c]) * tx;
                        let bot = sc[c] + (sd[c] - sc[c]) * tx;
                        v[c] = top + (bot - top) * ty;
                    }
                    if v[4] > 0.0 {
                        let e = |q: f32| (q.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                        out.set_pixel(xx, yy, Pixel16::from_f32s(v[0], v[1], v[2], v[3]));
                        out.set_alpha(xx, yy, e(v[4]));
                    }
                }
            }
            doc.layers[li] = out;
            continue;
        }
        for yy in 0..new_h as i32 {
            for xx in 0..new_w as i32 {
                let fx = (xx as f32 + 0.5) * sx - 0.5;
                let fy = (yy as f32 + 0.5) * sy - 0.5;
                let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let sample = |x: i32, y: i32| {
                    src.get_pixel(x.clamp(0, ow as i32 - 1), y.clamp(0, oh as i32 - 1))
                        .to_f32s()
                };
                let (a, b, c, d) = (
                    sample(x0, y0),
                    sample(x0 + 1, y0),
                    sample(x0, y0 + 1),
                    sample(x0 + 1, y0 + 1),
                );
                let lerp2 = |p: (f32, f32, f32, f32), q: (f32, f32, f32, f32), t: f32| {
                    (
                        p.0 + (q.0 - p.0) * t,
                        p.1 + (q.1 - p.1) * t,
                        p.2 + (q.2 - p.2) * t,
                        p.3 + (q.3 - p.3) * t,
                    )
                };
                let top = lerp2(a, b, tx);
                let bot = lerp2(c, d, tx);
                let v = lerp2(top, bot, ty);
                let p = Pixel16::from_f32s(v.0, v.1, v.2, v.3);
                if p != Pixel16::TRANSPARENT {
                    out.set_pixel(xx, yy, p);
                }
            }
        }
        doc.layers[li] = out;
    }
    doc.width = new_w;
    doc.height = new_h;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked_doc() -> Document {
        let mut doc = Document::new(4, 2);
        doc.layers[0].set_pixel(0, 0, Pixel16::from_rgba8(255, 0, 0, 255));
        doc.layers[0].set_pixel(3, 1, Pixel16::from_rgba8(0, 0, 255, 255));
        doc
    }

    #[test]
    fn flip_horizontal_mirrors() {
        let mut doc = marked_doc();
        flip_horizontal(&mut doc);
        assert_eq!(doc.layers[0].get_pixel(3, 0).to_rgba8().0, 255);
        assert_eq!(doc.layers[0].get_pixel(0, 1).to_rgba8().2, 255);
    }

    #[test]
    fn rotate_90_cw_swaps_dimensions_and_moves_pixels() {
        let mut doc = marked_doc();
        rotate_90_cw(&mut doc);
        assert_eq!((doc.width, doc.height), (2, 4));
        // (0,0) red -> (h-1-0, 0) = (1, 0)
        assert_eq!(doc.layers[0].get_pixel(1, 0).to_rgba8().0, 255);
    }

    #[test]
    fn rotate_four_times_is_identity() {
        let mut doc = marked_doc();
        let before = doc.render_checksum();
        for _ in 0..4 {
            rotate_90_cw(&mut doc);
        }
        assert_eq!(doc.render_checksum(), before);
    }

    #[test]
    fn canvas_size_grows_with_anchor() {
        let mut doc = marked_doc();
        canvas_size(&mut doc, 6, 4, (0, 0)); // center: dx=1, dy=1
        assert_eq!((doc.width, doc.height), (6, 4));
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 255);
    }

    #[test]
    fn crop_keeps_only_the_rect() {
        let mut doc = marked_doc();
        crop(&mut doc, 2, 0, 2, 2);
        assert_eq!((doc.width, doc.height), (2, 2));
        // blue was at (3,1) -> now (1,1); red at (0,0) is cropped away
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().2, 255);
        assert_eq!(doc.layers[0].tile_count(), 1);
        let mut reds = 0;
        for yy in 0..2 {
            for xx in 0..2 {
                if doc.layers[0].get_pixel(xx, yy).to_rgba8().0 == 255 {
                    reds += 1;
                }
            }
        }
        assert_eq!(reds, 0);
    }

    #[test]
    fn resize_up_keeps_colors_present() {
        let mut doc = marked_doc();
        resize_image(&mut doc, 8, 4);
        assert_eq!((doc.width, doc.height), (8, 4));
        assert!(doc.layers[0].get_pixel(0, 0).to_rgba8().0 > 200);
    }
}
