// Ported from tests/test_layout_advanced.cpp

use crate::css::apply_property;
use crate::types::*;
use super::harness::*;

// ── Min-Height / Max-Height Parsing ───────────────────────────────────────────

#[test]
fn layoutadv_min_height_parsed() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "min-height", "100px");
    assert_eq!(s.min_height.resolve(16.0, 0.0, 16.0), 100.0);
}

#[test]
fn layoutadv_max_height_parsed() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "max-height", "200px");
    assert_eq!(s.max_height.resolve(16.0, 0.0, 16.0), 200.0);
}

#[test]
fn layoutadv_min_height_percent() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "min-height", "50%");
    assert!(matches!(s.min_height, CssLength::Percent(_)));
    assert_eq!(s.min_height.resolve(16.0, 400.0, 16.0), 200.0);
}

#[test]
fn layoutadv_max_height_percent() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "max-height", "75%");
    assert!(matches!(s.max_height, CssLength::Percent(_)));
    assert_eq!(s.max_height.resolve(16.0, 400.0, 16.0), 300.0);
}

// ── Min-Height / Max-Height Layout Enforcement ────────────────────────────────

#[test]
fn layoutadv_min_height_enforced() {
    let doc = parse_and_layout(r#"<div style="min-height: 200px;">Short</div>"#, 800.0);
    let b = find_box(&doc.root, &|b| {
        b.tag == "div" && !b.style.min_height.is_auto()
            && (b.style.min_height.resolve(16.0, 0.0, 16.0) - 200.0).abs() < 1.0
    });
    assert!(b.is_some());
    assert!(b.unwrap().content_rect.h >= 200.0);
}

#[test]
fn layoutadv_max_height_enforced() {
    let doc = parse_and_layout(
        r#"<div style="max-height: 50px; overflow: hidden;"><p>Line1</p><p>Line2</p><p>Line3</p><p>Line4</p><p>Line5</p></div>"#,
        800.0,
    );
    let b = find_box(&doc.root, &|b| b.tag == "div" && !b.style.max_height.is_none());
    assert!(b.is_some());
    assert!(b.unwrap().content_rect.h <= 50.0);
}

// ── Margin Collapsing ─────────────────────────────────────────────────────────

#[test]
fn layoutadv_margin_collapsing_positive() {
    let doc = parse_and_layout(
        r#"<div style="margin-bottom: 30px;">A</div><div style="margin-top: 20px;">B</div>"#,
        800.0,
    );
    let divs = find_all_boxes(&doc.root, &|b| b.tag == "div");
    assert!(divs.len() >= 2);
    let content_gap = divs[1].content_rect.y - (divs[0].content_rect.y + divs[0].content_rect.h);
    // Collapsed to max(30,20)=30, not 50
    assert!(content_gap < 45.0, "expected collapsed gap < 45, got {}", content_gap);
}

#[test]
fn layoutadv_margin_collapsing_equal() {
    let doc = parse_and_layout(
        r#"<div style="margin-bottom: 20px;">A</div><div style="margin-top: 20px;">B</div>"#,
        800.0,
    );
    let divs = find_all_boxes(&doc.root, &|b| b.tag == "div");
    assert!(divs.len() >= 2);
    let content_gap = divs[1].content_rect.y - (divs[0].content_rect.y + divs[0].content_rect.h);
    // Should be ~20 (collapsed), not 40 (sum)
    assert!(content_gap < 35.0, "expected collapsed gap < 35, got {}", content_gap);
}

// ── Semantic HTML5 Elements ───────────────────────────────────────────────────

#[test]
fn layoutadv_article_is_block() {
    let doc = parse(r#"<article>Content</article>"#);
    let b = find_box(&doc.root, &|b| b.tag == "article");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_section_is_block() {
    let doc = parse(r#"<section>Content</section>"#);
    let b = find_box(&doc.root, &|b| b.tag == "section");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_header_is_block() {
    let doc = parse(r#"<header>Content</header>"#);
    let b = find_box(&doc.root, &|b| b.tag == "header");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_footer_is_block() {
    let doc = parse(r#"<footer>Content</footer>"#);
    let b = find_box(&doc.root, &|b| b.tag == "footer");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_nav_is_block() {
    let doc = parse(r#"<nav>Content</nav>"#);
    let b = find_box(&doc.root, &|b| b.tag == "nav");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_aside_is_block() {
    let doc = parse(r#"<aside>Content</aside>"#);
    let b = find_box(&doc.root, &|b| b.tag == "aside");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_main_is_block() {
    let doc = parse(r#"<main>Content</main>"#);
    let b = find_box(&doc.root, &|b| b.tag == "main");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_figure_is_block() {
    let doc = parse(r#"<figure><figcaption>Caption</figcaption></figure>"#);
    let b = find_box(&doc.root, &|b| b.tag == "figure");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

#[test]
fn layoutadv_figcaption_is_block() {
    let doc = parse(r#"<figure><figcaption>Caption</figcaption></figure>"#);
    let b = find_box(&doc.root, &|b| b.tag == "figcaption");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::Block);
}

// ── Table VALIGN ──────────────────────────────────────────────────────────────

#[test]
fn layoutadv_table_valign_top() {
    let doc = parse(r#"<table><tr><td valign="top">Top</td></tr></table>"#);
    let cell = find_box(&doc.root, &|b| b.tag == "td");
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().style.vertical_align, VerticalAlign::Top);
}

#[test]
fn layoutadv_table_valign_middle() {
    let doc = parse(r#"<table><tr><td valign="middle">Mid</td></tr></table>"#);
    let cell = find_box(&doc.root, &|b| b.tag == "td");
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().style.vertical_align, VerticalAlign::Middle);
}

#[test]
fn layoutadv_table_valign_bottom() {
    let doc = parse(r#"<table><tr><td valign="bottom">Bot</td></tr></table>"#);
    let cell = find_box(&doc.root, &|b| b.tag == "td");
    assert!(cell.is_some());
    assert_eq!(cell.unwrap().style.vertical_align, VerticalAlign::Bottom);
}

// ── Display Inline-Block/Flex/Grid Parsing ────────────────────────────────────

#[test]
fn layoutadv_inline_block_parsed() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "display", "inline-block");
    assert_eq!(s.display, Display::InlineBlock);
}

#[test]
fn layoutadv_inline_flex_parsed() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "display", "inline-flex");
    assert_eq!(s.display, Display::InlineFlex);
}

#[test]
fn layoutadv_inline_grid_parsed() {
    let mut s = ComputedStyle::default();
    apply_property(&mut s, "display", "inline-grid");
    assert_eq!(s.display, Display::InlineGrid);
}

// ── Nested Layout Smoke ───────────────────────────────────────────────────────

#[test]
fn layoutadv_deeply_nested_layout() {
    let doc = parse_and_layout(
        r#"<div style="padding: 10px;"><div style="margin: 5px; border: 1px solid black;"><div style="padding: 5px;"><p>Deeply nested</p></div></div></div>"#,
        800.0,
    );
    // Just verify it doesn't panic
    let _ = &doc.root;
}

#[test]
fn layoutadv_mixed_flow_layout() {
    let doc = parse_and_layout(
        r#"<div><div style="float: left; width: 200px;">Sidebar</div><div style="display: inline-block; width: 100px;">Inline</div><div>Block content</div></div>"#,
        800.0,
    );
    // Just verify it doesn't panic
    let _ = &doc.root;
}
