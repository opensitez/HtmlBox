//! Text selection on form controls — HTML §4.10.19.3.
//!
//! Every expectation here was read off Chrome first
//! (`/tmp/webcore-html/sel.html`, `sel2.html`, `sel3.html`) rather than off the
//! prose, because three of them are not what the prose suggests:
//!
//! * `select()` is NOT gated on the API applying — `checkbox.select()` and
//!   `number.select()` both succeed, while `checkbox.setSelectionRange(…)`
//!   throws.
//! * `selectionStart = n` past the end drags the END along; the same offsets
//!   through `setSelectionRange` collapse onto the end instead.
//! * `setSelectionRange(a, b)` with no direction RESETS the direction, and
//!   `selectionStart = n` leaves it alone.
//!
//! `None`/`false` is the thrown exception, as everywhere else in `api.rs`.

use crate::html::parse_html;
use crate::types::Document;

const FORM: &str = r#"<form>
<input id=t type=text value="Hello World">
<input id=pw type=password value="secret">
<input id=se type=search value="query">
<input id=url type=url value="http://a">
<input id=tel type=tel value="123">
<input id=plain value="abc">
<input id=weird type=weirdtype value="abc">
<input id=em type=email value="a@b.c">
<input id=num type=number value="42">
<input id=cb type=checkbox>
<input id=dt type=date>
<input id=file type=file>
<input id=hid type=hidden value="x">
<textarea id=ta>line1
line2</textarea>
<div id=div>not a control</div>
</form>"#;

fn form() -> Document { parse_html(FORM) }
fn el(d: &Document, id: &str) -> u32 { d.get_element_by_id(id).unwrap() }

/// Fresh single input, so a test that rewrites the value cannot disturb another.
fn one(markup: &str) -> (Document, u32) {
    let d = parse_html(&format!("<form>{markup}</form>"));
    let id = d.get_element_by_id("x").unwrap();
    (d, id)
}

// ─── which controls the API applies to ──────────────────────────────────────

#[test]
fn the_api_applies_to_the_five_text_states_and_textarea() {
    let d = form();
    // Chrome answers 0 (not null) for every one of these.
    for id in ["t", "pw", "se", "url", "tel", "plain", "weird", "ta"] {
        let e = el(&d, id);
        assert_eq!(d.selection_start(e), Some(0), "{id}.selectionStart");
        assert_eq!(d.selection_direction(e), Some("none"), "{id}.selectionDirection");
    }
}

#[test]
fn a_control_that_holds_a_value_still_need_not_support_selection() {
    let d = form();
    // ⛔ The predicate is NOT "has a value" and NOT "accepts typing". `number`,
    // `date` and `email` are all `ValueMode::Value`, all three take keystrokes,
    // and Chrome answers `null` for all three.
    for id in ["em", "num", "cb", "dt", "file", "hid", "div"] {
        let e = el(&d, id);
        assert_eq!(d.selection_start(e), None, "{id}.selectionStart");
        assert_eq!(d.selection_end(e), None, "{id}.selectionEnd");
        assert_eq!(d.selection_direction(e), None, "{id}.selectionDirection");
    }
}

#[test]
fn the_setters_throw_where_the_getters_answer_null() {
    let mut d = form();
    for id in ["em", "num", "cb", "dt", "file", "hid", "div"] {
        let e = el(&d, id);
        assert!(!d.set_selection_range(e, 0, 1, None), "{id}.setSelectionRange");
        assert!(!d.set_selection_start(e, 1), "{id}.selectionStart=");
        assert!(!d.set_selection_end(e, 1), "{id}.selectionEnd=");
        assert!(!d.set_selection_direction(e, "forward"), "{id}.selectionDirection=");
        assert!(!d.set_range_text(e, "Q", Some((0, 1)), "preserve"), "{id}.setRangeText");
    }
}

#[test]
fn select_is_not_gated_the_way_the_rest_of_the_api_is() {
    let mut d = form();
    // Chrome: `checkbox.select()` → ok, `checkbox.setSelectionRange(0,1)` →
    // InvalidStateError. The spec step is "return", not "throw", so this must
    // not be routed through the same guard as its neighbours.
    for id in ["em", "num", "cb", "file", "hid", "div"] {
        let e = el(&d, id);
        d.select(e); // must not panic and must leave the control alone
        assert_eq!(d.selection_start(e), None, "{id} gained a selection");
    }
}

#[test]
fn select_on_a_control_without_selectable_text_fires_nothing() {
    // Spec-derived rather than measured: the `select` event is QUEUED, so a
    // synchronous Chrome probe reads 0 either way and cannot tell these apart.
    // `select()` returns BEFORE the "set the selection range" step for a
    // control the API does not apply to, and that step is what fires the event.
    use std::sync::{Arc, Mutex};
    let mut d = form();
    let cb = el(&d, "cb");
    let seen = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&seen);
    d.add_event_listener(
        cb,
        "select",
        Box::new(move |_, _| { *counter.lock().unwrap() += 1; }),
        Default::default(),
    );
    d.select(cb);
    assert_eq!(*seen.lock().unwrap(), 0);
}

// ─── select() and setSelectionRange ─────────────────────────────────────────

#[test]
fn select_covers_the_whole_value_and_clears_the_direction() {
    let mut d = form();
    let t = el(&d, "t");
    assert!(d.set_selection_range(t, 2, 5, Some("backward")));
    assert_eq!(d.selection_direction(t), Some("backward"));
    d.select(t);
    assert_eq!(
        (d.selection_start(t), d.selection_end(t), d.selection_direction(t)),
        (Some(0), Some(11), Some("none")),
        "Chrome: [0,11,\"none\"] — select() resets the direction it found"
    );
}

#[test]
fn a_backwards_range_collapses_onto_its_end_not_its_start() {
    let mut d = form();
    let t = el(&d, "t");
    // Chrome: `setSelectionRange(3,1)` → [1,1]. The spec sets START to end.
    assert!(d.set_selection_range(t, 3, 1, None));
    assert_eq!((d.selection_start(t), d.selection_end(t)), (Some(1), Some(1)));
}

#[test]
fn offsets_past_the_value_clamp_to_its_length() {
    let mut d = form();
    let t = el(&d, "t");
    assert!(d.set_selection_range(t, 99, 99, None));
    assert_eq!((d.selection_start(t), d.selection_end(t)), (Some(11), Some(11)));
}

#[test]
fn an_unrecognised_direction_is_the_platform_default_not_an_error() {
    let mut d = form();
    let t = el(&d, "t");
    // The IDL type is a plain DOMString, so this stores "none" rather than
    // throwing (measured).
    assert!(d.set_selection_range(t, 2, 5, Some("bogus")));
    assert_eq!(d.selection_direction(t), Some("none"));
    assert!(d.set_selection_direction(t, "sideways"));
    assert_eq!(d.selection_direction(t), Some("none"));
    assert!(d.set_selection_range(t, 2, 5, Some("forward")));
    assert_eq!(d.selection_direction(t), Some("forward"));
}

#[test]
fn a_two_argument_range_resets_the_direction_and_the_start_setter_keeps_it() {
    let mut d = form();
    let t = el(&d, "t");
    // ⛔ The one place these two paths must NOT share an implementation.
    assert!(d.set_selection_range(t, 2, 5, Some("backward")));
    assert!(d.set_selection_range(t, 1, 3, None));
    assert_eq!(d.selection_direction(t), Some("none"), "the 2-arg call clears it");

    assert!(d.set_selection_range(t, 2, 5, Some("backward")));
    assert!(d.set_selection_start(t, 1));
    assert_eq!(
        (d.selection_start(t), d.selection_end(t), d.selection_direction(t)),
        (Some(1), Some(5), Some("backward")),
        "Chrome: [1,5,\"backward\"] — the start setter leaves the direction alone"
    );
}

#[test]
fn a_start_past_the_end_drags_the_end_and_an_end_before_the_start_drags_the_start() {
    let mut d = form();
    let t = el(&d, "t");
    assert!(d.set_selection_range(t, 1, 5, None));
    assert!(d.set_selection_start(t, 8));
    assert_eq!((d.selection_start(t), d.selection_end(t)), (Some(8), Some(8)));

    assert!(d.set_selection_range(t, 3, 5, None));
    assert!(d.set_selection_end(t, 0));
    assert_eq!((d.selection_start(t), d.selection_end(t)), (Some(0), Some(0)));
}

#[test]
fn a_backward_selection_puts_the_cursor_at_its_start() {
    // White-box, deliberately: the cursor/anchor ORDERING is what a caret
    // paints at and what a future shift+arrow would extend from, and the
    // selection API reads the pair as min/max, so nothing else here can see
    // it. `process_form_input_key` takes `_shift` and ignores it today — this
    // pins the invariant that implementing it will depend on.
    let mut d = form();
    let t = el(&d, "t");
    assert!(d.set_selection_range(t, 2, 5, Some("backward")));
    let n = d.find_webcore(t).unwrap();
    assert_eq!((n.input_cursor, n.input_sel_anchor), (2, 5), "backward: cursor leads at the start");

    assert!(d.set_selection_range(t, 2, 5, Some("forward")));
    let n = d.find_webcore(t).unwrap();
    assert_eq!((n.input_cursor, n.input_sel_anchor), (5, 2), "forward: cursor trails at the end");
}

// ─── setRangeText ───────────────────────────────────────────────────────────

#[test]
fn set_range_text_without_a_range_uses_the_current_selection() {
    let (mut d, x) = one(r#"<input id=x type=text value="Hello">"#);
    assert!(d.set_selection_range(x, 1, 3, None));
    assert!(d.set_range_text(x, "XY", None, "preserve"));
    assert_eq!(d.value(x), "HXYlo");
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(1), Some(3)));
}

#[test]
fn the_three_explicit_select_modes_land_where_chrome_puts_them() {
    for (mode, want) in [("select", (1, 3)), ("start", (1, 1)), ("end", (3, 3))] {
        let (mut d, x) = one(r#"<input id=x type=text value="Hello">"#);
        assert!(d.set_range_text(x, "ZZ", Some((1, 3)), mode));
        assert_eq!(d.value(x), "HZZlo", "{mode}");
        assert_eq!(
            (d.selection_start(x), d.selection_end(x)),
            (Some(want.0), Some(want.1)),
            "{mode}"
        );
    }
}

#[test]
fn preserve_moves_each_end_by_where_it_sits_relative_to_the_replaced_range() {
    // All seven shapes, straight off Chrome. This is the only arithmetic in the
    // feature, and an offset that sits INSIDE the replaced range is where an
    // off-by-one hides — the two "inside" rows below are the ones a naive
    // "shift everything after by delta" gets wrong.
    let cases: &[(&str, (u32, u32), &str, (u32, u32), &str, (u32, u32))] = &[
        // value, old selection, replacement, replaced range, new value, new selection
        ("Hello", (2, 4), "XYZ", (1, 3), "HXYZlo", (1, 5)), // start inside
        ("Hello", (0, 2), "XYZ", (1, 3), "HXYZlo", (0, 4)), // end inside
        ("Hello", (2, 3), "X", (1, 4), "HXo", (1, 2)),      // fully inside
        ("Hello", (4, 5), "XYZ", (0, 2), "XYZllo", (5, 6)), // wholly after
        ("Hello", (0, 1), "XYZ", (2, 4), "HeXYZo", (0, 1)), // wholly before
        ("Hello", (4, 5), "Z", (0, 3), "Zlo", (2, 3)),      // after, shrinking
        ("Hello", (3, 5), "", (1, 2), "Hllo", (2, 4)),      // after, deleting
        // ⛔ The equality boundaries. Without these the comparisons can all be
        // `>=` instead of `>` and every row above still passes — a mutation
        // run found exactly that hole, because none of the seven shapes puts
        // an offset ON an edge of the replaced range.
        ("Hello", (3, 5), "XYZ", (1, 3), "HXYZlo", (1, 6)), // start == range end
        ("Hello", (0, 3), "XYZ", (1, 3), "HXYZlo", (0, 4)), // end == range end
        ("Hello", (1, 4), "XYZ", (1, 3), "HXYZlo", (1, 5)), // start == range start
        ("Hello", (0, 1), "XYZ", (1, 3), "HXYZlo", (0, 1)), // end == range start
        ("Hello", (1, 3), "XYZ", (1, 3), "HXYZlo", (1, 4)), // selection IS the range
        ("Hello", (1, 1), "XYZ", (1, 3), "HXYZlo", (1, 1)), // collapsed on its start
        ("Hello", (3, 3), "XYZ", (1, 3), "HXYZlo", (1, 4)), // collapsed on its end
        ("Hello", (2, 4), "XYZ", (2, 2), "HeXYZllo", (2, 7)), // pure insertion
    ];
    for (value, sel, replacement, range, want_value, want_sel) in cases {
        let (mut d, x) = one(r#"<input id=x type=text value="">"#);
        d.set_value(x, value);
        assert!(d.set_selection_range(x, sel.0, sel.1, None));
        assert!(d.set_range_text(x, replacement, Some(*range), "preserve"));
        assert_eq!(&d.value(x), want_value, "{value:?} sel {sel:?} ← {replacement:?}@{range:?}");
        assert_eq!(
            (d.selection_start(x), d.selection_end(x)),
            (Some(want_sel.0), Some(want_sel.1)),
            "{value:?} sel {sel:?} ← {replacement:?}@{range:?}"
        );
    }
}

#[test]
fn a_start_past_its_end_is_an_index_size_error_but_one_past_the_value_is_not() {
    let (mut d, x) = one(r#"<input id=x type=text value="Hello">"#);
    // Chrome: IndexSizeError. The check is on the ARGUMENTS, before clamping.
    assert!(!d.set_range_text(x, "Z", Some((3, 1)), "preserve"));
    assert_eq!(d.value(x), "Hello", "the failed call must not have written");

    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "Hi");
    assert!(d.set_range_text(x, "Z", Some((10, 20)), "preserve"));
    assert_eq!(d.value(x), "HiZ");
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(2), Some(2)));
}

#[test]
fn an_unrecognised_select_mode_is_the_idl_enums_type_error() {
    let (mut d, x) = one(r#"<input id=x type=text value="Hello">"#);
    assert!(!d.set_range_text(x, "Z", Some((0, 1)), "bogus"));
    assert_eq!(d.value(x), "Hello");
}

#[test]
fn set_range_text_leaves_the_range_directionless() {
    let (mut d, x) = one(r#"<input id=x type=text value="Hello">"#);
    assert!(d.set_selection_range(x, 1, 3, Some("backward")));
    assert!(d.set_range_text(x, "Q", None, "preserve"));
    assert_eq!(
        (d.value(x), d.selection_start(x), d.selection_end(x), d.selection_direction(x)),
        ("HQlo".to_string(), Some(1), Some(2), Some("none")),
    );
}

// ─── the value setter's effect on the selection ─────────────────────────────

#[test]
fn assigning_a_different_value_collapses_the_selection_to_the_end() {
    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "abcdef");
    assert!(d.set_selection_range(x, 2, 4, None));
    d.set_value(x, "ghijkl");
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(6), Some(6)));
}

#[test]
fn assigning_the_same_value_leaves_the_selection_alone() {
    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "abcdef");
    assert!(d.set_selection_range(x, 2, 4, None));
    d.set_value(x, "abcdef");
    assert_eq!(
        (d.selection_start(x), d.selection_end(x)),
        (Some(2), Some(4)),
        "Chrome keeps [2,4] — the cursor moves only when the value CHANGES"
    );
}

// ─── textarea ───────────────────────────────────────────────────────────────

#[test]
fn textarea_offsets_index_the_lf_normalised_api_value() {
    let mut d = form();
    let ta = el(&d, "ta");
    assert_eq!(d.value(ta), "line1\nline2");
    d.select(ta);
    assert_eq!((d.selection_start(ta), d.selection_end(ta)), (Some(0), Some(11)));
    assert!(d.set_range_text(ta, "X", Some((0, 5)), "preserve"));
    assert_eq!(d.value(ta), "X\nline2", "the newline is one offset, not two");
}

// ─── encodings: the two fixtures that disagree ──────────────────────────────

#[test]
fn a_bmp_non_ascii_value_is_indexed_in_characters_not_bytes() {
    // "日本ab" is 4 UTF-16 units and 10 UTF-8 bytes. A byte-indexed
    // implementation reads 10 here and slices in the middle of a codepoint.
    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "日本ab");
    d.select(x);
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(0), Some(4)));

    assert!(d.set_selection_range(x, 1, 3, None));
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(1), Some(3)));

    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "日本ab");
    assert!(d.set_range_text(x, "Q", Some((1, 3)), "preserve"));
    assert_eq!(d.value(x), "日Qb");
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(3), Some(3)));
}

#[test]
fn an_astral_value_is_indexed_in_utf16_units_not_characters() {
    // "😀ab" is FOUR UTF-16 units and THREE chars — `value.length` is 4 in
    // Chrome. A char-indexed implementation answers 3 and is wrong at every
    // offset past the emoji.
    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "😀ab");
    d.select(x);
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(0), Some(4)));

    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "😀");
    assert!(d.set_selection_range(x, 0, 99, None));
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(0), Some(2)));

    // Both offsets past the end, and in the wrong order. `store_selection`
    // carries no length clamp of its own — it relies on `utf16_to_char_floor`
    // saturating — so this is the assertion that makes that reliance visible.
    assert!(d.set_selection_range(x, 99, 50, None));
    assert_eq!((d.selection_start(x), d.selection_end(x)), (Some(2), Some(2)));

    // An offset PAST the pair is exact.
    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "😀ab");
    assert!(d.set_range_text(x, "Q", Some((2, 3)), "preserve"));
    assert_eq!(d.value(x), "😀Qb");
}

#[test]
fn a_boundary_inside_a_surrogate_pair_moves_outward_the_one_named_deviation() {
    // Chrome's `setRangeText("Q",1,2)` on "😀ab" yields "\ud83dQab" — a LONE
    // SURROGATE, which a Rust `String` cannot hold. The replaced range rounds
    // outward instead, so the whole emoji goes. Pinned rather than papered
    // over: this is the single place this file knowingly differs from Chrome.
    let (mut d, x) = one(r#"<input id=x type=text value="">"#);
    d.set_value(x, "😀ab");
    assert!(d.set_range_text(x, "Q", Some((1, 2)), "preserve"));
    assert_eq!(d.value(x), "Qab", "Chrome: \"\\ud83dQab\"");
}

// ─── the select event ───────────────────────────────────────────────────────

#[test]
fn setting_a_selection_fires_select_at_the_control() {
    use std::sync::{Arc, Mutex};
    let mut d = form();
    let t = el(&d, "t");
    let seen = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&seen);
    d.add_event_listener(
        t,
        "select",
        Box::new(move |_, _| { *counter.lock().unwrap() += 1; }),
        Default::default(),
    );
    d.select(t);
    assert!(d.set_selection_range(t, 0, 2, None));
    assert!(d.set_range_text(t, "Q", Some((0, 1)), "preserve"));
    assert_eq!(*seen.lock().unwrap(), 3, "select(), setSelectionRange and setRangeText each fire it");
}
