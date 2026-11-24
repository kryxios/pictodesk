//! Phase 6.5 Batch 4 core checks: Selective Color and
//! Replace Color.

use pictodesk_core::adjust::{replace_color, selective_color};
use pictodesk_core::document::Document;
use pictodesk_core::pixel::Pixel16;

#[test]
fn selective_color_reds_only() {
    let mut doc = Document::new(8, 2);
    doc.layers[0].fill_rect(0, 0, 4, 2, Pixel16::from_rgba8(200, 40, 30, 255));
    doc.layers[0].fill_rect(4, 0, 4, 2, Pixel16::from_rgba8(30, 160, 60, 255));
    // Reds: add cyan ink (+C pushes red down toward cyan).
    selective_color(&mut doc, 0, None, 0, [1.0, 0.0, 0.0, 0.0], false);
    let red = doc.layers[0].get_pixel(1, 1).to_rgba8();
    let green = doc.layers[0].get_pixel(6, 1).to_rgba8();
    assert!(red.0 < 100, "red pixel's red channel drops: {red:?}");
    assert_eq!(green, (30, 160, 60, 255), "green pixel untouched");
}

#[test]
fn selective_color_neutrals_black_slider() {
    let mut doc = Document::new(4, 2);
    doc.layers[0].fill_rect(0, 0, 4, 2, Pixel16::from_rgba8(120, 120, 120, 255));
    // Neutrals: -K lightens the gray.
    selective_color(&mut doc, 0, None, 7, [0.0, 0.0, 0.0, -0.5], false);
    let p = doc.layers[0].get_pixel(1, 1).to_rgba8();
    assert!(p.0 > 160, "gray lightens when K drops: {p:?}");
}

#[test]
fn replace_color_shifts_matching_only() {
    let mut doc = Document::new(8, 2);
    doc.layers[0].fill_rect(0, 0, 4, 2, Pixel16::from_rgba8(200, 40, 30, 255));
    doc.layers[0].fill_rect(4, 0, 4, 2, Pixel16::from_rgba8(30, 40, 200, 255));
    // Target the red, rotate hue +120 (toward green).
    replace_color(
        &mut doc,
        0,
        None,
        (200.0 / 255.0, 40.0 / 255.0, 30.0 / 255.0),
        60.0,
        120.0,
        0.0,
        0.0,
    );
    let red = doc.layers[0].get_pixel(1, 1).to_rgba8();
    let blue = doc.layers[0].get_pixel(6, 1).to_rgba8();
    assert!(red.1 > red.0, "red rotated toward green: {red:?}");
    assert_eq!(blue, (30, 40, 200, 255), "blue untouched");
}
