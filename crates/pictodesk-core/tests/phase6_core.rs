//! Phase 6 core checks: the ten-effect styles engine, advanced
//! blending, shape / fill layers, layer sets, clipping groups,
//! Gradient Map, and the .pdk Phase 6 round trip.

use pictodesk_core::adjust::Adjustment;
use pictodesk_core::blend::BlendMode;
use pictodesk_core::document::{Document, LayerSet, Note, SliceRect};
use pictodesk_core::effects::Effects;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::shapes::{FillSpec, ShapeSpec};
use pictodesk_core::transform::perspective_crop;

fn px(doc: &Document, x: i32, y: i32) -> (u8, u8, u8, u8) {
    doc.render_pixel(x, y).to_rgba8()
}

#[test]
fn stroke_outside_paints_a_ring() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(6, 6, 8, 8, Pixel16::from_rgba8(200, 0, 0, 255));
    let mut fx = Effects::default();
    fx.stroke.enabled = true;
    fx.stroke.size = 2;
    fx.stroke.position = 0;
    fx.stroke.color = Pixel16::from_rgba8(0, 0, 255, 255);
    doc.layers[0].effects = Some(fx);
    // Just outside the block: blue ring.
    assert_eq!(px(&doc, 5, 10).2, 255);
    assert_eq!(px(&doc, 5, 10).3, 255);
    // The block itself stays red.
    assert_eq!(px(&doc, 10, 10).0, 200);
}

#[test]
fn color_overlay_replaces_content_color() {
    let mut doc = Document::new(10, 10);
    doc.layers[0].fill_rect(2, 2, 5, 5, Pixel16::from_rgba8(200, 0, 0, 255));
    let mut fx = Effects::default();
    fx.color_overlay.enabled = true;
    fx.color_overlay.color = Pixel16::from_rgba8(0, 200, 0, 255);
    doc.layers[0].effects = Some(fx);
    let p = px(&doc, 4, 4);
    assert_eq!((p.0, p.1), (0, 200));
}

#[test]
fn satin_and_gradient_overlay_change_pixels() {
    let mut doc = Document::new(16, 16);
    doc.layers[0].fill_rect(2, 2, 12, 12, Pixel16::from_rgba8(160, 160, 160, 255));
    let before = doc.render_checksum();
    let mut fx = Effects::default();
    fx.satin.enabled = true;
    fx.gradient_overlay.enabled = true;
    doc.layers[0].effects = Some(fx);
    assert_ne!(doc.render_checksum(), before);
}

#[test]
fn shape_layer_renders_and_stays_editable() {
    let mut doc = Document::new(30, 30);
    let li = doc.add_layer("Shape");
    doc.layers[li].shape = Some(ShapeSpec {
        kind: 2, // ellipse
        rect: (5, 5, 20, 20),
        fill: Pixel16::from_rgba8(0, 180, 0, 255),
        ..Default::default()
    });
    assert_eq!(px(&doc, 15, 15).1, 180);
    assert_eq!(px(&doc, 2, 2).3, 0);
    // Edit the spec: the render follows.
    doc.layers[li].shape.as_mut().unwrap().fill = Pixel16::from_rgba8(0, 0, 220, 255);
    assert_eq!(px(&doc, 15, 15).2, 220);
}

#[test]
fn fill_layer_gradient_spans_the_canvas() {
    let mut doc = Document::new(20, 10);
    let li = doc.add_layer("Fill");
    doc.layers[li].fill = Some(FillSpec {
        kind: 1,
        color: Pixel16::from_rgba8(0, 0, 0, 255),
        to: Pixel16::from_rgba8(255, 255, 255, 255),
        angle: 0,
        ..Default::default()
    });
    assert!(px(&doc, 1, 5).0 < 40);
    assert!(px(&doc, 18, 5).0 > 200);
}

#[test]
fn clipping_group_clips_to_the_base_alpha() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(4, 4, 6, 6, Pixel16::from_rgba8(0, 0, 200, 255));
    let top = doc.add_layer("Clipped");
    doc.layers[top].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(220, 0, 0, 255));
    doc.layers[top].clipped = true;
    // Inside the base: red wins. Outside: nothing from the top.
    assert_eq!(px(&doc, 6, 6).0, 220);
    assert_eq!(px(&doc, 15, 15).3, 0);
}

#[test]
fn fill_opacity_leaves_the_shadow_alone() {
    let mut doc = Document::new(24, 24);
    doc.layers[0].fill_rect(6, 6, 8, 8, Pixel16::from_rgba8(200, 0, 0, 255));
    let mut fx = Effects::default();
    fx.drop_shadow.enabled = true;
    fx.drop_shadow.distance = 4;
    fx.drop_shadow.blur = 0;
    doc.layers[0].effects = Some(fx);
    // (14, 15) is shadow-only; (7, 7) is content-only (the
    // shadow falls down-right of the block).
    let shadow_before = px(&doc, 14, 15);
    assert!(shadow_before.3 > 0);
    doc.layers[0].fill_opacity = 0.0;
    // Content gone, shadow still there.
    assert_eq!(px(&doc, 7, 7).3, 0);
    assert_eq!(px(&doc, 14, 15), shadow_before);
}

#[test]
fn blend_if_hides_by_luminance() {
    let mut doc = Document::new(10, 10);
    doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(250, 250, 250, 255));
    doc.layers[0].blend_if = [200, 255, 0, 255]; // only dark content shows — none here
                                                 // Bright layer hides itself where its own luma < 200 fails:
                                                 // luma 250 is in [200,255], so it still shows. Tighten:
    doc.layers[0].blend_if = [0, 100, 0, 255];
    assert_eq!(px(&doc, 5, 5).3, 0);
}

#[test]
fn layer_set_opacity_and_visibility() {
    let mut doc = Document::new(10, 10);
    doc.sets.push(LayerSet {
        name: "Set 1".into(),
        opacity: 0.5,
        mode: None,
        visible: true,
        color: 0,
    });
    doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(200, 0, 0, 255));
    doc.layers[0].group = 1;
    assert_eq!(px(&doc, 5, 5).3, 128 - 1 + 1); // ~50% alpha (127 or 128)
    doc.sets[0].visible = false;
    assert_eq!(px(&doc, 5, 5).3, 0);
}

#[test]
fn gradient_map_adjustment_maps_luminance() {
    let adj = Adjustment::GradientMap {
        from: (0, 0, 0),
        to: (255, 255, 255),
        reverse: false,
    };
    let (r, g, b) = adj.apply(0.2, 0.2, 0.2);
    assert!((r - 0.2).abs() < 0.02 && (g - 0.2).abs() < 0.02 && (b - 0.2).abs() < 0.02);
    let adj2 = Adjustment::GradientMap {
        from: (255, 0, 0),
        to: (0, 0, 255),
        reverse: false,
    };
    let (r, _, b) = adj2.apply(1.0, 1.0, 1.0);
    assert!(b > 0.9 && r < 0.1);
    assert_eq!(adj.name(), "Gradient Map");
    let _ = BlendMode::Normal;
}

#[test]
fn phase6_format_round_trip() {
    let mut doc = Document::new(16, 16);
    doc.sets.push(LayerSet {
        name: "Hero".into(),
        opacity: 0.8,
        mode: None,
        visible: true,
        color: 4,
    });
    doc.notes.push(Note {
        x: 3,
        y: 4,
        color: 2,
        text: "fix this edge".into(),
        audio: Some(vec![1, 2, 3, 4]),
    });
    doc.slices.push(SliceRect {
        x: 0,
        y: 0,
        w: 8,
        h: 8,
        name: "hero".into(),
        url: "https://example.com".into(),
        alt: "Hero".into(),
    });
    let li = doc.add_layer("Shape");
    doc.layers[li].group = 1;
    doc.layers[li].clipped = true;
    doc.layers[li].locks = 5;
    doc.layers[li].color = 3;
    doc.layers[li].fill_opacity = 0.6;
    doc.layers[li].blend_if = [10, 240, 5, 250];
    doc.layers[li].shape = Some(ShapeSpec {
        kind: 0,
        rect: (1, 1, 9, 9),
        fill: Pixel16::from_rgba8(10, 20, 30, 255),
        ..Default::default()
    });
    let mut fx = Effects::default();
    fx.stroke.enabled = true;
    fx.stroke.size = 4;
    fx.drop_shadow.contour = 2;
    doc.layers[li].effects = Some(fx);
    doc.global_altitude = 45;

    let bytes = pictodesk_core::format::serialize(&doc);
    let back = pictodesk_core::format::deserialize(&bytes).expect("round trip");
    assert_eq!(back.sets.len(), 1);
    assert_eq!(back.sets[0].name, "Hero");
    assert_eq!(back.notes.len(), 1);
    assert_eq!(back.notes[0].text, "fix this edge");
    assert_eq!(back.notes[0].audio, Some(vec![1, 2, 3, 4]));
    assert_eq!(back.slices.len(), 1);
    assert_eq!(back.slices[0].url, "https://example.com");
    let l = &back.layers[li];
    assert_eq!(l.group, 1);
    assert!(l.clipped);
    assert_eq!(l.locks, 5);
    assert_eq!(l.color, 3);
    assert_eq!(l.blend_if, [10, 240, 5, 250]);
    assert!(l.shape.is_some());
    let fx = l.effects.unwrap();
    assert!(fx.stroke.enabled && fx.stroke.size == 4);
    assert_eq!(fx.drop_shadow.contour, 2);
    assert_eq!(back.global_altitude, 45);
    assert_eq!(back.render_checksum(), doc.render_checksum());
}

#[test]
fn perspective_crop_identity_quad_keeps_size() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(4, 4, 8, 8, Pixel16::from_rgba8(255, 0, 0, 255));
    let before = doc.render_checksum();
    assert!(perspective_crop(
        &mut doc,
        [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]
    ));
    assert_eq!((doc.width, doc.height), (20, 20));
    assert_eq!(doc.render_checksum(), before);
}

#[test]
fn perspective_crop_sub_quad_crops() {
    let mut doc = Document::new(40, 40);
    doc.layers[0].fill_rect(10, 10, 20, 20, Pixel16::from_rgba8(0, 255, 0, 255));
    assert!(perspective_crop(
        &mut doc,
        [(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)]
    ));
    assert_eq!((doc.width, doc.height), (20, 20));
    // The green block fills the whole cropped canvas.
    assert_eq!(doc.layers[0].get_pixel(10, 10).g, 65535);
}
