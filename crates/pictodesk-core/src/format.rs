//! The `.pdesk` file format — v2 (v1 still readable).
//!
//! v2 layout (all integers little-endian):
//!   magic "PDSK" | u32 version (=2) | u32 uncompressed_len
//!   DEFLATE stream of the complete v1 payload (below)
//!
//! v1 payload layout:
//!   magic "PDSK" | u32 version (=1)
//!   u32 width | u32 height | u8 working_space (0=sRGB,1=Adobe,2=ProPhoto)
//!   u32 layer_count
//!   per layer:
//!     u16 name_len + name bytes (UTF-8)
//!     f32 opacity | u8 visible | u8 blend_mode_id | i32 offset_x | i32 offset_y
//!     u32 tile_count
//!     per tile: i32 tx | i32 ty | TILE_PIXELS * 4 * u16 (r,g,b,a per pixel)
//!
//! Design rules: lossless for everything the document model holds, and
//! old files never become unreadable — the loader dispatches on the
//! version field forever. Compression is DEFLATE (flate2/miniz_oxide,
//! pure Rust) over the whole payload: flat tiles, gradients, and layer
//! metadata all compress in one pass, and output is deterministic.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::blend::BlendMode;
use crate::color::WorkingSpace;
use crate::document::Document;
use crate::layer::Layer;
use crate::pixel::Pixel16;
use crate::tile::{Tile, TILE_PIXELS};

pub const MAGIC: &[u8; 4] = b"PDSK";
pub const FORMAT_VERSION: u32 = 2;
pub const LEGACY_VERSION: u32 = 1;
pub const FILE_EXTENSION: &str = "pdesk";

#[derive(Debug)]
pub enum FormatError {
    Io(io::Error),
    BadMagic,
    UnsupportedVersion(u32),
    Corrupt(&'static str),
}

impl From<io::Error> for FormatError {
    fn from(e: io::Error) -> Self {
        FormatError::Io(e)
    }
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "I/O error: {e}"),
            FormatError::BadMagic => write!(f, "not a PictoDesk file (bad magic)"),
            FormatError::UnsupportedVersion(v) => write!(f, "unsupported .pdesk version {v}"),
            FormatError::Corrupt(why) => write!(f, "corrupt .pdesk file: {why}"),
        }
    }
}

impl std::error::Error for FormatError {}

struct Writer {
    buf: Vec<u8>,
}
impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        if self.pos + n > self.buf.len() {
            return Err(FormatError::Corrupt("unexpected end of file"));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, FormatError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, FormatError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

fn space_id(s: WorkingSpace) -> u8 {
    match s {
        WorkingSpace::Srgb => 0,
        WorkingSpace::AdobeRgb => 1,
        WorkingSpace::ProPhoto => 2,
    }
}
fn space_from_id(id: u8) -> Result<WorkingSpace, FormatError> {
    Ok(match id {
        0 => WorkingSpace::Srgb,
        1 => WorkingSpace::AdobeRgb,
        2 => WorkingSpace::ProPhoto,
        _ => return Err(FormatError::Corrupt("unknown working space")),
    })
}

/// The v1 (uncompressed) serialization. Kept as the payload format
/// inside v2 and for backward-compatibility tests.
pub fn serialize_v1(doc: &Document) -> Vec<u8> {
    let mut w = Writer { buf: Vec::new() };
    w.bytes(MAGIC);
    w.u32(LEGACY_VERSION);
    w.u32(doc.width);
    w.u32(doc.height);
    w.u8(space_id(doc.working_space));
    w.u32(doc.layers.len() as u32);
    for layer in &doc.layers {
        let name = layer.name.as_bytes();
        w.u16(name.len() as u16);
        w.bytes(name);
        w.f32(layer.opacity);
        w.u8(if layer.visible { 1 } else { 0 });
        w.u8(layer.blend_mode.to_id());
        w.i32(layer.offset.0);
        w.i32(layer.offset.1);
        // Sort tile coords so files are deterministic (same doc = same bytes).
        let mut coords: Vec<_> = layer.tiles.keys().cloned().collect();
        coords.sort();
        w.u32(coords.len() as u32);
        for coord in coords {
            let tile = &layer.tiles[&coord];
            w.i32(coord.0);
            w.i32(coord.1);
            for p in &tile.pixels {
                w.u16(p.r);
                w.u16(p.g);
                w.u16(p.b);
                w.u16(p.a);
            }
        }
    }
    w.buf
}

/// The current (v2) serialization: header + DEFLATE-compressed v1 payload.
pub fn serialize(doc: &Document) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;
    let payload = serialize_v1(doc);
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&payload)
        .expect("deflate into memory cannot fail");
    let compressed = enc.finish().expect("deflate finish cannot fail");
    let mut out = Vec::with_capacity(12 + compressed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    out
}

pub fn deserialize(buf: &[u8]) -> Result<Document, FormatError> {
    if buf.len() < 8 || &buf[0..4] != MAGIC {
        return Err(FormatError::BadMagic);
    }
    let version = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    match version {
        LEGACY_VERSION => deserialize_v1(buf),
        FORMAT_VERSION => {
            use flate2::read::DeflateDecoder;
            use std::io::Read;
            if buf.len() < 12 {
                return Err(FormatError::Corrupt("v2 header truncated"));
            }
            let expected_len = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
            let mut dec = DeflateDecoder::new(&buf[12..]);
            let mut payload = Vec::with_capacity(expected_len);
            dec.read_to_end(&mut payload)
                .map_err(|_| FormatError::Corrupt("deflate stream invalid"))?;
            if payload.len() != expected_len {
                return Err(FormatError::Corrupt("decompressed size mismatch"));
            }
            deserialize_v1(&payload)
        }
        v => Err(FormatError::UnsupportedVersion(v)),
    }
}

fn deserialize_v1(buf: &[u8]) -> Result<Document, FormatError> {
    let mut r = Reader { buf, pos: 0 };
    if r.take(4)? != MAGIC {
        return Err(FormatError::BadMagic);
    }
    let version = r.u32()?;
    if version != LEGACY_VERSION {
        return Err(FormatError::UnsupportedVersion(version));
    }
    let width = r.u32()?;
    let height = r.u32()?;
    let space = space_from_id(r.u8()?)?;
    let layer_count = r.u32()?;
    let mut doc = Document::new(width, height);
    doc.working_space = space;
    doc.layers.clear();
    for _ in 0..layer_count {
        let name_len = r.u16()? as usize;
        let name_bytes = r.take(name_len)?;
        let name = String::from_utf8(name_bytes.to_vec())
            .map_err(|_| FormatError::Corrupt("layer name not UTF-8"))?;
        let mut layer = Layer::new(name);
        layer.opacity = r.f32()?;
        layer.visible = r.u8()? != 0;
        layer.blend_mode =
            BlendMode::from_id(r.u8()?).ok_or(FormatError::Corrupt("unknown blend mode"))?;
        layer.offset = (r.i32()?, r.i32()?);
        let tile_count = r.u32()?;
        for _ in 0..tile_count {
            let coord = (r.i32()?, r.i32()?);
            let mut tile = Tile::new_transparent();
            for i in 0..TILE_PIXELS {
                tile.pixels[i] = Pixel16 {
                    r: r.u16()?,
                    g: r.u16()?,
                    b: r.u16()?,
                    a: r.u16()?,
                };
            }
            tile.dirty = false;
            layer.tiles.insert(coord, tile);
        }
        doc.layers.push(layer);
    }
    if doc.layers.is_empty() {
        doc.layers.push(Layer::new("Background"));
    }
    doc.active_layer = doc.layers.len() - 1;
    Ok(doc)
}

pub fn save(doc: &Document, path: &Path) -> Result<(), FormatError> {
    fs::write(path, serialize(doc))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Document, FormatError> {
    deserialize(&fs::read(path)?)
}

/// Autosave / crash recovery: write to `path.tmp` then rename over
/// `path`, so a crash mid-save can never leave a half-written file —
/// the worst case is the previous recovery file, intact.
pub fn save_atomic(doc: &Document, path: &Path) -> Result<(), FormatError> {
    let mut tmp: PathBuf = path.to_path_buf();
    tmp.set_extension("pdesk.tmp");
    fs::write(&tmp, serialize(doc))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// The standard recovery-file path next to a document path.
pub fn recovery_path_for(doc_path: &Path) -> PathBuf {
    let mut p = doc_path.to_path_buf();
    p.set_extension("pdesk.recovery");
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::BlendMode;

    fn sample_doc() -> Document {
        let mut doc = Document::new(600, 400); // spans multiple tiles
        doc.working_space = WorkingSpace::AdobeRgb;
        doc.layers[0].fill_rect(0, 0, 600, 400, Pixel16::from_rgba8(30, 60, 90, 255));
        let top = doc.add_layer("Accent ✓");
        doc.layers[top].opacity = 0.75;
        doc.layers[top].blend_mode = BlendMode::Screen;
        doc.layers[top].fill_rect(250, 150, 100, 100, Pixel16::from_rgba8(200, 100, 50, 255));
        doc
    }

    #[test]
    fn round_trip_is_lossless() {
        let doc = sample_doc();
        let bytes = serialize(&doc);
        let back = deserialize(&bytes).unwrap();
        assert_eq!(doc, back);
        assert_eq!(doc.render_checksum(), back.render_checksum());
    }

    #[test]
    fn serialization_is_deterministic() {
        let doc = sample_doc();
        assert_eq!(serialize(&doc), serialize(&doc));
    }

    #[test]
    fn v1_files_still_load() {
        let doc = sample_doc();
        let v1 = serialize_v1(&doc);
        let back = deserialize(&v1).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn v2_compresses_and_round_trips() {
        let doc = sample_doc();
        let v1 = serialize_v1(&doc);
        let v2 = serialize(&doc);
        assert!(
            v2.len() < v1.len() / 4,
            "expected real compression: v1={} v2={}",
            v1.len(),
            v2.len()
        );
        assert_eq!(deserialize(&v2).unwrap(), doc);
        // Deterministic: same doc, same bytes.
        assert_eq!(serialize(&doc), v2);
    }

    #[test]
    fn bad_magic_and_version_are_rejected() {
        assert!(matches!(
            deserialize(b"XXXX1234"),
            Err(FormatError::BadMagic)
        ));
        let mut bytes = serialize(&sample_doc());
        bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(
            deserialize(&bytes),
            Err(FormatError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn save_load_and_atomic_recovery_via_disk() {
        let dir = std::env::temp_dir().join("pictodesk-format-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.pdesk");
        let doc = sample_doc();
        save(&doc, &path).unwrap();
        assert_eq!(load(&path).unwrap(), doc);

        let rec = recovery_path_for(&path);
        save_atomic(&doc, &rec).unwrap();
        assert!(rec.exists());
        assert_eq!(load(&rec).unwrap().render_checksum(), doc.render_checksum());
        // The temp staging file must not linger after a successful rename.
        let mut tmp = rec.clone();
        tmp.set_extension("pdesk.tmp");
        assert!(!tmp.exists());
        fs::remove_dir_all(&dir).ok();
    }
}
