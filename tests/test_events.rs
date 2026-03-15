// Tests for the event system in src/dom/mod.rs.

use rhtmledit::dom::*;
use rhtmledit::types::*;
use rhtmledit::parse_html;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn event_listener_registration() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("#btn", HtmlEventType::Click, Box::new(|_| {}));
    assert!(id > 0);
    assert!(!listeners.is_empty());
    
    listeners.remove(id);
    assert!(listeners.is_empty());
}

#[test]
fn event_dispatch_bubbling() {
    let doc = parse_html(r#"<div id="parent"><p id="child">Click me</p></div>"#);
    let mut listeners = EventListeners::new();
    
    let parent_called = Arc::new(AtomicUsize::new(0));
    let child_called = Arc::new(AtomicUsize::new(0));
    
    let pc = parent_called.clone();
    listeners.add("#parent", HtmlEventType::Click, Box::new(move |_| {
        pc.fetch_add(1, Ordering::SeqCst);
    }));
    
    let cc = child_called.clone();
    listeners.add("#child", HtmlEventType::Click, Box::new(move |_| {
        cc.fetch_add(1, Ordering::SeqCst);
    }));
    
    let child_box = query_selector(&doc.root, "#child").unwrap();
    let mut evt = HtmlEvent::new(HtmlEventType::Click);
    evt.target = child_box as *const HtmlBox;
    
    listeners.dispatch(&doc.root, evt);
    
    assert_eq!(child_called.load(Ordering::SeqCst), 1);
    assert_eq!(parent_called.load(Ordering::SeqCst), 1);
}

#[test]
fn event_stop_propagation() {
    let doc = parse_html(r#"<div id="parent"><p id="child">Click me</p></div>"#);
    let mut listeners = EventListeners::new();
    
    let parent_called = Arc::new(AtomicUsize::new(0));
    let pc = parent_called.clone();
    listeners.add("#parent", HtmlEventType::Click, Box::new(move |_| {
        pc.fetch_add(1, Ordering::SeqCst);
    }));
    
    listeners.add("#child", HtmlEventType::Click, Box::new(|evt| {
        evt.stop_propagation();
    }));
    
    let child_box = query_selector(&doc.root, "#child").unwrap();
    let mut evt = HtmlEvent::new(HtmlEventType::Click);
    evt.target = child_box as *const HtmlBox;
    
    listeners.dispatch(&doc.root, evt);
    
    assert_eq!(parent_called.load(Ordering::SeqCst), 0);
}

#[test]
fn event_prevent_default() {
    let doc = parse_html(r#"<div id="x">Test</div>"#);
    let mut listeners = EventListeners::new();
    
    listeners.add("#x", HtmlEventType::Click, Box::new(|evt| {
        evt.prevent_default();
    }));
    
    let x_box = query_selector(&doc.root, "#x").unwrap();
    let mut evt = HtmlEvent::new(HtmlEventType::Click);
    evt.target = x_box as *const HtmlBox;
    
    let prevented = listeners.dispatch(&doc.root, evt);
    assert!(prevented);
}

#[test]
fn event_selector_matching() {
    let doc = parse_html(r#"<div class="btn">A</div><div class="btn">B</div>"#);
    let mut listeners = EventListeners::new();
    
    let called_with = Arc::new(AtomicUsize::new(0));
    let cw = called_with.clone();
    
    listeners.add(".btn", HtmlEventType::Click, Box::new(move |_| {
        cw.fetch_add(1, Ordering::SeqCst);
    }));
    
    let divs = query_selector_all(&doc.root, ".btn");
    assert_eq!(divs.len(), 2);
    
    for d in divs {
        let mut evt = HtmlEvent::new(HtmlEventType::Click);
        evt.target = d as *const HtmlBox;
        listeners.dispatch(&doc.root, evt);
    }
    
    assert_eq!(called_with.load(Ordering::SeqCst), 2);
}
