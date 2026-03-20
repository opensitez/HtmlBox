// Ported from tests/test_selectors.cpp

use crate::css::{parse_selector, SelectorPart, Combinator, AttrOp, AncestorInfo};
use crate::types::*;

// ── Basic Selector Matching ───────────────────────────────────────────────────

#[test]
fn selectors_tag_match() {
    let mut b = HtmlBox::new("p");
    assert!(parse_selector("p").matches_box(&b));
    assert!(!parse_selector("div").matches_box(&b));
}

#[test]
fn selectors_class_match() {
    let mut b = HtmlBox::new("div");
    b.attributes.insert("class".into(), "foo bar".into());
    assert!(parse_selector(".foo").matches_box(&b));
    assert!(parse_selector(".bar").matches_box(&b));
    assert!(!parse_selector(".baz").matches_box(&b));
}

#[test]
fn selectors_id_match() {
    let mut b = HtmlBox::new("div");
    b.attributes.insert("id".into(), "main".into());
    assert!(parse_selector("#main").matches_box(&b));
    assert!(!parse_selector("#other").matches_box(&b));
}

#[test]
fn selectors_tag_and_class_combined() {
    let mut b = HtmlBox::new("p");
    b.attributes.insert("class".into(), "intro".into());
    assert!(parse_selector("p.intro").matches_box(&b));
    assert!(!parse_selector("div.intro").matches_box(&b));
}

#[test]
fn selectors_tag_and_id_combined() {
    let mut b = HtmlBox::new("div");
    b.attributes.insert("id".into(), "header".into());
    assert!(parse_selector("div#header").matches_box(&b));
    assert!(!parse_selector("p#header").matches_box(&b));
}

#[test]
fn selectors_universal_selector() {
    let b = HtmlBox::new("span");
    assert!(parse_selector("*").matches_box(&b));
}

#[test]
fn selectors_multiple_class_selector() {
    let mut b = HtmlBox::new("div");
    b.attributes.insert("class".into(), "foo bar baz".into());
    assert!(parse_selector(".foo.bar").matches_box(&b));
    assert!(parse_selector(".foo.baz").matches_box(&b));
    assert!(!parse_selector(".foo.missing").matches_box(&b));
}

// ── Attribute Selectors ───────────────────────────────────────────────────────

#[test]
fn selectors_attr_exists() {
    let sel = parse_selector("[href]");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Attribute { op: AttrOp::Exists, .. })));
}

#[test]
fn selectors_attr_equals() {
    let sel = parse_selector("[dir=\"rtl\"]");
    assert!(sel.parts.iter().any(|p| matches!(p,
        SelectorPart::Attribute { op: AttrOp::Eq, value, .. } if value == "rtl"
    )));
}

#[test]
fn selectors_attr_prefix() {
    let sel = parse_selector("[class^=\"btn\"]");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Attribute { op: AttrOp::StartsWith, .. })));
}

#[test]
fn selectors_attr_suffix() {
    let sel = parse_selector("[src$=\".png\"]");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Attribute { op: AttrOp::EndsWith, .. })));
}

#[test]
fn selectors_attr_substring() {
    let sel = parse_selector("[class*=\"mid\"]");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Attribute { op: AttrOp::Contains, .. })));
}

#[test]
fn selectors_attr_with_tag() {
    let sel = parse_selector("a[href]");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Tag(t) if t == "a")));
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Attribute { .. })));
}

// ── Structural Pseudo-Classes (parsing) ───────────────────────────────────────

#[test]
fn selectors_first_child_parsing() {
    let sel = parse_selector("p:first-child");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "first-child")));
}

#[test]
fn selectors_last_child_parsing() {
    let sel = parse_selector("p:last-child");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "last-child")));
}

#[test]
fn selectors_nth_child_parsing() {
    let sel = parse_selector("li:nth-child(2n+1)");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n.starts_with("nth-child"))));
}

#[test]
fn selectors_only_child_parsing() {
    let sel = parse_selector("p:only-child");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "only-child")));
}

#[test]
fn selectors_empty_parsing() {
    let sel = parse_selector("div:empty");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "empty")));
}

// ── Combinator Parsing ────────────────────────────────────────────────────────

#[test]
fn selectors_descendant_combinator() {
    let sel = parse_selector("div p");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::Descendant))));
}

#[test]
fn selectors_child_combinator() {
    let sel = parse_selector("div > p");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::Child))));
}

#[test]
fn selectors_adjacent_sibling() {
    let sel = parse_selector("h1 + p");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::AdjacentSibling))));
}

#[test]
fn selectors_general_sibling() {
    let sel = parse_selector("h1 ~ p");
    assert!(sel.parts.iter().any(|p| matches!(p, SelectorPart::Combinator(Combinator::GeneralSibling))));
}

// ── Child combinator with surrounding whitespace ──────────────────────────────

#[test]
fn selectors_child_combinator_no_spurious_descendant() {
    // "div > p" must parse to exactly [Tag("div"), Child, Tag("p")] — no extra Descendant.
    let sel = parse_selector("div > p");
    let combinators: Vec<_> = sel.parts.iter()
        .filter(|p| matches!(p, SelectorPart::Combinator(_)))
        .collect();
    assert_eq!(combinators.len(), 1, "expected exactly 1 combinator");
    assert!(matches!(combinators[0], SelectorPart::Combinator(Combinator::Child)));
}

#[test]
fn selectors_child_universal_no_spurious_descendant() {
    // ".grid-3 > *" must have exactly one combinator (Child), not Child + Descendant.
    let sel = parse_selector(".grid-3 > *");
    let combinators: Vec<_> = sel.parts.iter()
        .filter(|p| matches!(p, SelectorPart::Combinator(_)))
        .collect();
    assert_eq!(combinators.len(), 1, "expected exactly 1 combinator, got extra from trailing space after '>'");
    assert!(matches!(combinators[0], SelectorPart::Combinator(Combinator::Child)));
    // Should end with Universal
    assert!(matches!(sel.parts.last(), Some(SelectorPart::Universal)));
}

#[test]
fn selectors_child_universal_matches_direct_child() {
    // ".grid-3 > *" must match a direct child of .grid-3, not a grandchild.
    use crate::css::AncestorInfo;
    let sel = parse_selector(".grid-3 > *");
    let child = HtmlBox::new("div");

    // Direct child: parent is .grid-3
    let direct_ancestors = vec![AncestorInfo {
        tag: "div".into(),
        attributes: [("class".to_string(), "grid-3".to_string())].into(),
        child_index: 0,
        sibling_count: 3,
        ..Default::default()
    }];
    assert!(sel.matches_with_ancestors(&child, 0, 3, &direct_ancestors),
        "should match a direct child of .grid-3");

    // Grandchild: parent is a plain div inside .grid-3
    let grandchild_ancestors = vec![
        AncestorInfo {
            tag: "div".into(),
            attributes: [("class".to_string(), "grid-3".to_string())].into(),
            child_index: 0,
            sibling_count: 1,
            ..Default::default()
        },
        AncestorInfo {
            tag: "div".into(),
            attributes: Default::default(),
            child_index: 0,
            sibling_count: 1,
            ..Default::default()
        },
    ];
    assert!(!sel.matches_with_ancestors(&child, 0, 1, &grandchild_ancestors),
        "should NOT match a grandchild of .grid-3");
}

// ── nth-child keyword shorthands ──────────────────────────────────────────────

#[test]
fn selectors_nth_child_odd() {
    // :nth-child(odd) == 2n+1
    let sel = parse_selector("li:nth-child(odd)");
    // Parsed: child_index 0 (pos=1, odd) should match; child_index 1 (pos=2, even) should not
    let mut b_first = HtmlBox::new("li");
    let ancestors = vec![AncestorInfo {
        tag: "ul".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 4,
        ..Default::default()
    }];
    assert!(sel.matches_with_ancestors(&b_first, 0, 4, &ancestors));
    assert!(!sel.matches_with_ancestors(&b_first, 1, 4, &ancestors));
}

#[test]
fn selectors_nth_child_even() {
    // :nth-child(even) == 2n
    let sel = parse_selector("li:nth-child(even)");
    let ancestors = vec![AncestorInfo {
        tag: "ul".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 4,
        ..Default::default()
    }];
    let b = HtmlBox::new("li");
    assert!(!sel.matches_with_ancestors(&b, 0, 4, &ancestors)); // pos=1, odd
    assert!(sel.matches_with_ancestors(&b, 1, 4, &ancestors));  // pos=2, even
}

#[test]
fn selectors_nth_child_simple_number() {
    // :nth-child(3) matches only the 3rd child (child_index=2)
    let sel = parse_selector("li:nth-child(3)");
    let ancestors = vec![AncestorInfo {
        tag: "ul".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 4,
        ..Default::default()
    }];
    let b = HtmlBox::new("li");
    assert!(!sel.matches_with_ancestors(&b, 0, 4, &ancestors));
    assert!(!sel.matches_with_ancestors(&b, 1, 4, &ancestors));
    assert!(sel.matches_with_ancestors(&b, 2, 4, &ancestors));  // pos=3
    assert!(!sel.matches_with_ancestors(&b, 3, 4, &ancestors));
}

#[test]
fn selectors_first_child_match() {
    // li:first-child should match first child only
    let sel = parse_selector("li:first-child");
    let ancestors = vec![AncestorInfo {
        tag: "ul".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 2,
        ..Default::default()
    }];
    let b = HtmlBox::new("li");
    assert!(sel.matches_with_ancestors(&b, 0, 2, &ancestors));  // first child
    assert!(!sel.matches_with_ancestors(&b, 1, 2, &ancestors)); // second child
}

#[test]
fn selectors_last_child_match() {
    // li:last-child should match last child only
    let sel = parse_selector("li:last-child");
    let ancestors = vec![AncestorInfo {
        tag: "ul".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 2,
        ..Default::default()
    }];
    let b = HtmlBox::new("li");
    assert!(!sel.matches_with_ancestors(&b, 0, 2, &ancestors)); // first child
    assert!(sel.matches_with_ancestors(&b, 1, 2, &ancestors));  // last child
}

#[test]
fn selectors_only_child_match() {
    // p:only-child should match when sibling_count == 1
    let sel = parse_selector("p:only-child");
    let ancestors_single = vec![AncestorInfo {
        tag: "div".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 1,
        ..Default::default()
    }];
    let ancestors_two = vec![AncestorInfo {
        tag: "div".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 2,
        ..Default::default()
    }];
    let b = HtmlBox::new("p");
    assert!(sel.matches_with_ancestors(&b, 0, 1, &ancestors_single));  // only child
    assert!(!sel.matches_with_ancestors(&b, 0, 2, &ancestors_two));    // has sibling
}

#[test]
fn selectors_descendant_match() {
    // "div p" should match a p whose ancestor is a div
    let sel = parse_selector("div p");
    let b = HtmlBox::new("p");
    let ancestors = vec![AncestorInfo {
        tag: "div".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 1,
        ..Default::default()
    }];
    assert!(sel.matches_with_ancestors(&b, 0, 1, &ancestors));
    // "span p" should not match when ancestor is div
    let sel2 = parse_selector("span p");
    assert!(!sel2.matches_with_ancestors(&b, 0, 1, &ancestors));
}

#[test]
fn selectors_child_match() {
    // "div>p" (no spaces) should match p that is a direct child of div.
    // Note: the parser inserts an extra Descendant combinator for whitespace
    // around ">", so we use the no-space form to get a clean Child selector.
    let sel = parse_selector("div>p");
    let b = HtmlBox::new("p");
    let ancestors = vec![AncestorInfo {
        tag: "div".into(),
        attributes: Default::default(),
        child_index: 0,
        sibling_count: 1,
        ..Default::default()
    }];
    assert!(sel.matches_with_ancestors(&b, 0, 1, &ancestors));
}

#[test]
fn selectors_deep_descendant_match() {
    // "div p" should match p nested inside section inside div.
    // "div>p" (direct child) should NOT match (p's parent is section).
    // "section>p" should match (section is direct parent).
    let sel_desc      = parse_selector("div p");
    let sel_child     = parse_selector("div>p");
    let sel_sec_child = parse_selector("section>p");
    let b = HtmlBox::new("p");
    // ancestors listed outermost-first: div → section → p
    let ancestors = vec![
        AncestorInfo { tag: "div".into(),     attributes: Default::default(), child_index: 0, sibling_count: 1, ..Default::default() },
        AncestorInfo { tag: "section".into(), attributes: Default::default(), child_index: 0, sibling_count: 1, ..Default::default() },
    ];
    // "div p" — descendant match (div is ancestor)
    assert!(sel_desc.matches_with_ancestors(&b, 0, 1, &ancestors));
    // "div>p" — direct child: p's parent is section, not div → should NOT match
    assert!(!sel_child.matches_with_ancestors(&b, 0, 1, &ancestors));
    // "section>p" — direct child of section → should match
    assert!(sel_sec_child.matches_with_ancestors(&b, 0, 1, &ancestors));
}

// ── Specificity ───────────────────────────────────────────────────────────────

#[test]
fn selectors_specificity() {
    let sel1 = parse_selector("#main");
    let sel2 = parse_selector(".container");
    let sel3 = parse_selector("div");
    assert!(sel1.specificity() > sel2.specificity());
    assert!(sel2.specificity() > sel3.specificity());
}

// ── :nth-child ignores #text siblings ────────────────────────────────────────

/// Regression: the HTML parser creates #text nodes for whitespace between
/// elements.  If these are counted for :nth-child, the indices are wrong and
/// selectors like .dot:nth-child(1/2/3) match the wrong (or no) elements.
/// The cascade must count only element siblings, not text nodes.
#[test]
fn nth_child_ignores_text_node_siblings() {
    use super::harness::parse_and_layout;

    // Three divs separated by whitespace — the parser creates #text nodes
    // between them.  The :nth-child selectors must still use element-only index.
    let doc = parse_and_layout(r#"
        <style>
        .box:nth-child(1) { background: red; }
        .box:nth-child(2) { background: green; }
        .box:nth-child(3) { background: blue; }
        </style>
        <div id="wrap">
          <div class="box" id="b1"></div>
          <div class="box" id="b2"></div>
          <div class="box" id="b3"></div>
        </div>
    "#, 400.0);

    let b1 = super::harness::find_box(&doc.root, &|b| b.attributes.get("id").map(|s| s == "b1").unwrap_or(false))
        .expect("b1 not found");
    let b2 = super::harness::find_box(&doc.root, &|b| b.attributes.get("id").map(|s| s == "b2").unwrap_or(false))
        .expect("b2 not found");
    let b3 = super::harness::find_box(&doc.root, &|b| b.attributes.get("id").map(|s| s == "b3").unwrap_or(false))
        .expect("b3 not found");

    // Red = (255,0,0), Green = (0,128,0), Blue = (0,0,255)
    let red   = crate::css::parse_color("red").unwrap();
    let green = crate::css::parse_color("green").unwrap();
    let blue  = crate::css::parse_color("blue").unwrap();

    assert_eq!(b1.style.background_color.r, red.r,
        "b1 (nth-child 1) should have red background, got {:?}", b1.style.background_color);
    assert_eq!(b2.style.background_color.g, green.g,
        "b2 (nth-child 2) should have green background, got {:?}", b2.style.background_color);
    assert_eq!(b3.style.background_color.b, blue.b,
        "b3 (nth-child 3) should have blue background, got {:?}", b3.style.background_color);
}

/// Regression: loading dots animation — each .dot:nth-child(n) rule sets a
/// different animation-delay.  With text-node counting off, all three nth-child
/// rules miss all three dots (they fall on whitespace positions) so only the
/// element that accidentally matches gets an animation.
#[test]
fn nth_child_animation_delay_per_dot() {
    use super::harness::parse_and_layout;

    let doc = parse_and_layout(r#"
        <style>
        @keyframes pulse { from { opacity: 0; } to { opacity: 1; } }
        .dot:nth-child(1) { animation: pulse 1s linear 0s    infinite; }
        .dot:nth-child(2) { animation: pulse 1s linear 0.2s  infinite; }
        .dot:nth-child(3) { animation: pulse 1s linear 0.4s  infinite; }
        </style>
        <div class="dots">
          <div class="dot" id="d1"></div>
          <div class="dot" id="d2"></div>
          <div class="dot" id="d3"></div>
        </div>
    "#, 400.0);

    let d1 = super::harness::find_box(&doc.root, &|b| b.attributes.get("id").map(|s| s == "d1").unwrap_or(false))
        .expect("d1 not found");
    let d2 = super::harness::find_box(&doc.root, &|b| b.attributes.get("id").map(|s| s == "d2").unwrap_or(false))
        .expect("d2 not found");
    let d3 = super::harness::find_box(&doc.root, &|b| b.attributes.get("id").map(|s| s == "d3").unwrap_or(false))
        .expect("d3 not found");

    assert_eq!(d1.style.animations.len(), 1, "d1 should have 1 animation");
    assert_eq!(d2.style.animations.len(), 1, "d2 should have 1 animation");
    assert_eq!(d3.style.animations.len(), 1, "d3 should have 1 animation");

    assert!((d1.style.animations[0].delay_ms -   0.0).abs() < 1.0,
        "d1 delay should be 0ms, got {}", d1.style.animations[0].delay_ms);
    assert!((d2.style.animations[0].delay_ms - 200.0).abs() < 1.0,
        "d2 delay should be 200ms, got {}", d2.style.animations[0].delay_ms);
    assert!((d3.style.animations[0].delay_ms - 400.0).abs() < 1.0,
        "d3 delay should be 400ms, got {}", d3.style.animations[0].delay_ms);
}
