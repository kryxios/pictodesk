//! Phase 5 core tests: history states/snapshots, layer effects
//! rendering, channel mixer, type specs, format round-trip of the
//! new payload, PSD export round-trip, DCS export, quad warp.

use pictodesk_core::adjust::{channel_mixer, ChannelMixer};
use pictodesk_core::effects::{Effects, ShadowSpec};
use pictodesk_core::history::{History, Snapshot, TileEditCommand};
use pictodesk_core::text::{CharStyle, TextSpec};
use pictodesk_core::{Document, Pixel16};

fn red() -> Pixel16 {
    Pixel16::from_rgba8(255, 0, 0, 255)
}

fn paint(doc: &mut Document, hist: &mut History, label: &str, x: i32, y: i32, c: Pixel16) {
    let mut cmd = TileEditCommand::new(label);
    cmd.capture_before(doc, 0, (x.div_euclid(256), y.div_euclid(256)));
    doc.layers[0].set_pixel(x, y, c);
    cmd.capture_after(doc, 0, (x.div_euclid(256), y.div_euclid(256)));
    hist.push(Box::new(cmd));
}

#[test]
fn history_goto_and_labels() {
    let mut doc = Document::new(8, 8);
    let mut hist = History::new();
    paint(&mut doc, &mut hist, "Red", 1, 1, red());
    paint(
        &mut doc,
        &mut hist,
        "Green",
        2,
        2,
        Pixel16::from_rgba8(0, 255, 0, 255),
    );
    assert_eq!(hist.state_count(), 2);
    assert_eq!(hist.labels(), vec!["Red".to_string(), "Green".to_string()]);
    assert!(hist.goto_state(&mut doc, 1));
    assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().0, 255);
    assert_eq!(doc.layers[0].get_pixel(2, 2).to_rgba8().1, 0);
    assert!(hist.goto_state(&mut doc, 0));
    assert_eq!(doc.layers[0].get_pixel(1, 1).to_rgba8().3, 0);
    assert!(hist.goto_state(&mut doc, 2));
    assert_eq!(doc.layers[0].get_pixel(2, 2).to_rgba8().1, 255);
}

#[test]
fn history_linear_branch_discards_and_nonlinear_keeps() {
    let mut doc = Document::new(8, 8);
    let mut hist = History::new();
    paint(&mut doc, &mut hist, "A", 1, 1, red());
    paint(&mut doc, &mut hist, "B", 2, 2, red());
    assert!(hist.goto_state(&mut doc, 1));
    paint(&mut doc, &mut hist, "C", 3, 3, red());
    assert_eq!(hist.state_count(), 2, "linear: B discarded");
    assert_eq!(hist.labels()[1], "C");

    let mut doc2 = Document::new(8, 8);
    let mut hist2 = History::new();
    hist2.set_non_linear(true);
    paint(&mut doc2, &mut hist2, "A", 1, 1, red());
    paint(&mut doc2, &mut hist2, "B", 2, 2, red());
    assert!(hist2.goto_state(&mut doc2, 1));
    paint(&mut doc2, &mut hist2, "C", 3, 3, red());
    assert_eq!(hist2.state_count(), 3, "non-linear: B kept");
}

#[test]
fn history_doc_at_state_and_snapshot() {
    let mut doc = Document::new(8, 8);
    let mut hist = History::new();
    paint(&mut doc, &mut hist, "A", 1, 1, red());
    paint(
        &mut doc,
        &mut hist,
        "B",
        2,
        2,
        Pixel16::from_rgba8(0, 0, 255, 255),
    );
    let at1 = hist.doc_at_state(&doc, 1).unwrap();
    assert_eq!(at1.layers[0].get_pixel(1, 1).to_rgba8().0, 255);
    assert_eq!(at1.layers[0].get_pixel(2, 2).to_rgba8().3, 0);
    // Current position untouched.
    assert_eq!(hist.position(), 2);
    let snap = Snapshot {
        name: "S1".into(),
        doc: at1,
        kind: pictodesk_core::history::SnapKind::Full,
    };
    assert_eq!(snap.doc.layers[0].get_pixel(1, 1).to_rgba8().0, 255);
    hist.purge();
    assert_eq!(hist.state_count(), 0);
    assert!(!hist.can_undo());
}

#[test]
fn drop_shadow_renders_beside_the_shape() {
    let mut doc = Document::new(40, 40);
    doc.layers[0].fill_rect(10, 10, 10, 10, red());
    let fx = Effects {
        drop_shadow: ShadowSpec {
            enabled: true,
            blur: 1,
            distance: 6,
            opacity: 1.0,
            ..Default::default()
        },
        ..Default::default()
    };
    doc.layers[0].effects = Some(fx);
    assert!(doc.has_effects());
    // Shadow falls down-left at the default 120-degree angle:
    // a pixel below the square should be darkened (shadow over
    // transparency = dark translucent), the square itself red.
    let inside = doc.render_pixel(15, 15).to_rgba8();
    assert_eq!(inside.0, 255);
    let below = doc.render_pixel(15, 24).to_rgba8();
    assert!(
        below.3 > 0,
        "shadow alpha present below the shape: {below:?}"
    );
    assert!(below.0 < 100, "shadow is dark: {below:?}");
    // Far away: nothing.
    assert_eq!(doc.render_pixel(38, 2).to_rgba8().3, 0);
}

#[test]
fn disabled_effects_render_identically() {
    let mut doc = Document::new(16, 16);
    doc.layers[0].fill_rect(2, 2, 8, 8, red());
    let plain = doc.render_checksum();
    doc.layers[0].effects = Some(Effects::default());
    assert!(!doc.has_effects());
    assert_eq!(doc.render_checksum(), plain);
}

#[test]
fn channel_mixer_identity_and_mono() {
    let mut doc = Document::new(4, 4);
    doc.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(200, 100, 50, 255));
    let before = doc.render_checksum();
    channel_mixer(&mut doc, 0, None, &ChannelMixer::default());
    assert_eq!(
        doc.render_checksum(),
        before,
        "identity mix changes nothing"
    );
    let mix = ChannelMixer {
        rows: [[0.5, 0.5, 0.0, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]],
        constant: [0.0; 4],
        monochrome: true,
    };
    channel_mixer(&mut doc, 0, None, &mix);
    let p = doc.layers[0].get_pixel(1, 1).to_rgba8();
    assert_eq!(p.0, p.1);
    assert_eq!(p.1, p.2);
    assert!(
        (p.0 as i32 - 150).abs() <= 1,
        "50/50 mix = 150, got {}",
        p.0
    );
}

#[test]
fn text_spec_style_runs() {
    let mut spec = TextSpec::new("hello world");
    assert_eq!(spec.style_at(0).size, 36.0);
    let bold = CharStyle {
        size: 20.0,
        ..Default::default()
    };
    spec.set_style(0, 5, bold.clone());
    assert_eq!(spec.style_at(0).size, 20.0);
    assert_eq!(spec.style_at(4).size, 20.0);
    assert_eq!(spec.style_at(6).size, 36.0);
    // One run for the styled span, one for the default remainder.
    assert_eq!(spec.runs.len(), 2);
    assert_eq!((spec.runs[0].start, spec.runs[0].end), (0, 5));
    // Setting everything back to default collapses the runs.
    spec.set_style(0, 5, CharStyle::default());
    assert!(spec.runs.is_empty());
}

#[test]
fn format_roundtrip_keeps_effects_text_samplers_pattern() {
    let mut doc = Document::new(24, 24);
    doc.layers[0].fill_rect(1, 1, 6, 6, red());
    let fx = Effects {
        drop_shadow: ShadowSpec {
            enabled: true,
            distance: 9,
            ..Default::default()
        },
        bevel: pictodesk_core::effects::BevelSpec {
            enabled: true,
            depth: 150,
            ..Default::default()
        },
        ..Default::default()
    };
    doc.layers[0].effects = Some(fx);
    let top = doc.add_layer("Title");
    let mut spec = TextSpec::new("Sale!");
    spec.default_style.size = 22.0;
    doc.layers[top].text = Some(spec);
    doc.samplers = vec![(3, 4), (20, 20)];
    doc.global_angle = 90;
    doc.pattern = Some(pictodesk_core::document::PatternData {
        name: "Dots".into(),
        width: 2,
        height: 2,
        px: vec![red(); 4],
    });
    let bytes = pictodesk_core::format::serialize(&doc);
    let back = pictodesk_core::format::deserialize(&bytes).expect("reload");
    let fx2 = back.layers[0].effects.expect("effects survive");
    assert!(fx2.drop_shadow.enabled);
    assert_eq!(fx2.drop_shadow.distance, 9);
    assert!(fx2.bevel.enabled);
    assert_eq!(fx2.bevel.depth, 150);
    let t2 = back.layers[top].text.as_ref().expect("text survives");
    assert_eq!(t2.text, "Sale!");
    assert_eq!(t2.default_style.size, 22.0);
    assert_eq!(back.samplers, vec![(3, 4), (20, 20)]);
    assert_eq!(back.global_angle, 90);
    assert_eq!(back.pattern.as_ref().unwrap().name, "Dots");
    assert_eq!(back.render_checksum(), doc.render_checksum());
}

#[test]
fn psd_export_roundtrips_through_our_importer() {
    let mut doc = Document::new(20, 12);
    doc.layers[0].fill_rect(0, 0, 20, 12, Pixel16::from_rgba8(30, 60, 90, 255));
    let top = doc.add_layer("Accent");
    doc.layers[top].fill_rect(3, 3, 5, 4, red());
    doc.layers[top].opacity = 0.5;
    let (bytes, report) = pictodesk_core::psd::save_psd(&doc);
    assert!(bytes.starts_with(b"8BPS"));
    assert!(
        report.warnings.is_empty(),
        "no warnings for a plain doc: {:?}",
        report.warnings
    );
    let (back, _) = pictodesk_core::psd::load_psd(&bytes).expect("our PSD reopens");
    assert_eq!(back.layers.len(), 2);
    assert_eq!((back.width, back.height), (20, 12));
    // Composite should be close (8-bit quantization).
    let (a, b) = (
        doc.render_pixel(4, 4).to_rgba8(),
        back.render_pixel(4, 4).to_rgba8(),
    );
    assert!(
        (a.0 as i32 - b.0 as i32).abs() <= 2,
        "composite survives: {a:?} vs {b:?}"
    );
}

#[test]
fn dcs_export_makes_master_and_plates() {
    let mut doc = Document::new(6, 4);
    doc.layers[0].fill_rect(0, 0, 6, 4, red());
    let files = pictodesk_core::io::export_dcs(&doc, "job");
    assert_eq!(files.len(), 5, "master + CMYK plates");
    assert_eq!(files[0].0, "job.eps");
    let master = String::from_utf8_lossy(&files[0].1).into_owned();
    assert!(master.contains("%%DCSVersion: 2.0"));
    assert!(master.contains("%%PlateFile: (job.C.eps) Cyan"));
    assert!(files.iter().any(|(n, _)| n == "job.K.eps"));
}

#[test]
fn quad_warp_moves_the_painted_corner() {
    let mut doc = Document::new(20, 20);
    doc.layers[0].fill_rect(2, 2, 4, 4, red());
    // Drag the top-left corner of the painted bounds inward.
    pictodesk_core::transform::transform_layer_quad(
        &mut doc,
        0,
        [(3.0, 3.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)],
    );
    // Something red still exists, and the original corner is empty.
    assert_eq!(doc.layers[0].get_pixel(2, 2).to_rgba8().3, 0);
    let mut any = false;
    for y in 0..20 {
        for x in 0..20 {
            if doc.layers[0].get_pixel(x, y).to_rgba8().0 == 255 {
                any = true;
            }
        }
    }
    assert!(any, "warped paint survives somewhere");
}

#[test]
fn homography_identity() {
    let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let h = pictodesk_core::transform::Homography::through(corners, corners).unwrap();
    let (x, y) = h.apply(3.0, 4.0);
    assert!((x - 3.0).abs() < 0.01 && (y - 4.0).abs() < 0.01);
}
