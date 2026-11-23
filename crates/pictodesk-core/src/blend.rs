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
}

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
        }
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
        }
    }
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
    let blend = |b: f32, s: f32| mode.blend_channel(b, s);
    let (mr, mg, mb) = (blend(br, sr), blend(bg, sg), blend(bb, sb));
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
    let mut out = [0.0f32; 4];
    for i in 0..4 {
        let m = mode.blend_channel(b[i], s[i]);
        out[i] = (sa * (1.0 - ba) * s[i] + sa * ba * m + (1.0 - sa) * ba * b[i]) / out_a;
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
        for id in 0..=7u8 {
            assert_eq!(BlendMode::from_id(id).unwrap().to_id(), id);
        }
        assert!(BlendMode::from_id(99).is_none());
    }
}
