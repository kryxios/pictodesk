//! Phase 4.5 gap-pass tests: the five remaining classic blend
//! modes (incl. the Dissolve compositor gate), Equalize, the
//! Select > Modify family, Color Range, and the twelve added
//! filters. Each test pins observable behaviour, not internals.

use pictodesk_core::adjust;
use pictodesk_core::blend::BlendMode;
use pictodesk_core::filter;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::selection::Selection;
use pictodesk_core::Document;

fn solid(w: u32, h: u32, px: (u8, u8, u8)) -> Document {
    let mut doc = Document::new(w, h);
    doc.layers[0].fill_rect(0, 0, w, h, Pixel16::from_rgba8(px.0, px.1, px.2, 255));
    doc
}

#[test]
fn new_blend_modes_composite() {
    // Hard Light: a white source over mid-gray pushes toward white
    // harder than Normal-at-half would; a black source darkens.
    let mut doc = solid(4, 4, (128, 128, 128));
    let top = doc.add_layer("T");
    doc.layers[top].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(255, 255, 255, 255));
    doc.layers[top].blend_mode = BlendMode::HardLight;
    assert_eq!(doc.render_pixel(2, 2).to_rgba8().0, 255);
    doc.layers[top].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(0, 0, 0, 255));
    assert_eq!(doc.render_pixel(2, 2).to_rgba8().0, 0);

    // Color Dodge with a white source is white regardless.
    doc.layers[top].blend_mode = BlendMode::ColorDodge;
    doc.layers[top].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(255, 255, 255, 255));
    assert_eq!(doc.render_pixel(2, 2).to_rgba8().0, 255);
    // Color Burn with a black source is black.
    doc.layers[top].blend_mode = BlendMode::ColorBurn;
    doc.layers[top].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(0, 0, 0, 255));
    assert_eq!(doc.render_pixel(2, 2).to_rgba8().0, 0);
    // Exclusion with black is identity.
    doc.layers[top].blend_mode = BlendMode::Exclusion;
    assert_eq!(doc.render_pixel(2, 2).to_rgba8().0, 128);
}

#[test]
fn dissolve_is_a_stable_gate() {
    // Opaque dissolve == Normal.
    let mut doc = solid(16, 16, (10, 10, 10));
    let top = doc.add_layer("T");
    doc.layers[top].fill_rect(0, 0, 16, 16, Pixel16::from_rgba8(250, 250, 250, 255));
    doc.layers[top].blend_mode = BlendMode::Dissolve;
    assert_eq!(doc.render_pixel(3, 3).to_rgba8().0, 250);
    // At 50% opacity roughly half the pixels land — and the render
    // is identical between calls (no shimmer).
    doc.layers[top].opacity = 0.5;
    let a = doc.render_checksum();
    let b = doc.render_checksum();
    assert_eq!(a, b);
    let mut landed = 0;
    for y in 0..16 {
        for x in 0..16 {
            if doc.render_pixel(x, y).to_rgba8().0 == 250 {
                landed += 1;
            }
        }
    }
    assert!(
        (90..=166).contains(&landed),
        "expected roughly half of 256 pixels to land, got {landed}"
    );
}

#[test]
fn equalize_spreads_a_narrow_range() {
    // Pixels only at 100 and 150: after equalize they sit at the
    // extremes 0 and 255.
    let mut doc = Document::new(8, 4);
    for x in 0..4 {
        doc.layers[0].fill_rect(x, 0, 1, 4, Pixel16::from_rgba8(100, 100, 100, 255));
    }
    for x in 4..8 {
        doc.layers[0].fill_rect(x, 0, 1, 4, Pixel16::from_rgba8(150, 150, 150, 255));
    }
    adjust::equalize(&mut doc, 0, None);
    assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 0);
    assert_eq!(doc.layers[0].get_pixel(6, 1).to_rgba8().0, 255);
}

#[test]
fn modify_expand_contract_border_smooth() {
    let mut sel = Selection::rect(20, 20, 8, 8, 4, 4);
    sel.expand(2);
    assert_eq!(sel.value(6, 8), 255, "expand grows straight out by 2");
    assert_eq!(sel.value(6, 6), 0, "the diagonal is 2.83 away — beyond 2");
    assert_eq!(sel.value(5, 5), 0);
    sel.contract(1);
    assert_eq!(sel.value(6, 8), 0, "contract pulls the new edge back in");
    assert_eq!(sel.value(8, 8), 255);

    let mut band = Selection::rect(20, 20, 5, 5, 10, 10);
    band.border(4);
    assert_eq!(band.value(5, 10), 255, "the band covers the old edge");
    assert_eq!(band.value(3, 10), 255, "and reaches outside it");
    assert_eq!(band.value(10, 10), 0, "the middle is hollow");
    assert_eq!(band.value(1, 10), 0, "and it ends");

    // Smooth kills a one-pixel speck.
    let mut speck = Selection::none(12, 12);
    speck.set(6, 6, 255);
    speck.smooth(1);
    assert!(speck.is_empty(), "a lone pixel does not survive smoothing");
}

#[test]
fn color_range_selects_by_similarity() {
    let mut doc = Document::new(10, 2);
    doc.layers[0].fill_rect(0, 0, 5, 2, Pixel16::from_rgba8(200, 40, 40, 255));
    doc.layers[0].fill_rect(5, 0, 5, 2, Pixel16::from_rgba8(40, 40, 200, 255));
    let sel = Selection::color_range(&doc, (200, 40, 40), 40);
    assert_eq!(sel.value(2, 1), 255);
    assert_eq!(sel.value(7, 1), 0);
}

#[test]
fn filter_solarize_and_mosaic() {
    let mut doc = solid(4, 4, (200, 100, 30));
    filter::solarize(&mut doc, 0, None);
    // 200 > 128 inverts to 55; 100 and 30 pass through.
    assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8(), (55, 100, 30, 255));

    let mut doc = Document::new(4, 4);
    doc.layers[0].fill_rect(0, 0, 2, 4, Pixel16::from_rgba8(0, 0, 0, 255));
    doc.layers[0].fill_rect(2, 0, 2, 4, Pixel16::from_rgba8(200, 200, 200, 255));
    filter::mosaic(&mut doc, 0, None, 4);
    assert_eq!(doc.layers[0].get_pixel(0, 0).to_rgba8().0, 100);
    assert_eq!(doc.layers[0].get_pixel(3, 3).to_rgba8().0, 100);
}

#[test]
fn filter_offset_wraps_and_custom_identity() {
    let mut doc = Document::new(6, 3);
    doc.layers[0].set_pixel(0, 1, Pixel16::from_rgba8(255, 0, 0, 255));
    filter::offset_filter(&mut doc, 0, None, 2, 0);
    assert_eq!(doc.layers[0].get_pixel(2, 1).to_rgba8().0, 255);
    assert_eq!(doc.layers[0].get_pixel(0, 1).to_rgba8().0, 0);
    filter::offset_filter(&mut doc, 0, None, -2, 0);
    assert_eq!(doc.layers[0].get_pixel(0, 1).to_rgba8().0, 255);

    // Identity kernel (center 1, scale 1, offset 0) changes nothing.
    let mut doc = solid(5, 5, (77, 88, 99));
    let before = doc.render_checksum();
    let mut k = [0.0f32; 25];
    k[12] = 1.0;
    filter::custom_filter(&mut doc, 0, None, k, 1.0, 0.0);
    assert_eq!(doc.render_checksum(), before);
}

#[test]
fn filter_median_kills_impulse_despeckle_keeps_edge() {
    let mut doc = solid(7, 7, (120, 120, 120));
    doc.layers[0].set_pixel(3, 3, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::median(&mut doc, 0, None, 1);
    assert_eq!(doc.layers[0].get_pixel(3, 3).to_rgba8().0, 120);

    // Despeckle removes the same speck...
    let mut doc = solid(7, 7, (120, 120, 120));
    doc.layers[0].set_pixel(3, 3, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::despeckle(&mut doc, 0, None);
    assert_eq!(doc.layers[0].get_pixel(3, 3).to_rgba8().0, 120);
    // ...but leaves a real step edge's pixels alone (they agree
    // with half their neighborhood).
    let mut doc = Document::new(6, 6);
    doc.layers[0].fill_rect(0, 0, 3, 6, Pixel16::from_rgba8(60, 60, 60, 255));
    doc.layers[0].fill_rect(3, 0, 3, 6, Pixel16::from_rgba8(180, 180, 180, 255));
    let before = doc.render_checksum();
    filter::despeckle(&mut doc, 0, None);
    assert_eq!(doc.render_checksum(), before);
}

#[test]
fn filter_emboss_flat_is_gray_find_edges_flat_is_white() {
    let mut doc = solid(6, 6, (140, 90, 200));
    filter::emboss(&mut doc, 0, None, 1.0);
    let p = doc.layers[0].get_pixel(3, 3).to_rgba8();
    assert!(
        p.0 >= 126 && p.0 <= 130 && p.0 == p.1 && p.1 == p.2,
        "flat emboss {p:?}"
    );

    let mut doc = solid(6, 6, (140, 90, 200));
    filter::find_edges(&mut doc, 0, None);
    assert_eq!(
        doc.layers[0].get_pixel(3, 3).to_rgba8(),
        (255, 255, 255, 255)
    );
}

#[test]
fn filter_high_pass_flat_is_mid_gray() {
    let mut doc = solid(8, 8, (160, 40, 90));
    filter::high_pass(&mut doc, 0, None, 2.0);
    let p = doc.layers[0].get_pixel(4, 4).to_rgba8();
    assert!(p.0 >= 126 && p.0 <= 130, "flat high pass {p:?}");
}

#[test]
fn filter_clouds_deterministic_fg_bg_only() {
    let fg = Pixel16::from_rgba8(255, 255, 255, 255);
    let bg = Pixel16::from_rgba8(0, 0, 0, 255);
    let mut a = solid(24, 24, (9, 9, 9));
    let mut b = a.clone();
    filter::clouds(&mut a, 0, None, fg, bg, 7);
    filter::clouds(&mut b, 0, None, fg, bg, 7);
    assert_eq!(a.render_checksum(), b.render_checksum());
    // Every pixel is a gray between bg and fg, and the field varies.
    let mut seen = std::collections::HashSet::new();
    for y in 0..24 {
        for x in 0..24 {
            let p = a.layers[0].get_pixel(x, y).to_rgba8();
            assert_eq!(p.0, p.1);
            assert_eq!(p.1, p.2);
            seen.insert(p.0);
        }
    }
    assert!(
        seen.len() > 20,
        "clouds should vary, saw {} values",
        seen.len()
    );
}

#[test]
fn filter_motion_blur_smears_along_angle() {
    let mut doc = Document::new(15, 5);
    doc.layers[0].fill_rect(0, 0, 15, 5, Pixel16::from_rgba8(0, 0, 0, 255));
    doc.layers[0].set_pixel(7, 2, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::motion_blur(&mut doc, 0, None, 0.0, 6.0);
    assert!(
        doc.layers[0].get_pixel(9, 2).to_rgba8().0 > 10,
        "smear reaches right"
    );
    assert!(
        doc.layers[0].get_pixel(5, 2).to_rgba8().0 > 10,
        "smear reaches left"
    );
    assert_eq!(
        doc.layers[0].get_pixel(7, 0).to_rgba8().0,
        0,
        "no vertical smear at 0 deg"
    );
}

#[test]
fn filter_radial_blur_keeps_the_center() {
    let mut doc = Document::new(11, 11);
    doc.layers[0].fill_rect(0, 0, 11, 11, Pixel16::from_rgba8(0, 0, 0, 255));
    doc.layers[0].set_pixel(5, 5, Pixel16::from_rgba8(255, 255, 255, 255));
    doc.layers[0].set_pixel(9, 5, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::radial_blur(&mut doc, 0, None, 60.0);
    assert_eq!(
        doc.layers[0].get_pixel(5, 5).to_rgba8().0,
        255,
        "center is the pivot"
    );
    assert!(
        doc.layers[0].get_pixel(9, 5).to_rgba8().0 < 255,
        "off-center pixels smear along the arc"
    );
}
