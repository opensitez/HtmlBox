//! `DOMTokenList` — DOM §7.1.
//!
//! Read off Chrome first (`/tmp/webcore-html/dtl.html`). The two answers worth
//! staring at: `value` is the raw attribute while `length` counts the deduped
//! set, and `classList.supports()` is a `TypeError` rather than `false`.

use crate::dom::token_list::TokenError;
use crate::html::parse_html;

fn doc(html: &str) -> crate::types::Document {
    parse_html(html)
}

#[test]
fn value_is_verbatim_while_length_counts_the_parsed_set() {
    let d = doc(r#"<div id="d" class="a a b  c">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    let cl = d.class_list(el);

    assert_eq!(
        cl.value(),
        "a a b  c",
        "Chrome: value=\"a a b  c\" — the attribute, untouched"
    );
    assert_eq!(cl.length(), 3, "Chrome: length=3 — a, b, c");
    assert_eq!(cl.item(0).as_deref(), Some("a"));
    assert_eq!(
        cl.item(1).as_deref(),
        Some("b"),
        "the DUPLICATE a does not take slot 1"
    );
    assert_eq!(cl.item(9), None, "Chrome: item9=null");
    assert!(cl.contains("a"));
    assert!(!cl.contains("z"));
}

#[test]
fn add_and_remove_take_several_tokens_and_rewrite_the_serialized_set() {
    let mut d = doc(r#"<div id="d" class="a a b  c">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    d.class_list_mut(el).add(&["x", "y"]).unwrap();
    assert_eq!(
        d.get_attribute(el, "class").as_deref(),
        Some("a b c x y"),
        "Chrome: the mutation normalises — dedup, single spaces"
    );

    d.class_list_mut(el).remove(&["a", "c"]).unwrap();
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("b x y"));
}

#[test]
fn toggle_reports_presence_after_and_force_pins_the_direction() {
    let mut d = doc(r#"<div id="d" class="b x y">z</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    assert_eq!(
        d.class_list_mut(el).toggle("b", None),
        Ok(false),
        "Chrome: toggle b -> false"
    );
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("x y"));

    assert_eq!(d.class_list_mut(el).toggle("b", Some(true)), Ok(true));
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("x y b"));

    assert_eq!(d.class_list_mut(el).toggle("b", Some(false)), Ok(false));
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("x y"));

    // force=true on a token already present is still true, and writes nothing new.
    assert_eq!(d.class_list_mut(el).toggle("x", Some(true)), Ok(true));
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("x y"));
}

#[test]
fn replace_swaps_in_place_and_answers_false_for_a_token_that_is_not_there() {
    let mut d = doc(r#"<div id="d" class="x y">z</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    assert_eq!(d.class_list_mut(el).replace("x", "q"), Ok(true));
    assert_eq!(
        d.get_attribute(el, "class").as_deref(),
        Some("q y"),
        "Chrome: replace x->q true class=\"q y\" — position kept"
    );

    assert_eq!(d.class_list_mut(el).replace("nope", "q"), Ok(false));
    assert_eq!(
        d.get_attribute(el, "class").as_deref(),
        Some("q y"),
        "and nothing was written"
    );

    // Replacing with a token already in the list drops the old one rather than
    // duplicating the new one.
    assert_eq!(d.class_list_mut(el).replace("q", "y"), Ok(true));
    assert_eq!(d.get_attribute(el, "class").as_deref(), Some("y"));
}

#[test]
fn a_bad_token_is_rejected_before_anything_is_written() {
    let mut d = doc(r#"<div id="d" class="a">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    assert_eq!(
        d.class_list_mut(el).add(&[""]),
        Err(TokenError::Syntax),
        "Chrome: add('') throws SyntaxError"
    );
    assert_eq!(
        d.class_list_mut(el).add(&["a b"]),
        Err(TokenError::InvalidCharacter),
        "Chrome: add('a b') throws InvalidCharacterError"
    );

    // The good token in front of the bad one must not have landed.
    assert_eq!(
        d.class_list_mut(el).add(&["good", "bad token"]),
        Err(TokenError::InvalidCharacter)
    );
    assert_eq!(
        d.get_attribute(el, "class").as_deref(),
        Some("a"),
        "validation happens before the write, so the call is all-or-nothing"
    );
}

#[test]
fn supports_answers_for_rel_and_is_a_type_error_for_class() {
    let d = doc(r#"<a id="lnk" rel="noopener noreferrer">l</a>"#);
    let link = d.get_element_by_id("lnk").unwrap();

    let rl = d.rel_list(link);
    assert_eq!(rl.length(), 2, "Chrome: relList length=2");
    assert_eq!(rl.value(), "noopener noreferrer");
    assert_eq!(rl.supports("noopener"), Some(true));
    assert_eq!(rl.supports("zzz"), Some(false));

    assert_eq!(
        d.class_list(link).supports("a"),
        None,
        "Chrome: classList.supports throws TypeError — no supported-tokens definition"
    );
}

#[test]
fn setting_value_writes_the_attribute_without_normalising() {
    let mut d = doc(r#"<div id="d" class="a">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    d.class_list_mut(el).set_value("  p   q ");
    assert_eq!(
        d.get_attribute(el, "class").as_deref(),
        Some("  p   q "),
        "Chrome keeps the string exactly"
    );
    assert_eq!(d.class_list(el).length(), 2);
}

#[test]
fn an_element_with_no_such_attribute_does_not_grow_one() {
    let mut d = doc(r#"<div id="d">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    assert_eq!(d.class_list(el).length(), 0);
    assert_eq!(
        d.class_list(el).value(),
        "",
        "Chrome: value=\"\" with no attribute"
    );
    assert_eq!(d.get_attribute(el, "class"), None);

    d.class_list_mut(el).remove(&["nothing"]).unwrap();
    assert_eq!(
        d.get_attribute(el, "class"),
        None,
        "Chrome: remove on an absent attribute leaves it absent, not empty"
    );
}

#[test]
fn sandbox_and_part_are_the_same_type_over_different_attributes() {
    let mut d = doc(r#"<iframe id="f" sandbox="allow-scripts"></iframe>"#);
    let f = d.get_element_by_id("f").unwrap();

    assert!(d.sandbox(f).contains("allow-scripts"));
    assert_eq!(d.sandbox(f).supports("allow-forms"), Some(true));
    assert_eq!(d.sandbox(f).supports("allow-everything"), Some(false));

    d.part_mut(f).add(&["header", "footer"]).unwrap();
    assert_eq!(d.get_attribute(f, "part").as_deref(), Some("header footer"));
}
