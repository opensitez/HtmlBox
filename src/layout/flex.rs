use crate::types::*;
use crate::layout::{LayoutEngine, ResolvedBox, layout_positioned, shift_rects};

/// Flexbox layout (CSS Flexible Box).
/// Faithful port of C++ LayoutFlex.
pub fn layout_flex(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    rbox:         &ResolvedBox,
    containing_w: f32,
    x:            f32,
    y:            f32,
    font_px:      f32,
    root_font_px: f32,
) -> f32 {
    let mut content_w = match rbox.content_width {
        Some(w) => w,
        None    => (containing_w - rbox.h_space()).max(0.0),
    };
    let shrink_to_fit = node.style.display == Display::InlineFlex && rbox.content_width.is_none();
    let content_x = x + rbox.margin_left + rbox.border_left + rbox.padding_left;
    let content_y = y + rbox.margin_top  + rbox.border_top  + rbox.padding_top;

    let is_row = matches!(node.style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse);
    let is_reversed = matches!(node.style.flex_direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse);
    let can_wrap   = node.style.flex_wrap != FlexWrap::Nowrap;
    let wrap_reverse = node.style.flex_wrap == FlexWrap::WrapReverse;

    // Main axis size of container
    let mut main_size: f32 = if is_row {
        content_w
    } else if let Some(ch) = rbox.content_height {
        ch
    } else {
        0.0 // column with auto height: determined after items
    };

    let gap_main  = if is_row {
        node.style.column_gap.resolve_vp(font_px, content_w, root_font_px, engine.viewport_w, engine.viewport_h)
    } else {
        node.style.row_gap.resolve_vp(font_px, content_w, root_font_px, engine.viewport_w, engine.viewport_h)
    };
    let gap_cross = if is_row {
        node.style.row_gap.resolve_vp(font_px, content_w, root_font_px, engine.viewport_w, engine.viewport_h)
    } else {
        node.style.column_gap.resolve_vp(font_px, content_w, root_font_px, engine.viewport_w, engine.viewport_h)
    };

    // ── Collect flex items ────────────────────────────────────────────────────

    struct FlexItem {
        idx:         usize,
        order:       i32,
        flex_grow:   f32,
        flex_shrink: f32,
        /// Hypothetical main size (content-box, after min/max clamp)
        hyp:         f32,
        /// Padding + border + margin on main axis
        outer_extra: f32,
        /// Final content-box main size after grow/shrink
        main_used:   f32,
        /// Final margin-box cross size
        cross_size:  f32,
        /// Final main position (relative to content origin)
        main_pos:    f32,
        /// Final cross position (relative to content origin)
        cross_pos:   f32,
        /// Saved CSS width/height before flex mutation (restored after positioning)
        saved_width:  CssLength,
        saved_height: CssLength,
    }

    let mut items: Vec<FlexItem> = Vec::new();

    for (idx, child) in node.children.iter_mut().enumerate() {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) { continue; }
        // CSS Flexbox §4.1: whitespace-only anonymous flex items are not rendered
        if child.tag == "#text" && child.text.chars().all(|c| c.is_ascii_whitespace()) { continue; }

        let child_font = child.style.font_size_px(font_px, root_font_px);
        let irb = engine.res_box(&child.style, child_font, content_w, root_font_px);

        // Outer extra = padding + border + margin on main axis
        let outer_extra = if is_row {
            irb.padding_left + irb.padding_right + irb.border_left + irb.border_right
                + irb.margin_left + irb.margin_right
        } else {
            irb.padding_top + irb.padding_bottom + irb.border_top + irb.border_bottom
                + irb.margin_top + irb.margin_bottom
        };

        // Resolve flex-basis → basis_main (content-box size)
        let basis_main: f32 = if !child.style.flex_basis.is_auto() {
            let raw = child.style.flex_basis.resolve_vp(child_font, if is_row { content_w } else { 0.0 }, root_font_px, engine.viewport_w, engine.viewport_h);
            if child.style.box_sizing == BoxSizing::BorderBox {
                if is_row {
                    (raw - irb.border_left - irb.border_right - irb.padding_left - irb.padding_right).max(0.0)
                } else {
                    (raw - irb.border_top - irb.border_bottom - irb.padding_top - irb.padding_bottom).max(0.0)
                }
            } else {
                raw.max(0.0)
            }
        } else if is_row && !child.style.width.is_auto() {
            let raw = child.style.width.resolve_vp(child_font, content_w, root_font_px, engine.viewport_w, engine.viewport_h);
            if child.style.box_sizing == BoxSizing::BorderBox {
                (raw - irb.border_left - irb.border_right - irb.padding_left - irb.padding_right).max(0.0)
            } else {
                raw.max(0.0)
            }
        } else if !is_row && !child.style.height.is_auto() {
            let raw = child.style.height.resolve_vp(child_font, 0.0, root_font_px, engine.viewport_w, engine.viewport_h);
            if child.style.box_sizing == BoxSizing::BorderBox {
                (raw - irb.border_top - irb.border_bottom - irb.padding_top - irb.padding_bottom).max(0.0)
            } else {
                raw.max(0.0)
            }
        } else {
            // Content-based: lay out at full container width, then take the
            // max-content (shrink-to-fit) size on the main axis.
            // For row items with width:auto, using the stretched content_w would
            // make every item look as wide as the container, breaking flex-wrap.
            engine.layout_box(child, content_w, content_x, content_y, font_px, root_font_px);
            if is_row {
                if child.style.width.is_auto() {
                    crate::layout::block::compute_intrinsic_width(child)
                } else {
                    child.content_rect.w
                }
            } else {
                child.content_rect.h
            }
        };

        // Apply min/max constraints on main axis.
        // For border-box items, min/max refer to the border box; convert to content-box.
        let bb_main = if child.style.box_sizing == BoxSizing::BorderBox {
            if is_row {
                irb.padding_left + irb.padding_right + irb.border_left + irb.border_right
            } else {
                irb.padding_top + irb.padding_bottom + irb.border_top + irb.border_bottom
            }
        } else { 0.0 };
        let min_main: f32 = if is_row {
            if !child.style.min_width.is_auto() {
                let v = child.style.min_width.resolve_vp(child_font, content_w, root_font_px, engine.viewport_w, engine.viewport_h);
                (v - bb_main).max(0.0)
            } else { 0.0 }
        } else {
            if !child.style.min_height.is_auto() {
                let v = child.style.min_height.resolve_vp(child_font, 0.0, root_font_px, engine.viewport_w, engine.viewport_h);
                (v - bb_main).max(0.0)
            } else { 0.0 }
        };
        let max_main: f32 = if is_row {
            if !child.style.max_width.is_none() {
                let v = child.style.max_width.resolve_vp(child_font, content_w, root_font_px, engine.viewport_w, engine.viewport_h);
                (v - bb_main).max(0.0)
            } else { f32::MAX }
        } else {
            if !child.style.max_height.is_none() {
                let v = child.style.max_height.resolve_vp(child_font, 0.0, root_font_px, engine.viewport_w, engine.viewport_h);
                (v - bb_main).max(0.0)
            } else { f32::MAX }
        };
        let hyp = basis_main.max(min_main).min(max_main);

        items.push(FlexItem {
            idx,
            order: child.style.order,
            flex_grow:   child.style.flex_grow,
            flex_shrink: child.style.flex_shrink,
            hyp,
            outer_extra,
            main_used: hyp,
            cross_size: 0.0,
            main_pos:   0.0,
            cross_pos:  0.0,
            saved_width:  child.style.width.clone(),
            saved_height: child.style.height.clone(),
        });
    }

    // Sort by order (stable)
    items.sort_by(|a, b| a.order.cmp(&b.order));

    // Shrink-to-fit for inline-flex with auto width: content_w = sum of item intrinsic sizes
    if shrink_to_fit && is_row && !items.is_empty() {
        let total: f32 = items.iter().map(|i| i.hyp + i.outer_extra).sum::<f32>()
            + gap_main * items.len().saturating_sub(1) as f32;
        let max_w = (containing_w - rbox.h_space()).max(0.0);
        content_w = total.min(max_w);
        main_size = content_w;
    }

    if items.is_empty() {
        let ch = if let Some(h) = rbox.content_height {
            h
        } else if let Some(ratio) = node.style.aspect_ratio {
            if ratio > 0.0 { (content_w / ratio).max(0.0) } else { 0.0 }
        } else {
            0.0
        };
        finish_flex(node, rbox, content_x, content_y, content_w, ch);
        node.collapsed_margin_top    = rbox.margin_top;
        node.collapsed_margin_bottom = rbox.margin_bottom;
        layout_abs_children(engine, node, font_px, root_font_px);
        return node.margin_rect.h;
    }

    // ── Wrap into flex lines ──────────────────────────────────────────────────

    struct FlexLine {
        start: usize, // index into items
        count: usize,
        main_used:    f32,
        cross_size:   f32,
        cross_offset: f32,
    }

    let mut lines: Vec<FlexLine> = Vec::new();
    {
        let mut i = 0;
        while i < items.len() {
            let mut line = FlexLine { start: i, count: 0, main_used: 0.0, cross_size: 0.0, cross_offset: 0.0 };
            let mut count = 0usize;
            while i < items.len() {
                let item_outer = items[i].hyp + items[i].outer_extra;
                let total_with_gap = line.main_used + (if count > 0 { gap_main } else { 0.0 }) + item_outer;
                if can_wrap && count > 0 && total_with_gap > main_size {
                    break;
                }
                line.main_used += (if count > 0 { gap_main } else { 0.0 }) + item_outer;
                count += 1;
                i += 1;
            }
            line.count = count;
            lines.push(line);
        }
    }

    // For column flex with auto height, main size = sum of items
    let effective_main_size = if !is_row && rbox.content_height.is_none() {
        let mut ms = 0.0f32;
        for line in &lines {
            ms = ms.max(line.main_used);
        }
        ms
    } else {
        main_size
    };

    // ── Resolve flexible lengths per line ─────────────────────────────────────

    for line in &lines {
        let free_space = effective_main_size - line.main_used;
        if free_space > 0.0 {
            let total_grow: f32 = items[line.start..line.start + line.count].iter().map(|i| i.flex_grow).sum();
            if total_grow > 0.0 {
                let mut distributed = 0.0f32;
                for j in 0..line.count {
                    let idx = line.start + j;
                    if items[idx].flex_grow > 0.0 {
                        let extra = if j == line.count - 1 {
                            free_space - distributed
                        } else {
                            free_space * items[idx].flex_grow / total_grow
                        };
                        distributed += extra;
                        items[idx].main_used = items[idx].hyp + extra;
                    }
                }
            }
        } else if free_space < 0.0 {
            let total_shrink: f32 = items[line.start..line.start + line.count].iter()
                .map(|i| i.flex_shrink * i.hyp).sum();
            if total_shrink > 0.0 {
                let deficit = -free_space;
                let mut distributed = 0.0f32;
                for j in 0..line.count {
                    let idx = line.start + j;
                    if items[idx].flex_shrink > 0.0 {
                        let shrink = if j == line.count - 1 {
                            deficit - distributed
                        } else {
                            deficit * (items[idx].flex_shrink * items[idx].hyp) / total_shrink
                        };
                        distributed += shrink;
                        items[idx].main_used = (items[idx].hyp - shrink).max(0.0);
                    }
                }
            }
        }
    }

    // ── Layout each item at its resolved main size, compute cross sizes ────────
    // Mirror C++: set item.box->style.width/height = {cssW/cssH, Px} before LayoutBox

    for item in &mut items {
        let child = &mut node.children[item.idx];
        let child_font = child.style.font_size_px(font_px, root_font_px);
        let irb = engine.res_box(&child.style, child_font, content_w, root_font_px);

        // Set CSS dimension to the resolved content-box main size
        if is_row {
            let css_w = if child.style.box_sizing == BoxSizing::BorderBox {
                item.main_used + irb.padding_left + irb.padding_right + irb.border_left + irb.border_right
            } else {
                item.main_used
            };
            child.style.width = CssLength::Px(css_w);
        } else {
            let css_h = if child.style.box_sizing == BoxSizing::BorderBox {
                item.main_used + irb.padding_top + irb.padding_bottom + irb.border_top + irb.border_bottom
            } else {
                item.main_used
            };
            child.style.height = CssLength::Px(css_h);
        }

        let item_containing = if is_row {
            item.main_used + item.outer_extra // = margin-box width
        } else {
            content_w
        };

        engine.layout_box(child, item_containing, content_x, content_y, font_px, root_font_px);

        item.cross_size = if is_row { child.margin_rect.h } else { child.margin_rect.w };
    }

    // ── Compute line cross sizes ──────────────────────────────────────────────

    for line in &mut lines {
        for j in 0..line.count {
            let cs = items[line.start + j].cross_size;
            if cs > line.cross_size { line.cross_size = cs; }
        }
    }

    // ── Total cross size and align-content ────────────────────────────────────

    let total_cross: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
        + gap_cross * (lines.len().saturating_sub(1)) as f32;

    let container_cross = if is_row {
        if let Some(h) = rbox.content_height { h }
        else if let Some(ratio) = node.style.aspect_ratio {
            if ratio > 0.0 { (content_w / ratio).max(total_cross) } else { total_cross }
        } else { total_cross }
    } else {
        content_w
    };

    let free_cross = (container_cross - total_cross).max(0.0);
    let (align_content_offset, extra_cross_gap) = if free_cross > 0.0 && !lines.is_empty() {
        match node.style.align_content {
            AlignContent::Center       => (free_cross / 2.0, 0.0),
            AlignContent::FlexEnd      => (free_cross, 0.0),
            AlignContent::SpaceBetween => {
                if lines.len() > 1 { (0.0, free_cross / (lines.len() - 1) as f32) }
                else               { (0.0, 0.0) }
            }
            AlignContent::SpaceAround  => {
                let g = free_cross / lines.len() as f32;
                (g / 2.0, g)
            }
            AlignContent::SpaceEvenly  => {
                let g = free_cross / (lines.len() + 1) as f32;
                (g, g)
            }
            AlignContent::Stretch      => {
                let extra = free_cross / lines.len() as f32;
                for line in &mut lines { line.cross_size += extra; }
                (0.0, 0.0)
            }
            _ => (0.0, 0.0),
        }
    } else {
        (0.0, 0.0)
    };

    // ── Position items ────────────────────────────────────────────────────────

    let mut cross_offset = align_content_offset;

    for li in 0..lines.len() {
        let line_idx = if wrap_reverse { lines.len() - 1 - li } else { li };
        lines[line_idx].cross_offset = cross_offset;

        // Main-axis: total used
        let total_items_main: f32 = items[lines[line_idx].start..lines[line_idx].start + lines[line_idx].count]
            .iter().map(|i| i.main_used + i.outer_extra).sum();
        let total_gaps = gap_main * lines[line_idx].count.saturating_sub(1) as f32;
        let free_main = effective_main_size - total_items_main - total_gaps;

        // Check for explicit auto margins on main axis
        let has_main_auto = items[lines[line_idx].start..lines[line_idx].start + lines[line_idx].count]
            .iter().any(|item| {
                let child = &node.children[item.idx];
                if is_row {
                    child.style.margin_left.is_auto() || child.style.margin_right.is_auto()
                } else {
                    child.style.margin_top.is_auto() || child.style.margin_bottom.is_auto()
                }
            });

        let (main_start, main_extra_gap) = if has_main_auto {
            // Auto margins on main axis absorb all free space,
            // overriding justify-content. Distribute evenly.
            let mut auto_count = 0usize;
            for j in 0..lines[line_idx].count {
                let ci = &node.children[items[lines[line_idx].start + j].idx];
                if is_row {
                    if ci.style.margin_left.is_auto() { auto_count += 1; }
                    if ci.style.margin_right.is_auto() { auto_count += 1; }
                } else {
                    if ci.style.margin_top.is_auto() { auto_count += 1; }
                    if ci.style.margin_bottom.is_auto() { auto_count += 1; }
                }
            }
            let auto_margin_size = if auto_count > 0 && free_main > 0.0 {
                free_main / auto_count as f32
            } else { 0.0 };

            let mut auto_pos = 0.0f32;
            for ii in 0..lines[line_idx].count {
                let idx = if is_reversed {
                    lines[line_idx].start + lines[line_idx].count - 1 - ii
                } else {
                    lines[line_idx].start + ii
                };
                let ci = &node.children[items[idx].idx];
                if is_row {
                    if ci.style.margin_left.is_auto() { auto_pos += auto_margin_size; }
                } else {
                    if ci.style.margin_top.is_auto() { auto_pos += auto_margin_size; }
                }
                items[idx].main_pos = auto_pos;
                auto_pos += items[idx].main_used + items[idx].outer_extra;
                let ci = &node.children[items[idx].idx];
                if is_row {
                    if ci.style.margin_right.is_auto() { auto_pos += auto_margin_size; }
                } else {
                    if ci.style.margin_bottom.is_auto() { auto_pos += auto_margin_size; }
                }
                auto_pos += gap_main;
            }
            (0.0, gap_main) // signal that main_pos was already set
        } else {
            justify_spacing(node.style.justify_content, free_main, lines[line_idx].count, gap_main)
        };

        let mut main_pos = main_start;

        for ii in 0..lines[line_idx].count {
            let item_idx = if is_reversed {
                lines[line_idx].start + lines[line_idx].count - 1 - ii
            } else {
                lines[line_idx].start + ii
            };

            let lc = lines[line_idx].cross_size;
            let eff_align = effective_align_self(&node.children[items[item_idx].idx], node.style.align_items);

            // Cross-axis: check auto margins (overrides align-items/align-self)
            let cross_start_auto = if is_row {
                node.children[items[item_idx].idx].style.margin_top.is_auto()
            } else {
                node.children[items[item_idx].idx].style.margin_left.is_auto()
            };
            let cross_end_auto = if is_row {
                node.children[items[item_idx].idx].style.margin_bottom.is_auto()
            } else {
                node.children[items[item_idx].idx].style.margin_right.is_auto()
            };

            // Cross-axis alignment
            let cross_extra = items[item_idx].cross_size;
            let cross_pos = if cross_start_auto || cross_end_auto {
                // Auto margins on cross axis absorb extra space
                let extra = lc - cross_extra;
                if extra > 0.0 {
                    if cross_start_auto && cross_end_auto { extra / 2.0 }
                    else if cross_end_auto { 0.0 }
                    else { extra }
                } else { 0.0 }
            } else if eff_align == AlignItems::Stretch {
                // Stretch: re-layout with explicit cross-axis size, mirrors C++
                let child = &mut node.children[items[item_idx].idx];
                let child_font = child.style.font_size_px(font_px, root_font_px);
                let irb = engine.res_box(&child.style, child_font, content_w, root_font_px);
                let item_containing = if is_row { items[item_idx].main_used + items[item_idx].outer_extra } else { content_w };
                if is_row && child.style.height.is_auto() {
                    let cross_extra = irb.padding_top + irb.padding_bottom + irb.border_top + irb.border_bottom
                                    + irb.margin_top + irb.margin_bottom;
                    let stretch_h = (lc - cross_extra).max(0.0);
                    let css_h = if child.style.box_sizing == BoxSizing::BorderBox {
                        stretch_h + irb.padding_top + irb.padding_bottom + irb.border_top + irb.border_bottom
                    } else { stretch_h };
                    child.style.height = CssLength::Px(css_h);
                    engine.layout_box(child, item_containing, content_x, content_y, font_px, root_font_px);
                    items[item_idx].cross_size = child.margin_rect.h;
                } else if !is_row && child.style.width.is_auto() {
                    let cross_extra = irb.padding_left + irb.padding_right + irb.border_left + irb.border_right
                                    + irb.margin_left + irb.margin_right;
                    let stretch_w = (lc - cross_extra).max(0.0);
                    let css_w = if child.style.box_sizing == BoxSizing::BorderBox {
                        stretch_w + irb.padding_left + irb.padding_right + irb.border_left + irb.border_right
                    } else { stretch_w };
                    child.style.width = CssLength::Px(css_w);
                    engine.layout_box(child, css_w, content_x, content_y, font_px, root_font_px);
                    items[item_idx].cross_size = child.margin_rect.w;
                }
                0.0
            } else {
                match eff_align {
                    AlignItems::FlexEnd => lc - cross_extra,
                    AlignItems::Center  => (lc - cross_extra) / 2.0,
                    _ => 0.0,
                }
            };

            if !has_main_auto {
                items[item_idx].main_pos  = main_pos;
            }
            items[item_idx].cross_pos = cross_offset + cross_pos;

            if !has_main_auto {
                main_pos += items[item_idx].main_used + items[item_idx].outer_extra + main_extra_gap;
            }
        }

        cross_offset += lines[line_idx].cross_size + gap_cross + extra_cross_gap;
    }

    // ── Set final child positions ─────────────────────────────────────────────

    for item in &items {
        let child = &mut node.children[item.idx];
        let (target_x, target_y) = if is_row {
            (content_x + item.main_pos, content_y + item.cross_pos)
        } else {
            (content_x + item.cross_pos, content_y + item.main_pos)
        };
        // Shift so that margin_rect origin is at (target_x, target_y)
        let dx = target_x - child.margin_rect.x;
        let dy = target_y - child.margin_rect.y;
        shift_rects(child, dx, dy);

        // Apply relative offset if position:relative
        if matches!(child.style.position, Position::Relative | Position::Sticky) {
            let child_font = child.style.font_size_px(font_px, root_font_px);
            crate::layout::block::apply_relative_offset(child, child_font, content_w, root_font_px);
        }
    }

    // ── Restore original CSS dimensions so re-layout works correctly ──────────
    // Mirrors C++: item.box->style.width = item.savedWidth; etc.

    for item in &items {
        let sw = item.saved_width.clone();
        let sh = item.saved_height.clone();
        node.children[item.idx].style.width  = sw;
        node.children[item.idx].style.height = sh;
    }

    // ── Content height ────────────────────────────────────────────────────────

    let content_h = if let Some(h) = rbox.content_height {
        h
    } else if let Some(ratio) = node.style.aspect_ratio {
        // Derive height from width via aspect-ratio when no explicit height is set
        if ratio > 0.0 { (content_w / ratio).max(0.0) } else { 0.0 }
    } else if is_row {
        // Cross axis = height, which is cross_offset minus trailing gap
        let used = cross_offset - if lines.is_empty() { 0.0 } else { gap_cross };
        used.max(0.0)
    } else {
        // Column: main axis is vertical
        let mut max_main = 0.0f32;
        for item in &items {
            let end = item.main_pos + item.main_used + item.outer_extra;
            if end > max_main { max_main = end; }
        }
        max_main.max(0.0)
    };

    finish_flex(node, rbox, content_x, content_y, content_w, content_h);

    node.collapsed_margin_top    = rbox.margin_top;
    node.collapsed_margin_bottom = rbox.margin_bottom;

    layout_abs_children(engine, node, font_px, root_font_px);
    node.margin_rect.h
}

// ─── justify-content spacing ─────────────────────────────────────────────────

fn justify_spacing(jc: JustifyContent, free: f32, n: usize, base_gap: f32) -> (f32, f32) {
    if n == 0 { return (0.0, base_gap); }
    match jc {
        JustifyContent::FlexStart   => (0.0,        base_gap),
        JustifyContent::FlexEnd     => (free,        base_gap),
        JustifyContent::Center      => (free / 2.0,  base_gap),
        JustifyContent::SpaceBetween => {
            if n > 1 { (0.0, base_gap + free / (n - 1) as f32) }
            else     { (0.0, base_gap) }
        }
        JustifyContent::SpaceAround => {
            let s = free / n as f32;
            (s / 2.0, base_gap + s)
        }
        JustifyContent::SpaceEvenly => {
            let s = free / (n + 1) as f32;
            (s, base_gap + s)
        }
    }
}

// ─── effective align-self ─────────────────────────────────────────────────────

fn effective_align_self(child: &HtmlBox, parent_align: AlignItems) -> AlignItems {
    match child.style.align_self {
        AlignSelf::Auto      => parent_align,
        AlignSelf::Stretch   => AlignItems::Stretch,
        AlignSelf::FlexStart => AlignItems::FlexStart,
        AlignSelf::FlexEnd   => AlignItems::FlexEnd,
        AlignSelf::Center    => AlignItems::Center,
        AlignSelf::Baseline  => AlignItems::Baseline,
    }
}

// ─── finish: set box rects ───────────────────────────────────────────────────

fn finish_flex(
    node: &mut HtmlBox,
    rbox: &ResolvedBox,
    content_x: f32, content_y: f32,
    content_w: f32, content_h: f32,
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
        node.border_rect.x - rbox.margin_left,
        node.border_rect.y - rbox.margin_top,
        node.border_rect.w + rbox.margin_left + rbox.margin_right,
        node.border_rect.h + rbox.margin_top  + rbox.margin_bottom,
    );
    node.baseline = content_y + content_h;

    node.resolved_margin_top    = rbox.margin_top;
    node.resolved_margin_right  = rbox.margin_right;
    node.resolved_margin_bottom = rbox.margin_bottom;
    node.resolved_margin_left   = rbox.margin_left;
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

fn layout_abs_children(engine: &LayoutEngine, node: &mut HtmlBox, font_px: f32, root_font_px: f32) {
    let containing_rect = node.content_rect;
    let indices: Vec<usize> = node.children.iter().enumerate()
        .filter(|(_, c)| matches!(c.style.position, Position::Absolute | Position::Fixed))
        .map(|(i, _)| i)
        .collect();
    for i in indices {
        layout_positioned(engine, &mut node.children[i], containing_rect, font_px, root_font_px);
    }
}
