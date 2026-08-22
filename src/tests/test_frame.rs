//! Tests for the frame loop (EngineFrame).

use crate::frame::EngineFrame;
use crate::html::parse_html;

fn make_frame(html: &str, w: f32) -> EngineFrame {
    let doc = parse_html(html);
    EngineFrame::new(doc, w, 600.0)
}

// ─── Basic frame lifecycle ──────────────────────────────────────────────────

#[test]
fn initial_frame_runs_cascade_and_layout() {
    let mut frame = make_frame("<div>hello</div>", 800.0);
    // First frame should always need work
    assert!(frame.update_frame(), "initial frame should need redraw");

    // After initial frame, layout should produce valid geometry
    let div = frame.doc.query_selector("div").unwrap();
    assert!(frame.doc.offset_width(div) > 0.0, "div should have width after layout");
}

#[test]
fn no_changes_no_work() {
    let mut frame = make_frame("<div>hello</div>", 800.0);
    frame.update_frame(); // initial

    // Second frame with no changes should not need redraw
    let needs = frame.update_frame();
    assert!(!needs, "frame with no changes should not need redraw");
}

#[test]
fn viewport_resize_triggers_relayout() {
    let mut frame = make_frame("<div>hello</div>", 800.0);
    frame.update_frame(); // initial

    let div = frame.doc.query_selector("div").unwrap();
    let w1 = frame.doc.offset_width(div);

    frame.set_viewport(400.0, 600.0);
    assert!(frame.update_frame(), "resize should trigger redraw");

    let w2 = frame.doc.offset_width(div);
    // Width should change since viewport halved
    assert!(w2 < w1, "div should be narrower after viewport shrink: {} vs {}", w2, w1);
}

// ─── DOM mutation batching ──────────────────────────────────────────────────

#[test]
fn multiple_mutations_single_frame() {
    let mut frame = make_frame(r#"<ul id="list"></ul>"#, 800.0);
    frame.update_frame(); // initial

    let list = frame.doc.get_element_by_id("list").unwrap();

    // Add 5 items — all batched before next frame
    for i in 0..5 {
        let li = frame.doc.create_element("li");
        let text = frame.doc.create_text_node(&format!("Item {}", i));
        frame.doc.append_child(li, text);
        frame.doc.append_child(list, li);
    }
    frame.mark_style_dirty(); // batch: one dirty mark for all mutations

    // Single frame update processes all 5 items
    assert!(frame.update_frame(), "mutations should trigger redraw");

    let children = frame.doc.child_nodes(list);
    assert_eq!(children.len(), 5, "list should have 5 children");

    // All items should have valid layout
    for &child_id in &children {
        assert!(frame.doc.offset_height(child_id) > 0.0,
            "list item should have height after layout");
    }
}

#[test]
fn class_toggle_triggers_restyle() {
    let html = r#"<html><head><style>
        .highlight { background: yellow; }
    </style></head><body>
        <p id="target">Text</p>
    </body></html>"#;
    let mut frame = make_frame(html, 800.0);
    frame.update_frame();

    let p = frame.doc.get_element_by_id("target").unwrap();
    frame.toggle_class(p, "highlight");

    assert!(frame.update_frame(), "class toggle should trigger redraw");
}

#[test]
fn set_inner_html_replaces_content() {
    let mut frame = make_frame(r#"<div id="container">old</div>"#, 800.0);
    frame.update_frame();

    let container = frame.doc.get_element_by_id("container").unwrap();
    frame.set_inner_html(container, "<p>new</p><span>content</span>");

    assert!(frame.update_frame(), "innerHTML should trigger redraw");

    let children = frame.doc.child_nodes(container);
    assert_eq!(children.len(), 2);
    assert_eq!(frame.doc.tag_name(children[0]), Some("p"));
    assert_eq!(frame.doc.tag_name(children[1]), Some("span"));
}

// ─── Style changes ──────────────────────────────────────────────────────────

#[test]
fn inline_style_change_triggers_relayout() {
    let mut frame = make_frame(r#"<div id="box">content</div>"#, 800.0);
    frame.update_frame();

    let div = frame.doc.get_element_by_id("box").unwrap();
    let h1 = frame.doc.offset_height(div);

    frame.set_style(div, "padding", "50px");
    // Verify the attribute was set
    let style_attr = frame.doc.get_attribute(div, "style").unwrap_or_default();
    assert!(style_attr.contains("padding"), "style attr should contain padding: {}", style_attr);
    assert!(frame.update_frame(), "style change should trigger redraw");

    let h2 = frame.doc.offset_height(div);
    // padding: 50px adds 100px total (top + bottom) to the border rect
    assert!(h2 > h1 + 50.0, "padding should increase height significantly: {} → {}", h1, h2);
}

// ─── Proper HTML rendering tests ────────────────────────────────────────────

#[test]
fn block_elements_stack_vertically() {
    let mut frame = make_frame(r#"
        <div id="a" style="height: 50px">A</div>
        <div id="b" style="height: 50px">B</div>
        <div id="c" style="height: 50px">C</div>
    "#, 800.0);
    frame.update_frame();

    let a = frame.doc.get_element_by_id("a").unwrap();
    let b = frame.doc.get_element_by_id("b").unwrap();
    let c = frame.doc.get_element_by_id("c").unwrap();

    let ra = frame.doc.get_bounding_client_rect(a).unwrap();
    let rb = frame.doc.get_bounding_client_rect(b).unwrap();
    let rc = frame.doc.get_bounding_client_rect(c).unwrap();

    assert!(rb.y >= ra.y + ra.h, "B should be below A: B.y={} A.bottom={}", rb.y, ra.y + ra.h);
    assert!(rc.y >= rb.y + rb.h, "C should be below B: C.y={} B.bottom={}", rc.y, rb.y + rb.h);
}

#[test]
fn inline_elements_flow_horizontally() {
    let mut frame = make_frame(r#"
        <span id="a">Hello</span><span id="b">World</span>
    "#, 800.0);
    frame.update_frame();

    let a = frame.doc.get_element_by_id("a").unwrap();
    let b = frame.doc.get_element_by_id("b").unwrap();

    let ra = frame.doc.get_bounding_client_rect(a).unwrap();
    let rb = frame.doc.get_bounding_client_rect(b).unwrap();

    // Inline spans should be on the same line (same y)
    assert!((ra.y - rb.y).abs() < 5.0, "spans should be on same line: a.y={} b.y={}", ra.y, rb.y);
    // B should be to the right of A
    assert!(rb.x >= ra.x, "B should be right of A: b.x={} a.x={}", rb.x, ra.x);
}

#[test]
fn display_none_has_zero_size() {
    let mut frame = make_frame(r#"
        <div id="visible" style="height: 50px">V</div>
        <div id="hidden" style="display: none; height: 50px">H</div>
    "#, 800.0);
    frame.update_frame();

    let hidden = frame.doc.get_element_by_id("hidden").unwrap();
    assert_eq!(frame.doc.offset_width(hidden), 0.0);
    assert_eq!(frame.doc.offset_height(hidden), 0.0);
}

#[test]
fn percentage_width_resolves_to_parent() {
    let mut frame = make_frame(r#"
        <div style="width: 400px">
            <div id="half" style="width: 50%">half</div>
        </div>
    "#, 800.0);
    frame.update_frame();

    let half = frame.doc.get_element_by_id("half").unwrap();
    let w = frame.doc.offset_width(half);
    assert!((w - 200.0).abs() < 2.0, "50% of 400px should be ~200px, got {}", w);
}

#[test]
fn margin_auto_centers_block() {
    let mut frame = make_frame(r#"
        <div id="centered" style="width: 200px; margin: 0 auto">centered</div>
    "#, 800.0);
    frame.update_frame();

    let c = frame.doc.get_element_by_id("centered").unwrap();
    let rect = frame.doc.get_bounding_client_rect(c).unwrap();
    // Should be centered: (800 - 200) / 2 = 300
    assert!((rect.x - 300.0).abs() < 5.0, "margin:auto should center. x={}", rect.x);
}

#[test]
fn flex_row_distributes_children() {
    let mut frame = make_frame(r#"
        <div style="display: flex; width: 300px">
            <div id="a" style="flex: 1">A</div>
            <div id="b" style="flex: 2">B</div>
        </div>
    "#, 800.0);
    frame.update_frame();

    let a = frame.doc.get_element_by_id("a").unwrap();
    let b = frame.doc.get_element_by_id("b").unwrap();
    let wa = frame.doc.offset_width(a);
    let wb = frame.doc.offset_width(b);

    // B should be roughly 2x the width of A
    assert!(wb > wa, "flex:2 should be wider than flex:1: {}px vs {}px", wb, wa);
    // Total should be close to 300px
    assert!((wa + wb - 300.0).abs() < 5.0, "flex children should fill container: {} + {} = {}", wa, wb, wa + wb);
}

#[test]
fn dynamic_dom_mutation_updates_layout() {
    let mut frame = make_frame(r#"<div id="container"></div>"#, 800.0);
    frame.update_frame();

    let container = frame.doc.get_element_by_id("container").unwrap();
    let h1 = frame.doc.offset_height(container);

    // Add content dynamically
    frame.set_inner_html(container, r#"<p style="height: 100px">Big block</p>"#);
    frame.update_frame();

    let h2 = frame.doc.offset_height(container);
    assert!(h2 > h1, "container should grow after adding content: {} → {}", h1, h2);
}

#[test]
fn remove_child_shrinks_layout() {
    let mut frame = make_frame(r#"
        <div id="parent">
            <div id="child" style="height: 100px">big</div>
        </div>
    "#, 800.0);
    frame.update_frame();

    let parent = frame.doc.get_element_by_id("parent").unwrap();
    let child = frame.doc.get_element_by_id("child").unwrap();
    let h1 = frame.doc.offset_height(parent);

    frame.remove_child(child);
    frame.update_frame();

    let h2 = frame.doc.offset_height(parent);
    assert!(h2 < h1, "parent should shrink after removing child: {} → {}", h1, h2);
}
