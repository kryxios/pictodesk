//! A layer: a sparse grid of tiles plus the classic properties the third generation
//! made famous — opacity, visibility, blend mode. Layers exist in the
//! Phase 0 document model even though the UI won't expose them until
//! the Phase 3 milestone; the engine never has to be rewritten for them.

use std::collections::HashMap;

use crate::blend::BlendMode;
use crate::pixel::Pixel16;
use crate::tile::{split_coord, Tile, TileCoord, TILE_PIXELS};

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
    /// Per-pixel alpha for CMYK-mode layers, one sparse tile grid of
    /// u16 alpha values (absent tile = fully opaque). CMYK pixels
    /// need five values (C, M, Y, K + alpha); the four ink channels
    /// live in `tiles` (in Pixel16's r/g/b/a slots) and alpha lives
    /// here. Empty in RGB/Grayscale documents.
    pub cmyk_alpha: HashMap<TileCoord, Vec<u16>>,
    /// Linked layers move together with the Move tool.
    pub linked: bool,
    /// Layer mask (Phase 4): a sparse grid of u8 tiles in document
    /// space (offset-adjusted, like `cmyk_alpha`). An absent tile
    /// means 255 everywhere — fully revealed. `None` = no mask.
    pub mask: Option<HashMap<TileCoord, Vec<u8>>>,
    /// A disabled mask is kept but ignored by the compositor.
    pub mask_enabled: bool,
    /// Value of mask areas with no tile yet (255 for a reveal-all
    /// mask, 0 for hide-all). Meaningful only when `mask` is set.
    pub mask_fill: u8,
    /// Adjustment-layer payload (Phase 4). When present, the layer
    /// holds no pixels of its own: the compositor runs the
    /// adjustment over everything below it instead.
    pub adjustment: Option<crate::adjust::Adjustment>,
    /// Layer effects payload (Phase 5): shadows, glows, bevel —
    /// rendered by the compositor from the layer's alpha.
    pub effects: Option<crate::effects::Effects>,
    /// Type-layer payload (Phase 5): when present, the layer's
    /// pixels are a rendering of this text spec, and the spec is
    /// what gets edited. Rendering lives in the app.
    pub text: Option<crate::text::TextSpec>,
    // ---- Phase 6 fields.
    /// Fill opacity (Blending Options): scales the layer's own
    /// pixels but NOT its effects. 0.0..=1.0.
    pub fill_opacity: f32,
    /// Blend If ranges (Blending Options): [this_lo, this_hi,
    /// under_lo, under_hi] as 0..=255 luminance bounds — these
    /// are the INNER thresholds; `blend_if_split` feathers
    /// outward from them.
    pub blend_if: [u8; 4],
    /// Blend If split-slider feather widths (v0.6.1):
    /// [this_lo, this_hi, under_lo, under_hi] — how far beyond
    /// each inner threshold the range feathers to fully
    /// excluded. All zero = hard edges (the pre-0.6.1 behavior).
    pub blend_if_split: [u8; 4],
    /// Knockout: 0 None, 1 Shallow, 2 Deep. Shallow punches
    /// down to the layer's set base (or the bottom layer when
    /// the layer is in no set); Deep punches to transparency.
    pub knockout: u8,
    /// Per-channel blending switches (bit0 R/C, bit1 G/M,
    /// bit2 B/Y, bit3 K). A cleared channel keeps the backdrop's
    /// value for that channel.
    pub blend_channels: u8,
    /// Advanced blending toggles. The compositor honors
    /// blend_clipped_group (clipped members take the base
    /// layer's mode when set), transparency_shapes (off: the
    /// layer blends across its painted bounds at full
    /// strength) and mask_hides_effects (the mask also masks
    /// the below-layer effects). blend_interior_group: the
    /// pipeline always blends interior effects with the layer
    /// as a group, which is the ON behavior; the OFF path
    /// (effects blending separately from the layer) is not a
    /// separate compositor path.
    pub blend_clipped_group: bool,
    pub blend_interior_group: bool,
    pub transparency_shapes: bool,
    pub mask_hides_effects: bool,
    /// Layer set membership: 0 = none, else index+1 into
    /// `Document.sets` (Phase 6; sets are one level deep).
    pub group: u16,
    /// Clipping group member: this layer's content is clipped
    /// to the alpha of the nearest non-clipped layer below.
    pub clipped: bool,
    /// Layer locks bitmask: bit0 lock transparent pixels,
    /// bit1 lock image pixels, bit2 lock position, bit3 lock all.
    pub locks: u8,
    /// Layer color coding (Layer Properties): 0 None, 1 Red,
    /// 2 Orange, 3 Yellow, 4 Green, 5 Blue, 6 Violet, 7 Gray.
    pub color: u8,
    /// Vector shape payload (Phase 6): when present, the
    /// layer's content is rendered from this shape spec.
    pub shape: Option<crate::shapes::ShapeSpec>,
    /// Fill-layer payload (Phase 6): solid / gradient / pattern
    /// content rendered over the whole document.
    pub fill: Option<crate::shapes::FillSpec>,
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
            cmyk_alpha: HashMap::new(),
            linked: false,
            mask: None,
            mask_enabled: true,
            mask_fill: 255,
            adjustment: None,
            effects: None,
            text: None,
            fill_opacity: 1.0,
            blend_if: [0, 255, 0, 255],
            blend_if_split: [0, 0, 0, 0],
            knockout: 0,
            blend_channels: 0xF,
            blend_clipped_group: true,
            blend_interior_group: true,
            transparency_shapes: true,
            mask_hides_effects: false,
            group: 0,
            clipped: false,
            locks: 0,
            color: 0,
            shape: None,
            fill: None,
        }
    }

    /// Mask value at a document point (255 = fully revealed; also
    /// the answer everywhere when the layer has no mask).
    pub fn mask_value(&self, x: i32, y: i32) -> u8 {
        let Some(mask) = &self.mask else { return 255 };
        let (coord, ox, oy) = split_coord(x - self.offset.0, y - self.offset.1);
        match mask.get(&coord) {
            Some(t) => t[(oy as usize) * 256 + ox as usize],
            None => self.mask_fill,
        }
    }

    pub fn set_mask_value(&mut self, x: i32, y: i32, v: u8) {
        let fill = self.mask_fill;
        let (coord, ox, oy) = split_coord(x - self.offset.0, y - self.offset.1);
        let Some(mask) = &mut self.mask else { return };
        if v == fill && !mask.contains_key(&coord) {
            return; // the fill value is the default; don't allocate
        }
        let tile = mask.entry(coord).or_insert_with(|| vec![fill; TILE_PIXELS]);
        tile[(oy as usize) * 256 + ox as usize] = v;
    }

    /// Give the layer a mask filled with `fill` (255 = reveal all,
    /// 0 = hide all). Per-pixel values (e.g. from a selection) are
    /// painted in afterwards with `set_mask_value`.
    pub fn add_mask(&mut self, fill: u8) {
        self.mask = Some(HashMap::new());
        self.mask_enabled = true;
        self.mask_fill = fill;
    }

    /// Alpha of a CMYK-mode pixel (65535 = opaque; the default where
    /// no alpha tile exists).
    pub fn get_alpha(&self, x: i32, y: i32) -> u16 {
        let (coord, ox, oy) = split_coord(x - self.offset.0, y - self.offset.1);
        match self.cmyk_alpha.get(&coord) {
            Some(t) => t[(oy as usize) * 256 + ox as usize],
            None => u16::MAX,
        }
    }

    pub fn set_alpha(&mut self, x: i32, y: i32, v: u16) {
        let (coord, ox, oy) = split_coord(x - self.offset.0, y - self.offset.1);
        if v == u16::MAX && !self.cmyk_alpha.contains_key(&coord) {
            return; // opaque is the default; don't allocate for it
        }
        let tile = self
            .cmyk_alpha
            .entry(coord)
            .or_insert_with(|| vec![u16::MAX; TILE_PIXELS]);
        tile[(oy as usize) * 256 + ox as usize] = v;
    }

    /// Snapshot one CMYK alpha tile (undo + serialization support).
    pub fn snapshot_alpha_tile(&self, coord: TileCoord) -> Option<Vec<u16>> {
        self.cmyk_alpha.get(&coord).cloned()
    }

    pub fn restore_alpha_tile(&mut self, coord: TileCoord, tile: Option<Vec<u16>>) {
        match tile {
            Some(t) => {
                self.cmyk_alpha.insert(coord, t);
            }
            None => {
                self.cmyk_alpha.remove(&coord);
            }
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
