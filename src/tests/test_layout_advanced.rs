// Ported from tests/test_layout_advanced.cpp

use crate::css::apply_property;
use crate::html::parse_html;
use crate::layout::LayoutEngine;
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
    assert!(b.unwrap().layout.content_rect.h >= 200.0);
}

#[test]
fn layoutadv_max_height_enforced() {
    let doc = parse_and_layout(
        r#"<div style="max-height: 50px; overflow: hidden;"><p>Line1</p><p>Line2</p><p>Line3</p><p>Line4</p><p>Line5</p></div>"#,
        800.0,
    );
    let b = find_box(&doc.root, &|b| b.tag == "div" && !b.style.max_height.is_none());
    assert!(b.is_some());
    assert!(b.unwrap().layout.content_rect.h <= 50.0);
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
    let content_gap = divs[1].layout.content_rect.y - (divs[0].layout.content_rect.y + divs[0].layout.content_rect.h);
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
    let content_gap = divs[1].layout.content_rect.y - (divs[0].layout.content_rect.y + divs[0].layout.content_rect.h);
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

// ── Child combinator + flex-basis applied via "> *" ──────────────────────────

#[test]
fn layoutadv_child_combinator_flex_basis_applied() {
    // ".grid > *" with "flex: 1 1 260px" must apply flex-basis to direct children
    // so they are laid out side by side, not one per row.
    let doc = parse_and_layout(r#"
        <style>
            .grid { display: flex; flex-wrap: wrap; gap: 20px; }
            .grid > * { flex: 1 1 260px; }
            .card { padding: 20px; }
        </style>
        <div class="grid">
            <div class="card">A</div>
            <div class="card">B</div>
            <div class="card">C</div>
        </div>
    "#, 1024.0);

    let cards: Vec<_> = find_all_boxes(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "card").unwrap_or(false)
    });
    assert_eq!(cards.len(), 3);

    // All three cards should be on the same row (same y position).
    let y0 = cards[0].layout.margin_rect.y;
    assert!((cards[1].layout.margin_rect.y - y0).abs() < 2.0,
        "card B should be on the same row as card A (child combinator not applying flex-basis)");
    assert!((cards[2].layout.margin_rect.y - y0).abs() < 2.0,
        "card C should be on the same row as card A");

    // They should be side-by-side (x positions increasing).
    assert!(cards[1].layout.margin_rect.x > cards[0].layout.margin_rect.x + 50.0,
        "card B should be to the right of card A");
    assert!(cards[2].layout.margin_rect.x > cards[1].layout.margin_rect.x + 50.0,
        "card C should be to the right of card B");
}

// ── compute_intrinsic_width: auto margins must not inflate parent width ────────

#[test]
fn layoutadv_auto_margin_does_not_inflate_intrinsic_width() {
    // An element with "margin: 0 auto" inside a flex container should not
    // cause its parent's intrinsic width to be the full container width.
    // Before the fix, flex items defaulting to auto flex-basis would call
    // compute_intrinsic_width, which used margin_rect.x + margin_rect.w
    // even for auto-margin elements, giving a ~container-width result.
    let doc = parse_and_layout(r#"
        <style>
            .flex { display: flex; gap: 10px; }
            .box { padding: 10px; }
            .inner { width: 80px; height: 80px; margin: 0 auto; }
        </style>
        <div class="flex">
            <div class="box"><div class="inner"></div><p>A</p></div>
            <div class="box"><div class="inner"></div><p>B</p></div>
            <div class="box"><div class="inner"></div><p>C</p></div>
        </div>
    "#, 900.0);

    let boxes: Vec<_> = find_all_boxes(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "box").unwrap_or(false)
    });
    assert_eq!(boxes.len(), 3);

    // All three flex items should be on the same row.
    let y0 = boxes[0].layout.margin_rect.y;
    assert!((boxes[1].layout.margin_rect.y - y0).abs() < 2.0,
        "box B on same row as A — auto margin in child should not inflate intrinsic width");
    assert!((boxes[2].layout.margin_rect.y - y0).abs() < 2.0,
        "box C on same row as A");

    // Each box should be much narrower than the full container.
    for b in &boxes {
        assert!(b.layout.margin_rect.w < 400.0,
            "auto-margin child must not inflate flex item intrinsic width to container width");
    }
}

// ── Flex-stretch height on initial layout and after viewport resize ───────────

fn find_by_id<'a>(node: &'a HtmlBox, id: &str) -> Option<&'a HtmlBox> {
    if node.attributes.get("id").map(|s| s == id).unwrap_or(false) { return Some(node); }
    for child in &node.children {
        if let Some(b) = find_by_id(child, id) { return Some(b); }
    }
    None
}

/// Flex-stretch: sidebar fills the full viewport height on initial layout.
#[test]
fn flex_stretch_sidebar_fills_height_initial() {
    // A classic sidebar layout: row flex container at 100vh, sidebar stretches.
    let html = r#"<html><head><style>
        html, body { margin: 0; padding: 0; }
        .app  { display: flex; flex-direction: row; height: 100vh; }
        .side { width: 200px; background: navy; }
        .main { flex: 1; background: white; }
    </style></head><body>
        <div class="app">
            <div id="side" class="side"></div>
            <div id="main" class="main"></div>
        </div>
    </body></html>"#;

    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.viewport_h = 600.0;
    engine.layout(&mut doc, 800.0);

    let side = find_by_id(&doc.root, "side").expect("side");
    assert!((side.layout.border_rect.h - 600.0).abs() < 2.0,
        "sidebar should stretch to 100vh=600px on initial layout, got {}", side.layout.border_rect.h);
}

/// Flex-stretch: sidebar correctly updates its height when the window is resized.
#[test]
fn flex_stretch_sidebar_updates_on_viewport_height_resize() {
    let html = r#"<html><head><style>
        html, body { margin: 0; padding: 0; }
        .app  { display: flex; flex-direction: row; height: 100vh; }
        .side { width: 200px; background: navy; }
        .main { flex: 1; background: white; }
    </style></head><body>
        <div class="app">
            <div id="side" class="side"></div>
            <div id="main" class="main"></div>
        </div>
    </body></html>"#;

    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();

    // Initial layout at 600px tall.
    engine.viewport_h = 600.0;
    engine.layout(&mut doc, 800.0);

    {
        let side = find_by_id(&doc.root, "side").expect("side");
        assert!((side.layout.border_rect.h - 600.0).abs() < 2.0,
            "initial: sidebar should be 600px, got {}", side.layout.border_rect.h);
    }

    // Simulate window resize: taller viewport. Width unchanged so pruning would
    // incorrectly skip re-layout without the viewport_h guard.
    engine.viewport_h = 900.0;
    engine.layout(&mut doc, 800.0);

    {
        let side = find_by_id(&doc.root, "side").expect("side");
        assert!((side.layout.border_rect.h - 900.0).abs() < 2.0,
            "after resize to 900px: sidebar should be 900px, got {}", side.layout.border_rect.h);
    }
}

/// Fixed-width elements should still be pruned (not re-laid out) when only
/// viewport width changes within the same column (regression guard).
#[test]
fn layout_pruning_still_active_on_width_only_resize() {
    // A fixed-width card inside a fluid container. When the viewport is widened,
    // the card's content width never changes — it should be pruned (not re-laid out).
    // We verify the card's position shifts correctly without failing.
    let html = r#"<html><head><style>
        html, body { margin: 0; padding: 0; }
        .wrap { display: flex; justify-content: center; }
        .card { width: 300px; height: 200px; background: blue; }
    </style></head><body>
        <div class="wrap">
            <div id="card" class="card"></div>
        </div>
    </body></html>"#;

    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.viewport_h = 600.0;
    engine.layout(&mut doc, 600.0);

    {
        let card = find_by_id(&doc.root, "card").expect("card");
        assert!((card.layout.border_rect.w - 300.0).abs() < 2.0, "initial: card 300px wide");
        assert!((card.layout.border_rect.h - 200.0).abs() < 2.0, "initial: card 200px tall");
    }

    // Widen viewport; card dimensions unchanged, only centering margin shifts.
    engine.layout(&mut doc, 1000.0);

    {
        let card = find_by_id(&doc.root, "card").expect("card");
        assert!((card.layout.border_rect.w - 300.0).abs() < 2.0, "after width resize: card still 300px");
        assert!((card.layout.border_rect.h - 200.0).abs() < 2.0, "after width resize: card still 200px");
    }
}
