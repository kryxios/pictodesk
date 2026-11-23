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

mod ui;
mod viewport;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pictodesk_core::adjust;
use pictodesk_core::brush::{self, BrushTip};
use pictodesk_core::document::{AlphaChannel, ColorMode};
use pictodesk_core::filter;
use pictodesk_core::format;
use pictodesk_core::histogram::Histogram;
use pictodesk_core::history::{
    run_tile_edit, Command, DocumentCommand, MoveLayerCommand, TileEditCommand,
};
use pictodesk_core::io::{self, ExportFormat};
use pictodesk_core::path::{Anchor, Path as BezPath, Subpath};
use pictodesk_core::selection::Selection;
use pictodesk_core::transform;
use pictodesk_core::{Document, History, Pixel16};

use ui::{Frame, Rect};
use viewport::Viewport;

// ---------------------------------------------------------------- tools

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Move,
    MarqueeRect,
    MarqueeEllipse,
    Lasso,
    Wand,
    Eyedropper,
    Hand,
    Zoom,
    Brush,
    Pencil,
    Eraser,
    Stamp,
    Pen,
    Bucket,
    Gradient,
    Blur,
    Sharpen,
    Smudge,
}

impl Tool {
    fn name(&self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::MarqueeRect => "Rectangular Marquee",
            Tool::MarqueeEllipse => "Elliptical Marquee",
            Tool::Lasso => "Lasso",
            Tool::Wand => "Magic Wand",
            Tool::Eyedropper => "Eyedropper",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
            Tool::Brush => "Paintbrush",
            Tool::Pencil => "Pencil",
            Tool::Eraser => "Eraser",
            Tool::Stamp => "Clone Stamp",
            Tool::Pen => "Pen",
            Tool::Bucket => "Paint Bucket",
            Tool::Gradient => "Gradient",
            Tool::Blur => "Blur",
            Tool::Sharpen => "Sharpen",
            Tool::Smudge => "Smudge",
        }
    }
    fn key(&self) -> char {
        match self {
            Tool::Move => 'v',
            Tool::MarqueeRect | Tool::MarqueeEllipse => 'm',
            Tool::Lasso => 'l',
            Tool::Wand => 'w',
            Tool::Eyedropper => 'i',
            Tool::Hand => 'h',
            Tool::Zoom => 'z',
            Tool::Brush => 'b',
            Tool::Pencil => 'b',
            Tool::Eraser => 'e',
            Tool::Stamp => 's',
            Tool::Pen => 'p',
            Tool::Bucket => 'k',
            Tool::Gradient => 'g',
            Tool::Blur | Tool::Sharpen | Tool::Smudge => 'r',
        }
    }
}

/// Toolbox order, two columns, classic grouping.
const TOOLS: [Tool; 18] = [
    Tool::Move,
    Tool::MarqueeRect,
    Tool::MarqueeEllipse,
    Tool::Lasso,
    Tool::Wand,
    Tool::Pen,
    Tool::Eyedropper,
    Tool::Brush,
    Tool::Pencil,
    Tool::Eraser,
    Tool::Stamp,
    Tool::Bucket,
    Tool::Gradient,
    Tool::Blur,
    Tool::Sharpen,
    Tool::Smudge,
    Tool::Hand,
    Tool::Zoom,
];

pub const DOCK_W: i32 = 216;

#[derive(Clone, Copy, PartialEq, Debug)]
enum FtDrag {
    None,
    Move,
    Scale(usize),
    Rotate,
    Skew,
}

/// An in-progress Free Transform: the document is re-derived from
/// `origin` on every mouse move, so drags never accumulate error.
/// Enter commits (one undo step), Esc cancels.
struct FtState {
    origin: Document,
    targets: Vec<usize>,
    bounds: (f32, f32, f32, f32), // active layer's painted bounds
    sx: f32,
    sy: f32,
    rot: f32,
    skew: f32,
    dx: f32,
    dy: f32,
    drag: FtDrag,
    drag_start_doc: (f32, f32),
    start: (f32, f32, f32, f32, f32, f32), // params at drag start
    start_angle: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PanelKind {
    Cp,
    Brushes,
    Histogram,
    Color,
}

// ---------------------------------------------------------------- actions

#[derive(Clone, Debug, PartialEq)]
enum Action {
    New,
    OpenPanel,
    Save,
    SaveAsPanel,
    HistoryLog,
    ExportPng,
    Revert,
    Quit,
    UndoToggle,
    Cut,
    Copy,
    Paste,
    Clear,
    FillDialog,
    ImageSizeDialog,
    CanvasSizeDialog,
    CropToSelection,
    Rotate180,
    Rotate90Cw,
    Rotate90Ccw,
    FlipHorizontal,
    FlipVertical,
    AdjustDialog(&'static str),
    ModeRgb,
    ModeGrayscale,
    ModeCmyk,
    FilterGaussian,
    FilterSharpen,
    FilterUnsharp,
    FilterNoise,
    SelectAll,
    SelectNone,
    SelectInverse,
    FeatherDialog,
    SelectGrow,
    SelectSimilar,
    SaveSelection,
    LoadSelection(usize),
    ToggleHistogram,
    ToggleBrushes,
    ToggleChannels,
    ToggleLayers,
    NewLayer,
    DuplicateLayer,
    DeleteLayer,
    LayerOptions,
    BlendMode(usize),
    BringForward,
    SendBackward,
    FreeTransform,
    MergeDown,
    MergeVisible,
    FlattenImage,
    TogglePaths,
    ToggleColor,
    ToggleRulers,
    ZoomIn,
    ZoomOut,
    Zoom100,
    ZoomFit,
    None,
}

#[derive(Clone, Debug)]
struct MenuItem {
    label: String,
    action: Action,
    enabled: bool,
    checked: bool,
    submenu: Option<Vec<MenuItem>>,
    shortcut: Option<String>,
}

/// The platform command-modifier label: Cmd on macOS, Ctrl elsewhere.
fn shortcut_label(k: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("Cmd+{k}")
    } else {
        format!("Ctrl+{k}")
    }
}

impl MenuItem {
    fn new(label: impl Into<String>, action: Action) -> Self {
        MenuItem {
            label: label.into(),
            action,
            enabled: true,
            checked: false,
            submenu: None,
            shortcut: None,
        }
    }
    /// Attach a keyboard shortcut (single key name, e.g. "Z"); the
    /// menu shows it with the platform's command modifier.
    fn key(mut self, k: &str) -> Self {
        self.shortcut = Some(shortcut_label(k));
        self
    }
    fn sub(mut self, items: Vec<MenuItem>) -> Self {
        self.submenu = Some(items);
        self
    }
}

#[derive(Clone, Debug)]
struct Menu {
    title: &'static str,
    items: Vec<MenuItem>,
}

// ---------------------------------------------------------------- dialogs

#[derive(Clone, Debug, PartialEq)]
enum DialogKind {
    BrightnessContrast,
    HueSaturation,
    ColorBalance,
    Levels,
    Curves,
    Posterize,
    Threshold,
    GaussianBlur,
    Unsharp,
    AddNoise,
    ImageSize,
    CanvasSize,
    Feather,
    Fill,
    NewDoc,
    SaveAs,
    GrowSel,
    SimilarSel,
    SavePath,
    NewLayer,
    LayerOptions,
    HistoryLog,
}

impl DialogKind {
    fn title(&self) -> &'static str {
        match self {
            DialogKind::BrightnessContrast => "Brightness/Contrast",
            DialogKind::HueSaturation => "Hue/Saturation",
            DialogKind::ColorBalance => "Color Balance",
            DialogKind::Levels => "Levels",
            DialogKind::Curves => "Curves",
            DialogKind::Posterize => "Posterize",
            DialogKind::Threshold => "Threshold",
            DialogKind::GaussianBlur => "Gaussian Blur",
            DialogKind::Unsharp => "Unsharp Mask",
            DialogKind::AddNoise => "Add Noise",
            DialogKind::ImageSize => "Image Size",
            DialogKind::CanvasSize => "Canvas Size",
            DialogKind::Feather => "Feather Selection",
            DialogKind::Fill => "Fill",
            DialogKind::NewDoc => "New Image",
            DialogKind::SaveAs => "Save As",
            DialogKind::GrowSel => "Grow Selection",
            DialogKind::SimilarSel => "Select Similar",
            DialogKind::SavePath => "Save Path",
            DialogKind::NewLayer => "New Layer",
            DialogKind::LayerOptions => "Layer Options",
            DialogKind::HistoryLog => "History Log",
        }
    }
    /// Pixel dialogs preview live through a tile-snapshot command.
    fn is_pixel_dialog(&self) -> bool {
        matches!(
            self,
            DialogKind::BrightnessContrast
                | DialogKind::HueSaturation
                | DialogKind::ColorBalance
                | DialogKind::Levels
                | DialogKind::Curves
                | DialogKind::Posterize
                | DialogKind::Threshold
                | DialogKind::GaussianBlur
                | DialogKind::Unsharp
                | DialogKind::AddNoise
        )
    }
}

struct Dialog {
    kind: DialogKind,
    /// Slider values by index (meaning depends on kind).
    vals: Vec<f32>,
    /// Color Balance active range: 0 shadows, 1 midtones, 2 highlights.
    cb_range: usize,
    /// Curves control points.
    curve_points: Vec<(u8, u8)>,
    curve_drag: Option<usize>,
    /// Canvas Size anchor.
    anchor: (i32, i32),
    /// Fill: use background color instead of foreground.
    fill_bg: bool,
    /// Text field contents (Save As name).
    text: String,
    text_focus: bool,
    /// Live-preview snapshot (pixel dialogs only).
    preview: Option<TileEditCommand>,
    slider_drag: Option<usize>,
}

impl Dialog {
    fn new(kind: DialogKind, doc: &Document) -> Self {
        let vals = match kind {
            DialogKind::BrightnessContrast => vec![0.0, 0.0],
            DialogKind::HueSaturation => vec![0.0, 0.0, 0.0],
            DialogKind::ColorBalance => vec![0.0; 9],
            DialogKind::Levels => vec![0.0, 255.0, 1.0, 0.0, 255.0],
            DialogKind::Curves => vec![],
            DialogKind::Posterize => vec![4.0],
            DialogKind::Threshold => vec![128.0],
            DialogKind::GaussianBlur => vec![1.0],
            DialogKind::Unsharp => vec![0.5, 1.0, 0.0],
            DialogKind::AddNoise => vec![0.25],
            DialogKind::ImageSize | DialogKind::CanvasSize | DialogKind::NewDoc => {
                vec![doc.width as f32, doc.height as f32]
            }
            DialogKind::Feather => vec![5.0],
            DialogKind::GrowSel | DialogKind::SimilarSel => vec![32.0],
            DialogKind::SavePath => vec![],
            DialogKind::NewLayer => vec![],
            DialogKind::LayerOptions => {
                vec![doc.layers[doc.active_layer].opacity * 100.0]
            }
            DialogKind::Fill => vec![100.0],
            DialogKind::SaveAs => vec![],
            DialogKind::HistoryLog => vec![],
        };
        Dialog {
            kind,
            vals,
            cb_range: 1,
            curve_points: vec![(0, 0), (255, 255)],
            curve_drag: None,
            anchor: (0, 0),
            fill_bg: false,
            text: String::new(),
            text_focus: false,
            preview: None,
            slider_drag: None,
        }
    }

    /// (label, min, max) per slider index.
    fn slider_defs(&self) -> Vec<(&'static str, f32, f32)> {
        match self.kind {
            DialogKind::BrightnessContrast => {
                vec![("Brightness", -150.0, 150.0), ("Contrast", -100.0, 100.0)]
            }
            DialogKind::HueSaturation => vec![
                ("Hue", -180.0, 180.0),
                ("Saturation", -100.0, 100.0),
                ("Lightness", -100.0, 100.0),
            ],
            DialogKind::ColorBalance => vec![
                ("Cyan - Red", -100.0, 100.0),
                ("Magenta - Green", -100.0, 100.0),
                ("Yellow - Blue", -100.0, 100.0),
            ],
            DialogKind::Levels => vec![
                ("In Black", 0.0, 254.0),
                ("In White", 1.0, 255.0),
                ("Gamma", 0.1, 9.99),
                ("Out Black", 0.0, 255.0),
                ("Out White", 0.0, 255.0),
            ],
            DialogKind::Posterize => vec![("Levels", 2.0, 255.0)],
            DialogKind::Threshold => vec![("Threshold", 0.0, 255.0)],
            DialogKind::GaussianBlur => vec![("Radius", 0.1, 50.0)],
            DialogKind::Unsharp => vec![
                ("Amount", 0.0, 5.0),
                ("Radius", 0.1, 50.0),
                ("Threshold", 0.0, 255.0),
            ],
            DialogKind::AddNoise => vec![("Amount", 0.0, 1.0)],
            DialogKind::ImageSize | DialogKind::CanvasSize | DialogKind::NewDoc => {
                vec![("Width", 1.0, 10000.0), ("Height", 1.0, 10000.0)]
            }
            DialogKind::Feather => vec![("Radius", 0.0, 250.0)],
            DialogKind::Fill => vec![("Opacity", 0.0, 100.0)],
            DialogKind::Curves
            | DialogKind::SaveAs
            | DialogKind::SavePath
            | DialogKind::HistoryLog
            | DialogKind::NewLayer => vec![],
            DialogKind::LayerOptions => vec![("Opacity", 0.0, 100.0)],
            DialogKind::GrowSel | DialogKind::SimilarSel => vec![("Tolerance", 0.0, 255.0)],
        }
    }

    fn slider_display(&self, i: usize) -> String {
        let v = self.vals[i];
        match self.kind {
            DialogKind::Levels if i == 2 => format!("{v:.2}"),
            DialogKind::GaussianBlur => format!("{v:.1}"),
            DialogKind::Unsharp if i <= 1 => format!("{v:.2}"),
            DialogKind::AddNoise => format!("{:.0}%", v * 100.0),
            _ => format!("{}", v.round() as i32),
        }
    }
}

// ---------------------------------------------------------------- app state

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragMode {
    None,
    Paint,
    Pan,
    SelectRect,
    Lasso,
    GradientLine,
    MoveLayer,
    MenuNav,
    PenNew,
    PenDrag,
}

struct StrokeState {
    cmd: TileEditCommand,
    start_point: (f32, f32),
    last_point: (f32, f32),
    smudge_carry: Pixel16,
    tiles_seen: Vec<(i32, i32)>,
}

/// What the canvas is showing: the composite, one color channel,
/// or one alpha/spot channel (as grayscale, or tinted for spot).
#[derive(Clone, Copy, PartialEq, Debug)]
enum ChannelView {
    Composite,
    Channel(usize), // index into the mode's channel list (R,G,B / C,M,Y,K / Gray)
    Alpha(usize),
}

/// Where painting lands: the image itself, or one alpha channel.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PaintTarget {
    Image,
    Alpha(usize),
}

/// A path identity: the unsaved work path or a saved path by index.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PathRef {
    Work,
    Saved(usize),
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
    clone_src: Option<(f32, f32)>,
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
    file_panel: Option<Vec<PathBuf>>,
    // interaction
    cursor: (f32, f32),
    mouse_down: bool,
    drag: DragMode,
    drag_start_doc: (f32, f32),
    drag_cur_doc: (f32, f32),
    lasso_points: Vec<(f32, f32)>,
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
    ants_phase: i32,
    quit: bool,
}

impl App {
    fn new(doc: Document, file_path: Option<PathBuf>) -> Self {
        let mut hist = History::new();
        hist.set_limit(Some(1)); // 1990 rules: a single level of Undo
        App {
            doc,
            hist,
            sel: None,
            tool: Tool::Brush,
            fg: Pixel16::from_rgba8(0, 0, 0, 255),
            bg: Pixel16::from_rgba8(255, 255, 255, 255),
            tip: BrushTip::new(13.0, 0.7),
            opacity: 1.0,
            clone_src: None,
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
            file_panel: None,
            cursor: (0.0, 0.0),
            mouse_down: false,
            drag: DragMode::None,
            drag_start_doc: (0.0, 0.0),
            drag_cur_doc: (0.0, 0.0),
            lasso_points: Vec::new(),
            stroke: None,
            move_orig_offset: (0, 0),
            move_linked: Vec::new(),
            layers_scroll: 0,
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
            ants_phase: 0,
            quit: false,
        }
    }

    fn touch(&mut self) {
        self.doc_version += 1;
        self.dirty = true;
    }

    fn canvas_rect(&self, win_w: u32, win_h: u32) -> Rect {
        let (rx, ry) = if self.show_rulers {
            (ui::RULER_W, ui::RULER_H)
        } else {
            (0, 0)
        };
        let dock = if self.dock_open() { DOCK_W } else { 0 };
        Rect::new(
            ui::TOOLBAR_W + rx,
            ui::MENU_H + ry,
            (win_w as i32 - ui::TOOLBAR_W - rx - dock).max(10),
            (win_h as i32 - ui::MENU_H - ui::STATUS_H - ry).max(10),
        )
    }

    fn dock_open(&self) -> bool {
        self.show_cp || self.show_brushes || self.show_histogram || self.show_palette
    }

    fn channel_row_count(&self) -> usize {
        let base = match self.doc.mode {
            ColorMode::Rgb => 3,
            ColorMode::Cmyk => 4,
            ColorMode::Grayscale => 1,
        };
        1 + base + self.doc.alphas.len()
    }

    fn path_row_count(&self) -> usize {
        (if self.doc.work_path.is_some() { 1 } else { 0 }) + self.doc.paths.len()
    }

    /// The dock's panel groups, stacked top-down on the right edge.
    fn panel_groups(&self) -> Vec<(PanelKind, Rect)> {
        let mut out = Vec::new();
        if !self.dock_open() {
            return out;
        }
        let x = self.win_size.0 as i32 - DOCK_W;
        let bottom = self.win_size.1 as i32 - ui::STATUS_H;
        let mut y = ui::MENU_H;
        let place = |kind: PanelKind, h: i32, out: &mut Vec<(PanelKind, Rect)>, y: &mut i32| {
            if *y + h <= bottom {
                out.push((kind, Rect::new(x, *y, DOCK_W, h)));
                *y += h + 2;
            }
        };
        if self.show_cp {
            let h = match self.cp_tab {
                0 => 70 + self.doc.layers.len().min(8) as i32 * 28,
                1 => 20 + self.channel_row_count() as i32 * 15 + 22,
                _ => 20 + self.path_row_count().max(1) as i32 * 15 + 42,
            };
            place(PanelKind::Cp, h, &mut out, &mut y);
        }
        if self.show_brushes {
            place(PanelKind::Brushes, 84, &mut out, &mut y);
        }
        if self.show_histogram {
            place(PanelKind::Histogram, 132, &mut out, &mut y);
        }
        if self.show_palette {
            place(PanelKind::Color, 168, &mut out, &mut y);
        }
        out
    }

    fn group_rect(&self, kind: PanelKind) -> Option<Rect> {
        self.panel_groups()
            .into_iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, r)| r)
    }

    fn fit_view(&mut self, win_w: u32, win_h: u32) {
        let c = self.canvas_rect(win_w, win_h);
        self.viewport = Viewport::fit(
            self.doc.width as f32,
            self.doc.height as f32,
            c.w as f32,
            c.h as f32,
        );
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

impl App {
    fn build_menus(&self) -> Vec<Menu> {
        let has_sel = self.sel.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
        let undo_label = if self.hist.can_redo() && !self.hist.can_undo() {
            "Redo".to_string()
        } else {
            match self.hist_undo_label() {
                Some(l) => format!("Undo {l}"),
                None => "Undo".to_string(),
            }
        };
        let can_undo = self.hist.can_undo() || self.hist.can_redo();
        vec![
            Menu {
                title: "File",
                items: vec![
                    MenuItem::new("New...", Action::New).key("N"),
                    MenuItem::new("Open...", Action::OpenPanel).key("O"),
                    MenuItem::new("Save", Action::Save).key("S"),
                    MenuItem::new("Save As...", Action::SaveAsPanel),
                    MenuItem::new("History Log...", Action::HistoryLog),
                    MenuItem::new("Export PNG...", Action::ExportPng),
                    MenuItem::new("Revert", Action::Revert)
                        .off_if(!self.dirty && self.file_path.is_none()),
                    MenuItem::new("Quit", Action::Quit),
                ],
            },
            Menu {
                title: "Edit",
                items: vec![
                    MenuItem::new(undo_label, Action::UndoToggle)
                        .off_if(!can_undo)
                        .key("Z"),
                    MenuItem::new("Cut", Action::Cut).off_if(!has_sel),
                    MenuItem::new("Copy", Action::Copy).off_if(!has_sel),
                    MenuItem::new("Paste", Action::Paste)
                        .off_if(CLIPBOARD.with(|c| c.borrow().is_none())),
                    MenuItem::new("Clear", Action::Clear).off_if(!has_sel),
                    MenuItem::new("Fill...", Action::FillDialog),
                ],
            },
            Menu {
                title: "Image",
                items: vec![
                    MenuItem::new("Image Size...", Action::ImageSizeDialog),
                    MenuItem::new("Canvas Size...", Action::CanvasSizeDialog),
                    MenuItem::new("Crop", Action::CropToSelection).off_if(!has_sel),
                    MenuItem::new("Rotate", Action::None).sub(vec![
                        MenuItem::new("180 deg", Action::Rotate180),
                        MenuItem::new("90 deg CW", Action::Rotate90Cw),
                        MenuItem::new("90 deg CCW", Action::Rotate90Ccw),
                    ]),
                    MenuItem::new("Flip", Action::None).sub(vec![
                        MenuItem::new("Horizontal", Action::FlipHorizontal),
                        MenuItem::new("Vertical", Action::FlipVertical),
                    ]),
                    MenuItem::new("Adjust", Action::None).sub(vec![
                        MenuItem::new("Brightness/Contrast...", Action::AdjustDialog("bc")),
                        MenuItem::new("Hue/Saturation...", Action::AdjustDialog("hs")),
                        MenuItem::new("Color Balance...", Action::AdjustDialog("cb")),
                        MenuItem::new("Levels...", Action::AdjustDialog("levels")),
                        MenuItem::new("Curves...", Action::AdjustDialog("curves")),
                        MenuItem::new("Invert", Action::AdjustDialog("invert")).key("I"),
                        MenuItem::new("Desaturate", Action::AdjustDialog("desat")),
                        MenuItem::new("Posterize...", Action::AdjustDialog("posterize")),
                        MenuItem::new("Threshold...", Action::AdjustDialog("threshold")),
                    ]),
                ],
            },
            Menu {
                title: "Layer",
                items: {
                    let a = self.doc.active_layer;
                    let cur = self.doc.layers[a].blend_mode;
                    let blend_items: Vec<MenuItem> = pictodesk_core::blend::ALL_BLEND_MODES
                        .iter()
                        .enumerate()
                        .map(|(i, m)| {
                            MenuItem::new(m.name(), Action::BlendMode(i)).check_if(*m == cur)
                        })
                        .collect();
                    vec![
                        MenuItem::new("New Layer...", Action::NewLayer).key("Shift+N"),
                        MenuItem::new("Duplicate Layer", Action::DuplicateLayer),
                        MenuItem::new("Delete Layer", Action::DeleteLayer)
                            .off_if(self.doc.layers.len() <= 1),
                        MenuItem::new("Layer Options...", Action::LayerOptions),
                        MenuItem::new("Blend Mode", Action::None).sub(blend_items),
                        MenuItem::new("Bring Forward", Action::BringForward).key("]"),
                        MenuItem::new("Send Backward", Action::SendBackward).key("["),
                        MenuItem::new("Free Transform...", Action::FreeTransform).key("T"),
                        MenuItem::new("Merge Down", Action::MergeDown)
                            .key("E")
                            .off_if(a == 0),
                        MenuItem::new("Merge Visible", Action::MergeVisible),
                        MenuItem::new("Flatten Image", Action::FlattenImage),
                    ]
                },
            },
            Menu {
                title: "Mode",
                items: vec![
                    MenuItem::new("RGB Color", Action::ModeRgb)
                        .check_if(self.doc.mode == ColorMode::Rgb),
                    MenuItem::new("Grayscale", Action::ModeGrayscale)
                        .check_if(self.doc.mode == ColorMode::Grayscale),
                    MenuItem::new("CMYK Color", Action::ModeCmyk)
                        .check_if(self.doc.mode == ColorMode::Cmyk),
                ],
            },
            Menu {
                title: "Filter",
                items: vec![
                    MenuItem::new("Gaussian Blur...", Action::FilterGaussian),
                    MenuItem::new("Sharpen", Action::FilterSharpen),
                    MenuItem::new("Unsharp Mask...", Action::FilterUnsharp),
                    MenuItem::new("Add Noise...", Action::FilterNoise),
                ],
            },
            Menu {
                title: "Select",
                items: {
                    let mut items = vec![
                        MenuItem::new("All", Action::SelectAll).key("A"),
                        MenuItem::new("None", Action::SelectNone).key("D"),
                        MenuItem::new("Inverse", Action::SelectInverse).off_if(!has_sel),
                        MenuItem::new("Feather...", Action::FeatherDialog).off_if(!has_sel),
                        MenuItem::new("Grow...", Action::SelectGrow).off_if(!has_sel),
                        MenuItem::new("Similar...", Action::SelectSimilar).off_if(!has_sel),
                        MenuItem::new("Save Selection", Action::SaveSelection).off_if(!has_sel),
                    ];
                    let load_items: Vec<MenuItem> = self
                        .doc
                        .alphas
                        .iter()
                        .enumerate()
                        .map(|(i, ch)| MenuItem::new(ch.name.clone(), Action::LoadSelection(i)))
                        .collect();
                    items.push(
                        MenuItem::new("Load Selection", Action::None)
                            .off_if(load_items.is_empty())
                            .sub(load_items),
                    );
                    items
                },
            },
            Menu {
                title: "Window",
                items: vec![
                    MenuItem::new("Histogram", Action::ToggleHistogram)
                        .check_if(self.show_histogram),
                    MenuItem::new("Brushes", Action::ToggleBrushes).check_if(self.show_brushes),
                    MenuItem::new("Layers", Action::ToggleLayers)
                        .check_if(self.show_cp && self.cp_tab == 0),
                    MenuItem::new("Channels", Action::ToggleChannels)
                        .check_if(self.show_cp && self.cp_tab == 1),
                    MenuItem::new("Paths", Action::TogglePaths)
                        .check_if(self.show_cp && self.cp_tab == 2),
                    MenuItem::new("Color", Action::ToggleColor).check_if(self.show_palette),
                    MenuItem::new("Rulers", Action::ToggleRulers).check_if(self.show_rulers),
                    MenuItem::new("Zoom In", Action::ZoomIn).key("+"),
                    MenuItem::new("Zoom Out", Action::ZoomOut).key("-"),
                    MenuItem::new("Actual Pixels", Action::Zoom100).key("1"),
                    MenuItem::new("Fit on Screen", Action::ZoomFit).key("0"),
                ],
            },
        ]
    }

    fn hist_undo_label(&self) -> Option<String> {
        // History doesn't expose labels of the top command; track via status.
        // We store the last action label in `status` when pushing (see push_status).
        self.last_label.clone()
    }
}

// Small helpers on MenuItem that read better in the builder above.
trait MenuItemExt {
    fn off_if(self, cond: bool) -> Self;
    fn check_if(self, cond: bool) -> Self;
}
impl MenuItemExt for MenuItem {
    fn off_if(mut self, cond: bool) -> Self {
        if cond {
            self.enabled = false;
        }
        self
    }
    fn check_if(mut self, cond: bool) -> Self {
        self.checked = cond;
        self
    }
}

thread_local! {
    /// The 1.0-style internal clipboard: a copied pixel block + size.
    static CLIPBOARD: std::cell::RefCell<Option<(Vec<Pixel16>, u32, u32)>> =
        const { std::cell::RefCell::new(None) };
}

impl App {
    /// Push a finished command onto the history stack and record
    /// the edit in the document's provenance log (stored in the
    /// `.pdk` file, like the classic editors' embedded history).
    fn push_cmd(&mut self, cmd: Box<dyn Command>) {
        self.doc.log_event(cmd.label());
        self.hist.push(cmd);
    }

    fn structural(&mut self, label: &str, f: impl FnOnce(&mut Document)) {
        let mut cmd = DocumentCommand::begin(label, &self.doc);
        f(&mut self.doc);
        cmd.finish(&self.doc);
        self.push_cmd(Box::new(cmd));
        self.last_label = Some(label.to_string());
        self.status = label.to_string();
        self.touch();
    }

    fn begin_free_transform(&mut self) {
        if self.ft.is_some() {
            self.ft_commit();
            return;
        }
        let a = self.doc.active_layer;
        let Some(b) = pictodesk_core::transform::layer_painted_bounds(&self.doc.layers[a]) else {
            self.status = "Free Transform: the active layer is empty".into();
            return;
        };
        let targets: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i == a || l.linked)
            .map(|(i, _)| i)
            .collect();
        self.ft = Some(FtState {
            origin: self.doc.clone(),
            targets,
            bounds: (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32),
            sx: 1.0,
            sy: 1.0,
            rot: 0.0,
            skew: 0.0,
            dx: 0.0,
            dy: 0.0,
            drag: FtDrag::None,
            drag_start_doc: (0.0, 0.0),
            start: (1.0, 1.0, 0.0, 0.0, 0.0, 0.0),
            start_angle: 0.0,
        });
        self.status =
            "Free Transform: drag handles to scale, inside to move, outside to rotate, Ctrl+drag to skew — Enter applies, Esc cancels"
                .into();
    }

    /// Re-derive the document from the FT origin + current params.
    fn ft_apply(&mut self) {
        let Some(ft) = &self.ft else { return };
        let (sx, sy, rot, skew, dx, dy) = (ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy);
        let targets = ft.targets.clone();
        let identity = (sx - 1.0).abs() < 1e-6
            && (sy - 1.0).abs() < 1e-6
            && rot.abs() < 1e-6
            && skew.abs() < 1e-6;
        self.doc = ft.origin.clone();
        for t in &targets {
            if identity {
                let o = self.doc.layers[*t].offset;
                self.doc.layers[*t].offset = (o.0 + dx.round() as i32, o.1 + dy.round() as i32);
            } else {
                pictodesk_core::transform::transform_layer(&mut self.doc, *t, sx, sy, rot, skew);
                self.doc.layers[*t].offset = (dx.round() as i32, dy.round() as i32);
            }
        }
        self.status = format!(
            "Free Transform — W: {}%  H: {}%  Angle: {:.1}°  Skew: {:.1}°",
            (sx * 100.0).round() as i32,
            (sy * 100.0).round() as i32,
            rot,
            skew
        );
        self.touch();
    }

    fn ft_commit(&mut self) {
        if let Some(ft) = self.ft.take() {
            if ft.origin != self.doc {
                let mut cmd = DocumentCommand::begin("Free Transform", &ft.origin);
                cmd.finish(&self.doc);
                self.push_cmd(Box::new(cmd));
                self.last_label = Some("Free Transform".into());
            }
            self.status = "Free Transform applied".into();
            self.touch();
        }
    }

    fn ft_cancel(&mut self) {
        if let Some(ft) = self.ft.take() {
            self.doc = ft.origin;
            self.status = "Free Transform cancelled".into();
            self.touch();
        }
    }

    /// Map a document point through the current FT params (matches
    /// the core transform: scale, then skew, then rotate, about the
    /// bounds center, then translate).
    fn ft_point(ft: &FtState, p: (f32, f32)) -> (f32, f32) {
        let cx = (ft.bounds.0 + ft.bounds.2) / 2.0;
        let cy = (ft.bounds.1 + ft.bounds.3) / 2.0;
        let rel = (p.0 - cx, p.1 - cy);
        let mut v = (rel.0 * ft.sx, rel.1 * ft.sy);
        v.0 += ft.skew.to_radians().tan() * v.1;
        let th = ft.rot.to_radians();
        let (sn, cs) = (th.sin(), th.cos());
        let r = (v.0 * cs - v.1 * sn, v.0 * sn + v.1 * cs);
        (r.0 + cx + ft.dx, r.1 + cy + ft.dy)
    }

    /// The 8 handle positions (doc coords): corners then edge
    /// midpoints, in the transformed frame.
    fn ft_handles(ft: &FtState) -> [(f32, f32); 8] {
        let (x0, y0, x1, y1) = ft.bounds;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let raw = [
            (x0, y0),
            (x1, y0),
            (x1, y1),
            (x0, y1),
            (cx, y0),
            (x1, cy),
            (cx, y1),
            (x0, cy),
        ];
        raw.map(|p| Self::ft_point(ft, p))
    }

    /// Begin an FT drag if the canvas click hits the transform box.
    fn ft_mouse_down(&mut self, dp: (f32, f32), sx: f32, sy: f32) -> bool {
        let Some(ft) = &self.ft else { return false };
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let handles = Self::ft_handles(ft);
        let mut drag = FtDrag::None;
        for (i, h) in handles.iter().enumerate() {
            let s = self.viewport.doc_to_screen(*h);
            let (hx, hy) = (canvas.x as f32 + s.0, canvas.y as f32 + s.1);
            if (sx - hx).abs() <= 7.0 && (sy - hy).abs() <= 7.0 {
                drag = if self.ctrl_down || self.super_down {
                    FtDrag::Skew
                } else {
                    FtDrag::Scale(i)
                };
                break;
            }
        }
        if drag == FtDrag::None {
            // Inside the (axis-aligned bound of the) box: move.
            // Just outside it: rotate.
            let mut bb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for h in &handles {
                bb.0 = bb.0.min(h.0);
                bb.1 = bb.1.min(h.1);
                bb.2 = bb.2.max(h.0);
                bb.3 = bb.3.max(h.1);
            }
            if dp.0 >= bb.0 && dp.0 <= bb.2 && dp.1 >= bb.1 && dp.1 <= bb.3 {
                drag = FtDrag::Move;
            } else {
                let pad = 24.0 / self.viewport.scale.max(0.01);
                if dp.0 >= bb.0 - pad
                    && dp.0 <= bb.2 + pad
                    && dp.1 >= bb.1 - pad
                    && dp.1 <= bb.3 + pad
                {
                    drag = FtDrag::Rotate;
                }
            }
        }
        if drag == FtDrag::None {
            return false;
        }
        let ft = self.ft.as_mut().unwrap();
        ft.drag = drag;
        ft.drag_start_doc = dp;
        ft.start = (ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy);
        let cx = (ft.bounds.0 + ft.bounds.2) / 2.0 + ft.dx;
        let cy = (ft.bounds.1 + ft.bounds.3) / 2.0 + ft.dy;
        ft.start_angle = (dp.1 - cy).atan2(dp.0 - cx);
        true
    }

    fn ft_mouse_move(&mut self, dp: (f32, f32)) {
        let Some(ft) = &mut self.ft else { return };
        if ft.drag == FtDrag::None {
            return;
        }
        let (sx0, sy0, rot0, skew0, dx0, dy0) = ft.start;
        let (x0, y0, x1, y1) = ft.bounds;
        let (bw, bh) = (x1 - x0, y1 - y0);
        let center = ((x0 + x1) / 2.0 + dx0, (y0 + y1) / 2.0 + dy0);
        match ft.drag {
            FtDrag::Move => {
                ft.dx = dx0 + (dp.0 - ft.drag_start_doc.0);
                ft.dy = dy0 + (dp.1 - ft.drag_start_doc.1);
            }
            FtDrag::Rotate => {
                let ang = (dp.1 - center.1).atan2(dp.0 - center.0);
                let mut deg = rot0 + (ang - ft.start_angle).to_degrees();
                while deg > 180.0 {
                    deg -= 360.0;
                }
                while deg < -180.0 {
                    deg += 360.0;
                }
                ft.rot = deg;
            }
            FtDrag::Scale(i) => {
                let hx = [
                    -bw / 2.0,
                    bw / 2.0,
                    bw / 2.0,
                    -bw / 2.0,
                    0.0,
                    bw / 2.0,
                    0.0,
                    -bw / 2.0,
                ][i];
                let hy = [
                    -bh / 2.0,
                    -bh / 2.0,
                    bh / 2.0,
                    bh / 2.0,
                    -bh / 2.0,
                    0.0,
                    bh / 2.0,
                    0.0,
                ][i];
                let th = rot0.to_radians();
                let (sn, cs) = (th.sin(), th.cos());
                let rel = (dp.0 - center.0, dp.1 - center.1);
                let mu = rel.0 * cs + rel.1 * sn;
                let mv = -rel.0 * sn + rel.1 * cs;
                if hx != 0.0 {
                    ft.sx = (mu / hx).clamp(0.05, 20.0);
                }
                if hy != 0.0 {
                    ft.sy = (mv / hy).clamp(0.05, 20.0);
                }
                let _ = (sx0, sy0);
            }
            FtDrag::Skew => {
                let th = rot0.to_radians();
                let (sn, cs) = (th.sin(), th.cos());
                let d = (dp.0 - ft.drag_start_doc.0, dp.1 - ft.drag_start_doc.1);
                let du = d.0 * cs + d.1 * sn;
                let shear = skew0.to_radians().tan() + du / bh.max(1.0);
                ft.skew = shear.atan().to_degrees().clamp(-80.0, 80.0);
            }
            FtDrag::None => {}
        }
        self.ft_apply();
    }

    /// Begin a pen/path edit gesture: snapshot the document so the
    /// whole gesture becomes a single undo step on commit.
    fn begin_path_edit(&mut self) {
        if self.path_before.is_none() {
            self.path_before = Some(self.doc.clone());
        }
    }

    /// Finish a pen/path edit gesture: if the document changed,
    /// push the snapshot pair as one history command.
    fn commit_path_edit(&mut self, label: &str) {
        if let Some(before) = self.path_before.take() {
            if before != self.doc {
                let mut cmd = DocumentCommand::begin(label, &before);
                cmd.finish(&self.doc);
                self.push_cmd(Box::new(cmd));
                self.last_label = Some(label.to_string());
            }
        }
    }

    /// Where this document's crash-recovery file lives.
    fn recovery_target(&self) -> std::path::PathBuf {
        match &self.file_path {
            Some(p) => format::recovery_path_for(p),
            None => std::env::temp_dir().join("pictodesk-untitled.pdk.recovery"),
        }
    }

    /// Periodic autosave: when the document has unsaved changes,
    /// write a recovery copy (atomically) at most every 20 s.
    pub fn autosave_tick(&mut self) {
        if !self.dirty || self.doc_version == self.autosaved_version {
            return;
        }
        if self.autosave_at.elapsed().as_secs() < 20 {
            return;
        }
        let target = self.recovery_target();
        if format::save_atomic(&self.doc, &target).is_ok() {
            self.autosaved_version = self.doc_version;
            self.autosave_at = std::time::Instant::now();
            self.recovery_file = Some(target);
        }
    }

    /// The document is safely stored (or replaced): any recovery
    /// copy is now stale.
    fn clear_recovery(&mut self) {
        let target = self.recovery_target();
        if target.exists() {
            let _ = std::fs::remove_file(&target);
        }
        if let Some(old) = self.recovery_file.take() {
            if old != target && old.exists() {
                let _ = std::fs::remove_file(old);
            }
        }
        self.autosaved_version = self.doc_version;
        self.autosave_at = std::time::Instant::now();
    }

    fn open_path(&mut self, path: &Path) {
        let mut psd_note: Option<String> = None;
        let result = if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("psd"))
            .unwrap_or(false)
        {
            match std::fs::read(path) {
                Ok(bytes) => match pictodesk_core::psd::load_psd(&bytes) {
                    Ok((doc, report)) => {
                        let mut note = format!("PSD imported: {} layer(s)", report.layers_imported);
                        if !report.notes.is_empty() {
                            note.push_str(" — ");
                            note.push_str(&report.notes.join("; "));
                        }
                        psd_note = Some(note);
                        Ok(doc)
                    }
                    Err(e) => Err(e),
                },
                Err(e) => Err(e.to_string()),
            }
        } else if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| format::deserialize(&bytes).map_err(|e| e.to_string()))
        } else {
            io::load_file(path).map_err(|e| e.to_string())
        };
        match result {
            Ok(doc) => {
                self.doc = doc;
                self.doc.log_event("Opened");
                self.hist = History::new();
                self.hist.set_limit(Some(1));
                self.last_label = None;
                self.sel = None;
                self.channel_view = ChannelView::Composite;
                self.paint_target = PaintTarget::Image;
                self.active_path = if self.doc.work_path.is_some() {
                    Some(PathRef::Work)
                } else {
                    None
                };
                self.file_path = Some(path.to_path_buf());
                self.dirty = false;
                self.doc_version += 1;
                if let Some(old) = self.recovery_file.take() {
                    let _ = std::fs::remove_file(old);
                }
                self.autosaved_version = self.doc_version;
                self.autosave_at = std::time::Instant::now();
                self.fit_view(self.win_size.0, self.win_size.1);
                self.status = psd_note.unwrap_or_else(|| format!("Opened {}", path.display()));
            }
            Err(e) => {
                self.status = format!("Open failed: {e}");
            }
        }
    }

    fn save_to(&mut self, path: &Path) -> bool {
        if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            // Record the save in the file's own history log.
            self.doc.log_event("Saved");
        }
        let result = if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            std::fs::write(path, format::serialize(&self.doc)).map_err(|e| e.to_string())
        } else if ExportFormat::from_path(path).is_some() {
            io::save_file(path, &self.doc, 92).map_err(|e| e.to_string())
        } else {
            Err("unknown file extension".to_string())
        };
        match result {
            Ok(()) => {
                self.file_path = Some(path.to_path_buf());
                self.dirty = false;
                self.clear_recovery();
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    fn do_action(&mut self, action: Action) {
        let layer = self.doc.active_layer;
        match action {
            Action::None => {}
            Action::New => {
                let mut d = Dialog::new(DialogKind::NewDoc, &self.doc);
                d.text = "Untitled".into();
                self.dialog = Some(d);
            }
            Action::OpenPanel => {
                let mut files: Vec<PathBuf> = std::fs::read_dir(".")
                    .map(|rd| {
                        rd.filter_map(|e| e.ok().map(|e| e.path()))
                            .filter(|p| {
                                p.extension()
                                    .map(|e| {
                                        let e = e.to_ascii_lowercase();
                                        ["png", "jpg", "jpeg", "tif", "tiff", "bmp", "pdk", "psd"]
                                            .contains(&e.to_str().unwrap_or(""))
                                    })
                                    .unwrap_or(false)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                files.sort();
                self.file_panel = Some(files);
            }
            Action::Save => {
                if let Some(p) = self.file_path.clone() {
                    self.save_to(&p);
                } else {
                    self.do_action(Action::SaveAsPanel);
                }
            }
            Action::SaveAsPanel => {
                let mut d = Dialog::new(DialogKind::SaveAs, &self.doc);
                d.text = self
                    .file_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "untitled.pdk".into());
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::HistoryLog => {
                self.open_dialog(DialogKind::HistoryLog);
            }
            Action::ExportPng => {
                let name = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "untitled".into());
                let path = PathBuf::from(format!("{name}.png"));
                match io::export_image(&self.doc, ExportFormat::Png, 92) {
                    Ok(bytes) => match std::fs::write(&path, bytes) {
                        Ok(()) => self.status = format!("Exported {}", path.display()),
                        Err(e) => self.status = format!("Export failed: {e}"),
                    },
                    Err(e) => self.status = format!("Export failed: {e}"),
                }
            }
            Action::Revert => {
                if let Some(p) = self.file_path.clone() {
                    self.open_path(&p);
                    self.status = "Reverted to saved".into();
                }
            }
            Action::Quit => {
                self.quit = true;
            }
            Action::UndoToggle => {
                if self.hist.can_undo() {
                    if let Some(l) = self.hist.undo(&mut self.doc) {
                        self.status = format!("Undo {l}");
                        self.doc_version += 1;
                    }
                } else if self.hist.can_redo() {
                    if let Some(l) = self.hist.redo(&mut self.doc) {
                        self.status = format!("Redo {l}");
                        self.doc_version += 1;
                    }
                }
            }
            Action::Copy | Action::Cut => {
                if let Some(sel) = &self.sel {
                    if let Some((x, y, w, h)) = sel.bounds() {
                        let mut block = Vec::with_capacity((w * h) as usize);
                        for yy in 0..h as i32 {
                            for xx in 0..w as i32 {
                                block.push(self.doc.layers[layer].get_pixel(x + xx, y + yy));
                            }
                        }
                        CLIPBOARD.with(|c| *c.borrow_mut() = Some((block, w, h)));
                        self.status = "Copied".into();
                        if matches!(action, Action::Cut) {
                            self.clear_selection();
                            self.status = "Cut".into();
                        }
                    }
                }
            }
            Action::Paste => {
                let clip = CLIPBOARD.with(|c| c.borrow().clone());
                if let Some((block, w, h)) = clip {
                    let tiles = Selection::tiles_in_rect(0, 0, w, h);
                    run_tile_edit(&mut self.doc, &mut self.hist, "Paste", layer, &tiles, |d| {
                        for yy in 0..h as i32 {
                            for xx in 0..w as i32 {
                                let p = block[(yy as u32 * w + xx as u32) as usize];
                                if p.a > 0 {
                                    d.layers[layer].set_pixel(xx, yy, p);
                                }
                            }
                        }
                    });
                    self.last_label = Some("Paste".into());
                    self.status = "Pasted".into();
                    self.touch();
                }
            }
            Action::Clear => {
                self.clear_selection();
            }
            Action::FillDialog => {
                self.dialog = Some(Dialog::new(DialogKind::Fill, &self.doc));
            }
            Action::ImageSizeDialog => {
                self.dialog = Some(Dialog::new(DialogKind::ImageSize, &self.doc));
            }
            Action::CanvasSizeDialog => {
                self.dialog = Some(Dialog::new(DialogKind::CanvasSize, &self.doc));
            }
            Action::CropToSelection => {
                if let Some((x, y, w, h)) = self.sel.as_ref().and_then(|s| s.bounds()) {
                    self.structural("Crop", |d| transform::crop(d, x, y, w, h));
                    self.sel = None;
                    self.fit_view(self.win_size.0, self.win_size.1);
                }
            }
            Action::Rotate180 => self.structural("Rotate 180", transform::rotate_180),
            Action::Rotate90Cw => {
                self.structural("Rotate 90 CW", transform::rotate_90_cw);
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::Rotate90Ccw => {
                self.structural("Rotate 90 CCW", transform::rotate_90_ccw);
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::FlipHorizontal => {
                self.structural("Flip Horizontal", transform::flip_horizontal)
            }
            Action::FlipVertical => self.structural("Flip Vertical", transform::flip_vertical),
            Action::AdjustDialog(which) => match which {
                "invert" => {
                    let tiles = self.sel_tiles();
                    let sel = self.sel.clone();
                    run_tile_edit(
                        &mut self.doc,
                        &mut self.hist,
                        "Invert",
                        layer,
                        &tiles,
                        |d| {
                            adjust::invert(d, layer, sel.as_ref());
                        },
                    );
                    self.last_label = Some("Invert".into());
                    self.status = "Invert".into();
                    self.touch();
                }
                "desat" => {
                    let tiles = self.sel_tiles();
                    let sel = self.sel.clone();
                    run_tile_edit(
                        &mut self.doc,
                        &mut self.hist,
                        "Desaturate",
                        layer,
                        &tiles,
                        |d| {
                            adjust::desaturate(d, layer, sel.as_ref());
                        },
                    );
                    self.last_label = Some("Desaturate".into());
                    self.status = "Desaturate".into();
                    self.touch();
                }
                "bc" => self.open_dialog(DialogKind::BrightnessContrast),
                "hs" => self.open_dialog(DialogKind::HueSaturation),
                "cb" => self.open_dialog(DialogKind::ColorBalance),
                "levels" => self.open_dialog(DialogKind::Levels),
                "curves" => self.open_dialog(DialogKind::Curves),
                "posterize" => self.open_dialog(DialogKind::Posterize),
                "threshold" => self.open_dialog(DialogKind::Threshold),
                _ => {}
            },
            Action::ModeRgb => {
                self.structural("RGB Color", |d| {
                    pictodesk_core::mode::convert_mode(d, ColorMode::Rgb)
                });
                self.channel_view = ChannelView::Composite;
            }
            Action::ModeCmyk => {
                self.structural("CMYK Color", |d| {
                    pictodesk_core::mode::convert_mode(d, ColorMode::Cmyk)
                });
                self.channel_view = ChannelView::Composite;
            }
            Action::ModeGrayscale => {
                self.structural("Grayscale", |d| {
                    pictodesk_core::mode::convert_mode(d, ColorMode::Grayscale)
                });
                self.channel_view = ChannelView::Composite;
            }
            Action::SelectGrow => {
                self.dialog = Some(Dialog::new(DialogKind::GrowSel, &self.doc));
            }
            Action::SelectSimilar => {
                self.dialog = Some(Dialog::new(DialogKind::SimilarSel, &self.doc));
            }
            Action::SaveSelection => {
                if let Some(sel) = &self.sel {
                    let idx = self.doc.save_selection_as_channel(sel);
                    self.status = format!("Selection saved as {}", self.doc.alphas[idx].name);
                    self.show_cp = true;
                    self.cp_tab = 1;
                    self.touch();
                }
            }
            Action::LoadSelection(i) => {
                if let Some(sel) = self.doc.channel_as_selection(i) {
                    self.sel = Some(sel);
                    self.status = format!("Loaded {}", self.doc.alphas[i].name);
                }
            }
            Action::NewLayer => {
                let mut d = Dialog::new(DialogKind::NewLayer, &self.doc);
                d.text = format!("Layer {}", self.doc.layers.len() + 1);
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::LayerOptions => {
                let mut d = Dialog::new(DialogKind::LayerOptions, &self.doc);
                d.text = self.doc.layers[self.doc.active_layer].name.clone();
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::FreeTransform => {
                self.begin_free_transform();
            }
            Action::DuplicateLayer => {
                let a = self.doc.active_layer;
                self.structural("Duplicate Layer", |d| {
                    d.duplicate_layer(a);
                });
                self.status = "Layer duplicated".into();
            }
            Action::DeleteLayer => {
                let a = self.doc.active_layer;
                if self.doc.layers.len() > 1 {
                    self.structural("Delete Layer", |d| {
                        d.delete_layer(a);
                    });
                    self.status = "Layer deleted".into();
                } else {
                    self.status = "Cannot delete the only layer".into();
                }
            }
            Action::BlendMode(i) => {
                let m = pictodesk_core::blend::ALL_BLEND_MODES[i];
                self.structural("Blend Mode", |d| {
                    let a = d.active_layer;
                    d.layers[a].blend_mode = m;
                });
                self.status = format!("Blend mode: {}", m.name());
            }
            Action::BringForward => {
                let a = self.doc.active_layer;
                if a + 1 < self.doc.layers.len() {
                    self.structural("Bring Forward", |d| {
                        d.move_layer(a, a + 1);
                    });
                }
            }
            Action::SendBackward => {
                let a = self.doc.active_layer;
                if a > 0 {
                    self.structural("Send Backward", |d| {
                        d.move_layer(a, a - 1);
                    });
                }
            }
            Action::MergeDown => {
                let a = self.doc.active_layer;
                if a > 0 {
                    self.structural("Merge Down", |d| {
                        d.merge_down(a);
                    });
                    self.status = "Merged down".into();
                } else {
                    self.status = "Nothing below to merge into".into();
                }
            }
            Action::MergeVisible => {
                self.structural("Merge Visible", |d| {
                    d.merge_visible();
                });
                self.status = "Visible layers merged".into();
            }
            Action::FlattenImage => {
                self.structural("Flatten Image", |d| {
                    d.flatten();
                });
                self.status = "Image flattened".into();
            }
            Action::ToggleLayers => {
                if self.show_cp && self.cp_tab == 0 {
                    self.show_cp = false;
                } else {
                    self.show_cp = true;
                    self.cp_tab = 0;
                }
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ToggleChannels => {
                if self.show_cp && self.cp_tab == 1 {
                    self.show_cp = false;
                } else {
                    self.show_cp = true;
                    self.cp_tab = 1;
                }
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::TogglePaths => {
                if self.show_cp && self.cp_tab == 2 {
                    self.show_cp = false;
                } else {
                    self.show_cp = true;
                    self.cp_tab = 2;
                }
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ToggleColor => {
                self.show_palette = !self.show_palette;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::FilterGaussian => self.open_dialog(DialogKind::GaussianBlur),
            Action::FilterUnsharp => self.open_dialog(DialogKind::Unsharp),
            Action::FilterNoise => self.open_dialog(DialogKind::AddNoise),
            Action::FilterSharpen => {
                let tiles = self.sel_tiles();
                let sel = self.sel.clone();
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Sharpen",
                    layer,
                    &tiles,
                    |d| {
                        filter::sharpen(d, layer, sel.as_ref());
                    },
                );
                self.last_label = Some("Sharpen".into());
                self.status = "Sharpen".into();
                self.touch();
            }
            Action::SelectAll => {
                self.sel = Some(Selection::all(self.doc.width, self.doc.height));
                self.status = "Select All".into();
            }
            Action::SelectNone => {
                self.sel = None;
                self.status = "Deselected".into();
            }
            Action::SelectInverse => {
                if let Some(s) = &mut self.sel {
                    s.invert();
                    self.status = "Select Inverse".into();
                }
            }
            Action::FeatherDialog => {
                self.dialog = Some(Dialog::new(DialogKind::Feather, &self.doc));
            }
            Action::ToggleHistogram => {
                self.show_histogram = !self.show_histogram;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ToggleBrushes => {
                self.show_brushes = !self.show_brushes;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ToggleRulers => {
                self.show_rulers = !self.show_rulers;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ZoomIn | Action::ZoomOut => {
                let c = self.canvas_rect(self.win_size.0, self.win_size.1);
                let center = (c.w as f32 / 2.0, c.h as f32 / 2.0);
                let f = if matches!(action, Action::ZoomIn) {
                    1.25
                } else {
                    0.8
                };
                self.viewport.zoom_at(center, f);
            }
            Action::Zoom100 => {
                let c = self.canvas_rect(self.win_size.0, self.win_size.1);
                self.viewport.scale = 1.0;
                self.viewport.offset = (
                    (c.w as f32 - self.doc.width as f32) / 2.0,
                    (c.h as f32 - self.doc.height as f32) / 2.0,
                );
            }
            Action::ZoomFit => self.fit_view(self.win_size.0, self.win_size.1),
        }
    }

    fn clear_selection(&mut self) {
        if let PaintTarget::Alpha(a) = self.paint_target {
            let sel = self
                .sel
                .clone()
                .unwrap_or_else(|| Selection::all(self.doc.width, self.doc.height));
            self.structural("Clear", |d| {
                if let Some(ch) = d.alphas.get_mut(a) {
                    for idx in 0..ch.data.len() {
                        let m = sel.raw_mask()[idx];
                        if m > 0 {
                            let cur = ch.data[idx] as u32;
                            ch.data[idx] = (cur * (255 - m as u32) / 255) as u8;
                        }
                    }
                }
            });
            return;
        }
        let layer = self.doc.active_layer;
        if let Some(sel) = self.sel.clone() {
            if let Some((x, y, w, h)) = sel.bounds() {
                let tiles = Selection::tiles_in_rect(x, y, w, h);
                run_tile_edit(&mut self.doc, &mut self.hist, "Clear", layer, &tiles, |d| {
                    for yy in y..y + h as i32 {
                        for xx in x..x + w as i32 {
                            let m = sel.value(xx, yy);
                            if m == 0 {
                                continue;
                            }
                            if m == 255 {
                                d.layers[layer].set_pixel(xx, yy, Pixel16::TRANSPARENT);
                            } else {
                                let p = d.layers[layer].get_pixel(xx, yy);
                                let (r, g, b, a) = p.to_f32s();
                                d.layers[layer].set_pixel(
                                    xx,
                                    yy,
                                    Pixel16::from_f32s(r, g, b, a * (1.0 - m as f32 / 255.0)),
                                );
                            }
                        }
                    }
                });
                self.last_label = Some("Clear".into());
                self.status = "Cleared".into();
                self.touch();
            }
        }
    }

    // ------------------------------------------------------------ dialogs

    fn open_dialog(&mut self, kind: DialogKind) {
        let mut d = Dialog::new(kind.clone(), &self.doc);
        if kind.is_pixel_dialog() {
            let layer = self.doc.active_layer;
            let mut cmd = TileEditCommand::new(kind.title());
            for c in self.sel_tiles() {
                cmd.capture_before(&self.doc, layer, c);
            }
            d.preview = Some(cmd);
        }
        self.dialog = Some(d);
    }

    /// Re-run the dialog's effect from the snapshot (live preview).
    fn dialog_refresh_preview(&mut self) {
        let Some(d) = &mut self.dialog else { return };
        if !d.kind.is_pixel_dialog() {
            return;
        }
        if let Some(cmd) = &mut d.preview {
            cmd.revert(&mut self.doc);
        }
        let layer = self.doc.active_layer;
        let sel = self.sel.clone();
        let s = sel.as_ref();
        let v = &self.dialog.as_ref().unwrap().vals.clone();
        let kind = self.dialog.as_ref().unwrap().kind.clone();
        let pts = self.dialog.as_ref().unwrap().curve_points.clone();
        match kind {
            DialogKind::BrightnessContrast => {
                adjust::brightness_contrast(&mut self.doc, layer, s, v[0] as i32, v[1] as i32)
            }
            DialogKind::HueSaturation => adjust::hue_saturation(
                &mut self.doc,
                layer,
                s,
                v[0] as i32,
                v[1] as i32,
                v[2] as i32,
            ),
            DialogKind::ColorBalance => {
                let g = |r: usize, i: usize| v[r * 3 + i] as i32;
                adjust::color_balance(
                    &mut self.doc,
                    layer,
                    s,
                    (g(0, 0), g(0, 1), g(0, 2)),
                    (g(1, 0), g(1, 1), g(1, 2)),
                    (g(2, 0), g(2, 1), g(2, 2)),
                )
            }
            DialogKind::Levels => adjust::levels(
                &mut self.doc,
                layer,
                s,
                v[0] as u8,
                v[1] as u8,
                v[2],
                v[3] as u8,
                v[4] as u8,
            ),
            DialogKind::Curves => adjust::curves(&mut self.doc, layer, s, &pts),
            DialogKind::Posterize => adjust::posterize(&mut self.doc, layer, s, v[0] as u8),
            DialogKind::Threshold => adjust::threshold(&mut self.doc, layer, s, v[0] as u8),
            DialogKind::GaussianBlur => filter::gaussian_blur(&mut self.doc, layer, s, v[0]),
            DialogKind::Unsharp => {
                filter::unsharp_mask(&mut self.doc, layer, s, v[0], v[1], v[2] as u8)
            }
            DialogKind::AddNoise => filter::add_noise(&mut self.doc, layer, s, v[0], 12345),
            _ => {}
        }
        self.doc_version += 1;
    }

    fn dialog_ok(&mut self) {
        let Some(mut d) = self.dialog.take() else {
            return;
        };
        let layer = self.doc.active_layer;
        match d.kind {
            k if k.is_pixel_dialog() => {
                if let Some(mut cmd) = d.preview.take() {
                    for c in self.sel_tiles() {
                        cmd.capture_after(&self.doc, layer, c);
                    }
                    let title = k.title().to_string();
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some(title.clone());
                    self.status = title;
                }
                self.touch();
            }
            DialogKind::ImageSize => {
                let (w, h) = (
                    d.vals[0].round().max(1.0) as u32,
                    d.vals[1].round().max(1.0) as u32,
                );
                self.structural("Image Size", |doc| transform::resize_image(doc, w, h));
                self.sel = None;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            DialogKind::CanvasSize => {
                let (w, h) = (
                    d.vals[0].round().max(1.0) as u32,
                    d.vals[1].round().max(1.0) as u32,
                );
                let anchor = d.anchor;
                self.structural("Canvas Size", |doc| {
                    transform::canvas_size(doc, w, h, anchor)
                });
                self.sel = None;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            DialogKind::Feather => {
                if let Some(s) = &mut self.sel {
                    s.feather(d.vals[0].round() as u32);
                    self.status = format!("Feathered by {} px", d.vals[0].round() as u32);
                }
            }
            DialogKind::Fill => {
                if let PaintTarget::Alpha(a) = self.paint_target {
                    let sel = self
                        .sel
                        .clone()
                        .unwrap_or_else(|| Selection::all(self.doc.width, self.doc.height));
                    let src = if d.fill_bg { self.bg } else { self.fg };
                    let (r, g, b, _) = src.to_rgba8();
                    let v = ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8;
                    let op = d.vals[0] / 100.0;
                    self.structural("Fill", |doc| {
                        if let Some(ch) = doc.alphas.get_mut(a) {
                            for idx in 0..ch.data.len() {
                                let m = sel.raw_mask()[idx] as f32 / 255.0 * op;
                                if m > 0.0 {
                                    let cur = ch.data[idx] as f32;
                                    ch.data[idx] = (cur + (v as f32 - cur) * m).round() as u8;
                                }
                            }
                        }
                    });
                    self.last_label = Some("Fill".into());
                } else if let Some(sel) = self
                    .sel
                    .clone()
                    .or_else(|| Some(Selection::all(self.doc.width, self.doc.height)))
                {
                    let mut color = if d.fill_bg { self.bg } else { self.fg };
                    color.a = ((d.vals[0] / 100.0) * 65535.0) as u16;
                    if let Some((x, y, w, h)) = sel.bounds() {
                        let tiles = Selection::tiles_in_rect(x, y, w, h);
                        run_tile_edit(
                            &mut self.doc,
                            &mut self.hist,
                            "Fill",
                            layer,
                            &tiles,
                            |doc| {
                                brush::fill_selection(doc, layer, &sel, color);
                            },
                        );
                        self.last_label = Some("Fill".into());
                        self.status = "Fill".into();
                        self.touch();
                    }
                }
            }
            DialogKind::NewDoc => {
                let (w, h) = (
                    d.vals[0].round().max(1.0) as u32,
                    d.vals[1].round().max(1.0) as u32,
                );
                let mut doc = Document::new(w, h);
                doc.layers[0].fill_rect(0, 0, w, h, self.bg);
                doc.log_event("Created");
                self.doc = doc;
                self.channel_view = ChannelView::Composite;
                self.paint_target = PaintTarget::Image;
                self.active_path = None;
                self.hist = History::new();
                self.hist.set_limit(Some(1));
                self.last_label = None;
                self.sel = None;
                self.file_path = None;
                self.dirty = false;
                self.doc_version += 1;
                self.fit_view(self.win_size.0, self.win_size.1);
                self.status = "New image".into();
            }
            DialogKind::SaveAs => {
                let path = PathBuf::from(d.text.trim());
                self.save_to(&path);
            }
            DialogKind::GrowSel | DialogKind::SimilarSel => {
                if let Some(sel) = self.sel.clone() {
                    self.ensure_render_cache();
                    let img = self.cached_render.clone();
                    let tol = d.vals[0].round().clamp(0.0, 255.0) as u8;
                    let (new_sel, label) = if d.kind == DialogKind::GrowSel {
                        (sel.grow(&img, tol), "Grow Selection")
                    } else {
                        (sel.similar(&img, tol), "Select Similar")
                    };
                    self.sel = Some(new_sel);
                    self.status = format!("{label} (tolerance {tol})");
                }
            }
            DialogKind::NewLayer => {
                let name = d.text.trim().to_string();
                let name = if name.is_empty() {
                    format!("Layer {}", self.doc.layers.len() + 1)
                } else {
                    name
                };
                self.structural("New Layer", |doc| {
                    doc.add_layer(name.clone());
                });
                self.status = "New layer".into();
            }
            DialogKind::LayerOptions => {
                let name = d.text.trim().to_string();
                let opacity = (d.vals[0] / 100.0).clamp(0.0, 1.0);
                self.structural("Layer Options", |doc| {
                    let a = doc.active_layer;
                    if !name.is_empty() {
                        doc.layers[a].name = name.clone();
                    }
                    doc.layers[a].opacity = opacity;
                });
                self.status = "Layer options set".into();
            }
            DialogKind::SavePath if self.doc.work_path.is_some() => {
                self.begin_path_edit();
                let mut wp = self.doc.work_path.take().unwrap();
                let name = d.text.trim().to_string();
                wp.name = if name.is_empty() {
                    format!("Path {}", self.doc.paths.len() + 1)
                } else {
                    name
                };
                self.doc.paths.push(wp);
                self.active_path = Some(PathRef::Saved(self.doc.paths.len() - 1));
                self.commit_path_edit("Save Path");
                self.status = format!("Saved path '{}'", self.doc.paths.last().unwrap().name);
                self.touch();
            }
            _ => {}
        }
    }

    fn dialog_cancel(&mut self) {
        if let Some(mut d) = self.dialog.take() {
            if let Some(cmd) = &mut d.preview {
                cmd.revert(&mut self.doc);
                self.doc_version += 1;
            }
            self.status = "Cancelled".into();
        }
    }
}

// ---------------------------------------------------------------- drawing

pub struct DialogLayout {
    pub rect: Rect,
    pub slider_rows: Vec<Rect>,
    pub ok: Rect,
    pub cancel: Rect,
    pub graph: Option<Rect>,
    pub range_buttons: Vec<Rect>,
    pub anchor_cells: Vec<Rect>,
    pub fill_buttons: Vec<Rect>,
    pub text_field: Option<Rect>,
}

impl App {
    fn menu_title_rects(&self) -> Vec<(Rect, usize)> {
        let menus = self.build_menus();
        let mut x = 6;
        let mut out = Vec::new();
        for (i, m) in menus.iter().enumerate() {
            let w = Frame::text_width(m.title, 1) + 14;
            out.push((Rect::new(x, 0, w, ui::MENU_H), i));
            x += w + 2;
        }
        out
    }

    fn dropdown_rect(&self, menu_idx: usize) -> Rect {
        let menus = self.build_menus();
        let titles = self.menu_title_rects();
        let (tr, _) = titles[menu_idx];
        let m = &menus[menu_idx];
        let w = m
            .items
            .iter()
            .map(|it| {
                Frame::text_width(&it.label, 1)
                    + it.shortcut
                        .as_deref()
                        .map(|sc| Frame::text_width(sc, 1) + 12)
                        .unwrap_or(0)
                    + 44
            })
            .max()
            .unwrap_or(120);
        Rect::new(tr.x, ui::MENU_H, w, m.items.len() as i32 * 17 + 6)
    }

    fn dialog_layout(&self) -> Option<DialogLayout> {
        let d = self.dialog.as_ref()?;
        let (ww, wh) = (self.win_size.0 as i32, self.win_size.1 as i32);
        let sliders = d.slider_defs().len() as i32;
        let mut h = 46 + sliders * 20 + 40;
        let mut graph = None;
        if d.kind == DialogKind::Curves {
            h = 46 + 176 + 40;
            graph = Some(Rect::new(0, 0, 0, 0)); // positioned below
        }
        if matches!(d.kind, DialogKind::Levels | DialogKind::Threshold) {
            h += 34; // histogram strip
        }
        if d.kind == DialogKind::ColorBalance {
            h += 22; // range buttons
        }
        if d.kind == DialogKind::CanvasSize {
            h += 52; // anchor grid
        }
        if d.kind == DialogKind::Fill {
            h += 22;
        }
        if d.kind == DialogKind::HistoryLog {
            h = 46 + 15 * 13 + 40;
        }
        if matches!(
            d.kind,
            DialogKind::SaveAs
                | DialogKind::NewDoc
                | DialogKind::NewLayer
                | DialogKind::LayerOptions
        ) {
            h += 24; // text field
        }
        let w = 400;
        let rect = Rect::new((ww - w) / 2, (wh - h) / 2, w, h);
        let mut y = rect.y + 30;
        if matches!(d.kind, DialogKind::Levels | DialogKind::Threshold) {
            y += 34;
        }
        if d.kind == DialogKind::ColorBalance || d.kind == DialogKind::Fill {
            y += 22;
        }
        if matches!(
            d.kind,
            DialogKind::SaveAs
                | DialogKind::NewDoc
                | DialogKind::NewLayer
                | DialogKind::LayerOptions
        ) {
            y += 24;
        }
        let mut slider_rows = Vec::new();
        for i in 0..sliders {
            slider_rows.push(Rect::new(rect.x + 12, y + i * 20, w - 24, 14));
        }
        if let Some(g) = &mut graph {
            *g = Rect::new(rect.x + 12, rect.y + 30, 160, 160);
        }
        let ok = Rect::new(rect.x + w - 176, rect.y + h - 32, 80, 24);
        let cancel = Rect::new(rect.x + w - 88, rect.y + h - 32, 80, 24);
        let range_buttons = if d.kind == DialogKind::ColorBalance || d.kind == DialogKind::Fill {
            let n = if d.kind == DialogKind::ColorBalance {
                3
            } else {
                2
            };
            (0..n)
                .map(|i| Rect::new(rect.x + 12 + i * 100, rect.y + 30, 96, 18))
                .collect()
        } else {
            Vec::new()
        };
        let anchor_cells = if d.kind == DialogKind::CanvasSize {
            let gx = rect.x + w - 70;
            let gy = rect.y + h - 100;
            let mut cells = Vec::new();
            for row in 0..3 {
                for col in 0..3 {
                    cells.push(Rect::new(gx + col * 16, gy + row * 16, 15, 15));
                }
            }
            cells
        } else {
            Vec::new()
        };
        let text_field = if matches!(
            d.kind,
            DialogKind::SaveAs
                | DialogKind::NewDoc
                | DialogKind::NewLayer
                | DialogKind::LayerOptions
        ) {
            Some(Rect::new(rect.x + 12, rect.y + 28, w - 24, 18))
        } else {
            None
        };
        Some(DialogLayout {
            rect,
            slider_rows,
            ok,
            cancel,
            graph,
            range_buttons: range_buttons.clone(),
            anchor_cells,
            fill_buttons: range_buttons,
            text_field,
        })
    }

    fn draw(&mut self, f: &mut Frame) {
        self.draw_canvas(f);
        if self.show_rulers {
            self.draw_rulers(f);
        }
        self.draw_toolbox(f);
        if self.dock_open() {
            f.fill_rect(
                Rect::new(
                    f.w as i32 - DOCK_W,
                    ui::MENU_H,
                    DOCK_W,
                    f.h as i32 - ui::MENU_H - ui::STATUS_H,
                ),
                ui::PLATINUM,
            );
        }
        if self.show_palette {
            self.draw_palette(f);
        }
        if self.show_brushes {
            self.draw_brushes(f);
        }
        if self.show_histogram {
            self.draw_histogram_panel(f);
        }
        if self.show_cp {
            self.draw_channels_paths(f);
        }
        self.draw_menu_bar(f);
        self.draw_status(f);
        if self.file_panel.is_some() {
            self.draw_file_panel(f);
        }
        if self.dialog.is_some() {
            self.draw_dialog(f);
        }
    }

    fn ensure_render_cache(&mut self) {
        if self.cached_size != (self.doc.width, self.doc.height) || self.cached_render.is_empty() {
            self.cached_render = self.doc.render_full_rgba8();
            self.cached_size = (self.doc.width, self.doc.height);
            self.rendered_version = self.doc_version;
        } else if self.rendered_version != self.doc_version {
            self.cached_render = self.doc.render_full_rgba8();
            self.rendered_version = self.doc_version;
        }
    }

    /// Classic rulers along the top and left of the canvas:
    /// pixel coordinates in document space, tick spacing adapts to
    /// zoom, and a marker tracks the cursor on each ruler.
    fn draw_rulers(&self, f: &mut Frame) {
        let c = self.canvas_rect(f.w, f.h);
        let top = Rect::new(
            ui::TOOLBAR_W,
            ui::MENU_H,
            c.right() - ui::TOOLBAR_W,
            ui::RULER_H,
        );
        let left = Rect::new(
            ui::TOOLBAR_W,
            ui::MENU_H,
            ui::RULER_W,
            c.bottom() - ui::MENU_H,
        );
        f.fill_rect(top, ui::PLATINUM);
        f.fill_rect(left, ui::PLATINUM);
        f.fill_rect(
            Rect::new(ui::TOOLBAR_W, ui::MENU_H, ui::RULER_W, ui::RULER_H),
            ui::PLATINUM_LIGHT,
        );
        f.fill_rect(
            Rect::new(
                ui::TOOLBAR_W,
                ui::MENU_H + ui::RULER_H - 1,
                c.right() - ui::TOOLBAR_W,
                1,
            ),
            ui::DARK,
        );
        f.fill_rect(
            Rect::new(
                ui::TOOLBAR_W + ui::RULER_W - 1,
                ui::MENU_H,
                1,
                c.bottom() - ui::MENU_H,
            ),
            ui::DARK,
        );
        let scale = self.viewport.scale;
        let steps = [1i32, 2, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000];
        let step = steps
            .iter()
            .copied()
            .find(|s| *s as f32 * scale >= 48.0)
            .unwrap_or(10000);
        // Horizontal ticks + labels.
        let d0 = self.viewport.screen_to_doc((0.0, 0.0)).0;
        let mut p = (d0 / step as f32).floor() as i32 * step;
        loop {
            let sx = c.x + (p as f32 * scale + self.viewport.offset.0) as i32;
            if sx > c.right() {
                break;
            }
            if sx >= c.x {
                f.fill_rect(Rect::new(sx, ui::MENU_H + ui::RULER_H - 6, 1, 6), ui::DARK);
                f.text(sx + 3, ui::MENU_H + 3, &p.to_string(), 1, ui::BLACK);
            }
            p += step;
        }
        // Vertical ticks + labels.
        let e0 = self.viewport.screen_to_doc((0.0, 0.0)).1;
        let mut q = (e0 / step as f32).floor() as i32 * step;
        loop {
            let sy = c.y + (q as f32 * scale + self.viewport.offset.1) as i32;
            if sy > c.bottom() {
                break;
            }
            if sy >= c.y {
                f.fill_rect(
                    Rect::new(ui::TOOLBAR_W + ui::RULER_W - 6, sy, 6, 1),
                    ui::DARK,
                );
                // Vertical labels read bottom-to-top, rotated like
                // the classic rulers, centered on their tick.
                let label = q.to_string();
                let lw = Frame::text_width(&label, 1);
                f.text_vertical(ui::TOOLBAR_W + 5, sy + lw / 2, &label, 1, ui::BLACK);
            }
            q += step;
        }
        // Cursor markers.
        let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
        if c.contains(mx, my) {
            f.fill_rect(Rect::new(mx, ui::MENU_H + ui::RULER_H - 9, 1, 9), ui::BLACK);
            f.fill_rect(
                Rect::new(ui::TOOLBAR_W + ui::RULER_W - 9, my, 9, 1),
                ui::BLACK,
            );
        }
    }

    /// The on-screen color for one document pixel when a single
    /// channel is being viewed (grayscale, or tinted for spot).
    fn view_pixel(&self, dx: i32, dy: i32, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        match self.channel_view {
            ChannelView::Composite => (r, g, b),
            ChannelView::Channel(ci) => {
                let v = match self.doc.mode {
                    ColorMode::Rgb => [r, g, b][ci.min(2)],
                    ColorMode::Grayscale => {
                        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) as u8
                    }
                    ColorMode::Cmyk => {
                        // The stored inks themselves (the document
                        // holds real CMYK values now).
                        let (c, m, y, k) = self.doc.render_cmyk_pixel(dx, dy);
                        let d = |v: u16| ((v as u32 + 128) / 257) as u8;
                        255 - [d(c), d(m), d(y), d(k)][ci.min(3)]
                    }
                };
                (v, v, v)
            }
            ChannelView::Alpha(ai) => {
                let Some(ch) = self.doc.alphas.get(ai) else {
                    return (r, g, b);
                };
                let v = ch.value(self.doc.width, dx, dy);
                match ch.spot_color {
                    Some((sr, sg, sb)) => (
                        255 - ((255 - sr) as u32 * v as u32 / 255) as u8,
                        255 - ((255 - sg) as u32 * v as u32 / 255) as u8,
                        255 - ((255 - sb) as u32 * v as u32 / 255) as u8,
                    ),
                    None => (v, v, v),
                }
            }
        }
    }

    fn draw_canvas(&mut self, f: &mut Frame) {
        self.ensure_render_cache();
        let c = self.canvas_rect(f.w, f.h);
        f.fill_rect(c, ui::CANVAS_SURROUND);
        let origin = (
            c.x as f32 + self.viewport.offset.0,
            c.y as f32 + self.viewport.offset.1,
        );
        let scale = self.viewport.scale;
        let dw = (self.doc.width as f32 * scale) as i32;
        let dh = (self.doc.height as f32 * scale) as i32;
        let doc_rect = Rect::new(origin.0 as i32, origin.1 as i32, dw, dh);
        // clip doc rect to canvas
        let x0 = doc_rect.x.max(c.x);
        let y0 = doc_rect.y.max(c.y);
        let x1 = doc_rect.right().min(c.right());
        let y1 = doc_rect.bottom().min(c.bottom());
        let (docw, doch) = (self.doc.width as i32, self.doc.height as i32);
        for sy in y0..y1 {
            for sx in x0..x1 {
                let dx = ((sx - doc_rect.x) as f32 / scale) as i32;
                let dy = ((sy - doc_rect.y) as f32 / scale) as i32;
                if dx < 0 || dy < 0 || dx >= docw || dy >= doch {
                    continue;
                }
                let i = ((dy * docw + dx) * 4) as usize;
                let (r, g, b, a) = (
                    self.cached_render[i],
                    self.cached_render[i + 1],
                    self.cached_render[i + 2],
                    self.cached_render[i + 3],
                );
                if self.channel_view != ChannelView::Composite {
                    let v = self.view_pixel(dx, dy, r, g, b);
                    f.set(sx, sy, v);
                    continue;
                }
                if a == 255 {
                    f.set(sx, sy, (r, g, b));
                } else {
                    let cell = (((sx / 8) + (sy / 8)) % 2 == 0) as u8;
                    let bgc = if cell == 0 { 255 } else { 192 };
                    let af = a as f32 / 255.0;
                    f.set(
                        sx,
                        sy,
                        (
                            (r as f32 * af + bgc as f32 * (1.0 - af)) as u8,
                            (g as f32 * af + bgc as f32 * (1.0 - af)) as u8,
                            (b as f32 * af + bgc as f32 * (1.0 - af)) as u8,
                        ),
                    );
                }
            }
        }
        // drop shadow edge, like a sheet of paper on the surround
        f.fill_rect(
            Rect::new(doc_rect.x + 3, doc_rect.bottom(), doc_rect.w, 3),
            (70, 70, 70),
        );
        f.fill_rect(
            Rect::new(doc_rect.right(), doc_rect.y + 3, 3, doc_rect.h),
            (70, 70, 70),
        );

        // selection marching ants on the selection bounds
        if let Some(sel) = &self.sel {
            if let Some((bx, by, bw, bh)) = sel.bounds() {
                let tl = self.viewport.doc_to_screen((bx as f32, by as f32));
                let br = self
                    .viewport
                    .doc_to_screen(((bx + bw as i32) as f32, (by + bh as i32) as f32));
                let r = Rect::new(
                    c.x + tl.0 as i32,
                    c.y + tl.1 as i32,
                    (br.0 - tl.0) as i32,
                    (br.1 - tl.1) as i32,
                );
                f.marching_ants_rect(r, self.ants_phase);
            }
        }
        // drag previews
        match self.drag {
            DragMode::SelectRect => {
                let a = self.viewport.doc_to_screen(self.drag_start_doc);
                let b = self.viewport.doc_to_screen(self.drag_cur_doc);
                let r = Rect::new(
                    c.x + a.0.min(b.0) as i32,
                    c.y + a.1.min(b.1) as i32,
                    (a.0 - b.0).abs() as i32,
                    (a.1 - b.1).abs() as i32,
                );
                f.marching_ants_rect(r, self.ants_phase);
            }
            DragMode::Lasso => {
                let pts: Vec<(i32, i32)> = self
                    .lasso_points
                    .iter()
                    .map(|p| {
                        let s = self.viewport.doc_to_screen(*p);
                        (c.x + s.0 as i32, c.y + s.1 as i32)
                    })
                    .collect();
                for w in pts.windows(2) {
                    f.line(w[0].0, w[0].1, w[1].0, w[1].1, ui::WHITE);
                }
            }
            DragMode::GradientLine => {
                let a = self.viewport.doc_to_screen(self.drag_start_doc);
                let b = self.viewport.doc_to_screen(self.drag_cur_doc);
                f.line(
                    c.x + a.0 as i32,
                    c.y + a.1 as i32,
                    c.x + b.0 as i32,
                    c.y + b.1 as i32,
                    ui::WHITE,
                );
            }
            _ => {}
        }
        // Free Transform box: outline + 8 handles.
        if let Some(ft) = &self.ft {
            let handles = Self::ft_handles(ft);
            let to_scr = |p: (f32, f32)| {
                let s = self.viewport.doc_to_screen(p);
                (c.x + s.0 as i32, c.y + s.1 as i32)
            };
            let corners = [
                to_scr(handles[0]),
                to_scr(handles[1]),
                to_scr(handles[2]),
                to_scr(handles[3]),
            ];
            for i in 0..4 {
                let a = corners[i];
                let b = corners[(i + 1) % 4];
                f.line(a.0, a.1, b.0, b.1, ui::WHITE);
            }
            for h in &handles {
                let hs = to_scr(*h);
                let r = Rect::new(hs.0 - 3, hs.1 - 3, 7, 7);
                f.fill_rect(r, ui::WHITE);
                f.outline_rect(r, ui::BLACK);
            }
        }
        // path overlay: the active path (work or selected saved path)
        if let Some((_, path)) = self.active_path_clone() {
            let to_scr = |p: (f32, f32)| {
                let s = self.viewport.doc_to_screen(p);
                (c.x + s.0 as i32, c.y + s.1 as i32)
            };
            let ink = (50, 50, 190);
            for sub in &path.subpaths {
                let pts = sub.flatten();
                for w in pts.windows(2) {
                    let a = to_scr(w[0]);
                    let b = to_scr(w[1]);
                    f.line(a.0, a.1, b.0, b.1, ink);
                }
                if sub.closed && pts.len() > 2 {
                    let a = to_scr(*pts.last().unwrap());
                    let b = to_scr(pts[0]);
                    f.line(a.0, a.1, b.0, b.1, ink);
                }
            }
            if self.tool == Tool::Pen {
                for (si, sub) in path.subpaths.iter().enumerate() {
                    for (ai, a) in sub.anchors.iter().enumerate() {
                        let pnt = to_scr(a.point);
                        if self.pen_anchor == Some((si, ai)) && !a.is_corner() {
                            let ic = to_scr(a.in_ctrl);
                            let oc = to_scr(a.out_ctrl);
                            f.line(pnt.0, pnt.1, ic.0, ic.1, ui::SHADOW);
                            f.line(pnt.0, pnt.1, oc.0, oc.1, ui::SHADOW);
                            f.fill_rect(Rect::new(ic.0 - 2, ic.1 - 2, 4, 4), ink);
                            f.fill_rect(Rect::new(oc.0 - 2, oc.1 - 2, 4, 4), ink);
                        }
                        f.fill_rect(Rect::new(pnt.0 - 3, pnt.1 - 3, 6, 6), ui::WHITE);
                        f.outline_rect(Rect::new(pnt.0 - 3, pnt.1 - 3, 6, 6), ink);
                    }
                }
            }
        }
        // brush cursor ring
        if matches!(
            self.tool,
            Tool::Brush
                | Tool::Pencil
                | Tool::Eraser
                | Tool::Stamp
                | Tool::Blur
                | Tool::Sharpen
                | Tool::Smudge
        ) && self.dialog.is_none()
        {
            let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
            if c.contains(mx, my) {
                let r = (self.tip.size * scale / 2.0) as i32;
                for a in 0..64 {
                    let t = a as f32 / 64.0 * std::f32::consts::TAU;
                    f.set(
                        mx + (t.cos() * r as f32) as i32,
                        my + (t.sin() * r as f32) as i32,
                        ui::WHITE,
                    );
                }
            }
        }
    }

    fn tool_rect(i: usize) -> Rect {
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        Rect::new(3 + col * 25, ui::MENU_H + 4 + row * 25, 24, 24)
    }

    fn draw_toolbox(&mut self, f: &mut Frame) {
        let area = Rect::new(
            0,
            ui::MENU_H,
            ui::TOOLBAR_W,
            f.h as i32 - ui::MENU_H - ui::STATUS_H,
        );
        f.fill_rect(area, ui::PLATINUM);
        f.fill_rect(Rect::new(area.right() - 1, area.y, 1, area.h), ui::SHADOW);
        for (i, t) in TOOLS.iter().enumerate() {
            let r = Self::tool_rect(i);
            let selected = *t == self.tool
                || (self.tool == Tool::MarqueeEllipse && *t == Tool::MarqueeRect && false);
            if selected {
                f.fill_rect(r, ui::PLATINUM_LIGHT);
                f.bevel_sunken(r);
            }
            draw_tool_icon(f, *t, r);
        }
        // foreground/background swatches (classic layout: FG overlaps BG)
        let sy = area.bottom() - 52;
        f.fill_rect(Rect::new(4, sy - 18, area.w - 8, 1), ui::SHADOW);
        f.fill_rect(Rect::new(4, sy - 17, area.w - 8, 1), ui::WHITE);
        f.text(6, sy - 12, "Color", 1, ui::BLACK);
        let bg_r = Rect::new(24, sy + 12, 20, 20);
        let fg_r = Rect::new(10, sy, 20, 20);
        let (br, bgc, bb, _) = self.bg.to_rgba8();
        let (fr, fgc, fb, _) = self.fg.to_rgba8();
        f.fill_rect(bg_r, (br, bgc, bb));
        f.outline_rect(bg_r, ui::BLACK);
        f.fill_rect(fg_r, (fr, fgc, fb));
        f.outline_rect(fg_r, ui::BLACK);
        // default colors mini-swatches
        f.fill_rect(Rect::new(40, sy, 8, 8), (255, 255, 255));
        f.outline_rect(Rect::new(40, sy, 8, 8), ui::BLACK);
        f.fill_rect(Rect::new(36, sy + 4, 8, 8), (0, 0, 0));
        f.outline_rect(Rect::new(36, sy + 4, 8, 8), ui::BLACK);
    }

    fn swatch_default_rect(&self) -> Rect {
        let area_bottom = self.win_size.1 as i32 - ui::STATUS_H;
        Rect::new(36, area_bottom - 52, 12, 12)
    }

    fn draw_palette(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Color) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Color", 1, ui::BLACK);
        for (i, c) in ui::PALETTE.iter().enumerate() {
            let cell = Rect::new(
                r.x + 5 + (i % 8) as i32 * 17,
                r.y + 18 + (i / 8) as i32 * 17,
                16,
                16,
            );
            f.fill_rect(cell, *c);
            f.outline_rect(cell, ui::SHADOW);
        }
    }

    fn palette_cell_rect(&self, i: usize) -> Rect {
        let r = self
            .group_rect(PanelKind::Color)
            .unwrap_or(Rect::new(0, 0, 0, 0));
        Rect::new(
            r.x + 5 + (i % 8) as i32 * 17,
            r.y + 18 + (i / 8) as i32 * 17,
            16,
            16,
        )
    }

    fn draw_brushes(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Brushes) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Brushes", 1, ui::BLACK);
        f.text(
            r.x + 150,
            r.y + 5,
            &format!("{} px", self.tip.size.round() as i32),
            1,
            ui::BLACK,
        );
        let sizes = [1.0f32, 3.0, 5.0, 9.0, 13.0, 21.0, 35.0, 50.0];
        for (row, hardness) in [(0, 1.0f32), (1, 0.0f32)] {
            for (i, s) in sizes.iter().enumerate() {
                let cx = r.x + 16 + i as i32 * 23;
                let cy = r.y + 30 + row * 26;
                let rad = (s / 2.0).min(8.0) as i32;
                let col = if hardness == 1.0 {
                    ui::BLACK
                } else {
                    ui::SHADOW
                };
                for yy in -rad..=rad {
                    for xx in -rad..=rad {
                        if xx * xx + yy * yy <= rad * rad {
                            f.set(cx + xx, cy + yy, col);
                        }
                    }
                }
                if (self.tip.size - s).abs() < 0.1 && (self.tip.hardness - hardness).abs() < 0.1 {
                    f.outline_rect(Rect::new(cx - 10, cy - 10, 20, 20), ui::BLACK);
                }
            }
        }
    }

    fn brushes_cell(&self, i: usize, row: usize) -> Rect {
        let r = self
            .group_rect(PanelKind::Brushes)
            .unwrap_or(Rect::new(0, 0, 0, 0));
        Rect::new(r.x + 5 + i as i32 * 23, r.y + 19 + row as i32 * 26, 22, 22)
    }

    fn draw_histogram_panel(&mut self, f: &mut Frame) {
        self.ensure_render_cache();
        let Some(r) = self.group_rect(PanelKind::Histogram) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Histogram", 1, ui::BLACK);
        let mut lum = [0u32; 256];
        for px in self.cached_render.as_chunks::<4>().0 {
            let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as usize;
            lum[l.min(255)] += 1;
        }
        let peak = lum.iter().max().copied().unwrap_or(1).max(1);
        let graph = Rect::new(r.x + 8, r.y + 18, r.w - 16, 84);
        f.fill_rect(graph, ui::WHITE);
        f.bevel_sunken(graph);
        for i in 0..(r.w - 16) {
            let bin = lum[i as usize * 256 / (r.w - 16) as usize];
            let hgt = ((bin as f32 / peak as f32).sqrt() * graph.h as f32) as i32;
            f.fill_rect(
                Rect::new(graph.x + i, graph.bottom() - hgt, 1, hgt),
                ui::BLACK,
            );
        }
        let hist = Histogram::of_document(&self.doc);
        f.text(
            r.x + 8,
            r.bottom() - 14,
            &format!(
                "Mean {:.0}  Median {}",
                hist.mean_luminance(),
                hist.median_luminance()
            ),
            1,
            ui::BLACK,
        );
    }

    // ------------------------------------------------------------ dock panels

    fn channel_rows(&self) -> Vec<(String, ChannelView)> {
        let mut rows = Vec::new();
        let comp = match self.doc.mode {
            ColorMode::Rgb => "RGB",
            ColorMode::Cmyk => "CMYK",
            ColorMode::Grayscale => "Gray",
        };
        rows.push((comp.to_string(), ChannelView::Composite));
        let names: &[&str] = match self.doc.mode {
            ColorMode::Rgb => &["Red", "Green", "Blue"],
            ColorMode::Cmyk => &["Cyan", "Magenta", "Yellow", "Black"],
            ColorMode::Grayscale => &["Gray"],
        };
        for (i, n) in names.iter().enumerate() {
            rows.push((n.to_string(), ChannelView::Channel(i)));
        }
        for (i, ch) in self.doc.alphas.iter().enumerate() {
            let mut label = ch.name.clone();
            if ch.spot_color.is_some() {
                label += " (spot)";
            }
            rows.push((label, ChannelView::Alpha(i)));
        }
        rows
    }

    fn path_rows(&self) -> Vec<(String, PathRef)> {
        let mut rows = Vec::new();
        if self.doc.work_path.is_some() {
            rows.push(("Work Path".to_string(), PathRef::Work));
        }
        for (i, p) in self.doc.paths.iter().enumerate() {
            let mut label = if p.name.is_empty() {
                format!("Path {}", i + 1)
            } else {
                p.name.clone()
            };
            if self.doc.clipping_path == Some(i) {
                label += " (clipping)";
            }
            rows.push((label, PathRef::Saved(i)));
        }
        rows
    }

    fn draw_small_button(f: &mut Frame, r: Rect, label: &str, enabled: bool) {
        f.fill_rect(r, ui::PLATINUM);
        f.bevel_raised(r);
        let c = if enabled { ui::BLACK } else { ui::DISABLED };
        let tw = Frame::text_width(label, 1);
        f.text(r.x + (r.w - tw) / 2, r.y + 4, label, 1, c);
    }

    fn draw_channels_paths(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Cp) else {
            return;
        };
        f.panel(r);
        for (i, label) in ["Layers", "Channels", "Paths"].iter().enumerate() {
            let tr = Self::cp_tab_rect(r, i);
            if self.cp_tab == i {
                f.fill_rect(tr, ui::PLATINUM_LIGHT);
                f.bevel_raised(tr);
                f.text(tr.x + 5, tr.y + 3, label, 1, ui::BLACK);
            } else {
                f.outline_rect(tr, ui::SHADOW);
                f.text(tr.x + 5, tr.y + 3, label, 1, ui::DARK);
            }
        }
        if self.cp_tab == 0 {
            self.draw_layers_panel(f, r);
        } else if self.cp_tab == 1 {
            let rows = self.channel_rows();
            for (i, (label, view)) in rows.iter().enumerate() {
                let rr = Rect::new(r.x + 2, r.y + 20 + i as i32 * 15, r.w - 4, 15);
                let selected = self.channel_view == *view;
                if selected {
                    f.fill_rect(rr, ui::BLACK);
                }
                let c = if selected { ui::WHITE } else { ui::BLACK };
                if let ChannelView::Alpha(a) = view {
                    if self.paint_target == PaintTarget::Alpha(*a) {
                        f.fill_rect(Rect::new(rr.x + 3, rr.y + 5, 5, 5), c);
                    }
                }
                f.text(rr.x + 13, rr.y + 3, label, 1, c);
            }
            let labels = ["Save", "Load", "New", "Del", "Spot"];
            let bw = (r.w - 8) / 5;
            for (i, label) in labels.iter().enumerate() {
                let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
                Self::draw_small_button(f, br, label, true);
            }
        } else {
            let rows = self.path_rows();
            if rows.is_empty() {
                f.text(r.x + 8, r.y + 24, "No paths yet.", 1, ui::DARK);
                f.text(r.x + 8, r.y + 36, "Draw with the Pen (P),", 1, ui::DARK);
                f.text(r.x + 8, r.y + 48, "or Make from a selection.", 1, ui::DARK);
            }
            for (i, (label, pref)) in rows.iter().enumerate() {
                let rr = Rect::new(r.x + 2, r.y + 20 + i as i32 * 15, r.w - 4, 15);
                let selected = self.active_path == Some(*pref);
                if selected {
                    f.fill_rect(rr, ui::BLACK);
                }
                let c = if selected { ui::WHITE } else { ui::BLACK };
                f.text(rr.x + 6, rr.y + 3, label, 1, c);
            }
            let row1 = ["Fill", "Stroke", "Select", "Make"];
            let row2 = ["Save", "Clip", "Del"];
            let bw = (r.w - 8) / 4;
            for (i, label) in row1.iter().enumerate() {
                let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 39, bw - 2, 16);
                Self::draw_small_button(f, br, label, true);
            }
            let bw2 = (r.w - 8) / 3;
            for (i, label) in row2.iter().enumerate() {
                let br = Rect::new(r.x + 4 + i as i32 * bw2, r.bottom() - 19, bw2 - 2, 16);
                Self::draw_small_button(f, br, label, true);
            }
        }
    }

    fn cp_tab_rect(r: Rect, i: usize) -> Rect {
        match i {
            0 => Rect::new(r.x + 3, r.y + 3, 52, 15),
            1 => Rect::new(r.x + 58, r.y + 3, 70, 15),
            _ => Rect::new(r.x + 131, r.y + 3, 50, 15),
        }
    }

    fn layer_row_rect(r: Rect, row: usize) -> Rect {
        Rect::new(r.x + 2, r.y + 48 + row as i32 * 28, r.w - 4, 28)
    }

    fn draw_layers_panel(&mut self, f: &mut Frame, r: Rect) {
        let a = self.doc.active_layer;
        let (mode_name, opacity) = {
            let l = &self.doc.layers[a];
            (l.blend_mode.name(), (l.opacity * 100.0).round() as i32)
        };
        f.text(
            r.x + 6,
            r.y + 23,
            &format!("Mode: {mode_name}"),
            1,
            ui::BLACK,
        );
        f.text(
            r.x + 6,
            r.y + 36,
            &format!("Opacity: {opacity}%"),
            1,
            ui::BLACK,
        );
        let n = self.doc.layers.len();
        // Keep the active layer inside the visible window.
        let active_row = n - 1 - a;
        if active_row < self.layers_scroll {
            self.layers_scroll = active_row;
        }
        if active_row >= self.layers_scroll + 8 {
            self.layers_scroll = active_row - 7;
        }
        for row in 0..(n - self.layers_scroll).min(8) {
            let li = n - 1 - (self.layers_scroll + row);
            let rr = Self::layer_row_rect(r, row);
            let (visible, linked, name) = {
                let l = &self.doc.layers[li];
                (l.visible, l.linked, l.name.clone())
            };
            let active = li == a;
            if active {
                f.fill_rect(rr, ui::BLACK);
            }
            let c = if active { ui::WHITE } else { ui::BLACK };
            let eye = Rect::new(rr.x + 3, rr.y + 7, 14, 14);
            f.outline_rect(eye, c);
            if visible {
                // An actual eye: almond outline + pupil.
                let (ex, ey) = (eye.x + 7, eye.y + 7);
                for a in 0..32 {
                    let t = a as f32 / 32.0 * std::f32::consts::TAU;
                    f.set(ex + (t.cos() * 5.0) as i32, ey + (t.sin() * 3.0) as i32, c);
                }
                f.fill_rect(Rect::new(ex - 1, ey - 1, 3, 3), c);
            }
            let th = Rect::new(rr.x + 21, rr.y + 2, 36, 24);
            self.draw_layer_thumb(f, th, li);
            f.outline_rect(th, c);
            f.text(rr.x + 61, rr.y + 10, &name, 1, c);
            let lb = Rect::new(rr.right() - 18, rr.y + 8, 12, 12);
            if linked {
                // Two interlocking chain links.
                for (ox, oy) in [(-2, -2), (2, 2)] {
                    for a in 0..24 {
                        let t = a as f32 / 24.0 * std::f32::consts::TAU;
                        f.set(
                            lb.x + 6 + ox + (t.cos() * 3.0) as i32,
                            lb.y + 6 + oy + (t.sin() * 3.0) as i32,
                            c,
                        );
                    }
                }
            } else {
                f.outline_rect(lb, c);
            }
        }
        let labels = ["New", "Dup", "Del"];
        let bw = (r.w - 8 - 42) / 3;
        for (i, label) in labels.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
        // Scroll buttons (enabled only when there's more stack).
        let up = Rect::new(r.right() - 40, r.bottom() - 19, 18, 16);
        let dn = Rect::new(r.right() - 20, r.bottom() - 19, 18, 16);
        Self::draw_small_button(f, up, "^", self.layers_scroll > 0);
        Self::draw_small_button(f, dn, "v", self.layers_scroll + 8 < n);
    }

    /// A layer thumbnail: the layer alone over a checkerboard.
    fn draw_layer_thumb(&self, f: &mut Frame, th: Rect, li: usize) {
        let (dw, dh) = (
            (self.doc.width as i32).max(1),
            (self.doc.height as i32).max(1),
        );
        for by in 0..12i32 {
            for bx in 0..18i32 {
                let dx = ((bx * 2 + 1) * dw / 36).min(dw - 1);
                let dy = ((by * 2 + 1) * dh / 24).min(dh - 1);
                let px = self.doc.render_layer_pixel(li, dx, dy).to_rgba8();
                let checker = if (bx + by) % 2 == 0 { 255 } else { 170 };
                let a = px.3 as u32;
                let blend = |v: u8| ((v as u32 * a + checker * (255 - a)) / 255) as u8;
                f.fill_rect(
                    Rect::new(th.x + bx * 2, th.y + by * 2, 2, 2),
                    (blend(px.0), blend(px.1), blend(px.2)),
                );
            }
        }
    }

    fn layers_panel_click(&mut self, r: Rect, mx: i32, my: i32) -> bool {
        // Header: blend mode cycles; opacity opens Layer Options.
        if Rect::new(r.x + 2, r.y + 20, r.w - 4, 13).contains(mx, my) {
            let cur = self.doc.layers[self.doc.active_layer].blend_mode;
            let idx = pictodesk_core::blend::ALL_BLEND_MODES
                .iter()
                .position(|m| *m == cur)
                .unwrap_or(0);
            let next = (idx + 1) % pictodesk_core::blend::ALL_BLEND_MODES.len();
            self.do_action(Action::BlendMode(next));
            return true;
        }
        if Rect::new(r.x + 2, r.y + 33, r.w - 4, 13).contains(mx, my) {
            self.do_action(Action::LayerOptions);
            return true;
        }
        let n = self.doc.layers.len();
        for row in 0..(n - self.layers_scroll).min(8) {
            let li = n - 1 - (self.layers_scroll + row);
            let rr = Self::layer_row_rect(r, row);
            if !rr.contains(mx, my) {
                continue;
            }
            let eye = Rect::new(rr.x + 3, rr.y + 7, 14, 14);
            if eye.contains(mx, my) {
                self.structural("Toggle Visibility", |d| {
                    d.layers[li].visible = !d.layers[li].visible;
                });
                return true;
            }
            let lb = Rect::new(rr.right() - 18, rr.y + 8, 12, 12);
            if lb.contains(mx, my) {
                self.structural("Toggle Link", |d| {
                    d.layers[li].linked = !d.layers[li].linked;
                });
                return true;
            }
            self.doc.active_layer = li;
            self.channel_view = ChannelView::Composite;
            self.paint_target = PaintTarget::Image;
            self.touch();
            return true;
        }
        if self.layers_scroll > 0
            && Rect::new(r.right() - 40, r.bottom() - 19, 18, 16).contains(mx, my)
        {
            self.layers_scroll -= 1;
            return true;
        }
        if self.layers_scroll + 8 < self.doc.layers.len()
            && Rect::new(r.right() - 20, r.bottom() - 19, 18, 16).contains(mx, my)
        {
            self.layers_scroll += 1;
            return true;
        }
        let bw = (r.w - 8 - 42) / 3;
        for i in 0..3 {
            let br = Rect::new(r.x + 4 + i * bw, r.bottom() - 19, bw - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::NewLayer),
                    1 => self.do_action(Action::DuplicateLayer),
                    _ => self.do_action(Action::DeleteLayer),
                }
                return true;
            }
        }
        true // clicks inside the group are consumed
    }

    fn channel_button(&mut self, i: usize) {
        match i {
            0 => self.do_action(Action::SaveSelection),
            1 => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    self.do_action(Action::LoadSelection(a));
                } else {
                    self.status = "Select an alpha channel to load".into();
                }
            }
            2 => {
                let n = self.doc.alphas.len() + 1;
                let (w, h) = (self.doc.width, self.doc.height);
                self.structural("New Channel", |d| {
                    d.alphas.push(AlphaChannel::new(format!("Alpha {n}"), w, h));
                });
                let idx = self.doc.alphas.len() - 1;
                self.channel_view = ChannelView::Alpha(idx);
                self.paint_target = PaintTarget::Alpha(idx);
                self.status = format!("New channel: Alpha {n}");
            }
            3 => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    if a < self.doc.alphas.len() {
                        let name = self.doc.alphas[a].name.clone();
                        self.structural("Delete Channel", |d| {
                            d.alphas.remove(a);
                        });
                        if self.paint_target == PaintTarget::Alpha(a) {
                            self.paint_target = PaintTarget::Image;
                        }
                        self.channel_view = ChannelView::Composite;
                        self.status = format!("Deleted {name}");
                    }
                }
            }
            4 => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    if a < self.doc.alphas.len() {
                        let new_spot = if self.doc.alphas[a].spot_color.is_some() {
                            None
                        } else {
                            let (r, g, b, _) = self.fg.to_rgba8();
                            Some((r, g, b))
                        };
                        self.structural("Toggle Spot Channel", |d| {
                            if let Some(ch) = d.alphas.get_mut(a) {
                                ch.spot_color = new_spot;
                            }
                        });
                        self.status = "Spot channel toggled".into();
                    }
                }
            }
            _ => {}
        }
    }

    fn target_path_mut(&mut self) -> Option<&mut BezPath> {
        match self.active_path? {
            PathRef::Work => self.doc.work_path.as_mut(),
            PathRef::Saved(i) => self.doc.paths.get_mut(i),
        }
    }

    fn ensure_pen_target(&mut self) {
        let ok = match self.active_path {
            Some(PathRef::Work) => self.doc.work_path.is_some(),
            Some(PathRef::Saved(i)) => i < self.doc.paths.len(),
            None => false,
        };
        if !ok {
            if self.doc.work_path.is_none() {
                self.doc.work_path = Some(BezPath::new(""));
            }
            self.active_path = Some(PathRef::Work);
        }
    }

    fn active_path_clone(&self) -> Option<(PathRef, BezPath)> {
        match self.active_path? {
            PathRef::Work => self.doc.work_path.clone().map(|p| (PathRef::Work, p)),
            PathRef::Saved(i) => self
                .doc
                .paths
                .get(i)
                .cloned()
                .map(|p| (PathRef::Saved(i), p)),
        }
    }

    fn path_button(&mut self, i: usize) {
        // Make Work Path works from a selection, no active path needed.
        if i == 3 {
            if let Some(sel) = &self.sel {
                let mut p = pictodesk_core::path::selection_to_path(sel);
                p.name = String::new();
                self.structural("Make Work Path", |d| {
                    d.work_path = Some(p);
                });
                self.active_path = Some(PathRef::Work);
                self.status = "Made work path from selection".into();
            } else {
                self.status = "Make Path needs a selection".into();
            }
            return;
        }
        let Some((pref, path)) = self.active_path_clone() else {
            self.status = "No path selected".into();
            return;
        };
        let layer = self.doc.active_layer;
        match i {
            0 => {
                // Fill path
                let sel = path.to_selection(self.doc.width, self.doc.height);
                match self.paint_target {
                    PaintTarget::Alpha(a) => {
                        let v = {
                            let (r, g, b, _) = self.fg.to_rgba8();
                            ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
                        };
                        self.structural("Fill Path", |d| {
                            if let Some(ch) = d.alphas.get_mut(a) {
                                for idx in 0..ch.data.len() {
                                    let m = sel.raw_mask()[idx];
                                    if m > 0 {
                                        let cur = ch.data[idx] as u32;
                                        ch.data[idx] =
                                            (cur + (v as u32 - cur) * m as u32 / 255) as u8;
                                    }
                                }
                            }
                        });
                    }
                    PaintTarget::Image => {
                        if let Some((x, y, w, h)) = sel.bounds() {
                            let tiles = Selection::tiles_in_rect(x, y, w, h);
                            let fg = self.fg;
                            run_tile_edit(
                                &mut self.doc,
                                &mut self.hist,
                                "Fill Path",
                                layer,
                                &tiles,
                                |d| brush::fill_selection(d, layer, &sel, fg),
                            );
                            self.last_label = Some("Fill Path".into());
                            self.status = "Fill Path".into();
                            self.touch();
                        }
                    }
                }
            }
            1 => {
                // Stroke path with the current brush
                let spacing = (self.tip.size * 0.25).max(1.0);
                let pts = path.stroke_points(spacing);
                match self.paint_target {
                    PaintTarget::Alpha(a) => {
                        let tip = self.tip;
                        let v = {
                            let (r, g, b, _) = self.fg.to_rgba8();
                            ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
                        };
                        let (w, h) = (self.doc.width, self.doc.height);
                        self.structural("Stroke Path", |d| {
                            if let Some(ch) = d.alphas.get_mut(a) {
                                for pt in &pts {
                                    brush::stamp_channel(
                                        &mut ch.data,
                                        w,
                                        h,
                                        pt.0,
                                        pt.1,
                                        &tip,
                                        v,
                                        1.0,
                                    );
                                }
                            }
                        });
                    }
                    PaintTarget::Image => {
                        if let Some((bx, by, bw, bh)) = path.bounds() {
                            let tiles = Selection::tiles_in_rect(
                                bx as i32 - 32,
                                by as i32 - 32,
                                bw as u32 + 64,
                                bh as u32 + 64,
                            );
                            let tip = self.tip;
                            let fg = self.fg;
                            run_tile_edit(
                                &mut self.doc,
                                &mut self.hist,
                                "Stroke Path",
                                layer,
                                &tiles,
                                |d| {
                                    for pt in &pts {
                                        brush::stamp_brush(
                                            d, layer, None, pt.0, pt.1, fg, &tip, 1.0,
                                        );
                                    }
                                },
                            );
                            self.last_label = Some("Stroke Path".into());
                            self.status = "Stroke Path".into();
                            self.touch();
                        }
                    }
                }
            }
            2 => {
                self.sel = Some(path.to_selection(self.doc.width, self.doc.height));
                self.status = "Path converted to selection".into();
            }
            4 => {
                // Save work path under a name
                if pref == PathRef::Work && self.doc.work_path.is_some() {
                    let mut d = Dialog::new(DialogKind::SavePath, &self.doc);
                    d.text = format!("Path {}", self.doc.paths.len() + 1);
                    d.text_focus = true;
                    self.dialog = Some(d);
                } else {
                    self.status = "Only the work path needs saving".into();
                }
            }
            5 => {
                if let PathRef::Saved(idx) = pref {
                    let new_val = if self.doc.clipping_path == Some(idx) {
                        None
                    } else {
                        Some(idx)
                    };
                    let label = if new_val.is_some() {
                        "Set Clipping Path"
                    } else {
                        "Clear Clipping Path"
                    };
                    self.structural(label, |d| {
                        d.clipping_path = new_val;
                    });
                    self.status = if self.doc.clipping_path == Some(idx) {
                        "Clipping path set (exports with CMYK TIFF)"
                    } else {
                        "Clipping path cleared"
                    }
                    .into();
                } else {
                    self.status = "Save the path before making it a clipping path".into();
                }
            }
            6 => match pref {
                PathRef::Work => {
                    self.structural("Delete Path", |d| {
                        d.work_path = None;
                    });
                    self.active_path = None;
                    self.status = "Work path discarded".into();
                }
                PathRef::Saved(idx) => {
                    if idx < self.doc.paths.len() {
                        self.structural("Delete Path", |d| {
                            d.paths.remove(idx);
                            d.clipping_path = match d.clipping_path {
                                Some(c) if c == idx => None,
                                Some(c) if c > idx => Some(c - 1),
                                other => other,
                            };
                        });
                        self.active_path = None;
                        self.status = "Path deleted".into();
                    }
                }
            },
            _ => {}
        }
    }

    /// Dock mouse routing. Returns true when the click landed in the
    /// dock and was consumed.
    fn panel_mouse(&mut self, mx: i32, my: i32) -> bool {
        if !self.dock_open() || mx < self.win_size.0 as i32 - DOCK_W {
            return false;
        }
        // Color palette cells
        if self.show_palette {
            for i in 0..64 {
                if self.palette_cell_rect(i).contains(mx, my) {
                    let c = ui::PALETTE[i];
                    let p = Pixel16::from_rgba8(c.0, c.1, c.2, 255);
                    if self.alt_down {
                        self.bg = p;
                    } else {
                        self.fg = p;
                    }
                    return true;
                }
            }
        }
        // Brushes cells
        if self.show_brushes {
            let sizes = [1.0f32, 3.0, 5.0, 9.0, 13.0, 21.0, 35.0, 50.0];
            for row in 0..2 {
                for (i, sz) in sizes.iter().enumerate() {
                    if self.brushes_cell(i, row).contains(mx, my) {
                        self.tip = BrushTip::new(*sz, if row == 0 { 1.0 } else { 0.0 });
                        return true;
                    }
                }
            }
        }
        // Channels / Paths group
        if let Some(r) = self.group_rect(PanelKind::Cp) {
            if r.contains(mx, my) {
                for i in 0..3 {
                    let tr = Self::cp_tab_rect(r, i);
                    if tr.contains(mx, my) {
                        self.cp_tab = i;
                        return true;
                    }
                }
                if self.cp_tab == 0 {
                    return self.layers_panel_click(r, mx, my);
                }
                let row = (my - (r.y + 20)) / 15;
                if self.cp_tab == 1 {
                    let rows = self.channel_rows();
                    if row >= 0 && (row as usize) < rows.len() {
                        let view = rows[row as usize].1;
                        self.channel_view = view;
                        match view {
                            ChannelView::Alpha(a) => self.paint_target = PaintTarget::Alpha(a),
                            _ => self.paint_target = PaintTarget::Image,
                        }
                        return true;
                    }
                    let bw = (r.w - 8) / 5;
                    for i in 0..5 {
                        let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
                        if br.contains(mx, my) {
                            self.channel_button(i);
                            return true;
                        }
                    }
                } else {
                    let rows = self.path_rows();
                    if row >= 0 && (row as usize) < rows.len() {
                        self.active_path = Some(rows[row as usize].1);
                        return true;
                    }
                    let bw = (r.w - 8) / 4;
                    for i in 0..4 {
                        let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 39, bw - 2, 16);
                        if br.contains(mx, my) {
                            self.path_button(i);
                            return true;
                        }
                    }
                    let bw2 = (r.w - 8) / 3;
                    for i in 0..3 {
                        let br = Rect::new(r.x + 4 + i as i32 * bw2, r.bottom() - 19, bw2 - 2, 16);
                        if br.contains(mx, my) {
                            self.path_button(i + 4);
                            return true;
                        }
                    }
                }
                return true; // clicks inside the group are consumed
            }
        }
        false
    }

    fn draw_menu_bar(&mut self, f: &mut Frame) {
        let bar = Rect::new(0, 0, f.w as i32, ui::MENU_H);
        f.fill_rect(bar, ui::PLATINUM);
        f.fill_rect(Rect::new(0, ui::MENU_H - 1, f.w as i32, 1), ui::DARK);
        let menus = self.build_menus();
        let titles = self.menu_title_rects();
        for (r, i) in &titles {
            if self.open_menu == Some(*i) {
                f.fill_rect(*r, ui::BLACK);
                f.text(r.x + 7, r.y + 7, menus[*i].title, 1, ui::WHITE);
            } else {
                f.text(r.x + 7, r.y + 7, menus[*i].title, 1, ui::BLACK);
            }
        }
        // document name at right, Mac-style
        let name = self
            .file_path
            .as_ref()
            .map(|p| {
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_else(|| "Untitled".into());
        let label = format!(
            "{}{}  {}x{}",
            name,
            if self.dirty { " *" } else { "" },
            self.doc.width,
            self.doc.height
        );
        let tw = Frame::text_width(&label, 1);
        f.text(f.w as i32 - tw - 10, 7, &label, 1, ui::DARK);
        // dropdown
        if let Some(mi) = self.open_menu {
            let dr = self.dropdown_rect(mi);
            f.fill_rect(dr, ui::PLATINUM_LIGHT);
            f.bevel_raised(dr);
            for (ii, item) in menus[mi].items.iter().enumerate() {
                let ir = Rect::new(dr.x + 2, dr.y + 3 + ii as i32 * 17, dr.w - 4, 17);
                let hovered = self.hover_item == Some((mi, ii));
                if hovered && item.enabled {
                    f.fill_rect(ir, ui::BLACK);
                }
                let c = if !item.enabled {
                    ui::DISABLED
                } else if hovered {
                    ui::WHITE
                } else {
                    ui::BLACK
                };
                if item.checked {
                    // tiny checkmark
                    f.line(ir.x + 3, ir.y + 9, ir.x + 6, ir.y + 12, c);
                    f.line(ir.x + 6, ir.y + 12, ir.x + 12, ir.y + 4, c);
                }
                f.text(ir.x + 16, ir.y + 4, &item.label, 1, c);
                if let Some(sc) = &item.shortcut {
                    let sw = Frame::text_width(sc, 1);
                    f.text(ir.right() - sw - 6, ir.y + 4, sc, 1, c);
                }
                if item.submenu.is_some() {
                    f.text(ir.right() - 16, ir.y + 4, ">", 1, c);
                }
            }
            // submenu
            if let Some(si) = self.open_submenu {
                if let Some(sub) = &menus[mi].items[si].submenu {
                    let parent = Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                    let sw = sub
                        .iter()
                        .map(|it| {
                            Frame::text_width(&it.label, 1)
                                + it.shortcut
                                    .as_deref()
                                    .map(|sc| Frame::text_width(sc, 1) + 12)
                                    .unwrap_or(0)
                                + 40
                        })
                        .max()
                        .unwrap_or(120);
                    let sr = Rect::new(dr.right() - 2, parent.y - 3, sw, sub.len() as i32 * 17 + 6);
                    f.fill_rect(sr, ui::PLATINUM_LIGHT);
                    f.bevel_raised(sr);
                    for (ii, item) in sub.iter().enumerate() {
                        let ir = Rect::new(sr.x + 2, sr.y + 3 + ii as i32 * 17, sr.w - 4, 17);
                        let hovered = self.hover_sub_item == Some(ii);
                        if hovered && item.enabled {
                            f.fill_rect(ir, ui::BLACK);
                        }
                        let c = if !item.enabled {
                            ui::DISABLED
                        } else if hovered {
                            ui::WHITE
                        } else {
                            ui::BLACK
                        };
                        f.text(ir.x + 8, ir.y + 4, &item.label, 1, c);
                        if let Some(sc) = &item.shortcut {
                            let sw = Frame::text_width(sc, 1);
                            f.text(ir.right() - sw - 6, ir.y + 4, sc, 1, c);
                        }
                    }
                }
            }
        }
    }

    fn draw_status(&mut self, f: &mut Frame) {
        let r = Rect::new(0, f.h as i32 - ui::STATUS_H, f.w as i32, ui::STATUS_H);
        f.fill_rect(r, ui::PLATINUM);
        f.fill_rect(Rect::new(0, r.y, f.w as i32, 1), ui::SHADOW);
        let hint = if self.tool == Tool::Stamp && self.clone_src.is_none() {
            "Alt+click to set clone source".to_string()
        } else {
            format!("Tool: {}", self.tool.name())
        };
        let text = format!(
            "{}  |  {}x{}  |  Zoom {:.0}%  |  {}  |  FG {} {} {}",
            self.status,
            self.doc.width,
            self.doc.height,
            self.viewport.scale * 100.0,
            hint,
            self.fg.to_rgba8().0,
            self.fg.to_rgba8().1,
            self.fg.to_rgba8().2
        );
        f.text(8, r.y + 5, &text, 1, ui::BLACK);
    }

    fn draw_file_panel(&mut self, f: &mut Frame) {
        let files = self.file_panel.clone().unwrap_or_default();
        let (ww, wh) = (f.w as i32, f.h as i32);
        let r = Rect::new((ww - 430) / 2, (wh - 320) / 2, 430, 320);
        f.panel(r);
        f.fill_rect(Rect::new(r.x + 2, r.y + 2, r.w - 4, 18), ui::ACCENT);
        f.text(r.x + 8, r.y + 7, "Open", 1, ui::WHITE);
        f.text(r.x + 8, r.y + 26, "Files in current folder:", 1, ui::BLACK);
        if files.is_empty() {
            f.text(r.x + 8, r.y + 48, "(no image files here)", 1, ui::DARK);
        }
        for (i, p) in files.iter().take(14).enumerate() {
            let ir = Rect::new(r.x + 6, r.y + 44 + i as i32 * 17, r.w - 12, 17);
            if self.hover_file == Some(i) {
                f.fill_rect(ir, ui::BLACK);
                f.text(ir.x + 6, ir.y + 4, &p.display().to_string(), 1, ui::WHITE);
            } else {
                f.text(ir.x + 6, ir.y + 4, &p.display().to_string(), 1, ui::BLACK);
            }
        }
        f.button(
            Rect::new(r.right() - 92, r.bottom() - 32, 80, 24),
            "Cancel",
            false,
            true,
        );
    }

    fn file_panel_rect(&self) -> Rect {
        Rect::new(
            (self.win_size.0 as i32 - 430) / 2,
            (self.win_size.1 as i32 - 320) / 2,
            430,
            320,
        )
    }

    fn draw_dialog(&mut self, f: &mut Frame) {
        self.ensure_render_cache();
        let layout = self.dialog_layout().unwrap();
        let d = self.dialog.as_ref().unwrap();
        let r = layout.rect;
        f.panel(r);
        f.fill_rect(Rect::new(r.x + 2, r.y + 2, r.w - 4, 18), ui::ACCENT);
        f.text(r.x + 8, r.y + 7, d.kind.title(), 1, ui::WHITE);
        // histogram strip for Levels / Threshold
        if matches!(d.kind, DialogKind::Levels | DialogKind::Threshold) {
            let strip = Rect::new(r.x + 12, r.y + 26, r.w - 24, 30);
            f.fill_rect(strip, ui::WHITE);
            f.bevel_sunken(strip);
            let mut lum = [0u32; 256];
            for px in self.cached_render.as_chunks::<4>().0 {
                let l =
                    (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as usize;
                lum[l.min(255)] += 1;
            }
            let peak = lum.iter().max().copied().unwrap_or(1).max(1);
            // Quarter gridlines, matching the Curves graph.
            for gi in 1..4 {
                let gx = strip.x + gi * strip.w / 4;
                f.fill_rect(Rect::new(gx, strip.y, 1, strip.h), ui::PLATINUM);
            }
            for i in 0..(strip.w as usize) {
                let bin = lum[i * 256 / strip.w as usize];
                let hgt = ((bin as f32 / peak as f32).sqrt() * strip.h as f32) as i32;
                f.fill_rect(
                    Rect::new(strip.x + i as i32, strip.bottom() - hgt, 1, hgt),
                    ui::BLACK,
                );
            }
        }
        // history log: the document's provenance events
        if d.kind == DialogKind::HistoryLog {
            let boxr = Rect::new(r.x + 12, r.y + 26, r.w - 24, r.h - 26 - 34);
            f.fill_rect(boxr, ui::WHITE);
            f.bevel_sunken(boxr);
            let log = &self.doc.history_log;
            if log.is_empty() {
                f.text(
                    boxr.x + 6,
                    boxr.y + 6,
                    "(no history recorded yet)",
                    1,
                    ui::BLACK,
                );
            } else {
                let rows = ((boxr.h - 10) / 13).max(1) as usize;
                let start = log.len().saturating_sub(rows);
                for (i, ev) in log[start..].iter().enumerate() {
                    let when = pictodesk_core::document::format_history_time(ev.when);
                    f.text(
                        boxr.x + 6,
                        boxr.y + 5 + i as i32 * 13,
                        &format!("{when}  {}", ev.action),
                        1,
                        ui::BLACK,
                    );
                }
            }
        }
        // range / fill toggle buttons
        if d.kind == DialogKind::ColorBalance {
            let names = ["Shadows", "Midtones", "Highlights"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.cb_range == i, true);
            }
        }
        if d.kind == DialogKind::Fill {
            f.button(layout.fill_buttons[0], "Foreground", !d.fill_bg, true);
            f.button(layout.fill_buttons[1], "Background", d.fill_bg, true);
        }
        // text field
        if let Some(tf) = layout.text_field {
            f.fill_rect(tf, ui::WHITE);
            f.bevel_sunken(tf);
            f.text(tf.x + 5, tf.y + 5, &d.text, 1, ui::BLACK);
        }
        // sliders
        let defs = d.slider_defs();
        for (i, row) in layout.slider_rows.iter().enumerate() {
            let (label, min, max) = defs[i];
            let val = if d.kind == DialogKind::ColorBalance {
                d.vals[d.cb_range * 3 + i]
            } else {
                d.vals[i]
            };
            let t = (val - min) / (max - min);
            f.slider(
                row.x,
                row.y,
                row.w,
                label,
                &d.slider_display(if d.kind == DialogKind::ColorBalance {
                    d.cb_range * 3 + i
                } else {
                    i
                }),
                t,
            );
        }
        // curves graph
        if let Some(g) = layout.graph {
            f.fill_rect(g, ui::WHITE);
            f.bevel_sunken(g);
            for i in 1..4 {
                let gx = g.x + i * g.w / 4;
                let gy = g.y + i * g.h / 4;
                f.fill_rect(Rect::new(gx, g.y, 1, g.h), ui::PLATINUM);
                f.fill_rect(Rect::new(g.x, gy, g.w, 1), ui::PLATINUM);
            }
            let pts = &d.curve_points;
            for w in pts.windows(2) {
                let (x0, y0) = (
                    g.x + w[0].0 as i32 * g.w / 255,
                    g.bottom() - w[0].1 as i32 * g.h / 255,
                );
                let (x1, y1) = (
                    g.x + w[1].0 as i32 * g.w / 255,
                    g.bottom() - w[1].1 as i32 * g.h / 255,
                );
                f.line(x0, y0, x1, y1, ui::BLACK);
            }
            for p in pts {
                let (cx, cy) = (
                    g.x + p.0 as i32 * g.w / 255,
                    g.bottom() - p.1 as i32 * g.h / 255,
                );
                f.fill_rect(Rect::new(cx - 3, cy - 3, 7, 7), ui::WHITE);
                f.outline_rect(Rect::new(cx - 3, cy - 3, 7, 7), ui::BLACK);
            }
            f.text(g.right() + 12, g.y + 4, "Click graph to add", 1, ui::DARK);
            f.text(
                g.right() + 12,
                g.y + 16,
                "a point. Drag points",
                1,
                ui::DARK,
            );
            f.text(g.right() + 12, g.y + 28, "to shape the curve.", 1, ui::DARK);
        }
        // canvas anchor grid
        if d.kind == DialogKind::CanvasSize {
            f.text(
                layout.anchor_cells[0].x - 8,
                layout.anchor_cells[0].y - 14,
                "Anchor:",
                1,
                ui::BLACK,
            );
            for (i, cell) in layout.anchor_cells.iter().enumerate() {
                let is = (i as i32 % 3 - 1, i as i32 / 3 - 1) == d.anchor;
                f.fill_rect(*cell, if is { ui::BLACK } else { ui::WHITE });
                f.outline_rect(*cell, ui::DARK);
            }
        }
        f.button(layout.ok, "OK", false, true);
        f.button(layout.cancel, "Cancel", false, true);
    }
}

fn draw_tool_icon(f: &mut Frame, tool: Tool, r: Rect) {
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    let b = ui::BLACK;
    match tool {
        Tool::Move => {
            f.line(cx - 8, cy, cx + 8, cy, b);
            f.line(cx, cy - 8, cx, cy + 8, b);
            // Triangular arrowheads on all four points.
            for dy in -3i32..=3 {
                let inset = dy.abs();
                f.fill_rect(Rect::new(cx - 8 + inset, cy + dy, 4 - inset, 1), b);
                f.fill_rect(Rect::new(cx + 5 - (3 - inset), cy + dy, 4 - inset, 1), b);
            }
            for dx in -3i32..=3 {
                let inset = dx.abs();
                f.fill_rect(Rect::new(cx + dx, cy - 8 + inset, 1, 4 - inset), b);
                f.fill_rect(Rect::new(cx + dx, cy + 5 - (3 - inset), 1, 4 - inset), b);
            }
        }
        Tool::MarqueeRect => {
            for x in (cx - 7)..(cx + 8) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy - 6, b);
                    f.set(x, cy + 6, b);
                }
            }
            for y in (cy - 6)..(cy + 7) {
                if (y / 3) % 2 == 0 {
                    f.set(cx - 7, y, b);
                    f.set(cx + 7, y, b);
                }
            }
        }
        Tool::MarqueeEllipse => {
            for a in 0..48 {
                let t = a as f32 / 48.0 * std::f32::consts::TAU;
                if a % 4 < 2 {
                    f.set(cx + (t.cos() * 7.0) as i32, cy + (t.sin() * 6.0) as i32, b);
                }
            }
        }
        Tool::Lasso => {
            let pts = [
                (cx - 6, cy + 4),
                (cx - 2, cy - 6),
                (cx + 5, cy - 5),
                (cx + 7, cy + 2),
                (cx + 2, cy + 6),
            ];
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, b);
            }
            f.line(pts[4].0, pts[4].1, pts[0].0, pts[0].1, b);
        }
        Tool::Wand => {
            f.line(cx - 6, cy + 6, cx + 2, cy - 2, b);
            // Four-point sparkle at the tip, plus a small satellite.
            f.line(cx + 4, cy - 8, cx + 4, cy - 2, b);
            f.line(cx + 1, cy - 5, cx + 7, cy - 5, b);
            f.line(cx + 2, cy - 7, cx + 6, cy - 3, b);
            f.line(cx + 6, cy - 7, cx + 2, cy - 3, b);
            f.set(cx - 2, cy - 6, b);
            f.set(cx - 3, cy - 5, b);
            f.set(cx - 2, cy - 4, b);
            f.set(cx - 1, cy - 5, b);
        }
        Tool::Eyedropper => {
            // Pipette: rubber bulb upper-right, tube converging to
            // the picking tip at lower-left.
            f.line(cx - 6, cy + 6, cx + 3, cy - 3, b);
            f.line(cx - 4, cy + 7, cx + 5, cy - 2, b);
            f.fill_rect(Rect::new(cx + 3, cy - 7, 5, 5), b);
            f.set(cx - 7, cy + 7, b);
            f.set(cx - 6, cy + 6, b);
        }
        Tool::Hand => {
            // Four fingers with real gaps, a palm, and a thumb.
            for (i, h) in [5i32, 7, 6, 4].iter().enumerate() {
                f.fill_rect(Rect::new(cx - 5 + i as i32 * 3, cy - h, 2, h + 2), b);
            }
            f.fill_rect(Rect::new(cx - 5, cy - 1, 11, 7), b);
            f.fill_rect(Rect::new(cx - 7, cy + 1, 2, 4), b);
        }
        Tool::Zoom => {
            for a in 0..40 {
                let t = a as f32 / 40.0 * std::f32::consts::TAU;
                f.set(
                    cx - 2 + (t.cos() * 5.0) as i32,
                    cy - 2 + (t.sin() * 5.0) as i32,
                    b,
                );
            }
            f.line(cx + 2, cy + 2, cx + 7, cy + 7, b);
            f.line(cx + 3, cy + 2, cx + 7, cy + 6, b);
        }
        Tool::Brush => {
            // An angled paint brush: handle upper-right, ferrule,
            // bristles tapering to the tip, and a paint dab below.
            f.line(cx + 7, cy - 6, cx + 1, cy, b);
            f.line(cx + 6, cy - 7, cx, cy - 1, b);
            f.fill_rect(Rect::new(cx - 2, cy, 3, 3), b);
            for i in 0..4 {
                f.fill_rect(Rect::new(cx - 6 + i, cy + 3 + i, 4 - i, 1), b);
            }
            f.fill_rect(Rect::new(cx - 8, cy + 8, 6, 1), ui::SHADOW);
        }
        Tool::Pen => {
            f.line(cx - 5, cy + 7, cx, cy - 7, b);
            f.line(cx, cy - 7, cx + 5, cy + 7, b);
            f.line(cx - 5, cy + 7, cx + 5, cy + 7, b);
            f.fill_rect(Rect::new(cx - 1, cy, 3, 3), b);
            f.line(cx, cy + 7, cx, cy + 9, b);
        }
        Tool::Pencil => {
            // A single pencil: two body edges, a wood tip with a
            // lead point at lower-left, an eraser cap at upper-right.
            f.line(cx - 3, cy + 3, cx + 4, cy - 4, b);
            f.line(cx - 1, cy + 5, cx + 6, cy - 2, b);
            f.line(cx - 3, cy + 3, cx - 7, cy + 7, b);
            f.line(cx - 1, cy + 5, cx - 7, cy + 7, b);
            f.set(cx - 7, cy + 7, b);
            f.line(cx + 4, cy - 4, cx + 6, cy - 2, b);
            f.fill_rect(Rect::new(cx + 5, cy - 6, 3, 3), b);
        }
        Tool::Eraser => {
            // A slanted block eraser over the surface line it cleans.
            f.line(cx - 5, cy - 1, cx + 2, cy - 6, b);
            f.line(cx + 2, cy - 6, cx + 7, cy - 6, b);
            f.line(cx + 7, cy - 6, cx + 7, cy + 1, b);
            f.line(cx + 7, cy + 1, cx, cy + 6, b);
            f.line(cx, cy + 6, cx - 5, cy + 6, b);
            f.line(cx - 5, cy + 6, cx - 5, cy - 1, b);
            f.line(cx - 5, cy + 2, cx + 7, cy + 2, b);
            f.line(cx - 8, cy + 8, cx + 8, cy + 8, ui::SHADOW);
        }
        Tool::Stamp => {
            // Rubber stamp: round knob, neck, and a wide base.
            for a in 0..32 {
                let t = a as f32 / 32.0 * std::f32::consts::TAU;
                f.set(
                    cx + (t.cos() * 3.0) as i32,
                    cy - 5 + (t.sin() * 3.0) as i32,
                    b,
                );
            }
            f.fill_rect(Rect::new(cx - 1, cy - 2, 3, 3), b);
            f.fill_rect(Rect::new(cx - 3, cy + 1, 7, 2), b);
            f.fill_rect(Rect::new(cx - 6, cy + 3, 13, 3), b);
            f.fill_rect(Rect::new(cx - 6, cy + 7, 13, 1), ui::SHADOW);
        }
        Tool::Bucket => {
            f.line(cx - 6, cy - 4, cx + 3, cy - 4, b);
            f.line(cx - 6, cy - 4, cx - 3, cy + 4, b);
            f.line(cx + 3, cy - 4, cx + 6, cy + 4, b);
            f.line(cx - 3, cy + 4, cx + 6, cy + 4, b);
            f.line(cx - 5, cy - 1, cx + 4, cy - 1, b);
            // The pour: a falling drop at the spout.
            f.fill_rect(Rect::new(cx + 6, cy + 6, 2, 2), b);
            f.set(cx + 6, cy + 8, b);
        }
        Tool::Gradient => {
            for i in 0..14 {
                let v = (255 - i * 17) as u8;
                f.fill_rect(Rect::new(cx - 7 + i, cy - 5, 1, 11), (v, v, v));
            }
            f.outline_rect(Rect::new(cx - 7, cy - 5, 14, 11), b);
        }
        Tool::Blur => {
            // A water droplet: pointed top, round shaded body.
            f.line(cx, cy - 8, cx - 4, cy + 1, b);
            f.line(cx, cy - 8, cx + 4, cy + 1, b);
            for rad in [5, 3] {
                for yy in -rad..=rad {
                    for xx in -rad..=rad {
                        if xx * xx + yy * yy <= rad * rad {
                            f.set(cx + xx, cy + 3 + yy, if rad == 5 { ui::SHADOW } else { b });
                        }
                    }
                }
            }
        }
        Tool::Sharpen => {
            f.line(cx, cy - 7, cx - 6, cy + 6, b);
            f.line(cx, cy - 7, cx + 6, cy + 6, b);
            f.line(cx - 6, cy + 6, cx + 6, cy + 6, b);
        }
        Tool::Smudge => {
            // A fingertip pressing down, with the smear it drags.
            f.fill_rect(Rect::new(cx - 2, cy - 8, 5, 8), b);
            f.fill_rect(Rect::new(cx - 1, cy, 3, 3), b);
            f.line(cx - 7, cy + 5, cx + 1, cy + 5, ui::SHADOW);
            f.line(cx - 4, cy + 7, cx + 4, cy + 7, ui::SHADOW);
        }
    }
}

// ---------------------------------------------------------------- input

impl App {
    fn slider_track(row: Rect) -> Rect {
        Rect::new(row.x + 118, row.y + 3, row.w - 118 - 44, 8)
    }

    fn dialog_slider_value(&self, row: Rect, mx: i32) -> f32 {
        let track = Self::slider_track(row);
        ((mx - track.x) as f32 / (track.w - 8).max(1) as f32).clamp(0.0, 1.0)
    }

    fn set_dialog_slider(&mut self, idx: usize, t: f32) {
        let Some(d) = &mut self.dialog else { return };
        let defs = d.slider_defs();
        let (.., min, max) = defs[idx];
        let eff = if d.kind == DialogKind::ColorBalance {
            d.cb_range * 3 + idx
        } else {
            idx
        };
        let mut v = min + t * (max - min);
        // Step values that are conceptually integers.
        let integral = !matches!(
            (d.kind.clone(), idx),
            (DialogKind::Levels, 2)
                | (DialogKind::GaussianBlur, 0)
                | (DialogKind::Unsharp, 0)
                | (DialogKind::Unsharp, 1)
                | (DialogKind::AddNoise, 0)
        );
        if integral {
            v = v.round();
        }
        if d.vals[eff] != v {
            d.vals[eff] = v;
            self.dialog_refresh_preview();
        }
    }

    pub fn mouse_down(&mut self, x: f32, y: f32) {
        let (mx, my) = (x as i32, y as i32);
        self.mouse_down = true;
        // ---- modal dialog first
        if self.dialog.is_some() {
            let layout = self.dialog_layout().unwrap();
            if layout.ok.contains(mx, my) {
                self.dialog_ok();
                return;
            }
            if layout.cancel.contains(mx, my) {
                self.dialog_cancel();
                return;
            }
            if let Some(tf) = layout.text_field {
                self.dialog.as_mut().unwrap().text_focus = tf.contains(mx, my);
            }
            for (i, b) in layout.range_buttons.iter().enumerate() {
                if b.contains(mx, my) {
                    let d = self.dialog.as_mut().unwrap();
                    if d.kind == DialogKind::ColorBalance {
                        d.cb_range = i;
                    } else if d.kind == DialogKind::Fill {
                        d.fill_bg = i == 1;
                    }
                    return;
                }
            }
            for (i, cell) in layout.anchor_cells.iter().enumerate() {
                if cell.contains(mx, my) {
                    self.dialog.as_mut().unwrap().anchor = (i as i32 % 3 - 1, i as i32 / 3 - 1);
                    return;
                }
            }
            if let Some(g) = layout.graph {
                if g.contains(mx, my) {
                    let d = self.dialog.as_mut().unwrap();
                    let gx = ((mx - g.x) as f32 / g.w as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    let gy =
                        (255.0 - (my - g.y) as f32 / g.h as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    // grab a nearby point, else add one
                    let near = d.curve_points.iter().position(|p| {
                        (p.0 as i32 - gx as i32).abs() < 12 && (p.1 as i32 - gy as i32).abs() < 12
                    });
                    if let Some(i) = near {
                        d.curve_drag = Some(i);
                    } else {
                        d.curve_points.push((gx, gy));
                        d.curve_points.sort();
                        d.curve_drag = d.curve_points.iter().position(|p| *p == (gx, gy));
                    }
                    self.dialog_refresh_preview();
                    return;
                }
            }
            for (i, row) in layout.slider_rows.iter().enumerate() {
                if row.contains(mx, my) {
                    self.dialog.as_mut().unwrap().slider_drag = Some(i);
                    let t = self.dialog_slider_value(*row, mx);
                    self.set_dialog_slider(i, t);
                    return;
                }
            }
            return; // modal: swallow everything else
        }
        // ---- file panel
        if self.file_panel.is_some() {
            let pr = self.file_panel_rect();
            let files = self.file_panel.clone().unwrap();
            let cancel = Rect::new(pr.right() - 92, pr.bottom() - 32, 80, 24);
            if cancel.contains(mx, my) || !pr.contains(mx, my) {
                self.file_panel = None;
                return;
            }
            for (i, p) in files.iter().take(14).enumerate() {
                let ir = Rect::new(pr.x + 6, pr.y + 44 + i as i32 * 17, pr.w - 12, 17);
                if ir.contains(mx, my) {
                    let path = p.clone();
                    self.file_panel = None;
                    self.open_path(&path);
                    return;
                }
            }
            return;
        }
        // ---- dock panels
        if self.panel_mouse(mx, my) {
            return;
        }
        // ---- menu bar / open dropdown
        if my < ui::MENU_H {
            let titles = self.menu_title_rects();
            for (r, i) in titles {
                if r.contains(mx, my) {
                    self.open_menu = if self.open_menu == Some(i) {
                        None
                    } else {
                        Some(i)
                    };
                    self.open_submenu = None;
                    self.drag = DragMode::MenuNav;
                    return;
                }
            }
            self.open_menu = None;
            return;
        }
        if self.open_menu.is_some() {
            if self.menu_click(mx, my) {
                return;
            }
            self.open_menu = None;
            self.open_submenu = None;
            return;
        }
        // ---- toolbox
        if mx < ui::TOOLBAR_W {
            for (i, t) in TOOLS.iter().enumerate() {
                if Self::tool_rect(i).contains(mx, my) {
                    if self.ft.is_some() {
                        self.ft_commit();
                    }
                    self.tool = *t;
                    self.status = format!("Tool: {}", t.name());
                    return;
                }
            }
            if self.swatches_area().contains(mx, my) {
                if self.swatt_default_hit(mx, my) {
                    self.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                    self.bg = Pixel16::from_rgba8(255, 255, 255, 255);
                } else {
                    self.show_palette = !self.show_palette;
                }
                return;
            }
            return;
        }
        // ---- canvas
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        if !canvas.contains(mx, my) {
            return;
        }
        let dp = self.to_doc(x, y);
        self.drag_start_doc = dp;
        self.drag_cur_doc = dp;
        if self.ft.is_some() && self.ft_mouse_down(dp, x, y) {
            return;
        }
        if self.space_down {
            self.drag = DragMode::Pan;
            return;
        }
        match self.tool {
            Tool::Hand => self.drag = DragMode::Pan,
            Tool::Zoom => {
                let local = (x - canvas.x as f32, y - canvas.y as f32);
                self.viewport
                    .zoom_at(local, if self.alt_down { 0.5 } else { 2.0 });
            }
            Tool::Eyedropper => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    if let Some(ch) = self.doc.alphas.get(a) {
                        let v = ch.value(self.doc.width, dp.0.floor() as i32, dp.1.floor() as i32);
                        self.fg = Pixel16::from_rgba8(v, v, v, 255);
                        self.status = format!("Channel value: {v}");
                        return;
                    }
                }
                let p = self
                    .doc
                    .render_pixel(dp.0.floor() as i32, dp.1.floor() as i32);
                if self.alt_down {
                    self.bg = p;
                } else {
                    self.fg = p;
                }
                self.status = format!(
                    "Color: {} {} {}",
                    p.to_rgba8().0,
                    p.to_rgba8().1,
                    p.to_rgba8().2
                );
            }
            Tool::Wand => {
                let contiguous = !self.alt_down;
                self.sel = Some(Selection::wand(
                    &self.doc,
                    dp.0 as i32,
                    dp.1 as i32,
                    32,
                    contiguous,
                ));
                self.status = "Magic Wand selection".into();
            }
            Tool::Pen => {
                self.pen_down(x, y, dp);
            }
            Tool::Bucket => {
                if let PaintTarget::Alpha(a) = self.paint_target {
                    let mut cmd = DocumentCommand::begin("Paint Bucket", &self.doc);
                    let (w, h) = (self.doc.width, self.doc.height);
                    let v = self.fg_luminance();
                    if let Some(ch) = self.doc.alphas.get_mut(a) {
                        brush::flood_channel(&mut ch.data, w, h, dp.0 as i32, dp.1 as i32, v, 32);
                    }
                    cmd.finish(&self.doc);
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some("Paint Bucket".into());
                    self.status = "Paint Bucket (channel)".into();
                    self.touch();
                    return;
                }
                let region = Selection::wand(&self.doc, dp.0 as i32, dp.1 as i32, 32, true);
                if let Some((bx, by, bw, bh)) = region.bounds() {
                    let tiles = Selection::tiles_in_rect(bx, by, bw, bh);
                    let layer = self.doc.active_layer;
                    let fg = self.fg;
                    run_tile_edit(
                        &mut self.doc,
                        &mut self.hist,
                        "Paint Bucket",
                        layer,
                        &tiles,
                        |d| {
                            brush::fill_selection(d, layer, &region, fg);
                        },
                    );
                    self.last_label = Some("Paint Bucket".into());
                    self.status = "Paint Bucket".into();
                    self.touch();
                }
            }
            Tool::MarqueeRect | Tool::MarqueeEllipse => {
                self.drag = DragMode::SelectRect;
            }
            Tool::Lasso => {
                self.drag = DragMode::Lasso;
                self.lasso_points = vec![dp];
            }
            Tool::Gradient => {
                self.drag = DragMode::GradientLine;
            }
            Tool::Move => {
                self.drag = DragMode::MoveLayer;
                self.move_orig_offset = self.doc.layers[self.doc.active_layer].offset;
                let a = self.doc.active_layer;
                self.move_linked = self
                    .doc
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(i, l)| *i != a && l.linked)
                    .map(|(i, l)| (i, l.offset))
                    .collect();
            }
            Tool::Brush
            | Tool::Pencil
            | Tool::Eraser
            | Tool::Stamp
            | Tool::Blur
            | Tool::Sharpen
            | Tool::Smudge => {
                if let PaintTarget::Alpha(_) = self.paint_target {
                    if matches!(self.tool, Tool::Brush | Tool::Pencil | Tool::Eraser) {
                        self.alpha_cmd =
                            Some(DocumentCommand::begin("Paint on channel", &self.doc));
                        self.alpha_last = dp;
                        self.alpha_stamp(dp);
                        self.drag = DragMode::Paint;
                    } else {
                        self.status = "That tool paints on the image, not on a channel".into();
                    }
                    return;
                }
                if self.tool == Tool::Stamp && self.alt_down {
                    self.clone_src = Some(dp);
                    self.status = "Clone source set".into();
                    return;
                }
                if self.tool == Tool::Stamp && self.clone_src.is_none() {
                    self.status = "Alt+click to set clone source first".into();
                    return;
                }
                self.begin_stroke(dp);
                self.drag = DragMode::Paint;
            }
        }
    }

    fn swatches_area(&self) -> Rect {
        Rect::new(
            4,
            self.win_size.1 as i32 - ui::STATUS_H - 56,
            ui::TOOLBAR_W - 6,
            54,
        )
    }

    fn swatt_default_hit(&self, mx: i32, my: i32) -> bool {
        self.swatch_default_rect().contains(mx, my)
    }

    /// Returns true if the click landed in the open menu system.
    fn menu_click(&mut self, mx: i32, my: i32) -> bool {
        let mi = self.open_menu.unwrap();
        let menus = self.build_menus();
        let dr = self.dropdown_rect(mi);
        // submenu items first
        if let Some(si) = self.open_submenu {
            if let Some(sub) = &menus[mi].items[si].submenu {
                let parent = Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                let sw = sub
                    .iter()
                    .map(|it| Frame::text_width(&it.label, 1) + 40)
                    .max()
                    .unwrap_or(120);
                let sr = Rect::new(dr.right() - 2, parent.y - 3, sw, sub.len() as i32 * 17 + 6);
                for (ii, item) in sub.iter().enumerate() {
                    let ir = Rect::new(sr.x + 2, sr.y + 3 + ii as i32 * 17, sr.w - 4, 17);
                    if ir.contains(mx, my) {
                        if item.enabled {
                            let a = item.action.clone();
                            self.open_menu = None;
                            self.open_submenu = None;
                            self.do_action(a);
                        }
                        return true;
                    }
                }
                if sr.contains(mx, my) || parent.contains(mx, my) {
                    return true;
                }
            }
        }
        for (ii, item) in menus[mi].items.iter().enumerate() {
            let ir = Rect::new(dr.x + 2, dr.y + 3 + ii as i32 * 17, dr.w - 4, 17);
            if ir.contains(mx, my) {
                if item.submenu.is_some() {
                    self.open_submenu = Some(ii);
                    return true;
                }
                if item.enabled {
                    let a = item.action.clone();
                    self.open_menu = None;
                    self.open_submenu = None;
                    self.do_action(a);
                }
                return true;
            }
        }
        if dr.contains(mx, my) {
            return true;
        }
        // hovering another menu title switches
        if my < ui::MENU_H {
            for (r, i) in self.menu_title_rects() {
                if r.contains(mx, my) {
                    self.open_menu = Some(i);
                    self.open_submenu = None;
                    return true;
                }
            }
        }
        false
    }

    fn fg_luminance(&self) -> u8 {
        let (r, g, b, _) = self.fg.to_rgba8();
        ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
    }

    /// One brush/pencil/eraser stamp into the targeted alpha channel.
    fn alpha_stamp(&mut self, center: (f32, f32)) {
        let PaintTarget::Alpha(a) = self.paint_target else {
            return;
        };
        let value = if self.tool == Tool::Eraser {
            0
        } else {
            self.fg_luminance()
        };
        let tip = if self.tool == Tool::Pencil {
            BrushTip::new(self.tip.size, 1.0)
        } else {
            self.tip
        };
        let (w, h) = (self.doc.width, self.doc.height);
        if let Some(ch) = self.doc.alphas.get_mut(a) {
            brush::stamp_channel(
                &mut ch.data,
                w,
                h,
                center.0,
                center.1,
                &tip,
                value,
                self.opacity,
            );
        }
        self.touch();
    }

    /// Pen tool click: grab a handle/anchor if near one, close the
    /// current subpath if the first anchor is clicked, otherwise add
    /// an anchor (drag turns it into a smooth point).
    fn pen_down(&mut self, x: f32, y: f32, dp: (f32, f32)) {
        self.begin_path_edit();
        self.ensure_pen_target();
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let to_scr = |p: (f32, f32)| {
            let s = self.viewport.doc_to_screen(p);
            (canvas.x as f32 + s.0, canvas.y as f32 + s.1)
        };
        if let Some((_, path)) = self.active_path_clone() {
            // handles of the current anchor first
            if let Some((si, ai)) = self.pen_anchor {
                if let Some(a) = path.subpaths.get(si).and_then(|sub| sub.anchors.get(ai)) {
                    if !a.is_corner() {
                        for (part, cp) in [(1u8, a.in_ctrl), (2u8, a.out_ctrl)] {
                            let sp = to_scr(cp);
                            if (x - sp.0).abs() < 5.0 && (y - sp.1).abs() < 5.0 {
                                self.pen_hit = Some((si, ai, part));
                                self.drag = DragMode::PenDrag;
                                return;
                            }
                        }
                    }
                }
            }
            // any anchor
            for (si, sub) in path.subpaths.iter().enumerate() {
                for (ai, a) in sub.anchors.iter().enumerate() {
                    let sp = to_scr(a.point);
                    if (x - sp.0).abs() < 5.0 && (y - sp.1).abs() < 5.0 {
                        self.pen_hit = Some((si, ai, 0));
                        self.pen_anchor = Some((si, ai));
                        self.drag = DragMode::PenDrag;
                        return;
                    }
                }
            }
            // close: click the first anchor of the current open subpath
            if let Some(li) = path.subpaths.iter().rposition(|sub| !sub.closed) {
                let sub = &path.subpaths[li];
                if sub.anchors.len() >= 2 {
                    let sp = to_scr(sub.anchors[0].point);
                    if (x - sp.0).abs() < 6.0 && (y - sp.1).abs() < 6.0 {
                        if let Some(p) = self.target_path_mut() {
                            p.subpaths[li].closed = true;
                        }
                        self.commit_path_edit("Close Path");
                        self.status = "Path closed".into();
                        self.touch();
                        return;
                    }
                }
            }
        }
        // add an anchor to the current open subpath (or a fresh one)
        if let Some(p) = self.target_path_mut() {
            let si = match p.subpaths.iter().rposition(|sub| !sub.closed) {
                Some(i) => i,
                None => {
                    p.subpaths.push(Subpath {
                        anchors: Vec::new(),
                        closed: false,
                    });
                    p.subpaths.len() - 1
                }
            };
            p.subpaths[si].anchors.push(Anchor::corner(dp.0, dp.1));
            let ai = p.subpaths[si].anchors.len() - 1;
            self.pen_hit = Some((si, ai, 0));
            self.pen_anchor = Some((si, ai));
        }
        self.drag = DragMode::PenNew;
        self.touch();
    }

    fn pen_move(&mut self, dp: (f32, f32)) {
        let Some((si, ai, part)) = self.pen_hit else {
            return;
        };
        let drag = self.drag;
        if let Some(p) = self.target_path_mut() {
            let Some(a) = p
                .subpaths
                .get_mut(si)
                .and_then(|sub| sub.anchors.get_mut(ai))
            else {
                return;
            };
            match drag {
                DragMode::PenNew => {
                    let pt = a.point;
                    if (dp.0 - pt.0).abs() > 0.5 || (dp.1 - pt.1).abs() > 0.5 {
                        a.out_ctrl = dp;
                        a.in_ctrl = (2.0 * pt.0 - dp.0, 2.0 * pt.1 - dp.1);
                    }
                }
                DragMode::PenDrag => match part {
                    0 => {
                        let d = (dp.0 - a.point.0, dp.1 - a.point.1);
                        a.point = dp;
                        a.in_ctrl = (a.in_ctrl.0 + d.0, a.in_ctrl.1 + d.1);
                        a.out_ctrl = (a.out_ctrl.0 + d.0, a.out_ctrl.1 + d.1);
                    }
                    1 => {
                        a.in_ctrl = dp;
                        if !a.is_corner() {
                            let v = (a.point.0 - dp.0, a.point.1 - dp.1);
                            let len = ((a.out_ctrl.0 - a.point.0).powi(2)
                                + (a.out_ctrl.1 - a.point.1).powi(2))
                            .sqrt();
                            let vl = (v.0.powi(2) + v.1.powi(2)).sqrt().max(0.001);
                            a.out_ctrl = (a.point.0 + v.0 / vl * len, a.point.1 + v.1 / vl * len);
                        }
                    }
                    _ => {
                        a.out_ctrl = dp;
                        if !a.is_corner() {
                            let v = (a.point.0 - dp.0, a.point.1 - dp.1);
                            let len = ((a.in_ctrl.0 - a.point.0).powi(2)
                                + (a.in_ctrl.1 - a.point.1).powi(2))
                            .sqrt();
                            let vl = (v.0.powi(2) + v.1.powi(2)).sqrt().max(0.001);
                            a.in_ctrl = (a.point.0 + v.0 / vl * len, a.point.1 + v.1 / vl * len);
                        }
                    }
                },
                _ => {}
            }
        }
    }

    fn begin_stroke(&mut self, dp: (f32, f32)) {
        let layer = self.doc.active_layer;
        let carry = if self.tool == Tool::Smudge {
            brush::sample_average(&self.doc, layer, dp.0 as i32, dp.1 as i32, 2)
        } else {
            self.fg
        };
        self.stroke = Some(StrokeState {
            cmd: TileEditCommand::new(self.tool.name()),
            start_point: dp,
            last_point: dp,
            smudge_carry: carry,
            tiles_seen: Vec::new(),
        });
        // Snapshot BEFORE the first stamp lands, or undo would keep it.
        self.capture_segment_tiles(dp);
        self.stamp_at(dp);
    }

    fn stamp_at(&mut self, center: (f32, f32)) {
        let layer = self.doc.active_layer;
        let sel = self.sel.clone();
        let tip = self.tip;
        let opacity = self.opacity;
        let tool = self.tool;
        let (fg, clone_src, start) = (
            self.fg,
            self.clone_src,
            self.stroke
                .as_ref()
                .map(|s| s.start_point)
                .unwrap_or(center),
        );
        let bbox = match tool {
            Tool::Brush => brush::stamp_brush(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                fg,
                &tip,
                opacity,
            ),
            Tool::Pencil => brush::stamp_pencil(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                fg,
                tip.size,
                opacity,
            ),
            Tool::Eraser => brush::stamp_eraser(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                opacity,
            ),
            Tool::Stamp => {
                let src = clone_src.unwrap();
                let aligned = (src.0 + (center.0 - start.0), src.1 + (center.1 - start.1));
                brush::stamp_clone(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    aligned,
                    &tip,
                    opacity,
                )
            }
            Tool::Blur => brush::stamp_blur(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                0.5,
            ),
            Tool::Sharpen => brush::stamp_sharpen(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                0.5,
            ),
            Tool::Smudge => {
                let carry = self.stroke.as_ref().unwrap().smudge_carry;
                let b = brush::stamp_smudge(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    carry,
                    &tip,
                    0.6,
                );
                let new_carry =
                    brush::sample_average(&self.doc, layer, center.0 as i32, center.1 as i32, 2);
                if let Some(s) = &mut self.stroke {
                    s.smudge_carry = new_carry;
                }
                b
            }
            _ => return,
        };
        // Register tiles for undo. Segment capture (mouse_move /
        // begin_stroke) snapshots tiles before stamps land; anything
        // first seen here is captured now as a safety net.
        let tiles = Selection::tiles_in_rect(
            bbox.0,
            bbox.1,
            (bbox.2 - bbox.0) as u32,
            (bbox.3 - bbox.1) as u32,
        );
        if let Some(s) = &mut self.stroke {
            for t in tiles {
                if !s.tiles_seen.contains(&t) {
                    s.cmd.capture_before(&self.doc, layer, t);
                    s.tiles_seen.push(t);
                }
            }
        }
        self.doc_version += 1;
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        let (mx, my) = (x as i32, y as i32);
        let prev = self.cursor;
        self.cursor = (x, y);
        if self.ft.is_some() {
            let dp = self.to_doc(x, y);
            self.ft_mouse_move(dp);
            return;
        }
        // dialog drags
        if self.dialog.is_some() {
            let layout = self.dialog_layout().unwrap();
            if let Some(sd) = self.dialog.as_ref().unwrap().slider_drag {
                if let Some(row) = layout.slider_rows.get(sd) {
                    let t = self.dialog_slider_value(*row, mx);
                    self.set_dialog_slider(sd, t);
                }
            }
            if let Some(g) = layout.graph {
                let d = self.dialog.as_mut().unwrap();
                if let Some(ci) = d.curve_drag {
                    let gx = ((mx - g.x) as f32 / g.w as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    let gy =
                        (255.0 - (my - g.y) as f32 / g.h as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    let n = d.curve_points.len();
                    if ci > 0 && ci < n - 1 {
                        d.curve_points[ci] = (gx, gy);
                    } else {
                        d.curve_points[ci].1 = gy; // endpoints keep their x
                    }
                    self.dialog_refresh_preview();
                }
            }
            return;
        }
        // menu hover
        if self.open_menu.is_some() {
            let mi = self.open_menu.unwrap();
            let menus = self.build_menus();
            let dr = self.dropdown_rect(mi);
            self.hover_item = None;
            self.hover_sub_item = None;
            for (ii, item) in menus[mi].items.iter().enumerate() {
                let ir = Rect::new(dr.x + 2, dr.y + 3 + ii as i32 * 17, dr.w - 4, 17);
                if ir.contains(mx, my) {
                    self.hover_item = Some((mi, ii));
                    if item.submenu.is_some() {
                        self.open_submenu = Some(ii);
                    } else if self.open_submenu.is_some() {
                        // keep submenu while hovering its panel
                        let si = self.open_submenu.unwrap();
                        if let Some(sub) = &menus[mi].items[si].submenu {
                            let parent =
                                Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                            let sw = sub
                                .iter()
                                .map(|it| Frame::text_width(&it.label, 1) + 40)
                                .max()
                                .unwrap_or(120);
                            let sr = Rect::new(
                                dr.right() - 2,
                                parent.y - 3,
                                sw,
                                sub.len() as i32 * 17 + 6,
                            );
                            if !sr.contains(mx, my) {
                                self.open_submenu = None;
                            }
                        }
                    }
                }
            }
            if let Some(si) = self.open_submenu {
                if let Some(sub) = &menus[mi].items[si].submenu {
                    let parent = Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                    let sw = sub
                        .iter()
                        .map(|it| {
                            Frame::text_width(&it.label, 1)
                                + it.shortcut
                                    .as_deref()
                                    .map(|sc| Frame::text_width(sc, 1) + 12)
                                    .unwrap_or(0)
                                + 40
                        })
                        .max()
                        .unwrap_or(120);
                    let sr = Rect::new(dr.right() - 2, parent.y - 3, sw, sub.len() as i32 * 17 + 6);
                    for (ii, _) in sub.iter().enumerate() {
                        let ir = Rect::new(sr.x + 2, sr.y + 3 + ii as i32 * 17, sr.w - 4, 17);
                        if ir.contains(mx, my) {
                            self.hover_sub_item = Some(ii);
                            self.hover_item = Some((mi, si));
                        }
                    }
                }
            }
            // slide across menu titles
            if my < ui::MENU_H && self.mouse_down {
                for (r, i) in self.menu_title_rects() {
                    if r.contains(mx, my) && self.open_menu != Some(i) {
                        self.open_menu = Some(i);
                        self.open_submenu = None;
                    }
                }
            }
            return;
        }
        // file panel hover
        if let Some(files) = &self.file_panel {
            let pr = self.file_panel_rect();
            self.hover_file = None;
            for i in 0..files.len().min(14) {
                let ir = Rect::new(pr.x + 6, pr.y + 44 + i as i32 * 17, pr.w - 12, 17);
                if ir.contains(mx, my) {
                    self.hover_file = Some(i);
                }
            }
            return;
        }
        if !self.mouse_down {
            return;
        }
        let dp = self.to_doc(x, y);
        match self.drag {
            DragMode::Pan => {
                self.viewport.pan_by((x - prev.0, y - prev.1));
            }
            DragMode::PenNew | DragMode::PenDrag => {
                self.pen_move(dp);
            }
            DragMode::SelectRect | DragMode::GradientLine => {
                self.drag_cur_doc = dp;
            }
            DragMode::Lasso => {
                self.drag_cur_doc = dp;
                if let Some(last) = self.lasso_points.last() {
                    if (last.0 - dp.0).abs() + (last.1 - dp.1).abs() > 1.5 {
                        self.lasso_points.push(dp);
                    }
                }
            }
            DragMode::MoveLayer => {
                let layer = self.doc.active_layer;
                let dx = (dp.0 - self.drag_start_doc.0).round() as i32;
                let dy = (dp.1 - self.drag_start_doc.1).round() as i32;
                self.doc.layers[layer].offset =
                    (self.move_orig_offset.0 + dx, self.move_orig_offset.1 + dy);
                for (li, orig) in &self.move_linked {
                    self.doc.layers[*li].offset = (orig.0 + dx, orig.1 + dy);
                }
                self.doc_version += 1;
            }
            DragMode::Paint => {
                if self.alpha_cmd.is_some() {
                    let last = self.alpha_last;
                    let spacing = (self.tip.size * 0.25).max(1.0);
                    for c in brush::stroke_centers(&[last, dp], spacing) {
                        if c != last {
                            self.alpha_stamp(c);
                        }
                    }
                    self.alpha_last = dp;
                    return;
                }
                // capture tiles BEFORE stamping this segment
                self.capture_segment_tiles(dp);
                let last = self.stroke.as_ref().unwrap().last_point;
                let spacing = (self.tip.size * 0.25).max(1.0);
                for c in brush::stroke_centers(&[last, dp], spacing) {
                    if c != last {
                        self.stamp_at(c);
                    }
                }
                if let Some(s) = &mut self.stroke {
                    s.last_point = dp;
                }
            }
            _ => {}
        }
    }

    /// Snapshot (before-state) every tile the next stamp segment could
    /// touch, so undo restores the pre-stroke pixels exactly.
    fn capture_segment_tiles(&mut self, to: (f32, f32)) {
        let layer = self.doc.active_layer;
        let r = self.tip.size / 2.0 + 2.0;
        let (x0, y0) = ((to.0 - r).floor() as i32, (to.1 - r).floor() as i32);
        let (x1, y1) = ((to.0 + r).ceil() as i32, (to.1 + r).ceil() as i32);
        let tiles = Selection::tiles_in_rect(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
        // also the segment start area
        let last = self.stroke.as_ref().map(|s| s.last_point).unwrap_or(to);
        let (lx0, ly0) = ((last.0 - r).floor() as i32, (last.1 - r).floor() as i32);
        let (lx1, ly1) = ((last.0 + r).ceil() as i32, (last.1 + r).ceil() as i32);
        let tiles2 = Selection::tiles_in_rect(lx0, ly0, (lx1 - lx0) as u32, (ly1 - ly0) as u32);
        if let Some(s) = &mut self.stroke {
            for t in tiles.into_iter().chain(tiles2) {
                if !s.tiles_seen.contains(&t) {
                    s.cmd.capture_before(&self.doc, layer, t);
                    s.tiles_seen.push(t);
                }
            }
        }
    }

    pub fn mouse_up(&mut self, _x: f32, _y: f32) {
        self.mouse_down = false;
        if let Some(ft) = &mut self.ft {
            ft.drag = FtDrag::None;
        }
        if let Some(d) = &mut self.dialog {
            d.slider_drag = None;
            d.curve_drag = None;
        }
        match self.drag {
            DragMode::PenNew | DragMode::PenDrag => {
                let label = if self.drag == DragMode::PenNew {
                    "Add Anchor"
                } else {
                    "Edit Path"
                };
                self.drag = DragMode::None;
                self.pen_hit = None;
                self.commit_path_edit(label);
            }
            DragMode::Paint => {
                if let Some(mut cmd) = self.alpha_cmd.take() {
                    cmd.finish(&self.doc);
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some("Paint on channel".into());
                    self.status = "Paint on channel".into();
                    self.touch();
                } else if let Some(mut s) = self.stroke.take() {
                    let layer = self.doc.active_layer;
                    for t in &s.tiles_seen {
                        s.cmd.capture_after(&self.doc, layer, *t);
                    }
                    let label = self.tool.name().to_string();
                    self.push_cmd(Box::new(s.cmd));
                    self.last_label = Some(label.clone());
                    self.status = label;
                    self.touch();
                }
            }
            DragMode::SelectRect => {
                let (a, b) = (self.drag_start_doc, self.drag_cur_doc);
                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                if x1 - x0 < 2.0 && y1 - y0 < 2.0 {
                    self.sel = None;
                } else if self.tool == Tool::MarqueeEllipse {
                    self.sel = Some(Selection::ellipse(
                        self.doc.width,
                        self.doc.height,
                        x0 as i32,
                        y0 as i32,
                        (x1 - x0) as u32,
                        (y1 - y0) as u32,
                    ));
                } else {
                    self.sel = Some(Selection::rect(
                        self.doc.width,
                        self.doc.height,
                        x0 as i32,
                        y0 as i32,
                        (x1 - x0) as u32,
                        (y1 - y0) as u32,
                    ));
                }
            }
            DragMode::Lasso => {
                if self.lasso_points.len() >= 3 {
                    self.sel = Some(Selection::polygon(
                        self.doc.width,
                        self.doc.height,
                        &self.lasso_points,
                    ));
                } else {
                    self.sel = None;
                }
                self.lasso_points.clear();
            }
            DragMode::GradientLine => {
                let layer = self.doc.active_layer;
                let tiles = self.sel_tiles();
                let sel = self.sel.clone();
                let (from, to, fg, bg, op) = (
                    self.drag_start_doc,
                    self.drag_cur_doc,
                    self.fg,
                    self.bg,
                    self.opacity,
                );
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Gradient",
                    layer,
                    &tiles,
                    |d| {
                        brush::gradient(d, layer, sel.as_ref(), from, to, fg, bg, op);
                    },
                );
                self.last_label = Some("Gradient".into());
                self.status = "Gradient".into();
                self.touch();
            }
            DragMode::MoveLayer => {
                let layer = self.doc.active_layer;
                let now = self.doc.layers[layer].offset;
                let mut moves = vec![(layer, self.move_orig_offset, now)];
                for (li, orig) in &self.move_linked {
                    moves.push((*li, *orig, self.doc.layers[*li].offset));
                }
                if moves.iter().any(|(_, b, a)| b != a) {
                    self.hist
                        .push(Box::new(MoveLayerCommand::new_multi("Move", moves)));
                    self.last_label = Some("Move".into());
                    self.status = "Move".into();
                    self.touch();
                }
                self.move_linked.clear();
            }
            _ => {}
        }
        self.drag = DragMode::None;
    }

    pub fn scroll(&mut self, delta: f32) {
        if self.cp_tab == 0 {
            if let Some(r) = self.group_rect(PanelKind::Cp) {
                let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
                if r.contains(mx, my) {
                    let n = self.doc.layers.len();
                    if delta > 0.0 {
                        self.layers_scroll = self.layers_scroll.saturating_sub(1);
                    } else if self.layers_scroll + 8 < n {
                        self.layers_scroll += 1;
                    }
                    return;
                }
            }
        }
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
        if canvas.contains(mx, my) {
            let local = (
                self.cursor.0 - canvas.x as f32,
                self.cursor.1 - canvas.y as f32,
            );
            self.viewport
                .zoom_at(local, if delta > 0.0 { 1.1 } else { 0.9 });
        }
    }

    /// The command modifier for shortcuts: Cmd on macOS, Ctrl on
    /// Windows and Linux — each platform's own convention.
    fn primary_down(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.super_down
        } else {
            self.ctrl_down
        }
    }

    pub fn key_char(&mut self, c: char) {
        let primary = self.primary_down();
        // dialog text entry
        if let Some(d) = &mut self.dialog {
            if d.text_focus && !primary {
                d.text.push(c);
            }
            return;
        }
        if primary {
            match c.to_ascii_lowercase() {
                'n' if self.shift_down => self.do_action(Action::NewLayer),
                'e' => self.do_action(Action::MergeDown),
                't' => self.do_action(Action::FreeTransform),
                ']' => self.do_action(Action::BringForward),
                '[' => self.do_action(Action::SendBackward),
                'z' => self.do_action(Action::UndoToggle),
                's' => self.do_action(Action::Save),
                'o' => self.do_action(Action::OpenPanel),
                'n' => self.do_action(Action::New),
                'd' => self.do_action(Action::SelectNone),
                'a' => self.do_action(Action::SelectAll),
                'i' => self.do_action(Action::AdjustDialog("invert")),
                '+' | '=' => self.do_action(Action::ZoomIn),
                '-' => self.do_action(Action::ZoomOut),
                '0' => self.do_action(Action::ZoomFit),
                '1' => self.do_action(Action::Zoom100),
                _ => {}
            }
            return;
        }
        match c {
            'x' | 'X' => std::mem::swap(&mut self.fg, &mut self.bg),
            'd' | 'D' => {
                self.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                self.bg = Pixel16::from_rgba8(255, 255, 255, 255);
            }
            '[' => {
                let s = (self.tip.size / 1.25).max(1.0);
                self.tip = BrushTip::new(s, self.tip.hardness);
            }
            ']' => {
                let s = (self.tip.size * 1.25).min(500.0);
                self.tip = BrushTip::new(s, self.tip.hardness);
            }
            _ => {
                let lc = c.to_ascii_lowercase();
                // cycle within the key's tool group (classic behavior)
                let group: Vec<Tool> = TOOLS.iter().copied().filter(|t| t.key() == lc).collect();
                if !group.is_empty() {
                    if let Some(pos) = group.iter().position(|t| *t == self.tool) {
                        self.tool = group[(pos + 1) % group.len()];
                    } else {
                        self.tool = group[0];
                    }
                    self.status = format!("Tool: {}", self.tool.name());
                }
            }
        }
    }

    pub fn key_special(&mut self, key: SpecialKey) {
        match key {
            SpecialKey::Escape => {
                if self.ft.is_some() {
                    self.ft_cancel();
                } else if self.dialog.is_some() {
                    self.dialog_cancel();
                } else if self.file_panel.is_some() {
                    self.file_panel = None;
                } else if self.open_menu.is_some() {
                    self.open_menu = None;
                    self.open_submenu = None;
                } else if self.show_palette {
                    self.show_palette = false;
                    self.fit_view(self.win_size.0, self.win_size.1);
                } else {
                    self.sel = None;
                }
            }
            SpecialKey::Enter => {
                if self.ft.is_some() {
                    self.ft_commit();
                } else if self.dialog.is_some() {
                    self.dialog_ok();
                } else if self.tool == Tool::Pen {
                    self.status = "Path finished".into();
                }
            }
            SpecialKey::Backspace => {
                let mut popped = false;
                if let Some(d) = &mut self.dialog {
                    if d.text_focus {
                        d.text.pop();
                        popped = true;
                    }
                }
                if !popped && self.dialog.is_none() {
                    if self.tool == Tool::Pen {
                        // delete the last anchor of the current subpath
                        self.begin_path_edit();
                        let mut removed = false;
                        if let Some(pth) = self.target_path_mut() {
                            if let Some(sub) = pth
                                .subpaths
                                .iter_mut()
                                .rev()
                                .find(|sub| !sub.anchors.is_empty())
                            {
                                sub.anchors.pop();
                                removed = true;
                            }
                        }
                        if removed {
                            self.commit_path_edit("Delete Anchor");
                            self.pen_anchor = None;
                            self.status = "Anchor deleted".into();
                            self.touch();
                        }
                    } else if self.sel.is_some() {
                        self.clear_selection();
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum SpecialKey {
    Escape,
    Enter,
    Backspace,
}

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

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: Option<(wgpu::Texture, wgpu::BindGroup, (u32, u32))>,
    format: wgpu::TextureFormat,
}

impl Gpu {
    async fn new_for_surface(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Option<Self> {
        // Prefer a real GPU adapter. Forcing the software fallback
        // adapter leaves some machines (notably macOS) with a surface
        // that can never present — the window opens white and stays
        // white. Only fall back to software if no GPU adapter exists.
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: surface,
                force_fallback_adapter: false,
            })
            .await
        {
            Some(a) => Some(a),
            None => {
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        compatible_surface: surface,
                        force_fallback_adapter: true,
                    })
                    .await
            }
        }?;
        let info = adapter.get_info();
        eprintln!(
            "PictoDesk graphics: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("pictodesk-device"),
                    required_features: wgpu::Features::empty(),
                    // The adapter's own limits, NOT downlevel
                    // defaults: downlevel caps textures at 2048px,
                    // which a Retina window (2560x1600 physical)
                    // exceeds — configuring the surface then panics.
                    required_limits: adapter.limits(),
                },
                None,
            )
            .await
            .ok()?;
        let format = surface
            .and_then(|s| s.get_capabilities(&adapter).formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline"),
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
                    format,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Some(Gpu {
            device,
            queue,
            pipeline,
            layout,
            sampler,
            texture: None,
            format,
        })
    }

    fn upload(&mut self, frame: &Frame) {
        let size = (frame.w, frame.h);
        if self.texture.as_ref().map(|t| t.2) != Some(size) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ui-frame"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ui-bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.texture = Some((tex, bg, size));
        }
        let (tex, _, _) = self.texture.as_ref().unwrap();
        let pitch = size.0 * 4;
        let padded_pitch = pitch.div_ceil(256) * 256;
        if padded_pitch == pitch {
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.px,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(size.1),
                },
                wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
            );
        } else {
            let mut staged = vec![0u8; (padded_pitch * size.1) as usize];
            for y in 0..size.1 as usize {
                staged[y * padded_pitch as usize..y * padded_pitch as usize + pitch as usize]
                    .copy_from_slice(&frame.px[y * pitch as usize..(y + 1) * pitch as usize]);
            }
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &staged,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_pitch),
                    rows_per_image: Some(size.1),
                },
                wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn draw_to(&self, target: &wgpu::TextureView) {
        let (_, bg, _) = self.texture.as_ref().expect("upload before draw");
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }
}

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

fn write_ppm(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&px[0..3]);
    }
    std::fs::write(path, out)
}

/// Headless verification: draw one frame (optionally with a menu open
/// or a dialog up) and render it through the real GPU pipeline.
fn run_shot(args: &[String]) -> i32 {
    let mut shot_path: Option<PathBuf> = None;
    let mut menu: Option<usize> = None;
    let mut dialog_name: Option<String> = None;
    let mut size = (1280u32, 800u32);
    let mut demo2 = false;
    let mut demo3 = false;
    let mut tab: Option<String> = None;
    let mut cmyk = false;
    let mut ft = false;
    let mut many_layers = false;
    let mut view: Option<String> = None;
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
        let kind = match name.as_str() {
            "bc" => DialogKind::BrightnessContrast,
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
            _ => DialogKind::HueSaturation,
        };
        app.open_dialog(kind);
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--shot") {
        std::process::exit(run_shot(&args));
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

struct Runner {
    app: App,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    surface: Option<wgpu::Surface<'static>>,
    frame: Frame,
    last_ants: std::time::Instant,
    /// Display scale factor (2.0 on Retina). The UI is drawn on the
    /// CPU at LOGICAL size and the GPU stretches it over the physical
    /// surface — drawing at physical size made every control half
    /// size on high-DPI displays and quadrupled the per-frame CPU
    /// and upload cost.
    scale: f64,
}

impl Runner {
    fn logical_size(&self, phys_w: u32, phys_h: u32) -> (u32, u32) {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        (
            ((phys_w as f64 / s).round() as u32).max(1),
            ((phys_h as f64 / s).round() as u32).max(1),
        )
    }

    fn redraw(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let logical = self.logical_size(size.width, size.height);
        self.app.win_size = logical;
        if self.frame.w != logical.0 || self.frame.h != logical.1 {
            self.frame.resize(logical.0, logical.1);
        }
        self.app.draw(&mut self.frame);
        if let (Some(gpu), Some(surface)) = (&mut self.gpu, &self.surface) {
            gpu.upload(&self.frame);
            match surface.get_current_texture() {
                Ok(frame) => {
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    gpu.draw_to(&view);
                    frame.present();
                }
                Err(e) => {
                    // Any surface error (Lost, Outdated, Timeout,
                    // OutOfMemory): reconfigure and let the next event
                    // present a fresh frame, instead of swallowing the
                    // error and leaving the window blank forever.
                    eprintln!("PictoDesk surface: {e:?} — reconfiguring");
                    let sz = window.inner_size();
                    if sz.width > 0 && sz.height > 0 {
                        surface.configure(
                            &gpu.device,
                            &wgpu::SurfaceConfiguration {
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                format: gpu.format,
                                width: sz.width.min(gpu.device.limits().max_texture_dimension_2d),
                                height: sz.height.min(gpu.device.limits().max_texture_dimension_2d),
                                present_mode: wgpu::PresentMode::Fifo,
                                desired_maximum_frame_latency: 2,
                                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                                view_formats: vec![],
                            },
                        );
                    }
                }
            }
        }
        if self.app.quit {
            std::process::exit(0);
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("PictoDesk")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0)),
            )
            .expect("create window"),
        );
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).ok();
        let gpu = pollster::block_on(Gpu::new_for_surface(&instance, surface.as_ref()));
        if gpu.is_none() {
            // Never sit on a dead white window: say what happened in
            // the terminal and quit.
            eprintln!(
                "PictoDesk could not initialize graphics on this machine \
                 (no usable GPU adapter or device). Nothing was drawn. \
                 Please report this line along with your OS and GPU."
            );
            el.exit();
            return;
        }
        if let (Some(gpu), Some(surface)) = (&gpu, &surface) {
            let sz = window.inner_size();
            if sz.width > 0 && sz.height > 0 {
                surface.configure(
                    &gpu.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: gpu.format,
                        width: sz.width.min(gpu.device.limits().max_texture_dimension_2d),
                        height: sz.height.min(gpu.device.limits().max_texture_dimension_2d),
                        present_mode: wgpu::PresentMode::Fifo,
                        desired_maximum_frame_latency: 2,
                        alpha_mode: wgpu::CompositeAlphaMode::Auto,
                        view_formats: vec![],
                    },
                );
            }
        }
        self.scale = window.scale_factor();
        self.app.win_size = {
            let s = window.inner_size();
            self.logical_size(s.width, s.height)
        };
        self.app.fit_view(self.app.win_size.0, self.app.win_size.1);
        self.window = Some(window);
        self.gpu = gpu;
        self.surface = surface;
        self.redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                if let (Some(gpu), Some(surface)) = (&self.gpu, &self.surface) {
                    if sz.width > 0 && sz.height > 0 {
                        surface.configure(
                            &gpu.device,
                            &wgpu::SurfaceConfiguration {
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                format: gpu.format,
                                width: sz.width.min(gpu.device.limits().max_texture_dimension_2d),
                                height: sz.height.min(gpu.device.limits().max_texture_dimension_2d),
                                present_mode: wgpu::PresentMode::Fifo,
                                desired_maximum_frame_latency: 2,
                                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                                view_formats: vec![],
                            },
                        );
                    }
                }
                self.app.win_size = self.logical_size(sz.width, sz.height);
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let s = if self.scale > 0.0 { self.scale } else { 1.0 };
                self.app
                    .mouse_move((position.x / s) as f32, (position.y / s) as f32);
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.app.cursor;
                match state {
                    ElementState::Pressed => self.app.mouse_down(x, y),
                    ElementState::Released => self.app.mouse_up(x, y),
                }
                window.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.app.scroll(dy);
                window.request_redraw();
            }
            WindowEvent::ModifiersChanged(mods) => {
                let s: ModifiersState = mods.state();
                self.app.ctrl_down = s.control_key();
                self.app.super_down = s.super_key();
                self.app.alt_down = s.alt_key();
                self.app.shift_down = s.shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed || event.state == ElementState::Released {
                    let pressed = event.state == ElementState::Pressed;
                    match &event.logical_key {
                        Key::Named(NamedKey::Space) => {
                            self.app.space_down = pressed;
                        }
                        Key::Named(NamedKey::Escape) if pressed => {
                            self.app.key_special(SpecialKey::Escape)
                        }
                        Key::Named(NamedKey::Enter) if pressed => {
                            self.app.key_special(SpecialKey::Enter)
                        }
                        Key::Named(NamedKey::Backspace) if pressed => {
                            self.app.key_special(SpecialKey::Backspace)
                        }
                        Key::Character(s) if pressed => {
                            for c in s.chars() {
                                self.app.key_char(c);
                            }
                        }
                        _ => {}
                    }
                    window.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.app.open_path(&path);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        self.app.autosave_tick();
        // Marching ants animation tick.
        if self.last_ants.elapsed().as_millis() > 160 {
            self.last_ants = std::time::Instant::now();
            if self.app.sel.is_some() || self.app.drag == DragMode::SelectRect {
                self.app.ants_phase = self.app.ants_phase.wrapping_add(1);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }
}

fn run_window(path: Option<PathBuf>) {
    let mut app = App::new(demo_document(), None);
    let mtime = |p: &std::path::Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    };
    if let Some(p) = path {
        let rec = format::recovery_path_for(&p);
        if rec.exists() && (!p.exists() || mtime(&rec) > mtime(&p)) {
            match format::load(&rec) {
                Ok(doc) => {
                    app.doc = doc;
                    app.file_path = Some(p);
                    app.dirty = true;
                    app.doc_version += 1;
                    app.recovery_file = Some(rec);
                    app.status = "Recovered autosaved work — Save to keep it".into();
                }
                Err(_) => app.open_path(&p),
            }
        } else {
            app.open_path(&p);
        }
    } else {
        let rec = std::env::temp_dir().join("pictodesk-untitled.pdk.recovery");
        if rec.exists() {
            if let Ok(doc) = format::load(&rec) {
                app.doc = doc;
                app.dirty = true;
                app.doc_version += 1;
                app.recovery_file = Some(rec);
                app.status = "Recovered unsaved work — Save to keep it".into();
            }
        }
    }
    let event_loop = EventLoop::new().expect("event loop");
    let mut runner = Runner {
        app,
        window: None,
        gpu: None,
        surface: None,
        frame: Frame::new(1280, 800),
        last_ants: std::time::Instant::now(),
        scale: 1.0,
    };
    let _ = event_loop.run_app(&mut runner);
}
