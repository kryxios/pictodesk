//! Phase 6.5 Batch 1d core checks: the Brush Strokes gallery.

use pictodesk_core::document::Document;
use pictodesk_core::filter;
use pictodesk_core::pixel::Pixel16;

fn pattern_doc() -> Document {
    let mut doc = Document::new(24, 16);
    doc.layers[0].fill_rect(0, 0, 24, 16, Pixel16::from_rgba8(90, 90, 90, 255));
    doc.layers[0].fill_rect(4, 4, 8, 8, Pixel16::from_rgba8(230, 230, 230, 255));
    doc.layers[0].fill_rect(16, 8, 6, 6, Pixel16::from_rgba8(30, 60, 200, 255));
    doc
}

fn px(doc: &Document, x: i32, y: i32) -> (u8, u8, u8, u8) {
    doc.layers[0].get_pixel(x, y).to_rgba8()
}

fn changes(f: &dyn Fn(&mut Document)) -> u64 {
    let mut doc = pattern_doc();
    let before = doc.render_checksum();
    f(&mut doc);
    let after = doc.render_checksum();
    assert_ne!(before, after);
    after
}

#[test]
fn brush_strokes_apply_and_are_deterministic() {
    let cases: Vec<&dyn Fn(&mut Document)> = vec![
        &|d| filter::accented_edges(d, 0, None, 4.0, 40.0, 5.0),
        &|d| filter::angled_strokes(d, 0, None, 50.0, 15.0, 5.0, 7),
        &|d| filter::crosshatch(d, 0, None, 15.0, 5.0, 2.0),
        &|d| filter::dark_strokes(d, 0, None, 5.0, 5.0, 5.0),
        &|d| filter::sprayed_strokes(d, 0, None, 10.0, 12.0, 0, 7),
        &|d| filter::sprayed_strokes(d, 0, None, 10.0, 12.0, 1, 7),
    ];
    for f in cases {
        assert_eq!(changes(f), changes(f));
    }
}

#[test]
fn spatter_with_zero_radius_is_identity() {
    let mut doc = pattern_doc();
    let before = doc.render_checksum();
    filter::spatter(&mut doc, 0, None, 0.0, 7.0, 7);
    assert_eq!(before, doc.render_checksum());
    filter::spatter(&mut doc, 0, None, 12.0, 7.0, 7);
    assert_ne!(before, doc.render_checksum());
}

#[test]
fn ink_outlines_darkens_edges_and_lifts_flats() {
    let mut doc = pattern_doc();
    filter::ink_outlines(&mut doc, 0, None, 10.0, 50.0, 50.0);
    // The pixel just outside the bright block's edge inks down.
    assert!(px(&doc, 3, 8).0 < 90, "inked: {}", px(&doc, 3, 8).0);
    // A flat shadow pixel lifts toward paper (90 -> >90).
    assert!(px(&doc, 0, 0).0 > 90, "lifted: {}", px(&doc, 0, 0).0);
}

#[test]
fn sumi_e_reduces_the_tone_count() {
    let mut doc = Document::new(32, 4);
    for x in 0..32 {
        let v = (x * 8) as u8;
        doc.layers[0].fill_rect(x, 0, 1, 4, Pixel16::from_rgba8(v, v, v, 255));
    }
    filter::sumi_e(&mut doc, 0, None, 3.0, 0.0, 45.0);
    let mut vals: Vec<u8> = (0..32).map(|x| px(&doc, x, 1).0).collect();
    vals.sort_unstable();
    vals.dedup();
    assert!(vals.len() <= 6, "bands: {vals:?}");
}

#[test]
fn crosshatch_darkens_shadows_more_than_highlights() {
    let mut doc = pattern_doc();
    filter::crosshatch(&mut doc, 0, None, 10.0, 0.0, 3.0);
    // Some pixel in the mid-gray field must be hatched darker;
    // the bright block (230) sits above every threshold and
    // keeps its value.
    let mut darkened = false;
    for x in 0..24 {
        if px(&doc, x, 0).0 < 90 {
            darkened = true;
        }
    }
    assert!(darkened, "shadow hatching present");
    assert_eq!(px(&doc, 8, 8).0, 230);
}
