//! Rendering: the frame, canvas, rulers, toolbox, options bar, menus and status.
use super::*;
use crate::menus::OptAct;
use crate::panels::PanelKind;
use crate::tools::{ChannelView, DragMode, Tool};

pub(crate) fn draw_tool_icon(f: &mut Frame, tool: Tool, r: Rect) {
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    let b = ui::ICON;
    match tool {
        Tool::Move => {
            f.line(cx - 8, cy, cx + 8, cy, b);
            f.line(cx, cy - 8, cx, cy + 8, b);
            // Triangular arrowheads on all four points.
            for dy in -3i32..=3 {
                let inset = dy.abs();
                f.fill_rect(Rect::new(cx - 8 + inset, cy + dy, 4 - inset, 1), b);
                f.fill_rect(Rect::new(cx + 5 - (3 - inset), cy + dy, 4 - inset, 1), b);
            }
            for dx in -3i32..=3 {
                let inset = dx.abs();
                f.fill_rect(Rect::new(cx + dx, cy - 8 + inset, 1, 4 - inset), b);
                f.fill_rect(Rect::new(cx + dx, cy + 5 - (3 - inset), 1, 4 - inset), b);
            }
        }
        Tool::MarqueeRect => {
            for x in (cx - 7)..(cx + 8) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy - 6, b);
                    f.set(x, cy + 6, b);
                }
            }
            for y in (cy - 6)..(cy + 7) {
                if (y / 3) % 2 == 0 {
                    f.set(cx - 7, y, b);
                    f.set(cx + 7, y, b);
                }
            }
        }
        Tool::MarqueeEllipse => {
            for a in 0..48 {
                let t = a as f32 / 48.0 * std::f32::consts::TAU;
                if a % 4 < 2 {
                    f.set(cx + (t.cos() * 7.0) as i32, cy + (t.sin() * 6.0) as i32, b);
                }
            }
        }
        Tool::MarqueeRow => {
            for x in (cx - 8)..(cx + 9) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy, b);
                }
            }
            f.set(cx, cy - 5, b);
            f.set(cx, cy + 5, b);
        }
        Tool::MarqueeCol => {
            for y in (cy - 8)..(cy + 9) {
                if (y / 3) % 2 == 0 {
                    f.set(cx, y, b);
                }
            }
            f.set(cx - 5, cy, b);
            f.set(cx + 5, cy, b);
        }
        Tool::Lasso => {
            let pts = [
                (cx - 6, cy + 4),
                (cx - 2, cy - 6),
                (cx + 5, cy - 5),
                (cx + 7, cy + 2),
                (cx + 2, cy + 6),
            ];
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, b);
            }
            f.line(pts[4].0, pts[4].1, pts[0].0, pts[0].1, b);
        }
        Tool::Wand => {
            f.line(cx - 6, cy + 6, cx + 2, cy - 2, b);
            // Four-point sparkle at the tip, plus a small satellite.
            f.line(cx + 4, cy - 8, cx + 4, cy - 2, b);
            f.line(cx + 1, cy - 5, cx + 7, cy - 5, b);
            f.line(cx + 2, cy - 7, cx + 6, cy - 3, b);
            f.line(cx + 6, cy - 7, cx + 2, cy - 3, b);
            f.set(cx - 2, cy - 6, b);
            f.set(cx - 3, cy - 5, b);
            f.set(cx - 2, cy - 4, b);
            f.set(cx - 1, cy - 5, b);
        }
        Tool::Eyedropper => {
            // Pipette: rubber bulb upper-right, tube converging to
            // the picking tip at lower-left.
            f.line(cx - 6, cy + 6, cx + 3, cy - 3, b);
            f.line(cx - 4, cy + 7, cx + 5, cy - 2, b);
            f.fill_rect(Rect::new(cx + 3, cy - 7, 5, 5), b);
            f.set(cx - 7, cy + 7, b);
            f.set(cx - 6, cy + 6, b);
        }
        Tool::ShapeRect => {
            f.outline_rect(Rect::new(cx - 7, cy - 6, 15, 13), b);
        }
        Tool::ShapeRounded => {
            f.outline_rect(Rect::new(cx - 7, cy - 6, 15, 13), b);
            f.set(cx - 7, cy - 6, ui::PLATINUM);
            f.set(cx + 7, cy - 6, ui::PLATINUM);
            f.set(cx - 7, cy + 6, ui::PLATINUM);
            f.set(cx + 7, cy + 6, ui::PLATINUM);
        }
        Tool::ShapeEllipse => {
            for a in 0..48 {
                let t = a as f32 / 48.0 * std::f32::consts::TAU;
                f.set(cx + (t.cos() * 7.0) as i32, cy + (t.sin() * 6.0) as i32, b);
            }
        }
        Tool::ShapePolygon => {
            for i in 0..5 {
                let a1 = i as f32 / 5.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let a2 = (i + 1) as f32 / 5.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                f.line(
                    cx + (a1.cos() * 7.0) as i32,
                    cy + (a1.sin() * 7.0) as i32,
                    cx + (a2.cos() * 7.0) as i32,
                    cy + (a2.sin() * 7.0) as i32,
                    b,
                );
            }
        }
        Tool::ShapeLine => {
            f.line(cx - 7, cy + 6, cx + 7, cy - 6, b);
            f.fill_rect(Rect::new(cx - 8, cy + 5, 3, 3), b);
            f.fill_rect(Rect::new(cx + 6, cy - 7, 3, 3), b);
        }
        Tool::ShapeCustom => {
            for i in 0..10 {
                let a1 = i as f32 / 10.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let a2 =
                    (i + 1) as f32 / 10.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let r1 = if i % 2 == 0 { 7.0 } else { 3.0 };
                let r2 = if (i + 1) % 2 == 0 { 7.0 } else { 3.0 };
                f.line(
                    cx + (a1.cos() * r1) as i32,
                    cy + (a1.sin() * r1) as i32,
                    cx + (a2.cos() * r2) as i32,
                    cy + (a2.sin() * r2) as i32,
                    b,
                );
            }
        }
        Tool::Slice => {
            f.outline_rect(Rect::new(cx - 7, cy - 6, 15, 13), b);
            f.line(cx - 7, cy, cx + 7, cy, b);
            f.line(cx, cy - 6, cx, cy + 6, b);
        }
        Tool::SliceSelect => {
            f.outline_rect(Rect::new(cx - 7, cy - 6, 15, 13), b);
            f.line(cx - 2, cy + 4, cx + 5, cy - 3, b);
            f.fill_rect(Rect::new(cx + 3, cy - 5, 4, 4), b);
        }
        Tool::Notes => {
            f.fill_rect(Rect::new(cx - 6, cy - 7, 12, 14), (255, 214, 64));
            f.outline_rect(Rect::new(cx - 6, cy - 7, 12, 14), b);
            f.line(cx - 3, cy - 3, cx + 3, cy - 3, b);
            f.line(cx - 3, cy, cx + 3, cy, b);
            f.line(cx - 3, cy + 3, cx + 1, cy + 3, b);
        }
        Tool::Hand => {
            // Four fingers with real gaps, a palm, and a thumb.
            for (i, h) in [5i32, 7, 6, 4].iter().enumerate() {
                f.fill_rect(Rect::new(cx - 5 + i as i32 * 3, cy - h, 2, h + 2), b);
            }
            f.fill_rect(Rect::new(cx - 5, cy - 1, 11, 7), b);
            f.fill_rect(Rect::new(cx - 7, cy + 1, 2, 4), b);
        }
        Tool::Zoom => {
            for a in 0..40 {
                let t = a as f32 / 40.0 * std::f32::consts::TAU;
                f.set(
                    cx - 2 + (t.cos() * 5.0) as i32,
                    cy - 2 + (t.sin() * 5.0) as i32,
                    b,
                );
            }
            f.line(cx + 2, cy + 2, cx + 7, cy + 7, b);
            f.line(cx + 3, cy + 2, cx + 7, cy + 6, b);
        }
        Tool::Brush => {
            // An angled paint brush: handle upper-right, ferrule,
            // bristles tapering to the tip, and a paint dab below.
            f.line(cx + 7, cy - 6, cx + 1, cy, b);
            f.line(cx + 6, cy - 7, cx, cy - 1, b);
            f.fill_rect(Rect::new(cx - 2, cy, 3, 3), b);
            for i in 0..4 {
                f.fill_rect(Rect::new(cx - 6 + i, cy + 3 + i, 4 - i, 1), b);
            }
            f.fill_rect(Rect::new(cx - 8, cy + 8, 6, 1), ui::SHADOW);
        }
        Tool::Pen => {
            f.line(cx - 5, cy + 7, cx, cy - 7, b);
            f.line(cx, cy - 7, cx + 5, cy + 7, b);
            f.line(cx - 5, cy + 7, cx + 5, cy + 7, b);
            f.fill_rect(Rect::new(cx - 1, cy, 3, 3), b);
            f.line(cx, cy + 7, cx, cy + 9, b);
        }
        Tool::Pencil => {
            // A single pencil: two body edges, a wood tip with a
            // lead point at lower-left, an eraser cap at upper-right.
            f.line(cx - 3, cy + 3, cx + 4, cy - 4, b);
            f.line(cx - 1, cy + 5, cx + 6, cy - 2, b);
            f.line(cx - 3, cy + 3, cx - 7, cy + 7, b);
            f.line(cx - 1, cy + 5, cx - 7, cy + 7, b);
            f.set(cx - 7, cy + 7, b);
            f.line(cx + 4, cy - 4, cx + 6, cy - 2, b);
            f.fill_rect(Rect::new(cx + 5, cy - 6, 3, 3), b);
        }
        Tool::Eraser => {
            // A slanted block eraser over the surface line it cleans.
            f.line(cx - 5, cy - 1, cx + 2, cy - 6, b);
            f.line(cx + 2, cy - 6, cx + 7, cy - 6, b);
            f.line(cx + 7, cy - 6, cx + 7, cy + 1, b);
            f.line(cx + 7, cy + 1, cx, cy + 6, b);
            f.line(cx, cy + 6, cx - 5, cy + 6, b);
            f.line(cx - 5, cy + 6, cx - 5, cy - 1, b);
            f.line(cx - 5, cy + 2, cx + 7, cy + 2, b);
            f.line(cx - 8, cy + 8, cx + 8, cy + 8, ui::SHADOW);
        }
        Tool::Stamp => {
            // Rubber stamp: round knob, neck, and a wide base.
            for a in 0..32 {
                let t = a as f32 / 32.0 * std::f32::consts::TAU;
                f.set(
                    cx + (t.cos() * 3.0) as i32,
                    cy - 5 + (t.sin() * 3.0) as i32,
                    b,
                );
            }
            f.fill_rect(Rect::new(cx - 1, cy - 2, 3, 3), b);
            f.fill_rect(Rect::new(cx - 3, cy + 1, 7, 2), b);
            f.fill_rect(Rect::new(cx - 6, cy + 3, 13, 3), b);
            f.fill_rect(Rect::new(cx - 6, cy + 7, 13, 1), ui::SHADOW);
        }
        Tool::Bucket => {
            f.line(cx - 6, cy - 4, cx + 3, cy - 4, b);
            f.line(cx - 6, cy - 4, cx - 3, cy + 4, b);
            f.line(cx + 3, cy - 4, cx + 6, cy + 4, b);
            f.line(cx - 3, cy + 4, cx + 6, cy + 4, b);
            f.line(cx - 5, cy - 1, cx + 4, cy - 1, b);
            // The pour: a falling drop at the spout.
            f.fill_rect(Rect::new(cx + 6, cy + 6, 2, 2), b);
            f.set(cx + 6, cy + 8, b);
        }
        Tool::Gradient => {
            for i in 0..14 {
                let v = (255 - i * 17) as u8;
                f.fill_rect(Rect::new(cx - 7 + i, cy - 5, 1, 11), (v, v, v));
            }
            f.outline_rect(Rect::new(cx - 7, cy - 5, 14, 11), b);
        }
        Tool::Blur => {
            // A water droplet: pointed top, round shaded body.
            f.line(cx, cy - 8, cx - 4, cy + 1, b);
            f.line(cx, cy - 8, cx + 4, cy + 1, b);
            for rad in [5, 3] {
                for yy in -rad..=rad {
                    for xx in -rad..=rad {
                        if xx * xx + yy * yy <= rad * rad {
                            f.set(cx + xx, cy + 3 + yy, if rad == 5 { ui::SHADOW } else { b });
                        }
                    }
                }
            }
        }
        Tool::Sharpen => {
            f.line(cx, cy - 7, cx - 6, cy + 6, b);
            f.line(cx, cy - 7, cx + 6, cy + 6, b);
            f.line(cx - 6, cy + 6, cx + 6, cy + 6, b);
        }
        Tool::Smudge => {
            // A fingertip pressing down, with the smear it drags.
            f.fill_rect(Rect::new(cx - 2, cy - 8, 5, 8), b);
            f.fill_rect(Rect::new(cx - 1, cy, 3, 3), b);
            f.line(cx - 7, cy + 5, cx + 1, cy + 5, ui::SHADOW);
            f.line(cx - 4, cy + 7, cx + 4, cy + 7, ui::SHADOW);
        }
        Tool::Dodge => {
            // The darkroom dodging paddle: a disc on a stick, held
            // to keep light off part of the print.
            for yy in -5..=5i32 {
                for xx in -5..=5i32 {
                    let d2 = xx * xx + yy * yy;
                    if (13..=25).contains(&d2) {
                        f.set(cx - 2 + xx, cy - 3 + yy, b);
                    }
                }
            }
            f.line(cx + 1, cy + 1, cx + 6, cy + 7, b);
        }
        Tool::Burn => {
            // A cupped hand shading the print to burn in an area.
            for xx in -6..=6i32 {
                let yy = ((36 - xx * xx) as f32).sqrt() as i32;
                f.set(cx + xx, cy - yy / 2, b);
                f.set(cx + xx, cy + 1 - yy / 2, b);
            }
            f.line(cx - 6, cy + 3, cx - 6, cy + 6, b);
            f.line(cx + 6, cy + 3, cx + 6, cy + 6, b);
            f.line(cx - 6, cy + 6, cx + 6, cy + 6, b);
        }
        Tool::Sponge => {
            // A porous block.
            f.outline_rect(Rect::new(cx - 6, cy - 5, 13, 11), b);
            for (dx, dy) in [(-3, -2), (1, -3), (4, 0), (-1, 1), (-4, 3), (3, 3)] {
                f.set(cx + dx, cy + dy, b);
            }
        }
        Tool::PolyLasso => {
            // Straight-segment loop with square anchors.
            let pts = [
                (cx - 6, cy + 5),
                (cx - 3, cy - 6),
                (cx + 5, cy - 4),
                (cx + 6, cy + 4),
            ];
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, b);
            }
            f.line(pts[3].0, pts[3].1, pts[0].0, pts[0].1, b);
            for p in pts {
                f.fill_rect(Rect::new(p.0 - 1, p.1 - 1, 3, 3), b);
            }
        }
        Tool::MagneticLasso => {
            // Lasso loop plus a small magnet block.
            let pts = [
                (cx - 7, cy + 4),
                (cx - 3, cy - 6),
                (cx + 4, cy - 5),
                (cx + 6, cy + 2),
                (cx + 1, cy + 6),
            ];
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, b);
            }
            f.line(pts[4].0, pts[4].1, pts[0].0, pts[0].1, b);
            f.fill_rect(Rect::new(cx + 3, cy + 3, 3, 5), b);
            f.fill_rect(Rect::new(cx + 6, cy + 3, 3, 5), b);
            f.fill_rect(Rect::new(cx + 3, cy + 6, 6, 2), b);
        }
        Tool::ColorSampler => {
            // Eyedropper over a target ring.
            f.line(cx - 2, cy + 2, cx + 5, cy - 5, b);
            f.fill_rect(Rect::new(cx + 4, cy - 8, 4, 4), b);
            for a in 0..32 {
                let t = a as f32 / 32.0 * std::f32::consts::TAU;
                f.set(
                    cx - 4 + (t.cos() * 4.0) as i32,
                    cy + 4 + (t.sin() * 4.0) as i32,
                    b,
                );
            }
            f.set(cx - 4, cy + 4, b);
        }
        Tool::Measure => {
            // A ruler: long diagonal with end ticks.
            f.line(cx - 7, cy + 7, cx + 7, cy - 7, b);
            f.line(cx - 7, cy + 3, cx - 4, cy + 7, b);
            f.line(cx + 4, cy - 7, cx + 7, cy - 3, b);
            f.set(cx - 2, cy + 2, b);
            f.set(cx + 2, cy - 2, b);
        }
        Tool::HistoryBrush => {
            // Brush with a return arrow arc above.
            f.line(cx + 6, cy + 7, cx + 1, cy + 1, b);
            f.fill_rect(Rect::new(cx - 3, cy + 2, 4, 4), b);
            for a in 0..24 {
                let t = std::f32::consts::FRAC_PI_2 + a as f32 / 24.0 * std::f32::consts::PI;
                f.set(
                    cx + (t.cos() * 6.0) as i32,
                    cy - 3 + (t.sin() * 4.0) as i32,
                    b,
                );
            }
            f.set(cx - 6, cy - 4, b);
            f.set(cx - 6, cy - 2, b);
            f.set(cx - 4, cy - 3, b);
        }
        Tool::ArtHistoryBrush => {
            // Brush with a swirl.
            f.line(cx + 6, cy + 7, cx + 1, cy + 1, b);
            f.fill_rect(Rect::new(cx - 3, cy + 2, 4, 4), b);
            for a in 0..36 {
                let t = a as f32 / 36.0 * std::f32::consts::TAU * 1.5;
                let rr = 1.0 + a as f32 / 36.0 * 4.0;
                f.set(
                    cx - 2 + (t.cos() * rr) as i32,
                    cy - 4 + (t.sin() * rr * 0.7) as i32,
                    b,
                );
            }
        }
        Tool::MagicEraser => {
            // Eraser block with a sparkle.
            f.outline_rect(Rect::new(cx - 7, cy - 1, 11, 7), b);
            f.line(cx + 5, cy - 7, cx + 5, cy - 2, b);
            f.line(cx + 2, cy - 5, cx + 8, cy - 5, b);
            f.set(cx + 6, cy + 4, b);
            f.set(cx - 5, cy + 7, b);
        }
        Tool::BackgroundEraser => {
            // Eraser over a dotted (disappearing) ground.
            f.outline_rect(Rect::new(cx - 6, cy - 7, 11, 7), b);
            for (dx, dy) in [(-5, 3), (-1, 4), (3, 3), (6, 5), (-3, 7), (1, 7)] {
                f.set(cx + dx, cy + dy, b);
            }
        }
        Tool::Heal => {
            // Bandage: a diagonal strip with two dots.
            for t in 0..11 {
                let xx = cx - 5 + t;
                let yy = cy + 5 - t;
                f.fill_rect(Rect::new(xx, yy, 3, 3), b);
            }
            f.set(cx - 1, cy - 1, b);
            f.set(cx + 2, cy + 2, b);
        }
        Tool::Patch => {
            // Dashed selection square with a solid patch corner.
            for t in (0..13).step_by(3) {
                f.fill_rect(Rect::new(cx - 6 + t, cy - 6, 2, 1), b);
                f.fill_rect(Rect::new(cx - 6 + t, cy + 6, 2, 1), b);
                f.fill_rect(Rect::new(cx - 6, cy - 6 + t, 1, 2), b);
                f.fill_rect(Rect::new(cx + 6, cy - 6 + t, 1, 2), b);
            }
            f.fill_rect(Rect::new(cx + 1, cy + 1, 5, 5), b);
        }
        Tool::PatternStamp => {
            // Stamp over a checker patch.
            f.fill_rect(Rect::new(cx - 2, cy - 8, 4, 5), b);
            f.fill_rect(Rect::new(cx - 5, cy - 3, 10, 3), b);
            for (dx, dy) in [(-6, 2), (-2, 2), (2, 6), (-6, 6), (6, 2)] {
                f.fill_rect(Rect::new(cx + dx, cy + dy, 3, 3), b);
            }
            f.outline_rect(Rect::new(cx - 7, cy + 1, 13, 8), b);
        }
        Tool::FreeformPen => {
            // Pen nib with a wavy freehand line.
            f.line(cx - 6, cy - 7, cx - 2, cy - 2, b);
            f.line(cx - 2, cy - 2, cx + 2, cy - 6, b);
            for a in 0..16 {
                let t = a as f32 / 16.0;
                let xx = cx - 7 + (t * 14.0) as i32;
                let yy = cy + 4 + (t * std::f32::consts::TAU * 1.5).sin() as i32 * 2;
                f.set(xx, yy, b);
            }
        }
        Tool::MagneticPen => {
            // Pen nib with anchor dots pulled to a line.
            f.line(cx - 6, cy - 7, cx - 2, cy - 2, b);
            f.line(cx - 2, cy - 2, cx + 2, cy - 6, b);
            f.line(cx - 7, cy + 6, cx + 7, cy + 2, b);
            for p in [(cx - 7, cy + 6), (cx, cy + 4), (cx + 7, cy + 2)] {
                f.fill_rect(Rect::new(p.0 - 1, p.1 - 1, 3, 3), b);
            }
        }
        Tool::Airbrush => {
            // Spray can tilted, with a dotted spray cloud.
            f.line(cx - 7, cy + 7, cx - 1, cy - 1, b);
            f.line(cx - 5, cy + 8, cx + 1, cy + 1, b);
            f.fill_rect(Rect::new(cx - 2, cy - 6, 5, 5), b);
            for (dx, dy) in [(4, -6), (7, -3), (5, 0), (8, -7), (3, -3)] {
                f.set(cx + dx, cy + dy, b);
            }
        }
        Tool::ConvertAnchor => {
            // Anchor square with one handle pulled out.
            f.fill_rect(Rect::new(cx - 2, cy - 2, 4, 4), b);
            f.line(cx, cy, cx + 7, cy - 6, b);
            f.fill_rect(Rect::new(cx + 6, cy - 7, 3, 3), b);
            f.line(cx - 7, cy + 5, cx, cy, b);
        }
        Tool::Type => {
            // A capital T.
            f.fill_rect(Rect::new(cx - 6, cy - 7, 13, 2), b);
            f.fill_rect(Rect::new(cx - 1, cy - 7, 3, 14), b);
            f.fill_rect(Rect::new(cx - 3, cy + 6, 7, 2), b);
        }
        Tool::TypeMask => {
            // A T inside a dashed selection outline.
            f.fill_rect(Rect::new(cx - 3, cy - 6, 7, 2), b);
            f.fill_rect(Rect::new(cx - 1, cy - 6, 3, 12), b);
            for x in (cx - 8)..(cx + 9) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy - 8, b);
                    f.set(x, cy + 8, b);
                }
            }
            for y in (cy - 8)..(cy + 9) {
                if (y / 3) % 2 == 0 {
                    f.set(cx - 8, y, b);
                    f.set(cx + 8, y, b);
                }
            }
        }
        Tool::VertTypeMask => {
            // A sideways T inside a dashed outline.
            f.fill_rect(Rect::new(cx - 6, cy - 3, 2, 7), b);
            f.fill_rect(Rect::new(cx - 6, cy - 1, 10, 3), b);
            for x in (cx - 8)..(cx + 9) {
                if (x / 3) % 2 == 0 {
                    f.set(x, cy - 8, b);
                    f.set(x, cy + 8, b);
                }
            }
            for y in (cy - 8)..(cy + 9) {
                if (y / 3) % 2 == 0 {
                    f.set(cx - 8, y, b);
                    f.set(cx + 8, y, b);
                }
            }
        }
        Tool::VertType => {
            // A T on its side with a downward arrow.
            f.fill_rect(Rect::new(cx - 7, cy - 6, 2, 13), b);
            f.fill_rect(Rect::new(cx - 7, cy - 1, 12, 3), b);
            f.set(cx + 6, cy + 5, b);
            f.set(cx + 5, cy + 6, b);
            f.set(cx + 7, cy + 6, b);
            f.set(cx + 6, cy + 7, b);
        }
    }
}

impl App {
    /// Y coordinate where the work area starts: menu bar
    /// (hidden in full screen), plus the Options Bar in
    /// standard mode (Phase 6.5 Batch 5).
    pub(crate) fn top_y(&self) -> i32 {
        let menu = if self.display_mode < 2 { ui::MENU_H } else { 0 };
        let options = if self.show_options_bar && self.display_mode == 0 {
            ui::OPTIONS_H
        } else {
            0
        };
        let tabs = if self.display_mode == 0 && self.tab_count() > 1 {
            TAB_H
        } else {
            0
        };
        menu + options + tabs
    }

    pub(crate) fn canvas_rect(&self, win_w: u32, win_h: u32) -> Rect {
        let (rx, ry) = if self.show_rulers && self.display_mode == 0 {
            (ui::RULER_W, ui::RULER_H)
        } else {
            (0, 0)
        };
        let dock = if self.dock_open() && self.display_mode == 0 {
            DOCK_W
        } else {
            0
        };
        let toolbar = if self.display_mode == 0 {
            ui::TOOLBAR_W
        } else {
            0
        };
        let status = if self.display_mode == 0 {
            ui::STATUS_H
        } else {
            0
        };
        Rect::new(
            toolbar + rx,
            self.top_y() + ry,
            (win_w as i32 - toolbar - rx - dock).max(10),
            (win_h as i32 - self.top_y() - status - ry).max(10),
        )
    }

    pub(crate) fn dock_open(&self) -> bool {
        self.show_cp
            || self.show_brushes
            || self.show_histogram
            || self.show_palette
            || self.show_navigator
            || self.show_actions
    }

    pub(crate) fn fit_view(&mut self, win_w: u32, win_h: u32) {
        let c = self.canvas_rect(win_w, win_h);
        self.viewport = Viewport::fit(
            self.doc.width as f32,
            self.doc.height as f32,
            c.w as f32,
            c.h as f32,
        );
    }

    pub(crate) fn draw(&mut self, f: &mut Frame) {
        self.sync_panel_slots();
        self.draw_canvas(f);
        if self.show_rulers && self.display_mode == 0 {
            self.draw_rulers(f);
        }
        if self.display_mode == 0 {
            self.draw_toolbox(f);
        }
        if self.dock_open() && self.display_mode == 0 {
            f.fill_rect(
                Rect::new(
                    f.w as i32 - DOCK_W,
                    self.top_y(),
                    DOCK_W,
                    f.h as i32 - self.top_y() - ui::STATUS_H,
                ),
                ui::PLATINUM,
            );
        }
        if self.show_palette {
            self.draw_palette(f);
        }
        if self.show_brushes {
            self.draw_brushes(f);
        }
        if self.show_tool_presets {
            self.draw_tool_presets(f);
        }
        if self.show_histogram {
            self.draw_histogram_panel(f);
        }
        if self.show_cp {
            self.draw_channels_paths(f);
        }
        if self.show_navigator {
            self.draw_navigator(f);
        }
        if self.show_actions {
            self.draw_actions_panel(f);
        }
        if self.show_history {
            self.draw_history_panel(f);
        }
        if self.show_info {
            self.draw_info_panel(f);
        }
        if self.display_mode == 0 {
            self.draw_panel_strips(f);
        }
        if self.display_mode == 0 {
            if self.show_options_bar {
                self.draw_options_bar(f);
            }
            self.draw_tab_bar(f);
            self.draw_status(f);
        }
        // Menus and the preset popup paint after the strips they
        // open over (options bar, tab bar), so a dropdown is never
        // half-covered by the strip below it.
        if self.display_mode < 2 {
            self.draw_menu_bar(f);
        }
        if self.display_mode == 0 && self.show_options_bar && self.tool_preset_popup {
            self.draw_tool_preset_popup(f);
        }
        if self.file_panel.is_some() {
            self.draw_file_panel(f);
        }
        if self.dialog.is_some() {
            self.draw_dialog(f);
        }
        if self.context_menu.is_some() {
            self.draw_context_menu(f);
        }
        // Hover tooltip: show after a short hover, anchored near
        // the cursor; always on top.
        if self.tooltip.is_none() {
            if let (Some(text), Some(since)) = (&self.hover_text, self.hover_since) {
                if since.elapsed().as_millis() >= 600 {
                    let (x, y) = self.cursor;
                    self.tooltip = Some((text.clone(), x as i32 + 14, y as i32 + 18));
                }
            }
        }
        if let Some((text, tx, ty)) = self.tooltip.clone() {
            self.draw_tooltip(f, &text, tx, ty);
        }
    }

    /// Tooltip bubble at (tx, ty): rounded box, light text.
    pub(crate) fn draw_tooltip(&self, f: &mut Frame, text: &str, tx: i32, ty: i32) {
        let w = Frame::text_width(text, 1) + 16;
        let h = 20;
        let x = tx.min(self.win_size.0 as i32 - w - 4).max(4);
        let y = ty.min(self.win_size.1 as i32 - h - 4).max(4);
        f.panel_rounded(Rect::new(x, y, w, h), 4);
        f.text(x + 8, y + 5, text, 1, ui::TEXT);
    }

    /// Track the hover target for tooltips: a new target
    /// restarts the delay, a cleared target hides the bubble.
    pub(crate) fn set_hover(&mut self, text: Option<String>) {
        if text != self.hover_text {
            self.hover_text = text;
            self.hover_since = self.hover_text.as_ref().map(|_| std::time::Instant::now());
            self.tooltip = None;
        }
    }

    pub(crate) fn ensure_render_cache(&mut self) {
        if self.cached_size != (self.doc.width, self.doc.height) || self.cached_render.is_empty() {
            self.cached_render = self.doc.render_full_rgba8();
            self.cached_size = (self.doc.width, self.doc.height);
            self.rendered_version = self.doc_version;
        } else if self.rendered_version != self.doc_version {
            self.cached_render = self.doc.render_full_rgba8();
            self.rendered_version = self.doc_version;
        } else {
            return;
        }
        // Soft proof: run the fresh render through a CMYK round
        // trip so out-of-gamut colors clip the way print will.
        if self.proof_cmyk && self.doc.mode == ColorMode::Rgb {
            for px in self.cached_render.as_chunks_mut::<4>().0 {
                let (r, g, b) = (
                    px[0] as f32 / 255.0,
                    px[1] as f32 / 255.0,
                    px[2] as f32 / 255.0,
                );
                let (c, m, y, k) = pictodesk_core::color::rgb_to_cmyk(r, g, b);
                let (r2, g2, b2) = pictodesk_core::color::cmyk_to_rgb(c, m, y, k);
                px[0] = (r2 * 255.0).round() as u8;
                px[1] = (g2 * 255.0).round() as u8;
                px[2] = (b2 * 255.0).round() as u8;
            }
        }
        if self.gamut_warning && self.doc.mode == ColorMode::Rgb {
            for px in self.cached_render.as_chunks_mut::<4>().0 {
                if px[3] == 0 {
                    continue;
                }
                let (r, g, b) = (
                    px[0] as f32 / 255.0,
                    px[1] as f32 / 255.0,
                    px[2] as f32 / 255.0,
                );
                let (c, m, y, k) = pictodesk_core::color::rgb_to_cmyk(r, g, b);
                // Press-gamut heuristic on the separation itself:
                // total ink over a 280% TAC limit is unprintable,
                // and a color needing two heavy inks (>= 75%) is a
                // high-chroma secondary outside press gamut. The
                // printable primaries (one heavy ink) stay clean.
                let heavy = [c, m, y].iter().filter(|v| **v >= 0.75).count();
                if c + m + y + k > 2.8 || heavy >= 2 {
                    px[0] = 160;
                    px[1] = 160;
                    px[2] = 160;
                }
            }
        }
    }

    /// Classic rulers along the top and left of the canvas:
    /// pixel coordinates in document space, tick spacing adapts to
    /// zoom, and a marker tracks the cursor on each ruler.
    pub(crate) fn draw_rulers(&self, f: &mut Frame) {
        let c = self.canvas_rect(f.w, f.h);
        let ty = self.top_y();
        let top = Rect::new(ui::TOOLBAR_W, ty, c.right() - ui::TOOLBAR_W, ui::RULER_H);
        let left = Rect::new(ui::TOOLBAR_W, ty, ui::RULER_W, c.bottom() - ty);
        f.fill_rect(top, ui::PLATINUM);
        f.fill_rect(left, ui::PLATINUM);
        f.fill_rect(
            Rect::new(ui::TOOLBAR_W, ty, ui::RULER_W, ui::RULER_H),
            ui::PLATINUM_LIGHT,
        );
        f.fill_rect(
            Rect::new(
                ui::TOOLBAR_W,
                ty + ui::RULER_H - 1,
                c.right() - ui::TOOLBAR_W,
                1,
            ),
            ui::DARK,
        );
        f.fill_rect(
            Rect::new(ui::TOOLBAR_W + ui::RULER_W - 1, ty, 1, c.bottom() - ty),
            ui::DARK,
        );
        let scale = self.viewport.scale;
        let steps = [1i32, 2, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000];
        let step = steps
            .iter()
            .copied()
            .find(|s| *s as f32 * scale >= 48.0)
            .unwrap_or(10000);
        // Horizontal ticks + labels.
        let d0 = self.viewport.screen_to_doc((0.0, 0.0)).0;
        let mut p = (d0 / step as f32).floor() as i32 * step;
        loop {
            let sx = c.x + (p as f32 * scale + self.viewport.offset.0) as i32;
            if sx > c.right() {
                break;
            }
            if sx >= c.x {
                f.fill_rect(Rect::new(sx, ty + ui::RULER_H - 6, 1, 6), ui::DARK);
                f.text(sx + 3, ty + 3, &p.to_string(), 1, ui::TEXT);
            }
            p += step;
        }
        // Vertical ticks + labels.
        let e0 = self.viewport.screen_to_doc((0.0, 0.0)).1;
        let mut q = (e0 / step as f32).floor() as i32 * step;
        loop {
            let sy = c.y + (q as f32 * scale + self.viewport.offset.1) as i32;
            if sy > c.bottom() {
                break;
            }
            if sy >= c.y {
                f.fill_rect(
                    Rect::new(ui::TOOLBAR_W + ui::RULER_W - 6, sy, 6, 1),
                    ui::DARK,
                );
                // Vertical labels read bottom-to-top, rotated like
                // the classic rulers, centered on their tick.
                let label = q.to_string();
                let lw = Frame::text_width(&label, 1);
                f.text_vertical(ui::TOOLBAR_W + 5, sy + lw / 2, &label, 1, ui::TEXT);
            }
            q += step;
        }
        // Cursor markers.
        let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
        if c.contains(mx, my) {
            f.fill_rect(Rect::new(mx, ty + ui::RULER_H - 9, 1, 9), ui::TEXT_DIM);
            f.fill_rect(
                Rect::new(ui::TOOLBAR_W + ui::RULER_W - 9, my, 9, 1),
                ui::BLACK,
            );
        }
    }

    /// The on-screen color for one document pixel when a single
    /// channel is being viewed (grayscale, or tinted for spot).
    pub(crate) fn view_pixel(&self, dx: i32, dy: i32, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        match self.channel_view {
            ChannelView::Composite => (r, g, b),
            ChannelView::Channel(ci) => {
                let v = match self.doc.mode {
                    ColorMode::Rgb | ColorMode::Indexed => [r, g, b][ci.min(2)],
                    ColorMode::Grayscale | ColorMode::Bitmap | ColorMode::Duotone => {
                        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) as u8
                    }
                    ColorMode::Lab => {
                        let (l, a, b, _) = self.doc.render_lab_pixel(dx, dy);
                        match ci {
                            0 => (l * 2.55) as u8,
                            1 => (a + 128.0) as u8,
                            _ => (b + 128.0) as u8,
                        }
                    }
                    ColorMode::Multichannel => {
                        let plane = self.doc.multi_planes.get(ci);
                        let v16 = plane
                            .and_then(|p| {
                                p.get((dy as usize) * self.doc.width as usize + dx as usize)
                            })
                            .copied()
                            .unwrap_or(0);
                        255 - ((v16 as u32 + 128) / 257) as u8
                    }
                    ColorMode::Cmyk => {
                        // The stored inks themselves (the document
                        // holds real CMYK values now).
                        let (c, m, y, k) = self.doc.render_cmyk_pixel(dx, dy);
                        let d = |v: u16| ((v as u32 + 128) / 257) as u8;
                        255 - [d(c), d(m), d(y), d(k)][ci.min(3)]
                    }
                };
                (v, v, v)
            }
            ChannelView::Alpha(ai) => {
                let Some(ch) = self.doc.alphas.get(ai) else {
                    return (r, g, b);
                };
                let v = ch.value(self.doc.width, dx, dy);
                match ch.spot_color {
                    Some((sr, sg, sb)) => (
                        255 - ((255 - sr) as u32 * v as u32 / 255) as u8,
                        255 - ((255 - sg) as u32 * v as u32 / 255) as u8,
                        255 - ((255 - sb) as u32 * v as u32 / 255) as u8,
                    ),
                    None => (v, v, v),
                }
            }
            ChannelView::Mask(li) => {
                let Some(l) = self.doc.layers.get(li) else {
                    return (r, g, b);
                };
                let v = l.mask_value(dx, dy);
                (v, v, v)
            }
        }
    }

    pub(crate) fn draw_canvas(&mut self, f: &mut Frame) {
        self.ensure_render_cache();
        let c = self.canvas_rect(f.w, f.h);
        f.fill_rect(c, ui::CANVAS_SURROUND);
        let origin = (
            c.x as f32 + self.viewport.offset.0,
            c.y as f32 + self.viewport.offset.1,
        );
        let scale = self.viewport.scale;
        let dw = (self.doc.width as f32 * scale) as i32;
        let dh = (self.doc.height as f32 * scale) as i32;
        let doc_rect = Rect::new(origin.0 as i32, origin.1 as i32, dw, dh);
        // clip doc rect to canvas
        let x0 = doc_rect.x.max(c.x);
        let y0 = doc_rect.y.max(c.y);
        let x1 = doc_rect.right().min(c.right());
        let y1 = doc_rect.bottom().min(c.bottom());
        let (docw, doch) = (self.doc.width as i32, self.doc.height as i32);
        for sy in y0..y1 {
            for sx in x0..x1 {
                let dx = ((sx - doc_rect.x) as f32 / scale) as i32;
                let dy = ((sy - doc_rect.y) as f32 / scale) as i32;
                if dx < 0 || dy < 0 || dx >= docw || dy >= doch {
                    continue;
                }
                let i = ((dy * docw + dx) * 4) as usize;
                let (r, g, b, a) = (
                    self.cached_render[i],
                    self.cached_render[i + 1],
                    self.cached_render[i + 2],
                    self.cached_render[i + 3],
                );
                if self.channel_view != ChannelView::Composite {
                    let v = self.view_pixel(dx, dy, r, g, b);
                    f.set(sx, sy, v);
                    continue;
                }
                let shown = if a == 255 {
                    (r, g, b)
                } else {
                    let cell = (((sx / 8) + (sy / 8)) % 2 == 0) as u8;
                    let bgc = if cell == 0 { 255 } else { 192 };
                    let af = a as f32 / 255.0;
                    (
                        (r as f32 * af + bgc as f32 * (1.0 - af)) as u8,
                        (g as f32 * af + bgc as f32 * (1.0 - af)) as u8,
                        (b as f32 * af + bgc as f32 * (1.0 - af)) as u8,
                    )
                };
                // Quick Mask rubylith: red over the areas the
                // mask leaves unselected (Phase 6.5).
                let shown = if self.quick_mask {
                    match self.doc.alphas.iter().find(|c| c.name == "Quick Mask") {
                        Some(ch) => {
                            let v = ch.value(self.doc.width, dx, dy) as f32 / 255.0;
                            let cov = (1.0 - v) * 0.5;
                            (
                                (shown.0 as f32 + (255.0 - shown.0 as f32) * cov) as u8,
                                (shown.1 as f32 * (1.0 - cov)) as u8,
                                (shown.2 as f32 * (1.0 - cov)) as u8,
                            )
                        }
                        None => shown,
                    }
                } else {
                    shown
                };
                f.set(sx, sy, shown);
            }
        }
        // drop shadow edge, like a sheet of paper on the surround
        f.fill_rect(
            Rect::new(doc_rect.x + 3, doc_rect.bottom(), doc_rect.w, 3),
            (70, 70, 70),
        );
        f.fill_rect(
            Rect::new(doc_rect.right(), doc_rect.y + 3, 3, doc_rect.h),
            (70, 70, 70),
        );

        // Guides and grid (Phase 4), drawn over the document.
        if self.show_grid {
            let mut step = 64i32;
            while (step as f32 * scale) < 10.0 {
                step *= 2;
            }
            let gc = (176, 176, 176);
            let start_x = ((-self.viewport.offset.0) / scale).floor() as i32;
            let mut wx = start_x - start_x.rem_euclid(step);
            while (wx as f32) < (self.doc.width as f32) {
                let sx = doc_rect.x + (wx as f32 * scale) as i32;
                if sx >= c.x && sx < c.right() {
                    f.fill_rect(Rect::new(sx, y0.max(c.y), 1, y1 - y0.max(c.y)), gc);
                }
                wx += step;
            }
            let start_y = ((-self.viewport.offset.1) / scale).floor() as i32;
            let mut wy = start_y - start_y.rem_euclid(step);
            while (wy as f32) < (self.doc.height as f32) {
                let sy = doc_rect.y + (wy as f32 * scale) as i32;
                if sy >= c.y && sy < c.bottom() {
                    f.fill_rect(Rect::new(x0.max(c.x), sy, x1 - x0.max(c.x), 1), gc);
                }
                wy += step;
            }
        }
        if self.show_guides {
            let gc = (64, 156, 255);
            for g in &self.doc.guides_x {
                let sx = doc_rect.x + (*g as f32 * scale) as i32;
                if sx >= c.x && sx < c.right() {
                    f.fill_rect(Rect::new(sx, c.y, 1, c.h), gc);
                }
            }
            for g in &self.doc.guides_y {
                let sy = doc_rect.y + (*g as f32 * scale) as i32;
                if sy >= c.y && sy < c.bottom() {
                    f.fill_rect(Rect::new(c.x, sy, c.w, 1), gc);
                }
            }
            if let Some((vertical, pos, _)) = self.guide_drag {
                if vertical {
                    let sx = doc_rect.x + (pos as f32 * scale) as i32;
                    f.fill_rect(Rect::new(sx, c.y, 1, c.h), (255, 64, 64));
                } else {
                    let sy = doc_rect.y + (pos as f32 * scale) as i32;
                    f.fill_rect(Rect::new(c.x, sy, c.w, 1), (255, 64, 64));
                }
            }
        }

        // Phase 6 overlays: slices (numbered outlines) + notes.
        if !self.doc.slices.is_empty() {
            for (i, sl) in self.doc.slices.iter().enumerate() {
                let tl = self.viewport.doc_to_screen((sl.x as f32, sl.y as f32));
                let br = self
                    .viewport
                    .doc_to_screen(((sl.x + sl.w as i32) as f32, (sl.y + sl.h as i32) as f32));
                let r = Rect::new(
                    c.x + tl.0 as i32,
                    c.y + tl.1 as i32,
                    (br.0 - tl.0).max(1.0) as i32,
                    (br.1 - tl.1).max(1.0) as i32,
                );
                let col = if self.slice_selected == Some(i) {
                    (255, 64, 64)
                } else {
                    (64, 156, 255)
                };
                f.outline_rect(r, col);
                f.text(r.x + 2, r.y + 2, &format!("{}", i + 1), 1, col);
            }
        }
        for n in &self.doc.notes {
            let sp = self.viewport.doc_to_screen((n.x as f32, n.y as f32));
            let r = Rect::new(c.x + sp.0 as i32 - 5, c.y + sp.1 as i32 - 5, 10, 10);
            f.fill_rect(r, (255, 214, 64));
            f.outline_rect(r, ui::BLACK);
            f.text(r.x + 2, r.y + 1, "N", 1, ui::TEXT);
            if n.audio.is_some() {
                // Audio note: a play triangle in the corner.
                for t in 0..4 {
                    f.fill_rect(Rect::new(r.x + 5, r.y + 5 + t, 4 - t, 1), ui::BLACK);
                }
            }
        }
        // Perspective Crop quad + corner handles.
        if let Some(quad) = self.persp_crop {
            let pts: Vec<(i32, i32)> = quad
                .iter()
                .map(|q| {
                    let sp = self.viewport.doc_to_screen(*q);
                    (c.x + sp.0 as i32, c.y + sp.1 as i32)
                })
                .collect();
            for i in 0..4 {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % 4];
                f.line(ax, ay, bx, by, (255, 64, 64));
            }
            for (px, py) in pts {
                f.fill_rect(Rect::new(px - 4, py - 4, 8, 8), (255, 64, 64));
                f.outline_rect(Rect::new(px - 4, py - 4, 8, 8), ui::WHITE);
            }
        }

        // selection marching ants on the selection bounds
        if self.show_edges {
            if let Some(sel) = &self.sel {
                if let Some((bx, by, bw, bh)) = sel.bounds() {
                    let tl = self.viewport.doc_to_screen((bx as f32, by as f32));
                    let br = self
                        .viewport
                        .doc_to_screen(((bx + bw as i32) as f32, (by + bh as i32) as f32));
                    let r = Rect::new(
                        c.x + tl.0 as i32,
                        c.y + tl.1 as i32,
                        (br.0 - tl.0) as i32,
                        (br.1 - tl.1) as i32,
                    );
                    f.marching_ants_rect(r, self.ants_phase);
                }
            }
        }
        // drag previews
        match self.drag {
            DragMode::SelectRect => {
                let a = self.viewport.doc_to_screen(self.drag_start_doc);
                let b = self.viewport.doc_to_screen(self.drag_cur_doc);
                let r = Rect::new(
                    c.x + a.0.min(b.0) as i32,
                    c.y + a.1.min(b.1) as i32,
                    (a.0 - b.0).abs() as i32,
                    (a.1 - b.1).abs() as i32,
                );
                f.marching_ants_rect(r, self.ants_phase);
            }
            DragMode::Lasso => {
                let pts: Vec<(i32, i32)> = self
                    .lasso_points
                    .iter()
                    .map(|p| {
                        let s = self.viewport.doc_to_screen(*p);
                        (c.x + s.0 as i32, c.y + s.1 as i32)
                    })
                    .collect();
                for w in pts.windows(2) {
                    f.line(w[0].0, w[0].1, w[1].0, w[1].1, ui::WHITE);
                }
            }
            DragMode::GradientLine => {
                let a = self.viewport.doc_to_screen(self.drag_start_doc);
                let b = self.viewport.doc_to_screen(self.drag_cur_doc);
                f.line(
                    c.x + a.0 as i32,
                    c.y + a.1 as i32,
                    c.x + b.0 as i32,
                    c.y + b.1 as i32,
                    ui::WHITE,
                );
            }
            _ => {}
        }
        // Color sampler targets (numbered, as in the classic).
        for (i, (sx, sy)) in self.doc.samplers.iter().enumerate() {
            let s = self.viewport.doc_to_screen((*sx as f32, *sy as f32));
            let (px, py) = (c.x + s.0 as i32, c.y + s.1 as i32);
            f.outline_rect(Rect::new(px - 5, py - 5, 11, 11), ui::WHITE);
            f.text(px + 7, py - 5, &format!("{}", i + 1), 1, ui::WHITE);
        }
        // Measure line.
        if let Some(((x0, y0), (x1, y1))) = self.measure {
            let a = self.viewport.doc_to_screen((x0, y0));
            let b = self.viewport.doc_to_screen((x1, y1));
            f.line(
                c.x + a.0 as i32,
                c.y + a.1 as i32,
                c.x + b.0 as i32,
                c.y + b.1 as i32,
                ui::WHITE,
            );
        }
        // Polygonal / Magnetic Lasso in progress.
        if !self.poly_points.is_empty() {
            let pts: Vec<(i32, i32)> = self
                .poly_points
                .iter()
                .map(|p| {
                    let s = self.viewport.doc_to_screen(*p);
                    (c.x + s.0 as i32, c.y + s.1 as i32)
                })
                .collect();
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, ui::WHITE);
            }
            for p in &pts {
                f.fill_rect(Rect::new(p.0 - 2, p.1 - 2, 5, 5), ui::WHITE);
            }
            // Rubber band to the cursor.
            if let Some(last) = pts.last() {
                let cur = self
                    .viewport
                    .doc_to_screen(self.to_doc(self.cursor.0, self.cursor.1));
                f.line(
                    last.0,
                    last.1,
                    c.x + cur.0 as i32,
                    c.y + cur.1 as i32,
                    ui::PLATINUM,
                );
            }
        }
        // Freeform Pen drag preview.
        if self.drag == DragMode::FreePen && self.pen_free.len() > 1 {
            let pts: Vec<(i32, i32)> = self
                .pen_free
                .iter()
                .map(|p| {
                    let s = self.viewport.doc_to_screen(*p);
                    (c.x + s.0 as i32, c.y + s.1 as i32)
                })
                .collect();
            for w in pts.windows(2) {
                f.line(w[0].0, w[0].1, w[1].0, w[1].1, ui::WHITE);
            }
        }
        // Free Transform box: outline + 8 handles.
        if let Some(ft) = &self.ft {
            let handles = Self::ft_handles(ft);
            let to_scr = |p: (f32, f32)| {
                let s = self.viewport.doc_to_screen(p);
                (c.x + s.0 as i32, c.y + s.1 as i32)
            };
            let corners = [
                to_scr(handles[0]),
                to_scr(handles[1]),
                to_scr(handles[2]),
                to_scr(handles[3]),
            ];
            for i in 0..4 {
                let a = corners[i];
                let b = corners[(i + 1) % 4];
                f.line(a.0, a.1, b.0, b.1, ui::WHITE);
            }
            for h in &handles {
                let hs = to_scr(*h);
                let r = Rect::new(hs.0 - 3, hs.1 - 3, 7, 7);
                f.fill_rect(r, ui::WHITE);
                f.outline_rect(r, ui::BLACK);
            }
        }
        // path overlay: the active path (work or selected saved path)
        if self.show_path {
            if let Some((_, path)) = self.active_path_clone() {
                let to_scr = |p: (f32, f32)| {
                    let s = self.viewport.doc_to_screen(p);
                    (c.x + s.0 as i32, c.y + s.1 as i32)
                };
                let ink = (50, 50, 190);
                for sub in &path.subpaths {
                    let pts = sub.flatten();
                    for w in pts.windows(2) {
                        let a = to_scr(w[0]);
                        let b = to_scr(w[1]);
                        f.line(a.0, a.1, b.0, b.1, ink);
                    }
                    if sub.closed && pts.len() > 2 {
                        let a = to_scr(*pts.last().unwrap());
                        let b = to_scr(pts[0]);
                        f.line(a.0, a.1, b.0, b.1, ink);
                    }
                }
                if matches!(self.tool, Tool::Pen | Tool::ConvertAnchor) {
                    for (si, sub) in path.subpaths.iter().enumerate() {
                        for (ai, a) in sub.anchors.iter().enumerate() {
                            let pnt = to_scr(a.point);
                            if self.pen_anchor == Some((si, ai)) && !a.is_corner() {
                                let ic = to_scr(a.in_ctrl);
                                let oc = to_scr(a.out_ctrl);
                                f.line(pnt.0, pnt.1, ic.0, ic.1, ui::SHADOW);
                                f.line(pnt.0, pnt.1, oc.0, oc.1, ui::SHADOW);
                                f.fill_rect(Rect::new(ic.0 - 2, ic.1 - 2, 4, 4), ink);
                                f.fill_rect(Rect::new(oc.0 - 2, oc.1 - 2, 4, 4), ink);
                            }
                            f.fill_rect(Rect::new(pnt.0 - 3, pnt.1 - 3, 6, 6), ui::WHITE);
                            f.outline_rect(Rect::new(pnt.0 - 3, pnt.1 - 3, 6, 6), ink);
                        }
                    }
                }
            }
        }
        // Brush cursor ring + clone/heal source crosshair (modern
        // feedback: the ring shows the tip size, the crosshair shows
        // exactly where the source is sampled from).
        if self.dialog.is_none() && self.open_menu.is_none() {
            let (mx, my) = (self.cursor.0 as i32, self.cursor.1 as i32);
            if c.contains(mx, my) {
                let paint_tool = matches!(
                    self.tool,
                    Tool::Brush
                        | Tool::Airbrush
                        | Tool::Pencil
                        | Tool::Eraser
                        | Tool::Stamp
                        | Tool::PatternStamp
                        | Tool::Heal
                        | Tool::HistoryBrush
                        | Tool::ArtHistoryBrush
                        | Tool::BackgroundEraser
                        | Tool::Blur
                        | Tool::Sharpen
                        | Tool::Smudge
                        | Tool::Dodge
                        | Tool::Burn
                        | Tool::Sponge
                );
                if paint_tool {
                    // White ring with a black outer ring so it reads
                    // on light and dark image areas alike.
                    let r = ((self.tip.size * scale / 2.0) as i32).max(2);
                    let steps = (r * 4).clamp(24, 128);
                    for a in 0..steps {
                        let t = a as f32 / steps as f32 * std::f32::consts::TAU;
                        let (dx, dy) = (t.cos() * r as f32, t.sin() * r as f32);
                        f.set(mx + dx as i32, my + dy as i32, ui::WHITE);
                        let (dx2, dy2) = (t.cos() * (r + 1) as f32, t.sin() * (r + 1) as f32);
                        f.set(mx + dx2 as i32, my + dy2 as i32, ui::BLACK);
                    }
                }
                // Clone / heal source crosshair: during a stroke the
                // source tracks the cursor by the stroke offset;
                // while hovering it sits at the set source point.
                let src = match self.tool {
                    Tool::Stamp => self.clone_src,
                    Tool::Heal if !self.heal_use_pattern => self.heal_src,
                    _ => None,
                };
                if let Some(src) = src {
                    let dp = self.to_doc(self.cursor.0, self.cursor.1);
                    let aligned = match &self.stroke {
                        Some(st) if self.drag == DragMode::Paint => (
                            src.0 + (dp.0 - st.start_point.0),
                            src.1 + (dp.1 - st.start_point.1),
                        ),
                        _ => src,
                    };
                    let sp = self.viewport.doc_to_screen(aligned);
                    let (sx, sy) = (c.x + sp.0 as i32, c.y + sp.1 as i32);
                    if (0..c.w).contains(&(sx - c.x)) && (0..c.h).contains(&(sy - c.y)) {
                        let r = 6;
                        f.circle_outline(sx, sy, r, ui::ACCENT);
                        f.line(sx - r - 3, sy, sx - r + 1, sy, ui::ACCENT);
                        f.line(sx + r - 1, sy, sx + r + 3, sy, ui::ACCENT);
                        f.line(sx, sy - r - 3, sx, sy - r + 1, ui::ACCENT);
                        f.line(sx, sy + r - 1, sx, sy + r + 3, ui::ACCENT);
                    }
                }
            }
        }
    }

    pub(crate) fn tool_rect(i: usize, top: i32) -> Rect {
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        Rect::new(3 + col * 25, top + 4 + row * 25, 24, 24)
    }

    /// The Options Bar rect (Phase 6.5 Batch 5): directly
    /// under the menu bar, standard mode only.
    pub(crate) fn options_bar_rect(&self) -> Option<Rect> {
        if self.display_mode == 0 && self.show_options_bar {
            Some(Rect::new(
                0,
                ui::MENU_H,
                self.win_size.0 as i32,
                ui::OPTIONS_H,
            ))
        } else {
            None
        }
    }

    /// The bar's controls for the active tool, as
    /// (rect, label, selected, action). Steppers render as a
    /// label, a value button and - / + buttons; the value
    /// button itself carries the first action.
    pub(crate) fn options_items(&self) -> Vec<(Rect, String, bool, OptAct)> {
        let Some(bar) = self.options_bar_rect() else {
            return Vec::new();
        };
        let mut out: Vec<(Rect, String, bool, OptAct)> = Vec::new();
        let y = bar.y + 4;
        let mut x = bar.x + 118;
        let stepper = |out: &mut Vec<(Rect, String, bool, OptAct)>,
                       x: &mut i32,
                       label: &str,
                       value: String,
                       down: OptAct,
                       up: OptAct| {
            // label text is drawn by draw_options_bar from the
            // item order; here we emit value + steppers and
            // advance x past the label slot (56px).
            *x += 56;
            out.push((Rect::new(*x, y, 52, 18), value, false, down));
            *x += 54;
            out.push((Rect::new(*x, y, 16, 18), "-".into(), false, down));
            *x += 18;
            out.push((Rect::new(*x, y, 16, 18), "+".into(), false, up));
            *x += 26;
            let _ = label;
        };
        let toggle = |out: &mut Vec<(Rect, String, bool, OptAct)>,
                      x: &mut i32,
                      label: String,
                      selected: bool,
                      act: OptAct,
                      w: i32| {
            out.push((Rect::new(*x, y, w, 18), label, selected, act));
            *x += w + 8;
        };
        let paint = matches!(
            self.tool,
            Tool::Brush
                | Tool::Airbrush
                | Tool::Pencil
                | Tool::Stamp
                | Tool::PatternStamp
                | Tool::HistoryBrush
                | Tool::ArtHistoryBrush
                | Tool::Eraser
                | Tool::Blur
                | Tool::Sharpen
                | Tool::Smudge
        );
        if paint {
            stepper(
                &mut out,
                &mut x,
                "Size",
                format!("{:.0} px", self.tip.size),
                OptAct::TipSizeDown,
                OptAct::TipSizeUp,
            );
            stepper(
                &mut out,
                &mut x,
                "Hard",
                format!("{:.0}%", self.tip.hardness * 100.0),
                OptAct::TipHardDown,
                OptAct::TipHardUp,
            );
            if matches!(
                self.tool,
                Tool::Brush
                    | Tool::Airbrush
                    | Tool::Pencil
                    | Tool::Heal
                    | Tool::Stamp
                    | Tool::PatternStamp
            ) {
                stepper(
                    &mut out,
                    &mut x,
                    "Flow",
                    format!("{:.0}%", self.flow * 100.0),
                    OptAct::FlowDown,
                    OptAct::FlowUp,
                );
            }
        }
        match self.tool {
            Tool::Brush | Tool::Pencil => toggle(
                &mut out,
                &mut x,
                "Airbrush".into(),
                self.brush_dyn.airbrush,
                OptAct::AirbrushToggle,
                76,
            ),
            Tool::Heal => toggle(
                &mut out,
                &mut x,
                if self.heal_use_pattern {
                    "Source: Pattern".into()
                } else {
                    "Source: Sampled".into()
                },
                false,
                OptAct::HealSource,
                118,
            ),
            Tool::Stamp | Tool::PatternStamp => toggle(
                &mut out,
                &mut x,
                if self.stamp_sample_merged {
                    "Sample: Merged".into()
                } else {
                    "Sample: Layer".into()
                },
                false,
                OptAct::StampSample,
                118,
            ),
            Tool::Eraser => toggle(
                &mut out,
                &mut x,
                "Erase to History".into(),
                self.eraser_to_history,
                OptAct::EraseHistory,
                118,
            ),
            Tool::MagicEraser | Tool::BackgroundEraser => stepper(
                &mut out,
                &mut x,
                "Tol",
                format!("{}", self.eraser_tolerance),
                OptAct::TolDown,
                OptAct::TolUp,
            ),
            Tool::Dodge | Tool::Burn | Tool::Sponge => stepper(
                &mut out,
                &mut x,
                "Exp",
                format!("{:.0}%", self.tone_exposure * 100.0),
                OptAct::ToneExpDown,
                OptAct::ToneExpUp,
            ),
            Tool::ShapeRect
            | Tool::ShapeRounded
            | Tool::ShapeEllipse
            | Tool::ShapeLine
            | Tool::ShapePolygon
            | Tool::ShapeCustom => {
                stepper(
                    &mut out,
                    &mut x,
                    "Stroke",
                    format!("{} px", self.shape_stroke),
                    OptAct::StrokeDown,
                    OptAct::StrokeUp,
                );
                if self.tool == Tool::ShapeLine {
                    toggle(
                        &mut out,
                        &mut x,
                        "Arrow: Start".into(),
                        self.line_arrow_start,
                        OptAct::ArrowStart,
                        92,
                    );
                    toggle(
                        &mut out,
                        &mut x,
                        "Arrow: End".into(),
                        self.line_arrow_end,
                        OptAct::ArrowEnd,
                        84,
                    );
                }
            }
            Tool::Eyedropper | Tool::ColorSampler => toggle(
                &mut out,
                &mut x,
                if self.dropper_sample_merged {
                    "Sample: Merged".into()
                } else {
                    "Sample: Layer".into()
                },
                false,
                OptAct::DropperSample,
                118,
            ),
            Tool::Gradient => toggle(
                &mut out,
                &mut x,
                match self.gradient_shape {
                    brush::GradientShape::Linear => "Linear".into(),
                    brush::GradientShape::Radial => "Radial".into(),
                    brush::GradientShape::Angle => "Angle".into(),
                    brush::GradientShape::Diamond => "Diamond".into(),
                    brush::GradientShape::Reflected => "Reflected".into(),
                },
                false,
                OptAct::GradShape,
                92,
            ),
            _ => {}
        }
        out
    }

    /// Stepper captions aligned with options_items: the
    /// stepper groups in order.
    pub(crate) fn options_captions(&self) -> Vec<&'static str> {
        let mut caps = Vec::new();
        if matches!(
            self.tool,
            Tool::Brush
                | Tool::Airbrush
                | Tool::Pencil
                | Tool::Stamp
                | Tool::PatternStamp
                | Tool::HistoryBrush
                | Tool::ArtHistoryBrush
                | Tool::Eraser
                | Tool::Blur
                | Tool::Sharpen
                | Tool::Smudge
        ) {
            caps.push("Size:");
            caps.push("Hard:");
            if matches!(
                self.tool,
                Tool::Brush
                    | Tool::Airbrush
                    | Tool::Pencil
                    | Tool::Heal
                    | Tool::Stamp
                    | Tool::PatternStamp
            ) {
                caps.push("Flow:");
            }
        }
        match self.tool {
            Tool::MagicEraser | Tool::BackgroundEraser => caps.push("Tol:"),
            Tool::Dodge | Tool::Burn | Tool::Sponge => caps.push("Exp:"),
            Tool::ShapeRect
            | Tool::ShapeRounded
            | Tool::ShapeEllipse
            | Tool::ShapeLine
            | Tool::ShapePolygon
            | Tool::ShapeCustom => caps.push("Stroke:"),
            _ => {}
        }
        caps
    }

    pub(crate) fn draw_options_bar(&mut self, f: &mut Frame) {
        let Some(bar) = self.options_bar_rect() else {
            return;
        };
        f.fill_rect(bar, ui::PLATINUM);
        f.fill_rect(Rect::new(0, bar.bottom() - 1, bar.w, 1), ui::DARK);
        let picker_label = match self.current_tool_preset() {
            Some(i) => self.tool_presets[i].name.clone(),
            None => self.tool.name().to_string(),
        };
        f.button(
            Rect::new(bar.x + 4, bar.y + 3, 110, 20),
            &picker_label,
            self.tool_preset_popup,
            true,
        );
        // Stepper captions sit in the 56px slots the items
        // builder reserved before each value button.
        let caps = self.options_captions();
        let mut cx = bar.x + 118;
        for cap in &caps {
            f.text(cx, bar.y + 9, cap, 1, ui::TEXT);
            cx += 56 + 54 + 18 + 26;
        }
        for (r, label, selected, _act) in self.options_items() {
            f.button(r, &label, selected, true);
        }
        // Context readout at the right edge.
        let hint = match self.tool {
            Tool::Move => format!("Layer: {}", self.doc.layers[self.doc.active_layer].name),
            Tool::Zoom | Tool::Hand => format!("Zoom: {:.0}%", self.viewport.scale * 100.0),
            Tool::Stamp if self.clone_src.is_none() => "Alt+click to set the clone source".into(),
            Tool::Stamp => "Clone source set - Alt+click to move it".into(),
            Tool::Heal if !self.heal_use_pattern && self.heal_src.is_none() => {
                "Alt+click to set the heal source".into()
            }
            Tool::Heal if !self.heal_use_pattern => "Heal source set - Alt+click to move it".into(),
            _ => String::new(),
        };
        if !hint.is_empty() {
            let w = hint.len() as i32 * 9;
            f.text(bar.right() - w - 8, bar.y + 9, &hint, 1, ui::TEXT_DIM);
        }
    }

    pub(crate) fn options_press(&mut self, mx: i32, my: i32) -> bool {
        let Some(bar) = self.options_bar_rect() else {
            return false;
        };
        if Rect::new(bar.x + 4, bar.y + 3, 110, 20).contains(mx, my) {
            self.tool_preset_popup = !self.tool_preset_popup;
            return true;
        }
        if !bar.contains(mx, my) {
            return false;
        }
        for (r, _label, _sel, act) in self.options_items() {
            if r.contains(mx, my) {
                match act {
                    OptAct::TipSizeDown => {
                        self.tip = BrushTip::new((self.tip.size / 1.25).max(1.0), self.tip.hardness)
                    }
                    OptAct::TipSizeUp => {
                        self.tip =
                            BrushTip::new((self.tip.size * 1.25).min(500.0), self.tip.hardness)
                    }
                    OptAct::TipHardDown => {
                        self.tip = BrushTip::new(self.tip.size, (self.tip.hardness - 0.1).max(0.0))
                    }
                    OptAct::TipHardUp => {
                        self.tip = BrushTip::new(self.tip.size, (self.tip.hardness + 0.1).min(1.0))
                    }
                    OptAct::ToneExpDown => {
                        self.tone_exposure = (self.tone_exposure - 0.05).max(0.0)
                    }
                    OptAct::ToneExpUp => self.tone_exposure = (self.tone_exposure + 0.05).min(1.0),
                    OptAct::TolDown => {
                        self.eraser_tolerance = self.eraser_tolerance.saturating_sub(8)
                    }
                    OptAct::TolUp => {
                        self.eraser_tolerance = self.eraser_tolerance.saturating_add(8)
                    }
                    OptAct::StrokeDown => self.shape_stroke = self.shape_stroke.saturating_sub(1),
                    OptAct::StrokeUp => self.shape_stroke = (self.shape_stroke + 1).min(100),
                    OptAct::FlowDown => {
                        self.flow = (self.flow - 0.05).clamp(0.05, 1.0);
                    }
                    OptAct::FlowUp => {
                        self.flow = (self.flow + 0.05).clamp(0.05, 1.0);
                    }
                    OptAct::AirbrushToggle => {
                        self.brush_dyn.airbrush = !self.brush_dyn.airbrush;
                    }
                    OptAct::HealSource => {
                        self.heal_use_pattern = !self.heal_use_pattern;
                    }
                    OptAct::StampSample => {
                        self.stamp_sample_merged = !self.stamp_sample_merged;
                        self.status = if self.stamp_sample_merged {
                            "Clone Stamp samples the merged image".into()
                        } else {
                            "Clone Stamp samples the active layer".into()
                        };
                    }
                    OptAct::EraseHistory => {
                        self.eraser_to_history = !self.eraser_to_history;
                    }
                    OptAct::DropperSample => {
                        self.dropper_sample_merged = !self.dropper_sample_merged;
                    }
                    OptAct::ArrowStart => {
                        self.line_arrow_start = !self.line_arrow_start;
                    }
                    OptAct::ArrowEnd => {
                        self.line_arrow_end = !self.line_arrow_end;
                    }
                    OptAct::GradShape => {
                        use brush::GradientShape as G;
                        self.gradient_shape = match self.gradient_shape {
                            G::Linear => G::Radial,
                            G::Radial => G::Angle,
                            G::Angle => G::Diamond,
                            G::Diamond => G::Reflected,
                            G::Reflected => G::Linear,
                        };
                    }
                }
                return true;
            }
        }
        true // clicks anywhere on the bar are consumed
    }

    /// Tool Presets panel (7.0): click a row to apply it;
    /// the footer captures the current tool setup.
    pub(crate) fn draw_tool_presets(&mut self, f: &mut Frame) {
        let Some(r) = self.group_rect(PanelKind::ToolPresets) else {
            return;
        };
        f.text(r.x + 5, r.y + 5, "Tool Presets", 1, ui::TEXT);
        let cur = self.current_tool_preset();
        for (i, p) in self.tool_presets.iter().take(10).enumerate() {
            let rr = Rect::new(r.x + 4, r.y + 20 + i as i32 * 15, r.w - 8, 14);
            if cur == Some(i) {
                f.fill_rect(rr, (36, 84, 160));
                f.text(rr.x + 3, rr.y + 3, &p.name, 1, ui::WHITE);
            } else {
                f.text(rr.x + 3, rr.y + 3, &p.name, 1, ui::TEXT);
            }
        }
        Self::draw_small_button(
            f,
            Rect::new(r.x + 5, r.bottom() - 16, 100, 13),
            "New Preset",
            true,
        );
    }

    /// The options-bar tool preset picker popup.
    pub(crate) fn draw_tool_preset_popup(&mut self, f: &mut Frame) {
        let Some(bar) = self.options_bar_rect() else {
            return;
        };
        let w = 200;
        let h = self.tool_presets.len() as i32 * 16 + 6;
        let pop = Rect::new(bar.x + 4, bar.bottom(), w, h);
        f.fill_rect(pop, ui::PLATINUM);
        f.outline_rect(pop, ui::BLACK);
        let cur = self.current_tool_preset();
        for (i, p) in self.tool_presets.iter().enumerate() {
            let rr = Rect::new(pop.x + 3, pop.y + 3 + i as i32 * 16, w - 6, 15);
            if cur == Some(i) {
                f.fill_rect(rr, (36, 84, 160));
                f.text(rr.x + 3, rr.y + 3, &p.name, 1, ui::WHITE);
            } else {
                f.text(rr.x + 3, rr.y + 3, &p.name, 1, ui::TEXT);
            }
        }
    }

    pub(crate) fn tool_preset_row_rect(&self, i: usize) -> Option<Rect> {
        let bar = self.options_bar_rect()?;
        Some(Rect::new(
            bar.x + 7,
            bar.bottom() + 3 + i as i32 * 16,
            194,
            15,
        ))
    }

    pub(crate) fn draw_toolbox(&mut self, f: &mut Frame) {
        let area = Rect::new(
            0,
            self.top_y(),
            ui::TOOLBAR_W,
            f.h as i32 - self.top_y() - ui::STATUS_H,
        );
        f.fill_rect(area, ui::PLATINUM);
        f.fill_rect(Rect::new(area.right() - 1, area.y, 1, area.h), ui::SHADOW);
        for (i, t) in TOOLS.iter().enumerate() {
            let r = Self::tool_rect(i, self.top_y());
            let selected = *t == self.tool
                || (self.tool == Tool::MarqueeEllipse && *t == Tool::MarqueeRect && false);
            if selected {
                f.fill_rect(r, ui::PLATINUM_LIGHT);
                f.bevel_sunken(r);
            } else if r.contains(self.cursor.0 as i32, self.cursor.1 as i32) {
                f.fill_rect(r, ui::HOVER);
            }
            draw_tool_icon(f, *t, r);
        }
        // foreground/background swatches (classic layout: FG overlaps BG)
        let sy = area.bottom() - 52;
        f.fill_rect(Rect::new(4, sy - 18, area.w - 8, 1), ui::SHADOW);
        f.fill_rect(Rect::new(4, sy - 17, area.w - 8, 1), ui::WHITE);
        f.text(6, sy - 12, "Color", 1, ui::TEXT);
        let bg_r = Rect::new(24, sy + 12, 20, 20);
        let fg_r = Rect::new(10, sy, 20, 20);
        let (br, bgc, bb, _) = self.bg.to_rgba8();
        let (fr, fgc, fb, _) = self.fg.to_rgba8();
        f.fill_rect(bg_r, (br, bgc, bb));
        f.outline_rect(bg_r, ui::BLACK);
        f.fill_rect(fg_r, (fr, fgc, fb));
        f.outline_rect(fg_r, ui::BLACK);
        // Web-safe warning (Phase 5): the classic cube-only color
        // alert — click it to snap FG to the nearest web-safe color.
        if !Self::web_safe((fr, fgc, fb)) {
            let wr = Self::web_warn_rect(self.win_size.1 as i32);
            f.line(wr.x + 5, wr.y, wr.x, wr.y + 9, (200, 0, 0));
            f.line(wr.x, wr.y + 9, wr.x + 10, wr.y + 9, (200, 0, 0));
            f.line(wr.x + 10, wr.y + 9, wr.x + 5, wr.y, (200, 0, 0));
            f.fill_rect(Rect::new(wr.x + 4, wr.y + 3, 2, 4), (200, 0, 0));
            f.set(wr.x + 5, wr.y + 8, (200, 0, 0));
        }
        // default colors mini-swatches
        f.fill_rect(Rect::new(40, sy, 8, 8), (255, 255, 255));
        f.outline_rect(Rect::new(40, sy, 8, 8), ui::BLACK);
        f.fill_rect(Rect::new(36, sy + 4, 8, 8), (0, 0, 0));
        f.outline_rect(Rect::new(36, sy + 4, 8, 8), ui::BLACK);
    }

    pub(crate) fn draw_menu_bar(&mut self, f: &mut Frame) {
        let bar = Rect::new(0, 0, f.w as i32, ui::MENU_H);
        f.fill_rect(bar, ui::PLATINUM);
        f.fill_rect(Rect::new(0, ui::MENU_H - 1, f.w as i32, 1), ui::DARK);
        let menus = self.build_menus();
        let titles = self.menu_title_rects();
        for (r, i) in &titles {
            if self.open_menu == Some(*i) {
                f.fill_rect(*r, ui::ACCENT);
                f.text(r.x + 7, r.y + 7, menus[*i].title, 1, ui::WHITE);
            } else {
                f.text(r.x + 7, r.y + 7, menus[*i].title, 1, ui::TEXT);
            }
        }
        // document name at right, Mac-style
        let name = self
            .file_path
            .as_ref()
            .map(|p| {
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_else(|| "Untitled".into());
        let label = format!(
            "{}{}  {}x{}",
            name,
            if self.dirty { " *" } else { "" },
            self.doc.width,
            self.doc.height
        );
        let tw = Frame::text_width(&label, 1);
        f.text(f.w as i32 - tw - 10, 7, &label, 1, ui::TEXT_DIM);
        // dropdown
        if let Some(mi) = self.open_menu {
            let dr = self.dropdown_rect(mi);
            f.fill_rect(dr, ui::PLATINUM_LIGHT);
            f.bevel_raised(dr);
            for (ii, item) in menus[mi].items.iter().enumerate() {
                let ir = Rect::new(dr.x + 2, dr.y + 3 + ii as i32 * 17, dr.w - 4, 17);
                let hovered = self.hover_item == Some((mi, ii));
                if hovered && item.enabled {
                    f.fill_rect(ir, ui::ACCENT);
                }
                let c = if !item.enabled {
                    ui::DISABLED
                } else if hovered {
                    ui::WHITE
                } else {
                    ui::TEXT
                };
                if item.checked {
                    // tiny checkmark
                    f.line(ir.x + 3, ir.y + 9, ir.x + 6, ir.y + 12, c);
                    f.line(ir.x + 6, ir.y + 12, ir.x + 12, ir.y + 4, c);
                }
                f.text(ir.x + 16, ir.y + 4, &item.label, 1, c);
                if let Some(sc) = &item.shortcut {
                    let sw = Frame::text_width(sc, 1);
                    f.text(ir.right() - sw - 6, ir.y + 4, sc, 1, c);
                }
                if item.submenu.is_some() {
                    f.text(ir.right() - 16, ir.y + 4, ">", 1, c);
                }
            }
            // submenu
            if let Some(si) = self.open_submenu {
                if let Some(sub) = &menus[mi].items[si].submenu {
                    let parent = Rect::new(dr.x + 2, dr.y + 3 + si as i32 * 17, dr.w - 4, 17);
                    let sw = sub
                        .iter()
                        .map(|it| {
                            Frame::text_width(&it.label, 1)
                                + it.shortcut
                                    .as_deref()
                                    .map(|sc| Frame::text_width(sc, 1) + 12)
                                    .unwrap_or(0)
                                + 40
                        })
                        .max()
                        .unwrap_or(120);
                    let sr = Rect::new(dr.right() - 2, parent.y - 3, sw, sub.len() as i32 * 17 + 6);
                    f.fill_rect(sr, ui::PLATINUM_LIGHT);
                    f.bevel_raised(sr);
                    for (ii, item) in sub.iter().enumerate() {
                        let ir = Rect::new(sr.x + 2, sr.y + 3 + ii as i32 * 17, sr.w - 4, 17);
                        let hovered = self.hover_sub_item == Some(ii);
                        if hovered && item.enabled {
                            f.fill_rect(ir, ui::ACCENT);
                        }
                        let c = if !item.enabled {
                            ui::DISABLED
                        } else if hovered {
                            ui::WHITE
                        } else {
                            ui::TEXT
                        };
                        f.text(ir.x + 8, ir.y + 4, &item.label, 1, c);
                        if let Some(sc) = &item.shortcut {
                            let sw = Frame::text_width(sc, 1);
                            f.text(ir.right() - sw - 6, ir.y + 4, sc, 1, c);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn draw_status(&mut self, f: &mut Frame) {
        let r = Rect::new(0, f.h as i32 - ui::STATUS_H, f.w as i32, ui::STATUS_H);
        f.fill_rect(r, ui::PLATINUM);
        f.fill_rect(Rect::new(0, r.y, f.w as i32, 1), ui::SHADOW);
        let hint = if self.tool == Tool::Stamp && self.clone_src.is_none() {
            "Alt+click to set clone source".to_string()
        } else {
            format!("Tool: {}", self.tool.name())
        };
        let text = format!(
            "{}  |  {}x{}  |  Zoom {:.0}%  |  {}  |  FG {} {} {}",
            self.status,
            self.doc.width,
            self.doc.height,
            self.viewport.scale * 100.0,
            hint,
            self.fg.to_rgba8().0,
            self.fg.to_rgba8().1,
            self.fg.to_rgba8().2
        );
        f.text(8, r.y + 5, &text, 1, ui::TEXT);
    }
}
