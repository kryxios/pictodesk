//! Perspective Crop on CMYK documents: the resample runs
//! through Document::get_cmyk / set_cmyk, interpolating all
//! five values (C, M, Y, K + alpha) per channel, instead of
//! refusing CMYK outright.

use pictodesk_core::document::{ColorMode, Document};
use pictodesk_core::mode::convert_mode;
use pictodesk_core::transform::perspective_crop;

const INK: (u16, u16, u16, u16, u16) = (10000, 20000, 30000, 40000, u16::MAX);

fn cmyk_doc_with_block(
    w: u32,
    h: u32,
    block: (i32, i32, i32, i32),
    ink: (u16, u16, u16, u16, u16),
) -> Document {
    let mut doc = Document::new(w, h);
    convert_mode(&mut doc, ColorMode::Cmyk);
    for y in block.1..block.3 {
        for x in block.0..block.2 {
            doc.set_cmyk(0, x, y, ink);
        }
    }
    doc
}

#[test]
fn perspective_crop_cmyk_identity_quad_preserves_inks() {
    let mut doc = cmyk_doc_with_block(20, 20, (4, 4, 12, 12), INK);
    assert!(perspective_crop(
        &mut doc,
        [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]
    ));
    assert_eq!((doc.width, doc.height), (20, 20));
    // Block corners and center keep their exact ink values.
    assert_eq!(doc.get_cmyk(0, 4, 4), INK);
    assert_eq!(doc.get_cmyk(0, 8, 8), INK);
    assert_eq!(doc.get_cmyk(0, 11, 11), INK);
}

#[test]
fn perspective_crop_cmyk_sub_quad_crops() {
    let ink = (5000, 60000, 0, 0, u16::MAX);
    let mut doc = cmyk_doc_with_block(40, 40, (10, 10, 30, 30), ink);
    assert!(perspective_crop(
        &mut doc,
        [(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)]
    ));
    assert_eq!((doc.width, doc.height), (20, 20));
    // The ink block filled the quad, so it fills the cropped
    // canvas: its old (10, 10) corner now sits at (0, 0).
    assert_eq!(doc.get_cmyk(0, 0, 0), ink);
    assert_eq!(doc.get_cmyk(0, 10, 10), ink);
    assert_eq!(doc.get_cmyk(0, 19, 19), ink);
}
