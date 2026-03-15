// Tests for link parsing and hit-testing.

use rhtmledit::types::*;
use rhtmledit::parse_html;
use rhtmledit::layout::hit_test::*;
use rhtmledit::layout::LayoutEngine;

fn layout(html: &str, width: f32) -> Document {
    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, width);
    doc
}

#[test]
fn link_parsing_href() {
    let doc = parse_html(r#"<a href="http://example.com">Click</a>"#);
    let a = rhtmledit::dom::query_selector(&doc.root, "a").expect("a tag not found");
    assert_eq!(a.tag, "a");
    assert_eq!(a.attributes.get("href").unwrap(), "http://example.com");
}

#[test]
fn link_hit_test() {
    let doc = layout(r#"<p><a href="http://example.com">Link Text</a></p>"#, 800.0);
    
    // Find the link text position
    let text = doc.root.text_content();
    let pos = text.find("Link").unwrap();
    
    // offset_to_point requires (root, box_ptr, local_offset, scroll_x, scroll_y)
    // For simplicity, let's find the box containing the link
    let a_box = doc.root.query_selector_all("a")[0];
    let pt = offset_to_point(&doc.root, a_box as *const HtmlBox, 0, 0.0, 0.0).unwrap();
    
    let url = hit_test_link(&doc.root, (pt.0 + 2.0, pt.1 + 2.0), 0);
    assert_eq!(url, Some("http://example.com".to_string()));
}

#[test]
fn link_hit_test_no_link() {
    let doc = layout("<p>No link here</p>", 800.0);
    let url = hit_test_link(&doc.root, (10.0, 10.0), 0);
    assert!(url.is_none());
}
