//! Phase 6.5 Batch 6 core checks: PCX/Targa, arbitrary
//! rotation, matting, Edit Stroke, brush masks, arrowheads.

use pictodesk_core::adjust;
use pictodesk_core::brush::{stamp_brush, BrushMask, BrushTip};
use pictodesk_core::document::Document;
use pictodesk_core::io::{self, ExportFormat};
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::selection::Selection;
use pictodesk_core::shapes::{shape_pixel, ShapeSpec};
use pictodesk_core::transform;

fn px(doc: &Document, x: i32, y: i32) -> (u8, u8, u8, u8) {
    doc.render_pixel(x, y).to_rgba8()
}

#[test]
fn pcx_round_trip() {
    let mut doc = Document::new(10, 6);
    doc.layers[0].set_pixel(2, 2, Pixel16::from_rgba8(200, 40, 30, 255));
    doc.layers[0].set_pixel(9, 5, Pixel16::from_rgba8(10, 220, 90, 255));
    let bytes = io::export_image(&doc, ExportFormat::Pcx, 90).unwrap();
    assert_eq!(bytes[0], 0x0A);
    let back = io::import_image(&bytes).unwrap();
    assert_eq!(px(&back, 2, 2), (200, 40, 30, 255));
    assert_eq!(px(&back, 9, 5), (10, 220, 90, 255));
}

#[test]
fn tga_export_has_header() {
    let doc = Document::new(8, 4);
    let bytes = io::export_image(&doc, ExportFormat::Tga, 90).unwrap();
    // TGA: image type 10 (RLE truecolor) or 2, width at 12.
    assert!(bytes.len() > 18);
    assert_eq!(u16::from_le_bytes([bytes[12], bytes[13]]), 8);
    let back = io::import_image(&bytes).unwrap();
    assert_eq!((back.width, back.height), (8, 4));
}

#[test]
fn rotate_arbitrary_expands_and_moves_content() {
    let mut doc = Document::new(20, 10);
    doc.layers[0].set_pixel(0, 0, Pixel16::from_rgba8(255, 0, 0, 255));
    let sel = Selection::all(20, 10);
    let out_sel = transform::rotate_arbitrary(&mut doc, 90.0, Some(&sel));
    // A 90-degree rotation swaps the dimensions exactly.
    assert_eq!((doc.width, doc.height), (10, 20));
    assert!(out_sel.is_some());
    // The red corner moved: (0,0) rotated 90 CW about the
    // center lands near the top-right.
    let mut found = false;
    for y in 0..20 {
        for x in 0..10 {
            if px(&doc, x, y) == (255, 0, 0, 255) {
                found = true;
                assert!(x >= 8 && y <= 2, "red at {x},{y}");
            }
        }
    }
    assert!(found, "red pixel survived rotation");
    // 45 degrees expands both dimensions.
    let mut doc2 = Document::new(20, 10);
    transform::rotate_arbitrary(&mut doc2, 45.0, None);
    assert!(doc2.width > 20 && doc2.height > 10);
}

#[test]
fn defringe_replaces_edge_color() {
    let mut doc = Document::new(6, 6);
    // Opaque green interior, semi-transparent red fringe.
    for y in 2..4 {
        for x in 2..4 {
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(0, 200, 0, 255));
        }
    }
    doc.layers[0].set_pixel(1, 2, Pixel16::from_rgba8(255, 0, 0, 128));
    adjust::defringe(&mut doc, 0, 1);
    let p = doc.layers[0].get_pixel(1, 2).to_rgba8();
    assert_eq!((p.0, p.1, p.2), (0, 200, 0), "fringe takes interior color");
    assert_eq!(p.3, 128, "alpha untouched");
}

#[test]
fn remove_black_matte_unmultiplies() {
    let mut doc = Document::new(4, 4);
    // fg (1.0, 0.5, 0.0) at 50% over black = (0.5, 0.25, 0).
    doc.layers[0].set_pixel(1, 1, Pixel16::from_rgba8(128, 64, 0, 128));
    adjust::remove_matte(&mut doc, 0, false);
    let p = doc.layers[0].get_pixel(1, 1).to_rgba8();
    assert!(p.0 >= 250 && (120..=135).contains(&p.1), "got {p:?}");
    assert_eq!(p.3, 128);
}

#[test]
fn stroke_selection_paints_band() {
    let mut doc = Document::new(12, 12);
    let mut sel = Selection::none(12, 12);
    for y in 3..9 {
        for x in 3..9 {
            sel.set(x, y, 255);
        }
    }
    // Inside stroke, width 2, opaque blue.
    adjust::stroke_selection(
        &mut doc,
        0,
        &sel,
        2,
        Pixel16::from_rgba8(0, 0, 255, 255),
        0,
        1.0,
    );
    assert_eq!(px(&doc, 3, 5), (0, 0, 255, 255), "edge row stroked");
    assert_eq!(px(&doc, 4, 5), (0, 0, 255, 255), "second row stroked");
    assert_eq!(px(&doc, 5, 5).3, 0, "interior untouched");
    assert_eq!(px(&doc, 2, 5).3, 0, "outside untouched for Inside");
}

#[test]
fn brush_mask_stamp_follows_mask() {
    let mut doc = Document::new(20, 20);
    // Mask: left half opaque, right half empty.
    let mut data = vec![0.0f32; 100];
    for y in 0..10 {
        for x in 0..5 {
            data[y * 10 + x] = 1.0;
        }
    }
    let tip = BrushTip::with_mask(10.0, BrushMask { w: 10, h: 10, data });
    stamp_brush(
        &mut doc,
        0,
        None,
        10.0,
        10.0,
        Pixel16::from_rgba8(255, 0, 0, 255),
        &tip,
        1.0,
    );
    assert_eq!(px(&doc, 7, 10).0, 255, "left half painted");
    assert_eq!(px(&doc, 13, 10).3, 0, "right half untouched");
}

#[test]
fn line_arrowhead_renders() {
    let mut spec = ShapeSpec {
        kind: 4,
        rect: (0, 0, 60, 0),
        stroke_width: 2,
        stroke: Pixel16::from_rgba8(0, 0, 0, 255),
        fill_on: false,
        ..ShapeSpec::default()
    };
    // Without arrowhead: off-axis near the end is empty.
    assert_eq!(shape_pixel(&spec, 59, 3).a, 0);
    spec.arrow_end = true;
    // The head widens the line near the tip: a point 3px
    // off-axis near the end is inside with the head.
    assert!(
        shape_pixel(&spec, 52, 1).a > 0,
        "arrowhead covers off-axis point"
    );
    assert_eq!(shape_pixel(&spec, 30, 3).a, 0, "shaft stays thin mid-way");
}
