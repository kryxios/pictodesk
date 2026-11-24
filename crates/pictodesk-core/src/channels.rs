//! Channel arithmetic and channel document operations
//! (Phase 6.5 Batch 3): the plane extraction behind Apply
//! Image and Calculations, Split / Merge Channels, and
//! merging a layer set into one layer.

use crate::blend::BlendMode;
use crate::document::{ColorMode, Document};
use crate::layer::Layer;
use crate::pixel::Pixel16;

/// Where a grayscale plane comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaneSource {
    /// Luma of the flattened composite.
    CompositeGray,
    /// One channel of the flattened composite: 0 R, 1 G, 2 B,
    /// 3 A. In CMYK documents these are the RGB equivalents;
    /// in Lab documents, L / a+128 / b+128 / A.
    CompositeChannel(u8),
    /// One channel of one layer's own content (before
    /// compositing): RGB(A) through the mode funnel; CMYK
    /// documents give ink amounts, Lab gives slot values.
    LayerChannel(usize, u8),
    /// A saved alpha channel.
    Alpha(usize),
    /// Luma of one layer's own content.
    LayerGray(usize),
    /// Alpha of the flattened composite.
    CompositeAlpha,
}

/// Extract a document-sized grayscale plane (0..=255 per
/// pixel, row-major).
pub fn plane(doc: &Document, src: PlaneSource) -> Vec<u8> {
    let (w, h) = (doc.width as i32, doc.height as i32);
    let mut out = vec![0u8; (w * h) as usize];
    match src {
        PlaneSource::Alpha(ai) => {
            if let Some(ch) = doc.alphas.get(ai) {
                let n = out.len().min(ch.data.len());
                out[..n].copy_from_slice(&ch.data[..n]);
            }
        }
        PlaneSource::CompositeGray => {
            for y in 0..h {
                for x in 0..w {
                    let p = doc.render_pixel(x, y).to_rgba8();
                    out[(y * w + x) as usize] =
                        ((p.0 as u32 * 299 + p.1 as u32 * 587 + p.2 as u32 * 114) / 1000) as u8;
                }
            }
        }
        PlaneSource::CompositeAlpha => {
            for y in 0..h {
                for x in 0..w {
                    out[(y * w + x) as usize] = doc.render_pixel(x, y).to_rgba8().3;
                }
            }
        }
        PlaneSource::CompositeChannel(c) => {
            for y in 0..h {
                for x in 0..w {
                    let v = if doc.mode == ColorMode::Cmyk && c < 4 {
                        // CMYK composite channels are ink
                        // amounts (data convention).
                        let ink = doc.render_cmyk_pixel(x, y);
                        let pick = [ink.0, ink.1, ink.2, ink.3][c as usize];
                        (pick as u32 * 255 / 65535) as u8
                    } else if doc.mode == ColorMode::Lab {
                        let (l, a, b, al) = doc.render_lab_pixel(x, y);
                        match c {
                            0 => (l * 2.55) as u8,
                            1 => (a + 128.0).clamp(0.0, 255.0) as u8,
                            2 => (b + 128.0).clamp(0.0, 255.0) as u8,
                            _ => (al * 255.0) as u8,
                        }
                    } else {
                        let p = doc.render_pixel(x, y).to_rgba8();
                        match c {
                            0 => p.0,
                            1 => p.1,
                            2 => p.2,
                            _ => p.3,
                        }
                    };
                    out[(y * w + x) as usize] = v;
                }
            }
        }
        PlaneSource::LayerGray(li) => {
            if li < doc.layers.len() {
                for y in 0..h {
                    for x in 0..w {
                        let (r, g, b, _) = doc.content_pixel(li, x, y);
                        let v = (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0);
                        out[(y * w + x) as usize] = (v * 255.0 + 0.5) as u8;
                    }
                }
            }
        }
        PlaneSource::LayerChannel(li, c) => {
            if li < doc.layers.len() {
                for y in 0..h {
                    for x in 0..w {
                        let v = if doc.mode == ColorMode::Cmyk {
                            let ink = doc.get_cmyk(li, x, y);
                            let pick = [ink.0, ink.1, ink.2, ink.3, ink.4][(c as usize).min(4)];
                            (pick as u32 * 255 / 65535) as u8
                        } else if doc.mode == ColorMode::Lab {
                            let s = doc.get_lab(li, x, y);
                            let pick = [s.0, s.1, s.2, s.3][(c as usize).min(3)];
                            (pick as u32 * 255 / 65535) as u8
                        } else {
                            let p = doc.layers[li].get_pixel(x, y).to_rgba8();
                            match c {
                                0 => p.0,
                                1 => p.1,
                                2 => p.2,
                                _ => p.3,
                            }
                        };
                        out[(y * w + x) as usize] = v;
                    }
                }
            }
        }
    }
    out
}

/// A Calculations operation: a blend mode, or the era's
/// two arithmetic modes (Add clips at white; Subtract
/// carries the +255 bias so equal values give white).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalcOp {
    Mode(BlendMode),
    Add,
    Subtract,
}

impl CalcOp {
    pub fn apply(&self, base: u8, src: u8, opacity: f32) -> u8 {
        let (a, b) = (base as f32 / 255.0, src as f32 / 255.0);
        let blended = match self {
            CalcOp::Mode(m) => m.blend_channel(a, b),
            CalcOp::Add => (a + b).min(1.0),
            CalcOp::Subtract => (a - b + 1.0).clamp(0.0, 1.0),
        };
        let out = a + (blended - a) * opacity.clamp(0.0, 1.0);
        (out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }
}

/// Blend plane `b` over plane `a` with a Calculations
/// operation and opacity.
pub fn blend_planes_op(a: &[u8], b: &[u8], op: CalcOp, opacity: f32) -> Vec<u8> {
    a.iter()
        .zip(b.iter())
        .map(|(&av, &bv)| op.apply(av, bv, opacity))
        .collect()
}

/// Blend plane `b` over plane `a` with a blend mode and
/// opacity (the Calculations core; also Apply Image's math
/// for single-channel targets).
pub fn blend_planes(a: &[u8], b: &[u8], mode: BlendMode, opacity: f32) -> Vec<u8> {
    a.iter()
        .zip(b.iter())
        .map(|(&av, &bv)| {
            let base = av as f32 / 255.0;
            let src = bv as f32 / 255.0;
            let blended = mode.blend_channel(base, src);
            let out = base + (blended - base) * opacity.clamp(0.0, 1.0);
            (out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
        })
        .collect()
}

/// Apply Image: blend a source plane into `layer`, either
/// across all color channels (RGB blend of the gray source)
/// or into one channel only. `mask` (with invert flag) gates
/// the effect per pixel. Alpha of the target is preserved.
pub fn apply_image(
    doc: &mut Document,
    layer: usize,
    src: &[u8],
    mode: BlendMode,
    opacity: f32,
    mask: Option<(&[u8], bool)>,
    target_channel: Option<u8>,
) {
    if layer >= doc.layers.len() {
        return;
    }
    let (w, h) = (doc.width as i32, doc.height as i32);
    let gate = |i: usize| -> f32 {
        match mask {
            None => 1.0,
            Some((m, invert)) => {
                let v = m.get(i).copied().unwrap_or(0) as f32 / 255.0;
                if invert {
                    1.0 - v
                } else {
                    v
                }
            }
        }
    };
    match doc.mode {
        ColorMode::Cmyk => {
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let g = gate(i) * opacity;
                    if g <= 0.0 {
                        continue;
                    }
                    let s = src.get(i).copied().unwrap_or(0) as f32 / 255.0;
                    let ink = doc.get_cmyk(layer, x, y);
                    let mut vals = [
                        ink.0 as f32 / 65535.0,
                        ink.1 as f32 / 65535.0,
                        ink.2 as f32 / 65535.0,
                        ink.3 as f32 / 65535.0,
                    ];
                    match target_channel {
                        Some(c) if (c as usize) < 4 => {
                            let c = c as usize;
                            let blended = mode.blend_channel(vals[c], s);
                            vals[c] += (blended - vals[c]) * g;
                        }
                        _ => {
                            for v in vals.iter_mut() {
                                let blended = mode.blend_channel(*v, s);
                                *v += (blended - *v) * g;
                            }
                        }
                    }
                    let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                    doc.set_cmyk(
                        layer,
                        x,
                        y,
                        (e(vals[0]), e(vals[1]), e(vals[2]), e(vals[3]), ink.4),
                    );
                }
            }
        }
        ColorMode::Lab => {
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let g = gate(i) * opacity;
                    if g <= 0.0 {
                        continue;
                    }
                    let s = src.get(i).copied().unwrap_or(0) as f32 / 255.0;
                    let slots = doc.get_lab(layer, x, y);
                    let mut vals = [
                        slots.0 as f32 / 65535.0,
                        slots.1 as f32 / 65535.0,
                        slots.2 as f32 / 65535.0,
                    ];
                    match target_channel {
                        Some(c) if (c as usize) < 3 => {
                            let c = c as usize;
                            let blended = mode.blend_channel(vals[c], s);
                            vals[c] += (blended - vals[c]) * g;
                        }
                        _ => {
                            for v in vals.iter_mut() {
                                let blended = mode.blend_channel(*v, s);
                                *v += (blended - *v) * g;
                            }
                        }
                    }
                    let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                    doc.set_lab(layer, x, y, (e(vals[0]), e(vals[1]), e(vals[2]), slots.3));
                }
            }
        }
        _ => {
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let g = gate(i) * opacity;
                    if g <= 0.0 {
                        continue;
                    }
                    let s = src.get(i).copied().unwrap_or(0) as f32 / 255.0;
                    let base = doc.layers[layer].get_pixel(x, y);
                    let (br, bg, bb, ba) = base.to_f32s();
                    let out = match target_channel {
                        Some(c) => {
                            let mut v = [br, bg, bb];
                            if (c as usize) < 3 {
                                let ch = c as usize;
                                let blended = mode.blend_channel(v[ch], s);
                                v[ch] += (blended - v[ch]) * g;
                            }
                            Pixel16::from_f32s(v[0], v[1], v[2], ba)
                        }
                        None => {
                            // blend_rgb covers the component
                            // modes; per-channel modes go
                            // through blend_channel per component.
                            let blended =
                                match crate::blend::blend_rgb(mode, (br, bg, bb), (s, s, s)) {
                                    Some(v) => v,
                                    None => (
                                        mode.blend_channel(br, s),
                                        mode.blend_channel(bg, s),
                                        mode.blend_channel(bb, s),
                                    ),
                                };
                            Pixel16::from_f32s(
                                br + (blended.0 - br) * g,
                                bg + (blended.1 - bg) * g,
                                bb + (blended.2 - bb) * g,
                                ba,
                            )
                        }
                    };
                    doc.layers[layer].set_pixel(x, y, out);
                }
            }
        }
    }
}

/// Split Channels: one grayscale document per channel of the
/// composite (plus one per alpha channel), in DISPLAY values
/// (CMYK inks appear as 255 - ink, Lab a/b as value + 128) —
/// the same convention the Channels panel shows, and the one
/// Merge Channels consumes.
pub fn split_channels(doc: &Document) -> Vec<(String, Document)> {
    let (w, h) = (doc.width, doc.height);
    let names: Vec<String> = match doc.mode {
        ColorMode::Rgb | ColorMode::Indexed => {
            vec!["Red".into(), "Green".into(), "Blue".into()]
        }
        ColorMode::Cmyk => vec![
            "Cyan".into(),
            "Magenta".into(),
            "Yellow".into(),
            "Black".into(),
        ],
        ColorMode::Lab => vec!["Lightness".into(), "a".into(), "b".into()],
        _ => vec!["Gray".into()],
    };
    let mut out: Vec<(String, Document)> = Vec::new();
    for (ci, name) in names.iter().enumerate() {
        let plane_data: Vec<u8> = if doc.mode == ColorMode::Cmyk {
            (0..(w * h) as usize)
                .map(|i| {
                    let (x, y) = ((i as u32 % w) as i32, (i as u32 / w) as i32);
                    let ink = doc.render_cmyk_pixel(x, y);
                    let pick = [ink.0, ink.1, ink.2, ink.3][ci];
                    255 - ((pick as u32 * 255 + 32767) / 65535) as u8
                })
                .collect()
        } else {
            plane(doc, PlaneSource::CompositeChannel(ci as u8))
        };
        out.push((name.clone(), gray_doc(w, h, &plane_data, name)));
    }
    for ch in &doc.alphas {
        out.push((ch.name.clone(), gray_doc(w, h, &ch.data, &ch.name)));
    }
    out
}

fn gray_doc(w: u32, h: u32, plane_data: &[u8], name: &str) -> Document {
    let mut d = Document::new(w, h);
    d.mode = ColorMode::Grayscale;
    d.layers[0].name = name.to_string();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let v = plane_data[(y as u32 * w + x as u32) as usize];
            d.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
        }
    }
    d
}

/// Merge Channels: build a document in `mode` from display-
/// value planes (the Split Channels convention). Plane count
/// must match the mode (Gray 1, RGB/Lab 3, CMYK 4); missing
/// planes read as zero (full ink for CMYK).
pub fn merge_channels(mode: ColorMode, planes: &[&[u8]], w: u32, h: u32) -> Document {
    let mut d = Document::new(w, h);
    let at =
        |pi: usize, i: usize| -> u8 { planes.get(pi).and_then(|p| p.get(i)).copied().unwrap_or(0) };
    match mode {
        ColorMode::Grayscale => {
            d.mode = ColorMode::Grayscale;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let v = at(0, (y as u32 * w + x as u32) as usize);
                    d.layers[0].set_pixel(x, y, Pixel16::from_rgba8(v, v, v, 255));
                }
            }
        }
        ColorMode::Cmyk => {
            d.mode = ColorMode::Cmyk;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let i = (y as u32 * w + x as u32) as usize;
                    let ink = |pi: usize| ((255 - at(pi, i)) as u32 * 65535 / 255) as u16;
                    d.set_cmyk(0, x, y, (ink(0), ink(1), ink(2), ink(3), u16::MAX));
                }
            }
        }
        ColorMode::Lab => {
            d.mode = ColorMode::Lab;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let i = (y as u32 * w + x as u32) as usize;
                    let (l, a, b) = crate::color::lab_to_slots(
                        at(0, i) as f32 / 2.55,
                        at(1, i) as f32 - 128.0,
                        at(2, i) as f32 - 128.0,
                    );
                    let e = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                    d.set_lab(0, x, y, (e(l), e(a), e(b), u16::MAX));
                }
            }
        }
        _ => {
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let i = (y as u32 * w + x as u32) as usize;
                    d.layers[0].set_pixel(
                        x,
                        y,
                        Pixel16::from_rgba8(at(0, i), at(1, i), at(2, i), 255),
                    );
                }
            }
        }
    }
    d
}

/// Merge a layer set into a single layer (Layer > Merge
/// Group): the set's visible layers are flattened in a
/// scratch document (so set blend mode / opacity apply), the
/// result takes the set's place and name, and higher set ids
/// renumber down.
pub fn merge_set(doc: &mut Document, gid: u16) {
    let members: Vec<usize> = (0..doc.layers.len())
        .filter(|&i| doc.layers[i].group == gid)
        .collect();
    if members.is_empty() {
        return;
    }
    let mut scratch = Document::new(doc.width, doc.height);
    scratch.mode = doc.mode;
    scratch.working_space = doc.working_space;
    scratch.layers.clear();
    for &i in &members {
        scratch.layers.push(doc.layers[i].clone());
    }
    // The set's own blend mode / opacity ride along as a
    // wrapper is overkill here: member layers already carry
    // the set factor through render (set_f), but a scratch doc
    // has no sets — apply the set opacity to each member's
    // opacity approximately by scaling (exact for Normal sets).
    if let Some(set) = doc.sets.get((gid - 1) as usize) {
        if set.opacity < 1.0 {
            for l in scratch.layers.iter_mut() {
                l.opacity *= set.opacity;
            }
        }
    }
    scratch.flatten();
    let mut merged: Layer = scratch.layers.into_iter().next().unwrap();
    let set_name = doc
        .sets
        .get((gid - 1) as usize)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Merged".into());
    merged.name = set_name;
    merged.group = 0;
    let insert_at = members[0];
    // Remove members (highest index first), then insert.
    for &i in members.iter().rev() {
        doc.layers.remove(i);
    }
    let at = insert_at.min(doc.layers.len());
    doc.layers.insert(at, merged);
    doc.active_layer = at;
    // Drop the set and renumber higher group ids.
    if (gid as usize) >= 1 && (gid as usize) <= doc.sets.len() {
        doc.sets.remove((gid - 1) as usize);
    }
    for l in doc.layers.iter_mut() {
        if l.group > gid {
            l.group -= 1;
        }
    }
}
