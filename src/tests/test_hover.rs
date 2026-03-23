use crate::{parse_html, Document};
use crate::types::*;
use crate::layout::LayoutEngine;

fn layout_html(html: &str, width: f32) -> Document {
    let mut doc = parse_html(html);
    let mut eng = LayoutEngine::new();
    eng.viewport_h = 900.0;
    eng.layout(&mut doc, width);
    doc
}

fn find_by_id<'a>(node: &'a HtmlBox, id: &str) -> Option<&'a HtmlBox> {
    if node.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(node); }
    for child in &node.children { if let Some(n) = find_by_id(child, id) { return Some(n); } }
    None
}

/// Simulate hovering over element with given id: set hovered_box, mark changed, re-layout.
fn recascade_with_hover(doc: &mut Document, hovered_id: &str) {
    let hovered_ptr = find_by_id(&doc.root, hovered_id)
        .map(|n| n as *const HtmlBox)
        .unwrap_or(std::ptr::null());
    doc.hovered_box = hovered_ptr;
    doc.hover_changed = true;
    let mut eng = LayoutEngine::new();
    eng.viewport_h = 900.0;
    // Force cascade by setting last_cascade_vw to NaN (new engine)
    eng.layout(doc, 800.0);
}

// ── Basic self-hover ──────────────────────────────────────────────────────

#[test]
fn hover_self_changes_color() {
    let mut doc = layout_html(r#"
        <style>
            #btn { color: black; }
            #btn:hover { color: red; }
        </style>
        <div id="btn">Click me</div>
    "#, 800.0);

    // Before hover: color should be black
    let btn = find_by_id(&doc.root, "btn").unwrap();
    assert_eq!(btn.style.color, Color::rgb(0, 0, 0), "before hover: black");

    // After hovering #btn
    recascade_with_hover(&mut doc, "btn");
    let btn = find_by_id(&doc.root, "btn").unwrap();
    assert_eq!(btn.style.color, Color::rgb(255, 0, 0), "after hover: red");
}

// ── Parent:hover child — the CSS dropdown pattern ────────────────────────

#[test]
fn parent_hover_reveals_child_display_block() {
    let mut doc = layout_html(r#"
        <style>
            .dropdown { display: none; }
            .menu:hover .dropdown { display: block; }
        </style>
        <div class="menu" id="menu">
            <span>Menu Label</span>
            <div class="dropdown" id="dropdown">
                <p>Item 1</p>
                <p>Item 2</p>
            </div>
        </div>
    "#, 800.0);

    // Before hover: dropdown should be display:none (no height)
    let dropdown = find_by_id(&doc.root, "dropdown").unwrap();
    assert_eq!(dropdown.style.display, Display::None,
        "before hover: dropdown should be display:none");

    // Hover on menu (parent) — dropdown should become display:block
    recascade_with_hover(&mut doc, "menu");
    let dropdown = find_by_id(&doc.root, "dropdown").unwrap();
    assert_ne!(dropdown.style.display, Display::None,
        "after hovering parent: dropdown should be display:block");
}

#[test]
fn parent_hover_child_max_height() {
    let mut doc = layout_html(r#"
        <style>
            .sub { max-height: 0; overflow: hidden; }
            .nav:hover > .sub { max-height: 500px; }
        </style>
        <div class="nav" id="nav">
            <span>Nav Item</span>
            <ul class="sub" id="sub">
                <li>Sub 1</li>
                <li>Sub 2</li>
            </ul>
        </div>
    "#, 800.0);

    // Before hover: sub should be constrained to 0 height
    let sub = find_by_id(&doc.root, "sub").unwrap();
    assert!(sub.content_rect.h < 1.0,
        "before hover: sub should have ~0 height, got {}", sub.content_rect.h);

    // Hover on nav (parent) — sub should expand
    recascade_with_hover(&mut doc, "nav");
    let sub = find_by_id(&doc.root, "sub").unwrap();
    assert!(sub.content_rect.h > 10.0,
        "after hovering parent: sub should have visible height, got {}", sub.content_rect.h);
}

// ── Hover on sibling content within parent ──────────────────────────────

#[test]
fn hover_on_sibling_within_parent_activates_child() {
    let mut doc = layout_html(r#"
        <style>
            .dropdown { display: none; }
            .menu:hover .dropdown { display: block; }
        </style>
        <div class="menu" id="menu">
            <span id="label">Menu Label</span>
            <div class="dropdown" id="dropdown">Content</div>
        </div>
    "#, 800.0);

    // Hover on the label (child of .menu, sibling of .dropdown)
    // .menu:hover should match because .menu contains the hovered label
    recascade_with_hover(&mut doc, "label");
    let dropdown = find_by_id(&doc.root, "dropdown").unwrap();
    assert_ne!(dropdown.style.display, Display::None,
        "hovering sibling label should activate parent:hover child rule");
}

// ── Hover outside parent does NOT activate child ────────────────────────

#[test]
fn hover_outside_parent_does_not_activate_child() {
    let mut doc = layout_html(r#"
        <style>
            .dropdown { display: none; }
            .menu:hover .dropdown { display: block; }
        </style>
        <div class="menu" id="menu">
            <span>Menu Label</span>
            <div class="dropdown" id="dropdown">Content</div>
        </div>
        <div id="outside">Other content</div>
    "#, 800.0);

    // Hover on element outside .menu
    recascade_with_hover(&mut doc, "outside");
    let dropdown = find_by_id(&doc.root, "dropdown").unwrap();
    assert_eq!(dropdown.style.display, Display::None,
        "hovering outside parent should NOT activate dropdown");
}

// ── Multiple dropdown menus — only hovered one opens ────────────────────

#[test]
fn only_hovered_menu_opens() {
    let mut doc = layout_html(r#"
        <style>
            .sub { display: none; }
            .item:hover > .sub { display: block; }
        </style>
        <nav>
            <div class="item" id="item1">
                <span>File</span>
                <div class="sub" id="sub1">New, Open, Save</div>
            </div>
            <div class="item" id="item2">
                <span>Edit</span>
                <div class="sub" id="sub2">Cut, Copy, Paste</div>
            </div>
        </nav>
    "#, 800.0);

    // Hover on item1
    recascade_with_hover(&mut doc, "item1");
    let sub1 = find_by_id(&doc.root, "sub1").unwrap();
    let sub2 = find_by_id(&doc.root, "sub2").unwrap();
    assert_ne!(sub1.style.display, Display::None, "hovered menu should open");
    assert_eq!(sub2.style.display, Display::None, "other menu should stay closed");
}

// ── Nested hover (grandparent:hover grandchild) ────────────────────────

#[test]
fn grandparent_hover_affects_grandchild() {
    let mut doc = layout_html(r#"
        <style>
            .tooltip { visibility: hidden; }
            .container:hover .tooltip { visibility: visible; }
        </style>
        <div class="container" id="container">
            <div class="wrapper">
                <span class="tooltip" id="tip">Tooltip text</span>
            </div>
        </div>
    "#, 800.0);

    recascade_with_hover(&mut doc, "container");
    let tip = find_by_id(&doc.root, "tip").unwrap();
    assert!(tip.style.visibility,
        "grandparent hover should make grandchild visible");
}
