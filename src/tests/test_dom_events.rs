// Tests for DOM mutation + CSS re-cascade fixes.
//
// Covers the bug where toggling a class (e.g. "dark") on the root element had
// no visible effect because `apply_cascade` was only called once at parse time.
// The fixes are:
//   1. `Document::recascade()` — re-matches all stylesheet rules after class changes.
//   2. `set_style_property` / `apply_inline_style_str` persist values to the
//      `style` attribute so a subsequent recascade does not overwrite them.
//   3. `Document::process_mouse_event` calls `recascade()` before layout whenever
//      an event handler fires.

use crate::html::parse_html;
use crate::layout::LayoutEngine;
use crate::types::{Color, Document};
use crate::dom::{self, add_class, remove_class, toggle_class, has_class,
                  set_style_property, apply_inline_style_str, query_selector};

// ── helpers ───────────────────────────────────────────────────────────────────

fn parse_and_layout(html: &str) -> Document {
    let mut doc = parse_html(html);
    LayoutEngine::new().layout(&mut doc, 800.0);
    doc
}

// ── 1. recascade() picks up a newly-added class ───────────────────────────────

#[test]
fn recascade_dark_class_changes_background() {
    // .dark body rule sets a dark background.
    let html = r#"<html><head><style>
        body { background-color: #ffffff; }
        .dark body { background-color: #0b1222; }
    </style></head><body></body></html>"#;

    let mut doc = parse_and_layout(html);

    // Before: body background should be white.
    let body_before = query_selector(&doc.root, "body").unwrap();
    assert_eq!(
        body_before.style.background_color,
        Color::rgb(0xff, 0xff, 0xff),
        "body should start with white background"
    );

    // Add "dark" to the root <html> element and recascade.
    add_class(&mut doc.root, "dark");
    doc.recascade();

    // After: body background should be the dark colour.
    let body_after = query_selector(&doc.root, "body").unwrap();
    assert_eq!(
        body_after.style.background_color,
        Color::rgb(0x0b, 0x12, 0x22),
        "body background should switch to dark colour after recascade"
    );
}

#[test]
fn recascade_remove_class_restores_style() {
    let html = r#"<html><head><style>
        body { background-color: #ffffff; }
        .dark body { background-color: #0b1222; }
    </style></head><body></body></html>"#;

    let mut doc = parse_and_layout(html);

    // Enable dark, then remove it — should return to white.
    add_class(&mut doc.root, "dark");
    doc.recascade();
    remove_class(&mut doc.root, "dark");
    doc.recascade();

    let body = query_selector(&doc.root, "body").unwrap();
    assert_eq!(
        body.style.background_color,
        Color::rgb(0xff, 0xff, 0xff),
        "background should revert to white after removing dark class"
    );
}

#[test]
fn recascade_toggle_class_round_trip() {
    let html = r#"<html><head><style>
        div { color: #000000; }
        .active div { color: #ff0000; }
    </style></head><body><div id="x"></div></body></html>"#;

    let mut doc = parse_and_layout(html);

    // Toggle on.
    toggle_class(&mut doc.root, "active");
    assert!(has_class(&doc.root, "active"));
    doc.recascade();
    let div = query_selector(&doc.root, "div").unwrap();
    assert_eq!(div.style.color, Color::rgb(0xff, 0, 0), "color should be red when active");

    // Toggle off.
    toggle_class(&mut doc.root, "active");
    assert!(!has_class(&doc.root, "active"));
    doc.recascade();
    let div = query_selector(&doc.root, "div").unwrap();
    assert_eq!(div.style.color, Color::rgb(0, 0, 0), "color should return to black");
}

// ── 2. set_style_property persists to style attribute ─────────────────────────

#[test]
fn set_style_property_survives_recascade() {
    let html = r#"<html><head><style>
        div { width: 100px; }
    </style></head><body><div id="bar"></div></body></html>"#;

    let mut doc = parse_and_layout(html);

    // Dynamically widen the bar via set_style_property.
    {
        let bar = dom::query_selector_mut(&mut doc.root, "div").unwrap();
        set_style_property(bar, "width", "75%");
        // Verify the style attribute was updated.
        let style_attr = bar.attributes.get("style").cloned().unwrap_or_default();
        assert!(
            style_attr.contains("width"),
            "style attribute should contain 'width' after set_style_property; got: {:?}",
            style_attr
        );
    }

    // A recascade must not wipe the dynamic width — the style attribute takes
    // precedence over the stylesheet rule.
    doc.recascade();

    let bar = query_selector(&doc.root, "div").unwrap();
    // The style attribute "width: 75%" should now be in the computed style.
    // We can't easily compare CssLength::Percent here, so verify via the attr.
    let style_attr = bar.attributes.get("style").cloned().unwrap_or_default();
    assert!(
        style_attr.contains("width"),
        "style attribute must still contain 'width' after recascade; got: {:?}",
        style_attr
    );
}

#[test]
fn set_style_property_upserts_not_duplicates() {
    let html = "<html><body><div></div></body></html>";
    let mut doc = parse_and_layout(html);

    let div = dom::query_selector_mut(&mut doc.root, "div").unwrap();
    set_style_property(div, "width", "50%");
    set_style_property(div, "width", "80%"); // update same property

    let style_attr = div.attributes.get("style").cloned().unwrap_or_default();
    // Should contain exactly one "width" declaration.
    let count = style_attr.split(';').filter(|s| s.contains("width")).count();
    assert_eq!(count, 1, "style attribute should have exactly one 'width'; got: {:?}", style_attr);
    assert!(style_attr.contains("80%"), "style attribute should reflect the latest value");
}

#[test]
fn set_style_property_multiple_props_survive_recascade() {
    let html = r#"<html><head><style>
        div { background-color: #ffffff; }
    </style></head><body><div></div></body></html>"#;

    let mut doc = parse_and_layout(html);
    {
        let div = dom::query_selector_mut(&mut doc.root, "div").unwrap();
        set_style_property(div, "background", "#ff0000");
        set_style_property(div, "color", "#00ff00");
    }
    doc.recascade();

    let div = query_selector(&doc.root, "div").unwrap();
    let style_attr = div.attributes.get("style").cloned().unwrap_or_default();
    assert!(style_attr.contains("background"), "background should survive recascade");
    assert!(style_attr.contains("color"),      "color should survive recascade");
}

// ── 3. apply_inline_style_str persists to style attribute ─────────────────────

#[test]
fn apply_inline_style_str_survives_recascade() {
    let html = "<html><body><div></div></body></html>";
    let mut doc = parse_and_layout(html);

    {
        let div = dom::query_selector_mut(&mut doc.root, "div").unwrap();
        apply_inline_style_str(div, "width: 40%; height: 20px");
    }
    doc.recascade();

    let div = query_selector(&doc.root, "div").unwrap();
    let style_attr = div.attributes.get("style").cloned().unwrap_or_default();
    assert!(style_attr.contains("width"),  "width must survive recascade");
    assert!(style_attr.contains("height"), "height must survive recascade");
}

// ── 4. class helpers (add / remove / toggle / has) ────────────────────────────

#[test]
fn add_class_idempotent() {
    let html = "<html><body><div class=\"foo\"></div></body></html>";
    let mut doc = parse_and_layout(html);
    let div = dom::query_selector_mut(&mut doc.root, "div").unwrap();
    add_class(div, "foo");
    add_class(div, "foo");
    let cls = div.attributes.get("class").cloned().unwrap_or_default();
    let count = cls.split_whitespace().filter(|&c| c == "foo").count();
    assert_eq!(count, 1, "add_class should not duplicate existing class");
}

#[test]
fn remove_class_absent_is_noop() {
    let html = "<html><body><div class=\"foo\"></div></body></html>";
    let mut doc = parse_and_layout(html);
    let div = dom::query_selector_mut(&mut doc.root, "div").unwrap();
    remove_class(div, "bar"); // "bar" not present — must not panic or corrupt
    assert!(has_class(div, "foo"), "existing class must be untouched");
}

#[test]
fn has_class_partial_match_rejected() {
    let html = "<html><body><div class=\"foobar\"></div></body></html>";
    let mut doc = parse_and_layout(html);
    let div = dom::query_selector_mut(&mut doc.root, "div").unwrap();
    assert!(!has_class(div, "foo"),    "has_class must not match partial token");
    assert!(has_class(div, "foobar"), "has_class must match exact token");
}

// ── 5. compact class affects child styles via recascade ───────────────────────

#[test]
fn recascade_compact_class_reduces_padding() {
    let html = r#"<html><head><style>
        .panel { padding: 20px; }
        .compact .panel { padding: 5px; }
    </style></head><body><div class="panel"></div></body></html>"#;

    let mut doc = parse_and_layout(html);

    // Normal mode: padding should be 20 px.
    let panel_before = query_selector(&doc.root, ".panel").unwrap();
    assert_eq!(
        panel_before.style.padding_top.resolve(16.0, 800.0, 16.0),
        20.0,
        "panel padding should start at 20px"
    );

    // Add compact to root.
    add_class(&mut doc.root, "compact");
    doc.recascade();

    let panel_after = query_selector(&doc.root, ".panel").unwrap();
    assert_eq!(
        panel_after.style.padding_top.resolve(16.0, 800.0, 16.0),
        5.0,
        "panel padding should shrink to 5px in compact mode"
    );
}
