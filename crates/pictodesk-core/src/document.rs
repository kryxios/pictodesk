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
}

/// An alpha channel: a document-sized grayscale buffer. Holds saved
/// selections (Select > Save Selection) and, when `spot_color` is
/// set, a spot-color channel shown tinted in the Channels panel.
#[derive(Clone, Debug, PartialEq)]
pub struct AlphaChannel {
    pub name: String,
    pub spot_color: Option<(u8, u8, u8)>,
    pub data: Vec<u8>, // width * height, row-major
}

impl AlphaChannel {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        AlphaChannel {
            name: name.into(),
            spot_color: None,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub working_space: WorkingSpace,
    pub mode: ColorMode,
    pub alphas: Vec<AlphaChannel>,
    pub paths: Vec<crate::path::Path>,
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
            layers: vec![Layer::new("Background")],
            active_layer: 0,
            working_space: WorkingSpace::Srgb,
            mode: ColorMode::Rgb,
            alphas: Vec::new(),
            paths: Vec::new(),
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
                            let p = layer.get_pixel(dx, dy).to_f32s();
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
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            if let Some(adj) = &layer.adjustment {
                if acc.3 > 0.0 {
                    let t = layer.opacity * Self::mask_factor(layer, x, y);
                    if t > 0.0 {
                        let (r, g, b) = adj.apply(acc.0, acc.1, acc.2);
                        acc.0 += (r - acc.0) * t;
                        acc.1 += (g - acc.1) * t;
                        acc.2 += (b - acc.2) * t;
                    }
                }
                continue;
            }
            let buf = fx.iter().find(|b| b.li == li);
            if let Some(buf) = buf {
                let idx = ((y - buf.oy) as usize) * buf.w + (x - buf.ox) as usize;
                let bp = buf.below[idx].to_f32s();
                if bp.3 > 0.0 {
                    acc = composite_over(acc, bp, crate::blend::BlendMode::Normal, layer.opacity);
                }
                let mut p = buf.within[idx].to_f32s();
                let mut mode = layer.blend_mode;
                let mut opacity = layer.opacity;
                if mode == crate::blend::BlendMode::Dissolve {
                    let prob = opacity * p.3;
                    if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                        continue;
                    }
                    p.3 = 1.0;
                    opacity = 1.0;
                    mode = crate::blend::BlendMode::Normal;
                }
                acc = composite_over(acc, p, mode, opacity);
                continue;
            }
            let mut p = layer.get_pixel(x, y).to_f32s();
            p.3 *= Self::mask_factor(layer, x, y);
            let mut mode = layer.blend_mode;
            let mut opacity = layer.opacity;
            if mode == crate::blend::BlendMode::Dissolve {
                let prob = opacity * p.3;
                if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                    continue;
                }
                p.3 = 1.0;
                opacity = 1.0;
                mode = crate::blend::BlendMode::Normal;
            }
            acc = composite_over(acc, p, mode, opacity);
        }
        Pixel16::from_f32s(acc.0, acc.1, acc.2, acc.3)
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

    /// Flattened pixel at (x, y): all visible layers composited.
    /// CMYK documents composite in ink space and convert to RGB.
    pub fn render_pixel(&self, x: i32, y: i32) -> Pixel16 {
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
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            if let Some(adj) = &layer.adjustment {
                // Adjustment layer: transform the composite of
                // everything below, strength = opacity x mask.
                if acc.3 > 0.0 {
                    let t = layer.opacity * Self::mask_factor(layer, x, y);
                    if t > 0.0 {
                        let (r, g, b) = adj.apply(acc.0, acc.1, acc.2);
                        acc.0 += (r - acc.0) * t;
                        acc.1 += (g - acc.1) * t;
                        acc.2 += (b - acc.2) * t;
                    }
                }
                continue;
            }
            let mut p = layer.get_pixel(x, y).to_f32s();
            p.3 *= Self::mask_factor(layer, x, y);
            let mut mode = layer.blend_mode;
            let mut opacity = layer.opacity;
            if mode == crate::blend::BlendMode::Dissolve {
                // Dissolve: the pixel lands whole or not at all,
                // with probability = its final alpha, decided by a
                // stable per-pixel hash (no frame-to-frame shimmer).
                let prob = opacity * p.3;
                if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                    continue;
                }
                p.3 = 1.0;
                opacity = 1.0;
                mode = crate::blend::BlendMode::Normal;
            }
            acc = composite_over(acc, p, mode, opacity);
        }
        Pixel16::from_f32s(acc.0, acc.1, acc.2, acc.3)
    }

    /// Composite all visible layers at (x, y) in ink space; returns
    /// (c, m, y, k) in 0.0..=1.0 and the composited alpha.
    pub fn composite_inks(&self, x: i32, y: i32) -> ([f32; 4], f32) {
        let mut acc = ([0.0f32; 4], 0.0f32);
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            if let Some(adj) = &layer.adjustment {
                if acc.1 > 0.0 {
                    let t = layer.opacity * Self::mask_factor(layer, x, y);
                    if t > 0.0 {
                        let (r, g, b) = cmyk_to_rgb(acc.0[0], acc.0[1], acc.0[2], acc.0[3]);
                        let (r, g, b) = adj.apply(r, g, b);
                        let (ic, im, iy, ik) = crate::color::rgb_to_cmyk(r, g, b);
                        let ink = [ic, im, iy, ik];
                        for (acc_v, ink_v) in acc.0.iter_mut().zip(ink.iter()) {
                            *acc_v += (ink_v - *acc_v) * t;
                        }
                    }
                }
                continue;
            }
            let p = layer.get_pixel(x, y).to_f32s();
            let mut a = layer.get_alpha(x, y) as f32 / 65535.0 * Self::mask_factor(layer, x, y);
            let mut mode = layer.blend_mode;
            let mut opacity = layer.opacity;
            if mode == crate::blend::BlendMode::Dissolve {
                let prob = opacity * a;
                if prob <= 0.0 || crate::blend::dissolve_hash(x, y, li as u32) >= prob {
                    continue;
                }
                a = 1.0;
                opacity = 1.0;
                mode = crate::blend::BlendMode::Normal;
            }
            acc = composite_over_inks(acc, ([p.0, p.1, p.2, p.3], a), mode, opacity);
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
