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
    /// Gradient Map (Phase 6): map luminance through a
    /// two-color gradient (from = shadows, to = highlights),
    /// optionally reversed.
    GradientMap {
        from: (u8, u8, u8),
        to: (u8, u8, u8),
        reverse: bool,
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
            Adjustment::GradientMap { .. } => "Gradient Map",
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
            Adjustment::GradientMap { from, to, reverse } => {
                let l = 0.299 * r + 0.587 * g + 0.114 * b;
                let t = if *reverse { 1.0 - l } else { l }.clamp(0.0, 1.0);
                let mix = |a: u8, b: u8| {
                    (a as f32 / 255.0) + ((b as f32 / 255.0) - (a as f32 / 255.0)) * t
                };
                (mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
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
    // Lab documents (Phase 6.5): adjustments are RGB-space math,
    // so each stored Lab pixel is decoded to RGB for `f` and the
    // result encoded back — the same round-trip the filter path
    // takes in CMYK mode.
    let lab = doc.mode == crate::document::ColorMode::Lab;
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
                let pin = if lab {
                    let (r, g, b, a) = p.to_f32s();
                    let (l, la, lb) = crate::color::slots_to_lab(r, g, b);
                    let (r, g, b) = crate::color::lab_to_rgb(l, la, lb);
                    Pixel16::from_f32s(r, g, b, a)
                } else {
                    p
                };
                let out = f(pin);
                let out = if strength == 255 {
                    out
                } else {
                    blend_px(pin, out, strength as f32 / 255.0)
                };
                let stored = if lab {
                    let (r, g, b, a) = out.to_f32s();
                    let (l, la, lb) = crate::color::rgb_to_lab(r, g, b);
                    let (sr, sg, sb) = crate::color::lab_to_slots(l, la, lb);
                    Pixel16::from_f32s(sr, sg, sb, a)
                } else {
                    out
                };
                if stored != p {
                    tile.set(lx as u32, ly as u32, stored);
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

/// Hue/Saturation restricted to a hue range (Phase 5): the
/// adjustment's strength falls off with circular distance from
/// `center_deg`, reaching zero at `falloff_deg`. This is the
/// Reds/Yellows/.../Magentas editing the classic dialog offers
/// beside Master.
#[allow(clippy::too_many_arguments)]
pub fn hue_saturation_range(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    hue_deg: i32,
    saturation: i32,
    lightness: i32,
    center_deg: f32,
    falloff_deg: f32,
) {
    let adj = Adjustment::HueSaturation {
        hue: hue_deg,
        saturation,
        lightness,
    };
    let math = |r: f32, g: f32, b: f32| {
        // Pixel hue in degrees.
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        if (max - min).abs() < 1e-6 {
            return (r, g, b); // grays have no hue to select by
        }
        let h = if (max - r).abs() < 1e-6 {
            60.0 * ((g - b) / (max - min)).rem_euclid(6.0)
        } else if (max - g).abs() < 1e-6 {
            60.0 * ((b - r) / (max - min) + 2.0)
        } else {
            60.0 * ((r - g) / (max - min) + 4.0)
        };
        let mut dist = (h - center_deg).abs() % 360.0;
        if dist > 180.0 {
            dist = 360.0 - dist;
        }
        let w = (1.0 - dist / falloff_deg.max(1.0)).clamp(0.0, 1.0);
        if w <= 0.0 {
            return (r, g, b);
        }
        let (ar, ag, ab) = adj.apply(r, g, b);
        (r + (ar - r) * w, g + (ag - g) * w, b + (ab - b) * w)
    };
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

/// Image > Adjustments > Gradient Map (Phase 6): map each
/// pixel's luminance through a two-color gradient.
pub fn gradient_map(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    from: (u8, u8, u8),
    to: (u8, u8, u8),
    reverse: bool,
) {
    let adj = Adjustment::GradientMap { from, to, reverse };
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

/// Channel Mixer settings (Phase 5): each output channel is a
/// weighted sum of the source channels plus a constant. Weights
/// are fractions (1.0 = 100%). Rows/channels are R, G, B (+ K in
/// CMYK documents, where the fourth row/column apply). With
/// `monochrome` set, row 0 drives every output channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelMixer {
    pub rows: [[f32; 4]; 4],
    pub constant: [f32; 4],
    pub monochrome: bool,
}

impl Default for ChannelMixer {
    fn default() -> Self {
        let mut rows = [[0f32; 4]; 4];
        for (i, row) in rows.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        ChannelMixer {
            rows,
            constant: [0.0; 4],
            monochrome: false,
        }
    }
}

/// Image > Adjust > Channel Mixer.
/// Selective Color (Phase 6.5 Batch 4): shift the CMYK
/// components of pixels belonging to one color range.
/// `range`: 0 Reds, 1 Yellows, 2 Greens, 3 Cyans, 4 Blues,
/// 5 Magentas, 6 Whites, 7 Neutrals, 8 Blacks. `adj` holds
/// C/M/Y/K shifts in -1..=1 (era -100..+100). Membership is
/// fuzzy: hue sectors use a triangular falloff, Whites /
/// Blacks fade in near the ends, Neutrals peak at zero
/// saturation. Relative shifts scale by the component's
/// current amount; absolute shifts apply flat.
pub fn selective_color(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    range: u8,
    adj: [f32; 4],
    relative: bool,
) {
    map_layer_pixels(doc, layer, sel, |p| {
        let (r, g, b, a) = p.to_f32s();
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let w = selective_weight(range, h, s, l);
        if w <= 0.0 {
            return p;
        }
        // RGB -> simple CMYK.
        let k = 1.0 - r.max(g).max(b);
        let (mut c, mut m, mut y) = if k >= 0.999 {
            (0.0, 0.0, 0.0)
        } else {
            (
                (1.0 - r - k) / (1.0 - k),
                (1.0 - g - k) / (1.0 - k),
                (1.0 - b - k) / (1.0 - k),
            )
        };
        let mut kk = k;
        let comps = [&mut c, &mut m, &mut y, &mut kk];
        for (comp, shift) in comps.into_iter().zip(adj.iter()) {
            if relative {
                *comp = (*comp + shift * w * *comp).clamp(0.0, 1.0);
            } else {
                *comp = (*comp + shift * w).clamp(0.0, 1.0);
            }
        }
        let nr = (1.0 - c) * (1.0 - kk);
        let ng = (1.0 - m) * (1.0 - kk);
        let nb = (1.0 - y) * (1.0 - kk);
        Pixel16::from_f32s(nr, ng, nb, a)
    });
}

fn selective_weight(range: u8, h: f32, s: f32, l: f32) -> f32 {
    // Hue sector membership (this module's hues are in
    // turns, 0..1): triangular falloff a sixth of a turn
    // wide around the sector center, gated by saturation so
    // grays stay out of the chromatic ranges.
    let sector = |center: f32| -> f32 {
        let mut d = (h - center).abs();
        if d > 0.5 {
            d = 1.0 - d;
        }
        (1.0 - d / (1.0 / 6.0)).clamp(0.0, 1.0) * s.clamp(0.0, 1.0)
    };
    match range {
        0 => sector(0.0),
        1 => sector(1.0 / 6.0),
        2 => sector(2.0 / 6.0),
        3 => sector(3.0 / 6.0),
        4 => sector(4.0 / 6.0),
        5 => sector(5.0 / 6.0),
        6 => ((l - 0.7) / 0.3).clamp(0.0, 1.0),
        7 => (1.0 - s * 3.0).clamp(0.0, 1.0) * (1.0 - (l - 0.5).abs() * 2.0).clamp(0.0, 1.0),
        _ => ((0.3 - l) / 0.3).clamp(0.0, 1.0),
    }
}

/// Replace Color (Phase 6.5 Batch 4): pixels resembling
/// `target` (RGB 0..=1) within `fuzziness` (0..=200, era
/// scale) get an HSL shift (hue degrees -180..=180,
/// saturation / lightness -100..=100), weighted by
/// resemblance so edges blend.
#[allow(clippy::too_many_arguments)]
pub fn replace_color(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    target: (f32, f32, f32),
    fuzziness: f32,
    hue: f32,
    sat: f32,
    light: f32,
) {
    let (th, ts, tl) = rgb_to_hsl(target.0, target.1, target.2);
    map_layer_pixels(doc, layer, sel, |p| {
        let (r, g, b, a) = p.to_f32s();
        let (h, s, l) = rgb_to_hsl(r, g, b);
        // Distance in HSL (hues in turns): hue is circular
        // and counts double weight, saturation / lightness
        // linear.
        let mut dh = (h - th).abs();
        if dh > 0.5 {
            dh = 1.0 - dh;
        }
        let dist = ((dh / 0.5).powi(2) * 2.0 + (s - ts).powi(2) + (l - tl).powi(2)).sqrt();
        let range = (fuzziness / 200.0) * 1.5 + 0.02;
        let w = (1.0 - dist / range).clamp(0.0, 1.0);
        if w <= 0.0 {
            return p;
        }
        let nh = (h + (hue / 360.0) * w).rem_euclid(1.0);
        let ns = (s + (sat / 100.0) * w).clamp(0.0, 1.0);
        let nl = (l + (light / 100.0) * w).clamp(0.0, 1.0);
        let (nr, ng, nb) = hsl_to_rgb(nh, ns, nl);
        Pixel16::from_f32s(nr, ng, nb, a)
    });
}

pub fn channel_mixer(
    doc: &mut Document,
    layer: usize,
    sel: Option<&Selection>,
    mix: &ChannelMixer,
) {
    if doc.mode == ColorMode::Cmyk {
        let rows = mix.rows;
        let constant = mix.constant;
        let mono = mix.monochrome;
        map_layer_inks(doc, layer, sel, |ink| {
            let mut out = [0f32; 4];
            for c in 0..4 {
                let row = if mono { &rows[0] } else { &rows[c] };
                out[c] = (row[0] * ink[0]
                    + row[1] * ink[1]
                    + row[2] * ink[2]
                    + row[3] * ink[3]
                    + if mono { constant[0] } else { constant[c] })
                .clamp(0.0, 1.0);
            }
            out
        });
        return;
    }
    let rows = mix.rows;
    let constant = mix.constant;
    let mono = mix.monochrome;
    map_layer_pixels(doc, layer, sel, |p| {
        map_rgb(p, |r, g, b| {
            let src = [r, g, b, 0.0];
            let mut out = [0f32; 3];
            for c in 0..3 {
                let row = if mono { &rows[0] } else { &rows[c] };
                out[c] = (row[0] * src[0]
                    + row[1] * src[1]
                    + row[2] * src[2]
                    + if mono { constant[0] } else { constant[c] })
                .clamp(0.0, 1.0);
            }
            (out[0], out[1], out[2])
        })
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

/// Layer > Matting > Defringe (Phase 6.5): replace the color
/// of partially transparent edge pixels with the color of
/// the nearest fully opaque pixel within `width` + 1 px, so
/// a cut-out object loses the halo of its old background.
/// Alpha is untouched.
pub fn defringe(doc: &mut Document, layer: usize, width: u32) {
    let Some(b) = crate::transform::layer_painted_bounds(&doc.layers[layer]) else {
        return;
    };
    let cmyk = doc.mode == crate::document::ColorMode::Cmyk;
    let alpha = |doc: &Document, x: i32, y: i32| -> u16 {
        if cmyk {
            doc.layers[layer].get_alpha(x, y)
        } else {
            doc.layers[layer].get_pixel(x, y).a
        }
    };
    let (x0, y0, x1, y1) = (
        b.0.max(0),
        b.1.max(0),
        b.2.min(doc.width as i32),
        b.3.min(doc.height as i32),
    );
    let r = (width as i32 + 1).max(1);
    let mut fixes: Vec<(i32, i32, Pixel16)> = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let a = alpha(doc, x, y);
            if a == 0 || a == u16::MAX {
                continue;
            }
            // Nearest opaque pixel by expanding ring search.
            let mut best: Option<(i32, Pixel16)> = None;
            for rad in 1..=r {
                for dy in -rad..=rad {
                    for dx in -rad..=rad {
                        if dx.abs() != rad && dy.abs() != rad {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if alpha(doc, nx, ny) == u16::MAX {
                            let d2 = dx * dx + dy * dy;
                            if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                                best = Some((d2, doc.layers[layer].get_pixel(nx, ny)));
                            }
                        }
                    }
                }
                if best.is_some() {
                    break;
                }
            }
            if let Some((_, c)) = best {
                fixes.push((x, y, c));
            }
        }
    }
    for (x, y, c) in fixes {
        let mut p = doc.layers[layer].get_pixel(x, y);
        let a = p.a;
        p = Pixel16::new(c.r, c.g, c.b, a);
        doc.layers[layer].set_pixel(x, y, p);
    }
}

/// Layer > Matting > Remove Black/White Matte (Phase 6.5):
/// undo a composite over a black (`white == false`) or
/// white matte: c' = (c - m(1-a)) / a per channel. Alpha is
/// unchanged; fully transparent pixels are skipped.
pub fn remove_matte(doc: &mut Document, layer: usize, white: bool) {
    let m = if white { 1.0f32 } else { 0.0 };
    map_layer_pixels(doc, layer, None, |p| {
        let a = p.a as f32 / 65535.0;
        if a <= 0.0 {
            return p;
        }
        let f = |v: u16| -> u16 {
            let c = v as f32 / 65535.0;
            (((c - m * (1.0 - a)) / a).clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
        };
        Pixel16::new(f(p.r), f(p.g), f(p.b), p.a)
    });
}

/// Edit > Stroke (Phase 6.5): paint `color` along the
/// selection boundary on the active layer. `position`:
/// 0 = Inside, 1 = Center, 2 = Outside. Distances come from
/// iterative erosion of the selection mask, capped at
/// `width` (1..=64). `opacity` 0.0..=1.0.
pub fn stroke_selection(
    doc: &mut Document,
    layer: usize,
    sel: &Selection,
    width: u32,
    color: Pixel16,
    position: u8,
    opacity: f32,
) {
    let (w, h) = (doc.width as i32, doc.height as i32);
    if w <= 0 || h <= 0 {
        return;
    }
    let width = width.clamp(1, 64) as i32;
    let inside =
        |x: i32, y: i32| -> bool { x >= 0 && y >= 0 && x < w && y < h && sel.value(x, y) >= 128 };
    // Distance (in px, capped at width) from a selected pixel
    // to the nearest unselected pixel, and vice versa.
    let dist = |want_inside: bool| -> Vec<i32> {
        let mut d = vec![0i32; (w * h) as usize];
        let mut cur: Vec<bool> = (0..w * h)
            .map(|i| inside(i % w, i / w) == want_inside)
            .collect();
        for step in 1..=width {
            let mut next = cur.clone();
            let mut any = false;
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    if cur[i] && d[i] == 0 {
                        let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                            let (nx, ny) = (x + dx, y + dy);
                            nx < 0 || ny < 0 || nx >= w || ny >= h || !cur[(ny * w + nx) as usize]
                        });
                        if edge {
                            d[i] = step;
                            next[i] = false;
                            any = true;
                        }
                    }
                }
            }
            cur = next;
            if !any {
                break;
            }
        }
        // Pixels never reached are deeper than the cap.
        for i in 0..(w * h) as usize {
            if cur[i] && d[i] == 0 {
                d[i] = width + 1;
            }
        }
        d
    };
    let din = dist(true);
    let dout = dist(false);
    let (half_in, half_out) = match position {
        0 => (width, 0),
        2 => (0, width),
        _ => ((width + 1) / 2, width / 2),
    };
    let cf = color.to_f32s();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let hit = if inside(x, y) {
                half_in > 0 && din[i] <= half_in
            } else {
                half_out > 0 && dout[i] <= half_out
            };
            if !hit {
                continue;
            }
            let dst = doc.layers[layer].get_pixel(x, y);
            let df = dst.to_f32s();
            let sa = cf.3 * opacity;
            let out_a = sa + df.3 * (1.0 - sa);
            if out_a <= 0.0 {
                continue;
            }
            let mix = |sc: f32, dc: f32| (sc * sa + dc * df.3 * (1.0 - sa)) / out_a;
            let p = Pixel16::from_f32s(mix(cf.0, df.0), mix(cf.1, df.1), mix(cf.2, df.2), out_a);
            doc.layers[layer].set_pixel(x, y, p);
        }
    }
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
