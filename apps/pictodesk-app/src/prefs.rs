//! Preferences: the Photoshop 7.0 settings, as audited in
//! `~/workspace/photoshop-competitor/PS70-PREFS-AUDIT.md`.
//!
//! The dialog is data-driven: `PREF_PAGES` lists the panes and
//! `PREF_ROWS` every row on every pane. The dialog keeps a draft
//! in four vectors (one per row kind); opening the dialog fills
//! them from the app, OK writes them back. The old General
//! settings (tooltips, history states, ...) live in the same rows
//! — they read/write the App fields they always did — and the new
//! ones live in `Prefs` on the App.

use crate::ui::Rgb;

/// Named color presets for the cycle controls that pick a color
/// (guide, grid, slice, gamut). The swatch beside the value shows
/// the color; the name is the value.
pub const COLOR_PRESETS: [(&str, Rgb); 8] = [
    ("Gray", (160, 160, 160)),
    ("Light gray", (176, 176, 176)),
    ("Red", (255, 0, 0)),
    ("Orange", (255, 128, 0)),
    ("Yellow", (255, 255, 0)),
    ("Green", (0, 200, 0)),
    ("Cyan", (0, 200, 200)),
    ("Blue", (80, 120, 255)),
];

/// Transparency checkerboard color pairs (light cell, dark cell).
pub const CHECKER_PRESETS: [(&str, (Rgb, Rgb)); 4] = [
    ("Light", ((255, 255, 255), (192, 192, 192))),
    ("Medium", ((204, 204, 204), (153, 153, 153))),
    ("Dark", ((128, 128, 128), (96, 96, 96))),
    ("Blue", ((180, 200, 255), (120, 140, 200))),
];

/// The new settings (the old six — history states, auto snapshot,
/// options bar, tooltips, scroll zoom, default space — stay as App
/// fields and are read/written by the same rows).
#[derive(Clone, Debug)]
pub struct Prefs {
    // General
    pub interp: u8,   // 0 Nearest, 1 Bilinear, 2 Bicubic
    pub redo_key: u8, // 0 Ctrl+Z toggles, 1 Ctrl+Alt+Z steps back
    pub beep_done: bool,
    pub dyn_sliders: bool,
    pub save_palette_locs: bool,
    pub shift_tool_switch: bool,
    pub paste_resize: bool,
    // File Handling
    pub img_previews: u8,  // 0 Never, 1 Always Save, 2 Ask When Saving
    pub file_ext_case: u8, // 0 lower, 1 UPPER
    pub recent_count: u8,  // 0..=30
    // Display & Cursors
    pub channels_in_color: bool,
    pub pixel_doubling: bool,
    pub paint_cursor: u8, // 0 Standard, 1 Precise, 2 Brush Size
    pub brush_crosshair: bool,
    pub other_cursor: u8, // 0 Standard, 1 Precise
    // Transparency & Gamut
    pub grid_size: u8,     // 0 None, 1 Small, 2 Medium, 3 Large
    pub grid_colors: u8,   // index into CHECKER_PRESETS
    pub gamut_color: u8,   // index into COLOR_PRESETS
    pub gamut_opacity: u8, // 0..=100
    // Units & Rulers
    pub ruler_units: u8, // 0 Pixels, 1 Inches, 2 cm, 3 mm, 4 Points, 5 Picas, 6 Percent
    pub type_units: u8,  // 0 Points, 1 Pixels, 2 Millimeters
    pub new_print_res: u16, // dpi
    pub new_screen_res: u16, // dpi
    pub point_pica: u8,  // 0 PostScript 72, 1 Traditional 72.27
    // Guides, Grid & Slices
    pub guide_color: u8, // index into COLOR_PRESETS
    pub guide_style: u8, // 0 Lines, 1 Dashed Lines
    pub grid_color: u8,  // index into COLOR_PRESETS
    pub grid_style: u8,  // 0 Lines, 1 Dashed Lines, 2 Dots
    pub grid_every: u16, // px between gridlines
    pub grid_subdiv: u8,
    pub slice_color: u8, // index into COLOR_PRESETS
    pub slice_numbers: bool,
    // Memory & Image Cache
    pub ram_pct: u8,      // 5..=100
    pub cache_levels: u8, // 1..=8
    pub cache_histograms: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            interp: 2,
            redo_key: 0,
            beep_done: true,
            dyn_sliders: true,
            save_palette_locs: true,
            shift_tool_switch: false,
            paste_resize: true,
            img_previews: 2,
            file_ext_case: 0,
            recent_count: 4,
            channels_in_color: false,
            pixel_doubling: false,
            paint_cursor: 2,
            brush_crosshair: false,
            other_cursor: 0,
            grid_size: 2,
            grid_colors: 0,
            gamut_color: 0,
            gamut_opacity: 100,
            ruler_units: 0,
            type_units: 0,
            new_print_res: 300,
            new_screen_res: 72,
            point_pica: 0,
            guide_color: 6,
            guide_style: 0,
            grid_color: 1,
            grid_style: 0,
            grid_every: 64,
            grid_subdiv: 1,
            slice_color: 6,
            slice_numbers: true,
            ram_pct: 75,
            cache_levels: 4,
            cache_histograms: true,
        }
    }
}

/// One row of the dialog. `Toggle` and `Cycle` read/write the
/// dialog's draft vectors by their position in `PREF_ROWS`
/// (toggles in order, cycles in order, sliders in order); the
/// load/apply helpers below do the mapping, so no row ever holds
/// a raw index.
#[derive(Clone, Copy)]
pub enum PrefKind {
    Toggle(PrefBool),
    Cycle(PrefCycle, &'static [&'static str], bool), // options, draw-swatch?
    Slider(PrefNum, f32, f32, &'static str),         // key, min, max, unit
}

#[derive(Clone, Copy, PartialEq)]
pub enum PrefBool {
    Tooltips,
    BeepDone,
    DynSliders,
    SavePaletteLocs,
    ShiftToolSwitch,
    PasteResize,
    AutoSnapshot,
    OptionsBar,
    ZoomScroll,
    ChannelsInColor,
    PixelDoubling,
    BrushCrosshair,
    SliceNumbers,
    CacheHistograms,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PrefCycle {
    Interp,
    RedoKey,
    DefaultSpace,
    ImgPreviews,
    FileExtCase,
    PaintCursor,
    OtherCursor,
    GridSize,
    GridColors,
    RulerUnits,
    TypeUnits,
    PointPica,
    GuideColor,
    GuideStyle,
    GridColor,
    GridStyle,
    SliceColor,
    GamutColor,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PrefNum {
    HistStates,
    RecentCount,
    GamutOpacity,
    NewPrintRes,
    NewScreenRes,
    GridEvery,
    GridSubdiv,
    RamPct,
    CacheLevels,
}

#[derive(Clone, Copy)]
pub struct PrefRowDef {
    pub page: usize,
    pub label: &'static str,
    pub kind: PrefKind,
}

/// Pane names in order; the Shortcuts page is last (index 7).
pub const PREF_PAGES: [&str; 8] = [
    "General",
    "File Handling",
    "Display & Cursors",
    "Transparency & Gamut",
    "Units & Rulers",
    "Guides, Grid & Slices",
    "Memory & Image Cache",
    "Shortcuts",
];

const CYCLE_COLORS: [&str; 8] = [
    "Gray",
    "Light gray",
    "Red",
    "Orange",
    "Yellow",
    "Green",
    "Cyan",
    "Blue",
];

/// Every settings row on every pane, in dialog order. The dialog
/// draft vectors follow this order within each kind.
pub static PREF_ROWS: &[PrefRowDef] = &[
    // General
    PrefRowDef {
        page: 0,
        label: "Image Interpolation",
        kind: PrefKind::Cycle(
            PrefCycle::Interp,
            &["Nearest Neighbor", "Bilinear", "Bicubic"],
            false,
        ),
    },
    PrefRowDef {
        page: 0,
        label: "Redo Key",
        kind: PrefKind::Cycle(PrefCycle::RedoKey, &["Ctrl+Z", "Ctrl+Alt+Z"], false),
    },
    PrefRowDef {
        page: 0,
        label: "History States",
        kind: PrefKind::Slider(PrefNum::HistStates, 1.0, 100.0, ""),
    },
    PrefRowDef {
        page: 0,
        label: "Show Tool Tips",
        kind: PrefKind::Toggle(PrefBool::Tooltips),
    },
    PrefRowDef {
        page: 0,
        label: "Beep When Done",
        kind: PrefKind::Toggle(PrefBool::BeepDone),
    },
    PrefRowDef {
        page: 0,
        label: "Dynamic Color Sliders",
        kind: PrefKind::Toggle(PrefBool::DynSliders),
    },
    PrefRowDef {
        page: 0,
        label: "Save Palette Locations",
        kind: PrefKind::Toggle(PrefBool::SavePaletteLocs),
    },
    PrefRowDef {
        page: 0,
        label: "Use Shift Key for Tool Switch",
        kind: PrefKind::Toggle(PrefBool::ShiftToolSwitch),
    },
    PrefRowDef {
        page: 0,
        label: "Resize Image During Paste/Place",
        kind: PrefKind::Toggle(PrefBool::PasteResize),
    },
    PrefRowDef {
        page: 0,
        label: "Auto Snapshot",
        kind: PrefKind::Toggle(PrefBool::AutoSnapshot),
    },
    PrefRowDef {
        page: 0,
        label: "Options Bar",
        kind: PrefKind::Toggle(PrefBool::OptionsBar),
    },
    PrefRowDef {
        page: 0,
        label: "Scroll Wheel Zooms",
        kind: PrefKind::Toggle(PrefBool::ZoomScroll),
    },
    PrefRowDef {
        page: 0,
        label: "Default Working Space",
        kind: PrefKind::Cycle(
            PrefCycle::DefaultSpace,
            &["sRGB", "Adobe RGB", "ProPhoto"],
            false,
        ),
    },
    // File Handling
    PrefRowDef {
        page: 1,
        label: "Image Previews",
        kind: PrefKind::Cycle(
            PrefCycle::ImgPreviews,
            &["Never Save", "Always Save", "Ask When Saving"],
            false,
        ),
    },
    PrefRowDef {
        page: 1,
        label: "File Extension",
        kind: PrefKind::Cycle(PrefCycle::FileExtCase, &["Lower Case", "Upper Case"], false),
    },
    PrefRowDef {
        page: 1,
        label: "Recent File List",
        kind: PrefKind::Slider(PrefNum::RecentCount, 0.0, 30.0, "files"),
    },
    // Display & Cursors
    PrefRowDef {
        page: 2,
        label: "Color Channels in Color",
        kind: PrefKind::Toggle(PrefBool::ChannelsInColor),
    },
    PrefRowDef {
        page: 2,
        label: "Use Pixel Doubling",
        kind: PrefKind::Toggle(PrefBool::PixelDoubling),
    },
    PrefRowDef {
        page: 2,
        label: "Painting Cursors",
        kind: PrefKind::Cycle(
            PrefCycle::PaintCursor,
            &["Standard", "Precise", "Brush Size"],
            false,
        ),
    },
    PrefRowDef {
        page: 2,
        label: "Show Crosshair in Brush Tip",
        kind: PrefKind::Toggle(PrefBool::BrushCrosshair),
    },
    PrefRowDef {
        page: 2,
        label: "Other Cursors",
        kind: PrefKind::Cycle(PrefCycle::OtherCursor, &["Standard", "Precise"], false),
    },
    // Transparency & Gamut
    PrefRowDef {
        page: 3,
        label: "Grid Size",
        kind: PrefKind::Cycle(
            PrefCycle::GridSize,
            &["None", "Small", "Medium", "Large"],
            false,
        ),
    },
    PrefRowDef {
        page: 3,
        label: "Grid Colors",
        kind: PrefKind::Cycle(
            PrefCycle::GridColors,
            &["Light", "Medium", "Dark", "Blue"],
            false,
        ),
    },
    PrefRowDef {
        page: 3,
        label: "Gamut Warning Color",
        kind: PrefKind::Cycle(PrefCycle::GamutColor, &CYCLE_COLORS, true),
    },
    PrefRowDef {
        page: 3,
        label: "Gamut Opacity",
        kind: PrefKind::Slider(PrefNum::GamutOpacity, 0.0, 100.0, "%"),
    },
    // Units & Rulers
    PrefRowDef {
        page: 4,
        label: "Rulers",
        kind: PrefKind::Cycle(
            PrefCycle::RulerUnits,
            &[
                "Pixels",
                "Inches",
                "Centimeters",
                "Millimeters",
                "Points",
                "Picas",
                "Percent",
            ],
            false,
        ),
    },
    PrefRowDef {
        page: 4,
        label: "Type",
        kind: PrefKind::Cycle(
            PrefCycle::TypeUnits,
            &["Points", "Pixels", "Millimeters"],
            false,
        ),
    },
    PrefRowDef {
        page: 4,
        label: "Print Resolution",
        kind: PrefKind::Slider(PrefNum::NewPrintRes, 72.0, 600.0, "dpi"),
    },
    PrefRowDef {
        page: 4,
        label: "Screen Resolution",
        kind: PrefKind::Slider(PrefNum::NewScreenRes, 72.0, 300.0, "dpi"),
    },
    PrefRowDef {
        page: 4,
        label: "Point/Pica Size",
        kind: PrefKind::Cycle(
            PrefCycle::PointPica,
            &["PostScript (72)", "Traditional (72.27)"],
            false,
        ),
    },
    // Guides, Grid & Slices
    PrefRowDef {
        page: 5,
        label: "Guide Color",
        kind: PrefKind::Cycle(PrefCycle::GuideColor, &CYCLE_COLORS, true),
    },
    PrefRowDef {
        page: 5,
        label: "Guide Style",
        kind: PrefKind::Cycle(PrefCycle::GuideStyle, &["Lines", "Dashed Lines"], false),
    },
    PrefRowDef {
        page: 5,
        label: "Grid Color",
        kind: PrefKind::Cycle(PrefCycle::GridColor, &CYCLE_COLORS, true),
    },
    PrefRowDef {
        page: 5,
        label: "Grid Style",
        kind: PrefKind::Cycle(
            PrefCycle::GridStyle,
            &["Lines", "Dashed Lines", "Dots"],
            false,
        ),
    },
    PrefRowDef {
        page: 5,
        label: "Gridline Every",
        kind: PrefKind::Slider(PrefNum::GridEvery, 1.0, 1000.0, "px"),
    },
    PrefRowDef {
        page: 5,
        label: "Subdivisions",
        kind: PrefKind::Slider(PrefNum::GridSubdiv, 1.0, 100.0, ""),
    },
    PrefRowDef {
        page: 5,
        label: "Slice Line Color",
        kind: PrefKind::Cycle(PrefCycle::SliceColor, &CYCLE_COLORS, true),
    },
    PrefRowDef {
        page: 5,
        label: "Show Slice Numbers",
        kind: PrefKind::Toggle(PrefBool::SliceNumbers),
    },
    // Memory & Image Cache
    PrefRowDef {
        page: 6,
        label: "Used by PictoDesk",
        kind: PrefKind::Slider(PrefNum::RamPct, 5.0, 100.0, "%"),
    },
    PrefRowDef {
        page: 6,
        label: "Image Cache Levels",
        kind: PrefKind::Slider(PrefNum::CacheLevels, 1.0, 8.0, ""),
    },
    PrefRowDef {
        page: 6,
        label: "Use Cache for Histograms",
        kind: PrefKind::Toggle(PrefBool::CacheHistograms),
    },
];

/// Rows on one pane, in order.
pub fn prefs_page_rows(page: usize) -> Vec<&'static PrefRowDef> {
    PREF_ROWS.iter().filter(|r| r.page == page).collect()
}

/// The non-slider rows on one pane (the clickable ones).
pub fn prefs_page_buttons(page: usize) -> Vec<&'static PrefRowDef> {
    prefs_page_rows(page)
        .into_iter()
        .filter(|r| !matches!(r.kind, PrefKind::Slider(..)))
        .collect()
}

/// The sliders on one pane, in order (drives slider_defs/vals).
pub fn prefs_page_sliders(page: usize) -> Vec<&'static PrefRowDef> {
    prefs_page_rows(page)
        .into_iter()
        .filter(|r| matches!(r.kind, PrefKind::Slider(..)))
        .collect()
}

impl Prefs {
    /// One prefs.txt line per new setting (`key=value`).
    pub fn to_lines(&self) -> String {
        format!(
            "interp={}\nredo_key={}\nbeep_done={}\ndyn_sliders={}\nsave_palette_locs={}\nshift_tool_switch={}\npaste_resize={}\nimg_previews={}\nfile_ext_case={}\nrecent_count={}\nchannels_in_color={}\npixel_doubling={}\npaint_cursor={}\nbrush_crosshair={}\nother_cursor={}\ngrid_size={}\ngrid_colors={}\ngamut_color={}\ngamut_opacity={}\nruler_units={}\ntype_units={}\nnew_print_res={}\nnew_screen_res={}\npoint_pica={}\nguide_color={}\nguide_style={}\ngrid_color={}\ngrid_style={}\ngrid_every={}\ngrid_subdiv={}\nslice_color={}\nslice_numbers={}\nram_pct={}\ncache_levels={}\ncache_histograms={}\n",
            self.interp,
            self.redo_key,
            self.beep_done as u8,
            self.dyn_sliders as u8,
            self.save_palette_locs as u8,
            self.shift_tool_switch as u8,
            self.paste_resize as u8,
            self.img_previews,
            self.file_ext_case,
            self.recent_count,
            self.channels_in_color as u8,
            self.pixel_doubling as u8,
            self.paint_cursor,
            self.brush_crosshair as u8,
            self.other_cursor,
            self.grid_size,
            self.grid_colors,
            self.gamut_color,
            self.gamut_opacity,
            self.ruler_units,
            self.type_units,
            self.new_print_res,
            self.new_screen_res,
            self.point_pica,
            self.guide_color,
            self.guide_style,
            self.grid_color,
            self.grid_style,
            self.grid_every,
            self.grid_subdiv,
            self.slice_color,
            self.slice_numbers as u8,
            self.ram_pct,
            self.cache_levels,
            self.cache_histograms as u8,
        )
    }

    /// Parse one `key=value` line; unknown keys are ignored so old
    /// prefs.txt files keep loading.
    pub fn apply_line(&mut self, k: &str, v: &str) {
        let n = |d: u8| v.trim().parse::<u8>().unwrap_or(d);
        let n16 = |d: u16| v.trim().parse::<u16>().unwrap_or(d);
        let b = v.trim() == "1";
        match k {
            "interp" => self.interp = n(self.interp).min(2),
            "redo_key" => self.redo_key = n(self.redo_key).min(1),
            "beep_done" => self.beep_done = b,
            "dyn_sliders" => self.dyn_sliders = b,
            "save_palette_locs" => self.save_palette_locs = b,
            "shift_tool_switch" => self.shift_tool_switch = b,
            "paste_resize" => self.paste_resize = b,
            "img_previews" => self.img_previews = n(self.img_previews).min(2),
            "file_ext_case" => self.file_ext_case = n(self.file_ext_case).min(1),
            "recent_count" => self.recent_count = n(self.recent_count).min(30),
            "channels_in_color" => self.channels_in_color = b,
            "pixel_doubling" => self.pixel_doubling = b,
            "paint_cursor" => self.paint_cursor = n(self.paint_cursor).min(2),
            "brush_crosshair" => self.brush_crosshair = b,
            "other_cursor" => self.other_cursor = n(self.other_cursor).min(1),
            "grid_size" => self.grid_size = n(self.grid_size).min(3),
            "grid_colors" => self.grid_colors = n(self.grid_colors).min(3),
            "gamut_color" => self.gamut_color = n(self.gamut_color).min(7),
            "gamut_opacity" => self.gamut_opacity = n(self.gamut_opacity).min(100),
            "ruler_units" => self.ruler_units = n(self.ruler_units).min(6),
            "type_units" => self.type_units = n(self.type_units).min(2),
            "new_print_res" => self.new_print_res = n16(self.new_print_res).clamp(72, 1200),
            "new_screen_res" => self.new_screen_res = n16(self.new_screen_res).clamp(72, 1200),
            "point_pica" => self.point_pica = n(self.point_pica).min(1),
            "guide_color" => self.guide_color = n(self.guide_color).min(7),
            "guide_style" => self.guide_style = n(self.guide_style).min(1),
            "grid_color" => self.grid_color = n(self.grid_color).min(7),
            "grid_style" => self.grid_style = n(self.grid_style).min(2),
            "grid_every" => self.grid_every = n16(self.grid_every).clamp(1, 2000),
            "grid_subdiv" => self.grid_subdiv = n(self.grid_subdiv).clamp(1, 100),
            "slice_color" => self.slice_color = n(self.slice_color).min(7),
            "slice_numbers" => self.slice_numbers = b,
            "ram_pct" => self.ram_pct = n(self.ram_pct).clamp(5, 100),
            "cache_levels" => self.cache_levels = n(self.cache_levels).clamp(1, 8),
            "cache_histograms" => self.cache_histograms = b,
            _ => {}
        }
    }

    /// Checkerboard cell size in px (None = 0, no checkerboard).
    pub fn checker_cell(&self) -> i32 {
        match self.grid_size {
            0 => 0,
            1 => 4,
            2 => 8,
            _ => 16,
        }
    }

    /// Checkerboard colors (light cell, dark cell).
    pub fn checker_colors(&self) -> (Rgb, Rgb) {
        CHECKER_PRESETS[self.grid_colors as usize % CHECKER_PRESETS.len()].1
    }

    /// Points per inch for the Point/Pica Size setting.
    pub fn points_per_inch(&self) -> f32 {
        if self.point_pica == 1 {
            72.27
        } else {
            72.0
        }
    }

    /// The gamut overlay color at the set opacity over gray 160.
    pub fn gamut_overlay(&self) -> (u8, u8, u8) {
        let (r, g, b) = COLOR_PRESETS[self.gamut_color as usize % COLOR_PRESETS.len()].1;
        let a = self.gamut_opacity as f32 / 100.0;
        (
            (r as f32 * a + 160.0 * (1.0 - a)) as u8,
            (g as f32 * a + 160.0 * (1.0 - a)) as u8,
            (b as f32 * a + 160.0 * (1.0 - a)) as u8,
        )
    }
}

/// Position of one row within its kind's draft vector (the draft
/// vectors follow PREF_ROWS order within each kind).
pub fn prefs_toggle_idx(row: &PrefRowDef) -> usize {
    PREF_ROWS
        .iter()
        .take_while(|r| !std::ptr::eq(*r, row))
        .filter(|r| matches!(r.kind, PrefKind::Toggle(_)))
        .count()
}

pub fn prefs_cycle_idx(row: &PrefRowDef) -> usize {
    PREF_ROWS
        .iter()
        .take_while(|r| !std::ptr::eq(*r, row))
        .filter(|r| matches!(r.kind, PrefKind::Cycle(..)))
        .count()
}

pub fn prefs_slider_idx(row: &PrefRowDef) -> usize {
    PREF_ROWS
        .iter()
        .take_while(|r| !std::ptr::eq(*r, row))
        .filter(|r| matches!(r.kind, PrefKind::Slider(..)))
        .count()
}
