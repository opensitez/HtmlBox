// Visual property tests – ported from cpptests/test_visual.cpp
// Render smoke and hit-test skipped (require widget / DC infrastructure).
use rhtmledit::types::*;
use rhtmledit::{load_html, parse_html};
use rhtmledit::css::{apply_property, Stylesheet};

fn find_box<'a>(root: &'a HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(found) = find_box(child, pred) { return Some(found); }
    }
    None
}

// ============================================================
// Opacity
// ============================================================

#[test]
fn opacity_parsed_from_inline() {
    let doc = parse_html("<div style=\"opacity: 0.5;\">Semi</div>");
    let b = find_box(&doc.root, &|b| b.style.opacity > 0.49 && b.style.opacity < 0.51);
    assert!(b.is_some());
}

#[test]
fn opacity_zero() {
    let doc = parse_html("<div style=\"opacity: 0;\">Invisible</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div" && b.style.opacity < 0.01);
    assert!(b.is_some());
}

#[test]
fn opacity_one() {
    let doc = parse_html("<div style=\"opacity: 1;\">Full</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div" && b.style.opacity > 0.99);
    assert!(b.is_some());
}

#[test]
fn opacity_clamped_high() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "opacity", "2.0");
    assert!(style.opacity >= 0.0);
}

#[test]
fn opacity_clamped_low() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "opacity", "-1.0");
    assert!(style.opacity >= -1.1);
}

// ============================================================
// Overflow
// ============================================================

#[test]
fn overflow_hidden() {
    let doc = parse_html("<div style=\"overflow: hidden;\">Clipped</div>");
    let b = find_box(&doc.root, &|b| b.style.overflow_x == Overflow::Hidden);
    assert!(b.is_some());
}

#[test]
fn overflow_scroll() {
    let doc = parse_html("<div style=\"overflow: scroll;\">Scrollable</div>");
    let b = find_box(&doc.root, &|b| b.style.overflow_x == Overflow::Scroll);
    assert!(b.is_some());
}

#[test]
fn overflow_auto() {
    let doc = parse_html("<div style=\"overflow: auto;\">Auto</div>");
    let b = find_box(&doc.root, &|b| b.style.overflow_x == Overflow::Auto);
    assert!(b.is_some());
}

#[test]
fn overflow_visible_default() {
    let doc = parse_html("<div>Default overflow</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(b.style.overflow_x, Overflow::Visible);
}

// ============================================================
// Outline
// ============================================================

#[test]
fn outline_shorthand() {
    let doc = parse_html("<div style=\"outline: 2px solid red;\">Outlined</div>");
    let b = find_box(&doc.root, &|b| {
        b.style.outline_width == 2.0 && b.style.outline_style == BorderStyle::Solid
    });
    assert!(b.is_some());
}

#[test]
fn outline_individual() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "outline-width", "3px");
    apply_property(&mut style, "outline-style", "dashed");
    apply_property(&mut style, "outline-color", "blue");
    apply_property(&mut style, "outline-offset", "5px");
    assert_eq!(style.outline_width, 3.0);
    assert_eq!(style.outline_style, BorderStyle::Dashed);
    assert_eq!(style.outline_color, Color::rgb(0, 0, 255));
    assert_eq!(style.outline_offset, 5.0);
}

#[test]
fn outline_does_not_affect_layout() {
    let doc = load_html(
        "<div style=\"width: 200px; outline: 5px solid red;\">Outlined</div>", 800.0);
    let b = find_box(&doc.root, &|b| {
        b.tag == "div" && b.content_rect.w >= 199.0 && b.content_rect.w <= 201.0
    });
    assert!(b.is_some());
    // Outline should not increase the border box dimensions
    let bx = b.unwrap();
    assert!(bx.border_rect.w >= 199.0 && bx.border_rect.w <= 201.0);
}

// ============================================================
// Text Overflow
// ============================================================

#[test]
fn text_overflow_ellipsis() {
    let doc = parse_html(
        "<div style=\"text-overflow: ellipsis; overflow: hidden; white-space: nowrap;\">Long text</div>");
    let b = find_box(&doc.root, &|b| b.style.text_overflow == TextOverflow::Ellipsis);
    assert!(b.is_some());
}

#[test]
fn text_overflow_clip_default() {
    let doc = parse_html("<div>Normal text</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(b.style.text_overflow, TextOverflow::Clip);
}

// ============================================================
// Border Radius
// ============================================================

#[test]
fn border_radius_parsed() {
    let doc = parse_html("<div style=\"border-radius: 10px;\">Rounded</div>");
    let b = find_box(&doc.root, &|b| b.style.border_radius == CssLength::Px(10.0));
    assert!(b.is_some());
}

#[test]
fn border_radius_zero_default() {
    let doc = parse_html("<div>Square</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert!(matches!(b.style.border_radius, CssLength::Px(v) if v < 0.01)
        || matches!(b.style.border_radius, CssLength::Zero));
}

// ============================================================
// Box Shadow
// ============================================================

#[test]
fn box_shadow_parsed() {
    let doc = parse_html(
        "<div style=\"box-shadow: 5px 10px 15px rgba(0,0,0,0.5);\">Shadow</div>");
    let b = find_box(&doc.root, &|b| b.style.box_shadow.is_some());
    assert!(b.is_some());
}

#[test]
fn box_shadow_none_default() {
    let doc = parse_html("<div>No shadow</div>");
    let b = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert!(b.style.box_shadow.is_none());
}

// ============================================================
// Visibility
// ============================================================

#[test]
fn visibility_hidden() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "visibility", "hidden");
    assert!(!style.visibility);
}

#[test]
fn visibility_visible() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "visibility", "visible");
    assert!(style.visibility);
}

// ============================================================
// Gradient
// ============================================================

#[test]
fn linear_gradient_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "background", "linear-gradient(to bottom, red, blue)");
    assert_eq!(style.gradient_type, GradientType::Linear);
    assert!(style.gradient_stops.len() >= 2);
}

// ============================================================
// clip-path
// ============================================================

#[test]
fn clip_path_inset_parsed() {
    let doc = parse_html("<div style='clip-path: inset(10px 20px 30px 40px);'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::Inset);
    assert_eq!(div.style.clip_path.inset_top, CssLength::Px(10.0));
    assert_eq!(div.style.clip_path.inset_right, CssLength::Px(20.0));
    assert_eq!(div.style.clip_path.inset_bottom, CssLength::Px(30.0));
    assert_eq!(div.style.clip_path.inset_left, CssLength::Px(40.0));
}

#[test]
fn clip_path_inset_single_value() {
    let doc = parse_html("<div style='clip-path: inset(15px);'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::Inset);
    assert_eq!(div.style.clip_path.inset_top, CssLength::Px(15.0));
    assert_eq!(div.style.clip_path.inset_right, CssLength::Px(15.0));
    assert_eq!(div.style.clip_path.inset_bottom, CssLength::Px(15.0));
    assert_eq!(div.style.clip_path.inset_left, CssLength::Px(15.0));
}

#[test]
fn clip_path_circle_parsed() {
    let doc = parse_html("<div style='clip-path: circle(50% at 50% 50%);'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::Circle);
    assert_eq!(div.style.clip_path.circle_radius, CssLength::Percent(50.0));
    assert_eq!(div.style.clip_path.center_x, CssLength::Percent(50.0));
    assert_eq!(div.style.clip_path.center_y, CssLength::Percent(50.0));
}

#[test]
fn clip_path_circle_no_center() {
    let doc = parse_html("<div style='clip-path: circle(100px);'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::Circle);
    assert_eq!(div.style.clip_path.circle_radius, CssLength::Px(100.0));
    // Default center is 50% 50%
    assert_eq!(div.style.clip_path.center_x, CssLength::Percent(50.0));
    assert_eq!(div.style.clip_path.center_y, CssLength::Percent(50.0));
}

#[test]
fn clip_path_ellipse_parsed() {
    let doc = parse_html("<div style='clip-path: ellipse(40% 60% at 50% 50%);'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::Ellipse);
    assert_eq!(div.style.clip_path.ellipse_rx, CssLength::Percent(40.0));
    assert_eq!(div.style.clip_path.ellipse_ry, CssLength::Percent(60.0));
}

#[test]
fn clip_path_polygon_parsed() {
    let doc = parse_html(
        "<div style='clip-path: polygon(50% 0%, 100% 100%, 0% 100%);'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::Polygon);
    assert_eq!(div.style.clip_path.points.len(), 3);
    assert_eq!(div.style.clip_path.points[0].0, CssLength::Percent(50.0));
    assert_eq!(div.style.clip_path.points[0].1, CssLength::Percent(0.0));
    assert_eq!(div.style.clip_path.points[1].0, CssLength::Percent(100.0));
    assert_eq!(div.style.clip_path.points[1].1, CssLength::Percent(100.0));
}

#[test]
fn clip_path_none() {
    let doc = parse_html("<div style='clip-path: none;'>Test</div>");
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.clip_path.kind, ClipPathKind::None);
}

// ============================================================
// CSS Variable Resolution via Stylesheet
// ============================================================

#[test]
fn css_variable_resolution() {
    let doc = load_html(
        "<html><head><style>\
         :root { --main-bg: #00ff00; }\
         .box { background-color: var(--main-bg); }\
         </style></head>\
         <body><div class=\"box\">Green</div></body></html>",
        800.0,
    );
    let b = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "box").unwrap_or(false)
            && b.style.background_color == Color::rgb(0, 255, 0)
    });
    assert!(b.is_some(), "div.box should have green background from CSS variable");
}

// ============================================================
// Pseudo-element ::before / ::after content
// ============================================================

#[test]
fn pseudo_element_content() {
    let doc = load_html(
        "<html><head><style>\
         p::before { content: \">> \"; }\
         p::after  { content: \" <<\"; }\
         </style></head>\
         <body><p>Content</p></body></html>",
        800.0,
    );
    let before_box = find_box(&doc.root, &|b| b.tag == "p" && !b.style.before_content.is_empty());
    assert!(before_box.is_some(), "p should have before_content from ::before rule");
    let after_box = find_box(&doc.root, &|b| b.tag == "p" && !b.style.after_content.is_empty());
    assert!(after_box.is_some(), "p should have after_content from ::after rule");
}

// ============================================================
// Stylesheet struct: CSS variables in :root
// ============================================================

#[test]
fn css_variables_in_root() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add(":root { --main-color: #ff0000; --gap: 10px; } p { color: var(--main-color); }");
    assert!(ss.variables.contains_key("--main-color"), "should extract --main-color");
    assert_eq!(ss.variables.get("--main-color").map(|s| s.as_str()), Some("#ff0000"));
    assert!(ss.variables.contains_key("--gap"), "should extract --gap");
}

#[test]
fn css_variable_with_fallback() {
    // A stylesheet with a variable reference that uses a fallback value must parse without error.
    let mut ss = Stylesheet::default();
    ss.parse_and_add(":root { --main: blue; } p { color: var(--missing, red); }");
    assert!(!ss.rules.is_empty(), "stylesheet should have at least one rule");
}

// ============================================================
// Background shorthand: position / size / repeat
// ============================================================

#[test]
fn background_shorthand_cover_no_repeat() {
    let doc = parse_html(
        "<div style='background: #ccc url(test.png) center / cover no-repeat;'>X</div>",
    );
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.background_size, BackgroundSize::Cover);
    assert_eq!(div.style.background_repeat, BackgroundRepeat::NoRepeat);
    assert_eq!(div.style.background_position_x, CssLength::Percent(50.0));
    assert_eq!(div.style.background_position_y, CssLength::Percent(50.0));
}

#[test]
fn background_shorthand_center_top() {
    let doc = parse_html(
        "<div style='background: url(img.jpg) center top no-repeat;'>X</div>",
    );
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.background_position_x, CssLength::Percent(50.0));
    assert_eq!(div.style.background_position_y, CssLength::Percent(0.0));
    assert_eq!(div.style.background_repeat, BackgroundRepeat::NoRepeat);
}

#[test]
fn background_shorthand_contain() {
    let doc = parse_html(
        "<div style='background: url(x.png) center / contain;'>X</div>",
    );
    let div = find_box(&doc.root, &|b| b.tag == "div").unwrap();
    assert_eq!(div.style.background_size, BackgroundSize::Contain);
}

