//! Phase 6.5 Batch 1c core checks: the Artistic gallery.

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
fn artistic_set_applies_and_is_deterministic() {
    let cases: Vec<&dyn Fn(&mut Document)> = vec![
        &|d| filter::colored_pencil(d, 0, None, 4.0, 8.0, 25.0, 7),
        &|d| filter::dry_brush(d, 0, None, 5.0, 5.0, 2.0, 7),
        &|d| filter::fresco(d, 0, None, 5.0, 5.0, 2.0, 7),
        &|d| filter::film_grain(d, 0, None, 8.0, 5.0, 5.0, 7),
        &|d| filter::paint_daubs(d, 0, None, 8.0, 10.0, 0, 7),
        &|d| filter::plastic_wrap(d, 0, None, 8.0, 8.0, 8.0),
        &|d| filter::smudge_stick(d, 0, None, 6.0, 8.0, 5.0),
        &|d| filter::sponge(d, 0, None, 4.0, 12.0, 7.0, 7),
        &|d| filter::watercolor(d, 0, None, 8.0, 5.0, 2.0, 7),
        &|d| filter::rough_pastels(d, 0, None, 10.0, 8.0, 100.0, 20.0, 0),
        &|d| filter::underpainting(d, 0, None, 8.0, 20.0, 100.0, 20.0, 3),
    ];
    for f in cases {
        assert_eq!(changes(f), changes(f));
    }
}

#[test]
fn cutout_quantizes_to_levels() {
    let mut doc = pattern_doc();
    filter::cutout(&mut doc, 0, None, 4.0, 0.0, 1.0);
    // Levels = 4: every channel lands on a multiple of 85.
    for (x, y) in [(0, 0), (6, 6), (18, 10), (23, 15)] {
        let p = px(&doc, x, y);
        for v in [p.0, p.1, p.2] {
            assert_eq!(v % 85, 0, "channel {v} not on a level step at ({x},{y})");
        }
    }
}

#[test]
fn palette_knife_flattens_blocks() {
    let mut doc = pattern_doc();
    // Detail 1 = no original pulled back; softness 0 = hard
    // blocks. Two pixels in the same 28x14 block must match.
    filter::palette_knife(&mut doc, 0, None, 14.0, 1.0, 0.0);
    assert_eq!(px(&doc, 1, 1), px(&doc, 20, 5));
}

#[test]
fn poster_edges_inks_the_boundary() {
    let mut doc = pattern_doc();
    let flat_before = px(&doc, 0, 0).0;
    filter::poster_edges(&mut doc, 0, None, 10.0, 10.0, 0.0);
    // Pixel just left of the bright block's left edge (x=3)
    // sits on a strong gradient and gets inked darker.
    assert!(px(&doc, 3, 8).0 < 90, "edge inked: {}", px(&doc, 3, 8).0);
    // A flat interior pixel keeps its posterized value
    // (90 is not on the 8-level grid; it quantizes to 85 or
    // 102 — either way it is not inked to black).
    assert!(px(&doc, 0, 0).0 > 40);
    let _ = flat_before;
}

#[test]
fn neon_glow_tints_edges_with_the_glow_color() {
    let mut doc = pattern_doc();
    let red = Pixel16::from_rgba8(255, 0, 0, 255);
    filter::neon_glow(&mut doc, 0, None, 10.0, 50.0, red);
    // On the bright block's boundary the red channel must
    // clearly exceed the blue channel (red glow added).
    let p = px(&doc, 3, 8);
    assert!(p.0 > p.2 + 20, "edge glows red: {p:?}");
}

#[test]
fn film_grain_leaves_shadows_quieter_than_highlights() {
    let mut doc = Document::new(40, 4);
    doc.layers[0].fill_rect(0, 0, 20, 4, Pixel16::from_rgba8(20, 20, 20, 255));
    doc.layers[0].fill_rect(20, 0, 20, 4, Pixel16::from_rgba8(235, 235, 235, 255));
    filter::film_grain(&mut doc, 0, None, 20.0, 10.0, 10.0, 3);
    let dev = |doc: &Document, x0: i32| -> i32 {
        let mut d = 0;
        for x in x0..x0 + 20 {
            d += (px(doc, x, 1).0 as i32 - px(doc, x, 2).0 as i32).abs();
        }
        d
    };
    assert!(dev(&doc, 20) > dev(&doc, 0), "grain stronger in highlights");
}
