//! The document: canvas size, working space, and a stack of layers
//! (index 0 = bottom). Rendering composites layers bottom-up through
//! the blend/composite core into an 8-bit buffer for display/export.

use crate::blend::{composite_over, composite_over_inks};
use crate::color::{cmyk_to_rgb, WorkingSpace};
use crate::layer::Layer;
use crate::pixel::Pixel16;

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
        }
    }

    pub fn add_layer(&mut self, name: impl Into<String>) -> usize {
        self.layers.push(Layer::new(name));
        self.active_layer = self.layers.len() - 1;
        self.active_layer
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

    /// Flattened pixel at (x, y): all visible layers composited.
    /// CMYK documents composite in ink space and convert to RGB.
    pub fn render_pixel(&self, x: i32, y: i32) -> Pixel16 {
        if self.mode == ColorMode::Cmyk {
            let (ink, a) = self.composite_inks(x, y);
            let (r, g, b) = cmyk_to_rgb(ink[0], ink[1], ink[2], ink[3]);
            return Pixel16::from_f32s(r, g, b, a);
        }
        let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            let p = layer.get_pixel(x, y).to_f32s();
            acc = composite_over(acc, p, layer.blend_mode, layer.opacity);
        }
        Pixel16::from_f32s(acc.0, acc.1, acc.2, acc.3)
    }

    /// Composite all visible layers at (x, y) in ink space; returns
    /// (c, m, y, k) in 0.0..=1.0 and the composited alpha.
    pub fn composite_inks(&self, x: i32, y: i32) -> ([f32; 4], f32) {
        let mut acc = ([0.0f32; 4], 0.0f32);
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            let p = layer.get_pixel(x, y).to_f32s();
            let a = layer.get_alpha(x, y) as f32 / 65535.0;
            acc = composite_over_inks(
                acc,
                ([p.0, p.1, p.2, p.3], a),
                layer.blend_mode,
                layer.opacity,
            );
        }
        acc
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
        let mut out = vec![0u8; (w * h * 4) as usize];
        for yy in 0..h as i32 {
            for xx in 0..w as i32 {
                let p = self.render_pixel(x + xx, y + yy).to_rgba8();
                let i = ((yy as u32 * w + xx as u32) * 4) as usize;
                out[i] = p.0;
                out[i + 1] = p.1;
                out[i + 2] = p.2;
                out[i + 3] = p.3;
            }
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
