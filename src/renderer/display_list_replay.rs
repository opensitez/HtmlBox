//! Display list replay — rasterize paint commands to a pixmap.
//!
//! This replays a DisplayList (built by display_list_builder) to a
//! tiny_skia Pixmap, handling fills, borders, text, images, clips,
//! opacity, and transforms.

use tiny_skia::{
    FillRule, Paint, PathBuilder, Pixmap, Rect as SkRect, Transform, Color as SkColor,
};
use crate::types::{Rect, Color};
use super::display_list::{DisplayList, PaintCmd, ImageRef};

/// Replay a display list onto a pixmap.
pub fn replay(list: &DisplayList, pixmap: &mut Pixmap, scale: f32) {
    let ts = Transform::from_scale(scale, scale);
    let mut clip_stack: Vec<Rect> = Vec::new();
    let mut opacity_stack: Vec<f32> = Vec::new();
    let current_opacity = 1.0f32;

    for cmd in &list.commands {
        match cmd {
            PaintCmd::FillRect { rect, color, radius } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                let c = apply_opacity(color, alpha);
                let mut paint = Paint::default();
                paint.set_color(to_sk_color(&c));
                let max_r = radius[0].max(radius[1]).max(radius[2]).max(radius[3]);
                if max_r > 0.5 {
                    if let Some(path) = rounded_rect_path(rect.x, rect.y, rect.w, rect.h, max_r) {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
                    }
                } else if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                    pixmap.fill_rect(r, &paint, ts, None);
                }
            }

            PaintCmd::Border { rect, widths, colors, styles: _, radii } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                // Simple solid border rendering — draw each side as a filled rect
                let mut paint = Paint::default();

                // Top border
                if widths[0] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[0], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, widths[0]) {
                        pixmap.fill_rect(r, &paint, ts, None);
                    }
                }
                // Bottom border
                if widths[2] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[2], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y + rect.h - widths[2], rect.w, widths[2]) {
                        pixmap.fill_rect(r, &paint, ts, None);
                    }
                }
                // Left border
                if widths[3] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[3], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y, widths[3], rect.h) {
                        pixmap.fill_rect(r, &paint, ts, None);
                    }
                }
                // Right border
                if widths[1] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[1], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x + rect.w - widths[1], rect.y, widths[1], rect.h) {
                        pixmap.fill_rect(r, &paint, ts, None);
                    }
                }
            }

            PaintCmd::Image { rect, data } => {
                let (rgba, iw, ih) = match data {
                    ImageRef::Owned(d, w, h) => (d.as_slice(), *w, *h),
                    ImageRef::Shared(d, w, h) => (d.as_slice(), *w, *h),
                };
                if iw == 0 || ih == 0 { continue; }
                if let Some(img_pixmap) = tiny_skia::PixmapRef::from_bytes(rgba, iw, ih) {
                    let sx = rect.w / iw as f32;
                    let sy = rect.h / ih as f32;
                    let img_ts = ts.pre_translate(rect.x, rect.y).pre_scale(sx, sy);
                    pixmap.draw_pixmap(0, 0,
                        img_pixmap,
                        &tiny_skia::PixmapPaint::default(),
                        img_ts,
                        None,
                    );
                }
            }

            PaintCmd::Text { x, y, text, color, font_size, .. } => {
                // Simplified text rendering — just draw a colored rect as placeholder
                // Full text rendering requires cosmic_text integration which stays in
                // the legacy renderer for now.
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                let _ = (x, y, text, color, font_size, alpha);
                // TODO: integrate cosmic_text shaping + glyph rasterization
            }

            PaintCmd::PushClip { rect, .. } => {
                clip_stack.push(*rect);
                // TODO: actual clip mask on pixmap
            }
            PaintCmd::PopClip => {
                clip_stack.pop();
            }

            PaintCmd::PushOpacity { alpha } => {
                opacity_stack.push(*alpha);
            }
            PaintCmd::PopOpacity => {
                opacity_stack.pop();
            }

            PaintCmd::PushTransform { .. } => {
                // TODO: transform stack
            }
            PaintCmd::PopTransform => {}

            PaintCmd::BoxShadow { rect, color, offset_x, offset_y, blur, spread, inset, radii } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                if !inset {
                    let sr = Rect::new(
                        rect.x + offset_x - spread,
                        rect.y + offset_y - spread,
                        rect.w + spread * 2.0,
                        rect.h + spread * 2.0,
                    );
                    let mut paint = Paint::default();
                    // Simple shadow: just a filled rect with the shadow color
                    let mut c = apply_opacity(color, alpha);
                    c.a = (c.a as f32 * 0.5) as u8; // soften
                    paint.set_color(to_sk_color(&c));
                    if let Some(r) = SkRect::from_xywh(sr.x, sr.y, sr.w, sr.h) {
                        pixmap.fill_rect(r, &paint, ts, None);
                    }
                }
            }

            PaintCmd::BeginStackingContext { .. } => {}
            PaintCmd::EndStackingContext => {}
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn to_sk_color(c: &Color) -> SkColor {
    SkColor::from_rgba8(c.r, c.g, c.b, c.a)
}

fn apply_opacity(c: &Color, alpha: f32) -> Color {
    if alpha >= 1.0 { return *c; }
    Color::rgba(c.r, c.g, c.b, (c.a as f32 * alpha) as u8)
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}
