//! Phase 0 exit-criteria demo, headless:
//! build a document through the tile engine + color pipeline, paint on
//! two layers with a blend mode, undo/redo, save `.pdk`, reload,
//! verify the render is pixel-identical, and export a PPM preview.
//!
//! Usage: pictodesk-cli [outdir]   (default: ./phase0-demo)

use pictodesk_core::blend::BlendMode;
use pictodesk_core::document::Document;
use pictodesk_core::format;
use pictodesk_core::history::{History, TileEditCommand};
use pictodesk_core::pixel::Pixel16;
use std::path::PathBuf;

fn main() {
    let outdir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "phase0-demo".to_string()),
    );
    std::fs::create_dir_all(&outdir).expect("create outdir");

    // 1. A 640x400 document, painted through tiles on two layers.
    let mut doc = Document::new(640, 400);
    let mut hist = History::new();

    // Background: vertical-ish gradient bands (painted per-pixel via tiles).
    {
        let mut cmd = TileEditCommand::new("Paint background");
        for coord in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)] {
            cmd.capture_before(&doc, 0, coord);
        }
        for y in 0..400i32 {
            let t = y as f32 / 399.0;
            let p = Pixel16::from_f32s(0.10 + 0.20 * t, 0.25, 0.55 - 0.30 * t, 1.0);
            doc.layers[0].fill_rect(0, y, 640, 1, p);
        }
        for coord in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)] {
            cmd.capture_after(&doc, 0, coord);
        }
        hist.push(Box::new(cmd));
    }

    // Top layer: a Screen-blended accent block.
    let top = doc.add_layer("Accent");
    doc.layers[top].blend_mode = BlendMode::Screen;
    doc.layers[top].fill_rect(220, 120, 200, 160, Pixel16::from_rgba8(180, 60, 30, 255));
    let painted = doc.render_checksum();
    println!(
        "layers: {} (active: {})",
        doc.layers.len(),
        doc.layers[doc.active_layer].name
    );
    println!(
        "tiles allocated: bg={} top={}",
        doc.layers[0].tile_count(),
        doc.layers[1].tile_count()
    );
    println!("render checksum after paint: {painted:#018x}");

    // 2. Undo removes the background paint; redo restores the exact render.
    let label = hist.undo(&mut doc).expect("undo works");
    println!("undo: {label} -> checksum {:#018x}", doc.render_checksum());
    hist.redo(&mut doc).expect("redo works");
    assert_eq!(
        doc.render_checksum(),
        painted,
        "redo must restore the render exactly"
    );
    println!("redo: checksum matches painted render ✓");

    // 3. Save .pdk, reload, renders must be identical (lossless format).
    let file = outdir.join("demo.pdk");
    format::save(&doc, &file).expect("save .pdk");
    let loaded = format::load(&file).expect("load .pdk");
    assert_eq!(
        loaded, doc,
        "round-tripped document must equal the original"
    );
    println!(
        "saved {} ({} bytes), reloaded, document identical ✓",
        file.display(),
        std::fs::metadata(&file).unwrap().len()
    );

    // 4. Atomic recovery save (crash-safe autosave path).
    let rec = format::recovery_path_for(&file);
    format::save_atomic(&doc, &rec).expect("atomic recovery save");
    println!("recovery file: {} ✓", rec.display());

    // 5. Export a PPM preview of the flattened render (display path).
    let ppm = outdir.join("demo.ppm");
    let rgba = doc.render_full_rgba8();
    let mut bytes = format!("P6\n{} {}\n255\n", doc.width, doc.height).into_bytes();
    for px in rgba.as_chunks::<4>().0 {
        bytes.extend_from_slice(&px[..3]);
    }
    std::fs::write(&ppm, bytes).expect("write ppm");
    println!("preview: {} ✓", ppm.display());
    println!("PHASE 0 DEMO: PASS");
}
