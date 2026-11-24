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
