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
    for (coord, tile) in &layer.tiles {
        for ly in 0..256i32 {
            for lx in 0..256i32 {
                let p = tile.get(lx as u32, ly as u32);
                if p == Pixel16::TRANSPARENT {
                    continue;
                }
                let (dx, dy) = (
                    coord.0 * 256 + lx + layer.offset.0,
                    coord.1 * 256 + ly + layer.offset.1,
                );
                let (nx, ny) = map(dx, dy);
                if let Some((cw, ch)) = clip {
                    if nx < 0 || ny < 0 || nx >= cw as i32 || ny >= ch as i32 {
                        continue;
                    }
                }
                out.set_pixel(nx, ny, p);
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
/// anchor grid in Photoshop's dialog. Content outside the new canvas
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
    for li in 0..doc.layers.len() {
        let src = doc.layers[li].clone();
        let mut out = Layer::new(src.name.clone());
        out.opacity = src.opacity;
        out.visible = src.visible;
        out.blend_mode = src.blend_mode;
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
