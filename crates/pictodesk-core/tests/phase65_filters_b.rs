//! Phase 6.5 Batch 1b core checks: Wave, Glass, Diffuse Glow,
//! Displace, Lens Flare, Lighting Effects, Texture Fill.

use pictodesk_core::document::Document;
use pictodesk_core::filter;
use pictodesk_core::pixel::Pixel16;

fn pattern_doc() -> Document {
    let mut doc = Document::new(24, 16);
    doc.layers[0].fill_rect(0, 0, 24, 16, Pixel16::from_rgba8(90, 90, 90, 255));
    doc.layers[0].fill_rect(4, 4, 8, 8, Pixel16::from_rgba8(230, 230, 230, 255));
    doc.layers[0].fill_rect(16, 8, 6, 6, Pixel16::from_rgba8(30, 30, 30, 255));
    doc
}

fn layer_px(doc: &Document, x: i32, y: i32) -> (u8, u8, u8, u8) {
    doc.layers[0].get_pixel(x, y).to_rgba8()
}

#[test]
fn wave_and_glass_displace_deterministically() {
    let run = |f: &dyn Fn(&mut Document)| -> u64 {
        let mut doc = pattern_doc();
        let before = doc.render_checksum();
        f(&mut doc);
        let after = doc.render_checksum();
        assert_ne!(before, after);
        after
    };
    let w = |d: &mut Document| filter::wave(d, 0, None, 3, 60.0, 8.0, 100.0, 100.0, 0, false, 7);
    assert_eq!(run(&w), run(&w));
    let g = |d: &mut Document| filter::glass(d, 0, None, 6.0, 4.0, 100.0, 7);
    assert_eq!(run(&g), run(&g));
}

#[test]
fn wave_with_zero_amplitude_is_identity() {
    let mut doc = pattern_doc();
    let before = doc.render_checksum();
    filter::wave(&mut doc, 0, None, 3, 60.0, 0.0, 100.0, 100.0, 0, false, 7);
    assert_eq!(before, doc.render_checksum());
}

#[test]
fn diffuse_glow_brightens_highlights() {
    let mut doc = pattern_doc();
    let before = layer_px(&doc, 8, 8).0;
    filter::diffuse_glow(&mut doc, 0, None, 0.0, 20.0, 20.0, 7);
    // The bright block's neighborhood glows: its dark border
    // pixel picks up light from the blurred highlight.
    let edge_before = 90u8;
    let edge_after = layer_px(&doc, 3, 8).0;
    assert!(edge_after > edge_before, "glow bleeds: {edge_after}");
    let _ = before;
}

#[test]
fn displace_with_midgray_map_is_identity() {
    let mut doc = pattern_doc();
    let before = doc.render_checksum();
    // Mid-gray map: (0.5, 0.5) everywhere = zero deflection.
    let map = vec![0.5f32; 8 * 8 * 2];
    filter::displace(&mut doc, 0, None, &map, 8, 8, 50.0, 50.0, false, false);
    assert_eq!(before, doc.render_checksum());
}

#[test]
fn displace_shifts_content_with_a_bright_map() {
    let mut doc = Document::new(20, 10);
    doc.layers[0].fill_rect(0, 0, 20, 10, Pixel16::from_rgba8(0, 0, 0, 255));
    doc.layers[0].fill_rect(10, 0, 2, 10, Pixel16::from_rgba8(255, 255, 255, 255));
    // White map (1,1): dx = +64 * scale... at 10% scale the
    // deflection is (1-0.5)*2*0.1*128 = 25.6 px — the white
    // bar wraps off; use repeat-edge sampling and check the
    // bar moved right by sampling: at 10% the bar's white
    // appears where black was sampled from the left.
    let map = vec![1.0f32; 4 * 4 * 2];
    let before = doc.render_checksum();
    filter::displace(&mut doc, 0, None, &map, 4, 4, 10.0, 0.0, false, false);
    assert_ne!(before, doc.render_checksum());
    // Content sampled from x-25.6: x=15 previously black,
    // now samples x=-10.6 -> clamped to edge (black). The
    // white bar (x 10..12) now shows at x where x-25.6 in
    // 10..12 -> x in 35..38: outside. Instead the visible
    // change: pixels 10..12 now sample from negative x
    // (black), so the bar disappears from its old spot.
    assert_eq!(layer_px(&doc, 11, 5).0, 0);
}

#[test]
fn lens_flare_brightens_its_center() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(40, 40, 40, 255));
    filter::lens_flare(&mut doc, 0, None, 150.0, 10.0, 10.0, 0);
    assert!(layer_px(&doc, 10, 10).0 > 200);
    // A far corner gains only a little (ghosts aside).
    assert!(layer_px(&doc, 0, 0).0 < 160);
}

#[test]
fn lighting_darkens_away_from_the_light() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(160, 160, 160, 255));
    filter::lighting(&mut doc, 0, None, 60.0, 50.0, 4.0, 4.0, 0.0, true, false);
    let near = layer_px(&doc, 4, 4).0;
    let far = layer_px(&doc, 19, 19).0;
    assert!(near > far, "spotlight falloff: near {near} far {far}");
}

#[test]
fn texture_fill_tiles_the_plane() {
    let mut doc = Document::new(10, 6);
    doc.layers[0].fill_rect(0, 0, 10, 6, Pixel16::from_rgba8(200, 100, 50, 255));
    // 2x2 texture: black, white / white, black.
    let tex = vec![0.0f32, 1.0, 1.0, 0.0];
    filter::texture_fill(&mut doc, 0, None, &tex, 2, 2);
    assert_eq!(layer_px(&doc, 0, 0).0, 0);
    assert_eq!(layer_px(&doc, 1, 0).0, 255);
    assert_eq!(layer_px(&doc, 2, 0).0, 0);
    assert_eq!(layer_px(&doc, 0, 1).0, 255);
    // Alpha survives the fill.
    assert_eq!(layer_px(&doc, 0, 0).3, 255);
}
