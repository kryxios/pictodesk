//! Menus: items, the menu bar model, dropdowns, and menu clicks.
use super::*;
use crate::actions::{space_name, Action};

#[derive(Clone, Debug)]
pub(crate) struct MenuItem {
    pub(crate) label: String,
    pub(crate) action: Action,
    pub(crate) enabled: bool,
    pub(crate) checked: bool,
    pub(crate) submenu: Option<Vec<MenuItem>>,
    pub(crate) shortcut: Option<String>,
}

/// The platform command-modifier label: Cmd on macOS, Ctrl elsewhere.
pub(crate) fn shortcut_label(k: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("Cmd+{k}")
    } else {
        format!("Ctrl+{k}")
    }
}

impl MenuItem {
    pub(crate) fn new(label: impl Into<String>, action: Action) -> Self {
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
    pub(crate) fn key(mut self, k: &str) -> Self {
        self.shortcut = Some(shortcut_label(k));
        self
    }
    pub(crate) fn sub(mut self, items: Vec<MenuItem>) -> Self {
        self.submenu = Some(items);
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Menu {
    pub(crate) title: &'static str,
    pub(crate) items: Vec<MenuItem>,
}

/// One clickable control in the Options Bar (Phase 6.5
/// Batch 5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OptAct {
    TipSizeDown,
    TipSizeUp,
    TipHardDown,
    TipHardUp,
    ToneExpDown,
    ToneExpUp,
    TolDown,
    TolUp,
    StrokeDown,
    StrokeUp,
    StampSample,
    HealSource,
    FlowDown,
    FlowUp,
    AirbrushToggle,
    EraseHistory,
    DropperSample,
    GradShape,
    ArrowStart,
    ArrowEnd,
}

// Small helpers on MenuItem that read better in the builder above.
pub(crate) trait MenuItemExt {
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

impl App {
    pub(crate) fn build_menus(&self) -> Vec<Menu> {
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
        let mut workspace_items: Vec<MenuItem> = self
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, (n, _))| MenuItem::new(n, Action::ApplyWorkspace(i)))
            .collect();
        if !workspace_items.is_empty() {
            workspace_items.push(MenuItem::new("-", Action::None));
        }
        workspace_items.push(MenuItem::new(
            "Save Workspace...",
            Action::SaveWorkspaceDialog,
        ));
        let workspace_delete: Vec<MenuItem> = self
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, (n, _))| MenuItem::new(n, Action::DeleteWorkspace(i)))
            .collect();
        if !workspace_delete.is_empty() {
            workspace_items
                .push(MenuItem::new("Delete Workspace", Action::None).sub(workspace_delete));
        }
        vec![
            Menu {
                title: "File",
                items: vec![
                    MenuItem::new("New...", Action::New).key("N"),
                    MenuItem::new("Open...", Action::OpenPanel).key("O"),
                    MenuItem::new("Open Recent", Action::None).sub(
                        if self.recent_files.is_empty() {
                            vec![MenuItem::new("(No recent files)", Action::None)]
                        } else {
                            self.recent_files
                                .iter()
                                .enumerate()
                                .map(|(i, p)| {
                                    MenuItem::new(
                                        p.file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_else(|| p.display().to_string()),
                                        Action::OpenRecent(i),
                                    )
                                })
                                .collect()
                        },
                    ),
                    MenuItem::new("Close", Action::CloseTab).key("W"),
                    MenuItem::new("Save", Action::Save).key("S"),
                    MenuItem::new("Save As...", Action::SaveAsPanel),
                    MenuItem::new("Save a Copy...", Action::SaveCopyPanel),
                    MenuItem::new("Export Paths (.ai)...", Action::ExportAi),
                    MenuItem::new("File Info...", Action::FileInfoDialog),
                    MenuItem::new("History Log...", Action::HistoryLog),
                    MenuItem::new("Automate", Action::None).sub(vec![
                        MenuItem::new("Create Droplet...", Action::CreateDroplet),
                        MenuItem::new("Batch...", Action::MacroBatch),
                        MenuItem::new("Conditional Mode Change...", Action::CondModeDialog),
                        MenuItem::new("Contact Sheet II...", Action::ContactSheetDialog),
                        MenuItem::new("Fit Image...", Action::FitImageDialog),
                        MenuItem::new("Picture Package...", Action::PicturePackageDialog),
                        MenuItem::new("Web Photo Gallery...", Action::WebGalleryDialog),
                    ]),
                    MenuItem::new("Export", Action::None).sub(vec![
                        MenuItem::new("PNG...", Action::ExportPng),
                        MenuItem::new("Slices...", Action::ExportSlices)
                            .off_if(self.doc.slices.is_empty()),
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
                    MenuItem::new(
                        match &self.fade_source {
                            Some(fs) => format!("Fade {}...", fs.label),
                            None => "Fade...".to_string(),
                        },
                        Action::FadeDialog,
                    )
                    .off_if(self.fade_source.is_none()),
                    MenuItem::new("Fill...", Action::FillDialog),
                    MenuItem::new("Stroke...", Action::StrokeDialog).off_if(!has_sel),
                    MenuItem::new("Define Pattern", Action::DefinePattern),
                    MenuItem::new("Define Brush", Action::DefineBrush).off_if(!has_sel),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Find and Replace...", Action::FindReplaceDialog),
                    MenuItem::new("Check Spelling...", Action::SpellCheckDialog),
                    MenuItem::new("Define Custom Shape", Action::DefineCustomShape),
                    MenuItem::new("Preset Manager...", Action::PresetManagerDialog),
                    MenuItem::new("Free Transform...", Action::FreeTransform).key("T"),
                    MenuItem::new("Transform Again", Action::TransformAgain).key("Shift+T"),
                    MenuItem::new("Transform Numeric...", Action::FtNumericDialog)
                        .off_if(self.ft.is_none()),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Purge", Action::None).sub(vec![
                        MenuItem::new("Histories", Action::PurgeHistories),
                        MenuItem::new("Clipboard", Action::PurgeClipboard),
                        MenuItem::new("All", Action::PurgeAll),
                    ]),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Preferences...", Action::PreferencesDialog),
                ],
            },
            Menu {
                title: "Image",
                items: vec![
                    MenuItem::new("Image Size...", Action::ImageSizeDialog),
                    MenuItem::new("Canvas Size...", Action::CanvasSizeDialog),
                    MenuItem::new("Duplicate", Action::DuplicateImage),
                    MenuItem::new("Perspective Crop", Action::PerspectiveCrop),
                    MenuItem::new("Crop", Action::CropToSelection).off_if(!has_sel),
                    MenuItem::new("Rotate", Action::None).sub(vec![
                        MenuItem::new("180 deg", Action::Rotate180),
                        MenuItem::new("90 deg CW", Action::Rotate90Cw),
                        MenuItem::new("90 deg CCW", Action::Rotate90Ccw),
                        MenuItem::new("Arbitrary...", Action::RotateArbitraryDialog),
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
                        MenuItem::new("Auto Levels", Action::AutoLevelsCmd),
                        MenuItem::new("Auto Contrast", Action::AutoContrastCmd),
                        MenuItem::new("Auto Color", Action::AutoColorCmd),
                        MenuItem::new("Desaturate", Action::AdjustDialog("desat")),
                        MenuItem::new("Posterize...", Action::AdjustDialog("posterize")),
                        MenuItem::new("Threshold...", Action::AdjustDialog("threshold")),
                        MenuItem::new("Photo Filter...", Action::AdjustDialog("photo")),
                        MenuItem::new("Gradient Map...", Action::AdjustDialog("gradientmap")),
                        MenuItem::new("Channel Mixer...", Action::ChannelMixerDialog),
                        MenuItem::new("Selective Color...", Action::SelectiveColorDialog),
                        MenuItem::new("Replace Color...", Action::ReplaceColorDialog),
                        MenuItem::new("Variations...", Action::VariationsDialog),
                    ]),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Apply Image...", Action::ApplyImageDialog)
                        .off_if(self.doc.mode == ColorMode::Multichannel),
                    MenuItem::new("Calculations...", Action::CalculationsDialog),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Split Channels", Action::SplitChannels),
                    MenuItem::new("Merge Channels...", Action::MergeChannelsDialog)
                        .off_if(self.split_docs.is_empty()),
                    MenuItem::new("Channel Options...", Action::ChannelOptionsDialog)
                        .off_if(self.doc.alphas.is_empty()),
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
                            MenuItem::new("Gradient Map...", Action::NewAdjustment("gradientmap")),
                        ]),
                        MenuItem::new("Edit Adjustment Layer...", Action::EditAdjustment)
                            .off_if(!is_adj),
                        MenuItem::new("Duplicate Layer", Action::DuplicateLayer),
                        MenuItem::new("Delete Layer", Action::DeleteLayer)
                            .off_if(self.doc.layers.len() <= 1),
                        MenuItem::new("Layer Options...", Action::LayerOptions),
                        MenuItem::new("New Fill Layer", Action::None).sub(vec![
                            MenuItem::new("Solid Color...", Action::NewFillLayer(0)),
                            MenuItem::new("Gradient...", Action::NewFillLayer(1)),
                            MenuItem::new("Pattern...", Action::NewFillLayer(2)),
                        ]),
                        MenuItem::new("New Layer Set", Action::NewLayerSet),
                        MenuItem::new("Delete Layer Set", Action::DeleteLayerSet)
                            .off_if(self.doc.layers[a].group == 0),
                        MenuItem::new("Move to Set", Action::None).sub({
                            let mut items = vec![MenuItem::new("None", Action::AssignSet(0))
                                .check_if(self.doc.layers[a].group == 0)];
                            for (i, set) in self.doc.sets.iter().enumerate() {
                                items.push(
                                    MenuItem::new(&set.name, Action::AssignSet((i + 1) as u16))
                                        .check_if(self.doc.layers[a].group == (i + 1) as u16),
                                );
                            }
                            items
                        }),
                        MenuItem::new(
                            if self.doc.layers[a].clipped {
                                "Release Clipping Mask"
                            } else {
                                "Create Clipping Mask"
                            },
                            Action::ToggleClipped,
                        )
                        .key("G")
                        .off_if(a == 0),
                        MenuItem::new("Layer Color", Action::None).sub(vec![
                            MenuItem::new("None", Action::LayerColor(0)),
                            MenuItem::new("Red", Action::LayerColor(1)),
                            MenuItem::new("Orange", Action::LayerColor(2)),
                            MenuItem::new("Yellow", Action::LayerColor(3)),
                            MenuItem::new("Green", Action::LayerColor(4)),
                            MenuItem::new("Blue", Action::LayerColor(5)),
                            MenuItem::new("Violet", Action::LayerColor(6)),
                            MenuItem::new("Gray", Action::LayerColor(7)),
                        ]),
                        MenuItem::new("Lock", Action::None).sub(vec![
                            MenuItem::new("Lock Transparent Pixels", Action::LockToggle(1))
                                .check_if(self.doc.layers[a].locks & 1 != 0),
                            MenuItem::new("Lock Image Pixels", Action::LockToggle(2))
                                .check_if(self.doc.layers[a].locks & 2 != 0),
                            MenuItem::new("Lock Position", Action::LockToggle(4))
                                .check_if(self.doc.layers[a].locks & 4 != 0),
                            MenuItem::new("Lock All", Action::LockToggle(8))
                                .check_if(self.doc.layers[a].locks & 8 != 0),
                        ]),
                        MenuItem::new("Rasterize Shape/Fill Layer", Action::RasterizeLayer).off_if(
                            self.doc.layers[a].shape.is_none() && self.doc.layers[a].fill.is_none(),
                        ),
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
                                MenuItem::new("Satin...", Action::EffectsDialog(5)),
                                MenuItem::new("Color Overlay...", Action::EffectsDialog(6)),
                                MenuItem::new("Gradient Overlay...", Action::EffectsDialog(7)),
                                MenuItem::new("Pattern Overlay...", Action::EffectsDialog(8)),
                                MenuItem::new("Stroke...", Action::EffectsDialog(9)),
                                MenuItem::new("-", Action::None),
                                MenuItem::new("Blending Options...", Action::BlendingDialog),
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
                        MenuItem::new("Matting", Action::None).sub(vec![
                            MenuItem::new("Defringe...", Action::DefringeDialog),
                            MenuItem::new("Remove Black Matte", Action::RemoveBlackMatte),
                            MenuItem::new("Remove White Matte", Action::RemoveWhiteMatte),
                        ]),
                        MenuItem::new("Merge Group", Action::MergeGroup)
                            .off_if(self.doc.layers[a].group == 0),
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
                        MenuItem::new("Lab Color", Action::ModeLab)
                            .check_if(self.doc.mode == ColorMode::Lab),
                        MenuItem::new("Duotone...", Action::DuotoneDialog)
                            .check_if(self.doc.mode == ColorMode::Duotone),
                        MenuItem::new("Multichannel", Action::ModeMultichannel)
                            .check_if(self.doc.mode == ColorMode::Multichannel),
                        MenuItem::new("-", Action::None),
                        MenuItem::new("Indexed Color...", Action::IndexedDialog)
                            .check_if(self.doc.mode == ColorMode::Indexed),
                        MenuItem::new("Bitmap...", Action::BitmapDialog)
                            .check_if(self.doc.mode == ColorMode::Bitmap),
                        MenuItem::new("Color Table...", Action::ColorTableDialog),
                        MenuItem::new("-", Action::None),
                        MenuItem::new("Color Settings...", Action::ColorSettingsDialog),
                        MenuItem::new("Assign Profile", Action::None).sub(assign),
                        MenuItem::new("Convert to Profile", Action::None).sub(convert),
                        MenuItem::new("Proof Colors (CMYK)", Action::ToggleProof)
                            .check_if(self.proof_cmyk),
                        MenuItem::new("Gamut Warning", Action::ToggleGamut)
                            .check_if(self.gamut_warning),
                        MenuItem::new("Show Path", Action::ToggleShowPath).check_if(self.show_path),
                        MenuItem::new("Show Selection Edges", Action::TogglePathEdges)
                            .check_if(self.show_edges),
                    ]
                },
            },
            Menu {
                title: "Filter",
                items: vec![
                    MenuItem::new(
                        match &self.last_filter_step {
                            Some(st) => format!("Last Filter: {}", st.label()),
                            None => "Last Filter".to_string(),
                        },
                        Action::LastFilter,
                    )
                    .off_if(self.last_filter_step.is_none())
                    .key("^F"),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Extract...", Action::ExtractDialog),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Artistic", Action::None).sub(vec![
                        MenuItem::new("Colored Pencil...", Action::LegacyFilterDialog(29)),
                        MenuItem::new("Cutout...", Action::LegacyFilterDialog(30)),
                        MenuItem::new("Dry Brush...", Action::LegacyFilterDialog(31)),
                        MenuItem::new("Film Grain...", Action::LegacyFilterDialog(32)),
                        MenuItem::new("Fresco...", Action::LegacyFilterDialog(33)),
                        MenuItem::new("Neon Glow...", Action::LegacyFilterDialog(34)),
                        MenuItem::new("Paint Daubs...", Action::LegacyFilterDialog(35)),
                        MenuItem::new("Palette Knife...", Action::LegacyFilterDialog(36)),
                        MenuItem::new("Plastic Wrap...", Action::LegacyFilterDialog(37)),
                        MenuItem::new("Poster Edges...", Action::LegacyFilterDialog(38)),
                        MenuItem::new("Rough Pastels...", Action::LegacyFilterDialog(39)),
                        MenuItem::new("Smudge Stick...", Action::LegacyFilterDialog(40)),
                        MenuItem::new("Sponge...", Action::LegacyFilterDialog(41)),
                        MenuItem::new("Underpainting...", Action::LegacyFilterDialog(42)),
                        MenuItem::new("Watercolor...", Action::LegacyFilterDialog(43)),
                    ]),
                    MenuItem::new("Liquify...", Action::LiquifyDialog).key("X"),
                    MenuItem::new("Pattern Maker...", Action::PatternMakerDialog),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("Brush Strokes", Action::None).sub(vec![
                        MenuItem::new("Accented Edges...", Action::LegacyFilterDialog(44)),
                        MenuItem::new("Angled Strokes...", Action::LegacyFilterDialog(45)),
                        MenuItem::new("Crosshatch...", Action::LegacyFilterDialog(46)),
                        MenuItem::new("Dark Strokes...", Action::LegacyFilterDialog(47)),
                        MenuItem::new("Ink Outlines...", Action::LegacyFilterDialog(48)),
                        MenuItem::new("Spatter...", Action::LegacyFilterDialog(49)),
                        MenuItem::new("Sprayed Strokes...", Action::LegacyFilterDialog(50)),
                        MenuItem::new("Sumi-e...", Action::LegacyFilterDialog(51)),
                    ]),
                    MenuItem::new("Blur", Action::None).sub(vec![
                        MenuItem::new("Blur", Action::FilterSpecial(4)),
                        MenuItem::new("Blur More", Action::FilterSpecial(5)),
                        MenuItem::new("Gaussian Blur...", Action::FilterGaussian),
                        MenuItem::new("Motion Blur...", Action::FilterMotion),
                        MenuItem::new("Radial Blur...", Action::FilterRadial),
                        MenuItem::new("Smart Blur...", Action::LegacyFilterDialog(21)),
                    ]),
                    MenuItem::new("Distort", Action::None).sub(vec![
                        MenuItem::new("Pinch...", Action::LegacyFilterDialog(0)),
                        MenuItem::new("Spherize...", Action::LegacyFilterDialog(1)),
                        MenuItem::new("Twirl...", Action::LegacyFilterDialog(2)),
                        MenuItem::new("Ripple...", Action::LegacyFilterDialog(3)),
                        MenuItem::new("Ocean Ripple...", Action::LegacyFilterDialog(4)),
                        MenuItem::new("Shear...", Action::LegacyFilterDialog(5)),
                        MenuItem::new("ZigZag...", Action::LegacyFilterDialog(6)),
                        MenuItem::new("Wave...", Action::LegacyFilterDialog(23)),
                        MenuItem::new("Glass...", Action::LegacyFilterDialog(24)),
                        MenuItem::new("Diffuse Glow...", Action::LegacyFilterDialog(25)),
                        MenuItem::new("Displace...", Action::LegacyFilterDialog(26)),
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
                        MenuItem::new("Color Halftone...", Action::LegacyFilterDialog(18)),
                        MenuItem::new("Crystallize...", Action::LegacyFilterDialog(19)),
                        MenuItem::new("Facet", Action::FilterSpecial(3)),
                        MenuItem::new("Fragment", Action::FilterSpecial(2)),
                        MenuItem::new("Mezzotint", Action::None).sub(vec![
                            MenuItem::new("Fine Dots", Action::FilterMezzotint(0)),
                            MenuItem::new("Medium Dots", Action::FilterMezzotint(1)),
                            MenuItem::new("Grainy Dots", Action::FilterMezzotint(2)),
                            MenuItem::new("Coarse Lines", Action::FilterMezzotint(3)),
                        ]),
                        MenuItem::new("Mosaic...", Action::FilterMosaic),
                        MenuItem::new("Pointillize...", Action::LegacyFilterDialog(20)),
                    ]),
                    MenuItem::new("Render", Action::None).sub(vec![
                        MenuItem::new("Clouds", Action::FilterClouds),
                        MenuItem::new("Difference Clouds", Action::FilterSpecial(0)),
                        MenuItem::new("3D Transform...", Action::FilterTransform3d),
                        MenuItem::new("Lens Flare...", Action::LegacyFilterDialog(27)),
                        MenuItem::new("Lighting Effects...", Action::LegacyFilterDialog(28)),
                        MenuItem::new("Texture Fill...", Action::TextureFillPick),
                    ]),
                    MenuItem::new("Sharpen", Action::None).sub(vec![
                        MenuItem::new("Sharpen", Action::FilterSharpen),
                        MenuItem::new("Sharpen Edges", Action::FilterSpecial(1)),
                        MenuItem::new("Sharpen More", Action::FilterSpecial(6)),
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
                        MenuItem::new("Extrude...", Action::LegacyFilterDialog(16)),
                        MenuItem::new("Find Edges", Action::FilterFindEdges),
                        MenuItem::new("Glowing Edges...", Action::LegacyFilterDialog(11)),
                        MenuItem::new("Solarize", Action::FilterSolarize),
                        MenuItem::new("Tiles...", Action::LegacyFilterDialog(10)),
                        MenuItem::new("Trace Contour...", Action::LegacyFilterDialog(17)),
                        MenuItem::new("Wind...", Action::LegacyFilterDialog(15)),
                    ]),
                    MenuItem::new("Sketch", Action::None).sub(vec![
                        MenuItem::new("Chalk & Charcoal...", Action::LegacyFilterDialog(52)),
                        MenuItem::new("Charcoal...", Action::LegacyFilterDialog(53)),
                        MenuItem::new("Chrome...", Action::LegacyFilterDialog(54)),
                        MenuItem::new("Conte Crayon...", Action::LegacyFilterDialog(55)),
                        MenuItem::new("Graphic Pen...", Action::LegacyFilterDialog(56)),
                        MenuItem::new("Halftone Pattern...", Action::LegacyFilterDialog(57)),
                        MenuItem::new("Note Paper...", Action::LegacyFilterDialog(58)),
                        MenuItem::new("Photocopy...", Action::LegacyFilterDialog(59)),
                        MenuItem::new("Plaster...", Action::LegacyFilterDialog(60)),
                        MenuItem::new("Reticulation...", Action::LegacyFilterDialog(61)),
                        MenuItem::new("Torn Edges...", Action::LegacyFilterDialog(62)),
                        MenuItem::new("Water Paper...", Action::LegacyFilterDialog(63)),
                    ]),
                    MenuItem::new("Texture", Action::None).sub(vec![
                        MenuItem::new("Craquelure...", Action::LegacyFilterDialog(64)),
                        MenuItem::new("Grain...", Action::LegacyFilterDialog(65)),
                        MenuItem::new("Mosaic Tiles...", Action::LegacyFilterDialog(66)),
                        MenuItem::new("Patchwork...", Action::LegacyFilterDialog(12)),
                        MenuItem::new("Stained Glass...", Action::LegacyFilterDialog(67)),
                        MenuItem::new("Texturizer...", Action::LegacyFilterDialog(68)),
                    ]),
                    MenuItem::new("Video", Action::None).sub(vec![
                        MenuItem::new("De-Interlace...", Action::LegacyFilterDialog(22)),
                        MenuItem::new("NTSC Colors", Action::FilterSpecial(7)),
                    ]),
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
                    items.push(
                        MenuItem::new("Load Selection...", Action::LoadSelectionDialog)
                            .off_if(self.doc.alphas.is_empty()),
                    );
                    items.push(MenuItem::new("-", Action::None));
                    items.push(
                        MenuItem::new("Quick Mask", Action::QuickMaskToggle)
                            .key("Q")
                            .check_if(self.quick_mask),
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
                    MenuItem::new("Tool Presets", Action::ToggleToolPresets)
                        .check_if(self.show_tool_presets),
                    MenuItem::new("-", Action::None),
                    MenuItem::new("File Browser...", Action::FileBrowserDialog),
                    MenuItem::new("Workspace", Action::None).sub(workspace_items),
                    MenuItem::new("Rulers", Action::ToggleRulers).check_if(self.show_rulers),
                    MenuItem::new("Options Bar", Action::ToggleOptionsBar)
                        .check_if(self.show_options_bar),
                    MenuItem::new("Screen Mode", Action::None).sub(vec![
                        MenuItem::new("Standard", Action::ScreenMode(0))
                            .check_if(self.display_mode == 0),
                        MenuItem::new("Full + Menu Bar", Action::ScreenMode(1))
                            .check_if(self.display_mode == 1)
                            .key("F"),
                        MenuItem::new("Full Screen", Action::ScreenMode(2))
                            .check_if(self.display_mode == 2),
                    ]),
                    MenuItem::new("Guides", Action::ToggleGuides).check_if(self.show_guides),
                    MenuItem::new("Lock Guides", Action::ToggleLockGuides)
                        .check_if(self.guides_locked),
                    MenuItem::new("Grid", Action::ToggleGrid).check_if(self.show_grid),
                    MenuItem::new("Snap", Action::ToggleSnap).check_if(self.snap_enabled),
                    MenuItem::new("Clear Guides", Action::ClearGuides)
                        .off_if(self.doc.guides_x.is_empty() && self.doc.guides_y.is_empty()),
                    MenuItem::new("Zoom In", Action::ZoomIn).key("+"),
                    MenuItem::new("Zoom Out", Action::ZoomOut).key("-"),
                    MenuItem::new("Actual Pixels", Action::Zoom100).key("1"),
                    MenuItem::new("Print Size", Action::ZoomPrintSize),
                    MenuItem::new("Fit on Screen", Action::ZoomFit).key("0"),
                ],
            },
        ]
    }

    /// Every menu leaf as a "Menu > Item" path with its action
    /// (Phase 6.5 Batch 6d: Insert Menu Item + playback lookup).
    pub(crate) fn flatten_menu_items(&self) -> Vec<(String, Action)> {
        fn walk(items: &[MenuItem], prefix: &str, out: &mut Vec<(String, Action)>) {
            for it in items {
                if it.label == "-" {
                    continue;
                }
                let path = if prefix.is_empty() {
                    it.label.clone()
                } else {
                    format!("{prefix} > {}", it.label)
                };
                if let Some(sub) = &it.submenu {
                    walk(sub, &path, out);
                } else if it.action != Action::None {
                    out.push((path, it.action.clone()));
                }
            }
        }
        let mut out = Vec::new();
        for m in self.build_menus() {
            walk(&m.items, m.title, &mut out);
        }
        out
    }

    pub(crate) fn menu_action_for_path(&self, path: &str) -> Option<Action> {
        self.flatten_menu_items()
            .into_iter()
            .find(|(p, _)| p == path)
            .map(|(_, a)| a)
    }

    pub(crate) fn menu_title_rects(&self) -> Vec<(Rect, usize)> {
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

    pub(crate) fn dropdown_rect(&self, menu_idx: usize) -> Rect {
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

    /// Returns true if the click landed in the open menu system.
    pub(crate) fn menu_click(&mut self, mx: i32, my: i32) -> bool {
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
        if self.display_mode < 2 && my < ui::MENU_H {
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
}
