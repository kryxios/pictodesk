//! The `.pdk` file format — v3 (v1 and v2 still readable).
//!
//! v3 layout: the v2 envelope (magic "PDSK" | u32 version (=3) |
//! u32 uncompressed_len | DEFLATE stream) around a payload that is
//! the v1 payload plus an extension block:
//!   "PX2E" | u8 color mode (0=RGB, 1=CMYK, 2=Grayscale,
//!   3=Lab, 4=Indexed, 5=Bitmap, 6=Duotone, 7=Multichannel)
//!   u32 alpha_count; per alpha: u16 name_len + name | u8 spot flag
//!     (+ 3 bytes RGB when spot) | u32 data_len + data bytes
//!   i32 clipping_path index (-1 = none)
//!   u32 path_count; per path: u16 name_len + name | u32 subpaths;
//!     per subpath: u8 closed | u32 anchors; per anchor 6 x f32
//!     (point, in_ctrl, out_ctrl)
//!   u8 has_work_path (+ one path body when 1, name may be empty)
//! v2 files carry no extension block and load with defaults.
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
pub const FORMAT_VERSION: u32 = 3;
pub const V2_VERSION: u32 = 2;
pub const LEGACY_VERSION: u32 = 1;
pub const FILE_EXTENSION: &str = "pdk";

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
            FormatError::UnsupportedVersion(v) => write!(f, "unsupported .pdk version {v}"),
            FormatError::Corrupt(why) => write!(f, "corrupt .pdk file: {why}"),
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
    fn u64(&mut self, v: u64) {
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
    fn u64(&mut self) -> Result<u64, FormatError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
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

fn serialize_ext(w: &mut Writer, doc: &Document) {
    fn write_path(w: &mut Writer, path: &crate::path::Path) {
        let name = path.name.as_bytes();
        w.u16(name.len() as u16);
        w.bytes(name);
        w.u32(path.subpaths.len() as u32);
        for sp in &path.subpaths {
            w.u8(if sp.closed { 1 } else { 0 });
            w.u32(sp.anchors.len() as u32);
            for a in &sp.anchors {
                for v in [a.point, a.in_ctrl, a.out_ctrl] {
                    w.f32(v.0);
                    w.f32(v.1);
                }
            }
        }
    }
    w.bytes(b"PX2E");
    w.u8(match doc.mode {
        crate::document::ColorMode::Rgb => 0,
        crate::document::ColorMode::Cmyk => 1,
        crate::document::ColorMode::Grayscale => 2,
        crate::document::ColorMode::Lab => 3,
        crate::document::ColorMode::Indexed => 4,
        crate::document::ColorMode::Bitmap => 5,
        crate::document::ColorMode::Duotone => 6,
        crate::document::ColorMode::Multichannel => 7,
    });
    w.u32(doc.alphas.len() as u32);
    for ch in &doc.alphas {
        let name = ch.name.as_bytes();
        w.u16(name.len() as u16);
        w.bytes(name);
        match ch.spot_color {
            Some((r, g, b)) => {
                w.u8(1);
                w.u8(r);
                w.u8(g);
                w.u8(b);
            }
            None => w.u8(0),
        }
        w.u32(ch.data.len() as u32);
        w.bytes(&ch.data);
    }
    w.i32(doc.clipping_path.map(|i| i as i32).unwrap_or(-1));
    w.u32(doc.paths.len() as u32);
    for path in &doc.paths {
        write_path(w, path);
    }
    match &doc.work_path {
        Some(wp) => {
            w.u8(1);
            write_path(w, wp);
        }
        None => w.u8(0),
    }
    // CMYK alpha planes (v0.2.1 addition, appended so files written
    // before it still parse: the reader treats the section as
    // optional trailing data). Per layer: tile count, then per
    // tile (coord, TILE_PIXELS x u16).
    for layer in &doc.layers {
        w.u32(layer.cmyk_alpha.len() as u32);
        let mut coords: Vec<_> = layer.cmyk_alpha.keys().copied().collect();
        coords.sort();
        for coord in coords {
            w.i32(coord.0);
            w.i32(coord.1);
            for v in &layer.cmyk_alpha[&coord] {
                w.u16(*v);
            }
        }
    }
    // Layer link flags (v0.3.0 addition, also optional trailing).
    for layer in &doc.layers {
        w.u8(layer.linked as u8);
    }
    // History log (v0.3.2 addition, also optional trailing):
    // count, then per event (unix secs, name).
    w.u32(doc.history_log.len() as u32);
    for ev in &doc.history_log {
        w.u64(ev.when);
        let name = ev.action.as_bytes();
        w.u16(name.len() as u16);
        w.bytes(name);
    }
    // Phase 4 section (v0.4.0 addition, also optional trailing):
    // per layer a mask block then an adjustment block, then the
    // document guides.
    for layer in &doc.layers {
        match &layer.mask {
            Some(mask) => {
                w.u8(1);
                w.u8(layer.mask_enabled as u8);
                w.u8(layer.mask_fill);
                w.u32(mask.len() as u32);
                let mut coords: Vec<_> = mask.keys().copied().collect();
                coords.sort();
                for coord in coords {
                    w.i32(coord.0);
                    w.i32(coord.1);
                    w.bytes(&mask[&coord]);
                }
            }
            None => w.u8(0),
        }
        use crate::adjust::Adjustment;
        match &layer.adjustment {
            None => w.u8(0),
            Some(Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            }) => {
                w.u8(1);
                w.u8(*in_black);
                w.u8(*in_white);
                w.f32(*gamma);
                w.u8(*out_black);
                w.u8(*out_white);
            }
            Some(Adjustment::Curves { points }) => {
                w.u8(2);
                w.u8(points.len() as u8);
                for (px, py) in points {
                    w.u8(*px);
                    w.u8(*py);
                }
            }
            Some(Adjustment::BrightnessContrast {
                brightness,
                contrast,
            }) => {
                w.u8(3);
                w.i32(*brightness);
                w.i32(*contrast);
            }
            Some(Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
            }) => {
                w.u8(4);
                w.i32(*hue);
                w.i32(*saturation);
                w.i32(*lightness);
            }
            Some(Adjustment::ColorBalance {
                shadows,
                midtones,
                highlights,
            }) => {
                w.u8(5);
                for t in [shadows, midtones, highlights] {
                    w.i32(t.0);
                    w.i32(t.1);
                    w.i32(t.2);
                }
            }
            Some(Adjustment::PhotoFilter { color, density }) => {
                w.u8(6);
                w.u8(color.0);
                w.u8(color.1);
                w.u8(color.2);
                w.u8(*density);
            }
            Some(Adjustment::GradientMap { from, to, reverse }) => {
                w.u8(7);
                w.u8(from.0);
                w.u8(from.1);
                w.u8(from.2);
                w.u8(to.0);
                w.u8(to.1);
                w.u8(to.2);
                w.u8(*reverse as u8);
            }
        }
    }
    w.u32(doc.guides_x.len() as u32);
    for g in &doc.guides_x {
        w.i32(*g);
    }
    w.u32(doc.guides_y.len() as u32);
    for g in &doc.guides_y {
        w.i32(*g);
    }
    // Phase 5 section (v0.5.0 addition, optional trailing): layer
    // effects, type-layer specs, then document-level global angle,
    // samplers, and the defined pattern.
    fn write_px(w: &mut Writer, p: &Pixel16) {
        w.u16(p.r);
        w.u16(p.g);
        w.u16(p.b);
        w.u16(p.a);
    }
    fn write_shadow(w: &mut Writer, s: &crate::effects::ShadowSpec) {
        w.u8(s.enabled as u8);
        w.u8(s.mode.to_id());
        write_px(w, &s.color);
        w.f32(s.opacity);
        w.i32(s.angle);
        w.u8(s.use_global_angle as u8);
        w.u32(s.distance);
        w.u32(s.blur);
        w.u32(s.intensity);
    }
    fn write_glow(w: &mut Writer, g: &crate::effects::GlowSpec) {
        w.u8(g.enabled as u8);
        w.u8(g.mode.to_id());
        write_px(w, &g.color);
        w.f32(g.opacity);
        w.u32(g.blur);
        w.u32(g.intensity);
        w.u8(g.source_center as u8);
    }
    fn write_style(w: &mut Writer, s: &crate::text::CharStyle) {
        let f = s.font.as_bytes();
        w.u16(f.len() as u16);
        w.bytes(f);
        let st = s.style.as_bytes();
        w.u16(st.len() as u16);
        w.bytes(st);
        w.f32(s.size);
        w.f32(s.tracking);
        write_px(w, &s.color);
        w.u8(s.faux_bold as u8);
        w.u8(s.faux_italic as u8);
        w.u8(s.underline as u8);
        w.f32(s.baseline_shift);
        w.u8(s.auto_kerning as u8);
    }
    for layer in &doc.layers {
        match &layer.effects {
            None => w.u8(0),
            Some(fx) => {
                w.u8(1);
                write_shadow(w, &fx.drop_shadow);
                write_shadow(w, &fx.inner_shadow);
                write_glow(w, &fx.outer_glow);
                write_glow(w, &fx.inner_glow);
                let b = &fx.bevel;
                w.u8(b.enabled as u8);
                w.u8(b.style);
                w.u32(b.depth);
                w.u8(b.up as u8);
                w.u8(b.hi_mode.to_id());
                write_px(w, &b.hi_color);
                w.f32(b.hi_opacity);
                w.u8(b.sh_mode.to_id());
                write_px(w, &b.sh_color);
                w.f32(b.sh_opacity);
                w.i32(b.angle);
                w.u8(b.use_global_angle as u8);
            }
        }
        match &layer.text {
            None => w.u8(0),
            Some(t) => {
                w.u8(1);
                let tb = t.text.as_bytes();
                w.u32(tb.len() as u32);
                w.bytes(tb);
                w.f32(t.leading);
                w.u8(t.align);
                w.u8(t.vertical as u8);
                write_style(w, &t.default_style);
                w.u32(t.runs.len() as u32);
                for run in &t.runs {
                    w.u32(run.start as u32);
                    w.u32(run.end as u32);
                    write_style(w, &run.style);
                }
            }
        }
    }
    w.i32(doc.global_angle);
    w.u32(doc.samplers.len() as u32);
    for (sx, sy) in &doc.samplers {
        w.i32(*sx);
        w.i32(*sy);
    }
    match &doc.pattern {
        None => w.u8(0),
        Some(p) => {
            w.u8(1);
            let n = p.name.as_bytes();
            w.u16(n.len() as u16);
            w.bytes(n);
            w.u32(p.width);
            w.u32(p.height);
            for px in &p.px {
                write_px(w, px);
            }
        }
    }
    // Phase 6 section (v0.6.0 addition, optional trailing):
    // new effects + contours, advanced blending, layer org,
    // shape/fill payloads, text warp/paragraph, then document
    // sets, notes, slices, custom shapes, global altitude.
    fn write_str(w: &mut Writer, t: &str) {
        let b = t.as_bytes();
        w.u16(b.len() as u16);
        w.bytes(b);
    }
    for layer in &doc.layers {
        match &layer.effects {
            None => w.u8(0),
            Some(fx) => {
                w.u8(1);
                let sp = &fx.satin;
                w.u8(sp.enabled as u8);
                w.u8(sp.mode.to_id());
                write_px(w, &sp.color);
                w.f32(sp.opacity);
                w.i32(sp.angle);
                w.u32(sp.distance);
                w.u32(sp.blur);
                w.u8(sp.invert as u8);
                w.u8(sp.contour);
                let co = &fx.color_overlay;
                w.u8(co.enabled as u8);
                w.u8(co.mode.to_id());
                write_px(w, &co.color);
                w.f32(co.opacity);
                let go = &fx.gradient_overlay;
                w.u8(go.enabled as u8);
                w.u8(go.mode.to_id());
                w.f32(go.opacity);
                write_px(w, &go.from);
                write_px(w, &go.to);
                w.u8(go.style);
                w.i32(go.angle);
                w.u32(go.scale);
                let po = &fx.pattern_overlay;
                w.u8(po.enabled as u8);
                w.u8(po.mode.to_id());
                w.f32(po.opacity);
                w.u8(po.kind);
                w.u32(po.scale);
                let st = &fx.stroke;
                w.u8(st.enabled as u8);
                w.u8(st.mode.to_id());
                write_px(w, &st.color);
                w.f32(st.opacity);
                w.u32(st.size);
                w.u8(st.position);
                w.u8(fx.drop_shadow.contour);
                w.u8(fx.inner_shadow.contour);
                w.u8(fx.outer_glow.contour);
                w.u8(fx.inner_glow.contour);
                w.u8(fx.bevel.contour);
                w.u8(fx.bevel.texture as u8);
                w.u8(fx.bevel.texture_kind);
                w.u32(fx.bevel.texture_depth);
            }
        }
        w.f32(layer.fill_opacity);
        for v in layer.blend_if {
            w.u8(v);
        }
        w.u8(layer.knockout);
        w.u8(layer.blend_channels);
        w.u8(layer.blend_clipped_group as u8);
        w.u8(layer.blend_interior_group as u8);
        w.u8(layer.transparency_shapes as u8);
        w.u8(layer.mask_hides_effects as u8);
        w.u16(layer.group);
        w.u8(layer.clipped as u8);
        w.u8(layer.locks);
        w.u8(layer.color);
        match &layer.shape {
            None => w.u8(0),
            Some(sp) => {
                w.u8(1);
                w.u8(sp.kind);
                w.i32(sp.rect.0);
                w.i32(sp.rect.1);
                w.i32(sp.rect.2);
                w.i32(sp.rect.3);
                w.u8(sp.sides);
                w.u8(sp.star as u8);
                w.u32(sp.radius);
                write_px(w, &sp.fill);
                w.u8(sp.fill_on as u8);
                write_px(w, &sp.stroke);
                w.u32(sp.stroke_width);
                w.u32(sp.points.len() as u32);
                for pt in &sp.points {
                    w.f32(pt.0);
                    w.f32(pt.1);
                }
            }
        }
        match &layer.fill {
            None => w.u8(0),
            Some(fl) => {
                w.u8(1);
                w.u8(fl.kind);
                write_px(w, &fl.color);
                write_px(w, &fl.to);
                w.i32(fl.angle);
                w.u8(fl.style);
                w.u32(fl.scale);
                w.u8(fl.pattern_kind);
            }
        }
        match &layer.text {
            None => w.u8(0),
            Some(t) => {
                w.u8(1);
                w.u32(t.box_w);
                w.u32(t.box_h);
                w.f32(t.space_before);
                w.f32(t.space_after);
                w.f32(t.first_indent);
                w.u8(t.hyphenate as u8);
                w.u8(t.extras);
                w.u8(t.warp_style.unwrap_or(255));
                w.f32(t.warp_bend);
                w.f32(t.warp_h);
                w.f32(t.warp_v);
            }
        }
    }
    w.i32(doc.global_altitude);
    w.u32(doc.sets.len() as u32);
    for set in &doc.sets {
        write_str(w, &set.name);
        w.f32(set.opacity);
        w.u8(set.mode.map(|m| m.to_id()).unwrap_or(255));
        w.u8(set.visible as u8);
        w.u8(set.color);
    }
    w.u32(doc.notes.len() as u32);
    for n in &doc.notes {
        w.i32(n.x);
        w.i32(n.y);
        w.u8(n.color);
        write_str(w, &n.text);
    }
    w.u32(doc.slices.len() as u32);
    for sl in &doc.slices {
        w.i32(sl.x);
        w.i32(sl.y);
        w.u32(sl.w);
        w.u32(sl.h);
        write_str(w, &sl.name);
        write_str(w, &sl.url);
        write_str(w, &sl.alt);
    }
    w.u32(doc.custom_shapes.len() as u32);
    for cs in &doc.custom_shapes {
        write_str(w, &cs.name);
        w.u32(cs.points.len() as u32);
        for pt in &cs.points {
            w.f32(pt.0);
            w.f32(pt.1);
        }
    }
    // v0.6.1 section (optional trailing): Blend If split
    // feather widths, one [u8; 4] per layer in layer order.
    for layer in &doc.layers {
        for v in layer.blend_if_split {
            w.u8(v);
        }
    }
    // v0.6.1 section (optional trailing): note audio — per
    // note a presence byte, then length + WAV bytes.
    for n in &doc.notes {
        match &n.audio {
            None => w.u8(0),
            Some(wav) => {
                w.u8(1);
                w.u32(wav.len() as u32);
                w.bytes(wav);
            }
        }
    }
    // Phase 6.5 section (optional trailing): color table,
    // duotone spec, multichannel planes.
    w.u16(doc.color_table.len() as u16);
    for c in &doc.color_table {
        w.u8(c.0);
        w.u8(c.1);
        w.u8(c.2);
    }
    match &doc.duotone {
        None => w.u8(0),
        Some(spec) => {
            w.u8(1);
            w.u8(spec.inks.len() as u8);
            for ink in &spec.inks {
                w.u8(ink.color.0);
                w.u8(ink.color.1);
                w.u8(ink.color.2);
                w.bytes(&ink.curve);
            }
        }
    }
    // Phase 6.5 Batch 3 section (optional trailing): alpha
    // channels' "color indicates masked areas" flags.
    for ch in &doc.alphas {
        w.u8(ch.masked as u8);
    }
    w.u8(doc.multi_planes.len() as u8);
    for (i, plane) in doc.multi_planes.iter().enumerate() {
        let info = doc.multi_channels.get(i);
        write_str(w, info.map(|m| m.name.as_str()).unwrap_or(""));
        let c = info.map(|m| m.color).unwrap_or((0, 0, 0));
        w.u8(c.0);
        w.u8(c.1);
        w.u8(c.2);
        w.u32(plane.len() as u32);
        for v in plane {
            w.u16(*v);
        }
    }
    // Phase 6.5 Batch 6 section (optional trailing): line
    // arrowhead flags per shape layer (layer index + flags).
    let arrowed: Vec<(usize, u8)> = doc
        .layers
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            l.shape.as_ref().and_then(|sp| {
                if sp.arrow_start || sp.arrow_end {
                    Some((i, (sp.arrow_start as u8) | ((sp.arrow_end as u8) << 1)))
                } else {
                    None
                }
            })
        })
        .collect();
    w.u16(arrowed.len() as u16);
    for (i, flags) in arrowed {
        w.u16(i as u16);
        w.u8(flags);
    }
    // Phase 6.5 Batch 6 section (optional trailing): File
    // Info fields.
    write_str(w, &doc.info_title);
    write_str(w, &doc.info_caption);
    write_str(w, &doc.info_keywords);
    write_str(w, &doc.info_copyright);
    write_str(w, &doc.info_credit);
    // Resolution (Preferences wiring): trailing, so older
    // readers stop at the File Info block above.
    w.f32(doc.resolution);
}

/// The current (v3) serialization: header + DEFLATE-compressed
/// payload (v1 body + Phase 2 extension block).
pub fn serialize(doc: &Document) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut payload = serialize_v1(doc);
    {
        let mut w = Writer { buf: Vec::new() };
        serialize_ext(&mut w, doc);
        payload.extend_from_slice(&w.buf);
    }
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
        FORMAT_VERSION | V2_VERSION => {
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
            let mut doc = deserialize_v1(&payload)?;
            if version == FORMAT_VERSION {
                parse_ext(&payload, &mut doc)?;
            }
            Ok(doc)
        }
        v => Err(FormatError::UnsupportedVersion(v)),
    }
}

fn parse_ext(payload: &[u8], doc: &mut Document) -> Result<(), FormatError> {
    // Re-walk the v1 body to find where the extension block starts.
    let mut r = Reader {
        buf: payload,
        pos: 0,
    };
    r.take(4)?;
    r.u32()?; // version
    r.u32()?;
    r.u32()?;
    r.u8()?; // working space
    let layer_count = r.u32()?;
    for _ in 0..layer_count {
        let name_len = r.u16()? as usize;
        r.take(name_len)?;
        r.f32()?;
        r.u8()?;
        r.u8()?;
        r.i32()?;
        r.i32()?;
        let tile_count = r.u32()?;
        let bytes_per_tile = crate::tile::TILE_PIXELS * 4 * 2;
        r.take(tile_count as usize * (8 + bytes_per_tile))?;
    }
    if r.take(4)? != b"PX2E" {
        return Err(FormatError::Corrupt("missing extension block"));
    }
    fn read_path(r: &mut Reader) -> Result<crate::path::Path, FormatError> {
        let name_len = r.u16()? as usize;
        let name = String::from_utf8(r.take(name_len)?.to_vec())
            .map_err(|_| FormatError::Corrupt("path name not UTF-8"))?;
        let mut path = crate::path::Path::new(name);
        let subs = r.u32()?;
        for _ in 0..subs {
            let closed = r.u8()? != 0;
            let n = r.u32()?;
            let mut anchors = Vec::new();
            for _ in 0..n {
                let mut v = [0f32; 6];
                for v in v.iter_mut() {
                    *v = r.f32()?;
                }
                anchors.push(crate::path::Anchor {
                    point: (v[0], v[1]),
                    in_ctrl: (v[2], v[3]),
                    out_ctrl: (v[4], v[5]),
                });
            }
            path.subpaths.push(crate::path::Subpath { anchors, closed });
        }
        Ok(path)
    }
    doc.mode = match r.u8()? {
        0 => crate::document::ColorMode::Rgb,
        1 => crate::document::ColorMode::Cmyk,
        2 => crate::document::ColorMode::Grayscale,
        3 => crate::document::ColorMode::Lab,
        4 => crate::document::ColorMode::Indexed,
        5 => crate::document::ColorMode::Bitmap,
        6 => crate::document::ColorMode::Duotone,
        7 => crate::document::ColorMode::Multichannel,
        _ => return Err(FormatError::Corrupt("unknown color mode")),
    };
    let alpha_count = r.u32()?;
    for _ in 0..alpha_count {
        let name_len = r.u16()? as usize;
        let name = String::from_utf8(r.take(name_len)?.to_vec())
            .map_err(|_| FormatError::Corrupt("channel name not UTF-8"))?;
        let spot = if r.u8()? != 0 {
            Some((r.u8()?, r.u8()?, r.u8()?))
        } else {
            None
        };
        let len = r.u32()? as usize;
        let data = r.take(len)?.to_vec();
        doc.alphas.push(crate::document::AlphaChannel {
            name,
            spot_color: spot,
            masked: true,
            data,
        });
    }
    let clip = r.i32()?;
    doc.clipping_path = if clip >= 0 { Some(clip as usize) } else { None };
    let path_count = r.u32()?;
    for _ in 0..path_count {
        doc.paths.push(read_path(&mut r)?);
    }
    if r.u8()? != 0 {
        doc.work_path = Some(read_path(&mut r)?);
    }
    // Optional trailing section: CMYK alpha planes. Files written
    // before v0.2.1 end exactly here.
    if r.pos < payload.len() {
        for layer in &mut doc.layers {
            let count = r.u32()?;
            for _ in 0..count {
                let coord = (r.i32()?, r.i32()?);
                let mut data = vec![0u16; TILE_PIXELS];
                for v in &mut data {
                    *v = r.u16()?;
                }
                layer.cmyk_alpha.insert(coord, data);
            }
        }
    }
    // Optional trailing section: layer link flags (v0.3.0).
    if r.pos < payload.len() {
        for layer in &mut doc.layers {
            layer.linked = r.u8()? != 0;
        }
    }
    // Optional trailing section: history log (v0.3.2). Files
    // written before it end exactly here.
    if r.pos < payload.len() {
        let count = r.u32()?;
        for _ in 0..count {
            let when = r.u64()?;
            let len = r.u16()? as usize;
            let action = String::from_utf8(r.take(len)?.to_vec())
                .map_err(|_| FormatError::Corrupt("history action not UTF-8"))?;
            doc.history_log
                .push(crate::document::HistoryEvent { when, action });
        }
    }
    // Optional trailing section: Phase 4 masks, adjustment layers,
    // and guides (v0.4.0). Files written before it end exactly here.
    if r.pos < payload.len() {
        use crate::adjust::Adjustment;
        for layer in &mut doc.layers {
            if r.u8()? != 0 {
                layer.mask_enabled = r.u8()? != 0;
                layer.mask_fill = r.u8()?;
                let count = r.u32()?;
                let mut mask = std::collections::HashMap::new();
                for _ in 0..count {
                    let coord = (r.i32()?, r.i32()?);
                    let data = r.take(crate::tile::TILE_PIXELS)?.to_vec();
                    mask.insert(coord, data);
                }
                layer.mask = Some(mask);
            }
            layer.adjustment = match r.u8()? {
                0 => None,
                1 => Some(Adjustment::Levels {
                    in_black: r.u8()?,
                    in_white: r.u8()?,
                    gamma: r.f32()?,
                    out_black: r.u8()?,
                    out_white: r.u8()?,
                }),
                2 => {
                    let n = r.u8()?;
                    let mut points = Vec::new();
                    for _ in 0..n {
                        points.push((r.u8()?, r.u8()?));
                    }
                    Some(Adjustment::Curves { points })
                }
                3 => Some(Adjustment::BrightnessContrast {
                    brightness: r.i32()?,
                    contrast: r.i32()?,
                }),
                4 => Some(Adjustment::HueSaturation {
                    hue: r.i32()?,
                    saturation: r.i32()?,
                    lightness: r.i32()?,
                }),
                5 => {
                    let read3 = |r: &mut Reader| -> Result<(i32, i32, i32), FormatError> {
                        Ok((r.i32()?, r.i32()?, r.i32()?))
                    };
                    Some(Adjustment::ColorBalance {
                        shadows: read3(&mut r)?,
                        midtones: read3(&mut r)?,
                        highlights: read3(&mut r)?,
                    })
                }
                6 => Some(Adjustment::PhotoFilter {
                    color: (r.u8()?, r.u8()?, r.u8()?),
                    density: r.u8()?,
                }),
                7 => Some(Adjustment::GradientMap {
                    from: (r.u8()?, r.u8()?, r.u8()?),
                    to: (r.u8()?, r.u8()?, r.u8()?),
                    reverse: r.u8()? != 0,
                }),
                _ => return Err(FormatError::Corrupt("unknown adjustment kind")),
            };
        }
        let nx = r.u32()?;
        for _ in 0..nx {
            doc.guides_x.push(r.i32()?);
        }
        let ny = r.u32()?;
        for _ in 0..ny {
            doc.guides_y.push(r.i32()?);
        }
    }
    // Optional trailing section: Phase 5 effects, type specs,
    // global angle, samplers, pattern (v0.5.0).
    if r.pos < payload.len() {
        fn read_px(r: &mut Reader) -> Result<Pixel16, FormatError> {
            Ok(Pixel16::new(r.u16()?, r.u16()?, r.u16()?, r.u16()?))
        }
        fn read_mode(r: &mut Reader) -> Result<BlendMode, FormatError> {
            BlendMode::from_id(r.u8()?).ok_or(FormatError::Corrupt("bad blend mode id"))
        }
        fn read_shadow(r: &mut Reader) -> Result<crate::effects::ShadowSpec, FormatError> {
            Ok(crate::effects::ShadowSpec {
                enabled: r.u8()? != 0,
                mode: read_mode(r)?,
                color: read_px(r)?,
                opacity: r.f32()?,
                angle: r.i32()?,
                use_global_angle: r.u8()? != 0,
                distance: r.u32()?,
                blur: r.u32()?,
                intensity: r.u32()?,
                contour: 0,
            })
        }
        fn read_glow(r: &mut Reader) -> Result<crate::effects::GlowSpec, FormatError> {
            Ok(crate::effects::GlowSpec {
                enabled: r.u8()? != 0,
                mode: read_mode(r)?,
                color: read_px(r)?,
                opacity: r.f32()?,
                blur: r.u32()?,
                intensity: r.u32()?,
                source_center: r.u8()? != 0,
                contour: 0,
            })
        }
        fn read_str(r: &mut Reader) -> Result<String, FormatError> {
            let n = r.u16()? as usize;
            Ok(String::from_utf8_lossy(r.take(n)?).into_owned())
        }
        fn read_style(r: &mut Reader) -> Result<crate::text::CharStyle, FormatError> {
            Ok(crate::text::CharStyle {
                font: read_str(r)?,
                style: read_str(r)?,
                size: r.f32()?,
                tracking: r.f32()?,
                color: read_px(r)?,
                faux_bold: r.u8()? != 0,
                faux_italic: r.u8()? != 0,
                underline: r.u8()? != 0,
                baseline_shift: r.f32()?,
                auto_kerning: r.u8()? != 0,
            })
        }
        for layer in &mut doc.layers {
            if r.u8()? != 0 {
                let drop_shadow = read_shadow(&mut r)?;
                let inner_shadow = read_shadow(&mut r)?;
                let outer_glow = read_glow(&mut r)?;
                let inner_glow = read_glow(&mut r)?;
                let bevel = crate::effects::BevelSpec {
                    enabled: r.u8()? != 0,
                    style: r.u8()?,
                    depth: r.u32()?,
                    up: r.u8()? != 0,
                    hi_mode: read_mode(&mut r)?,
                    hi_color: read_px(&mut r)?,
                    hi_opacity: r.f32()?,
                    sh_mode: read_mode(&mut r)?,
                    sh_color: read_px(&mut r)?,
                    sh_opacity: r.f32()?,
                    angle: r.i32()?,
                    use_global_angle: r.u8()? != 0,
                    contour: 0,
                    texture: false,
                    texture_kind: 0,
                    texture_depth: 100,
                };
                layer.effects = Some(crate::effects::Effects {
                    drop_shadow,
                    inner_shadow,
                    outer_glow,
                    inner_glow,
                    bevel,
                    ..Default::default()
                });
            }
            if r.u8()? != 0 {
                let n = r.u32()? as usize;
                let text = String::from_utf8_lossy(r.take(n)?).into_owned();
                let leading = r.f32()?;
                let align = r.u8()?;
                let vertical = r.u8()? != 0;
                let default_style = read_style(&mut r)?;
                let nr = r.u32()?;
                let mut runs = Vec::new();
                for _ in 0..nr {
                    let start = r.u32()? as usize;
                    let end = r.u32()? as usize;
                    let style = read_style(&mut r)?;
                    runs.push(crate::text::TextRun { start, end, style });
                }
                layer.text = Some(crate::text::TextSpec {
                    text,
                    runs,
                    default_style,
                    leading,
                    align,
                    vertical,
                    ..Default::default()
                });
            }
        }
        doc.global_angle = r.i32()?;
        let ns = r.u32()?;
        for _ in 0..ns {
            let sx = r.i32()?;
            let sy = r.i32()?;
            doc.samplers.push((sx, sy));
        }
        if r.u8()? != 0 {
            let name = read_str(&mut r)?;
            let width = r.u32()?;
            let height = r.u32()?;
            let mut px = Vec::with_capacity((width * height) as usize);
            for _ in 0..width * height {
                px.push(read_px(&mut r)?);
            }
            doc.pattern = Some(crate::document::PatternData {
                name,
                width,
                height,
                px,
            });
        }
    }
    // Optional trailing section: Phase 6 additions (v0.6.0).
    if r.pos < payload.len() {
        fn read_px6(r: &mut Reader) -> Result<Pixel16, FormatError> {
            Ok(Pixel16::new(r.u16()?, r.u16()?, r.u16()?, r.u16()?))
        }
        fn read_mode6(r: &mut Reader) -> Result<BlendMode, FormatError> {
            BlendMode::from_id(r.u8()?).ok_or(FormatError::Corrupt("bad blend mode id"))
        }
        fn read_str6(r: &mut Reader) -> Result<String, FormatError> {
            let n = r.u16()? as usize;
            Ok(String::from_utf8_lossy(r.take(n)?).into_owned())
        }
        for layer in &mut doc.layers {
            if r.u8()? != 0 {
                let mut fx = layer.effects.unwrap_or_default();
                fx.satin = crate::effects::SatinSpec {
                    enabled: r.u8()? != 0,
                    mode: read_mode6(&mut r)?,
                    color: read_px6(&mut r)?,
                    opacity: r.f32()?,
                    angle: r.i32()?,
                    distance: r.u32()?,
                    blur: r.u32()?,
                    invert: r.u8()? != 0,
                    contour: r.u8()?,
                };
                fx.color_overlay = crate::effects::ColorOverlaySpec {
                    enabled: r.u8()? != 0,
                    mode: read_mode6(&mut r)?,
                    color: read_px6(&mut r)?,
                    opacity: r.f32()?,
                };
                fx.gradient_overlay = crate::effects::GradientOverlaySpec {
                    enabled: r.u8()? != 0,
                    mode: read_mode6(&mut r)?,
                    opacity: r.f32()?,
                    from: read_px6(&mut r)?,
                    to: read_px6(&mut r)?,
                    style: r.u8()?,
                    angle: r.i32()?,
                    scale: r.u32()?,
                };
                fx.pattern_overlay = crate::effects::PatternOverlaySpec {
                    enabled: r.u8()? != 0,
                    mode: read_mode6(&mut r)?,
                    opacity: r.f32()?,
                    kind: r.u8()?,
                    scale: r.u32()?,
                };
                fx.stroke = crate::effects::StrokeSpec {
                    enabled: r.u8()? != 0,
                    mode: read_mode6(&mut r)?,
                    color: read_px6(&mut r)?,
                    opacity: r.f32()?,
                    size: r.u32()?,
                    position: r.u8()?,
                };
                fx.drop_shadow.contour = r.u8()?;
                fx.inner_shadow.contour = r.u8()?;
                fx.outer_glow.contour = r.u8()?;
                fx.inner_glow.contour = r.u8()?;
                fx.bevel.contour = r.u8()?;
                fx.bevel.texture = r.u8()? != 0;
                fx.bevel.texture_kind = r.u8()?;
                fx.bevel.texture_depth = r.u32()?;
                layer.effects = Some(fx);
            }
            layer.fill_opacity = r.f32()?;
            for v in &mut layer.blend_if {
                *v = r.u8()?;
            }
            layer.knockout = r.u8()?;
            layer.blend_channels = r.u8()?;
            layer.blend_clipped_group = r.u8()? != 0;
            layer.blend_interior_group = r.u8()? != 0;
            layer.transparency_shapes = r.u8()? != 0;
            layer.mask_hides_effects = r.u8()? != 0;
            layer.group = r.u16()?;
            layer.clipped = r.u8()? != 0;
            layer.locks = r.u8()?;
            layer.color = r.u8()?;
            if r.u8()? != 0 {
                let kind = r.u8()?;
                let rect = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
                let sides = r.u8()?;
                let star = r.u8()? != 0;
                let radius = r.u32()?;
                let fill = read_px6(&mut r)?;
                let fill_on = r.u8()? != 0;
                let stroke = read_px6(&mut r)?;
                let stroke_width = r.u32()?;
                let np = r.u32()?;
                let mut points = Vec::new();
                for _ in 0..np {
                    points.push((r.f32()?, r.f32()?));
                }
                layer.shape = Some(crate::shapes::ShapeSpec {
                    kind,
                    rect,
                    sides,
                    star,
                    radius,
                    fill,
                    fill_on,
                    stroke,
                    stroke_width,
                    points,
                    arrow_start: false,
                    arrow_end: false,
                });
            }
            if r.u8()? != 0 {
                layer.fill = Some(crate::shapes::FillSpec {
                    kind: r.u8()?,
                    color: read_px6(&mut r)?,
                    to: read_px6(&mut r)?,
                    angle: r.i32()?,
                    style: r.u8()?,
                    scale: r.u32()?,
                    pattern_kind: r.u8()?,
                });
            }
            if r.u8()? != 0 {
                if let Some(t) = &mut layer.text {
                    t.box_w = r.u32()?;
                    t.box_h = r.u32()?;
                    t.space_before = r.f32()?;
                    t.space_after = r.f32()?;
                    t.first_indent = r.f32()?;
                    t.hyphenate = r.u8()? != 0;
                    t.extras = r.u8()?;
                    let ws = r.u8()?;
                    t.warp_style = if ws == 255 { None } else { Some(ws) };
                    t.warp_bend = r.f32()?;
                    t.warp_h = r.f32()?;
                    t.warp_v = r.f32()?;
                } else {
                    // Text payload without a text layer: skip.
                    let _ = r.u32()?;
                    let _ = r.u32()?;
                    let _ = r.f32()?;
                    let _ = r.f32()?;
                    let _ = r.f32()?;
                    let _ = r.u8()?;
                    let _ = r.u8()?;
                    let _ = r.u8()?;
                    let _ = r.f32()?;
                    let _ = r.f32()?;
                    let _ = r.f32()?;
                }
            }
        }
        doc.global_altitude = r.i32()?;
        let nsets = r.u32()?;
        for _ in 0..nsets {
            let name = read_str6(&mut r)?;
            let opacity = r.f32()?;
            let mid = r.u8()?;
            let mode = if mid == 255 {
                None
            } else {
                Some(BlendMode::from_id(mid).ok_or(FormatError::Corrupt("bad set mode"))?)
            };
            let visible = r.u8()? != 0;
            let color = r.u8()?;
            doc.sets.push(crate::document::LayerSet {
                name,
                opacity,
                mode,
                visible,
                color,
            });
        }
        let nnotes = r.u32()?;
        for _ in 0..nnotes {
            let x = r.i32()?;
            let y = r.i32()?;
            let color = r.u8()?;
            let text = read_str6(&mut r)?;
            doc.notes.push(crate::document::Note {
                x,
                y,
                color,
                text,
                audio: None,
            });
        }
        let nslices = r.u32()?;
        for _ in 0..nslices {
            let x = r.i32()?;
            let y = r.i32()?;
            let w = r.u32()?;
            let h = r.u32()?;
            let name = read_str6(&mut r)?;
            let url = read_str6(&mut r)?;
            let alt = read_str6(&mut r)?;
            doc.slices.push(crate::document::SliceRect {
                x,
                y,
                w,
                h,
                name,
                url,
                alt,
            });
        }
        let ncs = r.u32()?;
        for _ in 0..ncs {
            let name = read_str6(&mut r)?;
            let np = r.u32()?;
            let mut points = Vec::new();
            for _ in 0..np {
                points.push((r.f32()?, r.f32()?));
            }
            doc.custom_shapes
                .push(crate::document::CustomShape { name, points });
        }
        // v0.6.1 trailing: Blend If split widths. Absent in
        // v0.6.0 files — splits stay zero (hard edges).
        if r.pos < r.buf.len() {
            for layer in &mut doc.layers {
                for v in &mut layer.blend_if_split {
                    *v = r.u8()?;
                }
            }
        }
        // v0.6.1 trailing: note audio payloads.
        if r.pos < r.buf.len() {
            for n in &mut doc.notes {
                if r.u8()? != 0 {
                    let len = r.u32()? as usize;
                    n.audio = Some(r.take(len)?.to_vec());
                }
            }
        }
        // Phase 6.5 trailing: color table, duotone spec,
        // multichannel planes.
        if r.pos < r.buf.len() {
            let n = r.u16()? as usize;
            let mut table = Vec::with_capacity(n);
            for _ in 0..n {
                table.push((r.u8()?, r.u8()?, r.u8()?));
            }
            doc.color_table = table;
            if r.u8()? != 0 {
                let ink_count = r.u8()? as usize;
                let mut inks = Vec::with_capacity(ink_count);
                for _ in 0..ink_count {
                    let color = (r.u8()?, r.u8()?, r.u8()?);
                    let curve_bytes = r.take(256)?;
                    let mut curve = [0u8; 256];
                    curve.copy_from_slice(curve_bytes);
                    inks.push(crate::document::DuotoneInk { color, curve });
                }
                doc.duotone = Some(crate::document::DuotoneSpec { inks });
            }
            if r.pos < r.buf.len() {
                for ch in &mut doc.alphas {
                    if r.pos < r.buf.len() {
                        ch.masked = r.u8()? != 0;
                    }
                }
            }
            let planes = r.u8()? as usize;
            for _ in 0..planes {
                let name = read_str6(&mut r)?;
                let color = (r.u8()?, r.u8()?, r.u8()?);
                let len = r.u32()? as usize;
                let mut plane = Vec::with_capacity(len);
                for _ in 0..len {
                    plane.push(r.u16()?);
                }
                doc.multi_channels
                    .push(crate::document::MultiChannelInfo { name, color });
                doc.multi_planes.push(plane);
            }
            // Phase 6.5 Batch 6: shape arrowhead flags.
            if r.pos < r.buf.len() {
                let n = r.u16()? as usize;
                for _ in 0..n {
                    let li = r.u16()? as usize;
                    let flags = r.u8()?;
                    if let Some(sp) = doc.layers.get_mut(li).and_then(|l| l.shape.as_mut()) {
                        sp.arrow_start = flags & 1 != 0;
                        sp.arrow_end = flags & 2 != 0;
                    }
                }
            }
            // Phase 6.5 Batch 6: File Info fields.
            if r.pos < r.buf.len() {
                doc.info_title = read_str6(&mut r)?;
                doc.info_caption = read_str6(&mut r)?;
                doc.info_keywords = read_str6(&mut r)?;
                doc.info_copyright = read_str6(&mut r)?;
                doc.info_credit = read_str6(&mut r)?;
            }
            // Resolution (Preferences wiring): files written
            // before it end at File Info; default stays 72.
            if r.pos < r.buf.len() {
                doc.resolution = r.f32()?;
            }
        }
    }
    Ok(())
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
    tmp.set_extension("pdk.tmp");
    fs::write(&tmp, serialize(doc))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// The standard recovery-file path next to a document path.
pub fn recovery_path_for(doc_path: &Path) -> PathBuf {
    let mut p = doc_path.to_path_buf();
    p.set_extension("pdk.recovery");
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
    fn resolution_round_trips() {
        let mut doc = sample_doc();
        doc.resolution = 300.0;
        let back = deserialize(&serialize(&doc)).unwrap();
        assert_eq!(back.resolution, 300.0);
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
        let path = dir.join("test.pdk");
        let doc = sample_doc();
        save(&doc, &path).unwrap();
        assert_eq!(load(&path).unwrap(), doc);

        let rec = recovery_path_for(&path);
        save_atomic(&doc, &rec).unwrap();
        assert!(rec.exists());
        assert_eq!(load(&rec).unwrap().render_checksum(), doc.render_checksum());
        // The temp staging file must not linger after a successful rename.
        let mut tmp = rec.clone();
        tmp.set_extension("pdk.tmp");
        assert!(!tmp.exists());
        fs::remove_dir_all(&dir).ok();
    }
}
