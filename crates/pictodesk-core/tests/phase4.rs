//! Phase 4 task test, at engine level: color-grade a photo entirely
//! with adjustment layers, save, reopen later, change your mind —
//! every step still editable, and the pixels underneath untouched.
//! Plus layer masks on regular layers and the .pdk round-trip of
//! everything Phase 4 adds.

use pictodesk_core::adjust::{self, Adjustment};
use pictodesk_core::format;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::Document;

/// A stand-in "photo": a smooth gradient with two color blocks.
fn photo() -> Document {
    let mut doc = Document::new(48, 32);
    for y in 0..32i32 {
        for x in 0..48i32 {
            let r = (x * 255 / 47) as u8;
            let g = (y * 255 / 31) as u8;
            let b = ((x + y) * 255 / 78) as u8;
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(r, g, b, 255));
        }
    }
    doc.layers[0].fill_rect(4, 4, 12, 8, Pixel16::from_rgba8(200, 60, 40, 255));
    doc.layers[0].fill_rect(30, 18, 12, 8, Pixel16::from_rgba8(40, 90, 190, 255));
    doc
}

#[test]
fn grade_with_adjustment_layers_save_reopen_change_mind() {
    let mut doc = photo();
    let original_checksum = doc.render_checksum();

    // Grade entirely with adjustment layers: Levels, then a warm
    // Photo Filter, then a saturation lift.
    doc.add_adjustment_layer(
        "Levels",
        Adjustment::Levels {
            in_black: 20,
            in_white: 235,
            gamma: 1.2,
            out_black: 0,
            out_white: 255,
        },
    );
    doc.add_adjustment_layer(
        "Photo Filter",
        Adjustment::PhotoFilter {
            color: (255, 178, 84),
            density: 40,
        },
    );
    doc.add_adjustment_layer(
        "Hue/Saturation",
        Adjustment::HueSaturation {
            hue: 0,
            saturation: 25,
            lightness: 0,
        },
    );
    let graded_checksum = doc.render_checksum();
    assert_ne!(
        graded_checksum, original_checksum,
        "the grade must change the image"
    );

    // Save and reopen "a week later".
    let bytes = format::serialize(&doc);
    let mut reopened = format::deserialize(&bytes).expect("reopen");
    assert_eq!(reopened.layers.len(), 4);
    assert_eq!(
        reopened.render_checksum(),
        graded_checksum,
        "render must survive the round-trip"
    );
    match &reopened.layers[1].adjustment {
        Some(Adjustment::Levels {
            in_black, gamma, ..
        }) => {
            assert_eq!(*in_black, 20);
            assert!((gamma - 1.2).abs() < 1e-6);
        }
        other => panic!("levels params lost: {other:?}"),
    }

    // Change your mind: soften the Levels gamma — still editable,
    // and the render follows.
    if let Some(Adjustment::Levels { gamma, .. }) = &mut reopened.layers[1].adjustment {
        *gamma = 0.9;
    }
    assert_ne!(reopened.render_checksum(), graded_checksum);

    // Remove every adjustment layer: the original photo comes back
    // bit-for-bit. Nothing was ever baked in.
    reopened.layers.retain(|l| l.adjustment.is_none());
    assert_eq!(reopened.render_checksum(), original_checksum);
}

#[test]
fn adjustment_layer_mask_limits_the_effect() {
    let mut doc = photo();
    let idx = doc.add_adjustment_layer(
        "Brightness/Contrast",
        Adjustment::BrightnessContrast {
            brightness: 60,
            contrast: 0,
        },
    );
    // Hide the adjustment on the left half via its mask.
    for y in 0..32i32 {
        for x in 0..24i32 {
            doc.layers[idx].set_mask_value(x, y, 0);
        }
    }
    let base = photo();
    // Left half: untouched by the adjustment.
    assert_eq!(doc.render_pixel(6, 6), base.render_pixel(6, 6));
    // Right half: brightened.
    assert_ne!(doc.render_pixel(40, 6), base.render_pixel(40, 6));
    // Disabling the mask restores the effect everywhere.
    doc.layers[idx].mask_enabled = false;
    assert_ne!(doc.render_pixel(6, 6), base.render_pixel(6, 6));
}

#[test]
fn layer_mask_hides_reveals_and_feathers() {
    let mut doc = Document::new(32, 16);
    doc.layers[0].fill_rect(0, 0, 32, 16, Pixel16::from_rgba8(10, 10, 200, 255));
    let top = doc.add_layer("Top");
    doc.layers[top].fill_rect(0, 0, 32, 16, Pixel16::from_rgba8(220, 30, 30, 255));
    assert_eq!(doc.render_pixel(16, 8).to_rgba8(), (220, 30, 30, 255));

    // Hide-all mask: the layer vanishes, the one below shows.
    doc.layers[top].add_mask(0);
    assert_eq!(doc.render_pixel(16, 8).to_rgba8(), (10, 10, 200, 255));
    // Paint white on the right half of the mask: revealed there.
    for y in 0..16i32 {
        for x in 16..32i32 {
            doc.layers[top].set_mask_value(x, y, 255);
        }
    }
    assert_eq!(doc.render_pixel(24, 8).to_rgba8(), (220, 30, 30, 255));
    assert_eq!(doc.render_pixel(8, 8).to_rgba8(), (10, 10, 200, 255));
    // Feather the mask: the hard edge at x=16 becomes a blend.
    doc.feather_mask(top, 2);
    let mid = doc.render_pixel(16, 8).to_rgba8();
    assert!(
        mid.0 > 30 && mid.0 < 200,
        "feathered edge should blend the two layers, got {mid:?}"
    );
    // Mask survives the file round-trip.
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).expect("reopen");
    assert_eq!(back.render_checksum(), doc.render_checksum());
    assert!(back.layers[top].mask.is_some());
}

#[test]
fn adjustment_layer_agrees_with_destructive() {
    // Same numbers, two routes: a Brightness/Contrast adjustment
    // layer over the photo vs the destructive adjustment applied to
    // the photo's pixels. They share one math implementation.
    let mut layered = photo();
    layered.add_adjustment_layer(
        "Brightness/Contrast",
        Adjustment::BrightnessContrast {
            brightness: 30,
            contrast: 20,
        },
    );
    let mut baked = photo();
    adjust::brightness_contrast(&mut baked, 0, None, 30, 20);
    for (x, y) in [(0, 0), (10, 6), (36, 22), (47, 31)] {
        let a = layered.render_pixel(x, y).to_rgba8();
        let b = baked.render_pixel(x, y).to_rgba8();
        assert!(
            (a.0 as i32 - b.0 as i32).abs() <= 2
                && (a.1 as i32 - b.1 as i32).abs() <= 2
                && (a.2 as i32 - b.2 as i32).abs() <= 2,
            "adjustment layer {a:?} vs destructive {b:?} at ({x},{y})"
        );
    }
}

#[test]
fn curves_adjustment_layer_bends_tones() {
    let mut doc = photo();
    let before = doc.render_pixel(24, 16);
    doc.add_adjustment_layer(
        "Curves",
        Adjustment::Curves {
            points: vec![(0, 0), (128, 190), (255, 255)],
        },
    );
    let after = doc.render_pixel(24, 16);
    assert_ne!(before, after, "a lifted midtone curve must change midtones");
}

#[test]
fn guides_round_trip() {
    let mut doc = photo();
    doc.guides_x = vec![12, 30];
    doc.guides_y = vec![8];
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).expect("reopen");
    assert_eq!(back.guides_x, vec![12, 30]);
    assert_eq!(back.guides_y, vec![8]);
}

#[test]
fn merge_down_bakes_masks_and_adjustments_faithfully() {
    // Merging a masked layer down must not change a single pixel of
    // the composite — the merge bakes the pair's render instead of
    // dropping the mask (the old fast path's silent failure).
    let mut doc = photo();
    let top = doc.add_layer("Tint");
    doc.layers[top].fill_rect(0, 0, 48, 32, Pixel16::from_rgba8(20, 120, 120, 255));
    doc.layers[top].opacity = 0.6;
    doc.layers[top].add_mask(255);
    for y in 0..32i32 {
        for x in 24..48i32 {
            doc.layers[top].set_mask_value(x, y, 0);
        }
    }
    let before = doc.render_checksum();
    assert!(doc.merge_down(top));
    assert_eq!(doc.layers.len(), 1);
    assert!(
        doc.layers[0].mask.is_none(),
        "the mask is baked away, not kept"
    );
    assert_eq!(
        doc.render_checksum(),
        before,
        "masked merge must keep the composite identical"
    );

    // Same for an adjustment layer: merging it down bakes the grade
    // into the photo, pixel-identical to the live grade.
    let mut doc = photo();
    let adj = doc.add_adjustment_layer(
        "Photo Filter",
        Adjustment::PhotoFilter {
            color: (255, 178, 84),
            density: 40,
        },
    );
    let graded = doc.render_checksum();
    assert!(doc.merge_down(adj));
    assert_eq!(doc.layers.len(), 1);
    assert!(doc.layers[0].adjustment.is_none());
    assert_eq!(
        doc.render_checksum(),
        graded,
        "adjustment merge must keep the composite identical"
    );

    // And in a taller stack: merging the middle (masked) layer down
    // leaves the layer above untouched and the render unchanged.
    let mut doc = photo();
    let mid = doc.add_layer("Mid");
    doc.layers[mid].fill_rect(0, 0, 48, 32, Pixel16::from_rgba8(240, 200, 40, 255));
    doc.layers[mid].add_mask(0);
    for y in 0..16i32 {
        for x in 0..48i32 {
            doc.layers[mid].set_mask_value(x, y, 255);
        }
    }
    let cap = doc.add_layer("Cap");
    doc.layers[cap].fill_rect(0, 28, 48, 4, Pixel16::from_rgba8(30, 30, 30, 255));
    let stacked = doc.render_checksum();
    assert!(doc.merge_down(mid));
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.render_checksum(), stacked);
}

#[test]
fn old_files_still_load_without_phase4_data() {
    // A v3-era file (serialize, then the loader must cope) — the
    // Phase 4 section is optional trailing data; a document with no
    // masks/adjustments/guides writes it as empty and reads back
    // identically.
    let doc = photo();
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).expect("reopen");
    assert!(back
        .layers
        .iter()
        .all(|l| l.mask.is_none() && l.adjustment.is_none()));
    assert!(back.guides_x.is_empty() && back.guides_y.is_empty());
    assert_eq!(back.render_checksum(), doc.render_checksum());
}
