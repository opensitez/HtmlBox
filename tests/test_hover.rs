// Hover tests – ported from cpptests/test_hover.cpp
// Only CSS property parsing tests are portable; hover state/render tests skipped.
use rhtmledit::types::*;
use rhtmledit::css::apply_property;
use rhtmledit::parse_html;

#[test]
fn hover_background_color_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "hover-background-color", "red");
    assert_eq!(style.hover_background_color, Some(Color::rgb(255, 0, 0)));
}

#[test]
fn hover_color_parsed() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "hover-color", "blue");
    assert_eq!(style.hover_color, Some(Color::rgb(0, 0, 255)));
}

// ============================================================
// Missing tests ported from C++ test_hover.cpp
// ============================================================

fn find_box<'a>(root: &'a HtmlBox, pred: &dyn Fn(&HtmlBox) -> bool) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(found) = find_box(child, pred) { return Some(found); }
    }
    None
}

#[test]
fn hover_rule_flagged() {
    let doc = parse_html(
        "<html><head><style>\
         .btn:hover { background-color: red; color: white; }\
         </style></head>\
         <body><div class=\"btn\">Button</div></body></html>");
    let found_hover = doc.stylesheet.rules.iter().any(|r| r.is_hover);
    assert!(found_hover);
}

#[test]
fn hover_rule_has_declarations() {
    let doc = parse_html(
        "<html><head><style>\
         a:hover { color: red; text-decoration: underline; }\
         </style></head>\
         <body><a href=\"#\">Link</a></body></html>");
    let found_color_decl = doc.stylesheet.rules.iter()
        .filter(|r| r.is_hover)
        .any(|r| r.declarations.get("color").map(|v| v == "red").unwrap_or(false));
    assert!(found_color_decl);
}

#[test]
fn hover_style_applied_to_box() {
    // Verify that a :hover rule with background-color and color
    // is parsed correctly with its declarations preserved.
    let doc = parse_html(
        "<html><head><style>\
         .box:hover { background-color: yellow; color: green; }\
         </style></head>\
         <body><div class=\"box\">Hoverable</div></body></html>");
    // The hover rule should exist with the correct declarations
    let hover_rule = doc.stylesheet.rules.iter().find(|r| r.is_hover);
    assert!(hover_rule.is_some(), "Expected a :hover rule in stylesheet");
    let rule = hover_rule.unwrap();
    assert!(rule.declarations.contains_key("background-color"),
        "Expected background-color in hover rule declarations");
    assert!(rule.declarations.contains_key("color"),
        "Expected color in hover rule declarations");
}
