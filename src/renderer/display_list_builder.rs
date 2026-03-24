//! Display list builder — walks the box tree and records paint commands.
//!
//! This runs alongside the existing direct-paint renderer during migration.
//! Once validated, the renderer can replay the display list instead of
//! walking the tree directly.

use crate::types::{HtmlBox, Rect, Color, Display, Overflow, Position};
use super::display_list::{DisplayList, PaintCmd, TextDecoration, ImageRef};

/// Build a display list from a laid-out box tree.
pub fn build_display_list(root: &HtmlBox, viewport_w: f32, viewport_h: f32) -> DisplayList {
    let mut list = DisplayList::new();
    let clip = Rect::new(0.0, 0.0, viewport_w, viewport_h);
    build_for_box(root, &mut list, 0.0, 0.0, clip);
    list
}

fn build_for_box(node: &HtmlBox, list: &mut DisplayList, scroll_x: f32, scroll_y: f32, clip: Rect) {
    // Skip display:none
    if matches!(node.style.display, Display::None) { return; }
    if matches!(node.style.display, Display::Contents) {
        // display:contents: skip the box, process children
        for child in &node.children {
            build_for_box(child, list, scroll_x, scroll_y, clip);
        }
        return;
    }

    let br = node.border_rect;
    if br.w <= 0.0 && br.h <= 0.0 && node.tag != "#text" { return; }

    // Stacking context
    let creates_stacking_ctx = node.style.z_index != 0
        || node.style.opacity < 1.0
        || !node.style.transform.is_empty()
        || matches!(node.style.position, Position::Fixed);

    if creates_stacking_ctx {
        list.push(PaintCmd::BeginStackingContext {
            node_id: node.node_id,
            z_index: node.style.z_index,
        });
    }

    // Opacity
    if node.style.opacity < 1.0 {
        list.push(PaintCmd::PushOpacity { alpha: node.style.opacity });
    }

    // Clip for overflow
    let needs_clip = matches!(node.style.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto)
        || matches!(node.style.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto);
    if needs_clip {
        let pr = node.padding_rect;
        list.push(PaintCmd::PushClip {
            rect: pr,
            radius: extract_radii(node),
        });
    }

    // Background
    if node.style.background_color.a > 0 {
        list.push(PaintCmd::FillRect {
            rect: node.padding_rect,
            color: node.style.background_color,
            radius: extract_radii(node),
        });
    }

    // Background image
    if let Some(ref data) = node.bg_image_data {
        list.push(PaintCmd::Image {
            rect: node.padding_rect,
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
            rect: node.border_rect,
            widths: bw,
            colors: [
                node.style.border_top_color,
                node.style.border_right_color,
                node.style.border_bottom_color,
                node.style.border_left_color,
            ],
            styles: [
                border_style_to_u8(node.style.border_top_style),
                border_style_to_u8(node.style.border_right_style),
                border_style_to_u8(node.style.border_bottom_style),
                border_style_to_u8(node.style.border_left_style),
            ],
            radii: extract_radii(node),
        });
    }

    // Content: image
    if let Some(ref data) = node.image_data {
        list.push(PaintCmd::Image {
            rect: node.content_rect,
            data: ImageRef::Owned(data.clone(), node.image_width, node.image_height),
        });
    }

    // Content: text
    if node.tag == "#text" && !node.text.is_empty() {
        list.push(PaintCmd::Text {
            x: node.content_rect.x,
            y: node.content_rect.y,
            text: node.text.clone(),
            font_family: node.style.font_family.clone(),
            font_size: node.style.font_size_px(16.0, 16.0),
            font_weight: node.style.font_weight.value(),
            font_style: if node.style.font_style == crate::types::FontStyle::Italic { 1 } else { 0 },
            color: node.style.color,
            decoration: TextDecoration {
                underline: node.style.text_decoration.underline,
                overline: node.style.text_decoration.overline,
                strikethrough: node.style.text_decoration.strikethrough,
                color: node.style.text_decoration_color.unwrap_or(node.style.color),
                style: 0, // solid
                thickness: 1.0,
            },
        });
    }

    // Children
    let child_scroll_x = scroll_x + node.scroll_left;
    let child_scroll_y = scroll_y + node.scroll_top;
    for child in &node.children {
        build_for_box(child, list, child_scroll_x, child_scroll_y, clip);
    }

    // Pop clip
    if needs_clip {
        list.push(PaintCmd::PopClip);
    }

    // Pop opacity
    if node.style.opacity < 1.0 {
        list.push(PaintCmd::PopOpacity);
    }

    // End stacking context
    if creates_stacking_ctx {
        list.push(PaintCmd::EndStackingContext);
    }
}

fn extract_radii(node: &HtmlBox) -> [f32; 4] {
    [
        node.style.border_top_left_radius.resolve(16.0, node.border_rect.w, 16.0),
        node.style.border_top_right_radius.resolve(16.0, node.border_rect.w, 16.0),
        node.style.border_bottom_right_radius.resolve(16.0, node.border_rect.w, 16.0),
        node.style.border_bottom_left_radius.resolve(16.0, node.border_rect.w, 16.0),
    ]
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
