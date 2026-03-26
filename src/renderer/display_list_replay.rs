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
    Style as CTextStyle, Weight as CTextWeight,
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
}

fn replay_inner(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    scale: f32,
    mut text_ctx: Option<(&mut FontSystem, &mut SwashCache)>,
) {
    let mut ts = Transform::from_scale(scale, scale);
    let mut transform_stack: Vec<Transform> = Vec::new();
    let mut filter_stack: Vec<Vec<(u8, f32, crate::types::Color)>> = Vec::new();
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
                // Skip text that's entirely outside the current clip region
                // (handles text-indent:-9999px with overflow:hidden)
                if let Some(clip) = clip_stack.last() {
                    if *x + 1000.0 < clip.x || *x > clip.right()
                        || *y + *line_height < clip.y || *y > clip.bottom() {
                        continue;
                    }
                }
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
                // Apply CSS transform by modifying the global transform matrix.
                // The transform matrix m = [a,b,c,d,e,f] is a 2D affine transform
                // that already includes translate-to-origin and translate-back.
                let css_t = Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
                let new_ts = ts.pre_concat(css_t);
                // Push old ts onto a stack so we can restore it
                transform_stack.push(ts);
                ts = new_ts;
            }
            PaintCmd::PopTransform => {
                if let Some(old_ts) = transform_stack.pop() {
                    ts = old_ts;
                }
            }

            PaintCmd::PushFilter { filters } => {
                if let Some(layer_pixmap) = Pixmap::new(pw, ph) {
                    // Store filter ops encoded in the blend_mode field won't work,
                    // so we store them separately via a filter_stack
                    filter_stack.push(filters.clone());
                    layer_stack.push(Layer { pixmap: layer_pixmap, blend_mode: 254 });
                }
            }
            PaintCmd::PopFilter => {
                let filters = filter_stack.pop().unwrap_or_default();
                if let Some(layer) = layer_stack.pop() {
                    let mut pm = layer.pixmap;
                    // Apply each filter to the layer pixels
                    for (filter_type, value, _color) in &filters {
                        apply_pixel_filter(&mut pm, *filter_type, *value);
                    }
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    target.draw_pixmap(0, 0, pm.as_ref(),
                        &tiny_skia::PixmapPaint::default(),
                        Transform::identity(), None);
                }
            }
            PaintCmd::PushBlendMode { mode } => {
                // Create a temporary layer for blend compositing
                if let Some(layer_pixmap) = Pixmap::new(pw, ph) {
                    layer_stack.push(Layer { pixmap: layer_pixmap, blend_mode: *mode });
                }
            }
            PaintCmd::PopBlendMode => {
                if let Some(layer) = layer_stack.pop() {
                    // Composite the layer back onto the main pixmap with the blend mode
                    blend_composite(pixmap, &layer.pixmap, layer.blend_mode);
                }
            }

            PaintCmd::BoxShadow { rect, color, offset_x, offset_y, blur: _, spread, inset, radii: _ } => {
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

            PaintCmd::Outline { rect, width, color, style: _, offset: _ } => {
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
                                // Vertically center the text in the element
                                let line_h = *font_size * 1.2;
                                let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, text_y, display_text, font_family,
                                    *font_size, *font_weight, 0, 100.0, line_h,
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
                                // Vertically center the text in the element
                                let line_h = *font_size * 1.2;
                                let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, text_y, display_text, font_family,
                                    *font_size, *font_weight, 0, 100.0, line_h,
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
                                // Vertically center the text in the element
                                let line_h = *font_size * 1.2;
                                let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, text_y, display_text, font_family,
                                    *font_size, *font_weight, 0, 100.0, line_h,
                                    &c, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
                    }
                }
            }

            PaintCmd::TextShadow { x, y, text, font_family, font_size, font_weight, font_style, font_stretch, line_height, color, blur: _ } => {
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
                        let _start_x = if *repeat_x { cx } else { *pos_x };
                        let end_x   = if *repeat_x { cx + cw } else { *pos_x + *draw_w };
                        let _start_y = if *repeat_y { cy } else { *pos_y };
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

    // Draw text decorations (underline, overline, strikethrough)
    let line_w = buf.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0);
    let thickness = (decoration.thickness * sc).max(1.0);
    let mut paint = Paint::default();
    paint.set_color(to_sk_color(&Color::rgba(
        decoration.color.r, decoration.color.g, decoration.color.b, decoration.color.a,
    )));
    paint.anti_alias = true;

    let draw_deco_line = |pixmap: &mut Pixmap, y: f32, style: u8| {
        match style {
            0 => {
                // solid
                if let Some(r) = SkRect::from_xywh(phys_x, y, line_w, thickness) {
                    pixmap.fill_rect(r, &paint, Transform::identity(), None);
                }
            }
            1 => {
                // double
                if let Some(r) = SkRect::from_xywh(phys_x, y, line_w, 1.0f32.max(thickness * 0.4)) {
                    pixmap.fill_rect(r, &paint, Transform::identity(), None);
                }
                if let Some(r) = SkRect::from_xywh(phys_x, y + thickness * 1.5, line_w, 1.0f32.max(thickness * 0.4)) {
                    pixmap.fill_rect(r, &paint, Transform::identity(), None);
                }
            }
            2 => {
                // dotted
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = thickness;
                stroke.dash = tiny_skia::StrokeDash::new(vec![thickness * 1.5, thickness * 2.0], 0.0);
                let mut pb = PathBuilder::new();
                pb.move_to(phys_x, y + thickness / 2.0);
                pb.line_to(phys_x + line_w, y + thickness / 2.0);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
            3 => {
                // dashed
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = thickness;
                stroke.dash = tiny_skia::StrokeDash::new(vec![thickness * 4.0, thickness * 3.0], 0.0);
                let mut pb = PathBuilder::new();
                pb.move_to(phys_x, y + thickness / 2.0);
                pb.line_to(phys_x + line_w, y + thickness / 2.0);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
            4 => {
                // wavy
                let wave_h = thickness * 1.5;
                let wave_len = thickness * 4.0;
                let mut pb = PathBuilder::new();
                let mut x = phys_x;
                pb.move_to(x, y);
                while x < phys_x + line_w {
                    pb.quad_to(x + wave_len * 0.25, y - wave_h, x + wave_len * 0.5, y);
                    pb.quad_to(x + wave_len * 0.75, y + wave_h, x + wave_len, y);
                    x += wave_len;
                }
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = thickness;
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
            _ => {
                // fallback to solid
                if let Some(r) = SkRect::from_xywh(phys_x, y, line_w, thickness) {
                    pixmap.fill_rect(r, &paint, Transform::identity(), None);
                }
            }
        }
    };

    if decoration.underline {
        // Position underline below the baseline (baseline ≈ 80% of em)
        // plus text-underline-offset
        let baseline_y = phys_y + phys_px * 0.82;
        let offset = thickness * 2.0; // approximate text-underline-offset
        let uy = baseline_y + offset;
        draw_deco_line(pixmap, uy, decoration.style);
    }
    if decoration.overline {
        let oy = phys_y - thickness;
        draw_deco_line(pixmap, oy, decoration.style);
    }
    if decoration.strikethrough {
        let sy = phys_y + phys_px * 0.4;
        draw_deco_line(pixmap, sy, decoration.style);
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
            6 => { // color-dodge
                let dodge = |d: u32, s: u32| -> u32 { if s >= 255 { 255 } else { (d * 255 / (255 - s)).min(255) } };
                (dodge(dr_n, sr_n), dodge(dg_n, sg_n), dodge(db_n, sb_n))
            }
            7 => { // color-burn
                let burn = |d: u32, s: u32| -> u32 { if s == 0 { 0 } else { 255 - ((255 - d) * 255 / s).min(255) } };
                (burn(dr_n, sr_n), burn(dg_n, sg_n), burn(db_n, sb_n))
            }
            8 => { // hard-light (like overlay but src/dst swapped)
                let blend = |d: u32, s: u32| -> u32 {
                    if s < 128 { 2 * d * s / 255 } else { 255 - 2 * (255 - d) * (255 - s) / 255 }
                };
                (blend(dr_n, sr_n), blend(dg_n, sg_n), blend(db_n, sb_n))
            }
            9 => { // soft-light
                let soft = |d: u32, s: u32| -> u32 {
                    let df = d as f32 / 255.0;
                    let sf = s as f32 / 255.0;
                    let r = if sf <= 0.5 {
                        df - (1.0 - 2.0 * sf) * df * (1.0 - df)
                    } else {
                        let g = if df <= 0.25 { ((16.0 * df - 12.0) * df + 4.0) * df } else { df.sqrt() };
                        df + (2.0 * sf - 1.0) * (g - df)
                    };
                    (r * 255.0).round().clamp(0.0, 255.0) as u32
                };
                (soft(dr_n, sr_n), soft(dg_n, sg_n), soft(db_n, sb_n))
            }
            10 => { // difference
                let diff = |a: u32, b: u32| -> u32 { if a > b { a - b } else { b - a } };
                (diff(dr_n, sr_n), diff(dg_n, sg_n), diff(db_n, sb_n))
            }
            11 => { // exclusion
                let excl = |a: u32, b: u32| -> u32 { a + b - 2 * a * b / 255 };
                (excl(dr_n, sr_n), excl(dg_n, sg_n), excl(db_n, sb_n))
            }
            _ => (sr_n, sg_n, sb_n), // normal / hue / saturation / color / luminosity fallback
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

/// Apply a CSS filter to a pixmap's pixels in-place.
/// filter_type: 0=blur,1=brightness,2=contrast,3=grayscale,4=hue-rotate,5=invert,6=opacity,7=saturate,8=sepia
fn apply_pixel_filter(pm: &mut Pixmap, filter_type: u8, value: f32) {
    let pixels = pm.pixels_mut();

    // Helper: un-premultiply, apply transform, re-premultiply
    let process = |px: &mut tiny_skia::PremultipliedColorU8, f: &dyn Fn(f32, f32, f32) -> (f32, f32, f32)| {
        let a = px.alpha();
        if a == 0 { return; }
        // Un-premultiply
        let af = a as f32 / 255.0;
        let r = px.red() as f32 / af;
        let g = px.green() as f32 / af;
        let b = px.blue() as f32 / af;
        let (r2, g2, b2) = f(r, g, b);
        // Re-premultiply
        let pr = (r2 * af).round().clamp(0.0, 255.0) as u8;
        let pg = (g2 * af).round().clamp(0.0, 255.0) as u8;
        let pb = (b2 * af).round().clamp(0.0, 255.0) as u8;
        if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(pr, pg, pb, a) {
            *px = p;
        }
    };

    match filter_type {
        0 => {} // blur — needs convolution, skipped
        1 => {
            // brightness
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| (
                    (r * value).min(255.0),
                    (g * value).min(255.0),
                    (b * value).min(255.0),
                ));
            }
        }
        2 => {
            // contrast
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| {
                    let adj = |c: f32| ((c / 255.0 - 0.5) * value + 0.5) * 255.0;
                    (adj(r).clamp(0.0, 255.0), adj(g).clamp(0.0, 255.0), adj(b).clamp(0.0, 255.0))
                });
            }
        }
        3 => {
            // grayscale
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| {
                    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    let mix = |c: f32| c * (1.0 - value) + lum * value;
                    (mix(r), mix(g), mix(b))
                });
            }
        }
        4 => {
            // hue-rotate
            let rad = value * std::f32::consts::PI / 180.0;
            let cos = rad.cos();
            let sin = rad.sin();
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| {
                    let (rf, gf, bf) = (r / 255.0, g / 255.0, b / 255.0);
                    let r2 = ((0.213 + 0.787*cos - 0.213*sin)*rf + (0.715 - 0.715*cos - 0.715*sin)*gf + (0.072 - 0.072*cos + 0.928*sin)*bf) * 255.0;
                    let g2 = ((0.213 - 0.213*cos + 0.143*sin)*rf + (0.715 + 0.285*cos + 0.140*sin)*gf + (0.072 - 0.072*cos - 0.283*sin)*bf) * 255.0;
                    let b2 = ((0.213 - 0.213*cos - 0.787*sin)*rf + (0.715 - 0.715*cos + 0.715*sin)*gf + (0.072 + 0.928*cos + 0.072*sin)*bf) * 255.0;
                    (r2.clamp(0.0, 255.0), g2.clamp(0.0, 255.0), b2.clamp(0.0, 255.0))
                });
            }
        }
        5 => {
            // invert
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| {
                    let inv = |c: f32| (255.0 - c) * value + c * (1.0 - value);
                    (inv(r), inv(g), inv(b))
                });
            }
        }
        6 => {
            // opacity — operates on premultiplied alpha directly
            for px in pixels.iter_mut() {
                let a = px.alpha();
                if a == 0 { continue; }
                let new_a = (a as f32 * value).round().clamp(0.0, 255.0) as u8;
                let scale_factor = if a > 0 { new_a as f32 / a as f32 } else { 0.0 };
                let pr = (px.red() as f32 * scale_factor).round().clamp(0.0, 255.0) as u8;
                let pg = (px.green() as f32 * scale_factor).round().clamp(0.0, 255.0) as u8;
                let pb = (px.blue() as f32 * scale_factor).round().clamp(0.0, 255.0) as u8;
                if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(pr, pg, pb, new_a) {
                    *px = p;
                }
            }
        }
        7 => {
            // saturate
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| {
                    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    let sat = |c: f32| (lum + (c - lum) * value).clamp(0.0, 255.0);
                    (sat(r), sat(g), sat(b))
                });
            }
        }
        8 => {
            // sepia
            for px in pixels.iter_mut() {
                process(px, &|r, g, b| {
                    let sr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
                    let sg = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
                    let sb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
                    let mix = |c: f32, s: f32| c * (1.0 - value) + s * value;
                    (mix(r, sr), mix(g, sg), mix(b, sb))
                });
            }
        }
        _ => {} // drop-shadow or unknown
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
