// Ported from tests/test_css.cpp

use crate::css::{parse_declarations, parse_stylesheet, parse_selector, apply_property, Stylesheet, PseudoElement};
use crate::types::*;
use super::harness::*;

// ── CSS Declaration Parsing ───────────────────────────────────────────────────

#[test]
fn css_basic_declarations() {
    let decls = parse_declarations("color: red; font-size: 16px;");
    assert!(decls.len() >= 2);
    assert_eq!(decls.get("color").map(|s| s.as_str()), Some("red"));
    assert_eq!(decls.get("font-size").map(|s| s.as_str()), Some("16px"));
}

#[test]
fn css_empty_declarations() {
    let decls = parse_declarations("");
    assert_eq!(decls.len(), 0);
}

#[test]
fn css_trailing_semicolon() {
    let decls = parse_declarations("color: red;");
    assert!(decls.len() >= 1);
}

#[test]
fn css_no_semicolon() {
    let decls = parse_declarations("color: red");
    assert!(decls.len() >= 1);
    assert_eq!(decls.get("color").map(|s| s.as_str()), Some("red"));
}

#[test]
fn css_multiple_values() {
    let decls = parse_declarations("margin: 10px 20px 30px 40px;");
    assert!(decls.len() >= 1);
    assert!(decls.contains_key("margin"));
}

// ── Stylesheet Parsing ────────────────────────────────────────────────────────

#[test]
fn css_stylesheet_rules() {
    let ss = parse_stylesheet("p { color: blue; } .big { font-size: 24px; }").unwrap();
    assert!(ss.len() >= 2);
}

#[test]
fn css_stylesheet_multiple_selectors() {
    let ss = parse_stylesheet("h1, h2, h3 { font-weight: bold; }").unwrap();
    assert!(ss.len() >= 1);
}

#[test]
fn css_important_stripped_from_color() {
    let decls = parse_declarations("color: #ff0000 !important;");
    assert_eq!(decls.len(), 1);
    assert_eq!(decls.get("color").map(|s| s.as_str()), Some("#ff0000"));
}

#[test]
fn css_important_stripped_from_multiple() {
    let decls = parse_declarations(
        "background: #21262d !important; color: #6e7681 !important; cursor: default;",
    );
    assert_eq!(decls.len(), 3);
    assert_eq!(decls.get("background").map(|s| s.as_str()), Some("#21262d"));
    assert_eq!(decls.get("color").map(|s| s.as_str()), Some("#6e7681"));
    assert_eq!(decls.get("cursor").map(|s| s.as_str()), Some("default"));
}

#[test]
fn css_important_color_applied() {
    let doc = parse(
        r#"<html><head><style>.red { color: #ff0000 !important; }</style></head>
           <body><p class="red">Text</p></body></html>"#,
    );
    let p = find_box(&doc.root, &|b| b.tag == "p");
    assert!(p.is_some());
    let c = p.unwrap().style.color;
    assert_eq!(c.r, 255);
    assert_eq!(c.g, 0);
}

#[test]
fn css_important_background_applied() {
    let doc = parse(
        r#"<html><head><style>.bg { background-color: #334155 !important; }</style></head>
           <body><div class="bg">Box</div></body></html>"#,
    );
    let div = find_box(&doc.root, &|b| b.tag == "div");
    assert!(div.is_some());
    let c = div.unwrap().style.background_color;
    assert_eq!(c.r, 0x33);
}

// ── Selector Parsing ──────────────────────────────────────────────────────────

#[test]
fn css_selector_with_class() {
    let sel = parse_selector("div.container");
    assert!(!sel.parts.is_empty());
    // should have both a tag part and class part
    use crate::css::SelectorPart;
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Tag(t) if t == "div")));
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Class(c) if c == "container")));
}

#[test]
fn css_selector_with_id() {
    let sel = parse_selector("#main");
    use crate::css::SelectorPart;
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Id(id) if id == "main")));
}

#[test]
fn css_selector_multiple_classes() {
    let sel = parse_selector(".foo.bar.baz");
    use crate::css::SelectorPart;
    let classes: Vec<_> = sel.parts.iter().filter_map(|p| {
        if let SelectorPart::Class(c) = p { Some(c.as_str()) } else { None }
    }).collect();
    assert!(classes.len() >= 3);
}

#[test]
fn css_selector_descendant_combinator() {
    let sel = parse_selector("div p");
    use crate::css::{SelectorPart, Combinator};
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::Descendant))));
}

#[test]
fn css_selector_child_combinator() {
    let sel = parse_selector("div > p");
    use crate::css::{SelectorPart, Combinator};
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::Child))));
}

#[test]
fn css_selector_adjacent_sibling() {
    let sel = parse_selector("h1 + p");
    use crate::css::{SelectorPart, Combinator};
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::AdjacentSibling))));
}

#[test]
fn css_selector_general_sibling() {
    let sel = parse_selector("h1 ~ p");
    use crate::css::{SelectorPart, Combinator};
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::GeneralSibling))));
}

#[test]
fn css_specificity() {
    let sel1 = parse_selector("#main");
    let sel2 = parse_selector(".container");
    let sel3 = parse_selector("div");
    assert!(sel1.specificity() > sel2.specificity());
    assert!(sel2.specificity() > sel3.specificity());
}

// ── CSS Property Application ──────────────────────────────────────────────────

#[test]
fn css_display_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "display", "inline");
    assert_eq!(style.display, Display::Inline);
    apply_property(&mut style, "display", "inline-block");
    assert_eq!(style.display, Display::InlineBlock);
    apply_property(&mut style, "display", "flex");
    assert_eq!(style.display, Display::Flex);
    apply_property(&mut style, "display", "grid");
    assert_eq!(style.display, Display::Grid);
    apply_property(&mut style, "display", "none");
    assert_eq!(style.display, Display::None);
    apply_property(&mut style, "display", "list-item");
    assert_eq!(style.display, Display::ListItem);
}

#[test]
fn css_overflow_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "overflow", "hidden");
    assert_eq!(style.overflow_x, Overflow::Hidden);
    assert_eq!(style.overflow_y, Overflow::Hidden);
    apply_property(&mut style, "overflow", "scroll");
    assert_eq!(style.overflow_x, Overflow::Scroll);
    apply_property(&mut style, "overflow", "auto");
    assert_eq!(style.overflow_x, Overflow::Auto);
    apply_property(&mut style, "overflow", "visible");
    assert_eq!(style.overflow_x, Overflow::Visible);
}

#[test]
fn css_opacity_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "opacity", "0.5");
    assert!((style.opacity - 0.5).abs() < 0.01);
    apply_property(&mut style, "opacity", "0");
    assert!(style.opacity < 0.01);
    apply_property(&mut style, "opacity", "1");
    assert!(style.opacity > 0.99);
}

#[test]
fn css_position_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "position", "static");
    assert_eq!(style.position, Position::Static);
    apply_property(&mut style, "position", "relative");
    assert_eq!(style.position, Position::Relative);
    apply_property(&mut style, "position", "absolute");
    assert_eq!(style.position, Position::Absolute);
    apply_property(&mut style, "position", "fixed");
    assert_eq!(style.position, Position::Fixed);
}

#[test]
fn css_z_index_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "z-index", "10");
    assert_eq!(style.z_index, 10);
    apply_property(&mut style, "z-index", "-5");
    assert_eq!(style.z_index, -5);
}

#[test]
fn css_border_radius_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "border-radius", "8px");
    assert_eq!(style.border_radius, CssLength::Px(8.0));
}

#[test]
fn css_vertical_align_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "vertical-align", "middle");
    assert_eq!(style.vertical_align, VerticalAlign::Middle);
    apply_property(&mut style, "vertical-align", "top");
    assert_eq!(style.vertical_align, VerticalAlign::Top);
    apply_property(&mut style, "vertical-align", "bottom");
    assert_eq!(style.vertical_align, VerticalAlign::Bottom);
    apply_property(&mut style, "vertical-align", "super");
    assert_eq!(style.vertical_align, VerticalAlign::Super);
    apply_property(&mut style, "vertical-align", "sub");
    assert_eq!(style.vertical_align, VerticalAlign::Sub);
}

#[test]
fn css_list_style_type_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "list-style-type", "decimal");
    assert_eq!(style.list_style_type, ListStyleType::Decimal);
    apply_property(&mut style, "list-style-type", "circle");
    assert_eq!(style.list_style_type, ListStyleType::Circle);
    apply_property(&mut style, "list-style-type", "none");
    assert_eq!(style.list_style_type, ListStyleType::None);
}

// ── Stylesheet struct: CSS variables ──────────────────────────────────────────

#[test]
fn css_variables_in_root() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add(":root { --main-color: #ff0000; --gap: 10px; } p { color: var(--main-color); }");
    assert!(ss.variables.contains_key("--main-color"));
    assert_eq!(ss.variables["--main-color"], "#ff0000");
    assert!(ss.variables.contains_key("--gap"));
}

#[test]
fn css_variable_with_fallback() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add(":root { --main: blue; } p { color: var(--missing, red); }");
    assert!(!ss.rules.is_empty());
}

// ── Hover and pseudo-element rules ────────────────────────────────────────────

#[test]
fn css_hover_rule() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add("a:hover { color: red; }");
    let found_hover = ss.rules.iter().any(|r| r.is_hover);
    assert!(found_hover, "should detect :hover rule");
}

#[test]
fn css_pseudo_element_before() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add("p::before { content: \">\"; }");
    let found_before = ss.rules.iter().any(|r| r.pseudo_element == PseudoElement::Before);
    assert!(found_before, "should detect ::before pseudo-element rule");
}

#[test]
fn css_pseudo_element_after() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add("p::after { content: \"<\"; }");
    let found_after = ss.rules.iter().any(|r| r.pseudo_element == PseudoElement::After);
    assert!(found_after, "should detect ::after pseudo-element rule");
}

// ── Additional property application ───────────────────────────────────────────

#[test]
fn css_box_sizing_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "box-sizing", "border-box");
    assert_eq!(style.box_sizing, BoxSizing::BorderBox);
    apply_property(&mut style, "box-sizing", "content-box");
    assert_eq!(style.box_sizing, BoxSizing::ContentBox);
}

#[test]
fn css_text_overflow_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "text-overflow", "ellipsis");
    assert_eq!(style.text_overflow, TextOverflow::Ellipsis);
    apply_property(&mut style, "text-overflow", "clip");
    assert_eq!(style.text_overflow, TextOverflow::Clip);
}

#[test]
fn css_outline_properties() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "outline-width", "2px");
    assert_eq!(style.outline_width, 2.0);
    apply_property(&mut style, "outline-style", "solid");
    assert_eq!(style.outline_style, BorderStyle::Solid);
    apply_property(&mut style, "outline-style", "dashed");
    assert_eq!(style.outline_style, BorderStyle::Dashed);
    apply_property(&mut style, "outline-offset", "3px");
    assert_eq!(style.outline_offset, 3.0);
}

#[test]
fn css_background_size_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "background-size", "cover");
    assert_eq!(style.background_size, BackgroundSize::Cover);
    apply_property(&mut style, "background-size", "contain");
    assert_eq!(style.background_size, BackgroundSize::Contain);
    apply_property(&mut style, "background-size", "auto");
    assert_eq!(style.background_size, BackgroundSize::Auto);
}

#[test]
fn css_object_fit_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "object-fit", "contain");
    assert_eq!(style.object_fit, ObjectFit::Contain);
    apply_property(&mut style, "object-fit", "cover");
    assert_eq!(style.object_fit, ObjectFit::Cover);
}

#[test]
fn css_letter_spacing_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "letter-spacing", "2px");
    assert_eq!(style.letter_spacing, CssLength::Px(2.0));
}

#[test]
fn css_word_spacing_property() {
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "word-spacing", "5px");
    assert_eq!(style.word_spacing, CssLength::Px(5.0));
}
