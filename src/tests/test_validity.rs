//! Constraint validation — HTML §4.10.19.
//!
//! The flags and the `willValidate` answers here were read off Chrome first
//! (`/tmp/webcore-html/cv.html`). The `validationMessage` WORDING is
//! implementation-defined, so these assert the flag and that a message exists,
//! never Chrome's exact sentence — except for a custom message, which is the
//! one the spec pins down exactly.

use crate::html::parse_html;

const FORM: &str = r#"<form id=f>
<input id=req required value="">
<input id=req2 required value="x">
<input id=em type=email value="notanemail">
<input id=pat pattern="[0-9]+" value="abc">
<input id=ml maxlength=3 value="abcdef">
<input id=num type=number min=5 max=10 value="3">
<input id=num2 type=number min=5 max=10 value="30">
<input id=st type=number min=0 step=3 value="4">
<input id=hid type=hidden required value="">
<input id=dis required disabled value="">
<input id=ro required readonly value="">
<button id=btn type=button></button>
<button id=sub></button>
<output id=out></output>
<fieldset id=fs></fieldset>
<select id=sel required><option value="">--</option></select>
<textarea id=ta required></textarea>
</form>"#;

fn form() -> crate::types::Document {
    parse_html(FORM)
}

fn el(d: &crate::types::Document, id: &str) -> u32 {
    d.get_element_by_id(id).unwrap()
}

#[test]
fn a_barred_control_is_valid_no_matter_what_its_attributes_say() {
    let d = form();
    // Chrome: hid / dis / ro / btn / out / fs all willValidate=false, valid=true.
    for id in ["hid", "dis", "ro", "btn", "out", "fs"] {
        let e = el(&d, id);
        assert!(
            !d.will_validate(e),
            "{id} should be barred from constraint validation"
        );
        assert!(d.validity(e).valid(), "{id} is valid because it is barred");
        assert_eq!(d.validation_message(e), "", "{id} has no message");
    }
    // …and the ones that ARE candidates.
    for id in ["req", "req2", "em", "sub", "sel", "ta"] {
        assert!(d.will_validate(el(&d, id)), "{id} should be a candidate");
    }
}

#[test]
fn required_is_the_only_constraint_an_empty_value_can_violate() {
    let d = form();
    let req = el(&d, "req");
    assert!(
        d.validity(req).value_missing,
        "Chrome: req flags=[valueMissing]"
    );
    assert!(!d.validity(req).valid());
    assert!(!d.validation_message(req).is_empty());

    let req2 = el(&d, "req2");
    assert!(d.validity(req2).valid(), "Chrome: req2 valid=true");
    assert_eq!(d.validation_message(req2), "");

    // A `<select required>` whose selection has an empty value, and an empty
    // `<textarea required>`, are both valueMissing.
    assert!(
        d.validity(el(&d, "sel")).value_missing,
        "Chrome: sel flags=[valueMissing]"
    );
    assert!(
        d.validity(el(&d, "ta")).value_missing,
        "Chrome: ta flags=[valueMissing]"
    );
}

#[test]
fn type_and_pattern_mismatches_are_separate_flags() {
    let d = form();
    let em = d.validity(el(&d, "em"));
    assert!(em.type_mismatch, "Chrome: em flags=[typeMismatch]");
    assert!(!em.pattern_mismatch, "and ONLY typeMismatch");

    let pat = d.validity(el(&d, "pat"));
    assert!(pat.pattern_mismatch, "Chrome: pat flags=[patternMismatch]");
    assert!(!pat.type_mismatch);
}

#[test]
fn a_length_constraint_ignores_a_value_the_user_never_edited() {
    // ⛔ The one that would have been wrong from the spec text alone. Chrome on
    // `<input maxlength=3 value="abcdef">` answers valid=true: the constraint
    // is on the dirty value flag.
    let mut d = form();
    let ml = el(&d, "ml");
    assert!(
        d.validity(ml).valid(),
        "Chrome: ml valid=true — the author's default is exempt"
    );

    // Once the user (or the IDL setter, which raises the same flag) writes it,
    // the constraint applies.
    d.set_value(ml, "abcdef");
    assert!(
        d.validity(ml).too_long,
        "a dirty value over maxlength is tooLong"
    );
    assert!(!d.validity(ml).valid());
}

#[test]
fn range_and_step_constraints_report_which_bound_was_crossed() {
    let d = form();
    let under = d.validity(el(&d, "num"));
    assert!(under.range_underflow, "Chrome: num flags=[rangeUnderflow]");
    assert!(!under.range_overflow);
    assert!(
        d.validation_message(el(&d, "num")).contains('5'),
        "the message names the bound"
    );

    let over = d.validity(el(&d, "num2"));
    assert!(over.range_overflow, "Chrome: num2 flags=[rangeOverflow]");
    assert!(!over.range_underflow);

    assert!(
        d.validity(el(&d, "st")).step_mismatch,
        "Chrome: st flags=[stepMismatch] — 4 is not 0+3n"
    );
}

#[test]
fn a_custom_message_wins_and_an_empty_one_clears_it() {
    let mut d = form();
    let req2 = el(&d, "req2");
    assert!(d.validity(req2).valid());

    d.set_custom_validity(req2, "nope");
    assert!(d.validity(req2).custom_error, "Chrome: customError=true");
    assert!(!d.validity(req2).valid(), "Chrome: valid=false");
    assert_eq!(
        d.validation_message(req2),
        "nope",
        "Chrome: msg=\"nope\" — verbatim"
    );
    assert!(!d.check_validity(req2), "Chrome: check=false");

    d.set_custom_validity(req2, "");
    assert!(d.validity(req2).valid(), "Chrome: cleared: valid=true");
    assert_eq!(d.validation_message(req2), "");
}

#[test]
fn a_form_checks_every_control_it_owns() {
    let mut d = form();
    assert!(
        !d.check_validity(el(&d, "f")),
        "Chrome: form.checkValidity=false"
    );

    let mut clean =
        parse_html(r#"<form id=f><input id=a value="x"><input id=b required value="y"></form>"#);
    assert!(clean.check_validity(clean.get_element_by_id("f").unwrap()));
}

#[test]
fn check_validity_fires_invalid_at_each_failing_control() {
    use std::sync::{Arc, Mutex};
    let mut d = form();
    let seen: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

    for id in ["req", "em", "pat"] {
        let e = el(&d, id);
        let log = seen.clone();
        d.add_event_listener(
            e,
            "invalid",
            Box::new(move |ev, _| {
                log.lock().unwrap().push(ev.current_target);
            }),
            crate::dom::events::ListenerOptions::capture(false),
        );
    }
    let (req, em, pat) = (el(&d, "req"), el(&d, "em"), el(&d, "pat"));
    let form_el = el(&d, "f");
    d.check_validity(form_el);

    let fired = seen.lock().unwrap().clone();
    assert!(
        fired.contains(&req) && fired.contains(&em) && fired.contains(&pat),
        "every failing control gets an `invalid` event, not just the first: {fired:?}"
    );

    // A control that passes does not fire.
    let req2 = el(&d, "req2");
    assert!(!fired.contains(&req2));
}

#[test]
fn form_owner_prefers_the_attribute_over_the_ancestor() {
    let d = parse_html(
        r#"
        <form id="outer"><input id="inside"></form>
        <form id="other"></form>
        <input id="outside" form="other">
        <form id="wrap"><input id="redirected" form="other"></form>
    "#,
    );
    assert_eq!(d.form_owner(el(&d, "inside")), Some(el(&d, "outer")));
    assert_eq!(
        d.form_owner(el(&d, "outside")),
        Some(el(&d, "other")),
        "a control outside every form still has one when `form` names it"
    );
    assert_eq!(
        d.form_owner(el(&d, "redirected")),
        Some(el(&d, "other")),
        "the attribute BEATS the ancestor — that is what it is for"
    );
}

#[test]
fn form_elements_lists_the_listed_ones_in_tree_order() {
    let d = parse_html(
        r#"<form id="f">
        <input id="a"><p>text</p><select id="b"></select><textarea id="c"></textarea>
        <button id="d"></button><fieldset id="e"></fieldset><output id="g"></output>
        <input id="img" type="image"><div id="nope"></div>
    </form>"#,
    );
    let names: Vec<String> = d
        .form_elements(el(&d, "f"))
        .into_iter()
        .map(|n| d.get_attribute(n, "id").unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        vec!["a", "b", "c", "d", "e", "g"],
        "`<input type=image>` is excluded and a `<div>` was never listed"
    );
}

#[test]
fn labels_finds_both_the_for_attribute_and_the_wrapping_label() {
    let d = parse_html(
        r#"
        <label id="l1" for="x">by for</label>
        <label id="l2"><input id="x"></label>
        <label id="l3" for="other">not this one</label>
    "#,
    );
    let labels = d.labels(el(&d, "x"));
    assert_eq!(
        labels,
        vec![el(&d, "l1"), el(&d, "l2")],
        "tree order, both kinds"
    );
    assert!(
        d.labels(el(&d, "l3")).is_empty(),
        "a label is not labelable"
    );
}

#[test]
fn a_disabled_fieldset_disables_its_controls_but_not_its_first_legend() {
    let d = parse_html(
        r#"<form><fieldset disabled>
        <legend><input id="in_legend" required value=""></legend>
        <input id="shielded" required value="">
    </fieldset></form>"#,
    );
    assert!(
        !d.will_validate(el(&d, "shielded")),
        "inside a disabled fieldset"
    );
    assert!(
        d.will_validate(el(&d, "in_legend")),
        "the FIRST legend of a disabled fieldset is the escape hatch HTML gives authors"
    );
}

#[test]
fn a_radio_group_is_satisfied_by_any_checked_member() {
    let d = parse_html(
        r#"<form>
        <input id="r1" type="radio" name="g" required>
        <input id="r2" type="radio" name="g" checked>
    </form>"#,
    );
    assert!(
        d.validity(el(&d, "r1")).valid(),
        "`required` on one radio is a constraint on the GROUP, not on that button"
    );

    let d2 = parse_html(
        r#"<form>
        <input id="r1" type="radio" name="g" required>
        <input id="r2" type="radio" name="g">
    </form>"#,
    );
    assert!(
        d2.validity(el(&d2, "r1")).value_missing,
        "nothing checked in the group"
    );
}
