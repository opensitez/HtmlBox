//! Display list builder — walks the box tree and records paint commands.
//!
//! Uses EXACT positions from the layout engine. Never approximates.
//! Faithfully ports the render_box logic from mod.rs into PaintCmd recording.

use crate::types::{
    BackgroundRepeat, BackgroundSize, Color, ComputedStyle, Display,
    FontStyle, GradientType, ListStylePosition, ListStyleType,
    MixBlendMode, Overflow, Position, TextDecorationStyle,
    TextTransform,
};
use crate::types::{HtmlBox, Rect};
use super::display_list::{DisplayList, ImageRef, PaintCmd, TextDecoration};

/// Build a display list from a laid-out box tree.
pub fn build_display_list(root: &HtmlBox, viewport_w: f32, viewport_h: f32) -> DisplayList {
    let visited = std::collections::HashSet::new();
    // Use full document extent as clip — viewport culling is done at replay time.
    // Building with viewport clip causes scrolled-to content to be missing.
    let doc_h = crate::types::Document::scroll_height(root).max(viewport_h);
    let ctx = BuildContext {
        scroll_x: 0.0,
        scroll_y: 0.0,
        hovered_id: 0,
        active_id: 0,
        visited_hrefs: &visited,
        clip: Rect::new(0.0, 0.0, viewport_w, doc_h),
    };
    let mut list = DisplayList::new();
    build_for_box(root, &mut list, &ctx);
    list
}

/// Build with full context (scroll, hover, etc.).
pub fn build_display_list_full(
    root: &HtmlBox,
    viewport_w: f32,
    viewport_h: f32,
    scroll_x: f32,
    scroll_y: f32,
    hovered_id: u32,
    active_id: u32,
    visited_hrefs: &std::collections::HashSet<String>,
) -> DisplayList {
    let doc_h = crate::types::Document::scroll_height(root).max(viewport_h);
    let ctx = BuildContext {
        scroll_x,
        scroll_y,
        hovered_id,
        active_id,
        visited_hrefs,
        clip: Rect::new(0.0, 0.0, viewport_w, doc_h),
    };
    let mut list = DisplayList::new();
    build_for_box(root, &mut list, &ctx);

    // Fixed elements: rendered at viewport position (already scroll=0)
    let fixed_ctx = BuildContext {
        scroll_x: 0.0, scroll_y: 0.0,
        hovered_id, active_id, visited_hrefs,
        clip: Rect::new(0.0, 0.0, viewport_w, viewport_h),
    };
    let mut fixed_ids = Vec::new();
    collect_fixed_elements(root, &mut fixed_ids);
    for fid in fixed_ids {
        fn find_node(node: &HtmlBox, id: u32) -> Option<&HtmlBox> {
            if node.node_id == id { return Some(node); }
            for child in &node.children {
                if let Some(found) = find_node(child, id) { return Some(found); }
            }
            None
        }
        if let Some(node) = find_node(root, fid) {
            build_for_box(node, &mut list, &fixed_ctx);
        }
    }

    list
}

struct BuildContext<'a> {
    scroll_x: f32,
    scroll_y: f32,
    hovered_id: u32,
    active_id: u32,
    visited_hrefs: &'a std::collections::HashSet<String>,
    clip: Rect,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main entry: build_for_box — mirrors render_box in mod.rs exactly
// ═══════════════════════════════════════════════════════════════════════════════

fn build_for_box(node: &HtmlBox, list: &mut DisplayList, ctx: &BuildContext) {
    // ── Early exits (same as render_box) ─────────────────────────────────────
    if matches!(node.style.display, Display::None) {
        return;
    }
    if !node.style.visibility {
        return;
    }
    if node.style.opacity <= 0.0 {
        return;
    }

    // Display::Contents — skip the box itself, render children only
    if matches!(node.style.display, Display::Contents) {
        for child in node.effective_children() {
            build_for_box(child, list, ctx);
        }
        return;
    }

    let sx = ctx.scroll_x;
    let sy = ctx.scroll_y;
    let br = node.layout.border_rect;

    // ── Viewport culling ─────────────────────────────────────────────────────
    // Only cull elements that clip their children (overflow != visible).
    // Elements with overflow:visible may have children that extend beyond
    // the element's border_rect (e.g. height:100vh wrapper with overflowing content).
    let clips_children = matches!(node.style.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto)
        || matches!(node.style.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto);
    if clips_children
        && matches!(node.style.position, Position::Static | Position::Relative)
        && !node.style.is_inline_level()
        && !matches!(node.style.display, Display::Contents)
    {
        let bx = br.x - sx;
        let by = br.y - sy;
        if bx + br.w < ctx.clip.x
            || by + br.h < ctx.clip.y
            || bx > ctx.clip.right()
            || by > ctx.clip.bottom()
        {
            return;
        }
    }

    let pr = node.layout.padding_rect;
    let px = pr.x - sx;
    let py = pr.y - sy;
    let pw = pr.w;
    let ph = pr.h;
    let font_px = node.style.font_size_px(16.0, 16.0);

    // ── Border radii (exact same resolution as render_box) ───────────────────
    let r_shorthand = node.style.border_radius.resolve(font_px, pr.w, 16.0);
    let r_tl = if r_shorthand > 0.0 {
        r_shorthand
    } else {
        node.style
            .border_top_left_radius
            .resolve(font_px, pr.w, 16.0)
    };
    let r_tr = if r_shorthand > 0.0 {
        r_shorthand
    } else {
        node.style
            .border_top_right_radius
            .resolve(font_px, pr.w, 16.0)
    };
    let r_br = if r_shorthand > 0.0 {
        r_shorthand
    } else {
        node.style
            .border_bottom_right_radius
            .resolve(font_px, pr.w, 16.0)
    };
    let r_bl = if r_shorthand > 0.0 {
        r_shorthand
    } else {
        node.style
            .border_bottom_left_radius
            .resolve(font_px, pr.w, 16.0)
    };
    let radii_arr = [r_tl, r_tr, r_br, r_bl];

    // ── Hover / active / visited check ───────────────────────────────────────
    let is_hovered = ctx.hovered_id != 0
        && node.style.hover_style.is_some()
        && subtree_has(node, ctx.hovered_id);
    let is_active = ctx.active_id != 0
        && node.style.active_style.is_some()
        && subtree_has(node, ctx.active_id);
    let is_visited = node.style.visited_style.is_some()
        && !node.style.href.is_empty()
        && ctx.visited_hrefs.contains(&node.style.href);

    let eff_style: &ComputedStyle = if is_active {
        node.style
            .active_style
            .as_deref()
            .unwrap_or(&node.style)
    } else if is_visited {
        node.style
            .visited_style
            .as_deref()
            .unwrap_or(&node.style)
    } else if is_hovered {
        node.style.hover_style.as_deref().unwrap_or(&node.style)
    } else {
        &node.style
    };

    // ── Sticky positioning ───────────────────────────────────────────────────
    let (px, py) = if node.style.position == Position::Sticky {
        let top_val = node.style.top.resolve(font_px, ctx.clip.h, 16.0);
        let left_val = node.style.left.resolve(font_px, ctx.clip.w, 16.0);
        let nat_x = pr.x - sx;
        let nat_y = pr.y - sy;
        let cx = if !node.style.left.is_auto() {
            nat_x.max(ctx.clip.x + left_val)
        } else {
            nat_x
        };
        let cy = if !node.style.top.is_auto() {
            nat_y.max(ctx.clip.y + top_val)
        } else {
            nat_y
        };
        (cx, cy)
    } else {
        (px, py)
    };

    // Effective scroll offsets accounting for sticky clamping
    let eff_sx = pr.x - px;
    let eff_sy = pr.y - py;

    // ── Legacy clip: rect(top, right, bottom, left) ──────────────────────────
    if let Some(cr) = node.style.clip_rect {
        let clip_right = if cr[1] == f32::MAX { pw } else { cr[1] };
        let clip_bottom = if cr[2] == f32::MAX { ph } else { cr[2] };
        let cw = clip_right - cr[3];
        let ch = clip_bottom - cr[0];
        if cw <= 0.0 || ch <= 0.0 {
            return;
        }
    }

    // ── Stacking context ─────────────────────────────────────────────────────
    let stacking = eff_style.z_index != 0
        || eff_style.opacity < 1.0
        || !eff_style.css_transform.ops.is_empty()
        || matches!(eff_style.position, Position::Fixed);
    if stacking {
        list.push(PaintCmd::BeginStackingContext {
            node_id: node.node_id,
            z_index: eff_style.z_index,
        });
    }

    // ── Opacity ──────────────────────────────────────────────────────────────
    if eff_style.opacity < 1.0 {
        list.push(PaintCmd::PushOpacity {
            alpha: eff_style.opacity,
        });
    }

    // ── Blend mode ───────────────────────────────────────────────────────────
    let blend = blend_mode_to_u8(eff_style.mix_blend_mode);
    if blend != 0 {
        list.push(PaintCmd::PushBlendMode { mode: blend });
    }

    // ── CSS transform ────────────────────────────────────────────────────────
    // Use the element's DOCUMENT position (pr.x, pr.y) for the transform origin,
    // not the scroll-adjusted position (px, py). The scroll offset is applied
    // separately by the replay's global transform. This prevents transforms from
    // shifting when the user scrolls.
    let has_transform = !eff_style.css_transform.ops.is_empty();
    if has_transform {
        let tr_rect = Rect::new(px, py, pw, ph);
        list.push(PaintCmd::PushTransform {
            transform: compute_transform_matrix(eff_style, &tr_rect),
        });
    }

    // Form elements (input, select, button, textarea, etc.) are rendered entirely
    // by the FormElement paint command. Skip CSS background/border/text to avoid
    // double rendering.
    let _is_form_element = matches!(node.tag.as_str(),
        "input" | "textarea" | "select" | "button" | "progress" | "meter");

    // ── CSS filters ───────────────────────────────────────────────────────────
    let has_filter = !eff_style.css_filter.ops.is_empty();
    if has_filter {
        use crate::types::FilterOp;
        let filters: Vec<(u8, f32, Color)> = eff_style.css_filter.ops.iter().map(|op| {
            match op {
                FilterOp::Blur(v)       => (0, *v, Color::TRANSPARENT),
                FilterOp::Brightness(v) => (1, *v, Color::TRANSPARENT),
                FilterOp::Contrast(v)   => (2, *v, Color::TRANSPARENT),
                FilterOp::Grayscale(v)  => (3, *v, Color::TRANSPARENT),
                FilterOp::HueRotate(v)  => (4, *v, Color::TRANSPARENT),
                FilterOp::Invert(v)     => (5, *v, Color::TRANSPARENT),
                FilterOp::Opacity(v)    => (6, *v, Color::TRANSPARENT),
                FilterOp::Saturate(v)   => (7, *v, Color::TRANSPARENT),
                FilterOp::Sepia(v)      => (8, *v, Color::TRANSPARENT),
                FilterOp::DropShadow { dx: _, dy: _, blur, color } => (9, *blur, *color),
            }
        }).collect();
        list.push(PaintCmd::PushFilter { filters });
    }

    // ── (a) Outer box-shadow ─────────────────────────────────────────────────
    if let Some(ref bs) = eff_style.box_shadow {
        if !bs.inset {
            list.push(PaintCmd::BoxShadow {
                rect: Rect::new(px, py, pw, ph),
                color: bs.color,
                offset_x: bs.offset_x,
                offset_y: bs.offset_y,
                blur: bs.blur,
                spread: bs.spread,
                inset: false,
                radii: radii_arr,
            });
        }
    }

    // Form elements: CSS background/border/padding renders normally (below).
    // The FormElement command only draws the control CONTENT (value text,
    // placeholder, checkbox mark, radio dot, etc.) — not the box decoration.

    // ── (b) Background color (opacity applied to alpha) ──────────────────────
    {
        let raw_bg = eff_style.background_color;
        let opacity = eff_style.opacity;
        let has_mask = node.mask_image_data.is_some() && node.mask_image_width > 0;
        if raw_bg.a > 0 && !has_mask {
            let alpha = ((raw_bg.a as f32) * opacity) as u8;
            let bg = Color::rgba(raw_bg.r, raw_bg.g, raw_bg.b, alpha);
            list.push(PaintCmd::FillRect {
                rect: Rect::new(px, py, pw, ph),
                color: bg,
                radius: radii_arr,
            });
        }
        // CSS mask-image: draw background color masked by the mask image.
        // The mask SVG's luminance/alpha determines which pixels are visible.
        if has_mask {
            if let Some(ref mask_data) = node.mask_image_data {
                let mw = node.mask_image_width;
                let mh = node.mask_image_height;
                // Create a colored version: replace mask pixels' color with bg color,
                // keeping the mask's alpha channel
                let alpha = ((raw_bg.a as f32) * opacity) as u8;
                let mut colored = Vec::with_capacity((mw * mh * 4) as usize);
                for i in 0..(mw * mh) as usize {
                    let base = i * 4;
                    // Use mask pixel's luminance as alpha
                    let mr = mask_data.get(base).copied().unwrap_or(0) as u32;
                    let mg = mask_data.get(base + 1).copied().unwrap_or(0) as u32;
                    let mb = mask_data.get(base + 2).copied().unwrap_or(0) as u32;
                    let ma = mask_data.get(base + 3).copied().unwrap_or(0) as u32;
                    // Luminance-based alpha (weighted average)
                    let lum = (mr * 77 + mg * 150 + mb * 29) >> 8; // approx 0.3R + 0.59G + 0.11B
                    let final_alpha = ((lum * ma * alpha as u32) / (255 * 255)) as u8;
                    colored.push(raw_bg.r);
                    colored.push(raw_bg.g);
                    colored.push(raw_bg.b);
                    colored.push(final_alpha);
                }
                list.push(PaintCmd::Image {
                    rect: Rect::new(px, py, pw, ph),
                    data: ImageRef::Owned(colored, mw, mh),
                });
            }
        }
    }

    // ── (c) Gradient background ──────────────────────────────────────────────
    if node.style.gradient_type != GradientType::None
        && node.style.gradient_stops.len() >= 2
    {
        let opacity = eff_style.opacity;
        let grad_type_u8 = match node.style.gradient_type {
            GradientType::Linear => 1u8,
            GradientType::Radial => 2u8,
            GradientType::None => 0u8,
        };
        let stops: Vec<(Color, f32)> = node
            .style
            .gradient_stops
            .iter()
            .map(|s| {
                let a = ((s.color.a as f32) * opacity) as u8;
                (Color::rgba(s.color.r, s.color.g, s.color.b, a), s.position)
            })
            .collect();
        list.push(PaintCmd::Gradient {
            rect: Rect::new(px, py, pw, ph),
            gradient_type: grad_type_u8,
            angle: node.style.gradient_angle,
            stops,
            radii: radii_arr,
            opacity,
            blend_mode: blend,
        });
    }

    // ── (d) Background image ─────────────────────────────────────────────────
    if let Some(ref bg_data) = node.bg_image_data {
        if node.bg_image_width > 0 && node.bg_image_height > 0 {
            let iw = node.bg_image_width as f32;
            let ih = node.bg_image_height as f32;

            // Compute drawn image dimensions based on background-size
            let (draw_w, draw_h) = match node.style.background_size {
                BackgroundSize::Cover => {
                    let scale = (pw / iw).max(ph / ih);
                    (iw * scale, ih * scale)
                }
                BackgroundSize::Contain => {
                    let scale = (pw / iw).min(ph / ih);
                    (iw * scale, ih * scale)
                }
                BackgroundSize::Explicit => {
                    let w = if node.style.background_size_w.is_auto() {
                        iw
                    } else {
                        node.style.background_size_w.resolve(font_px, pw, 16.0)
                    };
                    let h = if node.style.background_size_h.is_auto() {
                        ih
                    } else {
                        node.style.background_size_h.resolve(font_px, ph, 16.0)
                    };
                    (w, h)
                }
                BackgroundSize::Auto => (iw, ih),
            };

            let pos_x = px
                + node
                    .style
                    .background_position_x
                    .resolve(font_px, pw - draw_w, 16.0);
            let pos_y = py
                + node
                    .style
                    .background_position_y
                    .resolve(font_px, ph - draw_h, 16.0);

            let repeat_x = matches!(
                node.style.background_repeat,
                BackgroundRepeat::Repeat | BackgroundRepeat::RepeatX
            );
            let repeat_y = matches!(
                node.style.background_repeat,
                BackgroundRepeat::Repeat | BackgroundRepeat::RepeatY
            );

            let size_mode = match node.style.background_size {
                BackgroundSize::Auto => 0u8,
                BackgroundSize::Cover => 1,
                BackgroundSize::Contain => 2,
                BackgroundSize::Explicit => 3,
            };

            list.push(PaintCmd::BackgroundImage {
                container: Rect::new(px, py, pw, ph),
                data: ImageRef::Owned(
                    bg_data.clone(),
                    node.bg_image_width,
                    node.bg_image_height,
                ),
                size_mode,
                draw_w,
                draw_h,
                pos_x,
                pos_y,
                repeat_x,
                repeat_y,
                radii: radii_arr,
            });
        }
    }

    // ── (e) Inset box-shadow ─────────────────────────────────────────────────
    if let Some(ref bs) = eff_style.box_shadow {
        if bs.inset {
            list.push(PaintCmd::BoxShadow {
                rect: Rect::new(px, py, pw, ph),
                color: bs.color,
                offset_x: bs.offset_x,
                offset_y: bs.offset_y,
                blur: bs.blur,
                spread: bs.spread,
                inset: true,
                radii: radii_arr,
            });
        }
    }

    // ── (f) Borders ──────────────────────────────────────────────────────────
    // render_box calls draw_borders_masked with eff_sx, eff_sy
    {
        let bw = [
            node.layout.resolved_border_top,
            node.layout.resolved_border_right,
            node.layout.resolved_border_bottom,
            node.layout.resolved_border_left,
        ];
        if bw.iter().any(|&w| w > 0.0) {
            let bx = br.x - eff_sx;
            let by = br.y - eff_sy;
            list.push(PaintCmd::Border {
                rect: Rect::new(bx, by, br.w, br.h),
                widths: bw,
                colors: [
                    eff_style.border_top_color,
                    eff_style.border_right_color,
                    eff_style.border_bottom_color,
                    eff_style.border_left_color,
                ],
                styles: [
                    bstyle(eff_style.border_top_style),
                    bstyle(eff_style.border_right_style),
                    bstyle(eff_style.border_bottom_style),
                    bstyle(eff_style.border_left_style),
                ],
                radii: radii_arr,
            });
        }
    }

    // ── (g) Outline ──────────────────────────────────────────────────────────
    if eff_style.outline_width > 0.0
        && eff_style.outline_style != crate::types::BorderStyle::None
    {
        let ofs = eff_style.outline_offset;
        let ow = eff_style.outline_width;
        let rx = br.x - eff_sx - ofs - ow;
        let ry = br.y - eff_sy - ofs - ow;
        let rw = br.w + 2.0 * (ofs + ow);
        let rh = br.h + 2.0 * (ofs + ow);
        list.push(PaintCmd::Outline {
            rect: Rect::new(rx, ry, rw, rh),
            width: ow,
            color: eff_style.outline_color,
            style: bstyle(eff_style.outline_style),
            offset: ofs,
        });
    }

    // ── (h) Overflow clip setup ──────────────────────────────────────────────
    let overflow_clips = matches!(
        node.style.overflow_x,
        Overflow::Hidden | Overflow::Scroll | Overflow::Auto
    ) || matches!(
        node.style.overflow_y,
        Overflow::Hidden | Overflow::Scroll | Overflow::Auto
    );
    if overflow_clips {
        list.push(PaintCmd::PushClip {
            rect: Rect::new(px, py, pw, ph),
            radius: radii_arr,
        });
    }

    // Tighter clip rect for children when overflow is clipping
    let child_clip = if overflow_clips {
        let cx1 = px.max(ctx.clip.x);
        let cy1 = py.max(ctx.clip.y);
        let cx2 = (px + pw).min(ctx.clip.right());
        let cy2 = (py + ph).min(ctx.clip.bottom());
        Rect::new(cx1, cy1, (cx2 - cx1).max(0.0), (cy2 - cy1).max(0.0))
    } else {
        ctx.clip
    };

    // Per-element scroll: children are shifted by the element's scroll
    let child_sx = eff_sx + node.layout.scroll_left;
    let child_sy = eff_sy + node.layout.scroll_top;

    let child_ctx = BuildContext {
        scroll_x: child_sx,
        scroll_y: child_sy,
        hovered_id: ctx.hovered_id,
        active_id: ctx.active_id,
        visited_hrefs: ctx.visited_hrefs,
        clip: child_clip,
    };

    // ── (i) Negative z-index children (paint behind text) ────────────────────
    {
        let eff_children = node.effective_children();
        for child in eff_children {
            if child.style.is_positioned()
                && child.style.z_index < 0
                && !matches!(child.style.display, Display::None)
            {
                build_for_box(child, list, &child_ctx);
            }
        }
    }

    // ── (j) ::before pseudo-element (inline text content) ────────────────────
    if !node.style.before_content.is_empty() && !node.layout.line_cache.is_empty() {
        let first = &node.layout.line_cache[0];
        let tx = first.x - eff_sx;
        let ty = first.y - eff_sy;
        let ps = node
            .style
            .before_style
            .as_deref()
            .unwrap_or(&node.style);
        let ps_font_px = {
            let f = ps.font_size.resolve(font_px, 0.0, 16.0);
            if f > 0.0 {
                f
            } else {
                font_px
            }
        };
        let line_h = ps
            .line_height
            .resolve(ps_font_px, 0.0, 16.0)
            .max(ps_font_px * 1.2);
        emit_text(
            list,
            tx,
            ty,
            &node.style.before_content,
            ps,
            ps_font_px,
            line_h,
        );
    }

    // ── (k) Inline text content (line_cache) ─────────────────────────────────
    if !node.layout.line_cache.is_empty() {
        build_inline_text(
            node, eff_style, list, child_sx, child_sy, is_hovered, is_active,
        );
    }

    // ── (l) ::after pseudo-element ───────────────────────────────────────────
    if !node.style.after_content.is_empty() && !node.layout.line_cache.is_empty() {
        let last = &node.layout.line_cache[node.layout.line_cache.len() - 1];
        let tx = last.x - eff_sx + last.width;
        let ty = last.y - eff_sy;
        let ps = node.style.after_style.as_deref().unwrap_or(&node.style);
        let ps_font_px = {
            let f = ps.font_size.resolve(font_px, 0.0, 16.0);
            if f > 0.0 {
                f
            } else {
                font_px
            }
        };
        let line_h = ps
            .line_height
            .resolve(ps_font_px, 0.0, 16.0)
            .max(ps_font_px * 1.2);
        emit_text(
            list,
            tx,
            ty,
            &node.style.after_content,
            ps,
            ps_font_px,
            line_h,
        );
    }

    // ── (m) List markers ─────────────────────────────────────────────────────
    if node.style.display == Display::ListItem && !node.layout.line_cache.is_empty() {
        build_list_marker(node, list, eff_sx, eff_sy);
    }

    // ── (n) HR ───────────────────────────────────────────────────────────────
    if node.tag == "hr" {
        let cr = node.layout.border_rect;
        let y_hr = cr.y + cr.h / 2.0 - eff_sy;
        list.push(PaintCmd::HorizontalRule {
            x1: cr.x - eff_sx,
            y1: y_hr,
            x2: cr.right() - eff_sx,
        });
    }

    // ── (o) Form elements (content only — box decoration handled by CSS steps above)
    build_form_element(node, list, eff_sx, eff_sy);

    // ── (p) Image / SVG ──────────────────────────────────────────────────────
    if node.tag == "img" || node.tag == "svg" {
        if let Some(ref data) = node.image_data {
            if node.image_width > 0 && node.image_height > 0 {
                let cr = node.layout.content_rect;
                list.push(PaintCmd::Image {
                    rect: Rect::new(cr.x - eff_sx, cr.y - eff_sy, cr.w, cr.h),
                    data: ImageRef::Owned(data.clone(), node.image_width, node.image_height),
                });
            }
        } else if node.tag == "svg" || (node.tag == "img" && node.svg_markup.is_some()) {
            // SVG: rasterize from svg_markup on demand (inline <svg> or <img src="*.svg">)
            if let Some(ref markup) = node.svg_markup {
                let cr = node.layout.content_rect;
                if cr.w > 0.0 && cr.h > 0.0 {
                    let raster_w = cr.w.round() as u32;
                    let raster_h = cr.h.round() as u32;
                    if raster_w > 0 && raster_h > 0 {
                        // Inject inherited CSS color for currentColor support
                        let c = node.style.color;
                        let color_hex = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
                        let mut colored = markup.replace("currentColor", &color_hex);
                        if colored.starts_with("<svg") {
                            if let Some(gt) = colored.find('>') {
                                let inject = format!(
                                    "<style>svg{{color:{0}}}path:not([fill]),circle:not([fill]),rect:not([fill]),polygon:not([fill]),line:not([fill]),polyline:not([fill]){{fill:{0}}}</style>",
                                    color_hex
                                );
                                colored.insert_str(gt + 1, &inject);
                            }
                        }
                        if let Some(rgba) = crate::html::rasterize_svg_to_rgba(&colored, raster_w, raster_h) {
                            list.push(PaintCmd::Image {
                                rect: Rect::new(cr.x - eff_sx, cr.y - eff_sy, cr.w, cr.h),
                                data: ImageRef::Owned(rgba, raster_w, raster_h),
                            });
                        }
                    }
                }
            }
        }
    }

    // ── (q) Children: non-positioned first, then positioned by z-index ───────
    // Skip ::before/::after (handled as inline text in steps j/l above).
    // Skip position:fixed (rendered in separate overlay pass).
    {
        let eff_children = node.effective_children();
        let is_renderable = |c: &HtmlBox| -> bool {
            !matches!(c.style.display, Display::None)
                && c.tag != "::before"
                && c.tag != "::after"
                && c.style.position != Position::Fixed
        };

        let has_positioned = eff_children
            .iter()
            .any(|c| is_renderable(c) && c.style.is_positioned());

        if !has_positioned {
            for child in eff_children {
                if is_renderable(child) {
                    build_for_box(child, list, &child_ctx);
                }
            }
        } else {
            // Non-positioned children (normal flow)
            for child in eff_children {
                if is_renderable(child) && !child.style.is_positioned() {
                    build_for_box(child, list, &child_ctx);
                }
            }

            // Positioned elements with z-index >= 0 (in front), sorted by z-index
            let mut positioned: Vec<&HtmlBox> = eff_children
                .iter()
                .filter(|c| is_renderable(c) && c.style.is_positioned() && c.style.z_index >= 0)
                .collect();
            positioned.sort_by_key(|c| c.style.z_index);

            for child in &positioned {
                build_for_box(child, list, &child_ctx);
            }
        }
    }

    // ── Pop in reverse order ─────────────────────────────────────────────────
    if overflow_clips {
        list.push(PaintCmd::PopClip);
    }
    if has_transform {
        list.push(PaintCmd::PopTransform);
    }
    if blend != 0 {
        list.push(PaintCmd::PopBlendMode);
    }
    if eff_style.opacity < 1.0 {
        list.push(PaintCmd::PopOpacity);
    }
    if stacking {
        list.push(PaintCmd::EndStackingContext);
    }

    if has_filter {
        list.push(PaintCmd::PopFilter);
    }
    // TODO: clip-path masks — no PaintCmd variant yet
}

// ═══════════════════════════════════════════════════════════════════════════════
// Inline text using EXACT layout positions
// ═══════════════════════════════════════════════════════════════════════════════

fn build_inline_text(
    node: &HtmlBox,
    eff_style: &ComputedStyle,
    list: &mut DisplayList,
    sx: f32,
    sy: f32,
    is_hovered: bool,
    is_active: bool,
) {
    let mut flat = String::new();
    collect_flat_text(node, &mut flat);
    if flat.is_empty() {
        return;
    }

    let opacity = eff_style.opacity;
    let fallback_font_px = node.style.font_size_px(16.0, 16.0).max(1.0);
    let fallback_letter_spc = node.style.letter_spacing.resolve(fallback_font_px, 0.0, 16.0);

    // When overflow is clipping and text-indent pushes content far outside the
    // element's box, skip all text rendering — the clip would hide it anyway and
    // this avoids emitting paint commands for offscreen text.
    let overflow_clips = matches!(
        node.style.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto
    ) || matches!(
        node.style.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto
    );
    if overflow_clips {
        let ti = node.style.text_indent.resolve(fallback_font_px, node.layout.content_rect.w, 16.0);
        if ti < -(node.layout.content_rect.w + 100.0) {
            return;
        }
    }

    for line in &node.layout.line_cache {
        let line_start = floor_cb(&flat, line.text_start.min(flat.len()));
        let line_end =
            floor_cb(&flat, (line.text_start + line.text_length).min(flat.len()));
        if line_start >= line_end {
            continue;
        }
        if flat[line_start..line_end].trim().is_empty() {
            continue;
        }

        // EXACT positions from layout
        let lx = line.x - sx;
        let ly = line.y - sy;

        // ── Build chunks from inline_runs / visual_segments ──────────────
        struct Chunk {
            s: usize,
            e: usize,
            run_idx: Option<usize>,
            rtl: bool,
        }
        let mut chunks: Vec<Chunk> = Vec::new();

        if !line.visual_segments.is_empty() && !node.layout.inline_runs.is_empty() {
            // BiDi: use visual segment order
            for vs in &line.visual_segments {
                let seg_s = vs.logical_start;
                let seg_e = vs.logical_start + vs.length;
                let is_rtl = (vs.level & 1) != 0;
                let mut seg_chunks: Vec<Chunk> = Vec::new();
                for (ri, run) in node.layout.inline_runs.iter().enumerate() {
                    let rs = run.text_offset;
                    let re = rs + run.length;
                    let cs = seg_s.max(rs);
                    let ce = seg_e.min(re);
                    if cs < ce {
                        seg_chunks.push(Chunk {
                            s: cs,
                            e: ce,
                            run_idx: Some(ri),
                            rtl: is_rtl,
                        });
                    }
                }
                if is_rtl {
                    // Trim leading whitespace from last chunk (becomes visual-first after reversal)
                    if let Some(last) = seg_chunks.last_mut() {
                        while last.s < last.e
                            && last.s < flat.len()
                            && matches!(
                                flat.as_bytes()[last.s],
                                b' ' | b'\t' | b'\n' | b'\r'
                            )
                        {
                            last.s += 1;
                        }
                    }
                    seg_chunks.reverse();
                }
                chunks.extend(seg_chunks);
            }
        } else if node.layout.inline_runs.is_empty() {
            chunks.push(Chunk {
                s: line_start,
                e: line_end,
                run_idx: None,
                rtl: false,
            });
        } else {
            for (ri, run) in node.layout.inline_runs.iter().enumerate() {
                let cs = line_start.max(run.text_offset);
                let ce = line_end.min(run.text_offset + run.length);
                if cs < ce {
                    chunks.push(Chunk {
                        s: cs,
                        e: ce,
                        run_idx: Some(ri),
                        rtl: false,
                    });
                }
            }
        }

        let mut cursor_x = lx + line.text_x_offset;

        for chunk in &chunks {
            let s = floor_cb(&flat, chunk.s);
            let e = floor_cb(&flat, chunk.e);
            if e <= s {
                continue;
            }

            let (run_style, run_font_px, run_letter_spc, _run_word_spc, _run_extra) =
                if let Some(ri) = chunk.run_idx {
                    let run = &node.layout.inline_runs[ri];
                    let fp = run.style.font_size_px(16.0, 16.0).max(1.0);
                    let ls = run.style.letter_spacing.resolve(fp, 0.0, 16.0);
                    let ws = run.style.word_spacing.resolve(fp, 0.0, 16.0);
                    (
                        Some(&run.style),
                        fp,
                        ls,
                        ws,
                        line.extra_space_per_word,
                    )
                } else {
                    (
                        None,
                        fallback_font_px,
                        fallback_letter_spc,
                        0.0,
                        line.extra_space_per_word,
                    )
                };

            let style_ref: &ComputedStyle = run_style.unwrap_or(&node.style);
            let seg_text = &flat[s..e];

            // Normalize raw newlines to spaces
            let seg_text_clean: String;
            let seg_text_for_draw: &str =
                if seg_text.contains('\n') || seg_text.contains('\r') {
                    seg_text_clean = seg_text
                        .chars()
                        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
                        .collect();
                    &seg_text_clean
                } else {
                    seg_text
                };
            let draw_text = apply_text_transform(seg_text_for_draw, style_ref.text_transform);
            if draw_text.is_empty() {
                continue;
            }

            let run_line_h = style_ref
                .line_height
                .resolve(run_font_px, 0.0, 16.0)
                .max(run_font_px * 1.2);

            // Use char_x for exact x position if available.
            // For RTL chunks, char_x byte offsets don't correspond to visual
            // position (logical byte 0 of Arabic maps to the rightmost glyph).
            // Use cursor_x instead, which advances in visual order.
            let x_pos = if chunk.rtl {
                cursor_x
            } else if !line.char_x.is_empty() {
                let char_offset = s - line_start;
                if char_offset < line.char_x.len() {
                    lx + line.text_x_offset + line.char_x[char_offset]
                } else {
                    cursor_x
                }
            } else {
                cursor_x
            };

            // Run background color
            if style_ref.background_color.a > 0 {
                let run_w = if !line.char_x.is_empty() {
                    let start_off = s - line_start;
                    let end_off = e - line_start;
                    let x_start = if start_off < line.char_x.len() {
                        line.char_x[start_off]
                    } else {
                        0.0
                    };
                    let x_end = if end_off < line.char_x.len() {
                        line.char_x[end_off]
                    } else if !line.char_x.is_empty() {
                        *line.char_x.last().unwrap()
                    } else {
                        0.0
                    };
                    (x_end - x_start).abs()
                } else {
                    // Fallback only when no char_x
                    draw_text.len() as f32 * run_font_px * 0.6
                };
                list.push(PaintCmd::FillRect {
                    rect: Rect::new(x_pos, ly, run_w, line.height),
                    color: style_ref.background_color,
                    radius: [0.0; 4],
                });
            }

            // Text color: use effective style color when run inherits from node
            let run_color =
                if std::ptr::eq(style_ref as *const _, &node.style as *const _)
                    || ((is_hovered || is_active) && style_ref.color == node.style.color)
                {
                    eff_style.color
                } else {
                    style_ref.color
                };
            let alpha = ((run_color.a as f32) * opacity) as u8;
            let text_color = Color::rgba(run_color.r, run_color.g, run_color.b, alpha);

            // Text shadow
            if let Some(ref ts) = style_ref.text_shadow {
                list.push(PaintCmd::TextShadow {
                    x: x_pos + ts.offset_x,
                    y: ly + ts.offset_y,
                    text: draw_text.clone(),
                    font_family: style_ref.font_family.clone(),
                    font_size: run_font_px,
                    font_weight: style_ref.font_weight.value(),
                    font_style: if chunk.rtl {
                        0
                    } else {
                        match style_ref.font_style {
                            FontStyle::Italic => 1,
                            FontStyle::Oblique => 2,
                            _ => 0,
                        }
                    },
                    font_stretch: style_ref.font_stretch,
                    line_height: run_line_h,
                    color: ts.color,
                    blur: ts.blur,
                });
            }

            // Main text
            let effective_font_style = if chunk.rtl {
                FontStyle::Normal
            } else {
                style_ref.font_style
            };
            let deco_t = style_ref
                .text_decoration_thickness
                .resolve(run_font_px, 0.0, 16.0);
            let letter_sp = run_letter_spc;

            list.push(PaintCmd::Text {
                x: x_pos,
                y: ly,
                text: draw_text.clone(),
                font_family: style_ref.font_family.clone(),
                font_size: run_font_px,
                font_weight: style_ref.font_weight.value(),
                font_style: match effective_font_style {
                    FontStyle::Italic => 1,
                    FontStyle::Oblique => 2,
                    _ => 0,
                },
                font_stretch: style_ref.font_stretch,
                line_height: run_line_h,
                color: text_color,
                decoration: TextDecoration {
                    underline: style_ref.text_decoration.underline,
                    overline: style_ref.text_decoration.overline,
                    strikethrough: style_ref.text_decoration.strikethrough,
                    color: style_ref
                        .text_decoration_color
                        .unwrap_or(text_color),
                    style: match style_ref.text_decoration_style {
                        TextDecorationStyle::Double => 1,
                        TextDecorationStyle::Dotted => 2,
                        TextDecorationStyle::Dashed => 3,
                        TextDecorationStyle::Wavy => 4,
                        _ => 0,
                    },
                    thickness: if deco_t > 0.0 {
                        deco_t
                    } else {
                        (run_font_px / 12.0).max(1.0)
                    },
                },
                letter_spacing: letter_sp,
                small_caps: style_ref.small_caps,
            });

            // Advance cursor using char_x if available
            if chunk.rtl {
                // For RTL chunks, advance cursor_x by the visual segment width
                let start_off = s.saturating_sub(line_start);
                let end_off = e.saturating_sub(line_start);
                if start_off < line.char_x.len() && end_off < line.char_x.len() {
                    cursor_x += (line.char_x[end_off] - line.char_x[start_off]).abs();
                }
            } else if !line.char_x.is_empty() {
                let end_off = e - line_start;
                if end_off < line.char_x.len() {
                    cursor_x = lx + line.text_x_offset + line.char_x[end_off];
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// List marker
// ═══════════════════════════════════════════════════════════════════════════════

fn build_list_marker(node: &HtmlBox, list: &mut DisplayList, sx: f32, sy: f32) {
    // Skip marker entirely when list-style-type is None
    if matches!(node.style.list_style_type, ListStyleType::None) {
        return;
    }
    let ms = node.style.marker_style.as_deref();
    let font_px = ms
        .map(|s| s.font_size_px(16.0, 16.0))
        .unwrap_or_else(|| node.style.font_size_px(16.0, 16.0));
    let first_line = match node.layout.line_cache.first() {
        Some(l) => l,
        None => return,
    };
    let inside = node.style.list_style_position == ListStylePosition::Inside;
    let c = ms.map(|s| s.color).unwrap_or(node.style.color);

    match node.style.list_style_type {
        ListStyleType::Disc => {
            let bx = if inside {
                first_line.x - sx + 4.0
            } else {
                first_line.x - sx - 10.0
            };
            let by = first_line.y - sy + first_line.height / 2.0;
            list.push(PaintCmd::ListMarker {
                marker_type: 0,
                x: bx,
                y: by,
                size: 3.0,
                color: c,
                text: String::new(),
                font_family: node.style.font_family.clone(),
                font_size: font_px,
                font_weight: node.style.font_weight.value(),
                font_style: match node.style.font_style {
                    FontStyle::Italic => 1,
                    FontStyle::Oblique => 2,
                    _ => 0,
                },
                line_height: font_px * 1.2,
            });
        }
        ListStyleType::Circle => {
            let bx = if inside {
                first_line.x - sx + 4.0
            } else {
                first_line.x - sx - 10.0
            };
            let by = first_line.y - sy + first_line.height / 2.0;
            list.push(PaintCmd::ListMarker {
                marker_type: 1,
                x: bx,
                y: by,
                size: 3.0,
                color: c,
                text: String::new(),
                font_family: node.style.font_family.clone(),
                font_size: font_px,
                font_weight: node.style.font_weight.value(),
                font_style: match node.style.font_style {
                    FontStyle::Italic => 1,
                    FontStyle::Oblique => 2,
                    _ => 0,
                },
                line_height: font_px * 1.2,
            });
        }
        ListStyleType::Square => {
            let bx = if inside {
                first_line.x - sx + 4.0
            } else {
                first_line.x - sx - 10.0
            };
            let by = first_line.y - sy + first_line.height / 2.0;
            list.push(PaintCmd::ListMarker {
                marker_type: 2,
                x: bx,
                y: by,
                size: 6.0,
                color: c,
                text: String::new(),
                font_family: node.style.font_family.clone(),
                font_size: font_px,
                font_weight: node.style.font_weight.value(),
                font_style: match node.style.font_style {
                    FontStyle::Italic => 1,
                    FontStyle::Oblique => 2,
                    _ => 0,
                },
                line_height: font_px * 1.2,
            });
        }
        ListStyleType::Decimal
        | ListStyleType::LowerAlpha
        | ListStyleType::UpperAlpha
        | ListStyleType::LowerRoman
        | ListStyleType::UpperRoman => {
            let marker = format_list_marker(node.style.list_style_type, node.style.list_index);
            let line_h = node
                .style
                .line_height
                .resolve(font_px, 0.0, 16.0)
                .max(font_px * 1.2);
            let mx = if inside {
                first_line.x - sx
            } else {
                // marker_w not available here (we don't have font shaping).
                // Use an approximate offset that matches the render_box convention:
                // mx = first_line.x - sx - marker_w - 4.0
                // Since we can't measure, emit marker text and let replay handle positioning.
                first_line.x - sx - 4.0
            };
            let my = first_line.y - sy;
            list.push(PaintCmd::ListMarker {
                marker_type: 3,
                x: mx,
                y: my,
                size: 0.0,
                color: c,
                text: marker,
                font_family: node.style.font_family.clone(),
                font_size: font_px,
                font_weight: node.style.font_weight.value(),
                font_style: match node.style.font_style {
                    FontStyle::Italic => 1,
                    FontStyle::Oblique => 2,
                    _ => 0,
                },
                line_height: line_h,
            });
        }
        ListStyleType::Disclosure => {
            let line_h = node
                .style
                .line_height
                .resolve(font_px, 0.0, 16.0)
                .max(font_px * 1.2);
            let mx = if inside {
                first_line.x - sx
            } else {
                first_line.x - sx - 4.0
            };
            let my = first_line.y - sy;
            list.push(PaintCmd::ListMarker {
                marker_type: 3,
                x: mx,
                y: my,
                size: 0.0,
                color: c,
                text: "\u{25b8}".to_string(),
                font_family: node.style.font_family.clone(),
                font_size: font_px,
                font_weight: node.style.font_weight.value(),
                font_style: match node.style.font_style {
                    FontStyle::Italic => 1,
                    FontStyle::Oblique => 2,
                    _ => 0,
                },
                line_height: line_h,
            });
        }
        ListStyleType::None => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Form element
// ═══════════════════════════════════════════════════════════════════════════════

fn build_form_element(node: &HtmlBox, list: &mut DisplayList, sx: f32, sy: f32) {
    let tag = node.tag.as_str();
    let input_type = node
        .attributes
        .get("type")
        .map(|s| s.as_str())
        .unwrap_or("text");

    let is_form = match tag {
        "input" | "textarea" | "select" | "button" | "progress" | "meter" => true,
        _ => false,
    };
    if !is_form {
        return;
    }

    let cr = node.layout.content_rect;
    let font_px = node.style.font_size_px(16.0, 16.0).max(1.0);
    let value = if tag == "select" {
        // For select, get the text of the selected option (or first option)
        node.children.iter()
            .find(|c| c.tag == "option" && c.attributes.get("selected").is_some())
            .or_else(|| node.children.iter().find(|c| c.tag == "option"))
            .map(|opt| {
                opt.children.iter()
                    .filter(|c| c.tag == "#text")
                    .map(|c| c.text.trim())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    } else {
        crate::types::input_value(node)
    };
    let placeholder = node
        .attributes
        .get("placeholder")
        .cloned()
        .unwrap_or_default();
    let checked = node
        .attributes
        .get("checked")
        .map(|_| true)
        .unwrap_or(false);

    let attrs: Vec<(String, String)> = node
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    list.push(PaintCmd::FormElement {
        tag: tag.to_string(),
        input_type: input_type.to_string(),
        rect: Rect::new(cr.x - sx, cr.y - sy, cr.w, cr.h),
        node_id: node.node_id,
        attributes: attrs,
        font_size: font_px,
        font_weight: node.style.font_weight.value(),
        font_family: node.style.font_family.clone(),
        color: node.style.color,
        checked,
        value,
        placeholder,
        input_cursor: node.input_cursor,
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn emit_text(
    list: &mut DisplayList,
    x: f32,
    y: f32,
    text: &str,
    style: &ComputedStyle,
    font_px: f32,
    line_h: f32,
) {
    if text.is_empty() || font_px <= 0.0 {
        return;
    }
    let fp = font_px.max(1.0);
    let lh = if line_h > 0.0 { line_h } else { fp * 1.2 };
    let letter_sp = style.letter_spacing.resolve(fp, 0.0, 16.0);
    let deco_t = style.text_decoration_thickness.resolve(fp, 0.0, 16.0);
    list.push(PaintCmd::Text {
        x,
        y,
        text: text.to_string(),
        font_family: style.font_family.clone(),
        font_size: fp,
        font_weight: style.font_weight.value(),
        font_style: match style.font_style {
            FontStyle::Italic => 1,
            FontStyle::Oblique => 2,
            _ => 0,
        },
        font_stretch: style.font_stretch,
        line_height: lh,
        color: style.color,
        decoration: TextDecoration {
            underline: style.text_decoration.underline,
            overline: style.text_decoration.overline,
            strikethrough: style.text_decoration.strikethrough,
            color: style.text_decoration_color.unwrap_or(style.color),
            style: match style.text_decoration_style {
                TextDecorationStyle::Double => 1,
                TextDecorationStyle::Dotted => 2,
                TextDecorationStyle::Dashed => 3,
                TextDecorationStyle::Wavy => 4,
                _ => 0,
            },
            thickness: if deco_t > 0.0 { deco_t } else { 1.0 },
        },
        letter_spacing: letter_sp,
        small_caps: style.small_caps,
    });
}

fn collect_flat_text(node: &HtmlBox, out: &mut String) {
    if node.tag == "#text" {
        out.push_str(&node.text);
        return;
    }
    for child in &node.children {
        if child.tag == "br" {
            out.push('\n');
        } else if matches!(
            child.style.display,
            Display::Inline | Display::None
        ) || child.tag == "#text"
        {
            collect_flat_text(child, out);
        }
    }
}

fn subtree_has(node: &HtmlBox, id: u32) -> bool {
    if node.node_id == id {
        return true;
    }
    node.children.iter().any(|c| subtree_has(c, id))
}

fn floor_cb(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn bstyle(s: crate::types::BorderStyle) -> u8 {
    use crate::types::BorderStyle::*;
    match s {
        None => 0,
        Solid => 1,
        Dashed => 2,
        Dotted => 3,
        Double => 4,
        Groove => 5,
        Ridge => 6,
        Inset => 7,
        Outset => 8,
        _ => 0,
    }
}

fn blend_mode_to_u8(m: MixBlendMode) -> u8 {
    use MixBlendMode::*;
    match m {
        Normal => 0,
        Multiply => 1,
        Screen => 2,
        Overlay => 3,
        Darken => 4,
        Lighten => 5,
        ColorDodge => 6,
        ColorBurn => 7,
        HardLight => 8,
        SoftLight => 9,
        Difference => 10,
        Exclusion => 11,
        Hue => 12,
        Saturation => 13,
        Color => 14,
        Luminosity => 15,
    }
}

fn compute_transform_matrix(style: &ComputedStyle, rect: &Rect) -> [f32; 6] {
    use crate::types::TransformOp;
    let ox = rect.x + rect.w * style.transform_origin_x;
    let oy = rect.y + rect.h * style.transform_origin_y;
    let (mut a, mut b, mut c, mut d, mut e, mut f) =
        (1.0f32, 0.0f32, 0.0f32, 1.0f32, 0.0f32, 0.0f32);
    for op in &style.css_transform.ops {
        match op {
            TransformOp::Translate(tx, ty) => {
                e += tx;
                f += ty;
            }
            TransformOp::TranslateX(tx) => {
                e += tx;
            }
            TransformOp::TranslateY(ty) => {
                f += ty;
            }
            TransformOp::Scale(sx, sy) => {
                a *= sx;
                d *= sy;
            }
            TransformOp::ScaleX(sx) => {
                a *= sx;
            }
            TransformOp::ScaleY(sy) => {
                d *= sy;
            }
            TransformOp::Rotate(deg) => {
                let rad = deg * std::f32::consts::PI / 180.0;
                let (cos, sin) = (rad.cos(), rad.sin());
                let (na, nb) = (a * cos + c * sin, b * cos + d * sin);
                let (nc, nd) = (a * -sin + c * cos, b * -sin + d * cos);
                a = na;
                b = nb;
                c = nc;
                d = nd;
            }
            TransformOp::SkewX(deg) => {
                let t = (deg * std::f32::consts::PI / 180.0).tan();
                c += a * t;
                d += b * t;
            }
            TransformOp::SkewY(deg) => {
                let t = (deg * std::f32::consts::PI / 180.0).tan();
                a += c * t;
                b += d * t;
            }
            TransformOp::Matrix(m0, m1, m2, m3, m4, m5) => {
                let (na, nb, nc, nd) = (
                    a * m0 + c * m1,
                    b * m0 + d * m1,
                    a * m2 + c * m3,
                    b * m2 + d * m3,
                );
                let (ne, nf) = (a * m4 + c * m5 + e, b * m4 + d * m5 + f);
                a = na;
                b = nb;
                c = nc;
                d = nd;
                e = ne;
                f = nf;
            }
        }
    }
    [
        a,
        b,
        c,
        d,
        e + ox - (a * ox + c * oy),
        f + oy - (b * ox + d * oy),
    ]
}

fn apply_text_transform(text: &str, tt: TextTransform) -> String {
    match tt {
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => capitalize_words(text),
        TextTransform::None => text.to_owned(),
    }
}

fn capitalize_words(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_space = true;
    for c in text.chars() {
        if prev_space && c.is_alphabetic() {
            for uc in c.to_uppercase() {
                result.push(uc);
            }
        } else {
            result.push(c);
        }
        prev_space = c.is_whitespace();
    }
    result
}

fn format_list_marker(lst: ListStyleType, index: i32) -> String {
    match lst {
        ListStyleType::Decimal => format!("{}.", index),
        ListStyleType::LowerAlpha => {
            if index > 0 && index <= 26 {
                format!("{}.", (b'a' + (index - 1) as u8) as char)
            } else {
                format!("{}.", index)
            }
        }
        ListStyleType::UpperAlpha => {
            if index > 0 && index <= 26 {
                format!("{}.", (b'A' + (index - 1) as u8) as char)
            } else {
                format!("{}.", index)
            }
        }
        ListStyleType::LowerRoman => format!("{}.", to_roman_lower(index)),
        ListStyleType::UpperRoman => format!("{}.", to_roman_upper(index)),
        _ => String::new(),
    }
}

fn to_roman_lower(n: i32) -> String {
    to_roman_upper(n).to_lowercase()
}

fn to_roman_upper(mut n: i32) -> String {
    if n <= 0 || n > 3999 {
        return n.to_string();
    }
    let vals = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(v, s) in &vals {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

fn collect_fixed_elements(node: &HtmlBox, out: &mut Vec<u32>) {
    if node.style.position == Position::Fixed && node.node_id != 0 {
        out.push(node.node_id);
    }
    for child in &node.children {
        collect_fixed_elements(child, out);
    }
}
