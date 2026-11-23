//! PSD / PSB import: 1/8/16-bit, Bitmap / Grayscale / Indexed /
//! RGB / CMYK / Multichannel, raw + PackBits + ZIP (with and
//! without prediction) channel data, layer records with names,
//! bounds, opacity, visibility, blend keys, and layer masks
//! (baked into transparency and also preserved as alpha
//! channels). Anything that cannot come across — type layers,
//! adjustment layers, groups as structure — is listed in
//! `PsdReport::notes`, never silently dropped.
//!
//! Format notes (Adobe's published spec): layer records are
//! stored TOP layer first; channel data follows all records;
//! compression 0 = raw, 1 = PackBits RLE, 2 = ZIP, 3 = ZIP with
//! row-wise delta prediction. PSB (version 2) widens the section
//! and channel lengths to 64-bit and RLE row counts to 32-bit.

use std::io::Read;

use crate::blend::BlendMode;
use crate::document::AlphaChannel;
use crate::document::{ColorMode, Document};
use crate::layer::Layer;
use crate::pixel::Pixel16;

pub struct PsdReport {
    pub layers_imported: usize,
    pub notes: Vec<String>,
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    psb: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.pos + n > self.buf.len() {
            return Err("PSD file truncated".into());
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
    fn remaining(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    /// A length field: 32-bit in PSD, 64-bit in PSB.
    fn len64(&mut self) -> Result<usize, String> {
        if self.psb {
            let b = self.take(8)?;
            Ok(u64::from_be_bytes(b.try_into().unwrap()) as usize)
        } else {
            Ok(self.u32()? as usize)
        }
    }
    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.take(n)?;
        Ok(())
    }
}

fn blend_from_key(key: &[u8]) -> Option<BlendMode> {
    Some(match key {
        b"norm" => BlendMode::Normal,
        b"mul " => BlendMode::Multiply,
        b"scrn" => BlendMode::Screen,
        b"over" => BlendMode::Overlay,
        b"sLit" => BlendMode::SoftLight,
        b"dark" => BlendMode::Darken,
        b"lite" => BlendMode::Lighten,
        b"diff" => BlendMode::Difference,
        b"hue " => BlendMode::Hue,
        b"sat " => BlendMode::Saturation,
        b"colr" => BlendMode::Color,
        b"lum " => BlendMode::Luminosity,
        _ => return None,
    })
}

struct ChannelInfo {
    id: i16,
    len: usize,
}

struct LayerRec {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    channels: Vec<ChannelInfo>,
    blend: BlendMode,
    blend_key: String,
    opacity: u8,
    visible: bool,
    name: String,
    is_group_marker: bool,
    has_raster: bool,
}

/// PackBits-decode one row of `expected` bytes.
fn packbits_row(data: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0usize;
    while i < data.len() && out.len() < expected {
        let n = data[i] as i8;
        i += 1;
        if n >= 0 {
            let count = n as usize + 1;
            let end = (i + count).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else if n != -128 {
            let count = (1 - n as i32) as usize;
            if i < data.len() {
                let v = data[i];
                i += 1;
                out.extend(std::iter::repeat_n(v, count));
            }
        }
    }
    out.truncate(expected);
    out
}

/// Read one channel plane as 16-bit samples (w*h of them).
/// `byte_len` is the channel's declared total size including the
/// compression word, when known (layer records); the merged-image
/// path passes None and ZIP streams self-terminate.
#[allow(clippy::too_many_arguments)]
fn read_channel(
    r: &mut Reader,
    w: usize,
    h: usize,
    depth: u16,
    byte_len: Option<usize>,
    report: &mut PsdReport,
    layer_name: &str,
) -> Result<Vec<u16>, String> {
    let compression = r.u16()?;
    let npix = w * h;
    if npix == 0 {
        if let Some(total) = byte_len {
            let used = 2usize;
            if total > used {
                r.skip(total - used)?;
            }
        }
        return Ok(Vec::new());
    }
    // Raw byte size of the plane before sample conversion.
    let raw_len = match depth {
        1 => (w.div_ceil(8)) * h,
        16 => npix * 2,
        _ => npix,
    };
    let bytes: Vec<u8> = match compression {
        0 => r.take(raw_len)?.to_vec(),
        1 => {
            let mut counts = Vec::with_capacity(h);
            for _ in 0..h {
                counts.push(if r.psb {
                    r.u32()? as usize
                } else {
                    r.u16()? as usize
                });
            }
            let row_bytes = raw_len / h.max(1);
            let mut out = Vec::with_capacity(raw_len);
            for count in &counts {
                let row = r.take(*count)?;
                out.extend_from_slice(&packbits_row(row, row_bytes));
            }
            out
        }
        2 | 3 => {
            let input: &[u8] = match byte_len {
                Some(total) => r.take(total.saturating_sub(2))?,
                None => r.remaining(),
            };
            let mut dec = flate2::read::ZlibDecoder::new(input);
            let mut out = Vec::with_capacity(raw_len);
            dec.read_to_end(&mut out)
                .map_err(|e| format!("PSD ZIP channel in '{layer_name}': {e}"))?;
            if byte_len.is_none() {
                let consumed = dec.total_in() as usize;
                r.skip(consumed)?;
            }
            if compression == 3 {
                apply_prediction(&mut out, w, h, depth);
            }
            out
        }
        other => {
            report.notes.push(format!(
                "Layer '{layer_name}': unknown channel compression {other} — channel read as blank"
            ));
            if let Some(total) = byte_len {
                r.skip(total.saturating_sub(2))?;
            }
            vec![0; raw_len]
        }
    };
    // Convert bytes to 16-bit samples.
    Ok(match depth {
        1 => {
            let row_bytes = w.div_ceil(8);
            let mut out = Vec::with_capacity(npix);
            for y in 0..h {
                for x in 0..w {
                    let byte = bytes.get(y * row_bytes + x / 8).copied().unwrap_or(0);
                    let bit = (byte >> (7 - x % 8)) & 1;
                    // Bitmap mode: 1 = black, 0 = white.
                    out.push(if bit == 1 { 0 } else { 65535 });
                }
            }
            out
        }
        16 => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect(),
        _ => bytes.iter().map(|&b| b as u16 * 257).collect(),
    })
}

/// Undo ZIP prediction: row-wise delta on bytes (8-bit) or on
/// 16-bit samples (16-bit documents).
fn apply_prediction(out: &mut [u8], w: usize, h: usize, depth: u16) {
    if depth == 16 {
        let row = w * 2;
        for y in 0..h {
            let start = y * row;
            if start + row > out.len() {
                break;
            }
            for i in 1..w {
                let idx = start + i * 2;
                let prev = u16::from_be_bytes([out[idx - 2], out[idx - 1]]);
                let cur = u16::from_be_bytes([out[idx], out[idx + 1]]);
                let v = cur.wrapping_add(prev);
                out[idx..idx + 2].copy_from_slice(&v.to_be_bytes());
            }
        }
    } else {
        for y in 0..h {
            let start = y * w;
            if start + w > out.len() {
                break;
            }
            for i in 1..w {
                out[start + i] = out[start + i].wrapping_add(out[start + i - 1]);
            }
        }
    }
}

pub fn load_psd(bytes: &[u8]) -> Result<(Document, PsdReport), String> {
    let mut report = PsdReport {
        layers_imported: 0,
        notes: Vec::new(),
    };
    let mut r = Reader {
        buf: bytes,
        pos: 0,
        psb: false,
    };
    if r.take(4)? != b"8BPS" {
        return Err("not a PSD file (bad signature)".into());
    }
    let version = r.u16()?;
    if version == 2 {
        r.psb = true;
    } else if version != 1 {
        return Err(format!("unsupported PSD version {version}"));
    }
    r.skip(6)?; // reserved
    let file_channels = r.u16()? as usize;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    if !matches!(depth, 1 | 8 | 16) {
        return Err(format!("unsupported PSD depth {depth}"));
    }
    let mode_id = r.u16()?;
    // Color mode data: the palette for Indexed mode.
    let cm_len = r.u32()? as usize;
    let color_data = r.take(cm_len)?.to_vec();
    let palette: Option<Vec<u8>> = if mode_id == 2 && color_data.len() >= 768 {
        Some(color_data[..768].to_vec())
    } else {
        None
    };
    let mode = match mode_id {
        3 | 2 => ColorMode::Rgb,
        4 => ColorMode::Cmyk,
        7 => {
            if file_channels >= 4 {
                report
                    .notes
                    .push("Multichannel PSD read as CMYK".into());
                ColorMode::Cmyk
            } else {
                ColorMode::Grayscale
            }
        }
        1 | 0 => ColorMode::Grayscale,
        other => return Err(format!("PSD color mode {other} is not supported")),
    };
    if mode_id == 2 && palette.is_none() {
        return Err("Indexed PSD without a color palette".into());
    }
    // Image resources: skipped wholesale.
    let n = r.u32()? as usize;
    r.skip(n)?;

    // Layer and mask information section.
    let section_len = r.len64()?;
    let mut records: Vec<LayerRec> = Vec::new();
    let mut planes: Vec<Vec<Vec<u16>>> = Vec::new();
    if section_len > 0 {
        let layer_info_len = r.len64()?;
        if layer_info_len > 0 {
            let count = r.i32()?.unsigned_abs() as usize;
            for _ in 0..count {
                let top = r.i32()?;
                let left = r.i32()?;
                let bottom = r.i32()?;
                let right = r.i32()?;
                let nch = r.u16()? as usize;
                let mut channels = Vec::with_capacity(nch);
                for _ in 0..nch {
                    let id = r.i16()?;
                    let len = r.len64()?;
                    channels.push(ChannelInfo { id, len });
                }
                let sig = r.take(4)?.to_vec();
                if &sig != b"8BIM" && &sig != b"8B64" {
                    return Err("PSD layer record: bad blend signature".into());
                }
                let key = r.take(4)?.to_vec();
                let blend_key = String::from_utf8_lossy(&key).to_string();
                let blend = blend_from_key(&key).unwrap_or(BlendMode::Normal);
                let opacity = r.u8()?;
                let _clipping = r.u8()?;
                let flags = r.u8()?;
                let _filler = r.u8()?;
                let extra_len = r.u32()? as usize;
                let extra_start = r.pos;
                let m = r.u32()? as usize;
                r.skip(m)?; // layer mask data (geometry only)
                let m = r.u32()? as usize;
                r.skip(m)?; // blending ranges
                let name_len = r.u8()? as usize;
                let raw_name = r.take(name_len)?.to_vec();
                let name = String::from_utf8_lossy(&raw_name).to_string();
                let name_field = 1 + name_len;
                if !name_field.is_multiple_of(4) {
                    r.skip(4 - name_field % 4)?;
                }
                let mut is_group_marker = false;
                let mut has_raster = true;
                let extra_end = extra_start + extra_len;
                while r.pos + 12 <= extra_end {
                    let sig = r.take(4)?.to_vec();
                    if &sig != b"8BIM" && &sig != b"8B64" {
                        break;
                    }
                    let k = r.take(4)?.to_vec();
                    let dlen = r.u32()? as usize;
                    match &k[..] {
                        b"lsct" => {
                            let t = if dlen >= 4 { r.u32()? } else { 0 };
                            if dlen > 4 {
                                r.skip(dlen - 4)?;
                            }
                            if (1..=3).contains(&t) {
                                is_group_marker = true;
                            }
                        }
                        b"tySh" | b"lfx2" | b"lrFX" => {
                            has_raster = false;
                            r.skip(dlen)?;
                        }
                        _ => {
                            r.skip(dlen)?;
                        }
                    }
                    if dlen % 2 == 1 {
                        r.skip(1)?;
                    }
                }
                if r.pos < extra_end {
                    r.skip(extra_end - r.pos)?;
                }
                records.push(LayerRec {
                    top,
                    left,
                    bottom,
                    right,
                    channels,
                    blend,
                    blend_key,
                    opacity,
                    visible: flags & 0x02 == 0,
                    name,
                    is_group_marker,
                    has_raster,
                });
            }
            // Channel image data for every record, in record order.
            for rec in &records {
                let w = (rec.right - rec.left).max(0) as usize;
                let h = (rec.bottom - rec.top).max(0) as usize;
                let mut layer_planes = Vec::with_capacity(rec.channels.len());
                for ch in &rec.channels {
                    let start = r.pos;
                    let plane = read_channel(
                        &mut r,
                        w,
                        h,
                        depth,
                        Some(ch.len),
                        &mut report,
                        &rec.name,
                    )?;
                    if r.pos != start + ch.len && ch.len >= 2 {
                        r.pos = start + ch.len;
                    }
                    layer_planes.push(plane);
                }
                planes.push(layer_planes);
            }
        }
    }

    let mut doc = Document::new(width, height);
    doc.mode = mode;
    doc.layers.clear();

    if records.is_empty() {
        // No layer data: import the merged composite image.
        let mut by_id: Vec<(i16, Vec<u16>)> = Vec::new();
        for c in 0..file_channels {
            let plane = read_channel(
                &mut r,
                width as usize,
                height as usize,
                depth,
                None,
                &mut report,
                "merged image",
            )?;
            by_id.push((c as i16, plane));
        }
        // Extra channels beyond the mode's own become alpha.
        let mode_channels = match mode {
            ColorMode::Rgb => 3,
            ColorMode::Cmyk => 4,
            ColorMode::Grayscale => 1,
        };
        if file_channels > mode_channels {
            if let Some((_, alpha)) = by_id.iter().find(|(id, _)| *id as usize == mode_channels) {
                let alpha = alpha.clone();
                by_id.retain(|(id, _)| (*id as usize) < mode_channels);
                by_id.push((-1, alpha));
            }
        }
        let mut layer = Layer::new("Background");
        compose_layer(&mut layer, &by_id, 0, 0, width, height, mode, palette.as_deref());
        doc.layers.push(layer);
        report.layers_imported = 1;
        report
            .notes
            .push("No layer data in file — imported the merged image as one layer".into());
        doc.active_layer = 0;
        return Ok((doc, report));
    }

    // Records are stored top-first; our stack is bottom-first.
    for (ri, rec) in records.iter().enumerate().rev() {
        if rec.is_group_marker {
            report.notes.push(format!(
                "Group '{}' was flattened into the layer list (groups arrive in a later phase)",
                rec.name
            ));
            continue;
        }
        if !rec.has_raster {
            report.notes.push(format!(
                "Layer '{}' is not a plain raster layer (type/effects) — skipped",
                rec.name
            ));
            continue;
        }
        if blend_from_key(rec.blend_key.as_bytes()).is_none() {
            report.notes.push(format!(
                "Layer '{}': blend mode '{}' not in the classic set — imported as Normal",
                rec.name, rec.blend_key
            ));
        }
        let w = (rec.right - rec.left).max(0);
        let h = (rec.bottom - rec.top).max(0);
        let mut layer = Layer::new(if rec.name.is_empty() {
            format!("Layer {}", ri + 1)
        } else {
            rec.name.clone()
        });
        layer.opacity = rec.opacity as f32 / 255.0;
        layer.visible = rec.visible;
        layer.blend_mode = rec.blend;
        layer.offset = (rec.left, rec.top);
        let by_id: Vec<(i16, Vec<u16>)> = rec
            .channels
            .iter()
            .enumerate()
            .map(|(ci, ch)| (ch.id, planes[ri][ci].clone()))
            .collect();
        if w > 0 && h > 0 {
            compose_layer(
                &mut layer,
                &by_id,
                rec.left,
                rec.top,
                w as u32,
                h as u32,
                mode,
                palette.as_deref(),
            );
            // Layer mask (channel -2): bake into transparency and
            // preserve the mask itself as a named alpha channel.
            if let Some(mask) = by_id.iter().find(|(id, _)| *id == -2).map(|(_, p)| p) {
                apply_layer_mask(&mut layer, mask, rec.left, rec.top, w as u32, h as u32, mode);
                let mut ch = AlphaChannel::new(
                    format!("{} Mask", layer.name),
                    width,
                    height,
                );
                for yy in 0..h {
                    for xx in 0..w {
                        let i = (yy * w + xx) as usize;
                        let v = (mask.get(i).copied().unwrap_or(65535) / 257) as u8;
                        let (dx, dy) = (rec.left + xx, rec.top + yy);
                        if dx >= 0 && dy >= 0 && dx < width as i32 && dy < height as i32 {
                            let di = (dy as u32 * width + dx as u32) as usize;
                            if di < ch.data.len() {
                                ch.data[di] = v;
                            }
                        }
                    }
                }
                doc.alphas.push(ch);
                report.notes.push(format!(
                    "Layer '{}': layer mask baked into transparency and kept as an alpha channel",
                    rec.name
                ));
            }
        }
        doc.layers.push(layer);
        report.layers_imported += 1;
    }
    if doc.layers.is_empty() {
        doc.layers.push(Layer::new("Background"));
    }
    doc.active_layer = doc.layers.len() - 1;
    Ok((doc, report))
}

/// Multiply a layer mask into the layer's alpha (CMYK: the alpha
/// plane; other modes: the pixel alpha).
fn apply_layer_mask(
    layer: &mut Layer,
    mask: &[u16],
    left: i32,
    top: i32,
    w: u32,
    h: u32,
    mode: ColorMode,
) {
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            let i = (yy as u32 * w + xx as u32) as usize;
            let m = mask.get(i).copied().unwrap_or(65535) as u32;
            if m == 65535 {
                continue;
            }
            let (dx, dy) = (left + xx, top + yy);
            match mode {
                ColorMode::Cmyk => {
                    let a = layer.get_alpha(dx, dy) as u32;
                    layer.set_alpha(dx, dy, (a * m / 65535) as u16);
                }
                _ => {
                    let p = layer.get_pixel(dx, dy);
                    let a = (p.a as u32 * m / 65535) as u16;
                    layer.set_pixel(dx, dy, Pixel16::new(p.r, p.g, p.b, a));
                }
            }
        }
    }
}

/// Compose a layer's channel planes (16-bit samples) into tiles.
/// `by_id` maps PSD channel ids to planes sized (w * h).
#[allow(clippy::too_many_arguments)]
fn compose_layer(
    layer: &mut Layer,
    by_id: &[(i16, Vec<u16>)],
    left: i32,
    top: i32,
    w: u32,
    h: u32,
    mode: ColorMode,
    palette: Option<&[u8]>,
) {
    let get = |id: i16| by_id.iter().find(|(i, _)| *i == id).map(|(_, p)| p);
    let at = |plane: Option<&Vec<u16>>, i: usize| -> u16 {
        plane.map(|p| p.get(i).copied().unwrap_or(0)).unwrap_or(0)
    };
    let alpha = get(-1);
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            let i = (yy as u32 * w + xx as u32) as usize;
            let a = alpha.map(|_| at(alpha, i)).unwrap_or(65535);
            match mode {
                ColorMode::Rgb => {
                    if a == 0 {
                        continue;
                    }
                    let (mut r, mut g, mut b) = (at(get(0), i), at(get(1), i), at(get(2), i));
                    if let Some(pal) = palette {
                        // Indexed: channel 0 holds palette indices;
                        // the palette is planar (256 R, 256 G, 256 B).
                        let bi = ((at(get(0), i) / 257) as usize).min(255);
                        r = pal[bi] as u16 * 257;
                        g = pal[256 + bi] as u16 * 257;
                        b = pal[512 + bi] as u16 * 257;
                    }
                    layer.set_pixel(left + xx, top + yy, Pixel16::new(r, g, b, a));
                }
                ColorMode::Grayscale => {
                    if a == 0 {
                        continue;
                    }
                    let v = at(get(0), i);
                    layer.set_pixel(left + xx, top + yy, Pixel16::new(v, v, v, a));
                }
                ColorMode::Cmyk => {
                    // PSD stores CMYK inverted (65535 = no ink).
                    let inv = |id: i16| 65535 - at(get(id), i);
                    layer.set_pixel(
                        left + xx,
                        top + yy,
                        Pixel16::new(inv(0), inv(1), inv(2), inv(3)),
                    );
                    layer.set_alpha(left + xx, top + yy, a);
                }
            }
        }
    }
}
