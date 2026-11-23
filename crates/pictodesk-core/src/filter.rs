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
