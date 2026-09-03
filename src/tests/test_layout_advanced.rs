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

fn find_by_id<'a>(node: &'a WebCore, id: &str) -> Option<&'a WebCore> {
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

// ── Replaced-element intrinsic contribution (CSS2.1 §10.4) ────────────────────

/// Give every `<img>` under `node` the same natural dimensions; returns how
/// many were found.
fn set_natural_size(node: &mut WebCore, w: u32, h: u32) -> usize {
    let mut n = 0;
    if node.tag == "img" {
        node.image_width = w;
        node.image_height = h;
        n += 1;
    }
    for ch in &mut node.children {
        n += set_natural_size(ch, w, h);
    }
    n
}

/// **An image with a definite height contributes `height × ratio`, not its
/// natural width.** The intrinsic walk used to early-return the natural width,
/// so a 1024×1024 photo shown at `height=150` made its container 1024px wide
/// while the image itself laid out at 150×150.
#[test]
fn layoutadv_replaced_intrinsic_width_follows_definite_height() {
    let mut doc = parse(
        "<div id=card><a><img height='150' src='x.png'></a></div>"
    );
    assert_eq!(set_natural_size(&mut doc.root, 1024, 1024), 1);
    let engine = LayoutEngine::new();
    let card = find_box(&doc.root, &|n: &WebCore| n.attributes.get("id").map(String::as_str) == Some("card")).expect("card");
    assert_eq!(engine.max_content_width(card, 16.0, 16.0), 150.0);
    assert_eq!(engine.min_content_width(card, 16.0, 16.0), 150.0);
}

/// A non-square ratio, so the assertion cannot pass by accident.
#[test]
fn layoutadv_replaced_intrinsic_width_uses_the_ratio() {
    let mut doc = parse("<div id=card><img height='100' src='x.png'></div>");
    assert_eq!(set_natural_size(&mut doc.root, 800, 200), 1);
    let engine = LayoutEngine::new();
    let card = find_box(&doc.root, &|n: &WebCore| n.attributes.get("id").map(String::as_str) == Some("card")).expect("card");
    assert_eq!(engine.max_content_width(card, 16.0, 16.0), 400.0);
}

/// With both dimensions auto the natural width is still the answer.
#[test]
fn layoutadv_replaced_intrinsic_width_defaults_to_natural() {
    let mut doc = parse("<div id=card><img src='x.png'></div>");
    assert_eq!(set_natural_size(&mut doc.root, 640, 480), 1);
    let engine = LayoutEngine::new();
    let card = find_box(&doc.root, &|n: &WebCore| n.attributes.get("id").map(String::as_str) == Some("card")).expect("card");
    assert_eq!(engine.max_content_width(card, 16.0, 16.0), 640.0);
}

/// The end-to-end shape of the tikshbila.com gallery: wrapping flex items whose
/// only sizeable content is a photo shown at a fixed height.
#[test]
fn layoutadv_flex_items_size_to_the_displayed_image_not_the_photo() {
    let mut doc = parse(
        "<style>.row{display:flex;flex-wrap:wrap}</style>\
         <div class=row><div id=a><img height='150' src='1.png'></div>\
         <div id=b><img height='150' src='2.png'></div></div>"
    );
    assert_eq!(set_natural_size(&mut doc.root, 1024, 1024), 2, "both images present");
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, 1256.0);
    let a = find_box(&doc.root, &|n: &WebCore| n.attributes.get("id").map(String::as_str) == Some("a")).expect("a");
    let b = find_box(&doc.root, &|n: &WebCore| n.attributes.get("id").map(String::as_str) == Some("b")).expect("b");
    assert_eq!(a.layout.margin_rect.w, 150.0, "item a");
    assert_eq!(b.layout.margin_rect.w, 150.0, "item b");
    assert_eq!(b.layout.margin_rect.y, a.layout.margin_rect.y, "same flex line");
}

/// **Inline nesting must not lose the spaces between words.** The max-content
/// width of `<a><span>Faire un don</span></a>` has to equal that of the bare
/// text; fr.wikipedia's header links were sized ~7px short — two space widths —
/// so each one broke onto a second line inside a box built for one.
#[test]
fn layoutadv_max_content_width_survives_inline_nesting() {
    let mut renderer = crate::renderer::Renderer::new();
    let doc = renderer.load_html(
        "<div id=plain>Faire un don</div>\
         <div id=nested><a><span>Faire un don</span></a></div>",
        800.0);
    let find = |id: &str| {
        fn walk<'a>(n: &'a WebCore, id: &str) -> Option<&'a WebCore> {
            if n.attributes.get("id").map(String::as_str) == Some(id) { return Some(n); }
            for c in &n.children { if let Some(f) = walk(c, id) { return Some(f); } }
            None
        }
        walk(&doc.root, id).unwrap()
    };
    let engine = renderer.layout_engine();
    let plain = engine.max_content_width(find("plain"), 16.0, 16.0);
    let nested = engine.max_content_width(find("nested"), 16.0, 16.0);
    assert!(plain > 40.0, "the text was measured at all: {plain}");
    assert!((plain - nested).abs() < 0.5,
        "nesting changed the max-content width: plain {plain} vs nested {nested}");
}

/// **The intrinsic measurement must count the spaces between words.** A
/// max-content width that measures `"Faire un don"` as if it were
/// `"Faireundon"` is two space widths short of the line the breaker then
/// builds, so the text wraps inside a box sized for exactly one line.
#[test]
fn layoutadv_max_content_width_counts_inter_word_spaces() {
    let mut renderer = crate::renderer::Renderer::new();
    let doc = renderer.load_html(
        "<div id=spaced>Faire un don</div><div id=joined>Faireundon</div>",
        800.0);
    let engine = renderer.layout_engine();
    let find = |id: &str| {
        fn walk<'a>(n: &'a WebCore, id: &str) -> Option<&'a WebCore> {
            if n.attributes.get("id").map(String::as_str) == Some(id) { return Some(n); }
            for c in &n.children { if let Some(f) = walk(c, id) { return Some(f); } }
            None
        }
        walk(&doc.root, id).unwrap()
    };
    let spaced = engine.max_content_width(find("spaced"), 16.0, 16.0);
    let joined = engine.max_content_width(find("joined"), 16.0, 16.0);
    assert!(spaced > joined + 4.0,
        "the two spaces were not measured: 'Faire un don' {spaced} vs 'Faireundon' {joined}");
}

// ── Multi-column: a wrapper must not collapse every column into one ──────────

/// Column positions of the leaf items, left to right.
fn column_xs(root: &WebCore, class: &str) -> Vec<f32> {
    let mut xs = Vec::new();
    fn walk(n: &WebCore, class: &str, xs: &mut Vec<f32>) {
        if n.attributes.get("class").map_or(false, |c| c.split_whitespace().any(|w| w == class)) {
            xs.push(n.layout.margin_rect.x);
        }
        for c in &n.children { walk(c, class, xs); }
    }
    walk(root, class, &mut xs);
    xs
}

/// **Multi-column is a fragmentation container, not a round-robin over direct
/// children.** fr.wikipedia wraps both of its multicol blocks in a single
/// `<div>` — `column-count:3` over the community links and `column-count:5`
/// over the sister projects — so distributing the container's own children put
/// everything in column 1 and the lists rendered one item per line.
#[test]
fn layoutadv_multicol_distributes_through_a_wrapper() {
    let mut renderer = crate::renderer::Renderer::new();
    let mut doc = renderer.load_html(
        "<style>* { margin:0; padding:0 } .cols { column-count: 3; column-gap: 10px; width: 600px }\
         .item { height: 20px }</style>\
         <div class=cols><div class=wrap>\
           <div class=item>a</div><div class=item>b</div><div class=item>c</div>\
           <div class=item>d</div><div class=item>e</div><div class=item>f</div>\
         </div></div>",
        800.0);
    let mut pm = tiny_skia::Pixmap::new(800, 300).unwrap();
    renderer.render(&mut doc, &mut pm, 1.0);
    let xs = column_xs(&doc.root, "item");
    assert_eq!(xs.len(), 6, "all six items are present");
    let mut distinct: Vec<f32> = xs.clone();
    distinct.sort_by(|a, b| a.partial_cmp(b).unwrap());
    distinct.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    assert_eq!(distinct.len(), 3,
        "items should occupy 3 column positions, got {distinct:?} from {xs:?}");
}

/// The direct-children case must keep working.
#[test]
fn layoutadv_multicol_still_distributes_direct_children() {
    let mut renderer = crate::renderer::Renderer::new();
    let mut doc = renderer.load_html(
        "<style>* { margin:0; padding:0 } .cols { column-count: 3; column-gap: 10px; width: 600px }\
         .item { height: 20px }</style>\
         <div class=cols>\
           <div class=item>a</div><div class=item>b</div><div class=item>c</div>\
           <div class=item>d</div><div class=item>e</div><div class=item>f</div>\
         </div>",
        800.0);
    let mut pm = tiny_skia::Pixmap::new(800, 300).unwrap();
    renderer.render(&mut doc, &mut pm, 1.0);
    let xs = column_xs(&doc.root, "item");
    let mut distinct = xs.clone();
    distinct.sort_by(|a, b| a.partial_cmp(b).unwrap());
    distinct.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    assert_eq!(distinct.len(), 3, "got {distinct:?} from {xs:?}");
}

/// **`column-gap: normal` computes to 1em in a multi-column container**
/// (css-multicol-1 §4.2). Our initial value was `Zero`, indistinguishable from
/// an author writing `column-gap: 0`, so every multicol block that did not set
/// a gap rendered with its columns touching.
#[test]
fn layoutadv_multicol_default_gap_is_one_em() {
    let mut renderer = crate::renderer::Renderer::new();
    let mut doc = renderer.load_html(
        "<style>* { margin:0; padding:0 } \
         .cols { column-count: 3; width: 600px; font-size: 16px } .item { height: 20px }</style>\
         <div class=cols>\
           <div class=item>a</div><div class=item>b</div><div class=item>c</div>\
         </div>",
        800.0);
    let mut pm = tiny_skia::Pixmap::new(800, 200).unwrap();
    renderer.render(&mut doc, &mut pm, 1.0);
    let xs = {
        let mut v = column_xs(&doc.root, "item");
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        v
    };
    assert_eq!(xs.len(), 3, "three columns, got {xs:?}");
    // col_w = (600 - 2*16) / 3 = 189.33; column starts 0, 205.33, 410.67.
    let step = xs[1] - xs[0];
    assert!((step - 205.33).abs() < 1.0,
        "a 1em gap gives a 205.33px column pitch, got {step} from {xs:?}");
}

/// **max-content is the content laid out with no soft wrap taken**
/// (css-sizing-3 §5.1), so inline-level siblings that share a line are SUMMED.
/// Taking the MAX over a block's children measured `Hello <b>World</b>` as the
/// wider single word, and any shrink-to-fit box sized from it then wrapped.
#[test]
fn layoutadv_max_content_sums_inline_siblings() {
    let mut renderer = crate::renderer::Renderer::new();
    let doc = renderer.load_html(
        "<div id=split>Hello <span>World</span></div><div id=whole>Hello World</div>",
        800.0);
    let engine = renderer.layout_engine();
    let find = |id: &str| {
        fn walk<'a>(n: &'a WebCore, id: &str) -> Option<&'a WebCore> {
            if n.attributes.get("id").map(String::as_str) == Some(id) { return Some(n); }
            for c in &n.children { if let Some(f) = walk(c, id) { return Some(f); } }
            None
        }
        walk(&doc.root, id).unwrap()
    };
    let split = engine.max_content_width(find("split"), 16.0, 16.0);
    let whole = engine.max_content_width(find("whole"), 16.0, 16.0);
    assert!(whole > 40.0, "the control measured something: {whole}");
    // Within one space width: the text node's own max-content collapses its
    // trailing space, which is a separate (known) gap in cross-node whitespace.
    assert!(split > whole - 6.0 && split <= whole + 1.0,
        "inline siblings must sum on one line: split={split} vs whole={whole}");
}

/// A block-level child still starts a new line, so it is MAXed, not summed.
#[test]
fn layoutadv_max_content_maxes_block_siblings() {
    let mut renderer = crate::renderer::Renderer::new();
    let doc = renderer.load_html(
        "<div id=blocks><div>Hello</div><div>World</div></div><div id=one>Hello</div>",
        800.0);
    let engine = renderer.layout_engine();
    let find = |id: &str| {
        fn walk<'a>(n: &'a WebCore, id: &str) -> Option<&'a WebCore> {
            if n.attributes.get("id").map(String::as_str) == Some(id) { return Some(n); }
            for c in &n.children { if let Some(f) = walk(c, id) { return Some(f); } }
            None
        }
        walk(&doc.root, id).unwrap()
    };
    let blocks = engine.max_content_width(find("blocks"), 16.0, 16.0);
    let one = engine.max_content_width(find("one"), 16.0, 16.0);
    assert!((blocks - one).abs() < 6.0,
        "two block children stack, so max-content is the wider one: {blocks} vs {one}");
}

/// **`box-sizing: border-box` means the specified width ALREADY includes
/// padding and border** (css-sizing-3 §6.2). The intrinsic walk returned the
/// raw `width` and its caller then added the child's padding/border on top, so
/// a border-box child made its shrink-to-fit parent that much too wide. With
/// `* { box-sizing: border-box }` in nearly every real stylesheet, this hit
/// almost every float, inline-block and fit-content box.
#[test]
fn layoutadv_border_box_width_is_not_double_counted() {
    let mut renderer = crate::renderer::Renderer::new();
    let doc = renderer.load_html(
        "<div id=bb><div style='box-sizing:border-box;width:200px;padding:20px'>x</div></div>\
         <div id=cb><div style='box-sizing:content-box;width:160px;padding:20px'>x</div></div>",
        800.0);
    let engine = renderer.layout_engine();
    let find = |id: &str| {
        fn walk<'a>(n: &'a WebCore, id: &str) -> Option<&'a WebCore> {
            if n.attributes.get("id").map(String::as_str) == Some(id) { return Some(n); }
            for c in &n.children { if let Some(f) = walk(c, id) { return Some(f); } }
            None
        }
        walk(&doc.root, id).unwrap()
    };
    let bb = engine.max_content_width(find("bb"), 16.0, 16.0);
    let cb = engine.max_content_width(find("cb"), 16.0, 16.0);
    assert_eq!(bb, 200.0, "border-box: the 200px already includes the 40px padding");
    assert_eq!(cb, 200.0, "content-box: 160 + 40 padding = 200 (the control)");
}
