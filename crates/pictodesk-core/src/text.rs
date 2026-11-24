//! Type layers (Phase 5): the text specification a type layer
//! carries so it can be re-edited forever. The spec is pure data —
//! fonts, shaping, and rasterization live in the app, which
//! renders the spec into the layer's tiles whenever it changes.
//! Keeping the spec in core means the document model, the file
//! format, and undo all treat a type layer as data, not pixels.

use crate::pixel::Pixel16;

/// Formatting for a run of characters.
#[derive(Clone, Debug, PartialEq)]
pub struct CharStyle {
    /// Font family name as the OS reports it (e.g. "DejaVu Sans").
    pub font: String,
    /// Style within the family ("Regular", "Bold", "Italic", ...).
    pub style: String,
    /// Point size (document pixels at 72dpi equivalence).
    pub size: f32,
    /// Tracking in 1/1000 em (can be negative).
    pub tracking: f32,
    pub color: Pixel16,
    /// Synthesized bold/italic for fonts that lack the face.
    pub faux_bold: bool,
    pub faux_italic: bool,
    pub underline: bool,
    /// Baseline shift in points (can be negative).
    pub baseline_shift: f32,
    /// Use the font's kerning pairs.
    pub auto_kerning: bool,
}

impl Default for CharStyle {
    fn default() -> Self {
        CharStyle {
            font: String::new(),
            style: "Regular".to_string(),
            size: 36.0,
            tracking: 0.0,
            color: Pixel16::from_rgba8(0, 0, 0, 255),
            faux_bold: false,
            faux_italic: false,
            underline: false,
            baseline_shift: 0.0,
            auto_kerning: true,
        }
    }
}

/// A byte range of the text with one style.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub start: usize,
    pub end: usize,
    pub style: CharStyle,
}

/// Everything a type layer remembers about its text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpec {
    pub text: String,
    /// Style runs over `text` (byte ranges). Empty = the default
    /// style covers everything.
    pub runs: Vec<TextRun>,
    pub default_style: CharStyle,
    /// Line spacing in points; 0 = auto (font's natural leading).
    pub leading: f32,
    /// 0 = left, 1 = center, 2 = right.
    pub align: u8,
    /// Vertical type (columns run top-to-bottom, right-to-left).
    pub vertical: bool,
    // ---- Phase 6 additions.
    /// Paragraph text box (0 = point text, no wrapping).
    pub box_w: u32,
    pub box_h: u32,
    /// Paragraph spacing / first-line indent, points.
    pub space_before: f32,
    pub space_after: f32,
    pub first_indent: f32,
    /// Hyphenate long words at the box edge (simple syllable-
    /// free break with a hyphen, v1).
    pub hyphenate: bool,
    /// Character extras bitmask: 1 Small Caps, 2 Superscript,
    /// 4 Subscript, 8 Strikethrough (layer-wide in v1).
    pub extras: u8,
    /// Warp Text (Phase 6): style index into warp::WARP_STYLES
    /// plus bend / horizontal / vertical distortion percents.
    pub warp_style: Option<u8>,
    pub warp_bend: f32,
    pub warp_h: f32,
    pub warp_v: f32,
}

impl Default for TextSpec {
    fn default() -> Self {
        TextSpec::new("")
    }
}

impl TextSpec {
    pub fn new(text: impl Into<String>) -> Self {
        TextSpec {
            text: text.into(),
            runs: Vec::new(),
            default_style: CharStyle::default(),
            leading: 0.0,
            align: 0,
            vertical: false,
            box_w: 0,
            box_h: 0,
            space_before: 0.0,
            space_after: 0.0,
            first_indent: 0.0,
            hyphenate: false,
            extras: 0,
            warp_style: None,
            warp_bend: 50.0,
            warp_h: 0.0,
            warp_v: 0.0,
        }
    }

    /// The style in force at a byte position.
    pub fn style_at(&self, byte_idx: usize) -> CharStyle {
        for run in &self.runs {
            if byte_idx >= run.start && byte_idx < run.end {
                return run.style.clone();
            }
        }
        self.default_style.clone()
    }

    /// Replace the style over a byte range, splitting and merging
    /// runs as needed. Ranges are clamped to the text length and
    /// snapped to char boundaries by the caller.
    pub fn set_style(&mut self, start: usize, end: usize, style: CharStyle) {
        if start >= end || start >= self.text.len() {
            return;
        }
        let end = end.min(self.text.len());
        // Materialize per-character styles, apply, then re-merge.
        let mut styles: Vec<CharStyle> = Vec::with_capacity(self.text.len());
        for (i, _) in self.text.char_indices() {
            styles.push(self.style_at(i));
        }
        // styles is indexed by char ordinal; convert byte range.
        let chars: Vec<(usize, char)> = self.text.char_indices().collect();
        for (ord, (b, _)) in chars.iter().enumerate() {
            if *b >= start && *b < end {
                styles[ord] = style.clone();
            }
        }
        self.runs.clear();
        let mut cur: Option<(usize, CharStyle)> = None;
        for (ord, (b, _)) in chars.iter().enumerate() {
            match &cur {
                Some((_, s)) if *s == styles[ord] => {}
                _ => {
                    if let Some((cb, cs)) = cur.take() {
                        self.runs.push(TextRun {
                            start: cb,
                            end: *b,
                            style: cs,
                        });
                    }
                    cur = Some((*b, styles[ord].clone()));
                }
            }
        }
        if let Some((cb, cs)) = cur.take() {
            self.runs.push(TextRun {
                start: cb,
                end: self.text.len(),
                style: cs,
            });
        }
        // If every run matches the default, collapse to no runs.
        if self.runs.iter().all(|r| r.style == self.default_style) {
            self.runs.clear();
        }
    }
}
