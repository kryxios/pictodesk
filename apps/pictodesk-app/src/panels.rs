//! Docked and floating panels: slots, strips, grouping, layout, and every panel's draw/click code.
use super::*;
use crate::actions::{save_tool_presets, Action, RecordedStep, ToolPreset, Workspace};
use crate::dialogs::{Dialog, DialogKind};
use crate::tools::{ChannelView, DragMode, PaintTarget, Tool};

/// One display row of the Layers panel (v0.6.1).
#[derive(Clone, Copy)]
pub(crate) enum PanelRow {
    /// Folder header for the set at this index in doc.sets.
    Set(usize),
    /// A layer row for this layer index.
    Layer(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PanelKind {
    Cp,
    Brushes,
    ToolPresets,
    Histogram,
    Color,
    Navigator,
    Actions,
    History,
    Info,
}

impl PanelKind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            PanelKind::Cp => "Layers",
            PanelKind::Brushes => "Brushes",
            PanelKind::ToolPresets => "Tool Presets",
            PanelKind::Histogram => "Histogram",
            PanelKind::Color => "Color",
            PanelKind::Navigator => "Navigator",
            PanelKind::Actions => "Actions",
            PanelKind::History => "History",
            PanelKind::Info => "Info",
        }
    }

    pub(crate) fn from_title(t: &str) -> Option<PanelKind> {
        Some(match t {
            "Layers" => PanelKind::Cp,
            "Brushes" => PanelKind::Brushes,
            "Tool Presets" => PanelKind::ToolPresets,
            "Histogram" => PanelKind::Histogram,
            "Color" => PanelKind::Color,
            "Navigator" => PanelKind::Navigator,
            "Actions" => PanelKind::Actions,
            "History" => PanelKind::History,
            "Info" => PanelKind::Info,
            _ => return None,
        })
    }

    pub(crate) fn all() -> [PanelKind; 9] {
        [
            PanelKind::Cp,
            PanelKind::Brushes,
            PanelKind::ToolPresets,
            PanelKind::Histogram,
            PanelKind::Color,
            PanelKind::Navigator,
            PanelKind::Actions,
            PanelKind::History,
            PanelKind::Info,
        ]
    }
}

/// A tab group of panels: one or more kinds sharing one rect, with
/// the active kind visible. Docked groups stack in the dock column;
/// floating groups have their own rect.
#[derive(Clone, Debug)]
pub(crate) struct PanelSlot {
    pub(crate) kinds: Vec<PanelKind>,
    pub(crate) active: usize,
}

/// Where a panel group lives.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SlotLoc {
    Dock(usize),
    Float(usize),
}

/// Drop target highlighted while a panel is dragged.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PanelDrop {
    Merge(SlotLoc),
    DockAt(i32),
}

/// Which frozen moment the History Brush paints from: a history
/// state (by index) or a snapshot (by index).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum HistSource {
    State(usize),
    Snapshot(usize),
}

/// A row in the History panel: a snapshot or a history state
/// (state 0 = as opened).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum HistRow {
    Snapshot(usize),
    State(usize),
}

/// A row in the Actions panel: a set header or an action in it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ActRow {
    Set(usize),
    Action(usize),
}

impl App {
    /// A different document arrived (new/open): snapshots and the
    /// brush source reset, and with Auto Snapshot on, the opened
    /// state is frozen as Snapshot 1 and becomes the brush source.
    pub(crate) fn reset_hist_for_new_doc(&mut self) {
        self.snapshots.clear();
        self.hist_scroll = 0;
        self.hist_selected = None;
        self.hist_src_cache = None;
        self.measure = None;
        if self.hist_auto_snapshot {
            self.snapshots.push(Snapshot {
                name: "Snapshot 1".into(),
                doc: self.doc.clone(),
                kind: SnapKind::Full,
            });
            self.hist_source = HistSource::Snapshot(0);
        } else {
            self.hist_source = HistSource::State(0);
        }
    }

    /// Freeze the current document as a new snapshot.
    /// Replace the current selection, remembering the outgoing
    /// one for Select > Reselect.
    pub(crate) fn replace_sel(&mut self, s: Option<Selection>) {
        if let Some(old) = self.sel.take() {
            if old.bounds().is_some() {
                self.doc.last_selection = Some(old);
            }
        }
        self.sel = s;
    }

    pub(crate) fn take_snapshot(&mut self) {
        self.take_snapshot_kind(SnapKind::Full);
    }

    /// Take a snapshot of the chosen kind: the whole document,
    /// the document merged flat, or just the active layer.
    pub(crate) fn take_snapshot_kind(&mut self, kind: SnapKind) {
        let n = self.snapshots.len() + 1;
        let (doc, tag) = match kind {
            SnapKind::Full => (self.doc.clone(), ""),
            SnapKind::Merged => {
                let mut d = self.doc.clone();
                d.flatten();
                (d, " (merged)")
            }
            SnapKind::Layer(_) => (self.doc.clone(), " (layer)"),
        };
        self.snapshots.push(Snapshot {
            name: format!("Snapshot {n}{tag}"),
            doc,
            kind,
        });
        self.status = format!("Snapshot {n}{tag} taken");
    }

    /// The document the History Brush and Fill with History paint
    /// from, rebuilt when the source or history position changes.
    pub(crate) fn hist_source_doc(&mut self) -> Option<Document> {
        // Cache key: the source plus the history shape it was
        // reconstructed from (position + state count).
        let shape = (self.hist.position(), self.hist.state_count());
        if let Some((src, pos, count, doc)) = &self.hist_src_cache {
            if *src == self.hist_source && (*pos, *count) == shape {
                return Some(doc.clone());
            }
        }
        let doc = match self.hist_source {
            HistSource::State(s) => self.hist.doc_at_state(&self.doc, s)?,
            HistSource::Snapshot(i) => {
                let snap = self.snapshots.get(i)?;
                match snap.kind {
                    SnapKind::Layer(li) => {
                        // A layer snapshot paints from the current
                        // document with that one layer rolled back.
                        let mut d = self.doc.clone();
                        if li < d.layers.len() && li < snap.doc.layers.len() {
                            d.layers[li] = snap.doc.layers[li].clone();
                        }
                        d
                    }
                    _ => snap.doc.clone(),
                }
            }
        };
        self.hist_src_cache = Some((self.hist_source, shape.0, shape.1, doc.clone()));
        Some(doc)
    }

    pub(crate) fn channel_row_count(&self) -> usize {
        let base = match self.doc.mode {
            ColorMode::Rgb => 3,
            ColorMode::Cmyk => 4,
            ColorMode::Grayscale => 1,
            ColorMode::Lab => 3,
            ColorMode::Indexed => 3,
            ColorMode::Bitmap | ColorMode::Duotone => 1,
            ColorMode::Multichannel => self.doc.multi_channels.len(),
        };
        1 + base + self.doc.alphas.len()
    }

    pub(crate) fn path_row_count(&self) -> usize {
        (if self.doc.work_path.is_some() { 1 } else { 0 }) + self.doc.paths.len()
    }

    /// Whether a panel kind is currently shown (Window menu flags).
    pub(crate) fn panel_visible(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Cp => self.show_cp,
            PanelKind::Brushes => self.show_brushes,
            PanelKind::ToolPresets => self.show_tool_presets,
            PanelKind::Histogram => self.show_histogram,
            PanelKind::Color => self.show_palette,
            PanelKind::Navigator => self.show_navigator,
            PanelKind::Actions => self.show_actions,
            PanelKind::History => self.show_history,
            PanelKind::Info => self.show_info,
        }
    }

    pub(crate) fn set_panel_visible(&mut self, kind: PanelKind, v: bool) {
        match kind {
            PanelKind::Cp => self.show_cp = v,
            PanelKind::Brushes => self.show_brushes = v,
            PanelKind::ToolPresets => self.show_tool_presets = v,
            PanelKind::Histogram => self.show_histogram = v,
            PanelKind::Color => self.show_palette = v,
            PanelKind::Navigator => self.show_navigator = v,
            PanelKind::Actions => self.show_actions = v,
            PanelKind::History => self.show_history = v,
            PanelKind::Info => self.show_info = v,
        }
    }

    /// Height of one panel's content (without its tab strip).
    pub(crate) fn panel_height(&self, kind: PanelKind) -> i32 {
        match kind {
            PanelKind::Cp => match self.cp_tab {
                0 => 70 + self.doc.layers.len().min(8) as i32 * self.layer_row_h(),
                1 => 20 + self.channel_row_count() as i32 * 15 + 22,
                _ => 20 + self.path_row_count().max(1) as i32 * 15 + 42,
            },
            PanelKind::Brushes => self.brushes_panel_h(),
            PanelKind::ToolPresets => 24 + self.tool_presets.len().min(10) as i32 * 15 + 20,
            PanelKind::Histogram => 132,
            PanelKind::Color => 186,
            PanelKind::Navigator => 128,
            PanelKind::Actions => {
                if self.actions_button_mode {
                    20 + self.actions.len().min(8) as i32 * 19 + 24
                } else {
                    let rows = self.action_rows().len().max(1);
                    20 + rows.min(6) as i32 * 15 + 58 + 20
                }
            }
            PanelKind::History => {
                let rows = self.snapshots.len() + self.hist.state_count() + 1;
                20 + rows.min(9) as i32 * 15 + 38
            }
            PanelKind::Info => 118,
        }
    }

    /// The dock's panel groups, stacked top-down on the right edge,
    /// plus floating groups. Each entry is the ACTIVE kind of its
    /// slot with its content rect (below the tab strip, if any).
    /// Single-kind docked slots have no strip, so their rect is
    /// exactly the classic one.
    pub(crate) fn panel_groups(&self) -> Vec<(PanelKind, Rect)> {
        let mut out = Vec::new();
        if self.display_mode > 0 {
            return out;
        }
        if self.dock_open() {
            let x = self.win_size.0 as i32 - DOCK_W;
            let bottom = self.win_size.1 as i32 - ui::STATUS_H;
            let mut y = self.top_y();
            for slot in &self.dock_slots {
                let kind = slot.kinds[slot.active];
                let strip = if slot.kinds.len() > 1 { PANEL_TAB_H } else { 0 };
                let h = self.panel_height(kind);
                if y + strip + h <= bottom {
                    out.push((kind, Rect::new(x, y + strip, DOCK_W, h)));
                    y += strip + h + 2;
                }
            }
        }
        for (slot, fx, fy, fw) in &self.float_slots {
            let kind = slot.kinds[slot.active];
            out.push((
                kind,
                Rect::new(*fx, *fy + PANEL_TAB_H, *fw, self.panel_height(kind)),
            ));
        }
        out
    }

    /// Which slot holds a kind, if any.
    pub(crate) fn slot_of(&self, kind: PanelKind) -> Option<SlotLoc> {
        for (i, slot) in self.dock_slots.iter().enumerate() {
            if slot.kinds.contains(&kind) {
                return Some(SlotLoc::Dock(i));
            }
        }
        for (i, (slot, _, _, _)) in self.float_slots.iter().enumerate() {
            if slot.kinds.contains(&kind) {
                return Some(SlotLoc::Float(i));
            }
        }
        None
    }

    pub(crate) fn slot_active_kind(&self, loc: SlotLoc) -> PanelKind {
        match loc {
            SlotLoc::Dock(i) => self.dock_slots[i].kinds[self.dock_slots[i].active],
            SlotLoc::Float(i) => self.float_slots[i].0.kinds[self.float_slots[i].0.active],
        }
    }

    pub(crate) fn set_slot_active(&mut self, loc: SlotLoc, kind: PanelKind) {
        match loc {
            SlotLoc::Dock(i) => {
                if let Some(slot) = self.dock_slots.get_mut(i) {
                    if let Some(pos) = slot.kinds.iter().position(|k| *k == kind) {
                        slot.active = pos;
                    }
                }
            }
            SlotLoc::Float(i) => {
                if let Some((slot, _, _, _)) = self.float_slots.get_mut(i) {
                    if let Some(pos) = slot.kinds.iter().position(|k| *k == kind) {
                        slot.active = pos;
                    }
                }
            }
        }
        self.save_panel_layout();
    }

    /// Reconcile slots with the Window-menu visibility flags: drop
    /// hidden kinds (and empty slots), append newly shown kinds as
    /// their own docked slots. Called every frame from draw(), so
    /// every toggle path is covered.
    pub(crate) fn sync_panel_slots(&mut self) {
        // Snapshot visibility first: the retain closures below
        // cannot borrow self while the slots are mutably borrowed.
        let visible: [bool; 9] = [
            self.show_cp,
            self.show_brushes,
            self.show_tool_presets,
            self.show_histogram,
            self.show_palette,
            self.show_navigator,
            self.show_actions,
            self.show_history,
            self.show_info,
        ];
        for slot in self
            .dock_slots
            .iter_mut()
            .chain(self.float_slots.iter_mut().map(|(s, _, _, _)| s))
        {
            slot.kinds.retain(|k| visible[*k as usize]);
            if slot.active >= slot.kinds.len() {
                slot.active = slot.kinds.len().saturating_sub(1);
            }
        }
        self.dock_slots.retain(|s| !s.kinds.is_empty());
        self.float_slots.retain(|(s, _, _, _)| !s.kinds.is_empty());
        for kind in PanelKind::all() {
            if self.panel_visible(kind) && self.slot_of(kind).is_none() {
                self.dock_slots.push(PanelSlot {
                    kinds: vec![kind],
                    active: 0,
                });
            }
        }
    }

    /// Rects of tab strips (multi-kind dock slots) and float headers.
    pub(crate) fn panel_strip_rects(&self) -> Vec<(SlotLoc, Rect)> {
        let mut out = Vec::new();
        if self.display_mode > 0 {
            return out;
        }
        if self.dock_open() {
            let x = self.win_size.0 as i32 - DOCK_W;
            let bottom = self.win_size.1 as i32 - ui::STATUS_H;
            let mut y = self.top_y();
            for (i, slot) in self.dock_slots.iter().enumerate() {
                let kind = slot.kinds[slot.active];
                let strip = if slot.kinds.len() > 1 { PANEL_TAB_H } else { 0 };
                let h = self.panel_height(kind);
                if strip > 0 && y + strip + h <= bottom {
                    out.push((SlotLoc::Dock(i), Rect::new(x, y, DOCK_W, PANEL_TAB_H)));
                }
                y += strip + h + 2;
            }
        }
        for (i, (_, fx, fy, fw)) in self.float_slots.iter().enumerate() {
            out.push((SlotLoc::Float(i), Rect::new(*fx, *fy, *fw, PANEL_TAB_H)));
        }
        out
    }

    /// Which tab of a strip sits under x (excluding a float
    /// header's dock/close buttons).
    pub(crate) fn strip_tab_at(&self, loc: SlotLoc, mx: i32, my: i32) -> Option<PanelKind> {
        let slot = match loc {
            SlotLoc::Dock(i) => &self.dock_slots[i],
            SlotLoc::Float(i) => &self.float_slots[i].0,
        };
        let (_, strip) = self
            .panel_strip_rects()
            .into_iter()
            .find(|(l, _)| *l == loc)?;
        let mut x = strip.x + 2;
        for kind in &slot.kinds {
            let tw = Frame::text_width(kind.title(), 1);
            let tr = Rect::new(x, strip.y + 2, tw + 14, PANEL_TAB_H - 4);
            if tr.contains(mx, my) {
                return Some(*kind);
            }
            x += tr.w + 2;
        }
        None
    }

    /// Drag handle of a single docked panel: its title-label zone
    /// (for Cp, the active Layers/Channels/Paths tab). Multi-kind
    /// slots and floats are handled by their strips instead.
    pub(crate) fn title_handle_at(&self, mx: i32, my: i32) -> Option<(PanelKind, SlotLoc)> {
        for (kind, r) in self.panel_groups() {
            let Some(loc) = self.slot_of(kind) else {
                continue;
            };
            let multi = match loc {
                SlotLoc::Dock(i) => self.dock_slots[i].kinds.len() > 1,
                SlotLoc::Float(_) => true,
            };
            if multi {
                continue;
            }
            let handle = if kind == PanelKind::Cp {
                Self::cp_tab_rect(r, self.cp_tab)
            } else {
                // The whole title row is a handle (the inactive
                // Cp tabs keep their own narrow handles above).
                Rect::new(r.x, r.y, r.w, 16)
            };
            if handle.contains(mx, my) {
                return Some((kind, loc));
            }
        }
        None
    }

    /// Draw tab strips and float headers (after the panels).
    pub(crate) fn draw_panel_strips(&mut self, f: &mut Frame) {
        for (loc, r) in self.panel_strip_rects() {
            let (slot, is_float) = match loc {
                SlotLoc::Dock(i) => (&self.dock_slots[i], false),
                SlotLoc::Float(i) => (&self.float_slots[i].0, true),
            };
            f.fill_rect(r, ui::DARK);
            let mut x = r.x + 2;
            for (i, kind) in slot.kinds.iter().enumerate() {
                let tw = Frame::text_width(kind.title(), 1);
                let tr = Rect::new(x, r.y + 2, tw + 14, PANEL_TAB_H - 4);
                if i == slot.active {
                    f.fill_rect(tr, ui::PLATINUM);
                    f.text(tr.x + 7, tr.y + 2, kind.title(), 1, ui::TEXT);
                } else {
                    f.text(tr.x + 7, tr.y + 2, kind.title(), 1, ui::TEXT_DIM);
                }
                x += tr.w + 2;
            }
            if is_float {
                // Dock button (square outline) and close button (x).
                let dock = Rect::new(r.right() - 32, r.y + 4, 10, 10);
                let close = Rect::new(r.right() - 16, r.y + 2, 14, PANEL_TAB_H - 4);
                f.outline_rect(dock, ui::TEXT_DIM);
                f.text(close.x + 4, close.y + 2, "x", 1, ui::TEXT_DIM);
            }
            f.fill_rect(Rect::new(r.x, r.bottom() - 1, r.w, 1), ui::EDGE);
        }
        // Drop highlight while dragging a panel.
        if let Some(drop) = self.panel_drop {
            match drop {
                PanelDrop::Merge(loc) => {
                    if let Some((_, r)) = self
                        .panel_strip_rects()
                        .into_iter()
                        .find(|(l, _)| *l == loc)
                    {
                        f.outline_rect(r, ui::ACCENT);
                    }
                }
                PanelDrop::DockAt(y) => {
                    f.fill_rect(
                        Rect::new(self.win_size.0 as i32 - DOCK_W, y - 1, DOCK_W, 2),
                        ui::ACCENT,
                    );
                }
            }
        }
    }

    /// Press on a strip/header: tab switch, float dock/close, or
    /// start a pending drag.
    pub(crate) fn panel_strip_mouse(&mut self, mx: i32, my: i32) -> bool {
        if self.display_mode > 0 {
            return false;
        }
        let Some((loc, strip)) = self
            .panel_strip_rects()
            .into_iter()
            .find(|(_, r)| r.contains(mx, my))
        else {
            return false;
        };
        if let SlotLoc::Float(_) = loc {
            let close = Rect::new(strip.right() - 16, strip.y + 2, 14, PANEL_TAB_H - 4);
            let dock = Rect::new(strip.right() - 32, strip.y + 2, 14, PANEL_TAB_H - 4);
            if close.contains(mx, my) {
                self.hide_panel_slot(loc);
                return true;
            }
            if dock.contains(mx, my) {
                self.redock_slot(loc);
                return true;
            }
        }
        if let Some(kind) = self.strip_tab_at(loc, mx, my) {
            if kind != self.slot_active_kind(loc) {
                self.set_slot_active(loc, kind);
                return true;
            }
            self.panel_pending = Some((kind, loc, mx, my));
            return true;
        }
        // Anywhere else on the strip drags the active tab: the
        // whole header is a handle, not just the tab's text.
        let kind = self.slot_active_kind(loc);
        self.panel_pending = Some((kind, loc, mx, my));
        true
    }

    /// Press on a single docked panel's title label: pending drag.
    pub(crate) fn panel_title_mouse(&mut self, mx: i32, my: i32) -> bool {
        if self.display_mode > 0 {
            return false;
        }
        let Some((kind, loc)) = self.title_handle_at(mx, my) else {
            return false;
        };
        self.panel_pending = Some((kind, loc, mx, my));
        true
    }

    /// Move a kind into its own float at (x, y).
    pub(crate) fn float_panel(&mut self, kind: PanelKind, x: i32, y: i32) {
        let Some(loc) = self.slot_of(kind) else {
            return;
        };
        self.remove_from_slot(kind, loc);
        self.float_slots.push((
            PanelSlot {
                kinds: vec![kind],
                active: 0,
            },
            x,
            y,
            DOCK_W,
        ));
    }

    pub(crate) fn remove_from_slot(&mut self, kind: PanelKind, loc: SlotLoc) {
        match loc {
            SlotLoc::Dock(i) => {
                if let Some(slot) = self.dock_slots.get_mut(i) {
                    slot.kinds.retain(|k| *k != kind);
                    if slot.active >= slot.kinds.len() {
                        slot.active = slot.kinds.len().saturating_sub(1);
                    }
                }
            }
            SlotLoc::Float(i) => {
                if let Some((slot, _, _, _)) = self.float_slots.get_mut(i) {
                    slot.kinds.retain(|k| *k != kind);
                    if slot.active >= slot.kinds.len() {
                        slot.active = slot.kinds.len().saturating_sub(1);
                    }
                }
            }
        }
        self.dock_slots.retain(|s| !s.kinds.is_empty());
        self.float_slots.retain(|(s, _, _, _)| !s.kinds.is_empty());
    }

    /// Group kind b into kind a's slot (b becomes active).
    pub(crate) fn group_panels(&mut self, a: PanelKind, b: PanelKind) {
        let (Some(lb), Some(la)) = (self.slot_of(b), self.slot_of(a)) else {
            return;
        };
        if la == lb {
            return;
        }
        self.remove_from_slot(b, lb);
        let Some(la2) = self.slot_of(a) else {
            return;
        };
        match la2 {
            SlotLoc::Dock(i) => {
                let slot = &mut self.dock_slots[i];
                slot.kinds.push(b);
                slot.active = slot.kinds.len() - 1;
            }
            SlotLoc::Float(i) => {
                let (slot, _, _, _) = &mut self.float_slots[i];
                slot.kinds.push(b);
                slot.active = slot.kinds.len() - 1;
            }
        }
        self.save_panel_layout();
    }

    /// Detach a kind into a float under the cursor; returns the
    /// float index and the grab offset within its header.
    pub(crate) fn detach_panel(
        &mut self,
        kind: PanelKind,
        loc: SlotLoc,
        press_x: i32,
        press_y: i32,
        cur_x: i32,
        cur_y: i32,
    ) -> (usize, i32, i32) {
        // Grab offset from the strip/title the press landed on.
        let (gx, gy) = match loc {
            SlotLoc::Dock(i) if self.dock_slots[i].kinds.len() > 1 => {
                let (_, strip) = self
                    .panel_strip_rects()
                    .into_iter()
                    .find(|(l, _)| *l == loc)
                    .unwrap();
                (press_x - strip.x, press_y - strip.y)
            }
            SlotLoc::Float(_) => {
                let (_, strip) = self
                    .panel_strip_rects()
                    .into_iter()
                    .find(|(l, _)| *l == loc)
                    .unwrap();
                (press_x - strip.x, press_y - strip.y)
            }
            _ => {
                // Single docked panel: grab offset from its title handle.
                let (_, r) = self
                    .panel_groups()
                    .into_iter()
                    .find(|(k, _)| *k == kind)
                    .unwrap();
                let handle = if kind == PanelKind::Cp {
                    Self::cp_tab_rect(r, self.cp_tab)
                } else {
                    let tw = Frame::text_width(kind.title(), 1);
                    Rect::new(r.x + 2, r.y, tw + 14, 16)
                };
                (press_x - handle.x, press_y - handle.y)
            }
        };
        let fx = (cur_x - gx).clamp(0, self.win_size.0 as i32 - 60);
        let fy = (cur_y - gy).clamp(0, self.win_size.1 as i32 - 40);
        self.float_panel(kind, fx, fy);
        (self.float_slots.len() - 1, gx, gy)
    }

    /// Drop target for the dragged float (excluding its own strip).
    pub(crate) fn compute_drop(&self, mx: i32, my: i32, drag_idx: usize) -> Option<PanelDrop> {
        // Merge first: any strip (dock or float, anywhere on
        // screen) takes the dragged tab into its group.
        for (loc, r) in self.panel_strip_rects() {
            if loc == SlotLoc::Float(drag_idx) {
                continue;
            }
            if r.contains(mx, my) {
                return Some(PanelDrop::Merge(loc));
            }
        }
        // Snap back to the dock: the cursor is in the dock
        // column, within the dock's vertical span.
        let dock_x = self.win_size.0 as i32 - DOCK_W;
        if self.dock_open()
            && mx >= dock_x
            && my >= self.top_y()
            && my <= self.win_size.1 as i32 - ui::STATUS_H
        {
            return Some(PanelDrop::DockAt(my));
        }
        None
    }

    /// Drop the dragged float's kind into another slot.
    pub(crate) fn merge_float_into(&mut self, idx: usize, loc: SlotLoc) {
        let kinds = self
            .float_slots
            .get(idx)
            .map(|(s, _, _, _)| s.kinds.clone())
            .unwrap_or_default();
        if kinds.is_empty() {
            return;
        }
        match loc {
            SlotLoc::Dock(i) => {
                if let Some(slot) = self.dock_slots.get_mut(i) {
                    for k in kinds {
                        if !slot.kinds.contains(&k) {
                            slot.kinds.push(k);
                        }
                    }
                    slot.active = slot.kinds.len() - 1;
                }
            }
            SlotLoc::Float(j) => {
                if let Some((slot, _, _, _)) = self.float_slots.get_mut(j) {
                    for k in kinds {
                        if !slot.kinds.contains(&k) {
                            slot.kinds.push(k);
                        }
                    }
                    slot.active = slot.kinds.len() - 1;
                }
            }
        }
        self.float_slots.remove(idx);
    }

    /// Drop the dragged float back into the dock at height y.
    pub(crate) fn redock_float_at(&mut self, idx: usize, y: i32) {
        let Some((slot, _, _, _)) = self.float_slots.get(idx) else {
            return;
        };
        let kinds = slot.kinds.clone();
        self.float_slots.remove(idx);
        let mut pos = self.dock_slots.len();
        let mut cy = self.top_y();
        for (i, s) in self.dock_slots.iter().enumerate() {
            let kind = s.kinds[s.active];
            let strip = if s.kinds.len() > 1 { PANEL_TAB_H } else { 0 };
            let h = self.panel_height(kind) + strip;
            if y < cy + h / 2 {
                pos = i;
                break;
            }
            cy += h + 2;
        }
        self.dock_slots.insert(pos, PanelSlot { kinds, active: 0 });
    }

    /// Float header's dock button: whole group back to the dock.
    pub(crate) fn redock_slot(&mut self, loc: SlotLoc) {
        if let SlotLoc::Float(i) = loc {
            if let Some((slot, _, _, _)) = self.float_slots.get(i) {
                let kinds = slot.kinds.clone();
                self.float_slots.remove(i);
                self.dock_slots.push(PanelSlot { kinds, active: 0 });
                self.save_panel_layout();
            }
        }
    }

    /// Float header's close button: hide the whole group.
    pub(crate) fn hide_panel_slot(&mut self, loc: SlotLoc) {
        let kinds = match loc {
            SlotLoc::Dock(i) => self.dock_slots.get(i).map(|s| s.kinds.clone()),
            SlotLoc::Float(i) => self.float_slots.get(i).map(|(s, _, _, _)| s.kinds.clone()),
        };
        if let Some(kinds) = kinds {
            for k in kinds {
                self.set_panel_visible(k, false);
            }
            self.sync_panel_slots();
            self.save_panel_layout();
        }
    }

    /// Persist the panel layout (dock order + floats).
    pub(crate) fn save_panel_layout(&self) {
        let dir = pictodesk_dir();
        let mut text = String::new();
        for slot in &self.dock_slots {
            let names: Vec<&str> = slot.kinds.iter().map(|k| k.title()).collect();
            text.push_str(&format!("D\t{}\t{}\n", names.join(","), slot.active));
        }
        for (slot, x, y, w) in &self.float_slots {
            let names: Vec<&str> = slot.kinds.iter().map(|k| k.title()).collect();
            text.push_str(&format!(
                "F\t{x},{y},{w}\t{}\t{}\n",
                names.join(","),
                slot.active
            ));
        }
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("layout.txt"), text);
    }

    pub(crate) fn load_panel_layout(&mut self) {
        let Ok(text) = std::fs::read_to_string(pictodesk_dir().join("layout.txt")) else {
            return;
        };
        let mut dock = Vec::new();
        let mut floats = Vec::new();
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            match parts.as_slice() {
                ["D", names, active] => {
                    let kinds: Vec<PanelKind> =
                        names.split(',').filter_map(PanelKind::from_title).collect();
                    if !kinds.is_empty() {
                        dock.push(PanelSlot {
                            kinds,
                            active: active.parse().unwrap_or(0),
                        });
                    }
                }
                ["F", geo, names, active] => {
                    let g: Vec<i32> = geo.split(',').filter_map(|s| s.parse().ok()).collect();
                    let kinds: Vec<PanelKind> =
                        names.split(',').filter_map(PanelKind::from_title).collect();
                    if g.len() == 3 && !kinds.is_empty() {
                        floats.push((
                            PanelSlot {
                                kinds,
                                active: active.parse().unwrap_or(0),
                            },
                            g[0],
                            g[1],
                            g[2],
                        ));
                    }
                }
                _ => {}
            }
        }
        if !dock.is_empty() || !floats.is_empty() {
            self.dock_slots = dock;
            self.float_slots = floats;
        }
    }

    pub(crate) fn group_rect(&self, kind: PanelKind) -> Option<Rect> {
        self.panel_groups()
            .into_iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, r)| r)
    }

    /// Persist the action library (Phase 6.5 Batch 6d).
    pub(crate) fn save_workspaces_file(&self) {
        let dir = pictodesk_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mut text = String::new();
        for (name, w) in &self.workspaces {
            let mut flags = String::new();
            for p in w.panels {
                flags.push(if p { '1' } else { '0' });
            }
            flags.push(if w.options_bar { '1' } else { '0' });
            text += &format!("{}\t{flags}\t{}\n", name.replace('\t', " "), w.display_mode);
        }
        let _ = std::fs::write(dir.join("workspaces.txt"), text);
    }

    pub(crate) fn apply_workspace(&mut self, i: usize) {
        let Some((name, w)) = self.workspaces.get(i).cloned() else {
            return;
        };
        self.show_cp = w.panels[0];
        self.show_brushes = w.panels[1];
        self.show_tool_presets = w.panels[2];
        self.show_histogram = w.panels[3];
        self.show_palette = w.panels[4];
        self.show_navigator = w.panels[5];
        self.show_actions = w.panels[6];
        self.show_history = w.panels[7];
        self.show_info = w.panels[8];
        self.show_options_bar = w.options_bar;
        self.display_mode = w.display_mode;
        self.status = format!("Workspace '{name}' applied");
        self.fit_view(self.win_size.0, self.win_size.1);
    }

    pub(crate) fn capture_workspace(&self) -> Workspace {
        Workspace {
            panels: [
                self.show_cp,
                self.show_brushes,
                self.show_tool_presets,
                self.show_histogram,
                self.show_palette,
                self.show_navigator,
                self.show_actions,
                self.show_history,
                self.show_info,
            ],
            options_bar: self.show_options_bar,
            display_mode: self.display_mode,
        }
    }

    pub(crate) fn capture_tool_preset(&self, name: String) -> ToolPreset {
        let fg = self.fg.to_rgba8();
        let bg = self.bg.to_rgba8();
        ToolPreset {
            name,
            tool: self.tool,
            size: self.tip.size,
            hardness: self.tip.hardness,
            dyn_: self.brush_dyn.clone(),
            flow: self.flow,
            fg: (fg.0, fg.1, fg.2),
            bg: (bg.0, bg.1, bg.2),
        }
    }

    pub(crate) fn apply_tool_preset(&mut self, idx: usize) {
        let Some(p) = self.tool_presets.get(idx).cloned() else {
            return;
        };
        self.tool = p.tool;
        self.tip = BrushTip::new(p.size, p.hardness);
        self.brush_dyn = p.dyn_.clone();
        self.flow = p.flow;
        self.fg = Pixel16::from_rgba8(p.fg.0, p.fg.1, p.fg.2, 255);
        self.bg = Pixel16::from_rgba8(p.bg.0, p.bg.1, p.bg.2, 255);
        self.status = format!("Tool preset '{}'", p.name);
    }

    /// The tool preset whose state matches the current tool
    /// setup, if any (drives the options-bar picker label).
    pub(crate) fn current_tool_preset(&self) -> Option<usize> {
        self.tool_presets.iter().position(|p| {
            p.tool == self.tool
                && p.size == self.tip.size
                && p.hardness == self.tip.hardness
                && p.dyn_ == self.brush_dyn
        })
    }

    pub(crate) fn swatch_default_rect(&self) -> Rect {
        let area_bottom = self.win_size.1 as i32 - ui::STATUS_H;
        Rect::new(36, area_bottom - 52, 12, 12)
    }

    /// Web-safe = every channel a multiple of 51 (the 216-color
    /// web palette of the era).
    pub(crate) fn web_safe(rgb: (u8, u8, u8)) -> bool {
        rgb.0.is_multiple_of(51) && rgb.1.is_multiple_of(51) && rgb.2.is_multiple_of(51)
    }

    pub(crate) fn web_warn_rect(win_h: i32) -> Rect {
        let sy = win_h - ui::STATUS_H - 52;
        Rect::new(10, sy + 24, 11, 11)
    }

    pub(crate) fn draw_palette(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Color) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Color", 1, ui::TEXT);
        for (i, c) in self.palette.iter().enumerate() {
            let cell = Rect::new(
                r.x + 5 + (i % 8) as i32 * 17,
                r.y + 18 + (i / 8) as i32 * 17,
                16,
                16,
            );
            f.fill_rect(cell, *c);
            f.outline_rect(cell, ui::SHADOW);
        }
        Self::draw_small_button(
            f,
            Rect::new(r.x + 5, r.bottom() - 16, 100, 13),
            "Save .ACO",
            true,
        );
        Self::draw_small_button(
            f,
            Rect::new(r.x + 110, r.bottom() - 16, 100, 13),
            "Load .ACO",
            true,
        );
    }

    pub(crate) fn palette_cell_rect(&self, i: usize) -> Rect {
        let r = self
            .group_rect(PanelKind::Color)
            .unwrap_or(Rect::new(0, 0, 0, 0));
        Rect::new(
            r.x + 5 + (i % 8) as i32 * 17,
            r.y + 18 + (i / 8) as i32 * 17,
            16,
            16,
        )
    }

    pub(crate) fn brush_section_names() -> [&'static str; 8] {
        [
            "Tip Shape",
            "Shape Dynamics",
            "Scattering",
            "Texture",
            "Dual Brush",
            "Color Dynamics",
            "Other Dynamics",
            "Toggles",
        ]
    }

    pub(crate) fn brush_section_active(&self, sec: u8) -> bool {
        let d = &self.brush_dyn;
        match sec {
            0 => {
                d.angle_deg != 0.0
                    || d.roundness != 1.0
                    || d.spacing != 0.25
                    || d.flip_x
                    || d.flip_y
            }
            1 => {
                d.size_jitter > 0.0
                    || d.angle_jitter > 0.0
                    || d.round_jitter > 0.0
                    || d.size_control > 0
                    || d.angle_control > 0
                    || d.round_control > 0
            }
            2 => d.scatter > 0.0 || d.count > 1,
            3 => d.texture_on,
            4 => d.dual_on,
            5 => {
                d.fb_jitter > 0.0 || d.hue_jitter > 0.0 || d.sat_jitter > 0.0 || d.bri_jitter > 0.0
            }
            6 => {
                d.opacity_jitter > 0.0
                    || d.flow_jitter > 0.0
                    || d.opacity_control > 0
                    || d.flow_control > 0
            }
            _ => d.noise || d.wet_edges || d.airbrush || d.smoothing,
        }
    }

    pub(crate) fn brushes_panel_h(&self) -> i32 {
        let rows = self.dyn_row_defs().len() as i32;
        let tool_row = if self.has_tool_row() { 24 } else { 0 };
        86 + 8 * 13 + 6 + rows * 17 + 22 + tool_row
    }

    /// (id, label, value text, kind) for the current section.
    /// kind: 0 stepper, 1 toggle, 2 cycle.
    pub(crate) fn dyn_row_defs(&self) -> Vec<(u8, String, String, u8)> {
        let d = &self.brush_dyn;
        let pct = |v: f32| format!("{:.0}%", v * 100.0);
        let ctl = |c: u8| {
            ["Off", "Fade", "Pen Pressure", "Pen Tilt", "Stylus Wheel"][(c as usize).min(4)]
                .to_string()
        };
        let step = |id: u8, label: &str, v: String| (id, label.to_string(), v, 0u8);
        let tog = |id: u8, label: &str, on: bool| {
            (
                id,
                label.to_string(),
                if on { "On".into() } else { "Off".into() },
                1u8,
            )
        };
        let cyc = |id: u8, label: &str, v: String| (id, label.to_string(), v, 2u8);
        match self.brush_section {
            0 => vec![
                step(1, "Angle", format!("{:.0} deg", d.angle_deg)),
                step(2, "Roundness", pct(d.roundness)),
                step(3, "Spacing", pct(d.spacing)),
                tog(4, "Flip X", d.flip_x),
                tog(5, "Flip Y", d.flip_y),
            ],
            1 => vec![
                step(10, "Size Jitter", pct(d.size_jitter)),
                cyc(11, "Size Control", ctl(d.size_control)),
                step(12, "Min Diameter", pct(d.min_diameter)),
                step(13, "Angle Jitter", pct(d.angle_jitter)),
                cyc(14, "Angle Control", ctl(d.angle_control)),
                step(15, "Round Jitter", pct(d.round_jitter)),
                cyc(16, "Round Control", ctl(d.round_control)),
                step(17, "Min Roundness", pct(d.min_roundness)),
                step(18, "Fade Steps", format!("{}", d.fade_steps)),
            ],
            2 => vec![
                step(20, "Scatter", pct(d.scatter)),
                tog(21, "Both Axes", d.both_axes),
                step(22, "Count", format!("{}", d.count)),
                step(23, "Count Jitter", pct(d.count_jitter)),
            ],
            3 => vec![
                tog(30, "Texture", d.texture_on),
                step(31, "Scale", pct(d.tex_scale)),
                step(32, "Depth", pct(d.tex_depth)),
                step(33, "Min Depth", pct(d.tex_min_depth)),
                cyc(
                    34,
                    "Mode",
                    if d.tex_mode == 1 {
                        "Subtract".into()
                    } else {
                        "Multiply".into()
                    },
                ),
                tog(35, "Invert", d.tex_invert),
                tog(36, "Protect Texture", d.protect_texture),
            ],
            4 => vec![
                tog(40, "Dual Brush", d.dual_on),
                step(41, "Diameter", format!("{:.0} px", d.dual_size)),
                step(42, "Hardness", pct(d.dual_hardness)),
                cyc(
                    43,
                    "Mode",
                    pictodesk_core::blend::BlendMode::from_id(d.dual_mode)
                        .map(|m| m.name().to_string())
                        .unwrap_or_else(|| "Multiply".into()),
                ),
                step(44, "Scatter", pct(d.dual_scatter)),
            ],
            5 => vec![
                step(50, "F/B Jitter", pct(d.fb_jitter)),
                step(51, "Hue Jitter", pct(d.hue_jitter)),
                step(52, "Saturation Jit", pct(d.sat_jitter)),
                step(53, "Brightness Jit", pct(d.bri_jitter)),
            ],
            6 => vec![
                step(60, "Opacity Jitter", pct(d.opacity_jitter)),
                cyc(61, "Opacity Control", ctl(d.opacity_control)),
                step(62, "Flow Jitter", pct(d.flow_jitter)),
                cyc(63, "Flow Control", ctl(d.flow_control)),
            ],
            _ => vec![
                tog(70, "Noise", d.noise),
                tog(71, "Wet Edges", d.wet_edges),
                tog(72, "Airbrush", d.airbrush),
                tog(73, "Smoothing", d.smoothing),
            ],
        }
    }

    pub(crate) fn dyn_adjust(&mut self, id: u8, dir: i32) {
        let d = &mut self.brush_dyn;
        let f = dir as f32;
        match id {
            1 => d.angle_deg = (d.angle_deg + f * 5.0).clamp(-180.0, 180.0),
            2 => d.roundness = (d.roundness + f * 0.05).clamp(0.02, 1.0),
            3 => d.spacing = (d.spacing + f * 0.05).clamp(0.01, 2.0),
            10 => d.size_jitter = (d.size_jitter + f * 0.05).clamp(0.0, 1.0),
            12 => d.min_diameter = (d.min_diameter + f * 0.05).clamp(0.0, 1.0),
            13 => d.angle_jitter = (d.angle_jitter + f * 0.05).clamp(0.0, 1.0),
            15 => d.round_jitter = (d.round_jitter + f * 0.05).clamp(0.0, 1.0),
            17 => d.min_roundness = (d.min_roundness + f * 0.05).clamp(0.0, 1.0),
            18 => {
                d.fade_steps = (d.fade_steps as i32 + dir * 10).clamp(10, 9999) as u32;
            }
            20 => d.scatter = (d.scatter + f * 0.1).clamp(0.0, 10.0),
            22 => d.count = (d.count as i32 + dir).clamp(1, 16) as u8,
            23 => d.count_jitter = (d.count_jitter + f * 0.05).clamp(0.0, 1.0),
            31 => d.tex_scale = (d.tex_scale + f * 0.1).clamp(0.1, 4.0),
            32 => d.tex_depth = (d.tex_depth + f * 0.05).clamp(0.0, 1.0),
            33 => d.tex_min_depth = (d.tex_min_depth + f * 0.05).clamp(0.0, 1.0),
            41 => d.dual_size = (d.dual_size + f * 2.0).clamp(1.0, 500.0),
            42 => d.dual_hardness = (d.dual_hardness + f * 0.05).clamp(0.0, 1.0),
            44 => d.dual_scatter = (d.dual_scatter + f * 0.1).clamp(0.0, 10.0),
            50 => d.fb_jitter = (d.fb_jitter + f * 0.05).clamp(0.0, 1.0),
            51 => d.hue_jitter = (d.hue_jitter + f * 0.05).clamp(0.0, 1.0),
            52 => d.sat_jitter = (d.sat_jitter + f * 0.05).clamp(0.0, 1.0),
            53 => d.bri_jitter = (d.bri_jitter + f * 0.05).clamp(0.0, 1.0),
            60 => d.opacity_jitter = (d.opacity_jitter + f * 0.05).clamp(0.0, 1.0),
            62 => d.flow_jitter = (d.flow_jitter + f * 0.05).clamp(0.0, 1.0),
            _ => {}
        }
    }

    pub(crate) fn dyn_toggle(&mut self, id: u8) {
        let d = &mut self.brush_dyn;
        match id {
            4 => d.flip_x = !d.flip_x,
            5 => d.flip_y = !d.flip_y,
            21 => d.both_axes = !d.both_axes,
            30 => d.texture_on = !d.texture_on,
            35 => d.tex_invert = !d.tex_invert,
            36 => d.protect_texture = !d.protect_texture,
            40 => d.dual_on = !d.dual_on,
            70 => d.noise = !d.noise,
            71 => d.wet_edges = !d.wet_edges,
            72 => d.airbrush = !d.airbrush,
            73 => d.smoothing = !d.smoothing,
            _ => {}
        }
    }

    pub(crate) fn dyn_cycle(&mut self, id: u8) {
        let d = &mut self.brush_dyn;
        match id {
            11 => d.size_control = (d.size_control + 1) % 5,
            14 => d.angle_control = (d.angle_control + 1) % 5,
            16 => d.round_control = (d.round_control + 1) % 5,
            34 => d.tex_mode = (d.tex_mode + 1) % 2,
            43 => {
                d.dual_mode = (d.dual_mode + 1) % 22;
            }
            61 => d.opacity_control = (d.opacity_control + 1) % 5,
            63 => d.flow_control = (d.flow_control + 1) % 5,
            _ => {}
        }
    }

    pub(crate) fn dyn_section_rect(&self, r: Rect, i: u8) -> Rect {
        Rect::new(r.x + 4, r.y + 86 + i as i32 * 13, r.w - 8, 12)
    }

    pub(crate) fn dyn_row_rects(&self, r: Rect, j: usize) -> (Rect, Rect, Rect) {
        let y = r.y + 196 + j as i32 * 17;
        (
            Rect::new(r.x + 96, y, 62, 15),
            Rect::new(r.x + 160, y, 16, 15),
            Rect::new(r.x + 178, y, 16, 15),
        )
    }

    pub(crate) fn draw_brushes(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Brushes) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Brushes", 1, ui::TEXT);
        f.text(
            r.x + 150,
            r.y + 5,
            &format!("{} px", self.tip.size.round() as i32),
            1,
            ui::TEXT,
        );
        for (idx, p) in self.brush_presets.iter().enumerate() {
            let (sz, hardness) = (p.size, p.hardness);
            let (row, i) = ((idx / 8) as i32, (idx % 8) as i32);
            let cx = r.x + 16 + i * 23;
            let cy = r.y + 30 + row * 26;
            let rad = (sz / 2.0).min(8.0) as i32;
            let col = if hardness > 0.5 {
                ui::TEXT
            } else {
                ui::TEXT_DIM
            };
            for yy in -rad..=rad {
                for xx in -rad..=rad {
                    if xx * xx + yy * yy <= rad * rad {
                        f.set(cx + xx, cy + yy, col);
                    }
                }
            }
            if (self.tip.size - sz).abs() < 0.1
                && (self.tip.hardness - hardness).abs() < 0.1
                && self.brush_dyn == p.dyn_
            {
                f.outline_rect(Rect::new(cx - 10, cy - 10, 20, 20), ui::ACCENT);
            }
        }
        // Phase 7: dynamics section list + the section's rows.
        for (i, name) in Self::brush_section_names().iter().enumerate() {
            let sr = self.dyn_section_rect(r, i as u8);
            let label = if self.brush_section_active(i as u8) {
                format!("{name} *")
            } else {
                name.to_string()
            };
            f.button(sr, &label, self.brush_section == i as u8, true);
        }
        for (j, (id, label, value, kind)) in self.dyn_row_defs().iter().enumerate() {
            let (vb, minus, plus) = self.dyn_row_rects(r, j);
            f.text(r.x + 6, vb.y + 4, label, 1, ui::TEXT);
            f.button(vb, value, false, true);
            if *kind == 0 {
                f.button(minus, "-", false, true);
                f.button(plus, "+", false, true);
            }
            let _ = id;
        }
        Self::draw_small_button(
            f,
            Rect::new(r.x + 5, r.bottom() - 16, 100, 13),
            "Save .ABR",
            true,
        );
        Self::draw_small_button(
            f,
            Rect::new(r.x + 110, r.bottom() - 16, 100, 13),
            "Load .ABR",
            true,
        );
        self.draw_tone_row(f);
        self.draw_tool_row(f);
    }

    /// Option row rects for the Phase 5 tools that carry options
    /// in the Brushes panel: (value button, minus, plus). The
    /// Eraser and Art History rows use only the value button.
    /// Whether the current tool shows the Brushes panel's option
    /// row (tool-match only — no rect lookup, so panel_height()
    /// can call it without recursing through group_rect).
    pub(crate) fn has_tool_row(&self) -> bool {
        matches!(
            self.tool,
            Tool::Eraser
                | Tool::MagicEraser
                | Tool::BackgroundEraser
                | Tool::ArtHistoryBrush
                | Tool::Gradient
                | Tool::ShapeRect
                | Tool::ShapeRounded
                | Tool::ShapeEllipse
                | Tool::ShapePolygon
                | Tool::ShapeLine
                | Tool::ShapeCustom
        )
    }

    pub(crate) fn tool_row_rects(&self) -> Option<(Rect, Rect, Rect)> {
        if !self.has_tool_row() {
            return None;
        }
        let r = self.group_rect(PanelKind::Brushes)?;
        let y = r.bottom() - 21;
        Some((
            Rect::new(r.x + 52, y, 96, 16),
            Rect::new(r.x + 152, y, 16, 16),
            Rect::new(r.x + 190, y, 16, 16),
        ))
    }

    pub(crate) fn draw_tool_row(&mut self, f: &mut Frame) {
        let Some((value_btn, minus, plus)) = self.tool_row_rects() else {
            return;
        };
        let r = self.group_rect(PanelKind::Brushes).unwrap();
        f.fill_rect(Rect::new(r.x + 4, r.bottom() - 24, r.w - 8, 1), ui::SHADOW);
        match self.tool {
            Tool::Eraser => {
                f.text(r.x + 6, value_btn.y + 4, "Mode:", 1, ui::TEXT);
                f.button(
                    value_btn,
                    if self.eraser_to_history {
                        "Erase to History"
                    } else {
                        "Erase"
                    },
                    self.eraser_to_history,
                    true,
                );
            }
            Tool::Gradient => {
                f.text(r.x + 6, value_btn.y + 4, "Shape:", 1, ui::TEXT);
                f.button(
                    value_btn,
                    match self.gradient_shape {
                        brush::GradientShape::Linear => "Linear",
                        brush::GradientShape::Radial => "Radial",
                        brush::GradientShape::Angle => "Angle",
                        brush::GradientShape::Diamond => "Diamond",
                        brush::GradientShape::Reflected => "Reflected",
                    },
                    false,
                    true,
                );
            }
            Tool::Stamp => {
                f.text(r.x + 6, value_btn.y + 4, "Sample:", 1, ui::TEXT);
                f.button(
                    value_btn,
                    if self.stamp_sample_merged {
                        "Merged"
                    } else {
                        "Layer"
                    },
                    self.stamp_sample_merged,
                    true,
                );
            }
            Tool::Eyedropper => {
                f.text(r.x + 6, value_btn.y + 4, "Sample:", 1, ui::TEXT);
                f.button(
                    value_btn,
                    if self.dropper_sample_merged {
                        "Merged"
                    } else {
                        "Layer"
                    },
                    self.dropper_sample_merged,
                    true,
                );
            }
            Tool::ArtHistoryBrush => {
                f.text(r.x + 6, value_btn.y + 4, "Style:", 1, ui::TEXT);
                f.button(
                    value_btn,
                    match self.art_hist_style {
                        0 => "Tight",
                        1 => "Loose",
                        _ => "Dab",
                    },
                    false,
                    true,
                );
            }
            Tool::ShapePolygon => {
                f.text(r.x + 6, value_btn.y + 4, "Sides:", 1, ui::TEXT);
                f.button(value_btn, &format!("{}", self.shape_sides), false, true);
                f.button(minus, "-", false, true);
                f.button(plus, "+", false, true);
            }
            Tool::ShapeCustom => {
                f.text(r.x + 6, value_btn.y + 4, "Shape:", 1, ui::TEXT);
                let (name, _) = self.custom_shape_choice(self.custom_idx);
                f.button(value_btn, &name, false, true);
            }
            Tool::ShapeRect | Tool::ShapeRounded | Tool::ShapeEllipse | Tool::ShapeLine => {
                f.text(r.x + 6, value_btn.y + 4, "Stroke:", 1, ui::TEXT);
                f.button(value_btn, &format!("{} px", self.shape_stroke), false, true);
                f.button(minus, "-", false, true);
                f.button(plus, "+", false, true);
            }
            _ => {
                // Magic / Background Eraser: tolerance stepper.
                f.text(r.x + 6, value_btn.y + 4, "Tol:", 1, ui::TEXT);
                f.button(
                    value_btn,
                    &format!("{}", self.eraser_tolerance),
                    false,
                    true,
                );
                f.button(minus, "-", false, true);
                f.button(plus, "+", false, true);
            }
        }
    }

    /// The toning tools' option row at the bottom of the Brushes
    /// panel: (value button, exposure -, exposure +). Present only
    /// while Dodge, Burn or Sponge is the active tool.
    pub(crate) fn tone_row_rects(&self) -> Option<(Rect, Rect, Rect)> {
        if !matches!(self.tool, Tool::Dodge | Tool::Burn | Tool::Sponge) {
            return None;
        }
        let r = self.group_rect(PanelKind::Brushes)?;
        let y = r.bottom() - 21;
        Some((
            Rect::new(r.x + 52, y, 76, 16),
            Rect::new(r.x + 152, y, 16, 16),
            Rect::new(r.x + 190, y, 16, 16),
        ))
    }

    pub(crate) fn draw_tone_row(&mut self, f: &mut Frame) {
        let Some((value_btn, minus, plus)) = self.tone_row_rects() else {
            return;
        };
        let r = self.group_rect(PanelKind::Brushes).unwrap();
        f.fill_rect(Rect::new(r.x + 4, r.bottom() - 24, r.w - 8, 1), ui::SHADOW);
        if self.tool == Tool::Sponge {
            f.text(r.x + 6, value_btn.y + 4, "Mode:", 1, ui::TEXT);
            f.button(
                value_btn,
                if self.sponge_saturate {
                    "Saturate"
                } else {
                    "Desaturate"
                },
                false,
                true,
            );
        } else {
            f.text(r.x + 6, value_btn.y + 4, "Range:", 1, ui::TEXT);
            f.button(
                value_btn,
                match self.tone_range {
                    brush::ToneRange::Shadows => "Shadows",
                    brush::ToneRange::Midtones => "Midtones",
                    brush::ToneRange::Highlights => "Highlights",
                },
                false,
                true,
            );
        }
        f.button(minus, "-", false, true);
        f.text(
            minus.x + 21,
            minus.y + 4,
            &format!("{:.0}%", self.tone_exposure * 100.0),
            1,
            ui::TEXT,
        );
        f.button(plus, "+", false, true);
    }

    pub(crate) fn brushes_cell(&self, i: usize, row: usize) -> Rect {
        let r = self
            .group_rect(PanelKind::Brushes)
            .unwrap_or(Rect::new(0, 0, 0, 0));
        Rect::new(r.x + 5 + i as i32 * 23, r.y + 19 + row as i32 * 26, 22, 22)
    }

    pub(crate) fn draw_histogram_panel(&mut self, f: &mut Frame) {
        self.ensure_render_cache();
        let Some(r) = self.group_rect(PanelKind::Histogram) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Histogram", 1, ui::TEXT);
        let mut lum = [0u32; 256];
        for px in self.cached_render.as_chunks::<4>().0 {
            let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as usize;
            lum[l.min(255)] += 1;
        }
        let peak = lum.iter().max().copied().unwrap_or(1).max(1);
        let graph = Rect::new(r.x + 8, r.y + 18, r.w - 16, 84);
        f.fill_rect(graph, ui::WHITE);
        f.bevel_sunken(graph);
        for i in 0..(r.w - 16) {
            let bin = lum[i as usize * 256 / (r.w - 16) as usize];
            let hgt = ((bin as f32 / peak as f32).sqrt() * graph.h as f32) as i32;
            f.fill_rect(
                Rect::new(graph.x + i, graph.bottom() - hgt, 1, hgt),
                ui::BLACK,
            );
        }
        let hist = Histogram::of_document(&self.doc);
        f.text(
            r.x + 8,
            r.bottom() - 14,
            &format!(
                "Mean {:.0}  Median {}",
                hist.mean_luminance(),
                hist.median_luminance()
            ),
            1,
            ui::TEXT,
        );
    }

    pub(crate) fn channel_rows(&self) -> Vec<(String, ChannelView)> {
        let mut rows = Vec::new();
        let comp = match self.doc.mode {
            ColorMode::Rgb => "RGB",
            ColorMode::Cmyk => "CMYK",
            ColorMode::Grayscale => "Gray",
            ColorMode::Lab => "Lab",
            ColorMode::Indexed => "RGB",
            ColorMode::Bitmap => "Bitmap",
            ColorMode::Duotone => "Gray",
            ColorMode::Multichannel => "Multi",
        };
        rows.push((comp.to_string(), ChannelView::Composite));
        let multi_names: Vec<String> = self
            .doc
            .multi_channels
            .iter()
            .map(|m| m.name.clone())
            .collect();
        let names: &[&str] = match self.doc.mode {
            ColorMode::Rgb | ColorMode::Indexed => &["Red", "Green", "Blue"],
            ColorMode::Cmyk => &["Cyan", "Magenta", "Yellow", "Black"],
            ColorMode::Grayscale | ColorMode::Bitmap | ColorMode::Duotone => &["Gray"],
            ColorMode::Lab => &["Lightness", "a", "b"],
            ColorMode::Multichannel => &[],
        };
        if self.doc.mode == ColorMode::Multichannel {
            for (i, n) in multi_names.iter().enumerate() {
                rows.push((n.clone(), ChannelView::Channel(i)));
            }
        }
        for (i, n) in names.iter().enumerate() {
            rows.push((n.to_string(), ChannelView::Channel(i)));
        }
        for (i, ch) in self.doc.alphas.iter().enumerate() {
            let mut label = ch.name.clone();
            if ch.spot_color.is_some() {
                label += " (spot)";
            }
            rows.push((label, ChannelView::Alpha(i)));
        }
        rows
    }

    pub(crate) fn path_rows(&self) -> Vec<(String, PathRef)> {
        let mut rows = Vec::new();
        if self.doc.work_path.is_some() {
            rows.push(("Work Path".to_string(), PathRef::Work));
        }
        for (i, p) in self.doc.paths.iter().enumerate() {
            let mut label = if p.name.is_empty() {
                format!("Path {}", i + 1)
            } else {
                p.name.clone()
            };
            if self.doc.clipping_path == Some(i) {
                label += " (clipping)";
            }
            rows.push((label, PathRef::Saved(i)));
        }
        rows
    }

    pub(crate) fn draw_small_button(f: &mut Frame, r: Rect, label: &str, enabled: bool) {
        f.fill_rect(r, ui::PLATINUM);
        f.bevel_raised(r);
        let c = if enabled { ui::TEXT } else { ui::DISABLED };
        let tw = Frame::text_width(label, 1);
        f.text(r.x + (r.w - tw) / 2, r.y + 4, label, 1, c);
    }

    pub(crate) fn draw_channels_paths(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Cp) else {
            return;
        };
        f.panel(r);
        for (i, label) in ["Layers", "Channels", "Paths"].iter().enumerate() {
            let tr = Self::cp_tab_rect(r, i);
            if self.cp_tab == i {
                f.fill_rect(tr, ui::PLATINUM_LIGHT);
                f.bevel_raised(tr);
                f.text(tr.x + 5, tr.y + 3, label, 1, ui::TEXT);
            } else {
                f.outline_rect(tr, ui::SHADOW);
                f.text(tr.x + 5, tr.y + 3, label, 1, ui::TEXT_DIM);
            }
        }
        if self.cp_tab == 0 {
            self.draw_layers_panel(f, r);
        } else if self.cp_tab == 1 {
            let rows = self.channel_rows();
            for (i, (label, view)) in rows.iter().enumerate() {
                let rr = Rect::new(r.x + 2, r.y + 20 + i as i32 * 15, r.w - 4, 15);
                let selected = self.channel_view == *view;
                if selected {
                    f.fill_rect(rr, ui::SELROW);
                }
                let c = if selected { ui::WHITE } else { ui::TEXT };
                if let ChannelView::Alpha(a) = view {
                    if self.paint_target == PaintTarget::Alpha(*a) {
                        f.fill_rect(Rect::new(rr.x + 3, rr.y + 5, 5, 5), c);
                    }
                }
                f.text(rr.x + 13, rr.y + 3, label, 1, c);
            }
            let labels = ["Save", "Load", "New", "Del", "Spot"];
            let bw = (r.w - 8) / 5;
            for (i, label) in labels.iter().enumerate() {
                let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
                Self::draw_small_button(f, br, label, true);
            }
        } else {
            let rows = self.path_rows();
            if rows.is_empty() {
                f.text(r.x + 8, r.y + 24, "No paths yet.", 1, ui::DARK);
                f.text(r.x + 8, r.y + 36, "Draw with the Pen (P),", 1, ui::DARK);
                f.text(r.x + 8, r.y + 48, "or Make from a selection.", 1, ui::DARK);
            }
            for (i, (label, pref)) in rows.iter().enumerate() {
                let rr = Rect::new(r.x + 2, r.y + 20 + i as i32 * 15, r.w - 4, 15);
                let selected = self.active_path == Some(*pref);
                if selected {
                    f.fill_rect(rr, ui::SELROW);
                }
                let c = if selected { ui::WHITE } else { ui::TEXT };
                f.text(rr.x + 6, rr.y + 3, label, 1, c);
            }
            let row1 = ["Fill", "Stroke", "Select", "Make"];
            let row2 = ["Save", "Clip", "Del"];
            let bw = (r.w - 8) / 4;
            for (i, label) in row1.iter().enumerate() {
                let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 39, bw - 2, 16);
                Self::draw_small_button(f, br, label, true);
            }
            let bw2 = (r.w - 8) / 3;
            for (i, label) in row2.iter().enumerate() {
                let br = Rect::new(r.x + 4 + i as i32 * bw2, r.bottom() - 19, bw2 - 2, 16);
                Self::draw_small_button(f, br, label, true);
            }
        }
    }

    /// The Navigator panel (Phase 4): a thumbnail of the whole
    /// document with a red rectangle marking the visible area;
    /// clicking or dragging in it pans the main view.
    pub(crate) fn draw_navigator(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Navigator) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Navigator", 1, ui::TEXT);
        let pct = (self.viewport.scale * 100.0).round() as i32;
        f.text(r.x + 130, r.y + 5, &format!("{pct}%"), 1, ui::DARK);
        let th = Self::nav_thumb_rect(r);
        self.ensure_render_cache();
        let (dw, dh) = (self.doc.width as i32, self.doc.height as i32);
        for yy in 0..th.h {
            for xx in 0..th.w {
                let dx = (xx * dw / th.w).min(dw - 1);
                let dy = (yy * dh / th.h).min(dh - 1);
                let i = ((dy * dw + dx) * 4) as usize;
                let (pr, pg, pb, pa) = (
                    self.cached_render[i],
                    self.cached_render[i + 1],
                    self.cached_render[i + 2],
                    self.cached_render[i + 3],
                );
                let col = if pa == 255 {
                    (pr, pg, pb)
                } else {
                    let cell = (((xx / 4) + (yy / 4)) % 2 == 0) as u32;
                    let bg = if cell == 0 { 255 } else { 170 };
                    let a = pa as u32;
                    (
                        ((pr as u32 * a + bg * (255 - a)) / 255) as u8,
                        ((pg as u32 * a + bg * (255 - a)) / 255) as u8,
                        ((pb as u32 * a + bg * (255 - a)) / 255) as u8,
                    )
                };
                f.set(th.x + xx, th.y + yy, col);
            }
        }
        f.outline_rect(th, ui::SHADOW);
        // Visible-area rectangle.
        let c = self.canvas_rect(f.w, f.h);
        let tl = self.viewport.screen_to_doc((0.0, 0.0));
        let br = self.viewport.screen_to_doc((c.w as f32, c.h as f32));
        let vx = th.x + (tl.0 / dw as f32 * th.w as f32) as i32;
        let vy = th.y + (tl.1 / dh as f32 * th.h as f32) as i32;
        let vw = ((br.0 - tl.0) / dw as f32 * th.w as f32) as i32;
        let vh = ((br.1 - tl.1) / dh as f32 * th.h as f32) as i32;
        // Clamp to the thumbnail: when the whole document is
        // visible, the view rectangle is the thumbnail itself.
        let x0 = vx.max(th.x);
        let y0 = vy.max(th.y);
        let x1 = (vx + vw).min(th.right());
        let y1 = (vy + vh).min(th.bottom());
        if x1 > x0 && y1 > y0 {
            f.outline_rect(Rect::new(x0, y0, x1 - x0, y1 - y0), (255, 0, 0));
        }
    }

    pub(crate) fn nav_thumb_rect(r: Rect) -> Rect {
        Rect::new(r.x + 6, r.y + 18, r.w - 12, 88)
    }

    /// Pan so the document point under a Navigator thumbnail point
    /// sits at the center of the canvas.
    pub(crate) fn navigator_to(&mut self, mx: i32, my: i32) {
        let Some(r) = self.group_rect(PanelKind::Navigator) else {
            return;
        };
        let th = Self::nav_thumb_rect(r);
        let fx = ((mx - th.x) as f32 / th.w as f32).clamp(0.0, 1.0);
        let fy = ((my - th.y) as f32 / th.h as f32).clamp(0.0, 1.0);
        let (dx, dy) = (fx * self.doc.width as f32, fy * self.doc.height as f32);
        let c = self.canvas_rect(self.win_size.0, self.win_size.1);
        self.viewport.offset = (
            c.w as f32 / 2.0 - dx * self.viewport.scale,
            c.h as f32 / 2.0 - dy * self.viewport.scale,
        );
    }

    /// The Actions panel (Phase 4): action list with step counts,
    /// and the New / Record / Play / Delete / Batch buttons. While
    /// recording, a red REC marker shows.
    /// Actions panel rows: a header per set that has actions (or
    /// is the current set), then that set's actions.
    pub(crate) fn action_rows(&self) -> Vec<ActRow> {
        let mut rows = Vec::new();
        for (si, sname) in self.action_sets.iter().enumerate() {
            let has = self.actions.iter().any(|a| &a.set == sname);
            if has || sname == &self.action_set {
                rows.push(ActRow::Set(si));
                for (ai, a) in self.actions.iter().enumerate() {
                    if &a.set == sname {
                        rows.push(ActRow::Action(ai));
                    }
                }
            }
        }
        rows
    }

    pub(crate) fn draw_actions_panel(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Actions) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Actions", 1, ui::TEXT);
        if self.recording {
            f.fill_rect(Rect::new(r.x + 70, r.y + 5, 7, 7), (255, 0, 0));
            f.text(r.x + 81, r.y + 5, "REC", 1, (200, 0, 0));
        }
        if self.actions.is_empty() {
            f.text(r.x + 8, r.y + 24, "No actions yet.", 1, ui::DARK);
            f.text(r.x + 8, r.y + 38, "New, then Record, then", 1, ui::DARK);
            f.text(r.x + 8, r.y + 50, "work: steps are captured.", 1, ui::DARK);
        }
        let rows = self.action_rows();
        let visible = ((r.h - 78) / 15).max(1) as usize;
        for (vi, row) in rows.iter().take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            match row {
                ActRow::Set(si) => {
                    f.fill_rect(rr, ui::PLATINUM);
                    let name = &self.action_sets[*si];
                    let cur = name == &self.action_set;
                    f.text(
                        rr.x + 4,
                        rr.y + 3,
                        &format!("{}{}", if cur { "> " } else { "" }, name),
                        1,
                        ui::BLACK,
                    );
                }
                ActRow::Action(ai) => {
                    let selected = *ai == self.active_action;
                    if selected {
                        f.fill_rect(rr, ui::SELROW);
                    }
                    let c = if selected { ui::WHITE } else { ui::TEXT };
                    let a = &self.actions[*ai];
                    f.text(
                        rr.x + 14,
                        rr.y + 3,
                        &format!("{} ({})", a.name, a.steps.len()),
                        1,
                        c,
                    );
                }
            }
        }
        // The selected action's steps, above the buttons.
        if let Some(a) = self.actions.get(self.active_action) {
            let sy = r.bottom() - 75;
            for (j, s) in a.steps.iter().take(3).enumerate() {
                f.text(
                    r.x + 14,
                    sy + j as i32 * 11,
                    &format!("- {}", s.label()),
                    1,
                    ui::DARK,
                );
            }
            if a.steps.len() > 3 {
                f.text(
                    r.x + 14,
                    sy + 33,
                    &format!("  +{} more", a.steps.len() - 3),
                    1,
                    ui::DARK,
                );
            }
        }
        // New-set button in the header (the classic flyout spot).
        Self::draw_small_button(f, Rect::new(r.right() - 18, r.y + 3, 14, 13), "+", true);
        // Button Mode toggle (Phase 6.5 Batch 6d).
        Self::draw_small_button(
            f,
            Rect::new(r.right() - 52, r.y + 3, 30, 13),
            if self.actions_button_mode {
                "List"
            } else {
                "Btn"
            },
            true,
        );
        if self.actions_button_mode {
            if self.actions.is_empty() {
                f.text(r.x + 8, r.y + 24, "No actions yet.", 1, ui::DARK);
            }
            for (i, a) in self.actions.iter().take(8).enumerate() {
                let br = Rect::new(r.x + 4, r.y + 20 + i as i32 * 19, r.w - 8, 17);
                let label: String = if a.name.chars().count() > 24 {
                    a.name.chars().take(24).collect()
                } else {
                    a.name.clone()
                };
                Self::draw_small_button(f, br, &label, true);
            }
            return;
        }
        let labels = ["New", "Rec", "Play", "Del", "Batch"];
        let bw = (r.w - 8) / 5;
        for (i, label) in labels.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
            let text = if i == 1 && self.recording {
                "Stop"
            } else {
                label
            };
            Self::draw_small_button(f, br, text, true);
        }
        // Insert row (Phase 6.5 Batch 6d).
        let bw2 = (r.w - 8) / 2;
        for (i, label) in ["Ins Stop", "Ins Menu"].iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw2, r.bottom() - 38, bw2 - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
    }

    pub(crate) fn actions_panel_click(&mut self, r: Rect, mx: i32, my: i32) -> bool {
        let rows = self.action_rows();
        let visible = ((r.h - 78) / 15).max(1) as usize;
        for (vi, row) in rows.iter().take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            if rr.contains(mx, my) {
                match row {
                    ActRow::Set(si) => {
                        self.action_set = self.action_sets[*si].clone();
                        self.status = format!("Action set: {}", self.action_set);
                    }
                    ActRow::Action(ai) => {
                        self.active_action = *ai;
                    }
                }
                return true;
            }
        }
        if Rect::new(r.right() - 52, r.y + 3, 30, 13).contains(mx, my) {
            self.actions_button_mode = !self.actions_button_mode;
            self.status = if self.actions_button_mode {
                "Actions: Button Mode (click an action to play it)".into()
            } else {
                "Actions: list mode".into()
            };
            return true;
        }
        if self.actions_button_mode {
            for i in 0..self.actions.len().min(8) {
                let br = Rect::new(r.x + 4, r.y + 20 + i as i32 * 19, r.w - 8, 17);
                if br.contains(mx, my) {
                    self.active_action = i;
                    self.play_action(i);
                    return true;
                }
            }
            return true;
        }
        if Rect::new(r.right() - 18, r.y + 3, 14, 13).contains(mx, my) {
            let mut d = Dialog::new(DialogKind::ActionSetName, &self.doc);
            d.text = format!("Set {}", self.action_sets.len() + 1);
            d.text_focus = true;
            self.dialog = Some(d);
            return true;
        }
        let bw2 = (r.w - 8) / 2;
        for i in 0..2i32 {
            let br = Rect::new(r.x + 4 + i * bw2, r.bottom() - 38, bw2 - 2, 16);
            if br.contains(mx, my) {
                if i == 0 {
                    self.open_insert_stop();
                } else {
                    self.open_insert_menu();
                }
                return true;
            }
        }
        let bw = (r.w - 8) / 5;
        for i in 0..5i32 {
            let br = Rect::new(r.x + 4 + i * bw, r.bottom() - 19, bw - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::MacroNew),
                    1 => {
                        if self.recording {
                            self.do_action(Action::MacroStop)
                        } else {
                            self.do_action(Action::MacroRecord)
                        }
                    }
                    2 => self.do_action(Action::MacroPlay),
                    3 => self.do_action(Action::MacroDelete),
                    _ => self.do_action(Action::MacroBatch),
                }
                return true;
            }
        }
        true
    }

    /// History panel rows in display order: snapshots first, then
    /// states newest-to-oldest (state index = commands applied).
    pub(crate) fn hist_rows(&self) -> Vec<HistRow> {
        let mut rows = Vec::new();
        for i in 0..self.snapshots.len() {
            rows.push(HistRow::Snapshot(i));
        }
        for idx in (0..=self.hist.state_count()).rev() {
            rows.push(HistRow::State(idx));
        }
        rows
    }

    pub(crate) fn hist_row_label(&self, row: HistRow) -> String {
        match row {
            HistRow::Snapshot(i) => self
                .snapshots
                .get(i)
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            HistRow::State(0) => {
                let base = self
                    .file_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                format!("{base} (Open)")
            }
            HistRow::State(idx) => self
                .hist
                .labels()
                .get(idx - 1)
                .cloned()
                .unwrap_or_else(|| format!("State {idx}")),
        }
    }

    pub(crate) fn draw_history_panel(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::History) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "History", 1, ui::TEXT);
        // Scroll buttons (rows hidden above/below).
        let rows = self.hist_rows();
        let visible = ((r.h - 58) / 15).max(1) as usize;
        if rows.len() > visible {
            Self::draw_small_button(f, Rect::new(r.right() - 34, r.y + 3, 14, 13), "^", true);
            Self::draw_small_button(f, Rect::new(r.right() - 18, r.y + 3, 14, 13), "v", true);
        }
        let pos = self.hist.position();
        for (vi, row) in rows.iter().skip(self.hist_scroll).take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            let is_current = matches!(row, HistRow::State(idx) if *idx == pos);
            let is_selected = self.hist_selected == Some(*row);
            if is_current {
                f.fill_rect(rr, ui::SELROW);
            } else if is_selected {
                f.fill_rect(rr, ui::PLATINUM_LIGHT);
            }
            let c = if is_current { ui::WHITE } else { ui::TEXT };
            // Source box: the state the History Brush paints from.
            let sb = Rect::new(rr.x + 2, rr.y + 2, 11, 11);
            let is_source = match (row, self.hist_source) {
                (HistRow::Snapshot(a), HistSource::Snapshot(b)) => *a == b,
                (HistRow::State(a), HistSource::State(b)) => *a == b,
                _ => false,
            };
            f.outline_rect(sb, c);
            if is_source {
                f.fill_rect(Rect::new(sb.x + 2, sb.y + 2, 7, 7), c);
            }
            let tag = match row {
                HistRow::Snapshot(_) => "S ",
                HistRow::State(_) => "",
            };
            f.text(
                rr.x + 17,
                rr.y + 3,
                &format!("{tag}{}", self.hist_row_label(*row)),
                1,
                c,
            );
        }
        let row_a = ["Snap", "Merge", "Layer"];
        let bw3 = (r.w - 8) / 3;
        for (i, label) in row_a.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw3, r.bottom() - 36, bw3 - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
        let labels = ["New", "Opts", "Trash"];
        for (i, label) in labels.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw3, r.bottom() - 19, bw3 - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
    }

    pub(crate) fn history_panel_click(&mut self, r: Rect, mx: i32, my: i32) -> bool {
        let rows = self.hist_rows();
        let visible = ((r.h - 58) / 15).max(1) as usize;
        if rows.len() > visible {
            let up = Rect::new(r.right() - 34, r.y + 3, 14, 13);
            let dn = Rect::new(r.right() - 18, r.y + 3, 14, 13);
            if up.contains(mx, my) {
                self.hist_scroll = self.hist_scroll.saturating_sub(1);
                return true;
            }
            if dn.contains(mx, my) {
                let max = rows.len() - visible;
                self.hist_scroll = (self.hist_scroll + 1).min(max);
                return true;
            }
        }
        for (vi, row) in rows.iter().skip(self.hist_scroll).take(visible).enumerate() {
            let rr = Rect::new(r.x + 2, r.y + 20 + vi as i32 * 15, r.w - 4, 15);
            if rr.contains(mx, my) {
                let row = *row;
                self.hist_selected = Some(row);
                // Left source box: choose the brush/fill source.
                if mx < rr.x + 15 {
                    self.hist_source = match row {
                        HistRow::Snapshot(i) => HistSource::Snapshot(i),
                        HistRow::State(i) => HistSource::State(i),
                    };
                    self.hist_src_cache = None;
                    self.status = "History source set".into();
                    return true;
                }
                match row {
                    HistRow::State(idx) => {
                        if self.hist.goto_state(&mut self.doc, idx) {
                            self.doc_version += 1;
                            self.dirty = true;
                            self.status = format!("State: {}", self.hist_row_label(row));
                        }
                    }
                    HistRow::Snapshot(i) => {
                        if let Some(snap) = self.snapshots.get(i).cloned() {
                            let mut cmd = DocumentCommand::begin(&snap.name, &self.doc);
                            self.doc = snap.doc;
                            cmd.finish(&self.doc);
                            self.hist.push(Box::new(cmd));
                            self.doc_version += 1;
                            self.dirty = true;
                            self.status = format!("Snapshot: {}", snap.name);
                        }
                    }
                }
                return true;
            }
        }
        let bw3 = (r.w - 8) / 3;
        for i in 0..3i32 {
            let br = Rect::new(r.x + 4 + i * bw3, r.bottom() - 36, bw3 - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::HistSnapshot),
                    1 => self.do_action(Action::HistSnapshotMerged),
                    _ => self.do_action(Action::HistSnapshotLayer),
                }
                return true;
            }
        }
        for i in 0..3i32 {
            let br = Rect::new(r.x + 4 + i * bw3, r.bottom() - 19, bw3 - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::HistNewDoc),
                    1 => self.do_action(Action::HistOptions),
                    _ => self.do_action(Action::HistTrash),
                }
                return true;
            }
        }
        true
    }

    pub(crate) fn draw_info_panel(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::Info) else {
            return;
        };
        f.panel(r);
        f.text(r.x + 6, r.y + 5, "Info", 1, ui::TEXT);
        let mut y = r.y + 22;
        let line = |f: &mut Frame, y: i32, s: &str| {
            f.text(r.x + 8, y, s, 1, ui::DARK);
        };
        let mode_name = match self.doc.mode {
            ColorMode::Rgb => "RGB",
            ColorMode::Cmyk => "CMYK",
            ColorMode::Grayscale => "Grayscale",
            ColorMode::Lab => "Lab",
            ColorMode::Indexed => "Indexed",
            ColorMode::Bitmap => "Bitmap",
            ColorMode::Duotone => "Duotone",
            ColorMode::Multichannel => "Multichannel",
        };
        line(
            f,
            y,
            &format!(
                "Image: {} x {} px, {}",
                self.doc.width, self.doc.height, mode_name
            ),
        );
        y += 13;
        let (fx, fy) = self.to_doc(self.cursor.0, self.cursor.1);
        let (dx, dy) = (fx.floor() as i32, fy.floor() as i32);
        if dx >= 0 && dy >= 0 && (dx as u32) < self.doc.width && (dy as u32) < self.doc.height {
            let p = self.doc.render_pixel(dx, dy);
            let (cr, cg, cb, ca) = p.to_rgba8();
            line(f, y, &format!("X: {dx}  Y: {dy}"));
            y += 13;
            line(f, y, &format!("R: {cr}  G: {cg}  B: {cb}  A: {ca}"));
            y += 13;
            if self.doc.mode == ColorMode::Cmyk {
                let (inks, _) = self.doc.composite_inks(dx, dy);
                let pc = |v: f32| (v * 100.0).round() as i32;
                line(
                    f,
                    y,
                    &format!(
                        "C: {}%  M: {}%  Y: {}%  K: {}%",
                        pc(inks[0]),
                        pc(inks[1]),
                        pc(inks[2]),
                        pc(inks[3])
                    ),
                );
                y += 13;
            }
        } else {
            line(f, y, "Move over the image for values.");
            y += 13;
        }
        for (i, (sx, sy)) in self.doc.samplers.iter().enumerate() {
            let p = self.doc.render_pixel(*sx, *sy);
            let (cr, cg, cb, _) = p.to_rgba8();
            line(
                f,
                y,
                &format!("#{}: R{cr} G{cg} B{cb}  ({sx}, {sy})", i + 1),
            );
            y += 13;
        }
        if let Some(((x0, y0), (x1, y1))) = self.measure {
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx).to_degrees();
            line(f, y, &format!("D: {len:.1} px  A: {ang:.1} deg"));
        }
    }

    pub(crate) fn cp_tab_rect(r: Rect, i: usize) -> Rect {
        match i {
            0 => Rect::new(r.x + 3, r.y + 3, 52, 15),
            1 => Rect::new(r.x + 58, r.y + 3, 70, 15),
            _ => Rect::new(r.x + 131, r.y + 3, 50, 15),
        }
    }

    /// The Layers panel's display rows (v0.6.1): top of stack
    /// first, with a folder header row above each contiguous
    /// run of set members. Non-contiguous members of one set
    /// get one header per run — the honest picture of the
    /// stack.
    pub(crate) fn panel_rows(&self) -> Vec<PanelRow> {
        let mut rows = Vec::new();
        let n = self.doc.layers.len();
        let mut open_set: Option<u16> = None;
        for i in 0..n {
            let li = n - 1 - i;
            let g = self.doc.layers[li].group;
            if g > 0 && open_set != Some(g) && self.doc.sets.get((g - 1) as usize).is_some() {
                rows.push(PanelRow::Set((g - 1) as usize));
                open_set = Some(g);
            }
            if g == 0 {
                open_set = None;
            }
            rows.push(PanelRow::Layer(li));
        }
        rows
    }

    pub(crate) fn layer_row_rect(r: Rect, row: usize, row_h: i32) -> Rect {
        Rect::new(r.x + 2, r.y + 48 + row as i32 * row_h, r.w - 4, row_h)
    }

    /// Layers-panel thumbnail size (Phase 6.5 Batch 6c):
    /// 0 = small (28px rows), 1 = medium (40), 2 = large (54).
    pub(crate) fn layer_row_h(&self) -> i32 {
        [28, 40, 54][(self.layer_thumb as usize).min(2)]
    }

    pub(crate) fn layer_thumb_wh(&self) -> (i32, i32) {
        [(36, 24), (54, 36), (75, 50)][(self.layer_thumb as usize).min(2)]
    }

    pub(crate) fn draw_layers_panel(&mut self, f: &mut Frame, r: Rect) {
        let a = self.doc.active_layer;
        let (mode_name, opacity) = {
            let l = &self.doc.layers[a];
            (l.blend_mode.name(), (l.opacity * 100.0).round() as i32)
        };
        f.text(
            r.x + 6,
            r.y + 23,
            &format!("Mode: {mode_name}"),
            1,
            ui::TEXT,
        );
        f.text(
            r.x + 6,
            r.y + 36,
            &format!("Opacity: {opacity}%"),
            1,
            ui::TEXT,
        );
        // (The active layer's set is named by the folder
        // header row above it in the panel — v0.6.1 removed a
        // redundant "Set:" label here that collided with the
        // Opacity text on the same line.)
        let rows = self.panel_rows();
        let nrows = rows.len();
        // Keep the active layer inside the visible window.
        let active_row = rows
            .iter()
            .position(|pr| matches!(pr, PanelRow::Layer(li) if *li == a))
            .unwrap_or(0);
        if active_row < self.layers_scroll {
            self.layers_scroll = active_row;
        }
        if active_row >= self.layers_scroll + 8 {
            self.layers_scroll = active_row - 7;
        }
        for row in 0..(nrows - self.layers_scroll).min(8) {
            let rr = Self::layer_row_rect(r, row, self.layer_row_h());
            let prow = rows[self.layers_scroll + row];
            if let PanelRow::Set(si) = prow {
                // Folder header row (v0.6.1): set color bar,
                // eye, folder glyph, name + mode summary.
                let set = &self.doc.sets[si];
                const SET_COLORS2: [ui::Rgb; 8] = [
                    (160, 160, 160),
                    (220, 60, 60),
                    (230, 140, 40),
                    (230, 200, 40),
                    (70, 180, 80),
                    (60, 110, 220),
                    (150, 90, 200),
                    (140, 140, 140),
                ];
                f.fill_rect(rr, (188, 188, 188));
                let sc = SET_COLORS2[(set.color as usize).min(7)];
                f.fill_rect(Rect::new(rr.x, rr.y, 3, rr.h), sc);
                let c = ui::BLACK;
                let eye = Rect::new(rr.x + 3, rr.y + 7, 14, 14);
                f.outline_rect(eye, c);
                if set.visible {
                    let (ex, ey) = (eye.x + 7, eye.y + 7);
                    for a2 in 0..32 {
                        let t = a2 as f32 / 32.0 * std::f32::consts::TAU;
                        f.set(ex + (t.cos() * 5.0) as i32, ey + (t.sin() * 3.0) as i32, c);
                    }
                    f.fill_rect(Rect::new(ex - 1, ey - 1, 3, 3), c);
                }
                f.fill_rect(Rect::new(rr.x + 21, rr.y + 7, 8, 4), (232, 201, 96));
                f.fill_rect(Rect::new(rr.x + 21, rr.y + 10, 22, 11), (232, 201, 96));
                f.outline_rect(Rect::new(rr.x + 21, rr.y + 10, 22, 11), c);
                let mode_txt = match set.mode {
                    Some(m) => {
                        format!("{} {}%", m.name(), (set.opacity * 100.0).round() as i32)
                    }
                    None => "Pass Thru".to_string(),
                };
                f.text(
                    rr.x + 47,
                    rr.y + 10,
                    &format!("{}  {}", set.name, mode_txt),
                    1,
                    c,
                );
                continue;
            }
            let PanelRow::Layer(li) = prow else { continue };
            let (visible, linked, name, has_mask, mask_enabled, is_adj, is_mask_target, has_fx) = {
                let l = &self.doc.layers[li];
                (
                    l.visible,
                    l.linked,
                    l.name.clone(),
                    l.mask.is_some(),
                    l.mask_enabled,
                    l.adjustment.is_some(),
                    self.paint_target == PaintTarget::Mask(li),
                    l.effects.map(|e| e.any_enabled()).unwrap_or(false),
                )
            };
            let (l_group, l_clipped, l_locks, l_color) = {
                let l = &self.doc.layers[li];
                (l.group, l.clipped, l.locks, l.color)
            };
            // Phase 6 row decorations: set color bar on grouped
            // layers, color-coding chip, clipped indent, lock
            // badge. Set colors reuse the layer-color palette.
            const SET_COLORS: [ui::Rgb; 8] = [
                (160, 160, 160),
                (220, 60, 60),
                (230, 140, 40),
                (230, 200, 40),
                (70, 180, 80),
                (60, 110, 220),
                (150, 90, 200),
                (140, 140, 140),
            ];
            if l_group > 0 {
                let sc = SET_COLORS[((l_group - 1) as usize) % 7 + 1];
                f.fill_rect(Rect::new(rr.x, rr.y, 3, rr.h), sc);
            }
            if l_color > 0 {
                let cc = SET_COLORS[(l_color as usize).min(7)];
                f.fill_rect(Rect::new(rr.x + 21, rr.bottom() - 5, 36, 3), cc);
            }
            let active = li == a;
            if active {
                f.fill_rect(rr, ui::SELROW);
            }
            let c = if active { ui::WHITE } else { ui::TEXT };
            let eye = Rect::new(rr.x + 3, rr.y + 7, 14, 14);
            f.outline_rect(eye, c);
            if visible {
                // An actual eye: almond outline + pupil.
                let (ex, ey) = (eye.x + 7, eye.y + 7);
                for a in 0..32 {
                    let t = a as f32 / 32.0 * std::f32::consts::TAU;
                    f.set(ex + (t.cos() * 5.0) as i32, ey + (t.sin() * 3.0) as i32, c);
                }
                f.fill_rect(Rect::new(ex - 1, ey - 1, 3, 3), c);
            }
            let th = Rect::new(
                rr.x + 21,
                rr.y + 2,
                self.layer_thumb_wh().0,
                self.layer_thumb_wh().1,
            );
            if is_adj {
                // Adjustment layer: the classic half-and-half disc.
                f.fill_rect(th, ui::WHITE);
                let (cx, cy) = (th.x + 18, th.y + 12);
                for yy in -9..=9i32 {
                    for xx in -9..=9i32 {
                        if xx * xx + yy * yy <= 81 {
                            f.set(cx + xx, cy + yy, if xx < 0 { ui::BLACK } else { ui::WHITE });
                        }
                    }
                }
                f.outline_rect(th, c);
            } else {
                self.draw_layer_thumb(f, th, li);
                f.outline_rect(th, c);
            }
            let mut name_x = rr.x + 61;
            if has_mask {
                let mh = Rect::new(
                    rr.x + 23 + self.layer_thumb_wh().0,
                    rr.y + 2,
                    self.layer_thumb_wh().0 * 7 / 9,
                    self.layer_thumb_wh().1,
                );
                self.draw_mask_thumb(f, mh, li);
                if is_mask_target {
                    // The targeted thumbnail gets a double outline.
                    f.outline_rect(Rect::new(mh.x - 1, mh.y - 1, mh.w + 2, mh.h + 2), c);
                }
                f.outline_rect(mh, c);
                if !mask_enabled {
                    // Disabled mask: red X, the classic marker.
                    for t in 0..24i32 {
                        f.set(mh.x + 2 + t, mh.y + 2 + t * 20 / 24, (255, 0, 0));
                        f.set(mh.x + 2 + t, mh.y + 21 - t * 20 / 24, (255, 0, 0));
                    }
                }
                name_x = rr.x + 27 + self.layer_thumb_wh().0 + self.layer_thumb_wh().0 * 7 / 9;
            }
            if l_clipped {
                name_x += 8;
            }
            f.text(name_x, rr.y + 10, &name, 1, c);
            if l_locks != 0 {
                f.text(rr.right() - 44, rr.y + 10, "L", 1, c);
            }
            if has_fx {
                // The classic italic "f" badge: the layer carries
                // live effects.
                f.text(rr.right() - 32, rr.y + 10, "f", 1, c);
            }
            let lb = Rect::new(rr.right() - 18, rr.y + 8, 12, 12);
            if linked {
                // Two interlocking chain links.
                for (ox, oy) in [(-2, -2), (2, 2)] {
                    for a in 0..24 {
                        let t = a as f32 / 24.0 * std::f32::consts::TAU;
                        f.set(
                            lb.x + 6 + ox + (t.cos() * 3.0) as i32,
                            lb.y + 6 + oy + (t.sin() * 3.0) as i32,
                            c,
                        );
                    }
                }
            } else {
                f.outline_rect(lb, c);
            }
        }
        let labels = ["New", "Dup", "Del", "Thumb"];
        let bw = (r.w - 8 - 42) / 4;
        for (i, label) in labels.iter().enumerate() {
            let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
            Self::draw_small_button(f, br, label, true);
        }
        // Scroll buttons (enabled only when there's more stack).
        let up = Rect::new(r.right() - 40, r.bottom() - 19, 18, 16);
        let dn = Rect::new(r.right() - 20, r.bottom() - 19, 18, 16);
        Self::draw_small_button(f, up, "^", self.layers_scroll > 0);
        Self::draw_small_button(f, dn, "v", self.layers_scroll + 8 < nrows);
    }

    /// A layer thumbnail: the layer alone over a checkerboard.
    pub(crate) fn draw_layer_thumb(&self, f: &mut Frame, th: Rect, li: usize) {
        let (dw, dh) = (
            (self.doc.width as i32).max(1),
            (self.doc.height as i32).max(1),
        );
        for by in 0..12i32 {
            for bx in 0..18i32 {
                let dx = ((bx * 2 + 1) * dw / 36).min(dw - 1);
                let dy = ((by * 2 + 1) * dh / 24).min(dh - 1);
                let px = self.doc.render_layer_pixel(li, dx, dy).to_rgba8();
                let checker = if (bx + by) % 2 == 0 { 255 } else { 170 };
                let a = px.3 as u32;
                let blend = |v: u8| ((v as u32 * a + checker * (255 - a)) / 255) as u8;
                f.fill_rect(
                    Rect::new(th.x + bx * 2, th.y + by * 2, 2, 2),
                    (blend(px.0), blend(px.1), blend(px.2)),
                );
            }
        }
    }

    /// A mask thumbnail: the mask plane as a small grayscale image.
    pub(crate) fn draw_mask_thumb(&self, f: &mut Frame, th: Rect, li: usize) {
        let (dw, dh) = (
            (self.doc.width as i32).max(1),
            (self.doc.height as i32).max(1),
        );
        for by in 0..12i32 {
            for bx in 0..14i32 {
                let dx = ((bx * 2 + 1) * dw / 28).min(dw - 1);
                let dy = ((by * 2 + 1) * dh / 24).min(dh - 1);
                let v = self.doc.layers[li].mask_value(dx, dy);
                f.fill_rect(Rect::new(th.x + bx * 2, th.y + by * 2, 2, 2), (v, v, v));
            }
        }
    }

    pub(crate) fn layers_panel_click(&mut self, r: Rect, mx: i32, my: i32) -> bool {
        // Header: blend mode cycles; opacity opens Layer Options.
        if Rect::new(r.x + 2, r.y + 20, r.w - 4, 13).contains(mx, my) {
            let cur = self.doc.layers[self.doc.active_layer].blend_mode;
            let idx = pictodesk_core::blend::ALL_BLEND_MODES
                .iter()
                .position(|m| *m == cur)
                .unwrap_or(0);
            let next = (idx + 1) % pictodesk_core::blend::ALL_BLEND_MODES.len();
            self.do_action(Action::BlendMode(next));
            return true;
        }
        if Rect::new(r.x + 2, r.y + 33, r.w - 4, 13).contains(mx, my) {
            self.do_action(Action::LayerOptions);
            return true;
        }
        let rows = self.panel_rows();
        let nrows = rows.len();
        for row in 0..(nrows - self.layers_scroll).min(8) {
            let rr = Self::layer_row_rect(r, row, self.layer_row_h());
            if !rr.contains(mx, my) {
                continue;
            }
            if let PanelRow::Set(si) = rows[self.layers_scroll + row] {
                let eye = Rect::new(rr.x + 3, rr.y + 7, 14, 14);
                if eye.contains(mx, my) {
                    self.structural("Toggle Set Visibility", |d| {
                        d.sets[si].visible = !d.sets[si].visible;
                    });
                    return true;
                }
                // Clicking the folder selects its top member.
                if let Some(PanelRow::Layer(top)) = rows.get(self.layers_scroll + row + 1).copied()
                {
                    if self.doc.active_layer != top {
                        self.record_step(RecordedStep::LayerSelect(top));
                    }
                    self.doc.active_layer = top;
                    self.channel_view = ChannelView::Composite;
                    self.paint_target = PaintTarget::Image;
                    self.touch();
                }
                return true;
            }
            let PanelRow::Layer(li) = rows[self.layers_scroll + row] else {
                continue;
            };
            let eye = Rect::new(rr.x + 3, rr.y + 7, 14, 14);
            if eye.contains(mx, my) {
                self.structural("Toggle Visibility", |d| {
                    d.layers[li].visible = !d.layers[li].visible;
                });
                return true;
            }
            let lb = Rect::new(rr.right() - 18, rr.y + 8, 12, 12);
            if lb.contains(mx, my) {
                self.structural("Toggle Link", |d| {
                    d.layers[li].linked = !d.layers[li].linked;
                });
                return true;
            }
            // Mask thumbnail: click targets the mask for painting;
            // Alt+click toggles it enabled/disabled.
            if self.doc.layers[li].mask.is_some() {
                let mh = Rect::new(
                    rr.x + 23 + self.layer_thumb_wh().0,
                    rr.y + 2,
                    self.layer_thumb_wh().0 * 7 / 9,
                    self.layer_thumb_wh().1,
                );
                if mh.contains(mx, my) {
                    self.doc.active_layer = li;
                    if self.alt_down {
                        self.structural("Toggle Mask", |d| {
                            d.layers[li].mask_enabled = !d.layers[li].mask_enabled;
                        });
                    } else {
                        self.paint_target = PaintTarget::Mask(li);
                        self.channel_view = ChannelView::Mask(li);
                        self.status = "Painting on the mask (white reveals, black hides)".into();
                    }
                    self.touch();
                    return true;
                }
            }
            // Adjustment thumbnail: click opens the editor.
            if self.doc.layers[li].adjustment.is_some() {
                let th = Rect::new(
                    rr.x + 21,
                    rr.y + 2,
                    self.layer_thumb_wh().0,
                    self.layer_thumb_wh().1,
                );
                if th.contains(mx, my) {
                    self.doc.active_layer = li;
                    self.do_action(Action::EditAdjustment);
                    return true;
                }
            }
            if li == self.doc.active_layer && mx >= rr.x + 64 {
                self.open_rename(0, li);
                return true;
            }
            if self.doc.active_layer != li {
                self.record_step(RecordedStep::LayerSelect(li));
            }
            self.doc.active_layer = li;
            self.channel_view = ChannelView::Composite;
            self.paint_target = PaintTarget::Image;
            self.touch();
            return true;
        }
        if self.layers_scroll > 0
            && Rect::new(r.right() - 40, r.bottom() - 19, 18, 16).contains(mx, my)
        {
            self.layers_scroll -= 1;
            return true;
        }
        if self.layers_scroll + 8 < nrows
            && Rect::new(r.right() - 20, r.bottom() - 19, 18, 16).contains(mx, my)
        {
            self.layers_scroll += 1;
            return true;
        }
        let bw = (r.w - 8 - 42) / 4;
        for i in 0..4 {
            let br = Rect::new(r.x + 4 + i * bw, r.bottom() - 19, bw - 2, 16);
            if br.contains(mx, my) {
                match i {
                    0 => self.do_action(Action::NewLayer),
                    1 => self.do_action(Action::DuplicateLayer),
                    2 => self.do_action(Action::DeleteLayer),
                    _ => {
                        self.layer_thumb = (self.layer_thumb + 1) % 3;
                        self.status = format!(
                            "Layer thumbnails: {}",
                            ["Small", "Medium", "Large"][(self.layer_thumb as usize).min(2)]
                        );
                    }
                }
                return true;
            }
        }
        true // clicks inside the group are consumed
    }

    pub(crate) fn channel_button(&mut self, i: usize) {
        match i {
            0 => self.do_action(Action::SaveSelection),
            1 => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    self.do_action(Action::LoadSelection(a));
                } else {
                    self.status = "Select an alpha channel to load".into();
                }
            }
            2 => {
                let n = self.doc.alphas.len() + 1;
                let (w, h) = (self.doc.width, self.doc.height);
                self.structural("New Channel", |d| {
                    d.alphas.push(AlphaChannel::new(format!("Alpha {n}"), w, h));
                });
                let idx = self.doc.alphas.len() - 1;
                self.channel_view = ChannelView::Alpha(idx);
                self.paint_target = PaintTarget::Alpha(idx);
                self.status = format!("New channel: Alpha {n}");
            }
            3 => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    if a < self.doc.alphas.len() {
                        let name = self.doc.alphas[a].name.clone();
                        self.structural("Delete Channel", |d| {
                            d.alphas.remove(a);
                        });
                        if self.paint_target == PaintTarget::Alpha(a) {
                            self.paint_target = PaintTarget::Image;
                        }
                        self.channel_view = ChannelView::Composite;
                        self.status = format!("Deleted {name}");
                    }
                }
            }
            4 => {
                if let ChannelView::Alpha(a) = self.channel_view {
                    if a < self.doc.alphas.len() {
                        let new_spot = if self.doc.alphas[a].spot_color.is_some() {
                            None
                        } else {
                            let (r, g, b, _) = self.fg.to_rgba8();
                            Some((r, g, b))
                        };
                        self.structural("Toggle Spot Channel", |d| {
                            if let Some(ch) = d.alphas.get_mut(a) {
                                ch.spot_color = new_spot;
                            }
                        });
                        self.status = "Spot channel toggled".into();
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn target_path_mut(&mut self) -> Option<&mut BezPath> {
        match self.active_path? {
            PathRef::Work => self.doc.work_path.as_mut(),
            PathRef::Saved(i) => self.doc.paths.get_mut(i),
        }
    }

    pub(crate) fn ensure_pen_target(&mut self) {
        let ok = match self.active_path {
            Some(PathRef::Work) => self.doc.work_path.is_some(),
            Some(PathRef::Saved(i)) => i < self.doc.paths.len(),
            None => false,
        };
        if !ok {
            if self.doc.work_path.is_none() {
                self.doc.work_path = Some(BezPath::new(""));
            }
            self.active_path = Some(PathRef::Work);
        }
    }

    pub(crate) fn active_path_clone(&self) -> Option<(PathRef, BezPath)> {
        match self.active_path? {
            PathRef::Work => self.doc.work_path.clone().map(|p| (PathRef::Work, p)),
            PathRef::Saved(i) => self
                .doc
                .paths
                .get(i)
                .cloned()
                .map(|p| (PathRef::Saved(i), p)),
        }
    }

    pub(crate) fn path_button(&mut self, i: usize) {
        // Make Work Path works from a selection, no active path needed.
        if i == 3 {
            if let Some(sel) = &self.sel {
                let mut p = pictodesk_core::path::selection_to_path(sel);
                p.name = String::new();
                self.structural("Make Work Path", |d| {
                    d.work_path = Some(p);
                });
                self.active_path = Some(PathRef::Work);
                self.status = "Made work path from selection".into();
            } else {
                self.status = "Make Path needs a selection".into();
            }
            return;
        }
        let Some((pref, path)) = self.active_path_clone() else {
            self.status = "No path selected".into();
            return;
        };
        let layer = self.doc.active_layer;
        match i {
            0 => {
                // Fill path
                let sel = path.to_selection(self.doc.width, self.doc.height);
                match self.paint_target {
                    PaintTarget::Alpha(a) => {
                        let v = {
                            let (r, g, b, _) = self.fg.to_rgba8();
                            ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
                        };
                        self.structural("Fill Path", |d| {
                            if let Some(ch) = d.alphas.get_mut(a) {
                                for idx in 0..ch.data.len() {
                                    let m = sel.raw_mask()[idx];
                                    if m > 0 {
                                        let cur = ch.data[idx] as u32;
                                        ch.data[idx] =
                                            (cur + (v as u32 - cur) * m as u32 / 255) as u8;
                                    }
                                }
                            }
                        });
                    }
                    PaintTarget::Image => {
                        if let Some((x, y, w, h)) = sel.bounds() {
                            let tiles = Selection::tiles_in_rect(x, y, w, h);
                            let fg = self.fg;
                            run_tile_edit(
                                &mut self.doc,
                                &mut self.hist,
                                "Fill Path",
                                layer,
                                &tiles,
                                |d| brush::fill_selection(d, layer, &sel, fg),
                            );
                            self.last_label = Some("Fill Path".into());
                            self.status = "Fill Path".into();
                            self.touch();
                        }
                    }
                    PaintTarget::Mask(li) => {
                        let v = self.fg_luminance();
                        self.structural("Fill Path", |d| {
                            if let Some((bx, by, bw, bh)) = sel.bounds() {
                                for yy in by..by + bh as i32 {
                                    for xx in bx..bx + bw as i32 {
                                        let m = sel.value(xx, yy);
                                        if m > 0 {
                                            let cur = d.layers[li].mask_value(xx, yy) as u32;
                                            let nv =
                                                (cur + (v as u32 - cur) * m as u32 / 255) as u8;
                                            d.layers[li].set_mask_value(xx, yy, nv);
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
            1 => {
                // Stroke path with the current brush
                let spacing = (self.tip.size * 0.25).max(1.0);
                let pts = path.stroke_points(spacing);
                match self.paint_target {
                    PaintTarget::Alpha(a) => {
                        let tip = self.tip.clone();
                        let v = {
                            let (r, g, b, _) = self.fg.to_rgba8();
                            ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
                        };
                        let (w, h) = (self.doc.width, self.doc.height);
                        self.structural("Stroke Path", |d| {
                            if let Some(ch) = d.alphas.get_mut(a) {
                                for pt in &pts {
                                    brush::stamp_channel(
                                        &mut ch.data,
                                        w,
                                        h,
                                        pt.0,
                                        pt.1,
                                        &tip,
                                        v,
                                        1.0,
                                    );
                                }
                            }
                        });
                    }
                    PaintTarget::Mask(li) => {
                        let tip = self.tip.clone();
                        let v = self.fg_luminance();
                        let (w, h) = (self.doc.width, self.doc.height);
                        self.structural("Stroke Path", |d| {
                            for pt in &pts {
                                brush::stamp_mask(
                                    &mut d.layers[li],
                                    w,
                                    h,
                                    pt.0,
                                    pt.1,
                                    &tip,
                                    v,
                                    1.0,
                                );
                            }
                        });
                    }
                    PaintTarget::Image => {
                        if let Some((bx, by, bw, bh)) = path.bounds() {
                            let tiles = Selection::tiles_in_rect(
                                bx as i32 - 32,
                                by as i32 - 32,
                                bw as u32 + 64,
                                bh as u32 + 64,
                            );
                            let tip = self.tip.clone();
                            let fg = self.fg;
                            run_tile_edit(
                                &mut self.doc,
                                &mut self.hist,
                                "Stroke Path",
                                layer,
                                &tiles,
                                |d| {
                                    for pt in &pts {
                                        brush::stamp_brush(
                                            d, layer, None, pt.0, pt.1, fg, &tip, 1.0,
                                        );
                                    }
                                },
                            );
                            self.last_label = Some("Stroke Path".into());
                            self.status = "Stroke Path".into();
                            self.touch();
                        }
                    }
                }
            }
            2 => {
                self.sel = Some(path.to_selection(self.doc.width, self.doc.height));
                self.status = "Path converted to selection".into();
            }
            4 => {
                // Save work path under a name
                if pref == PathRef::Work && self.doc.work_path.is_some() {
                    let mut d = Dialog::new(DialogKind::SavePath, &self.doc);
                    d.text = format!("Path {}", self.doc.paths.len() + 1);
                    d.text_focus = true;
                    self.dialog = Some(d);
                } else {
                    self.status = "Only the work path needs saving".into();
                }
            }
            5 => {
                if let PathRef::Saved(idx) = pref {
                    let new_val = if self.doc.clipping_path == Some(idx) {
                        None
                    } else {
                        Some(idx)
                    };
                    let label = if new_val.is_some() {
                        "Set Clipping Path"
                    } else {
                        "Clear Clipping Path"
                    };
                    self.structural(label, |d| {
                        d.clipping_path = new_val;
                    });
                    self.status = if self.doc.clipping_path == Some(idx) {
                        "Clipping path set (exports with CMYK TIFF)"
                    } else {
                        "Clipping path cleared"
                    }
                    .into();
                } else {
                    self.status = "Save the path before making it a clipping path".into();
                }
            }
            6 => match pref {
                PathRef::Work => {
                    self.structural("Delete Path", |d| {
                        d.work_path = None;
                    });
                    self.active_path = None;
                    self.status = "Work path discarded".into();
                }
                PathRef::Saved(idx) => {
                    if idx < self.doc.paths.len() {
                        self.structural("Delete Path", |d| {
                            d.paths.remove(idx);
                            d.clipping_path = match d.clipping_path {
                                Some(c) if c == idx => None,
                                Some(c) if c > idx => Some(c - 1),
                                other => other,
                            };
                        });
                        self.active_path = None;
                        self.status = "Path deleted".into();
                    }
                }
            },
            _ => {}
        }
    }

    /// Dock mouse routing. Returns true when the click landed in the
    /// dock and was consumed.
    pub(crate) fn panel_mouse(&mut self, mx: i32, my: i32) -> bool {
        if !self.dock_open() || mx < self.win_size.0 as i32 - DOCK_W {
            return false;
        }
        // Color palette cells
        if self.show_palette {
            // Footer: Save/Load swatch library (.ACO).
            if let Some(r) = self.group_rect(PanelKind::Color) {
                let save = Rect::new(r.x + 5, r.bottom() - 16, 100, 13);
                let load = Rect::new(r.x + 110, r.bottom() - 16, 100, 13);
                if save.contains(mx, my) {
                    self.open_save_dialog(2, "palette.aco");
                    return true;
                }
                if load.contains(mx, my) {
                    self.file_panel_purpose = 3;
                    self.open_file_panel();
                    return true;
                }
            }
            for i in 0..self.palette.len() {
                if self.palette_cell_rect(i).contains(mx, my) {
                    let c = self.palette[i];
                    let p = Pixel16::from_rgba8(c.0, c.1, c.2, 255);
                    if self.alt_down {
                        self.bg = p;
                    } else {
                        self.fg = p;
                    }
                    return true;
                }
            }
        }
        // Navigator thumbnail: click/drag pans the main view.
        if let Some(r) = self.group_rect(PanelKind::Navigator) {
            if r.contains(mx, my) {
                let th = Self::nav_thumb_rect(r);
                if th.contains(mx, my) {
                    self.navigator_to(mx, my);
                    self.drag = DragMode::NavDrag;
                }
                return true;
            }
        }
        // Actions panel
        if let Some(r) = self.group_rect(PanelKind::Actions) {
            if r.contains(mx, my) {
                return self.actions_panel_click(r, mx, my);
            }
        }
        // History panel
        if let Some(r) = self.group_rect(PanelKind::History) {
            if r.contains(mx, my) {
                return self.history_panel_click(r, mx, my);
            }
        }
        // Toning-tool option row (Brushes panel, tone tools only)
        if let Some((value_btn, minus, plus)) = self.tone_row_rects() {
            if value_btn.contains(mx, my) {
                if self.tool == Tool::Sponge {
                    self.sponge_saturate = !self.sponge_saturate;
                    self.status = format!(
                        "Sponge mode: {}",
                        if self.sponge_saturate {
                            "Saturate"
                        } else {
                            "Desaturate"
                        }
                    );
                } else {
                    self.tone_range = match self.tone_range {
                        brush::ToneRange::Shadows => brush::ToneRange::Midtones,
                        brush::ToneRange::Midtones => brush::ToneRange::Highlights,
                        brush::ToneRange::Highlights => brush::ToneRange::Shadows,
                    };
                    self.status = format!(
                        "Range: {}",
                        match self.tone_range {
                            brush::ToneRange::Shadows => "Shadows",
                            brush::ToneRange::Midtones => "Midtones",
                            brush::ToneRange::Highlights => "Highlights",
                        }
                    );
                }
                return true;
            }
            if minus.contains(mx, my) {
                self.tone_exposure = (self.tone_exposure - 0.05).max(0.05);
                return true;
            }
            if plus.contains(mx, my) {
                self.tone_exposure = (self.tone_exposure + 0.05).min(1.0);
                return true;
            }
        }
        // Phase 5 tool option row (Brushes panel)
        if let Some((value_btn, minus, plus)) = self.tool_row_rects() {
            if value_btn.contains(mx, my) {
                match self.tool {
                    Tool::Stamp => {
                        self.stamp_sample_merged = !self.stamp_sample_merged;
                        self.status = if self.stamp_sample_merged {
                            "Clone Stamp samples the merged composite".into()
                        } else {
                            "Clone Stamp samples the active layer".into()
                        };
                    }
                    Tool::Eyedropper => {
                        self.dropper_sample_merged = !self.dropper_sample_merged;
                        self.status = if self.dropper_sample_merged {
                            "Eyedropper samples the merged composite".into()
                        } else {
                            "Eyedropper samples the active layer".into()
                        };
                    }
                    Tool::Eraser => {
                        self.eraser_to_history = !self.eraser_to_history;
                        self.status = if self.eraser_to_history {
                            "Eraser: erase to the history source".into()
                        } else {
                            "Eraser: erase to transparency".into()
                        };
                    }
                    Tool::Gradient => {
                        use brush::GradientShape as GS;
                        self.gradient_shape = match self.gradient_shape {
                            GS::Linear => GS::Radial,
                            GS::Radial => GS::Angle,
                            GS::Angle => GS::Diamond,
                            GS::Diamond => GS::Reflected,
                            GS::Reflected => GS::Linear,
                        };
                        self.status = "Gradient shape changed".into();
                    }
                    Tool::ArtHistoryBrush => {
                        self.art_hist_style = (self.art_hist_style + 1) % 3;
                        self.status = format!(
                            "Art History style: {}",
                            ["Tight", "Loose", "Dab"][self.art_hist_style as usize]
                        );
                    }
                    Tool::ShapeCustom => {
                        let total = 3 + self.doc.custom_shapes.len();
                        self.custom_idx = (self.custom_idx + 1) % total.max(1);
                        let (name, _) = self.custom_shape_choice(self.custom_idx);
                        self.status = format!("Custom shape: {name}");
                    }
                    Tool::ShapePolygon => {
                        self.shape_star = !self.shape_star;
                        self.status = if self.shape_star {
                            "Polygon: star".into()
                        } else {
                            "Polygon".into()
                        };
                    }
                    _ => {}
                }
                return true;
            }
            if minus.contains(mx, my) {
                match self.tool {
                    Tool::ShapePolygon => {
                        self.shape_sides = self.shape_sides.saturating_sub(1).max(3);
                    }
                    Tool::ShapeRect | Tool::ShapeRounded | Tool::ShapeEllipse | Tool::ShapeLine => {
                        self.shape_stroke = self.shape_stroke.saturating_sub(1);
                    }
                    _ => {
                        self.eraser_tolerance = self.eraser_tolerance.saturating_sub(8);
                    }
                }
                return true;
            }
            if plus.contains(mx, my) {
                match self.tool {
                    Tool::ShapePolygon => {
                        self.shape_sides = (self.shape_sides + 1).min(32);
                    }
                    Tool::ShapeRect | Tool::ShapeRounded | Tool::ShapeEllipse | Tool::ShapeLine => {
                        self.shape_stroke = (self.shape_stroke + 1).min(50);
                    }
                    _ => {
                        self.eraser_tolerance = self.eraser_tolerance.saturating_add(8);
                    }
                }
                return true;
            }
        }
        // Brushes cells
        if self.show_brushes {
            if let Some(r) = self.group_rect(PanelKind::Brushes) {
                let save = Rect::new(r.x + 5, r.bottom() - 16, 100, 13);
                let load = Rect::new(r.x + 110, r.bottom() - 16, 100, 13);
                if save.contains(mx, my) {
                    self.open_save_dialog(3, "brushes.abr");
                    return true;
                }
                if load.contains(mx, my) {
                    self.file_panel_purpose = 4;
                    self.open_file_panel();
                    return true;
                }
            }
            if let Some(r) = self.group_rect(PanelKind::Brushes) {
                for i in 0..8u8 {
                    if self.dyn_section_rect(r, i).contains(mx, my) {
                        self.brush_section = i;
                        return true;
                    }
                }
                for (j, (id, _label, _value, kind)) in self.dyn_row_defs().iter().enumerate() {
                    let (vb, minus, plus) = self.dyn_row_rects(r, j);
                    match kind {
                        0 => {
                            if minus.contains(mx, my) {
                                self.dyn_adjust(*id, -1);
                                return true;
                            }
                            if plus.contains(mx, my) {
                                self.dyn_adjust(*id, 1);
                                return true;
                            }
                        }
                        1 => {
                            if vb.contains(mx, my) {
                                self.dyn_toggle(*id);
                                return true;
                            }
                        }
                        _ => {
                            if vb.contains(mx, my) {
                                self.dyn_cycle(*id);
                                return true;
                            }
                        }
                    }
                }
            }
            for (idx, p) in self.brush_presets.iter().enumerate() {
                let (row, i) = (idx / 8, idx % 8);
                if self.brushes_cell(i, row).contains(mx, my) {
                    self.tip = BrushTip::new(p.size, p.hardness);
                    self.brush_dyn = p.dyn_.clone();
                    return true;
                }
            }
        }
        if let Some(r) = self.group_rect(PanelKind::ToolPresets) {
            if r.contains(mx, my) {
                for (i, _) in self.tool_presets.iter().take(10).enumerate() {
                    let rr = Rect::new(r.x + 4, r.y + 20 + i as i32 * 15, r.w - 8, 14);
                    if rr.contains(mx, my) {
                        self.apply_tool_preset(i);
                        return true;
                    }
                }
                if Rect::new(r.x + 5, r.bottom() - 16, 100, 13).contains(mx, my) {
                    let name = format!(
                        "{} Preset {}",
                        self.tool.name(),
                        self.tool_presets.len() + 1
                    );
                    let p = self.capture_tool_preset(name);
                    self.tool_presets.push(p);
                    save_tool_presets(&self.tool_presets);
                    self.status = "Tool preset captured".into();
                    return true;
                }
                return true;
            }
        }
        // Channels / Paths group
        if let Some(r) = self.group_rect(PanelKind::Cp) {
            if r.contains(mx, my) {
                for i in 0..3 {
                    let tr = Self::cp_tab_rect(r, i);
                    if tr.contains(mx, my) {
                        self.cp_tab = i;
                        return true;
                    }
                }
                if self.cp_tab == 0 {
                    return self.layers_panel_click(r, mx, my);
                }
                let row = (my - (r.y + 20)) / 15;
                if self.cp_tab == 1 {
                    let rows = self.channel_rows();
                    if row >= 0 && (row as usize) < rows.len() {
                        let view = rows[row as usize].1;
                        if let ChannelView::Alpha(a) = view {
                            if self.channel_view == view && mx >= r.x + 64 {
                                self.open_rename(1, a);
                                return true;
                            }
                        }
                        self.channel_view = view;
                        self.doc.multi_active = match view {
                            ChannelView::Channel(i) if self.doc.mode == ColorMode::Multichannel => {
                                Some(i)
                            }
                            _ => None,
                        };
                        match view {
                            ChannelView::Alpha(a) => self.paint_target = PaintTarget::Alpha(a),
                            _ => self.paint_target = PaintTarget::Image,
                        }
                        return true;
                    }
                    let bw = (r.w - 8) / 5;
                    for i in 0..5 {
                        let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 19, bw - 2, 16);
                        if br.contains(mx, my) {
                            self.channel_button(i);
                            return true;
                        }
                    }
                } else {
                    let rows = self.path_rows();
                    if row >= 0 && (row as usize) < rows.len() {
                        self.active_path = Some(rows[row as usize].1);
                        return true;
                    }
                    let bw = (r.w - 8) / 4;
                    for i in 0..4 {
                        let br = Rect::new(r.x + 4 + i as i32 * bw, r.bottom() - 39, bw - 2, 16);
                        if br.contains(mx, my) {
                            self.path_button(i);
                            return true;
                        }
                    }
                    let bw2 = (r.w - 8) / 3;
                    for i in 0..3 {
                        let br = Rect::new(r.x + 4 + i as i32 * bw2, r.bottom() - 19, bw2 - 2, 16);
                        if br.contains(mx, my) {
                            self.path_button(i + 4);
                            return true;
                        }
                    }
                }
                return true; // clicks inside the group are consumed
            }
        }
        false
    }

    pub(crate) fn swatches_area(&self) -> Rect {
        Rect::new(
            4,
            self.win_size.1 as i32 - ui::STATUS_H - 56,
            ui::TOOLBAR_W - 6,
            54,
        )
    }

    pub(crate) fn swatt_default_hit(&self, mx: i32, my: i32) -> bool {
        self.swatch_default_rect().contains(mx, my)
    }

    pub(crate) fn fg_luminance(&self) -> u8 {
        let (r, g, b, _) = self.fg.to_rgba8();
        ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) / 256) as u8
    }
}
