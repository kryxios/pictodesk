//! Color pipeline: working spaces, gamma, and space conversion via XYZ.
//!
//! Phase 0 scope: the three standard RGB working spaces with their
//! published primaries/white points and transfer functions, converted
//! through CIE XYZ with a 3x3 matrix core. The public API here is the
//! seam where a full ICC engine (LittleCMS) plugs in later without
//! touching the document/render code — see GAMEPLAN Phase 0 notes.

/// Minimal 3x3 matrix (row-major).
#[derive(Clone, Copy, Debug)]
pub struct Mat3 {
    pub m: [[f32; 3]; 3],
}

impl Mat3 {
    pub fn mul_vec(&self, v: (f32, f32, f32)) -> (f32, f32, f32) {
        (
            self.m[0][0] * v.0 + self.m[0][1] * v.1 + self.m[0][2] * v.2,
            self.m[1][0] * v.0 + self.m[1][1] * v.1 + self.m[1][2] * v.2,
            self.m[2][0] * v.0 + self.m[2][1] * v.1 + self.m[2][2] * v.2,
        )
    }

    pub fn inverse(&self) -> Mat3 {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.m;
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        assert!(det.abs() > 1e-12, "singular matrix");
        let s = 1.0 / det;
        Mat3 {
            m: [
                [
                    (e * i - f * h) * s,
                    (c * h - b * i) * s,
                    (b * f - c * e) * s,
                ],
                [
                    (f * g - d * i) * s,
                    (a * i - c * g) * s,
                    (c * d - a * f) * s,
                ],
                [
                    (d * h - e * g) * s,
                    (b * g - a * h) * s,
                    (a * e - b * d) * s,
                ],
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkingSpace {
    Srgb,
    AdobeRgb,
    ProPhoto,
}

impl WorkingSpace {
    /// Published linear-RGB -> CIE XYZ matrix for this space.
    #[allow(clippy::excessive_precision)] // constants are published reference values; keep full precision
    pub fn to_xyz_matrix(&self) -> Mat3 {
        match self {
            // sRGB, D65
            WorkingSpace::Srgb => Mat3 {
                m: [
                    [0.4123908, 0.3575843, 0.1804808],
                    [0.2126390, 0.7151687, 0.0721923],
                    [0.0193308, 0.1191948, 0.9505322],
                ],
            },
            // Adobe RGB (1998), D50-adapted published matrix
            WorkingSpace::AdobeRgb => Mat3 {
                m: [
                    [0.57667, 0.18556, 0.18823],
                    [0.29734, 0.62736, 0.07529],
                    [0.02703, 0.07069, 0.99134],
                ],
            },
            // ProPhoto / ROMM, D50
            WorkingSpace::ProPhoto => Mat3 {
                m: [
                    [0.7976749, 0.1351917, 0.0313534],
                    [0.2880401, 0.7118741, 0.0000857],
                    [0.0, 0.0, 0.8251651],
                ],
            },
        }
    }

    /// Encoded (gamma) -> linear for one channel.
    #[allow(clippy::excessive_precision)] // 2.19921875 is Adobe RGB's exact 563/256 gamma
    pub fn to_linear(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        match self {
            WorkingSpace::Srgb => {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            }
            WorkingSpace::AdobeRgb => v.powf(2.19921875),
            WorkingSpace::ProPhoto => {
                if v < 0.03125 {
                    v / 16.0
                } else {
                    v.powf(1.8)
                }
            }
        }
    }

    /// Linear -> encoded (gamma) for one channel.
    #[allow(clippy::excessive_precision)]
    pub fn from_linear(&self, v: f32) -> f32 {
        let v = v.max(0.0);
        match self {
            WorkingSpace::Srgb => {
                if v <= 0.0031308 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                }
            }
            WorkingSpace::AdobeRgb => v.powf(1.0 / 2.19921875),
            WorkingSpace::ProPhoto => {
                if v < 0.001953 {
                    v * 16.0
                } else {
                    v.powf(1.0 / 1.8)
                }
            }
        }
    }

    /// Convert an encoded RGB triple from this space to `other`.
    /// Out-of-gamut results are clamped — Phase 0 has no gamut mapping;
    /// that is a deliberate, documented limitation, not a silent one.
    pub fn convert_rgb(&self, rgb: (f32, f32, f32), other: WorkingSpace) -> (f32, f32, f32) {
        if *self == other {
            return rgb;
        }
        let lin = (
            self.to_linear(rgb.0),
            self.to_linear(rgb.1),
            self.to_linear(rgb.2),
        );
        let xyz = self.to_xyz_matrix().mul_vec(lin);
        let lin2 = other.to_xyz_matrix().inverse().mul_vec(xyz);
        (
            other.from_linear(lin2.0).clamp(0.0, 1.0),
            other.from_linear(lin2.1).clamp(0.0, 1.0),
            other.from_linear(lin2.2).clamp(0.0, 1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn gamma_round_trips() {
        for space in [
            WorkingSpace::Srgb,
            WorkingSpace::AdobeRgb,
            WorkingSpace::ProPhoto,
        ] {
            for v in [0.0f32, 0.01, 0.18, 0.5, 0.9, 1.0] {
                let rt = space.from_linear(space.to_linear(v));
                assert!(
                    close(rt, v, 0.002),
                    "{space:?} gamma round trip failed at {v}: {rt}"
                );
            }
        }
    }

    #[test]
    fn srgb_white_maps_to_adobe_white() {
        // D65 white converted into Adobe space should stay near-neutral.
        let (r, g, b) = WorkingSpace::Srgb.convert_rgb((1.0, 1.0, 1.0), WorkingSpace::AdobeRgb);
        assert!(
            close(r, g, 0.03) && close(g, b, 0.03),
            "white drifted: {r} {g} {b}"
        );
        assert!(r > 0.9, "white dimmed: {r}");
    }

    #[test]
    fn srgb_mid_gray_survives_round_trip() {
        let there = WorkingSpace::Srgb.convert_rgb((0.5, 0.4, 0.3), WorkingSpace::ProPhoto);
        let back = WorkingSpace::ProPhoto.convert_rgb(there, WorkingSpace::Srgb);
        assert!(
            close(back.0, 0.5, 0.01) && close(back.1, 0.4, 0.01) && close(back.2, 0.3, 0.01),
            "round trip drifted: {back:?}"
        );
    }

    #[test]
    fn matrix_inverse_is_real() {
        let m = WorkingSpace::Srgb.to_xyz_matrix();
        let v = m.mul_vec((0.2, 0.3, 0.4));
        let back = m.inverse().mul_vec(v);
        assert!(close(back.0, 0.2, 1e-5) && close(back.1, 0.3, 1e-5) && close(back.2, 0.4, 1e-5));
    }
}

/// RGB -> CMYK (each component 0.0..=1.0), simple undercolor removal:
/// K is the full common black, CMY are what remains — the classic
/// conversion every early-90s editor shipped.
pub fn rgb_to_cmyk(r: f32, g: f32, b: f32) -> (f32, f32, f32, f32) {
    let k = 1.0 - r.max(g).max(b);
    if k >= 0.99999 {
        return (0.0, 0.0, 0.0, 1.0);
    }
    let d = 1.0 - k;
    (
        (1.0 - r - k) / d,
        (1.0 - g - k) / d,
        (1.0 - b - k) / d,
        k,
    )
}

/// CMYK -> RGB, the exact inverse of the forward formulas.
pub fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> (f32, f32, f32) {
    (
        (1.0 - c) * (1.0 - k),
        (1.0 - m) * (1.0 - k),
        (1.0 - y) * (1.0 - k),
    )
}
