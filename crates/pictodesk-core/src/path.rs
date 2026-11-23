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
    let inside =
        |x: i32, y: i32| -> bool { x >= 0 && y >= 0 && x < w && y < h && sel.value(x, y) >= 128 };
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
                let nd = (e.1 .0 - e.0 .0, e.1 .1 - e.0 .1);
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
        // Drop the duplicated closing vertex.
        if verts.len() > 1 && verts.first() == verts.last() {
            verts.pop();
        }
        if verts.len() < 3 {
            continue;
        }
        let pts: Vec<(f32, f32)> = verts.iter().map(|v| (v.0 as f32, v.1 as f32)).collect();
        let anchors = fit_closed_contour(&pts);
        if anchors.len() >= 3 {
            path.subpaths.push(Subpath {
                anchors,
                closed: true,
            });
        }
    }
    path
}

// ---- Bézier fitting (Schneider's algorithm) -------------------------

type Pt = (f32, f32);

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}
fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}
fn mul(a: Pt, t: f32) -> Pt {
    (a.0 * t, a.1 * t)
}
fn len(a: Pt) -> f32 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}
fn unit(a: Pt) -> Pt {
    let l = len(a);
    if l < 1e-9 {
        (0.0, 0.0)
    } else {
        (a.0 / l, a.1 / l)
    }
}
fn dist(a: Pt, b: Pt) -> f32 {
    len(sub(a, b))
}

/// One fitted cubic: control points and end point (the start is the
/// previous cubic's end, or the span's first point).
struct Cubic {
    c1: Pt,
    c2: Pt,
    end: Pt,
}

fn cubic_at(p0: Pt, c: &Cubic, t: f32) -> Pt {
    let u = 1.0 - t;
    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        b0 * p0.0 + b1 * c.c1.0 + b2 * c.c2.0 + b3 * c.end.0,
        b0 * p0.1 + b1 * c.c1.1 + b2 * c.c2.1 + b3 * c.end.1,
    )
}

/// Fit a chain of points with cubic segments (Schneider 1990):
/// chord-length parameterization, least-squares handle lengths,
/// Newton reparameterization, split at the worst point when the
/// error exceeds `tol`.
fn fit_span(pts: &[Pt], t1: Pt, t2: Pt, tol: f32) -> Vec<Cubic> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let (p0, p3) = (pts[0], pts[n - 1]);
    let d = dist(p0, p3);
    if n == 2 || d < 1e-6 {
        return vec![Cubic {
            c1: add(p0, mul(t1, d / 3.0)),
            c2: add(p3, mul(t2, -d / 3.0)),
            end: p3,
        }];
    }
    // Chord-length parameters.
    let mut u: Vec<f32> = vec![0.0; n];
    for i in 1..n {
        u[i] = u[i - 1] + dist(pts[i], pts[i - 1]);
    }
    let total = u[n - 1];
    if total < 1e-9 {
        return Vec::new();
    }
    for v in &mut u {
        *v /= total;
    }
    let mut bez: Option<Cubic> = None;
    let mut max_err = f32::MAX;
    let mut split = n / 2;
    for _ in 0..4 {
        // Least squares for the two handle lengths.
        let (mut c00, mut c01, mut c11) = (0.0f32, 0.0f32, 0.0f32);
        let (mut x0, mut x1) = (0.0f32, 0.0f32);
        for i in 0..n {
            let t = u[i];
            let w = 1.0 - t;
            let (b0, b1, b2, b3) = (w * w * w, 3.0 * w * w * t, 3.0 * w * t * t, t * t * t);
            let a1 = mul(t1, b1);
            let a2 = mul(t2, -b2);
            c00 += a1.0 * a1.0 + a1.1 * a1.1;
            c01 += a1.0 * a2.0 + a1.1 * a2.1;
            c11 += a2.0 * a2.0 + a2.1 * a2.1;
            let tmp = (
                pts[i].0 - (b0 + b1) * p0.0 - (b2 + b3) * p3.0,
                pts[i].1 - (b0 + b1) * p0.1 - (b2 + b3) * p3.1,
            );
            x0 += a1.0 * tmp.0 + a1.1 * tmp.1;
            x1 += a2.0 * tmp.0 + a2.1 * tmp.1;
        }
        let det = c00 * c11 - c01 * c01;
        let (mut al1, mut al2) = if det.abs() > 1e-9 {
            ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
        } else {
            (0.0, 0.0)
        };
        if al1 < 0.0 || al2 < 0.0 {
            al1 = d / 3.0;
            al2 = d / 3.0;
        }
        let cand = Cubic {
            c1: add(p0, mul(t1, al1)),
            c2: add(p3, mul(t2, -al2)),
            end: p3,
        };
        // Error + Newton reparameterization.
        max_err = 0.0;
        split = n / 2;
        for i in 1..n - 1 {
            let q = cubic_at(p0, &cand, u[i]);
            let e = dist(q, pts[i]);
            if e > max_err {
                max_err = e;
                split = i;
            }
            // Newton step for u[i]: project the error onto the tangent.
            let d1 = (
                3.0 * (1.0 - u[i]).powi(2) * (cand.c1.0 - p0.0)
                    + 6.0 * (1.0 - u[i]) * u[i] * (cand.c2.0 - cand.c1.0)
                    + 3.0 * u[i].powi(2) * (cand.end.0 - cand.c2.0),
                3.0 * (1.0 - u[i]).powi(2) * (cand.c1.1 - p0.1)
                    + 6.0 * (1.0 - u[i]) * u[i] * (cand.c2.1 - cand.c1.1)
                    + 3.0 * u[i].powi(2) * (cand.end.1 - cand.c2.1),
            );
            let denom = d1.0 * d1.0 + d1.1 * d1.1;
            if denom > 1e-9 {
                let num = (q.0 - pts[i].0) * d1.0 + (q.1 - pts[i].1) * d1.1;
                u[i] = (u[i] - num / denom).clamp(0.0, 1.0);
            }
        }
        bez = Some(cand);
        if max_err < tol {
            break;
        }
    }
    if max_err < tol {
        return vec![bez.unwrap()];
    }
    // Split at the worst point; the join tangent follows the chain
    // over a wide window (the chain is a pixel staircase, so a
    // 1-step direction is quantized to the axes and would kink).
    let split = split.clamp(1, n - 2);
    let tm = unit(sub(
        pts[(split + 4).min(n - 1)],
        pts[split.saturating_sub(4)],
    ));
    let mut left = fit_span(&pts[..=split], t1, tm, tol);
    let right = fit_span(&pts[split..], tm, t2, tol);
    left.extend(right);
    left
}

/// Max distance from any point of the chain to the line a-b.
fn line_deviation(pts: &[Pt], a: Pt, b: Pt) -> f32 {
    let ab = sub(b, a);
    let l = len(ab);
    if l < 1e-9 {
        return pts.iter().map(|&p| dist(p, a)).fold(0.0, f32::max);
    }
    let u = (ab.0 / l, ab.1 / l);
    pts.iter()
        .map(|&p| {
            let ap = sub(p, a);
            (ap.0 * u.1 - ap.1 * u.0).abs()
        })
        .fold(0.0, f32::max)
}

/// Fit a closed boundary contour: corners (sharp direction changes)
/// stay corner anchors; smooth runs become fitted Bézier segments
/// with smooth anchors.
fn fit_closed_contour(pts: &[Pt]) -> Vec<Anchor> {
    let n = pts.len();
    // Smooth the pixel staircase first (two 5-point moving-average
    // passes): every local measurement below — corner angles,
    // tangents, fit error — is meaningless on raw staircase data.
    // Corner node positions still come from the original points.
    let orig: Vec<Pt> = pts.to_vec();
    let sm: Vec<Pt> = {
        let avg = |v: &[Pt]| -> Vec<Pt> {
            (0..n)
                .map(|i| {
                    let mut acc = (0.0f32, 0.0f32);
                    for k in -2..=2i32 {
                        let p = v[(i as i32 + k).rem_euclid(n as i32) as usize];
                        acc.0 += p.0;
                        acc.1 += p.1;
                    }
                    (acc.0 / 5.0, acc.1 / 5.0)
                })
                .collect()
        };
        avg(&avg(&orig))
    };
    let pts = &sm[..];
    if n < 8 {
        // Tiny contour: keep the direction-change corners as-is.
        let mut out = Vec::new();
        for i in 0..n {
            let prev = pts[(i + n - 1) % n];
            let next = pts[(i + 1) % n];
            if sub(pts[i], prev) != sub(next, pts[i]) {
                out.push(Anchor::corner(pts[i].0, pts[i].1));
            }
        }
        return out;
    }
    // Corner detection on the smoothed contour, ±3-index windows.
    let turn_cos = |i: usize| -> f32 {
        let a = sub(pts[i], pts[(i + n - 3) % n]);
        let b = sub(pts[(i + 3) % n], pts[i]);
        let (la, lb) = (len(a), len(b));
        if la < 1e-9 || lb < 1e-9 {
            return 1.0;
        }
        (a.0 * b.0 + a.1 * b.1) / (la * lb)
    };
    // Candidates cluster around a true corner (the vertices beside
    // it also turn); keep only the sharpest index of each cluster.
    let mut nodes: Vec<usize> = Vec::new();
    {
        let cands: Vec<usize> = (0..n).filter(|&i| turn_cos(i) < 0.65).collect();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for &i in &cands {
            match groups.last_mut() {
                Some(g) if i - *g.last().unwrap() <= 2 => g.push(i),
                _ => groups.push(vec![i]),
            }
        }
        // Merge a wrap-around cluster (first and last groups adjacent
        // across index 0).
        if groups.len() > 1 {
            let first_min = *groups.first().unwrap().first().unwrap();
            let last_max = *groups.last().unwrap().last().unwrap();
            if first_min + n - last_max <= 2 {
                let mut merged = groups.pop().unwrap();
                merged.extend(groups.remove(0));
                groups.push(merged);
            }
        }
        for g in groups {
            let best = g
                .iter()
                .copied()
                .min_by(|&a, &b| turn_cos(a).partial_cmp(&turn_cos(b)).unwrap())
                .unwrap();
            nodes.push(best);
        }
        nodes.sort_unstable();
    }
    let smooth_node: Vec<bool>;
    if nodes.is_empty() {
        // No corners (ellipse, blob): split at the farthest point so
        // the loop becomes two fittable spans with smooth joins.
        let mut far = 0usize;
        let mut fd = 0.0f32;
        for i in 0..n {
            let d = dist(pts[i], pts[0]);
            if d > fd {
                fd = d;
                far = i;
            }
        }
        nodes = vec![0, far];
        smooth_node = vec![true, true];
    } else {
        smooth_node = vec![false; nodes.len()];
    }
    // Fit each span between consecutive nodes (cyclic).
    struct SegFit {
        cubics: Vec<Cubic>, // empty = straight
    }
    let mut seg_fits: Vec<SegFit> = Vec::new();
    let m = nodes.len();
    for s in 0..m {
        let (i0, i1) = (nodes[s], nodes[(s + 1) % m]);
        // Collect the chain i0 -> i1 (cyclic, inclusive), with the
        // endpoints pinned to the node positions (exact originals
        // at corners).
        let mut chain: Vec<Pt> = Vec::new();
        let mut i = i0;
        loop {
            chain.push(pts[i]);
            if i == i1 {
                break;
            }
            i = (i + 1) % n;
        }
        let node_pos = |idx: usize, smooth: bool| if smooth { pts[idx] } else { orig[idx] };
        let last = chain.len() - 1;
        chain[0] = node_pos(i0, smooth_node[s]);
        chain[last] = node_pos(i1, smooth_node[(s + 1) % m]);
        if line_deviation(&chain, chain[0], chain[chain.len() - 1]) < 0.6 {
            seg_fits.push(SegFit { cubics: Vec::new() });
            continue;
        }
        // End tangents: a smooth node shares the contour's local
        // direction; a corner takes the span's own end direction.
        let local_dir = |idx: usize| unit(sub(pts[(idx + 4) % n], pts[(idx + n - 4) % n]));
        let span_dir = |chain: &[Pt], at_start: bool| {
            let k = 3.min(chain.len() - 1);
            if at_start {
                unit(sub(chain[k], chain[0]))
            } else {
                unit(sub(chain[chain.len() - 1], chain[chain.len() - 1 - k]))
            }
        };
        let t1 = if smooth_node[s] {
            local_dir(i0)
        } else {
            span_dir(&chain, true)
        };
        let t2 = if smooth_node[(s + 1) % m] {
            local_dir(i1)
        } else {
            span_dir(&chain, false)
        };
        seg_fits.push(SegFit {
            cubics: fit_span(&chain, t1, t2, 0.5),
        });
    }
    // Assemble anchors: node anchors get their handles from the
    // adjacent segment fits; interior fit joins are smooth anchors.
    let mut anchors: Vec<Anchor> = Vec::new();
    for s in 0..m {
        let node_pt = if smooth_node[s] {
            pts[nodes[s]]
        } else {
            orig[nodes[s]]
        };
        let prev = &seg_fits[(s + m - 1) % m];
        let next = &seg_fits[s];
        let in_ctrl = match prev.cubics.last() {
            Some(c) => c.c2,
            None => node_pt,
        };
        let out_ctrl = match next.cubics.first() {
            Some(c) => c.c1,
            None => node_pt,
        };
        anchors.push(Anchor {
            point: node_pt,
            in_ctrl,
            out_ctrl,
        });
        // Interior joins of this segment.
        for c in &next.cubics[..next.cubics.len().saturating_sub(1)] {
            anchors.push(Anchor {
                point: c.end,
                in_ctrl: c.c2,
                out_ctrl: (0.0, 0.0), // patched below from the next cubic
            });
        }
        // Patch the out_ctrl of interior joins from the following cubic.
        let base = anchors.len() - next.cubics.len().saturating_sub(1);
        for (j, c) in next
            .cubics
            .iter()
            .enumerate()
            .take(next.cubics.len().saturating_sub(1))
        {
            let _ = c;
            anchors[base + j].out_ctrl = next.cubics[j + 1].c1;
        }
    }
    anchors
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
        assert!(
            line.len() > 24,
            "curve should subdivide, got {}",
            line.len()
        );
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
    fn selection_to_path_fits_curves() {
        // An ellipse must come back as a handful of SMOOTH anchors
        // (curve fitting), not hundreds of staircase corners.
        let sel = Selection::ellipse(120, 90, 10, 10, 100, 70);
        let p = selection_to_path(&sel);
        assert_eq!(p.subpaths.len(), 1);
        let anchors = &p.subpaths[0].anchors;
        assert!(
            anchors.len() >= 4 && anchors.len() <= 12,
            "ellipse fitted to {} anchors",
            anchors.len()
        );
        for a in anchors {
            assert!(
                a.in_ctrl != a.point && a.out_ctrl != a.point,
                "anchor at {:?} should be smooth",
                a.point
            );
        }
        // The fitted path's fill closely matches the original
        // selection (small symmetric difference at the boundary).
        let back = p.to_selection(120, 90);
        let mut diff = 0u32;
        let mut area = 0u32;
        for y in 0..90 {
            for x in 0..120 {
                let a = sel.value(x, y) >= 128;
                let b = back.value(x, y) >= 128;
                if a {
                    area += 1;
                }
                if a != b {
                    diff += 1;
                }
            }
        }
        assert!(
            diff * 50 < area,
            "fitted fill differs by {diff}px of {area}px"
        );
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
