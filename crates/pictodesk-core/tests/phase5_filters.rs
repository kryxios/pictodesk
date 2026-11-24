//! Phase 5 quick-win filter tests: the Distort remappers, the
//! Noise/Other utilities (Maximum, Minimum, Dust & Scratches,
//! Diffuse, Difference Clouds, Sharpen Edges), and the one-pass
//! Pixelate / Stylize / Texture / Sketch effects. Each test pins
//! observable behaviour, not internals.

use pictodesk_core::filter;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::Document;

fn solid(w: u32, h: u32, px: (u8, u8, u8)) -> Document {
    let mut doc = Document::new(w, h);
    doc.layers[0].fill_rect(0, 0, w, h, Pixel16::from_rgba8(px.0, px.1, px.2, 255));
    doc
}

#[test]
fn distort_zero_amounts_are_identity() {
    // Every remapper at zero strength leaves a flat field
    // untouched, down to the checksum.
    let flat = solid(16, 16, (90, 140, 200));
    let want = flat.render_checksum();

    let mut doc = flat.clone();
    filter::pinch(&mut doc, 0, None, 0.0);
    assert_eq!(doc.render_checksum(), want, "pinch 0");

    let mut doc = flat.clone();
    filter::spherize(&mut doc, 0, None, 0.0);
    assert_eq!(doc.render_checksum(), want, "spherize 0");

    let mut doc = flat.clone();
    filter::twirl(&mut doc, 0, None, 0.0);
    assert_eq!(doc.render_checksum(), want, "twirl 0");

    let mut doc = flat.clone();
    filter::ripple(&mut doc, 0, None, 0.0, 12.0);
    assert_eq!(doc.render_checksum(), want, "ripple 0");

    let mut doc = flat.clone();
    filter::ocean_ripple(&mut doc, 0, None, 12.0, 0.0);
    assert_eq!(doc.render_checksum(), want, "ocean ripple 0");

    let mut doc = flat.clone();
    filter::shear(&mut doc, 0, None, [0.0; 5]);
    assert_eq!(doc.render_checksum(), want, "shear 0");

    let mut doc = flat.clone();
    filter::zigzag(&mut doc, 0, None, 0.0, 4, 0);
    assert_eq!(doc.render_checksum(), want, "zigzag 0");
}

#[test]
fn distort_flat_field_holds_at_the_center() {
    // At real strengths the remappers move pixels around, but a
    // uniform field has nothing to move: samples that land
    // inside the field return the same color.
    let flat = (90, 140, 200);
    let center = |doc: &Document| doc.layers[0].get_pixel(8, 8).to_rgba8();

    let mut doc = solid(16, 16, flat);
    filter::pinch(&mut doc, 0, None, 0.8);
    assert_eq!(center(&doc), (90, 140, 200, 255), "pinch");

    let mut doc = solid(16, 16, flat);
    filter::spherize(&mut doc, 0, None, -0.7);
    assert_eq!(center(&doc), (90, 140, 200, 255), "spherize");

    let mut doc = solid(16, 16, flat);
    filter::twirl(&mut doc, 0, None, 150.0);
    assert_eq!(center(&doc), (90, 140, 200, 255), "twirl");

    let mut doc = solid(16, 16, flat);
    filter::ripple(&mut doc, 0, None, 30.0, 12.0);
    assert_eq!(center(&doc), (90, 140, 200, 255), "ripple");

    let mut doc = solid(16, 16, flat);
    filter::ocean_ripple(&mut doc, 0, None, 14.0, 20.0);
    assert_eq!(center(&doc), (90, 140, 200, 255), "ocean ripple");

    let mut doc = solid(16, 16, flat);
    filter::shear(&mut doc, 0, None, [0.2, -0.1, 0.05, 0.15, -0.2]);
    assert_eq!(center(&doc), (90, 140, 200, 255), "shear");

    for style in 0..=2u8 {
        let mut doc = solid(16, 16, flat);
        filter::zigzag(&mut doc, 0, None, 60.0, 4, style);
        assert_eq!(center(&doc), (90, 140, 200, 255), "zigzag style {style}");
    }

    let mut doc = solid(16, 16, flat);
    filter::polar_coordinates(&mut doc, 0, None, true);
    assert_eq!(center(&doc), (90, 140, 200, 255), "to polar");

    let mut doc = solid(16, 16, flat);
    filter::polar_coordinates(&mut doc, 0, None, false);
    assert_eq!(center(&doc), (90, 140, 200, 255), "from polar");
}

#[test]
fn distort_actually_moves_a_gradient() {
    // Sanity: at full strength the remappers are not no-ops.
    let mut grad = Document::new(16, 16);
    for x in 0..16 {
        let v = (x * 17) as u8;
        grad.layers[0].fill_rect(x, 0, 1, 16, Pixel16::from_rgba8(v, v, v, 255));
    }
    let before = grad.render_checksum();

    let mut doc = grad.clone();
    filter::pinch(&mut doc, 0, None, 1.0);
    assert_ne!(doc.render_checksum(), before, "pinch moves pixels");

    let mut doc = grad.clone();
    filter::twirl(&mut doc, 0, None, 180.0);
    assert_ne!(doc.render_checksum(), before, "twirl moves pixels");
}

#[test]
fn difference_clouds_is_deterministic_and_changes_mid_gray() {
    let mut a = solid(16, 16, (128, 128, 128));
    let mut b = a.clone();
    let before = a.render_checksum();
    filter::difference_clouds(&mut a, 0, None, 5);
    filter::difference_clouds(&mut b, 0, None, 5);
    assert_eq!(
        a.render_checksum(),
        b.render_checksum(),
        "same seed, same field"
    );
    assert_ne!(a.render_checksum(), before, "the field does something");
    let mut changed = 0;
    for y in 0..16 {
        for x in 0..16 {
            if a.layers[0].get_pixel(x, y).to_rgba8().0 != 128 {
                changed += 1;
            }
        }
    }
    assert!(
        changed > 100,
        "most pixels move off mid-gray, got {changed}"
    );
}

#[test]
fn maximum_spreads_and_minimum_shrinks_an_impulse() {
    let mut doc = solid(9, 9, (0, 0, 0));
    doc.layers[0].set_pixel(4, 4, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::maximum(&mut doc, 0, None, 1);
    assert_eq!(
        doc.layers[0].get_pixel(5, 4).to_rgba8().0,
        255,
        "right neighbor"
    );
    assert_eq!(
        doc.layers[0].get_pixel(3, 3).to_rgba8().0,
        255,
        "diagonal neighbor"
    );
    assert_eq!(
        doc.layers[0].get_pixel(6, 4).to_rgba8().0,
        0,
        "two away is untouched"
    );

    let mut doc = solid(9, 9, (255, 255, 255));
    doc.layers[0].set_pixel(4, 4, Pixel16::from_rgba8(0, 0, 0, 255));
    filter::minimum(&mut doc, 0, None, 1);
    assert_eq!(
        doc.layers[0].get_pixel(4, 5).to_rgba8().0,
        0,
        "below neighbor"
    );
    assert_eq!(
        doc.layers[0].get_pixel(5, 5).to_rgba8().0,
        0,
        "diagonal neighbor"
    );
    assert_eq!(
        doc.layers[0].get_pixel(4, 6).to_rgba8().0,
        255,
        "two away is untouched"
    );
}

#[test]
fn diffuse_output_always_comes_from_the_neighborhood() {
    // A field of distinct colors: after Diffuse, every pixel
    // must equal one of its source neighbors (or, for Darken
    // Only, the darkest of the neighborhood including itself).
    let mut src = Document::new(8, 8);
    for y in 0..8i32 {
        for x in 0..8i32 {
            src.layers[0].set_pixel(
                x,
                y,
                Pixel16::from_rgba8((x * 30) as u8, (y * 30) as u8, ((x + y) * 15) as u8, 255),
            );
        }
    }
    let at = |doc: &Document, x: i32, y: i32| {
        doc.layers[0]
            .get_pixel(x.clamp(0, 7), y.clamp(0, 7))
            .to_rgba8()
    };

    let mut doc = src.clone();
    filter::diffuse(&mut doc, 0, None, 0, 9);
    for y in 0..8i32 {
        for x in 0..8i32 {
            let out = at(&doc, x, y);
            let neighbors = [
                at(&src, x - 1, y),
                at(&src, x + 1, y),
                at(&src, x, y - 1),
                at(&src, x, y + 1),
            ];
            assert!(
                neighbors.contains(&out),
                "mode 0 at ({x},{y}): {out:?} not a neighbor"
            );
        }
    }

    let mut doc = src.clone();
    filter::diffuse(&mut doc, 0, None, 3, 9);
    for y in 0..8i32 {
        for x in 0..8i32 {
            let out = at(&doc, x, y);
            assert!(
                out == at(&src, x - 1, y) || out == at(&src, x + 1, y),
                "anisotropic at ({x},{y}): {out:?} not left/right"
            );
        }
    }

    // Darken Only on a crafted neighborhood picks the darkest.
    let mut doc = solid(5, 5, (200, 200, 200));
    doc.layers[0].set_pixel(1, 2, Pixel16::from_rgba8(50, 50, 50, 255));
    doc.layers[0].set_pixel(3, 2, Pixel16::from_rgba8(80, 80, 80, 255));
    doc.layers[0].set_pixel(2, 1, Pixel16::from_rgba8(110, 110, 110, 255));
    doc.layers[0].set_pixel(2, 3, Pixel16::from_rgba8(140, 140, 140, 255));
    filter::diffuse(&mut doc, 0, None, 1, 9);
    assert_eq!(doc.layers[0].get_pixel(2, 2).to_rgba8().0, 50);
}

#[test]
fn dust_and_scratches_removes_a_speck_keeps_the_flat() {
    let mut doc = solid(7, 7, (120, 120, 120));
    doc.layers[0].set_pixel(3, 3, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::dust_and_scratches(&mut doc, 0, None, 1, 0.1);
    assert_eq!(
        doc.layers[0].get_pixel(3, 3).to_rgba8().0,
        120,
        "speck gone"
    );

    let mut doc = solid(8, 8, (120, 120, 120));
    let before = doc.render_checksum();
    filter::dust_and_scratches(&mut doc, 0, None, 2, 0.1);
    assert_eq!(doc.render_checksum(), before, "flat field untouched");
}

#[test]
fn sharpen_edges_leaves_a_flat_field_alone() {
    let mut doc = solid(8, 8, (100, 150, 200));
    let before = doc.render_checksum();
    filter::sharpen_edges(&mut doc, 0, None);
    assert_eq!(doc.render_checksum(), before, "no edges, no change");
}

#[test]
fn fragment_on_flat_is_identity_and_softens_an_impulse() {
    let mut doc = solid(12, 12, (77, 88, 99));
    filter::fragment(&mut doc, 0, None);
    assert_eq!(doc.layers[0].get_pixel(6, 6).to_rgba8(), (77, 88, 99, 255));

    let mut doc = solid(9, 9, (0, 0, 0));
    doc.layers[0].set_pixel(4, 4, Pixel16::from_rgba8(255, 255, 255, 255));
    filter::fragment(&mut doc, 0, None);
    let center = doc.layers[0].get_pixel(4, 4).to_rgba8().0;
    assert!(
        (48..=54).contains(&center),
        "center keeps a fifth of the impulse, got {center}"
    );
}

#[test]
fn tiles_with_zero_offset_is_identity() {
    let mut doc = Document::new(16, 16);
    for y in 0..16i32 {
        for x in 0..16i32 {
            doc.layers[0].set_pixel(
                x,
                y,
                Pixel16::from_rgba8((x * 16) as u8, (y * 16) as u8, 128, 255),
            );
        }
    }
    let before = doc.render_checksum();
    filter::tiles(&mut doc, 0, None, 4, 0.0, 3);
    assert_eq!(doc.render_checksum(), before);
}

#[test]
fn stamp_effect_is_pure_black_and_white() {
    let mut doc = Document::new(8, 4);
    doc.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(40, 40, 40, 255));
    doc.layers[0].fill_rect(4, 0, 4, 4, Pixel16::from_rgba8(220, 220, 220, 255));
    filter::stamp_effect(&mut doc, 0, None, 0.5, 0);
    for y in 0..4 {
        for x in 0..8 {
            let p = doc.layers[0].get_pixel(x, y).to_rgba8();
            assert!(
                p == (0, 0, 0, 255) || p == (255, 255, 255, 255),
                "stamp pixel {p:?}"
            );
        }
    }
    assert_eq!(
        doc.layers[0].get_pixel(1, 1).to_rgba8().0,
        0,
        "dark side black"
    );
    assert_eq!(
        doc.layers[0].get_pixel(6, 1).to_rgba8().0,
        255,
        "light side white"
    );
}

#[test]
fn mezzotint_is_pure_black_and_white_for_every_kind() {
    for kind in 0..=3u8 {
        let mut doc = solid(16, 16, (128, 128, 128));
        filter::mezzotint(&mut doc, 0, None, kind, 11);
        let (mut black, mut white) = (0, 0);
        for y in 0..16 {
            for x in 0..16 {
                let p = doc.layers[0].get_pixel(x, y).to_rgba8();
                if p == (0, 0, 0, 255) {
                    black += 1;
                } else if p == (255, 255, 255, 255) {
                    white += 1;
                } else {
                    panic!("kind {kind}: mezzotint pixel {p:?} is not black or white");
                }
            }
        }
        assert!(
            black > 0 && white > 0,
            "kind {kind}: mid-gray should print both, got {black} black / {white} white"
        );
    }
}

#[test]
fn glowing_edges_on_flat_is_black() {
    let mut doc = solid(8, 8, (200, 100, 50));
    filter::glowing_edges(&mut doc, 0, None, 1, 1.0, 0);
    assert_eq!(doc.layers[0].get_pixel(4, 4).to_rgba8(), (0, 0, 0, 255));
}

#[test]
fn wind_on_flat_is_identity() {
    let flat = solid(16, 4, (160, 160, 160));
    let want = flat.render_checksum();
    for method in 0..=1u8 {
        for from_left in [true, false] {
            let mut doc = flat.clone();
            filter::wind(&mut doc, 0, None, method, from_left, 13);
            assert_eq!(
                doc.render_checksum(),
                want,
                "method {method} from_left {from_left}"
            );
        }
    }
}

#[test]
fn patchwork_squares_are_flat_and_near_the_average() {
    let mut doc = solid(16, 16, (128, 128, 128));
    filter::patchwork(&mut doc, 0, None, 4, 10);
    assert_eq!(
        doc.layers[0].get_pixel(0, 0).to_rgba8(),
        doc.layers[0].get_pixel(3, 3).to_rgba8(),
        "one square, one color"
    );
    let v = doc.layers[0].get_pixel(0, 0).to_rgba8().0;
    assert!((115..=141).contains(&v), "relief nudges ±10%, got {v}");
}

#[test]
fn bas_relief_on_flat_is_the_midpoint_mix() {
    let mut doc = solid(8, 8, (90, 90, 90));
    let fg = Pixel16::from_rgba8(255, 255, 255, 255);
    let bg = Pixel16::from_rgba8(0, 0, 0, 255);
    filter::bas_relief(&mut doc, 0, None, fg, bg, 1, 0);
    let v = doc.layers[0].get_pixel(4, 4).to_rgba8().0;
    assert!(
        (126..=129).contains(&v),
        "flat relief sits at the mix, got {v}"
    );
}

#[test]
fn facet_on_flat_is_identity() {
    let mut doc = solid(8, 8, (60, 120, 180));
    let before = doc.render_checksum();
    filter::facet(&mut doc, 0, None);
    assert_eq!(doc.render_checksum(), before);
}
