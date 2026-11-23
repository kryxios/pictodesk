//! Phase 0 exit criteria, as integration tests:
//! a document renders through the tile engine + color pipeline,
//! history restores it, and the file format round-trips it losslessly.

use pictodesk_core::blend::BlendMode;
use pictodesk_core::color::WorkingSpace;
use pictodesk_core::document::Document;
use pictodesk_core::format;
use pictodesk_core::history::{History, TileEditCommand};
use pictodesk_core::pixel::Pixel16;

#[test]
fn phase0_exit_criteria() {
    // A document larger than one tile, painted on two layers.
    let mut doc = Document::new(700, 500);
    doc.layers[0].fill_rect(0, 0, 700, 500, Pixel16::from_rgba8(40, 80, 120, 255));
    let top = doc.add_layer("Overlay");
    doc.layers[top].blend_mode = BlendMode::Overlay;
    doc.layers[top].opacity = 0.8;
    doc.layers[top].fill_rect(100, 100, 300, 200, Pixel16::from_rgba8(220, 180, 60, 255));

    // Render goes through tiles: pixel inside and outside the overlay differ.
    let inside = doc.render_pixel(200, 200);
    let outside = doc.render_pixel(600, 450);
    assert_ne!(inside, outside);
    assert_eq!(outside.to_rgba8(), (40, 80, 120, 255));

    // History: a captured tile edit undoes and redoes exactly.
    let mut hist = History::new();
    let before = doc.render_checksum();
    let mut cmd = TileEditCommand::new("Spot");
    cmd.capture_before(&doc, 0, (0, 0));
    doc.layers[0].set_pixel(5, 5, Pixel16::from_rgba8(255, 255, 255, 255));
    cmd.capture_after(&doc, 0, (0, 0));
    hist.push(Box::new(cmd));
    assert_ne!(doc.render_checksum(), before);
    hist.undo(&mut doc);
    assert_eq!(doc.render_checksum(), before);
    hist.redo(&mut doc);
    assert_ne!(doc.render_checksum(), before);
    hist.undo(&mut doc);

    // Color: converting the document's working space is a data change
    // the model carries, and sRGB is the default.
    assert_eq!(doc.working_space, WorkingSpace::Srgb);
    let gray = WorkingSpace::Srgb.convert_rgb((0.5, 0.5, 0.5), WorkingSpace::AdobeRgb);
    assert!((gray.0 - gray.1).abs() < 0.01);

    // Format: serialize/deserialize is lossless, render included.
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).unwrap();
    assert_eq!(doc, back);
    assert_eq!(doc.render_checksum(), back.render_checksum());
}
