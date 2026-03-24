//! Display list builder — walks the box tree and records paint commands.
//!
//! Handles: backgrounds, borders, images, inline text (line_cache + inline_runs),
//! hover/active/visited style switching, scrolling, opacity, stacking contexts.

use crate::types::{
    HtmlBox, Rect, Color, CssLength, ComputedStyle, Display, Overflow, Position,
    FontWeight, FontStyle, TextDecorationStyle,
};
use super::display_list::{DisplayList, PaintCmd, TextDecoration, ImageRef};

/// Build a display list from a laid-out box tree.
pub fn build_display_list(root: &HtmlBox, viewport_w: f32, viewport_h: f32) -> DisplayList {
    let ctx = BuildContext {
        scroll_x: 0.0,
        scroll_y: 0.0,
        hovered_id: 0,
        active_id: 0,
        visited_hrefs: &std::collections::HashSet::new(),
    };
    let mut list = DisplayList::new();
    build_for_box(root, &mut list, &ctx);
    list
}

/// Build with hover/scroll context.
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
    let ctx = BuildContext {
        scroll_x, scroll_y,
        hovered_id, active_id, visited_hrefs,
    };
    let mut list = DisplayList::new();
    build_for_box(root, &mut list, &ctx);
    list
}

struct BuildContext<'a> {
    scroll_x: f32,
    scroll_y: f32,
    hovered_id: u32,
    active_id: u32,
    visited_hrefs: &'a std::collections::HashSet<String>,
}

fn build_for_box(node: &HtmlBox, list: &mut DisplayList, ctx: &BuildContext) {
    if matches!(node.style.display, Display::None) { return; }
    if !node.style.visibility { return; }

    if matches!(node.style.display, Display::Contents) {
        for child in &node.children {
            build_for_box(child, list, ctx);
        }
        return;
    }

    let br = node.border_rect;
    if br.w <= 0.0 && br.h <= 0.0 && node.tag != "#text" { return; }

    // Resolve effective style (hover/active/visited)
    let is_hovered = ctx.hovered_id != 0
        && node.style.hover_style.is_some()
        && subtree_contains_id(node, ctx.hovered_id);
    let is_active = ctx.active_id != 0
        && node.style.active_style.is_some()
        && subtree_contains_id(node, ctx.active_id);
    let is_visited = node.style.visited_style.is_some()
        && !node.style.href.is_empty()
        && ctx.visited_hrefs.contains(&node.style.href);

    let eff_style: &ComputedStyle = if is_active {
        node.style.active_style.as_deref().unwrap_or(&node.style)
    } else if is_visited {
        node.style.visited_style.as_deref().unwrap_or(&node.style)
    } else if is_hovered {
        node.style.hover_style.as_deref().unwrap_or(&node.style)
    } else {
        &node.style
    };

    // Stacking context
    let creates_stacking_ctx = eff_style.z_index != 0
        || eff_style.opacity < 1.0
        || !eff_style.transform.is_empty()
        || matches!(eff_style.position, Position::Fixed);

    if creates_stacking_ctx {
        list.push(PaintCmd::BeginStackingContext {
            node_id: node.node_id,
            z_index: eff_style.z_index,
        });
    }

    // Opacity
    if eff_style.opacity < 1.0 {
        list.push(PaintCmd::PushOpacity { alpha: eff_style.opacity });
    }

    // Blend mode
    let blend_mode = blend_mode_to_u8(eff_style.mix_blend_mode);
    if blend_mode != 0 {
        list.push(PaintCmd::PushBlendMode { mode: blend_mode });
    }

    // CSS Transform
    let has_transform = !eff_style.css_transform.ops.is_empty();
    if has_transform {
        let matrix = compute_transform_matrix(eff_style, &adjusted_rect(&br, ctx));
        list.push(PaintCmd::PushTransform { transform: matrix });
    }

    // Overflow clip
    let needs_clip = matches!(eff_style.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto)
        || matches!(eff_style.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto);
    if needs_clip {
        list.push(PaintCmd::PushClip {
            rect: adjusted_rect(&node.padding_rect, ctx),
            radius: extract_radii(node, eff_style),
        });
    }

    // Box shadow (before background)
    if let Some(ref s) = eff_style.box_shadow {
        if !s.inset {
            list.push(PaintCmd::BoxShadow {
                rect: adjusted_rect(&br, ctx),
                color: s.color,
                offset_x: s.offset_x,
                offset_y: s.offset_y,
                blur: s.blur,
                spread: s.spread,
                inset: false,
                radii: extract_radii(node, eff_style),
            });
        }
    }

    // Background
    if eff_style.background_color.a > 0 {
        list.push(PaintCmd::FillRect {
            rect: adjusted_rect(&node.padding_rect, ctx),
            color: eff_style.background_color,
            radius: extract_radii(node, eff_style),
        });
    }

    // Background image
    if let Some(ref data) = node.bg_image_data {
        list.push(PaintCmd::Image {
            rect: adjusted_rect(&node.padding_rect, ctx),
            data: ImageRef::Owned(data.clone(), node.bg_image_width, node.bg_image_height),
        });
    }

    // Border
    let bw = [
        node.resolved_border_top,
        node.resolved_border_right,
        node.resolved_border_bottom,
        node.resolved_border_left,
    ];
    if bw.iter().any(|&w| w > 0.0) {
        list.push(PaintCmd::Border {
            rect: adjusted_rect(&br, ctx),
            widths: bw,
            colors: [
                eff_style.border_top_color,
                eff_style.border_right_color,
                eff_style.border_bottom_color,
                eff_style.border_left_color,
            ],
            styles: [
                border_style_to_u8(eff_style.border_top_style),
                border_style_to_u8(eff_style.border_right_style),
                border_style_to_u8(eff_style.border_bottom_style),
                border_style_to_u8(eff_style.border_left_style),
            ],
            radii: extract_radii(node, eff_style),
        });
    }

    // Inset box shadow (after background, before content)
    if let Some(ref s) = eff_style.box_shadow {
        if s.inset {
            list.push(PaintCmd::BoxShadow {
                rect: adjusted_rect(&node.padding_rect, ctx),
                color: s.color,
                offset_x: s.offset_x,
                offset_y: s.offset_y,
                blur: s.blur,
                spread: s.spread,
                inset: true,
                radii: extract_radii(node, eff_style),
            });
        }
    }

    // Content: replaced element (image)
    if let Some(ref data) = node.image_data {
        list.push(PaintCmd::Image {
            rect: adjusted_rect(&node.content_rect, ctx),
            data: ImageRef::Owned(data.clone(), node.image_width, node.image_height),
        });
    }

    // Content: inline text via line_cache
    if !node.line_cache.is_empty() {
        build_inline_content(node, eff_style, list, ctx);
    } else if node.tag == "#text" && !node.text.is_empty() {
        // Fallback for text nodes without line_cache
        build_text_node(node, eff_style, list, ctx);
    }

    // Children (with scroll offset)
    let child_ctx = BuildContext {
        scroll_x: ctx.scroll_x + node.scroll_left,
        scroll_y: ctx.scroll_y + node.scroll_top,
        ..*ctx
    };
    for child in &node.children {
        if child.tag == "::before" || child.tag == "::after" {
            build_for_box(child, list, &child_ctx);
        }
    }
    for child in &node.children {
        if child.tag != "::before" && child.tag != "::after" {
            build_for_box(child, list, &child_ctx);
        }
    }

    // Pop clip
    if needs_clip { list.push(PaintCmd::PopClip); }
    if has_transform { list.push(PaintCmd::PopTransform); }
    if blend_mode != 0 { list.push(PaintCmd::PopBlendMode); }
    if eff_style.opacity < 1.0 { list.push(PaintCmd::PopOpacity); }
    if creates_stacking_ctx { list.push(PaintCmd::EndStackingContext); }
}

/// Build paint commands for inline content (text with line breaks).
fn build_inline_content(node: &HtmlBox, eff_style: &ComputedStyle, list: &mut DisplayList, ctx: &BuildContext) {
    // Collect flat text from inline children
    let mut flat = String::new();
    collect_flat_text(node, &mut flat);
    if flat.is_empty() { return; }

    let font_px = eff_style.font_size_px(16.0, 16.0);
    let line_h_val = match &eff_style.line_height {
        CssLength::None => font_px * 1.2,
        CssLength::Em(n) => font_px * n,
        other => other.resolve(font_px, 0.0, 16.0).max(font_px),
    };
    let letter_sp = eff_style.letter_spacing.resolve(font_px, 0.0, 16.0);

    for line in &node.line_cache {
        let line_start = floor_char_boundary(&flat, line.text_start.min(flat.len()));
        let line_end = floor_char_boundary(&flat, (line.text_start + line.text_length).min(flat.len()));
        if line_start >= line_end { continue; }

        let line_text = &flat[line_start..line_end];
        if line_text.trim().is_empty() { continue; }

        let lx = line.x - ctx.scroll_x;
        let ly = line.y - ctx.scroll_y;

        // Find which inline run(s) cover this line
        if node.inline_runs.is_empty() {
            // No runs — use the node's own style
            emit_text_cmd(list, lx + line.text_x_offset, ly, line_text, eff_style,
                         font_px, line_h_val, letter_sp);
        } else {
            // Walk runs that overlap this line range
            let mut cursor_x = lx + line.text_x_offset;
            for run in &node.inline_runs {
                let run_start = run.text_offset;
                let run_end = run.text_offset + run.length;
                let overlap_start = floor_char_boundary(&flat, run_start.max(line_start));
                let overlap_end = floor_char_boundary(&flat, run_end.min(line_end));
                if overlap_start >= overlap_end { continue; }

                let run_text = &flat[overlap_start..overlap_end];
                if run_text.is_empty() { continue; }

                let run_font_px = run.style.font_size_px(16.0, 16.0);
                let run_line_h = match &run.style.line_height {
                    CssLength::None => run_font_px * 1.2,
                    CssLength::Em(n) => run_font_px * n,
                    other => other.resolve(run_font_px, 0.0, 16.0).max(run_font_px),
                };
                let run_letter_sp = run.style.letter_spacing.resolve(run_font_px, 0.0, 16.0);

                emit_text_cmd(list, cursor_x, ly, run_text, &run.style,
                             run_font_px, run_line_h, run_letter_sp);

                // Advance cursor (approximate — proper advance needs shaping)
                cursor_x += run_text.len() as f32 * run_font_px * 0.6;
            }
        }
    }
}

/// Build a text command for a simple text node (no line_cache).
fn build_text_node(node: &HtmlBox, eff_style: &ComputedStyle, list: &mut DisplayList, ctx: &BuildContext) {
    let font_px = eff_style.font_size_px(16.0, 16.0);
    let line_h = match &eff_style.line_height {
        CssLength::None => font_px * 1.2,
        CssLength::Em(n) => font_px * n,
        other => other.resolve(font_px, 0.0, 16.0).max(font_px),
    };
    let letter_sp = eff_style.letter_spacing.resolve(font_px, 0.0, 16.0);

    emit_text_cmd(
        list,
        node.content_rect.x - ctx.scroll_x,
        node.content_rect.y - ctx.scroll_y,
        &node.text,
        eff_style,
        font_px, line_h, letter_sp,
    );
}

fn emit_text_cmd(
    list: &mut DisplayList,
    x: f32, y: f32,
    text: &str,
    style: &ComputedStyle,
    font_px: f32,
    line_h: f32,
    letter_sp: f32,
) {
    if text.is_empty() || font_px <= 0.0 { return; }
    let font_px = font_px.max(1.0);
    let line_h = if line_h > 0.0 { line_h } else { font_px * 1.2 };
    let deco_thickness = style.text_decoration_thickness.resolve(font_px, 0.0, 16.0);
    list.push(PaintCmd::Text {
        x, y,
        text: text.to_string(),
        font_family: style.font_family.clone(),
        font_size: font_px,
        font_weight: style.font_weight.value(),
        font_style: match style.font_style {
            FontStyle::Italic => 1,
            FontStyle::Oblique => 2,
            _ => 0,
        },
        font_stretch: style.font_stretch,
        line_height: line_h,
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
            thickness: if deco_thickness > 0.0 { deco_thickness } else { 1.0 },
        },
        letter_spacing: letter_sp,
        small_caps: style.small_caps,
    });
}

/// Collect flat text from a node's inline children (for line_cache matching).
fn collect_flat_text(node: &HtmlBox, out: &mut String) {
    if node.tag == "#text" {
        out.push_str(&node.text);
        return;
    }
    for child in &node.children {
        if child.tag == "br" {
            out.push('\n');
        } else if matches!(child.style.display, Display::Inline | Display::None) || child.tag == "#text" {
            collect_flat_text(child, out);
        }
    }
}

/// Snap a byte index down to the nearest UTF-8 char boundary.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() { return s.len(); }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) { i -= 1; }
    i
}

fn subtree_contains_id(node: &HtmlBox, target_id: u32) -> bool {
    if node.node_id == target_id { return true; }
    node.children.iter().any(|c| subtree_contains_id(c, target_id))
}

fn adjusted_rect(rect: &Rect, ctx: &BuildContext) -> Rect {
    Rect::new(
        rect.x - ctx.scroll_x,
        rect.y - ctx.scroll_y,
        rect.w,
        rect.h,
    )
}

fn extract_radii(node: &HtmlBox, style: &ComputedStyle) -> [f32; 4] {
    let font_px = style.font_size_px(16.0, 16.0);
    let w = node.border_rect.w;
    let r_shorthand = style.border_radius.resolve(font_px, w, 16.0);
    if r_shorthand > 0.0 {
        [r_shorthand; 4]
    } else {
        [
            style.border_top_left_radius.resolve(font_px, w, 16.0),
            style.border_top_right_radius.resolve(font_px, w, 16.0),
            style.border_bottom_right_radius.resolve(font_px, w, 16.0),
            style.border_bottom_left_radius.resolve(font_px, w, 16.0),
        ]
    }
}

fn blend_mode_to_u8(mode: crate::types::MixBlendMode) -> u8 {
    use crate::types::MixBlendMode;
    match mode {
        MixBlendMode::Normal     => 0,
        MixBlendMode::Multiply   => 1,
        MixBlendMode::Screen     => 2,
        MixBlendMode::Overlay    => 3,
        MixBlendMode::Darken     => 4,
        MixBlendMode::Lighten    => 5,
        MixBlendMode::ColorDodge => 6,
        MixBlendMode::ColorBurn  => 7,
        MixBlendMode::HardLight  => 8,
        MixBlendMode::SoftLight  => 9,
        MixBlendMode::Difference => 10,
        MixBlendMode::Exclusion  => 11,
        MixBlendMode::Hue        => 12,
        MixBlendMode::Saturation => 13,
        MixBlendMode::Color      => 14,
        MixBlendMode::Luminosity => 15,
    }
}

fn compute_transform_matrix(style: &ComputedStyle, rect: &Rect) -> [f32; 6] {
    use crate::types::TransformOp;
    // transform_origin_x/y are 0.0-1.0 fractions
    let ox = rect.x + rect.w * style.transform_origin_x;
    let oy = rect.y + rect.h * style.transform_origin_y;

    let mut a: f32 = 1.0; let mut b: f32 = 0.0;
    let mut c: f32 = 0.0; let mut d: f32 = 1.0;
    let mut e: f32 = 0.0; let mut f: f32 = 0.0;

    for op in &style.css_transform.ops {
        match op {
            TransformOp::Translate(tx, ty) => { e += tx; f += ty; }
            TransformOp::TranslateX(tx) => { e += tx; }
            TransformOp::TranslateY(ty) => { f += ty; }
            TransformOp::Scale(sx, sy) => { a *= sx; d *= sy; }
            TransformOp::ScaleX(sx) => { a *= sx; }
            TransformOp::ScaleY(sy) => { d *= sy; }
            TransformOp::Rotate(deg) => {
                let rad = deg * std::f32::consts::PI / 180.0;
                let cos = rad.cos(); let sin = rad.sin();
                let na = a * cos + c * sin;
                let nb = b * cos + d * sin;
                let nc = a * -sin + c * cos;
                let nd = b * -sin + d * cos;
                a = na; b = nb; c = nc; d = nd;
            }
            TransformOp::SkewX(deg) => {
                let tan = (deg * std::f32::consts::PI / 180.0).tan();
                c += a * tan; d += b * tan;
            }
            TransformOp::SkewY(deg) => {
                let tan = (deg * std::f32::consts::PI / 180.0).tan();
                a += c * tan; b += d * tan;
            }
            TransformOp::Matrix(m0, m1, m2, m3, m4, m5) => {
                let na = a * m0 + c * m1;
                let nb = b * m0 + d * m1;
                let nc = a * m2 + c * m3;
                let nd = b * m2 + d * m3;
                let ne = a * m4 + c * m5 + e;
                let nf = b * m4 + d * m5 + f;
                a = na; b = nb; c = nc; d = nd; e = ne; f = nf;
            }
        }
    }

    // Apply transform origin
    let te = e + ox - (a * ox + c * oy);
    let tf = f + oy - (b * ox + d * oy);

    [a, b, c, d, te, tf]
}

fn border_style_to_u8(s: crate::types::BorderStyle) -> u8 {
    match s {
        crate::types::BorderStyle::None   => 0,
        crate::types::BorderStyle::Solid  => 1,
        crate::types::BorderStyle::Dashed => 2,
        crate::types::BorderStyle::Dotted => 3,
        crate::types::BorderStyle::Double => 4,
        crate::types::BorderStyle::Groove => 5,
        crate::types::BorderStyle::Ridge  => 6,
        crate::types::BorderStyle::Inset  => 7,
        crate::types::BorderStyle::Outset => 8,
        _ => 0,
    }
}
