//! History: a command stack designed in Phase 0 to become the History
//! panel in Phase 5. v0.1 will expose single undo; the stack underneath
//! already supports unlimited undo/redo.
//!
//! Commands are tile-snapshot based: painting a stroke snapshots the
//! tiles it will touch (before) and captures them again after. Memory
//! cost is bounded by tiles touched, not by canvas size.

use crate::document::Document;
use crate::tile::{Tile, TileCoord};

pub trait Command {
    fn label(&self) -> &str;
    fn apply(&mut self, doc: &mut Document);
    fn revert(&mut self, doc: &mut Document);
}

/// One layer's tile before/after pair.
#[derive(Clone, Debug)]
pub struct TileChange {
    pub layer: usize,
    pub coord: TileCoord,
    pub before: Option<Tile>,
    pub after: Option<Tile>,
    /// CMYK-mode alpha plane tile at the same coord (None in RGB docs
    /// and where no alpha tile exists).
    pub before_alpha: Option<Vec<u16>>,
    pub after_alpha: Option<Vec<u16>>,
}

/// A command made of tile changes — the workhorse for brush strokes,
/// fills, pastes, and any other pixel edit.
#[derive(Clone, Debug)]
pub struct TileEditCommand {
    label: String,
    pub changes: Vec<TileChange>,
}

impl TileEditCommand {
    pub fn new(label: impl Into<String>) -> Self {
        TileEditCommand {
            label: label.into(),
            changes: Vec::new(),
        }
    }

    /// Snapshot a tile's current state as a "before" (call BEFORE painting).
    pub fn capture_before(&mut self, doc: &Document, layer: usize, coord: TileCoord) {
        if self
            .changes
            .iter()
            .any(|c| c.layer == layer && c.coord == coord)
        {
            return;
        }
        self.changes.push(TileChange {
            layer,
            coord,
            before: doc.layers[layer].snapshot_tile(coord),
            after: None,
            before_alpha: doc.layers[layer].snapshot_alpha_tile(coord),
            after_alpha: None,
        });
    }

    /// Capture the "after" state (call AFTER painting). Pairs with the
    /// matching capture_before entry.
    pub fn capture_after(&mut self, doc: &Document, layer: usize, coord: TileCoord) {
        let after = doc.layers[layer].snapshot_tile(coord);
        let after_alpha = doc.layers[layer].snapshot_alpha_tile(coord);
        if let Some(c) = self
            .changes
            .iter_mut()
            .find(|c| c.layer == layer && c.coord == coord)
        {
            c.after = after;
            c.after_alpha = after_alpha;
        } else {
            self.changes.push(TileChange {
                layer,
                coord,
                before: None,
                after,
                before_alpha: None,
                after_alpha,
            });
        }
    }
}

impl Command for TileEditCommand {
    fn label(&self) -> &str {
        &self.label
    }

    fn apply(&mut self, doc: &mut Document) {
        for c in &self.changes {
            doc.layers[c.layer].restore_tile(c.coord, c.after.clone());
            doc.layers[c.layer].restore_alpha_tile(c.coord, c.after_alpha.clone());
        }
    }

    fn revert(&mut self, doc: &mut Document) {
        for c in self.changes.iter().rev() {
            doc.layers[c.layer].restore_tile(c.coord, c.before.clone());
            doc.layers[c.layer].restore_alpha_tile(c.coord, c.before_alpha.clone());
        }
    }
}

/// Run an edit as one tile-snapshot command: capture `before` for the
/// given tiles on `layer`, apply `edit`, capture `after`, push.
/// This is the standard wrapper for every pixel edit (paint, adjust,
/// filter) so undo granularity is always exactly the tiles touched.
pub fn run_tile_edit(
    doc: &mut Document,
    hist: &mut History,
    label: impl Into<String>,
    layer: usize,
    tiles: &[TileCoord],
    edit: impl FnOnce(&mut Document),
) {
    let mut cmd = TileEditCommand::new(label);
    for &c in tiles {
        cmd.capture_before(doc, layer, c);
    }
    edit(doc);
    for &c in tiles {
        cmd.capture_after(doc, layer, c);
    }
    hist.push(Box::new(cmd));
}

/// A whole-document before/after command — for structural edits
/// (resize, rotate, crop, canvas size) where tile deltas don't fit.
/// Memory-heavy on huge documents; correct first, optimized later.
#[derive(Clone, Debug)]
pub struct DocumentCommand {
    label: String,
    before: Document,
    after: Document,
}

impl DocumentCommand {
    /// Snapshot `doc` as the before state; call `finish` after editing.
    pub fn begin(label: impl Into<String>, doc: &Document) -> Self {
        DocumentCommand {
            label: label.into(),
            before: doc.clone(),
            after: doc.clone(),
        }
    }

    pub fn finish(&mut self, doc: &Document) {
        self.after = doc.clone();
    }
}

impl Command for DocumentCommand {
    fn label(&self) -> &str {
        &self.label
    }
    fn apply(&mut self, doc: &mut Document) {
        *doc = self.after.clone();
    }
    fn revert(&mut self, doc: &mut Document) {
        *doc = self.before.clone();
    }
}

/// A layer-offset change (the Move tool): no pixels change, only the
/// layer's position in the document.
/// One layer's offset change: (layer index, before, after).
pub type LayerMove = (usize, (i32, i32), (i32, i32));

#[derive(Clone, Debug)]
pub struct MoveLayerCommand {
    label: String,
    moves: Vec<LayerMove>,
}

impl MoveLayerCommand {
    pub fn new(
        label: impl Into<String>,
        layer: usize,
        before: (i32, i32),
        after: (i32, i32),
    ) -> Self {
        MoveLayerCommand {
            label: label.into(),
            moves: vec![(layer, before, after)],
        }
    }

    /// One command moving several layers together (linked layers).
    pub fn new_multi(label: impl Into<String>, moves: Vec<LayerMove>) -> Self {
        MoveLayerCommand {
            label: label.into(),
            moves,
        }
    }
}

impl Command for MoveLayerCommand {
    fn label(&self) -> &str {
        &self.label
    }
    fn apply(&mut self, doc: &mut Document) {
        for (layer, _, after) in &self.moves {
            doc.layers[*layer].offset = *after;
        }
    }
    fn revert(&mut self, doc: &mut Document) {
        for (layer, before, _) in &self.moves {
            doc.layers[*layer].offset = *before;
        }
    }
}

/// What a snapshot froze (Phase 5): the whole document, the
/// document flattened to one layer, or a single layer's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Full,
    Merged,
    Layer(usize),
}

/// A named, frozen copy of a document state (Phase 5 snapshots).
/// Snapshots are session-only; they are never written to disk.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub name: String,
    pub doc: Document,
    pub kind: SnapKind,
}

/// The undo/redo state list behind the History panel (Phase 5).
/// States are commands; `pos` counts how many are currently
/// applied, so state 0 is the document as opened and state `pos`
/// is what you see. In linear mode (the classic default) a new
/// edit made from a middle state discards the states above it;
/// with non-linear history on, the new state is inserted and the
/// later states are kept — commands store absolute tile states,
/// so jumping between divergent states still lands exactly.
#[derive(Default)]
pub struct History {
    states: Vec<Box<dyn Command>>,
    pos: usize,
    /// Cap on the number of remembered states. The 1990 original
    /// exposed exactly ONE level of undo, so the Phase 1 app set
    /// this to 1; Phase 5 raises it (the classic default is 20).
    limit: Option<usize>,
    non_linear: bool,
}

impl History {
    pub fn new() -> Self {
        History::default()
    }

    /// Set a cap on remembered states (None = unlimited). Oldest
    /// states fall off the bottom when the cap is exceeded.
    pub fn set_limit(&mut self, limit: Option<usize>) {
        self.limit = limit;
        self.trim();
    }

    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    pub fn set_non_linear(&mut self, on: bool) {
        self.non_linear = on;
    }

    pub fn non_linear(&self) -> bool {
        self.non_linear
    }

    fn trim(&mut self) {
        if let Some(limit) = self.limit {
            while self.states.len() > limit {
                self.states.remove(0);
                self.pos = self.pos.saturating_sub(1);
            }
        }
    }

    /// Record a command that has ALREADY been applied to the document
    /// (the normal painting flow: snapshot, paint, capture, push).
    pub fn push(&mut self, cmd: Box<dyn Command>) {
        if self.pos < self.states.len() {
            if self.non_linear {
                self.states.insert(self.pos, cmd);
            } else {
                self.states.truncate(self.pos);
                self.states.push(cmd);
            }
        } else {
            self.states.push(cmd);
        }
        self.pos += 1;
        self.trim();
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        if self.pos == 0 {
            return None;
        }
        let label = self.states[self.pos - 1].label().to_string();
        self.states[self.pos - 1].revert(doc);
        self.pos -= 1;
        Some(label)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        if self.pos >= self.states.len() {
            return None;
        }
        let label = self.states[self.pos].label().to_string();
        self.states[self.pos].apply(doc);
        self.pos += 1;
        Some(label)
    }

    /// Jump straight to a state (0 = as opened). Returns false if
    /// the target is out of range.
    pub fn goto_state(&mut self, doc: &mut Document, target: usize) -> bool {
        if target > self.states.len() {
            return false;
        }
        while self.pos > target {
            if self.undo(doc).is_none() {
                break;
            }
        }
        while self.pos < target {
            if self.redo(doc).is_none() {
                break;
            }
        }
        true
    }

    /// Reconstruct the document as it was at `target` WITHOUT
    /// moving the current position (used to paint from a state
    /// with the History Brush and to snapshot a state).
    pub fn doc_at_state(&mut self, doc: &Document, target: usize) -> Option<Document> {
        if target > self.states.len() {
            return None;
        }
        let mut d = doc.clone();
        if target < self.pos {
            for i in (target..self.pos).rev() {
                self.states[i].revert(&mut d);
            }
        } else {
            for i in self.pos..target {
                self.states[i].apply(&mut d);
            }
        }
        Some(d)
    }

    /// Drop every state (the document itself is untouched). This
    /// is Edit > Purge > Histories / the panel's Clear History.
    pub fn purge(&mut self) {
        self.states.clear();
        self.pos = 0;
    }

    pub fn can_undo(&self) -> bool {
        self.pos > 0
    }

    pub fn can_redo(&self) -> bool {
        self.pos < self.states.len()
    }

    pub fn undo_depth(&self) -> usize {
        self.pos
    }

    /// Number of remembered states (the panel shows these plus
    /// the initial "as opened" row).
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Current position: how many states are applied.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// State labels in order (state 1 first).
    pub fn labels(&self) -> Vec<String> {
        self.states.iter().map(|s| s.label().to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Pixel16;

    fn paint_red_square(doc: &mut Document, hist: &mut History) {
        let mut cmd = TileEditCommand::new("Paint red");
        cmd.capture_before(doc, 0, (0, 0));
        doc.layers[0].fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(255, 0, 0, 255));
        cmd.capture_after(doc, 0, (0, 0));
        hist.push(Box::new(cmd));
    }

    #[test]
    fn undo_redo_round_trip_restores_pixels() {
        let mut doc = Document::new(32, 32);
        let mut hist = History::new();
        let blank = doc.render_checksum();
        paint_red_square(&mut doc, &mut hist);
        let painted = doc.render_checksum();
        assert_ne!(blank, painted);
        assert_eq!(hist.undo(&mut doc).as_deref(), Some("Paint red"));
        assert_eq!(doc.render_checksum(), blank);
        assert_eq!(hist.redo(&mut doc).as_deref(), Some("Paint red"));
        assert_eq!(doc.render_checksum(), painted);
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut doc = Document::new(32, 32);
        let mut hist = History::new();
        paint_red_square(&mut doc, &mut hist);
        hist.undo(&mut doc);
        assert!(hist.can_redo());
        paint_red_square(&mut doc, &mut hist);
        assert!(!hist.can_redo());
    }

    #[test]
    fn undo_on_empty_history_is_none() {
        let mut doc = Document::new(8, 8);
        let mut hist = History::new();
        assert_eq!(hist.undo(&mut doc), None);
    }
}
