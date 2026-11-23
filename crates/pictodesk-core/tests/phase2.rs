//! Phase 2 task test, from the plan: "cut a product cleanly out
//! with the Pen Tool and place it on a pure-white catalog
//! background, exported in CMYK TIFF."
//!
//! Plus the phase's supporting guarantees: selections save to alpha
//! channels and back, Grow/Similar behave, the .pdesk v3 format
//! round-trips paths/channels/mode, and 16-bit files keep their
//! precision end to end.

use pictodesk_core::brush::fill_selection;
use pictodesk_core::document::ColorMode;
use pictodesk_core::io::{export_tiff_cmyk, import_image};
use pictodesk_core::path::{Anchor, Path, Subpath};
use pictodesk_core::{format, mode, Document, Pixel16, Selection};

const W: u32 = 240;
const H: u32 = 180;
const CX: f32 = 120.0;
const CY: f32 = 90.0;
const RX: f32 = 70.0;
const RY: f32 = 45.0;

fn product_shot() -> Document {
    let mut doc = Document::new(W, H);
    // Dull blue-gray studio backdrop.
    doc.layers[0].fill_rect(0, 0, W, H, Pixel16::from_rgba8(96, 104, 116, 255));
    // The product: a red ellipse.
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let d = ((x as f32 - CX) / RX).powi(2) + ((y as f32 - CY) / RY).powi(2);
            if d <= 1.0 {
                doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(198, 42, 38, 255));
            }
        }
    }
    doc
}

/// The pen path a user would draw around the product: four smooth
/// anchors with proper Bézier handles.
fn product_path() -> Path {
    let k = 0.5523f32;
    let mut p = Path::new("Product");
    p.subpaths.push(Subpath {
        closed: true,
        anchors: vec![
            Anchor::smooth(
                (CX + RX, CY),
                (CX + RX, CY - RY * k),
                (CX + RX, CY + RY * k),
            ),
            Anchor::smooth(
                (CX, CY + RY),
                (CX + RX * k, CY + RY),
                (CX - RX * k, CY + RY),
            ),
            Anchor::smooth(
                (CX - RX, CY),
                (CX - RX, CY + RY * k),
                (CX - RX, CY - RY * k),
            ),
            Anchor::smooth(
                (CX, CY - RY),
                (CX - RX * k, CY - RY),
                (CX + RX * k, CY - RY),
            ),
        ],
    });
    p
}

fn tiff_u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(b[off..off + 2].try_into().unwrap())
}
fn tiff_u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
/// Find a tag in our own TIFF and return (type, count, value/offset).
fn tiff_tag(b: &[u8], want: u16) -> Option<(u16, u32, u32)> {
    let ifd = tiff_u32le(b, 4) as usize;
    let n = tiff_u16le(b, ifd) as usize;
    for i in 0..n {
        let e = ifd + 2 + i * 12;
        if tiff_u16le(b, e) == want {
            return Some((
                tiff_u16le(b, e + 2),
                tiff_u32le(b, e + 4),
                tiff_u32le(b, e + 8),
            ));
        }
    }
    None
}

#[test]
fn pen_cutout_on_white_exported_as_cmyk_tiff() {
    let mut doc = product_shot();
    let path = product_path();

    // Pen path -> selection -> invert -> fill white.
    let mut outside = path.to_selection(W, H);
    assert_eq!(outside.value(120, 90), 255, "product interior selected");
    outside.invert();
    fill_selection(
        &mut doc,
        0,
        &outside,
        Pixel16::from_rgba8(255, 255, 255, 255),
    );
    let corner = doc.render_pixel(4, 4).to_rgba8();
    assert_eq!(corner, (255, 255, 255, 255), "catalog white background");
    let center = doc.render_pixel(120, 90).to_rgba8();
    assert_eq!(center, (198, 42, 38, 255), "product untouched");

    // Save the cutout as an alpha channel; make the path the
    // clipping path; convert to CMYK; export.
    let inside = path.to_selection(W, H);
    let ch = doc.save_selection_as_channel(&inside);
    assert_eq!(doc.alphas[ch].name, "Alpha 1");
    let loaded = doc.channel_as_selection(ch).unwrap();
    assert_eq!(loaded.value(120, 90), 255);
    assert_eq!(loaded.value(4, 4), 0);

    doc.paths.push(path);
    doc.clipping_path = Some(0);
    mode::convert_mode(&mut doc, ColorMode::Cmyk);
    assert_eq!(doc.mode, ColorMode::Cmyk);

    // .pdesk v3 round trip keeps mode, paths, channels, clipping.
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).unwrap();
    assert_eq!(back.mode, ColorMode::Cmyk);
    assert_eq!(back.paths.len(), 1);
    assert_eq!(back.paths[0].name, "Product");
    assert_eq!(back.clipping_path, Some(0));
    assert_eq!(back.alphas.len(), 1);
    assert_eq!(back.render_checksum(), doc.render_checksum());

    // CMYK TIFF: separations photometric, clipping path embedded.
    let tiff = export_tiff_cmyk(&doc);
    assert_eq!(&tiff[0..2], b"II");
    let (_, _, photo) = tiff_tag(&tiff, 262).expect("photometric tag");
    assert_eq!(photo, 5, "CMYK separations");
    let (_, count, off) = tiff_tag(&tiff, 34377).expect("photoshop resource tag");
    let irb = &tiff[off as usize..(off + count) as usize];
    assert!(irb.windows(4).any(|w| w == b"8BIM"), "IRB magic present");
    // Resource 2999 (clipping path name) follows the path resource.
    assert!(
        irb.windows(2).any(|w| w == [0x0B, 0xB7]),
        "resource id 2999 present"
    );

    // Pixel checks in the strip: white corner -> all-zero CMYK;
    // red center -> low C, high M and Y, modest K.
    let (_, _, strip_off) = tiff_tag(&tiff, 273).unwrap();
    let px = |x: usize, y: usize| -> [u8; 4] {
        let i = strip_off as usize + (y * W as usize + x) * 4;
        tiff[i..i + 4].try_into().unwrap()
    };
    assert_eq!(px(4, 4), [0, 0, 0, 0], "white prints as no ink");
    let c = px(120, 90);
    assert!(c[0] < 30, "red has little cyan, got {}", c[0]);
    assert!(c[1] > 180 && c[2] > 180, "red is magenta+yellow, got {c:?}");
    assert!(c[3] > 20 && c[3] < 90, "red K is modest, got {}", c[3]);
}

#[test]
fn grow_and_similar_expand_by_color() {
    // 30x10: columns 0-9 color A, 10-19 color A' (6 levels off),
    // 20-29 color B (far away).
    let mut doc = Document::new(30, 10);
    doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(100, 100, 100, 255));
    doc.layers[0].fill_rect(10, 0, 10, 10, Pixel16::from_rgba8(106, 100, 100, 255));
    doc.layers[0].fill_rect(20, 0, 10, 10, Pixel16::from_rgba8(230, 40, 40, 255));
    let img = doc.render_full_rgba8();
    let seed = Selection::rect(30, 10, 2, 2, 4, 4);

    let similar = seed.similar(&img, 16);
    assert_eq!(similar.value(15, 5), 255, "similar finds A' anywhere");
    assert_eq!(similar.value(25, 5), 0, "similar rejects B");

    let grown = seed.grow(&img, 16);
    assert_eq!(grown.value(15, 5), 255, "grow crosses into A'");
    assert_eq!(grown.value(25, 5), 0, "grow stops at B");

    let strict = seed.grow(&img, 2);
    assert_eq!(strict.value(15, 5), 0, "tight tolerance stays in A");
    assert_eq!(strict.value(9, 5), 255, "but still fills A itself");
}

#[test]
fn sixteen_bit_png_survives_import_exactly() {
    // A 16-bit PNG whose values 8-bit would destroy.
    let mut buf = image::RgbaImage::new(4, 1);
    // image crate's RgbaImage is 8-bit; build the 16-bit buffer via
    // ImageBuffer directly.
    let mut buf16 = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::new(4, 1);
    buf16.put_pixel(0, 0, image::Rgba([0x1234, 0xABCD, 0x0001, 0xFFFF]));
    buf16.put_pixel(1, 0, image::Rgba([0xFF00, 0x00FF, 0x8080, 0xFFFF]));
    buf16.put_pixel(2, 0, image::Rgba([0, 0, 0, 0xFFFF]));
    buf16.put_pixel(3, 0, image::Rgba([0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF]));
    let mut bytes: Vec<u8> = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut bytes);
    use image::ImageEncoder;
    let raw8: Vec<u8> = buf16
        .as_raw()
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    enc.write_image(&raw8, 4, 1, image::ExtendedColorType::Rgba16)
        .unwrap();
    let _ = &mut buf;
    let doc = import_image(&bytes).unwrap();
    let p = doc.render_pixel(0, 0);
    assert_eq!(
        (p.r, p.g, p.b, p.a),
        (0x1234, 0xABCD, 0x0001, 0xFFFF),
        "16-bit values must arrive bit-exact"
    );
    // And through .pdesk too (the format is 16-bit native).
    let back = format::deserialize(&format::serialize(&doc)).unwrap();
    let p2 = back.render_pixel(1, 0);
    assert_eq!((p2.r, p2.g), (0xFF00, 0x00FF));
}
