//! Pixel type: RGBA, 16 bits per channel — the internal pipeline depth.
//! Photoshop 1.0 was 8-bit; we build 16-bit from day one (Phase 0 rule:
//! features in Adobe's order, engine modern underneath).

/// One pixel: red, green, blue, alpha — each 0..=65535, alpha is straight
/// (not premultiplied) at rest; compositing premultiplies on the fly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pixel16 {
    pub r: u16,
    pub g: u16,
    pub b: u16,
    pub a: u16,
}

impl Pixel16 {
    pub const TRANSPARENT: Pixel16 = Pixel16 {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const MAX: f32 = 65535.0;

    pub fn new(r: u16, g: u16, b: u16, a: u16) -> Self {
        Pixel16 { r, g, b, a }
    }

    /// 8-bit -> 16-bit using the standard *257 expansion (0->0, 255->65535).
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Pixel16 {
            r: (r as u16) * 257,
            g: (g as u16) * 257,
            b: (b as u16) * 257,
            a: (a as u16) * 257,
        }
    }

    /// 16-bit -> 8-bit with rounding (divide by 257 is exact for values
    /// produced by from_rgba8).
    pub fn to_rgba8(self) -> (u8, u8, u8, u8) {
        fn d(v: u16) -> u8 {
            ((v as u32 + 128) / 257) as u8
        }
        (d(self.r), d(self.g), d(self.b), d(self.a))
    }

    /// Channels as f32 in 0.0..=1.0 (encoded / display-referred values).
    pub fn to_f32s(self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / Self::MAX,
            self.g as f32 / Self::MAX,
            self.b as f32 / Self::MAX,
            self.a as f32 / Self::MAX,
        )
    }

    pub fn from_f32s(r: f32, g: f32, b: f32, a: f32) -> Self {
        fn e(v: f32) -> u16 {
            (v.clamp(0.0, 1.0) * Pixel16::MAX + 0.5) as u16
        }
        Pixel16 {
            r: e(r),
            g: e(g),
            b: e(b),
            a: e(a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba8_round_trip_is_exact() {
        for v in [0u8, 1, 17, 128, 254, 255] {
            let p = Pixel16::from_rgba8(v, v, v, v);
            assert_eq!(p.to_rgba8(), (v, v, v, v));
        }
    }

    #[test]
    fn f32_round_trip_within_one_step() {
        let p = Pixel16::new(1000, 32000, 65535, 40000);
        let (r, g, b, a) = p.to_f32s();
        let q = Pixel16::from_f32s(r, g, b, a);
        assert!((p.r as i32 - q.r as i32).abs() <= 1);
        assert!((p.g as i32 - q.g as i32).abs() <= 1);
        assert!((p.b as i32 - q.b as i32).abs() <= 1);
        assert!((p.a as i32 - q.a as i32).abs() <= 1);
    }
}
