//! The basic Histogram palette (Window > Histogram): per-channel
//! counts over the flattened image, plus mean and median luminance.

use crate::document::Document;

#[derive(Clone, Debug)]
pub struct Histogram {
    pub r: [u32; 256],
    pub g: [u32; 256],
    pub b: [u32; 256],
    pub luminance: [u32; 256],
    pub pixels: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Histogram {
            r: [0; 256],
            g: [0; 256],
            b: [0; 256],
            luminance: [0; 256],
            pixels: 0,
        }
    }
}

impl Histogram {
    pub fn of_document(doc: &Document) -> Self {
        let mut hist = Histogram::default();
        let img = doc.render_full_rgba8();
        for px in img.as_chunks::<4>().0 {
            let (r, g, b) = (px[0] as usize, px[1] as usize, px[2] as usize);
            hist.r[r] += 1;
            hist.g[g] += 1;
            hist.b[b] += 1;
            let lum = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32 + 0.5) as usize;
            hist.luminance[lum.min(255)] += 1;
            hist.pixels += 1;
        }
        hist
    }

    pub fn mean_luminance(&self) -> f32 {
        if self.pixels == 0 {
            return 0.0;
        }
        let sum: u64 = self
            .luminance
            .iter()
            .enumerate()
            .map(|(i, &c)| i as u64 * c as u64)
            .sum();
        sum as f32 / self.pixels as f32
    }

    pub fn median_luminance(&self) -> u8 {
        let half = self.pixels / 2;
        let mut acc = 0u64;
        for (i, &c) in self.luminance.iter().enumerate() {
            acc += c as u64;
            if acc >= half {
                return i as u8;
            }
        }
        255
    }

    /// Highest bin across all channels — for scaling the palette graph.
    pub fn peak(&self) -> u32 {
        let mut m = 1;
        for i in 0..256 {
            m = m
                .max(self.r[i])
                .max(self.g[i])
                .max(self.b[i])
                .max(self.luminance[i]);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Pixel16;

    #[test]
    fn flat_color_histogram_is_a_spike() {
        let mut doc = Document::new(4, 4);
        doc.layers[0].fill_rect(0, 0, 4, 4, Pixel16::from_rgba8(10, 20, 30, 255));
        let h = Histogram::of_document(&doc);
        assert_eq!(h.pixels, 16);
        assert_eq!(h.r[10], 16);
        assert_eq!(h.g[20], 16);
        assert_eq!(
            h.median_luminance(),
            (0.299 * 10.0 + 0.587 * 20.0 + 0.114 * 30.0 + 0.5) as u8
        );
    }
}
