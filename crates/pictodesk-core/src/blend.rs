//! Blend modes + alpha compositing.
//!
//! Phase 0 ships the core mode set; the full classic set completes by
//! Phase 5 of the game plan. Modes operate per-channel on encoded values
//! in 0.0..=1.0, matching the classic default (non-linear) blending.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    Darken,
    Lighten,
    Difference,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Dissolve,
    HardLight,
    ColorDodge,
    ColorBurn,
    Exclusion,
    LinearBurn,
    LinearDodge,
    VividLight,
    LinearLight,
    PinLight,
}

/// All modes in menu order (the classic palette order).
pub const ALL_BLEND_MODES: [BlendMode; 22] = [
    BlendMode::Normal,
    BlendMode::Dissolve,
    BlendMode::Multiply,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::Screen,
    BlendMode::ColorDodge,
    BlendMode::LinearDodge,
    BlendMode::ColorBurn,
    BlendMode::LinearBurn,
    BlendMode::Overlay,
    BlendMode::SoftLight,
    BlendMode::HardLight,
    BlendMode::VividLight,
    BlendMode::LinearLight,
    BlendMode::PinLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

impl BlendMode {
    pub fn to_id(self) -> u8 {
        match self {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
            BlendMode::SoftLight => 4,
            BlendMode::Darken => 5,
            BlendMode::Lighten => 6,
            BlendMode::Difference => 7,
            BlendMode::Hue => 8,
            BlendMode::Saturation => 9,
            BlendMode::Color => 10,
            BlendMode::Luminosity => 11,
            BlendMode::Dissolve => 12,
            BlendMode::HardLight => 13,
            BlendMode::ColorDodge => 14,
            BlendMode::ColorBurn => 15,
            BlendMode::Exclusion => 16,
            BlendMode::LinearBurn => 17,
            BlendMode::LinearDodge => 18,
            BlendMode::VividLight => 19,
            BlendMode::LinearLight => 20,
            BlendMode::PinLight => 21,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::Difference => "Difference",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::Dissolve => "Dissolve",
            BlendMode::HardLight => "Hard Light",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::LinearBurn => "Linear Burn",
            BlendMode::LinearDodge => "Linear Dodge",
            BlendMode::VividLight => "Vivid Light",
            BlendMode::LinearLight => "Linear Light",
            BlendMode::PinLight => "Pin Light",
        }
    }

    /// The component modes (Hue/Saturation/Color/Luminosity) blend
    /// whole RGB triples, not per-channel values.
    pub fn is_component(self) -> bool {
        matches!(
            self,
            BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity
        )
    }

    pub fn from_id(id: u8) -> Option<BlendMode> {
        Some(match id {
            0 => BlendMode::Normal,
            1 => BlendMode::Multiply,
            2 => BlendMode::Screen,
            3 => BlendMode::Overlay,
            4 => BlendMode::SoftLight,
            5 => BlendMode::Darken,
            6 => BlendMode::Lighten,
            7 => BlendMode::Difference,
            8 => BlendMode::Hue,
            9 => BlendMode::Saturation,
            10 => BlendMode::Color,
            11 => BlendMode::Luminosity,
            12 => BlendMode::Dissolve,
            13 => BlendMode::HardLight,
            14 => BlendMode::ColorDodge,
            15 => BlendMode::ColorBurn,
            16 => BlendMode::Exclusion,
            17 => BlendMode::LinearBurn,
            18 => BlendMode::LinearDodge,
            19 => BlendMode::VividLight,
            20 => BlendMode::LinearLight,
            21 => BlendMode::PinLight,
            _ => return None,
        })
    }

    /// Blend one channel: `b` = backdrop (below), `s` = source (above).
    pub fn blend_channel(&self, b: f32, s: f32) -> f32 {
        match self {
            BlendMode::Normal => s,
            BlendMode::Multiply => b * s,
            BlendMode::Screen => 1.0 - (1.0 - b) * (1.0 - s),
            BlendMode::Overlay => {
                if b < 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }
            BlendMode::SoftLight => {
                if s < 0.5 {
                    b - (1.0 - 2.0 * s) * b * (1.0 - b)
                } else {
                    let d = if b < 0.25 {
                        ((16.0 * b - 12.0) * b + 4.0) * b
                    } else {
                        b.sqrt()
                    };
                    b + (2.0 * s - 1.0) * (d - b)
                }
            }
            BlendMode::Darken => b.min(s),
            BlendMode::Lighten => b.max(s),
            BlendMode::Difference => (b - s).abs(),
            BlendMode::HardLight => {
                if s < 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }
            BlendMode::ColorDodge => {
                if s >= 1.0 {
                    1.0
                } else {
                    (b / (1.0 - s)).min(1.0)
                }
            }
            BlendMode::ColorBurn => {
                if s <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - b) / s).min(1.0)
                }
            }
            BlendMode::Exclusion => b + s - 2.0 * b * s,
            BlendMode::LinearBurn => (b + s - 1.0).max(0.0),
            BlendMode::LinearDodge => (b + s).min(1.0),
            BlendMode::VividLight => {
                if s < 0.5 {
                    let d = 2.0 * s;
                    if d <= 0.0 {
                        0.0
                    } else {
                        1.0 - ((1.0 - b) / d).min(1.0)
                    }
                } else {
                    let d = 2.0 * (s - 0.5);
                    if d >= 1.0 {
                        1.0
                    } else {
                        (b / (1.0 - d)).min(1.0)
                    }
                }
            }
            BlendMode::LinearLight => {
                if s < 0.5 {
                    (b + 2.0 * s - 1.0).max(0.0)
                } else {
                    (b + 2.0 * (s - 0.5)).min(1.0)
                }
            }
            BlendMode::PinLight => {
                if s < 0.5 {
                    b.min(2.0 * s)
                } else {
                    b.max(2.0 * (s - 0.5))
                }
            }
            // Dissolve is not a channel formula: the compositor
            // gates the whole pixel on a deterministic hash (see
            // Document::render_pixel). Fall back to Normal here.
            BlendMode::Dissolve => s,
            // Component modes never reach blend_channel (see blend_rgb).
            BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => s,
        }
    }
}

/// Deterministic 0.0..1.0 hash for Dissolve: stable per pixel and
/// per layer, so renders (and checksums) don't shimmer between
/// frames. `seed` is the layer index.
pub fn dissolve_hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ seed.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % 100_000) as f32 / 100_000.0
}

// ---- Component blend modes (the classic PDF/SVG formulation) ----

fn lum(c: (f32, f32, f32)) -> f32 {
    0.3 * c.0 + 0.59 * c.1 + 0.11 * c.2
}

fn sat(c: (f32, f32, f32)) -> f32 {
    c.0.max(c.1).max(c.2) - c.0.min(c.1).min(c.2)
}

fn clip_color(mut c: (f32, f32, f32)) -> (f32, f32, f32) {
    let l = lum(c);
    let n = c.0.min(c.1).min(c.2);
    if n < 0.0 {
        let f = l / (l - n);
        c.0 = l + (c.0 - l) * f;
        c.1 = l + (c.1 - l) * f;
        c.2 = l + (c.2 - l) * f;
    }
    let x = c.0.max(c.1).max(c.2);
    if x > 1.0 {
        let f = (1.0 - l) / (x - l);
        c.0 = l + (c.0 - l) * f;
        c.1 = l + (c.1 - l) * f;
        c.2 = l + (c.2 - l) * f;
    }
    c
}

fn set_lum(c: (f32, f32, f32), l: f32) -> (f32, f32, f32) {
    let d = l - lum(c);
    clip_color((c.0 + d, c.1 + d, c.2 + d))
}

fn set_sat(c: (f32, f32, f32), s: f32) -> (f32, f32, f32) {
    // Order channels by value; stretch the min..max range to s.
    let mut idx = [0usize, 1, 2];
    let v = [c.0, c.1, c.2];
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap());
    let (mn, md, mx) = (v[idx[0]], v[idx[1]], v[idx[2]]);
    let mut out = [0.0f32; 3];
    if mx > mn {
        let mid = (md - mn) * s / (mx - mn);
        out[idx[0]] = 0.0;
        out[idx[1]] = mid;
        out[idx[2]] = s;
    }
    (out[0], out[1], out[2])
}

/// Blend whole RGB triples for the component modes; None for the
/// per-channel modes.
pub fn blend_rgb(
    mode: BlendMode,
    b: (f32, f32, f32),
    s: (f32, f32, f32),
) -> Option<(f32, f32, f32)> {
    Some(match mode {
        BlendMode::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        BlendMode::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        BlendMode::Color => set_lum(s, lum(b)),
        BlendMode::Luminosity => set_lum(b, lum(s)),
        _ => return None,
    })
}

/// Composite a source pixel over a backdrop pixel (Porter-Duff "over"
/// with a blend mode applied to the overlapping region), honoring layer
/// opacity. All values 0.0..=1.0, straight alpha in/out.
pub fn composite_over(
    backdrop: (f32, f32, f32, f32),
    source: (f32, f32, f32, f32),
    mode: BlendMode,
    opacity: f32,
) -> (f32, f32, f32, f32) {
    let (br, bg, bb, ba) = backdrop;
    let (sr, sg, sb, sa0) = source;
    let sa = sa0 * opacity.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return backdrop;
    }
    if ba <= 0.0 {
        return (sr, sg, sb, sa);
    }
    let out_a = sa + ba * (1.0 - sa);
    if out_a <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    // Blended color where both overlap, then standard over-composite of
    // the blended result treated at the source's coverage.
    let (mr, mg, mb) = match blend_rgb(mode, (br, bg, bb), (sr, sg, sb)) {
        Some(m) => m,
        None => (
            mode.blend_channel(br, sr),
            mode.blend_channel(bg, sg),
            mode.blend_channel(bb, sb),
        ),
    };
    // Weight: source contribution vs backdrop showing through.
    let chan = |m: f32, b: f32, s: f32| -> f32 {
        // Result of blending where source is opaque, mixed by coverage:
        // out = (sa*(1-ba)*s + sa*ba*m + (1-sa)*ba*b) / out_a
        (sa * (1.0 - ba) * s + sa * ba * m + (1.0 - sa) * ba * b) / out_a
    };
    (chan(mr, br, sr), chan(mg, bg, sg), chan(mb, bb, sb), out_a)
}

/// Composite a CMYK source pixel over a backdrop in ink space.
/// Inks are (c, m, y, k) in 0.0..=1.0 with a separate straight alpha
/// — the five-value CMYK pixel. Blend modes apply per ink channel;
/// the coverage math is Porter-Duff "over", same as composite_over.
pub fn composite_over_inks(
    backdrop: ([f32; 4], f32),
    source: ([f32; 4], f32),
    mode: BlendMode,
    opacity: f32,
) -> ([f32; 4], f32) {
    let (b, ba) = backdrop;
    let (s, sa0) = source;
    let sa = sa0 * opacity.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return backdrop;
    }
    if ba <= 0.0 {
        return (s, sa);
    }
    let out_a = sa + ba * (1.0 - sa);
    if out_a <= 0.0 {
        return ([0.0; 4], 0.0);
    }
    let blended: [f32; 4] = match blend_rgb(
        mode,
        crate::color::cmyk_to_rgb(b[0], b[1], b[2], b[3]),
        crate::color::cmyk_to_rgb(s[0], s[1], s[2], s[3]),
    ) {
        Some(m) => {
            let (c, m2, y, k) = crate::color::rgb_to_cmyk(m.0, m.1, m.2);
            [c, m2, y, k]
        }
        None => {
            let mut r = [0.0f32; 4];
            for i in 0..4 {
                r[i] = mode.blend_channel(b[i], s[i]);
            }
            r
        }
    };
    let mut out = [0.0f32; 4];
    for i in 0..4 {
        out[i] = (sa * (1.0 - ba) * s[i] + sa * ba * blended[i] + (1.0 - sa) * ba * b[i]) / out_a;
    }
    (out, out_a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn mode_math_spot_checks() {
        assert!(close(BlendMode::Multiply.blend_channel(0.5, 0.5), 0.25));
        assert!(close(BlendMode::Screen.blend_channel(0.5, 0.5), 0.75));
        assert!(close(BlendMode::Difference.blend_channel(0.8, 0.3), 0.5));
        assert!(close(BlendMode::Darken.blend_channel(0.8, 0.3), 0.3));
        assert!(close(BlendMode::Lighten.blend_channel(0.8, 0.3), 0.8));
        // Overlay with neutral gray source (0.5) is identity on backdrop.
        assert!(close(BlendMode::Overlay.blend_channel(0.3, 0.5), 0.3));
        assert!(close(BlendMode::Overlay.blend_channel(0.7, 0.5), 0.7));
    }

    #[test]
    fn opaque_normal_source_wins() {
        let out = composite_over(
            (1.0, 0.0, 0.0, 1.0),
            (0.0, 0.0, 1.0, 1.0),
            BlendMode::Normal,
            1.0,
        );
        assert!(close(out.0, 0.0) && close(out.2, 1.0) && close(out.3, 1.0));
    }

    #[test]
    fn transparent_source_changes_nothing() {
        let bd = (0.2, 0.4, 0.6, 1.0);
        let out = composite_over(bd, (1.0, 1.0, 1.0, 0.0), BlendMode::Multiply, 1.0);
        assert_eq!(out, bd);
    }

    #[test]
    fn half_opacity_normal_is_midpoint_over_opaque() {
        let out = composite_over(
            (0.0, 0.0, 0.0, 1.0),
            (1.0, 1.0, 1.0, 1.0),
            BlendMode::Normal,
            0.5,
        );
        assert!(close(out.0, 0.5) && close(out.3, 1.0));
    }

    #[test]
    fn multiply_over_white_is_source() {
        let out = composite_over(
            (1.0, 1.0, 1.0, 1.0),
            (0.5, 0.25, 0.0, 1.0),
            BlendMode::Multiply,
            1.0,
        );
        assert!(close(out.0, 0.5) && close(out.1, 0.25) && close(out.2, 0.0));
    }

    #[test]
    fn ids_round_trip() {
        for id in 0..=16u8 {
            assert_eq!(BlendMode::from_id(id).unwrap().to_id(), id);
        }
        assert!(BlendMode::from_id(99).is_none());
    }

    #[test]
    fn phase45_mode_math() {
        // Hard Light with a mid-gray source is identity, like Overlay.
        assert!(close(BlendMode::HardLight.blend_channel(0.3, 0.5), 0.3));
        assert!(close(BlendMode::HardLight.blend_channel(0.7, 0.5), 0.7));
        // Color Dodge blows out to white as the source nears white.
        assert!(close(BlendMode::ColorDodge.blend_channel(0.2, 1.0), 1.0));
        assert!(close(BlendMode::ColorDodge.blend_channel(0.25, 0.5), 0.5));
        // Color Burn with a black source is black.
        assert!(close(BlendMode::ColorBurn.blend_channel(0.8, 0.0), 0.0));
        assert!(close(BlendMode::ColorBurn.blend_channel(0.5, 0.5), 0.0));
        // Exclusion: black source is identity, white inverts.
        assert!(close(BlendMode::Exclusion.blend_channel(0.3, 0.0), 0.3));
        assert!(close(BlendMode::Exclusion.blend_channel(0.3, 1.0), 0.7));
        // Dissolve hash is stable and in range.
        let h = dissolve_hash(17, 42, 3);
        assert!((0.0..1.0).contains(&h));
        assert_eq!(h, dissolve_hash(17, 42, 3));
        assert_ne!(h, dissolve_hash(18, 42, 3));
    }
}
