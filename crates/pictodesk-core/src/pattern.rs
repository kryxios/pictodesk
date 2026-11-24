//! Pattern Maker (Phase 7): generate a seamless tile from a
//! rectangular sample of the composite. The tile blends the
//! sample with its half-offset copy under a sin^2 weight
//! field that is zero at the tile edges and one at the
//! center — at the edges the tile therefore samples the
//! sample's center region on all four sides, so opposite
//! edges continue each other and the tile repeats without a
//! visible seam, while the middle keeps the original
//! content. (The classic plug-in resynthesized texture;
//! this is the deterministic cross-blend form, named in the
//! phase delivery.)

use crate::document::{Document, PatternData};
use crate::pixel::Pixel16;

/// Build a seamless tile from the composite inside
/// `rect` (x, y, w, h in document coords). Samples larger
/// than 512 px on a side are box-downscaled by an integer
/// factor first. Returns the tile as PatternData.
pub fn make_tile(doc: &Document, rect: (i32, i32, i32, i32), name: &str) -> Option<PatternData> {
    let (x0, y0, mut w, mut h) = rect;
    if w < 4 || h < 4 {
        return None;
    }
    // Sample the composite at an integer stride so the tile
    // stays <= 512 px on a side.
    let mut stride = 1i32;
    while w / stride > 512 || h / stride > 512 {
        stride += 1;
    }
    w /= stride;
    h /= stride;
    let mut s: Vec<Pixel16> = Vec::with_capacity((w * h) as usize);
    for yy in 0..h {
        for xx in 0..w {
            // Box-average the stride block.
            let mut acc = [0f32; 4];
            let mut n = 0f32;
            for by in 0..stride {
                for bx in 0..stride {
                    let p = doc.render_pixel(
                        (x0 + xx * stride + bx).clamp(0, doc.width as i32 - 1),
                        (y0 + yy * stride + by).clamp(0, doc.height as i32 - 1),
                    );
                    acc[0] += p.r as f32;
                    acc[1] += p.g as f32;
                    acc[2] += p.b as f32;
                    acc[3] += p.a as f32;
                    n += 1.0;
                }
            }
            s.push(Pixel16 {
                r: (acc[0] / n) as u16,
                g: (acc[1] / n) as u16,
                b: (acc[2] / n) as u16,
                a: (acc[3] / n) as u16,
            });
        }
    }
    let (w, h) = (w as usize, h as usize);
    let at = |x: usize, y: usize| -> [f32; 4] {
        let p = s[y * w + x];
        [p.r as f32, p.g as f32, p.b as f32, p.a as f32]
    };
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let my = (std::f32::consts::PI * y as f32 / h as f32).sin().powi(2);
        for x in 0..w {
            let mx = (std::f32::consts::PI * x as f32 / w as f32).sin().powi(2);
            let m = mx * my; // 0 at edges, 1 at center
            let a = at(x, y);
            let b = at((x + w / 2) % w, (y + h / 2) % h);
            let mix = |i: usize| (a[i] * m + b[i] * (1.0 - m)).clamp(0.0, 65535.0) as u16;
            out.push(Pixel16 {
                r: mix(0),
                g: mix(1),
                b: mix(2),
                a: mix(3),
            });
        }
    }
    Some(PatternData {
        name: name.into(),
        width: w as u32,
        height: h as u32,
        px: out,
    })
}

/// Fill the active layer's selection (or whole canvas) with
/// the document pattern, tiled from the document origin
/// (Pattern Maker's Fill button and Edit > Fill share the
/// tiling convention).
pub fn fill_layer_pattern(
    doc: &mut Document,
    layer: usize,
    sel: Option<&crate::selection::Selection>,
) {
    let Some(pat) = doc.pattern.clone() else {
        return;
    };
    let (w, h) = (doc.width as i32, doc.height as i32);
    for y in 0..h {
        for x in 0..w {
            if let Some(s) = sel {
                if s.value(x, y) == 0 {
                    continue;
                }
            }
            let tx = x.rem_euclid(pat.width as i32) as usize;
            let ty = y.rem_euclid(pat.height as i32) as usize;
            let p = pat.px[ty * pat.width as usize + tx];
            if p.a == 0 {
                continue;
            }
            doc.layers[layer].set_pixel(x, y, p);
        }
    }
}
