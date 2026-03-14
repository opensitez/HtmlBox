use crate::types::*;
use cosmic_text::{Attrs, Buffer, Metrics, Shaping};
use crate::layout::{LayoutEngine, ResolvedBox, FloatContext};
use crate::layout::block::collapse_two;
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
    float_ctx:    Option<&FloatContext>,
) -> f32 {
    let raw_w = match rbox.content_width {
        Some(w) => w,
        None    => (containing_w - rbox.h_space()).max(0.0),
    };
    // Apply min/max-width
    let min_w = node.style.min_width.resolve(font_px, containing_w, root_font_px);
    let max_w = if node.style.max_width.is_none() { f32::MAX }
                else { node.style.max_width.resolve(font_px, containing_w, root_font_px) };
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

    // ── 0. Pre-layout atomic inline-block children so their sizes are known ─────
    //    Mirrors C++ LayoutInlines(): LayoutBox(*run.atomicBox, …) before line-breaking
    for ci in 0..node.children.len() {
        if matches!(node.children[ci].style.display,
                    Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) {
            engine.layout_box(&mut node.children[ci], content_w,
                               0.0, 0.0, font_px, root_font_px);
        }
    }

    // ── 1. Measure ::before / ::after pseudo-element widths ───────────────────
    let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
    let before_w = if !node.style.before_content.is_empty() {
        measure_text_width(&node.style.before_content, font_px, font_system)
    } else { 0.0 };
    // Need to re-get because it might have been consumed (borrow checker)
    let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
    let after_w = if !node.style.after_content.is_empty() {
        measure_text_width(&node.style.after_content, font_px, font_system)
    } else { 0.0 };

    // ── 2. Collect flat inline items from all inline children ─────────────────
    let mut text_offset = 0usize;
    let mut items: Vec<InlineItem> = Vec::new();
    let mut runs:  Vec<InlineRun>  = Vec::new();
    for (i, child) in node.children.iter().enumerate() {
        if matches!(child.style.display, Display::None) { continue; }
        collect_items(engine, child, font_px, root_font_px, &mut items, &mut runs, &mut text_offset, i);
    }
    // Also collect from own text (text directly inside element)
    if !node.text.is_empty() && !node.is_text_node() {
        let mut tmp_node = HtmlBox::new("#text");
        tmp_node.text = node.text.clone();
        tmp_node.style = node.style.clone();
        collect_items(engine, &tmp_node, font_px, root_font_px, &mut items, &mut runs, &mut text_offset, 0);
    }

    // ── 3. Save old lines for early-stop optimization ─────────────────────────
    let old_lines: Vec<LayoutLine> = std::mem::take(&mut node.line_cache);

    if items.is_empty() {
        // Nothing to lay out
        let content_h = match rbox.content_height { Some(h) => h, None => 0.0 };
        set_box_rects(node, content_x, content_y, content_w, content_h,
                      rbox, margin_left, margin_right);
        node.inline_runs = runs;
        return node.margin_rect.h;
    }

    // ── 4. Line-by-line layout (float-aware) ──────────────────────────────────
    let text_indent = node.style.text_indent.resolve(font_px, content_w, root_font_px);
    let is_rtl = node.style.direction == Direction::RTL;

    let mut cursor_y     = content_y;
    let mut item_idx     = 0usize;
    let mut line_cache:  Vec<LayoutLine>           = Vec::new();
    let mut atomic_pos:  Vec<(usize, f32, f32)>    = Vec::new(); // (child_idx, x, y)
    let mut old_line_idx = 0usize;
    let mut ends_with_break = false;
    let mut loop_guard   = 0usize;

    while item_idx < items.len() {
        loop_guard += 1;
        if loop_guard > 10000 { break; }
        let is_first_line = line_cache.is_empty();

        // Query float context for available horizontal band at this Y
        let est_line_h = font_px * 1.2;
        let (mut fc_left, mut fc_right) = (0.0f32, content_w);
        if let Some(fc) = float_ctx {
            fc.available_width(cursor_y - content_y, est_line_h, content_w,
                               &mut fc_left, &mut fc_right);
        }

        // Apply text-indent and ::before on first line
        if is_first_line {
            fc_left += text_indent;
            fc_left += before_w;
        }

        let avail_w = (fc_right - fc_left).max(20.0);

        // Break items for this line
        let (line_end, next_start, was_break) =
            break_one_line(&items, item_idx, avail_w);

        ends_with_break = was_break;

        if line_end == item_idx && !was_break {
            // No progress and no forced break — should not happen but guard anyway
            break;
        }

        let line_items = &items[item_idx..line_end];

        // Compute line metrics from items
        let (mut line_h, mut line_asc, mut line_desc) =
            measure_metrics(line_items, font_px);

        // CSS line-height: apply only when explicitly set (not auto)
        if !node.style.line_height.is_auto() {
            let lh_val = node.style.line_height.resolve(font_px, 0.0, root_font_px);
            if lh_val > line_h {
                let half = ((lh_val - line_h) / 2.0).floor();
                line_asc  += half;
                line_desc += (lh_val - line_h) - half;
                line_h     = lh_val;
            }
        }

        // Measure content width (omit trailing forced-break item width)
        let content_line_w: f32 = line_items.iter()
            .filter(|it| !matches!(it.kind, InlineItemKind::Break))
            .map(|it| it.advance)
            .sum();

        // Resolve text-align Start/End based on direction
        let effective_align = match node.style.text_align {
            TextAlign::Start => if is_rtl { TextAlign::Right } else { TextAlign::Left },
            TextAlign::End   => if is_rtl { TextAlign::Left  } else { TextAlign::Right },
            a => a,
        };

        // Justify: compute extra space per word gap
        let extra_per_gap = if effective_align == TextAlign::Justify && !was_break && next_start < items.len() {
            let gaps = line_items.iter().filter(|it| it.is_space).count() as f32;
            if gaps > 0.0 { ((avail_w - content_line_w) / gaps).max(0.0) } else { 0.0 }
        } else { 0.0 };

        // X offset for text alignment
        let mut line_x = content_x + fc_left + match effective_align {
            TextAlign::Right  => (avail_w - content_line_w).max(0.0),
            TextAlign::Center => ((avail_w - content_line_w) / 2.0).max(0.0),
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

        // Compute text range for this line (needed for early-stop and LayoutLine)
        let text_s = line_items.iter().filter_map(|it| {
            if let InlineItemKind::Text { text_start, .. } = &it.kind { Some(*text_start) } else { None }
        }).min().unwrap_or(0);
        let text_e = line_items.iter().filter_map(|it| {
            if let InlineItemKind::Text { text_start, text_len, .. } = &it.kind {
                Some(text_start + text_len)
            } else { None }
        }).max().unwrap_or(text_s);

        // Early-stop: if matching an old cached line with same breaks at same Y
        // (only when no floats involved)
        if float_ctx.is_none() && old_line_idx > 0 && old_line_idx < old_lines.len() {
            let ol = &old_lines[old_line_idx];
            if ol.text_start == text_s
                && ol.text_length == text_e.saturating_sub(text_s)
                && ol.y == cursor_y
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
                let min_h = node.style.min_height.resolve(font_px, 0.0, root_font_px);
                let max_h = if node.style.max_height.is_none() { f32::MAX }
                            else { node.style.max_height.resolve(font_px, 0.0, root_font_px) };
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
                if let InlineItemKind::Atomic { child_idx } = &item.kind {
                    // Vertical: bottom-align with line baseline
                    let box_h = if *child_idx < node.children.len() {
                        node.children[*child_idx].margin_rect.h
                    } else { item.height };
                    let ay = cursor_y + (line_h - box_h).max(0.0);
                    atomic_pos.push((*child_idx, cur_x, ay));
                }
                cur_x += item.advance;
            }
        }

        // Build LayoutLine
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
            visual_segments: Vec::new(),
        };

        // Resolve BiDi visual segments for this line
        let para_dir = node.style.direction;
        resolve_bidi_line(&collect_flat_text(node), &mut ll, para_dir);

        line_cache.push(ll);
        cursor_y += line_h;
        item_idx = next_start;
        old_line_idx += 1;
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
            extra_space_per_word: 0.0,
            visual_segments: Vec::new(),
        });
        cursor_y += font_px * 1.2;
    }

    // ── 5. Compute content height ──────────────────────────────────────────────
    let raw_h = (cursor_y - content_y).max(0.0);
    let min_h = node.style.min_height.resolve(font_px, 0.0, root_font_px);
    let max_h = if node.style.max_height.is_none() { f32::MAX }
                else { node.style.max_height.resolve(font_px, 0.0, root_font_px) };
    let content_h = match rbox.content_height { Some(h) => h, None => raw_h };
    let content_h = content_h.max(min_h).min(max_h);

    set_box_rects(node, content_x, content_y, content_w, content_h,
                  rbox, margin_left, margin_right);
    node.line_cache  = line_cache;
    node.inline_runs = runs;

    // ── 6. Position atomic inline-block children ──────────────────────────────
    //    Separate pass after all lines are built (mirrors C++ post-loop pass)
    for (ci, ax, ay) in atomic_pos {
        if ci < node.children.len() {
            // Shift child rects to final position
            let dx = ax - node.children[ci].margin_rect.x;
            let dy = ay - node.children[ci].margin_rect.y;
            crate::layout::shift_rects(&mut node.children[ci], dx, dy);
        }
    }

    node.margin_rect.h
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
    node.margin_rect = Rect::new(
        node.border_rect.x - margin_left,
        node.border_rect.y - rbox.margin_top,
        node.border_rect.w + margin_left + margin_right,
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
    /// An inline-block child (child_idx into parent's children Vec).
    Atomic { child_idx: usize },
    /// Forced line break (<br>).
    Break,
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
pub fn collect_items(
    engine:         &LayoutEngine,
    node:           &HtmlBox,
    parent_font_px: f32,
    root_font_px:   f32,
    items:          &mut Vec<InlineItem>,
    runs:           &mut Vec<InlineRun>,
    text_offset:    &mut usize,
    box_idx:        usize,
) {
    if matches!(node.style.display, Display::None) { return; }

    let font_px = node.style.font_size_px(parent_font_px, root_font_px);
    let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
    let (ascent, descent) = approx_font_metrics(font_px, font_system);
    let line_h = node.style.line_height.resolve(font_px, 0.0, root_font_px)
                     .max(font_px * 1.2);

    // ── Text node ─────────────────────────────────────────────────────────
    if node.is_text_node() {
        if !node.text.is_empty() {
            let start = *text_offset;
            tokenize_text(engine, &node.text, start, font_px, ascent, descent, line_h, box_idx, items);
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
    if matches!(node.style.display, Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) {
        // Use the pre-laid-out margin-rect width (set by the pre-layout pass)
        let box_w = if node.margin_rect.w > 0.0 { node.margin_rect.w } else { 50.0 };
        let box_h = if node.margin_rect.h > 0.0 { node.margin_rect.h } else { font_px * 1.2 };
        items.push(InlineItem {
            kind:      InlineItemKind::Atomic { child_idx: box_idx },
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
        tokenize_text(engine, &node.text, start, font_px, ascent, descent, line_h, box_idx, items);
        runs.push(InlineRun { text_offset: start, length: node.text.len(), style: node.style.clone() });
        *text_offset += node.text.len();
    }

    // ── Recurse into children ─────────────────────────────────────────────
    for (i, child) in node.children.iter().enumerate() {
        collect_items(engine, child, font_px, root_font_px, items, runs, text_offset, i);
    }
}

/// Split `text` at whitespace boundaries and emit word/space InlineItems.
fn tokenize_text(
    engine:      &LayoutEngine,
    text:        &str,
    base_offset: usize,
    font_px:     f32,
    ascent:      f32,
    descent:     f32,
    line_h:      f32,
    box_idx:     usize,
    items:       &mut Vec<InlineItem>,
) {
    if text.is_empty() { return; }
    let bytes = text.as_bytes();
    let mut word_start = 0usize;
    let mut i = 0usize;

    while i <= bytes.len() {
        let at_end   = i == bytes.len();
        let is_space = !at_end && bytes[i].is_ascii_whitespace();

        if (at_end || is_space) && i > word_start {
            // Emit word
            let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
            let w = measure_text_width(&text[word_start..i], font_px, font_system);
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

        if is_space {
            // Emit one space item, collapse all consecutive spaces
            let font_system = unsafe { engine.font_system.map(|fs| &mut *fs) };
            let space_w = measure_text_width(" ", font_px, font_system);
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
                is_space:  true,
                breakable: true,
            });
            // Skip all consecutive whitespace
            while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
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

// ─── Text measurement ─────────────────────────────────────────────────────────

pub fn measure_text_width(text: &str, font_px: f32, font_system: Option<&mut cosmic_text::FontSystem>) -> f32 {
    if let Some(fs) = font_system {
        measure_text_width_fs(fs, text, font_px)
    } else {
        measure_text_width_ts(text, font_px, 8)
    }
}

pub fn measure_text_width_fs(fs: &mut cosmic_text::FontSystem, text: &str, font_px: f32) -> f32 {
    if text.is_empty() { return 0.0; }
    // Use a large enough height to avoid vertical wrapping if not desired here.
    let metrics = Metrics::new(font_px, font_px * 1.2);
    let mut buffer = Buffer::new(fs, metrics);
    buffer.set_text(fs, text, Attrs::new(), Shaping::Advanced);
    // Since we want the natural width, we don't set a width limit.
    buffer.shape_until_scroll(fs, false);
    
    let mut max_w = 0.0f32;
    for run in buffer.layout_runs() {
        if run.line_w > max_w { max_w = run.line_w; }
    }
    max_w
}

pub fn measure_text_width_ts(text: &str, font_px: f32, tab_size: i32) -> f32 {
    let char_w  = font_px * 0.55;
    let space_w = char_w * 0.35;
    let ts = (tab_size.max(1)) as f32;
    text.chars().map(|c| {
        if c == '\t'                     { space_w * ts }
        else if "iIlj1!|:;,.'`".contains(c) { char_w * 0.45 }
        else if "mwMW".contains(c)       { char_w * 1.20 }
        else if c == ' '                 { space_w }
        else if c.is_ascii()             { char_w }
        else                             { char_w }        // CJK ≈ same for now
    }).sum()
}

pub fn approx_font_metrics(font_px: f32, _fs: Option<&mut cosmic_text::FontSystem>) -> (f32, f32) {
    (font_px * 0.80, font_px * 0.20)
}

// ─── Collect flat text (same traversal as collect_items) ─────────────────────
/// Used by the renderer to map text_start offsets back to characters.
pub fn collect_flat_text(node: &HtmlBox) -> String {
    let mut out = String::new();
    collect_flat_text_inner(node, &mut out);
    out
}

fn collect_flat_text_inner(node: &HtmlBox, out: &mut String) {
    if node.is_text_node() {
        out.push_str(&node.text);
        return;
    }
    if matches!(node.style.display, Display::None) { return; }
    if node.tag == "br" { return; }
    if !node.text.is_empty() {
        out.push_str(&node.text);
    }
    for child in &node.children {
        collect_flat_text_inner(child, out);
    }
}
