//! PictoDesk desktop shell — Phase 1: a classic 1990-era workbench.
//!
//! The chrome is deliberate 1990: platinum menu bar (File, Edit,
//! Image, Mode, Filter, Select, Window), a two-column toolbox with
//! the 1.0 tool set, foreground/background swatches, floating
//! palettes (Brushes, Histogram), modal adjustment dialogs with
//! live preview, and exactly ONE level of Undo in the Edit menu —
//! because that's how the 1990 original worked. (The engine underneath
//! already supports unlimited history; Phase 5 will expose it.)
//!
//! All UI is drawn on the CPU into a framebuffer and uploaded as a
//! single GPU texture; the document is composited into the same
//! frame from pictodesk-core's renderer.

mod tabs;

use crate::actions::{
    apply_step, line_to_step, load_actions_state, load_recent_files, load_shortcuts,
    load_tool_presets, load_workspaces, AudioRec, BrushPreset, FadeSource, RecordedAction,
    RecordedStep, ShortcutDef, ToolPreset, Workspace,
};
use crate::dialogs::Dialog;
use crate::file_panel::last_open_dir;
use crate::headless::run_shot;
use crate::input::ContextMenu;
use crate::panels::{HistRow, HistSource, PanelDrop, PanelKind, PanelSlot, SlotLoc};
use crate::prefs::Prefs;
use crate::runner::run_window;
use crate::tabs::DocTab;
use crate::tools::{
    BrushDynamics, ChanRef, ChannelView, DragMode, FtState, PaintTarget, StrokeState, Tool,
};
mod actions;
mod dialogs;
mod file_panel;
mod headless;
mod input;
mod menus;
mod panels;
mod prefs;
mod render;
mod runner;
mod tools;
mod typo;
mod ui;
mod viewport;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use font8x8::UnicodeFonts as _;
use pictodesk_core::adjust;
use pictodesk_core::blend::BlendMode;
use pictodesk_core::brush::{self, BrushTip};
use pictodesk_core::document::{AlphaChannel, ColorMode};
use pictodesk_core::filter;
use pictodesk_core::format;
use pictodesk_core::histogram::Histogram;
use pictodesk_core::history::{
    run_tile_edit, Command, DocumentCommand, MoveLayerCommand, SnapKind, Snapshot, TileEditCommand,
};
use pictodesk_core::io::{self, ExportFormat};
use pictodesk_core::path::{Anchor, Path as BezPath, Subpath};
use pictodesk_core::selection::Selection;
use pictodesk_core::transform;
use pictodesk_core::{Document, History, Pixel16};

use ui::{Frame, Rect};
use viewport::Viewport;

// ---------------------------------------------------------------- tools

/// Toolbox order, two columns, classic grouping.
const TOOLS: [Tool; 50] = [
    Tool::Move,
    Tool::MarqueeRect,
    Tool::MarqueeEllipse,
    Tool::MarqueeRow,
    Tool::MarqueeCol,
    Tool::Lasso,
    Tool::PolyLasso,
    Tool::MagneticLasso,
    Tool::Wand,
    Tool::Pen,
    Tool::FreeformPen,
    Tool::MagneticPen,
    Tool::ConvertAnchor,
    Tool::Eyedropper,
    Tool::ColorSampler,
    Tool::Measure,
    Tool::Brush,
    Tool::Pencil,
    Tool::HistoryBrush,
    Tool::ArtHistoryBrush,
    Tool::Eraser,
    Tool::MagicEraser,
    Tool::BackgroundEraser,
    Tool::Stamp,
    Tool::PatternStamp,
    Tool::Heal,
    Tool::Patch,
    Tool::Bucket,
    Tool::Gradient,
    Tool::Blur,
    Tool::Sharpen,
    Tool::Smudge,
    Tool::Dodge,
    Tool::Burn,
    Tool::Sponge,
    Tool::Type,
    Tool::VertType,
    Tool::TypeMask,
    Tool::VertTypeMask,
    Tool::ShapeRect,
    Tool::ShapeRounded,
    Tool::ShapeEllipse,
    Tool::ShapePolygon,
    Tool::ShapeLine,
    Tool::ShapeCustom,
    Tool::Slice,
    Tool::SliceSelect,
    Tool::Notes,
    Tool::Hand,
    Tool::Zoom,
];

pub const DOCK_W: i32 = 216;
/// Document tab bar height (always shown in standard mode).
pub const TAB_H: i32 = 24;

#[derive(Clone, Copy, PartialEq, Debug)]
enum FtDrag {
    None,
    Move,
    Scale(usize),
    Rotate,
    Skew,
    /// Free corner drag (Distort); the bool is Perspective
    /// (the neighbouring corner mirrors).
    Quad(usize, bool),
}

/// Height of a panel tab strip / float header bar.
const PANEL_TAB_H: i32 = 16;

// ---------------------------------------------------------------- actions

// ---------------------------------------------------------------- dialogs

// ---------------------------------------------------------------- app state

/// One LCG step -> 0..1. Stroke randomness is seeded per
/// stroke so a stroke is reproducible in tests.
fn next_rand(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    (*state >> 9) as f32 / (1u32 << 23) as f32
}

/// rgb -> hsl with hue in turns (this codebase's convention).
fn rgb_to_hsl_turns(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) / 2.0;
    if (mx - mn).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 {
        d / (2.0 - mx - mn)
    } else {
        d / (mx + mn)
    };
    let h = if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_turns_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let h = h.rem_euclid(1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

/// Photo Filter preset colors (Phase 4): name + 8-bit RGB.
/// Duotone curve presets (Phase 6.5) as 256-entry LUTs over
/// ink density (0 = paper, 255 = full shadow): 0 Linear,
/// 1 Shadows (ink waits for deep tones), 2 Highlights (ink
/// arrives early), 3 Steep (S-curve).
fn duo_curve_lut(preset: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        let x = i as f32 / 255.0;
        let y = match preset {
            1 => x.powf(1.7),
            2 => 1.0 - (1.0 - x) * (1.0 - x),
            3 => x * x * (3.0 - 2.0 * x),
            _ => x,
        };
        *v = (y * 255.0).round() as u8;
    }
    lut
}

/// Classify a stored duotone curve LUT back to its nearest
/// preset (for reopening the Duotone dialog on a document
/// that already has a spec).
fn duo_curve_preset(lut: &[u8; 256]) -> u8 {
    let mut best = 0u8;
    let mut bd = i64::MAX;
    for p in 0..4u8 {
        let cand = duo_curve_lut(p);
        let d: i64 = (0..256)
            .map(|i| (lut[i] as i64 - cand[i] as i64).abs())
            .sum();
        if d < bd {
            bd = d;
            best = p;
        }
    }
    best
}

/// Duotone dialog ink-swatch rect (shared by draw + press).
fn duo_swatch_rect(rect: Rect, i: usize) -> Rect {
    Rect::new(rect.x + 12 + i as i32 * 44, rect.y + 52, 40, 18)
}

/// Color Table dialog cell rect (shared by draw + press).
fn table_cell_rect(rect: Rect, i: usize) -> Rect {
    Rect::new(
        rect.x + 12 + (i % 16) as i32 * 23,
        rect.y + 30 + (i / 16) as i32 * 23,
        22,
        22,
    )
}

/// The channel cycle list for the channel dialogs: Gray,
/// the mode's color channels, Transparency, then every
/// saved alpha channel by name.
fn chan_cycle(doc: &pictodesk_core::document::Document) -> Vec<(String, ChanRef)> {
    use pictodesk_core::document::ColorMode;
    let mut v: Vec<(String, ChanRef)> = vec![("Gray".into(), ChanRef::Gray)];
    let names: &[&str] = match doc.mode {
        ColorMode::Rgb | ColorMode::Indexed => &["Red", "Green", "Blue"],
        ColorMode::Cmyk => &["Cyan", "Magenta", "Yellow", "Black"],
        ColorMode::Lab => &["Lightness", "a", "b"],
        _ => &[],
    };
    for (i, n) in names.iter().enumerate() {
        v.push((n.to_string(), ChanRef::Color(i as u8)));
    }
    v.push(("Transparency".into(), ChanRef::Transparency));
    for (i, ch) in doc.alphas.iter().enumerate() {
        v.push((ch.name.clone(), ChanRef::Alpha(i)));
    }
    v
}

/// Resolve a channel dialog source (layer cycle + channel
/// cycle) to a core plane source. Layer cycle 0 = composite.
fn resolve_plane(
    doc: &pictodesk_core::document::Document,
    layer_sel: usize,
    chan_idx: usize,
) -> pictodesk_core::channels::PlaneSource {
    use pictodesk_core::channels::PlaneSource;
    let list = chan_cycle(doc);
    let cr = list.get(chan_idx).map(|e| e.1).unwrap_or(ChanRef::Gray);
    match cr {
        ChanRef::Gray => {
            if layer_sel == 0 {
                PlaneSource::CompositeGray
            } else {
                PlaneSource::LayerGray(layer_sel - 1)
            }
        }
        ChanRef::Color(c) => {
            if layer_sel == 0 {
                PlaneSource::CompositeChannel(c)
            } else {
                PlaneSource::LayerChannel(layer_sel - 1, c)
            }
        }
        ChanRef::Transparency => {
            if layer_sel == 0 {
                PlaneSource::CompositeAlpha
            } else {
                // The layer's own alpha slot index per mode.
                let ai = match doc.mode {
                    pictodesk_core::document::ColorMode::Cmyk => 4,
                    _ => 3,
                };
                PlaneSource::LayerChannel(layer_sel - 1, ai)
            }
        }
        ChanRef::Alpha(i) => PlaneSource::Alpha(i),
    }
}

/// The blend cycle shared by Apply Image (first 12) and
/// Calculations (all 14, ending in the arithmetic modes).
const BLEND_CYCLE: &[&str] = &[
    "Normal",
    "Multiply",
    "Screen",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Darken",
    "Lighten",
    "Difference",
    "Exclusion",
    "Color Dodge",
    "Color Burn",
    "Add",
    "Subtract",
];

fn blend_cycle_mode(i: usize) -> Option<pictodesk_core::blend::BlendMode> {
    use pictodesk_core::blend::BlendMode as BM;
    Some(match i {
        0 => BM::Normal,
        1 => BM::Multiply,
        2 => BM::Screen,
        3 => BM::Overlay,
        4 => BM::SoftLight,
        5 => BM::HardLight,
        6 => BM::Darken,
        7 => BM::Lighten,
        8 => BM::Difference,
        9 => BM::Exclusion,
        10 => BM::ColorDodge,
        11 => BM::ColorBurn,
        _ => return None,
    })
}

fn calc_op(i: usize) -> pictodesk_core::channels::CalcOp {
    match blend_cycle_mode(i) {
        Some(m) => pictodesk_core::channels::CalcOp::Mode(m),
        None => {
            if i == 12 {
                pictodesk_core::channels::CalcOp::Add
            } else {
                pictodesk_core::channels::CalcOp::Subtract
            }
        }
    }
}

/// The mode's color channel names (Apply Image targets).
fn color_chan_names(doc: &pictodesk_core::document::Document) -> Vec<String> {
    use pictodesk_core::document::ColorMode;
    match doc.mode {
        ColorMode::Rgb | ColorMode::Indexed => {
            vec!["Red".into(), "Green".into(), "Blue".into()]
        }
        ColorMode::Cmyk => vec![
            "Cyan".into(),
            "Magenta".into(),
            "Yellow".into(),
            "Black".into(),
        ],
        ColorMode::Lab => vec!["Lightness".into(), "a".into(), "b".into()],
        _ => vec![],
    }
}

/// Advance a cycle index.
fn cyc(v: usize, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (v + 1) % n
    }
}

/// Layer cycle label: 0 = the flattened composite.
fn layer_cycle_name(doc: &pictodesk_core::document::Document, sel: usize) -> String {
    if sel == 0 {
        "Composite".into()
    } else {
        doc.layers
            .get(sel - 1)
            .map(|l| l.name.clone())
            .unwrap_or_else(|| "Composite".into())
    }
}

const PHOTO_FILTER_PRESETS: [(&str, (u8, u8, u8)); 6] = [
    ("Warming", (255, 178, 84)),
    ("Cooling", (96, 165, 255)),
    ("Red", (255, 60, 40)),
    ("Green", (60, 200, 90)),
    ("Blue", (60, 90, 255)),
    ("Sepia", (172, 122, 58)),
];

/// A path identity: the unsaved work path or a saved path by index.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PathRef {
    Work,
    Saved(usize),
}

type LastFt = (f32, f32, f32, f32, f32, f32, Option<[(f32, f32); 4]>);

/// A no-dialog filter function (see `App::filter_now`).
type FilterFn = fn(&mut Document, usize, Option<&Selection>);

/// One wireframe line segment in the 3D Transform preview, and
/// the segment list the preview builds per shape.
type WireSeg = ((i32, i32), (i32, i32));
type WireSegs = Vec<WireSeg>;

/// Load the persisted action sets + actions, if the file
/// exists. Format: header, then `set <name>` lines and
/// `action <set>\t<name>` lines each followed by step lines.
/// Stamp text into a layer in the 8x8 UI font (Picture
/// Package labels).
fn stamp_label(
    layer: &mut pictodesk_core::layer::Layer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel16,
) {
    for (ci, ch) in text.chars().enumerate() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row, &bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        layer.set_pixel(x + (ci * 8 + col) as i32, y + row as i32, color);
                    }
                }
            }
        }
    }
}

/// Save for Web color-to-transparency: pixels equal to
/// `key` become transparent. With dithering, pixels near the
/// key color join in by color distance, error-diffused, so
/// edges soften instead of stair-stepping.
fn knockout_color(rgba: &mut [u8], w: u32, h: u32, key: (u8, u8, u8), dither: bool) {
    let (w, h) = (w as usize, h as usize);
    if !dither {
        for px in rgba.as_chunks_mut::<4>().0 {
            if (px[0], px[1], px[2]) == key {
                px[3] = 0;
            }
        }
        return;
    }
    let dist = |px: &[u8]| -> f32 {
        let dr = px[0] as f32 - key.0 as f32;
        let dg = px[1] as f32 - key.1 as f32;
        let db = px[2] as f32 - key.2 as f32;
        (dr * dr + dg * dg + db * db).sqrt()
    };
    let mut err = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            // score: 1 at the key color, 0 at distance >= 72.
            let score = (1.0 - dist(&rgba[i * 4..i * 4 + 3]) / 72.0).clamp(0.0, 1.0) + err[i];
            let transparent = score >= 0.5;
            if transparent {
                rgba[i * 4 + 3] = 0;
            }
            let e = score - if transparent { 1.0 } else { 0.0 };
            if x + 1 < w {
                err[i + 1] += e * 7.0 / 16.0;
            }
            if y + 1 < h {
                if x > 0 {
                    err[i + w - 1] += e * 3.0 / 16.0;
                }
                err[i + w] += e * 5.0 / 16.0;
                if x + 1 < w {
                    err[i + w + 1] += e / 16.0;
                }
            }
        }
    }
}

fn pictodesk_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join(".pictodesk")
}

fn tool_id(t: Tool) -> usize {
    TOOLS.iter().position(|x| *x == t).unwrap_or(0)
}

/// Fade blend modes: display name + core blend mode
/// (None = Normal, a plain mix).
const FADE_MODES: [(&str, Option<BlendMode>); 12] = [
    ("Normal", None),
    ("Multiply", Some(BlendMode::Multiply)),
    ("Screen", Some(BlendMode::Screen)),
    ("Overlay", Some(BlendMode::Overlay)),
    ("Soft Light", Some(BlendMode::SoftLight)),
    ("Hard Light", Some(BlendMode::HardLight)),
    ("Darken", Some(BlendMode::Darken)),
    ("Lighten", Some(BlendMode::Lighten)),
    ("Difference", Some(BlendMode::Difference)),
    ("Exclusion", Some(BlendMode::Exclusion)),
    ("Color Dodge", Some(BlendMode::ColorDodge)),
    ("Color Burn", Some(BlendMode::ColorBurn)),
];

/// Blend one tile toward another (Fade's core math).
fn blend_tile(
    before: &pictodesk_core::tile::Tile,
    after: &pictodesk_core::tile::Tile,
    mode: Option<BlendMode>,
    opacity: f32,
) -> pictodesk_core::tile::Tile {
    let mut out = before.clone();
    for (bp, ap) in out.pixels.iter_mut().zip(after.pixels.iter()) {
        let b = bp.to_f32s();
        let a = ap.to_f32s();
        let blended = match mode {
            None => (a.0, a.1, a.2),
            Some(m) => pictodesk_core::blend::blend_rgb(m, (b.0, b.1, b.2), (a.0, a.1, a.2))
                .unwrap_or_else(|| {
                    (
                        m.blend_channel(b.0, a.0),
                        m.blend_channel(b.1, a.1),
                        m.blend_channel(b.2, a.2),
                    )
                }),
        };
        let mix = |x: f32, y: f32| x + (y - x) * opacity;
        *bp = Pixel16::from_f32s(
            mix(b.0, blended.0),
            mix(b.1, blended.1),
            mix(b.2, blended.2),
            mix(b.3, a.3),
        );
    }
    out.dirty = true;
    out
}

struct App {
    doc: Document,
    hist: History,
    sel: Option<Selection>,
    tool: Tool,
    fg: Pixel16,
    bg: Pixel16,
    tip: BrushTip,
    opacity: f32,
    /// Dodge/Burn range and per-stamp exposure; Sponge mode.
    tone_range: brush::ToneRange,
    tone_exposure: f32,
    sponge_saturate: bool,
    clone_src: Option<(f32, f32)>,
    /// Phase 7: Healing Brush source + Sampled/Pattern toggle.
    heal_src: Option<(f32, f32)>,
    heal_use_pattern: bool,
    /// Phase 7 paint engine: dynamics, Flow, palette section,
    /// and the stroke seed counter.
    brush_dyn: BrushDynamics,
    flow: f32,
    brush_section: u8,
    stroke_seed: u32,
    viewport: Viewport,
    file_path: Option<PathBuf>,
    dirty: bool,
    status: String,
    /// Label of the most recent history command — drives the Edit
    /// menu's "Undo <action>" text, as in the classics.
    last_label: Option<String>,
    // ui state
    open_menu: Option<usize>,
    open_submenu: Option<usize>,
    hover_item: Option<(usize, usize)>,
    hover_sub_item: Option<usize>,
    hover_file: Option<usize>,
    dialog: Option<Dialog>,
    /// In-flight microphone capture for an audio note.
    audio_rec: Option<AudioRec>,
    /// Currently playing audio-note stream (kept alive until
    /// it runs out of samples).
    audio_out: Option<cpal::Stream>,
    show_histogram: bool,
    show_brushes: bool,
    show_rulers: bool,
    show_palette: bool,
    show_cp: bool,
    cp_tab: usize, // 0 = Layers, 1 = Channels, 2 = Paths
    channel_view: ChannelView,
    paint_target: PaintTarget,
    active_path: Option<PathRef>,
    pen_hit: Option<(usize, usize, u8)>, // subpath, anchor, part (0 point, 1 in, 2 out)
    pen_anchor: Option<(usize, usize)>,
    alpha_cmd: Option<DocumentCommand>,
    /// Doc snapshot taken when a pen/path edit begins; committed as
    /// one undo step when the gesture ends (path edits are undoable).
    path_before: Option<Document>,
    alpha_last: (f32, f32),
    // Phase 4: navigator, guides/grid/snap, actions, context menu.
    show_navigator: bool,
    show_actions: bool,
    // Phase 5: history panel, snapshots, info panel, type editing.
    show_history: bool,
    show_info: bool,
    snapshots: Vec<Snapshot>,
    hist_source: HistSource,
    hist_scroll: usize,
    hist_selected: Option<HistRow>,
    hist_auto_snapshot: bool,
    /// Cached render-source document for the History Brush /
    /// Fill with History: (source, hist position, state count, doc).
    hist_src_cache: Option<(HistSource, usize, usize, Document)>,
    /// The persistent measurement (Measure tool): start, end in
    /// document coords.
    measure: Option<((f32, f32), (f32, f32))>,
    /// Magic/Background Eraser tolerance (0..=255 color distance).
    eraser_tolerance: u8,
    /// Gradient tool geometry (Brushes panel option row).
    gradient_shape: brush::GradientShape,
    /// Layer > Effects clipboard (Copy/Paste Effects).
    fx_clipboard: Option<pictodesk_core::effects::Effects>,
    /// Print setup (points): paper size + scale + orientation,
    /// remembered across Print dialogs.
    page_w: f32,
    page_h: f32,
    page_scale: f32,
    page_landscape: bool,
    /// Color proofing: soft-proof through CMYK on screen.
    proof_cmyk: bool,
    /// Phase 6.5 Batch 6c view/tool state.
    gamut_warning: bool,
    show_path: bool,
    show_edges: bool,
    guides_locked: bool,
    layer_thumb: u8,
    line_arrow_start: bool,
    line_arrow_end: bool,
    /// Last Free Transform parameters for Transform Again
    /// (sx, sy, rot, skew, dx, dy) + optional distort quad.
    last_ft: Option<LastFt>,
    /// Extract dialog: the last computed preview render.
    extract_preview_cache: Option<Vec<u8>>,
    /// Eraser option: erase back to the history source state.
    eraser_to_history: bool,
    /// Clone Stamp "Sample Merged" option (Phase 6.5).
    stamp_sample_merged: bool,
    /// Eyedropper "Sample Merged" option (Phase 6.5); the
    /// eyedropper historically samples the composite, so this
    /// starts on.
    dropper_sample_merged: bool,
    /// Color Settings: working space given to new documents.
    default_space: pictodesk_core::color::WorkingSpace,
    /// Edit > Fade source (Phase 6.5 Batch 6).
    fade_source: Option<FadeSource>,
    /// The most recent Filter-menu step (Last Filter).
    last_filter_step: Option<RecordedStep>,
    /// Options Bar visible (Phase 6.5 Batch 5).
    show_options_bar: bool,
    /// Tooltips on hover (Preferences > General).
    show_tooltips: bool,
    /// The Photoshop 7.0 settings (prefs.rs).
    prefs: Prefs,
    /// File > Open Recent (Preferences > File Handling sets
    /// how many are kept).
    recent_files: Vec<std::path::PathBuf>,
    /// Plain scroll wheel zooms instead of panning (Preferences).
    zoom_with_scroll: bool,
    /// Keyboard shortcuts: (id, label, combo). Combos are
    /// canonical ("primary+z", "b"); "primary" is Cmd on macOS,
    /// Ctrl elsewhere.
    shortcuts: Vec<ShortcutDef>,
    /// Preferences Shortcuts page: the row being (re)assigned,
    /// and a (row, combo) waiting for a second press to steal a
    /// shortcut another command owns.
    prefs_capture: Option<usize>,
    prefs_confirm: Option<(usize, String)>,
    /// Hover tooltip: the bubble text + anchor, and the hover
    /// that started it (the bubble appears after a delay).
    tooltip: Option<(String, i32, i32)>,
    hover_text: Option<String>,
    hover_since: Option<std::time::Instant>,
    /// Display mode (Batch 5): 0 standard, 1 full screen
    /// with menu bar, 2 full screen. Cycled with F.
    display_mode: u8,
    /// Print Options state (Phase 6.5 Batch 4).
    print_centered: bool,
    print_fit: bool,
    print_pos: (f32, f32),
    print_marks: u8,
    print_negative: bool,
    print_emulsion: bool,
    print_bilinear: bool,
    print_profile: u8,
    /// Quick Mask mode is active (Phase 6.5): the mask lives
    /// as a temporary alpha channel named "Quick Mask".
    quick_mask: bool,
    /// The Split Channels set (Phase 6.5), kept for Merge
    /// Channels: (channel name, grayscale document).
    split_docs: Vec<(String, pictodesk_core::document::Document)>,
    pkg_sources: Vec<std::path::PathBuf>,
    liq_return: Option<Dialog>,
    mesh_bytes: Vec<u8>,
    // Phase 6 app state.
    /// Shape tools: stroke width in px (0 = no stroke), polygon
    /// sides, polygon-as-star.
    shape_stroke: u32,
    shape_sides: u8,
    shape_star: bool,
    /// Index into the custom shape choices (built-ins first,
    /// then the document's defined shapes).
    custom_idx: usize,
    /// The shape layer being dragged out right now.
    shape_drag_layer: Option<usize>,
    /// Selected slice (Slice Select tool / Slices list).
    slice_selected: Option<usize>,
    /// Slice drag origin (the slice's position at drag start).
    slice_orig: (i32, i32),
    /// Style presets (Preset Manager / Layer Style dialog):
    /// session list seeded with built-ins; user presets append.
    style_presets: Vec<(String, pictodesk_core::effects::Effects)>,
    /// Preset Manager's current kind tab: 0 Styles, 1 Shapes.
    preset_tab: usize,
    /// Liquify freeze mask being painted in the dialog (doc
    /// coords, w*h bytes) — session copy of the dialog state.
    /// Perspective Crop (6.0): the editable quad while the
    /// mode is armed, and the corner being dragged.
    persp_crop: Option<[(f32, f32); 4]>,
    persp_drag: Option<usize>,
    /// Art History Brush style: 0 Tight, 1 Loose, 2 Dab.
    art_hist_style: u8,
    show_guides: bool,
    show_grid: bool,
    snap_enabled: bool,
    actions: Vec<RecordedAction>,
    /// Phase 6.5 Batch 6d: Actions panel button mode + a paused
    /// playback waiting on a Stop dialog.
    actions_button_mode: bool,
    play_pending: Option<(Vec<RecordedStep>, String)>,
    /// Phase 6.5 Batch 6d: swatch palette + brush preset state
    /// (save/load as .ACO / .ABR) and a stashed settings step.
    palette: Vec<(u8, u8, u8)>,
    brush_presets: Vec<BrushPreset>,
    tool_presets: Vec<ToolPreset>,
    /// Docked tab groups, top-down; floats: (slot, x, y, w).
    dock_slots: Vec<PanelSlot>,
    float_slots: Vec<(PanelSlot, i32, i32, i32)>,
    /// Panel drag in progress: float index + grab offset.
    panel_drag: Option<(usize, i32, i32)>,
    /// Pressed on a tab/title, not yet past the drag threshold:
    /// (kind, slot, down x, down y).
    panel_pending: Option<(PanelKind, SlotLoc, i32, i32)>,
    /// Drop target highlighted while dragging a panel.
    panel_drop: Option<PanelDrop>,
    show_tool_presets: bool,
    tool_preset_popup: bool,
    workspaces: Vec<(String, Workspace)>,
    settings_step: Option<RecordedStep>,
    active_action: usize,
    recording: bool,
    /// Action sets (Phase 5): named groups in the Actions panel.
    /// New actions land in `action_set`.
    action_sets: Vec<String>,
    action_set: String,
    context_menu: Option<ContextMenu>,
    /// Guide being dragged out of a ruler, or an existing guide
    /// being moved: (vertical?, doc position, existing index).
    guide_drag: Option<(bool, i32, Option<usize>)>,
    /// Open documents other than the active one, in tab-bar
    /// order (the active tab's state lives in App's live fields).
    tabs: Vec<DocTab>,
    /// Position of the active tab in the bar (0..=tabs.len()).
    active_tab: usize,
    /// Name shown for the active tab (its DocTab holds the
    /// inactive tabs' names).
    doc_name: String,
    /// Close-confirm dialog waiting on this tab-bar index.
    pending_close: Option<usize>,
    file_panel: Option<Vec<PathBuf>>,
    /// Subdirectories of the file panel's current folder.
    file_panel_dirs: Vec<PathBuf>,
    /// Folder the file panel is showing (navigable; persisted
    /// as the last-used open folder).
    file_panel_dir: PathBuf,
    /// What a file-panel click means: 0 open document,
    /// 1 displacement map, 2 texture fill.
    file_panel_purpose: u8,
    /// Displace parameters waiting for their map file:
    /// (h scale, v scale, tile, wrap).
    pending_displace: Option<(f32, f32, bool, bool)>,
    // interaction
    cursor: (f32, f32),
    mouse_down: bool,
    drag: DragMode,
    drag_start_doc: (f32, f32),
    drag_cur_doc: (f32, f32),
    lasso_points: Vec<(f32, f32)>,
    /// Polygonal / Magnetic Lasso anchors in progress, and the
    /// Magnetic Lasso's cached composite luma plane (keyed by
    /// doc_version) for edge snapping.
    poly_points: Vec<(f32, f32)>,
    mag_cache: Option<(u64, Vec<u8>)>,
    /// Freeform Pen drag points, fitted into a path on mouse-up.
    pen_free: Vec<(f32, f32)>,
    stroke: Option<StrokeState>,
    move_orig_offset: (i32, i32),
    /// Original offsets of linked layers during a Move drag.
    move_linked: Vec<(usize, (i32, i32))>,
    /// Layers panel scroll: display rows hidden above the window.
    layers_scroll: usize,
    ft: Option<FtState>,
    /// Autosave bookkeeping: when the last recovery write happened
    /// and which doc_version it captured.
    autosave_at: std::time::Instant,
    autosaved_version: u64,
    recovery_file: Option<std::path::PathBuf>,
    space_down: bool,
    alt_down: bool,
    ctrl_down: bool,
    super_down: bool,
    shift_down: bool,
    win_size: (u32, u32),
    // render cache
    doc_version: u64,
    cached_render: Vec<u8>,
    cached_size: (u32, u32),
    rendered_version: u64,
    /// Downsampled render levels (level k = 1/2^k size), built
    /// in ensure_render_cache per the Memory & Image Cache
    /// prefs; draw_canvas picks one when zoomed out.
    cache_pyramid: Vec<(u32, u32, Vec<u8>)>,
    pyramid_version: u64,
    ants_phase: i32,
    quit: bool,
}

impl App {
    fn new(doc: Document, file_path: Option<PathBuf>) -> Self {
        let (saved_sets, saved_actions) = load_actions_state();
        let saved_tool_presets = load_tool_presets();
        let saved_workspaces = load_workspaces();
        let mut hist = History::new();
        hist.set_limit(Some(20)); // Phase 5: the History panel's default depth
        let doc_name = file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let mut app = App {
            doc,
            hist,
            sel: None,
            tool: Tool::Brush,
            fg: Pixel16::from_rgba8(0, 0, 0, 255),
            bg: Pixel16::from_rgba8(255, 255, 255, 255),
            tip: BrushTip::new(13.0, 0.7),
            opacity: 1.0,
            tone_range: brush::ToneRange::Midtones,
            tone_exposure: 0.5,
            sponge_saturate: true,
            clone_src: None,
            heal_src: None,
            heal_use_pattern: false,
            brush_dyn: BrushDynamics::default(),
            flow: 1.0,
            brush_section: 0,
            stroke_seed: 0,
            viewport: Viewport {
                scale: 1.0,
                offset: (0.0, 0.0),
            },
            file_path,
            dirty: false,
            status: "Ready".into(),
            last_label: None,
            open_menu: None,
            open_submenu: None,
            hover_item: None,
            hover_sub_item: None,
            hover_file: None,
            dialog: None,
            show_histogram: true,
            show_brushes: true,
            show_rulers: true,
            show_palette: true,
            show_cp: true,
            cp_tab: 0,
            channel_view: ChannelView::Composite,
            paint_target: PaintTarget::Image,
            active_path: None,
            pen_hit: None,
            pen_anchor: None,
            alpha_cmd: None,
            path_before: None,
            alpha_last: (0.0, 0.0),
            show_navigator: true,
            show_actions: false,
            show_history: true,
            show_info: false,
            snapshots: Vec::new(),
            hist_source: HistSource::State(0),
            hist_scroll: 0,
            hist_selected: None,
            hist_auto_snapshot: true,
            hist_src_cache: None,
            measure: None,
            eraser_tolerance: 32,
            gradient_shape: brush::GradientShape::Linear,
            fx_clipboard: None,
            page_w: 612.0,
            page_h: 792.0,
            page_scale: 100.0,
            page_landscape: false,
            proof_cmyk: false,
            gamut_warning: false,
            show_path: true,
            show_edges: true,
            guides_locked: false,
            layer_thumb: 0,
            line_arrow_start: false,
            line_arrow_end: false,
            last_ft: None,
            extract_preview_cache: None,
            eraser_to_history: false,
            stamp_sample_merged: false,
            dropper_sample_merged: true,
            default_space: pictodesk_core::color::WorkingSpace::Srgb,
            fade_source: None,
            last_filter_step: None,
            show_options_bar: true,
            show_tooltips: true,
            prefs: Prefs::default(),
            recent_files: load_recent_files(),
            zoom_with_scroll: false,
            shortcuts: load_shortcuts(),
            prefs_capture: None,
            prefs_confirm: None,
            tooltip: None,
            hover_text: None,
            hover_since: None,
            display_mode: 0,
            print_centered: true,
            print_fit: false,
            print_pos: (36.0, 36.0),
            print_marks: 0,
            print_negative: false,
            print_emulsion: false,
            print_bilinear: false,
            print_profile: 0,
            quick_mask: false,
            split_docs: Vec::new(),
            pkg_sources: Vec::new(),
            liq_return: None,
            mesh_bytes: Vec::new(),
            shape_stroke: 0,
            shape_sides: 5,
            shape_star: false,
            custom_idx: 0,
            shape_drag_layer: None,
            slice_selected: None,
            slice_orig: (0, 0),
            style_presets: Vec::new(),
            preset_tab: 0,
            persp_crop: None,
            persp_drag: None,
            art_hist_style: 0,
            show_guides: true,
            show_grid: false,
            snap_enabled: true,
            actions: saved_actions,
            actions_button_mode: false,
            play_pending: None,
            palette: ui::PALETTE.to_vec(),
            tool_presets: saved_tool_presets,
            show_tool_presets: false,
            tool_preset_popup: false,
            workspaces: saved_workspaces,
            dock_slots: Vec::new(),
            float_slots: Vec::new(),
            panel_drag: None,
            panel_pending: None,
            panel_drop: None,
            brush_presets: [1.0f32, 3.0, 5.0, 9.0, 13.0, 21.0, 35.0, 50.0]
                .iter()
                .map(|sz| BrushPreset {
                    size: *sz,
                    hardness: 1.0,
                    dyn_: BrushDynamics::default(),
                })
                .chain(
                    [1.0f32, 3.0, 5.0, 9.0, 13.0, 21.0, 35.0, 50.0]
                        .iter()
                        .map(|sz| BrushPreset {
                            size: *sz,
                            hardness: 0.0,
                            dyn_: BrushDynamics::default(),
                        }),
                )
                .collect(),
            settings_step: None,
            active_action: 0,
            recording: false,
            action_sets: saved_sets,
            action_set: "Default Set".to_string(),
            context_menu: None,
            guide_drag: None,
            tabs: Vec::new(),
            active_tab: 0,
            doc_name,
            pending_close: None,
            file_panel: None,
            file_panel_dirs: Vec::new(),
            file_panel_dir: last_open_dir(),
            file_panel_purpose: 0,
            pending_displace: None,
            cursor: (0.0, 0.0),
            mouse_down: false,
            drag: DragMode::None,
            drag_start_doc: (0.0, 0.0),
            drag_cur_doc: (0.0, 0.0),
            lasso_points: Vec::new(),
            poly_points: Vec::new(),
            mag_cache: None,
            pen_free: Vec::new(),
            stroke: None,
            move_orig_offset: (0, 0),
            move_linked: Vec::new(),
            layers_scroll: 0,
            audio_rec: None,
            audio_out: None,
            ft: None,
            autosave_at: std::time::Instant::now(),
            autosaved_version: 0,
            recovery_file: None,
            space_down: false,
            alt_down: false,
            ctrl_down: false,
            super_down: false,
            shift_down: false,
            win_size: (1280, 800),
            doc_version: 0,
            cached_render: Vec::new(),
            cached_size: (0, 0),
            rendered_version: u64::MAX,
            cache_pyramid: Vec::new(),
            pyramid_version: u64::MAX,
            ants_phase: 0,
            quit: false,
        };
        app.reset_hist_for_new_doc();
        app.load_prefs_file();
        app.sync_panel_slots();
        app.load_panel_layout();
        app.sync_panel_slots();
        app
    }

    fn touch(&mut self) {
        self.doc_version += 1;
        self.dirty = true;
    }

    /// Screen (window) coords -> document coords.
    fn to_doc(&self, sx: f32, sy: f32) -> (f32, f32) {
        let c = self.canvas_rect(self.win_size.0, self.win_size.1);
        self.viewport
            .screen_to_doc((sx - c.x as f32, sy - c.y as f32))
    }

    fn active_tiles_all(&self) -> Vec<(i32, i32)> {
        Selection::tiles_in_rect(0, 0, self.doc.width, self.doc.height)
    }

    fn sel_tiles(&self) -> Vec<(i32, i32)> {
        match self.sel.as_ref().and_then(|s| s.bounds()) {
            Some((x, y, w, h)) => Selection::tiles_in_rect(x, y, w, h),
            None => self.active_tiles_all(),
        }
    }
}

// ---------------------------------------------------------------- menus

thread_local! {
    /// The 1.0-style internal clipboard: a copied pixel block + size.
    static CLIPBOARD: std::cell::RefCell<Option<(Vec<Pixel16>, u32, u32)>> =
        const { std::cell::RefCell::new(None) };
}

impl App {
    // ------------------------------------------------------------ dialogs
}

// ---------------------------------------------------------------- drawing

impl App {
    // ---- Phase 7 Brushes palette: dynamics sections ----

    // ------------------------------------------------------------ dock panels

    /// The right-click context menu, drawn like a menu dropdown.
    fn draw_context_menu(&mut self, f: &mut Frame) {
        let Some(cm) = &self.context_menu else { return };
        let w = cm
            .items
            .iter()
            .map(|(l, _, _)| Frame::text_width(l, 1) + 28)
            .max()
            .unwrap_or(120);
        let r = Rect::new(cm.x, cm.y, w, cm.items.len() as i32 * 17 + 6);
        f.panel(r);
        let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
        for (i, (label, _, enabled)) in cm.items.iter().enumerate() {
            let ir = Rect::new(r.x + 2, r.y + 3 + i as i32 * 17, r.w - 4, 17);
            if label == "-" {
                f.fill_rect(Rect::new(ir.x + 2, ir.y + 8, ir.w - 4, 1), ui::SHADOW);
                continue;
            }
            if ir.contains(mx, my) && *enabled {
                f.fill_rect(ir, ui::ACCENT);
                f.text(ir.x + 8, ir.y + 4, label, 1, ui::WHITE);
            } else {
                let c = if *enabled { ui::TEXT } else { ui::TEXT_DIM };
                f.text(ir.x + 8, ir.y + 4, label, 1, c);
            }
        }
    }
}

// ---------------------------------------------------------------- input

// ---------------------------------------------------------------- GPU plumbing

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var out: VsOut;
    let xy = p[i];
    out.pos = vec4<f32>(xy, 0.0, 1.0);
    out.uv = vec2<f32>(xy.x * 0.5 + 0.5, 0.5 - xy.y * 0.5);
    return out;
}
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, smp, in.uv);
}
"#;

// ---------------------------------------------------------------- entry

fn demo_document() -> Document {
    use pictodesk_core::blend::BlendMode;
    let mut doc = Document::new(640, 400);
    for y in 0..400 {
        for x in 0..640 {
            let t = x as f32 / 639.0;
            let r = 40.0 + 50.0 * t;
            let g = 70.0 + 20.0 * t;
            let b = 140.0 - 30.0 * t;
            doc.layers[0].set_pixel(x, y, Pixel16::from_rgba8(r as u8, g as u8, b as u8, 255));
        }
    }
    let top = doc.add_layer("Accent");
    doc.layers[top].blend_mode = BlendMode::Screen;
    doc.layers[top].opacity = 0.85;
    doc.layers[top].fill_rect(320, 120, 160, 160, Pixel16::from_rgba8(220, 80, 95, 255));
    doc.active_layer = 0;
    doc
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--shot") {
        std::process::exit(run_shot(&args));
    }
    // Droplet execution (Phase 6): --droplet-run <steps> <files...>
    // applies the steps to each image and writes <name>-edited.png.
    if let Some(pos) = args.iter().position(|a| a == "--droplet-run") {
        let Some(steps_path) = args.get(pos + 1) else {
            eprintln!("--droplet-run needs a steps file");
            std::process::exit(2);
        };
        let text = std::fs::read_to_string(steps_path).unwrap_or_default();
        let steps: Vec<RecordedStep> = text.lines().filter_map(line_to_step).collect();
        let mut done = 0usize;
        for file in &args[pos + 2..] {
            let bytes = match std::fs::read(file) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let Ok(mut doc) = pictodesk_core::io::import_image(&bytes) else {
                continue;
            };
            for step in &steps {
                apply_step(&mut doc, step, None);
            }
            let path = std::path::PathBuf::from(file);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let out = path.with_file_name(format!("{stem}-edited.png"));
            if let Ok(png) =
                pictodesk_core::io::export_image(&doc, pictodesk_core::io::ExportFormat::Png, 90)
            {
                if std::fs::write(&out, png).is_ok() {
                    done += 1;
                }
            }
        }
        println!("droplet: {done} file(s) processed");
        std::process::exit(0);
    }
    let path_arg = args.iter().find(|a| !a.starts_with("--")).cloned();
    run_window(path_arg.map(PathBuf::from));
}

// ---------------------------------------------------------------- window runner

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};
