//! Phase 6.5 Batch 1e core checks: the Sketch + Texture
//! galleries (closes Batch 1).
#![allow(clippy::type_complexity)]

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
fn sketch_and_texture_apply_and_are_deterministic() {
    let black = Pixel16::from_rgba8(0, 0, 0, 255);
    let white = Pixel16::from_rgba8(255, 255, 255, 255);
    let cases: Vec<Box<dyn Fn(&mut Document)>> = vec![
        Box::new(|d| filter::chalk_charcoal(d, 0, None, 25.0, 25.0, 3.0, black, white, 7)),
        Box::new(|d| filter::charcoal_fx(d, 0, None, 4.0, 3.0, 50.0)),
        Box::new(|d| filter::chrome(d, 0, None, 5.0, 5.0)),
        Box::new(|d| filter::conte_crayon(d, 0, None, 50.0, 50.0, 100.0, 20.0, 0, black, white)),
        Box::new(|d| filter::note_paper(d, 0, None, 25.0, 10.0, 12.0, 7)),
        Box::new(|d| filter::photocopy(d, 0, None, 12.0, 25.0, 7)),
        Box::new(|d| filter::plaster(d, 0, None, 25.0, 7.0, 0)),
        Box::new(|d| filter::reticulation(d, 0, None, 25.0, 25.0, 25.0, black, white, 7)),
        Box::new(|d| filter::water_paper(d, 0, None, 15.0, 60.0, 55.0, black, white, 7)),
        Box::new(|d| filter::craquelure(d, 0, None, 12.0, 6.0, 2.0, 7)),
        Box::new(|d| filter::mosaic_tiles(d, 0, None, 8.0, 2.0, 5.0, 7)),
        Box::new(|d| filter::stained_glass(d, 0, None, 10.0, 2.0, 5.0, 7)),
        Box::new(|d| filter::texturizer(d, 0, None, 100.0, 30.0, 0, 0)),
    ];
    for f in &cases {
        assert_eq!(changes(f.as_ref()), changes(f.as_ref()));
    }
    for kind in 0..10u8 {
        let g = move |d: &mut Document| filter::grain_fx(d, 0, None, 60.0, 50.0, kind, 7);
        assert_eq!(changes(&g), changes(&g), "grain kind {kind}");
    }
    for kind in 0..3u8 {
        let h = move |d: &mut Document| filter::halftone_pattern(d, 0, None, 4.0, 25.0, kind);
        assert_eq!(changes(&h), changes(&h), "halftone kind {kind}");
    }
    for dir in 0..4u8 {
        let p =
            move |d: &mut Document| filter::graphic_pen(d, 0, None, 15.0, 50.0, dir, black, white);
        assert_eq!(changes(&p), changes(&p), "graphic pen dir {dir}");
    }
}

#[test]
fn graphic_pen_uses_only_ink_and_paper() {
    let mut doc = pattern_doc();
    let black = Pixel16::from_rgba8(0, 0, 0, 255);
    let white = Pixel16::from_rgba8(255, 255, 255, 255);
    filter::graphic_pen(&mut doc, 0, None, 15.0, 50.0, 0, black, white);
    let mut pure = 0;
    let mut total = 0;
    for y in 0..16 {
        for x in 0..24 {
            total += 1;
            let p = px(&doc, x, y);
            if (p.0 == 0 || p.0 == 255) && p.0 == p.1 && p.1 == p.2 {
                pure += 1;
            }
        }
    }
    assert!(pure * 100 / total >= 85, "pure ink/paper: {pure}/{total}");
}

#[test]
fn torn_edges_paints_with_fg_and_bg() {
    let mut doc = pattern_doc();
    let red = Pixel16::from_rgba8(255, 0, 0, 255);
    let blue = Pixel16::from_rgba8(0, 0, 255, 255);
    filter::torn_edges(&mut doc, 0, None, 25.0, 7.0, 12.0, red, blue, 7);
    // Every pixel is a red/blue mix: green channel stays 0.
    for y in 0..16 {
        for x in 0..24 {
            assert_eq!(px(&doc, x, y).1, 0, "green leaked at ({x},{y})");
        }
    }
    // Both colors appear (the doc has dark and bright areas).
    let has_red = (0..16).any(|y| (0..24).any(|x| px(&doc, x, y).0 > 200));
    let has_blue = (0..16).any(|y| (0..24).any(|x| px(&doc, x, y).2 > 200));
    assert!(has_red && has_blue);
}

#[test]
fn halftone_on_flat_gray_makes_black_and_white() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(128, 128, 128, 255));
    filter::halftone_pattern(&mut doc, 0, None, 4.0, 25.0, 0);
    let mut blacks = 0;
    let mut whites = 0;
    for y in 0..20 {
        for x in 0..20 {
            match px(&doc, x, y).0 {
                0 => blacks += 1,
                255 => whites += 1,
                v => panic!("halftone produced gray {v}"),
            }
        }
    }
    assert!(
        blacks > 20 && whites > 20,
        "screen: {blacks} black, {whites} white"
    );
}

#[test]
fn mosaic_tiles_have_uniform_interiors() {
    let mut doc = pattern_doc();
    filter::mosaic_tiles(&mut doc, 0, None, 8.0, 1.0, 0.0, 7);
    // Inside one tile (away from the 1px grout) all pixels
    // share the tile's averaged, jittered value.
    assert_eq!(px(&doc, 2, 2), px(&doc, 5, 2));
    assert_eq!(px(&doc, 2, 2), px(&doc, 2, 5));
}
