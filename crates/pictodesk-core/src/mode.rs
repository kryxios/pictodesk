//! Color mode conversion — Image > Mode in classic terms.
//!
//! Converting to CMYK rewrites the layers as real CMYK storage:
//! ink values in the tile slots plus a per-layer alpha plane (see
//! `ColorMode`). Converting back folds the inks and alpha back into
//! RGB pixels. Converting to Grayscale desaturates for real.

use crate::adjust;
use crate::color::{cmyk_to_rgb, rgb_to_cmyk, WorkingSpace};
use crate::document::{ColorMode, Document};
use crate::pixel::Pixel16;

/// Convert the document's RGB data from its current working
/// space to `to` (Image > Mode > Convert to Profile, Phase 5):
/// every painted pixel is re-encoded through XYZ, so the image
/// looks the same while its numbers change. CMYK documents keep
/// their inks — only the RGB interpretation space changes.
pub fn convert_working_space(doc: &mut Document, to: WorkingSpace) {
    let from = doc.working_space;
    if from == to {
        return;
    }
    if doc.mode != ColorMode::Cmyk {
        for li in 0..doc.layers.len() {
            if doc.layers[li].adjustment.is_some() {
                continue;
            }
            adjust::map_layer_pixels(doc, li, None, |p| {
                let (r, g, b, a) = p.to_f32s();
                let (nr, ng, nb) = from.convert_rgb((r, g, b), to);
                Pixel16::from_f32s(nr, ng, nb, a)
            });
        }
    }
    doc.working_space = to;
}

pub fn convert_mode(doc: &mut Document, to: ColorMode) {
    if doc.mode == to {
        return;
    }
    // Hub: bring storage to RGB first, then out to the target.
    // Bitmap / Indexed / Duotone / Multichannel have their own
    // parameterized converters (mode_bitmap / mode_indexed /
    // mode_duotone); convert_mode reaches them with era
    // defaults so internal callers stay simple.
    match doc.mode {
        ColorMode::Cmyk => cmyk_storage_to_rgb(doc),
        ColorMode::Lab => lab_storage_to_rgb(doc),
        ColorMode::Duotone => {
            doc.duotone = None;
        }
        ColorMode::Multichannel => {
            // Planes were built from the flattened CMYK layer;
            // sync any plane edits back, then the generic
            // CMYK-out path below takes over.
            multichannel_sync_to_layer(doc);
            doc.multi_channels.clear();
            doc.multi_planes.clear();
            doc.mode = ColorMode::Cmyk;
            cmyk_storage_to_rgb(doc);
        }
        _ => {}
    }
    // Every exit path above leaves RGB-shaped storage.
    doc.mode = ColorMode::Rgb;
    match to {
        ColorMode::Rgb => {}
        ColorMode::Grayscale => {
            for layer in 0..doc.layers.len() {
                adjust::desaturate(doc, layer, None);
            }
        }
        ColorMode::Cmyk => rgb_storage_to_cmyk(doc),
        ColorMode::Lab => rgb_storage_to_lab(doc),
        ColorMode::Bitmap => {
            for layer in 0..doc.layers.len() {
                adjust::desaturate(doc, layer, None);
            }
            doc.mode = ColorMode::Grayscale;
            to_bitmap(doc, 1, 0.5);
            return; // to_bitmap sets the mode itself
        }
        ColorMode::Indexed => {
            doc.mode = ColorMode::Rgb;
            to_indexed(doc, 0, 256, true);
            return; // to_indexed sets the mode itself
        }
        ColorMode::Duotone => {
            for layer in 0..doc.layers.len() {
                adjust::desaturate(doc, layer, None);
            }
            doc.mode = ColorMode::Grayscale;
            to_duotone(doc, crate::document::DuotoneSpec::default());
            return; // to_duotone sets the mode itself
        }
        ColorMode::Multichannel => {
            doc.mode = ColorMode::Rgb;
            to_multichannel(doc);
            return; // to_multichannel sets the mode itself
        }
    }
    doc.mode = to;
}

/// Rewrite every layer's RGB pixels as Lab slot values
/// (Phase 6.5). Direct tile loop — NOT map_layer_pixels,
/// whose Lab awareness would fight the conversion.
fn rgb_storage_to_lab(doc: &mut Document) {
    for li in 0..doc.layers.len() {
        if doc.layers[li].adjustment.is_some() {
            continue;
        }
        let coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
        for coord in coords {
            let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
            for ly in 0..256u32 {
                for lx in 0..256u32 {
                    let p = tile.get(lx, ly);
                    if p.a == 0 {
                        continue;
                    }
                    let (r, g, b, a) = p.to_f32s();
                    let (l, la, lb) = crate::color::rgb_to_lab(r, g, b);
                    let (sr, sg, sb) = crate::color::lab_to_slots(l, la, lb);
                    tile.set(lx, ly, Pixel16::from_f32s(sr, sg, sb, a));
                }
            }
        }
    }
}

/// Rewrite Lab slot values back to RGB pixels.
fn lab_storage_to_rgb(doc: &mut Document) {
    for li in 0..doc.layers.len() {
        if doc.layers[li].adjustment.is_some() {
            continue;
        }
        let coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
        for coord in coords {
            let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
            for ly in 0..256u32 {
                for lx in 0..256u32 {
                    let p = tile.get(lx, ly);
                    if p.a == 0 {
                        continue;
                    }
                    let (r, g, b, a) = p.to_f32s();
                    let (l, la, lb) = crate::color::slots_to_lab(r, g, b);
                    let (r, g, b) = crate::color::lab_to_rgb(l, la, lb);
                    tile.set(lx, ly, Pixel16::from_f32s(r, g, b, a));
                }
            }
        }
    }
}

/// Bitmap conversion (Phase 6.5): the document must already
/// hold grayscale pixels. Flattens (era behavior) and reduces
/// to pure black/white. Method: 0 Threshold (at `threshold`
/// 0..=1; the era's 50% Threshold is threshold = 0.5),
/// 1 Pattern Dither (4x4 Bayer), 2 Diffusion Dither
/// (Floyd-Steinberg).
pub fn to_bitmap(doc: &mut Document, method: u8, threshold: f32) {
    doc.flatten();
    let (w, h) = (doc.width as i32, doc.height as i32);
    let mut gray = vec![0.0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let p = doc.layers[0].get_pixel(x, y).to_f32s();
            gray[(y * w + x) as usize] = 0.299 * p.0 + 0.587 * p.1 + 0.114 * p.2;
        }
    }
    let mut out = vec![false; (w * h) as usize]; // true = white
    match method {
        0 => {
            for (i, g) in gray.iter().enumerate() {
                out[i] = *g >= threshold;
            }
        }
        1 => {
            const BAYER: [[u8; 4]; 4] =
                [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
            for y in 0..h {
                for x in 0..w {
                    let t = (BAYER[(y % 4) as usize][(x % 4) as usize] as f32 + 0.5) / 16.0;
                    out[(y * w + x) as usize] = gray[(y * w + x) as usize] >= t;
                }
            }
        }
        _ => {
            let mut buf = gray.clone();
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let old = buf[i];
                    let white = old >= 0.5;
                    out[i] = white;
                    let err = old - if white { 1.0 } else { 0.0 };
                    let spread = |buf: &mut Vec<f32>, x: i32, y: i32, f: f32| {
                        if x >= 0 && x < w && y >= 0 && y < h {
                            buf[(y * w + x) as usize] += err * f;
                        }
                    };
                    spread(&mut buf, x + 1, y, 7.0 / 16.0);
                    spread(&mut buf, x - 1, y + 1, 3.0 / 16.0);
                    spread(&mut buf, x, y + 1, 5.0 / 16.0);
                    spread(&mut buf, x + 1, y + 1, 1.0 / 16.0);
                }
            }
        }
    }
    let black = Pixel16::from_rgba8(0, 0, 0, 255);
    let white = Pixel16::from_rgba8(255, 255, 255, 255);
    for y in 0..h {
        for x in 0..w {
            let a = doc.layers[0].get_pixel(x, y).to_f32s().3;
            let v = if out[(y * w + x) as usize] {
                white
            } else {
                black
            };
            doc.layers[0].set_pixel(
                x,
                y,
                Pixel16::from_f32s(v.to_f32s().0, v.to_f32s().1, v.to_f32s().2, a.max(1.0)),
            );
        }
    }
    doc.mode = ColorMode::Bitmap;
}

/// Indexed conversion (Phase 6.5): flattens (era behavior),
/// builds a palette, and snaps every pixel to it. Palette:
/// 0 Adaptive (median-cut to `colors` entries), 1 Web (216),
/// 2 Grayscale ramp (`colors` entries). `dither` applies
/// Floyd-Steinberg diffusion during the mapping.
pub fn to_indexed(doc: &mut Document, palette: u8, colors: u16, dither: bool) {
    doc.flatten();
    let (w, h) = (doc.width as i32, doc.height as i32);
    let mut src = vec![(0u8, 0u8, 0u8); (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (r, g, b, _) = doc.layers[0].get_pixel(x, y).to_rgba8();
            src[(y * w + x) as usize] = (r, g, b);
        }
    }
    let table: Vec<(u8, u8, u8)> = match palette {
        1 => {
            let steps = [0u8, 51, 102, 153, 204, 255];
            let mut t = Vec::with_capacity(216);
            for &r in &steps {
                for &g in &steps {
                    for &b in &steps {
                        t.push((r, g, b));
                    }
                }
            }
            t
        }
        2 => {
            let n = colors.clamp(2, 256) as u32;
            (0..n)
                .map(|i| {
                    let v = (i * 255 / (n - 1)) as u8;
                    (v, v, v)
                })
                .collect()
        }
        _ => median_cut_palette(&src, colors.clamp(2, 256) as usize),
    };
    let nearest =
        |cache: &mut std::collections::HashMap<(u8, u8, u8), usize>, c: (u8, u8, u8)| -> usize {
            if let Some(&i) = cache.get(&c) {
                return i;
            }
            let mut best = 0usize;
            let mut bd = i64::MAX;
            for (i, &p) in table.iter().enumerate() {
                let dr = (c.0 as i64 - p.0 as i64) * 3;
                let dg = (c.1 as i64 - p.1 as i64) * 5;
                let db = (c.2 as i64 - p.2 as i64) * 2;
                let d = dr * dr + dg * dg + db * db;
                if d < bd {
                    bd = d;
                    best = i;
                }
            }
            cache.insert(c, best);
            best
        };
    let mut cache = std::collections::HashMap::new();
    if dither {
        let mut buf: Vec<[f32; 3]> = src
            .iter()
            .map(|c| [c.0 as f32, c.1 as f32, c.2 as f32])
            .collect();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let cur = (
                    buf[i][0].clamp(0.0, 255.0) as u8,
                    buf[i][1].clamp(0.0, 255.0) as u8,
                    buf[i][2].clamp(0.0, 255.0) as u8,
                );
                let pi = nearest(&mut cache, cur);
                let pc = table[pi];
                src[i] = pc;
                let err = [
                    buf[i][0] - pc.0 as f32,
                    buf[i][1] - pc.1 as f32,
                    buf[i][2] - pc.2 as f32,
                ];
                let spread = |buf: &mut Vec<[f32; 3]>, x: i32, y: i32, f: f32| {
                    if x >= 0 && x < w && y >= 0 && y < h {
                        let j = (y * w + x) as usize;
                        for c in 0..3 {
                            buf[j][c] += err[c] * f;
                        }
                    }
                };
                spread(&mut buf, x + 1, y, 7.0 / 16.0);
                spread(&mut buf, x - 1, y + 1, 3.0 / 16.0);
                spread(&mut buf, x, y + 1, 5.0 / 16.0);
                spread(&mut buf, x + 1, y + 1, 1.0 / 16.0);
            }
        }
    } else {
        for c in src.iter_mut() {
            *c = table[nearest(&mut cache, *c)];
        }
    }
    for y in 0..h {
        for x in 0..w {
            let c = src[(y * w + x) as usize];
            let a = doc.layers[0].get_pixel(x, y).to_f32s().3;
            doc.layers[0].set_pixel(
                x,
                y,
                Pixel16::from_f32s(
                    c.0 as f32 / 255.0,
                    c.1 as f32 / 255.0,
                    c.2 as f32 / 255.0,
                    a.max(1.0),
                ),
            );
        }
    }
    doc.color_table = table;
    doc.mode = ColorMode::Indexed;
}

/// Median-cut palette over 8-bit RGB samples.
fn median_cut_palette(samples: &[(u8, u8, u8)], target: usize) -> Vec<(u8, u8, u8)> {
    // Stride-sample very large images to bound the sort cost.
    let stride = (samples.len() / 262_144).max(1);
    let mut pool: Vec<(u8, u8, u8)> = samples.iter().step_by(stride).copied().collect();
    if pool.is_empty() {
        return vec![(0, 0, 0), (255, 255, 255)];
    }
    pool.sort_unstable();
    pool.dedup();
    if pool.len() <= target {
        return pool;
    }
    struct Cube {
        px: Vec<(u8, u8, u8)>,
        range: u32,
    }
    let mut cubes = vec![Cube { range: 0, px: pool }];
    // Compute the channel range of a cube.
    let range_of = |px: &[(u8, u8, u8)]| -> (u32, usize) {
        let (mut mn, mut mx) = ([255u8; 3], [0u8; 3]);
        for p in px {
            let v = [p.0, p.1, p.2];
            for c in 0..3 {
                mn[c] = mn[c].min(v[c]);
                mx[c] = mx[c].max(v[c]);
            }
        }
        let mut best = (0u32, 0usize);
        for c in 0..3 {
            let r = mx[c] as u32 - mn[c] as u32;
            if r > best.0 {
                best = (r, c);
            }
        }
        best
    };
    cubes[0].range = range_of(&cubes[0].px).0;
    while cubes.len() < target {
        // Split the cube with the largest range (and >1 pixel).
        let mut bi: Option<usize> = None;
        for (i, c) in cubes.iter().enumerate() {
            if c.px.len() > 1 && (bi.is_none() || c.range > cubes[bi.unwrap()].range) {
                bi = Some(i);
            }
        }
        let Some(bi) = bi else { break };
        let mut cube = cubes.remove(bi);
        let (_, ch) = range_of(&cube.px);
        cube.px.sort_by_key(|p| [p.0, p.1, p.2][ch]);
        let mid = cube.px.len() / 2;
        let right = cube.px.split_off(mid);
        let left = cube.px;
        cubes.push(Cube {
            range: range_of(&left).0,
            px: left,
        });
        cubes.push(Cube {
            range: range_of(&right).0,
            px: right,
        });
    }
    cubes
        .iter()
        .map(|c| {
            let n = c.px.len() as u32;
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for p in &c.px {
                r += p.0 as u32;
                g += p.1 as u32;
                b += p.2 as u32;
            }
            ((r / n) as u8, (g / n) as u8, (b / n) as u8)
        })
        .collect()
}

/// Duotone conversion (Phase 6.5): the document must already
/// hold grayscale pixels. Layers are kept (era duotone keeps
/// one grayscale image; PictoDesk keeps its layers — the ink
/// mapping applies to the composite at render time).
pub fn to_duotone(doc: &mut Document, spec: crate::document::DuotoneSpec) {
    doc.duotone = Some(spec);
    doc.mode = ColorMode::Duotone;
}

/// Multichannel conversion (Phase 6.5): flattens, then splits
/// the image into independent ink planes — CMYK documents
/// give C/M/Y/K planes; anything else goes through CMYK
/// separation without the black plate dropped (RGB sources
/// produce C/M/Y plus a K derived from the separation, so
/// the round trip is lossless through `convert_mode` back
/// to CMYK). The flattened RGB stays in the single layer as
/// the working image for tools that do not know planes yet;
/// the render path composites the planes.
pub fn to_multichannel(doc: &mut Document) {
    if doc.mode != ColorMode::Cmyk {
        convert_mode(doc, ColorMode::Cmyk);
    }
    doc.flatten();
    let (w, h) = (doc.width as usize, doc.height as usize);
    let names = ["Cyan", "Magenta", "Yellow", "Black"];
    // Subtractive primaries: under the multiply composite a
    // cyan plane must absorb red completely, and so on — with
    // these, the multichannel composite of a CMYK separation
    // is exactly the CMYK composite.
    let colors = [
        (0u8, 255u8, 255u8),
        (255u8, 0u8, 255u8),
        (255u8, 255u8, 0u8),
        (0u8, 0u8, 0u8),
    ];
    let mut planes: Vec<Vec<u16>> = (0..4).map(|_| vec![0u16; w * h]).collect();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (c, m, y2, k, _a) = doc.get_cmyk(0, x, y);
            let i = (y as usize) * w + x as usize;
            planes[0][i] = c;
            planes[1][i] = m;
            planes[2][i] = y2;
            planes[3][i] = k;
        }
    }
    doc.multi_channels = names
        .iter()
        .zip(colors.iter())
        .map(|(n, c)| crate::document::MultiChannelInfo {
            name: n.to_string(),
            color: *c,
        })
        .collect();
    doc.multi_planes = planes;
    doc.mode = ColorMode::Multichannel;
}

/// Rebuild CMYK storage from the multichannel planes (the
/// Multichannel exit path used by convert_mode's hub: the
/// hub calls this before the generic CMYK-out step by
/// leaving mode = Cmyk storage in place — planes were built
/// FROM the flattened CMYK layer, which is still intact, so
/// exiting Multichannel only needs the flag cleared. This
/// function re-syncs the layer from edited planes first.)
pub fn multichannel_sync_to_layer(doc: &mut Document) {
    if doc.multi_planes.len() != 4 || doc.layers.is_empty() {
        return;
    }
    let (w, h) = (doc.width as i32, doc.height as i32);
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * doc.width as usize + x as usize;
            let v = (
                doc.multi_planes[0][i],
                doc.multi_planes[1][i],
                doc.multi_planes[2][i],
                doc.multi_planes[3][i],
                u16::MAX,
            );
            doc.set_cmyk(0, x, y, v);
        }
    }
}

/// Rewrite every layer's RGB pixels as real CMYK ink values (tile
/// slots) + alpha plane entries.
fn rgb_storage_to_cmyk(doc: &mut Document) {
    for li in 0..doc.layers.len() {
        let offset = doc.layers[li].offset;
        let coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
        for coord in coords {
            let mut alphas: Vec<(u32, u32, u16)> = Vec::new();
            {
                let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
                for ly in 0..256u32 {
                    for lx in 0..256u32 {
                        let p = tile.get(lx, ly);
                        if p == Pixel16::TRANSPARENT {
                            continue;
                        }
                        let (r, g, b, _) = p.to_f32s();
                        let (c, m, y, k) = rgb_to_cmyk(r, g, b);
                        tile.set(lx, ly, Pixel16::from_f32s(c, m, y, k));
                        alphas.push((lx, ly, p.a));
                    }
                }
            }
            for (lx, ly, a) in alphas {
                doc.layers[li].set_alpha(
                    coord.0 * 256 + lx as i32 + offset.0,
                    coord.1 * 256 + ly as i32 + offset.1,
                    a,
                );
            }
        }
    }
}

/// Rewrite CMYK storage back to RGB pixels; clears the alpha planes
/// (alpha folds back into the pixel's alpha channel).
fn cmyk_storage_to_rgb(doc: &mut Document) {
    for li in 0..doc.layers.len() {
        let mut coords: Vec<_> = doc.layers[li].tiles.keys().copied().collect();
        for c in doc.layers[li].cmyk_alpha.keys() {
            if !coords.contains(c) {
                coords.push(*c);
            }
        }
        for coord in coords {
            // Ensure the ink tile exists so writes have somewhere to go.
            doc.layers[li]
                .tiles
                .entry(coord)
                .or_insert_with(crate::tile::Tile::new_transparent);
            let alphas: Vec<u16> = match doc.layers[li].cmyk_alpha.get(&coord) {
                Some(t) => t.clone(),
                None => Vec::new(),
            };
            let tile = doc.layers[li].tiles.get_mut(&coord).unwrap();
            for ly in 0..256u32 {
                for lx in 0..256u32 {
                    let a = if alphas.is_empty() {
                        u16::MAX
                    } else {
                        alphas[(ly as usize) * 256 + lx as usize]
                    };
                    if a == 0 {
                        continue;
                    }
                    let p = tile.get(lx, ly);
                    let (r, g, b) = cmyk_to_rgb(
                        p.r as f32 / 65535.0,
                        p.g as f32 / 65535.0,
                        p.b as f32 / 65535.0,
                        p.a as f32 / 65535.0,
                    );
                    let out = Pixel16::from_f32s(r, g, b, a as f32 / 65535.0);
                    if out != p {
                        tile.set(lx, ly, out);
                    }
                }
            }
        }
        doc.layers[li].cmyk_alpha.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_conversion_stores_real_inks() {
        let mut doc = Document::new(4, 1);
        doc.layers[0].fill_rect(0, 0, 2, 1, Pixel16::from_rgba8(255, 0, 0, 255));
        doc.layers[0].fill_rect(2, 0, 2, 1, Pixel16::from_rgba8(120, 130, 140, 255));
        convert_mode(&mut doc, ColorMode::Cmyk);
        assert_eq!(doc.mode, ColorMode::Cmyk);
        // Pure red stores as C=0, M=100%, Y=100%, K=0, alpha opaque.
        let (c, m, y, k, a) = doc.get_cmyk(0, 0, 0);
        assert_eq!((c, k, a), (0, 0, u16::MAX));
        assert!(m > 65000 && y > 65000, "red inks: m={m} y={y}");
        // The screen render still shows red (ink composite -> RGB).
        assert_eq!(doc.render_pixel(0, 0).to_rgba8(), (255, 0, 0, 255));
        // CMYK -> RGB restores the original colors.
        convert_mode(&mut doc, ColorMode::Rgb);
        assert_eq!(doc.render_pixel(0, 0).to_rgba8(), (255, 0, 0, 255));
        let kept = doc.render_pixel(3, 0).to_rgba8();
        assert!((kept.0 as i32 - 120).abs() <= 2 && (kept.2 as i32 - 140).abs() <= 2);
        assert!(doc.layers[0].cmyk_alpha.is_empty());
    }

    #[test]
    fn cmyk_alpha_survives_conversion() {
        let mut doc = Document::new(2, 1);
        doc.layers[0].set_pixel(0, 0, Pixel16::from_rgba8(0, 0, 255, 128));
        convert_mode(&mut doc, ColorMode::Cmyk);
        let (_, _, _, _, a) = doc.get_cmyk(0, 0, 0);
        assert!((a as i32 - 128 * 257).abs() <= 1, "alpha {a}");
        convert_mode(&mut doc, ColorMode::Rgb);
        assert_eq!(doc.layers[0].get_pixel(0, 0).to_rgba8().3, 128);
    }

    #[test]
    fn grayscale_mode_desaturates() {
        let mut doc = Document::new(2, 1);
        doc.layers[0].fill_rect(0, 0, 2, 1, Pixel16::from_rgba8(200, 40, 40, 255));
        convert_mode(&mut doc, ColorMode::Grayscale);
        let p = doc.render_pixel(0, 0).to_rgba8();
        assert_eq!(p.0, p.1);
        assert_eq!(p.1, p.2);
    }
}
