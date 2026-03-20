// Tests for Tab/focus keyboard navigation and :focus visual indicator.

use crate::html::parse_html;
use crate::layout::LayoutEngine;
use crate::types::{Document, Color};
use crate::types::BorderStyle;
use crate::css::apply_cascade_vp;

// ── helpers ───────────────────────────────────────────────────────────────────

fn parse_and_layout(html: &str) -> Document {
    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.viewport_w = 800.0;
    engine.viewport_h = 600.0;
    engine.layout(&mut doc, 800.0);
    doc
}

// ── 1. Tab advances focus through native-focusable elements ───────────────────

#[test]
fn tab_focus_advances_in_document_order() {
    let mut doc = parse_and_layout(
        "<html><body><button id=a>A</button><button id=b>B</button><button id=c>C</button></body></html>",
    );

    // Initially no focus.
    assert!(doc.focused_box.is_null(), "no initial focus");

    // Tab → first focusable (button A).
    assert!(doc.focus_next(), "first Tab must return true");
    let a_ptr = crate::dom::query_selector(&doc.root, "#a").map(|n| n as *const _);
    assert_eq!(doc.focused_box, a_ptr.unwrap(), "focus must be on button A after first Tab");

    // Tab → second (B).
    assert!(doc.focus_next());
    let b_ptr = crate::dom::query_selector(&doc.root, "#b").map(|n| n as *const _);
    assert_eq!(doc.focused_box, b_ptr.unwrap(), "focus must be on button B");

    // Tab → third (C).
    assert!(doc.focus_next());
    let c_ptr = crate::dom::query_selector(&doc.root, "#c").map(|n| n as *const _);
    assert_eq!(doc.focused_box, c_ptr.unwrap(), "focus must be on button C");

    // Tab → wraps back to A.
    assert!(doc.focus_next());
    assert_eq!(doc.focused_box, a_ptr.unwrap(), "focus must wrap to button A");
}

#[test]
fn shift_tab_reverses_focus() {
    let mut doc = parse_and_layout(
        "<html><body><button id=a>A</button><button id=b>B</button></body></html>",
    );
    // Focus A, then Shift+Tab should go to B (wrap).
    doc.focus_next();
    doc.focus_prev();
    let b_ptr = crate::dom::query_selector(&doc.root, "#b").map(|n| n as *const _);
    assert_eq!(doc.focused_box, b_ptr.unwrap(), "Shift+Tab from first element must wrap to last");
}

#[test]
fn tab_includes_inputs_and_anchors() {
    let mut doc = parse_and_layout(
        "<html><body>\
            <input type=\"text\" id=i>\
            <a href=\"/\" id=l>Link</a>\
            <textarea id=t></textarea>\
        </body></html>",
    );
    let mut visited_ids: Vec<&str> = Vec::new();
    for _ in 0..3 {
        doc.focus_next();
        let focused = doc.focused_box;
        for id in ["i", "l", "t"] {
            if let Some(node) = crate::dom::query_selector(&doc.root, &format!("#{id}")) {
                if std::ptr::eq(node as *const _, focused) {
                    visited_ids.push(id);
                }
            }
        }
    }
    assert_eq!(visited_ids.len(), 3, "Tab must visit input, link, and textarea");
}

// ── 2. tabindex=-1 excluded, tabindex=0 included in normal order ──────────────

#[test]
fn tabindex_minus1_excluded_from_tab_order() {
    let mut doc = parse_and_layout(
        "<html><body>\
            <button id=a>A</button>\
            <button id=b tabindex=\"-1\">B (skip)</button>\
            <button id=c>C</button>\
        </body></html>",
    );
    doc.focus_next(); // → A
    doc.focus_next(); // → C (B is skipped)
    let c_ptr = crate::dom::query_selector(&doc.root, "#c").map(|n| n as *const _);
    assert_eq!(doc.focused_box, c_ptr.unwrap(), "tabindex=-1 element must be skipped in Tab order");
}

#[test]
fn tabindex_zero_included_in_normal_order() {
    let mut doc = parse_and_layout(
        "<html><body>\
            <div id=d tabindex=\"0\">Div</div>\
            <button id=b>Button</button>\
        </body></html>",
    );
    doc.focus_next(); // → div (tabindex=0, first in document order)
    let d_ptr = crate::dom::query_selector(&doc.root, "#d").map(|n| n as *const _);
    assert_eq!(doc.focused_box, d_ptr.unwrap(), "tabindex=0 element must be in tab order");
}

// ── 3. Positive tabindex ordering ─────────────────────────────────────────────

#[test]
fn positive_tabindex_sorted_before_normal_focusable() {
    let mut doc = parse_and_layout(
        "<html><body>\
            <button id=first>First (no tabindex)</button>\
            <button id=high tabindex=\"3\">High index</button>\
            <button id=low  tabindex=\"1\">Low index</button>\
        </body></html>",
    );
    // Tab order: tabindex=1 → tabindex=3 → natural (no tabindex)
    doc.focus_next();
    let low_ptr  = crate::dom::query_selector(&doc.root, "#low").map(|n| n as *const _);
    assert_eq!(doc.focused_box, low_ptr.unwrap(), "tabindex=1 must be first in tab order");

    doc.focus_next();
    let high_ptr = crate::dom::query_selector(&doc.root, "#high").map(|n| n as *const _);
    assert_eq!(doc.focused_box, high_ptr.unwrap(), "tabindex=3 must come before natural focusable");

    doc.focus_next();
    let first_ptr = crate::dom::query_selector(&doc.root, "#first").map(|n| n as *const _);
    assert_eq!(doc.focused_box, first_ptr.unwrap(), "natural button must come last");
}

// ── 4. Viewport stored in Document after layout ───────────────────────────────

#[test]
fn layout_stores_viewport_in_doc() {
    let doc = parse_and_layout("<html><body></body></html>");
    assert_eq!(doc.viewport_w, 800.0);
    assert_eq!(doc.viewport_h, 600.0);
}

// ── 5. focus_next / focus_prev fire Focus/Blur events ─────────────────────────

#[test]
fn tab_fires_focus_event() {
    use crate::dom::HtmlEventType;
    use std::sync::{Arc, Mutex};

    let mut doc = parse_and_layout(
        "<html><body><button id=a>A</button><button id=b>B</button></body></html>",
    );

    let focused_count = Arc::new(Mutex::new(0u32));
    let fc = focused_count.clone();

    // "button" selector matches both buttons; Focus is direct (non-bubbling) so
    // it fires on the focused element directly.
    doc.events.add("button", HtmlEventType::Focus, Box::new(move |_evt| {
        *fc.lock().unwrap() += 1;
    }));

    doc.focus_next(); // A gets focus → Focus fires on A
    doc.focus_next(); // B gets focus → Focus fires on B

    assert_eq!(*focused_count.lock().unwrap(), 2, "each Tab must fire a Focus event on the target");
}

// ── 6. :focus-visible indicator — keyboard only, not mouse ───────────────────

#[test]
fn keyboard_focused_element_has_ua_outline() {
    let mut doc = parse_and_layout(
        "<html><body><button id=btn>Click</button></body></html>",
    );
    // Tab = keyboard focus → :focus-visible fires → UA outline appears.
    doc.focus_next();
    assert!(doc.keyboard_focus, "focus_next must set keyboard_focus=true");

    let btn = crate::dom::query_selector(&doc.root, "#btn").unwrap();
    assert!(
        btn.style.outline_width > 0.0,
        "keyboard-focused element must have UA outline"
    );
    assert_ne!(
        btn.style.outline_style,
        BorderStyle::None,
        "keyboard-focused element must have non-None outline_style"
    );
}

#[test]
fn unfocused_element_has_no_ua_outline() {
    let doc = parse_and_layout(
        "<html><body><button id=btn>Click</button></body></html>",
    );
    let btn = crate::dom::query_selector(&doc.root, "#btn").unwrap();
    assert_eq!(
        btn.style.outline_style,
        BorderStyle::None,
        "unfocused element must not have an outline"
    );
}

#[test]
fn mouse_focus_does_not_show_outline() {
    // Simulate what happens after a mouse click sets focus (keyboard_focus = false).
    // :focus-visible must NOT match, so no UA outline.
    let mut doc = parse_and_layout(
        "<html><body><button id=btn>Click</button></body></html>",
    );
    // Manually set focus as if from mouse (keyboard_focus stays false).
    let btn_ptr = crate::dom::query_selector(&doc.root, "#btn").map(|n| n as *const _).unwrap();
    doc.focused_box = btn_ptr;
    doc.keyboard_focus = false;
    // Recascade with keyboard_focus=false.
    doc.stylesheet.rebuild_index();
    crate::css::apply_cascade_vp(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        doc.viewport_w, doc.viewport_h, doc.focused_box, false,
    );

    let btn = crate::dom::query_selector(&doc.root, "#btn").unwrap();
    assert_eq!(
        btn.style.outline_style,
        BorderStyle::None,
        "mouse-focused element must NOT get the :focus-visible UA outline"
    );
}

#[test]
fn author_can_override_focus_outline_color() {
    let mut doc = parse_and_layout(
        r#"<html><head><style>
            button:focus { outline-color: #ff0000; }
        </style></head><body><button id=btn>B</button></body></html>"#,
    );
    doc.focus_next();

    let btn = crate::dom::query_selector(&doc.root, "#btn").unwrap();
    // Author rule overrides UA outline color to red.
    assert_eq!(
        btn.style.outline_color,
        Color::rgb(0xff, 0x00, 0x00),
        "author :focus outline-color must override the UA default"
    );
}

#[test]
fn author_can_suppress_focus_outline() {
    let mut doc = parse_and_layout(
        r#"<html><head><style>
            button:focus { outline: none; }
        </style></head><body><button id=btn>B</button></body></html>"#,
    );
    doc.focus_next();

    let btn = crate::dom::query_selector(&doc.root, "#btn").unwrap();
    assert_eq!(
        btn.style.outline_style,
        BorderStyle::None,
        "author outline:none must suppress the UA focus outline"
    );
}

// ── 7. text inputs always show :focus-visible even on mouse click ─────────────

#[test]
fn mouse_focused_text_input_shows_outline() {
    // <input type="text"> is a text-entry control: :focus-visible must match
    // even when keyboard_focus=false (mouse click).
    let mut doc = parse_and_layout(
        "<html><body><input type=\"text\" id=inp></body></html>",
    );
    let inp_ptr = crate::dom::query_selector(&doc.root, "#inp").map(|n| n as *const _).unwrap();
    doc.focused_box = inp_ptr;
    doc.keyboard_focus = false;
    doc.stylesheet.rebuild_index();
    apply_cascade_vp(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        doc.viewport_w, doc.viewport_h, doc.focused_box, false,
    );

    let inp = crate::dom::query_selector(&doc.root, "#inp").unwrap();
    assert!(
        inp.style.outline_width > 0.0,
        "mouse-focused text input must still get :focus-visible outline"
    );
}

#[test]
fn mouse_focused_button_no_outline() {
    // <button> is not text-entry: no :focus-visible on mouse click.
    let mut doc = parse_and_layout(
        "<html><body><button id=btn>Click</button></body></html>",
    );
    let btn_ptr = crate::dom::query_selector(&doc.root, "#btn").map(|n| n as *const _).unwrap();
    doc.focused_box = btn_ptr;
    doc.keyboard_focus = false;
    doc.stylesheet.rebuild_index();
    apply_cascade_vp(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        doc.viewport_w, doc.viewport_h, doc.focused_box, false,
    );

    let btn = crate::dom::query_selector(&doc.root, "#btn").unwrap();
    assert_eq!(
        btn.style.outline_style,
        BorderStyle::None,
        "mouse-focused button must NOT show :focus-visible outline"
    );
}

// ── 8. contenteditable included in tab order ──────────────────────────────────

#[test]
fn contenteditable_is_focusable() {
    let mut doc = parse_and_layout(
        "<html><body><div id=ed contenteditable=\"true\">Edit me</div></body></html>",
    );
    doc.focus_next();
    let ed_ptr = crate::dom::query_selector(&doc.root, "#ed").map(|n| n as *const _);
    assert_eq!(doc.focused_box, ed_ptr.unwrap(), "contenteditable must be in tab order");
}

// ── 8. display:none elements skipped ─────────────────────────────────────────

#[test]
fn display_none_element_skipped_in_tab_order() {
    let mut doc = parse_and_layout(
        "<html><head><style>#hidden { display: none; }</style></head>\
         <body><button id=hidden>Hidden</button><button id=vis>Visible</button></body></html>",
    );
    doc.focus_next();
    let vis_ptr = crate::dom::query_selector(&doc.root, "#vis").map(|n| n as *const _);
    assert_eq!(doc.focused_box, vis_ptr.unwrap(), "display:none button must not be in tab order");
}
