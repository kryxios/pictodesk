//! Raster file IO — File > Open / Save for the formats Photoshop
//! 1.0 spoke: PNG, JPEG, TIFF (plus BMP). Decoding/encoding is the
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
    let (w, h) = (rgba.width(), rgba.height());
    let mut doc = Document::new(w, h);
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
