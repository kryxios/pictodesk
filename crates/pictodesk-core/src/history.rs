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
        });
    }

    /// Capture the "after" state (call AFTER painting). Pairs with the
    /// matching capture_before entry.
    pub fn capture_after(&mut self, doc: &Document, layer: usize, coord: TileCoord) {
        let after = doc.layers[layer].snapshot_tile(coord);
        if let Some(c) = self
            .changes
            .iter_mut()
            .find(|c| c.layer == layer && c.coord == coord)
        {
            c.after = after;
        } else {
            self.changes.push(TileChange {
                layer,
                coord,
                before: None,
                after,
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
        }
    }

    fn revert(&mut self, doc: &mut Document) {
        for c in self.changes.iter().rev() {
            doc.layers[c.layer].restore_tile(c.coord, c.before.clone());
        }
    }
}

/// The undo/redo stacks.
#[derive(Default)]
pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
}

impl History {
    pub fn new() -> Self {
        History::default()
    }

    /// Record a command that has ALREADY been applied to the document
    /// (the normal painting flow: snapshot, paint, capture, push).
    pub fn push(&mut self, cmd: Box<dyn Command>) {
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        let mut cmd = self.undo_stack.pop()?;
        let label = cmd.label().to_string();
        cmd.revert(doc);
        self.redo_stack.push(cmd);
        Some(label)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        let mut cmd = self.redo_stack.pop()?;
        let label = cmd.label().to_string();
        cmd.apply(doc);
        self.undo_stack.push(cmd);
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
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
