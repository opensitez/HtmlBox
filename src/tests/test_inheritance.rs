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
    
    let inner = find_box(&doc.root, &|b| b.id == "inner").expect("inner div not found");
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
