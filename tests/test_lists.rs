// List tests – ported from cpptests/test_lists.cpp
use rhtmledit::types::*;
use rhtmledit::{load_html, parse_html};
use rhtmledit::css::apply_property;

fn find_box<'a>(root: &'a HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(found) = find_box(child, pred) { return Some(found); }
    }
    None
}

fn count_boxes(root: &HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> usize {
    let mut n = if pred(root) { 1 } else { 0 };
    for child in &root.children {
        n += count_boxes(child, pred);
    }
    n
}

fn walk_boxes<'a>(root: &'a HtmlBox, out: &mut Vec<&'a HtmlBox>, pred: &dyn Fn(&HtmlBox) -> bool) {
    if pred(root) { out.push(root); }
    for child in &root.children {
        walk_boxes(child, out, pred);
    }
}

// ============================================================
// List Structure Parsing
// ============================================================

#[test]
fn unordered_list_structure() {
    let doc = parse_html("<ul><li>A</li><li>B</li><li>C</li></ul>");
    let ul = find_box(&doc.root, &|b| b.tag == "ul");
    assert!(ul.is_some());
    assert_eq!(ul.unwrap().style.display, Display::Block);
}

#[test]
fn ordered_list_structure() {
    let doc = parse_html("<ol><li>First</li><li>Second</li></ol>");
    let ol = find_box(&doc.root, &|b| b.tag == "ol");
    assert!(ol.is_some());
}

#[test]
fn li_display_list_item() {
    let doc = parse_html("<ul><li>Item</li></ul>");
    let li = find_box(&doc.root, &|b| b.tag == "li");
    assert!(li.is_some());
    assert_eq!(li.unwrap().style.display, Display::ListItem);
}

#[test]
fn li_count() {
    let doc = parse_html("<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>");
    let count = count_boxes(&doc.root, &|b| b.tag == "li");
    assert_eq!(count, 4);
}

// ============================================================
// List Style Type
// ============================================================

#[test]
fn default_disc_style() {
    let doc = parse_html("<ul><li>Item</li></ul>");
    let li = find_box(&doc.root, &|b| b.tag == "li");
    assert!(li.is_some());
    assert_eq!(li.unwrap().style.list_style_type, ListStyleType::Disc);
}

#[test]
fn ordered_list_decimal() {
    let doc = parse_html("<ol><li>First</li><li>Second</li></ol>");
    let li = find_box(&doc.root, &|b| {
        b.tag == "li" && b.style.list_style_type == ListStyleType::Decimal
    });
    assert!(li.is_some());
}

#[test]
fn custom_list_style_square() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "list-style-type", "square");
    assert_eq!(style.list_style_type, ListStyleType::Square);
}

#[test]
fn custom_list_style_none() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "list-style-type", "none");
    assert_eq!(style.list_style_type, ListStyleType::None);
}

// ============================================================
// Nested Lists
// ============================================================

#[test]
fn nested_list() {
    let doc = parse_html(
        "<ul>\
           <li>Parent\
             <ul><li>Child</li></ul>\
           </li>\
         </ul>");
    let li_count = count_boxes(&doc.root, &|b| b.tag == "li");
    assert!(li_count >= 2);
}

// ============================================================
// Definition Lists
// ============================================================

#[test]
fn definition_list() {
    let doc = parse_html("<dl><dt>Term</dt><dd>Definition</dd></dl>");
    let dl = find_box(&doc.root, &|b| b.tag == "dl");
    assert!(dl.is_some());
    assert_eq!(dl.unwrap().style.display, Display::Block);
}

#[test]
fn dt_element() {
    let doc = parse_html("<dl><dt>Term</dt><dd>Definition</dd></dl>");
    let dt = find_box(&doc.root, &|b| b.tag == "dt");
    assert!(dt.is_some());
}

#[test]
fn dd_element() {
    let doc = parse_html("<dl><dt>Term</dt><dd>Definition</dd></dl>");
    let dd = find_box(&doc.root, &|b| b.tag == "dd");
    assert!(dd.is_some());
}

// ============================================================
// List Layout
// ============================================================

#[test]
fn list_items_stacked() {
    let doc = load_html(
        "<ul><li>First</li><li>Second</li><li>Third</li></ul>", 800.0);
    let mut items = Vec::new();
    walk_boxes(&doc.root, &mut items, &|b| b.tag == "li");
    assert!(items.len() >= 2);
    // Second item below first
    assert!(items[1].content_rect.y > items[0].content_rect.y);
}

// ============================================================
// Missing tests ported from C++ test_lists.cpp
// ============================================================

#[test]
fn ordered_list_index() {
    let doc = parse_html("<ol><li>A</li><li>B</li><li>C</li></ol>");
    let mut items = Vec::new();
    walk_boxes(&doc.root, &mut items, &|b| {
        b.tag == "li" && b.style.list_style_type == ListStyleType::Decimal
    });
    assert!(items.len() >= 2);
    // Second item should have a higher index than first
    assert!(items[1].style.list_index > items[0].style.list_index);
}

#[test]
fn custom_list_style_circle() {
    // Parse with an inherited list-style-type: circle; verify li is found
    let doc = load_html(
        "<ul style=\"list-style-type: circle;\"><li>Item</li></ul>", 800.0);
    let li = find_box(&doc.root, &|b| b.tag == "li");
    assert!(li.is_some());
}
