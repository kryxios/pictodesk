//! Phase 3: layer stack operations, component blend modes, free
//! transform, PSD import v1, and the phase task test (a composite
//! built from separate "photos" plus a title layer).

use pictodesk_core::blend::BlendMode;
use pictodesk_core::document::{ColorMode, Document};
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::transform;

fn fill_layer(doc: &mut Document, li: usize, x0: i32, y0: i32, w: i32, h: i32, px: Pixel16) {
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            doc.layers[li].set_pixel(x, y, px);
        }
    }
}

#[test]
fn layer_stack_ops() {
    let mut doc = Document::new(10, 10);
    fill_layer(
        &mut doc,
        0,
        0,
        0,
        10,
        10,
        Pixel16::from_rgba8(255, 0, 0, 255),
    );
    let top = doc.add_layer("Blue");
    fill_layer(
        &mut doc,
        top,
        0,
        0,
        10,
        10,
        Pixel16::from_rgba8(0, 0, 255, 255),
    );
    assert_eq!(doc.render_pixel(5, 5).to_rgba8().2, 255); // blue on top
                                                          // Reorder: blue goes to the bottom.
    doc.move_layer(top, 0);
    assert_eq!(doc.render_pixel(5, 5).to_rgba8().0, 255);
    // Duplicate the (now top) red layer and hide the original.
    let dup = doc.duplicate_layer(1).unwrap();
    assert_eq!(doc.layers.len(), 3);
    doc.layers[1].visible = false;
    assert_eq!(doc.render_pixel(5, 5).to_rgba8().0, 255); // dup still red
                                                          // Delete refuses on a single layer.
    assert!(doc.delete_layer(dup));
    assert!(doc.delete_layer(0));
    assert!(!doc.delete_layer(0));
}

#[test]
fn merge_down_and_flatten_respect_blends() {
    let mut doc = Document::new(8, 8);
    fill_layer(
        &mut doc,
        0,
        0,
        0,
        8,
        8,
        Pixel16::from_rgba8(200, 200, 200, 255),
    );
    let top = doc.add_layer("Shade");
    fill_layer(
        &mut doc,
        top,
        0,
        0,
        8,
        8,
        Pixel16::from_rgba8(128, 128, 128, 255),
    );
    doc.layers[top].blend_mode = BlendMode::Multiply;
    assert!(doc.merge_down(top));
    assert_eq!(doc.layers.len(), 1);
    let v = doc.render_pixel(4, 4).to_rgba8().0;
    assert!(
        (98..=104).contains(&v),
        "multiply merge should be ~100, got {v}"
    );

    // Flatten with a hidden layer: hidden content is discarded.
    let ghost = doc.add_layer("Ghost");
    fill_layer(
        &mut doc,
        ghost,
        0,
        0,
        8,
        8,
        Pixel16::from_rgba8(0, 255, 0, 255),
    );
    doc.layers[ghost].visible = false;
    doc.flatten();
    assert_eq!(doc.layers.len(), 1);
    assert_eq!(
        doc.render_pixel(4, 4).to_rgba8().1,
        doc.render_pixel(4, 4).to_rgba8().0
    );
}

#[test]
fn component_blend_modes_behave() {
    let red = Pixel16::from_rgba8(255, 0, 0, 255);
    let green = Pixel16::from_rgba8(0, 255, 0, 255);
    // Luminosity: green over red takes red's brightness, green's hue.
    let mut doc = Document::new(4, 4);
    fill_layer(&mut doc, 0, 0, 0, 4, 4, red);
    let t = doc.add_layer("g");
    fill_layer(&mut doc, t, 0, 0, 4, 4, green);
    doc.layers[t].blend_mode = BlendMode::Luminosity;
    let out = doc.render_pixel(2, 2).to_rgba8();
    // Luminosity keeps the backdrop's hue (red) at the source's
    // lightness: lum(green)=0.59 lifts red to (255, 106, 106).
    assert!(
        out.0 == 255 && (95..=118).contains(&out.1),
        "luminosity result: {out:?}"
    );
    // Color mode: red's hue/sat at green's luminosity — still red.
    doc.layers[t].blend_mode = BlendMode::Color;
    doc.layers[0].set_pixel(2, 2, green);
    doc.layers[t].set_pixel(2, 2, red);
    let out2 = doc.render_pixel(2, 2).to_rgba8();
    // Color = source hue/sat (red) at backdrop luminosity (0.59).
    assert!(
        out2.0 == 255 && (95..=118).contains(&out2.1),
        "color result: {out2:?}"
    );
}

#[test]
fn free_transform_scales_and_translates() {
    let mut doc = Document::new(40, 40);
    fill_layer(
        &mut doc,
        0,
        10,
        10,
        10,
        10,
        Pixel16::from_rgba8(255, 0, 0, 255),
    );
    // Translate by (5, -3).
    transform::translate_layer(&mut doc, 0, 5, -3);
    assert_eq!(doc.layers[0].get_pixel(15, 7).a, 65535);
    assert_eq!(doc.layers[0].get_pixel(10, 10).a, 0);
    // Scale 200% about the content center (20, 12): bounds double.
    transform::transform_layer(&mut doc, 0, 2.0, 2.0, 0.0, 0.0);
    assert_eq!(doc.layers[0].get_pixel(12, 5).a, 65535); // grew up-left
    assert_eq!(doc.layers[0].get_pixel(5, 5).a, 0); // not that far
                                                    // Rotate 90 degrees: a square stays a square (smoke check that
                                                    // content survives rotation).
    transform::transform_layer(&mut doc, 0, 1.0, 1.0, 90.0, 0.0);
    assert_eq!(doc.layers[0].get_pixel(20, 12).a, 65535);
}

#[test]
fn phase3_task_test_three_photos_and_a_title() {
    // Three "photo" documents composited into one image, plus a
    // title bar layer, then flattened — the Phase 3 task test.
    let mut doc = Document::new(60, 40);
    // Sky photo (bottom layer).
    for y in 0..40 {
        for x in 0..60 {
            let v = 180 - y * 2;
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(120, 160, v as u8, 255));
        }
    }
    // Mountain photo pasted as a layer (opaque triangle-ish block).
    let mtn = doc.add_layer("Mountains");
    for y in 18..40 {
        for x in 0..60 {
            if (x - 30i32).abs() < (y - 17) * 2 {
                doc.layers[mtn].set_pixel(x, y, Pixel16::from_rgba8(90, 80, 70, 255));
            }
        }
    }
    // Sun photo as a Screen layer.
    let sun = doc.add_layer("Sun");
    for y in 4..14 {
        for x in 44..54 {
            if (x - 49) * (x - 49) + (y - 9) * (y - 9) < 25 {
                doc.layers[sun].set_pixel(x, y, Pixel16::from_rgba8(255, 220, 120, 255));
            }
        }
    }
    doc.layers[sun].blend_mode = BlendMode::Screen;
    // Title bar.
    let title = doc.add_layer("Title");
    fill_layer(
        &mut doc,
        title,
        0,
        0,
        60,
        7,
        Pixel16::from_rgba8(20, 20, 30, 255),
    );
    doc.layers[title].opacity = 0.85;

    let px = doc.render_pixel(49, 9);
    assert!(
        px.to_rgba8().0 > 200,
        "sun should screen bright: {:?}",
        px.to_rgba8()
    );
    assert!(
        doc.render_pixel(30, 35).to_rgba8().0 == 90,
        "mountain visible"
    );
    assert!(
        doc.render_pixel(30, 3).to_rgba8().2 < 120,
        "title bar darkens top"
    );
    doc.flatten();
    assert_eq!(doc.layers.len(), 1);
    assert!(doc.render_pixel(49, 9).to_rgba8().0 > 200);
    // The flattened document survives a .pdk round trip.
    let bytes = pictodesk_core::format::serialize(&doc);
    let back = pictodesk_core::format::deserialize(&bytes).unwrap();
    assert_eq!(back.render_pixel(49, 9), doc.render_pixel(49, 9));
    assert_eq!(back.mode, ColorMode::Rgb);
}

// ---------------------------------------------------------- PSD tests

struct PsdBuilder {
    buf: Vec<u8>,
}

impl PsdBuilder {
    fn new(w: u32, h: u32, mode: u16) -> Self {
        let mut b = PsdBuilder { buf: Vec::new() };
        b.buf.extend_from_slice(b"8BPS");
        b.buf.extend_from_slice(&1u16.to_be_bytes()); // version
        b.buf.extend_from_slice(&[0; 6]); // reserved
        b.buf.extend_from_slice(&3u16.to_be_bytes()); // channels
        b.buf.extend_from_slice(&h.to_be_bytes());
        b.buf.extend_from_slice(&w.to_be_bytes());
        b.buf.extend_from_slice(&8u16.to_be_bytes()); // depth
        b.buf.extend_from_slice(&mode.to_be_bytes());
        b.buf.extend_from_slice(&0u32.to_be_bytes()); // color mode data
        b.buf.extend_from_slice(&0u32.to_be_bytes()); // image resources
        b
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
}

/// Build a two-layer 4x2 PSD (raw compression): bottom "Base" solid
/// red; top "Shade" solid black at 50% opacity, Multiply.
fn sample_psd() -> Vec<u8> {
    let mut b = PsdBuilder::new(4, 2, 3);
    // Layer records are stored TOP first.
    let layer_info_len_placeholder = b.buf.len();
    b.u32(0); // section length (patched below)
    let section_start = b.buf.len();
    b.u32(0); // layer info length (patched below)
    let info_start = b.buf.len();
    b.buf.extend_from_slice(&2i16.to_be_bytes()); // 2 layers

    let record = |b: &mut PsdBuilder, name: &str, key: &[u8; 4], opacity: u8, npx: usize| {
        b.u32(0); // top
        b.u32(0); // left
        b.u32(2); // bottom
        b.u32(4); // right
        b.buf.extend_from_slice(&4u16.to_be_bytes()); // 4 channels
        for id in [-1i16, 0, 1, 2] {
            b.buf.extend_from_slice(&id.to_be_bytes());
            b.u32((2 + npx) as u32); // compression + data
        }
        b.buf.extend_from_slice(b"8BIM");
        b.buf.extend_from_slice(key);
        b.buf.push(opacity);
        b.buf.push(0); // clipping
        b.buf.push(0); // flags (visible)
        b.buf.push(0); // filler
                       // Extra data: mask(0) + ranges(0) + padded Pascal name.
        let name_field = 1 + name.len();
        let pad = (4 - name_field % 4) % 4;
        let extra = 4 + 4 + name_field + pad;
        b.u32(extra as u32);
        b.u32(0);
        b.u32(0);
        b.buf.push(name.len() as u8);
        b.buf.extend_from_slice(name.as_bytes());
        b.buf.extend(std::iter::repeat_n(0u8, pad));
    };
    record(&mut b, "Shade", b"mul ", 128, 8);
    record(&mut b, "Base", b"norm", 255, 8);
    // Channel data in record order: Shade (black, alpha 255), Base (red).
    let chan = |b: &mut PsdBuilder, fill: u8| {
        b.buf.extend_from_slice(&0u16.to_be_bytes()); // raw
        b.buf.extend(std::iter::repeat_n(fill, 8));
    };
    chan(&mut b, 255); // shade alpha
    chan(&mut b, 0); // shade R
    chan(&mut b, 0); // shade G
    chan(&mut b, 0); // shade B
    chan(&mut b, 255); // base alpha
    chan(&mut b, 255); // base R
    chan(&mut b, 0); // base G
    chan(&mut b, 0); // base B
                     // Global mask info word (the parser stops before it).
    b.u32(0);
    let info_len = b.buf.len() - info_start;
    let section_len = b.buf.len() - section_start;
    // Patch the two length words: section length at the placeholder,
    // layer-info length at section_start.
    b.buf[layer_info_len_placeholder..layer_info_len_placeholder + 4]
        .copy_from_slice(&(section_len as u32).to_be_bytes());
    b.buf[section_start..section_start + 4].copy_from_slice(&(info_len as u32).to_be_bytes());
    b.buf
}

#[test]
fn psd_import_two_layers() {
    let bytes = sample_psd();
    let (doc, report) = pictodesk_core::psd::load_psd(&bytes).expect("PSD should import");
    assert_eq!(report.layers_imported, 2);
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[0].name, "Base");
    assert_eq!(doc.layers[1].name, "Shade");
    assert_eq!(doc.layers[1].blend_mode, BlendMode::Multiply);
    assert!((doc.layers[1].opacity - 0.502).abs() < 0.01);
    // Black multiply at 50% over red: red halved.
    let px = doc.render_pixel(1, 1).to_rgba8();
    assert!((120..=135).contains(&px.0), "expected ~127 red, got {px:?}");
    assert_eq!(px.1, 0);
}

#[test]
fn psd_rejects_non_psd() {
    assert!(pictodesk_core::psd::load_psd(b"not a psd at all").is_err());
}
