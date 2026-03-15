use crate::types::*;
use crate::layout::{LayoutEngine, ResolvedBox, layout_positioned, shift_rects};
use crate::layout::block::apply_relative_offset;
#[allow(unused_imports)]
use crate::css::parse_single_track;

/// CSS Grid layout.
/// Returns total outer height.
/// Mirrors C++ LayoutGrid.
pub fn layout_grid(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    rbox:         &ResolvedBox,
    containing_w: f32,
    x:            f32,
    y:            f32,
    font_px:      f32,
    root_font_px: f32,
) -> f32 {
    let content_w = match rbox.content_width {
        Some(w) => w,
        None    => (containing_w - rbox.h_space()).max(0.0),
    };
    let content_x = x + rbox.margin_left + rbox.border_left + rbox.padding_left;
    let content_y = y + rbox.margin_top  + rbox.border_top  + rbox.padding_top;

    let col_gap = engine.res_len(&node.style.column_gap, font_px, content_w, root_font_px);
    let row_gap = engine.res_len(&node.style.row_gap, font_px, content_w, root_font_px);

    // Resolve column track sizes
    let col_tracks = resolve_track_sizes(&node.style.grid_template_columns,
        &node.style.auto_repeat_columns, content_w, font_px, root_font_px);
    let row_tracks = node.style.grid_template_rows.clone();

    // Collect visible items (non-abs-positioned)
    // CSS Grid §4: whitespace-only anonymous grid items are not rendered
    let mut item_indices: Vec<usize> = node.children.iter().enumerate()
        .filter(|(_, c)| !matches!(c.style.display, Display::None)
            && !matches!(c.style.position, Position::Absolute | Position::Fixed)
            && !(c.tag == "#text" && c.text.chars().all(|ch| ch.is_ascii_whitespace())))
        .map(|(i, _)| i)
        .collect();
    // Sort by order property (CSS order: default 0)
    item_indices.sort_by(|&a, &b| node.children[a].style.order.cmp(&node.children[b].style.order));

    let n_items = item_indices.len();
    if n_items == 0 {
        let ch = rbox.content_height.unwrap_or(0.0);
        let result = finish_grid(node, rbox, content_x, content_y, content_w, ch);
        layout_abs_children(engine, node, font_px, root_font_px);
        return result;
    }

    // ── Grid placement algorithm ─────────────────────────────────────────────

    // Named area lookup
    let area_map = build_area_map(&node.style.grid_template_areas);

    let n_explicit_cols = col_tracks.len();

    // Pre-compute area dimensions from grid-template-areas
    let area_cols = if !node.style.grid_template_areas.is_empty() {
        node.style.grid_template_areas[0].len()
    } else { 0 };
    let area_rows = node.style.grid_template_areas.len();

    // Placement result: (col_start, col_end, row_start, row_end) — all 0-based
    let mut placements: Vec<(usize, usize, usize, usize)> = vec![(0, 1, 0, 1); n_items];

    // Pass 1: Place items with explicit positions
    let mut max_row = if area_rows > 0 { area_rows } else { 0 };
    let mut max_col = if n_explicit_cols > 0 { n_explicit_cols } else if area_cols > 0 { area_cols } else { 1 };

    for (ii, &idx) in item_indices.iter().enumerate() {
        let child = &node.children[idx];
        let (cs, ce, rs, re) = resolve_placement(child, &area_map, n_explicit_cols);
        placements[ii] = (cs, ce, rs, re);
        if ce > max_col { max_col = ce; }
        if re > max_row { max_row = re; }
    }

    // Pass 2: Auto-place remaining items
    // Occupation grid: row → set of occupied columns
    let col_flow = matches!(node.style.grid_auto_flow, GridAutoFlow::Column | GridAutoFlow::ColumnDense);
    let dense    = matches!(node.style.grid_auto_flow, GridAutoFlow::RowDense | GridAutoFlow::ColumnDense);

    let mut occ: Vec<Vec<bool>> = vec![vec![false; max_col.max(1)]; (max_row + n_items + 1).max(1)];

    // Mark explicitly placed items
    for (ii, &idx) in item_indices.iter().enumerate() {
        let child = &node.children[idx];
        if is_explicitly_placed(child, &area_map) {
            let (cs, ce, rs, re) = placements[ii];
            for r in rs..re {
                ensure_row(&mut occ, r, max_col);
                for c in cs..ce {
                    if c < occ[r].len() { occ[r][c] = true; }
                }
            }
        }
    }

    // Auto-place items without explicit position
    let mut auto_row = 0usize;
    let mut auto_col = 0usize;

    for (ii, &idx) in item_indices.iter().enumerate() {
        let child = &node.children[idx];
        if is_explicitly_placed(child, &area_map) { continue; }

        let span_col = get_span_col(child);
        let span_row = get_span_row(child);

        if dense { auto_row = 0; auto_col = 0; }

        if col_flow {
            // Column-flow: fill down each column, then move to next column
            // Number of rows per column = explicit row track count (or unlimited if none)
            let col_row_limit = n_rows_from_template(&row_tracks, n_items);
            // auto_col = current column, auto_row = current row within column
            'outer_col: loop {
                ensure_row(&mut occ, auto_row + span_row, max_col);
                if auto_row + span_row > col_row_limit {
                    // Move to next column
                    auto_col += 1;
                    auto_row  = 0;
                    if auto_col >= max_col { max_col += 1; ensure_row(&mut occ, 1, max_col); }
                    ensure_row(&mut occ, auto_row + span_row, max_col);
                    continue;
                }
                let mut fits = true;
                'check_col: for r in auto_row..auto_row + span_row {
                    ensure_row(&mut occ, r, max_col);
                    for c in auto_col..auto_col + span_col {
                        if c < occ[r].len() && occ[r][c] { fits = false; break 'check_col; }
                    }
                }
                if fits { break 'outer_col; }
                auto_row += 1;
            }
        } else {
            // Row-flow: fill across each row, then move to next row
            'outer: loop {
                ensure_row(&mut occ, auto_row + span_row, max_col);
                if auto_col + span_col > max_col {
                    auto_row += 1;
                    auto_col  = 0;
                    ensure_row(&mut occ, auto_row + span_row, max_col);
                    continue;
                }
                let mut fits = true;
                'check: for r in auto_row..auto_row + span_row {
                    ensure_row(&mut occ, r, max_col);
                    for c in auto_col..auto_col + span_col {
                        if c < occ[r].len() && occ[r][c] { fits = false; break 'check; }
                    }
                }
                if fits { break 'outer; }
                auto_col += 1;
                if auto_col + span_col > max_col {
                    auto_row += 1;
                    auto_col  = 0;
                }
            }
        }

        placements[ii] = (auto_col, auto_col + span_col, auto_row, auto_row + span_row);

        // Mark occupied
        for r in auto_row..auto_row + span_row {
            ensure_row(&mut occ, r, max_col);
            for c in auto_col..auto_col + span_col {
                if c < occ[r].len() { occ[r][c] = true; }
            }
        }
        if re_from(&placements[ii]) > max_row { max_row = re_from(&placements[ii]); }

        if col_flow {
            auto_row += span_row;
        } else {
            auto_col += span_col;
        }
    }

    let n_rows = if max_row > 0 { max_row } else {
        // Auto rows based on child count
        ((n_items + max_col - 1) / max_col).max(1)
    };
    if n_rows == 0 {
        let ch = rbox.content_height.unwrap_or(0.0);
        let result = finish_grid(node, rbox, content_x, content_y, content_w, ch);
        layout_abs_children(engine, node, font_px, root_font_px);
        return result;
    }

    // ── Resolve column pixel widths ──────────────────────────────────────────

    // Measure items for Auto track sizing
    let mut col_content_widths = vec![0.0f32; n_explicit_cols.max(max_col)];
    for (ii, &idx) in item_indices.iter().enumerate() {
        let (cs, ce, _rs, _re) = placements[ii];
        let child = &mut node.children[idx];
        let _child_font = child.style.font_size_px(font_px, root_font_px);
        // Dry run layout to find intrinsic width
        engine.layout_box(child, 10000.0, 0.0, 0.0, font_px, root_font_px);
        let intrinsic_w = crate::layout::block::compute_intrinsic_width(child);
        let span = (ce - cs).max(1);
        let w_per_col = intrinsic_w / span as f32;
        for c in cs..ce {
            if c < col_content_widths.len() {
                if w_per_col > col_content_widths[c] { col_content_widths[c] = w_per_col; }
            }
        }
    }

    let col_px = resolve_to_pixels(&col_tracks, &node.style.grid_auto_columns,
                                   content_w, col_gap, n_explicit_cols.max(max_col),
                                   font_px, root_font_px, &col_content_widths);
    let n_cols_actual = col_px.len();

    // justify-content: compute extra horizontal space distribution
    let grid_total_w: f32 = col_px.iter().sum::<f32>()
        + col_gap * n_cols_actual.saturating_sub(1) as f32;
    let extra_x = (content_w - grid_total_w).max(0.0);
    let (grid_offset_x, extra_gap_col) =
        compute_justify_content(node.style.justify_content, extra_x, n_cols_actual);

    // Column X positions (including justify-content offset)
    let col_x: Vec<f32> = {
        let mut xs = Vec::with_capacity(n_cols_actual);
        let mut cx = grid_offset_x;
        for i in 0..n_cols_actual {
            xs.push(cx);
            cx += col_px[i] + col_gap + extra_gap_col;
        }
        xs
    };

    // ── First pass: layout all items to get row heights ──────────────────────

    let mut row_heights: Vec<f32> = vec![0.0; n_rows];

    for (ii, &idx) in item_indices.iter().enumerate() {
        let (cs, ce, rs, re) = placements[ii];
        let cs = cs.min(n_cols_actual.saturating_sub(1));
        let ce = ce.min(n_cols_actual).max(cs + 1);

        // Compute span width
        let span_w = span_width(&col_px, &col_x, cs, ce, col_gap, content_w);

        let child = &mut node.children[idx];
        let child_font = child.style.font_size_px(font_px, root_font_px);
        let crbox = engine.res_box(&child.style, child_font, span_w, root_font_px);
        engine.layout_box(child, span_w, content_x, 0.0, font_px, root_font_px);
        let h = child.border_rect.h + crbox.margin_top + crbox.margin_bottom;
        // Distribute height across spanned rows
        let row_span = (re - rs).max(1);
        let h_per_row = h / row_span as f32;
        for r in rs..re.min(n_rows) {
            if h_per_row > row_heights[r] { row_heights[r] = h_per_row; }
        }
    }

    // Apply explicit row track sizes where specified (with MinMax clamping)
    for (ri, track) in row_tracks.iter().enumerate() {
        if ri < row_heights.len() {
            match track.kind {
                GridTrackKind::MinMax => {
                    let min_h = track_to_px(
                        &GridTrackSize { kind: track.min_kind, value: track.min_value, ..Default::default() },
                        containing_w, font_px, root_font_px,
                    );
                    let max_h = track_to_px(
                        &GridTrackSize { kind: track.max_kind, value: track.max_value, ..Default::default() },
                        containing_w, font_px, root_font_px,
                    );
                    let mut h = row_heights[ri].max(min_h);
                    if max_h > 0.0 { h = h.min(max_h); }
                    h = h.max(min_h);
                    row_heights[ri] = h;
                }
                _ => {
                    let px = track_to_px(track, containing_w, font_px, root_font_px);
                    if px > 0.0 && px > row_heights[ri] { row_heights[ri] = px; }
                }
            }
        }
    }

    // Apply grid-auto-rows to implicit rows (rows beyond the explicit row count)
    {
        let n_explicit_rows = node.style.grid_template_rows.len();
        let auto_h = track_to_px(&node.style.grid_auto_rows, containing_w, font_px, root_font_px);
        if auto_h > 0.0 {
            for r in n_explicit_rows..n_rows {
                if auto_h > row_heights[r] { row_heights[r] = auto_h; }
            }
        }
    }

    // align-content: compute extra vertical space distribution
    let grid_total_h: f32 = row_heights.iter().sum::<f32>()
        + row_gap * n_rows.saturating_sub(1) as f32;
    let container_h = rbox.content_height.unwrap_or(grid_total_h);
    let extra_y = (container_h - grid_total_h).max(0.0);
    let (grid_offset_y, extra_gap_row) =
        compute_align_content(node.style.align_content, extra_y, n_rows);

    // Row Y positions (including align-content offset)
    let row_y: Vec<f32> = {
        let mut ys = Vec::with_capacity(n_rows);
        let mut cy = grid_offset_y;
        for i in 0..n_rows {
            ys.push(cy);
            cy += row_heights[i] + row_gap + extra_gap_row;
        }
        ys
    };

    // ── Second pass: position items ─────────────────────────────────────────

    for (ii, &idx) in item_indices.iter().enumerate() {
        let (cs, ce, rs, re) = placements[ii];
        let cs = cs.min(n_cols_actual.saturating_sub(1));
        let ce = ce.min(n_cols_actual).max(cs + 1);
        let rs = rs.min(n_rows.saturating_sub(1));
        let re = re.min(n_rows).max(rs + 1);

        let span_w = span_width(&col_px, &col_x, cs, ce, col_gap, content_w);
        let cell_h = row_heights[rs..re].iter().sum::<f32>()
            + row_gap * (re - rs).saturating_sub(1) as f32;

        let ix = content_x + col_x.get(cs).copied().unwrap_or(0.0);
        let iy = content_y + row_y.get(rs).copied().unwrap_or(0.0);

        let child = &mut node.children[idx];
        let child_font = child.style.font_size_px(font_px, root_font_px);
        let crbox = engine.res_box(&child.style, child_font, span_w, root_font_px);

        // Handle justify-self / align-self
        let eff_justify = effective_justify_self(child, node.style.justify_items);
        let eff_align   = effective_align_self_grid(child, node.style.align_items);

        // Stretch align-self: set explicit height and re-layout
        if eff_align == AlignItems::Stretch && child.style.height.is_auto() {
            // Compute the height value to assign to child.style.height.
            // resolve_box will later interpret this through box-sizing:
            //   border-box → subtracts padding+border from the assigned value
            //   content-box → uses the assigned value as-is
            // So for border-box we pass (cell_h - margins) and let resolve_box
            // subtract padding+border. For content-box we subtract them ourselves.
            let css_h = if child.style.box_sizing == BoxSizing::BorderBox {
                (cell_h - crbox.margin_top - crbox.margin_bottom).max(0.0)
            } else {
                (cell_h
                    - crbox.margin_top - crbox.margin_bottom
                    - crbox.padding_top - crbox.padding_bottom
                    - crbox.border_top  - crbox.border_bottom)
                    .max(0.0)
            };
            let saved_h = child.style.height.clone();
            child.style.height = CssLength::Px(css_h);
            engine.layout_box(child, span_w, ix, iy, font_px, root_font_px);
            child.style.height = saved_h;
        } else {
            engine.layout_box(child, span_w, ix, iy, font_px, root_font_px);
        }

        // Re-read crbox after potential re-layout
        let crbox = engine.res_box(&child.style, child_font, span_w, root_font_px);
        let cell_w = span_w;

        let dx_align = match eff_justify {
            AlignItems::FlexStart => 0.0,
            AlignItems::FlexEnd   => cell_w - child.border_rect.w - crbox.margin_left - crbox.margin_right,
            AlignItems::Center    => (cell_w - child.border_rect.w - crbox.margin_left - crbox.margin_right) / 2.0,
            AlignItems::Stretch   => 0.0,
            AlignItems::Baseline  => 0.0,
        };
        let dy_align = match eff_align {
            AlignItems::FlexStart => 0.0,
            AlignItems::FlexEnd   => cell_h - child.border_rect.h - crbox.margin_top - crbox.margin_bottom,
            AlignItems::Center    => (cell_h - child.border_rect.h - crbox.margin_top - crbox.margin_bottom) / 2.0,
            AlignItems::Stretch   => 0.0,
            AlignItems::Baseline  => 0.0,
        };

        // Target border_rect position
        let target_x = ix + crbox.margin_left + dx_align;
        let target_y = iy + crbox.margin_top  + dy_align;

        let dx = target_x - child.border_rect.x;
        let dy = target_y - child.border_rect.y;
        shift_rects(child, dx, dy);

        // Apply relative offset
        if child.style.position == Position::Relative {
            apply_relative_offset(child, child_font, content_w, root_font_px);
        }
    }

    let total_h = row_y.last().copied().unwrap_or(0.0)
        + row_heights.last().copied().unwrap_or(0.0);

    let ch = rbox.content_height.unwrap_or(total_h);

    // Save restored heights
    for &idx in &item_indices {
        node.children[idx].scroll_height = node.children[idx].content_rect.h;
    }

    // Collapsed margins: no pass-through
    node.collapsed_margin_top    = rbox.margin_top;
    node.collapsed_margin_bottom = rbox.margin_bottom;

    let result = finish_grid(node, rbox, content_x, content_y, content_w, ch);
    layout_abs_children(engine, node, font_px, root_font_px);
    result
}

// ─── Placement helpers ────────────────────────────────────────────────────────

fn build_area_map(areas: &[Vec<String>]) -> std::collections::HashMap<String, (usize, usize, usize, usize)> {
    let mut map = std::collections::HashMap::new();
    for (r, row) in areas.iter().enumerate() {
        for (c, name) in row.iter().enumerate() {
            if name == "." { continue; }
            let e = map.entry(name.clone()).or_insert((c, c+1, r, r+1));
            if c < e.0 { e.0 = c; }
            if c + 1 > e.1 { e.1 = c + 1; }
            if r < e.2 { e.2 = r; }
            if r + 1 > e.3 { e.3 = r + 1; }
        }
    }
    map
}

fn is_explicitly_placed(child: &HtmlBox, area_map: &std::collections::HashMap<String, (usize,usize,usize,usize)>) -> bool {
    if !child.style.grid_area.is_empty() && area_map.contains_key(&child.style.grid_area) {
        return true;
    }
    // Require BOTH axes to be specified (matching C++)
    child.style.grid_column_start != 0 && child.style.grid_row_start != 0
}

/// Resolve placement to (col_start, col_end, row_start, row_end), all 0-based.
fn resolve_placement(
    child: &HtmlBox,
    area_map: &std::collections::HashMap<String,(usize,usize,usize,usize)>,
    _n_cols: usize,
) -> (usize, usize, usize, usize) {
    // Named grid area
    if !child.style.grid_area.is_empty() {
        if let Some(&(cs, ce, rs, re)) = area_map.get(&child.style.grid_area) {
            return (cs, ce, rs, re);
        }
    }

    let cs_raw = child.style.grid_column_start;
    let ce_raw = child.style.grid_column_end;
    let rs_raw = child.style.grid_row_start;
    let re_raw = child.style.grid_row_end;

    // col start
    let cs = if cs_raw > 0 { (cs_raw as usize).saturating_sub(1) } else { 0 };
    // col end
    let ce = if ce_raw > 0 {
        (ce_raw as usize).saturating_sub(1).max(cs + 1)
    } else if ce_raw < 0 {
        cs + (-ce_raw) as usize  // span
    } else {
        cs + 1
    };
    // row start
    let rs = if rs_raw > 0 { (rs_raw as usize).saturating_sub(1) } else { 0 };
    // row end
    let re = if re_raw > 0 {
        (re_raw as usize).saturating_sub(1).max(rs + 1)
    } else if re_raw < 0 {
        rs + (-re_raw) as usize
    } else {
        rs + 1
    };

    (cs, ce, rs, re)
}

fn get_span_col(child: &HtmlBox) -> usize {
    if child.style.grid_column_end < 0 { (-child.style.grid_column_end) as usize }
    else if child.style.grid_column_start < 0 { (-child.style.grid_column_start) as usize }
    else if child.style.grid_column_end > 0 && child.style.grid_column_start > 0 {
        (child.style.grid_column_end - child.style.grid_column_start).max(1) as usize
    } else { 1 }
}

fn get_span_row(child: &HtmlBox) -> usize {
    if child.style.grid_row_end < 0 { (-child.style.grid_row_end) as usize }
    else if child.style.grid_row_start < 0 { (-child.style.grid_row_start) as usize }
    else if child.style.grid_row_end > 0 && child.style.grid_row_start > 0 {
        (child.style.grid_row_end - child.style.grid_row_start).max(1) as usize
    } else { 1 }
}

fn re_from(p: &(usize, usize, usize, usize)) -> usize { p.3 }

fn n_rows_from_template(row_tracks: &[GridTrackSize], n_items: usize) -> usize {
    if !row_tracks.is_empty() { row_tracks.len() } else { n_items.max(1) }
}

fn ensure_row(occ: &mut Vec<Vec<bool>>, row: usize, n_cols: usize) {
    while occ.len() <= row {
        occ.push(vec![false; n_cols.max(1)]);
    }
    // Extend existing rows if n_cols grew
    for r in occ.iter_mut() {
        while r.len() < n_cols.max(1) { r.push(false); }
    }
}

// ─── Track resolution ────────────────────────────────────────────────────────

/// Resolve auto-repeat and other tracks to a final list.
fn resolve_track_sizes(
    tracks: &[GridTrackSize],
    auto_repeat: &[GridTrackSize],
    container: f32,
    font_px: f32,
    root_font_px: f32,
) -> Vec<GridTrackSize> {
    let mut result = Vec::new();
    for t in tracks {
        if t.kind == GridTrackKind::Auto && t.value == -1.0 {
            // Auto-fill/fit placeholder — expand using auto_repeat_columns
            if !auto_repeat.is_empty() {
                // Estimate pattern width, accounting for all tracks in the pattern
                let mut total_fixed = 0.0f32;
                for rt in auto_repeat {
                    let px = track_to_px(rt, container, font_px, root_font_px);
                    if px > 0.0 { total_fixed += px; } else { total_fixed += 50.0; }
                }
                let pattern_w = total_fixed.max(1.0);
                let _pat_size = auto_repeat.len() as f32;
                // Repeat count accounts for gaps between pattern tracks
                let avail = container;
                let count = (avail / (pattern_w)).max(1.0).min(100.0) as usize;
                for _ in 0..count {
                    for rt in auto_repeat {
                        result.push(rt.clone());
                    }
                }
            }
        } else {
            result.push(t.clone());
        }
    }
    if result.is_empty() && !auto_repeat.is_empty() {
        // Only auto-repeat
        let mut total_fixed = 0.0f32;
        for rt in auto_repeat {
            let px = track_to_px(rt, container, font_px, root_font_px);
            if px > 0.0 { total_fixed += px; } else { total_fixed += 50.0; }
        }
        let pattern_w = total_fixed.max(1.0);
        let count = (container / pattern_w).max(1.0).min(100.0) as usize;
        for _ in 0..count {
            for rt in auto_repeat {
                result.push(rt.clone());
            }
        }
    }
    result
}

/// Convert a single track to approximate pixel value for auto-fill estimation.
pub fn track_to_px(track: &GridTrackSize, container: f32, font_px: f32, root_font_px: f32) -> f32 {
    match track.kind {
        GridTrackKind::Fixed    => track.value,
        GridTrackKind::Percent  => track.value / 100.0 * container,
        GridTrackKind::Auto | GridTrackKind::MinContent | GridTrackKind::MaxContent => 0.0,
        GridTrackKind::Fractional => 0.0,  // resolved below
        GridTrackKind::MinMax => {
            // Use max value
            let max_t = GridTrackSize { kind: track.max_kind, value: track.max_value, ..Default::default() };
            track_to_px(&max_t, container, font_px, root_font_px)
        }
        GridTrackKind::FitContent => track.value,
    }
}

/// Resolve track list to pixel widths (distributing fr units).
/// Mirrors C++ column-width resolution in LayoutGrid.
fn resolve_to_pixels(
    tracks: &[GridTrackSize],
    auto_cols: &GridTrackSize,
    container: f32,
    gap: f32,
    n_cols: usize,
    font_px: f32,
    root_font_px: f32,
    content_widths: &[f32],
) -> Vec<f32> {
    let effective_n = tracks.len().max(n_cols);
    let mut sizes: Vec<f32> = vec![0.0; effective_n];
    let mut fr_indices: Vec<usize> = Vec::new();
    let mut fr_values:  Vec<f32>   = Vec::new();
    let mut used = 0.0f32;
    let mut flexible_cols = 0usize;
    let total_gap = gap * effective_n.saturating_sub(1) as f32;

    // First pass: resolve fixed/percent/minmax-min sizes; tally fr and flexible
    for i in 0..effective_n {
        let track = if i < tracks.len() {
            Some(&tracks[i])
        } else if auto_cols.kind != GridTrackKind::Auto {
            Some(auto_cols)
        } else {
            None
        };

        let track = match track {
            Some(t) => t,
            None => { flexible_cols += 1; continue; }
        };

        match track.kind {
            GridTrackKind::Fixed => {
                sizes[i] = track.value;
                used += track.value;
            }
            GridTrackKind::Percent => {
                let px = track.value / 100.0 * container;
                sizes[i] = px;
                used += px;
            }
            GridTrackKind::Fractional => {
                fr_indices.push(i);
                fr_values.push(track.value);
                flexible_cols += 1;
            }
            GridTrackKind::MinMax => {
                let min_px = track_to_px(
                    &GridTrackSize { kind: track.min_kind, value: track.min_value, ..Default::default() },
                    container, font_px, root_font_px,
                );
                sizes[i] = min_px;
                used += min_px;
                if track.max_kind == GridTrackKind::Fractional {
                    fr_indices.push(i);
                    fr_values.push(track.max_value);
                }
                // Non-fr max handled in second pass
            }
            GridTrackKind::Auto | GridTrackKind::MinContent | GridTrackKind::MaxContent => {
                let cw = content_widths.get(i).copied().unwrap_or(0.0);
                sizes[i] = cw;
                used += cw;
                flexible_cols += 1;
            }
            GridTrackKind::FitContent => {
                let cw = content_widths.get(i).copied().unwrap_or(0.0);
                sizes[i] = cw;
                used += cw;
                flexible_cols += 1;
            }
        }
    }

    let free_space = (container - used - total_gap).max(0.0);
    let total_fr: f32 = fr_values.iter().sum();

    // Auto share: when no fr tracks, flexible cols share free space equally
    let auto_share = if flexible_cols > 0 && total_fr <= 0.0 {
        free_space / flexible_cols as f32
    } else {
        0.0
    };

    // Second pass: resolve fr, auto, content-based, and minmax max values
    for i in 0..effective_n {
        let track = if i < tracks.len() {
            Some(&tracks[i])
        } else if auto_cols.kind != GridTrackKind::Auto {
            Some(auto_cols)
        } else {
            None
        };

        let track = match track {
            Some(t) => t,
            None => {
                // Implicit auto column
                sizes[i] = if total_fr > 0.0 { 0.0 } else { auto_share };
                continue;
            }
        };

        match track.kind {
            GridTrackKind::Fractional => {
                if total_fr > 0.0 {
                    sizes[i] = free_space * track.value / total_fr;
                }
            }
            GridTrackKind::MinMax => {
                let min_w = sizes[i]; // already set in first pass
                if track.max_kind == GridTrackKind::Fractional && total_fr > 0.0 {
                    let fr_w = free_space * track.max_value / total_fr;
                    sizes[i] = min_w.max(fr_w);
                } else if track.max_kind == GridTrackKind::Fixed {
                    sizes[i] = min_w.max(track.max_value);
                } else if track.max_kind == GridTrackKind::Percent {
                    let max_w = track.max_value / 100.0 * container;
                    sizes[i] = min_w.max(max_w);
                } else {
                    // auto/min-content/max-content max: use free space share
                    sizes[i] = min_w.max(if total_fr > 0.0 { 0.0 } else { auto_share });
                }
            }
            GridTrackKind::Auto | GridTrackKind::MinContent | GridTrackKind::MaxContent => {
                sizes[i] += if total_fr > 0.0 { 0.0 } else { auto_share };
            }
            GridTrackKind::FitContent => {
                sizes[i] += if total_fr > 0.0 { 0.0 } else { auto_share };
                // Clamp to fit-content limit
                if track.max_kind == GridTrackKind::Fixed {
                    sizes[i] = sizes[i].min(track.value);
                } else if track.max_kind == GridTrackKind::Percent {
                    sizes[i] = sizes[i].min(track.value / 100.0 * container);
                }
            }
            _ => {} // Fixed/Percent already handled
        }
        if sizes[i] < 0.0 { sizes[i] = 0.0; }
    }

    sizes
}

fn span_width(col_px: &[f32], _col_x: &[f32], cs: usize, ce: usize, col_gap: f32, fallback: f32) -> f32 {
    if col_px.is_empty() { return fallback; }
    let cs = cs.min(col_px.len().saturating_sub(1));
    let ce = ce.min(col_px.len());
    if cs >= ce { return col_px.get(cs).copied().unwrap_or(fallback); }
    let w: f32 = col_px[cs..ce].iter().sum::<f32>() + col_gap * (ce - cs).saturating_sub(1) as f32;
    w
}

// ─── Alignment helpers ────────────────────────────────────────────────────────

fn effective_justify_self(child: &HtmlBox, parent: AlignItems) -> AlignItems {
    match child.style.justify_self {
        AlignSelf::Auto      => parent,
        AlignSelf::Stretch   => AlignItems::Stretch,
        AlignSelf::FlexStart => AlignItems::FlexStart,
        AlignSelf::FlexEnd   => AlignItems::FlexEnd,
        AlignSelf::Center    => AlignItems::Center,
        AlignSelf::Baseline  => AlignItems::Baseline,
    }
}

fn effective_align_self_grid(child: &HtmlBox, parent: AlignItems) -> AlignItems {
    match child.style.align_self {
        AlignSelf::Auto      => parent,
        AlignSelf::Stretch   => AlignItems::Stretch,
        AlignSelf::FlexStart => AlignItems::FlexStart,
        AlignSelf::FlexEnd   => AlignItems::FlexEnd,
        AlignSelf::Center    => AlignItems::Center,
        AlignSelf::Baseline  => AlignItems::Baseline,
    }
}

// ─── finish & abs children ───────────────────────────────────────────────────

fn finish_grid(
    node: &mut HtmlBox,
    rbox: &ResolvedBox,
    content_x: f32, content_y: f32,
    content_w: f32, content_h: f32,
) -> f32 {
    let ch = rbox.content_height.unwrap_or(content_h);
    node.content_rect = Rect::new(content_x, content_y, content_w, ch);
    node.padding_rect = Rect::new(
        content_x - rbox.padding_left, content_y - rbox.padding_top,
        content_w + rbox.padding_left + rbox.padding_right,
        ch + rbox.padding_top + rbox.padding_bottom,
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
    node.baseline = node.content_rect.y + ch;
    node.margin_rect.h
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

// ─── justify-content / align-content distribution ────────────────────────────

/// Returns (offset, extra_gap) for justify-content.
/// Mirrors C++ grid justify-content calculation.
fn compute_justify_content(jc: JustifyContent, extra: f32, n: usize) -> (f32, f32) {
    if n == 0 || extra <= 0.0 { return (0.0, 0.0); }
    match jc {
        JustifyContent::Center      => (extra / 2.0, 0.0),
        JustifyContent::FlexEnd     => (extra, 0.0),
        JustifyContent::SpaceBetween =>
            if n > 1 { (0.0, extra / (n - 1) as f32) } else { (0.0, 0.0) },
        JustifyContent::SpaceAround => {
            let g = extra / n as f32;
            (g / 2.0, g)
        }
        JustifyContent::SpaceEvenly => {
            let g = extra / (n + 1) as f32;
            (g, g)
        }
        JustifyContent::FlexStart => (0.0, 0.0),
    }
}

/// Returns (offset, extra_gap) for align-content.
/// Mirrors C++ grid align-content calculation.
fn compute_align_content(ac: AlignContent, extra: f32, n: usize) -> (f32, f32) {
    if n == 0 || extra <= 0.0 { return (0.0, 0.0); }
    match ac {
        AlignContent::Center      => (extra / 2.0, 0.0),
        AlignContent::FlexEnd     => (extra, 0.0),
        AlignContent::SpaceBetween =>
            if n > 1 { (0.0, extra / (n - 1) as f32) } else { (0.0, 0.0) },
        AlignContent::SpaceAround => {
            let g = extra / n as f32;
            (g / 2.0, g)
        }
        AlignContent::SpaceEvenly => {
            let g = extra / (n + 1) as f32;
            (g, g)
        }
        AlignContent::FlexStart | AlignContent::Stretch => (0.0, 0.0),
    }
}

// ─── Old parse_track_sizes kept for backward compat ──────────────────────────

/// Parse grid-template-columns / grid-template-rows into pixel sizes (legacy string API).
/// Used by table.rs and any remaining callers.
pub fn parse_track_sizes(template: &str, container: f32, font_px: f32, root_font_px: f32) -> Vec<f32> {
    if template.is_empty() { return Vec::new(); }

    let expanded = expand_repeat(template, container);
    let mut sizes = Vec::new();
    let mut fr_indices: Vec<usize> = Vec::new();
    let mut fr_values:  Vec<f32>   = Vec::new();
    let mut used = 0.0f32;

    for token in expanded.split_whitespace() {
        let token = token.trim_matches(|c: char| c == '(' || c == ')');
        if token.ends_with("fr") {
            let fr: f32 = token[..token.len()-2].parse().unwrap_or(1.0);
            fr_indices.push(sizes.len());
            fr_values.push(fr);
            sizes.push(0.0);
        } else if token.ends_with("px") {
            let px: f32 = token[..token.len()-2].parse().unwrap_or(0.0);
            used += px;
            sizes.push(px);
        } else if token.ends_with('%') {
            let pct: f32 = token[..token.len()-1].parse().unwrap_or(0.0);
            let px = pct / 100.0 * container;
            used += px;
            sizes.push(px);
        } else if token.starts_with("minmax") {
            let inner = token.trim_start_matches("minmax").trim_matches(|c: char| c == '(' || c == ')');
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if let Some(max_part) = parts.get(1) {
                let p = max_part.trim();
                if p.ends_with("fr") {
                    let fr: f32 = p[..p.len()-2].parse().unwrap_or(1.0);
                    fr_indices.push(sizes.len());
                    fr_values.push(fr);
                    sizes.push(0.0);
                } else {
                    let px = crate::css::parse_length(p).resolve(font_px, container, root_font_px);
                    used += px;
                    sizes.push(px);
                }
            }
        } else if token == "auto" {
            sizes.push(0.0);
        }
    }

    if !fr_indices.is_empty() {
        let remaining = (container - used).max(0.0);
        let total_fr: f32 = fr_values.iter().sum();
        for (ii, &idx) in fr_indices.iter().enumerate() {
            sizes[idx] = remaining * fr_values[ii] / total_fr;
        }
    }

    sizes
}

fn expand_repeat(template: &str, container: f32) -> String {
    if !template.contains("repeat") { return template.to_string(); }

    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("repeat(") {
        out.push_str(&rest[..start]);
        let inner_start = start + 7;
        let mut depth = 1;
        let mut end = inner_start;
        let bytes = rest.as_bytes();
        while end < bytes.len() {
            if bytes[end] == b'(' { depth += 1; }
            if bytes[end] == b')' { depth -= 1; if depth == 0 { break; } }
            end += 1;
        }
        let inner = &rest[inner_start..end];
        let comma_pos = inner.find(',').unwrap_or(0);
        let count_str = inner[..comma_pos].trim();
        let track_str = inner[comma_pos + 1..].trim();

        let count = if count_str == "auto-fill" || count_str == "auto-fit" {
            let ts = crate::css::parse_length(track_str).resolve(16.0, container, 16.0).max(1.0);
            (container / ts) as usize
        } else {
            count_str.parse::<usize>().unwrap_or(1)
        };

        for i in 0..count {
            if i > 0 { out.push(' '); }
            out.push_str(track_str);
        }

        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}
