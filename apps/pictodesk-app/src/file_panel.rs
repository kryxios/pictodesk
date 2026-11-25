//! Open panel: folder navigation, file listing, and the per-purpose file loaders.
use super::*;
use crate::actions::{line_to_step, read_abr, read_aco, RecordedStep};

/// Folder the Open panel starts in: the last folder used, else
/// the user's Pictures folder when it exists, else ".".
pub(crate) fn last_open_dir() -> std::path::PathBuf {
    let saved = pictodesk_dir().join("last-dir.txt");
    if let Ok(text) = std::fs::read_to_string(&saved) {
        let p = std::path::PathBuf::from(text.trim());
        if p.is_dir() {
            return p;
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let pictures = std::path::PathBuf::from(home).join("Pictures");
    if pictures.is_dir() {
        pictures
    } else {
        std::path::PathBuf::from(".")
    }
}

pub(crate) fn save_last_open_dir(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(pictodesk_dir());
    let _ = std::fs::write(
        pictodesk_dir().join("last-dir.txt"),
        dir.display().to_string(),
    );
}

impl App {
    /// Fill the file panel with the openable files in the
    /// working directory (shared by Open and the map/texture
    /// pickers; `file_panel_purpose` says what a click means).
    /// Open panel starts in the last-used folder (or Pictures),
    /// and can move between folders — files no longer have to
    /// sit in the folder the app was launched from.
    pub(crate) fn open_file_panel(&mut self) {
        self.file_panel_dir = last_open_dir();
        self.refresh_file_panel();
    }

    pub(crate) fn refresh_file_panel(&mut self) {
        let dir = self.file_panel_dir.clone();
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p
                    .extension()
                    .map(|e| {
                        let e = e.to_ascii_lowercase();
                        let exts: &[&str] = match self.file_panel_purpose {
                            6 => &["pdmesh"],
                            3 => &["aco"],
                            4 => &["abr"],
                            5 => &["txt"],
                            _ => &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "pdk", "psd"],
                        };
                        exts.contains(&e.to_str().unwrap_or(""))
                    })
                    .unwrap_or(false)
                {
                    files.push(p);
                }
            }
        }
        dirs.sort();
        files.sort();
        self.file_panel_dirs = dirs;
        self.file_panel = Some(files);
    }

    /// Rows shown in the Open panel, in display order: ".."
    /// (parent) first when one exists, then subdirectories,
    /// then files matching the panel's purpose.
    /// (path, is_dir); None path = the ".." row.
    pub(crate) fn file_panel_rows(&self) -> Vec<(Option<PathBuf>, bool)> {
        let mut rows = Vec::new();
        if let Some(parent) = self.file_panel_dir.parent() {
            if parent != self.file_panel_dir {
                rows.push((None, true));
            }
        }
        for d in &self.file_panel_dirs {
            rows.push((Some(d.clone()), true));
        }
        if let Some(files) = &self.file_panel {
            for f in files {
                rows.push((Some(f.clone()), false));
            }
        }
        rows
    }

    /// Load any supported document file without touching the
    /// current document (the Displace / Texture Fill map
    /// route; mirrors open_path's format routing).
    pub(crate) fn load_any_doc(&self, path: &Path) -> Option<Document> {
        if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("psd"))
            .unwrap_or(false)
        {
            let bytes = std::fs::read(path).ok()?;
            pictodesk_core::psd::load_psd(&bytes)
                .ok()
                .map(|(doc, _)| doc)
        } else if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            let bytes = std::fs::read(path).ok()?;
            format::deserialize(&bytes).ok()
        } else {
            io::load_file(path).ok()
        }
    }

    /// A file was picked while the panel was in a map/texture
    /// purpose: build the plane (capped at 1024 px on the long
    /// side so an Action step stays a sane size) and apply the
    /// pending filter.
    pub(crate) fn load_swatches_file(&mut self, path: &Path) {
        match std::fs::read(path).ok().and_then(|b| read_aco(&b)) {
            Some(mut colors) => {
                colors.truncate(72);
                self.palette = colors;
                self.status = format!("Loaded {} swatches", self.palette.len());
            }
            None => self.status = "Could not read that .ACO file".into(),
        }
    }

    pub(crate) fn load_brushes_file(&mut self, path: &Path) {
        match std::fs::read(path).ok().and_then(|b| read_abr(&b)) {
            Some(presets) if !presets.is_empty() => {
                self.brush_presets = presets;
                self.status = format!("Loaded {} brush presets", self.brush_presets.len());
            }
            _ => self.status = "Could not read that brush file".into(),
        }
    }

    pub(crate) fn load_settings_file(&mut self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            self.status = "Could not read that settings file".into();
            return;
        };
        let Some(step) = text.lines().next().and_then(line_to_step) else {
            self.status = "That file holds no adjustment settings".into();
            return;
        };
        self.apply_step_to_dialog(step);
    }

    pub(crate) fn take_map_file(&mut self, path: &Path) {
        let purpose = self.file_panel_purpose;
        let Some(mdoc) = self.load_any_doc(path) else {
            self.status = "Could not load that file as an image".into();
            return;
        };
        let (w, h) = (mdoc.width, mdoc.height);
        if w == 0 || h == 0 {
            self.status = "That image is empty".into();
            return;
        }
        let step = w.max(h).div_ceil(1024);
        let (pw, ph) = ((w / step).max(1), (h / step).max(1));
        if purpose == 1 {
            let (hs, vs, tile, wrap) = self
                .pending_displace
                .take()
                .unwrap_or((50.0, 50.0, false, true));
            let mut plane: Vec<f32> = Vec::with_capacity((pw * ph * 2) as usize);
            for yy in 0..ph as i32 {
                for xx in 0..pw as i32 {
                    let p = mdoc
                        .render_pixel(xx * step as i32, yy * step as i32)
                        .to_f32s();
                    plane.push(p.0);
                    plane.push(p.1);
                }
            }
            let rec = RecordedStep::Displace(plane.clone(), pw, ph, hs, vs, tile, wrap);
            self.filter_now("Displace", Some(rec), |d, l, s| {
                filter::displace(d, l, s, &plane, pw, ph, hs, vs, tile, wrap)
            });
        } else {
            let mut plane: Vec<f32> = Vec::with_capacity((pw * ph) as usize);
            for yy in 0..ph as i32 {
                for xx in 0..pw as i32 {
                    let p = mdoc
                        .render_pixel(xx * step as i32, yy * step as i32)
                        .to_f32s();
                    plane.push(0.299 * p.0 + 0.587 * p.1 + 0.114 * p.2);
                }
            }
            let rec = RecordedStep::TextureFill(plane.clone(), pw, ph);
            self.filter_now("Texture Fill", Some(rec), |d, l, s| {
                filter::texture_fill(d, l, s, &plane, pw, ph)
            });
        }
    }

    pub(crate) fn draw_file_panel(&mut self, f: &mut Frame) {
        let rows = self.file_panel_rows();
        let (ww, wh) = (f.w as i32, f.h as i32);
        let r = Rect::new((ww - 430) / 2, (wh - 320) / 2, 430, 320);
        f.panel_rounded(r, 6);
        f.fill_rounded(
            Rect::new(r.x + 2, r.y + 2, r.w - 4, 18),
            4,
            ui::PLATINUM_LIGHT,
        );
        f.text(r.x + 8, r.y + 7, "Open", 1, ui::WHITE);
        let dir_label = format!("Folder: {}", self.file_panel_dir.display());
        let dir_label = if dir_label.chars().count() > 58 {
            format!(
                "...{}",
                dir_label
                    .chars()
                    .skip(dir_label.chars().count() - 55)
                    .collect::<String>()
            )
        } else {
            dir_label
        };
        f.text(r.x + 8, r.y + 26, &dir_label, 1, ui::TEXT);
        if rows.is_empty() {
            f.text(r.x + 8, r.y + 48, "(nothing here)", 1, ui::TEXT_DIM);
        }
        for (i, (p, is_dir)) in rows.iter().take(14).enumerate() {
            let ir = Rect::new(r.x + 6, r.y + 44 + i as i32 * 17, r.w - 12, 17);
            let label = match p {
                None => "..".to_string(),
                Some(p) if *is_dir => format!(
                    "{}/",
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.display().to_string())
                ),
                Some(p) => p.display().to_string(),
            };
            if self.hover_file == Some(i) {
                f.fill_rect(ir, ui::ACCENT);
                f.text(ir.x + 6, ir.y + 4, &label, 1, ui::WHITE);
            } else {
                f.text(ir.x + 6, ir.y + 4, &label, 1, ui::TEXT);
            }
        }
        f.button(
            Rect::new(r.right() - 92, r.bottom() - 32, 80, 24),
            "Cancel",
            false,
            true,
        );
    }

    pub(crate) fn file_panel_rect(&self) -> Rect {
        Rect::new(
            (self.win_size.0 as i32 - 430) / 2,
            (self.win_size.1 as i32 - 320) / 2,
            430,
            320,
        )
    }
}
