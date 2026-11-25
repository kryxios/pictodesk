//! Modern UI kit: a CPU framebuffer with a dark professional
//! theme (dark panels, light text, one blue accent), and the
//! drawing primitives the menu bar, toolbox, palettes, and
//! dialogs are built from. Flat surfaces with 1px borders; the
//! old raised/sunken bevels are kept as API but render flat.
//!
//! Text is real antialiased type, not a pixel font: Liberation
//! Sans (SIL OFL) embedded as base64 text, rasterized with
//! fontdue, glyphs cached. Scale 1 = 12px regular, scale 2 =
//! 18px bold. Callers keep the old (x, y, text, scale, color)
//! signatures; widths are real advances now, not 9px/char.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub type Rgb = (u8, u8, u8);
// Dark theme (modern shell, decided 2026-11-25): panels in the
// mid-50s, fields near-black, light text, one blue accent. The
// old PLATINUM/SHADOW/DARK names are kept as aliases so the
// hundreds of chrome call sites restyle without edits.
pub const PLATINUM: Rgb = (56, 56, 56);
pub const RULER_W: i32 = 18;
pub const RULER_H: i32 = 18;
pub const PLATINUM_LIGHT: Rgb = (74, 74, 74);
pub const WHITE: Rgb = (255, 255, 255);
pub const BLACK: Rgb = (0, 0, 0);
pub const SHADOW: Rgb = (24, 24, 24);
pub const DARK: Rgb = (40, 40, 40);
pub const DISABLED: Rgb = (120, 120, 120);
pub const CANVAS_SURROUND: Rgb = (32, 32, 32);
pub const ACCENT: Rgb = (49, 168, 255);
pub const TEXT: Rgb = (232, 232, 232);
pub const TEXT_DIM: Rgb = (164, 164, 164);
pub const FIELD: Rgb = (32, 32, 32);
pub const ICON: Rgb = (232, 232, 232);
pub const HOVER: Rgb = (76, 76, 76);
/// Soft hairline for panel/control edges — a divider, not a black frame.
pub const EDGE: Rgb = (44, 44, 44);
/// Slightly stronger edge for rounded dialogs and pop-ups.
pub const EDGE_SOFT: Rgb = (64, 64, 64);
/// Mid-gray edge for sunken/raised controls (reads on dark and white).
pub const EDGE_MID: Rgb = (88, 88, 88);
pub const SELROW: Rgb = (62, 92, 128);

pub const MENU_H: i32 = 22;
pub const OPTIONS_H: i32 = 26;
pub const TOOLBAR_W: i32 = 54;
pub const STATUS_H: i32 = 18;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>, // RGBA8
}

impl Frame {
    pub fn new(w: u32, h: u32) -> Self {
        Frame {
            w,
            h,
            px: vec![0; (w * h * 4) as usize],
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.w = w;
        self.h = h;
        self.px = vec![0; (w * h * 4) as usize];
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        self.px[i] = c.0;
        self.px[i + 1] = c.1;
        self.px[i + 2] = c.2;
        self.px[i + 3] = 255;
    }

    pub fn fill_rect(&mut self, r: Rect, c: Rgb) {
        for yy in r.y..r.y + r.h {
            for xx in r.x..r.x + r.w {
                self.set(xx, yy, c);
            }
        }
    }

    pub fn outline_rect(&mut self, r: Rect, c: Rgb) {
        self.fill_rect(Rect::new(r.x, r.y, r.w, 1), c);
        self.fill_rect(Rect::new(r.x, r.y + r.h - 1, r.w, 1), c);
        self.fill_rect(Rect::new(r.x, r.y, 1, r.h), c);
        self.fill_rect(Rect::new(r.x + r.w - 1, r.y, 1, r.h), c);
    }

    /// Flat raised edge: 1px border + a light top line. Draws
    /// edges ONLY — callers fill the rect first (graphs, text
    /// fields and lists keep their own backgrounds).
    pub fn bevel_raised(&mut self, r: Rect) {
        self.outline_rect(r, EDGE_MID);
        self.fill_rect(Rect::new(r.x, r.y, r.w, 1), PLATINUM_LIGHT);
    }

    /// Flat sunken edge: 1px border only; callers fill first.
    pub fn bevel_sunken(&mut self, r: Rect) {
        self.outline_rect(r, EDGE_MID);
    }

    pub fn panel(&mut self, r: Rect) {
        self.fill_rect(r, PLATINUM);
        self.outline_rect(r, EDGE);
    }

    /// Fill a rect with rounded corners (radius in px).
    pub fn fill_rounded(&mut self, r: Rect, rad: i32, c: Rgb) {
        let rad = rad.clamp(0, (r.w.min(r.h)) / 2);
        // Center block + side strips, then rounded corners.
        self.fill_rect(Rect::new(r.x + rad, r.y, r.w - 2 * rad, r.h), c);
        self.fill_rect(Rect::new(r.x, r.y + rad, r.w, r.h - 2 * rad), c);
        for (cx, cy) in [
            (r.x + rad, r.y + rad),
            (r.right() - rad - 1, r.y + rad),
            (r.x + rad, r.bottom() - rad - 1),
            (r.right() - rad - 1, r.bottom() - rad - 1),
        ] {
            for dy in -rad..=0i32 {
                for dx in -rad..=0i32 {
                    if dx * dx + dy * dy <= rad * rad {
                        let (px, py) = match (cx, cy) {
                            (x, y) if x == r.x + rad && y == r.y + rad => (x + dx, y + dy),
                            (x, y) if x == r.right() - rad - 1 && y == r.y + rad => {
                                (x - dx, y + dy)
                            }
                            (x, y) if x == r.x + rad && y == r.bottom() - rad - 1 => {
                                (x + dx, y - dy)
                            }
                            (x, y) => (x - dx, y - dy),
                        };
                        self.set(px, py, c);
                    }
                }
            }
        }
    }

    /// 1px rounded outline.
    pub fn outline_rounded(&mut self, r: Rect, rad: i32, c: Rgb) {
        let rad = rad.clamp(0, (r.w.min(r.h)) / 2);
        let (x0, y0, x1, y1) = (r.x, r.y, r.right() - 1, r.bottom() - 1);
        self.fill_rect(Rect::new(x0 + rad, y0, r.w - 2 * rad, 1), c);
        self.fill_rect(Rect::new(x0 + rad, y1, r.w - 2 * rad, 1), c);
        self.fill_rect(Rect::new(x0, y0 + rad, 1, r.h - 2 * rad), c);
        self.fill_rect(Rect::new(x1, y0 + rad, 1, r.h - 2 * rad), c);
        for (cx, cy, sx, sy) in [
            (x0 + rad, y0 + rad, -1, -1),
            (x1 - rad, y0 + rad, 1, -1),
            (x0 + rad, y1 - rad, -1, 1),
            (x1 - rad, y1 - rad, 1, 1),
        ] {
            let mut a = 0.0f32;
            while a <= std::f32::consts::FRAC_PI_2 {
                let dx = (rad as f32 * a.cos()).round() as i32;
                let dy = (rad as f32 * a.sin()).round() as i32;
                self.set(cx + sx * dx, cy + sy * dy, c);
                a += 0.08;
            }
        }
    }

    /// A dialog/pop-up surface: rounded corners, soft edge.
    pub fn panel_rounded(&mut self, r: Rect, rad: i32) {
        self.fill_rounded(r, rad, PLATINUM);
        self.outline_rounded(r, rad, EDGE_SOFT);
    }

    /// Blend one pixel of `c` over the frame at coverage `a`
    /// (0..255): the antialiasing edge for rasterized type.
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, a: u8) {
        if a == 0 || x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        let (a, inv) = (a as u32, 255 - a as u32);
        self.px[i] = ((c.0 as u32 * a + self.px[i] as u32 * inv) / 255) as u8;
        self.px[i + 1] = ((c.1 as u32 * a + self.px[i + 1] as u32 * inv) / 255) as u8;
        self.px[i + 2] = ((c.2 as u32 * a + self.px[i + 2] as u32 * inv) / 255) as u8;
    }

    /// Ink width of a run: advances of all but the last glyph plus
    /// the last glyph's ink width (no trailing advance).
    pub fn text_width(text: &str, scale: i32) -> i32 {
        let (px, bold) = text_px(scale);
        let mut w = 0.0f32;
        let mut last: Option<Arc<Glyph>> = None;
        for ch in text.chars() {
            if let Some(g) = last.take() {
                w += g.advance;
            }
            last = Some(glyph(ch, px, bold));
        }
        if let Some(g) = last {
            w += (g.xmin + g.w) as f32;
        }
        w.ceil() as i32
    }

    /// Draw antialiased text, top-left at (x, y).
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: i32, c: Rgb) {
        let (px, bold) = text_px(scale);
        let font = ui_font(bold);
        let baseline = y + font
            .horizontal_line_metrics(px)
            .map(|m| m.ascent)
            .unwrap_or(px * 0.8) as i32;
        let mut pen = x as f32;
        for ch in text.chars() {
            let g = glyph(ch, px, bold);
            if !g.bitmap.is_empty() {
                // fontdue: the bitmap's bottom row sits at
                // baseline - ymin - 1, so its top row is at
                // baseline - height - ymin.
                let top = baseline - g.h - g.ymin;
                for gy in 0..g.h {
                    for gx in 0..g.w {
                        let a = g.bitmap[(gy * g.w + gx) as usize];
                        if a > 0 {
                            self.blend(pen as i32 + g.xmin + gx, top + gy, c, a);
                        }
                    }
                }
            }
            pen += g.advance;
        }
    }

    /// Draw text rotated 90° counter-clockwise, reading
    /// bottom-to-top (vertical ruler). `(x, y)` is the bottom-left
    /// corner of the rotated run.
    pub fn text_vertical(&mut self, x: i32, y: i32, text: &str, scale: i32, c: Rgb) {
        let (px, bold) = text_px(scale);
        let font = ui_font(bold);
        let ascent = font
            .horizontal_line_metrics(px)
            .map(|m| m.ascent)
            .unwrap_or(px * 0.8) as i32;
        let mut pen = 0.0f32; // advance along the run (upward after rotation)
        for ch in text.chars() {
            let g = glyph(ch, px, bold);
            for gy in 0..g.h {
                for gx in 0..g.w {
                    let a = g.bitmap[(gy * g.w + gx) as usize];
                    if a > 0 {
                        // Unrotated: u = along the run, v = down from
                        // the text top (bitmap top = ascent - h - ymin).
                        // Rotated 90° CCW: (x + v, y - u).
                        let u = pen as i32 + g.xmin + gx;
                        let v = ascent - g.h - g.ymin + gy;
                        self.blend(x + v, y - u, c, a);
                    }
                }
            }
            pen += g.advance;
        }
    }

    pub fn text_centered(&mut self, r: Rect, text: &str, scale: i32, c: Rgb) {
        let tw = Self::text_width(text, scale);
        let (px, _) = text_px(scale);
        let th = px as i32;
        self.text(r.x + (r.w - tw) / 2, r.y + (r.h - th) / 2, text, scale, c);
    }

    /// A dialog button: flat fill only — no border. The fill
    /// contrast against the panel is the button's edge.
    pub fn button(&mut self, r: Rect, label: &str, pressed: bool, enabled: bool) {
        self.fill_rect(
            r,
            if pressed {
                (58, 96, 138)
            } else {
                PLATINUM_LIGHT
            },
        );
        let c = if enabled { TEXT } else { DISABLED };
        self.text_centered(r, label, 1, c);
    }

    /// A horizontal slider: label left, dark track with an accent
    /// fill up to the knob, light round-ish knob, numeric value
    /// right. Returns the track rect for hit-testing.
    pub fn slider(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        label: &str,
        value_text: &str,
        t: f32,
    ) -> Rect {
        self.text(x, y + 2, label, 1, TEXT);
        let track = Rect::new(x + 118, y + 5, w - 118 - 44, 4);
        self.fill_rect(track, FIELD);
        let t = t.clamp(0.0, 1.0);
        let fill_w = ((track.w as f32) * t) as i32;
        if fill_w > 0 {
            self.fill_rect(Rect::new(track.x, track.y, fill_w, track.h), ACCENT);
        }
        let knob_x = track.x + ((track.w - 10) as f32 * t) as i32;
        let knob = Rect::new(knob_x, y, 10, 14);
        self.fill_rounded(knob, 3, (208, 208, 208));
        self.text(track.right() + 6, y + 2, value_text, 1, TEXT);
        Rect::new(x, y, w, 14)
    }

    /// Circle outline by parametric plot (used for cursor rings
    /// and the clone/heal source crosshair).
    pub fn circle_outline(&mut self, cx: i32, cy: i32, r: i32, c: Rgb) {
        if r < 1 {
            self.set(cx, cy, c);
            return;
        }
        let steps = (r * 4).clamp(24, 128);
        for a in 0..steps {
            let t = a as f32 / steps as f32 * std::f32::consts::TAU;
            self.set(
                cx + (t.cos() * r as f32) as i32,
                cy + (t.sin() * r as f32) as i32,
                c,
            );
        }
    }

    /// Marching ants: dashes along a rect border, phase-animated.
    pub fn marching_ants_rect(&mut self, r: Rect, phase: i32) {
        let dash = |x: i32, y: i32, f: &mut Frame| {
            let on = ((x + y + phase) / 4) % 2 == 0;
            f.set(x, y, if on { WHITE } else { BLACK });
        };
        for xx in r.x..r.x + r.w {
            dash(xx, r.y, self);
            dash(xx, r.y + r.h - 1, self);
        }
        for yy in r.y..r.y + r.h {
            dash(r.x, yy, self);
            dash(r.x + r.w - 1, yy, self);
        }
    }

    pub fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: Rgb) {
        // Bresenham
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

/// One rasterized glyph: bitmap coverage plus placement.
struct Glyph {
    w: i32,
    h: i32,
    xmin: i32,
    ymin: i32,
    advance: f32,
    bitmap: Vec<u8>,
}

/// px size and weight for a scale: 1 = 12px regular, 2 = 18px bold.
fn text_px(scale: i32) -> (f32, bool) {
    match scale {
        2 => (18.0, true),
        s => (12.0 * s.max(1) as f32, false),
    }
}

/// The embedded UI fonts, decoded once.
fn ui_font(bold: bool) -> &'static fontdue::Font {
    static FONTS: OnceLock<(fontdue::Font, fontdue::Font)> = OnceLock::new();
    let (reg, bld) = FONTS.get_or_init(|| {
        let reg = decode_b64(include_str!("../LiberationSans-Regular.b64"));
        let bold = decode_b64(include_str!("../LiberationSans-Bold.b64"));
        (
            fontdue::Font::from_bytes(reg, fontdue::FontSettings::default())
                .expect("LiberationSans-Regular"),
            fontdue::Font::from_bytes(bold, fontdue::FontSettings::default())
                .expect("LiberationSans-Bold"),
        )
    });
    if bold {
        bld
    } else {
        reg
    }
}

type GlyphCache = Mutex<HashMap<(char, u32, bool), Arc<Glyph>>>;

fn glyph(ch: char, px: f32, bold: bool) -> Arc<Glyph> {
    static CACHE: OnceLock<GlyphCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (ch, px.to_bits(), bold);
    if let Some(g) = cache.lock().unwrap().get(&key) {
        return Arc::clone(g);
    }
    let font = ui_font(bold);
    let (m, bitmap) = font.rasterize(ch, px);
    let advance = if m.advance_width > 0.0 {
        m.advance_width
    } else {
        px * 0.5
    };
    let g = Arc::new(Glyph {
        w: m.width as i32,
        h: m.height as i32,
        xmin: m.xmin,
        ymin: m.ymin,
        advance,
        bitmap,
    });
    cache.lock().unwrap().insert(key, Arc::clone(&g));
    g
}

/// Minimal base64 decoder (whitespace-tolerant, no new dependency).
fn decode_b64(s: &str) -> Vec<u8> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = s
        .bytes()
        .filter(|b| val(*b).is_some() || *b == b'=')
        .collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for quad in clean.chunks(4) {
        if quad.len() < 2 {
            break;
        }
        let n: Vec<u8> = quad.iter().filter_map(|b| val(*b)).collect();
        if n.len() < 2 {
            break;
        }
        out.push((n[0] << 2) | (n[1] >> 4));
        if n.len() > 2 {
            out.push(((n[1] & 0x0f) << 4) | (n[2] >> 2));
        }
        if n.len() > 3 {
            out.push(((n[2] & 0x03) << 6) | n[3]);
        }
    }
    out
}

/// The classic preset palette of colors for the color
/// picker panel (a compact 8x8 grid + gray ramp is drawn by caller).
pub const PALETTE: [Rgb; 64] = [
    (0, 0, 0),
    (34, 34, 34),
    (68, 68, 68),
    (102, 102, 102),
    (136, 136, 136),
    (170, 170, 170),
    (204, 204, 204),
    (255, 255, 255),
    (255, 0, 0),
    (255, 128, 0),
    (255, 255, 0),
    (128, 255, 0),
    (0, 255, 0),
    (0, 255, 128),
    (0, 255, 255),
    (0, 128, 255),
    (0, 0, 255),
    (128, 0, 255),
    (255, 0, 255),
    (255, 0, 128),
    (128, 0, 0),
    (128, 64, 0),
    (128, 128, 0),
    (64, 128, 0),
    (0, 128, 0),
    (0, 128, 64),
    (0, 128, 128),
    (0, 64, 128),
    (0, 0, 128),
    (64, 0, 128),
    (128, 0, 128),
    (128, 0, 64),
    (255, 170, 170),
    (255, 213, 170),
    (255, 255, 170),
    (213, 255, 170),
    (170, 255, 170),
    (170, 255, 213),
    (170, 255, 255),
    (170, 213, 255),
    (170, 170, 255),
    (213, 170, 255),
    (255, 170, 255),
    (255, 170, 213),
    (170, 85, 85),
    (170, 128, 85),
    (170, 170, 85),
    (128, 170, 85),
    (85, 170, 85),
    (85, 170, 128),
    (85, 170, 170),
    (85, 128, 170),
    (85, 85, 170),
    (128, 85, 170),
    (170, 85, 170),
    (170, 85, 128),
    (255, 220, 180),
    (240, 200, 150),
    (210, 170, 120),
    (180, 140, 95),
    (150, 110, 70),
    (120, 85, 50),
    (90, 60, 35),
    (60, 40, 22),
];
