//! Hit testing: map screen coordinates ↔ text offsets.
//! Ported from C++ HitTest.cpp.

use crate::types::*;
use crate::layout::inline_layout::collect_flat_text;

// ─── Character-width approximation ───────────────────────────────────────────
// Matches the renderer's approx_text_width_ls — same coefficients.

fn approx_char_width(ch: char, font_px: f32) -> f32 {
    let base = font_px * 0.55;
    if "iIlj1!|:;,.'`".contains(ch) { base * 0.45 }
    else if "mwMW".contains(ch)      { base * 1.20 }
    else if ch == ' '                 { base * 0.35 }
    else                              { base }
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) { idx -= 1; }
    idx
}

// ─── Measure a range [start, end) across inline runs ─────────────────────────

fn measure_run_range(
    text:           &str,
    runs:           &[InlineRun],
    start:          usize,
    end:            usize,
    extra_per_word: f32,
) -> f32 {
    if start >= end { return 0.0; }
    let mut w = 0.0f32;
    for run in runs {
        let seg_start = start.max(run.text_offset);
        let seg_end   = end.min(run.text_offset + run.length);
        if seg_start >= seg_end { continue; }

        let font_px  = run.style.font_size_px(16.0, 16.0);
        let letter_s = run.style.letter_spacing.resolve(font_px, 0.0, 16.0);
        let word_s   = run.style.word_spacing.resolve(font_px, 0.0, 16.0);

        let s = floor_char_boundary(text, seg_start.min(text.len()));
        let e = floor_char_boundary(text, seg_end.min(text.len()));
        if s >= e { continue; }

        for ch in text[s..e].chars() {
            if ch == '\n' || ch == '\r' { continue; }
            w += approx_char_width(ch, font_px) + letter_s;
            if ch == ' ' { w += word_s + extra_per_word; }
        }
    }
    w
}

// ─── Caret X from offset ──────────────────────────────────────────────────────

/// Returns the X pixel position of the caret at `offset` within `line`.
/// `text` is the flat text of the containing box.
pub fn get_caret_x(
    text:   &str,
    runs:   &[InlineRun],
    line:   &LayoutLine,
    offset: usize,
) -> f32 {
    let line_end = line.text_start + line.text_length;
    let offset   = offset.min(line_end);

    // Fast path: use pre-computed char_x positions (shaped at the same physical
    // pixel size as the renderer, giving pixel-accurate caret placement).
    if !line.char_x.is_empty() {
        let idx = offset.saturating_sub(line.text_start).min(line.char_x.len() - 1);
        return line.x + line.char_x[idx];
    }

    // BiDi path
    if !line.visual_segments.is_empty() {
        let mut x = line.x;
        for vs in &line.visual_segments {
            let seg_start = vs.logical_start;
            let seg_end   = (vs.logical_start + vs.length).min(line_end);
            if seg_start >= seg_end { continue; }
            let is_rtl = (vs.level & 1) != 0;
            let seg_w  = measure_run_range(text, runs, seg_start, seg_end,
                                           line.extra_space_per_word);
            if offset >= seg_start && offset <= seg_end {
                return if is_rtl {
                    x + measure_run_range(text, runs, offset, seg_end,
                                          line.extra_space_per_word)
                } else {
                    x + measure_run_range(text, runs, seg_start, offset,
                                          line.extra_space_per_word)
                };
            }
            x += seg_w;
        }
        return x;
    }

    // LTR path
    line.x + measure_run_range(text, runs, line.text_start, offset,
                               line.extra_space_per_word)
}

// ─── Offset from X ────────────────────────────────────────────────────────────

/// Returns the text byte-offset closest to `x` pixels within `line`.
pub fn get_offset_from_x(
    text: &str,
    runs: &[InlineRun],
    line: &LayoutLine,
    x:    f32,
) -> usize {
    if x <= line.x { return line.text_start; }

    let line_end = line.text_start + line.text_length;

    // Fast path: use pre-computed char_x positions for pixel-accurate click mapping.
    // char_x[i] is the x position (relative to line.x) at byte offset line.text_start+i.
    if !line.char_x.is_empty() {
        let rel_x = x - line.x;
        let range_end = line.char_x.len() - 1; // last entry is end-of-line position
        let line_start = line.text_start;
        let measure_end = line_start + range_end;
        // Walk character boundaries, return where rel_x falls before the midpoint.
        let flat_slice_s = floor_char_boundary(text, line_start.min(text.len()));
        let flat_slice_e = floor_char_boundary(text, measure_end.min(text.len()));
        let mut byte_off = line_start;
        for ch in text[flat_slice_s..flat_slice_e].chars() {
            let i     = byte_off - line_start;
            let next  = byte_off + ch.len_utf8();
            let ni    = (next - line_start).min(line.char_x.len() - 1);
            let x0 = line.char_x[i];
            let x1 = line.char_x[ni];
            if rel_x < x0 + (x1 - x0) / 2.0 { return byte_off; }
            byte_off = next;
        }
        return measure_end;
    }

    // Strip trailing newline from measurement
    let measure_end = if line_end > line.text_start
        && line_end <= text.len()
        && text.as_bytes().get(line_end - 1) == Some(&b'\n')
    {
        line_end - 1
    } else {
        line_end
    };

    let range_len = measure_end.saturating_sub(line.text_start);
    if range_len == 0 { return line.text_start; }

    // Build per-byte-offset character widths
    let mut char_widths = vec![0.0f32; range_len];

    for run in runs {
        let seg_start = line.text_start.max(run.text_offset);
        let seg_end   = measure_end.min(run.text_offset + run.length);
        if seg_start >= seg_end { continue; }

        let font_px  = run.style.font_size_px(16.0, 16.0);
        let letter_s = run.style.letter_spacing.resolve(font_px, 0.0, 16.0);
        let word_s   = run.style.word_spacing.resolve(font_px, 0.0, 16.0);

        let s = floor_char_boundary(text, seg_start.min(text.len()));
        let e = floor_char_boundary(text, seg_end.min(text.len()));
        if s >= e { continue; }

        let mut byte_off = seg_start;
        for ch in text[s..e].chars() {
            if ch != '\n' && ch != '\r' {
                let idx = byte_off - line.text_start;
                if idx < range_len {
                    let mut cw = approx_char_width(ch, font_px) + letter_s;
                    if ch == ' ' { cw += word_s; }
                    char_widths[idx] = cw;
                }
            }
            byte_off += ch.len_utf8();
        }
    }

    // Add justified extra spacing on spaces
    if line.extra_space_per_word > 0.0 {
        let s = floor_char_boundary(text, line.text_start.min(text.len()));
        let e = floor_char_boundary(text, measure_end.min(text.len()));
        let mut byte_off = line.text_start;
        for ch in text[s..e].chars() {
            if ch == ' ' {
                let idx = byte_off - line.text_start;
                if idx < range_len { char_widths[idx] += line.extra_space_per_word; }
            }
            byte_off += ch.len_utf8();
        }
    }

    let rel_x = x - line.x;

    // BiDi path
    if !line.visual_segments.is_empty() {
        let mut cur_x = 0.0f32;
        for vs in &line.visual_segments {
            let seg_start = vs.logical_start;
            let seg_end   = (vs.logical_start + vs.length).min(measure_end);
            if seg_start < line.text_start || seg_start >= seg_end { continue; }
            let is_rtl = (vs.level & 1) != 0;
            let seg_w: f32 = (seg_start..seg_end)
                .map(|i| char_widths.get(i - line.text_start).copied().unwrap_or(0.0))
                .sum();

            if rel_x < cur_x + seg_w {
                let s = floor_char_boundary(text, seg_start.min(text.len()));
                let e = floor_char_boundary(text, seg_end.min(text.len()));
                if is_rtl {
                    let mut seg_x   = cur_x + seg_w;
                    let mut byte_off = seg_start;
                    for ch in text[s..e].chars() {
                        let cw = char_widths.get(byte_off - line.text_start)
                                            .copied().unwrap_or(0.0);
                        seg_x -= cw;
                        if rel_x >= seg_x && rel_x < seg_x + cw {
                            return if rel_x < seg_x + cw / 2.0 {
                                byte_off + ch.len_utf8()
                            } else {
                                byte_off
                            };
                        }
                        byte_off += ch.len_utf8();
                    }
                    return seg_start;
                } else {
                    let mut seg_x   = cur_x;
                    let mut byte_off = seg_start;
                    for ch in text[s..e].chars() {
                        let cw = char_widths.get(byte_off - line.text_start)
                                            .copied().unwrap_or(0.0);
                        if rel_x < seg_x + cw / 2.0 { return byte_off; }
                        seg_x    += cw;
                        byte_off += ch.len_utf8();
                    }
                    return seg_end;
                }
            }
            cur_x += seg_w;
        }
        return measure_end;
    }

    // LTR path
    let mut cur_x    = 0.0f32;
    let mut byte_off = line.text_start;
    let s = floor_char_boundary(text, line.text_start.min(text.len()));
    let e = floor_char_boundary(text, measure_end.min(text.len()));
    for ch in text[s..e].chars() {
        let cw = char_widths.get(byte_off - line.text_start).copied().unwrap_or(0.0);
        if rel_x < cur_x + cw / 2.0 { return byte_off; }
        cur_x    += cw;
        byte_off += ch.len_utf8();
    }
    measure_end
}

// ─── Hit result ───────────────────────────────────────────────────────────────

/// Result of a hit test: identifies which box was hit and where within it.
pub struct HitResult {
    /// Raw pointer to the hit box (valid while the Document is unmodified).
    pub box_ptr:      *const HtmlBox,
    /// Byte offset within that box's flat text.
    pub local_offset: usize,
}

// ─── Recursive box hit test ───────────────────────────────────────────────────
//
// NOTE: In the Rust layout engine ALL coordinates (border_rect, content_rect,
// line.x, line.y) are in ABSOLUTE document space — unlike the C++ where they
// were relative to the parent's content area.  The hit test therefore works
// entirely in absolute coordinates and does NOT subtract content_rect offsets
// when recursing into children.

fn hit_test_impl(node: &HtmlBox, doc_pt: (f32, f32), _button: u8) -> Option<HitResult> {
    if matches!(node.style.display, Display::None) { return None; }

    // Adjust for this node's own scroll offset (rare — only scrollable boxes)
    let px = doc_pt.0 + node.scroll_left;
    let py = doc_pt.1 + node.scroll_top;

    // Note: inline-content hit test is performed after child checks so that
    // block children (e.g. buttons) receive hits even when the parent has
    // inline text nodes. See fallback handling below.

    // Pass 1: deepest child whose borderRect (absolute) contains the point
    for child in node.children.iter().rev() {
        // display:contents elements are transparent — recurse into their children directly
        if matches!(child.style.display, crate::types::Display::Contents) {
            if let Some(r) = hit_test_impl(child, (px, py), _button) { return Some(r); }
            continue;
        }
        let b = &child.border_rect;
        if px >= b.x && px < b.x + b.w && py >= b.y && py < b.y + b.h {
            if let Some(r) = hit_test_impl(child, (px, py), _button) { return Some(r); }
            return Some(HitResult { box_ptr: child as *const HtmlBox, local_offset: 0 });
        }
    }

    // Pass 2: children whose marginRect contains the point (gap / margin areas)
    for child in node.children.iter().rev() {
        let b = &child.border_rect;
        let in_border = px >= b.x && px < b.x + b.w && py >= b.y && py < b.y + b.h;
        if in_border { continue; }
        let m = &child.margin_rect;
        if px >= m.x && px < m.x + m.w && py >= m.y && py < m.y + m.h {
            if let Some(r) = hit_test_impl(child, (px, py), _button) { return Some(r); }
        }
    }

    // Pass 3: X-range only — handles margin-collapse overflow
    for child in node.children.iter().rev() {
        let m = &child.margin_rect;
        let in_margin = px >= m.x && px < m.x + m.w && py >= m.y && py < m.y + m.h;
        if in_margin { continue; }
        let b = &child.border_rect;
        if px >= b.x && px < b.x + b.w {
            if let Some(r) = hit_test_impl(child, (px, py), _button) { return Some(r); }
        }
    }

    // Fallback: If no children hit, but this node contains the point
    let b = &node.border_rect;
    if px >= b.x && px < b.x + b.w && py >= b.y && py < b.y + b.h {
        // If this node has inline content, return the inline hit offset
        // (caret/text hit). Otherwise select the node itself.
            if !node.line_cache.is_empty() {
            let flat = collect_flat_text(node);
            let line = snap_to_line(&node.line_cache, py);
            let off  = get_offset_from_x(&flat, &node.inline_runs, line, px);
            return Some(HitResult { box_ptr: node as *const HtmlBox, local_offset: off });
        }
        return Some(HitResult { box_ptr: node as *const HtmlBox, local_offset: 0 });
    }

    None
}

fn snap_to_line(lines: &[LayoutLine], y: f32) -> &LayoutLine {
    if lines.is_empty() { panic!("snap_to_line called with empty lines"); }
    
    let mut lo = 0usize;
    let mut hi = lines.len() - 1;
    
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let l = &lines[mid];
        
        if y < l.y {
            if mid == 0 { return &lines[0]; }
            hi = mid - 1;
        } else if y >= l.y + l.height {
            lo = mid + 1;
        } else {
            return l;
        }
    }
    
    // Snap to the closest line if we're between lines
    if lo >= lines.len() { return lines.last().unwrap(); }
    &lines[lo]
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Map a document-space (x, y) point to the box and byte-offset it hits.
///
/// The point is relative to the document origin (top-left of viewport content),
/// before any scroll offset is applied — i.e. `(mouse_x + scroll_x, mouse_y + scroll_y)`.
pub fn point_to_hit(root: &HtmlBox, doc_pt: (f32, f32), button: u8) -> Option<HitResult> {
    // Coordinates are absolute — pass directly (root.content_rect is always 0,0)
    hit_test_impl(root, doc_pt, button)
}

/// Map a (box_ptr, local_byte_offset) back to a document-space (x, y) point
/// for caret or selection-anchor rendering.
///
/// Pass `scroll_x = 0, scroll_y = 0` to get absolute document coordinates.
pub fn offset_to_point(
    root:         &HtmlBox,
    box_ptr:      *const HtmlBox,
    local_offset: usize,
    scroll_x:     f32,
    scroll_y:     f32,
) -> Option<(f32, f32)> {
    find_and_measure(root, box_ptr, local_offset, scroll_x, scroll_y)
}

fn find_and_measure(
    node:         &HtmlBox,
    box_ptr:      *const HtmlBox,
    local_offset: usize,
    scroll_x:     f32,
    scroll_y:     f32,
) -> Option<(f32, f32)> {
    if std::ptr::eq(node as *const HtmlBox, box_ptr) {
        let flat = collect_flat_text(node);
        // line.x and line.y are absolute doc coords — just subtract scroll
        return Some(caret_point_in_box(
            &flat, &node.inline_runs, &node.line_cache,
            local_offset, scroll_x, scroll_y,
        ));
    }

    // If this box is a block container with inline content (has line_cache),
    // check whether the target is an inline descendant laid out here.
    if !node.line_cache.is_empty() {
        let mut acc: usize = 0;
        if let Some(abs_off) = inline_offset_of(node, box_ptr, local_offset, &mut acc) {
            let flat = collect_flat_text(node);
            return Some(caret_point_in_box(
                &flat, &node.inline_runs, &node.line_cache,
                abs_off, scroll_x, scroll_y,
            ));
        }
    }

    for child in &node.children {
        if let Some(pt) = find_and_measure(child, box_ptr, local_offset, scroll_x, scroll_y) {
            return Some(pt);
        }
    }
    None
}

/// Walk inline content of `node` in document order, accumulating text byte-lengths.
/// When `target` is found, return `acc + local_offset` (absolute offset within the
/// containing block's flat text).  Returns `None` if `target` is not in this subtree.
fn inline_offset_of(
    node:         &HtmlBox,
    target:       *const HtmlBox,
    local_offset: usize,
    acc:          &mut usize,
) -> Option<usize> {
    if std::ptr::eq(node as *const HtmlBox, target) {
        return Some(*acc + local_offset);
    }
    // Atomic inline-blocks have their own layout — don't look inside them
    if matches!(node.style.display,
                Display::InlineBlock | Display::InlineFlex | Display::InlineGrid) {
        return None;
    }
    if matches!(node.style.display, Display::None) { return None; }

    if node.is_text_node() {
        *acc += node.text.len();
        return None;
    }

    // Inline element's own text (set directly on the element, not via child #text)
    if !node.text.is_empty() {
        *acc += node.text.len();
    }

    for child in &node.children {
        if let Some(r) = inline_offset_of(child, target, local_offset, acc) {
            return Some(r);
        }
    }
    None
}

fn caret_point_in_box(
    flat:         &str,
    runs:         &[InlineRun],
    lines:        &[LayoutLine],
    local_offset: usize,
    scroll_x:     f32,
    scroll_y:     f32,
) -> (f32, f32) {
    // line.x / line.y are absolute document coordinates
    for line in lines {
        let line_end = line.text_start + line.text_length;
        if local_offset >= line.text_start && local_offset <= line_end {
            let cx = get_caret_x(flat, runs, line, local_offset);
            return (cx - scroll_x, line.y - scroll_y);
        }
    }
    if let Some(last) = lines.last() {
        let cx = get_caret_x(flat, runs, last, local_offset);
        return (cx - scroll_x, last.y - scroll_y);
    }
    (0.0, 0.0)
}

/// Find the deepest box at a document-space point.
pub fn hit_test_box_at(root: &HtmlBox, doc_pt: (f32, f32), button: u8) -> *const HtmlBox {
    deepest_box_at(root, doc_pt, button).unwrap_or(root as *const HtmlBox)
}

fn deepest_box_at(node: &HtmlBox, pt: (f32, f32), _button: u8) -> Option<*const HtmlBox> {
    if matches!(node.style.display, Display::None) { return None; }
    let (px, py) = (pt.0 + node.scroll_left, pt.1 + node.scroll_top);
    for child in node.children.iter().rev() {
        let m = &child.margin_rect;
        if px >= m.x && px < m.x + m.w && py >= m.y && py < m.y + m.h {
            if let Some(r) = deepest_box_at(child, (px, py), _button) { return Some(r); }
            return Some(child as *const HtmlBox);
        }
    }
    None
}

/// Find a link URL at a document-space point, if any.
pub fn hit_test_link(root: &HtmlBox, doc_pt: (f32, f32), button: u8) -> Option<String> {
    // 1. Try hitting text content (inline runs)
    if let Some(hit) = hit_test_impl(root, doc_pt, button) {
        let node = unsafe { &*hit.box_ptr };
        // Find which run was hit
        for run in &node.inline_runs {
            if hit.local_offset >= run.text_offset && hit.local_offset < run.text_offset + run.length {
                if !run.style.href.is_empty() {
                    return Some(run.style.href.clone());
                }
            }
        }
    }

    // 2. Fallback: find the deepest box and search up for 'href' attribute
    let box_ptr = hit_test_box_at(root, doc_pt, button);
    if box_ptr.is_null() { return None; }
    find_href_up(root, box_ptr)
}

fn find_href_up(root: &HtmlBox, target: *const HtmlBox) -> Option<String> {
    let mut path = Vec::new();
    if collect_path(root, target, &mut path) {
        for &node_ptr in path.iter().rev() {
            let node = unsafe { &*node_ptr };
            if let Some(href) = node.attributes.get("href") {
                if !href.is_empty() { return Some(href.clone()); }
            }
        }
    }
    None
}

fn collect_path(
    node:   &HtmlBox,
    target: *const HtmlBox,
    path:   &mut Vec<*const HtmlBox>,
) -> bool {
    path.push(node as *const HtmlBox);
    if std::ptr::eq(node as *const HtmlBox, target) { return true; }
    for child in &node.children {
        if collect_path(child, target, path) { return true; }
    }
    path.pop();
    false
}
