// Image tests – ported from cpptests/test_images.cpp
// Only object-fit parsing + img tag parsing are portable.
// Image layout (bitmap, intrinsic dimensions, IsReplaced) skipped.
use rhtmledit::types::*;
use rhtmledit::parse_html;
use rhtmledit::css::apply_property;

fn find_box<'a>(root: &'a HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(found) = find_box(child, pred) { return Some(found); }
    }
    None
}

// ============================================================
// Image Parsing
// ============================================================

#[test]
fn img_element_parsed() {
    let doc = parse_html("<img src=\"test.png\" width=\"100\" height=\"50\">");
    let b = find_box(&doc.root, &|b| b.tag == "img");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.display, Display::InlineBlock);
}

#[test]
fn img_src_attribute() {
    let doc = parse_html("<img src=\"photo.jpg\">");
    let b = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(b.get_attr("src"), Some("photo.jpg"));
}

#[test]
fn img_width_height_attributes() {
    let doc = parse_html("<img src=\"test.png\" width=\"200\" height=\"100\">");
    let b = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(b.get_attr("width"), Some("200"));
    assert_eq!(b.get_attr("height"), Some("100"));
}

#[test]
fn img_is_void() {
    let doc = parse_html("<img src=\"test.png\">");
    let b = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert!(b.is_void());
}

// ============================================================
// Object-Fit Parsing
// ============================================================

#[test]
fn object_fit_contain() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "object-fit", "contain");
    assert_eq!(style.object_fit, ObjectFit::Contain);
}

#[test]
fn object_fit_cover() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "object-fit", "cover");
    assert_eq!(style.object_fit, ObjectFit::Cover);
}

#[test]
fn object_fit_none() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "object-fit", "none");
    assert_eq!(style.object_fit, ObjectFit::None);
}

#[test]
fn object_fit_scale_down() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "object-fit", "scale-down");
    assert_eq!(style.object_fit, ObjectFit::ScaleDown);
}

#[test]
fn object_fit_fill() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "object-fit", "fill");
    assert_eq!(style.object_fit, ObjectFit::Fill);
}

// ============================================================
// Missing tests ported from C++ test_images.cpp
// ============================================================

#[test]
fn img_with_inline_style() {
    let doc = parse_html("<img src=\"test.png\" style=\"width: 300px; height: 150px;\">");
    let b = find_box(&doc.root, &|b| b.tag == "img").unwrap();
    assert_eq!(b.style.width, rhtmledit::types::CssLength::Px(300.0));
    assert_eq!(b.style.height, rhtmledit::types::CssLength::Px(150.0));
}

#[test]
fn img_id_and_class() {
    let doc = parse_html("<img src=\"test.png\" id=\"logo\" class=\"banner\">");
    let b = find_box(&doc.root, &|b| b.tag == "img" && b.get_attr("id") == Some("logo"));
    assert!(b.is_some());
    assert_eq!(b.unwrap().get_attr("class"), Some("banner"));
}

#[test]
fn img_serialization_preserves_src() {
    use rhtmledit::html::serialize_html;
    let doc = parse_html("<p><img src=\"photo.jpg\"></p>");
    let html = serialize_html(&doc);
    assert!(html.contains("photo.jpg"));
}
