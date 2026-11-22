//! The Hihu Island icon, drawn with the canvas for the Settings window (the
//! sidebar's tile, About, the version card). The same picture as
//! `res/hihu.ico` (`tools/make_hihu_icon.py`): an amber tile, a black notch
//! pill like bangs, two happy closed eyes, blush and a small smile. At 24
//! DIPs and below it is the icon's simpler face, as in the small .ico sizes.

use crate::gfx::Canvas;
use poni_core::geom::{Color, Point, Rect};

const AMBER: Color = Color::hex(0xF5B942);
const INK: Color = Color::hex(0x141414);
const BLUSH: Color = Color::rgba(1.0, 0.502, 0.439, 0.784);

/// The icon, `size` DIPs square, centred on `center`. Keeps its own colours
/// in every theme.
pub fn app_icon(c: &mut Canvas, center: Point, size: f32) {
    c.unthemed(|c| draw(c, center, size));
}

fn draw(c: &mut Canvas, center: Point, size: f32) {
    let (x0, y0) = (center.x - size / 2.0, center.y - size / 2.0);
    // A point and a box in the icon's own 0..1 units.
    let at = |x: f32, y: f32| Point::new(x0 + x * size, y0 + y * size);
    let rect = |l: f32, t: f32, r: f32, b: f32| Rect::new(x0 + l * size, y0 + t * size, (r - l) * size, (b - t) * size);
    let small = size <= 24.0;
    let m = if small { 0.0 } else { 0.03 };
    c.fill_rrect(rect(m, m, 1.0 - m, 1.0 - m), 0.24 * size, AMBER);
    // One closed eye: a caret (^) with round ends.
    let caret = |c: &mut Canvas, cx: f32, top: f32, half: f32, h: f32, w: f32| {
        let (l, p, r) = (at(cx - half, top + h), at(cx, top), at(cx + half, top + h));
        c.line(l, p, INK, w * size);
        c.line(p, r, INK, w * size);
    };
    if small {
        c.fill_rrect(rect(0.12, 0.10, 0.88, 0.42), 0.16 * size, INK);
        for cx in [0.31, 0.69] {
            caret(c, cx, 0.56, 0.14, 0.14, 0.12);
        }
    } else {
        c.fill_rrect(rect(0.20, 0.14, 0.80, 0.36), 0.11 * size, INK);
        for cx in [0.35, 0.65] {
            caret(c, cx, 0.50, 0.085, 0.085, 0.05);
        }
        for cx in [0.24, 0.76] {
            c.fill_rrect(rect(cx - 0.065, 0.66, cx + 0.065, 0.72), 0.03 * size, BLUSH);
        }
        // The smile: the lower part of a small circle, 20° to 160° from 3 o'clock.
        c.arc(at(0.5, 0.70), 0.06 * size, 0.25 + 20.0 / 360.0, 140.0 / 360.0, INK, 0.035 * size);
    }
}
