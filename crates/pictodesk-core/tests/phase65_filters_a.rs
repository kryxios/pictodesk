//! Phase 6.5 Batch 1a core checks: Extrude, Trace Contour,
//! Color Halftone, Crystallize, Pointillize, Smart Blur,
//! De-Interlace, NTSC Colors, Sharpen More.

use pictodesk_core::document::Document;
use pictodesk_core::filter;
use pictodesk_core::pixel::Pixel16;

fn half_doc() -> Document {
    // Left half black, right half white, fully opaque.
    let mut doc = Document::new(24, 16);
    doc.layers[0].fill_rect(0, 0, 12, 16, Pixel16::from_rgba8(0, 0, 0, 255));
    doc.layers[0].fill_rect(12, 0, 12, 16, Pixel16::from_rgba8(255, 255, 255, 255));
    doc
}

fn layer_px(doc: &Document, x: i32, y: i32) -> (u8, u8, u8, u8) {
    doc.layers[0].get_pixel(x, y).to_rgba8()
}

#[test]
fn trace_contour_marks_only_the_boundary() {
    let mut doc = half_doc();
    filter::trace_contour(&mut doc, 0, None, 0.5, false);
    // Far from the boundary: white.
    assert_eq!(layer_px(&doc, 2, 8).0, 255);
    assert_eq!(layer_px(&doc, 20, 8).0, 255);
    // On the dark side of the boundary (Lower): black line.
    assert_eq!(layer_px(&doc, 11, 8).0, 0);
}

#[test]
fn deinterlace_duplication_copies_the_kept_field() {
    let mut doc = Document::new(8, 8);
    for y in 0..8 {
        let v = if y % 2 == 0 { 40 } else { 200 };
        doc.layers[0].fill_rect(0, y, 8, 1, Pixel16::from_rgba8(v, v, v, 255));
    }
    // Eliminate odd fields (rows 0,2,4,6 in 0-based parity):
    // each is rebuilt from the row below by duplication.
    filter::deinterlace(&mut doc, 0, None, false, false);
    assert_eq!(layer_px(&doc, 3, 0).0, 200);
    assert_eq!(layer_px(&doc, 3, 2).0, 200);
    assert_eq!(layer_px(&doc, 3, 1).0, 200);
}

#[test]
fn deinterlace_interpolation_averages_neighbours() {
    let mut doc = Document::new(8, 8);
    for y in 0..8 {
        let v = if y % 2 == 0 { 0 } else { 200 };
        doc.layers[0].fill_rect(0, y, 8, 1, Pixel16::from_rgba8(v, v, v, 255));
    }
    filter::deinterlace(&mut doc, 0, None, false, true);
    // Row 2 sits between two 200 rows: becomes 200.
    assert_eq!(layer_px(&doc, 3, 2).0, 200);
    // Row 0 has only a below neighbour in range-clamped
    // sampling: becomes the average of row 0 and row 1.
    assert_eq!(layer_px(&doc, 3, 0).0, 100);
}

#[test]
fn ntsc_colors_pulls_hot_orange_back_into_gamut() {
    // Fully saturated orange reconstructs slightly out of
    // range in YIQ, so its chroma is scaled down.
    let mut doc = Document::new(6, 6);
    doc.layers[0].fill_rect(0, 0, 6, 6, Pixel16::from_rgba8(255, 128, 0, 255));
    let before = layer_px(&doc, 3, 3);
    filter::ntsc_colors(&mut doc, 0, None);
    let after = layer_px(&doc, 3, 3);
    assert_ne!(before, after);
    assert!(after.0 < 255, "red pulled in: {after:?}");
    // An in-gamut gray is untouched.
    let mut doc2 = Document::new(4, 4);
    doc2.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(120, 120, 120, 255));
    filter::ntsc_colors(&mut doc2, 0, None);
    assert_eq!(layer_px(&doc2, 2, 2), (120, 120, 120, 255));
}

#[test]
fn smart_blur_edge_only_draws_the_edge() {
    let mut doc = half_doc();
    filter::smart_blur(&mut doc, 0, None, 2.0, 10.0, true);
    // Flat interior: black. Boundary column: white.
    assert_eq!(layer_px(&doc, 2, 8).0, 0);
    assert_eq!(layer_px(&doc, 11, 8).0, 255);
}

#[test]
fn smart_blur_normal_keeps_a_hard_edge() {
    let mut doc = half_doc();
    filter::smart_blur(&mut doc, 0, None, 3.0, 10.0, false);
    // Threshold keeps the two halves from mixing.
    assert_eq!(layer_px(&doc, 2, 8).0, 0);
    assert_eq!(layer_px(&doc, 20, 8).0, 255);
}

#[test]
fn gallery_filters_change_pixels_and_are_deterministic() {
    let run = |f: &dyn Fn(&mut Document)| -> u64 {
        let mut doc = half_doc();
        // Add mid-gray texture so cell filters have tone to eat.
        doc.layers[0].fill_rect(4, 4, 6, 6, Pixel16::from_rgba8(120, 90, 60, 255));
        let before = doc.render_checksum();
        f(&mut doc);
        let after = doc.render_checksum();
        assert_ne!(before, after, "filter must change the render");
        after
    };
    let extrude = |d: &mut Document| filter::extrude(d, 0, None, 6, 40, 0, 12345);
    assert_eq!(run(&extrude), run(&extrude));
    let halftone = |d: &mut Document| filter::color_halftone(d, 0, None, 6);
    assert_eq!(run(&halftone), run(&halftone));
    let cryst = |d: &mut Document| filter::crystallize(d, 0, None, 6, 12345);
    assert_eq!(run(&cryst), run(&cryst));
    let points = |d: &mut Document| {
        filter::pointillize(d, 0, None, 6, Pixel16::from_rgba8(0, 0, 0, 255), 12345)
    };
    assert_eq!(run(&points), run(&points));
    let sharp = |d: &mut Document| filter::sharpen_more(d, 0, None);
    assert_eq!(run(&sharp), run(&sharp));
}

#[test]
fn sharpen_more_leaves_flat_color_unchanged() {
    let mut doc = Document::new(10, 10);
    doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(90, 140, 200, 255));
    filter::sharpen_more(&mut doc, 0, None);
    assert_eq!(layer_px(&doc, 5, 5), (90, 140, 200, 255));
}
