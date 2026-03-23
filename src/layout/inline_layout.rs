use crate::types::*;
use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping, Stretch, Style as CTextStyle, Weight};
use crate::layout::{LayoutEngine, ResolvedBox, FloatContext, FloatSide, layout_positioned};
use crate::layout::block::{collapse_two, compute_intrinsic_width};
use crate::layout::has_block_children;
use crate::layout::text::resolve_bidi_line;

/// Lay out a box whose children are inline-level (text runs, inline-block).
/// Returns total outer height of the box.
/// `float_ctx` is the float context from the containing block (may be None).
pub fn layout_inline_block(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    rbox:         &ResolvedBox,
    containing_w: f32,
    x:            f32,
    y:            f32,
    font_px:      f32,
    root_font_px: f32,
    parent_float_ctx: Option<&mut FloatContext>,
) -> f32 {
    // Create a local float context if the parent didn't provide one.
    // This ensures floated children inside inline containers are placed correctly.
    let mut fc_owned = FloatContext::default();
    let has_parent_fc = parent_float_ctx.is_some();
    let mut float_ctx: Option<&mut FloatContext> = if let Some(fc) = parent_float_ctx {
        // Use parent's float context -- but we can't store it directly because of
        // borrow rules, so we clone its state into fc_owned and use that.
        fc_owned = fc.clone();
        Some(&mut fc_owned)
    } else {
        Some(&mut fc_owned)
    };

    let raw_w = match rbox.content_width {
        Some(w) => w,
        None    => (containing_w - rbox.h_space()).max(0.0),
    };
    // Apply min/max-width, converting from border-box to content-box when needed.
    // CSS: with box-sizing:border-box, min/max-width refer to the border box, not the content box.
    let bb_extra = if node.style.box_sizing == crate::types::BoxSizing::BorderBox {
        rbox.padding_left + rbox.padding_right + rbox.border_left + rbox.border_right
    } else { 0.0 };
    let min_w = {
        let v = engine.res_len(&node.style.min_width, font_px, containing_w, root_font_px);
        (v - bb_extra).max(0.0)
    };
    let max_w = if node.style.max_width.is_none() || node.style.max_width.is_auto() { f32::MAX } else {
        let v = engine.res_len(&node.style.max_width, font_px, containing_w, root_font_px);
        (v - bb_extra).max(0.0)
    };
    let content_w = raw_w.max(min_w).min(max_w);

    // Auto margin centering (CSS 2.1 §10.3.3)
    let margin_left;
    let margin_right;
    let left_is_auto  = node.style.margin_left.is_auto();
    let right_is_auto = node.style.margin_right.is_auto();
    if !node.style.width.is_auto() && (left_is_auto || right_is_auto) {
        let non_margin_space = rbox.border_left + rbox.padding_left + content_w
                             + rbox.padding_right + rbox.border_right;
        let available = (containing_w - non_margin_space).max(0.0);
        if left_is_auto && right_is_auto {
            margin_left  = (available / 2.0).floor();
            margin_right = available - margin_left;
        } else if left_is_auto {
            margin_left  = available - rbox.margin_right;
            margin_right = rbox.margin_right;
        } else {
            margin_left  = rbox.margin_left;
            margin_right = available - rbox.margin_left;
        }
    } else {
        margin_left  = rbox.margin_left;
        margin_right = rbox.margin_right;
    }

    let content_x = x + margin_left + rbox.border_left + rbox.padding_left;
    let content_y = y + rbox.margin_top  + rbox.border_top  + rbox.padding_top;

    // Set float context origin now that content_y is known
    if let Some(ref mut fc) = float_ctx {
        if fc.origin_y == 0.0 && fc.floats.is_empty() {
            fc.origin_y = content_y;
        }
    }

    // ── 0. Pre-layout atomic inline-block and float children so sizes are known ──
    //    Mirrors C++ LayoutInlines(): LayoutBox(*run.atomicBox, …) before line-breaking
    //    Also recurse into inline children to pre-layout nested inline-blocks (e.g.
    //    <input type="radio"> inside a <label>).
    prelayout_nested_inline_blocks(engine, node, content_w, font_px, root_font_px);

    for ci in 0..node.children.len() {
        if matches!(node.children[ci].style.display,
                    Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) {
            engine.layout_box(&mut node.children[ci], content_w,
                               0.0, 0.0, font_px, root_font_px);
            // Shrink-to-fit for auto-width inline-block (CSS §10.3.9):
            // InlineBlock with width:auto should size to content, not expand to fill container.
            if node.children[ci].style.width.is_auto() {
                // Use line.width (raw text content width) not line.x + line.width - origin,
                // because line.x includes the text-align centering offset which inflates
                // the result when text-align:center is inherited.
                let max_line_w = node.children[ci].line_cache.iter()
                    .map(|l| l.width)
                    .fold(0.0_f32, f32::max);
                // For block-container inline-blocks (e.g. ul/div with block children),
                // line_cache is empty — fall back to compute_intrinsic_width which
                // recurses into block children to find max content width.
                let intrinsic_w = if max_line_w > 0.0 {
                    max_line_w
                } else {
                    compute_intrinsic_width(&node.children[ci])
                };
                {
                    let irb = &node.children[ci];
                    let shrink_w = intrinsic_w
                        + irb.resolved_pad_left + irb.resolved_pad_right
                        + irb.resolved_border_left + irb.resolved_border_right
                        + irb.resolved_margin_left + irb.resolved_margin_right;
                    if shrink_w < content_w {
                        engine.layout_box(&mut node.children[ci], shrink_w,
                                           0.0, 0.0, font_px, root_font_px);
                    }
                }
            }
        } else if node.children[ci].style.is_inline_level()
                  && has_block_children(&node.children[ci]) {
            // Inline element containing block-level children (e.g. <a><strong style="display:block">).
            // Per CSS, this creates an anonymous block formatting context. We approximate by
            // pre-laying the element out as a block container so its children get proper dimensions.
            engine.layout_box(&mut node.children[ci], content_w,
                               0.0, 0.0, font_px, root_font_px);
        } else if !matches!(node.children[ci].style.float, crate::types::Float::None) {
            // Float children need to be laid out to get valid dimensions.
            engine.layout_box(&mut node.children[ci], content_w,
                               content_x, content_y, font_px, root_font_px);
            // Shrink-to-fit for auto-width floats
            if node.children[ci].style.width.is_auto() {
                let intrinsic_w = compute_intrinsic_width(&node.children[ci]);
                if intrinsic_w > 0.0 && intrinsic_w < content_w {
                    let irb = &node.children[ci];
                    let shrink_w = intrinsic_w
                        + irb.resolved_pad_left + irb.resolved_pad_right
                        + irb.resolved_border_left + irb.resolved_border_right
                        + irb.resolved_margin_left + irb.resolved_margin_right;
                    engine.layout_box(&mut node.children[ci], shrink_w,
                                       content_x, content_y, font_px, root_font_px);
                }
            }
        }
    }

    // ── 1. Measure ::before / ::after pseudo-element widths ───────────────────
    let pseudo_font_px = |ps: Option<&ComputedStyle>| -> f32 {
        ps.and_then(|s| { let f = s.font_size.resolve(font_px, 0.0, root_font_px); if f > 0.0 { Some(f) } else { None } })
          .unwrap_or(font_px)
    };
    let scale = engine.scale;
    let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
    let before_w = if !node.style.before_content.is_empty() {
        let bfpx = pseudo_font_px(node.style.before_style.as_deref());
        measure_text_width_scaled(&node.style.before_content, bfpx, font_system, scale)
    } else { 0.0 };
    let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
    let after_w = if !node.style.after_content.is_empty() {
        let afpx = pseudo_font_px(node.style.after_style.as_deref());
        measure_text_width_scaled(&node.style.after_content, afpx, font_system, scale)
    } else { 0.0 };

    // ── 2. Collect flat inline items from all inline children ─────────────────
    let mut text_offset = 0usize;
    let mut items: Vec<InlineItem> = Vec::new();
    let mut runs:  Vec<InlineRun>  = Vec::new();
    for (i, child) in node.children.iter().enumerate() {
        if matches!(child.style.display, Display::None) { continue; }
        collect_items(engine, child, font_px, root_font_px, &mut items, &mut runs, &mut text_offset, i, true, &[]);
    }
    // Also collect from own text (text directly inside element)
    if !node.text.is_empty() {
        if node.is_text_node() {
            // Text node laid out directly (e.g. as a flex child): collect self,
            // but skip whitespace-only nodes (handled by parent inline layout).
            if !node.text.chars().all(|c| c.is_ascii_whitespace()) {
                collect_items(engine, node, font_px, root_font_px, &mut items, &mut runs, &mut text_offset, 0, false, &[]);
            }
        } else if !node.inline_runs.is_empty() {
            // Block has pre-built inline runs (e.g. from the markdown parser).
            // Emit one #text item per run to preserve bold, italic, link color, etc.
            // The plain "node.style" path below would silently drop all inline formatting.
            let saved_runs = node.inline_runs.clone();
            for run in &saved_runs {
                let end = (run.text_offset + run.length).min(node.text.len());
                if run.text_offset >= end { continue; }
                let run_text = node.text[run.text_offset..end].to_string();
                let mut tmp = HtmlBox::new("#text");
                tmp.text = run_text;
                tmp.style = run.style.clone();
                collect_items(engine, &tmp, font_px, root_font_px, &mut items, &mut runs, &mut text_offset, 0, false, &[]);
            }
        } else {
            let mut tmp_node = HtmlBox::new("#text");
            tmp_node.text = node.text.clone();
            tmp_node.style = node.style.clone();
            collect_items(engine, &tmp_node, font_px, root_font_px, &mut items, &mut runs, &mut text_offset, 0, false, &[]);
        }
    }

    // ── 3. Save old lines for early-stop optimization ─────────────────────────
    let old_lines: Vec<LayoutLine> = std::mem::take(&mut node.line_cache);

    if items.is_empty() {
        // Nothing to lay out.
        // For non-void blocks with no children (e.g. empty <p> after Enter), add a
        // placeholder line so the caret has a home and the block has visible height.
        const VOID_TAGS: &[&str] = &[
            "area", "base", "br", "col", "embed", "hr", "img", "input",
            "link", "meta", "param", "source", "track", "wbr",
        ];
        let is_void = VOID_TAGS.contains(&node.tag.as_str());
        // Only add a placeholder line for elements that can hold inline/prose content
        // and need a visible cursor when empty. Generic structural divs/sections must
        // NOT get one — it would break margin collapsing for empty blocks.
        let is_prose_tag = matches!(node.tag.as_str(),
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            | "li" | "dt" | "dd" | "pre" | "blockquote"
            | "td" | "th" | "caption"
        );
        let is_contenteditable = node.attributes.get("contenteditable")
            .map(|v| v == "true").unwrap_or(false);
        let has_pseudo_content = before_w > 0.0 || after_w > 0.0;
        let add_placeholder = !is_void
            && node.children.is_empty()
            && rbox.content_height.is_none()
            && (is_prose_tag || is_contenteditable || has_pseudo_content);
        if add_placeholder {
            let pseudo_w = before_w + after_w;
            let ps_font = if has_pseudo_content {
                pseudo_font_px(node.style.before_style.as_deref()
                    .or(node.style.after_style.as_deref()))
            } else { font_px };
            let eff_fpx = if has_pseudo_content { ps_font } else { font_px };
            let line_h = eff_fpx * 1.2;
            node.line_cache = vec![LayoutLine {
                text_start: text_offset,
                text_length: 0,
                x: content_x,
                y: content_y,
                width: pseudo_w,
                height: line_h,
                ascent: eff_fpx,
                descent: eff_fpx * 0.2,
                extra_space_per_word: 0.0, text_x_offset: 0.0,
                visual_segments: Vec::new(),
                char_x: Vec::new(),
            }];
        }
        // <br> in block context (no inline siblings collected it as a Break item)
        // must still produce a line-height of vertical space, just like it would
        // inside a paragraph.
        let br_h = if node.tag == "br" { font_px * 1.2 } else { 0.0 };
        let eff_placeholder_fpx = if has_pseudo_content {
            pseudo_font_px(node.style.before_style.as_deref()
                .or(node.style.after_style.as_deref()))
        } else { font_px };
        let placeholder_h = if add_placeholder { eff_placeholder_fpx * 1.2 } else { br_h };
        let min_h = engine.res_len(&node.style.min_height, font_px, 0.0, root_font_px);
        let max_h = if node.style.max_height.is_none() || node.style.max_height.is_auto() { f32::MAX }
                    else {
                        let v = engine.res_len(&node.style.max_height, font_px, 0.0, root_font_px);
                        // Percentage max-height against unknown (0) containing height → treat as none
                        if v == 0.0 && matches!(node.style.max_height, CssLength::Percent(_)) { f32::MAX } else { v }
                    };
        let content_h = if let Some(h) = rbox.content_height {
            h
        } else if let Some(ratio) = node.style.aspect_ratio {
            if ratio > 0.0 { (content_w / ratio).max(0.0).max(min_h).min(max_h) } else { placeholder_h }
        } else {
            placeholder_h
        };
        let content_h = content_h.max(min_h).min(max_h);
        set_box_rects(node, content_x, content_y, content_w, content_h,
                      rbox, margin_left, margin_right);
        node.inline_runs = runs;
        // Still need to lay out absolutely/fixed positioned children.
        // Use collect_grid_children to flatten through display:contents,
        // matching block layout behaviour (CSS §2.7).
        let containing_rect = if !matches!(node.style.position, Position::Static) {
            node.padding_rect
        } else {
            engine.pos_cb.get()
        };
        let eff = crate::layout::grid::collect_grid_children(node);
        let abs_paths: Vec<Vec<usize>> = eff.into_iter()
            .filter(|path| {
                let c = crate::layout::grid::grid_child_ref(node, path);
                matches!(c.style.position, Position::Absolute | Position::Fixed)
            })
            .collect();
        for path in &abs_paths {
            let child = crate::layout::grid::grid_child_mut(node, path);
            layout_positioned(engine, child, containing_rect, font_px, root_font_px);
            // All-auto correction: when no insets are specified, place at containing block's origin.
            let child = crate::layout::grid::grid_child_mut(node, path);
            let all_auto = child.style.left.is_auto()  && child.style.right.is_auto()
                        && child.style.top.is_auto()   && child.style.bottom.is_auto();
            if all_auto && matches!(child.style.position, Position::Absolute) {
                let dx = containing_rect.x - child.border_rect.x;
                let dy = containing_rect.y - child.border_rect.y;
                if dx.abs() > 0.01 || dy.abs() > 0.01 {
                    crate::layout::shift_rects(child, dx, dy);
                }
            }
        }
        return node.margin_rect.h;
    }

    // ── 4. Line-by-line layout (float-aware) ──────────────────────────────────
    let text_indent = engine.res_len(&node.style.text_indent, font_px, content_w, root_font_px);
    let is_rtl = node.style.direction == Direction::RTL;

    let mut cursor_y     = content_y;
    let mut item_idx     = 0usize;
    let mut line_cache:  Vec<LayoutLine>           = Vec::new();
    let mut atomic_pos:  Vec<(Vec<usize>, f32, f32)> = Vec::new(); // (path, x, y)
    let mut old_line_idx = 0usize;
    let mut ends_with_break = false;
    let mut loop_guard   = 0usize;

    while item_idx < items.len() {
        loop_guard += 1;
        if loop_guard > 10000 { break; }
        let is_first_line = line_cache.is_empty();

        // ── Place leading floats before current line ──────────────────────────
        while item_idx < items.len() {
            if let InlineItemKind::Float { child_idx } = items[item_idx].kind {
                if let Some(ref mut fc) = float_ctx {
                    let child = &mut node.children[child_idx];
                    let float_w = (child.border_rect.w
                        + child.resolved_margin_left + child.resolved_margin_right).max(0.0);
                    let float_h = child.margin_rect.h;
                    let side = if child.style.float == crate::types::Float::Right { FloatSide::Right } else { FloatSide::Left };
                    let placed = fc.place_float(cursor_y - fc.origin_y, float_w, float_h, content_w, side);
                    let dx = content_x + placed.x - child.margin_rect.x;
                    let dy = fc.origin_y + placed.y - child.margin_rect.y;
                    crate::layout::shift_rects(child, dx, dy);
                }
                item_idx += 1;
            } else {
                break;
            }
        }

        // Query float context for available horizontal band at this Y
        let est_line_h = font_px * 1.2;
        let (mut fc_left, mut fc_right) = (0.0f32, content_w);
        if let Some(fc) = float_ctx.as_ref() {
            fc.available_width(cursor_y - fc.origin_y, est_line_h, content_w,
                               &mut fc_left, &mut fc_right);
        }

        // Apply text-indent and ::before on first line
        if is_first_line {
            fc_left += text_indent;
            fc_left += before_w;
        }

        // white-space:pre and nowrap never wrap at word boundaries.
        // avail_w controls line-breaking; align_w is used for text alignment
        // (always finite so right/center alignment doesn't overflow).
        let finite_w = (fc_right - fc_left).max(0.0);
        let mut avail_w = if matches!(node.style.white_space, WhiteSpace::Pre | WhiteSpace::Nowrap) {
            f32::MAX
        } else {
            finite_w
        };
        let align_w = finite_w;

        // Break items for this line
        let (mut line_end, mut next_start, mut was_break) =
            break_one_line(&items, item_idx, avail_w);

        // Greedily pull in and place floats that were included in the line slice
        // If a float placement narrows the width such that items no longer fit,
        // we must re-break the line.
        let mut i = item_idx;
        while i < line_end {
            if let InlineItemKind::Float { child_idx } = items[i].kind {
                if let Some(ref mut fc) = float_ctx {
                    let child = &mut node.children[child_idx];
                    let float_w = (child.border_rect.w
                        + child.resolved_margin_left + child.resolved_margin_right).max(0.0);
                    let float_h = child.margin_rect.h;
                    let side = if child.style.float == crate::types::Float::Right { FloatSide::Right } else { FloatSide::Left };
                    let placed = fc.place_float(cursor_y - fc.origin_y, float_w, float_h, content_w, side);
                    let dx = content_x + placed.x - child.margin_rect.x;
                    let dy = fc.origin_y + placed.y - child.margin_rect.y;
                    crate::layout::shift_rects(child, dx, dy);
                    
                    // Width might have changed
                    fc.available_width(cursor_y - fc.origin_y, est_line_h, content_w, &mut fc_left, &mut fc_right);
                    let temp_fc_left = if is_first_line { fc_left + text_indent + before_w } else { fc_left };
                    avail_w = (fc_right - temp_fc_left).max(0.0);
                    
                    // Re-evaluate line break from THIS point forward
                    let (new_end, new_next, new_break) = break_one_line(&items, i + 1, avail_w);
                    line_end = new_end;
                    next_start = new_next;
                    was_break = new_break;
                }
            }
            i += 1;
        }

        ends_with_break = was_break;

        if line_end == item_idx && !was_break {
            // Safety: avoid infinite loop if no progress made
            if item_idx < items.len() && matches!(items[item_idx].kind, InlineItemKind::Float { .. }) {
                item_idx += 1;
                continue;
            }
            break;
        }

        let line_items = &items[item_idx..line_end];

        // Compute line metrics from items
        let (mut line_h, mut line_asc, mut line_desc) =
            measure_metrics(line_items, font_px);

        // CSS line-height: apply only when explicitly set (not auto)
        if !node.style.line_height.is_auto() {
            let lh_val = engine.res_len(&node.style.line_height, font_px, 0.0, root_font_px);
            if lh_val > line_h {
                let half = ((lh_val - line_h) / 2.0).floor();
                line_asc  += half;
                line_desc += (lh_val - line_h) - half;
                line_h     = lh_val;
            }
        }

        // Measure content width: CSS requires stripping leading/trailing
        // collapsible whitespace from each line before alignment.
        // Find the first and last non-space, non-break items.
        let first_content = line_items.iter()
            .position(|it| !it.is_space && !matches!(it.kind, InlineItemKind::Break));
        let last_content = line_items.iter()
            .rposition(|it| !it.is_space && !matches!(it.kind, InlineItemKind::Break));
        let content_line_w: f32 = match (first_content, last_content) {
            (Some(f), Some(l)) => line_items[f..=l].iter().map(|it| it.advance).sum(),
            _ => 0.0,
        };

        // Resolve text-align Start/End based on direction
        let effective_align = match node.style.text_align {
            TextAlign::Start => if is_rtl { TextAlign::Right } else { TextAlign::Left },
            TextAlign::End   => if is_rtl { TextAlign::Left  } else { TextAlign::Right },
            a => a,
        };

        // Justify: compute extra space per word gap (use align_w, never f32::MAX)
        let extra_per_gap = if effective_align == TextAlign::Justify && !was_break && next_start < items.len() {
            let gaps = line_items.iter().filter(|it| it.is_space).count() as f32;
            if gaps > 0.0 { ((align_w - content_line_w) / gaps).max(0.0) } else { 0.0 }
        } else { 0.0 };

        // X offset for text alignment (use align_w, never f32::MAX)
        let mut line_x = content_x + fc_left + match effective_align {
            TextAlign::Right  => (align_w - content_line_w).max(0.0),
            TextAlign::Center => ((align_w - content_line_w) / 2.0).max(0.0),
            _                 => 0.0,
        };
        if line_x < content_x { line_x = content_x; }

        // Account for ::before on first line, ::after on last line
        let is_last_line = next_start >= items.len();
        if is_first_line && before_w > 0.0 {
            line_x -= before_w;
            if line_x < content_x { line_x = content_x; }
        }
        let line_w_total = content_line_w
            + if is_first_line { before_w } else { 0.0 }
            + if is_last_line  { after_w  } else { 0.0 };

        // Compute text range for this line, stripping leading/trailing collapsible
        // whitespace per CSS §16.6.1. Use first_content/last_content from above.
        let content_items = match (first_content, last_content) {
            (Some(f), Some(l)) => &line_items[f..=l],
            _ => &line_items[0..0],
        };
        let text_s = content_items.iter().filter_map(|it| {
            if let InlineItemKind::Text { text_start, .. } = &it.kind { Some(*text_start) } else { None }
        }).min().unwrap_or_else(|| {
            // Fallback: use any item if all are spaces
            line_items.iter().filter_map(|it| {
                if let InlineItemKind::Text { text_start, .. } = &it.kind { Some(*text_start) } else { None }
            }).min().unwrap_or(0)
        });
        let text_e = content_items.iter().filter_map(|it| {
            if let InlineItemKind::Text { text_start, text_len, .. } = &it.kind {
                Some(text_start + text_len)
            } else { None }
        }).max().unwrap_or_else(|| {
            line_items.iter().filter_map(|it| {
                if let InlineItemKind::Text { text_start, text_len, .. } = &it.kind {
                    Some(text_start + text_len)
                } else { None }
            }).max().unwrap_or(text_s)
        });

        // Early-stop: if matching an old cached line with same breaks at same X and Y
        // (only when no floats involved; check x so different column positions don't reuse cache)
        if float_ctx.is_none() && old_line_idx > 0 && old_line_idx < old_lines.len() {
            let ol = &old_lines[old_line_idx];
            if ol.text_start == text_s
                && ol.text_length == text_e.saturating_sub(text_s)
                && ol.y == cursor_y
                && (ol.x - line_x).abs() < 0.5
            {
                // Rest of lines unchanged — copy them
                for j in old_line_idx..old_lines.len() {
                    line_cache.push(old_lines[j].clone());
                }
                node.line_cache = line_cache;
                node.inline_runs = runs;
                // Update box rects with cached height
                let bottom = node.line_cache.last()
                    .map(|l| l.y + l.height)
                    .unwrap_or(cursor_y);
                let raw_h = (bottom - content_y).max(0.0);
                let content_h = match rbox.content_height { Some(h) => h, None => raw_h };
                let min_h = engine.res_len(&node.style.min_height, font_px, 0.0, root_font_px);
                let max_h = if node.style.max_height.is_none() || node.style.max_height.is_auto() { f32::MAX }
                            else {
                                let v = engine.res_len(&node.style.max_height, font_px, 0.0, root_font_px);
                                if v == 0.0 && matches!(node.style.max_height, CssLength::Percent(_)) { f32::MAX } else { v }
                            };
                let content_h = content_h.max(min_h).min(max_h);
                set_box_rects(node, content_x, content_y, content_w, content_h,
                              rbox, margin_left, margin_right);
                return node.margin_rect.h;
            }
        }

        // Collect atomic positions on this line
        {
            let mut cur_x = line_x;
            for item in line_items {
                if let InlineItemKind::Atomic { path } = &item.kind {
                    let child_node = resolve_path(node, path);
                    let box_h = child_node
                        .map(|n| n.margin_rect.h)
                        .unwrap_or(item.height);
                    let valign = child_node
                        .map(|n| n.style.vertical_align)
                        .unwrap_or(crate::types::VerticalAlign::Baseline);
                    let ay = match valign {
                        crate::types::VerticalAlign::Top =>
                            cursor_y,
                        crate::types::VerticalAlign::Bottom =>
                            cursor_y + line_h - box_h,
                        crate::types::VerticalAlign::Middle =>
                            cursor_y + (line_h - box_h) / 2.0,
                        _ => {
                            // Baseline: bottom margin edge on the line baseline
                            let ay = cursor_y + line_asc - box_h;
                            ay.max(cursor_y)
                        }
                    };
                    atomic_pos.push((path.clone(), cur_x, ay));
                }
                cur_x += item.advance;
            }
        }

        // Build LayoutLine
        // Compute text_x_offset: sum of advances of items before first text item
        let mut text_x_off = 0.0f32;
        for item in line_items.iter() {
            match &item.kind {
                InlineItemKind::Text { .. } => break,
                _ => text_x_off += item.advance,
            }
        }

        let mut ll = LayoutLine {
            text_start:  text_s,
            text_length: text_e.saturating_sub(text_s),
            x:      line_x,
            y:      cursor_y,
            width:  line_w_total,
            height: line_h,
            ascent: line_asc,
            descent: line_desc,
            extra_space_per_word: extra_per_gap,
            text_x_offset: text_x_off,
            visual_segments: Vec::new(),
            char_x: Vec::new(),
        };

        // Resolve BiDi visual segments for this line
        let para_dir = node.style.direction;
        let flat_text = collect_flat_text(node);
        resolve_bidi_line(&flat_text, &mut ll, para_dir);

        // Fill per-character x positions using real glyph metrics, shaped at
        // physical pixel size so positions match the renderer exactly.
        if let Some(fs_ptr) = engine.font_system {
            let fs = unsafe { &mut *fs_ptr };
            fill_char_x_for_line(fs, &flat_text, &runs, &mut ll, engine.scale);
        }

        line_cache.push(ll);
        cursor_y += line_h;
        item_idx = next_start;
        old_line_idx += 1;
    }

    // Empty block with no content: add one empty line so the caret has a home.
    if line_cache.is_empty() && items.is_empty() {
        line_cache.push(LayoutLine {
            text_start:  text_offset,
            text_length: 0,
            x:      content_x,
            y:      cursor_y,
            width:  0.0,
            height: font_px * 1.2,
            ascent: font_px * 1.2,
            descent: 0.0,
            extra_space_per_word: 0.0, text_x_offset: 0.0,
            visual_segments: Vec::new(),
            char_x: Vec::new(),
        });
        cursor_y += font_px * 1.2;
    }

    // Trailing empty line after <br> (for caret positioning after Enter)
    if ends_with_break {
        line_cache.push(LayoutLine {
            text_start:  text_offset,
            text_length: 0,
            x:      content_x,
            y:      cursor_y,
            width:  0.0,
            height: font_px * 1.2,
            ascent: font_px * 1.2,
            descent: 0.0,
            extra_space_per_word: 0.0, text_x_offset: 0.0,
            visual_segments: Vec::new(),
            char_x: Vec::new(),
        });
        cursor_y += font_px * 1.2;
    }

    // ── 5. Compute content height ──────────────────────────────────────────────
    let inline_h = (cursor_y - content_y).max(0.0);
    // Include float bottom so the container encloses its floats.
    // Only do this for the element that OWNS the float context, not children
    // that inherit it (they shouldn't extend to contain the parent's floats).
    let float_bottom = if !has_parent_fc {
        if let Some(ref fc) = float_ctx {
            fc.floats.iter().map(|f| f.clear).fold(0.0f32, f32::max)
        } else { 0.0 }
    } else { 0.0 };
    let raw_h = inline_h.max(float_bottom);
    let min_h = engine.res_len(&node.style.min_height, font_px, 0.0, root_font_px);
    let max_h = if node.style.max_height.is_none() || node.style.max_height.is_auto() { f32::MAX }
                else {
                    let v = engine.res_len(&node.style.max_height, font_px, 0.0, root_font_px);
                    if v == 0.0 && matches!(node.style.max_height, CssLength::Percent(_)) { f32::MAX } else { v }
                };
    let content_h = match rbox.content_height { Some(h) => h, None => raw_h };
    let content_h = content_h.max(min_h).min(max_h);

    // Apply aspect-ratio: if height is auto and aspect_ratio is set, derive height from width
    let content_h = if rbox.content_height.is_none() {
        if let Some(ratio) = node.style.aspect_ratio {
            if ratio > 0.0 { (content_w / ratio).max(0.0).max(min_h).min(max_h) } else { content_h }
        } else { content_h }
    } else { content_h };

    set_box_rects(node, content_x, content_y, content_w, content_h,
                  rbox, margin_left, margin_right);
    node.line_cache  = line_cache;
    node.inline_runs = runs;

    // ── 5b. Scroll extent for overflow:scroll/auto inline containers ───────────
    if matches!(node.style.overflow_x, crate::types::Overflow::Scroll | crate::types::Overflow::Auto)
    || matches!(node.style.overflow_y, crate::types::Overflow::Scroll | crate::types::Overflow::Auto)
    {
        // Natural content height from inline lines
        let natural_h = raw_h.max(content_h);
        // Natural content width: max width across all lines
        let natural_w = node.line_cache.iter()
            .map(|l| l.width)
            .fold(content_w, f32::max);
        node.scroll_height = natural_h;
        node.scroll_width  = natural_w;
        let max_v = (node.scroll_height - content_h).max(0.0);
        let max_h = (node.scroll_width  - content_w).max(0.0);
        node.scroll_top  = node.scroll_top.min(max_v).max(0.0);
        node.scroll_left = node.scroll_left.min(max_h).max(0.0);
    } else {
        node.scroll_height = content_h;
        node.scroll_width  = content_w;
        node.scroll_top    = 0.0;
        node.scroll_left   = 0.0;
    }

    // ── 6. Position atomic inline-block children ──────────────────────────────
    //    Separate pass after all lines are built (mirrors C++ post-loop pass)
    for (path, ax, ay) in atomic_pos {
        if let Some(target) = resolve_path_mut(node, &path) {
            // Shift child rects to final position
            let dx = ax - target.margin_rect.x;
            let dy = ay - target.margin_rect.y;
            crate::layout::shift_rects(target, dx, dy);
        }
    }

    // ── 7. Absolutely/fixed positioned children ────────────────────────────────
    //    Inline containers can still be containing blocks for absolutely-positioned
    //    children (e.g. a `position:relative` div whose only visible in-flow content
    //    is text while its absolutely-placed children are out-of-flow).
    let containing_rect = if !matches!(node.style.position, Position::Static) {
        node.padding_rect
    } else {
        engine.pos_cb.get()
    };
    let eff2 = crate::layout::grid::collect_grid_children(node);
    let abs_paths2: Vec<Vec<usize>> = eff2.into_iter()
        .filter(|path| {
            let c = crate::layout::grid::grid_child_ref(node, path);
            matches!(c.style.position, Position::Absolute | Position::Fixed)
        })
        .collect();
    for path in &abs_paths2 {
        let child = crate::layout::grid::grid_child_mut(node, path);
        layout_positioned(engine, child, containing_rect, font_px, root_font_px);
        let child = crate::layout::grid::grid_child_mut(node, path);
        let all_auto = child.style.left.is_auto()  && child.style.right.is_auto()
                    && child.style.top.is_auto()   && child.style.bottom.is_auto();
        if all_auto && matches!(child.style.position, Position::Absolute) {
            let dx = containing_rect.x - child.border_rect.x;
            let dy = containing_rect.y - child.border_rect.y;
            if dx.abs() > 0.01 || dy.abs() > 0.01 {
                crate::layout::shift_rects(child, dx, dy);
            }
        }
    }

    node.margin_rect.h
}

// ─── Path resolution helpers ─────────────────────────────────────────────────

/// Follow a chain of child indices to find the target node (immutable).
fn resolve_path<'a>(root: &'a HtmlBox, path: &[usize]) -> Option<&'a HtmlBox> {
    let mut cur = root;
    for &idx in path {
        if idx >= cur.children.len() { return None; }
        cur = &cur.children[idx];
    }
    Some(cur)
}

/// Follow a chain of child indices to find the target node (mutable).
fn resolve_path_mut<'a>(root: &'a mut HtmlBox, path: &[usize]) -> Option<&'a mut HtmlBox> {
    let mut cur = root;
    for &idx in path {
        if idx >= cur.children.len() { return None; }
        cur = &mut cur.children[idx];
    }
    Some(cur)
}

// ─── Box rect helper ──────────────────────────────────────────────────────────

fn set_box_rects(
    node:       &mut HtmlBox,
    content_x:  f32, content_y: f32,
    content_w:  f32, content_h: f32,
    rbox:       &ResolvedBox,
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
    let mr_w = (node.border_rect.w + margin_left + margin_right).max(node.border_rect.w);
    node.margin_rect = Rect::new(
        node.border_rect.x - margin_left,
        node.border_rect.y - rbox.margin_top,
        mr_w,
        node.border_rect.h + rbox.margin_top  + rbox.margin_bottom,
    );
    node.baseline = content_y + content_h;
    // Cache resolved values (same as build_box_rects in block.rs)
    node.resolved_margin_top    = rbox.margin_top;
    node.resolved_margin_right  = margin_right;
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
    // Expose own margins for parent's margin-collapsing logic.
    // For "empty" blocks (no content, no border, no padding, no explicit height),
    // top and bottom margins collapse into each other per CSS 2.1 §8.3.1.
    let is_empty = content_h == 0.0
        && rbox.border_top    == 0.0 && rbox.border_bottom    == 0.0
        && rbox.padding_top   == 0.0 && rbox.padding_bottom   == 0.0
        && rbox.content_height.is_none();
    if is_empty && node.style.min_height.is_auto() {
        node.collapsed_margin_top    = collapse_two(rbox.margin_top, rbox.margin_bottom);
        node.collapsed_margin_bottom = 0.0;
    } else {
        node.collapsed_margin_top    = rbox.margin_top;
        node.collapsed_margin_bottom = rbox.margin_bottom;
    }
}

// ─── Line metrics ─────────────────────────────────────────────────────────────

fn measure_metrics(items: &[InlineItem], fallback_font_px: f32) -> (f32, f32, f32) {
    let mut max_asc  = 0.0f32;
    let mut max_desc = 0.0f32;
    let mut any = false;
    for it in items {
        if matches!(it.kind, InlineItemKind::Break) { continue; }
        if it.ascent  > max_asc  { max_asc  = it.ascent;  }
        if it.descent > max_desc { max_desc = it.descent; }
        any = true;
    }
    if !any {
        let (a, d) = approx_font_metrics(fallback_font_px, None);
        return (a + d, a, d);
    }
    let h = (max_asc + max_desc).max(fallback_font_px * 1.2);
    (h, max_asc, max_desc)
}

// ─── Inline Item ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum InlineItemKind {
    /// A word or space segment. text_start/text_len are offsets into the
    /// concatenated text collected by `collect_items`.
    Text  { text_start: usize, text_len: usize, box_idx: usize },
    /// An inline-block child.  `path` is a chain of child indices from the
    /// block container down to the actual InlineBlock node (e.g. [2, 0, 0]
    /// means node.children[2].children[0].children[0]).
    Atomic { path: Vec<usize> },
    /// Forced line break (<br>).
    Break,
    /// A floated child.
    Float { child_idx: usize },
}

#[derive(Debug, Clone)]
pub struct InlineItem {
    pub kind:      InlineItemKind,
    pub advance:   f32,
    pub ascent:    f32,
    pub descent:   f32,
    pub height:    f32,
    pub is_space:  bool,
    pub breakable: bool,
}

// ─── Collect inline items ────────────────────────────────────────────────────

/// Walk a node and emit InlineItems into `items`, also building style runs.
/// `text_offset` tracks the current byte position in the global flat-text string.
/// `is_direct_child` must be `true` only when `node` is an immediate child of the
/// inline container being laid out; Float items use `box_idx` to index back into
/// that container's `children` vec, so the index is only valid at depth 0.
pub fn collect_items(
    engine:          &LayoutEngine,
    node:            &HtmlBox,
    parent_font_px:  f32,
    root_font_px:    f32,
    items:           &mut Vec<InlineItem>,
    runs:            &mut Vec<InlineRun>,
    text_offset:     &mut usize,
    box_idx:         usize,
    is_direct_child: bool,
    ancestor_path:   &[usize],
) {
    if matches!(node.style.display, Display::None) { return; }

    // Absolutely/fixed positioned elements are out of flow — skip them here;
    // they are laid out separately by layout_positioned.
    if matches!(node.style.position, Position::Absolute | Position::Fixed) { return; }

    // ── Float ─────────────────────────────────────────────────────────────
    // Only emit a Float item when this node is a *direct* child of the
    // inline container being laid out.  Nested floats (float inside a <span>
    // inside a block) would produce an out-of-bounds child_idx; we fall through
    // and render them inline instead.
    if !matches!(node.style.float, crate::types::Float::None) && is_direct_child {
        items.push(InlineItem {
            kind: InlineItemKind::Float { child_idx: box_idx },
            advance: 0.0, ascent: 0.0, descent: 0.0, height: 0.0,
            is_space: false, breakable: false,
        });
        return;
    }

    let font_px = node.style.font_size_px(parent_font_px, root_font_px);
    let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
    let (ascent, descent) = approx_font_metrics(font_px, font_system);
    let line_h = engine.res_len(&node.style.line_height, font_px, 0.0, root_font_px)
                     .max(font_px * 1.2);

    // ── Text node ─────────────────────────────────────────────────────────
    if node.is_text_node() {
        if !node.text.is_empty() {
            let start = *text_offset;
            tokenize_text(engine, &node.text, node.style.white_space, start, font_px, ascent, descent, line_h, box_idx, items, node.style.font_weight, node.style.font_style);
            runs.push(InlineRun { text_offset: start, length: node.text.len(), style: node.style.clone() });
            *text_offset += node.text.len();
        }
        return;
    }

    // ── Forced line break ─────────────────────────────────────────────────
    if node.tag == "br" {
        items.push(InlineItem {
            kind:      InlineItemKind::Break,
            advance:   0.0, ascent, descent, height: line_h,
            is_space:  false, breakable: false,
        });
        return;
    }

    // ── Atomic inline-block ───────────────────────────────────────────────
    // Also treat inline elements that contain block-level children as atomic.
    // This handles the "block inside inline" case (e.g. <a><strong display:block>).
    let is_inline_with_block_children = is_direct_child
        && node.style.is_inline_level()
        && !node.is_text_node()
        && has_block_children(node);
    if matches!(node.style.display, Display::InlineBlock | Display::InlineFlex | Display::InlineGrid)
        || is_inline_with_block_children
    {
        // Use the pre-laid-out margin-rect width (set by the pre-layout pass)
        let box_w = if node.margin_rect.w > 0.0 { node.margin_rect.w } else { 50.0 };
        let box_h = if node.margin_rect.h > 0.0 { node.margin_rect.h } else { font_px * 1.2 };
        let mut full_path = ancestor_path.to_vec();
        full_path.push(box_idx);
        items.push(InlineItem {
            kind:      InlineItemKind::Atomic { path: full_path },
            advance:   box_w,
            ascent:    box_h,
            descent:   0.0,
            height:    box_h,
            is_space:  false,
            breakable: true,
        });
        return;
    }

    // ── Own text ──────────────────────────────────────────────────────────
    if !node.text.is_empty() {
        let start = *text_offset;
        tokenize_text(engine, &node.text, node.style.white_space, start, font_px, ascent, descent, line_h, box_idx, items, node.style.font_weight, node.style.font_style);
        runs.push(InlineRun { text_offset: start, length: node.text.len(), style: node.style.clone() });
        *text_offset += node.text.len();
    }

    // ── Recurse into children ─────────────────────────────────────────────
    let runs_before = runs.len();
    let mut child_path = ancestor_path.to_vec();
    child_path.push(box_idx);
    for (i, child) in node.children.iter().enumerate() {
        collect_items(engine, child, font_px, root_font_px, items, runs, text_offset, i, false, &child_path);
    }
    // CSS background-color is not inherited, but an inline element's background
    // must visually paint behind its descendant text runs.  Propagate this
    // element's background-color down to any child run that has none of its own.
    if node.style.background_color.a > 0 && !node.is_text_node() {
        for run in &mut runs[runs_before..] {
            if run.style.background_color.a == 0 {
                run.style.background_color = node.style.background_color;
            }
        }
    }
}

/// Split `text` at whitespace boundaries and emit word/space InlineItems.
/// For `white-space: pre`, `pre-wrap`, and `pre-line`, newlines (`\n`) produce
/// a forced `Break` item rather than being treated as a collapsible space.
fn tokenize_text(
    engine:      &LayoutEngine,
    text:        &str,
    white_space: WhiteSpace,
    base_offset: usize,
    font_px:     f32,
    ascent:      f32,
    descent:     f32,
    line_h:      f32,
    box_idx:     usize,
    items:       &mut Vec<InlineItem>,
    font_weight: FontWeight,
    font_style:  FontStyle,
) {
    if text.is_empty() { return; }

    let preserve_newlines = matches!(white_space, WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::PreLine);

    let bytes = text.as_bytes();
    let mut word_start = 0usize;
    let mut i = 0usize;

    while i <= bytes.len() {
        let at_end   = i == bytes.len();
        let is_nl    = !at_end && bytes[i] == b'\n' && preserve_newlines;
        let is_space = !at_end && !is_nl && bytes[i].is_ascii_whitespace();

        if (at_end || is_space || is_nl) && i > word_start {
            // Emit word
            let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
            let w = measure_text_width_weighted(&text[word_start..i], font_px, font_system, font_weight, font_style, engine.scale);
            items.push(InlineItem {
                kind:      InlineItemKind::Text {
                    text_start: base_offset + word_start,
                    text_len:   i - word_start,
                    box_idx,
                },
                advance:   w,
                ascent,
                descent,
                height:    line_h,
                is_space:  false,
                breakable: word_start > 0,
            });
        }

        if is_nl {
            // Newline in a pre-like context: forced line break.
            // The newline byte itself is represented as a 1-byte Text item with
            // zero advance so caret offsets stay in sync.
            items.push(InlineItem {
                kind:      InlineItemKind::Text {
                    text_start: base_offset + i,
                    text_len:   1,
                    box_idx,
                },
                advance:   0.0,
                ascent,
                descent,
                height:    line_h,
                is_space:  false,
                breakable: false,
            });
            items.push(InlineItem {
                kind:      InlineItemKind::Break,
                advance:   0.0, ascent, descent, height: line_h,
                is_space:  false, breakable: false,
            });
            i += 1;
            word_start = i;
            continue;
        }

        if is_space {
            // Emit one space item per space character so caret byte offsets stay in sync.
            // (Previously all consecutive spaces were collapsed to one rendered item,
            //  causing the caret to drift right while text stayed left.)
            let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
            let space_w = measure_text_width_weighted(" ", font_px, font_system, font_weight, font_style, engine.scale);
            // In white-space:pre / pre-wrap, spaces are significant (not collapsible).
            // Mark them as non-space so break_one_line doesn't strip leading whitespace,
            // and non-breakable in pre mode (only \n breaks lines).
            let preserve_spaces = matches!(white_space, WhiteSpace::Pre | WhiteSpace::PreWrap);
            items.push(InlineItem {
                kind:      InlineItemKind::Text {
                    text_start: base_offset + i,
                    text_len:   1,
                    box_idx,
                },
                advance:   space_w,
                ascent,
                descent,
                height:    line_h,
                is_space:  !preserve_spaces,
                breakable: !matches!(white_space, WhiteSpace::Pre),
            });
            i += 1;       // consume exactly one space byte
            word_start = i;
            continue;
        }

        i += 1;
    }
}

// ─── Per-line breaker ─────────────────────────────────────────────────────────

/// Break one line from `items[start_idx..]` fitting in `avail_w`.
///
/// Returns `(line_end, next_start, was_forced_break)`:
/// - `items[start_idx..line_end]` are on the current line.
/// - `items[next_start..]` remain for subsequent lines.
/// - `was_forced_break` is true if a `<br>` item terminated the line.
fn break_one_line(items: &[InlineItem], start_idx: usize, avail_w: f32) -> (usize, usize, bool) {
    // Skip leading spaces
    let mut i = start_idx;
    while i < items.len() && items[i].is_space { i += 1; }
    let line_start = i;

    let mut cur_w    = 0.0f32;
    let mut last_bp: Option<usize> = None;  // items index of last break opportunity

    while i < items.len() {
        let item = &items[i];

        // Forced break: terminate line here, consume the break item
        if matches!(item.kind, InlineItemKind::Break) {
            return (i, i + 1, true);
        }

        // Track last break opportunity BEFORE overflow check (matches original logic)
        if item.breakable {
            last_bp = Some(i);
        }

        let new_w = cur_w + item.advance;
        if new_w > avail_w && i > line_start {
            if let Some(bp) = last_bp {
                // Trim trailing spaces from line
                let mut line_end = bp;
                while line_end > line_start && items[line_end - 1].is_space {
                    line_end -= 1;
                }
                // Skip leading spaces at start of next line
                let mut next = bp;
                while next < items.len() && items[next].is_space { next += 1; }
                return (line_end, next, false);
            } else {
                // No break point: force break before current item
                return (i, i, false);
            }
        }

        cur_w += item.advance;
        i += 1;
    }

    // Consumed all items
    (i, i, false)
}

// ─── Legacy full-document line breaker (kept for compatibility) ───────────────

/// Greedy line-breaker. Returns a vec of LineBuild, each holding its items
/// plus computed metrics (height, ascent, descent) and text byte-range.
/// NOTE: Does not handle floats; use `layout_inline_block` with a `FloatContext`
/// for float-aware layout.
pub fn break_lines(items: &[InlineItem], max_w: f32) -> Vec<LineBuild> {
    let mut lines: Vec<LineBuild> = Vec::new();
    let mut idx = 0;
    while idx < items.len() {
        let (line_end, next, was_break) = break_one_line(items, idx, max_w);
        if line_end > idx {
            push_line_from_slice(&mut lines, &items[idx..line_end]);
        } else if !was_break && line_end == idx {
            // No progress and no break — guard against infinite loop
            idx += 1;
            continue;
        }
        if was_break {
            // Emit empty break line
            push_line_from_slice(&mut lines, &[]);
        }
        idx = next;
    }
    lines
}

// ─── Line buffer ──────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct LineBuild {
    pub items:      Vec<InlineItem>,
    pub width:      f32,
    pub height:     f32,
    pub ascent:     f32,
    pub descent:    f32,
    /// Byte offset of the first character on this line in the flat text string.
    pub text_start: usize,
    /// Total byte length of text on this line.
    pub text_len:   usize,
}

fn push_line_from_slice(lines: &mut Vec<LineBuild>, slice: &[InlineItem]) {
    let mut line = LineBuild::default();
    let mut text_start = usize::MAX;
    let mut text_end   = 0usize;

    for item in slice {
        line.items.push(item.clone());
        line.width   += item.advance;
        if item.ascent  > line.ascent  { line.ascent  = item.ascent;  }
        if item.descent > line.descent { line.descent = item.descent; }
        if item.height  > line.height  { line.height  = item.height;  }

        if let InlineItemKind::Text { text_start: ts, text_len: tl, .. } = &item.kind {
            if *ts < text_start { text_start = *ts; }
            let end = ts + tl;
            if end > text_end { text_end = end; }
        }
    }

    if line.height == 0.0 {
        line.height = (line.ascent + line.descent).max(16.0);
    }

    line.text_start = if text_start == usize::MAX { 0 } else { text_start };
    line.text_len   = if text_end > line.text_start { text_end - line.text_start } else { 0 };

    lines.push(line);
}

// ─── Font resolution helpers ───────────────────────────────────────────────────

/// Resolve the first family name from a CSS `font-family` value (comma-separated,
/// possibly with quoted names) into a cosmic-text `Family`.
///
/// The returned `Family::Name` borrows directly from `raw` (zero-copy for named fonts).
/// Generic keywords (`sans-serif`, `serif`, …) return the corresponding enum variant.
pub(crate) fn css_family_to_cosmic(raw: &str) -> Family<'_> {
    let first = extract_first_css_family(raw);
    match first {
        "serif"      => Family::Serif,
        "monospace"  => Family::Monospace,
        "cursive"    => Family::Cursive,
        "fantasy"    => Family::Fantasy,
        ""           => Family::SansSerif,
        name         => Family::Name(name),
    }
}

/// Extract the first font-family name as a `&str` slice into `raw`.
/// Strips surrounding quotes for quoted names.
fn extract_first_css_family(raw: &str) -> &str {
    let raw = raw.trim();
    if raw.is_empty() { return ""; }
    // Quoted name: `"Times New Roman"` or `'Foo'`
    if raw.starts_with('"') {
        return raw[1..].split('"').next().unwrap_or("").trim();
    }
    if raw.starts_with('\'') {
        return raw[1..].split('\'').next().unwrap_or("").trim();
    }
    // Unquoted: up to the first comma
    raw.split(',').next().unwrap_or(raw).trim()
}

/// Map a `font_stretch` percentage (100.0 = normal) to a cosmic-text `Stretch` variant.
pub(crate) fn stretch_from_percent(pct: f32) -> Stretch {
    // CSS spec breakpoints (midpoints between defined values):
    if      pct <= 56.25  { Stretch::UltraCondensed }
    else if pct <= 68.75  { Stretch::ExtraCondensed }
    else if pct <= 81.25  { Stretch::Condensed }
    else if pct <= 93.75  { Stretch::SemiCondensed }
    else if pct <= 106.25 { Stretch::Normal }
    else if pct <= 118.75 { Stretch::SemiExpanded }
    else if pct <= 137.5  { Stretch::Expanded }
    else if pct <= 175.0  { Stretch::ExtraExpanded }
    else                  { Stretch::UltraExpanded }
}

/// Build a cosmic-text `Weight` from a `FontWeight` enum, optionally overridden
/// by a `font-variation-settings` `"wght"` axis.
pub(crate) fn weight_from_style(weight: FontWeight, var: &[(String, f32)]) -> Weight {
    // font-variation-settings 'wght' overrides the logical font-weight.
    for (tag, val) in var {
        if tag == "wght" { return Weight(*val as u16); }
    }
    Weight(weight.value())
}

// ─── Text measurement ─────────────────────────────────────────────────────────

pub fn measure_text_width(text: &str, font_px: f32, font_system: Option<&mut cosmic_text::FontSystem>) -> f32 {
    measure_text_width_scaled(text, font_px, font_system, 1.0)
}

pub fn measure_text_width_scaled(text: &str, font_px: f32, font_system: Option<&mut cosmic_text::FontSystem>, scale: f32) -> f32 {
    if let Some(fs) = font_system {
        measure_text_width_fs(fs, text, font_px, scale)
    } else {
        measure_text_width_ts(text, font_px, 8)
    }
}

pub fn measure_text_width_weighted(
    text:        &str,
    font_px:     f32,
    font_system: Option<&mut cosmic_text::FontSystem>,
    weight:      FontWeight,
    style:       FontStyle,
    scale:       f32,
) -> f32 {
    if let Some(fs) = font_system {
        let ct_weight = Weight(weight.value());
        let ct_style  = match style {
            FontStyle::Italic  => CTextStyle::Italic,
            FontStyle::Oblique => CTextStyle::Oblique,
            FontStyle::Normal  => CTextStyle::Normal,
        };
        measure_text_width_fs_attrs(fs, text, font_px, ct_weight, ct_style, scale)
    } else {
        let w = measure_text_width_ts(text, font_px, 8);
        // Bold/semi-bold text is typically ~15% wider than normal weight.
        // Apply a correction factor so layout content-width better matches the
        // actual rendered width, preventing text from overflowing the background.
        if weight.is_bold() { w * 1.15 } else { w }
    }
}

pub fn measure_text_width_fs(fs: &mut cosmic_text::FontSystem, text: &str, font_px: f32, scale: f32) -> f32 {
    measure_text_width_fs_attrs(fs, text, font_px, Weight::NORMAL, CTextStyle::Normal, scale)
}

/// Measure text width by shaping at physical pixel size (font_px * scale) and
/// scaling the result back to logical pixels.  This matches the renderer which
/// also shapes at physical size, ensuring line-breaking decisions agree with
/// the actual rendered glyph widths.
pub fn measure_text_width_fs_attrs(
    fs:     &mut cosmic_text::FontSystem,
    text:   &str,
    font_px: f32,
    weight: Weight,
    style:  CTextStyle,
    scale:  f32,
) -> f32 {
    if text.is_empty() { return 0.0; }
    // Shape at physical pixel size to match renderer glyph widths, then
    // convert back to logical pixels.  At scale=1 this is a no-op.
    let phys_px = font_px * scale.max(1.0);
    let inv     = if scale > 1.0 { 1.0 / scale } else { 1.0 };
    let metrics = Metrics::new(phys_px, phys_px * 1.2);
    let mut buffer = Buffer::new(fs, metrics);
    let attrs = Attrs::new().weight(weight).style(style);
    buffer.set_text(fs, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(fs, false);

    let mut max_w = 0.0f32;
    for run in buffer.layout_runs() {
        if run.line_w > max_w { max_w = run.line_w; }
    }
    max_w * inv
}

pub fn measure_text_width_ts(text: &str, font_px: f32, tab_size: i32) -> f32 {
    let char_w  = font_px * 0.55;
    let space_w = char_w * 0.35;
    let ts = (tab_size.max(1)) as f32;
    text.chars().map(|c| {
        if c == '\t'                         { space_w * ts }
        else if "iIlj1!|:;,.'`".contains(c) { char_w * 0.45 }
        else if "mwMW".contains(c)           { char_w * 1.20 }
        else if c == ' '                     { space_w }
        else if c.is_ascii()                 { char_w }
        else                                 { font_px * 1.0 }  // emoji / CJK: full square width
    }).sum()
}

pub fn approx_font_metrics(font_px: f32, _fs: Option<&mut cosmic_text::FontSystem>) -> (f32, f32) {
    (font_px * 0.80, font_px * 0.20)
}

// ─── Accurate per-character x positions using cosmic_text ────────────────────

/// Populate `line.char_x` with real glyph x positions (relative to `line.x`).
///
/// Each entry `char_x[i]` is the visual x of the caret at byte offset
/// `line.text_start + i` within `flat`.  Uses the same shaping as the renderer
/// (Basic for ASCII, Advanced otherwise) so click-to-caret and caret rendering
/// agree exactly with the rendered text positions.
pub fn fill_char_x_for_line(
    fs:    &mut cosmic_text::FontSystem,
    flat:  &str,
    runs:  &[InlineRun],
    line:  &mut LayoutLine,
    scale: f32,
) {
    let line_start = line.text_start;
    let line_end   = (line.text_start + line.text_length).min(flat.len());
    let range_len  = line_end.saturating_sub(line_start);
    if range_len == 0 { return; }

    // One entry per byte boundary plus one for end-of-line.
    let mut positions = vec![f32::NAN; range_len + 1];

    let mut cursor_x = 0.0f32; // x relative to line.x, advances across runs

    for run in runs {
        let seg_s = line_start.max(run.text_offset);
        let seg_e = line_end.min(run.text_offset + run.length);
        if seg_s >= seg_e { continue; }

        // Snap to char boundaries.
        let mut s = seg_s;
        while s < flat.len() && !flat.is_char_boundary(s) { s += 1; }
        let mut e = seg_e;
        while e > 0 && !flat.is_char_boundary(e) { e -= 1; }
        if s >= e { continue; }

        let seg_text = &flat[s..e];
        let font_px  = run.style.font_size_px(16.0, 16.0);
        let ct_w = weight_from_style(run.style.font_weight, &run.style.font_variation_settings);
        let ct_s = match run.style.font_style {
            FontStyle::Italic  => CTextStyle::Italic,
            FontStyle::Oblique => CTextStyle::Oblique,
            FontStyle::Normal  => CTextStyle::Normal,
        };
        let ct_stretch = stretch_from_percent(run.style.font_stretch);

        // Shape at physical pixel size (matching the renderer) so that char_x
        // positions agree with what is actually drawn on screen. Positions are
        // then converted back to logical pixels by dividing by scale.
        let phys_px  = font_px * scale;
        let metrics = Metrics::new(phys_px, phys_px * 1.2);
        let mut buf = Buffer::new(fs, metrics);
        let family  = css_family_to_cosmic(&run.style.font_family);
        let attrs   = Attrs::new().weight(ct_w).style(ct_s).stretch(ct_stretch).family(family);
        // Always use Advanced shaping: Basic reports word-relative byte offsets
        // (glyph.start=0..4 for "world" in "Hello world") rather than buffer-relative
        // (6..10). Only Advanced gives correct offsets needed to populate char_x.
        let shaping = Shaping::Advanced;
        buf.set_text(fs, seg_text, &attrs, shaping, None);
        buf.shape_until_scroll(fs, false);

        // Glyphs are in physical pixels; divide by scale to get logical pixels.
        let inv_scale = if scale > 0.0 { 1.0 / scale } else { 1.0 };
        let mut seg_advance = 0.0f32;
        for lr in buf.layout_runs() {
            for glyph in lr.glyphs {
                // glyph.start / .end are byte offsets within seg_text.
                let abs_s = s + glyph.start;
                let abs_e = s + glyph.end;
                let i_s   = abs_s.saturating_sub(line_start);
                let i_e   = abs_e.saturating_sub(line_start).min(positions.len() - 1);
                let x0    = cursor_x + glyph.x * inv_scale;
                let x1    = cursor_x + (glyph.x + glyph.w) * inv_scale;
                // Distribute position linearly across all byte boundaries in this glyph.
                // Single-char glyphs: sets positions[i_s] and positions[i_e] exactly.
                // Multi-char glyphs (Shaping::Basic word glyphs, ligatures): interpolates
                // so that each character boundary gets a proportional x position rather
                // than leaving intermediate bytes as NaN and forward-filling them all to
                // the same value (which would make the whole word appear zero-width).
                let span = (i_e.saturating_sub(i_s)).max(1);
                for k in 0..=span {
                    let idx = i_s + k;
                    if idx < positions.len() && positions[idx].is_nan() {
                        positions[idx] = x0 + (x1 - x0) * k as f32 / span as f32;
                    }
                }
                let right = x1 - cursor_x;
                if right > seg_advance { seg_advance = right; }
            }
            let lw = lr.line_w * inv_scale;
            if lw > seg_advance { seg_advance = lw; }
        }

        // Advance cursor_x by the segment's actual advance + extra word spacing.
        // Mirrors what the renderer does: actual_advance + n_spaces*(word_s+extra).
        let word_s = run.style.word_spacing.resolve(font_px, 0.0, 16.0);
        let extra  = line.extra_space_per_word;
        let n_spc  = seg_text.chars().filter(|&c| c == ' ').count() as f32;
        cursor_x += seg_advance + n_spc * (word_s + extra);
    }

    // End-of-line position.
    positions[range_len] = cursor_x;

    // Forward-fill NaN gaps (intermediate bytes of multi-byte / ligature glyphs).
    let mut last = 0.0f32;
    for p in positions.iter_mut() {
        if p.is_nan() { *p = last; } else { last = *p; }
    }

    line.char_x = positions;
}

// ─── Collect flat text (same traversal as collect_items) ─────────────────────
/// Used by the renderer to map text_start offsets back to characters.
pub fn collect_flat_text(node: &HtmlBox) -> String {
    let mut out = String::new();
    collect_flat_text_inner(node, &mut out, true);
    out
}

fn collect_flat_text_inner(node: &HtmlBox, out: &mut String, is_root: bool) {
    if node.is_text_node() {
        out.push_str(&node.text);
        return;
    }
    if matches!(node.style.display, Display::None) { return; }
    if node.tag == "br" { return; }
    // Floats are emitted as Float items in collect_items and their text is not
    // counted in text_offset — skip them here to keep byte offsets in sync.
    // Exception: when called as root (rendering the float itself), include its text.
    if !is_root && !matches!(node.style.float, crate::types::Float::None) { return; }
    // Atomic inline-blocks are emitted as Atomic items by the parent; their internal
    // text is NOT part of the parent's flat-text string. However when we are rendering
    // the inline-block itself (is_root=true) we DO want its own text content.
    if !is_root && matches!(node.style.display,
        Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) { return; }
    if !node.text.is_empty() {
        out.push_str(&node.text);
    }
    for child in &node.children {
        // Skip out-of-flow children — they don't contribute to this box's flat text.
        // (collect_flat_text is called separately on each positioned box for its own content.)
        if matches!(child.style.position, Position::Absolute | Position::Fixed) { continue; }
        collect_flat_text_inner(child, out, false);
    }
}

// ─── Recursive inline-block pre-layout ───────────────────────────────────────

/// Recursively walk inline children and pre-layout any nested inline-block
/// elements (e.g. `<input>` inside `<label>`).  This ensures that when
/// `collect_items` encounters an `InlineBlock`, its `margin_rect` is non-zero
/// so the item gets the correct advance width and ascent.
fn prelayout_nested_inline_blocks(
    engine:       &LayoutEngine,
    node:         &mut HtmlBox,
    content_w:    f32,
    font_px:      f32,
    root_font_px: f32,
) {
    for ci in 0..node.children.len() {
        if matches!(node.children[ci].style.display, Display::None) { continue; }
        if matches!(node.children[ci].style.position, Position::Absolute | Position::Fixed) { continue; }
        if matches!(node.children[ci].style.display,
                    Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) {
            // When called from the top-level (block container), direct inline-block
            // children are already handled by the step 0 loop in layout_inline_block().
            // Only lay out here in the recursive case (node is an inline wrapper,
            // e.g. span > a > img where this function was called on the <a>).
            if matches!(node.style.display, Display::Inline) {
                engine.layout_box(
                    &mut node.children[ci],
                    content_w, 0.0, 0.0, font_px, root_font_px,
                );
                if node.children[ci].style.width.is_auto() {
                    let max_line_w = node.children[ci].line_cache.iter()
                        .map(|l| l.width).fold(0.0_f32, f32::max);
                    let intrinsic_w = if max_line_w > 0.0 { max_line_w }
                        else { compute_intrinsic_width(&node.children[ci]) };
                    let gc = &node.children[ci];
                    let shrink_w = intrinsic_w
                        + gc.resolved_pad_left + gc.resolved_pad_right
                        + gc.resolved_border_left + gc.resolved_border_right
                        + gc.resolved_margin_left + gc.resolved_margin_right;
                    if shrink_w < content_w {
                        engine.layout_box(
                            &mut node.children[ci],
                            shrink_w, 0.0, 0.0, font_px, root_font_px,
                        );
                    }
                }
            }
            continue;
        }
        // Inline children: recurse to find nested inline-blocks.
        if matches!(node.children[ci].style.display, Display::Inline) {
            let child_font_px = node.children[ci].style.font_size_px(font_px, root_font_px);
            // Pre-layout any inline-block grandchildren inside this inline child.
            for gci in 0..node.children[ci].children.len() {
                let grandchild_display = node.children[ci].children[gci].style.display;
                if matches!(grandchild_display, Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) {
                    engine.layout_box(
                        &mut node.children[ci].children[gci],
                        content_w, 0.0, 0.0, child_font_px, root_font_px,
                    );
                    // Shrink-to-fit for auto-width nested inline-blocks
                    if node.children[ci].children[gci].style.width.is_auto() {
                        let max_line_w = node.children[ci].children[gci].line_cache.iter()
                            .map(|l| l.width).fold(0.0_f32, f32::max);
                        let intrinsic_w = if max_line_w > 0.0 { max_line_w }
                            else { compute_intrinsic_width(&node.children[ci].children[gci]) };
                        let gc = &node.children[ci].children[gci];
                        let shrink_w = intrinsic_w
                            + gc.resolved_pad_left + gc.resolved_pad_right
                            + gc.resolved_border_left + gc.resolved_border_right
                            + gc.resolved_margin_left + gc.resolved_margin_right;
                        if shrink_w < content_w {
                            engine.layout_box(
                                &mut node.children[ci].children[gci],
                                shrink_w, 0.0, 0.0, child_font_px, root_font_px,
                            );
                        }
                    }
                } else if matches!(grandchild_display, Display::Inline) {
                    // One more level: recurse for deeper nesting.
                    prelayout_nested_inline_blocks(
                        engine, &mut node.children[ci].children[gci],
                        content_w, child_font_px, root_font_px,
                    );
                }
            }
        }
    }
}
