//! Phase 0 performance harness — not a test, a measuring stick.
//! Run with: cargo run --release --example bench -p pictodesk-core
//!
//! Builds an 8000x8000 (64MP) two-layer document and times the three
//! operations the whole product's feel depends on: painting, region
//! rendering (what the canvas does while you pan/zoom), and a full
//! flatten (what export does). Numbers go in the commit message / PR
//! when this changes; a regression here is a Phase-blocking bug.

use pictodesk_core::blend::BlendMode;
use pictodesk_core::document::Document;
use pictodesk_core::pixel::Pixel16;
use std::time::Instant;

fn ms(t: Instant) -> String {
    format!("{:>9.1} ms", t.elapsed().as_secs_f64() * 1000.0)
}

fn main() {
    let (w, h) = (8000u32, 8000u32);
    println!(
        "document: {w}x{h} = {} MP, 2 layers",
        (w as u64 * h as u64) / 1_000_000
    );

    let t = Instant::now();
    let mut doc = Document::new(w, h);
    doc.layers[0].fill_rect(0, 0, w, h, Pixel16::from_rgba8(52, 84, 120, 255));
    let top = doc.add_layer("Overlay");
    doc.layers[top].blend_mode = BlendMode::Overlay;
    doc.layers[top].opacity = 0.8;
    doc.layers[top].fill_rect(
        1000,
        1000,
        4000,
        3000,
        Pixel16::from_rgba8(220, 180, 60, 255),
    );
    println!("paint full + overlay: {}", ms(t));
    println!(
        "tiles allocated: bg={} top={}",
        doc.layers[0].tile_count(),
        doc.layers[1].tile_count()
    );

    let t = Instant::now();
    let region = doc.render_region_rgba8(2000, 1500, 1024, 1024);
    println!(
        "render 1024x1024 region: {} ({} bytes)",
        ms(t),
        region.len()
    );

    let t = Instant::now();
    let px = doc.render_pixel(3000, 2500);
    println!("single pixel composite: {:?} -> {:?}", ms(t), px.to_rgba8());

    let t = Instant::now();
    let bytes = pictodesk_core::format::serialize(&doc);
    println!("serialize .pdesk v2: {} ({} bytes)", ms(t), bytes.len());

    let t = Instant::now();
    let back = pictodesk_core::format::deserialize(&bytes).unwrap();
    println!("deserialize: {}", ms(t));
    assert_eq!(back.layers[0].tile_count(), doc.layers[0].tile_count());
    println!("BENCH DONE");
}
