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
