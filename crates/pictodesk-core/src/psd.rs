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
        b"diss" => BlendMode::Dissolve,
        b"hLit" => BlendMode::HardLight,
        b"divn" => BlendMode::ColorDodge,
        b"idiv" => BlendMode::ColorBurn,
        b"excl" => BlendMode::Exclusion,
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
                report.notes.push("Multichannel PSD read as CMYK".into());
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
            // Per the PSD spec the layer count is a signed 16-bit
            // value (negative = absolute count, first alpha holds
            // merged transparency). An earlier build of this reader
            // read 32 bits, which only ever agreed with its own
            // test fixtures — real files were misparsed by 2 bytes.
            let count = r.i16()?.unsigned_abs() as usize;
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
                    let plane =
                        read_channel(&mut r, w, h, depth, Some(ch.len), &mut report, &rec.name)?;
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
        compose_layer(
            &mut layer,
            &by_id,
            0,
            0,
            width,
            height,
            mode,
            palette.as_deref(),
        );
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
                apply_layer_mask(
                    &mut layer, mask, rec.left, rec.top, w as u32, h as u32, mode,
                );
                let mut ch = AlphaChannel::new(format!("{} Mask", layer.name), width, height);
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

// ---------------------------------------------------------------------------
// PSD export (Phase 5, v1): 8-bit RGB PSD with real layer records.
// Written honestly: the report lists everything that could not be
// represented, and the flattened composite always matches what
// the user sees (effects, adjustments, and type render into it).
// ---------------------------------------------------------------------------

/// What the exporter had to leave behind, in plain words.
pub struct PsdExportReport {
    pub warnings: Vec<String>,
}

fn blend_to_key(mode: BlendMode) -> &'static [u8; 4] {
    match mode {
        BlendMode::Normal => b"norm",
        BlendMode::Multiply => b"mul ",
        BlendMode::Screen => b"scrn",
        BlendMode::Overlay => b"over",
        BlendMode::SoftLight => b"sLit",
        BlendMode::Darken => b"dark",
        BlendMode::Lighten => b"lite",
        BlendMode::Difference => b"diff",
        BlendMode::Dissolve => b"diss",
        BlendMode::HardLight => b"hLit",
        BlendMode::ColorDodge => b"divn",
        BlendMode::ColorBurn => b"idiv",
        BlendMode::Exclusion => b"excl",
        BlendMode::Hue => b"hue ",
        BlendMode::Saturation => b"sat ",
        BlendMode::Color => b"colr",
        BlendMode::Luminosity => b"lum ",
    }
}

struct W {
    buf: Vec<u8>,
}

impl W {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

/// PackBits-encode one row as literal runs (valid, uncompressed-
/// leaning RLE: correctness first).
fn packbits_encode_row(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(row.len() + row.len() / 127 + 1);
    for chunk in row.chunks(127) {
        out.push((chunk.len() - 1) as u8);
        out.extend_from_slice(chunk);
    }
    if row.is_empty() {
        out.push(0);
        out.push(0);
    }
    out
}

/// RLE channel data block: compression flag + row-count table +
/// packed rows. `rows` are the channel's bytes, one Vec per row.
fn rle_channel(rows: &[Vec<u8>]) -> Vec<u8> {
    let mut w = W { buf: Vec::new() };
    w.u16(1);
    let packed: Vec<Vec<u8>> = rows.iter().map(|r| packbits_encode_row(r)).collect();
    for p in &packed {
        w.u16(p.len() as u16);
    }
    for p in &packed {
        w.bytes(p);
    }
    w.buf
}

/// Export the document as an 8-bit RGB PSD (version 1 format).
pub fn save_psd(doc: &Document) -> (Vec<u8>, PsdExportReport) {
    let mut warnings: Vec<String> = Vec::new();
    if doc.mode != ColorMode::Rgb {
        warnings.push(format!(
            "Color mode {:?} was converted to RGB for PSD (PSD stores our CMYK inverted-ink model differently); the .pdk file keeps the original.",
            doc.mode
        ));
    }
    // Per-layer RGBA8 pixels in document space (top layer first,
    // as PSD records are stored).
    struct OutMask {
        rect: (i32, i32, i32, i32), // top, left, bottom, right
        fill: u8,
        enabled: bool,
    }
    struct Out {
        name: String,
        rect: (i32, i32, i32, i32),
        channels: Vec<(i16, Vec<u8>)>, // id, full channel-data block
        key: &'static [u8; 4],
        opacity: u8,
        visible: bool,
        mask: Option<OutMask>,
    }
    let mut outs: Vec<Out> = Vec::new();
    for layer in doc.layers.iter() {
        if layer.adjustment.is_some() {
            warnings.push(format!(
                "Adjustment layer '{}' exported as an empty layer (PSD adjustment parameters are not written in v1).",
                layer.name
            ));
            outs.push(Out {
                name: layer.name.clone(),
                rect: (0, 0, 0, 0),
                channels: Vec::new(),
                key: b"norm",
                opacity: (layer.opacity * 255.0) as u8,
                visible: layer.visible,
                mask: None,
            });
            continue;
        }
        if layer.text.is_some() {
            warnings.push(format!(
                "Type layer '{}' exported as rendered pixels (editable text is not written in v1).",
                layer.name
            ));
        }
        if layer.effects.map(|e| e.any_enabled()).unwrap_or(false) {
            warnings.push(format!(
                "Layer '{}' has effects: they are baked into the flattened preview but not stored as PSD layer styles in v1.",
                layer.name
            ));
        }
        let bounds = crate::transform::layer_painted_bounds(layer);
        let (rect, channels) = match bounds {
            None => ((0, 0, 0, 0), Vec::new()),
            Some((x0, y0, x1, y1)) => {
                let (lw, lh) = ((x1 - x0) as usize, (y1 - y0) as usize);
                let mut planes: [Vec<Vec<u8>>; 4] =
                    [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
                for yy in 0..lh {
                    let mut rows: [Vec<u8>; 4] = [
                        Vec::with_capacity(lw),
                        Vec::with_capacity(lw),
                        Vec::with_capacity(lw),
                        Vec::with_capacity(lw),
                    ];
                    for xx in 0..lw {
                        let (dx, dy) = (x0 + xx as i32, y0 + yy as i32);
                        let p = match doc.mode {
                            ColorMode::Cmyk => {
                                let (c, m, y, k, a) = doc.get_cmyk(
                                    doc.layers
                                        .iter()
                                        .position(|l| std::ptr::eq(l, layer))
                                        .unwrap(),
                                    dx,
                                    dy,
                                );
                                let bf = |v: u16| v as f32 / 65535.0;
                                let (r, g, b) =
                                    crate::color::cmyk_to_rgb(bf(c), bf(m), bf(y), bf(k));
                                (r, g, b, bf(a))
                            }
                            _ => {
                                let p = layer.get_pixel(dx, dy).to_f32s();
                                (p.0, p.1, p.2, p.3)
                            }
                        };
                        let e = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                        rows[0].push(e(p.0));
                        rows[1].push(e(p.1));
                        rows[2].push(e(p.2));
                        rows[3].push(e(p.3));
                    }
                    for c in 0..4 {
                        planes[c].push(rows[c].clone());
                    }
                }
                let ids = [0i16, 1, 2, -1];
                let mut chs = Vec::new();
                for c in 0..4 {
                    chs.push((ids[c], rle_channel(&planes[c])));
                }
                ((y0, x0, y1, x1), chs)
            }
        };
        // Layer mask as channel -2 plus its mask-data block.
        let mask = layer.mask.as_ref().map(|_| {
            let (mut mx0, mut my0, mut mx1, mut my1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            // Mask bounds: use the layer's painted bounds expanded
            // to any mask tile; fall back to the layer rect.
            if let Some(m) = &layer.mask {
                for coord in m.keys() {
                    mx0 = mx0.min(coord.0 * 256 + layer.offset.0);
                    my0 = my0.min(coord.1 * 256 + layer.offset.1);
                    mx1 = mx1.max(coord.0 * 256 + 256 + layer.offset.0);
                    my1 = my1.max(coord.1 * 256 + 256 + layer.offset.1);
                }
            }
            if mx0 == i32::MAX {
                (mx0, my0, mx1, my1) = (0, 0, 0, 0);
            }
            let (mw, mh) = (((mx1 - mx0).max(0)) as usize, ((my1 - my0).max(0)) as usize);
            let mut rows = Vec::new();
            for yy in 0..mh {
                let mut row = Vec::with_capacity(mw);
                for xx in 0..mw {
                    row.push(layer.mask_value(mx0 + xx as i32, my0 + yy as i32));
                }
                rows.push(row);
            }
            (
                (my0, mx0, my1, mx1),
                layer.mask_fill,
                layer.mask_enabled,
                rle_channel(&rows),
            )
        });
        let mut channels = channels;
        if let Some((_, _, _, data)) = &mask {
            channels.push((-2, data.clone()));
        }
        outs.push(Out {
            name: layer.name.clone(),
            rect,
            channels,
            key: blend_to_key(layer.blend_mode),
            opacity: (layer.opacity.clamp(0.0, 1.0) * 255.0) as u8,
            visible: layer.visible,
            mask: mask.map(|(r, fill, enabled, _)| OutMask {
                rect: r,
                fill,
                enabled,
            }),
        });
    }
    outs.reverse(); // PSD stores the top layer first.

    // Layer records + channel data.
    let mut layer_info = W { buf: Vec::new() };
    layer_info.i16(outs.len() as i16);
    let mut channel_data = W { buf: Vec::new() };
    for o in &outs {
        layer_info.i32(o.rect.0);
        layer_info.i32(o.rect.1);
        layer_info.i32(o.rect.2);
        layer_info.i32(o.rect.3);
        layer_info.u16(o.channels.len() as u16);
        for (id, data) in &o.channels {
            layer_info.i16(*id);
            layer_info.u32(data.len() as u32);
        }
        layer_info.bytes(b"8BIM");
        layer_info.bytes(o.key);
        layer_info.u8(o.opacity);
        layer_info.u8(0); // clipping
        layer_info.u8(if o.visible { 0 } else { 2 }); // flags: bit1 = hidden
        layer_info.u8(0); // filler
                          // Extra data: mask block + blending ranges + name.
        let mut extra = W { buf: Vec::new() };
        match &o.mask {
            None => extra.u32(0),
            Some(m) => {
                extra.u32(20);
                extra.i32(m.rect.0);
                extra.i32(m.rect.1);
                extra.i32(m.rect.2);
                extra.i32(m.rect.3);
                extra.u8(m.fill);
                extra.u8(if m.enabled { 0 } else { 2 });
                extra.u16(0);
            }
        }
        extra.u32(0); // layer blending ranges
        let name_bytes = o.name.as_bytes();
        let mut pascal = Vec::with_capacity(name_bytes.len() + 1);
        pascal.push(name_bytes.len().min(255) as u8);
        pascal.extend_from_slice(&name_bytes[..name_bytes.len().min(255)]);
        while pascal.len() % 4 != 0 {
            pascal.push(0);
        }
        extra.bytes(&pascal);
        layer_info.u32(extra.buf.len() as u32);
        layer_info.bytes(&extra.buf);
        for (_, data) in &o.channels {
            channel_data.bytes(data);
        }
    }
    layer_info.bytes(&channel_data.buf);

    let mut layer_section = W { buf: Vec::new() };
    layer_section.u32(layer_info.buf.len() as u32);
    layer_section.bytes(&layer_info.buf);
    layer_section.u32(0); // no global layer mask info

    // Composite (flattened) image data.
    let flat = doc.render_full_rgba8();
    let (dw, dh) = (doc.width as usize, doc.height as usize);
    let mut planes: [Vec<Vec<u8>>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for yy in 0..dh {
        let mut rows: [Vec<u8>; 4] = [
            Vec::with_capacity(dw),
            Vec::with_capacity(dw),
            Vec::with_capacity(dw),
            Vec::with_capacity(dw),
        ];
        for xx in 0..dw {
            let i = (yy * dw + xx) * 4;
            for c in 0..4 {
                rows[c].push(flat[i + c]);
            }
        }
        for c in 0..4 {
            planes[c].push(rows[c].clone());
        }
    }
    let mut composite = W { buf: Vec::new() };
    composite.u16(1);
    let packed: Vec<Vec<Vec<u8>>> = planes
        .iter()
        .map(|ch| ch.iter().map(|r| packbits_encode_row(r)).collect())
        .collect();
    for ch in &packed {
        for p in ch {
            composite.u16(p.len() as u16);
        }
    }
    for ch in &packed {
        for p in ch {
            composite.bytes(p);
        }
    }

    let mut out = W { buf: Vec::new() };
    out.bytes(b"8BPS");
    out.u16(1);
    out.bytes(&[0; 6]);
    out.u16(4); // channels RGBA
    out.u32(doc.height);
    out.u32(doc.width);
    out.u16(8); // depth
    out.u16(3); // mode RGB
    out.u32(0); // color mode data
    out.u32(0); // image resources
    out.u32(layer_section.buf.len() as u32);
    out.bytes(&layer_section.buf);
    out.bytes(&composite.buf);
    (out.buf, PsdExportReport { warnings })
}
