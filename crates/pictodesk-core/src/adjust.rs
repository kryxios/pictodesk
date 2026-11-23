//! Destructive adjustments — the classic Image > Adjust menu.
//! Each function edits one layer in place, masked by the optional
//! selection (partial mask values blend the effect, which is what
//! feathered selections are for). Wrap calls in
//! `history::run_tile_edit` to make them undoable.
//!
//! Classic semantics kept: alpha is never created or destroyed by a
//! color adjustment — fully transparent pixels are left untouched,
//! and every adjustment preserves the pixel's alpha.

use crate::color::{cmyk_to_rgb, rgb_to_cmyk};
use crate::document::{ColorMode, Document};
use crate::pixel::Pixel16;
use crate::selection::Selection;

/// A non-destructive adjustment — the payload of a Phase 4
/// adjustment layer. Parameters mirror the destructive dialogs
/// exactly, and `apply` runs the same math those dialogs run, so an
/// adjustment layer and the same adjustment applied destructively
/// agree. The compositor calls `apply` on the accumulated composite
/// of the layers below the adjustment layer.
#[derive(Clone, Debug, PartialEq)]
pub enum Adjustment {
    Levels {
        in_black: u8,
        in_white: u8,
        gamma: f32,
        out_black: u8,
        out_white: u8,
    },
    Curves {
        points: Vec<(u8, u8)>,
    },
    BrightnessContrast {
        brightness: i32,
        contrast: i32,
    },
    HueSaturation {
        hue: i32,
        saturation: i32,
        lightness: i32,
    },
    ColorBalance {
        shadows: (i32, i32, i32),
        midtones: (i32, i32, i32),
        highlights: (i32, i32, i32),
    },
    PhotoFilter {
        /// Filter color as 8-bit RGB.
        color: (u8, u8, u8),
        /// Strength, 0..=100 percent.
        density: u8,
    },
}

impl Adjustment {
    pub fn name(&self) -> &'static str {
        match self {
            Adjustment::Levels { .. } => "Levels",
            Adjustment::Curves { .. } => "Curves",
            Adjustment::BrightnessContrast { .. } => "Brightness/Contrast",
            Adjustment::HueSaturation { .. } => "Hue/Saturation",
            Adjustment::ColorBalance { .. } => "Color Balance",
            Adjustment::PhotoFilter { .. } => "Photo Filter",
        }
    }

    /// Apply the adjustment to one pixel's color (each 0.0..=1.0).
    /// Alpha is the caller's business and never touched here.
    pub fn apply(&self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        match self {
            Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => {
                let (ib, iw) = (
                    *in_black as f32 / 255.0,
                    (*in_white).max(*in_black + 1) as f32 / 255.0,
                );
                let (ob, ow) = (*out_black as f32 / 255.0, *out_white as f32 / 255.0);
                let g = (*gamma).clamp(0.1, 9.99);
                let ev = |x: f32| {
                    let t = ((x - ib) / (iw - ib)).clamp(0.0, 1.0).powf(1.0 / g);
                    ob + t * (ow - ob)
                };
                (ev(r), ev(g), ev(b))
            }
            Adjustment::Curves { points } => {
                let ev = |x: f32| curve_eval(points, x);
                (ev(r), ev(g), ev(b))
            }
            Adjustment::BrightnessContrast {
                brightness,
                contrast,
            } => {
                let bb = (*brightness).clamp(-150, 150) as f32 / 255.0;
                let c8 = (*contrast).clamp(-100, 100) as f32 * 2.55;
                let factor = (259.0 * (c8 + 255.0)) / (255.0 * (259.0 - c8));
                (
                    factor * (r + bb - 0.5) + 0.5,
                    factor * (g + bb - 0.5) + 0.5,
                    factor * (b + bb - 0.5) + 0.5,
                )
            }
            Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
            } => {
                let hs = (*hue).clamp(-180, 180) as f32 / 360.0;
                let ss = (*saturation).clamp(-100, 100) as f32 / 100.0;
                let ls = (*lightness).clamp(-100, 100) as f32 / 100.0;
                let (h, s, l) = rgb_to_hsl(r, g, b);
                let s2 = if ss >= 0.0 {
                    s + (1.0 - s) * ss
                } else {
                    s * (1.0 + ss)
                };
                let l2 = if ls >= 0.0 {
                    l + (1.0 - l) * ls
                } else {
                    l * (1.0 + ls)
                };
                hsl_to_rgb(h + hs, s2.clamp(0.0, 1.0), l2.clamp(0.0, 1.0))
            }
            Adjustment::ColorBalance {
                shadows,
                midtones,
                highlights,
            } => {
                let norm = |t: (i32, i32, i32)| {
                    (
                        t.0.clamp(-100, 100) as f32 / 100.0,
                        t.1.clamp(-100, 100) as f32 / 100.0,
                        t.2.clamp(-100, 100) as f32 / 100.0,
                    )
                };
                let (sh, mi, hi) = (norm(*shadows), norm(*midtones), norm(*highlights));
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                let wsh = (1.0 - lum) * (1.0 - lum);
                let whi = lum * lum;
                let wmi = (1.0 - (2.0 * lum - 1.0).abs()).max(0.0);
                (
                    r + 0.25 * (sh.0 * wsh + mi.0 * wmi + hi.0 * whi),
                    g + 0.25 * (sh.1 * wsh + mi.1 * wmi + hi.1 * whi),
                    b + 0.25 * (sh.2 * wsh + mi.2 * wmi + hi.2 * whi),
                )
            }
            Adjustment::PhotoFilter { color, density } => {
                // Tint the pixel toward the filter color at the
                // pixel's own luminance (the classic photo-filter
                // model), blended by density.
                let f = (
                    color.0 as f32 / 255.0,
                    color.1 as f32 / 255.0,
                    color.2 as f32 / 255.0,
                );
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                let flum = 0.299 * f.0 + 0.587 * f.1 + 0.114 * f.2;
                let tinted = if flum > 1e-5 {
                    (lum * f.0 / flum, lum * f.1 / flum, lum * f.2 / flum)
                } else {
                    (lum, lum, lum)
                };
                let t = (*density as f32 / 100.0).clamp(0.0, 1.0);
                (
                    r + (tinted.0 - r) * t,
                    g + (tinted.1 - g) * t,
                    b + (tinted.2 - b) * t,
                )
            }
        }
    }
}

/// Evaluate a Curves point set at `x` (0.0..=1.0), piecewise linear
/// — the same interpolation the destructive Curves LUT is built
/// from.
pub fn curve_eval(points: &[(u8, u8)], x: f32) -> f32 {
    let mut pts: Vec<(f32, f32)> = points
        .iter()
        .map(|&(px, py)| (px as f32 / 255.0, py as f32 / 255.0))
        .collect();
    if pts.len() < 2 {
        pts = vec![(0.0, 0.0), (1.0, 1.0)];
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if x <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        if x <= w[1].0 {
            let t = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
            return w[0].1 + t * (w[1].1 - w[0].1);
        }
    }
    pts[pts.len() - 1].1
}

/// Apply `f` to every painted pixel of `layer`, blended by the
/// selection mask (None = full strength everywhere).
pub fn map_layer_pixels(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    f: impl Fn(Pixel16) -> Pixel16,
) {
    let offset = doc.layers[layer].offset;
    let coords: Vec<_> = doc.layers[layer].tiles.keys().copied().collect();
    for coord in coords {
        let tile = doc.layers[layer].tiles.get_mut(&coord).unwrap();
        for ly in 0..256i32 {
            for lx in 0..256i32 {
                let p = tile.get(lx as u32, ly as u32);
                if p.a == 0 {
                    continue;
                }
                let (dx, dy) = (coord.0 * 256 + lx + offset.0, coord.1 * 256 + ly + offset.1);
                let strength = match sel {
                    None => 255,
                    Some(s) => s.value(dx, dy),
                };
                if strength == 0 {
                    continue;
                }
                let out = f(p);
                let out = if strength == 255 {
                    out
                } else {
                    blend_px(p, out, strength as f32 / 255.0)
                };
                if out != p {
                    tile.set(lx as u32, ly as u32, out);
                }
            }
        }
    }
}

/// The CMYK counterpart of map_layer_pixels: apply `f` to the ink
/// values of every painted (alpha > 0) pixel of `layer`, blended by
/// the selection mask. Alpha is preserved.
pub fn map_layer_inks(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    f: impl Fn([f32; 4]) -> [f32; 4],
) {
    let offset = doc.layers[layer].offset;
    let mut coords: Vec<_> = doc.layers[layer].tiles.keys().copied().collect();
    for c in doc.layers[layer].cmyk_alpha.keys() {
        if !coords.contains(c) {
            coords.push(*c);
        }
    }
    for coord in coords {
        for ly in 0..256i32 {
            for lx in 0..256i32 {
                let (dx, dy) = (coord.0 * 256 + lx + offset.0, coord.1 * 256 + ly + offset.1);
                let (c, m, y, k, a) = doc.get_cmyk(layer, dx, dy);
                if a == 0 {
                    continue;
                }
                let strength = match sel {
                    None => 255,
                    Some(s) => s.value(dx, dy),
                };
                if strength == 0 {
                    continue;
                }
                let ink = [
                    c as f32 / 65535.0,
                    m as f32 / 65535.0,
                    y as f32 / 65535.0,
                    k as f32 / 65535.0,
                ];
                let mut out = f(ink);
                if strength != 255 {
                    let t = strength as f32 / 255.0;
                    for i in 0..4 {
                        out[i] = ink[i] + (out[i] - ink[i]) * t;
                    }
                }
                let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                doc.set_cmyk(
                    layer,
                    dx,
                    dy,
                    (e(out[0]), e(out[1]), e(out[2]), e(out[3]), a),
                );
            }
        }
    }
}

/// Run an RGB-space adjustment math closure on a CMYK document:
/// each pixel's inks convert to RGB, the math applies, and the
/// result converts back to inks. (Editors do the same internally
/// for the RGB-defined adjustments in CMYK mode.)
fn via_rgb(ink: [f32; 4], f: impl Fn(f32, f32, f32) -> (f32, f32, f32)) -> [f32; 4] {
    let (r, g, b) = cmyk_to_rgb(ink[0], ink[1], ink[2], ink[3]);
    let (r, g, b) = f(r, g, b);
    let (c, m, y, k) = rgb_to_cmyk(r, g, b);
    [c, m, y, k]
}

fn blend_px(a: Pixel16, b: Pixel16, t: f32) -> Pixel16 {
    let l = |x: u16, y: u16| -> u16 { (x as f32 + (y as f32 - x as f32) * t + 0.5) as u16 };
    Pixel16::new(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), a.a)
}

fn map_rgb(p: Pixel16, f: impl Fn(f32, f32, f32) -> (f32, f32, f32)) -> Pixel16 {
    let (r, g, b, a) = p.to_f32s();
    let (r, g, b) = f(r, g, b);
    Pixel16::from_f32s(r, g, b, a)
}

/// Image > Adjust > Brightness/Contrast (legacy editor math).
/// brightness: -150..=150, contrast: -100..=100.
pub fn brightness_contrast(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    brightness: i32,
    contrast: i32,
) {
    let b = brightness.clamp(-150, 150) as f32 / 255.0;
    let c8 = contrast.clamp(-100, 100) as f32 * 2.55;
    let factor = (259.0 * (c8 + 255.0)) / (255.0 * (259.0 - c8));
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| {
            [
                factor * (ink[0] + b - 0.5) + 0.5,
                factor * (ink[1] + b - 0.5) + 0.5,
                factor * (ink[2] + b - 0.5) + 0.5,
                factor * (ink[3] + b - 0.5) + 0.5,
            ]
        });
        return;
    }
    let adj = Adjustment::BrightnessContrast {
        brightness,
        contrast,
    };
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, bl| adj.apply(r, g, bl))
    });
}

pub fn invert(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| {
            [1.0 - ink[0], 1.0 - ink[1], 1.0 - ink[2], 1.0 - ink[3]]
        });
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, b| (1.0 - r, 1.0 - g, 1.0 - b))
    });
}

/// Image > Adjust > Desaturate — the classic method: gray = HSL
/// lightness, (max + min) / 2 of the RGB channels.
pub fn desaturate(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    let math = |r: f32, g: f32, b: f32| {
        let l = (r.max(g).max(b) + r.min(g).min(b)) / 2.0;
        (l, l, l)
    };
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| via_rgb(ink, math));
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| map_rgb(p, math));
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if (mx - mn).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 {
        d / (2.0 - mx - mn)
    } else {
        d / (mx + mn)
    };
    let h = if mx == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if mx == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let conv = |mut t: f32| -> f32 {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (conv(h + 1.0 / 3.0), conv(h), conv(h - 1.0 / 3.0))
}

/// Image > Adjust > Hue/Saturation.
/// hue: -180..=180 degrees, saturation/lightness: -100..=100 percent.
pub fn hue_saturation(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    hue_deg: i32,
    saturation: i32,
    lightness: i32,
) {
    let adj = Adjustment::HueSaturation {
        hue: hue_deg,
        saturation,
        lightness,
    };
    let math = |r: f32, g: f32, b: f32| adj.apply(r, g, b);
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| via_rgb(ink, math));
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| map_rgb(p, math));
}

/// Image > Adjust > Color Balance. Each range is (cyan-red,
/// magenta-green, yellow-blue) shifts in -100..=100, applied with
/// Classic luminance weighting per range.
pub fn color_balance(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    shadows: (i32, i32, i32),
    midtones: (i32, i32, i32),
    highlights: (i32, i32, i32),
) {
    let adj = Adjustment::ColorBalance {
        shadows,
        midtones,
        highlights,
    };
    let math = |r: f32, g: f32, b: f32| adj.apply(r, g, b);
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| via_rgb(ink, math));
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| map_rgb(p, math));
}

/// A 0..=255 -> 0..=255 lookup table, the shared currency of Levels
/// and Curves.
pub fn build_lut(f: impl Fn(u8) -> u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        *v = f(i as u8);
    }
    lut
}

/// Apply a per-channel LUT (same table on R, G, B) with f32
/// interpolation so 16-bit data isn't quantized to 256 steps.
pub fn apply_lut(doc: &mut Document, layer: usize, sel: Option<&Selection>, lut: &[u8; 256]) {
    let lut_f: Vec<f32> = lut.iter().map(|&v| v as f32 / 255.0).collect();
    let sample = |v: f32| -> f32 {
        let x = (v.clamp(0.0, 1.0) * 255.0).min(254.999);
        let i = x.floor() as usize;
        lut_f[i] + (lut_f[i + 1] - lut_f[i]) * (x - i as f32)
    };
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| {
            [
                sample(ink[0]),
                sample(ink[1]),
                sample(ink[2]),
                sample(ink[3]),
            ]
        });
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, b| (sample(r), sample(g), sample(b)))
    });
}

/// Image > Adjust > Levels. Input black/white points (0..=255),
/// gamma (0.1..=9.99), output black/white points.
#[allow(clippy::too_many_arguments)]
pub fn levels(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    in_black: u8,
    in_white: u8,
    gamma: f32,
    out_black: u8,
    out_white: u8,
) {
    let (ib, iw) = (
        in_black as f32 / 255.0,
        in_white.max(in_black + 1) as f32 / 255.0,
    );
    let (ob, ow) = (out_black as f32 / 255.0, out_white as f32 / 255.0);
    let g = gamma.clamp(0.1, 9.99);
    let lut = build_lut(|v| {
        let x = v as f32 / 255.0;
        let t = ((x - ib) / (iw - ib)).clamp(0.0, 1.0).powf(1.0 / g);
        ((ob + t * (ow - ob)) * 255.0 + 0.5) as u8
    });
    apply_lut(doc, layer, sel, &lut);
}

/// Image > Adjust > Curves with a single combined RGB curve through
/// `points` ((input, output) pairs, 0..=255, piecewise linear — the
/// 1.0 dialog drew straight segments between points too, until you
/// smoothed them; spline smoothing is a later refinement).
pub fn curves(doc: &mut Document, layer: usize, sel: Option<&Selection>, points: &[(u8, u8)]) {
    let mut pts: Vec<(f32, f32)> = points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
    if pts.len() < 2 {
        pts = vec![(0.0, 0.0), (255.0, 255.0)];
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let lut = build_lut(|v| {
        let x = v as f32;
        if x <= pts[0].0 {
            return pts[0].1 as u8;
        }
        for w in pts.windows(2) {
            if x <= w[1].0 {
                let t = (x - w[0].0) / (w[1].0 - w[0].0).max(1.0);
                return (w[0].1 + t * (w[1].1 - w[0].1) + 0.5) as u8;
            }
        }
        pts[pts.len() - 1].1 as u8
    });
    apply_lut(doc, layer, sel, &lut);
}

/// Image > Adjust > Photo Filter: tint toward `color` at the
/// pixel's own luminance, blended by `density` percent.
pub fn photo_filter(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    color: (u8, u8, u8),
    density: u8,
) {
    let adj = Adjustment::PhotoFilter { color, density };
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| {
            via_rgb(ink, |r, g, b| adj.apply(r, g, b))
        });
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, b| adj.apply(r, g, b))
    });
}

/// Image > Adjust > Posterize: reduce to `levels_count` tonal levels.
/// Image > Adjust > Equalize: redistribute each channel's values
/// so its histogram spans the full range evenly (darkest present
/// value -> 0, brightest -> 255, cumulative in between). The
/// histogram is gathered over the affected region (selection
/// strength >= half).
pub fn equalize(doc: &mut Document, layer: usize, sel: Option<&Selection>) {
    let mut hist = [[0u32; 256]; 3];
    let mut total: u32 = 0;
    {
        let l = &doc.layers[layer];
        let offset = l.offset;
        for coord in l.tiles.keys() {
            let tile = &l.tiles[coord];
            for ly in 0..256i32 {
                for lx in 0..256i32 {
                    let p = tile.get(lx as u32, ly as u32);
                    if p.a == 0 {
                        continue;
                    }
                    let (dx, dy) = (coord.0 * 256 + lx + offset.0, coord.1 * 256 + ly + offset.1);
                    let s = sel.map(|s| s.value(dx, dy)).unwrap_or(255);
                    if s < 128 {
                        continue;
                    }
                    let (r, g, b, _) = p.to_rgba8();
                    hist[0][r as usize] += 1;
                    hist[1][g as usize] += 1;
                    hist[2][b as usize] += 1;
                    total += 1;
                }
            }
        }
    }
    if total == 0 {
        return;
    }
    let mut luts = [[0u8; 256]; 3];
    for (c, lut) in luts.iter_mut().enumerate() {
        let mut cdf = [0u32; 256];
        let mut run = 0u32;
        for i in 0..256 {
            run += hist[c][i];
            cdf[i] = run;
        }
        let cdf_min = cdf.iter().copied().find(|&v| v > 0).unwrap_or(0);
        let denom = (total - cdf_min).max(1) as f32;
        for i in 0..256 {
            lut[i] = if cdf[i] <= cdf_min {
                0
            } else {
                (((cdf[i] - cdf_min) as f32 / denom) * 255.0 + 0.5) as u8
            };
        }
    }
    map_layer_pixels(doc, layer, sel, |p| {
        let (r, g, b, a) = p.to_rgba8();
        Pixel16::from_rgba8(
            luts[0][r as usize],
            luts[1][g as usize],
            luts[2][b as usize],
            a,
        )
    });
}

pub fn posterize(doc: &mut Document, layer: usize, sel: Option<&Selection>, levels_count: u8) {
    let n = levels_count.clamp(2, 255) as f32;
    let q = |v: f32| ((v * (n - 1.0)).round() / (n - 1.0)).clamp(0.0, 1.0);
    if doc.mode == ColorMode::Cmyk {
        map_layer_inks(doc, layer, sel, |ink| {
            [q(ink[0]), q(ink[1]), q(ink[2]), q(ink[3])]
        });
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, b| (q(r), q(g), q(b)))
    });
}

/// Image > Adjust > Threshold: luminance above `level` (0..=255)
/// becomes white, below becomes black. Alpha preserved.
pub fn threshold(doc: &mut Document, layer: usize, sel: Option<&Selection>, level: u8) {
    let t = level as f32 / 255.0;
    if doc.mode == ColorMode::Cmyk {
        // Threshold on the proof's luminance; white is no ink,
        // black is K only.
        map_layer_inks(doc, layer, sel, |ink| {
            let (r, g, b) = cmyk_to_rgb(ink[0], ink[1], ink[2], ink[3]);
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            if lum >= t {
                [0.0, 0.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        });
        return;
    }
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, b| {
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            if lum >= t {
                (1.0, 1.0, 1.0)
            } else {
                (0.0, 0.0, 0.0)
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(px: (u8, u8, u8)) -> Document {
        let mut doc = Document::new(4, 4);
        doc.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(px.0, px.1, px.2, 255));
        doc
    }

    #[test]
    fn invert_swaps_channels_and_keeps_alpha() {
        let mut doc = doc_with((255, 0, 128));
        invert(&mut doc, 0, None);
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8(), (0, 255, 127, 255));
    }

    #[test]
    fn adjustments_skip_transparent_pixels() {
        let mut doc = Document::new(4, 4);
        invert(&mut doc, 0, None);
        assert_eq!(doc.layers[0].get_pixel(1, 1), Pixel16::TRANSPARENT);
    }

    #[test]
    fn selection_limits_the_effect() {
        let mut doc = doc_with((100, 100, 100));
        let sel = Selection::rect(4, 4, 0, 0, 2, 4);
        invert(&mut doc, 0, Some(&sel));
        assert_eq!(doc.layers[0].get_pixel(0, 0).to_rgba8().0, 155);
        assert_eq!(doc.layers[0].get_pixel(3, 0).to_rgba8().0, 100);
    }

    #[test]
    fn levels_identity_changes_nothing() {
        let mut doc = doc_with((77, 150, 20));
        levels(&mut doc, 0, None, 0, 255, 1.0, 0, 255);
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8(), (77, 150, 20, 255));
    }

    #[test]
    fn levels_stretch_brightens_midtones() {
        let mut doc = doc_with((64, 64, 64));
        levels(&mut doc, 0, None, 0, 128, 1.0, 0, 255);
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 128);
    }

    #[test]
    fn desaturate_pure_red_is_mid_gray() {
        let mut doc = doc_with((255, 0, 0));
        desaturate(&mut doc, 0, None);
        let v = doc.layers[0].get_pixel(1, 1).to_rgba8();
        assert!((127..=128).contains(&v.0) && v.0 == v.1 && v.1 == v.2);
    }

    #[test]
    fn threshold_splits_on_luminance() {
        let mut doc = doc_with((200, 200, 200));
        threshold(&mut doc, 0, None, 128);
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 255);
    }

    #[test]
    fn posterize_quantizes() {
        let mut doc = doc_with((100, 100, 100));
        posterize(&mut doc, 0, None, 2);
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 0);
    }

    #[test]
    fn curves_identity_and_boost() {
        let mut doc = doc_with((64, 64, 64));
        curves(&mut doc, 0, None, &[(0, 0), (255, 255)]);
        assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 64);
        curves(&mut doc, 0, None, &[(0, 0), (128, 192), (255, 255)]);
        assert!(doc.layers[0].get_pixel(1, 1).to_rgba8().0 > 64);
    }

    #[test]
    fn brightness_positive_lifts_black() {
        let mut doc = doc_with((0, 0, 0));
        brightness_contrast(&mut doc, 0, None, 100, 0);
        assert!(doc.layers[0].get_pixel(1, 1).to_rgba8().0 > 80);
    }
}
