// Ported from cpptests/test_contenteditable.cpp
//
// Tests that can be expressed without widget infrastructure:
//   - BoxDefaultFalse: HtmlBox has no contentEditable flag → parse works fine
//   - DOM manipulation: toggle_bold, toggle_italic, toggle_underline,
//     set_font_size, set_text_color using the dom module APIs

use rhtmledit::types::*;
use rhtmledit::parse_html;
use rhtmledit::dom::{
    toggle_bold, toggle_italic, toggle_underline, set_font_size, set_text_color,
    TextRange,
};

fn find_box<'a, F: Fn(&HtmlBox) -> bool>(root: &'a HtmlBox, pred: &F) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(b) = find_box(child, pred) { return Some(b); }
    }
    None
}

// ============================================================
// contentEditable attribute on Box
// HtmlBox does not expose a content_editable field in Rust;
// these tests verify that parsing/layout work correctly and that
// the box tree is accessible (structural equivalents of BoxDefaultFalse).
// ============================================================

#[test]
fn ce_box_parse_produces_tree() {
    // Equivalent of BoxDefaultFalse: parse produces a valid box tree
    // and the p element exists without any unexpected flags.
    let doc = parse_html("<p>Text</p>");
    let p = find_box(&doc.root, &|b| b.tag == "p");
    assert!(p.is_some(), "parsed document should have a <p> box");
    // HtmlBox doesn't have a content_editable field; confirm that
    // tag and text are as expected.
    let p = p.unwrap();
    assert_eq!(p.tag, "p");
    // text_content includes inline text
    assert!(p.text_content().contains("Text"));
}

#[test]
fn ce_nested_divs_are_accessible() {
    // Equivalent of IsContentEditableInherited structural test
    let doc = parse_html("<div><p>Text</p></div>");
    let div = find_box(&doc.root, &|b| b.tag == "div");
    assert!(div.is_some(), "should find <div>");
    let p = find_box(div.unwrap(), &|b| b.tag == "p");
    assert!(p.is_some(), "should find nested <p>");
}

#[test]
fn ce_sibling_boxes_are_independent() {
    // Equivalent of NotInheritedFromSibling: two sibling <p> elements
    // are independent boxes
    let doc = parse_html(r#"<div><p id="a">A</p><p id="b">B</p></div>"#);
    let paras: Vec<_> = {
        let mut v = vec![];
        fn collect<'a>(b: &'a HtmlBox, v: &mut Vec<&'a HtmlBox>) {
            if b.tag == "p" { v.push(b); }
            for c in &b.children { collect(c, v); }
        }
        collect(&doc.root, &mut v);
        v
    };
    assert!(paras.len() >= 2, "should have at least 2 <p> siblings");
    // They are independent (different pointers)
    assert!(
        std::ptr::eq(paras[0], paras[0]) && !std::ptr::eq(paras[0], paras[1]),
        "sibling paragraphs should be distinct boxes"
    );
}

// ============================================================
// DOM manipulation: toggle_bold
// ============================================================

#[test]
fn ce_toggle_bold_turns_on() {
    let mut b = HtmlBox::new("p");
    b.text = "Hello World".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 11, style: ComputedStyle::default() },
    ];
    let range = TextRange { start: 0, end: 11 };
    toggle_bold(&mut b, &range);
    assert!(b.inline_runs[0].style.font_weight.is_bold(), "should be bold after toggle");
}

#[test]
fn ce_toggle_bold_turns_off_when_all_bold() {
    let mut b = HtmlBox::new("p");
    b.text = "Hello".to_string();
    let mut style = ComputedStyle::default();
    style.font_weight = FontWeight::Bold;
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 5, style },
    ];
    let range = TextRange { start: 0, end: 5 };
    toggle_bold(&mut b, &range);
    assert!(!b.inline_runs[0].style.font_weight.is_bold(), "should be normal after un-toggle");
}

#[test]
fn ce_toggle_bold_partial_range() {
    // Only the overlapping run should be affected
    let mut b = HtmlBox::new("p");
    b.text = "Hello World".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 5,  style: ComputedStyle::default() }, // "Hello"
        InlineRun { text_offset: 6, length: 5,  style: ComputedStyle::default() }, // "World"
    ];
    // Toggle bold on "World" only
    let range = TextRange { start: 6, end: 11 };
    toggle_bold(&mut b, &range);
    assert!(!b.inline_runs[0].style.font_weight.is_bold(), "Hello run should remain normal");
    assert!(b.inline_runs[1].style.font_weight.is_bold(), "World run should be bold");
}

// ============================================================
// DOM manipulation: toggle_italic
// ============================================================

#[test]
fn ce_toggle_italic_turns_on() {
    let mut b = HtmlBox::new("p");
    b.text = "Test".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 4, style: ComputedStyle::default() },
    ];
    let range = TextRange { start: 0, end: 4 };
    toggle_italic(&mut b, &range);
    assert_eq!(b.inline_runs[0].style.font_style, FontStyle::Italic);
}

#[test]
fn ce_toggle_italic_turns_off() {
    let mut b = HtmlBox::new("p");
    b.text = "Test".to_string();
    let mut style = ComputedStyle::default();
    style.font_style = FontStyle::Italic;
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 4, style },
    ];
    let range = TextRange { start: 0, end: 4 };
    toggle_italic(&mut b, &range);
    assert_eq!(b.inline_runs[0].style.font_style, FontStyle::Normal);
}

// ============================================================
// DOM manipulation: toggle_underline
// ============================================================

#[test]
fn ce_toggle_underline_turns_on() {
    let mut b = HtmlBox::new("p");
    b.text = "Test".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 4, style: ComputedStyle::default() },
    ];
    let range = TextRange { start: 0, end: 4 };
    toggle_underline(&mut b, &range);
    assert!(b.inline_runs[0].style.text_decoration.underline, "underline should be on");
}

#[test]
fn ce_toggle_underline_turns_off() {
    let mut b = HtmlBox::new("p");
    b.text = "Test".to_string();
    let mut style = ComputedStyle::default();
    style.text_decoration.underline = true;
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 4, style },
    ];
    let range = TextRange { start: 0, end: 4 };
    toggle_underline(&mut b, &range);
    assert!(!b.inline_runs[0].style.text_decoration.underline, "underline should be off");
}

// ============================================================
// DOM manipulation: set_font_size
// ============================================================

#[test]
fn ce_set_font_size() {
    let mut b = HtmlBox::new("p");
    b.text = "Hello".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 5, style: ComputedStyle::default() },
    ];
    let range = TextRange { start: 0, end: 5 };
    set_font_size(&mut b, &range, 24.0);
    assert_eq!(b.inline_runs[0].style.font_size, CssLength::Px(24.0));
}

#[test]
fn ce_set_font_size_partial() {
    let mut b = HtmlBox::new("p");
    b.text = "Hello World".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 5,  style: ComputedStyle::default() },
        InlineRun { text_offset: 6, length: 5,  style: ComputedStyle::default() },
    ];
    // Set size only on "World"
    let range = TextRange { start: 6, end: 11 };
    set_font_size(&mut b, &range, 18.0);
    // "Hello" run should be unchanged (default font size)
    assert!(!matches!(b.inline_runs[0].style.font_size, CssLength::Px(v) if (v - 18.0).abs() < 0.1),
        "Hello run should not have size 18");
    assert_eq!(b.inline_runs[1].style.font_size, CssLength::Px(18.0));
}

// ============================================================
// DOM manipulation: set_text_color
// ============================================================

#[test]
fn ce_set_text_color() {
    let mut b = HtmlBox::new("p");
    b.text = "Hello".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 5, style: ComputedStyle::default() },
    ];
    let range = TextRange { start: 0, end: 5 };
    set_text_color(&mut b, &range, Color::rgb(255, 0, 0));
    assert_eq!(b.inline_runs[0].style.color, Color::rgb(255, 0, 0));
}

#[test]
fn ce_set_text_color_partial() {
    let mut b = HtmlBox::new("p");
    b.text = "Hello World".to_string();
    b.inline_runs = vec![
        InlineRun { text_offset: 0, length: 5,  style: ComputedStyle::default() },
        InlineRun { text_offset: 6, length: 5,  style: ComputedStyle::default() },
    ];
    // Color only "World" blue
    let range = TextRange { start: 6, end: 11 };
    set_text_color(&mut b, &range, Color::rgb(0, 0, 255));
    assert_eq!(b.inline_runs[0].style.color, Color::BLACK, "Hello should remain black");
    assert_eq!(b.inline_runs[1].style.color, Color::rgb(0, 0, 255));
}
