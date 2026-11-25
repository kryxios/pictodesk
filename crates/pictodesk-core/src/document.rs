//! The document: canvas size, working space, and a stack of layers
//! (index 0 = bottom). Rendering composites layers bottom-up through
//! the blend/composite core into an 8-bit buffer for display/export.

use crate::blend::{composite_over, composite_over_inks};
use crate::color::{cmyk_to_rgb, WorkingSpace};
use crate::layer::Layer;
use crate::pixel::Pixel16;
use crate::selection::Selection;

/// The document's color mode — the classic Mode menu. In CMYK mode
/// the layers store real CMYK ink values (in the tile pixels' four
/// slots) plus a per-layer alpha plane; rendering composites in ink
/// space and converts to RGB for the screen, and CMYK TIFF export
/// writes the stored inks. Grayscale documents store desaturated
/// RGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Rgb,
    Cmyk,
    Grayscale,
    /// Phase 6.5: CIE Lab — tile slots hold (L/100,
    /// (a+128)/255, (b+128)/255), alpha stays in the pixel.
    Lab,
    /// Phase 6.5: palette-indexed color — pixels hold palette
    /// RGB values; the palette lives in `Document::color_table`.
    Indexed,
    /// Phase 6.5: 1-bit black/white.
    Bitmap,
    /// Phase 6.5: duotone family — grayscale pixels rendered
    /// through ink curves (`Document::duotone`).
    Duotone,
    /// Phase 6.5: channels as independent ink planes.
    Multichannel,
}

/// One duotone ink (Phase 6.5): an RGB ink color and its
/// transfer curve as a 256-entry LUT over the gray value.
#[derive(Clone, Debug, PartialEq)]
pub struct DuotoneInk {
    pub color: (u8, u8, u8),
    pub curve: [u8; 256],
}

impl DuotoneInk {
    /// A linear-curve ink.
    pub fn new(color: (u8, u8, u8)) -> Self {
        let mut curve = [0u8; 256];
        for (i, v) in curve.iter_mut().enumerate() {
            *v = i as u8;
        }
        DuotoneInk { color, curve }
    }
}

/// Duotone family setup (Phase 6.5): 1 ink = Monotone,
/// 2 = Duotone, 3 = Tritone, 4 = Quadtone.
#[derive(Clone, Debug, PartialEq)]
pub struct DuotoneSpec {
    pub inks: Vec<DuotoneInk>,
}

impl Default for DuotoneSpec {
    /// Era default: a single black ink, linear curve.
    fn default() -> Self {
        DuotoneSpec {
            inks: vec![DuotoneInk::new((0, 0, 0))],
        }
    }
}

/// Multichannel plane metadata (Phase 6.5).
#[derive(Clone, Debug, PartialEq)]
pub struct MultiChannelInfo {
    pub name: String,
    pub color: (u8, u8, u8),
}

/// An alpha channel: a document-sized grayscale buffer. Holds saved
/// selections (Select > Save Selection) and, when `spot_color` is
/// set, a spot-color channel shown tinted in the Channels panel.
#[derive(Clone, Debug, PartialEq)]
pub struct AlphaChannel {
    pub name: String,
    pub spot_color: Option<(u8, u8, u8)>,
    /// Channel Options (Phase 6.5): true when the channel's
    /// color overlay indicates MASKED areas (the rubylith
    /// covers unselected pixels), false for SELECTED areas.
    pub masked: bool,
    pub data: Vec<u8>, // width * height, row-major
}

impl AlphaChannel {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        AlphaChannel {
            name: name.into(),
            spot_color: None,
            masked: true,
            data: vec![0; (width * height) as usize],
        }
    }
    pub fn value(&self, width: u32, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= width as i32 {
            return 0;
        }
        let i = (y as u32 * width + x as u32) as usize;
        self.data.get(i).copied().unwrap_or(0)
    }
    pub fn set(&mut self, width: u32, x: i32, y: i32, v: u8) {
        if x < 0 || y < 0 || x >= width as i32 {
            return;
        }
        let i = (y as u32 * width + x as u32) as usize;
        if i < self.data.len() {
            self.data[i] = v;
        }
    }
}

/// One entry in a document's history log — provenance metadata
/// stored in the file itself (the way classic editors embed an
/// event log: created / opened / saved / each edit's label). This
/// is NOT the undo stack; undo states are session-only.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEvent {
    /// Seconds since the Unix epoch (UTC).
    pub when: u64,
    pub action: String,
}

/// Format a Unix timestamp as `YYYY-MM-DD HH:MM` in UTC.
pub fn format_history_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let day_secs = secs % 86_400;
    let (hh, mm) = (day_secs / 3600, (day_secs % 3600) / 60);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
}

/// Isolated set-group state for the layer loops: (group id,
/// blend mode, set opacity, group accumulator).
type SetGroup<T> = (u16, crate::blend::BlendMode, f32, T);

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Pixels per inch (metadata for rulers/printing; the
    /// canvas itself is always pixel-addressed). Default 72.
    pub resolution: f32,
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub working_space: WorkingSpace,
    pub mode: ColorMode,
    /// Indexed-mode palette (Phase 6.5): up to 256 RGB entries.
    /// Pixels in an Indexed document hold these exact values.
    pub color_table: Vec<(u8, u8, u8)>,
    /// Duotone-mode ink setup (Phase 6.5).
    pub duotone: Option<DuotoneSpec>,
    /// Multichannel-mode plane metadata (Phase 6.5): names and
    /// ink colors, parallel to `multi_planes`.
    pub multi_channels: Vec<MultiChannelInfo>,
    /// Multichannel-mode planes (Phase 6.5): one full-document
    /// grayscale buffer per channel, row-major u16.
    pub multi_planes: Vec<Vec<u16>>,
    /// Session-only: the multichannel plane painting targets
    /// (None = all planes, the composite-view behavior).
    /// Not persisted.
    pub multi_active: Option<usize>,
    pub alphas: Vec<AlphaChannel>,
    pub paths: Vec<crate::path::Path>,
    /// File Info (IPTC-style) fields, Phase 6.5.
    pub info_title: String,
    pub info_caption: String,
    pub info_keywords: String,
    pub info_copyright: String,
    pub info_credit: String,
    pub work_path: Option<crate::path::Path>,
    pub clipping_path: Option<usize>,
    /// Provenance event log, stored in the `.pdk` file.
    pub history_log: Vec<HistoryEvent>,
    /// Vertical guide positions in document pixels (Phase 4).
    pub guides_x: Vec<i32>,
    /// Horizontal guide positions in document pixels (Phase 4).
    pub guides_y: Vec<i32>,
    /// Color sampler points (Phase 5, max 4): persistent,
    /// non-printing measurement points read by the Info panel.
    pub samplers: Vec<(i32, i32)>,
    /// The document's defined pattern (Phase 5, Edit > Define
    /// Pattern), used by the Pattern Stamp and pattern fills.
    pub pattern: Option<PatternData>,
    /// The most recent selection (Phase 5, Select > Reselect).
    pub last_selection: Option<Selection>,
    /// Shared light angle in degrees for effects that opt into
    /// the global angle (Phase 5, Layer > Effects > Global Angle).
    pub global_angle: i32,
    /// Session-only: hide all layer effects without removing
    /// them (Layer > Effects > Hide All Effects). Not persisted.
    pub effects_suppressed: bool,
    /// Layer sets (Phase 6), one level deep.
    pub sets: Vec<LayerSet>,
    /// Annotations (Phase 6, Notes tool).
    pub notes: Vec<Note>,
    /// Web slices (Phase 6).
    pub slices: Vec<SliceRect>,
    /// User-defined custom shapes (Phase 6).
    pub custom_shapes: Vec<CustomShape>,
    /// Global light altitude in degrees (Phase 6, Layer Style
    /// Global Light: angle is `global_angle`).
    pub global_altitude: i32,
}

/// A layer set (Phase 6): a one-level folder of layers. Sets
/// default to Pass Through (`mode: None`) — members blend with
/// everything below the set, exactly the classic default; a
/// set opacity scales its members, an invisible set hides them.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSet {
    pub name: String,
    pub opacity: f32,
    pub mode: Option<crate::blend::BlendMode>,
    pub visible: bool,
    pub color: u8,
}

/// A document annotation (Phase 6, Notes tool): a small marker
/// at a document point with text. Non-printing.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub x: i32,
    pub y: i32,
    /// Marker color index (palette row, 0..=7).
    pub color: u8,
    pub text: String,
    /// Audio annotation (v0.6.1): a recorded voice note as
    /// WAV bytes (16-bit mono PCM), played back from the
    /// Notes tool. None for text-only notes.
    pub audio: Option<Vec<u8>>,
}

/// A web slice (Phase 6): a rectangular region exported as its
/// own image, with link metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub name: String,
    pub url: String,
    pub alt: String,
}

/// A user-defined custom shape (Phase 6, Edit > Define Custom
/// Shape): normalized points (0..=1) of a polygon.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomShape {
    pub name: String,
    pub points: Vec<(f32, f32)>,
}

/// A defined pattern: a small RGBA tile that repeats.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternData {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub px: Vec<Pixel16>,
}

/// Pre-rendered effect buffers for one layer over a region (the
/// region is expanded by the effect's margin, so blurs and shadow
/// offsets have room). Built per render region, never stored.
struct FxBuf {
    li: usize,
    ox: i32,
    oy: i32,
    w: usize,
    below: Vec<Pixel16>,
    within: Vec<Pixel16>,
}

impl Document {
    /// A new document always starts with one transparent layer, like
    /// every editor's "new image" — except ours is a real layer in the
    /// model from the first minute.
    pub fn new(width: u32, height: u32) -> Self {
        Document {
            width,
            height,
            resolution: 72.0,
            layers: vec![Layer::new("Background")],
            active_layer: 0,
            working_space: WorkingSpace::Srgb,
            mode: ColorMode::Rgb,
            color_table: Vec::new(),
            duotone: None,
            multi_channels: Vec::new(),
            multi_planes: Vec::new(),
            multi_active: None,
            alphas: Vec::new(),
            paths: Vec::new(),
            info_title: String::new(),
            info_caption: String::new(),
            info_keywords: String::new(),
            info_copyright: String::new(),
            info_credit: String::new(),
            work_path: None,
            clipping_path: None,
            history_log: Vec::new(),
            guides_x: Vec::new(),
            guides_y: Vec::new(),
            samplers: Vec::new(),
            pattern: None,
            last_selection: None,
            global_angle: 120,
            effects_suppressed: false,
            sets: Vec::new(),
            notes: Vec::new(),
            slices: Vec::new(),
            custom_shapes: Vec::new(),
            global_altitude: 30,
        }
    }

    /// Append an event to the history log, timestamped now. The
    /// log is capped at 500 entries (oldest dropped) so the file
    /// stays lean.
    pub fn log_event(&mut self, action: impl Into<String>) {
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.history_log.push(HistoryEvent {
            when,
            action: action.into(),
        });
        if self.history_log.len() > 500 {
            let excess = self.history_log.len() - 500;
            self.history_log.drain(..excess);
        }
    }

    pub fn add_layer(&mut self, name: impl Into<String>) -> usize {
        self.layers.push(Layer::new(name));
        self.active_layer = self.layers.len() - 1;
        self.active_layer
    }

    /// Duplicate a layer in place (copy sits directly above).
    pub fn duplicate_layer(&mut self, idx: usize) -> Option<usize> {
        let mut copy = self.layers.get(idx)?.clone();
        copy.name = format!("{} copy", copy.name);
        self.layers.insert(idx + 1, copy);
        self.active_layer = idx + 1;
        Some(idx + 1)
    }

    /// Delete a layer; refuses when it's the only one left.
    pub fn delete_layer(&mut self, idx: usize) -> bool {
        if self.layers.len() <= 1 || idx >= self.layers.len() {
            return false;
        }
        self.layers.remove(idx);
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        }
        true
    }

    /// Move a layer to a new stack position (0 = bottom).
    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        if self.active_layer == from {
            self.active_layer = to;
        }
    }

    /// Merge layer `idx` into the layer below it, honoring the top
    /// layer's blend mode, opacity, and visibility.
    pub fn merge_down(&mut self, idx: usize) -> bool {
        if idx == 0 || idx >= self.layers.len() {
            return false;
        }
        // Phase 4 bake path: when either layer carries a mask, or
        // the top is an adjustment layer (which has no pixels of its
        // own), the pair is rendered together and the result becomes
        // one plain layer — the only way to keep the composite
        // identical after the merge.
        let needs_bake = self.layers[idx].adjustment.is_some()
            || self.layers[idx].mask.is_some()
            || self.layers[idx - 1].mask.is_some();
        if needs_bake {
            let mut pair = Document::new(self.width, self.height);
            pair.mode = self.mode;
            pair.working_space = self.working_space;
            let mut below = self.layers[idx - 1].clone();
            // The merged layer inherits the bottom layer's blend
            // mode and opacity (the classic merge rule), so the pair
            // is baked with the bottom layer at Normal / 100% and
            // those properties are re-applied to the result.
            let (bmode, bop) = (below.blend_mode, below.opacity);
            below.blend_mode = crate::blend::BlendMode::Normal;
            below.opacity = 1.0;
            pair.layers = vec![below, self.layers[idx].clone()];
            let mut merged = Layer::new(self.layers[idx - 1].name.clone());
            merged.blend_mode = bmode;
            merged.opacity = bop;
            match self.mode {
                ColorMode::Cmyk => {
                    for y in 0..self.height as i32 {
                        for x in 0..self.width as i32 {
                            let (ink, a) = pair.composite_inks(x, y);
                            if a > 0.0 {
                                let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                                merged.set_pixel(
                                    x,
                                    y,
                                    Pixel16::from_f32s(ink[0], ink[1], ink[2], ink[3]),
                                );
                                merged.set_alpha(x, y, e(a));
                            }
                        }
                    }
                }
                ColorMode::Lab => {
                    for y in 0..self.height as i32 {
                        for x in 0..self.width as i32 {
                            let p = pair.render_pixel(x, y);
                            if p.a > 0 {
                                let (r, g, b, a) = p.to_f32s();
                                let (l, la, lb) = crate::color::rgb_to_lab(r, g, b);
                                let (sr, sg, sb) = crate::color::lab_to_slots(l, la, lb);
                                merged.set_pixel(x, y, Pixel16::from_f32s(sr, sg, sb, a));
                            }
                        }
                    }
                }
                _ => {
                    for y in 0..self.height as i32 {
                        for x in 0..self.width as i32 {
                            let p = pair.render_pixel(x, y);
                            if p.a > 0 {
                                merged.set_pixel(x, y, p);
                            }
                        }
                    }
                }
            }
            self.layers.remove(idx);
            self.layers[idx - 1] = merged;
            self.active_layer = idx - 1;
            return true;
        }
        let top = self.layers.remove(idx);
        if top.visible {
            match self.mode {
                ColorMode::Cmyk => {
                    for y in 0..self.height as i32 {
                        for x in 0..self.width as i32 {
                            let (tc, tm, ty, tk, ta) = {
                                let p = top.get_pixel(x, y).to_f32s();
                                (p.0, p.1, p.2, p.3, top.get_alpha(x, y) as f32 / 65535.0)
                            };
                            if ta <= 0.0 {
                                continue;
                            }
                            let (bc, bm, by, bk, ba) = self.get_cmyk(idx - 1, x, y);
                            let bf = |v: u16| v as f32 / 65535.0;
                            let (out, oa) = composite_over_inks(
                                ([bf(bc), bf(bm), bf(by), bf(bk)], bf(ba)),
                                ([tc, tm, ty, tk], ta),
                                top.blend_mode,
                                top.opacity,
                            );
                            let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                            self.set_cmyk(
                                idx - 1,
                                x,
                                y,
                                (e(out[0]), e(out[1]), e(out[2]), e(out[3]), e(oa)),
                            );
                        }
                    }
                }
                _ => {
                    for y in 0..self.height as i32 {
                        for x in 0..self.width as i32 {
                            let tp = top.get_pixel(x, y);
                            if tp.a == 0 {
                                continue;
                            }
                            let bp = self.layers[idx - 1].get_pixel(x, y);
                            let out = composite_over(
                                bp.to_f32s(),
                                tp.to_f32s(),
                                top.blend_mode,
                                top.opacity,
                            );
                            self.layers[idx - 1].set_pixel(
                                x,
                                y,
                                Pixel16::from_f32s(out.0, out.1, out.2, out.3),
                            );
                        }
                    }
                }
            }
        }
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        }
        true
    }

    /// Merge Visible: bake all visible layers into the lowest
    /// visible one; hidden layers are kept untouched.
    pub fn merge_visible(&mut self) {
        let Some(base) = self.layers.iter().position(|l| l.visible) else {
            return;
        };
        let mut flat = Layer::new(self.layers[base].name.clone());
        match self.mode {
            ColorMode::Cmyk => {
                for y in 0..self.height as i32 {
                    for x in 0..self.width as i32 {
                        let (ink, a) = self.composite_inks(x, y);
                        if a > 0.0 {
                            let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                            flat.set_pixel(
                                x,
                                y,
                                Pixel16::from_f32s(ink[0], ink[1], ink[2], ink[3]),
                            );
                            flat.set_alpha(x, y, e(a));
                        }
                    }
                }
            }
            _ => {
                for y in 0..self.height as i32 {
                    for x in 0..self.width as i32 {
                        let p = self.render_pixel(x, y);
                        if p.a > 0 {
                            flat.set_pixel(x, y, p);
                        }
                    }
                }
            }
        }
        self.layers[base] = flat;
        // Remove the other visible layers, top-down.
        for i in (0..self.layers.len()).rev() {
            if i != base && self.layers[i].visible {
                self.layers.remove(i);
            }
        }
        // Layers removed were all above `base`, so its index holds.
        self.active_layer = base;
    }

    /// Render one layer alone (for thumbnails): RGB-space result.
    pub fn render_layer_pixel(&self, layer: usize, x: i32, y: i32) -> Pixel16 {
        let l = &self.layers[layer];
        if !l.visible {
            return l.get_pixel(x, y);
        }
        match self.mode {
            ColorMode::Cmyk => {
                let (c, m, y, k, a) = self.get_cmyk(layer, x, y);
                let bf = |v: u16| v as f32 / 65535.0;
                let (r, g, b) = crate::color::cmyk_to_rgb(bf(c), bf(m), bf(y), bf(k));
                Pixel16::from_f32s(r, g, b, bf(a) * l.opacity)
            }
            _ => {
                let p = l.get_pixel(x, y).to_f32s();
                Pixel16::from_f32s(p.0, p.1, p.2, p.3 * l.opacity)
            }
        }
    }

    /// Flatten: bake the visible composite into a single layer.
    /// Hidden layers are discarded, as in the classic dialog flow.
    pub fn flatten(&mut self) {
        let mut flat = Layer::new("Background");
        // Render in horizontal bands so effected documents build
        // their effect buffers once per band, not once per pixel.
        let band = 128i32;
        match self.mode {
            ColorMode::Cmyk if !self.has_effects() => {
                for y in 0..self.height as i32 {
                    for x in 0..self.width as i32 {
                        let (ink, a) = self.composite_inks(x, y);
                        if a > 0.0 {
                            let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                            flat.set_pixel(
                                x,
                                y,
                                Pixel16::from_f32s(ink[0], ink[1], ink[2], ink[3]),
                            );
                            flat.set_alpha(x, y, e(a));
                        }
                    }
                }
            }
            ColorMode::Cmyk => {
                // Effected CMYK: composite in RGB (the effects
                // path), then convert the result back to inks.
                let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                let mut y = 0i32;
                while y < self.height as i32 {
                    let bh = band.min(self.height as i32 - y) as u32;
                    let px = self.render_region_px(0, y, self.width, bh);
                    for (n, p) in px.iter().enumerate() {
                        let (xx, yy) = (
                            (n as u32 % self.width) as i32,
                            y + (n as u32 / self.width) as i32,
                        );
                        let (r, g, b, a) = p.to_f32s();
                        if a > 0.0 {
                            let (c, m, yel, k) = crate::color::rgb_to_cmyk(r, g, b);
                            flat.set_pixel(xx, yy, Pixel16::from_f32s(c, m, yel, k));
                            flat.set_alpha(xx, yy, e(a));
                        }
                    }
                    y += bh as i32;
                }
            }
            ColorMode::Lab => {
                // Composite in RGB (render path), then encode
                // the flat layer back into Lab slots.
                let mut y = 0i32;
                while y < self.height as i32 {
                    let bh = band.min(self.height as i32 - y) as u32;
                    let px = self.render_region_px(0, y, self.width, bh);
                    for (n, p) in px.iter().enumerate() {
                        if p.a > 0 {
                            let (xx, yy) = (
                                (n as u32 % self.width) as i32,
                                y + (n as u32 / self.width) as i32,
                            );
                            let (r, g, b, a) = p.to_f32s();
                            let (l, la, lb) = crate::color::rgb_to_lab(r, g, b);
                            let (sr, sg, sb) = crate::color::lab_to_slots(l, la, lb);
                            flat.set_pixel(xx, yy, Pixel16::from_f32s(sr, sg, sb, a));
                        }
                    }
                    y += bh as i32;
                }
            }
            _ => {
                let mut y = 0i32;
                while y < self.height as i32 {
                    let bh = band.min(self.height as i32 - y) as u32;
                    let px = self.render_region_px(0, y, self.width, bh);
                    for (n, p) in px.iter().enumerate() {
                        if p.a > 0 {
                            let (xx, yy) = (
                                (n as u32 % self.width) as i32,
                                y + (n as u32 / self.width) as i32,
                            );
                            flat.set_pixel(xx, yy, *p);
                        }
                    }
                    y += bh as i32;
                }
            }
        }
        self.layers = vec![flat];
        self.active_layer = 0;
    }

    /// One pixel of `layer` as CMYK + alpha, each 0..=65535: the ink
    /// values from the tile slots and alpha from the layer's alpha
    /// plane. Meaningful when `mode == ColorMode::Cmyk`.
    pub fn get_cmyk(&self, layer: usize, x: i32, y: i32) -> (u16, u16, u16, u16, u16) {
        let l = &self.layers[layer];
        let p = l.get_pixel(x, y);
        (p.r, p.g, p.b, p.a, l.get_alpha(x, y))
    }

    pub fn set_cmyk(&mut self, layer: usize, x: i32, y: i32, v: (u16, u16, u16, u16, u16)) {
        let l = &mut self.layers[layer];
        l.set_pixel(x, y, Pixel16::new(v.0, v.1, v.2, v.3));
        l.set_alpha(x, y, v.4);
    }

    /// Phase 6 set handling, full form (v0.6.1): returns None
    /// when the layer's set is invisible (the layer does not
    /// render), else (opacity_factor, isolated). A Pass Through
    /// set (no blend mode) multiplies member opacity directly;
    /// a set WITH a blend mode renders its members into an
    /// isolated group accumulator (factor 1.0 inside) that is
    /// flushed onto the backdrop with the set's mode + opacity.
    fn set_render(&self, layer: &Layer) -> Option<(f32, bool)> {
        if layer.group == 0 {
            return Some((1.0, false));
        }
        match self.sets.get((layer.group - 1) as usize) {
            Some(set) => {
                if !set.visible {
                    None
                } else if set.mode.is_some() {
                    Some((1.0, true))
                } else {
                    Some((set.opacity, false))
                }
            }
            None => Some((1.0, false)),
        }
    }

    /// The blend mode a clipped layer effectively uses when
    /// "Blend Clipped Layers as Group" is on: the mode of the
    /// nearest non-clipped visible layer below (its base).
    fn clipped_base_mode(&self, li: usize) -> Option<crate::blend::BlendMode> {
        for bi in (0..li).rev() {
            let b = &self.layers[bi];
            if !b.clipped && b.visible {
                return Some(b.blend_mode);
            }
        }
        None
    }

    /// The bounds inside which a layer with Transparency
    /// Shapes off blends at full strength: the shape spec's
    /// rect for shape layers, the whole document for fill
    /// layers, else the layer's painted bounds.
    fn shapes_off_bounds(&self, layer: &Layer) -> (i32, i32, i32, i32) {
        if let Some(sp) = &layer.shape {
            return (
                sp.rect.0 + layer.offset.0,
                sp.rect.1 + layer.offset.1,
                sp.rect.2 + layer.offset.0,
                sp.rect.3 + layer.offset.1,
            );
        }
        if layer.fill.is_some() {
            return (0, 0, self.width as i32, self.height as i32);
        }
        crate::transform::layer_painted_bounds(layer).unwrap_or((
            0,
            0,
            self.width as i32,
            self.height as i32,
        ))
    }

    /// Knockout punch on an accumulator's alpha (v0.6.1):
    /// Deep drives alpha toward 0; Shallow drives it toward
    /// `base_a` — the alpha the accumulator had at the base of
    /// the layer's set (0 for a group accumulator) or just
    /// above the bottom layer for set-less layers.
    fn knockout_punch(acc_a: f32, base_a: f32, deep: bool, punch: f32) -> f32 {
        if deep {
            acc_a * (1.0 - punch)
        } else {
            base_a + (acc_a - base_a) * (1.0 - punch)
        }
    }

    /// Phase 6: a layer's content pixel at a document point,
    /// honoring shape and fill payloads (RGB path; CMYK shape /
    /// fill layers fall back to their stored pixels).
    pub fn content_pixel(&self, li: usize, x: i32, y: i32) -> (f32, f32, f32, f32) {
        let layer = &self.layers[li];
        if let Some(spec) = &layer.shape {
            return crate::shapes::shape_pixel(spec, x - layer.offset.0, y - layer.offset.1)
                .to_f32s();
        }
        if let Some(spec) = &layer.fill {
            return crate::shapes::fill_pixel(spec, x, y, self.width, self.height).to_f32s();
        }
        let p = layer.get_pixel(x, y).to_f32s();
        if self.mode == ColorMode::Lab {
            // Lab storage: slots decode to Lab, then to RGB for
            // the (RGB-space) compositor. Shape/fill layers
            // above keep their RGB-defined colors.
            let (l, a, b) = crate::color::slots_to_lab(p.0, p.1, p.2);
            let (r, g, b) = crate::color::lab_to_rgb(l, a, b);
            return (r, g, b, p.3);
        }
        p
    }

    /// The composited pixel at (x, y) expressed as CIE Lab
    /// (L 0..=100, a/b -128..=127) plus alpha — the Channels
    /// panel's Lab view and Lab readouts use this.
    pub fn render_lab_pixel(&self, x: i32, y: i32) -> (f32, f32, f32, f32) {
        let p = self.render_pixel(x, y).to_f32s();
        let (l, a, b) = crate::color::rgb_to_lab(p.0, p.1, p.2);
        (l, a, b, p.3)
    }

    /// One pixel of `layer` as Lab slots + alpha, each
    /// 0..=65535. Meaningful when `mode == ColorMode::Lab`.
    pub fn get_lab(&self, layer: usize, x: i32, y: i32) -> (u16, u16, u16, u16) {
        let p = self.layers[layer].get_pixel(x, y);
        (p.r, p.g, p.b, p.a)
    }

    pub fn set_lab(&mut self, layer: usize, x: i32, y: i32, v: (u16, u16, u16, u16)) {
        self.layers[layer].set_pixel(x, y, Pixel16::new(v.0, v.1, v.2, v.3));
    }

    /// Phase 6 advanced blending: composite one layer pixel
    /// with Blend If (feathered), clipping-group, knockout,
    /// and per-channel switches applied around the classic
    /// composite_over. `ko_base_a` is the alpha at the base of
    /// the accumulator the layer lands in — the floor a
    /// Shallow knockout punches toward. `p` must already carry
    /// mask, fill-opacity, and set scaling. Returns the new
    /// accumulator.
    #[allow(clippy::too_many_arguments)]
    fn adv_composite(
        &self,
        mut acc: (f32, f32, f32, f32),
        li: usize,
        layer: &Layer,
        p: (f32, f32, f32, f32),
        mode: crate::blend::BlendMode,
        opacity: f32,
        ko_base_a: f32,
        x: i32,
        y: i32,
    ) -> (f32, f32, f32, f32) {
        let luma = |c: (f32, f32, f32)| 0.299 * c.0 + 0.587 * c.1 + 0.114 * c.2;
        let mut p = p;
        // Blend If with split-slider feathering (v0.6.1): the
        // stored ranges are the inner thresholds; each split
        // width feathers the factor outward from its threshold.
        // Zero splits reproduce the old hard edges exactly.
        let [tlo, thi, ulo, uhi] = layer.blend_if;
        let [stlo, sthi, sulo, suhi] = layer.blend_if_split;
        if tlo > 0
            || thi < 255
            || ulo > 0
            || uhi < 255
            || stlo > 0
            || sthi > 0
            || sulo > 0
            || suhi > 0
        {
            let ramp = |v: f32, lo: u8, lf: u8, hi: u8, hf: u8| -> f32 {
                let (lo, lf, hi, hf) = (lo as f32, lf as f32, hi as f32, hf as f32);
                if v < lo - lf || v > hi + hf {
                    0.0
                } else if v < lo {
                    (v - (lo - lf)) / lf.max(1.0)
                } else if v > hi {
                    1.0 - (v - hi) / hf.max(1.0)
                } else {
                    1.0
                }
            };
            let tl = luma((p.0, p.1, p.2)) * 255.0;
            let ul = luma((acc.0, acc.1, acc.2)) * 255.0;
            p.3 *= ramp(tl, tlo, stlo, thi, sthi) * ramp(ul, ulo, sulo, uhi, suhi);
        }
        // Clipping group: clip to the nearest non-clipped
        // visible layer below.
        if layer.clipped && p.3 > 0.0 {
            let mut factor = 0.0f32;
            for bi in (0..li).rev() {
                let below = &self.layers[bi];
                if below.clipped || !below.visible {
                    continue;
                }
                let bp = self.content_pixel(bi, x, y);
                factor = bp.3 * Self::mask_factor(below, x, y);
                break;
            }
            p.3 *= factor;
        }
        if p.3 <= 0.0 {
            return acc;
        }
        // Knockout: the layer's coverage punches the target
        // accumulator's alpha down before the layer itself
        // lands. Deep punches toward transparency; Shallow
        // punches toward `ko_base_a`, the alpha at the base of
        // the accumulator the layer lands in (its set's base,
        // or just above the bottom layer), supplied by the
        // layer loops.
        if layer.knockout != 0 {
            let punch = (p.3 * opacity).clamp(0.0, 1.0);
            acc.3 = Self::knockout_punch(acc.3, ko_base_a, layer.knockout == 2, punch);
        }
        let before = acc;
        acc = composite_over(acc, p, mode, opacity);
        // Per-channel switches: a cleared channel keeps the
        // backdrop's value.
        if layer.blend_channels != 0xF {
            if layer.blend_channels & 1 == 0 {
                acc.0 = before.0;
            }
            if layer.blend_channels & 2 == 0 {
                acc.1 = before.1;
            }
            if layer.blend_channels & 4 == 0 {
                acc.2 = before.2;
            }
        }
        acc
    }

    /// The layer's mask strength at (x, y) as 0.0..=1.0 — 1.0 when
    /// there is no mask or the mask is disabled.
    fn mask_factor(layer: &Layer, x: i32, y: i32) -> f32 {
        if layer.mask.is_some() && layer.mask_enabled {
            layer.mask_value(x, y) as f32 / 255.0
        } else {
            1.0
        }
    }

    /// True when any visible layer carries an enabled effect —
    /// those layers need region buffers (blurs), so rendering
    /// routes through the effects compositor.
    pub fn has_effects(&self) -> bool {
        if self.effects_suppressed {
            return false;
        }
        self.layers.iter().any(|l| {
            l.visible
                && l.adjustment.is_none()
                && l.effects.map(|e| e.any_enabled()).unwrap_or(false)
        })
    }

    /// Build effect buffers for every effected layer covering
    /// (x, y, w, h) plus each effect's margin.
    fn fx_buffers(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<FxBuf> {
        let mut out = Vec::new();
        for (li, layer) in self.layers.iter().enumerate() {
            let Some(fx) = &layer.effects else { continue };
            if !layer.visible || layer.adjustment.is_some() || !fx.any_enabled() {
                continue;
            }
            let m = fx.margin();
            let (ox, oy) = (x - m, y - m);
            let (bw, bh) = (
                (w as i32 + 2 * m).max(0) as usize,
                (h as i32 + 2 * m).max(0) as usize,
            );
            let mut content = vec![Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0); bw * bh];
            for yy in 0..bh as i32 {
                for xx in 0..bw as i32 {
                    let (dx, dy) = (ox + xx, oy + yy);
                    let mf = Self::mask_factor(layer, dx, dy);
                    let p = match self.mode {
                        ColorMode::Cmyk => {
                            let (c, mm, yel, k, a) = self.get_cmyk(li, dx, dy);
                            let bf = |v: u16| v as f32 / 65535.0;
                            let (r, g, b) =
                                crate::color::cmyk_to_rgb(bf(c), bf(mm), bf(yel), bf(k));
                            Pixel16::from_f32s(r, g, b, bf(a) * mf)
                        }
                        _ => {
                            let p = self.content_pixel(li, dx, dy);
                            Pixel16::from_f32s(p.0, p.1, p.2, p.3 * mf)
                        }
                    };
                    content[(yy as usize) * bw + xx as usize] = p;
                }
            }
            let below = crate::effects::below_buffer(&content, bw, bh, fx, self.global_angle);
            let mut within = content;
            crate::effects::apply_within(&mut within, bw, bh, fx, self.global_angle);
            out.push(FxBuf {
                li,
                ox,
                oy,
                w: bw,
                below,
                within,
            });
        }
        out
    }

    /// The render_pixel loop with effect buffers substituted for
    /// effected layers. Only called when `has_effects()`.
    fn composite_with_fx(&self, fx: &[FxBuf], x: i32, y: i32) -> Pixel16 {
        let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        // Isolated set-group state (v0.6.1): (group id, mode,
        // opacity, group accumulator). A set with a blend mode
        // renders its members in isolation; the group result is
        // flushed onto the backdrop with the set's own mode and
        // opacity when the run of members ends.
        let mut group: Option<SetGroup<(f32, f32, f32, f32)>> = None;
        // Main-accumulator snapshot just above the bottom
        // rendered layer: a set-less Shallow knockout's floor.
        let mut base: Option<(f32, f32, f32, f32)> = None;
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            let Some((set_f, isolated)) = self.set_render(layer) else {
                if let Some((_, gm, go, gacc)) = group.take() {
                    acc = composite_over(acc, gacc, gm, go);
                    if base.is_none() {
                        base = Some(acc);
                    }
                }
                continue;
            };
            let want = if isolated { Some(layer.group) } else { None };
            let in_group = matches!(&group, Some((gid, _, _, _)) if Some(*gid) == want);
            if !in_group {
                if let Some((_, gm, go, gacc)) = group.take() {
                    acc = composite_over(acc, gacc, gm, go);
                    if base.is_none() {
                        base = Some(acc);
                    }
                }
                if let Some(gid) = want {
                    let set = &self.sets[(gid - 1) as usize];
                    group = Some((gid, set.mode.unwrap(), set.opacity, (0.0, 0.0, 0.0, 0.0)));
                }
            }
            let ko_base_a = if group.is_some() {
                0.0
            } else {
                base.map(|b| b.3).unwrap_or(0.0)
            };
            let mut landed = false;
            if let Some(adj) = &layer.adjustment {
                let target = match &mut group {
                    Some((_, _, _, g)) => g,
                    None => &mut acc,
                };
                if target.3 > 0.0 {
                    let t = layer.opacity * set_f * Self::mask_factor(layer, x, y);
                    if t > 0.0 {
                        let (r, g, b) = adj.apply(target.0, target.1, target.2);
                        target.0 += (r - target.0) * t;
                        target.1 += (g - target.1) * t;
                        target.2 += (b - target.2) * t;
                        landed = true;
                    }
                }
            } else {
                let buf = fx.iter().find(|b| b.li == li);
                if let Some(buf) = buf {
                    let idx = ((y - buf.oy) as usize) * buf.w + (x - buf.ox) as usize;
                    let mut bp = buf.below[idx].to_f32s();
                    if layer.mask_hides_effects {
                        bp.3 *= Self::mask_factor(layer, x, y);
                    }
                    {
                        let target = match &mut group {
                            Some((_, _, _, g)) => g,
                            None => &mut acc,
                        };
                        if bp.3 > 0.0 {
                            *target = composite_over(
                                *target,
                                bp,
                                crate::blend::BlendMode::Normal,
                                layer.opacity * set_f,
                            );
                            landed = true;
                        }
                    }
                    let mut p = buf.within[idx].to_f32s();
                    if !layer.transparency_shapes && p.3 > 0.0 {
                        let (bx0, by0, bx1, by1) = self.shapes_off_bounds(layer);
                        if x >= bx0 && x < bx1 && y >= by0 && y < by1 {
                            // The within alpha already carries
                            // the mask; lift the content part to
                            // full strength (mask still applies).
                            p.3 = Self::mask_factor(layer, x, y);
                        }
                    }
                    p.3 *= layer.fill_opacity;
                    let mut mode = layer.blend_mode;
                    if layer.clipped && layer.blend_clipped_group {
                        if let Some(bm) = self.clipped_base_mode(li) {
                            mode = bm;
                        }
                    }
                    let mut opacity = layer.opacity * set_f;
                    let mut skip = false;
                    if mode == crate::blend::BlendMode::Dissolve {
                        let prob = opacity * p.3;
                        if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                            skip = true;
                        } else {
                            p.3 = 1.0;
                            opacity = 1.0;
                            mode = crate::blend::BlendMode::Normal;
                        }
                    }
                    if !skip {
                        let target = match &mut group {
                            Some((_, _, _, g)) => g,
                            None => &mut acc,
                        };
                        *target = self
                            .adv_composite(*target, li, layer, p, mode, opacity, ko_base_a, x, y);
                        landed = true;
                    }
                } else {
                    let mut p = self.content_pixel(li, x, y);
                    if !layer.transparency_shapes && p.3 > 0.0 {
                        let (bx0, by0, bx1, by1) = self.shapes_off_bounds(layer);
                        if x >= bx0 && x < bx1 && y >= by0 && y < by1 {
                            p.3 = 1.0;
                        }
                    }
                    p.3 *= Self::mask_factor(layer, x, y) * layer.fill_opacity;
                    let mut mode = layer.blend_mode;
                    if layer.clipped && layer.blend_clipped_group {
                        if let Some(bm) = self.clipped_base_mode(li) {
                            mode = bm;
                        }
                    }
                    let mut opacity = layer.opacity * set_f;
                    let mut skip = false;
                    if mode == crate::blend::BlendMode::Dissolve {
                        let prob = opacity * p.3;
                        if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                            skip = true;
                        } else {
                            p.3 = 1.0;
                            opacity = 1.0;
                            mode = crate::blend::BlendMode::Normal;
                        }
                    }
                    if !skip {
                        let target = match &mut group {
                            Some((_, _, _, g)) => g,
                            None => &mut acc,
                        };
                        *target = self
                            .adv_composite(*target, li, layer, p, mode, opacity, ko_base_a, x, y);
                        landed = true;
                    }
                }
            }
            if landed && group.is_none() && base.is_none() {
                base = Some(acc);
            }
        }
        if let Some((_, gm, go, gacc)) = group.take() {
            acc = composite_over(acc, gacc, gm, go);
        }
        self.duotone_map(Pixel16::from_f32s(acc.0, acc.1, acc.2, acc.3))
    }

    /// Render a region to Pixel16 rows, effects-aware: buffers are
    /// built once per region instead of once per pixel.
    fn render_region_px(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<Pixel16> {
        if !self.has_effects() {
            let mut out = Vec::with_capacity((w * h) as usize);
            for yy in 0..h as i32 {
                for xx in 0..w as i32 {
                    out.push(self.render_pixel(x + xx, y + yy));
                }
            }
            return out;
        }
        let fx = self.fx_buffers(x, y, w, h);
        let mut out = Vec::with_capacity((w * h) as usize);
        for yy in 0..h as i32 {
            for xx in 0..w as i32 {
                out.push(self.composite_with_fx(&fx, x + xx, y + yy));
            }
        }
        out
    }

    /// Duotone ink mapping (Phase 6.5) applied to a composited
    /// pixel: the gray value drives each ink's curve, and the
    /// inks mix subtractively over paper white. Identity for
    /// non-Duotone documents.
    pub fn duotone_map(&self, p: Pixel16) -> Pixel16 {
        if self.mode != ColorMode::Duotone {
            return p;
        }
        let Some(spec) = &self.duotone else {
            return p;
        };
        let (r, g, b, a) = p.to_f32s();
        let gray = 0.299 * r + 0.587 * g + 0.114 * b;
        let d = ((1.0 - gray) * 255.0).round().clamp(0.0, 255.0) as usize;
        let mut out = (1.0f32, 1.0f32, 1.0f32);
        for ink in &spec.inks {
            let amt = ink.curve[d] as f32 / 255.0;
            let ic = (
                ink.color.0 as f32 / 255.0,
                ink.color.1 as f32 / 255.0,
                ink.color.2 as f32 / 255.0,
            );
            out.0 *= 1.0 - amt * (1.0 - ic.0);
            out.1 *= 1.0 - amt * (1.0 - ic.1);
            out.2 *= 1.0 - amt * (1.0 - ic.2);
        }
        Pixel16::from_f32s(out.0, out.1, out.2, a)
    }

    /// Multichannel composite (Phase 6.5): the planes mix
    /// subtractively over paper white, like duotone inks with
    /// fixed colors. Alpha comes from the flattened layer's
    /// CMYK alpha plane.
    pub fn composite_multi(&self, x: i32, y: i32) -> Pixel16 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Pixel16::TRANSPARENT;
        }
        let i = (y as usize) * self.width as usize + x as usize;
        let mut out = (1.0f32, 1.0f32, 1.0f32);
        for (pi, plane) in self.multi_planes.iter().enumerate() {
            let amt = plane.get(i).copied().unwrap_or(0) as f32 / 65535.0;
            if amt <= 0.0 {
                continue;
            }
            let c = self
                .multi_channels
                .get(pi)
                .map(|m| m.color)
                .unwrap_or((0, 0, 0));
            let ic = (c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0);
            out.0 *= 1.0 - amt * (1.0 - ic.0);
            out.1 *= 1.0 - amt * (1.0 - ic.1);
            out.2 *= 1.0 - amt * (1.0 - ic.2);
        }
        let a = if self.layers.is_empty() {
            1.0
        } else {
            self.layers[0].get_alpha(x, y) as f32 / 65535.0
        };
        Pixel16::from_f32s(out.0, out.1, out.2, a)
    }

    /// Flattened pixel at (x, y): all visible layers composited.
    /// CMYK documents composite in ink space and convert to RGB.
    pub fn render_pixel(&self, x: i32, y: i32) -> Pixel16 {
        if self.mode == ColorMode::Multichannel && !self.multi_planes.is_empty() {
            return self.composite_multi(x, y);
        }
        if self.has_effects() {
            // Single-pixel query on an effected document: render
            // a window around the point (margin-capped) and take
            // the center. Region renders (display, flatten,
            // export) build the buffers once and stay fast.
            let m = self
                .layers
                .iter()
                .filter_map(|l| l.effects)
                .map(|e| e.margin())
                .max()
                .unwrap_or(0)
                .min(256);
            let side = (2 * m + 1) as u32;
            let region = self.render_region_px(x - m, y - m, side, side);
            return region[(m as usize) * side as usize + m as usize];
        }
        if self.mode == ColorMode::Cmyk {
            let (ink, a) = self.composite_inks(x, y);
            let (r, g, b) = cmyk_to_rgb(ink[0], ink[1], ink[2], ink[3]);
            return Pixel16::from_f32s(r, g, b, a);
        }
        let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        // Isolated set-group state (v0.6.1): (group id, mode,
        // opacity, group accumulator). A set with a blend mode
        // renders its members in isolation; the group result is
        // flushed onto the backdrop with the set's own mode and
        // opacity when the run of members ends.
        let mut group: Option<SetGroup<(f32, f32, f32, f32)>> = None;
        // Main-accumulator snapshot just above the bottom
        // rendered layer: a set-less Shallow knockout's floor.
        let mut base: Option<(f32, f32, f32, f32)> = None;
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            let Some((set_f, isolated)) = self.set_render(layer) else {
                if let Some((_, gm, go, gacc)) = group.take() {
                    acc = composite_over(acc, gacc, gm, go);
                    if base.is_none() {
                        base = Some(acc);
                    }
                }
                continue;
            };
            let want = if isolated { Some(layer.group) } else { None };
            let in_group = matches!(&group, Some((gid, _, _, _)) if Some(*gid) == want);
            if !in_group {
                if let Some((_, gm, go, gacc)) = group.take() {
                    acc = composite_over(acc, gacc, gm, go);
                    if base.is_none() {
                        base = Some(acc);
                    }
                }
                if let Some(gid) = want {
                    let set = &self.sets[(gid - 1) as usize];
                    group = Some((gid, set.mode.unwrap(), set.opacity, (0.0, 0.0, 0.0, 0.0)));
                }
            }
            let ko_base_a = if group.is_some() {
                0.0
            } else {
                base.map(|b| b.3).unwrap_or(0.0)
            };
            let mut landed = false;
            if let Some(adj) = &layer.adjustment {
                // Adjustment layer: transform the composite of
                // everything below it in its accumulator,
                // strength = opacity x mask.
                let target = match &mut group {
                    Some((_, _, _, g)) => g,
                    None => &mut acc,
                };
                if target.3 > 0.0 {
                    let t = layer.opacity * set_f * Self::mask_factor(layer, x, y);
                    if t > 0.0 {
                        let (r, g, b) = adj.apply(target.0, target.1, target.2);
                        target.0 += (r - target.0) * t;
                        target.1 += (g - target.1) * t;
                        target.2 += (b - target.2) * t;
                        landed = true;
                    }
                }
            } else {
                let mut p = self.content_pixel(li, x, y);
                if !layer.transparency_shapes && p.3 > 0.0 {
                    let (bx0, by0, bx1, by1) = self.shapes_off_bounds(layer);
                    if x >= bx0 && x < bx1 && y >= by0 && y < by1 {
                        p.3 = 1.0;
                    }
                }
                p.3 *= Self::mask_factor(layer, x, y) * layer.fill_opacity;
                let mut mode = layer.blend_mode;
                if layer.clipped && layer.blend_clipped_group {
                    if let Some(bm) = self.clipped_base_mode(li) {
                        mode = bm;
                    }
                }
                let mut opacity = layer.opacity * set_f;
                let mut skip = false;
                if mode == crate::blend::BlendMode::Dissolve {
                    // Dissolve: the pixel lands whole or not at
                    // all, with probability = its final alpha,
                    // decided by a stable per-pixel hash (no
                    // frame-to-frame shimmer).
                    let prob = opacity * p.3;
                    if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                        skip = true;
                    } else {
                        p.3 = 1.0;
                        opacity = 1.0;
                        mode = crate::blend::BlendMode::Normal;
                    }
                }
                if !skip {
                    let target = match &mut group {
                        Some((_, _, _, g)) => g,
                        None => &mut acc,
                    };
                    *target =
                        self.adv_composite(*target, li, layer, p, mode, opacity, ko_base_a, x, y);
                    landed = true;
                }
            }
            if landed && group.is_none() && base.is_none() {
                base = Some(acc);
            }
        }
        if let Some((_, gm, go, gacc)) = group.take() {
            acc = composite_over(acc, gacc, gm, go);
        }
        self.duotone_map(Pixel16::from_f32s(acc.0, acc.1, acc.2, acc.3))
    }

    /// Composite all visible layers at (x, y) in ink space; returns
    /// (c, m, y, k) in 0.0..=1.0 and the composited alpha.
    pub fn composite_inks(&self, x: i32, y: i32) -> ([f32; 4], f32) {
        let mut acc = ([0.0f32; 4], 0.0f32);
        // Isolated set-group state (v0.6.1): (group id, mode,
        // opacity, group accumulator). A set with a blend mode
        // renders its members in isolation; the group result is
        // flushed onto the backdrop with the set's own mode and
        // opacity when the run of members ends.
        let mut group: Option<SetGroup<([f32; 4], f32)>> = None;
        // Main-accumulator snapshot just above the bottom
        // rendered layer: a set-less Shallow knockout's floor.
        let mut base: Option<([f32; 4], f32)> = None;
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            let Some((set_f, isolated)) = self.set_render(layer) else {
                if let Some((_, gm, go, gacc)) = group.take() {
                    acc = composite_over_inks(acc, gacc, gm, go);
                    if base.is_none() {
                        base = Some(acc);
                    }
                }
                continue;
            };
            let want = if isolated { Some(layer.group) } else { None };
            let in_group = matches!(&group, Some((gid, _, _, _)) if Some(*gid) == want);
            if !in_group {
                if let Some((_, gm, go, gacc)) = group.take() {
                    acc = composite_over_inks(acc, gacc, gm, go);
                    if base.is_none() {
                        base = Some(acc);
                    }
                }
                if let Some(gid) = want {
                    let set = &self.sets[(gid - 1) as usize];
                    group = Some((
                        gid,
                        set.mode.unwrap(),
                        set.opacity,
                        ([0.0, 0.0, 0.0, 0.0], 0.0),
                    ));
                }
            }
            let mut landed = false;
            if let Some(adj) = &layer.adjustment {
                let target = match &mut group {
                    Some((_, _, _, g)) => g,
                    None => &mut acc,
                };
                if target.1 > 0.0 {
                    let t = layer.opacity * set_f * Self::mask_factor(layer, x, y);
                    if t > 0.0 {
                        let (r, g, b) =
                            cmyk_to_rgb(target.0[0], target.0[1], target.0[2], target.0[3]);
                        let (r, g, b) = adj.apply(r, g, b);
                        let (ic, im, iy, ik) = crate::color::rgb_to_cmyk(r, g, b);
                        let ink = [ic, im, iy, ik];
                        for (acc_v, ink_v) in target.0.iter_mut().zip(ink.iter()) {
                            *acc_v += (ink_v - *acc_v) * t;
                        }
                        landed = true;
                    }
                }
            } else {
                // Shape / fill layers render from their specs
                // (v0.6.1: converted to ink space; before, CMYK
                // fell back to their stored pixels).
                let (ink, raw_a) = if layer.shape.is_some() || layer.fill.is_some() {
                    let c = self.content_pixel(li, x, y);
                    let (ic, im, iy, ik) = crate::color::rgb_to_cmyk(c.0, c.1, c.2);
                    ([ic, im, iy, ik], c.3)
                } else {
                    let p = layer.get_pixel(x, y).to_f32s();
                    ([p.0, p.1, p.2, p.3], layer.get_alpha(x, y) as f32 / 65535.0)
                };
                let mut a = raw_a * Self::mask_factor(layer, x, y);
                let mut mode = layer.blend_mode;
                if layer.clipped && layer.blend_clipped_group {
                    if let Some(bm) = self.clipped_base_mode(li) {
                        mode = bm;
                    }
                }
                let mut opacity = layer.opacity * set_f;
                let mut skip = false;
                if mode == crate::blend::BlendMode::Dissolve {
                    let prob = opacity * a;
                    if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                        skip = true;
                    } else {
                        a = 1.0;
                        opacity = 1.0;
                        mode = crate::blend::BlendMode::Normal;
                    }
                }
                if !skip {
                    let target = match &mut group {
                        Some((_, _, _, g)) => g,
                        None => &mut acc,
                    };
                    *target = composite_over_inks(*target, (ink, a), mode, opacity);
                    landed = true;
                }
            }
            if landed && group.is_none() && base.is_none() {
                base = Some(acc);
            }
        }
        if let Some((_, gm, go, gacc)) = group.take() {
            acc = composite_over_inks(acc, gacc, gm, go);
        }
        acc
    }

    /// Add an adjustment layer above the active layer and make it
    /// active; returns its index. It starts with a reveal-all mask,
    /// like the classic editor's adjustment layers.
    pub fn add_adjustment_layer(
        &mut self,
        name: impl Into<String>,
        adj: crate::adjust::Adjustment,
    ) -> usize {
        let mut layer = Layer::new(name);
        layer.adjustment = Some(adj);
        layer.add_mask(255);
        let at = (self.active_layer + 1).min(self.layers.len());
        self.layers.insert(at, layer);
        self.active_layer = at;
        at
    }

    /// Soften a layer's mask edges with a box blur of the given
    /// radius (document pixels) — the "feather the mask" operation.
    pub fn feather_mask(&mut self, layer_idx: usize, radius: u32) {
        let (w, h) = (self.width as i32, self.height as i32);
        let layer = &mut self.layers[layer_idx];
        if layer.mask.is_none() || radius == 0 {
            return;
        }
        let mut plane: Vec<u8> = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                plane.push(layer.mask_value(x, y));
            }
        }
        let r = radius as i32;
        let mut out = vec![0u8; plane.len()];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n) = (0u32, 0u32);
                for dy in -r..=r {
                    for dx in -r..=r {
                        let (sx, sy) = (x + dx, y + dy);
                        if sx >= 0 && sy >= 0 && sx < w && sy < h {
                            sum += plane[(sy * w + sx) as usize] as u32;
                            n += 1;
                        }
                    }
                }
                out[(y * w + x) as usize] = (sum / n.max(1)) as u8;
            }
        }
        // Rewrite sparsely: tiles that are uniformly the fill value
        // are dropped back to the default.
        let fill = layer.mask_fill;
        if let Some(mask) = &mut layer.mask {
            mask.clear();
        }
        for y in 0..h {
            for x in 0..w {
                let v = out[(y * w + x) as usize];
                if v != fill {
                    layer.set_mask_value(x, y, v);
                }
            }
        }
    }

    /// The flattened CMYK pixel as it would print: layers composited
    /// in ink space, with any remaining transparency laid over white
    /// paper (no ink). Each component 0..=65535.
    pub fn render_cmyk_pixel(&self, x: i32, y: i32) -> (u16, u16, u16, u16) {
        let (ink, a) = self.composite_inks(x, y);
        let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
        (e(ink[0] * a), e(ink[1] * a), e(ink[2] * a), e(ink[3] * a))
    }

    /// Render a rectangle to packed RGBA8 (row-major, w*h*4 bytes).
    /// This is the Phase 0 "display" path — the GPU renderer later
    /// consumes the same tiles, not this buffer.
    pub fn render_region_rgba8(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
        let px = self.render_region_px(x, y, w, h);
        let mut out = vec![0u8; (w * h * 4) as usize];
        for (n, p) in px.iter().enumerate() {
            let p = p.to_rgba8();
            out[n * 4] = p.0;
            out[n * 4 + 1] = p.1;
            out[n * 4 + 2] = p.2;
            out[n * 4 + 3] = p.3;
        }
        out
    }

    pub fn render_full_rgba8(&self) -> Vec<u8> {
        self.render_region_rgba8(0, 0, self.width, self.height)
    }

    /// Save the current selection as a new alpha channel; returns its
    /// index. This is Select > Save Selection, and the Channels panel
    /// is where it lives afterwards.
    pub fn save_selection_as_channel(&mut self, sel: &crate::selection::Selection) -> usize {
        let n = self.alphas.len() + 1;
        let mut ch = AlphaChannel::new(format!("Alpha {n}"), self.width, self.height);
        ch.data.copy_from_slice(sel.raw_mask());
        self.alphas.push(ch);
        self.alphas.len() - 1
    }

    /// Load an alpha channel back as a selection.
    pub fn channel_as_selection(&self, idx: usize) -> Option<crate::selection::Selection> {
        let ch = self.alphas.get(idx)?;
        let mut sel = crate::selection::Selection::none(self.width, self.height);
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                sel.set(x, y, ch.value(self.width, x, y));
            }
        }
        Some(sel)
    }

    /// Simple FNV-1a checksum of the flattened render — used by tests
    /// and the CLI demo to prove save/load and undo didn't change the image.
    pub fn render_checksum(&self) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in self.render_full_rgba8() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::BlendMode;

    #[test]
    fn top_opaque_layer_wins() {
        let mut doc = Document::new(8, 8);
        doc.layers[0].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(255, 0, 0, 255));
        let top = doc.add_layer("Top");
        doc.layers[top].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(0, 0, 255, 255));
        assert_eq!(doc.render_pixel(4, 4).to_rgba8(), (0, 0, 255, 255));
    }

    #[test]
    fn hidden_layer_is_skipped() {
        let mut doc = Document::new(8, 8);
        doc.layers[0].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(255, 0, 0, 255));
        let top = doc.add_layer("Top");
        doc.layers[top].fill_rect(0, 0, 8, 8, Pixel16::from_rgba8(0, 0, 255, 255));
        doc.layers[top].visible = false;
        assert_eq!(doc.render_pixel(4, 4).to_rgba8(), (255, 0, 0, 255));
    }

    #[test]
    fn multiply_layer_darkens_as_expected() {
        let mut doc = Document::new(4, 4);
        doc.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(255, 255, 255, 255));
        let top = doc.add_layer("Shade");
        doc.layers[top].blend_mode = BlendMode::Multiply;
        doc.layers[top].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(128, 128, 128, 255));
        let px = doc.render_pixel(2, 2).to_rgba8();
        assert!(
            (126..=130).contains(&px.0),
            "multiply gray over white gave {}",
            px.0
        );
    }

    #[test]
    fn region_render_size_and_content() {
        let mut doc = Document::new(300, 300); // crosses tile boundary
        doc.layers[0].set_pixel(299, 299, Pixel16::from_rgba8(9, 9, 9, 255));
        let buf = doc.render_region_rgba8(290, 290, 10, 10);
        assert_eq!(buf.len(), 10 * 10 * 4);
        let i = (9 * 10 + 9) * 4;
        assert_eq!((buf[i], buf[i + 3]), (9, 255));
    }
}
