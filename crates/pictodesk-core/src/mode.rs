//! Color mode conversion — Image > Mode in classic terms.
//!
//! Pixel storage is always 16-bit RGB (see `ColorMode`). Converting
//! to CMYK runs every painted pixel through RGB -> CMYK -> RGB, so
//! what remains is exactly the printable gamut: the screen becomes a
//! soft proof of the print. Converting to Grayscale desaturates for
//! real. Converting back to RGB only changes the flag — the stored
//! values are already RGB-representable, which is also why a
//! CMYK -> RGB -> CMYK round trip is stable.

use crate::adjust;
use crate::color::{cmyk_to_rgb, rgb_to_cmyk};
use crate::document::{ColorMode, Document};
use crate::pixel::Pixel16;

pub fn convert_mode(doc: &mut Document, to: ColorMode) {
    if doc.mode == to {
        return;
    }
    match (doc.mode, to) {
        (_, ColorMode::Cmyk) => {
            // RGB (or Gray) -> CMYK: gamut-clip through the round trip.
            for layer in 0..doc.layers.len() {
                adjust::map_layer_pixels(doc, layer, None, |p| {
                    let (r, g, b, a) = p.to_f32s();
                    let (c, m, y, k) = rgb_to_cmyk(r, g, b);
                    let (r2, g2, b2) = cmyk_to_rgb(c, m, y, k);
                    Pixel16::from_f32s(r2, g2, b2, a)
                });
            }
        }
        (_, ColorMode::Grayscale) => {
            for layer in 0..doc.layers.len() {
                adjust::desaturate(doc, layer, None);
            }
        }
        (_, ColorMode::Rgb) => {
            // Stored pixels are already RGB-representable.
        }
    }
    doc.mode = to;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_round_trip_clips_to_printable_and_is_stable() {
        let mut doc = Document::new(4, 1);
        // Pure sRGB green is far outside the CMYK gamut.
        doc.layers[0].fill_rect(0, 0, 2, 1, Pixel16::from_rgba8(0, 255, 0, 255));
        doc.layers[0].fill_rect(2, 0, 2, 1, Pixel16::from_rgba8(120, 130, 140, 255));
        convert_mode(&mut doc, ColorMode::Cmyk);
        assert_eq!(doc.mode, ColorMode::Cmyk);
        let clipped = doc.render_pixel(0, 0);
        // After the round trip the pixel must be exactly representable:
        // converting RGB->CMYK again gives the same CMYK values.
        let (r, g, b, _) = clipped.to_f32s();
        let (c, m, y, k) = rgb_to_cmyk(r, g, b);
        let back = cmyk_to_rgb(c, m, y, k);
        assert!((back.0 - r).abs() < 0.002 && (back.1 - g).abs() < 0.002 && (back.2 - b).abs() < 0.002);
        // An in-gamut gray-blue survives almost unchanged.
        let kept = doc.render_pixel(3, 0).to_rgba8();
        assert!((kept.0 as i32 - 120).abs() <= 2 && (kept.2 as i32 - 140).abs() <= 2);
        // CMYK -> RGB -> CMYK is a no-op the second time.
        let before = doc.render_checksum();
        convert_mode(&mut doc, ColorMode::Rgb);
        convert_mode(&mut doc, ColorMode::Cmyk);
        assert_eq!(doc.render_checksum(), before);
    }

    #[test]
    fn grayscale_mode_desaturates() {
        let mut doc = Document::new(2, 1);
        doc.layers[0].fill_rect(0, 0, 2, 1, Pixel16::from_rgba8(200, 40, 40, 255));
        convert_mode(&mut doc, ColorMode::Grayscale);
        let p = doc.render_pixel(0, 0).to_rgba8();
        assert_eq!(p.0, p.1);
        assert_eq!(p.1, p.2);
    }
}
