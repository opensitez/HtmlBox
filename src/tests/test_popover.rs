//! The popover API, the top layer, and the two pseudo-classes that used to
//! match nothing — HTML §6.12, CSS Position §6.
//!
//! `:modal` and `:popover-open` were both in `is_known_pseudo_class`, so a
//! stylesheet using them PARSED and the rule simply never applied. That is
//! indistinguishable from a rule that is merely never true, which is why the
//! tests below assert through the CASCADE rather than through the API alone.
//!
//! All measured (`/tmp/webcore-html/pv3.html`, `pv4.html`, `pv5.html`).

use crate::types::{Document, Position, TopLayerKind};

const PAGE: &str = "<div id=pa popover>A</div>\
<div id=pa2 popover=auto>A2</div>\
<div id=ph popover=hint>H</div>\
<div id=pm popover=manual>M</div>\
<div id=pb popover=bogus>B</div>\
<div id=plain>plain</div>\
<dialog id=dlg1></dialog><dialog id=dlg2></dialog>";

fn page() -> Document {
    let mut renderer = crate::Renderer::new();
    renderer.load_html(PAGE, 400.0)
}
fn el(d: &Document, id: &str) -> u32 {
    d.get_element_by_id(id).unwrap()
}

// ─── the popover attribute ──────────────────────────────────────────────────

#[test]
fn the_invalid_value_default_is_manual_and_a_bare_attribute_is_auto() {
    // ⛔ `bogus` is `manual`, not `auto` — and those are opposite ends of the
    // light-dismiss rule, so collapsing them is not cosmetic.
    let d = page();
    assert_eq!(
        d.popover(el(&d, "pa")).as_deref(),
        Some("auto"),
        "a bare attribute"
    );
    assert_eq!(d.popover(el(&d, "pa2")).as_deref(), Some("auto"));
    assert_eq!(d.popover(el(&d, "ph")).as_deref(), Some("hint"));
    assert_eq!(d.popover(el(&d, "pm")).as_deref(), Some("manual"));
    assert_eq!(
        d.popover(el(&d, "pb")).as_deref(),
        Some("manual"),
        "invalid → manual"
    );
    assert_eq!(
        d.popover(el(&d, "plain")),
        None,
        "absent is null, not a keyword"
    );
}

#[test]
fn the_popover_setter_takes_null_to_remove_the_attribute() {
    let mut d = page();
    let e = el(&d, "plain");
    d.set_popover(e, Some("AUTO"));
    assert_eq!(d.popover(e).as_deref(), Some("auto"), "case-insensitive");
    d.set_popover(e, None);
    assert_eq!(d.popover(e), None);
    assert!(!d.has_attribute(e, "popover"));
}

// ─── showing and hiding ─────────────────────────────────────────────────────

#[test]
fn showing_a_popover_puts_it_in_the_top_layer_and_hiding_takes_it_out() {
    let mut d = page();
    let m = el(&d, "pm");
    assert!(!d.popover_open(m));
    assert!(d.show_popover(m));
    assert!(d.popover_open(m));
    assert_eq!(d.top_layer_nodes(), &[m]);
    assert!(d.hide_popover(m));
    assert!(!d.popover_open(m));
    assert!(d.top_layer_nodes().is_empty());
}

#[test]
fn showing_one_that_is_already_showing_succeeds_and_changes_nothing() {
    // Measured in isolation, because an earlier probe had shown the popover on
    // a previous line and could not tell "returned early" from "threw".
    let mut d = page();
    let m = el(&d, "pm");
    assert!(d.show_popover(m));
    assert!(d.show_popover(m), "no error");
    assert_eq!(d.top_layer_nodes(), &[m], "and no duplicate entry");
    assert!(d.hide_popover(m));
    assert!(d.hide_popover(m), "hiding a hidden one is fine too");
}

#[test]
fn a_non_popover_and_a_disconnected_one_are_both_refused() {
    let mut d = page();
    let plain = el(&d, "plain");
    assert!(!d.show_popover(plain), "NotSupportedError");
    assert!(!d.hide_popover(plain));

    let loose = d.create_element("div");
    d.set_popover(loose, Some("auto"));
    assert!(!d.show_popover(loose), "InvalidStateError — not connected");
    assert!(d.top_layer_nodes().is_empty());
}

#[test]
fn toggle_returns_whether_it_ends_up_showing() {
    let mut d = page();
    let m = el(&d, "pm");
    assert!(d.toggle_popover(m, None));
    assert!(!d.toggle_popover(m, None));
    assert!(d.toggle_popover(m, Some(true)));
    assert!(
        d.toggle_popover(m, Some(true)),
        "forcing twice keeps it showing"
    );
    assert!(!d.toggle_popover(m, Some(false)));
}

// ─── light dismiss: four rows, four different answers ───────────────────────

#[test]
fn light_dismiss_is_keyed_on_the_state_of_the_one_being_opened() {
    // The whole rule, measured row by row. "Popovers close each other" is
    // wrong in three of these four.
    let mut d = page();
    let (a, a2, h, m) = (el(&d, "pa"), el(&d, "pa2"), el(&d, "ph"), el(&d, "pm"));

    // auto closes auto
    d.show_popover(a);
    d.show_popover(a2);
    assert!(!d.popover_open(a), "the first auto went");
    assert!(d.popover_open(a2));

    // auto closes hint
    d.hide_popover(a2);
    d.show_popover(h);
    d.show_popover(a);
    assert!(!d.popover_open(h), "opening an auto dismisses a hint");
    assert!(d.popover_open(a));

    // hint does NOT close auto
    d.show_popover(h);
    assert!(d.popover_open(a), "opening a hint leaves an auto alone");
    assert!(d.popover_open(h));

    // manual closes nothing, and nothing closes a manual
    d.show_popover(m);
    assert!(d.popover_open(a), "a manual dismisses nothing");
    d.hide_popover(a);
    d.show_popover(a);
    assert!(d.popover_open(m), "and an auto does not dismiss a manual");
}

#[test]
fn a_hint_closes_another_hint() {
    let mut d = page();
    let h = el(&d, "ph");
    let h2 = d.create_element("div");
    d.set_popover(h2, Some("hint"));
    let body = d.body().unwrap();
    d.append_child(body, h2);
    d.show_popover(h);
    d.show_popover(h2);
    assert!(!d.popover_open(h));
    assert!(d.popover_open(h2));
}

// ─── beforetoggle ───────────────────────────────────────────────────────────

#[test]
fn before_toggle_is_cancelable_and_cancelling_it_stops_the_show() {
    // ⛔ An event fired but not honoured is write-only surface. Measured:
    // `preventDefault()` in `beforetoggle` leaves `:popover-open` false.
    use std::sync::{Arc, Mutex};
    let mut d = page();
    let m = el(&d, "pm");
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let rec = Arc::clone(&seen);
    d.add_event_listener(
        m,
        "beforetoggle",
        Box::new(
            move |ev: &mut crate::dom::events::DomEvent, _: &mut Document| {
                rec.lock()
                    .unwrap()
                    .push((ev.old_state.clone(), ev.new_state.clone()));
                ev.prevent_default();
            },
        ),
        Default::default(),
    );
    assert!(!d.show_popover(m), "the listener cancelled it");
    assert!(!d.popover_open(m));
    assert_eq!(
        *seen.lock().unwrap(),
        [("closed".to_string(), "open".to_string())]
    );
}

#[test]
fn before_toggle_reports_both_states_when_it_is_not_cancelled() {
    use std::sync::{Arc, Mutex};
    let mut d = page();
    let m = el(&d, "pm");
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let rec = Arc::clone(&seen);
    d.add_event_listener(
        m,
        "beforetoggle",
        Box::new(
            move |ev: &mut crate::dom::events::DomEvent, _: &mut Document| {
                rec.lock()
                    .unwrap()
                    .push((ev.old_state.clone(), ev.new_state.clone()));
            },
        ),
        Default::default(),
    );
    d.show_popover(m);
    d.hide_popover(m);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            ("closed".to_string(), "open".to_string()),
            ("open".to_string(), "closed".to_string())
        ]
    );
}

// ─── the pseudo-classes, through the cascade ────────────────────────────────

#[test]
fn popover_open_now_matches_and_drives_the_ua_display_rule() {
    // ⛔ Asserted through the CASCADE. `:popover-open` parsed before this
    // landed and matched nothing, so an API-only test would have passed
    // against a selector that never applied.
    let mut d = page();
    let m = el(&d, "pm");
    d.recascade();
    assert_eq!(
        d.get_computed_style(m).map(|s| s.display),
        Some(crate::types::Display::None),
        "`[popover]:not(:popover-open)` hides it"
    );
    assert_eq!(
        d.get_computed_style(m).map(|s| s.position),
        Some(Position::Fixed),
        "and `[popover]` positions it"
    );

    d.show_popover(m);
    d.recascade();
    assert_ne!(
        d.get_computed_style(m).map(|s| s.display),
        Some(crate::types::Display::None),
        "showing it makes the negation stop matching"
    );
}

#[test]
fn modal_matches_show_modal_and_not_show() {
    let mut d = page();
    let modal = el(&d, "dlg1");
    let plain = el(&d, "dlg2");
    d.show_dialog(modal, true);
    d.show_dialog(plain, false);
    assert!(
        d.dialog_open(modal) && d.dialog_open(plain),
        "both are open"
    );
    assert_eq!(
        d.top_layer_nodes(),
        &[modal],
        "only one is in the top layer"
    );
    d.recascade();
    assert_eq!(
        d.get_computed_style(modal).map(|s| s.position),
        Some(Position::Fixed)
    );
    assert_ne!(
        d.get_computed_style(plain).map(|s| s.position),
        Some(Position::Fixed)
    );

    d.close_dialog(modal);
    d.recascade();
    assert!(d.top_layer_nodes().is_empty());
    assert_ne!(
        d.get_computed_style(modal).map(|s| s.position),
        Some(Position::Fixed),
        "closing drops the rule with the membership"
    );
}

// ─── the two halves of the state must agree ─────────────────────────────────

#[test]
fn the_ordered_list_and_the_node_flag_never_drift() {
    // ⛔ Two representations of one fact, which is the `checkedness`-vs-arena
    // hazard. They have exactly one write point each; this drives a mixed
    // sequence through both kinds and checks they still agree.
    let mut d = page();
    let (a, m, dlg) = (el(&d, "pa"), el(&d, "pm"), el(&d, "dlg1"));
    let check = |d: &Document, what: &str| {
        // Every node in the list carries a kind…
        for id in d.top_layer_nodes() {
            assert!(
                d.find_webcore(*id).and_then(|n| n.top_layer_kind).is_some(),
                "{what}: {id} is in the list with no kind"
            );
        }
        // …and no node outside it does.
        for id in [a, m, dlg] {
            let flagged = d.find_webcore(id).and_then(|n| n.top_layer_kind).is_some();
            assert_eq!(
                flagged,
                d.top_layer_nodes().contains(&id),
                "{what}: {id} flag/list disagree"
            );
        }
    };
    check(&d, "empty");
    d.show_dialog(dlg, true);
    check(&d, "after showModal");
    d.show_popover(m);
    check(&d, "after showPopover(manual)");
    d.show_popover(a);
    check(&d, "after showPopover(auto)");
    assert_eq!(
        d.top_layer_nodes(),
        &[dlg, m, a],
        "bottom-first, in entry order"
    );
    d.close_dialog(dlg);
    check(&d, "after close");
    d.hide_popover(a);
    check(&d, "after hidePopover");
    assert_eq!(d.top_layer_nodes(), &[m]);
    assert_eq!(
        d.find_webcore(m).and_then(|n| n.top_layer_kind),
        Some(TopLayerKind::Popover)
    );
}

#[test]
fn the_two_kinds_do_not_answer_each_others_pseudo_class() {
    // ⛔ A mutation found this: every earlier test had ONE kind in the top
    // layer at a time, so `:modal` matching any member — and `:popover-open`
    // matching any member — passed both. The enum exists exactly for this, and
    // nothing was checking it. Both kinds are open here at once.
    let mut d = page();
    let modal = el(&d, "dlg1");
    let pop = el(&d, "pm");
    d.show_dialog(modal, true);
    assert!(d.show_popover(pop));
    assert_eq!(d.top_layer_nodes(), &[modal, pop], "both kinds, at once");
    d.recascade();

    // The popover is in the top layer and must NOT be `:modal` — which the UA
    // sheet asks by giving `dialog:modal` its position.
    assert_eq!(
        d.find_webcore(pop).and_then(|n| n.top_layer_kind),
        Some(TopLayerKind::Popover)
    );
    assert_eq!(
        d.find_webcore(modal).and_then(|n| n.top_layer_kind),
        Some(TopLayerKind::ModalDialog)
    );

    // And the modal dialog must not satisfy `[popover]:not(:popover-open)`
    // either way — it has no `popover` attribute — so the discriminating
    // check is the popover's own display: it is SHOWING, so the negation must
    // not hide it, even with a modal dialog sitting in the layer beside it.
    assert_ne!(
        d.get_computed_style(pop).map(|s| s.display),
        Some(crate::types::Display::None),
        "an open popover stays visible while a modal is also in the layer"
    );

    // Now close the popover but leave the modal: `:popover-open` must stop
    // matching even though the layer is still occupied.
    d.hide_popover(pop);
    d.recascade();
    assert_eq!(
        d.get_computed_style(pop).map(|s| s.display),
        Some(crate::types::Display::None),
        "a modal in the layer must not keep `:popover-open` true"
    );
    assert_eq!(
        d.get_computed_style(modal).map(|s| s.position),
        Some(Position::Fixed),
        "and the modal is still modal"
    );
}

#[test]
fn showing_a_modal_twice_does_not_double_enter_the_top_layer() {
    // `show_dialog` has no already-open early return, so it is the one API
    // path that can call `add_to_top_layer` twice for the same node.
    let mut d = page();
    let modal = el(&d, "dlg1");
    d.show_dialog(modal, true);
    d.show_dialog(modal, true);
    assert_eq!(d.top_layer_nodes(), &[modal], "one entry, not two");
    d.close_dialog(modal);
    assert!(d.top_layer_nodes().is_empty(), "and one removal clears it");
}

#[test]
fn the_pseudo_classes_are_keyed_on_the_kind_not_on_membership() {
    // ⛔ Two mutations survived the test above: making EITHER pseudo-class
    // answer "is in the top layer" left it green, because `dialog:modal` and
    // `[popover]:not(:popover-open)` are both tag-constrained — the popover is
    // not a `<dialog>` and the dialog has no `popover` attribute, so the
    // compound fails before the pseudo-class is ever consulted.
    //
    // These selectors ask the pseudo-classes DIRECTLY, of an element of the
    // other kind, with both kinds in the layer at once.
    let mut renderer = crate::Renderer::new();
    let mut d = renderer.load_html(
        "<style>         #pop:modal { color: rgb(1, 2, 3); }         #dlg:popover-open { color: rgb(4, 5, 6); }         </style>         <div id=pop popover=manual>P</div><dialog id=dlg></dialog>",
        400.0,
    );
    let pop = el(&d, "pop");
    let dlg = el(&d, "dlg");
    d.show_dialog(dlg, true);
    assert!(d.show_popover(pop));
    assert_eq!(d.top_layer_nodes(), &[dlg, pop], "both kinds in the layer");
    d.recascade();

    let colour = |d: &Document, id: u32| d.get_computed_style(id).map(|s| s.color);
    let modal_colour = crate::types::Color {
        r: 1,
        g: 2,
        b: 3,
        a: 255,
    };
    let popover_colour = crate::types::Color {
        r: 4,
        g: 5,
        b: 6,
        a: 255,
    };
    assert_ne!(
        colour(&d, pop),
        Some(modal_colour),
        "a showing POPOVER is in the top layer and is not `:modal`"
    );
    assert_ne!(
        colour(&d, dlg),
        Some(popover_colour),
        "a modal DIALOG is in the top layer and is not `:popover-open`"
    );

    // And the same selectors DO match the right element, so the rules are not
    // merely inert — the check above would pass against a broken selector.
    let mut d2 = renderer.load_html(
        "<style>         #pop:popover-open { color: rgb(4, 5, 6); }         #dlg:modal { color: rgb(1, 2, 3); }         </style>         <div id=pop popover=manual>P</div><dialog id=dlg></dialog>",
        400.0,
    );
    let pop2 = el(&d2, "pop");
    let dlg2 = el(&d2, "dlg");
    d2.show_dialog(dlg2, true);
    assert!(d2.show_popover(pop2));
    d2.recascade();
    assert_eq!(
        colour(&d2, pop2),
        Some(popover_colour),
        "`:popover-open` matches the popover"
    );
    assert_eq!(
        colour(&d2, dlg2),
        Some(modal_colour),
        "`:modal` matches the dialog"
    );
}
