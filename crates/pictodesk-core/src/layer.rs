//! A layer: a sparse grid of tiles plus the classic properties the third generation
//! made famous — opacity, visibility, blend mode. Layers exist in the
//! Phase 0 document model even though the UI won't expose them until
//! the Phase 3 milestone; the engine never has to be rewritten for them.

use std::collections::HashMap;

use crate::blend::BlendMode;
use crate::pixel::Pixel16;
use crate::tile::{split_coord, Tile, TileCoord};

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub tiles: HashMap<TileCoord, Tile>,
    /// 0.0..=1.0
    pub opacity: f32,
    pub visible: bool,
    pub blend_mode: BlendMode,
    /// Layer content offset in document pixels (Phase 0: always 0,0;
    /// the field exists so Phase 3 transforms don't change the format).
    pub offset: (i32, i32),
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Layer {
            name: name.into(),
            tiles: HashMap::new(),
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            offset: (0, 0),
        }
    }

    /// Pixel at document-space (x, y); transparent if never painted.
    pub fn get_pixel(&self, x: i32, y: i32) -> Pixel16 {
        let (coord, ox, oy) = split_coord(x - self.offset.0, y - self.offset.1);
        match self.tiles.get(&coord) {
            Some(t) => t.get(ox, oy),
            None => Pixel16::TRANSPARENT,
        }
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, p: Pixel16) {
        let (coord, ox, oy) = split_coord(x - self.offset.0, y - self.offset.1);
        let tile = self
            .tiles
            .entry(coord)
            .or_insert_with(Tile::new_transparent);
        tile.set(ox, oy, p);
    }

    /// Fill a document-space rectangle (used by tests, fills, and the CLI demo).
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, p: Pixel16) {
        for yy in y..y + h as i32 {
            for xx in x..x + w as i32 {
                self.set_pixel(xx, yy, p);
            }
        }
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Snapshot one tile (or None if unallocated) — the unit of undo.
    pub fn snapshot_tile(&self, coord: TileCoord) -> Option<Tile> {
        self.tiles.get(&coord).cloned()
    }

    /// Restore a tile snapshot; None removes the tile entirely.
    pub fn restore_tile(&mut self, coord: TileCoord, tile: Option<Tile>) {
        match tile {
            Some(t) => {
                self.tiles.insert(coord, t);
            }
            None => {
                self.tiles.remove(&coord);
            }
        }
    }

    pub fn dirty_tile_count(&self) -> usize {
        self.tiles.values().filter(|t| t.dirty).count()
    }

    pub fn clear_dirty(&mut self) {
        for t in self.tiles.values_mut() {
            t.dirty = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_pixels_round_trip_across_tiles() {
        let mut l = Layer::new("Base");
        let p = Pixel16::from_rgba8(200, 10, 10, 255);
        l.set_pixel(0, 0, p);
        l.set_pixel(300, 300, p); // second tile
        l.set_pixel(-5, -5, p); // negative tile
        assert_eq!(l.get_pixel(0, 0), p);
        assert_eq!(l.get_pixel(300, 300), p);
        assert_eq!(l.get_pixel(-5, -5), p);
        assert_eq!(l.get_pixel(100, 100), Pixel16::TRANSPARENT);
        assert_eq!(l.tile_count(), 3);
    }

    #[test]
    fn snapshot_restore_undoes_a_paint() {
        let mut l = Layer::new("Base");
        let before = l.snapshot_tile((0, 0));
        l.fill_rect(0, 0, 10, 10, Pixel16::from_rgba8(1, 2, 3, 255));
        assert_ne!(l.get_pixel(5, 5), Pixel16::TRANSPARENT);
        l.restore_tile((0, 0), before);
        assert_eq!(l.get_pixel(5, 5), Pixel16::TRANSPARENT);
        assert_eq!(l.tile_count(), 0);
    }
}
