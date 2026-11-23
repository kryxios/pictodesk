//! Phase 1 exit test — the game plan's task test, end to end:
//! "take a scanned/dull photo → correct it with Levels, clone out a
//! dust spot, crop, resize, export a clean JPEG." Plus the history
//! contract: adjustments undo exactly, and Photoshop 1.0's single
//! level of undo behaves like 1.0 (toggle Undo/Redo).

use pictodesk_core::adjust;
use pictodesk_core::brush::{stroke_clone, BrushTip};
use pictodesk_core::history::{run_tile_edit, DocumentCommand};
use pictodesk_core::io::{export_image, import_image, ExportFormat};
use pictodesk_core::selection::Selection;
use pictodesk_core::transform;
use pictodesk_core::{Document, History, Pixel16};

/// A dull "scan": compressed tonal range (60..170), sky gradient on
/// top, darker ground below, and a near-black dust spot in the sky.
fn dull_photo_png() -> Vec<u8> {
    let mut doc = Document::new(200, 150);
    for y in 0..150 {
        for x in 0..200 {
            let v = if y < 90 {
                // dull sky: 120..170
                120 + ((90 - y) as f32 / 90.0 * 50.0) as u8
            } else {
                // dull ground: 60..90
                60 + ((y - 90) as f32 / 60.0 * 30.0) as u8
            };
            let _ = x;
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v + 5, 255));
        }
    }
    // dust spot at (100, 30), 5x5 near-black
    doc.layers[0].fill_rect(98, 28, 5, 5, Pixel16::from_rgba8(8, 8, 8, 255));
    export_image(&doc, ExportFormat::Png, 90).unwrap()
}

#[test]
fn phase1_task_test() {
    // Open the "scan"
    let mut doc = import_image(&dull_photo_png()).unwrap();
    assert_eq!((doc.width, doc.height), (200, 150));
    let mut hist = History::new();

    // Sanity: it really is dull (max channel value <= ~175)
    let before_max = (0..150)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .map(|(x, y)| doc.render_pixel(x, y).to_rgba8().0)
        .max()
        .unwrap();
    assert!(
        before_max <= 176,
        "photo should start dull, max {before_max}"
    );

    // 1) Levels: stretch the photo's actual range (55..170, read off
    // the histogram like a user would) to full range, one undoable edit
    let tiles = Selection::tiles_in_rect(0, 0, 200, 150);
    run_tile_edit(&mut doc, &mut hist, "Levels", 0, &tiles, |d| {
        adjust::levels(d, 0, None, 55, 170, 1.0, 0, 255);
    });
    let after_max = (0..150)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .map(|(x, y)| doc.render_pixel(x, y).to_rgba8().0)
        .max()
        .unwrap();
    assert!(
        after_max >= 250,
        "levels should stretch to near-white, max {after_max}"
    );

    // 2) Clone out the dust spot: source = clean sky 20px left
    let tip = BrushTip::new(9.0, 1.0);
    let bbox = stroke_clone(&mut doc, 0, None, &[(100.0, 30.0)], (80.0, 30.0), &tip, 1.0);
    let tiles = Selection::tiles_in_rect(bbox.0, bbox.1, bbox.2, bbox.3);
    let _ = tiles;
    let dust = doc.render_pixel(100, 30).to_rgba8().0;
    let sky = doc.render_pixel(80, 30).to_rgba8().0;
    assert!(
        (dust as i32 - sky as i32).abs() <= 6,
        "dust spot cloned away: dust {dust} vs sky {sky}"
    );

    // 3) Crop + 4) Resize (structural edits, DocumentCommand undo)
    let mut cmd = DocumentCommand::begin("Crop", &doc);
    transform::crop(&mut doc, 20, 10, 160, 120);
    cmd.finish(&doc);
    hist.push(Box::new(cmd));
    assert_eq!((doc.width, doc.height), (160, 120));

    let mut cmd = DocumentCommand::begin("Image Size", &doc);
    transform::resize_image(&mut doc, 80, 60);
    cmd.finish(&doc);
    hist.push(Box::new(cmd));
    assert_eq!((doc.width, doc.height), (80, 60));

    // 5) Export a clean JPEG
    let jpeg = export_image(&doc, ExportFormat::Jpeg, 92).unwrap();
    assert_eq!(&jpeg[0..2], &[0xFF, 0xD8], "JPEG magic bytes");
    let back = import_image(&jpeg).unwrap();
    assert_eq!((back.width, back.height), (80, 60));
}

#[test]
fn adjustment_undo_is_exact_and_single_level_toggles() {
    let mut doc = Document::new(32, 32);
    doc.layers[0].fill_rect(0, 0, 32, 32, Pixel16::from_rgba8(90, 90, 90, 255));
    let original = doc.render_checksum();

    // Photoshop 1.0 mode: exactly one level of undo.
    let mut hist = History::new();
    hist.set_limit(Some(1));
    let tiles = Selection::tiles_in_rect(0, 0, 32, 32);
    run_tile_edit(&mut doc, &mut hist, "Invert", 0, &tiles, |d| {
        adjust::invert(d, 0, None);
    });
    run_tile_edit(&mut doc, &mut hist, "Invert", 0, &tiles, |d| {
        adjust::invert(d, 0, None);
    });
    // Two inverts = back to original pixels, but history holds only
    // the LAST command (limit 1): one Undo inverts once more (not
    // back to the pre-first-invert state via two undos).
    assert_eq!(doc.render_checksum(), original);
    assert_eq!(hist.undo_depth(), 1);
    assert_eq!(hist.undo(&mut doc).as_deref(), Some("Invert"));
    assert_ne!(doc.render_checksum(), original);
    // Redo toggles back — the 1.0 Undo/Redo dance.
    assert_eq!(hist.redo(&mut doc).as_deref(), Some("Invert"));
    assert_eq!(doc.render_checksum(), original);
    // And a fresh single edit undoes exactly:
    run_tile_edit(&mut doc, &mut hist, "Invert", 0, &tiles, |d| {
        adjust::invert(d, 0, None);
    });
    hist.undo(&mut doc);
    assert_eq!(doc.render_checksum(), original);
}
