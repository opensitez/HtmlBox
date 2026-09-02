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
    replay_inner(list, pixmap, scale, None, 0.0, 0.0);
}

/// Replay with text rendering via cosmic_text.
/// How far outside the viewport a command is still painted. Generous, because
/// a command's own bounds do not account for shadows, outlines or decoration
/// that spill beyond them.
const CULL_MARGIN: f32 = 512.0;

thread_local! {
    /// (font-DB face count, shaped buffers by text+attrs). See the note at the
    /// shaping site: this is the difference between re-shaping every visible
    /// text run on every frame and shaping each distinct one once.
    static SHAPED: std::cell::RefCell<(usize, std::collections::HashMap<u64, Buffer>)> =
        std::cell::RefCell::new((usize::MAX, std::collections::HashMap::new()));
}

/// The document-space vertical extent of a DRAWING command, or `None` for a
/// command that must never be skipped (anything that manipulates a stack, and
/// anything whose extent is not simply its rect).
fn cmd_y_range(cmd: &PaintCmd) -> Option<(f32, f32)> {
    match cmd {
        PaintCmd::FillRect { rect, .. }
        | PaintCmd::Border { rect, .. }
        | PaintCmd::Image { rect, .. }
        | PaintCmd::Gradient { rect, .. }
        | PaintCmd::Outline { rect, .. } => Some((rect.y, rect.y + rect.h)),
        PaintCmd::Text { y, font_size, line_height, .. } => {
            let pad = font_size.max(*line_height) * 2.0;
            Some((y - pad, y + pad))
        }
        _ => None,
    }
}

pub fn replay_with_text(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    scale: f32,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
) {
    replay_inner(list, pixmap, scale, Some((font_system, swash_cache)), 0.0, 0.0);
}

/// Replay with a scroll offset — the display list is in document coordinates,
/// the scroll offset translates to screen coordinates during replay.
pub fn replay_with_scroll(
    list: &DisplayList,
    pixmap: &mut Pixmap,
    scale: f32,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    scroll_x: f32,
    scroll_y: f32,
) {
    replay_inner(list, pixmap, scale, Some((font_system, swash_cache)), scroll_x, scroll_y);
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
    scroll_x: f32,
    scroll_y: f32,
) {
    // Start with scale + scroll translation. Display list is in document
    // coordinates; the scroll offset maps to screen coordinates.
    let mut ts = Transform::from_scale(scale, scale)
        .pre_translate(-scroll_x, -scroll_y);
    let mut transform_stack: Vec<Transform> = Vec::new();
    let mut filter_stack: Vec<Vec<(u8, f32, crate::types::Color)>> = Vec::new();
    let mut clip_stack: Vec<Rect> = Vec::new();
    let mut clip_mask_stack: Vec<Option<tiny_skia::Mask>> = Vec::new();
    let mut opacity_stack: Vec<f32> = Vec::new();
    let mut layer_stack: Vec<Layer> = Vec::new();

    let pw = pixmap.width();
    let ph = pixmap.height();

    // ── Viewport culling ────────────────────────────────────────────────────
    //
    // ⛔ Replay used to paint the WHOLE document every frame. The display list
    // covers the full page height, so on a long page most commands drew far
    // outside the pixmap — and a `Text` command costs a cosmic-text shaping
    // pass whether or not any of it lands on screen. Measured on Wikipedia: a
    // render with NOTHING changed cost 5567 ms, and so did every scroll.
    //
    // Only DRAWING commands are skipped, and only while no transform is in
    // effect — a transform can move a command anywhere, and the push/pop
    // commands must all run or the clip and layer stacks desync.
    let vis_top = scroll_y - CULL_MARGIN;
    let vis_bot = scroll_y + (ph as f32) / scale.max(0.001) + CULL_MARGIN;
    let mut transform_depth = 0i32;

    for cmd in &list.commands {
        match cmd {
            PaintCmd::PushTransform { .. } => transform_depth += 1,
            PaintCmd::PopTransform      => transform_depth -= 1,
            _ => {}
        }
        // ⛔ Also never while a LAYER is active. `PushOpacity`, `PushFilter`
        // and `PushBlendMode` redirect drawing into an offscreen pixmap that is
        // composited later, so a command inside one cannot be judged against
        // the document band the way an ordinary command can.
        if transform_depth == 0 && layer_stack.is_empty() {
            if let Some((top, bot)) = cmd_y_range(cmd) {
                if bot < vis_top || top > vis_bot { continue; }
            }
        }
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
                    // Apply the current transform to text position and scale.
                    // Extract effective scale factor from the transform matrix.
                    let eff_sx = (ts.sx * ts.sx + ts.ky * ts.ky).sqrt();
                    let eff_sy = (ts.kx * ts.kx + ts.sy * ts.sy).sqrt();
                    let eff_scale = eff_sx.max(eff_sy);
                    // Transform the text origin
                    let phys_x = ts.sx * *x + ts.ky * *y + ts.tx;
                    let phys_y = ts.kx * *x + ts.sy * *y + ts.ty;
                    // draw_text_cmd expects logical coords that it will multiply by scale.
                    // We pass pre-transformed coords divided by eff_scale so the multiplication
                    // brings them back to the correct physical position.
                    let text_x = phys_x / eff_scale;
                    let text_y = phys_y / eff_scale;
                    draw_text_cmd(
                        target, *fs, *sc, eff_scale,
                        text_x, text_y, text, font_family, *font_size, *font_weight,
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

            PaintCmd::FormElement { tag, input_type, rect, node_id, attributes, font_size, font_weight, font_family, color, checked, value, placeholder, input_cursor, vertical, options, selected, selected_all } => {
                // CSS background/border/padding are drawn by the normal pipeline.
                // FormElement only draws the CONTENT: value text, check marks, radio dots, etc.
                let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                let _ = (node_id, attributes, input_cursor, &options, selected, &selected_all); // suppress warnings

                match (tag.as_str(), input_type.as_str()) {
                    ("input", "checkbox") => {
                        // Use Checkbox widget
                        let sz = rect.w.min(rect.h);
                        let bx = rect.x + (rect.w - sz) / 2.0;
                        let by = rect.y + (rect.h - sz) / 2.0;
                        let mut cb = crate::widgets::Checkbox::new("");
                        cb.checked = *checked;
                        cb.size = sz;
                        cb.paint(target, bx, by, scale);
                    }
                    ("input", "radio") => {
                        // Use Radio widget
                        let sz = rect.w.min(rect.h);
                        let bx = rect.x + (rect.w - sz) / 2.0;
                        let by = rect.y + (rect.h - sz) / 2.0;
                        let mut rb = crate::widgets::Radio::new("");
                        rb.selected = *checked;
                        rb.size = sz;
                        rb.paint(target, bx, by, scale);
                    }
                    // **A LIST BOX, not a dropdown** — HTML §4.10.7: a
                    // `<select>` with `size` above one, or `multiple`, shows
                    // its options as ROWS instead of one closed value. Both
                    // spellings mean the same control; `size` alone was not
                    // enough, since `<select multiple>` defaults to a list.
                    //
                    // This was painted as a closed combobox whatever the
                    // markup said: a four-row list drew one row and a dropdown
                    // arrow, showing only the first option. The options and the
                    // selected index reach here on the display item precisely
                    // so the rows can be drawn.
                    // `<progress>` and `<meter>` — HTML §4.10.13/§4.10.14.
                    //
                    // Neither is expressible in CSS: the fill is a FRACTION of
                    // two attributes. With no arm here they fell to the generic
                    // text branch below and drew their value as a STRING, which
                    // is why the widget gallery showed `0.6` where a bar goes.
                    ("progress", _) | ("meter", _) => {
                        let attr = |name: &str| {
                            attributes
                                .iter()
                                .find(|(k, _)| k == name)
                                .and_then(|(_, v)| v.trim().parse::<f32>().ok())
                        };
                        // The defaults are the spec's: `max` is 1 for a
                        // progress bar and for a meter, `min` is 0.
                        let min = attr("min").unwrap_or(0.0);
                        let max = attr("max").unwrap_or(1.0).max(min);
                        let has_value = attributes.iter().any(|(k, _)| k == "value");
                        let value = attr("value").unwrap_or(min).clamp(min, max);
                        let span = max - min;

                        let mut gauge = crate::widgets::Gauge::new(if span > 0.0 {
                            (value - min) / span
                        } else {
                            0.0
                        });
                        gauge.width = rect.w;
                        gauge.height = rect.h;
                        // **A `<progress>` with no `value` is indeterminate**,
                        // which HTML distinguishes from `value="0"`. A meter
                        // has no such state — `value` is required.
                        gauge.indeterminate = tag == "progress" && !has_value;
                        if tag == "meter" {
                            gauge.band = crate::widgets::meter_band(
                                value,
                                min,
                                max,
                                attr("low").unwrap_or(min),
                                attr("high").unwrap_or(max),
                                attr("optimum").unwrap_or((min + max) / 2.0),
                            );
                        }
                        gauge.paint(target, rect.x, rect.y, scale);
                    }
                    // A list box is decided by DISPLAY SIZE (HTML §15.5.16),
                    // which defaults to 4 under `multiple` and 1 otherwise.
                    // The predicate here used to be `multiple || size > 1`,
                    // read off the raw attribute with Rust's own parser — close
                    // enough to agree most of the time, and wrong for
                    // `multiple size=1` (a multi-select DROP-DOWN) and for the
                    // lenient integer parsing HTML actually specifies.
                    ("select", _)
                        if attributes
                            .iter()
                            .find(|(k, _)| k == "size")
                            .and_then(|(_, v)| crate::html::forms::parse_non_negative_integer(v))
                            .unwrap_or(if attributes.iter().any(|(k, _)| k == "multiple") { 4 } else { 1 })
                            > 1 =>
                    {
                        // The box itself: the UA sheet gives a `<select>` a
                        // white field and a grey border, and a list box is the
                        // same field with rows in it.
                        let ts = Transform::from_scale(scale, scale);
                        let mut fill = Paint::default();
                        fill.anti_alias = true;
                        fill.set_color_rgba8(255, 255, 255, 255);
                        if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                            target.fill_rect(r, &fill, ts, None);
                        }

                        // Shared with the hit test, so a click cannot land on a
                        // row other than the one drawn here.
                        let line_h = crate::html::forms::list_box_row_height(*font_size);
                        let pad = crate::html::forms::LIST_BOX_PADDING;
                        for (i, label) in options.iter().enumerate() {
                            let row_y = rect.y + pad + i as f32 * line_h;
                            // Clip to the box: a list shows the rows that FIT
                            // and scrolls the rest, and drawing past the border
                            // would paint over whatever is beside it.
                            if row_y + line_h > rect.y + rect.h - pad {
                                break;
                            }
                            let mut text_color = apply_opacity(color, a2);
                            // EVERY selected row, not one index — a `multiple`
                            // list box can have several, and a fresh one has
                            // none at all.
                            if selected_all.get(i).copied().unwrap_or(false) {
                                // The selected row is a filled bar with
                                // reversed text, which is what every browser
                                // and every toolkit draws.
                                let mut bar = Paint::default();
                                bar.set_color_rgba8(0, 120, 215, 255);
                                if let Some(r) = SkRect::from_xywh(
                                    rect.x + 1.0,
                                    row_y,
                                    (rect.w - 2.0).max(0.0),
                                    line_h,
                                ) {
                                    target.fill_rect(r, &bar, ts, None);
                                }
                                text_color = crate::types::Color::rgba(255, 255, 255, 255);
                            }
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 4.0, row_y, label, font_family,
                                    *font_size, *font_weight, 0, rect.w - 8.0, line_h,
                                    &text_color, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
                    }
                    ("select", _) => {
                        // Use Select widget for the arrow
                        let mut sel = crate::widgets::Select::new(vec![]);
                        sel.width = rect.w;
                        sel.height = rect.h;
                        sel.paint(target, rect.x, rect.y, scale);
                        // Draw selected value text
                        let display_text = if value.is_empty() { placeholder } else { value };
                        if !display_text.is_empty() {
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                let c = apply_opacity(color, a2);
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
                    // **`<input type=image>` is an image AND a submit button**
                    // (HTML §4.10.5.1.19). The image itself is painted by the
                    // `<img>` path — `is_image_element` is what lets it in —
                    // so all that is left here is the spec's fallback: "if the
                    // image is unavailable, the alt text is used". Without an
                    // arm it fell to the generic text branch and drew its
                    // VALUE, which for a submit button is the submission name,
                    // not anything a person should see.
                    ("input", "image") => {
                        // `src` alone: the resolved URL is a node FIELD now and
                        // the display list only carries content attributes.
                        let has_image = attributes.iter().any(|(k, _)| k == "src");
                        let alt = attributes
                            .iter()
                            .find(|(k, _)| k == "alt")
                            .map(|(_, v)| v.as_str())
                            .unwrap_or("");
                        // The alt is drawn only when there is no image to show
                        // — an image that HAS loaded is painted by the image
                        // command and must not have text over it.
                        if !alt.is_empty() && !has_image {
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                let c = apply_opacity(color, a2);
                                let line_h = *font_size * 1.2;
                                let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    rect.x + 2.0, text_y, alt, font_family,
                                    *font_size, *font_weight, 0, rect.w, line_h,
                                    &c, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
                    }
                    // `<input type=file>` is a BUTTON plus the chosen file's
                    // name (HTML §4.10.5.1.18) — it fell to the generic arm and
                    // drew the value as bare text, which for an empty control
                    // is nothing at all.
                    ("input", "file") => {
                        let line_h = *font_size * 1.2;
                        let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                        // Measured from the label so the chrome cannot clip its
                        // own word in a large font.
                        let label_w =
                            crate::widgets::CHOOSE.chars().count() as f32 * *font_size * 0.55;
                        let button_w =
                            crate::widgets::FileButton::width_for(label_w).min(rect.w);
                        let mut button =
                            crate::widgets::FileButton::new(button_w, rect.h);
                        button.disabled = attributes.iter().any(|(k, _)| k == "disabled");
                        button.paint(target, rect.x, rect.y, scale);
                        if let Some((ref mut fs, ref mut sc)) = text_ctx {
                            let c = apply_opacity(color, a2);
                            draw_text_cmd(
                                target, *fs, *sc, scale,
                                rect.x + 8.0, text_y, crate::widgets::CHOOSE, font_family,
                                *font_size, *font_weight, 0, button_w, line_h,
                                &c, &super::display_list::TextDecoration::default(),
                                0.0, false,
                            );
                            // ⛔ The empty case is a LABEL, not the value: a
                            // file control with nothing chosen has `value ==
                            // ""`, and drawing this string from the value would
                            // be a control that submits "No file chosen".
                            let name = if value.is_empty() {
                                crate::widgets::NOTHING_CHOSEN
                            } else {
                                value
                            };
                            draw_text_cmd(
                                target, *fs, *sc, scale,
                                rect.x + button_w + 8.0, text_y, name, font_family,
                                *font_size, *font_weight, 0,
                                (rect.w - button_w - 8.0).max(0.0), line_h,
                                &c, &super::display_list::TextDecoration::default(),
                                0.0, false,
                            );
                        }
                    }
                    // The date and time family — a formatted field with a
                    // picker affordance. Five input types, one control.
                    ("input", _)
                        if crate::widgets::DateKind::for_input(input_type.as_str()).is_some() =>
                    {
                        let (kind, pattern) =
                            crate::widgets::DateKind::for_input(input_type.as_str())
                                .expect("guarded above");
                        let mut field = crate::widgets::DateField::new(kind, rect.w, rect.h);
                        field.disabled = attributes.iter().any(|(k, _)| k == "disabled");
                        field.paint(target, rect.x, rect.y, scale);
                        if let Some((ref mut fs, ref mut sc)) = text_ctx {
                            // An empty field shows the PATTERN, dimmed — the
                            // same treatment a placeholder gets, and what makes
                            // an empty date input tell you what it wants.
                            let mut c = apply_opacity(color, a2);
                            let shown = if value.is_empty() {
                                c.a = (c.a as f32 * 0.5) as u8;
                                pattern
                            } else {
                                value
                            };
                            let line_h = *font_size * 1.2;
                            let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                            let room = (rect.w
                                - crate::widgets::DateField::glyph_width(rect.h)
                                - 4.0)
                                .max(0.0);
                            draw_text_cmd(
                                target, *fs, *sc, scale,
                                rect.x + 4.0, text_y, shown, font_family,
                                *font_size, *font_weight, 0, room, line_h,
                                &c, &super::display_list::TextDecoration::default(),
                                0.0, false,
                            );
                        }
                    }
                    // `<input type=color>` is a SWATCH, not a text field. It
                    // fell to the generic arm and rendered `#3366cc` as a
                    // string — the one thing this control never shows.
                    ("input", "color") => {
                        let mut swatch = crate::widgets::ColorSwatch::new(
                            crate::widgets::ColorSwatch::parse(value),
                        );
                        swatch.width = rect.w;
                        swatch.height = rect.h;
                        swatch.paint(target, rect.x, rect.y, scale);
                    }
                    ("input", "text") | ("input", "tel") | ("input", "email") |
                    ("input", "password") | ("input", "search") | ("input", "url") |
                    ("input", "number") | ("textarea", _) => {
                        // Draw value or placeholder text.
                        //
                        // ⛔ **A password field must not draw what it holds.**
                        // HTML §4.10.5.1.5: the value is "obscured so that
                        // people cannot read it" — so the characters are
                        // replaced one for one, which keeps the caret and the
                        // measured width honest. This arm drew the value
                        // verbatim, so `<input type=password value="hunter2">`
                        // rendered the password on screen.
                        //
                        // The PLACEHOLDER is not obscured: it is not the value,
                        // and every browser shows it.
                        let masked: String;
                        let display_text = if value.is_empty() {
                            placeholder
                        } else if input_type == "password" {
                            masked = value.chars().map(|_| '\u{2022}').collect();
                            &masked
                        } else {
                            value
                        };
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
                        // **The spinner, drawn after the field's own text.**
                        // `<input type=number>` IS a text field with a stepper
                        // on it: the field is a CSS box with a text run, which
                        // the engine already draws, and the two arrows are the
                        // part no declaration expresses. Last, so the well
                        // covers a value long enough to reach it — which is
                        // what a browser does as well.
                        if input_type == "number" {
                            let mut stepper = crate::widgets::Stepper::new(rect.w, rect.h);
                            stepper.disabled = attributes.iter().any(|(k, _)| k == "disabled");
                            stepper.paint(target, rect.x, rect.y, scale);
                        }
                    }
                    ("input", "range") => {
                        // Use Slider widget
                        // ⛔ The SPEC's number parser, the same one the click
                        // path uses. Rust's `parse` rejects the trailing junk
                        // HTML's rules ignore, so `min="10 "` read as 0 here
                        // and as 10 in the hit test: the thumb drew in one
                        // place and landed in another.
                        let attr = |name: &str| {
                            attributes
                                .iter()
                                .find(|(k, _)| k == name)
                                .and_then(|(_, v)| crate::html::forms::parse_floating_point(v))
                                .map(|n| n as f32)
                        };
                        let min: f32 = attr("min").unwrap_or(0.0);
                        let max: f32 = attr("max").unwrap_or(100.0);
                        // The value has already been sanitized into range by
                        // the time it reaches paint, so its own fallback is the
                        // state's default rather than a bare 50.
                        let val: f32 = crate::html::forms::parse_floating_point(value)
                            .map(|n| n as f32)
                            .unwrap_or_else(|| if max < min { min } else { min + (max - min) / 2.0 });
                        let mut slider = crate::widgets::Slider::new(min, max, val);
                        slider.width = rect.w;
                        slider.height = rect.h;
                        slider.vertical = *vertical;
                        slider.paint(target, rect.x, rect.y, scale);
                    }
                    // `<button>` takes its label from its CHILDREN, which the
                    // inline text pipeline lays out and draws. Nothing to do.
                    ("button", _) => {}
                    // ⛔ An `<input>` button is a VOID element — it has no
                    // children for that pipeline to find, and its label is the
                    // `value` ATTRIBUTE (HTML §4.10.5.1.19). Sharing the arm
                    // with `<button>` meant `<input type=submit value="Send">`
                    // drew an empty pill: correct chrome, no word on it.
                    //
                    // With no `value` the UA supplies the label, which is why a
                    // bare `<input type=submit>` reads "Submit" in every
                    // browser rather than being blank.
                    ("input", "submit") | ("input", "button") | ("input", "reset") => {
                        let label: &str = if !value.is_empty() {
                            value
                        } else {
                            match input_type.as_str() {
                                "submit" => "Submit",
                                "reset" => "Reset",
                                _ => "",
                            }
                        };
                        if !label.is_empty() {
                            if let Some((ref mut fs, ref mut sc)) = text_ctx {
                                let c = apply_opacity(color, a2);
                                let line_h = *font_size * 1.2;
                                // Centred both ways, as button chrome is.
                                let text_y = rect.y + (rect.h - line_h).max(0.0) / 2.0;
                                let text_w = label.chars().count() as f32 * *font_size * 0.5;
                                let text_x = rect.x + (rect.w - text_w).max(0.0) / 2.0;
                                draw_text_cmd(
                                    target, *fs, *sc, scale,
                                    text_x, text_y, label, font_family,
                                    *font_size, *font_weight, 0, rect.w, line_h,
                                    &c, &super::display_list::TextDecoration::default(),
                                    0.0, false,
                                );
                            }
                        }
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

/// Blit an ALREADY-SHAPED cosmic-text buffer onto the pixmap at a physical
/// origin, source-over.
///
/// Extracted from `draw_text_cmd` so the canvas can share it. The two callers
/// need the same last step and nothing before it: `draw_text_cmd` shapes from a
/// display-list command, `<canvas>`'s `fillText` shapes from the 2D context's
/// own font state, and only then do both want these exact glyphs composited.
///
/// `color`'s alpha scales every glyph on top of whatever cosmic-text resolves
/// per glyph — a span carrying its own colour keeps it, and the parameter still
/// controls the overall opacity.
pub(crate) fn blit_shaped_buffer(
    pixmap: &mut Pixmap,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    buf: &mut Buffer,
    phys_x: f32,
    phys_y: f32,
    color: CTextColor,
) {
    let color_a = color.a() as u32;

    let pix_w = pixmap.width() as i32;
    let pix_h = pixmap.height() as i32;
    let stride = pix_w as usize;
    let pixels = pixmap.pixels_mut();

    buf.draw(font_system, swash_cache, color, |gx, gy, gw, gh, gc| {
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

    let phys_x = x * sc;
    let phys_y = y * sc;
    let ct_color = CTextColor::rgba(color.r, color.g, color.b, color.a);

    // ⛔ SHAPE ONCE, BLIT MANY. This built a `Buffer` and ran a full
    // cosmic-text shaping pass for EVERY text run on EVERY frame — so a page
    // that had not changed at all re-shaped all its visible text just to put
    // the same pixels back. `SwashCache` caches RASTERISED GLYPHS, which is a
    // different thing and does not help here.
    //
    // The shaped buffer depends only on the string and the font attributes, so
    // it is cached on those. Position and colour are applied at blit time.
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        phys_px.to_bits().hash(&mut h);
        phys_lh.to_bits().hash(&mut h);
        font_weight.hash(&mut h);
        font_style.hash(&mut h);
        font_family.hash(&mut h);
        // ⛔ In the key even though this function IGNORES them today — they
        // are a property of the run, and the day they reach the shaper a stale
        // buffer would be a silent wrong-glyph-positions bug.
        _letter_spacing.to_bits().hash(&mut h);
        _small_caps.hash(&mut h);
        h.finish()
    };

    let line_w = SHAPED.with(|cell| {
        let mut map = cell.borrow_mut();
        // A font load changes what any string shapes to, so the whole cache
        // goes when the face count moves.
        let faces = font_system.db().len();
        if map.0 != faces { map.1.clear(); map.0 = faces; }
        // Bounded: a long session on many pages should not grow for ever.
        if map.1.len() > 8192 { map.1.clear(); }

        if !map.1.contains_key(&key) {
            let mut buf = Buffer::new(font_system, metrics);
            buf.set_size(font_system, None, Some((phys_lh + 4.0).max(1.0)));
            buf.set_text(font_system, text, &attrs, Shaping::Advanced, None);
            buf.shape_until_scroll(font_system, false);
            map.1.insert(key, buf);
        }
        let buf = map.1.get_mut(&key).expect("just inserted");
        blit_shaped_buffer(pixmap, font_system, swash_cache, buf, phys_x, phys_y, ct_color);
        buf.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0)
    });

    // Draw text decorations (underline, overline, strikethrough)
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
///
/// `pub(crate)` so `canvas::effects` can reach the colour maths instead of
/// keeping a second copy of it. Blur is still `0 => {}` here; the canvas side
/// now has a real one (`canvas::effects::blur_pixmap`) and dispatches to it
/// before ever reaching this function, so the two are not in disagreement —
/// this one is simply not the caller that can do it yet.
pub(crate) fn apply_pixel_filter(pm: &mut Pixmap, filter_type: u8, value: f32) {
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
