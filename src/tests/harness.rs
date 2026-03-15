// Test harness helpers — mirrors test_harness.h

use crate::types::*;
use crate::html::parse_html;
use crate::layout::LayoutEngine;
use crate::css::apply_property;

/// Parse HTML and run layout at given viewport width.
pub fn parse_and_layout(html: &str, viewport_width: f32) -> Document {
    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, viewport_width);
    doc
}

/// Parse HTML + apply cascade (no layout).
pub fn parse(html: &str) -> Document {
    parse_html(html)
}

/// Walk every box depth-first, calling visitor on each.
pub fn walk_boxes<F: FnMut(&HtmlBox)>(root: &HtmlBox, visitor: &mut F) {
    visitor(root);
    for child in &root.children {
        walk_boxes(child, visitor);
    }
}

/// Find first box matching predicate.
pub fn find_box<'a, F: Fn(&HtmlBox) -> bool>(root: &'a HtmlBox, pred: &F) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(b) = find_box(child, pred) { return Some(b); }
    }
    None
}

/// Find all boxes matching predicate (depth-first).
pub fn find_all_boxes<'a, F: Fn(&HtmlBox) -> bool>(root: &'a HtmlBox, pred: &F) -> Vec<&'a HtmlBox> {
    let mut result = Vec::new();
    collect_matching(root, pred, &mut result);
    result
}

fn collect_matching<'a, F: Fn(&HtmlBox) -> bool>(
    node: &'a HtmlBox, pred: &F, out: &mut Vec<&'a HtmlBox>
) {
    if pred(node) { out.push(node); }
    for child in &node.children {
        collect_matching(child, pred, out);
    }
}

/// Count boxes matching predicate.
pub fn count_boxes<F: Fn(&HtmlBox) -> bool>(root: &HtmlBox, pred: &F) -> usize {
    let mut n = if pred(root) { 1 } else { 0 };
    for child in &root.children {
        n += count_boxes(child, pred);
    }
    n
}

/// Get text content of the whole document.
pub fn doc_text(doc: &Document) -> String {
    doc.root.text_content()
}

/// Apply a CSS property to a fresh ComputedStyle and return it.
pub fn style_with(prop: &str, val: &str) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, prop, val);
    style
}
