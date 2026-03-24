//! Tests for the NodeId-based event dispatch with capture/bubble phases.

use crate::html::parse_html;
use crate::dom::events::{DomEvent, EventPhase, EventTargetMap};
use crate::dom::arena::NodeId;
use std::sync::{Arc, Mutex};

fn setup() -> (crate::types::Document, u32, u32, u32) {
    // <div id="outer"><p id="middle"><span id="inner">text</span></p></div>
    let doc = parse_html(r#"<div id="outer"><p id="middle"><span id="inner">click me</span></p></div>"#);
    let outer = doc.get_element_by_id("outer").unwrap();
    let middle = doc.get_element_by_id("middle").unwrap();
    let inner = doc.get_element_by_id("inner").unwrap();
    (doc, outer, middle, inner)
}

#[test]
fn bubble_fires_target_then_ancestors() {
    let (doc, outer, middle, inner) = setup();
    let log: Arc<Mutex<Vec<(u32, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let mut targets = EventTargetMap::new();

    // Register bubble listeners on all three
    let log1 = log.clone();
    targets.add_event_listener(inner, "click", Box::new(move |e| {
        log1.lock().unwrap().push((e.current_target, "inner".into()));
    }), false);

    let log2 = log.clone();
    targets.add_event_listener(middle, "click", Box::new(move |e| {
        log2.lock().unwrap().push((e.current_target, "middle".into()));
    }), false);

    let log3 = log.clone();
    targets.add_event_listener(outer, "click", Box::new(move |e| {
        log3.lock().unwrap().push((e.current_target, "outer".into()));
    }), false);

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);

    let result = log.lock().unwrap();
    assert_eq!(result.len(), 3, "all three should fire");
    assert_eq!(result[0].1, "inner", "target fires first");
    assert_eq!(result[1].1, "middle", "parent fires second");
    assert_eq!(result[2].1, "outer", "grandparent fires third");
}

#[test]
fn capture_fires_before_bubble() {
    let (doc, outer, _middle, inner) = setup();
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut targets = EventTargetMap::new();

    // Capture listener on outer
    let log1 = log.clone();
    targets.add_event_listener(outer, "click", Box::new(move |_| {
        log1.lock().unwrap().push("outer-capture".into());
    }), true);

    // Bubble listener on outer
    let log2 = log.clone();
    targets.add_event_listener(outer, "click", Box::new(move |_| {
        log2.lock().unwrap().push("outer-bubble".into());
    }), false);

    // Bubble listener on inner
    let log3 = log.clone();
    targets.add_event_listener(inner, "click", Box::new(move |_| {
        log3.lock().unwrap().push("inner-bubble".into());
    }), false);

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);

    let result = log.lock().unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], "outer-capture", "capture should fire first");
    assert_eq!(result[1], "inner-bubble", "target fires after capture");
    assert_eq!(result[2], "outer-bubble", "bubble fires last");
}

#[test]
fn stop_propagation_stops_bubble() {
    let (doc, outer, middle, inner) = setup();
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut targets = EventTargetMap::new();

    let log1 = log.clone();
    targets.add_event_listener(inner, "click", Box::new(move |_| {
        log1.lock().unwrap().push("inner".into());
    }), false);

    let log2 = log.clone();
    targets.add_event_listener(middle, "click", Box::new(move |e| {
        log2.lock().unwrap().push("middle".into());
        e.stop_propagation();
    }), false);

    let log3 = log.clone();
    targets.add_event_listener(outer, "click", Box::new(move |_| {
        log3.lock().unwrap().push("outer".into());
    }), false);

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);

    let result = log.lock().unwrap();
    assert_eq!(result.len(), 2, "outer should NOT fire");
    assert_eq!(result[0], "inner");
    assert_eq!(result[1], "middle");
}

#[test]
fn prevent_default_is_readable() {
    let (doc, _outer, _middle, inner) = setup();
    let mut targets = EventTargetMap::new();

    targets.add_event_listener(inner, "click", Box::new(|e| {
        e.prevent_default();
    }), false);

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);

    assert!(event.default_prevented(), "preventDefault should be set");
}

#[test]
fn event_phase_is_correct() {
    let (doc, outer, _middle, inner) = setup();
    let phases: Arc<Mutex<Vec<EventPhase>>> = Arc::new(Mutex::new(Vec::new()));
    let mut targets = EventTargetMap::new();

    let p1 = phases.clone();
    targets.add_event_listener(outer, "click", Box::new(move |e| {
        p1.lock().unwrap().push(e.phase);
    }), true); // capture

    let p2 = phases.clone();
    targets.add_event_listener(inner, "click", Box::new(move |e| {
        p2.lock().unwrap().push(e.phase);
    }), false); // target/bubble

    let p3 = phases.clone();
    targets.add_event_listener(outer, "click", Box::new(move |e| {
        p3.lock().unwrap().push(e.phase);
    }), false); // bubble

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);

    let result = phases.lock().unwrap();
    assert_eq!(result[0], EventPhase::Capture);
    assert_eq!(result[1], EventPhase::Target);
    assert_eq!(result[2], EventPhase::Bubble);
}

#[test]
fn remove_listener_works() {
    let (doc, _outer, _middle, inner) = setup();
    let count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let mut targets = EventTargetMap::new();

    let c = count.clone();
    let id = targets.add_event_listener(inner, "click", Box::new(move |_| {
        *c.lock().unwrap() += 1;
    }), false);

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);
    assert_eq!(*count.lock().unwrap(), 1);

    targets.remove_event_listener(id);

    let mut event2 = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event2);
    assert_eq!(*count.lock().unwrap(), 1, "should not fire after removal");
}

#[test]
fn different_event_types_dont_interfere() {
    let (doc, _outer, _middle, inner) = setup();
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut targets = EventTargetMap::new();

    let l1 = log.clone();
    targets.add_event_listener(inner, "click", Box::new(move |_| {
        l1.lock().unwrap().push("click".into());
    }), false);

    let l2 = log.clone();
    targets.add_event_listener(inner, "mousedown", Box::new(move |_| {
        l2.lock().unwrap().push("mousedown".into());
    }), false);

    let mut event = DomEvent::new("click", inner);
    targets.dispatch_event(&doc.arena, &mut event);

    let result = log.lock().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "click");
}

#[test]
fn frame_on_off_integration() {
    let doc = parse_html(r#"<button id="btn">Click</button>"#);
    let mut frame = crate::frame::EngineFrame::new(doc, 800.0, 600.0);
    frame.update_frame();

    let btn = frame.doc.get_element_by_id("btn").unwrap();
    let clicked = Arc::new(Mutex::new(false));
    let c = clicked.clone();

    let id = frame.on(btn, "click", Box::new(move |_| {
        *c.lock().unwrap() = true;
    }));

    let mut event = DomEvent::new("click", btn);
    frame.dispatch_event(&mut event);
    assert!(*clicked.lock().unwrap(), "click handler should fire");

    // Remove and verify
    *clicked.lock().unwrap() = false;
    frame.off(id);
    let mut event2 = DomEvent::new("click", btn);
    frame.dispatch_event(&mut event2);
    assert!(!*clicked.lock().unwrap(), "should not fire after off()");
}
