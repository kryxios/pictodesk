//! Vector shapes and fill layers (Phase 6).
//!
//! A shape layer carries a `ShapeSpec` instead of painted
//! pixels: the compositor samples `shape_pixel` per point, so
//! the shape stays editable forever (change the spec, the
//! render changes) until the user rasterizes it. Fill layers
//! work the same way with `FillSpec` over the whole canvas.
//! Rasterization is point-sampled at pixel centers — crisp
//! classic edges, no anti-aliasing, like the era's vector
//! masks at 100%.

use crate::pixel::Pixel16;

/// A vector shape: kind + bounding rect in document coords.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeSpec {
    /// 0 Rectangle, 1 Rounded Rectangle, 2 Ellipse, 3 Polygon,
    /// 4 Line, 5 Custom (points in `points`, normalized 0..=1
    /// over the rect).
    pub kind: u8,
    pub rect: (i32, i32, i32, i32),
    /// Polygon sides (3..=100) / star points.
    pub sides: u8,
    /// Polygon as a star (indent by ~50%).
    pub star: bool,
    /// Rounded-rect corner radius / polygon smoothness, px.
    pub radius: u32,
    pub fill: Pixel16,
    pub fill_on: bool,
    pub stroke: Pixel16,
    pub stroke_width: u32,
    pub points: Vec<(f32, f32)>,
    /// Line arrowheads (Phase 6.5): filled triangles at the
    /// segment's start and/or end, sized from the stroke
    /// width.
    pub arrow_start: bool,
    pub arrow_end: bool,
}

impl Default for ShapeSpec {
    fn default() -> Self {
        ShapeSpec {
            kind: 0,
            rect: (0, 0, 100, 100),
            sides: 5,
            star: false,
            radius: 8,
            fill: Pixel16::from_rgba8(255, 0, 0, 255),
            fill_on: true,
            stroke: Pixel16::from_rgba8(0, 0, 0, 255),
            stroke_width: 0,
            points: Vec::new(),
            arrow_start: false,
            arrow_end: false,
        }
    }
}

/// Fill-layer content over the whole document.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillSpec {
    /// 0 Solid, 1 Gradient, 2 Pattern.
    pub kind: u8,
    pub color: Pixel16,
    pub to: Pixel16,
    /// Gradient angle (degrees) / style (0 Linear, 1 Radial).
    pub angle: i32,
    pub style: u8,
    pub scale: u32,
    /// Pattern kind (see effects::pattern_value).
    pub pattern_kind: u8,
}

impl Default for FillSpec {
    fn default() -> Self {
        FillSpec {
            kind: 0,
            color: Pixel16::from_rgba8(255, 255, 255, 255),
            to: Pixel16::from_rgba8(0, 0, 0, 255),
            angle: 90,
            style: 0,
            scale: 100,
            pattern_kind: 0,
        }
    }
}

fn in_polygon(pts: &[(f32, f32)], x: f32, y: f32) -> bool {
    // Even-odd ray cast.
    let mut inside = false;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % n];
        if (y1 > y) != (y2 > y) {
            let xi = x1 + (y - y1) / (y2 - y1) * (x2 - x1);
            if x < xi {
                inside = !inside;
            }
        }
    }
    inside
}

fn polygon_points(spec: &ShapeSpec) -> Vec<(f32, f32)> {
    let (rx, ry, rw, rh) = spec.rect;
    let (cx, cy) = (rx as f32 + rw as f32 / 2.0, ry as f32 + rh as f32 / 2.0);
    let n = spec.sides.clamp(3, 100) as usize;
    let steps = if spec.star { n * 2 } else { n };
    let mut pts = Vec::with_capacity(steps);
    for i in 0..steps {
        let a = (i as f32 / steps as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let inner = spec.star && i % 2 == 1;
        let f = if inner { 0.5 } else { 1.0 };
        pts.push((
            cx + a.cos() * (rw as f32 / 2.0) * f,
            cy + a.sin() * (rh as f32 / 2.0) * f,
        ));
    }
    pts
}

/// Is (x, y) inside the shape shrunk by `shrink` px on each
/// side (used for stroke rings)?
fn inside(spec: &ShapeSpec, x: f32, y: f32, shrink: f32) -> bool {
    let (rx, ry, rw, rh) = spec.rect;
    let (rx, ry, rw, rh) = (
        rx as f32 + shrink,
        ry as f32 + shrink,
        rw as f32 - 2.0 * shrink,
        rh as f32 - 2.0 * shrink,
    );
    // Lines may legitimately have a zero width or height
    // (perfectly horizontal / vertical drags).
    if rw < 0.0 || rh < 0.0 || (spec.kind != 4 && (rw <= 0.0 || rh <= 0.0)) {
        return false;
    }
    match spec.kind {
        0 => x >= rx && x < rx + rw && y >= ry && y < ry + rh,
        1 => {
            let r = (spec.radius as f32).min(rw / 2.0).min(rh / 2.0);
            if !(x >= rx && x < rx + rw && y >= ry && y < ry + rh) {
                return false;
            }
            let cy = if y < ry + r {
                ry + r
            } else if y >= ry + rh - r {
                ry + rh - r
            } else {
                y
            };
            let cxx = if x < rx + r {
                rx + r
            } else if x >= rx + rw - r {
                rx + rw - r
            } else {
                x
            };
            (x - cxx) * (x - cxx) + (y - cy) * (y - cy) <= r * r
        }
        2 => {
            let (cx, cy) = (rx + rw / 2.0, ry + rh / 2.0);
            let (dx, dy) = ((x - cx) / (rw / 2.0), (y - cy) / (rh / 2.0));
            dx * dx + dy * dy <= 1.0
        }
        3 => {
            let mut sp = spec.clone();
            sp.rect = (rx as i32, ry as i32, rw as i32, rh as i32);
            in_polygon(&polygon_points(&sp), x, y)
        }
        4 => {
            // Line: distance to the rect's diagonal segment.
            let (x1, y1) = (rx, ry + rh);
            let (x2, y2) = (rx + rw, ry);
            let (dx, dy) = (x2 - x1, y2 - y1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 <= 0.0 {
                0.0
            } else {
                (((x - x1) * dx + (y - y1) * dy) / len2).clamp(0.0, 1.0)
            };
            let (px, py) = (x1 + t * dx, y1 + t * dy);
            let half = (spec.stroke_width.max(1) as f32) / 2.0 - shrink;
            if (x - px) * (x - px) + (y - py) * (y - py) <= half * half {
                return true;
            }
            // Arrowheads: triangles at either end.
            if (spec.arrow_start || spec.arrow_end) && len2 > 0.0 {
                let len = len2.sqrt();
                let (ux, uy) = (dx / len, dy / len);
                let (nx, ny) = (-uy, ux);
                let sw = spec.stroke_width.max(1) as f32;
                let hl = (sw * 4.0).max(10.0);
                let hh = (sw * 1.6).max(3.0);
                let head = |tip: (f32, f32), dir: f32| -> bool {
                    let (bx, by) = (tip.0 - ux * dir * hl, tip.1 - uy * dir * hl);
                    in_polygon(
                        &[
                            tip,
                            (bx + nx * hh, by + ny * hh),
                            (bx - nx * hh, by - ny * hh),
                        ],
                        x,
                        y,
                    )
                };
                if spec.arrow_end && head((x2, y2), 1.0) {
                    return true;
                }
                if spec.arrow_start && head((x1, y1), -1.0) {
                    return true;
                }
            }
            false
        }
        _ => {
            if spec.points.len() < 3 {
                return false;
            }
            let pts: Vec<(f32, f32)> = spec
                .points
                .iter()
                .map(|(nx, ny)| (rx + nx * rw, ry + ny * rh))
                .collect();
            in_polygon(&pts, x, y)
        }
    }
}

/// The shape layer's pixel at a document point.
pub fn shape_pixel(spec: &ShapeSpec, x: i32, y: i32) -> Pixel16 {
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let sw = spec.stroke_width as f32;
    if sw > 0.0 {
        // Stroke ring: inside the shape but not inside it shrunk
        // by the stroke width (for lines, `inside` already is
        // the stroke body).
        if spec.kind == 4 {
            if inside(spec, fx, fy, 0.0) {
                return spec.stroke;
            }
            return Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0);
        }
        if inside(spec, fx, fy, 0.0) && !inside(spec, fx, fy, sw) {
            return spec.stroke;
        }
    }
    if spec.fill_on && inside(spec, fx, fy, 0.0) {
        return spec.fill;
    }
    Pixel16::from_f32s(0.0, 0.0, 0.0, 0.0)
}

/// The fill layer's pixel at a document point.
pub fn fill_pixel(spec: &FillSpec, x: i32, y: i32, w: u32, h: u32) -> Pixel16 {
    match spec.kind {
        1 => {
            let (nx, ny) = (
                x as f32 / w.max(1) as f32 - 0.5,
                y as f32 / h.max(1) as f32 - 0.5,
            );
            let scale = (spec.scale.max(10) as f32) / 100.0;
            let t = if spec.style == 1 {
                ((nx * nx + ny * ny).sqrt() * 1.42 / scale).clamp(0.0, 1.0)
            } else {
                let a = (spec.angle as f32).to_radians();
                ((nx * a.cos() - ny * a.sin()) / scale + 0.5).clamp(0.0, 1.0)
            };
            let (fr, fg, fb, fa) = spec.color.to_f32s();
            let (tr, tg, tb, ta) = spec.to.to_f32s();
            Pixel16::from_f32s(
                fr + (tr - fr) * t,
                fg + (tg - fg) * t,
                fb + (tb - fb) * t,
                fa + (ta - fa) * t,
            )
        }
        2 => {
            let v = crate::effects::pattern_value(spec.pattern_kind, x, y, spec.scale);
            let (fr, fg, fb, _) = spec.color.to_f32s();
            let (tr, tg, tb, _) = spec.to.to_f32s();
            Pixel16::from_f32s(
                tr + (fr - tr) * v,
                tg + (fg - tg) * v,
                tb + (fb - tb) * v,
                1.0,
            )
        }
        _ => spec.color,
    }
}
