//! Input: mouse, keyboard, wheel, and the context menu.
use super::*;
use crate::actions::{
    combo_for_key, default_shortcuts, dialog_candidate, save_tool_presets, Action,
};
use crate::dialogs::{opt_act_tooltip, Dialog, DialogKind};
use crate::file_panel::save_last_open_dir;
use crate::panels::{PanelDrop, PanelKind, PanelRow};
use crate::runner::SpecialKey;
use crate::tools::{ChannelView, DragMode, PaintTarget, Tool};

/// A right-click context menu (Phase 4): items are (label, action,
/// enabled), drawn at (x, y) like a menu-bar dropdown.
pub(crate) struct ContextMenu {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) items: Vec<(String, Action, bool)>,
}

impl App {
    /// Right-click (Phase 4): open a context menu suited to where
    /// the click landed — layer operations over the Layers panel,
    /// editing operations over the canvas.
    pub(crate) fn right_click(&mut self, x: f32, y: f32) {
        let (mx, my) = (x as i32, y as i32);
        if self.dialog.is_some() || self.file_panel.is_some() {
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            return;
        }
        // Over a layer row?
        if let Some(r) = self.group_rect(PanelKind::Cp) {
            if self.cp_tab == 0 && r.contains(mx, my) {
                let rows = self.panel_rows();
                for row in 0..(rows.len() - self.layers_scroll).min(8) {
                    let rr = Self::layer_row_rect(r, row, self.layer_row_h());
                    if rr.contains(mx, my) {
                        let PanelRow::Layer(li) = rows[self.layers_scroll + row] else {
                            // Folder header row: no context menu.
                            return;
                        };
                        self.doc.active_layer = li;
                        let l = &self.doc.layers[li];
                        let mut items = vec![
                            ("Layer Options...".to_string(), Action::LayerOptions, true),
                            ("Duplicate Layer".to_string(), Action::DuplicateLayer, true),
                            (
                                "Delete Layer".to_string(),
                                Action::DeleteLayer,
                                self.doc.layers.len() > 1,
                            ),
                            ("-".to_string(), Action::None, false),
                        ];
                        if l.adjustment.is_some() {
                            items.push((
                                "Edit Adjustment...".to_string(),
                                Action::EditAdjustment,
                                true,
                            ));
                        }
                        if l.mask.is_none() {
                            items.push((
                                "Add Mask (Reveal All)".to_string(),
                                Action::AddMaskReveal,
                                true,
                            ));
                            items.push((
                                "Add Mask (Hide All)".to_string(),
                                Action::AddMaskHide,
                                true,
                            ));
                        } else {
                            items.push(("Delete Mask".to_string(), Action::DeleteMask, true));
                            items.push((
                                "Disable/Enable Mask".to_string(),
                                Action::ToggleMaskEnabled,
                                true,
                            ));
                        }
                        items.push(("-".to_string(), Action::None, false));
                        items.push((
                            "Blending Options...".to_string(),
                            Action::BlendingDialog,
                            true,
                        ));
                        items.push((
                            if l.clipped {
                                "Release Clipping Mask".to_string()
                            } else {
                                "Create Clipping Mask".to_string()
                            },
                            Action::ToggleClipped,
                            li > 0,
                        ));
                        items.push((
                            "Cycle Layer Color".to_string(),
                            Action::LayerColor((l.color + 1) % 8),
                            true,
                        ));
                        items.push(("-".to_string(), Action::None, false));
                        items.push(("Merge Down".to_string(), Action::MergeDown, li > 0));
                        self.context_menu = Some(ContextMenu {
                            x: mx,
                            y: my,
                            items,
                        });
                        return;
                    }
                }
            }
        }
        // Over the canvas?
        let c = self.canvas_rect(self.win_size.0, self.win_size.1);
        if c.contains(mx, my) {
            let has_sel = self.sel.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
            let items = vec![
                ("Undo/Redo".to_string(), Action::UndoToggle, true),
                ("-".to_string(), Action::None, false),
                ("Cut".to_string(), Action::Cut, has_sel),
                ("Copy".to_string(), Action::Copy, has_sel),
                ("Paste".to_string(), Action::Paste, true),
                ("Clear".to_string(), Action::Clear, has_sel),
                ("-".to_string(), Action::None, false),
                ("Select All".to_string(), Action::SelectAll, true),
                ("Deselect".to_string(), Action::SelectNone, has_sel),
                ("Inverse".to_string(), Action::SelectInverse, has_sel),
                ("-".to_string(), Action::None, false),
                (
                    "Free Transform".to_string(),
                    Action::FreeTransform,
                    self.doc.layers[self.doc.active_layer].adjustment.is_none(),
                ),
                (
                    "Add Layer Mask (Reveal)".to_string(),
                    Action::AddMaskReveal,
                    self.doc.layers[self.doc.active_layer].mask.is_none(),
                ),
            ];
            self.context_menu = Some(ContextMenu {
                x: mx,
                y: my,
                items,
            });
        }
    }

    pub(crate) fn mouse_down(&mut self, x: f32, y: f32) {
        let (mx, my) = (x as i32, y as i32);
        self.mouse_down = true;
        self.set_hover(None);
        // ---- modal dialog first
        if self.dialog.is_some() {
            let layout = self.dialog_layout().unwrap();
            // Settings Save/Load buttons (Phase 6.5 Batch 6d):
            // stash or load, closing this dialog first (its
            // preview reverts; the settings are what carry over).
            {
                let cand = self.dialog.as_ref().and_then(dialog_candidate);
                if cand.is_some() {
                    let r = layout.rect;
                    let save_r = Rect::new(r.x + 12, r.bottom() - 32, 52, 18);
                    let load_r = Rect::new(r.x + 70, r.bottom() - 32, 52, 18);
                    if save_r.contains(mx, my) {
                        self.settings_step = cand;
                        self.dialog_cancel();
                        self.open_save_dialog(4, "settings.txt");
                        self.status = "Name the settings file".into();
                        return;
                    }
                    if load_r.contains(mx, my) {
                        self.dialog_cancel();
                        self.file_panel_purpose = 5;
                        self.open_file_panel();
                        self.status = "Choose a settings file".into();
                        return;
                    }
                }
            }
            if layout.ok.contains(mx, my) {
                self.dialog_ok();
                return;
            }
            if layout.cancel.contains(mx, my) {
                self.dialog_cancel();
                return;
            }
            if let Some(tf) = layout.text_field {
                self.dialog.as_mut().unwrap().text_focus = tf.contains(mx, my);
            }
            let fg_rgba = self.fg.to_rgba8();
            let bg_rgba = self.bg.to_rgba8();
            let doc_wh = (self.doc.width, self.doc.height);
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::NoteEdit)
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        if i == 0 {
                            self.note_record_toggle();
                        } else {
                            self.note_play_clicked();
                        }
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::InsertMenuItem) {
                let r = layout.rect;
                let prev = Rect::new(r.x + 12, r.bottom() - 58, 80, 16);
                let next = Rect::new(r.x + 100, r.bottom() - 58, 80, 16);
                let d = self.dialog.as_mut().unwrap();
                if prev.contains(mx, my) {
                    d.list_offset = d.list_offset.saturating_sub(14);
                    return;
                }
                if next.contains(mx, my) {
                    let max_off = d.menu_items.len().saturating_sub(14);
                    d.list_offset = (d.list_offset + 14).min(max_off);
                    return;
                }
                for j in 0..14usize {
                    let row = Rect::new(r.x + 12, r.y + 30 + j as i32 * 15, r.w - 24, 14);
                    if row.contains(mx, my) && d.list_offset + j < d.menu_items.len() {
                        d.fx_page = d.list_offset + j;
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::SpellCheck) {
                let r = layout.rect;
                for i in 0..5usize {
                    let br = Rect::new(r.x + 12 + i as i32 * 76, r.y + 122, 72, 18);
                    if br.contains(mx, my) {
                        match i {
                            0 => self.spell_change(),
                            1 => {
                                if let Some(d) = &mut self.dialog {
                                    d.spell_pos += 1;
                                    if let Some((_, _, _, w)) =
                                        d.spell_items.get(d.spell_pos).cloned()
                                    {
                                        d.text = w;
                                    }
                                }
                            }
                            2 => {
                                let word = self.dialog.as_ref().and_then(|d| {
                                    d.spell_items
                                        .get(d.spell_pos)
                                        .map(|(_, _, _, w)| w.to_lowercase())
                                });
                                if let Some(w) = word {
                                    if let Some(d) = &mut self.dialog {
                                        d.spell_ignore.push(w);
                                    }
                                    self.spell_refresh();
                                }
                            }
                            3 => self.spell_add(),
                            _ => self.dialog_ok(),
                        }
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::Preferences) {
                let (fx_page, list_offset) = {
                    let d = self.dialog.as_ref().unwrap();
                    (d.fx_page, d.list_offset)
                };
                if fx_page == 1 {
                    let r = layout.rect;
                    for j in 0..12usize {
                        let row = Rect::new(r.x + 12, r.y + 52 + j as i32 * 15, r.w - 24, 15);
                        if row.contains(mx, my) {
                            let idx = list_offset + j;
                            if idx < self.shortcuts.len() {
                                self.prefs_capture = Some(idx);
                                self.prefs_confirm = None;
                                self.set_hover(None);
                            }
                            return;
                        }
                    }
                    let reset = Rect::new(r.x + 12, r.y + 238, 90, 18);
                    if reset.contains(mx, my) {
                        self.shortcuts = default_shortcuts();
                        self.prefs_capture = None;
                        self.prefs_confirm = None;
                        self.save_shortcuts_file();
                        self.status = "Shortcuts reset to defaults".into();
                        return;
                    }
                    if Rect::new(r.x + 12, r.y + 52, r.w - 24, 180).contains(mx, my) {
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::FindReplace) {
                if let Some(tf) = layout.text_field {
                    let d = self.dialog.as_mut().unwrap();
                    d.text_focus = true;
                    if tf.contains(mx, my) {
                        d.text_focus_n = 0;
                        return;
                    }
                    let f2 = Rect::new(tf.x, tf.y + 34, tf.w, tf.h);
                    if f2.contains(mx, my) {
                        d.text_focus_n = 1;
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::FileBrowser) {
                let r = layout.rect;
                for i in 0..12usize {
                    let cell = self.browser_cell(r, i);
                    if cell.contains(mx, my) {
                        if let Some(d) = &mut self.dialog {
                            if i < d.browser_entries.len() {
                                d.browser_sel = i;
                            }
                        }
                        return;
                    }
                }
                for (i, br) in self.browser_button_rects(r).iter().enumerate() {
                    if br.contains(mx, my) {
                        match i {
                            0 => {
                                self.dialog_ok();
                            }
                            1 => {
                                let parent = {
                                    let d = self.dialog.as_ref().unwrap();
                                    std::path::Path::new(&d.browser_dir)
                                        .parent()
                                        .map(|p| p.to_string_lossy().into_owned())
                                };
                                if let Some(p) = parent {
                                    self.dialog.as_mut().unwrap().browser_dir = p;
                                    self.browser_rescan();
                                }
                            }
                            2 | 3 => {
                                let delta: i8 = if i == 2 { -1 } else { 1 };
                                if let Some(d) = &mut self.dialog {
                                    if let Some(e) = d.browser_entries.get_mut(d.browser_sel) {
                                        if !e.is_dir {
                                            e.rotate = (e.rotate + delta).clamp(-1, 1);
                                        }
                                    }
                                }
                                self.browser_save_meta();
                            }
                            4 => {
                                if let Some(d) = &mut self.dialog {
                                    if let Some(e) = d.browser_entries.get_mut(d.browser_sel) {
                                        if !e.is_dir {
                                            e.rank = (e.rank + 1) % 6;
                                        }
                                    }
                                }
                                self.browser_save_meta();
                            }
                            5 => {
                                if let Some(d) = &mut self.dialog {
                                    d.browser_sort_rank = !d.browser_sort_rank;
                                }
                                self.browser_rescan();
                            }
                            6 => self.browser_batch_rename(),
                            _ => self.browser_rescan(),
                        }
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::Liquify) {
                let zr = self.liquify_zoom_rects(layout.rect);
                for (i, z) in zr.iter().enumerate() {
                    if z.contains(mx, my) {
                        if let Some(d) = &mut self.dialog {
                            match i {
                                0 => d.liq_zoom = (d.liq_zoom * 0.8).max(0.25),
                                1 => d.liq_zoom = (d.liq_zoom * 1.25).min(8.0),
                                _ => {
                                    d.liq_zoom = 1.0;
                                    d.liq_pan =
                                        (self.doc.width as f32 / 2.0, self.doc.height as f32 / 2.0);
                                }
                            }
                        }
                        return;
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::PatternMaker) {
                let fb = Rect::new(layout.rect.x + 12, layout.rect.y + 36 + 158, 150, 18);
                if fb.contains(mx, my) {
                    if let Some(d) = &mut self.dialog {
                        d.opt_a = !d.opt_a;
                    }
                    return;
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::Variations) {
                if let Some(cells) = self.var_cells() {
                    for (cell, _label, kind) in cells {
                        if kind != 0 && cell.contains(mx, my) {
                            self.var_apply_click(kind);
                            return;
                        }
                    }
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::Duotone) {
                let mut handled = false;
                {
                    let d = self.dialog.as_mut().unwrap();
                    for i in 0..d.duo_inks.len() {
                        if duo_swatch_rect(layout.rect, i).contains(mx, my) {
                            // Save the edited ink, select the
                            // clicked one.
                            if d.fx_page < d.duo_inks.len() {
                                let cur = (
                                    [
                                        d.vals[0].round() as u8,
                                        d.vals[1].round() as u8,
                                        d.vals[2].round() as u8,
                                    ],
                                    d.vals[3].round() as u8,
                                );
                                d.duo_inks[d.fx_page] = cur;
                            }
                            d.fx_page = i;
                            let ink = d.duo_inks[i];
                            d.vals = vec![
                                ink.0[0] as f32,
                                ink.0[1] as f32,
                                ink.0[2] as f32,
                                ink.1 as f32,
                            ];
                            handled = true;
                        }
                    }
                }
                if handled {
                    return;
                }
            }
            if self.dialog.as_ref().map(|d| &d.kind) == Some(&DialogKind::ColorTable) {
                let mut picked: Option<usize> = None;
                for i in 0..self.doc.color_table.len() {
                    if table_cell_rect(layout.rect, i).contains(mx, my) {
                        picked = Some(i);
                        break;
                    }
                }
                if let Some(i) = picked {
                    let c = self.doc.color_table[i];
                    let d = self.dialog.as_mut().unwrap();
                    d.table_sel = i;
                    d.vals = vec![c.0 as f32, c.1 as f32, c.2 as f32];
                    return;
                }
            }
            for (i, b) in layout.range_buttons.iter().enumerate() {
                if b.contains(mx, my) {
                    // This loop only owns seven dialogs' buttons;
                    // every other dialog's button falls through to
                    // the Block A loop below (an unguarded return
                    // here used to swallow those clicks).
                    let kind = self.dialog.as_ref().unwrap().kind.clone();
                    if !matches!(
                        kind,
                        DialogKind::ColorBalance
                            | DialogKind::Fill
                            | DialogKind::HistoryOptions
                            | DialogKind::PhotoFilter
                            | DialogKind::Blending
                            | DialogKind::GradientMap
                            | DialogKind::Liquify
                    ) {
                        break;
                    }
                    let is_photo = {
                        let d = self.dialog.as_mut().unwrap();
                        if d.kind == DialogKind::ColorBalance {
                            d.cb_range = i;
                        } else if d.kind == DialogKind::Fill {
                            d.fill_bg = i == 1;
                            d.fill_history = i == 2;
                        } else if d.kind == DialogKind::HistoryOptions {
                            if i == 0 {
                                d.opt_a = !d.opt_a;
                            } else {
                                d.opt_b = !d.opt_b;
                            }
                        } else if d.kind == DialogKind::PhotoFilter {
                            d.photo_color = PHOTO_FILTER_PRESETS[i].1;
                        } else if d.kind == DialogKind::Blending {
                            match i {
                                0 => d.blend_knockout = (d.blend_knockout + 1) % 3,
                                1 => {
                                    d.blend_channels = match d.blend_channels {
                                        0xF => 0x1,
                                        0x1 => 0x2,
                                        0x2 => 0x4,
                                        0x4 => 0x7,
                                        0x7 => 0x0,
                                        _ => 0xF,
                                    }
                                }
                                _ => d.blend_toggles ^= 1u8 << (i - 2),
                            }
                        } else if d.kind == DialogKind::GradientMap {
                            match i {
                                0 => {
                                    let (r, g, b, _) = fg_rgba;
                                    d.photo_color = (r, g, b);
                                }
                                1 => {
                                    let (r, g, b, _) = bg_rgba;
                                    d.gm_to = (r, g, b);
                                }
                                _ => d.opt_a = !d.opt_a,
                            }
                        } else if d.kind == DialogKind::Liquify {
                            match i {
                                0..=11 => d.liq_tool = i as u8,
                                12 => d.liq_mesh = !d.liq_mesh,
                                13 => {
                                    d.liq_undo.push(d.liq_disp.clone());
                                    let mut fld = pictodesk_core::liquify::LiquifyField {
                                        w: doc_wh.0,
                                        h: doc_wh.1,
                                        disp: std::mem::take(&mut d.liq_disp),
                                    };
                                    fld.reconstruct(1, &d.liq_frozen.clone());
                                    d.liq_disp = fld.disp;
                                }
                                14 => {
                                    d.liq_undo.push(d.liq_disp.clone());
                                    for v in d.liq_disp.iter_mut() {
                                        *v = (0.0, 0.0);
                                    }
                                }
                                15 => {
                                    if let Some(prev) = d.liq_undo.pop() {
                                        d.liq_disp = prev;
                                    }
                                }
                                16 => d.liq_backdrop = !d.liq_backdrop,
                                17 => d.liq_save_req = true,
                                _ => d.liq_load_req = true,
                            }
                        }
                        d.kind == DialogKind::PhotoFilter
                    };
                    if self
                        .dialog
                        .as_ref()
                        .map(|d| d.liq_save_req || d.liq_load_req)
                        .unwrap_or(false)
                    {
                        self.liquify_mesh_io();
                        return;
                    }
                    let kind6 = self.dialog.as_ref().unwrap().kind.clone();
                    if kind6 == DialogKind::Blending {
                        self.blend_sync_from_vals();
                    } else if kind6 == DialogKind::GradientMap || is_photo {
                        self.dialog_refresh_preview();
                    }
                    return;
                }
            }
            // Choice buttons for the Automate / Print / Web
            // dialogs, filter option buttons, 3D shapes.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::CondMode)
                    | Some(DialogKind::PicturePackage)
                    | Some(DialogKind::FindReplace)
                    | Some(DialogKind::SaveForWeb)
                    | Some(DialogKind::Print)
                    | Some(DialogKind::PageSetup)
                    | Some(DialogKind::Transform3d)
                    | Some(DialogKind::BitmapConv)
                    | Some(DialogKind::IndexedConv)
                    | Some(DialogKind::Duotone)
                    | Some(DialogKind::ApplyImage)
                    | Some(DialogKind::Calculations)
                    | Some(DialogKind::LoadSel)
                    | Some(DialogKind::ChannelOptions)
                    | Some(DialogKind::MergeChannels)
                    | Some(DialogKind::SelectiveColor)
                    | Some(DialogKind::ReplaceColor)
                    | Some(DialogKind::Fade)
                    | Some(DialogKind::StrokeSel)
                    | Some(DialogKind::RotateArbitrary)
                    | Some(DialogKind::Preferences)
                    | Some(DialogKind::FileInfo)
                    | Some(DialogKind::NewLayer)
                    | Some(DialogKind::LegacyFilter(_))
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        let kind = self.dialog.as_ref().unwrap().kind.clone();
                        match kind {
                            DialogKind::CondMode => {
                                let d = self.dialog.as_mut().unwrap();
                                if i == 0 {
                                    d.cond_from = (d.cond_from + 1) % 4;
                                } else {
                                    d.cond_to = (d.cond_to + 1) % 3;
                                }
                            }
                            DialogKind::PicturePackage => match i {
                                0 => {
                                    let d = self.dialog.as_mut().unwrap();
                                    d.pkg_layout = (d.pkg_layout + 1) % 4;
                                }
                                1 => {
                                    let d = self.dialog.as_mut().unwrap();
                                    d.fx_page = (d.fx_page + 1) % 5;
                                    let (pw, ph) = [
                                        (612.0, 792.0),
                                        (595.0, 842.0),
                                        (288.0, 432.0),
                                        (360.0, 504.0),
                                        (612.0, 1008.0),
                                    ][d.fx_page];
                                    d.vals[0] = pw;
                                    d.vals[1] = ph;
                                }
                                2 => {
                                    let d = self.dialog.as_mut().unwrap();
                                    d.fx_a = (d.fx_a + 1) % 4;
                                }
                                _ => {
                                    self.file_panel_purpose = 7;
                                    self.open_file_panel();
                                }
                            },
                            DialogKind::FindReplace => match i {
                                0 => {
                                    self.find_replace_all();
                                }
                                1 => {
                                    let d = self.dialog.as_mut().unwrap();
                                    d.opt_a = !d.opt_a;
                                }
                                _ => {
                                    let d = self.dialog.as_mut().unwrap();
                                    d.opt_b = !d.opt_b;
                                }
                            },
                            DialogKind::SaveForWeb => {
                                {
                                    let d = self.dialog.as_mut().unwrap();
                                    match i {
                                        0..=3 => d.web_format = i as u8,
                                        4 => d.web_weighted = !d.web_weighted,
                                        5 => d.web_knockout = !d.web_knockout,
                                        _ => d.web_dither = !d.web_dither,
                                    }
                                }
                                self.refresh_web_size();
                            }
                            DialogKind::Print => {
                                let d = self.dialog.as_mut().unwrap();
                                match i {
                                    0 => d.print_profile = (d.print_profile + 1) % 2,
                                    1 => d.opt_a = !d.opt_a,
                                    2 => d.opt_b = !d.opt_b,
                                    3 => d.pr_bilinear = !d.pr_bilinear,
                                    4..=7 => d.fx_a ^= 1 << (i - 4),
                                    8 => d.fx_b = !d.fx_b,
                                    _ => d.pr_emulsion = !d.pr_emulsion,
                                }
                            }
                            DialogKind::PageSetup => {
                                let d = self.dialog.as_mut().unwrap();
                                d.opt_a = !d.opt_a;
                                // Paper turns with the orientation.
                                let (pw, ph) = (d.vals[0], d.vals[1]);
                                d.vals[0] = ph;
                                d.vals[1] = pw;
                            }
                            DialogKind::Transform3d => {
                                self.dialog.as_mut().unwrap().shape3d = i as u8;
                                self.dialog_refresh_preview();
                            }
                            DialogKind::BitmapConv => {
                                self.dialog.as_mut().unwrap().fx_a = i as u8;
                            }
                            DialogKind::SelectiveColor => {
                                {
                                    let d = self.dialog.as_mut().unwrap();
                                    if i < 9 {
                                        d.fx_a = i as u8;
                                    } else {
                                        d.fx_b = !d.fx_b;
                                    }
                                }
                                self.dialog_refresh_preview();
                            }
                            DialogKind::ReplaceColor => {
                                {
                                    let d = self.dialog.as_mut().unwrap();
                                    d.fx_b = !d.fx_b;
                                    let c = if d.fx_b { self.bg } else { self.fg };
                                    let (r, g, b, _) = c.to_rgba8();
                                    d.rc_target = (r, g, b);
                                }
                                self.dialog_refresh_preview();
                            }
                            DialogKind::ApplyImage => {
                                let doc = &self.doc;
                                let d = self.dialog.as_mut().unwrap();
                                match i {
                                    0 => d.src_layer = cyc(d.src_layer, doc.layers.len() + 1),
                                    1 => d.src_chan = cyc(d.src_chan, chan_cycle(doc).len()),
                                    2 => d.blend_idx = cyc(d.blend_idx, 12),
                                    3 => d.mask_sel = cyc(d.mask_sel, doc.alphas.len() + 1),
                                    4 => {
                                        let n = color_chan_names(doc).len() + 1;
                                        d.target_sel = cyc(d.target_sel, n);
                                    }
                                    _ => d.fx_b = !d.fx_b,
                                }
                            }
                            DialogKind::Calculations => {
                                let doc = &self.doc;
                                let d = self.dialog.as_mut().unwrap();
                                match i {
                                    0 => d.src_layer = cyc(d.src_layer, doc.layers.len() + 1),
                                    1 => d.src_chan = cyc(d.src_chan, chan_cycle(doc).len()),
                                    2 => d.blend_idx = cyc(d.blend_idx, 14),
                                    3 => d.src2_layer = cyc(d.src2_layer, doc.layers.len() + 1),
                                    4 => d.src2_chan = cyc(d.src2_chan, chan_cycle(doc).len()),
                                    _ => d.result_sel = cyc(d.result_sel, 3),
                                }
                            }
                            DialogKind::LoadSel => {
                                let d = self.dialog.as_mut().unwrap();
                                if i == 0 {
                                    let n = self.doc.alphas.len();
                                    d.load_chan = cyc(d.load_chan, n);
                                } else {
                                    d.load_op = i - 1;
                                }
                            }
                            DialogKind::ChannelOptions => {
                                let d = self.dialog.as_mut().unwrap();
                                if i == 0 {
                                    d.fx_b = !d.fx_b;
                                } else {
                                    d.fx_a = (d.fx_a + 1) % 2;
                                }
                            }
                            DialogKind::Fade => {
                                let d = self.dialog.as_mut().unwrap();
                                d.fx_a = (d.fx_a + 1) % FADE_MODES.len() as u8;
                                self.dialog_refresh_preview();
                            }
                            DialogKind::StrokeSel => {
                                let d = self.dialog.as_mut().unwrap();
                                if i < 3 {
                                    d.fx_a = i as u8;
                                } else {
                                    d.fx_b = !d.fx_b;
                                }
                            }
                            DialogKind::RotateArbitrary => {
                                let d = self.dialog.as_mut().unwrap();
                                d.fx_a = i as u8;
                            }
                            DialogKind::Preferences => {
                                let d = self.dialog.as_mut().unwrap();
                                match i {
                                    0 => d.fx_page = 0,
                                    1 => d.fx_page = 1,
                                    2 => d.opt_a = !d.opt_a,
                                    3 => d.opt_b = !d.opt_b,
                                    4 => d.opt_c = !d.opt_c,
                                    5 => d.opt_d = !d.opt_d,
                                    _ => d.fx_a = (d.fx_a + 1) % 3,
                                }
                            }
                            DialogKind::FileInfo => {
                                let d = self.dialog.as_mut().unwrap();
                                if i != d.fx_page && i < 5 {
                                    let cur = d.text.trim().to_string();
                                    d.info_fields[d.fx_page.min(4)] = cur;
                                    d.fx_page = i;
                                    d.text = d.info_fields[i].clone();
                                    d.text_focus = true;
                                }
                            }
                            DialogKind::NewLayer => {
                                let d = self.dialog.as_mut().unwrap();
                                d.fx_b = !d.fx_b;
                            }
                            DialogKind::MergeChannels => {
                                let d = self.dialog.as_mut().unwrap();
                                if i == 0 {
                                    d.merge_mode = cyc(d.merge_mode, 4);
                                } else {
                                    let need = [1, 3, 4, 3][d.merge_mode.min(3)];
                                    if i <= need {
                                        let n = self.split_docs.len();
                                        d.merge_src[i - 1] = cyc(d.merge_src[i - 1], n);
                                    }
                                }
                            }
                            DialogKind::IndexedConv => {
                                let d = self.dialog.as_mut().unwrap();
                                if i < 3 {
                                    d.fx_a = i as u8;
                                } else {
                                    d.fx_b = !d.fx_b;
                                }
                            }
                            DialogKind::Duotone => {
                                // Ink-count buttons: save the
                                // edited ink, resize the spec.
                                let d = self.dialog.as_mut().unwrap();
                                if d.fx_page < d.duo_inks.len() {
                                    let cur = (
                                        [
                                            d.vals[0].round() as u8,
                                            d.vals[1].round() as u8,
                                            d.vals[2].round() as u8,
                                        ],
                                        d.vals[3].round() as u8,
                                    );
                                    d.duo_inks[d.fx_page] = cur;
                                }
                                let want = i + 1;
                                const DEFAULTS: [([u8; 3], u8); 4] = [
                                    ([0, 0, 0], 0),
                                    ([112, 88, 64], 0),
                                    ([48, 72, 128], 0),
                                    ([160, 48, 32], 0),
                                ];
                                while d.duo_inks.len() < want {
                                    let idx = d.duo_inks.len();
                                    d.duo_inks.push(DEFAULTS[idx]);
                                }
                                d.duo_inks.truncate(want);
                                if d.fx_page >= want {
                                    d.fx_page = want - 1;
                                    let ink = d.duo_inks[d.fx_page];
                                    d.vals = vec![
                                        ink.0[0] as f32,
                                        ink.0[1] as f32,
                                        ink.0[2] as f32,
                                        ink.1 as f32,
                                    ];
                                }
                            }
                            DialogKind::LegacyFilter(id) => {
                                let d = self.dialog.as_mut().unwrap();
                                if id == 6 || matches!(id, 16 | 17 | 21 | 23 | 27 | 35 | 50 | 57) {
                                    d.fx_a = i as u8;
                                } else if id == 65 {
                                    d.fx_a = (d.fx_a + 1) % 10;
                                } else if matches!(id, 55 | 56 | 60) {
                                    d.fx_a = (d.fx_a + 1) % 4;
                                } else if id == 68 {
                                    if i == 0 {
                                        d.fx_a = (d.fx_a + 1) % 4;
                                    } else {
                                        d.fx_c = (d.fx_c + 1) % 4;
                                    }
                                } else if matches!(id, 39 | 42) {
                                    d.fx_a = (d.fx_a + 1) % 4;
                                } else if matches!(id, 26 | 28) {
                                    if i == 0 {
                                        d.fx_a = (d.fx_a + 1) % 2;
                                    } else {
                                        d.fx_b = !d.fx_b;
                                    }
                                } else if id == 15 {
                                    if i == 0 {
                                        d.fx_a = (d.fx_a + 1) % 3;
                                    } else {
                                        d.fx_b = !d.fx_b;
                                    }
                                } else if id == 22 {
                                    if i == 0 {
                                        d.fx_a = (d.fx_a + 1) % 2;
                                    } else {
                                        d.fx_b = !d.fx_b;
                                    }
                                }
                                self.dialog_refresh_preview();
                            }
                            _ => {}
                        }
                        return;
                    }
                }
            }
            // Range/output buttons for H/S, Channel Mixer, Color
            // Settings (handled before the generic range loop).
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::HueSaturation)
                    | Some(DialogKind::ChannelMixer)
                    | Some(DialogKind::ColorSettings)
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        let kind = self.dialog.as_ref().unwrap().kind.clone();
                        match kind {
                            DialogKind::HueSaturation => {
                                self.dialog.as_mut().unwrap().hs_range = i;
                            }
                            DialogKind::ColorSettings => {
                                let d = self.dialog.as_mut().unwrap();
                                if i < 3 {
                                    d.cb_range = i;
                                } else {
                                    d.cs_default = i - 3;
                                }
                            }
                            DialogKind::ChannelMixer => {
                                let cmyk = self.doc.mode == ColorMode::Cmyk;
                                let d = self.dialog.as_mut().unwrap();
                                if i == 3 && !cmyk {
                                    d.mixer.monochrome = !d.mixer.monochrome;
                                } else {
                                    // Stash the current row, switch,
                                    // load the new row's values.
                                    d.mixer.rows[d.mix_out] = [
                                        d.vals[0] / 100.0,
                                        d.vals[1] / 100.0,
                                        d.vals[2] / 100.0,
                                        d.vals[3] / 100.0,
                                    ];
                                    d.mixer.constant[d.mix_out] = d.vals[4] / 100.0;
                                    d.mix_out = i;
                                    let row = d.mixer.rows[i];
                                    d.vals = vec![
                                        row[0] * 100.0,
                                        row[1] * 100.0,
                                        row[2] * 100.0,
                                        row[3] * 100.0,
                                        d.mixer.constant[i] * 100.0,
                                    ];
                                }
                            }
                            _ => {}
                        }
                        self.dialog_refresh_preview();
                        return;
                    }
                }
            }
            // Liquify editor: stamp with the current tool.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Liquify)
            ) {
                if let Some(dp) = self.liquify_doc_point(mx, my) {
                    self.liquify_stamp(dp);
                    return;
                }
            }
            // Preset Manager: tabs, rows, action buttons.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::PresetManager)
            ) {
                let r = layout.rect;
                for i in 0..3usize {
                    let br = Rect::new(r.x + 12 + i as i32 * 92, r.y + 52, 88, 18);
                    if br.contains(mx, my) {
                        self.preset_tab = i;
                        self.dialog.as_mut().unwrap().preset_sel = 0;
                        return;
                    }
                }
                let rows = self.preset_rows();
                for i in 0..rows.len().min(12) {
                    let rr = Rect::new(r.x + 12, r.y + 78 + i as i32 * 15, r.w - 24, 14);
                    if rr.contains(mx, my) {
                        self.dialog.as_mut().unwrap().preset_sel = i;
                        return;
                    }
                }
                let by = r.bottom() - 58;
                if Rect::new(r.x + 12, by, 110, 18).contains(mx, my) {
                    self.dialog_ok();
                    return;
                }
                if self.preset_tab == 2 {
                    if Rect::new(r.x + 130, by, 90, 18).contains(mx, my) {
                        let typed = self.dialog.as_ref().unwrap().text.trim().to_string();
                        let name = if typed.is_empty() {
                            format!(
                                "{} Preset {}",
                                self.tool.name(),
                                self.tool_presets.len() + 1
                            )
                        } else {
                            typed
                        };
                        let p = self.capture_tool_preset(name.clone());
                        self.tool_presets.push(p);
                        save_tool_presets(&self.tool_presets);
                        self.status = format!("Tool preset '{name}' saved");
                        return;
                    }
                    if Rect::new(r.x + 228, by, 90, 18).contains(mx, my) {
                        let sel = self.dialog.as_ref().unwrap().preset_sel;
                        if sel < self.tool_presets.len() {
                            let p = self.tool_presets.remove(sel);
                            save_tool_presets(&self.tool_presets);
                            self.status = format!("Tool preset '{}' deleted", p.name);
                        }
                        return;
                    }
                }
                if self.preset_tab == 0 {
                    if Rect::new(r.x + 130, by, 90, 18).contains(mx, my) {
                        // New: save the active layer's effects
                        // under the name in the text field.
                        let name = self.dialog.as_ref().unwrap().text.trim().to_string();
                        if !name.is_empty() {
                            let fx = self.doc.layers[self.doc.active_layer]
                                .effects
                                .unwrap_or_default();
                            self.style_presets.push((name.clone(), fx));
                            self.status = format!("Style preset '{name}' saved");
                        } else {
                            self.status = "Preset Manager: type a name first".into();
                        }
                        return;
                    }
                    if Rect::new(r.x + 228, by, 90, 18).contains(mx, my) {
                        let sel = self.dialog.as_ref().unwrap().preset_sel;
                        if sel < self.style_presets.len() {
                            let (name, _) = self.style_presets.remove(sel);
                            self.status = format!("Style preset '{name}' deleted");
                        }
                        return;
                    }
                }
            }
            // Slice Properties: focus the URL / Alt fields.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::SliceProps)
            ) {
                if let Some(tf) = layout.text_field {
                    let d = self.dialog.as_mut().unwrap();
                    d.text_focus = true;
                    if tf.contains(mx, my) {
                        d.text_focus_n = 0;
                        return;
                    }
                    if Rect::new(tf.x, tf.y + 24, tf.w, tf.h).contains(mx, my) {
                        d.text_focus_n = 1;
                        return;
                    }
                    if Rect::new(tf.x, tf.y + 48, tf.w, tf.h).contains(mx, my) {
                        d.text_focus_n = 2;
                        return;
                    }
                }
            }
            // Effects dialog: page tabs + toggle buttons
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Effects)
            ) {
                for (i, b) in layout.range_buttons.iter().enumerate() {
                    if b.contains(mx, my) {
                        self.fx_sync_from_vals();
                        self.dialog.as_mut().unwrap().fx_page = i;
                        self.fx_load_vals();
                        return;
                    }
                }
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if br.contains(mx, my) {
                        self.fx_button_click(i);
                        return;
                    }
                }
            }
            // Extract dialog: mode buttons + editor painting
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Extract)
            ) {
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if br.contains(mx, my) {
                        match i {
                            4 => {
                                // Cleanup: tighten the fill —
                                // drop fill pixels that lack a
                                // filled 4-neighbour (specks) and
                                // erode one pixel all round.
                                let d = self.dialog.as_mut().unwrap();
                                let (w, h) = (self.doc.width as i32, self.doc.height as i32);
                                let old = d.extract_fill.clone();
                                let at = |x: i32, y: i32| -> bool {
                                    x >= 0
                                        && y >= 0
                                        && x < w
                                        && y < h
                                        && old[(y * w + x) as usize] > 0
                                };
                                for y in 0..h {
                                    for x in 0..w {
                                        let keep = at(x, y)
                                            && at(x - 1, y)
                                            && at(x + 1, y)
                                            && at(x, y - 1)
                                            && at(x, y + 1);
                                        d.extract_fill[(y * w + x) as usize] =
                                            if keep { 255 } else { 0 };
                                    }
                                }
                                self.status = "Cleanup: fill tightened by one pixel".into();
                            }
                            5 => {
                                let d = self.dialog.as_mut().unwrap();
                                d.extract_smart = !d.extract_smart;
                            }
                            6 => {
                                let d = self.dialog.as_mut().unwrap();
                                d.extract_preview = !d.extract_preview;
                                let on = d.extract_preview;
                                if on {
                                    self.compute_extract_preview();
                                }
                            }
                            _ => {
                                self.dialog.as_mut().unwrap().extract_mode = i as u8;
                            }
                        }
                        return;
                    }
                }
                if let Some(dp) = self.extract_doc_point(mx, my) {
                    self.extract_paint(dp);
                    return;
                }
                if self
                    .extract_editor_rect()
                    .map(|r| r.contains(mx, my))
                    .unwrap_or(false)
                {
                    return;
                }
            }
            // Type dialog: font lists and format buttons
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Type)
            ) {
                if let Some((fams_r, styles_r)) = layout.font_list {
                    let up = Rect::new(fams_r.right() - 15, fams_r.y + 1, 14, 14);
                    let dn = Rect::new(fams_r.right() - 15, fams_r.bottom() - 15, 14, 14);
                    if up.contains(mx, my) {
                        let d = self.dialog.as_mut().unwrap();
                        d.font_scroll = d.font_scroll.saturating_sub(5);
                        return;
                    }
                    if dn.contains(mx, my) {
                        let max = typo::families().len().saturating_sub(6);
                        let d = self.dialog.as_mut().unwrap();
                        d.font_scroll = (d.font_scroll + 5).min(max);
                        return;
                    }
                    if fams_r.contains(mx, my) {
                        let row = ((my - fams_r.y - 2) / 15).max(0) as usize;
                        let fams = typo::families();
                        let scroll = self.dialog.as_ref().unwrap().font_scroll;
                        if let Some(fam) = fams.get(scroll + row).cloned() {
                            let first_style =
                                typo::styles_of(&fam).into_iter().next().unwrap_or_default();
                            self.type_style_apply(|st| {
                                st.font = fam.clone();
                                st.style = first_style.clone();
                            });
                        }
                        return;
                    }
                    if styles_r.contains(mx, my) {
                        let row = ((my - styles_r.y - 2) / 15).max(0) as usize;
                        let fam = self
                            .dialog
                            .as_ref()
                            .unwrap()
                            .type_spec
                            .as_ref()
                            .map(|s| s.default_style.font.clone())
                            .unwrap_or_default();
                        if let Some(sty) = typo::styles_of(&fam).get(row).cloned() {
                            self.type_style_apply(|st| st.style = sty.clone());
                        }
                        return;
                    }
                }
                for (i, br) in layout.type_buttons.iter().enumerate() {
                    if br.contains(mx, my) {
                        match i {
                            0 => self.type_style_apply(|st| st.faux_bold = !st.faux_bold),
                            1 => self.type_style_apply(|st| st.faux_italic = !st.faux_italic),
                            2 => self.type_style_apply(|st| st.underline = !st.underline),
                            3..=5 => {
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.align = (i - 3) as u8;
                                }
                            }
                            6 => {
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.vertical = !spec.vertical;
                                }
                            }
                            7 => {
                                let fg = self.fg;
                                self.type_style_apply(|st| st.color = fg);
                            }
                            8 => {
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.warp_style = match spec.warp_style {
                                        None => Some(0),
                                        Some(w) if w < 14 => Some(w + 1),
                                        _ => None,
                                    };
                                }
                            }
                            9..=12 => {
                                let bit = [1u8, 2, 4, 8][i - 9];
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.extras ^= bit;
                                    if bit == 2 {
                                        spec.extras &= !4;
                                    }
                                    if bit == 4 {
                                        spec.extras &= !2;
                                    }
                                }
                            }
                            13 => {
                                if let Some(spec) = &mut self.dialog.as_mut().unwrap().type_spec {
                                    spec.align = if spec.align == 3 { 0 } else { 3 };
                                }
                            }
                            _ => {}
                        }
                        return;
                    }
                }
            }
            for (i, cell) in layout.anchor_cells.iter().enumerate() {
                if cell.contains(mx, my) {
                    self.dialog.as_mut().unwrap().anchor = (i as i32 % 3 - 1, i as i32 / 3 - 1);
                    return;
                }
            }
            if let Some(g) = layout.graph {
                if g.contains(mx, my) {
                    let d = self.dialog.as_mut().unwrap();
                    let gx = ((mx - g.x) as f32 / g.w as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    let gy =
                        (255.0 - (my - g.y) as f32 / g.h as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    // grab a nearby point, else add one
                    let near = d.curve_points.iter().position(|p| {
                        (p.0 as i32 - gx as i32).abs() < 12 && (p.1 as i32 - gy as i32).abs() < 12
                    });
                    if let Some(i) = near {
                        d.curve_drag = Some(i);
                    } else {
                        d.curve_points.push((gx, gy));
                        d.curve_points.sort();
                        d.curve_drag = d.curve_points.iter().position(|p| *p == (gx, gy));
                    }
                    self.dialog_refresh_preview();
                    return;
                }
            }
            for (i, cell) in layout.kernel_cells.iter().enumerate() {
                if cell.contains(mx, my) {
                    self.dialog.as_mut().unwrap().kernel_sel = i;
                    return;
                }
            }
            for (i, b) in layout.kernel_buttons.iter().enumerate() {
                if b.contains(mx, my) {
                    {
                        let d = self.dialog.as_mut().unwrap();
                        match i {
                            0 => d.kernel_scale = (d.kernel_scale - 1).max(1),
                            1 => d.kernel_scale = (d.kernel_scale + 1).min(999),
                            2 => d.kernel_offset = (d.kernel_offset - 1).max(-255),
                            _ => d.kernel_offset = (d.kernel_offset + 1).min(255),
                        }
                    }
                    self.dialog_refresh_preview();
                    return;
                }
            }
            // 3D Transform wireframe: press inside it to start
            // a rotate drag.
            if matches!(
                self.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Transform3d)
            ) {
                if let Some(wr) = self.wireframe_rect() {
                    if wr.contains(mx, my) {
                        self.dialog.as_mut().unwrap().wire_drag = Some((mx, my));
                        return;
                    }
                }
            }
            for (i, row) in layout.slider_rows.iter().enumerate() {
                if row.contains(mx, my) {
                    self.dialog.as_mut().unwrap().slider_drag = Some(i);
                    let t = self.dialog_slider_value(*row, mx);
                    self.set_dialog_slider(i, t);
                    return;
                }
            }
            return; // modal: swallow everything else
        }
        // ---- file panel
        if self.file_panel.is_some() {
            let pr = self.file_panel_rect();
            let rows = self.file_panel_rows();
            let cancel = Rect::new(pr.right() - 92, pr.bottom() - 32, 80, 24);
            if cancel.contains(mx, my) || !pr.contains(mx, my) {
                self.file_panel = None;
                self.file_panel_purpose = 0;
                self.pending_displace = None;
                return;
            }
            for (i, (p, is_dir)) in rows.iter().take(14).enumerate() {
                let ir = Rect::new(pr.x + 6, pr.y + 44 + i as i32 * 17, pr.w - 12, 17);
                if ir.contains(mx, my) {
                    // ".." goes up one folder; a directory goes in.
                    if p.is_none() {
                        if let Some(parent) = self.file_panel_dir.parent().map(|p| p.to_path_buf())
                        {
                            if parent != self.file_panel_dir {
                                self.file_panel_dir = parent;
                                save_last_open_dir(&self.file_panel_dir.clone());
                                self.refresh_file_panel();
                            }
                        }
                        return;
                    }
                    let p = p.as_ref().unwrap();
                    if *is_dir {
                        self.file_panel_dir = p.clone();
                        save_last_open_dir(&self.file_panel_dir.clone());
                        self.refresh_file_panel();
                        return;
                    }
                    let path = p.clone();
                    self.file_panel = None;
                    let purpose = self.file_panel_purpose;
                    self.file_panel_purpose = 0;
                    match purpose {
                        0 => self.open_path(&path),
                        3 => self.load_swatches_file(&path),
                        4 => self.load_brushes_file(&path),
                        5 => self.load_settings_file(&path),
                        7 => {
                            self.pkg_sources.push(path.clone());
                            self.status =
                                format!("Picture Package sources: {}", self.pkg_sources.len());
                        }
                        6 => {
                            match std::fs::read(&path)
                                .ok()
                                .and_then(|b| pictodesk_core::liquify::field_from_bytes(&b))
                            {
                                Some(f) => self.liquify_install_mesh(f),
                                None => self.status = "Not a mesh file".into(),
                            }
                        }
                        _ => self.take_map_file(&path),
                    }
                    return;
                }
            }
            return;
        }
        // ---- document tab bar
        if self.display_mode == 0 && self.tab_bar_mouse(mx, my) {
            return;
        }
        // ---- context menu (open): click an item or dismiss
        if let Some(cm) = self.context_menu.take() {
            let w = cm
                .items
                .iter()
                .map(|(l, _, _)| Frame::text_width(l, 1) + 28)
                .max()
                .unwrap_or(120);
            let r = Rect::new(cm.x, cm.y, w, cm.items.len() as i32 * 17 + 6);
            if r.contains(mx, my) {
                for (i, (label, action, enabled)) in cm.items.iter().enumerate() {
                    let ir = Rect::new(r.x + 2, r.y + 3 + i as i32 * 17, r.w - 4, 17);
                    if ir.contains(mx, my) && *enabled && label != "-" {
                        let action = action.clone();
                        self.do_action(action);
                        return;
                    }
                }
            }
            return;
        }
        // ---- guides: drag out of a ruler, or drag an existing one
        if self.show_guides && self.dialog.is_none() && !self.guides_locked {
            let c = self.canvas_rect(self.win_size.0, self.win_size.1);
            let dp = self.to_doc(x, y);
            if self.show_rulers && self.display_mode == 0 {
                let top = Rect::new(
                    ui::TOOLBAR_W,
                    self.top_y(),
                    c.right() - ui::TOOLBAR_W,
                    ui::RULER_H,
                );
                let left = Rect::new(
                    ui::TOOLBAR_W,
                    self.top_y(),
                    ui::RULER_W,
                    c.bottom() - self.top_y(),
                );
                if top.contains(mx, my) && mx >= c.x {
                    self.guide_drag = Some((false, dp.1 as i32, None));
                    self.drag = DragMode::GuideDrag;
                    return;
                }
                if left.contains(mx, my) && my >= c.y {
                    self.guide_drag = Some((true, dp.0 as i32, None));
                    self.drag = DragMode::GuideDrag;
                    return;
                }
            }
            if self.tool == Tool::Move && c.contains(mx, my) {
                for (i, g) in self.doc.guides_x.iter().enumerate() {
                    let sx = c.x + self.viewport.doc_to_screen((*g as f32, 0.0)).0 as i32;
                    if (mx - sx).abs() <= 3 {
                        self.guide_drag = Some((true, *g, Some(i)));
                        self.drag = DragMode::GuideDrag;
                        return;
                    }
                }
                for (i, g) in self.doc.guides_y.iter().enumerate() {
                    let sy = c.y + self.viewport.doc_to_screen((0.0, *g as f32)).1 as i32;
                    if (my - sy).abs() <= 3 {
                        self.guide_drag = Some((false, *g, Some(i)));
                        self.drag = DragMode::GuideDrag;
                        return;
                    }
                }
            }
        }
        // Tool preset picker popup: a row applies, any
        // other click dismisses.
        if self.tool_preset_popup {
            self.tool_preset_popup = false;
            for i in 0..self.tool_presets.len() {
                if let Some(rr) = self.tool_preset_row_rect(i) {
                    if rr.contains(mx, my) {
                        self.apply_tool_preset(i);
                        return;
                    }
                }
            }
            return;
        }
        // ---- options bar (Phase 6.5 Batch 5)
        if self.options_press(mx, my) {
            return;
        }
        // ---- dock panels: tab strips, float headers, title handles
        if self.panel_strip_mouse(mx, my) {
            return;
        }
        if self.panel_title_mouse(mx, my) {
            return;
        }
        if self.panel_mouse(mx, my) {
            return;
        }
        // ---- menu bar / open dropdown
        if self.display_mode < 2 && my < ui::MENU_H {
            let titles = self.menu_title_rects();
            for (r, i) in titles {
                if r.contains(mx, my) {
                    self.open_menu = if self.open_menu == Some(i) {
                        None
                    } else {
                        Some(i)
                    };
                    self.open_submenu = None;
                    self.drag = DragMode::MenuNav;
                    return;
                }
            }
            self.open_menu = None;
            return;
        }
        if self.open_menu.is_some() {
            if self.menu_click(mx, my) {
                return;
            }
            self.open_menu = None;
            self.open_submenu = None;
            return;
        }
        // ---- toolbox
        if mx < ui::TOOLBAR_W {
            for (i, t) in TOOLS.iter().enumerate() {
                if Self::tool_rect(i, self.top_y()).contains(mx, my) {
                    if self.ft.is_some() {
                        self.ft_commit();
                    }
                    self.tool = *t;
                    self.status = format!("Tool: {}", t.name());
                    return;
                }
            }
            if self.swatches_area().contains(mx, my) {
                let (fr, fgc, fb, _) = self.fg.to_rgba8();
                if !Self::web_safe((fr, fgc, fb))
                    && Self::web_warn_rect(self.win_size.1 as i32).contains(mx, my)
                {
                    let snap = |v: u8| ((v as u16 + 25) / 51 * 51).min(255) as u8;
                    self.fg = Pixel16::from_rgba8(snap(fr), snap(fgc), snap(fb), 255);
                    self.status = "Foreground snapped to the nearest web-safe color".into();
                } else if self.swatt_default_hit(mx, my) {
                    self.fg = Pixel16::from_rgba8(0, 0, 0, 255);
                    self.bg = Pixel16::from_rgba8(255, 255, 255, 255);
                } else {
                    self.show_palette = !self.show_palette;
                }
                return;
            }
            return;
        }
        // ---- canvas
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        if !canvas.contains(mx, my) {
            return;
        }
        let dp = self.to_doc(x, y);
        self.drag_start_doc = dp;
        self.drag_cur_doc = dp;
        if let Some(quad) = self.persp_crop {
            let tol = 10.0 / self.viewport.scale.max(0.01);
            for (i, c) in quad.iter().enumerate() {
                if (dp.0 - c.0).abs() < tol && (dp.1 - c.1).abs() < tol {
                    self.persp_drag = Some(i);
                    return;
                }
            }
            return;
        }
        if self.ft.is_some() && self.ft_mouse_down(dp, x, y) {
            return;
        }
        if self.space_down {
            self.drag = DragMode::Pan;
            return;
        }
        // Phase 6 layer locks: image lock blocks painting
        // tools, position lock blocks the Move tool. (The
        // transparent-pixels lock is stored and shown but not
        // enforced by the brush engine in v1.)
        {
            let locks = self.doc.layers[self.doc.active_layer].locks;
            let paints = matches!(
                self.tool,
                Tool::Brush
                    | Tool::Pencil
                    | Tool::Eraser
                    | Tool::MagicEraser
                    | Tool::BackgroundEraser
                    | Tool::HistoryBrush
                    | Tool::ArtHistoryBrush
                    | Tool::Stamp
                    | Tool::PatternStamp
                    | Tool::Bucket
                    | Tool::Gradient
                    | Tool::Dodge
                    | Tool::Burn
                    | Tool::Sponge
                    | Tool::Smudge
                    | Tool::Blur
                    | Tool::Sharpen
            );
            if (locks & 0xA) != 0 && paints {
                self.status = "Layer is locked (image pixels)".into();
                return;
            }
            if (locks & 0xC) != 0 && self.tool == Tool::Move {
                self.status = "Layer is locked (position)".into();
                return;
            }
        }
        match self.tool {
            Tool::Hand => self.drag = DragMode::Pan,
            Tool::Zoom => {
                let local = (x - canvas.x as f32, y - canvas.y as f32);
                self.viewport
                    .zoom_at(local, if self.alt_down { 0.5 } else { 2.0 });
            }
            Tool::Eyedropper => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    if let Some(ch) = self.doc.alphas.get(a) {
                        let v = ch.value(self.doc.width, dp.0.floor() as i32, dp.1.floor() as i32);
                        self.fg = Pixel16::from_rgba8(v, v, v, 255);
                        self.status = format!("Channel value: {v}");
                        return;
                    }
                }
                let (px, py) = (dp.0.floor() as i32, dp.1.floor() as i32);
                let p = if self.dropper_sample_merged {
                    self.doc.render_pixel(px, py)
                } else {
                    let (r, g, b, a) = self.doc.content_pixel(self.doc.active_layer, px, py);
                    Pixel16::from_f32s(r, g, b, a)
                };
                if self.alt_down {
                    self.bg = p;
                } else {
                    self.fg = p;
                }
                self.status = format!(
                    "Color: {} {} {}",
                    p.to_rgba8().0,
                    p.to_rgba8().1,
                    p.to_rgba8().2
                );
            }
            Tool::Wand => {
                let contiguous = !self.alt_down;
                let w = Selection::wand(&self.doc, dp.0 as i32, dp.1 as i32, 32, contiguous);
                self.replace_sel(Some(w));
                self.status = "Magic Wand selection".into();
            }
            Tool::MagicEraser => {
                let layer = self.doc.active_layer;
                let tol = self.eraser_tolerance;
                let contiguous = !self.alt_down;
                let tiles = self.active_tiles_all();
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Magic Eraser",
                    layer,
                    &tiles,
                    |d| {
                        brush::erase_magic(d, layer, dp.0 as i32, dp.1 as i32, tol, contiguous);
                    },
                );
                self.last_label = Some("Magic Eraser".into());
                self.status = "Magic Eraser".into();
                self.touch();
            }
            Tool::ColorSampler => {
                let (sx, sy) = (dp.0.floor() as i32, dp.1.floor() as i32);
                // Clicking near an existing sampler moves it.
                let near = self
                    .doc
                    .samplers
                    .iter()
                    .position(|(x, y)| (x - sx).abs() <= 3 && (y - sy).abs() <= 3);
                if let Some(i) = near {
                    self.doc.samplers[i] = (sx, sy);
                    self.status = format!("Sampler {} moved", i + 1);
                } else if self.doc.samplers.len() < 4 {
                    self.doc.samplers.push((sx, sy));
                    self.status = format!("Sampler {} set", self.doc.samplers.len());
                    if !self.show_info {
                        self.show_info = true;
                    }
                } else {
                    self.status = "Four samplers maximum (click one to move it)".into();
                }
            }
            Tool::Measure => {
                self.measure = Some(((dp.0, dp.1), (dp.0, dp.1)));
                self.drag = DragMode::MeasureDrag;
                if !self.show_info {
                    self.show_info = true;
                }
            }
            Tool::Pen => {
                self.pen_down(x, y, dp);
            }
            Tool::Type | Tool::VertType | Tool::TypeMask | Tool::VertTypeMask => {
                let vertical = matches!(self.tool, Tool::VertType | Tool::VertTypeMask);
                let mask = matches!(self.tool, Tool::TypeMask | Tool::VertTypeMask);
                self.open_type_dialog(dp, vertical, mask);
            }
            Tool::PolyLasso => {
                // Click-click-click; clicking the first anchor (or
                // double-clicking) closes the selection.
                let close = self.poly_points.len() >= 3
                    && (dp.0 - self.poly_points[0].0).abs() < 6.0
                    && (dp.1 - self.poly_points[0].1).abs() < 6.0;
                if close {
                    self.close_poly_selection();
                } else {
                    self.poly_points.push(dp);
                }
            }
            Tool::MagneticLasso => {
                let p = self.mag_snap(dp);
                self.poly_points = vec![p];
                self.drag = DragMode::MagLasso;
            }
            Tool::FreeformPen => {
                self.begin_path_edit();
                self.ensure_pen_target();
                self.pen_free = vec![dp];
                self.drag = DragMode::FreePen;
            }
            Tool::MagneticPen => {
                self.begin_path_edit();
                self.ensure_pen_target();
                let p = self.mag_snap(dp);
                self.poly_points = vec![p];
                self.drag = DragMode::MagPen;
            }
            Tool::Bucket => {
                if let PaintTarget::Mask(li) = self.paint_target {
                    let mut cmd = DocumentCommand::begin("Paint Bucket", &self.doc);
                    let (w, h) = (self.doc.width, self.doc.height);
                    let v = self.fg_luminance();
                    if li < self.doc.layers.len() {
                        brush::flood_mask(
                            &mut self.doc.layers[li],
                            w,
                            h,
                            dp.0 as i32,
                            dp.1 as i32,
                            v,
                            32,
                        );
                    }
                    cmd.finish(&self.doc);
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some("Paint Bucket".into());
                    self.status = "Paint Bucket (mask)".into();
                    self.touch();
                    return;
                }
                if let PaintTarget::Alpha(a) = self.paint_target {
                    let mut cmd = DocumentCommand::begin("Paint Bucket", &self.doc);
                    let (w, h) = (self.doc.width, self.doc.height);
                    let v = self.fg_luminance();
                    if let Some(ch) = self.doc.alphas.get_mut(a) {
                        brush::flood_channel(&mut ch.data, w, h, dp.0 as i32, dp.1 as i32, v, 32);
                    }
                    cmd.finish(&self.doc);
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some("Paint Bucket".into());
                    self.status = "Paint Bucket (channel)".into();
                    self.touch();
                    return;
                }
                let region = Selection::wand(&self.doc, dp.0 as i32, dp.1 as i32, 32, true);
                if let Some((bx, by, bw, bh)) = region.bounds() {
                    let tiles = Selection::tiles_in_rect(bx, by, bw, bh);
                    let layer = self.doc.active_layer;
                    let fg = self.fg;
                    run_tile_edit(
                        &mut self.doc,
                        &mut self.hist,
                        "Paint Bucket",
                        layer,
                        &tiles,
                        |d| {
                            brush::fill_selection(d, layer, &region, fg);
                        },
                    );
                    self.last_label = Some("Paint Bucket".into());
                    self.status = "Paint Bucket".into();
                    self.touch();
                }
            }
            Tool::ShapeRect
            | Tool::ShapeRounded
            | Tool::ShapeEllipse
            | Tool::ShapePolygon
            | Tool::ShapeLine
            | Tool::ShapeCustom => {
                self.shape_press(dp);
            }
            Tool::Slice => {
                self.begin_path_edit();
                let (x, y) = (dp.0.floor() as i32, dp.1.floor() as i32);
                let n = self.doc.slices.len() + 1;
                self.doc.slices.push(pictodesk_core::document::SliceRect {
                    x,
                    y,
                    w: 1,
                    h: 1,
                    name: format!("slice{n}"),
                    url: String::new(),
                    alt: String::new(),
                });
                self.slice_selected = Some(self.doc.slices.len() - 1);
                self.drag = DragMode::SliceDrag;
                self.touch();
            }
            Tool::SliceSelect => {
                let (x, y) = (dp.0.floor() as i32, dp.1.floor() as i32);
                let hit = self
                    .doc
                    .slices
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, sl)| {
                        x >= sl.x && x < sl.x + sl.w as i32 && y >= sl.y && y < sl.y + sl.h as i32
                    })
                    .map(|(i, _)| i);
                if let Some(i) = hit {
                    if self.slice_selected == Some(i) {
                        // Second press on the selected slice:
                        // open its properties (name / URL / alt).
                        self.do_action(Action::SlicePropsDialog);
                        return;
                    }
                    self.slice_selected = Some(i);
                    self.slice_orig = (self.doc.slices[i].x, self.doc.slices[i].y);
                    self.begin_path_edit();
                    self.drag = DragMode::SliceDrag;
                    self.status = format!("Slice: {}", self.doc.slices[i].name);
                } else {
                    self.slice_selected = None;
                }
            }
            Tool::Notes => {
                let (x, y) = (dp.0.floor() as i32, dp.1.floor() as i32);
                let hit = self
                    .doc
                    .notes
                    .iter()
                    .enumerate()
                    .find(|(_, n)| (n.x - x).abs() <= 6 && (n.y - y).abs() <= 6)
                    .map(|(i, _)| i);
                let mut d = Dialog::new(DialogKind::NoteEdit, &self.doc);
                match hit {
                    Some(i) => {
                        d.note_target = Some(i);
                        d.text = self.doc.notes[i].text.clone();
                        d.note_audio = self.doc.notes[i].audio.clone();
                        if let Some(wav) = self.doc.notes[i].audio.clone() {
                            self.play_note_audio(&wav);
                        }
                    }
                    None => {
                        d.type_anchor = (x, y);
                    }
                }
                d.text_focus = true;
                self.dialog = Some(d);
            }
            Tool::MarqueeRect | Tool::MarqueeEllipse | Tool::MarqueeRow | Tool::MarqueeCol => {
                self.drag = DragMode::SelectRect;
            }
            Tool::ConvertAnchor => {
                self.convert_anchor_down(x, y);
            }
            Tool::Lasso => {
                self.drag = DragMode::Lasso;
                self.lasso_points = vec![dp];
            }
            Tool::Gradient => {
                self.drag = DragMode::GradientLine;
            }
            Tool::Move => {
                self.drag = DragMode::MoveLayer;
                self.move_orig_offset = self.doc.layers[self.doc.active_layer].offset;
                let a = self.doc.active_layer;
                self.move_linked = self
                    .doc
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(i, l)| *i != a && l.linked)
                    .map(|(i, l)| (i, l.offset))
                    .collect();
            }
            Tool::Patch => {
                if self.sel.is_some() {
                    self.drag = DragMode::PatchDrag;
                } else {
                    self.status = "The Patch tool needs a selection first".into();
                }
            }
            Tool::Brush
            | Tool::Airbrush
            | Tool::Pencil
            | Tool::Eraser
            | Tool::Heal
            | Tool::Stamp
            | Tool::HistoryBrush
            | Tool::ArtHistoryBrush
            | Tool::PatternStamp
            | Tool::BackgroundEraser
            | Tool::Blur
            | Tool::Sharpen
            | Tool::Smudge
            | Tool::Dodge
            | Tool::Burn
            | Tool::Sponge => {
                if let PaintTarget::Mask(_) = self.paint_target {
                    if matches!(
                        self.tool,
                        Tool::Brush | Tool::Airbrush | Tool::Pencil | Tool::Eraser
                    ) {
                        self.alpha_cmd = Some(DocumentCommand::begin("Paint on mask", &self.doc));
                        self.alpha_last = dp;
                        self.mask_stamp(dp);
                        self.drag = DragMode::Paint;
                    } else {
                        self.status = "That tool paints on the image, not on a mask".into();
                    }
                    return;
                }
                if let PaintTarget::Alpha(_) = self.paint_target {
                    if matches!(
                        self.tool,
                        Tool::Brush | Tool::Airbrush | Tool::Pencil | Tool::Eraser
                    ) {
                        self.alpha_cmd =
                            Some(DocumentCommand::begin("Paint on channel", &self.doc));
                        self.alpha_last = dp;
                        self.alpha_stamp(dp);
                        self.drag = DragMode::Paint;
                    } else {
                        self.status = "That tool paints on the image, not on a channel".into();
                    }
                    return;
                }
                if self.tool == Tool::Stamp && self.alt_down {
                    self.clone_src = Some(dp);
                    self.status = "Clone source set".into();
                    return;
                }
                if self.tool == Tool::Stamp && self.clone_src.is_none() {
                    self.status = "Alt+click to set clone source first".into();
                    return;
                }
                if self.tool == Tool::Heal && !self.heal_use_pattern {
                    if !matches!(
                        self.doc.mode,
                        pictodesk_core::document::ColorMode::Rgb
                            | pictodesk_core::document::ColorMode::Grayscale
                    ) {
                        self.status = "Healing works in RGB and Grayscale documents for now".into();
                        return;
                    }
                    if self.alt_down {
                        self.heal_src = Some(dp);
                        self.status = "Heal source set".into();
                        return;
                    }
                    if self.heal_src.is_none() {
                        self.status = "Alt+click to set the heal source first".into();
                        return;
                    }
                }
                if self.tool == Tool::PatternStamp && self.doc.pattern.is_none() {
                    self.status =
                        "Define a pattern first (select, then Edit > Define Pattern)".into();
                    return;
                }
                self.begin_stroke(dp);
                self.drag = DragMode::Paint;
            }
        }
    }

    pub(crate) fn mouse_move(&mut self, x: f32, y: f32) {
        let (mx, my) = (x as i32, y as i32);
        let prev = self.cursor;
        self.cursor = (x, y);
        // Toolbox hover: name the tool under the cursor in the
        // status bar (with its key and, for source tools, what to
        // do next) so tools are discoverable without guessing.
        if self.drag == DragMode::None && self.dialog.is_none() && mx < ui::TOOLBAR_W {
            for (i, t) in TOOLS.iter().enumerate() {
                if Self::tool_rect(i, self.top_y()).contains(mx, my) {
                    let extra = match *t {
                        Tool::Stamp if self.clone_src.is_none() => " - Alt+click to set the source",
                        Tool::Stamp => " - source set",
                        Tool::Heal if !self.heal_use_pattern && self.heal_src.is_none() => {
                            " - Alt+click to set the source"
                        }
                        Tool::Heal if !self.heal_use_pattern => " - source set",
                        _ => "",
                    };
                    self.status = format!("{} ({}){}", t.name(), t.key(), extra);
                    break;
                }
            }
        }
        // Hover tooltips: tool name + shortcut over the toolbox,
        // control name over the Options Bar. The bubble appears
        // after a short hover; set_hover tracks the target.
        let mut hover: Option<String> = None;
        if self.show_tooltips && self.dialog.is_none() && self.display_mode == 0 {
            if mx < ui::TOOLBAR_W {
                for (i, t) in TOOLS.iter().enumerate() {
                    if Self::tool_rect(i, self.top_y()).contains(mx, my) {
                        hover = Some(format!("{} ({})", t.name(), t.key().to_ascii_uppercase()));
                        break;
                    }
                }
            } else if let Some(bar) = self.options_bar_rect() {
                if bar.contains(mx, my) {
                    for (r, _, _, act) in self.options_items() {
                        if r.contains(mx, my) {
                            hover = Some(opt_act_tooltip(act));
                            break;
                        }
                    }
                }
            }
        }
        self.set_hover(hover);
        // Panel drag: a press on a tab/title starts a drag past a
        // 4px threshold; the dragged float follows the cursor and
        // shows its drop target (merge / dock / stay floating).
        if let Some((kind, loc, dx, dy)) = self.panel_pending {
            if (mx - dx).abs() + (my - dy).abs() > 4 && self.dialog.is_none() {
                let (idx, gx, gy) = self.detach_panel(kind, loc, dx, dy, mx, my);
                self.panel_drag = Some((idx, gx, gy));
                self.panel_pending = None;
            }
        }
        if let Some((idx, gdx, gdy)) = self.panel_drag {
            if let Some((_, fx, fy, _)) = self.float_slots.get_mut(idx) {
                *fx = (mx - gdx).clamp(0, self.win_size.0 as i32 - 60);
                *fy = (my - gdy).clamp(0, self.win_size.1 as i32 - 40);
            }
            self.panel_drop = self.compute_drop(mx, my, idx);
            return;
        }
        if self.drag == DragMode::NavDrag {
            self.navigator_to(mx, my);
            return;
        }
        if self.drag == DragMode::GuideDrag {
            let dp = self.to_doc(x, y);
            if let Some(g) = &mut self.guide_drag {
                g.1 = if g.0 { dp.0 as i32 } else { dp.1 as i32 };
            }
            return;
        }
        if self.drag == DragMode::MeasureDrag {
            let dp = self.to_doc(x, y);
            if let Some(m) = &mut self.measure {
                m.1 = dp;
            }
            return;
        }
        if self.drag == DragMode::MagLasso || self.drag == DragMode::MagPen {
            let dp = self.to_doc(x, y);
            let far = self
                .poly_points
                .last()
                .map(|l| (dp.0 - l.0).abs() > 8.0 || (dp.1 - l.1).abs() > 8.0)
                .unwrap_or(true);
            if far {
                let p = self.mag_snap(dp);
                self.poly_points.push(p);
            }
            return;
        }
        if self.drag == DragMode::FreePen {
            let dp = self.to_doc(x, y);
            let far = self
                .pen_free
                .last()
                .map(|l| (dp.0 - l.0).abs() > 2.0 || (dp.1 - l.1).abs() > 2.0)
                .unwrap_or(true);
            if far {
                self.pen_free.push(dp);
            }
            return;
        }
        if let Some(ci) = self.persp_drag {
            if self.persp_crop.is_some() {
                let dp = self.to_doc(x, y);
                if let Some(quad) = &mut self.persp_crop {
                    quad[ci] = dp;
                }
                return;
            }
        }
        if self.ft.is_some() {
            let dp = self.to_doc(x, y);
            self.ft_mouse_move(dp);
            return;
        }
        // dialog drags
        if self.dialog.is_some() {
            let layout = self.dialog_layout().unwrap();
            // Extract editor: drag paints with the current tool.
            if self.mouse_down
                && matches!(
                    self.dialog.as_ref().map(|d| &d.kind),
                    Some(DialogKind::Extract)
                )
            {
                if let Some(dp) = self.extract_doc_point(mx, my) {
                    let mode = self.dialog.as_ref().unwrap().extract_mode;
                    if mode != 1 {
                        self.extract_paint(dp);
                    }
                }
                return;
            }
            // Liquify editor: drag keeps stamping.
            if self.mouse_down
                && matches!(
                    self.dialog.as_ref().map(|d| &d.kind),
                    Some(DialogKind::Liquify)
                )
            {
                if let Some(dp) = self.liquify_doc_point(mx, my) {
                    self.liquify_stamp(dp);
                }
                return;
            }
            // 3D Transform wireframe drag: horizontal movement
            // turns the solid around Y, vertical around X, at
            // half a degree per pixel, wrapping at +-180 so the
            // drag never sticks at the slider ends. The vals
            // feed both the sliders and the live preview.
            let wire_moved = {
                let d = self.dialog.as_mut().unwrap();
                if let Some((lx, ly)) = d.wire_drag {
                    let (dx, dy) = (mx - lx, my - ly);
                    d.wire_drag = Some((mx, my));
                    let wrap = |v: f32| ((v + 180.0).rem_euclid(360.0)) - 180.0;
                    d.vals[1] = wrap(d.vals[1] + dx as f32 * 0.5);
                    d.vals[0] = wrap(d.vals[0] + dy as f32 * 0.5);
                    true
                } else {
                    false
                }
            };
            if wire_moved {
                self.dialog_refresh_preview();
                return;
            }
            if let Some(sd) = self.dialog.as_ref().unwrap().slider_drag {
                if let Some(row) = layout.slider_rows.get(sd) {
                    let t = self.dialog_slider_value(*row, mx);
                    self.set_dialog_slider(sd, t);
                }
            }
            if let Some(g) = layout.graph {
                let d = self.dialog.as_mut().unwrap();
                if let Some(ci) = d.curve_drag {
                    let gx = ((mx - g.x) as f32 / g.w as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    let gy =
                        (255.0 - (my - g.y) as f32 / g.h as f32 * 255.0).clamp(0.0, 255.0) as u8;
                    let n = d.curve_points.len();
                    if ci > 0 && ci < n - 1 {
                        d.curve_points[ci] = (gx, gy);
                    } else {
                        d.curve_points[ci].1 = gy; // endpoints keep their x
                    }
                    self.dialog_refresh_preview();
                }
            }
            return;
        }
        // menu hover
        if self.open_menu.is_some() {
            let mi = self.open_menu.unwrap();
            let menus = self.build_menus();
            let dr = self.dropdown_rect(mi);
            self.hover_item = None;
            self.hover_sub_item = None;
            for (ii, item) in menus[mi].items.iter().enumerate() {
                let ir = Rect::new(dr.x + 2, dr.y + 3 + ii as i32 * 17, dr.w - 4, 17);
                if ir.contains(mx, my) {
                    self.hover_item = Some((mi, ii));
                    if item.submenu.is_some() {
                        self.open_submenu = Some(ii);
                    } else if self.open_submenu.is_some() {
                        // keep submenu while hovering its panel
                        let si = self.open_submenu.unwrap();
                        if let Some(sub) = &menus[mi].items[si].submenu {
                            let parent =
                                Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                            let sw = sub
                                .iter()
                                .map(|it| Frame::text_width(&it.label, 1) + 40)
                                .max()
                                .unwrap_or(120);
                            let sr = Rect::new(
                                dr.right() - 2,
                                parent.y - 3,
                                sw,
                                sub.len() as i32 * 17 + 6,
                            );
                            if !sr.contains(mx, my) {
                                self.open_submenu = None;
                            }
                        }
                    }
                }
            }
            if let Some(si) = self.open_submenu {
                if let Some(sub) = &menus[mi].items[si].submenu {
                    let parent = Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                    let sw = sub
                        .iter()
                        .map(|it| {
                            Frame::text_width(&it.label, 1)
                                + it.shortcut
                                    .as_deref()
                                    .map(|sc| Frame::text_width(sc, 1) + 12)
                                    .unwrap_or(0)
                                + 40
                        })
                        .max()
                        .unwrap_or(120);
                    let sr = Rect::new(dr.right() - 2, parent.y - 3, sw, sub.len() as i32 * 17 + 6);
                    for (ii, _) in sub.iter().enumerate() {
                        let ir = Rect::new(sr.x + 2, sr.y + 3 + ii as i32 * 17, sr.w - 4, 17);
                        if ir.contains(mx, my) {
                            self.hover_sub_item = Some(ii);
                            self.hover_item = Some((mi, si));
                        }
                    }
                }
            }
            // slide across menu titles
            if self.display_mode < 2 && my < ui::MENU_H && self.mouse_down {
                for (r, i) in self.menu_title_rects() {
                    if r.contains(mx, my) && self.open_menu != Some(i) {
                        self.open_menu = Some(i);
                        self.open_submenu = None;
                    }
                }
            }
            return;
        }
        // file panel hover
        if self.file_panel.is_some() {
            let pr = self.file_panel_rect();
            self.hover_file = None;
            let rows = self.file_panel_rows();
            for i in 0..rows.len().min(14) {
                let ir = Rect::new(pr.x + 6, pr.y + 44 + i as i32 * 17, pr.w - 12, 17);
                if ir.contains(mx, my) {
                    self.hover_file = Some(i);
                }
            }
            return;
        }
        if !self.mouse_down {
            return;
        }
        let dp = self.to_doc(x, y);
        match self.drag {
            DragMode::Pan => {
                self.viewport.pan_by((x - prev.0, y - prev.1));
            }
            DragMode::PenNew | DragMode::PenDrag | DragMode::PenConvert => {
                self.pen_move(dp);
            }
            DragMode::SelectRect | DragMode::GradientLine | DragMode::PatchDrag => {
                self.drag_cur_doc = dp;
            }
            DragMode::Lasso => {
                self.drag_cur_doc = dp;
                if let Some(last) = self.lasso_points.last() {
                    if (last.0 - dp.0).abs() + (last.1 - dp.1).abs() > 1.5 {
                        self.lasso_points.push(dp);
                    }
                }
            }
            DragMode::ShapeDrag => {
                if let Some(li) = self.shape_drag_layer {
                    let (a, b) = (self.drag_start_doc, dp);
                    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                    if let Some(sp) = &mut self.doc.layers[li].shape {
                        if sp.kind == 4 {
                            // Lines keep their true diagonal.
                            sp.rect = (
                                a.0.floor() as i32,
                                a.1.floor() as i32,
                                (b.0 - a.0).round() as i32,
                                (b.1 - a.1).round() as i32,
                            );
                        } else {
                            sp.rect = (
                                x0.floor() as i32,
                                y0.floor() as i32,
                                (x1 - x0).round().max(1.0) as i32,
                                (y1 - y0).round().max(1.0) as i32,
                            );
                        }
                    }
                    self.touch();
                }
            }
            DragMode::SliceDrag => {
                if let Some(i) = self.slice_selected {
                    let (a, b) = (self.drag_start_doc, dp);
                    if self.tool == Tool::SliceSelect {
                        let dx = (b.0 - a.0).round() as i32;
                        let dy = (b.1 - a.1).round() as i32;
                        self.doc.slices[i].x = self.slice_orig.0 + dx;
                        self.doc.slices[i].y = self.slice_orig.1 + dy;
                    } else {
                        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                        self.doc.slices[i].x = x0.floor() as i32;
                        self.doc.slices[i].y = y0.floor() as i32;
                        self.doc.slices[i].w = (x1 - x0).round().max(1.0) as u32;
                        self.doc.slices[i].h = (y1 - y0).round().max(1.0) as u32;
                    }
                    self.touch();
                }
            }
            DragMode::MoveLayer => {
                let layer = self.doc.active_layer;
                let mut dx = (dp.0 - self.drag_start_doc.0).round() as i32;
                let mut dy = (dp.1 - self.drag_start_doc.1).round() as i32;
                if self.snap_enabled {
                    // Snap the grab point to guides (and to the grid
                    // while it's shown) within a few screen pixels.
                    let thresh = 6.0 / self.viewport.scale.max(0.01);
                    let grab = (
                        self.drag_start_doc.0 + dx as f32,
                        self.drag_start_doc.1 + dy as f32,
                    );
                    let mut best: Option<(f32, f32)> = None;
                    let mut consider = |cand: f32, is_x: bool| {
                        let g = if is_x { grab.0 } else { grab.1 };
                        let d = (cand - g).abs();
                        if d <= thresh && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                            best = Some((d, cand));
                        }
                    };
                    for g in &self.doc.guides_x {
                        consider(*g as f32, true);
                    }
                    if self.show_grid {
                        let step = 64.0f32;
                        consider((grab.0 / step).round() * step, true);
                    }
                    if let Some((_, cand)) = best {
                        dx = (cand - self.drag_start_doc.0).round() as i32;
                    }
                    let mut besty: Option<(f32, f32)> = None;
                    for g in &self.doc.guides_y {
                        let d = (*g as f32 - grab.1).abs();
                        if d <= thresh && besty.map(|(bd, _)| d < bd).unwrap_or(true) {
                            besty = Some((d, *g as f32));
                        }
                    }
                    if self.show_grid {
                        let step = 64.0f32;
                        let cand = (grab.1 / step).round() * step;
                        let d = (cand - grab.1).abs();
                        if d <= thresh && besty.map(|(bd, _)| d < bd).unwrap_or(true) {
                            besty = Some((d, cand));
                        }
                    }
                    if let Some((_, cand)) = besty {
                        dy = (cand - self.drag_start_doc.1).round() as i32;
                    }
                }
                self.doc.layers[layer].offset =
                    (self.move_orig_offset.0 + dx, self.move_orig_offset.1 + dy);
                for (li, orig) in &self.move_linked {
                    self.doc.layers[*li].offset = (orig.0 + dx, orig.1 + dy);
                }
                self.doc_version += 1;
            }
            DragMode::Paint => {
                if self.alpha_cmd.is_some() {
                    let last = self.alpha_last;
                    let spacing = (self.tip.size * 0.25).max(1.0);
                    for c in brush::stroke_centers(&[last, dp], spacing) {
                        if c != last {
                            if matches!(self.paint_target, PaintTarget::Mask(_)) {
                                self.mask_stamp(c);
                            } else {
                                self.alpha_stamp(c);
                            }
                        }
                    }
                    self.alpha_last = dp;
                    return;
                }
                // Smoothing (Phase 7): the stroke chases a
                // lagged point, which irons out hand jitter.
                let dp = if self.brush_dyn.smoothing {
                    let prev = self.stroke.as_ref().unwrap().smooth;
                    let sm = (
                        prev.0 + (dp.0 - prev.0) * 0.45,
                        prev.1 + (dp.1 - prev.1) * 0.45,
                    );
                    if let Some(s) = &mut self.stroke {
                        s.smooth = sm;
                    }
                    sm
                } else {
                    dp
                };
                // capture tiles BEFORE stamping this segment
                self.capture_segment_tiles(dp);
                let last = self.stroke.as_ref().unwrap().last_point;
                let spacing = (self.tip.size * self.brush_dyn.spacing).max(1.0);
                for c in brush::stroke_centers(&[last, dp], spacing) {
                    if c != last {
                        self.stamp_at(c);
                    }
                }
                if let Some(s) = &mut self.stroke {
                    s.last_point = dp;
                }
            }
            _ => {}
        }
    }

    /// Snapshot (before-state) every tile the next stamp segment could
    /// touch, so undo restores the pre-stroke pixels exactly.
    pub(crate) fn capture_segment_tiles(&mut self, to: (f32, f32)) {
        let layer = self.doc.active_layer;
        let r = self.tip.size / 2.0 + self.brush_dyn.scatter * self.tip.size + 2.0;
        let (x0, y0) = ((to.0 - r).floor() as i32, (to.1 - r).floor() as i32);
        let (x1, y1) = ((to.0 + r).ceil() as i32, (to.1 + r).ceil() as i32);
        let tiles = Selection::tiles_in_rect(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
        // also the segment start area
        let last = self.stroke.as_ref().map(|s| s.last_point).unwrap_or(to);
        let (lx0, ly0) = ((last.0 - r).floor() as i32, (last.1 - r).floor() as i32);
        let (lx1, ly1) = ((last.0 + r).ceil() as i32, (last.1 + r).ceil() as i32);
        let tiles2 = Selection::tiles_in_rect(lx0, ly0, (lx1 - lx0) as u32, (ly1 - ly0) as u32);
        if let Some(s) = &mut self.stroke {
            for t in tiles.into_iter().chain(tiles2) {
                if !s.tiles_seen.contains(&t) {
                    s.cmd.capture_before(&self.doc, layer, t);
                    s.tiles_seen.push(t);
                }
            }
        }
    }

    pub(crate) fn mouse_up(&mut self, _x: f32, _y: f32) {
        self.mouse_down = false;
        if let Some((idx, _, _)) = self.panel_drag.take() {
            match self.panel_drop.take() {
                Some(PanelDrop::Merge(loc)) => self.merge_float_into(idx, loc),
                Some(PanelDrop::DockAt(y)) => self.redock_float_at(idx, y),
                None => {}
            }
            self.save_panel_layout();
        }
        self.panel_pending = None;
        if self.drag == DragMode::PatchDrag {
            self.drag = DragMode::None;
            let (dx, dy) = (
                (self.drag_cur_doc.0 - self.drag_start_doc.0).round() as i32,
                (self.drag_cur_doc.1 - self.drag_start_doc.1).round() as i32,
            );
            if dx != 0 || dy != 0 {
                if !matches!(
                    self.doc.mode,
                    pictodesk_core::document::ColorMode::Rgb
                        | pictodesk_core::document::ColorMode::Grayscale
                ) {
                    self.status = "Patch works in RGB and Grayscale documents for now".into();
                } else {
                    self.filter_now("Patch", None, move |doc, layer, sel| {
                        if let Some(s) = sel {
                            pictodesk_core::heal::heal_patch(doc, layer, s, dx, dy);
                        }
                    });
                    self.status = "Patched".into();
                }
            }
            return;
        }
        if let Some(ft) = &mut self.ft {
            ft.drag = FtDrag::None;
        }
        if self.drag == DragMode::GuideDrag {
            self.drag = DragMode::None;
            if let Some((vertical, pos, existing)) = self.guide_drag.take() {
                // Dropping over the canvas keeps the guide; dropping
                // outside removes it (for an existing guide, deletes).
                let c = self.canvas_rect(self.win_size.0, self.win_size.1);
                let inside = c.contains(_x as i32, _y as i32);
                match (existing, inside) {
                    (None, true) => {
                        self.structural("Add Guide", |d| {
                            if vertical {
                                d.guides_x.push(pos);
                            } else {
                                d.guides_y.push(pos);
                            }
                        });
                        self.status = "Guide added — drag it with the Move tool".into();
                    }
                    (Some(i), true) => {
                        self.structural("Move Guide", |d| {
                            if vertical {
                                if i < d.guides_x.len() {
                                    d.guides_x[i] = pos;
                                }
                            } else if i < d.guides_y.len() {
                                d.guides_y[i] = pos;
                            }
                        });
                    }
                    (Some(i), false) => {
                        self.structural("Delete Guide", |d| {
                            if vertical {
                                if i < d.guides_x.len() {
                                    d.guides_x.remove(i);
                                }
                            } else if i < d.guides_y.len() {
                                d.guides_y.remove(i);
                            }
                        });
                    }
                    (None, false) => {}
                }
            }
            return;
        }
        if self.drag == DragMode::NavDrag {
            self.drag = DragMode::None;
            return;
        }
        self.persp_drag = None;
        if let Some(d) = &mut self.dialog {
            d.slider_drag = None;
            d.curve_drag = None;
            d.wire_drag = None;
            d.liq_last = None;
        }
        match self.drag {
            DragMode::PenNew | DragMode::PenDrag | DragMode::PenConvert => {
                let label = if self.drag == DragMode::PenNew {
                    "Add Anchor"
                } else if self.drag == DragMode::PenConvert {
                    "Convert Anchor Point"
                } else {
                    "Edit Path"
                };
                self.drag = DragMode::None;
                self.pen_hit = None;
                self.commit_path_edit(label);
            }
            DragMode::Paint => {
                if let Some(mut cmd) = self.alpha_cmd.take() {
                    cmd.finish(&self.doc);
                    self.push_cmd(Box::new(cmd));
                    self.last_label = Some("Paint on channel".into());
                    self.status = "Paint on channel".into();
                    self.touch();
                } else if let Some(mut s) = self.stroke.take() {
                    let layer = self.doc.active_layer;
                    for t in &s.tiles_seen {
                        s.cmd.capture_after(&self.doc, layer, *t);
                    }
                    let label = self.tool.name().to_string();
                    self.push_cmd(Box::new(s.cmd));
                    self.last_label = Some(label.clone());
                    self.status = label;
                    self.touch();
                }
            }
            DragMode::SelectRect => {
                if self.tool == Tool::MarqueeRow {
                    let y = (self.drag_cur_doc.1 as i32).clamp(0, self.doc.height as i32 - 1);
                    self.replace_sel(Some(Selection::rect(
                        self.doc.width,
                        self.doc.height,
                        0,
                        y,
                        self.doc.width,
                        1,
                    )));
                    return;
                }
                if self.tool == Tool::MarqueeCol {
                    let x = (self.drag_cur_doc.0 as i32).clamp(0, self.doc.width as i32 - 1);
                    self.replace_sel(Some(Selection::rect(
                        self.doc.width,
                        self.doc.height,
                        x,
                        0,
                        1,
                        self.doc.height,
                    )));
                    return;
                }
                let (a, b) = (self.drag_start_doc, self.drag_cur_doc);
                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                if x1 - x0 < 2.0 && y1 - y0 < 2.0 {
                    self.replace_sel(None);
                } else if self.tool == Tool::MarqueeEllipse {
                    self.replace_sel(Some(Selection::ellipse(
                        self.doc.width,
                        self.doc.height,
                        x0 as i32,
                        y0 as i32,
                        (x1 - x0) as u32,
                        (y1 - y0) as u32,
                    )));
                } else {
                    self.replace_sel(Some(Selection::rect(
                        self.doc.width,
                        self.doc.height,
                        x0 as i32,
                        y0 as i32,
                        (x1 - x0) as u32,
                        (y1 - y0) as u32,
                    )));
                }
            }
            DragMode::Lasso => {
                if self.lasso_points.len() >= 3 {
                    let p = Selection::polygon(self.doc.width, self.doc.height, &self.lasso_points);
                    self.replace_sel(Some(p));
                } else {
                    self.replace_sel(None);
                }
                self.lasso_points.clear();
            }
            DragMode::MagLasso => {
                // Close on the start anchor when the drag ends
                // near it; otherwise close straight across.
                self.close_poly_selection();
            }
            DragMode::MagPen => {
                let pts = std::mem::take(&mut self.poly_points);
                if pts.len() >= 2 {
                    if let Some(p) = self.target_path_mut() {
                        p.subpaths.push(Subpath {
                            anchors: pts.iter().map(|q| Anchor::corner(q.0, q.1)).collect(),
                            closed: false,
                        });
                    }
                    self.commit_path_edit("Magnetic Pen");
                    self.status = "Magnetic Pen path".into();
                    self.touch();
                } else {
                    self.path_before = None;
                }
            }
            DragMode::FreePen => {
                let pts = std::mem::take(&mut self.pen_free);
                if pts.len() >= 2 {
                    let anchors = pictodesk_core::path::fit_open_contour(&pts);
                    if anchors.len() >= 2 {
                        if let Some(p) = self.target_path_mut() {
                            p.subpaths.push(Subpath {
                                anchors,
                                closed: false,
                            });
                        }
                        self.commit_path_edit("Freeform Pen");
                        self.status = "Freeform Pen path".into();
                        self.touch();
                    } else {
                        self.path_before = None;
                    }
                } else {
                    self.path_before = None;
                }
            }
            DragMode::GradientLine => {
                let layer = self.doc.active_layer;
                let tiles = self.sel_tiles();
                let sel = self.sel.clone();
                let (from, to, fg, bg, op, shape) = (
                    self.drag_start_doc,
                    self.drag_cur_doc,
                    self.fg,
                    self.bg,
                    self.opacity,
                    self.gradient_shape,
                );
                run_tile_edit(
                    &mut self.doc,
                    &mut self.hist,
                    "Gradient",
                    layer,
                    &tiles,
                    |d| {
                        brush::gradient_shaped(d, layer, sel.as_ref(), from, to, fg, bg, op, shape);
                    },
                );
                self.last_label = Some("Gradient".into());
                self.status = "Gradient".into();
                self.touch();
            }
            DragMode::ShapeDrag => {
                self.shape_drag_layer = None;
                self.commit_path_edit("Shape");
                self.status = "Shape layer".into();
                self.touch();
            }
            DragMode::SliceDrag => {
                self.commit_path_edit("Slice");
                self.status = "Slice".into();
                self.touch();
            }
            DragMode::MoveLayer => {
                let layer = self.doc.active_layer;
                let now = self.doc.layers[layer].offset;
                let mut moves = vec![(layer, self.move_orig_offset, now)];
                for (li, orig) in &self.move_linked {
                    moves.push((*li, *orig, self.doc.layers[*li].offset));
                }
                if moves.iter().any(|(_, b, a)| b != a) {
                    self.hist
                        .push(Box::new(MoveLayerCommand::new_multi("Move", moves)));
                    self.last_label = Some("Move".into());
                    self.status = "Move".into();
                    self.touch();
                }
                self.move_linked.clear();
            }
            _ => {}
        }
        self.drag = DragMode::None;
    }

    pub(crate) fn scroll(&mut self, dx: f32, dy: f32) {
        // Preferences Shortcuts list scrolls when the wheel is
        // over the open Preferences dialog.
        let prefs_shortcuts = self
            .dialog
            .as_ref()
            .map(|d| (d.kind == DialogKind::Preferences, d.fx_page));
        if let Some((true, 1)) = prefs_shortcuts {
            if let Some(layout) = self.dialog_layout() {
                let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
                if layout.rect.contains(mx, my) {
                    let max_off = self.shortcuts.len().saturating_sub(12);
                    let d = self.dialog.as_mut().unwrap();
                    if dy > 0.0 {
                        d.list_offset = d.list_offset.saturating_sub(1);
                    } else {
                        d.list_offset = (d.list_offset + 1).min(max_off);
                    }
                    return;
                }
            }
        }
        if self.cp_tab == 0 {
            if let Some(r) = self.group_rect(PanelKind::Cp) {
                let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
                if r.contains(mx, my) {
                    let n = self.panel_rows().len();
                    if dy > 0.0 {
                        self.layers_scroll = self.layers_scroll.saturating_sub(1);
                    } else if self.layers_scroll + 8 < n {
                        self.layers_scroll += 1;
                    }
                    return;
                }
            }
        }
        let canvas = self.canvas_rect(self.win_size.0, self.win_size.1);
        let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
        if canvas.contains(mx, my) {
            if self.zoom_with_scroll || self.primary_down() {
                // Primary (Cmd/Ctrl) + wheel zooms, and so does a
                // plain wheel when the preference says so.
                let local = (
                    self.cursor.0 - canvas.x as f32,
                    self.cursor.1 - canvas.y as f32,
                );
                self.viewport
                    .zoom_at(local, if dy > 0.0 { 1.1 } else { 0.9 });
            } else {
                // Plain wheel pans the canvas (the classic
                // default): vertical, Shift for horizontal.
                let step = 24.0;
                if self.shift_down {
                    self.viewport.offset.0 += dy * step;
                } else {
                    self.viewport.offset.0 += dx * step;
                    self.viewport.offset.1 += dy * step;
                }
            }
        }
    }

    /// The command modifier for shortcuts: Cmd on macOS, Ctrl on
    /// Windows and Linux — each platform's own convention.
    pub(crate) fn primary_down(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.super_down
        } else {
            self.ctrl_down
        }
    }

    pub(crate) fn key_char(&mut self, c: char) {
        let primary = self.primary_down();
        // Custom Filter kernel entry: digits build the selected
        // cell's value, '-' flips its sign.
        if !primary
            && self
                .dialog
                .as_ref()
                .map(|d| d.kind == DialogKind::CustomFilter)
                .unwrap_or(false)
        {
            let mut changed = false;
            {
                let d = self.dialog.as_mut().unwrap();
                let cell = &mut d.kernel[d.kernel_sel];
                if c.is_ascii_digit() {
                    let sign = if *cell < 0 { -1 } else { 1 };
                    *cell = sign * ((*cell).abs() * 10 + c.to_digit(10).unwrap() as i32);
                    *cell = (*cell).clamp(-999, 999);
                    changed = true;
                } else if c == '-' {
                    *cell = -*cell;
                    changed = true;
                }
            }
            if changed {
                self.dialog_refresh_preview();
            }
            return;
        }
        // Shortcut capture: the Preferences dialog is waiting
        // for a key — it gets the key, not the text field.
        if self.prefs_capture.is_some() {
            let combo = combo_for_key(c, self.primary_down(), self.shift_down, self.alt_down);
            self.dispatch_combo(&combo);
            return;
        }
        // dialog text entry
        if let Some(d) = &mut self.dialog {
            if d.text_focus && !primary {
                if d.kind == DialogKind::SliceProps {
                    match d.text_focus_n {
                        1 => d.text2.push(c),
                        2 => d.text3.push(c),
                        _ => d.text.push(c),
                    }
                } else {
                    d.text.push(c);
                }
            }
            return;
        }
        // Shortcuts: build the canonical combo from the
        // modifiers and dispatch through the (reassignable)
        // shortcut map (Preferences > Shortcuts).
        let combo = combo_for_key(c, self.primary_down(), self.shift_down, self.alt_down);
        self.dispatch_combo(&combo);
    }

    pub(crate) fn key_special(&mut self, key: SpecialKey) {
        // Shortcut capture (Preferences): Esc cancels the capture,
        // Backspace clears the row's shortcut; the dialog stays open.
        if self.prefs_capture.is_some() {
            match key {
                SpecialKey::Escape => {
                    self.dispatch_combo("escape");
                    return;
                }
                SpecialKey::Backspace => {
                    self.dispatch_combo("backspace");
                    return;
                }
                _ => {}
            }
        }
        match key {
            SpecialKey::Escape => {
                if self.persp_crop.take().is_some() {
                    self.persp_drag = None;
                    self.status = "Perspective Crop cancelled".into();
                }
                if self.ft.is_some() {
                    self.ft_cancel();
                } else if !self.poly_points.is_empty() {
                    self.poly_points.clear();
                    self.status = "Selection cancelled".into();
                } else if self.dialog.is_some() {
                    self.dialog_cancel();
                } else if self.file_panel.is_some() {
                    self.file_panel = None;
                } else if self.open_menu.is_some() {
                    self.open_menu = None;
                    self.open_submenu = None;
                } else if self.display_mode > 0 {
                    self.do_action(Action::ScreenMode(0));
                } else if self.show_palette {
                    self.show_palette = false;
                    self.fit_view(self.win_size.0, self.win_size.1);
                } else {
                    self.replace_sel(None);
                }
            }
            SpecialKey::Enter => {
                if self.persp_crop.is_some() {
                    let quad = self.persp_crop.take().unwrap();
                    self.structural("Perspective Crop", |doc| {
                        pictodesk_core::transform::perspective_crop(doc, quad);
                    });
                    self.sel = None;
                    self.fit_view(self.win_size.0, self.win_size.1);
                    self.status = "Perspective Crop applied".into();
                } else if self.ft.is_some() {
                    self.ft_commit();
                } else if self.shift_down
                    && matches!(
                        self.dialog.as_ref().map(|d| &d.kind),
                        Some(DialogKind::Type)
                    )
                {
                    // Shift+Enter inside the Type dialog = newline.
                    if let Some(d) = &mut self.dialog {
                        d.text.push('\n');
                    }
                } else if self.dialog.is_some() {
                    self.dialog_ok();
                } else if self.tool == Tool::PolyLasso && !self.poly_points.is_empty() {
                    self.close_poly_selection();
                } else if self.tool == Tool::Pen {
                    self.status = "Path finished".into();
                }
            }
            SpecialKey::Backspace => {
                if self
                    .dialog
                    .as_ref()
                    .map(|d| d.kind == DialogKind::CustomFilter)
                    .unwrap_or(false)
                {
                    {
                        let d = self.dialog.as_mut().unwrap();
                        let sel = d.kernel_sel;
                        d.kernel[sel] /= 10;
                    }
                    self.dialog_refresh_preview();
                    return;
                }
                let mut popped = false;
                if let Some(d) = &mut self.dialog {
                    if d.text_focus {
                        if d.kind == DialogKind::SliceProps {
                            match d.text_focus_n {
                                1 => {
                                    d.text2.pop();
                                }
                                2 => {
                                    d.text3.pop();
                                }
                                _ => {
                                    d.text.pop();
                                }
                            }
                        } else {
                            d.text.pop();
                        }
                        popped = true;
                    }
                }
                if !popped && self.dialog.is_none() {
                    if self.tool == Tool::Pen {
                        // delete the last anchor of the current subpath
                        self.begin_path_edit();
                        let mut removed = false;
                        if let Some(pth) = self.target_path_mut() {
                            if let Some(sub) = pth
                                .subpaths
                                .iter_mut()
                                .rev()
                                .find(|sub| !sub.anchors.is_empty())
                            {
                                sub.anchors.pop();
                                removed = true;
                            }
                        }
                        if removed {
                            self.commit_path_edit("Delete Anchor");
                            self.pen_anchor = None;
                            self.status = "Anchor deleted".into();
                            self.touch();
                        }
                    } else if self.sel.is_some() {
                        self.clear_selection();
                    }
                }
            }
        }
    }
}
