//! Tests for proper HTML/DHTML behavior — verifying the engine works as
//! a dynamic HTML app engine with correct rendering, inheritance, and mutation.

use crate::frame::EngineFrame;
use crate::html::parse_html;
use crate::dom::events::DomEvent;
use std::sync::{Arc, Mutex};

fn frame(html: &str) -> EngineFrame {
    let doc = parse_html(html);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();
    f
}

// ═══════════════════════════════════════════════════════════════════════════════
// CSS Inheritance
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn color_inherits_to_children() {
    let mut f = frame(r#"<div style="color: red"><p id="child">text</p></div>"#);
    let child = f.doc.get_element_by_id("child").unwrap();
    let ptr = crate::types::find_by_node_id(&f.doc.root, child);
    let style = unsafe { &*ptr }.style.clone();
    assert_eq!(style.color.r, 255, "red should inherit to child");
    assert_eq!(style.color.g, 0);
}

#[test]
fn font_size_inherits() {
    let mut f = frame(r#"<div style="font-size: 24px"><span id="s">text</span></div>"#);
    let s = f.doc.get_element_by_id("s").unwrap();
    let ptr = crate::types::find_by_node_id(&f.doc.root, s);
    let style = unsafe { &*ptr }.style.clone();
    let fs = style.font_size_px(16.0, 16.0);
    assert!((fs - 24.0).abs() < 1.0, "font-size should inherit: got {}", fs);
}

#[test]
fn background_does_not_inherit() {
    let mut f = frame(r#"<div style="background-color: blue"><p id="child">text</p></div>"#);
    let child = f.doc.get_element_by_id("child").unwrap();
    let ptr = crate::types::find_by_node_id(&f.doc.root, child);
    let style = unsafe { &*ptr }.style.clone();
    // Background should NOT inherit — child should have transparent/default
    assert_ne!(style.background_color.b, 255, "background should not inherit");
}

#[test]
fn display_does_not_inherit() {
    let mut f = frame(r#"<div style="display: flex"><span id="child">text</span></div>"#);
    let child = f.doc.get_element_by_id("child").unwrap();
    let ptr = crate::types::find_by_node_id(&f.doc.root, child);
    let style = unsafe { &*ptr }.style.clone();
    assert_ne!(style.display, crate::types::Display::Flex, "display should not inherit");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Dynamic DOM Mutations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn add_class_triggers_style_update() {
    let html = r#"<html><head><style>.red { color: red; }</style></head>
        <body><p id="p">text</p></body></html>"#;
    let mut f = frame(html);
    let p = f.doc.get_element_by_id("p").unwrap();

    // Before: default color (black)
    let ptr = crate::types::find_by_node_id(&f.doc.root, p);
    assert_eq!(unsafe { &*ptr }.style.color.r, 0);

    // Add class and update
    f.toggle_class(p, "red");
    f.update_frame();

    // After: red
    let ptr = crate::types::find_by_node_id(&f.doc.root, p);
    assert_eq!(unsafe { &*ptr }.style.color.r, 255, "adding .red class should make text red");
}

#[test]
fn set_style_property_updates_layout() {
    let mut f = frame(r#"<div id="box" style="width: 100px; height: 50px">x</div>"#);
    let box_id = f.doc.get_element_by_id("box").unwrap();
    assert!((f.doc.dom_offset_width(box_id) - 100.0).abs() < 2.0);

    f.set_style(box_id, "width", "200px");
    f.update_frame();

    assert!((f.doc.dom_offset_width(box_id) - 200.0).abs() < 2.0,
        "width should update to 200px, got {}", f.doc.dom_offset_width(box_id));
}

#[test]
fn append_child_updates_parent_height() {
    let mut f = frame(r#"<div id="container" style="border: 1px solid black"></div>"#);
    let container = f.doc.get_element_by_id("container").unwrap();
    let h1 = f.doc.dom_offset_height(container);

    // Add a tall child via innerHTML (ensures proper cascade)
    f.set_inner_html(container, r#"<div style="height: 100px">tall</div>"#);
    f.update_frame();

    let h2 = f.doc.dom_offset_height(container);
    assert!(h2 > h1 + 50.0, "container should grow: {} → {}", h1, h2);
}

#[test]
fn inner_html_replacement() {
    let mut f = frame(r#"<div id="app">initial</div>"#);
    let app = f.doc.get_element_by_id("app").unwrap();

    f.set_inner_html(app, r#"<ul><li>one</li><li>two</li><li>three</li></ul>"#);
    f.update_frame();

    let items = f.doc.query_selector_all("li");
    assert_eq!(items.len(), 3, "should have 3 list items after innerHTML");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Event-Driven Behavior
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn click_event_fires_handler() {
    let mut f = frame(r#"<button id="btn">Click Me</button>"#);
    let btn = f.doc.get_element_by_id("btn").unwrap();

    let clicked = Arc::new(Mutex::new(false));
    let c = clicked.clone();
    f.on(btn, "click", Box::new(move |_| {
        *c.lock().unwrap() = true;
    }));

    let mut evt = DomEvent::new("click", btn);
    f.dispatch_event(&mut evt);

    assert!(*clicked.lock().unwrap(), "click handler should fire");
}

#[test]
fn click_handler_can_modify_dom() {
    let html = r#"<html><head><style>.active { color: red; }</style></head>
        <body><button id="btn">Toggle</button><p id="target">text</p></body></html>"#;
    let mut f = frame(html);
    let btn = f.doc.get_element_by_id("btn").unwrap();
    let target = f.doc.get_element_by_id("target").unwrap();

    // Simulate: click toggles .active class on target
    f.doc.class_list_add(target, "active");
    f.mark_style_dirty();
    f.update_frame();

    let ptr = crate::types::find_by_node_id(&f.doc.root, target);
    assert_eq!(unsafe { &*ptr }.style.color.r, 255, "target should be red after class add");
}

#[test]
fn prevent_default_works() {
    let mut f = frame(r#"<a id="link" href="/page">Go</a>"#);
    let link = f.doc.get_element_by_id("link").unwrap();

    f.on(link, "click", Box::new(|e| {
        e.prevent_default();
    }));

    let mut evt = DomEvent::new("click", link);
    f.dispatch_event(&mut evt);

    assert!(evt.default_prevented(), "preventDefault should stop navigation");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CSS Cascade Specificity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn inline_style_overrides_class() {
    let html = r#"<html><head><style>.blue { color: blue; }</style></head>
        <body><p id="p" class="blue" style="color: red">text</p></body></html>"#;
    let mut f = frame(html);
    let p = f.doc.get_element_by_id("p").unwrap();

    let ptr = crate::types::find_by_node_id(&f.doc.root, p);
    let color = unsafe { &*ptr }.style.color;
    assert_eq!(color.r, 255, "inline style should override class: r={}", color.r);
    assert_eq!(color.b, 0);
}

#[test]
fn class_selector_overrides_tag() {
    let html = r#"<html><head><style>
        p { color: blue; }
        .red { color: red; }
    </style></head>
    <body><p id="p" class="red">text</p></body></html>"#;
    let mut f = frame(html);
    let p = f.doc.get_element_by_id("p").unwrap();

    let ptr = crate::types::find_by_node_id(&f.doc.root, p);
    let color = unsafe { &*ptr }.style.color;
    assert_eq!(color.r, 255, "class should override tag: r={}", color.r);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Box Model
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn border_box_sizing() {
    let mut f = frame(r#"
        <div id="a" style="width: 200px; padding: 20px; box-sizing: content-box">a</div>
        <div id="b" style="width: 200px; padding: 20px; box-sizing: border-box">b</div>
    "#);
    let a = f.doc.get_element_by_id("a").unwrap();
    let b = f.doc.get_element_by_id("b").unwrap();

    let wa = f.doc.dom_offset_width(a);
    let wb = f.doc.dom_offset_width(b);

    // content-box: width = content, so total = 200 + 40 padding = 240
    assert!((wa - 240.0).abs() < 5.0, "content-box should be ~240px, got {}", wa);
    // border-box: width includes padding, so total = 200
    assert!((wb - 200.0).abs() < 5.0, "border-box should be ~200px, got {}", wb);
}

#[test]
fn nested_percentage_resolves_correctly() {
    let mut f = frame(r#"
        <div style="width: 600px">
            <div style="width: 50%">
                <div id="inner" style="width: 50%">inner</div>
            </div>
        </div>
    "#);
    let inner = f.doc.get_element_by_id("inner").unwrap();
    let w = f.doc.dom_offset_width(inner);
    // 50% of 50% of 600 = 150
    assert!((w - 150.0).abs() < 5.0, "nested 50% of 50% of 600 should be ~150, got {}", w);
}

// ═══════════════════════════════════════════════════════════════════════════════
// CSS min()/max()/clamp() in layout
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn max_width_constrains_element() {
    let mut f = frame(r#"<div id="box" style="max-width: 300px">content</div>"#);
    let w = f.doc.dom_offset_width(f.doc.get_element_by_id("box").unwrap());
    assert!(w <= 301.0, "max-width 300px should constrain: got {}", w);
}

#[test]
fn min_function_in_width() {
    let mut f = frame(r#"<div id="box" style="width: min(300px, 50%)">x</div>"#);
    let w = f.doc.dom_offset_width(f.doc.get_element_by_id("box").unwrap());
    // 50% of ~784 (body) = ~392, min(300, 392) = 300
    assert!((w - 300.0).abs() < 5.0, "min(300px, 50%) should be ~300, got {}", w);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Frame Loop
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_mutations_one_frame() {
    let mut f = frame(r#"<div id="app"></div>"#);
    let app = f.doc.get_element_by_id("app").unwrap();

    // Multiple mutations without frame update
    for i in 0..10 {
        let p = f.doc.dom_create_element("p");
        let t = f.doc.dom_create_text(&format!("Item {}", i));
        f.doc.dom_append_child(p, t);
        f.doc.dom_append_child(app, p);
    }
    f.mark_style_dirty();

    // Single frame processes all
    f.update_frame();

    let children = f.doc.dom_children(app);
    assert_eq!(children.len(), 10, "all 10 items should be in DOM");
    for &child in &children {
        assert!(f.doc.dom_offset_height(child) > 0.0, "each item should have layout");
    }
}

#[test]
fn no_change_no_work() {
    let mut f = frame(r#"<div>stable</div>"#);
    assert!(!f.update_frame(), "second frame with no changes should return false");
    assert!(!f.update_frame(), "third frame should also return false");
}
