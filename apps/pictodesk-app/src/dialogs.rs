//! Modal dialogs: kinds, state, layout, drawing, and every dialog's OK path.
use super::*;
use crate::actions::{
    combo_display, dialog_candidate, paths_to_ai, space_name, step_to_line, write_abr, write_aco,
    Action, BrowserEntry, FadeSource, RecordedStep,
};
use crate::menus::OptAct;
use crate::prefs::{
    prefs_page_sliders, PrefBool, PrefCycle, PrefKind, PrefNum, PREF_PAGES, PREF_ROWS,
};
use crate::tools::{audio_play, audio_record_start, wav_encode, ChannelView, PaintTarget};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DialogKind {
    SpellCheck,
    FindReplace,
    SaveWorkspace,
    RenameItem,
    FileBrowser,
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
    // Phase 6.5 Batch 2 kinds: color-mode dialogs.
    BitmapConv,
    IndexedConv,
    Duotone,
    ColorTable,
    // Phase 6.5 Batch 4 kinds.
    SelectiveColor,
    ReplaceColor,
    Variations,
    PatternMaker,
    // Phase 6.5 Batch 3 kinds: channel dialogs.
    ApplyImage,
    Calculations,
    LoadSel,
    ChannelOptions,
    MergeChannels,
    Fade,
    StrokeSel,
    RotateArbitrary,
    FileInfo,
    Preferences,
    Defringe,
    ActionSetName,
    InsertStop,
    InsertMenuItem,
    ActionStop,
    ConfirmClose,
    FtNumeric,
    // Phase 6 kinds.
    Blending,
    Liquify,
    PresetManager,
    NoteEdit,
    SliceProps,
    GradientMap,
    Droplet,
}

/// Plain-English name for an Options Bar control, for tooltips.
pub(crate) fn opt_act_tooltip(act: OptAct) -> String {
    match act {
        OptAct::TipSizeDown | OptAct::TipSizeUp => "Brush size".into(),
        OptAct::TipHardDown | OptAct::TipHardUp => "Hardness".into(),
        OptAct::ToneExpDown | OptAct::ToneExpUp => "Exposure".into(),
        OptAct::TolDown | OptAct::TolUp => "Tolerance".into(),
        OptAct::StrokeDown | OptAct::StrokeUp => "Stroke width".into(),
        OptAct::StampSample => "Sample: current layer / all layers".into(),
        OptAct::HealSource => "Healing source: sampled / pattern".into(),
        OptAct::FlowDown | OptAct::FlowUp => "Flow".into(),
        OptAct::AirbrushToggle => "Airbrush: hold to build up".into(),
        OptAct::EraseHistory => "Erase to history".into(),
        OptAct::DropperSample => "Sample: current layer / all layers".into(),
        OptAct::GradShape => "Gradient shape".into(),
        OptAct::ArrowStart => "Arrowhead at start".into(),
        OptAct::ArrowEnd => "Arrowhead at end".into(),
    }
}

/// Names of the LegacyFilter gallery, by id.
pub(crate) fn fx_title(id: u8) -> &'static str {
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
        16 => "Extrude",
        17 => "Trace Contour",
        18 => "Color Halftone",
        19 => "Crystallize",
        20 => "Pointillize",
        21 => "Smart Blur",
        22 => "De-Interlace",
        23 => "Wave",
        24 => "Glass",
        25 => "Diffuse Glow",
        26 => "Displace",
        27 => "Lens Flare",
        28 => "Lighting Effects",
        29 => "Colored Pencil",
        30 => "Cutout",
        31 => "Dry Brush",
        32 => "Film Grain",
        33 => "Fresco",
        34 => "Neon Glow",
        35 => "Paint Daubs",
        36 => "Palette Knife",
        37 => "Plastic Wrap",
        38 => "Poster Edges",
        39 => "Rough Pastels",
        40 => "Smudge Stick",
        41 => "Sponge",
        42 => "Underpainting",
        43 => "Watercolor",
        44 => "Accented Edges",
        45 => "Angled Strokes",
        46 => "Crosshatch",
        47 => "Dark Strokes",
        48 => "Ink Outlines",
        49 => "Spatter",
        50 => "Sprayed Strokes",
        51 => "Sumi-e",
        52 => "Chalk & Charcoal",
        53 => "Charcoal",
        54 => "Chrome",
        55 => "Conte Crayon",
        56 => "Graphic Pen",
        57 => "Halftone Pattern",
        58 => "Note Paper",
        59 => "Photocopy",
        60 => "Plaster",
        61 => "Reticulation",
        62 => "Torn Edges",
        63 => "Water Paper",
        64 => "Craquelure",
        65 => "Grain",
        66 => "Mosaic Tiles",
        67 => "Stained Glass",
        68 => "Texturizer",
        _ => "Filter",
    }
}

impl DialogKind {
    pub(crate) fn title(&self) -> &'static str {
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
            DialogKind::BitmapConv => "Bitmap",
            DialogKind::IndexedConv => "Indexed Color",
            DialogKind::Duotone => "Duotone",
            DialogKind::ColorTable => "Color Table",
            DialogKind::SelectiveColor => "Selective Color",
            DialogKind::ReplaceColor => "Replace Color",
            DialogKind::Variations => "Variations",
            DialogKind::PatternMaker => "Pattern Maker",
            DialogKind::SaveWorkspace => "Save Workspace",
            DialogKind::SpellCheck => "Check Spelling",
            DialogKind::FindReplace => "Find and Replace",
            DialogKind::RenameItem => "Rename",
            DialogKind::FileBrowser => "File Browser",
            DialogKind::ApplyImage => "Apply Image",
            DialogKind::Calculations => "Calculations",
            DialogKind::LoadSel => "Load Selection",
            DialogKind::ChannelOptions => "Channel Options",
            DialogKind::MergeChannels => "Merge Channels",
            DialogKind::Fade => "Fade",
            DialogKind::StrokeSel => "Stroke",
            DialogKind::RotateArbitrary => "Rotate Canvas",
            DialogKind::FileInfo => "File Info",
            DialogKind::Preferences => "Preferences",
            DialogKind::Defringe => "Defringe",
            DialogKind::ColorSettings => "Color Settings",
            DialogKind::GlobalAngle => "Global Angle",
            DialogKind::LegacyFilter(id) => fx_title(*id),
            DialogKind::ActionSetName => "New Action Set",
            DialogKind::InsertStop => "Insert Stop",
            DialogKind::InsertMenuItem => "Insert Menu Item",
            DialogKind::ActionStop => "Action Paused",
            DialogKind::ConfirmClose => "Close without saving?",
            DialogKind::FtNumeric => "Free Transform",
            DialogKind::Blending => "Blending Options",
            DialogKind::Liquify => "Liquify",
            DialogKind::PresetManager => "Preset Manager",
            DialogKind::NoteEdit => "Note",
            DialogKind::SliceProps => "Slice Options",
            DialogKind::GradientMap => "Gradient Map",
            DialogKind::Droplet => "Create Droplet",
        }
    }
    /// Pixel dialogs preview live through a tile-snapshot command.
    pub(crate) fn is_pixel_dialog(&self) -> bool {
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
                | DialogKind::GradientMap
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
                | DialogKind::SelectiveColor
                | DialogKind::ReplaceColor
                | DialogKind::Fade
                | DialogKind::Transform3d
                | DialogKind::LegacyFilter(_)
        )
    }
}

pub(crate) struct Dialog {
    pub(crate) kind: DialogKind,
    /// Slider values by index (meaning depends on kind).
    pub(crate) vals: Vec<f32>,
    /// Color Balance active range: 0 shadows, 1 midtones, 2 highlights.
    pub(crate) cb_range: usize,
    /// Curves control points.
    pub(crate) curve_points: Vec<(u8, u8)>,
    pub(crate) curve_drag: Option<usize>,
    /// Canvas Size anchor.
    pub(crate) anchor: (i32, i32),
    /// Fill: use background color instead of foreground.
    pub(crate) fill_bg: bool,
    /// Text field contents (Save As name).
    pub(crate) text: String,
    pub(crate) text_focus: bool,
    /// Live-preview snapshot (pixel dialogs only).
    pub(crate) preview: Option<TileEditCommand>,
    pub(crate) slider_drag: Option<usize>,
    /// Photo Filter color (preset swatches set this).
    pub(crate) photo_color: (u8, u8, u8),
    /// When set, this dialog edits an adjustment layer's parameters
    /// live instead of previewing a destructive edit (Phase 4).
    pub(crate) adj_layer: Option<usize>,
    /// Document state before an adjustment-layer dialog opened, so
    /// Cancel can restore it and OK can record one undo step.
    pub(crate) adj_snapshot: Option<Document>,
    /// Modify Selection operation: 0 Expand, 1 Contract, 2 Smooth,
    /// 3 Border.
    pub(crate) modify_op: u8,
    /// Custom Filter 5x5 kernel (row-major), the cell being edited,
    /// and its scale / offset.
    pub(crate) kernel: [i32; 25],
    pub(crate) kernel_sel: usize,
    pub(crate) kernel_scale: i32,
    pub(crate) kernel_offset: i32,
    /// Generic toggle buttons (History Options, Color Settings...).
    pub(crate) opt_a: bool,
    pub(crate) opt_b: bool,
    /// Preferences extras: tooltips, scroll-wheel zoom.
    /// Preferences draft (prefs.rs): one entry per row of the
    /// matching kind in PREF_ROWS order. Toggles and cycles hold
    /// the whole dialog's values; sliders hold only the current
    /// page's (reloaded on page switch).
    pub(crate) pref_toggles: Vec<bool>,
    pub(crate) pref_cycles: Vec<u8>,
    pub(crate) pref_sliders: Vec<f32>,
    /// Hue/Saturation edit range: 0 Master, 1 Reds, 2 Yellows,
    /// 3 Greens, 4 Cyans, 5 Blues, 6 Magentas.
    pub(crate) hs_range: usize,
    /// Fill contents: paint with the history source document.
    pub(crate) fill_history: bool,
    /// Channel Mixer working copy + the output channel being edited.
    pub(crate) mixer: adjust::ChannelMixer,
    pub(crate) mix_out: usize,
    /// Layer Effects working copy + the selected effect page
    /// (0 shadow, 1 inner shadow, 2 outer glow, 3 inner glow, 4 bevel).
    pub(crate) fx: pictodesk_core::effects::Effects,
    pub(crate) fx_page: usize,
    /// Type dialog: the layer being edited (None = new layer) and
    /// the click point where a new layer's text starts.
    pub(crate) type_layer: Option<usize>,
    pub(crate) type_anchor: (i32, i32),
    /// Type dialog working copy of the spec + font list scroll.
    pub(crate) type_spec: Option<pictodesk_core::text::TextSpec>,
    /// Type dialog opened by a Type Mask tool: OK produces a
    /// selection instead of a layer.
    pub(crate) type_mask: bool,
    pub(crate) type_unit_k: f32,
    pub(crate) type_size_min: f32,
    pub(crate) type_size_max: f32,
    pub(crate) font_scroll: usize,
    /// Extract dialog masks (document-sized) + tool mode
    /// (0 highlight brush, 1 fill, 2 eraser) + preview flag.
    pub(crate) extract_hi: Vec<u8>,
    pub(crate) extract_fill: Vec<u8>,
    /// Extract 6.0 upgrades: Edge Touchup marks (paint back
    /// original pixels after the cut) + Smart Highlighting.
    pub(crate) extract_touch: Vec<u8>,
    pub(crate) extract_smart: bool,
    pub(crate) extract_mode: u8,
    pub(crate) extract_preview: bool,
    /// 3D Transform shape: 0 cube, 1 sphere, 2 cylinder.
    pub(crate) shape3d: u8,
    /// 3D Transform wireframe drag state: the last mouse
    /// position while dragging the wireframe to rotate it.
    pub(crate) wire_drag: Option<(i32, i32)>,
    /// Automate dialog choices.
    pub(crate) cond_from: u8,
    pub(crate) cond_to: u8,
    pub(crate) pkg_layout: u8,
    /// Save for Web format: 0 JPEG, 1 PNG, 2 GIF, 3 WebP; and the
    /// last encoded size in bytes (shown in the dialog).
    pub(crate) web_format: u8,
    pub(crate) web_size: usize,
    /// Weighted Optimization toggle (v0.6.1): JPEG quality is
    /// weighted by the first alpha channel between the Quality
    /// slider (vals[0]) and the Min quality slider (vals[1]).
    pub(crate) web_weighted: bool,
    /// Print profile choice: 0 = document working space, 1 = let
    /// the system driver manage color.
    pub(crate) print_profile: u8,
    /// LegacyFilter option state: ZigZag style / Wind method,
    /// and the Wind direction (true = from the left).
    pub(crate) fx_a: u8,
    pub(crate) fx_b: bool,
    /// Second option index (Texturizer light direction).
    pub(crate) fx_c: u8,
    /// Duotone dialog work-in-progress: (ink RGB, curve preset)
    /// per ink; fx_page selects the ink being edited.
    pub(crate) duo_inks: Vec<([u8; 3], u8)>,
    /// Color Table dialog: the selected palette entry.
    pub(crate) table_sel: usize,
    /// Channel-dialog state (Phase 6.5 Batch 3): source layer
    /// cycles (0 = composite, else layer index + 1), channel
    /// cycles (indices into `chan_cycle`), blend cycle, mask
    /// (0 = none, else alpha + 1), target (0 = whole image,
    /// else channel + 1), result and merge choices.
    pub(crate) src_layer: usize,
    pub(crate) src_chan: usize,
    pub(crate) src2_layer: usize,
    pub(crate) src2_chan: usize,
    pub(crate) blend_idx: usize,
    pub(crate) mask_sel: usize,
    pub(crate) target_sel: usize,
    pub(crate) result_sel: usize,
    pub(crate) merge_mode: usize,
    pub(crate) merge_src: [usize; 4],
    /// Save As routing (Phase 6.5): 0 save as, 1 save a
    /// copy, 2 save swatches, 3 save brushes, 4 save
    /// adjustment settings, 5 export paths (.ai).
    pub(crate) save_mode: u8,
    /// File Info dialog: the five fields; fx_page selects
    /// the one in the text box.
    pub(crate) info_fields: [String; 5],
    pub(crate) load_op: usize,
    pub(crate) load_chan: usize,
    /// Print Options (Phase 6.5 Batch 4): emulsion-down and
    /// bilinear interpolation toggles (the rest ride opt_a /
    /// opt_b / fx_a marks bitmask / fx_b negative).
    pub(crate) pr_emulsion: bool,
    pub(crate) pr_bilinear: bool,
    /// Replace Color target (Phase 6.5 Batch 4), RGB 0..=255.
    pub(crate) rc_target: (u8, u8, u8),
    /// Variations working copy (Batch 4).
    pub(crate) var_doc: Option<pictodesk_core::document::Document>,
    /// Phase 7: the Pattern Maker's generated tile.
    pub(crate) pm_tile: Option<pictodesk_core::document::PatternData>,
    /// Variations thumbnail of the working copy (88x56 RGB).
    pub(crate) var_thumb: Vec<(u8, u8, u8)>,
    /// Color Settings: pending default working space index.
    pub(crate) cs_default: usize,
    // Phase 6 dialog state.
    /// Liquify: active tool (0 Warp, 1 Twirl CW, 2 Twirl CCW,
    /// 3 Pucker, 4 Bloat, 5 Shift, 6 Reflect, 7 Reconstruct,
    /// 8 Freeze, 9 Thaw), last stamp point, freeze mask, mesh.
    pub(crate) liq_tool: u8,
    pub(crate) liq_last: Option<(f32, f32)>,
    pub(crate) liq_zoom: f32,
    pub(crate) liq_pan: (f32, f32),
    pub(crate) liq_backdrop: bool,
    pub(crate) liq_undo: Vec<Vec<(f32, f32)>>,
    pub(crate) liq_seed: u32,
    pub(crate) liq_save_req: bool,
    pub(crate) liq_load_req: bool,
    pub(crate) spell_items: Vec<(usize, usize, usize, String)>,
    pub(crate) spell_pos: usize,
    pub(crate) spell_ignore: Vec<String>,
    pub(crate) web_knockout: bool,
    pub(crate) web_dither: bool,
    pub(crate) browser_dir: String,
    pub(crate) browser_entries: Vec<BrowserEntry>,
    pub(crate) browser_sel: usize,
    pub(crate) browser_sort_rank: bool,
    pub(crate) liq_frozen: Vec<u8>,
    pub(crate) liq_mesh: bool,
    /// Preset Manager: selected preset row.
    pub(crate) preset_sel: usize,
    /// Gradient Map "to" color (the "from" rides photo_color).
    pub(crate) gm_to: (u8, u8, u8),
    /// Note / slice being edited.
    pub(crate) note_target: Option<usize>,
    /// Audio annotation being edited (WAV bytes), carried
    /// from the note into the dialog and back on OK.
    pub(crate) note_audio: Option<Vec<u8>>,
    /// Insert Menu Item (Phase 6.5 Batch 6d): the flattened
    /// menu list and its scroll offset.
    pub(crate) menu_items: Vec<(String, Action)>,
    pub(crate) list_offset: usize,
    pub(crate) slice_target: Option<usize>,
    /// Extra text fields (Slice Options URL / Alt) + which of
    /// the three slice fields has keyboard focus (0/1/2).
    pub(crate) text2: String,
    pub(crate) text3: String,
    pub(crate) text_focus_n: u8,
    /// Liquify displacement field (doc-sized, source offsets),
    /// converted to the core field on OK.
    pub(crate) liq_disp: Vec<(f32, f32)>,
    /// Blending Options working copies: knockout mode, channel
    /// mask, toggles bitmask (1 clipped-as-group, 2 interior
    /// as group, 4 transparency shapes, 8 mask hides effects).
    pub(crate) blend_knockout: u8,
    pub(crate) blend_channels: u8,
    pub(crate) blend_toggles: u8,
}

impl Dialog {
    pub(crate) fn new(kind: DialogKind, doc: &Document) -> Self {
        let vals = match kind {
            DialogKind::ConfirmClose => vec![],
            DialogKind::SaveWorkspace
            | DialogKind::RenameItem
            | DialogKind::FileBrowser
            | DialogKind::SpellCheck
            | DialogKind::FindReplace => {
                vec![]
            }
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
            DialogKind::CanvasSize => vec![doc.width as f32, doc.height as f32],
            DialogKind::ImageSize => {
                vec![doc.width as f32, doc.height as f32, doc.resolution]
            }
            DialogKind::NewDoc => vec![doc.width as f32, doc.height as f32, 300.0],
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
            DialogKind::Type => vec![36.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 50.0, 0.0, 0.0],
            DialogKind::Effects => vec![],
            DialogKind::Extract => vec![],
            DialogKind::Transform3d => vec![0.0, 30.0],
            DialogKind::FitImage => vec![doc.width as f32, doc.height as f32],
            DialogKind::CondMode => vec![],
            DialogKind::ContactSheet => vec![4.0, 160.0],
            DialogKind::PicturePackage => vec![612.0, 792.0],
            DialogKind::WebGallery => vec![120.0],
            DialogKind::PageSetup => vec![612.0, 792.0, 100.0],
            DialogKind::Print => vec![1.0, 100.0, 36.0, 36.0],
            DialogKind::SaveForWeb => vec![80.0, 30.0],
            DialogKind::BitmapConv => vec![50.0],
            DialogKind::IndexedConv => vec![256.0],
            DialogKind::Duotone => vec![0.0, 0.0, 0.0, 0.0],
            DialogKind::ColorTable => vec![0.0, 0.0, 0.0],
            DialogKind::SelectiveColor => vec![0.0, 0.0, 0.0, 0.0],
            DialogKind::ReplaceColor => vec![40.0, 0.0, 0.0, 0.0],
            DialogKind::Variations => vec![20.0],
            DialogKind::PatternMaker => vec![],
            DialogKind::ApplyImage => vec![100.0],
            DialogKind::Calculations => vec![100.0],
            DialogKind::LoadSel => vec![],
            DialogKind::ChannelOptions => vec![255.0, 0.0, 0.0],
            DialogKind::MergeChannels => vec![],
            DialogKind::Fade => vec![100.0],
            DialogKind::StrokeSel => vec![3.0, 100.0],
            DialogKind::RotateArbitrary => vec![15.0],
            DialogKind::FileInfo => vec![],
            DialogKind::Preferences => vec![20.0],
            DialogKind::Defringe => vec![1.0],
            DialogKind::ColorSettings => vec![],
            DialogKind::GlobalAngle => vec![120.0],
            DialogKind::Blending => vec![
                100.0, 0.0, 255.0, 0.0, 255.0, 0.0, 0.0, 0.0, 0.0, 120.0, 30.0,
            ],
            DialogKind::Liquify => vec![50.0, 50.0],
            DialogKind::GradientMap => vec![],
            DialogKind::PresetManager => vec![],
            DialogKind::NoteEdit => vec![],
            DialogKind::SliceProps => vec![],
            DialogKind::Droplet => vec![],
            DialogKind::InsertStop => vec![],
            DialogKind::InsertMenuItem => vec![],
            DialogKind::ActionStop => vec![],
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
                16 => vec![20.0, 30.0],
                17 => vec![128.0],
                18 => vec![16.0],
                19 => vec![20.0],
                20 => vec![20.0],
                21 => vec![3.0, 25.0],
                23 => vec![3.0, 60.0, 10.0, 100.0, 100.0],
                24 => vec![5.0, 3.0, 100.0],
                25 => vec![3.0, 10.0, 15.0],
                26 => vec![50.0, 50.0],
                27 => vec![150.0, 50.0, 50.0],
                28 => vec![60.0, 50.0, 50.0, 40.0, 0.0],
                29 => vec![4.0, 8.0, 25.0],
                30 => vec![4.0, 4.0, 2.0],
                31 => vec![5.0, 5.0, 2.0],
                32 => vec![8.0, 5.0, 5.0],
                33 => vec![5.0, 5.0, 2.0],
                34 => vec![10.0, 25.0],
                35 => vec![12.0, 10.0],
                36 => vec![14.0, 2.0, 3.0],
                37 => vec![8.0, 8.0, 8.0],
                38 => vec![3.0, 5.0, 3.0],
                39 => vec![10.0, 8.0, 100.0, 20.0],
                40 => vec![6.0, 8.0, 5.0],
                41 => vec![4.0, 12.0, 7.0],
                42 => vec![16.0, 20.0, 100.0, 20.0],
                43 => vec![8.0, 5.0, 2.0],
                44 => vec![4.0, 38.0, 5.0],
                45 => vec![50.0, 15.0, 5.0],
                46 => vec![15.0, 5.0, 2.0],
                47 => vec![5.0, 5.0, 5.0],
                48 => vec![10.0, 25.0, 25.0],
                49 => vec![10.0, 7.0],
                50 => vec![10.0, 12.0],
                51 => vec![7.0, 8.0, 20.0],
                52 => vec![25.0, 25.0, 3.0],
                53 => vec![4.0, 3.0, 50.0],
                54 => vec![5.0, 5.0],
                55 => vec![50.0, 50.0, 100.0, 20.0],
                56 => vec![15.0, 50.0],
                57 => vec![4.0, 25.0],
                58 => vec![25.0, 10.0, 12.0],
                59 => vec![12.0, 25.0],
                60 => vec![25.0, 7.0],
                61 => vec![25.0, 25.0, 25.0],
                62 => vec![25.0, 7.0, 12.0],
                63 => vec![15.0, 60.0, 55.0],
                64 => vec![30.0, 5.0, 7.0],
                65 => vec![50.0, 50.0],
                66 => vec![20.0, 4.0, 5.0],
                67 => vec![16.0, 4.0, 5.0],
                68 => vec![100.0, 20.0],
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
            pref_toggles: Vec::new(),
            pref_cycles: Vec::new(),
            pref_sliders: Vec::new(),
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
            // Type Units (Preferences > Units & Rulers): px per
            // display unit for the Size slider, and its range in
            // display units (set by open_type_dialog).
            type_unit_k: 1.0,
            type_size_min: 6.0,
            type_size_max: 288.0,
            font_scroll: 0,
            extract_hi: Vec::new(),
            extract_fill: Vec::new(),
            extract_touch: Vec::new(),
            extract_smart: false,
            extract_mode: 0,
            extract_preview: false,
            shape3d: 1,
            wire_drag: None,
            cond_from: 0,
            cond_to: 0,
            pkg_layout: 0,
            web_format: 0,
            web_size: 0,
            web_weighted: false,
            print_profile: 0,
            fx_a: 0,
            fx_b: true,
            fx_c: 0,
            duo_inks: vec![([0, 0, 0], 0)],
            table_sel: 0,
            src_layer: 0,
            src_chan: 0,
            src2_layer: 0,
            src2_chan: 0,
            blend_idx: 0,
            mask_sel: 0,
            target_sel: 0,
            result_sel: 0,
            merge_mode: 0,
            merge_src: [0, 1, 2, 3],
            save_mode: 0,
            info_fields: Default::default(),
            load_op: 0,
            load_chan: 0,
            pr_emulsion: false,
            pr_bilinear: false,
            rc_target: (0, 0, 0),
            var_doc: None,
            pm_tile: None,
            var_thumb: Vec::new(),
            cs_default: 0,
            liq_tool: 0,
            liq_last: None,
            liq_zoom: 1.0,
            liq_pan: (0.0, 0.0),
            liq_backdrop: false,
            liq_undo: Vec::new(),
            liq_seed: 1,
            liq_save_req: false,
            liq_load_req: false,
            spell_items: Vec::new(),
            spell_pos: 0,
            spell_ignore: Vec::new(),
            web_knockout: false,
            web_dither: false,
            browser_dir: String::new(),
            browser_entries: Vec::new(),
            browser_sel: 0,
            browser_sort_rank: false,
            liq_frozen: Vec::new(),
            liq_mesh: false,
            preset_sel: 0,
            gm_to: (255, 255, 255),
            note_target: None,
            note_audio: None,
            menu_items: Vec::new(),
            list_offset: 0,
            slice_target: None,
            text2: String::new(),
            text3: String::new(),
            text_focus_n: 0,
            liq_disp: Vec::new(),
            blend_knockout: 0,
            blend_channels: 0xF,
            blend_toggles: 0,
        }
    }

    /// (label, min, max) per slider index.
    pub(crate) fn slider_defs(&self) -> Vec<(&'static str, f32, f32)> {
        match self.kind {
            DialogKind::SaveWorkspace
            | DialogKind::RenameItem
            | DialogKind::FileBrowser
            | DialogKind::SpellCheck
            | DialogKind::FindReplace
            | DialogKind::ConfirmClose => {
                vec![]
            }
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
            DialogKind::CanvasSize => {
                vec![("Width", 1.0, 10000.0), ("Height", 1.0, 10000.0)]
            }
            DialogKind::ImageSize | DialogKind::NewDoc => {
                vec![
                    ("Width", 1.0, 10000.0),
                    ("Height", 1.0, 10000.0),
                    ("Resolution", 1.0, 1200.0),
                ]
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
                ("Size", self.type_size_min, self.type_size_max),
                ("Tracking", -100.0, 1000.0),
                ("Leading (0=auto)", 0.0, 500.0),
                ("From char", 0.0, 10000.0),
                ("To char", 0.0, 10000.0),
                ("Box width (0=point)", 0.0, 4000.0),
                ("Space before", 0.0, 200.0),
                ("First-line indent", -200.0, 400.0),
                ("Warp bend", -100.0, 100.0),
                ("Warp H distort", -100.0, 100.0),
                ("Warp V distort", -100.0, 100.0),
            ],
            DialogKind::Effects => match self.fx_page {
                0 | 1 => vec![
                    ("Opacity", 0.0, 100.0),
                    ("Angle", -180.0, 180.0),
                    ("Distance", 0.0, 100.0),
                    ("Blur", 0.0, 100.0),
                    ("Intensity", 0.0, 100.0),
                    ("Contour", 0.0, 3.0),
                ],
                2 | 3 => vec![
                    ("Opacity", 0.0, 100.0),
                    ("Blur", 0.0, 100.0),
                    ("Intensity", 0.0, 100.0),
                    ("Contour", 0.0, 3.0),
                ],
                4 => vec![
                    ("Depth", 0.0, 1000.0),
                    ("Angle", -180.0, 180.0),
                    ("Highlight opacity", 0.0, 100.0),
                    ("Shadow opacity", 0.0, 100.0),
                    ("Contour", 0.0, 3.0),
                    ("Texture depth", 0.0, 200.0),
                ],
                5 => vec![
                    ("Opacity", 0.0, 100.0),
                    ("Angle", -180.0, 180.0),
                    ("Distance", 0.0, 100.0),
                    ("Blur", 0.0, 100.0),
                    ("Contour", 0.0, 3.0),
                ],
                6 => vec![("Opacity", 0.0, 100.0)],
                7 => vec![
                    ("Opacity", 0.0, 100.0),
                    ("Angle", -180.0, 180.0),
                    ("Scale", 10.0, 300.0),
                ],
                8 => vec![("Opacity", 0.0, 100.0), ("Scale", 10.0, 300.0)],
                _ => vec![("Opacity", 0.0, 100.0), ("Size", 1.0, 50.0)],
            },
            DialogKind::Blending => vec![
                ("Fill opacity", 0.0, 100.0),
                ("This: black", 0.0, 255.0),
                ("This: white", 0.0, 255.0),
                ("Under: black", 0.0, 255.0),
                ("Under: white", 0.0, 255.0),
                ("This blk fth", 0.0, 255.0),
                ("This wht fth", 0.0, 255.0),
                ("Und blk fth", 0.0, 255.0),
                ("Und wht fth", 0.0, 255.0),
                ("Global light angle", -180.0, 180.0),
                ("Global light altitude", 0.0, 90.0),
            ],
            DialogKind::Liquify => vec![("Brush size", 1.0, 500.0), ("Pressure", 1.0, 100.0)],
            DialogKind::GradientMap => vec![],
            DialogKind::PresetManager => vec![],
            DialogKind::NoteEdit => vec![],
            DialogKind::SliceProps => vec![],
            DialogKind::Droplet => vec![],
            DialogKind::InsertStop => vec![],
            DialogKind::InsertMenuItem => vec![],
            DialogKind::ActionStop => vec![],
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
            DialogKind::Print => vec![
                ("Copies", 1.0, 99.0),
                ("Scale %", 10.0, 400.0),
                ("Left (pt)", 0.0, 612.0),
                ("Top (pt)", 0.0, 792.0),
            ],
            DialogKind::SaveForWeb => vec![("Quality", 1.0, 100.0), ("Min quality", 1.0, 100.0)],
            DialogKind::BitmapConv => vec![("Threshold %", 0.0, 100.0)],
            DialogKind::IndexedConv => vec![("Colors", 2.0, 256.0)],
            DialogKind::Duotone => vec![
                ("Ink Red", 0.0, 255.0),
                ("Ink Green", 0.0, 255.0),
                ("Ink Blue", 0.0, 255.0),
                ("Curve Preset", 0.0, 3.0),
            ],
            DialogKind::ColorTable => vec![
                ("Red", 0.0, 255.0),
                ("Green", 0.0, 255.0),
                ("Blue", 0.0, 255.0),
            ],
            DialogKind::SelectiveColor => vec![
                ("Cyan %", -100.0, 100.0),
                ("Magenta %", -100.0, 100.0),
                ("Yellow %", -100.0, 100.0),
                ("Black %", -100.0, 100.0),
            ],
            DialogKind::ReplaceColor => vec![
                ("Fuzziness", 0.0, 200.0),
                ("Hue", -180.0, 180.0),
                ("Saturation", -100.0, 100.0),
                ("Lightness", -100.0, 100.0),
            ],
            DialogKind::Variations => vec![("Fine - Coarse", 1.0, 40.0)],
            DialogKind::PatternMaker => vec![],
            DialogKind::ApplyImage => vec![("Opacity %", 0.0, 100.0)],
            DialogKind::Calculations => vec![("Opacity %", 0.0, 100.0)],
            DialogKind::LoadSel => vec![],
            DialogKind::MergeChannels => vec![],
            DialogKind::Fade => vec![("Opacity %", 0.0, 100.0)],
            DialogKind::StrokeSel => vec![("Width px", 1.0, 64.0), ("Opacity %", 0.0, 100.0)],
            DialogKind::RotateArbitrary => vec![("Angle deg", 0.0, 180.0)],
            DialogKind::FileInfo => vec![],
            DialogKind::Preferences => prefs_page_sliders(self.fx_page)
                .iter()
                .map(|r| match r.kind {
                    PrefKind::Slider(_, min, max, _) => (r.label, min, max),
                    _ => ("", 0.0, 0.0),
                })
                .collect(),
            DialogKind::Defringe => vec![("Width px", 1.0, 10.0)],
            DialogKind::ChannelOptions => vec![
                ("Spot Red", 0.0, 255.0),
                ("Spot Green", 0.0, 255.0),
                ("Spot Blue", 0.0, 255.0),
            ],
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
                16 => vec![("Size", 2.0, 255.0), ("Depth", 1.0, 255.0)],
                17 => vec![("Level", 0.0, 255.0)],
                18 => vec![("Max radius", 4.0, 127.0)],
                19 => vec![("Cell size", 3.0, 300.0)],
                20 => vec![("Cell size", 3.0, 300.0)],
                21 => vec![("Radius", 1.0, 32.0), ("Threshold", 0.0, 100.0)],
                23 => vec![
                    ("Generators", 1.0, 20.0),
                    ("Wavelength", 10.0, 200.0),
                    ("Amplitude", 1.0, 60.0),
                    ("Scale H %", 1.0, 100.0),
                    ("Scale V %", 1.0, 100.0),
                ],
                24 => vec![
                    ("Distortion", 0.0, 20.0),
                    ("Smoothness", 1.0, 15.0),
                    ("Scaling %", 50.0, 200.0),
                ],
                25 => vec![
                    ("Graininess", 0.0, 10.0),
                    ("Glow amount", 0.0, 20.0),
                    ("Clear amount", 0.0, 20.0),
                ],
                26 => vec![("Scale H %", -200.0, 200.0), ("Scale V %", -200.0, 200.0)],
                27 => vec![
                    ("Brightness %", 10.0, 300.0),
                    ("Center X %", 0.0, 100.0),
                    ("Center Y %", 0.0, 100.0),
                ],
                28 => vec![
                    ("Intensity", 0.0, 100.0),
                    ("Focus", 1.0, 100.0),
                    ("Light X %", 0.0, 100.0),
                    ("Light Y %", 0.0, 100.0),
                    ("Exposure", -100.0, 100.0),
                ],
                29 => vec![
                    ("Pencil Width", 1.0, 24.0),
                    ("Pressure", 0.0, 15.0),
                    ("Paper Bright.", 0.0, 50.0),
                ],
                30 => vec![
                    ("Levels", 2.0, 8.0),
                    ("Simplicity", 0.0, 10.0),
                    ("Edge Fidelity", 1.0, 3.0),
                ],
                31 => vec![
                    ("Brush Size", 0.0, 10.0),
                    ("Brush Detail", 0.0, 10.0),
                    ("Texture", 1.0, 3.0),
                ],
                32 => vec![
                    ("Grain", 0.0, 20.0),
                    ("Highlight", 0.0, 10.0),
                    ("Intensity", 0.0, 10.0),
                ],
                33 => vec![
                    ("Brush Size", 0.0, 10.0),
                    ("Brush Detail", 0.0, 10.0),
                    ("Texture", 1.0, 3.0),
                ],
                34 => vec![("Glow Size", 0.0, 30.0), ("Brightness", 0.0, 50.0)],
                35 => vec![("Brush Size", 0.0, 50.0), ("Sharpness", 0.0, 40.0)],
                36 => vec![
                    ("Stroke Size", 0.0, 50.0),
                    ("Stroke Detail", 1.0, 3.0),
                    ("Softness", 0.0, 10.0),
                ],
                37 => vec![
                    ("Strength", 0.0, 15.0),
                    ("Detail", 1.0, 15.0),
                    ("Smoothness", 1.0, 15.0),
                ],
                38 => vec![
                    ("Thickness", 0.0, 10.0),
                    ("Intensity", 0.0, 10.0),
                    ("Posterize", 0.0, 6.0),
                ],
                39 => vec![
                    ("Stroke Length", 0.0, 40.0),
                    ("Stroke Detail", 1.0, 20.0),
                    ("Scaling %", 50.0, 200.0),
                    ("Relief", 0.0, 50.0),
                ],
                40 => vec![
                    ("Stroke Length", 0.0, 20.0),
                    ("Highlight", 0.0, 20.0),
                    ("Intensity", 0.0, 10.0),
                ],
                41 => vec![
                    ("Brush Size", 0.0, 10.0),
                    ("Definition", 0.0, 25.0),
                    ("Smoothness", 1.0, 15.0),
                ],
                42 => vec![
                    ("Brush Size", 0.0, 40.0),
                    ("Coverage", 0.0, 40.0),
                    ("Scaling %", 50.0, 200.0),
                    ("Relief", 0.0, 50.0),
                ],
                43 => vec![
                    ("Brush Detail", 1.0, 14.0),
                    ("Shadow", 0.0, 10.0),
                    ("Texture", 1.0, 3.0),
                ],
                44 => vec![
                    ("Edge Width", 1.0, 14.0),
                    ("Brightness", 0.0, 50.0),
                    ("Smoothness", 1.0, 15.0),
                ],
                45 => vec![
                    ("Direction Bal.", 0.0, 100.0),
                    ("Stroke Length", 3.0, 50.0),
                    ("Sharpness", 0.0, 10.0),
                ],
                46 => vec![
                    ("Stroke Length", 3.0, 50.0),
                    ("Sharpness", 0.0, 10.0),
                    ("Strength", 1.0, 3.0),
                ],
                47 => vec![
                    ("Balance", 0.0, 10.0),
                    ("Black Int.", 0.0, 10.0),
                    ("White Int.", 0.0, 10.0),
                ],
                48 => vec![
                    ("Stroke Length", 1.0, 50.0),
                    ("Dark Int.", 0.0, 50.0),
                    ("Light Int.", 0.0, 50.0),
                ],
                49 => vec![("Spray Radius", 0.0, 25.0), ("Smoothness", 1.0, 15.0)],
                50 => vec![("Stroke Length", 0.0, 20.0), ("Spray Radius", 0.0, 25.0)],
                51 => vec![
                    ("Stroke Width", 3.0, 15.0),
                    ("Pressure", 0.0, 15.0),
                    ("Contrast", 0.0, 45.0),
                ],
                52 => vec![
                    ("Charcoal Area", 0.0, 50.0),
                    ("Chalk Area", 0.0, 50.0),
                    ("Stroke Press.", 0.0, 5.0),
                ],
                53 => vec![
                    ("Thickness", 1.0, 10.0),
                    ("Detail", 0.0, 5.0),
                    ("L/D Balance", 0.0, 100.0),
                ],
                54 => vec![("Detail", 0.0, 10.0), ("Smoothness", 0.0, 10.0)],
                55 => vec![
                    ("FG Level", 0.0, 100.0),
                    ("BG Level", 0.0, 100.0),
                    ("Scaling %", 50.0, 200.0),
                    ("Relief", 0.0, 50.0),
                ],
                56 => vec![("Stroke Length", 1.0, 50.0), ("Balance", 0.0, 100.0)],
                57 => vec![("Size", 1.0, 12.0), ("Contrast", 0.0, 50.0)],
                58 => vec![
                    ("Balance", 0.0, 50.0),
                    ("Graininess", 0.0, 20.0),
                    ("Relief", 0.0, 25.0),
                ],
                59 => vec![("Detail", 1.0, 24.0), ("Darkness", 1.0, 50.0)],
                60 => vec![("Balance", 0.0, 50.0), ("Smoothness", 1.0, 15.0)],
                61 => vec![
                    ("Density", 0.0, 50.0),
                    ("FG Level", 0.0, 50.0),
                    ("BG Level", 0.0, 50.0),
                ],
                62 => vec![
                    ("Balance", 0.0, 50.0),
                    ("Smoothness", 1.0, 15.0),
                    ("Contrast", 0.0, 25.0),
                ],
                63 => vec![
                    ("Fiber Length", 1.0, 50.0),
                    ("Brightness", 0.0, 100.0),
                    ("Contrast", 0.0, 100.0),
                ],
                64 => vec![
                    ("Crack Spacing", 2.0, 100.0),
                    ("Crack Depth", 1.0, 10.0),
                    ("Crack Bright", 0.0, 10.0),
                ],
                65 => vec![("Intensity", 0.0, 100.0), ("Contrast", 0.0, 100.0)],
                66 => vec![
                    ("Tile Size", 2.0, 100.0),
                    ("Grout Width", 1.0, 15.0),
                    ("Lighten Grout", 0.0, 10.0),
                ],
                67 => vec![
                    ("Cell Size", 2.0, 50.0),
                    ("Border Thick.", 1.0, 10.0),
                    ("Light Int.", 0.0, 10.0),
                ],
                68 => vec![("Scaling %", 50.0, 200.0), ("Relief", 0.0, 50.0)],
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

    pub(crate) fn slider_display(&self, i: usize) -> String {
        let v = self.vals[i];
        match self.kind {
            DialogKind::Levels if i == 2 => format!("{v:.2}"),
            DialogKind::GaussianBlur => format!("{v:.1}"),
            DialogKind::HighPass => format!("{v:.1}"),
            DialogKind::Unsharp if i <= 1 => format!("{v:.2}"),
            DialogKind::AddNoise => format!("{:.0}%", v * 100.0),
            DialogKind::Preferences => {
                let unit = crate::prefs::prefs_page_sliders(self.fx_page)
                    .get(i)
                    .and_then(|r| match r.kind {
                        crate::prefs::PrefKind::Slider(_, _, _, u) => Some(u),
                        _ => None,
                    })
                    .unwrap_or("");
                if unit.is_empty() {
                    format!("{}", v.round() as i32)
                } else {
                    format!("{}{}", v.round() as i32, unit)
                }
            }
            _ => format!("{}", v.round() as i32),
        }
    }
}

/// Replace a byte range in a type layer's text, shifting
/// style runs so character styling survives the edit.
pub(crate) fn spec_replace(
    spec: &mut pictodesk_core::text::TextSpec,
    start: usize,
    end: usize,
    repl: &str,
) {
    spec.text.replace_range(start..end, repl);
    let delta = repl.len() as isize - (end - start) as isize;
    for run in &mut spec.runs {
        if run.end <= start {
            // Before the edit: untouched.
        } else if run.start >= end {
            run.start = (run.start as isize + delta).max(0) as usize;
            run.end = (run.end as isize + delta).max(0) as usize;
        } else {
            if run.start > start {
                run.start = start;
            }
            run.end = if run.end > end {
                (run.end as isize + delta).max(0) as usize
            } else {
                start + repl.len()
            };
        }
    }
    spec.runs.retain(|r| r.end > r.start);
}

/// The bundled basic English wordlist (spell check).
pub(crate) fn wordlist() -> &'static std::collections::HashSet<String> {
    static WL: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    WL.get_or_init(|| {
        include_str!("../words.txt")
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    })
}

pub(crate) fn personal_dict() -> std::collections::HashSet<String> {
    std::fs::read_to_string(pictodesk_dir().join("dictionary.txt"))
        .map(|t| {
            t.lines()
                .map(|l| l.trim().to_lowercase())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Word tokens of a text: (byte start, byte end, word).
/// Words are letter runs with internal apostrophes.
pub(crate) fn word_tokens(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        let part = c.is_alphabetic() || (c == '\'' && start.is_some());
        if part {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(st) = start.take() {
            out.push((st, i, text[st..i].to_string()));
        }
    }
    if let Some(st) = start {
        out.push((st, text.len(), text[st..].to_string()));
    }
    out
}

pub(crate) fn spell_known(
    word: &str,
    personal: &std::collections::HashSet<String>,
    ignore: &[String],
) -> bool {
    let lower = word.to_lowercase();
    let base = lower.strip_suffix("'s").unwrap_or(&lower);
    wordlist().contains(base) || personal.contains(base) || ignore.iter().any(|i| i == base)
}

/// Minimal JPEG EXIF read: Make / Model / DateTime from the
/// APP1 segment's TIFF IFD0.
pub(crate) fn exif_summary(bytes: &[u8]) -> String {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return String::new();
    }
    let mut i = 2usize;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            break;
        }
        let marker = bytes[i + 1];
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 {
            break;
        }
        if marker == 0xE1 && i + 10 <= bytes.len() && &bytes[i + 4..i + 10] == b"Exif\0\0" {
            return tiff_summary(&bytes[i + 10..(i + 2 + len).min(bytes.len())]);
        }
        i += 2 + len;
    }
    String::new()
}

pub(crate) fn tiff_summary(t: &[u8]) -> String {
    if t.len() < 8 {
        return String::new();
    }
    let le = &t[0..2] == b"II";
    let u16f = |o: usize| -> u16 {
        if o + 2 > t.len() {
            0
        } else if le {
            u16::from_le_bytes([t[o], t[o + 1]])
        } else {
            u16::from_be_bytes([t[o], t[o + 1]])
        }
    };
    let u32f = |o: usize| -> u32 {
        if o + 4 > t.len() {
            0
        } else if le {
            u32::from_le_bytes([t[o], t[o + 1], t[o + 2], t[o + 3]])
        } else {
            u32::from_be_bytes([t[o], t[o + 1], t[o + 2], t[o + 3]])
        }
    };
    if u16f(2) != 42 {
        return String::new();
    }
    let ifd = u32f(4) as usize;
    if ifd + 2 > t.len() {
        return String::new();
    }
    let count = u16f(ifd) as usize;
    let mut parts: Vec<String> = Vec::new();
    for k in 0..count {
        let e = ifd + 2 + k * 12;
        if e + 12 > t.len() {
            break;
        }
        let tag = u16f(e);
        if !matches!(tag, 0x010F | 0x0110 | 0x0132) || u16f(e + 2) != 2 {
            continue;
        }
        let n = u32f(e + 4) as usize;
        let bytes: Vec<u8> = if n <= 4 {
            t[e + 8..e + 8 + n].to_vec()
        } else {
            let off = u32f(e + 8) as usize;
            if off + n > t.len() {
                continue;
            }
            t[off..off + n].to_vec()
        };
        let text: String = bytes
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as char)
            .collect();
        if !text.trim().is_empty() {
            parts.push(text.trim().to_string());
        }
    }
    parts.join(" · ")
}

pub(crate) struct DialogLayout {
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
    /// Open the Modify Selection dialog for one operation
    /// (0 Expand, 1 Contract, 2 Smooth, 3 Border).
    pub(crate) fn open_modify_dialog(&mut self, op: u8) {
        let mut d = Dialog::new(DialogKind::ModifySel, &self.doc);
        d.modify_op = op;
        self.dialog = Some(d);
    }

    /// Build the Adjustment a dialog's current values describe.
    pub(crate) fn adjustment_from_dialog(d: &Dialog) -> pictodesk_core::adjust::Adjustment {
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
            DialogKind::GradientMap => Adjustment::GradientMap {
                from: d.photo_color,
                to: d.gm_to,
                reverse: d.opt_a,
            },
            _ => Adjustment::PhotoFilter {
                color: d.photo_color,
                density: v[0] as u8,
            },
        }
    }

    /// Open an adjustment dialog that creates a new adjustment
    /// layer (existing = None) or edits an existing one live.
    pub(crate) fn open_adjustment_dialog(&mut self, kind: &str, existing: Option<usize>) {
        use pictodesk_core::adjust::Adjustment;
        let dk = match kind {
            "levels" => DialogKind::Levels,
            "curves" => DialogKind::Curves,
            "brightness" => DialogKind::BrightnessContrast,
            "hue" => DialogKind::HueSaturation,
            "balance" => DialogKind::ColorBalance,
            "gradientmap" => DialogKind::GradientMap,
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
                    "gradientmap" => Adjustment::GradientMap {
                        from: (0, 0, 0),
                        to: (255, 255, 255),
                        reverse: false,
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
                Adjustment::GradientMap { from, to, reverse } => {
                    d.photo_color = *from;
                    d.gm_to = *to;
                    d.opt_a = *reverse;
                }
            }
        }
        d.adj_layer = Some(layer_idx);
        d.adj_snapshot = Some(snapshot);
        let title = d.kind.title();
        self.dialog = Some(d);
        self.status = format!("Adjustment layer: {title}");
    }

    /// File Browser (7.0): browse a folder's images with
    /// thumbnails, letter ranks, rotate-on-open flags, an
    /// EXIF info line, and batch rename.
    pub(crate) fn open_file_browser(&mut self) {
        let mut d = Dialog::new(DialogKind::FileBrowser, &self.doc);
        d.text_focus = true;
        let start = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".into());
        d.browser_dir = start;
        self.dialog = Some(d);
        self.browser_rescan();
    }

    pub(crate) fn browser_meta() -> std::collections::HashMap<String, (u8, i8)> {
        let mut map = std::collections::HashMap::new();
        if let Ok(text) = std::fs::read_to_string(pictodesk_dir().join("browser.txt")) {
            for line in text.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() >= 3 {
                    map.insert(
                        f[0].to_string(),
                        (f[1].parse().unwrap_or(0), f[2].parse().unwrap_or(0)),
                    );
                }
            }
        }
        map
    }

    pub(crate) fn browser_save_meta(&self) {
        let Some(d) = &self.dialog else { return };
        let mut map = Self::browser_meta();
        for e in &d.browser_entries {
            if !e.is_dir {
                map.insert(e.path.to_string_lossy().into_owned(), (e.rank, e.rotate));
            }
        }
        let dir = pictodesk_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mut text = String::new();
        for (k, (rank, rot)) in map.iter() {
            if *rank != 0 || *rot != 0 {
                text += &format!("{k}\t{rank}\t{rot}\n");
            }
        }
        let _ = std::fs::write(dir.join("browser.txt"), text);
    }

    pub(crate) fn browser_rescan(&mut self) {
        let meta = Self::browser_meta();
        let (dir, sort_rank) = {
            let Some(d) = &self.dialog else { return };
            (d.browser_dir.clone(), d.browser_sort_rank)
        };
        let mut entries: Vec<BrowserEntry> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut paths: Vec<std::path::PathBuf> =
                rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            paths.sort();
            for p in paths {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if p.is_dir() {
                    entries.push(BrowserEntry {
                        path: p,
                        name,
                        is_dir: true,
                        rank: 0,
                        rotate: 0,
                        tw: 0,
                        th: 0,
                        thumb: Vec::new(),
                        info: "Folder".into(),
                    });
                    continue;
                }
                let ext = p
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if !matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "tga" | "pcx"
                ) {
                    continue;
                }
                let key = p.to_string_lossy().into_owned();
                let (rank, rotate) = meta.get(&key).copied().unwrap_or((0, 0));
                let mut entry = BrowserEntry {
                    path: p.clone(),
                    name,
                    is_dir: false,
                    rank,
                    rotate,
                    tw: 0,
                    th: 0,
                    thumb: Vec::new(),
                    info: String::new(),
                };
                if let Ok(bytes) = std::fs::read(&p) {
                    let exif = exif_summary(&bytes);
                    if let Ok(doc) = pictodesk_core::io::import_image(&bytes) {
                        let (w, h) = (doc.width, doc.height);
                        let full = doc.render_region_rgba8(0, 0, w, h);
                        let (tw, th) = if w >= h {
                            (56u32, (56 * h / w.max(1)).max(1))
                        } else {
                            ((56 * w / h.max(1)).max(1), 56u32)
                        };
                        let mut thumb = vec![0u8; (tw * th * 4) as usize];
                        for y in 0..th {
                            for x in 0..tw {
                                let (sx, sy) = (x * w / tw, y * h / th);
                                let si = ((sy * w + sx) * 4) as usize;
                                let di = ((y * tw + x) * 4) as usize;
                                thumb[di..di + 4].copy_from_slice(&full[si..si + 4]);
                            }
                        }
                        entry.tw = tw;
                        entry.th = th;
                        entry.thumb = thumb;
                        entry.info = if exif.is_empty() {
                            format!("{w} x {h}")
                        } else {
                            format!("{w} x {h} · {exif}")
                        };
                    } else {
                        entry.info = "Unreadable".into();
                    }
                }
                entries.push(entry);
                if entries.len() >= 60 {
                    break;
                }
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                if sort_rank {
                    a.rank
                        .cmp(&b.rank)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                } else {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                }
            })
        });
        if let Some(d) = &mut self.dialog {
            d.browser_entries = entries;
            d.browser_sel = 0;
        }
    }

    pub(crate) fn browser_cell(&self, r: Rect, i: usize) -> Rect {
        Rect::new(
            r.x + 12 + (i % 4) as i32 * 94,
            r.y + 64 + (i / 4) as i32 * 68,
            92,
            66,
        )
    }

    pub(crate) fn browser_button_rects(&self, r: Rect) -> [Rect; 8] {
        let mut out = [Rect::new(0, 0, 0, 0); 8];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = Rect::new(
                r.x + 12 + (i % 4) as i32 * 94,
                r.y + 288 + (i / 4) as i32 * 24,
                92,
                20,
            );
        }
        out
    }

    pub(crate) fn browser_batch_rename(&mut self) {
        let prefix = {
            let Some(d) = &self.dialog else { return };
            d.text.trim().to_string()
        };
        if prefix.is_empty() {
            self.status = "Batch Rename: type a prefix first".into();
            return;
        }
        let mut n = 0;
        let mut renames: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
        if let Some(d) = &self.dialog {
            for e in &d.browser_entries {
                if e.is_dir {
                    continue;
                }
                n += 1;
                let ext = e
                    .path
                    .extension()
                    .map(|x| format!(".{}", x.to_string_lossy()))
                    .unwrap_or_default();
                let new_path = e.path.with_file_name(format!("{prefix}{n:03}{ext}"));
                if new_path != e.path && !new_path.exists() {
                    renames.push((e.path.clone(), new_path));
                }
            }
        }
        let mut done = 0;
        for (from, to) in &renames {
            if std::fs::rename(from, to).is_ok() {
                done += 1;
            }
        }
        self.browser_rescan();
        self.status = format!("Batch Rename: {done} file(s) renamed");
    }

    /// All unknown words across the document's type layers:
    /// (layer, byte start, byte end, word).
    pub(crate) fn spell_collect(&self, ignore: &[String]) -> Vec<(usize, usize, usize, String)> {
        let personal = personal_dict();
        let mut out = Vec::new();
        for (li, layer) in self.doc.layers.iter().enumerate() {
            let Some(spec) = &layer.text else { continue };
            for (st, en, word) in word_tokens(&spec.text) {
                if word.len() >= 2 && !spell_known(&word, &personal, ignore) {
                    out.push((li, st, en, word));
                }
            }
        }
        out
    }

    pub(crate) fn open_spell_check(&mut self) {
        let mut d = Dialog::new(DialogKind::SpellCheck, &self.doc);
        d.spell_items = self.spell_collect(&d.spell_ignore);
        d.spell_pos = 0;
        d.text_focus = true;
        if let Some((_, _, _, w)) = d.spell_items.first() {
            d.text = w.clone();
        }
        self.dialog = Some(d);
    }

    /// After a spell action, refresh the item list (offsets
    /// may have shifted) and prefill the Change field.
    pub(crate) fn spell_refresh(&mut self) {
        let ignore = self
            .dialog
            .as_ref()
            .map(|d| d.spell_ignore.clone())
            .unwrap_or_default();
        let items = self.spell_collect(&ignore);
        if let Some(d) = &mut self.dialog {
            d.spell_items = items;
            if d.spell_pos > d.spell_items.len() {
                d.spell_pos = d.spell_items.len();
            }
            if let Some((_, _, _, w)) = d.spell_items.get(d.spell_pos) {
                d.text = w.clone();
            }
        }
    }

    pub(crate) fn spell_change(&mut self) {
        let Some(d) = &self.dialog else { return };
        let Some((li, st, en, _)) = d.spell_items.get(d.spell_pos).cloned() else {
            return;
        };
        let repl = d.text.trim().to_string();
        if repl.is_empty() {
            return;
        }
        self.structural("Check Spelling", |doc| {
            if let Some(spec) = &mut doc.layers[li].text {
                spec_replace(spec, st, en, &repl);
            }
            typo::render_type_layer(doc, li);
        });
        self.spell_refresh();
    }

    pub(crate) fn spell_add(&mut self) {
        let word = {
            let Some(d) = &self.dialog else { return };
            let Some((_, _, _, w)) = d.spell_items.get(d.spell_pos) else {
                return;
            };
            w.to_lowercase()
        };
        let dir = pictodesk_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("dictionary.txt");
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text += &format!("{word}\n");
        let _ = std::fs::write(&path, text);
        self.spell_refresh();
    }

    /// Matches of `needle` in type layers: (layer, start, end).
    pub(crate) fn find_text_matches(
        &self,
        needle: &str,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        if needle.is_empty() {
            return out;
        }
        for (li, layer) in self.doc.layers.iter().enumerate() {
            let Some(spec) = &layer.text else { continue };
            let hay = if case_sensitive {
                spec.text.clone()
            } else {
                spec.text.to_lowercase()
            };
            let ndl = if case_sensitive {
                needle.to_string()
            } else {
                needle.to_lowercase()
            };
            let mut from = 0usize;
            while let Some(pos) = hay[from..].find(&ndl) {
                let (st, en) = (from + pos, from + pos + ndl.len());
                let ok = if whole_word {
                    let before = hay[..st].chars().last();
                    let after = hay[en..].chars().next();
                    !before.map(|c| c.is_alphanumeric()).unwrap_or(false)
                        && !after.map(|c| c.is_alphanumeric()).unwrap_or(false)
                } else {
                    true
                };
                if ok {
                    out.push((li, st, en));
                }
                from = en.max(st + 1);
            }
        }
        out
    }

    pub(crate) fn find_replace_all(&mut self) -> usize {
        let (needle, repl, case_sensitive, whole_word) = {
            let Some(d) = &self.dialog else { return 0 };
            (d.text.clone(), d.text2.clone(), d.opt_a, d.opt_b)
        };
        let mut matches = self.find_text_matches(&needle, case_sensitive, whole_word);
        if matches.is_empty() {
            self.status = "Find and Replace: no matches".into();
            return 0;
        }
        let count = matches.len();
        // Replace back to front so earlier offsets stay valid.
        matches.sort_by_key(|m| std::cmp::Reverse((m.0, m.1)));
        self.structural("Find and Replace", |doc| {
            let mut touched: Vec<usize> = Vec::new();
            for (li, st, en) in &matches {
                if let Some(spec) = &mut doc.layers[*li].text {
                    spec_replace(spec, *st, *en, &repl);
                    if !touched.contains(li) {
                        touched.push(*li);
                    }
                }
            }
            for li in touched {
                typo::render_type_layer(doc, li);
            }
        });
        self.status = format!("Replaced {count} occurrence(s)");
        count
    }

    /// Clicking a panel name opens the rename dialog (7.0):
    /// kind 0 = layer, 1 = alpha channel.
    pub(crate) fn open_rename(&mut self, kind: u8, idx: usize) {
        let name = if kind == 0 {
            self.doc.layers.get(idx).map(|l| l.name.clone())
        } else {
            self.doc.alphas.get(idx).map(|a| a.name.clone())
        };
        let Some(name) = name else { return };
        let mut d = Dialog::new(DialogKind::RenameItem, &self.doc);
        d.fx_page = kind as usize;
        d.preset_sel = idx;
        d.text = name;
        d.text_focus = true;
        self.dialog = Some(d);
    }

    /// Reopen the adjustment dialog a settings step belongs to,
    /// with the step's values loaded (Phase 6.5 Batch 6d).
    pub(crate) fn apply_step_to_dialog(&mut self, step: RecordedStep) {
        use pictodesk_core::adjust::Adjustment;
        let kind = match &step {
            RecordedStep::Adjustment(adj) => match adj {
                Adjustment::Levels { .. } => DialogKind::Levels,
                Adjustment::Curves { .. } => DialogKind::Curves,
                Adjustment::BrightnessContrast { .. } => DialogKind::BrightnessContrast,
                Adjustment::HueSaturation { .. } => DialogKind::HueSaturation,
                Adjustment::ColorBalance { .. } => DialogKind::ColorBalance,
                Adjustment::GradientMap { .. } => DialogKind::GradientMap,
                Adjustment::PhotoFilter { .. } => DialogKind::PhotoFilter,
            },
            RecordedStep::ChannelMixer(_) => DialogKind::ChannelMixer,
            RecordedStep::SelectiveColor(..) => DialogKind::SelectiveColor,
            RecordedStep::ReplaceColor(..) => DialogKind::ReplaceColor,
            _ => {
                self.status = "That file holds no adjustment settings".into();
                return;
            }
        };
        self.open_dialog(kind);
        let Some(d) = &mut self.dialog else {
            return;
        };
        match step {
            RecordedStep::Adjustment(adj) => match adj {
                Adjustment::Levels {
                    in_black,
                    in_white,
                    gamma,
                    out_black,
                    out_white,
                } => {
                    d.vals = vec![
                        in_black as f32,
                        in_white as f32,
                        gamma,
                        out_black as f32,
                        out_white as f32,
                    ];
                }
                Adjustment::Curves { points } => d.curve_points = points,
                Adjustment::BrightnessContrast {
                    brightness,
                    contrast,
                } => {
                    d.vals = vec![brightness as f32, contrast as f32];
                }
                Adjustment::HueSaturation {
                    hue,
                    saturation,
                    lightness,
                } => {
                    d.vals = vec![hue as f32, saturation as f32, lightness as f32];
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
                Adjustment::GradientMap { from, to, reverse } => {
                    d.photo_color = from;
                    d.gm_to = to;
                    d.opt_a = reverse;
                }
                Adjustment::PhotoFilter { color, density } => {
                    d.photo_color = color;
                    d.vals = vec![density as f32];
                }
            },
            RecordedStep::ChannelMixer(m) => {
                d.mixer = m;
                d.mix_out = 0;
                d.vals = vec![
                    m.rows[0][0] * 100.0,
                    m.rows[0][1] * 100.0,
                    m.rows[0][2] * 100.0,
                    m.rows[0][3] * 100.0,
                    m.constant[0] * 100.0,
                ];
            }
            RecordedStep::SelectiveColor(range, adj, rel) => {
                d.fx_a = range;
                d.vals = vec![adj[0] as f32, adj[1] as f32, adj[2] as f32, adj[3] as f32];
                d.fx_b = rel;
            }
            RecordedStep::ReplaceColor(t, fz, hu, sa, li) => {
                d.rc_target = t;
                d.vals = vec![fz as f32, hu as f32, sa as f32, li as f32];
            }
            _ => {}
        }
        self.dialog_refresh_preview();
        self.status = "Settings loaded".into();
    }

    // ------------------------------------------------------------ preferences
    /// Read one draft toggle by its key.
    pub(crate) fn prefs_bool(&self, k: PrefBool) -> bool {
        match k {
            PrefBool::Tooltips => self.show_tooltips,
            PrefBool::BeepDone => self.prefs.beep_done,
            PrefBool::DynSliders => self.prefs.dyn_sliders,
            PrefBool::SavePaletteLocs => self.prefs.save_palette_locs,
            PrefBool::ShiftToolSwitch => self.prefs.shift_tool_switch,
            PrefBool::PasteResize => self.prefs.paste_resize,
            PrefBool::AutoSnapshot => self.hist_auto_snapshot,
            PrefBool::OptionsBar => self.show_options_bar,
            PrefBool::ZoomScroll => self.zoom_with_scroll,
            PrefBool::ChannelsInColor => self.prefs.channels_in_color,
            PrefBool::PixelDoubling => self.prefs.pixel_doubling,
            PrefBool::BrushCrosshair => self.prefs.brush_crosshair,
            PrefBool::SliceNumbers => self.prefs.slice_numbers,
            PrefBool::CacheHistograms => self.prefs.cache_histograms,
        }
    }

    pub(crate) fn prefs_set_bool(&mut self, k: PrefBool, v: bool) {
        match k {
            PrefBool::Tooltips => self.show_tooltips = v,
            PrefBool::BeepDone => self.prefs.beep_done = v,
            PrefBool::DynSliders => self.prefs.dyn_sliders = v,
            PrefBool::SavePaletteLocs => self.prefs.save_palette_locs = v,
            PrefBool::ShiftToolSwitch => self.prefs.shift_tool_switch = v,
            PrefBool::PasteResize => self.prefs.paste_resize = v,
            PrefBool::AutoSnapshot => self.hist_auto_snapshot = v,
            PrefBool::OptionsBar => self.show_options_bar = v,
            PrefBool::ZoomScroll => self.zoom_with_scroll = v,
            PrefBool::ChannelsInColor => self.prefs.channels_in_color = v,
            PrefBool::PixelDoubling => self.prefs.pixel_doubling = v,
            PrefBool::BrushCrosshair => self.prefs.brush_crosshair = v,
            PrefBool::SliceNumbers => self.prefs.slice_numbers = v,
            PrefBool::CacheHistograms => self.prefs.cache_histograms = v,
        }
    }

    /// Read one draft cycle by its key.
    pub(crate) fn prefs_cycle(&self, k: PrefCycle) -> u8 {
        match k {
            PrefCycle::Interp => self.prefs.interp,
            PrefCycle::RedoKey => self.prefs.redo_key,
            PrefCycle::DefaultSpace => match self.default_space {
                pictodesk_core::color::WorkingSpace::Srgb => 0,
                pictodesk_core::color::WorkingSpace::AdobeRgb => 1,
                pictodesk_core::color::WorkingSpace::ProPhoto => 2,
            },
            PrefCycle::ImgPreviews => self.prefs.img_previews,
            PrefCycle::FileExtCase => self.prefs.file_ext_case,
            PrefCycle::PaintCursor => self.prefs.paint_cursor,
            PrefCycle::OtherCursor => self.prefs.other_cursor,
            PrefCycle::GridSize => self.prefs.grid_size,
            PrefCycle::GridColors => self.prefs.grid_colors,
            PrefCycle::RulerUnits => self.prefs.ruler_units,
            PrefCycle::TypeUnits => self.prefs.type_units,
            PrefCycle::PointPica => self.prefs.point_pica,
            PrefCycle::GuideColor => self.prefs.guide_color,
            PrefCycle::GuideStyle => self.prefs.guide_style,
            PrefCycle::GridColor => self.prefs.grid_color,
            PrefCycle::GridStyle => self.prefs.grid_style,
            PrefCycle::SliceColor => self.prefs.slice_color,
            PrefCycle::GamutColor => self.prefs.gamut_color,
        }
    }

    pub(crate) fn prefs_set_cycle(&mut self, k: PrefCycle, v: u8) {
        match k {
            PrefCycle::Interp => self.prefs.interp = v.min(2),
            PrefCycle::RedoKey => self.prefs.redo_key = v.min(1),
            PrefCycle::DefaultSpace => {
                self.default_space = match v {
                    1 => pictodesk_core::color::WorkingSpace::AdobeRgb,
                    2 => pictodesk_core::color::WorkingSpace::ProPhoto,
                    _ => pictodesk_core::color::WorkingSpace::Srgb,
                }
            }
            PrefCycle::ImgPreviews => self.prefs.img_previews = v.min(2),
            PrefCycle::FileExtCase => self.prefs.file_ext_case = v.min(1),
            PrefCycle::PaintCursor => self.prefs.paint_cursor = v.min(2),
            PrefCycle::OtherCursor => self.prefs.other_cursor = v.min(1),
            PrefCycle::GridSize => self.prefs.grid_size = v.min(3),
            PrefCycle::GridColors => self.prefs.grid_colors = v.min(3),
            PrefCycle::RulerUnits => self.prefs.ruler_units = v.min(6),
            PrefCycle::TypeUnits => self.prefs.type_units = v.min(2),
            PrefCycle::PointPica => self.prefs.point_pica = v.min(1),
            PrefCycle::GuideColor => self.prefs.guide_color = v.min(7),
            PrefCycle::GuideStyle => self.prefs.guide_style = v.min(1),
            PrefCycle::GridColor => self.prefs.grid_color = v.min(7),
            PrefCycle::GridStyle => self.prefs.grid_style = v.min(2),
            PrefCycle::SliceColor => self.prefs.slice_color = v.min(7),
            PrefCycle::GamutColor => self.prefs.gamut_color = v.min(7),
        }
    }

    /// Read one draft slider by its key.
    pub(crate) fn prefs_num(&self, k: PrefNum) -> f32 {
        match k {
            PrefNum::HistStates => self.hist.limit().unwrap_or(20) as f32,
            PrefNum::RecentCount => self.prefs.recent_count as f32,
            PrefNum::GamutOpacity => self.prefs.gamut_opacity as f32,
            PrefNum::NewPrintRes => self.prefs.new_print_res as f32,
            PrefNum::NewScreenRes => self.prefs.new_screen_res as f32,
            PrefNum::GridEvery => self.prefs.grid_every as f32,
            PrefNum::GridSubdiv => self.prefs.grid_subdiv as f32,
            PrefNum::RamPct => self.prefs.ram_pct as f32,
            PrefNum::CacheLevels => self.prefs.cache_levels as f32,
        }
    }

    pub(crate) fn prefs_set_num(&mut self, k: PrefNum, v: f32) {
        match k {
            PrefNum::HistStates => self.hist.set_limit(Some((v.round() as usize).max(1))),
            PrefNum::RecentCount => {
                self.prefs.recent_count = (v.round() as u8).min(30);
            }
            PrefNum::GamutOpacity => {
                self.prefs.gamut_opacity = (v.round() as u8).min(100);
            }
            PrefNum::NewPrintRes => {
                self.prefs.new_print_res = (v.round() as u16).clamp(72, 1200);
            }
            PrefNum::NewScreenRes => {
                self.prefs.new_screen_res = (v.round() as u16).clamp(72, 1200);
            }
            PrefNum::GridEvery => {
                self.prefs.grid_every = (v.round() as u16).clamp(1, 2000);
            }
            PrefNum::GridSubdiv => {
                self.prefs.grid_subdiv = (v.round() as u8).clamp(1, 100);
            }
            PrefNum::RamPct => self.prefs.ram_pct = (v.round() as u8).clamp(5, 100),
            PrefNum::CacheLevels => {
                self.prefs.cache_levels = (v.round() as u8).clamp(1, 8);
            }
        }
    }

    /// All slider rows in PREF_ROWS order (the draft's slider order).
    fn prefs_all_sliders() -> Vec<&'static crate::prefs::PrefRowDef> {
        PREF_ROWS
            .iter()
            .filter(|r| matches!(r.kind, PrefKind::Slider(..)))
            .collect()
    }

    /// Fill the dialog draft from the app (open). Takes the
    /// local dialog: open_dialog assigns self.dialog only after
    /// the init block runs, so the draft cannot go through it.
    pub(crate) fn prefs_load_draft(&mut self, d: &mut Dialog) {
        d.pref_toggles = PREF_ROWS
            .iter()
            .filter_map(|r| match r.kind {
                PrefKind::Toggle(k) => Some(self.prefs_bool(k)),
                _ => None,
            })
            .collect();
        d.pref_cycles = PREF_ROWS
            .iter()
            .filter_map(|r| match r.kind {
                PrefKind::Cycle(k, _, _) => Some(self.prefs_cycle(k)),
                _ => None,
            })
            .collect();
        d.pref_sliders = PREF_ROWS
            .iter()
            .filter_map(|r| match r.kind {
                PrefKind::Slider(k, _, _, _) => Some(self.prefs_num(k)),
                _ => None,
            })
            .collect();
        // Page 0's slider values live in d.vals.
        let all = Self::prefs_all_sliders();
        d.vals = prefs_page_sliders(d.fx_page)
            .iter()
            .map(|row| {
                let idx = all.iter().position(|r| std::ptr::eq(*r, *row)).unwrap_or(0);
                d.pref_sliders.get(idx).copied().unwrap_or(0.0)
            })
            .collect();
    }

    /// Load d.vals from the draft for the dialog's current page.
    pub(crate) fn prefs_load_page_vals(&mut self) {
        let page = self.dialog.as_ref().map(|d| d.fx_page).unwrap_or(0);
        let vals: Vec<f32> = prefs_page_sliders(page)
            .iter()
            .map(|row| {
                let idx = crate::prefs::prefs_slider_idx(row);
                self.dialog
                    .as_ref()
                    .and_then(|d| d.pref_sliders.get(idx).copied())
                    .unwrap_or(0.0)
            })
            .collect();
        if let Some(d) = &mut self.dialog {
            d.vals = vals;
        }
    }

    /// Save d.vals into the draft for the current page.
    pub(crate) fn prefs_save_page_vals(&mut self) {
        let page = self.dialog.as_ref().map(|d| d.fx_page).unwrap_or(0);
        let vals = self
            .dialog
            .as_ref()
            .map(|d| d.vals.clone())
            .unwrap_or_default();
        let Some(d) = &mut self.dialog else {
            return;
        };
        for (i, row) in prefs_page_sliders(page).iter().enumerate() {
            let idx = crate::prefs::prefs_slider_idx(row);
            if let (Some(v), Some(slot)) = (vals.get(i), d.pref_sliders.get_mut(idx)) {
                *slot = *v;
            }
        }
    }

    /// Switch the Preferences page, carrying the draft's slider
    /// values across (they live per-page in d.vals).
    pub(crate) fn prefs_set_page(&mut self, page: usize) {
        self.prefs_save_page_vals();
        if let Some(d) = &mut self.dialog {
            d.fx_page = page % PREF_PAGES.len();
        }
        self.prefs_load_page_vals();
    }

    /// Write the whole draft back to the app (OK). Takes the
    /// dialog: dialog_ok takes it out of self.dialog first.
    pub(crate) fn prefs_apply_draft(&mut self, d: &Dialog) {
        // Save the current page's slider values into the draft.
        let all = Self::prefs_all_sliders();
        let mut sliders = d.pref_sliders.clone();
        for (i, row) in prefs_page_sliders(d.fx_page).iter().enumerate() {
            if let Some(idx) = all.iter().position(|r| std::ptr::eq(*r, *row)) {
                if let Some(v) = d.vals.get(i) {
                    if let Some(slot) = sliders.get_mut(idx) {
                        *slot = *v;
                    }
                }
            }
        }
        let toggles = d.pref_toggles.clone();
        let cycles = d.pref_cycles.clone();
        let (mut ti, mut ci, mut si) = (0usize, 0usize, 0usize);
        for row in PREF_ROWS {
            match row.kind {
                PrefKind::Toggle(k) => {
                    if let Some(v) = toggles.get(ti) {
                        self.prefs_set_bool(k, *v);
                    }
                    ti += 1;
                }
                PrefKind::Cycle(k, opts, _) => {
                    if let Some(v) = cycles.get(ci) {
                        self.prefs_set_cycle(k, *v % opts.len() as u8);
                    }
                    ci += 1;
                }
                PrefKind::Slider(k, _, _, _) => {
                    if let Some(v) = sliders.get(si) {
                        self.prefs_set_num(k, *v);
                    }
                    si += 1;
                }
            }
        }
        // Live effect: palette locations are saved on exit when the
        // setting is on — save them now too, so the choice sticks
        // even if the app quits without a clean shutdown path.
        if self.prefs.save_palette_locs {
            self.save_panel_layout();
        }
    }

    pub(crate) fn open_dialog(&mut self, kind: DialogKind) {
        let mut d = Dialog::new(kind.clone(), &self.doc);
        if kind == DialogKind::LegacyFilter(28) {
            d.fx_b = false; // Lighting texture channel starts off.
        }
        if kind == DialogKind::Duotone {
            // Load the document's current spec, or the era
            // default (one black ink).
            let defaults: [([u8; 3], u8); 4] = [
                ([0, 0, 0], 0),
                ([112, 88, 64], 0),
                ([48, 72, 128], 0),
                ([160, 48, 32], 0),
            ];
            if let Some(spec) = &self.doc.duotone {
                d.duo_inks = spec
                    .inks
                    .iter()
                    .map(|ink| {
                        (
                            [ink.color.0, ink.color.1, ink.color.2],
                            duo_curve_preset(&ink.curve),
                        )
                    })
                    .collect();
            }
            if d.duo_inks.is_empty() {
                d.duo_inks = vec![defaults[0]];
            }
            d.fx_page = 0;
            let ink = d.duo_inks[0];
            d.vals = vec![
                ink.0[0] as f32,
                ink.0[1] as f32,
                ink.0[2] as f32,
                ink.1 as f32,
            ];
        }
        if kind == DialogKind::Calculations {
            d.blend_idx = 1; // Multiply, the classic default.
        }
        if kind == DialogKind::SelectiveColor || kind == DialogKind::ApplyImage {
            // Era defaults: Absolute method / mask not inverted
            // (Dialog::new defaults fx_b to true).
            d.fx_b = false;
        }
        if kind == DialogKind::ReplaceColor {
            let (r, g, b, _) = self.fg.to_rgba8();
            d.rc_target = (r, g, b);
            d.fx_b = false;
        }
        if kind == DialogKind::LoadSel {
            d.load_chan = self.doc.alphas.len().saturating_sub(1);
            d.load_op = 0;
        }
        if kind == DialogKind::ChannelOptions {
            let idx = match self.channel_view {
                ChannelView::Alpha(a) => Some(a),
                _ => self.doc.alphas.len().checked_sub(1),
            };
            if let Some(idx) = idx {
                if let Some(ch) = self.doc.alphas.get(idx) {
                    d.fx_page = idx;
                    d.text = ch.name.clone();
                    d.fx_b = ch.spot_color.is_some();
                    d.fx_a = if ch.masked { 0 } else { 1 };
                    if let Some(c) = ch.spot_color {
                        d.vals = vec![c.0 as f32, c.1 as f32, c.2 as f32];
                    }
                }
            }
        }
        if kind == DialogKind::MergeChannels {
            // Default each channel's source to the split doc
            // with the matching channel name.
            let want: [&str; 4] = ["Red", "Green", "Blue", "Black"];
            for (i, wname) in want.iter().enumerate() {
                if let Some(pos) = self.split_docs.iter().position(|e| &e.0 == wname) {
                    d.merge_src[i] = pos;
                } else if i < self.split_docs.len() {
                    d.merge_src[i] = i;
                }
            }
        }
        if kind == DialogKind::FileInfo {
            d.info_fields = [
                self.doc.info_title.clone(),
                self.doc.info_caption.clone(),
                self.doc.info_keywords.clone(),
                self.doc.info_copyright.clone(),
                self.doc.info_credit.clone(),
            ];
            d.fx_page = 0;
            d.text = d.info_fields[0].clone();
            d.text_focus = true;
        }
        if kind == DialogKind::Preferences {
            d.fx_page = 0;
            self.prefs_load_draft(&mut d);
        }
        if kind == DialogKind::NewDoc {
            d.vals[2] = self.prefs.new_print_res as f32;
        }
        if kind == DialogKind::ColorTable {
            d.table_sel = 0;
            if let Some(c) = self.doc.color_table.first() {
                d.vals = vec![c.0 as f32, c.1 as f32, c.2 as f32];
            }
        }
        if kind.is_pixel_dialog() {
            let layer = self.doc.active_layer;
            let mut cmd = TileEditCommand::new(kind.title());
            if kind == DialogKind::Fade {
                // Fade's preview command covers the last
                // operation's tiles, not the selection's.
                if let Some(fs) = &self.fade_source {
                    for ch in &fs.changes {
                        cmd.capture_before(&self.doc, ch.layer, ch.coord);
                    }
                }
            } else {
                for c in self.sel_tiles() {
                    cmd.capture_before(&self.doc, layer, c);
                }
            }
            d.preview = Some(cmd);
        }
        self.dialog = Some(d);
        // Apply the default values as the first preview, so OK
        // commits them even if the user never touches a slider
        // (before this, OK on an untouched dialog was a no-op).
        self.dialog_refresh_preview();
    }

    /// Variations (Phase 6.5 Batch 4): open with a working
    /// copy of the document; thumbnail clicks adjust the
    /// copy, OK commits it as one undo step.
    pub(crate) fn open_variations(&mut self) {
        let mut d = Dialog::new(DialogKind::Variations, &self.doc);
        d.var_doc = Some(self.doc.clone());
        self.dialog = Some(d);
        self.var_rebuild_thumb();
    }

    /// Pattern Maker (Phase 7): generate the seamless tile
    /// from the selection (or the whole canvas) up front;
    /// the dialog previews it tiled and OK installs it as
    /// the document pattern.
    pub(crate) fn open_pattern_maker(&mut self) {
        let rect = match self.sel.as_ref().and_then(|s| s.bounds()) {
            Some((x, y, w, h)) => (x, y, w as i32, h as i32),
            None => (0, 0, self.doc.width as i32, self.doc.height as i32),
        };
        let tile = pictodesk_core::pattern::make_tile(&self.doc, rect, "Pattern Maker");
        match tile {
            Some(t) => {
                let mut d = Dialog::new(DialogKind::PatternMaker, &self.doc);
                d.pm_tile = Some(t);
                self.dialog = Some(d);
            }
            None => {
                self.status = "Pattern Maker needs a sample of at least 4x4 pixels".into();
            }
        }
    }

    /// The 3x3 Variations cells: (label, channel shift kind)
    /// in dialog coordinates. Kinds: 0 current, 1..=6 color
    /// pushes (R+, G+, B+, Y, C, M), 7 darker, 8 lighter.
    pub(crate) fn var_cells(&self) -> Option<Vec<(Rect, &'static str, i32)>> {
        let layout = self.dialog_layout()?;
        let r = layout.rect;
        let defs = [
            ("More Green", 2),
            ("More Yellow", 4),
            ("More Red", 1),
            ("More Magenta", 6),
            ("Current Pick", 0),
            ("More Cyan", 5),
            ("Darker", 7),
            ("More Blue", 3),
            ("Lighter", 8),
        ];
        Some(
            defs.iter()
                .enumerate()
                .map(|(i, (label, kind))| {
                    (
                        Rect::new(
                            r.x + 22 + (i as i32 % 3) * 122,
                            r.y + 36 + (i as i32 / 3) * 66,
                            88,
                            52,
                        ),
                        *label,
                        *kind,
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn var_rebuild_thumb(&mut self) {
        let Some(d) = &mut self.dialog else { return };
        let Some(doc) = &d.var_doc else { return };
        let (tw, th) = (88usize, 52usize);
        let mut thumb = Vec::with_capacity(tw * th);
        for ty in 0..th {
            for tx in 0..tw {
                let dx = ((tx as f32 / tw as f32) * doc.width as f32) as i32;
                let dy = ((ty as f32 / th as f32) * doc.height as f32) as i32;
                let (r, g, b, _) = doc.render_pixel(dx, dy).to_rgba8();
                thumb.push((r, g, b));
            }
        }
        d.var_thumb = thumb;
    }

    /// Apply one Variations click to the working copy.
    pub(crate) fn var_apply_click(&mut self, kind: i32) {
        let amt = self.dialog.as_ref().map(|d| d.vals[0]).unwrap_or(20.0) / 255.0;
        let Some(d) = &mut self.dialog else { return };
        let Some(doc) = &mut d.var_doc else { return };
        let layer = doc.active_layer;
        pictodesk_core::adjust::map_layer_pixels(doc, layer, None, |p| {
            let (r, g, b, a) = p.to_f32s();
            let (r, g, b) = match kind {
                1 => (r + amt, g, b),
                2 => (r, g + amt, b),
                3 => (r, g, b + amt),
                4 => (r, g, b - amt),
                5 => (r - amt, g, b),
                6 => (r, g - amt, b),
                7 => (r - amt, g - amt, b - amt),
                8 => (r + amt, g + amt, b + amt),
                _ => (r, g, b),
            };
            pictodesk_core::pixel::Pixel16::from_f32s(
                r.clamp(0.0, 1.0),
                g.clamp(0.0, 1.0),
                b.clamp(0.0, 1.0),
                a,
            )
        });
        self.var_rebuild_thumb();
    }

    /// Record button in the Note dialog: start capturing, or
    /// stop and attach the take to the note being edited.
    pub(crate) fn note_record_toggle(&mut self) {
        if self.audio_rec.is_some() {
            self.stop_recording(true);
            self.status = "Audio recorded".into();
        } else {
            match audio_record_start() {
                Ok(rec) => {
                    self.audio_rec = Some(rec);
                    self.status = "Recording... Stop when done".into();
                }
                Err(e) => self.status = format!("Record failed: {e}"),
            }
        }
        self.touch();
    }

    /// Stop any in-flight capture. With `keep`, the take is
    /// encoded and attached to the open Note dialog.
    pub(crate) fn stop_recording(&mut self, keep: bool) {
        if let Some(rec) = self.audio_rec.take() {
            drop(rec.stream);
            if keep {
                let wav = {
                    let g = rec.samples.lock().unwrap();
                    wav_encode(&g, rec.rate, rec.channels)
                };
                if let Some(d) = &mut self.dialog {
                    d.note_audio = Some(wav);
                }
            }
        }
    }

    /// Play button in the Note dialog: play the attached take.
    pub(crate) fn note_play_clicked(&mut self) {
        let wav = self.dialog.as_ref().and_then(|d| d.note_audio.clone());
        match wav {
            Some(w) => match audio_play(&w) {
                Ok(stream) => {
                    self.audio_out = Some(stream);
                    self.status = "Playing audio note".into();
                }
                Err(e) => self.status = format!("Play failed: {e}"),
            },
            None => self.status = "No audio on this note yet".into(),
        }
    }

    /// Play a note's audio (canvas click on its marker).
    pub(crate) fn play_note_audio(&mut self, wav: &[u8]) {
        match audio_play(wav) {
            Ok(stream) => self.audio_out = Some(stream),
            Err(e) => self.status = format!("Play failed: {e}"),
        }
    }

    /// The bytes Save for Web would write for the dialog's
    /// current settings, honoring Weighted Optimization (JPEG
    /// format + toggle on + an alpha channel to weight by).
    /// Shared by the live size readout and the OK write.
    pub(crate) fn web_export_bytes(&self, d: &Dialog) -> Option<Vec<u8>> {
        if d.web_weighted && d.web_format == 0 {
            if let Some(alpha) = self.doc.alphas.first() {
                return io::export_weighted_jpeg(
                    &self.doc,
                    d.vals[0] as u8,
                    d.vals.get(1).copied().unwrap_or(30.0) as u8,
                    &alpha.data,
                );
            }
        }
        let fmt = match d.web_format {
            1 => ExportFormat::Png,
            2 => ExportFormat::Gif,
            3 => ExportFormat::Wbmp,
            _ => ExportFormat::Jpeg,
        };
        if d.web_knockout && matches!(fmt, ExportFormat::Png | ExportFormat::Gif) {
            // Color-to-transparency: the current background
            // color knocks out to transparent, optionally
            // with Floyd-Steinberg dithered edges.
            let mut rgba = self.doc.render_full_rgba8();
            let bg = self.bg.to_rgba8();
            knockout_color(
                &mut rgba,
                self.doc.width,
                self.doc.height,
                (bg.0, bg.1, bg.2),
                d.web_dither,
            );
            return io::encode_rgba(&rgba, self.doc.width, self.doc.height, fmt, d.vals[0] as u8)
                .ok();
        }
        io::export_image(&self.doc, fmt, d.vals[0] as u8).ok()
    }

    /// Re-run the dialog's effect from the snapshot (live preview).
    /// Save for Web: recompute the encoded size for the current
    /// format + quality.
    pub(crate) fn refresh_web_size(&mut self) {
        let bytes = match &self.dialog {
            Some(d) if d.kind == DialogKind::SaveForWeb => self.web_export_bytes(d),
            _ => return,
        };
        if let Some(bytes) = bytes {
            if let Some(d) = &mut self.dialog {
                d.web_size = bytes.len();
            }
        }
    }

    /// Write the Fade blend of the stored source into the
    /// document (the dialog preview; OK commits it through
    /// the preview command's after-capture).
    pub(crate) fn fade_apply_preview(&mut self, opacity: f32, mode_idx: usize) {
        let Some(fs) = &self.fade_source else {
            return;
        };
        let mode = FADE_MODES.get(mode_idx).map(|m| m.1).unwrap_or(None);
        let changes = fs.changes.clone();
        for ch in &changes {
            let before = ch
                .before
                .clone()
                .unwrap_or_else(pictodesk_core::tile::Tile::new_transparent);
            let after = ch
                .after
                .clone()
                .unwrap_or_else(pictodesk_core::tile::Tile::new_transparent);
            let blended = blend_tile(&before, &after, mode, opacity);
            self.doc.layers[ch.layer].restore_tile(ch.coord, Some(blended));
            if let (Some(b), Some(a)) = (&ch.before_alpha, &ch.after_alpha) {
                let mixed: Vec<u16> = b
                    .iter()
                    .zip(a.iter())
                    .map(|(x, y)| (*x as f32 + (*y as f32 - *x as f32) * opacity + 0.5) as u16)
                    .collect();
                self.doc.layers[ch.layer].restore_alpha_tile(ch.coord, Some(mixed));
            }
        }
    }

    pub(crate) fn dialog_refresh_preview(&mut self) {
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
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::Blending)
        ) {
            self.blend_sync_from_vals();
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
            DialogKind::Fade => {
                let d = self.dialog.as_ref().unwrap();
                let (opacity, mode_idx) = (v[0] / 100.0, d.fx_a as usize);
                self.fade_apply_preview(opacity, mode_idx);
            }
            DialogKind::ChannelMixer => adjust::channel_mixer(&mut self.doc, layer, s, &mixer),
            DialogKind::SelectiveColor => {
                let d = self.dialog.as_ref().unwrap();
                adjust::selective_color(
                    &mut self.doc,
                    layer,
                    s,
                    d.fx_a,
                    [v[0] / 100.0, v[1] / 100.0, v[2] / 100.0, v[3] / 100.0],
                    d.fx_b,
                )
            }
            DialogKind::ReplaceColor => {
                let d = self.dialog.as_ref().unwrap();
                let t = d.rc_target;
                adjust::replace_color(
                    &mut self.doc,
                    layer,
                    s,
                    (t.0 as f32 / 255.0, t.1 as f32 / 255.0, t.2 as f32 / 255.0),
                    v[0],
                    v[1],
                    v[2],
                    v[3],
                )
            }
            DialogKind::Transform3d => {
                filter::transform_3d(&mut self.doc, layer, s, shape3d, v[0], v[1])
            }
            DialogKind::LegacyFilter(id) => {
                let d = self.dialog.as_ref().unwrap();
                let (fx_a, fx_b, fg, bg) = (d.fx_a, d.fx_b, self.fg, self.bg);
                let fx_c = d.fx_c;
                let (doc_w, doc_h) = (self.doc.width as f32, self.doc.height as f32);
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
                    16 => filter::extrude(
                        &mut self.doc,
                        layer,
                        s,
                        v[0] as u32,
                        v[1] as u32,
                        fx_a,
                        12345,
                    ),
                    17 => filter::trace_contour(&mut self.doc, layer, s, v[0] / 255.0, fx_a == 1),
                    18 => filter::color_halftone(&mut self.doc, layer, s, v[0] as u32),
                    19 => filter::crystallize(&mut self.doc, layer, s, v[0] as u32, 12345),
                    20 => filter::pointillize(&mut self.doc, layer, s, v[0] as u32, bg, 12345),
                    21 => filter::smart_blur(&mut self.doc, layer, s, v[0], v[1], fx_a == 1),
                    22 => filter::deinterlace(&mut self.doc, layer, s, fx_a == 1, fx_b),
                    23 => filter::wave(
                        &mut self.doc,
                        layer,
                        s,
                        v[0] as u32,
                        v[1],
                        v[2],
                        v[3],
                        v[4],
                        fx_a,
                        fx_b,
                        12345,
                    ),
                    24 => filter::glass(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    25 => filter::diffuse_glow(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    26 => {} // Displace applies after the map file is picked.
                    27 => filter::lens_flare(
                        &mut self.doc,
                        layer,
                        s,
                        v[0],
                        v[1] / 100.0 * doc_w,
                        v[2] / 100.0 * doc_h,
                        fx_a,
                    ),
                    52 => filter::chalk_charcoal(
                        &mut self.doc,
                        layer,
                        s,
                        v[0],
                        v[1],
                        v[2],
                        fg,
                        bg,
                        12345,
                    ),
                    53 => filter::charcoal_fx(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    54 => filter::chrome(&mut self.doc, layer, s, v[0], v[1]),
                    55 => filter::conte_crayon(
                        &mut self.doc,
                        layer,
                        s,
                        v[0],
                        v[1],
                        v[2],
                        v[3],
                        fx_a,
                        fg,
                        bg,
                    ),
                    56 => filter::graphic_pen(&mut self.doc, layer, s, v[0], v[1], fx_a, fg, bg),
                    57 => filter::halftone_pattern(&mut self.doc, layer, s, v[0], v[1], fx_a),
                    58 => filter::note_paper(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    59 => filter::photocopy(&mut self.doc, layer, s, v[0], v[1], 12345),
                    60 => filter::plaster(&mut self.doc, layer, s, v[0], v[1], fx_a),
                    61 => filter::reticulation(
                        &mut self.doc,
                        layer,
                        s,
                        v[0],
                        v[1],
                        v[2],
                        fg,
                        bg,
                        12345,
                    ),
                    62 => {
                        filter::torn_edges(&mut self.doc, layer, s, v[0], v[1], v[2], fg, bg, 12345)
                    }
                    63 => filter::water_paper(
                        &mut self.doc,
                        layer,
                        s,
                        v[0],
                        v[1],
                        v[2],
                        fg,
                        bg,
                        12345,
                    ),
                    64 => filter::craquelure(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    65 => filter::grain_fx(&mut self.doc, layer, s, v[0], v[1], fx_a, 12345),
                    66 => filter::mosaic_tiles(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    67 => filter::stained_glass(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    68 => filter::texturizer(&mut self.doc, layer, s, v[0], v[1], fx_a, fx_c),
                    44 => filter::accented_edges(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    45 => filter::angled_strokes(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    46 => filter::crosshatch(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    47 => filter::dark_strokes(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    48 => filter::ink_outlines(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    49 => filter::spatter(&mut self.doc, layer, s, v[0], v[1], 12345),
                    50 => filter::sprayed_strokes(&mut self.doc, layer, s, v[0], v[1], fx_a, 12345),
                    51 => filter::sumi_e(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    29 => filter::colored_pencil(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    30 => filter::cutout(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    31 => filter::dry_brush(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    32 => filter::film_grain(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    33 => filter::fresco(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    34 => filter::neon_glow(&mut self.doc, layer, s, v[0], v[1], fg),
                    35 => filter::paint_daubs(&mut self.doc, layer, s, v[0], v[1], fx_a, 12345),
                    36 => filter::palette_knife(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    37 => filter::plastic_wrap(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    38 => filter::poster_edges(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    39 => {
                        filter::rough_pastels(&mut self.doc, layer, s, v[0], v[1], v[2], v[3], fx_a)
                    }
                    40 => filter::smudge_stick(&mut self.doc, layer, s, v[0], v[1], v[2]),
                    41 => filter::sponge(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    42 => {
                        filter::underpainting(&mut self.doc, layer, s, v[0], v[1], v[2], v[3], fx_a)
                    }
                    43 => filter::watercolor(&mut self.doc, layer, s, v[0], v[1], v[2], 12345),
                    28 => filter::lighting(
                        &mut self.doc,
                        layer,
                        s,
                        v[0],
                        v[1],
                        v[2] / 100.0 * doc_w,
                        v[3] / 100.0 * doc_h,
                        v[4],
                        fx_a == 0,
                        fx_b,
                    ),
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
            DialogKind::GradientMap => {
                let d = self.dialog.as_ref().unwrap();
                adjust::gradient_map(&mut self.doc, layer, s, d.photo_color, d.gm_to, d.opt_a)
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

    pub(crate) fn dialog_ok(&mut self) {
        // A shortcut capture never survives the dialog closing.
        self.prefs_capture = None;
        self.prefs_confirm = None;
        let Some(mut d) = self.dialog.take() else {
            return;
        };
        let layer = self.doc.active_layer;
        // Displace: the dialog only collects the scales — the
        // filter itself runs once the map file is picked (its
        // preview arm is a deliberate no-op until then).
        if let DialogKind::LegacyFilter(26) = d.kind {
            self.pending_displace = Some((d.vals[0], d.vals[1], d.fx_a == 1, d.fx_b));
            self.file_panel_purpose = 1;
            self.status = "Choose the displacement map image".into();
            self.open_file_panel();
            return;
        }
        // Liquify: bake the displacement field into the layer
        // before the snapshot-commit below records the step.
        if d.kind == DialogKind::Liquify {
            if let Some(li) = d.adj_layer {
                let field = pictodesk_core::liquify::LiquifyField {
                    w: self.doc.width,
                    h: self.doc.height,
                    disp: d.liq_disp.clone(),
                };
                if !field.is_identity() {
                    pictodesk_core::liquify::apply_liquify(&mut self.doc, li, None, &field);
                    self.doc_version += 1;
                }
            }
        }
        // Adjustment-layer dialogs commit as one document step built
        // from the snapshot taken when the dialog opened.
        if let (Some(snap), Some(_li)) = (d.adj_snapshot.take(), d.adj_layer) {
            let created = snap.layers.len() != self.doc.layers.len();
            let is_fx = d.kind == DialogKind::Effects;
            let label = if is_fx {
                "Layer Effects"
            } else if d.kind == DialogKind::Blending {
                "Blending Options"
            } else if d.kind == DialogKind::Liquify {
                "Liquify"
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
            | DialogKind::GradientMap
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
                16 => RecordedStep::Extrude(d.vals[0] as u32, d.vals[1] as u32, d.fx_a),
                17 => RecordedStep::TraceContour(d.vals[0] / 255.0, d.fx_a == 1),
                18 => RecordedStep::ColorHalftone(d.vals[0] as u32),
                19 => RecordedStep::Crystallize(d.vals[0] as u32),
                20 => RecordedStep::Pointillize(self.bg, d.vals[0] as u32),
                21 => RecordedStep::SmartBlur(d.vals[0], d.vals[1], d.fx_a == 1),
                22 => RecordedStep::DeInterlace(d.fx_a == 1, d.fx_b),
                23 => RecordedStep::Wave(
                    d.vals[0] as u32,
                    d.vals[1],
                    d.vals[2],
                    d.vals[3],
                    d.vals[4],
                    d.fx_a,
                    d.fx_b,
                ),
                24 => RecordedStep::Glass(d.vals[0], d.vals[1], d.vals[2]),
                25 => RecordedStep::DiffuseGlow(d.vals[0], d.vals[1], d.vals[2]),
                27 => RecordedStep::LensFlare(d.vals[0], d.vals[1], d.vals[2], d.fx_a),
                52 => {
                    RecordedStep::ChalkCharcoal(d.vals[0], d.vals[1], d.vals[2], self.fg, self.bg)
                }
                53 => RecordedStep::CharcoalFx(d.vals[0], d.vals[1], d.vals[2]),
                54 => RecordedStep::Chrome(d.vals[0], d.vals[1]),
                55 => RecordedStep::ConteCrayon(
                    d.vals[0], d.vals[1], d.vals[2], d.vals[3], d.fx_a, self.fg, self.bg,
                ),
                56 => RecordedStep::GraphicPen(d.vals[0], d.vals[1], d.fx_a, self.fg, self.bg),
                57 => RecordedStep::HalftonePattern(d.vals[0], d.vals[1], d.fx_a),
                58 => RecordedStep::NotePaper(d.vals[0], d.vals[1], d.vals[2]),
                59 => RecordedStep::Photocopy(d.vals[0], d.vals[1]),
                60 => RecordedStep::Plaster(d.vals[0], d.vals[1], d.fx_a),
                61 => RecordedStep::Reticulation(d.vals[0], d.vals[1], d.vals[2], self.fg, self.bg),
                62 => RecordedStep::TornEdges(d.vals[0], d.vals[1], d.vals[2], self.fg, self.bg),
                63 => RecordedStep::WaterPaper(d.vals[0], d.vals[1], d.vals[2], self.fg, self.bg),
                64 => RecordedStep::Craquelure(d.vals[0], d.vals[1], d.vals[2]),
                65 => RecordedStep::GrainFx(d.vals[0], d.vals[1], d.fx_a),
                66 => RecordedStep::MosaicTiles(d.vals[0], d.vals[1], d.vals[2]),
                67 => RecordedStep::StainedGlass(d.vals[0], d.vals[1], d.vals[2]),
                68 => RecordedStep::Texturizer(d.vals[0], d.vals[1], d.fx_a, d.fx_c),
                44 => RecordedStep::AccentedEdges(d.vals[0], d.vals[1], d.vals[2]),
                45 => RecordedStep::AngledStrokes(d.vals[0], d.vals[1], d.vals[2]),
                46 => RecordedStep::Crosshatch(d.vals[0], d.vals[1], d.vals[2]),
                47 => RecordedStep::DarkStrokes(d.vals[0], d.vals[1], d.vals[2]),
                48 => RecordedStep::InkOutlines(d.vals[0], d.vals[1], d.vals[2]),
                49 => RecordedStep::Spatter(d.vals[0], d.vals[1]),
                50 => RecordedStep::SprayedStrokes(d.vals[0], d.vals[1], d.fx_a),
                51 => RecordedStep::SumiE(d.vals[0], d.vals[1], d.vals[2]),
                29 => RecordedStep::ColoredPencil(d.vals[0], d.vals[1], d.vals[2]),
                30 => RecordedStep::Cutout(d.vals[0], d.vals[1], d.vals[2]),
                31 => RecordedStep::DryBrush(d.vals[0], d.vals[1], d.vals[2]),
                32 => RecordedStep::FilmGrain(d.vals[0], d.vals[1], d.vals[2]),
                33 => RecordedStep::Fresco(d.vals[0], d.vals[1], d.vals[2]),
                34 => RecordedStep::NeonGlow(d.vals[0], d.vals[1], self.fg),
                35 => RecordedStep::PaintDaubs(d.vals[0], d.vals[1], d.fx_a),
                36 => RecordedStep::PaletteKnife(d.vals[0], d.vals[1], d.vals[2]),
                37 => RecordedStep::PlasticWrap(d.vals[0], d.vals[1], d.vals[2]),
                38 => RecordedStep::PosterEdges(d.vals[0], d.vals[1], d.vals[2]),
                39 => {
                    RecordedStep::RoughPastels(d.vals[0], d.vals[1], d.vals[2], d.vals[3], d.fx_a)
                }
                40 => RecordedStep::SmudgeStick(d.vals[0], d.vals[1], d.vals[2]),
                41 => RecordedStep::Sponge(d.vals[0], d.vals[1], d.vals[2]),
                42 => {
                    RecordedStep::Underpainting(d.vals[0], d.vals[1], d.vals[2], d.vals[3], d.fx_a)
                }
                43 => RecordedStep::Watercolor(d.vals[0], d.vals[1], d.vals[2]),
                28 => RecordedStep::Lighting(
                    d.vals[0],
                    d.vals[1],
                    d.vals[2],
                    d.vals[3],
                    d.vals[4],
                    d.fx_a == 0,
                    d.fx_b,
                ),
                _ => RecordedStep::Sharpen,
            }),
            _ => None,
        };
        match d.kind {
            k if k.is_pixel_dialog() => {
                if let Some(mut cmd) = d.preview.take() {
                    if k == DialogKind::Fade {
                        if let Some(fs) = &self.fade_source {
                            let coords: Vec<(usize, pictodesk_core::tile::TileCoord)> =
                                fs.changes.iter().map(|c| (c.layer, c.coord)).collect();
                            for (li, c) in coords {
                                cmd.capture_after(&self.doc, li, c);
                            }
                        }
                    } else {
                        for c in self.sel_tiles() {
                            cmd.capture_after(&self.doc, layer, c);
                        }
                    }
                    let title = k.title().to_string();
                    let changes = cmd.changes.clone();
                    self.push_cmd(Box::new(cmd));
                    if k == DialogKind::Fade {
                        // Re-fading blends from the same
                        // original: keep `before`, adopt the
                        // faded tiles as the new `after`.
                        if let Some(fs) = &mut self.fade_source {
                            for (fc, cc) in fs.changes.iter_mut().zip(changes.iter()) {
                                fc.after = cc.after.clone();
                                fc.after_alpha = cc.after_alpha.clone();
                            }
                        }
                    } else {
                        self.fade_source = Some(FadeSource {
                            label: title.clone(),
                            changes,
                        });
                    }
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
                let interp = self.prefs.interp;
                let res = d.vals[2].clamp(1.0, 1200.0);
                self.structural("Image Size", move |doc| {
                    transform::resize_image(doc, w, h, interp);
                    doc.resolution = res;
                });
                self.record_step(RecordedStep::ImageSize(w, h, interp));
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
                let (size, tracking) = (d.vals[0] * d.type_unit_k, d.vals[1]);
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
                if d.vals.len() > 10 {
                    spec.box_w = d.vals[5].max(0.0) as u32;
                    spec.space_before = d.vals[6];
                    spec.first_indent = d.vals[7];
                    spec.warp_bend = d.vals[8];
                    spec.warp_h = d.vals[9];
                    spec.warp_v = d.vals[10];
                }
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
            DialogKind::NoteEdit => {
                // A take still recording when OK lands is
                // stopped and kept.
                self.stop_recording(true);
                let text = d.text.clone();
                let audio = d.note_audio.clone();
                match d.note_target {
                    Some(i) => {
                        self.structural("Edit Note", |doc| {
                            if let Some(n) = doc.notes.get_mut(i) {
                                n.text = text.clone();
                                n.audio = audio.clone();
                            }
                        });
                        self.status = "Note updated".into();
                    }
                    None => {
                        let (x, y) = d.type_anchor;
                        self.structural("Add Note", |doc| {
                            doc.notes.push(pictodesk_core::document::Note {
                                x,
                                y,
                                color: 0,
                                text: text.clone(),
                                audio: audio.clone(),
                            });
                        });
                        self.status = "Note added".into();
                    }
                }
            }
            DialogKind::SliceProps => {
                if let Some(i) = d.slice_target {
                    let (name, url, alt) = (d.text.clone(), d.text2.clone(), d.text3.clone());
                    self.structural("Slice Options", |doc| {
                        if let Some(sl) = doc.slices.get_mut(i) {
                            if !name.trim().is_empty() {
                                sl.name = name.trim().to_string();
                            }
                            sl.url = url.clone();
                            sl.alt = alt.clone();
                        }
                    });
                    self.status = "Slice options saved".into();
                }
            }
            DialogKind::FileBrowser => {
                if let Some(entry) = d.browser_entries.get(d.browser_sel).cloned() {
                    if entry.is_dir {
                        self.open_file_browser();
                        if let Some(nd) = &mut self.dialog {
                            nd.browser_dir = entry.path.to_string_lossy().into_owned();
                        }
                        self.browser_rescan();
                    } else {
                        let rotate = entry.rotate;
                        self.open_path(&entry.path);
                        match rotate {
                            1 => self.do_action(Action::Rotate90Cw),
                            -1 => self.do_action(Action::Rotate90Ccw),
                            _ => {}
                        }
                    }
                }
            }
            DialogKind::SaveWorkspace => {
                let name = d.text.trim().to_string();
                if name.is_empty() {
                    self.status = "Workspace needs a name".into();
                } else {
                    let w = self.capture_workspace();
                    if let Some(slot) = self.workspaces.iter_mut().find(|(n, _)| *n == name) {
                        slot.1 = w;
                    } else {
                        self.workspaces.push((name.clone(), w));
                    }
                    self.save_workspaces_file();
                    self.status = format!("Workspace '{name}' saved");
                }
            }
            DialogKind::RenameItem => {
                let name = d.text.trim().to_string();
                if !name.is_empty() {
                    if d.fx_page == 0 {
                        let li = d.preset_sel.min(self.doc.layers.len().saturating_sub(1));
                        self.structural("Rename Layer", |doc| {
                            doc.layers[li].name = name.clone();
                        });
                    } else {
                        let ai = d.preset_sel.min(self.doc.alphas.len().saturating_sub(1));
                        self.structural("Rename Channel", |doc| {
                            if ai < doc.alphas.len() {
                                doc.alphas[ai].name = name.clone();
                            }
                        });
                    }
                    self.status = format!("Renamed to '{name}'");
                }
            }
            DialogKind::PresetManager => {
                // OK applies the selected preset: a style on
                // the Styles tab, a tool setup on the Tools tab.
                if self.preset_tab == 2 {
                    let sel = d.preset_sel;
                    self.apply_tool_preset(sel);
                }
                if self.preset_tab == 0 {
                    if let Some((name, fx)) = self.style_presets.get(d.preset_sel).cloned() {
                        let li = self.doc.active_layer;
                        self.structural("Apply Style Preset", |doc| {
                            doc.layers[li].effects = if fx.any_enabled() { Some(fx) } else { None };
                        });
                        self.status = format!("Style preset '{name}' applied");
                    }
                }
            }
            DialogKind::Droplet => {
                self.create_droplet(&d.text);
            }
            DialogKind::InsertStop => {
                if self.actions.is_empty() {
                    self.status = "Insert Stop: no action selected".into();
                } else {
                    let msg = if d.text.trim().is_empty() {
                        "Stopped — press Continue".to_string()
                    } else {
                        d.text.trim().to_string()
                    };
                    let ai = self.active_action.min(self.actions.len() - 1);
                    self.actions[ai].steps.push(RecordedStep::Stop(msg));
                    self.save_actions_file();
                    self.status = "Stop inserted at the end of the action".into();
                }
            }
            DialogKind::InsertMenuItem => {
                if self.actions.is_empty() {
                    self.status = "Insert Menu Item: no action selected".into();
                } else if let Some((path, _)) = d.menu_items.get(d.fx_page) {
                    let path = path.clone();
                    let ai = self.active_action.min(self.actions.len() - 1);
                    self.actions[ai]
                        .steps
                        .push(RecordedStep::MenuCommand(path.clone()));
                    self.save_actions_file();
                    self.status = format!("Menu item inserted: {path}");
                }
            }
            DialogKind::ActionStop => {
                if let Some((rest, name)) = self.play_pending.take() {
                    if !rest.is_empty() {
                        // The dialog is already taken; resuming
                        // may open the next Stop dialog.
                        self.dialog = None;
                        self.play_steps_from(rest, name);
                    } else {
                        self.status = format!("Played '{name}'");
                    }
                }
            }
            DialogKind::ConfirmClose => {
                if let Some(i) = self.pending_close.take() {
                    self.close_tab_force(i);
                }
            }
            DialogKind::ActionSetName => {
                let name = d.text.trim().to_string();
                if !name.is_empty() && !self.action_sets.contains(&name) {
                    self.action_sets.push(name.clone());
                    self.action_set = name.clone();
                    self.status = format!("Action set '{name}' created");
                    self.save_actions_file();
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
                    let interp = self.prefs.interp;
                    self.structural("Fit Image", move |doc| {
                        pictodesk_core::transform::resize_image(doc, nw, nh, interp);
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
                self.run_picture_package(
                    pw,
                    ph,
                    d.pkg_layout,
                    d.fx_a == 1 || d.fx_a == 3,
                    d.fx_a >= 2,
                );
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
                self.page_scale = d.vals[1];
                self.print_pos = (d.vals[2], d.vals[3]);
                self.print_centered = d.opt_a;
                self.print_fit = d.opt_b;
                self.print_marks = d.fx_a;
                self.print_negative = d.fx_b;
                self.print_emulsion = d.pr_emulsion;
                self.print_bilinear = d.pr_bilinear;
                self.print_profile = d.print_profile;
                self.do_print(copies);
            }
            DialogKind::PatternMaker => {
                if let Some(t) = d.pm_tile.clone() {
                    self.doc.pattern = Some(t);
                    if d.opt_a {
                        self.filter_now("Fill with Pattern", None, |doc, layer, sel| {
                            pictodesk_core::pattern::fill_layer_pattern(doc, layer, sel);
                        });
                    }
                    self.status = "Pattern Maker: tile installed as the document pattern".into();
                }
            }
            DialogKind::Variations => {
                if let Some(vd) = d.var_doc.clone() {
                    self.structural("Variations", |doc| *doc = vd);
                }
            }
            DialogKind::ApplyImage => {
                use pictodesk_core::channels as chn;
                let src_plane =
                    chn::plane(&self.doc, resolve_plane(&self.doc, d.src_layer, d.src_chan));
                let mask_plane: Option<Vec<u8>> = if d.mask_sel > 0 {
                    Some(chn::plane(
                        &self.doc,
                        chn::PlaneSource::Alpha(d.mask_sel - 1),
                    ))
                } else {
                    None
                };
                let invert = d.fx_b;
                let mode = blend_cycle_mode(d.blend_idx)
                    .unwrap_or(pictodesk_core::blend::BlendMode::Normal);
                let opacity = d.vals[0] / 100.0;
                let target = if d.target_sel == 0 {
                    None
                } else {
                    Some((d.target_sel - 1) as u8)
                };
                let layer = self.doc.active_layer;
                self.structural("Apply Image", |doc| {
                    chn::apply_image(
                        doc,
                        layer,
                        &src_plane,
                        mode,
                        opacity,
                        mask_plane.as_deref().map(|m| (m, invert)),
                        target,
                    );
                });
            }
            DialogKind::Calculations => {
                use pictodesk_core::channels as chn;
                let a = chn::plane(&self.doc, resolve_plane(&self.doc, d.src_layer, d.src_chan));
                let b = chn::plane(
                    &self.doc,
                    resolve_plane(&self.doc, d.src2_layer, d.src2_chan),
                );
                let result = chn::blend_planes_op(&a, &b, calc_op(d.blend_idx), d.vals[0] / 100.0);
                match d.result_sel {
                    1 => {
                        self.sel = Some(pictodesk_core::selection::Selection::from_mask(
                            self.doc.width,
                            self.doc.height,
                            result,
                        ));
                        self.status = "Calculation became the selection".into();
                    }
                    2 => {
                        let (w, h) = (self.doc.width, self.doc.height);
                        self.structural("Calculations", |doc| {
                            *doc = chn::merge_channels(ColorMode::Grayscale, &[&result], w, h);
                        });
                    }
                    _ => {
                        self.structural("Calculations", |doc| {
                            let mut ch = pictodesk_core::document::AlphaChannel::new(
                                "Calc Result",
                                doc.width,
                                doc.height,
                            );
                            let n = ch.data.len().min(result.len());
                            ch.data[..n].copy_from_slice(&result[..n]);
                            doc.alphas.push(ch);
                        });
                        self.show_cp = true;
                        self.cp_tab = 1;
                    }
                }
            }
            DialogKind::LoadSel => {
                if let Some(loaded) = self.doc.channel_as_selection(d.load_chan) {
                    let (w, h) = (self.doc.width, self.doc.height);
                    let mut base = self
                        .sel
                        .take()
                        .unwrap_or_else(|| pictodesk_core::selection::Selection::none(w, h));
                    match d.load_op {
                        0 => base = loaded,
                        1 => base.union_with(&loaded),
                        2 => base.subtract(&loaded),
                        _ => base.intersect(&loaded),
                    }
                    self.sel = if base.is_empty() { None } else { Some(base) };
                    self.status = "Selection loaded".into();
                }
            }
            DialogKind::ChannelOptions => {
                let idx = d.fx_page;
                let name = d.text.trim().to_string();
                let spot = if d.fx_b {
                    Some((
                        d.vals[0].round() as u8,
                        d.vals[1].round() as u8,
                        d.vals[2].round() as u8,
                    ))
                } else {
                    None
                };
                let masked = d.fx_a == 0;
                self.structural("Channel Options", |doc| {
                    if let Some(ch) = doc.alphas.get_mut(idx) {
                        if !name.is_empty() {
                            ch.name = name.clone();
                        }
                        ch.spot_color = spot;
                        ch.masked = masked;
                    }
                });
            }
            DialogKind::MergeChannels => {
                let mode = [
                    ColorMode::Rgb,
                    ColorMode::Grayscale,
                    ColorMode::Cmyk,
                    ColorMode::Lab,
                ][d.merge_mode.min(3)];
                let need = [3usize, 1, 4, 3][d.merge_mode.min(3)];
                let planes: Vec<Vec<u8>> = (0..need)
                    .map(|i| {
                        let src = &self.split_docs[d.merge_src[i].min(self.split_docs.len() - 1)].1;
                        pictodesk_core::channels::plane(
                            src,
                            pictodesk_core::channels::PlaneSource::CompositeChannel(0),
                        )
                    })
                    .collect();
                let (w, h) = {
                    let src = &self.split_docs[0].1;
                    (src.width, src.height)
                };
                let refs: Vec<&[u8]> = planes.iter().map(|p| p.as_slice()).collect();
                self.structural("Merge Channels", |doc| {
                    *doc = pictodesk_core::channels::merge_channels(mode, &refs, w, h);
                });
                self.split_docs.clear();
                self.channel_view = ChannelView::Composite;
            }
            DialogKind::BitmapConv => {
                let (method, thr) = (d.fx_a, d.vals[0] / 100.0);
                self.structural("Bitmap", |doc| {
                    if doc.mode != ColorMode::Grayscale && doc.mode != ColorMode::Bitmap {
                        pictodesk_core::mode::convert_mode(doc, ColorMode::Grayscale);
                    }
                    pictodesk_core::mode::to_bitmap(doc, method, thr);
                });
                self.channel_view = ChannelView::Composite;
            }
            DialogKind::IndexedConv => {
                let (palette, colors, dither) = (d.fx_a, d.vals[0].round() as u16, d.fx_b);
                self.structural("Indexed Color", |doc| {
                    if matches!(
                        doc.mode,
                        ColorMode::Cmyk
                            | ColorMode::Lab
                            | ColorMode::Multichannel
                            | ColorMode::Duotone
                            | ColorMode::Bitmap
                    ) {
                        pictodesk_core::mode::convert_mode(doc, ColorMode::Rgb);
                    }
                    pictodesk_core::mode::to_indexed(doc, palette, colors, dither);
                });
                self.channel_view = ChannelView::Composite;
            }
            DialogKind::Duotone => {
                // Fold the selected ink's slider edits in.
                let mut inks = d.duo_inks.clone();
                if let Some(slot) = inks.get_mut(d.fx_page) {
                    *slot = (
                        [
                            d.vals[0].round() as u8,
                            d.vals[1].round() as u8,
                            d.vals[2].round() as u8,
                        ],
                        d.vals[3].round() as u8,
                    );
                }
                let spec = pictodesk_core::document::DuotoneSpec {
                    inks: inks
                        .iter()
                        .map(|(c, preset)| pictodesk_core::document::DuotoneInk {
                            color: (c[0], c[1], c[2]),
                            curve: duo_curve_lut(*preset),
                        })
                        .collect(),
                };
                self.structural("Duotone", |doc| {
                    if doc.mode != ColorMode::Grayscale && doc.mode != ColorMode::Duotone {
                        pictodesk_core::mode::convert_mode(doc, ColorMode::Grayscale);
                    }
                    pictodesk_core::mode::to_duotone(doc, spec.clone());
                });
                self.channel_view = ChannelView::Composite;
            }
            DialogKind::ColorTable => {
                let sel = d.table_sel;
                let newc = (
                    d.vals[0].round() as u8,
                    d.vals[1].round() as u8,
                    d.vals[2].round() as u8,
                );
                if sel < self.doc.color_table.len() {
                    let oldc = self.doc.color_table[sel];
                    if oldc != newc {
                        self.structural("Color Table", |doc| {
                            // Remap every pixel using the old
                            // entry to the new color.
                            for li in 0..doc.layers.len() {
                                let coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
                                for coord in coords {
                                    let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
                                    for px in tile.pixels.iter_mut() {
                                        let (r, g, b, a) = px.to_rgba8();
                                        if (r, g, b) == oldc && a > 0 {
                                            *px = pictodesk_core::pixel::Pixel16::from_rgba8(
                                                newc.0, newc.1, newc.2, a,
                                            );
                                        }
                                    }
                                }
                            }
                            if sel < doc.color_table.len() {
                                doc.color_table[sel] = newc;
                            }
                        });
                    }
                }
            }
            DialogKind::SaveForWeb => {
                let ext = match d.web_format {
                    1 => "png",
                    2 => "gif",
                    3 => "wbmp",
                    _ => "jpg",
                };
                let name = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "untitled".into());
                let path = PathBuf::from(format!("{name}-web.{}", self.ext_case(ext)));
                match self.web_export_bytes(&d) {
                    Some(bytes) => {
                        let n = bytes.len();
                        match std::fs::write(&path, bytes) {
                            Ok(()) => {
                                self.status = format!("Saved {} ({n} bytes)", path.display());
                                self.beep_done();
                            }
                            Err(e) => self.status = format!("Save for Web failed: {e}"),
                        }
                    }
                    None => self.status = "Save for Web failed: encode error".into(),
                }
            }
            DialogKind::Extract => {
                let layer = self.doc.active_layer;
                let tiles = self.active_tiles_all();
                let (hi, fill) = (d.extract_hi.clone(), d.extract_fill.clone());
                let touch = d.extract_touch.clone();
                let orig = self.doc.layers[layer].clone();
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Extract",
                    layer,
                    &tiles,
                    |doc| {
                        filter::extract(doc, layer, &hi, &fill);
                        // Edge Touchup: paint the original
                        // pixels back where marked.
                        if touch.iter().any(|v| *v > 0) {
                            let (w, h) = (doc.width as i32, doc.height as i32);
                            for y in 0..h {
                                for x in 0..w {
                                    if touch[(y * w + x) as usize] > 0 {
                                        let p = orig.get_pixel(x, y);
                                        doc.layers[layer].set_pixel(x, y, p);
                                    }
                                }
                            }
                        }
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
                self.default_space = [
                    pictodesk_core::color::WorkingSpace::Srgb,
                    pictodesk_core::color::WorkingSpace::AdobeRgb,
                    pictodesk_core::color::WorkingSpace::ProPhoto,
                ][d.cs_default.min(2)];
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
                doc.resolution = d.vals[2].clamp(1.0, 1200.0);
                doc.working_space = self.default_space;
                doc.layers[0].fill_rect(0, 0, w, h, self.bg);
                doc.log_event("Created");
                let name = d.text.trim().to_string();
                let name = if name.is_empty() {
                    "Untitled".into()
                } else {
                    name
                };
                self.new_tab_with_doc(doc, None, name);
                self.status = "New image".into();
            }
            DialogKind::SaveAs => {
                let path = PathBuf::from(d.text.trim());
                match d.save_mode {
                    0 => {
                        self.save_to(&path);
                    }
                    1 => self.save_copy_to(&path),
                    2 => match write_aco(&path, &self.palette) {
                        Ok(()) => {
                            self.status = format!("Saved {} swatches (.ACO)", self.palette.len())
                        }
                        Err(e) => self.status = format!("Swatch save failed: {e}"),
                    },
                    3 => match write_abr(&path, &self.brush_presets) {
                        Ok(()) => {
                            self.status =
                                format!("Saved {} brush presets (.ABR)", self.brush_presets.len())
                        }
                        Err(e) => self.status = format!("Brush save failed: {e}"),
                    },
                    4 => match self.settings_step.take().and_then(|st| step_to_line(&st)) {
                        Some(line) => match std::fs::write(&path, format!("{line}\n")) {
                            Ok(()) => self.status = "Settings saved".into(),
                            Err(e) => self.status = format!("Settings save failed: {e}"),
                        },
                        None => self.status = "No settings to save".into(),
                    },
                    6 => {
                        match std::fs::write(&path, &self.mesh_bytes) {
                            Ok(()) => self.status = "Mesh saved".into(),
                            Err(e) => self.status = format!("Mesh save failed: {e}"),
                        }
                        if let Some(back) = self.liq_return.take() {
                            self.dialog = Some(back);
                        }
                    }
                    5 => {
                        let text = paths_to_ai(&self.doc);
                        match std::fs::write(&path, text) {
                            Ok(()) => self.status = "Paths exported (.ai)".into(),
                            Err(e) => self.status = format!("Export failed: {e}"),
                        }
                    }
                    _ => {
                        self.save_to(&path);
                    }
                }
            }
            DialogKind::StrokeSel => {
                let color = if d.fx_b { self.fg } else { self.bg };
                let (width, opacity, pos) = (
                    d.vals[0].round().max(1.0) as u32,
                    (d.vals[1] / 100.0).clamp(0.0, 1.0),
                    d.fx_a,
                );
                self.filter_now("Stroke", None, move |doc, layer, sel| {
                    if let Some(sel) = sel {
                        adjust::stroke_selection(doc, layer, sel, width, color, pos, opacity);
                    }
                });
            }
            DialogKind::RotateArbitrary => {
                let eff = if d.fx_a == 0 { d.vals[0] } else { -d.vals[0] };
                let old_sel = self.sel.clone();
                self.structural("Rotate Canvas", |doc| {
                    transform::rotate_arbitrary(doc, eff, None);
                });
                self.sel = old_sel.and_then(|s| {
                    let r = transform::rotate_selection(&s, eff);
                    if r.is_empty() {
                        None
                    } else {
                        Some(r)
                    }
                });
                self.channel_view = ChannelView::Composite;
                self.fit_view(self.win_size.0, self.win_size.1);
            }
            DialogKind::FileInfo => {
                let mut fields = d.info_fields.clone();
                fields[d.fx_page.min(4)] = d.text.trim().to_string();
                self.doc.info_title = fields[0].clone();
                self.doc.info_caption = fields[1].clone();
                self.doc.info_keywords = fields[2].clone();
                self.doc.info_copyright = fields[3].clone();
                self.doc.info_credit = fields[4].clone();
                self.doc.log_event("File Info updated");
                self.status = "File Info updated".into();
                self.touch();
            }
            DialogKind::Preferences => {
                self.prefs_apply_draft(&d);
                self.save_prefs_file();
                self.status = "Preferences updated".into();
            }
            DialogKind::Defringe => {
                let width = d.vals[0].round().max(1.0) as u32;
                self.filter_now("Defringe", None, move |doc, layer, _| {
                    adjust::defringe(doc, layer, width);
                });
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
                let neutral = d.fx_b;
                self.structural("New Layer", |doc| {
                    doc.add_layer(name.clone());
                    if neutral {
                        let (w, h) = (doc.width, doc.height);
                        let top = doc.layers.len() - 1;
                        doc.layers[top].fill_rect(
                            0,
                            0,
                            w,
                            h,
                            Pixel16::from_rgba8(128, 128, 128, 255),
                        );
                    }
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

    pub(crate) fn dialog_cancel(&mut self) {
        self.prefs_capture = None;
        self.prefs_confirm = None;
        // A capture in flight is abandoned, not attached.
        self.stop_recording(false);
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::ConfirmClose)
        ) {
            self.pending_close = None;
        }
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::ActionStop)
        ) {
            self.play_pending = None;
            self.status = "Playback stopped".into();
        }
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::FtNumeric)
        ) {
            self.dialog = None;
            self.ft_cancel();
            return;
        }
        if let Some(mut d) = self.dialog.take() {
            if d.kind == DialogKind::SaveAs && d.save_mode == 6 {
                if let Some(back) = self.liq_return.take() {
                    self.dialog = Some(back);
                    return;
                }
            }
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

    pub(crate) fn dialog_layout(&self) -> Option<DialogLayout> {
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
                | DialogKind::PageSetup
        ) {
            h += 22; // choice buttons
        }
        if d.kind == DialogKind::Print {
            h += 250; // page preview + three button rows
        }
        if d.kind == DialogKind::InsertMenuItem {
            h += 232; // 14 menu rows + paging buttons
        }
        if d.kind == DialogKind::ActionStop {
            h += 22; // the stop message line
        }
        if d.kind == DialogKind::SaveForWeb {
            h += 36; // second button row + encoded-size line
        }
        if matches!(
            d.kind,
            DialogKind::BitmapConv | DialogKind::IndexedConv | DialogKind::Duotone
        ) {
            h += 22; // option buttons
        }
        if d.kind == DialogKind::Duotone {
            h += 40; // ink swatches + ramp preview
        }
        if d.kind == DialogKind::ColorTable {
            // Swatch grid: 16 entries per row.
            let rows = (self.doc.color_table.len() as i32 + 15) / 16;
            h += rows * 23 + 6;
        }
        if matches!(
            d.kind,
            DialogKind::ApplyImage
                | DialogKind::Calculations
                | DialogKind::MergeChannels
                | DialogKind::LoadSel
        ) {
            h += 76; // two captioned cycle-button rows
        }
        if d.kind == DialogKind::ChannelOptions {
            h += 22; // spot / indicates buttons
        }
        if matches!(d.kind, DialogKind::Fade | DialogKind::RotateArbitrary) {
            h += 22; // mode / direction buttons
        }
        if d.kind == DialogKind::StrokeSel {
            h += 44; // two button rows
        }
        if d.kind == DialogKind::Preferences {
            // Header row + one row per control (settings pages),
            // or header + the shortcut list (Shortcuts page).
            h += if d.fx_page == PREF_PAGES.len() - 1 {
                22 + 210
            } else {
                22 + crate::prefs::prefs_page_buttons(d.fx_page).len() as i32 * 22
            };
        }
        if d.kind == DialogKind::FileInfo {
            h += 46; // text field + field tabs
        }
        if d.kind == DialogKind::NewLayer {
            h += 22; // neutral fill toggle
        }
        if d.kind == DialogKind::ColorSettings {
            h += 44; // second working-space row + captions
        }
        if d.kind == DialogKind::SelectiveColor {
            h += 44; // range button rows
        }
        if d.kind == DialogKind::ReplaceColor {
            h += 22; // target row
        }
        if d.kind == DialogKind::Variations {
            h += 210; // 3x3 thumbnail grid + labels
        }
        if d.kind == DialogKind::PatternMaker {
            h += 196; // tiled preview + fill toggle
        }
        if d.kind == DialogKind::NoteEdit {
            h += 44; // audio buttons + attached-audio line
        }
        if let DialogKind::LegacyFilter(id) = d.kind {
            if id == 6
                || id == 15
                || matches!(
                    id,
                    16 | 17
                        | 21
                        | 22
                        | 23
                        | 26
                        | 27
                        | 28
                        | 35
                        | 39
                        | 42
                        | 50
                        | 55
                        | 56
                        | 57
                        | 60
                        | 65
                        | 68
                )
            {
                h += 22; // style / method / type buttons
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
                | DialogKind::NoteEdit
                | DialogKind::SliceProps
                | DialogKind::PresetManager
                | DialogKind::Droplet
                | DialogKind::ChannelOptions
        ) {
            h += 24; // text field
        }
        if d.kind == DialogKind::Type {
            h += 150; // font lists + two format button rows
        }
        if d.kind == DialogKind::Effects {
            h += 68; // two tab rows + toggle row
            if d.fx_page == 4 {
                h += 44; // bevel mode row + texture row
            }
        }
        if d.kind == DialogKind::Blending {
            h += 100; // toggle button rows + note
        }
        if d.kind == DialogKind::GradientMap {
            h += 46; // swatch/button row + note
        }
        if d.kind == DialogKind::PresetManager {
            h = 46 + 15 * 13 + 64;
        }
        if d.kind == DialogKind::SliceProps {
            h += 48; // two extra text fields
        }
        if d.kind == DialogKind::Extract {
            h = 46 + 322 + 40; // mini-editor canvas + two button rows + OK/Cancel
        }
        if d.kind == DialogKind::Liquify {
            h = 46 + 366 + 84; // editor + tool rows + sliders + OK/Cancel
        }
        if d.kind == DialogKind::FileBrowser {
            h = 46 + 336; // text row + thumb grid + info + buttons
        }
        if d.kind == DialogKind::SpellCheck {
            h = 46 + 150;
        }
        if d.kind == DialogKind::FindReplace {
            h = 46 + 132;
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
        if let DialogKind::LegacyFilter(id) = d.kind {
            if id == 6
                || id == 15
                || matches!(
                    id,
                    16 | 17
                        | 21
                        | 22
                        | 23
                        | 26
                        | 27
                        | 28
                        | 35
                        | 39
                        | 42
                        | 50
                        | 55
                        | 56
                        | 57
                        | 60
                        | 65
                        | 68
                )
            {
                y += 22; // option buttons sit above the sliders
            }
        }
        if d.kind == DialogKind::Transform3d {
            y += 196; // wireframe preview sits above the sliders
        }
        if matches!(
            d.kind,
            DialogKind::BitmapConv | DialogKind::IndexedConv | DialogKind::Duotone
        ) {
            y += 22; // option buttons sit above the sliders
        }
        if d.kind == DialogKind::Duotone {
            y += 40; // swatch row + ramp preview above the sliders
        }
        if d.kind == DialogKind::ColorTable {
            let rows = (self.doc.color_table.len() as i32 + 15) / 16;
            y += rows * 23 + 6; // swatch grid above the sliders
        }
        if matches!(
            d.kind,
            DialogKind::ApplyImage | DialogKind::Calculations | DialogKind::MergeChannels
        ) {
            y += 76; // two captioned cycle rows above the sliders
        }
        if d.kind == DialogKind::ChannelOptions {
            y += 46; // text field + buttons above the sliders
        }
        if matches!(d.kind, DialogKind::Fade | DialogKind::RotateArbitrary) {
            y += 22;
        }
        if d.kind == DialogKind::StrokeSel {
            y += 44;
        }
        if d.kind == DialogKind::Preferences {
            y += if d.fx_page == PREF_PAGES.len() - 1 {
                22
            } else {
                22 + crate::prefs::prefs_page_buttons(d.fx_page).len() as i32 * 22
            };
        }
        if d.kind == DialogKind::SelectiveColor {
            y += 44;
        }
        if d.kind == DialogKind::ReplaceColor {
            y += 22;
        }
        if d.kind == DialogKind::Variations {
            y += 210;
        }
        if d.kind == DialogKind::Effects {
            y += 22; // second tab row
        }
        if d.kind == DialogKind::GradientMap {
            y += 22;
        }
        if d.kind == DialogKind::Liquify {
            y += 406; // editor canvas + tool rows sit above the sliders
        }
        if matches!(
            d.kind,
            DialogKind::CondMode
                | DialogKind::PicturePackage
                | DialogKind::SaveForWeb
                | DialogKind::PageSetup
        ) {
            y += 22;
        }
        if d.kind == DialogKind::SaveForWeb {
            y += 22; // second button row
        }
        if d.kind == DialogKind::Print {
            y += 250;
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
                | DialogKind::NoteEdit
                | DialogKind::SliceProps
                | DialogKind::PresetManager
                | DialogKind::Droplet
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
                6 => 3,                 // ZigZag styles
                15 => 2,                // Wind method + direction
                16 | 17 | 21 | 22 => 2, // type / edge / mode / field+method
                23 | 27 => 3,           // wave shape / lens type
                26 | 28 => 2,           // fit+undefined / light type+texture
                35 => 3,                // daub brush type
                39 | 42 => 1,           // texture kind (cycles)
                50 => 2,                // spray direction
                55 | 56 | 60 | 65 => 1, // cycling option (texture/dir/light/kind)
                57 => 3,                // halftone pattern type
                68 => 2,                // texture kind + light direction
                _ => 0,
            };
            (0..n)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect()
        } else if d.kind == DialogKind::Transform3d {
            (0..3)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect::<Vec<Rect>>()
        } else if matches!(
            d.kind,
            DialogKind::ApplyImage | DialogKind::Calculations | DialogKind::MergeChannels
        ) {
            // Two captioned rows of three cycle buttons.
            (0..6)
                .map(|i| {
                    Rect::new(
                        rect.x + 12 + (i % 3) * 128,
                        rect.y + 40 + (i / 3) * 34,
                        124,
                        18,
                    )
                })
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::LoadSel {
            let mut v = vec![Rect::new(rect.x + 12, rect.y + 40, 376, 18)];
            for i in 0..4 {
                v.push(Rect::new(rect.x + 12 + i * 94, rect.y + 74, 92, 18));
            }
            v
        } else if d.kind == DialogKind::ChannelOptions {
            vec![
                Rect::new(rect.x + 12, rect.y + 52, 124, 18),
                Rect::new(rect.x + 140, rect.y + 52, 188, 18),
            ]
        } else if d.kind == DialogKind::Fade {
            vec![Rect::new(rect.x + 12, rect.y + 30, 200, 18)]
        } else if d.kind == DialogKind::StrokeSel {
            let mut v: Vec<Rect> = (0..3)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect();
            v.push(Rect::new(rect.x + 12, rect.y + 52, 200, 18));
            v
        } else if d.kind == DialogKind::RotateArbitrary {
            (0..2)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::Preferences {
            // Prev / Next header, then one full-width row rect per
            // button control on the page (the Shortcuts list is
            // drawn + clicked by the custom Preferences block).
            let mut v = vec![
                Rect::new(rect.x + 12, rect.y + 30, 60, 18),
                Rect::new(rect.right() - 72, rect.y + 30, 60, 18),
            ];
            if d.fx_page != PREF_PAGES.len() - 1 {
                let n = crate::prefs::prefs_page_buttons(d.fx_page).len();
                for i in 0..n {
                    v.push(Rect::new(
                        rect.x + 12,
                        rect.y + 52 + i as i32 * 22,
                        rect.w - 24,
                        18,
                    ));
                }
            }
            v
        } else if d.kind == DialogKind::FileInfo {
            (0..5)
                .map(|i| Rect::new(rect.x + 12 + i * 75, rect.y + 52, 73, 18))
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::NewLayer {
            vec![Rect::new(rect.x + 12, rect.y + 52, 220, 18)]
        } else if d.kind == DialogKind::SelectiveColor {
            // Nine range buttons (5 + 4) + the Relative /
            // Absolute toggle.
            (0..10)
                .map(|i| {
                    Rect::new(
                        rect.x + 12 + (i % 5) * 75,
                        rect.y + 30 + (i / 5) * 22,
                        73,
                        18,
                    )
                })
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::ReplaceColor {
            vec![Rect::new(rect.x + 12, rect.y + 30, 124, 18)]
        } else if d.kind == DialogKind::BitmapConv {
            (0..3)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::IndexedConv {
            (0..4)
                .map(|i| Rect::new(rect.x + 12 + i * 96, rect.y + 30, 92, 18))
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::Duotone {
            // Ink-count buttons.
            (0..4)
                .map(|i| Rect::new(rect.x + 12 + i * 96, rect.y + 30, 92, 18))
                .collect::<Vec<Rect>>()
        } else if d.kind == DialogKind::Effects {
            // The ten effect pages as tabs, two rows of five.
            (0..10)
                .map(|i| {
                    Rect::new(
                        rect.x + 12 + (i % 5) * 76,
                        rect.y + 30 + (i / 5) * 22,
                        72,
                        18,
                    )
                })
                .collect()
        } else if d.kind == DialogKind::Blending {
            // Six toggles under the sliders, three rows of two.
            (0..6)
                .map(|i| {
                    Rect::new(
                        rect.x + 12 + (i % 2) * 190,
                        rect.y + 30 + 22 + sliders * 20 + 8 + (i / 2) * 22,
                        186,
                        18,
                    )
                })
                .collect()
        } else if d.kind == DialogKind::GradientMap {
            (0..3)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                .collect()
        } else if d.kind == DialogKind::Liquify {
            // Tool buttons: four rows under the editor canvas.
            (0..19)
                .map(|i| {
                    Rect::new(
                        rect.x + 12 + (i % 5) * 76,
                        rect.y + 30 + 282 + (i / 5) * 22,
                        72,
                        18,
                    )
                })
                .collect()
        } else if d.kind == DialogKind::PhotoFilter {
            (0..6)
                .map(|i| Rect::new(rect.x + 12 + i * 62, rect.y + 30, 56, 18))
                .collect()
        } else if d.kind == DialogKind::Print {
            // Button 0 (color handling) sits beside the page
            // preview; buttons 1..=9 form three rows below it.
            let mut v = vec![Rect::new(rect.x + 176, rect.y + 30, 200, 18)];
            for i in 0..9 {
                v.push(Rect::new(
                    rect.x + 12 + (i % 3) * 128,
                    rect.y + 150 + (i / 3) * 34,
                    124,
                    18,
                ));
            }
            v
        } else if d.kind == DialogKind::PicturePackage {
            (0..4)
                .map(|i| Rect::new(rect.x + 12 + i * 96, rect.y + 30, 92, 18))
                .collect()
        } else if d.kind == DialogKind::FindReplace {
            (0..3)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 104, 124, 18))
                .collect()
        } else if matches!(
            d.kind,
            DialogKind::CondMode | DialogKind::SaveForWeb | DialogKind::PageSetup
        ) {
            let n = match d.kind {
                DialogKind::CondMode => 2,
                DialogKind::SaveForWeb => 7,
                _ => 1,
            };
            if d.kind == DialogKind::SaveForWeb {
                // Seven buttons in two rows: four formats,
                // then Weighted / Knockout / Dither toggles.
                (0..n)
                    .map(|i| {
                        Rect::new(
                            rect.x + 12 + (i % 4) * 96,
                            rect.y + 30 + (i / 4) * 22,
                            92,
                            18,
                        )
                    })
                    .collect()
            } else {
                (0..n)
                    .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 30, 124, 18))
                    .collect()
            }
        } else if d.kind == DialogKind::HueSaturation {
            // Master + the six hue ranges.
            (0..7)
                .map(|i| Rect::new(rect.x + 12 + i * 54, rect.y + 30, 52, 18))
                .collect()
        } else if d.kind == DialogKind::ColorSettings {
            // Two rows of three: document space, then the
            // new-document default space.
            (0..6)
                .map(|i| {
                    Rect::new(
                        rect.x + 12 + (i % 3) * 100,
                        rect.y + 32 + (i / 3) * 34,
                        96,
                        18,
                    )
                })
                .collect()
        } else if d.kind == DialogKind::ColorBalance
            || d.kind == DialogKind::Fill
            || d.kind == DialogKind::HistoryOptions
            || d.kind == DialogKind::ChannelMixer
        {
            let n = if d.kind == DialogKind::ColorBalance || d.kind == DialogKind::Fill {
                3
            } else if d.kind == DialogKind::HistoryOptions {
                2
            } else {
                4 // ChannelMixer output rows
            };
            (0..n)
                .map(|i| Rect::new(rect.x + 12 + i * 100, rect.y + 30, 96, 18))
                .collect()
        } else if d.kind == DialogKind::NoteEdit {
            // Audio annotation buttons under the text field.
            (0..2)
                .map(|i| Rect::new(rect.x + 12 + i * 128, rect.y + 52, 124, 18))
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
                | DialogKind::NoteEdit
                | DialogKind::SliceProps
                | DialogKind::PresetManager
                | DialogKind::Droplet
                | DialogKind::ChannelOptions
                | DialogKind::FileInfo
                | DialogKind::InsertStop
                | DialogKind::SaveWorkspace
                | DialogKind::RenameItem
                | DialogKind::FileBrowser
                | DialogKind::SpellCheck
                | DialogKind::FindReplace
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
            for i in 0..3 {
                btns.push(Rect::new(rect.x + 12 + i * 96, by + 22, 92, 18));
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
            for i in 0..6 {
                btns.push(Rect::new(rect.x + 12 + i * 64, by + 22, 60, 18));
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
                btns.push(Rect::new(rect.x + 12, by + 44, 92, 18));
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

    /// The Save-As dialog in one of its library modes
    /// (Phase 6.5 Batch 6d): 2 swatches, 3 brushes,
    /// 4 adjustment settings, 5 paths export.
    pub(crate) fn open_save_dialog(&mut self, mode: u8, default_name: &str) {
        let mut d = Dialog::new(DialogKind::SaveAs, &self.doc);
        d.save_mode = mode;
        d.text = if default_name.contains('/') {
            default_name.into()
        } else if let Some((stem, ext)) = default_name.rsplit_once('.') {
            format!("{stem}.{}", self.ext_case(ext))
        } else {
            default_name.into()
        };
        d.text_focus = true;
        self.dialog = Some(d);
    }

    pub(crate) fn open_insert_stop(&mut self) {
        if self.actions.is_empty() {
            self.status = "Insert Stop: create an action first".into();
            return;
        }
        let mut d = Dialog::new(DialogKind::InsertStop, &self.doc);
        d.text = "Stopped — press Continue".into();
        d.text_focus = true;
        self.dialog = Some(d);
    }

    pub(crate) fn open_insert_menu(&mut self) {
        if self.actions.is_empty() {
            self.status = "Insert Menu Item: create an action first".into();
            return;
        }
        let mut d = Dialog::new(DialogKind::InsertMenuItem, &self.doc);
        d.menu_items = self.flatten_menu_items();
        self.dialog = Some(d);
    }

    pub(crate) fn draw_dialog(&mut self, f: &mut Frame) {
        self.ensure_render_cache();
        let layout = self.dialog_layout().unwrap();
        let d = self.dialog.as_ref().unwrap();
        let r = layout.rect;
        f.panel_rounded(r, 6);
        f.fill_rounded(
            Rect::new(r.x + 2, r.y + 2, r.w - 4, 18),
            4,
            ui::PLATINUM_LIGHT,
        );
        f.text(
            r.x + 8,
            r.y + 7,
            if d.kind == DialogKind::SaveAs && d.save_mode > 0 {
                [
                    "",
                    "Save a Copy",
                    "Save Swatches (.ACO)",
                    "Save Brushes (.ABR)",
                    "Save Settings",
                    "Export Paths (.ai)",
                    "Save Mesh",
                ][(d.save_mode as usize).min(6)]
            } else {
                d.kind.title()
            },
            1,
            ui::WHITE,
        );
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
        if d.kind == DialogKind::BitmapConv {
            let names = ["Threshold", "Pattern", "Diffusion"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.fx_a == i as u8, true);
            }
        }
        if d.kind == DialogKind::IndexedConv {
            let names = ["Adaptive", "Web", "Grayscale"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                if i < 3 {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                } else {
                    f.button(
                        *br,
                        if d.fx_b { "Dither On" } else { "Dither Off" },
                        d.fx_b,
                        true,
                    );
                }
            }
        }
        if d.kind == DialogKind::Duotone {
            let names = ["1 Ink", "2 Inks", "3 Inks", "4 Inks"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.duo_inks.len() == i + 1, true);
            }
            // Ink swatches for the current spec.
            for i in 0..4 {
                let sr = duo_swatch_rect(layout.rect, i);
                if i < d.duo_inks.len() {
                    let c = d.duo_inks[i].0;
                    f.fill_rect(sr, (c[0], c[1], c[2]));
                    f.outline_rect(sr, ui::SHADOW);
                    if d.fx_page == i {
                        f.outline_rect(
                            Rect::new(sr.x - 1, sr.y - 1, sr.w + 2, sr.h + 2),
                            ui::BLACK,
                        );
                    }
                } else {
                    f.outline_rect(sr, ui::SHADOW);
                }
            }
            // Ramp preview: gray -> duotone with the live spec
            // (the selected ink's slider edits are folded in).
            let mut inks: Vec<([u8; 3], u8)> = d.duo_inks.clone();
            if let Some(slot) = inks.get_mut(d.fx_page) {
                *slot = (
                    [
                        d.vals[0].round() as u8,
                        d.vals[1].round() as u8,
                        d.vals[2].round() as u8,
                    ],
                    d.vals[3].round() as u8,
                );
            }
            let ramp = Rect::new(layout.rect.x + 12, layout.rect.y + 74, 376, 12);
            for px in 0..ramp.w {
                let gray = px as f32 / ramp.w as f32;
                let dens = ((1.0 - gray) * 255.0).round() as usize;
                let mut out = (1.0f32, 1.0f32, 1.0f32);
                for (color, preset) in &inks {
                    let lut = duo_curve_lut(*preset);
                    let amt = lut[dens] as f32 / 255.0;
                    out.0 *= 1.0 - amt * (1.0 - color[0] as f32 / 255.0);
                    out.1 *= 1.0 - amt * (1.0 - color[1] as f32 / 255.0);
                    out.2 *= 1.0 - amt * (1.0 - color[2] as f32 / 255.0);
                }
                let c = (
                    (out.0 * 255.0) as u8,
                    (out.1 * 255.0) as u8,
                    (out.2 * 255.0) as u8,
                );
                f.fill_rect(Rect::new(ramp.x + px, ramp.y, 1, ramp.h), c);
            }
            f.outline_rect(ramp, ui::SHADOW);
        }
        if matches!(
            d.kind,
            DialogKind::ApplyImage | DialogKind::Calculations | DialogKind::MergeChannels
        ) {
            let chans = chan_cycle(&self.doc);
            let chan_name = |idx: usize| {
                chans
                    .get(idx)
                    .map(|e| e.0.clone())
                    .unwrap_or_else(|| "Gray".into())
            };
            let (caps, labels): ([&str; 6], Vec<String>) = match d.kind {
                DialogKind::ApplyImage => (
                    [
                        "Layer",
                        "Channel",
                        "Blending",
                        "Mask",
                        "Target",
                        "Invert Mask",
                    ],
                    vec![
                        layer_cycle_name(&self.doc, d.src_layer),
                        chan_name(d.src_chan),
                        BLEND_CYCLE[d.blend_idx.min(11)].to_string(),
                        if d.mask_sel == 0 {
                            "None".into()
                        } else {
                            self.doc
                                .alphas
                                .get(d.mask_sel - 1)
                                .map(|c| c.name.clone())
                                .unwrap_or_else(|| "None".into())
                        },
                        if d.target_sel == 0 {
                            "Image".into()
                        } else {
                            color_chan_names(&self.doc)
                                .get(d.target_sel - 1)
                                .cloned()
                                .unwrap_or_else(|| "Image".into())
                        },
                        if d.fx_b { "On".into() } else { "Off".into() },
                    ],
                ),
                DialogKind::Calculations => (
                    [
                        "Layer 1",
                        "Channel 1",
                        "Blending",
                        "Layer 2",
                        "Channel 2",
                        "Result",
                    ],
                    vec![
                        layer_cycle_name(&self.doc, d.src_layer),
                        chan_name(d.src_chan),
                        BLEND_CYCLE[d.blend_idx.min(13)].to_string(),
                        layer_cycle_name(&self.doc, d.src2_layer),
                        chan_name(d.src2_chan),
                        ["New Channel", "Selection", "New Document"][d.result_sel.min(2)]
                            .to_string(),
                    ],
                ),
                _ => (
                    ["Mode", "Source 1", "Source 2", "Source 3", "Source 4", ""],
                    vec![
                        ["RGB", "Grayscale", "CMYK", "Lab"][d.merge_mode.min(3)].to_string(),
                        self.split_docs
                            .get(d.merge_src[0])
                            .map(|e| e.0.clone())
                            .unwrap_or_default(),
                        self.split_docs
                            .get(d.merge_src[1])
                            .map(|e| e.0.clone())
                            .unwrap_or_default(),
                        self.split_docs
                            .get(d.merge_src[2])
                            .map(|e| e.0.clone())
                            .unwrap_or_default(),
                        self.split_docs
                            .get(d.merge_src[3])
                            .map(|e| e.0.clone())
                            .unwrap_or_default(),
                        String::new(),
                    ],
                ),
            };
            let need = match d.kind {
                DialogKind::MergeChannels => [1, 3, 4, 3][d.merge_mode.min(3)],
                _ => 0,
            };
            for (i, br) in layout.range_buttons.iter().enumerate() {
                if !caps[i].is_empty() {
                    f.text(br.x, br.y - 10, caps[i], 1, ui::DARK);
                }
                if labels[i].is_empty() {
                    continue;
                }
                let enabled = d.kind != DialogKind::MergeChannels || i == 0 || i <= need;
                f.button(*br, &labels[i], false, enabled);
            }
        }
        if d.kind == DialogKind::Variations {
            if let Some(cells) = self.var_cells() {
                let amt = d.vals[0] as i32;
                for (cell, label, kind) in cells {
                    for ty in 0..52i32 {
                        for tx in 0..88i32 {
                            let base = d
                                .var_thumb
                                .get((ty * 88 + tx) as usize)
                                .copied()
                                .unwrap_or((0, 0, 0));
                            let (r, g, b) = base;
                            let cl = |v: i32| v.clamp(0, 255) as u8;
                            let px = match kind {
                                1 => (cl(r as i32 + amt), g, b),
                                2 => (r, cl(g as i32 + amt), b),
                                3 => (r, g, cl(b as i32 + amt)),
                                4 => (r, g, cl(b as i32 - amt)),
                                5 => (cl(r as i32 - amt), g, b),
                                6 => (r, cl(g as i32 - amt), b),
                                7 => (cl(r as i32 - amt), cl(g as i32 - amt), cl(b as i32 - amt)),
                                8 => (cl(r as i32 + amt), cl(g as i32 + amt), cl(b as i32 + amt)),
                                _ => base,
                            };
                            f.set(cell.x + tx, cell.y + ty, px);
                        }
                    }
                    f.outline_rect(cell, if kind == 0 { ui::ACCENT } else { ui::SHADOW });
                    f.text(cell.x, cell.y + 54, label, 1, ui::TEXT);
                }
            }
        }
        if d.kind == DialogKind::PatternMaker {
            if let Some(t) = &d.pm_tile {
                let r = layout.rect;
                let bx = r.x + 12;
                let by = r.y + 36;
                // Tiled preview: the tile repeated into a
                // 240x150 box, scaled down by an integer
                // factor when the tile is large.
                let mut stride = 1usize;
                while t.width as usize / stride > 120 || t.height as usize / stride > 75 {
                    stride += 1;
                }
                let (tw, th) = (t.width as usize / stride, t.height as usize / stride);
                if tw > 0 && th > 0 {
                    for yy in 0..150i32 {
                        for xx in 0..240i32 {
                            let tx = (xx as usize % tw) * stride;
                            let ty = (yy as usize % th) * stride;
                            let p = t.px[ty * t.width as usize + tx].to_rgba8();
                            f.set(bx + xx, by + yy, (p.0, p.1, p.2));
                        }
                    }
                }
                f.outline_rect(Rect::new(bx, by, 240, 150), ui::SHADOW);
                f.text(
                    bx + 248,
                    by + 4,
                    &format!("Tile: {}x{}", t.width, t.height),
                    1,
                    ui::TEXT,
                );
                f.text(bx + 248, by + 20, "Seamless", 1, ui::TEXT);
                f.text(bx + 248, by + 36, "cross-blend", 1, ui::TEXT);
                let fb = Rect::new(bx, by + 158, 150, 18);
                f.button(fb, "Fill layer on OK", d.opt_a, true);
            }
        }
        if d.kind == DialogKind::SelectiveColor {
            let names = [
                "Reds", "Yellows", "Greens", "Cyans", "Blues", "Magentas", "Whites", "Neutrals",
                "Blacks",
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                if i < 9 {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                } else {
                    f.button(
                        *br,
                        if d.fx_b { "Relative" } else { "Absolute" },
                        d.fx_b,
                        true,
                    );
                }
            }
        }
        if d.kind == DialogKind::ReplaceColor {
            for br in layout.range_buttons.iter() {
                f.button(
                    *br,
                    if d.fx_b { "Target: BG" } else { "Target: FG" },
                    false,
                    true,
                );
            }
            let t = d.rc_target;
            let sw = Rect::new(layout.rect.x + 148, layout.rect.y + 30, 40, 18);
            f.fill_rect(sw, t);
            f.outline_rect(sw, ui::SHADOW);
            f.text(
                sw.x + 48,
                sw.y + 5,
                &format!("R {} G {} B {}", t.0, t.1, t.2),
                1,
                ui::TEXT,
            );
        }
        if d.kind == DialogKind::ActionStop {
            let msg = if d.text.chars().count() > 44 {
                format!("{}...", d.text.chars().take(44).collect::<String>())
            } else {
                d.text.clone()
            };
            f.text(layout.rect.x + 12, layout.rect.y + 36, &msg, 1, ui::DARK);
            f.text(
                layout.rect.x + 12,
                layout.rect.y + 52,
                "OK continues the action.",
                1,
                ui::DARK,
            );
        }
        if d.kind == DialogKind::ConfirmClose {
            let msg = if d.text.chars().count() > 44 {
                format!("{}...", d.text.chars().take(44).collect::<String>())
            } else {
                d.text.clone()
            };
            f.text(layout.rect.x + 12, layout.rect.y + 36, &msg, 1, ui::TEXT);
            f.text(
                layout.rect.x + 12,
                layout.rect.y + 52,
                "Close discards the changes.",
                1,
                ui::TEXT,
            );
        }
        if d.kind == DialogKind::InsertMenuItem {
            let r = layout.rect;
            for j in 0..14usize {
                let idx = d.list_offset + j;
                let Some((path, _)) = d.menu_items.get(idx) else {
                    break;
                };
                let row = Rect::new(r.x + 12, r.y + 30 + j as i32 * 15, r.w - 24, 14);
                let label: String = if path.chars().count() > 44 {
                    format!("{}...", path.chars().take(38).collect::<String>())
                } else {
                    path.clone()
                };
                if idx == d.fx_page {
                    f.fill_rect(row, ui::SELROW);
                    f.text(row.x + 4, row.y + 3, &label, 1, ui::WHITE);
                } else {
                    f.text(row.x + 4, row.y + 3, &label, 1, ui::TEXT_DIM);
                }
            }
            let prev = Rect::new(r.x + 12, r.bottom() - 58, 80, 16);
            let next = Rect::new(r.x + 100, r.bottom() - 58, 80, 16);
            f.button(prev, "< Prev", false, d.list_offset > 0);
            f.button(
                next,
                "Next >",
                false,
                d.list_offset + 14 < d.menu_items.len(),
            );
        }
        if d.kind == DialogKind::Fade {
            for br in layout.range_buttons.iter() {
                let name = FADE_MODES
                    .get(d.fx_a as usize)
                    .map(|m| m.0)
                    .unwrap_or("Normal");
                f.button(*br, &format!("Mode: {name}"), false, true);
            }
            f.text(
                layout.rect.x + 12,
                layout.rect.bottom() - 34,
                &format!(
                    "Fading: {}",
                    self.fade_source
                        .as_ref()
                        .map(|f| f.label.as_str())
                        .unwrap_or("")
                ),
                1,
                ui::DARK,
            );
        }
        if d.kind == DialogKind::StrokeSel {
            let names = ["Inside", "Center", "Outside"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                if i < 3 {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                } else {
                    f.button(
                        *br,
                        if d.fx_b {
                            "Color: Foreground"
                        } else {
                            "Color: Background"
                        },
                        false,
                        true,
                    );
                }
            }
        }
        if d.kind == DialogKind::RotateArbitrary {
            let names = ["Clockwise", "Counter CW"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.fx_a == i as u8, true);
            }
        }
        if d.kind == DialogKind::Preferences {
            // Header: Prev / pane name / Next (the 7.0 dialog's own
            // pattern: a pane menu with Next/Prev buttons).
            let r = layout.rect;
            let name = PREF_PAGES[d.fx_page % PREF_PAGES.len()];
            if let Some(br) = layout.range_buttons.first() {
                f.button(*br, "< Prev", false, true);
            }
            if let Some(br) = layout.range_buttons.get(1) {
                f.button(*br, "Next >", false, true);
            }
            let nw = Frame::text_width(name, 2);
            f.text(r.x + (r.w - nw) / 2, r.y + 33, name, 2, ui::TEXT);
            if d.fx_page == PREF_PAGES.len() - 1 {
                // Shortcut list: 12 rows, scrollable.
                for j in 0..12usize {
                    let idx = d.list_offset + j;
                    let Some(def) = self.shortcuts.get(idx) else {
                        break;
                    };
                    let row = Rect::new(r.x + 12, r.y + 52 + j as i32 * 15, r.w - 24, 15);
                    let capturing = Some(idx) == self.prefs_capture;
                    if capturing {
                        f.fill_rect(row, ui::ACCENT);
                    }
                    let combo = if def.combo.is_empty() {
                        "\u{2014}".to_string()
                    } else {
                        combo_display(&def.combo)
                    };
                    let cw = Frame::text_width(&combo, 1);
                    let label = if capturing {
                        "Press a key... (Esc cancels)".to_string()
                    } else {
                        def.label.clone()
                    };
                    let tc = if capturing { ui::WHITE } else { ui::TEXT };
                    // Long tool-group names must not run into the
                    // combo column: truncate with an ellipsis.
                    let max_w = (row.right() - cw - 4) - (row.x + 4) - 8;
                    let mut label = label;
                    if Frame::text_width(&label, 1) > max_w {
                        while !label.is_empty()
                            && Frame::text_width(&format!("{label}\u{2026}"), 1) > max_w
                        {
                            label.pop();
                        }
                        label.push('\u{2026}');
                    }
                    f.text(row.x + 4, row.y + 2, &label, 1, tc);
                    f.text(
                        row.right() - cw - 4,
                        row.y + 2,
                        &combo,
                        1,
                        if capturing { ui::WHITE } else { ui::TEXT_DIM },
                    );
                }
                // Reset row + hint (or the steal warning).
                let reset = Rect::new(r.x + 12, r.y + 238, 90, 18);
                f.button(reset, "Reset All", false, true);
                if let Some((_, c)) = &self.prefs_confirm {
                    f.text(
                        r.x + 110,
                        r.y + 242,
                        &format!(
                            "{} is taken \u{2014} press it again to steal it.",
                            combo_display(c)
                        ),
                        1,
                        ui::TEXT,
                    );
                } else {
                    f.text(
                        r.x + 110,
                        r.y + 242,
                        "Click a row, then press a key. Backspace clears.",
                        1,
                        ui::TEXT_DIM,
                    );
                }
            } else {
                // Settings rows: label left, control right.
                let rows = crate::prefs::prefs_page_buttons(d.fx_page);
                for (j, row) in rows.iter().enumerate() {
                    let Some(br) = layout.range_buttons.get(j + 2) else {
                        continue;
                    };
                    f.text(br.x + 4, br.y + 3, row.label, 1, ui::TEXT);
                    let ctl = Rect::new(br.right() - 150, br.y, 140, 18);
                    match row.kind {
                        PrefKind::Toggle(k) => {
                            let idx = crate::prefs::prefs_toggle_idx(row);
                            let on = d.pref_toggles.get(idx).copied().unwrap_or(false);
                            let _ = k;
                            f.button(ctl, if on { "On" } else { "Off" }, on, true);
                        }
                        PrefKind::Cycle(k, opts, swatch) => {
                            let idx = crate::prefs::prefs_cycle_idx(row);
                            let v = d.pref_cycles.get(idx).copied().unwrap_or(0) as usize
                                % opts.len().max(1);
                            let _ = k;
                            if swatch {
                                let (name, col) = crate::prefs::COLOR_PRESETS
                                    [v % crate::prefs::COLOR_PRESETS.len()];
                                let _ = name;
                                f.button(ctl, "", false, true);
                                let sw = Rect::new(ctl.x + 6, ctl.y + 3, 12, 12);
                                f.fill_rect(sw, col);
                                f.outline_rect(sw, ui::EDGE_MID);
                                f.text(ctl.x + 24, ctl.y + 3, opts[v], 1, ui::TEXT);
                            } else {
                                let mut label = opts[v].to_string();
                                let max_w = ctl.w - 12;
                                if Frame::text_width(&label, 1) > max_w {
                                    while !label.is_empty()
                                        && Frame::text_width(&format!("{label}\u{2026}"), 1) > max_w
                                    {
                                        label.pop();
                                    }
                                    label.push('\u{2026}');
                                }
                                f.button(ctl, &label, false, true);
                            }
                        }
                        PrefKind::Slider(..) => {}
                    }
                }
            }
        }
        if d.kind == DialogKind::FileInfo {
            let names = ["Title", "Caption", "Keys", "Rights", "Credit"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, names[i], d.fx_page == i, true);
            }
        }
        if d.kind == DialogKind::NewLayer {
            for br in layout.range_buttons.iter() {
                f.button(
                    *br,
                    if d.fx_b {
                        "Neutral Fill: On"
                    } else {
                        "Neutral Fill: Off"
                    },
                    d.fx_b,
                    true,
                );
            }
        }
        if d.kind == DialogKind::LoadSel {
            let chans = &self.doc.alphas;
            for (i, br) in layout.range_buttons.iter().enumerate() {
                if i == 0 {
                    f.text(br.x, br.y - 10, "Channel", 1, ui::DARK);
                    let name = chans
                        .get(d.load_chan)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| "(none)".into());
                    f.button(*br, &name, false, !chans.is_empty());
                } else {
                    let ops = ["New", "Add", "Subtract", "Intersect"];
                    f.button(*br, ops[i - 1], d.load_op == i - 1, true);
                }
            }
        }
        if d.kind == DialogKind::ChannelOptions {
            for (i, br) in layout.range_buttons.iter().enumerate() {
                if i == 0 {
                    f.button(
                        *br,
                        if d.fx_b { "Spot: On" } else { "Spot: Off" },
                        d.fx_b,
                        true,
                    );
                } else {
                    f.button(
                        *br,
                        if d.fx_a == 0 {
                            "Indicates: Masked"
                        } else {
                            "Indicates: Selected"
                        },
                        false,
                        true,
                    );
                }
            }
        }
        if d.kind == DialogKind::ColorTable {
            for (i, c) in self.doc.color_table.iter().enumerate() {
                let cr = table_cell_rect(layout.rect, i);
                f.fill_rect(cr, *c);
                f.outline_rect(cr, ui::SHADOW);
                if d.table_sel == i {
                    f.outline_rect(Rect::new(cr.x - 1, cr.y - 1, cr.w + 2, cr.h + 2), ui::BLACK);
                }
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
                            } else if d.extract_touch.get(mi).copied().unwrap_or(0) > 0 {
                                r = ((r as u16 + 255) / 2) as u8;
                                g = ((g as u16 + 220) / 2) as u8;
                                b = (b as u16 / 2) as u8;
                            } else if d.extract_fill.get(mi).copied().unwrap_or(0) > 0 {
                                r = ((r as u16 + 40) / 2) as u8;
                                g = ((g as u16 + 80) / 2) as u8;
                                b = 255;
                            }
                        }
                        f.set(er.x + px, er.y + py, (r, g, b));
                    }
                }
                f.outline_rect(er, ui::PLATINUM_LIGHT);
                let names = [
                    "Highlight",
                    "Fill",
                    "Erase",
                    "Touchup",
                    "Cleanup",
                    "Smart",
                    "Preview",
                ];
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    let on = match i {
                        4 => false,
                        5 => d.extract_smart,
                        6 => d.extract_preview,
                        _ => d.extract_mode == i as u8,
                    };
                    f.button(*br, names[i], on, true);
                }
                f.text(
                    r.x + 12,
                    er.bottom() + 50,
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
                f.outline_rect(wr, ui::PLATINUM_LIGHT);
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
            } else if id == 16 {
                let names = ["Blocks", "Pyramids"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 17 {
                let names = ["Lower Edge", "Upper Edge"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 21 {
                let names = ["Normal", "Edge Only"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 57 {
                let names = ["Circle", "Dot", "Line"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if matches!(id, 55 | 68) {
                let names = ["Canvas", "Burlap", "Brick", "Sandstone"];
                f.button(
                    layout.range_buttons[0],
                    names[(d.fx_a % 4) as usize],
                    false,
                    true,
                );
                if id == 68 {
                    let lights = ["Light: Top", "Light: Bottom", "Light: Left", "Light: Right"];
                    f.button(
                        layout.range_buttons[1],
                        lights[(d.fx_c % 4) as usize],
                        false,
                        true,
                    );
                }
            } else if id == 56 {
                let names = ["Right Diag.", "Left Diag.", "Vertical", "Horizontal"];
                f.button(
                    layout.range_buttons[0],
                    names[(d.fx_a % 4) as usize],
                    false,
                    true,
                );
            } else if id == 60 {
                let names = ["Light: Top", "Light: Bottom", "Light: Left", "Light: Right"];
                f.button(
                    layout.range_buttons[0],
                    names[(d.fx_a % 4) as usize],
                    false,
                    true,
                );
            } else if id == 65 {
                let names = [
                    "Regular",
                    "Soft",
                    "Sprinkles",
                    "Clumped",
                    "Contrasty",
                    "Enlarged",
                    "Stippled",
                    "Horizontal",
                    "Vertical",
                    "Speckle",
                ];
                f.button(
                    layout.range_buttons[0],
                    names[(d.fx_a % 10) as usize],
                    false,
                    true,
                );
            } else if id == 50 {
                let names = ["Right Diag.", "Left Diag."];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 35 {
                let names = ["Simple", "Rough", "Sparkle"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if matches!(id, 39 | 42) {
                let names = ["Canvas", "Burlap", "Brick", "Sandstone"];
                f.button(
                    layout.range_buttons[0],
                    names[(d.fx_a % 4) as usize],
                    false,
                    true,
                );
            } else if id == 23 {
                let names = ["Sine", "Triangle", "Square"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 26 {
                f.button(
                    layout.range_buttons[0],
                    if d.fx_a == 1 {
                        "Tile Map"
                    } else {
                        "Stretch Fit"
                    },
                    false,
                    true,
                );
                f.button(
                    layout.range_buttons[1],
                    if d.fx_b { "Wrap Around" } else { "Repeat Edge" },
                    false,
                    true,
                );
            } else if id == 27 {
                let names = ["50-300 Zoom", "35mm Prime", "105mm Prime"];
                for (i, br) in layout.range_buttons.iter().enumerate() {
                    f.button(*br, names[i], d.fx_a == i as u8, true);
                }
            } else if id == 28 {
                f.button(
                    layout.range_buttons[0],
                    if d.fx_a == 1 {
                        "Point Light"
                    } else {
                        "Spotlight"
                    },
                    false,
                    true,
                );
                f.button(
                    layout.range_buttons[1],
                    if d.fx_b {
                        "Texture: On"
                    } else {
                        "Texture: Off"
                    },
                    false,
                    true,
                );
            } else if id == 22 {
                f.button(
                    layout.range_buttons[0],
                    if d.fx_a == 1 {
                        "Even Fields"
                    } else {
                        "Odd Fields"
                    },
                    false,
                    true,
                );
                f.button(
                    layout.range_buttons[1],
                    if d.fx_b {
                        "Interpolation"
                    } else {
                        "Duplication"
                    },
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
            let layouts = [
                "Layout: 1-up",
                "Layout: 2-up",
                "Layout: 4-up",
                "Layout: wallets",
            ];
            let pages = [
                "Page: Letter",
                "Page: A4",
                "Page: 4x6",
                "Page: 5x7",
                "Page: Legal",
            ];
            let outputs = [
                "Output: Flat",
                "Output: Layers",
                "Flat + Label",
                "Layers + Label",
            ];
            let src = if self.pkg_sources.is_empty() {
                "Source: This doc".to_string()
            } else {
                format!("Sources: {}", self.pkg_sources.len())
            };
            let labels = [
                layouts[d.pkg_layout as usize].to_string(),
                pages[(d.fx_page).min(4)].to_string(),
                outputs[(d.fx_a as usize).min(3)].to_string(),
                src,
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, &labels[i], false, true);
            }
        }
        if d.kind == DialogKind::SaveForWeb {
            let names = [
                "JPEG", "PNG", "GIF", "WBMP", "Weighted", "Knockout", "Dither",
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let on = match i {
                    0..=3 => d.web_format == i as u8,
                    4 => d.web_weighted,
                    5 => d.web_knockout,
                    _ => d.web_dither,
                };
                f.button(*br, names[i], on, true);
            }
            f.text(
                r.x + 12,
                r.bottom() - 52,
                &format!("Encoded size: {} bytes", d.web_size),
                1,
                ui::DARK,
            );
            if d.web_weighted {
                let hint = if self.doc.alphas.is_empty() {
                    "Weighted: add an alpha channel to weight by"
                } else {
                    "Weighted: alpha 1 sets the quality map"
                };
                f.text(r.x + 12, r.bottom() - 38, hint, 1, ui::DARK);
            }
        }
        if d.kind == DialogKind::Print {
            // Positioned page preview (Phase 6.5 Print
            // Options): page outline + the image's place on it.
            let pv = Rect::new(layout.rect.x + 12, layout.rect.y + 30, 150, 110);
            let (pw, ph) = if self.page_landscape {
                (self.page_h, self.page_w)
            } else {
                (self.page_w, self.page_h)
            };
            let pscale = (pv.w as f32 / pw).min(pv.h as f32 / ph);
            let (pw2, ph2) = ((pw * pscale) as i32, (ph * pscale) as i32);
            let page_r = Rect::new(pv.x + (pv.w - pw2) / 2, pv.y + (pv.h - ph2) / 2, pw2, ph2);
            f.fill_rect(page_r, (255, 255, 255));
            f.outline_rect(page_r, ui::SHADOW);
            let fit = if d.opt_b {
                (pw / self.doc.width as f32).min(ph / self.doc.height as f32)
            } else {
                d.vals[1] / 100.0
            };
            let (tw, th) = (self.doc.width as f32 * fit, self.doc.height as f32 * fit);
            let (ox, oy) = if d.opt_a {
                (((pw - tw) / 2.0).max(0.0), ((ph - th) / 2.0).max(0.0))
            } else {
                (d.vals[2].max(0.0), d.vals[3].max(0.0))
            };
            let raw_img = Rect::new(
                page_r.x + (ox * pscale) as i32,
                page_r.y + (oy * pscale) as i32,
                ((tw * pscale) as i32).max(2),
                ((th * pscale) as i32).max(2),
            );
            let img_r = Rect::new(
                raw_img.x.max(page_r.x),
                raw_img.y.max(page_r.y),
                (raw_img.right().min(page_r.right()) - raw_img.x.max(page_r.x)).max(1),
                (raw_img.bottom().min(page_r.bottom()) - raw_img.y.max(page_r.y)).max(1),
            );
            f.fill_rect(img_r, (170, 190, 220));
            f.outline_rect(img_r, ui::ACCENT);
            let labels: [&str; 10] = [
                if d.print_profile == 0 {
                    "Color: document"
                } else {
                    "Color: printer"
                },
                if d.opt_a {
                    "Centered: On"
                } else {
                    "Centered: Off"
                },
                if d.opt_b { "Fit: On" } else { "Fit: Off" },
                if d.pr_bilinear { "Bilinear" } else { "Nearest" },
                "Crop Marks",
                "Reg Marks",
                "Calib Bars",
                "Labels",
                if d.fx_b {
                    "Negative: On"
                } else {
                    "Negative: Off"
                },
                if d.pr_emulsion {
                    "Emul: Down"
                } else {
                    "Emul: Up"
                },
            ];
            let states = [
                false,
                d.opt_a,
                d.opt_b,
                d.pr_bilinear,
                d.fx_a & 1 != 0,
                d.fx_a & 2 != 0,
                d.fx_a & 4 != 0,
                d.fx_a & 8 != 0,
                d.fx_b,
                d.pr_emulsion,
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                f.button(*br, labels[i], states[i], true);
            }
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
            f.text(r.x + 12, r.y + 22, "This document", 1, ui::DARK);
            f.text(r.x + 12, r.y + 56, "New documents", 1, ui::DARK);
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let selected = if i < 3 {
                    d.cb_range == i
                } else {
                    d.cs_default == i - 3
                };
                f.button(*br, names[i % 3], selected, true);
            }
            f.text(
                r.x + 12,
                r.bottom() - 40,
                "Assign / Convert: Mode menu.",
                1,
                ui::DARK,
            );
        }
        if d.kind == DialogKind::Effects {
            let tabs = [
                "Drop", "Inner", "Glow", "InGlow", "Bevel", "Satin", "Color", "Grad", "Patrn",
                "Stroke",
            ];
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
                4 => (
                    fx.bevel.enabled,
                    ["Outer", "Inner", "Emboss", "Pillow"][fx.bevel.style as usize],
                    if fx.bevel.up { "Up" } else { "Down" },
                    fx.bevel.up,
                ),
                5 => (
                    fx.satin.enabled,
                    fx.satin.mode.name(),
                    if fx.satin.invert {
                        "Inverted"
                    } else {
                        "Normal"
                    },
                    fx.satin.invert,
                ),
                6 => (
                    fx.color_overlay.enabled,
                    fx.color_overlay.mode.name(),
                    "-",
                    false,
                ),
                7 => (
                    fx.gradient_overlay.enabled,
                    fx.gradient_overlay.mode.name(),
                    if fx.gradient_overlay.style == 1 {
                        "Radial"
                    } else {
                        "Linear"
                    },
                    fx.gradient_overlay.style == 1,
                ),
                8 => (
                    fx.pattern_overlay.enabled,
                    fx.pattern_overlay.mode.name(),
                    ["Checker", "Dots", "Lines"][fx.pattern_overlay.kind as usize],
                    false,
                ),
                _ => (
                    fx.stroke.enabled,
                    fx.stroke.mode.name(),
                    ["Outside", "Inside", "Center"][fx.stroke.position as usize],
                    false,
                ),
            };
            let color = match d.fx_page {
                0 => fx.drop_shadow.color,
                1 => fx.inner_shadow.color,
                2 => fx.outer_glow.color,
                3 => fx.inner_glow.color,
                5 => fx.satin.color,
                6 => fx.color_overlay.color,
                7 => {
                    if d.opt_a {
                        fx.gradient_overlay.to
                    } else {
                        fx.gradient_overlay.from
                    }
                }
                9 => fx.stroke.color,
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
                    7 => f.button(*br, "Texture", fx.bevel.texture, true),
                    _ => {
                        let label = if d.fx_page == 4 {
                            if d.opt_a {
                                "Sh:"
                            } else {
                                "Hi:"
                            }
                        } else if d.fx_page == 7 {
                            if d.opt_a {
                                "To:"
                            } else {
                                "From:"
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
        if d.kind == DialogKind::Blending {
            let labels = [
                format!(
                    "Knockout: {}",
                    ["None", "Shallow", "Deep"][d.blend_knockout as usize]
                ),
                format!(
                    "Channels: {}",
                    match d.blend_channels {
                        0xF => "RGB",
                        0x1 => "R",
                        0x2 => "G",
                        0x4 => "B",
                        0x7 => "RG",
                        _ => "None",
                    }
                ),
                "Clip as Group".to_string(),
                "Interior as Group".to_string(),
                "Transp Shapes".to_string(),
                "Mask Hides FX".to_string(),
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let on = match i {
                    0 | 1 => false,
                    _ => d.blend_toggles & (1u8 << (i - 2)) != 0,
                };
                f.button(*br, &labels[i], on, true);
            }
            f.text(
                r.x + 12,
                r.y + 272,
                "Feather sliders split the Blend If points.",
                1,
                ui::TEXT,
            );
        }
        if d.kind == DialogKind::GradientMap {
            let swf = Rect::new(r.x + 12, r.y + 58, 22, 22);
            f.fill_rect(swf, d.photo_color);
            f.outline_rect(swf, ui::BLACK);
            f.text(r.x + 40, r.y + 64, "From (dark)", 1, ui::TEXT);
            let swt = Rect::new(r.x + 150, r.y + 58, 22, 22);
            f.fill_rect(swt, d.gm_to);
            f.outline_rect(swt, ui::BLACK);
            f.text(r.x + 178, r.y + 64, "To (light)", 1, ui::TEXT);
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let (label, on) = match i {
                    0 => ("From: FG", false),
                    1 => ("To: BG", false),
                    _ => ("Reverse", d.opt_a),
                };
                f.button(*br, label, on, true);
            }
        }
        if d.kind == DialogKind::Liquify {
            let er = self.liquify_editor_rect().unwrap_or(layout.rect);
            let src = self
                .doc
                .render_region_rgba8(0, 0, self.doc.width, self.doc.height);
            let (dw, dh) = (self.doc.width.max(1) as f32, self.doc.height.max(1) as f32);
            let lscale = (er.w as f32 / dw).min(er.h as f32 / dh) * d.liq_zoom.max(0.05);
            let (lox, loy) = (
                er.x as f32 + er.w as f32 / 2.0 - d.liq_pan.0 * lscale,
                er.y as f32 + er.h as f32 / 2.0 - d.liq_pan.1 * lscale,
            );
            // Backdrop (7.0): the other layers, dimmed, under
            // the warped active layer.
            let (under, over) = if d.liq_backdrop {
                let li = d.adj_layer.unwrap_or(self.doc.active_layer);
                let mut du = self.doc.clone();
                if li < du.layers.len() {
                    du.layers[li].visible = false;
                }
                let mut dv = self.doc.clone();
                for (i, l) in dv.layers.iter_mut().enumerate() {
                    l.visible = i == li;
                }
                (
                    Some(du.render_region_rgba8(0, 0, du.width, du.height)),
                    Some(dv.render_region_rgba8(0, 0, dv.width, dv.height)),
                )
            } else {
                (None, None)
            };
            for py in 0..er.h {
                for px in 0..er.w {
                    let (dx, dy) = (
                        ((er.x + px) as f32 - lox) / lscale,
                        ((er.y + py) as f32 - loy) / lscale,
                    );
                    if dx < 0.0 || dy < 0.0 || dx >= dw || dy >= dh {
                        continue;
                    }
                    // Warped preview (v0.6.1): sample the
                    // source at the doc point PLUS the field's
                    // displacement there — the same resample
                    // apply_liquify performs on OK, so what the
                    // editor shows is what gets baked.
                    let di = (dy as usize).min(self.doc.height as usize - 1)
                        * self.doc.width as usize
                        + (dx as usize).min(self.doc.width as usize - 1);
                    let (ox, oy) = d.liq_disp.get(di).copied().unwrap_or((0.0, 0.0));
                    let (sx, sy) = (
                        (dx + ox).clamp(0.0, dw - 1.0),
                        (dy + oy).clamp(0.0, dh - 1.0),
                    );
                    let idx = ((sy as u32) * self.doc.width + sx as u32) as usize * 4;
                    let (mut cr, mut cg, mut cb) = (
                        src.get(idx).copied().unwrap_or(0),
                        src.get(idx + 1).copied().unwrap_or(0),
                        src.get(idx + 2).copied().unwrap_or(0),
                    );
                    if let (Some(u), Some(o)) = (&under, &over) {
                        let (ar, ag, ab, aa) = (
                            o.get(idx).copied().unwrap_or(0),
                            o.get(idx + 1).copied().unwrap_or(0),
                            o.get(idx + 2).copied().unwrap_or(0),
                            o.get(idx + 3).copied().unwrap_or(0),
                        );
                        let uidx = ((dy as u32) * self.doc.width + dx as u32) as usize * 4;
                        let dim = |v: u8| (v as f32 * 0.55) as u8;
                        let (ur, ug, ub) = (
                            dim(u.get(uidx).copied().unwrap_or(0)),
                            dim(u.get(uidx + 1).copied().unwrap_or(0)),
                            dim(u.get(uidx + 2).copied().unwrap_or(0)),
                        );
                        let mix = |a: u8, b: u8| {
                            ((a as u16 * aa as u16 + b as u16 * (255 - aa as u16)) / 255) as u8
                        };
                        cr = mix(ar, ur);
                        cg = mix(ag, ug);
                        cb = mix(ab, ub);
                    }
                    let fi = (sy as usize) * self.doc.width as usize + sx as usize;
                    if d.liq_frozen.get(fi).copied().unwrap_or(0) != 0 {
                        cr = ((cr as u16 + 255) / 2) as u8;
                        cg = (cg as u16 / 2) as u8;
                        cb = (cb as u16 / 2) as u8;
                    }
                    if d.liq_mesh && ((sx as i32) % 24 == 0 || (sy as i32) % 24 == 0) {
                        cr = 255 - cr;
                        cg = 255 - cg;
                        cb = 255 - cb;
                    }
                    f.set(er.x + px, er.y + py, (cr, cg, cb));
                }
            }
            f.outline_rect(er, ui::PLATINUM_LIGHT);
            let tools = [
                "Warp", "Twirl+", "Twirl-", "Pucker", "Bloat", "Shift", "Reflect", "Recon",
                "Freeze", "Thaw", "Turbul", "Hand", "Mesh", "Stiff", "Clear", "Undo", "Bckdrop",
                "SaveMesh", "LoadMesh",
            ];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let on = if i < 12 {
                    d.liq_tool == i as u8
                } else if i == 12 {
                    d.liq_mesh
                } else if i == 16 {
                    d.liq_backdrop
                } else {
                    false
                };
                f.button(*br, tools[i], on, true);
            }
            f.text(
                r.x + 12,
                r.y + 402,
                "Live warp preview. Freeze = red mask.",
                1,
                ui::TEXT,
            );
            let zr = self.liquify_zoom_rects(r);
            f.button(zr[0], "-", false, true);
            f.button(zr[1], "+", false, true);
            f.button(zr[2], "Fit", d.liq_zoom == 1.0, true);
        }
        if d.kind == DialogKind::SpellCheck {
            if let Some(tf) = layout.text_field {
                f.text(tf.x, tf.y - 10, "Change to:", 1, ui::TEXT);
            }
            match d.spell_items.get(d.spell_pos) {
                Some((li, _, _, word)) => {
                    f.text(r.x + 12, r.y + 58, "Not in dictionary:", 1, ui::TEXT);
                    f.text(r.x + 12, r.y + 74, word, 2, ui::TEXT);
                    f.text(
                        r.x + 12,
                        r.y + 104,
                        &format!(
                            "Layer '{}' · {} of {}",
                            self.doc.layers[*li].name,
                            d.spell_pos + 1,
                            d.spell_items.len()
                        ),
                        1,
                        ui::DARK,
                    );
                }
                None => {
                    f.text(
                        r.x + 12,
                        r.y + 74,
                        "Check complete: no unknown words.",
                        1,
                        ui::TEXT,
                    );
                }
            }
            let labels = ["Change", "Ignore", "Ign All", "Add", "Done"];
            for (i, label) in labels.iter().enumerate() {
                let br = Rect::new(r.x + 12 + i as i32 * 76, r.y + 122, 72, 18);
                f.button(br, label, false, true);
            }
        }
        if d.kind == DialogKind::FindReplace {
            if let Some(tf) = layout.text_field {
                if d.text.is_empty() {
                    f.text(tf.x + 4, tf.y + 4, "Find text", 1, ui::SHADOW);
                }
                let f2 = Rect::new(tf.x, tf.y + 34, tf.w, tf.h);
                f.text(f2.x, f2.y - 10, "Replace:", 1, ui::TEXT);
                if d.text2.is_empty() {
                    f.text(f2.x + 4, f2.y + 4, "Replacement text", 1, ui::SHADOW);
                }
                f.fill_rect(f2, ui::WHITE);
                f.outline_rect(f2, ui::SHADOW);
                if d.text_focus && d.text_focus_n == 1 {
                    f.outline_rect(Rect::new(f2.x - 1, f2.y - 1, f2.w + 2, f2.h + 2), ui::BLACK);
                }
                f.text(f2.x + 4, f2.y + 4, &d.text2, 1, ui::BLACK);
            }
            let n = self.find_text_matches(&d.text, d.opt_a, d.opt_b).len();
            f.text(
                r.x + 12,
                r.y + 92,
                &format!("{n} match(es) in type layers"),
                1,
                ui::DARK,
            );
            let labels = ["Replace All", "Case", "Whole Word"];
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let on = match i {
                    1 => d.opt_a,
                    2 => d.opt_b,
                    _ => false,
                };
                f.button(*br, labels[i], on, true);
            }
        }
        if d.kind == DialogKind::FileBrowser {
            f.text(
                r.x + 12,
                r.y + 52,
                &format!("Folder: {}", d.browser_dir),
                1,
                ui::TEXT,
            );
            for (i, e) in d.browser_entries.iter().take(12).enumerate() {
                let cell = self.browser_cell(r, i);
                if i == d.browser_sel {
                    f.fill_rect(cell, (36, 84, 160));
                } else {
                    f.fill_rect(cell, ui::WHITE);
                }
                f.outline_rect(cell, ui::BLACK);
                if e.is_dir {
                    f.text(cell.x + 8, cell.y + 22, "[Folder]", 1, ui::TEXT);
                } else if e.tw > 0 {
                    let (ox, oy) = (
                        cell.x + (92 - e.tw as i32) / 2,
                        cell.y + 3 + (46 - e.th as i32) / 2,
                    );
                    for y in 0..e.th as i32 {
                        for x in 0..e.tw as i32 {
                            let idx = ((y * e.tw as i32 + x) * 4) as usize;
                            if idx + 2 < e.thumb.len() {
                                f.set(
                                    ox + x,
                                    oy + y,
                                    (e.thumb[idx], e.thumb[idx + 1], e.thumb[idx + 2]),
                                );
                            }
                        }
                    }
                }
                let label = if e.rank > 0 {
                    format!("{} [{}]", e.name, (b'A' + e.rank - 1) as char)
                } else {
                    e.name.clone()
                };
                let label: String = label.chars().take(11).collect();
                let tc = if i == d.browser_sel {
                    ui::WHITE
                } else {
                    ui::BLACK
                };
                f.text(cell.x + 4, cell.y + 52, &label, 1, tc);
            }
            let info = d
                .browser_entries
                .get(d.browser_sel)
                .map(|e| e.info.clone())
                .unwrap_or_default();
            f.text(r.x + 12, r.y + 276, &info, 1, ui::TEXT);
            let labels = [
                "Open", "Up", "Rotate L", "Rotate R", "Rank", "Sort", "Rename", "Refresh",
            ];
            for (i, br) in self.browser_button_rects(r).iter().enumerate() {
                let on = (i == 5 && d.browser_sort_rank)
                    || (i == 4
                        && d.browser_entries
                            .get(d.browser_sel)
                            .map(|e| e.rank > 0)
                            .unwrap_or(false));
                f.button(*br, labels[i], on, true);
            }
        }
        if d.kind == DialogKind::PresetManager {
            let tabs = ["Styles", "Shapes", "Tools"];
            for (i, t) in tabs.iter().enumerate() {
                let br = Rect::new(r.x + 12 + i as i32 * 92, r.y + 52, 88, 18);
                f.button(br, t, self.preset_tab == i, true);
            }
            let rows = self.preset_rows();
            for (i, name) in rows.iter().take(12).enumerate() {
                let rr = Rect::new(r.x + 12, r.y + 78 + i as i32 * 15, r.w - 24, 14);
                if i == d.preset_sel {
                    f.fill_rect(rr, (36, 84, 160));
                    f.text(rr.x + 3, rr.y + 3, name, 1, ui::WHITE);
                } else {
                    f.text(rr.x + 3, rr.y + 3, name, 1, ui::TEXT);
                }
            }
            let by = r.bottom() - 58;
            f.button(Rect::new(r.x + 12, by, 110, 18), "Apply (OK)", false, true);
            f.button(
                Rect::new(r.x + 130, by, 90, 18),
                "New...",
                self.preset_tab != 1,
                true,
            );
            f.button(
                Rect::new(r.x + 228, by, 90, 18),
                "Delete",
                self.preset_tab != 1,
                true,
            );
            f.text(
                r.x + 12,
                r.y + 178,
                if self.preset_tab == 0 {
                    "New = save layer effects by name."
                } else if self.preset_tab == 2 {
                    "New = capture the current tool."
                } else {
                    "Shapes: built-ins + this doc's."
                },
                1,
                ui::BLACK,
            );
        }
        if d.kind == DialogKind::SliceProps {
            if let Some(tf) = layout.text_field {
                f.text(tf.x, tf.y - 10, "Name:", 1, ui::TEXT);
                let f2 = Rect::new(tf.x, tf.y + 24, tf.w, tf.h);
                let f3 = Rect::new(tf.x, tf.y + 48, tf.w, tf.h);
                f.text(f2.x, f2.y - 10, "URL:", 1, ui::TEXT);
                f.text(f3.x, f3.y - 10, "Alt text:", 1, ui::TEXT);
                for (i, fr) in [f2, f3].iter().enumerate() {
                    f.fill_rect(*fr, ui::WHITE);
                    f.outline_rect(*fr, ui::SHADOW);
                    if d.text_focus && d.text_focus_n == (i + 1) as u8 {
                        f.outline_rect(
                            Rect::new(fr.x - 1, fr.y - 1, fr.w + 2, fr.h + 2),
                            ui::BLACK,
                        );
                    }
                }
                f.text(f2.x + 4, f2.y + 4, &d.text2, 1, ui::BLACK);
                f.text(f3.x + 4, f3.y + 4, &d.text3, 1, ui::BLACK);
            }
        }
        if d.kind == DialogKind::NoteEdit {
            if let Some(tf) = layout.text_field {
                f.text(tf.x, tf.y - 10, "Note text:", 1, ui::TEXT);
            }
            let rec_on = self.audio_rec.is_some();
            for (i, br) in layout.range_buttons.iter().enumerate() {
                let (label, on) = if i == 0 {
                    (if rec_on { "Stop" } else { "Record" }, rec_on)
                } else {
                    ("Play", false)
                };
                f.button(*br, label, on, true);
            }
            let info = match &d.note_audio {
                Some(w) => format!("Audio attached ({} bytes)", w.len()),
                None => "No audio attached".to_string(),
            };
            f.text(r.x + 12, r.y + 76, &info, 1, ui::DARK);
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
                f.text(tx, cell.y + 6, &label, 1, ui::TEXT);
            }
            let by = r.y + 156;
            f.text(r.x + 48, by + 5, "Scale:", 1, ui::TEXT);
            f.text(r.x + 96, by + 5, &d.kernel_scale.to_string(), 1, ui::TEXT);
            f.button(layout.kernel_buttons[0], "-", false, true);
            f.button(layout.kernel_buttons[1], "+", false, true);
            f.text(r.x + 216, by + 5, "Offset:", 1, ui::TEXT);
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
            f.text(r.x + 12, r.y + 56, &format!("Action: {name}"), 1, ui::TEXT);
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
                        f.fill_rect(
                            Rect::new(fams_r.x + 1, ry - 1, fams_r.w - 2, 14),
                            ui::SELROW,
                        );
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
                let labels2 = ["Warp", "SmCp", "Sup", "Sub", "Strk", "Just"];
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if i < 8 {
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
                    } else {
                        let j = i - 8;
                        let on = match j {
                            0 => spec.warp_style.is_some(),
                            1 => spec.extras & 1 != 0,
                            2 => spec.extras & 2 != 0,
                            3 => spec.extras & 4 != 0,
                            4 => spec.extras & 8 != 0,
                            _ => spec.align == 3,
                        };
                        let label = if j == 0 {
                            match spec.warp_style {
                                Some(w) => pictodesk_core::warp::WARP_STYLES
                                    .get(w as usize)
                                    .copied()
                                    .unwrap_or("Warp"),
                                None => "Warp",
                            }
                        } else {
                            labels2[j]
                        };
                        f.button(*br, label, on, true);
                    }
                }
                f.text(
                    r.x + 12,
                    layout.type_buttons[0].y + 44,
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
                f.fill_rect(*cell, if is { ui::ACCENT } else { ui::PLATINUM_LIGHT });
                f.outline_rect(*cell, ui::DARK);
            }
        }
        // Settings Save/Load (Phase 6.5 Batch 6d): bottom-left
        // of settings-capable adjustment dialogs.
        if dialog_candidate(d).is_some() {
            Self::draw_small_button(
                f,
                Rect::new(r.x + 12, r.bottom() - 32, 52, 18),
                "Save",
                true,
            );
            Self::draw_small_button(
                f,
                Rect::new(r.x + 70, r.bottom() - 32, 52, 18),
                "Load",
                true,
            );
        }
        f.button(layout.ok, "OK", false, true);
        f.button(layout.cancel, "Cancel", false, true);
    }

    pub(crate) fn slider_track(row: Rect) -> Rect {
        Rect::new(row.x + 118, row.y + 3, row.w - 118 - 44, 8)
    }

    pub(crate) fn dialog_slider_value(&self, row: Rect, mx: i32) -> f32 {
        let track = Self::slider_track(row);
        ((mx - track.x) as f32 / (track.w - 8).max(1) as f32).clamp(0.0, 1.0)
    }

    pub(crate) fn set_dialog_slider(&mut self, idx: usize, t: f32) {
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

    /// Type tool: click empty canvas for a new type layer, click
    /// an existing type layer to re-edit it.
    pub(crate) fn open_type_dialog(&mut self, dp: (f32, f32), vertical: bool, mask: bool) {
        // Type Units (Preferences > Units & Rulers): the Size
        // slider shows points / pixels / millimeters; the stored
        // size stays in pixels. k = px per display unit.
        let res = self.doc.resolution.max(1.0);
        let k = match self.prefs.type_units {
            0 => res / 72.0,
            2 => res / 25.4,
            _ => 1.0,
        };
        // Range: 6..288 pt in display units.
        let (tmin, tmax) = (6.0 * (res / 72.0) / k, 288.0 * (res / 72.0) / k);
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
            d.vals = vec![
                s.default_style.size / k,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                50.0,
                0.0,
                0.0,
            ];
            d.type_unit_k = k;
            d.type_size_min = tmin;
            d.type_size_max = tmax;
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
        d.type_unit_k = k;
        d.type_size_min = tmin;
        d.type_size_max = tmax;
        d.vals = vec![
            spec.default_style.size / k,
            spec.default_style.tracking,
            spec.leading,
            0.0,
            spec.text.len() as f32,
            spec.box_w as f32,
            spec.space_before,
            spec.first_indent,
            spec.warp_bend,
            spec.warp_h,
            spec.warp_v,
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
    pub(crate) fn type_style_apply<F: Fn(&mut pictodesk_core::text::CharStyle)>(&mut self, f: F) {
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

    /// Layer > Effects: open the shared effects dialog at a page.
    pub(crate) fn open_effects_dialog(&mut self, page: usize) {
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
    pub(crate) fn fx_load_vals(&mut self) {
        let Some(d) = &mut self.dialog else { return };
        let fx = d.fx;
        d.vals = match d.fx_page {
            0 => vec![
                fx.drop_shadow.opacity * 100.0,
                fx.drop_shadow.angle as f32,
                fx.drop_shadow.distance as f32,
                fx.drop_shadow.blur as f32,
                fx.drop_shadow.intensity as f32,
                fx.drop_shadow.contour as f32,
            ],
            1 => vec![
                fx.inner_shadow.opacity * 100.0,
                fx.inner_shadow.angle as f32,
                fx.inner_shadow.distance as f32,
                fx.inner_shadow.blur as f32,
                fx.inner_shadow.intensity as f32,
                fx.inner_shadow.contour as f32,
            ],
            2 => vec![
                fx.outer_glow.opacity * 100.0,
                fx.outer_glow.blur as f32,
                fx.outer_glow.intensity as f32,
                fx.outer_glow.contour as f32,
            ],
            3 => vec![
                fx.inner_glow.opacity * 100.0,
                fx.inner_glow.blur as f32,
                fx.inner_glow.intensity as f32,
                fx.inner_glow.contour as f32,
            ],
            4 => vec![
                fx.bevel.depth as f32,
                fx.bevel.angle as f32,
                fx.bevel.hi_opacity * 100.0,
                fx.bevel.sh_opacity * 100.0,
                fx.bevel.contour as f32,
                fx.bevel.texture_depth as f32,
            ],
            5 => vec![
                fx.satin.opacity * 100.0,
                fx.satin.angle as f32,
                fx.satin.distance as f32,
                fx.satin.blur as f32,
                fx.satin.contour as f32,
            ],
            6 => vec![fx.color_overlay.opacity * 100.0],
            7 => vec![
                fx.gradient_overlay.opacity * 100.0,
                fx.gradient_overlay.angle as f32,
                fx.gradient_overlay.scale as f32,
            ],
            8 => vec![
                fx.pattern_overlay.opacity * 100.0,
                fx.pattern_overlay.scale as f32,
            ],
            _ => vec![fx.stroke.opacity * 100.0, fx.stroke.size as f32],
        };
    }

    /// Write the Effects dialog sliders back into the spec and
    /// apply it to the layer (live preview; the dialog's snapshot
    /// makes Cancel exact and OK a single undo step).
    pub(crate) fn fx_sync_from_vals(&mut self) {
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
                if v.len() > 5 {
                    fx.drop_shadow.contour = (v[5] as u8).min(3);
                }
            }
            1 => {
                fx.inner_shadow.opacity = v[0] / 100.0;
                fx.inner_shadow.angle = v[1] as i32;
                fx.inner_shadow.distance = v[2].max(0.0) as u32;
                fx.inner_shadow.blur = v[3].max(0.0) as u32;
                fx.inner_shadow.intensity = v[4].max(0.0) as u32;
                if v.len() > 5 {
                    fx.inner_shadow.contour = (v[5] as u8).min(3);
                }
            }
            2 => {
                fx.outer_glow.opacity = v[0] / 100.0;
                fx.outer_glow.blur = v[1].max(0.0) as u32;
                fx.outer_glow.intensity = v[2].max(0.0) as u32;
                if v.len() > 3 {
                    fx.outer_glow.contour = (v[3] as u8).min(3);
                }
            }
            3 => {
                fx.inner_glow.opacity = v[0] / 100.0;
                fx.inner_glow.blur = v[1].max(0.0) as u32;
                fx.inner_glow.intensity = v[2].max(0.0) as u32;
                if v.len() > 3 {
                    fx.inner_glow.contour = (v[3] as u8).min(3);
                }
            }
            4 => {
                fx.bevel.depth = v[0].max(0.0) as u32;
                fx.bevel.angle = v[1] as i32;
                if v.len() > 3 {
                    fx.bevel.hi_opacity = v[2] / 100.0;
                    fx.bevel.sh_opacity = v[3] / 100.0;
                }
                if v.len() > 5 {
                    fx.bevel.contour = (v[4] as u8).min(3);
                    fx.bevel.texture_depth = v[5].max(0.0) as u32;
                }
            }
            5 => {
                fx.satin.opacity = v[0] / 100.0;
                fx.satin.angle = v[1] as i32;
                fx.satin.distance = v[2].max(0.0) as u32;
                fx.satin.blur = v[3].max(0.0) as u32;
                if v.len() > 4 {
                    fx.satin.contour = (v[4] as u8).min(3);
                }
            }
            6 => {
                fx.color_overlay.opacity = v[0] / 100.0;
            }
            7 => {
                fx.gradient_overlay.opacity = v[0] / 100.0;
                fx.gradient_overlay.angle = v[1] as i32;
                fx.gradient_overlay.scale = v[2].max(10.0) as u32;
            }
            8 => {
                fx.pattern_overlay.opacity = v[0] / 100.0;
                fx.pattern_overlay.scale = v[1].max(10.0) as u32;
            }
            _ => {
                fx.stroke.opacity = v[0] / 100.0;
                fx.stroke.size = v[1].max(1.0) as u32;
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
    pub(crate) fn create_effect_layers(&mut self) {
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
    pub(crate) fn fx_button_click(&mut self, i: usize) {
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
            (4, 7) => fx.bevel.texture = !fx.bevel.texture,
            (5, 0) => fx.satin.enabled = !fx.satin.enabled,
            (5, 1) => fx.satin.mode = next_mode(fx.satin.mode),
            (5, 2) => fx.satin.invert = !fx.satin.invert,
            (5, 3) => fx.satin.color = fg,
            (6, 0) => fx.color_overlay.enabled = !fx.color_overlay.enabled,
            (6, 1) => fx.color_overlay.mode = next_mode(fx.color_overlay.mode),
            (6, 3) => fx.color_overlay.color = fg,
            (7, 0) => fx.gradient_overlay.enabled = !fx.gradient_overlay.enabled,
            (7, 1) => fx.gradient_overlay.mode = next_mode(fx.gradient_overlay.mode),
            (7, 2) => fx.gradient_overlay.style = (fx.gradient_overlay.style + 1) % 2,
            (7, 3) => {
                if d.opt_a {
                    fx.gradient_overlay.to = fg;
                    d.opt_a = false;
                } else {
                    fx.gradient_overlay.from = fg;
                    d.opt_a = true;
                }
            }
            (8, 0) => fx.pattern_overlay.enabled = !fx.pattern_overlay.enabled,
            (8, 1) => fx.pattern_overlay.mode = next_mode(fx.pattern_overlay.mode),
            (8, 2) => fx.pattern_overlay.kind = (fx.pattern_overlay.kind + 1) % 3,
            (9, 0) => fx.stroke.enabled = !fx.stroke.enabled,
            (9, 1) => fx.stroke.mode = next_mode(fx.stroke.mode),
            (9, 2) => fx.stroke.position = (fx.stroke.position + 1) % 3,
            (9, 3) => fx.stroke.color = fg,
            _ => {}
        }
        self.fx_sync_from_vals();
    }

    /// Layer > Blending Options: fill opacity, Blend If,
    /// knockout, channel switches, advanced toggles, and the
    /// global light (angle + altitude). Live-applies through
    /// the adj_layer/adj_snapshot plumbing, like Effects.
    pub(crate) fn open_blending_dialog(&mut self) {
        let li = self.doc.active_layer;
        let l = &self.doc.layers[li];
        let mut d = Dialog::new(DialogKind::Blending, &self.doc);
        d.adj_layer = Some(li);
        d.adj_snapshot = Some(self.doc.clone());
        d.vals = vec![
            l.fill_opacity * 100.0,
            l.blend_if[0] as f32,
            l.blend_if[1] as f32,
            l.blend_if[2] as f32,
            l.blend_if[3] as f32,
            l.blend_if_split[0] as f32,
            l.blend_if_split[1] as f32,
            l.blend_if_split[2] as f32,
            l.blend_if_split[3] as f32,
            self.doc.global_angle as f32,
            self.doc.global_altitude as f32,
        ];
        d.blend_knockout = l.knockout;
        d.blend_channels = l.blend_channels;
        d.blend_toggles = (l.blend_clipped_group as u8)
            | ((l.blend_interior_group as u8) << 1)
            | ((l.transparency_shapes as u8) << 2)
            | ((l.mask_hides_effects as u8) << 3);
        self.dialog = Some(d);
    }

    /// Write the Blending dialog state into the layer (live).
    pub(crate) fn blend_sync_from_vals(&mut self) {
        let Some(d) = &mut self.dialog else { return };
        let v = d.vals.clone();
        let (ko, ch, tg) = (d.blend_knockout, d.blend_channels, d.blend_toggles);
        if let Some(li) = d.adj_layer {
            let l = &mut self.doc.layers[li];
            l.fill_opacity = (v[0] / 100.0).clamp(0.0, 1.0);
            l.blend_if = [
                v[1].clamp(0.0, 255.0) as u8,
                v[2].clamp(0.0, 255.0) as u8,
                v[3].clamp(0.0, 255.0) as u8,
                v[4].clamp(0.0, 255.0) as u8,
            ];
            l.blend_if_split = [
                v[5].clamp(0.0, 255.0) as u8,
                v[6].clamp(0.0, 255.0) as u8,
                v[7].clamp(0.0, 255.0) as u8,
                v[8].clamp(0.0, 255.0) as u8,
            ];
            l.knockout = ko;
            l.blend_channels = ch;
            l.blend_clipped_group = tg & 1 != 0;
            l.blend_interior_group = tg & 2 != 0;
            l.transparency_shapes = tg & 4 != 0;
            l.mask_hides_effects = tg & 8 != 0;
            self.doc.global_angle = v[9] as i32;
            self.doc.global_altitude = v[10] as i32;
            self.doc_version += 1;
        }
    }

    /// Image > Liquify: the mesh-warp dialog. Opens on the
    /// active layer with a fresh displacement field.
    pub(crate) fn open_liquify_dialog(&mut self) {
        let li = self.doc.active_layer;
        if self.doc.layers[li].adjustment.is_some() {
            self.status = "Liquify needs a pixel layer".into();
            return;
        }
        let mut d = Dialog::new(DialogKind::Liquify, &self.doc);
        d.adj_layer = Some(li);
        d.adj_snapshot = Some(self.doc.clone());
        let n = (self.doc.width * self.doc.height) as usize;
        d.liq_disp = vec![(0.0, 0.0); n];
        d.liq_frozen = vec![0u8; n];
        d.liq_pan = (self.doc.width as f32 / 2.0, self.doc.height as f32 / 2.0);
        self.dialog = Some(d);
        self.status = "Liquify: paint with the warp tools, OK to apply".into();
    }

    /// The Liquify editor canvas rect (same geometry as Extract).
    pub(crate) fn liquify_editor_rect(&self) -> Option<Rect> {
        let layout = self.dialog_layout()?;
        let r = layout.rect;
        Some(Rect::new(r.x + 12, r.y + 30, r.w - 24, 276))
    }

    /// Map a window point in the Liquify editor to doc coords
    /// (Phase 7: zoom + pan around the pan point).
    pub(crate) fn liquify_doc_point(&self, mx: i32, my: i32) -> Option<(f32, f32)> {
        let er = self.liquify_editor_rect()?;
        if !er.contains(mx, my) {
            return None;
        }
        let d = self.dialog.as_ref()?;
        let (w, h) = (self.doc.width as f32, self.doc.height as f32);
        let scale = (er.w as f32 / w).min(er.h as f32 / h) * d.liq_zoom.max(0.05);
        let (ox, oy) = (
            er.x as f32 + er.w as f32 / 2.0 - d.liq_pan.0 * scale,
            er.y as f32 + er.h as f32 / 2.0 - d.liq_pan.1 * scale,
        );
        Some(((mx as f32 - ox) / scale, (my as f32 - oy) / scale))
    }

    /// The three zoom buttons on the Liquify hint line.
    pub(crate) fn liquify_zoom_rects(&self, r: Rect) -> [Rect; 3] {
        [
            Rect::new(r.x + 296, r.y + 400, 30, 16),
            Rect::new(r.x + 328, r.y + 400, 30, 16),
            Rect::new(r.x + 360, r.y + 400, 34, 16),
        ]
    }

    /// Act on the Liquify Save/Load Mesh requests the button
    /// handler flagged (they need &mut self beyond the dialog).
    pub(crate) fn liquify_mesh_io(&mut self) {
        let (save, load) = {
            let d = self.dialog.as_mut().unwrap();
            let r = (d.liq_save_req, d.liq_load_req);
            d.liq_save_req = false;
            d.liq_load_req = false;
            r
        };
        if load {
            self.file_panel_purpose = 6;
            self.open_file_panel();
        }
        if save {
            let bytes = {
                let d = self.dialog.as_ref().unwrap();
                let field = pictodesk_core::liquify::LiquifyField {
                    w: self.doc.width,
                    h: self.doc.height,
                    disp: d.liq_disp.clone(),
                };
                pictodesk_core::liquify::field_to_bytes(&field)
            };
            self.mesh_bytes = bytes;
            self.liq_return = self.dialog.take();
            let mut d = Dialog::new(DialogKind::SaveAs, &self.doc);
            d.save_mode = 6;
            d.text = "mesh.pdmesh".into();
            self.dialog = Some(d);
        }
    }

    /// Install a loaded mesh into the open Liquify dialog,
    /// resampling the field when the mesh size differs.
    pub(crate) fn liquify_install_mesh(&mut self, field: pictodesk_core::liquify::LiquifyField) {
        let (w, h) = (self.doc.width, self.doc.height);
        let disp = if field.w == w && field.h == h {
            field.disp
        } else {
            let (sw, sh) = (field.w.max(1) as f32, field.h.max(1) as f32);
            let (xr, yr) = (w as f32 / sw, h as f32 / sh);
            (0..(w * h) as usize)
                .map(|i| {
                    let (x, y) = ((i as u32 % w) as f32 / xr, (i as u32 / w) as f32 / yr);
                    let (x0, y0) = (x.floor(), y.floor());
                    let (fx, fy) = (x - x0, y - y0);
                    let get = |xx: i32, yy: i32| {
                        field
                            .disp
                            .get(
                                (yy.clamp(0, sh as i32 - 1) as usize) * (sw as usize)
                                    + xx.clamp(0, sw as i32 - 1) as usize,
                            )
                            .copied()
                            .unwrap_or((0.0, 0.0))
                    };
                    let (ax, ay) = get(x0 as i32, y0 as i32);
                    let (bx, by) = get(x0 as i32 + 1, y0 as i32);
                    let (cx, cy) = get(x0 as i32, y0 as i32 + 1);
                    let (dx, dy) = get(x0 as i32 + 1, y0 as i32 + 1);
                    (
                        (ax + (bx - ax) * fx + (cx - ax) * fy + (ax - bx - cx + dx) * fx * fy) * xr,
                        (ay + (by - ay) * fx + (cy - ay) * fy + (ay - by - cy + dy) * fx * fy) * yr,
                    )
                })
                .collect()
        };
        if let Some(d) = &mut self.dialog {
            if d.kind == DialogKind::Liquify {
                d.liq_undo.push(d.liq_disp.clone());
                d.liq_disp = disp;
                self.status = "Mesh loaded".into();
            }
        }
    }

    /// One Liquify brush stamp at a doc point.
    pub(crate) fn liquify_stamp(&mut self, dp: (f32, f32)) {
        let Some(d) = &mut self.dialog else { return };
        // Hand tool: drag pans the view.
        if d.liq_tool == 11 {
            if let Some(l) = d.liq_last {
                d.liq_pan.0 -= dp.0 - l.0;
                d.liq_pan.1 -= dp.1 - l.1;
            }
            d.liq_last = Some(dp);
            return;
        }
        // Multiple undo: snapshot the field at each drag start.
        if d.liq_last.is_none() {
            d.liq_undo.push(d.liq_disp.clone());
            if d.liq_undo.len() > 24 {
                d.liq_undo.remove(0);
            }
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let mut field = pictodesk_core::liquify::LiquifyField {
            w,
            h,
            disp: std::mem::take(&mut d.liq_disp),
        };
        let frozen = d.liq_frozen.clone();
        let radius = d.vals[0].max(1.0);
        let pressure = d.vals[1] / 100.0;
        let last = d.liq_last.unwrap_or(dp);
        match d.liq_tool {
            0 => field.stamp_warp(last, dp, radius, pressure, &frozen),
            1 => field.stamp_twirl(dp, radius, pressure, true, &frozen),
            2 => field.stamp_twirl(dp, radius, pressure, false, &frozen),
            3 => field.stamp_pucker(dp, radius, pressure, &frozen),
            4 => field.stamp_bloat(dp, radius, pressure, &frozen),
            5 => field.stamp_shift(last, dp, radius, pressure, &frozen),
            6 => field.stamp_reflect(last, dp, radius, pressure, &frozen),
            7 => field.reconstruct(2, &frozen),
            10 => {
                let seed = d.liq_seed;
                d.liq_seed = d.liq_seed.wrapping_add(1);
                field.stamp_turbulence(dp, radius, pressure, &frozen, seed);
            }
            8 | 9 => {
                // Freeze / Thaw: paint the mask directly.
                let val = if d.liq_tool == 8 { 255u8 } else { 0u8 };
                let (cx, cy) = (dp.0.round() as i32, dp.1.round() as i32);
                let r = radius.round() as i32;
                for yy in (cy - r)..=(cy + r) {
                    for xx in (cx - r)..=(cx + r) {
                        if (xx - cx) * (xx - cx) + (yy - cy) * (yy - cy) <= r * r
                            && xx >= 0
                            && yy >= 0
                            && xx < w as i32
                            && yy < h as i32
                        {
                            d.liq_frozen[(yy as usize) * (w as usize) + xx as usize] = val;
                        }
                    }
                }
            }
            _ => {}
        }
        d.liq_disp = field.disp;
        d.liq_last = Some(dp);
    }
}
