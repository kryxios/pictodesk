//! Phase 7 Batch 7a core checks: the five 7.0 blend modes,
//! the Auto family, healing (stamp + patch), Pattern Maker.

use pictodesk_core::adjust;
use pictodesk_core::blend::BlendMode;
use pictodesk_core::brush::BrushTip;
use pictodesk_core::document::Document;
use pictodesk_core::heal;
use pictodesk_core::pattern;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::selection::Selection;

fn px(doc: &Document, x: i32, y: i32) -> (u8, u8, u8, u8) {
    doc.layers[0].get_pixel(x, y).to_rgba8()
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

#[test]
fn blend_modes_70_values() {
    let b = BlendMode::LinearBurn;
    assert!(approx(b.blend_channel(0.5, 0.5), 0.0));
    assert!(approx(b.blend_channel(0.9, 0.5), 0.4));
    let d = BlendMode::LinearDodge;
    assert!(approx(d.blend_channel(0.5, 0.5), 1.0));
    assert!(approx(d.blend_channel(0.2, 0.3), 0.5));
    let ll = BlendMode::LinearLight;
    assert!(approx(ll.blend_channel(0.5, 0.25), 0.0)); // burn side
    assert!(approx(ll.blend_channel(0.5, 0.75), 1.0)); // dodge side
    let pl = BlendMode::PinLight;
    assert!(approx(pl.blend_channel(0.6, 0.25), 0.5)); // darker wins
    assert!(approx(pl.blend_channel(0.4, 0.75), 0.5)); // lighter wins
    let vl = BlendMode::VividLight;
    assert!(approx(vl.blend_channel(0.5, 0.0), 0.0));
    assert!(approx(vl.blend_channel(0.5, 1.0), 1.0));
    // All five are in the menu list with stable ids.
    for (m, id) in [
        (BlendMode::LinearBurn, 17u8),
        (BlendMode::LinearDodge, 18),
        (BlendMode::VividLight, 19),
        (BlendMode::LinearLight, 20),
        (BlendMode::PinLight, 21),
    ] {
        assert_eq!(m.to_id(), id);
        assert_eq!(BlendMode::from_id(id), Some(m));
    }
    assert_eq!(pictodesk_core::blend::ALL_BLEND_MODES.len(), 22);
}

fn gray_doc(w: u32, h: u32, v: u8) -> Document {
    let mut doc = Document::new(w, h);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
        }
    }
    doc
}

#[test]
fn auto_levels_stretches() {
    // Low contrast: everything between 100 and 150.
    let mut doc = gray_doc(20, 20, 100);
    for i in 0..10 {
        doc.layers[0].set_pixel(i, 0, Pixel16::from_rgba8(150, 150, 150, 255));
    }
    adjust::auto_levels(&mut doc, 0, None);
    assert_eq!(px(&doc, 15, 10).0, 0, "dark end goes to black");
    assert_eq!(px(&doc, 5, 0).0, 255, "bright end goes to white");
}

#[test]
fn auto_contrast_keeps_hue_direction() {
    // Red-dominant low contrast: a shared stretch must not
    // turn the red channel's lead into a different hue.
    let mut doc = Document::new(20, 20);
    for y in 0..20 {
        for x in 0..20 {
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(120, 90, 90, 255));
        }
    }
    for i in 0..10 {
        doc.layers[0].set_pixel(i, 0, Pixel16::from_rgba8(160, 110, 110, 255));
    }
    adjust::auto_contrast(&mut doc, 0, None);
    // Combined range is 90..160: the shared stretch sends
    // the darkest channel value to 0 and the brightest to
    // 255, with every channel moved by the same LUT.
    let (r, g, _, _) = px(&doc, 15, 10);
    assert_eq!(g, 0);
    assert_eq!(r, 109); // (120-90)/(160-90)*255
    let (r, g, _, _) = px(&doc, 5, 0);
    assert_eq!(r, 255);
    assert!(r > g, "red stays ahead after a shared stretch");
}

#[test]
fn auto_color_reduces_cast() {
    // A neutral scene shot through a strong red cast.
    let mut doc = Document::new(30, 30);
    for y in 0..30 {
        for x in 0..30 {
            let v = 60 + ((x * 5) % 120) as u8;
            doc.layers[0].set_pixel(
                x,
                y,
                Pixel16::from_rgba8(
                    ((v as u16 * 3 / 2).min(255)) as u8,
                    v,
                    (v as u16 * 2 / 3) as u8,
                    255,
                ),
            );
        }
    }
    let mean = |doc: &Document, ch: usize| -> f32 {
        let mut s = 0f32;
        for y in 0..30 {
            for x in 0..30 {
                let p = px(doc, x, y);
                s += [p.0, p.1, p.2][ch] as f32;
            }
        }
        s / 900.0
    };
    let spread_before = (mean(&doc, 0) - mean(&doc, 2)).abs();
    adjust::auto_color(&mut doc, 0, None);
    let spread_after = (mean(&doc, 0) - mean(&doc, 2)).abs();
    assert!(
        spread_after < spread_before * 0.6,
        "cast reduced: {spread_before} -> {spread_after}"
    );
}

/// Destination: smooth dark-to-light gradient. Source: flat
/// mid-gray with a noise texture. A clone would import the
/// flat brightness; a heal keeps the gradient and adds the
/// texture.
fn heal_scene() -> Document {
    let mut doc = Document::new(60, 30);
    for y in 0..30 {
        for x in 0..60 {
            let v = if x < 30 {
                // destination: gradient 40..100
                40 + (x * 2) as u8
            } else {
                // source: flat 180 with +-20 checker texture
                (180 + if (x + y) % 2 == 0 { 20 } else { -20 }) as u8
            };
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
        }
    }
    doc
}

#[test]
fn heal_stamp_keeps_destination_tone() {
    let mut doc = heal_scene();
    let tip = BrushTip::new(15.0, 0.5);
    // Heal at (10, 15) sampling from (45, 15).
    heal::stamp_heal(&mut doc, 0, None, 10.0, 15.0, (45.0, 15.0), &tip, 1.0);
    let center = px(&doc, 10, 15).0;
    // Destination tone there is ~60; the source is ~180. A
    // heal lands near the destination tone (with texture),
    // nowhere near a clone's 180.
    assert!(
        center < 110,
        "healed center keeps destination tone, got {center}"
    );
    // Texture arrived: neighbors differ by more than the
    // smooth gradient's 2-per-pixel step.
    let a = px(&doc, 9, 15).0 as i32;
    let b = px(&doc, 10, 15).0 as i32;
    assert!((a - b).abs() > 4, "source texture transferred: {a} vs {b}");
}

#[test]
fn heal_patch_fills_selection() {
    let mut doc = heal_scene();
    // A "blemish": dark spot in the destination area.
    for y in 12..18 {
        for x in 8..14 {
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(10, 10, 10, 255));
        }
    }
    let sel = Selection::rect(60, 30, 8, 12, 6, 6);
    heal::heal_patch(&mut doc, 0, &sel, 37, 0);
    let center = px(&doc, 10, 15).0;
    assert!(
        center > 40,
        "blemish healed toward destination tone, got {center}"
    );
}

#[test]
fn pattern_maker_tile_is_seamless() {
    // Random-ish texture sample.
    let mut doc = Document::new(40, 40);
    for y in 0..40 {
        for x in 0..40 {
            let v = ((x * 37 + y * 91) % 200 + 30) as u8;
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
        }
    }
    let tile = pattern::make_tile(&doc, (0, 0, 40, 40), "T").unwrap();
    assert_eq!((tile.width, tile.height), (40, 40));
    let t = |x: usize, y: usize| tile.px[y * 40 + x].to_rgba8().0 as i32;
    // Opposite edges continue each other: the jump across
    // the seam is no worse than a typical interior jump.
    let mut seam = 0;
    let mut inner = 0;
    for y in 0..40 {
        seam = seam.max((t(39, y) - t(0, y)).abs());
        inner = inner.max((t(20, y) - t(21, y)).abs());
    }
    assert!(seam <= inner + 24, "seam jump {seam} vs interior {inner}");
    // The center of the tile is the untouched sample.
    let c = tile.px[20 * 40 + 20].to_rgba8().0;
    let orig = ((20 * 37 + 20 * 91) % 200 + 30) as u8;
    assert_eq!(c, orig, "tile center keeps the sample");
}

#[test]
fn tip_ellipse_and_texture_and_dual() {
    use pictodesk_core::brush::{BrushTexture, DualTip};
    use std::rc::Rc;
    // Ellipse: roundness 0.25 squashes the y extent.
    let mut tip = BrushTip::new(20.0, 1.0);
    tip.roundness = 0.25;
    let sx = tip.strength(19, 10, 10.0, 10.0, 9.0);
    let sy = tip.strength(10, 19, 10.0, 10.0, 9.0);
    assert!(sx > 0.9, "x extent keeps full strength: {sx}");
    assert_eq!(sy, 0.0, "y extent is squashed away");
    // Texture multiply: a black texture cell kills the stamp.
    let mut tip2 = BrushTip::new(10.0, 1.0);
    tip2.tex = Some(Rc::new(BrushTexture {
        w: 2,
        h: 1,
        lum: vec![0.0, 1.0],
        scale: 1.0,
        depth: 1.0,
        min_depth: 0.0,
        mode: 0,
        invert: false,
    }));
    assert_eq!(tip2.strength(0, 0, 0.0, 0.0, 0.0), 0.0);
    assert!(tip2.strength(1, 0, 1.0, 0.0, 0.0) > 0.9);
    // Dual brush (multiply): where the second tip is absent,
    // the stamp vanishes.
    let mut tip3 = BrushTip::new(10.0, 1.0);
    tip3.dual = Some(Box::new(DualTip {
        size: 4.0,
        hardness: 1.0,
        mask: None,
        mode: 1, // Multiply id? 1 is Multiply in BlendMode ids
        dx: 0.0,
        dy: 0.0,
    }));
    let center = tip3.strength(5, 5, 5.0, 5.0, 0.0);
    let edge = tip3.strength(9, 5, 5.0, 5.0, 4.0);
    assert!(center > 0.9, "dual center keeps strength: {center}");
    assert_eq!(edge, 0.0, "outside the dual tip: no stamp");
}

#[test]
fn turbulence_and_mesh_roundtrip() {
    use pictodesk_core::liquify::{field_from_bytes, field_to_bytes, LiquifyField};
    let mut f = LiquifyField::new(30, 30);
    f.stamp_turbulence((15.0, 15.0), 12.0, 1.0, &[], 3);
    assert!(!f.is_identity(), "turbulence displaces the field");
    // Deterministic for the same seed.
    let mut g = LiquifyField::new(30, 30);
    g.stamp_turbulence((15.0, 15.0), 12.0, 1.0, &[], 3);
    assert_eq!(f.disp, g.disp);
    let bytes = field_to_bytes(&f);
    let back = field_from_bytes(&bytes).expect("mesh parses");
    assert_eq!(back.w, 30);
    assert_eq!(back.disp, f.disp);
    assert!(field_from_bytes(b"junk").is_none());
}

#[test]
fn wbmp_and_xmp() {
    use pictodesk_core::io::{encode_wbmp, export_image, ExportFormat};
    // 8x1: black white black white... luminance decides bits.
    let mut rgba = Vec::new();
    for i in 0..8 {
        let v = if i % 2 == 0 { 0u8 } else { 255u8 };
        rgba.extend_from_slice(&[v, v, v, 255]);
    }
    let wb = encode_wbmp(&rgba, 8, 1);
    assert_eq!(&wb[0..4], &[0, 0, 8, 1]);
    assert_eq!(wb[4], 0b0101_0101);
    // XMP rides in JPEG exports when File Info is set.
    let mut doc = Document::new(8, 8);
    doc.info_title = "Sunset & Sea".into();
    doc.info_keywords = "beach, dusk".into();
    let bytes = export_image(&doc, ExportFormat::Jpeg, 90).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("http://ns.adobe.com/xap/1.0/"), "xmp header");
    assert!(text.contains("Sunset &amp; Sea"), "escaped title");
    assert!(text.contains("<rdf:li>beach</rdf:li>"), "keyword bag");
    let plain = export_image(&Document::new(8, 8), ExportFormat::Jpeg, 90).unwrap();
    assert!(
        !String::from_utf8_lossy(&plain).contains("xmpmeta"),
        "no xmp without file info"
    );
}
