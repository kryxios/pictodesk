//! Layer effects (Phase 5): the classic five — Drop Shadow,
//! Inner Shadow, Outer Glow, Inner Glow, Bevel and Emboss.
//!
//! Effects are computed from the layer's own alpha over a region
//! buffer: `below_buffer` produces what renders UNDER the layer
//! content (drop shadow, outer glow), and `apply_within` modifies
//! the content buffer itself (inner shadow, inner glow, bevel).
//! The document compositor calls both when a layer carries
//! effects. Effect blend modes are honored where the effect meets
//! the layer's own pixels; the below-buffer is merged in effect
//! order and composited under the layer with Normal, so a below
//! effect's mode does not reach the backdrop — a known v1
//! simplification, recorded here rather than hidden.

use crate::blend::BlendMode;
use crate::pixel::Pixel16;

/// Drop/inner shadow parameters (the classic dialog set).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub color: Pixel16,
    /// 0.0..=1.0
    pub opacity: f32,
    /// Light angle in degrees (0 = from the right, 90 = from above).
    pub angle: i32,
    pub use_global_angle: bool,
    /// Shadow offset in pixels.
    pub distance: u32,
    /// Blur radius in pixels.
    pub blur: u32,
    /// Choke/intensity percent: raises the shadow's alpha.
    pub intensity: u32,
}

impl Default for ShadowSpec {
    fn default() -> Self {
        ShadowSpec {
            enabled: false,
            mode: BlendMode::Multiply,
            color: Pixel16::from_rgba8(0, 0, 0, 255),
            opacity: 0.75,
            angle: 120,
            use_global_angle: true,
            distance: 5,
            blur: 5,
            intensity: 0,
        }
    }
}

/// Outer/inner glow parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowSpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub color: Pixel16,
    /// 0.0..=1.0
    pub opacity: f32,
    /// Blur radius in pixels.
    pub blur: u32,
    /// Intensity percent: raises the glow's alpha.
    pub intensity: u32,
    /// Inner glow only: true = glow wells up from the center,
    /// false = glow creeps in from the edges.
    pub source_center: bool,
}

impl Default for GlowSpec {
    fn default() -> Self {
        GlowSpec {
            enabled: false,
            mode: BlendMode::Screen,
            color: Pixel16::from_rgba8(255, 255, 190, 255),
            opacity: 0.75,
            blur: 5,
            intensity: 0,
            source_center: false,
        }
    }
}

/// Bevel and Emboss parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BevelSpec {
    pub enabled: bool,
    /// 0 Outer Bevel, 1 Inner Bevel, 2 Emboss, 3 Pillow Emboss.
    pub style: u8,
    /// Depth percent (100 = classic full strength).
    pub depth: u32,
    /// true = Up (light raises the near edge), false = Down.
    pub up: bool,
    pub hi_mode: BlendMode,
    pub hi_color: Pixel16,
    pub hi_opacity: f32,
    pub sh_mode: BlendMode,
    pub sh_color: Pixel16,
    pub sh_opacity: f32,
    pub angle: i32,
    pub use_global_angle: bool,
}

impl Default for BevelSpec {
    fn default() -> Self {
        BevelSpec {
            enabled: false,
            style: 1,
            depth: 100,
            up: true,
            hi_mode: BlendMode::Screen,
            hi_color: Pixel16::from_rgba8(255, 255, 255, 255),
            hi_opacity: 1.0,
            sh_mode: BlendMode::Multiply,
            sh_color: Pixel16::from_rgba8(0, 0, 0, 255),
            sh_opacity: 1.0,
            angle: 120,
            use_global_angle: true,
        }
    }
}

/// The full effect set carried by a layer. All five default to
/// disabled; a layer with `effects: Some(..)` but nothing enabled
/// renders exactly like a plain layer.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Effects {
    pub drop_shadow: ShadowSpec,
    pub inner_shadow: ShadowSpec,
    pub outer_glow: GlowSpec,
    pub inner_glow: GlowSpec,
    pub bevel: BevelSpec,
}

impl Effects {
    pub fn any_enabled(&self) -> bool {
        self.drop_shadow.enabled
            || self.inner_shadow.enabled
            || self.outer_glow.enabled
            || self.inner_glow.enabled
            || self.bevel.enabled
    }

    /// Does this set paint anything under the layer content?
    pub fn has_below(&self) -> bool {
        self.drop_shadow.enabled || self.outer_glow.enabled
    }

    /// Extra margin (pixels) a render region needs around the
    /// layer content so blurs and offsets aren't clipped.
    pub fn margin(&self) -> i32 {
        let mut m = 0i32;
        if self.drop_shadow.enabled {
            m = m.max(self.drop_shadow.blur as i32 * 2 + self.drop_shadow.distance as i32 + 2);
        }
        if self.outer_glow.enabled {
            m = m.max(self.outer_glow.blur as i32 * 2 + 2);
        }
        if self.bevel.enabled {
            m = m.max(6);
        }
        if self.inner_shadow.enabled {
            m = m.max(self.inner_shadow.blur as i32 * 2 + self.inner_shadow.distance as i32 + 2);
        }
        if self.inner_glow.enabled {
            m = m.max(self.inner_glow.blur as i32 * 2 + 2);
        }
        m
    }
}

fn alpha_plane(px: &[Pixel16]) -> Vec<f32> {
    px.iter().map(|p| p.to_f32s().3).collect()
}

/// Separable box blur over an f32 plane, `passes` iterations of
/// radius `r` (approximates a gaussian of similar reach).
fn blur_plane(src: &[f32], w: usize, h: usize, r: i32, passes: u32) -> Vec<f32> {
    if r <= 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let mut cur = src.to_vec();
    let mut tmp = vec![0f32; w * h];
    for _ in 0..passes {
        // Horizontal.
        for y in 0..h {
            let row = y * w;
            let mut acc = 0f32;
            for x in 0..w as i32 {
                let add = x + r;
                let sub = x - r - 1;
                if add < w as i32 {
                    acc += cur[row + add as usize];
                } else {
                    acc += cur[row + w - 1];
                }
                if sub >= 0 {
                    acc -= cur[row + sub as usize];
                } else {
                    acc -= cur[row];
                }
                // Prime the accumulator at x = 0.
                if x == 0 {
                    acc = 0f32;
                    for k in -r..=r {
                        let xx = k.clamp(0, w as i32 - 1) as usize;
                        acc += cur[row + xx];
                    }
                }
                tmp[row + x as usize] = acc / (2 * r + 1) as f32;
            }
        }
        std::mem::swap(&mut cur, &mut tmp);
        // Vertical.
        for x in 0..w {
            let mut acc = 0f32;
            for y in 0..h as i32 {
                if y == 0 {
                    acc = 0f32;
                    for k in -r..=r {
                        let yy = k.clamp(0, h as i32 - 1) as usize;
                        acc += cur[yy * w + x];
                    }
                } else {
                    let add = y + r;
                    let sub = y - r - 1;
                    if add < h as i32 {
                        acc += cur[add as usize * w + x];
                    } else {
                        acc += cur[(h - 1) * w + x];
                    }
                    if sub >= 0 {
                        acc -= cur[sub as usize * w + x];
                    } else {
                        acc -= cur[x];
                    }
                }
                tmp[y as usize * w + x] = acc / (2 * r + 1) as f32;
            }
        }
        std::mem::swap(&mut cur, &mut tmp);
    }
    cur
}

fn offset_of(angle_deg: i32, distance: u32) -> (i32, i32) {
    let a = (angle_deg as f32).to_radians();
    // Screen space: y grows downward; a 120-degree light throws
    // its shadow down-left.
    (
        (-a.cos() * distance as f32).round() as i32,
        (a.sin() * distance as f32).round() as i32,
    )
}

fn sample_plane(plane: &[f32], w: usize, h: usize, x: i32, y: i32) -> f32 {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        0.0
    } else {
        plane[y as usize * w + x as usize]
    }
}

/// Blend `color` over `px` per channel with `mode`, mixed by
/// `strength` (0..=1), writing rgb only (alpha untouched).
fn blend_toward(px: &mut Pixel16, mode: BlendMode, color: Pixel16, strength: f32) {
    if strength <= 0.0 {
        return;
    }
    let (r, g, b, a) = px.to_f32s();
    let (cr, cg, cb, _) = color.to_f32s();
    let mix = |base: f32, src: f32| {
        let blended = mode.blend_channel(base, src);
        base + (blended - base) * strength.clamp(0.0, 1.0)
    };
    *px = Pixel16::from_f32s(mix(r, cr), mix(g, cg), mix(b, cb), a);
}

/// Composite a colored-alpha pixel over a buffer pixel (Normal).
fn over(dst: &mut Pixel16, rgb: (f32, f32, f32), alpha: f32) {
    let (dr, dg, db, da) = dst.to_f32s();
    let out_a = alpha + da * (1.0 - alpha);
    if out_a <= 0.0 {
        *dst = Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0);
        return;
    }
    let mix = |d: f32, s: f32| (s * alpha + d * da * (1.0 - alpha)) / out_a;
    *dst = Pixel16::from_f32s(mix(dr, rgb.0), mix(dg, rgb.1), mix(db, rgb.2), out_a);
}

/// What renders UNDER the layer: outer glow first, then the drop
/// shadow over it (the classic stacking). Buffer is the same size
/// as the content buffer passed in; callers render content into a
/// region expanded by `Effects::margin` so offsets fit.
pub fn below_buffer(
    content: &[Pixel16],
    w: usize,
    h: usize,
    fx: &Effects,
    global_angle: i32,
) -> Vec<Pixel16> {
    let mut out = vec![Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0); w * h];
    if w == 0 || h == 0 {
        return out;
    }
    let alpha = alpha_plane(content);
    if fx.outer_glow.enabled {
        let g = &fx.outer_glow;
        let blurred = blur_plane(&alpha, w, h, g.blur as i32, 2);
        let boost = 1.0 + g.intensity as f32 / 100.0 * 1.5;
        let (cr, cg, cb, _) = g.color.to_f32s();
        for i in 0..w * h {
            let a = (blurred[i] * boost).clamp(0.0, 1.0) * g.opacity;
            if a > 0.0 {
                over(&mut out[i], (cr, cg, cb), a);
            }
        }
    }
    if fx.drop_shadow.enabled {
        let s = &fx.drop_shadow;
        let angle = if s.use_global_angle {
            global_angle
        } else {
            s.angle
        };
        let (dx, dy) = offset_of(angle, s.distance);
        let blurred = blur_plane(&alpha, w, h, s.blur as i32, 2);
        let boost = 1.0 + s.intensity as f32 / 100.0 * 1.5;
        let (cr, cg, cb, _) = s.color.to_f32s();
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let a = (sample_plane(&blurred, w, h, x - dx, y - dy) * boost).clamp(0.0, 1.0)
                    * s.opacity;
                if a > 0.0 {
                    over(&mut out[(y as usize) * w + x as usize], (cr, cg, cb), a);
                }
            }
        }
    }
    out
}

/// Apply the within-content effects in place: inner shadow, inner
/// glow, then bevel shading.
pub fn apply_within(content: &mut [Pixel16], w: usize, h: usize, fx: &Effects, global_angle: i32) {
    if w == 0 || h == 0 {
        return;
    }
    let alpha = alpha_plane(content);
    if fx.inner_shadow.enabled {
        let s = &fx.inner_shadow;
        let angle = if s.use_global_angle {
            global_angle
        } else {
            s.angle
        };
        let (dx, dy) = offset_of(angle, s.distance);
        // Blur the alpha, then sample it shifted TOWARD the light:
        // deep inside the shape the sample stays ~1 (no shadow);
        // near the edge away from the light it falls, and the
        // shortfall becomes shadow.
        let blurred = blur_plane(&alpha, w, h, s.blur as i32, 2);
        let boost = 1.0 + s.intensity as f32 / 100.0;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * w + x as usize;
                if alpha[i] <= 0.0 {
                    continue;
                }
                let t = sample_plane(&blurred, w, h, x + dx, y + dy);
                let shade = ((1.0 - t) * boost).clamp(0.0, 1.0) * alpha[i] * s.opacity;
                if shade > 0.0 {
                    blend_toward(&mut content[i], s.mode, s.color, shade);
                }
            }
        }
    }
    if fx.inner_glow.enabled {
        let g = &fx.inner_glow;
        let blurred = blur_plane(&alpha, w, h, g.blur as i32, 2);
        let boost = 1.0 + g.intensity as f32 / 100.0 * 0.5;
        for i in 0..w * h {
            if alpha[i] <= 0.0 {
                continue;
            }
            let e = if g.source_center {
                alpha[i] * blurred[i] * blurred[i]
            } else {
                ((alpha[i] - blurred[i]) * 3.0).clamp(0.0, 1.0) * alpha[i]
            };
            let strength = (e * boost).clamp(0.0, 1.0) * g.opacity;
            if strength > 0.0 {
                blend_toward(&mut content[i], g.mode, g.color, strength);
            }
        }
    }
    if fx.bevel.enabled {
        let b = &fx.bevel;
        let angle = if b.use_global_angle {
            global_angle
        } else {
            b.angle
        };
        let a = (angle as f32).to_radians();
        let (lx, ly) = (-a.cos(), -a.sin());
        let r = ((b.depth as f32 / 100.0) * 3.0).round() as i32;
        let height = blur_plane(&alpha, w, h, r.max(1), 2);
        let strength_scale = b.depth as f32 / 100.0 * 2.0;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * w + x as usize;
                let gx =
                    sample_plane(&height, w, h, x + 1, y) - sample_plane(&height, w, h, x - 1, y);
                let gy =
                    sample_plane(&height, w, h, x, y + 1) - sample_plane(&height, w, h, x, y - 1);
                let mut shade = (gx * lx + gy * ly) * strength_scale;
                if !b.up {
                    shade = -shade;
                }
                if shade.abs() < 0.004 {
                    continue;
                }
                let inside = alpha[i] >= 0.5;
                let zone = match b.style {
                    0 => !inside && height[i] > 0.02, // Outer
                    1 => inside,                      // Inner
                    2 => height[i] > 0.02,            // Emboss
                    _ => height[i] > 0.02,            // Pillow (sign flips below)
                };
                if !zone {
                    continue;
                }
                if b.style == 3 {
                    shade = -shade;
                }
                let mag = shade.abs().clamp(0.0, 1.0);
                if shade > 0.0 {
                    if inside {
                        blend_toward(&mut content[i], b.hi_mode, b.hi_color, mag * b.hi_opacity);
                    } else {
                        // Outer-bevel highlight lives on transparent
                        // pixels beside the content: give them the
                        // highlight color and the bevel's alpha.
                        let ba = (height[i] * mag * b.hi_opacity).clamp(0.0, 1.0);
                        let (cr, cg, cb, _) = b.hi_color.to_f32s();
                        over(&mut content[i], (cr, cg, cb), ba);
                    }
                } else if inside {
                    blend_toward(&mut content[i], b.sh_mode, b.sh_color, mag * b.sh_opacity);
                } else {
                    let ba = (height[i] * mag * b.sh_opacity).clamp(0.0, 1.0);
                    let (cr, cg, cb, _) = b.sh_color.to_f32s();
                    over(&mut content[i], (cr, cg, cb), ba);
                }
            }
        }
    }
}
