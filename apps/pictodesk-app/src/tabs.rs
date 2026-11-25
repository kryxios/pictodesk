//! Document tabs: each tab keeps its own document, history, selection and viewport.
use super::*;
use crate::actions::FadeSource;
use crate::dialogs::{Dialog, DialogKind};
use crate::panels::{HistRow, HistSource};
use crate::tools::{ChannelView, DragMode, PaintTarget};

/// One open document, as stashed while another tab is active.
/// The live fields on App are the active tab's working copy;
/// switching tabs swaps them with the stored one (mem::swap),
/// so every per-document value — history, selection, zoom,
/// render cache, autosave bookkeeping — travels with its doc.
/// Tool/palette/panel state stays global on App.
pub(crate) struct DocTab {
    pub(crate) doc: Document,
    pub(crate) hist: History,
    pub(crate) sel: Option<Selection>,
    pub(crate) clone_src: Option<(f32, f32)>,
    pub(crate) heal_src: Option<(f32, f32)>,
    pub(crate) viewport: Viewport,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) name: String,
    pub(crate) dirty: bool,
    pub(crate) status: String,
    pub(crate) last_label: Option<String>,
    pub(crate) active_path: Option<PathRef>,
    pub(crate) pen_anchor: Option<(usize, usize)>,
    pub(crate) snapshots: Vec<Snapshot>,
    pub(crate) hist_scroll: usize,
    pub(crate) hist_selected: Option<HistRow>,
    pub(crate) hist_source: HistSource,
    pub(crate) hist_src_cache: Option<(HistSource, usize, usize, Document)>,
    pub(crate) measure: Option<((f32, f32), (f32, f32))>,
    pub(crate) fade_source: Option<FadeSource>,
    pub(crate) split_docs: Vec<(String, pictodesk_core::document::Document)>,
    pub(crate) quick_mask: bool,
    pub(crate) autosave_at: std::time::Instant,
    pub(crate) autosaved_version: u64,
    pub(crate) recovery_file: Option<PathBuf>,
    pub(crate) doc_version: u64,
    pub(crate) cached_render: Vec<u8>,
    pub(crate) cached_size: (u32, u32),
    pub(crate) rendered_version: u64,
    pub(crate) last_ft: Option<LastFt>,
    pub(crate) layers_scroll: usize,
    pub(crate) channel_view: ChannelView,
    pub(crate) paint_target: PaintTarget,
    pub(crate) slice_selected: Option<usize>,
}

impl DocTab {
    /// Placeholder values used while a tab's real state lives
    /// in the App's live fields (swap target only — a placeholder
    /// tab is never drawn or saved).
    pub(crate) fn placeholder() -> Self {
        DocTab {
            doc: Document::new(1, 1),
            hist: History::new(),
            sel: None,
            clone_src: None,
            heal_src: None,
            viewport: Viewport {
                scale: 1.0,
                offset: (0.0, 0.0),
            },
            file_path: None,
            name: String::new(),
            dirty: false,
            status: String::new(),
            last_label: None,
            active_path: None,
            pen_anchor: None,
            snapshots: Vec::new(),
            hist_scroll: 0,
            hist_selected: None,
            hist_source: HistSource::State(0),
            hist_src_cache: None,
            measure: None,
            fade_source: None,
            split_docs: Vec::new(),
            quick_mask: false,
            autosave_at: std::time::Instant::now(),
            autosaved_version: 0,
            recovery_file: None,
            doc_version: 0,
            cached_render: Vec::new(),
            cached_size: (0, 0),
            rendered_version: u64::MAX,
            last_ft: None,
            layers_scroll: 0,
            channel_view: ChannelView::Composite,
            paint_target: PaintTarget::Image,
            slice_selected: None,
        }
    }
}

impl App {
    // ---- document tabs ----
    pub(crate) fn tab_count(&self) -> usize {
        self.tabs.len() + 1
    }

    pub(crate) fn tab_name(&self, bar: usize) -> &str {
        if bar == self.active_tab {
            &self.doc_name
        } else if bar < self.active_tab {
            &self.tabs[bar].name
        } else {
            &self.tabs[bar - 1].name
        }
    }

    pub(crate) fn tab_dirty(&self, bar: usize) -> bool {
        if bar == self.active_tab {
            self.dirty
        } else if bar < self.active_tab {
            self.tabs[bar].dirty
        } else {
            self.tabs[bar - 1].dirty
        }
    }

    /// Swap every per-document live field with a DocTab.
    /// After the swap the tab holds the real state and the App
    /// holds the tab's (placeholder) values, or the reverse.
    pub(crate) fn swap_with_tab(&mut self, tab: &mut DocTab) {
        std::mem::swap(&mut self.doc, &mut tab.doc);
        std::mem::swap(&mut self.hist, &mut tab.hist);
        std::mem::swap(&mut self.sel, &mut tab.sel);
        std::mem::swap(&mut self.clone_src, &mut tab.clone_src);
        std::mem::swap(&mut self.heal_src, &mut tab.heal_src);
        std::mem::swap(&mut self.viewport, &mut tab.viewport);
        std::mem::swap(&mut self.file_path, &mut tab.file_path);
        std::mem::swap(&mut self.doc_name, &mut tab.name);
        std::mem::swap(&mut self.dirty, &mut tab.dirty);
        std::mem::swap(&mut self.status, &mut tab.status);
        std::mem::swap(&mut self.last_label, &mut tab.last_label);
        std::mem::swap(&mut self.active_path, &mut tab.active_path);
        std::mem::swap(&mut self.pen_anchor, &mut tab.pen_anchor);
        std::mem::swap(&mut self.snapshots, &mut tab.snapshots);
        std::mem::swap(&mut self.hist_scroll, &mut tab.hist_scroll);
        std::mem::swap(&mut self.hist_selected, &mut tab.hist_selected);
        std::mem::swap(&mut self.hist_source, &mut tab.hist_source);
        std::mem::swap(&mut self.hist_src_cache, &mut tab.hist_src_cache);
        std::mem::swap(&mut self.measure, &mut tab.measure);
        std::mem::swap(&mut self.fade_source, &mut tab.fade_source);
        std::mem::swap(&mut self.split_docs, &mut tab.split_docs);
        std::mem::swap(&mut self.quick_mask, &mut tab.quick_mask);
        std::mem::swap(&mut self.autosave_at, &mut tab.autosave_at);
        std::mem::swap(&mut self.autosaved_version, &mut tab.autosaved_version);
        std::mem::swap(&mut self.recovery_file, &mut tab.recovery_file);
        std::mem::swap(&mut self.doc_version, &mut tab.doc_version);
        std::mem::swap(&mut self.cached_render, &mut tab.cached_render);
        std::mem::swap(&mut self.cached_size, &mut tab.cached_size);
        std::mem::swap(&mut self.rendered_version, &mut tab.rendered_version);
        std::mem::swap(&mut self.last_ft, &mut tab.last_ft);
        std::mem::swap(&mut self.layers_scroll, &mut tab.layers_scroll);
        std::mem::swap(&mut self.channel_view, &mut tab.channel_view);
        std::mem::swap(&mut self.paint_target, &mut tab.paint_target);
        std::mem::swap(&mut self.slice_selected, &mut tab.slice_selected);
    }

    /// Take the active tab's state out of the live fields,
    /// leaving placeholders behind.
    pub(crate) fn take_active_tab(&mut self) -> DocTab {
        let mut tab = DocTab::placeholder();
        self.swap_with_tab(&mut tab);
        tab
    }

    /// Install a stored tab into the live fields.
    pub(crate) fn install_tab(&mut self, tab: DocTab) {
        let mut tab = tab;
        self.swap_with_tab(&mut tab);
        self.reset_transient_for_switch();
    }

    /// A tab switch cancels any in-progress gesture on the old
    /// document: no stroke, drag, transform, or path edit
    /// survives the switch.
    pub(crate) fn reset_transient_for_switch(&mut self) {
        self.mouse_down = false;
        self.drag = DragMode::None;
        self.ft = None;
        self.persp_crop = None;
        self.persp_drag = None;
        self.guide_drag = None;
        self.shape_drag_layer = None;
        self.stroke = None;
        self.lasso_points.clear();
        self.poly_points.clear();
        self.mag_cache = None;
        self.pen_free.clear();
        self.move_orig_offset = (0, 0);
        self.move_linked.clear();
        self.pen_hit = None;
        self.path_before = None;
        self.alpha_cmd = None;
        self.panel_drag = None;
        self.panel_pending = None;
        self.panel_drop = None;
        self.context_menu = None;
        self.open_menu = None;
        self.open_submenu = None;
        self.hover_file = None;
    }

    pub(crate) fn switch_tab(&mut self, bar: usize) {
        if bar == self.active_tab || bar >= self.tab_count() {
            return;
        }
        if self.dialog.is_some() || self.file_panel.is_some() {
            return;
        }
        let cur = self.take_active_tab();
        self.tabs.insert(self.active_tab, cur);
        let tab = self.tabs.remove(bar);
        self.active_tab = bar;
        self.install_tab(tab);
    }

    pub(crate) fn next_tab(&mut self) {
        let n = self.tab_count();
        if n > 1 {
            self.switch_tab((self.active_tab + 1) % n);
        }
    }

    pub(crate) fn prev_tab(&mut self) {
        let n = self.tab_count();
        if n > 1 {
            self.switch_tab((self.active_tab + n - 1) % n);
        }
    }

    /// Open a stored tab at the end of the bar and make it active.
    pub(crate) fn open_tab(&mut self, tab: DocTab) {
        let cur = self.take_active_tab();
        self.tabs.insert(self.active_tab, cur);
        self.active_tab = self.tabs.len();
        self.install_tab(tab);
        self.reset_hist_for_new_doc();
        self.fit_view(self.win_size.0, self.win_size.1);
    }

    /// Build a fresh tab around a document and open it.
    pub(crate) fn new_tab_with_doc(&mut self, doc: Document, path: Option<PathBuf>, name: String) {
        let mut tab = DocTab::placeholder();
        tab.doc = doc;
        tab.file_path = path;
        tab.name = name;
        tab.hist.set_limit(Some(20));
        tab.active_path = if tab.doc.work_path.is_some() {
            Some(PathRef::Work)
        } else {
            None
        };
        tab.doc_version = 1;
        self.open_tab(tab);
    }

    /// Close the tab at a bar position, asking first when it
    /// has unsaved changes.
    pub(crate) fn close_tab(&mut self, bar: usize) {
        if bar >= self.tab_count() || self.dialog.is_some() || self.file_panel.is_some() {
            return;
        }
        if self.tab_dirty(bar) {
            let name = self.tab_name(bar).to_string();
            self.pending_close = Some(bar);
            let mut d = Dialog::new(DialogKind::ConfirmClose, &self.doc);
            d.text = format!("'{name}' has unsaved changes.");
            self.dialog = Some(d);
            return;
        }
        self.close_tab_force(bar);
    }

    pub(crate) fn close_tab_force(&mut self, bar: usize) {
        if bar >= self.tab_count() {
            return;
        }
        if bar == self.active_tab {
            let _ = self.take_active_tab();
            if self.tabs.is_empty() {
                let tab = DocTab::placeholder();
                self.active_tab = 0;
                let mut tab = tab;
                tab.name = "Untitled".into();
                self.install_tab(tab);
                self.doc.log_event("Created");
                self.fit_view(self.win_size.0, self.win_size.1);
            } else {
                let at = if bar < self.tabs.len() { bar } else { bar - 1 };
                let tab = self.tabs.remove(at);
                self.active_tab = at;
                self.install_tab(tab);
            }
        } else {
            let idx = if bar < self.active_tab { bar } else { bar - 1 };
            self.tabs.remove(idx);
            if bar < self.active_tab {
                self.active_tab -= 1;
            }
        }
    }

    /// Load a document file without touching any tab (the
    /// format routing mirrors the old open_path).
    pub(crate) fn load_doc_from_path(
        &self,
        path: &Path,
    ) -> Result<(Document, Option<String>), String> {
        let mut psd_note: Option<String> = None;
        let result = if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("psd"))
            .unwrap_or(false)
        {
            match std::fs::read(path) {
                Ok(bytes) => match pictodesk_core::psd::load_psd(&bytes) {
                    Ok((doc, report)) => {
                        let mut note = format!("PSD imported: {} layer(s)", report.layers_imported);
                        if !report.notes.is_empty() {
                            note.push_str(" — ");
                            note.push_str(&report.notes.join("; "));
                        }
                        psd_note = Some(note);
                        Ok(doc)
                    }
                    Err(e) => Err(e),
                },
                Err(e) => Err(e.to_string()),
            }
        } else if path.extension().map(|e| e == "pdk").unwrap_or(false) {
            std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| format::deserialize(&bytes).map_err(|e| e.to_string()))
        } else {
            io::load_file(path).map_err(|e| e.to_string())
        };
        result.map(|doc| (doc, psd_note))
    }

    /// Replace the ACTIVE tab's document with a file's contents
    /// (File > Revert, and the launch path that opens the file
    /// the app was started with).
    pub(crate) fn replace_doc_from_path(&mut self, path: &Path) {
        match self.load_doc_from_path(path) {
            Ok((doc, psd_note)) => {
                self.doc = doc;
                self.doc.log_event("Opened");
                self.hist = History::new();
                self.hist.set_limit(Some(20));
                self.reset_hist_for_new_doc();
                self.last_label = None;
                self.sel = None;
                self.channel_view = ChannelView::Composite;
                self.paint_target = PaintTarget::Image;
                self.active_path = if self.doc.work_path.is_some() {
                    Some(PathRef::Work)
                } else {
                    None
                };
                self.file_path = Some(path.to_path_buf());
                self.doc_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                self.dirty = false;
                self.doc_version += 1;
                if let Some(old) = self.recovery_file.take() {
                    let _ = std::fs::remove_file(old);
                }
                self.autosaved_version = self.doc_version;
                self.autosave_at = std::time::Instant::now();
                self.fit_view(self.win_size.0, self.win_size.1);
                self.status = psd_note.unwrap_or_else(|| format!("Opened {}", path.display()));
            }
            Err(e) => {
                self.status = format!("Open failed: {e}");
            }
        }
    }

    // ---- tab bar ----
    pub(crate) fn tab_bar_rect(&self) -> Rect {
        let dock = if self.dock_open() && self.display_mode == 0 {
            DOCK_W
        } else {
            0
        };
        Rect::new(
            ui::TOOLBAR_W,
            ui::MENU_H + ui::OPTIONS_H,
            self.win_size.0 as i32 - ui::TOOLBAR_W - dock,
            TAB_H,
        )
    }

    /// (tab rect, close-button rect) per bar position.
    pub(crate) fn tab_rects(&self) -> Vec<(Rect, Rect)> {
        let bar = self.tab_bar_rect();
        let mut out = Vec::new();
        let mut x = bar.x;
        for i in 0..self.tab_count() {
            let label = if self.tab_dirty(i) {
                format!("{} \u{2022}", self.tab_name(i))
            } else {
                self.tab_name(i).to_string()
            };
            let w = (Frame::text_width(&label, 1) + 36).max(56);
            let r = Rect::new(x, bar.y, w, bar.h);
            let close = Rect::new(r.right() - 18, r.y + 6, 12, 12);
            out.push((r, close));
            x = r.right() + 2;
        }
        out
    }

    pub(crate) fn draw_tab_bar(&mut self, f: &mut Frame) {
        // One document, no tabs: the bar stays hidden until a
        // second document opens.
        if self.tab_count() <= 1 {
            return;
        }
        let bar = self.tab_bar_rect();
        f.fill_rect(bar, ui::CANVAS_SURROUND);
        let rects = self.tab_rects();
        for (i, (r, close)) in rects.iter().enumerate() {
            let active = i == self.active_tab;
            f.fill_rect(
                *r,
                if active {
                    ui::PLATINUM_LIGHT
                } else {
                    ui::PLATINUM
                },
            );
            f.fill_rect(Rect::new(r.x, r.bottom() - 1, r.w, 1), ui::EDGE);
            let label = if self.tab_dirty(i) {
                format!("{} \u{2022}", self.tab_name(i))
            } else {
                self.tab_name(i).to_string()
            };
            f.text(
                r.x + 8,
                r.y + 6,
                &label,
                1,
                if active { ui::TEXT } else { ui::TEXT_DIM },
            );
            f.text(close.x + 3, close.y, "x", 1, ui::TEXT_DIM);
        }
        // "+" opens a blank document in a new tab.
        let last = rects.last().map(|(r, _)| r.right() + 2).unwrap_or(bar.x);
        f.fill_rect(Rect::new(last, bar.y + 2, 20, 20), ui::PLATINUM);
        f.text(last + 6, bar.y + 6, "+", 1, ui::TEXT_DIM);
    }

    /// Click on the tab bar: switch tabs, close one, or open a
    /// blank document. Returns true when the click was handled.
    pub(crate) fn tab_bar_mouse(&mut self, mx: i32, my: i32) -> bool {
        if self.display_mode != 0 || self.tab_count() <= 1 {
            return false;
        }
        let bar = self.tab_bar_rect();
        if !bar.contains(mx, my) {
            return false;
        }
        let rects = self.tab_rects();
        for (i, (r, close)) in rects.iter().enumerate() {
            if close.contains(mx, my) {
                self.close_tab(i);
                return true;
            }
            if r.contains(mx, my) {
                self.switch_tab(i);
                return true;
            }
        }
        let last = rects.last().map(|(r, _)| r.right() + 2).unwrap_or(bar.x);
        if Rect::new(last, bar.y + 2, 20, 20).contains(mx, my) {
            let mut doc = Document::new(640, 400);
            doc.working_space = self.default_space;
            doc.layers[0].fill_rect(0, 0, 640, 400, self.bg);
            doc.log_event("Created");
            self.new_tab_with_doc(doc, None, "Untitled".into());
            return true;
        }
        true
    }

    /// Open a file in a NEW tab (File > Open, File Browser,
    /// drag-and-drop). The current document stays open.
    pub(crate) fn open_path(&mut self, path: &Path) {
        match self.load_doc_from_path(path) {
            Ok((doc, psd_note)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                self.new_tab_with_doc(doc, Some(path.to_path_buf()), name);
                self.status = psd_note.unwrap_or_else(|| format!("Opened {}", path.display()));
            }
            Err(e) => {
                self.status = format!("Open failed: {e}");
            }
        }
    }
}
