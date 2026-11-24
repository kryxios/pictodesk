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

mod typo;
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Move,
    MarqueeRect,
    MarqueeEllipse,
    Lasso,
    PolyLasso,
    MagneticLasso,
    Wand,
    Eyedropper,
    ColorSampler,
    Measure,
    Hand,
    Zoom,
    Brush,
    Pencil,
    HistoryBrush,
    ArtHistoryBrush,
    Eraser,
    MagicEraser,
    BackgroundEraser,
    Stamp,
    PatternStamp,
    Pen,
    FreeformPen,
    MagneticPen,
    Bucket,
    Gradient,
    Blur,
    Sharpen,
    Smudge,
    Dodge,
    Burn,
    Sponge,
    Type,
    VertType,
    TypeMask,
    VertTypeMask,
}

impl Tool {
    fn name(&self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::MarqueeRect => "Rectangular Marquee",
            Tool::MarqueeEllipse => "Elliptical Marquee",
            Tool::Lasso => "Lasso",
            Tool::PolyLasso => "Polygonal Lasso",
            Tool::MagneticLasso => "Magnetic Lasso",
            Tool::Wand => "Magic Wand",
            Tool::Eyedropper => "Eyedropper",
            Tool::ColorSampler => "Color Sampler",
            Tool::Measure => "Measure",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
            Tool::Brush => "Paintbrush",
            Tool::Pencil => "Pencil",
            Tool::HistoryBrush => "History Brush",
            Tool::ArtHistoryBrush => "Art History Brush",
            Tool::Eraser => "Eraser",
            Tool::MagicEraser => "Magic Eraser",
            Tool::BackgroundEraser => "Background Eraser",
            Tool::Stamp => "Clone Stamp",
            Tool::PatternStamp => "Pattern Stamp",
            Tool::Pen => "Pen",
            Tool::FreeformPen => "Freeform Pen",
            Tool::MagneticPen => "Magnetic Pen",
            Tool::Bucket => "Paint Bucket",
            Tool::Type => "Type",
            Tool::VertType => "Vertical Type",
            Tool::TypeMask => "Type Mask",
            Tool::VertTypeMask => "Vertical Type Mask",
            Tool::Gradient => "Gradient",
            Tool::Blur => "Blur",
            Tool::Sharpen => "Sharpen",
            Tool::Smudge => "Smudge",
            Tool::Dodge => "Dodge",
            Tool::Burn => "Burn",
            Tool::Sponge => "Sponge",
        }
    }
    fn key(&self) -> char {
        match self {
            Tool::Move => 'v',
            Tool::MarqueeRect | Tool::MarqueeEllipse => 'm',
            Tool::Lasso | Tool::PolyLasso | Tool::MagneticLasso => 'l',
            Tool::Wand => 'w',
            Tool::Eyedropper | Tool::ColorSampler | Tool::Measure => 'i',
            Tool::Hand => 'h',
            Tool::Zoom => 'z',
            Tool::Brush => 'b',
            Tool::Pencil => 'b',
            Tool::HistoryBrush | Tool::ArtHistoryBrush => 'y',
            Tool::Eraser | Tool::MagicEraser | Tool::BackgroundEraser => 'e',
            Tool::Stamp | Tool::PatternStamp => 's',
            Tool::Pen | Tool::FreeformPen | Tool::MagneticPen => 'p',
            Tool::Type | Tool::VertType | Tool::TypeMask | Tool::VertTypeMask => 't',
            Tool::Bucket => 'k',
            Tool::Gradient => 'g',
            Tool::Blur | Tool::Sharpen | Tool::Smudge => 'r',
            Tool::Dodge | Tool::Burn | Tool::Sponge => 'o',
        }
    }
}

/// Toolbox order, two columns, classic grouping.
const TOOLS: [Tool; 36] = [
    Tool::Move,
    Tool::MarqueeRect,
    Tool::MarqueeEllipse,
    Tool::Lasso,
    Tool::PolyLasso,
    Tool::MagneticLasso,
    Tool::Wand,
    Tool::Pen,
    Tool::FreeformPen,
    Tool::MagneticPen,
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
    /// Free corner drag (Distort); the bool is Perspective
    /// (the neighbouring corner mirrors).
    Quad(usize, bool),
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
    /// Distort mode: the four corners (TL, TR, BR, BL) in doc
    /// coords, once a corner has been dragged freely. While set,
    /// the affine params are ignored.
    quad: Option<[(f32, f32); 4]>,
    start_quad: Option<[(f32, f32); 4]>,
    /// Transform Selection mode: warp the selection mask instead
    /// of any layer; `origin_sel` is the mask being warped.
    sel_mode: bool,
    origin_sel: Option<Selection>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PanelKind {
    Cp,
    Brushes,
    Histogram,
    Color,
    Navigator,
    Actions,
    History,
    Info,
}

/// Which frozen moment the History Brush paints from: a history
/// state (by index) or a snapshot (by index).
#[derive(Clone, Copy, PartialEq, Debug)]
enum HistSource {
    State(usize),
    Snapshot(usize),
}

/// A row in the History panel: a snapshot or a history state
/// (state 0 = as opened).
#[derive(Clone, Copy, PartialEq, Debug)]
enum HistRow {
    Snapshot(usize),
    State(usize),
}

/// A row in the Actions panel: a set header or an action in it.
#[derive(Clone, Copy, PartialEq, Debug)]
enum ActRow {
    Set(usize),
    Action(usize),
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
    FilterTransform3d,
    FilterUnsharp,
    FilterNoise,
    FilterMotion,
    FilterRadial,
    FilterDespeckle,
    FilterMedian,
    FilterEmboss,
    FilterFindEdges,
    FilterSolarize,
    FilterClouds,
    FilterMosaic,
    FilterHighPass,
    FilterOffset,
    FilterCustom,
    AdjustEqualize,
    CopyMerged,
    PasteInto,
    SelectAll,
    SelectNone,
    SelectInverse,
    FeatherDialog,
    SelectGrow,
    SelectSimilar,
    SelectColorRange,
    ModifyExpand,
    ModifyContract,
    ModifySmooth,
    ModifyBorder,
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
    LayerToFront,
    LayerToBack,
    FreeTransform,
    MergeDown,
    MergeVisible,
    FlattenImage,
    NewAdjustment(&'static str),
    EditAdjustment,
    AddMaskReveal,
    AddMaskHide,
    AddMaskSelection,
    DeleteMask,
    ToggleMaskEnabled,
    FeatherMask,
    ToggleNavigator,
    ToggleActionsPanel,
    ToggleHistoryPanel,
    ToggleInfoPanel,
    EffectsDialog(usize),
    CopyEffects,
    PasteEffects,
    PasteEffectsLinked,
    ClearEffects,
    CreateEffectLayers,
    GlobalAngleDialog,
    ToggleEffectsVisible,
    DefinePattern,
    Reselect,
    TransformSelection,
    TransformAgain,
    FtNumericDialog,
    AlignLinked(u8),
    DistributeLinked(u8),
    ColorSettingsDialog,
    AssignProfile(usize),
    ConvertProfile(usize),
    ToggleProof,
    ChannelMixerDialog,
    FitImageDialog,
    CondModeDialog,
    ContactSheetDialog,
    PicturePackageDialog,
    WebGalleryDialog,
    PageSetupDialog,
    PrintDialog,
    SaveForWebDialog,
    ExportPsd,
    ExportDcs,
    LegacyFilterDialog(u8),
    FilterDiffuse(u8),
    FilterMezzotint(u8),
    FilterPolar(bool),
    /// 0 Difference Clouds, 1 Sharpen Edges, 2 Fragment, 3 Facet.
    FilterSpecial(u8),
    ExtractDialog,
    Undo,
    Redo,
    HistSnapshot,
    HistSnapshotMerged,
    HistSnapshotLayer,
    HistNewDoc,
    HistOptions,
    HistTrash,
    ToggleGuides,
    ToggleGrid,
    ToggleSnap,
    ClearGuides,
    MacroRecord,
    MacroStop,
    MacroPlay,
    MacroNew,
    MacroDelete,
    MacroBatch,
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
    PhotoFilter,
    FeatherMask,
    Batch,
    MotionBlur,
    RadialBlur,
    MedianFilter,
    Emboss,
    Mosaic,
    HighPass,
    OffsetFilter,
    CustomFilter,
    ColorRange,
    ModifySel,
    HistoryOptions,
    ChannelMixer,
    Type,
    Effects,
    Extract,
    Transform3d,
    FitImage,
    CondMode,
    ContactSheet,
    PicturePackage,
    WebGallery,
    PageSetup,
    Print,
    SaveForWeb,
    ColorSettings,
    GlobalAngle,
    /// The Phase 5 legacy filter gallery: one dialog kind whose
    /// payload selects the filter (see `fx_title`).
    LegacyFilter(u8),
    ActionSetName,
    FtNumeric,
}

/// Names of the LegacyFilter gallery, by id.
fn fx_title(id: u8) -> &'static str {
    match id {
        0 => "Pinch",
        1 => "Spherize",
        2 => "Twirl",
        3 => "Ripple",
        4 => "Ocean Ripple",
        5 => "Shear",
        6 => "ZigZag",
        7 => "Maximum",
        8 => "Minimum",
        9 => "Dust & Scratches",
        10 => "Tiles",
        11 => "Glowing Edges",
        12 => "Patchwork",
        13 => "Stamp",
        14 => "Bas Relief",
        15 => "Wind",
        _ => "Filter",
    }
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
            DialogKind::PhotoFilter => "Photo Filter",
            DialogKind::FeatherMask => "Feather Mask",
            DialogKind::Batch => "Batch Process",
            DialogKind::MotionBlur => "Motion Blur",
            DialogKind::RadialBlur => "Radial Blur",
            DialogKind::MedianFilter => "Median",
            DialogKind::Emboss => "Emboss",
            DialogKind::Mosaic => "Mosaic",
            DialogKind::HighPass => "High Pass",
            DialogKind::OffsetFilter => "Offset",
            DialogKind::CustomFilter => "Custom Filter",
            DialogKind::ColorRange => "Color Range",
            DialogKind::ModifySel => "Modify Selection",
            DialogKind::HistoryOptions => "History Options",
            DialogKind::ChannelMixer => "Channel Mixer",
            DialogKind::Type => "Type",
            DialogKind::Effects => "Layer Effects",
            DialogKind::Extract => "Extract",
            DialogKind::Transform3d => "3D Transform",
            DialogKind::FitImage => "Fit Image",
            DialogKind::CondMode => "Conditional Mode Change",
            DialogKind::ContactSheet => "Contact Sheet II",
            DialogKind::PicturePackage => "Picture Package",
            DialogKind::WebGallery => "Web Photo Gallery",
            DialogKind::PageSetup => "Page Setup",
            DialogKind::Print => "Print",
            DialogKind::SaveForWeb => "Save for Web",
            DialogKind::ColorSettings => "Color Settings",
            DialogKind::GlobalAngle => "Global Angle",
            DialogKind::LegacyFilter(id) => fx_title(*id),
            DialogKind::ActionSetName => "New Action Set",
            DialogKind::FtNumeric => "Free Transform",
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
                | DialogKind::PhotoFilter
                | DialogKind::MotionBlur
                | DialogKind::RadialBlur
                | DialogKind::MedianFilter
                | DialogKind::Emboss
                | DialogKind::Mosaic
                | DialogKind::HighPass
                | DialogKind::OffsetFilter
                | DialogKind::CustomFilter
                | DialogKind::ChannelMixer
                | DialogKind::Transform3d
                | DialogKind::LegacyFilter(_)
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
    /// Photo Filter color (preset swatches set this).
    photo_color: (u8, u8, u8),
    /// When set, this dialog edits an adjustment layer's parameters
    /// live instead of previewing a destructive edit (Phase 4).
    adj_layer: Option<usize>,
    /// Document state before an adjustment-layer dialog opened, so
    /// Cancel can restore it and OK can record one undo step.
    adj_snapshot: Option<Document>,
    /// Modify Selection operation: 0 Expand, 1 Contract, 2 Smooth,
    /// 3 Border.
    modify_op: u8,
    /// Custom Filter 5x5 kernel (row-major), the cell being edited,
    /// and its scale / offset.
    kernel: [i32; 25],
    kernel_sel: usize,
    kernel_scale: i32,
    kernel_offset: i32,
    /// Generic toggle buttons (History Options, Color Settings...).
    opt_a: bool,
    opt_b: bool,
    /// Hue/Saturation edit range: 0 Master, 1 Reds, 2 Yellows,
    /// 3 Greens, 4 Cyans, 5 Blues, 6 Magentas.
    hs_range: usize,
    /// Fill contents: paint with the history source document.
    fill_history: bool,
    /// Channel Mixer working copy + the output channel being edited.
    mixer: adjust::ChannelMixer,
    mix_out: usize,
    /// Layer Effects working copy + the selected effect page
    /// (0 shadow, 1 inner shadow, 2 outer glow, 3 inner glow, 4 bevel).
    fx: pictodesk_core::effects::Effects,
    fx_page: usize,
    /// Type dialog: the layer being edited (None = new layer) and
    /// the click point where a new layer's text starts.
    type_layer: Option<usize>,
    type_anchor: (i32, i32),
    /// Type dialog working copy of the spec + font list scroll.
    type_spec: Option<pictodesk_core::text::TextSpec>,
    /// Type dialog opened by a Type Mask tool: OK produces a
    /// selection instead of a layer.
    type_mask: bool,
    font_scroll: usize,
    /// Extract dialog masks (document-sized) + tool mode
    /// (0 highlight brush, 1 fill, 2 eraser) + preview flag.
    extract_hi: Vec<u8>,
    extract_fill: Vec<u8>,
    extract_mode: u8,
    extract_preview: bool,
    /// 3D Transform shape: 0 cube, 1 sphere, 2 cylinder.
    shape3d: u8,
    /// 3D Transform wireframe drag state: the last mouse
    /// position while dragging the wireframe to rotate it.
    wire_drag: Option<(i32, i32)>,
    /// Automate dialog choices.
    cond_from: u8,
    cond_to: u8,
    pkg_layout: u8,
    /// Save for Web format: 0 JPEG, 1 PNG, 2 GIF, 3 WebP; and the
    /// last encoded size in bytes (shown in the dialog).
    web_format: u8,
    web_size: usize,
    /// Print profile choice: 0 = document working space, 1 = let
    /// the system driver manage color.
    print_profile: u8,
    /// LegacyFilter option state: ZigZag style / Wind method,
    /// and the Wind direction (true = from the left).
    fx_a: u8,
    fx_b: bool,
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
            DialogKind::PhotoFilter => vec![25.0],
            DialogKind::FeatherMask => vec![5.0],
            DialogKind::Batch => vec![],
            DialogKind::MotionBlur => vec![0.0, 10.0],
            DialogKind::RadialBlur => vec![20.0],
            DialogKind::MedianFilter => vec![1.0],
            DialogKind::Emboss => vec![100.0],
            DialogKind::Mosaic => vec![8.0],
            DialogKind::HighPass => vec![10.0],
            DialogKind::OffsetFilter => vec![0.0, 0.0],
            DialogKind::CustomFilter => vec![],
            DialogKind::ColorRange => vec![40.0],
            DialogKind::ModifySel => vec![1.0],
            DialogKind::HistoryOptions => vec![20.0],
            DialogKind::ChannelMixer => vec![100.0, 0.0, 0.0, 0.0, 0.0],
            DialogKind::Type => vec![36.0, 0.0, 0.0, 0.0, 0.0],
            DialogKind::Effects => vec![],
            DialogKind::Extract => vec![],
            DialogKind::Transform3d => vec![0.0, 30.0],
            DialogKind::FitImage => vec![doc.width as f32, doc.height as f32],
            DialogKind::CondMode => vec![],
            DialogKind::ContactSheet => vec![4.0, 160.0],
            DialogKind::PicturePackage => vec![612.0, 792.0],
            DialogKind::WebGallery => vec![120.0],
            DialogKind::PageSetup => vec![612.0, 792.0, 100.0],
            DialogKind::Print => vec![1.0],
            DialogKind::SaveForWeb => vec![80.0],
            DialogKind::ColorSettings => vec![],
            DialogKind::GlobalAngle => vec![120.0],
            DialogKind::ActionSetName => vec![],
            DialogKind::FtNumeric => vec![100.0, 100.0, 0.0, 0.0, 0.0, 0.0],
            DialogKind::LegacyFilter(id) => match id {
                0 | 1 => vec![50.0],
                2 => vec![50.0],
                3 => vec![100.0, 20.0],
                4 => vec![10.0, 10.0],
                5 => vec![0.0, 0.0, 0.0, 0.0, 0.0],
                6 => vec![20.0, 5.0],
                7 | 8 => vec![1.0],
                9 => vec![1.0, 10.0],
                10 => vec![10.0, 10.0],
                11 => vec![2.0, 20.0, 5.0],
                12 => vec![4.0, 10.0],
                13 => vec![25.0, 5.0],
                14 => vec![10.0, 3.0],
                _ => vec![],
            },
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
            photo_color: (255, 178, 84),
            adj_layer: None,
            adj_snapshot: None,
            modify_op: 0,
            kernel: {
                let mut k = [0i32; 25];
                k[12] = 1; // identity until the user says otherwise
                k
            },
            kernel_sel: 12,
            kernel_scale: 1,
            kernel_offset: 0,
            opt_a: false,
            opt_b: false,
            hs_range: 0,
            fill_history: false,
            mixer: adjust::ChannelMixer::default(),
            mix_out: 0,
            fx: pictodesk_core::effects::Effects::default(),
            fx_page: 0,
            type_layer: None,
            type_anchor: (0, 0),
            type_spec: None,
            type_mask: false,
            font_scroll: 0,
            extract_hi: Vec::new(),
            extract_fill: Vec::new(),
            extract_mode: 0,
            extract_preview: false,
            shape3d: 1,
            wire_drag: None,
            cond_from: 0,
            cond_to: 0,
            pkg_layout: 0,
            web_format: 0,
            web_size: 0,
            print_profile: 0,
            fx_a: 0,
            fx_b: true,
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
            DialogKind::PhotoFilter => vec![("Density", 0.0, 100.0)],
            DialogKind::FeatherMask => vec![("Radius", 0.0, 250.0)],
            DialogKind::Curves
            | DialogKind::SaveAs
            | DialogKind::SavePath
            | DialogKind::HistoryLog
            | DialogKind::Batch
            | DialogKind::NewLayer => vec![],
            DialogKind::LayerOptions => vec![("Opacity", 0.0, 100.0)],
            DialogKind::GrowSel | DialogKind::SimilarSel => vec![("Tolerance", 0.0, 255.0)],
            DialogKind::MotionBlur => {
                vec![("Angle", 0.0, 360.0), ("Distance", 1.0, 999.0)]
            }
            DialogKind::RadialBlur => vec![("Amount", 1.0, 100.0)],
            DialogKind::MedianFilter => vec![("Radius", 1.0, 10.0)],
            DialogKind::Emboss => vec![("Amount", 0.0, 200.0)],
            DialogKind::Mosaic => vec![("Cell Size", 2.0, 100.0)],
            DialogKind::HighPass => vec![("Radius", 0.1, 250.0)],
            DialogKind::OffsetFilter => {
                vec![("Horizontal", -999.0, 999.0), ("Vertical", -999.0, 999.0)]
            }
            DialogKind::CustomFilter => vec![],
            DialogKind::HistoryOptions => vec![("Maximum states", 1.0, 100.0)],
            DialogKind::ChannelMixer => vec![
                ("Red / C source", -200.0, 200.0),
                ("Green / M source", -200.0, 200.0),
                ("Blue / Y source", -200.0, 200.0),
                ("K source (CMYK)", -200.0, 200.0),
                ("Constant", -100.0, 100.0),
            ],
            DialogKind::Type => vec![
                ("Size", 6.0, 288.0),
                ("Tracking", -100.0, 1000.0),
                ("Leading (0=auto)", 0.0, 500.0),
                ("From char", 0.0, 10000.0),
                ("To char", 0.0, 10000.0),
            ],
            DialogKind::Effects => match self.fx_page {
                0 | 1 => vec![
                    ("Opacity", 0.0, 100.0),
                    ("Angle", -180.0, 180.0),
                    ("Distance", 0.0, 100.0),
                    ("Blur", 0.0, 100.0),
                    ("Intensity", 0.0, 100.0),
                ],
                2 | 3 => vec![
                    ("Opacity", 0.0, 100.0),
                    ("Blur", 0.0, 100.0),
                    ("Intensity", 0.0, 100.0),
                ],
                _ => vec![
                    ("Depth", 0.0, 1000.0),
                    ("Angle", -180.0, 180.0),
                    ("Highlight opacity", 0.0, 100.0),
                    ("Shadow opacity", 0.0, 100.0),
                ],
            },
            DialogKind::Extract => vec![],
            DialogKind::Transform3d => {
                vec![("Rotate X", -180.0, 180.0), ("Rotate Y", -180.0, 180.0)]
            }
            DialogKind::FitImage => vec![("Width", 1.0, 10000.0), ("Height", 1.0, 10000.0)],
            DialogKind::CondMode => vec![],
            DialogKind::ContactSheet => {
                vec![("Columns", 1.0, 10.0), ("Thumbnail size", 32.0, 512.0)]
            }
            DialogKind::PicturePackage => {
                vec![
                    ("Page width", 100.0, 2000.0),
                    ("Page height", 100.0, 2000.0),
                ]
            }
            DialogKind::WebGallery => vec![("Thumbnail size", 64.0, 320.0)],
            DialogKind::PageSetup => vec![
                ("Paper width (pt)", 100.0, 2000.0),
                ("Paper height (pt)", 100.0, 2000.0),
                ("Scale %", 10.0, 400.0),
            ],
            DialogKind::Print => vec![("Copies", 1.0, 99.0)],
            DialogKind::SaveForWeb => vec![("Quality", 1.0, 100.0)],
            DialogKind::ColorSettings => vec![],
            DialogKind::GlobalAngle => vec![("Angle", -180.0, 180.0)],
            DialogKind::ActionSetName => vec![],
            DialogKind::FtNumeric => vec![
                ("Scale X %", 5.0, 400.0),
                ("Scale Y %", 5.0, 400.0),
                ("Angle", -180.0, 180.0),
                ("Skew", -80.0, 80.0),
                ("Move X", -2000.0, 2000.0),
                ("Move Y", -2000.0, 2000.0),
            ],
            DialogKind::LegacyFilter(id) => match id {
                0 => vec![("Amount", -100.0, 100.0)],
                1 => vec![("Amount", -100.0, 100.0)],
                2 => vec![("Angle", -999.0, 999.0)],
                3 => vec![("Amount", -999.0, 999.0), ("Size", 1.0, 100.0)],
                4 => vec![("Size", 1.0, 100.0), ("Magnitude", 0.0, 20.0)],
                5 => vec![
                    ("Top", -64.0, 64.0),
                    ("Upper", -64.0, 64.0),
                    ("Middle", -64.0, 64.0),
                    ("Lower", -64.0, 64.0),
                    ("Bottom", -64.0, 64.0),
                ],
                6 => vec![("Amount", -100.0, 100.0), ("Ridges", 1.0, 20.0)],
                7 | 8 => vec![("Radius", 1.0, 16.0)],
                9 => vec![("Radius", 0.0, 10.0), ("Threshold", 0.0, 255.0)],
                10 => vec![("Tiles", 1.0, 99.0), ("Max offset %", 0.0, 99.0)],
                11 => vec![
                    ("Edge width", 1.0, 14.0),
                    ("Brightness", 0.0, 50.0),
                    ("Smoothness", 1.0, 15.0),
                ],
                12 => vec![("Size", 1.0, 10.0), ("Relief", 0.0, 30.0)],
                13 => vec![("Light/Dark balance", 0.0, 50.0), ("Smoothness", 1.0, 50.0)],
                14 => vec![("Detail", 0.0, 20.0), ("Smoothness", 0.0, 10.0)],
                _ => vec![],
            },
            DialogKind::ColorRange => vec![("Fuzziness", 0.0, 200.0)],
            DialogKind::ModifySel => {
                let label = match self.modify_op {
                    0 => "Expand By",
                    1 => "Contract By",
                    2 => "Smooth By",
                    _ => "Border Width",
                };
                vec![(label, 1.0, 200.0)]
            }
        }
    }

    fn slider_display(&self, i: usize) -> String {
        let v = self.vals[i];
        match self.kind {
            DialogKind::Levels if i == 2 => format!("{v:.2}"),
            DialogKind::GaussianBlur => format!("{v:.1}"),
            DialogKind::HighPass => format!("{v:.1}"),
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
    NavDrag,
    GuideDrag,
    MeasureDrag,
    MagLasso,
    FreePen,
    MagPen,
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
    /// A layer mask, shown as grayscale (Phase 4).
    Mask(usize),
}

/// Where painting lands: the image itself, one alpha channel, or a
/// layer mask (Phase 4).
#[derive(Clone, Copy, PartialEq, Debug)]
enum PaintTarget {
    Image,
    Alpha(usize),
    Mask(usize),
}

/// Photo Filter preset colors (Phase 4): name + 8-bit RGB.
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

/// One replayable step of a recorded Action (Phase 4). Steps carry
/// their full parameters so replay needs no dialogs.
#[derive(Clone, Debug)]
enum RecordedStep {
    Adjustment(pictodesk_core::adjust::Adjustment),
    AdjustmentLayer(pictodesk_core::adjust::Adjustment),
    Posterize(u8),
    Threshold(u8),
    Invert,
    Desaturate,
    GaussianBlur(f32),
    Unsharp(f32, f32, u8),
    AddNoise(f32),
    ImageSize(u32, u32),
    CanvasSize(u32, u32, (i32, i32)),
    Rotate180,
    Rotate90Cw,
    Rotate90Ccw,
    FlipH,
    FlipV,
    Flatten,
    CropToSelection,
    Equalize,
    Sharpen,
    MotionBlur(f32, f32),
    RadialBlur(f32),
    Median(u32),
    Despeckle,
    Emboss(f32),
    FindEdges,
    Solarize,
    Clouds(Pixel16, Pixel16),
    Mosaic(u32),
    HighPass(f32),
    OffsetPx(i32, i32),
    CustomKernel([i32; 25], i32, i32),
    // Phase 5 additions: the legacy filter gallery, Channel
    // Mixer, effects, and recordable panel (layer) operations.
    Pinch(f32),
    Spherize(f32),
    Twirl(f32),
    Ripple(f32, f32),
    OceanRipple(f32, f32),
    Polar(bool),
    Shear([f32; 5]),
    ZigZag(f32, u32, u8),
    Maximum(u32),
    Minimum(u32),
    DustScratches(u32, f32),
    Tiles(u32, f32),
    GlowingEdges(u32, f32, u32),
    Patchwork(u32, u32),
    StampEffect(f32, u32),
    BasRelief(Pixel16, Pixel16, u32, u32),
    Wind(u8, bool),
    Diffuse(u8),
    Mezzotint(u8),
    DifferenceClouds,
    SharpenEdges,
    Fragment,
    Facet,
    Transform3d(u8, f32, f32),
    ChannelMixer(adjust::ChannelMixer),
    SetEffects(pictodesk_core::effects::Effects),
    LayerNew,
    LayerDelete,
    LayerDuplicate,
    LayerSelect(usize),
}

impl RecordedStep {
    fn label(&self) -> String {
        match self {
            RecordedStep::Adjustment(a) => a.name().to_string(),
            RecordedStep::AdjustmentLayer(a) => format!("{} Layer", a.name()),
            RecordedStep::Posterize(_) => "Posterize".into(),
            RecordedStep::Threshold(_) => "Threshold".into(),
            RecordedStep::Invert => "Invert".into(),
            RecordedStep::Desaturate => "Desaturate".into(),
            RecordedStep::GaussianBlur(_) => "Gaussian Blur".into(),
            RecordedStep::Unsharp(..) => "Unsharp Mask".into(),
            RecordedStep::AddNoise(_) => "Add Noise".into(),
            RecordedStep::ImageSize(..) => "Image Size".into(),
            RecordedStep::CanvasSize(..) => "Canvas Size".into(),
            RecordedStep::Rotate180 => "Rotate 180".into(),
            RecordedStep::Rotate90Cw => "Rotate 90 CW".into(),
            RecordedStep::Rotate90Ccw => "Rotate 90 CCW".into(),
            RecordedStep::FlipH => "Flip Horizontal".into(),
            RecordedStep::FlipV => "Flip Vertical".into(),
            RecordedStep::Flatten => "Flatten Image".into(),
            RecordedStep::CropToSelection => "Crop".into(),
            RecordedStep::Equalize => "Equalize".into(),
            RecordedStep::Sharpen => "Sharpen".into(),
            RecordedStep::MotionBlur(..) => "Motion Blur".into(),
            RecordedStep::RadialBlur(_) => "Radial Blur".into(),
            RecordedStep::Median(_) => "Median".into(),
            RecordedStep::Despeckle => "Despeckle".into(),
            RecordedStep::Emboss(_) => "Emboss".into(),
            RecordedStep::FindEdges => "Find Edges".into(),
            RecordedStep::Solarize => "Solarize".into(),
            RecordedStep::Clouds(..) => "Clouds".into(),
            RecordedStep::Mosaic(_) => "Mosaic".into(),
            RecordedStep::HighPass(_) => "High Pass".into(),
            RecordedStep::OffsetPx(..) => "Offset".into(),
            RecordedStep::CustomKernel(..) => "Custom Filter".into(),
            RecordedStep::Pinch(_) => "Pinch".into(),
            RecordedStep::Spherize(_) => "Spherize".into(),
            RecordedStep::Twirl(_) => "Twirl".into(),
            RecordedStep::Ripple(..) => "Ripple".into(),
            RecordedStep::OceanRipple(..) => "Ocean Ripple".into(),
            RecordedStep::Polar(_) => "Polar Coordinates".into(),
            RecordedStep::Shear(_) => "Shear".into(),
            RecordedStep::ZigZag(..) => "ZigZag".into(),
            RecordedStep::Maximum(_) => "Maximum".into(),
            RecordedStep::Minimum(_) => "Minimum".into(),
            RecordedStep::DustScratches(..) => "Dust & Scratches".into(),
            RecordedStep::Tiles(..) => "Tiles".into(),
            RecordedStep::GlowingEdges(..) => "Glowing Edges".into(),
            RecordedStep::Patchwork(..) => "Patchwork".into(),
            RecordedStep::StampEffect(..) => "Stamp".into(),
            RecordedStep::BasRelief(..) => "Bas Relief".into(),
            RecordedStep::Wind(..) => "Wind".into(),
            RecordedStep::Diffuse(_) => "Diffuse".into(),
            RecordedStep::Mezzotint(_) => "Mezzotint".into(),
            RecordedStep::DifferenceClouds => "Difference Clouds".into(),
            RecordedStep::SharpenEdges => "Sharpen Edges".into(),
            RecordedStep::Fragment => "Fragment".into(),
            RecordedStep::Facet => "Facet".into(),
            RecordedStep::Transform3d(..) => "3D Transform".into(),
            RecordedStep::ChannelMixer(_) => "Channel Mixer".into(),
            RecordedStep::SetEffects(_) => "Layer Effects".into(),
            RecordedStep::LayerNew => "New Layer".into(),
            RecordedStep::LayerDelete => "Delete Layer".into(),
            RecordedStep::LayerDuplicate => "Duplicate Layer".into(),
            RecordedStep::LayerSelect(_) => "Select Layer".into(),
        }
    }
}

/// A named, replayable sequence of steps (Phase 4 Actions).
#[derive(Clone, Debug)]
struct RecordedAction {
    name: String,
    steps: Vec<RecordedStep>,
    /// The action set this action belongs to (Phase 5).
    set: String,
}

/// The last Free Transform gesture, for Transform Again:
/// (sx, sy, rot, skew, dx, dy) + distort quad as offsets from
/// the bounds corners.
type LastFt = (f32, f32, f32, f32, f32, f32, Option<[(f32, f32); 4]>);

/// A no-dialog filter function (see `App::filter_now`).
type FilterFn = fn(&mut Document, usize, Option<&Selection>);

/// One wireframe line segment in the 3D Transform preview, and
/// the segment list the preview builds per shape.
type WireSeg = ((i32, i32), (i32, i32));
type WireSegs = Vec<WireSeg>;

/// A right-click context menu (Phase 4): items are (label, action,
/// enabled), drawn at (x, y) like a menu-bar dropdown.
struct ContextMenu {
    x: i32,
    y: i32,
    items: Vec<(String, Action, bool)>,
}

/// Apply one recorded step to a document (shared by action replay
/// and batch processing). Adjustments land on the active layer.
fn apply_step(doc: &mut Document, step: &RecordedStep, sel: Option<&Selection>) {
    use pictodesk_core::adjust::Adjustment;
    let layer = doc.active_layer;
    match step {
        RecordedStep::Adjustment(adj) => match adj {
            Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => adjust::levels(
                doc, layer, sel, *in_black, *in_white, *gamma, *out_black, *out_white,
            ),
            Adjustment::Curves { points } => adjust::curves(doc, layer, sel, points),
            Adjustment::BrightnessContrast {
                brightness,
                contrast,
            } => adjust::brightness_contrast(doc, layer, sel, *brightness, *contrast),
            Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
            } => adjust::hue_saturation(doc, layer, sel, *hue, *saturation, *lightness),
            Adjustment::ColorBalance {
                shadows,
                midtones,
                highlights,
            } => adjust::color_balance(doc, layer, sel, *shadows, *midtones, *highlights),
            Adjustment::PhotoFilter { color, density } => {
                adjust::photo_filter(doc, layer, sel, *color, *density)
            }
        },
        RecordedStep::AdjustmentLayer(adj) => {
            doc.add_adjustment_layer(adj.name(), adj.clone());
        }
        RecordedStep::Posterize(n) => adjust::posterize(doc, layer, sel, *n),
        RecordedStep::Threshold(n) => adjust::threshold(doc, layer, sel, *n),
        RecordedStep::Invert => adjust::invert(doc, layer, sel),
        RecordedStep::Desaturate => adjust::desaturate(doc, layer, sel),
        RecordedStep::GaussianBlur(r) => filter::gaussian_blur(doc, layer, sel, *r),
        RecordedStep::Unsharp(a, r, t) => filter::unsharp_mask(doc, layer, sel, *a, *r, *t),
        RecordedStep::AddNoise(a) => filter::add_noise(doc, layer, sel, *a, 12345),
        RecordedStep::ImageSize(w, h) => transform::resize_image(doc, *w, *h),
        RecordedStep::CanvasSize(w, h, anchor) => transform::canvas_size(doc, *w, *h, *anchor),
        RecordedStep::Rotate180 => transform::rotate_180(doc),
        RecordedStep::Rotate90Cw => transform::rotate_90_cw(doc),
        RecordedStep::Rotate90Ccw => transform::rotate_90_ccw(doc),
        RecordedStep::FlipH => transform::flip_horizontal(doc),
        RecordedStep::FlipV => transform::flip_vertical(doc),
        RecordedStep::Flatten => doc.flatten(),
        RecordedStep::CropToSelection => {
            if let Some((x, y, w, h)) = sel.and_then(|s| s.bounds()) {
                transform::crop(doc, x, y, w, h);
            }
        }
        RecordedStep::Equalize => adjust::equalize(doc, layer, sel),
        RecordedStep::Sharpen => filter::sharpen(doc, layer, sel),
        RecordedStep::MotionBlur(a, d) => filter::motion_blur(doc, layer, sel, *a, *d),
        RecordedStep::RadialBlur(a) => filter::radial_blur(doc, layer, sel, *a),
        RecordedStep::Median(r) => filter::median(doc, layer, sel, *r),
        RecordedStep::Despeckle => filter::despeckle(doc, layer, sel),
        RecordedStep::Emboss(a) => filter::emboss(doc, layer, sel, *a),
        RecordedStep::FindEdges => filter::find_edges(doc, layer, sel),
        RecordedStep::Solarize => filter::solarize(doc, layer, sel),
        RecordedStep::Clouds(fg, bg) => filter::clouds(doc, layer, sel, *fg, *bg, 12345),
        RecordedStep::Mosaic(c) => filter::mosaic(doc, layer, sel, *c),
        RecordedStep::HighPass(r) => filter::high_pass(doc, layer, sel, *r),
        RecordedStep::OffsetPx(dx, dy) => filter::offset_filter(doc, layer, sel, *dx, *dy),
        RecordedStep::CustomKernel(k, s, o) => {
            let kf: [f32; 25] = k.map(|x| x as f32);
            filter::custom_filter(doc, layer, sel, kf, *s as f32, *o as f32 / 255.0)
        }
        RecordedStep::Pinch(a) => filter::pinch(doc, layer, sel, *a),
        RecordedStep::Spherize(a) => filter::spherize(doc, layer, sel, *a),
        RecordedStep::Twirl(a) => filter::twirl(doc, layer, sel, *a),
        RecordedStep::Ripple(a, s) => filter::ripple(doc, layer, sel, *a, *s),
        RecordedStep::OceanRipple(s, m) => filter::ocean_ripple(doc, layer, sel, *s, *m),
        RecordedStep::Polar(to_polar) => filter::polar_coordinates(doc, layer, sel, *to_polar),
        RecordedStep::Shear(o) => filter::shear(doc, layer, sel, *o),
        RecordedStep::ZigZag(a, r, st) => filter::zigzag(doc, layer, sel, *a, *r, *st),
        RecordedStep::Maximum(r) => filter::maximum(doc, layer, sel, *r),
        RecordedStep::Minimum(r) => filter::minimum(doc, layer, sel, *r),
        RecordedStep::DustScratches(r, t) => filter::dust_and_scratches(doc, layer, sel, *r, *t),
        RecordedStep::Tiles(n, o) => filter::tiles(doc, layer, sel, *n, *o, 12345),
        RecordedStep::GlowingEdges(w, b, s) => filter::glowing_edges(doc, layer, sel, *w, *b, *s),
        RecordedStep::Patchwork(s, r) => filter::patchwork(doc, layer, sel, *s, *r),
        RecordedStep::StampEffect(b, s) => filter::stamp_effect(doc, layer, sel, *b, *s),
        RecordedStep::BasRelief(fg, bg, d, s) => {
            filter::bas_relief(doc, layer, sel, *fg, *bg, *d, *s)
        }
        RecordedStep::Wind(m, l) => filter::wind(doc, layer, sel, *m, *l, 12345),
        RecordedStep::Diffuse(m) => filter::diffuse(doc, layer, sel, *m, 12345),
        RecordedStep::Mezzotint(k) => filter::mezzotint(doc, layer, sel, *k, 12345),
        RecordedStep::DifferenceClouds => filter::difference_clouds(doc, layer, sel, 12345),
        RecordedStep::SharpenEdges => filter::sharpen_edges(doc, layer, sel),
        RecordedStep::Fragment => filter::fragment(doc, layer, sel),
        RecordedStep::Facet => filter::facet(doc, layer, sel),
        RecordedStep::Transform3d(s, rx, ry) => filter::transform_3d(doc, layer, sel, *s, *rx, *ry),
        RecordedStep::ChannelMixer(m) => adjust::channel_mixer(doc, layer, sel, m),
        RecordedStep::SetEffects(fx) => {
            doc.layers[layer].effects = Some(*fx);
        }
        RecordedStep::LayerNew => {
            let n = doc.layers.len();
            doc.add_layer(format!("Layer {}", n + 1));
        }
        RecordedStep::LayerDelete => {
            doc.delete_layer(layer);
        }
        RecordedStep::LayerDuplicate => {
            doc.duplicate_layer(layer);
        }
        RecordedStep::LayerSelect(i) => {
            if *i < doc.layers.len() {
                doc.active_layer = *i;
            }
        }
    }
}

fn apply_steps(doc: &mut Document, steps: &[RecordedStep], sel: Option<&Selection>) {
    for step in steps {
        apply_step(doc, step, sel);
    }
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
    /// Last Free Transform parameters for Transform Again
    /// (sx, sy, rot, skew, dx, dy) + optional distort quad.
    last_ft: Option<LastFt>,
    /// Extract dialog: the last computed preview render.
    extract_preview_cache: Option<Vec<u8>>,
    /// Eraser option: erase back to the history source state.
    eraser_to_history: bool,
    /// Art History Brush style: 0 Tight, 1 Loose, 2 Dab.
    art_hist_style: u8,
    show_guides: bool,
    show_grid: bool,
    snap_enabled: bool,
    actions: Vec<RecordedAction>,
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
    file_panel: Option<Vec<PathBuf>>,
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
    ants_phase: i32,
    quit: bool,
}

impl App {
    fn new(doc: Document, file_path: Option<PathBuf>) -> Self {
        let mut hist = History::new();
        hist.set_limit(Some(20)); // Phase 5: the History panel's default depth
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
            last_ft: None,
            extract_preview_cache: None,
            eraser_to_history: false,
            art_hist_style: 0,
            show_guides: true,
            show_grid: false,
            snap_enabled: true,
            actions: Vec::new(),
            active_action: 0,
            recording: false,
            action_sets: vec!["Default Set".to_string()],
            action_set: "Default Set".to_string(),
            context_menu: None,
            guide_drag: None,
            file_panel: None,
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
        };
        app.reset_hist_for_new_doc();
        app
    }

    /// A different document arrived (new/open): snapshots and the
    /// brush source reset, and with Auto Snapshot on, the opened
    /// state is frozen as Snapshot 1 and becomes the brush source.
    fn reset_hist_for_new_doc(&mut self) {
        self.snapshots.clear();
        self.hist_scroll = 0;
        self.hist_selected = None;
        self.hist_src_cache = None;
        self.measure = None;
        if self.hist_auto_snapshot {
            self.snapshots.push(Snapshot {
                name: "Snapshot 1".into(),
                doc: self.doc.clone(),
                kind: SnapKind::Full,
            });
            self.hist_source = HistSource::Snapshot(0);
        } else {
            self.hist_source = HistSource::State(0);
        }
    }

    /// Freeze the current document as a new snapshot.
    /// Replace the current selection, remembering the outgoing
    /// one for Select > Reselect.
    fn replace_sel(&mut self, s: Option<Selection>) {
        if let Some(old) = self.sel.take() {
            if old.bounds().is_some() {
                self.doc.last_selection = Some(old);
            }
        }
        self.sel = s;
    }

    fn take_snapshot(&mut self) {
        self.take_snapshot_kind(SnapKind::Full);
    }

    /// Take a snapshot of the chosen kind: the whole document,
    /// the document merged flat, or just the active layer.
    fn take_snapshot_kind(&mut self, kind: SnapKind) {
        let n = self.snapshots.len() + 1;
        let (doc, tag) = match kind {
            SnapKind::Full => (self.doc.clone(), ""),
            SnapKind::Merged => {
                let mut d = self.doc.clone();
                d.flatten();
                (d, " (merged)")
            }
            SnapKind::Layer(_) => (self.doc.clone(), " (layer)"),
        };
        self.snapshots.push(Snapshot {
            name: format!("Snapshot {n}{tag}"),
            doc,
            kind,
        });
        self.status = format!("Snapshot {n}{tag} taken");
    }

    /// The document the History Brush and Fill with History paint
    /// from, rebuilt when the source or history position changes.
    fn hist_source_doc(&mut self) -> Option<Document> {
        // Cache key: the source plus the history shape it was
        // reconstructed from (position + state count).
        let shape = (self.hist.position(), self.hist.state_count());
        if let Some((src, pos, count, doc)) = &self.hist_src_cache {
            if *src == self.hist_source && (*pos, *count) == shape {
                return Some(doc.clone());
            }
        }
        let doc = match self.hist_source {
            HistSource::State(s) => self.hist.doc_at_state(&self.doc, s)?,
            HistSource::Snapshot(i) => {
                let snap = self.snapshots.get(i)?;
                match snap.kind {
                    SnapKind::Layer(li) => {
                        // A layer snapshot paints from the current
                        // document with that one layer rolled back.
                        let mut d = self.doc.clone();
                        if li < d.layers.len() && li < snap.doc.layers.len() {
                            d.layers[li] = snap.doc.layers[li].clone();
                        }
                        d
                    }
                    _ => snap.doc.clone(),
                }
            }
        };
        self.hist_src_cache = Some((self.hist_source, shape.0, shape.1, doc.clone()));
        Some(doc)
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
        self.show_cp
            || self.show_brushes
            || self.show_histogram
            || self.show_palette
            || self.show_navigator
            || self.show_actions
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
            let brushes_h = if matches!(
                self.tool,
                Tool::Dodge
                    | Tool::Burn
                    | Tool::Sponge
                    | Tool::Eraser
                    | Tool::MagicEraser
                    | Tool::BackgroundEraser
                    | Tool::ArtHistoryBrush
                    | Tool::Gradient
            ) {
                108
            } else {
                84
            };
            place(PanelKind::Brushes, brushes_h, &mut out, &mut y);
        }
        if self.show_histogram {
            place(PanelKind::Histogram, 132, &mut out, &mut y);
        }
        if self.show_palette {
            place(PanelKind::Color, 168, &mut out, &mut y);
        }
        if self.show_navigator {
            place(PanelKind::Navigator, 128, &mut out, &mut y);
        }
        if self.show_actions {
            let rows = self.action_rows().len().max(1);
            let h = 20 + rows.min(6) as i32 * 15 + 58;
            place(PanelKind::Actions, h, &mut out, &mut y);
        }
        if self.show_history {
            let rows = self.snapshots.len() + self.hist.state_count() + 1;
            let h = 20 + rows.min(9) as i32 * 15 + 38;
            place(PanelKind::History, h, &mut out, &mut y);
        }
        if self.show_info {
            place(PanelKind::Info, 118, &mut out, &mut y);
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
        let labels = self.hist.labels();
        let undo_label = if self.hist.can_undo() {
            format!("Undo {}", labels[self.hist.position() - 1])
        } else {
            "Undo".to_string()
        };
        let redo_label = if self.hist.can_redo() {
            format!("Redo {}", labels[self.hist.position()])
        } else {
            "Redo".to_string()
        };
        let can_undo = self.hist.can_undo();
        let can_redo = self.hist.can_redo();
        vec![
            Menu {
                title: "File",
                items: vec![
                    MenuItem::new("New...", Action::New).key("N"),
                    MenuItem::new("Open...", Action::OpenPanel).key("O"),
                    MenuItem::new("Save", Action::Save).key("S"),
                    MenuItem::new("Save As...", Action::SaveAsPanel),
                    MenuItem::new("History Log...", Action::HistoryLog),
                    MenuItem::new("Automate", Action::None).sub(vec![
                        MenuItem::new("Batch...", Action::MacroBatch),
                        MenuItem::new("Conditional Mode Change...", Action::CondModeDialog),
                        MenuItem::new("Contact Sheet II...", Action::ContactSheetDialog),
                        MenuItem::new("Fit Image...", Action::FitImageDialog),
                        MenuItem::new("Picture Package...", Action::PicturePackageDialog),
                        MenuItem::new("Web Photo Gallery...", Action::WebGalleryDialog),
                    ]),
                    MenuItem::new("Export", Action::None).sub(vec![
                        MenuItem::new("PNG...", Action::ExportPng),
                        MenuItem::new("PSD (layered)...", Action::ExportPsd),
                        MenuItem::new("EPS DCS 2.0...", Action::ExportDcs),
                        MenuItem::new("Save for Web...", Action::SaveForWebDialog),
                    ]),
                    MenuItem::new("Page Setup...", Action::PageSetupDialog),
                    MenuItem::new("Print...", Action::PrintDialog).key("P"),
                    MenuItem::new("Revert", Action::Revert)
                        .off_if(!self.dirty && self.file_path.is_none()),
                    MenuItem::new("Quit", Action::Quit),
                ],
            },
            Menu {
                title: "Edit",
                items: vec![
                    MenuItem::new(undo_label, Action::Undo)
                        .off_if(!can_undo)
                        .key("Z"),
                    MenuItem::new(redo_label, Action::Redo)
                        .off_if(!can_redo)
                        .key("Shift+Z"),
                    MenuItem::new("Cut", Action::Cut).off_if(!has_sel).key("X"),
                    MenuItem::new("Copy", Action::Copy)
                        .off_if(!has_sel)
                        .key("C"),
                    MenuItem::new("Copy Merged", Action::CopyMerged)
                        .off_if(!has_sel)
                        .key("Shift+C"),
                    MenuItem::new("Paste", Action::Paste)
                        .off_if(CLIPBOARD.with(|c| c.borrow().is_none()))
                        .key("V"),
                    MenuItem::new("Paste Into", Action::PasteInto)
                        .off_if(CLIPBOARD.with(|c| c.borrow().is_none()) || !has_sel)
                        .key("Shift+V"),
                    MenuItem::new("Clear", Action::Clear).off_if(!has_sel),
                    MenuItem::new("Fill...", Action::FillDialog),
                    MenuItem::new("Define Pattern", Action::DefinePattern),
                    MenuItem::new("Free Transform...", Action::FreeTransform).key("T"),
                    MenuItem::new("Transform Again", Action::TransformAgain).key("Shift+T"),
                    MenuItem::new("Transform Numeric...", Action::FtNumericDialog)
                        .off_if(self.ft.is_none()),
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
                        MenuItem::new("Equalize", Action::AdjustEqualize),
                        MenuItem::new("Desaturate", Action::AdjustDialog("desat")),
                        MenuItem::new("Posterize...", Action::AdjustDialog("posterize")),
                        MenuItem::new("Threshold...", Action::AdjustDialog("threshold")),
                        MenuItem::new("Photo Filter...", Action::AdjustDialog("photo")),
                        MenuItem::new("Channel Mixer...", Action::ChannelMixerDialog),
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
                    let has_mask = self.doc.layers[a].mask.is_some();
                    let is_adj = self.doc.layers[a].adjustment.is_some();
                    vec![
                        MenuItem::new("New Layer...", Action::NewLayer).key("Shift+N"),
                        MenuItem::new("New Adjustment Layer", Action::None).sub(vec![
                            MenuItem::new("Levels...", Action::NewAdjustment("levels")),
                            MenuItem::new("Curves...", Action::NewAdjustment("curves")),
                            MenuItem::new(
                                "Brightness/Contrast...",
                                Action::NewAdjustment("brightness"),
                            ),
                            MenuItem::new("Hue/Saturation...", Action::NewAdjustment("hue")),
                            MenuItem::new("Color Balance...", Action::NewAdjustment("balance")),
                            MenuItem::new("Photo Filter...", Action::NewAdjustment("photo")),
                        ]),
                        MenuItem::new("Edit Adjustment Layer...", Action::EditAdjustment)
                            .off_if(!is_adj),
                        MenuItem::new("Duplicate Layer", Action::DuplicateLayer),
                        MenuItem::new("Delete Layer", Action::DeleteLayer)
                            .off_if(self.doc.layers.len() <= 1),
                        MenuItem::new("Layer Options...", Action::LayerOptions),
                        MenuItem::new("Add Layer Mask", Action::None)
                            .off_if(has_mask)
                            .sub(vec![
                                MenuItem::new("Reveal All", Action::AddMaskReveal),
                                MenuItem::new("Hide All", Action::AddMaskHide),
                                MenuItem::new("From Selection", Action::AddMaskSelection)
                                    .off_if(!has_sel),
                            ]),
                        MenuItem::new("Delete Layer Mask", Action::DeleteMask).off_if(!has_mask),
                        MenuItem::new("Disable/Enable Mask", Action::ToggleMaskEnabled)
                            .off_if(!has_mask),
                        MenuItem::new("Feather Mask...", Action::FeatherMask).off_if(!has_mask),
                        MenuItem::new("Blend Mode", Action::None).sub(blend_items),
                        MenuItem::new("Effects", Action::None)
                            .off_if(is_adj)
                            .sub(vec![
                                MenuItem::new("Drop Shadow...", Action::EffectsDialog(0)),
                                MenuItem::new("Inner Shadow...", Action::EffectsDialog(1)),
                                MenuItem::new("Outer Glow...", Action::EffectsDialog(2)),
                                MenuItem::new("Inner Glow...", Action::EffectsDialog(3)),
                                MenuItem::new("Bevel and Emboss...", Action::EffectsDialog(4)),
                                MenuItem::new("-", Action::None),
                                MenuItem::new("Copy Effects", Action::CopyEffects),
                                MenuItem::new("Paste Effects", Action::PasteEffects),
                                MenuItem::new(
                                    "Paste Effects to Linked",
                                    Action::PasteEffectsLinked,
                                ),
                                MenuItem::new("Clear Effects", Action::ClearEffects),
                                MenuItem::new("Create Layers", Action::CreateEffectLayers),
                                MenuItem::new("Global Angle...", Action::GlobalAngleDialog),
                                MenuItem::new(
                                    if self.doc.effects_suppressed {
                                        "Show All Effects"
                                    } else {
                                        "Hide All Effects"
                                    },
                                    Action::ToggleEffectsVisible,
                                ),
                            ]),
                        MenuItem::new("Align Linked", Action::None).sub(vec![
                            MenuItem::new("Top Edges", Action::AlignLinked(0)),
                            MenuItem::new("Vertical Centers", Action::AlignLinked(1)),
                            MenuItem::new("Bottom Edges", Action::AlignLinked(2)),
                            MenuItem::new("Left Edges", Action::AlignLinked(3)),
                            MenuItem::new("Horizontal Centers", Action::AlignLinked(4)),
                            MenuItem::new("Right Edges", Action::AlignLinked(5)),
                        ]),
                        MenuItem::new("Distribute Linked", Action::None).sub(vec![
                            MenuItem::new("Top Edges", Action::DistributeLinked(0)),
                            MenuItem::new("Vertical Centers", Action::DistributeLinked(1)),
                            MenuItem::new("Bottom Edges", Action::DistributeLinked(2)),
                            MenuItem::new("Left Edges", Action::DistributeLinked(3)),
                            MenuItem::new("Horizontal Centers", Action::DistributeLinked(4)),
                            MenuItem::new("Right Edges", Action::DistributeLinked(5)),
                        ]),
                        MenuItem::new("Bring to Front", Action::LayerToFront).key("Shift+]"),
                        MenuItem::new("Bring Forward", Action::BringForward).key("]"),
                        MenuItem::new("Send Backward", Action::SendBackward).key("["),
                        MenuItem::new("Send to Back", Action::LayerToBack).key("Shift+["),
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
                items: {
                    let cur = self.doc.working_space;
                    let spaces = [
                        pictodesk_core::color::WorkingSpace::Srgb,
                        pictodesk_core::color::WorkingSpace::AdobeRgb,
                        pictodesk_core::color::WorkingSpace::ProPhoto,
                    ];
                    let assign: Vec<MenuItem> = spaces
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            MenuItem::new(space_name(*s), Action::AssignProfile(i))
                                .check_if(*s == cur)
                        })
                        .collect();
                    let convert: Vec<MenuItem> = spaces
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            MenuItem::new(space_name(*s), Action::ConvertProfile(i))
                                .check_if(*s == cur)
                        })
                        .collect();
                    vec![
                        MenuItem::new("RGB Color", Action::ModeRgb)
                            .check_if(self.doc.mode == ColorMode::Rgb),
                        MenuItem::new("Grayscale", Action::ModeGrayscale)
                            .check_if(self.doc.mode == ColorMode::Grayscale),
                        MenuItem::new("CMYK Color", Action::ModeCmyk)
                            .check_if(self.doc.mode == ColorMode::Cmyk),
                        MenuItem::new("-", Action::None),
                        MenuItem::new("Color Settings...", Action::ColorSettingsDialog),
                        MenuItem::new("Assign Profile", Action::None).sub(assign),
                        MenuItem::new("Convert to Profile", Action::None).sub(convert),
                        MenuItem::new("Proof Colors (CMYK)", Action::ToggleProof)
                            .check_if(self.proof_cmyk),
                    ]
                },
            },
            Menu {
                title: "Filter",
                items: vec![
                    MenuItem::new("Extract...", Action::ExtractDialog),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Blur", Action::None).sub(vec![
                        MenuItem::new("Gaussian Blur...", Action::FilterGaussian),
                        MenuItem::new("Motion Blur...", Action::FilterMotion),
                        MenuItem::new("Radial Blur...", Action::FilterRadial),
                    ]),
                    MenuItem::new("Distort", Action::None).sub(vec![
                        MenuItem::new("Pinch...", Action::LegacyFilterDialog(0)),
                        MenuItem::new("Spherize...", Action::LegacyFilterDialog(1)),
                        MenuItem::new("Twirl...", Action::LegacyFilterDialog(2)),
                        MenuItem::new("Ripple...", Action::LegacyFilterDialog(3)),
                        MenuItem::new("Ocean Ripple...", Action::LegacyFilterDialog(4)),
                        MenuItem::new("Shear...", Action::LegacyFilterDialog(5)),
                        MenuItem::new("ZigZag...", Action::LegacyFilterDialog(6)),
                        MenuItem::new("Polar Coordinates", Action::None).sub(vec![
                            MenuItem::new("Rectangular to Polar", Action::FilterPolar(true)),
                            MenuItem::new("Polar to Rectangular", Action::FilterPolar(false)),
                        ]),
                    ]),
                    MenuItem::new("Noise", Action::None).sub(vec![
                        MenuItem::new("Add Noise...", Action::FilterNoise),
                        MenuItem::new("Despeckle", Action::FilterDespeckle),
                        MenuItem::new("Dust & Scratches...", Action::LegacyFilterDialog(9)),
                        MenuItem::new("Median...", Action::FilterMedian),
                    ]),
                    MenuItem::new("Pixelate", Action::None).sub(vec![
                        MenuItem::new("Facet", Action::FilterSpecial(3)),
                        MenuItem::new("Fragment", Action::FilterSpecial(2)),
                        MenuItem::new("Mezzotint", Action::None).sub(vec![
                            MenuItem::new("Fine Dots", Action::FilterMezzotint(0)),
                            MenuItem::new("Medium Dots", Action::FilterMezzotint(1)),
                            MenuItem::new("Grainy Dots", Action::FilterMezzotint(2)),
                            MenuItem::new("Coarse Lines", Action::FilterMezzotint(3)),
                        ]),
                        MenuItem::new("Mosaic...", Action::FilterMosaic),
                    ]),
                    MenuItem::new("Render", Action::None).sub(vec![
                        MenuItem::new("Clouds", Action::FilterClouds),
                        MenuItem::new("Difference Clouds", Action::FilterSpecial(0)),
                        MenuItem::new("3D Transform...", Action::FilterTransform3d),
                    ]),
                    MenuItem::new("Sharpen", Action::None).sub(vec![
                        MenuItem::new("Sharpen", Action::FilterSharpen),
                        MenuItem::new("Sharpen Edges", Action::FilterSpecial(1)),
                        MenuItem::new("Unsharp Mask...", Action::FilterUnsharp),
                    ]),
                    MenuItem::new("Sketch", Action::None).sub(vec![
                        MenuItem::new("Bas Relief...", Action::LegacyFilterDialog(14)),
                        MenuItem::new("Stamp...", Action::LegacyFilterDialog(13)),
                    ]),
                    MenuItem::new("Stylize", Action::None).sub(vec![
                        MenuItem::new("Diffuse", Action::None).sub(vec![
                            MenuItem::new("Normal", Action::FilterDiffuse(0)),
                            MenuItem::new("Darken Only", Action::FilterDiffuse(1)),
                            MenuItem::new("Lighten Only", Action::FilterDiffuse(2)),
                            MenuItem::new("Anisotropic", Action::FilterDiffuse(3)),
                        ]),
                        MenuItem::new("Emboss...", Action::FilterEmboss),
                        MenuItem::new("Find Edges", Action::FilterFindEdges),
                        MenuItem::new("Glowing Edges...", Action::LegacyFilterDialog(11)),
                        MenuItem::new("Solarize", Action::FilterSolarize),
                        MenuItem::new("Tiles...", Action::LegacyFilterDialog(10)),
                        MenuItem::new("Wind...", Action::LegacyFilterDialog(15)),
                    ]),
                    MenuItem::new("Texture", Action::None).sub(vec![MenuItem::new(
                        "Patchwork...",
                        Action::LegacyFilterDialog(12),
                    )]),
                    MenuItem::new("Other", Action::None).sub(vec![
                        MenuItem::new("Custom...", Action::FilterCustom),
                        MenuItem::new("High Pass...", Action::FilterHighPass),
                        MenuItem::new("Maximum...", Action::LegacyFilterDialog(7)),
                        MenuItem::new("Minimum...", Action::LegacyFilterDialog(8)),
                        MenuItem::new("Offset...", Action::FilterOffset),
                    ]),
                ],
            },
            Menu {
                title: "Select",
                items: {
                    let mut items = vec![
                        MenuItem::new("All", Action::SelectAll).key("A"),
                        MenuItem::new("None", Action::SelectNone).key("D"),
                        MenuItem::new("Reselect", Action::Reselect)
                            .off_if(self.doc.last_selection.is_none())
                            .key("Shift+D"),
                        MenuItem::new("Inverse", Action::SelectInverse).off_if(!has_sel),
                        MenuItem::new("Transform Selection...", Action::TransformSelection)
                            .off_if(!has_sel),
                        MenuItem::new("Color Range...", Action::SelectColorRange),
                        MenuItem::new("Feather...", Action::FeatherDialog).off_if(!has_sel),
                        MenuItem::new("Modify", Action::None)
                            .off_if(!has_sel)
                            .sub(vec![
                                MenuItem::new("Border...", Action::ModifyBorder),
                                MenuItem::new("Smooth...", Action::ModifySmooth),
                                MenuItem::new("Expand...", Action::ModifyExpand),
                                MenuItem::new("Contract...", Action::ModifyContract),
                            ]),
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
                    MenuItem::new("Navigator", Action::ToggleNavigator)
                        .check_if(self.show_navigator),
                    MenuItem::new("Actions", Action::ToggleActionsPanel)
                        .check_if(self.show_actions),
                    MenuItem::new("History", Action::ToggleHistoryPanel)
                        .check_if(self.show_history),
                    MenuItem::new("Info", Action::ToggleInfoPanel).check_if(self.show_info),
                    MenuItem::new("Rulers", Action::ToggleRulers).check_if(self.show_rulers),
                    MenuItem::new("Guides", Action::ToggleGuides).check_if(self.show_guides),
                    MenuItem::new("Grid", Action::ToggleGrid).check_if(self.show_grid),
                    MenuItem::new("Snap", Action::ToggleSnap).check_if(self.snap_enabled),
                    MenuItem::new("Clear Guides", Action::ClearGuides)
                        .off_if(self.doc.guides_x.is_empty() && self.doc.guides_y.is_empty()),
                    MenuItem::new("Zoom In", Action::ZoomIn).key("+"),
                    MenuItem::new("Zoom Out", Action::ZoomOut).key("-"),
                    MenuItem::new("Actual Pixels", Action::Zoom100).key("1"),
                    MenuItem::new("Fit on Screen", Action::ZoomFit).key("0"),
                ],
            },
        ]
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

    /// Run a no-dialog filter or adjustment as one undoable tile
    /// edit, and record it as an Action step.
    fn apply_filter_now(
        &mut self,
        label: &str,
        step: RecordedStep,
        f: impl FnOnce(&mut Document, usize, Option<&Selection>),
    ) {
        let layer = self.doc.active_layer;
        let tiles = self.sel_tiles();
        let sel = self.sel.clone();
        run_tile_edit(&mut self.doc, &mut self.hist, label, layer, &tiles, |d| {
            f(d, layer, sel.as_ref());
        });
        self.record_step(step);
        self.last_label = Some(label.into());
        self.status = label.into();
        self.touch();
    }

    /// Open the Modify Selection dialog for one operation
    /// (0 Expand, 1 Contract, 2 Smooth, 3 Border).
    fn open_modify_dialog(&mut self, op: u8) {
        let mut d = Dialog::new(DialogKind::ModifySel, &self.doc);
        d.modify_op = op;
        self.dialog = Some(d);
    }

    /// If an Actions recording is running, append a step to it.
    fn record_step(&mut self, step: RecordedStep) {
        if self.recording {
            if let Some(a) = self.actions.get_mut(self.active_action) {
                a.steps.push(step);
            }
        }
    }

    /// Build the Adjustment a dialog's current values describe.
    fn adjustment_from_dialog(d: &Dialog) -> pictodesk_core::adjust::Adjustment {
        use pictodesk_core::adjust::Adjustment;
        let v = &d.vals;
        match d.kind {
            DialogKind::Levels => Adjustment::Levels {
                in_black: v[0] as u8,
                in_white: v[1] as u8,
                gamma: v[2],
                out_black: v[3] as u8,
                out_white: v[4] as u8,
            },
            DialogKind::Curves => Adjustment::Curves {
                points: d.curve_points.clone(),
            },
            DialogKind::BrightnessContrast => Adjustment::BrightnessContrast {
                brightness: v[0] as i32,
                contrast: v[1] as i32,
            },
            DialogKind::HueSaturation => Adjustment::HueSaturation {
                hue: v[0] as i32,
                saturation: v[1] as i32,
                lightness: v[2] as i32,
            },
            DialogKind::ColorBalance => {
                let g = |r: usize, i: usize| v[r * 3 + i] as i32;
                Adjustment::ColorBalance {
                    shadows: (g(0, 0), g(0, 1), g(0, 2)),
                    midtones: (g(1, 0), g(1, 1), g(1, 2)),
                    highlights: (g(2, 0), g(2, 1), g(2, 2)),
                }
            }
            _ => Adjustment::PhotoFilter {
                color: d.photo_color,
                density: v[0] as u8,
            },
        }
    }

    /// Open an adjustment dialog that creates a new adjustment
    /// layer (existing = None) or edits an existing one live.
    fn open_adjustment_dialog(&mut self, kind: &str, existing: Option<usize>) {
        use pictodesk_core::adjust::Adjustment;
        let dk = match kind {
            "levels" => DialogKind::Levels,
            "curves" => DialogKind::Curves,
            "brightness" => DialogKind::BrightnessContrast,
            "hue" => DialogKind::HueSaturation,
            "balance" => DialogKind::ColorBalance,
            _ => DialogKind::PhotoFilter,
        };
        let snapshot = self.doc.clone();
        let layer_idx = match existing {
            Some(i) => i,
            None => {
                let adj = match kind {
                    "levels" => Adjustment::Levels {
                        in_black: 0,
                        in_white: 255,
                        gamma: 1.0,
                        out_black: 0,
                        out_white: 255,
                    },
                    "curves" => Adjustment::Curves {
                        points: vec![(0, 0), (255, 255)],
                    },
                    "brightness" => Adjustment::BrightnessContrast {
                        brightness: 0,
                        contrast: 0,
                    },
                    "hue" => Adjustment::HueSaturation {
                        hue: 0,
                        saturation: 0,
                        lightness: 0,
                    },
                    "balance" => Adjustment::ColorBalance {
                        shadows: (0, 0, 0),
                        midtones: (0, 0, 0),
                        highlights: (0, 0, 0),
                    },
                    _ => Adjustment::PhotoFilter {
                        color: (255, 178, 84),
                        density: 25,
                    },
                };
                let name = adj.name().to_string();
                let i = self.doc.add_adjustment_layer(name, adj);
                self.doc_version += 1;
                i
            }
        };
        self.doc.active_layer = layer_idx;
        let mut d = Dialog::new(dk, &self.doc);
        // Seed the dialog from the layer's current parameters.
        if let Some(adj) = &self.doc.layers[layer_idx].adjustment {
            match adj {
                Adjustment::Levels {
                    in_black,
                    in_white,
                    gamma,
                    out_black,
                    out_white,
                } => {
                    d.vals = vec![
                        *in_black as f32,
                        *in_white as f32,
                        *gamma,
                        *out_black as f32,
                        *out_white as f32,
                    ];
                }
                Adjustment::Curves { points } => {
                    d.curve_points = points.clone();
                }
                Adjustment::BrightnessContrast {
                    brightness,
                    contrast,
                } => {
                    d.vals = vec![*brightness as f32, *contrast as f32];
                }
                Adjustment::HueSaturation {
                    hue,
                    saturation,
                    lightness,
                } => {
                    d.vals = vec![*hue as f32, *saturation as f32, *lightness as f32];
                }
                Adjustment::ColorBalance {
                    shadows,
                    midtones,
                    highlights,
                } => {
                    d.vals = vec![
                        shadows.0 as f32,
                        shadows.1 as f32,
                        shadows.2 as f32,
                        midtones.0 as f32,
                        midtones.1 as f32,
                        midtones.2 as f32,
                        highlights.0 as f32,
                        highlights.1 as f32,
                        highlights.2 as f32,
                    ];
                }
                Adjustment::PhotoFilter { color, density } => {
                    d.vals = vec![*density as f32];
                    d.photo_color = *color;
                }
            }
        }
        d.adj_layer = Some(layer_idx);
        d.adj_snapshot = Some(snapshot);
        let title = d.kind.title();
        self.dialog = Some(d);
        self.status = format!("Adjustment layer: {title}");
    }

    /// Batch-process a folder with the selected action: every
    /// image file in the folder is opened, the action's steps are
    /// applied, and the result is written next to the original as
    /// `<name>-edited.png`. Originals are never overwritten.
    fn run_batch(&mut self, folder_text: &str) {
        let Some(action) = self.actions.get(self.active_action).cloned() else {
            self.status = "No action selected for batch".into();
            return;
        };
        if action.steps.is_empty() {
            self.status = "That action has no steps yet".into();
            return;
        }
        let dir = std::path::PathBuf::from(folder_text.trim());
        if !dir.is_dir() {
            self.status = format!("Batch: '{}' is not a folder", dir.display());
            return;
        }
        let mut done = 0usize;
        let mut failed = 0usize;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut paths: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "gif" | "webp"
                ) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    failed += 1;
                    continue;
                };
                let Ok(mut doc) = io::import_image(&bytes) else {
                    failed += 1;
                    continue;
                };
                apply_steps(&mut doc, &action.steps, None);
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let out = dir.join(format!("{stem}-edited.png"));
                match io::export_image(&doc, ExportFormat::Png, 90) {
                    Ok(png) => {
                        if std::fs::write(&out, png).is_ok() {
                            done += 1;
                        } else {
                            failed += 1;
                        }
                    }
                    Err(_) => failed += 1,
                }
            }
        }
        self.status = format!(
            "Batch '{}' : {done} file(s) processed, {failed} failed",
            action.name
        );
    }

    /// Replay an action's steps onto the current document as one
    /// undoable step.
    fn play_action(&mut self, idx: usize) {
        let Some(action) = self.actions.get(idx).cloned() else {
            self.status = "No action to play".into();
            return;
        };
        if action.steps.is_empty() {
            self.status = "That action has no steps yet".into();
            return;
        }
        let sel = self.sel.clone();
        let steps = action.steps.clone();
        let label = format!("Play: {}", action.name);
        self.structural(&label, |doc| {
            apply_steps(doc, &steps, sel.as_ref());
        });
        self.status = format!("Played '{}'", action.name);
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
            quad: None,
            start_quad: None,
            sel_mode: false,
            origin_sel: None,
        });
        self.status =
            "Free Transform: drag handles to scale, inside to move, outside to rotate, Ctrl+drag to skew, Ctrl+drag a corner to distort — Enter applies, Esc cancels"
                .into();
    }

    /// Edit > Transform Again: re-apply the last Free Transform
    /// gesture to the active layer (and its linked partners).
    fn transform_again(&mut self) {
        let Some((sx, sy, rot, skew, dx, dy, rel_quad)) = self.last_ft else {
            self.status = "Transform Again: nothing to repeat yet".into();
            return;
        };
        let a = self.doc.active_layer;
        let Some(b) = pictodesk_core::transform::layer_painted_bounds(&self.doc.layers[a]) else {
            self.status = "Transform Again: the active layer is empty".into();
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
            targets: if rel_quad.is_some() { vec![a] } else { targets },
            bounds: (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32),
            sx,
            sy,
            rot,
            skew,
            dx,
            dy,
            drag: FtDrag::None,
            drag_start_doc: (0.0, 0.0),
            start: (sx, sy, rot, skew, dx, dy),
            start_angle: 0.0,
            quad: rel_quad.map(|rel| {
                let base = [
                    (b.0 as f32, b.1 as f32),
                    (b.2 as f32, b.1 as f32),
                    (b.2 as f32, b.3 as f32),
                    (b.0 as f32, b.3 as f32),
                ];
                let mut q = [(0.0f32, 0.0f32); 4];
                for i in 0..4 {
                    q[i] = (base[i].0 + rel[i].0, base[i].1 + rel[i].1);
                }
                q
            }),
            start_quad: None,
            sel_mode: false,
            origin_sel: None,
        });
        self.ft_apply();
        self.ft_commit();
    }

    /// Painted bounds of a layer as (x0, y0, x1, y1) exclusive.
    fn layer_bounds(&self, li: usize) -> Option<(f32, f32, f32, f32)> {
        pictodesk_core::transform::layer_painted_bounds(&self.doc.layers[li])
            .map(|b| (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32))
    }

    /// Layer > Align Linked: line up every linked layer against
    /// the active layer. Modes: 0 top, 1 v-center, 2 bottom,
    /// 3 left, 4 h-center, 5 right.
    fn align_linked(&mut self, mode: u8) {
        let a = self.doc.active_layer;
        let Some(r) = self.layer_bounds(a) else {
            self.status = "Align: the active layer is empty".into();
            return;
        };
        let targets: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i != a && l.linked)
            .map(|(i, _)| i)
            .collect();
        if targets.is_empty() {
            self.status = "Align Linked: link some layers first".into();
            return;
        }
        let mut moves: Vec<(usize, i32, i32)> = Vec::new();
        for t in &targets {
            let Some(b) = self.layer_bounds(*t) else {
                continue;
            };
            let (mut dx, mut dy) = (0.0f32, 0.0f32);
            match mode {
                0 => dy = r.1 - b.1,
                1 => dy = (r.1 + r.3) / 2.0 - (b.1 + b.3) / 2.0,
                2 => dy = r.3 - b.3,
                3 => dx = r.0 - b.0,
                4 => dx = (r.0 + r.2) / 2.0 - (b.0 + b.2) / 2.0,
                _ => dx = r.2 - b.2,
            }
            moves.push((*t, dx.round() as i32, dy.round() as i32));
        }
        if moves.is_empty() {
            return;
        }
        self.structural("Align Linked", |doc| {
            for (t, dx, dy) in &moves {
                let o = doc.layers[*t].offset;
                doc.layers[*t].offset = (o.0 + dx, o.1 + dy);
            }
        });
        self.status = "Layers aligned".into();
    }

    /// Layer > Distribute Linked: space the linked layers (plus
    /// the active one) evenly along an axis; the two end layers
    /// stay put. Needs at least three layers in the set.
    fn distribute_linked(&mut self, mode: u8) {
        let a = self.doc.active_layer;
        let mut set: Vec<usize> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i == a || l.linked)
            .map(|(i, _)| i)
            .collect();
        if set.len() < 3 {
            self.status = "Distribute needs the active layer plus two linked".into();
            return;
        }
        // Anchor per layer: the mode's edge/center coordinate.
        let anchor = |me: &Self, li: usize| -> Option<f32> {
            let b = me.layer_bounds(li)?;
            Some(match mode {
                0 => b.1,
                1 => (b.1 + b.3) / 2.0,
                2 => b.3,
                3 => b.0,
                4 => (b.0 + b.2) / 2.0,
                _ => b.2,
            })
        };
        let mut keyed: Vec<(usize, f32)> = Vec::new();
        for li in set.drain(..) {
            if let Some(v) = anchor(self, li) {
                keyed.push((li, v));
            }
        }
        if keyed.len() < 3 {
            self.status = "Distribute: too many empty layers".into();
            return;
        }
        keyed.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
        let first = keyed.first().unwrap().1;
        let last = keyed.last().unwrap().1;
        let step = (last - first) / (keyed.len() - 1) as f32;
        let vertical = mode <= 2;
        let mut moves: Vec<(usize, i32, i32)> = Vec::new();
        for (k, (li, cur)) in keyed.iter().enumerate() {
            let want = first + step * k as f32;
            let d = (want - cur).round() as i32;
            if d != 0 {
                moves.push((
                    *li,
                    if vertical { 0 } else { d },
                    if vertical { d } else { 0 },
                ));
            }
        }
        if moves.is_empty() {
            self.status = "Already evenly spaced".into();
            return;
        }
        self.structural("Distribute Linked", |doc| {
            for (t, dx, dy) in &moves {
                let o = doc.layers[*t].offset;
                doc.layers[*t].offset = (o.0 + dx, o.1 + dy);
            }
        });
        self.status = "Layers distributed".into();
    }

    /// Select > Transform Selection: the same transform box, but
    /// it warps the selection mask rather than any layer.
    fn begin_transform_selection(&mut self) {
        if self.ft.is_some() {
            self.ft_commit();
            return;
        }
        let Some(sel) = self.sel.clone() else {
            self.status = "Transform Selection needs a selection".into();
            return;
        };
        let Some((bx, by, bw, bh)) = sel.bounds() else {
            self.status = "Transform Selection needs a selection".into();
            return;
        };
        self.ft = Some(FtState {
            origin: self.doc.clone(),
            targets: Vec::new(),
            bounds: (
                bx as f32,
                by as f32,
                (bx + bw as i32) as f32,
                (by + bh as i32) as f32,
            ),
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
            quad: None,
            start_quad: None,
            sel_mode: true,
            origin_sel: Some(sel),
        });
        self.status = "Transform Selection: drag handles — Enter applies, Esc cancels".into();
    }

    /// Re-derive the document from the FT origin + current params.
    fn ft_apply(&mut self) {
        let Some(ft) = &self.ft else { return };
        let (sx, sy, rot, skew, dx, dy) = (ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy);
        let targets = ft.targets.clone();
        // Transform Selection: warp the mask, leave the doc alone.
        if ft.sel_mode {
            if let Some(origin_sel) = &ft.origin_sel {
                let corners = Self::ft_corners(ft);
                let mut s = origin_sel.clone();
                pictodesk_core::transform::transform_selection_quad(&mut s, corners);
                self.sel = Some(s);
            }
            self.status = "Transform Selection".into();
            self.touch();
            return;
        }
        let identity = (sx - 1.0).abs() < 1e-6
            && (sy - 1.0).abs() < 1e-6
            && rot.abs() < 1e-6
            && skew.abs() < 1e-6;
        self.doc = ft.origin.clone();
        if let Some(quad) = ft.quad {
            for t in &targets {
                pictodesk_core::transform::transform_layer_quad(&mut self.doc, *t, quad);
            }
            self.status = "Free Transform — Distort".into();
            self.touch();
            return;
        }
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

    /// The four transform-box corners under the current params:
    /// the free quad when distorting, else the affine image of
    /// the bounds corners.
    fn ft_corners(ft: &FtState) -> [(f32, f32); 4] {
        if let Some(q) = ft.quad {
            return q;
        }
        let (x0, y0, x1, y1) = ft.bounds;
        [
            Self::ft_point(ft, (x0, y0)),
            Self::ft_point(ft, (x1, y0)),
            Self::ft_point(ft, (x1, y1)),
            Self::ft_point(ft, (x0, y1)),
        ]
    }

    fn ft_commit(&mut self) {
        if let Some(ft) = self.ft.take() {
            if ft.sel_mode {
                self.status = "Transform Selection applied".into();
                self.touch();
                return;
            }
            // Remember the gesture for Edit > Transform Again.
            let (x0, y0, x1, y1) = ft.bounds;
            let base = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
            let rel_quad = ft.quad.map(|q| {
                let mut rel = [(0.0f32, 0.0f32); 4];
                for i in 0..4 {
                    rel[i] = (q[i].0 - base[i].0, q[i].1 - base[i].1);
                }
                rel
            });
            self.last_ft = Some((ft.sx, ft.sy, ft.rot, ft.skew, ft.dx, ft.dy, rel_quad));
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
            if ft.sel_mode {
                self.sel = ft.origin_sel.clone();
                self.status = "Transform Selection cancelled".into();
                self.touch();
                return;
            }
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
        let c = Self::ft_corners(ft);
        let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        [
            c[0],
            c[1],
            c[2],
            c[3],
            mid(c[0], c[1]),
            mid(c[1], c[2]),
            mid(c[2], c[3]),
            mid(c[3], c[0]),
        ]
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
                let primary = self.ctrl_down || self.super_down;
                drag = if i < 4 && (primary || ft.quad.is_some()) {
                    // Corner + Ctrl (or already distorted): free
                    // corner; with Shift it is Perspective.
                    FtDrag::Quad(i, self.shift_down)
                } else if primary {
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
        if matches!(drag, FtDrag::Quad(..)) && ft.quad.is_none() {
            // Enter Distort: freeze the current affine corners as
            // the free quad; linked targets drop out (a quad is
            // fitted to one layer's bounds).
            ft.quad = Some(Self::ft_corners(ft));
            let a = ft.origin.active_layer;
            ft.targets = vec![a];
        }
        ft.start_quad = ft.quad;
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
            FtDrag::Quad(i, perspective) => {
                if let (Some(q), Some(sq)) = (&mut ft.quad, ft.start_quad) {
                    let d = (dp.0 - ft.drag_start_doc.0, dp.1 - ft.drag_start_doc.1);
                    if perspective {
                        // Move along the dominant axis only; the
                        // partner corners converge symmetrically.
                        if d.0.abs() >= d.1.abs() {
                            q[i] = (sq[i].0 + d.0, sq[i].1);
                            let p = i ^ 1;
                            q[p] = (sq[p].0 - d.0, sq[p].1);
                        } else {
                            q[i] = (sq[i].0, sq[i].1 + d.1);
                            let p = i ^ 3;
                            q[p] = (sq[p].0, sq[p].1 - d.1);
                        }
                    } else {
                        q[i] = dp;
                    }
                }
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
                self.hist.set_limit(Some(20));
                self.reset_hist_for_new_doc();
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
        let mut psd_note = String::new();
        let result = if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            std::fs::write(path, format::serialize(&self.doc)).map_err(|e| e.to_string())
        } else if path.extension().map(|e| e == "psd").unwrap_or(false) {
            let (bytes, report) = pictodesk_core::psd::save_psd(&self.doc);
            if !report.warnings.is_empty() {
                psd_note = format!(" — PSD: {}", report.warnings.join("; "));
            }
            std::fs::write(path, bytes).map_err(|e| e.to_string())
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
                self.status = format!("Saved {}{}", path.display(), psd_note);
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
            Action::Undo => {
                if let Some(l) = self.hist.undo(&mut self.doc) {
                    self.status = format!("Undo {l}");
                    self.doc_version += 1;
                    self.dirty = true;
                }
            }
            Action::Redo => {
                if let Some(l) = self.hist.redo(&mut self.doc) {
                    self.status = format!("Redo {l}");
                    self.doc_version += 1;
                    self.dirty = true;
                }
            }
            Action::ToggleHistoryPanel => {
                self.show_history = !self.show_history;
            }
            Action::ToggleInfoPanel => {
                self.show_info = !self.show_info;
            }
            Action::HistSnapshot => {
                self.take_snapshot();
            }
            Action::HistSnapshotMerged => {
                self.take_snapshot_kind(SnapKind::Merged);
            }
            Action::HistSnapshotLayer => {
                let li = self.doc.active_layer;
                self.take_snapshot_kind(SnapKind::Layer(li));
            }
            Action::HistNewDoc => {
                // New Document from State: the viewed state becomes
                // a fresh document state on top of the stack (the
                // one-window equivalent of PS opening it separately).
                let target = self.hist.position();
                if let Some(d) = self.hist.doc_at_state(&self.doc, target) {
                    let mut cmd = DocumentCommand::begin("New Document from State", &self.doc);
                    self.doc = d;
                    cmd.finish(&self.doc);
                    self.hist.push(Box::new(cmd));
                    self.doc_version += 1;
                    self.dirty = true;
                    self.status = "New document from history state".into();
                }
            }
            Action::HistOptions => {
                let mut d = Dialog::new(DialogKind::HistoryOptions, &self.doc);
                d.vals[0] = self.hist.limit().map(|l| l as f32).unwrap_or(100.0);
                d.opt_a = self.hist_auto_snapshot;
                d.opt_b = self.hist.non_linear();
                self.dialog = Some(d);
            }
            Action::EffectsDialog(page) => {
                self.open_effects_dialog(page);
            }
            Action::CopyEffects => {
                let li = self.doc.active_layer;
                match self.doc.layers[li].effects {
                    Some(fx) if fx.any_enabled() => {
                        self.fx_clipboard = Some(fx);
                        self.status = "Effects copied".into();
                    }
                    _ => self.status = "The active layer has no effects to copy".into(),
                }
            }
            Action::PasteEffects => {
                let li = self.doc.active_layer;
                match self.fx_clipboard {
                    Some(fx) => {
                        self.structural("Paste Effects", |doc| {
                            doc.layers[li].effects = Some(fx);
                        });
                        self.status = "Effects pasted".into();
                    }
                    None => self.status = "No effects copied yet".into(),
                }
            }
            Action::PasteEffectsLinked => match self.fx_clipboard {
                Some(fx) => {
                    let targets: Vec<usize> = self
                        .doc
                        .layers
                        .iter()
                        .enumerate()
                        .filter(|(_, l)| l.linked)
                        .map(|(i, _)| i)
                        .collect();
                    if targets.is_empty() {
                        self.status = "No linked layers to paste onto".into();
                    } else {
                        self.structural("Paste Effects to Linked", |doc| {
                            for i in &targets {
                                doc.layers[*i].effects = Some(fx);
                            }
                        });
                        self.status = format!("Effects pasted to {} linked layers", targets.len());
                    }
                }
                None => self.status = "No effects copied yet".into(),
            },
            Action::ClearEffects => {
                let li = self.doc.active_layer;
                if self.doc.layers[li].effects.is_some() {
                    self.structural("Clear Effects", |doc| {
                        doc.layers[li].effects = None;
                    });
                    self.status = "Effects cleared".into();
                }
            }
            Action::CreateEffectLayers => {
                self.create_effect_layers();
            }
            Action::GlobalAngleDialog => {
                let mut d = Dialog::new(DialogKind::GlobalAngle, &self.doc);
                d.vals = vec![self.doc.global_angle as f32];
                self.dialog = Some(d);
            }
            Action::ToggleEffectsVisible => {
                self.doc.effects_suppressed = !self.doc.effects_suppressed;
                self.doc_version += 1;
                self.status = if self.doc.effects_suppressed {
                    "All effects hidden".into()
                } else {
                    "All effects shown".into()
                };
            }
            Action::DefinePattern => {
                let bounds = self.sel.as_ref().and_then(|s| s.bounds()).unwrap_or((
                    0,
                    0,
                    self.doc.width,
                    self.doc.height,
                ));
                let (bx, by, bw, bh) = bounds;
                if bw == 0 || bh == 0 || bw > 1024 || bh > 1024 {
                    self.status = "Define Pattern: select a rectangle up to 1024 px".into();
                } else {
                    let mut px = Vec::with_capacity((bw * bh) as usize);
                    for yy in by..by + bh as i32 {
                        for xx in bx..bx + bw as i32 {
                            px.push(self.doc.render_pixel(xx, yy));
                        }
                    }
                    self.doc.pattern = Some(pictodesk_core::document::PatternData {
                        name: format!("Pattern {bw}x{bh}"),
                        width: bw,
                        height: bh,
                        px,
                    });
                    self.doc_version += 1;
                    self.dirty = true;
                    self.status = format!("Pattern defined ({bw}x{bh})");
                }
            }
            Action::Reselect => match self.doc.last_selection.clone() {
                Some(s) => {
                    self.sel = Some(s);
                    self.status = "Selection restored".into();
                }
                None => self.status = "No previous selection to restore".into(),
            },
            Action::TransformSelection => {
                self.begin_transform_selection();
            }
            Action::TransformAgain => {
                self.transform_again();
            }
            Action::FtNumericDialog => {
                if let Some(ft) = &self.ft {
                    let mut d = Dialog::new(DialogKind::FtNumeric, &self.doc);
                    d.vals = vec![ft.sx * 100.0, ft.sy * 100.0, ft.rot, ft.skew, ft.dx, ft.dy];
                    self.dialog = Some(d);
                } else {
                    self.status = "Numeric entry needs an active Free Transform".into();
                }
            }
            Action::AlignLinked(mode) => {
                self.align_linked(mode);
            }
            Action::DistributeLinked(mode) => {
                self.distribute_linked(mode);
            }
            Action::ColorSettingsDialog => {
                let mut d = Dialog::new(DialogKind::ColorSettings, &self.doc);
                d.cb_range = match self.doc.working_space {
                    pictodesk_core::color::WorkingSpace::Srgb => 0,
                    pictodesk_core::color::WorkingSpace::AdobeRgb => 1,
                    pictodesk_core::color::WorkingSpace::ProPhoto => 2,
                };
                self.dialog = Some(d);
            }
            Action::AssignProfile(i) => {
                let space = [
                    pictodesk_core::color::WorkingSpace::Srgb,
                    pictodesk_core::color::WorkingSpace::AdobeRgb,
                    pictodesk_core::color::WorkingSpace::ProPhoto,
                ][i];
                self.structural("Assign Profile", |doc| {
                    doc.working_space = space;
                });
                self.status = format!("Profile assigned: {}", space_name(space));
            }
            Action::ConvertProfile(i) => {
                let space = [
                    pictodesk_core::color::WorkingSpace::Srgb,
                    pictodesk_core::color::WorkingSpace::AdobeRgb,
                    pictodesk_core::color::WorkingSpace::ProPhoto,
                ][i];
                self.structural("Convert to Profile", |doc| {
                    pictodesk_core::mode::convert_working_space(doc, space);
                });
                self.status = format!("Converted to {}", space_name(space));
            }
            Action::ToggleProof => {
                self.proof_cmyk = !self.proof_cmyk;
                self.doc_version += 1;
                self.status = if self.proof_cmyk {
                    "Proof Colors: CMYK soft proof on".into()
                } else {
                    "Proof Colors off".into()
                };
            }
            Action::ChannelMixerDialog => {
                self.open_dialog(DialogKind::ChannelMixer);
            }
            Action::FitImageDialog => {
                self.open_dialog(DialogKind::FitImage);
            }
            Action::CondModeDialog => {
                self.open_dialog(DialogKind::CondMode);
            }
            Action::ContactSheetDialog => {
                let mut d = Dialog::new(DialogKind::ContactSheet, &self.doc);
                d.text = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::PicturePackageDialog => {
                self.open_dialog(DialogKind::PicturePackage);
            }
            Action::WebGalleryDialog => {
                let mut d = Dialog::new(DialogKind::WebGallery, &self.doc);
                d.text = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::PageSetupDialog => {
                let mut d = Dialog::new(DialogKind::PageSetup, &self.doc);
                d.vals = vec![self.page_w, self.page_h, self.page_scale];
                d.opt_a = self.page_landscape;
                self.dialog = Some(d);
            }
            Action::PrintDialog => {
                let mut d = Dialog::new(DialogKind::Print, &self.doc);
                d.vals = vec![1.0];
                self.dialog = Some(d);
            }
            Action::SaveForWebDialog => {
                self.open_dialog(DialogKind::SaveForWeb);
            }
            Action::ExportPsd => {
                self.export_psd();
            }
            Action::ExportDcs => {
                self.export_dcs();
            }
            Action::LegacyFilterDialog(id) => {
                self.open_dialog(DialogKind::LegacyFilter(id));
            }
            Action::FilterTransform3d => {
                self.open_dialog(DialogKind::Transform3d);
            }
            Action::FilterDiffuse(mode) => {
                self.filter_now("Diffuse", Some(RecordedStep::Diffuse(mode)), |d, l, s| {
                    filter::diffuse(d, l, s, mode, 12345)
                });
            }
            Action::FilterMezzotint(kind) => {
                self.filter_now(
                    "Mezzotint",
                    Some(RecordedStep::Mezzotint(kind)),
                    |d, l, s| filter::mezzotint(d, l, s, kind, 12345),
                );
            }
            Action::FilterPolar(to_polar) => {
                self.filter_now(
                    "Polar Coordinates",
                    Some(RecordedStep::Polar(to_polar)),
                    |d, l, s| filter::polar_coordinates(d, l, s, to_polar),
                );
            }
            Action::FilterSpecial(which) => {
                let (label, step, f): (&str, RecordedStep, FilterFn) = match which {
                    0 => (
                        "Difference Clouds",
                        RecordedStep::DifferenceClouds,
                        |d, l, s| filter::difference_clouds(d, l, s, 12345),
                    ),
                    1 => ("Sharpen Edges", RecordedStep::SharpenEdges, |d, l, s| {
                        filter::sharpen_edges(d, l, s)
                    }),
                    2 => ("Fragment", RecordedStep::Fragment, |d, l, s| {
                        filter::fragment(d, l, s)
                    }),
                    _ => ("Facet", RecordedStep::Facet, |d, l, s| {
                        filter::facet(d, l, s)
                    }),
                };
                self.filter_now(label, Some(step), f);
            }
            Action::ExtractDialog => {
                let mut d = Dialog::new(DialogKind::Extract, &self.doc);
                let n = (self.doc.width * self.doc.height) as usize;
                d.extract_hi = vec![0; n];
                d.extract_fill = vec![0; n];
                self.dialog = Some(d);
                self.extract_preview_cache = None;
            }
            Action::HistTrash => match self.hist_selected {
                Some(HistRow::Snapshot(i)) if i < self.snapshots.len() => {
                    self.snapshots.remove(i);
                    if self.hist_source == HistSource::Snapshot(i) {
                        self.hist_source = HistSource::State(0);
                    } else if let HistSource::Snapshot(s) = self.hist_source {
                        if s > i {
                            self.hist_source = HistSource::Snapshot(s - 1);
                        }
                    }
                    self.hist_selected = None;
                    self.status = "Snapshot deleted".into();
                }
                _ => {
                    self.hist.purge();
                    self.hist_selected = None;
                    self.status = "History cleared".into();
                }
            },
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
            Action::CopyMerged => {
                // Like Copy, but the block comes from the flattened
                // composite of every visible layer.
                let (x, y, w, h) = self.sel.as_ref().and_then(|s| s.bounds()).unwrap_or((
                    0,
                    0,
                    self.doc.width,
                    self.doc.height,
                ));
                let mut block = Vec::with_capacity((w * h) as usize);
                for yy in 0..h as i32 {
                    for xx in 0..w as i32 {
                        block.push(self.doc.render_pixel(x + xx, y + yy));
                    }
                }
                CLIPBOARD.with(|c| *c.borrow_mut() = Some((block, w, h)));
                self.status = "Copied Merged".into();
            }
            Action::PasteInto => {
                // Paste the clipboard into the selection: the block
                // lands at the selection's origin and only shows
                // where the selection is active (feathered edges
                // blend by the mask value).
                let clip = CLIPBOARD.with(|c| c.borrow().clone());
                if let (Some((block, w, h)), Some(sel)) = (clip, self.sel.clone()) {
                    if let Some((ox, oy, _, _)) = sel.bounds() {
                        let tiles = Selection::tiles_in_rect(ox, oy, w, h);
                        run_tile_edit(
                            &mut self.doc,
                            &mut self.hist,
                            "Paste Into",
                            layer,
                            &tiles,
                            |d| {
                                for yy in 0..h as i32 {
                                    for xx in 0..w as i32 {
                                        let p = block[(yy as u32 * w + xx as u32) as usize];
                                        if p.a == 0 {
                                            continue;
                                        }
                                        let (dx, dy) = (ox + xx, oy + yy);
                                        let strength = sel.value(dx, dy);
                                        if strength == 0 {
                                            continue;
                                        }
                                        if strength == 255 {
                                            d.layers[layer].set_pixel(dx, dy, p);
                                        } else {
                                            let orig = d.layers[layer].get_pixel(dx, dy);
                                            let t = strength as f32 / 255.0;
                                            let l = |a: u16, b: u16| {
                                                (a as f32 + (b as f32 - a as f32) * t + 0.5) as u16
                                            };
                                            d.layers[layer].set_pixel(
                                                dx,
                                                dy,
                                                Pixel16::new(
                                                    l(orig.r, p.r),
                                                    l(orig.g, p.g),
                                                    l(orig.b, p.b),
                                                    l(orig.a, p.a),
                                                ),
                                            );
                                        }
                                    }
                                }
                            },
                        );
                        self.last_label = Some("Paste Into".into());
                        self.status = "Pasted Into".into();
                        self.touch();
                    }
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
                    self.record_step(RecordedStep::CropToSelection);
                    self.sel = None;
                    self.fit_view(self.win_size.0, self.win_size.1);
                }
            }
            Action::Rotate180 => {
                self.structural("Rotate 180", transform::rotate_180);
                self.record_step(RecordedStep::Rotate180);
            }
            Action::Rotate90Cw => {
                self.structural("Rotate 90 CW", transform::rotate_90_cw);
                self.record_step(RecordedStep::Rotate90Cw);
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::Rotate90Ccw => {
                self.structural("Rotate 90 CCW", transform::rotate_90_ccw);
                self.record_step(RecordedStep::Rotate90Ccw);
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::FlipHorizontal => {
                self.structural("Flip Horizontal", transform::flip_horizontal);
                self.record_step(RecordedStep::FlipH);
            }
            Action::FlipVertical => {
                self.structural("Flip Vertical", transform::flip_vertical);
                self.record_step(RecordedStep::FlipV);
            }
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
                    self.record_step(RecordedStep::Invert);
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
                    self.record_step(RecordedStep::Desaturate);
                    self.touch();
                }
                "photo" => self.open_dialog(DialogKind::PhotoFilter),
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
                self.record_step(RecordedStep::LayerDuplicate);
                self.status = "Layer duplicated".into();
            }
            Action::DeleteLayer => {
                let a = self.doc.active_layer;
                if self.doc.layers.len() > 1 {
                    self.structural("Delete Layer", |d| {
                        d.delete_layer(a);
                    });
                    self.record_step(RecordedStep::LayerDelete);
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
            Action::LayerToFront => {
                let a = self.doc.active_layer;
                let top = self.doc.layers.len() - 1;
                if a < top {
                    self.structural("Bring to Front", |d| {
                        d.move_layer(a, top);
                    });
                }
            }
            Action::LayerToBack => {
                let a = self.doc.active_layer;
                if a > 0 {
                    self.structural("Send to Back", |d| {
                        d.move_layer(a, 0);
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
                self.record_step(RecordedStep::Flatten);
                self.status = "Image flattened".into();
            }
            Action::NewAdjustment(kind) => {
                self.open_adjustment_dialog(kind, None);
            }
            Action::EditAdjustment => {
                let a = self.doc.active_layer;
                if self.doc.layers[a].adjustment.is_some() {
                    let kind = match &self.doc.layers[a].adjustment {
                        Some(pictodesk_core::adjust::Adjustment::Levels { .. }) => "levels",
                        Some(pictodesk_core::adjust::Adjustment::Curves { .. }) => "curves",
                        Some(pictodesk_core::adjust::Adjustment::BrightnessContrast { .. }) => {
                            "brightness"
                        }
                        Some(pictodesk_core::adjust::Adjustment::HueSaturation { .. }) => "hue",
                        Some(pictodesk_core::adjust::Adjustment::ColorBalance { .. }) => "balance",
                        Some(pictodesk_core::adjust::Adjustment::PhotoFilter { .. }) => "photo",
                        None => "levels",
                    };
                    self.open_adjustment_dialog(kind, Some(a));
                } else {
                    self.status = "The active layer is not an adjustment layer".into();
                }
            }
            Action::AddMaskReveal | Action::AddMaskHide | Action::AddMaskSelection => {
                let a = self.doc.active_layer;
                if self.doc.layers[a].adjustment.is_some() && self.doc.layers[a].mask.is_some() {
                    self.status = "That layer already has a mask".into();
                    return;
                }
                let fill = if action == Action::AddMaskHide {
                    0
                } else {
                    255
                };
                let from_sel = action == Action::AddMaskSelection;
                let sel = self.sel.clone();
                let label = if from_sel {
                    "Add Mask from Selection"
                } else if fill == 0 {
                    "Add Mask (Hide All)"
                } else {
                    "Add Mask (Reveal All)"
                };
                self.structural(label, |d| {
                    d.layers[a].add_mask(fill);
                    if from_sel {
                        if let Some(s) = &sel {
                            for y in 0..d.height as i32 {
                                for x in 0..d.width as i32 {
                                    d.layers[a].set_mask_value(x, y, s.value(x, y));
                                }
                            }
                        }
                    }
                });
                self.paint_target = PaintTarget::Mask(a);
                self.channel_view = ChannelView::Mask(a);
                self.status = "Mask added — paint on it with the brush".into();
            }
            Action::DeleteMask => {
                let a = self.doc.active_layer;
                self.structural("Delete Mask", |d| {
                    d.layers[a].mask = None;
                });
                if self.paint_target == PaintTarget::Mask(a) {
                    self.paint_target = PaintTarget::Image;
                    self.channel_view = ChannelView::Composite;
                }
                self.status = "Mask deleted".into();
            }
            Action::ToggleMaskEnabled => {
                let a = self.doc.active_layer;
                self.structural("Toggle Mask", |d| {
                    d.layers[a].mask_enabled = !d.layers[a].mask_enabled;
                });
            }
            Action::FeatherMask => {
                self.open_dialog(DialogKind::FeatherMask);
            }
            Action::ToggleNavigator => {
                self.show_navigator = !self.show_navigator;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ToggleActionsPanel => {
                self.show_actions = !self.show_actions;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::ToggleGuides => {
                self.show_guides = !self.show_guides;
            }
            Action::ToggleGrid => {
                self.show_grid = !self.show_grid;
            }
            Action::ToggleSnap => {
                self.snap_enabled = !self.snap_enabled;
            }
            Action::ClearGuides => {
                self.structural("Clear Guides", |d| {
                    d.guides_x.clear();
                    d.guides_y.clear();
                });
                self.status = "Guides cleared".into();
            }
            Action::MacroNew => {
                let n = self.actions.len() + 1;
                self.actions.push(RecordedAction {
                    name: format!("Action {n}"),
                    steps: Vec::new(),
                    set: self.action_set.clone(),
                });
                self.active_action = self.actions.len() - 1;
                self.status = "New action — press Record, then work".into();
            }
            Action::MacroRecord => {
                if self.actions.is_empty() {
                    self.do_action(Action::MacroNew);
                }
                self.recording = true;
                self.status =
                    "Recording… every adjustment, filter and transform is captured".into();
            }
            Action::MacroStop => {
                self.recording = false;
                self.status = "Recording stopped".into();
            }
            Action::MacroPlay => {
                self.play_action(self.active_action);
            }
            Action::MacroDelete => {
                if !self.actions.is_empty() {
                    self.actions.remove(self.active_action);
                    if self.active_action >= self.actions.len() && self.active_action > 0 {
                        self.active_action -= 1;
                    }
                    self.recording = false;
                }
            }
            Action::MacroBatch => {
                if self.actions.is_empty() {
                    self.status = "Record an action first, then batch it".into();
                } else {
                    let mut d = Dialog::new(DialogKind::Batch, &self.doc);
                    d.text = String::new();
                    d.text_focus = true;
                    self.dialog = Some(d);
                }
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
                self.record_step(RecordedStep::Sharpen);
                self.last_label = Some("Sharpen".into());
                self.status = "Sharpen".into();
                self.touch();
            }
            Action::FilterMotion => self.open_dialog(DialogKind::MotionBlur),
            Action::FilterRadial => self.open_dialog(DialogKind::RadialBlur),
            Action::FilterMedian => self.open_dialog(DialogKind::MedianFilter),
            Action::FilterEmboss => self.open_dialog(DialogKind::Emboss),
            Action::FilterMosaic => self.open_dialog(DialogKind::Mosaic),
            Action::FilterHighPass => self.open_dialog(DialogKind::HighPass),
            Action::FilterOffset => self.open_dialog(DialogKind::OffsetFilter),
            Action::FilterCustom => self.open_dialog(DialogKind::CustomFilter),
            Action::FilterDespeckle => {
                self.apply_filter_now("Despeckle", RecordedStep::Despeckle, |d, l, s| {
                    filter::despeckle(d, l, s)
                });
            }
            Action::FilterFindEdges => {
                self.apply_filter_now("Find Edges", RecordedStep::FindEdges, |d, l, s| {
                    filter::find_edges(d, l, s)
                });
            }
            Action::FilterSolarize => {
                self.apply_filter_now("Solarize", RecordedStep::Solarize, |d, l, s| {
                    filter::solarize(d, l, s)
                });
            }
            Action::FilterClouds => {
                let (fg, bg) = (self.fg, self.bg);
                self.apply_filter_now("Clouds", RecordedStep::Clouds(fg, bg), move |d, l, s| {
                    filter::clouds(d, l, s, fg, bg, 12345)
                });
            }
            Action::AdjustEqualize => {
                self.apply_filter_now("Equalize", RecordedStep::Equalize, |d, l, s| {
                    adjust::equalize(d, l, s)
                });
            }
            Action::SelectColorRange => self.open_dialog(DialogKind::ColorRange),
            Action::ModifyExpand => self.open_modify_dialog(0),
            Action::ModifyContract => self.open_modify_dialog(1),
            Action::ModifySmooth => self.open_modify_dialog(2),
            Action::ModifyBorder => self.open_modify_dialog(3),
            Action::SelectAll => {
                self.replace_sel(Some(Selection::all(self.doc.width, self.doc.height)));
                self.status = "Select All".into();
            }
            Action::SelectNone => {
                self.replace_sel(None);
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
        // Apply the default values as the first preview, so OK
        // commits them even if the user never touches a slider
        // (before this, OK on an untouched dialog was a no-op).
        self.dialog_refresh_preview();
    }

    /// Re-run the dialog's effect from the snapshot (live preview).
    /// Save for Web: recompute the encoded size for the current
    /// format + quality.
    fn refresh_web_size(&mut self) {
        let (fmt, q) = match &self.dialog {
            Some(d) if d.kind == DialogKind::SaveForWeb => {
                let fmt = match d.web_format {
                    1 => ExportFormat::Png,
                    2 => ExportFormat::Gif,
                    _ => ExportFormat::Jpeg,
                };
                (fmt, d.vals[0] as u8)
            }
            _ => return,
        };
        if let Ok(bytes) = io::export_image(&self.doc, fmt, q) {
            if let Some(d) = &mut self.dialog {
                d.web_size = bytes.len();
            }
        }
    }

    fn dialog_refresh_preview(&mut self) {
        // Numeric Free Transform: sliders drive the live FT state.
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::FtNumeric)
        ) {
            let v = self.dialog.as_ref().unwrap().vals.clone();
            if let Some(ft) = &mut self.ft {
                ft.sx = v[0] / 100.0;
                ft.sy = v[1] / 100.0;
                ft.rot = v[2];
                ft.skew = v[3];
                ft.dx = v[4];
                ft.dy = v[5];
                ft.quad = None;
            }
            self.ft_apply();
            return;
        }
        // Save for Web tracks its encoded size live.
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::SaveForWeb)
        ) {
            self.refresh_web_size();
            return;
        }
        // The Effects dialog borrows adj_layer/adj_snapshot for
        // its commit/cancel plumbing but is not an adjustment.
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::Effects)
        ) {
            self.fx_sync_from_vals();
            return;
        }
        let Some(d) = &mut self.dialog else { return };
        if let Some(li) = d.adj_layer {
            // Adjustment-layer dialog: write the parameters straight
            // into the layer; the compositor does the rest live.
            let adj = Self::adjustment_from_dialog(d);
            self.doc.layers[li].adjustment = Some(adj);
            self.doc_version += 1;
            return;
        }
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
        let kern = self.dialog.as_ref().unwrap().kernel;
        let kscale = self.dialog.as_ref().unwrap().kernel_scale;
        let koffset = self.dialog.as_ref().unwrap().kernel_offset;
        let hs_range = self.dialog.as_ref().unwrap().hs_range;
        let shape3d = self.dialog.as_ref().unwrap().shape3d;
        let mut mixer = self.dialog.as_ref().unwrap().mixer;
        let mix_out = self.dialog.as_ref().unwrap().mix_out;
        if kind == DialogKind::ChannelMixer {
            // Stash the sliders into the mixer's current row.
            mixer.rows[mix_out] = [v[0] / 100.0, v[1] / 100.0, v[2] / 100.0, v[3] / 100.0];
            mixer.constant[mix_out] = v[4] / 100.0;
        }
        match kind {
            DialogKind::BrightnessContrast => {
                adjust::brightness_contrast(&mut self.doc, layer, s, v[0] as i32, v[1] as i32)
            }
            DialogKind::HueSaturation => {
                if hs_range == 0 {
                    adjust::hue_saturation(
                        &mut self.doc,
                        layer,
                        s,
                        v[0] as i32,
                        v[1] as i32,
                        v[2] as i32,
                    )
                } else {
                    let centers = [0.0f32, 0.0, 60.0, 120.0, 180.0, 240.0, 300.0];
                    adjust::hue_saturation_range(
                        &mut self.doc,
                        layer,
                        s,
                        v[0] as i32,
                        v[1] as i32,
                        v[2] as i32,
                        centers[hs_range],
                        60.0,
                    )
                }
            }
            DialogKind::ChannelMixer => adjust::channel_mixer(&mut self.doc, layer, s, &mixer),
            DialogKind::Transform3d => {
                filter::transform_3d(&mut self.doc, layer, s, shape3d, v[0], v[1])
            }
            DialogKind::LegacyFilter(id) => {
                let d = self.dialog.as_ref().unwrap();
                let (fx_a, fx_b, fg, bg) = (d.fx_a, d.fx_b, self.fg, self.bg);
                match id {
                    0 => filter::pinch(&mut self.doc, layer, s, v[0] / 100.0),
                    1 => filter::spherize(&mut self.doc, layer, s, v[0] / 100.0),
                    2 => filter::twirl(&mut self.doc, layer, s, v[0]),
                    3 => filter::ripple(&mut self.doc, layer, s, v[0], v[1]),
                    4 => filter::ocean_ripple(&mut self.doc, layer, s, v[0], v[1]),
                    5 => filter::shear(&mut self.doc, layer, s, [v[0], v[1], v[2], v[3], v[4]]),
                    6 => filter::zigzag(&mut self.doc, layer, s, v[0], v[1] as u32, fx_a),
                    7 => filter::maximum(&mut self.doc, layer, s, v[0] as u32),
                    8 => filter::minimum(&mut self.doc, layer, s, v[0] as u32),
                    9 => filter::dust_and_scratches(&mut self.doc, layer, s, v[0] as u32, v[1]),
                    10 => filter::tiles(&mut self.doc, layer, s, v[0] as u32, v[1], 12345),
                    11 => filter::glowing_edges(
                        &mut self.doc,
                        layer,
                        s,
                        v[0] as u32,
                        v[1],
                        v[2] as u32,
                    ),
                    12 => filter::patchwork(&mut self.doc, layer, s, v[0] as u32, v[1] as u32),
                    13 => filter::stamp_effect(&mut self.doc, layer, s, v[0], v[1] as u32),
                    14 => filter::bas_relief(
                        &mut self.doc,
                        layer,
                        s,
                        fg,
                        bg,
                        v[0] as u32,
                        v[1] as u32,
                    ),
                    15 => filter::wind(&mut self.doc, layer, s, fx_a, fx_b, 12345),
                    _ => {}
                }
            }
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
            DialogKind::PhotoFilter => {
                let c = self.dialog.as_ref().unwrap().photo_color;
                adjust::photo_filter(&mut self.doc, layer, s, c, v[0] as u8)
            }
            DialogKind::MotionBlur => filter::motion_blur(&mut self.doc, layer, s, v[0], v[1]),
            DialogKind::RadialBlur => filter::radial_blur(&mut self.doc, layer, s, v[0]),
            DialogKind::MedianFilter => filter::median(&mut self.doc, layer, s, v[0] as u32),
            DialogKind::Emboss => filter::emboss(&mut self.doc, layer, s, v[0] / 100.0),
            DialogKind::Mosaic => filter::mosaic(&mut self.doc, layer, s, v[0] as u32),
            DialogKind::HighPass => filter::high_pass(&mut self.doc, layer, s, v[0]),
            DialogKind::OffsetFilter => {
                filter::offset_filter(&mut self.doc, layer, s, v[0] as i32, v[1] as i32)
            }
            DialogKind::CustomFilter => {
                let k: [f32; 25] = kern.map(|x| x as f32);
                filter::custom_filter(
                    &mut self.doc,
                    layer,
                    s,
                    k,
                    kscale as f32,
                    koffset as f32 / 255.0,
                )
            }
            _ => {}
        }
        self.doc_version += 1;
    }

    fn dialog_ok(&mut self) {
        let Some(mut d) = self.dialog.take() else {
            return;
        };
        let layer = self.doc.active_layer;
        // Adjustment-layer dialogs commit as one document step built
        // from the snapshot taken when the dialog opened.
        if let (Some(snap), Some(_li)) = (d.adj_snapshot.take(), d.adj_layer) {
            let created = snap.layers.len() != self.doc.layers.len();
            let is_fx = d.kind == DialogKind::Effects;
            let label = if is_fx {
                "Layer Effects"
            } else if created {
                "New Adjustment Layer"
            } else {
                "Edit Adjustment"
            };
            let mut cmd = DocumentCommand::begin(label, &snap);
            cmd.finish(&self.doc);
            self.push_cmd(Box::new(cmd));
            self.last_label = Some(label.into());
            self.status = label.into();
            if is_fx {
                if let Some(fx) = self.doc.layers[self.doc.active_layer].effects {
                    self.record_step(RecordedStep::SetEffects(fx));
                }
            } else if created {
                if let Some(adj) = self.doc.layers[self.doc.active_layer].adjustment.clone() {
                    self.record_step(RecordedStep::AdjustmentLayer(adj));
                }
            }
            self.touch();
            return;
        }
        // The step this dialog represents, if an Action is recording
        // (computed before the match moves d.kind).
        let record_candidate = match d.kind {
            DialogKind::BrightnessContrast
            | DialogKind::HueSaturation
            | DialogKind::ColorBalance
            | DialogKind::Levels
            | DialogKind::Curves
            | DialogKind::PhotoFilter => {
                Some(RecordedStep::Adjustment(Self::adjustment_from_dialog(&d)))
            }
            DialogKind::GaussianBlur => Some(RecordedStep::GaussianBlur(d.vals[0])),
            DialogKind::Unsharp => {
                Some(RecordedStep::Unsharp(d.vals[0], d.vals[1], d.vals[2] as u8))
            }
            DialogKind::AddNoise => Some(RecordedStep::AddNoise(d.vals[0])),
            DialogKind::Posterize => Some(RecordedStep::Posterize(d.vals[0] as u8)),
            DialogKind::Threshold => Some(RecordedStep::Threshold(d.vals[0] as u8)),
            DialogKind::MotionBlur => Some(RecordedStep::MotionBlur(d.vals[0], d.vals[1])),
            DialogKind::RadialBlur => Some(RecordedStep::RadialBlur(d.vals[0])),
            DialogKind::MedianFilter => Some(RecordedStep::Median(d.vals[0] as u32)),
            DialogKind::Emboss => Some(RecordedStep::Emboss(d.vals[0] / 100.0)),
            DialogKind::Mosaic => Some(RecordedStep::Mosaic(d.vals[0] as u32)),
            DialogKind::HighPass => Some(RecordedStep::HighPass(d.vals[0])),
            DialogKind::OffsetFilter => {
                Some(RecordedStep::OffsetPx(d.vals[0] as i32, d.vals[1] as i32))
            }
            DialogKind::CustomFilter => Some(RecordedStep::CustomKernel(
                d.kernel,
                d.kernel_scale,
                d.kernel_offset,
            )),
            DialogKind::ChannelMixer => {
                let mut m = d.mixer;
                m.rows[d.mix_out] = [
                    d.vals[0] / 100.0,
                    d.vals[1] / 100.0,
                    d.vals[2] / 100.0,
                    d.vals[3] / 100.0,
                ];
                m.constant[d.mix_out] = d.vals[4] / 100.0;
                Some(RecordedStep::ChannelMixer(m))
            }
            DialogKind::Transform3d => {
                Some(RecordedStep::Transform3d(d.shape3d, d.vals[0], d.vals[1]))
            }
            DialogKind::LegacyFilter(id) => Some(match id {
                0 => RecordedStep::Pinch(d.vals[0] / 100.0),
                1 => RecordedStep::Spherize(d.vals[0] / 100.0),
                2 => RecordedStep::Twirl(d.vals[0]),
                3 => RecordedStep::Ripple(d.vals[0], d.vals[1]),
                4 => RecordedStep::OceanRipple(d.vals[0], d.vals[1]),
                5 => RecordedStep::Shear([d.vals[0], d.vals[1], d.vals[2], d.vals[3], d.vals[4]]),
                6 => RecordedStep::ZigZag(d.vals[0], d.vals[1] as u32, d.fx_a),
                7 => RecordedStep::Maximum(d.vals[0] as u32),
                8 => RecordedStep::Minimum(d.vals[0] as u32),
                9 => RecordedStep::DustScratches(d.vals[0] as u32, d.vals[1]),
                10 => RecordedStep::Tiles(d.vals[0] as u32, d.vals[1]),
                11 => RecordedStep::GlowingEdges(d.vals[0] as u32, d.vals[1], d.vals[2] as u32),
                12 => RecordedStep::Patchwork(d.vals[0] as u32, d.vals[1] as u32),
                13 => RecordedStep::StampEffect(d.vals[0], d.vals[1] as u32),
                14 => RecordedStep::BasRelief(self.fg, self.bg, d.vals[0] as u32, d.vals[1] as u32),
                15 => RecordedStep::Wind(d.fx_a, d.fx_b),
                _ => RecordedStep::Sharpen,
            }),
            _ => None,
        };
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
                if let Some(step) = record_candidate.clone() {
                    self.record_step(step);
                }
                self.touch();
            }
            DialogKind::ImageSize => {
                let (w, h) = (
                    d.vals[0].round().max(1.0) as u32,
                    d.vals[1].round().max(1.0) as u32,
                );
                self.structural("Image Size", |doc| transform::resize_image(doc, w, h));
                self.record_step(RecordedStep::ImageSize(w, h));
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
                self.record_step(RecordedStep::CanvasSize(w, h, anchor));
                self.sel = None;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            DialogKind::FeatherMask => {
                let a = self.doc.active_layer;
                let radius = d.vals[0].round() as u32;
                if self.doc.layers[a].mask.is_some() {
                    self.structural("Feather Mask", |doc| {
                        doc.feather_mask(a, radius);
                    });
                    self.status = format!("Mask feathered by {radius} px");
                } else {
                    self.status = "The active layer has no mask to feather".into();
                }
            }
            DialogKind::Batch => {
                self.run_batch(&d.text);
            }
            DialogKind::Feather => {
                if let Some(s) = &mut self.sel {
                    s.feather(d.vals[0].round() as u32);
                    self.status = format!("Feathered by {} px", d.vals[0].round() as u32);
                }
            }
            DialogKind::ColorRange => {
                // Samples the foreground color (set it with the
                // Eyedropper first); fuzziness widens the match.
                let (r, g, b, _) = self.fg.to_rgba8();
                let fuzz = d.vals[0].round().clamp(0.0, 200.0) as u8;
                let cr = Selection::color_range(&self.doc, (r, g, b), fuzz);
                self.replace_sel(Some(cr));
                self.status = format!("Color Range: foreground ({r}, {g}, {b}), fuzziness {fuzz}");
            }
            DialogKind::ModifySel => {
                if let Some(s) = &mut self.sel {
                    let n = d.vals[0].round().max(1.0) as u32;
                    let label = match d.modify_op {
                        0 => {
                            s.expand(n);
                            "Expand Selection"
                        }
                        1 => {
                            s.contract(n);
                            "Contract Selection"
                        }
                        2 => {
                            s.smooth(n);
                            "Smooth Selection"
                        }
                        _ => {
                            s.border(n);
                            "Border Selection"
                        }
                    };
                    self.status = format!("{label} by {n} px");
                }
            }
            DialogKind::Fill => {
                if let PaintTarget::Mask(li) = self.paint_target {
                    let sel = self
                        .sel
                        .clone()
                        .unwrap_or_else(|| Selection::all(self.doc.width, self.doc.height));
                    let src = if d.fill_bg { self.bg } else { self.fg };
                    let (r, g, b, _) = src.to_rgba8();
                    let v = ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8;
                    let op = d.vals[0] / 100.0;
                    self.structural("Fill", |doc| {
                        for yy in 0..doc.height as i32 {
                            for xx in 0..doc.width as i32 {
                                let m = sel.value(xx, yy) as f32 / 255.0 * op;
                                if m > 0.0 {
                                    let cur = doc.layers[li].mask_value(xx, yy) as f32;
                                    doc.layers[li].set_mask_value(
                                        xx,
                                        yy,
                                        (cur + (v as f32 - cur) * m).round() as u8,
                                    );
                                }
                            }
                        }
                    });
                    self.last_label = Some("Fill".into());
                } else if let PaintTarget::Alpha(a) = self.paint_target {
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
                    if d.fill_history {
                        // Fill with History: paint the source
                        // state's flattened image into the selection.
                        if let Some(src_doc) = self.hist_source_doc() {
                            let op = d.vals[0] / 100.0;
                            if let Some((x, y, w, h)) = sel.bounds() {
                                let tiles = Selection::tiles_in_rect(x, y, w, h);
                                run_tile_edit(
                                    &mut self.doc,
                                    &mut self.hist,
                                    "Fill with History",
                                    layer,
                                    &tiles,
                                    |doc| {
                                        brush::fill_from_document(doc, layer, &sel, &src_doc, op);
                                    },
                                );
                                self.last_label = Some("Fill with History".into());
                                self.status = "Filled with the history source".into();
                                self.touch();
                            }
                        } else {
                            self.status = "No history source available".into();
                        }
                    } else {
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
            }
            DialogKind::Type => {
                let Some(mut spec) = d.type_spec.take() else {
                    return;
                };
                spec.text = d.text.clone();
                if spec.text.trim().is_empty() && d.type_layer.is_none() {
                    self.status = "Type: no text entered".into();
                    return;
                }
                // Sliders: size/tracking onto the target range (or
                // the whole text), leading is spec-wide.
                let (from, to) = (
                    d.vals[3].round().max(0.0) as usize,
                    d.vals[4].round().max(0.0) as usize,
                );
                let (size, tracking) = (d.vals[0], d.vals[1]);
                if from < to && to <= spec.text.len() {
                    let mut st = spec.style_at(from);
                    st.size = size;
                    st.tracking = tracking;
                    spec.set_style(from, to, st);
                } else {
                    spec.default_style.size = size;
                    spec.default_style.tracking = tracking;
                    for r in &mut spec.runs {
                        r.style.size = size;
                        r.style.tracking = tracking;
                    }
                }
                spec.leading = d.vals[2];
                if d.type_mask {
                    // Type Mask: render the text on a scratch layer
                    // and take the selection from its alpha.
                    let mut scratch = Document::new(self.doc.width, self.doc.height);
                    scratch.layers[0].offset = d.type_anchor;
                    scratch.layers[0].text = Some(spec);
                    typo::render_type_layer(&mut scratch, 0);
                    let (w, h) = (self.doc.width, self.doc.height);
                    let mut mask = vec![0u8; (w * h) as usize];
                    for yy in 0..h as i32 {
                        for xx in 0..w as i32 {
                            let a = scratch.layers[0].get_pixel(xx, yy).a;
                            mask[(yy as u32 * w + xx as u32) as usize] = (a >> 8) as u8;
                        }
                    }
                    let sel = Selection::from_mask(w, h, mask);
                    if sel.bounds().is_some() {
                        self.replace_sel(Some(sel));
                        self.status = "Type mask selection".into();
                    } else {
                        self.status = "Type mask: nothing rendered".into();
                    }
                    return;
                }
                match d.type_layer {
                    Some(li) => {
                        self.structural("Edit Type Layer", |doc| {
                            doc.layers[li].text = Some(spec);
                            typo::render_type_layer(doc, li);
                        });
                        self.status = "Type layer updated".into();
                    }
                    None => {
                        let anchor = d.type_anchor;
                        let name = {
                            let first = spec.text.lines().next().unwrap_or("Type");
                            let short: String = first.chars().take(24).collect();
                            if short.is_empty() {
                                "Type".to_string()
                            } else {
                                short
                            }
                        };
                        self.structural("New Type Layer", |doc| {
                            let mut l = pictodesk_core::layer::Layer::new(name);
                            l.offset = anchor;
                            l.text = Some(spec);
                            doc.layers.push(l);
                            let li = doc.layers.len() - 1;
                            doc.active_layer = li;
                            typo::render_type_layer(doc, li);
                        });
                        self.status = "Type layer created".into();
                    }
                }
                self.last_label = Some("Type".into());
                self.touch();
            }
            DialogKind::FtNumeric => {
                self.ft_commit();
            }
            DialogKind::ActionSetName => {
                let name = d.text.trim().to_string();
                if !name.is_empty() && !self.action_sets.contains(&name) {
                    self.action_sets.push(name.clone());
                    self.action_set = name.clone();
                    self.status = format!("Action set '{name}' created");
                }
            }
            DialogKind::FitImage => {
                let (max_w, max_h) = (d.vals[0].max(1.0), d.vals[1].max(1.0));
                let (w, h) = (self.doc.width as f32, self.doc.height as f32);
                let scale = (max_w / w).min(max_h / h).min(1.0);
                let (nw, nh) = (
                    ((w * scale).round() as u32).max(1),
                    ((h * scale).round() as u32).max(1),
                );
                if (nw, nh) != (self.doc.width, self.doc.height) {
                    self.structural("Fit Image", |doc| {
                        pictodesk_core::transform::resize_image(doc, nw, nh);
                    });
                    self.status = format!("Fit Image: {nw}x{nh}");
                } else {
                    self.status = "Fit Image: already inside the box".into();
                }
            }
            DialogKind::CondMode => {
                let from_ok = match d.cond_from {
                    0 => true,
                    1 => self.doc.mode == ColorMode::Rgb,
                    2 => self.doc.mode == ColorMode::Grayscale,
                    _ => self.doc.mode == ColorMode::Cmyk,
                };
                if from_ok {
                    let to = [ColorMode::Rgb, ColorMode::Grayscale, ColorMode::Cmyk]
                        [d.cond_to.min(2) as usize];
                    if to != self.doc.mode {
                        self.structural("Conditional Mode Change", |doc| {
                            pictodesk_core::mode::convert_mode(doc, to);
                        });
                        self.status = "Mode converted".into();
                    } else {
                        self.status = "Already in the target mode".into();
                    }
                } else {
                    self.status = "Mode condition not met — nothing converted".into();
                }
            }
            DialogKind::ContactSheet => {
                let (cols, thumb) = (d.vals[0].round().max(1.0) as usize, d.vals[1] as u32);
                let folder = d.text.clone();
                self.run_contact_sheet(&folder, cols, thumb);
            }
            DialogKind::PicturePackage => {
                let (pw, ph) = (
                    d.vals[0].round().max(50.0) as u32,
                    d.vals[1].round().max(50.0) as u32,
                );
                self.run_picture_package(pw, ph, d.pkg_layout);
            }
            DialogKind::WebGallery => {
                let folder = d.text.clone();
                let thumb = d.vals[0] as u32;
                self.run_web_gallery(&folder, thumb);
            }
            DialogKind::PageSetup => {
                self.page_w = d.vals[0];
                self.page_h = d.vals[1];
                self.page_scale = d.vals[2];
                self.page_landscape = d.opt_a;
                self.status = format!(
                    "Page: {}x{} pt, {}%, {}",
                    self.page_w as i32,
                    self.page_h as i32,
                    self.page_scale as i32,
                    if self.page_landscape {
                        "landscape"
                    } else {
                        "portrait"
                    }
                );
            }
            DialogKind::Print => {
                let copies = d.vals[0].round().max(1.0) as u32;
                self.do_print(copies);
            }
            DialogKind::SaveForWeb => {
                let (fmt, ext) = match d.web_format {
                    1 => (ExportFormat::Png, "png"),
                    2 => (ExportFormat::Gif, "gif"),
                    _ => (ExportFormat::Jpeg, "jpg"),
                };
                let name = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "untitled".into());
                let path = PathBuf::from(format!("{name}-web.{ext}"));
                match io::export_image(&self.doc, fmt, d.vals[0] as u8) {
                    Ok(bytes) => {
                        let n = bytes.len();
                        match std::fs::write(&path, bytes) {
                            Ok(()) => self.status = format!("Saved {} ({n} bytes)", path.display()),
                            Err(e) => self.status = format!("Save for Web failed: {e}"),
                        }
                    }
                    Err(e) => self.status = format!("Save for Web failed: {e}"),
                }
            }
            DialogKind::Extract => {
                let layer = self.doc.active_layer;
                let tiles = self.active_tiles_all();
                let (hi, fill) = (d.extract_hi.clone(), d.extract_fill.clone());
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Extract",
                    layer,
                    &tiles,
                    |doc| {
                        filter::extract(doc, layer, &hi, &fill);
                    },
                );
                self.last_label = Some("Extract".into());
                self.status = "Extract applied".into();
                self.touch();
            }
            DialogKind::GlobalAngle => {
                let a = d.vals[0] as i32;
                self.structural("Global Angle", |doc| {
                    doc.global_angle = a;
                });
                self.status = format!("Global light angle: {a} degrees");
            }
            DialogKind::ColorSettings => {
                let space = [
                    pictodesk_core::color::WorkingSpace::Srgb,
                    pictodesk_core::color::WorkingSpace::AdobeRgb,
                    pictodesk_core::color::WorkingSpace::ProPhoto,
                ][d.cb_range.min(2)];
                if space != self.doc.working_space {
                    self.structural("Color Settings", |doc| {
                        doc.working_space = space;
                    });
                }
                self.status = format!("Working space: {}", space_name(space));
            }
            DialogKind::HistoryOptions => {
                let n = d.vals[0].round().clamp(1.0, 100.0) as usize;
                self.hist.set_limit(Some(n));
                self.hist.set_non_linear(d.opt_b);
                self.hist_auto_snapshot = d.opt_a;
                self.status = format!(
                    "History: {n} states, {}",
                    if d.opt_b { "non-linear" } else { "linear" }
                );
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
                self.hist.set_limit(Some(20));
                self.reset_hist_for_new_doc();
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
                self.record_step(RecordedStep::LayerNew);
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
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::FtNumeric)
        ) {
            self.dialog = None;
            self.ft_cancel();
            return;
        }
        if let Some(mut d) = self.dialog.take() {
            if let Some(snap) = d.adj_snapshot.take() {
                // Adjustment dialog cancelled: restore the document
                // exactly as it was before the dialog opened.
                self.doc = snap;
                self.doc_version += 1;
                self.touch();
                return;
            }
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
    /// Custom Filter: the 25 kernel cells and the Scale/Offset
    /// stepper buttons (scale -, scale +, offset -, offset +).
    pub kernel_cells: Vec<Rect>,
    pub kernel_buttons: Vec<Rect>,
    /// Type dialog: (families list, styles list) and the format
    /// button row (Bold, Italic, Under, Left, Center, Right,
    /// Vertical, Color).
    pub font_list: Option<(Rect, Rect)>,
    pub type_buttons: Vec<Rect>,
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
        if matches!(
            d.kind,
            DialogKind::HueSaturation | DialogKind::ChannelMixer | DialogKind::ColorSettings
        ) {
            h += 22; // range / output buttons
        }
        if d.kind == DialogKind::Transform3d {
            h += 22 + 196; // shape buttons + wireframe preview
        }
        if matches!(
            d.kind,
            DialogKind::CondMode
                | DialogKind::PicturePackage
                | DialogKind::SaveForWeb
                | DialogKind::Print
                | DialogKind::PageSetup
        ) {
            h += 22; // choice buttons
        }
        if d.kind == DialogKind::SaveForWeb {
            h += 14; // encoded-size line
        }
        if let DialogKind::LegacyFilter(id) = d.kind {
            if id == 6 || id == 15 {
                h += 22; // style / method buttons
            }
        }
        if d.kind == DialogKind::PhotoFilter {
            h += 22; // preset swatches
        }
        if d.kind == DialogKind::ColorRange {
            h += 22; // sample swatch row
        }
        if d.kind == DialogKind::Batch {
            h += 52; // info lines
        }
        if d.kind == DialogKind::CustomFilter {
            h = 46 + 5 * 24 + 26 + 40; // kernel grid + scale/offset row
        }
        if d.kind == DialogKind::CanvasSize {
            h += 52; // anchor grid
        }
        if d.kind == DialogKind::Fill || d.kind == DialogKind::HistoryOptions {
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
                | DialogKind::Batch
                | DialogKind::Type
                | DialogKind::ContactSheet
                | DialogKind::WebGallery
                | DialogKind::ActionSetName
        ) {
            h += 24; // text field
        }
        if d.kind == DialogKind::Type {
            h += 128; // font lists + format buttons
        }
        if d.kind == DialogKind::Effects {
            h += 46; // page tabs + toggle row
            if d.fx_page == 4 {
                h += 22; // bevel highlight/shadow mode row
            }
        }
        if d.kind == DialogKind::Extract {
            h = 46 + 300 + 40; // mini-editor canvas + OK/Cancel
        }
        let w = 400;
        let rect = Rect::new((ww - w) / 2, (wh - h) / 2, w, h);
        let mut y = rect.y + 30;
        if matches!(d.kind, DialogKind::Levels | DialogKind::Threshold) {
            y += 34;
        }
        if d.kind == DialogKind::ColorBalance
            || d.kind == DialogKind::Fill
            || d.kind == DialogKind::HistoryOptions
            || d.kind == DialogKind::Effects
            || d.kind == DialogKind::HueSaturation
            || d.kind == DialogKind::ChannelMixer
            || d.kind == DialogKind::ColorSettings
            || d.kind == DialogKind::Transform3d
        {
            y += 22;
        }
        if d.kind == DialogKind::Transform3d {
            y += 196; // wireframe preview sits above the sliders
        }
        if matches!(
            d.kind,
            DialogKind::CondMode
                | DialogKind::PicturePackage
                | DialogKind::SaveForWeb
                | DialogKind::Print
                | DialogKind::PageSetup
        ) {
            y += 22;
        }
        if let DialogKind::LegacyFilter(id) = d.kind {
            if id == 6 || id == 15 {
                y += 22;
            }
        }
        if d.kind == DialogKind::PhotoFilter {
            y += 22;
        }
        if d.kind == DialogKind::ColorRange {
            y += 22;
        }
        if matches!(
            d.kind,
            DialogKind::SaveAs
                | DialogKind::NewDoc
                | DialogKind::NewLayer
                | DialogKind::LayerOptions
                | DialogKind::Batch
                | DialogKind::Type
                | DialogKind::ContactSheet
                | DialogKind::WebGallery
                | DialogKind::ActionSetName
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
        let range_buttons = if let DialogKind::LegacyFilter(id) = d.kind {
            let n = match id {
                6 => 3,  // ZigZag styles
                15 => 2, // Wind method + direction
                _ => 0,
            };
            (0..n)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect()
        } else if d.kind == DialogKind::Transform3d {
            (0..3)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::Effects {
            // The five effect pages as tabs.
            (0..5)
                .map(|i| Rect::new(rect.x + 12 + i * 76, rect.y + 30, 72, 18))
                .collect()
        } else if d.kind == DialogKind::PhotoFilter {
            (0..6)
                .map(|i| Rect::new(rect.x + 12 + i * 62, rect.y + 30, 56, 18))
                .collect()
        } else if matches!(
            d.kind,
            DialogKind::CondMode
                | DialogKind::PicturePackage
                | DialogKind::SaveForWeb
                | DialogKind::Print
                | DialogKind::PageSetup
        ) {
            let n = match d.kind {
                DialogKind::CondMode => 2,
                DialogKind::SaveForWeb => 3,
                _ => 1,
            };
            (0..n)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect()
        } else if d.kind == DialogKind::HueSaturation {
            // Master + the six hue ranges.
            (0..7)
                .map(|i| Rect::new(rect.x + 12 + i * 54, rect.y + 30, 52, 18))
                .collect()
        } else if d.kind == DialogKind::ColorBalance
            || d.kind == DialogKind::Fill
            || d.kind == DialogKind::HistoryOptions
            || d.kind == DialogKind::ChannelMixer
            || d.kind == DialogKind::ColorSettings
        {
            let n = if d.kind == DialogKind::ColorBalance || d.kind == DialogKind::Fill {
                3
            } else if d.kind == DialogKind::HistoryOptions {
                2
            } else if d.kind == DialogKind::ChannelMixer {
                4
            } else {
                3 // ColorSettings: the three working spaces
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
                | DialogKind::Batch
                | DialogKind::Type
                | DialogKind::ContactSheet
                | DialogKind::WebGallery
                | DialogKind::ActionSetName
        ) {
            Some(Rect::new(rect.x + 12, rect.y + 28, w - 24, 18))
        } else {
            None
        };
        let (font_list, type_buttons) = if d.kind == DialogKind::Extract {
            // Mode buttons under the mini-editor canvas.
            let by = rect.y + 30 + 282;
            let mut btns = Vec::new();
            for i in 0..4 {
                btns.push(Rect::new(rect.x + 12 + i * 96, by, 92, 18));
            }
            (None, btns)
        } else if d.kind == DialogKind::Type {
            let ly = rect.y + 30 + 24 + sliders * 20 + 8;
            let fams = Rect::new(rect.x + 12, ly, 218, 90);
            let styles = Rect::new(rect.x + 238, ly, w - 250, 90);
            let by = ly + 96;
            let mut btns = Vec::new();
            for i in 0..8 {
                btns.push(Rect::new(rect.x + 12 + i * 48, by, 44, 18));
            }
            (Some((fams, styles)), btns)
        } else if d.kind == DialogKind::Effects {
            // Toggle row under the sliders: Enable, Blend/Style,
            // Global/Up-Down, Color. The bevel page adds a second
            // row: highlight mode, shadow mode.
            let by = rect.y + 30 + 22 + sliders * 20 + 8;
            let mut btns = Vec::new();
            for i in 0..4 {
                btns.push(Rect::new(rect.x + 12 + i * 96, by, 92, 18));
            }
            if d.fx_page == 4 {
                btns.push(Rect::new(rect.x + 12, by + 22, 128, 18));
                btns.push(Rect::new(rect.x + 144, by + 22, 128, 18));
                btns.push(Rect::new(rect.x + 276, by + 22, 116, 18));
            }
            (None, btns)
        } else {
            (None, Vec::new())
        };
        let (kernel_cells, kernel_buttons) = if d.kind == DialogKind::CustomFilter {
            let gx = rect.x + 48;
            let gy = rect.y + 30;
            let mut cells = Vec::new();
            for row in 0..5 {
                for col in 0..5 {
                    cells.push(Rect::new(gx + col * 62, gy + row * 24, 56, 20));
                }
            }
            let by = gy + 5 * 24 + 6;
            let buttons = vec![
                Rect::new(rect.x + 150, by, 18, 18),
                Rect::new(rect.x + 172, by, 18, 18),
                Rect::new(rect.x + 318, by, 18, 18),
                Rect::new(rect.x + 340, by, 18, 18),
            ];
            (cells, buttons)
        } else {
            (Vec::new(), Vec::new())
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
            kernel_cells,
            kernel_buttons,
            font_list,
            type_buttons,
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
        if self.show_navigator {
            self.draw_navigator(f);
        }
        if self.show_actions {
            self.draw_actions_panel(f);
        }
        if self.show_history {
            self.draw_history_panel(f);
        }
        if self.show_info {
            self.draw_info_panel(f);
        }
        self.draw_menu_bar(f);
        self.draw_status(f);
        if self.file_panel.is_some() {
            self.draw_file_panel(f);
        }
        if self.dialog.is_some() {
            self.draw_dialog(f);
        }
        if self.context_menu.is_some() {
            self.draw_context_menu(f);
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
        } else {
            return;
        }
        // Soft proof: run the fresh render through a CMYK round
        // trip so out-of-gamut colors clip the way print will.
        if self.proof_cmyk && self.doc.mode == ColorMode::Rgb {
            for px in self.cached_render.as_chunks_mut::<4>().0 {
                let (r, g, b) = (
                    px[0] as f32 / 255.0,
                    px[1] as f32 / 255.0,
                    px[2] as f32 / 255.0,
                );
                let (c, m, y, k) = pictodesk_core::color::rgb_to_cmyk(r, g, b);
                let (r2, g2, b2) = pictodesk_core::color::cmyk_to_rgb(c, m, y, k);
                px[0] = (r2 * 255.0).round() as u8;
                px[1] = (g2 * 255.0).round() as u8;
                px[2] = (b2 * 255.0).round() as u8;
            }
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
            ChannelView::Mask(li) => {
                let Some(l) = self.doc.layers.get(li) else {
                    return (r, g, b);
                };
                let v = l.mask_value(dx, dy);
                (v, v, v)
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

        // Guides and grid (Phase 4), drawn over the document.
        if self.show_grid {
            let mut step = 64i32;
            while (step as f32 * scale) < 10.0 {
                step *= 2;
            }
            let gc = (176, 176, 176);
            let start_x = ((-self.viewport.offset.0) / scale).floor() as i32;
            let mut wx = start_x - start_x.rem_euclid(step);
            while (wx as f32) < (self.doc.width as f32) {
                let sx = doc_rect.x + (wx as f32 * scale) as i32;
                if sx >= c.x && sx < c.right() {
                    f.fill_rect(Rect::new(sx, y0.max(c.y), 1, y1 - y0.max(c.y)), gc);
                }
                wx += step;
            }
            let start_y = ((-self.viewport.offset.1) / scale).floor() as i32;
            let mut wy = start_y - start_y.rem_euclid(step);
            while (wy as f32) < (self.doc.height as f32) {
                let sy = doc_rect.y + (wy as f32 * scale) as i32;
                if sy >= c.y && sy < c.bottom() {
                    f.fill_rect(Rect::new(x0.max(c.x), sy, x1 - x0.max(c.x), 1), gc);
                }
                wy += step;
            }
        }
        if self.show_guides {
            let gc = (64, 156, 255);
            for g in &self.doc.guides_x {
                let sx = doc_rect.x + (*g as f32 * scale) as i32;
                if sx >= c.x && sx < c.right() {
                    f.fill_rect(Rect::new(sx, c.y, 1, c.h), gc);
                }
            }
            for g in &self.doc.guides_y {
                let sy = doc_rect.y + (*g as f32 * scale) as i32;
                if sy >= c.y && sy < c.bottom() {
                    f.fill_rect(Rect::new(c.x, sy, c.w, 1), gc);
                }
            }
            if let Some((vertical, pos, _)) = self.guide_drag {
                if vertical {
                    let sx = doc_rect.x + (pos as f32 * scale) as i32;
                    f.fill_rect(Rect::new(sx, c.y, 1, c.h), (255, 64, 64));
                } else {
                    let sy = doc_rect.y + (pos as f32 * scale) as i32;
                    f.fill_rect(Rect::new(c.x, sy, c.w, 1), (255, 64, 64));
                }
            }
        }

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
        // Color sampler targets (numbered, as in the classic).
        for (i, (sx, sy)) in self.doc.samplers.iter().enumerate() {
            let s = self.viewport.doc_to_screen((*sx as f32, *sy as f32));
            let (px, py) = (c.x + s.0 as i32, c.y + s.1 as i32);
            f.outline_rect(Rect::new(px - 5, py - 5, 11, 11), ui::WHITE);
            f.text(px + 7, py - 5, &format!("{}", i + 1), 1, ui::WHITE);
        }
        // Measure line.
        if let Some(((x0, y0), (x1, y1))) = self.measure {
            let a = self.viewport.doc_to_screen((x0, y0));
            let b = self.viewport.doc_to_screen((x1, y1));
            f.line(
                c.x + a.0 as i32,
                c.y + a.1 as i32,
                c.x + b.0 as i32,
                c.y + b.1 as i32,
                ui::WHITE,
            );
        }
        // Polygonal / Magnetic Lasso in progress.
        if !self.poly_points.is_empty() {
            let pts: Vec<(i32, i32)> = self
                .poly_points
                .iter()
                .map(|p| {
                    let s = self.viewport.doc_to_screen(*p);
                    (c.x + s.0 as i32, c.y + s.1 as i32)
                })
                .collect();
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, ui::WHITE);
            }
            for p in &pts {
                f.fill_rect(Rect::new(p.0 - 2, p.1 - 2, 5, 5), ui::WHITE);
            }
            // Rubber band to the cursor.
            if let Some(last) = pts.last() {
                let cur = self
                    .viewport
                    .doc_to_screen(self.to_doc(self.cursor.0, self.cursor.1));
                f.line(
                    last.0,
                    last.1,
                    c.x + cur.0 as i32,
                    c.y + cur.1 as i32,
                    ui::PLATINUM,
                );
            }
        }
        // Freeform Pen drag preview.
        if self.drag == DragMode::FreePen && self.pen_free.len() > 1 {
            let pts: Vec<(i32, i32)> = self
                .pen_free
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
        // Web-safe warning (Phase 5): the classic cube-only color
        // alert — click it to snap FG to the nearest web-safe color.
        if !Self::web_safe((fr, fgc, fb)) {
            let wr = Self::web_warn_rect(self.win_size.1 as i32);
            f.line(wr.x + 5, wr.y, wr.x, wr.y + 9, (200, 0, 0));
            f.line(wr.x, wr.y + 9, wr.x + 10, wr.y + 9, (200, 0, 0));
            f.line(wr.x + 10, wr.y + 9, wr.x + 5, wr.y, (200, 0, 0));
            f.fill_rect(Rect::new(wr.x + 4, wr.y + 3, 2, 4), (200, 0, 0));
            f.set(wr.x + 5, wr.y + 8, (200, 0, 0));
        }
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

    /// Web-safe = every channel a multiple of 51 (the 216-color
    /// web palette of the era).
    fn web_safe(rgb: (u8, u8, u8)) -> bool {
        rgb.0.is_multiple_of(51) && rgb.1.is_multiple_of(51) && rgb.2.is_multiple_of(51)
    }

    fn web_warn_rect(win_h: i32) -> Rect {
        let sy = win_h - ui::STATUS_H - 52;
        Rect::new(10, sy + 24, 11, 11)
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
        self.draw_tone_row(f);
        self.draw_tool_row(f);
    }

    /// Option row rects for the Phase 5 tools that carry options
    /// in the Brushes panel: (value button, minus, plus). The
    /// Eraser and Art History rows use only the value button.
    fn tool_row_rects(&self) -> Option<(Rect, Rect, Rect)> {
        if !matches!(
            self.tool,
            Tool::Eraser
                | Tool::MagicEraser
                | Tool::BackgroundEraser
                | Tool::ArtHistoryBrush
                | Tool::Gradient
        ) {
            return None;
        }
        let r = self.group_rect(PanelKind::Brushes)?;
        let y = r.bottom() - 21;
        Some((
            Rect::new(r.x + 52, y, 96, 16),
            Rect::new(r.x + 152, y, 16, 16),
            Rect::new(r.x + 190, y, 16, 16),
        ))
    }

    fn draw_tool_row(&mut self, f: &mut Frame) {
        let Some((value_btn, minus, plus)) = self.tool_row_rects() else {
            return;
        };
        let r = self.group_rect(PanelKind::Brushes).unwrap();
        f.fill_rect(Rect::new(r.x + 4, r.bottom() - 24, r.w - 8, 1), ui::SHADOW);
        match self.tool {
            Tool::Eraser => {
                f.text(r.x + 6, value_btn.y + 4, "Mode:", 1, ui::BLACK);
                f.button(
                    value_btn,
                    if self.eraser_to_history {
                        "Erase to History"
                    } else {
                        "Erase"
                    },
                    self.eraser_to_history,
                    true,
                );
            }
            Tool::Gradient => {
                f.text(r.x + 6, value_btn.y + 4, "Shape:", 1, ui::BLACK);
                f.button(
                    value_btn,
                    match self.gradient_shape {
                        brush::GradientShape::Linear => "Linear",
                        brush::GradientShape::Radial => "Radial",
                        brush::GradientShape::Angle => "Angle",
                        brush::GradientShape::Diamond => "Diamond",
                        brush::GradientShape::Reflected => "Reflected",
                    },
                    false,
                    true,
                );
            }
            Tool::ArtHistoryBrush => {
                f.text(r.x + 6, value_btn.y + 4, "Style:", 1, ui::BLACK);
                f.button(
                    value_btn,
                    match self.art_hist_style {
                        0 => "Tight",
                        1 => "Loose",
                        _ => "Dab",
                    },
                    false,
                    true,
                );
            }
            _ => {
                // Magic / Background Eraser: tolerance stepper.
                f.text(r.x + 6, value_btn.y + 4, "Tol:", 1, ui::BLACK);
                f.button(
                    value_btn,
                    &format!("{}", self.eraser_tolerance),
                    false,
                    true,
                );
                f.button(minus, "-", false, true);
                f.button(plus, "+", false, true);
            }
        }
    }

    /// The toning tools' option row at the bottom of the Brushes
    /// panel: (value button, exposure -, exposure +). Present only
    /// while Dodge, Burn or Sponge is the active tool.
    fn tone_row_rects(&self) -> Option<(Rect, Rect, Rect)> {
        if !matches!(self.tool, Tool::Dodge | Tool::Burn | Tool::Sponge) {
            return None;
        }
        let r = self.group_rect(PanelKind::Brushes)?;
        let y = r.bottom() - 21;
        Some((
            Rect::new(r.x + 52, y, 76, 16),
            Rect::new(r.x + 152, y, 16, 16),
            Rect::new(r.x + 190, y, 16, 16),
        ))
    }

    fn draw_tone_row(&mut self, f: &mut Frame) {
        let Some((value_btn, minus, plus)) = self.tone_row_rects() else {
            return;
        };
        let r = self.group_rect(PanelKind::Brushes).unwrap();
        f.fill_rect(Rect::new(r.x + 4, r.bottom() - 24, r.w - 8, 1), ui::SHADOW);
        if self.tool == Tool::Sponge {
            f.text(r.x + 6, value_btn.y + 4, "Mode:", 1, ui::BLACK);
            f.button(
                value_btn,
                if self.sponge_saturate {
                    "Saturate"
                } else {
                    "Desaturate"
                },
                false,
                true,
            );
        } else {
            f.text(r.x + 6, value_btn.y + 4, "Range:", 1, ui::BLACK);
            f.button(
                value_btn,
                match self.tone_range {
                    brush::ToneRange::Shadows => "Shadows",
                    brush::ToneRange::Midtones => "Midtones",
                    brush::ToneRange::Highlights => "Highlights",
                },
                false,
                true,
            );
        }
        f.button(minus, "-", false, true);
        f.text(
            minus.x + 21,
            minus.y + 4,
            &format!("{:.0}%", self.tone_exposure * 100.0),
            1,
            ui::BLACK,
        );
        f.button(plus, "+", false, true);
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

    /// The Navigator panel (Phase 4): a thumbnail of the whole
    /// document with a red rectangle marking the visible area;
    /// clicking or dragging in it pans the main view.
    fn draw_navigator(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Navigator) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Navigator", 1, ui::BLACK);
        let pct = (self.viewport.scale * 100.0).round() as i32;
        f.text(r.x + 130, r.y + 5, &format!("{pct}%"), 1, ui::DARK);
        let th = Self::nav_thumb_rect(r);
        self.ensure_render_cache();
        let (dw, dh) = (self.doc.width as i32, self.doc.height as i32);
        for yy in 0..th.h {
            for xx in 0..th.w {
                let dx = (xx * dw / th.w).min(dw - 1);
                let dy = (yy * dh / th.h).min(dh - 1);
                let i = ((dy * dw + dx) * 4) as usize;
                let (pr, pg, pb, pa) = (
                    self.cached_render[i],
                    self.cached_render[i + 1],
                    self.cached_render[i + 2],
                    self.cached_render[i + 3],
                );
                let col = if pa == 255 {
                    (pr, pg, pb)
                } else {
                    let cell = (((xx / 4) + (yy / 4)) % 2 == 0) as u32;
                    let bg = if cell == 0 { 255 } else { 170 };
                    let a = pa as u32;
                    (
                        ((pr as u32 * a + bg * (255 - a)) / 255) as u8,
                        ((pg as u32 * a + bg * (255 - a)) / 255) as u8,
                        ((pb as u32 * a + bg * (255 - a)) / 255) as u8,
                    )
                };
                f.set(th.x + xx, th.y + yy, col);
            }
        }
        f.outline_rect(th, ui::SHADOW);
        // Visible-area rectangle.
        let c = self.canvas_rect(f.w, f.h);
        let tl = self.viewport.screen_to_doc((0.0, 0.0));
        let br = self.viewport.screen_to_doc((c.w as f32, c.h as f32));
        let vx = th.x + (tl.0 / dw as f32 * th.w as f32) as i32;
        let vy = th.y + (tl.1 / dh as f32 * th.h as f32) as i32;
        let vw = ((br.0 - tl.0) / dw as f32 * th.w as f32) as i32;
        let vh = ((br.1 - tl.1) / dh as f32 * th.h as f32) as i32;
        // Clamp to the thumbnail: when the whole document is
        // visible, the view rectangle is the thumbnail itself.
        let x0 = vx.max(th.x);
        let y0 = vy.max(th.y);
        let x1 = (vx + vw).min(th.right());
        let y1 = (vy + vh).min(th.bottom());
        if x1 > x0 && y1 > y0 {
            f.outline_rect(Rect::new(x0, y0, x1 - x0, y1 - y0), (255, 0, 0));
        }
    }

    fn nav_thumb_rect(r: Rect) -> Rect {
        Rect::new(r.x + 6, r.y + 18, r.w - 12, 88)
    }

    /// Pan so the document point under a Navigator thumbnail point
    /// sits at the center of the canvas.
    fn navigator_to(&mut self, mx: i32, my: i32) {
        let Some(r) = self.group_rect(PanelKind::Navigator) else {
            return;
        };
        let th = Self::nav_thumb_rect(r);
        let fx = ((mx - th.x) as f32 / th.w as f32).clamp(0.0, 1.0);
        let fy = ((my - th.y) as f32 / th.h as f32).clamp(0.0, 1.0);
        let (dx, dy) = (fx * self.doc.width as f32, fy * self.doc.height as f32);
        let c = self.canvas_rect(self.win_size.0, self.win_size.1);
        self.viewport.offset = (
            c.w as f32 / 2.0 - dx * self.viewport.scale,
            c.h as f32 / 2.0 - dy * self.viewport.scale,
        );
    }

    /// The Actions panel (Phase 4): action list with step counts,
    /// and the New / Record / Play / Delete / Batch buttons. While
    /// recording, a red REC marker shows.
    /// Actions panel rows: a header per set that has actions (or
    /// is the current set), then that set's actions.
    fn action_rows(&self) -> Vec<ActRow> {
        let mut rows = Vec::new();
        for (si, sname) in self.action_sets.iter().enumerate() {
            let has = self.actions.iter().any(|a| &a.set == sname);
            if has || sname == &self.action_set {
                rows.push(ActRow::Set(si));
                for (ai, a) in self.actions.iter().enumerate() {
                    if &a.set == sname {
                        rows.push(ActRow::Action(ai));
                    }
                }
            }
        }
        rows
    }

    fn draw_actions_panel(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Actions) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Actions", 1, ui::BLACK);
        if self.recording {
            f.fill_rect(Rect::new(r.x + 70, r.y + 5, 7, 7), (255, 0, 0));
            f.text(r.x + 81, r.y + 5, "REC", 1, (200, 0, 0));
        }
        if self.actions.is_empty() {
            f.text(r.x + 8, r.y + 24, "No actions yet.", 1, ui::DARK);
            f.text(r.x + 8, r.y + 38, "New, then Record, then", 1, ui::DARK);
            f.text(r.x + 8, r.y + 50, "work: steps are captured.", 1, ui::DARK);
        }
        let rows = self.action_rows();
        let visible = ((r.h - 78) / 15).max(1) as usize;
        for (vi, row) in rows.iter().take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            match row {
                ActRow::Set(si) => {
                    f.fill_rect(rr, ui::PLATINUM);
                    let name = &self.action_sets[*si];
                    let cur = name == &self.action_set;
                    f.text(
                        rr.x + 4,
                        rr.y + 3,
                        &format!("{}{}", if cur { "> " } else { "" }, name),
                        1,
                        ui::BLACK,
                    );
                }
                ActRow::Action(ai) => {
                    let selected = *ai == self.active_action;
                    if selected {
                        f.fill_rect(rr, ui::BLACK);
                    }
                    let c = if selected { ui::WHITE } else { ui::BLACK };
                    let a = &self.actions[*ai];
                    f.text(
                        rr.x + 14,
                        rr.y + 3,
                        &format!("{} ({})", a.name, a.steps.len()),
                        1,
                        c,
                    );
                }
            }
        }
        // The selected action's steps, above the buttons.
        if let Some(a) = self.actions.get(self.active_action) {
            let sy = r.bottom() - 55;
            for (j, s) in a.steps.iter().take(3).enumerate() {
                f.text(
                    r.x + 14,
                    sy + j as i32 * 11,
                    &format!("- {}", s.label()),
                    1,
                    ui::DARK,
                );
            }
            if a.steps.len() > 3 {
                f.text(
                    r.x + 14,
                    sy + 33,
                    &format!("  +{} more", a.steps.len() - 3),
                    1,
                    ui::DARK,
                );
            }
        }
        // New-set button in the header (the classic flyout spot).
        Self::draw_small_button(f, Rect::new(r.right() - 18, r.y + 3, 14, 13), "+", true);
        let labels = ["New", "Rec", "Play", "Del", "Batch"];
        let bw = (r.w - 8) / 5;
        for (i, label) in labels.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
            let text = if i == 1 && self.recording {
                "Stop"
            } else {
                label
            };
            Self::draw_small_button(f, br, text, true);
        }
    }

    fn actions_panel_click(&mut self, r: Rect, mx: i32, my: i32) -> bool {
        let rows = self.action_rows();
        let visible = ((r.h - 78) / 15).max(1) as usize;
        for (vi, row) in rows.iter().take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            if rr.contains(mx, my) {
                match row {
                    ActRow::Set(si) => {
                        self.action_set = self.action_sets[*si].clone();
                        self.status = format!("Action set: {}", self.action_set);
                    }
                    ActRow::Action(ai) => {
                        self.active_action = *ai;
                    }
                }
                return true;
            }
        }
        if Rect::new(r.right() - 18, r.y + 3, 14, 13).contains(mx, my) {
            let mut d = Dialog::new(DialogKind::ActionSetName, &self.doc);
            d.text = format!("Set {}", self.action_sets.len() + 1);
            d.text_focus = true;
            self.dialog = Some(d);
            return true;
        }
        let bw = (r.w - 8) / 5;
        for i in 0..5i32 {
            let br = Rect::new(r.x + 4 + i * bw, r.bottom() - 19, bw - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::MacroNew),
                    1 => {
                        if self.recording {
                            self.do_action(Action::MacroStop)
                        } else {
                            self.do_action(Action::MacroRecord)
                        }
                    }
                    2 => self.do_action(Action::MacroPlay),
                    3 => self.do_action(Action::MacroDelete),
                    _ => self.do_action(Action::MacroBatch),
                }
                return true;
            }
        }
        true
    }

    /// History panel rows in display order: snapshots first, then
    /// states newest-to-oldest (state index = commands applied).
    fn hist_rows(&self) -> Vec<HistRow> {
        let mut rows = Vec::new();
        for i in 0..self.snapshots.len() {
            rows.push(HistRow::Snapshot(i));
        }
        for idx in (0..=self.hist.state_count()).rev() {
            rows.push(HistRow::State(idx));
        }
        rows
    }

    fn hist_row_label(&self, row: HistRow) -> String {
        match row {
            HistRow::Snapshot(i) => self
                .snapshots
                .get(i)
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            HistRow::State(0) => {
                let base = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                format!("{base} (Open)")
            }
            HistRow::State(idx) => self
                .hist
                .labels()
                .get(idx - 1)
                .cloned()
                .unwrap_or_else(|| format!("State {idx}")),
        }
    }

    fn draw_history_panel(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::History) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "History", 1, ui::BLACK);
        // Scroll buttons (rows hidden above/below).
        let rows = self.hist_rows();
        let visible = ((r.h - 58) / 15).max(1) as usize;
        if rows.len() > visible {
            Self::draw_small_button(f, Rect::new(r.right() - 34, r.y + 3, 14, 13), "^", true);
            Self::draw_small_button(f, Rect::new(r.right() - 18, r.y + 3, 14, 13), "v", true);
        }
        let pos = self.hist.position();
        for (vi, row) in rows.iter().skip(self.hist_scroll).take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            let is_current = matches!(row, HistRow::State(idx) if *idx == pos);
            let is_selected = self.hist_selected == Some(*row);
            if is_current {
                f.fill_rect(rr, ui::BLACK);
            } else if is_selected {
                f.fill_rect(rr, ui::PLATINUM_LIGHT);
            }
            let c = if is_current { ui::WHITE } else { ui::BLACK };
            // Source box: the state the History Brush paints from.
            let sb = Rect::new(rr.x + 2, rr.y + 2, 11, 11);
            let is_source = match (row, self.hist_source) {
                (HistRow::Snapshot(a), HistSource::Snapshot(b)) => *a == b,
                (HistRow::State(a), HistSource::State(b)) => *a == b,
                _ => false,
            };
            f.outline_rect(sb, c);
            if is_source {
                f.fill_rect(Rect::new(sb.x + 2, sb.y + 2, 7, 7), c);
            }
            let tag = match row {
                HistRow::Snapshot(_) => "S ",
                HistRow::State(_) => "",
            };
            f.text(
                rr.x + 17,
                rr.y + 3,
                &format!("{tag}{}", self.hist_row_label(*row)),
                1,
                c,
            );
        }
        let row_a = ["Snap", "Merge", "Layer"];
        let bw3 = (r.w - 8) / 3;
        for (i, label) in row_a.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw3, r.bottom() - 36, bw3 - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
        let labels = ["New", "Opts", "Trash"];
        for (i, label) in labels.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw3, r.bottom() - 19, bw3 - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
    }

    fn history_panel_click(&mut self, r: Rect, mx: i32, my: i32) -> bool {
        let rows = self.hist_rows();
        let visible = ((r.h - 58) / 15).max(1) as usize;
        if rows.len() > visible {
            let up = Rect::new(r.right() - 34, r.y + 3, 14, 13);
            let dn = Rect::new(r.right() - 18, r.y + 3, 14, 13);
            if up.contains(mx, my) {
                self.hist_scroll = self.hist_scroll.saturating_sub(1);
                return true;
            }
            if dn.contains(mx, my) {
                let max = rows.len() - visible;
                self.hist_scroll = (self.hist_scroll + 1).min(max);
                return true;
            }
        }
        for (vi, row) in rows.iter().skip(self.hist_scroll).take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            if rr.contains(mx, my) {
                let row = *row;
                self.hist_selected = Some(row);
                // Left source box: choose the brush/fill source.
                if mx < rr.x + 15 {
                    self.hist_source = match row {
                        HistRow::Snapshot(i) => HistSource::Snapshot(i),
                        HistRow::State(i) => HistSource::State(i),
                    };
                    self.hist_src_cache = None;
                    self.status = "History source set".into();
                    return true;
                }
                match row {
                    HistRow::State(idx) => {
                        if self.hist.goto_state(&mut self.doc, idx) {
                            self.doc_version += 1;
                            self.dirty = true;
                            self.status = format!("State: {}", self.hist_row_label(row));
                        }
                    }
                    HistRow::Snapshot(i) => {
                        if let Some(snap) = self.snapshots.get(i).cloned() {
                            let mut cmd = DocumentCommand::begin(&snap.name, &self.doc);
                            self.doc = snap.doc;
                            cmd.finish(&self.doc);
                            self.hist.push(Box::new(cmd));
                            self.doc_version += 1;
                            self.dirty = true;
                            self.status = format!("Snapshot: {}", snap.name);
                        }
                    }
                }
                return true;
            }
        }
        let bw3 = (r.w - 8) / 3;
        for i in 0..3i32 {
            let br = Rect::new(r.x + 4 + i * bw3, r.bottom() - 36, bw3 - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::HistSnapshot),
                    1 => self.do_action(Action::HistSnapshotMerged),
                    _ => self.do_action(Action::HistSnapshotLayer),
                }
                return true;
            }
        }
        for i in 0..3i32 {
            let br = Rect::new(r.x + 4 + i * bw3, r.bottom() - 19, bw3 - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::HistNewDoc),
                    1 => self.do_action(Action::HistOptions),
                    _ => self.do_action(Action::HistTrash),
                }
                return true;
            }
        }
        true
    }

    fn draw_info_panel(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Info) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Info", 1, ui::BLACK);
        let mut y = r.y + 22;
        let line = |f: &mut Frame, y: i32, s: &str| {
            f.text(r.x + 8, y, s, 1, ui::DARK);
        };
        let mode_name = match self.doc.mode {
            ColorMode::Rgb => "RGB",
            ColorMode::Cmyk => "CMYK",
            ColorMode::Grayscale => "Grayscale",
        };
        line(
            f,
            y,
            &format!(
                "Image: {} x {} px, {}",
                self.doc.width, self.doc.height, mode_name
            ),
        );
        y += 13;
        let (fx, fy) = self.to_doc(self.cursor.0, self.cursor.1);
        let (dx, dy) = (fx.floor() as i32, fy.floor() as i32);
        if dx >= 0 && dy >= 0 && (dx as u32) < self.doc.width && (dy as u32) < self.doc.height {
            let p = self.doc.render_pixel(dx, dy);
            let (cr, cg, cb, ca) = p.to_rgba8();
            line(f, y, &format!("X: {dx}  Y: {dy}"));
            y += 13;
            line(f, y, &format!("R: {cr}  G: {cg}  B: {cb}  A: {ca}"));
            y += 13;
            if self.doc.mode == ColorMode::Cmyk {
                let (inks, _) = self.doc.composite_inks(dx, dy);
                let pc = |v: f32| (v * 100.0).round() as i32;
                line(
                    f,
                    y,
                    &format!(
                        "C: {}%  M: {}%  Y: {}%  K: {}%",
                        pc(inks[0]),
                        pc(inks[1]),
                        pc(inks[2]),
                        pc(inks[3])
                    ),
                );
                y += 13;
            }
        } else {
            line(f, y, "Move over the image for values.");
            y += 13;
        }
        for (i, (sx, sy)) in self.doc.samplers.iter().enumerate() {
            let p = self.doc.render_pixel(*sx, *sy);
            let (cr, cg, cb, _) = p.to_rgba8();
            line(
                f,
                y,
                &format!("#{}: R{cr} G{cg} B{cb}  ({sx}, {sy})", i + 1),
            );
            y += 13;
        }
        if let Some(((x0, y0), (x1, y1))) = self.measure {
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx).to_degrees();
            line(f, y, &format!("D: {len:.1} px  A: {ang:.1} deg"));
        }
    }

    /// Right-click (Phase 4): open a context menu suited to where
    /// the click landed — layer operations over the Layers panel,
    /// editing operations over the canvas.
    pub fn right_click(&mut self, x: f32, y: f32) {
        let (mx, my) = (x as i32, y as i32);
        if self.dialog.is_some() || self.file_panel.is_some() {
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            return;
        }
        // Over a layer row?
        if let Some(r) = self.group_rect(PanelKind::Cp) {
            if self.cp_tab == 0 && r.contains(mx, my) {
                let n = self.doc.layers.len();
                for row in 0..(n - self.layers_scroll).min(8) {
                    let li = n - 1 - (self.layers_scroll + row);
                    let rr = Self::layer_row_rect(r, row);
                    if rr.contains(mx, my) {
                        self.doc.active_layer = li;
                        let l = &self.doc.layers[li];
                        let mut items = vec![
                            ("Layer Options...".to_string(), Action::LayerOptions, true),
                            ("Duplicate Layer".to_string(), Action::DuplicateLayer, true),
                            (
                                "Delete Layer".to_string(),
                                Action::DeleteLayer,
                                self.doc.layers.len() > 1,
                            ),
                            ("-".to_string(), Action::None, false),
                        ];
                        if l.adjustment.is_some() {
                            items.push((
                                "Edit Adjustment...".to_string(),
                                Action::EditAdjustment,
                                true,
                            ));
                        }
                        if l.mask.is_none() {
                            items.push((
                                "Add Mask (Reveal All)".to_string(),
                                Action::AddMaskReveal,
                                true,
                            ));
                            items.push((
                                "Add Mask (Hide All)".to_string(),
                                Action::AddMaskHide,
                                true,
                            ));
                        } else {
                            items.push(("Delete Mask".to_string(), Action::DeleteMask, true));
                            items.push((
                                "Disable/Enable Mask".to_string(),
                                Action::ToggleMaskEnabled,
                                true,
                            ));
                        }
                        items.push(("-".to_string(), Action::None, false));
                        items.push(("Merge Down".to_string(), Action::MergeDown, li > 0));
                        self.context_menu = Some(ContextMenu {
                            x: mx,
                            y: my,
                            items,
                        });
                        return;
                    }
                }
            }
        }
        // Over the canvas?
        let c = self.canvas_rect(self.win_size.0, self.win_size.1);
        if c.contains(mx, my) {
            let has_sel = self.sel.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
            let items = vec![
                ("Undo/Redo".to_string(), Action::UndoToggle, true),
                ("-".to_string(), Action::None, false),
                ("Cut".to_string(), Action::Cut, has_sel),
                ("Copy".to_string(), Action::Copy, has_sel),
                ("Paste".to_string(), Action::Paste, true),
                ("Clear".to_string(), Action::Clear, has_sel),
                ("-".to_string(), Action::None, false),
                ("Select All".to_string(), Action::SelectAll, true),
                ("Deselect".to_string(), Action::SelectNone, has_sel),
                ("Inverse".to_string(), Action::SelectInverse, has_sel),
                ("-".to_string(), Action::None, false),
                (
                    "Free Transform".to_string(),
                    Action::FreeTransform,
                    self.doc.layers[self.doc.active_layer].adjustment.is_none(),
                ),
                (
                    "Add Layer Mask (Reveal)".to_string(),
                    Action::AddMaskReveal,
                    self.doc.layers[self.doc.active_layer].mask.is_none(),
                ),
            ];
            self.context_menu = Some(ContextMenu {
                x: mx,
                y: my,
                items,
            });
        }
    }

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
                f.fill_rect(ir, ui::BLACK);
                f.text(ir.x + 8, ir.y + 4, label, 1, ui::WHITE);
            } else {
                let c = if *enabled { ui::BLACK } else { ui::SHADOW };
                f.text(ir.x + 8, ir.y + 4, label, 1, c);
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
            let (visible, linked, name, has_mask, mask_enabled, is_adj, is_mask_target, has_fx) = {
                let l = &self.doc.layers[li];
                (
                    l.visible,
                    l.linked,
                    l.name.clone(),
                    l.mask.is_some(),
                    l.mask_enabled,
                    l.adjustment.is_some(),
                    self.paint_target == PaintTarget::Mask(li),
                    l.effects.map(|e| e.any_enabled()).unwrap_or(false),
                )
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
            if is_adj {
                // Adjustment layer: the classic half-and-half disc.
                f.fill_rect(th, ui::WHITE);
                let (cx, cy) = (th.x + 18, th.y + 12);
                for yy in -9..=9i32 {
                    for xx in -9..=9i32 {
                        if xx * xx + yy * yy <= 81 {
                            f.set(cx + xx, cy + yy, if xx < 0 { ui::BLACK } else { ui::WHITE });
                        }
                    }
                }
                f.outline_rect(th, c);
            } else {
                self.draw_layer_thumb(f, th, li);
                f.outline_rect(th, c);
            }
            let mut name_x = rr.x + 61;
            if has_mask {
                let mh = Rect::new(rr.x + 59, rr.y + 2, 28, 24);
                self.draw_mask_thumb(f, mh, li);
                if is_mask_target {
                    // The targeted thumbnail gets a double outline.
                    f.outline_rect(Rect::new(mh.x - 1, mh.y - 1, mh.w + 2, mh.h + 2), c);
                }
                f.outline_rect(mh, c);
                if !mask_enabled {
                    // Disabled mask: red X, the classic marker.
                    for t in 0..24i32 {
                        f.set(mh.x + 2 + t, mh.y + 2 + t * 20 / 24, (255, 0, 0));
                        f.set(mh.x + 2 + t, mh.y + 21 - t * 20 / 24, (255, 0, 0));
                    }
                }
                name_x = rr.x + 91;
            }
            f.text(name_x, rr.y + 10, &name, 1, c);
            if has_fx {
                // The classic italic "f" badge: the layer carries
                // live effects.
                f.text(rr.right() - 32, rr.y + 10, "f", 1, c);
            }
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

    /// A mask thumbnail: the mask plane as a small grayscale image.
    fn draw_mask_thumb(&self, f: &mut Frame, th: Rect, li: usize) {
        let (dw, dh) = (
            (self.doc.width as i32).max(1),
            (self.doc.height as i32).max(1),
        );
        for by in 0..12i32 {
            for bx in 0..14i32 {
                let dx = ((bx * 2 + 1) * dw / 28).min(dw - 1);
                let dy = ((by * 2 + 1) * dh / 24).min(dh - 1);
                let v = self.doc.layers[li].mask_value(dx, dy);
                f.fill_rect(Rect::new(th.x + bx * 2, th.y + by * 2, 2, 2), (v, v, v));
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
            // Mask thumbnail: click targets the mask for painting;
            // Alt+click toggles it enabled/disabled.
            if self.doc.layers[li].mask.is_some() {
                let mh = Rect::new(rr.x + 59, rr.y + 2, 28, 24);
                if mh.contains(mx, my) {
                    self.doc.active_layer = li;
                    if self.alt_down {
                        self.structural("Toggle Mask", |d| {
                            d.layers[li].mask_enabled = !d.layers[li].mask_enabled;
                        });
                    } else {
                        self.paint_target = PaintTarget::Mask(li);
                        self.channel_view = ChannelView::Mask(li);
                        self.status = "Painting on the mask (white reveals, black hides)".into();
                    }
                    self.touch();
                    return true;
                }
            }
            // Adjustment thumbnail: click opens the editor.
            if self.doc.layers[li].adjustment.is_some() {
                let th = Rect::new(rr.x + 21, rr.y + 2, 36, 24);
                if th.contains(mx, my) {
                    self.doc.active_layer = li;
                    self.do_action(Action::EditAdjustment);
                    return true;
                }
            }
            if self.doc.active_layer != li {
                self.record_step(RecordedStep::LayerSelect(li));
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
                    PaintTarget::Mask(li) => {
                        let v = self.fg_luminance();
                        self.structural("Fill Path", |d| {
                            if let Some((bx, by, bw, bh)) = sel.bounds() {
                                for yy in by..by + bh as i32 {
                                    for xx in bx..bx + bw as i32 {
                                        let m = sel.value(xx, yy);
                                        if m > 0 {
                                            let cur = d.layers[li].mask_value(xx, yy) as u32;
                                            let nv =
                                                (cur + (v as u32 - cur) * m as u32 / 255) as u8;
                                            d.layers[li].set_mask_value(xx, yy, nv);
                                        }
                                    }
                                }
                            }
                        });
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
                    PaintTarget::Mask(li) => {
                        let tip = self.tip;
                        let v = self.fg_luminance();
                        let (w, h) = (self.doc.width, self.doc.height);
                        self.structural("Stroke Path", |d| {
                            for pt in &pts {
                                brush::stamp_mask(
                                    &mut d.layers[li],
                                    w,
                                    h,
                                    pt.0,
                                    pt.1,
                                    &tip,
                                    v,
                                    1.0,
                                );
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
        // Navigator thumbnail: click/drag pans the main view.
        if let Some(r) = self.group_rect(PanelKind::Navigator) {
            if r.contains(mx, my) {
                let th = Self::nav_thumb_rect(r);
                if th.contains(mx, my) {
                    self.navigator_to(mx, my);
                    self.drag = DragMode::NavDrag;
                }
                return true;
            }
        }
        // Actions panel
        if let Some(r) = self.group_rect(PanelKind::Actions) {
            if r.contains(mx, my) {
                return self.actions_panel_click(r, mx, my);
            }
        }
        // History panel
        if let Some(r) = self.group_rect(PanelKind::History) {
            if r.contains(mx, my) {
                return self.history_panel_click(r, mx, my);
            }
        }
        // Toning-tool option row (Brushes panel, tone tools only)
        if let Some((value_btn, minus, plus)) = self.tone_row_rects() {
            if value_btn.contains(mx, my) {
                if self.tool == Tool::Sponge {
                    self.sponge_saturate = !self.sponge_saturate;
                    self.status = format!(
                        "Sponge mode: {}",
                        if self.sponge_saturate {
                            "Saturate"
                        } else {
                            "Desaturate"
                        }
                    );
                } else {
                    self.tone_range = match self.tone_range {
                        brush::ToneRange::Shadows => brush::ToneRange::Midtones,
                        brush::ToneRange::Midtones => brush::ToneRange::Highlights,
                        brush::ToneRange::Highlights => brush::ToneRange::Shadows,
                    };
                    self.status = format!(
                        "Range: {}",
                        match self.tone_range {
                            brush::ToneRange::Shadows => "Shadows",
                            brush::ToneRange::Midtones => "Midtones",
                            brush::ToneRange::Highlights => "Highlights",
                        }
                    );
                }
                return true;
            }
            if minus.contains(mx, my) {
                self.tone_exposure = (self.tone_exposure - 0.05).max(0.05);
                return true;
            }
            if plus.contains(mx, my) {
                self.tone_exposure = (self.tone_exposure + 0.05).min(1.0);
                return true;
            }
        }
        // Phase 5 tool option row (Brushes panel)
        if let Some((value_btn, minus, plus)) = self.tool_row_rects() {
            if value_btn.contains(mx, my) {
                match self.tool {
                    Tool::Eraser => {
                        self.eraser_to_history = !self.eraser_to_history;
                        self.status = if self.eraser_to_history {
                            "Eraser: erase to the history source".into()
                        } else {
                            "Eraser: erase to transparency".into()
                        };
                    }
                    Tool::Gradient => {
                        use brush::GradientShape as GS;
                        self.gradient_shape = match self.gradient_shape {
                            GS::Linear => GS::Radial,
                            GS::Radial => GS::Angle,
                            GS::Angle => GS::Diamond,
                            GS::Diamond => GS::Reflected,
                            GS::Reflected => GS::Linear,
                        };
                        self.status = "Gradient shape changed".into();
                    }
                    Tool::ArtHistoryBrush => {
                        self.art_hist_style = (self.art_hist_style + 1) % 3;
                        self.status = format!(
                            "Art History style: {}",
                            ["Tight", "Loose", "Dab"][self.art_hist_style as usize]
                        );
                    }
                    _ => {}
                }
                return true;
            }
            if minus.contains(mx, my) {
                self.eraser_tolerance = self.eraser_tolerance.saturating_sub(8);
                return true;
            }
            if plus.contains(mx, my) {
                self.eraser_tolerance = self.eraser_tolerance.saturating_add(8);
                return true;
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
            f.button(
                layout.fill_buttons[0],
                "Foreground",
                !d.fill_bg && !d.fill_history,
                true,
            );
            f.button(
                layout.fill_buttons[1],
                "Background",
                d.fill_bg && !d.fill_history,
                true,
            );
            f.button(layout.fill_buttons[2], "History", d.fill_history, true);
        }
        if d.kind == DialogKind::HistoryOptions {
            f.button(layout.fill_buttons[0], "Auto snapshot", d.opt_a, true);
            f.button(layout.fill_buttons[1], "Non-linear", d.opt_b, true);
        }
        if d.kind == DialogKind::HueSaturation {
            let names = [
                "Master", "Reds", "Yellw", "Green", "Cyans", "Blues", "Magnt",
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.hs_range == i, true);
            }
        }
        if d.kind == DialogKind::ChannelMixer {
            let names = if self.doc.mode == ColorMode::Cmyk {
                ["Out C", "Out M", "Out Y", "Out K"]
            } else {
                ["Out R", "Out G", "Out B", "Mono"]
            };
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let on = if i == 3 && self.doc.mode != ColorMode::Cmyk {
                    d.mixer.monochrome
                } else {
                    d.mix_out == i
                };
                f.button(*br, names[i], on, true);
            }
        }
        if d.kind == DialogKind::Extract {
            if let Some(er) = self.extract_editor_rect() {
                f.fill_rect(er, (128, 128, 128));
                let (w, h) = (self.doc.width as f32, self.doc.height as f32);
                let scale = (er.w as f32 / w).min(er.h as f32 / h);
                let (ox, oy) = (
                    er.x as f32 + (er.w as f32 - w * scale) / 2.0,
                    er.y as f32 + (er.h as f32 - h * scale) / 2.0,
                );
                let preview = if d.extract_preview {
                    self.extract_preview_cache.as_ref()
                } else {
                    None
                };
                let src: &[u8] = preview.unwrap_or(&self.cached_render);
                let (dw, dh) = (self.doc.width, self.doc.height);
                for py in 0..er.h {
                    for px in 0..er.w {
                        let dx = ((px as f32 + er.x as f32 - ox) / scale).floor() as i32;
                        let dy = ((py as f32 + er.y as f32 - oy) / scale).floor() as i32;
                        if dx < 0 || dy < 0 || dx as u32 >= dw || dy as u32 >= dh {
                            continue;
                        }
                        let i = ((dy as u32 * dw + dx as u32) * 4) as usize;
                        if i + 3 >= src.len() {
                            continue;
                        }
                        let (mut r, mut g, mut b) = (src[i], src[i + 1], src[i + 2]);
                        if preview.is_none() {
                            let mi = (dy as u32 * dw + dx as u32) as usize;
                            if d.extract_hi.get(mi).copied().unwrap_or(0) > 0 {
                                r = 255;
                                g = 120;
                                b = 0;
                            } else if d.extract_fill.get(mi).copied().unwrap_or(0) > 0 {
                                r = ((r as u16 + 40) / 2) as u8;
                                g = ((g as u16 + 80) / 2) as u8;
                                b = 255;
                            }
                        }
                        f.set(er.x + px, er.y + py, (r, g, b));
                    }
                }
                f.outline_rect(er, ui::BLACK);
                let names = ["Highlight", "Fill", "Erase", "Preview"];
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    let on = if i == 3 {
                        d.extract_preview
                    } else {
                        d.extract_mode == i as u8
                    };
                    f.button(*br, names[i], on, true);
                }
                f.text(
                    r.x + 12,
                    er.bottom() + 24,
                    "Paint the edge, fill inside it, then OK.",
                    1,
                    ui::DARK,
                );
            }
        }
        if d.kind == DialogKind::Transform3d {
            let names = ["Cube", "Sphere", "Cylinder"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.shape3d == i as u8, true);
            }
            // Wireframe preview: the solid for the current shape
            // at the current rotation. Dragging inside it rotates
            // the solid (see the mouse handlers); the sliders
            // stay in sync because both edit vals[0] / vals[1].
            if let Some(wr) = self.wireframe_rect() {
                f.fill_rect(wr, (34, 34, 42));
                f.outline_rect(wr, ui::BLACK);
                let (rx, ry) = (d.vals[0].to_radians(), d.vals[1].to_radians());
                let (sx, cx) = (rx.sin(), rx.cos());
                let (sy, cy) = (ry.sin(), ry.cos());
                let (ccx, ccy) = (wr.x + wr.w / 2, wr.y + wr.h / 2);
                let half = wr.h as f32 * 0.46;
                // Object -> world: RotY(ry) applied after
                // RotX(rx), the inverse of the core ray-caster's
                // unrotation, then a camera-at-z=3 perspective.
                let proj = |p: (f32, f32, f32)| -> (i32, i32) {
                    let (x1, y1, z1) = (p.0, cx * p.1 - sx * p.2, sx * p.1 + cx * p.2);
                    let (x2, y2, z2) = (cy * x1 + sy * z1, y1, -sy * x1 + cy * z1);
                    let persp = 1.9 / (3.0 - z2);
                    (
                        ccx + (x2 * half * persp) as i32,
                        ccy - (y2 * half * persp) as i32,
                    )
                };
                let mut segs: WireSegs = Vec::new();
                let mut handles: Vec<(i32, i32)> = Vec::new();
                let circle = |segs: &mut WireSegs,
                              proj: &dyn Fn((f32, f32, f32)) -> (i32, i32),
                              pt: &dyn Fn(f32) -> (f32, f32, f32)| {
                    let n = 48;
                    for i in 0..n {
                        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
                        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
                        segs.push((proj(pt(a0)), proj(pt(a1))));
                    }
                };
                match d.shape3d {
                    1 => {
                        // Sphere: equator + two meridians.
                        circle(&mut segs, &proj, &|a: f32| (a.cos(), 0.0, a.sin()));
                        circle(&mut segs, &proj, &|a: f32| (0.0, a.sin(), a.cos()));
                        circle(&mut segs, &proj, &|a: f32| (a.cos(), a.sin(), 0.0));
                        for k in 0..4 {
                            let a = k as f32 * std::f32::consts::FRAC_PI_2;
                            handles.push(proj((a.cos(), 0.0, a.sin())));
                        }
                    }
                    2 => {
                        // Cylinder: top + bottom rings and four
                        // verticals joining them.
                        circle(&mut segs, &proj, &|a: f32| (a.cos(), 1.0, a.sin()));
                        circle(&mut segs, &proj, &|a: f32| (a.cos(), -1.0, a.sin()));
                        for k in 0..4 {
                            let a = k as f32 * std::f32::consts::FRAC_PI_2;
                            let top = proj((a.cos(), 1.0, a.sin()));
                            let bot = proj((a.cos(), -1.0, a.sin()));
                            segs.push((top, bot));
                            handles.push(top);
                            handles.push(bot);
                        }
                    }
                    _ => {
                        // Cube: 8 corners, 12 edges; corners are
                        // drawn as drag handles.
                        let mut corners = Vec::new();
                        for &zz in &[-1.0f32, 1.0] {
                            for &yy in &[-1.0f32, 1.0] {
                                for &xx in &[-1.0f32, 1.0] {
                                    corners.push(proj((xx, yy, zz)));
                                }
                            }
                        }
                        // Corner index bits: x = bit0, y = bit1,
                        // z = bit2 (after the -1 / +1 ordering
                        // above is normalised below).
                        let idx = |x: usize, y: usize, z: usize| -> usize {
                            // loops above push z outer, y mid, x
                            // inner, each -1 then +1.
                            z * 4 + y * 2 + x
                        };
                        for z in 0..2 {
                            for y in 0..2 {
                                segs.push((corners[idx(0, y, z)], corners[idx(1, y, z)]));
                            }
                            for x in 0..2 {
                                segs.push((corners[idx(x, 0, z)], corners[idx(x, 1, z)]));
                            }
                        }
                        for y in 0..2 {
                            for x in 0..2 {
                                segs.push((corners[idx(x, y, 0)], corners[idx(x, y, 1)]));
                            }
                        }
                        handles = corners;
                    }
                }
                for (a, b) in &segs {
                    f.line(a.0, a.1, b.0, b.1, (140, 200, 255));
                }
                for hp in &handles {
                    f.fill_rect(Rect::new(hp.0 - 2, hp.1 - 2, 5, 5), (255, 220, 120));
                }
                f.text(wr.x + 6, wr.y + 6, "Drag to rotate", 1, (170, 170, 180));
            }
        }
        if let DialogKind::LegacyFilter(id) = d.kind {
            if id == 6 {
                let names = ["Pond Ripples", "Out From Center", "Around Center"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 15 {
                let methods = ["Method: Wind", "Method: Blast", "Method: Stagger"];
                f.button(
                    layout.range_buttons[0],
                    methods[d.fx_a as usize],
                    false,
                    true,
                );
                f.button(
                    layout.range_buttons[1],
                    if d.fx_b { "From: Left" } else { "From: Right" },
                    false,
                    true,
                );
            }
        }
        if d.kind == DialogKind::CondMode {
            let froms = ["Any mode", "If RGB", "If Grayscale", "If CMYK"];
            let tos = ["To RGB", "To Grayscale", "To CMYK"];
            f.button(
                layout.range_buttons[0],
                froms[d.cond_from as usize],
                false,
                true,
            );
            f.button(
                layout.range_buttons[1],
                tos[d.cond_to as usize],
                false,
                true,
            );
        }
        if d.kind == DialogKind::PicturePackage {
            let names = [
                "Layout: 1-up",
                "Layout: 2-up",
                "Layout: 4-up",
                "Layout: wallets",
            ];
            f.button(
                layout.range_buttons[0],
                names[d.pkg_layout as usize],
                false,
                true,
            );
        }
        if d.kind == DialogKind::SaveForWeb {
            let names = ["JPEG", "PNG", "GIF"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.web_format == i as u8, true);
            }
            f.text(
                r.x + 12,
                r.bottom() - 52,
                &format!("Encoded size: {} bytes", d.web_size),
                1,
                ui::DARK,
            );
        }
        if d.kind == DialogKind::Print {
            f.button(
                layout.range_buttons[0],
                if d.print_profile == 0 {
                    "Color: document"
                } else {
                    "Color: printer"
                },
                false,
                true,
            );
        }
        if d.kind == DialogKind::PageSetup {
            f.button(
                layout.range_buttons[0],
                if d.opt_a { "Landscape" } else { "Portrait" },
                d.opt_a,
                true,
            );
        }
        if d.kind == DialogKind::ColorSettings {
            let names = ["sRGB", "Adobe RGB", "ProPhoto"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.cb_range == i, true);
            }
            f.text(
                r.x + 12,
                r.bottom() - 52,
                "Working space for new documents; Assign/Convert live in the Mode menu.",
                1,
                ui::DARK,
            );
        }
        if d.kind == DialogKind::Effects {
            let tabs = ["Drop", "Inner", "Glow", "InGlow", "Bevel"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, tabs[i], d.fx_page == i, true);
            }
            let fx = d.fx;
            let (enabled, blend_label, mid_label, mid_on) = match d.fx_page {
                0 => (
                    fx.drop_shadow.enabled,
                    fx.drop_shadow.mode.name(),
                    "Global angle",
                    fx.drop_shadow.use_global_angle,
                ),
                1 => (
                    fx.inner_shadow.enabled,
                    fx.inner_shadow.mode.name(),
                    "Global angle",
                    fx.inner_shadow.use_global_angle,
                ),
                2 => (fx.outer_glow.enabled, fx.outer_glow.mode.name(), "-", false),
                3 => (
                    fx.inner_glow.enabled,
                    fx.inner_glow.mode.name(),
                    if fx.inner_glow.source_center {
                        "Center"
                    } else {
                        "Edge"
                    },
                    fx.inner_glow.source_center,
                ),
                _ => (
                    fx.bevel.enabled,
                    ["Outer", "Inner", "Emboss", "Pillow"][fx.bevel.style as usize],
                    if fx.bevel.up { "Up" } else { "Down" },
                    fx.bevel.up,
                ),
            };
            let color = match d.fx_page {
                0 => fx.drop_shadow.color,
                1 => fx.inner_shadow.color,
                2 => fx.outer_glow.color,
                3 => fx.inner_glow.color,
                _ => {
                    if d.opt_a {
                        fx.bevel.sh_color
                    } else {
                        fx.bevel.hi_color
                    }
                }
            };
            let (cr, cg, cb, _) = color.to_rgba8();
            for (i, br) in layout.type_buttons.iter().enumerate() {
                match i {
                    0 => f.button(*br, "Apply", enabled, true),
                    1 => f.button(*br, blend_label, false, true),
                    2 => f.button(*br, mid_label, mid_on, d.fx_page != 2),
                    4 => f.button(
                        *br,
                        &format!("Hi: {}", fx.bevel.hi_mode.name()),
                        false,
                        true,
                    ),
                    5 => f.button(
                        *br,
                        &format!("Sh: {}", fx.bevel.sh_mode.name()),
                        false,
                        true,
                    ),
                    6 => f.button(*br, "Global", fx.bevel.use_global_angle, true),
                    _ => {
                        let label = if d.fx_page == 4 {
                            if d.opt_a {
                                "Sh:"
                            } else {
                                "Hi:"
                            }
                        } else {
                            "Col:"
                        };
                        f.button(*br, label, false, true);
                        let sw = Rect::new(br.x + 44, br.y + 2, 14, 14);
                        f.fill_rect(sw, (cr, cg, cb));
                        f.outline_rect(sw, ui::BLACK);
                    }
                }
            }
        }
        if d.kind == DialogKind::PhotoFilter {
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let c = PHOTO_FILTER_PRESETS[i].1;
                f.fill_rect(*br, c);
                f.outline_rect(*br, ui::SHADOW);
                if d.photo_color == c {
                    f.outline_rect(Rect::new(br.x - 1, br.y - 1, br.w + 2, br.h + 2), ui::BLACK);
                }
            }
        }
        if d.kind == DialogKind::ColorRange {
            let (sr, sg, sb, _) = self.fg.to_rgba8();
            let sw = Rect::new(r.x + 12, r.y + 28, 18, 18);
            f.fill_rect(sw, (sr, sg, sb));
            f.outline_rect(sw, ui::BLACK);
            f.text(
                r.x + 38,
                r.y + 34,
                "Sampling the foreground color",
                1,
                ui::BLACK,
            );
        }
        if d.kind == DialogKind::CustomFilter {
            for (i, cell) in layout.kernel_cells.iter().enumerate() {
                f.fill_rect(*cell, ui::WHITE);
                f.bevel_sunken(*cell);
                if i == d.kernel_sel {
                    f.outline_rect(
                        Rect::new(cell.x - 1, cell.y - 1, cell.w + 2, cell.h + 2),
                        ui::BLACK,
                    );
                }
                let label = d.kernel[i].to_string();
                let tx = cell.x + (cell.w - label.len() as i32 * 6) / 2;
                f.text(tx, cell.y + 6, &label, 1, ui::BLACK);
            }
            let by = r.y + 156;
            f.text(r.x + 48, by + 5, "Scale:", 1, ui::BLACK);
            f.text(r.x + 96, by + 5, &d.kernel_scale.to_string(), 1, ui::BLACK);
            f.button(layout.kernel_buttons[0], "-", false, true);
            f.button(layout.kernel_buttons[1], "+", false, true);
            f.text(r.x + 216, by + 5, "Offset:", 1, ui::BLACK);
            f.text(
                r.x + 268,
                by + 5,
                &d.kernel_offset.to_string(),
                1,
                ui::BLACK,
            );
            f.button(layout.kernel_buttons[2], "-", false, true);
            f.button(layout.kernel_buttons[3], "+", false, true);
            f.text(
                r.x + 48,
                by + 26,
                "Click a cell, then type its value.",
                1,
                ui::DARK,
            );
        }
        if d.kind == DialogKind::Batch {
            let name = self
                .actions
                .get(self.active_action)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "(none)".into());
            f.text(r.x + 12, r.y + 56, &format!("Action: {name}"), 1, ui::BLACK);
            f.text(
                r.x + 12,
                r.y + 70,
                "Type the folder path above, then OK.",
                1,
                ui::DARK,
            );
            f.text(
                r.x + 12,
                r.y + 82,
                "Results are written as <name>-edited.png",
                1,
                ui::DARK,
            );
            f.text(
                r.x + 12,
                r.y + 94,
                "beside each original (never overwritten).",
                1,
                ui::DARK,
            );
        }
        // text field
        if let Some(tf) = layout.text_field {
            f.fill_rect(tf, ui::WHITE);
            f.bevel_sunken(tf);
            let shown = d.text.replace('\n', " / ");
            f.text(tf.x + 5, tf.y + 5, &shown, 1, ui::BLACK);
        }
        // Type dialog: font lists + format buttons
        if d.kind == DialogKind::Type {
            if let (Some((fams_r, styles_r)), Some(spec)) = (layout.font_list, &d.type_spec) {
                let fams = typo::families();
                let cur_fam = spec.default_style.font.clone();
                let cur_sty = spec.default_style.style.clone();
                f.fill_rect(fams_r, ui::WHITE);
                f.bevel_sunken(fams_r);
                for (vi, fam) in fams.iter().skip(d.font_scroll).take(6).enumerate() {
                    let ry = fams_r.y + 2 + vi as i32 * 15;
                    if *fam == cur_fam {
                        f.fill_rect(Rect::new(fams_r.x + 1, ry - 1, fams_r.w - 2, 14), ui::BLACK);
                        f.text(fams_r.x + 5, ry + 2, fam, 1, ui::WHITE);
                    } else {
                        f.text(fams_r.x + 5, ry + 2, fam, 1, ui::BLACK);
                    }
                }
                let styles = typo::styles_of(&cur_fam);
                f.fill_rect(styles_r, ui::WHITE);
                f.bevel_sunken(styles_r);
                for (vi, sty) in styles.iter().take(6).enumerate() {
                    let ry = styles_r.y + 2 + vi as i32 * 15;
                    if *sty == cur_sty {
                        f.fill_rect(
                            Rect::new(styles_r.x + 1, ry - 1, styles_r.w - 2, 14),
                            ui::BLACK,
                        );
                        f.text(styles_r.x + 5, ry + 2, sty, 1, ui::WHITE);
                    } else {
                        f.text(styles_r.x + 5, ry + 2, sty, 1, ui::BLACK);
                    }
                }
                // Family list scroll steppers.
                Self::draw_small_button(
                    f,
                    Rect::new(fams_r.right() - 15, fams_r.y + 1, 14, 14),
                    "^",
                    true,
                );
                Self::draw_small_button(
                    f,
                    Rect::new(fams_r.right() - 15, fams_r.bottom() - 15, 14, 14),
                    "v",
                    true,
                );
                let st = spec.style_at(0);
                let labels = [
                    "Bold", "Ital", "Undr", "Left", "Cntr", "Rght", "Vert", "Colr",
                ];
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    let on = match i {
                        0 => st.faux_bold,
                        1 => st.faux_italic,
                        2 => st.underline,
                        3 => spec.align == 0,
                        4 => spec.align == 1,
                        5 => spec.align == 2,
                        6 => spec.vertical,
                        _ => false,
                    };
                    f.button(*br, labels[i], on, true);
                }
                f.text(
                    r.x + 12,
                    layout.type_buttons[0].y + 22,
                    "Shift+Enter = new line. From/To chars style a range.",
                    1,
                    ui::DARK,
                );
            }
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

/// A minimal one-page PDF wrapping a JPEG as a full-page image.
fn build_pdf_one_page(jpeg: &[u8], w: u32, h: u32) -> Vec<u8> {
    let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q");
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        let n = offsets.len();
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(
        &mut out,
        &mut offsets,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    obj(
        &mut out,
        &mut offsets,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        )
        .as_bytes(),
    );
    // Image object (stream): header, jpeg bytes, trailer.
    offsets.push(out.len());
    out.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(jpeg);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    obj(
        &mut out,
        &mut offsets,
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
        .as_bytes(),
    );
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Hand a PDF to the platform print spooler; returns a status
/// line. macOS/Linux spool with `lp`; Windows prints through
/// PowerShell's Print verb.
fn spool_print(path: &std::path::Path, copies: u32) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = copies; // the verb prints once per call
        let mut ok = true;
        for _ in 0..copies.max(1) {
            let r = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!("Start-Process -FilePath '{}' -Verb Print", path.display()),
                ])
                .status();
            ok = ok && r.map(|s| s.success()).unwrap_or(false);
        }
        if ok {
            return "Sent to the printer".into();
        }
        return format!("Print failed — the PDF is at {}", path.display());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let r = std::process::Command::new("lp")
            .arg(format!("-n{}", copies.max(1)))
            .arg(path)
            .status();
        match r {
            Ok(s) if s.success() => "Sent to the printer".into(),
            _ => format!(
                "Print spooler unavailable — the PDF is at {}",
                path.display()
            ),
        }
    }
}

/// Display name for a working color space.
fn space_name(s: pictodesk_core::color::WorkingSpace) -> &'static str {
    match s {
        pictodesk_core::color::WorkingSpace::Srgb => "sRGB",
        pictodesk_core::color::WorkingSpace::AdobeRgb => "Adobe RGB (1998)",
        pictodesk_core::color::WorkingSpace::ProPhoto => "ProPhoto RGB",
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
        Tool::Dodge => {
            // The darkroom dodging paddle: a disc on a stick, held
            // to keep light off part of the print.
            for yy in -5..=5i32 {
                for xx in -5..=5i32 {
                    let d2 = xx * xx + yy * yy;
                    if (13..=25).contains(&d2) {
                        f.set(cx - 2 + xx, cy - 3 + yy, b);
                    }
                }
            }
            f.line(cx + 1, cy + 1, cx + 6, cy + 7, b);
        }
        Tool::Burn => {
            // A cupped hand shading the print to burn in an area.
            for xx in -6..=6i32 {
                let yy = ((36 - xx * xx) as f32).sqrt() as i32;
                f.set(cx + xx, cy - yy / 2, b);
                f.set(cx + xx, cy + 1 - yy / 2, b);
            }
            f.line(cx - 6, cy + 3, cx - 6, cy + 6, b);
            f.line(cx + 6, cy + 3, cx + 6, cy + 6, b);
            f.line(cx - 6, cy + 6, cx + 6, cy + 6, b);
        }
        Tool::Sponge => {
            // A porous block.
            f.outline_rect(Rect::new(cx - 6, cy - 5, 13, 11), b);
            for (dx, dy) in [(-3, -2), (1, -3), (4, 0), (-1, 1), (-4, 3), (3, 3)] {
                f.set(cx + dx, cy + dy, b);
            }
        }
        Tool::PolyLasso => {
            // Straight-segment loop with square anchors.
            let pts = [
                (cx - 6, cy + 5),
                (cx - 3, cy - 6),
                (cx + 5, cy - 4),
                (cx + 6, cy + 4),
            ];
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, b);
            }
            f.line(pts[3].0, pts[3].1, pts[0].0, pts[0].1, b);
            for p in pts {
                f.fill_rect(Rect::new(p.0 - 1, p.1 - 1, 3, 3), b);
            }
        }
        Tool::MagneticLasso => {
            // Lasso loop plus a small magnet block.
            let pts = [
                (cx - 7, cy + 4),
                (cx - 3, cy - 6),
                (cx + 4, cy - 5),
                (cx + 6, cy + 2),
                (cx + 1, cy + 6),
            ];
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, b);
            }
            f.line(pts[4].0, pts[4].1, pts[0].0, pts[0].1, b);
            f.fill_rect(Rect::new(cx + 3, cy + 3, 3, 5), b);
            f.fill_rect(Rect::new(cx + 6, cy + 3, 3, 5), b);
            f.fill_rect(Rect::new(cx + 3, cy + 6, 6, 2), b);
        }
        Tool::ColorSampler => {
            // Eyedropper over a target ring.
            f.line(cx - 2, cy + 2, cx + 5, cy - 5, b);
            f.fill_rect(Rect::new(cx + 4, cy - 8, 4, 4), b);
            for a in 0..32 {
                let t = a as f32 / 32.0 * std::f32::consts::TAU;
                f.set(
                    cx - 4 + (t.cos() * 4.0) as i32,
                    cy + 4 + (t.sin() * 4.0) as i32,
                    b,
                );
            }
            f.set(cx - 4, cy + 4, b);
        }
        Tool::Measure => {
            // A ruler: long diagonal with end ticks.
            f.line(cx - 7, cy + 7, cx + 7, cy - 7, b);
            f.line(cx - 7, cy + 3, cx - 4, cy + 7, b);
            f.line(cx + 4, cy - 7, cx + 7, cy - 3, b);
            f.set(cx - 2, cy + 2, b);
            f.set(cx + 2, cy - 2, b);
        }
        Tool::HistoryBrush => {
            // Brush with a return arrow arc above.
            f.line(cx + 6, cy + 7, cx + 1, cy + 1, b);
            f.fill_rect(Rect::new(cx - 3, cy + 2, 4, 4), b);
            for a in 0..24 {
                let t = std::f32::consts::FRAC_PI_2 + a as f32 / 24.0 * std::f32::consts::PI;
                f.set(
                    cx + (t.cos() * 6.0) as i32,
                    cy - 3 + (t.sin() * 4.0) as i32,
                    b,
                );
            }
            f.set(cx - 6, cy - 4, b);
            f.set(cx - 6, cy - 2, b);
            f.set(cx - 4, cy - 3, b);
        }
        Tool::ArtHistoryBrush => {
            // Brush with a swirl.
            f.line(cx + 6, cy + 7, cx + 1, cy + 1, b);
            f.fill_rect(Rect::new(cx - 3, cy + 2, 4, 4), b);
            for a in 0..36 {
                let t = a as f32 / 36.0 * std::f32::consts::TAU * 1.5;
                let rr = 1.0 + a as f32 / 36.0 * 4.0;
                f.set(
                    cx - 2 + (t.cos() * rr) as i32,
                    cy - 4 + (t.sin() * rr * 0.7) as i32,
                    b,
                );
            }
        }
        Tool::MagicEraser => {
            // Eraser block with a sparkle.
            f.outline_rect(Rect::new(cx - 7, cy - 1, 11, 7), b);
            f.line(cx + 5, cy - 7, cx + 5, cy - 2, b);
            f.line(cx + 2, cy - 5, cx + 8, cy - 5, b);
            f.set(cx + 6, cy + 4, b);
            f.set(cx - 5, cy + 7, b);
        }
        Tool::BackgroundEraser => {
            // Eraser over a dotted (disappearing) ground.
            f.outline_rect(Rect::new(cx - 6, cy - 7, 11, 7), b);
            for (dx, dy) in [(-5, 3), (-1, 4), (3, 3), (6, 5), (-3, 7), (1, 7)] {
                f.set(cx + dx, cy + dy, b);
            }
        }
        Tool::PatternStamp => {
            // Stamp over a checker patch.
            f.fill_rect(Rect::new(cx - 2, cy - 8, 4, 5), b);
            f.fill_rect(Rect::new(cx - 5, cy - 3, 10, 3), b);
            for (dx, dy) in [(-6, 2), (-2, 2), (2, 6), (-6, 6), (6, 2)] {
                f.fill_rect(Rect::new(cx + dx, cy + dy, 3, 3), b);
            }
            f.outline_rect(Rect::new(cx - 7, cy + 1, 13, 8), b);
        }
        Tool::FreeformPen => {
            // Pen nib with a wavy freehand line.
            f.line(cx - 6, cy - 7, cx - 2, cy - 2, b);
            f.line(cx - 2, cy - 2, cx + 2, cy - 6, b);
            for a in 0..16 {
                let t = a as f32 / 16.0;
                let xx = cx - 7 + (t * 14.0) as i32;
                let yy = cy + 4 + (t * std::f32::consts::TAU * 1.5).sin() as i32 * 2;
                f.set(xx, yy, b);
            }
        }
        Tool::MagneticPen => {
            // Pen nib with anchor dots pulled to a line.
            f.line(cx - 6, cy - 7, cx - 2, cy - 2, b);
            f.line(cx - 2, cy - 2, cx + 2, cy - 6, b);
            f.line(cx - 7, cy + 6, cx + 7, cy + 2, b);
            for p in [(cx - 7, cy + 6), (cx, cy + 4), (cx + 7, cy + 2)] {
                f.fill_rect(Rect::new(p.0 - 1, p.1 - 1, 3, 3), b);
            }
        }
        Tool::Type => {
            // A capital T.
            f.fill_rect(Rect::new(cx - 6, cy - 7, 13, 2), b);
            f.fill_rect(Rect::new(cx - 1, cy - 7, 3, 14), b);
            f.fill_rect(Rect::new(cx - 3, cy + 6, 7, 2), b);
        }
        Tool::TypeMask => {
            // A T inside a dashed selection outline.
            f.fill_rect(Rect::new(cx - 3, cy - 6, 7, 2), b);
            f.fill_rect(Rect::new(cx - 1, cy - 6, 3, 12), b);
            for x in (cx - 8)..(cx + 9) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy - 8, b);
                    f.set(x, cy + 8, b);
                }
            }
            for y in (cy - 8)..(cy + 9) {
                if (y / 3) % 2 == 0 {
                    f.set(cx - 8, y, b);
                    f.set(cx + 8, y, b);
                }
            }
        }
        Tool::VertTypeMask => {
            // A sideways T inside a dashed outline.
            f.fill_rect(Rect::new(cx - 6, cy - 3, 2, 7), b);
            f.fill_rect(Rect::new(cx - 6, cy - 1, 10, 3), b);
            for x in (cx - 8)..(cx + 9) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy - 8, b);
                    f.set(x, cy + 8, b);
                }
            }
            for y in (cy - 8)..(cy + 9) {
                if (y / 3) % 2 == 0 {
                    f.set(cx - 8, y, b);
                    f.set(cx + 8, y, b);
                }
            }
        }
        Tool::VertType => {
            // A T on its side with a downward arrow.
            f.fill_rect(Rect::new(cx - 7, cy - 6, 2, 13), b);
            f.fill_rect(Rect::new(cx - 7, cy - 1, 12, 3), b);
            f.set(cx + 6, cy + 5, b);
            f.set(cx + 5, cy + 6, b);
            f.set(cx + 7, cy + 6, b);
            f.set(cx + 6, cy + 7, b);
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
                | (DialogKind::HighPass, 0)
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
                    let is_photo = {
                        let d = self.dialog.as_mut().unwrap();
                        if d.kind == DialogKind::ColorBalance {
                            d.cb_range = i;
                        } else if d.kind == DialogKind::Fill {
                            d.fill_bg = i == 1;
                            d.fill_history = i == 2;
                        } else if d.kind == DialogKind::HistoryOptions {
                            if i == 0 {
                                d.opt_a = !d.opt_a;
                            } else {
                                d.opt_b = !d.opt_b;
                            }
                        } else if d.kind == DialogKind::PhotoFilter {
                            d.photo_color = PHOTO_FILTER_PRESETS[i].1;
                        }
                        d.kind == DialogKind::PhotoFilter
                    };
                    if is_photo {
                        self.dialog_refresh_preview();
                    }
                    return;
                }
            }
            // Choice buttons for the Automate / Print / Web
            // dialogs, filter option buttons, 3D shapes.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::CondMode)
                    | Some(DialogKind::PicturePackage)
                    | Some(DialogKind::SaveForWeb)
                    | Some(DialogKind::Print)
                    | Some(DialogKind::PageSetup)
                    | Some(DialogKind::Transform3d)
                    | Some(DialogKind::LegacyFilter(_))
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        let kind = self.dialog.as_ref().unwrap().kind.clone();
                        match kind {
                            DialogKind::CondMode => {
                                let d = self.dialog.as_mut().unwrap();
                                if i == 0 {
                                    d.cond_from = (d.cond_from + 1) % 4;
                                } else {
                                    d.cond_to = (d.cond_to + 1) % 3;
                                }
                            }
                            DialogKind::PicturePackage => {
                                let d = self.dialog.as_mut().unwrap();
                                d.pkg_layout = (d.pkg_layout + 1) % 4;
                            }
                            DialogKind::SaveForWeb => {
                                self.dialog.as_mut().unwrap().web_format = i as u8;
                                self.refresh_web_size();
                            }
                            DialogKind::Print => {
                                let d = self.dialog.as_mut().unwrap();
                                d.print_profile = (d.print_profile + 1) % 2;
                            }
                            DialogKind::PageSetup => {
                                let d = self.dialog.as_mut().unwrap();
                                d.opt_a = !d.opt_a;
                                // Paper turns with the orientation.
                                let (pw, ph) = (d.vals[0], d.vals[1]);
                                d.vals[0] = ph;
                                d.vals[1] = pw;
                            }
                            DialogKind::Transform3d => {
                                self.dialog.as_mut().unwrap().shape3d = i as u8;
                                self.dialog_refresh_preview();
                            }
                            DialogKind::LegacyFilter(id) => {
                                let d = self.dialog.as_mut().unwrap();
                                if id == 6 {
                                    d.fx_a = i as u8;
                                } else if id == 15 {
                                    if i == 0 {
                                        d.fx_a = (d.fx_a + 1) % 3;
                                    } else {
                                        d.fx_b = !d.fx_b;
                                    }
                                }
                                self.dialog_refresh_preview();
                            }
                            _ => {}
                        }
                        return;
                    }
                }
            }
            // Range/output buttons for H/S, Channel Mixer, Color
            // Settings (handled before the generic range loop).
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::HueSaturation)
                    | Some(DialogKind::ChannelMixer)
                    | Some(DialogKind::ColorSettings)
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        let kind = self.dialog.as_ref().unwrap().kind.clone();
                        match kind {
                            DialogKind::HueSaturation => {
                                self.dialog.as_mut().unwrap().hs_range = i;
                            }
                            DialogKind::ColorSettings => {
                                self.dialog.as_mut().unwrap().cb_range = i;
                            }
                            DialogKind::ChannelMixer => {
                                let cmyk = self.doc.mode == ColorMode::Cmyk;
                                let d = self.dialog.as_mut().unwrap();
                                if i == 3 && !cmyk {
                                    d.mixer.monochrome = !d.mixer.monochrome;
                                } else {
                                    // Stash the current row, switch,
                                    // load the new row's values.
                                    d.mixer.rows[d.mix_out] = [
                                        d.vals[0] / 100.0,
                                        d.vals[1] / 100.0,
                                        d.vals[2] / 100.0,
                                        d.vals[3] / 100.0,
                                    ];
                                    d.mixer.constant[d.mix_out] = d.vals[4] / 100.0;
                                    d.mix_out = i;
                                    let row = d.mixer.rows[i];
                                    d.vals = vec![
                                        row[0] * 100.0,
                                        row[1] * 100.0,
                                        row[2] * 100.0,
                                        row[3] * 100.0,
                                        d.mixer.constant[i] * 100.0,
                                    ];
                                }
                            }
                            _ => {}
                        }
                        self.dialog_refresh_preview();
                        return;
                    }
                }
            }
            // Effects dialog: page tabs + toggle buttons
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Effects)
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        self.fx_sync_from_vals();
                        self.dialog.as_mut().unwrap().fx_page = i;
                        self.fx_load_vals();
                        return;
                    }
                }
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if br.contains(mx, my) {
                        self.fx_button_click(i);
                        return;
                    }
                }
            }
            // Extract dialog: mode buttons + editor painting
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Extract)
            ) {
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if br.contains(mx, my) {
                        if i == 3 {
                            let d = self.dialog.as_mut().unwrap();
                            d.extract_preview = !d.extract_preview;
                            let on = d.extract_preview;
                            if on {
                                self.compute_extract_preview();
                            }
                        } else {
                            self.dialog.as_mut().unwrap().extract_mode = i as u8;
                        }
                        return;
                    }
                }
                if let Some(dp) = self.extract_doc_point(mx, my) {
                    self.extract_paint(dp);
                    return;
                }
                if self
                    .extract_editor_rect()
                    .map(|r| r.contains(mx, my))
                    .unwrap_or(false)
                {
                    return;
                }
            }
            // Type dialog: font lists and format buttons
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Type)
            ) {
                if let Some((fams_r, styles_r)) = layout.font_list {
                    let up = Rect::new(fams_r.right() - 15, fams_r.y + 1, 14, 14);
                    let dn = Rect::new(fams_r.right() - 15, fams_r.bottom() - 15, 14, 14);
                    if up.contains(mx, my) {
                        let d = self.dialog.as_mut().unwrap();
                        d.font_scroll = d.font_scroll.saturating_sub(5);
                        return;
                    }
                    if dn.contains(mx, my) {
                        let max = typo::families().len().saturating_sub(6);
                        let d = self.dialog.as_mut().unwrap();
                        d.font_scroll = (d.font_scroll + 5).min(max);
                        return;
                    }
                    if fams_r.contains(mx, my) {
                        let row = ((my - fams_r.y - 2) / 15).max(0) as usize;
                        let fams = typo::families();
                        let scroll = self.dialog.as_ref().unwrap().font_scroll;
                        if let Some(fam) = fams.get(scroll + row).cloned() {
                            let first_style =
                                typo::styles_of(&fam).into_iter().next().unwrap_or_default();
                            self.type_style_apply(|st| {
                                st.font = fam.clone();
                                st.style = first_style.clone();
                            });
                        }
                        return;
                    }
                    if styles_r.contains(mx, my) {
                        let row = ((my - styles_r.y - 2) / 15).max(0) as usize;
                        let fam = self
                            .dialog
                            .as_ref()
                            .unwrap()
                            .type_spec
                            .as_ref()
                            .map(|s| s.default_style.font.clone())
                            .unwrap_or_default();
                        if let Some(sty) = typo::styles_of(&fam).get(row).cloned() {
                            self.type_style_apply(|st| st.style = sty.clone());
                        }
                        return;
                    }
                }
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if br.contains(mx, my) {
                        match i {
                            0 => self.type_style_apply(|st| st.faux_bold = !st.faux_bold),
                            1 => self.type_style_apply(|st| st.faux_italic = !st.faux_italic),
                            2 => self.type_style_apply(|st| st.underline = !st.underline),
                            3..=5 => {
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.align = (i - 3) as u8;
                                }
                            }
                            6 => {
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.vertical = !spec.vertical;
                                }
                            }
                            _ => {
                                let fg = self.fg;
                                self.type_style_apply(|st| st.color = fg);
                            }
                        }
                        return;
                    }
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
            for (i, cell) in layout.kernel_cells.iter().enumerate() {
                if cell.contains(mx, my) {
                    self.dialog.as_mut().unwrap().kernel_sel = i;
                    return;
                }
            }
            for (i, b) in layout.kernel_buttons.iter().enumerate() {
                if b.contains(mx, my) {
                    {
                        let d = self.dialog.as_mut().unwrap();
                        match i {
                            0 => d.kernel_scale = (d.kernel_scale - 1).max(1),
                            1 => d.kernel_scale = (d.kernel_scale + 1).min(999),
                            2 => d.kernel_offset = (d.kernel_offset - 1).max(-255),
                            _ => d.kernel_offset = (d.kernel_offset + 1).min(255),
                        }
                    }
                    self.dialog_refresh_preview();
                    return;
                }
            }
            // 3D Transform wireframe: press inside it to start
            // a rotate drag.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Transform3d)
            ) {
                if let Some(wr) = self.wireframe_rect() {
                    if wr.contains(mx, my) {
                        self.dialog.as_mut().unwrap().wire_drag = Some((mx, my));
                        return;
                    }
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
        // ---- context menu (open): click an item or dismiss
        if let Some(cm) = self.context_menu.take() {
            let w = cm
                .items
                .iter()
                .map(|(l, _, _)| Frame::text_width(l, 1) + 28)
                .max()
                .unwrap_or(120);
            let r = Rect::new(cm.x, cm.y, w, cm.items.len() as i32 * 17 + 6);
            if r.contains(mx, my) {
                for (i, (label, action, enabled)) in cm.items.iter().enumerate() {
                    let ir = Rect::new(r.x + 2, r.y + 3 + i as i32 * 17, r.w - 4, 17);
                    if ir.contains(mx, my) && *enabled && label != "-" {
                        let action = action.clone();
                        self.do_action(action);
                        return;
                    }
                }
            }
            return;
        }
        // ---- guides: drag out of a ruler, or drag an existing one
        if self.show_guides && self.dialog.is_none() {
            let c = self.canvas_rect(self.win_size.0, self.win_size.1);
            let dp = self.to_doc(x, y);
            if self.show_rulers {
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
                if top.contains(mx, my) && mx >= c.x {
                    self.guide_drag = Some((false, dp.1 as i32, None));
                    self.drag = DragMode::GuideDrag;
                    return;
                }
                if left.contains(mx, my) && my >= c.y {
                    self.guide_drag = Some((true, dp.0 as i32, None));
                    self.drag = DragMode::GuideDrag;
                    return;
                }
            }
            if self.tool == Tool::Move && c.contains(mx, my) {
                for (i, g) in self.doc.guides_x.iter().enumerate() {
                    let sx = c.x + self.viewport.doc_to_screen((*g as f32, 0.0)).0 as i32;
                    if (mx - sx).abs() <= 3 {
                        self.guide_drag = Some((true, *g, Some(i)));
                        self.drag = DragMode::GuideDrag;
                        return;
                    }
                }
                for (i, g) in self.doc.guides_y.iter().enumerate() {
                    let sy = c.y + self.viewport.doc_to_screen((0.0, *g as f32)).1 as i32;
                    if (my - sy).abs() <= 3 {
                        self.guide_drag = Some((false, *g, Some(i)));
                        self.drag = DragMode::GuideDrag;
                        return;
                    }
                }
            }
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
                let (fr, fgc, fb, _) = self.fg.to_rgba8();
                if !Self::web_safe((fr, fgc, fb))
                    && Self::web_warn_rect(self.win_size.1 as i32).contains(mx, my)
                {
                    let snap = |v: u8| ((v as u16 + 25) / 51 * 51).min(255) as u8;
                    self.fg = Pixel16::from_rgba8(snap(fr), snap(fgc), snap(fb), 255);
                    self.status = "Foreground snapped to the nearest web-safe color".into();
                } else if self.swatt_default_hit(mx, my) {
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
                let w = Selection::wand(&self.doc, dp.0 as i32, dp.1 as i32, 32, contiguous);
                self.replace_sel(Some(w));
                self.status = "Magic Wand selection".into();
            }
            Tool::MagicEraser => {
                let layer = self.doc.active_layer;
                let tol = self.eraser_tolerance;
                let contiguous = !self.alt_down;
                let tiles = self.active_tiles_all();
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Magic Eraser",
                    layer,
                    &tiles,
                    |d| {
                        brush::erase_magic(d, layer, dp.0 as i32, dp.1 as i32, tol, contiguous);
                    },
                );
                self.last_label = Some("Magic Eraser".into());
                self.status = "Magic Eraser".into();
                self.touch();
            }
            Tool::ColorSampler => {
                let (sx, sy) = (dp.0.floor() as i32, dp.1.floor() as i32);
                // Clicking near an existing sampler moves it.
                let near = self
                    .doc
                    .samplers
                    .iter()
                    .position(|(x, y)| (x - sx).abs() <= 3 && (y - sy).abs() <= 3);
                if let Some(i) = near {
                    self.doc.samplers[i] = (sx, sy);
                    self.status = format!("Sampler {} moved", i + 1);
                } else if self.doc.samplers.len() < 4 {
                    self.doc.samplers.push((sx, sy));
                    self.status = format!("Sampler {} set", self.doc.samplers.len());
                    if !self.show_info {
                        self.show_info = true;
                    }
                } else {
                    self.status = "Four samplers maximum (click one to move it)".into();
                }
            }
            Tool::Measure => {
                self.measure = Some(((dp.0, dp.1), (dp.0, dp.1)));
                self.drag = DragMode::MeasureDrag;
                if !self.show_info {
                    self.show_info = true;
                }
            }
            Tool::Pen => {
                self.pen_down(x, y, dp);
            }
            Tool::Type | Tool::VertType | Tool::TypeMask | Tool::VertTypeMask => {
                let vertical = matches!(self.tool, Tool::VertType | Tool::VertTypeMask);
                let mask = matches!(self.tool, Tool::TypeMask | Tool::VertTypeMask);
                self.open_type_dialog(dp, vertical, mask);
            }
            Tool::PolyLasso => {
                // Click-click-click; clicking the first anchor (or
                // double-clicking) closes the selection.
                let close = self.poly_points.len() >= 3
                    && (dp.0 - self.poly_points[0].0).abs() < 6.0
                    && (dp.1 - self.poly_points[0].1).abs() < 6.0;
                if close {
                    self.close_poly_selection();
                } else {
                    self.poly_points.push(dp);
                }
            }
            Tool::MagneticLasso => {
                let p = self.mag_snap(dp);
                self.poly_points = vec![p];
                self.drag = DragMode::MagLasso;
            }
            Tool::FreeformPen => {
                self.begin_path_edit();
                self.ensure_pen_target();
                self.pen_free = vec![dp];
                self.drag = DragMode::FreePen;
            }
            Tool::MagneticPen => {
                self.begin_path_edit();
                self.ensure_pen_target();
                let p = self.mag_snap(dp);
                self.poly_points = vec![p];
                self.drag = DragMode::MagPen;
            }
            Tool::Bucket => {
                if let PaintTarget::Mask(li) = self.paint_target {
                    let mut cmd = DocumentCommand::begin("Paint Bucket", &self.doc);
                    let (w, h) = (self.doc.width, self.doc.height);
                    let v = self.fg_luminance();
                    if li < self.doc.layers.len() {
                        brush::flood_mask(
                            &mut self.doc.layers[li],
                            w,
                            h,
                            dp.0 as i32,
                            dp.1 as i32,
                            v,
                            32,
                        );
                    }
                    cmd.finish(&self.doc);
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some("Paint Bucket".into());
                    self.status = "Paint Bucket (mask)".into();
                    self.touch();
                    return;
                }
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
            | Tool::HistoryBrush
            | Tool::ArtHistoryBrush
            | Tool::PatternStamp
            | Tool::BackgroundEraser
            | Tool::Blur
            | Tool::Sharpen
            | Tool::Smudge
            | Tool::Dodge
            | Tool::Burn
            | Tool::Sponge => {
                if let PaintTarget::Mask(_) = self.paint_target {
                    if matches!(self.tool, Tool::Brush | Tool::Pencil | Tool::Eraser) {
                        self.alpha_cmd = Some(DocumentCommand::begin("Paint on mask", &self.doc));
                        self.alpha_last = dp;
                        self.mask_stamp(dp);
                        self.drag = DragMode::Paint;
                    } else {
                        self.status = "That tool paints on the image, not on a mask".into();
                    }
                    return;
                }
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
                if self.tool == Tool::PatternStamp && self.doc.pattern.is_none() {
                    self.status =
                        "Define a pattern first (select, then Edit > Define Pattern)".into();
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

    /// One brush/pencil/eraser stamp into the targeted layer mask.
    /// Painting with white reveals, black hides; the eraser paints
    /// with the background color, as on a mask in the classic editor.
    fn mask_stamp(&mut self, center: (f32, f32)) {
        let PaintTarget::Mask(li) = self.paint_target else {
            return;
        };
        let value = if self.tool == Tool::Eraser {
            let (r, g, b, _) = self.bg.to_rgba8();
            ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
        } else {
            self.fg_luminance()
        };
        let tip = if self.tool == Tool::Pencil {
            BrushTip::new(self.tip.size, 1.0)
        } else {
            self.tip
        };
        let (w, h) = (self.doc.width, self.doc.height);
        if li < self.doc.layers.len() && self.doc.layers[li].mask.is_some() {
            brush::stamp_mask(
                &mut self.doc.layers[li],
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
    /// Snap a point to the strongest luminance edge within a
    /// 10 px radius (Magnetic Lasso / Magnetic Pen). The luma
    /// plane of the flattened image is cached per doc_version.
    fn mag_snap(&mut self, dp: (f32, f32)) -> (f32, f32) {
        if self.mag_cache.as_ref().map(|(v, _)| *v) != Some(self.doc_version) {
            let rgba = self.doc.render_full_rgba8();
            let mut luma = Vec::with_capacity((self.doc.width * self.doc.height) as usize);
            for px in rgba.as_chunks::<4>().0 {
                luma.push(
                    ((px[0] as u32 * 77 + px[1] as u32 * 150 + px[2] as u32 * 29) / 256) as u8,
                );
            }
            self.mag_cache = Some((self.doc_version, luma));
        }
        let (w, h) = (self.doc.width as i32, self.doc.height as i32);
        let (_, luma) = self.mag_cache.as_ref().unwrap();
        let at = |x: i32, y: i32| -> i32 {
            if x < 0 || y < 0 || x >= w || y >= h {
                0
            } else {
                luma[(y * w + x) as usize] as i32
            }
        };
        let (cx, cy) = (dp.0.round() as i32, dp.1.round() as i32);
        let mut best = (dp.0, dp.1);
        let mut best_score = -1i32;
        for yy in (cy - 10)..=(cy + 10) {
            for xx in (cx - 10)..=(cx + 10) {
                if xx < 0 || yy < 0 || xx >= w || yy >= h {
                    continue;
                }
                let g = (at(xx + 1, yy) - at(xx - 1, yy)).abs()
                    + (at(xx, yy + 1) - at(xx, yy - 1)).abs();
                // Prefer strong edges; break ties toward the cursor.
                let d2 = (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy);
                let score = g * 64 - d2;
                if score > best_score {
                    best_score = score;
                    best = (xx as f32, yy as f32);
                }
            }
        }
        best
    }

    /// Type tool: click empty canvas for a new type layer, click
    /// an existing type layer to re-edit it.
    fn open_type_dialog(&mut self, dp: (f32, f32), vertical: bool, mask: bool) {
        use pictodesk_core::text::TextSpec;
        let (px, py) = (dp.0.floor() as i32, dp.1.floor() as i32);
        let mut target = None;
        if mask {
            // Type Mask never edits a layer; it makes a selection.
            let mut d = Dialog::new(DialogKind::Type, &self.doc);
            let mut s = pictodesk_core::text::TextSpec::new("");
            s.vertical = vertical;
            if let Some(fam) = typo::families().into_iter().next() {
                s.default_style.font = fam;
            }
            s.default_style.color = self.fg;
            d.type_anchor = (px, py);
            d.type_mask = true;
            d.text_focus = true;
            d.vals = vec![s.default_style.size, 0.0, 0.0, 0.0, 0.0];
            d.type_spec = Some(s);
            self.dialog = Some(d);
            return;
        }
        for (i, l) in self.doc.layers.iter().enumerate().rev() {
            if l.text.is_some() && l.visible {
                if let Some((bx, by, ex, ey)) = pictodesk_core::transform::layer_painted_bounds(l) {
                    if px >= bx && py >= by && px < ex && py < ey {
                        target = Some(i);
                        break;
                    }
                }
            }
        }
        let mut d = Dialog::new(DialogKind::Type, &self.doc);
        let spec = if let Some(li) = target {
            d.type_layer = Some(li);
            self.doc.layers[li].text.clone().unwrap()
        } else {
            let mut s = TextSpec::new("");
            s.vertical = vertical;
            if let Some(fam) = typo::families().into_iter().next() {
                s.default_style.font = fam;
            }
            s.default_style.color = self.fg;
            d.type_anchor = (px, py);
            s
        };
        d.text = spec.text.clone();
        d.text_focus = true;
        d.vals = vec![
            spec.default_style.size,
            spec.default_style.tracking,
            spec.leading,
            0.0,
            spec.text.len() as f32,
        ];
        // Scroll the family list so the current family shows.
        let fams = typo::families();
        if let Some(idx) = fams.iter().position(|f| *f == spec.default_style.font) {
            d.font_scroll = idx.saturating_sub(2);
        }
        d.type_spec = Some(spec);
        self.dialog = Some(d);
    }

    /// Apply a style change in the Type dialog: to the From–To
    /// character range when one is set, otherwise to the whole
    /// text (default style and every run alike).
    fn type_style_apply<F: Fn(&mut pictodesk_core::text::CharStyle)>(&mut self, f: F) {
        let Some(d) = &mut self.dialog else { return };
        let Some(spec) = &mut d.type_spec else { return };
        let (from, to) = (
            d.vals[3].round().max(0.0) as usize,
            d.vals[4].round().max(0.0) as usize,
        );
        if from < to && to <= spec.text.len() {
            let mut st = spec.style_at(from);
            f(&mut st);
            spec.set_style(from, to, st);
        } else {
            f(&mut spec.default_style);
            for r in &mut spec.runs {
                f(&mut r.style);
            }
        }
    }

    /// Replace the current document wholesale (Automate results,
    /// New): fresh history, no file, fitted view.
    fn swap_in_doc(&mut self, doc: Document, status: &str) {
        self.doc = doc;
        self.channel_view = ChannelView::Composite;
        self.paint_target = PaintTarget::Image;
        self.active_path = None;
        self.hist = History::new();
        self.hist.set_limit(Some(20));
        self.reset_hist_for_new_doc();
        self.last_label = None;
        self.sel = None;
        self.file_path = None;
        self.dirty = true;
        self.doc_version += 1;
        self.fit_view(self.win_size.0, self.win_size.1);
        self.status = status.into();
    }

    /// Image files (by extension) directly inside a folder, sorted.
    fn image_files_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                let ext = p
                    .extension()
                    .map(|x| x.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "pdk" | "psd"
                ) {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// File > Automate > Contact Sheet II: thumbnails of a
    /// folder's images in a grid, each captioned with its file
    /// name, as a new document.
    fn run_contact_sheet(&mut self, folder: &str, cols: usize, thumb: u32) {
        let dir = std::path::PathBuf::from(folder.trim());
        let files = Self::image_files_in(&dir);
        if files.is_empty() {
            self.status = format!("Contact Sheet: no images in '{}'", dir.display());
            return;
        }
        let cols = cols.max(1);
        let cell_w = thumb + 16;
        let cell_h = thumb + 34; // room for the caption
        let rows = files.len().div_ceil(cols);
        let (sw, sh) = (cell_w * cols as u32 + 16, cell_h * rows as u32 + 16);
        let mut sheet = Document::new(sw, sh);
        sheet.layers[0].fill_rect(0, 0, sw, sh, Pixel16::from_rgba8(255, 255, 255, 255));
        let mut placed = 0usize;
        for (i, path) in files.iter().enumerate() {
            let Ok(img) = io::load_file(path) else {
                continue;
            };
            let rgba = img.render_full_rgba8();
            let (iw, ih) = (img.width.max(1), img.height.max(1));
            let scale = (thumb as f32 / iw as f32).min(thumb as f32 / ih as f32);
            let (tw, th) = (
                ((iw as f32 * scale) as u32).max(1),
                ((ih as f32 * scale) as u32).max(1),
            );
            let cx = 8 + (i % cols) as u32 * cell_w + (thumb - tw) / 2;
            let cy = 8 + (i / cols) as u32 * cell_h;
            for yy in 0..th {
                for xx in 0..tw {
                    let sx = (xx as f32 / scale) as u32;
                    let sy = (yy as f32 / scale) as u32;
                    let idx = ((sy * iw + sx) * 4) as usize;
                    if idx + 3 < rgba.len() {
                        sheet.layers[0].set_pixel(
                            (cx + xx) as i32,
                            (cy + yy) as i32,
                            Pixel16::from_rgba8(
                                rgba[idx],
                                rgba[idx + 1],
                                rgba[idx + 2],
                                rgba[idx + 3],
                            ),
                        );
                    }
                }
            }
            // Caption: the file name, rendered through the type
            // engine on a scratch layer and copied over.
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut cap = Document::new(sw, sh);
            let mut spec = pictodesk_core::text::TextSpec::new(name);
            if let Some(fam) = typo::families().into_iter().next() {
                spec.default_style.font = fam;
            }
            spec.default_style.size = 12.0;
            cap.layers[0].offset = (cx as i32, (cy + thumb + 4) as i32);
            cap.layers[0].text = Some(spec);
            typo::render_type_layer(&mut cap, 0);
            if let Some((bx, by, ex, ey)) =
                pictodesk_core::transform::layer_painted_bounds(&cap.layers[0])
            {
                for yy in by..ey {
                    for xx in bx..ex {
                        let p = cap.layers[0].get_pixel(xx, yy);
                        if p.a > 0 {
                            sheet.layers[0].set_pixel(xx, yy, p);
                        }
                    }
                }
            }
            placed += 1;
        }
        self.swap_in_doc(
            sheet,
            &format!("Contact sheet: {placed} images from {}", dir.display()),
        );
    }

    /// File > Automate > Picture Package: the current image
    /// repeated on one page in a classic layout.
    fn run_picture_package(&mut self, page_w: u32, page_h: u32, layout: u8) {
        let src = self.doc.render_full_rgba8();
        let (iw, ih) = (self.doc.width.max(1), self.doc.height.max(1));
        let mut page = Document::new(page_w, page_h);
        page.layers[0].fill_rect(
            0,
            0,
            page_w,
            page_h,
            Pixel16::from_rgba8(255, 255, 255, 255),
        );
        // (cols, rows) per layout: 1-up, 2-up, 4-up, 8-up wallets.
        let (cols, rows) = match layout {
            0 => (1u32, 1u32),
            1 => (1, 2),
            2 => (2, 2),
            _ => (2, 4),
        };
        let margin = 24u32;
        let cell_w = (page_w - margin * 2) / cols;
        let cell_h = (page_h - margin * 2) / rows;
        for r in 0..rows {
            for c in 0..cols {
                let scale = ((cell_w - 8) as f32 / iw as f32).min((cell_h - 8) as f32 / ih as f32);
                let (tw, th) = (
                    ((iw as f32 * scale) as u32).max(1),
                    ((ih as f32 * scale) as u32).max(1),
                );
                let ox = margin + c * cell_w + (cell_w - tw) / 2;
                let oy = margin + r * cell_h + (cell_h - th) / 2;
                for yy in 0..th {
                    for xx in 0..tw {
                        let sx = (xx as f32 / scale) as u32;
                        let sy = (yy as f32 / scale) as u32;
                        let idx = ((sy * iw + sx) * 4) as usize;
                        if idx + 3 < src.len() {
                            page.layers[0].set_pixel(
                                (ox + xx) as i32,
                                (oy + yy) as i32,
                                Pixel16::from_rgba8(
                                    src[idx],
                                    src[idx + 1],
                                    src[idx + 2],
                                    src[idx + 3],
                                ),
                            );
                        }
                    }
                }
            }
        }
        self.swap_in_doc(page, "Picture Package page");
    }

    /// File > Automate > Web Photo Gallery: a folder of images
    /// becomes a static gallery (index.html + full images +
    /// thumbnails) in a `gallery` subfolder.
    fn run_web_gallery(&mut self, folder: &str, thumb: u32) {
        let dir = std::path::PathBuf::from(folder.trim());
        let files = Self::image_files_in(&dir);
        if files.is_empty() {
            self.status = format!("Web Gallery: no images in '{}'", dir.display());
            return;
        }
        let out = dir.join("gallery");
        let thumbs = out.join("thumbs");
        if std::fs::create_dir_all(&thumbs).is_err() {
            self.status = "Web Gallery: could not create the output folder".into();
            return;
        }
        let mut items = String::new();
        let mut made = 0usize;
        for path in &files {
            let Ok(img) = io::load_file(path) else {
                continue;
            };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "image".into());
            let full_name = format!("{name}.jpg");
            let thumb_name = format!("{name}.jpg");
            if let Ok(bytes) = io::export_image(&img, ExportFormat::Jpeg, 85) {
                if std::fs::write(out.join(&full_name), bytes).is_err() {
                    continue;
                }
            } else {
                continue;
            }
            // Thumbnail: scale down by re-rendering a resized copy.
            let mut small = img.clone();
            let scale = (thumb as f32 / img.width.max(1) as f32).min(1.0);
            if scale < 1.0 {
                let (nw, nh) = (
                    ((img.width as f32 * scale) as u32).max(1),
                    ((img.height as f32 * scale) as u32).max(1),
                );
                pictodesk_core::transform::resize_image(&mut small, nw, nh);
            }
            if let Ok(bytes) = io::export_image(&small, ExportFormat::Jpeg, 80) {
                let _ = std::fs::write(thumbs.join(&thumb_name), bytes);
            }
            items.push_str(&format!(
                "<a href=\"{full_name}\"><img src=\"thumbs/{thumb_name}\" alt=\"{name}\"></a>\n"
            ));
            made += 1;
        }
        let html = format!(
            "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Web Photo Gallery</title></head>\n<body><h1>Web Photo Gallery</h1>\n{items}</body></html>\n"
        );
        let _ = std::fs::write(out.join("index.html"), html);
        self.status = format!("Web gallery: {made} images in {}", out.display());
    }

    /// Apply a no-dialog filter to the active layer as one undo
    /// step, recording it when an Action is being recorded.
    fn filter_now(
        &mut self,
        label: &str,
        step: Option<RecordedStep>,
        f: impl Fn(&mut Document, usize, Option<&Selection>),
    ) {
        let layer = self.doc.active_layer;
        let tiles = self.sel_tiles();
        let sel = self.sel.clone();
        run_tile_edit(&mut self.doc, &mut self.hist, label, layer, &tiles, |d| {
            f(d, layer, sel.as_ref())
        });
        if let Some(s) = step {
            self.record_step(s);
        }
        self.last_label = Some(label.into());
        self.status = label.into();
        self.touch();
    }

    /// File > Export > PSD: write a layered PSD beside the
    /// document's file (or the working directory), surfacing the
    /// exporter's honesty warnings in the status line.
    fn export_psd(&mut self) {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "untitled".into());
        let path = PathBuf::from(format!("{name}.psd"));
        let (bytes, report) = pictodesk_core::psd::save_psd(&self.doc);
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                self.status = if report.warnings.is_empty() {
                    format!("Exported {}", path.display())
                } else {
                    format!(
                        "Exported {} — {}",
                        path.display(),
                        report.warnings.join("; ")
                    )
                };
            }
            Err(e) => self.status = format!("PSD export failed: {e}"),
        }
    }

    /// File > Export > EPS DCS 2.0: the master EPS plus one plate
    /// file per ink, beside the document.
    fn export_dcs(&mut self) {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "untitled".into());
        let files = io::export_dcs(&self.doc, &name);
        let mut written = 0usize;
        for (fname, bytes) in &files {
            if std::fs::write(fname, bytes).is_ok() {
                written += 1;
            }
        }
        self.status = format!("DCS 2.0: wrote {written} of {} files", files.len());
    }

    /// Render the print page: the document centered on the paper
    /// at the Page Setup scale, as an RGBA buffer.
    fn print_page_rgba(&self) -> (Vec<u8>, u32, u32) {
        let (pw, ph) = if self.page_landscape {
            (self.page_h as u32, self.page_w as u32)
        } else {
            (self.page_w as u32, self.page_h as u32)
        };
        let mut page = vec![255u8; (pw * ph * 4) as usize];
        let img = self.doc.render_full_rgba8();
        let (iw, ih) = (self.doc.width as f32, self.doc.height as f32);
        let scale = self.page_scale / 100.0;
        let (tw, th) = (iw * scale, ih * scale);
        let ox = ((pw as f32 - tw) / 2.0).max(0.0);
        let oy = ((ph as f32 - th) / 2.0).max(0.0);
        for yy in 0..(th as u32) {
            for xx in 0..(tw as u32) {
                let (dx, dy) = ((ox as u32 + xx), (oy as u32 + yy));
                if dx >= pw || dy >= ph {
                    continue;
                }
                let sx = (xx as f32 / scale) as u32;
                let sy = (yy as f32 / scale) as u32;
                let si = ((sy * iw as u32 + sx) * 4) as usize;
                let di = ((dy * pw + dx) * 4) as usize;
                if si + 3 < img.len() {
                    let a = img[si + 3] as f32 / 255.0;
                    for c in 0..3 {
                        page[di + c] =
                            (img[si + c] as f32 * a + page[di + c] as f32 * (1.0 - a)) as u8;
                    }
                    page[di + 3] = 255;
                }
            }
        }
        (page, pw, ph)
    }

    /// Print: build a one-page PDF (JPEG image XObject) in the
    /// temp dir and hand it to the platform print spooler.
    fn do_print(&mut self, copies: u32) {
        let (rgba, pw, ph) = self.print_page_rgba();
        // JPEG-encode the page through a scratch document.
        let mut scratch = Document::new(pw, ph);
        for yy in 0..ph as i32 {
            for xx in 0..pw as i32 {
                let i = ((yy * pw as i32 + xx) * 4) as usize;
                scratch.layers[0].set_pixel(
                    xx,
                    yy,
                    Pixel16::from_rgba8(rgba[i], rgba[i + 1], rgba[i + 2], 255),
                );
            }
        }
        let Ok(jpeg) = io::export_image(&scratch, ExportFormat::Jpeg, 92) else {
            self.status = "Print: could not render the page".into();
            return;
        };
        let pdf = build_pdf_one_page(&jpeg, pw, ph);
        let path = std::env::temp_dir().join("pictodesk-print.pdf");
        if std::fs::write(&path, pdf).is_err() {
            self.status = "Print: could not write the print file".into();
            return;
        }
        let status = spool_print(&path, copies);
        self.status = status;
    }

    /// 3D Transform dialog: the wireframe preview rect inside
    /// the dialog (below the shape buttons, above the sliders).
    fn wireframe_rect(&self) -> Option<Rect> {
        let layout = self.dialog_layout()?;
        let r = layout.rect;
        Some(Rect::new(r.x + 12, r.y + 54, r.w - 24, 188))
    }

    /// Extract dialog: the mini-editor rect inside the dialog.
    fn extract_editor_rect(&self) -> Option<Rect> {
        let layout = self.dialog_layout()?;
        let r = layout.rect;
        Some(Rect::new(r.x + 12, r.y + 30, r.w - 24, 276))
    }

    /// Map a window point in the Extract editor to doc coords.
    fn extract_doc_point(&self, mx: i32, my: i32) -> Option<(i32, i32)> {
        let er = self.extract_editor_rect()?;
        if !er.contains(mx, my) {
            return None;
        }
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        let scale = (er.w as f32 / w).min(er.h as f32 / h);
        let (ox, oy) = (
            er.x as f32 + (er.w as f32 - w * scale) / 2.0,
            er.y as f32 + (er.h as f32 - h * scale) / 2.0,
        );
        let dx = ((mx as f32 - ox) / scale).floor() as i32;
        let dy = ((my as f32 - oy) / scale).floor() as i32;
        if dx >= 0 && dy >= 0 && (dx as u32) < self.doc.width && (dy as u32) < self.doc.height {
            Some((dx, dy))
        } else {
            None
        }
    }

    /// Paint one Extract tool dab at a doc point.
    fn extract_paint(&mut self, dp: (i32, i32)) {
        let mut on_highlight = false;
        {
            let Some(d) = &mut self.dialog else { return };
            if d.kind != DialogKind::Extract {
                return;
            }
            let (w, h) = (self.doc.width as i32, self.doc.height as i32);
            let idx = |x: i32, y: i32| (y * w + x) as usize;
            match d.extract_mode {
                0 => {
                    // Highlight brush: a soft disc of edge marks.
                    for yy in (dp.1 - 6)..=(dp.1 + 6) {
                        for xx in (dp.0 - 6)..=(dp.0 + 6) {
                            if xx >= 0 && yy >= 0 && xx < w && yy < h {
                                let dd = (xx - dp.0) * (xx - dp.0) + (yy - dp.1) * (yy - dp.1);
                                if dd <= 36 {
                                    d.extract_hi[idx(xx, yy)] = 255;
                                }
                            }
                        }
                    }
                }
                1 => {
                    // Fill: flood the region the highlight encloses.
                    if dp.0 < 0 || dp.1 < 0 || dp.0 >= w || dp.1 >= h {
                        return;
                    }
                    if d.extract_hi[idx(dp.0, dp.1)] > 0 {
                        on_highlight = true;
                    }
                    let mut stack = vec![(dp.0, dp.1)];
                    while let Some((cx, cy)) = stack.pop() {
                        if cx < 0 || cy < 0 || cx >= w || cy >= h {
                            continue;
                        }
                        let i = idx(cx, cy);
                        if d.extract_hi[i] > 0 || d.extract_fill[i] > 0 {
                            continue;
                        }
                        d.extract_fill[i] = 255;
                        stack.push((cx - 1, cy));
                        stack.push((cx + 1, cy));
                        stack.push((cx, cy - 1));
                        stack.push((cx, cy + 1));
                    }
                }
                _ => {
                    // Eraser: clear both masks under the brush.
                    for yy in (dp.1 - 6)..=(dp.1 + 6) {
                        for xx in (dp.0 - 6)..=(dp.0 + 6) {
                            if xx >= 0 && yy >= 0 && xx < w && yy < h {
                                let dd = (xx - dp.0) * (xx - dp.0) + (yy - dp.1) * (yy - dp.1);
                                if dd <= 36 {
                                    d.extract_hi[idx(xx, yy)] = 0;
                                    d.extract_fill[idx(xx, yy)] = 0;
                                }
                            }
                        }
                    }
                }
            }
        }
        if on_highlight {
            self.status = "Fill inside the highlighted edge".into();
        }
        self.extract_preview_cache = None;
        if self
            .dialog
            .as_ref()
            .map(|d| d.extract_preview)
            .unwrap_or(false)
        {
            self.compute_extract_preview();
        }
    }

    /// Recompute the Extract preview (the filter applied to a
    /// scratch copy of the document).
    fn compute_extract_preview(&mut self) {
        let Some(d) = &self.dialog else { return };
        if d.kind != DialogKind::Extract {
            return;
        }
        let (hi, fill) = (d.extract_hi.clone(), d.extract_fill.clone());
        let mut scratch = self.doc.clone();
        let layer = scratch.active_layer;
        filter::extract(&mut scratch, layer, &hi, &fill);
        self.extract_preview_cache = Some(scratch.render_full_rgba8());
    }

    /// Layer > Effects: open the shared effects dialog at a page.
    fn open_effects_dialog(&mut self, page: usize) {
        let li = self.doc.active_layer;
        if self.doc.layers[li].adjustment.is_some() {
            self.status = "Adjustment layers cannot carry effects".into();
            return;
        }
        if li == 0 && self.doc.layers[0].name == "Background" {
            // The classic forbids effects on the Background layer.
            self.status = "Effects need a regular layer (the Background cannot carry them)".into();
            return;
        }
        let mut d = Dialog::new(DialogKind::Effects, &self.doc);
        d.adj_layer = Some(li);
        d.adj_snapshot = Some(self.doc.clone());
        d.fx = self.doc.layers[li].effects.unwrap_or_default();
        d.fx_page = page;
        self.dialog = Some(d);
        self.fx_load_vals();
    }

    /// Load the Effects dialog sliders from the current page's spec.
    fn fx_load_vals(&mut self) {
        let Some(d) = &mut self.dialog else { return };
        let fx = d.fx;
        d.vals = match d.fx_page {
            0 => vec![
                fx.drop_shadow.opacity * 100.0,
                fx.drop_shadow.angle as f32,
                fx.drop_shadow.distance as f32,
                fx.drop_shadow.blur as f32,
                fx.drop_shadow.intensity as f32,
            ],
            1 => vec![
                fx.inner_shadow.opacity * 100.0,
                fx.inner_shadow.angle as f32,
                fx.inner_shadow.distance as f32,
                fx.inner_shadow.blur as f32,
                fx.inner_shadow.intensity as f32,
            ],
            2 => vec![
                fx.outer_glow.opacity * 100.0,
                fx.outer_glow.blur as f32,
                fx.outer_glow.intensity as f32,
            ],
            3 => vec![
                fx.inner_glow.opacity * 100.0,
                fx.inner_glow.blur as f32,
                fx.inner_glow.intensity as f32,
            ],
            _ => vec![
                fx.bevel.depth as f32,
                fx.bevel.angle as f32,
                fx.bevel.hi_opacity * 100.0,
                fx.bevel.sh_opacity * 100.0,
            ],
        };
    }

    /// Write the Effects dialog sliders back into the spec and
    /// apply it to the layer (live preview; the dialog's snapshot
    /// makes Cancel exact and OK a single undo step).
    fn fx_sync_from_vals(&mut self) {
        let Some(d) = &mut self.dialog else { return };
        let v = d.vals.clone();
        let fx = &mut d.fx;
        match d.fx_page {
            0 => {
                fx.drop_shadow.opacity = v[0] / 100.0;
                fx.drop_shadow.angle = v[1] as i32;
                fx.drop_shadow.distance = v[2].max(0.0) as u32;
                fx.drop_shadow.blur = v[3].max(0.0) as u32;
                fx.drop_shadow.intensity = v[4].max(0.0) as u32;
            }
            1 => {
                fx.inner_shadow.opacity = v[0] / 100.0;
                fx.inner_shadow.angle = v[1] as i32;
                fx.inner_shadow.distance = v[2].max(0.0) as u32;
                fx.inner_shadow.blur = v[3].max(0.0) as u32;
                fx.inner_shadow.intensity = v[4].max(0.0) as u32;
            }
            2 => {
                fx.outer_glow.opacity = v[0] / 100.0;
                fx.outer_glow.blur = v[1].max(0.0) as u32;
                fx.outer_glow.intensity = v[2].max(0.0) as u32;
            }
            3 => {
                fx.inner_glow.opacity = v[0] / 100.0;
                fx.inner_glow.blur = v[1].max(0.0) as u32;
                fx.inner_glow.intensity = v[2].max(0.0) as u32;
            }
            _ => {
                fx.bevel.depth = v[0].max(0.0) as u32;
                fx.bevel.angle = v[1] as i32;
                if v.len() > 3 {
                    fx.bevel.hi_opacity = v[2] / 100.0;
                    fx.bevel.sh_opacity = v[3] / 100.0;
                }
            }
        }
        let fx = d.fx;
        if let Some(li) = d.adj_layer {
            self.doc.layers[li].effects = if fx == pictodesk_core::effects::Effects::default() {
                None
            } else {
                Some(fx)
            };
            self.doc_version += 1;
        }
    }

    /// Layer > Effects > Create Layers: bake the below-layer
    /// effects (Drop Shadow, Outer Glow) into real layers under
    /// the effected layer, and switch those two off in the spec.
    /// Inner effects stay live (they modify the content itself).
    fn create_effect_layers(&mut self) {
        use pictodesk_core::effects::{self, Effects};
        use pictodesk_core::layer::Layer;
        let li = self.doc.active_layer;
        let Some(fx) = self.doc.layers[li].effects else {
            self.status = "The active layer has no effects".into();
            return;
        };
        if !fx.has_below() {
            self.status = "Only Drop Shadow and Outer Glow become layers".into();
            return;
        }
        let name = self.doc.layers[li].name.clone();
        self.structural("Create Effect Layers", |doc| {
            let (w, h) = (doc.width as usize, doc.height as usize);
            let angle = doc.global_angle;
            // The layer's own content as an RGB buffer (the same
            // conversion the compositor's effects path uses).
            let mut content = vec![Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0); w * h];
            for yy in 0..h as i32 {
                for xx in 0..w as i32 {
                    let idx = (yy as usize) * w + xx as usize;
                    content[idx] = if doc.mode == ColorMode::Cmyk {
                        let (c, m, y, k, a) = doc.get_cmyk(li, xx, yy);
                        let f = |v: u16| v as f32 / 65535.0;
                        let (r, g, b) = pictodesk_core::color::cmyk_to_rgb(f(c), f(m), f(y), f(k));
                        Pixel16::from_f32s(r, g, b, f(a))
                    } else {
                        doc.layers[li].get_pixel(xx, yy)
                    };
                }
            }
            let mk_layer = |label: &str, only: Effects| -> Option<Layer> {
                let buf = effects::below_buffer(&content, w, h, &only, angle);
                if buf.iter().all(|p| p.a == 0) {
                    return None;
                }
                let mut l = Layer::new(format!("{name} {label}"));
                for yy in 0..h as i32 {
                    for xx in 0..w as i32 {
                        let p = buf[(yy as usize) * w + xx as usize];
                        if p.a > 0 {
                            l.set_pixel(xx, yy, p);
                        }
                    }
                }
                Some(l)
            };
            // Stack (bottom to top): Outer Glow, Drop Shadow,
            // content — matching below_buffer's paint order.
            let shadow_fx = Effects {
                drop_shadow: fx.drop_shadow,
                ..Default::default()
            };
            let glow_fx = Effects {
                outer_glow: fx.outer_glow,
                ..Default::default()
            };
            let mut above = 0usize;
            if fx.drop_shadow.enabled {
                if let Some(l) = mk_layer("Drop Shadow", shadow_fx) {
                    doc.layers.insert(li, l);
                    above += 1;
                }
            }
            if fx.outer_glow.enabled {
                if let Some(l) = mk_layer("Outer Glow", glow_fx) {
                    doc.layers.insert(li, l);
                    above += 1;
                }
            }
            // The content layer moved up past the inserted layers;
            // switch its baked effects off.
            let ci = li + above;
            if let Some(e) = &mut doc.layers[ci].effects {
                e.drop_shadow.enabled = false;
                e.outer_glow.enabled = false;
                if !e.any_enabled() && *e == Effects::default() {
                    doc.layers[ci].effects = None;
                }
            }
            doc.active_layer = ci;
        });
        self.status = "Effects baked into layers".into();
    }

    /// One click on an Effects dialog toggle button.
    fn fx_button_click(&mut self, i: usize) {
        use pictodesk_core::blend::ALL_BLEND_MODES;
        let fg = self.fg;
        let Some(d) = &mut self.dialog else { return };
        let fx = &mut d.fx;
        let next_mode = |m: pictodesk_core::blend::BlendMode| {
            let pos = ALL_BLEND_MODES.iter().position(|x| *x == m).unwrap_or(0);
            ALL_BLEND_MODES[(pos + 1) % ALL_BLEND_MODES.len()]
        };
        match (d.fx_page, i) {
            (0, 0) => fx.drop_shadow.enabled = !fx.drop_shadow.enabled,
            (1, 0) => fx.inner_shadow.enabled = !fx.inner_shadow.enabled,
            (2, 0) => fx.outer_glow.enabled = !fx.outer_glow.enabled,
            (3, 0) => fx.inner_glow.enabled = !fx.inner_glow.enabled,
            (4, 0) => fx.bevel.enabled = !fx.bevel.enabled,
            (0, 1) => fx.drop_shadow.mode = next_mode(fx.drop_shadow.mode),
            (1, 1) => fx.inner_shadow.mode = next_mode(fx.inner_shadow.mode),
            (2, 1) => fx.outer_glow.mode = next_mode(fx.outer_glow.mode),
            (3, 1) => fx.inner_glow.mode = next_mode(fx.inner_glow.mode),
            (4, 1) => fx.bevel.style = (fx.bevel.style + 1) % 4,
            (0, 2) => fx.drop_shadow.use_global_angle = !fx.drop_shadow.use_global_angle,
            (1, 2) => fx.inner_shadow.use_global_angle = !fx.inner_shadow.use_global_angle,
            (3, 2) => fx.inner_glow.source_center = !fx.inner_glow.source_center,
            (4, 2) => fx.bevel.up = !fx.bevel.up,
            (0, 3) => fx.drop_shadow.color = fg,
            (1, 3) => fx.inner_shadow.color = fg,
            (2, 3) => fx.outer_glow.color = fg,
            (3, 3) => fx.inner_glow.color = fg,
            (4, 3) => {
                if d.opt_a {
                    fx.bevel.sh_color = fg;
                    d.opt_a = false;
                } else {
                    fx.bevel.hi_color = fg;
                    d.opt_a = true;
                }
            }
            (4, 4) => fx.bevel.hi_mode = next_mode(fx.bevel.hi_mode),
            (4, 5) => fx.bevel.sh_mode = next_mode(fx.bevel.sh_mode),
            (4, 6) => fx.bevel.use_global_angle = !fx.bevel.use_global_angle,
            _ => {}
        }
        self.fx_sync_from_vals();
    }

    /// Close the in-progress polygonal selection.
    fn close_poly_selection(&mut self) {
        if self.poly_points.len() >= 3 {
            let p = Selection::polygon(self.doc.width, self.doc.height, &self.poly_points);
            self.replace_sel(Some(p));
            self.status = "Selection closed".into();
        }
        self.poly_points.clear();
    }

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
            Tool::Eraser => {
                if self.eraser_to_history {
                    match self.hist_source_doc() {
                        Some(src) => brush::stamp_sample(
                            &mut self.doc,
                            layer,
                            sel.as_ref(),
                            center.0,
                            center.1,
                            &tip,
                            opacity,
                            &|x, y| src.render_pixel(x, y),
                        ),
                        None => (0, 0, 0, 0),
                    }
                } else {
                    brush::stamp_eraser(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        &tip,
                        opacity,
                    )
                }
            }
            Tool::HistoryBrush => match self.hist_source_doc() {
                Some(src) => brush::stamp_sample(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    &tip,
                    opacity,
                    &|x, y| src.render_pixel(x, y),
                ),
                None => (0, 0, 0, 0),
            },
            Tool::ArtHistoryBrush => match self.hist_source_doc() {
                Some(src) => {
                    // Art History: the sample point wanders around
                    // the brush center; the style sets how far.
                    let mag = match self.art_hist_style {
                        0 => 2.0,
                        1 => 6.0,
                        _ => 3.0,
                    };
                    let seed = (center.0 * 7919.0 + center.1 * 104729.0) as i64;
                    let jx = ((seed % 13) as f32 / 6.0 - 1.0) * mag;
                    let jy = (((seed / 13) % 11) as f32 / 5.0 - 1.0) * mag;
                    let tip2 = if self.art_hist_style == 2 {
                        BrushTip::new(tip.size * 1.5, tip.hardness)
                    } else {
                        tip
                    };
                    brush::stamp_sample(
                        &mut self.doc,
                        layer,
                        sel.as_ref(),
                        center.0,
                        center.1,
                        &tip2,
                        opacity,
                        &|x, y| src.render_pixel(x + jx as i32, y + jy as i32),
                    )
                }
                None => (0, 0, 0, 0),
            },
            Tool::PatternStamp => brush::stamp_pattern(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                opacity,
            )
            .unwrap_or((0, 0, 0, 0)),
            Tool::BackgroundEraser => {
                let sample = self.doc.layers[layer].get_pixel(center.0 as i32, center.1 as i32);
                brush::stamp_erase_bg(
                    &mut self.doc,
                    layer,
                    sel.as_ref(),
                    center.0,
                    center.1,
                    &tip,
                    opacity,
                    sample,
                    self.eraser_tolerance,
                )
            }
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
            Tool::Dodge | Tool::Burn | Tool::Sponge => brush::stamp_tone(
                &mut self.doc,
                layer,
                sel.as_ref(),
                center.0,
                center.1,
                &tip,
                match tool {
                    Tool::Dodge => brush::ToneKind::Dodge,
                    Tool::Burn => brush::ToneKind::Burn,
                    _ => brush::ToneKind::Sponge,
                },
                self.tone_range,
                self.tone_exposure,
                self.sponge_saturate,
                opacity,
            ),
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
        if self.drag == DragMode::NavDrag {
            self.navigator_to(mx, my);
            return;
        }
        if self.drag == DragMode::GuideDrag {
            let dp = self.to_doc(x, y);
            if let Some(g) = &mut self.guide_drag {
                g.1 = if g.0 { dp.0 as i32 } else { dp.1 as i32 };
            }
            return;
        }
        if self.drag == DragMode::MeasureDrag {
            let dp = self.to_doc(x, y);
            if let Some(m) = &mut self.measure {
                m.1 = dp;
            }
            return;
        }
        if self.drag == DragMode::MagLasso || self.drag == DragMode::MagPen {
            let dp = self.to_doc(x, y);
            let far = self
                .poly_points
                .last()
                .map(|l| (dp.0 - l.0).abs() > 8.0 || (dp.1 - l.1).abs() > 8.0)
                .unwrap_or(true);
            if far {
                let p = self.mag_snap(dp);
                self.poly_points.push(p);
            }
            return;
        }
        if self.drag == DragMode::FreePen {
            let dp = self.to_doc(x, y);
            let far = self
                .pen_free
                .last()
                .map(|l| (dp.0 - l.0).abs() > 2.0 || (dp.1 - l.1).abs() > 2.0)
                .unwrap_or(true);
            if far {
                self.pen_free.push(dp);
            }
            return;
        }
        if self.ft.is_some() {
            let dp = self.to_doc(x, y);
            self.ft_mouse_move(dp);
            return;
        }
        // dialog drags
        if self.dialog.is_some() {
            let layout = self.dialog_layout().unwrap();
            // Extract editor: drag paints with the current tool.
            if self.mouse_down
                && matches!(
                    self.dialog.as_ref().map(|d| &d.kind),
                    Some(DialogKind::Extract)
                )
            {
                if let Some(dp) = self.extract_doc_point(mx, my) {
                    let mode = self.dialog.as_ref().unwrap().extract_mode;
                    if mode != 1 {
                        self.extract_paint(dp);
                    }
                }
                return;
            }
            // 3D Transform wireframe drag: horizontal movement
            // turns the solid around Y, vertical around X, at
            // half a degree per pixel, wrapping at +-180 so the
            // drag never sticks at the slider ends. The vals
            // feed both the sliders and the live preview.
            let wire_moved = {
                let d = self.dialog.as_mut().unwrap();
                if let Some((lx, ly)) = d.wire_drag {
                    let (dx, dy) = (mx - lx, my - ly);
                    d.wire_drag = Some((mx, my));
                    let wrap = |v: f32| ((v + 180.0).rem_euclid(360.0)) - 180.0;
                    d.vals[1] = wrap(d.vals[1] + dx as f32 * 0.5);
                    d.vals[0] = wrap(d.vals[0] + dy as f32 * 0.5);
                    true
                } else {
                    false
                }
            };
            if wire_moved {
                self.dialog_refresh_preview();
                return;
            }
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
                let mut dx = (dp.0 - self.drag_start_doc.0).round() as i32;
                let mut dy = (dp.1 - self.drag_start_doc.1).round() as i32;
                if self.snap_enabled {
                    // Snap the grab point to guides (and to the grid
                    // while it's shown) within a few screen pixels.
                    let thresh = 6.0 / self.viewport.scale.max(0.01);
                    let grab = (
                        self.drag_start_doc.0 + dx as f32,
                        self.drag_start_doc.1 + dy as f32,
                    );
                    let mut best: Option<(f32, f32)> = None;
                    let mut consider = |cand: f32, is_x: bool| {
                        let g = if is_x { grab.0 } else { grab.1 };
                        let d = (cand - g).abs();
                        if d <= thresh && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                            best = Some((d, cand));
                        }
                    };
                    for g in &self.doc.guides_x {
                        consider(*g as f32, true);
                    }
                    if self.show_grid {
                        let step = 64.0f32;
                        consider((grab.0 / step).round() * step, true);
                    }
                    if let Some((_, cand)) = best {
                        dx = (cand - self.drag_start_doc.0).round() as i32;
                    }
                    let mut besty: Option<(f32, f32)> = None;
                    for g in &self.doc.guides_y {
                        let d = (*g as f32 - grab.1).abs();
                        if d <= thresh && besty.map(|(bd, _)| d < bd).unwrap_or(true) {
                            besty = Some((d, *g as f32));
                        }
                    }
                    if self.show_grid {
                        let step = 64.0f32;
                        let cand = (grab.1 / step).round() * step;
                        let d = (cand - grab.1).abs();
                        if d <= thresh && besty.map(|(bd, _)| d < bd).unwrap_or(true) {
                            besty = Some((d, cand));
                        }
                    }
                    if let Some((_, cand)) = besty {
                        dy = (cand - self.drag_start_doc.1).round() as i32;
                    }
                }
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
                            if matches!(self.paint_target, PaintTarget::Mask(_)) {
                                self.mask_stamp(c);
                            } else {
                                self.alpha_stamp(c);
                            }
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
        if self.drag == DragMode::GuideDrag {
            self.drag = DragMode::None;
            if let Some((vertical, pos, existing)) = self.guide_drag.take() {
                // Dropping over the canvas keeps the guide; dropping
                // outside removes it (for an existing guide, deletes).
                let c = self.canvas_rect(self.win_size.0, self.win_size.1);
                let inside = c.contains(_x as i32, _y as i32);
                match (existing, inside) {
                    (None, true) => {
                        self.structural("Add Guide", |d| {
                            if vertical {
                                d.guides_x.push(pos);
                            } else {
                                d.guides_y.push(pos);
                            }
                        });
                        self.status = "Guide added — drag it with the Move tool".into();
                    }
                    (Some(i), true) => {
                        self.structural("Move Guide", |d| {
                            if vertical {
                                if i < d.guides_x.len() {
                                    d.guides_x[i] = pos;
                                }
                            } else if i < d.guides_y.len() {
                                d.guides_y[i] = pos;
                            }
                        });
                    }
                    (Some(i), false) => {
                        self.structural("Delete Guide", |d| {
                            if vertical {
                                if i < d.guides_x.len() {
                                    d.guides_x.remove(i);
                                }
                            } else if i < d.guides_y.len() {
                                d.guides_y.remove(i);
                            }
                        });
                    }
                    (None, false) => {}
                }
            }
            return;
        }
        if self.drag == DragMode::NavDrag {
            self.drag = DragMode::None;
            return;
        }
        if let Some(d) = &mut self.dialog {
            d.slider_drag = None;
            d.curve_drag = None;
            d.wire_drag = None;
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
                    self.replace_sel(None);
                } else if self.tool == Tool::MarqueeEllipse {
                    self.replace_sel(Some(Selection::ellipse(
                        self.doc.width,
                        self.doc.height,
                        x0 as i32,
                        y0 as i32,
                        (x1 - x0) as u32,
                        (y1 - y0) as u32,
                    )));
                } else {
                    self.replace_sel(Some(Selection::rect(
                        self.doc.width,
                        self.doc.height,
                        x0 as i32,
                        y0 as i32,
                        (x1 - x0) as u32,
                        (y1 - y0) as u32,
                    )));
                }
            }
            DragMode::Lasso => {
                if self.lasso_points.len() >= 3 {
                    let p = Selection::polygon(self.doc.width, self.doc.height, &self.lasso_points);
                    self.replace_sel(Some(p));
                } else {
                    self.replace_sel(None);
                }
                self.lasso_points.clear();
            }
            DragMode::MagLasso => {
                // Close on the start anchor when the drag ends
                // near it; otherwise close straight across.
                self.close_poly_selection();
            }
            DragMode::MagPen => {
                let pts = std::mem::take(&mut self.poly_points);
                if pts.len() >= 2 {
                    if let Some(p) = self.target_path_mut() {
                        p.subpaths.push(Subpath {
                            anchors: pts.iter().map(|q| Anchor::corner(q.0, q.1)).collect(),
                            closed: false,
                        });
                    }
                    self.commit_path_edit("Magnetic Pen");
                    self.status = "Magnetic Pen path".into();
                    self.touch();
                } else {
                    self.path_before = None;
                }
            }
            DragMode::FreePen => {
                let pts = std::mem::take(&mut self.pen_free);
                if pts.len() >= 2 {
                    let anchors = pictodesk_core::path::fit_open_contour(&pts);
                    if anchors.len() >= 2 {
                        if let Some(p) = self.target_path_mut() {
                            p.subpaths.push(Subpath {
                                anchors,
                                closed: false,
                            });
                        }
                        self.commit_path_edit("Freeform Pen");
                        self.status = "Freeform Pen path".into();
                        self.touch();
                    } else {
                        self.path_before = None;
                    }
                } else {
                    self.path_before = None;
                }
            }
            DragMode::GradientLine => {
                let layer = self.doc.active_layer;
                let tiles = self.sel_tiles();
                let sel = self.sel.clone();
                let (from, to, fg, bg, op, shape) = (
                    self.drag_start_doc,
                    self.drag_cur_doc,
                    self.fg,
                    self.bg,
                    self.opacity,
                    self.gradient_shape,
                );
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Gradient",
                    layer,
                    &tiles,
                    |d| {
                        brush::gradient_shaped(d, layer, sel.as_ref(), from, to, fg, bg, op, shape);
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
        // Custom Filter kernel entry: digits build the selected
        // cell's value, '-' flips its sign.
        if !primary
            && self
                .dialog
                .as_ref()
                .map(|d| d.kind == DialogKind::CustomFilter)
                .unwrap_or(false)
        {
            let mut changed = false;
            {
                let d = self.dialog.as_mut().unwrap();
                let cell = &mut d.kernel[d.kernel_sel];
                if c.is_ascii_digit() {
                    let sign = if *cell < 0 { -1 } else { 1 };
                    *cell = sign * ((*cell).abs() * 10 + c.to_digit(10).unwrap() as i32);
                    *cell = (*cell).clamp(-999, 999);
                    changed = true;
                } else if c == '-' {
                    *cell = -*cell;
                    changed = true;
                }
            }
            if changed {
                self.dialog_refresh_preview();
            }
            return;
        }
        // dialog text entry
        if let Some(d) = &mut self.dialog {
            if d.text_focus && !primary {
                d.text.push(c);
            }
            return;
        }
        if primary {
            match c.to_ascii_lowercase() {
                'c' if self.shift_down => self.do_action(Action::CopyMerged),
                'v' if self.shift_down => self.do_action(Action::PasteInto),
                'c' => self.do_action(Action::Copy),
                'x' => self.do_action(Action::Cut),
                'v' => self.do_action(Action::Paste),
                'n' if self.shift_down => self.do_action(Action::NewLayer),
                'e' => self.do_action(Action::MergeDown),
                't' => self.do_action(Action::FreeTransform),
                ']' => self.do_action(Action::BringForward),
                '[' => self.do_action(Action::SendBackward),
                'z' if self.shift_down => self.do_action(Action::Redo),
                'z' => self.do_action(Action::Undo),
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
                } else if !self.poly_points.is_empty() {
                    self.poly_points.clear();
                    self.status = "Selection cancelled".into();
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
                    self.replace_sel(None);
                }
            }
            SpecialKey::Enter => {
                if self.ft.is_some() {
                    self.ft_commit();
                } else if self.shift_down
                    && matches!(
                        self.dialog.as_ref().map(|d| &d.kind),
                        Some(DialogKind::Type)
                    )
                {
                    // Shift+Enter inside the Type dialog = newline.
                    if let Some(d) = &mut self.dialog {
                        d.text.push('\n');
                    }
                } else if self.dialog.is_some() {
                    self.dialog_ok();
                } else if self.tool == Tool::PolyLasso && !self.poly_points.is_empty() {
                    self.close_poly_selection();
                } else if self.tool == Tool::Pen {
                    self.status = "Path finished".into();
                }
            }
            SpecialKey::Backspace => {
                if self
                    .dialog
                    .as_ref()
                    .map(|d| d.kind == DialogKind::CustomFilter)
                    .unwrap_or(false)
                {
                    {
                        let d = self.dialog.as_mut().unwrap();
                        let sel = d.kernel_sel;
                        d.kernel[sel] /= 10;
                    }
                    self.dialog_refresh_preview();
                    return;
                }
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
    let mut demo4 = false;
    let mut ctx = false;
    let mut batchtest = false;
    let mut gaptest = false;
    let mut phase5test = false;
    let mut tone = false;
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
            "--demo4" => demo4 = true,
            "--ctx" => ctx = true,
            "--batchtest" => batchtest = true,
            "--gaptest" => gaptest = true,
            "--phase5test" => phase5test = true,
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
            "3d" => DialogKind::Transform3d,
            _ => DialogKind::HueSaturation,
        };
        if !matches!(
            name.as_str(),
            "type" | "effects" | "extract" | "historypanel"
        ) {
            app.open_dialog(kind);
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
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                let (x, y) = self.app.cursor;
                self.app.right_click(x, y);
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
