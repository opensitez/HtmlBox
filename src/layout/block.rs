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
    let cached = node.cached_intrinsic_w.get();
    if !cached.is_nan() { return cached; }
    let result = compute_intrinsic_width_inner(node);
    node.cached_intrinsic_w.set(result);
    result
}

fn compute_intrinsic_width_inner(node: &HtmlBox) -> f32 {
    // If the box has a fixed width, that IS its intrinsic width (min and max).
    // (In a more complete engine we'd distinguish min-content vs max-content,
    // but for now max-content is what matters for 'Auto' tracks).
    if let crate::types::CssLength::Px(px) = node.style.width {
        if px >= 0.0 { return px; }
    }

    // For row-direction flex containers, the intrinsic width is the SUM of all
    // flex items' intrinsic widths (+ padding/border/margin), not the max of their
    // laid-out margin_rect positions (which reflect the container width, not content).
    let is_row_flex = matches!(node.style.display, Display::Flex | Display::InlineFlex)
        && matches!(node.style.flex_direction, FlexDirection::Row | FlexDirection::RowReverse);
    if is_row_flex {
        let mut total = 0.0f32;
        for ch in &node.children {
            if matches!(ch.style.display, Display::None) { continue; }
            if matches!(ch.style.position, Position::Absolute | Position::Fixed) { continue; }
            if ch.tag == "#text" && ch.text.chars().all(|c| c.is_ascii_whitespace()) { continue; }
            let child_w = compute_intrinsic_width(ch)
                + ch.resolved_pad_left + ch.resolved_pad_right
                + ch.resolved_border_left + ch.resolved_border_right
                + ch.resolved_margin_left + ch.resolved_margin_right;
            total += child_w;
        }
        return if total > 0.0 { total + 1.0 } else { total };
    }

    let origin = node.content_rect.x;
    let mut w = 0.0f32;
    // Inline line widths — use line.width (raw content width) directly.
    // line.x includes the text-align offset (e.g. centred text shifts line.x right by
    // (avail_w − text_w) / 2), so line.x + line.width − origin would over-report the
    // intrinsic width for centred/right-aligned content.
    for line in &node.line_cache {
        if line.width > w { w = line.width; }
    }
    // In a flex/grid formatting context, children are positioned by flex/grid layout
    // (not stacked vertically), so use their actual margin_rect extents for all children.
    let is_flex_or_grid = matches!(node.style.display,
        Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid);

    // Children
    for ch in &node.children {
        if matches!(ch.style.display, Display::None) { continue; }
        if matches!(ch.style.position, Position::Absolute | Position::Fixed) { continue; }
        // Inline-display children: measure text nodes and inline elements that were
        // laid out as standalone flex/grid items. Regular inline content is in line_cache.
        if !is_flex_or_grid && matches!(ch.style.display, Display::Inline) {
            if ch.is_text_node() && !ch.line_cache.is_empty() {
                // Text node flex child: its intrinsic width is its own line widths
                let cw = compute_intrinsic_width(ch);
                let total = cw
                    + ch.resolved_pad_left + ch.resolved_pad_right
                    + ch.resolved_border_left + ch.resolved_border_right
                    + ch.resolved_margin_left + ch.resolved_margin_right;
                if total > w { w = total; }
            } else if !ch.is_text_node() && node.line_cache.is_empty() {
                // In a block context with mixed block/inline children, line_cache is
                // empty so inline children aren't captured there.  Recurse to get
                // their intrinsic width (e.g. <a> wrapping an <img width=200>).
                let cw = compute_intrinsic_width(ch);
                let total = cw
                    + ch.resolved_pad_left + ch.resolved_pad_right
                    + ch.resolved_border_left + ch.resolved_border_right
                    + ch.resolved_margin_left + ch.resolved_margin_right;
                if total > w { w = total; }
            }
            // When line_cache IS populated, non-text inline elements are already
            // captured there. Their margin_rect.x is stale after shift_rects, so skip.
            continue;
        }
        // Block children with auto width: their marginRect is inflated to containing width.
        // Recurse to get real content width.
        // Floated children are positioned by the float algorithm (not stacked
        // vertically), so use their laid-out right edge for intrinsic width.
        if !matches!(ch.style.float, Float::None) {
            let right = (ch.margin_rect.x - origin) + ch.margin_rect.w;
            if right > w { w = right; }
            continue;
        }
        // Container children with auto or percentage width: their margin_rect
        // is inflated to the containing width during layout, so recurse to get the
        // real intrinsic content width. Percentage widths resolve to the container
        // width during layout, which doesn't reflect intrinsic content width.
        let is_fluid_width_container = (ch.style.width.is_auto() || matches!(ch.style.width, CssLength::Percent(_)))
            && matches!(ch.style.display,
                Display::Block | Display::ListItem | Display::Flex | Display::InlineFlex
                | Display::Grid | Display::InlineGrid);
        if is_fluid_width_container {
            let child_content = compute_intrinsic_width(ch);
            let total = child_content
                + ch.resolved_pad_left + ch.resolved_pad_right
                + ch.resolved_border_left + ch.resolved_border_right
                + ch.resolved_margin_left + ch.resolved_margin_right;
            if total > w { w = total; }
        } else {
            // Skip whitespace-only text nodes in flex/grid containers — they are not
            // laid out as flex items and their margin_rect accumulates stale position
            // offsets across re-renders (via shift_rects), producing spuriously large widths.
            if is_flex_or_grid && ch.is_text_node()
                && ch.text.chars().all(|c| c.is_ascii_whitespace())
            {
                continue;
            }
            // InlineBlock/InlineFlex/InlineGrid children inside inline flow are already
            // captured as Atomic items in the parent's line_cache widths.  Their
            // margin_rect.x includes text-align centering offsets which would inflate
            // the intrinsic width.  Skip them when line_cache is non-empty.
            if !node.line_cache.is_empty()
                && matches!(ch.style.display,
                    Display::InlineBlock | Display::InlineFlex | Display::InlineGrid)
            {
                continue;
            }
            // Fixed-width or non-block child. Avoid counting auto margins (e.g. `margin: 0 auto`
            // on a centered image): those expand to the container width during layout but are
            // not part of the element's intrinsic size.
            let has_auto_h_margin = ch.style.margin_left.is_auto() || ch.style.margin_right.is_auto();
            let rw = if has_auto_h_margin {
                // Content + padding + border + any non-auto margins.
                ch.content_rect.w
                    + ch.resolved_pad_left   + ch.resolved_pad_right
                    + ch.resolved_border_left + ch.resolved_border_right
                    + (if ch.style.margin_left.is_auto()  { 0.0 } else { ch.resolved_margin_left  })
                    + (if ch.style.margin_right.is_auto() { 0.0 } else { ch.resolved_margin_right })
            } else {
                (ch.margin_rect.x - origin) + ch.margin_rect.w
            };
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
    // For the margin-rect width, negative margins can collapse it to zero or less.
    // Clamp to at least the border-box width so floats with negative margins
    // (e.g. float:left; width:320px; margin-left:-320px) occupy their visual width.
    let mr_w = (node.border_rect.w + margin_left + margin_right).max(node.border_rect.w);
    node.margin_rect = Rect::new(
        node.border_rect.x - margin_left,
        node.border_rect.y - rbox.margin_top,
        mr_w,
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

    // Apply min/max-width constraints, converting from border-box to content-box when needed.
    // CSS: with box-sizing:border-box, min/max-width refer to the border box, not the content box.
    let bb_extra = if node.style.box_sizing == crate::types::BoxSizing::BorderBox {
        rbox.padding_left + rbox.padding_right + rbox.border_left + rbox.border_right
    } else { 0.0 };
    let min_w = {
        let v = engine.res_len(&node.style.min_width, font_px, containing_w, root_font_px);
        (v - bb_extra).max(0.0)
    };
    let max_w = if node.style.max_width.is_none() { f32::MAX } else {
        let v = engine.res_len(&node.style.max_width, font_px, containing_w, root_font_px);
        (v - bb_extra).max(0.0)
    };
    let content_w = raw_w.max(min_w).min(max_w);

    // ── Scrollbar width reservation ───────────────────────────────────────────
    // A vertical scrollbar (10px wide) overlays the right edge of the content box.
    // Without reserving that space, children are laid out at full content_w and
    // their rightmost 10px gets painted over by the scrollbar.
    //
    // • overflow-y: scroll → scrollbar is always present: always reserve.
    // • overflow-y: auto with max-height → scrollbar appears when content
    //   overflows max-height, which is the common case for demo panels; reserve
    //   proactively.  (A full two-pass layout would be needed for perfect accuracy
    //   but is unnecessary for the demos that trigger this path.)
    const SBW: f32 = 10.0; // must match renderer::SCROLLBAR_WIDTH
    let reserve_v_scrollbar =
        matches!(node.style.overflow_y, Overflow::Scroll)
        || (matches!(node.style.overflow_y, Overflow::Auto)
            && !node.style.max_height.is_none());
    let child_content_w = if reserve_v_scrollbar { (content_w - SBW).max(0.0) } else { content_w };

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

    // ─── Multi-column layout (early return path) ──────────────────────────────
    if establishes_column_context(&node.style) && !node.children.is_empty() {
        let col_h = layout_columns(engine, node, rbox, content_x, content_y, content_w, font_px, root_font_px);
        let content_h = match rbox.content_height { Some(h) => h, None => col_h };
        let min_h = engine.res_len(&node.style.min_height, font_px, 0.0, root_font_px);
        let max_h = if node.style.max_height.is_none() { f32::MAX }
                    else {
                        let v = engine.res_len(&node.style.max_height, font_px, 0.0, root_font_px);
                        if v == 0.0 && matches!(node.style.max_height, CssLength::Percent(_)) { f32::MAX } else { v }
                    };
        let content_h = content_h.max(min_h).min(max_h).max(0.0);
        build_box_rects(node, rbox, content_x, content_y, content_w, content_h, margin_left, margin_right);
        // Absolute/fixed children
        let containing_rect = if !matches!(node.style.position, Position::Static) {
            node.padding_rect
        } else {
            engine.pos_cb.get()
        };
        for &i in &abs_children {
            let child = &mut node.children[i];
            layout_positioned(engine, child, containing_rect, font_px, root_font_px);
        }
        node.layout_dirty = false;
        node.last_containing_width = containing_w;
        return node.margin_rect.h;
    }

    // ─── Main block children loop ─────────────────────────────────────────────
    let mut child_y = 0.0f32;
    let mut prev_bottom_margin = 0.0f32;
    let mut is_first_in_flow  = true;
    let mut first_child_collapsed = false;
    let mut first_in_flow_idx: Option<usize> = None;
    let mut last_in_flow_idx:  Option<usize> = None;
    let mut seen_float = false;
    // Inline flow state for anonymous inline formatting contexts
    let mut inline_x = 0.0f32;
    let mut inline_line_h = 0.0f32;

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

        // Flush any pending inline line before a block or float child
        if (node.children[i].style.is_block_level() || !matches!(child_float, Float::None))
            && inline_line_h > 0.0
        {
            child_y += inline_line_h;
            inline_x = 0.0;
            inline_line_h = 0.0;
        }

        if !matches!(child_float, Float::None) {
            seen_float = true;
            // Layout float to get natural size
            engine.layout_box(
                &mut node.children[i], child_content_w, content_x, content_y + child_y,
                font_px, root_font_px
            );
            // Shrink-to-fit for auto-width floats
            if node.children[i].style.width.is_auto() {
                let intrinsic_w = compute_intrinsic_width(&node.children[i]);
                if intrinsic_w > 0.0 && intrinsic_w < child_content_w {
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
            // For float placement, use the effective margin-box width including
            // negative margins.  A float with width:320px; margin-left:-320px
            // occupies 0px of horizontal space (Holy Grail pattern).
            let ch = &node.children[i];
            let effective_w = ch.border_rect.w
                + ch.resolved_margin_left + ch.resolved_margin_right;
            let float_w = effective_w.max(0.0);
            let float_h = ch.margin_rect.h;
            let side = if child_float == Float::Left { FloatSide::Left } else { FloatSide::Right };
            let placed = fc.place_float(content_y + child_y - fc.origin_y, float_w, float_h, child_content_w, side);
            let dx = content_x + placed.x - node.children[i].margin_rect.x;
            let dy = content_y + placed.y - node.children[i].margin_rect.y;
            shift_rects(&mut node.children[i], dx, dy);

            if matches!(node.children[i].style.position, Position::Relative | Position::Sticky) {
                let rel_font_px = node.children[i].style.font_size_px(font_px, root_font_px);
                apply_relative_offset(&mut node.children[i], rel_font_px, child_content_w, root_font_px);
            }
            continue;
        }

        if node.children[i].style.is_block_level() {
            // Layout child to get its geometry
            engine.layout_box(
                &mut node.children[i], child_content_w, content_x, content_y,
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

            // Check available width from floats.
            // Per CSS §9.5.1, only BFC-establishing blocks must not overlap float
            // margin boxes.  Normal in-flow blocks extend to full container width;
            // their *inline content* wraps around floats (handled by the float
            // context during line breaking).
            let child_h = node.children[i].margin_rect.h;
            let mut left_edge = 0.0f32;
            let mut right_edge = child_content_w;
            let child_is_bfc = establishes_bfc(&node.children[i].style);
            if child_is_bfc {
                fc.available_width(content_y + child_y - fc.origin_y, child_h, child_content_w, &mut left_edge, &mut right_edge);
            }

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

            if matches!(node.children[i].style.position, Position::Relative | Position::Sticky) {
                let rel_font_px = node.children[i].style.font_size_px(font_px, root_font_px);
                apply_relative_offset(&mut node.children[i], rel_font_px, child_content_w, root_font_px);
            }

            child_y = node.children[i].margin_rect.y - content_y
                    + node.children[i].margin_rect.h;
            prev_bottom_margin = child_bottom_margin;
            last_in_flow_idx = Some(i);
            is_first_in_flow = false;

        } else if node.children[i].style.is_inline_level() {
            // Inline-level children in a block container with mixed content form an
            // anonymous inline formatting context (CSS §9.2.1.1).  Lay them out with
            // horizontal flow, wrapping to the next line when they exceed the container.
            let is_whitespace_only_text = node.children[i].is_text_node()
                && node.children[i].text.chars().all(|c| c.is_ascii_whitespace());
            if node.line_cache.is_empty() && !is_whitespace_only_text {
                // Layout to get dimensions
                engine.layout_box(
                    &mut node.children[i], child_content_w, content_x, content_y + child_y,
                    font_px, root_font_px
                );
                // Shrink-to-fit for auto-width InlineBlock children (CSS §10.3.9)
                if node.children[i].style.width.is_auto()
                    && matches!(node.children[i].style.display,
                        Display::InlineBlock | Display::InlineFlex | Display::InlineGrid)
                {
                    let max_line_w = node.children[i].line_cache.iter()
                        .map(|l| l.width).fold(0.0_f32, f32::max);
                    let intrinsic_w = if max_line_w > 0.0 { max_line_w }
                                      else { compute_intrinsic_width(&node.children[i]) };
                    if intrinsic_w > 0.0 {
                        let irb = &node.children[i];
                        let shrink_w = intrinsic_w
                            + irb.resolved_pad_left + irb.resolved_pad_right
                            + irb.resolved_border_left + irb.resolved_border_right
                            + irb.resolved_margin_left + irb.resolved_margin_right;
                        if shrink_w < child_content_w {
                            engine.layout_box(
                                &mut node.children[i], shrink_w, content_x, content_y + child_y,
                                font_px, root_font_px
                            );
                        }
                    }
                }

                let child_mw = node.children[i].margin_rect.w;
                let child_mh = node.children[i].margin_rect.h;

                // Horizontal flow: wrap to next line if this child doesn't fit
                if inline_x > 0.0 && inline_x + child_mw > child_content_w {
                    // Wrap: advance to next line
                    child_y += inline_line_h;
                    inline_x = 0.0;
                    inline_line_h = 0.0;
                }

                // Position child at (content_x + inline_x, content_y + child_y)
                let dx = content_x + inline_x - node.children[i].margin_rect.x;
                let dy = content_y + child_y  - node.children[i].margin_rect.y;
                if dx.abs() > 0.01 || dy.abs() > 0.01 {
                    shift_rects(&mut node.children[i], dx, dy);
                }

                inline_x += child_mw;
                if child_mh > inline_line_h { inline_line_h = child_mh; }

                if matches!(node.children[i].style.position, Position::Relative | Position::Sticky) {
                    let rel_font_px = node.children[i].style.font_size_px(font_px, root_font_px);
                    apply_relative_offset(&mut node.children[i], rel_font_px, child_content_w, root_font_px);
                }
            }
            // else: skip — already positioned by LayoutInlines (line_cache exists)
        }
    }

    // Flush trailing inline line
    if inline_line_h > 0.0 {
        child_y += inline_line_h;
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
                else {
                    let v = engine.res_len(&node.style.max_height, font_px, 0.0, root_font_px);
                    if v == 0.0 && matches!(node.style.max_height, CssLength::Percent(_)) { f32::MAX } else { v }
                };
    let content_h = content_h.max(min_h).min(max_h).max(0.0);

    // Apply aspect-ratio: if height is auto and aspect_ratio is set, derive height from width
    let content_h = if rbox.content_height.is_none() {
        if let Some(ratio) = node.style.aspect_ratio {
            if ratio > 0.0 { (content_w / ratio).max(0.0) } else { content_h }
        } else { content_h }
    } else { content_h };

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
    let containing_rect = if !matches!(node.style.position, Position::Static) {
        node.padding_rect
    } else {
        engine.pos_cb.get()
    };
    for &i in &abs_children {
        layout_positioned(engine, &mut node.children[i], containing_rect, font_px, root_font_px);
        // For abs children with all insets auto, clamp to containing block's top-left.
        // layout_positioned leaves them at wherever layout_box placed them (origin 0,0)
        // which is inside the containing block only if the containing block is at 0,0.
        let child = &mut node.children[i];
        let all_auto = child.style.left.is_auto()  && child.style.right.is_auto()
                    && child.style.top.is_auto()   && child.style.bottom.is_auto();
        if all_auto && matches!(child.style.position, Position::Absolute) {
            let dx = containing_rect.x - child.border_rect.x;
            let dy = containing_rect.y - child.border_rect.y;
            if dx != 0.0 || dy != 0.0 {
                crate::layout::shift_rects(child, dx, dy);
            }
        }
    }

    // ─── Relative offsets ─────────────────────────────────────────────────────
    // (already applied per-child above; nothing more needed here)

    node.layout_dirty = false;
    node.last_containing_width = containing_w;

    node.margin_rect.h
}

// ─── Multi-column layout ──────────────────────────────────────────────────────

/// Returns true if this element establishes a multi-column container.
pub fn establishes_column_context(style: &ComputedStyle) -> bool {
    style.column_count.is_some() || !style.column_width.is_auto()
}

/// Lay out node's children in a multi-column arrangement.
/// Returns the total content height.
pub fn layout_columns(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    _rbox:        &ResolvedBox,
    content_x:    f32,
    content_y:    f32,
    content_w:    f32,
    font_px:      f32,
    root_font_px: f32,
) -> f32 {
    // 1. Determine column gap
    let gap = if !node.style.column_gap.is_auto() {
        engine.res_len(&node.style.column_gap, font_px, content_w, root_font_px)
    } else {
        font_px  // Default gap is 1em
    };

    // 2. Determine column count
    let col_count_from_width = if !node.style.column_width.is_auto() {
        let cw = engine.res_len(&node.style.column_width, font_px, content_w, root_font_px);
        if cw > 0.0 { ((content_w + gap) / (cw + gap)).floor().max(1.0) as u32 } else { 1 }
    } else {
        u32::MAX
    };

    let n_cols = match node.style.column_count {
        Some(c) if c > 0 => {
            let c = c as u32;
            if !node.style.column_width.is_auto() { c.min(col_count_from_width) } else { c }
        }
        _ => {
            if col_count_from_width == u32::MAX { 1 } else { col_count_from_width }
        }
    }.max(1);

    // 3. Column width
    let total_gaps = gap * (n_cols - 1) as f32;
    let col_w = ((content_w - total_gaps) / n_cols as f32).max(1.0);

    // 4. First-pass layout to get child heights (with span-all flag)
    let mut child_heights: Vec<(f32, bool)> = Vec::new(); // (height, is_span_all)
    for child in node.children.iter_mut() {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) { continue; }
        let h = engine.layout_box(child, col_w, content_x, content_y, font_px, root_font_px);
        child_heights.push((h, child.style.column_span_all));
    }

    // 5. Distribute children into columns
    let balance = node.style.column_fill; // true = balance
    // Exclude column-span:all children from balance total (they don't occupy a column)
    let total_content_h: f32 = child_heights.iter().filter(|(_, span)| !span).map(|(h, _)| h).sum();
    let target_col_h = if balance && n_cols > 1 {
        (total_content_h / n_cols as f32).max(1.0)
    } else {
        f32::MAX
    };

    let mut col_idx = 0usize;
    let mut col_cursor: Vec<f32> = vec![0.0; n_cols as usize];
    let mut in_flow_idx = 0usize;
    // Tracks the y-offset added by column-span:all elements
    let mut span_all_y_offset = 0.0f32;

    for i in 0..node.children.len() {
        if matches!(node.children[i].style.display, Display::None) { continue; }
        if matches!(node.children[i].style.position, Position::Absolute | Position::Fixed) { continue; }

        let (child_h, _) = child_heights[in_flow_idx];
        in_flow_idx += 1;

        // column-span: all — lay out across full width, then resume all columns below it
        if node.children[i].style.column_span_all {
            let max_col_y = col_cursor.iter().cloned().fold(0.0f32, f32::max);
            let span_y = content_y + span_all_y_offset + max_col_y;
            // Re-layout at full content_w to get correct height (first pass used col_w)
            let actual_span_h = engine.layout_box(
                &mut node.children[i], content_w, content_x, span_y, font_px, root_font_px
            );
            span_all_y_offset += max_col_y + actual_span_h;
            col_cursor = vec![0.0; n_cols as usize];
            col_idx = 0;
            continue;
        }

        if col_idx + 1 < n_cols as usize
            && balance
            && col_cursor[col_idx] + child_h > target_col_h * 1.1
        {
            col_idx += 1;
        }
        if col_idx >= n_cols as usize { col_idx = n_cols as usize - 1; }

        let col_x = content_x + col_idx as f32 * (col_w + gap);
        let col_y = content_y + span_all_y_offset + col_cursor[col_idx];

        // Re-layout child at its column position
        engine.layout_box(
            &mut node.children[i], col_w, col_x, col_y, font_px, root_font_px
        );

        col_cursor[col_idx] += child_h;

        if col_cursor[col_idx] >= target_col_h && col_idx + 1 < n_cols as usize {
            col_idx += 1;
        }
    }

    let max_col_y = col_cursor.iter().cloned().fold(0.0f32, f32::max);
    span_all_y_offset + max_col_y
}

