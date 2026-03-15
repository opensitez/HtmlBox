use crate::tests::harness::{parse_and_layout, find_box};
use crate::types::*;

pub fn find_by_id<'a>(node: &'a HtmlBox, id: &str) -> Option<&'a HtmlBox> {
    find_box(node, &|b| b.attributes.get("id").map(|s| s == id).unwrap_or(false))
}

#[test]
fn test_grid_auto_tracks() {
    let html = r#"
        <div style="display: grid; grid-template-columns: auto auto; width: 400px; font-size: 16px;">
            <div id="c1" style="background: red;">Longer Text Content</div>
            <div id="c2" style="background: blue;">Short</div>
        </div>
    "#;
    let doc = parse_and_layout(html, 400.0);
    let c1 = find_by_id(&doc.root, "c1").unwrap();
    let c2 = find_by_id(&doc.root, "c2").unwrap();

    // With auto auto, items should take their content width.
    // "Longer Text Content" is ~19 chars. "Short" is 5 chars.
    // In current buggy implementation, they might get 200px each (equal share).
    assert!(c1.border_rect.w > c2.border_rect.w);
}

#[test]
fn test_grid_item_stretch_background() {
    let html = r#"
        <div style="display: grid; grid-template-columns: 100px 100px; width: 200px; font-size: 16px;">
            <div id="c1" style="background: red; display: inline-block;">Small</div>
            <div id="c2" style="background: blue;">Block</div>
        </div>
    "#;
    let doc = parse_and_layout(html, 200.0);
    let c1 = find_by_id(&doc.root, "c1").unwrap();
    let c2 = find_by_id(&doc.root, "c2").unwrap();

    // Default justify-self is stretch.
    // c1 is inline-block, so it might shrink to fit if not forced to stretch.
    // c2 is block, so it should stretch naturally.
    assert_eq!(c1.border_rect.w, 100.0);
    assert_eq!(c2.border_rect.w, 100.0);
}

#[test]
fn test_grid_fr_with_auto() {
    let html = r#"
        <div style="display: grid; grid-template-columns: auto 1fr; width: 400px; gap: 0;">
            <div id="c1" style="width: 100px;">Fixed</div>
            <div id="c2">Flexible</div>
        </div>
    "#;
    let doc = parse_and_layout(html, 400.0);
    let c1 = find_by_id(&doc.root, "c1").unwrap();
    let c2 = find_by_id(&doc.root, "c2").unwrap();

    // c1 should be 100px, c2 should be 300px.
    // In current buggy implementation, if there is an 'fr', 'auto' gets 0.
    assert_eq!(c1.border_rect.w, 100.0);
    assert_eq!(c2.border_rect.w, 300.0);
}
