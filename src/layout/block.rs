use crate::types::*;
use crate::layout::{LayoutEngine, ResolvedBox, FloatContext, FloatSide,
                    shift_rects, layout_positioned};

// ─── Margin collapsing helpers ────────────────────────────────────────────────

/// Collapse two adjoining margins per CSS spec:
/// both positive → max; both negative → min (most negative); mixed → sum.
pub fn collapse_two(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a <= 0.0 && b <= 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

// ─── BFC detection ───────────────────────────────────────────────────────────

/// Returns true if this node establishes a Block Formatting Context.
/// Mirrors C++ EstablishesBFC.
pub fn establishes_bfc(style: &ComputedStyle) -> bool {
    matches!(style.float, Float::Left | Float::Right)
        || !matches!(style.overflow_x, Overflow::Visible)
        || !matches!(style.overflow_y, Overflow::Visible)
        || matches!(style.display, Display::InlineBlock
            | Display::Flex | Display::InlineFlex
            | Display::Grid | Display::InlineGrid
            | Display::Table | Display::FlowRoot)
        || matches!(style.position, Position::Absolute | Position::Fixed)
}

/// Can top margin of this box collapse with its first child's top margin?
/// Mirrors C++ CanCollapseTopWithFirstChild.
fn can_collapse_top_with_first_child(node: &HtmlBox, rbox: &ResolvedBox) -> bool {
    if establishes_bfc(&node.style) { return false; }
    if rbox.border_top > 0.0 { return false; }
    if rbox.padding_top > 0.0 { return false; }
    if !node.line_cache.is_empty() { return false; }
    true
}

/// Can bottom margin of this box collapse with its last child's bottom margin?
/// Mirrors C++ CanCollapseBottomWithLastChild.
fn can_collapse_bottom_with_last_child(node: &HtmlBox, rbox: &ResolvedBox) -> bool {
    if establishes_bfc(&node.style) { return false; }
    if rbox.border_bottom > 0.0 { return false; }
    if rbox.padding_bottom > 0.0 { return false; }
    if rbox.content_height.is_some() { return false; }
    if !node.style.min_height.is_auto() { return false; }
    if !node.line_cache.is_empty() { return false; }
    true
}

/// Is this an "empty" block (no borders, padding, inline content, explicit height, in-flow children)?
/// Mirrors C++ IsEmptyBlock.
fn is_empty_block(node: &HtmlBox, rbox: &ResolvedBox) -> bool {
    if rbox.border_top != 0.0 || rbox.border_bottom != 0.0 { return false; }
    if rbox.padding_top != 0.0 || rbox.padding_bottom != 0.0 { return false; }
    if !node.line_cache.is_empty() { return false; }
    if rbox.content_height.is_some() { return false; }
    if !node.style.min_height.is_auto() { return false; }
    // Has in-flow block children?
    for child in &node.children {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) { continue; }
        if !matches!(child.style.float, Float::None) { continue; }
        return false; // has in-flow child
    }
    true
}

// ─── Shrink-to-fit intrinsic width ────────────────────────────────────────────

/// Compute the intrinsic (max-content) width of a box that was laid out at a
/// larger containing width.  Mirrors C++ ComputeIntrinsicWidth.
pub fn compute_intrinsic_width(node: &HtmlBox) -> f32 {
    // If the box has a fixed width, that IS its intrinsic width (min and max).
    // (In a more complete engine we'd distinguish min-content vs max-content,
    // but for now max-content is what matters for 'Auto' tracks).
    if let crate::types::CssLength::Px(px) = node.style.width {
        if px >= 0.0 { return px; }
    }

    // Line positions are absolute; subtract the box's own content origin so we
    // measure content-relative width (avoids double-counting padding/border).
    let origin = node.content_rect.x;
    let mut w = 0.0f32;
    // Inline line widths
    for line in &node.line_cache {
        let rw = line.x + line.width - origin;
        if rw > w { w = rw; }
    }
    // Children
    for ch in &node.children {
        if matches!(ch.style.display, Display::None) { continue; }
        if matches!(ch.style.position, Position::Absolute | Position::Fixed) { continue; }
        // Inline-display children: only measure text nodes that have been laid out
        // as standalone items (e.g. flex children). Regular inline content is in line_cache.
        if matches!(ch.style.display, Display::Inline) {
            if ch.is_text_node() && !ch.line_cache.is_empty() {
                // Text node flex child: its intrinsic width is its own line widths
                let cw = compute_intrinsic_width(ch);
                let total = cw
                    + ch.resolved_pad_left + ch.resolved_pad_right
                    + ch.resolved_border_left + ch.resolved_border_right
                    + ch.resolved_margin_left + ch.resolved_margin_right;
                if total > w { w = total; }
            }
            continue;
        }
        // Block children with auto width: their marginRect is inflated to containing width.
        // Recurse to get real content width.
        let is_auto_width_block = ch.style.width.is_auto()
            && matches!(ch.style.display,
                Display::Block | Display::ListItem);
        if is_auto_width_block {
            let child_content = compute_intrinsic_width(ch);
            let total = child_content
                + ch.resolved_pad_left + ch.resolved_pad_right
                + ch.resolved_border_left + ch.resolved_border_right
                + ch.resolved_margin_left + ch.resolved_margin_right;
            if total > w { w = total; }
        } else {
            // Use origin-relative position for non-block children (e.g. fixed-width blocks, images)
            let rw = (ch.margin_rect.x - origin) + ch.margin_rect.w;
            if rw > w { w = rw; }
        }
    }
    // Add 1px epsilon to prevent floating-point rounding from causing spurious wraps
    // when the layout re-runs at exactly the measured width.
    if w > 0.0 { w + 1.0 } else { w }
}

// ─── Apply relative offset ────────────────────────────────────────────────────

/// Apply position:relative offset to a node's rects after layout.
/// Mirrors C++ ApplyRelativeOffset.
pub fn apply_relative_offset(node: &mut HtmlBox, font_px: f32, containing_w: f32, root_font_px: f32) {
    if !matches!(node.style.position, Position::Relative) { return; }
    let dx = if !node.style.left.is_auto() {
        node.style.left.resolve(font_px, containing_w, root_font_px)
    } else if !node.style.right.is_auto() {
        -node.style.right.resolve(font_px, containing_w, root_font_px)
    } else {
        0.0
    };
    let dy = if !node.style.top.is_auto() {
        node.style.top.resolve(font_px, containing_w, root_font_px)
    } else if !node.style.bottom.is_auto() {
        -node.style.bottom.resolve(font_px, containing_w, root_font_px)
    } else {
        0.0
    };
    if dx != 0.0 || dy != 0.0 {
        shift_rects(node, dx, dy);
    }
}

// ─── Build box rects and cache resolved values ────────────────────────────────

/// Set node rects from rbox and geometry.
/// Mirrors C++ BuildBoxRects.
pub fn build_box_rects(
    node: &mut HtmlBox,
    rbox: &ResolvedBox,
    content_x: f32, content_y: f32,
    content_w: f32, content_h: f32,
    margin_left: f32, margin_right: f32,
) {
    node.content_rect = Rect::new(content_x, content_y, content_w, content_h);
    node.padding_rect = Rect::new(
        content_x - rbox.padding_left, content_y - rbox.padding_top,
        content_w + rbox.padding_left + rbox.padding_right,
        content_h + rbox.padding_top  + rbox.padding_bottom,
    );
    node.border_rect = Rect::new(
        node.padding_rect.x - rbox.border_left,
        node.padding_rect.y - rbox.border_top,
        node.padding_rect.w + rbox.border_left + rbox.border_right,
        node.padding_rect.h + rbox.border_top  + rbox.border_bottom,
    );
    node.margin_rect = Rect::new(
        node.border_rect.x - margin_left,
        node.border_rect.y - rbox.margin_top,
        node.border_rect.w + margin_left + margin_right,
        node.border_rect.h + rbox.margin_top + rbox.margin_bottom,
    );
    node.baseline = content_y + content_h;

    // Cache resolved values
    node.resolved_margin_top    = rbox.margin_top;
    node.resolved_margin_right  = rbox.margin_right;
    node.resolved_margin_bottom = rbox.margin_bottom;
    node.resolved_margin_left   = margin_left;
    node.resolved_border_top    = rbox.border_top;
    node.resolved_border_right  = rbox.border_right;
    node.resolved_border_bottom = rbox.border_bottom;
    node.resolved_border_left   = rbox.border_left;
    node.resolved_pad_top       = rbox.padding_top;
    node.resolved_pad_right     = rbox.padding_right;
    node.resolved_pad_bottom    = rbox.padding_bottom;
    node.resolved_pad_left      = rbox.padding_left;
    node.resolved_content_width = content_w;
}

// ─── Block formatting context layout ─────────────────────────────────────────

/// Block formatting context layout.
/// Mirrors C++ LayoutBlockFlow.
pub fn layout_block(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    rbox:         &ResolvedBox,
    containing_w: f32,
    x:            f32,
    y:            f32,
    font_px:      f32,
    root_font_px: f32,
) -> f32 {
    layout_block_with_fc(engine, node, rbox, containing_w, x, y, font_px, root_font_px, None)
}

/// Block formatting context layout with optional parent float context.
/// Non-BFC blocks share parent's float context; BFC blocks get their own.
pub fn layout_block_with_fc(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    rbox:         &ResolvedBox,
    containing_w: f32,
    x:            f32,
    y:            f32,
    font_px:      f32,
    root_font_px: f32,
    parent_fc:    Option<&mut FloatContext>,
) -> f32 {
    // Content width: respect box-sizing (already resolved in rbox via resolve_box).
    let raw_w = match rbox.content_width {
        Some(w) => w,
        None => (containing_w - rbox.h_space()).max(0.0),
    };

    // Apply min/max-width constraints
    let min_w = engine.res_len(&node.style.min_width, font_px, containing_w, root_font_px);
    let max_w = if node.style.max_width.is_none() { f32::MAX }
                else { engine.res_len(&node.style.max_width, font_px, containing_w, root_font_px) };
    let content_w = raw_w.max(min_w).min(max_w);

    // Auto margin centering (CSS 2.1 §10.3.3)
    let left_is_auto  = node.style.margin_left.is_auto();
    let right_is_auto = node.style.margin_right.is_auto();
    let (margin_left, margin_right) =
        if !node.style.width.is_auto() && (left_is_auto || right_is_auto) {
            let non_margin_space = rbox.border_left + rbox.padding_left + content_w
                                 + rbox.padding_right + rbox.border_right;
            let available = (containing_w - non_margin_space).max(0.0);
            if left_is_auto && right_is_auto {
                let ml = (available / 2.0).floor();
                (ml, available - ml)
            } else if left_is_auto {
                (available - rbox.margin_right, rbox.margin_right)
            } else {
                (rbox.margin_left, available - rbox.margin_left)
            }
        } else {
            (rbox.margin_left, rbox.margin_right)
        };

    let is_bfc = establishes_bfc(&node.style);

    let content_x = x + margin_left + rbox.border_left + rbox.padding_left;
    let content_y = y + rbox.margin_top + rbox.border_top + rbox.padding_top;

    // ─── CSS margin collapsing setup ──────────────────────────────────────────
    let can_collapse_top    = can_collapse_top_with_first_child(node, rbox);
    let can_collapse_bottom = can_collapse_bottom_with_last_child(node, rbox);

    // ─── Collect out-of-flow children ─────────────────────────────────────────
    let mut abs_children: Vec<usize> = Vec::new();
    for i in 0..node.children.len() {
        if matches!(node.children[i].style.position, Position::Absolute | Position::Fixed) {
            abs_children.push(i);
        }
    }

    // ─── Float context ────────────────────────────────────────────────────────
    let mut fc_owned;
    let fc = if let Some(f) = parent_fc {
        f
    } else {
        fc_owned = FloatContext::default();
        fc_owned.origin_y = content_y;
        &mut fc_owned
    };

    // ─── Main block children loop ─────────────────────────────────────────────
    let mut child_y = 0.0f32;
    let mut prev_bottom_margin = 0.0f32;
    let mut is_first_in_flow  = true;
    let mut first_child_collapsed = false;
    let mut first_in_flow_idx: Option<usize> = None;
    let mut last_in_flow_idx:  Option<usize> = None;
    let mut seen_float = false;

    for i in 0..node.children.len() {
        let child_display  = node.children[i].style.display;
        let child_float    = node.children[i].style.float;
        let child_clear    = node.children[i].style.clear;
        let child_position = node.children[i].style.position;

        if matches!(child_display, Display::None) { continue; }
        if matches!(child_position, Position::Absolute | Position::Fixed) { continue; }

        // Handle clear
        match child_clear {
            Clear::None => {}
            clear => {
                child_y = fc.clear_y(content_y + child_y - fc.origin_y, clear) - (content_y - fc.origin_y);
                prev_bottom_margin = 0.0;
            }
        }

        if !matches!(child_float, Float::None) {
            seen_float = true;
            // Layout float to get natural size
            engine.layout_box(
                &mut node.children[i], content_w, content_x, content_y + child_y,
                font_px, root_font_px
            );
            // Shrink-to-fit for auto-width floats
            if node.children[i].style.width.is_auto() {
                let intrinsic_w = compute_intrinsic_width(&node.children[i]);
                if intrinsic_w > 0.0 && intrinsic_w < content_w {
                    let irb = &node.children[i];
                    let shrink_w = intrinsic_w
                        + irb.resolved_pad_left + irb.resolved_pad_right
                        + irb.resolved_border_left + irb.resolved_border_right
                        + irb.resolved_margin_left + irb.resolved_margin_right;
                    engine.layout_box(
                        &mut node.children[i], shrink_w, content_x, content_y + child_y,
                        font_px, root_font_px
                    );
                }
            }
            let float_w = node.children[i].margin_rect.w;
            let float_h = node.children[i].margin_rect.h;
            let side = if child_float == Float::Left { FloatSide::Left } else { FloatSide::Right };
            let placed = fc.place_float(content_y + child_y - fc.origin_y, float_w, float_h, content_w, side);
            let dx = content_x + placed.x - node.children[i].margin_rect.x;
            let dy = content_y + placed.y - node.children[i].margin_rect.y;
            shift_rects(&mut node.children[i], dx, dy);

            if matches!(node.children[i].style.position, Position::Relative) {
                let rel_font_px = node.children[i].style.font_size_px(font_px, root_font_px);
                apply_relative_offset(&mut node.children[i], rel_font_px, content_w, root_font_px);
            }
            continue;
        }

        if node.children[i].style.is_block_level() {
            // Layout child to get its geometry
            engine.layout_box(
                &mut node.children[i], content_w, content_x, content_y,
                font_px, root_font_px
            );

            // Use child's collapsed margins (includes grandchild pass-through)
            let child_top_margin    = node.children[i].collapsed_margin_top;
            let child_bottom_margin = node.children[i].collapsed_margin_bottom;

            if is_first_in_flow && can_collapse_top && !seen_float {
                // Parent-first-child collapsing: child's top margin is absorbed into parent
                first_child_collapsed = true;
                first_in_flow_idx = Some(i);
                // Don't advance child_y for this child's top margin
            } else {
                let collapsed = collapse_two(prev_bottom_margin, child_top_margin);
                child_y += collapsed - prev_bottom_margin;
                is_first_in_flow = false; // value may not be read again
            }
            let _ = is_first_in_flow;

            // Check available width from floats
            let child_h = node.children[i].margin_rect.h;
            let mut left_edge = 0.0f32;
            let mut right_edge = content_w;
            fc.available_width(content_y + child_y - fc.origin_y, child_h, content_w, &mut left_edge, &mut right_edge);

            // Rebuild rects at correct position using cached resolved values
            let child_margin_left  = node.children[i].resolved_margin_left;
            let child_margin_right = node.children[i].resolved_margin_right;
            let child_border_top   = node.children[i].resolved_border_top;
            let child_pad_top      = node.children[i].resolved_pad_top;
            let child_content_h    = node.children[i].content_rect.h;
            let child_rbox_copy = ResolvedBox {
                margin_top:    node.children[i].resolved_margin_top,
                margin_right:  child_margin_right,
                margin_bottom: node.children[i].resolved_margin_bottom,
                margin_left:   child_margin_left,
                border_top:    child_border_top,
                border_right:  node.children[i].resolved_border_right,
                border_bottom: node.children[i].resolved_border_bottom,
                border_left:   node.children[i].resolved_border_left,
                padding_top:   node.children[i].resolved_pad_top,
                padding_right: node.children[i].resolved_pad_right,
                padding_bottom:node.children[i].resolved_pad_bottom,
                padding_left:  node.children[i].resolved_pad_left,
                content_width: Some(node.children[i].resolved_content_width),
                content_height:Some(child_content_h),
            };
            let cx = content_x + left_edge + child_margin_left + child_rbox_copy.border_left + child_rbox_copy.padding_left;
            let cy = content_y + child_y   + child_border_top  + child_pad_top;
            let dx = cx - node.children[i].content_rect.x;
            let dy = cy - node.children[i].content_rect.y;
            shift_rects(&mut node.children[i], dx, dy);

            if matches!(node.children[i].style.position, Position::Relative) {
                let rel_font_px = node.children[i].style.font_size_px(font_px, root_font_px);
                apply_relative_offset(&mut node.children[i], rel_font_px, content_w, root_font_px);
            }

            child_y = node.children[i].margin_rect.y - content_y
                    + node.children[i].margin_rect.h;
            prev_bottom_margin = child_bottom_margin;
            last_in_flow_idx = Some(i);
            is_first_in_flow = false;

        } else if node.children[i].style.is_inline_level() {
            // Skip atomic inline-run boxes (positioned by LayoutInlines via inlineContent).
            // In the Rust architecture these appear as children when inline content
            // is processed; simply skip them here as in C++ isAtomicInlineRun check.
            // (If there is no inline content, lay them out as block-like items.)
            if node.line_cache.is_empty() {
                engine.layout_box(
                    &mut node.children[i], content_w, content_x, content_y + child_y,
                    font_px, root_font_px
                );
                let child_content_h = node.children[i].content_rect.h;
                let child_rbox_copy = ResolvedBox {
                    margin_top:    node.children[i].resolved_margin_top,
                    margin_right:  node.children[i].resolved_margin_right,
                    margin_bottom: node.children[i].resolved_margin_bottom,
                    margin_left:   node.children[i].resolved_margin_left,
                    border_top:    node.children[i].resolved_border_top,
                    border_right:  node.children[i].resolved_border_right,
                    border_bottom: node.children[i].resolved_border_bottom,
                    border_left:   node.children[i].resolved_border_left,
                    padding_top:   node.children[i].resolved_pad_top,
                    padding_right: node.children[i].resolved_pad_right,
                    padding_bottom:node.children[i].resolved_pad_bottom,
                    padding_left:  node.children[i].resolved_pad_left,
                    content_width: Some(node.children[i].resolved_content_width),
                    content_height:Some(child_content_h),
                };
                let cx = content_x + child_rbox_copy.border_left + child_rbox_copy.padding_left;
                let cy = content_y + child_y + child_rbox_copy.border_top + child_rbox_copy.padding_top;
                let dx = cx - node.children[i].content_rect.x;
                let dy = cy - node.children[i].content_rect.y;
                shift_rects(&mut node.children[i], dx, dy);
                child_y += node.children[i].margin_rect.h;

                if matches!(node.children[i].style.position, Position::Relative) {
                    let rel_font_px = node.children[i].style.font_size_px(font_px, root_font_px);
                    apply_relative_offset(&mut node.children[i], rel_font_px, content_w, root_font_px);
                }
            }
            // else: skip — already positioned by LayoutInlines (line_cache exists)
        }
    }

    // ─── Parent-last-child bottom margin collapsing ───────────────────────────
    let _last_child_collapsed_bottom = if let Some(idx) = last_in_flow_idx {
        if can_collapse_bottom {
            let lcb = node.children[idx].collapsed_margin_bottom;
            child_y -= lcb;
            lcb
        } else {
            0.0
        }
    } else {
        0.0
    };

    // ─── Content height ───────────────────────────────────────────────────────
    // Include float bottom if BFC
    let float_bottom = if is_bfc {
        fc.floats.iter().map(|f| f.clear).fold(0.0f32, f32::max)
    } else {
        0.0
    };
    // Include inline content (line_cache) height
    let inline_bottom = if !node.line_cache.is_empty() {
        let last = node.line_cache.last().unwrap();
        last.y - content_y + last.height
    } else {
        0.0
    };
    let natural_h = child_y.max(float_bottom).max(inline_bottom);

    let content_h = match rbox.content_height {
        Some(h) => h,
        None    => natural_h,
    };

    // Apply min/max-height
    let min_h = engine.res_len(&node.style.min_height, font_px, 0.0, root_font_px);
    let max_h = if node.style.max_height.is_none() { f32::MAX }
                else { engine.res_len(&node.style.max_height, font_px, 0.0, root_font_px) };
    let content_h = content_h.max(min_h).min(max_h).max(0.0);

    // ─── Build rects ──────────────────────────────────────────────────────────
    build_box_rects(node, rbox, content_x, content_y, content_w, content_h, margin_left, margin_right);

    // ─── Scroll extent ────────────────────────────────────────────────────────
    if matches!(node.style.overflow_x, Overflow::Scroll | Overflow::Auto)
    || matches!(node.style.overflow_y, Overflow::Scroll | Overflow::Auto)
    {
        let natural_scroll_h = child_y.max(float_bottom).max(inline_bottom)
                                      .max(content_h);
        node.scroll_height = natural_scroll_h;
        node.scroll_width  = content_w;
        // Clamp scrollTop
        let max_scroll = (node.scroll_height - content_h).max(0.0);
        node.scroll_top = node.scroll_top.min(max_scroll).max(0.0);
    } else {
        node.scroll_height = content_h;
        node.scroll_width  = content_w;
        node.scroll_top    = 0.0;
        node.scroll_left   = 0.0;
    }

    // ─── Collapsed margins (pass-through to parent) ───────────────────────────
    node.collapsed_margin_top    = rbox.margin_top;
    node.collapsed_margin_bottom = rbox.margin_bottom;

    if is_empty_block(node, rbox) {
        // Empty block: own top and bottom margins collapse
        let own = collapse_two(rbox.margin_top, rbox.margin_bottom);
        node.collapsed_margin_top    = own;
        node.collapsed_margin_bottom = 0.0;
    } else {
        // Parent-first-child collapsing
        if first_child_collapsed {
            if let Some(idx) = first_in_flow_idx {
                node.collapsed_margin_top =
                    collapse_two(rbox.margin_top, node.children[idx].collapsed_margin_top);
            }
        }
        // Parent-last-child collapsing
        if can_collapse_bottom {
            if let Some(idx) = last_in_flow_idx {
                node.collapsed_margin_bottom =
                    collapse_two(rbox.margin_bottom, node.children[idx].collapsed_margin_bottom);
            }
        }
    }

    // ─── Absolute/fixed children ──────────────────────────────────────────────
    // Use the padding box as the containing block for positioned children
    // (CSS: containing block for absolutely positioned elements is the padding box
    //  of the nearest positioned ancestor).
    let containing_rect = node.padding_rect;
    for &i in &abs_children {
        let child = &mut node.children[i];
        layout_positioned(engine, child, containing_rect, font_px, root_font_px);
    }

    // ─── Relative offsets ─────────────────────────────────────────────────────
    // (already applied per-child above; nothing more needed here)

    node.layout_dirty = false;
    node.last_containing_width = containing_w;

    node.margin_rect.h
}
