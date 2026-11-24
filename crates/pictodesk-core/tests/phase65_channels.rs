//! Phase 6.5 Batch 3 core checks: planes, Apply Image,
//! Calculations math, Split/Merge Channels, Merge Group,
//! selection combine ops, Erase to History, Sample Merged.

use pictodesk_core::blend::BlendMode;
use pictodesk_core::brush::{stamp_clone_merged, stamp_erase_to_history, BrushTip};
use pictodesk_core::channels::{
    apply_image, blend_planes, blend_planes_op, merge_channels, merge_set, plane, split_channels,
    CalcOp, PlaneSource,
};
use pictodesk_core::document::{ColorMode, Document};
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::selection::Selection;

fn doc_rg() -> Document {
    let mut doc = Document::new(8, 4);
    doc.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(200, 40, 30, 255));
    doc.layers[0].fill_rect(4, 0, 4, 4, Pixel16::from_rgba8(30, 160, 60, 255));
    doc
}

#[test]
fn planes_extract_channels() {
    let doc = doc_rg();
    let red = plane(&doc, PlaneSource::CompositeChannel(0));
    assert_eq!(red[0], 200);
    assert_eq!(red[5], 30);
    let gray = plane(&doc, PlaneSource::CompositeGray);
    // Luma of (200,40,30) = 86.7; of (30,160,60) = 109.7.
    assert!((gray[0] as i32 - 87).abs() <= 1, "gray {}", gray[0]);
    assert!((gray[5] as i32 - 110).abs() <= 1, "gray {}", gray[5]);
    let layer_red = plane(&doc, PlaneSource::LayerChannel(0, 0));
    assert_eq!(layer_red[0], 200);
}

#[test]
fn calculations_multiply_and_add() {
    let a = vec![128u8; 4];
    let b = vec![128u8; 4];
    let m = blend_planes(&a, &b, BlendMode::Multiply, 1.0);
    assert_eq!(m[0], 64); // 0.5 * 0.5 = 0.25
    let add = blend_planes_op(&a, &b, CalcOp::Add, 1.0);
    assert!(add[0] >= 254, "add clamps: {}", add[0]);
    let sub = blend_planes_op(&a, &b, CalcOp::Subtract, 1.0);
    assert_eq!(sub[0], 255); // equal values + bias = white
    let scr = blend_planes(&a, &b, BlendMode::Screen, 1.0);
    assert!((scr[0] as i32 - 192).abs() <= 1, "screen {}", scr[0]);
    let half = blend_planes(&a, &b, BlendMode::Multiply, 0.5);
    assert_eq!(half[0], 96); // halfway between 128 and 64
}

#[test]
fn apply_image_multiply_darkens_and_mask_gates() {
    let mut doc = doc_rg();
    let src = vec![128u8; 32];
    apply_image(&mut doc, 0, &src, BlendMode::Multiply, 1.0, None, None);
    let p = doc.layers[0].get_pixel(1, 1).to_rgba8();
    assert_eq!(p.0, 100); // 200 * 0.5
                          // Mask of zeros = no effect anywhere.
    let mut doc2 = doc_rg();
    let zeros = vec![0u8; 32];
    apply_image(
        &mut doc2,
        0,
        &src,
        BlendMode::Multiply,
        1.0,
        Some((&zeros, false)),
        None,
    );
    assert_eq!(doc2.layers[0].get_pixel(1, 1).to_rgba8().0, 200);
    // Single-channel target: only red changes.
    let mut doc3 = doc_rg();
    apply_image(&mut doc3, 0, &src, BlendMode::Multiply, 1.0, None, Some(0));
    let p = doc3.layers[0].get_pixel(1, 1).to_rgba8();
    assert_eq!(p.0, 100);
    assert_eq!(p.1, 40);
}

#[test]
fn split_merge_rgb_round_trip() {
    let doc = doc_rg();
    let parts = split_channels(&doc);
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0].0, "Red");
    let planes: Vec<&[u8]> = parts
        .iter()
        .map(|(_, d)| {
            let p = plane(d, PlaneSource::CompositeChannel(0));
            // Leak intentionally: test-scope shortcut to keep
            // borrows simple (few hundred bytes).
            &*Box::leak(p.into_boxed_slice())
        })
        .collect();
    let merged = merge_channels(ColorMode::Rgb, &planes, 8, 4);
    let p = merged.render_pixel(1, 1).to_rgba8();
    assert_eq!((p.0, p.1, p.2), (200, 40, 30));
    let p = merged.render_pixel(6, 1).to_rgba8();
    assert_eq!((p.0, p.1, p.2), (30, 160, 60));
}

#[test]
fn split_cmyk_uses_display_values() {
    let mut doc = doc_rg();
    pictodesk_core::mode::convert_mode(&mut doc, ColorMode::Cmyk);
    let parts = split_channels(&doc);
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0].0, "Cyan");
    // Round trip through merge returns the same render.
    let before = doc.render_pixel(1, 1).to_rgba8();
    let planes: Vec<Vec<u8>> = parts
        .iter()
        .map(|(_, d)| plane(d, PlaneSource::CompositeChannel(0)))
        .collect();
    let refs: Vec<&[u8]> = planes.iter().map(|p| p.as_slice()).collect();
    let merged = merge_channels(ColorMode::Cmyk, &refs, 8, 4);
    let after = merged.render_pixel(1, 1).to_rgba8();
    assert!(
        (before.0 as i32 - after.0 as i32).abs() <= 3
            && (before.1 as i32 - after.1 as i32).abs() <= 3,
        "cmyk round trip {before:?} -> {after:?}"
    );
}

#[test]
fn merge_set_flattens_members() {
    let mut doc = Document::new(8, 4);
    doc.layers[0].fill_rect(0, 0, 8, 4, Pixel16::from_rgba8(10, 10, 10, 255));
    doc.add_layer("Base");
    doc.layers[1].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(200, 0, 0, 255));
    doc.add_layer("Top");
    doc.layers[2].fill_rect(4, 0, 4, 4, Pixel16::from_rgba8(0, 0, 200, 255));
    doc.sets.push(pictodesk_core::document::LayerSet {
        name: "Pair".into(),
        opacity: 1.0,
        mode: None,
        visible: true,
        color: 0,
    });
    doc.layers[1].group = 1;
    doc.layers[2].group = 1;
    merge_set(&mut doc, 1);
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[1].name, "Pair");
    assert_eq!(doc.sets.len(), 0);
    let p = doc.render_pixel(1, 1).to_rgba8();
    assert_eq!((p.0, p.1, p.2), (200, 0, 0));
    let p = doc.render_pixel(6, 1).to_rgba8();
    assert_eq!((p.0, p.1, p.2), (0, 0, 200));
}

#[test]
fn selection_combine_ops() {
    let mut a = Selection::rect(8, 4, 0, 0, 4, 4);
    let b = Selection::rect(8, 4, 2, 0, 4, 4);
    let mut u = a.clone();
    u.union_with(&b);
    assert_eq!(u.value(5, 1), 255);
    assert_eq!(u.value(1, 1), 255);
    let mut s = a.clone();
    s.subtract(&b);
    assert_eq!(s.value(1, 1), 255);
    assert_eq!(s.value(3, 1), 0);
    a.intersect(&b);
    assert_eq!(a.value(3, 1), 255);
    assert_eq!(a.value(1, 1), 0);
}

#[test]
fn erase_to_history_restores_source_pixels() {
    let mut src = Document::new(8, 4);
    src.layers[0].fill_rect(0, 0, 8, 4, Pixel16::from_rgba8(200, 40, 30, 255));
    let mut doc = src.clone();
    doc.layers[0].fill_rect(0, 0, 8, 4, Pixel16::from_rgba8(0, 0, 0, 0));
    let tip = BrushTip::new(3.0, 1.0);
    stamp_erase_to_history(&mut doc, 0, None, 2.0, 2.0, &tip, 1.0, &src);
    let p = doc.layers[0].get_pixel(2, 2).to_rgba8();
    assert_eq!((p.0, p.3), (200, 255));
}

#[test]
fn clone_merged_samples_the_composite() {
    let mut doc = Document::new(16, 4);
    doc.layers[0].fill_rect(0, 0, 16, 4, Pixel16::from_rgba8(10, 10, 10, 255));
    doc.add_layer("Paint");
    doc.layers[1].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(220, 30, 30, 255));
    // Active layer is 1 (has the red); stamp onto layer 0 from
    // a spot where only the composite shows red — layer 0's own
    // pixel there is dark, the merged sample is red.
    let tip = BrushTip::new(2.0, 1.0);
    stamp_clone_merged(&mut doc, 0, None, 10.0, 2.0, (2.0, 2.0), &tip, 1.0);
    let p = doc.layers[0].get_pixel(10, 2).to_rgba8();
    assert_eq!(p.0, 220, "merged clone paints composite red: {p:?}");
}
