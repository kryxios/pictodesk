//! Real CMYK storage (v0.2.1): painting, erasing, adjusting,
//! transforming and exporting all operate on stored ink values.

use pictodesk_core::adjust;
use pictodesk_core::brush::{self, BrushTip};
use pictodesk_core::document::{ColorMode, Document};
use pictodesk_core::format;
use pictodesk_core::history::{run_tile_edit, History};
use pictodesk_core::io;
use pictodesk_core::mode::convert_mode;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::transform;

fn cmyk_doc(w: u32, h: u32) -> Document {
    let mut doc = Document::new(w, h);
    doc.layers[0].fill_rect(0, 0, w, h, Pixel16::from_rgba8(255, 255, 255, 255));
    convert_mode(&mut doc, ColorMode::Cmyk);
    doc
}

#[test]
fn painting_deposits_real_inks() {
    let mut doc = cmyk_doc(40, 40);
    let tip = BrushTip::new(10.0, 1.0);
    let red = Pixel16::from_rgba8(255, 0, 0, 255);
    brush::stamp_brush(&mut doc, 0, None, 20.0, 20.0, red, &tip, 1.0);
    let (c, m, y, k, a) = doc.get_cmyk(0, 20, 20);
    assert_eq!(a, u16::MAX);
    assert!(c < 2000 && k < 2000, "red needs no C or K: c={c} k={k}");
    assert!(m > 60000 && y > 60000, "red is M+Y: m={m} y={y}");
}

#[test]
fn eraser_reduces_alpha_plane_and_undo_restores() {
    let mut doc = cmyk_doc(40, 40);
    let mut hist = History::new();
    hist.set_limit(Some(10));
    let tip = BrushTip::new(12.0, 1.0);
    run_tile_edit(&mut doc, &mut hist, "Eraser", 0, &[(0, 0)], |d| {
        brush::stamp_eraser(d, 0, None, 20.0, 20.0, &tip, 1.0);
    });
    let (_, _, _, _, a) = doc.get_cmyk(0, 20, 20);
    assert!(a < 1000, "erased center alpha {a}");
    hist.undo(&mut doc);
    let (_, _, _, _, a) = doc.get_cmyk(0, 20, 20);
    assert_eq!(a, u16::MAX, "undo must restore the alpha plane");
}

#[test]
fn adjustments_operate_on_inks() {
    let mut doc = cmyk_doc(10, 10);
    // Mid gray in CMYK: some K. Invert must flip every ink channel.
    doc.set_cmyk(0, 5, 5, (10000, 20000, 30000, 40000, u16::MAX));
    adjust::invert(&mut doc, 0, None);
    let v = doc.get_cmyk(0, 5, 5);
    assert_eq!(
        (v.0, v.1, v.2, v.3, v.4),
        (55535, 45535, 35535, 25535, u16::MAX)
    );
    // Threshold: this pixel's proof is dark -> K-only black.
    adjust::threshold(&mut doc, 0, None, 200);
    let v = doc.get_cmyk(0, 5, 5);
    assert_eq!((v.0, v.1, v.2), (0, 0, 0));
    assert!(v.3 > 60000, "threshold black is K: {}", v.3);
}

#[test]
fn flip_preserves_white_ink_pixel() {
    let mut doc = cmyk_doc(8, 2);
    // Zero inks + full alpha: a painted "white" pixel. Tile slots
    // read as TRANSPARENT, so transforms must consult the alpha
    // plane or this pixel vanishes.
    doc.set_cmyk(0, 0, 0, (0, 0, 0, 0, u16::MAX));
    doc.set_cmyk(0, 7, 0, (0, 0, 0, 30000, u16::MAX));
    transform::flip_horizontal(&mut doc);
    let v = doc.get_cmyk(0, 7, 0);
    assert_eq!(v.4, u16::MAX, "white-ink pixel survived the flip");
    let v2 = doc.get_cmyk(0, 0, 0);
    assert_eq!(v2.3, 30000, "K pixel moved to the other side");
}

#[test]
fn pdk_round_trip_keeps_alpha_plane() {
    let mut doc = cmyk_doc(30, 30);
    let tip = BrushTip::new(10.0, 1.0);
    brush::stamp_eraser(&mut doc, 0, None, 15.0, 15.0, &tip, 1.0);
    let before = doc.get_cmyk(0, 15, 15);
    assert!(before.4 < u16::MAX);
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).unwrap();
    assert_eq!(back.mode, ColorMode::Cmyk);
    assert_eq!(back.get_cmyk(0, 15, 15), before);
    assert_eq!(back.render_checksum(), doc.render_checksum());
}

#[test]
fn tiff_export_writes_stored_inks_and_spot_plate() {
    let mut doc = cmyk_doc(6, 4);
    doc.set_cmyk(0, 2, 1, (1000, 2000, 3000, 4000, u16::MAX));
    let mut spot = pictodesk_core::document::AlphaChannel::new("Spot Red", 6, 4);
    spot.spot_color = Some((200, 30, 30));
    spot.set(6, 2, 1, 255);
    spot.set(6, 0, 0, 128);
    doc.alphas.push(spot);
    let tiff = io::export_tiff_cmyk(&doc);
    // Photometric 5, 5 inks named in InkNames.
    let text = String::from_utf8_lossy(&tiff);
    assert!(text.contains("Spot Red"), "spot ink named in the TIFF");
    assert!(text.contains("Cyan\0Magenta\0Yellow\0Black\0"));
    // Find the strip via StripOffsets tag (273) and check samples.
    let rd16 = |o: usize| u16::from_le_bytes(tiff[o..o + 2].try_into().unwrap());
    let rd32 = |o: usize| u32::from_le_bytes(tiff[o..o + 4].try_into().unwrap());
    let n = rd16(8) as usize;
    let mut strip_off = 0u32;
    let mut spp = 0u32;
    for i in 0..n {
        let e = 10 + i * 12;
        match rd16(e) {
            273 => strip_off = rd32(e + 8),
            277 => spp = rd32(e + 8),
            _ => {}
        }
    }
    assert_eq!(spp, 5, "CMYK + one spot plate");
    let px = strip_off as usize + (6 + 2) * 5;
    let d = |v: u16| ((v as u32 + 128) / 257) as u8;
    assert_eq!(
        (
            tiff[px],
            tiff[px + 1],
            tiff[px + 2],
            tiff[px + 3],
            tiff[px + 4]
        ),
        (d(1000), d(2000), d(3000), d(4000), 255),
        "stored inks + spot value, byte for byte"
    );
    let px0 = strip_off as usize;
    assert_eq!(tiff[px0 + 4], 128, "spot plate value at (0,0)");
}

#[test]
fn tiff_clipping_path_irb_decodes_to_original_path() {
    // Strict independent decode of the Photoshop IRB in the exported
    // TIFF: resource ids, Pascal name, and every knot's 8.24
    // fixed-point control points must reproduce the source path.
    let mut doc = cmyk_doc(120, 90);
    let mut path = pictodesk_core::path::Path::new("Product outline");
    path.subpaths.push(pictodesk_core::path::Subpath {
        anchors: vec![
            pictodesk_core::path::Anchor::smooth((30.0, 20.0), (30.0, 20.0), (48.0, 20.0)),
            pictodesk_core::path::Anchor::smooth((90.0, 20.0), (72.0, 20.0), (90.0, 20.0)),
            pictodesk_core::path::Anchor::smooth((90.0, 70.0), (90.0, 70.0), (72.0, 70.0)),
            pictodesk_core::path::Anchor::smooth((30.0, 70.0), (48.0, 70.0), (30.0, 70.0)),
        ],
        closed: true,
    });
    doc.paths.push(path);
    doc.clipping_path = Some(0);
    let tiff = io::export_tiff_cmyk(&doc);

    // Locate tag 34377 in the IFD.
    let rd16 = |b: &[u8], o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    let rd32 = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let n = rd16(&tiff, 8) as usize;
    let (mut irb_off, mut irb_len) = (0usize, 0usize);
    for i in 0..n {
        let e = 10 + i * 12;
        if rd16(&tiff, e) == 34377 {
            irb_len = rd32(&tiff, e + 4) as usize;
            irb_off = rd32(&tiff, e + 8) as usize;
        }
    }
    assert!(irb_len > 0, "IRB tag present");
    let irb = &tiff[irb_off..irb_off + irb_len];

    // Walk the resource blocks.
    let be16 = |b: &[u8], o: usize| u16::from_be_bytes(b[o..o + 2].try_into().unwrap());
    let be32 = |b: &[u8], o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    let mut pos = 0usize;
    let mut path_data: &[u8] = &[];
    let mut clip_name = String::new();
    while pos + 8 <= irb.len() {
        assert_eq!(&irb[pos..pos + 4], b"8BIM");
        let id = be16(irb, pos + 4);
        let name_len = irb[pos + 6] as usize;
        let mut p = pos + 7 + name_len;
        if (1 + name_len) % 2 == 1 {
            p += 1;
        }
        let size = be32(irb, p) as usize;
        let payload = &irb[p + 4..p + 4 + size];
        match id {
            2000 => path_data = payload,
            2999 => {
                let l = payload[0] as usize;
                clip_name = String::from_utf8(payload[1..1 + l].to_vec()).unwrap();
            }
            other => panic!("unexpected resource id {other}"),
        }
        pos = p + 4 + size + (size % 2);
    }
    assert_eq!(clip_name, "Product outline");

    // Decode knots: length record, then 26-byte knot records with
    // (vertical, horizontal) 8.24 fixed-point pairs per point.
    let knots = be16(path_data, 0) as usize;
    assert_eq!(knots, 4);
    let fx = |b: &[u8], o: usize| -> f32 {
        i32::from_be_bytes(b[o..o + 4].try_into().unwrap()) as f32 / 16_777_216.0
    };
    let orig = &doc.paths[0].subpaths[0].anchors;
    for (i, a) in orig.iter().enumerate() {
        let rec = 26 + i * 26;
        assert_eq!(path_data[rec], 2, "closed-subpath knot selector");
        let pts = [
            (
                fx(path_data, rec + 6) * 120.0,
                fx(path_data, rec + 2) * 90.0,
            ),
            (
                fx(path_data, rec + 14) * 120.0,
                fx(path_data, rec + 10) * 90.0,
            ),
            (
                fx(path_data, rec + 22) * 120.0,
                fx(path_data, rec + 18) * 90.0,
            ),
        ];
        // Records store (in_ctrl, anchor, out_ctrl) as (x, y) pairs.
        let want = [a.in_ctrl, a.point, a.out_ctrl];
        for j in 0..3 {
            assert!(
                (pts[j].0 - want[j].0).abs() < 0.01 && (pts[j].1 - want[j].1).abs() < 0.01,
                "knot {i} point {j}: got {:?}, want {:?}",
                pts[j],
                want[j]
            );
        }
    }
}
