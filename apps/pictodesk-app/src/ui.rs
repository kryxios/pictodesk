//! Classic-era UI kit: a CPU framebuffer with platinum-gray
//! panels, black pixel-font text, and the drawing primitives the
//! menu bar, toolbox, palettes, and dialogs are built from.
//! Everything is drawn in the style of the 1990 original: light gray
//! chrome, white/shadow bevels, inverted-black menu highlights.

use font8x8::UnicodeFonts;

pub type Rgb = (u8, u8, u8);
pub const PLATINUM: Rgb = (204, 204, 204);
pub const RULER_W: i32 = 18;
pub const RULER_H: i32 = 18;
pub const PLATINUM_LIGHT: Rgb = (221, 221, 221);
pub const WHITE: Rgb = (255, 255, 255);
pub const BLACK: Rgb = (0, 0, 0);
pub const SHADOW: Rgb = (102, 102, 102);
pub const DARK: Rgb = (68, 68, 68);
pub const DISABLED: Rgb = (140, 140, 140);
pub const CANVAS_SURROUND: Rgb = (105, 105, 105);
pub const ACCENT: Rgb = (36, 36, 36);

pub const MENU_H: i32 = 22;
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

    /// Classic raised bevel: white top/left, shadow bottom/right.
    pub fn bevel_raised(&mut self, r: Rect) {
        self.fill_rect(Rect::new(r.x, r.y, r.w, 1), WHITE);
        self.fill_rect(Rect::new(r.x, r.y, 1, r.h), WHITE);
        self.fill_rect(Rect::new(r.x, r.y + r.h - 1, r.w, 1), DARK);
        self.fill_rect(Rect::new(r.x + r.w - 1, r.y, 1, r.h), DARK);
        self.fill_rect(Rect::new(r.x + 1, r.y + r.h - 2, r.w - 2, 1), SHADOW);
        self.fill_rect(Rect::new(r.x + r.w - 2, r.y + 1, 1, r.h - 2), SHADOW);
    }

    pub fn bevel_sunken(&mut self, r: Rect) {
        self.fill_rect(Rect::new(r.x, r.y, r.w, 1), SHADOW);
        self.fill_rect(Rect::new(r.x, r.y, 1, r.h), SHADOW);
        self.fill_rect(Rect::new(r.x, r.y + r.h - 1, r.w, 1), WHITE);
        self.fill_rect(Rect::new(r.x + r.w - 1, r.y, 1, r.h), WHITE);
    }

    pub fn panel(&mut self, r: Rect) {
        self.fill_rect(r, PLATINUM);
        self.bevel_raised(r);
    }

    pub fn text_width(text: &str, scale: i32) -> i32 {
        text.chars().count() as i32 * 9 * scale - scale
    }

    /// Draw pixel-font text. Glyphs are font8x8 rows, bit 0 = leftmost
    /// pixel of the row (verified against the crate's renderer).
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: i32, c: Rgb) {
        let mut cx = x;
        for ch in text.chars() {
            if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                for (row, &bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) != 0 {
                            self.fill_rect(
                                Rect::new(cx + col * scale, y + row as i32 * scale, scale, scale),
                                c,
                            );
                        }
                    }
                }
            }
            cx += 9 * scale;
        }
    }

    pub fn text_centered(&mut self, r: Rect, text: &str, scale: i32, c: Rgb) {
        let tw = Self::text_width(text, scale);
        let th = 8 * scale;
        self.text(r.x + (r.w - tw) / 2, r.y + (r.h - th) / 2, text, scale, c);
    }

    /// A dialog button: platinum, raised; sunken + offset label when pressed.
    pub fn button(&mut self, r: Rect, label: &str, pressed: bool, enabled: bool) {
        self.fill_rect(r, PLATINUM);
        if pressed {
            self.bevel_sunken(r);
        } else {
            self.bevel_raised(r);
        }
        let c = if enabled { BLACK } else { DISABLED };
        let off = if pressed { 1 } else { 0 };
        self.text_centered(Rect::new(r.x + off, r.y + off, r.w, r.h), label, 1, c);
    }

    /// A horizontal slider: label left, track, knob at value position,
    /// numeric value right. Returns the track rect for hit-testing.
    pub fn slider(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        label: &str,
        value_text: &str,
        t: f32,
    ) -> Rect {
        self.text(x, y + 2, label, 1, BLACK);
        let track = Rect::new(x + 118, y + 3, w - 118 - 44, 8);
        self.fill_rect(track, WHITE);
        self.bevel_sunken(track);
        let knob_x = track.x + ((track.w - 8) as f32 * t.clamp(0.0, 1.0)) as i32;
        let knob = Rect::new(knob_x, y, 8, 14);
        self.fill_rect(knob, PLATINUM);
        self.bevel_raised(knob);
        self.text(track.right() + 6, y + 2, value_text, 1, BLACK);
        Rect::new(x, y, w, 14)
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
