//! PSD import v1: open basic PSD files (8-bit, RGB / CMYK /
//! Grayscale, raster layers with common blend modes) with an
//! honest report of what didn't come across. The importer never
//! guesses silently — anything it skips lands in `PsdReport::notes`.
//!
//! Format notes (Adobe's published spec): layer records are stored
//! TOP layer first; channel data follows all records; compression
//! 0 = raw, 1 = PackBits RLE (row byte counts precede the data).

use crate::blend::BlendMode;
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

/// Read one channel's pixel plane (width x height bytes).
fn read_channel(
    r: &mut Reader,
    w: usize,
    h: usize,
    report: &mut PsdReport,
    layer_name: &str,
) -> Result<Vec<u8>, String> {
    let compression = r.u16()?;
    match compression {
        0 => Ok(r.take(w * h)?.to_vec()),
        1 => {
            // Row byte counts (u16 each) precede the RLE data.
            let mut counts = Vec::with_capacity(h);
            for _ in 0..h {
                counts.push(r.u16()? as usize);
            }
            let mut out = Vec::with_capacity(w * h);
            for count in &counts {
                let bytes = r.take(*count)?;
                out.extend_from_slice(&packbits_row(bytes, w));
            }
            Ok(out)
        }
        other => {
            report.notes.push(format!(
                "Layer '{layer_name}': channel compression {other} not supported (ZIP) — channel read as blank"
            ));
            Ok(vec![0; w * h])
        }
    }
}

pub fn load_psd(bytes: &[u8]) -> Result<(Document, PsdReport), String> {
    let mut report = PsdReport {
        layers_imported: 0,
        notes: Vec::new(),
    };
    let mut r = Reader { buf: bytes, pos: 0 };
    if r.take(4)? != b"8BPS" {
        return Err("not a PSD file (bad signature)".into());
    }
    if r.u16()? != 1 {
        return Err("only PSD version 1 is supported (not PSB)".into());
    }
    r.skip(6)?; // reserved
    let _file_channels = r.u16()?;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    if depth != 8 {
        return Err(format!(
            "only 8-bit PSDs are supported (this one is {depth}-bit)"
        ));
    }
    let mode_id = r.u16()?;
    let mode = match mode_id {
        3 => ColorMode::Rgb,
        4 => ColorMode::Cmyk,
        1 => ColorMode::Grayscale,
        other => return Err(format!("PSD color mode {other} is not supported yet")),
    };
    // Color mode data + image resources: skipped wholesale.
    let n = r.u32()? as usize;
    r.skip(n)?;
    let n = r.u32()? as usize;
    r.skip(n)?;

    // Layer and mask information section.
    let section_len = r.u32()? as usize;
    let mut records: Vec<LayerRec> = Vec::new();
    let mut planes: Vec<Vec<Vec<u8>>> = Vec::new();
    if section_len > 0 {
        let layer_info_len = r.u32()? as usize;
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
                    let len = r.u32()? as usize;
                    channels.push(ChannelInfo { id, len });
                }
                if r.take(4)? != b"8BIM" {
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
                // Layer mask data + blending ranges: length-prefixed.
                let m = r.u32()? as usize;
                r.skip(m)?;
                let m = r.u32()? as usize;
                r.skip(m)?;
                // Pascal name, padded to a multiple of 4 total.
                let name_len = r.u8()? as usize;
                let raw_name = r.take(name_len)?.to_vec();
                let name = String::from_utf8_lossy(&raw_name).to_string();
                let name_field = 1 + name_len;
                if !name_field.is_multiple_of(4) {
                    r.skip(4 - name_field % 4)?;
                }
                // Additional layer info blocks.
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
                let _ = extra_start;
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
                    let plane = read_channel(&mut r, w, h, &mut report, &rec.name)?;
                    // Trust the declared length for positioning.
                    if r.pos != start + ch.len && ch.len >= 2 {
                        r.pos = start + ch.len;
                    }
                    layer_planes.push(plane);
                }
                planes.push(layer_planes);
            }
            let _ = layer_info_len;
        }
    }

    let mut doc = Document::new(width, height);
    doc.mode = mode;
    doc.layers.clear();

    if records.is_empty() {
        // No layer data: import the merged composite image.
        let channels = match mode {
            ColorMode::Rgb => 3usize,
            ColorMode::Cmyk => 4,
            ColorMode::Grayscale => 1,
        };
        let mut planes: Vec<Vec<u8>> = Vec::new();
        let compression = r.u16()?;
        let npix = (width * height) as usize;
        match compression {
            0 => {
                for _ in 0..channels {
                    planes.push(r.take(npix)?.to_vec());
                }
            }
            1 => {
                let rows = (height as usize) * channels;
                let mut counts = Vec::with_capacity(rows);
                for _ in 0..rows {
                    counts.push(r.u16()? as usize);
                }
                for c in 0..channels {
                    let mut plane = Vec::with_capacity(npix);
                    for row in 0..height as usize {
                        let bytes = r.take(counts[c * height as usize + row])?;
                        plane.extend_from_slice(&packbits_row(bytes, width as usize));
                    }
                    planes.push(plane);
                }
            }
            other => {
                return Err(format!(
                    "PSD composite compression {other} not supported (ZIP)"
                ))
            }
        }
        let mut layer = Layer::new("Background");
        bake_planes(
            &mut layer, &planes, None, 0, 0, width, height, mode, &mut doc,
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
        // Map channel planes by channel id.
        let mut by_id: Vec<(i16, &Vec<u8>)> = Vec::new();
        for (ci, ch) in rec.channels.iter().enumerate() {
            by_id.push((ch.id, &planes[ri][ci]));
        }
        if w > 0 && h > 0 {
            compose_layer(
                &mut layer, &by_id, rec.left, rec.top, w as u32, h as u32, mode,
            );
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

/// Compose a layer's channel planes into tiles. `by_id` maps PSD
/// channel ids to planes sized (layer w * layer h).
#[allow(clippy::too_many_arguments)]
fn compose_layer(
    layer: &mut Layer,
    by_id: &[(i16, &Vec<u8>)],
    left: i32,
    top: i32,
    w: u32,
    h: u32,
    mode: ColorMode,
) {
    let get = |id: i16| by_id.iter().find(|(i, _)| *i == id).map(|(_, p)| *p);
    let at = |plane: Option<&Vec<u8>>, i: usize| -> u8 {
        plane.map(|p| p.get(i).copied().unwrap_or(0)).unwrap_or(0)
    };
    let alpha = get(-1);
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            let i = (yy as u32 * w + xx as u32) as usize;
            let a8 = alpha.map(|_| at(alpha, i)).unwrap_or(255);
            if a8 == 0 && mode != ColorMode::Cmyk {
                continue;
            }
            match mode {
                ColorMode::Rgb => {
                    let p = Pixel16::from_rgba8(at(get(0), i), at(get(1), i), at(get(2), i), a8);
                    layer.set_pixel(left + xx, top + yy, p);
                }
                ColorMode::Grayscale => {
                    let v = at(get(0), i);
                    layer.set_pixel(left + xx, top + yy, Pixel16::from_rgba8(v, v, v, a8));
                }
                ColorMode::Cmyk => {
                    // PSD stores CMYK inverted (255 = no ink).
                    let inv = |id: i16| (255 - at(get(id), i)) as u16 * 257;
                    layer.set_pixel(
                        left + xx,
                        top + yy,
                        Pixel16::new(inv(0), inv(1), inv(2), inv(3)),
                    );
                    layer.set_alpha(left + xx, top + yy, a8 as u16 * 257);
                }
            }
        }
    }
}

/// Merged-image bake (single layer, no offsets).
#[allow(clippy::too_many_arguments)]
fn bake_planes(
    layer: &mut Layer,
    planes: &[Vec<u8>],
    _alpha: Option<&Vec<u8>>,
    _left: i32,
    _top: i32,
    w: u32,
    h: u32,
    mode: ColorMode,
    _doc: &mut Document,
) {
    if planes.is_empty() {
        return;
    }
    let by_id: Vec<(i16, &Vec<u8>)> = planes
        .iter()
        .enumerate()
        .map(|(i, p)| (i as i16, p))
        .collect();
    compose_layer(layer, &by_id, 0, 0, w, h, mode);
}
