//! Headless mode: --shot screenshots and the --phaseNtest / --batchtest suites.
use super::*;
use crate::actions::{
    apply_steps, default_shortcuts, dyn_from_bytes, dyn_to_bytes, line_to_step, load_actions_state,
    load_shortcuts, paths_to_ai, read_abr, read_aco, step_to_line, write_abr, write_aco, Action,
    BrushPreset, RecordedAction, RecordedStep,
};
use crate::dialogs::{fx_title, Dialog, DialogKind};
use crate::file_panel::last_open_dir;
use crate::menus::OptAct;
use crate::panels::{HistSource, PanelDrop, PanelKind, PanelRow, SlotLoc};
use crate::runner::Gpu;
use crate::tools::{BrushDynamics, ChannelView, DragMode, PaintTarget, Tool};

pub(crate) fn write_ppm(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&px[0..3]);
    }
    std::fs::write(path, out)
}

/// Headless verification: draw one frame (optionally with a menu open
/// or a dialog up) and render it through the real GPU pipeline.
pub(crate) fn run_shot(args: &[String]) -> i32 {
    let mut shot_path: Option<PathBuf> = None;
    let mut menu: Option<usize> = None;
    let mut dialog_name: Option<String> = None;
    let mut size = (1280u32, 800u32);
    let mut demo2 = false;
    let mut demo3 = false;
    let mut demo4 = false;
    let mut ctx = false;
    let mut batchtest = false;
    let mut gaptest = false;
    let mut phase5test = false;
    let mut phase6test = false;
    let mut phase65test = false;
    let mut phase7test = false;
    let mut tone = false;
    let mut tab: Option<String> = None;
    let mut cmyk = false;
    let mut ft = false;
    let mut many_layers = false;
    let mut view: Option<String> = None;
    let mut uitool: Option<String> = None;
    let mut uipanels = false;
    let mut twotabs = false;
    let mut uitooltip = false;
    let mut prefshortcuts = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--shot" => {
                shot_path = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--menu" => {
                menu = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 1;
            }
            "--dialog" => {
                dialog_name = args.get(i + 1).cloned();
                i += 1;
            }
            "--demo2" => demo2 = true,
            "--demo3" => demo3 = true,
            "--demo4" => demo4 = true,
            "--ctx" => ctx = true,
            "--batchtest" => batchtest = true,
            "--gaptest" => gaptest = true,
            "--phase5test" => phase5test = true,
            "--phase6test" => phase6test = true,
            "--phase65test" => phase65test = true,
            "--phase7test" => phase7test = true,
            "--tone" => tone = true,
            "--cmyk" => cmyk = true,
            "--ft" => ft = true,
            "--manylayers" => many_layers = true,
            "--tab" => {
                tab = args.get(i + 1).cloned();
                i += 1;
            }
            "--view" => {
                view = args.get(i + 1).cloned();
                i += 1;
            }
            "--uitool" => {
                uitool = args.get(i + 1).cloned();
                i += 1;
            }
            "--uipanels" => uipanels = true,
            "--twotabs" => twotabs = true,
            "--uitooltip" => uitooltip = true,
            "--prefshortcuts" => prefshortcuts = true,
            "--size" => {
                if let Some(s) = args.get(i + 1) {
                    if let Some((w, h)) = s.split_once('x') {
                        size = (w.parse().unwrap_or(1280), h.parse().unwrap_or(800));
                    }
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let Some(shot_path) = shot_path else {
        eprintln!("--shot needs an output path");
        return 2;
    };
    let mut app = App::new(demo_document(), None);
    app.win_size = size;
    app.fit_view(size.0, size.1);
    if demo2 {
        // Phase 2 demo state: a product ellipse, its pen path, a saved
        // selection channel, the dock open.
        let mut doc = Document::new(640, 400);
        doc.layers[0].fill_rect(0, 0, 640, 400, Pixel16::from_rgba8(120, 130, 140, 255));
        let sel = Selection::ellipse(640, 400, 170, 90, 300, 220);
        brush::fill_selection(&mut doc, 0, &sel, Pixel16::from_rgba8(215, 40, 30, 255));
        let k = 0.552_284_8f32;
        let (cx, cy, rx, ry) = (320.0f32, 200.0f32, 150.0f32, 110.0f32);
        let mut path = BezPath::new("");
        path.subpaths.push(Subpath {
            closed: true,
            anchors: vec![
                Anchor::smooth(
                    (cx + rx, cy),
                    (cx + rx, cy - k * ry),
                    (cx + rx, cy + k * ry),
                ),
                Anchor::smooth(
                    (cx, cy + ry),
                    (cx + k * rx, cy + ry),
                    (cx - k * rx, cy + ry),
                ),
                Anchor::smooth(
                    (cx - rx, cy),
                    (cx - rx, cy + k * ry),
                    (cx - rx, cy - k * ry),
                ),
                Anchor::smooth(
                    (cx, cy - ry),
                    (cx - k * rx, cy - ry),
                    (cx + k * rx, cy - ry),
                ),
            ],
        });
        doc.work_path = Some(path);
        doc.save_selection_as_channel(&sel);
        if cmyk {
            pictodesk_core::mode::convert_mode(&mut doc, ColorMode::Cmyk);
        }
        app.doc = doc;
        app.doc_version += 1;
        app.tool = Tool::Pen;
        app.active_path = Some(PathRef::Work);
        app.pen_anchor = Some((0, 0));
        app.show_cp = true;
        app.cp_tab = match tab.as_deref() {
            Some("paths") => 2,
            Some("channels") => 1,
            _ => 0,
        };
        match view.as_deref() {
            Some("alpha") => {
                app.channel_view = ChannelView::Alpha(0);
                app.paint_target = PaintTarget::Alpha(0);
            }
            Some("channel") => app.channel_view = ChannelView::Channel(0),
            _ => {}
        }
        app.fit_view(size.0, size.1);
    }
    if demo3 {
        // Phase 3 demo state: a four-layer composite (sky, mountains,
        // a Screen sun linked to the sky, a translucent title bar)
        // with the Layers panel up front and the Move tool active.
        let mut doc = Document::new(640, 400);
        doc.layers[0].name = "Sky".into();
        for y in 0..400i32 {
            for x in 0..640i32 {
                let v = 205 - y / 4;
                doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(110, 155, v as u8, 255));
            }
        }
        let mtn = doc.add_layer("Mountains");
        for y in 170..400i32 {
            for x in 0..640i32 {
                if (x - 320).abs() < (y - 160) * 2 {
                    doc.layers[mtn].set_pixel(x, y, Pixel16::from_rgba8(95, 85, 75, 255));
                }
            }
        }
        let sun = doc.add_layer("Sun");
        for y in 40..130i32 {
            for x in 460..550i32 {
                if (x - 505) * (x - 505) + (y - 85) * (y - 85) < 1900 {
                    doc.layers[sun].set_pixel(x, y, Pixel16::from_rgba8(255, 225, 130, 255));
                }
            }
        }
        doc.layers[sun].blend_mode = pictodesk_core::blend::BlendMode::Screen;
        doc.layers[sun].linked = true;
        let title = doc.add_layer("Title Bar");
        doc.layers[title].fill_rect(0, 0, 640, 64, Pixel16::from_rgba8(18, 18, 28, 255));
        doc.layers[title].opacity = 0.85;
        doc.active_layer = title;
        app.doc = doc;
        app.doc_version += 1;
        app.tool = Tool::Move;
        app.show_cp = true;
        app.cp_tab = 0;
        app.fit_view(size.0, size.1);
    }
    if demo4 {
        // Phase 4 demo state: a photo graded by two adjustment
        // layers (the Photo Filter's mask hides it on the left), a
        // masked color layer, guides, the Navigator and Actions
        // panels open, and one recorded action.
        let mut doc = Document::new(640, 400);
        doc.layers[0].name = "Photo".into();
        for y in 0..400i32 {
            for x in 0..640i32 {
                let r = (x * 255 / 639) as u8;
                let g = (y * 255 / 399) as u8;
                let b = ((x + y) * 255 / 1038) as u8;
                doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(r, g, b, 255));
            }
        }
        doc.layers[0].fill_rect(60, 60, 180, 120, Pixel16::from_rgba8(200, 60, 40, 255));
        doc.layers[0].fill_rect(400, 220, 180, 120, Pixel16::from_rgba8(40, 90, 190, 255));
        let tint = doc.add_layer("Teal Wash");
        doc.layers[tint].fill_rect(0, 0, 640, 400, Pixel16::from_rgba8(20, 120, 120, 255));
        doc.layers[tint].opacity = 0.45;
        doc.layers[tint].add_mask(255);
        for y in 0..400i32 {
            for x in 320..640i32 {
                doc.layers[tint].set_mask_value(x, y, 0);
            }
        }
        doc.add_adjustment_layer(
            "Levels",
            pictodesk_core::adjust::Adjustment::Levels {
                in_black: 25,
                in_white: 230,
                gamma: 1.15,
                out_black: 0,
                out_white: 255,
            },
        );
        let pf = doc.add_adjustment_layer(
            "Photo Filter",
            pictodesk_core::adjust::Adjustment::PhotoFilter {
                color: (255, 178, 84),
                density: 45,
            },
        );
        for y in 0..400i32 {
            for x in 0..240i32 {
                doc.layers[pf].set_mask_value(x, y, 0);
            }
        }
        doc.guides_x = vec![320];
        doc.guides_y = vec![200];
        doc.active_layer = pf;
        app.doc = doc;
        app.doc_version += 1;
        app.tool = Tool::Move;
        app.show_cp = true;
        app.cp_tab = 0;
        app.show_actions = true;
        app.show_grid = true;
        app.actions = vec![RecordedAction {
            name: "Web Resize + Warm".into(),
            set: "Default Set".into(),
            steps: vec![
                RecordedStep::ImageSize(800, 600),
                RecordedStep::AdjustmentLayer(pictodesk_core::adjust::Adjustment::PhotoFilter {
                    color: (255, 178, 84),
                    density: 30,
                }),
                RecordedStep::Adjustment(pictodesk_core::adjust::Adjustment::HueSaturation {
                    hue: 0,
                    saturation: 15,
                    lightness: 0,
                }),
            ],
        }];
        app.active_action = 0;
        app.paint_target = PaintTarget::Mask(pf);
        app.fit_view(size.0, size.1);
    }
    if ctx {
        // Right-click on the canvas at a fixed spot.
        app.right_click(300.0, 300.0);
        app.cursor = (300.0, 300.0);
    }
    if batchtest {
        // End-to-end Actions check: record two steps on the demo
        // document, play them onto a fresh document and compare
        // renders; then batch a folder of generated PNGs.
        app.actions = vec![RecordedAction {
            name: "Test Action".into(),
            steps: Vec::new(),
            set: "Default Set".into(),
        }];
        app.active_action = 0;
        app.recording = true;
        app.do_action(Action::FlipHorizontal);
        app.do_action(Action::AdjustDialog("invert"));
        app.recording = false;
        let steps = app.actions[0].steps.len();
        let mut other = App::new(demo_document(), None);
        other.actions = app.actions.clone();
        other.active_action = 0;
        let before = other.doc.render_checksum();
        other.play_action(0);
        let played = other.doc.render_checksum();
        let mut expected = demo_document();
        apply_steps(&mut expected, &app.actions[0].steps.clone(), None);
        let dir = std::env::temp_dir().join("pictodesk-batchtest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["a", "b"] {
            let png = io::export_image(&demo_document(), ExportFormat::Png, 90).unwrap();
            std::fs::write(dir.join(format!("{n}.png")), png).unwrap();
        }
        app.run_batch(dir.to_str().unwrap());
        let out_a = dir.join("a-edited.png").exists();
        let out_b = dir.join("b-edited.png").exists();
        let ok = steps == 2
            && played != before
            && played == expected.render_checksum()
            && out_a
            && out_b;
        eprintln!(
            "BATCHTEST steps={steps} played_ok={} batch_ok={} {}",
            played == expected.render_checksum(),
            out_a && out_b,
            if ok { "PASS" } else { "FAIL" }
        );
        if !ok {
            return 1;
        }
    }
    if gaptest {
        // End-to-end check of the Phase 4.5 app paths: toning
        // tools, Copy Merged / Paste Into, Color Range, Modify,
        // a filter dialog (with Actions recording + replay), the
        // Custom Filter dialog, Bring to Front, and Equalize.
        let mut ok = true;
        let mut check = |name: &str, cond: bool| {
            eprintln!("GAPTEST {name}: {}", if cond { "ok" } else { "FAIL" });
            ok = ok && cond;
        };
        // Dodge lightens a mid-gray pixel.
        let mut doc = Document::new(24, 24);
        doc.layers[0].fill_rect(0, 0, 24, 24, Pixel16::from_rgba8(100, 100, 100, 255));
        let mut app = App::new(doc, None);
        app.tool = Tool::Dodge;
        app.tip = BrushTip::new(13.0, 0.0);
        app.begin_stroke((12.0, 12.0));
        let dodged = app.doc.layers[0].get_pixel(12, 12).to_rgba8().0;
        check("dodge lightens", dodged > 100);
        app.tool = Tool::Burn;
        app.begin_stroke((12.0, 12.0));
        let burned = app.doc.layers[0].get_pixel(12, 12).to_rgba8().0;
        check("burn darkens back", burned < dodged);
        // Copy Merged takes the composite; Paste Into clips to the
        // current selection.
        let mut doc = Document::new(10, 10);
        doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(0, 0, 200, 255));
        let top = doc.add_layer("Top");
        doc.layers[top].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(200, 0, 0, 128));
        let mut app = App::new(doc, None);
        app.sel = Some(Selection::all(10, 10));
        app.do_action(Action::CopyMerged);
        let mut app2 = App::new(Document::new(10, 10), None);
        app2.sel = Some(Selection::rect(10, 10, 2, 2, 4, 4));
        app2.do_action(Action::PasteInto);
        let inside = app2.doc.layers[0].get_pixel(3, 3).to_rgba8();
        let outside = app2.doc.layers[0].get_pixel(0, 0).to_rgba8();
        check(
            "paste into lands the merged color inside",
            inside.0 > 60 && inside.2 > 60,
        );
        check(
            "paste into leaves the outside alone",
            outside == (0, 0, 0, 0),
        );
        // Color Range selects the red half by the foreground color.
        let mut doc = Document::new(10, 4);
        doc.layers[0].fill_rect(0, 0, 5, 4, Pixel16::from_rgba8(200, 40, 40, 255));
        doc.layers[0].fill_rect(5, 0, 5, 4, Pixel16::from_rgba8(40, 40, 200, 255));
        let mut app = App::new(doc, None);
        app.fg = Pixel16::from_rgba8(200, 40, 40, 255);
        app.open_dialog(DialogKind::ColorRange);
        app.dialog_ok();
        let sel = app.sel.clone().unwrap();
        check(
            "color range picks red only",
            sel.value(2, 2) == 255 && sel.value(7, 2) == 0,
        );
        // Modify > Expand grows the selection by the dialog value.
        app.sel = Some(Selection::rect(10, 4, 4, 1, 2, 2));
        app.open_modify_dialog(0);
        if let Some(d) = &mut app.dialog {
            d.vals[0] = 1.0;
        }
        app.dialog_ok();
        check(
            "modify expand grows by 1",
            app.sel.as_ref().unwrap().value(3, 1) == 255,
        );
        // A filter dialog applies, records, and replays identically.
        let mut app = App::new(demo_document(), None);
        app.actions = vec![RecordedAction {
            name: "Gap".into(),
            steps: Vec::new(),
            set: "Default Set".into(),
        }];
        app.active_action = 0;
        app.recording = true;
        app.open_dialog(DialogKind::MotionBlur);
        app.dialog_ok();
        app.recording = false;
        let mut expected = demo_document();
        apply_steps(&mut expected, &app.actions[0].steps.clone(), None);
        check(
            "motion blur dialog records + replays",
            app.actions[0].steps.len() == 1
                && app.doc.render_checksum() == expected.render_checksum(),
        );
        // Custom Filter dialog with a sharpen kernel changes pixels.
        let mut app = App::new(demo_document(), None);
        let before = app.doc.render_checksum();
        app.open_dialog(DialogKind::CustomFilter);
        if let Some(d) = &mut app.dialog {
            d.kernel = [
                0, 0, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0,
            ];
        }
        app.dialog_refresh_preview();
        app.dialog_ok();
        check("custom filter applies", app.doc.render_checksum() != before);
        // Bring to Front moves the bottom layer to the top.
        let mut doc = Document::new(6, 6);
        doc.add_layer("Mid");
        doc.add_layer("Top");
        let mut app = App::new(doc, None);
        app.doc.active_layer = 0;
        app.do_action(Action::LayerToFront);
        check(
            "bring to front",
            app.doc.active_layer == 2 && app.doc.layers[2].name == "Background",
        );
        // Equalize runs as a direct action.
        let mut doc = Document::new(8, 2);
        for x in 0..4 {
            doc.layers[0].fill_rect(x, 0, 1, 2, Pixel16::from_rgba8(100, 100, 100, 255));
        }
        for x in 4..8 {
            doc.layers[0].fill_rect(x, 0, 1, 2, Pixel16::from_rgba8(150, 150, 150, 255));
        }
        let mut app = App::new(doc, None);
        app.do_action(Action::AdjustEqualize);
        check(
            "equalize action spreads values",
            app.doc.layers[0].get_pixel(1, 1).to_rgba8().0 == 0
                && app.doc.layers[0].get_pixel(6, 1).to_rgba8().0 == 255,
        );
        eprintln!("GAPTEST {}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            return 1;
        }
    }
    if phase5test {
        // End-to-end checks of the Phase 5 app paths: History
        // panel state travel + snapshots, the Effects dialog,
        // type layers, the Magnetic Lasso, Fit Image, PSD export,
        // Save for Web sizing, Define Pattern + Pattern Stamp,
        // and Channel Mixer recording.
        let mut ok = true;
        let mut check = |name: &str, cond: bool| {
            eprintln!("PHASE5 {name}: {}", if cond { "ok" } else { "FAIL" });
            ok = ok && cond;
        };
        // --- History: two edits, travel back, snapshot source.
        let mut doc = Document::new(20, 20);
        doc.layers[0].fill_rect(2, 2, 6, 6, Pixel16::from_rgba8(200, 30, 30, 255));
        let mut app = App::new(doc, None);
        app.do_action(Action::AdjustDialog("invert"));
        let after_one = app.doc.render_checksum();
        app.do_action(Action::FlipHorizontal);
        check("history has states", app.hist.state_count() >= 2);
        app.do_action(Action::Undo);
        check(
            "undo returns to state 1",
            app.doc.render_checksum() == after_one,
        );
        app.do_action(Action::Redo);
        check("redo moves forward", app.doc.render_checksum() != after_one);
        let snaps_before = app.snapshots.len();
        app.take_snapshot();
        check("snapshot taken", app.snapshots.len() == snaps_before + 1);
        app.hist_source = HistSource::Snapshot(app.snapshots.len() - 1);
        app.hist_src_cache = None;
        check(
            "snapshot is a valid source",
            app.hist_source_doc().map(|d| d.render_checksum()) == Some(app.doc.render_checksum()),
        );
        // --- Effects dialog: enable Drop Shadow, commit, verify.
        let mut app = App::new(Document::new(30, 30), None);
        let top = app.doc.add_layer("Shape");
        app.doc.layers[top].fill_rect(8, 8, 10, 10, Pixel16::from_rgba8(200, 0, 0, 255));
        let plain = app.doc.render_checksum();
        app.do_action(Action::EffectsDialog(0));
        let has_dlg = app.dialog.is_some();
        if let Some(d) = &mut app.dialog {
            d.fx.drop_shadow.enabled = true;
            d.fx.drop_shadow.distance = 6;
            d.fx.drop_shadow.blur = 1;
        }
        app.fx_sync_from_vals();
        app.dialog_ok();
        check("effects dialog opened", has_dlg);
        check(
            "drop shadow committed",
            app.doc.layers[top]
                .effects
                .map(|e| e.drop_shadow.enabled)
                .unwrap_or(false),
        );
        check("shadow renders", app.doc.render_checksum() != plain);
        // --- Type layer through the dialog.
        let mut app = App::new(Document::new(120, 60), None);
        let layers_before = app.doc.layers.len();
        app.open_type_dialog((6.0, 6.0), false, false);
        if let Some(d) = &mut app.dialog {
            d.text = "Hi".into();
        }
        app.dialog_ok();
        let tl = app.doc.layers.iter().position(|l| l.text.is_some());
        check(
            "type layer created",
            tl.is_some() && app.doc.layers.len() == layers_before + 1,
        );
        check(
            "type rendered pixels",
            tl.and_then(|i| pictodesk_core::transform::layer_painted_bounds(&app.doc.layers[i]))
                .is_some(),
        );
        // --- Magnetic Lasso on a hard edge.
        let mut doc = Document::new(30, 20);
        doc.layers[0].fill_rect(0, 0, 15, 20, Pixel16::from_rgba8(0, 0, 0, 255));
        doc.layers[0].fill_rect(15, 0, 15, 20, Pixel16::from_rgba8(255, 255, 255, 255));
        let mut app = App::new(doc, None);
        app.tool = Tool::MagneticLasso;
        app.fit_view(1280, 800);
        let c = app.canvas_rect(1280, 800);
        let scr: Vec<(f32, f32)> = [
            (15.0f32, 2.0f32),
            (15.0, 10.0),
            (15.0, 18.0),
            (4.0, 18.0),
            (4.0, 4.0),
        ]
        .iter()
        .map(|p| {
            let s = app.viewport.doc_to_screen(*p);
            (c.x as f32 + s.0, c.y as f32 + s.1)
        })
        .collect();
        app.mouse_down(scr[0].0, scr[0].1);
        for p in &scr[1..] {
            app.mouse_move(p.0, p.1);
        }
        app.mouse_up(scr[0].0, scr[0].1);
        check(
            "magnetic lasso selects",
            app.sel.as_ref().and_then(|s| s.bounds()).is_some(),
        );
        // --- Fit Image shrinks to the box.
        let mut app = App::new(Document::new(100, 50), None);
        app.open_dialog(DialogKind::FitImage);
        if let Some(d) = &mut app.dialog {
            d.vals = vec![40.0, 40.0];
        }
        app.dialog_ok();
        check(
            "fit image fits",
            app.doc.width == 40 && app.doc.height == 20,
        );
        // --- PSD export produces a real header + round trip.
        let (bytes, report) = pictodesk_core::psd::save_psd(&app.doc);
        check(
            "psd bytes + no warnings on a plain doc",
            bytes.starts_with(b"8BPS") && report.warnings.is_empty(),
        );
        // --- Save for Web computes a size.
        let mut app = App::new(Document::new(16, 16), None);
        app.do_action(Action::SaveForWebDialog);
        app.refresh_web_size();
        let sz = app.dialog.as_ref().map(|d| d.web_size).unwrap_or(0);
        app.dialog_cancel();
        check("save for web size > 0", sz > 0);
        // --- Define Pattern + Pattern Stamp paints.
        let mut doc = Document::new(16, 16);
        doc.layers[0].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(0, 200, 0, 255));
        let mut app = App::new(doc, None);
        app.sel = Some(Selection::rect(16, 16, 0, 0, 8, 8));
        app.do_action(Action::DefinePattern);
        check("pattern defined", app.doc.pattern.is_some());
        app.sel = None;
        app.tool = Tool::PatternStamp;
        app.tip = BrushTip::new(9.0, 1.0);
        app.begin_stroke((12.0, 12.0));
        check(
            "pattern stamp paints the pattern",
            app.doc.layers[0].get_pixel(12, 12).to_rgba8().1 > 100,
        );
        // --- Channel Mixer dialog records a step.
        let mut app = App::new(Document::new(8, 8), None);
        app.actions = vec![RecordedAction {
            name: "Mix".into(),
            steps: Vec::new(),
            set: "Default Set".into(),
        }];
        app.active_action = 0;
        app.recording = true;
        app.open_dialog(DialogKind::ChannelMixer);
        app.dialog_ok();
        app.recording = false;
        check(
            "channel mixer recorded",
            app.actions[0].steps.len() == 1
                && matches!(app.actions[0].steps[0], RecordedStep::ChannelMixer(_)),
        );
        // --- Snapshot variants: merged + current layer.
        let mut doc = Document::new(12, 12);
        doc.layers[0].fill_rect(0, 0, 12, 12, Pixel16::from_rgba8(10, 20, 30, 255));
        let top = doc.add_layer("Top");
        doc.layers[top].fill_rect(2, 2, 4, 4, Pixel16::from_rgba8(200, 200, 200, 255));
        let mut app = App::new(doc, None);
        app.take_snapshot_kind(SnapKind::Merged);
        let merged = app.snapshots.last().unwrap();
        check(
            "merged snapshot is flat",
            merged.kind == SnapKind::Merged && merged.doc.layers.len() == 1,
        );
        app.take_snapshot_kind(SnapKind::Layer(0));
        check(
            "layer snapshot tagged",
            app.snapshots.last().unwrap().kind == SnapKind::Layer(0),
        );
        // --- Type Mask makes a selection shaped like the text.
        let mut app = App::new(Document::new(120, 60), None);
        app.open_type_dialog((6.0, 6.0), false, true);
        if let Some(d) = &mut app.dialog {
            d.text = "Hi".into();
        }
        app.dialog_ok();
        check(
            "type mask selects",
            app.sel.as_ref().and_then(|s| s.bounds()).is_some() && app.doc.layers.len() == 1,
        );
        // --- Gradient shapes: a radial gradient differs from linear.
        let doc = Document::new(20, 20);
        let mut app = App::new(doc, None);
        app.gradient_shape = brush::GradientShape::Radial;
        let before = app.doc.render_checksum();
        brush::gradient_shaped(
            &mut app.doc,
            0,
            None,
            (10.0, 10.0),
            (19.0, 10.0),
            Pixel16::from_rgba8(255, 0, 0, 255),
            Pixel16::from_rgba8(0, 0, 255, 255),
            1.0,
            app.gradient_shape,
        );
        check(
            "radial gradient paints",
            app.doc.render_checksum() != before
                && app.doc.layers[0].get_pixel(10, 19).to_rgba8().2 > 200
                && app.doc.layers[0].get_pixel(10, 1).to_rgba8().2 > 200
                && app.doc.layers[0].get_pixel(10, 10).to_rgba8().0 > 200,
        );
        // --- Numeric transform dialog drives FT.
        let mut doc = Document::new(20, 20);
        doc.layers[0].fill_rect(2, 2, 5, 5, Pixel16::from_rgba8(90, 90, 90, 255));
        let mut app = App::new(doc, None);
        app.begin_free_transform();
        app.do_action(Action::FtNumericDialog);
        if let Some(d) = &mut app.dialog {
            d.vals[0] = 200.0;
        }
        app.dialog_refresh_preview();
        let scaled = app.ft.as_ref().map(|f| f.sx) == Some(2.0);
        app.dialog_ok();
        check("numeric FT scales + commits", scaled && app.ft.is_none());
        // --- 3D Transform wireframe drag rotates the solid.
        let mut app = App::new(Document::new(20, 20), None);
        app.open_dialog(DialogKind::Transform3d);
        let wr = app.wireframe_rect();
        check("wireframe rect exists", wr.is_some());
        if let Some(wr) = wr {
            let (cx, cy) = (wr.x + wr.w / 2, wr.y + wr.h / 2);
            let (rx0, ry0) = (
                app.dialog.as_ref().unwrap().vals[0],
                app.dialog.as_ref().unwrap().vals[1],
            );
            app.mouse_down(cx as f32, cy as f32);
            let dragging = app
                .dialog
                .as_ref()
                .map(|d| d.wire_drag.is_some())
                .unwrap_or(false);
            app.mouse_move((cx + 40) as f32, (cy + 20) as f32);
            app.mouse_up((cx + 40) as f32, (cy + 20) as f32);
            let d = app.dialog.as_ref().unwrap();
            check(
                "wireframe drag rotates + releases",
                dragging
                    && (d.vals[1] - (ry0 + 20.0)).abs() < 0.01
                    && (d.vals[0] - (rx0 + 10.0)).abs() < 0.01
                    && d.wire_drag.is_none(),
            );
        }
        app.dialog_cancel();
        eprintln!("PHASE5TEST {}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            return 1;
        }
    }
    if phase6test {
        // End-to-end checks of the Phase 6 app paths: shape
        // layers, layer sets + clipping + locks + colors,
        // Gradient Map, Liquify, type warp, slices + export,
        // notes, Perspective Crop, droplets, presets.
        let mut ok = true;
        let mut check = |name: &str, cond: bool| {
            eprintln!("PHASE6 {name}: {}", if cond { "ok" } else { "FAIL" });
            ok = ok && cond;
        };
        // --- Shape layer: create via action-less tool path
        // (shape_press equivalent: build the layer directly
        // through the same fields the canvas sets).
        let mut app = App::new(Document::new(60, 40), None);
        app.do_action(Action::NewFillLayer(1));
        check(
            "fill layer created",
            app.doc.layers[app.doc.active_layer].fill.is_some(),
        );
        app.do_action(Action::RasterizeLayer);
        check(
            "fill layer rasterized",
            app.doc.layers[app.doc.active_layer].fill.is_none()
                && app.doc.layers[app.doc.active_layer].tile_count() > 0,
        );
        // --- Sets, clipping, locks, colors.
        let mut app = App::new(Document::new(30, 30), None);
        let top = app.doc.add_layer("Top");
        app.doc.active_layer = top;
        app.do_action(Action::NewLayerSet);
        check(
            "layer set created + assigned",
            app.doc.sets.len() == 1 && app.doc.layers[top].group == 1,
        );
        app.do_action(Action::ToggleClipped);
        check("clipping on", app.doc.layers[top].clipped);
        app.do_action(Action::LayerColor(4));
        check("layer color set", app.doc.layers[top].color == 4);
        app.do_action(Action::LockToggle(2));
        check("image lock set", app.doc.layers[top].locks & 2 != 0);
        app.do_action(Action::AssignSet(0));
        check("set unassigned", app.doc.layers[top].group == 0);
        // --- Gradient Map adjustment layer.
        let mut app = App::new(Document::new(20, 20), None);
        app.doc.layers[0].fill_rect(2, 2, 8, 8, Pixel16::from_rgba8(200, 200, 200, 255));
        let before = app.doc.render_checksum();
        app.do_action(Action::NewAdjustment("gradientmap"));
        if let Some(d) = &mut app.dialog {
            d.photo_color = (0, 0, 0);
            d.gm_to = (255, 0, 0);
        }
        app.dialog_refresh_preview();
        app.dialog_ok();
        check(
            "gradient map layer renders",
            app.doc.layers.iter().any(|l| {
                matches!(
                    l.adjustment,
                    Some(pictodesk_core::adjust::Adjustment::GradientMap { .. })
                )
            }) && app.doc.render_checksum() != before,
        );
        // --- Liquify: open, stamp a warp, OK bakes it.
        let mut app = App::new(Document::new(30, 30), None);
        app.doc.layers[0].fill_rect(5, 5, 20, 20, Pixel16::from_rgba8(0, 180, 0, 255));
        let plain = app.doc.render_checksum();
        app.do_action(Action::LiquifyDialog);
        let has_dlg = app.dialog.is_some();
        if app.dialog.is_some() {
            app.liquify_stamp((15.0, 15.0));
            app.liquify_stamp((18.0, 15.0));
        }
        app.dialog_ok();
        check("liquify dialog opened", has_dlg);
        check("liquify baked", app.doc.render_checksum() != plain);
        // --- Type warp: warped text renders differently.
        let mk_text = |warp: Option<u8>| -> u64 {
            let mut doc = Document::new(120, 50);
            let li = doc.add_layer("T");
            let mut spec = pictodesk_core::text::TextSpec::new("Warp");
            spec.warp_style = warp;
            spec.warp_bend = 40.0;
            doc.layers[li].text = Some(spec);
            doc.active_layer = li;
            typo::render_type_layer(&mut doc, li);
            doc.render_checksum()
        };
        check("type warp renders", mk_text(Some(0)) != mk_text(None));
        // --- Slices + export.
        let mut app = App::new(Document::new(40, 30), None);
        app.doc.layers[0].fill_rect(0, 0, 40, 30, Pixel16::from_rgba8(10, 20, 30, 255));
        app.doc.slices.push(pictodesk_core::document::SliceRect {
            x: 2,
            y: 2,
            w: 10,
            h: 8,
            name: "hero".into(),
            url: String::new(),
            alt: String::new(),
        });
        let dir = std::env::temp_dir().join("pictodesk-p6-slices");
        let _ = std::fs::create_dir_all(&dir);
        app.file_path = Some(dir.join("doc.pdk"));
        app.do_action(Action::ExportSlices);
        check("slice exported", dir.join("doc-hero.png").exists());
        // --- Notes.
        let mut app = App::new(Document::new(20, 20), None);
        app.structural("Add Note", |doc| {
            doc.notes.push(pictodesk_core::document::Note {
                x: 3,
                y: 4,
                color: 0,
                text: "check this".into(),
                audio: None,
            });
        });
        check("note stored", app.doc.notes.len() == 1);
        // --- Perspective Crop.
        let mut doc = Document::new(40, 40);
        doc.layers[0].fill_rect(10, 10, 20, 20, Pixel16::from_rgba8(255, 0, 0, 255));
        let cropped = pictodesk_core::transform::perspective_crop(
            &mut doc,
            [(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)],
        );
        check(
            "perspective crop resizes + keeps content",
            cropped && doc.width == 20 && doc.height == 20 && doc.render_checksum() != 0,
        );
        // --- Preset Manager: seed + apply.
        let mut app = App::new(Document::new(20, 20), None);
        let top = app.doc.add_layer("Fx");
        app.doc.active_layer = top;
        app.do_action(Action::PresetManagerDialog);
        let has_pm = app.dialog.is_some();
        app.dialog_ok();
        check(
            "preset manager applies style",
            has_pm
                && app.doc.layers[top]
                    .effects
                    .map(|e| e.any_enabled())
                    .unwrap_or(false),
        );
        // --- v0.6.1 full-fat: Blend If feathering.
        {
            let mut doc = Document::new(20, 20);
            doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(255, 255, 255, 255));
            let top = doc.add_layer("Gray");
            doc.layers[top].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(60, 60, 60, 255));
            doc.layers[top].blend_if = [100, 255, 0, 255];
            let hard = doc.render_pixel(10, 10).to_rgba8();
            doc.layers[top].blend_if_split = [100, 0, 0, 0];
            let soft = doc.render_pixel(10, 10).to_rgba8();
            check(
                "blend-if feather softens the edge",
                hard.0 == 255 && soft.0 > 60 && soft.0 < 255,
            );
        }
        // --- v0.6.1 full-fat: set blend mode honored.
        {
            let mk = |mode: Option<pictodesk_core::blend::BlendMode>| {
                let mut doc = Document::new(20, 20);
                doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(255, 0, 0, 255));
                let top = doc.add_layer("Member");
                doc.layers[top].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(128, 128, 128, 255));
                doc.sets.push(pictodesk_core::document::LayerSet {
                    name: "S".into(),
                    opacity: 1.0,
                    mode,
                    visible: true,
                    color: 0,
                });
                doc.layers[top].group = 1;
                doc.render_pixel(10, 10).to_rgba8()
            };
            let pass = mk(None);
            let mult = mk(Some(pictodesk_core::blend::BlendMode::Multiply));
            check(
                "set mode multiply blends the group",
                pass.0 == 128 && pass.1 == 128 && mult.0 == 128 && mult.1 == 0,
            );
        }
        // --- v0.6.1 full-fat: Knockout Shallow stops at the
        // base, Deep punches through.
        {
            let mk = |ko: u8| {
                let mut doc = Document::new(20, 20);
                doc.layers[0].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(255, 0, 0, 255));
                let mid = doc.add_layer("Mid");
                doc.layers[mid].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(0, 255, 0, 255));
                let top = doc.add_layer("KO");
                doc.layers[top].fill_rect(0, 0, 20, 20, Pixel16::from_rgba8(0, 0, 255, 128));
                doc.layers[top].knockout = ko;
                doc.render_pixel(10, 10).to_rgba8()
            };
            let shallow = mk(1);
            let deep = mk(2);
            check(
                "knockout shallow vs deep differ",
                shallow.3 == 255 && deep.3 < 255 && shallow != deep,
            );
        }
        // --- v0.6.1 full-fat: Mask Hides Effects.
        {
            let mk = |hides: bool| {
                let mut doc = Document::new(30, 30);
                doc.layers[0].fill_rect(0, 0, 30, 30, Pixel16::from_rgba8(255, 255, 255, 255));
                let top = doc.add_layer("Fx");
                doc.layers[top].fill_rect(12, 12, 6, 6, Pixel16::from_rgba8(200, 0, 0, 255));
                let mut fx = pictodesk_core::effects::Effects::default();
                fx.drop_shadow.enabled = true;
                fx.drop_shadow.opacity = 1.0;
                fx.drop_shadow.distance = 5;
                fx.drop_shadow.blur = 1;
                fx.drop_shadow.use_global_angle = false;
                fx.drop_shadow.angle = 0;
                doc.layers[top].effects = Some(fx);
                doc.layers[top].add_mask(0);
                for yy in 12..18 {
                    for xx in 12..18 {
                        doc.layers[top].set_mask_value(xx, yy, 255);
                    }
                }
                doc.layers[top].mask_hides_effects = hides;
                // Any darkened pixel outside the content square
                // is shadow that escaped the mask.
                let mut escaped = false;
                for yy in 0..30 {
                    for xx in 0..30 {
                        if (12..18).contains(&xx) && (12..18).contains(&yy) {
                            continue;
                        }
                        let px = doc.render_pixel(xx, yy).to_rgba8();
                        if px.0 < 250 {
                            escaped = true;
                        }
                    }
                }
                escaped
            };
            check("mask hides effects when set", mk(false) && !mk(true));
        }
        // --- v0.6.1 full-fat: Weighted Optimization sizes.
        {
            let mut app = App::new(Document::new(24, 24), None);
            app.doc.layers[0].fill_rect(0, 0, 24, 24, Pixel16::from_rgba8(90, 140, 200, 255));
            let mut alpha = pictodesk_core::document::AlphaChannel::new("Weights", 24, 24);
            alpha.data.iter_mut().for_each(|v| *v = 128);
            app.doc.alphas.push(alpha);
            app.do_action(Action::SaveForWebDialog);
            if let Some(d) = &mut app.dialog {
                d.web_weighted = true;
            }
            app.dialog_refresh_preview();
            let size = app.dialog.as_ref().map(|d| d.web_size).unwrap_or(0);
            app.dialog_cancel();
            check("weighted optimization sizes", size > 0);
        }
        // --- v0.6.1 full-fat: Layers panel folder rows.
        {
            let mut app = App::new(Document::new(20, 20), None);
            let top = app.doc.add_layer("Member");
            app.doc.active_layer = top;
            app.do_action(Action::NewLayerSet);
            let rows = app.panel_rows();
            check(
                "panel rows include a folder header",
                rows.len() == 3
                    && matches!(rows[0], PanelRow::Set(0))
                    && matches!(rows[1], PanelRow::Layer(1)),
            );
        }
        eprintln!("PHASE6TEST {}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            return 1;
        }
    }
    if phase7test {
        // Phase 7 Batch 7a: blend modes, Auto family, healing
        // tools, Patch, Pattern Maker.
        let mut ok = true;
        let mut check = |name: &str, cond: bool| {
            eprintln!("PHASE7 {name}: {}", if cond { "ok" } else { "FAIL" });
            ok = ok && cond;
        };
        check(
            "22 blend modes in the menu list",
            pictodesk_core::blend::ALL_BLEND_MODES.len() == 22,
        );
        check("50 tools (Airbrush is a toggle now)", TOOLS.len() == 50);
        // Low-contrast document: the Auto family stretches it.
        let flat = || {
            let mut doc = Document::new(30, 20);
            doc.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(110, 100, 90, 255));
            doc.layers[0].fill_rect(3, 3, 8, 8, Pixel16::from_rgba8(140, 120, 100, 255));
            doc
        };
        {
            let mut app = App::new(flat(), None);
            app.do_action(Action::AutoLevelsCmd);
            let lo = app.doc.render_pixel(20, 15).to_rgba8();
            let hi = app.doc.render_pixel(5, 5).to_rgba8();
            check(
                "auto levels stretches to black/white",
                lo.0 == 0 && hi.0 == 255,
            );
        }
        {
            let mut app = App::new(flat(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::AutoContrastCmd);
            check("auto contrast applies", app.doc.render_checksum() != before);
            let mut app = App::new(flat(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::AutoColorCmd);
            check("auto color applies", app.doc.render_checksum() != before);
            check(
                "auto steps serialize",
                step_to_line(&RecordedStep::AutoColor).as_deref() == Some("autocolor")
                    && line_to_step("autolevels")
                        .and_then(|st| step_to_line(&st))
                        .as_deref()
                        == Some("autolevels"),
            );
        }
        // Healing through the real tool path.
        {
            let mut doc = Document::new(60, 30);
            for y in 0..30 {
                for x in 0..60 {
                    let v = if x < 30 {
                        40 + (x * 2) as u8
                    } else {
                        (180i32 + if (x + y) % 2 == 0 { 20 } else { -20 }) as u8
                    };
                    doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
                }
            }
            // A blemish in the destination half.
            doc.layers[0].fill_rect(8, 12, 6, 6, Pixel16::from_rgba8(10, 10, 10, 255));
            let mut app = App::new(doc, None);
            app.tool = Tool::Heal;
            app.heal_src = Some((45.0, 15.0));
            app.begin_stroke((10.0, 15.0));
            app.stamp_at((10.0, 15.0));
            let healed = app.doc.layers[0].get_pixel(10, 15).to_rgba8().0;
            check(
                "healing brush repairs toward destination tone",
                healed > 30 && healed < 130,
            );
            // Pattern-source healing needs a pattern.
            app.doc.pattern = Some(pictodesk_core::document::PatternData {
                name: "t".into(),
                width: 4,
                height: 4,
                px: vec![Pixel16::from_rgba8(90, 90, 90, 255); 16],
            });
            app.heal_use_pattern = true;
            let before = app.doc.render_checksum();
            app.begin_stroke((20.0, 15.0));
            app.stamp_at((20.0, 15.0));
            check(
                "pattern-source healing stamps",
                app.doc.render_checksum() != before,
            );
        }
        // Patch through the drag path.
        {
            let mut doc = Document::new(60, 30);
            for y in 0..30 {
                for x in 0..60 {
                    let v = if x < 30 {
                        40 + (x * 2) as u8
                    } else {
                        (180i32 + if (x + y) % 2 == 0 { 20 } else { -20 }) as u8
                    };
                    doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
                }
            }
            doc.layers[0].fill_rect(8, 12, 6, 6, Pixel16::from_rgba8(10, 10, 10, 255));
            let mut app = App::new(doc, None);
            app.tool = Tool::Patch;
            app.replace_sel(Some(Selection::rect(60, 30, 8, 12, 6, 6)));
            app.drag = DragMode::PatchDrag;
            app.drag_start_doc = (10.0, 15.0);
            app.drag_cur_doc = (47.0, 15.0);
            app.mouse_up(0.0, 0.0);
            let patched = app.doc.layers[0].get_pixel(10, 15).to_rgba8().0;
            check("patch tool heals the selection", patched > 35);
        }
        // Pattern Maker installs a pattern.
        {
            let mut app = App::new(flat(), None);
            app.do_action(Action::PatternMakerDialog);
            check("pattern maker dialog opens", app.dialog.is_some());
            if let Some(d) = &mut app.dialog {
                d.opt_a = true;
            }
            app.dialog_ok();
            check("pattern installed", app.doc.pattern.is_some());
            check(
                "pattern maker fill painted the layer",
                app.doc.render_pixel(3, 3).to_rgba8() != (140, 120, 100, 255),
            );
        }
        // --- Batch 7b: paint engine ---
        {
            // Scatter throws stamps outside the tip radius.
            let mut app = App::new(flat(), None);
            app.tool = Tool::Brush;
            app.tip = BrushTip::new(9.0, 1.0);
            app.brush_dyn.scatter = 2.0;
            app.brush_dyn.count = 8;
            app.begin_stroke((20.0, 15.0));
            let mut painted_far = false;
            for y in 0..30 {
                for x in 0..40 {
                    let d = ((x as f32 - 20.0).powi(2) + (y as f32 - 15.0).powi(2)).sqrt();
                    if d > 8.0 && app.doc.layers[0].get_pixel(x, y).to_rgba8().0 < 200 {
                        painted_far = true;
                    }
                }
            }
            check("scatter paints outside the tip", painted_far);
        }
        {
            // Elliptical tip via dynamics shape.
            let mut big = Document::new(40, 40);
            big.layers[0].fill_rect(0, 0, 40, 40, Pixel16::from_rgba8(200, 200, 200, 255));
            let mut app = App::new(big, None);
            app.tool = Tool::Brush;
            app.tip = BrushTip::new(21.0, 1.0);
            app.brush_dyn.roundness = 0.2;
            app.fg = Pixel16::from_rgba8(0, 0, 0, 255);
            app.begin_stroke((20.0, 20.0));
            let wide = app.doc.layers[0].get_pixel(29, 20).to_rgba8().0;
            let tall = app.doc.layers[0].get_pixel(20, 29).to_rgba8().0;
            check("roundness squashes the stamp", wide < 100 && tall == 200);
        }
        {
            // Flow scales the stamp's strength.
            let mk = |flow: f32| {
                let mut app = App::new(flat(), None);
                app.tool = Tool::Brush;
                app.tip = BrushTip::new(13.0, 1.0);
                app.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                app.flow = flow;
                app.begin_stroke((10.0, 10.0));
                app.doc.layers[0].get_pixel(10, 10).to_rgba8().0
            };
            check("flow weakens the stamp", mk(0.2) > mk(1.0));
        }
        {
            // A preset carries its dynamics with it.
            let mut app = App::new(flat(), None);
            app.brush_presets[3].dyn_.scatter = 1.5;
            let cell = app.brushes_cell(3, 0);
            app.panel_mouse(cell.x + 2, cell.y + 2);
            check(
                "preset applies its dynamics",
                app.brush_dyn.scatter == 1.5 && app.tip.size == 9.0,
            );
        }
        {
            // ABR v2 round-trips dynamics.
            let mut presets = vec![BrushPreset {
                size: 9.0,
                hardness: 1.0,
                dyn_: BrushDynamics::default(),
            }];
            presets[0].dyn_.scatter = 0.75;
            presets[0].dyn_.texture_on = true;
            write_abr(std::path::Path::new("/tmp/pd-test2.abr"), &presets).unwrap();
            let back = std::fs::read("/tmp/pd-test2.abr")
                .ok()
                .and_then(|b| read_abr(&b));
            check(
                "abr v2 keeps dynamics",
                back.as_deref() == Some(&presets[..]),
            );
        }
        // --- Batch 7c: Liquify round 2 ---
        {
            let mut app = App::new(flat(), None);
            app.open_liquify_dialog();
            check("liquify opens", app.dialog.is_some());
            if let Some(d) = &mut app.dialog {
                d.liq_tool = 10; // Turbulence
            }
            app.liquify_stamp((15.0, 10.0));
            let (displaced, undos) = {
                let d = app.dialog.as_ref().unwrap();
                (
                    d.liq_disp.iter().any(|v| *v != (0.0, 0.0)),
                    d.liq_undo.len(),
                )
            };
            check("turbulence displaces via the dialog", displaced);
            check("drag start snapshots for undo", undos >= 1);
            // Undo button (index 15) restores the identity field.
            let layout = app.dialog_layout().unwrap();
            let b = layout.range_buttons[15];
            app.mouse_down((b.x + 4) as f32, (b.y + 4) as f32);
            let restored = app
                .dialog
                .as_ref()
                .map(|d| d.liq_disp.iter().all(|v| *v == (0.0, 0.0)))
                .unwrap_or(false);
            check("liquify undo button restores", restored);
            // Mesh save request swaps in the SaveAs dialog and
            // Cancel returns to Liquify.
            let layout = app.dialog_layout().unwrap();
            let b = layout.range_buttons[17];
            app.mouse_down((b.x + 4) as f32, (b.y + 4) as f32);
            check(
                "save mesh opens save-as",
                matches!(
                    app.dialog.as_ref().map(|d| &d.kind),
                    Some(DialogKind::SaveAs)
                ) && app.liq_return.is_some(),
            );
            app.dialog_cancel();
            check(
                "cancel returns to liquify",
                matches!(
                    app.dialog.as_ref().map(|d| &d.kind),
                    Some(DialogKind::Liquify)
                ),
            );
            // Mesh install resamples a foreign-size field.
            let mut f = pictodesk_core::liquify::LiquifyField::new(10, 10);
            f.disp[0] = (2.0, 0.0);
            app.liquify_install_mesh(f);
            let got = app
                .dialog
                .as_ref()
                .map(|d| d.liq_disp[0])
                .unwrap_or((0.0, 0.0));
            check("mesh install resamples", got.0 > 1.0);
            app.dialog_cancel();
        }
        // --- Batch 7d: presets, workspaces, browser, rename ---
        {
            // Tool preset capture/apply round trip.
            let mut app = App::new(flat(), None);
            let n0 = app.tool_presets.len();
            check("seed tool presets exist", n0 >= 5);
            app.tool = Tool::Brush;
            app.tip = BrushTip::new(34.5, 0.42);
            let p = app.capture_tool_preset("Test Preset".into());
            app.tool_presets.push(p);
            app.tip = BrushTip::new(5.0, 1.0);
            app.apply_tool_preset(n0);
            check(
                "tool preset applies tip + tool",
                app.tip.size == 34.5 && app.tool == Tool::Brush,
            );
            check(
                "current preset detected",
                app.current_tool_preset() == Some(n0),
            );
            // Persistence round trip via the dyn block codec
            // (the file format's only non-trivial part).
            let mut bytes = Vec::new();
            dyn_to_bytes(&app.brush_dyn, &mut bytes);
            check(
                "tool preset dyn block round-trips",
                dyn_from_bytes(&bytes, 0).map(|(d, _)| d) == Some(app.brush_dyn.clone()),
            );
            // Workspace capture/apply.
            app.show_histogram = false;
            app.show_tool_presets = true;
            let w = app.capture_workspace();
            app.workspaces.push(("Test WS".into(), w));
            app.show_histogram = true;
            app.show_tool_presets = false;
            app.apply_workspace(app.workspaces.len() - 1);
            check(
                "workspace applies panel set",
                !app.show_histogram && app.show_tool_presets,
            );
            // Rename dialog renames the active layer.
            app.open_rename(0, 0);
            if let Some(d) = &mut app.dialog {
                d.text = "Hero Layer".into();
            }
            app.dialog_ok();
            check(
                "rename renames the layer",
                app.doc.layers[0].name == "Hero Layer",
            );
            // File Browser scans a folder of images.
            let dir = std::path::Path::new("/tmp/pd-browser");
            let _ = std::fs::create_dir_all(dir);
            std::fs::write(dir.join("a.png"), b"\x89PNG").unwrap();
            app.open_file_browser();
            if let Some(d) = &mut app.dialog {
                d.browser_dir = dir.to_string_lossy().into_owned();
            }
            app.browser_rescan();
            let found = app
                .dialog
                .as_ref()
                .map(|d| d.browser_entries.iter().any(|e| e.name == "a.png"))
                .unwrap_or(false);
            check("file browser lists images", found);
            app.dialog_cancel();
        }
        // --- Batch 7e: spell, find/replace, web, package ---
        {
            let mut doc = flat();
            doc.layers[0].text = Some(pictodesk_core::text::TextSpec::new(
                "The quik brown fox jumps",
            ));
            let mut app = App::new(doc, None);
            app.open_spell_check();
            let found = app
                .dialog
                .as_ref()
                .map(|d| {
                    d.spell_items.iter().any(|(_, _, _, w)| w == "quik")
                        && !d.spell_items.iter().any(|(_, _, _, w)| w == "brown")
                })
                .unwrap_or(false);
            check("spell flags only the unknown word", found);
            if let Some(d) = &mut app.dialog {
                if let Some(pos) = d.spell_items.iter().position(|(_, _, _, w)| w == "quik") {
                    d.spell_pos = pos;
                    d.text = "quick".into();
                }
            }
            app.spell_change();
            check(
                "spell change fixes the text",
                app.doc.layers[0]
                    .text
                    .as_ref()
                    .map(|t| t.text.contains("quick"))
                    .unwrap_or(false),
            );
            app.dialog_cancel();
            // Find and Replace across the same layer.
            let mut d2 = Dialog::new(DialogKind::FindReplace, &app.doc);
            d2.text = "fox".into();
            d2.text2 = "wolf".into();
            app.dialog = Some(d2);
            let n = app.find_replace_all();
            check(
                "find/replace swaps the word",
                n == 1
                    && app.doc.layers[0]
                        .text
                        .as_ref()
                        .map(|t| t.text.contains("wolf"))
                        .unwrap_or(false),
            );
            app.dialog_cancel();
        }
        {
            // Save for Web: WBMP header + knockout alpha.
            let mut app = App::new(flat(), None);
            let mut d = Dialog::new(DialogKind::SaveForWeb, &app.doc);
            d.web_format = 3;
            let wb = app.web_export_bytes(&d).unwrap_or_default();
            check(
                "wbmp export has the 1-bit header",
                wb.len() > 4 && wb[0] == 0 && wb[1] == 0,
            );
            d.web_format = 1;
            d.web_knockout = true;
            app.bg = Pixel16::from_rgba8(110, 100, 90, 255);
            let png = app.web_export_bytes(&d).unwrap_or_default();
            let back = pictodesk_core::io::import_image(&png).unwrap();
            check(
                "knockout makes the bg color transparent",
                back.layers[0].get_pixel(0, 0).to_rgba8().3 == 0
                    && back.layers[0].get_pixel(5, 5).to_rgba8().3 == 255,
            );
        }
        {
            // Picture Package: layered + labeled from sources.
            let tiny = Document::new(10, 10);
            let bytes =
                pictodesk_core::io::export_image(&tiny, pictodesk_core::io::ExportFormat::Png, 90)
                    .unwrap();
            std::fs::write("/tmp/pd-pkg-src.png", &bytes).unwrap();
            let mut app = App::new(flat(), None);
            app.pkg_sources = vec![std::path::PathBuf::from("/tmp/pd-pkg-src.png")];
            app.run_picture_package(612, 792, 2, true, true);
            check(
                "picture package builds a layered page",
                app.doc.width == 612 && app.doc.layers.len() == 5,
            );
        }
        // --- Tool sweep: every paint/selection tool through the
        // real mouse path (screen coords -> to_doc -> press/move/up).
        // Banning found tools dead on his Mac (2026-11-25); this
        // suite proves the engine path works so a future failure
        // can only be the event path or the tool's own UI.
        {
            let scr = |app: &App, dx: f32, dy: f32| {
                let c = app.canvas_rect(app.win_size.0, app.win_size.1);
                (
                    c.x as f32 + app.viewport.offset.0 + dx * app.viewport.scale,
                    c.y as f32 + app.viewport.offset.1 + dy * app.viewport.scale,
                )
            };
            let two_tone = || {
                let mut doc = Document::new(30, 20);
                doc.layers[0].fill_rect(0, 0, 15, 20, Pixel16::from_rgba8(255, 0, 0, 255));
                doc.layers[0].fill_rect(15, 0, 15, 20, Pixel16::from_rgba8(0, 0, 255, 255));
                doc
            };
            // Clone Stamp: Alt+click arms it, paint copies red.
            {
                let mut app = App::new(two_tone(), None);
                app.tool = Tool::Stamp;
                app.tip = BrushTip::new(5.0, 1.0);
                let (sx, sy) = scr(&app, 5.0, 10.0);
                app.alt_down = true;
                app.mouse_down(sx, sy);
                check("stamp alt-click sets source", app.clone_src.is_some());
                app.alt_down = false;
                app.mouse_up(sx, sy);
                let (tx, ty) = scr(&app, 22.0, 10.0);
                app.mouse_down(tx, ty);
                app.mouse_move(tx + 4.0, ty);
                app.mouse_up(tx + 4.0, ty);
                let px = app.doc.layers[0].get_pixel(22, 10).to_rgba8();
                check("stamp paints the source color", px.0 > 200 && px.2 < 50);
            }
            // Brush + Eraser round trip.
            {
                let mut doc = Document::new(30, 20);
                doc.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(255, 255, 255, 255));
                let mut app = App::new(doc, None);
                app.tool = Tool::Brush;
                app.tip = BrushTip::new(5.0, 1.0);
                app.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                let (bx, by) = scr(&app, 10.0, 10.0);
                app.mouse_down(bx, by);
                app.mouse_up(bx, by);
                check(
                    "brush paints",
                    app.doc.layers[0].get_pixel(10, 10).to_rgba8().0 < 50,
                );
                app.tool = Tool::Eraser;
                app.mouse_down(bx, by);
                app.mouse_up(bx, by);
                check(
                    "eraser clears to transparent",
                    app.doc.layers[0].get_pixel(10, 10).to_rgba8().3 == 0,
                );
            }
            // Bucket fills, Gradient runs dark to light.
            {
                let mut doc = Document::new(30, 20);
                doc.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(255, 255, 255, 255));
                let mut app = App::new(doc, None);
                app.tool = Tool::Bucket;
                app.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                let (kx, ky) = scr(&app, 10.0, 10.0);
                app.mouse_down(kx, ky);
                app.mouse_up(kx, ky);
                check(
                    "bucket fills",
                    app.doc.layers[0].get_pixel(10, 10).to_rgba8().0 == 0,
                );
                let mut doc2 = Document::new(30, 20);
                doc2.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(255, 255, 255, 255));
                let mut app2 = App::new(doc2, None);
                app2.tool = Tool::Gradient;
                app2.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                app2.bg = Pixel16::from_rgba8(255, 255, 255, 255);
                let (gx, gy) = scr(&app2, 4.0, 10.0);
                app2.mouse_down(gx, gy);
                let (gx2, _) = scr(&app2, 26.0, 10.0);
                app2.mouse_move(gx2, gy);
                app2.mouse_up(gx2, gy);
                let lo = app2.doc.layers[0].get_pixel(5, 10).to_rgba8().0;
                let hi = app2.doc.layers[0].get_pixel(25, 10).to_rgba8().0;
                check("gradient runs dark to light", lo < 100 && hi > 200);
            }
            // Eyedropper, Wand, Marquee, Move.
            {
                let mut app = App::new(two_tone(), None);
                app.tool = Tool::Eyedropper;
                let (ex, ey) = scr(&app, 5.0, 10.0);
                app.mouse_down(ex, ey);
                app.mouse_up(ex, ey);
                check("eyedropper picks the color", app.fg.to_rgba8().0 == 255);
                app.tool = Tool::Wand;
                app.mouse_down(ex, ey);
                app.mouse_up(ex, ey);
                check(
                    "wand selects the region",
                    app.sel
                        .as_ref()
                        .and_then(|s| s.bounds())
                        .map(|(x, _, w, _)| x == 0 && w == 15)
                        .unwrap_or(false),
                );
                app.tool = Tool::MarqueeRect;
                let (mx, my) = scr(&app, 4.0, 4.0);
                app.mouse_down(mx, my);
                let (mx2, my2) = scr(&app, 20.0, 15.0);
                app.mouse_move(mx2, my2);
                app.mouse_up(mx2, my2);
                check(
                    "marquee makes a selection",
                    app.sel
                        .as_ref()
                        .and_then(|s| s.bounds())
                        .map(|(_, _, w, h)| w == 16 && h == 11)
                        .unwrap_or(false),
                );
                app.tool = Tool::Move;
                let (vx, vy) = scr(&app, 10.0, 10.0);
                app.mouse_down(vx, vy);
                let (vx2, vy2) = scr(&app, 15.0, 12.0);
                app.mouse_move(vx2, vy2);
                app.mouse_up(vx2, vy2);
                check("move shifts the layer", app.doc.layers[0].offset == (5, 2));
            }
            // Dodge lightens, Burn darkens.
            {
                let gray = || {
                    let mut doc = Document::new(30, 20);
                    doc.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(100, 100, 100, 255));
                    doc
                };
                let mut app = App::new(gray(), None);
                app.tool = Tool::Dodge;
                app.tip = BrushTip::new(9.0, 1.0);
                let (dx, dy) = scr(&app, 15.0, 10.0);
                app.mouse_down(dx, dy);
                app.mouse_up(dx, dy);
                check(
                    "dodge lightens",
                    app.doc.layers[0].get_pixel(15, 10).to_rgba8().0 > 120,
                );
                let mut app2 = App::new(gray(), None);
                app2.tool = Tool::Burn;
                app2.tip = BrushTip::new(9.0, 1.0);
                let (bx, by) = scr(&app2, 15.0, 10.0);
                app2.mouse_down(bx, by);
                app2.mouse_up(bx, by);
                check(
                    "burn darkens",
                    app2.doc.layers[0].get_pixel(15, 10).to_rgba8().0 < 80,
                );
            }
            // Smudge drags color across the edge; Blur softens it.
            {
                let mut app = App::new(two_tone(), None);
                app.tool = Tool::Smudge;
                app.tip = BrushTip::new(9.0, 1.0);
                let (sx, sy) = scr(&app, 14.0, 10.0);
                app.mouse_down(sx, sy);
                let (sx2, _) = scr(&app, 18.0, 10.0);
                app.mouse_move(sx2, sy);
                app.mouse_up(sx2, sy);
                let px = app.doc.layers[0].get_pixel(17, 10).to_rgba8();
                check("smudge mixes the edge", px.0 > 40 && px.2 > 40);
                let mut app2 = App::new(two_tone(), None);
                app2.tool = Tool::Blur;
                app2.tip = BrushTip::new(9.0, 1.0);
                let (bx, by) = scr(&app2, 15.0, 10.0);
                app2.mouse_down(bx, by);
                app2.mouse_up(bx, by);
                let px2 = app2.doc.layers[0].get_pixel(15, 10).to_rgba8();
                check("blur softens the edge", px2.0 > 40 && px2.0 < 220);
            }
            // Healing Brush: Alt+click a clean source, paint over a
            // black spot, and the spot is gone.
            {
                let mut doc = Document::new(30, 20);
                doc.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(255, 0, 0, 255));
                doc.layers[0].fill_rect(15, 8, 3, 3, Pixel16::from_rgba8(0, 0, 0, 255));
                let mut app = App::new(doc, None);
                app.tool = Tool::Heal;
                app.tip = BrushTip::new(9.0, 1.0);
                let (hx, hy) = scr(&app, 5.0, 10.0);
                app.alt_down = true;
                app.mouse_down(hx, hy);
                app.alt_down = false;
                app.mouse_up(hx, hy);
                let (tx, ty) = scr(&app, 16.0, 9.0);
                app.mouse_down(tx, ty);
                app.mouse_up(tx, ty);
                let px = app.doc.layers[0].get_pixel(16, 9).to_rgba8();
                check("healing removes the spot", px.0 > 100);
            }
        }
        {
            // Modern UI: draggable panel slots — group, float, redock.
            // (HOME is the real one here, so the layout file the
            // checks write is removed again afterwards.)
            let mut app = App::new(flat(), None);
            app.sync_panel_slots();
            check(
                "slots default one kind each",
                app.dock_slots.iter().all(|s| s.kinds.len() == 1) && app.float_slots.is_empty(),
            );
            app.group_panels(PanelKind::Brushes, PanelKind::Histogram);
            let grouped = app.slot_of(PanelKind::Brushes) == app.slot_of(PanelKind::Histogram)
                && app.dock_slots.iter().any(|s| s.kinds.len() == 2);
            check("brushes+histogram grouped in one slot", grouped);
            check(
                "grouped slot shows only the active kind",
                app.group_rect(PanelKind::Histogram).is_some()
                    != app.group_rect(PanelKind::Brushes).is_some(),
            );
            app.float_panel(PanelKind::Navigator, 700, 150);
            check(
                "navigator floated",
                app.float_slots.len() == 1
                    && matches!(app.slot_of(PanelKind::Navigator), Some(SlotLoc::Float(_))),
            );
            let idx = app.float_slots.len() - 1;
            app.redock_float_at(idx, app.top_y() + 10);
            check(
                "navigator redocked",
                app.float_slots.is_empty()
                    && app
                        .dock_slots
                        .iter()
                        .any(|s| s.kinds.contains(&PanelKind::Navigator)),
            );
            let _ = std::fs::remove_file(pictodesk_dir().join("layout.txt"));
            // Tab click switches the grouped slot's active panel.
            {
                let mut app = App::new(flat(), None);
                app.sync_panel_slots();
                app.group_panels(PanelKind::Brushes, PanelKind::Histogram);
                let loc = app.slot_of(PanelKind::Brushes).unwrap();
                let (_, strip) = app
                    .panel_strip_rects()
                    .into_iter()
                    .find(|(l, _)| *l == loc)
                    .unwrap();
                // Click the Brushes tab (first tab, left end).
                let (tx, ty) = (strip.x + 10, strip.y + 8);
                app.mouse_down(tx as f32, ty as f32);
                app.mouse_up(tx as f32, ty as f32);
                check(
                    "tab click switches active panel",
                    app.slot_active_kind(loc) == PanelKind::Brushes
                        && app.group_rect(PanelKind::Brushes).is_some(),
                );
                let _ = std::fs::remove_file(pictodesk_dir().join("layout.txt"));
            }
            // Real drag: press a title handle, move past the
            // threshold onto the canvas, release -> floating.
            // (The dock only fits the first few panels at 800px, so
            // drag the first group entry, whatever kind it is.)
            {
                let mut app = App::new(flat(), None);
                app.sync_panel_slots();
                let (_drag_kind, r) = app.panel_groups().into_iter().next().unwrap();
                let (hx, hy) = (r.x + 6, r.y + 6);
                app.mouse_down(hx as f32, hy as f32);
                check(
                    "title press starts pending drag",
                    app.panel_pending.is_some(),
                );
                app.mouse_move(400.0, 300.0);
                check(
                    "drag past threshold floats the panel",
                    app.panel_drag.is_some() && app.float_slots.len() == 1,
                );
                app.mouse_up(400.0, 300.0);
                check(
                    "drop over canvas leaves it floating",
                    app.float_slots.len() == 1 && app.panel_drop.is_none(),
                );
                let _ = std::fs::remove_file(pictodesk_dir().join("layout.txt"));
            }
        }
        // Document tabs: two docs keep separate pixels, history,
        // and zoom; a dirty tab asks before closing.
        {
            let mut doc1 = Document::new(30, 20);
            doc1.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(255, 0, 0, 255));
            let mut app = App::new(doc1, None);
            app.doc_name = "red.png".into();
            check("one tab at start", app.tab_count() == 1);
            let mut doc2 = Document::new(30, 20);
            doc2.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(0, 0, 255, 255));
            app.new_tab_with_doc(doc2, None, "blue.png".into());
            check(
                "second tab opens",
                app.tab_count() == 2 && app.active_tab == 1,
            );
            app.tool = Tool::Brush;
            app.tip = BrushTip::new(5.0, 1.0);
            app.fg = Pixel16::from_rgba8(0, 0, 0, 255);
            let c = app.canvas_rect(app.win_size.0, app.win_size.1);
            let sp = app.viewport.doc_to_screen((10.0, 10.0));
            app.mouse_down(c.x as f32 + sp.0, c.y as f32 + sp.1);
            app.mouse_up(c.x as f32 + sp.0, c.y as f32 + sp.1);
            check(
                "paint lands on tab 2",
                app.doc.layers[0].get_pixel(10, 10).to_rgba8().0 < 50,
            );
            app.switch_tab(0);
            check(
                "tab 1 pixels untouched",
                app.doc.layers[0].get_pixel(10, 10).to_rgba8() == (255, 0, 0, 255),
            );
            check(
                "tab 1 has no undo from tab 2",
                !app.hist.can_undo() && app.doc_name == "red.png",
            );
            app.switch_tab(1);
            check(
                "back on tab 2, paint and undo survive",
                app.doc.layers[0].get_pixel(10, 10).to_rgba8().0 < 50 && app.hist.can_undo(),
            );
            app.close_tab(1);
            check(
                "closing a dirty tab asks first",
                app.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::ConfirmClose)
                    && app.tab_count() == 2,
            );
            app.dialog_cancel();
            check("cancel keeps the tab", app.tab_count() == 2);
            app.close_tab(1);
            app.dialog_ok();
            check(
                "confirm closes the tab",
                app.tab_count() == 1 && app.active_tab == 0,
            );
            app.viewport = Viewport {
                scale: 2.0,
                offset: (0.0, 0.0),
            };
            let mut doc3 = Document::new(30, 20);
            doc3.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(0, 255, 0, 255));
            app.new_tab_with_doc(doc3, None, "green.png".into());
            let z2 = app.viewport.scale;
            app.switch_tab(0);
            check(
                "zoom stays with its tab",
                app.viewport.scale == 2.0 && z2 != 2.0,
            );
        }
        // Open panel: folders are navigable, the last folder is
        // remembered on disk.
        {
            let base = std::path::PathBuf::from("/tmp/pd-tabs-test");
            let sub = base.join("sub");
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&sub).unwrap();
            let mut doc = Document::new(8, 8);
            doc.layers[0].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(10, 20, 30, 255));
            let bytes = io::export_image(&doc, ExportFormat::Png, 92).unwrap();
            std::fs::write(sub.join("a.png"), bytes).unwrap();
            std::env::set_var("HOME", "/tmp/pdhome-test");
            let _ = std::fs::remove_dir_all("/tmp/pdhome-test");
            let mut app = App::new(flat(), None);
            app.file_panel_dir = base.clone();
            app.file_panel_purpose = 0;
            app.refresh_file_panel();
            let rows = app.file_panel_rows();
            let sub_row = rows.iter().position(|(p, d)| {
                *d && p
                    .as_ref()
                    .map(|p| p.file_name().map(|n| n == "sub").unwrap_or(false))
                    .unwrap_or(false)
            });
            check("panel lists the subfolder", sub_row.is_some());
            let pr = app.file_panel_rect();
            let ry = pr.y + 44 + sub_row.unwrap() as i32 * 17 + 8;
            app.mouse_down(pr.x as f32 + 20.0, ry as f32);
            check(
                "clicking a folder enters it",
                app.file_panel_dir == sub && app.file_panel.is_some(),
            );
            let rows2 = app.file_panel_rows();
            check(
                "entered folder lists its image",
                rows2.iter().any(|(p, d)| {
                    !*d && p
                        .as_ref()
                        .map(|p| p.file_name().map(|n| n == "a.png").unwrap_or(false))
                        .unwrap_or(false)
                }),
            );
            let ry0 = pr.y + 44 + 8;
            app.mouse_down(pr.x as f32 + 20.0, ry0 as f32);
            check(".. goes back up", app.file_panel_dir == base);
            check("last folder is remembered on disk", last_open_dir() == base);
            let _ = std::fs::remove_dir_all(&base);
            let _ = std::fs::remove_dir_all("/tmp/pdhome-test");
            std::env::set_var("HOME", "/home/hatch");
        }
        // --- 0.7.1 six-fixes checks ---------------------------------------
        // Tab bar hides with a single document.
        {
            let mut app = App::new(flat(), None);
            check(
                "single tab hides the tab bar",
                app.tab_count() == 1 && app.top_y() == ui::MENU_H + ui::OPTIONS_H,
            );
            let mut doc2 = Document::new(30, 20);
            doc2.layers[0].fill_rect(0, 0, 30, 20, Pixel16::from_rgba8(0, 0, 255, 255));
            app.new_tab_with_doc(doc2, None, "blue.png".into());
            check(
                "second tab shows the tab bar",
                app.tab_count() == 2 && app.top_y() == ui::MENU_H + ui::OPTIONS_H + TAB_H,
            );
        }
        // Scroll: plain wheel pans, primary + wheel zooms.
        {
            let mut app = App::new(flat(), None);
            let c = app.canvas_rect(app.win_size.0, app.win_size.1);
            app.mouse_move(c.x as f32 + c.w as f32 / 2.0, c.y as f32 + c.h as f32 / 2.0);
            let off0 = app.viewport.offset;
            let sc0 = app.viewport.scale;
            app.scroll(0.0, -1.0);
            check(
                "plain wheel pans the canvas",
                app.viewport.offset.1 < off0.1 && app.viewport.scale == sc0,
            );
            app.ctrl_down = true;
            let sc1 = app.viewport.scale;
            app.scroll(0.0, 1.0);
            check("primary + wheel zooms", app.viewport.scale > sc1);
            app.ctrl_down = false;
        }
        // Panel drag: the whole title row is a handle; a float can merge
        // into another float's group and snap back to the dock.
        {
            let mut app = App::new(flat(), None);
            app.sync_panel_slots();
            let (_, r) = app
                .panel_groups()
                .into_iter()
                .find(|(k, _)| *k != PanelKind::Cp)
                .unwrap();
            app.mouse_down(r.x as f32 + r.w as f32 - 10.0, r.y as f32 + 8.0);
            check(
                "title row drags from its empty end",
                app.panel_pending.is_some(),
            );
            app.mouse_up(r.x as f32, r.y as f32);
            app.float_panel(PanelKind::Navigator, 700, 150);
            app.float_panel(PanelKind::Histogram, 400, 150);
            let strips = app.panel_strip_rects();
            let hist_strip = strips
                .iter()
                .find(|(l, _)| *l == SlotLoc::Float(1))
                .map(|(_, r)| *r)
                .unwrap();
            let drop = app.compute_drop(hist_strip.x + hist_strip.w / 2, hist_strip.y + 8, 0);
            check(
                "a float can merge into another float's group",
                matches!(drop, Some(PanelDrop::Merge(SlotLoc::Float(1)))),
            );
            app.merge_float_into(0, SlotLoc::Float(1));
            check(
                "merged float holds both kinds",
                app.float_slots.len() == 1 && app.float_slots[0].0.kinds.len() == 2,
            );
            let mut snap_y = app.top_y() + 10;
            for y in (app.top_y()..app.win_size.1 as i32 - ui::STATUS_H).step_by(20) {
                let p = (app.win_size.0 as i32 - 10, y);
                if !strips.iter().any(|(_, r)| r.contains(p.0, p.1)) {
                    snap_y = y;
                    break;
                }
            }
            let drop2 = app.compute_drop(app.win_size.0 as i32 - 10, snap_y, 0);
            check(
                "dock column snaps a float back",
                matches!(drop2, Some(PanelDrop::DockAt(_))),
            );
        }
        // Tooltips: tool name + shortcut over the toolbox and Options Bar.
        {
            let mut app = App::new(flat(), None);
            let bi = TOOLS.iter().position(|t| *t == Tool::Brush).unwrap();
            let br = App::tool_rect(bi, app.top_y());
            app.mouse_move(br.x as f32 + 8.0, br.y as f32 + 8.0);
            check(
                "toolbox hover names the tool and its key",
                app.hover_text.as_deref() == Some("Paintbrush (B)"),
            );
            let act = app
                .options_items()
                .into_iter()
                .find(|(_, _, _, a)| *a == OptAct::TipSizeDown)
                .map(|(r, _, _, _)| r);
            if let Some(r) = act {
                app.mouse_move(r.x as f32 + 4.0, r.y as f32 + 4.0);
                check(
                    "options bar hover names the control",
                    app.hover_text.as_deref() == Some("Brush size"),
                );
            } else {
                check("options bar hover names the control", false);
            }
        }
        // Shortcuts: defaults, dispatch, capture + steal, persistence.
        {
            std::env::set_var("HOME", "/tmp/pdhome-test");
            let _ = std::fs::remove_dir_all("/tmp/pdhome-test");
            let mut app = App::new(flat(), None);
            check(
                "shortcut defaults carry the tool groups",
                default_shortcuts()
                    .iter()
                    .find(|d| d.id == "tool:b")
                    .map(|d| d.combo.as_str())
                    == Some("b"),
            );
            app.tool = Tool::Move;
            app.key_char('b');
            check("pressing b selects the Paintbrush", app.tool == Tool::Brush);
            app.key_char('b');
            check(
                "pressing b again cycles to Pencil",
                app.tool == Tool::Pencil,
            );
            app.open_dialog(DialogKind::Preferences);
            if let Some(d) = &mut app.dialog {
                d.fx_page = 1;
            }
            let brow = app.shortcuts.iter().position(|d| d.id == "tool:b").unwrap();
            app.prefs_capture = Some(brow);
            app.dispatch_combo("x");
            check(
                "capturing a taken key asks first",
                app.prefs_confirm == Some((brow, "x".to_string()))
                    && app.shortcuts[brow].combo == "b",
            );
            app.dispatch_combo("x");
            check(
                "second press steals the key",
                app.shortcuts[brow].combo == "x"
                    && app
                        .shortcuts
                        .iter()
                        .find(|d| d.id == "swap-colors")
                        .map(|d| d.combo.as_str())
                        == Some(""),
            );
            app.shortcuts = default_shortcuts();
            app.save_shortcuts_file();
            let loaded = load_shortcuts();
            check(
                "shortcuts persist and reload",
                loaded
                    .iter()
                    .find(|d| d.id == "tool:b")
                    .map(|d| d.combo.as_str())
                    == Some("b"),
            );
            let _ = std::fs::remove_dir_all("/tmp/pdhome-test");
            std::env::set_var("HOME", "/home/hatch");
        }
        // Preferences dialog: pages, list click captures a key.
        {
            let mut app = App::new(flat(), None);
            app.open_dialog(DialogKind::Preferences);
            let layout = app.dialog_layout().unwrap();
            check(
                "preferences opens on the General page",
                app.dialog.as_ref().map(|d| d.fx_page) == Some(0)
                    && layout.range_buttons.len() == 7,
            );
            let tab = layout.range_buttons[1];
            app.mouse_down(tab.x as f32 + 10.0, tab.y as f32 + 9.0);
            app.mouse_up(tab.x as f32 + 10.0, tab.y as f32 + 9.0);
            let layout2 = app.dialog_layout().unwrap();
            check(
                "shortcuts tab shows the list, no sliders",
                app.dialog.as_ref().map(|d| d.fx_page) == Some(1)
                    && layout2.range_buttons.len() == 2
                    && app.dialog.as_ref().map(|d| d.slider_defs().is_empty()) == Some(true),
            );
            let r = layout2.rect;
            app.mouse_down(r.x as f32 + 20.0, r.y as f32 + 52.0 + 7.0);
            app.mouse_up(r.x as f32 + 20.0, r.y as f32 + 52.0 + 7.0);
            check(
                "clicking a shortcut row starts capture",
                app.prefs_capture == Some(0),
            );
            app.dialog_cancel();
            check(
                "closing the dialog clears capture",
                app.prefs_capture.is_none(),
            );
        }

        eprintln!("PHASE7TEST {}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            std::process::exit(1);
        }
    }
    if phase65test {
        // Phase 6.5 Batch 1a: the new gallery filters apply
        // end-to-end through their dialogs / direct actions.
        let mut ok = true;
        let mut check = |name: &str, cond: bool| {
            eprintln!("PHASE65 {name}: {}", if cond { "ok" } else { "FAIL" });
            ok = ok && cond;
        };
        let content = || {
            let mut doc = Document::new(40, 30);
            doc.layers[0].fill_rect(0, 0, 40, 30, Pixel16::from_rgba8(200, 200, 200, 255));
            doc.layers[0].fill_rect(4, 4, 14, 10, Pixel16::from_rgba8(20, 40, 180, 255));
            doc.layers[0].fill_rect(22, 12, 12, 12, Pixel16::from_rgba8(255, 128, 0, 255));
            doc
        };
        for id in 16..=22u8 {
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::LegacyFilterDialog(id));
            app.dialog_ok();
            check(
                &format!("gallery filter {id} ({}) applies", fx_title(id)),
                app.doc.render_checksum() != before,
            );
        }
        for w in 4..=7u8 {
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::FilterSpecial(w));
            check(
                &format!("direct filter {w} applies"),
                app.doc.render_checksum() != before,
            );
        }
        {
            // Batch 6b: Fade after a blur, then re-fade.
            let mut app = App::new(content(), None);
            let pre_blur = app.doc.render_pixel(24, 16).to_rgba8();
            app.filter_now("Gaussian Blur", None, |d, l, s| {
                filter::gaussian_blur(d, l, s, 3.0)
            });
            check(
                "fade source captured after a filter",
                app.fade_source.is_some(),
            );
            let after_blur = app.doc.render_pixel(24, 16).to_rgba8();
            check("blur changed the pixel", after_blur != pre_blur);
            app.open_dialog(DialogKind::Fade);
            if let Some(d) = &mut app.dialog {
                d.vals[0] = 0.0; // fade fully back
            }
            app.dialog_refresh_preview();
            app.dialog_ok();
            let faded = app.doc.render_pixel(24, 16).to_rgba8();
            check(
                "fade at 0% restores the pre-filter pixel",
                faded == pre_blur && app.last_label.as_deref() == Some("Fade"),
            );
            check("last filter tracked", app.last_filter_step.is_none());
        }
        {
            // Batch 6b: Stroke dialog paints the selection edge.
            let mut app = App::new(content(), None);
            let mut sel = Selection::none(48, 32);
            for y in 8..24 {
                for x in 8..40 {
                    sel.set(x, y, 255);
                }
            }
            app.sel = Some(sel);
            app.open_dialog(DialogKind::StrokeSel);
            app.dialog_ok();
            let edge = app.doc.layers[0].get_pixel(8, 16).to_rgba8();
            check("stroke dialog paints the edge", edge.3 == 255);
        }
        {
            // Batch 6b: Rotate Arbitrary swaps a 48x32 canvas.
            let mut app = App::new(content(), None);
            app.open_dialog(DialogKind::RotateArbitrary);
            if let Some(d) = &mut app.dialog {
                d.vals[0] = 90.0;
            }
            app.dialog_ok();
            check(
                "rotate arbitrary 90 swaps dimensions",
                (app.doc.width, app.doc.height) == (30, 40),
            );
        }
        {
            // Batch 6b: File Info + Preferences + Defringe.
            let mut app = App::new(content(), None);
            app.open_dialog(DialogKind::FileInfo);
            if let Some(d) = &mut app.dialog {
                d.text = "Sunset over the bay".into();
            }
            app.dialog_ok();
            check(
                "file info title saved to the document",
                app.doc.info_title == "Sunset over the bay",
            );
            app.open_dialog(DialogKind::Preferences);
            if let Some(d) = &mut app.dialog {
                d.opt_b = false;
            }
            app.dialog_ok();
            check("preferences toggle the options bar", !app.show_options_bar);
            // Restore the options bar: this toggle must not leak
            // into the real prefs file for the blocks below.
            app.show_options_bar = true;
            app.save_prefs_file();
            app.open_dialog(DialogKind::Defringe);
            app.dialog_ok();
            check(
                "defringe dialog runs",
                app.last_label.as_deref() == Some("Defringe"),
            );
        }
        {
            // Batch 6b: Define Brush + Save a Copy + neutral layer.
            let mut app = App::new(content(), None);
            app.sel = Some(Selection::all(48, 32));
            app.do_action(Action::DefineBrush);
            check("define brush captures a mask tip", app.tip.mask.is_some());
            let before_path = app.file_path.clone();
            app.save_copy_to(std::path::Path::new("/tmp/pictodesk-copy-test.pdk"));
            check(
                "save a copy writes without taking the path",
                std::path::Path::new("/tmp/pictodesk-copy-test.pdk").exists()
                    && app.file_path == before_path,
            );
            app.do_action(Action::NewLayer);
            if let Some(d) = &mut app.dialog {
                d.fx_b = true;
                d.text = "Neutral".into();
            }
            app.dialog_ok();
            let top = app.doc.layers.len() - 1;
            let gp = app.doc.layers[top].get_pixel(4, 4).to_rgba8();
            check(
                "new layer neutral fill is mid gray",
                (gp.0, gp.1, gp.2, gp.3) == (128, 128, 128, 255),
            );
        }
        {
            // Batch 6c: single row/column marquees.
            let mut app = App::new(content(), None);
            app.tool = Tool::MarqueeRow;
            app.drag = DragMode::SelectRect;
            app.drag_cur_doc = (5.0, 7.0);
            app.mouse_up(0.0, 0.0);
            check(
                "single row marquee selects a 1px full-width row",
                app.sel.as_ref().and_then(|s| s.bounds()) == Some((0, 7, 40, 1)),
            );
            app.tool = Tool::MarqueeCol;
            app.drag = DragMode::SelectRect;
            app.drag_cur_doc = (9.0, 3.0);
            app.mouse_up(0.0, 0.0);
            check(
                "single column marquee selects a 1px full-height column",
                app.sel.as_ref().and_then(|s| s.bounds()) == Some((9, 0, 1, 30)),
            );
        }
        {
            // Batch 6c: airbrush builds up while held.
            let mut app = App::new(content(), None);
            app.tool = Tool::Airbrush;
            app.begin_stroke((10.0, 8.0));
            app.drag = DragMode::Paint;
            let after_one = app.doc.layers[0].get_pixel(10, 8).to_rgba8().0;
            for _ in 0..12 {
                app.airbrush_tick();
            }
            let after_hold = app.doc.layers[0].get_pixel(10, 8).to_rgba8().0;
            check(
                "airbrush hold darkens toward the paint color",
                after_hold < after_one,
            );
            app.mouse_up(0.0, 0.0);
        }
        {
            // Batch 6c: convert anchor collapses a smooth point.
            let mut app = App::new(content(), None);
            app.doc.work_path = Some(BezPath {
                name: "Work Path".into(),
                subpaths: vec![Subpath {
                    anchors: vec![
                        Anchor::corner(4.0, 4.0),
                        Anchor::smooth((20.0, 15.0), (12.0, 15.0), (28.0, 15.0)),
                    ],
                    closed: false,
                }],
            });
            app.active_path = Some(PathRef::Work);
            let c = app.canvas_rect(app.win_size.0, app.win_size.1);
            let sp = app.viewport.doc_to_screen((20.0, 15.0));
            app.convert_anchor_down(c.x as f32 + sp.0, c.y as f32 + sp.1);
            let a = &app.doc.work_path.as_ref().unwrap().subpaths[0].anchors[1];
            check("convert anchor makes a corner point", a.is_corner());
        }
        {
            // Batch 6c: gamut warning grays saturated blue; print
            // size zoom; thumb sizes; guide lock.
            let mut app = App::new(content(), None);
            app.gamut_warning = true;
            app.ensure_render_cache();
            let idx = (8 * 40 + 10) * 4;
            check(
                "gamut warning paints out-of-gamut pixels gray",
                app.cached_render[idx] == 160 && app.cached_render[idx + 2] == 160,
            );
            app.do_action(Action::ZoomPrintSize);
            check(
                "print size view uses 96/72 scale",
                (app.viewport.scale - 96.0 / 72.0).abs() < 0.001,
            );
            app.layer_thumb = 2;
            check("large layer thumbs give 54px rows", app.layer_row_h() == 54);
            app.do_action(Action::ToggleLockGuides);
            check("lock guides toggles", app.guides_locked);
        }
        {
            // Batch 6d: step serialization round-trips every
            // payload shape (the droplet/persistence format).
            use pictodesk_core::adjust::{Adjustment, ChannelMixer};
            let mut fx = pictodesk_core::effects::Effects::default();
            fx.drop_shadow.enabled = true;
            fx.drop_shadow.distance = 9;
            fx.bevel.enabled = true;
            fx.gradient_overlay.angle = -30;
            let mut mixer = ChannelMixer::default();
            mixer.rows[0][1] = 0.5;
            mixer.monochrome = true;
            let steps = vec![
                RecordedStep::Adjustment(Adjustment::Curves {
                    points: vec![(0, 0), (128, 140), (255, 255)],
                }),
                RecordedStep::Adjustment(Adjustment::ColorBalance {
                    shadows: (5, 0, -5),
                    midtones: (0, 10, 0),
                    highlights: (-3, 0, 3),
                }),
                RecordedStep::AdjustmentLayer(Adjustment::Levels {
                    in_black: 10,
                    in_white: 240,
                    gamma: 1.2,
                    out_black: 0,
                    out_white: 255,
                }),
                RecordedStep::ChannelMixer(mixer),
                RecordedStep::SetEffects(fx),
                RecordedStep::Displace(vec![0.5, -0.25, 1.0], 4, 3, 50.0, 25.0, true, false),
                RecordedStep::TextureFill(vec![0.1, 0.9], 2, 2),
                RecordedStep::CustomKernel(
                    [
                        0, -1, 0, -1, 5, -1, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0,
                    ],
                    1,
                    0,
                ),
                RecordedStep::Shear([0.1, 0.0, -0.2, 0.0, 0.05]),
                RecordedStep::SelectiveColor(0, [10, -5, 0, 3], true),
                RecordedStep::ReplaceColor((10, 20, 30), 40, 5, -10, 0),
                RecordedStep::Wave(3, 10.0, 5.0, 1.0, 99.0, 2, true),
                RecordedStep::ChalkCharcoal(
                    10.0,
                    20.0,
                    30.0,
                    Pixel16::from_rgba8(1, 2, 3, 255),
                    Pixel16::from_rgba8(250, 251, 252, 255),
                ),
                RecordedStep::CanvasSize(100, 80, (5, -3)),
                RecordedStep::ImageSize(64, 48),
                RecordedStep::LayerSelect(2),
                RecordedStep::Stop("Check the proof".into()),
                RecordedStep::MenuCommand("Filter > Blur".into()),
            ];
            let mut all_ok = true;
            for st in &steps {
                let l1 = step_to_line(st).unwrap_or_default();
                let back = line_to_step(&l1);
                let l2 = back.as_ref().and_then(step_to_line).unwrap_or_default();
                if l1.is_empty() || l1 != l2 {
                    eprintln!("round trip mismatch: {l1:?} -> {l2:?}");
                    all_ok = false;
                }
            }
            check("step serialization round-trips all payload shapes", all_ok);
        }
        {
            // Batch 6d: actions persistence, Stop playback,
            // Insert Stop.
            std::env::set_var("HOME", "/tmp/pdhome-test");
            // Test isolation: App::new loads any saved actions,
            // so start from a clean home or re-runs see the
            // previous run's file.
            std::fs::remove_dir_all("/tmp/pdhome-test").ok();
            let mut app = App::new(content(), None);
            app.actions.push(RecordedAction {
                name: "Demo".into(),
                steps: vec![
                    RecordedStep::Invert,
                    RecordedStep::Stop("hold on".into()),
                    RecordedStep::MenuCommand("Edit > Undo".into()),
                ],
                set: "Default Set".into(),
            });
            app.save_actions_file();
            let (sets, loaded) = load_actions_state();
            check(
                "actions persist to disk and reload",
                sets.iter().any(|x| x == "Default Set")
                    && loaded.len() == 1
                    && loaded[0].steps.len() == 3
                    && loaded[0].steps[1].label() == "Stop: hold on",
            );
            // Playback pauses at the Stop.
            let before = app.doc.render_checksum();
            app.play_action(0);
            check(
                "playback pauses at a Stop with its dialog",
                app.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::ActionStop)
                    && app.doc.render_checksum() != before
                    && app.play_pending.is_some(),
            );
            app.dialog_cancel();
            check(
                "cancelling a Stop ends playback",
                app.play_pending.is_none(),
            );
            // Insert Stop appends to the selected action.
            app.open_insert_stop();
            if let Some(d) = &mut app.dialog {
                d.text = "inserted".into();
            }
            app.dialog_ok();
            check(
                "insert stop appends a Stop step",
                matches!(
                    app.actions[0].steps.last(),
                    Some(RecordedStep::Stop(m)) if m == "inserted"
                ),
            );
            std::env::set_var("HOME", "/home/hatch");
        }
        {
            // Batch 5: Options Bar + screen modes.
            let mut app = App::new(content(), None);
            app.win_size = (1280, 800);
            app.tool = Tool::Brush;
            let items = app.options_items();
            let size0 = app.tip.size;
            let up = items
                .iter()
                .find(|(_, _, _, a)| *a == OptAct::TipSizeUp)
                .map(|(r, _, _, _)| *r);
            check(
                "options bar offers brush steppers",
                items.len() >= 6 && up.is_some(),
            );
            if let Some(r) = up {
                app.options_press(r.x + 8, r.y + 9);
            }
            check(
                "options bar size stepper grows the tip",
                app.tip.size > size0,
            );
            let std_h = app.canvas_rect(1280, 800).h;
            app.do_action(Action::ScreenMode(2));
            let full_h = app.canvas_rect(1280, 800).h;
            check(
                "full screen grows the canvas and hides panels",
                full_h > std_h && app.panel_groups().is_empty(),
            );
            app.do_action(Action::ScreenMode(0));
            check(
                "standard mode restores the canvas",
                app.canvas_rect(1280, 800).h == std_h,
            );
        }
        {
            // Batch 4: Selective Color + Replace Color dialogs.
            let mut app = App::new(content(), None);
            app.do_action(Action::SelectiveColorDialog);
            if let Some(d) = &mut app.dialog {
                d.fx_a = 4; // Blues
                d.vals[2] = 100.0; // +Yellow pushes blue down
            }
            app.dialog_refresh_preview();
            app.dialog_ok();
            check("selective color dialog commits", app.dialog.is_none());
            let before = app.doc.layers[app.doc.active_layer]
                .get_pixel(4, 4)
                .to_rgba8();
            app.do_action(Action::ReplaceColorDialog);
            if let Some(d) = &mut app.dialog {
                let (r, g, b, _) = before;
                d.rc_target = (r, g, b);
                d.vals[0] = 120.0;
                d.vals[2] = -100.0;
            }
            app.dialog_refresh_preview();
            app.dialog_ok();
            let after = app.doc.layers[app.doc.active_layer]
                .get_pixel(4, 4)
                .to_rgba8();
            check("replace color desaturates its target", after != before);
        }
        {
            // Batch 4: Variations clicks + Print page render.
            let mut app = App::new(content(), None);
            let before = app.doc.render_pixel(4, 4).to_rgba8();
            app.do_action(Action::VariationsDialog);
            app.var_apply_click(8); // Lighter
            app.dialog_ok();
            let after = app.doc.render_pixel(4, 4).to_rgba8();
            check(
                "variations lighter click commits",
                after.0 >= before.0 && after.1 >= before.1,
            );
            app.print_marks = 1; // crop marks
            app.print_negative = true;
            let (page, pw, ph) = app.print_page_rgba();
            check(
                "print page renders with marks",
                pw == 612 && ph == 792 && page.len() == (pw * ph * 4) as usize,
            );
        }
        {
            // Batch 3: channel dialogs + commands.
            let mut app = App::new(content(), None);
            app.do_action(Action::ApplyImageDialog);
            if let Some(d) = &mut app.dialog {
                d.blend_idx = 1; // Multiply
            }
            let before = app.doc.layers[app.doc.active_layer]
                .get_pixel(2, 2)
                .to_rgba8();
            app.dialog_ok();
            let after = app.doc.layers[app.doc.active_layer]
                .get_pixel(2, 2)
                .to_rgba8();
            check(
                "apply image multiply runs",
                app.dialog.is_none() && after.0 <= before.0,
            );
            let n_alpha = app.doc.alphas.len();
            app.do_action(Action::CalculationsDialog);
            app.dialog_ok();
            check(
                "calculations makes a new channel",
                app.doc.alphas.len() == n_alpha + 1
                    && app.doc.alphas.last().map(|c| c.name.as_str()) == Some("Calc Result"),
            );
            app.do_action(Action::ChannelOptionsDialog);
            if let Some(d) = &mut app.dialog {
                d.text = "Renamed".into();
            }
            app.dialog_ok();
            check(
                "channel options renames the channel",
                app.doc.alphas.last().map(|c| c.name.as_str()) == Some("Renamed"),
            );
        }
        {
            let mut app = App::new(content(), None);
            let (dw, dh) = (app.doc.width, app.doc.height);
            app.sel = Some(pictodesk_core::selection::Selection::rect(
                dw, dh, 2, 2, 10, 10,
            ));
            app.do_action(Action::SaveSelection);
            app.sel = Some(pictodesk_core::selection::Selection::rect(
                dw, dh, 30, 2, 10, 10,
            ));
            app.do_action(Action::LoadSelectionDialog);
            if let Some(d) = &mut app.dialog {
                d.load_op = 1; // Add
            }
            app.dialog_ok();
            let v = app.sel.as_ref().map(|s| (s.value(5, 5), s.value(35, 5)));
            check("load selection Add unions", v == Some((255, 255)));
            // Quick Mask round trip.
            app.do_action(Action::QuickMaskToggle);
            let on = app.quick_mask && app.doc.alphas.iter().any(|c| c.name == "Quick Mask");
            app.do_action(Action::QuickMaskToggle);
            check(
                "quick mask enters and exits to a selection",
                on && !app.quick_mask && app.sel.is_some(),
            );
            app.do_action(Action::PurgeHistories);
            check(
                "purge histories empties states",
                app.hist.state_count() == 0,
            );
        }
        {
            let mut app = App::new(content(), None);
            let before = app.doc.render_pixel(4, 4).to_rgba8();
            app.do_action(Action::SplitChannels);
            let n = app.split_docs.len();
            app.do_action(Action::MergeChannelsDialog);
            app.dialog_ok();
            let after = app.doc.render_pixel(4, 4).to_rgba8();
            check(
                "split then merge restores the image",
                n == 3
                    && app.doc.mode == ColorMode::Rgb
                    && (before.0 as i32 - after.0 as i32).abs() <= 2
                    && (before.1 as i32 - after.1 as i32).abs() <= 2,
            );
        }
        {
            let mut app = App::new(content(), None);
            app.do_action(Action::ModeLab);
            check(
                "mode converts to Lab and back",
                app.doc.mode == ColorMode::Lab,
            );
            app.do_action(Action::ModeRgb);
            check("mode converts back to RGB", app.doc.mode == ColorMode::Rgb);
            app.do_action(Action::BitmapDialog);
            app.dialog_ok();
            let bw_only = (0..app.doc.width as i32).all(|x| {
                let p = app.doc.render_pixel(x, 4).to_rgba8();
                (p.0 == 0 || p.0 == 255) && p.0 == p.1 && p.1 == p.2
            });
            check(
                "bitmap dialog converts to pure black/white",
                app.doc.mode == ColorMode::Bitmap && bw_only,
            );
        }
        {
            let mut app = App::new(content(), None);
            app.do_action(Action::IndexedDialog);
            app.dialog_ok();
            check(
                "indexed dialog builds a palette",
                app.doc.mode == ColorMode::Indexed && !app.doc.color_table.is_empty(),
            );
            app.do_action(Action::ColorTableDialog);
            check("color table dialog opens", app.dialog.is_some());
            app.dialog_cancel();
            app.do_action(Action::DuotoneDialog);
            // The doc is Indexed now; the duotone path converts
            // through grayscale inside its OK handler.
            app.dialog_ok();
            check(
                "duotone dialog applies a spec",
                app.doc.mode == ColorMode::Duotone && app.doc.duotone.is_some(),
            );
        }
        {
            let mut app = App::new(content(), None);
            app.do_action(Action::ModeMultichannel);
            check(
                "multichannel builds four planes",
                app.doc.mode == ColorMode::Multichannel && app.doc.multi_planes.len() == 4,
            );
        }
        for id in 52..=68u8 {
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::LegacyFilterDialog(id));
            app.dialog_ok();
            check(
                &format!("sketch/texture filter {id} ({}) applies", fx_title(id)),
                app.doc.render_checksum() != before,
            );
        }
        for id in 44..=51u8 {
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::LegacyFilterDialog(id));
            app.dialog_ok();
            check(
                &format!("brush filter {id} ({}) applies", fx_title(id)),
                app.doc.render_checksum() != before,
            );
        }
        for id in 29..=43u8 {
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::LegacyFilterDialog(id));
            app.dialog_ok();
            check(
                &format!("artistic filter {id} ({}) applies", fx_title(id)),
                app.doc.render_checksum() != before,
            );
        }
        for id in [23u8, 24, 25, 27, 28] {
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.do_action(Action::LegacyFilterDialog(id));
            app.dialog_ok();
            check(
                &format!("gallery filter {id} ({}) applies", fx_title(id)),
                app.doc.render_checksum() != before,
            );
        }
        // Displace + Texture Fill run through the file route:
        // write a half-black / half-white map, then pick it.
        {
            let mut mapdoc = Document::new(16, 16);
            mapdoc.layers[0].fill_rect(0, 0, 8, 16, Pixel16::from_rgba8(0, 0, 0, 255));
            mapdoc.layers[0].fill_rect(8, 0, 8, 16, Pixel16::from_rgba8(255, 255, 255, 255));
            let bytes = pictodesk_core::io::export_image(
                &mapdoc,
                pictodesk_core::io::ExportFormat::Png,
                90,
            )
            .unwrap_or_default();
            std::fs::write("/tmp/pd-map.png", &bytes).ok();
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.pending_displace = Some((50.0, 50.0, false, false));
            app.file_panel_purpose = 1;
            app.take_map_file(std::path::Path::new("/tmp/pd-map.png"));
            check(
                "displace via picked map applies",
                app.doc.render_checksum() != before,
            );
            let mut app = App::new(content(), None);
            let before = app.doc.render_checksum();
            app.file_panel_purpose = 2;
            app.take_map_file(std::path::Path::new("/tmp/pd-map.png"));
            check(
                "texture fill via picked file applies",
                app.doc.render_checksum() != before,
            );
        }
        // Batch 6d: libraries, settings, paths export.
        {
            // .ACO round trip
            let cols = vec![(10u8, 20u8, 30u8), (200, 100, 50), (0, 0, 0)];
            write_aco(std::path::Path::new("/tmp/pd-test.aco"), &cols).unwrap();
            let back = std::fs::read("/tmp/pd-test.aco")
                .ok()
                .and_then(|b| read_aco(&b));
            check("aco round trip", back.as_deref() == Some(&cols[..]));
            // .ABR round trip
            let presets: Vec<BrushPreset> = [(9.0f32, 1.0f32), (21.0, 0.0), (3.5, 0.5)]
                .iter()
                .map(|(sz, h)| BrushPreset {
                    size: *sz,
                    hardness: *h,
                    dyn_: BrushDynamics::default(),
                })
                .collect();
            write_abr(std::path::Path::new("/tmp/pd-test.abr"), &presets).unwrap();
            let back = std::fs::read("/tmp/pd-test.abr")
                .ok()
                .and_then(|b| read_abr(&b));
            check("abr round trip", back.as_deref() == Some(&presets[..]));
            // Palette loads through the panel route
            let mut app = App::new(content(), None);
            app.load_swatches_file(std::path::Path::new("/tmp/pd-test.aco"));
            check("palette loaded from aco", app.palette == cols);
            app.load_brushes_file(std::path::Path::new("/tmp/pd-test.abr"));
            check(
                "brush presets loaded from abr",
                app.brush_presets == presets,
            );
            // Settings: Levels step -> dialog fields
            let mut app = App::new(content(), None);
            app.apply_step_to_dialog(RecordedStep::Adjustment(
                pictodesk_core::adjust::Adjustment::Levels {
                    in_black: 12,
                    in_white: 230,
                    gamma: 1.4,
                    out_black: 5,
                    out_white: 250,
                },
            ));
            let vals_ok = app
                .dialog
                .as_ref()
                .map(|d| {
                    d.kind == DialogKind::Levels && d.vals == vec![12.0, 230.0, 1.4, 5.0, 250.0]
                })
                .unwrap_or(false);
            check("settings load restores levels dialog", vals_ok);
            // Settings file round trip through the SaveAs mode 4
            let mut app = App::new(content(), None);
            app.settings_step = Some(RecordedStep::Adjustment(
                pictodesk_core::adjust::Adjustment::BrightnessContrast {
                    brightness: 25,
                    contrast: -10,
                },
            ));
            app.open_save_dialog(4, "/tmp/pd-settings.txt");
            app.dialog_ok();
            let text = std::fs::read_to_string("/tmp/pd-settings.txt").unwrap_or_default();
            let expect =
                RecordedStep::Adjustment(pictodesk_core::adjust::Adjustment::BrightnessContrast {
                    brightness: 25,
                    contrast: -10,
                });
            check(
                "settings file written",
                line_to_step(text.trim()).and_then(|st| step_to_line(&st)) == step_to_line(&expect),
            );
            let mut app = App::new(content(), None);
            app.load_settings_file(std::path::Path::new("/tmp/pd-settings.txt"));
            let ok2 = app
                .dialog
                .as_ref()
                .map(|d| d.kind == DialogKind::BrightnessContrast && d.vals == vec![25.0, -10.0])
                .unwrap_or(false);
            check("settings file loads into dialog", ok2);
            // Paths export
            let mut app = App::new(content(), None);
            let anch = |x: f32, y: f32| pictodesk_core::path::Anchor {
                point: (x, y),
                in_ctrl: (x - 4.0, y),
                out_ctrl: (x + 4.0, y),
            };
            app.doc.paths.push(pictodesk_core::path::Path {
                name: "P1".into(),
                subpaths: vec![pictodesk_core::path::Subpath {
                    anchors: vec![anch(2.0, 2.0), anch(20.0, 10.0)],
                    closed: false,
                }],
            });
            let ai = paths_to_ai(&app.doc);
            check(
                "paths.ai has header + curveto",
                ai.starts_with("%!PS-Adobe-3.0") && ai.contains("curveto"),
            );
            app.open_save_dialog(5, "/tmp/pd-paths.ai");
            app.dialog_ok();
            check(
                "paths.ai file written",
                std::fs::read_to_string("/tmp/pd-paths.ai")
                    .map(|t| t.contains("curveto"))
                    .unwrap_or(false),
            );
            // SaveAs mode 2 writes the palette
            let mut app = App::new(content(), None);
            app.open_save_dialog(2, "/tmp/pd-pal.aco");
            app.dialog_ok();
            check(
                "saveas mode 2 writes aco",
                std::fs::read("/tmp/pd-pal.aco")
                    .map(|b| b.len() > 100)
                    .unwrap_or(false),
            );
        }
        eprintln!("PHASE65TEST {}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            return 1;
        }
    }
    // UI verification: pose a tool + cursor so the shot shows the
    // brush ring / clone crosshair the user actually sees.
    if let Some(tool_name) = &uitool {
        let scr = |app: &App, dx: f32, dy: f32| {
            let c = app.canvas_rect(app.win_size.0, app.win_size.1);
            (
                c.x as f32 + app.viewport.offset.0 + dx * app.viewport.scale,
                c.y as f32 + app.viewport.offset.1 + dy * app.viewport.scale,
            )
        };
        match tool_name.as_str() {
            "stamp" => {
                app.tool = Tool::Stamp;
                app.tip = BrushTip::new(27.0, 1.0);
                app.clone_src = Some((100.0, 100.0));
                app.cursor = scr(&app, 400.0, 200.0);
            }
            "brush" => {
                app.tool = Tool::Brush;
                app.tip = BrushTip::new(40.0, 1.0);
                app.cursor = scr(&app, 400.0, 200.0);
            }
            _ => {}
        }
    }
    // UI verification: a grouped dock slot (Brushes + Histogram)
    // and a floating Navigator, so the shot shows tabs + a float.
    // The demo layout is never left on disk: stash layout.txt
    // first, restore it after the frame is drawn.
    let layout_before = std::fs::read(pictodesk_dir().join("layout.txt")).ok();
    if uipanels {
        app.sync_panel_slots();
        app.group_panels(PanelKind::Brushes, PanelKind::Histogram);
        if let Some(loc) = app.slot_of(PanelKind::Brushes) {
            app.set_slot_active(loc, PanelKind::Brushes);
        }
        app.float_panel(PanelKind::Navigator, 700, 150);
    }
    if twotabs {
        app.new_tab_with_doc(
            demo_document(),
            Some(PathBuf::from("photo.png")),
            "photo.png".into(),
        );
    }
    if uitooltip {
        let bi = TOOLS.iter().position(|t| *t == Tool::Brush).unwrap();
        let br = App::tool_rect(bi, app.top_y());
        app.cursor = (br.x as f32 + 8.0, br.y as f32 + 8.0);
        app.hover_text = Some("Paintbrush (B)".into());
        app.hover_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
    }
    if let Some(m) = menu {
        app.open_menu = Some(m);
        app.hover_item = Some((m, 0));
        // open the first submenu too, so the shot shows the full tree
        let menus = app.build_menus();
        if let Some(si) = menus[m].items.iter().position(|it| it.submenu.is_some()) {
            app.open_submenu = Some(si);
            app.hover_item = Some((m, si));
            app.hover_sub_item = Some(0);
        }
    }
    if let Some(name) = dialog_name {
        // Phase 5 dialogs that need setup beyond open_dialog.
        match name.as_str() {
            "type" => {
                app.open_type_dialog((40.0, 40.0), false, false);
                if let Some(d) = &mut app.dialog {
                    d.text = "PictoDesk".into();
                }
            }
            "effects" => {
                // The Photo layer (a pixel layer), not an adjustment.
                if let Some(i) = app.doc.layers.iter().position(|l| l.adjustment.is_none()) {
                    app.doc.active_layer = i;
                }
                app.do_action(Action::EffectsDialog(4));
                if let Some(d) = &mut app.dialog {
                    d.fx.bevel.enabled = true;
                }
                app.fx_sync_from_vals();
            }
            "extract" => {
                app.do_action(Action::ExtractDialog);
            }
            "liquify" => {
                app.do_action(Action::LiquifyDialog);
            }
            "patternmaker" => {
                app.do_action(Action::PatternMakerDialog);
            }
            "filebrowser" => {
                app.do_action(Action::FileBrowserDialog);
            }
            "spellcheck" => {
                app.do_action(Action::SpellCheckDialog);
            }
            "findreplace" => {
                app.do_action(Action::FindReplaceDialog);
            }
            "blending" => {
                app.do_action(Action::BlendingDialog);
            }
            "presets" => {
                app.do_action(Action::PresetManagerDialog);
            }
            "gradmap" => {
                app.do_action(Action::NewAdjustment("gradientmap"));
            }
            "note" => {
                app.open_dialog(DialogKind::NoteEdit);
            }
            "insertmenu" => {
                app.actions.push(RecordedAction {
                    name: "Demo".into(),
                    steps: vec![RecordedStep::Invert],
                    set: "Default Set".into(),
                });
                app.active_action = 0;
                app.open_insert_menu();
            }
            "saveaco" => {
                app.open_save_dialog(2, "palette.aco");
            }
            "layers" => {
                // Give the shot doc a layer set so the panel
                // shows folder header rows.
                app.doc.sets.push(pictodesk_core::document::LayerSet {
                    name: "Group 1".into(),
                    opacity: 1.0,
                    mode: None,
                    visible: true,
                    color: 2,
                });
                let n = app.doc.layers.len();
                if n >= 2 {
                    app.doc.layers[n - 1].group = 1;
                    app.doc.layers[n - 2].group = 1;
                }
            }
            "effects6" => {
                app.do_action(Action::EffectsDialog(9));
            }
            "historypanel" => {
                app.do_action(Action::AdjustDialog("invert"));
                app.do_action(Action::FlipHorizontal);
                app.take_snapshot();
            }
            _ => {}
        }
        let kind = match name.as_str() {
            "bc" => DialogKind::BrightnessContrast,
            "custom" => DialogKind::CustomFilter,
            "colorrange" => DialogKind::ColorRange,
            "photo" => DialogKind::PhotoFilter,
            "levels" => DialogKind::Levels,
            "curves" => DialogKind::Curves,
            "cb" => DialogKind::ColorBalance,
            "history" => {
                use pictodesk_core::document::HistoryEvent;
                for (when, action) in [
                    (1_763_827_200, "Created"),
                    (1_763_830_800, "Levels"),
                    (1_763_834_400, "Clone Stamp"),
                    (1_763_838_000, "Saved"),
                    (1_763_841_600, "Free Transform"),
                    (1_763_845_200, "Opened"),
                    (1_763_848_800, "Gaussian Blur"),
                ] {
                    app.doc.history_log.push(HistoryEvent {
                        when,
                        action: action.into(),
                    });
                }
                DialogKind::HistoryLog
            }
            "mixer" => DialogKind::ChannelMixer,
            "web" => DialogKind::SaveForWeb,
            "zigzag" => DialogKind::LegacyFilter(6),
            "extrude" => DialogKind::LegacyFilter(16),
            "smartblur" => DialogKind::LegacyFilter(21),
            "deinterlace" => DialogKind::LegacyFilter(22),
            "wave" => DialogKind::LegacyFilter(23),
            "lensflare" => DialogKind::LegacyFilter(27),
            "lighting" => DialogKind::LegacyFilter(28),
            "cutout" => DialogKind::LegacyFilter(30),
            "pastels" => DialogKind::LegacyFilter(39),
            "sumie" => DialogKind::LegacyFilter(51),
            "chrome" => DialogKind::LegacyFilter(54),
            "bitmapdlg" => DialogKind::BitmapConv,
            "indexeddlg" => DialogKind::IndexedConv,
            "duodlg" => DialogKind::Duotone,
            "applyimg" => DialogKind::ApplyImage,
            "calcdlg" => DialogKind::Calculations,
            "loadsel" => DialogKind::LoadSel,
            "mergem" => DialogKind::MergeChannels,
            "selcolor" => DialogKind::SelectiveColor,
            "fade" => DialogKind::Fade,
            "stroke" => DialogKind::StrokeSel,
            "rotatearb" => DialogKind::RotateArbitrary,
            "fileinfo" => DialogKind::FileInfo,
            "prefs" => DialogKind::Preferences,
            "defringe" => DialogKind::Defringe,
            "replcolor" => DialogKind::ReplaceColor,
            "printopts" => DialogKind::Print,
            "colorset" => DialogKind::ColorSettings,
            "stained" => DialogKind::LegacyFilter(67),
            "3d" => DialogKind::Transform3d,
            _ => DialogKind::HueSaturation,
        };
        if name.as_str() == "variations" {
            app.open_variations();
        } else if !matches!(
            name.as_str(),
            "type"
                | "effects"
                | "extract"
                | "historypanel"
                | "liquify"
                | "patternmaker"
                | "filebrowser"
                | "spellcheck"
                | "findreplace"
                | "blending"
                | "presets"
                | "gradmap"
                | "effects6"
                | "note"
                | "insertmenu"
                | "saveaco"
                | "layers"
        ) {
            app.open_dialog(kind);
        }
    }
    if prefshortcuts {
        if let Some(d) = &mut app.dialog {
            if d.kind == DialogKind::Preferences {
                d.fx_page = 1;
            }
        }
    }
    if tone {
        app.tool = Tool::Dodge;
    }
    let mut frame = Frame::new(size.0, size.1);
    if ft {
        // Free Transform shot: transform the Sun layer, box visible.
        app.doc.active_layer = 2;
        app.begin_free_transform();
        if let Some(f) = &mut app.ft {
            f.sx = 1.5;
            f.sy = 0.8;
            f.rot = 18.0;
        }
        app.ft_apply();
    }
    if many_layers {
        // Duplicate the stack up to 12 layers and select the bottom
        // one, so the Layers panel has to scroll it into view.
        while app.doc.layers.len() < 12 {
            let top = app.doc.layers.len() - 1;
            app.doc.duplicate_layer(top);
            let n = app.doc.layers.len();
            app.doc.layers[n - 1].name = format!("Copy {}", n - 4);
        }
        app.doc.active_layer = 0;
        app.doc_version += 1;
    }
    app.draw(&mut frame);
    // Restore the stashed panel layout (the --uipanels demo must
    // not leak into later runs' ~/.pictodesk).
    match layout_before {
        Some(bytes) => {
            let _ = std::fs::write(pictodesk_dir().join("layout.txt"), bytes);
        }
        None => {
            let _ = std::fs::remove_file(pictodesk_dir().join("layout.txt"));
        }
    }

    // CPU-only shot (no GPU adapter in this environment): save
    // the CPU frame directly; the GPU cross-check is skipped.
    if std::env::var_os("PICTODESK_CPU_SHOT").is_some() {
        return match write_ppm(&shot_path, size.0, size.1, &frame.px) {
            Ok(()) => {
                println!("SHOT CPU: wrote {}", shot_path.display());
                0
            }
            Err(e) => {
                eprintln!("write failed: {e}");
                1
            }
        };
    }

    let instance = wgpu::Instance::default();
    let mut gpu = match pollster::block_on(Gpu::new_for_surface(&instance, None)) {
        Some(g) => g,
        None => {
            eprintln!("no GPU adapter available");
            return 1;
        }
    };
    gpu.format = wgpu::TextureFormat::Rgba8UnormSrgb;
    // rebuild pipeline for the offscreen format
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
    let pipeline_layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pl"),
            bind_group_layouts: &[&gpu.layout],
            push_constant_ranges: &[],
        });
    gpu.pipeline = gpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline-offscreen"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
    gpu.upload(&frame);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shot-target"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    gpu.draw_to(&view);
    // readback
    let pitch = size.0 * 4;
    let padded = pitch.div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shot-readback"),
        size: (padded * size.1) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buf,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size.1),
            },
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::Maintain::Wait);
    if rx.recv().ok().and_then(|r| r.ok()).is_none() {
        eprintln!("readback failed");
        return 1;
    }
    let data = slice.get_mapped_range();
    let mut rgba = vec![0u8; (size.0 * size.1 * 4) as usize];
    for y in 0..size.1 as usize {
        rgba[y * pitch as usize..(y + 1) * pitch as usize]
            .copy_from_slice(&data[y * padded as usize..y * padded as usize + pitch as usize]);
    }
    drop(data);
    // The framebuffer IS the UI (CPU-drawn); verify GPU output matches
    // the CPU frame at a few sample points, then save the GPU output.
    let check = |x: u32, y: u32| -> bool {
        let i = ((y * size.0 + x) * 4) as usize;
        (0..3).all(|c| (rgba[i + c] as i32 - frame.px[i + c] as i32).abs() <= 2)
    };
    let ok = check(10, 10) && check(size.0 / 2, size.1 / 2) && check(size.0 - 20, 30);
    match write_ppm(&shot_path, size.0, size.1, &rgba) {
        Ok(()) => {
            println!(
                "SHOT {}: wrote {}",
                if ok { "PASS" } else { "MISMATCH" },
                shot_path.display()
            );
            if ok {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("write failed: {e}");
            1
        }
    }
}
