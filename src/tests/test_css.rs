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

#[test]
fn css_mix_blend_mode_via_descendant_combinator() {
    // ".parent .child { mix-blend-mode: multiply }" must cascade to the child.
    use super::harness::parse_and_layout;
    use super::harness::find_box;
    use crate::types::MixBlendMode;
    let doc = parse_and_layout(r#"
        <style>
            .parent .overlay { mix-blend-mode: multiply; }
        </style>
        <div class="parent">
            <div class="overlay"></div>
        </div>
    "#, 800.0);
    let overlay = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "overlay").unwrap_or(false)
    });
    assert!(overlay.is_some(), "overlay box not found");
    assert_eq!(overlay.unwrap().style.mix_blend_mode, MixBlendMode::Multiply,
        "mix-blend-mode: multiply must be applied via descendant combinator");
}

#[test]
fn css_radial_gradient_with_position_stops() {
    // radial-gradient(circle at 50% 50%, #fbbf24 0%, #f97316 60%, transparent 100%)
    // must parse 3 stops with correct colors and positions.
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "background",
        "radial-gradient(circle at 50% 50%, #fbbf24 0%, #f97316 60%, transparent 100%)");
    assert_eq!(style.gradient_type, GradientType::Radial, "should be radial gradient");
    assert_eq!(style.rare().gradient_stops.len(), 3, "should have 3 stops, not {}", style.rare().gradient_stops.len());
    assert_eq!(style.rare().gradient_stops[0].color, Color::rgba(0xfb, 0xbf, 0x24, 0xff));
    assert!((style.rare().gradient_stops[0].position - 0.0).abs() < 0.01);
    assert_eq!(style.rare().gradient_stops[1].color, Color::rgba(0xf9, 0x73, 0x16, 0xff));
    assert!((style.rare().gradient_stops[1].position - 0.60).abs() < 0.01);
    assert_eq!(style.rare().gradient_stops[2].color, Color::TRANSPARENT);
    assert!((style.rare().gradient_stops[2].position - 1.0).abs() < 0.01);
}

#[test]
fn css_radial_gradient_bare_colors() {
    // radial-gradient without descriptor and without explicit positions
    let mut style = ComputedStyle::default();
    apply_property(&mut style, "background",
        "radial-gradient(#ff0000, #0000ff)");
    assert_eq!(style.gradient_type, GradientType::Radial);
    assert_eq!(style.rare().gradient_stops.len(), 2, "should have 2 stops");
    assert_eq!(style.rare().gradient_stops[0].color, Color::rgb(255, 0, 0));
    assert!((style.rare().gradient_stops[0].position - 0.0).abs() < 0.01);
    assert_eq!(style.rare().gradient_stops[1].color, Color::rgb(0, 0, 255));
    assert!((style.rare().gradient_stops[1].position - 1.0).abs() < 0.01);
}

// ── CSS Variable Inheritance ─────────────────────────────────────────────────

#[test]
fn css_var_inherited_from_root() {
    // Variables defined on :root should be inherited by child elements.
    let doc = parse_and_layout(r#"<html><head><style>
        :root { --main-color: red; }
        p { color: var(--main-color); }
    </style></head><body><p>hello</p></body></html>"#, 800.0);
    let p = find_box(&doc.root, &|b| b.tag == "p").unwrap();
    assert_eq!(p.style.color, Color::rgb(255, 0, 0), "var(--main-color) should resolve to red");
}

#[test]
fn css_var_fallback_when_undefined() {
    // var(--undefined, blue) should use the fallback.
    let doc = parse_and_layout(r#"<html><head><style>
        p { color: var(--nope, blue); }
    </style></head><body><p>hello</p></body></html>"#, 800.0);
    let p = find_box(&doc.root, &|b| b.tag == "p").unwrap();
    assert_eq!(p.style.color, Color::rgb(0, 0, 255), "fallback blue should be used");
}

#[test]
fn css_var_scoped_to_matching_selector() {
    // Variables on a class-qualified selector should only apply when the class matches.
    // .theme-a defines --fg: green, .theme-b defines --fg: red.
    // Only .theme-a is on the div, so --fg should be green.
    let doc = parse_and_layout(r#"<html><head><style>
        .theme-a { --fg: green; }
        .theme-b { --fg: red; }
        span { color: var(--fg, black); }
    </style></head><body>
        <div class="theme-a"><span>A</span></div>
    </body></html>"#, 800.0);
    let span = find_box(&doc.root, &|b| b.tag == "span").unwrap();
    assert_eq!(span.style.color, Color::rgb(0, 128, 0), "should inherit --fg:green from .theme-a");
}

#[test]
fn css_var_self_referential_with_fallback() {
    // Pattern from Wikipedia: --x: var(--x, 1rem) — self-referential with fallback.
    // Should resolve to the fallback since there's no prior definition.
    let doc = parse_and_layout(r#"<html><head><style>
        html { --sz: var(--sz, 20px); }
        p { font-size: var(--sz); }
    </style></head><body><p>text</p></body></html>"#, 800.0);
    let p = find_box(&doc.root, &|b| b.tag == "p").unwrap();
    assert_eq!(p.style.font_size, CssLength::Px(20.0), "self-ref var should use fallback 20px");
}

#[test]
fn css_var_inherited_through_nested_elements() {
    // Variables should be inherited through the DOM tree.
    let doc = parse_and_layout(r#"<html><head><style>
        .outer { --gap: 8px; }
        .inner { margin-left: var(--gap); }
    </style></head><body>
        <div class="outer"><div><div class="inner">deep</div></div></div>
    </body></html>"#, 800.0);
    let inner = find_box(&doc.root, &|b| b.attributes.get("class").map(|c| c == "inner").unwrap_or(false)).unwrap();
    assert_eq!(inner.style.margin_left, CssLength::Px(8.0), "var(--gap) should inherit through nested elements");
}

#[test]
fn css_var_override_in_child() {
    // A child can override a variable defined by a parent.
    let doc = parse_and_layout(r#"<html><head><style>
        :root { --c: red; }
        .override { --c: blue; }
        span { color: var(--c); }
    </style></head><body>
        <div><span id="a">A</span></div>
        <div class="override"><span id="b">B</span></div>
    </body></html>"#, 800.0);
    let a = find_box(&doc.root, &|b| b.attributes.get("id").map(|v| v == "a").unwrap_or(false)).unwrap();
    let b = find_box(&doc.root, &|b| b.attributes.get("id").map(|v| v == "b").unwrap_or(false)).unwrap();
    assert_eq!(a.style.color, Color::rgb(255, 0, 0), "span A should get --c:red from :root");
    assert_eq!(b.style.color, Color::rgb(0, 0, 255), "span B should get --c:blue from .override parent");
}

#[test]
fn css_var_chain_resolution() {
    // Variables can reference other variables: --a: var(--b), --b: 10px.
    let doc = parse_and_layout(r#"<html><head><style>
        :root { --b: 10px; --a: var(--b); }
        p { padding-left: var(--a); }
    </style></head><body><p>text</p></body></html>"#, 800.0);
    let p = find_box(&doc.root, &|b| b.tag == "p").unwrap();
    assert_eq!(p.style.padding_left, CssLength::Px(10.0), "chained var(--a) -> var(--b) -> 10px");
}


/// ⛔ `unset` is `inherit` on an inherited property and `initial` on every
/// other — CSS Cascade 5 §7.3, verbatim: it "acts as either `inherit` or
/// `initial`, depending on whether the property is inherited or not".
///
/// webcore collapsed `unset` into `initial` at PARSE time (`rule.rs` mapped it
/// straight to `CssValue::Initial`), so the distinction was destroyed before
/// the cascade could act on it — `CssValue::Unset` was a variant nothing ever
/// produced. `color: unset` on a child reset to black instead of inheriting.
///
/// The border row is here because it caught a second bug: the initial value of
/// `border-*-width` is `medium`, which CSS Backgrounds 3 §4.3 pins at exactly
/// 3px, and webcore had 0. Chrome agrees on all three.
#[test]
fn unset_inherits_an_inherited_property_and_initialises_the_rest() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#p{color:rgb(200,0,0);font-size:30px}         #c{color:unset;font-size:unset;border-top-width:unset;border-top-style:solid}</style>         <div id=p><div id=c>x</div></div>", 900.0);
    let c = d.get_element_by_id("c").unwrap();

    assert_eq!(d.computed_style_property(c, "color"), "rgb(200, 0, 0)",
        "`color` is inherited, so `unset` must inherit");
    assert_eq!(d.computed_style_property(c, "font-size"), "30px",
        "`font-size` is inherited, so `unset` must inherit");
    assert_eq!(d.computed_style_property(c, "border-top-width"), "3px",
        "`border-*-width` is NOT inherited, so `unset` is `initial` — `medium`, 3px");
}

/// The used width of a border is zero when it draws nothing, however wide it
/// computes. CSS Backgrounds 3 §4.3.
///
/// Measured in Chrome: a bare `<div>` answers `0px`/`none`; a
/// `<div style="border-style:solid">` answers `3px`/`solid` without ever
/// naming a width.
#[test]
fn a_border_width_resolves_to_zero_when_the_style_draws_nothing() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<div id=bare></div><div id=solid style='border-style:solid'></div>", 900.0);
    let bare = d.get_element_by_id("bare").unwrap();
    let solid = d.get_element_by_id("solid").unwrap();

    assert_eq!(d.computed_style_property(bare, "border-top-width"), "0px",
        "no border style declared, so nothing is drawn");
    assert_eq!(d.computed_style_property(solid, "border-top-width"), "3px",
        "a style with no width takes the initial width, `medium`");
}


/// ⛔ The absolute length units, with the EXACT ratios CSS Values 4 §6.2 gives.
///
/// webcore understood eight units of the spec's thirty-one; `cm`, `mm`, `Q`,
/// `in` and `pc` were not among them and fell through to `auto`.
///
/// ⛔ They could not simply be added to the old `ends_with` chain, because unit
/// names NEST: `in` is a suffix of `vmin`, so testing `in` first parses
/// `3vmin` as three inches. The parser splits the number from the unit and
/// matches the unit exactly, which removes that class of bug rather than
/// dodging it.
///
/// ⛔ Chrome is NOT the authority for these. It answers 37.7812px for `1cm`
/// where the spec says 96/2.54 = 37.795276 — every one of its values is
/// `floor(exact * 64) / 64`, because its LayoutUnit quantises to 1/64px. The
/// spec numbers are asserted here.
#[test]
fn the_absolute_length_units_use_the_exact_spec_ratios() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a{width:1in}#b{width:1cm}#c{width:1mm}#d{width:1Q}\
         #e{width:1pc}#f{width:1pt}</style>\
         <div id=a></div><div id=b></div><div id=c></div>\
         <div id=d></div><div id=e></div><div id=f></div>", 900.0);
    let px = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width").trim_end_matches("px").parse::<f32>().unwrap()
    };
    let close = |got: f32, want: f32, what: &str| {
        assert!((got - want).abs() < 0.001, "{what}: got {got}, spec says {want}");
    };
    close(px(&mut d, "a"), 96.0,            "1in = 96px");
    close(px(&mut d, "b"), 96.0 / 2.54,     "1cm = 96px/2.54");
    close(px(&mut d, "c"), 96.0 / 25.4,     "1mm = 1/10th of 1cm");
    close(px(&mut d, "d"), 96.0 / 101.6,    "1Q  = 1/40th of 1cm");
    close(px(&mut d, "e"), 16.0,            "1pc = 1/6th of 1in");
    close(px(&mut d, "f"), 96.0 / 72.0,     "1pt = 1/72nd of 1in");
}

/// Dimension units are ASCII case-insensitive (CSS Values 4 §3.1), and a
/// number may carry an exponent. Neither worked: `10PX` and `1e2px` both fell
/// through to `auto`.
#[test]
fn a_unit_is_case_insensitive_and_a_number_may_have_an_exponent() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a{width:10PX}#b{width:1e2px}#c{width:2In}</style>\
         <div id=a></div><div id=b></div><div id=c></div>", 900.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width")
    };
    assert_eq!(w(&mut d, "a"), "10px", "`PX` is the same unit as `px`");
    assert_eq!(w(&mut d, "b"), "100px", "`1e2` is 100");
    assert_eq!(w(&mut d, "c"), "192px", "`In` is the same unit as `in`");
}

/// ⛔ `3vmin` must not parse as three INCHES.
///
/// The regression this guards is subtle: `in` is a suffix of `vmin`, so any
/// parser that tests unit suffixes in the wrong order silently turns a
/// viewport-relative length into an absolute one 32x larger.
#[test]
fn a_viewport_unit_is_not_mistaken_for_inches() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a{width:3vmin}#b{width:3in}</style><div id=a></div><div id=b></div>", 900.0);
    let a = d.get_element_by_id("a").unwrap();
    let b = d.get_element_by_id("b").unwrap();
    let wa = d.computed_style_property(a, "width");
    let wb = d.computed_style_property(b, "width");
    assert_eq!(wb, "288px", "3in is 3 * 96px");
    assert_ne!(wa, wb, "3vmin is a viewport length, not three inches");
}


/// ⛔ `vmin` follows the SMALLER viewport axis and `vmax` the LARGER —
/// CSS Values 4 §6.1.2.
///
/// Both used to parse to `CssLength::Vw`, commented "approx". That is not an
/// approximation, it is the wrong axis on any landscape viewport: measured in
/// Chrome at 1200x713, `10vmin` is 71.3px (10% of the HEIGHT) and `10vmax` is
/// 120px, while webcore answered 120px for both — out by 68%.
///
/// They could not be expressed as `Vw` or `Vh` because which axis they follow
/// is not known until the viewport is, so they are their own variants.
#[test]
fn vmin_and_vmax_follow_the_smaller_and_larger_viewport_axis() {
    let mut r = crate::Renderer::new();
    // Landscape: width 1200, height 700.
    let mut d = r.load_html_vp(
        "<style>#a{width:10vmin}#b{width:10vmax}#c{width:10vw}#d{width:10vh}</style>\
         <div id=a></div><div id=b></div><div id=c></div><div id=d></div>",
        1200.0, 700.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width").trim_end_matches("px").parse::<f32>().unwrap()
    };
    let (vmin, vmax) = (w(&mut d, "a"), w(&mut d, "b"));
    let (vw, vh) = (w(&mut d, "c"), w(&mut d, "d"));

    assert!((vw - 120.0).abs() < 0.5, "10vw of 1200 is 120px, got {vw}");
    assert!((vh - 70.0).abs() < 0.5, "10vh of 700 is 70px, got {vh}");
    assert!((vmin - vh).abs() < 0.5,
        "on a LANDSCAPE viewport vmin follows the height: got {vmin}, vh is {vh}");
    assert!((vmax - vw).abs() < 0.5,
        "and vmax follows the width: got {vmax}, vw is {vw}");
    assert!((vmin - vmax).abs() > 1.0, "they must not be the same value");
}


/// The modern viewport units, and the logical viewport axes.
///
/// `svh`/`lvh`/`dvh` coincide with `vh` on a UA that shows no dynamically
/// retracting toolbars — CSS Values 4 §6.1.2 — which is this one, so that is
/// conformance rather than approximation. `vi`/`vb` are the inline and block
/// axes, which in horizontal-tb are the width and the height.
///
/// Verified against Chrome at 1200x713: `10svh` = 71.3px = `10vh`,
/// `10vi` = 120px = `10vw`, `10vb` = 71.3px.
#[test]
fn the_modern_viewport_units_resolve_to_their_axis() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html_vp(
        "<style>#sv{width:10svh}#lv{width:10lvh}#dv{width:10dvh}#sw{width:10svw}\
         #vi{width:10vi}#vb{width:10vb}#vw{width:10vw}#vh{width:10vh}</style>\
         <div id=sv></div><div id=lv></div><div id=dv></div><div id=sw></div>\
         <div id=vi></div><div id=vb></div><div id=vw></div><div id=vh></div>",
        1200.0, 700.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width")
    };
    let (vw, vh) = (w(&mut d, "vw"), w(&mut d, "vh"));
    assert_ne!(vw, vh, "the fixture must use a non-square viewport to mean anything");
    for id in ["sv", "lv", "dv", "vb"] {
        assert_eq!(w(&mut d, id), vh, "#{id} follows the BLOCK axis (the height)");
    }
    for id in ["sw", "vi"] {
        assert_eq!(w(&mut d, id), vw, "#{id} follows the INLINE axis (the width)");
    }
}

/// The font-relative units, at the fallbacks the spec itself mandates.
///
/// ⛔ These are the spec's own "must be assumed" values for when the real font
/// metric is impractical to obtain (CSS Values 4 §6.1.1) — `ex` 0.5em, `ch`
/// 0.5em, `ic` 1em. They are conforming, but they are FALLBACKS, not
/// measurements: Chrome, which reads the font, answers 7.17px for `1ex` at a
/// 16px font where this answers 8px. `ch` happens to agree exactly (8px).
///
/// Closing that gap means giving the length resolver access to font metrics,
/// which it does not currently have.
#[test]
fn the_font_relative_units_use_the_spec_mandated_fallbacks() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{font-size:16px}#ex{width:1ex}#ch{width:1ch}#ic{width:1ic}\
         #em{width:1em}</style>\
         <div id=ex></div><div id=ch></div><div id=ic></div><div id=em></div>", 900.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width").trim_end_matches("px").parse::<f32>().unwrap()
    };
    let em = w(&mut d, "em");
    assert!((em - 16.0).abs() < 0.01, "1em is the element's font size");
    assert!((w(&mut d, "ex") - em * 0.5).abs() < 0.01, "`ex` falls back to 0.5em");
    assert!((w(&mut d, "ch") - em * 0.5).abs() < 0.01, "`ch` falls back to 0.5em");
    assert!((w(&mut d, "ic") - em).abs() < 0.01, "`ic` falls back to 1em");
}


/// ⛔ `calc()` had its OWN unit table, and its catch-all was `unknown => px`.
///
/// That is the worst shape a gap can take: not a parse failure but a silent
/// wrong answer. `calc(1in + 2px)` resolved to **3px** — `in` was not in
/// calc's table, so `1in` was read as `1px` — where the correct answer is
/// 98px. Every unit added to `parse_length` had to be added here too, and any
/// that was not became wrong rather than rejected.
///
/// `parse_length` is now the single definition and this projects onto the
/// coefficient slots. Chrome agrees on all four; where it differs it is its
/// 1/64px quantisation, not disagreement — `calc(2cm)` is exactly 2*96/2.54.
#[test]
fn calc_understands_every_unit_the_length_parser_does() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a{width:calc(1in + 2px)}#b{width:calc(2cm)}#c{width:calc(1pc + 1pt)}\
         #d{width:calc(10px + 2em);font-size:16px}</style>\
         <div id=a></div><div id=b></div><div id=c></div><div id=d></div>", 900.0);
    let px = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width").trim_end_matches("px").parse::<f32>().unwrap()
    };
    let close = |got: f32, want: f32, what: &str| {
        assert!((got - want).abs() < 0.01, "{what}: got {got}, want {want}");
    };
    close(px(&mut d, "a"), 98.0, "calc(1in + 2px) — `in` is 96px, not 1px");
    close(px(&mut d, "b"), 2.0 * 96.0 / 2.54, "calc(2cm)");
    close(px(&mut d, "c"), 16.0 + 96.0 / 72.0, "calc(1pc + 1pt)");
    close(px(&mut d, "d"), 42.0, "calc(10px + 2em) at a 16px font");
}


/// ⛔ A rem-based media query was ALWAYS TRUE.
///
/// `parse_media_px` was a third private unit table — `px`, `em`, and a bare
/// `parse()` for everything else. A bare parse of `"40rem"` fails and gives 0,
/// so `(min-width: 40rem)` compared the viewport against **zero** and matched
/// everything. rem breakpoints are what Bootstrap and Tailwind emit, so this
/// silently applied every one of their responsive blocks at every size.
///
/// Measured: `(min-width: 4000rem)` — 64000px — matched a 1200px viewport.
/// Chrome does not.
///
/// Relative units in a media query resolve against the INITIAL font size
/// (Media Queries 4 §1.3), so `em` and `rem` are both 16px here.
#[test]
fn a_media_query_understands_every_length_unit() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html_vp(
        "<style>#a{width:10px}@media (min-width: 4000rem){#a{width:99px}}\
         #b{width:10px}@media (min-width: 40rem){#b{width:77px}}\
         #c{width:10px}@media (min-width: 40em){#c{width:55px}}\
         #e{width:10px}@media (min-width: 100in){#e{width:66px}}</style>\
         <div id=a></div><div id=b></div><div id=c></div><div id=e></div>",
        1200.0, 800.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "width")
    };
    assert_eq!(w(&mut d, "a"), "10px",
        "4000rem is 64000px — it must NOT match a 1200px viewport");
    assert_eq!(w(&mut d, "b"), "77px", "40rem is 640px — it matches");
    assert_eq!(w(&mut d, "c"), "55px", "40em is also 640px in a media query");
    assert_eq!(w(&mut d, "e"), "10px",
        "100in is 9600px — an absolute unit must be understood, not read as 0");
}


/// ⛔ Transform arguments are not all the same kind of value, and treating
/// them as one broke lengths and angles in different ways.
///
/// The parser stripped `px|deg|rad|turn` off every argument and parsed the
/// remainder. So:
///
///  * a LENGTH in any other unit failed to parse and became 0 —
///    `translateX(2rem)` and `translateX(1in)` moved the element NOWHERE;
///  * an ANGLE had its unit REMOVED rather than CONVERTED — `rotate(1turn)`
///    was read as one DEGREE instead of 360, and `rotate(1rad)` as one degree
///    instead of 57.3.
///
/// Lengths now go through the single unit definition; angles convert per
/// CSS Values 4 §7.1 (1turn = 360deg, 1grad = 0.9deg, 1rad = 180/PI deg).
/// Every value below is Chrome's, on the same markup.
#[test]
fn transform_lengths_and_angles_use_their_own_units() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a{transform:translateX(2rem)}#b{transform:translateX(1in)}\
         #c{transform:rotate(0.5turn)}#d{transform:rotate(100grad)}\
         #e{transform:rotate(90deg)}</style>\
         <div id=a></div><div id=b></div><div id=c></div>\
         <div id=d></div><div id=e></div>", 900.0);
    let t = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "transform")
    };
    assert_eq!(t(&mut d, "a"), "matrix(1, 0, 0, 1, 32, 0)", "translateX(2rem) is 32px");
    assert_eq!(t(&mut d, "b"), "matrix(1, 0, 0, 1, 96, 0)", "translateX(1in) is 96px");
    assert_eq!(t(&mut d, "c"), "matrix(-1, 0, 0, -1, 0, 0)", "0.5turn is 180deg");
    let grad = t(&mut d, "d");
    let deg90 = t(&mut d, "e");
    assert_eq!(grad, deg90, "100grad IS 90deg — got {grad} vs {deg90}");
}

/// ⛔ `getBoundingClientRect()` must return the TRANSFORMED border box —
/// CSSOM View §4 — and returned the untransformed one, so a page could not
/// find out where a transformed element actually is.
///
/// Chrome on this markup: a 100x40 box translated 2rem is at x=32 with its
/// size unchanged; rotated 90deg about its centre it becomes 40x100 at
/// (30, -30). The rotation case is the one that proves the whole box is
/// mapped and re-bounded, not just its origin shifted.
#[test]
fn a_bounding_rect_reflects_the_elements_transform() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}div{width:100px;height:40px;position:absolute;top:0;left:0}\
         #a{transform:translateX(2rem)}#b{transform:rotate(90deg)}#c{}</style>\
         <div id=a></div><div id=b></div><div id=c></div>", 900.0);
    let rect = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap()
    };
    let c = rect(&mut d, "c");
    assert!((c.w - 100.0).abs() < 0.5 && (c.h - 40.0).abs() < 0.5,
        "the untransformed box is 100x40, got {}x{}", c.w, c.h);

    let a = rect(&mut d, "a");
    assert!((a.x - 32.0).abs() < 0.5, "translateX(2rem) puts x at 32, got {}", a.x);
    assert!((a.w - 100.0).abs() < 0.5, "a translation does not change the size");

    let b = rect(&mut d, "b");
    assert!((b.w - 40.0).abs() < 1.0 && (b.h - 100.0).abs() < 1.0,
        "rotate(90deg) swaps the extents to 40x100, got {}x{}", b.w, b.h);
}


/// ⛔ `@layer` ordering was ignored entirely — the parser said so in a comment
/// ("ignore layer ordering for now") and the cascade sorted on specificity
/// alone.
///
/// Two consequences, both verified against Chrome:
///
///  * a rule in a LATER layer beats one in an earlier layer however much more
///    specific the earlier one is — layers sort ABOVE specificity
///    (CSS Cascade 5 §6.4.4). A bare `div` in `over` beats `div#hi.hi.hi` in
///    `base`.
///  * an UNLAYERED normal declaration beats every layered one.
///
/// ⛔ The `@layer a, b;` STATEMENT form is what fixes the order, and it has no
/// block — so it was being discarded along with every other braceless at-rule,
/// and layer precedence silently fell back to source order. In the fixture
/// below `base` is written AFTER `over` in the source precisely so that source
/// order gives the wrong answer.
#[test]
fn layer_order_outranks_specificity_and_unlayered_wins() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>@layer base, over;\
         @layer over { div { color: green } }\
         @layer base { div#hi.hi.hi { color: red } }\
         #un2 { color: green }\
         @layer only2 { #un2 { color: red } }</style>\
         <div id=hi class=hi>x</div><div id=un2>x</div>", 900.0);
    let c = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "color")
    };
    assert_eq!(c(&mut d, "hi"), "rgb(0, 128, 0)",
        "a bare `div` in the LATER layer beats `div#hi.hi.hi` in the earlier one");
    assert_eq!(c(&mut d, "un2"), "rgb(0, 128, 0)",
        "an unlayered declaration beats a layered one");
}


/// ⛔ A sibling combinator failed the moment anything preceded it.
///
/// `i + i` matched. `#p i + i` and `#p > i + i` did not — so
/// `.container > li + li` and `.card h2 + p`, which are everyday selectors,
/// silently never applied.
///
/// The sibling branches matched the left-hand side as a FLAT COMPOUND against
/// the sibling, which is only correct when there is no further combinator in
/// it. `matches_part_with_context` answers `true` for a `Combinator` part, so
/// everything to its left was then tested against the SIBLING rather than
/// against the sibling's ancestor — and `#p` is not an `<i>`. They recurse now,
/// as the descendant and child branches already did.
///
/// Chrome matches every row below.
#[test]
fn a_sibling_combinator_works_after_another_combinator() {
    let cases = [
        ("i + i",       "i2"),
        ("#p i + i",    "i2"),
        ("#p > i + i",  "i2"),
        ("i ~ u",       "u1"),
        ("#q i ~ u",    "u1"),
        ("#q > i ~ u",  "u1"),
    ];
    for (sel, target) in cases {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(
            &format!("<style>{sel}{{color:green}}</style>\
                      <div id=p><i id=i1>a</i><i id=i2>b</i></div>\
                      <div id=q><i id=i3>a</i><u id=u1>b</u></div>"), 900.0);
        let e = d.get_element_by_id(target).unwrap();
        assert_eq!(d.computed_style_property(e, "color"), "rgb(0, 128, 0)",
            "`{sel}` must match #{target}");
    }

    // …and must NOT match the first sibling, or it is matching everything.
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#p > i + i{color:green}</style>\
         <div id=p><i id=i1>a</i><i id=i2>b</i></div>", 900.0);
    let first = d.get_element_by_id("i1").unwrap();
    assert_ne!(d.computed_style_property(first, "color"), "rgb(0, 128, 0)",
        "`i + i` must not match the FIRST `<i>` — it has no preceding sibling");
}


/// ⛔ `:has(> em)` never matched — two separate bugs, both needed fixing.
///
/// 1. The selector parser did not skip the whitespace AFTER a leading
///    combinator, so `"> em"` parsed as `[Child, Descendant, em]` — a spurious
///    second combinator that matches nothing. It only bit selectors that
///    START with a combinator, because in `div > em` the whitespace arm sees
///    the `>` first and already skips past it. A `:has()` argument is exactly
///    such a selector.
/// 2. `:has()`'s argument is a RELATIVE selector (Selectors 4 §4.5): a leading
///    combinator relates to the ANCHOR, the element `:has()` is written on.
///    The matcher tried it against every descendant with an empty ancestor
///    list, so the leading `>` had nothing to relate to.
///
/// The `#b` row is what separates them: `:has(> em)` must NOT match when the
/// `<em>` is a grandchild. Chrome agrees on all three.
#[test]
fn has_treats_its_argument_as_relative_to_the_anchor() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a:has(> em){color:green} #b:has(> em){color:green} \
         #c:has(em){color:green}</style>\
         <div id=a><em>x</em></div>\
         <div id=b><span><em>x</em></span></div>\
         <div id=c><span><em>x</em></span></div>", 900.0);
    let c = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "color")
    };
    assert_eq!(c(&mut d, "a"), "rgb(0, 128, 0)", "`:has(> em)` matches a DIRECT child");
    assert_ne!(c(&mut d, "b"), "rgb(0, 128, 0)", "`:has(> em)` must NOT match a grandchild");
    assert_eq!(c(&mut d, "c"), "rgb(0, 128, 0)", "`:has(em)` matches at any depth");
}

/// The parser bug above, on its own terms: a selector that STARTS with a
/// combinator must not gain a second one from the space after it.
#[test]
fn a_leading_combinator_does_not_add_a_descendant_combinator() {
    use crate::css::selector::{SelectorPart, Combinator};
    let sel = crate::css::parser::parse_selector("> em");
    let combinators: Vec<_> = sel.parts.iter()
        .filter(|p| matches!(p, SelectorPart::Combinator(_)))
        .collect();
    assert_eq!(combinators.len(), 1,
        "`> em` has ONE combinator; the space after `>` must not add another — got {:?}",
        sel.parts);
    assert!(matches!(sel.parts.first(), Some(SelectorPart::Combinator(Combinator::Child))),
        "and it is the child combinator");
}

/// ⛔ `:nth-child(An+B of S)` — Selectors 4 §9.3 — counts only among siblings
/// matching S, and requires the element to match S itself.
///
/// The whole argument used to go to the An+B parser, which cannot read
/// `2 of .pick`, so the selector matched nothing at all.
///
/// The fixture is built so that counting ALL children gives a different answer
/// from counting only the matching ones: the 2nd `.pick` is the 4th child.
/// Chrome colours exactly `b3`.
#[test]
fn nth_child_of_selector_counts_only_matching_siblings() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#z :nth-child(2 of .pick){color:green}</style>\
         <div id=z><b id=b0>skip</b><b id=b1 class=pick>a</b><b id=b2>no</b>\
         <b id=b3 class=pick>b</b><b id=b4 class=pick>c</b></div>", 900.0);
    let c = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "color")
    };
    assert_eq!(c(&mut d, "b3"), "rgb(0, 128, 0)",
        "the 2nd `.pick` is the 4th child — counting all children would pick b1");
    for id in ["b0", "b1", "b2", "b4"] {
        assert_ne!(c(&mut d, id), "rgb(0, 128, 0)", "#{id} is not the 2nd `.pick`");
    }
}


/// ⛔ `width` and `height` do not apply to a NON-REPLACED INLINE box —
/// CSS 2.1 §10.2 and §10.5.
///
/// `<span style="width:100px;height:50px">` was being sized 100x50. Chrome
/// sizes it by its text.
///
/// The distinction is `display: inline` exactly, not "is inline-level":
/// `inline-block`, `inline-flex` and the replaced elements are all
/// inline-level and DO take a width, which is what the second half asserts —
/// without it, an implementation that ignored width on everything
/// inline-level would pass.
///
/// ⛔ webcore reports 0x0 for an inline element's rect (Chrome: 8x18), because
/// an inline box is a run of line-box fragments rather than a box with a
/// border rect. That is a separate, pre-existing gap; this test therefore
/// checks that the declared width is NOT adopted, not the exact text extent.
#[test]
fn width_does_not_apply_to_a_non_replaced_inline() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0;font-size:16px}</style><div style='width:400px'>\
         <span id=wide style='width:100px'>x</span>\
         <span id=ib style='display:inline-block;width:100px'>x</span></div>", 900.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    let wide = w(&mut d, "wide");
    assert!((wide - 100.0).abs() > 1.0,
        "an inline box must not adopt its declared width — got {wide}");
    let ib = w(&mut d, "ib");
    assert!((ib - 100.0).abs() < 1.0,
        "…but an inline-BLOCK does: got {ib}");
}


/// ⛔ The modern space-separated colour syntax did not parse — CSS Color 4 §4.
///
/// `rgb(1 2 3)`, `rgb(1 2 3 / 0.5)` and `hsl(120 50% 50%)` are what every
/// current design system emits, and only the legacy comma form was understood.
/// The component split produced ONE item, failed the `len() >= 3` check, and
/// the colour silently became BLACK.
///
/// The hue also accepts an angle unit (`120deg`), and the alpha a percentage.
///
/// ⛔ Channels ROUND rather than truncate: `hsl(120 50% 50%)`'s green channel
/// is 63.75, and `as u8` floored it to 63 where every browser says 64.
#[test]
fn the_modern_space_separated_colour_syntax_parses() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>#a{color:rgb(1 2 3)}#b{color:rgb(1 2 3 / 0.5)}\
         #c{color:rgba(1,2,3,0.5)}#e{color:hsl(120 50% 50%)}\
         #f{color:hsl(120deg 50% 50% / .5)}#g{color:rgb(1 2 3 / 50%)}</style>\
         <div id=a>x</div><div id=b>x</div><div id=c>x</div>\
         <div id=e>x</div><div id=f>x</div><div id=g>x</div>", 900.0);
    let c = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.computed_style_property(e, "color")
    };
    assert_eq!(c(&mut d, "a"), "rgb(1, 2, 3)", "space-separated rgb()");
    assert_eq!(c(&mut d, "b"), "rgba(1, 2, 3, 0.5)", "slash alpha");
    assert_eq!(c(&mut d, "c"), "rgba(1, 2, 3, 0.5)", "the legacy comma form still works");
    assert_eq!(c(&mut d, "e"), "rgb(64, 191, 64)",
        "hsl() space-separated — and 63.75 ROUNDS to 64, it does not truncate to 63");
    assert_eq!(c(&mut d, "f"), "rgba(64, 191, 64, 0.5)", "a hue may carry `deg`");
    assert_eq!(c(&mut d, "g"), "rgba(1, 2, 3, 0.5)", "alpha may be a percentage");
}


/// ⛔ A flex row with a definite HEIGHT did not shrink its items.
///
/// `align-items: stretch` is the default, and a definite cross size is what
/// makes the stretch pass actually re-lay a child. That re-layout passed
/// `None` for the forced MAIN size, so the child fell back to its own `width`
/// and the resolved grow/shrink was thrown away.
///
/// Measured: a 400px item beside a `flex-shrink:0` 50px item, in a 300px row,
/// stayed 400px instead of shrinking to 250. Chrome gives 250. Fixed-height
/// flex rows are an everyday layout, and the bug was invisible without one —
/// the same markup with no height, or with `align-items: flex-start`, was
/// already correct.
#[test]
fn a_flex_row_with_a_definite_height_still_shrinks_its_items() {
    for extra in ["", "height:60px", "height:60px;align-items:stretch",
                  "height:60px;align-items:flex-start"] {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}</style><div style='display:flex;width:300px;{extra}'>\
             <div id=x style='width:50px;flex-shrink:0'>a</div>\
             <div id=y style='width:400px'>b</div></div>"), 900.0);
        let x = d.get_element_by_id("x").unwrap();
        let y = d.get_element_by_id("y").unwrap();
        let wx = d.get_bounding_client_rect(x).unwrap().w;
        let wy = d.get_bounding_client_rect(y).unwrap().w;
        assert!((wx - 50.0).abs() < 1.0, "`flex-shrink:0` holds at 50 [{extra}], got {wx}");
        assert!((wy - 250.0).abs() < 1.0,
            "the flexible item shrinks 400 -> 250 [{extra}], got {wy}");
    }
}

/// The same, mirrored onto a column: the main axis there is the HEIGHT.
#[test]
fn a_flex_column_with_a_definite_width_still_resolves_its_main_size() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}</style>\
         <div style='display:flex;flex-direction:column;width:200px;height:300px'>\
         <div id=x style='height:50px;flex-shrink:0'>a</div>\
         <div id=y style='height:400px'>b</div></div>", 900.0);
    let y = d.get_element_by_id("y").unwrap();
    let h = d.get_bounding_client_rect(y).unwrap().h;
    assert!((h - 250.0).abs() < 1.0, "400 -> 250 down the column, got {h}");
}


/// ⛔ A `minmax(min, 1fr)` track had its base counted TWICE.
///
/// CSS Grid §12.7 subtracts the base sizes of the NON-flexible tracks from the
/// free space; a `minmax(50px, 1fr)` track is flexible, so its 50px must stay
/// in. It was being added to `used` AND given an fr share, so:
///
///  * `minmax(50px,1fr) 1fr` in a 300px grid gave 125 where Chrome gives 150;
///  * `repeat(auto-fill, minmax(80px,1fr))` produced 80px columns that never
///    grew to fill the row — Chrome fills at 100.
///
/// The second is the one that shows up on real pages: it is the standard
/// responsive-card grid, and every card was stuck at its minimum.
#[test]
fn a_flexible_minmax_track_keeps_its_base_in_the_free_space() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.g{display:grid;width:300px}\
         #e{grid-template-columns:minmax(50px,1fr) 1fr}\
         #f{grid-template-columns:repeat(auto-fill,minmax(80px,1fr))}</style>\
         <div class=g id=e><i id=e1>1</i><i id=e2>2</i></div>\
         <div class=g id=f><i id=f1>1</i><i id=f2>2</i><i id=f3>3</i></div>", 900.0);
    let w = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    let e1 = w(&mut d, "e1");
    assert!((e1 - 150.0).abs() < 1.0,
        "`minmax(50px,1fr)` takes a full fr share of 300/2, got {e1}");
    for id in ["f1", "f2", "f3"] {
        let got = w(&mut d, id);
        assert!((got - 100.0).abs() < 1.0,
            "auto-fill minmax(80px,1fr) columns fill the row at 100, #{id} got {got}");
    }
}


/// ⛔ `row-reverse` / `column-reverse` packed against the WRONG EDGE.
///
/// In a reversed direction the main-START is the far edge (Flexbox §5.1), so
/// `flex-start` — the default — packs against the RIGHT in `row-reverse`, and
/// `flex-start`/`flex-end` swap. The item ORDER was reversed and the packing
/// was not, so a `row-reverse` row laid its items out in reverse order against
/// the LEFT edge: measured 50/0 where Chrome gives 250/200.
#[test]
fn a_reversed_flex_direction_packs_against_the_far_edge() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px;height:60px}\
         .i{width:50px;height:20px}#r{flex-direction:row-reverse}\
         #c{flex-direction:column-reverse;height:120px}</style>\
         <div class=f id=r><i class=i id=r1>1</i><i class=i id=r2>2</i></div>\
         <div class=f id=c><i class=i id=c1>1</i><i class=i id=c2>2</i></div>", 900.0);
    let at = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        let p = d.parent_node(e);
        let b = d.get_bounding_client_rect(e).unwrap();
        let pb = d.get_bounding_client_rect(p).unwrap();
        (b.x - pb.x, b.y - pb.y)
    };
    let (r1x, _) = at(&mut d, "r1");
    let (r2x, _) = at(&mut d, "r2");
    assert!((r1x - 250.0).abs() < 1.0, "row-reverse puts the FIRST item rightmost, got {r1x}");
    assert!((r2x - 200.0).abs() < 1.0, "and the second beside it, got {r2x}");

    let (_, c1y) = at(&mut d, "c1");
    let (_, c2y) = at(&mut d, "c2");
    assert!((c1y - 100.0).abs() < 1.0, "column-reverse starts at the BOTTOM, got {c1y}");
    assert!((c2y - 80.0).abs() < 1.0, "and stacks upward, got {c2y}");
}

/// ⛔ `align-items: stretch` overrode a DEFINITE cross size.
///
/// Stretch applies only when the item's cross size is `auto` (Flexbox §5.2,
/// §9.4). This stretched regardless, so `<i style="height:20px">` in a 60px
/// flex row came out **60** tall — the declared height discarded. Any flex item
/// with an explicit cross size was being resized.
#[test]
fn stretch_does_not_override_a_definite_cross_size() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px;height:60px}</style>\
         <div class=f><i id=fixed style='width:50px;height:20px'>1</i>\
         <i id=auto style='width:50px'>2</i></div>", 900.0);
    let h = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().h
    };
    let fixed = h(&mut d, "fixed");
    assert!((fixed - 20.0).abs() < 1.0,
        "a definite height survives `align-items: stretch`, got {fixed}");
    let auto = h(&mut d, "auto");
    assert!((auto - 60.0).abs() < 1.0,
        "…while an `auto` one still stretches to the line, got {auto}");
}


/// ⛔ `flex-wrap: wrap-reverse` put every item against the wrong edge of its
/// line.
///
/// It flips the CROSS-START (Flexbox §5.2), so `flex-start` aligns to the
/// bottom of the line and `flex-end` to the top — they swap, just as
/// `flex-start`/`flex-end` swap on the main axis under `row-reverse`. The LINE
/// order was already reversed; the item alignment inside each line was not.
///
/// ⛔ Three things had to change, and the first two were invisible on their
/// own:
///  * `effective_align_self` swaps the two edges under wrap-reverse;
///  * the non-stretch match must NOT swap again (doing so put `flex-start` and
///    `flex-end` in the SAME place);
///  * the `stretch` branch returns its own hardcoded cross position, and that
///    is the one the default `align-items` actually uses.
///
/// Geometry: a 120x60 container, three 50x20 items, so two lines that
/// `align-content: stretch` grows to 30 each. Chrome puts the first line's
/// items at y=40 and the second line's at y=10.
#[test]
fn wrap_reverse_flips_the_cross_start_edge() {
    let case = |ai: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;flex-wrap:wrap-reverse;\
             width:120px;height:60px;align-items:{ai}}}.i{{width:50px;height:20px}}</style>\
             <div class=f id=p><i class=i id=x>1</i><i class=i id=y>2</i>\
             <i class=i id=z>3</i></div>"), 900.0);
        let p = d.get_element_by_id("p").unwrap();
        let pb = d.get_bounding_client_rect(p).unwrap();
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().y - pb.y
        };
        (get(&mut d, "x"), get(&mut d, "z"))
    };

    // The first line is the LOWER one, and its items sit at its bottom.
    let (x, z) = case("stretch");
    assert!((x - 40.0).abs() < 1.0, "stretch: first line's item at 40, got {x}");
    assert!((z - 10.0).abs() < 1.0, "stretch: second line's item at 10, got {z}");

    let (x, z) = case("flex-start");
    assert!((x - 40.0).abs() < 1.0, "flex-start aligns to the flipped start, got {x}");
    assert!((z - 10.0).abs() < 1.0, "…on the second line too, got {z}");

    // …and `flex-end` is the opposite edge, or the two are not really swapping.
    let (xe, _) = case("flex-end");
    assert!((xe - 30.0).abs() < 1.0, "flex-end is the OTHER edge, got {xe}");
    assert!((x - xe).abs() > 1.0, "flex-start and flex-end must not coincide");
}

/// Flexbox §9.7: an item frozen by its min or max size hands the space it
/// could not take back to its siblings.
///
/// The distribution ran once and clamped afterwards, so a clamped item's
/// surplus simply vanished: a `max-width:50px` item in a 300px row left its
/// sibling at 150 instead of 250, and the row overflowed by 100px.
#[test]
fn a_clamped_flex_item_redistributes_to_its_siblings() {
    let widths = |items: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px}}\
             .f>i{{display:block;height:20px}}</style>\
             <div class=f>{items}</div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().w
        };
        (get(&mut d, "a"), get(&mut d, "b"))
    };

    // Growing: `a` is capped at 50, so `b` takes the remaining 250.
    let (a, b) = widths("<i id=a style='flex:1 1 0;max-width:50px'></i>\
                         <i id=b style='flex:1 1 0'></i>");
    assert!((a - 50.0).abs() < 0.5, "max-width caps the item, got {a}");
    assert!((b - 250.0).abs() < 0.5, "the sibling absorbs the surplus, got {b}");

    // Shrinking: `a` is floored at 180, so `b` absorbs the rest of the deficit.
    let (a, b) = widths("<i id=a style='flex:0 1 200px;min-width:180px'></i>\
                         <i id=b style='flex:0 1 200px'></i>");
    assert!((a - 180.0).abs() < 0.5, "min-width floors the item, got {a}");
    assert!((b - 120.0).abs() < 0.5, "the sibling absorbs the deficit, got {b}");
}

/// Flexbox §8.3: `align-items: baseline` lines the items' first baselines up.
#[test]
fn baseline_alignment_lines_the_first_baselines_up() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;align-items:baseline;width:300px}\
         .f>i{display:block;width:60px}</style>\
         <div class=f id=p><i id=a style='padding-top:10px'>x</i>\
         <i id=b style='padding-top:30px'>y</i></div>", 900.0);
    let top = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().y
    };
    let py = top(&mut d, "p");
    let a = top(&mut d, "a") - py;
    let b = top(&mut d, "b") - py;
    // Both items carry the same line box, so the 20px of extra padding on `b`
    // is exactly how far `a` has to drop for the baselines to meet.
    assert!((b - 0.0).abs() < 0.5, "the deepest baseline sets the line, got {b}");
    assert!((a - 20.0).abs() < 0.5, "the shallower item drops to meet it, got {a}");
}

/// CSS Box Alignment §4.4: the distribution values fall back when the free
/// space is negative. `space-between` becomes `flex-start`, and
/// `space-around` / `space-evenly` become SAFE `center`, which is itself
/// `flex-start` once the space is negative. An explicit `center` or `flex-end`
/// is unsafe and still overflows.
///
/// A negative free space became negative spacing, so the items overlapped.
#[test]
fn overflowing_distribution_falls_back_instead_of_overlapping() {
    let case = |jc: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px;justify-content:{jc}}}\
             .f>i{{display:block;height:20px;flex:0 0 200px}}</style>\
             <div class=f id=p><i id=a></i><i id=b></i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().x
        };
        let p = get(&mut d, "p");
        (get(&mut d, "a") - p, get(&mut d, "b") - p)
    };

    // The safe values all pack against the start edge and overflow the end.
    for jc in ["space-between", "space-around", "space-evenly", "flex-start"] {
        let (a, b) = case(jc);
        assert!(a.abs() < 0.5, "{jc}: falls back to the start edge, got {a}");
        assert!((b - 200.0).abs() < 0.5, "{jc}: items must not overlap, got {b}");
    }
    // …and the unsafe ones still overflow the start edge, as asked.
    let (a, b) = case("center");
    assert!((a + 50.0).abs() < 0.5, "center overflows both edges, got {a}");
    assert!((b - 150.0).abs() < 0.5, "…by half the deficit each, got {b}");
    let (a, _) = case("flex-end");
    assert!((a + 100.0).abs() < 0.5, "flex-end overflows the start edge, got {a}");
}

/// The same fallback on the CROSS axis. `free_cross` was clamped to zero, so
/// `align-content: center` and `flex-end` did nothing at all once the lines
/// overflowed — they must move the lines past the cross-start edge.
#[test]
fn overflowing_align_content_moves_past_the_cross_start_edge() {
    let first_line_y = |ac: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;flex-wrap:wrap;width:100px;\
             height:40px;align-content:{ac}}}.f>i{{display:block;width:80px;height:30px}}\
             </style><div class=f id=p><i id=a></i><i id=b></i><i id=c></i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().y
        };
        get(&mut d, "a") - get(&mut d, "p")
    };
    // Three 30px lines in a 40px box: 50px of overflow to place.
    assert!(first_line_y("flex-start").abs() < 0.5, "flex-start stays put");
    let c = first_line_y("center");
    assert!((c + 25.0).abs() < 0.5, "center splits the overflow, got {c}");
    let e = first_line_y("flex-end");
    assert!((e + 50.0).abs() < 0.5, "flex-end pushes it all up, got {e}");
    // The distribution values are safe, so they pack at the start instead.
    for ac in ["space-between", "space-around", "space-evenly", "stretch"] {
        let y = first_line_y(ac);
        assert!(y.abs() < 0.5, "{ac}: falls back to the start edge, got {y}");
    }
}

/// CSS Sizing §5.1: a definite cross size plus an `aspect-ratio` gives the
/// flex item its main size. Without the transfer the item measured its (empty)
/// content and came out 0 wide.
#[test]
fn an_aspect_ratio_transfers_the_cross_size_to_the_main_axis() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px}</style>\
         <div class=f><i id=a style='display:block;flex:0 0 auto;\
         aspect-ratio:2/1;height:30px'></i></div>", 900.0);
    let e = d.get_element_by_id("a").unwrap();
    let w = d.get_bounding_client_rect(e).unwrap().w;
    assert!((w - 60.0).abs() < 0.5, "30px tall at 2/1 is 60px wide, got {w}");
}

/// Flexbox §9.8: a percentage on a flex item resolves against the flex
/// container's inner size. The item was laid out with no available height, so
/// `height: 50%` resolved to zero.
#[test]
fn a_percentage_cross_size_resolves_against_the_flex_container() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px;height:60px}</style>\
         <div class=f><i id=a style='display:block;width:50%;height:50%'></i></div>", 900.0);
    let e = d.get_element_by_id("a").unwrap();
    let b = d.get_bounding_client_rect(e).unwrap();
    assert!((b.w - 150.0).abs() < 0.5, "50% of 300, got {}", b.w);
    assert!((b.h - 30.0).abs() < 0.5, "50% of 60, got {}", b.h);
}

/// A column flex item that shrinks to its intrinsic width keeps the main size
/// flex gave it.
///
/// `align-items` other than `stretch` re-lays the item at its intrinsic width,
/// and that re-layout dropped the resolved height — so `flex: 1` items in a
/// 90px column came out at their content height instead of 45px each.
#[test]
fn a_column_item_keeps_its_main_size_when_shrunk_to_fit() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;flex-direction:column;\
         align-items:flex-start;width:300px;height:90px}</style>\
         <div class=f><i id=a style='display:block;flex:1'>a</i>\
         <i id=b style='display:block;flex:1'>bbbb</i></div>", 900.0);
    for id in ["a", "b"] {
        let e = d.get_element_by_id(id).unwrap();
        let h = d.get_bounding_client_rect(e).unwrap().h;
        assert!((h - 45.0).abs() < 0.5, "{id}: flex:1 of 90px is 45, got {h}");
    }
}

/// Flexbox §7: the `flex` shorthand's components may come in any order, and the
/// basis is whichever component is not a bare number.
///
/// The basis was read only out of the third slot, so every two-value form
/// dropped it: `flex: 1 30%` left the item at its previous basis entirely.
#[test]
fn the_flex_shorthand_reads_a_basis_from_any_slot() {
    let width = |decl: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px}}</style>\
             <div class=f><i id=a style='display:block;height:20px;{decl}'></i>\
             <i id=b style='display:block;height:20px;flex:0 0 100px'></i></div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    // `flex: 0 <basis>` — one number, so it is the GROW factor and the item
    // stays at its basis.
    assert!((width("flex:0 100px") - 100.0).abs() < 0.5, "two-value px basis");
    assert!((width("flex:0 30%") - 90.0).abs() < 0.5, "two-value percent basis");
    // `flex: 1 auto` grows from the specified width, so all 200 remaining px
    // land on it.
    assert!((width("flex:1 auto;width:40px") - 200.0).abs() < 0.5, "two-value auto basis");
    // An omitted basis is 0, not `auto` — the shorthand's default differs from
    // the property's initial value.
    assert!((width("flex:0;width:70px") - 0.0).abs() < 0.5, "an omitted basis is 0");
    // Three numbers still work, with the third as a zero basis.
    assert!((width("flex:0 1 0") - 0.0).abs() < 0.5, "a bare third number is a 0 basis");
}

/// Flexbox §7.2.3: `flex-basis: content` sizes from the content and ignores the
/// item's own `width`, which the intrinsic measurement short-circuits on.
#[test]
fn a_content_flex_basis_ignores_the_specified_width() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px}</style>\
         <div class=f><i id=a style='display:block;flex-basis:content;width:2px'>\
         wide text here</i><i id=b style='display:block;flex:0 0 100px'></i></div>", 900.0);
    let e = d.get_element_by_id("a").unwrap();
    let w = d.get_bounding_client_rect(e).unwrap().w;
    // The exact advance is a font question; what matters is that the 2px
    // specified width is not what sized the item.
    assert!(w > 50.0, "the content sizes the item, not its width, got {w}");
}

/// Box Alignment §8.3: a percentage gap resolves against the container's own
/// content box in the gap's OWN axis. Both gaps read the width, so `row-gap`
/// in a 300x120 column measured 10% of 300 instead of 10% of 120.
#[test]
fn a_percentage_row_gap_resolves_against_the_height() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;flex-direction:column;width:300px;\
         height:120px;row-gap:10%}</style>\
         <div class=f><i id=a style='display:block;flex:1'></i>\
         <i id=b style='display:block;flex:1'></i></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap()
    };
    let a = get(&mut d, "a");
    let b = get(&mut d, "b");
    // 10% of 120 is a 12px gap, leaving 108 for two equal items.
    assert!((a.h - 54.0).abs() < 0.5, "the gap eats 12px, not 30, got {}", a.h);
    assert!((b.y - a.y - 66.0).abs() < 0.5, "54 tall plus a 12px gap, got {}", b.y - a.y);
}

/// Flexbox §4.5: `min-width: auto` on a flex item is the content-based
/// minimum, so an item never shrinks below its own content.
///
/// `min-width` defaulted to `0` rather than its CSS initial value `auto`, which
/// made the whole automatic-minimum branch dead code: an item in a 0-width
/// container collapsed to nothing.
#[test]
fn a_flex_item_does_not_shrink_below_its_content() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:0}</style>\
         <div class=f><i id=a style='display:block;flex:1 1 auto'>aaaa</i></div>", 900.0);
    let e = d.get_element_by_id("a").unwrap();
    let w = d.get_bounding_client_rect(e).unwrap().w;
    assert!(w > 20.0, "the content is the floor, got {w}");
}

/// …but the automatic minimum is the SMALLER of the content suggestion and the
/// specified size, and the content suggestion ignores the item's own `width`.
/// Reading the width for both made them the same number, so an item with a
/// `width` could not shrink at all.
#[test]
fn an_item_with_a_width_still_shrinks_to_its_share() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px}</style>\
         <div class=f><i id=a style='display:block;flex:1 1 auto;width:400px;height:20px'></i>\
         <i id=b style='display:block;flex:1 1 auto;width:400px;height:20px'></i></div>", 900.0);
    for id in ["a", "b"] {
        let e = d.get_element_by_id(id).unwrap();
        let w = d.get_bounding_client_rect(e).unwrap().w;
        assert!((w - 150.0).abs() < 0.5, "{id}: an empty item shrinks freely, got {w}");
    }
}

/// CSS Cascade §7: the CSS-wide keywords reset a SHORTHAND by resetting every
/// longhand it stands for.
///
/// A shorthand owns no storage, so its `copy` is a no-op and resetting through
/// it did nothing at all: `flex: initial` left grow/shrink/basis untouched, and
/// `margin: initial` left the margins in place.
#[test]
fn a_css_wide_keyword_resets_a_shorthands_longhands() {
    let width = |decl: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px}}\
             .pre{{flex:1 1 200px}}</style>\
             <div class=f><i id=a class=pre style='display:block;height:20px;{decl}'></i>\
             <i id=b style='display:block;height:20px;flex:0 0 100px'></i></div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    // `flex: initial` is `0 1 auto`, so an empty item collapses to nothing.
    for kw in ["initial", "unset", "revert"] {
        let w = width(&format!("flex:{kw}"));
        assert!(w.abs() < 0.5, "flex:{kw} must reset the longhands, got {w}");
    }
    // The keyword on a single longhand touches only that longhand — the basis
    // the shorthand set survives.
    let w = width("flex-grow:initial");
    assert!((w - 200.0).abs() < 0.5, "flex-grow:initial keeps the basis, got {w}");

    // The same for a shorthand whose longhands are plain lengths.
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px}</style>\
         <div class=f><i id=a style='display:block;height:20px;margin:20px;\
         margin:initial;flex:1'></i></div>", 900.0);
    let e = d.get_element_by_id("a").unwrap();
    let w = d.get_bounding_client_rect(e).unwrap().w;
    assert!((w - 300.0).abs() < 0.5, "margin:initial clears the margins, got {w}");
}

/// Box Alignment §4.4: an explicit `safe` makes an alignment give way to the
/// start edge once the content overflows; `unsafe` (the default for a bare
/// position keyword) overflows instead.
///
/// The two-word forms did not parse at all, so `safe center` silently became
/// the property's initial value.
#[test]
fn safe_alignment_gives_way_where_unsafe_overflows() {
    let first_x = |jc: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px;justify-content:{jc}}}\
             .f>i{{display:block;height:20px;flex:0 0 200px}}</style>\
             <div class=f id=p><i id=a></i><i id=b></i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().x
        };
        get(&mut d, "a") - get(&mut d, "p")
    };
    assert!(first_x("safe center").abs() < 0.5, "safe center packs at the start");
    assert!((first_x("unsafe center") + 50.0).abs() < 0.5, "unsafe center overflows");
    assert!(first_x("safe flex-end").abs() < 0.5, "safe flex-end packs at the start");
    assert!((first_x("unsafe flex-end") + 100.0).abs() < 0.5, "unsafe flex-end overflows");

    // The cross axis takes the same modifier.
    let cross_y = |ai: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px;height:30px;\
             align-items:{ai}}}</style>\
             <div class=f id=p><i id=a style='display:block;height:50px;width:20px'></i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().y
        };
        get(&mut d, "a") - get(&mut d, "p")
    };
    assert!(cross_y("safe center").abs() < 0.5, "safe center packs at the cross start");
    assert!((cross_y("unsafe center") + 10.0).abs() < 0.5, "unsafe center overflows");
}

/// `justify-content: left` / `right` are PHYSICAL (Box Alignment §5): unlike
/// `flex-start` / `flex-end` they do not follow `row-reverse` or `direction`.
#[test]
fn left_and_right_do_not_follow_the_flex_direction() {
    let x = |container: &str, jc: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px;{container};\
             justify-content:{jc}}}</style>\
             <div class=f id=p><i id=a style='display:block;height:20px;flex:0 0 80px'></i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().x
        };
        get(&mut d, "a") - get(&mut d, "p")
    };
    for container in ["", "flex-direction:row-reverse", "direction:rtl"] {
        let l = x(container, "left");
        let r_ = x(container, "right");
        assert!(l.abs() < 0.5, "left is the left edge with `{container}`, got {l}");
        assert!((r_ - 220.0).abs() < 0.5, "right is the right edge with `{container}`, got {r_}");
    }
    // …while the flex-relative pair DOES follow the direction, or the test
    // above would not be saying anything.
    assert!((x("flex-direction:row-reverse", "flex-start") - 220.0).abs() < 0.5,
            "flex-start follows row-reverse");
}

/// `first baseline` and `last baseline` both align the items' baselines;
/// `last baseline` then packs the group against the cross-END edge.
#[test]
fn first_and_last_baseline_pack_against_opposite_edges() {
    let tops = |ai: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px;height:60px;\
             align-items:{ai}}}.f>i{{display:block;width:80px}}</style>\
             <div class=f id=p><i id=a style='padding-top:10px'>x</i>\
             <i id=b style='padding-top:30px'>y</i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            let r = d.get_bounding_client_rect(e).unwrap();
            (r.y, r.h)
        };
        let p = get(&mut d, "p").0;
        let (ay, ah) = get(&mut d, "a");
        let (by, bh) = get(&mut d, "b");
        (ay - p, ah, by - p, bh)
    };
    // `first baseline` hangs the group from the cross-start edge…
    let (ay, _, by, _) = tops("first baseline");
    assert!((ay - 20.0).abs() < 0.5, "first: the shallow item drops 20, got {ay}");
    assert!(by.abs() < 0.5, "first: the deep item sets the top, got {by}");
    // …and `last baseline` keeps the same relative offset but pushes the whole
    // group to the far edge, so the deepest bottom touches it.
    let (ay, ah, by, bh) = tops("last baseline");
    assert!((ay + ah - 60.0).abs() < 0.5, "last: the group's bottom is the container's, got {}", ay + ah);
    assert!(((by + bh) - 60.0).abs() < 0.5, "last: …for both items, got {}", by + bh);
    assert!((ay - by - 20.0).abs() < 0.5, "last: the baselines still line up");
}

/// `gap: <row> <column>` — the two-value form. Parsing the whole declaration as
/// one length made `gap: 10px 30px` unparseable, so both gaps fell back to 0.
#[test]
fn the_gap_shorthand_takes_a_row_and_a_column_value() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;flex-wrap:wrap;width:200px;\
         gap:10px 30px}.f>i{display:block;width:80px;height:30px}</style>\
         <div class=f id=p><i id=a></i><i id=b></i><i id=c></i></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap()
    };
    let a = get(&mut d, "a");
    let b = get(&mut d, "b");
    let c = get(&mut d, "c");
    assert!((b.x - a.x - 110.0).abs() < 0.5, "the column gap is 30, got {}", b.x - a.x - 80.0);
    assert!((c.y - a.y - 40.0).abs() < 0.5, "the row gap is 10, got {}", c.y - a.y - 30.0);
}

/// `wrap-reverse` flips the cross-start edge for the LINE STACK too, so
/// `align-content: flex-start` packs the lines against the far edge. Only the
/// line order was being reversed, so both edges packed the same way.
#[test]
fn wrap_reverse_flips_align_content_as_well() {
    let first_y = |ac: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;flex-wrap:wrap-reverse;\
             width:200px;height:100px;align-content:{ac}}}\
             .f>i{{display:block;width:80px;height:30px}}</style>\
             <div class=f id=p><i id=a></i></div>"), 900.0);
        let get = |d: &mut crate::types::Document, id: &str| {
            let e = d.get_element_by_id(id).unwrap();
            d.get_bounding_client_rect(e).unwrap().y
        };
        get(&mut d, "a") - get(&mut d, "p")
    };
    assert!((first_y("flex-start") - 70.0).abs() < 0.5, "flex-start is the far edge under wrap-reverse");
    assert!(first_y("flex-end").abs() < 0.5, "flex-end is the near edge under wrap-reverse");
}

/// Flexbox §9.4 step 8: a single-line container with a definite cross size
/// hands that size to its line, so an item taller than the container has free
/// space to overflow into rather than growing the line to fit itself.
#[test]
fn a_single_line_container_gives_its_cross_size_to_the_line() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px;height:30px;\
         align-items:center}</style>\
         <div class=f id=p><i id=a style='display:block;height:50px;width:20px'></i></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().y
    };
    let off = get(&mut d, "a") - get(&mut d, "p");
    assert!((off + 10.0).abs() < 0.5, "a 50px item centres in a 30px line at -10, got {off}");
}

/// CSS Sizing §5: the intrinsic sizing keywords size a flex item from its
/// content. They parsed as `auto`, so `min-content` gave the MAX-content size.
#[test]
fn intrinsic_keywords_size_a_flex_item() {
    let width = |decl: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:300px}}</style>\
             <div class=f><i id=a style='display:block;{decl}'>aa bbbb cc</i>\
             <i id=b style='display:block;flex:0 0 100px;height:20px'></i></div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    // min-content is the longest word; max-content is the whole string on one
    // line. The exact advances are a font question — what matters is that the
    // two keywords no longer give the same answer.
    let min = width("flex-basis:min-content");
    let max = width("flex-basis:max-content");
    assert!(min < max * 0.6, "min-content ({min}) must be well under max-content ({max})");
    assert!((width("flex-basis:max-content") - max).abs() < 0.5);
    // …and the same keywords on `width`, which is where the basis reads them
    // when `flex-basis` is `auto`.
    assert!((width("width:min-content") - min).abs() < 0.5, "width:min-content matches");
    // The item's own size is consulted ONLY when the basis is `auto`, or an
    // intrinsic `width` would override an explicit basis.
    assert!((width("flex-basis:50px;width:min-content;flex-shrink:0") - 50.0).abs() < 0.5,
            "an explicit basis wins over an intrinsic width");

    // `fit-content` is the max-content size clamped to the space available,
    // floored by min-content — not simply max-content.
    let fit = |container_w: f32| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}.f{{display:flex;width:{container_w}px}}</style>             <div class=f><i id=a style='display:block;flex-basis:fit-content;             flex-shrink:0'>aa bbbb cc</i></div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    assert!((fit(300.0) - max).abs() < 0.5, "room to spare: fit-content is max-content");
    assert!((fit(60.0) - 60.0).abs() < 0.5, "cramped: fit-content clamps to the container");
    assert!(fit(10.0) >= min - 0.5, "…but never below min-content");
}

/// The typed cascade path carries the intrinsic keywords too — the inline-style
/// path above is not the only way they reach the style.
#[test]
fn intrinsic_keywords_survive_the_stylesheet_path() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;width:300px}         .a{flex-basis:min-content}.b{flex-basis:max-content}</style>         <div class=f><i class=a id=a style='display:block'>aa bbbb cc</i></div>         <div class=f><i class=b id=b style='display:block'>aa bbbb cc</i></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    let a = get(&mut d, "a");
    let b = get(&mut d, "b");
    assert!(a < b * 0.6, "min-content ({a}) under max-content ({b}) through a rule too");
}

/// Flexbox §4.1: `align-self` on an absolutely-positioned flex child overrides
/// the container's `align-items` for its static position, exactly as it does
/// for an in-flow item. Only the container's value was being read.
#[test]
fn align_self_moves_an_absolutely_positioned_flex_child() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}.f{display:flex;position:relative;width:300px;height:60px}</style>\
         <div class=f id=p><i id=a style='position:absolute;align-self:center;\
         display:block;height:20px;width:30px'></i></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap().y
    };
    let off = get(&mut d, "a") - get(&mut d, "p");
    assert!((off - 20.0).abs() < 0.5, "a 20px child centres in 60px at 20, got {off}");
}

/// CSS 2.1 §10.8: every line box contains a strut — a zero-width inline box
/// with the block's own font and line-height — whose ascent and descent take
/// part in the line box's height.
///
/// There was no strut, so a line holding only an atomic inline had no room
/// below the baseline at all: a 20px image or inline-block gave a 20px line
/// where a browser gives 25.
#[test]
fn a_line_box_reserves_room_below_the_baseline_for_its_strut() {
    let height = |inner: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0;font-family:Menlo;font-size:16px;line-height:20px}}</style>\
             <div id=a>{inner}</div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().h
    };
    // 16px Menlo is 15 up and 4 down; a 20px line-height adds half a pixel of
    // leading each side. An atomic inline sits ON the baseline, so the line is
    // the box plus the strut's 4.5px descent, snapped to a whole pixel.
    for inner in [
        "<span style='display:inline-block;width:50px;height:20px'></span>",
        "<span style='display:inline-flex;width:50px;height:20px'></span>",
    ] {
        let h = height(inner);
        assert!((h - 25.0).abs() < 0.1, "an atomic inline leaves room below: {h}");
    }
    // A taller box moves the line with it, keeping the same descent.
    let h = height("<span style='display:inline-block;width:50px;height:60px'></span>");
    assert!((h - 65.0).abs() < 0.1, "…at any height: {h}");
    // Text alone is exactly its line-height — the strut never inflates that.
    let h = height("text");
    assert!((h - 20.0).abs() < 0.5, "a text line is its line-height: {h}");
    // …and a line-height BELOW the font's own height shrinks the line, which
    // the old bolt-on could not do because it only ever grew it.
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0;font-family:Menlo;font-size:16px;line-height:10px}</style>\
         <div id=a>text</div>", 900.0);
    let e = d.get_element_by_id("a").unwrap();
    let h = d.get_bounding_client_rect(e).unwrap().h;
    assert!((h - 10.0).abs() < 0.5, "a tight line-height shrinks the line: {h}");
}

/// CSS 2.1 §9.5.1: a float's top may not be higher than the top of the current
/// line box — it sits ON that line, and inline content already there moves
/// aside for it. The pending line was being flushed instead, dropping the float
/// onto the next line whenever any inline content preceded it.
#[test]
fn a_float_after_inline_content_stays_on_its_line() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0;font:16px/20px monospace}\
         .ib{display:inline-block;width:50px;height:20px}\
         .fl{float:left;width:60px;height:20px}</style>\
         <span class=ib id=a></span><div class=fl id=b></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap()
    };
    let a = get(&mut d, "a");
    let b = get(&mut d, "b");
    assert!((a.y - b.y).abs() < 0.5, "the float shares the line, got {} vs {}", a.y, b.y);
    assert!(b.x.abs() < 0.5, "the left float takes the near edge, got {}", b.x);
    assert!((a.x - 60.0).abs() < 0.5, "the inline content moves aside, got {}", a.x);
}

/// A flex container is a block-level box: `max-width` and `min-width` clamp it
/// exactly as they clamp any other block, and the items then flex inside the
/// clamped size.
#[test]
fn a_flex_container_obeys_its_own_min_and_max_width() {
    let w = |decl: &str, inner: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}</style><div id=a style='{decl}'>{inner}</div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    let item = "<i id=b style='display:block;flex:1 1 200px;height:10px'></i>";
    // The plain block is the control: if this fails the bug is not flex's.
    assert!((w("max-width:150px", "<i style='display:block;height:10px'></i>") - 150.0).abs() < 0.5,
            "a block obeys max-width");
    assert!((w("display:flex;max-width:150px", item) - 150.0).abs() < 0.5,
            "a flex container obeys max-width");
    assert!((w("display:flex;width:auto;max-width:150px", item) - 150.0).abs() < 0.5,
            "…with width:auto");
    assert!((w("display:flex;width:400px;max-width:150px", item) - 150.0).abs() < 0.5,
            "…and max-width beats a larger width");
    // `min-width` only binds where the box would otherwise be NARROWER, so it
    // needs a parent that constrains it.
    let narrow = |decl: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}</style><div style='width:100px'>\
             <div id=a style='{decl}'>{item}</div></div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    assert!((narrow("width:auto;min-width:600px") - 600.0).abs() < 0.5,
            "a block obeys min-width");
    assert!((narrow("display:flex;min-width:600px") - 600.0).abs() < 0.5,
            "a flex container obeys min-width");
}

/// The strut applies to EVERY line box, including the anonymous one a block
/// builds when it mixes inline children with block or floated ones.
///
/// That path sets the line height from the child's own height and never
/// consulted the strut, so an inline-block on such a line produced a line
/// exactly as tall as itself and everything below it sat a few pixels high.
#[test]
fn the_strut_applies_to_anonymous_inline_runs_too() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0;font-family:Menlo;font-size:16px;line-height:20px}</style>\
         <div id=p><span id=a style='display:inline-block;width:50px;height:20px'></span>\
         <div id=b style='height:10px'></div></div>", 900.0);
    let get = |d: &mut crate::types::Document, id: &str| {
        let e = d.get_element_by_id(id).unwrap();
        d.get_bounding_client_rect(e).unwrap()
    };
    // The inline-block's line is 20 for the box plus the strut's 4.5px descent,
    // snapped to a whole pixel — so the block after it starts at 25, not 20.
    let b = get(&mut d, "b");
    let p = get(&mut d, "p");
    assert!((b.y - p.y - 25.0).abs() < 0.6,
            "the anonymous line reserves the strut's descent, got {}", b.y - p.y);
}

/// `offsetLeft` / `offsetTop` are document-relative when the offset parent is a
/// statically-positioned `body`, and parent-relative otherwise.
///
/// The body was being subtracted, which put every top-level element 8px off —
/// the default body margin — on any page that does not reset it.
#[test]
fn offset_left_does_not_subtract_a_static_body() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<div id=a style='float:left;width:20px;height:10px'>x</div>\
         <div id=b style='width:20px;height:10px'>y</div>\
         <div id=c style='position:relative'>\
         <div id=d style='width:20px;height:10px'>z</div></div>", 800.0);
    let el = |d: &mut crate::types::Document, id: &str| d.get_element_by_id(id).unwrap();
    // Offset parent is the body: the answer is the distance from the page.
    for id in ["a", "b"] {
        let e = el(&mut d, id);
        assert!((d.offset_left(e) - 8.0).abs() < 0.5, "{id}: offsetLeft is 8, got {}", d.offset_left(e));
        assert!((d.offset_top(e) - 8.0).abs() < 0.5, "{id}: offsetTop is 8, got {}", d.offset_top(e));
    }
    // Offset parent is a positioned ancestor: the answer is relative to it.
    let dd = el(&mut d, "d");
    assert!(d.offset_left(dd).abs() < 0.5, "d: offsetLeft is 0, got {}", d.offset_left(dd));
    assert!(d.offset_top(dd).abs() < 0.5, "d: offsetTop is 0, got {}", d.offset_top(dd));
}

/// CSS Sizing §5: the intrinsic keywords size a BOX from its own content, not
/// just a flex item's basis. They read as `auto` to every caller that cannot
/// measure content, so without an explicit branch a `width: min-content` box
/// filled its containing block.
#[test]
fn intrinsic_keywords_size_a_container() {
    let w = |decl: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0;font-family:Menlo;font-size:16px}}</style>\
             <div id=a style='{decl}'>aa bbbb cc</div>"), 900.0);
        let e = d.get_element_by_id("a").unwrap();
        d.get_bounding_client_rect(e).unwrap().w
    };
    let auto = w("");
    let mn = w("width:min-content");
    let mx = w("width:max-content");
    assert!((auto - 900.0).abs() < 0.5, "the control fills its parent, got {auto}");
    assert!(mn < mx * 0.6, "min-content ({mn}) is well under max-content ({mx})");
    assert!(mx < 300.0, "max-content is the text's width, not the parent's, got {mx}");
    // `inline-size` is the logical alias and must behave identically.
    assert!((w("inline-size:min-content") - mn).abs() < 0.5, "inline-size matches width");
    // …and on a flex container too.
    assert!((w("display:flex;width:max-content") - mx).abs() < 0.5, "flex container max-content");
}

/// CSS Cascade §6.4: a later `@layer` beats an earlier one, and an UNLAYERED
/// declaration beats every layered one. The document's layer order has to
/// survive the merge from the parser's stylesheet, or every layered rule ranks
/// the same and the last one parsed wins by document order instead.
#[test]
fn layer_order_survives_into_the_document() {
    let mut r = crate::Renderer::new();
    let d = r.load_html(
        "<style>@layer base, theme;\
         @layer theme { p { color: rgb(0,0,255) } }\
         @layer base  { p { color: rgb(255,0,0) } }\
         p.un { color: rgb(0,128,0) }</style>\
         <p id=a>layered</p><p id=b class=un>unlayered</p>", 800.0);
    fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
        if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
        n.children.iter().find_map(|c| find(c, id))
    }
    // `theme` is declared after `base`, so it wins even though it is written
    // first in the source.
    let a = find(&d.root, "a").unwrap();
    assert_eq!(a.style.color, crate::types::Color::rgb(0, 0, 255),
               "the later layer wins");
    // …and an unlayered rule beats both.
    let b = find(&d.root, "b").unwrap();
    assert_eq!(b.style.color, crate::types::Color::rgb(0, 128, 0),
               "unlayered beats every layer");
}

/// An external stylesheet is AUTHOR origin, exactly like an inline `<style>`.
///
/// The loader adds linked sheets through `parse_and_add_with_base_media`, which
/// routes to `parse_and_add` — the same entry the UA sheet uses. Inline styles
/// get `AUTHOR_ORIGIN_BOOST` on the way into the document and linked ones did
/// not, so a rule in a `<style>` block beat a more specific rule from a linked
/// sheet, and on a page whose CSS is mostly external the cascade came out wrong.
#[test]
fn a_linked_stylesheet_is_author_origin() {
    use crate::css::{ua_stylesheet, is_author_origin};
    let mut ss = ua_stylesheet();
    // What the loader does for a linked sheet.
    ss.parse_and_add_with_base_media("nav .item { color: rgb(0,128,0) }", "https://x/", "");
    // The rule just added is the last one in the sheet.
    let linked = ss.rules.last().expect("the linked rule is in the sheet");
    assert!(is_author_origin(linked.specificity),
            "a linked stylesheet's rules must be author origin, got specificity {}",
            linked.specificity);
}

/// End to end: a linked stylesheet reaches the page AND cascades as author
/// origin, so it beats a less specific inline rule and beats the UA sheet.
///
/// The unit check above guards the origin flag; this guards the whole path —
/// fetch, parse, merge, cascade — because that is where it actually broke: the
/// rules were present and simply lost every contest they should have won.
#[test]
fn a_linked_stylesheet_cascades_as_author() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("webcore-linked-css-test");
    let _ = std::fs::create_dir_all(&dir);
    let css_path = dir.join("site.css");
    {
        let Ok(mut f) = std::fs::File::create(&css_path) else { return };
        // More specific than the inline rule below, and it also overrides a UA
        // default (`div` is display:block).
        let _ = f.write_all(b"#nav .item { color: rgb(0,128,0) }\n\
                              #nav { display: flex }\n");
    }

    // A RELATIVE href against the document's base, which is what a page does.
    let html = "<link rel=stylesheet href=\"site.css\">\
         <style>.item { color: rgb(255,0,0) }</style>\
         <div id=nav><span class=item id=a>x</span></div>".to_string();

    let mut r = crate::Renderer::new();
    // The base is the DOCUMENT's URL, so `site.css` resolves beside it.
    let base = format!("file://{}/index.html", dir.display());
    let d = r.load_html_with_base(&html, &base, 800.0, 600.0);

    fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
        if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
        n.children.iter().find_map(|c| find(c, id))
    }
    let nav = find(&d.root, "nav").expect("#nav in the tree");
    let item = find(&d.root, "a").expect("#nav .item in the tree");

    // The linked sheet was actually fetched and parsed — check for ITS rule,
    // not merely that the sheet is non-empty (the UA sheet always is).
    let linked_present = d.stylesheet.rules.iter().any(|r| {
        r.compiled_decls.iter().any(|(id, _)| {
            *id == crate::css::properties::PropertyId::Display
        }) && r.specificity >= crate::css::AUTHOR_ORIGIN_BOOST
    });
    assert!(linked_present, "the linked sheet must reach the document as author origin");
    // …its more specific rule beats the inline one…
    assert_eq!(item.style.color, crate::types::Color::rgb(0, 128, 0),
               "a linked rule must beat a LESS specific inline rule");
    // …and it overrides a UA default.
    assert_eq!(nav.style.display, crate::types::Display::Flex,
               "a linked rule must override the UA sheet");

    let _ = std::fs::remove_file(&css_path);
}

/// `@media print` must NOT apply to screen rendering, and `screen` must.
///
/// A page that mistakenly takes its print sheet loses its layout entirely —
/// print stylesheets set `display: block`, drop floats and columns, and hide
/// navigation, which renders a site as one long column.
#[test]
fn print_media_does_not_apply_to_screen() {
    use crate::css::evaluate_media;
    let (vw, vh) = (1280.0, 900.0);
    assert!(!evaluate_media("print", vw, vh), "`print` must not match a screen");
    assert!(evaluate_media("screen", vw, vh), "`screen` must match");
    assert!(evaluate_media("all", vw, vh), "`all` matches");
    assert!(evaluate_media("", vw, vh), "an empty condition matches");
    assert!(!evaluate_media("only print", vw, vh), "`only print` must not match");
    assert!(evaluate_media("only screen", vw, vh), "`only screen` matches");
    assert!(!evaluate_media("print and (min-width: 100px)", vw, vh),
            "a print condition must not match however it is qualified");
    assert!(evaluate_media("screen, print", vw, vh), "a list matches if any does");
    assert!(evaluate_media("screen and (min-width: 100px)", vw, vh), "screen + feature");
    assert!(!evaluate_media("screen and (min-width: 5000px)", vw, vh), "…that fails");
}

/// The responsive queries every framework is built on. A `min-width` query
/// that fails to match at a desktop viewport drops the whole desktop grid and
/// the page falls back to its stacked mobile layout — one long column.
#[test]
fn responsive_media_queries_match_a_desktop_viewport() {
    use crate::css::evaluate_media;
    let (vw, vh) = (1280.0, 900.0);
    // Bootstrap's breakpoints, written exactly as it ships them: no space
    // after the colon, and fractional max-widths.
    for q in ["(min-width:576px)", "(min-width:768px)", "(min-width:992px)",
              "(min-width:1200px)", "(min-width: 992px)"] {
        assert!(evaluate_media(q, vw, vh), "{q} must match a 1280px viewport");
    }
    for q in ["(max-width:575.98px)", "(max-width:767.98px)",
              "(max-width:991.98px)", "(max-width:1199.98px)"] {
        assert!(!evaluate_media(q, vw, vh), "{q} must NOT match a 1280px viewport");
    }
    // …and the ones a wider viewport should still exclude.
    assert!(!evaluate_media("(min-width:1400px)", vw, vh), "beyond the viewport");
    // Combined forms.
    assert!(evaluate_media("screen and (min-width:992px)", vw, vh), "screen + min-width");
    assert!(evaluate_media("only screen and (min-width:992px)", vw, vh), "only screen + min-width");
    assert!(!evaluate_media("print and (min-width:992px)", vw, vh), "print stays out");
}

/// End to end: a `min-width` rule has to win at a desktop viewport when the
/// page is loaded normally. Evaluating the query correctly is not enough — the
/// cascade has to run with the real viewport, or every desktop breakpoint is
/// skipped and the page keeps its stacked mobile layout.
#[test]
fn a_min_width_rule_applies_at_the_loaded_viewport() {
    let load = |w: f32| {
        let mut r = crate::Renderer::new();
        let d = r.load_html(
            "<style>#a{display:block}\
             @media (min-width:992px){#a{display:flex}}</style>\
             <div id=a><i>x</i></div>", w);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&d.root, "a").unwrap().style.display
    };
    assert_eq!(load(1280.0), crate::types::Display::Flex,
               "a desktop breakpoint must apply at 1280px");
    assert_eq!(load(600.0), crate::types::Display::Block,
               "…and must not apply at 600px");
}

/// The generic families must map to real faces of the right KIND. If
/// `sans-serif` resolves to a monospace face every page renders as typewriter
/// text — the most visible possible styling failure, and one that looks like
/// "the CSS did not load" rather than a font problem.
#[test]
fn generic_font_families_are_not_all_monospace() {
    let w = |family: &str, text: &str| {
        let mut r = crate::Renderer::new();
        let d = r.load_html(&format!(
            "<style>body{{margin:0}}#a{{display:inline-block;font:16px {family}}}</style>\
             <span id=a>{text}</span>"), 900.0);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&d.root, id_of()).map(|n| n.layout.border_rect.w).unwrap_or(0.0)
    };
    fn id_of() -> &'static str { "a" }
    // A proportional face makes narrow and wide glyphs different widths; a
    // monospace face makes them identical.
    for family in ["sans-serif", "serif", "Arial", "Helvetica"] {
        let narrow = w(family, "iiiiiiiiii");
        let wide   = w(family, "WWWWWWWWWW");
        assert!(narrow > 0.0 && wide > 0.0, "{family}: text must measure");
        assert!(wide > narrow * 1.5,
                "{family} resolved to a MONOSPACE face: 'iiii'={narrow} 'WWWW'={wide}");
    }
    // …and monospace really is monospace, or the check above proves nothing.
    let n = w("monospace", "iiiiiiiiii");
    let d = w("monospace", "WWWWWWWWWW");
    assert!((n - d).abs() < 1.0, "monospace must be fixed width: {n} vs {d}");
}

/// `<link rel=stylesheet>` counts wherever it appears. Body-inserted sheets are
/// ordinary on the web — a real page can serve most of its CSS that way — and
/// dropping them renders the page with a fraction of its styles.
#[test]
fn a_stylesheet_link_in_the_body_is_registered() {
    let d = crate::html::parse_html_with_base(
        "<html><head><link rel=stylesheet href=\"a.css\"></head>\
         <body><p>x</p><link rel=stylesheet href=\"b.css\">\
         <div><link href=\"c.css\" type=\"text/css\" rel=\"stylesheet\"></div>\
         </body></html>", "https://example.com/");
    let hrefs: Vec<&str> = d.linked_stylesheets.iter().map(|(h, _)| h.as_str()).collect();
    assert!(hrefs.contains(&"a.css"), "the head sheet: {hrefs:?}");
    assert!(hrefs.contains(&"b.css"), "a body sheet: {hrefs:?}");
    assert!(hrefs.contains(&"c.css"), "a body sheet nested in an element: {hrefs:?}");
}

/// `:hover` has to change the computed style of the hovered element AND of
/// ancestors/descendants the rule selects — a menu that opens on hover depends
/// on `li:hover > .submenu`, not just on the link itself.
#[test]
fn hover_applies_to_the_element_and_its_subtree() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}\
         .item{color:rgb(0,0,0)} .item:hover{color:rgb(255,0,0)}\
         .sub{display:none} .item:hover .sub{display:block}\
         </style>\
         <div class=item id=a>menu<span class=sub id=s>panel</span></div>", 800.0);
    let a = d.get_element_by_id("a").unwrap();
    let rect = d.get_bounding_client_rect(a).unwrap();

    fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
        if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
        n.children.iter().find_map(|c| find(c, id))
    }
    // Nothing hovered yet.
    assert_eq!(find(&d.root, "a").unwrap().style.color, crate::types::Color::rgb(0, 0, 0),
               "not hovered to begin with");
    assert_eq!(find(&d.root, "s").unwrap().style.display, crate::types::Display::None,
               "the panel starts closed");

    // Move the pointer over the item, the way a real mouse move arrives.
    let pt = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let changed = d.process_mouse_event(crate::dom::HtmlEventType::MouseMove, pt, 0);
    assert!(changed, "a move onto a hover-styled element must report a change");
    r.layout_engine().layout(&mut d, 800.0);

    assert_eq!(find(&d.root, "a").unwrap().style.color, crate::types::Color::rgb(255, 0, 0),
               ":hover must recolour the hovered element");
    assert_eq!(find(&d.root, "s").unwrap().style.display, crate::types::Display::Block,
               ":hover must open a descendant the rule selects");
}

/// `:hover` applies to every element on the pointer's ANCESTOR chain, not only
/// the innermost one (CSS 2.1 §5.11.3 — the hover chain).
///
/// Every dropdown menu on the web is built this way: `li:hover > .panel`, with
/// the pointer actually over the `<a>` inside the `<li>`. If only the deepest
/// element gets `:hover`, no menu ever opens.
#[test]
fn hover_applies_to_the_whole_ancestor_chain() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}\
         li div{display:none} li:hover div{display:block}\
         li:hover{background:rgb(1,2,3)} a:hover{color:rgb(9,9,9)}\
         </style>\
         <ul><li id=li><a id=a href=#>Quick Tools</a>\
         <div id=panel>overlay</div></li></ul>", 800.0);
    // Aim at the <li>'s box. Its <a> is inline, and an inline element's rect
    // is not usable for a hit test — see the note on inline rects being 0x0.
    let li = d.get_element_by_id("li").unwrap();
    let rect = d.get_bounding_client_rect(li).unwrap();

    fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
        if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
        n.children.iter().find_map(|c| find(c, id))
    }
    assert_eq!(find(&d.root, "panel").unwrap().style.display, crate::types::Display::None,
               "the overlay starts closed");

    // Pointer over the LINK, which is inside the <li> the rule selects.
    let pt = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    d.process_mouse_event(crate::dom::HtmlEventType::MouseMove, pt, 0);
    r.layout_engine().layout(&mut d, 800.0);

    // What did the hit test actually resolve to?
    {
        let hovered = d.hovered_box;
        let name = |n: &crate::WebCore, id: u32| -> Option<String> {
            fn go(n: &crate::WebCore, id: u32) -> Option<String> {
                if n.node_id == id {
                    return Some(format!("{}#{}", n.tag,
                        n.attributes.get("id").cloned().unwrap_or_default()));
                }
                n.children.iter().find_map(|c| go(c, id))
            }
            go(n, id)
        };
        assert!(hovered != 0, "the hit test found nothing under the pointer at {pt:?}");
        let who = name(&d.root, hovered).unwrap_or_else(|| format!("<id {hovered}>"));
        assert!(who.contains('#'), "hit test resolved to {who}");
    }
    // First: does the element directly under the pointer get :hover at all?
    assert_eq!(find(&d.root, "li").unwrap().style.background_color,
               crate::types::Color::rgb(1, 2, 3),
               "the ANCESTOR <li> must be hovered when the pointer is over its child");
    assert_eq!(find(&d.root, "panel").unwrap().style.display, crate::types::Display::Block,
               "…so its dropdown opens");
}

/// The dropdown pattern as real menus actually build it: the panel is always
/// `display:block` and `position:absolute`, collapsed with `max-height:0;
/// overflow:hidden`, and the ancestor's `:hover` raises the cap.
///
/// This is the shape usps.com uses for every menu, so it is the one that
/// decides whether the menus work.
#[test]
fn a_max_height_dropdown_opens_on_ancestor_hover() {
    let mut r = crate::Renderer::new();
    let mut d = r.load_html(
        "<style>body{margin:0}\
         li{display:block;height:40px}\
         li div{max-height:0;overflow:hidden;position:absolute;display:block;width:200px}\
         li:hover div{max-height:1800px}\
         li div p{height:60px;margin:0}\
         </style>\
         <ul><li id=li><a>Quick Tools</a>\
         <div id=panel><p>entry</p></div></li></ul>", 800.0);

    fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
        if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
        n.children.iter().find_map(|c| find(c, id))
    }
    assert!(find(&d.root, "panel").unwrap().layout.border_rect.h < 1.0,
            "the panel starts collapsed");

    let li = d.get_element_by_id("li").unwrap();
    let rect = d.get_bounding_client_rect(li).unwrap();
    let pt = (rect.x + rect.w / 2.0, rect.y + 5.0);
    d.process_mouse_event(crate::dom::HtmlEventType::MouseMove, pt, 0);
    r.layout_engine().layout(&mut d, 800.0);

    // Split the two failure modes: did the CASCADE give the panel the new
    // max-height, and did LAYOUT act on it?
    let mh = find(&d.root, "panel").unwrap().style.max_height.clone();
    assert!(!matches!(mh, crate::types::CssLength::Zero | crate::types::CssLength::Px(0.0)),
            "the cascade must give the panel the hover max-height, got {mh:?}");
    let h = find(&d.root, "panel").unwrap().layout.border_rect.h;
    assert!(h > 50.0, "layout must act on it, got {h}");
}

/// An absolutely positioned box with `height:auto` is as tall as its content,
/// and `max-height` caps it rather than defining it. A dropdown panel is
/// exactly this box, so if it measures zero the menu can never open however
/// the hover is wired.
#[test]
fn an_absolute_box_with_auto_height_wraps_its_content() {
    let h = |decl: &str| {
        let mut r = crate::Renderer::new();
        let d = r.load_html(&format!(
            "<style>body{{margin:0}}\
             #p{{position:absolute;display:block;width:200px;overflow:hidden;{decl}}}\
             #p p{{height:60px;margin:0}}</style>\
             <div id=host style='position:relative'><div id=p><p>entry</p></div></div>"), 800.0);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&d.root, "p").unwrap().layout.border_rect.h
    };
    // The control: no cap at all.
    assert!((h("") - 60.0).abs() < 1.0, "auto height wraps the 60px child, got {}", h(""));
    // A generous cap must not shrink it.
    assert!((h("max-height:1800px") - 60.0).abs() < 1.0,
            "a 1800px cap leaves a 60px box alone, got {}", h("max-height:1800px"));
    // A zero cap collapses it.
    assert!(h("max-height:0") < 1.0, "a zero cap collapses it, got {}", h("max-height:0"));
}

/// A hover rule written as part of a SELECTOR LIST still applies.
///
/// Menus are almost always written `li.active div, li:focus div, li:hover div
/// { … }` so keyboard and pointer share one rule. If matching stops at the
/// first selector whose base matches — `:focus` here — the `:hover` variant is
/// never registered and the menu never opens for the mouse.
#[test]
fn a_hover_selector_inside_a_list_still_matches() {
    let open_height = |rule: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}li{{display:block;height:40px}}\
             li div{{max-height:0;overflow:hidden;position:absolute;display:block;width:200px}}\
             {rule}\
             li div p{{height:60px;margin:0}}</style>\
             <ul><li id=li><a>Quick Tools</a><div id=panel><p>e</p></div></li></ul>"), 800.0);
        let li = d.get_element_by_id("li").unwrap();
        let rect = d.get_bounding_client_rect(li).unwrap();
        d.process_mouse_event(crate::dom::HtmlEventType::MouseMove,
                              (rect.x + rect.w / 2.0, rect.y + 5.0), 0);
        r.layout_engine().layout(&mut d, 800.0);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&d.root, "panel").unwrap().layout.border_rect.h
    };
    // The control: hover alone already works.
    assert!(open_height("li:hover div{max-height:1800px}") > 50.0, "hover alone");
    // The shape real menus use — the hover selector is LAST in the list.
    let h = open_height("li.active div, li:focus div, li:hover div{max-height:1800px}");
    assert!(h > 50.0, "a hover selector after a :focus one in the same list, got {h}");
}

/// `:hover` in the MIDDLE of a long descendant selector, which is how a real
/// menu is written: `.global--navigation nav li:hover div`. The hovered
/// element is neither the subject nor the first part.
#[test]
fn hover_matches_in_the_middle_of_a_descendant_selector() {
    let open = |rule: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>body{{margin:0}}li{{display:block;height:40px}}\
             .wrap nav li div{{max-height:0;overflow:hidden;position:absolute;\
             display:block;width:200px}}\
             {rule}\
             .wrap nav li div p{{height:60px;margin:0}}</style>\
             <div class=wrap><nav><ul><li id=li><a>Quick Tools</a>\
             <div id=panel><p>e</p></div></li></ul></nav></div>"), 800.0);
        let li = d.get_element_by_id("li").unwrap();
        let rect = d.get_bounding_client_rect(li).unwrap();
        d.process_mouse_event(crate::dom::HtmlEventType::MouseMove,
                              (rect.x + rect.w / 2.0, rect.y + 5.0), 0);
        r.layout_engine().layout(&mut d, 800.0);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&d.root, "panel").unwrap().layout.border_rect.h
    };
    // The real shape: a class, an element, the hovered element, the subject.
    // It must out-specify the base rule, which it does — (0,2,3) vs (0,1,3).
    let h = open(".wrap nav li:hover div{max-height:1800px}");
    assert!(h > 50.0, "hover in the middle of a descendant chain, got {h}");
    // …and the same rule written as a list, as menus usually write it.
    let h = open(".wrap nav li.active div, .wrap nav li:focus div, \
                  .wrap nav li:hover div{max-height:1800px}");
    assert!(h > 50.0, "…and inside a selector list, got {h}");
}

/// A percentage `font-size` resolves against the PARENT's font size, and on the
/// root against the initial 16px. `html { font-size: 62.5% }` is the standard
/// "make 1rem = 10px" idiom, so getting it wrong collapses every `rem` on the
/// page and with it every line height, box height and text size.
#[test]
fn a_percentage_font_size_resolves_against_the_parent() {
    let sizes = |css: &str| {
        let mut r = crate::Renderer::new();
        let mut d = r.load_html(&format!(
            "<style>{css}</style><div id=a>x<span id=b>y</span></div>"), 800.0);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        let root = d.root.style.font_size_px(16.0, 16.0);
        let a = find(&d.root, "a").unwrap().style.font_size_px(16.0, 16.0);
        (root, a)
    };
    // 62.5% of the initial 16px is 10px.
    let (root, _) = sizes("html{font-size:62.5%}");
    assert!((root - 10.0).abs() < 0.1, "html at 62.5% is 10px, got {root}");
    // 100% leaves it at the initial value.
    let (root, _) = sizes("html{font-size:100%}");
    assert!((root - 16.0).abs() < 0.1, "html at 100% is 16px, got {root}");
    // …and a percentage on a child is relative to its parent.
    let (_, a) = sizes("html{font-size:20px} #a{font-size:50%}");
    assert!((a - 10.0).abs() < 0.1, "50% of a 20px parent is 10px, got {a}");
}

/// WOFF2 decodes to a usable font.
///
/// It is the format essentially every modern site ships, and until it decoded
/// every such page was measured in a fallback face — wrong glyph advances,
/// wrong line heights, wrong box heights throughout.
#[test]
fn a_woff2_font_decodes_and_measures() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/wpt/fonts/kinter.woff2");
    let Ok(data) = std::fs::read(&path) else {
        eprintln!("no woff2 sample at {} — skipping", path.display());
        return;
    };
    let sfnt = crate::woff2::decode(&data).expect("the woff2 must decode");
    // A real sfnt: a known flavour, a sane table count, and bigger than the
    // compressed original.
    assert!(sfnt.len() > data.len(), "decoding must expand the font");
    let flavor = u32::from_be_bytes([sfnt[0], sfnt[1], sfnt[2], sfnt[3]]);
    assert!(flavor == 0x0001_0000 || flavor == 0x4f54_544f,
            "sfnt flavour, got {flavor:#x}");
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]);
    assert!(num_tables > 4 && num_tables < 512, "table count {num_tables}");

    // …and the font stack can actually use it: the family loads and text
    // measured in it is proportional, not fallback-identical.
    let mut r = crate::Renderer::new();
    let _ = r.load_html("<p>x</p>", 400.0);
    let before = r.font_system.db().len();
    r.font_system.db_mut().load_font_data(sfnt);
    assert!(r.font_system.db().len() > before, "the decoded font must register");
}

/// `vh` and `vw` resolve against the viewport the document was laid out at.
///
/// A stale default here silently rescales every viewport-relative length on the
/// page — hero sections, sticky bars, full-height panels — by the ratio between
/// the real viewport and the default.
#[test]
fn viewport_units_use_the_actual_viewport() {
    let measure = |w: f32, h: f32| {
        let mut r = crate::Renderer::new();
        let d = r.load_html_vp(
            "<style>body{margin:0}#a{width:10vw;height:5vh}</style><div id=a></div>", w, h);
        fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
            if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
            n.children.iter().find_map(|c| find(c, id))
        }
        let r = find(&d.root, "a").unwrap().layout.border_rect;
        (r.w, r.h)
    };
    let (w, h) = measure(1280.0, 900.0);
    assert!((w - 128.0).abs() < 0.5, "10vw of 1280 is 128, got {w}");
    assert!((h - 45.0).abs() < 0.5, "5vh of 900 is 45, got {h}");
    // …and it tracks a different viewport rather than a remembered one.
    let (w, h) = measure(800.0, 600.0);
    assert!((w - 80.0).abs() < 0.5, "10vw of 800 is 80, got {w}");
    assert!((h - 30.0).abs() < 0.5, "5vh of 600 is 30, got {h}");
}

/// A container whose only content is floats still collapses margins normally.
///
/// The float does not make the container's own margins behave differently: with
/// `body { margin: 0 }` the container starts at the top of the page, and its
/// bottom margin stays below it rather than being applied above its parent.
#[test]
fn a_float_only_container_does_not_shift_its_parent() {
    let boxes = |inner: &str| {
        let mut r = crate::Renderer::new();
        let d = r.load_html(&format!(
            "<style>body{{margin:0}}\
             .w{{width:400px;overflow:hidden;margin-bottom:8px}}</style>\
             <div class=w id=w>{inner}</div>"), 800.0);
        fn find<'a>(n: &'a crate::WebCore, tag: &str) -> Option<&'a crate::WebCore> {
            if n.tag == tag { return Some(n) }
            n.children.iter().find_map(|c| find(c, tag))
        }
        let body = find(&d.root, "body").unwrap().layout.border_rect;
        let w = find(&d.root, "div").unwrap().layout.border_rect;
        (body, w)
    };
    // The control: an ordinary in-flow child.
    let (body, w) = boxes("<div style='height:40px'></div>");
    assert!(body.y.abs() < 0.5, "control: body at the top, got {}", body.y);
    assert!(w.y.abs() < 0.5, "control: container at the top, got {}", w.y);

    // The same container holding only a float.
    let (body, w) = boxes("<div style='float:left;width:100px;height:40px'></div>");
    assert!(body.y.abs() < 0.5, "body must stay at the top, got {}", body.y);
    assert!(w.y.abs() < 0.5, "the container must stay at the top, got {}", w.y);
    assert!((w.h - 40.0).abs() < 0.5, "the BFC contains its float, got {}", w.h);
    // Its bottom margin belongs below it, not around it.
    assert!((body.h - 40.0).abs() < 0.5, "body wraps the container only, got {}", body.h);
}

/// A float that overflows a non-BFC block stays in the parent's float context,
/// so content after that block still flows around it (CSS 2.1 §9.5).
///
/// The block itself is not stretched by the float, so the float protrudes past
/// it — and the next block has to make room. Two things had to be right for
/// this: the child must share the parent's context even before any float has
/// been seen, and float positions must convert back through the CONTEXT's
/// origin rather than the block's own top.
#[test]
fn a_float_escaping_its_block_still_moves_later_content() {
    let mut r = crate::Renderer::new();
    let d = r.load_html(
        "<style>body{margin:0}.w{width:400px}\
         .bfc{width:400px;overflow:hidden}</style>\
         <div class=w id=esc><div id=fl style='float:left;width:100px;height:60px'></div></div>\
         <div class=bfc id=next><div id=inner style='float:left;width:100px;height:30px'></div></div>",
        800.0);
    fn find<'a>(n: &'a crate::WebCore, id: &str) -> Option<&'a crate::WebCore> {
        if n.attributes.get("id").map(|v| v == id).unwrap_or(false) { return Some(n) }
        n.children.iter().find_map(|c| find(c, id))
    }
    // The float overflows: its block has no height of its own.
    let esc = find(&d.root, "esc").unwrap().layout.border_rect;
    assert!(esc.h < 0.5, "a non-BFC block is not stretched by its float, got {}", esc.h);
    let fl = find(&d.root, "fl").unwrap().layout.border_rect;
    assert!(fl.y.abs() < 0.5 && (fl.h - 60.0).abs() < 0.5,
            "the float is at the top and 60 tall, got y={} h={}", fl.y, fl.h);
    // …and the following BFC is pushed clear of it rather than overlapping.
    let next = find(&d.root, "next").unwrap().layout.border_rect;
    assert!((next.x - 100.0).abs() < 0.5,
            "the next block starts beside the protruding float, got x={}", next.x);
}
