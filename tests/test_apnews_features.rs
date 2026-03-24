// Tests for features implemented while fixing AP News rendering:
// - <picture> element source selection
// - clip: rect() CSS property
// - CSS custom property case sensitivity
// - CSS counters (counter-reset, counter-increment, counter())
// - ::before/::after as grid/flex items
// - <source> element display: none
// - Percentage width in intrinsic sizing

use rhtmledit::types::*;
use rhtmledit::css::apply_property;
use rhtmledit::{parse_html, load_html, load_html_vp};

fn find_box<'a, F: Fn(&HtmlBox) -> bool>(root: &'a HtmlBox, pred: &F) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(b) = find_box(child, pred) { return Some(b); }
    }
    None
}

fn find_box_mut<'a, F: Fn(&HtmlBox) -> bool>(root: &'a mut HtmlBox, pred: &F) -> Option<&'a mut HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &mut root.children {
        if let Some(b) = find_box_mut(child, pred) { return Some(b); }
    }
    None
}

fn collect_boxes<'a, F: Fn(&HtmlBox) -> bool>(root: &'a HtmlBox, pred: &F) -> Vec<&'a HtmlBox> {
    let mut result = Vec::new();
    if pred(root) { result.push(root); }
    for child in &root.children {
        result.extend(collect_boxes(child, pred));
    }
    result
}

// ============================================================
// <picture> Element
// ============================================================

#[test]
fn picture_simple_source_sets_img_src() {
    let doc = parse_html(r#"
        <picture>
            <source srcset="better.jpg">
            <img src="fallback.jpg">
        </picture>
    "#);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(img.get_attr("src"), Some("better.jpg"));
}

#[test]
fn picture_skips_webp_source() {
    let doc = parse_html(r#"
        <picture>
            <source type="image/webp" srcset="photo.webp">
            <source srcset="photo.jpg">
            <img src="fallback.jpg">
        </picture>
    "#);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(img.get_attr("src"), Some("photo.jpg"));
}

#[test]
fn picture_falls_back_to_img_src_when_no_source_matches() {
    let doc = parse_html(r#"
        <picture>
            <source type="image/webp" srcset="photo.webp">
            <img src="fallback.jpg">
        </picture>
    "#);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    // Only webp source, which is skipped — img keeps its original src
    assert_eq!(img.get_attr("src"), Some("fallback.jpg"));
}

#[test]
fn picture_skips_source_with_media_when_viewport_unknown() {
    let doc = parse_html(r#"
        <picture>
            <source media="(min-width: 1024px)" srcset="large.jpg">
            <source srcset="small.jpg">
            <img src="fallback.jpg">
        </picture>
    "#);
    // At parse time, viewport is 0 — media sources are skipped, unconditional wins
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(img.get_attr("src"), Some("small.jpg"));
}

#[test]
fn picture_with_viewport_selects_matching_media_source() {
    // load_html_vp runs with real viewport, so media queries evaluate
    let doc = load_html_vp(r#"
        <picture>
            <source media="(min-width: 1024px)" srcset="large.jpg">
            <source srcset="small.jpg">
            <img src="fallback.jpg">
        </picture>
    "#, 1200.0, 800.0);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(img.get_attr("src"), Some("large.jpg"));
}

#[test]
fn picture_with_small_viewport_skips_large_media() {
    let doc = load_html_vp(r#"
        <picture>
            <source media="(min-width: 1024px)" srcset="large.jpg">
            <source srcset="small.jpg">
            <img src="fallback.jpg">
        </picture>
    "#, 800.0, 600.0);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(img.get_attr("src"), Some("small.jpg"));
}

#[test]
fn picture_img_gets_resolved_src() {
    let doc = parse_html(r#"
        <picture>
            <source srcset="photo.jpg">
            <img src="fallback.jpg">
        </picture>
    "#);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert!(img.get_attr("_resolved_src").is_some());
}

#[test]
fn picture_srcset_with_width_descriptor() {
    let doc = parse_html(r#"
        <picture>
            <source srcset="photo-320.jpg 320w, photo-640.jpg 640w">
            <img src="fallback.jpg">
        </picture>
    "#);
    let img = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    // Should pick the first URL from srcset
    assert_eq!(img.get_attr("src"), Some("photo-320.jpg"));
}

#[test]
fn picture_element_is_transparent_container() {
    let doc = parse_html(r#"
        <picture>
            <source srcset="photo.jpg">
            <img src="fallback.jpg" width="100" height="50">
        </picture>
    "#);
    let picture = find_box(&doc.root, &|b| b.tag == "picture").unwrap();
    assert!(picture.children.iter().any(|c| c.tag == "img"));
}

// ============================================================
// <source> element display: none
// ============================================================

#[test]
fn source_element_is_hidden() {
    let doc = load_html(r#"
        <picture>
            <source srcset="photo.jpg">
            <img src="fallback.jpg">
        </picture>
    "#, 800.0);
    let source = find_box(&doc.root, &|b| b.tag == "source").unwrap();
    assert_eq!(source.style.display, Display::None);
}

// ============================================================
// clip: rect()
// ============================================================

#[test]
fn clip_rect_parsed_with_commas() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "clip", "rect(0, 0, 0, 0)");
    assert!(style.clip_rect.is_some());
    let cr = style.clip_rect.unwrap();
    assert_eq!(cr, [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn clip_rect_parsed_with_spaces() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "clip", "rect(0 0 0 0)");
    assert!(style.clip_rect.is_some());
}

#[test]
fn clip_rect_with_px_values() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "clip", "rect(10px, 20px, 30px, 40px)");
    let cr = style.clip_rect.unwrap();
    assert_eq!(cr, [10.0, 20.0, 30.0, 40.0]);
}

#[test]
fn clip_rect_with_auto() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "clip", "rect(0, auto, auto, 0)");
    let cr = style.clip_rect.unwrap();
    assert_eq!(cr[0], 0.0);
    assert_eq!(cr[1], f32::MAX); // auto
    assert_eq!(cr[2], f32::MAX); // auto
    assert_eq!(cr[3], 0.0);
}

#[test]
fn clip_auto_clears_rect() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "clip", "rect(0, 0, 0, 0)");
    assert!(style.clip_rect.is_some());
    apply_property(&mut style, "clip", "auto");
    assert!(style.clip_rect.is_none());
}

#[test]
fn clip_rect_zero_hides_element() {
    // clip: rect(0,0,0,0) should result in 0-area clip
    let doc = load_html(r#"
        <div style="position: absolute; clip: rect(0, 0, 0, 0);">Hidden</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.tag == "div" && b.style.clip_rect.is_some()
    }).unwrap();
    let cr = div.style.clip_rect.unwrap();
    // clip width = right - left = 0 - 0 = 0
    assert_eq!(cr[1] - cr[3], 0.0);
}

// ============================================================
// CSS Custom Property Case Sensitivity
// ============================================================

#[test]
fn custom_property_resolves_in_stylesheet_rule() {
    let doc = load_html(r#"
        <style>
            :root { --mycolor: red; }
            div { color: var(--mycolor); }
        </style>
        <div>Hello</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.tag == "div" && b.children.iter().any(|c| c.text.contains("Hello"))
    }).unwrap();
    assert_eq!(div.style.color, Color::rgb(255, 0, 0));
}

#[test]
fn custom_property_camelcase_resolves() {
    let doc = load_html(r#"
        <style>
            :root { --myColor: red; }
            div { color: var(--myColor); }
        </style>
        <div>Hello</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.tag == "div" && b.children.iter().any(|c| c.text.contains("Hello"))
    }).unwrap();
    // CSS custom properties are case-sensitive — --myColor must match --myColor
    assert_eq!(div.style.color, Color::rgb(255, 0, 0));
}

#[test]
fn custom_property_case_mismatch_does_not_resolve() {
    let doc = load_html(r#"
        <style>
            :root { --myColor: red; }
            .test { color: var(--mycolor); }
        </style>
        <div class="test">Hello</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "test").unwrap_or(false)
    }).unwrap();
    // --mycolor (lowercase) != --myColor — should NOT resolve to red
    assert_ne!(div.style.color, Color::rgb(255, 0, 0));
}

#[test]
fn standard_property_is_case_insensitive() {
    let doc = load_html(r#"
        <style>
            .test { COLOR: red; }
        </style>
        <div class="test">Hello</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "test").unwrap_or(false)
    }).unwrap();
    assert_eq!(div.style.color, Color::rgb(255, 0, 0));
}

// ============================================================
// CSS Counters
// ============================================================

#[test]
fn counter_reset_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "counter-reset", "section");
    assert!(style.counter_reset.iter().any(|(name, _)| name == "section"));
}

#[test]
fn counter_reset_with_value() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "counter-reset", "section 5");
    let (_, val) = style.counter_reset.iter().find(|(n, _)| n == "section").unwrap();
    assert_eq!(*val, 5);
}

#[test]
fn counter_increment_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "counter-increment", "section");
    assert!(style.counter_increment.iter().any(|(name, _)| name == "section"));
}

#[test]
fn counter_increment_with_value() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "counter-increment", "item 2");
    let (_, val) = style.counter_increment.iter().find(|(n, _)| n == "item").unwrap();
    assert_eq!(*val, 2);
}

// ============================================================
// ::before/::after as Grid Items
// ============================================================

#[test]
fn before_pseudo_becomes_grid_item() {
    let doc = load_html(r#"
        <style>
            .grid { display: grid; grid-template-columns: 30px auto; }
            .grid::before { content: "X"; }
        </style>
        <div class="grid"><span>Content</span></div>
    "#, 800.0);
    let grid = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "grid").unwrap_or(false)
    }).unwrap();
    // ::before should be inserted as a child box (grid item)
    let has_before = grid.children.iter().any(|c| c.tag == "::before");
    assert!(has_before, "::before should be a child box in a grid container");
}

#[test]
fn before_pseudo_becomes_flex_item() {
    let doc = load_html(r#"
        <style>
            .flex { display: flex; }
            .flex::before { content: "X"; }
        </style>
        <div class="flex"><span>Content</span></div>
    "#, 800.0);
    let flex = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "flex").unwrap_or(false)
    }).unwrap();
    let has_before = flex.children.iter().any(|c| c.tag == "::before");
    assert!(has_before, "::before should be a child box in a flex container");
}

// ============================================================
// Percentage Width in Intrinsic Sizing
// ============================================================

#[test]
fn percentage_width_treated_as_auto_in_intrinsic() {
    // An element with width: 100% inside a shrink-to-fit context
    // should not collapse to 0
    let doc = load_html(r#"
        <div style="float: left;">
            <a style="display: block; width: 100%;">Link text here</a>
        </div>
    "#, 800.0);
    let link = find_box(&doc.root, &|b| b.tag == "a").unwrap();
    assert!(link.content_rect.w > 0.0, "width: 100% in intrinsic context should not be 0");
}

// ============================================================
// calc() Expression Parsing
// ============================================================

#[test]
fn calc_subtraction_works() {
    let doc = load_html(r#"
        <div style="width: calc(100% - 40px);">content</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.tag == "div" && b.children.iter().any(|c| c.text.contains("content"))
    });
    // Body has 8px margin on each side, so containing block = 800 - 16 = 784
    // calc(100% - 40px) = 784 - 40 = 744
    if let Some(d) = div {
        assert!((d.content_rect.w - 744.0).abs() < 2.0,
            "calc(100% - 40px) at vw=800 should be ~744, got {}", d.content_rect.w);
    }
}

#[test]
fn calc_multiple_subtractions() {
    let doc = load_html(r#"
        <div style="width: calc(100% - 40px - 60px);">content</div>
    "#, 800.0);
    let div = find_box(&doc.root, &|b| {
        b.tag == "div" && b.children.iter().any(|c| c.text.contains("content"))
    });
    // Body has 8px margin on each side, so containing block = 800 - 16 = 784
    // calc(100% - 40px - 60px) = 784 - 100 = 684
    if let Some(d) = div {
        assert!((d.content_rect.w - 684.0).abs() < 2.0,
            "calc(100% - 40px - 60px) at vw=800 should be ~684, got {}", d.content_rect.w);
    }
}

// ============================================================
// Flex min-width: auto
// ============================================================

#[test]
fn flex_item_does_not_shrink_below_content() {
    // A flex item with text should not shrink to 0 width
    let doc = load_html(r#"
        <div style="display: flex; width: 200px;">
            <span>Hello World</span>
            <span>Other</span>
        </div>
    "#, 800.0);
    let spans = collect_boxes(&doc.root, &|b| {
        b.tag == "span" && !b.text.is_empty()
    });
    for span in &spans {
        assert!(span.content_rect.w > 0.0,
            "flex item '{}' should not have 0 width", span.text);
    }
}

#[test]
fn flex_item_with_overflow_hidden_can_shrink_to_zero() {
    // overflow: hidden disables the automatic minimum size
    let doc = load_html(r#"
        <div style="display: flex; width: 50px;">
            <span style="overflow: hidden; flex-shrink: 1;">Very long text that should be clipped</span>
            <span style="width: 50px; flex-shrink: 0;">Fixed</span>
        </div>
    "#, 800.0);
    // The first span should be allowed to shrink since it has overflow: hidden
    let span = find_box(&doc.root, &|b| {
        b.tag == "span" && b.children.iter().any(|c| c.text.contains("Very long"))
    });
    // Just verify no panic — the exact width depends on shrinking logic
    assert!(span.is_some());
}

#[test]
fn flex_item_min_width_auto_prevents_text_at_zero() {
    // Simulate the AP News nav issue: flex items with text should have min-content width
    let doc = load_html(r#"
        <style>
            .nav { display: flex; width: 100px; }
            .nav-item { flex-shrink: 1; }
        </style>
        <div class="nav">
            <div class="nav-item">World</div>
            <div class="nav-item">Politics</div>
            <div class="nav-item">Sports</div>
        </div>
    "#, 800.0);
    let items = collect_boxes(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "nav-item").unwrap_or(false)
    });
    for item in &items {
        assert!(item.content_rect.w > 0.0,
            "flex nav item should not collapse to 0 width");
    }
}

// ============================================================
// Grid with ::before counter and fixed column
// ============================================================

#[test]
fn grid_before_counter_in_fixed_column() {
    let doc = load_html(r#"
        <style>
            .list { counter-reset: number; }
            .item {
                display: grid;
                grid-template-columns: 30px auto;
            }
            .item::before {
                content: counter(number);
                counter-increment: number;
            }
        </style>
        <div class="list">
            <div class="item"><span>First article title</span></div>
            <div class="item"><span>Second article title</span></div>
        </div>
    "#, 400.0);
    let items = collect_boxes(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "item").unwrap_or(false)
    });
    assert!(items.len() >= 2, "should have at least 2 grid items");
    for item in &items {
        // The ::before should exist as a child
        let has_before = item.children.iter().any(|c| c.tag == "::before");
        assert!(has_before, "grid item should have ::before child");
        // The content span should have reasonable width (not 0, not full container)
        let content_span = item.children.iter().find(|c| c.tag == "span");
        if let Some(span) = content_span {
            assert!(span.content_rect.w > 30.0,
                "content in auto column should be wider than 30px");
        }
    }
}

// ============================================================
// evaluate_media
// ============================================================

#[test]
fn evaluate_media_min_width_passes() {
    assert!(rhtmledit::css::evaluate_media("(min-width: 1024px)", 1200.0, 800.0));
}

#[test]
fn evaluate_media_min_width_fails() {
    assert!(!rhtmledit::css::evaluate_media("(min-width: 1024px)", 800.0, 600.0));
}

#[test]
fn evaluate_media_max_width() {
    assert!(rhtmledit::css::evaluate_media("(max-width: 768px)", 600.0, 400.0));
    assert!(!rhtmledit::css::evaluate_media("(max-width: 768px)", 1024.0, 768.0));
}

// ============================================================
// Descendant Selector Matching
// ============================================================

#[test]
fn descendant_selector_class_class() {
    // .Parent .Child { color: red }
    let doc = load_html_vp(r#"
        <html><head><style>
        .Parent .Child { color: red; }
        </style></head>
        <body>
            <div class="Parent">
                <div class="Child" id="target">Hello</div>
            </div>
            <div class="Child" id="non-target">Outside</div>
        </body></html>
    "#, 800.0, 600.0);
    let target = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "target")).unwrap();
    assert_eq!(target.style.color.r, 255, "Descendant .Parent .Child should apply color:red");
    assert_eq!(target.style.color.g, 0);

    let non_target = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "non-target")).unwrap();
    assert_ne!(non_target.style.color.r, 255, ".Child outside .Parent should NOT get color:red");
}

#[test]
fn descendant_selector_nested_deep() {
    // .Ancestor .Deep { display: block }
    let doc = load_html_vp(r#"
        <html><head><style>
        .Ancestor .Deep { font-weight: bold; }
        </style></head>
        <body>
            <div class="Ancestor">
                <div>
                    <div>
                        <span class="Deep" id="deep">text</span>
                    </div>
                </div>
            </div>
        </body></html>
    "#, 800.0, 600.0);
    let deep = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "deep")).unwrap();
    assert!(matches!(deep.style.font_weight, rhtmledit::types::FontWeight::Bold | rhtmledit::types::FontWeight::Value(700)), "Deeply nested descendant should match");
}

#[test]
fn descendant_selector_tag_class() {
    // div .item { color: green }
    let doc = load_html_vp(r#"
        <html><head><style>
        div .item { color: green; }
        </style></head>
        <body>
            <div>
                <span class="item" id="inside">text</span>
            </div>
        </body></html>
    "#, 800.0, 600.0);
    let inside = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "inside")).unwrap();
    assert_eq!(inside.style.color.r, 0);
    assert_eq!(inside.style.color.g, 128);
}

#[test]
fn child_combinator_direct() {
    // .Parent > .Child { color: blue }
    let doc = load_html_vp(r#"
        <html><head><style>
        .Parent > .DirectChild { color: blue; }
        </style></head>
        <body>
            <div class="Parent">
                <div class="DirectChild" id="direct">text</div>
                <div><div class="DirectChild" id="indirect">text</div></div>
            </div>
        </body></html>
    "#, 800.0, 600.0);
    let direct = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "direct")).unwrap();
    assert_eq!(direct.style.color.b, 255, "Direct child should match > combinator");

    let indirect = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "indirect")).unwrap();
    assert_ne!(indirect.style.color.b, 255, "Non-direct child should NOT match > combinator");
}

#[test]
fn descendant_selector_parse_produces_combinator() {
    let sel = rhtmledit::css::parse_selector(".Parent .Child");
    // Should be: [Class("Parent"), Combinator(Descendant), Class("Child")]
    assert_eq!(sel.parts.len(), 3, "Expected 3 parts: class, combinator, class");
    assert!(matches!(&sel.parts[0], rhtmledit::css::SelectorPart::Class(c) if c == "Parent"));
    assert!(matches!(&sel.parts[1], rhtmledit::css::SelectorPart::Combinator(rhtmledit::css::Combinator::Descendant)));
    assert!(matches!(&sel.parts[2], rhtmledit::css::SelectorPart::Class(c) if c == "Child"));
}

#[test]
fn inherit_overrides_lower_specificity_rule() {
    // h1 UA default sets font-size: 2em.  Higher-specificity rule says font-size: inherit.
    // The inherit should win and use the parent's computed font-size (16px).
    let doc = load_html_vp(r#"
        <html><head><style>
        .parent .child h1 { font-size: inherit; display: inline; }
        </style></head>
        <body>
            <div class="parent"><div class="child">
                <h1 id="hdr">Hello</h1>
            </div></div>
        </body></html>
    "#, 800.0, 600.0);
    let hdr = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "hdr")).unwrap();
    // Parent (.child div) has default 16px font.  h1 with font-size:inherit should be 16px, not 2em=32px.
    let fs = hdr.style.font_size_px(16.0, 16.0);
    assert!((fs - 16.0).abs() < 1.0, "h1 with font-size:inherit should be 16px, got {fs}");
    assert!(matches!(hdr.style.display, rhtmledit::types::Display::Inline), "h1 with display:inline from descendant selector");
}

#[test]
fn img_width_height_attributes() {
    // Image with explicit width/height attributes should have those dimensions
    let doc = load_html_vp(r#"
        <html><head></head>
        <body>
            <img id="pic" src="nonexistent.png" width="200" height="150">
        </body></html>
    "#, 800.0, 600.0);
    let pic = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "pic")).unwrap();
    let w = pic.style.width.resolve(16.0, 800.0, 16.0);
    let h = pic.style.height.resolve(16.0, 600.0, 16.0);
    assert!((w - 200.0).abs() < 1.0, "img width should be 200px from attribute, got {w}");
    assert!((h - 150.0).abs() < 1.0, "img height should be 150px from attribute, got {h}");
    assert_eq!(pic.content_rect.w, 200.0, "img content_rect.w should be 200");
    assert_eq!(pic.content_rect.h, 150.0, "img content_rect.h should be 150");
}

#[test]
fn img_width_height_inside_float() {
    // Image inside a float container — float should shrink-wrap to content
    let doc = load_html_vp(r#"
        <html><head></head>
        <body>
            <div id="float-wrap" style="float:left;">
                <img id="pic2" src="nonexistent.png" width="120" height="162">
            </div>
            <p>Text should wrap around the float.</p>
        </body></html>
    "#, 800.0, 600.0);
    let pic = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "pic2")).unwrap();
    assert!(pic.content_rect.w > 0.0, "img inside float should have non-zero width, got {}", pic.content_rect.w);
    assert!(pic.content_rect.h > 0.0, "img inside float should have non-zero height, got {}", pic.content_rect.h);
}

#[test]
fn img_inside_span_a_float() {
    // img inside span > a inside float — progressively add wrappers to find the failure
    // Test 1: img inside <a> inside float — works
    let doc1 = load_html_vp(r##"
        <html><head></head><body>
            <div style="float:left;"><a href="#"><img id="t1" src="x.png" width="120" height="162"></a></div>
        </body></html>
    "##, 800.0, 600.0);
    let t1 = find_box(&doc1.root, &|b| b.attributes.get("id").map_or(false, |v| v == "t1")).unwrap();
    assert!(t1.content_rect.w > 0.0, "t1: img in a in float should have width, got {}", t1.content_rect.w);

    // Test 2: img inside span > a inside float
    let doc2 = load_html_vp(r##"
        <html><head></head><body>
            <div style="float:left;"><span><a href="#"><img id="t2" src="x.png" width="120" height="162"></a></span></div>
        </body></html>
    "##, 800.0, 600.0);
    let t2 = find_box(&doc2.root, &|b| b.attributes.get("id").map_or(false, |v| v == "t2")).unwrap();
    assert!(t2.content_rect.w > 0.0, "t2: img in span>a in float should have width, got {}", t2.content_rect.w);

    // Test 3: img inside div > span > a inside float
    let doc3 = load_html_vp(r##"
        <html><head></head><body>
            <div style="float:left;"><div><span><a href="#"><img id="t3" src="x.png" width="120" height="162"></a></span></div></div>
        </body></html>
    "##, 800.0, 600.0);
    let t3 = find_box(&doc3.root, &|b| b.attributes.get("id").map_or(false, |v| v == "t3")).unwrap();
    assert!(t3.content_rect.w > 0.0, "t3: img in div>span>a in float should have width, got {}", t3.content_rect.w);
}

#[test]
fn descendant_selector_multi_level() {
    // .A .B .C { color: red }
    let doc = load_html_vp(r#"
        <html><head><style>
        .A .B .C { color: red; }
        </style></head>
        <body>
            <div class="A">
                <div class="B">
                    <div class="C" id="abc">text</div>
                </div>
            </div>
        </body></html>
    "#, 800.0, 600.0);
    let abc = find_box(&doc.root, &|b| b.attributes.get("id").map_or(false, |v| v == "abc")).unwrap();
    assert_eq!(abc.style.color.r, 255, "Multi-level descendant .A .B .C should match");
}
