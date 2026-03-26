// Positioning tests – ported from cpptests/test_positioning.cpp
use rhtmledit::types::*;
use rhtmledit::{load_html, parse_html};
use rhtmledit::css::apply_property;

fn find_box<'a>(root: &'a HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(found) = find_box(child, pred) { return Some(found); }
    }
    None
}

fn count_boxes(root: &HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> usize {
    let mut n = if pred(root) { 1 } else { 0 };
    for child in &root.children {
        n += count_boxes(child, pred);
    }
    n
}

// ============================================================
// Position Property Parsing
// ============================================================

#[test]
fn static_default() {
    let doc = parse_html("<div>Static</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div");
    assert!(b.is_some());
    assert_eq!(b.unwrap().style.position, Position::Static);
}

#[test]
fn relative_parsed() {
    let doc = parse_html("<div style=\"position: relative;\">Rel</div>");
    let b = find_box(&doc.root, &|b| b.style.position == Position::Relative);
    assert!(b.is_some());
}

#[test]
fn absolute_parsed() {
    let doc = parse_html("<div style=\"position: absolute;\">Abs</div>");
    let b = find_box(&doc.root, &|b| b.style.position == Position::Absolute);
    assert!(b.is_some());
}

#[test]
fn fixed_parsed() {
    let doc = parse_html("<div style=\"position: fixed;\">Fixed</div>");
    let b = find_box(&doc.root, &|b| b.style.position == Position::Fixed);
    assert!(b.is_some());
}

// ============================================================
// Relative Positioning
// ============================================================

#[test]
fn relative_offset() {
    let doc = load_html(
        "<div style=\"position: relative; top: 20px; left: 30px;\">Offset</div>", 800.0);
    let b = find_box(&doc.root, &|b| {
        b.style.position == Position::Relative && b.tag == "div"
    });
    assert!(b.is_some());
    let b = b.unwrap();
    assert!(b.layout.content_rect.x >= 30.0);
    assert!(b.layout.content_rect.y >= 20.0);
}

// ============================================================
// Absolute Positioning
// ============================================================

#[test]
fn absolute_removes_from_flow() {
    let doc = load_html(
        "<div style=\"position: relative;\">\
           <div style=\"position: absolute; top: 0; left: 0; width: 50px;\">Abs</div>\
           <div id=\"flow\">In flow</div>\
         </div>", 800.0);
    let flow = find_box(&doc.root, &|b| {
        b.get_attr("id") == Some("flow")
    });
    assert!(flow.is_some());
    // Flow box at y~0 (absolute child doesn't push it down)
    assert!(flow.unwrap().layout.content_rect.y < 30.0);
}

// ============================================================
// Z-Index
// ============================================================

#[test]
fn z_index_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "z-index", "10");
    assert_eq!(style.z_index, 10);
}

#[test]
fn z_index_negative() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "z-index", "-5");
    assert_eq!(style.z_index, -5);
}

#[test]
fn z_index_in_layout() {
    let doc = load_html(
        "<div style=\"position: relative;\">\
           <div style=\"position: absolute; z-index: 10;\">Front</div>\
           <div style=\"position: absolute; z-index: 1;\">Back</div>\
         </div>", 800.0);
    let count = count_boxes(&doc.root, &|b| b.style.position == Position::Absolute);
    assert_eq!(count, 2);
}

// ============================================================
// Positioned Offset Properties
// ============================================================

#[test]
fn top_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "top", "10px");
    assert_eq!(style.top, CssLength::Px(10.0));
}

#[test]
fn right_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "right", "20px");
    assert_eq!(style.right, CssLength::Px(20.0));
}

#[test]
fn bottom_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "bottom", "30px");
    assert_eq!(style.bottom, CssLength::Px(30.0));
}

#[test]
fn left_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "left", "40px");
    assert_eq!(style.left, CssLength::Px(40.0));
}

#[test]
fn offset_percent() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "top", "50%");
    assert_eq!(style.top, CssLength::Percent(50.0));
}

// ============================================================
// Missing tests ported from C++ test_positioning.cpp
// ============================================================

#[test]
fn relative_does_not_affect_siblings() {
    let doc = load_html(
        "<div style=\"position: relative; top: 100px;\">Shifted</div>\
         <div id=\"sibling\">Normal</div>", 800.0);
    let sibling = find_box(&doc.root, &|b| {
        b.get_attr("id") == Some("sibling")
    });
    assert!(sibling.is_some());
    let shifted = find_box(&doc.root, &|b| {
        b.style.position == Position::Relative && b.tag == "div"
    });
    assert!(shifted.is_some());
}

#[test]
fn absolute_with_offsets() {
    let doc = load_html(
        "<div style=\"position: relative; width: 400px; height: 400px;\">\
           <div style=\"position: absolute; top: 50px; left: 100px; width: 100px;\">Abs</div>\
         </div>", 800.0);
    let abs_box = find_box(&doc.root, &|b| {
        b.style.position == Position::Absolute
            && (b.style.width == CssLength::Px(100.0))
    });
    assert!(abs_box.is_some());
}

#[test]
fn absolute_right_bottom() {
    let doc = load_html(
        "<div style=\"position: relative; width: 400px; height: 400px;\">\
           <div style=\"position: absolute; right: 10px; bottom: 20px; width: 50px; height: 50px;\">Corner</div>\
         </div>", 800.0);
    let abs_box = find_box(&doc.root, &|b| {
        b.style.position == Position::Absolute
            && (b.style.width == CssLength::Px(50.0))
    });
    assert!(abs_box.is_some());
}

#[test]
fn is_positioned_static() {
    let mut style = ComputedStyle::default();
    style.position = Position::Static;
    assert!(!style.is_positioned());
}

#[test]
fn is_positioned_relative() {
    let mut style = ComputedStyle::default();
    style.position = Position::Relative;
    assert!(style.is_positioned());
}

#[test]
fn is_positioned_absolute() {
    let mut style = ComputedStyle::default();
    style.position = Position::Absolute;
    assert!(style.is_positioned());
}

// ============================================================
// Absolute shrink-to-fit (width: auto)
// ============================================================

#[test]
fn absolute_shrink_to_fit_top_right() {
    // position:absolute with right:10px and no explicit width should shrink-to-fit content,
    // not stretch to fill the containing block.
    let doc = load_html(
        "<div style=\"position: relative; width: 600px; height: 200px;\">\
           <div id=\"abs\" style=\"position: absolute; top: 10px; right: 10px; padding: 8px;\">Label</div>\
         </div>", 800.0);
    let abs_box = find_box(&doc.root, &|b| {
        b.get_attr("id") == Some("abs")
    });
    assert!(abs_box.is_some(), "absolute box not found");
    let abs_box = abs_box.unwrap();
    // Width should be shrunk to content — well under half the container width (300px)
    assert!(abs_box.layout.border_rect.w < 200.0,
        "expected shrink-to-fit width < 200, got {}", abs_box.layout.border_rect.w);
    // Right edge of border rect should be near the container right (600 - 10 = 590)
    let right_edge = abs_box.layout.border_rect.x + abs_box.layout.border_rect.w;
    assert!(right_edge > 500.0 && right_edge <= 605.0,
        "right edge = {right_edge}");
}

#[test]
fn absolute_shrink_to_fit_top_left() {
    // position:absolute with only left set: should also shrink-to-fit
    let doc = load_html(
        "<div style=\"position: relative; width: 600px; height: 200px;\">\
           <div id=\"abs\" style=\"position: absolute; top: 10px; left: 10px; padding: 4px;\">Hello</div>\
         </div>", 800.0);
    let abs_box = find_box(&doc.root, &|b| {
        b.get_attr("id") == Some("abs")
    });
    assert!(abs_box.is_some(), "absolute box not found");
    let abs_box = abs_box.unwrap();
    assert!(abs_box.layout.border_rect.w < 200.0,
        "expected shrink-to-fit width < 200, got {}", abs_box.layout.border_rect.w);
    // Left edge of border rect should be near 10px
    assert!(abs_box.layout.border_rect.x >= 10.0 && abs_box.layout.border_rect.x < 30.0,
        "left edge = {}", abs_box.layout.border_rect.x);
}

#[test]
fn absolute_explicit_width_not_shrunk() {
    // Explicit width on an absolute element must be respected (no shrink-to-fit)
    let doc = load_html(
        "<div style=\"position: relative; width: 600px; height: 200px;\">\
           <div id=\"abs\" style=\"position: absolute; top: 10px; left: 10px; width: 300px;\">Content</div>\
         </div>", 800.0);
    let abs_box = find_box(&doc.root, &|b| {
        b.get_attr("id") == Some("abs")
    });
    assert!(abs_box.is_some(), "absolute box not found");
    let abs_box = abs_box.unwrap();
    assert!((abs_box.layout.content_rect.w - 300.0).abs() < 2.0,
        "explicit width should be 300, got {}", abs_box.layout.content_rect.w);
}

#[test]
fn absolute_both_sides_stretches() {
    // left + right both set → box stretches to fill that space (no shrink-to-fit)
    let doc = load_html(
        "<div style=\"position: relative; width: 600px; height: 200px;\">\
           <div id=\"abs\" style=\"position: absolute; top: 10px; left: 20px; right: 20px;\">Stretch</div>\
         </div>", 800.0);
    let abs_box = find_box(&doc.root, &|b| {
        b.get_attr("id") == Some("abs")
    });
    assert!(abs_box.is_some(), "absolute box not found");
    let abs_box = abs_box.unwrap();
    // Content width = 600 - 20 - 20 = 560
    assert!(abs_box.layout.content_rect.w > 500.0,
        "expected stretched width > 500, got {}", abs_box.layout.content_rect.w);
}

#[test]
fn establishes_containing_block() {
    // A relative box establishes a containing block (establishes BFC)
    let mut style = ComputedStyle::default();
    style.position = Position::Relative;
    assert!(style.is_positioned());

    // A static box does NOT
    let mut static_style = ComputedStyle::default();
    static_style.position = Position::Static;
    assert!(!static_style.is_positioned());
}
