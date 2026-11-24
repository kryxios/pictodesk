//! Phase 6.5 Batch 2 core checks: Lab, Bitmap, Indexed,
//! Duotone, Multichannel.

use pictodesk_core::color::{lab_to_rgb, rgb_to_lab};
use pictodesk_core::document::{ColorMode, Document, DuotoneInk, DuotoneSpec};
use pictodesk_core::mode::{convert_mode, to_bitmap, to_indexed, to_multichannel};
use pictodesk_core::pixel::Pixel16;

fn colorful_doc() -> Document {
    let mut doc = Document::new(16, 16);
    doc.layers[0].fill_rect(0, 0, 8, 16, Pixel16::from_rgba8(200, 40, 30, 255));
    doc.layers[0].fill_rect(8, 0, 8, 8, Pixel16::from_rgba8(30, 160, 60, 255));
    doc.layers[0].fill_rect(8, 8, 8, 8, Pixel16::from_rgba8(40, 60, 200, 255));
    doc
}

fn close(a: u8, b: u8, tol: u8) -> bool {
    (a as i32 - b as i32).abs() <= tol as i32
}

#[test]
fn lab_math_matches_reference_values() {
    // Reference: published CIE Lab (D50) for sRGB red —
    // (54.29, 80.81, 69.89). (The often-quoted 53.24/80.09/
    // 67.20 triple is the D65 value; the classic editor's
    // Lab is D50.)
    let (l, a, b) = rgb_to_lab(1.0, 0.0, 0.0);
    assert!((l - 54.29).abs() < 1.0, "red L {l}");
    assert!((a - 80.81).abs() < 1.5, "red a {a}");
    assert!((b - 69.89).abs() < 1.5, "red b {b}");
    let (l, a, b) = rgb_to_lab(1.0, 1.0, 1.0);
    assert!((l - 100.0).abs() < 0.5 && a.abs() < 1.0 && b.abs() < 1.0);
    // Round trip across the gamut corners.
    for (r, g, b) in [
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.0, 1.0),
        (0.25, 0.5, 0.75),
        (0.9, 0.8, 0.1),
    ] {
        let (l, a, bb) = rgb_to_lab(r, g, b);
        let (r2, g2, b2) = lab_to_rgb(l, a, bb);
        assert!(
            (r - r2).abs() < 0.01 && (g - g2).abs() < 0.01 && (b - b2).abs() < 0.01,
            "round trip ({r},{g},{b}) -> ({r2},{g2},{b2})"
        );
    }
}

#[test]
fn lab_mode_stores_lab_and_renders_the_same() {
    let mut doc = colorful_doc();
    let before = doc.render_pixel(2, 2).to_rgba8();
    convert_mode(&mut doc, ColorMode::Lab);
    assert_eq!(doc.mode, ColorMode::Lab);
    // Slots hold Lab: the doc's red (200,40,30) has L = 44.9.
    let (l, _a, _b, _al) = doc.get_lab(0, 2, 2);
    let l100 = l as f32 / 65535.0 * 100.0;
    assert!((l100 - 44.9).abs() < 3.0, "stored L {l100}");
    let after = doc.render_pixel(2, 2).to_rgba8();
    assert!(
        close(before.0, after.0, 4) && close(before.1, after.1, 4) && close(before.2, after.2, 4),
        "render {before:?} -> {after:?}"
    );
    convert_mode(&mut doc, ColorMode::Rgb);
    let back = doc.render_pixel(2, 2).to_rgba8();
    assert!(close(before.0, back.0, 4) && close(before.2, back.2, 4));
}

#[test]
fn bitmap_threshold_and_diffusion() {
    let mut doc = Document::new(16, 4);
    for x in 0..16 {
        let v = (x * 17) as u8;
        doc.layers[0].fill_rect(x, 0, 1, 4, Pixel16::from_rgba8(v, v, v, 255));
    }
    convert_mode(&mut doc, ColorMode::Grayscale);
    to_bitmap(&mut doc, 0, 0.5);
    assert_eq!(doc.mode, ColorMode::Bitmap);
    assert_eq!(doc.render_pixel(2, 0).to_rgba8().0, 0);
    assert_eq!(doc.render_pixel(14, 0).to_rgba8().0, 255);
    // Diffusion keeps the average roughly honest.
    let mut doc2 = Document::new(16, 4);
    doc2.layers[0].fill_rect(0, 0, 16, 4, Pixel16::from_rgba8(128, 128, 128, 255));
    convert_mode(&mut doc2, ColorMode::Grayscale);
    to_bitmap(&mut doc2, 2, 0.5);
    let whites = (0..16)
        .filter(|&x| doc2.render_pixel(x, 1).to_rgba8().0 == 255)
        .count();
    assert!((6..=10).contains(&whites), "diffusion whites {whites}/16");
}

#[test]
fn indexed_adaptive_and_web() {
    let mut doc = colorful_doc();
    to_indexed(&mut doc, 0, 256, false);
    assert_eq!(doc.mode, ColorMode::Indexed);
    assert!(doc.color_table.len() <= 256 && doc.color_table.len() >= 3);
    // Every pixel is a palette color now.
    for y in 0..16 {
        for x in 0..16 {
            let p = doc.layers[0].get_pixel(x, y).to_rgba8();
            assert!(
                doc.color_table.contains(&(p.0, p.1, p.2)),
                "pixel {p:?} not in palette"
            );
        }
    }
    // The three flat colors survive nearly exactly.
    let p = doc.render_pixel(2, 2).to_rgba8();
    assert!(close(p.0, 200, 12) && close(p.1, 40, 12));
    let mut doc2 = colorful_doc();
    to_indexed(&mut doc2, 1, 256, false);
    assert_eq!(doc2.color_table.len(), 216);
}

#[test]
fn duotone_default_is_grayscale_and_ink_tints() {
    let mut doc = Document::new(8, 2);
    doc.layers[0].fill_rect(0, 0, 4, 2, Pixel16::from_rgba8(40, 40, 40, 255));
    doc.layers[0].fill_rect(4, 0, 4, 2, Pixel16::from_rgba8(230, 230, 230, 255));
    convert_mode(&mut doc, ColorMode::Duotone);
    assert_eq!(doc.mode, ColorMode::Duotone);
    // Default: one black ink, linear — renders like grayscale.
    let dark = doc.render_pixel(1, 0).to_rgba8();
    assert!(close(dark.0, 40, 6) && dark.0 == dark.1 && dark.1 == dark.2);
    // A red ink monotone: shadows go red, paper stays white.
    doc.duotone = Some(DuotoneSpec {
        inks: vec![DuotoneInk::new((200, 30, 20))],
    });
    let dark = doc.render_pixel(1, 0).to_rgba8();
    assert!(dark.0 > dark.1 + 60, "red ink shadow: {dark:?}");
    let light = doc.render_pixel(6, 0).to_rgba8();
    assert!(light.0 > 200 && light.1 > 200, "paper: {light:?}");
}

#[test]
fn multichannel_round_trip_through_cmyk() {
    let mut doc = colorful_doc();
    convert_mode(&mut doc, ColorMode::Cmyk);
    let before = doc.render_pixel(2, 2).to_rgba8();
    let ink_before = doc.get_cmyk(0, 2, 2);
    to_multichannel(&mut doc);
    assert_eq!(doc.mode, ColorMode::Multichannel);
    assert_eq!(doc.multi_planes.len(), 4);
    assert_eq!(doc.multi_channels.len(), 4);
    // Plane values match the CMYK separation.
    let i = 2 * 16 + 2;
    assert_eq!(doc.multi_planes[1][i], ink_before.1);
    let rendered = doc.render_pixel(2, 2).to_rgba8();
    assert!(
        close(before.0, rendered.0, 6) && close(before.1, rendered.1, 6),
        "multi render {before:?} -> {rendered:?}"
    );
    convert_mode(&mut doc, ColorMode::Rgb);
    assert_eq!(doc.mode, ColorMode::Rgb);
    assert!(doc.multi_planes.is_empty());
    let back = doc.render_pixel(2, 2).to_rgba8();
    assert!(close(before.0, back.0, 8) && close(before.1, back.1, 8));
}

#[test]
fn mode_data_survives_format_round_trip() {
    let mut doc = colorful_doc();
    to_indexed(&mut doc, 0, 16, false);
    let bytes = pictodesk_core::format::serialize(&doc);
    let back = pictodesk_core::format::deserialize(&bytes).unwrap();
    assert_eq!(back.mode, ColorMode::Indexed);
    assert_eq!(back.color_table, doc.color_table);

    let mut doc = Document::new(8, 2);
    doc.layers[0].fill_rect(0, 0, 8, 2, Pixel16::from_rgba8(90, 90, 90, 255));
    convert_mode(&mut doc, ColorMode::Duotone);
    doc.duotone = Some(DuotoneSpec {
        inks: vec![
            DuotoneInk::new((10, 20, 30)),
            DuotoneInk::new((200, 100, 50)),
        ],
    });
    let bytes = pictodesk_core::format::serialize(&doc);
    let back = pictodesk_core::format::deserialize(&bytes).unwrap();
    assert_eq!(back.mode, ColorMode::Duotone);
    assert_eq!(back.duotone.as_ref().unwrap().inks.len(), 2);
    assert_eq!(back.duotone.as_ref().unwrap().inks[1].color, (200, 100, 50));
}
