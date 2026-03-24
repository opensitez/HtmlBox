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
    let mut opacity_stack: Vec<f32> = Vec::new();
    let mut layer_stack: Vec<Layer> = Vec::new();

    // Current drawing target — either the main pixmap or a layer
    // We use a pointer approach to avoid borrow issues
    let pw = pixmap.width();
    let ph = pixmap.height();

    for cmd in &list.commands {
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
                        target.fill_path(&path, &paint, FillRule::Winding, ts, None);
                    }
                } else if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                    target.fill_rect(r, &paint, ts, None);
                }
            }

            PaintCmd::Border { rect, widths, colors, styles: _, radii } => {
                let alpha = opacity_stack.iter().product::<f32>().min(1.0);
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                let mut paint = Paint::default();
                if widths[0] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[0], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, widths[0]) {
                        target.fill_rect(r, &paint, ts, None);
                    }
                }
                if widths[2] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[2], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y + rect.h - widths[2], rect.w, widths[2]) {
                        target.fill_rect(r, &paint, ts, None);
                    }
                }
                if widths[3] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[3], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y, widths[3], rect.h) {
                        target.fill_rect(r, &paint, ts, None);
                    }
                }
                if widths[1] > 0.0 {
                    paint.set_color(to_sk_color(&apply_opacity(&colors[1], alpha)));
                    if let Some(r) = SkRect::from_xywh(rect.x + rect.w - widths[1], rect.y, widths[1], rect.h) {
                        target.fill_rect(r, &paint, ts, None);
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
                        None,
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
                        pixmap.fill_rect(r, &paint, ts, None);
                    }
                }
            }

            PaintCmd::BeginStackingContext { .. } => {}
            PaintCmd::EndStackingContext => {}

            PaintCmd::Gradient { rect, gradient_type, angle, stops, radii, opacity: grad_opacity, blend_mode: _ } => {
                // TODO: full gradient replay (linear/radial with stops)
                // For now, draw a simple fill with the first stop color as fallback
                if let Some((color, _)) = stops.first() {
                    let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                    let mut paint = Paint::default();
                    paint.set_color(to_sk_color(&apply_opacity(color, a2 * grad_opacity)));
                    let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                    if let Some(r) = SkRect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                        target.fill_rect(r, &paint, ts, None);
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
                    target.stroke_path(&path, &paint, &stroke, ts, None);
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
                    target.stroke_path(&path, &paint, &stroke, ts, None);
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
                            target.fill_rect(r, &paint, ts, None);
                        }
                    }
                    1 => {
                        // circle (filled as fallback)
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&c));
                        if let Some(r) = SkRect::from_xywh(x - size, y - size, size * 2.0, size * 2.0) {
                            target.fill_rect(r, &paint, ts, None);
                        }
                    }
                    2 => {
                        // square
                        let mut paint = Paint::default();
                        paint.set_color(to_sk_color(&c));
                        let half = size / 2.0;
                        if let Some(r) = SkRect::from_xywh(x - half, y - half, *size, *size) {
                            target.fill_rect(r, &paint, ts, None);
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
                // TODO: full form element replay — for now draw a border and value text
                let a2 = opacity_stack.iter().product::<f32>().min(1.0);
                let target = layer_stack.last_mut().map(|l| &mut l.pixmap).unwrap_or(pixmap);
                // Draw border
                let mut paint = Paint::default();
                paint.set_color_rgba8(169, 169, 169, 200);
                let mut stroke = tiny_skia::Stroke::default();
                stroke.width = 1.0;
                if let Some(path) = rounded_rect_path(rect.x, rect.y, rect.w, rect.h, 2.0) {
                    target.stroke_path(&path, &paint, &stroke, ts, None);
                }
                // Draw value text
                let display_text = if value.is_empty() { placeholder } else { value };
                if !display_text.is_empty() {
                    if let Some((ref mut fs, ref mut sc)) = text_ctx {
                        let c = apply_opacity(color, a2);
                        draw_text_cmd(
                            target, *fs, *sc, scale,
                            rect.x + 2.0, rect.y, display_text, font_family, *font_size, *font_weight,
                            0, 100.0, *font_size * 1.2,
                            &c, &super::display_list::TextDecoration::default(),
                            0.0, false,
                        );
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
                // Draw background image
                let (rgba, iw, ih) = match data {
                    ImageRef::Owned(d, w, h) => (d.as_slice(), *w, *h),
                    ImageRef::Shared(d, w, h) => (d.as_slice(), *w, *h),
                };
                if iw == 0 || ih == 0 || *draw_w <= 0.0 || *draw_h <= 0.0 { continue; }
                if let Some(img_pixmap) = tiny_skia::PixmapRef::from_bytes(rgba, iw, ih) {
                    let sx_img = draw_w / iw as f32;
                    let sy_img = draw_h / ih as f32;
                    let img_ts = ts.pre_translate(*pos_x, *pos_y).pre_scale(sx_img, sy_img);
                    pixmap.draw_pixmap(0, 0, img_pixmap,
                        &tiny_skia::PixmapPaint::default(), img_ts, None);
                    // TODO: repeat_x/repeat_y tiling
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
