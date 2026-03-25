//! Display list replay — rasterize paint commands to a pixmap.
//!
//! This replays a DisplayList (built by display_list_builder) to a
//! tiny_skia Pixmap, handling fills, borders, text, images, clips,
//! opacity, and transforms.

use tiny_skia::{
    FillRule, Paint, PathBuilder, Pixmap, Rect as SkRect, Transform, Color as SkColor,
};
use cosmic_text::{
    Attrs, Buffer, Color as CTextColor, FontSystem, Metrics, Shaping, SwashCache,
    Style as CTextStyle, Weight as CTextWeight, Stretch as CTextStretch,
};
use crate::types::{Rect, Color};
use super::display_list::{DisplayList, PaintCmd, ImageRef};

/// Replay a display list onto a pixmap (no text — use replay_with_text for full rendering).
pub fn replay(list: &DisplayList, pixmap: &mut Pixmap, scale: f32) {
    replay_inner(list, pixmap, scale, None);
}

/// Replay with text rendering via cosmic_text.
pub fn replay_with_text(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    scale: f32,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
) {
    replay_inner(list, pixmap, scale, Some((font_system, swash_cache)));
}

/// Layer for blend mode / opacity compositing.
struct Layer {
    pixmap: Pixmap,
    blend_mode: u8,
    transform: Option<[f32; 6]>,  // CSS transform matrix, if this is a transform layer
}

fn replay_inner(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    scale: f32,
    mut text_ctx: Option<(&mut FontSystem, &mut SwashCache)>,
) {
    let ts = Transform::from_scale(scale, scale);
    let mut clip_stack: Vec<Rect> = Vec::new();
    let mut clip_mask_stack: Vec<Option<tiny_skia::Mask>> = Vec::new();
    let mut opacity_stack: Vec<f32> = Vec::new();
    let mut layer_stack: Vec<Layer> = Vec::new();

    let pw = pixmap.width();
    let ph = pixmap.height();

    for cmd in &list.commands {
        // Get the current clip mask (topmost on the stack)
        let clip_mask = clip_mask_stack.last().and_then(|m| m.as_ref());

        match cmd {
            PaintCmd::FillRect { rect, color, radius } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                let c = apply_opacity(color, alpha);
                let mut paint = Paint::default();
                paint.set_color(to_sk_color(&c));
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                let max_r = radius[0].max(radius[1]).max(radius[2]).max(radius[3]);
                if max_r > 0.5 {
                    if let Some(path) = rounded_rect_path(rect.x, rect.y, rect.w, rect.h, max_r) {
                        target.fill_path(&path, &paint, FillRule::Winding, ts, clip_mask);
                    }
                } else if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                    target.fill_rect(r, &paint, ts, clip_mask);
                }
            }

            PaintCmd::Border { rect, widths, colors, styles: _, radii } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                let max_r = radii[0].max(radii[1]).max(radii[2]).max(radii[3]);

                // Uniform border with border-radius → use stroked rounded rect
                let uniform_width = widths[0] == widths[1] && widths[1] == widths[2] && widths[2] == widths[3];
                let uniform_color = colors[0] == colors[1] && colors[1] == colors[2] && colors[2] == colors[3];

                if max_r > 0.5 && uniform_width && uniform_color && widths[0] > 0.0 {
                    let bw = widths[0];
                    let half = bw / 2.0;
                    // Inset the path by half the border width so the stroke straddles the edge
                    if let Some(path) = rounded_rect_path_corners(
                        rect.x + half, rect.y + half,
                        rect.w - bw, rect.h - bw,
                        (radii[0] - half).max(0.0),
                        (radii[1] - half).max(0.0),
                        (radii[2] - half).max(0.0),
                        (radii[3] - half).max(0.0),
                    ) {
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&apply_opacity(&colors[0], alpha)));
                        paint.anti_alias = true;
                        let mut stroke = tiny_skia::Stroke::default();
                        stroke.width = bw;
                        target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                    }
                } else {
                    // Fallback: draw borders as filled rectangles (no rounding)
                    let mut paint = Paint::default();
                    if widths[0] > 0.0 {
                        paint.set_color(to_sk_color(&apply_opacity(&colors[0], alpha)));
                        if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, widths[0]) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    if widths[2] > 0.0 {
                        paint.set_color(to_sk_color(&apply_opacity(&colors[2], alpha)));
                        if let Some(r) = SkRect::from_xywh(rect.x, rect.y + rect.h - widths[2], rect.w, widths[2]) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    if widths[3] > 0.0 {
                        paint.set_color(to_sk_color(&apply_opacity(&colors[3], alpha)));
                        if let Some(r) = SkRect::from_xywh(rect.x, rect.y, widths[3], rect.h) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    if widths[1] > 0.0 {
                        paint.set_color(to_sk_color(&apply_opacity(&colors[1], alpha)));
                        if let Some(r) = SkRect::from_xywh(rect.x + rect.w - widths[1], rect.y, widths[1], rect.h) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                }
            }

            PaintCmd::Image { rect, data } => {
                let (rgba, iw, ih) = match data {
                    ImageRef::Owned(d, w, h) => (d.as_slice(), *w, *h),
                    ImageRef::Shared(d, w, h) => (d.as_slice(), *w, *h),
                };
                if iw == 0 || ih == 0 || rect.w <= 0.0 || rect.h <= 0.0 { continue; }
                if let Some(img_pixmap) = tiny_skia::PixmapRef::from_bytes(rgba, iw, ih) {
                    let sx = rect.w / iw as f32;
                    let sy = rect.h / ih as f32;
                    let img_ts = ts.pre_translate(rect.x, rect.y).pre_scale(sx, sy);
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    target.draw_pixmap(0, 0,
                        img_pixmap,
                        &tiny_skia::PixmapPaint::default(),
                        img_ts,
                        clip_mask,
                    );
                }
            }

            PaintCmd::Text { x, y, text, font_family, font_size, font_weight,
                             font_style, font_stretch, line_height, color,
                             letter_spacing, small_caps, decoration } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                if let Some((ref mut fs, ref mut sc)) = text_ctx {
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    draw_text_cmd(
                        target, *fs, *sc, scale,
                        *x, *y, text, font_family, *font_size, *font_weight,
                        *font_style, *font_stretch, *line_height,
                        &apply_opacity(color, alpha), decoration,
                        *letter_spacing, *small_caps,
                    );
                }
            }

            PaintCmd::PushClip { rect, radius } => {
                clip_stack.push(*rect);
                // Build a clip mask from the clip rect
                let mask = build_clip_mask(rect, radius, pw, ph, scale);
                clip_mask_stack.push(mask);
            }
            PaintCmd::PopClip => {
                clip_stack.pop();
                clip_mask_stack.pop();
            }

            PaintCmd::PushOpacity { alpha } => {
                opacity_stack.push(*alpha);
            }
            PaintCmd::PopOpacity => {
                opacity_stack.pop();
            }

            PaintCmd::PushTransform { transform: m } => {
                // Render into a temp layer, composite with transform applied
                if let Some(layer_pixmap) = Pixmap::new(pw, ph) {
                    // Store transform matrix for use when popping
                    layer_stack.push(Layer { pixmap: layer_pixmap, blend_mode: 255, transform: Some(*m) });
                }
            }
            PaintCmd::PopTransform => {
                if let Some(layer) = layer_stack.pop() {
                    if let Some(m) = layer.transform {
                        // Composite layer with CSS transform applied
                        let css_t = Transform::from_row(m[0], m[1], m[2], m[3], m[4] * scale, m[5] * scale);
                        let combined = Transform::from_scale(scale, scale)
                            .pre_concat(css_t)
                            .pre_concat(Transform::from_scale(1.0 / scale, 1.0 / scale));
                        pixmap.draw_pixmap(
                            0, 0, layer.pixmap.as_ref(),
                            &tiny_skia::PixmapPaint::default(),
                            combined, None,
                        );
                    }
                }
            }

            PaintCmd::PushBlendMode { mode } => {
                // Create a temporary layer for blend compositing
                if let Some(layer_pixmap) = Pixmap::new(pw, ph) {
                    layer_stack.push(Layer { pixmap: layer_pixmap, blend_mode: *mode, transform: None });
                }
            }
            PaintCmd::PopBlendMode => {
                if let Some(layer) = layer_stack.pop() {
                    // Composite the layer back onto the main pixmap with the blend mode
                    blend_composite(pixmap, &layer.pixmap, layer.blend_mode);
                }
            }

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
                        pixmap.fill_rect(r, &paint, ts, clip_mask);
                    }
                }
            }

            PaintCmd::BeginStackingContext { .. } => {}
            PaintCmd::EndStackingContext => {}

            PaintCmd::Gradient { rect, gradient_type, angle, stops, radii, opacity: grad_opacity, blend_mode: _ } => {
                use tiny_skia::{
                    LinearGradient, RadialGradient,
                    GradientStop as SkStop, SpreadMode, Point as SkPoint,
                };
                if stops.len() < 2 { continue; }
                let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                let combined_opacity = a2 * grad_opacity;

                let px = rect.x;
                let py = rect.y;
                let pw = rect.w;
                let ph = rect.h;
                if pw <= 0.0 || ph <= 0.0 { continue; }

                let sk_stops: Vec<SkStop> = stops.iter()
                    .map(|(color, pos)| {
                        let a = ((color.a as f32) * combined_opacity) as u8;
                        SkStop::new(*pos, tiny_skia::Color::from_rgba8(color.r, color.g, color.b, a))
                    })
                    .collect();

                let mut paint = Paint::default();
                paint.anti_alias = true;

                let shader = match gradient_type {
                    1 => {
                        // Linear gradient
                        let rad = angle * std::f32::consts::PI / 180.0;
                        let dx = rad.sin();
                        let dy = -rad.cos();
                        let corners = [0.0f32, dx, dy, dx + dy];
                        let t_min = corners.iter().cloned().fold(f32::MAX, f32::min);
                        let t_max = corners.iter().cloned().fold(f32::MIN, f32::max);
                        let t_range = (t_max - t_min).max(0.001);

                        let start_nx = if dx >= 0.0 { 0.0 } else { 1.0 };
                        let start_ny = if dy >= 0.0 { 0.0 } else { 1.0 };
                        let sx = px + start_nx * pw;
                        let sy = py + start_ny * ph;

                        let denom = dx * dx * ph * ph + dy * dy * pw * pw;
                        let (ex, ey) = if denom > 1e-6 {
                            (sx + dx * pw * ph * ph * t_range / denom,
                             sy + dy * ph * pw * pw * t_range / denom)
                        } else {
                            (sx + pw, sy)
                        };

                        LinearGradient::new(
                            SkPoint::from_xy(sx, sy), SkPoint::from_xy(ex, ey),
                            sk_stops, SpreadMode::Pad, Transform::identity(),
                        )
                    }
                    2 => {
                        // Radial gradient
                        let cx = px + pw / 2.0;
                        let cy = py + ph / 2.0;
                        let r = ((pw / 2.0).powi(2) + (ph / 2.0).powi(2)).sqrt().max(1.0);
                        let center = SkPoint::from_xy(cx, cy);
                        RadialGradient::new(center, 0.0, center, r,
                            sk_stops, SpreadMode::Pad, Transform::identity())
                    }
                    _ => None,
                };

                if let Some(shader) = shader {
                    paint.shader = shader;
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    let [r_tl, r_tr, r_br, r_bl] = radii;
                    let max_r = (*r_tl).max(*r_tr).max(*r_br).max(*r_bl);
                    if max_r > 0.0 {
                        if let Some(path) = rounded_rect_path_corners(px, py, pw, ph, *r_tl, *r_tr, *r_br, *r_bl) {
                            target.fill_path(&path, &paint, FillRule::Winding, ts, clip_mask);
                        }
                    } else if let Some(r) = SkRect::from_xywh(px, py, pw, ph) {
                        target.fill_rect(r, &paint, ts, clip_mask);
                    }
                }
            }

            PaintCmd::Outline { rect, width, color, style: _, offset } => {
                let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                let mut paint = Paint::default();
                paint.set_color(to_sk_color(&apply_opacity(color, a2)));
                paint.anti_alias = true;
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = *width;
                if let Some(path) = rounded_rect_path(rect.x, rect.y, rect.w, rect.h, 0.0) {
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                }
            }

            PaintCmd::HorizontalRule { x1, y1, x2 } => {
                let mut paint = Paint::default();
                paint.set_color_rgba8(128, 128, 128, 255);
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.0;
                let mut pb = tiny_skia::PathBuilder::new();
                pb.move_to(*x1, *y1);
                pb.line_to(*x2, *y1);
                if let Some(path) = pb.finish() {
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                }
            }

            PaintCmd::ListMarker { marker_type, x, y, size, color, text, font_family, font_size, font_weight, font_style, line_height } => {
                let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                let c = apply_opacity(color, a2);
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                match marker_type {
                    0 => {
                        // disc
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&c));
                        if let Some(r) = SkRect::from_xywh(x - size, y - size, size * 2.0, size * 2.0) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    1 => {
                        // circle (filled as fallback)
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&c));
                        if let Some(r) = SkRect::from_xywh(x - size, y - size, size * 2.0, size * 2.0) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    2 => {
                        // square
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&c));
                        let half = size / 2.0;
                        if let Some(r) = SkRect::from_xywh(x - half, y - half, *size, *size) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    3 => {
                        // text marker
                        if let Some((ref mut fs, ref mut sc)) = text_ctx {
                            draw_text_cmd(
                                target, *fs, *sc, scale,
                                *x, *y, text, font_family, *font_size, *font_weight,
                                *font_style, 100.0, *line_height,
                                &c, &super::display_list::TextDecoration::default(),
                                0.0, false,
                            );
                        }
                    }
                    _ => {}
                }
            }

            PaintCmd::FormElement { tag, input_type, rect, node_id, attributes, font_size, font_weight, font_family, color, checked, value, placeholder, input_cursor } => {
                // CSS background/border/padding are drawn by the normal pipeline.
                // FormElement only draws the CONTENT: value text, check marks, radio dots, etc.
                let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                let _ = (node_id, attributes, input_cursor); // suppress warnings

                match (tag.as_str(), input_type.as_str()) {
                    ("input", "checkbox") => {
                        // Draw checkbox box
                        let sz = rect.w.min(rect.h);
                        let bx = rect.x + (rect.w - sz) / 2.0;
                        let by = rect.y + (rect.h - sz) / 2.0;
                        // Background
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(255, 255, 255, 255);
                        paint.anti_alias = true;
                        if let Some(path) = rounded_rect_path(bx, by, sz, sz, 2.0) {
                            target.fill_path(&path, &paint, FillRule::Winding, ts, clip_mask);
                        }
                        // Border
                        paint.set_color_rgba8(118, 118, 118, 255);
                        let mut stroke = tiny_skia::Stroke::default();
                        stroke.width = 1.0;
                        if let Some(path) = rounded_rect_path(bx, by, sz, sz, 2.0) {
                            target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                        }
                        // Check mark
                        if *checked {
                            let cx = bx + sz / 2.0;
                            let cy = by + sz / 2.0;
                            let s = sz * 0.3;
                            paint.set_color_rgba8(51, 51, 51, 255);
                            stroke.width = 2.0;
                            let mut pb = PathBuilder::new();
                            pb.move_to(cx - s, cy);
                            pb.line_to(cx - s * 0.3, cy + s * 0.7);
                            pb.line_to(cx + s, cy - s * 0.6);
                            if let Some(path) = pb.finish() {
                                target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                            }
                        }
                    }
                    ("input", "radio") => {
                        // Draw radio circle
                        let sz = rect.w.min(rect.h);
                        let cx = rect.x + rect.w / 2.0;
                        let cy = rect.y + rect.h / 2.0;
                        let r = sz / 2.0;
                        let mut paint = Paint::default();
                        paint.anti_alias = true;
                        // Outer circle (white fill + gray border)
                        if let Some(path) = circle_path_4q(cx, cy, r) {
                            paint.set_color_rgba8(255, 255, 255, 255);
                            target.fill_path(&path, &paint, FillRule::Winding, ts, clip_mask);
                            paint.set_color_rgba8(118, 118, 118, 255);
                            let mut stroke = tiny_skia::Stroke::default();
                            stroke.width = 1.0;
                            target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                        }
                        // Inner dot if checked
                        if *checked {
                            let ir = r * 0.45;
                            if let Some(path) = circle_path_4q(cx, cy, ir) {
                                paint.set_color_rgba8(51, 51, 51, 255);
                                target.fill_path(&path, &paint, FillRule::Winding, ts, clip_mask);
                            }
                        }
                    }
                    ("select", _) => {
                        // Draw dropdown arrow
                        let arrow_x = rect.x + rect.w - 16.0;
                        let arrow_y = rect.y + rect.h / 2.0;
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&apply_opacity(color, a2)));
                        paint.anti_alias = true;
                        let mut pb = PathBuilder::new();
                        pb.move_to(arrow_x - 4.0, arrow_y - 2.0);
                        pb.line_to(arrow_x, arrow_y + 2.0);
                        pb.line_to(arrow_x + 4.0, arrow_y - 2.0);
                        let mut stroke = tiny_skia::Stroke::default();
                        stroke.width = 1.5;
                        if let Some(path) = pb.finish() {
                            target.stroke_path(&path, &paint, &stroke, ts, clip_mask);
                        }
                        // Draw selected value text
                        let display_text = if value.is_empty() { placeholder } else { value };
                        if !display_text.is_empty() {
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                let c = apply_opacity(color, a2);
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, rect.y, display_text, font_family,
                                    *font_size, *font_weight, 0, 100.0, *font_size * 1.2,
                                    &c, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
                    }
                    ("input", "text") | ("input", "tel") | ("input", "email") |
                    ("input", "password") | ("input", "search") | ("input", "url") |
                    ("input", "number") | ("textarea", _) => {
                        // Draw value or placeholder text
                        let display_text = if value.is_empty() { placeholder } else { value };
                        if !display_text.is_empty() {
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                let c = if value.is_empty() {
                                    // Placeholder: use a dimmed version of the color
                                    let mut pc = apply_opacity(color, a2);
                                    pc.a = (pc.a as f32 * 0.5) as u8;
                                    pc
                                } else {
                                    apply_opacity(color, a2)
                                };
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, rect.y, display_text, font_family,
                                    *font_size, *font_weight, 0, 100.0, *font_size * 1.2,
                                    &c, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
                    }
                    ("input", "range") => {
                        // Draw track
                        let track_y = rect.y + rect.h / 2.0;
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(128, 128, 128, 180);
                        if let Some(r) = SkRect::from_xywh(rect.x, track_y - 2.0, rect.w, 4.0) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                        // Draw thumb
                        let val: f32 = value.parse().unwrap_or(50.0);
                        let min: f32 = attributes.iter().find(|(k,_)| k == "min").map(|(_,v)| v.parse().unwrap_or(0.0)).unwrap_or(0.0);
                        let max: f32 = attributes.iter().find(|(k,_)| k == "max").map(|(_,v)| v.parse().unwrap_or(100.0)).unwrap_or(100.0);
                        let pct = ((val - min) / (max - min).max(0.001)).clamp(0.0, 1.0);
                        let thumb_x = rect.x + pct * rect.w;
                        paint.set_color(to_sk_color(&apply_opacity(color, a2)));
                        if let Some(r) = SkRect::from_xywh(thumb_x - 6.0, track_y - 6.0, 12.0, 12.0) {
                            target.fill_rect(r, &paint, ts, clip_mask);
                        }
                    }
                    ("input", "submit") | ("input", "button") | ("input", "reset") | ("button", _) => {
                        // Button text is rendered by the inline text pipeline — nothing to do here
                    }
                    _ => {
                        // Other form elements: draw value text if present
                        let display_text = if value.is_empty() { placeholder } else { value };
                        if !display_text.is_empty() {
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                let c = apply_opacity(color, a2);
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, rect.y, display_text, font_family,
                                    *font_size, *font_weight, 0, 100.0, *font_size * 1.2,
                                    &c, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
                    }
                }
            }

            PaintCmd::TextShadow { x, y, text, font_family, font_size, font_weight, font_style, font_stretch, line_height, color, blur } => {
                // Draw text shadow (simplified — no blur convolution)
                if let Some((ref mut fs, ref mut sc)) = text_ctx {
                    let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                    let c = apply_opacity(color, a2);
                    draw_text_cmd(
                        pixmap, *fs, *sc, scale,
                        *x, *y, text, font_family, *font_size, *font_weight,
                        *font_style, *font_stretch, *line_height,
                        &c, &super::display_list::TextDecoration::default(),
                        0.0, false,
                    );
                }
            }

            PaintCmd::BackgroundImage { container, data, size_mode: _, draw_w, draw_h, pos_x, pos_y, repeat_x, repeat_y, radii } => {
                // Draw background image, clipped to the container rect
                let (rgba, iw, ih) = match data {
                    ImageRef::Owned(d, w, h) => (d.as_slice(), *w, *h),
                    ImageRef::Shared(d, w, h) => (d.as_slice(), *w, *h),
                };
                if iw == 0 || ih == 0 || *draw_w <= 0.0 || *draw_h <= 0.0 { continue; }
                // Build a clip mask for the container so background doesn't bleed out
                // (essential for CSS sprites where background-position is negative)
                let bg_clip = build_clip_mask(container, radii, pw, ph, scale);
                let bg_clip_ref = bg_clip.as_ref().or(clip_mask);
                if let Some(img_pixmap) = tiny_skia::PixmapRef::from_bytes(rgba, iw, ih) {
                    let sx_img = draw_w / iw as f32;
                    let sy_img = draw_h / ih as f32;
                    let paint = tiny_skia::PixmapPaint::default();
                    let cx = container.x;
                    let cy = container.y;
                    let cw = container.w;
                    let ch = container.h;

                    if *repeat_x || *repeat_y {
                        // Tile the image across the container
                        let start_x = if *repeat_x { cx } else { *pos_x };
                        let end_x   = if *repeat_x { cx + cw } else { *pos_x + *draw_w };
                        let start_y = if *repeat_y { cy } else { *pos_y };
                        let end_y   = if *repeat_y { cy + ch } else { *pos_y + *draw_h };

                        // Align to tile grid from pos_x/pos_y
                        let first_tx = if *repeat_x {
                            let offset = ((*pos_x - cx) % draw_w + draw_w) % draw_w;
                            cx - (draw_w - offset) % draw_w
                        } else { *pos_x };
                        let first_ty = if *repeat_y {
                            let offset = ((*pos_y - cy) % draw_h + draw_h) % draw_h;
                            cy - (draw_h - offset) % draw_h
                        } else { *pos_y };

                        let mut ty = first_ty;
                        while ty < end_y {
                            let mut tx = first_tx;
                            while tx < end_x {
                                let tile_ts = ts.pre_translate(tx, ty).pre_scale(sx_img, sy_img);
                                pixmap.draw_pixmap(0, 0, img_pixmap, &paint, tile_ts, bg_clip_ref);
                                tx += draw_w;
                                if !*repeat_x { break; }
                            }
                            ty += draw_h;
                            if !*repeat_y { break; }
                        }
                    } else {
                        let img_ts = ts.pre_translate(*pos_x, *pos_y).pre_scale(sx_img, sy_img);
                        pixmap.draw_pixmap(0, 0, img_pixmap, &paint, img_ts, bg_clip_ref);
                    }
                }
            }
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

fn draw_text_cmd(
    pixmap: &mut Pixmap,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    scale: f32,
    x: f32, y: f32,
    text: &str,
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    font_style: u8,
    _font_stretch: f32,
    line_height: f32,
    color: &Color,
    decoration: &super::display_list::TextDecoration,
    _letter_spacing: f32,
    _small_caps: bool,
) {
    if text.is_empty() { return; }
    let sc = scale;
    let phys_px = (font_size * sc).max(1.0);
    let phys_lh = (line_height * sc).max(1.0);  // cosmic-text panics on 0
    let metrics = Metrics::new(phys_px, phys_lh);

    // Convert font family
    let family = crate::layout::inline_layout::css_family_to_cosmic(font_family);
    let ct_w = CTextWeight(font_weight);
    let ct_s = match font_style {
        1 => CTextStyle::Italic,
        2 => CTextStyle::Oblique,
        _ => CTextStyle::Normal,
    };
    let attrs = Attrs::new().weight(ct_w).style(ct_s).family(family);

    let mut buf = Buffer::new(font_system, metrics);
    buf.set_size(font_system, None, Some((phys_lh + 4.0).max(1.0)));
    buf.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buf.shape_until_scroll(font_system, false);

    let phys_x = x * sc;
    let phys_y = y * sc;
    let ct_color = CTextColor::rgba(color.r, color.g, color.b, color.a);
    let color_a = color.a as u32;

    let pix_w = pixmap.width() as i32;
    let pix_h = pixmap.height() as i32;
    let stride = pix_w as usize;
    let pixels = pixmap.pixels_mut();

    buf.draw(font_system, swash_cache, ct_color, |gx, gy, gw, gh, gc| {
        let ga = gc.a();
        if ga == 0 { return; }
        let eff_a = ga as u32 * color_a / 255;
        if eff_a == 0 { return; }
        let bx = phys_x as i32 + gx;
        let by = phys_y as i32 + gy;
        let sa = eff_a;
        let ia = 255 - sa;
        let pr = gc.r() as u32 * sa / 255;
        let pg = gc.g() as u32 * sa / 255;
        let pb = gc.b() as u32 * sa / 255;
        for dy in 0..gh as i32 {
            let py = by + dy;
            if py < 0 || py >= pix_h { continue; }
            let row = py as usize * stride;
            for dx in 0..gw as i32 {
                let px_x = bx + dx;
                if px_x < 0 || px_x >= pix_w { continue; }
                let dst = &mut pixels[row + px_x as usize];
                let r = (pr + dst.red()   as u32 * ia / 255) as u8;
                let g = (pg + dst.green() as u32 * ia / 255) as u8;
                let b = (pb + dst.blue()  as u32 * ia / 255) as u8;
                let a = (sa + dst.alpha() as u32 * ia / 255) as u8;
                if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, a) {
                    *dst = p;
                }
            }
        }
    });

    // Draw text decorations (underline, strikethrough)
    if decoration.underline {
        let uy = phys_y + phys_px * 0.9;
        let mut paint = Paint::default();
        paint.set_color(to_sk_color(&Color::rgba(
            decoration.color.r, decoration.color.g, decoration.color.b, decoration.color.a,
        )));
        let thickness = (decoration.thickness * sc).max(1.0);
        if let Some(r) = SkRect::from_xywh(phys_x, uy, buf.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0), thickness) {
            pixmap.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
    if decoration.strikethrough {
        let sy = phys_y + phys_px * 0.55;
        let mut paint = Paint::default();
        paint.set_color(to_sk_color(&Color::rgba(
            decoration.color.r, decoration.color.g, decoration.color.b, decoration.color.a,
        )));
        let thickness = (decoration.thickness * sc).max(1.0);
        if let Some(r) = SkRect::from_xywh(phys_x, sy, buf.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0), thickness) {
            pixmap.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

/// Composite a layer onto the destination pixmap with a blend mode.
fn blend_composite(dst: &mut Pixmap, src: &Pixmap, mode: u8) {
    let dst_pixels = dst.pixels_mut();
    let src_pixels = src.pixels();
    for (d, s) in dst_pixels.iter_mut().zip(src_pixels.iter()) {
        if s.alpha() == 0 { continue; }
        let (sr, sg, sb, sa) = (s.red() as u32, s.green() as u32, s.blue() as u32, s.alpha() as u32);
        let (dr, dg, db, _da) = (d.red() as u32, d.green() as u32, d.blue() as u32, d.alpha() as u32);

        // Unpremultiply for blending
        let (sr_n, sg_n, sb_n) = if sa > 0 { (sr * 255 / sa, sg * 255 / sa, sb * 255 / sa) } else { (0,0,0) };
        let (dr_n, dg_n, db_n) = (dr.min(255), dg.min(255), db.min(255));

        let (br, bg, bb) = match mode {
            1 => { // multiply
                (dr_n * sr_n / 255, dg_n * sg_n / 255, db_n * sb_n / 255)
            }
            2 => { // screen
                (dr_n + sr_n - dr_n * sr_n / 255,
                 dg_n + sg_n - dg_n * sg_n / 255,
                 db_n + sb_n - db_n * sb_n / 255)
            }
            3 => { // overlay
                let blend = |d: u32, s: u32| -> u32 {
                    if d < 128 { 2 * d * s / 255 } else { 255 - 2 * (255 - d) * (255 - s) / 255 }
                };
                (blend(dr_n, sr_n), blend(dg_n, sg_n), blend(db_n, sb_n))
            }
            4 => (dr_n.min(sr_n), dg_n.min(sg_n), db_n.min(sb_n)), // darken
            5 => (dr_n.max(sr_n), dg_n.max(sg_n), db_n.max(sb_n)), // lighten
            10 => { // difference
                let diff = |a: u32, b: u32| -> u32 { if a > b { a - b } else { b - a } };
                (diff(dr_n, sr_n), diff(dg_n, sg_n), diff(db_n, sb_n))
            }
            _ => (sr_n, sg_n, sb_n), // normal fallback
        };

        // Premultiply result and composite with source alpha
        let ia = 255 - sa;
        let fr = (br * sa / 255 + dr * ia / 255).min(255) as u8;
        let fg = (bg * sa / 255 + dg * ia / 255).min(255) as u8;
        let fb = (bb * sa / 255 + db * ia / 255).min(255) as u8;
        let fa = (sa + _da * ia / 255).min(255) as u8;
        if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(fr, fg, fb, fa) {
            *d = p;
        }
    }
}

fn circle_path_4q(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    // Approximate circle with 4 cubic bezier curves (kappa = 0.5522847498)
    let k = r * 0.5522847498;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    pb.close();
    pb.finish()
}

fn build_clip_mask(rect: &Rect, radius: &[f32; 4], pw: u32, ph: u32, scale: f32) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(pw, ph)?;
    let ts = Transform::from_scale(scale, scale);
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    let max_r = radius[0].max(radius[1]).max(radius[2]).max(radius[3]);
    if max_r > 0.5 {
        if let Some(path) = rounded_rect_path_corners(rect.x, rect.y, rect.w, rect.h,
            radius[0], radius[1], radius[2], radius[3])
        {
            mask.fill_path(&path, FillRule::Winding, true, ts);
        }
    } else {
        // Simple rect clip
        if let Some(path) = {
            let mut pb = PathBuilder::new();
            if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                pb.push_rect(r);
            }
            pb.finish()
        } {
            mask.fill_path(&path, FillRule::Winding, true, ts);
        }
    }
    Some(mask)
}

fn rounded_rect_path_corners(x: f32, y: f32, w: f32, h: f32, r_tl: f32, r_tr: f32, r_br: f32, r_bl: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let half = (w / 2.0).min(h / 2.0);
    let tl = r_tl.min(half);
    let tr = r_tr.min(half);
    let br = r_br.min(half);
    let bl = r_bl.min(half);
    let mut pb = PathBuilder::new();
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.quad_to(x + w, y, x + w, y + tr);
    pb.line_to(x + w, y + h - br);
    pb.quad_to(x + w, y + h, x + w - br, y + h);
    pb.line_to(x + bl, y + h);
    pb.quad_to(x, y + h, x, y + h - bl);
    pb.line_to(x, y + tl);
    pb.quad_to(x, y, x + tl, y);
    pb.close();
    pb.finish()
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
