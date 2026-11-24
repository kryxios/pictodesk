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
    /// Contour curve (Phase 6): 0 Linear, 1 Cone, 2 Ring,
    /// 3 Cove. Applied to the blurred alpha before compositing.
    pub contour: u8,
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
            contour: 0,
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
    /// Contour curve (Phase 6), same numbering as ShadowSpec.
    pub contour: u8,
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
            contour: 0,
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
    /// Contour curve (Phase 6), same numbering as ShadowSpec.
    pub contour: u8,
    /// Texture (Phase 6): modulate the bevel shading with a
    /// built-in pattern (0 = none/checker when on, 1 = dots,
    /// 2 = lines). `texture_depth` scales the modulation.
    pub texture: bool,
    pub texture_kind: u8,
    pub texture_depth: u32,
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
            contour: 0,
            texture: false,
            texture_kind: 0,
            texture_depth: 100,
        }
    }
}

/// Satin (Phase 6): two offset, blurred copies of the shape
/// blended inside it, giving a satiny sheen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatinSpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub color: Pixel16,
    pub opacity: f32,
    pub angle: i32,
    pub distance: u32,
    pub blur: u32,
    /// Invert the satin (swap light/dark lobes).
    pub invert: bool,
    pub contour: u8,
}

impl Default for SatinSpec {
    fn default() -> Self {
        SatinSpec {
            enabled: false,
            mode: BlendMode::Multiply,
            color: Pixel16::from_rgba8(0, 0, 0, 255),
            opacity: 0.5,
            angle: 19,
            distance: 11,
            blur: 14,
            invert: false,
            contour: 0,
        }
    }
}

/// Color Overlay (Phase 6): a flat color blended over the
/// layer's opaque pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorOverlaySpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub color: Pixel16,
    pub opacity: f32,
}

impl Default for ColorOverlaySpec {
    fn default() -> Self {
        ColorOverlaySpec {
            enabled: false,
            mode: BlendMode::Normal,
            color: Pixel16::from_rgba8(255, 0, 0, 255),
            opacity: 1.0,
        }
    }
}

/// Gradient Overlay (Phase 6): a two-color gradient blended
/// over the layer's opaque pixels, mapped over the content's
/// bounds. Style 0 = Linear (at `angle`), 1 = Radial.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientOverlaySpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub opacity: f32,
    pub from: Pixel16,
    pub to: Pixel16,
    pub style: u8,
    pub angle: i32,
    /// Gradient scale percent (100 = spans the content bounds).
    pub scale: u32,
}

impl Default for GradientOverlaySpec {
    fn default() -> Self {
        GradientOverlaySpec {
            enabled: false,
            mode: BlendMode::Normal,
            opacity: 1.0,
            from: Pixel16::from_rgba8(0, 0, 0, 255),
            to: Pixel16::from_rgba8(255, 255, 255, 255),
            style: 0,
            angle: 90,
            scale: 100,
        }
    }
}

/// Pattern Overlay (Phase 6): a built-in pattern blended over
/// the layer's opaque pixels. Pattern kinds: 0 checker,
/// 1 dots, 2 diagonal lines (the app's user patterns ride the
/// pattern stamp; overlays use these built-ins in v1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternOverlaySpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub opacity: f32,
    pub kind: u8,
    /// Pattern scale percent.
    pub scale: u32,
}

impl Default for PatternOverlaySpec {
    fn default() -> Self {
        PatternOverlaySpec {
            enabled: false,
            mode: BlendMode::Normal,
            opacity: 1.0,
            kind: 0,
            scale: 100,
        }
    }
}

/// Stroke (Phase 6): an outline around the layer's alpha edge.
/// Position: 0 Outside, 1 Inside, 2 Center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeSpec {
    pub enabled: bool,
    pub mode: BlendMode,
    pub color: Pixel16,
    pub opacity: f32,
    /// Stroke width in pixels.
    pub size: u32,
    pub position: u8,
}

impl Default for StrokeSpec {
    fn default() -> Self {
        StrokeSpec {
            enabled: false,
            mode: BlendMode::Normal,
            color: Pixel16::from_rgba8(0, 0, 0, 255),
            opacity: 1.0,
            size: 3,
            position: 0,
        }
    }
}

/// Apply a contour curve to a 0..=1 alpha value (Phase 6).
pub fn contour_curve(kind: u8, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        1 => t * t,                                   // Cone (peaked falloff)
        2 => (t * std::f32::consts::TAU).sin().abs(), // Ring
        3 => 1.0 - (1.0 - t) * (1.0 - t),             // Cove (fast rise)
        _ => t,                                       // Linear
    }
}

/// A built-in pattern's coverage at a point (Phase 6 overlays
/// and bevel texture): returns 0..=1.
pub fn pattern_value(kind: u8, x: i32, y: i32, scale: u32) -> f32 {
    let cell = ((scale.max(10) as f32 / 100.0) * 8.0).max(2.0);
    let (fx, fy) = (x as f32 / cell, y as f32 / cell);
    match kind {
        1 => {
            let d = ((fx.fract() - 0.5).powi(2) + (fy.fract() - 0.5).powi(2)).sqrt();
            if d < 0.32 {
                1.0
            } else {
                0.0
            }
        }
        2 => {
            if ((fx + fy).fract()) < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        _ => {
            if (fx.floor() as i32 + fy.floor() as i32) % 2 == 0 {
                1.0
            } else {
                0.0
            }
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
    // Phase 6 additions.
    pub satin: SatinSpec,
    pub color_overlay: ColorOverlaySpec,
    pub gradient_overlay: GradientOverlaySpec,
    pub pattern_overlay: PatternOverlaySpec,
    pub stroke: StrokeSpec,
}

impl Effects {
    pub fn any_enabled(&self) -> bool {
        self.drop_shadow.enabled
            || self.inner_shadow.enabled
            || self.outer_glow.enabled
            || self.inner_glow.enabled
            || self.bevel.enabled
            || self.satin.enabled
            || self.color_overlay.enabled
            || self.gradient_overlay.enabled
            || self.pattern_overlay.enabled
            || self.stroke.enabled
    }

    /// Does this set paint anything under the layer content?
    pub fn has_below(&self) -> bool {
        self.drop_shadow.enabled
            || self.outer_glow.enabled
            || (self.stroke.enabled && self.stroke.position != 1)
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
        if self.stroke.enabled && self.stroke.position != 1 {
            m = m.max(self.stroke.size as i32 + 2);
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

/// Max-filter dilation of an alpha plane by radius `r`
/// (separable approximation, two passes).
fn dilate_plane(src: &[f32], w: usize, h: usize, r: i32) -> Vec<f32> {
    if r <= 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let mut cur = src.to_vec();
    let mut tmp = vec![0f32; w * h];
    for _ in 0..2 {
        for y in 0..h {
            for x in 0..w {
                let mut m = 0f32;
                for k in -r..=r {
                    m = m.max(sample_plane(&cur, w, h, x as i32 + k, y as i32));
                }
                tmp[y * w + x] = m;
            }
        }
        std::mem::swap(&mut cur, &mut tmp);
        for x in 0..w {
            for y in 0..h {
                let mut m = 0f32;
                for k in -r..=r {
                    m = m.max(sample_plane(&cur, w, h, x as i32, y as i32 + k));
                }
                tmp[y * w + x] = m;
            }
        }
        std::mem::swap(&mut cur, &mut tmp);
    }
    cur
}

/// Erosion via the complement of a dilation.
fn erode_plane(src: &[f32], w: usize, h: usize, r: i32) -> Vec<f32> {
    let inv: Vec<f32> = src.iter().map(|a| 1.0 - a).collect();
    dilate_plane(&inv, w, h, r)
        .iter()
        .map(|a| 1.0 - a)
        .collect()
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
        let blurred: Vec<f32> = blur_plane(&alpha, w, h, s.blur as i32, 2)
            .iter()
            .map(|a| contour_curve(s.contour, *a))
            .collect();
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
    if fx.stroke.enabled && fx.stroke.position != 1 {
        // Outside (0) or Center (2): the ring between the alpha
        // and its dilation (full size for Outside, half for
        // Center), painted under the content.
        let st = &fx.stroke;
        let r = if st.position == 2 {
            (st.size / 2).max(1)
        } else {
            st.size
        };
        let dil = dilate_plane(&alpha, w, h, r as i32);
        let (cr, cg, cb, _) = st.color.to_f32s();
        for i in 0..w * h {
            let ring = (dil[i] - alpha[i]).clamp(0.0, 1.0) * st.opacity;
            if ring > 0.0 {
                over(&mut out[i], (cr, cg, cb), ring);
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

    // ---- Phase 6 within-content effects, in dialog order:
    // satin, then the three overlays, then the inside stroke.
    if fx.satin.enabled {
        let sp = &fx.satin;
        let (dx, dy) = offset_of(sp.angle, sp.distance);
        let blurred = blur_plane(&alpha, w, h, sp.blur as i32, 2);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * w + x as usize;
                if alpha[i] <= 0.0 {
                    continue;
                }
                let a = sample_plane(&blurred, w, h, x - dx, y - dy);
                let b = sample_plane(&blurred, w, h, x + dx, y + dy);
                let mut lobe = ((a - b) * 0.5 + 0.5).clamp(0.0, 1.0);
                lobe = contour_curve(sp.contour, lobe);
                if sp.invert {
                    lobe = 1.0 - lobe;
                }
                let strength = ((lobe - 0.5).abs() * 2.0) * alpha[i] * sp.opacity;
                if strength > 0.01 {
                    blend_toward(&mut content[i], sp.mode, sp.color, strength);
                }
            }
        }
    }
    if fx.color_overlay.enabled {
        let o = &fx.color_overlay;
        for i in 0..w * h {
            if alpha[i] > 0.0 {
                blend_toward(&mut content[i], o.mode, o.color, o.opacity * alpha[i]);
            }
        }
    }
    if fx.gradient_overlay.enabled {
        let o = &fx.gradient_overlay;
        // Map over the content's painted bounds.
        let (mut x0, mut y0, mut x1, mut y1) = (w as i32, h as i32, -1i32, -1i32);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                if alpha[(y as usize) * w + x as usize] > 0.0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 >= x0 {
            let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
            let ang = (o.angle as f32).to_radians();
            let (ga_x, ga_y) = (ang.cos(), -ang.sin());
            let (fr, fg, fb, _) = o.from.to_f32s();
            let (tr, tg, tb, _) = o.to.to_f32s();
            let scale = (o.scale.max(10) as f32) / 100.0;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let i = (y as usize) * w + x as usize;
                    if alpha[i] <= 0.0 {
                        continue;
                    }
                    let (nx, ny) = ((x - x0) as f32 / bw - 0.5, (y - y0) as f32 / bh - 0.5);
                    let t = if o.style == 1 {
                        ((nx * nx + ny * ny).sqrt() * 1.42 / scale).clamp(0.0, 1.0)
                    } else {
                        ((nx * ga_x + ny * ga_y) / scale + 0.5).clamp(0.0, 1.0)
                    };
                    let col = Pixel16::from_f32s(
                        fr + (tr - fr) * t,
                        fg + (tg - fg) * t,
                        fb + (tb - fb) * t,
                        1.0,
                    );
                    blend_toward(&mut content[i], o.mode, col, o.opacity * alpha[i]);
                }
            }
        }
    }
    if fx.pattern_overlay.enabled {
        let o = &fx.pattern_overlay;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * w + x as usize;
                if alpha[i] <= 0.0 {
                    continue;
                }
                let v = pattern_value(o.kind, x, y, o.scale);
                let col = Pixel16::from_f32s(v, v, v, 1.0);
                blend_toward(&mut content[i], o.mode, col, o.opacity * alpha[i]);
            }
        }
    }
    if fx.stroke.enabled && fx.stroke.position != 0 {
        // Inside (1) or Center (2): the ring between the alpha
        // and its erosion, painted over the content edge.
        let st = &fx.stroke;
        let r = if st.position == 2 {
            (st.size / 2).max(1)
        } else {
            st.size
        };
        let ero = erode_plane(&alpha, w, h, r as i32);
        for i in 0..w * h {
            let ring = (alpha[i] - ero[i]).clamp(0.0, 1.0) * st.opacity;
            if ring > 0.0 {
                blend_toward(&mut content[i], st.mode, st.color, ring);
            }
        }
    }
}
