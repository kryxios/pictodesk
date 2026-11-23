//! Filters v1 — the classic Filter menu: Gaussian Blur,
//! Sharpen, Unsharp Mask, Add Noise. Filters read a snapshot of the
//! layer region first (no feedback smearing), then write back
//! blended by the selection mask.

use crate::document::Document;
use crate::pixel::Pixel16;
use crate::selection::Selection;

struct Region {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    px: Vec<Pixel16>,
}

fn read_region(doc: &Document, layer: usize, rect: (i32, i32, u32, u32)) -> Region {
    let (x, y, w, h) = rect;
    let mut px = Vec::with_capacity((w * h) as usize);
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            px.push(doc.layers[layer].get_pixel(x + xx, y + yy));
        }
    }
    Region { x, y, w, h, px }
}

impl Region {
    fn at(&self, x: i32, y: i32) -> Pixel16 {
        let cx = x.clamp(0, self.w as i32 - 1) as u32;
        let cy = y.clamp(0, self.h as i32 - 1) as u32;
        self.px[(cy * self.w + cx) as usize]
    }
}

fn write_region(doc: &mut Document, layer: usize, region: &Region, sel: Option<&Selection>) {
    for yy in 0..region.h as i32 {
        for xx in 0..region.w as i32 {
            let (dx, dy) = (region.x + xx, region.y + yy);
            let strength = match sel {
                None => 255,
                Some(s) => s.value(dx, dy),
            };
            if strength == 0 {
                continue;
            }
            let out = region.px[(yy as u32 * region.w + xx as u32) as usize];
            if strength == 255 {
                doc.layers[layer].set_pixel(dx, dy, out);
            } else {
                let orig = doc.layers[layer].get_pixel(dx, dy);
                let t = strength as f32 / 255.0;
                let l = |a: u16, b: u16| (a as f32 + (b as f32 - a as f32) * t + 0.5) as u16;
                doc.layers[layer].set_pixel(
                    dx,
                    dy,
                    Pixel16::new(
                        l(orig.r, out.r),
                        l(orig.g, out.g),
                        l(orig.b, out.b),
                        l(orig.a, out.a),
                    ),
                );
            }
        }
    }
}

fn work_rect(doc: &Document, sel: Option<&Selection>) -> (i32, i32, u32, u32) {
    match sel.and_then(|s| s.bounds()) {
        Some(b) => b,
        None => (0, 0, doc.width, doc.height),
    }
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let half = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut k: Vec<f32> = (-half..=half)
        .map(|i| (-(i as f32 * i as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    k
}

fn blur_region(src: &Region, sigma: f32) -> Region {
    let kernel = gaussian_kernel(sigma);
    let half = (kernel.len() / 2) as i32;
    let (w, h) = (src.w as i32, src.h as i32);
    let mut tmp = vec![(0.0f32, 0.0f32, 0.0f32, 0.0f32); src.px.len()];
    let mut out = vec![(0.0f32, 0.0f32, 0.0f32, 0.0f32); src.px.len()];
    for yy in 0..h {
        for xx in 0..w {
            let mut acc = (0.0, 0.0, 0.0, 0.0);
            for (ki, &kv) in kernel.iter().enumerate() {
                let p = src.at(xx + ki as i32 - half, yy).to_f32s();
                acc.0 += p.0 * kv;
                acc.1 += p.1 * kv;
                acc.2 += p.2 * kv;
                acc.3 += p.3 * kv;
            }
            tmp[(yy * w + xx) as usize] = acc;
        }
    }
    for yy in 0..h {
        for xx in 0..w {
            let mut acc = (0.0, 0.0, 0.0, 0.0);
            for (ki, &kv) in kernel.iter().enumerate() {
                let p = tmp[((yy + ki as i32 - half).clamp(0, h - 1) * w + xx) as usize];
                acc.0 += p.0 * kv;
                acc.1 += p.1 * kv;
                acc.2 += p.2 * kv;
                acc.3 += p.3 * kv;
            }
            out[(yy * w + xx) as usize] = acc;
        }
    }
    Region {
        x: src.x,
        y: src.y,
        w: src.w,
        h: src.h,
        px: out
            .iter()
            .map(|&(r, g, b, a)| Pixel16::from_f32s(r, g, b, a))
            .collect(),
    }
}

/// Filter > Blur > Gaussian Blur. `radius` is the classic radius:
/// roughly the blur's standard deviation in pixels.
pub fn gaussian_blur(doc: &mut Document, layer: usize, sel: Option<&Selection>, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    let src = read_region(doc, layer, work_rect(doc, sel));
    let blurred = blur_region(&src, radius.max(0.1));
    write_region(doc, layer, &blurred, sel);
}

/// Filter > Sharpen > Sharpen: the classic 3x3 kernel.
pub fn sharpen(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    let src = read_region(doc, layer, work_rect(doc, sel));
    let mut out = src.px.clone();
    for yy in 0..src.h as i32 {
        for xx in 0..src.w as i32 {
            let c = src.at(xx, yy).to_f32s();
            let mut acc = (c.0 * 5.0, c.1 * 5.0, c.2 * 5.0);
            for (nx, ny) in [(xx - 1, yy), (xx + 1, yy), (xx, yy - 1), (xx, yy + 1)] {
                let p = src.at(nx, ny).to_f32s();
                acc.0 -= p.0;
                acc.1 -= p.1;
                acc.2 -= p.2;
            }
            out[(yy as u32 * src.w + xx as u32) as usize] =
                Pixel16::from_f32s(acc.0, acc.1, acc.2, c.3);
        }
    }
    let rect = (src.x, src.y, src.w, src.h);
    write_region(
        doc,
        layer,
        &Region {
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            px: out,
        },
        sel,
    );
}

/// Filter > Sharpen > Unsharp Mask. amount: 0.0..=5.0 (the classic
/// percent / 100), radius: blur radius, threshold: 0..=255 levels
/// that must differ before sharpening applies.
pub fn unsharp_mask(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    amount: f32,
    radius: f32,
    threshold: u8,
) {
    let src = read_region(doc, layer, work_rect(doc, sel));
    let blurred = blur_region(&src, radius.max(0.1));
    let thr = threshold as f32 / 255.0;
    let mut out = src.px.clone();
    for (i, out_px) in out.iter_mut().enumerate() {
        let s = src.px[i].to_f32s();
        let b = blurred.px[i].to_f32s();
        let adj = |sv: f32, bv: f32| -> f32 {
            let d = sv - bv;
            if d.abs() < thr {
                sv
            } else {
                sv + d * amount
            }
        };
        *out_px = Pixel16::from_f32s(adj(s.0, b.0), adj(s.1, b.1), adj(s.2, b.2), s.3);
    }
    let rect = (src.x, src.y, src.w, src.h);
    write_region(
        doc,
        layer,
        &Region {
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            px: out,
        },
        sel,
    );
}

fn hash_unit(seed: u64, x: i32, y: i32, ch: u64) -> f32 {
    let mut z = seed
        ^ (x as u64).wrapping_mul(0x9E3779B97F4A7C15)
        ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ ch.wrapping_mul(0x165667B19E3779F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// Filter > Noise > Add Noise. amount: 0.0..=1.0 (percent
/// / 100), uniform distribution, per-channel (not monochromatic).
pub fn add_noise(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    amount: f32,
    seed: u64,
) {
    let rect = work_rect(doc, sel);
    let src = read_region(doc, layer, rect);
    let mut out = src.px.clone();
    for yy in 0..src.h as i32 {
        for xx in 0..src.w as i32 {
            let i = (yy as u32 * src.w + xx as u32) as usize;
            let p = src.px[i];
            if p.a == 0 {
                continue;
            }
            let (r, g, b, a) = p.to_f32s();
            let d = |ch: u64| (hash_unit(seed, rect.0 + xx, rect.1 + yy, ch) - 0.5) * amount;
            out[i] = Pixel16::from_f32s(r + d(0), g + d(1), b + d(2), a);
        }
    }
    write_region(
        doc,
        layer,
        &Region {
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            px: out,
        },
        sel,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_doc() -> Document {
        let mut doc = Document::new(9, 9);
        doc.layers[0].fill_rect(0, 0, 9, 9, Pixel16::from_rgba8(0, 0, 0, 255));
        doc.layers[0].set_pixel(4, 4, Pixel16::from_rgba8(255, 255, 255, 255));
        doc
    }

    #[test]
    fn gaussian_blur_spreads_the_impulse() {
        let mut doc = impulse_doc();
        gaussian_blur(&mut doc, 0, None, 1.0);
        let center = doc.layers[0].get_pixel(4, 4).to_rgba8().0;
        let neighbor = doc.layers[0].get_pixel(5, 4).to_rgba8().0;
        assert!(center < 255 && center > 30, "center {center}");
        assert!(neighbor > 5, "neighbor {neighbor}");
    }

    #[test]
    fn blur_of_flat_field_is_identity() {
        let mut doc = Document::new(8, 8);
        doc.layers[0].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(90, 90, 90, 255));
        gaussian_blur(&mut doc, 0, None, 2.0);
        assert_eq!(doc.layers[0].get_pixel(4, 4).to_rgba8().0, 90);
    }

    #[test]
    fn sharpen_boosts_an_edge() {
        let mut doc = Document::new(6, 1);
        for x in 0..3 {
            doc.layers[0].set_pixel(x, 0, Pixel16::from_rgba8(100, 100, 100, 255));
        }
        for x in 3..6 {
            doc.layers[0].set_pixel(x, 0, Pixel16::from_rgba8(160, 160, 160, 255));
        }
        sharpen(&mut doc, 0, None);
        assert!(doc.layers[0].get_pixel(2, 0).to_rgba8().0 < 100);
        assert!(doc.layers[0].get_pixel(3, 0).to_rgba8().0 > 160);
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let mut a = Document::new(8, 8);
        a.layers[0].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(128, 128, 128, 255));
        let mut b = a.clone();
        add_noise(&mut a, 0, None, 0.5, 42);
        add_noise(&mut b, 0, None, 0.5, 42);
        assert_eq!(a.render_checksum(), b.render_checksum());
        let v = a.layers[0].get_pixel(3, 3).to_rgba8().0;
        assert!(v != 128 && (64..=192).contains(&v), "noisy value {v}");
    }
}
