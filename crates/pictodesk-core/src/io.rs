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
}

impl ExportFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpeg),
            "tif" | "tiff" => Some(ExportFormat::Tiff),
            "bmp" => Some(ExportFormat::Bmp),
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
pub fn import_image(bytes: &[u8]) -> Result<Document, IoError> {
    let format = image::guess_format(bytes).map_err(|e| IoError::Decode(e.to_string()))?;
    let icc = embedded_icc(bytes, format);
    let img: DynamicImage =
        image::load_from_memory(bytes).map_err(|e| IoError::Decode(e.to_string()))?;
    let (w, h) = (img.width(), img.height());
    // 16-bit sources keep their full precision end to end (Phase 2:
    // 16-bit/channel editing exposed). The ICC conversion path works
    // at 8-bit, so it applies to 8-bit sources; a 16-bit file's
    // profile conversion is a later refinement.
    let pixels16: Option<Vec<Pixel16>> = if let Some(buf) = img.as_rgba16() {
        Some(
            buf.as_raw()
                .chunks_exact(4)
                .map(|c| Pixel16::new(c[0], c[1], c[2], c[3]))
                .collect(),
        )
    } else if let Some(buf) = img.as_rgb16() {
        Some(
            buf.as_raw()
                .chunks_exact(3)
                .map(|c| Pixel16::new(c[0], c[1], c[2], 65535))
                .collect(),
        )
    } else {
        None
    };
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
    }
    Ok(out)
}

pub fn load_file(path: &Path) -> Result<Document, IoError> {
    let bytes = std::fs::read(path)?;
    import_image(&bytes)
}

pub fn save_file(path: &Path, doc: &Document, jpeg_quality: u8) -> Result<(), IoError> {
    let format = ExportFormat::from_path(path)
        .ok_or_else(|| IoError::Encode(format!("no raster format for {}", path.display())))?;
    let bytes = export_image(doc, format, jpeg_quality)?;
    std::fs::write(path, bytes)?;
    Ok(())
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
    fn garbage_bytes_are_a_decode_error() {
        assert!(import_image(b"not an image at all").is_err());
    }
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
    let mut block = |id: u16, name: &str, payload: &[u8], irb: &mut Vec<u8>| {
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
    let rgba = doc.render_full_rgba8();
    let mut pixels: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    for px in rgba.chunks_exact(4) {
        let (c, m, y, k) = crate::color::rgb_to_cmyk(
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
        );
        pixels.push((c * 255.0).round() as u8);
        pixels.push((m * 255.0).round() as u8);
        pixels.push((y * 255.0).round() as u8);
        pixels.push((k * 255.0).round() as u8);
    }
    let irb = doc
        .clipping_path
        .and_then(|i| doc.paths.get(i))
        .map(|p| photoshop_path_resource(p, w, h));

    // Layout: header (8) | IFD | external data (bits-per-sample,
    // strip pixels, IRB). Entries sorted by tag, as TIFF requires.
    let n_entries: u16 = if irb.is_some() { 11 } else { 10 };
    let ifd_size = 2 + n_entries as u32 * 12 + 4;
    let bits_off = 8 + ifd_size;
    let strip_off = bits_off + 8;
    let irb_off = strip_off + pixels.len() as u32;

    let mut out: Vec<u8> = Vec::new();
    let mut u16le = |v: u16, out: &mut Vec<u8>| out.extend_from_slice(&v.to_le_bytes());
    let mut u32le = |v: u32, out: &mut Vec<u8>| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"II");
    u16le(42, &mut out);
    u32le(8, &mut out); // IFD offset
    u16le(n_entries, &mut out);
    let mut entry = |tag: u16, typ: u16, count: u32, val: u32, out: &mut Vec<u8>| {
        u16le(tag, out);
        u16le(typ, out);
        u32le(count, out);
        u32le(val, out);
    };
    entry(256, 4, 1, w, &mut out); // ImageWidth (LONG)
    entry(257, 4, 1, h, &mut out); // ImageLength
    entry(258, 3, 4, bits_off, &mut out); // BitsPerSample -> offset
    entry(259, 3, 1, 1, &mut out); // Compression = none
    entry(262, 3, 1, 5, &mut out); // Photometric = CMYK separations
    entry(273, 4, 1, strip_off, &mut out); // StripOffsets
    entry(277, 3, 1, 4, &mut out); // SamplesPerPixel
    entry(278, 4, 1, h, &mut out); // RowsPerStrip
    entry(279, 4, 1, pixels.len() as u32, &mut out); // StripByteCounts
    entry(284, 3, 1, 1, &mut out); // PlanarConfig = chunky
    if let Some(irb) = &irb {
        entry(34377, 1, irb.len() as u32, irb_off, &mut out); // Photoshop IRB
    }
    u32le(0, &mut out); // no next IFD
    for _ in 0..4 {
        u16le(8, &mut out); // BitsPerSample values 8,8,8,8
    }
    out.extend_from_slice(&pixels);
    if let Some(irb) = &irb {
        out.extend_from_slice(irb);
    }
    out
}
