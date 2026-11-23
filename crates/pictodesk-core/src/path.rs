//! Bézier paths — the the classic Pen Tool data model.
//!
//! A path is a list of subpaths; each subpath is a list of anchors.
//! Every anchor carries an incoming and an outgoing control point
//! (the classic direction points). When both control points coincide
//! with the anchor it is a corner point; otherwise it is a smooth
//! point. Paths are resolution-independent: they flatten to polygons
//! for selection conversion and stroking, and they serialize into
//! the `.pdesk` format and into Photoshop path resources on export.

use crate::selection::Selection;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub point: (f32, f32),
    pub in_ctrl: (f32, f32),
    pub out_ctrl: (f32, f32),
}

impl Anchor {
    pub fn corner(x: f32, y: f32) -> Self {
        Anchor {
            point: (x, y),
            in_ctrl: (x, y),
            out_ctrl: (x, y),
        }
    }
    pub fn smooth(point: (f32, f32), in_ctrl: (f32, f32), out_ctrl: (f32, f32)) -> Self {
        Anchor {
            point,
            in_ctrl,
            out_ctrl,
        }
    }
    pub fn is_corner(&self) -> bool {
        self.in_ctrl == self.point && self.out_ctrl == self.point
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subpath {
    pub anchors: Vec<Anchor>,
    pub closed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub name: String,
    pub subpaths: Vec<Subpath>,
}

fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

fn flatten_segment(
    out: &mut Vec<(f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    depth: u32,
) {
    // Flatness: distance of both control points from the chord.
    let chord = ((p3.0 - p0.0), (p3.1 - p0.1));
    let len = (chord.0 * chord.0 + chord.1 * chord.1).sqrt().max(1e-6);
    let d1 = ((p1.0 - p0.0) * chord.1 - (p1.1 - p0.1) * chord.0).abs() / len;
    let d2 = ((p2.0 - p0.0) * chord.1 - (p2.1 - p0.1) * chord.0).abs() / len;
    if (d1 + d2) < 0.35 || depth >= 12 {
        out.push(p3);
        return;
    }
    let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let q2 = mid(p2, p3);
    let r0 = mid(q0, q1);
    let r1 = mid(q1, q2);
    let s = mid(r0, r1);
    flatten_segment(out, p0, q0, r0, s, depth + 1);
    flatten_segment(out, s, r1, q2, p3, depth + 1);
}

impl Subpath {
    /// Flatten to a polyline. Closed subpaths do not repeat the first
    /// point at the end; consumers wrap when they need to.
    pub fn flatten(&self) -> Vec<(f32, f32)> {
        let n = self.anchors.len();
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.anchors[0].point];
        }
        let mut out = vec![self.anchors[0].point];
        let segs = if self.closed { n } else { n - 1 };
        for i in 0..segs {
            let a = &self.anchors[i];
            let b = &self.anchors[(i + 1) % n];
            flatten_segment(&mut out, a.point, a.out_ctrl, b.in_ctrl, b.point, 0);
        }
        if self.closed && out.len() > 1 && out.last() == out.first() {
            out.pop();
        }
        out
    }
}

impl Path {
    pub fn new(name: impl Into<String>) -> Self {
        Path {
            name: name.into(),
            subpaths: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.iter().all(|s| s.anchors.is_empty())
    }

    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(f32, f32, f32, f32)> = None;
        for sp in &self.subpaths {
            for pt in sp.flatten() {
                b = Some(match b {
                    None => (pt.0, pt.1, pt.0, pt.1),
                    Some((x0, y0, x1, y1)) => {
                        (x0.min(pt.0), y0.min(pt.1), x1.max(pt.0), y1.max(pt.1))
                    }
                });
            }
        }
        b
    }

    /// Rasterize the path into a selection mask: subpaths are filled
    /// and combined with even-odd (XOR), so a subpath inside another
    /// punches a hole — the classic combined-path behavior at this
    /// level.
    pub fn to_selection(&self, width: u32, height: u32) -> Selection {
        let mut sel = Selection::none(width, height);
        for sp in &self.subpaths {
            if sp.anchors.len() < 2 {
                continue;
            }
            let poly = sp.flatten();
            if poly.len() < 3 {
                continue;
            }
            let one = Selection::polygon(width, height, &poly);
            for y in 0..height as i32 {
                for x in 0..width as i32 {
                    if one.value(x, y) > 0 {
                        let cur = sel.value(x, y);
                        sel.set(x, y, if cur > 0 { 0 } else { 255 });
                    }
                }
            }
        }
        sel
    }

    /// Points along the path at roughly `spacing` pixel intervals,
    /// for brush stroking.
    pub fn stroke_points(&self, spacing: f32) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for sp in &self.subpaths {
            let line = sp.flatten();
            if line.len() < 2 {
                continue;
            }
            let mut pts = line.clone();
            if sp.closed {
                pts.push(line[0]);
            }
            let mut acc = spacing; // emit the first point immediately
            for seg in pts.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                if len < 1e-6 {
                    continue;
                }
                let mut t = acc;
                while t <= len {
                    out.push((a.0 + (b.0 - a.0) * t / len, a.1 + (b.1 - a.1) * t / len));
                    t += spacing;
                }
                acc = t - len;
            }
        }
        out
    }
}

/// Make Work Path from a selection: trace the selection boundary
/// (crack following on the pixel grid) into closed subpaths of
/// corner anchors. Holes become their own subpaths, so converting
/// back with even-odd fill reproduces the shape.
pub fn selection_to_path(sel: &Selection) -> Path {
    let w = sel.width as i32;
    let h = sel.height as i32;
    let inside = |x: i32, y: i32| -> bool {
        x >= 0 && y >= 0 && x < w && y < h && sel.value(x, y) >= 128
    };
    // Directed boundary edges on pixel corners, interior on the left
    // of travel... we chain purely by endpoint matching; direction
    // does not matter for even-odd fill.
    use std::collections::HashMap;
    let mut edges: Vec<((i32, i32), (i32, i32))> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            if !inside(x, y - 1) {
                edges.push(((x, y), (x + 1, y))); // top
            }
            if !inside(x, y + 1) {
                edges.push(((x + 1, y + 1), (x, y + 1))); // bottom
            }
            if !inside(x - 1, y) {
                edges.push(((x, y + 1), (x, y))); // left
            }
            if !inside(x + 1, y) {
                edges.push(((x + 1, y), (x + 1, y + 1))); // right
            }
        }
    }
    // Index edges by start vertex.
    let mut by_start: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        by_start.entry(e.0).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut path = Path::new("Work Path");
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut verts: Vec<(i32, i32)> = Vec::new();
        let mut cur = start;
        loop {
            used[cur] = true;
            let (a, b) = edges[cur];
            if verts.is_empty() {
                verts.push(a);
            }
            verts.push(b);
            if b == verts[0] {
                break;
            }
            // Next edge: one starting at b, preferring a right turn
            // (keeps the traced loop tight around the region).
            let cands = by_start.get(&b).cloned().unwrap_or_default();
            let dir = (b.0 - a.0, b.1 - a.1);
            let mut next: Option<usize> = None;
            let mut best = i32::MIN;
            for ci in cands {
                if used[ci] {
                    continue;
                }
                let e = edges[ci];
                let nd = (e.1.0 - e.0.0, e.1.1 - e.0.1);
                // score: right turn +2, straight +1, left 0, back -1
                let cross = dir.0 * nd.1 - dir.1 * nd.0;
                let dot = dir.0 * nd.0 + dir.1 * nd.1;
                let score = if cross < 0 {
                    2
                } else if dot > 0 {
                    1
                } else if cross > 0 {
                    0
                } else {
                    -1
                };
                if score > best {
                    best = score;
                    next = Some(ci);
                }
            }
            match next {
                Some(n) => cur = n,
                None => break,
            }
            if verts.len() > edges.len() + 2 {
                break; // safety
            }
        }
        // Drop the duplicated closing vertex, remove collinear runs.
        if verts.len() > 1 && verts.first() == verts.last() {
            verts.pop();
        }
        if verts.len() < 3 {
            continue;
        }
        let mut anchors: Vec<Anchor> = Vec::new();
        let n = verts.len();
        for i in 0..n {
            let prev = verts[(i + n - 1) % n];
            let cur = verts[i];
            let next = verts[(i + 1) % n];
            let d1 = (cur.0 - prev.0, cur.1 - prev.1);
            let d2 = (next.0 - cur.0, next.1 - cur.1);
            if d1 != d2 {
                anchors.push(Anchor::corner(cur.0 as f32, cur.1 as f32));
            }
        }
        if anchors.len() >= 3 {
            path.subpaths.push(Subpath {
                anchors,
                closed: true,
            });
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ellipse_path() -> Path {
        // Four smooth anchors approximating a 200x100 ellipse at (200,100).
        let mut p = Path::new("Ellipse");
        let k = 0.5523f32;
        let (cx, cy, rx, ry) = (200.0f32, 100.0f32, 100.0f32, 50.0f32);
        let anchors = vec![
            Anchor::smooth(
                (cx + rx, cy),
                (cx + rx, cy - ry * k),
                (cx + rx, cy + ry * k),
            ),
            Anchor::smooth(
                (cx, cy + ry),
                (cx + rx * k, cy + ry),
                (cx - rx * k, cy + ry),
            ),
            Anchor::smooth(
                (cx - rx, cy),
                (cx - rx, cy + ry * k),
                (cx - rx, cy - ry * k),
            ),
            Anchor::smooth(
                (cx, cy - ry),
                (cx - rx * k, cy - ry),
                (cx + rx * k, cy - ry),
            ),
        ];
        p.subpaths.push(Subpath {
            anchors,
            closed: true,
        });
        p
    }

    #[test]
    fn flatten_ellipse_is_round_and_closed_shape() {
        let p = ellipse_path();
        let line = p.subpaths[0].flatten();
        assert!(line.len() > 24, "curve should subdivide, got {}", line.len());
        let (x0, y0, x1, y1) = p.bounds().unwrap();
        assert!((x0 - 100.0).abs() < 1.5 && (x1 - 300.0).abs() < 1.5);
        assert!((y0 - 50.0).abs() < 1.5 && (y1 - 150.0).abs() < 1.5);
    }

    #[test]
    fn path_to_selection_fills_inside_not_outside() {
        let p = ellipse_path();
        let sel = p.to_selection(400, 200);
        assert_eq!(sel.value(200, 100), 255, "center selected");
        assert_eq!(sel.value(200, 60), 255, "near top inside");
        assert_eq!(sel.value(105, 55), 0, "corner outside the curve");
        assert_eq!(sel.value(10, 10), 0);
    }

    #[test]
    fn selection_round_trip_through_path() {
        let sel = Selection::rect(60, 40, 10, 8, 30, 20);
        let p = selection_to_path(&sel);
        assert_eq!(p.subpaths.len(), 1);
        // A rectangle traces to exactly 4 corner anchors.
        assert_eq!(p.subpaths[0].anchors.len(), 4);
        let back = p.to_selection(60, 40);
        for y in 0..40 {
            for x in 0..60 {
                assert_eq!(sel.value(x, y), back.value(x, y), "at {x},{y}");
            }
        }
    }

    #[test]
    fn selection_to_path_keeps_holes() {
        // Ring: outer rect minus inner rect, via two selections XORed
        // manually through path conversion of each and even-odd fill.
        let outer = Selection::rect(40, 40, 5, 5, 30, 30);
        let mut p = selection_to_path(&outer);
        let inner = Selection::rect(40, 40, 15, 15, 10, 10);
        let ip = selection_to_path(&inner);
        p.subpaths.extend(ip.subpaths);
        let back = p.to_selection(40, 40);
        assert_eq!(back.value(10, 10), 255, "ring body");
        assert_eq!(back.value(20, 20), 0, "hole stays empty");
    }

    #[test]
    fn stroke_points_cover_the_path() {
        let p = ellipse_path();
        let pts = p.stroke_points(5.0);
        assert!(pts.len() > 60, "ellipse circumference ~484px / 5 = ~97");
        // Every stroke point lies near the ellipse boundary.
        for (x, y) in &pts {
            let d = (((x - 200.0) / 100.0).powi(2) + ((y - 100.0) / 50.0).powi(2)).sqrt();
            assert!((d - 1.0).abs() < 0.08, "point {x},{y} off-curve: {d}");
        }
    }
}
