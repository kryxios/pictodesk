//! Viewport: the mapping between document pixels and screen pixels.
//! Pure math, unit-tested — the window code is a thin shell over this.
//!
//! screen = doc * scale + offset

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub scale: f32,
    pub offset: (f32, f32),
}

impl Viewport {
    /// Fit a doc_w x doc_h document into a win_w x win_h window, centered.
    pub fn fit(doc_w: f32, doc_h: f32, win_w: f32, win_h: f32) -> Self {
        let scale = (win_w / doc_w).min(win_h / doc_h).min(1.0);
        let offset = ((win_w - doc_w * scale) / 2.0, (win_h - doc_h * scale) / 2.0);
        Viewport { scale, offset }
    }

    pub fn doc_to_screen(&self, doc: (f32, f32)) -> (f32, f32) {
        (
            doc.0 * self.scale + self.offset.0,
            doc.1 * self.scale + self.offset.1,
        )
    }

    pub fn screen_to_doc(&self, screen: (f32, f32)) -> (f32, f32) {
        (
            (screen.0 - self.offset.0) / self.scale,
            (screen.1 - self.offset.1) / self.scale,
        )
    }

    /// Zoom by `factor`, keeping the document point under `cursor`
    /// (screen coords) exactly under the cursor. This is the behavior
    /// that makes zoom feel right in every real editor.
    pub fn zoom_at(&mut self, cursor: (f32, f32), factor: f32) {
        let before = self.screen_to_doc(cursor);
        self.scale = (self.scale * factor).clamp(0.05, 64.0);
        let after_screen = self.doc_to_screen(before);
        self.offset.0 += cursor.0 - after_screen.0;
        self.offset.1 += cursor.1 - after_screen.1;
    }

    pub fn pan_by(&mut self, delta: (f32, f32)) {
        self.offset.0 += delta.0;
        self.offset.1 += delta.1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn fit_centers_and_never_upscales() {
        let vp = Viewport::fit(640.0, 400.0, 1280.0, 800.0);
        assert!(close(vp.scale, 1.0));
        assert!(close(vp.offset.0, 320.0) && close(vp.offset.1, 200.0));
        let vp2 = Viewport::fit(640.0, 400.0, 320.0, 400.0);
        assert!(close(vp2.scale, 0.5));
    }

    #[test]
    fn zoom_at_keeps_cursor_point_fixed() {
        let mut vp = Viewport::fit(640.0, 400.0, 800.0, 600.0);
        let cursor = (300.0, 250.0);
        let doc_before = vp.screen_to_doc(cursor);
        vp.zoom_at(cursor, 2.0);
        let doc_after = vp.screen_to_doc(cursor);
        assert!(
            close(doc_before.0, doc_after.0) && close(doc_before.1, doc_after.1),
            "doc point drifted: {doc_before:?} -> {doc_after:?}"
        );
        // 400% zoom from fit must be reachable (Phase 0 exit: smooth at 400%).
        vp.zoom_at(cursor, 2.0);
        assert!(vp.scale >= 3.9, "scale {}", vp.scale);
    }

    #[test]
    fn pan_moves_the_view() {
        let mut vp = Viewport::fit(100.0, 100.0, 200.0, 200.0);
        let c = vp.doc_to_screen((50.0, 50.0));
        vp.pan_by((10.0, -5.0));
        let c2 = vp.doc_to_screen((50.0, 50.0));
        assert!(close(c2.0, c.0 + 10.0) && close(c2.1, c.1 - 5.0));
    }

    #[test]
    fn zoom_clamps() {
        let mut vp = Viewport {
            scale: 1.0,
            offset: (0.0, 0.0),
        };
        vp.zoom_at((0.0, 0.0), 1000.0);
        assert!(vp.scale <= 64.0);
        vp.zoom_at((0.0, 0.0), 0.0001);
        assert!(vp.scale >= 0.05);
    }
}
