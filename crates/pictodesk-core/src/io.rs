//! Raster file IO — File > Open / Save for the classic formats:
//! PNG, JPEG, TIFF (plus BMP). Decoding/encoding is the
//! pure-Rust `image` crate; pixels land in a one-layer document.
//!
//! Color: if the file carries an embedded ICC profile (PNG/JPEG),
//! pixels are converted from that profile into the document's
//! working space (sRGB) on import — the Phase 0 ICC engine doing
//! real work. Exports are written as sRGB.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat as CrateFormat};

use crate::document::Document;
use crate::icc::IccProfile;
use crate::pixel::Pixel16;
use crate::tile::Tile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
    Tiff,
    Bmp,
    Gif,
    Pcx,
    Tga,
}

impl ExportFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpeg),
            "tif" | "tiff" => Some(ExportFormat::Tiff),
            "bmp" => Some(ExportFormat::Bmp),
            "pcx" => Some(ExportFormat::Pcx),
            "tga" => Some(ExportFormat::Tga),
            "gif" => Some(ExportFormat::Gif),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum IoError {
    Decode(String),
    Encode(String),
    Io(std::io::Error),
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoError::Decode(m) => write!(f, "could not decode image: {m}"),
            IoError::Encode(m) => write!(f, "could not encode image: {m}"),
            IoError::Io(e) => write!(f, "file error: {e}"),
        }
    }
}

impl std::error::Error for IoError {}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError::Io(e)
    }
}

fn embedded_icc(bytes: &[u8], format: CrateFormat) -> Option<Vec<u8>> {
    match format {
        CrateFormat::Png => {
            let mut dec = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).ok()?;
            dec.icc_profile().ok().flatten()
        }
        CrateFormat::Jpeg => {
            let mut dec = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes)).ok()?;
            dec.icc_profile().ok().flatten()
        }
        _ => None,
    }
}

/// Decode image bytes (any supported format) into a document with a
/// single layer named "Background", like opening a flat photo.
/// PCX (ZSoft) decode: 24-bit (3 planes x 8 bits) and 8-bit
/// palette images, RLE compressed. Returns RGBA8 pixels.
/// Phase 6.5: the `image` crate has no PCX codec, so the
/// format is handled here directly.
pub fn decode_pcx(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 128 || bytes[0] != 0x0A || bytes[2] > 1 {
        return None;
    }
    let bpp = bytes[3] as usize;
    let xmin = u16::from_le_bytes([bytes[4], bytes[5]]) as i32;
    let ymin = u16::from_le_bytes([bytes[6], bytes[7]]) as i32;
    let xmax = u16::from_le_bytes([bytes[8], bytes[9]]) as i32;
    let ymax = u16::from_le_bytes([bytes[10], bytes[11]]) as i32;
    let w = (xmax - xmin + 1).max(0) as usize;
    let h = (ymax - ymin + 1).max(0) as usize;
    let planes = bytes[65] as usize;
    let bpl = u16::from_le_bytes([bytes[66], bytes[67]]) as usize;
    if w == 0 || h == 0 || bpp != 8 || !(planes == 1 || planes == 3) || bpl < w {
        return None;
    }
    let mut data = &bytes[128..];
    // Optional trailing 256-color palette (0x0C marker).
    let mut palette: Option<&[u8]> = None;
    if planes == 1 && bytes.len() >= 769 && bytes[bytes.len() - 769] == 0x0C {
        palette = Some(&bytes[bytes.len() - 768..]);
        data = &bytes[128..bytes.len() - 769];
    }
    let row_len = bpl * planes;
    let mut raw = vec![0u8; row_len * h];
    let mut pos = 0usize;
    let mut out_i = 0usize;
    while out_i < raw.len() && pos < data.len() {
        let b = data[pos];
        pos += 1;
        if bytes[2] == 1 && (b & 0xC0) == 0xC0 {
            let count = (b & 0x3F) as usize;
            if pos >= data.len() {
                break;
            }
            let v = data[pos];
            pos += 1;
            for _ in 0..count {
                if out_i < raw.len() {
                    raw[out_i] = v;
                    out_i += 1;
                }
            }
        } else {
            raw[out_i] = b;
            out_i += 1;
        }
    }
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            if planes == 3 {
                rgba[o] = raw[y * row_len + x];
                rgba[o + 1] = raw[y * row_len + bpl + x];
                rgba[o + 2] = raw[y * row_len + 2 * bpl + x];
            } else if let Some(pal) = palette {
                let idx = raw[y * row_len + x] as usize * 3;
                rgba[o] = pal[idx];
                rgba[o + 1] = pal[idx + 1];
                rgba[o + 2] = pal[idx + 2];
            } else {
                let v = raw[y * row_len + x];
                rgba[o] = v;
                rgba[o + 1] = v;
                rgba[o + 2] = v;
            }
            rgba[o + 3] = 255;
        }
    }
    Some((w as u32, h as u32, rgba))
}

/// PCX encode: 24-bit planes, RLE, alpha flattened over
/// black (the format has no alpha channel).
pub fn encode_pcx(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; 128];
    out[0] = 0x0A;
    out[1] = 5;
    out[2] = 1;
    out[3] = 8;
    let xmax = (w - 1) as u16;
    let ymax = (h - 1) as u16;
    out[8..10].copy_from_slice(&xmax.to_le_bytes());
    out[10..12].copy_from_slice(&ymax.to_le_bytes());
    out[12..14].copy_from_slice(&72u16.to_le_bytes());
    out[14..16].copy_from_slice(&72u16.to_le_bytes());
    out[65] = 3;
    let bpl = (w + (w & 1)) as u16;
    out[66..68].copy_from_slice(&bpl.to_le_bytes());
    let bpl = bpl as usize;
    let mut row = vec![0u8; bpl];
    for y in 0..h as usize {
        for plane in 0..3usize {
            row.iter_mut().for_each(|v| *v = 0);
            for x in 0..w as usize {
                row[x] = rgba[(y * w as usize + x) * 4 + plane];
            }
            // RLE encode this plane row.
            let mut i = 0usize;
            while i < bpl {
                let v = row[i];
                let mut run = 1usize;
                while i + run < bpl && row[i + run] == v && run < 63 {
                    run += 1;
                }
                if run > 1 || (v & 0xC0) == 0xC0 {
                    out.push(0xC0 | run as u8);
                    out.push(v);
                } else {
                    out.push(v);
                }
                i += run;
            }
        }
    }
    out
}

pub fn import_image(bytes: &[u8]) -> Result<Document, IoError> {
    // PCX has no magic the image crate knows; route it first.
    if bytes.first() == Some(&0x0A) {
        if let Some((w, h, rgba)) = decode_pcx(bytes) {
            // Re-encode as PNG and take the normal path
            // (lossless; keeps one import pipeline).
            let mut png = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut png);
            use image::ImageEncoder;
            if enc
                .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
                .is_ok()
            {
                return import_image(&png);
            }
        }
    }
    // TGA has no magic bytes for guess_format; fall back to
    // an explicit TGA decode when detection fails.
    let format = image::guess_format(bytes).unwrap_or(image::ImageFormat::Tga);
    let icc = embedded_icc(bytes, format);
    let img: DynamicImage = match image::load_from_memory(bytes) {
        Ok(i) => i,
        Err(_) => image::load_from_memory_with_format(bytes, image::ImageFormat::Tga)
            .map_err(|e| IoError::Decode(e.to_string()))?,
    };
    let (w, h) = (img.width(), img.height());
    // 16-bit sources keep their full precision end to end (Phase 2:
    // 16-bit/channel editing exposed), including color management:
    // an embedded profile is converted into the sRGB working space
    // through the f32 ICC path, at full 16-bit precision.
    let mut pixels16: Option<Vec<Pixel16>> = img
        .as_rgba16()
        .map(|buf| {
            buf.as_raw()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| Pixel16::new(c[0], c[1], c[2], c[3]))
                .collect()
        })
        .or_else(|| {
            img.as_rgb16().map(|buf| {
                buf.as_raw()
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| Pixel16::new(c[0], c[1], c[2], 65535))
                    .collect()
            })
        });
    if let Some(pixels) = &mut pixels16 {
        if let Some(profile_bytes) = &icc {
            if let Ok(src_profile) = IccProfile::from_bytes(profile_bytes) {
                let mut flat: Vec<u16> = pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
                if src_profile
                    .convert_rgba_u16(&IccProfile::srgb(), &mut flat)
                    .is_ok()
                {
                    *pixels = flat
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| Pixel16::new(c[0], c[1], c[2], c[3]))
                        .collect();
                }
            }
        }
    }
    let mut doc = Document::new(w, h);
    if let Some(pixels) = &pixels16 {
        for ty in 0..(h as i32 + 255) / 256 {
            for tx in 0..(w as i32 + 255) / 256 {
                let mut tile = Tile::new_transparent();
                let mut any = false;
                for ly in 0..256u32 {
                    let y = ty as u32 * 256 + ly;
                    if y >= h {
                        break;
                    }
                    for lx in 0..256u32 {
                        let x = tx as u32 * 256 + lx;
                        if x >= w {
                            break;
                        }
                        let p = pixels[(y * w + x) as usize];
                        if p != Pixel16::TRANSPARENT {
                            any = true;
                        }
                        tile.set(lx, ly, p);
                    }
                }
                if any {
                    tile.dirty = true;
                    doc.layers[0].tiles.insert((tx, ty), tile);
                }
            }
        }
        return Ok(doc);
    }
    let mut rgba = img.to_rgba8();
    if let Some(profile_bytes) = icc {
        if let Ok(src_profile) = IccProfile::from_bytes(&profile_bytes) {
            let mut converted = vec![0u8; rgba.len()];
            if src_profile
                .convert_rgba8(&IccProfile::srgb(), rgba.as_raw(), &mut converted)
                .is_ok()
            {
                rgba = image::RgbaImage::from_raw(rgba.width(), rgba.height(), converted)
                    .unwrap_or(rgba);
            }
        }
    }
    let raw = rgba.as_raw();
    // Fill tiles directly (row-segment copies) instead of per-pixel
    // set_pixel — imports of real photos stay fast.
    for ty in 0..(h as i32 + 255) / 256 {
        for tx in 0..(w as i32 + 255) / 256 {
            let mut tile = Tile::new_transparent();
            let mut any = false;
            for ly in 0..256u32 {
                let y = ty as u32 * 256 + ly;
                if y >= h {
                    break;
                }
                for lx in 0..256u32 {
                    let x = tx as u32 * 256 + lx;
                    if x >= w {
                        break;
                    }
                    let i = ((y * w + x) * 4) as usize;
                    let p = Pixel16::from_rgba8(raw[i], raw[i + 1], raw[i + 2], raw[i + 3]);
                    if p != Pixel16::TRANSPARENT {
                        any = true;
                    }
                    tile.set(lx, ly, p);
                }
            }
            if any {
                tile.dirty = true;
                doc.layers[0].tiles.insert((tx, ty), tile);
            }
        }
    }
    Ok(doc)
}

/// Encode the flattened document. JPEG has no alpha: pixels are
/// written over black (standard for opaque photos, which is the
/// Phase 1 case).
pub fn export_image(
    doc: &Document,
    format: ExportFormat,
    jpeg_quality: u8,
) -> Result<Vec<u8>, IoError> {
    // A CMYK-mode document saved as TIFF goes out as real CMYK
    // separations (with its clipping path, if one is set).
    if matches!(format, ExportFormat::Tiff) && doc.mode == crate::document::ColorMode::Cmyk {
        return Ok(export_tiff_cmyk(doc));
    }
    let rgba = doc.render_full_rgba8();
    let (w, h) = (doc.width, doc.height);
    let mut out: Vec<u8> = Vec::new();
    match format {
        ExportFormat::Png => {
            let enc = image::codecs::png::PngEncoder::new(&mut out);
            enc.write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| IoError::Encode(e.to_string()))?;
        }
        ExportFormat::Jpeg => {
            let rgb: Vec<u8> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, jpeg_quality);
            enc.write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
                .map_err(|e| IoError::Encode(e.to_string()))?;
        }
        ExportFormat::Tiff => {
            // TIFF encoding needs a Seek writer.
            let mut cursor = Cursor::new(Vec::new());
            let enc = image::codecs::tiff::TiffEncoder::new(&mut cursor);
            enc.write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| IoError::Encode(e.to_string()))?;
            out = cursor.into_inner();
        }
        ExportFormat::Bmp => {
            let enc = image::codecs::bmp::BmpEncoder::new(&mut out);
            enc.write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| IoError::Encode(e.to_string()))?;
        }
        ExportFormat::Gif => {
            let enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| IoError::Encode(e.to_string()))?;
        }
        ExportFormat::Pcx => {
            out = encode_pcx(&rgba, w, h);
        }
        ExportFormat::Tga => {
            let enc = image::codecs::tga::TgaEncoder::new(&mut out);
            enc.write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| IoError::Encode(e.to_string()))?;
        }
    }
    Ok(out)
}

pub fn load_file(path: &Path) -> Result<Document, IoError> {
    let bytes = std::fs::read(path)?;
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("psd"))
        .unwrap_or(false)
    {
        let (doc, _report) = crate::psd::load_psd(&bytes).map_err(IoError::Decode)?;
        return Ok(doc);
    }
    import_image(&bytes)
}

/// Weighted Optimization (Save for Web; v0.6.1): encode the
/// flattened image as JPEG at `q_hi` and at `q_lo`, decode
/// both, composite per pixel by `weights` (255 keeps the
/// high-quality pixel, 0 the low-quality one), then encode
/// the composite at `q_hi`. The weights normally come from an
/// alpha channel: detailed areas ride at high quality while
/// backgrounds compress hard. Returns None on any failure.
pub fn export_weighted_jpeg(doc: &Document, q_hi: u8, q_lo: u8, weights: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (doc.width, doc.height);
    if w == 0 || h == 0 || weights.len() < (w * h) as usize {
        return None;
    }
    let hi = export_image(doc, ExportFormat::Jpeg, q_hi).ok()?;
    let lo = export_image(doc, ExportFormat::Jpeg, q_lo).ok()?;
    let hi_px = image::load_from_memory(&hi).ok()?.to_rgb8().into_raw();
    let lo_px = image::load_from_memory(&lo).ok()?.to_rgb8().into_raw();
    if hi_px.len() != lo_px.len() || hi_px.len() < (w * h * 3) as usize {
        return None;
    }
    let mut out_rgb = vec![0u8; hi_px.len()];
    for (p, &wv) in weights.iter().enumerate().take((w * h) as usize) {
        let t = wv as u32;
        for c in 0..3 {
            let i = p * 3 + c;
            out_rgb[i] = ((hi_px[i] as u32 * t + lo_px[i] as u32 * (255 - t)) / 255) as u8;
        }
    }
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q_hi);
    enc.write_image(&out_rgb, w, h, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

pub fn save_file(path: &Path, doc: &Document, jpeg_quality: u8) -> Result<(), IoError> {
    let format = ExportFormat::from_path(path)
        .ok_or_else(|| IoError::Encode(format!("no raster format for {}", path.display())))?;
    let bytes = export_image(doc, format, jpeg_quality)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

// ------------------------------------------------------- CMYK TIFF
//
// A hand-rolled classic TIFF writer for CMYK export: the `image`
// crate's TIFF encoder cannot attach Photoshop image resources, and
// the clipping path (tag 34377) is the point of a print export.
// Uncompressed, single strip, little-endian — every print shop and
// every TIFF reader accepts this shape.

fn photoshop_path_resource(path: &crate::path::Path, doc_w: u32, doc_h: u32) -> Vec<u8> {
    // Path data in Adobe's documented format: per subpath, a length
    // record (u16 BE knot count + 24 zero bytes), then one 26-byte
    // record per knot: selector, padding, then the preceding control
    // point, anchor, and following control point, each as (vertical,
    // horizontal) 8.24 fixed-point fractions of the image size.
    let mut data: Vec<u8> = Vec::new();
    let frac = |v: f32, dim: u32| -> [u8; 4] {
        let n = (v / dim.max(1) as f32 * 16_777_216.0).round() as i32;
        n.to_be_bytes()
    };
    for sp in &path.subpaths {
        if sp.anchors.len() < 2 {
            continue;
        }
        data.extend_from_slice(&(sp.anchors.len() as u16).to_be_bytes());
        data.extend_from_slice(&[0u8; 24]);
        let selector: u8 = if sp.closed { 2 } else { 5 };
        for a in &sp.anchors {
            data.push(selector);
            data.push(0);
            for pt in [a.in_ctrl, a.point, a.out_ctrl] {
                data.extend_from_slice(&frac(pt.1, doc_h));
                data.extend_from_slice(&frac(pt.0, doc_w));
            }
        }
    }
    // Image resource block: "8BIM" | u16 id | Pascal name (padded to
    // even) | u32 data size | data (padded to even).
    let mut irb: Vec<u8> = Vec::new();
    let block = |id: u16, name: &str, payload: &[u8], irb: &mut Vec<u8>| {
        irb.extend_from_slice(b"8BIM");
        irb.extend_from_slice(&id.to_be_bytes());
        let nb = name.as_bytes();
        irb.push(nb.len() as u8);
        irb.extend_from_slice(nb);
        if (1 + nb.len()) % 2 == 1 {
            irb.push(0);
        }
        irb.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        irb.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            irb.push(0);
        }
    };
    block(2000, "", &data, &mut irb);
    // Resource 2999: the name of the clipping path.
    let mut name_payload: Vec<u8> = Vec::new();
    let nb = path.name.as_bytes();
    name_payload.push(nb.len() as u8);
    name_payload.extend_from_slice(nb);
    block(2999, "", &name_payload, &mut irb);
    irb
}

/// Export the document as a CMYK TIFF (8 bits per channel, C,M,Y,K
/// byte order per pixel). When the document has a clipping path set,
/// it is embedded as a Photoshop image resource (tag 34377), the way
/// print workflows expect.
pub fn export_tiff_cmyk(doc: &Document) -> Vec<u8> {
    let (w, h) = (doc.width, doc.height);
    // Spot channels print as their own plates: in a Separation TIFF
    // they ride as extra ink samples, named via InkNames.
    let spots: Vec<&crate::document::AlphaChannel> = doc
        .alphas
        .iter()
        .filter(|c| c.spot_color.is_some())
        .collect();
    let spp = 4 + spots.len();
    let mut pixels: Vec<u8> = Vec::with_capacity((w * h) as usize * spp);
    for yy in 0..h as i32 {
        for xx in 0..w as i32 {
            let (c, m, y, k) = doc.render_cmyk_pixel(xx, yy);
            let d = |v: u16| ((v as u32 + 128) / 257) as u8;
            pixels.push(d(c));
            pixels.push(d(m));
            pixels.push(d(y));
            pixels.push(d(k));
            for ch in &spots {
                pixels.push(ch.value(w, xx, yy));
            }
        }
    }
    let mut ink_names: Vec<u8> = Vec::new();
    for n in ["Cyan", "Magenta", "Yellow", "Black"] {
        ink_names.extend_from_slice(n.as_bytes());
        ink_names.push(0);
    }
    for ch in &spots {
        ink_names.extend_from_slice(ch.name.as_bytes());
        ink_names.push(0);
    }
    let irb = doc
        .clipping_path
        .and_then(|i| doc.paths.get(i))
        .map(|p| photoshop_path_resource(p, w, h));

    // Layout: header (8) | IFD | external data (bits-per-sample,
    // ink names, strip pixels, IRB). Entries sorted by tag.
    let n_entries: u16 = if irb.is_some() { 14 } else { 13 };
    let ifd_size = 2 + n_entries as u32 * 12 + 4;
    let bits_off = 8 + ifd_size;
    let names_off = bits_off + spp as u32 * 2;
    let strip_off = names_off + ink_names.len() as u32;
    let irb_off = strip_off + pixels.len() as u32;

    let mut out: Vec<u8> = Vec::new();
    let u16le = |v: u16, out: &mut Vec<u8>| out.extend_from_slice(&v.to_le_bytes());
    let u32le = |v: u32, out: &mut Vec<u8>| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"II");
    u16le(42, &mut out);
    u32le(8, &mut out); // IFD offset
    u16le(n_entries, &mut out);
    let entry = |tag: u16, typ: u16, count: u32, val: u32, out: &mut Vec<u8>| {
        u16le(tag, out);
        u16le(typ, out);
        u32le(count, out);
        u32le(val, out);
    };
    entry(256, 4, 1, w, &mut out); // ImageWidth (LONG)
    entry(257, 4, 1, h, &mut out); // ImageLength
    entry(258, 3, spp as u32, bits_off, &mut out); // BitsPerSample -> offset
    entry(259, 3, 1, 1, &mut out); // Compression = none
    entry(262, 3, 1, 5, &mut out); // Photometric = Separation
    entry(273, 4, 1, strip_off, &mut out); // StripOffsets
    entry(277, 3, 1, spp as u32, &mut out); // SamplesPerPixel
    entry(278, 4, 1, h, &mut out); // RowsPerStrip
    entry(279, 4, 1, pixels.len() as u32, &mut out); // StripByteCounts
    entry(284, 3, 1, 1, &mut out); // PlanarConfig = chunky
    entry(332, 3, 1, 1, &mut out); // InkSet = CMYK
    entry(333, 2, ink_names.len() as u32, names_off, &mut out); // InkNames
    entry(334, 3, 1, spp as u32, &mut out); // NumberOfInks
    if let Some(irb) = &irb {
        entry(34377, 1, irb.len() as u32, irb_off, &mut out); // Photoshop IRB
    }
    u32le(0, &mut out); // no next IFD
    for _ in 0..spp {
        u16le(8, &mut out); // BitsPerSample: 8 per ink
    }
    out.extend_from_slice(&ink_names);
    out.extend_from_slice(&pixels);
    if let Some(irb) = &irb {
        out.extend_from_slice(irb);
    }
    out
}

/// One plate of an EPS separation: PostScript text with hex image
/// data (grayscale plate, 8-bit).
fn eps_plate(doc: &Document, title: &str, plate: &[u8]) -> Vec<u8> {
    let (w, h) = (doc.width, doc.height);
    let mut s = String::new();
    s.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
    s.push_str("%%Creator: PictoDesk\n");
    s.push_str(&format!("%%Title: {title}\n"));
    s.push_str(&format!("%%BoundingBox: 0 0 {w} {h}\n"));
    s.push_str("%%EndComments\n");
    s.push_str(&format!("{w} {h} scale\n"));
    s.push_str(&format!("/picstr {w} string def\n"));
    s.push_str(&format!(
        "{w} {h} 8 [{w} 0 0 -{h} 0 {h}] {{ currentfile picstr readhexstring pop }} image\n"
    ));
    for (i, b) in plate.iter().enumerate() {
        if i > 0 && i % 36 == 0 {
            s.push('\n');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s.push_str("\nshowpage\n%%EOF\n");
    s.into_bytes()
}

/// EPS DCS 2.0 export (Phase 5): a master file with a composite
/// preview plus one EPS per separation plate — the four process
/// plates from the composited inks, and one plate per spot
/// channel. Returns (file name, contents) pairs; the caller
/// writes them side by side. Plate values are ink amounts
/// (255 = full ink), the DCS convention.
pub fn export_dcs(doc: &Document, base: &str) -> Vec<(String, Vec<u8>)> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let mut plates: Vec<Vec<u8>> = vec![vec![0u8; w * h]; 4];
    let mut spots: Vec<(String, Vec<u8>)> = Vec::new();
    for ch in &doc.alphas {
        if ch.spot_color.is_some() {
            spots.push((ch.name.clone(), vec![0u8; w * h]));
        }
    }
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (ink, a) = doc.composite_inks(x, y);
            let i = (y as usize) * w + x as usize;
            for c in 0..4 {
                plates[c][i] = (ink[c] * a * 255.0 + 0.5) as u8;
            }
        }
    }
    // Spot plates come from the spot alpha channels' own data.
    let mut spot_iter = spots.iter_mut();
    for ch in &doc.alphas {
        if ch.spot_color.is_some() {
            let (_, plate) = spot_iter.next().unwrap();
            for (i, v) in ch.data.iter().enumerate().take(w * h) {
                plate[i] = *v;
            }
        }
    }
    let names = ["Cyan", "Magenta", "Yellow", "Black"];
    let suffixes = ["C", "M", "Y", "K"];
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut plate_comments = String::new();
    for (c, plate) in plates.iter().enumerate() {
        let fname = format!("{base}.{}.eps", suffixes[c]);
        plate_comments.push_str(&format!("%%PlateFile: ({fname}) {}\n", names[c]));
        out.push((
            fname,
            eps_plate(doc, &format!("{base} ({})", names[c]), plate),
        ));
    }
    for (name, plate) in &spots {
        let safe: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        let fname = format!("{base}.{safe}.eps");
        plate_comments.push_str(&format!("%%PlateFile: ({fname}) {name}\n"));
        out.push((fname, eps_plate(doc, &format!("{base} ({name})"), plate)));
    }
    // Master: composite preview + DCS header comments naming the
    // plates. Preview is a grayscale EPS of the composite so any
    // PostScript consumer can place it.
    let flat = doc.render_full_rgba8();
    let mut gray = vec![0u8; w * h];
    for i in 0..w * h {
        let (r, g, b) = (
            flat[i * 4] as u32,
            flat[i * 4 + 1] as u32,
            flat[i * 4 + 2] as u32,
        );
        gray[i] = ((r * 77 + g * 150 + b * 29) >> 8) as u8;
    }
    let mut master = String::from_utf8(eps_plate(doc, &format!("{base} (master)"), &gray)).unwrap();
    let insert_at = master.find("%%EndComments").unwrap();
    let header = format!(
        "%%DCSVersion: 2.0\n%%DocumentProcessColors: Cyan Magenta Yellow Black\n{plate_comments}"
    );
    master.insert_str(insert_at, &header);
    out.insert(0, (format!("{base}.eps"), master.into_bytes()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc() -> Document {
        let mut doc = Document::new(16, 12);
        doc.layers[0].fill_rect(0, 0, 16, 12, Pixel16::from_rgba8(30, 120, 200, 255));
        doc.layers[0].fill_rect(4, 3, 5, 5, Pixel16::from_rgba8(240, 200, 40, 255));
        doc
    }

    #[test]
    fn png_round_trip_is_lossless() {
        let doc = sample_doc();
        let bytes = export_image(&doc, ExportFormat::Png, 90).unwrap();
        let back = import_image(&bytes).unwrap();
        assert_eq!(back.render_checksum(), doc.render_checksum());
    }

    #[test]
    fn tiff_and_bmp_round_trip_lossless() {
        let doc = sample_doc();
        for fmt in [ExportFormat::Tiff, ExportFormat::Bmp] {
            let bytes = export_image(&doc, fmt, 90).unwrap();
            let back = import_image(&bytes).unwrap();
            assert_eq!(back.render_checksum(), doc.render_checksum(), "{fmt:?}");
        }
    }

    #[test]
    fn jpeg_round_trip_is_close() {
        let doc = sample_doc();
        let bytes = export_image(&doc, ExportFormat::Jpeg, 95).unwrap();
        let back = import_image(&bytes).unwrap();
        let px = back.render_pixel(1, 1).to_rgba8();
        assert!(
            (px.0 as i32 - 30).abs() <= 4 && (px.2 as i32 - 200).abs() <= 6,
            "jpeg px {px:?}"
        );
    }

    #[test]
    fn weighted_jpeg_uses_the_weight_map() {
        let doc = sample_doc();
        let n = (doc.width * doc.height) as usize;
        // All weights high: output tracks the plain q95 encode.
        let all_hi = vec![255u8; n];
        let w_hi = export_weighted_jpeg(&doc, 95, 20, &all_hi).expect("weighted hi");
        assert_eq!(&w_hi[0..2], &[0xFF, 0xD8], "JPEG SOI");
        // All weights low: the composite comes from the q20
        // encode, so the result must differ from all-high.
        let all_lo = vec![0u8; n];
        let w_lo = export_weighted_jpeg(&doc, 95, 20, &all_lo).expect("weighted lo");
        assert_ne!(w_hi, w_lo, "weight map must change the output");
        // A short weight map is refused.
        assert!(export_weighted_jpeg(&doc, 95, 20, &[0u8; 4]).is_none());
    }

    #[test]
    fn garbage_bytes_are_a_decode_error() {
        assert!(import_image(b"not an image at all").is_err());
    }
}
