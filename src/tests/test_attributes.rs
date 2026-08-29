//! An element's attribute list is ORDERED, and the order is observable.
//!
//! Every expectation here was read off Chrome
//! (`--headless --dump-dom`, probe in `/tmp/webcore-html/attr.html`) before it
//! was written. The rule Chrome demonstrates is: **first set wins the
//! position, last set wins the value.**

use crate::html::parse_html;
use crate::html::serializer::escape_html;

fn doc(html: &str) -> crate::types::Document { parse_html(html) }

#[test]
fn attribute_names_come_back_in_source_order() {
    let d = doc(r#"<div id="d" zebra="1" alpha="2" mid="3">x</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    assert_eq!(
        d.get_attribute_names(el),
        vec!["id", "zebra", "alpha", "mid"],
        "Chrome: names: id,zebra,alpha,mid"
    );
}

#[test]
fn a_new_attribute_is_appended_and_a_reset_one_keeps_its_place() {
    let mut d = doc(r#"<div id="d" zebra="1" alpha="2" mid="3">x</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    d.set_attribute(el, "aaa", "9");
    assert_eq!(
        d.get_attribute_names(el),
        vec!["id", "zebra", "alpha", "mid", "aaa"],
        "Chrome: after set: id,zebra,alpha,mid,aaa — a NEW name goes last"
    );

    // The one place this differs from a CSS declaration list, where a
    // redeclaration moves to the end. An attribute's position is its identity.
    d.set_attribute(el, "zebra", "7");
    assert_eq!(
        d.get_attribute_names(el),
        vec!["id", "zebra", "alpha", "mid", "aaa"],
        "Chrome: after reset zebra: unchanged — a RESET name does not move"
    );
    assert_eq!(d.get_attribute(el, "zebra").as_deref(), Some("7"), "…but takes the new value");
}

#[test]
fn outer_html_writes_attributes_in_list_order() {
    let d = doc(r#"<div id="d" zebra="1" alpha="2" mid="3">x</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    assert_eq!(
        d.outer_html(el),
        r#"<div id="d" zebra="1" alpha="2" mid="3">x</div>"#,
        "Chrome writes the attribute list in order, not sorted"
    );
}

#[test]
fn an_empty_attribute_value_is_still_quoted() {
    // Chrome: `<input type="checkbox" checked="" disabled="">`. A bare name is
    // what the SOURCE may say and never what a serializer writes.
    let d = doc(r#"<p><input type=checkbox checked disabled></p>"#);
    let input = d.query_selector("input").unwrap();
    assert_eq!(
        d.outer_html(input),
        r#"<input type="checkbox" checked="" disabled="">"#
    );
}

#[test]
fn a_duplicate_attribute_keeps_the_first_occurrence() {
    // HTML §13.2.5.33 — the tokenizer DROPS a duplicate attribute name, so the
    // first one wins both the position and the value.
    let d = doc(r#"<div id="d" class="first" title="t" class="second">x</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("first"));
    assert_eq!(d.get_attribute_names(el), vec!["id", "class", "title"]);
}

#[test]
fn removing_an_attribute_closes_the_gap_without_reordering() {
    let mut d = doc(r#"<div id="d" a="1" b="2" c="3">x</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    d.remove_attribute(el, "b");
    assert_eq!(d.get_attribute_names(el), vec!["id", "a", "c"]);
}

#[test]
fn the_order_is_stable_across_runs() {
    // The bug this file exists for: a `HashMap` answered a different order on
    // every process, so `outerHTML` was nondeterministic. Ten fresh parses of
    // the same markup must serialize identically.
    let first = {
        let d = doc(r#"<div id="d" q="1" w="2" e="3" r="4" t="5" y="6">x</div>"#);
        d.outer_html(d.get_element_by_id("d").unwrap())
    };
    for _ in 0..10 {
        let d = doc(r#"<div id="d" q="1" w="2" e="3" r="4" t="5" y="6">x</div>"#);
        assert_eq!(d.outer_html(d.get_element_by_id("d").unwrap()), first);
    }
}

#[test]
fn a_no_break_space_is_escaped_in_text_and_in_an_attribute() {
    // HTML §13.3 "escaping a string" replaces U+00A0 in BOTH modes. Without it
    // a round trip turns an nbsp into an ordinary space in some contexts and
    // the two are not the same character to layout.
    assert_eq!(escape_html("a\u{00A0}b"), "a&nbsp;b");
    let mut d = doc(r#"<div id="d">x</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    d.set_attribute(el, "t", "a\u{00A0}b");
    assert!(d.outer_html(el).contains(r#"t="a&nbsp;b""#), "got {}", d.outer_html(el));
}
