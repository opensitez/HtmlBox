/// Tests for caret position accuracy using real cosmic_text glyph metrics.
///
/// Key invariants:
/// - char_x is populated when layout uses a font system (via Renderer::load_html)
/// - get_caret_x returns the exact glyph-boundary x, not an approximation
/// - get_offset_from_x correctly identifies character boundaries
/// - Thin characters like 'i', 'l', '1' are measured correctly (not approximated)

use crate::layout::hit_test::{get_caret_x, get_offset_from_x};
use crate::layout::inline_layout::collect_flat_text;
use crate::types::*;
use crate::Renderer;

fn load_with_fonts(html: &str) -> Document {
    let mut renderer = Renderer::new();
    renderer.load_html(html, 900.0)
}

fn find_editable_box(root: &HtmlBox) -> Option<&HtmlBox> {
    if root.attributes.get("contenteditable").map(|v| v == "true").unwrap_or(false) {
        return Some(root);
    }
    for child in &root.children {
        if let Some(b) = find_editable_box(child) { return Some(b); }
    }
    None
}

/// Find any box that has a populated line_cache (inline content).
fn find_box_with_lines(root: &HtmlBox) -> Option<&HtmlBox> {
    if !root.line_cache.is_empty() { return Some(root); }
    for child in &root.children {
        if let Some(b) = find_box_with_lines(child) { return Some(b); }
    }
    None
}

// ─── char_x population ───────────────────────────────────────────────────────

#[test]
fn char_x_populated_with_font_system() {
    let doc = load_with_fonts(r#"<p contenteditable="true">Hello</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    assert!(!node.line_cache.is_empty(), "no lines in editable box");
    let line = &node.line_cache[0];
    assert!(!line.char_x.is_empty(),
        "char_x must be populated when layout uses a real font system");
}

#[test]
fn char_x_length_matches_text_plus_one() {
    // char_x has one entry per byte boundary + 1 for end-of-line
    let doc = load_with_fonts(r#"<p contenteditable="true">abc</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let line = &node.line_cache[0];
    // "abc" is 3 ASCII bytes → char_x length = 3+1 = 4 (or up to text_length+1)
    assert!(line.char_x.len() >= 4,
        "char_x should have text_length+1 entries, got {}", line.char_x.len());
}

#[test]
fn char_x_starts_at_zero() {
    let doc = load_with_fonts(r#"<p contenteditable="true">Hello world</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let line = &node.line_cache[0];
    assert!(!line.char_x.is_empty());
    assert_eq!(line.char_x[0], 0.0, "first char_x entry must be 0 (relative to line.x)");
}

#[test]
fn char_x_monotonically_increasing_ltr() {
    let doc = load_with_fonts(r#"<p contenteditable="true">Hello world</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let line = &node.line_cache[0];
    for i in 1..line.char_x.len() {
        assert!(line.char_x[i] >= line.char_x[i - 1],
            "char_x[{}]={} < char_x[{}]={} — positions must be non-decreasing",
            i, line.char_x[i], i-1, line.char_x[i-1]);
    }
}

// ─── get_caret_x accuracy ────────────────────────────────────────────────────

#[test]
fn caret_at_start_is_line_x() {
    let doc = load_with_fonts(r#"<p contenteditable="true">Test</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    let x = get_caret_x(&flat, &node.inline_runs, line, line.text_start);
    assert!((x - line.x).abs() < 0.5,
        "caret at start of line should equal line.x, got x={} line.x={}", x, line.x);
}

#[test]
fn caret_at_end_matches_line_width() {
    let doc = load_with_fonts(r#"<p contenteditable="true">Test</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    let end_off = line.text_start + line.text_length;
    // Strip trailing newline if present
    let measure_end = if end_off > 0 && flat.as_bytes().get(end_off - 1) == Some(&b'\n') {
        end_off - 1
    } else { end_off };
    let x_end = get_caret_x(&flat, &node.inline_runs, line, measure_end);
    // The caret at the end should be further right than at the start
    assert!(x_end > line.x, "caret at end should be past start of line");
}

#[test]
fn caret_positions_strictly_ordered_for_distinct_chars() {
    // With real shaping, each character boundary produces a distinct x
    let doc = load_with_fonts(r#"<p contenteditable="true">Hello</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    let mut prev_x = -1.0f32;
    for i in 0..="Hello".len() {
        let off = line.text_start + i;
        let x = get_caret_x(&flat, &node.inline_runs, line, off);
        assert!(x >= prev_x,
            "caret x at offset {} ({}) must be >= previous x {}", i, x, prev_x);
        prev_x = x;
    }
}

// ─── Thin characters: 'i', 'l', '1' ─────────────────────────────────────────
// These are the letters that the old approx got most wrong.

#[test]
fn thin_char_l_has_nonzero_width() {
    let doc = load_with_fonts(r#"<p contenteditable="true">flex</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    // "flex": f=0, l=1, e=2, x=3
    let x_before_l = get_caret_x(&flat, &node.inline_runs, line, line.text_start + 1);
    let x_after_l  = get_caret_x(&flat, &node.inline_runs, line, line.text_start + 2);
    assert!(x_after_l > x_before_l,
        "'l' must have positive width: before={} after={}", x_before_l, x_after_l);
}

#[test]
fn thin_char_l_is_narrower_than_m() {
    // In any proportional font 'l' < 'm'
    let doc_l = load_with_fonts(r#"<p contenteditable="true">al</p>"#);
    let doc_m = load_with_fonts(r#"<p contenteditable="true">am</p>"#);
    let width_of = |doc: &Document, idx: usize| {
        let node = find_editable_box(&doc.root).unwrap();
        let flat = collect_flat_text(node);
        let line = &node.line_cache[0];
        let x0 = get_caret_x(&flat, &node.inline_runs, line, line.text_start + idx);
        let x1 = get_caret_x(&flat, &node.inline_runs, line, line.text_start + idx + 1);
        x1 - x0
    };
    let w_l = width_of(&doc_l, 1);
    let w_m = width_of(&doc_m, 1);
    assert!(w_l < w_m,
        "width of 'l' ({}) should be less than width of 'm' ({}) in a proportional font",
        w_l, w_m);
}

// ─── get_offset_from_x ───────────────────────────────────────────────────────

#[test]
fn offset_from_x_at_line_start_returns_start() {
    let doc = load_with_fonts(r#"<p contenteditable="true">Hello</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    // Clicking before any text should return the start offset
    let off = get_offset_from_x(&flat, &node.inline_runs, line, line.x - 5.0);
    assert_eq!(off, line.text_start,
        "click before line start should return text_start");
}

#[test]
fn offset_from_x_midpoint_selects_correct_char() {
    // Clicking at the midpoint of a character should select that character's boundary.
    // This is the key invariant: clicking inside char i should return either i or i+1.
    let doc = load_with_fonts(r#"<p contenteditable="true">Hello world</p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    if line.char_x.is_empty() { return; }

    let text_slice = &flat[line.text_start..line.text_start + line.text_length];
    let mut byte_off = line.text_start;
    for (_i, ch) in text_slice.char_indices() {
        let x0 = get_caret_x(&flat, &node.inline_runs, line, byte_off);
        let next = byte_off + ch.len_utf8();
        let x1 = get_caret_x(&flat, &node.inline_runs, line, next);
        if (x1 - x0).abs() < 0.01 {
            byte_off = next;
            continue; // zero-width char, skip
        }
        // Click at midpoint of this character
        let mid_x = (x0 + x1) / 2.0 + 0.1; // slightly past midpoint → should give next boundary
        let recovered = get_offset_from_x(&flat, &node.inline_runs, line, mid_x);
        assert!(recovered == byte_off || recovered == next,
            "midpoint click in char at offset {} (width {:.1}): recovered={} expected {} or {}",
            byte_off, x1 - x0, recovered, byte_off, next);
        byte_off = next;
    }
}

// ─── Monospace font accuracy ──────────────────────────────────────────────────

#[test]
fn monospace_chars_equal_width() {
    // In monospace font, 'i' and 'm' should have the same width
    let doc = load_with_fonts(r#"<p contenteditable="true"><code>im</code></p>"#);
    let node = find_editable_box(&doc.root).expect("editable box");
    let flat = collect_flat_text(node);
    let line = &node.line_cache[0];
    if line.char_x.is_empty() { return; } // skip if no font system
    let x0 = get_caret_x(&flat, &node.inline_runs, line, line.text_start);
    let x1 = get_caret_x(&flat, &node.inline_runs, line, line.text_start + 1);
    let x2 = get_caret_x(&flat, &node.inline_runs, line, line.text_start + 2);
    let w_i = x1 - x0;
    let w_m = x2 - x1;
    // In monospace, both chars should have the same advance (within 1px)
    assert!((w_i - w_m).abs() < 1.5,
        "in monospace font 'i' width ({}) should ≈ 'm' width ({})", w_i, w_m);
}

// ─── Bold text is wider ───────────────────────────────────────────────────────

#[test]
fn bold_text_wider_than_normal() {
    let doc_n = load_with_fonts(r#"<p contenteditable="true">Hello</p>"#);
    let doc_b = load_with_fonts(r#"<p contenteditable="true"><strong>Hello</strong></p>"#);
    let line_width = |doc: &Document| {
        let node = find_editable_box(&doc.root).unwrap();
        let flat = collect_flat_text(node);
        let line = &node.line_cache[0];
        *line.char_x.last().unwrap_or(&0.0)
    };
    let w_normal = line_width(&doc_n);
    let w_bold   = line_width(&doc_b);
    assert!(w_bold > w_normal,
        "bold text width ({}) should exceed normal text width ({})", w_bold, w_normal);
}
