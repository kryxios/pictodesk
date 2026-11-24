//! Color mode conversion — Image > Mode in classic terms.
//!
//! Converting to CMYK rewrites the layers as real CMYK storage:
//! ink values in the tile slots plus a per-layer alpha plane (see
//! `ColorMode`). Converting back folds the inks and alpha back into
//! RGB pixels. Converting to Grayscale desaturates for real.

use crate::adjust;
use crate::color::{cmyk_to_rgb, rgb_to_cmyk, WorkingSpace};
use crate::document::{ColorMode, Document};
use crate::pixel::Pixel16;

/// Convert the document's RGB data from its current working
/// space to `to` (Image > Mode > Convert to Profile, Phase 5):
/// every painted pixel is re-encoded through XYZ, so the image
/// looks the same while its numbers change. CMYK documents keep
/// their inks — only the RGB interpretation space changes.
pub fn convert_working_space(doc: &mut Document, to: WorkingSpace) {
    let from = doc.working_space;
    if from == to {
        return;
    }
    if doc.mode != ColorMode::Cmyk {
        for li in 0..doc.layers.len() {
            if doc.layers[li].adjustment.is_some() {
                continue;
            }
            adjust::map_layer_pixels(doc, li, None, |p| {
                let (r, g, b, a) = p.to_f32s();
                let (nr, ng, nb) = from.convert_rgb((r, g, b), to);
                Pixel16::from_f32s(nr, ng, nb, a)
            });
        }
    }
    doc.working_space = to;
}

pub fn convert_mode(doc: &mut Document, to: ColorMode) {
    if doc.mode == to {
        return;
    }
    let from = doc.mode;
    if from == ColorMode::Cmyk && to == ColorMode::Grayscale {
        // Through RGB: restore real RGB pixels, then desaturate.
        cmyk_storage_to_rgb(doc);
        doc.mode = ColorMode::Rgb;
        for layer in 0..doc.layers.len() {
            adjust::desaturate(doc, layer, None);
        }
        doc.mode = ColorMode::Grayscale;
        return;
    }
    match to {
        ColorMode::Cmyk => rgb_storage_to_cmyk(doc),
        ColorMode::Grayscale => {
            for layer in 0..doc.layers.len() {
                adjust::desaturate(doc, layer, None);
            }
        }
        ColorMode::Rgb => {
            if from == ColorMode::Cmyk {
                cmyk_storage_to_rgb(doc);
            }
        }
    }
    doc.mode = to;
}

/// Rewrite every layer's RGB pixels as real CMYK ink values (tile
/// slots) + alpha plane entries.
fn rgb_storage_to_cmyk(doc: &mut Document) {
    for li in 0..doc.layers.len() {
        let offset = doc.layers[li].offset;
        let coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
        for coord in coords {
            let mut alphas: Vec<(u32, u32, u16)> = Vec::new();
            {
                let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
                for ly in 0..256u32 {
                    for lx in 0..256u32 {
                        let p = tile.get(lx, ly);
                        if p == Pixel16::TRANSPARENT {
                            continue;
                        }
                        let (r, g, b, _) = p.to_f32s();
                        let (c, m, y, k) = rgb_to_cmyk(r, g, b);
                        tile.set(lx, ly, Pixel16::from_f32s(c, m, y, k));
                        alphas.push((lx, ly, p.a));
                    }
                }
            }
            for (lx, ly, a) in alphas {
                doc.layers[li].set_alpha(
                    coord.0 * 256 + lx as i32 + offset.0,
                    coord.1 * 256 + ly as i32 + offset.1,
                    a,
                );
            }
        }
    }
}

/// Rewrite CMYK storage back to RGB pixels; clears the alpha planes
/// (alpha folds back into the pixel's alpha channel).
fn cmyk_storage_to_rgb(doc: &mut Document) {
    for li in 0..doc.layers.len() {
        let mut coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
        for c in doc.layers[li].cmyk_alpha.keys() {
            if !coords.contains(c) {
                coords.push(*c);
            }
        }
        for coord in coords {
            // Ensure the ink tile exists so writes have somewhere to go.
            doc.layers[li]
                .tiles
                .entry(coord)
                .or_insert_with(crate::tile::Tile::new_transparent);
            let alphas: Vec<u16> = match doc.layers[li].cmyk_alpha.get(&coord) {
                Some(t) => t.clone(),
                None => Vec::new(),
            };
            let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
            for ly in 0..256u32 {
                for lx in 0..256u32 {
                    let a = if alphas.is_empty() {
                        u16::MAX
                    } else {
                        alphas[(ly as usize) * 256 + lx as usize]
                    };
                    if a == 0 {
                        continue;
                    }
                    let p = tile.get(lx, ly);
                    let (r, g, b) = cmyk_to_rgb(
                        p.r as f32 / 65535.0,
                        p.g as f32 / 65535.0,
                        p.b as f32 / 65535.0,
                        p.a as f32 / 65535.0,
                    );
                    let out = Pixel16::from_f32s(r, g, b, a as f32 / 65535.0);
                    if out != p {
                        tile.set(lx, ly, out);
                    }
                }
            }
        }
        doc.layers[li].cmyk_alpha.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_conversion_stores_real_inks() {
        let mut doc = Document::new(4, 1);
        doc.layers[0].fill_rect(0, 0, 2, 1, Pixel16::from_rgba8(255, 0, 0, 255));
        doc.layers[0].fill_rect(2, 0, 2, 1, Pixel16::from_rgba8(120, 130, 140, 255));
        convert_mode(&mut doc, ColorMode::Cmyk);
        assert_eq!(doc.mode, ColorMode::Cmyk);
        // Pure red stores as C=0, M=100%, Y=100%, K=0, alpha opaque.
        let (c, m, y, k, a) = doc.get_cmyk(0, 0, 0);
        assert_eq!((c, k, a), (0, 0, u16::MAX));
        assert!(m > 65000 && y > 65000, "red inks: m={m} y={y}");
        // The screen render still shows red (ink composite -> RGB).
        assert_eq!(doc.render_pixel(0, 0).to_rgba8(), (255, 0, 0, 255));
        // CMYK -> RGB restores the original colors.
        convert_mode(&mut doc, ColorMode::Rgb);
        assert_eq!(doc.render_pixel(0, 0).to_rgba8(), (255, 0, 0, 255));
        let kept = doc.render_pixel(3, 0).to_rgba8();
        assert!((kept.0 as i32 - 120).abs() <= 2 && (kept.2 as i32 - 140).abs() <= 2);
        assert!(doc.layers[0].cmyk_alpha.is_empty());
    }

    #[test]
    fn cmyk_alpha_survives_conversion() {
        let mut doc = Document::new(2, 1);
        doc.layers[0].set_pixel(0, 0, Pixel16::from_rgba8(0, 0, 255, 128));
        convert_mode(&mut doc, ColorMode::Cmyk);
        let (_, _, _, _, a) = doc.get_cmyk(0, 0, 0);
        assert!((a as i32 - 128 * 257).abs() <= 1, "alpha {a}");
        convert_mode(&mut doc, ColorMode::Rgb);
        assert_eq!(doc.layers[0].get_pixel(0, 0).to_rgba8().3, 128);
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
