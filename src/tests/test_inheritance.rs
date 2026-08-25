use crate::types::*;
use super::harness::*;

#[test]
fn test_font_size_inheritance_em() {
    // Basic em resolution and inheritance
    let html = r#"
        <div style="font-size: 2em;">
            <p>Nested</p>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    
    // Parent div should be 2 * 16px = 32px
    let div = find_box(&doc.root, &|b| b.tag == "div").expect("div not found");
    assert_eq!(div.style.font_size, CssLength::Px(32.0));
    
    // Child p should inherit 32px as Px, NOT 2em
    let p = find_box(&doc.root, &|b| b.tag == "p").expect("p not found");
    assert_eq!(p.style.font_size, CssLength::Px(32.0));
}

#[test]
fn test_nested_em_scaling() {
    // Nested em should scale correctly: 2em * 2em = 4em = 64px
    let html = r#"
        <div style="font-size: 2em;">
            <div id="inner" style="font-size: 2em;">
                Double scaled
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    
    let inner = find_box(&doc.root, &|b| b.attributes.get("id").map(|v| v == "inner").unwrap_or(false)).expect("inner div not found");
    // Outer div resolved to 32px. Inner div resolves 2em against 32px -> 64px.
    assert_eq!(inner.style.font_size, CssLength::Px(64.0));
}

#[test]
fn test_h1_ua_font_size() {
    // UA stylesheet specifies h1 { font-size: 2em; }
    let html = "<h1>Heading</h1>";
    let doc = parse_and_layout(html, 800.0);
    
    let h1 = find_box(&doc.root, &|b| b.tag == "h1").expect("h1 not found");
    // Default root is 16px, h1 is 2em -> 32px
    assert_eq!(h1.style.font_size, CssLength::Px(32.0));
}

#[test]
fn test_rem_resolution() {
    // rem should resolve against root font size (16px) regardless of nesting
    let html = r#"
        <div style="font-size: 2em;">
            <p style="font-size: 1rem;">Nested rem</p>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    
    let p = find_box(&doc.root, &|b| b.tag == "p").expect("p not found");
    assert_eq!(p.style.font_size, CssLength::Px(16.0));
}

#[test]
fn test_pt_resolution() {
    // pt should resolve to 4/3 px (12pt = 16px)
    let html = r#"<p style="font-size: 12pt;">12pt text</p>"#;
    let doc = parse_and_layout(html, 800.0);
    
    let p = find_box(&doc.root, &|b| b.tag == "p").expect("p not found");
    assert_eq!(p.style.font_size, CssLength::Px(16.0));
}

// ── text-shadow inheritance ───────────────────────────────────────────────────

#[test]
fn test_text_shadow_inherited_by_text_node() {
    // text-shadow is an inherited property: #text children must carry it so the
    // renderer can apply the shadow when drawing the run.
    let html = r#"<p style="text-shadow: 2px 2px 4px rgba(0,0,0,0.3);">Hello</p>"#;
    let doc = parse_and_layout(html, 800.0);

    let p = find_box(&doc.root, &|b| b.tag == "p").expect("p not found");
    assert!(p.style.text_shadow.is_some(), "p should have text_shadow");

    // The #text child must inherit it
    let text_node = find_box(&doc.root, &|b| b.tag == "#text").expect("#text not found");
    let ts = text_node.style.text_shadow.as_ref()
        .expect("#text must inherit text_shadow from parent");
    assert!((ts.offset_x - 2.0).abs() < 0.01, "offset_x should be 2");
    assert!((ts.offset_y - 2.0).abs() < 0.01, "offset_y should be 2");
    assert!((ts.blur - 4.0).abs() < 0.01,     "blur should be 4");
    assert_eq!(ts.color.a, 76,                 "alpha ~ 0.3*255 = 76");
}

#[test]
fn test_text_shadow_not_inherited_across_block_boundary() {
    // text-shadow IS inherited but a sibling block should not get it
    let html = r#"
        <p style="text-shadow: 1px 1px black;">Shadow here</p>
        <p>No shadow here</p>
    "#;
    let doc = parse_and_layout(html, 800.0);

    let paras: Vec<_> = {
        let mut v = Vec::new();
        let mut collect = |b: &WebCore| {
            if b.tag == "p" { v.push(b.style.text_shadow.is_some()); }
        };
        walk_boxes(&doc.root, &mut collect);
        v
    };
    assert_eq!(paras.len(), 2);
    assert!(paras[0],  "first p should have text_shadow");
    assert!(!paras[1], "second p should NOT have text_shadow");
}

// ── background-color propagation to inline runs ───────────────────────────────

#[test]
fn test_inline_span_background_propagated_to_text_run() {
    // An inline span with background-color: the background_color must be visible
    // on its child #text node so the renderer draws the highlight correctly.
    let html = r#"<p><span style="background-color: yellow;">highlighted</span></p>"#;
    let doc = parse_and_layout(html, 800.0);

    // The span itself should have the colour
    let span = find_box(&doc.root, &|b| b.tag == "span").expect("span not found");
    assert_eq!(span.style.background_color, Color::rgb(255, 255, 0),
               "span should have yellow background");

    // Every inline run collected from the span should carry the background colour.
    let p = find_box(&doc.root, &|b| b.tag == "p").expect("p not found");
    assert!(!p.layout.inline_runs.is_empty(), "p should have inline runs");
    for run in &p.layout.inline_runs {
        assert_eq!(run.style.background_color, Color::rgb(255, 255, 0),
                   "all runs under highlighted span should carry yellow background");
    }
}
