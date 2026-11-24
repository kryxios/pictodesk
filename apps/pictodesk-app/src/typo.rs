//! Type engine (Phase 5): system-font enumeration, shaping with
//! rustybuzz, and rasterization with fontdue into a type layer's
//! tiles. The document stores only the `TextSpec`; this module
//! turns a spec back into pixels whenever it changes.
//!
//! Layout convention: the layer's `offset` is the text block's
//! origin (top-left of the first line box). All drawing happens
//! in document coordinates, so the layer machinery's offset
//! adjustment stores the glyphs in layer-local tiles and the
//! Move tool carries the text with the layer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use pictodesk_core::document::Document;
use pictodesk_core::pixel::Pixel16;
use pictodesk_core::text::{CharStyle, TextSpec};

/// One installed font face.
#[derive(Clone, Debug)]
pub struct FontEntry {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
    pub index: u32,
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/Library/Fonts")));
        }
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(format!("{local}\\Microsoft\\Windows\\Fonts")));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/.fonts")));
            dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
        }
    }
    dirs
}

fn scan_dir(dir: &PathBuf, depth: u32, out: &mut Vec<PathBuf>) {
    if depth > 3 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_dir(&p, depth + 1, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc") {
                out.push(p);
            }
        }
    }
}

/// All installed faces, scanned once and cached.
pub fn fonts() -> &'static Vec<FontEntry> {
    static FONTS: OnceLock<Vec<FontEntry>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut files = Vec::new();
        for d in font_dirs() {
            scan_dir(&d, 0, &mut files);
        }
        files.sort();
        let mut out = Vec::new();
        for path in files {
            let Ok(data) = std::fs::read(&path) else {
                continue;
            };
            let count = rustybuzz::ttf_parser::fonts_in_collection(&data).unwrap_or(1);
            for index in 0..count {
                let Some(face) = rustybuzz::ttf_parser::Face::parse(&data, index).ok() else {
                    continue;
                };
                let mut family = None;
                let mut style = None;
                for name in face.names() {
                    // Typographic family/subfamily win over the
                    // legacy pair when present.
                    match name.name_id {
                        16 => family = name.to_string().or(family.take()),
                        17 => style = name.to_string().or(style.take()),
                        1 if family.is_none() => family = name.to_string(),
                        2 if style.is_none() => style = name.to_string(),
                        _ => {}
                    }
                }
                let Some(family) = family else { continue };
                out.push(FontEntry {
                    family,
                    style: style.unwrap_or_else(|| "Regular".to_string()),
                    path: path.clone(),
                    index,
                });
            }
        }
        out.sort_by(|a, b| {
            a.family
                .cmp(&b.family)
                .then_with(|| a.style.cmp(&b.style))
                .then_with(|| a.index.cmp(&b.index))
        });
        out.dedup_by(|a, b| a.family == b.family && a.style == b.style && a.path == b.path);
        out
    })
}

/// Distinct family names, sorted.
pub fn families() -> Vec<String> {
    let mut fams: Vec<String> = fonts().iter().map(|f| f.family.clone()).collect();
    fams.dedup();
    fams
}

/// Styles available within a family.
pub fn styles_of(family: &str) -> Vec<String> {
    let mut out: Vec<String> = fonts()
        .iter()
        .filter(|f| f.family == family)
        .map(|f| f.style.clone())
        .collect();
    out.dedup();
    out
}

fn find_entry(family: &str, style: &str) -> Option<&'static FontEntry> {
    let all = fonts();
    all.iter()
        .find(|f| f.family == family && f.style == style)
        .or_else(|| all.iter().find(|f| f.family == family))
        .or_else(|| all.first())
}

/// Loaded faces for one font file, cached by path.
struct LoadedFont {
    due: fontdue::Font,
    buzz: rustybuzz::Face<'static>,
    units_per_em: f32,
    ascent: f32,  // in em units
    descent: f32, // in em units (negative)
}

fn load_font(entry: &FontEntry) -> Option<&'static LoadedFont> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<PathBuf, &'static LoadedFont>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Some(f) = cache.lock().unwrap().get(&entry.path) {
        return Some(f);
    }
    let data = std::fs::read(&entry.path).ok()?;
    let due = fontdue::Font::from_bytes(data.clone(), fontdue::FontSettings::default()).ok()?;
    // Leak the bytes so the rustybuzz face can borrow them for the
    // life of the process (fonts are never unloaded).
    let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
    let buzz = rustybuzz::Face::from_slice(leaked, entry.index)?;
    let ttf = rustybuzz::ttf_parser::Face::parse(leaked, entry.index).ok()?;
    let loaded: &'static LoadedFont = Box::leak(Box::new(LoadedFont {
        due,
        buzz,
        units_per_em: ttf.units_per_em() as f32,
        ascent: ttf.ascender() as f32,
        descent: ttf.descender() as f32,
    }));
    cache.lock().unwrap().insert(entry.path.clone(), loaded);
    Some(loaded)
}

/// One shaped glyph ready to blit.
struct PlacedGlyph {
    font: &'static LoadedFont,
    glyph_id: u16,
    style: CharStyle,
    /// Pen position (document coords, baseline-relative origin).
    x: f32,
    y_baseline: f32,
}

/// Shape one segment (one style, one line) and return its glyphs
/// plus the total advance in pixels.
fn shape_segment(
    text: &str,
    style: &CharStyle,
    origin_x: f32,
    baseline: f32,
    out: &mut Vec<PlacedGlyph>,
) -> f32 {
    let Some(entry) = find_entry(&style.font, &style.style) else {
        return 0.0;
    };
    let Some(font) = load_font(entry) else {
        return 0.0;
    };
    let scale = style.size / font.units_per_em;
    let tracking_px = style.tracking / 1000.0 * style.size;
    let mut pen = origin_x;
    if !style.auto_kerning {
        // No shaping: one glyph per char, raw advances, no kern
        // pairs and no ligatures (what "kerning off" means).
        for ch in text.chars() {
            let glyph_id = font.due.lookup_glyph_index(ch);
            let adv = font.due.metrics_indexed(glyph_id, style.size).advance_width;
            out.push(PlacedGlyph {
                font,
                glyph_id,
                style: style.clone(),
                x: pen,
                y_baseline: baseline + style.baseline_shift,
            });
            pen += adv + tracking_px;
        }
        return pen - origin_x;
    }
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let shaped = rustybuzz::shape(&font.buzz, &[], buffer);
    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();
    for (info, pos) in infos.iter().zip(positions.iter()) {
        out.push(PlacedGlyph {
            font,
            glyph_id: info.glyph_id as u16,
            style: style.clone(),
            x: pen + pos.x_offset as f32 * scale,
            y_baseline: baseline - pos.y_offset as f32 * scale + style.baseline_shift,
        });
        pen += pos.x_advance as f32 * scale + tracking_px;
    }
    pen - origin_x
}

/// Split a line into (byte range, style) segments.
fn segments(spec: &TextSpec, line: &str, line_start: usize) -> Vec<(usize, usize, CharStyle)> {
    let mut out = Vec::new();
    if line.is_empty() {
        return out;
    }
    let mut cur_start = 0usize;
    let mut cur_style = spec.style_at(line_start);
    for (i, _) in line.char_indices().skip(1) {
        let st = spec.style_at(line_start + i);
        if st != cur_style {
            out.push((cur_start, i, cur_style));
            cur_start = i;
            cur_style = st;
        }
    }
    out.push((cur_start, line.len(), cur_style));
    out
}

/// Measure one line's advance width in pixels.
fn measure_line(spec: &TextSpec, line: &str, line_start: usize) -> f32 {
    let mut scratch = Vec::new();
    let mut w = 0.0;
    for (s, e, style) in segments(spec, line, line_start) {
        w += shape_segment(&line[s..e], &style, 0.0, 0.0, &mut scratch);
    }
    w
}

/// Blit one glyph's coverage into the layer (document coords).
#[allow(clippy::too_many_arguments)]
fn blit_glyph(
    doc: &mut Document,
    layer: usize,
    g: &PlacedGlyph,
    out: &mut Vec<(i32, i32, i32, i32)>,
) {
    let style = &g.style;
    let passes: &[f32] = if style.faux_bold {
        &[0.0, (style.size * 0.03).max(1.0)]
    } else {
        &[0.0]
    };
    for &dx in passes {
        let (metrics, coverage) = g.font.due.rasterize_indexed(g.glyph_id, style.size);
        if metrics.width == 0 || metrics.height == 0 {
            continue;
        }
        let gx = (g.x + metrics.xmin as f32 + dx).round() as i32;
        // fontdue ymin is the distance from the baseline down to
        // the glyph's bottom edge.
        let gy = (g.y_baseline - metrics.ymin as f32 - metrics.height as f32).round() as i32;
        for row in 0..metrics.height {
            let cov_row = &coverage[row * metrics.width..(row + 1) * metrics.width];
            // Faux italic: rows lean right toward the top.
            let shear = if style.faux_italic {
                ((metrics.height - row) as f32 * 0.21) as i32
            } else {
                0
            };
            for (col, &cov) in cov_row.iter().enumerate() {
                if cov == 0 {
                    continue;
                }
                let (xx, yy) = (gx + col as i32 + shear, gy + row as i32);
                let a = (style.color.a as f32 / 65535.0) * (cov as f32 / 255.0);
                if a <= 0.0 {
                    continue;
                }
                let mut px = style.color;
                px.a = ((style.color.a as f32) * (cov as f32 / 255.0)) as u16;
                let cur = doc.layers[layer].get_pixel(xx, yy);
                doc.layers[layer].set_pixel(xx, yy, over(cur, px));
                out.push((xx, yy, xx + 1, yy + 1));
            }
        }
    }
}

/// Source-over composite of `top` onto `bot` (both straight alpha).
fn over(bot: Pixel16, top: Pixel16) -> Pixel16 {
    let ta = top.a as f32 / 65535.0;
    let ba = bot.a as f32 / 65535.0;
    let oa = ta + ba * (1.0 - ta);
    if oa <= 0.0 {
        return Pixel16::from_rgba8(0, 0, 0, 0);
    }
    let mix = |t: u16, b: u16| -> u16 {
        let tf = t as f32 / 65535.0;
        let bf = b as f32 / 65535.0;
        (((tf * ta + bf * ba * (1.0 - ta)) / oa) * 65535.0).round() as u16
    };
    Pixel16 {
        r: mix(top.r, bot.r),
        g: mix(top.g, bot.g),
        b: mix(top.b, bot.b),
        a: (oa * 65535.0).round() as u16,
    }
}

/// Re-render a type layer from its spec. Clears the layer's tiles
/// first (a type layer holds nothing but its text), then lays out
/// and blits every glyph. No-op when the layer has no spec.
pub fn render_type_layer(doc: &mut Document, layer: usize) {
    let Some(spec) = doc.layers[layer].text.clone() else {
        return;
    };
    doc.layers[layer].tiles.clear();
    doc.layers[layer].cmyk_alpha.clear();
    let (ox, oy) = doc.layers[layer].offset;
    let mut placed: Vec<PlacedGlyph> = Vec::new();
    let mut scratch: Vec<(i32, i32, i32, i32)> = Vec::new();

    if spec.vertical {
        render_vertical(doc, layer, &spec, ox, oy, &mut placed);
    } else {
        render_horizontal(doc, layer, &spec, ox, oy, &mut placed);
    }
    for g in &placed {
        blit_glyph(doc, layer, g, &mut scratch);
    }
    // Underlines: one rule per underlined segment per line.
    draw_underlines(doc, layer, &spec, ox, oy);
}

fn line_height(spec: &TextSpec) -> f32 {
    if spec.leading > 0.0 {
        spec.leading
    } else {
        spec.default_style.size * 1.2
    }
}

fn render_horizontal(
    doc: &mut Document,
    layer: usize,
    spec: &TextSpec,
    ox: i32,
    oy: i32,
    placed: &mut Vec<PlacedGlyph>,
) {
    let lh = line_height(spec);
    let mut byte_pos = 0usize;
    for (li, line) in spec.text.split('\n').enumerate() {
        // Metrics from the line's first style (the default when
        // the line is empty).
        let first_style = if line.is_empty() {
            spec.default_style.clone()
        } else {
            spec.style_at(byte_pos)
        };
        let font = find_entry(&first_style.font, &first_style.style).and_then(load_font);
        let (ascent, _descent) = match font {
            Some(f) => (
                f.ascent * (first_style.size / f.units_per_em),
                f.descent * (first_style.size / f.units_per_em),
            ),
            None => (first_style.size * 0.8, first_style.size * -0.2),
        };
        let baseline = oy as f32 + li as f32 * lh + ascent;
        let width = measure_line(spec, line, byte_pos);
        let start_x = match spec.align {
            1 => ox as f32 - width / 2.0,
            2 => ox as f32 - width,
            _ => ox as f32,
        };
        let mut pen = start_x;
        for (s, e, style) in segments(spec, line, byte_pos) {
            pen += shape_segment(&line[s..e], &style, pen, baseline, placed);
        }
        byte_pos += line.len() + 1;
        let _ = layer;
        let _ = doc;
    }
}

fn render_vertical(
    doc: &mut Document,
    layer: usize,
    spec: &TextSpec,
    ox: i32,
    oy: i32,
    placed: &mut Vec<PlacedGlyph>,
) {
    // Columns run top-to-bottom; each '\n' starts a new column to
    // the LEFT of the previous one. Glyphs stay upright.
    let col_w = line_height(spec);
    let mut byte_pos = 0usize;
    for (ci, column) in spec.text.split('\n').enumerate() {
        let col_x = ox as f32 - ci as f32 * col_w;
        let mut y = oy as f32;
        for (bi, ch) in column.char_indices() {
            let style = spec.style_at(byte_pos + bi);
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            let adv = shape_segment(s, &style, col_x - style.size / 2.0, y + style.size, placed);
            let _ = adv;
            y += style.size * 1.2;
        }
        byte_pos += column.len() + 1;
    }
    let _ = layer;
    let _ = doc;
}

fn draw_underlines(doc: &mut Document, layer: usize, spec: &TextSpec, ox: i32, oy: i32) {
    if spec.vertical {
        return; // vertical underlines (side rules) are a later refinement
    }
    let lh = line_height(spec);
    let mut byte_pos = 0usize;
    for (li, line) in spec.text.split('\n').enumerate() {
        let first_style = if line.is_empty() {
            spec.default_style.clone()
        } else {
            spec.style_at(byte_pos)
        };
        let font = find_entry(&first_style.font, &first_style.style).and_then(load_font);
        let ascent = match font {
            Some(f) => f.ascent * (first_style.size / f.units_per_em),
            None => first_style.size * 0.8,
        };
        let baseline = oy as f32 + li as f32 * lh + ascent;
        let width = measure_line(spec, line, byte_pos);
        let mut pen = match spec.align {
            1 => ox as f32 - width / 2.0,
            2 => ox as f32 - width,
            _ => ox as f32,
        };
        for (s, e, style) in segments(spec, line, byte_pos) {
            let mut scratch = Vec::new();
            let w = shape_segment(&line[s..e], &style, 0.0, 0.0, &mut scratch);
            if style.underline && w > 0.0 {
                let t = (style.size * 0.05).max(1.0);
                let y0 = (baseline + style.size * 0.12).round() as i32;
                for yy in y0..y0 + t as i32 {
                    for xx in pen.round() as i32..(pen + w).round() as i32 {
                        let cur = doc.layers[layer].get_pixel(xx, yy);
                        doc.layers[layer].set_pixel(xx, yy, over(cur, style.color));
                    }
                }
            }
            pen += w;
        }
        byte_pos += line.len() + 1;
    }
}
