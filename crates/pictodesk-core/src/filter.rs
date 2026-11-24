//! Filters v1 — the classic Filter menu: Gaussian Blur,
//! Sharpen, Unsharp Mask, Add Noise. Filters read a snapshot of the
//! layer region first (no feedback smearing), then write back
//! blended by the selection mask.

use crate::document::{ColorMode, Document};
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

// ---- CMYK path: filters run on a 5-channel ink region (c, m, y, k, a).

struct InkRegion {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    px: Vec<[f32; 5]>,
}

fn read_ink_region(doc: &Document, layer: usize, rect: (i32, i32, u32, u32)) -> InkRegion {
    let (x, y, w, h) = rect;
    let mut px = Vec::with_capacity((w * h) as usize);
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            let (c, m, y2, k, a) = doc.get_cmyk(layer, x + xx, y + yy);
            let f = |v: u16| v as f32 / 65535.0;
            px.push([f(c), f(m), f(y2), f(k), f(a)]);
        }
    }
    InkRegion { x, y, w, h, px }
}

impl InkRegion {
    fn at(&self, x: i32, y: i32) -> [f32; 5] {
        let cx = x.clamp(0, self.w as i32 - 1) as u32;
        let cy = y.clamp(0, self.h as i32 - 1) as u32;
        self.px[(cy * self.w + cx) as usize]
    }
}

fn write_ink_region(doc: &mut Document, layer: usize, region: &InkRegion, sel: Option<&Selection>) {
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
            let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            if strength == 255 {
                doc.set_cmyk(
                    layer,
                    dx,
                    dy,
                    (e(out[0]), e(out[1]), e(out[2]), e(out[3]), e(out[4])),
                );
            } else {
                let orig = doc.get_cmyk(layer, dx, dy);
                let t = strength as f32 / 255.0;
                let l = |a: u16, b: f32| -> u16 {
                    let af = a as f32 / 65535.0;
                    e(af + (b - af) * t)
                };
                doc.set_cmyk(
                    layer,
                    dx,
                    dy,
                    (
                        l(orig.0, out[0]),
                        l(orig.1, out[1]),
                        l(orig.2, out[2]),
                        l(orig.3, out[3]),
                        l(orig.4, out[4]),
                    ),
                );
            }
        }
    }
}

fn blur_ink_region(src: &InkRegion, sigma: f32) -> InkRegion {
    let kernel = gaussian_kernel(sigma);
    let half = (kernel.len() / 2) as i32;
    let (w, h) = (src.w as i32, src.h as i32);
    let mut tmp = vec![[0.0f32; 5]; src.px.len()];
    let mut out = vec![[0.0f32; 5]; src.px.len()];
    for yy in 0..h {
        for xx in 0..w {
            let mut acc = [0.0f32; 5];
            for (ki, &kv) in kernel.iter().enumerate() {
                let p = src.at(xx + ki as i32 - half, yy);
                for c in 0..5 {
                    acc[c] += p[c] * kv;
                }
            }
            tmp[(yy * w + xx) as usize] = acc;
        }
    }
    for yy in 0..h {
        for xx in 0..w {
            let mut acc = [0.0f32; 5];
            for (ki, &kv) in kernel.iter().enumerate() {
                let p = tmp[((yy + ki as i32 - half).clamp(0, h - 1) * w + xx) as usize];
                for c in 0..5 {
                    acc[c] += p[c] * kv;
                }
            }
            out[(yy * w + xx) as usize] = acc;
        }
    }
    InkRegion {
        x: src.x,
        y: src.y,
        w: src.w,
        h: src.h,
        px: out,
    }
}

/// Filter > Blur > Gaussian Blur. `radius` is the classic radius:
/// roughly the blur's standard deviation in pixels.
pub fn gaussian_blur(doc: &mut Document, layer: usize, sel: Option<&Selection>, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    if doc.mode == ColorMode::Cmyk {
        let src = read_ink_region(doc, layer, work_rect(doc, sel));
        let blurred = blur_ink_region(&src, radius.max(0.1));
        write_ink_region(doc, layer, &blurred, sel);
        return;
    }
    let src = read_region(doc, layer, work_rect(doc, sel));
    let blurred = blur_region(&src, radius.max(0.1));
    write_region(doc, layer, &blurred, sel);
}

/// Filter > Sharpen > Sharpen: the classic 3x3 kernel.
pub fn sharpen(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    if doc.mode == ColorMode::Cmyk {
        let src = read_ink_region(doc, layer, work_rect(doc, sel));
        let mut out = src.px.clone();
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let c = src.at(xx, yy);
                let mut acc = [c[0] * 5.0, c[1] * 5.0, c[2] * 5.0, c[3] * 5.0];
                for (nx, ny) in [(xx - 1, yy), (xx + 1, yy), (xx, yy - 1), (xx, yy + 1)] {
                    let p = src.at(nx, ny);
                    for i in 0..4 {
                        acc[i] -= p[i];
                    }
                }
                let o = &mut out[(yy as u32 * src.w + xx as u32) as usize];
                o[0] = acc[0];
                o[1] = acc[1];
                o[2] = acc[2];
                o[3] = acc[3];
            }
        }
        write_ink_region(
            doc,
            layer,
            &InkRegion {
                x: src.x,
                y: src.y,
                w: src.w,
                h: src.h,
                px: out,
            },
            sel,
        );
        return;
    }
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
    if doc.mode == ColorMode::Cmyk {
        let src = read_ink_region(doc, layer, work_rect(doc, sel));
        let blurred = blur_ink_region(&src, radius.max(0.1));
        let thr = threshold as f32 / 255.0;
        let mut out = src.px.clone();
        for (i, o) in out.iter_mut().enumerate() {
            let sv = src.px[i];
            let bv = blurred.px[i];
            for c in 0..4 {
                let d = sv[c] - bv[c];
                o[c] = if d.abs() < thr {
                    sv[c]
                } else {
                    sv[c] + d * amount
                };
            }
        }
        write_ink_region(
            doc,
            layer,
            &InkRegion {
                x: src.x,
                y: src.y,
                w: src.w,
                h: src.h,
                px: out,
            },
            sel,
        );
        return;
    }
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
    if doc.mode == ColorMode::Cmyk {
        let src = read_ink_region(doc, layer, rect);
        let mut out = src.px.clone();
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                if src.px[i][4] <= 0.0 {
                    continue;
                }
                for (c, oc) in out[i].iter_mut().take(4).enumerate() {
                    *oc = src.px[i][c]
                        + (hash_unit(seed, rect.0 + xx, rect.1 + yy, c as u64) - 0.5) * amount;
                }
            }
        }
        write_ink_region(
            doc,
            layer,
            &InkRegion {
                x: rect.0,
                y: rect.1,
                w: rect.2,
                h: rect.3,
                px: out,
            },
            sel,
        );
        return;
    }
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

/// Run an RGB-space filter over the layer region, whatever the
/// document mode: CMYK documents round-trip their ink region
/// through RGB (the same route the Photo Filter adjustment takes),
/// so each filter below is implemented exactly once.
fn rgb_filter_rect(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    rect: (i32, i32, u32, u32),
    f: impl Fn(&Region) -> Region,
) {
    if doc.mode == ColorMode::Cmyk {
        let ink = read_ink_region(doc, layer, rect);
        let rgb = Region {
            x: ink.x,
            y: ink.y,
            w: ink.w,
            h: ink.h,
            px: ink
                .px
                .iter()
                .map(|p| {
                    let (r, g, b) = crate::color::cmyk_to_rgb(p[0], p[1], p[2], p[3]);
                    Pixel16::from_f32s(r, g, b, p[4])
                })
                .collect(),
        };
        let out = f(&rgb);
        let back = InkRegion {
            x: out.x,
            y: out.y,
            w: out.w,
            h: out.h,
            px: out
                .px
                .iter()
                .map(|p| {
                    let (r, g, b, a) = p.to_f32s();
                    let (c, m, y, k) = crate::color::rgb_to_cmyk(r, g, b);
                    [c, m, y, k, a]
                })
                .collect(),
        };
        write_ink_region(doc, layer, &back, sel);
        return;
    }
    if doc.mode == ColorMode::Lab {
        // Lab (Phase 6.5): decode slots to RGB, filter, encode
        // back — the same round-trip as the CMYK path above.
        let raw = read_region(doc, layer, rect);
        let rgb = Region {
            x: raw.x,
            y: raw.y,
            w: raw.w,
            h: raw.h,
            px: raw
                .px
                .iter()
                .map(|p| {
                    let (r, g, b, a) = p.to_f32s();
                    let (l, la, lb) = crate::color::slots_to_lab(r, g, b);
                    let (r, g, b) = crate::color::lab_to_rgb(l, la, lb);
                    Pixel16::from_f32s(r, g, b, a)
                })
                .collect(),
        };
        let out = f(&rgb);
        let back = Region {
            x: out.x,
            y: out.y,
            w: out.w,
            h: out.h,
            px: out
                .px
                .iter()
                .map(|p| {
                    let (r, g, b, a) = p.to_f32s();
                    let (l, la, lb) = crate::color::rgb_to_lab(r, g, b);
                    let (sr, sg, sb) = crate::color::lab_to_slots(l, la, lb);
                    Pixel16::from_f32s(sr, sg, sb, a)
                })
                .collect(),
        };
        write_region(doc, layer, &back, sel);
        return;
    }
    let src = read_region(doc, layer, rect);
    let out = f(&src);
    write_region(doc, layer, &out, sel);
}

fn rgb_filter(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    f: impl Fn(&Region) -> Region,
) {
    rgb_filter_rect(doc, layer, sel, work_rect(doc, sel), f);
}

/// Bilinear sample of a region at fractional coordinates.
fn sample_bilinear(src: &Region, fx: f32, fy: f32) -> (f32, f32, f32, f32) {
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let p = |x: i32, y: i32| src.at(x, y).to_f32s();
    let (a, b, c, d) = (p(x0, y0), p(x0 + 1, y0), p(x0, y0 + 1), p(x0 + 1, y0 + 1));
    let mix = |u: f32, v: f32, w: f32, z: f32| {
        u + (v - u) * tx + (w - u) * ty + (u - v - w + z) * tx * ty
    };
    (
        mix(a.0, b.0, c.0, d.0),
        mix(a.1, b.1, c.1, d.1),
        mix(a.2, b.2, c.2, d.2),
        mix(a.3, b.3, c.3, d.3),
    )
}

fn blank_like(src: &Region) -> Region {
    Region {
        x: src.x,
        y: src.y,
        w: src.w,
        h: src.h,
        px: src.px.clone(),
    }
}

/// Filter > Blur > Motion Blur: smear along `angle` degrees for
/// `distance` pixels.
pub fn motion_blur(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    angle: f32,
    distance: f32,
) {
    if distance <= 0.0 {
        return;
    }
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (dx, dy) = (angle.to_radians().cos(), angle.to_radians().sin());
        let steps = (distance.ceil() as i32).max(1);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for i in 0..=steps {
                    let t = (i as f32 / steps as f32 - 0.5) * distance;
                    let p = sample_bilinear(src, xx as f32 + dx * t, yy as f32 + dy * t);
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                    acc.3 += p.3;
                }
                let n = (steps + 1) as f32;
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(acc.0 / n, acc.1 / n, acc.2 / n, acc.3 / n);
            }
        }
        out
    });
}

/// Filter > Blur > Radial Blur (spin method): rotate samples
/// around the region center; `amount` 1..=100 scales the swirl.
pub fn radial_blur(doc: &mut Document, layer: usize, sel: Option<&Selection>, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (cx, cy) = ((src.w as f32 - 1.0) / 2.0, (src.h as f32 - 1.0) / 2.0);
        let max_rad = amount * 0.0035; // 100 -> ~20 degrees of swirl
        const SAMPLES: i32 = 10;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (ox, oy) = (xx as f32 - cx, yy as f32 - cy);
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for k in 0..SAMPLES {
                    let a = (k as f32 / (SAMPLES - 1) as f32 - 0.5) * max_rad;
                    let (s, c) = (a.sin(), a.cos());
                    let p = sample_bilinear(src, cx + ox * c - oy * s, cy + ox * s + oy * c);
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                    acc.3 += p.3;
                }
                let n = SAMPLES as f32;
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(acc.0 / n, acc.1 / n, acc.2 / n, acc.3 / n);
            }
        }
        out
    });
}

fn median_of(vals: &mut [f32]) -> f32 {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vals[vals.len() / 2]
}

/// Filter > Noise > Median: per-channel median over a
/// (2*radius+1) square window. Removes impulse noise while
/// keeping edges.
pub fn median(doc: &mut Document, layer: usize, sel: Option<&Selection>, radius: u32) {
    if radius == 0 {
        return;
    }
    let r = radius.min(10) as i32;
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let mut win: Vec<f32> = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut chans: [f32; 3] = [0.0; 3];
                for c in 0..3 {
                    win.clear();
                    for wy in -r..=r {
                        for wx in -r..=r {
                            let p = src.at(xx + wx, yy + wy).to_f32s();
                            win.push([p.0, p.1, p.2][c]);
                        }
                    }
                    chans[c] = median_of(&mut win);
                }
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(chans[0], chans[1], chans[2], a);
            }
        }
        out
    });
}

/// Filter > Noise > Despeckle: a 3x3 median applied only where a
/// pixel disagrees strongly with its neighborhood — specks go,
/// detail stays.
pub fn despeckle(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let mut win: Vec<f32> = Vec::with_capacity(9);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let c = src.at(xx, yy).to_f32s();
                let mut med = [0.0f32; 3];
                for ch in 0..3 {
                    win.clear();
                    for wy in -1..=1 {
                        for wx in -1..=1 {
                            let p = src.at(xx + wx, yy + wy).to_f32s();
                            win.push([p.0, p.1, p.2][ch]);
                        }
                    }
                    med[ch] = median_of(&mut win);
                }
                let dev = (c.0 - med[0])
                    .abs()
                    .max((c.1 - med[1]).abs())
                    .max((c.2 - med[2]).abs());
                if dev > 0.1 {
                    out.px[(yy as u32 * src.w + xx as u32) as usize] =
                        Pixel16::from_f32s(med[0], med[1], med[2], c.3);
                }
            }
        }
        out
    });
}

/// Filter > Stylize > Emboss: a directional relief of the
/// luminance, hovering at mid-gray on flat areas. `amount`
/// 0.0..=2.0 scales the relief.
pub fn emboss(doc: &mut Document, layer: usize, sel: Option<&Selection>, amount: f32) {
    rgb_filter(doc, layer, sel, |src| {
        let lum = |x: i32, y: i32| {
            let p = src.at(x, y).to_f32s();
            0.3 * p.0 + 0.59 * p.1 + 0.11 * p.2
        };
        let mut out = blank_like(src);
        const K: [[f32; 3]; 3] = [[-2.0, -1.0, 0.0], [-1.0, 1.0, 1.0], [0.0, 1.0, 2.0]];
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut conv = 0.0f32;
                for ky in 0..3i32 {
                    for kx in 0..3i32 {
                        conv += K[ky as usize][kx as usize] * lum(xx + kx - 1, yy + ky - 1);
                    }
                }
                let v = 0.5 + (conv - lum(xx, yy)) * amount;
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] = Pixel16::from_f32s(v, v, v, a);
            }
        }
        out
    });
}

/// Filter > Stylize > Find Edges: Sobel gradient magnitude per
/// channel, inverted — white where the image is flat, colored
/// lines where it changes.
pub fn find_edges(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut rgb = [0.0f32; 3];
                for (ch, v) in rgb.iter_mut().enumerate() {
                    let g = |x: i32, y: i32| {
                        let p = src.at(x, y).to_f32s();
                        [p.0, p.1, p.2][ch]
                    };
                    let gx = (g(xx + 1, yy - 1) + 2.0 * g(xx + 1, yy) + g(xx + 1, yy + 1))
                        - (g(xx - 1, yy - 1) + 2.0 * g(xx - 1, yy) + g(xx - 1, yy + 1));
                    let gy = (g(xx - 1, yy + 1) + 2.0 * g(xx, yy + 1) + g(xx + 1, yy + 1))
                        - (g(xx - 1, yy - 1) + 2.0 * g(xx, yy - 1) + g(xx + 1, yy - 1));
                    *v = 1.0 - ((gx.abs() + gy.abs()) * 0.5).min(1.0);
                }
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(rgb[0], rgb[1], rgb[2], a);
            }
        }
        out
    });
}

/// Filter > Stylize > Solarize: tones above mid-gray invert,
/// tones below pass through.
pub fn solarize(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for (i, p) in src.px.iter().enumerate() {
            let (r, g, b, a) = p.to_f32s();
            let s = |v: f32| if v <= 0.5 { v } else { 1.0 - v };
            out.px[i] = Pixel16::from_f32s(s(r), s(g), s(b), a);
        }
        out
    });
}

fn value_noise(seed: u64, x: f32, y: f32, cell: f32) -> f32 {
    let (gx, gy) = (x / cell, y / cell);
    let (ix, iy) = (gx.floor() as i32, gy.floor() as i32);
    let (fx, fy) = (gx - ix as f32, gy - iy as f32);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let lat = |x: i32, y: i32| hash_unit(seed, x, y, 7);
    let (a, b, c, d) = (
        lat(ix, iy),
        lat(ix + 1, iy),
        lat(ix, iy + 1),
        lat(ix + 1, iy + 1),
    );
    a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy
}

/// Filter > Render > Clouds: fractal value-noise rendered between
/// the foreground and background colors. Deterministic per seed;
/// the existing alpha is preserved.
pub fn clouds(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    fg: Pixel16,
    bg: Pixel16,
    seed: u64,
) {
    let (f, b) = (fg.to_f32s(), bg.to_f32s());
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (ax, ay) = ((src.x + xx) as f32, (src.y + yy) as f32);
                let mut n = 0.0f32;
                let mut amp = 0.5f32;
                let mut cell = 96.0f32;
                let mut norm = 0.0f32;
                for _ in 0..4 {
                    n += amp * value_noise(seed, ax, ay, cell);
                    norm += amp;
                    amp *= 0.5;
                    cell *= 0.5;
                }
                let n = n / norm;
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] = Pixel16::from_f32s(
                    b.0 + (f.0 - b.0) * n,
                    b.1 + (f.1 - b.1) * n,
                    b.2 + (f.2 - b.2) * n,
                    a,
                );
            }
        }
        out
    });
}

/// Filter > Pixelate > Mosaic: average each `cell`-pixel square
/// (aligned to the document grid) into one flat tile of color.
pub fn mosaic(doc: &mut Document, layer: usize, sel: Option<&Selection>, cell: u32) {
    if cell <= 1 {
        return;
    }
    let cell = cell as i32;
    rgb_filter(doc, layer, sel, |src| {
        use std::collections::HashMap;
        // Per-cell channel sums; element 4 is the pixel count.
        let mut sums: HashMap<(i32, i32), [f32; 5]> = HashMap::new();
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let key = ((src.x + xx).div_euclid(cell), (src.y + yy).div_euclid(cell));
                let p = src.px[(yy as u32 * src.w + xx as u32) as usize].to_f32s();
                let e = sums.entry(key).or_default();
                e[0] += p.0;
                e[1] += p.1;
                e[2] += p.2;
                e[3] += p.3;
                e[4] += 1.0;
            }
        }
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let key = ((src.x + xx).div_euclid(cell), (src.y + yy).div_euclid(cell));
                let e = sums[&key];
                let n = e[4];
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(e[0] / n, e[1] / n, e[2] / n, e[3] / n);
            }
        }
        out
    });
}

/// Filter > Other > High Pass: keeps the detail, discards the
/// tones — pixel minus its blur, parked on mid-gray.
pub fn high_pass(doc: &mut Document, layer: usize, sel: Option<&Selection>, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    rgb_filter(doc, layer, sel, |src| {
        let blurred = blur_region(src, radius.max(0.1));
        let mut out = blank_like(src);
        for (i, p) in src.px.iter().enumerate() {
            let s = p.to_f32s();
            let b = blurred.px[i].to_f32s();
            out.px[i] = Pixel16::from_f32s(s.0 - b.0 + 0.5, s.1 - b.1 + 0.5, s.2 - b.2 + 0.5, s.3);
        }
        out
    });
}

/// Filter > Other > Offset: shift the layer's content by
/// (dx, dy) pixels, wrapping around the document edges.
pub fn offset_filter(doc: &mut Document, layer: usize, sel: Option<&Selection>, dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }
    let full = (0, 0, doc.width, doc.height);
    rgb_filter_rect(doc, layer, sel, full, |src| {
        let mut out = blank_like(src);
        let (w, h) = (src.w as i32, src.h as i32);
        for yy in 0..h {
            for xx in 0..w {
                let sx = (xx - dx).rem_euclid(w);
                let sy = (yy - dy).rem_euclid(h);
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    src.px[(sy as u32 * src.w + sx as u32) as usize];
            }
        }
        out
    });
}

/// Filter > Other > Custom: a user 5x5 convolution kernel;
/// result = sum(kernel x neighborhood) / scale + offset
/// (offset in 0.0..=1.0).
pub fn custom_filter(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    kernel: [f32; 25],
    scale: f32,
    offset: f32,
) {
    let scale = if scale == 0.0 { 1.0 } else { scale };
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut acc = [0.0f32; 3];
                for ky in 0..5i32 {
                    for kx in 0..5i32 {
                        let kv = kernel[(ky as usize) * 5 + kx as usize];
                        if kv == 0.0 {
                            continue;
                        }
                        let p = src.at(xx + kx - 2, yy + ky - 2).to_f32s();
                        acc[0] += kv * p.0;
                        acc[1] += kv * p.1;
                        acc[2] += kv * p.2;
                    }
                }
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] = Pixel16::from_f32s(
                    acc[0] / scale + offset,
                    acc[1] / scale + offset,
                    acc[2] / scale + offset,
                    a,
                );
            }
        }
        out
    });
}

// ---- Phase 5 quick wins: the classic gallery's small builds —
// the Distort remappers, the Noise/Other utilities, and the
// one-pass Pixelate / Stylize / Texture / Sketch effects.

/// Bilinear sample where taps outside the region contribute
/// transparent black instead of the clamped edge pixel — the
/// Distort filters can pull content from outside the frame, and
/// outside the frame there is nothing.
fn sample_clipped(src: &Region, fx: f32, fy: f32) -> (f32, f32, f32, f32) {
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let tap = |x: i32, y: i32| -> (f32, f32, f32, f32) {
        if x < 0 || y < 0 || x >= src.w as i32 || y >= src.h as i32 {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            src.px[(y as u32 * src.w + x as u32) as usize].to_f32s()
        }
    };
    let (a, b, c, d) = (
        tap(x0, y0),
        tap(x0 + 1, y0),
        tap(x0, y0 + 1),
        tap(x0 + 1, y0 + 1),
    );
    let mix = |u: f32, v: f32, w: f32, z: f32| {
        u + (v - u) * tx + (w - u) * ty + (u - v - w + z) * tx * ty
    };
    (
        mix(a.0, b.0, c.0, d.0),
        mix(a.1, b.1, c.1, d.1),
        mix(a.2, b.2, c.2, d.2),
        mix(a.3, b.3, c.3, d.3),
    )
}

fn luma(p: (f32, f32, f32, f32)) -> f32 {
    0.3 * p.0 + 0.59 * p.1 + 0.11 * p.2
}

fn luma_plane(src: &Region) -> Vec<f32> {
    src.px.iter().map(|p| luma(p.to_f32s())).collect()
}

/// Two-pass box blur of one channel plane, edges clamped.
fn box_blur_plane(values: &[f32], w: u32, h: u32, radius: u32) -> Vec<f32> {
    if radius == 0 {
        return values.to_vec();
    }
    let r = radius as i32;
    let (w, h) = (w as i32, h as i32);
    let at = |v: &[f32], x: i32, y: i32| v[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    let n = (2 * r + 1) as f32;
    let mut tmp = vec![0.0f32; values.len()];
    let mut out = vec![0.0f32; values.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for k in -r..=r {
                acc += at(values, x + k, y);
            }
            tmp[(y * w + x) as usize] = acc / n;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for k in -r..=r {
                acc += at(&tmp, x, y + k);
            }
            out[(y * w + x) as usize] = acc / n;
        }
    }
    out
}

fn box_blur_region(src: &Region, radius: u32) -> Region {
    let mut planes: [Vec<f32>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for (ch, plane) in planes.iter_mut().enumerate() {
        let raw: Vec<f32> = src
            .px
            .iter()
            .map(|p| {
                let f = p.to_f32s();
                [f.0, f.1, f.2, f.3][ch]
            })
            .collect();
        *plane = box_blur_plane(&raw, src.w, src.h, radius);
    }
    let mut out = blank_like(src);
    for (i, px) in out.px.iter_mut().enumerate() {
        *px = Pixel16::from_f32s(planes[0][i], planes[1][i], planes[2][i], planes[3][i]);
    }
    out
}

/// Sobel gradient magnitude of a luma plane, normalized to
/// 0.0..=1.0 the same way Find Edges normalizes it.
fn sobel_luma(lum: &[f32], w: u32, h: u32, x: i32, y: i32) -> f32 {
    let (w, h) = (w as i32, h as i32);
    let at = |xx: i32, yy: i32| lum[(yy.clamp(0, h - 1) * w + xx.clamp(0, w - 1)) as usize];
    let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
        - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
    let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
        - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
    ((gx.abs() + gy.abs()) * 0.5).min(1.0)
}

/// Shared driver for the Distort family: `map` gives the source
/// coordinates each output pixel samples from.
fn distort(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    map: impl Fn(&Region, i32, i32) -> (f32, f32),
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (fx, fy) = map(src, xx, yy);
                let p = sample_clipped(src, fx, fy);
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(p.0, p.1, p.2, p.3);
            }
        }
        out
    });
}

/// (center x, center y, max radius) of a region, in local
/// coordinates — the Distort family's shared geometry.
fn distort_frame(src: &Region) -> (f32, f32, f32) {
    (
        (src.w as f32 - 1.0) / 2.0,
        (src.h as f32 - 1.0) / 2.0,
        src.w.min(src.h) as f32 * 0.5,
    )
}

/// Filter > Distort > Pinch: pulls the image toward the center
/// (amount > 0) or pushes it away (amount < 0), the pull fading
/// quadratically toward the rim of the center circle.
pub fn pinch(doc: &mut Document, layer: usize, sel: Option<&Selection>, amount: f32) {
    let amount = amount.clamp(-1.0, 1.0);
    distort(doc, layer, sel, move |src, xx, yy| {
        let (cx, cy, r_max) = distort_frame(src);
        let (dx, dy) = (xx as f32 - cx, yy as f32 - cy);
        let dist = (dx * dx + dy * dy).sqrt();
        let r = dist / r_max;
        if dist == 0.0 || r >= 1.0 {
            return (xx as f32, yy as f32);
        }
        let factor = 1.0 + amount * (1.0 - r) * (1.0 - r);
        let sr = dist * factor;
        (cx + dx / dist * sr, cy + dy / dist * sr)
    });
}

/// Filter > Distort > Spherize: the image bulges (amount > 0)
/// or dents (amount < 0) as if mapped onto a sphere — Pinch's
/// geometry with a (1 - r^2) falloff.
pub fn spherize(doc: &mut Document, layer: usize, sel: Option<&Selection>, amount: f32) {
    let amount = amount.clamp(-1.0, 1.0);
    distort(doc, layer, sel, move |src, xx, yy| {
        let (cx, cy, r_max) = distort_frame(src);
        let (dx, dy) = (xx as f32 - cx, yy as f32 - cy);
        let dist = (dx * dx + dy * dy).sqrt();
        let r = dist / r_max;
        if dist == 0.0 || r >= 1.0 {
            return (xx as f32, yy as f32);
        }
        let factor = 1.0 + amount * (1.0 - r * r) * 0.5;
        let sr = dist * factor;
        (cx + dx / dist * sr, cy + dy / dist * sr)
    });
}

/// Filter > Distort > Twirl: rotates pixels around the center,
/// the rotation strongest at the center and fading to zero at
/// the rim of the center circle. `angle_deg` may be negative.
pub fn twirl(doc: &mut Document, layer: usize, sel: Option<&Selection>, angle_deg: f32) {
    distort(doc, layer, sel, move |src, xx, yy| {
        let (cx, cy, r_max) = distort_frame(src);
        let (dx, dy) = (xx as f32 - cx, yy as f32 - cy);
        let dist = (dx * dx + dy * dy).sqrt();
        let r = dist / r_max;
        if dist == 0.0 || r >= 1.0 {
            return (xx as f32, yy as f32);
        }
        let a = angle_deg.to_radians() * (1.0 - r) * (1.0 - r);
        let (s, c) = a.sin_cos();
        (cx + dx * c - dy * s, cy + dx * s + dy * c)
    });
}

/// Filter > Distort > Ripple: a regular sine-wave wobble.
/// `amount` -100..=100 (a tenth of it is the displacement in
/// pixels), `size_px` the wavelength.
pub fn ripple(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    amount: f32,
    size_px: f32,
) {
    let px = amount * 0.1;
    let size = if size_px <= 0.0 { 1.0 } else { size_px };
    distort(doc, layer, sel, move |_src, xx, yy| {
        let tau = std::f32::consts::TAU;
        let dx = px * (tau * yy as f32 / size).sin();
        let dy = px * (tau * xx as f32 / size).sin();
        (xx as f32 + dx, yy as f32 + dy)
    });
}

/// Filter > Distort > Ocean Ripple: Ripple's irregular cousin —
/// three sine waves with hash-derived directions and phases,
/// phased on document coordinates so the pattern does not move
/// with the selection. `magnitude` scales the displacement
/// (a fifth of it, in pixels, is the most a pixel can move).
pub fn ocean_ripple(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size_px: f32,
    magnitude: f32,
) {
    let mag = magnitude * 0.2;
    let size = if size_px <= 0.0 { 1.0 } else { size_px };
    let tau = std::f32::consts::TAU;
    let mut waves = [(0.0f32, 0.0f32, 0.0f32); 3];
    for (k, wv) in waves.iter_mut().enumerate() {
        let ang = hash_unit(7, k as i32, 0, 41) * tau;
        let phase = hash_unit(7, k as i32, 0, 42) * tau;
        *wv = (ang.cos(), ang.sin(), phase);
    }
    distort(doc, layer, sel, move |src, xx, yy| {
        let (ax, ay) = ((src.x + xx) as f32, (src.y + yy) as f32);
        let (mut dx, mut dy) = (0.0f32, 0.0f32);
        for wv in &waves {
            let (dirx, diry, phase) = *wv;
            let s = (tau * (ax * dirx + ay * diry) / size + phase).sin();
            dx += dirx * s;
            dy += diry * s;
        }
        (xx as f32 + dx * mag / 3.0, yy as f32 + dy * mag / 3.0)
    });
}

/// Filter > Distort > Polar Coordinates: wraps the rectangle
/// around a circle (rectangular to polar) or unwraps it
/// (polar to rectangular).
pub fn polar_coordinates(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    to_polar: bool,
) {
    distort(doc, layer, sel, move |src, xx, yy| {
        let (cx, cy) = ((src.w as f32 - 1.0) / 2.0, (src.h as f32 - 1.0) / 2.0);
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);
        let tau = std::f32::consts::TAU;
        if to_polar {
            let theta = xx as f32 / src.w as f32 * tau;
            let r = yy as f32 / src.h as f32 * max_r;
            (cx + r * theta.cos(), cy + r * theta.sin())
        } else {
            let (dx, dy) = (xx as f32 - cx, yy as f32 - cy);
            let r = (dx * dx + dy * dy).sqrt() / max_r;
            let mut theta = dy.atan2(dx);
            if theta < 0.0 {
                theta += tau;
            }
            (theta / tau * src.w as f32, r * src.h as f32)
        }
    });
}

/// Filter > Distort > Shear: each row shifts horizontally by
/// an amount interpolated from five control values — fractions
/// of the region width (-0.25..=0.25) at y fractions 0, .25,
/// .5, .75, 1.
pub fn shear(doc: &mut Document, layer: usize, sel: Option<&Selection>, offsets: [f32; 5]) {
    distort(doc, layer, sel, move |src, xx, yy| {
        let t = if src.h > 1 {
            yy as f32 / (src.h - 1) as f32
        } else {
            0.0
        };
        let pos = (t * 4.0).min(3.999);
        let i0 = pos.floor() as usize;
        let frac = pos - i0 as f32;
        let shift = offsets[i0] + (offsets[i0 + 1] - offsets[i0]) * frac;
        (xx as f32 - shift * src.w as f32, yy as f32)
    });
}

/// Filter > Distort > ZigZag: concentric ripples around the
/// center. Styles: 0 = Around Center (displacement swings both
/// ways), 1 = Out From Center (outward only), 2 = Pond Ripples
/// (the phase runs on the diagonal instead of the radius).
pub fn zigzag(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    amount: f32,
    ridges: u32,
    style: u8,
) {
    let amp = amount * 0.1;
    distort(doc, layer, sel, move |src, xx, yy| {
        let (cx, cy, r_max) = distort_frame(src);
        let (dx, dy) = (xx as f32 - cx, yy as f32 - cy);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist == 0.0 {
            return (xx as f32, yy as f32);
        }
        let tau = std::f32::consts::TAU;
        let phase = if style == 2 {
            ((xx + yy) as f32 / r_max) * ridges as f32 * tau
        } else {
            (dist / r_max) * ridges as f32 * tau
        };
        let s = phase.sin();
        let disp = if style == 1 {
            (0.5 + 0.5 * s) * amp
        } else {
            s * amp
        };
        let sr = dist + disp;
        (cx + dx / dist * sr, cy + dy / dist * sr)
    });
}

/// Filter > Render > Difference Clouds: the Clouds noise field
/// again (same value-noise construction, same seed behaviour),
/// but each channel becomes its absolute difference from the
/// noise gray — psychedelic edges where tones cross the field.
pub fn difference_clouds(doc: &mut Document, layer: usize, sel: Option<&Selection>, seed: u64) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                if p.3 <= 0.0 {
                    continue;
                }
                let (ax, ay) = ((src.x + xx) as f32, (src.y + yy) as f32);
                let mut n = 0.0f32;
                let mut amp = 0.5f32;
                let mut cell = 96.0f32;
                let mut norm = 0.0f32;
                for _ in 0..4 {
                    n += amp * value_noise(seed, ax, ay, cell);
                    norm += amp;
                    amp *= 0.5;
                    cell *= 0.5;
                }
                let g = n / norm;
                out.px[i] =
                    Pixel16::from_f32s((p.0 - g).abs(), (p.1 - g).abs(), (p.2 - g).abs(), p.3);
            }
        }
        out
    });
}

/// The darkest (or lightest) of a pixel and its four direct
/// neighbors, by luminance — Diffuse's Darken/Lighten Only.
fn extreme_neighbor(src: &Region, x: i32, y: i32, darkest: bool) -> Pixel16 {
    let mut best = src.at(x, y);
    let mut best_l = luma(best.to_f32s());
    for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
        let p = src.at(nx, ny);
        let l = luma(p.to_f32s());
        if (darkest && l < best_l) || (!darkest && l > best_l) {
            best = p;
            best_l = l;
        }
    }
    best
}

/// Filter > Stylize > Diffuse: each pixel is replaced by one of
/// its four direct neighbors, which breaks up hard edges.
/// Modes: 0 = Normal (a hash-picked neighbor), 1 = Darken Only,
/// 2 = Lighten Only (the extreme of the neighbors and the
/// pixel itself), 3 = Anisotropic (left/right neighbors only).
pub fn diffuse(doc: &mut Document, layer: usize, sel: Option<&Selection>, mode: u8, seed: u64) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let (ax, ay) = (src.x + xx, src.y + yy);
                out.px[i] = match mode {
                    1 => extreme_neighbor(src, xx, yy, true),
                    2 => extreme_neighbor(src, xx, yy, false),
                    3 => {
                        if hash_unit(seed, ax, ay, 51) < 0.5 {
                            src.at(xx - 1, yy)
                        } else {
                            src.at(xx + 1, yy)
                        }
                    }
                    _ => {
                        let pick = (hash_unit(seed, ax, ay, 50) * 4.0) as usize % 4;
                        [
                            src.at(xx - 1, yy),
                            src.at(xx + 1, yy),
                            src.at(xx, yy - 1),
                            src.at(xx, yy + 1),
                        ][pick]
                    }
                };
            }
        }
        out
    });
}

/// Shared engine for Maximum / Minimum: per-channel extreme
/// over a square window, alpha included.
fn extreme_filter(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    radius: u32,
    is_max: bool,
) {
    rgb_filter(doc, layer, sel, |src| {
        let r = radius as i32;
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let c = src.at(xx, yy).to_f32s();
                let mut acc = [c.0, c.1, c.2, c.3];
                for wy in -r..=r {
                    for wx in -r..=r {
                        if wx == 0 && wy == 0 {
                            continue;
                        }
                        let p = src.at(xx + wx, yy + wy).to_f32s();
                        let pv = [p.0, p.1, p.2, p.3];
                        for (a, v) in acc.iter_mut().zip(pv) {
                            *a = if is_max { a.max(v) } else { a.min(v) };
                        }
                    }
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(acc[0], acc[1], acc[2], acc[3]);
            }
        }
        out
    });
}

/// Filter > Other > Maximum: spreads the bright areas —
/// per-channel maximum over a (2*radius+1) square.
pub fn maximum(doc: &mut Document, layer: usize, sel: Option<&Selection>, radius: u32) {
    extreme_filter(doc, layer, sel, radius, true);
}

/// Filter > Other > Minimum: spreads the dark areas —
/// per-channel minimum over a (2*radius+1) square.
pub fn minimum(doc: &mut Document, layer: usize, sel: Option<&Selection>, radius: u32) {
    extreme_filter(doc, layer, sel, radius, false);
}

/// Filter > Noise > Dust & Scratches: a median filter gated by
/// `threshold` (0.0..=1.0) — a pixel is replaced by its
/// neighborhood median only where it disagrees with that
/// median by more than the threshold, so specks are removed
/// and agreed-upon detail is left exactly alone.
pub fn dust_and_scratches(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    radius: u32,
    threshold: f32,
) {
    let r = radius.min(10) as i32;
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let mut win: Vec<f32> = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let c = src.at(xx, yy).to_f32s();
                let mut med = [0.0f32; 3];
                for ch in 0..3 {
                    win.clear();
                    for wy in -r..=r {
                        for wx in -r..=r {
                            let p = src.at(xx + wx, yy + wy).to_f32s();
                            win.push([p.0, p.1, p.2][ch]);
                        }
                    }
                    med[ch] = median_of(&mut win);
                }
                let dev = (c.0 - med[0])
                    .abs()
                    .max((c.1 - med[1]).abs())
                    .max((c.2 - med[2]).abs());
                if dev > threshold {
                    out.px[(yy as u32 * src.w + xx as u32) as usize] =
                        Pixel16::from_f32s(med[0], med[1], med[2], c.3);
                }
            }
        }
        out
    });
}

/// Filter > Sharpen > Sharpen Edges: unsharp masking that only
/// fires where the Sobel edge detector sees an edge — flat
/// areas keep their noise exactly as it was.
pub fn sharpen_edges(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let blurred = box_blur_region(src, 1);
        let lum = luma_plane(src);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let gain = (sobel_luma(&lum, src.w, src.h, xx, yy) * 3.0).clamp(0.0, 1.0);
                let s = src.px[i].to_f32s();
                let b = blurred.px[i].to_f32s();
                out.px[i] = Pixel16::from_f32s(
                    s.0 + (s.0 - b.0) * gain,
                    s.1 + (s.1 - b.1) * gain,
                    s.2 + (s.2 - b.2) * gain,
                    s.3,
                );
            }
        }
        out
    });
}

/// Filter > Pixelate > Fragment: averages each pixel with the
/// samples four pixels away on both diagonals — a soft,
/// four-ghost shimmer.
pub fn fragment(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for (ox, oy) in [(0, 0), (4, 4), (4, -4), (-4, 4), (-4, -4)] {
                    let p = src.at(xx + ox, yy + oy).to_f32s();
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                    acc.3 += p.3;
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(acc.0 / 5.0, acc.1 / 5.0, acc.2 / 5.0, acc.3 / 5.0);
            }
        }
        out
    });
}

/// Filter > Stylize > Tiles: breaks the region into a
/// count x count grid of tiles and shifts each tile's sampling
/// by a hash-random offset (up to `max_offset` of the tile
/// size). Where a tile's shifted sample falls outside the
/// tile's own square, the gap shows through as transparency.
pub fn tiles(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    count: u32,
    max_offset: f32,
    seed: u64,
) {
    if count == 0 {
        return;
    }
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (tw, th) = (src.w as f32 / count as f32, src.h as f32 / count as f32);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let tx = ((xx as f32 / tw).floor() as i32).min(count as i32 - 1);
                let ty = ((yy as f32 / th).floor() as i32).min(count as i32 - 1);
                let (gx, gy) = (
                    src.x + (tx as f32 * tw) as i32,
                    src.y + (ty as f32 * th) as i32,
                );
                let ox = (hash_unit(seed, gx, gy, 61) * 2.0 - 1.0) * max_offset * tw;
                let oy = (hash_unit(seed, gx, gy, 62) * 2.0 - 1.0) * max_offset * th;
                let (sx, sy) = (xx as f32 + ox, yy as f32 + oy);
                let (x0, x1) = (tx as f32 * tw, (tx + 1) as f32 * tw);
                let (y0, y1) = (ty as f32 * th, (ty + 1) as f32 * th);
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    if sx < x0 || sx >= x1 || sy < y0 || sy >= y1 {
                        Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0)
                    } else {
                        let p = sample_bilinear(src, sx, sy);
                        Pixel16::from_f32s(p.0, p.1, p.2, p.3)
                    };
            }
        }
        out
    });
}

/// Filter > Stylize > Glowing Edges: the image's edges light up
/// in its own colors on black. `width` widens the glow,
/// `brightness` scales it, `smoothness` softens the result
/// with a box blur pass (two at the highest settings).
pub fn glowing_edges(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    width: u32,
    brightness: f32,
    smoothness: u32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let lum = luma_plane(src);
        let mut wide: Vec<f32> = (0..src.px.len())
            .map(|i| {
                sobel_luma(
                    &lum,
                    src.w,
                    src.h,
                    (i as u32 % src.w) as i32,
                    (i as u32 / src.w) as i32,
                )
            })
            .collect();
        // Widen: a maximum filter over the edge plane.
        let r = width.saturating_sub(1).min(3) as i32;
        if r > 0 {
            let eplane = wide.clone();
            for yy in 0..src.h as i32 {
                for xx in 0..src.w as i32 {
                    let mut m = 0.0f32;
                    for wy in -r..=r {
                        for wx in -r..=r {
                            let cx = (xx + wx).clamp(0, src.w as i32 - 1);
                            let cy = (yy + wy).clamp(0, src.h as i32 - 1);
                            m = m.max(eplane[(cy as u32 * src.w + cx as u32) as usize]);
                        }
                    }
                    wide[(yy as u32 * src.w + xx as u32) as usize] = m;
                }
            }
        }
        let mut out = blank_like(src);
        for (i, p) in src.px.iter().enumerate() {
            let s = p.to_f32s();
            let e = wide[i];
            let peak = s.0.max(s.1).max(s.2);
            let (nr, ng, nb) = if peak > 0.0 {
                (s.0 / peak, s.1 / peak, s.2 / peak)
            } else {
                (0.0, 0.0, 0.0)
            };
            out.px[i] = Pixel16::from_f32s(
                nr * e * brightness,
                ng * e * brightness,
                nb * e * brightness,
                s.3.max(e),
            );
        }
        if smoothness >= 16 {
            box_blur_region(&box_blur_region(&out, 1), 1)
        } else if smoothness >= 8 {
            box_blur_region(&out, 1)
        } else {
            out
        }
    });
}

/// Filter > Stylize > Wind: bright streaks smear sideways.
/// Walking in the wind direction, a carried value decays by a
/// fixed factor per step and the output is the max of the pixel
/// and the decayed carry — streaks break where the hash says
/// so. Methods: 0 = Wind (decay 0.92), 1 = Blast (decay 0.96
/// with occasional double-steps, so streaks run longer and
/// rougher).
pub fn wind(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    method: u8,
    from_left: bool,
    seed: u64,
) {
    let decay = if method == 1 { 0.96 } else { 0.92 };
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let w = src.w as i32;
        for yy in 0..src.h as i32 {
            let mut carried = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for step in 0..w {
                let xx = if from_left { step } else { w - 1 - step };
                let (ax, ay) = (src.x + xx, src.y + yy);
                if hash_unit(seed, ax, ay, 71) < 0.05 {
                    carried = (0.0, 0.0, 0.0, 0.0);
                }
                if method == 1 && hash_unit(seed, ax, ay, 72) < 0.15 {
                    carried = (
                        carried.0 * decay,
                        carried.1 * decay,
                        carried.2 * decay,
                        carried.3 * decay,
                    );
                }
                let s = src.at(xx, yy).to_f32s();
                let o = (
                    s.0.max(carried.0 * decay),
                    s.1.max(carried.1 * decay),
                    s.2.max(carried.2 * decay),
                    s.3.max(carried.3 * decay),
                );
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(o.0, o.1, o.2, o.3);
                carried = o;
            }
        }
        out
    });
}

/// Filter > Texture > Patchwork: the image rebuilt from flat
/// squares of `size` pixels (aligned to the document grid),
/// each square filled with its average color and its
/// brightness nudged by a per-square hash — `relief` scales
/// the nudge, like light raking across tiles.
pub fn patchwork(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: u32,
    relief: u32,
) {
    let size = size.max(1) as i32;
    rgb_filter(doc, layer, sel, |src| {
        use std::collections::HashMap;
        // Per-square channel sums; element 4 is the pixel count.
        let mut sums: HashMap<(i32, i32), [f32; 5]> = HashMap::new();
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let key = ((src.x + xx).div_euclid(size), (src.y + yy).div_euclid(size));
                let p = src.px[(yy as u32 * src.w + xx as u32) as usize].to_f32s();
                let e = sums.entry(key).or_default();
                e[0] += p.0;
                e[1] += p.1;
                e[2] += p.2;
                e[3] += p.3;
                e[4] += 1.0;
            }
        }
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let key = ((src.x + xx).div_euclid(size), (src.y + yy).div_euclid(size));
                let e = sums[&key];
                let n = e[4];
                let shade = 1.0 + (hash_unit(77, key.0, key.1, 4) - 0.5) * relief as f32 * 0.02;
                out.px[(yy as u32 * src.w + xx as u32) as usize] = Pixel16::from_f32s(
                    e[0] / n * shade,
                    e[1] / n * shade,
                    e[2] / n * shade,
                    e[3] / n,
                );
            }
        }
        out
    });
}

/// Filter > Sketch > Stamp: a two-tone woodcut — the smoothed
/// luminance (box-blurred by `smoothness`, capped at radius 8)
/// thresholded at `balance` (0.0..=1.0): light becomes white,
/// dark becomes black, alpha is preserved.
pub fn stamp_effect(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    balance: f32,
    smoothness: u32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let lum = luma_plane(src);
        let blurred = box_blur_plane(&lum, src.w, src.h, smoothness.min(8));
        let mut out = blank_like(src);
        for (i, p) in src.px.iter().enumerate() {
            let v = if blurred[i] >= balance { 1.0 } else { 0.0 };
            out.px[i] = Pixel16::from_f32s(v, v, v, p.to_f32s().3);
        }
        out
    });
}

/// Filter > Sketch > Bas Relief: the luminance relief shaded
/// as if lit from the top-left and cast between two colors —
/// where the smoothed gradient climbs toward the light the
/// mix leans to `fg`, where it falls away it leans to `bg`.
/// `detail` scales the shading, `smoothness` pre-blurs the
/// relief (radius capped at 8). Alpha is preserved.
pub fn bas_relief(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    fg: Pixel16,
    bg: Pixel16,
    detail: u32,
    smoothness: u32,
) {
    let (f, b) = (fg.to_f32s(), bg.to_f32s());
    rgb_filter(doc, layer, sel, |src| {
        let lum = luma_plane(src);
        let blurred = box_blur_plane(&lum, src.w, src.h, smoothness.min(8));
        let (w, h) = (src.w as i32, src.h as i32);
        let at = |x: i32, y: i32| blurred[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
        let gain = 2.0 + detail as f32;
        let mut out = blank_like(src);
        for yy in 0..h {
            for xx in 0..w {
                let i = (yy * w + xx) as usize;
                let gx = at(xx + 1, yy) - at(xx - 1, yy);
                let gy = at(xx, yy + 1) - at(xx, yy - 1);
                let shade =
                    (0.5 + (gx + gy) * std::f32::consts::FRAC_1_SQRT_2 * gain).clamp(0.0, 1.0);
                out.px[i] = Pixel16::from_f32s(
                    b.0 + (f.0 - b.0) * shade,
                    b.1 + (f.1 - b.1) * shade,
                    b.2 + (f.2 - b.2) * shade,
                    src.px[i].to_f32s().3,
                );
            }
        }
        out
    });
}

/// Filter > Pixelate > Mezzotint: a monochrome dot pattern
/// driven by luminance — a pixel prints white where its cell's
/// hash falls below the pixel's luminance. Kinds: 0 = Fine
/// Dots (per-pixel), 1 = Medium Dots (2x2 cells), 2 = Grainy
/// Dots (3x3 cells, jittered threshold), 3 = Short Lines
/// (6x1 cells). Alpha is preserved.
pub fn mezzotint(doc: &mut Document, layer: usize, sel: Option<&Selection>, kind: u8, seed: u64) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let (ax, ay) = (src.x + xx, src.y + yy);
                let t = match kind {
                    1 => hash_unit(seed, ax.div_euclid(2), ay.div_euclid(2), 81),
                    2 => {
                        let cell = hash_unit(seed, ax.div_euclid(3), ay.div_euclid(3), 82);
                        let jitter = (hash_unit(seed, ax, ay, 83) - 0.5) * 0.6;
                        cell + jitter
                    }
                    3 => hash_unit(seed, ax.div_euclid(6), ay, 84),
                    _ => hash_unit(seed, ax, ay, 80),
                };
                let v = if t < luma(p) { 1.0 } else { 0.0 };
                out.px[i] = Pixel16::from_f32s(v, v, v, p.3);
            }
        }
        out
    });
}

/// Filter > Pixelate > Facet: each pixel becomes the average
/// of the 3x3 neighbors whose luminance is within 0.08 of its
/// own — flat facets of near-equal tone, edges preserved.
pub fn facet(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let cl = luma(src.at(xx, yy).to_f32s());
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                let mut n = 0.0f32;
                for wy in -1..=1 {
                    for wx in -1..=1 {
                        let p = src.at(xx + wx, yy + wy).to_f32s();
                        if (luma(p) - cl).abs() <= 0.08 {
                            acc.0 += p.0;
                            acc.1 += p.1;
                            acc.2 += p.2;
                            acc.3 += p.3;
                            n += 1.0;
                        }
                    }
                }
                if n > 0.0 {
                    out.px[(yy as u32 * src.w + xx as u32) as usize] =
                        Pixel16::from_f32s(acc.0 / n, acc.1 / n, acc.2 / n, acc.3 / n);
                }
            }
        }
        out
    });
}

/// Filter > Render > 3D Transform (Phase 5): maps the image
/// onto a rotating solid — cube (0), sphere (1), or cylinder (2)
/// — by ray casting. Each output pixel casts a ray from a camera
/// at z = +3; the intersection point's surface coordinates become
/// texture coordinates into the source region (the full texture
/// wraps each cube face, and wraps the sphere/cylinder around).
/// Misses are transparent. Rotation is in degrees around the
/// vertical (ry) then horizontal (rx) axes.
pub fn transform_3d(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    shape: u8,
    rx_deg: f32,
    ry_deg: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let (w, h) = (src.w as f32, src.h as f32);
        let mut out = blank_like(src);
        let (rx, ry) = (rx_deg.to_radians(), ry_deg.to_radians());
        let (sx, cx) = (rx.sin(), rx.cos());
        let (sy, cy) = (ry.sin(), ry.cos());
        // Rotate a point by -ry then -rx (world -> object space).
        let unrotate = |p: (f32, f32, f32)| -> (f32, f32, f32) {
            let (x1, y1, z1) = (cy * p.0 - sy * p.2, p.1, sy * p.0 + cy * p.2);
            (x1, cx * y1 + sx * z1, -sx * y1 + cx * z1)
        };

        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                // NDC in -1..=1 (y up), ray from camera (0,0,3)
                // through the pixel toward -z.
                let nx = ((xx as f32 + 0.5) / w) * 2.0 - 1.0;
                let ny = 1.0 - ((yy as f32 + 0.5) / h) * 2.0;
                let aspect = w / h;
                let dir = normalize3((nx * aspect * 0.7, ny * 0.7, -1.0));
                let cam = (0.0f32, 0.0f32, 3.0f32);
                let d = unrotate(dir);
                let o = unrotate(cam);
                let hit: Option<(f32, f32)> = match shape {
                    1 => {
                        // Sphere radius 1: |o + t d| = 1.
                        let b = o.0 * d.0 + o.1 * d.1 + o.2 * d.2;
                        let c = o.0 * o.0 + o.1 * o.1 + o.2 * o.2 - 1.0;
                        let disc = b * b - c;
                        if disc < 0.0 {
                            None
                        } else {
                            let t = -b - disc.sqrt();
                            if t <= 0.0 {
                                None
                            } else {
                                let p = (o.0 + t * d.0, o.1 + t * d.1, o.2 + t * d.2);
                                let u = 0.5 + p.2.atan2(p.0) / (2.0 * std::f32::consts::PI);
                                let v = 0.5 - p.1.clamp(-1.0, 1.0).asin() / std::f32::consts::PI;
                                Some((u, v))
                            }
                        }
                    }
                    2 => {
                        // Cylinder: x^2 + z^2 = 1, |y| <= 1.
                        let a = d.0 * d.0 + d.2 * d.2;
                        let b = o.0 * d.0 + o.2 * d.2;
                        let c = o.0 * o.0 + o.2 * o.2 - 1.0;
                        let disc = b * b - a * c;
                        if a < 1e-9 || disc < 0.0 {
                            None
                        } else {
                            let t = (-b - disc.sqrt()) / a;
                            let y = o.1 + t * d.1;
                            if t <= 0.0 || y.abs() > 1.0 {
                                None
                            } else {
                                let p = (o.0 + t * d.0, y, o.2 + t * d.2);
                                let u = 0.5 + p.2.atan2(p.0) / (2.0 * std::f32::consts::PI);
                                Some((u, (1.0 - (p.1 + 1.0) / 2.0)))
                            }
                        }
                    }
                    _ => {
                        // Cube: slab test against [-1,1]^3; the hit
                        // face's two axes become (u, v).
                        let mut tmin = f32::NEG_INFINITY;
                        let mut tmax = f32::INFINITY;
                        let mut axis = 0usize;
                        let o3 = [o.0, o.1, o.2];
                        let d3 = [d.0, d.1, d.2];
                        let mut ok = true;
                        for a in 0..3 {
                            if d3[a].abs() < 1e-9 {
                                if o3[a] < -1.0 || o3[a] > 1.0 {
                                    ok = false;
                                }
                            } else {
                                let mut t1 = (-1.0 - o3[a]) / d3[a];
                                let mut t2 = (1.0 - o3[a]) / d3[a];
                                if t1 > t2 {
                                    std::mem::swap(&mut t1, &mut t2);
                                }
                                if t1 > tmin {
                                    tmin = t1;
                                    axis = a;
                                }
                                tmax = tmax.min(t2);
                            }
                        }
                        if !ok || tmin > tmax || tmin <= 0.0 {
                            None
                        } else {
                            let p = (o.0 + tmin * d.0, o.1 + tmin * d.1, o.2 + tmin * d.2);
                            let p3 = [p.0, p.1, p.2];
                            let (u, v) = match axis {
                                0 => ((p3[2] + 1.0) / 2.0, (1.0 - (p3[1] + 1.0) / 2.0)),
                                1 => ((p3[0] + 1.0) / 2.0, (1.0 - (p3[2] + 1.0) / 2.0)),
                                _ => ((p3[0] + 1.0) / 2.0, (1.0 - (p3[1] + 1.0) / 2.0)),
                            };
                            Some((u, v))
                        }
                    }
                };
                if let Some((u, v)) = hit {
                    let fx = u * w - 0.5;
                    let fy = v * h - 0.5;
                    let s = sample_bilinear(src, fx, fy);
                    out.px[i] = Pixel16::from_f32s(s.0, s.1, s.2, s.3);
                }
            }
        }
        out
    });
}

fn normalize3(v: (f32, f32, f32)) -> (f32, f32, f32) {
    let l = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt().max(1e-9);
    (v.0 / l, v.1 / l, v.2 / l)
}

/// Filter > Extract (Phase 5): cut out the region the user filled
/// inside a painted edge highlight. `highlight` and `filled` are
/// document-sized masks (w*h bytes, 0/255) in document coords.
/// Definite foreground = filled and not highlighted; definite
/// background = not filled and not highlighted. Highlight pixels
/// get a matting alpha from their color distance to the nearest
/// definite-foreground and definite-background samples, and their
/// color is decontaminated against the background sample.
/// Everything outside the fill becomes transparent.
pub fn extract(doc: &mut Document, layer: usize, highlight: &[u8], filled: &[u8]) {
    let (w, h) = (doc.width as i32, doc.height as i32);
    if highlight.len() != (w * h) as usize || filled.len() != (w * h) as usize {
        return;
    }
    let at = |m: &[u8], x: i32, y: i32| -> bool {
        x >= 0 && y >= 0 && x < w && y < h && m[(y * w + x) as usize] > 127
    };
    let layer_ref = &doc.layers[layer];
    let px = |x: i32, y: i32| layer_ref.get_pixel(x, y).to_f32s();
    // Nearest definite fg/bg colors within a search window.
    let nearest = |x: i32, y: i32, want_fg: bool| -> Option<(f32, f32, f32)> {
        let mut best: Option<(i32, (f32, f32, f32))> = None;
        for dy in -16..=16i32 {
            for dx in -16..=16i32 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let is_fg = at(filled, nx, ny) && !at(highlight, nx, ny);
                let is_bg = !at(filled, nx, ny) && !at(highlight, nx, ny);
                if (want_fg && is_fg) || (!want_fg && is_bg) {
                    let d = dx * dx + dy * dy;
                    if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                        let c = px(nx, ny);
                        best = Some((d, (c.0, c.1, c.2)));
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    };
    let dist = |a: (f32, f32, f32), b: (f32, f32, f32)| {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
    };
    let mut writes: Vec<(i32, i32, Pixel16)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let c = px(x, y);
            if !at(filled, x, y) {
                if c.3 > 0.0 {
                    writes.push((x, y, Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0)));
                }
                continue;
            }
            if !at(highlight, x, y) {
                continue; // definite foreground: untouched
            }
            let fg = nearest(x, y, true);
            let bg = nearest(x, y, false);
            let (Some(fg), Some(bg)) = (fg, bg) else {
                continue;
            };
            let cc = (c.0, c.1, c.2);
            let d_fg = dist(cc, fg);
            let d_bg = dist(cc, bg);
            let a = if d_fg + d_bg < 1e-6 {
                1.0
            } else {
                (d_bg / (d_fg + d_bg)).clamp(0.0, 1.0)
            };
            let new_a = a * c.3;
            let clean = if new_a > 0.01 {
                let unmix = |v: f32, b: f32| ((v - (1.0 - a) * b) / a).clamp(0.0, 1.0);
                (unmix(c.0, bg.0), unmix(c.1, bg.1), unmix(c.2, bg.2))
            } else {
                cc
            };
            writes.push((x, y, Pixel16::from_f32s(clean.0, clean.1, clean.2, new_a)));
        }
    }
    let l = &mut doc.layers[layer];
    for (x, y, p) in writes {
        l.set_pixel(x, y, p);
    }
}

// ----------------------------------------------------------------
// Phase 6.5 Batch 1a: the remaining Stylize / Pixelate / Blur /
// Video gallery filters. All deterministic (seeded hashes), all
// implemented once over RGB regions via `rgb_filter`.

/// Filter > Stylize > Extrude: the image breaks into blocks
/// (kind 0) or pyramids (kind 1) that jut toward the viewer by
/// a hashed depth per block. `size` is the block edge in
/// pixels, `depth` the maximum extrusion (era: 1..=255 levels
/// of it). A block's front face is its average color; the rim
/// the depth exposes is shaded by which side it belongs to.
pub fn extrude(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: u32,
    depth: u32,
    kind: u8,
    seed: u64,
) {
    let cell = size.max(2) as i32;
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (w, h) = (src.w as i32, src.h as i32);
        let (bw, bh) = ((w + cell - 1) / cell, (h + cell - 1) / cell);
        // Per-block average color and extrusion depth.
        let mut avg = vec![(0.0f32, 0.0f32, 0.0f32, 0.0f32); (bw * bh) as usize];
        let mut cnt = vec![0u32; (bw * bh) as usize];
        for yy in 0..h {
            for xx in 0..w {
                let bi = ((yy / cell) * bw + xx / cell) as usize;
                let p = src.at(xx, yy).to_f32s();
                let a = &mut avg[bi];
                a.0 += p.0;
                a.1 += p.1;
                a.2 += p.2;
                a.3 += p.3;
                cnt[bi] += 1;
            }
        }
        for (a, &n) in avg.iter_mut().zip(cnt.iter()) {
            if n > 0 {
                let n = n as f32;
                a.0 /= n;
                a.1 /= n;
                a.2 /= n;
                a.3 /= n;
            }
        }
        for yy in 0..h {
            for xx in 0..w {
                let (bx, by) = (xx / cell, yy / cell);
                let bi = (by * bw + bx) as usize;
                let a = avg[bi];
                let (pw, ph) = ((w - bx * cell).min(cell), (h - by * cell).min(cell));
                let (lx, ly) = (xx - bx * cell, yy - by * cell);
                let dep = hash_unit(seed, bx, by, 71) * depth as f32;
                let inset = (dep / (depth as f32 + 24.0)) * (cell as f32 * 0.5);
                let edges = [ly, lx, pw - 1 - lx, ph - 1 - ly];
                let e = edges.iter().copied().fold(i32::MAX, i32::min) as f32;
                let side = edges.iter().position(|&v| v as f32 == e).unwrap_or(0);
                let px = if e >= inset {
                    a
                } else if kind == 1 {
                    // Pyramid: facets brighten toward the apex.
                    let t = if inset > 0.0 { e / inset } else { 1.0 };
                    let side_tint = [1.05, 0.95, 0.88, 0.78][side];
                    let s = (0.45 + 0.55 * t) * side_tint;
                    (a.0 * s, a.1 * s, a.2 * s, a.3)
                } else {
                    let s = [1.10, 0.95, 0.85, 0.70][side];
                    (a.0 * s, a.1 * s, a.2 * s, a.3)
                };
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(px.0, px.1, px.2, px.3);
            }
        }
        out
    });
}

/// Filter > Stylize > Trace Contour: pixels become white
/// except where the luma crosses `level` (0.0..=1.0) between a
/// pixel and its right/down neighbor — those keep their color,
/// drawing the contour lines. `upper` selects which side of
/// the crossing is traced (era: Lower / Upper Edge).
pub fn trace_contour(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    level: f32,
    upper: bool,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let p = src.at(xx, yy).to_f32s();
                let l = luma(p);
                let mut mark = false;
                for (nx, ny) in [(xx + 1, yy), (xx, yy + 1)] {
                    let ln = luma(src.at(nx, ny).to_f32s());
                    if (l < level) != (ln < level) && upper == (l >= level) {
                        mark = true;
                    }
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] = if mark {
                    src.at(xx, yy)
                } else {
                    Pixel16::from_f32s(1.0, 1.0, 1.0, p.3)
                };
            }
        }
        out
    });
}

/// Filter > Pixelate > Color Halftone: each RGB channel is
/// screened through its own rotated dot grid (15 / 75 / 45
/// degrees, the classic screen angles). A channel's dot grows
/// with the channel's average value inside the cell, up to
/// `max_radius`.
pub fn color_halftone(doc: &mut Document, layer: usize, sel: Option<&Selection>, max_radius: u32) {
    let max_r = max_radius.max(1) as f32;
    let pitch = (max_r * 2.0).max(2.0);
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let angles = [15.0f32, 75.0, 45.0];
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut rgb = (0.0f32, 0.0f32, 0.0f32);
                for (ch, ang) in angles.iter().enumerate() {
                    let (sin, cos) = ang.to_radians().sin_cos();
                    // Rotate the point into screen space.
                    let rx = xx as f32 * cos + yy as f32 * sin;
                    let ry = -xx as f32 * sin + yy as f32 * cos;
                    let cx = (rx / pitch).floor() * pitch + pitch * 0.5;
                    let cy = (ry / pitch).floor() * pitch + pitch * 0.5;
                    let d = ((rx - cx).powi(2) + (ry - cy).powi(2)).sqrt();
                    // Back to document space for the cell sample.
                    let dx = cx * cos - cy * sin;
                    let dy = cx * sin + cy * cos;
                    // Average the channel over a small patch at
                    // the cell center (the cell's tone).
                    let mut v = 0.0f32;
                    for oy in -1..=1 {
                        for ox in -1..=1 {
                            let p = src
                                .at(dx.round() as i32 + ox, dy.round() as i32 + oy)
                                .to_f32s();
                            v += [p.0, p.1, p.2][ch];
                        }
                    }
                    v /= 9.0;
                    let on = if d <= max_r * v { 1.0 } else { 0.0 };
                    match ch {
                        0 => rgb.0 = on,
                        1 => rgb.1 = on,
                        _ => rgb.2 = on,
                    }
                }
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(rgb.0, rgb.1, rgb.2, a);
            }
        }
        out
    });
}

/// Filter > Pixelate > Crystallize: a Voronoi mosaic — seed
/// points sit on a jittered grid of `cell` pixels and every
/// pixel takes the color of its nearest seed point.
pub fn crystallize(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cell: u32,
    seed: u64,
) {
    let cs = cell.max(2) as i32;
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let seed_at = |gx: i32, gy: i32| -> (f32, f32) {
            (
                gx as f32 * cs as f32 + hash_unit(seed, gx, gy, 81) * cs as f32,
                gy as f32 * cs as f32 + hash_unit(seed, gx, gy, 82) * cs as f32,
            )
        };
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (gx, gy) = (xx / cs, yy / cs);
                let mut best = (f32::MAX, 0.0f32, 0.0f32);
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        let (sx, sy) = seed_at(gx + ox, gy + oy);
                        let d = (xx as f32 - sx).powi(2) + (yy as f32 - sy).powi(2);
                        if d < best.0 {
                            best = (d, sx, sy);
                        }
                    }
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    src.at(best.1.round() as i32, best.2.round() as i32);
            }
        }
        out
    });
}

/// Filter > Pixelate > Pointillize: dots of each cell's
/// average color on the background color, one jittered dot per
/// `cell`-sized square (era: the dots land on the Background
/// color).
pub fn pointillize(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    cell: u32,
    bg: Pixel16,
    seed: u64,
) {
    let cs = cell.max(3) as i32;
    rgb_filter(doc, layer, sel, |src| {
        let (w, h) = (src.w as i32, src.h as i32);
        let bgf = bg.to_f32s();
        let mut out = blank_like(src);
        for p in out.px.iter_mut() {
            let a = p.to_f32s().3;
            *p = Pixel16::from_f32s(bgf.0, bgf.1, bgf.2, a);
        }
        let center = |gx: i32, gy: i32| -> (f32, f32) {
            (
                gx as f32 * cs as f32
                    + cs as f32 * 0.5
                    + (hash_unit(seed, gx, gy, 91) - 0.5) * cs as f32 * 0.5,
                gy as f32 * cs as f32
                    + cs as f32 * 0.5
                    + (hash_unit(seed, gx, gy, 92) - 0.5) * cs as f32 * 0.5,
            )
        };
        let radius = cs as f32 * 0.55;
        for yy in 0..h {
            for xx in 0..w {
                let (gx, gy) = (xx / cs, yy / cs);
                let mut best: Option<(f32, Pixel16)> = None;
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        let (cx, cy) = center(gx + ox, gy + oy);
                        let d = ((xx as f32 - cx).powi(2) + (yy as f32 - cy).powi(2)).sqrt();
                        if d <= radius && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                            best = Some((d, src.at(cx.round() as i32, cy.round() as i32)));
                        }
                    }
                }
                if let Some((_, c)) = best {
                    let a = src.at(xx, yy).to_f32s().3;
                    let cf = c.to_f32s();
                    out.px[(yy as u32 * src.w + xx as u32) as usize] =
                        Pixel16::from_f32s(cf.0, cf.1, cf.2, a);
                }
            }
        }
        out
    });
}

/// Filter > Blur > Smart Blur: averages only the neighbors
/// whose luma is within `threshold` (0..=100, era scale) of the
/// center pixel, so edges survive the blur. `edge_only`
/// renders the era Edge Only mode: white edges on black.
pub fn smart_blur(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    radius: f32,
    threshold: f32,
    edge_only: bool,
) {
    let r = (radius.round() as i32).clamp(1, 12);
    let th = (threshold / 100.0).max(0.0);
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let c = src.at(xx, yy).to_f32s();
                let lc = luma(c);
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                let mut n = 0.0f32;
                let mut edge = false;
                for oy in -r..=r {
                    for ox in -r..=r {
                        let p = src.at(xx + ox, yy + oy).to_f32s();
                        if (luma(p) - lc).abs() <= th {
                            acc.0 += p.0;
                            acc.1 += p.1;
                            acc.2 += p.2;
                            acc.3 += p.3;
                            n += 1.0;
                        } else {
                            edge = true;
                        }
                    }
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] = if edge_only {
                    let v = if edge { 1.0 } else { 0.0 };
                    Pixel16::from_f32s(v, v, v, c.3)
                } else if n > 0.0 {
                    Pixel16::from_f32s(acc.0 / n, acc.1 / n, acc.2 / n, acc.3 / n)
                } else {
                    src.at(xx, yy)
                };
            }
        }
        out
    });
}

/// Filter > Video > De-Interlace: rebuilds one field's rows
/// from the other field — by duplication (`interpolate`
/// false) or by averaging the rows above and below.
pub fn deinterlace(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    eliminate_even: bool,
    interpolate: bool,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let target = if eliminate_even { 1 } else { 0 };
        for yy in 0..src.h as i32 {
            // Field parity follows the document row, not the
            // (possibly selection-clipped) region row.
            if (src.y + yy) % 2 != target {
                continue;
            }
            for xx in 0..src.w as i32 {
                let v = if interpolate {
                    let a = src.at(xx, yy - 1).to_f32s();
                    let b = src.at(xx, yy + 1).to_f32s();
                    Pixel16::from_f32s(
                        (a.0 + b.0) * 0.5,
                        (a.1 + b.1) * 0.5,
                        (a.2 + b.2) * 0.5,
                        (a.3 + b.3) * 0.5,
                    )
                } else if yy > 0 {
                    src.at(xx, yy - 1)
                } else {
                    src.at(xx, yy + 1)
                };
                out.px[(yy as u32 * src.w + xx as u32) as usize] = v;
            }
        }
        out
    });
}

/// Filter > Video > NTSC Colors: pulls out-of-gamut colors
/// back inside the NTSC-safe range by scaling chroma (in YIQ)
/// down until the RGB result fits, leaving in-gamut colors
/// untouched.
pub fn ntsc_colors(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for (i, p) in src.px.iter().enumerate() {
            let (r, g, b, a) = p.to_f32s();
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let mut ci = 0.596 * r - 0.274 * g - 0.322 * b;
            let mut cq = 0.211 * r - 0.523 * g + 0.312 * b;
            let mut rgb = (r, g, b);
            for _ in 0..12 {
                rgb = (
                    y + 0.956 * ci + 0.619 * cq,
                    y - 0.272 * ci - 0.647 * cq,
                    y - 1.106 * ci + 1.703 * cq,
                );
                if (0.0..=1.0).contains(&rgb.0)
                    && (0.0..=1.0).contains(&rgb.1)
                    && (0.0..=1.0).contains(&rgb.2)
                {
                    break;
                }
                ci *= 0.85;
                cq *= 0.85;
            }
            out.px[i] = Pixel16::from_f32s(rgb.0, rgb.1, rgb.2, a);
        }
        out
    });
}

/// Filter > Sharpen > Sharpen More: the fixed, stronger
/// sibling of Sharpen (center-weighted kernel, weights sum
/// to 1 so flat areas are unchanged).
pub fn sharpen_more(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let c = src.at(xx, yy).to_f32s();
                let mut acc = (c.0 * 9.0, c.1 * 9.0, c.2 * 9.0);
                for (nx, ny) in [(xx - 1, yy), (xx + 1, yy), (xx, yy - 1), (xx, yy + 1)] {
                    let p = src.at(nx, ny).to_f32s();
                    acc.0 -= 2.0 * p.0;
                    acc.1 -= 2.0 * p.1;
                    acc.2 -= 2.0 * p.2;
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(acc.0, acc.1, acc.2, c.3);
            }
        }
        out
    });
}

// ----------------------------------------------------------------
// Phase 6.5 Batch 1b: Render + the remaining Distort filters.

/// Smooth value noise: bilinear interpolation (with smoothstep
/// easing) over a hashed integer lattice.
fn noise2(seed: u64, x: f32, y: f32, ch: u64) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let g = |ix: f32, iy: f32| hash_unit(seed, ix as i32, iy as i32, ch);
    let (a, b, c, d) = (
        g(x0, y0),
        g(x0 + 1.0, y0),
        g(x0, y0 + 1.0),
        g(x0 + 1.0, y0 + 1.0),
    );
    a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy
}

/// Bilinear sample with wrapped (tiling) coordinates.
fn sample_bilinear_wrap(src: &Region, fx: f32, fy: f32) -> (f32, f32, f32, f32) {
    let (w, h) = (src.w as f32, src.h as f32);
    let wx = ((fx % w) + w) % w;
    let wy = ((fy % h) + h) % h;
    sample_bilinear(src, wx.min(w - 1.0), wy.min(h - 1.0))
}

/// Filter > Distort > Wave: the image is displaced by a sum
/// of sinusoidal generators (era: 1..=999 of them; the field
/// here sums `generators` waves with hashed directions,
/// wavelengths and amplitudes around the slider centers).
/// `wave_type`: 0 Sine, 1 Triangle, 2 Square. `wrap` selects
/// the era Undefined Areas behaviour (Wrap Around vs Repeat
/// Edge Pixels).
#[allow(clippy::too_many_arguments)]
pub fn wave(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    generators: u32,
    wavelength: f32,
    amplitude: f32,
    h_scale: f32,
    v_scale: f32,
    wave_type: u8,
    wrap: bool,
    seed: u64,
) {
    let gens: Vec<(f32, f32, f32, f32, f32)> = (0..generators.max(1))
        .map(|g| {
            let g = g as i32;
            let theta = hash_unit(seed, g, 0, 41) * std::f32::consts::TAU;
            let lam = wavelength.max(4.0) * (0.6 + 0.8 * hash_unit(seed, g, 0, 42));
            let amp = amplitude * (0.5 + hash_unit(seed, g, 0, 43));
            let phase = hash_unit(seed, g, 0, 44) * std::f32::consts::TAU;
            (theta.cos(), theta.sin(), lam, amp, phase)
        })
        .collect();
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let shape = |t: f32| -> f32 {
            let s = (std::f32::consts::TAU * t).sin();
            match wave_type {
                1 => (2.0 / std::f32::consts::PI) * s.asin(),
                2 => {
                    if s > 0.0 {
                        1.0
                    } else if s < 0.0 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                _ => s,
            }
        };
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (mut dx, mut dy) = (0.0f32, 0.0f32);
                for &(ct, st, lam, amp, phase) in &gens {
                    let t = (xx as f32 * ct + yy as f32 * st) / lam + phase / std::f32::consts::TAU;
                    let v = shape(t) * amp;
                    dx += ct * v;
                    dy += st * v;
                }
                dx *= h_scale / 100.0;
                dy *= v_scale / 100.0;
                let p = if wrap {
                    sample_bilinear_wrap(src, xx as f32 - dx, yy as f32 - dy)
                } else {
                    sample_bilinear(src, xx as f32 - dx, yy as f32 - dy)
                };
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(p.0, p.1, p.2, p.3);
            }
        }
        out
    });
}

/// Filter > Distort > Glass: the image seen through frosted
/// glass — a smooth noise field displaces the sampling,
/// `distortion` sets how far, `smoothness` the lump size,
/// `scaling` zooms the texture.
pub fn glass(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    distortion: f32,
    smoothness: f32,
    scaling: f32,
    seed: u64,
) {
    let cell = (2.0 + smoothness * 1.5) * (scaling / 100.0);
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let nx = noise2(seed, xx as f32 / cell, yy as f32 / cell, 51);
                let ny = noise2(seed, xx as f32 / cell, yy as f32 / cell, 52);
                let dx = (nx - 0.5) * distortion * 4.0;
                let dy = (ny - 0.5) * distortion * 4.0;
                let p = sample_bilinear(src, xx as f32 + dx, yy as f32 + dy);
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(p.0, p.1, p.2, p.3);
            }
        }
        out
    });
}

/// Filter > Distort > Diffuse Glow: highlights bleed into a
/// soft glow (a blurred copy of the bright areas is added
/// back), with film grain over the result. `clear` sets how
/// much of the tonal range counts as highlight.
pub fn diffuse_glow(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    graininess: f32,
    glow: f32,
    clear: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let blurred = blur_region(src, 3.0);
        let mut out = blank_like(src);
        let t = 1.0 - (clear / 20.0).clamp(0.0, 1.0) * 0.6;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let g = blurred.px[i].to_f32s();
                // The mask reads the BLURRED luma so the glow
                // bleeds into the darker surroundings of a
                // highlight, not just inside it.
                let l = luma(g);
                let m = ((l - t) / (1.0 - t + 0.001)).clamp(0.0, 1.0);
                let grain = (hash_unit(seed, xx, yy, 33) - 0.5) * graininess * 0.05;
                let k = m * (glow / 10.0);
                out.px[i] = Pixel16::from_f32s(
                    p.0 + g.0 * k + grain,
                    p.1 + g.1 * k + grain,
                    p.2 + g.2 * k + grain,
                    p.3,
                );
            }
        }
        out
    });
}

/// Filter > Distort > Displace: pixels shift by a
/// displacement map — the map's red channel drives the
/// horizontal shift, green the vertical (a grayscale map
/// therefore shifts both together, as in the classic
/// behaviour). `map` holds interleaved (R, G) pairs over an
/// mw x mh plane; scales are percents (100% = 128 px at full
/// deflection). `tile` tiles the map instead of stretching
/// it; `wrap` wraps the displaced sampling at the edges.
#[allow(clippy::too_many_arguments)]
pub fn displace(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    map: &[f32],
    mw: u32,
    mh: u32,
    h_scale: f32,
    v_scale: f32,
    tile: bool,
    wrap: bool,
) {
    if mw == 0 || mh == 0 || map.len() < (mw * mh * 2) as usize {
        return;
    }
    let map_at = |mx: f32, my: f32| -> (f32, f32) {
        let x = (mx.round() as i32).clamp(0, mw as i32 - 1) as u32;
        let y = (my.round() as i32).clamp(0, mh as i32 - 1) as u32;
        let i = ((y * mw + x) * 2) as usize;
        (map[i], map[i + 1])
    };
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (w, h) = (src.w as f32, src.h as f32);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (mvx, mvy) = if tile {
                    map_at((xx % mw as i32) as f32, (yy % mh as i32) as f32)
                } else {
                    map_at(
                        xx as f32 * (mw as f32 - 1.0) / (w - 1.0).max(1.0),
                        yy as f32 * (mh as f32 - 1.0) / (h - 1.0).max(1.0),
                    )
                };
                let dx = (mvx - 0.5) * 2.0 * (h_scale / 100.0) * 128.0;
                let dy = (mvy - 0.5) * 2.0 * (v_scale / 100.0) * 128.0;
                let p = if wrap {
                    sample_bilinear_wrap(src, xx as f32 - dx, yy as f32 - dy)
                } else {
                    sample_bilinear(src, xx as f32 - dx, yy as f32 - dy)
                };
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(p.0, p.1, p.2, p.3);
            }
        }
        out
    });
}

/// Filter > Render > Lens Flare: a bright glow at (cx, cy)
/// (document coordinates), a halo ring, and the chain of
/// ghost reflections along the axis through the image
/// center. `lens`: 0 = 50-300mm Zoom, 1 = 35mm Prime,
/// 2 = 105mm Prime (each with its own ghost set). Brightness
/// is the era percent (10..=300).
pub fn lens_flare(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    brightness: f32,
    cx: f32,
    cy: f32,
    lens: u8,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (fx, fy) = (cx - src.x as f32, cy - src.y as f32);
        let (w, h) = (src.w as f32, src.h as f32);
        let mind = w.min(h);
        let b = brightness / 100.0;
        let r_glow = (mind * 0.45).max(1.0);
        // Ghosts: (position along the flare->center axis,
        // radius as a fraction of the smaller dimension,
        // color, strength).
        type Ghost = (f32, f32, (f32, f32, f32), f32);
        let ghosts: &[Ghost] = match lens {
            1 => &[
                (0.5, 0.08, (0.55, 0.65, 1.0), 0.35),
                (1.0, 0.03, (1.0, 1.0, 1.0), 0.50),
                (1.6, 0.12, (1.0, 0.75, 0.45), 0.30),
            ],
            2 => &[
                (0.25, 0.02, (0.7, 0.8, 1.0), 0.60),
                (0.9, 0.05, (0.9, 0.6, 1.0), 0.45),
                (1.2, 0.025, (1.0, 1.0, 1.0), 0.50),
                (2.0, 0.07, (0.5, 0.9, 1.0), 0.30),
            ],
            _ => &[
                (0.3, 0.03, (0.55, 0.65, 1.0), 0.50),
                (0.7, 0.06, (0.85, 0.6, 1.0), 0.40),
                (1.0, 0.02, (1.0, 1.0, 1.0), 0.60),
                (1.4, 0.09, (1.0, 0.72, 0.4), 0.35),
                (1.8, 0.04, (0.5, 0.9, 1.0), 0.40),
            ],
        };
        let (ccx, ccy) = (w * 0.5, h * 0.5);
        let (ax, ay) = (ccx - fx, ccy - fy);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (px, py) = (xx as f32, yy as f32);
                let d = ((px - fx).powi(2) + (py - fy).powi(2)).sqrt();
                let mut add = (0.0f32, 0.0f32, 0.0f32);
                let glow = (-(d / r_glow).powi(2) * 2.2).exp() * 0.85 * b;
                add.0 += glow;
                add.1 += glow * 0.97;
                add.2 += glow * 0.92;
                let ring_r = r_glow * 0.55;
                let ring = (-((d - ring_r) / (r_glow * 0.06 + 1.0)).powi(2)).exp() * 0.25 * b;
                add.0 += ring * 0.65;
                add.1 += ring * 0.75;
                add.2 += ring;
                for &(t, rf, col, s) in ghosts {
                    let (gx, gy) = (fx + ax * t, fy + ay * t);
                    let gd = ((px - gx).powi(2) + (py - gy).powi(2)).sqrt();
                    let gr = (mind * rf).max(1.0);
                    let gv = (-(gd / gr).powi(2)).exp() * s * b;
                    add.0 += gv * col.0;
                    add.1 += gv * col.1;
                    add.2 += gv * col.2;
                }
                let p = src.at(xx, yy).to_f32s();
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(p.0 + add.0, p.1 + add.1, p.2 + add.2, p.3);
            }
        }
        out
    });
}

/// Filter > Render > Lighting Effects (single-light form):
/// a spotlight or point light at (lx, ly) (document
/// coordinates) shades the image by distance, with an
/// ambient floor so shadows keep detail. `focus` tightens
/// the spotlight's falloff; `texture` embosses the shading
/// with the image's own luma as a height field; `exposure`
/// scales the result (-100..=100).
#[allow(clippy::too_many_arguments)]
pub fn lighting(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    intensity: f32,
    focus: f32,
    lx: f32,
    ly: f32,
    exposure: f32,
    spot: bool,
    texture: bool,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let (fx, fy) = (lx - src.x as f32, ly - src.y as f32);
        let (w, h) = (src.w as f32, src.h as f32);
        let maxd = (w * w + h * h).sqrt().max(1.0);
        let inten = intensity / 100.0;
        let exp = 1.0 + exposure / 100.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let (px, py) = (xx as f32, yy as f32);
                let d = ((px - fx).powi(2) + (py - fy).powi(2)).sqrt();
                let fall = if spot {
                    (1.0 - d / maxd)
                        .clamp(0.0, 1.0)
                        .powf(1.0 + (100.0 - focus) / 25.0)
                } else {
                    1.0 / (1.0 + d / (maxd * 0.5))
                };
                let mut shade = (0.25 + inten * 1.6 * fall) * exp;
                if texture && d > 0.5 {
                    let gx =
                        luma(src.at(xx + 1, yy).to_f32s()) - luma(src.at(xx - 1, yy).to_f32s());
                    let gy =
                        luma(src.at(xx, yy + 1).to_f32s()) - luma(src.at(xx, yy - 1).to_f32s());
                    let emboss = (gx * (fx - px) / d + gy * (fy - py) / d) * 2.0;
                    shade *= 1.0 + emboss * 0.8;
                }
                let spec = fall.powi(8) * inten * 0.3;
                let p = src.at(xx, yy).to_f32s();
                out.px[(yy as u32 * src.w + xx as u32) as usize] = Pixel16::from_f32s(
                    p.0 * shade + spec,
                    p.1 * shade + spec,
                    p.2 * shade + spec,
                    p.3,
                );
            }
        }
        out
    });
}

/// Filter > Render > Texture Fill: the layer is filled with
/// a grayscale texture (a luma plane), tiled to cover.
/// Alpha is preserved from the source.
pub fn texture_fill(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    tex: &[f32],
    tw: u32,
    th: u32,
) {
    if tw == 0 || th == 0 || tex.len() < (tw * th) as usize {
        return;
    }
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let tx = ((src.x + xx) % tw as i32 + tw as i32) % tw as i32;
                let ty = ((src.y + yy) % th as i32 + th as i32) % th as i32;
                let v = tex[(ty as u32 * tw + tx as u32) as usize];
                let a = src.at(xx, yy).to_f32s().3;
                out.px[(yy as u32 * src.w + xx as u32) as usize] = Pixel16::from_f32s(v, v, v, a);
            }
        }
        out
    });
}

// ----------------------------------------------------------------
// Phase 6.5 Batch 1c: the Artistic gallery. These are
// painterly approximations in the spirit of the originals —
// deterministic, seeded, and parameter-faithful.

/// Per-cell running sums for the daub filters.
type CellSums = std::collections::HashMap<(i32, i32), (f32, f32, f32, f32, u32)>;

fn luma_at(src: &Region, x: i32, y: i32) -> f32 {
    luma(src.at(x, y).to_f32s())
}

fn grad_mag(src: &Region, x: i32, y: i32) -> f32 {
    let gx = luma_at(src, x + 1, y) - luma_at(src, x - 1, y);
    let gy = luma_at(src, x, y + 1) - luma_at(src, x, y - 1);
    (gx * gx + gy * gy).sqrt()
}

/// Axis-aligned block averages (the daub / knife / sponge
/// base): every block of cw x ch pixels becomes its mean.
fn cell_avg(src: &Region, cw: i32, chh: i32) -> Region {
    let (cw, chh) = (cw.max(1), chh.max(1));
    let gw = (src.w as i32 + cw - 1) / cw;
    let gh = (src.h as i32 + chh - 1) / chh;
    let mut sums = vec![(0.0f32, 0.0f32, 0.0f32, 0.0f32, 0u32); (gw * gh) as usize];
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            let p = src.at(x, y).to_f32s();
            let c = &mut sums[((y / chh) * gw + (x / cw)) as usize];
            c.0 += p.0;
            c.1 += p.1;
            c.2 += p.2;
            c.3 += p.3;
            c.4 += 1;
        }
    }
    let mut out = blank_like(src);
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            let c = sums[((y / chh) * gw + (x / cw)) as usize];
            let n = c.4 as f32;
            out.px[(y as u32 * src.w + x as u32) as usize] =
                Pixel16::from_f32s(c.0 / n, c.1 / n, c.2 / n, c.3 / n);
        }
    }
    out
}

/// Procedural stand-ins for the era's loaded texture files:
/// 0 Canvas, 1 Burlap, 2 Brick, 3 Sandstone. Returns a
/// brightness factor field in roughly 0.2..=0.8.
fn texture_pattern(kind: u8, x: i32, y: i32, scaling: f32) -> f32 {
    let sc = scaling / 100.0;
    let (fx, fy) = (x as f32 * sc, y as f32 * sc);
    match kind {
        0 => {
            let checker = ((fx as i32 + fy as i32) % 2 + 2) % 2;
            0.45 + checker as f32 * 0.1 + (noise2(1, fx * 0.5, fy * 0.5, 91) - 0.5) * 0.2
        }
        1 => 0.5 + 0.25 * (fx * 1.9).sin() * (fy * 1.9).sin(),
        2 => {
            let row = (fy / 8.0).floor();
            let off = (row as i32 % 2 + 2) % 2 * 16;
            if (fy % 8.0) < 1.0 || ((fx + off as f32) % 32.0) < 1.0 {
                0.25
            } else {
                0.6 + (noise2(3, fx * 0.2, fy * 0.2, 93) - 0.5) * 0.12
            }
        }
        _ => noise2(2, fx * 0.08, fy * 0.08, 92) * 0.7 + 0.15,
    }
}

/// Artistic > Colored Pencil: bright flat areas fall back to
/// the paper color; edges pick up dark hatched pencil
/// strokes.
pub fn colored_pencil(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    width: f32,
    pressure: f32,
    paper: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let paper_v = paper / 50.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let l = luma(p);
                let show = l * l * (1.0 - pressure / 15.0 * 0.75) * 0.8;
                let mut base = (
                    p.0 + (paper_v - p.0) * show,
                    p.1 + (paper_v - p.1) * show,
                    p.2 + (paper_v - p.2) * show,
                );
                let hatch = noise2(seed, xx as f32 * 0.35, yy as f32 * 0.06, 61) * 0.5
                    + noise2(seed, xx as f32 * 0.06, yy as f32 * 0.35, 62) * 0.5;
                let e = grad_mag(src, xx, yy);
                let dark = (e * width * 0.22).min(0.75) * (0.55 + 0.45 * hatch);
                base.0 *= 1.0 - dark;
                base.1 *= 1.0 - dark;
                base.2 *= 1.0 - dark;
                out.px[i] = Pixel16::from_f32s(base.0, base.1, base.2, p.3);
            }
        }
        out
    });
}

/// Artistic > Cutout: flat posterized shapes — smooth first
/// (Edge Simplicity), quantize to `levels`, then let Edge
/// Fidelity pull some of the original detail back.
pub fn cutout(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    levels: f32,
    simplicity: f32,
    fidelity: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, simplicity * 0.7 + 0.001);
        let lv = levels.max(2.0);
        let q = |v: f32| (v * (lv - 1.0)).round() / (lv - 1.0);
        let f = (fidelity - 1.0) / 2.0 * 0.6;
        let mut out = blank_like(src);
        for i in 0..src.px.len() {
            let a = sm.px[i].to_f32s();
            let b = src.px[i].to_f32s();
            out.px[i] = Pixel16::from_f32s(
                q(a.0) + (q(b.0) - q(a.0)) * f,
                q(a.1) + (q(b.1) - q(a.1)) * f,
                q(a.2) + (q(b.2) - q(a.2)) * f,
                a.3,
            );
        }
        out
    });
}

/// Artistic > Dry Brush: a nearly-dry brush drags color —
/// smoothed shapes, detail pulled back, a whisper of grain.
pub fn dry_brush(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    detail: f32,
    texture: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, size * 0.45 + 0.001);
        let mut out = blank_like(src);
        let k = detail / 10.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let a = sm.px[i].to_f32s();
                let b = src.px[i].to_f32s();
                let g = (hash_unit(seed, xx, yy, 63) - 0.5) * texture * 0.03;
                out.px[i] = Pixel16::from_f32s(
                    a.0 + (b.0 - a.0) * k + g,
                    a.1 + (b.1 - a.1) * k + g,
                    a.2 + (b.2 - a.2) * k + g,
                    b.3,
                );
            }
        }
        out
    });
}

/// Artistic > Film Grain: monochrome grain that lives
/// mostly in the highlights; `highlight` sets how far down
/// the tones it reaches, `intensity` its contrast.
pub fn film_grain(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    grain: f32,
    highlight: f32,
    intensity: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let t = 1.0 - highlight / 10.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let l = luma(p);
                let w = if highlight <= 0.0 {
                    l
                } else {
                    ((l - t) / (1.0 - t + 0.001)).clamp(0.0, 1.0)
                };
                let n = (hash_unit(seed, xx, yy, 71) - 0.5)
                    * grain
                    * 0.055
                    * (0.3 + w * (0.4 + intensity / 10.0));
                out.px[i] = Pixel16::from_f32s(p.0 + n, p.1 + n, p.2 + n, p.3);
            }
        }
        out
    });
}

/// Artistic > Fresco: plaster-wall painting — coarser and
/// darker than Dry Brush, colors slightly intensified,
/// mineral grain over the top.
pub fn fresco(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    detail: f32,
    texture: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, size * 0.6 + 0.001);
        let mut out = blank_like(src);
        let k = detail / 10.0 * 0.8;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let a = sm.px[i].to_f32s();
                let b = src.px[i].to_f32s();
                let mut m = (
                    a.0 + (b.0 - a.0) * k,
                    a.1 + (b.1 - a.1) * k,
                    a.2 + (b.2 - a.2) * k,
                );
                let l = 0.299 * m.0 + 0.587 * m.1 + 0.114 * m.2;
                m.0 = (l + (m.0 - l) * 1.15) * 0.94;
                m.1 = (l + (m.1 - l) * 1.15) * 0.94;
                m.2 = (l + (m.2 - l) * 1.15) * 0.94;
                let g = (hash_unit(seed, xx, yy, 64) - 0.5) * texture * 0.045;
                out.px[i] = Pixel16::from_f32s(m.0 + g, m.1 + g, m.2 + g, b.3);
            }
        }
        out
    });
}

/// Artistic > Neon Glow: the image dims and its edges light
/// up in the glow color (the foreground color, as in the
/// original's color well).
pub fn neon_glow(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    brightness: f32,
    color: Pixel16,
) {
    rgb_filter(doc, layer, sel, |src| {
        let bl = blur_region(src, (size * 0.3).max(0.5));
        let c = color.to_f32s();
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let e = grad_mag(&bl, xx, yy);
                let glow = (e * 2.2).clamp(0.0, 1.0) * (brightness / 50.0) * 1.4;
                out.px[i] = Pixel16::from_f32s(
                    p.0 * 0.55 + c.0 * glow,
                    p.1 * 0.55 + c.1 * glow,
                    p.2 * 0.55 + c.2 * glow,
                    p.3,
                );
            }
        }
        out
    });
}

/// Artistic > Paint Daubs: jittered daub cells painted with
/// their average color. Brush types: 0 Simple, 1 Rough
/// (per-daub value jitter), 2 Sparkle (bright flecks).
/// Sharpness pulls original edge detail back in.
pub fn paint_daubs(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    sharpness: f32,
    brush_type: u8,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cs = (size.max(2.0)) as i32;
        let center = |gx: i32, gy: i32| -> (f32, f32) {
            (
                gx as f32 * cs as f32
                    + cs as f32 * 0.5
                    + (hash_unit(seed, gx, gy, 91) - 0.5) * cs as f32 * 0.5,
                gy as f32 * cs as f32
                    + cs as f32 * 0.5
                    + (hash_unit(seed, gx, gy, 92) - 0.5) * cs as f32 * 0.5,
            )
        };
        let cell_of = |x: i32, y: i32| -> (i32, i32) {
            let (gx, gy) = (x.div_euclid(cs), y.div_euclid(cs));
            let mut best = (gx, gy);
            let mut bd = f32::MAX;
            for cy in (gy - 1)..=(gy + 1) {
                for cx in (gx - 1)..=(gx + 1) {
                    let (px, py) = center(cx, cy);
                    let d = (x as f32 - px).powi(2) + (y as f32 - py).powi(2);
                    if d < bd {
                        bd = d;
                        best = (cx, cy);
                    }
                }
            }
            best
        };
        let mut sums: CellSums = std::collections::HashMap::new();
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let p = src.at(xx, yy).to_f32s();
                let e = sums.entry(cell_of(xx, yy)).or_default();
                e.0 += p.0;
                e.1 += p.1;
                e.2 += p.2;
                e.3 += p.3;
                e.4 += 1;
            }
        }
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let cell = cell_of(xx, yy);
                let s = sums[&cell];
                let n = s.4 as f32;
                let mut m = (s.0 / n, s.1 / n, s.2 / n, s.3 / n);
                match brush_type {
                    1 => {
                        let j = 0.92 + hash_unit(seed, cell.0, cell.1, 81) * 0.16;
                        m.0 *= j;
                        m.1 *= j;
                        m.2 *= j;
                    }
                    2 if hash_unit(seed, xx, yy, 82) > 0.995 => {
                        m.0 += 0.3;
                        m.1 += 0.3;
                        m.2 += 0.3;
                    }
                    _ => {}
                }
                let e = grad_mag(src, xx, yy).clamp(0.0, 1.0);
                let p = src.px[i].to_f32s();
                let k = (sharpness / 40.0) * e * 0.8;
                out.px[i] = Pixel16::from_f32s(
                    m.0 + (p.0 - m.0) * k,
                    m.1 + (p.1 - m.1) * k,
                    m.2 + (p.2 - m.2) * k,
                    m.3,
                );
            }
        }
        out
    });
}

/// Artistic > Palette Knife: wide flat smears — rectangular
/// block averages, softened; Detail restores a little of
/// the original.
pub fn palette_knife(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    detail: f32,
    softness: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let blocks = cell_avg(src, (size * 2.0) as i32, size as i32);
        let soft = if softness > 0.0 {
            blur_region(&blocks, softness * 0.35)
        } else {
            blocks
        };
        let f = (detail - 1.0) / 2.0 * 0.25;
        let mut out = blank_like(src);
        for i in 0..src.px.len() {
            let a = soft.px[i].to_f32s();
            let b = src.px[i].to_f32s();
            out.px[i] = Pixel16::from_f32s(
                a.0 + (b.0 - a.0) * f,
                a.1 + (b.1 - a.1) * f,
                a.2 + (b.2 - a.2) * f,
                a.3,
            );
        }
        out
    });
}

/// Artistic > Plastic Wrap: a specular sheen from the
/// image's own height field, as if shrink-wrapped. Detail
/// sets the height field's resolution; Smoothness softens
/// the sheen.
pub fn plastic_wrap(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    strength: f32,
    detail: f32,
    smoothness: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let hgt = blur_region(src, ((16.0 - detail) * 0.25 + 0.3).max(0.3));
        let mut out = blank_like(src);
        let k = strength / 15.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let gx = luma_at(&hgt, xx + 1, yy) - luma_at(&hgt, xx - 1, yy);
                let gy = luma_at(&hgt, xx, yy + 1) - luma_at(&hgt, xx, yy - 1);
                let shade = gx * -0.707 + gy * -0.707;
                let spec = (shade * 3.0).clamp(0.0, 1.0) * k;
                let dark = (-shade * 1.5).clamp(0.0, 0.6) * k * 0.5;
                out.px[i] = Pixel16::from_f32s(
                    p.0 * (1.0 - dark) + spec,
                    p.1 * (1.0 - dark) + spec,
                    p.2 * (1.0 - dark) + spec,
                    p.3,
                );
            }
        }
        if smoothness > 1.0 {
            let soft = blur_region(&out, smoothness * 0.15);
            for i in 0..out.px.len() {
                let a = out.px[i].to_f32s();
                let b = soft.px[i].to_f32s();
                out.px[i] = Pixel16::from_f32s(
                    a.0 + (b.0 - a.0) * 0.5,
                    a.1 + (b.1 - a.1) * 0.5,
                    a.2 + (b.2 - a.2) * 0.5,
                    a.3,
                );
            }
        }
        out
    });
}

/// Artistic > Poster Edges: posterized fills with black
/// inked edges.
pub fn poster_edges(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    thickness: f32,
    intensity: f32,
    posterization: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let lv = (8.0 - posterization).max(2.0);
        let q = |v: f32| (v * (lv - 1.0)).round() / (lv - 1.0);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let e = grad_mag(src, xx, yy);
                let edge = (e * (0.2 + intensity * 0.3)).clamp(0.0, 1.0) * (thickness / 10.0);
                let keep = 1.0 - edge;
                out.px[i] = Pixel16::from_f32s(q(p.0) * keep, q(p.1) * keep, q(p.2) * keep, p.3);
            }
        }
        out
    });
}

/// Artistic > Rough Pastels: chalky block strokes over a
/// textured ground (Canvas / Burlap / Brick / Sandstone),
/// the texture's Relief embossing the result.
#[allow(clippy::too_many_arguments)]
pub fn rough_pastels(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    length: f32,
    detail: f32,
    scaling: f32,
    relief: f32,
    texture_kind: u8,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cw = length.max(2.0) as i32;
        let blocks = cell_avg(src, cw, (cw / 2).max(1));
        let mut out = blank_like(src);
        let dk = detail / 20.0 * 0.25;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let a = blocks.px[i].to_f32s();
                let b = src.px[i].to_f32s();
                let pat = texture_pattern(texture_kind, src.x + xx, src.y + yy, scaling);
                let emb = 1.0 + (pat - 0.5) * (relief / 50.0) * 0.7;
                out.px[i] = Pixel16::from_f32s(
                    (a.0 + (b.0 - a.0) * dk) * emb,
                    (a.1 + (b.1 - a.1) * dk) * emb,
                    (a.2 + (b.2 - a.2) * dk) * emb,
                    a.3,
                );
            }
        }
        out
    });
}

/// Artistic > Smudge Stick: short smearing strokes; the
/// Highlight Area keeps bright passages crisp while the
/// shadows smear.
pub fn smudge_stick(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    length: f32,
    highlight: f32,
    intensity: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let bl = blur_region(src, length * 0.5 + 0.3);
        let mut out = blank_like(src);
        let t = 1.0 - highlight / 20.0;
        for i in 0..src.px.len() {
            let p = src.px[i].to_f32s();
            let b = bl.px[i].to_f32s();
            let l = luma(p);
            let keep = if highlight <= 0.0 {
                0.0
            } else {
                ((l - t) / (1.0 - t + 0.001)).clamp(0.0, 1.0) * 0.75
            };
            let k = (intensity / 10.0) * (1.0 - keep);
            out.px[i] = Pixel16::from_f32s(
                p.0 + (b.0 - p.0) * k,
                p.1 + (b.1 - p.1) * k,
                p.2 + (b.2 - p.2) * k,
                p.3,
            );
        }
        out
    });
}

/// Artistic > Sponge: dabbed color — cell averages pushed
/// around by a per-dab value jitter; Definition sets the
/// dab contrast, Smoothness blends the dabs together.
pub fn sponge(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    definition: f32,
    smoothness: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cell = (size * 3.0).max(3.0) as i32;
        let avg = cell_avg(src, cell, cell);
        let mut out = blank_like(src);
        let def = definition / 25.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let a = avg.px[i].to_f32s();
                let dab = 0.82 + hash_unit(seed, xx / cell, yy / cell, 95) * 0.36;
                let m = (
                    p.0 + (a.0 - p.0) * 0.55,
                    p.1 + (a.1 - p.1) * 0.55,
                    p.2 + (a.2 - p.2) * 0.55,
                );
                let j = 1.0 + (dab - 1.0) * (0.3 + def * 0.7);
                out.px[i] = Pixel16::from_f32s(m.0 * j, m.1 * j, m.2 * j, p.3);
            }
        }
        if smoothness > 1.0 {
            let soft = blur_region(&out, smoothness * 0.15);
            for i in 0..out.px.len() {
                let a = out.px[i].to_f32s();
                let b = soft.px[i].to_f32s();
                out.px[i] = Pixel16::from_f32s(
                    a.0 + (b.0 - a.0) * 0.4,
                    a.1 + (b.1 - a.1) * 0.4,
                    a.2 + (b.2 - a.2) * 0.4,
                    a.3,
                );
            }
        }
        out
    });
}

/// Artistic > Underpainting: broad blocky underpainting
/// with a texture ground showing through — Coverage sets
/// how strongly the ground asserts itself, Relief its
/// emboss depth.
#[allow(clippy::too_many_arguments)]
pub fn underpainting(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    coverage: f32,
    scaling: f32,
    relief: f32,
    texture_kind: u8,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cw = size.max(2.0) as i32;
        let blocks = cell_avg(src, cw, cw);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let a = blocks.px[i].to_f32s();
                let b = src.px[i].to_f32s();
                let pat = texture_pattern(texture_kind, src.x + xx, src.y + yy, scaling);
                let tex = (pat - 0.5) * (coverage / 40.0) * (relief / 50.0) * 1.2;
                let base = (
                    a.0 + (b.0 - a.0) * 0.15,
                    a.1 + (b.1 - a.1) * 0.15,
                    a.2 + (b.2 - a.2) * 0.15,
                );
                out.px[i] = Pixel16::from_f32s(
                    base.0 * (1.0 + tex),
                    base.1 * (1.0 + tex),
                    base.2 * (1.0 + tex),
                    a.3,
                );
            }
        }
        out
    });
}

/// Artistic > Watercolor: soft washes (Detail sets how
/// much structure survives), pigment pooling dark at the
/// edges (Shadow Intensity), paper grain over the top.
pub fn watercolor(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    detail: f32,
    shadow: f32,
    texture: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let wash = blur_region(src, ((15.0 - detail) * 0.22 + 0.3).max(0.3));
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = wash.px[i].to_f32s();
                let e = grad_mag(src, xx, yy);
                let pool = (e * shadow * 0.35).clamp(0.0, 0.65);
                let l = luma(p);
                let desat = |c: f32| l + (c - l) * 0.9;
                let g = (hash_unit(seed, xx, yy, 66) - 0.5) * texture * 0.02;
                out.px[i] = Pixel16::from_f32s(
                    desat(p.0) * (1.0 - pool) + g,
                    desat(p.1) * (1.0 - pool) + g,
                    desat(p.2) * (1.0 - pool) + g,
                    p.3,
                );
            }
        }
        out
    });
}

// ----------------------------------------------------------------
// Phase 6.5 Batch 1d: the Brush Strokes gallery.

/// Brush Strokes > Accented Edges: edges are pushed bright
/// or dark (Brightness above/below the midpoint decides),
/// over a smoothed read of the image.
pub fn accented_edges(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    width: f32,
    brightness: f32,
    smoothness: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, smoothness * 0.2 + 0.2);
        let d = (width / 2.0).max(1.0) as i32;
        let mut out = blank_like(src);
        let accent = (brightness - 25.0) / 25.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let gx = luma_at(&sm, xx + d, yy) - luma_at(&sm, xx - d, yy);
                let gy = luma_at(&sm, xx, yy + d) - luma_at(&sm, xx, yy - d);
                let e = (gx * gx + gy * gy).sqrt();
                let delta = e * accent * 1.1;
                out.px[i] = Pixel16::from_f32s(p.0 + delta, p.1 + delta, p.2 + delta, p.3);
            }
        }
        out
    });
}

/// Brush Strokes > Angled Strokes: a directional smear that
/// follows the image's own gradient direction, biased
/// toward a fixed diagonal by Direction Balance.
pub fn angled_strokes(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    balance: f32,
    length: f32,
    sharpness: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let b = balance / 100.0;
        let (fx, fy) = (
            -std::f32::consts::FRAC_PI_4.cos(),
            -std::f32::consts::FRAC_PI_4.sin(),
        );
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let gx = luma_at(src, xx + 1, yy) - luma_at(src, xx - 1, yy);
                let gy = luma_at(src, xx, yy + 1) - luma_at(src, xx, yy - 1);
                let ang = gy.atan2(gx) + std::f32::consts::FRAC_PI_2;
                let (mut vx, mut vy) = (ang.cos(), ang.sin());
                vx = vx * (1.0 - b) + fx * b;
                vy = vy * (1.0 - b) + fy * b;
                let vl = (vx * vx + vy * vy).sqrt().max(0.001);
                let (vx, vy) = (vx / vl, vy / vl);
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for k in -3..=3i32 {
                    let off = k as f32 / 3.0 * length * 0.5;
                    let j = (hash_unit(seed, xx, yy, 100 + (k + 3) as u64) - 0.5) * 1.5;
                    let p = sample_bilinear(
                        src,
                        xx as f32 + vx * off - vy * j,
                        yy as f32 + vy * off + vx * j,
                    );
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                    acc.3 += p.3;
                }
                let base = (acc.0 / 7.0, acc.1 / 7.0, acc.2 / 7.0, acc.3 / 7.0);
                let p = src.px[i].to_f32s();
                let e = grad_mag(src, xx, yy).clamp(0.0, 1.0);
                let kk = (sharpness / 10.0) * e * 0.6;
                out.px[i] = Pixel16::from_f32s(
                    base.0 + (p.0 - base.0) * kk,
                    base.1 + (p.1 - base.1) * kk,
                    base.2 + (p.2 - base.2) * kk,
                    p.3,
                );
            }
        }
        out
    });
}

/// Brush Strokes > Crosshatch: pencil hatching in up to
/// three directions, each layer appearing only below its
/// tonal threshold; Strength (1..=3) arms the layers.
pub fn crosshatch(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    length: f32,
    sharpness: f32,
    strength: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let spacing = (length * 0.35).max(2.0);
        let layers: [(f32, f32); 3] = [(0.785, 0.78), (-0.785, 0.55), (0.0, 0.34)];
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let l = luma(p);
                let mut m = 1.0f32;
                for (li, (ang, th)) in layers.iter().enumerate() {
                    let f = (strength - li as f32).clamp(0.0, 1.0);
                    if f <= 0.0 || l >= *th {
                        continue;
                    }
                    let proj = xx as f32 * ang.cos() + yy as f32 * ang.sin();
                    let line = ((proj % spacing) + spacing) % spacing;
                    if line < spacing * 0.38 {
                        m *= 1.0 - 0.28 * f * ((th - l) / th + 0.3).min(1.0);
                    }
                }
                let e = grad_mag(src, xx, yy).clamp(0.0, 1.0);
                let kk = (sharpness / 10.0) * e * 0.4;
                let dark = (p.0 * m, p.1 * m, p.2 * m);
                out.px[i] = Pixel16::from_f32s(
                    dark.0 + (p.0 - dark.0) * kk,
                    dark.1 + (p.1 - dark.1) * kk,
                    dark.2 + (p.2 - dark.2) * kk,
                    p.3,
                );
            }
        }
        out
    });
}

/// Brush Strokes > Dark Strokes: shadows are dragged into
/// long dark strokes, highlights kept short and lifted.
pub fn dark_strokes(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    balance: f32,
    black: f32,
    white: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, 1.5 + balance * 0.4);
        let mut out = blank_like(src);
        for i in 0..src.px.len() {
            let p = src.px[i].to_f32s();
            let b = sm.px[i].to_f32s();
            let l = luma(p);
            let darkw = ((balance / 10.0 + 0.25 - l) / 0.5).clamp(0.0, 1.0);
            let mut m = (
                p.0 + (b.0 - p.0) * darkw * 0.7,
                p.1 + (b.1 - p.1) * darkw * 0.7,
                p.2 + (b.2 - p.2) * darkw * 0.7,
            );
            let dk = 1.0 - darkw * (black / 10.0) * 0.45;
            m.0 *= dk;
            m.1 *= dk;
            m.2 *= dk;
            let whitew = ((l - 0.65) / 0.35).clamp(0.0, 1.0) * (white / 10.0) * 0.5;
            out.px[i] = Pixel16::from_f32s(
                m.0 + (1.0 - m.0) * whitew,
                m.1 + (1.0 - m.1) * whitew,
                m.2 + (1.0 - m.2) * whitew,
                p.3,
            );
        }
        out
    });
}

/// Brush Strokes > Ink Outlines: flat areas lift toward
/// paper white; every edge is inked dark, longer strokes
/// reaching further from the contour.
pub fn ink_outlines(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    length: f32,
    dark: f32,
    light: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let paper = 1.0 + (light / 50.0) * 0.25;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let e = grad_mag(src, xx, yy) * (0.5 + length / 25.0);
                let ink = e.clamp(0.0, 1.0) * (dark / 50.0);
                let k = paper * (1.0 - ink * 0.9);
                out.px[i] = Pixel16::from_f32s(p.0 * k, p.1 * k, p.2 * k, p.3);
            }
        }
        out
    });
}

/// Brush Strokes > Spatter: the image sampled through a
/// speckled screen — clumped noise displaces the read,
/// Smoothness setting the clump size.
pub fn spatter(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    radius: f32,
    smoothness: f32,
    seed: u64,
) {
    let sc = 1.5 + smoothness * 0.8;
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let dx = (noise2(seed, xx as f32 / sc, yy as f32 / sc, 101) - 0.5) * radius * 3.0;
                let dy = (noise2(seed, xx as f32 / sc, yy as f32 / sc, 102) - 0.5) * radius * 3.0;
                let p = sample_bilinear(src, xx as f32 + dx, yy as f32 + dy);
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(p.0, p.1, p.2, p.3);
            }
        }
        out
    });
}

/// Brush Strokes > Sprayed Strokes: color sprayed along a
/// diagonal — samples averaged along the stroke direction
/// with a seeded spray jitter across it. Direction:
/// 0 Right Diagonal, 1 Left Diagonal.
pub fn sprayed_strokes(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    length: f32,
    radius: f32,
    direction: u8,
    seed: u64,
) {
    let dir = if direction == 0 {
        (
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        )
    } else {
        (
            std::f32::consts::FRAC_1_SQRT_2,
            -std::f32::consts::FRAC_1_SQRT_2,
        )
    };
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for k in 0..7i32 {
                    let t = (k as f32 / 6.0 - 0.5) * length;
                    let jx = (hash_unit(seed, xx, yy, 110 + k as u64) - 0.5) * radius;
                    let jy = (hash_unit(seed, xx, yy, 120 + k as u64) - 0.5) * radius;
                    let p = sample_bilinear(
                        src,
                        xx as f32 + dir.0 * t + jx,
                        yy as f32 + dir.1 * t + jy,
                    );
                    acc.0 += p.0;
                    acc.1 += p.1;
                    acc.2 += p.2;
                    acc.3 += p.3;
                }
                out.px[(yy as u32 * src.w + xx as u32) as usize] =
                    Pixel16::from_f32s(acc.0 / 7.0, acc.1 / 7.0, acc.2 / 7.0, acc.3 / 7.0);
            }
        }
        out
    });
}

/// Brush Strokes > Sumi-e: Japanese ink painting — tones
/// quantized into a few ink bands (Contrast sets how few),
/// edges inked by Stroke Pressure.
pub fn sumi_e(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    width: f32,
    pressure: f32,
    contrast: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, width * 0.3 + 0.001);
        let bands = (3.0 + (45.0 - contrast) / 9.0).max(2.0);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = sm.px[i].to_f32s();
                let l = luma(p);
                let ql = (l * (bands - 1.0)).round() / (bands - 1.0);
                // Scale the color so its luma lands exactly on
                // the ink band (hue survives; tone does not).
                let base = if l > 0.001 {
                    let sc = ql / l;
                    (p.0 * sc, p.1 * sc, p.2 * sc)
                } else {
                    (ql, ql, ql)
                };
                let e = grad_mag(src, xx, yy);
                let ink = (e * pressure * 0.3).clamp(0.0, 0.85);
                out.px[i] = Pixel16::from_f32s(
                    base.0 * (1.0 - ink),
                    base.1 * (1.0 - ink),
                    base.2 * (1.0 - ink),
                    p.3,
                );
            }
        }
        out
    });
}

// ----------------------------------------------------------------
// Phase 6.5 Batch 1e: the Sketch and Texture galleries.
// Sketch filters draw in the foreground/background colors
// (ink and paper), as the originals do.

/// Ink-and-paper mix: v = 1 is paper (bg), v = 0 is ink (fg).
fn paper_mix(fg: (f32, f32, f32), bg: (f32, f32, f32), v: f32) -> (f32, f32, f32) {
    (
        fg.0 + (bg.0 - fg.0) * v,
        fg.1 + (bg.1 - fg.1) * v,
        fg.2 + (bg.2 - fg.2) * v,
    )
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Sketch > Chalk & Charcoal: shadows fill with hatched
/// charcoal (foreground), highlights with chalk
/// (background), midtones rest on gray paper.
#[allow(clippy::too_many_arguments)]
pub fn chalk_charcoal(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    charcoal_area: f32,
    chalk_area: f32,
    pressure: f32,
    fg: Pixel16,
    bg: Pixel16,
    seed: u64,
) {
    let (fg, bg) = (fg.to_f32s(), bg.to_f32s());
    let (fg3, bg3) = ((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2));
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let ct = (charcoal_area / 50.0) * 0.65;
        let ht = 1.0 - (chalk_area / 50.0) * 0.65;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let l = luma(p);
                let stroke = noise2(seed, xx as f32 * 0.3, yy as f32 * 0.05, 131) * 0.6
                    + noise2(seed, xx as f32 * 0.05, yy as f32 * 0.3, 132) * 0.4;
                let paper = paper_mix(fg3, bg3, 0.5 + (l - 0.5) * 0.3);
                let ink = ((ct - l) / ct.max(0.01)).clamp(0.0, 1.0);
                let chalk = ((l - ht) / (1.0 - ht).max(0.01)).clamp(0.0, 1.0);
                let fs = (
                    fg.0 * (0.8 + 0.2 * stroke * (pressure / 2.5)),
                    fg.1 * (0.8 + 0.2 * stroke * (pressure / 2.5)),
                    fg.2 * (0.8 + 0.2 * stroke * (pressure / 2.5)),
                );
                let mut c = paper;
                c.0 += (fs.0 - c.0) * ink;
                c.1 += (fs.1 - c.1) * ink;
                c.2 += (fs.2 - c.2) * ink;
                c.0 += (bg.0 - c.0) * chalk;
                c.1 += (bg.1 - c.1) * chalk;
                c.2 += (bg.2 - c.2) * chalk;
                out.px[i] = Pixel16::from_f32s(c.0, c.1, c.2, p.3);
            }
        }
        out
    });
}

/// Sketch > Charcoal: a grayscale charcoal drawing — soft
/// tones, edges dragged into thick dark strokes.
pub fn charcoal_fx(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    thickness: f32,
    detail: f32,
    balance: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, ((10.0 - detail) * 0.3 + 0.2).max(0.2));
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(sm.px[i].to_f32s());
                let v = ((l + (50.0 - balance) / 200.0 - 0.125) * (1.0 + detail * 0.12))
                    .clamp(0.0, 1.0);
                let e = grad_mag(src, xx, yy);
                let stroke = (e * thickness * 0.5).clamp(0.0, 0.8);
                let g = v * (1.0 - stroke);
                out.px[i] = Pixel16::from_f32s(g, g, g, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Chrome: liquid metal — the luma height field is
/// shaded through a repeating specular band.
pub fn chrome(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    detail: f32,
    smoothness: f32,
) {
    rgb_filter(doc, layer, sel, |src| {
        let h = blur_region(src, ((10.0 - smoothness) * 0.5 + 0.3).max(0.3));
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(h.px[i].to_f32s());
                let e = grad_mag(&h, xx, yy);
                let ph = l * (2.0 + detail) + e * 1.2;
                let fr = ph.fract();
                let tri = 1.0 - (2.0 * fr - 1.0).abs();
                let band = tri.powf(1.2);
                let v = band + (l - band) * (smoothness / 10.0) * 0.3;
                out.px[i] = Pixel16::from_f32s(v, v, v, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Conte Crayon: the image redrawn in two crayon
/// colors over a textured ground.
#[allow(clippy::too_many_arguments)]
pub fn conte_crayon(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    fg_level: f32,
    bg_level: f32,
    scaling: f32,
    relief: f32,
    texture_kind: u8,
    fg: Pixel16,
    bg: Pixel16,
) {
    let (fg, bg) = (fg.to_f32s(), bg.to_f32s());
    let (fg3, bg3) = ((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2));
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let l = luma(p);
                let v = l.powf(1.0 + (fg_level - bg_level) / 150.0).clamp(0.0, 1.0);
                let pat = texture_pattern(texture_kind, src.x + xx, src.y + yy, scaling);
                let emb = 1.0 + (pat - 0.5) * (relief / 50.0) * 0.8;
                let c = paper_mix(fg3, bg3, v);
                out.px[i] = Pixel16::from_f32s(c.0 * emb, c.1 * emb, c.2 * emb, p.3);
            }
        }
        out
    });
}

/// Sketch > Graphic Pen: black-and-white pen hatching —
/// line density follows the tone. Direction: 0 Right
/// Diagonal, 1 Left Diagonal, 2 Vertical, 3 Horizontal.
#[allow(clippy::too_many_arguments)]
pub fn graphic_pen(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    length: f32,
    balance: f32,
    direction: u8,
    fg: Pixel16,
    bg: Pixel16,
) {
    let (fg, bg) = (fg.to_f32s(), bg.to_f32s());
    let (fg3, bg3) = ((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2));
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, 0.8);
        let spacing = (length * 0.3).max(1.5);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(sm.px[i].to_f32s());
                let proj = match direction {
                    0 => (xx + yy) as f32 * std::f32::consts::FRAC_1_SQRT_2,
                    1 => (xx - yy) as f32 * std::f32::consts::FRAC_1_SQRT_2,
                    2 => xx as f32,
                    _ => yy as f32,
                };
                let density = (1.0 - l) * (0.35 + balance / 100.0);
                let line = ((proj % spacing) + spacing) % spacing / spacing;
                let ink = 1.0 - smoothstep(density - 0.06, density + 0.06, line);
                let c = paper_mix(fg3, bg3, 1.0 - ink);
                out.px[i] = Pixel16::from_f32s(c.0, c.1, c.2, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Halftone Pattern: a press screen — Circle,
/// Dot (square) or Line patterns thresholded by the local
/// tone.
pub fn halftone_pattern(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    contrast: f32,
    kind: u8,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cell = size + 2.0;
        let tone = blur_region(src, cell * 0.5);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let mut t = luma(tone.px[i].to_f32s());
                t = ((t - 0.5) * (1.0 + contrast / 50.0) + 0.5).clamp(0.0, 1.0);
                let cx = ((xx as f32 % cell) / cell) - 0.5;
                let cy = ((yy as f32 % cell) / cell) - 0.5;
                let ink = match kind {
                    0 => (cx * cx + cy * cy).sqrt() * 1.414 < (1.0 - t) * 1.05,
                    1 => cx.abs().max(cy.abs()) * 2.0 < (1.0 - t) * 1.15,
                    _ => cy + 0.5 < 1.0 - t,
                };
                let v = if ink { 0.0 } else { 1.0 };
                out.px[i] = Pixel16::from_f32s(v, v, v, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Note Paper: the image embossed into textured
/// paper — darks stay as image, lights rise into grainy
/// paper.
pub fn note_paper(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    balance: f32,
    graininess: f32,
    relief: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, 1.0);
        let mut out = blank_like(src);
        let mid = balance / 50.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(sm.px[i].to_f32s());
                let raised = smoothstep(mid - 0.15, mid + 0.15, l);
                let emb =
                    (luma_at(&sm, xx - 1, yy - 1) - luma_at(&sm, xx + 1, yy + 1)) * relief * 0.15;
                let grain = (hash_unit(seed, xx, yy, 161) - 0.5) * graininess * 0.03;
                let v = l * (1.0 - raised) + (0.93 + emb + grain) * raised;
                out.px[i] = Pixel16::from_f32s(v, v, v, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Photocopy: a tired office copy — near-binary
/// tones, speckle dirt in the whites.
pub fn photocopy(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    detail: f32,
    darkness: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let lb = blur_region(src, ((25.0 - detail) * 0.12 + 0.2).max(0.2));
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(lb.px[i].to_f32s());
                let mut v = smoothstep(0.32, 0.68, l);
                if v > 0.9 && hash_unit(seed, xx, yy, 171) > 0.997 {
                    v = 0.4; // copier dirt
                }
                let g = v * (1.05 - darkness / 50.0 * 0.55);
                out.px[i] = Pixel16::from_f32s(g, g, g, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Plaster: the image cast in plaster — tones wash
/// toward plaster white and the relief shading of a chosen
/// light direction does the drawing. Light: 0 Top,
/// 1 Bottom, 2 Left, 3 Right.
pub fn plaster(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    balance: f32,
    smoothness: f32,
    light_dir: u8,
) {
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, (smoothness * 0.35).max(0.3));
        let lv: (f32, f32) = match light_dir {
            0 => (0.0, -1.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (1.0, 0.0),
        };
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(sm.px[i].to_f32s());
                let gx = luma_at(&sm, xx + 1, yy) - luma_at(&sm, xx - 1, yy);
                let gy = luma_at(&sm, xx, yy + 1) - luma_at(&sm, xx, yy - 1);
                let shade = (gx * lv.0 + gy * lv.1) * 2.2;
                let base = l + (0.88 - l) * (balance / 50.0) * 0.7;
                let v = (base + shade * (0.4 + balance / 50.0 * 0.4)).clamp(0.0, 1.0);
                out.px[i] = Pixel16::from_f32s(v, v, v, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Reticulation: heat-wrinkled emulsion — tones
/// displaced through a coarse cellular field, crack lines
/// darkened, levels pulled toward the ink/paper poles.
#[allow(clippy::too_many_arguments)]
pub fn reticulation(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    density: f32,
    fg_level: f32,
    bg_level: f32,
    fg: Pixel16,
    bg: Pixel16,
    seed: u64,
) {
    let (fg, bg) = (fg.to_f32s(), bg.to_f32s());
    let (fg3, bg3) = ((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2));
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let sc = 0.05 * (0.5 + density / 25.0);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let dx = (noise2(seed, xx as f32 * sc, yy as f32 * sc, 141) - 0.5) * 14.0;
                let dy = (noise2(seed, xx as f32 * sc, yy as f32 * sc, 142) - 0.5) * 14.0;
                let p = sample_bilinear(src, xx as f32 + dx, yy as f32 + dy);
                let l = luma(p);
                let v =
                    (l * (1.0 + fg_level / 100.0 * 0.3) - bg_level / 100.0 * 0.15).clamp(0.0, 1.0);
                let n = noise2(seed, xx as f32 * sc * 1.7, yy as f32 * sc * 1.7, 143);
                let ridge = 1.0 - (2.0 * n - 1.0).abs();
                let crack = smoothstep(0.92, 1.0, ridge) * (density / 50.0) * 0.5;
                let c = paper_mix(fg3, bg3, v);
                let k = 1.0 - crack;
                out.px[i] = Pixel16::from_f32s(c.0 * k, c.1 * k, c.2 * k, p.3);
            }
        }
        out
    });
}

/// Sketch > Torn Edges: black and white with a torn-paper
/// boundary — the threshold itself is ragged with noise.
#[allow(clippy::too_many_arguments)]
pub fn torn_edges(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    balance: f32,
    smoothness: f32,
    contrast: f32,
    fg: Pixel16,
    bg: Pixel16,
    seed: u64,
) {
    let (fg, bg) = (fg.to_f32s(), bg.to_f32s());
    let (fg3, bg3) = ((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2));
    rgb_filter(doc, layer, sel, |src| {
        let sm = blur_region(src, (smoothness * 0.3).max(0.3));
        let mut out = blank_like(src);
        let sharp = 0.02 + (25.0 - contrast) * 0.004;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let l = luma(sm.px[i].to_f32s());
                let rag = (noise2(seed, xx as f32 * 0.05, yy as f32 * 0.05, 151) - 0.5) * 0.3;
                let thr = balance / 50.0 + rag;
                let v = smoothstep(thr - sharp, thr + sharp, l);
                let c = paper_mix(fg3, bg3, v);
                out.px[i] = Pixel16::from_f32s(c.0, c.1, c.2, src.px[i].to_f32s().3);
            }
        }
        out
    });
}

/// Sketch > Water Paper: fibrous horizontal streaks soften
/// the image onto tinted paper.
#[allow(clippy::too_many_arguments)]
pub fn water_paper(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    fiber: f32,
    brightness: f32,
    contrast: f32,
    fg: Pixel16,
    bg: Pixel16,
    seed: u64,
) {
    let (fg, bg) = (fg.to_f32s(), bg.to_f32s());
    let (fg3, bg3) = ((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2));
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let mut acc = 0.0f32;
                for k in -2..=2i32 {
                    let jx = k as f32 * fiber * 0.3;
                    let jy = (hash_unit(seed, xx, yy, 180 + (k + 2) as u64) - 0.5) * 2.0;
                    acc += luma(sample_bilinear(src, xx as f32 + jx, yy as f32 + jy));
                }
                let l = acc / 5.0;
                let v = ((l - 0.5) * (0.4 + contrast / 100.0 * 1.2)
                    + 0.5
                    + (brightness - 55.0) / 100.0 * 0.4)
                    .clamp(0.0, 1.0);
                let c = paper_mix(fg3, bg3, v);
                let g = (v, v, v);
                out.px[i] = Pixel16::from_f32s(
                    g.0 + (c.0 - g.0) * 0.35,
                    g.1 + (c.1 - g.1) * 0.35,
                    g.2 + (c.2 - g.2) * 0.35,
                    src.px[i].to_f32s().3,
                );
            }
        }
        out
    });
}

/// Texture > Craquelure: a crackle network over the image —
/// Voronoi cell borders crack; Depth sets their strength,
/// Brightness whether they read dark or light.
pub fn craquelure(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    spacing: f32,
    depth: f32,
    brightness: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cs = spacing.max(3.0);
        let center = |gx: i32, gy: i32| -> (f32, f32) {
            (
                gx as f32 * cs + cs * 0.5 + (hash_unit(seed, gx, gy, 191) - 0.5) * cs * 0.6,
                gy as f32 * cs + cs * 0.5 + (hash_unit(seed, gx, gy, 192) - 0.5) * cs * 0.6,
            )
        };
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let (gx, gy) = (
                    (xx as f32 / cs).floor() as i32,
                    (yy as f32 / cs).floor() as i32,
                );
                let (mut f1, mut f2) = (f32::MAX, f32::MAX);
                for cy in (gy - 1)..=(gy + 1) {
                    for cx in (gx - 1)..=(gx + 1) {
                        let (px, py) = center(cx, cy);
                        let d = ((xx as f32 - px).powi(2) + (yy as f32 - py).powi(2)).sqrt();
                        if d < f1 {
                            f2 = f1;
                            f1 = d;
                        } else if d < f2 {
                            f2 = d;
                        }
                    }
                }
                let crack = 1.0 - ((f2 - f1) / (cs * 0.14)).clamp(0.0, 1.0);
                let delta = crack * (depth / 10.0) * ((brightness - 5.0) / 5.0) * 0.5;
                out.px[i] = Pixel16::from_f32s(p.0 + delta, p.1 + delta, p.2 + delta, p.3);
            }
        }
        out
    });
}

/// Texture > Grain: luma grain in ten era flavors (kind
/// cycles): 0 Regular, 1 Soft, 2 Sprinkles, 3 Clumped,
/// 4 Contrasty, 5 Enlarged, 6 Stippled, 7 Horizontal,
/// 8 Vertical, 9 Speckle.
pub fn grain_fx(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    intensity: f32,
    contrast: f32,
    kind: u8,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let amp = intensity / 100.0 * 0.5;
        let shape = 0.5 + contrast / 100.0;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let n = match kind {
                    0 => hash_unit(seed, xx, yy, 201),
                    1 => noise2(seed, xx as f32 * 0.5, yy as f32 * 0.5, 202),
                    2 => {
                        let h = hash_unit(seed, xx, yy, 203);
                        if h > 0.97 {
                            1.0
                        } else if h < 0.03 {
                            0.0
                        } else {
                            0.5
                        }
                    }
                    3 => noise2(seed, xx as f32 * 0.08, yy as f32 * 0.08, 204),
                    4 => {
                        if hash_unit(seed, xx, yy, 205) > 0.5 {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    5 => hash_unit(seed, xx / 4, yy / 4, 206),
                    6 => {
                        if xx % 3 == 0 && yy % 3 == 0 {
                            hash_unit(seed, xx, yy, 207)
                        } else {
                            0.5
                        }
                    }
                    7 => hash_unit(seed, 0, yy, 208),
                    8 => hash_unit(seed, xx, 0, 209),
                    _ => {
                        let h = hash_unit(seed, xx, yy, 210);
                        if h > 0.995 {
                            1.0
                        } else if h < 0.005 {
                            0.0
                        } else {
                            0.5
                        }
                    }
                };
                let g = (n - 0.5) * shape * amp * 2.0;
                out.px[i] = Pixel16::from_f32s(p.0 + g, p.1 + g, p.2 + g, p.3);
            }
        }
        out
    });
}

/// Texture > Mosaic Tiles: block tiles with a slight
/// per-tile value jitter and grout lines between them.
pub fn mosaic_tiles(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    grout: f32,
    lighten: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let ts = size.max(2.0) as i32;
        let blocks = cell_avg(src, ts, ts);
        let mut out = blank_like(src);
        let gw = grout.max(1.0) as i32;
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let a = blocks.px[i].to_f32s();
                let j = 0.94 + hash_unit(seed, xx / ts, yy / ts, 221) * 0.12;
                let is_grout = (xx % ts) < gw || (yy % ts) < gw;
                let m = if is_grout {
                    0.55 + lighten / 10.0 * 0.5
                } else {
                    j
                };
                out.px[i] = Pixel16::from_f32s(a.0 * m, a.1 * m, a.2 * m, a.3);
            }
        }
        out
    });
}

/// Texture > Stained Glass: Voronoi cells of averaged
/// color, lead-came borders between them, and light falling
/// off from each cell's heart.
pub fn stained_glass(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    size: f32,
    border: f32,
    light: f32,
    seed: u64,
) {
    rgb_filter(doc, layer, sel, |src| {
        let cs = size.max(3.0) as i32;
        let center = |gx: i32, gy: i32| -> (f32, f32) {
            (
                gx as f32 * cs as f32
                    + cs as f32 * 0.5
                    + (hash_unit(seed, gx, gy, 231) - 0.5) * cs as f32 * 0.5,
                gy as f32 * cs as f32
                    + cs as f32 * 0.5
                    + (hash_unit(seed, gx, gy, 232) - 0.5) * cs as f32 * 0.5,
            )
        };
        let cell_of = |x: i32, y: i32| -> (i32, i32) {
            let (gx, gy) = (x.div_euclid(cs), y.div_euclid(cs));
            let mut best = (gx, gy);
            let mut bd = f32::MAX;
            for cy in (gy - 1)..=(gy + 1) {
                for cx in (gx - 1)..=(gx + 1) {
                    let (px, py) = center(cx, cy);
                    let d = (x as f32 - px).powi(2) + (y as f32 - py).powi(2);
                    if d < bd {
                        bd = d;
                        best = (cx, cy);
                    }
                }
            }
            best
        };
        let mut sums: CellSums = std::collections::HashMap::new();
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let p = src.at(xx, yy).to_f32s();
                let e = sums.entry(cell_of(xx, yy)).or_default();
                e.0 += p.0;
                e.1 += p.1;
                e.2 += p.2;
                e.3 += p.3;
                e.4 += 1;
            }
        }
        let bw = border.max(1.0);
        let mut out = blank_like(src);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let cell = cell_of(xx, yy);
                let s = sums[&cell];
                let n = s.4 as f32;
                let mut m = (s.0 / n, s.1 / n, s.2 / n, s.3 / n);
                // Light falls off from the cell's heart.
                let (cx, cy) = center(cell.0, cell.1);
                let d = ((xx as f32 - cx).powi(2) + (yy as f32 - cy).powi(2)).sqrt();
                let fall = 1.0 + (1.0 - (d / (cs as f32 * 0.9)).min(1.0)) * (light / 10.0) * 0.5;
                m.0 *= fall;
                m.1 *= fall;
                m.2 *= fall;
                // Lead came: a pixel whose neighbours belong
                // to another cell sits on the border.
                let near_border = cell_of(xx + bw as i32, yy) != cell
                    || cell_of(xx - bw as i32, yy) != cell
                    || cell_of(xx, yy + bw as i32) != cell
                    || cell_of(xx, yy - bw as i32) != cell;
                if near_border {
                    m.0 *= 0.25;
                    m.1 *= 0.25;
                    m.2 *= 0.25;
                }
                out.px[i] = Pixel16::from_f32s(m.0, m.1, m.2, m.3);
            }
        }
        out
    });
}

/// Texture > Texturizer: the era texture-ground emboss as
/// a standalone filter — brightness relief from one of the
/// four procedural grounds under a chosen light.
#[allow(clippy::too_many_arguments)]
pub fn texturizer(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    scaling: f32,
    relief: f32,
    texture_kind: u8,
    light_dir: u8,
) {
    rgb_filter(doc, layer, sel, |src| {
        let mut out = blank_like(src);
        let pat = |x: i32, y: i32| texture_pattern(texture_kind, src.x + x, src.y + y, scaling);
        for yy in 0..src.h as i32 {
            for xx in 0..src.w as i32 {
                let i = (yy as u32 * src.w + xx as u32) as usize;
                let p = src.px[i].to_f32s();
                let (gx, gy) = match light_dir {
                    0 => (0.0, pat(xx, yy - 1) - pat(xx, yy + 1)),
                    1 => (0.0, pat(xx, yy + 1) - pat(xx, yy - 1)),
                    2 => (pat(xx - 1, yy) - pat(xx + 1, yy), 0.0),
                    _ => (pat(xx + 1, yy) - pat(xx - 1, yy), 0.0),
                };
                let emb = 1.0 + (gx + gy) * (relief / 50.0) * 1.4;
                out.px[i] = Pixel16::from_f32s(p.0 * emb, p.1 * emb, p.2 * emb, p.3);
            }
        }
        out
    });
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
