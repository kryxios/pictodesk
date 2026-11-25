//! Commands and persistence: the Action enum, recorded steps, shortcuts, exports, and do_action.
use super::*;
use crate::dialogs::{Dialog, DialogKind};
use crate::panels::{HistRow, HistSource};
use crate::tools::{BrushDynamics, ChannelView, PaintTarget, Tool};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    New,
    OpenPanel,
    Save,
    SaveAsPanel,
    HistoryLog,
    ExportPng,
    Revert,
    CloseTab,
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
    ModeLab,
    ModeMultichannel,
    BitmapDialog,
    IndexedDialog,
    DuotoneDialog,
    ColorTableDialog,
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
    AutoLevelsCmd,
    AutoContrastCmd,
    AutoColorCmd,
    PatternMakerDialog,
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
    // Phase 6.5 Batch 3 actions.
    ScreenMode(u8),
    ToggleOptionsBar,
    FadeDialog,
    StrokeDialog,
    RotateArbitraryDialog,
    FileInfoDialog,
    PreferencesDialog,
    DefringeDialog,
    SaveCopyPanel,
    DuplicateImage,
    ExportAi,
    RemoveBlackMatte,
    RemoveWhiteMatte,
    DefineBrush,
    LastFilter,
    SelectiveColorDialog,
    ReplaceColorDialog,
    VariationsDialog,
    ApplyImageDialog,
    CalculationsDialog,
    SplitChannels,
    MergeChannelsDialog,
    LoadSelectionDialog,
    ChannelOptionsDialog,
    QuickMaskToggle,
    MergeGroup,
    PurgeHistories,
    PurgeClipboard,
    PurgeAll,
    ToggleHistogram,
    ToggleBrushes,
    ToggleToolPresets,
    SpellCheckDialog,
    FindReplaceDialog,
    FileBrowserDialog,
    SaveWorkspaceDialog,
    ApplyWorkspace(usize),
    DeleteWorkspace(usize),
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
    // Phase 6 actions.
    BlendingDialog,
    LiquifyDialog,
    NewLayerSet,
    DeleteLayerSet,
    AssignSet(u16),
    ToggleClipped,
    LayerColor(u8),
    LockToggle(u8),
    RasterizeLayer,
    DefineCustomShape,
    PresetManagerDialog,
    CreateDroplet,
    ExportSlices,
    SlicePropsDialog,
    NewFillLayer(u8),
    PerspectiveCrop,
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
    TextureFillPick,
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
    ZoomPrintSize,
    ToggleLockGuides,
    ToggleGamut,
    ToggleShowPath,
    TogglePathEdges,
    ZoomIn,
    ZoomOut,
    Zoom100,
    ZoomFit,
    None,
}

/// One replayable step of a recorded Action (Phase 4). Steps carry
/// their full parameters so replay needs no dialogs.
#[derive(Clone, Debug)]
pub(crate) enum RecordedStep {
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
    AutoLevels,
    AutoContrast,
    AutoColor,
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
    // Phase 6.5 Batch 1a gallery additions.
    Extrude(u32, u32, u8),
    TraceContour(f32, bool),
    ColorHalftone(u32),
    Crystallize(u32),
    Pointillize(Pixel16, u32),
    SmartBlur(f32, f32, bool),
    DeInterlace(bool, bool),
    SharpenMore,
    NtscColors,
    // Phase 6.5 Batch 1b.
    Wave(u32, f32, f32, f32, f32, u8, bool),
    Glass(f32, f32, f32),
    DiffuseGlow(f32, f32, f32),
    Displace(Vec<f32>, u32, u32, f32, f32, bool, bool),
    LensFlare(f32, f32, f32, u8),
    Lighting(f32, f32, f32, f32, f32, bool, bool),
    TextureFill(Vec<f32>, u32, u32),
    // Phase 6.5 Batch 1c (Artistic).
    ColoredPencil(f32, f32, f32),
    Cutout(f32, f32, f32),
    DryBrush(f32, f32, f32),
    FilmGrain(f32, f32, f32),
    Fresco(f32, f32, f32),
    NeonGlow(f32, f32, Pixel16),
    PaintDaubs(f32, f32, u8),
    PaletteKnife(f32, f32, f32),
    PlasticWrap(f32, f32, f32),
    PosterEdges(f32, f32, f32),
    RoughPastels(f32, f32, f32, f32, u8),
    SmudgeStick(f32, f32, f32),
    Sponge(f32, f32, f32),
    Underpainting(f32, f32, f32, f32, u8),
    Watercolor(f32, f32, f32),
    // Phase 6.5 Batch 1d (Brush Strokes).
    AccentedEdges(f32, f32, f32),
    AngledStrokes(f32, f32, f32),
    Crosshatch(f32, f32, f32),
    DarkStrokes(f32, f32, f32),
    InkOutlines(f32, f32, f32),
    Spatter(f32, f32),
    SprayedStrokes(f32, f32, u8),
    SumiE(f32, f32, f32),
    // Phase 6.5 Batch 1e (Sketch + Texture).
    ChalkCharcoal(f32, f32, f32, Pixel16, Pixel16),
    CharcoalFx(f32, f32, f32),
    Chrome(f32, f32),
    ConteCrayon(f32, f32, f32, f32, u8, Pixel16, Pixel16),
    GraphicPen(f32, f32, u8, Pixel16, Pixel16),
    HalftonePattern(f32, f32, u8),
    NotePaper(f32, f32, f32),
    Photocopy(f32, f32),
    Plaster(f32, f32, u8),
    Reticulation(f32, f32, f32, Pixel16, Pixel16),
    TornEdges(f32, f32, f32, Pixel16, Pixel16),
    WaterPaper(f32, f32, f32, Pixel16, Pixel16),
    Craquelure(f32, f32, f32),
    GrainFx(f32, f32, u8),
    MosaicTiles(f32, f32, f32),
    StainedGlass(f32, f32, f32),
    Texturizer(f32, f32, u8, u8),
    Diffuse(u8),
    Mezzotint(u8),
    DifferenceClouds,
    SharpenEdges,
    Fragment,
    Facet,
    Transform3d(u8, f32, f32),
    ChannelMixer(adjust::ChannelMixer),
    SelectiveColor(u8, [i32; 4], bool),
    ReplaceColor((u8, u8, u8), i32, i32, i32, i32),
    SetEffects(pictodesk_core::effects::Effects),
    LayerNew,
    LayerDelete,
    LayerDuplicate,
    LayerSelect(usize),
    // Phase 6.5 Batch 6d: interactive action steps. Playback
    // intercepts them; apply_step treats them as no-ops.
    Stop(String),
    MenuCommand(String),
}

impl RecordedStep {
    pub(crate) fn label(&self) -> String {
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
            RecordedStep::AutoLevels => "Auto Levels".into(),
            RecordedStep::AutoContrast => "Auto Contrast".into(),
            RecordedStep::AutoColor => "Auto Color".into(),
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
            RecordedStep::Extrude(..) => "Extrude".into(),
            RecordedStep::TraceContour(..) => "Trace Contour".into(),
            RecordedStep::ColorHalftone(..) => "Color Halftone".into(),
            RecordedStep::Crystallize(..) => "Crystallize".into(),
            RecordedStep::Pointillize(..) => "Pointillize".into(),
            RecordedStep::SmartBlur(..) => "Smart Blur".into(),
            RecordedStep::DeInterlace(..) => "De-Interlace".into(),
            RecordedStep::SharpenMore => "Sharpen More".into(),
            RecordedStep::NtscColors => "NTSC Colors".into(),
            RecordedStep::Wave(..) => "Wave".into(),
            RecordedStep::Glass(..) => "Glass".into(),
            RecordedStep::DiffuseGlow(..) => "Diffuse Glow".into(),
            RecordedStep::Displace(..) => "Displace".into(),
            RecordedStep::LensFlare(..) => "Lens Flare".into(),
            RecordedStep::Lighting(..) => "Lighting Effects".into(),
            RecordedStep::TextureFill(..) => "Texture Fill".into(),
            RecordedStep::ColoredPencil(..) => "Colored Pencil".into(),
            RecordedStep::Cutout(..) => "Cutout".into(),
            RecordedStep::DryBrush(..) => "Dry Brush".into(),
            RecordedStep::FilmGrain(..) => "Film Grain".into(),
            RecordedStep::Fresco(..) => "Fresco".into(),
            RecordedStep::NeonGlow(..) => "Neon Glow".into(),
            RecordedStep::PaintDaubs(..) => "Paint Daubs".into(),
            RecordedStep::PaletteKnife(..) => "Palette Knife".into(),
            RecordedStep::PlasticWrap(..) => "Plastic Wrap".into(),
            RecordedStep::PosterEdges(..) => "Poster Edges".into(),
            RecordedStep::RoughPastels(..) => "Rough Pastels".into(),
            RecordedStep::SmudgeStick(..) => "Smudge Stick".into(),
            RecordedStep::Sponge(..) => "Sponge".into(),
            RecordedStep::Underpainting(..) => "Underpainting".into(),
            RecordedStep::Watercolor(..) => "Watercolor".into(),
            RecordedStep::AccentedEdges(..) => "Accented Edges".into(),
            RecordedStep::AngledStrokes(..) => "Angled Strokes".into(),
            RecordedStep::Crosshatch(..) => "Crosshatch".into(),
            RecordedStep::DarkStrokes(..) => "Dark Strokes".into(),
            RecordedStep::InkOutlines(..) => "Ink Outlines".into(),
            RecordedStep::Spatter(..) => "Spatter".into(),
            RecordedStep::SprayedStrokes(..) => "Sprayed Strokes".into(),
            RecordedStep::SumiE(..) => "Sumi-e".into(),
            RecordedStep::ChalkCharcoal(..) => "Chalk & Charcoal".into(),
            RecordedStep::CharcoalFx(..) => "Charcoal".into(),
            RecordedStep::Chrome(..) => "Chrome".into(),
            RecordedStep::ConteCrayon(..) => "Conte Crayon".into(),
            RecordedStep::GraphicPen(..) => "Graphic Pen".into(),
            RecordedStep::HalftonePattern(..) => "Halftone Pattern".into(),
            RecordedStep::NotePaper(..) => "Note Paper".into(),
            RecordedStep::Photocopy(..) => "Photocopy".into(),
            RecordedStep::Plaster(..) => "Plaster".into(),
            RecordedStep::Reticulation(..) => "Reticulation".into(),
            RecordedStep::TornEdges(..) => "Torn Edges".into(),
            RecordedStep::WaterPaper(..) => "Water Paper".into(),
            RecordedStep::Craquelure(..) => "Craquelure".into(),
            RecordedStep::GrainFx(..) => "Grain".into(),
            RecordedStep::MosaicTiles(..) => "Mosaic Tiles".into(),
            RecordedStep::StainedGlass(..) => "Stained Glass".into(),
            RecordedStep::Texturizer(..) => "Texturizer".into(),
            RecordedStep::Diffuse(_) => "Diffuse".into(),
            RecordedStep::Mezzotint(_) => "Mezzotint".into(),
            RecordedStep::DifferenceClouds => "Difference Clouds".into(),
            RecordedStep::SharpenEdges => "Sharpen Edges".into(),
            RecordedStep::Fragment => "Fragment".into(),
            RecordedStep::Facet => "Facet".into(),
            RecordedStep::Transform3d(..) => "3D Transform".into(),
            RecordedStep::ChannelMixer(_) => "Channel Mixer".into(),
            RecordedStep::SelectiveColor(..) => "Selective Color".into(),
            RecordedStep::ReplaceColor(..) => "Replace Color".into(),
            RecordedStep::SetEffects(_) => "Layer Effects".into(),
            RecordedStep::LayerNew => "New Layer".into(),
            RecordedStep::LayerDelete => "Delete Layer".into(),
            RecordedStep::LayerDuplicate => "Duplicate Layer".into(),
            RecordedStep::LayerSelect(_) => "Select Layer".into(),
            RecordedStep::Stop(m) => format!("Stop: {m}"),
            RecordedStep::MenuCommand(p) => format!("Menu: {p}"),
        }
    }
}

/// A named, replayable sequence of steps (Phase 4 Actions).
#[derive(Clone, Debug)]
pub(crate) struct RecordedAction {
    pub(crate) name: String,
    pub(crate) steps: Vec<RecordedStep>,
    /// The action set this action belongs to (Phase 5).
    pub(crate) set: String,
}

/// The last Free Transform gesture, for Transform Again:
/// (sx, sy, rot, skew, dx, dy) + distort quad as offsets from
/// the bounds corners.
/// One reassignable keyboard shortcut: a stable id, the label
/// shown in Preferences, and the canonical combo ("primary+z",
/// "b", ""). An empty combo means unassigned.
#[derive(Clone, Debug)]
pub(crate) struct ShortcutDef {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) combo: String,
}

/// Step serialization: one line per step, space-separated.
/// Covers every RecordedStep variant (Phase 6.5 Batch 6d);
/// used by droplets, action persistence between sessions,
/// and adjustment-settings save/load.
pub(crate) fn step_to_line(step: &RecordedStep) -> Option<String> {
    let f = |v: f32| format!("{v}");
    Some(match step {
        RecordedStep::Adjustment(adj) => adj_to_line(adj, "adj")?,
        RecordedStep::AdjustmentLayer(adj) => adj_to_line(adj, "adjlayer")?,
        RecordedStep::Posterize(v0) => format!("posterize {}", *v0),
        RecordedStep::Threshold(v0) => format!("threshold {}", *v0),
        RecordedStep::Invert => "invert".into(),
        RecordedStep::Desaturate => "desaturate".into(),
        RecordedStep::GaussianBlur(v0) => format!("blur {}", f(*v0)),
        RecordedStep::Unsharp(v0, v1, v2) => format!("unsharp {} {} {}", f(*v0), f(*v1), *v2),
        RecordedStep::AddNoise(v0) => format!("noise {}", f(*v0)),
        RecordedStep::Sharpen => "sharpen".into(),
        RecordedStep::FlipH => "fliph".into(),
        RecordedStep::FlipV => "flipv".into(),
        RecordedStep::Equalize => "equalize".into(),
        RecordedStep::AutoLevels => "autolevels".into(),
        RecordedStep::AutoContrast => "autocontrast".into(),
        RecordedStep::AutoColor => "autocolor".into(),
        RecordedStep::ImageSize(v0, v1) => format!("imagesize {} {}", *v0, *v1),
        RecordedStep::CanvasSize(w, h, a) => format!("canvassize {w} {h} {} {}", a.0, a.1),
        RecordedStep::Rotate180 => "rot180".into(),
        RecordedStep::Rotate90Cw => "rot90cw".into(),
        RecordedStep::Rotate90Ccw => "rot90ccw".into(),
        RecordedStep::Flatten => "flatten".into(),
        RecordedStep::CropToSelection => "crop".into(),
        RecordedStep::MotionBlur(v0, v1) => format!("motionblur {} {}", f(*v0), f(*v1)),
        RecordedStep::RadialBlur(v0) => format!("radialblur {}", f(*v0)),
        RecordedStep::Median(v0) => format!("median {}", *v0),
        RecordedStep::Despeckle => "despeckle".into(),
        RecordedStep::Emboss(v0) => format!("emboss {}", f(*v0)),
        RecordedStep::FindEdges => "findedges".into(),
        RecordedStep::Solarize => "solarize".into(),
        RecordedStep::Mosaic(v0) => format!("mosaic {}", *v0),
        RecordedStep::HighPass(v0) => format!("highpass {}", f(*v0)),
        RecordedStep::OffsetPx(v0, v1) => format!("offset {} {}", *v0, *v1),
        RecordedStep::Pinch(v0) => format!("pinch {}", f(*v0)),
        RecordedStep::Spherize(v0) => format!("spherize {}", f(*v0)),
        RecordedStep::Twirl(v0) => format!("twirl {}", f(*v0)),
        RecordedStep::Ripple(v0, v1) => format!("ripple {} {}", f(*v0), f(*v1)),
        RecordedStep::OceanRipple(v0, v1) => format!("oceanripple {} {}", f(*v0), f(*v1)),
        RecordedStep::Polar(v0) => format!("polar {}", *v0 as u8),
        RecordedStep::ZigZag(v0, v1, v2) => format!("zigzag {} {} {}", f(*v0), *v1, *v2),
        RecordedStep::Maximum(v0) => format!("maximum {}", *v0),
        RecordedStep::Minimum(v0) => format!("minimum {}", *v0),
        RecordedStep::DustScratches(v0, v1) => format!("dust {} {}", *v0, f(*v1)),
        RecordedStep::Tiles(v0, v1) => format!("tiles {} {}", *v0, f(*v1)),
        RecordedStep::GlowingEdges(v0, v1, v2) => {
            format!("glowingedges {} {} {}", *v0, f(*v1), *v2)
        }
        RecordedStep::Patchwork(v0, v1) => format!("patchwork {} {}", *v0, *v1),
        RecordedStep::StampEffect(v0, v1) => format!("stampeffect {} {}", f(*v0), *v1),
        RecordedStep::Wind(v0, v1) => format!("wind {} {}", *v0, *v1 as u8),
        RecordedStep::Extrude(v0, v1, v2) => format!("extrude {} {} {}", *v0, *v1, *v2),
        RecordedStep::TraceContour(v0, v1) => format!("tracecontour {} {}", f(*v0), *v1 as u8),
        RecordedStep::ColorHalftone(v0) => format!("colorhalftone {}", *v0),
        RecordedStep::Crystallize(v0) => format!("crystallize {}", *v0),
        RecordedStep::SmartBlur(v0, v1, v2) => {
            format!("smartblur {} {} {}", f(*v0), f(*v1), *v2 as u8)
        }
        RecordedStep::DeInterlace(v0, v1) => format!("deinterlace {} {}", *v0 as u8, *v1 as u8),
        RecordedStep::SharpenMore => "sharpenmore".into(),
        RecordedStep::NtscColors => "ntsc".into(),
        RecordedStep::Wave(v0, v1, v2, v3, v4, v5, v6) => format!(
            "wave {} {} {} {} {} {} {}",
            *v0,
            f(*v1),
            f(*v2),
            f(*v3),
            f(*v4),
            *v5,
            *v6 as u8
        ),
        RecordedStep::Glass(v0, v1, v2) => format!("glass {} {} {}", f(*v0), f(*v1), f(*v2)),
        RecordedStep::DiffuseGlow(v0, v1, v2) => {
            format!("diffuseglow {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::LensFlare(v0, v1, v2, v3) => {
            format!("lensflare {} {} {} {}", f(*v0), f(*v1), f(*v2), *v3)
        }
        RecordedStep::Lighting(v0, v1, v2, v3, v4, v5, v6) => format!(
            "lighting {} {} {} {} {} {} {}",
            f(*v0),
            f(*v1),
            f(*v2),
            f(*v3),
            f(*v4),
            *v5 as u8,
            *v6 as u8
        ),
        RecordedStep::ColoredPencil(v0, v1, v2) => {
            format!("coloredpencil {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Cutout(v0, v1, v2) => format!("cutout {} {} {}", f(*v0), f(*v1), f(*v2)),
        RecordedStep::DryBrush(v0, v1, v2) => format!("drybrush {} {} {}", f(*v0), f(*v1), f(*v2)),
        RecordedStep::FilmGrain(v0, v1, v2) => {
            format!("filmgrain {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Fresco(v0, v1, v2) => format!("fresco {} {} {}", f(*v0), f(*v1), f(*v2)),
        RecordedStep::PaintDaubs(v0, v1, v2) => format!("paintdaubs {} {} {}", f(*v0), f(*v1), *v2),
        RecordedStep::PaletteKnife(v0, v1, v2) => {
            format!("paletteknife {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::PlasticWrap(v0, v1, v2) => {
            format!("plasticwrap {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::PosterEdges(v0, v1, v2) => {
            format!("posteredges {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::RoughPastels(v0, v1, v2, v3, v4) => format!(
            "roughpastels {} {} {} {} {}",
            f(*v0),
            f(*v1),
            f(*v2),
            f(*v3),
            *v4
        ),
        RecordedStep::SmudgeStick(v0, v1, v2) => {
            format!("smudgestick {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Sponge(v0, v1, v2) => format!("spongefx {} {} {}", f(*v0), f(*v1), f(*v2)),
        RecordedStep::Underpainting(v0, v1, v2, v3, v4) => format!(
            "underpainting {} {} {} {} {}",
            f(*v0),
            f(*v1),
            f(*v2),
            f(*v3),
            *v4
        ),
        RecordedStep::Watercolor(v0, v1, v2) => {
            format!("watercolor {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::AccentedEdges(v0, v1, v2) => {
            format!("accentededges {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::AngledStrokes(v0, v1, v2) => {
            format!("angledstrokes {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Crosshatch(v0, v1, v2) => {
            format!("crosshatch {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::DarkStrokes(v0, v1, v2) => {
            format!("darkstrokes {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::InkOutlines(v0, v1, v2) => {
            format!("inkoutlines {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Spatter(v0, v1) => format!("spatter {} {}", f(*v0), f(*v1)),
        RecordedStep::SprayedStrokes(v0, v1, v2) => {
            format!("sprayedstrokes {} {} {}", f(*v0), f(*v1), *v2)
        }
        RecordedStep::SumiE(v0, v1, v2) => format!("sumie {} {} {}", f(*v0), f(*v1), f(*v2)),
        RecordedStep::CharcoalFx(v0, v1, v2) => {
            format!("charcoal {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Chrome(v0, v1) => format!("chromefx {} {}", f(*v0), f(*v1)),
        RecordedStep::HalftonePattern(v0, v1, v2) => {
            format!("halftonepattern {} {} {}", f(*v0), f(*v1), *v2)
        }
        RecordedStep::NotePaper(v0, v1, v2) => {
            format!("notepaper {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Photocopy(v0, v1) => format!("photocopy {} {}", f(*v0), f(*v1)),
        RecordedStep::Plaster(v0, v1, v2) => format!("plaster {} {} {}", f(*v0), f(*v1), *v2),
        RecordedStep::Craquelure(v0, v1, v2) => {
            format!("craquelure {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::GrainFx(v0, v1, v2) => format!("grainfx {} {} {}", f(*v0), f(*v1), *v2),
        RecordedStep::MosaicTiles(v0, v1, v2) => {
            format!("mosaictiles {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::StainedGlass(v0, v1, v2) => {
            format!("stainedglass {} {} {}", f(*v0), f(*v1), f(*v2))
        }
        RecordedStep::Texturizer(v0, v1, v2, v3) => {
            format!("texturizer {} {} {} {}", f(*v0), f(*v1), *v2, *v3)
        }
        RecordedStep::Diffuse(v0) => format!("diffuse {}", *v0),
        RecordedStep::Mezzotint(v0) => format!("mezzotint {}", *v0),
        RecordedStep::DifferenceClouds => "diffclouds".into(),
        RecordedStep::SharpenEdges => "sharpenedges".into(),
        RecordedStep::Fragment => "fragment".into(),
        RecordedStep::Facet => "facet".into(),
        RecordedStep::Transform3d(v0, v1, v2) => {
            format!("transform3d {} {} {}", *v0, f(*v1), f(*v2))
        }
        RecordedStep::LayerNew => "layernew".into(),
        RecordedStep::LayerDelete => "layerdelete".into(),
        RecordedStep::LayerDuplicate => "layerduplicate".into(),
        RecordedStep::LayerSelect(v0) => format!("layerselect {}", *v0),
        RecordedStep::Clouds(a, b) => format!(
            "clouds {}",
            [a, b]
                .iter()
                .map(|p| {
                    let q = p.to_rgba8();
                    format!("{} {} {} {}", q.0, q.1, q.2, q.3)
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
        RecordedStep::Pointillize(p, n) => format!("pointillize {} {n}", {
            let q = p.to_rgba8();
            format!("{} {} {} {}", q.0, q.1, q.2, q.3)
        }),
        RecordedStep::NeonGlow(a, b, p) => format!("neonglow {} {} {}", f(*a), f(*b), {
            let q = p.to_rgba8();
            format!("{} {} {} {}", q.0, q.1, q.2, q.3)
        }),
        RecordedStep::BasRelief(a, b, x, y) => format!(
            "basrelief {} {} {x} {y}",
            {
                let q = a.to_rgba8();
                format!("{} {} {} {}", q.0, q.1, q.2, q.3)
            },
            {
                let q = b.to_rgba8();
                format!("{} {} {} {}", q.0, q.1, q.2, q.3)
            }
        ),
        RecordedStep::ChalkCharcoal(a, b, c, p, q) => format!(
            "chalkcharcoal {} {} {} {} {}",
            f(*a),
            f(*b),
            f(*c),
            {
                let z = p.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            },
            {
                let z = q.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            }
        ),
        RecordedStep::ConteCrayon(a, b, c, d, e, p, q) => format!(
            "contecrayon {} {} {} {} {e} {} {}",
            f(*a),
            f(*b),
            f(*c),
            f(*d),
            {
                let z = p.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            },
            {
                let z = q.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            }
        ),
        RecordedStep::GraphicPen(a, b, e, p, q) => format!(
            "graphicpen {} {} {e} {} {}",
            f(*a),
            f(*b),
            {
                let z = p.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            },
            {
                let z = q.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            }
        ),
        RecordedStep::Reticulation(a, b, c, p, q) => format!(
            "reticulation {} {} {} {} {}",
            f(*a),
            f(*b),
            f(*c),
            {
                let z = p.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            },
            {
                let z = q.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            }
        ),
        RecordedStep::TornEdges(a, b, c, p, q) => format!(
            "tornedges {} {} {} {} {}",
            f(*a),
            f(*b),
            f(*c),
            {
                let z = p.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            },
            {
                let z = q.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            }
        ),
        RecordedStep::WaterPaper(a, b, c, p, q) => format!(
            "waterpaper {} {} {} {} {}",
            f(*a),
            f(*b),
            f(*c),
            {
                let z = p.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            },
            {
                let z = q.to_rgba8();
                format!("{} {} {} {}", z.0, z.1, z.2, z.3)
            }
        ),
        RecordedStep::CustomKernel(k, sc, o) => format!(
            "kernel {} {sc} {o}",
            k.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        RecordedStep::Shear(o) => format!(
            "shear {}",
            o.iter().map(|v| f(*v)).collect::<Vec<_>>().join(" ")
        ),
        RecordedStep::Displace(plane, w, h, sx, sy, st, fi) => format!(
            "displace {} {} {w} {h} {} {} {} {}",
            plane.len(),
            plane.iter().map(|v| f(*v)).collect::<Vec<_>>().join(" "),
            f(*sx),
            f(*sy),
            *st as u8,
            *fi as u8
        ),
        RecordedStep::TextureFill(plane, w, h) => format!(
            "texturefill {} {} {w} {h}",
            plane.len(),
            plane.iter().map(|v| f(*v)).collect::<Vec<_>>().join(" ")
        ),
        RecordedStep::ChannelMixer(m) => format!(
            "mixer {} {}",
            m.rows
                .iter()
                .flatten()
                .chain(m.constant.iter())
                .map(|v| f(*v))
                .collect::<Vec<_>>()
                .join(" "),
            m.monochrome as u8
        ),
        RecordedStep::SelectiveColor(r, a, rel) => format!(
            "selcolor {r} {} {} {} {} {}",
            a[0], a[1], a[2], a[3], *rel as u8
        ),
        RecordedStep::ReplaceColor(t, fz, hu, sa, li) => {
            format!("replcolor {} {} {} {fz} {hu} {sa} {li}", t.0, t.1, t.2)
        }
        RecordedStep::SetEffects(e) => format!("effects {}", effects_to_text(e)),
        RecordedStep::Stop(msg) => format!("stop {msg}"),
        RecordedStep::MenuCommand(path) => format!("menu {path}"),
    })
}

/// Serialize one Adjustment (shared by the Adjustment and
/// AdjustmentLayer steps and the settings save/load).
pub(crate) fn adj_to_line(adj: &pictodesk_core::adjust::Adjustment, tag: &str) -> Option<String> {
    use pictodesk_core::adjust::Adjustment;
    let f = |v: f32| format!("{v}");
    Some(match adj {
        Adjustment::Levels {
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => format!(
            "{tag} levels {in_black} {in_white} {} {out_black} {out_white}",
            f(*gamma)
        ),
        Adjustment::BrightnessContrast {
            brightness,
            contrast,
        } => format!("{tag} bc {brightness} {contrast}"),
        Adjustment::HueSaturation {
            hue,
            saturation,
            lightness,
        } => format!("{tag} hue {hue} {saturation} {lightness}"),
        Adjustment::PhotoFilter { color, density } => {
            format!("{tag} photo {} {} {} {density}", color.0, color.1, color.2)
        }
        Adjustment::GradientMap { from, to, reverse } => format!(
            "{tag} gradmap {} {} {} {} {} {} {}",
            from.0, from.1, from.2, to.0, to.1, to.2, *reverse as u8
        ),
        Adjustment::Curves { points } => format!(
            "{tag} curves {} {}",
            points.len(),
            points
                .iter()
                .map(|p| format!("{} {}", p.0, p.1))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Adjustment::ColorBalance {
            shadows,
            midtones,
            highlights,
        } => format!(
            "{tag} colorbal {} {} {} {} {} {} {} {} {}",
            shadows.0,
            shadows.1,
            shadows.2,
            midtones.0,
            midtones.1,
            midtones.2,
            highlights.0,
            highlights.1,
            highlights.2
        ),
    })
}

pub(crate) fn line_to_step(line: &str) -> Option<RecordedStep> {
    let t: Vec<&str> = line.split_whitespace().collect();
    let p = |i: usize| t.get(i).and_then(|v| v.parse::<u8>().ok());
    let pu = |i: usize| t.get(i).and_then(|v| v.parse::<u32>().ok());
    let pf = |i: usize| t.get(i).and_then(|v| v.parse::<f32>().ok());
    let pi = |i: usize| t.get(i).and_then(|v| v.parse::<i32>().ok());
    let px = |i: usize| -> Option<Pixel16> {
        Some(Pixel16::from_rgba8(p(i)?, p(i + 1)?, p(i + 2)?, p(i + 3)?))
    };
    Some(match t.first().copied()? {
        "adj" => RecordedStep::Adjustment(line_to_adj(&t)?),
        "adjlayer" => RecordedStep::AdjustmentLayer(line_to_adj(&t)?),
        "posterize" => RecordedStep::Posterize(p(1)?),
        "threshold" => RecordedStep::Threshold(p(1)?),
        "invert" => RecordedStep::Invert,
        "desaturate" => RecordedStep::Desaturate,
        "blur" => RecordedStep::GaussianBlur(pf(1)?),
        "unsharp" => RecordedStep::Unsharp(pf(1)?, pf(2)?, p(3)?),
        "noise" => RecordedStep::AddNoise(pf(1)?),
        "sharpen" => RecordedStep::Sharpen,
        "fliph" => RecordedStep::FlipH,
        "flipv" => RecordedStep::FlipV,
        "equalize" => RecordedStep::Equalize,
        "autolevels" => RecordedStep::AutoLevels,
        "autocontrast" => RecordedStep::AutoContrast,
        "autocolor" => RecordedStep::AutoColor,
        "imagesize" => RecordedStep::ImageSize(pu(1)?, pu(2)?),
        "canvassize" => RecordedStep::CanvasSize(pu(1)?, pu(2)?, (pi(3)?, pi(4)?)),
        "rot180" => RecordedStep::Rotate180,
        "rot90cw" => RecordedStep::Rotate90Cw,
        "rot90ccw" => RecordedStep::Rotate90Ccw,
        "flatten" => RecordedStep::Flatten,
        "crop" => RecordedStep::CropToSelection,
        "motionblur" => RecordedStep::MotionBlur(pf(1)?, pf(2)?),
        "radialblur" => RecordedStep::RadialBlur(pf(1)?),
        "median" => RecordedStep::Median(pu(1)?),
        "despeckle" => RecordedStep::Despeckle,
        "emboss" => RecordedStep::Emboss(pf(1)?),
        "findedges" => RecordedStep::FindEdges,
        "solarize" => RecordedStep::Solarize,
        "mosaic" => RecordedStep::Mosaic(pu(1)?),
        "highpass" => RecordedStep::HighPass(pf(1)?),
        "offset" => RecordedStep::OffsetPx(pi(1)?, pi(2)?),
        "pinch" => RecordedStep::Pinch(pf(1)?),
        "spherize" => RecordedStep::Spherize(pf(1)?),
        "twirl" => RecordedStep::Twirl(pf(1)?),
        "ripple" => RecordedStep::Ripple(pf(1)?, pf(2)?),
        "oceanripple" => RecordedStep::OceanRipple(pf(1)?, pf(2)?),
        "polar" => RecordedStep::Polar(p(1)? != 0),
        "zigzag" => RecordedStep::ZigZag(pf(1)?, pu(2)?, p(3)?),
        "maximum" => RecordedStep::Maximum(pu(1)?),
        "minimum" => RecordedStep::Minimum(pu(1)?),
        "dust" => RecordedStep::DustScratches(pu(1)?, pf(2)?),
        "tiles" => RecordedStep::Tiles(pu(1)?, pf(2)?),
        "glowingedges" => RecordedStep::GlowingEdges(pu(1)?, pf(2)?, pu(3)?),
        "patchwork" => RecordedStep::Patchwork(pu(1)?, pu(2)?),
        "stampeffect" => RecordedStep::StampEffect(pf(1)?, pu(2)?),
        "wind" => RecordedStep::Wind(p(1)?, p(2)? != 0),
        "extrude" => RecordedStep::Extrude(pu(1)?, pu(2)?, p(3)?),
        "tracecontour" => RecordedStep::TraceContour(pf(1)?, p(2)? != 0),
        "colorhalftone" => RecordedStep::ColorHalftone(pu(1)?),
        "crystallize" => RecordedStep::Crystallize(pu(1)?),
        "smartblur" => RecordedStep::SmartBlur(pf(1)?, pf(2)?, p(3)? != 0),
        "deinterlace" => RecordedStep::DeInterlace(p(1)? != 0, p(2)? != 0),
        "sharpenmore" => RecordedStep::SharpenMore,
        "ntsc" => RecordedStep::NtscColors,
        "wave" => RecordedStep::Wave(pu(1)?, pf(2)?, pf(3)?, pf(4)?, pf(5)?, p(6)?, p(7)? != 0),
        "glass" => RecordedStep::Glass(pf(1)?, pf(2)?, pf(3)?),
        "diffuseglow" => RecordedStep::DiffuseGlow(pf(1)?, pf(2)?, pf(3)?),
        "lensflare" => RecordedStep::LensFlare(pf(1)?, pf(2)?, pf(3)?, p(4)?),
        "lighting" => RecordedStep::Lighting(
            pf(1)?,
            pf(2)?,
            pf(3)?,
            pf(4)?,
            pf(5)?,
            p(6)? != 0,
            p(7)? != 0,
        ),
        "coloredpencil" => RecordedStep::ColoredPencil(pf(1)?, pf(2)?, pf(3)?),
        "cutout" => RecordedStep::Cutout(pf(1)?, pf(2)?, pf(3)?),
        "drybrush" => RecordedStep::DryBrush(pf(1)?, pf(2)?, pf(3)?),
        "filmgrain" => RecordedStep::FilmGrain(pf(1)?, pf(2)?, pf(3)?),
        "fresco" => RecordedStep::Fresco(pf(1)?, pf(2)?, pf(3)?),
        "paintdaubs" => RecordedStep::PaintDaubs(pf(1)?, pf(2)?, p(3)?),
        "paletteknife" => RecordedStep::PaletteKnife(pf(1)?, pf(2)?, pf(3)?),
        "plasticwrap" => RecordedStep::PlasticWrap(pf(1)?, pf(2)?, pf(3)?),
        "posteredges" => RecordedStep::PosterEdges(pf(1)?, pf(2)?, pf(3)?),
        "roughpastels" => RecordedStep::RoughPastels(pf(1)?, pf(2)?, pf(3)?, pf(4)?, p(5)?),
        "smudgestick" => RecordedStep::SmudgeStick(pf(1)?, pf(2)?, pf(3)?),
        "spongefx" => RecordedStep::Sponge(pf(1)?, pf(2)?, pf(3)?),
        "underpainting" => RecordedStep::Underpainting(pf(1)?, pf(2)?, pf(3)?, pf(4)?, p(5)?),
        "watercolor" => RecordedStep::Watercolor(pf(1)?, pf(2)?, pf(3)?),
        "accentededges" => RecordedStep::AccentedEdges(pf(1)?, pf(2)?, pf(3)?),
        "angledstrokes" => RecordedStep::AngledStrokes(pf(1)?, pf(2)?, pf(3)?),
        "crosshatch" => RecordedStep::Crosshatch(pf(1)?, pf(2)?, pf(3)?),
        "darkstrokes" => RecordedStep::DarkStrokes(pf(1)?, pf(2)?, pf(3)?),
        "inkoutlines" => RecordedStep::InkOutlines(pf(1)?, pf(2)?, pf(3)?),
        "spatter" => RecordedStep::Spatter(pf(1)?, pf(2)?),
        "sprayedstrokes" => RecordedStep::SprayedStrokes(pf(1)?, pf(2)?, p(3)?),
        "sumie" => RecordedStep::SumiE(pf(1)?, pf(2)?, pf(3)?),
        "charcoal" => RecordedStep::CharcoalFx(pf(1)?, pf(2)?, pf(3)?),
        "chromefx" => RecordedStep::Chrome(pf(1)?, pf(2)?),
        "halftonepattern" => RecordedStep::HalftonePattern(pf(1)?, pf(2)?, p(3)?),
        "notepaper" => RecordedStep::NotePaper(pf(1)?, pf(2)?, pf(3)?),
        "photocopy" => RecordedStep::Photocopy(pf(1)?, pf(2)?),
        "plaster" => RecordedStep::Plaster(pf(1)?, pf(2)?, p(3)?),
        "craquelure" => RecordedStep::Craquelure(pf(1)?, pf(2)?, pf(3)?),
        "grainfx" => RecordedStep::GrainFx(pf(1)?, pf(2)?, p(3)?),
        "mosaictiles" => RecordedStep::MosaicTiles(pf(1)?, pf(2)?, pf(3)?),
        "stainedglass" => RecordedStep::StainedGlass(pf(1)?, pf(2)?, pf(3)?),
        "texturizer" => RecordedStep::Texturizer(pf(1)?, pf(2)?, p(3)?, p(4)?),
        "diffuse" => RecordedStep::Diffuse(p(1)?),
        "mezzotint" => RecordedStep::Mezzotint(p(1)?),
        "diffclouds" => RecordedStep::DifferenceClouds,
        "sharpenedges" => RecordedStep::SharpenEdges,
        "fragment" => RecordedStep::Fragment,
        "facet" => RecordedStep::Facet,
        "transform3d" => RecordedStep::Transform3d(p(1)?, pf(2)?, pf(3)?),
        "layernew" => RecordedStep::LayerNew,
        "layerdelete" => RecordedStep::LayerDelete,
        "layerduplicate" => RecordedStep::LayerDuplicate,
        "layerselect" => RecordedStep::LayerSelect(pu(1)? as usize),
        "clouds" => RecordedStep::Clouds(px(1)?, px(5)?),
        "pointillize" => RecordedStep::Pointillize(px(1)?, pu(5)?),
        "neonglow" => RecordedStep::NeonGlow(pf(1)?, pf(2)?, px(3)?),
        "basrelief" => RecordedStep::BasRelief(px(1)?, px(5)?, pu(9)?, pu(10)?),
        "chalkcharcoal" => RecordedStep::ChalkCharcoal(pf(1)?, pf(2)?, pf(3)?, px(4)?, px(8)?),
        "contecrayon" => {
            RecordedStep::ConteCrayon(pf(1)?, pf(2)?, pf(3)?, pf(4)?, p(5)?, px(6)?, px(10)?)
        }
        "graphicpen" => RecordedStep::GraphicPen(pf(1)?, pf(2)?, p(3)?, px(4)?, px(8)?),
        "reticulation" => RecordedStep::Reticulation(pf(1)?, pf(2)?, pf(3)?, px(4)?, px(8)?),
        "tornedges" => RecordedStep::TornEdges(pf(1)?, pf(2)?, pf(3)?, px(4)?, px(8)?),
        "waterpaper" => RecordedStep::WaterPaper(pf(1)?, pf(2)?, pf(3)?, px(4)?, px(8)?),
        "kernel" => {
            let mut k = [0i32; 25];
            for (j, v) in k.iter_mut().enumerate() {
                *v = pi(1 + j)?;
            }
            RecordedStep::CustomKernel(k, pi(26)?, pi(27)?)
        }
        "shear" => {
            let mut o = [0f32; 5];
            for (j, v) in o.iter_mut().enumerate() {
                *v = pf(1 + j)?;
            }
            RecordedStep::Shear(o)
        }
        "displace" => {
            let n = pu(1)? as usize;
            let mut plane = Vec::new();
            for j in 0..n {
                plane.push(pf(2 + j)?);
            }
            let b = 2 + n;
            RecordedStep::Displace(
                plane,
                pu(b)?,
                pu(b + 1)?,
                pf(b + 2)?,
                pf(b + 3)?,
                p(b + 4)? != 0,
                p(b + 5)? != 0,
            )
        }
        "texturefill" => {
            let n = pu(1)? as usize;
            let mut plane = Vec::new();
            for j in 0..n {
                plane.push(pf(2 + j)?);
            }
            let b = 2 + n;
            RecordedStep::TextureFill(plane, pu(b)?, pu(b + 1)?)
        }
        "mixer" => {
            let mut rows = [[0f32; 4]; 4];
            let mut constant = [0f32; 4];
            for (r, row) in rows.iter_mut().enumerate() {
                for (c, cell) in row.iter_mut().enumerate() {
                    *cell = pf(1 + r * 4 + c)?;
                }
            }
            for (c, cell) in constant.iter_mut().enumerate() {
                *cell = pf(17 + c)?;
            }
            RecordedStep::ChannelMixer(pictodesk_core::adjust::ChannelMixer {
                rows,
                constant,
                monochrome: p(21)? != 0,
            })
        }
        "selcolor" => {
            RecordedStep::SelectiveColor(p(1)?, [pi(2)?, pi(3)?, pi(4)?, pi(5)?], p(6)? != 0)
        }
        "replcolor" => {
            RecordedStep::ReplaceColor((p(1)?, p(2)?, p(3)?), pi(4)?, pi(5)?, pi(6)?, pi(7)?)
        }
        "effects" => RecordedStep::SetEffects(text_to_effects(&t[1..])?),
        "stop" => RecordedStep::Stop(line.strip_prefix("stop ").unwrap_or("").to_string()),
        "menu" => RecordedStep::MenuCommand(line.strip_prefix("menu ").unwrap_or("").to_string()),
        _ => return None,
    })
}

/// Where the recorded actions live between sessions.
pub(crate) fn actions_file_path() -> Option<PathBuf> {
    std::env::home_dir().map(|h| h.join(".pictodesk").join("actions.txt"))
}

/// Tool Presets on disk: one per line, tab-separated, the
/// dynamics block hex-encoded (the .ABR v2 layout).
pub(crate) fn load_tool_presets() -> Vec<ToolPreset> {
    let path = pictodesk_dir().join("tool-presets.txt");
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&path) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 8 {
                continue;
            }
            let (Ok(tid), Ok(size), Ok(hard), Ok(flow)) = (
                f[1].parse::<usize>(),
                f[2].parse::<f32>(),
                f[3].parse::<f32>(),
                f[4].parse::<f32>(),
            ) else {
                continue;
            };
            let rgb = |t: &str| {
                let p: Vec<u8> = t.split(',').filter_map(|x| x.parse::<u8>().ok()).collect();
                if p.len() == 3 {
                    (p[0], p[1], p[2])
                } else {
                    (0, 0, 0)
                }
            };
            let dyn_ = hex_decode(f[7])
                .and_then(|b| dyn_from_bytes(&b, 0))
                .map(|(d, _)| d)
                .unwrap_or_default();
            out.push(ToolPreset {
                name: f[0].to_string(),
                tool: TOOLS.get(tid).copied().unwrap_or(Tool::Brush),
                size,
                hardness: hard,
                dyn_,
                flow,
                fg: rgb(f[5]),
                bg: rgb(f[6]),
            });
        }
    }
    if out.is_empty() {
        let seed = |name: &str, tool: Tool, size: f32, hard: f32, air: bool, flow: f32| {
            let dyn_ = BrushDynamics {
                airbrush: air,
                ..BrushDynamics::default()
            };
            ToolPreset {
                name: name.into(),
                tool,
                size,
                hardness: hard,
                dyn_,
                flow,
                fg: (0, 0, 0),
                bg: (255, 255, 255),
            }
        };
        out = vec![
            seed("Hard Round 13", Tool::Brush, 13.0, 1.0, false, 1.0),
            seed("Soft Round 100", Tool::Brush, 100.0, 0.0, false, 1.0),
            seed("Airbrush Soft", Tool::Brush, 45.0, 0.0, true, 0.5),
            seed("Eraser Hard 19", Tool::Eraser, 19.0, 1.0, false, 1.0),
            seed("Clone Stamp 27", Tool::Stamp, 27.0, 0.6, false, 1.0),
        ];
    }
    out
}

pub(crate) fn save_tool_presets(presets: &[ToolPreset]) {
    let dir = pictodesk_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut text = String::new();
    for p in presets {
        let mut bytes = Vec::new();
        dyn_to_bytes(&p.dyn_, &mut bytes);
        text += &format!(
            "{}\t{}\t{}\t{}\t{}\t{},{},{}\t{},{},{}\t{}\n",
            p.name.replace('\t', " "),
            tool_id(p.tool),
            p.size,
            p.hardness,
            p.flow,
            p.fg.0,
            p.fg.1,
            p.fg.2,
            p.bg.0,
            p.bg.1,
            p.bg.2,
            hex_encode(&bytes)
        );
    }
    let _ = std::fs::write(dir.join("tool-presets.txt"), text);
}

pub(crate) fn load_workspaces() -> Vec<(String, Workspace)> {
    let path = pictodesk_dir().join("workspaces.txt");
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&path) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 3 || f[1].len() < 9 {
                continue;
            }
            let mut panels = [false; 9];
            for (i, c) in f[1].chars().take(9).enumerate() {
                panels[i] = c == '1';
            }
            out.push((
                f[0].to_string(),
                Workspace {
                    panels,
                    options_bar: f[1].chars().nth(9) == Some('1'),
                    display_mode: f[2].parse().unwrap_or(0),
                },
            ));
        }
    }
    out
}

pub(crate) fn load_actions_state() -> (Vec<String>, Vec<RecordedAction>) {
    let mut sets = vec!["Default Set".to_string()];
    let mut actions = Vec::new();
    let Some(path) = actions_file_path() else {
        return (sets, actions);
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return (sets, actions);
    };
    let mut cur: Option<usize> = None;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("set ") {
            if !sets.contains(&name.to_string()) {
                sets.push(name.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("action ") {
            let mut parts = rest.splitn(2, '\t');
            let set = parts.next().unwrap_or("Default Set").to_string();
            let name = parts.next().unwrap_or("Action").to_string();
            if !sets.contains(&set) {
                sets.push(set.clone());
            }
            actions.push(RecordedAction {
                name,
                steps: Vec::new(),
                set,
            });
            cur = Some(actions.len() - 1);
        } else if let (Some(i), Some(step)) = (cur, line_to_step(line)) {
            actions[i].steps.push(step);
        }
    }
    (sets, actions)
}

/// .ACO (Adobe Color Swatches): version 1 block (RGB entries)
/// plus a version 2 block with names. Components are u16,
/// 8-bit values scaled by 257.
/// Paths -> a minimal Illustrator-3-style EPS text file
/// (Phase 6.5 Batch 6d): moveto/curveto/closepath per subpath,
/// stroked, not filled.
/// Canonical combo for a key press: modifiers in order, then
/// the key ("primary+z", "primary+shift+z", "b", "primary+=").
/// Shift only counts for letter keys under primary (so plain
/// 'F' still means "f"); '+' normalizes to '=' (Zoom In).
pub(crate) fn combo_for_key(c: char, primary: bool, shift: bool, alt: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if primary {
        parts.push("primary".into());
    }
    if primary && shift && c.is_ascii_alphabetic() {
        parts.push("shift".into());
    }
    if alt {
        parts.push("alt".into());
    }
    let key = if c == '+' { '=' } else { c };
    parts.push(key.to_ascii_lowercase().to_string());
    parts.join("+")
}

/// Display form of a canonical combo: "primary" is Cmd on macOS,
/// Ctrl elsewhere; "tab" reads as Tab.
pub(crate) fn combo_display(combo: &str) -> String {
    let primary = if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    };
    combo
        .split('+')
        .map(|p| match p {
            "primary" => primary.to_string(),
            "shift" => "Shift".to_string(),
            "alt" => "Alt".to_string(),
            "tab" => "Tab".to_string(),
            other => other.to_ascii_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Keyboard shortcuts: defaults, with per-user overrides from
/// ~/.pictodesk/shortcuts.txt ("id=combo", one per line).
pub(crate) fn default_shortcuts() -> Vec<ShortcutDef> {
    let mut defs: Vec<ShortcutDef> = Vec::new();
    // Tool groups: one row per distinct tool key, in toolbox
    // order, named after every tool on the key.
    let mut seen: Vec<char> = Vec::new();
    for t in TOOLS {
        let k = t.key();
        if seen.contains(&k) {
            continue;
        }
        seen.push(k);
        let names: Vec<&str> = TOOLS
            .iter()
            .copied()
            .filter(|u| u.key() == k)
            .map(|u| u.name())
            .collect();
        defs.push(ShortcutDef {
            id: format!("tool:{k}"),
            label: names.join(" / "),
            combo: k.to_string(),
        });
    }
    let cmds: &[(&str, &str, &str)] = &[
        ("undo", "Undo", "primary+z"),
        ("redo", "Redo", "primary+shift+z"),
        ("cut", "Cut", "primary+x"),
        ("copy", "Copy", "primary+c"),
        ("paste", "Paste", "primary+v"),
        ("copy-merged", "Copy Merged", "primary+shift+c"),
        ("paste-into", "Paste Into", "primary+shift+v"),
        ("save", "Save", "primary+s"),
        ("open", "Open...", "primary+o"),
        ("new", "New...", "primary+n"),
        ("close-tab", "Close Tab", "primary+w"),
        ("new-layer", "New Layer", "primary+shift+n"),
        ("merge-down", "Merge Down", "primary+e"),
        ("free-transform", "Free Transform", "primary+t"),
        ("bring-forward", "Bring Forward", "primary+]"),
        ("send-backward", "Send Backward", "primary+["),
        ("select-all", "Select All", "primary+a"),
        ("select-none", "Deselect", "primary+d"),
        ("invert", "Invert", "primary+i"),
        ("zoom-in", "Zoom In", "primary+="),
        ("zoom-out", "Zoom Out", "primary+-"),
        ("zoom-fit", "Fit on Screen", "primary+0"),
        ("zoom-100", "Actual Pixels", "primary+1"),
        ("next-tab", "Next Tab", "primary+tab"),
        ("prev-tab", "Previous Tab", "primary+shift+tab"),
        ("screen-mode", "Screen Mode", "f"),
        ("quick-mask", "Quick Mask", "q"),
        ("swap-colors", "Swap Foreground/Background", "x"),
        ("default-colors", "Default Foreground/Background", "d"),
        ("brush-smaller", "Decrease Brush Size", "["),
        ("brush-larger", "Increase Brush Size", "]"),
    ];
    for (id, label, combo) in cmds {
        defs.push(ShortcutDef {
            id: (*id).into(),
            label: (*label).into(),
            combo: (*combo).into(),
        });
    }
    defs
}

pub(crate) fn load_shortcuts() -> Vec<ShortcutDef> {
    let mut defs = default_shortcuts();
    let path = pictodesk_dir().join("shortcuts.txt");
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            let Some((id, combo)) = line.split_once('=') else {
                continue;
            };
            if let Some(d) = defs.iter_mut().find(|d| d.id == id.trim()) {
                d.combo = combo.trim().to_string();
            }
        }
    }
    defs
}

pub(crate) fn paths_to_ai(doc: &Document) -> String {
    let mut out =
        String::from("%!PS-Adobe-3.0 EPSF-3.0\n%%Creator: PictoDesk\n%%BoundingBox: 0 0 ");
    out.push_str(&format!("{} {}\n", doc.width, doc.height));
    out.push_str("%%EndComments\n1 setlinewidth\n0 setgray\n");
    let flip = |y: f32| doc.height as f32 - y;
    pub(crate) fn emit(out: &mut String, p: &pictodesk_core::path::Path, height: f32) {
        for sub in &p.subpaths {
            if sub.anchors.is_empty() {
                continue;
            }
            let a0 = &sub.anchors[0];
            out.push_str(&format!(
                "newpath\n{} {} moveto\n",
                a0.point.0,
                height - a0.point.1
            ));
            let n = sub.anchors.len();
            let segs = if sub.closed { n } else { n - 1 };
            for j in 0..segs {
                let a = &sub.anchors[j];
                let b = &sub.anchors[(j + 1) % n];
                out.push_str(&format!(
                    "{} {} {} {} {} {} curveto\n",
                    a.out_ctrl.0,
                    height - a.out_ctrl.1,
                    b.in_ctrl.0,
                    height - b.in_ctrl.1,
                    b.point.0,
                    height - b.point.1
                ));
            }
            if sub.closed {
                out.push_str("closepath\n");
            }
            out.push_str("stroke\n");
        }
    }
    let h = doc.height as f32;
    if let Some(wp) = &doc.work_path {
        emit(&mut out, wp, h);
    }
    for p in &doc.paths {
        emit(&mut out, p, h);
    }
    let _ = flip;
    out.push_str("showpage\n%%EOF\n");
    out
}

pub(crate) fn write_aco(path: &std::path::Path, colors: &[(u8, u8, u8)]) -> std::io::Result<()> {
    let mut out: Vec<u8> = Vec::new();
    let block = |out: &mut Vec<u8>, version: u16| {
        out.extend_from_slice(&version.to_be_bytes());
        out.extend_from_slice(&(colors.len() as u16).to_be_bytes());
        for (i, (r, g, b)) in colors.iter().enumerate() {
            out.extend_from_slice(&0u16.to_be_bytes()); // RGB space
            for v in [r, g, b] {
                out.extend_from_slice(&((*v as u16) * 257).to_be_bytes());
            }
            out.extend_from_slice(&0u16.to_be_bytes());
            if version == 2 {
                let name = format!("Color {}", i + 1);
                out.extend_from_slice(&((name.len() as u32 + 1) * 2).to_be_bytes());
                for ch in name.encode_utf16() {
                    out.extend_from_slice(&ch.to_be_bytes());
                }
                out.extend_from_slice(&0u16.to_be_bytes());
            }
        }
    };
    block(&mut out, 1);
    block(&mut out, 2);
    std::fs::write(path, out)
}

pub(crate) fn read_aco(bytes: &[u8]) -> Option<Vec<(u8, u8, u8)>> {
    let u16be = |i: usize| -> Option<u16> {
        Some(u16::from_be_bytes([*bytes.get(i)?, *bytes.get(i + 1)?]))
    };
    if u16be(0)? != 1 {
        return None;
    }
    let count = u16be(2)? as usize;
    let mut out = Vec::new();
    let mut i = 4usize;
    for _ in 0..count {
        let space = u16be(i)?;
        if space == 0 {
            let (r, g, b) = (u16be(i + 2)?, u16be(i + 4)?, u16be(i + 6)?);
            out.push(((r / 257) as u8, (g / 257) as u8, (b / 257) as u8));
        }
        i += 10;
    }
    Some(out)
}

/// Brush preset set: a compact binary ("PDABR1" + count +
/// (diameter, hardness) f32 pairs). The full .abr format
/// carries sampled brush images; PictoDesk presets are round
/// brushes, so diameter + hardness is the whole preset.
/// A brush preset (Phase 7): tip size/hardness plus the
/// full dynamics state, so a preset recalls a whole brush.
/// A Tool Preset (7.0): a tool plus the option state it was
/// captured with — tip, dynamics, flow and colors.
#[derive(Clone, PartialEq)]
pub(crate) struct ToolPreset {
    pub(crate) name: String,
    pub(crate) tool: Tool,
    pub(crate) size: f32,
    pub(crate) hardness: f32,
    pub(crate) dyn_: BrushDynamics,
    pub(crate) flow: f32,
    pub(crate) fg: (u8, u8, u8),
    pub(crate) bg: (u8, u8, u8),
}

/// One File Browser entry (7.0): a folder or image file
/// with its letter rank, rotate-on-open flag and thumbnail.
#[derive(Clone)]
pub(crate) struct BrowserEntry {
    pub(crate) path: std::path::PathBuf,
    pub(crate) name: String,
    pub(crate) is_dir: bool,
    pub(crate) rank: u8,   // 0 = unranked, 1..=5 = A..E
    pub(crate) rotate: i8, // -1 CCW, 0 none, 1 CW (applied on open)
    pub(crate) tw: u32,
    pub(crate) th: u32,
    pub(crate) thumb: Vec<u8>,
    pub(crate) info: String,
}

/// A Workspace (7.0): the panel visibility set, the Options
/// Bar, and the screen mode.
#[derive(Clone)]
pub(crate) struct Workspace {
    pub(crate) panels: [bool; 9], // cp, brushes, tool presets, histogram,
    // palette, navigator, actions, history, info
    pub(crate) options_bar: bool,
    pub(crate) display_mode: u8,
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrushPreset {
    pub(crate) size: f32,
    pub(crate) hardness: f32,
    pub(crate) dyn_: BrushDynamics,
}

pub(crate) fn dyn_to_bytes(d: &BrushDynamics, out: &mut Vec<u8>) {
    let fs = [
        d.spacing,
        d.angle_deg,
        d.roundness,
        d.size_jitter,
        d.min_diameter,
        d.angle_jitter,
        d.round_jitter,
        d.min_roundness,
        d.scatter,
        d.count_jitter,
        d.tex_scale,
        d.tex_depth,
        d.tex_min_depth,
        d.dual_size,
        d.dual_hardness,
        d.dual_scatter,
        d.fb_jitter,
        d.hue_jitter,
        d.sat_jitter,
        d.bri_jitter,
        d.opacity_jitter,
        d.flow_jitter,
    ];
    for v in fs {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&[
        d.size_control,
        d.angle_control,
        d.round_control,
        d.count,
        d.tex_mode,
        d.dual_mode,
        d.opacity_control,
        d.flow_control,
    ]);
    let mut bits: u16 = 0;
    for (i, b) in [
        d.flip_x,
        d.flip_y,
        d.both_axes,
        d.texture_on,
        d.tex_invert,
        d.protect_texture,
        d.dual_on,
        d.noise,
        d.wet_edges,
        d.airbrush,
        d.smoothing,
    ]
    .iter()
    .enumerate()
    {
        if *b {
            bits |= 1 << i;
        }
    }
    out.extend_from_slice(&bits.to_be_bytes());
    out.extend_from_slice(&d.fade_steps.to_be_bytes());
}

pub(crate) fn dyn_from_bytes(bytes: &[u8], mut b: usize) -> Option<(BrushDynamics, usize)> {
    let mut d = BrushDynamics::default();
    let mut fs = [
        &mut d.spacing,
        &mut d.angle_deg,
        &mut d.roundness,
        &mut d.size_jitter,
        &mut d.min_diameter,
        &mut d.angle_jitter,
        &mut d.round_jitter,
        &mut d.min_roundness,
        &mut d.scatter,
        &mut d.count_jitter,
        &mut d.tex_scale,
        &mut d.tex_depth,
        &mut d.tex_min_depth,
        &mut d.dual_size,
        &mut d.dual_hardness,
        &mut d.dual_scatter,
        &mut d.fb_jitter,
        &mut d.hue_jitter,
        &mut d.sat_jitter,
        &mut d.bri_jitter,
        &mut d.opacity_jitter,
        &mut d.flow_jitter,
    ];
    for slot in fs.iter_mut() {
        **slot = f32::from_be_bytes(bytes.get(b..b + 4)?.try_into().ok()?);
        b += 4;
    }
    let u8s = bytes.get(b..b + 8)?;
    d.size_control = u8s[0];
    d.angle_control = u8s[1];
    d.round_control = u8s[2];
    d.count = u8s[3];
    d.tex_mode = u8s[4];
    d.dual_mode = u8s[5];
    d.opacity_control = u8s[6];
    d.flow_control = u8s[7];
    b += 8;
    let bits = u16::from_be_bytes(bytes.get(b..b + 2)?.try_into().ok()?);
    b += 2;
    let flags = [
        &mut d.flip_x,
        &mut d.flip_y,
        &mut d.both_axes,
        &mut d.texture_on,
        &mut d.tex_invert,
        &mut d.protect_texture,
        &mut d.dual_on,
        &mut d.noise,
        &mut d.wet_edges,
        &mut d.airbrush,
        &mut d.smoothing,
    ];
    for (i, f) in flags.into_iter().enumerate() {
        *f = bits & (1 << i) != 0;
    }
    d.fade_steps = u32::from_be_bytes(bytes.get(b..b + 4)?.try_into().ok()?);
    b += 4;
    Some((d, b))
}

/// .ABR v2 (Phase 7): presets carry dynamics. v1 files
/// (size/hardness only) still load, with default dynamics.
/// (Still PictoDesk's own compact format — named in the
/// Phase 6.5 delivery; v2 extends it the same way.)
pub(crate) fn write_abr(path: &std::path::Path, presets: &[BrushPreset]) -> std::io::Result<()> {
    let mut out: Vec<u8> = b"PDABR2".to_vec();
    out.extend_from_slice(&(presets.len() as u16).to_be_bytes());
    for p in presets {
        out.extend_from_slice(&p.size.to_be_bytes());
        out.extend_from_slice(&p.hardness.to_be_bytes());
        dyn_to_bytes(&p.dyn_, &mut out);
    }
    std::fs::write(path, out)
}

pub(crate) fn read_abr(bytes: &[u8]) -> Option<Vec<BrushPreset>> {
    match bytes.get(..6)? {
        b"PDABR1" => {
            let count = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;
            let mut out = Vec::new();
            for j in 0..count {
                let b = 8 + j * 8;
                let sz = f32::from_be_bytes(bytes.get(b..b + 4)?.try_into().ok()?);
                let h = f32::from_be_bytes(bytes.get(b + 4..b + 8)?.try_into().ok()?);
                out.push(BrushPreset {
                    size: sz,
                    hardness: h,
                    dyn_: BrushDynamics::default(),
                });
            }
            Some(out)
        }
        b"PDABR2" => {
            let count = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;
            let mut out = Vec::new();
            let mut b = 8usize;
            for _ in 0..count {
                let sz = f32::from_be_bytes(bytes.get(b..b + 4)?.try_into().ok()?);
                let h = f32::from_be_bytes(bytes.get(b + 4..b + 8)?.try_into().ok()?);
                b += 8;
                let (dyn_, nb) = dyn_from_bytes(bytes, b)?;
                b = nb;
                out.push(BrushPreset {
                    size: sz,
                    hardness: h,
                    dyn_,
                });
            }
            Some(out)
        }
        _ => None,
    }
}

/// The adjustment step a settings-capable dialog currently
/// represents (Phase 6.5 Batch 6d settings save).
pub(crate) fn dialog_candidate(d: &Dialog) -> Option<RecordedStep> {
    match d.kind {
        DialogKind::BrightnessContrast
        | DialogKind::HueSaturation
        | DialogKind::ColorBalance
        | DialogKind::Levels
        | DialogKind::Curves
        | DialogKind::GradientMap
        | DialogKind::PhotoFilter => Some(RecordedStep::Adjustment(App::adjustment_from_dialog(d))),
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
        DialogKind::SelectiveColor => Some(RecordedStep::SelectiveColor(
            d.fx_a,
            [
                d.vals[0] as i32,
                d.vals[1] as i32,
                d.vals[2] as i32,
                d.vals[3] as i32,
            ],
            d.fx_b,
        )),
        DialogKind::ReplaceColor => Some(RecordedStep::ReplaceColor(
            d.rc_target,
            d.vals[0] as i32,
            d.vals[1] as i32,
            d.vals[2] as i32,
            d.vals[3] as i32,
        )),
        _ => None,
    }
}

pub(crate) fn line_to_adj(t: &[&str]) -> Option<pictodesk_core::adjust::Adjustment> {
    use pictodesk_core::adjust::Adjustment;
    let p = |i: usize| t.get(i).and_then(|v| v.parse::<u8>().ok());
    let pf = |i: usize| t.get(i).and_then(|v| v.parse::<f32>().ok());
    let pi = |i: usize| t.get(i).and_then(|v| v.parse::<i32>().ok());
    Some(match t.get(1).copied()? {
        "levels" => Adjustment::Levels {
            in_black: p(2)?,
            in_white: p(3)?,
            gamma: pf(4)?,
            out_black: p(5)?,
            out_white: p(6)?,
        },
        "bc" => Adjustment::BrightnessContrast {
            brightness: pi(2)?,
            contrast: pi(3)?,
        },
        "hue" => Adjustment::HueSaturation {
            hue: pi(2)?,
            saturation: pi(3)?,
            lightness: pi(4)?,
        },
        "photo" => Adjustment::PhotoFilter {
            color: (p(2)?, p(3)?, p(4)?),
            density: p(5)?,
        },
        "gradmap" => Adjustment::GradientMap {
            from: (p(2)?, p(3)?, p(4)?),
            to: (p(5)?, p(6)?, p(7)?),
            reverse: p(8)? != 0,
        },
        "curves" => {
            let n = t.get(2)?.parse::<usize>().ok()?;
            let mut points = Vec::new();
            for j in 0..n {
                points.push((p(3 + j * 2)?, p(4 + j * 2)?));
            }
            Adjustment::Curves { points }
        }
        "colorbal" => Adjustment::ColorBalance {
            shadows: (pi(2)?, pi(3)?, pi(4)?),
            midtones: (pi(5)?, pi(6)?, pi(7)?),
            highlights: (pi(8)?, pi(9)?, pi(10)?),
        },
        _ => return None,
    })
}

/// Effects <-> text for step serialization: every spec field in
/// a fixed order, blend modes as ALL_BLEND_MODES indices, colors
/// as 8-bit RGBA quads.
pub(crate) fn effects_to_text(e: &pictodesk_core::effects::Effects) -> String {
    use pictodesk_core::effects::*;
    let mode = |m: pictodesk_core::blend::BlendMode| {
        pictodesk_core::blend::ALL_BLEND_MODES
            .iter()
            .position(|x| *x == m)
            .unwrap_or(0)
    };
    let col = |p: Pixel16| {
        let q = p.to_rgba8();
        format!("{} {} {} {}", q.0, q.1, q.2, q.3)
    };
    let shadow = |s: &ShadowSpec| {
        format!(
            "{} {} {} {} {} {} {} {} {} {}",
            s.enabled as u8,
            mode(s.mode),
            col(s.color),
            s.opacity,
            s.angle,
            s.use_global_angle as u8,
            s.distance,
            s.blur,
            s.intensity,
            s.contour
        )
    };
    let glow = |s: &GlowSpec| {
        format!(
            "{} {} {} {} {} {} {} {}",
            s.enabled as u8,
            mode(s.mode),
            col(s.color),
            s.opacity,
            s.blur,
            s.intensity,
            s.source_center as u8,
            s.contour
        )
    };
    [
        shadow(&e.drop_shadow),
        shadow(&e.inner_shadow),
        glow(&e.outer_glow),
        glow(&e.inner_glow),
        format!(
            "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            e.bevel.enabled as u8,
            e.bevel.style,
            e.bevel.depth,
            e.bevel.up as u8,
            mode(e.bevel.hi_mode),
            col(e.bevel.hi_color),
            e.bevel.hi_opacity,
            mode(e.bevel.sh_mode),
            col(e.bevel.sh_color),
            e.bevel.sh_opacity,
            e.bevel.angle,
            e.bevel.use_global_angle as u8,
            e.bevel.contour,
            e.bevel.texture as u8,
            e.bevel.texture_kind,
            e.bevel.texture_depth
        ),
        format!(
            "{} {} {} {} {} {} {} {} {}",
            e.satin.enabled as u8,
            mode(e.satin.mode),
            col(e.satin.color),
            e.satin.opacity,
            e.satin.angle,
            e.satin.distance,
            e.satin.blur,
            e.satin.invert as u8,
            e.satin.contour
        ),
        format!(
            "{} {} {} {}",
            e.color_overlay.enabled as u8,
            mode(e.color_overlay.mode),
            col(e.color_overlay.color),
            e.color_overlay.opacity
        ),
        format!(
            "{} {} {} {} {} {} {} {}",
            e.gradient_overlay.enabled as u8,
            mode(e.gradient_overlay.mode),
            e.gradient_overlay.opacity,
            col(e.gradient_overlay.from),
            col(e.gradient_overlay.to),
            e.gradient_overlay.style,
            e.gradient_overlay.angle,
            e.gradient_overlay.scale
        ),
        format!(
            "{} {} {} {} {}",
            e.pattern_overlay.enabled as u8,
            mode(e.pattern_overlay.mode),
            e.pattern_overlay.opacity,
            e.pattern_overlay.kind,
            e.pattern_overlay.scale
        ),
        format!(
            "{} {} {} {} {} {}",
            e.stroke.enabled as u8,
            mode(e.stroke.mode),
            col(e.stroke.color),
            e.stroke.opacity,
            e.stroke.size,
            e.stroke.position
        ),
    ]
    .join(" ")
}

#[allow(unused_assignments)]
pub(crate) fn text_to_effects(t: &[&str]) -> Option<pictodesk_core::effects::Effects> {
    use pictodesk_core::effects::*;
    let mut i = 0usize;
    macro_rules! u8v {
        () => {{
            let v = t.get(i)?.parse::<u8>().ok()?;
            i += 1;
            v
        }};
    }
    macro_rules! u32v {
        () => {{
            let v = t.get(i)?.parse::<u32>().ok()?;
            i += 1;
            v
        }};
    }
    macro_rules! i32v {
        () => {{
            let v = t.get(i)?.parse::<i32>().ok()?;
            i += 1;
            v
        }};
    }
    macro_rules! f32v {
        () => {{
            let v = t.get(i)?.parse::<f32>().ok()?;
            i += 1;
            v
        }};
    }
    macro_rules! boolv {
        () => {
            u8v!() != 0
        };
    }
    macro_rules! modev {
        () => {{
            let idx = u32v!() as usize;
            *pictodesk_core::blend::ALL_BLEND_MODES.get(idx)?
        }};
    }
    macro_rules! colv {
        () => {
            Pixel16::from_rgba8(u8v!(), u8v!(), u8v!(), u8v!())
        };
    }
    macro_rules! shadow {
        () => {
            ShadowSpec {
                enabled: boolv!(),
                mode: modev!(),
                color: colv!(),
                opacity: f32v!(),
                angle: i32v!(),
                use_global_angle: boolv!(),
                distance: u32v!(),
                blur: u32v!(),
                intensity: u32v!(),
                contour: u8v!(),
            }
        };
    }
    macro_rules! glow {
        () => {
            GlowSpec {
                enabled: boolv!(),
                mode: modev!(),
                color: colv!(),
                opacity: f32v!(),
                blur: u32v!(),
                intensity: u32v!(),
                source_center: boolv!(),
                contour: u8v!(),
            }
        };
    }
    Some(Effects {
        drop_shadow: shadow!(),
        inner_shadow: shadow!(),
        outer_glow: glow!(),
        inner_glow: glow!(),
        bevel: BevelSpec {
            enabled: boolv!(),
            style: u8v!(),
            depth: u32v!(),
            up: boolv!(),
            hi_mode: modev!(),
            hi_color: colv!(),
            hi_opacity: f32v!(),
            sh_mode: modev!(),
            sh_color: colv!(),
            sh_opacity: f32v!(),
            angle: i32v!(),
            use_global_angle: boolv!(),
            contour: u8v!(),
            texture: boolv!(),
            texture_kind: u8v!(),
            texture_depth: u32v!(),
        },
        satin: SatinSpec {
            enabled: boolv!(),
            mode: modev!(),
            color: colv!(),
            opacity: f32v!(),
            angle: i32v!(),
            distance: u32v!(),
            blur: u32v!(),
            invert: boolv!(),
            contour: u8v!(),
        },
        color_overlay: ColorOverlaySpec {
            enabled: boolv!(),
            mode: modev!(),
            color: colv!(),
            opacity: f32v!(),
        },
        gradient_overlay: GradientOverlaySpec {
            enabled: boolv!(),
            mode: modev!(),
            opacity: f32v!(),
            from: colv!(),
            to: colv!(),
            style: u8v!(),
            angle: i32v!(),
            scale: u32v!(),
        },
        pattern_overlay: PatternOverlaySpec {
            enabled: boolv!(),
            mode: modev!(),
            opacity: f32v!(),
            kind: u8v!(),
            scale: u32v!(),
        },
        stroke: StrokeSpec {
            enabled: boolv!(),
            mode: modev!(),
            color: colv!(),
            opacity: f32v!(),
            size: u32v!(),
            position: u8v!(),
        },
    })
}

/// Apply one recorded step to a document (shared by action replay
/// and batch processing). Adjustments land on the active layer.
pub(crate) fn apply_step(doc: &mut Document, step: &RecordedStep, sel: Option<&Selection>) {
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
            Adjustment::GradientMap { from, to, reverse } => {
                adjust::gradient_map(doc, layer, sel, *from, *to, *reverse)
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
        RecordedStep::AutoLevels => adjust::auto_levels(doc, layer, sel),
        RecordedStep::AutoContrast => adjust::auto_contrast(doc, layer, sel),
        RecordedStep::AutoColor => adjust::auto_color(doc, layer, sel),
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
        RecordedStep::Extrude(size, depth, kind) => {
            filter::extrude(doc, layer, sel, *size, *depth, *kind, 12345)
        }
        RecordedStep::TraceContour(level, upper) => {
            filter::trace_contour(doc, layer, sel, *level, *upper)
        }
        RecordedStep::ColorHalftone(r) => filter::color_halftone(doc, layer, sel, *r),
        RecordedStep::Crystallize(cell) => filter::crystallize(doc, layer, sel, *cell, 12345),
        RecordedStep::Pointillize(bg, cell) => {
            filter::pointillize(doc, layer, sel, *cell, *bg, 12345)
        }
        RecordedStep::SmartBlur(r, t, e) => filter::smart_blur(doc, layer, sel, *r, *t, *e),
        RecordedStep::DeInterlace(even, interp) => {
            filter::deinterlace(doc, layer, sel, *even, *interp)
        }
        RecordedStep::SharpenMore => filter::sharpen_more(doc, layer, sel),
        RecordedStep::NtscColors => filter::ntsc_colors(doc, layer, sel),
        RecordedStep::Wave(g, wl, amp, hs, vs, t, wrap) => {
            filter::wave(doc, layer, sel, *g, *wl, *amp, *hs, *vs, *t, *wrap, 12345)
        }
        RecordedStep::Glass(d, s, sc) => filter::glass(doc, layer, sel, *d, *s, *sc, 12345),
        RecordedStep::DiffuseGlow(g, gl, c) => {
            filter::diffuse_glow(doc, layer, sel, *g, *gl, *c, 12345)
        }
        RecordedStep::Displace(map, mw, mh, hs, vs, tile, wrap) => {
            filter::displace(doc, layer, sel, map, *mw, *mh, *hs, *vs, *tile, *wrap)
        }
        RecordedStep::LensFlare(b, xp, yp, lens) => filter::lens_flare(
            doc,
            layer,
            sel,
            *b,
            *xp / 100.0 * doc.width as f32,
            *yp / 100.0 * doc.height as f32,
            *lens,
        ),
        RecordedStep::Lighting(i, f, xp, yp, e, spot, tex) => filter::lighting(
            doc,
            layer,
            sel,
            *i,
            *f,
            *xp / 100.0 * doc.width as f32,
            *yp / 100.0 * doc.height as f32,
            *e,
            *spot,
            *tex,
        ),
        RecordedStep::TextureFill(tex, tw, th) => {
            filter::texture_fill(doc, layer, sel, tex, *tw, *th)
        }
        RecordedStep::ColoredPencil(a, b, c) => {
            filter::colored_pencil(doc, layer, sel, *a, *b, *c, 12345)
        }
        RecordedStep::Cutout(a, b, c) => filter::cutout(doc, layer, sel, *a, *b, *c),
        RecordedStep::DryBrush(a, b, c) => filter::dry_brush(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::FilmGrain(a, b, c) => filter::film_grain(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::Fresco(a, b, c) => filter::fresco(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::NeonGlow(a, b, col) => filter::neon_glow(doc, layer, sel, *a, *b, *col),
        RecordedStep::PaintDaubs(a, b, t) => {
            filter::paint_daubs(doc, layer, sel, *a, *b, *t, 12345)
        }
        RecordedStep::PaletteKnife(a, b, c) => filter::palette_knife(doc, layer, sel, *a, *b, *c),
        RecordedStep::PlasticWrap(a, b, c) => filter::plastic_wrap(doc, layer, sel, *a, *b, *c),
        RecordedStep::PosterEdges(a, b, c) => filter::poster_edges(doc, layer, sel, *a, *b, *c),
        RecordedStep::RoughPastels(a, b, c, e, t) => {
            filter::rough_pastels(doc, layer, sel, *a, *b, *c, *e, *t)
        }
        RecordedStep::SmudgeStick(a, b, c) => filter::smudge_stick(doc, layer, sel, *a, *b, *c),
        RecordedStep::Sponge(a, b, c) => filter::sponge(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::Underpainting(a, b, c, e, t) => {
            filter::underpainting(doc, layer, sel, *a, *b, *c, *e, *t)
        }
        RecordedStep::Watercolor(a, b, c) => filter::watercolor(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::AccentedEdges(a, b, c) => filter::accented_edges(doc, layer, sel, *a, *b, *c),
        RecordedStep::AngledStrokes(a, b, c) => {
            filter::angled_strokes(doc, layer, sel, *a, *b, *c, 12345)
        }
        RecordedStep::Crosshatch(a, b, c) => filter::crosshatch(doc, layer, sel, *a, *b, *c),
        RecordedStep::DarkStrokes(a, b, c) => filter::dark_strokes(doc, layer, sel, *a, *b, *c),
        RecordedStep::InkOutlines(a, b, c) => filter::ink_outlines(doc, layer, sel, *a, *b, *c),
        RecordedStep::Spatter(a, b) => filter::spatter(doc, layer, sel, *a, *b, 12345),
        RecordedStep::SprayedStrokes(a, b, t) => {
            filter::sprayed_strokes(doc, layer, sel, *a, *b, *t, 12345)
        }
        RecordedStep::SumiE(a, b, c) => filter::sumi_e(doc, layer, sel, *a, *b, *c),
        RecordedStep::ChalkCharcoal(a, b, c, fg, bg) => {
            filter::chalk_charcoal(doc, layer, sel, *a, *b, *c, *fg, *bg, 12345)
        }
        RecordedStep::CharcoalFx(a, b, c) => filter::charcoal_fx(doc, layer, sel, *a, *b, *c),
        RecordedStep::Chrome(a, b) => filter::chrome(doc, layer, sel, *a, *b),
        RecordedStep::ConteCrayon(a, b, c, e, t, fg, bg) => {
            filter::conte_crayon(doc, layer, sel, *a, *b, *c, *e, *t, *fg, *bg)
        }
        RecordedStep::GraphicPen(a, b, t, fg, bg) => {
            filter::graphic_pen(doc, layer, sel, *a, *b, *t, *fg, *bg)
        }
        RecordedStep::HalftonePattern(a, b, t) => {
            filter::halftone_pattern(doc, layer, sel, *a, *b, *t)
        }
        RecordedStep::NotePaper(a, b, c) => filter::note_paper(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::Photocopy(a, b) => filter::photocopy(doc, layer, sel, *a, *b, 12345),
        RecordedStep::Plaster(a, b, t) => filter::plaster(doc, layer, sel, *a, *b, *t),
        RecordedStep::Reticulation(a, b, c, fg, bg) => {
            filter::reticulation(doc, layer, sel, *a, *b, *c, *fg, *bg, 12345)
        }
        RecordedStep::TornEdges(a, b, c, fg, bg) => {
            filter::torn_edges(doc, layer, sel, *a, *b, *c, *fg, *bg, 12345)
        }
        RecordedStep::WaterPaper(a, b, c, fg, bg) => {
            filter::water_paper(doc, layer, sel, *a, *b, *c, *fg, *bg, 12345)
        }
        RecordedStep::Craquelure(a, b, c) => filter::craquelure(doc, layer, sel, *a, *b, *c, 12345),
        RecordedStep::GrainFx(a, b, t) => filter::grain_fx(doc, layer, sel, *a, *b, *t, 12345),
        RecordedStep::MosaicTiles(a, b, c) => {
            filter::mosaic_tiles(doc, layer, sel, *a, *b, *c, 12345)
        }
        RecordedStep::StainedGlass(a, b, c) => {
            filter::stained_glass(doc, layer, sel, *a, *b, *c, 12345)
        }
        RecordedStep::Texturizer(a, b, t, l) => filter::texturizer(doc, layer, sel, *a, *b, *t, *l),
        RecordedStep::Diffuse(m) => filter::diffuse(doc, layer, sel, *m, 12345),
        RecordedStep::Mezzotint(k) => filter::mezzotint(doc, layer, sel, *k, 12345),
        RecordedStep::DifferenceClouds => filter::difference_clouds(doc, layer, sel, 12345),
        RecordedStep::SharpenEdges => filter::sharpen_edges(doc, layer, sel),
        RecordedStep::Fragment => filter::fragment(doc, layer, sel),
        RecordedStep::Facet => filter::facet(doc, layer, sel),
        RecordedStep::Transform3d(s, rx, ry) => filter::transform_3d(doc, layer, sel, *s, *rx, *ry),
        RecordedStep::ChannelMixer(m) => adjust::channel_mixer(doc, layer, sel, m),
        RecordedStep::SelectiveColor(range, adj, rel) => adjust::selective_color(
            doc,
            layer,
            sel,
            *range,
            [
                adj[0] as f32 / 100.0,
                adj[1] as f32 / 100.0,
                adj[2] as f32 / 100.0,
                adj[3] as f32 / 100.0,
            ],
            *rel,
        ),
        RecordedStep::ReplaceColor(t, fuzz, h, s2, l) => adjust::replace_color(
            doc,
            layer,
            sel,
            (t.0 as f32 / 255.0, t.1 as f32 / 255.0, t.2 as f32 / 255.0),
            *fuzz as f32,
            *h as f32,
            *s2 as f32,
            *l as f32,
        ),
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
        // Interactive steps are intercepted by action playback;
        // reaching apply_step (e.g. inside a droplet) is a no-op.
        RecordedStep::Stop(_) | RecordedStep::MenuCommand(_) => {}
    }
}

pub(crate) fn apply_steps(doc: &mut Document, steps: &[RecordedStep], sel: Option<&Selection>) {
    for step in steps {
        apply_step(doc, step, sel);
    }
}

/// An in-flight microphone capture: the live stream plus
/// the samples it has gathered so far.
pub(crate) struct AudioRec {
    pub(crate) stream: cpal::Stream,
    pub(crate) samples: std::sync::Arc<std::sync::Mutex<Vec<f32>>>,
    pub(crate) rate: u32,
    pub(crate) channels: u16,
}

/// Edit > Fade source (Phase 6.5): the tile changes of
/// the most recent filter/adjustment operation. `before`
/// tiles hold the pre-operation state, `after` tiles the
/// current state; Fade blends between them.
pub(crate) struct FadeSource {
    pub(crate) label: String,
    pub(crate) changes: Vec<pictodesk_core::history::TileChange>,
}

/// True for steps the Filter menu produces (Last Filter
/// tracking; adjustments and structural steps excluded).
pub(crate) fn is_filter_step(step: &RecordedStep) -> bool {
    use RecordedStep::*;
    matches!(
        step,
        GaussianBlur(_)
            | Unsharp(..)
            | AddNoise(_)
            | MotionBlur(..)
            | RadialBlur(_)
            | Median(_)
            | Despeckle
            | Emboss(_)
            | FindEdges
            | Solarize
            | Clouds(..)
            | DifferenceClouds
            | Mosaic(_)
            | HighPass(_)
            | CustomKernel(..)
            | Pinch(_)
            | Spherize(_)
            | Twirl(_)
            | Ripple(..)
            | OceanRipple(..)
            | Polar(_)
            | Shear(_)
            | ZigZag(..)
            | Maximum(_)
            | Minimum(_)
            | DustScratches(..)
            | Tiles(..)
            | GlowingEdges(..)
            | Patchwork(..)
            | StampEffect(..)
            | BasRelief(..)
            | Wind(..)
            | Extrude(..)
            | TraceContour(..)
            | ColorHalftone(_)
            | Crystallize(_)
            | Pointillize(..)
            | SmartBlur(..)
            | DeInterlace(..)
            | Sharpen
            | SharpenMore
            | SharpenEdges
            | NtscColors
            | Wave(..)
            | Glass(..)
            | DiffuseGlow(..)
            | Diffuse(_)
            | Displace(..)
            | LensFlare(..)
            | Lighting(..)
            | TextureFill(..)
            | ColoredPencil(..)
            | Cutout(..)
            | DryBrush(..)
            | FilmGrain(..)
            | Fresco(..)
            | NeonGlow(..)
            | PaintDaubs(..)
            | PaletteKnife(..)
            | PlasticWrap(..)
            | PosterEdges(..)
            | RoughPastels(..)
            | SmudgeStick(..)
            | Sponge(..)
            | Underpainting(..)
            | Watercolor(..)
            | AccentedEdges(..)
            | AngledStrokes(..)
            | Crosshatch(..)
            | DarkStrokes(..)
            | InkOutlines(..)
            | Spatter(..)
            | SprayedStrokes(..)
            | SumiE(..)
            | ChalkCharcoal(..)
            | CharcoalFx(..)
            | Chrome(..)
            | ConteCrayon(..)
            | GraphicPen(..)
            | HalftonePattern(..)
            | NotePaper(..)
            | Photocopy(..)
            | Plaster(..)
            | Reticulation(..)
            | TornEdges(..)
            | WaterPaper(..)
            | Craquelure(..)
            | GrainFx(..)
            | MosaicTiles(..)
            | StainedGlass(..)
            | Texturizer(..)
            | Mezzotint(_)
            | Fragment
            | Facet
            | OffsetPx(..)
            | Transform3d(..)
    )
}

/// A minimal one-page PDF wrapping a JPEG as a full-page image.
pub(crate) fn build_pdf_one_page(jpeg: &[u8], w: u32, h: u32) -> Vec<u8> {
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
pub(crate) fn spool_print(path: &std::path::Path, copies: u32) -> String {
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
pub(crate) fn space_name(s: pictodesk_core::color::WorkingSpace) -> &'static str {
    match s {
        pictodesk_core::color::WorkingSpace::Srgb => "sRGB",
        pictodesk_core::color::WorkingSpace::AdobeRgb => "Adobe RGB (1998)",
        pictodesk_core::color::WorkingSpace::ProPhoto => "ProPhoto RGB",
    }
}

impl App {
    /// Push a finished command onto the history stack and record
    /// the edit in the document's provenance log (stored in the
    /// `.pdk` file, like the classic editors' embedded history).
    pub(crate) fn push_cmd(&mut self, cmd: Box<dyn Command>) {
        // Any new edit invalidates the Fade source; the
        // operations that capture one set it after pushing.
        self.fade_source = None;
        self.doc.log_event(cmd.label());
        self.hist.push(cmd);
    }

    pub(crate) fn structural(&mut self, label: &str, f: impl FnOnce(&mut Document)) {
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
    pub(crate) fn apply_filter_now(
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

    /// If an Actions recording is running, append a step to it.
    pub(crate) fn record_step(&mut self, step: RecordedStep) {
        if is_filter_step(&step) {
            self.last_filter_step = Some(step.clone());
        }
        if self.recording {
            if let Some(a) = self.actions.get_mut(self.active_action) {
                a.steps.push(step);
            }
        }
    }

    /// Batch-process a folder with the selected action: every
    /// image file in the folder is opened, the action's steps are
    /// applied, and the result is written next to the original as
    /// `<name>-edited.png`. Originals are never overwritten.
    pub(crate) fn run_batch(&mut self, folder_text: &str) {
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

    pub(crate) fn save_actions_file(&self) {
        let Some(path) = actions_file_path() else {
            return;
        };
        let mut out = String::from("pictodesk-actions 1\n");
        for s in &self.action_sets {
            out.push_str(&format!("set {s}\n"));
        }
        let mut skipped = 0usize;
        for a in &self.actions {
            out.push_str(&format!("action {}\t{}\n", a.set, a.name));
            for st in &a.steps {
                match step_to_line(st) {
                    Some(l) => out.push_str(&format!("{l}\n")),
                    None => skipped += 1,
                }
            }
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&path, out).is_ok() && skipped > 0 {
            // Surface, never silent (full coverage should make
            // this unreachable).
            eprintln!("actions save: {skipped} step(s) not serializable");
        }
    }

    /// Replay an action onto the current document (Phase 6.5
    /// Batch 6d playback: plain steps run as one undoable block;
    /// Stop and Menu Command steps are interactive).
    pub(crate) fn play_action(&mut self, idx: usize) {
        let Some(action) = self.actions.get(idx).cloned() else {
            self.status = "No action to play".into();
            return;
        };
        if action.steps.is_empty() {
            self.status = "That action has no steps yet".into();
            return;
        }
        self.play_steps_from(action.steps.clone(), action.name.clone());
    }

    /// The playback engine: run plain steps until a Stop (pause
    /// with a dialog; OK resumes) or a Menu Command (run the
    /// menu item, then continue with the rest).
    pub(crate) fn play_steps_from(&mut self, steps: Vec<RecordedStep>, name: String) {
        let mut i = 0usize;
        let mut plain: Vec<RecordedStep> = Vec::new();
        while i < steps.len() {
            match &steps[i] {
                RecordedStep::Stop(_) | RecordedStep::MenuCommand(_) => break,
                st => plain.push(st.clone()),
            }
            i += 1;
        }
        if !plain.is_empty() {
            let sel = self.sel.clone();
            let label = format!("Play: {name}");
            self.structural(&label, |doc| {
                apply_steps(doc, &plain, sel.as_ref());
            });
        }
        if i >= steps.len() {
            self.status = format!("Played '{name}'");
            return;
        }
        let rest: Vec<RecordedStep> = steps[i + 1..].to_vec();
        match &steps[i] {
            RecordedStep::Stop(msg) => {
                self.play_pending = Some((rest, name));
                let mut d = Dialog::new(DialogKind::ActionStop, &self.doc);
                d.text = msg.clone();
                self.dialog = Some(d);
            }
            RecordedStep::MenuCommand(path) => {
                match self.menu_action_for_path(path) {
                    Some(act) => self.do_action(act),
                    None => self.status = format!("Menu item not found: {path}"),
                }
                if !rest.is_empty() {
                    self.play_steps_from(rest, name);
                } else {
                    self.status = format!("Played '{name}'");
                }
            }
            _ => unreachable!(),
        }
    }

    /// Where this document's crash-recovery file lives.
    pub(crate) fn recovery_target(&self) -> std::path::PathBuf {
        match &self.file_path {
            Some(p) => format::recovery_path_for(p),
            None => std::env::temp_dir().join("pictodesk-untitled.pdk.recovery"),
        }
    }

    /// Periodic autosave: when the document has unsaved changes,
    /// write a recovery copy (atomically) at most every 20 s.
    pub(crate) fn autosave_tick(&mut self) {
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
    pub(crate) fn clear_recovery(&mut self) {
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

    pub(crate) fn save_to(&mut self, path: &Path) -> bool {
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

    /// File > Save a Copy (Phase 6.5): write the document
    /// to a new path WITHOUT changing the working file,
    /// its path, or its dirty flag.
    pub(crate) fn save_copy_to(&mut self, path: &Path) {
        let result = if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            std::fs::write(path, format::serialize(&self.doc)).map_err(|e| e.to_string())
        } else if path.extension().map(|e| e == "psd").unwrap_or(false) {
            let (bytes, _report) = pictodesk_core::psd::save_psd(&self.doc);
            std::fs::write(path, bytes).map_err(|e| e.to_string())
        } else if ExportFormat::from_path(path).is_some() {
            io::save_file(path, &self.doc, 92).map_err(|e| e.to_string())
        } else {
            Err("unknown file extension".to_string())
        };
        self.status = match result {
            Ok(()) => format!("Saved a copy to {}", path.display()),
            Err(e) => format!("Save a Copy failed: {e}"),
        };
    }

    /// Edit > Define Brush (Phase 6.5): capture the
    /// selection's contents as a sampled brush tip.
    /// Coverage = darkness x alpha of the composite.
    pub(crate) fn define_brush_from_selection(&mut self) {
        let Some(sel) = self.sel.clone() else {
            self.status = "Define Brush needs a selection".into();
            return;
        };
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for y in 0..self.doc.height as i32 {
            for x in 0..self.doc.width as i32 {
                if sel.value(x, y) > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 < x0 {
            self.status = "Define Brush: the selection is empty".into();
            return;
        }
        let (bw, bh) = ((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32);
        self.ensure_render_cache();
        let img = self.cached_render.clone();
        let (dw, dh) = (self.doc.width as usize, self.doc.height as usize);
        let mut data = Vec::with_capacity((bw * bh) as usize);
        for y in 0..bh as i32 {
            for x in 0..bw as i32 {
                let i = (((y0 + y) as usize * dw) + (x0 + x) as usize) * 4;
                let (r, g, b, a) = (
                    img[i] as f32 / 255.0,
                    img[i + 1] as f32 / 255.0,
                    img[i + 2] as f32 / 255.0,
                    img[i + 3] as f32 / 255.0,
                );
                let luma = 0.299 * r + 0.587 * g + 0.114 * b;
                data.push((1.0 - luma) * a);
            }
        }
        let _ = dh;
        self.tip = BrushTip::with_mask(bw.max(bh) as f32, brush::BrushMask { w: bw, h: bh, data });
        self.tool = Tool::Brush;
        self.status = format!("Defined a {bw} x {bh} brush from the selection");
    }

    pub(crate) fn do_action(&mut self, action: Action) {
        let layer = self.doc.active_layer;
        match action {
            Action::None => {}
            Action::New => {
                let mut d = Dialog::new(DialogKind::NewDoc, &self.doc);
                d.text = "Untitled".into();
                self.dialog = Some(d);
            }
            Action::OpenPanel => {
                self.file_panel_purpose = 0;
                self.open_file_panel();
            }
            Action::CloseTab => {
                self.close_tab(self.active_tab);
            }
            Action::TextureFillPick => {
                self.file_panel_purpose = 2;
                self.status = "Choose the texture image".into();
                self.open_file_panel();
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
                    self.replace_doc_from_path(&p);
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
            Action::BlendingDialog => {
                self.open_blending_dialog();
            }
            Action::LiquifyDialog => {
                self.open_liquify_dialog();
            }
            Action::NewLayerSet => {
                let a = self.doc.active_layer;
                self.structural("New Layer Set", |d| {
                    let n = d.sets.len() + 1;
                    d.sets.push(pictodesk_core::document::LayerSet {
                        name: format!("Set {n}"),
                        opacity: 1.0,
                        mode: None,
                        visible: true,
                        color: 0,
                    });
                    let idx = d.sets.len() as u16;
                    d.layers[a].group = idx;
                });
                self.status = "New layer set (active layer assigned)".into();
            }
            Action::DeleteLayerSet => {
                let g = self.doc.layers[self.doc.active_layer].group;
                if g == 0 {
                    self.status = "The active layer is not in a set".into();
                } else {
                    self.structural("Delete Layer Set", |d| {
                        let idx = (g - 1) as usize;
                        if idx < d.sets.len() {
                            d.sets.remove(idx);
                        }
                        for l in &mut d.layers {
                            if l.group == g {
                                l.group = 0;
                            } else if l.group > g {
                                l.group -= 1;
                            }
                        }
                    });
                    self.status = "Layer set deleted (layers kept)".into();
                }
            }
            Action::AssignSet(g) => {
                let a = self.doc.active_layer;
                self.structural("Assign to Set", |d| {
                    d.layers[a].group = g;
                });
            }
            Action::ToggleClipped => {
                let a = self.doc.active_layer;
                if a == 0 {
                    self.status = "The bottom layer cannot be clipped".into();
                } else {
                    self.structural("Clipping Mask", |d| {
                        d.layers[a].clipped = !d.layers[a].clipped;
                    });
                    self.status = if self.doc.layers[a].clipped {
                        "Clipping mask: layer clipped to the one below".into()
                    } else {
                        "Clipping mask released".into()
                    };
                }
            }
            Action::LayerColor(c) => {
                let a = self.doc.active_layer;
                self.structural("Layer Color", |d| {
                    d.layers[a].color = c;
                });
            }
            Action::LockToggle(bit) => {
                let a = self.doc.active_layer;
                self.structural("Layer Lock", |d| {
                    d.layers[a].locks ^= bit;
                });
            }
            Action::RasterizeLayer => {
                let a = self.doc.active_layer;
                if self.doc.layers[a].shape.is_none() && self.doc.layers[a].fill.is_none() {
                    self.status = "The active layer is not a shape or fill layer".into();
                } else {
                    self.structural("Rasterize Layer", |d| {
                        let (w, h) = (d.width, d.height);
                        for y in 0..h as i32 {
                            for x in 0..w as i32 {
                                let p = d.content_pixel(a, x, y);
                                if p.3 > 0.0 {
                                    d.layers[a].set_pixel(
                                        x,
                                        y,
                                        pictodesk_core::pixel::Pixel16::from_f32s(
                                            p.0, p.1, p.2, p.3,
                                        ),
                                    );
                                }
                            }
                        }
                        d.layers[a].shape = None;
                        d.layers[a].fill = None;
                    });
                    self.status = "Layer rasterized".into();
                }
            }
            Action::DefineCustomShape => {
                self.define_custom_shape();
            }
            Action::PresetManagerDialog => {
                self.ensure_style_presets();
                let mut d = Dialog::new(DialogKind::PresetManager, &self.doc);
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::CreateDroplet => {
                let mut d = Dialog::new(DialogKind::Droplet, &self.doc);
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::ExportSlices => {
                self.export_slices();
            }
            Action::NewFillLayer(kind) => {
                let (fg, bg) = (self.fg, self.bg);
                self.structural("New Fill Layer", |d| {
                    let name = ["Solid Color", "Gradient", "Pattern"][kind as usize];
                    let li = d.add_layer(name);
                    d.layers[li].fill = Some(pictodesk_core::shapes::FillSpec {
                        kind,
                        color: fg,
                        to: bg,
                        ..Default::default()
                    });
                    d.active_layer = li;
                });
                self.status =
                    "Fill layer created (edit via Layer > Fill Content in v1: recreate)".into();
            }
            Action::PerspectiveCrop => {
                if self.doc.mode == pictodesk_core::document::ColorMode::Cmyk {
                    self.status =
                        "Perspective Crop is not available for CMYK documents in v1".into();
                } else {
                    let (w, h) = (self.doc.width as f32, self.doc.height as f32);
                    let (x0, y0, x1, y1) = match self.sel.as_ref().and_then(|s| s.bounds()) {
                        Some((bx, by, bw, bh)) => (
                            bx as f32,
                            by as f32,
                            (bx + bw as i32) as f32,
                            (by + bh as i32) as f32,
                        ),
                        None => (w * 0.1, h * 0.1, w * 0.9, h * 0.9),
                    };
                    self.persp_crop = Some([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
                    self.status =
                        "Perspective Crop: drag the corners, Enter applies, Esc cancels".into();
                }
            }
            Action::SlicePropsDialog => {
                if let Some(i) = self.slice_selected {
                    let sl = self.doc.slices[i].clone();
                    let mut d = Dialog::new(DialogKind::SliceProps, &self.doc);
                    d.slice_target = Some(i);
                    d.text = sl.name.clone();
                    d.text2 = sl.url.clone();
                    d.text3 = sl.alt.clone();
                    d.text_focus = true;
                    self.dialog = Some(d);
                } else {
                    self.status = "Select a slice first (Slice Select tool)".into();
                }
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
                d.cs_default = match self.default_space {
                    pictodesk_core::color::WorkingSpace::Srgb => 0,
                    pictodesk_core::color::WorkingSpace::AdobeRgb => 1,
                    pictodesk_core::color::WorkingSpace::ProPhoto => 2,
                };
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
                d.vals = vec![1.0, self.page_scale, self.print_pos.0, self.print_pos.1];
                d.opt_a = self.print_centered;
                d.opt_b = self.print_fit;
                d.fx_a = self.print_marks;
                d.fx_b = self.print_negative;
                d.pr_emulsion = self.print_emulsion;
                d.pr_bilinear = self.print_bilinear;
                d.print_profile = self.print_profile;
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
                    3 => ("Facet", RecordedStep::Facet, |d, l, s| {
                        filter::facet(d, l, s)
                    }),
                    4 => ("Blur", RecordedStep::GaussianBlur(0.8), |d, l, s| {
                        filter::gaussian_blur(d, l, s, 0.8)
                    }),
                    5 => ("Blur More", RecordedStep::GaussianBlur(2.5), |d, l, s| {
                        filter::gaussian_blur(d, l, s, 2.5)
                    }),
                    6 => ("Sharpen More", RecordedStep::SharpenMore, |d, l, s| {
                        filter::sharpen_more(d, l, s)
                    }),
                    _ => ("NTSC Colors", RecordedStep::NtscColors, |d, l, s| {
                        filter::ntsc_colors(d, l, s)
                    }),
                };
                self.filter_now(label, Some(step), f);
            }
            Action::ExtractDialog => {
                let mut d = Dialog::new(DialogKind::Extract, &self.doc);
                let n = (self.doc.width * self.doc.height) as usize;
                d.extract_hi = vec![0; n];
                d.extract_fill = vec![0; n];
                d.extract_touch = vec![0; n];
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
                "gradientmap" => self.open_dialog(DialogKind::GradientMap),
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
            Action::ModeLab => {
                self.structural("Lab Color", |d| {
                    pictodesk_core::mode::convert_mode(d, ColorMode::Lab)
                });
                self.channel_view = ChannelView::Composite;
            }
            Action::ModeMultichannel => {
                self.structural("Multichannel", |d| {
                    pictodesk_core::mode::convert_mode(d, ColorMode::Multichannel)
                });
                self.channel_view = ChannelView::Composite;
                self.doc.multi_active = None;
                self.status =
                    "Multichannel: pick a channel in the Channels panel to paint into it".into();
            }
            Action::BitmapDialog => self.open_dialog(DialogKind::BitmapConv),
            Action::IndexedDialog => self.open_dialog(DialogKind::IndexedConv),
            Action::DuotoneDialog => self.open_dialog(DialogKind::Duotone),
            Action::ColorTableDialog => {
                if self.doc.mode == ColorMode::Indexed {
                    self.open_dialog(DialogKind::ColorTable);
                } else {
                    self.status = "Color Table needs an Indexed Color image".into();
                }
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
            Action::ToggleOptionsBar => {
                self.show_options_bar = !self.show_options_bar;
                self.status = if self.show_options_bar {
                    "Options Bar shown".into()
                } else {
                    "Options Bar hidden".into()
                };
            }
            Action::ScreenMode(m) => {
                self.display_mode = m.min(2);
                if self.display_mode == 2 {
                    self.open_menu = None;
                    self.open_submenu = None;
                }
                self.status = match self.display_mode {
                    0 => "Screen mode: Standard".into(),
                    1 => "Screen mode: Full Screen with Menu Bar (F cycles)".into(),
                    _ => "Screen mode: Full Screen (F or Esc returns)".into(),
                };
            }
            Action::FadeDialog => {
                if self.fade_source.is_some() {
                    self.open_dialog(DialogKind::Fade);
                } else {
                    self.status = "Fade: run a filter or adjustment first".into();
                }
            }
            Action::StrokeDialog => self.open_dialog(DialogKind::StrokeSel),
            Action::RotateArbitraryDialog => self.open_dialog(DialogKind::RotateArbitrary),
            Action::FileInfoDialog => self.open_dialog(DialogKind::FileInfo),
            Action::PreferencesDialog => self.open_dialog(DialogKind::Preferences),
            Action::DefringeDialog => self.open_dialog(DialogKind::Defringe),
            Action::SaveCopyPanel => {
                let mut d = Dialog::new(DialogKind::SaveAs, &self.doc);
                d.save_mode = 1;
                d.text = self
                    .file_path
                    .as_ref()
                    .map(|p| {
                        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                        format!("{stem} copy.pdk")
                    })
                    .unwrap_or_else(|| "untitled copy.pdk".into());
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::ExportAi => {
                self.open_save_dialog(5, "paths.ai");
            }
            Action::DuplicateImage => {
                let dup = self.doc.clone();
                let name = format!("{} copy", self.doc_name);
                self.new_tab_with_doc(dup, None, name);
                self.status = "Duplicated into a new tab".into();
            }
            Action::RemoveBlackMatte => {
                self.filter_now("Remove Black Matte", None, |d, l, _| {
                    adjust::remove_matte(d, l, false)
                });
            }
            Action::RemoveWhiteMatte => {
                self.filter_now("Remove White Matte", None, |d, l, _| {
                    adjust::remove_matte(d, l, true)
                });
            }
            Action::DefineBrush => {
                self.define_brush_from_selection();
            }
            Action::LastFilter => {
                if let Some(step) = self.last_filter_step.clone() {
                    let sel = self.sel.clone();
                    let label = format!("Last Filter: {}", step.label());
                    let steps = vec![step];
                    self.structural(&label, |doc| {
                        apply_steps(doc, &steps, sel.as_ref());
                    });
                } else {
                    self.status = "No filter has run yet this session".into();
                }
            }
            Action::SelectiveColorDialog => self.open_dialog(DialogKind::SelectiveColor),
            Action::ReplaceColorDialog => self.open_dialog(DialogKind::ReplaceColor),
            Action::VariationsDialog => self.open_variations(),
            Action::ApplyImageDialog => {
                if self.doc.mode == ColorMode::Multichannel {
                    self.status = "Apply Image is not available in Multichannel mode".into();
                } else {
                    self.open_dialog(DialogKind::ApplyImage);
                }
            }
            Action::CalculationsDialog => self.open_dialog(DialogKind::Calculations),
            Action::LoadSelectionDialog => {
                if self.doc.alphas.is_empty() {
                    self.status = "No saved channels to load".into();
                } else {
                    self.open_dialog(DialogKind::LoadSel);
                }
            }
            Action::ChannelOptionsDialog => {
                if self.doc.alphas.is_empty() {
                    self.status = "No channels to edit".into();
                } else {
                    self.open_dialog(DialogKind::ChannelOptions);
                }
            }
            Action::MergeChannelsDialog => {
                if self.split_docs.is_empty() {
                    self.status = "Split Channels first, then merge".into();
                } else {
                    self.open_dialog(DialogKind::MergeChannels);
                }
            }
            Action::SplitChannels => {
                let parts = pictodesk_core::channels::split_channels(&self.doc);
                let (dir, stem) = match &self.file_path {
                    Some(p) => (
                        p.parent()
                            .map(|d| d.to_path_buf())
                            .unwrap_or_else(|| std::path::PathBuf::from(".")),
                        p.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "untitled".into()),
                    ),
                    None => (std::path::PathBuf::from("."), "untitled".into()),
                };
                let mut written = 0;
                for (name, doc) in &parts {
                    let safe: String = name
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '_' })
                        .collect();
                    let path = dir.join(format!("{stem}_{safe}.pdk"));
                    if pictodesk_core::format::save(doc, &path).is_ok() {
                        written += 1;
                    }
                }
                self.status = format!(
                    "Split into {} channels ({} .pdk files written; Merge Channels recombines them)",
                    parts.len(),
                    written
                );
                self.split_docs = parts;
            }
            Action::MergeGroup => {
                let g = self.doc.layers[self.doc.active_layer].group;
                if g > 0 {
                    self.structural("Merge Group", |d| {
                        pictodesk_core::channels::merge_set(d, g);
                    });
                    self.status = "Layer group merged".into();
                }
            }
            Action::PurgeHistories => {
                self.hist.purge();
                self.hist_src_cache = None;
                self.status = "History purged".into();
            }
            Action::PurgeClipboard => {
                CLIPBOARD.with(|c| *c.borrow_mut() = None);
                self.status = "Clipboard purged".into();
            }
            Action::PurgeAll => {
                self.hist.purge();
                self.hist_src_cache = None;
                CLIPBOARD.with(|c| *c.borrow_mut() = None);
                self.status = "Histories and clipboard purged".into();
            }
            Action::QuickMaskToggle => self.toggle_quick_mask(),
            Action::NewLayer => {
                let mut d = Dialog::new(DialogKind::NewLayer, &self.doc);
                d.text = format!("Layer {}", self.doc.layers.len() + 1);
                d.text_focus = true;
                d.fx_b = false; // Neutral Fill starts off
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
                        Some(pictodesk_core::adjust::Adjustment::GradientMap { .. }) => {
                            "gradientmap"
                        }
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
                self.save_actions_file();
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
                self.save_actions_file();
            }
            Action::MacroPlay => {
                self.play_action(self.active_action);
            }
            Action::MacroDelete => {
                if !self.actions.is_empty() {
                    self.actions.remove(self.active_action);
                    self.save_actions_file();
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
            Action::AutoLevelsCmd => {
                self.apply_filter_now("Auto Levels", RecordedStep::AutoLevels, |d, l, s| {
                    adjust::auto_levels(d, l, s)
                });
            }
            Action::AutoContrastCmd => {
                self.apply_filter_now("Auto Contrast", RecordedStep::AutoContrast, |d, l, s| {
                    adjust::auto_contrast(d, l, s)
                });
            }
            Action::AutoColorCmd => {
                self.apply_filter_now("Auto Color", RecordedStep::AutoColor, |d, l, s| {
                    adjust::auto_color(d, l, s)
                });
            }
            Action::PatternMakerDialog => {
                self.open_pattern_maker();
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
            Action::SpellCheckDialog => {
                self.open_spell_check();
            }
            Action::FindReplaceDialog => {
                let mut d = Dialog::new(DialogKind::FindReplace, &self.doc);
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::ToggleToolPresets => {
                self.show_tool_presets = !self.show_tool_presets;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            Action::FileBrowserDialog => {
                self.open_file_browser();
            }
            Action::SaveWorkspaceDialog => {
                let mut d = Dialog::new(DialogKind::SaveWorkspace, &self.doc);
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Action::ApplyWorkspace(i) => {
                self.apply_workspace(i);
            }
            Action::DeleteWorkspace(i) => {
                if i < self.workspaces.len() {
                    let (name, _) = self.workspaces.remove(i);
                    self.save_workspaces_file();
                    self.status = format!("Workspace '{name}' deleted");
                }
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
            Action::ZoomPrintSize => {
                // Print Size: 72 dpi document at a 96 dpi screen.
                self.viewport.scale = 96.0 / 72.0;
                self.status = "Print Size view".into();
            }
            Action::ToggleLockGuides => {
                self.guides_locked = !self.guides_locked;
                self.status = if self.guides_locked {
                    "Guides locked".into()
                } else {
                    "Guides unlocked".into()
                };
            }
            Action::ToggleShowPath => {
                self.show_path = !self.show_path;
                self.status = if self.show_path {
                    "Path shown".into()
                } else {
                    "Path hidden".into()
                };
            }
            Action::ToggleGamut => {
                self.gamut_warning = !self.gamut_warning;
                self.cached_render = Vec::new();
                self.status = if self.gamut_warning {
                    "Gamut Warning on: out-of-CMYK-gamut pixels show gray".into()
                } else {
                    "Gamut Warning off".into()
                };
            }
            Action::TogglePathEdges => {
                // One item flips whichever the caller encoded in
                // fx-free fashion: path visibility is toggled by
                // its own menu item below; this action toggles the
                // selection edges.
                self.show_edges = !self.show_edges;
                self.status = if self.show_edges {
                    "Selection edges shown".into()
                } else {
                    "Selection edges hidden".into()
                };
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

    pub(crate) fn clear_selection(&mut self) {
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

    /// Quick Mask (Phase 6.5): enter with the current
    /// selection as the mask (paint white to add, black to
    /// remove, on a temporary alpha channel), exit to turn
    /// the painted mask back into the selection.
    pub(crate) fn toggle_quick_mask(&mut self) {
        if self.quick_mask {
            if let Some(idx) = self.doc.alphas.iter().position(|c| c.name == "Quick Mask") {
                if let Some(sel) = self.doc.channel_as_selection(idx) {
                    self.sel = if sel.is_empty() { None } else { Some(sel) };
                }
                self.doc.alphas.remove(idx);
            }
            self.quick_mask = false;
            self.channel_view = ChannelView::Composite;
            self.paint_target = PaintTarget::Image;
            self.status = "Quick Mask off — selection updated from the mask".into();
            self.touch();
        } else {
            let mut ch = pictodesk_core::document::AlphaChannel::new(
                "Quick Mask",
                self.doc.width,
                self.doc.height,
            );
            if let Some(sel) = &self.sel {
                ch.data.copy_from_slice(sel.raw_mask());
            }
            self.doc.alphas.push(ch);
            let idx = self.doc.alphas.len() - 1;
            self.quick_mask = true;
            self.channel_view = ChannelView::Alpha(idx);
            self.paint_target = PaintTarget::Alpha(idx);
            self.show_cp = true;
            self.cp_tab = 1;
            self.status =
                "Quick Mask: paint white to add to the selection, black to remove; Q exits".into();
            self.touch();
        }
    }

    /// Replace the current document wholesale (Automate results,
    /// New): fresh history, no file, fitted view.
    pub(crate) fn swap_in_doc(&mut self, doc: Document, status: &str) {
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
    pub(crate) fn image_files_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
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
    pub(crate) fn run_contact_sheet(&mut self, folder: &str, cols: usize, thumb: u32) {
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
    pub(crate) fn run_picture_package(
        &mut self,
        page_w: u32,
        page_h: u32,
        layout: u8,
        layered: bool,
        labels: bool,
    ) {
        // Sources (7.0): the picked files, or the current
        // document when none were picked. Cells cycle through
        // them, so a page can hold different images.
        let mut sources: Vec<(Vec<u8>, u32, u32, String)> = Vec::new();
        for p in &self.pkg_sources {
            if let Ok(bytes) = std::fs::read(p) {
                if let Ok(doc) = pictodesk_core::io::import_image(&bytes) {
                    let label = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    sources.push((doc.render_full_rgba8(), doc.width, doc.height, label));
                }
            }
        }
        if sources.is_empty() {
            sources.push((
                self.doc.render_full_rgba8(),
                self.doc.width,
                self.doc.height,
                "Document".to_string(),
            ));
        }
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
        let mut cell_no = 0usize;
        for r in 0..rows {
            for c in 0..cols {
                let (src, iw, ih, label) = &sources[cell_no % sources.len()];
                let (iw, ih) = (*iw, *ih);
                cell_no += 1;
                let scale = ((cell_w - 8) as f32 / iw as f32).min((cell_h - 8) as f32 / ih as f32);
                let (tw, th) = (
                    ((iw as f32 * scale) as u32).max(1),
                    ((ih as f32 * scale) as u32).max(1),
                );
                let ox = margin + c * cell_w + (cell_w - tw) / 2;
                let oy = margin + r * cell_h + (cell_h - th) / 2;
                // Layered output: each image gets its own
                // layer; flattened: everything on the base.
                let target = if layered {
                    let l = pictodesk_core::layer::Layer::new(label.clone());
                    page.layers.push(l);
                    page.layers.len() - 1
                } else {
                    0
                };
                for yy in 0..th {
                    for xx in 0..tw {
                        let sx = (xx as f32 / scale) as u32;
                        let sy = (yy as f32 / scale) as u32;
                        let idx = ((sy * iw + sx) * 4) as usize;
                        if idx + 3 < src.len() {
                            page.layers[target].set_pixel(
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
                if labels {
                    let black = Pixel16::from_rgba8(0, 0, 0, 255);
                    stamp_label(
                        &mut page.layers[target],
                        ox as i32,
                        (oy + th + 2) as i32,
                        label,
                        black,
                    );
                }
            }
        }
        self.swap_in_doc(page, "Picture Package page");
    }

    /// File > Automate > Web Photo Gallery: a folder of images
    /// becomes a static gallery (index.html + full images +
    /// thumbnails) in a `gallery` subfolder.
    pub(crate) fn run_web_gallery(&mut self, folder: &str, thumb: u32) {
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
    pub(crate) fn filter_now(
        &mut self,
        label: &str,
        step: Option<RecordedStep>,
        f: impl Fn(&mut Document, usize, Option<&Selection>),
    ) {
        let layer = self.doc.active_layer;
        let tiles = self.sel_tiles();
        let sel = self.sel.clone();
        let cmd = pictodesk_core::history::run_tile_edit_captured(
            &mut self.doc,
            &mut self.hist,
            label,
            layer,
            &tiles,
            |d| f(d, layer, sel.as_ref()),
        );
        self.fade_source = Some(FadeSource {
            label: label.to_string(),
            changes: cmd.changes.clone(),
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
    pub(crate) fn export_psd(&mut self) {
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
    pub(crate) fn export_dcs(&mut self) {
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
    pub(crate) fn print_page_rgba(&self) -> (Vec<u8>, u32, u32) {
        let (pw, ph) = if self.page_landscape {
            (self.page_h as u32, self.page_w as u32)
        } else {
            (self.page_w as u32, self.page_h as u32)
        };
        let mut page = vec![255u8; (pw * ph * 4) as usize];
        let mut img = self.doc.render_full_rgba8();
        // "Color: printer" converts the page to sRGB first;
        // "Color: document" prints the working space as-is.
        if self.print_profile == 1
            && self.doc.working_space != pictodesk_core::color::WorkingSpace::Srgb
        {
            let src = pictodesk_core::icc::IccProfile::for_working_space(self.doc.working_space);
            let dst = pictodesk_core::icc::IccProfile::srgb();
            let mut converted = vec![0u8; img.len()];
            if src.convert_rgba8(&dst, &img, &mut converted).is_ok() {
                img = converted;
            }
        }
        let (iw, ih) = (self.doc.width as f32, self.doc.height as f32);
        let scale = if self.print_fit {
            (pw as f32 / iw).min(ph as f32 / ih)
        } else {
            self.page_scale / 100.0
        };
        let (tw, th) = (iw * scale, ih * scale);
        let (ox, oy) = if self.print_centered {
            (
                ((pw as f32 - tw) / 2.0).max(0.0),
                ((ph as f32 - th) / 2.0).max(0.0),
            )
        } else {
            (self.print_pos.0.max(0.0), self.print_pos.1.max(0.0))
        };
        let sample = |fx: f32, fy: f32| -> (u8, u8, u8, u8) {
            let (max_x, max_y) = (iw as i32 - 1, ih as i32 - 1);
            if self.print_bilinear {
                let x0 = (fx.floor() as i32).clamp(0, max_x);
                let y0 = (fy.floor() as i32).clamp(0, max_y);
                let x1 = (x0 + 1).min(max_x);
                let y1 = (y0 + 1).min(max_y);
                let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
                let mut out = (0u8, 0u8, 0u8, 0u8);
                let px = |xx: i32, yy: i32, c: usize| -> f32 {
                    img[((yy * iw as i32 + xx) * 4) as usize + c] as f32
                };
                let mix = |c: usize| -> u8 {
                    let top = px(x0, y0, c) * (1.0 - tx) + px(x1, y0, c) * tx;
                    let bot = px(x0, y1, c) * (1.0 - tx) + px(x1, y1, c) * tx;
                    (top * (1.0 - ty) + bot * ty) as u8
                };
                out.0 = mix(0);
                out.1 = mix(1);
                out.2 = mix(2);
                out.3 = mix(3);
                out
            } else {
                let sx = (fx as i32).clamp(0, max_x);
                let sy = (fy as i32).clamp(0, max_y);
                let si = ((sy * iw as i32 + sx) * 4) as usize;
                (img[si], img[si + 1], img[si + 2], img[si + 3])
            }
        };
        for yy in 0..(th as u32) {
            for xx in 0..(tw as u32) {
                let (dx, dy) = ((ox as u32 + xx), (oy as u32 + yy));
                if dx >= pw || dy >= ph {
                    continue;
                }
                // Emulsion down mirrors the image on the page.
                let src_x = if self.print_emulsion {
                    tw - 1.0 - xx as f32
                } else {
                    xx as f32
                };
                let (r, g, b, a) = sample(src_x / scale, yy as f32 / scale);
                let di = ((dy * pw + dx) * 4) as usize;
                let af = a as f32 / 255.0;
                for (c, v) in [r, g, b].iter().enumerate() {
                    let mut val = (*v as f32 * af + page[di + c] as f32 * (1.0 - af)) as u8;
                    if self.print_negative {
                        val = 255 - val;
                    }
                    page[di + c] = val;
                }
                page[di + 3] = 255;
            }
        }
        // ---- Output marks (Print Options) ----
        let set_px = |page: &mut Vec<u8>, x: i32, y: i32, v: u8| {
            if x >= 0 && y >= 0 && (x as u32) < pw && (y as u32) < ph {
                let i = ((y as u32 * pw + x as u32) * 4) as usize;
                page[i] = v;
                page[i + 1] = v;
                page[i + 2] = v;
                page[i + 3] = 255;
            }
        };
        let (ix0, iy0) = (ox as i32, oy as i32);
        let (ix1, iy1) = (ix0 + tw as i32, iy0 + th as i32);
        if self.print_marks & 1 != 0 {
            // Crop marks: short lines offset from each corner.
            for (cx, cy, sx, sy) in [
                (ix0, iy0, -1, -1),
                (ix1, iy0, 1, -1),
                (ix0, iy1, -1, 1),
                (ix1, iy1, 1, 1),
            ] {
                for t in 6..24 {
                    set_px(&mut page, cx + sx * t, cy, 0);
                    set_px(&mut page, cx, cy + sy * t, 0);
                }
            }
        }
        if self.print_marks & 2 != 0 {
            // Registration marks: circle + cross at mid-edges.
            let centers = [
                ((ix0 + ix1) / 2, iy0 - 16),
                ((ix0 + ix1) / 2, iy1 + 16),
                (ix0 - 16, (iy0 + iy1) / 2),
                (ix1 + 16, (iy0 + iy1) / 2),
            ];
            for (cx, cy) in centers {
                for t in -8..=8 {
                    set_px(&mut page, cx + t, cy, 0);
                    set_px(&mut page, cx, cy + t, 0);
                }
                for dy in -6..=6 {
                    for dx in -6..=6 {
                        let d2 = dx * dx + dy * dy;
                        if (30..=38).contains(&d2) {
                            set_px(&mut page, cx + dx, cy + dy, 0);
                        }
                    }
                }
            }
        }
        if self.print_marks & 4 != 0 && pw > 320 {
            // Calibration bars along the bottom margin.
            for i in 0..11 {
                let v = (255 - i * 25) as u8;
                for yy in 0..8 {
                    for xx in 0..20 {
                        set_px(&mut page, 36 + i * 22 + xx, ph as i32 - 24 + yy, v);
                    }
                }
            }
            let patches: [(u8, u8, u8); 7] = [
                (0, 255, 255),
                (255, 0, 255),
                (255, 255, 0),
                (0, 0, 0),
                (255, 0, 0),
                (0, 255, 0),
                (0, 0, 255),
            ];
            for (i, (r, g, b)) in patches.iter().enumerate() {
                for yy in 0..8i32 {
                    for xx in 0..20i32 {
                        let x = 36 + i as i32 * 22 + xx;
                        let y = ph as i32 - 14 + yy;
                        if x >= 0 && y >= 0 && (x as u32) < pw && (y as u32) < ph {
                            let idx = ((y as u32 * pw + x as u32) * 4) as usize;
                            page[idx] = *r;
                            page[idx + 1] = *g;
                            page[idx + 2] = *b;
                            page[idx + 3] = 255;
                        }
                    }
                }
            }
        }
        if self.print_marks & 8 != 0 {
            // Label: the file name under the image, in the
            // 8x8 UI font.
            let label = self
                .file_path
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "Untitled".into());
            let ly = iy1 + 10;
            for (ci, ch) in label.chars().enumerate() {
                if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                    for (row, &bits) in glyph.iter().enumerate() {
                        for col in 0..8 {
                            if bits & (1 << col) != 0 {
                                set_px(&mut page, ix0 + (ci * 8 + col) as i32, ly + row as i32, 0);
                            }
                        }
                    }
                }
            }
        }
        (page, pw, ph)
    }

    /// Print: build a one-page PDF (JPEG image XObject) in the
    /// temp dir and hand it to the platform print spooler.
    pub(crate) fn do_print(&mut self, copies: u32) {
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

    /// File > Export > Slices: write each slice as its own PNG
    /// next to the document (or in the home folder when unsaved).
    pub(crate) fn export_slices(&mut self) {
        if self.doc.slices.is_empty() {
            self.status = "No slices to export (Slice tool)".into();
            return;
        }
        let base = self
            .file_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let stem = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "pictodesk".to_string());
        let rgba = self
            .doc
            .render_region_rgba8(0, 0, self.doc.width, self.doc.height);
        let dw = self.doc.width as usize;
        let mut written = 0usize;
        for sl in &self.doc.slices {
            let (x0, y0) = (sl.x.max(0), sl.y.max(0));
            let (x1, y1) = (
                (sl.x + sl.w as i32).min(self.doc.width as i32),
                (sl.y + sl.h as i32).min(self.doc.height as i32),
            );
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let (sw, sh) = ((x1 - x0) as usize, (y1 - y0) as usize);
            let mut out = vec![0u8; sw * sh * 4];
            for yy in 0..sh {
                for xx in 0..sw {
                    let src = ((y0 as usize + yy) * dw + (x0 as usize + xx)) * 4;
                    let dst = (yy * sw + xx) * 4;
                    out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
            let path = base.join(format!("{stem}-{}.png", sl.name));
            let mut sdoc = Document::new(sw as u32, sh as u32);
            for yy in 0..sh as i32 {
                for xx in 0..sw as i32 {
                    let i = ((yy as usize) * sw + xx as usize) * 4;
                    sdoc.layers[0].set_pixel(
                        xx,
                        yy,
                        Pixel16::from_rgba8(out[i], out[i + 1], out[i + 2], out[i + 3]),
                    );
                }
            }
            if let Ok(png) =
                pictodesk_core::io::export_image(&sdoc, pictodesk_core::io::ExportFormat::Png, 90)
            {
                if std::fs::write(&path, png).is_ok() {
                    written += 1;
                }
            }
        }
        self.status = format!("Exported {written} slice PNG(s) next to the document");
    }

    /// Rows shown by the Preset Manager (tab 0: session style
    /// presets; tab 1: built-in + document custom shapes).
    pub(crate) fn preset_rows(&self) -> Vec<String> {
        if self.preset_tab == 0 {
            self.style_presets.iter().map(|(n, _)| n.clone()).collect()
        } else if self.preset_tab == 2 {
            self.tool_presets.iter().map(|p| p.name.clone()).collect()
        } else {
            let mut v: Vec<String> = ["Star", "Arrow", "Heart"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            v.extend(self.doc.custom_shapes.iter().map(|c| c.name.clone()));
            v
        }
    }

    /// File > Automate > Create Droplet: write an executable
    /// shell script + a steps file next to the document (or in
    /// the working folder). Dropping image files on the script
    /// runs this app with --droplet-run, which applies the
    /// current action's steps to each file and writes PNGs.
    pub(crate) fn create_droplet(&mut self, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.status = "Create Droplet: give the droplet a name".into();
            return;
        }
        let action = match self.actions.get(self.active_action) {
            Some(a) if !a.steps.is_empty() => a.clone(),
            _ => {
                self.status = "Create Droplet: record or pick an action with steps first".into();
                return;
            }
        };
        let base = self
            .file_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let script_path = base.join(format!("{name}.sh"));
        let steps_path = base.join(format!("{name}.steps"));
        let mut lines = String::new();
        let mut skipped = 0usize;
        for step in &action.steps {
            match step_to_line(step) {
                Some(l) => lines.push_str(&l),
                None => skipped += 1,
            }
            lines.push('\n');
        }
        if std::fs::write(&steps_path, lines).is_err() {
            self.status = "Create Droplet: could not write the steps file".into();
            return;
        }
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "pictodesk-app".into());
        let script = format!(
            "#!/bin/sh\n# PictoDesk droplet: applies the action '{}' to each file given.\nexec \"{}\" --droplet-run \"{}\" \"$@\"\n",
            action.name,
            exe,
            steps_path.to_string_lossy()
        );
        if std::fs::write(&script_path, script).is_err() {
            self.status = "Create Droplet: could not write the script".into();
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
        }
        self.status = format!(
            "Droplet '{}' created ({} steps{}{})",
            name,
            action.steps.len() - skipped,
            if skipped > 0 {
                format!(", {skipped} step(s) not droplet-safe and skipped")
            } else {
                String::new()
            },
            ""
        );
    }

    /// Seed the session style presets (built-ins) once.
    pub(crate) fn ensure_style_presets(&mut self) {
        use pictodesk_core::effects::Effects;
        use pictodesk_core::pixel::Pixel16;
        if !self.style_presets.is_empty() {
            return;
        }
        let mk = |name: &str, f: &dyn Fn(&mut Effects)| {
            let mut fx = Effects::default();
            f(&mut fx);
            (name.to_string(), fx)
        };
        self.style_presets = vec![
            mk("Soft Drop Shadow", &|fx: &mut Effects| {
                fx.drop_shadow.enabled = true;
                fx.drop_shadow.distance = 6;
                fx.drop_shadow.blur = 8;
                fx.drop_shadow.opacity = 0.6;
            }),
            mk("Button Bevel", &|fx: &mut Effects| {
                fx.bevel.enabled = true;
                fx.bevel.depth = 150;
            }),
            mk("Gold Gradient", &|fx: &mut Effects| {
                fx.gradient_overlay.enabled = true;
                fx.gradient_overlay.from = Pixel16::from_rgba8(120, 70, 0, 255);
                fx.gradient_overlay.to = Pixel16::from_rgba8(255, 210, 90, 255);
            }),
            mk("Red Stroke", &|fx: &mut Effects| {
                fx.stroke.enabled = true;
                fx.stroke.size = 3;
                fx.stroke.color = Pixel16::from_rgba8(220, 0, 0, 255);
            }),
            mk("Satin Sheen", &|fx: &mut Effects| {
                fx.satin.enabled = true;
            }),
            mk("Neon Glow", &|fx: &mut Effects| {
                fx.outer_glow.enabled = true;
                fx.outer_glow.color = Pixel16::from_rgba8(80, 255, 120, 255);
                fx.outer_glow.blur = 12;
            }),
        ];
    }

    /// Persist General preferences to ~/.pictodesk/prefs.txt.
    pub(crate) fn save_prefs_file(&self) {
        let _ = std::fs::create_dir_all(pictodesk_dir());
        let space = match self.default_space {
            pictodesk_core::color::WorkingSpace::Srgb => "srgb",
            pictodesk_core::color::WorkingSpace::AdobeRgb => "adobe",
            pictodesk_core::color::WorkingSpace::ProPhoto => "prophoto",
        };
        let out = format!(
            "hist_limit={}\nauto_snapshot={}\noptions_bar={}\ntooltips={}\nzoom_scroll={}\ndefault_space={}\n",
            self.hist.limit().unwrap_or(20),
            self.hist_auto_snapshot as u8,
            self.show_options_bar as u8,
            self.show_tooltips as u8,
            self.zoom_with_scroll as u8,
            space,
        );
        let _ = std::fs::write(pictodesk_dir().join("prefs.txt"), out);
    }

    /// Load saved preferences (defaults when the file is missing).
    pub(crate) fn load_prefs_file(&mut self) {
        let Ok(text) = std::fs::read_to_string(pictodesk_dir().join("prefs.txt")) else {
            return;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "hist_limit" => {
                    if let Ok(n) = v.trim().parse::<usize>() {
                        self.hist.set_limit(Some(n.max(1)));
                    }
                }
                "auto_snapshot" => self.hist_auto_snapshot = v.trim() == "1",
                "options_bar" => self.show_options_bar = v.trim() == "1",
                "tooltips" => self.show_tooltips = v.trim() == "1",
                "zoom_scroll" => self.zoom_with_scroll = v.trim() == "1",
                "default_space" => {
                    self.default_space = match v.trim() {
                        "adobe" => pictodesk_core::color::WorkingSpace::AdobeRgb,
                        "prophoto" => pictodesk_core::color::WorkingSpace::ProPhoto,
                        _ => pictodesk_core::color::WorkingSpace::Srgb,
                    };
                }
                _ => {}
            }
        }
    }

    /// Persist the shortcut map to ~/.pictodesk/shortcuts.txt.
    pub(crate) fn save_shortcuts_file(&self) {
        let _ = std::fs::create_dir_all(pictodesk_dir());
        let mut out = String::new();
        for d in &self.shortcuts {
            out.push_str(&format!("{}={}\n", d.id, d.combo));
        }
        let _ = std::fs::write(pictodesk_dir().join("shortcuts.txt"), out);
    }

    /// Run the command bound to a canonical combo ("primary+z",
    /// "b"), or capture it when the Preferences Shortcuts page
    /// is waiting for a key.
    pub(crate) fn dispatch_combo(&mut self, combo: &str) {
        if let Some(row) = self.prefs_capture {
            self.capture_shortcut(row, combo);
            return;
        }
        let Some(def) = self.shortcuts.iter().find(|d| d.combo == combo).cloned() else {
            return;
        };
        self.run_shortcut(&def.id);
    }

    pub(crate) fn run_shortcut(&mut self, id: &str) {
        if let Some(key) = id.strip_prefix("tool:") {
            // Cycle within the key's tool group (classic behavior).
            let k = key.chars().next().unwrap_or(' ');
            let group: Vec<Tool> = TOOLS.iter().copied().filter(|t| t.key() == k).collect();
            if !group.is_empty() {
                if let Some(pos) = group.iter().position(|t| *t == self.tool) {
                    self.tool = group[(pos + 1) % group.len()];
                } else {
                    self.tool = group[0];
                }
                self.status = format!("Tool: {}", self.tool.name());
            }
            return;
        }
        match id {
            "undo" => self.do_action(Action::Undo),
            "redo" => self.do_action(Action::Redo),
            "cut" => self.do_action(Action::Cut),
            "copy" => self.do_action(Action::Copy),
            "paste" => self.do_action(Action::Paste),
            "copy-merged" => self.do_action(Action::CopyMerged),
            "paste-into" => self.do_action(Action::PasteInto),
            "save" => self.do_action(Action::Save),
            "open" => self.do_action(Action::OpenPanel),
            "new" => self.do_action(Action::New),
            "close-tab" => self.do_action(Action::CloseTab),
            "new-layer" => self.do_action(Action::NewLayer),
            "merge-down" => self.do_action(Action::MergeDown),
            "free-transform" => self.do_action(Action::FreeTransform),
            "bring-forward" => self.do_action(Action::BringForward),
            "send-backward" => self.do_action(Action::SendBackward),
            "select-all" => self.do_action(Action::SelectAll),
            "select-none" => self.do_action(Action::SelectNone),
            "invert" => self.do_action(Action::AdjustDialog("invert")),
            "zoom-in" => self.do_action(Action::ZoomIn),
            "zoom-out" => self.do_action(Action::ZoomOut),
            "zoom-fit" => self.do_action(Action::ZoomFit),
            "zoom-100" => self.do_action(Action::Zoom100),
            "next-tab" => self.next_tab(),
            "prev-tab" => self.prev_tab(),
            "screen-mode" => {
                let next = (self.display_mode + 1) % 3;
                self.do_action(Action::ScreenMode(next));
            }
            "quick-mask" => self.do_action(Action::QuickMaskToggle),
            "swap-colors" => std::mem::swap(&mut self.fg, &mut self.bg),
            "default-colors" => {
                self.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                self.bg = Pixel16::from_rgba8(255, 255, 255, 255);
            }
            "brush-smaller" => {
                let s = (self.tip.size / 1.25).max(1.0);
                self.tip = BrushTip::new(s, self.tip.hardness);
            }
            "brush-larger" => {
                let s = (self.tip.size * 1.25).min(500.0);
                self.tip = BrushTip::new(s, self.tip.hardness);
            }
            _ => {}
        }
    }

    /// A key arrived while the Preferences Shortcuts page is
    /// waiting for one: assign it, or ask again when another
    /// command already owns it.
    pub(crate) fn capture_shortcut(&mut self, row: usize, combo: &str) {
        if combo == "escape" {
            self.prefs_capture = None;
            self.prefs_confirm = None;
            return;
        }
        if combo == "backspace" {
            if let Some(d) = self.shortcuts.get_mut(row) {
                d.combo = String::new();
            }
            self.prefs_capture = None;
            self.prefs_confirm = None;
            self.save_shortcuts_file();
            return;
        }
        // A second press of a taken combo steals it.
        if let Some((r, c)) = &self.prefs_confirm {
            if *r == row && c == combo {
                let row_id = self.shortcuts[row].id.clone();
                if let Some(other) = self
                    .shortcuts
                    .iter_mut()
                    .find(|d| d.combo == combo && d.id != row_id)
                {
                    other.combo = String::new();
                }
                if let Some(d) = self.shortcuts.get_mut(row) {
                    d.combo = combo.to_string();
                }
                self.prefs_capture = None;
                self.prefs_confirm = None;
                self.save_shortcuts_file();
                return;
            }
        }
        let taken = self
            .shortcuts
            .iter()
            .enumerate()
            .find(|(i, d)| *i != row && d.combo == combo && !combo.is_empty());
        match taken {
            Some((_, other)) => {
                self.status = format!(
                    "{} is already assigned to {} — press it again to take it",
                    combo_display(combo),
                    other.label
                );
                self.prefs_confirm = Some((row, combo.to_string()));
            }
            None => {
                if let Some(d) = self.shortcuts.get_mut(row) {
                    d.combo = combo.to_string();
                }
                self.prefs_capture = None;
                self.prefs_confirm = None;
                self.save_shortcuts_file();
            }
        }
    }
}
