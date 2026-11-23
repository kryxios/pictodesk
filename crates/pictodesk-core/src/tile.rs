//! Tiles: the document is stored and composited in 256x256 tiles so a
//! 100-megapixel image never has to live in one flat buffer, and edits
//! only invalidate the tiles they touch.

use crate::pixel::Pixel16;

pub const TILE_SIZE: u32 = 256;
pub const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;

/// Tile coordinate in tile units (can be negative once layer offsets exist).
pub type TileCoord = (i32, i32);

/// One 256x256 block of pixels. Allocated lazily by layers: a tile that has
/// never been painted does not exist, and reads as transparent.
#[derive(Clone, Debug)]
pub struct Tile {
    pub pixels: Vec<Pixel16>,
    /// Set when contents change since last render/save; the renderer and
    /// autosave only touch dirty tiles. Transient bookkeeping, NOT content:
    /// Tile equality deliberately ignores it (a freshly loaded tile is
    /// clean, a freshly painted one is dirty — same picture).
    pub dirty: bool,
}

impl PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        self.pixels == other.pixels
    }
}

impl Tile {
    pub fn new_transparent() -> Self {
        Tile {
            pixels: vec![Pixel16::TRANSPARENT; TILE_PIXELS],
            dirty: true,
        }
    }

    pub fn filled(p: Pixel16) -> Self {
        Tile {
            pixels: vec![p; TILE_PIXELS],
            dirty: true,
        }
    }

    #[inline]
    fn idx(x: u32, y: u32) -> usize {
        (y * TILE_SIZE + x) as usize
    }

    /// x,y are local to the tile: 0..TILE_SIZE.
    pub fn get(&self, x: u32, y: u32) -> Pixel16 {
        self.pixels[Self::idx(x, y)]
    }

    pub fn set(&mut self, x: u32, y: u32, p: Pixel16) {
        self.pixels[Self::idx(x, y)] = p;
        self.dirty = true;
    }

    pub fn is_fully_transparent(&self) -> bool {
        self.pixels.iter().all(|p| p.a == 0)
    }
}

/// Split a document-space pixel coordinate into (tile coord, in-tile offset).
/// Handles negative coordinates correctly (floor division).
pub fn split_coord(x: i32, y: i32) -> (TileCoord, u32, u32) {
    let t = TILE_SIZE as i32;
    let tx = x.div_euclid(t);
    let ty = y.div_euclid(t);
    let ox = x.rem_euclid(t) as u32;
    let oy = y.rem_euclid(t) as u32;
    ((tx, ty), ox, oy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_coord_positive_and_negative() {
        assert_eq!(split_coord(0, 0), ((0, 0), 0, 0));
        assert_eq!(split_coord(255, 256), ((0, 1), 255, 0));
        assert_eq!(split_coord(256, 0), ((1, 0), 0, 0));
        assert_eq!(split_coord(-1, -1), ((-1, -1), 255, 255));
        assert_eq!(split_coord(-256, 10), ((-1, 0), 0, 10));
    }

    #[test]
    fn tile_set_get_and_dirty() {
        let mut t = Tile::new_transparent();
        assert!(t.is_fully_transparent());
        let p = Pixel16::from_rgba8(10, 20, 30, 255);
        t.set(5, 7, p);
        assert_eq!(t.get(5, 7), p);
        assert!(t.dirty);
        assert!(!t.is_fully_transparent());
    }
}
