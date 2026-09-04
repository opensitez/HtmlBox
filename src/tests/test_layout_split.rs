//! Tests for the DOM/Layout split — verifying LayoutBox and LayoutStore work
//! correctly alongside the existing WebCore tree.

use crate::frame::EngineFrame;
use crate::html::parse_html;
use crate::layout::layout_box::{LayoutBox, LayoutStore};

#[test]
fn layout_store_basic_operations() {
    let mut store = LayoutStore::new();
    assert_eq!(store.len(), 0);

    let lb = store.get_or_create(42);
    lb.layout.content_rect.w = 100.0;
    lb.layout.content_rect.h = 50.0;
    assert_eq!(store.len(), 1);

    let lb = store.get(42).unwrap();
    assert_eq!(lb.layout.content_rect.w, 100.0);
    assert_eq!(lb.node_id, 42);

    store.remove(42);
    assert!(!store.contains(42));
    assert_eq!(store.len(), 0);
}

#[test]
fn layout_store_on_document() {
    let doc = parse_html("<div>hello</div>");
    // Document should have an empty layout store initially
    assert_eq!(doc.layout_store.len(), 0);
}

#[test]
fn layout_box_has_correct_defaults() {
    let lb = LayoutBox::new(7);
    assert_eq!(lb.node_id, 7);
    assert_eq!(lb.layout.content_rect.w, 0.0);
    assert_eq!(lb.layout.content_rect.h, 0.0);
    assert!(lb.layout.layout_dirty);
    assert!(lb.layout.cached_intrinsic_w.get().is_nan());
}

#[test]
fn layout_store_get_or_create_is_idempotent() {
    let mut store = LayoutStore::new();
    store.get_or_create(10).layout.content_rect.w = 200.0;
    store.get_or_create(10).layout.content_rect.h = 100.0;

    // Should still be the same box
    let lb = store.get(10).unwrap();
    assert_eq!(lb.layout.content_rect.w, 200.0);
    assert_eq!(lb.layout.content_rect.h, 100.0);
    assert_eq!(store.len(), 1);
}

#[test]
fn document_has_all_architecture_pieces() {
    // Verify the Document struct has all the new architectural components
    let mut doc = parse_html("<div><p>text</p></div>");

    // Arena
    assert!(doc.arena.len() > 1, "arena should have nodes");

    // Node map
    doc.rebuild_node_map();
    assert!(!doc.node_index.is_empty(), "node_map should be populated");

    // Layout store
    assert_eq!(doc.layout_store.len(), 0, "layout store starts empty");

    // Event targets
    assert!(doc.event_targets.is_empty(), "no listeners initially");

    // Pending nodes
    assert!(doc.pending_nodes.is_empty(), "no pending nodes initially");

    // Next node ID is > 0
    assert!(doc.next_node_id > 0, "next_node_id should be assigned");
}
