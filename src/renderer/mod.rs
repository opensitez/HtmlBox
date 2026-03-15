use tiny_skia::{
    FillRule, LineCap, Mask, Paint, PathBuilder, Pixmap, Rect as SkRect, Stroke, Transform,
};
use cosmic_text::{
    Attrs, Buffer, Color as CTextColor, FontSystem, Metrics, Shaping, SwashCache,
    Style as CTextStyle, Weight,
};
use crate::types::*;
use crate::layout::inline_layout::collect_flat_text;

const SCROLLBAR_WIDTH: f32 = 10.0;

// ─── Renderer ─────────────────────────────────────────────────────────────────

pub struct Renderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub component_registry: ComponentRegistry,
    scale: f32,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            component_registry: ComponentRegistry::default(),
            scale: 1.0,
        }
    }

    pub fn register_component(&mut self, tag: &str, measure: ComponentMeasureFn, paint: ComponentPaintFn) {
        self.component_registry.register(tag, measure, paint);
    }

    /// Returns a transform that upscales logical-pixel coordinates to physical pixels.
    #[inline]
    fn ts(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
    }

    /// Render the full document onto a pixmap.
    ///
    /// `scale` — HiDPI scale factor (physical pixels / logical pixel); pass the value
    /// provided by `Platform::render`.
    ///
    /// `caret_info` — `Some((box_ptr, local_byte_offset))` where `box_ptr` is a raw pointer
    /// to the `HtmlBox` that owns the caret and `local_byte_offset` is the byte index within
    /// that box's flat text.  Mirrors C++ `Render(... caretPos, caretVisible, hasFocus)`.
    pub fn render(
        &mut self,
        doc:          &Document,
        pixmap:       &mut Pixmap,
        scale:        f32,
        scroll_x:     f32,
        scroll_y:     f32,
        sel_start:    Option<usize>,
        sel_end:      Option<usize>,
        caret_info:   Option<(*const HtmlBox, usize)>,
        caret_visible: bool,
        has_focus:    bool,
    ) {
        self.scale = scale;
        pixmap.fill(tiny_skia::Color::WHITE);
        // Clip rect in logical pixels (layout coordinates).
        let w = pixmap.width()  as f32 / self.scale;
        let h = pixmap.height() as f32 / self.scale;
        let clip = Rect::new(0.0, 0.0, w, h);

        self.render_box(
            &doc.root, pixmap,
            scroll_x, scroll_y,
            &clip,
            /* parent_mask */ None,
            sel_start, sel_end,
            std::ptr::null(),
        );

        // ── Caret ─────────────────────────────────────────────────────────────
        if caret_visible && has_focus {
            if let Some((caret_box_ptr, caret_local)) = caret_info {
                self.draw_caret(
                    &doc.root, pixmap,
                    scroll_x, scroll_y,
                    caret_box_ptr, caret_local,
                );
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Main per-box render  (mirrors RenderBox in C++)
    // ─────────────────────────────────────────────────────────────────────────

    fn render_box(
        &mut self,
        node:         &HtmlBox,
        pixmap:       &mut Pixmap,
        scroll_x:     f32,
        scroll_y:     f32,
        clip:         &Rect,
        parent_mask:  Option<&Mask>,
        sel_start:    Option<usize>,
        sel_end:      Option<usize>,
        hovered_ptr:  *const HtmlBox,
    ) {
        if matches!(node.style.display, Display::None) { return; }
        if !node.style.visibility { return; }
        if node.style.opacity <= 0.0 { return; }

        let sx = scroll_x;
        let sy = scroll_y;
        let br = node.border_rect;

        // ── Viewport culling ──────────────────────────────────────────────────
        if matches!(node.style.position, Position::Static | Position::Relative) {
            let bx = br.x - sx;
            let by = br.y - sy;
            if bx + br.w < clip.x || by + br.h < clip.y
                || bx > clip.right() || by > clip.bottom()
            {
                return;
            }
        }

        let pr      = node.padding_rect;
        let px      = pr.x - sx;
        let py      = pr.y - sy;
        let pw      = pr.w;
        let ph      = pr.h;
        let font_px = node.style.font_size_px(16.0, 16.0);
        let radius  = {
            let r = node.style.border_radius.resolve(font_px, pr.w, 16.0);
            if r > 0.0 { r } else {
                // Fallback to corners if shorthand is zero
                node.style.border_top_left_radius.resolve(font_px, pr.w, 16.0)
                    .max(node.style.border_top_right_radius.resolve(font_px, pr.w, 16.0))
                    .max(node.style.border_bottom_left_radius.resolve(font_px, pr.w, 16.0))
                    .max(node.style.border_bottom_right_radius.resolve(font_px, pr.w, 16.0))
            }
        };

        // ── Hover check (exact box match; C++ traverses parent chain) ─────────
        let is_hovered = !hovered_ptr.is_null()
            && (node.style.hover_background_color.is_some()
                || node.style.hover_color.is_some())
            && std::ptr::eq(node as *const HtmlBox, hovered_ptr);

        // ── Clip-path ─────────────────────────────────────────────────────────
        // Build a pixmap-sized clip mask from clip-path shape (if any).
        let clip_path_mask = make_clip_path_mask(pixmap, node, px, py, pw, ph, font_px, self.scale);
        // Effective mask: prefer clip-path mask, fall back to parent mask.
        let eff_mask: Option<&Mask> = if let Some(ref m) = clip_path_mask {
            Some(m)
        } else {
            parent_mask
        };

        // ── Outer box-shadow (before background) ─────────────────────────────
        if let Some(ref bs) = node.style.box_shadow {
            if !bs.inset {
                let shadow_x = px + bs.offset_x - bs.spread;
                let shadow_y = py + bs.offset_y - bs.spread;
                let shadow_w = pw + 2.0 * bs.spread;
                let shadow_h = ph + 2.0 * bs.spread;
                let layers   = ((bs.blur / 2.0) as i32).max(1);
                let base_a   = bs.color.a;
                for i in (0..=layers).rev() {
                    let la = ((base_a as i32) / (layers + 1)) as u8;
                    let sc = Color::rgba(bs.color.r, bs.color.g, bs.color.b, la);
                    let expand = i as f32;
                    let sx2 = shadow_x - expand;
                    let sy2 = shadow_y - expand;
                    let sw2 = shadow_w + 2.0 * expand;
                    let sh2 = shadow_h + 2.0 * expand;
                    let mut paint = Paint::default();
                    paint.set_color(sc.to_tiny_skia());
                    paint.anti_alias = true;
                    if radius > 0.0 {
                        if let Some(path) = rounded_rect_path(sx2, sy2, sw2, sh2, radius) {
                            pixmap.fill_path(&path, &paint, FillRule::Winding,
                                Transform::from_scale(self.scale, self.scale), eff_mask);
                        }
                    } else if let Some(r) = SkRect::from_xywh(sx2, sy2, sw2, sh2) {
                        pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), eff_mask);
                    }
                }
            }
        }

        // ── Background ───────────────────────────────────────────────────────
        {
            let raw_bg = if is_hovered {
                node.style.hover_background_color.unwrap_or(node.style.background_color)
            } else {
                node.style.background_color
            };
            let opacity = node.style.opacity;
            if raw_bg.a > 0 {
                let alpha = ((raw_bg.a as f32) * opacity) as u8;
                let bg = Color::rgba(raw_bg.r, raw_bg.g, raw_bg.b, alpha);
                let mut paint = Paint::default();
                paint.set_color(bg.to_tiny_skia());
                paint.anti_alias = true;
                if radius > 0.0 {
                    if let Some(path) = rounded_rect_path(px, py, pw, ph, radius) {
                        pixmap.fill_path(&path, &paint, FillRule::Winding,
                            Transform::from_scale(self.scale, self.scale), eff_mask);
                    }
                } else if let Some(r) = SkRect::from_xywh(px, py, pw, ph) {
                    pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), eff_mask);
                }
            }
        }

        // ── Gradient background ──────────────────────────────────────────────
        if node.style.gradient_type != GradientType::None
            && node.style.gradient_stops.len() >= 2
        {
            self.draw_gradient(node, pixmap, sx, sy);
        }

        // ── Inset box-shadow (after background, before borders) ───────────────
        if let Some(ref bs) = node.style.box_shadow {
            if bs.inset {
                // Draw as a darker border-like effect inside the padding box.
                let layers = ((bs.blur / 2.0) as i32).max(1);
                let base_a = bs.color.a;
                for i in 0..=layers {
                    let la = ((base_a as i32) / (layers + 1)) as u8;
                    let sc = Color::rgba(bs.color.r, bs.color.g, bs.color.b, la);
                    let shrink = i as f32;
                    let ix = px + bs.offset_x + bs.spread + shrink;
                    let iy = py + bs.offset_y + bs.spread + shrink;
                    let iw = (pw - 2.0 * (bs.spread + shrink)).max(0.0);
                    let ih = (ph - 2.0 * (bs.spread + shrink)).max(0.0);
                    if iw < 1.0 || ih < 1.0 { break; }
                    let mut paint = Paint::default();
                    paint.set_color(sc.to_tiny_skia());
                    paint.anti_alias = true;
                    let mut stroke = Stroke::default();
                    stroke.width = 1.0;
                    if let Some(path) = rect_path(ix, iy, iw, ih) {
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), eff_mask);
                    }
                }
            }
        }

        // ── Borders ──────────────────────────────────────────────────────────
        self.draw_borders_masked(node, pixmap, sx, sy, eff_mask);

        // ── Outline ──────────────────────────────────────────────────────────
        if node.style.outline_width > 0.0 && node.style.outline_style != BorderStyle::None {
            let br2 = node.border_rect;
            let ofs = node.style.outline_offset;
            let ow  = node.style.outline_width;
            let rx  = br2.x - sx - ofs - ow;
            let ry  = br2.y - sy - ofs - ow;
            let rw  = br2.w + 2.0 * (ofs + ow);
            let rh  = br2.h + 2.0 * (ofs + ow);
            let mut paint = Paint::default();
            paint.set_color(node.style.outline_color.to_tiny_skia());
            paint.anti_alias = true;
            let mut stroke = Stroke::default();
            stroke.width = ow;
            match node.style.outline_style {
                BorderStyle::Dashed => {
                    draw_dashed_line(pixmap, &paint, ow, rx, ry, rx + rw, ry, self.scale);
                    draw_dashed_line(pixmap, &paint, ow, rx + rw, ry, rx + rw, ry + rh, self.scale);
                    draw_dashed_line(pixmap, &paint, ow, rx, ry + rh, rx + rw, ry + rh, self.scale);
                    draw_dashed_line(pixmap, &paint, ow, rx, ry, rx, ry + rh, self.scale);
                }
                BorderStyle::Dotted => {
                    draw_dotted_line(pixmap, &paint, ow, rx, ry, rx + rw, ry, self.scale);
                    draw_dotted_line(pixmap, &paint, ow, rx + rw, ry, rx + rw, ry + rh, self.scale);
                    draw_dotted_line(pixmap, &paint, ow, rx, ry + rh, rx + rw, ry + rh, self.scale);
                    draw_dotted_line(pixmap, &paint, ow, rx, ry, rx, ry + rh, self.scale);
                }
                _ => {
                    if let Some(path) = rect_path(rx, ry, rw, rh) {
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), eff_mask);
                    }
                }
            }
        }

        // ── Build overflow clip for children ──────────────────────────────────
        // For overflow:hidden/scroll/auto, children are culled via a tighter clip rect.
        // We also build a Mask so pixel-level clips work for children that partially overlap.
        let overflow_clips = matches!(
            node.style.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto
        ) || matches!(
            node.style.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto
        );
        let overflow_mask = if overflow_clips {
            make_overflow_clip_mask(pixmap, px, py, pw, ph, radius, self.scale)
        } else {
            None
        };
        // Effective child mask: prefer overflow mask if present, else propagate eff_mask.
        let child_mask: Option<&Mask> = if let Some(ref om) = overflow_mask {
            Some(om)
        } else {
            eff_mask
        };

        // Tighter clip rect for children when overflow is clipping.
        let child_clip = if overflow_clips {
            let cx1 = px.max(clip.x);
            let cy1 = py.max(clip.y);
            let cx2 = (px + pw).min(clip.right());
            let cy2 = (py + ph).min(clip.bottom());
            Rect::new(cx1, cy1, (cx2 - cx1).max(0.0), (cy2 - cy1).max(0.0))
        } else {
            *clip
        };

        // ── Per-element scroll: children are shifted by the element's scroll ──
        let child_sx = sx + node.scroll_left;
        let child_sy = sy + node.scroll_top;

        // ── ::before pseudo-element ───────────────────────────────────────────
        if !node.style.before_content.is_empty() && !node.line_cache.is_empty() {
            let first = &node.line_cache[0];
            let tx = first.x - sx;
            let ty = first.y - sy;
            let ps = node.style.before_style.as_deref().unwrap_or(&node.style);
            let ps_font_px = { let f = ps.font_size.resolve(font_px, 0.0, 16.0); if f > 0.0 { f } else { font_px } };
            let line_h = ps.line_height.resolve(ps_font_px, 0.0, 16.0).max(ps_font_px * 1.2);
            let fc = ps.color;
            let ct_col = CTextColor::rgba(fc.r, fc.g, fc.b, ((fc.a as f32) * ps.opacity) as u8);
            self.draw_text_run(
                &node.style.before_content.clone(), tx, ty, ps_font_px, line_h,
                ps.font_weight, ps.font_style, ct_col, pixmap, eff_mask,
            );
        }

        // ── Inline content (text lines + selection) ───────────────────────────
        if !node.line_cache.is_empty() {
            let flat = collect_flat_text(node);
            self.draw_inline_content(
                node, &flat, pixmap, sx, sy,
                sel_start, sel_end,
                is_hovered, eff_mask,
            );
        }

        // ── ::after pseudo-element ────────────────────────────────────────────
        if !node.style.after_content.is_empty() && !node.line_cache.is_empty() {
            let last = &node.line_cache[node.line_cache.len() - 1];
            let tx = last.x - sx + last.width;
            let ty = last.y - sy;
            let ps = node.style.after_style.as_deref().unwrap_or(&node.style);
            let ps_font_px = { let f = ps.font_size.resolve(font_px, 0.0, 16.0); if f > 0.0 { f } else { font_px } };
            let line_h = ps.line_height.resolve(ps_font_px, 0.0, 16.0).max(ps_font_px * 1.2);
            let fc = ps.color;
            let ct_col = CTextColor::rgba(fc.r, fc.g, fc.b, ((fc.a as f32) * ps.opacity) as u8);
            self.draw_text_run(
                &node.style.after_content.clone(), tx, ty, ps_font_px, line_h,
                ps.font_weight, ps.font_style, ct_col, pixmap, eff_mask,
            );
        }

        // ── List marker ──────────────────────────────────────────────────────
        if node.style.display == Display::ListItem && !node.line_cache.is_empty() {
            self.draw_list_marker(node, pixmap, sx, sy, eff_mask);
        }

        // ── HR ───────────────────────────────────────────────────────────────
        if node.tag == "hr" {
            self.draw_hr(node, pixmap, sx, sy, eff_mask);
        }

        // ── Custom Component Painting ────────────────────────────────────────
        if let Some(callbacks) = self.component_registry.map.get(&node.tag) {
            let cr = node.content_rect;
            (callbacks.paint)(node, pixmap, cr.x - sx, cr.y - sy, cr.w, cr.h, self.scale);
        }

        // ── Image placeholder for <img> ─────────────────────────────────────
        if node.tag == "img" {
            let cr = node.content_rect;
            eprintln!("[RENDER img] content_rect=({},{},{},{}) has_data={} iw={} ih={}",
                      cr.x, cr.y, cr.w, cr.h,
                      node.image_data.is_some(), node.image_width, node.image_height);
            self.draw_image_placeholder(node, pixmap, sx, sy, eff_mask);
        }

        // ── Children: non-positioned first, then positioned by z-index ───────
        let has_positioned = node.children.iter().any(|c|
            c.style.is_positioned() && !matches!(c.style.display, Display::None));

        if !has_positioned {
            for child in &node.children {
                if !matches!(child.style.display, Display::None) {
                    self.render_box(
                        child, pixmap, child_sx, child_sy,
                        &child_clip, child_mask,
                        sel_start, sel_end, hovered_ptr,
                    );
                }
            }
        } else {
            // Non-positioned first
            for child in &node.children {
                if !matches!(child.style.display, Display::None) && !child.style.is_positioned() {
                    self.render_box(
                        child, pixmap, child_sx, child_sy,
                        &child_clip, child_mask,
                        sel_start, sel_end, hovered_ptr,
                    );
                }
            }
            // Positioned sorted by z-index
            let mut positioned: Vec<&HtmlBox> = node.children.iter()
                .filter(|c| c.style.is_positioned() && !matches!(c.style.display, Display::None))
                .collect();
            positioned.sort_by_key(|c| c.style.z_index);
            for child in positioned {
                let (csx, csy) = if child.style.position == Position::Fixed {
                    (0.0, 0.0)
                } else {
                    (child_sx, child_sy)
                };
                // Fixed/absolute positioned children are not constrained by overflow clip
                self.render_box(
                    child, pixmap, csx, csy,
                    clip, eff_mask,
                    sel_start, sel_end, hovered_ptr,
                );
            }
        }

        // ── Scrollbars ────────────────────────────────────────────────────────
        self.draw_scrollbars(node, pixmap, sx, sy);
    }

    // ─── Borders (mask-aware) ────────────────────────────────────────────────

    fn draw_borders_masked(
        &self,
        node:   &HtmlBox,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        mask:   Option<&Mask>,
    ) {
        let br      = node.border_rect;
        let font_px = node.style.font_size_px(16.0, 16.0);
        let radius  = node.style.border_radius.resolve(font_px, br.w, 16.0);
        let rx      = br.x - sx;
        let ry      = br.y - sy;

        let all_same = node.style.border_top_style    == node.style.border_right_style
            && node.style.border_right_style  == node.style.border_bottom_style
            && node.style.border_bottom_style == node.style.border_left_style
            && node.style.border_top_color    == node.style.border_right_color
            && node.style.border_right_color  == node.style.border_bottom_color
            && node.style.border_bottom_color == node.style.border_left_color;

        let opacity = node.style.opacity;

        if all_same && node.style.border_top_style != BorderStyle::None {
            let tw = node.style.border_top_width.resolve(font_px, br.w, 16.0).max(1.0);
            let c  = node.style.border_top_color;
            let ca = ((c.a as f32) * opacity) as u8;
            let mut paint = Paint::default();
            paint.set_color(Color::rgba(c.r, c.g, c.b, ca).to_tiny_skia());
            paint.anti_alias = true;
            let mut stroke = Stroke::default();
            stroke.width = tw;

            if radius > 0.0 {
                if let Some(path) = rounded_rect_path(
                    rx + tw/2.0, ry + tw/2.0, br.w - tw, br.h - tw, radius,
                ) {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
                }
            } else if let Some(path) = rect_path(rx, ry, br.w, br.h) {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
            }
        } else {
            self.draw_border_side_masked(pixmap, sx, sy, node, Side::Top,    opacity, mask);
            self.draw_border_side_masked(pixmap, sx, sy, node, Side::Right,  opacity, mask);
            self.draw_border_side_masked(pixmap, sx, sy, node, Side::Bottom, opacity, mask);
            self.draw_border_side_masked(pixmap, sx, sy, node, Side::Left,   opacity, mask);
        }
    }

    fn draw_border_side_masked(
        &self,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        node:   &HtmlBox,
        side:   Side,
        opacity: f32,
        mask:   Option<&Mask>,
    ) {
        let (style, color, width_l) = match side {
            Side::Top    => (node.style.border_top_style,    node.style.border_top_color,    &node.style.border_top_width),
            Side::Right  => (node.style.border_right_style,  node.style.border_right_color,  &node.style.border_right_width),
            Side::Bottom => (node.style.border_bottom_style, node.style.border_bottom_color, &node.style.border_bottom_width),
            Side::Left   => (node.style.border_left_style,   node.style.border_left_color,   &node.style.border_left_width),
        };
        if style == BorderStyle::None || style == BorderStyle::Hidden { return; }
        let font_px = node.style.font_size_px(16.0, 16.0);
        let w = width_l.resolve(font_px, node.border_rect.w, 16.0);
        if w < 0.5 { return; }

        let br = node.border_rect;
        let rx = br.x - sx;
        let ry = br.y - sy;
        let ca = ((color.a as f32) * opacity) as u8;
        let color2 = Color::rgba(color.r, color.g, color.b, ca);

        let (x1, y1, x2, y2) = match side {
            Side::Top    => (rx,                ry + w/2.0,           rx + br.w,          ry + w/2.0),
            Side::Bottom => (rx,                ry + br.h - w/2.0,    rx + br.w,          ry + br.h - w/2.0),
            Side::Left   => (rx + w/2.0,        ry,                   rx + w/2.0,         ry + br.h),
            Side::Right  => (rx + br.w - w/2.0, ry,                   rx + br.w - w/2.0,  ry + br.h),
        };

        let mut paint = Paint::default();
        paint.set_color(color2.to_tiny_skia());
        paint.anti_alias = true;
        let mut stroke = Stroke::default();
        stroke.width = w;
        stroke.line_cap = LineCap::Square;

        match style {
            BorderStyle::Dashed => draw_dashed_line(pixmap, &paint, w, x1, y1, x2, y2, self.scale),
            BorderStyle::Dotted => draw_dotted_line(pixmap, &paint, w, x1, y1, x2, y2, self.scale),
            BorderStyle::Double => {
                let third = w / 3.0;
                let mut s2 = Stroke::default();
                s2.width = third;
                if let Some(path) = line_path(x1, y1, x2, y2) {
                    pixmap.stroke_path(&path, &paint, &s2, Transform::from_scale(self.scale, self.scale), mask);
                }
                let (ix1, iy1, ix2, iy2) = match side {
                    Side::Top    => (x1, y1 + 2.0*third, x2, y2 + 2.0*third),
                    Side::Bottom => (x1, y1 - 2.0*third, x2, y2 - 2.0*third),
                    Side::Left   => (x1 + 2.0*third, y1, x2 + 2.0*third, y2),
                    Side::Right  => (x1 - 2.0*third, y1, x2 - 2.0*third, y2),
                };
                if let Some(path) = line_path(ix1, iy1, ix2, iy2) {
                    pixmap.stroke_path(&path, &paint, &s2, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            _ => {
                if let Some(path) = line_path(x1, y1, x2, y2) {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
        }
    }

    // ─── Gradient ────────────────────────────────────────────────────────────

    fn draw_gradient(&self, node: &HtmlBox, pixmap: &mut Pixmap, sx: f32, sy: f32) {
        let pr = node.padding_rect;
        let sc = self.scale;
        // Work in physical pixels: scale logical coords/sizes to match the pixmap.
        let pw = (pr.w * sc) as i32;
        let ph = (pr.h * sc) as i32;
        if pw <= 0 || ph <= 0 { return; }
        let ox = ((pr.x - sx) * sc) as i32;
        let oy = ((pr.y - sy) * sc) as i32;

        let stops = &node.style.gradient_stops;

        let interp_color = |t: f32| -> Color {
            let t = t.max(0.0).min(1.0);
            if t <= stops.first().unwrap().position { return stops.first().unwrap().color; }
            if t >= stops.last().unwrap().position  { return stops.last().unwrap().color;  }
            for i in 0..stops.len() - 1 {
                if t >= stops[i].position && t <= stops[i+1].position {
                    let range = stops[i+1].position - stops[i].position;
                    let f = if range > 0.0 { (t - stops[i].position) / range } else { 0.0 };
                    let c1 = stops[i].color;
                    let c2 = stops[i+1].color;
                    return Color::rgba(
                        (c1.r as f32 + (c2.r as f32 - c1.r as f32) * f) as u8,
                        (c1.g as f32 + (c2.g as f32 - c1.g as f32) * f) as u8,
                        (c1.b as f32 + (c2.b as f32 - c1.b as f32) * f) as u8,
                        (c1.a as f32 + (c2.a as f32 - c1.a as f32) * f) as u8,
                    );
                }
            }
            stops.last().unwrap().color
        };

        let pix_w = pixmap.width()  as i32;
        let pix_h = pixmap.height() as i32;
        let data  = pixmap.pixels_mut();

        match node.style.gradient_type {
            GradientType::Linear => {
                let angle = node.style.gradient_angle;
                let rad   = angle * std::f32::consts::PI / 180.0;
                let dx    = rad.sin();
                let dy    = -rad.cos();
                let corners  = [0.0f32, dx, dy, dx + dy];
                let t_min    = corners.iter().cloned().fold(f32::MAX, f32::min);
                let t_max    = corners.iter().cloned().fold(f32::MIN, f32::max);
                let t_range  = if (t_max - t_min) < 0.001 { 1.0 } else { t_max - t_min };

                for py2 in 0..ph {
                    let ny = py2 as f32 / (ph - 1).max(1) as f32;
                    for px2 in 0..pw {
                        let nx = px2 as f32 / (pw - 1).max(1) as f32;
                        let t  = (nx * dx + ny * dy - t_min) / t_range;
                        let c  = interp_color(t);
                        let screen_x = ox + px2;
                        let screen_y = oy + py2;
                        if screen_x >= 0 && screen_x < pix_w && screen_y >= 0 && screen_y < pix_h {
                            let idx = (screen_y * pix_w + screen_x) as usize;
                            let af  = c.a as f32 / 255.0;
                            let ex  = data[idx];
                            let nr  = (c.r as f32 * af + ex.red()   as f32 * (1.0 - af)) as u8;
                            let ng  = (c.g as f32 * af + ex.green() as f32 * (1.0 - af)) as u8;
                            let nb  = (c.b as f32 * af + ex.blue()  as f32 * (1.0 - af)) as u8;
                            if let Some(pv) = tiny_skia::PremultipliedColorU8::from_rgba(nr, ng, nb, 255) {
                                data[idx] = pv;
                            }
                        }
                    }
                }
            }
            GradientType::Radial => {
                let cx    = pw as f32 / 2.0;
                let cy    = ph as f32 / 2.0;
                let max_r = (cx * cx + cy * cy).sqrt().max(1.0);
                for py2 in 0..ph {
                    for px2 in 0..pw {
                        let dist = ((px2 as f32 - cx).powi(2) + (py2 as f32 - cy).powi(2)).sqrt();
                        let t    = dist / max_r;
                        let c    = interp_color(t);
                        let screen_x = ox + px2;
                        let screen_y = oy + py2;
                        if screen_x >= 0 && screen_x < pix_w && screen_y >= 0 && screen_y < pix_h {
                            let idx = (screen_y * pix_w + screen_x) as usize;
                            if let Some(pv) = tiny_skia::PremultipliedColorU8::from_rgba(c.r, c.g, c.b, c.a) {
                                data[idx] = pv;
                            }
                        }
                    }
                }
            }
            GradientType::None => {}
        }
    }

    // ─── Inline content ───────────────────────────────────────────────────────

    fn draw_inline_content(
        &mut self,
        node:      &HtmlBox,
        flat:      &str,
        pixmap:    &mut Pixmap,
        sx:        f32,
        sy:        f32,
        sel_start: Option<usize>,
        sel_end:   Option<usize>,
        is_hovered: bool,
        mask:      Option<&Mask>,
    ) {
        if node.line_cache.is_empty() || flat.is_empty() { return; }

        let opacity             = node.style.opacity;
        let fallback_font_px    = node.style.font_size_px(16.0, 16.0);
        let fallback_letter_spc = node.style.letter_spacing.resolve(fallback_font_px, 0.0, 16.0);
        let fallback_color      = if is_hovered && node.style.hover_color.is_some() {
            node.style.hover_color.unwrap()
        } else {
            node.style.color
        };
        let _fallback_ct_color = CTextColor::rgba(
            fallback_color.r, fallback_color.g, fallback_color.b,
            ((fallback_color.a as f32) * opacity) as u8,
        );

        let (sel_min, sel_max) = match (sel_start, sel_end) {
            (Some(s), Some(e)) => (s.min(e), s.max(e)),
            _ => (0, 0),
        };

        let use_ellipsis = node.style.text_overflow == TextOverflow::Ellipsis
            && (node.style.overflow_x == Overflow::Hidden
                || node.style.overflow_x == Overflow::Scroll);
        let max_content_w = node.padding_rect.w;

        for line in node.line_cache.clone() {
            let line_start = floor_char_boundary(flat, line.text_start.min(flat.len()));
            let line_end   = floor_char_boundary(flat, (line.text_start + line.text_length).min(flat.len()));
            if line_start >= line_end { continue; }
            if flat[line_start..line_end].trim().is_empty() { continue; }

            let lx = line.x - sx;
            let ly = line.y - sy;

            // ── Selection highlight ──────────────────────────────────────────
            if sel_min < sel_max && sel_min < line_end && sel_max > line_start {
                let h_start = sel_min.max(line_start);
                let h_end   = sel_max.min(line_end);
                if h_start < h_end {
                    let mut sel_paint = Paint::default();
                    if let Some(ss) = node.style.selection_style.as_deref() {
                        let bg = ss.background_color;
                        sel_paint.set_color_rgba8(bg.r, bg.g, bg.b, if bg.a > 0 { bg.a } else { 200 });
                    } else {
                        sel_paint.set_color_rgba8(200, 220, 255, 200);
                    }
                    if !line.visual_segments.is_empty() {
                        for vs in &line.visual_segments {
                            let seg_s = vs.logical_start;
                            let seg_e = vs.logical_start + vs.length;
                            let hl_s  = h_start.max(seg_s);
                            let hl_e  = h_end.min(seg_e);
                            if hl_s >= hl_e { continue; }
                            let frac_s = if vs.length > 0 { (hl_s - seg_s) as f32 / vs.length as f32 } else { 0.0 };
                            let frac_e = if vs.length > 0 { (hl_e - seg_s) as f32 / vs.length as f32 } else { 1.0 };
                            let xs = vs.x + frac_s * vs.width;
                            let xe = vs.x + frac_e * vs.width;
                            let (xl, xr) = if xs < xe { (xs, xe) } else { (xe, xs) };
                            if let Some(r) = SkRect::from_xywh(lx + xl, ly, xr - xl, line.height) {
                                pixmap.fill_rect(r, &sel_paint, Transform::from_scale(self.scale, self.scale), mask);
                            }
                        }
                    } else {
                        let len     = line_end - line_start;
                        let ratio_s = if len > 0 { (h_start - line_start) as f32 / len as f32 } else { 0.0 };
                        let ratio_e = if len > 0 { (h_end   - line_start) as f32 / len as f32 } else { 1.0 };
                        let xs = lx + ratio_s * line.width;
                        let xe = lx + ratio_e * line.width;
                        if let Some(r) = SkRect::from_xywh(xs, ly, xe - xs, line.height) {
                            pixmap.fill_rect(r, &sel_paint, Transform::from_scale(self.scale, self.scale), mask);
                        }
                    }
                }
            }

            // ── Text rendering ───────────────────────────────────────────────
            // Build rendering order: visual segments if BiDi, otherwise logical.
            struct Chunk { s: usize, e: usize, run_idx: Option<usize>, #[allow(dead_code)] rtl: bool }
            let mut chunks: Vec<Chunk> = Vec::new();

            if !line.visual_segments.is_empty() && !node.inline_runs.is_empty() {
                for vs in &line.visual_segments {
                    let seg_s   = vs.logical_start;
                    let seg_e   = vs.logical_start + vs.length;
                    let is_rtl  = (vs.level & 1) != 0;
                    let mut seg_chunks: Vec<Chunk> = Vec::new();
                    for (ri, run) in node.inline_runs.iter().enumerate() {
                        let rs  = run.text_offset;
                        let re  = rs + run.length;
                        let cs  = seg_s.max(rs);
                        let ce  = seg_e.min(re);
                        if cs < ce {
                            seg_chunks.push(Chunk { s: cs, e: ce, run_idx: Some(ri), rtl: is_rtl });
                        }
                    }
                    if is_rtl { seg_chunks.reverse(); }
                    chunks.extend(seg_chunks);
                }
            } else if node.inline_runs.is_empty() {
                chunks.push(Chunk { s: line_start, e: line_end, run_idx: None, rtl: false });
            } else {
                for (ri, run) in node.inline_runs.iter().enumerate() {
                    let cs = line_start.max(run.text_offset);
                    let ce = line_end.min(run.text_offset + run.length);
                    if cs < ce {
                        chunks.push(Chunk { s: cs, e: ce, run_idx: Some(ri), rtl: false });
                    }
                }
            }

            let mut cursor_x = lx;

            for chunk in &chunks {
                let s = floor_char_boundary(flat, chunk.s);
                let e = floor_char_boundary(flat, chunk.e);
                if e <= s { continue; }

                let (run_style, run_font_px, run_letter_spc, run_word_spc, run_extra) =
                    if let Some(ri) = chunk.run_idx {
                        let run = &node.inline_runs[ri];
                        let fp  = run.style.font_size_px(16.0, 16.0);
                        let ls  = run.style.letter_spacing.resolve(fp, 0.0, 16.0);
                        let ws  = run.style.word_spacing.resolve(fp, 0.0, 16.0);
                        (Some(&run.style), fp, ls, ws, line.extra_space_per_word)
                    } else {
                        (None, fallback_font_px, fallback_letter_spc, 0.0, line.extra_space_per_word)
                    };

                let style_ref: &ComputedStyle = run_style.unwrap_or(&node.style);
                let seg_text = &flat[s..e];
                let draw_text = apply_text_transform(seg_text, style_ref.text_transform);

                // Run background color
                if style_ref.background_color.a > 0 {
                    let seg_w = approx_text_width_ls(&draw_text, run_font_px, run_letter_spc);
                    let mut bp = Paint::default();
                    bp.set_color(style_ref.background_color.to_tiny_skia());
                    if let Some(r) = SkRect::from_xywh(cursor_x, ly, seg_w, line.height) {
                        pixmap.fill_rect(r, &bp, Transform::from_scale(self.scale, self.scale), mask);
                    }
                }

                // Text color (hover override applies to all runs)
                let run_color = if is_hovered && node.style.hover_color.is_some() {
                    node.style.hover_color.unwrap()
                } else {
                    style_ref.color
                };
                let ct_color = CTextColor::rgba(
                    run_color.r, run_color.g, run_color.b,
                    ((run_color.a as f32) * opacity) as u8,
                );

                let run_line_h = style_ref.line_height.resolve(run_font_px, 0.0, 16.0)
                    .max(run_font_px * 1.2);

                // Compute segment width (used for decorations and cursor advance)
                let seg_w = approx_text_width_ls(&draw_text, run_font_px, run_letter_spc);

                // text-overflow: ellipsis check
                let final_text = if use_ellipsis && cursor_x + seg_w > max_content_w {
                    let avail = max_content_w - cursor_x;
                    if avail > 0.0 {
                        truncate_with_ellipsis(&draw_text, run_font_px, run_letter_spc, avail)
                    } else {
                        String::from("…")
                    }
                } else {
                    draw_text.clone()
                };

                // Text shadow
                if let Some(ref ts) = style_ref.text_shadow {
                    let sh = CTextColor::rgba(ts.color.r, ts.color.g, ts.color.b, ts.color.a);
                    self.draw_text_run(
                        &final_text,
                        cursor_x + ts.offset_x, ly + ts.offset_y,
                        run_font_px, run_line_h,
                        style_ref.font_weight, style_ref.font_style,
                        sh, pixmap, mask,
                    );
                }

                // Main text
                self.draw_text_run(
                    &final_text, cursor_x, ly,
                    run_font_px, run_line_h,
                    style_ref.font_weight, style_ref.font_style,
                    ct_color, pixmap, mask,
                );

                // Per-segment text decorations
                self.draw_text_decorations_segment(
                    style_ref, cursor_x, ly, seg_w, line.height, line.ascent, opacity, pixmap, mask,
                );

                // Advance cursor with word/letter spacing
                cursor_x += advance_with_spacing(
                    &draw_text, run_font_px, run_letter_spc, run_word_spc, run_extra,
                );
            }

            // Whole-line decorations (when no per-run decorations)
            if node.inline_runs.is_empty() {
                self.draw_text_decorations_line(node, &line, lx, ly, opacity, pixmap, mask);
            }
        }
    }

    // ─── Text decorations (per segment) ──────────────────────────────────────

    fn draw_text_decorations_segment(
        &self,
        style:   &ComputedStyle,
        x:       f32,
        y:       f32,
        width:   f32,
        height:  f32,
        ascent:  f32,
        opacity: f32,
        pixmap:  &mut Pixmap,
        mask:    Option<&Mask>,
    ) {
        let dec = style.text_decoration;
        if !dec.underline && !dec.strikethrough && !dec.overline { return; }
        let font_px    = style.font_size_px(16.0, 16.0);
        let line_thick = (font_px / 12.0).max(1.0);
        let color      = style.color;
        let alpha      = ((color.a as f32) * opacity) as u8;
        let mut paint  = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, alpha);

        if dec.underline {
            let uy = y + ascent + line_thick * 2.0;
            if let Some(r) = SkRect::from_xywh(x, uy, width, line_thick) {
                pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
            }
        }
        if dec.strikethrough {
            let sy2 = y + ascent * 0.55;
            if let Some(r) = SkRect::from_xywh(x, sy2, width, line_thick) {
                pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
            }
        }
        if dec.overline {
            if let Some(r) = SkRect::from_xywh(x, y, width, line_thick) {
                pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
            }
        }
        let _ = height;
    }

    fn draw_text_decorations_line(
        &self,
        node:   &HtmlBox,
        line:   &LayoutLine,
        ox:     f32,
        oy:     f32,
        opacity: f32,
        pixmap: &mut Pixmap,
        mask:   Option<&Mask>,
    ) {
        let dec = node.style.text_decoration;
        if !dec.underline && !dec.strikethrough && !dec.overline { return; }

        let font_px = node.style.font_size_px(16.0, 16.0);
        let c  = node.style.color;
        let ca = ((c.a as f32) * opacity) as u8;
        let mut paint = Paint::default();
        paint.set_color(Color::rgba(c.r, c.g, c.b, ca).to_tiny_skia());
        let mut stroke = Stroke::default();
        stroke.width = (font_px * 0.08).max(1.0);

        if dec.underline {
            let uy = oy + line.ascent + 2.0;
            if let Some(path) = line_path(ox, uy, ox + line.width, uy) {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
            }
        }
        if dec.strikethrough {
            let sy2 = oy + line.ascent * 0.55;
            if let Some(path) = line_path(ox, sy2, ox + line.width, sy2) {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
            }
        }
        if dec.overline {
            if let Some(path) = line_path(ox, oy, ox + line.width, oy) {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
            }
        }
    }

    // ─── Text run (cosmic-text) ───────────────────────────────────────────────

    fn draw_text_run(
        &mut self,
        text:       &str,
        x:          f32,
        y:          f32,
        font_px:    f32,
        line_h:     f32,
        weight:     FontWeight,
        font_style: FontStyle,
        color:      CTextColor,
        pixmap:     &mut Pixmap,
        mask:       Option<&Mask>,
    ) {
        if text.is_empty() { return; }
        // Cosmic-text shapes at physical pixel sizes for correct sub-pixel rendering.
        let sc       = self.scale;
        let phys_px  = font_px  * sc;
        let phys_lh  = line_h   * sc;
        let metrics = Metrics::new(phys_px, phys_lh);
        let attrs = Attrs::new()
            .weight(if weight.is_bold() { Weight::BOLD } else { Weight::NORMAL })
            .style(match font_style {
                FontStyle::Italic  => CTextStyle::Italic,
                FontStyle::Oblique => CTextStyle::Oblique,
                FontStyle::Normal  => CTextStyle::Normal,
            });

        let mut buf = Buffer::new(&mut self.font_system, metrics);
        buf.set_size(
            &mut self.font_system,
            Some((approx_text_width(text, phys_px) + 4.0).max(1.0)),
            Some((phys_lh + 4.0).max(1.0)),
        );
        buf.set_text(&mut self.font_system, text, attrs, Shaping::Advanced);
        buf.shape_until_scroll(&mut self.font_system, false);

        // Glyph positions from cosmic-text are in physical pixels.
        // Draw them directly without a scale transform.
        let phys_x = x * sc;
        let phys_y = y * sc;

        #[derive(Clone)]
        struct G { x: f32, y: f32, w: f32, h: f32, r: u8, g: u8, b: u8, a: u8 }
        let mut glyphs: Vec<G> = Vec::new();

        buf.draw(&mut self.font_system, &mut self.swash_cache, color, |gx, gy, gw, gh, gc| {
            if gc.a() == 0 { return; }
            glyphs.push(G {
                x: phys_x + gx as f32, y: phys_y + gy as f32,
                w: gw as f32, h: gh as f32,
                r: gc.r(), g: gc.g(), b: gc.b(), a: gc.a(),
            });
        });

        for g in &glyphs {
            if let Some(r) = SkRect::from_xywh(g.x, g.y, g.w, g.h) {
                let mut paint = Paint::default();
                paint.set_color_rgba8(g.r, g.g, g.b, g.a);
                paint.anti_alias = true;
                // Glyphs are already in physical pixels; use identity transform.
                pixmap.fill_rect(r, &paint, Transform::identity(), mask);
            }
        }
    }

    // ─── List marker ─────────────────────────────────────────────────────────

    fn draw_list_marker(
        &mut self,
        node:   &HtmlBox,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        mask:   Option<&Mask>,
    ) {
        let ms         = node.style.marker_style.as_deref();
        let font_px    = ms.map(|s| s.font_size_px(16.0, 16.0)).unwrap_or_else(|| node.style.font_size_px(16.0, 16.0));
        let first_line = match node.line_cache.first() { Some(l) => l.clone(), None => return };
        let inside     = node.style.list_style_position == ListStylePosition::Inside;

        let c = ms.map(|s| s.color).unwrap_or(node.style.color);
        let mut paint = Paint::default();
        paint.set_color(c.to_tiny_skia());
        paint.anti_alias = true;

        match node.style.list_style_type {
            ListStyleType::Disc => {
                let bx = if inside { first_line.x - sx + 4.0 } else { first_line.x - sx - 10.0 };
                let by = first_line.y - sy + first_line.height / 2.0;
                if let Some(path) = circle_path(bx, by, 3.0) {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            ListStyleType::Circle => {
                let bx = if inside { first_line.x - sx + 4.0 } else { first_line.x - sx - 10.0 };
                let by = first_line.y - sy + first_line.height / 2.0;
                let mut stroke = Stroke::default();
                stroke.width = 1.0;
                if let Some(path) = circle_path(bx, by, 3.0) {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            ListStyleType::Square => {
                let bx = if inside { first_line.x - sx + 4.0 } else { first_line.x - sx - 10.0 };
                let by = first_line.y - sy + first_line.height / 2.0;
                let s  = 6.0f32;
                if let Some(r) = SkRect::from_xywh(bx - s/2.0, by - s/2.0, s, s) {
                    pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            ListStyleType::Decimal | ListStyleType::LowerAlpha | ListStyleType::UpperAlpha
            | ListStyleType::LowerRoman | ListStyleType::UpperRoman => {
                let marker   = format_list_marker(node.style.list_style_type, node.style.list_index);
                let marker_w = approx_text_width(&marker, font_px);
                let mx = if inside { first_line.x - sx } else { first_line.x - sx - marker_w - 4.0 };
                let my = first_line.y - sy;
                let line_h   = node.style.line_height.resolve(font_px, 0.0, 16.0).max(font_px * 1.2);
                let ct_color = CTextColor::rgba(c.r, c.g, c.b, c.a);
                self.draw_text_run(&marker, mx, my, font_px, line_h,
                    node.style.font_weight, node.style.font_style, ct_color, pixmap, mask);
            }
            ListStyleType::Disclosure => {
                let marker   = "▸";
                let marker_w = approx_text_width(marker, font_px);
                let mx = if inside { first_line.x - sx } else { first_line.x - sx - marker_w - 4.0 };
                let my = first_line.y - sy;
                let line_h   = node.style.line_height.resolve(font_px, 0.0, 16.0).max(font_px * 1.2);
                let ct_color = CTextColor::rgba(c.r, c.g, c.b, c.a);
                self.draw_text_run(marker, mx, my, font_px, line_h,
                    node.style.font_weight, node.style.font_style, ct_color, pixmap, mask);
            }
            ListStyleType::None => {}
        }
    }

    // ─── HR ──────────────────────────────────────────────────────────────────

    fn draw_hr(&self, node: &HtmlBox, pixmap: &mut Pixmap, sx: f32, sy: f32, mask: Option<&Mask>) {
        let cr = node.border_rect;
        let y  = cr.y + cr.h / 2.0 - sy;
        let mut paint = Paint::default();
        paint.set_color_rgba8(128, 128, 128, 255);
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        if let Some(path) = line_path(cr.x - sx, y, cr.right() - sx, y) {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
        }
    }

    // ─── Image drawing ───────────────────────────────────────────────────────

    fn draw_image_placeholder(
        &self,
        node:   &HtmlBox,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        mask:   Option<&Mask>,
    ) {
        let cr = node.content_rect;
        if cr.w <= 0.0 || cr.h <= 0.0 { return; }

        // If we have actual pixel data, draw it
        if let Some(data) = &node.image_data {
            if node.image_width > 0 && node.image_height > 0 {
                self.draw_image_data(
                    data, node.image_width, node.image_height,
                    cr, node.style.object_fit,
                    pixmap, sx, sy, mask,
                );
                return;
            }
        }

        // Fallback: draw grey placeholder with border
        let rx = cr.x - sx;
        let ry = cr.y - sy;
        let mut paint = Paint::default();
        paint.set_color_rgba8(220, 220, 220, 200);
        if let Some(r) = SkRect::from_xywh(rx, ry, cr.w, cr.h) {
            pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
        }
        paint.set_color_rgba8(180, 180, 180, 255);
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        if let Some(path) = rect_path(rx, ry, cr.w, cr.h) {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
        }
    }

    fn draw_image_data(
        &self,
        data:     &[u8],
        img_w:    u32,
        img_h:    u32,
        dest:     Rect,
        fit:      ObjectFit,
        pixmap:   &mut Pixmap,
        sx:       f32,
        sy:       f32,
        mask:     Option<&Mask>,
    ) {
        // Build a tiny_skia Pixmap from RGBA8 data
        // tiny_skia uses premultiplied alpha internally
        let mut src_pm = match Pixmap::new(img_w, img_h) {
            Some(p) => p,
            None => return,
        };
        // Copy, converting straight alpha to premultiplied
        {
            let pix = src_pm.pixels_mut();
            let src_len = (img_w * img_h * 4) as usize;
            if data.len() < src_len { return; }
            for (i, px) in pix.iter_mut().enumerate() {
                let base = i * 4;
                let r = data[base] as u32;
                let g = data[base + 1] as u32;
                let b = data[base + 2] as u32;
                let a = data[base + 3];
                // Premultiply
                let pr = ((r * a as u32 + 127) / 255) as u8;
                let pg = ((g * a as u32 + 127) / 255) as u8;
                let pb = ((b * a as u32 + 127) / 255) as u8;
                *px = tiny_skia::PremultipliedColorU8::from_rgba(pr, pg, pb, a)
                    .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
            }
        }

        let dest_x = dest.x - sx;
        let dest_y = dest.y - sy;
        let dest_w = dest.w;
        let dest_h = dest.h;

        let (draw_x, draw_y, draw_w, draw_h, clip_to_dest) = compute_object_fit_rect(
            img_w as f32, img_h as f32, dest_w, dest_h, dest_x, dest_y, fit,
        );

        // Build the scale transform: map src (img_w x img_h) → (draw_w x draw_h)
        let scale_x = draw_w / img_w as f32;
        let scale_y = draw_h / img_h as f32;
        let transform = Transform::from_scale(scale_x, scale_y)
            .pre_concat(Transform::from_translate(draw_x / scale_x, draw_y / scale_y));

        // If the image extends outside the dest rect (cover/none), we need a clip mask
        let clip_mask_storage;
        let final_mask: Option<&Mask>;
        if clip_to_dest {
            // Create an intersection mask: dest rect clipped
            let pw = pixmap.width();
            let ph = pixmap.height();
            if let Some(mut combined) = Mask::new(pw, ph) {
                // Fill the dest rect in the mask
                let mut pb = PathBuilder::new();
                pb.move_to(dest_x, dest_y);
                pb.line_to(dest_x + dest_w, dest_y);
                pb.line_to(dest_x + dest_w, dest_y + dest_h);
                pb.line_to(dest_x, dest_y + dest_h);
                pb.close();
                if let Some(clip_path) = pb.finish() {
                    combined.fill_path(&clip_path, FillRule::Winding, true, Transform::from_scale(self.scale, self.scale));
                }
                // Intersect with existing mask if any — tiny-skia 0.11 has no direct
                // intersect_with, so we manually AND each byte of the mask pixels
                if let Some(m) = mask {
                    let src = m.data();
                    let dst = combined.data_mut();
                    for (d, &s) in dst.iter_mut().zip(src.iter()) {
                        *d = ((*d as u16 * s as u16) / 255) as u8;
                    }
                }
                clip_mask_storage = Some(combined);
                final_mask = clip_mask_storage.as_ref();
            } else {
                final_mask = mask;
                clip_mask_storage = None;
            }
        } else {
            final_mask = mask;
            clip_mask_storage = None;
        }
        let _ = clip_mask_storage; // suppress unused warning when mask not stored

        let final_transform = Transform::from_scale(self.scale, self.scale).pre_concat(transform);
        pixmap.draw_pixmap(
            0, 0,
            src_pm.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            final_transform,
            final_mask,
        );
    }

    // ─── Scrollbars ──────────────────────────────────────────────────────────

    fn draw_scrollbars(&self, node: &HtmlBox, pixmap: &mut Pixmap, sx: f32, sy: f32) {
        let show_v = node.style.overflow_y == Overflow::Scroll
            || (node.style.overflow_y == Overflow::Auto && node.scroll_height > node.content_rect.h);
        let show_h = node.style.overflow_x == Overflow::Scroll
            || (node.style.overflow_x == Overflow::Auto && node.scroll_width > node.content_rect.w);
        if !show_v && !show_h { return; }

        let thumb_col = node.style.scrollbar_thumb_color.unwrap_or(Color::rgba(128, 128, 128, 160));
        let track_col = node.style.scrollbar_track_color.unwrap_or(Color::rgba(128, 128, 128, 40));

        let cr = node.content_rect;
        let cx = cr.x - sx;
        let cy = cr.y - sy;

        if show_v && node.scroll_height > cr.h {
            let track_h = cr.h;
            let thumb_h = (track_h * track_h / node.scroll_height).max(20.0);
            let max_s   = node.scroll_height - cr.h;
            let thumb_y = if max_s > 0.0 { node.scroll_top * (track_h - thumb_h) / max_s } else { 0.0 };
            let track_x = cx + cr.w - SCROLLBAR_WIDTH;
            let mut paint = Paint::default();
            paint.set_color(track_col.to_tiny_skia());
            if let Some(r) = SkRect::from_xywh(track_x, cy, SCROLLBAR_WIDTH, track_h) {
                pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), None);
            }
            paint.set_color(thumb_col.to_tiny_skia());
            if let Some(path) = rounded_rect_path(track_x + 1.0, cy + thumb_y + 1.0,
                    SCROLLBAR_WIDTH - 2.0, thumb_h - 2.0, 3.0) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::from_scale(self.scale, self.scale), None);
            }
        }

        if show_h && node.scroll_width > cr.w {
            let track_w = cr.w - if show_v { SCROLLBAR_WIDTH } else { 0.0 };
            let thumb_w = (track_w * track_w / node.scroll_width).max(20.0);
            let max_s   = node.scroll_width - cr.w;
            let thumb_x = if max_s > 0.0 { node.scroll_left * (track_w - thumb_w) / max_s } else { 0.0 };
            let track_y = cy + cr.h - SCROLLBAR_WIDTH;
            let mut paint = Paint::default();
            paint.set_color(track_col.to_tiny_skia());
            if let Some(r) = SkRect::from_xywh(cx, track_y, track_w, SCROLLBAR_WIDTH) {
                pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), None);
            }
            paint.set_color(thumb_col.to_tiny_skia());
            if let Some(path) = rounded_rect_path(cx + thumb_x + 1.0, track_y + 1.0,
                    thumb_w - 2.0, SCROLLBAR_WIDTH - 2.0, 3.0) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::from_scale(self.scale, self.scale), None);
            }
        }
    }

    // ─── Caret ───────────────────────────────────────────────────────────────
    // Mirrors C++ Render() caret section: finds the line containing caretPos
    // and draws a vertical line using the box's caret-color / color.

    fn draw_caret(
        &mut self,
        root:         &HtmlBox,
        pixmap:       &mut Pixmap,
        sx:           f32,
        sy:           f32,
        caret_box_ptr: *const HtmlBox,
        caret_local:  usize,
    ) {
        self.draw_caret_walk(root, pixmap, sx, sy, caret_box_ptr, caret_local);
    }

    fn draw_caret_walk(
        &mut self,
        node:          &HtmlBox,
        pixmap:        &mut Pixmap,
        sx:            f32,
        sy:            f32,
        caret_box_ptr: *const HtmlBox,
        caret_local:   usize,
    ) -> bool {
        if std::ptr::eq(node as *const HtmlBox, caret_box_ptr) {
            // Found the box; find its line
            let flat     = collect_flat_text(node);
            let font_px  = node.style.font_size_px(16.0, 16.0);
            let letter_s = node.style.letter_spacing.resolve(font_px, 0.0, 16.0);

            let mut caret_x    = node.border_rect.x - sx;
            let mut caret_y    = node.border_rect.y - sy;
            let mut caret_h    = font_px * 1.2;
            let mut found_line = false;

            for line in &node.line_cache {
                let line_end = line.text_start + line.text_length;
                if caret_local >= line.text_start && caret_local <= line_end {
                    caret_y = line.y - sy;
                    caret_h = line.height.max(font_px * 1.0);
                    // Measure from line start to caret_local
                    let from = floor_char_boundary(&flat, line.text_start.min(flat.len()));
                    let to   = floor_char_boundary(&flat, caret_local.min(flat.len()));
                    let pre  = if to > from { &flat[from..to] } else { "" };
                    let pre_w = approx_text_width_ls(pre, font_px, letter_s);
                    caret_x = line.x - sx + pre_w;
                    found_line = true;
                    break;
                }
            }
            if !found_line && !node.line_cache.is_empty() {
                let last = node.line_cache.last().unwrap();
                caret_y = last.y - sy;
                caret_h = last.height.max(font_px);
                caret_x = last.x - sx + last.width;
            }

            // Resolve caret color: caret-color > color > black
            let col = node.style.caret_color
                .unwrap_or(node.style.color);

            let mut paint = Paint::default();
            paint.set_color(col.to_tiny_skia());
            let mut stroke = Stroke::default();
            stroke.width = 1.5;
            if let Some(path) = line_path(caret_x, caret_y, caret_x, caret_y + caret_h) {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), None);
            }
            return true;
        }

        for child in &node.children {
            if self.draw_caret_walk(child, pixmap, sx, sy, caret_box_ptr, caret_local) {
                return true;
            }
        }
        false
    }
}

// ─── Clip-path mask builder ───────────────────────────────────────────────────

fn make_clip_path_mask(
    pixmap: &Pixmap,
    node:   &HtmlBox,
    px: f32, py: f32, pw: f32, ph: f32,
    font_px: f32,
    scale: f32,
) -> Option<Mask> {
    let cp = &node.style.clip_path;
    if cp.kind == ClipPathKind::None || pw <= 0.0 || ph <= 0.0 { return None; }

    let path = match cp.kind {
        ClipPathKind::Inset => {
            let t = cp.inset_top.resolve(font_px, ph, 16.0);
            let r = cp.inset_right.resolve(font_px, pw, 16.0);
            let b = cp.inset_bottom.resolve(font_px, ph, 16.0);
            let l = cp.inset_left.resolve(font_px, pw, 16.0);
            rect_path(px + l, py + t, pw - l - r, ph - t - b)?
        }
        ClipPathKind::Circle => {
            let cx  = cp.center_x.resolve(font_px, pw, 16.0) + px;
            let cy  = cp.center_y.resolve(font_px, ph, 16.0) + py;
            let ref_r = (pw * pw + ph * ph).sqrt() / std::f32::consts::SQRT_2;
            let r   = cp.circle_radius.resolve(font_px, ref_r, 16.0);
            circle_path(cx, cy, r)?
        }
        ClipPathKind::Ellipse => {
            let cx = cp.center_x.resolve(font_px, pw, 16.0) + px;
            let cy = cp.center_y.resolve(font_px, ph, 16.0) + py;
            let rx = cp.ellipse_rx.resolve(font_px, pw, 16.0);
            let ry = cp.ellipse_ry.resolve(font_px, ph, 16.0);
            ellipse_path(cx, cy, rx, ry)?
        }
        ClipPathKind::Polygon => {
            if cp.points.len() < 3 { return None; }
            polygon_path(&cp.points, px, py, pw, ph, font_px)?
        }
        ClipPathKind::None => return None,
    };

    let ts = Transform::from_scale(scale, scale);
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    mask.fill_path(&path, FillRule::Winding, true, ts);
    Some(mask)
}

// ─── Overflow clip mask builder ───────────────────────────────────────────────

fn make_overflow_clip_mask(
    pixmap: &Pixmap,
    px: f32, py: f32, pw: f32, ph: f32,
    radius: f32,
    scale: f32,
) -> Option<Mask> {
    if pw <= 0.0 || ph <= 0.0 { return None; }
    let path = if radius > 0.0 {
        rounded_rect_path(px, py, pw, ph, radius)?
    } else {
        rect_path(px, py, pw, ph)?
    };
    let ts = Transform::from_scale(scale, scale);
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    mask.fill_path(&path, FillRule::Winding, true, ts);
    Some(mask)
}

// ─── Object-fit rect computation ──────────────────────────────────────────────

/// Returns (draw_x, draw_y, draw_w, draw_h, clip_to_dest).
/// draw_* are the final screen coordinates to draw the image at (possibly larger than dest).
/// clip_to_dest = true means the image must be clipped to dest bounds.
fn compute_object_fit_rect(
    img_w: f32, img_h: f32,
    dest_w: f32, dest_h: f32,
    dest_x: f32, dest_y: f32,
    fit: ObjectFit,
) -> (f32, f32, f32, f32, bool) {
    match fit {
        ObjectFit::Fill => {
            (dest_x, dest_y, dest_w, dest_h, false)
        }
        ObjectFit::Contain => {
            let scale = (dest_w / img_w).min(dest_h / img_h);
            let dw = img_w * scale;
            let dh = img_h * scale;
            let dx = dest_x + (dest_w - dw) / 2.0;
            let dy = dest_y + (dest_h - dh) / 2.0;
            (dx, dy, dw, dh, false)
        }
        ObjectFit::Cover => {
            let scale = (dest_w / img_w).max(dest_h / img_h);
            let dw = img_w * scale;
            let dh = img_h * scale;
            let dx = dest_x + (dest_w - dw) / 2.0;
            let dy = dest_y + (dest_h - dh) / 2.0;
            (dx, dy, dw, dh, true)
        }
        ObjectFit::None => {
            // Natural size, centered, clipped
            let dx = dest_x + (dest_w - img_w) / 2.0;
            let dy = dest_y + (dest_h - img_h) / 2.0;
            (dx, dy, img_w, img_h, true)
        }
        ObjectFit::ScaleDown => {
            // Smaller of contain vs none
            let scale = ((dest_w / img_w).min(dest_h / img_h)).min(1.0);
            let dw = img_w * scale;
            let dh = img_h * scale;
            let dx = dest_x + (dest_w - dw) / 2.0;
            let dy = dest_y + (dest_h - dh) / 2.0;
            (dx, dy, dw, dh, false)
        }
    }
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h);
    pb.line_to(x, y + h);
    pb.close();
    pb.finish()
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    if r <= 0.0 { return rect_path(x, y, w, h); }
    let k = r * 0.5522848;  // kappa for quarter-circle approximation
    let mut pb = PathBuilder::new();
    // Top edge
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    // Top-right corner
    pb.cubic_to(x + w - r + k, y,          x + w, y + r - k,      x + w, y + r);
    // Right edge
    pb.line_to(x + w, y + h - r);
    // Bottom-right corner
    pb.cubic_to(x + w, y + h - r + k,      x + w - r + k, y + h,  x + w - r, y + h);
    // Bottom edge
    pb.line_to(x + r, y + h);
    // Bottom-left corner
    pb.cubic_to(x + r - k, y + h,          x, y + h - r + k,      x, y + h - r);
    // Left edge
    pb.line_to(x, y + r);
    // Top-left corner
    pb.cubic_to(x, y + r - k,              x + r - k, y,           x + r, y);
    pb.close();
    pb.finish()
}

fn line_path(x1: f32, y1: f32, x2: f32, y2: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    pb.finish()
}

fn circle_path(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    let k = 0.5522848f32;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + r*k, cy - r,   cx + r,   cy - r*k,  cx + r, cy);
    pb.cubic_to(cx + r,   cy + r*k, cx + r*k, cy + r,    cx,     cy + r);
    pb.cubic_to(cx - r*k, cy + r,   cx - r,   cy + r*k,  cx - r, cy);
    pb.cubic_to(cx - r,   cy - r*k, cx - r*k, cy - r,    cx,     cy - r);
    pb.close();
    pb.finish()
}

fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<tiny_skia::Path> {
    let k = 0.5522848f32;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - ry);
    pb.cubic_to(cx + rx*k, cy - ry, cx + rx, cy - ry*k, cx + rx, cy);
    pb.cubic_to(cx + rx, cy + ry*k, cx + rx*k, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - rx*k, cy + ry, cx - rx, cy + ry*k, cx - rx, cy);
    pb.cubic_to(cx - rx, cy - ry*k, cx - rx*k, cy - ry, cx, cy - ry);
    pb.close();
    pb.finish()
}

fn polygon_path(
    points: &[(CssLength, CssLength)],
    px: f32, py: f32, pw: f32, ph: f32,
    font_px: f32,
) -> Option<tiny_skia::Path> {
    if points.len() < 3 { return None; }
    let mut pb = PathBuilder::new();
    let (x0, y0) = (
        points[0].0.resolve(font_px, pw, 16.0) + px,
        points[0].1.resolve(font_px, ph, 16.0) + py,
    );
    pb.move_to(x0, y0);
    for pt in &points[1..] {
        let vx = pt.0.resolve(font_px, pw, 16.0) + px;
        let vy = pt.1.resolve(font_px, ph, 16.0) + py;
        pb.line_to(vx, vy);
    }
    pb.close();
    pb.finish()
}

fn draw_dashed_line(pixmap: &mut Pixmap, paint: &Paint, w: f32, x1: f32, y1: f32, x2: f32, y2: f32, scale: f32) {
    let dash_len = w * 3.0;
    let gap_len  = w * 2.0;
    let dx = x2 - x1; let dy = y2 - y1;
    let len = (dx*dx + dy*dy).sqrt();
    if len < 0.5 { return; }
    let nx = dx / len; let ny = dy / len;
    let mut t = 0.0f32; let mut on = true;
    let mut stroke = Stroke::default();
    stroke.width = w;
    while t < len {
        let seg = if on { dash_len } else { gap_len };
        if on {
            let ex = (t + seg).min(len);
            if let Some(path) = line_path(x1 + nx*t, y1 + ny*t, x1 + nx*ex, y1 + ny*ex) {
                pixmap.stroke_path(&path, paint, &stroke, Transform::from_scale(scale, scale), None);
            }
        }
        t += seg; on = !on;
    }
}

fn draw_dotted_line(pixmap: &mut Pixmap, paint: &Paint, w: f32, x1: f32, y1: f32, x2: f32, y2: f32, scale: f32) {
    let r = w / 2.0; let gap = w;
    let dx = x2 - x1; let dy = y2 - y1;
    let len = (dx*dx + dy*dy).sqrt();
    if len < 0.5 { return; }
    let nx = dx / len; let ny = dy / len;
    let mut t = r;
    while t < len {
        if let Some(path) = circle_path(x1 + nx*t, y1 + ny*t, r) {
            pixmap.fill_path(&path, paint, FillRule::Winding, Transform::from_scale(scale, scale), None);
        }
        t += w + gap;
    }
}

// ─── Text helpers ─────────────────────────────────────────────────────────────

fn apply_text_transform(text: &str, tt: TextTransform) -> String {
    match tt {
        TextTransform::Uppercase  => text.to_uppercase(),
        TextTransform::Lowercase  => text.to_lowercase(),
        TextTransform::Capitalize => capitalize_words(text),
        TextTransform::None       => text.to_owned(),
    }
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() { return s.len(); }
    while idx > 0 && !s.is_char_boundary(idx) { idx -= 1; }
    idx
}

fn format_list_marker(lst: ListStyleType, index: i32) -> String {
    match lst {
        ListStyleType::Decimal    => format!("{}.", index),
        ListStyleType::LowerAlpha => format!("{}.", to_alpha(index, false)),
        ListStyleType::UpperAlpha => format!("{}.", to_alpha(index, true)),
        ListStyleType::LowerRoman => format!("{}.", to_roman(index, false)),
        ListStyleType::UpperRoman => format!("{}.", to_roman(index, true)),
        _ => String::from("•"),
    }
}

fn to_alpha(mut n: i32, upper: bool) -> String {
    if n <= 0 { return String::from("?"); }
    let base: u8 = if upper { b'A' } else { b'a' };
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (base + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

fn to_roman(n: i32, upper: bool) -> String {
    let vals = [(1000,"m"),(900,"cm"),(500,"d"),(400,"cd"),
                (100,"c"),(90,"xc"),(50,"l"),(40,"xl"),
                (10,"x"),(9,"ix"),(5,"v"),(4,"iv"),(1,"i")];
    let mut out = String::new(); let mut rem = n;
    for (v, s) in &vals { while rem >= *v { out.push_str(s); rem -= v; } }
    if upper { out.to_ascii_uppercase() } else { out }
}

fn capitalize_words(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_space = true;
    for ch in s.chars() {
        if ch.is_whitespace() { result.push(ch); prev_space = true; }
        else if prev_space    { result.extend(ch.to_uppercase()); prev_space = false; }
        else                  { result.push(ch); }
    }
    result
}

/// Approximate text width (no letter-spacing).
fn approx_text_width(text: &str, font_px: f32) -> f32 {
    approx_text_width_ls(text, font_px, 0.0)
}

/// Approximate text width with letter-spacing.
fn approx_text_width_ls(text: &str, font_px: f32, letter_spacing: f32) -> f32 {
    let base = font_px * 0.55;
    let mut w = 0.0f32;
    for ch in text.chars() {
        let cw = if "iIlj1!|:;,.'`".contains(ch) { base * 0.45 }
                 else if "mwMW".contains(ch)       { base * 1.20 }
                 else if ch == ' '                  { base * 0.35 }
                 else if ch.is_ascii()              { base }
                 else                               { font_px * 1.0 };  // emoji / CJK: full square
        w += cw + letter_spacing;
    }
    w
}

/// Advance cursor by text width accounting for letter/word spacing and justify extra.
fn advance_with_spacing(
    text: &str,
    font_px: f32,
    letter_spacing: f32,
    word_spacing: f32,
    extra_per_word: f32,
) -> f32 {
    let base = font_px * 0.55;
    let mut w = 0.0f32;
    for ch in text.chars() {
        let cw = if "iIlj1!|:;,.'`".contains(ch) { base * 0.45 }
                 else if "mwMW".contains(ch)       { base * 1.20 }
                 else if ch == ' '                  { base * 0.35 }
                 else                               { base };
        w += cw + letter_spacing;
        if ch == ' ' {
            w += word_spacing + extra_per_word;
        }
    }
    w
}

fn truncate_with_ellipsis(text: &str, font_px: f32, letter_spacing: f32, max_w: f32) -> String {
    let ellipsis = "…";
    let ew = approx_text_width_ls(ellipsis, font_px, letter_spacing);
    let available = max_w - ew;
    if available <= 0.0 { return ellipsis.to_owned(); }
    let mut w = 0.0f32;
    let mut cut = text.len();
    for (i, ch) in text.char_indices() {
        let base = font_px * 0.55;
        let cw = if "iIlj1!|:;,.'`".contains(ch) { base * 0.45 }
                 else if "mwMW".contains(ch)       { base * 1.20 }
                 else if ch == ' '                  { base * 0.35 }
                 else                               { base };
        if w + cw > available { cut = i; break; }
        w += cw + letter_spacing;
    }
    let mut result = text[..cut].to_owned();
    result.push_str(ellipsis);
    result
}

enum Side { Top, Right, Bottom, Left }

impl Default for Renderer {
    fn default() -> Self { Self::new() }
}
