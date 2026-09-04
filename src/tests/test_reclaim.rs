//! What `retained_ids` must keep alive, one root source at a time.
//!
//! ⛔ Each test here holds a detached node through exactly ONE store and
//! asserts it is not offered as a candidate. That is the only way an omission
//! shows: a root source left out of `retained_ids` breaks nothing visible —
//! the node is simply offered up for freeing, and whoever acts on that offer
//! is the one who crashes.
//!
//! ⛔ It cannot catch a store added LATER. Rust gives no way to enumerate a
//! struct's fields, so a new id-keyed map on `Document` is a silent hole until
//! someone adds its test. Stated rather than hoped: if you add one, add a case.

use crate::types::Document;

fn doc() -> Document {
    crate::html::parse_html("<div id=host>text</div>")
}

/// A node that is in the tree is never a candidate.
#[test]
fn a_node_in_the_tree_is_never_a_candidate() {
    let d = doc();
    let host = d.get_element_by_id("host").unwrap();
    assert!(
        !d.detached_candidates().contains(&host),
        "it is in the document"
    );
}

/// …and one that is detached and held by nothing IS one.
#[test]
fn a_node_nothing_holds_is_a_candidate() {
    let mut d = doc();
    let orphan = d.create_element("i");
    // `create_element` parks it in `pending_nodes`, which IS a root — the
    // document is holding it for insertion. Drop that and nothing names it.
    d.pending_nodes.remove(&orphan);
    assert!(
        d.detached_candidates().contains(&orphan),
        "nothing names it, so it is offered"
    );
}

/// A node created and not yet inserted is held by the document itself.
#[test]
fn pending_nodes_holds_an_uninserted_node() {
    let mut d = doc();
    let made = d.create_element("i");
    assert!(
        !d.detached_candidates().contains(&made),
        "pending_nodes names it"
    );
}

/// …and everything under it, not just the node itself.
#[test]
fn pending_nodes_holds_a_whole_detached_subtree() {
    let mut d = doc();
    let outer = d.create_element("div");
    let inner = d.create_element("span");
    d.append_child(outer, inner);
    let cands = d.detached_candidates();
    assert!(!cands.contains(&outer), "the subtree root");
    assert!(!cands.contains(&inner), "and its child");
}

/// A removed node stays held — `removeChild` hands it back and the caller may
/// re-insert it, which the memory suite pins.
#[test]
fn a_removed_node_is_still_held() {
    let mut d = doc();
    let host = d.get_element_by_id("host").unwrap();
    let text = d.child_nodes(host)[0];
    d.remove_child(text);
    assert!(
        !d.detached_candidates().contains(&text),
        "still re-insertable"
    );
}

fn detached_but_for(f: impl FnOnce(&mut Document, u32)) -> (Vec<u32>, u32) {
    let mut d = doc();
    let node = d.create_element("i");
    d.pending_nodes.remove(&node); // nothing holds it now…
    f(&mut d, node); //                …except what the case under test sets up
    (d.detached_candidates(), node)
}

#[test]
fn focus_holds_a_node() {
    let (c, n) = detached_but_for(|d, n| d.focused_box = n);
    assert!(!c.contains(&n));
}

#[test]
fn hover_holds_a_node() {
    let (c, n) = detached_but_for(|d, n| d.hovered_box = n);
    assert!(!c.contains(&n));
}

#[test]
fn the_top_layer_holds_a_node() {
    let (c, n) = detached_but_for(|d, n| d.top_layer.push(n));
    assert!(!c.contains(&n));
}

#[test]
fn custom_validity_holds_a_node() {
    let (c, n) = detached_but_for(|d, n| {
        d.custom_validity.insert(n, "bad".into());
    });
    assert!(!c.contains(&n));
}

#[test]
fn an_event_listener_holds_its_node() {
    let (c, n) = detached_but_for(|d, n| {
        d.add_event_listener(n, "click", Box::new(|_, _| {}), Default::default());
    });
    assert!(!c.contains(&n), "a listener is a reference to the node");
}

/// ⛔ A range's store is keyed by RANGE id. Reading its keys would miss the
/// containers entirely and offer up range handles as though they were nodes.
#[test]
fn a_range_holds_its_containers() {
    let (c, n) = detached_but_for(|d, n| {
        let r = d.create_range();
        d.range_set_start(r, n, 0);
    });
    assert!(
        !c.contains(&n),
        "a range end-point is a reference to the node"
    );
}

/// ⛔ Same shape as a range: keyed by TRAVERSAL id, node ids inside the value.
#[test]
fn a_traversal_holds_its_root() {
    let (c, n) = detached_but_for(|d, n| {
        d.create_tree_walker(n, 0xFFFF_FFFF, None);
    });
    assert!(
        !c.contains(&n),
        "a walker's root is a reference to the node"
    );
}

#[test]
fn the_caret_holds_its_node() {
    let (c, n) = detached_but_for(|d, n| d.editor.caret_box = Some(n));
    assert!(!c.contains(&n));
}

/// The whole point of the API: it offers, it does not free.
#[test]
fn enumerating_candidates_frees_nothing() {
    let mut d = doc();
    let orphan = d.create_element("i");
    d.pending_nodes.remove(&orphan);
    let before = d.arena.len();
    let cands = d.detached_candidates();
    assert!(cands.contains(&orphan));
    assert_eq!(
        d.arena.len(),
        before,
        "enumeration must not mutate the arena"
    );
    assert!(
        d.arena.is_alive(crate::dom::arena::NodeId(orphan)),
        "a candidate is still a live node — freeing is the caller's call"
    );
}

/// ⛔ Shadow nodes are NOT arena nodes, so they can never be candidates — and
/// that is why the two guards that used to be here were vacuous.
///
/// `attachShadow` and the shadow tree take ids from `next_shadow_node_id()`,
/// which counts DOWN from `u32::MAX - 2`, while `detached_candidates` scans
/// `1..arena.len()`. Measured: a fresh shadow root is `4294967293` against an
/// arena length of 6, and `is_alive` is false for every node in it.
///
/// `retained_ids` still records the `ShadowRoot`'s own id — it IS a node in
/// the spec and `attachShadow` hands it to a caller — but that line cannot
/// fire today, and a line no test can falsify is worth saying so about.
///
/// The day shadow nodes join the arena this goes red, which is the right
/// moment to re-read `retained_ids`: its shadow walk becomes load-bearing.
#[test]
fn shadow_nodes_live_outside_the_arena() {
    let mut d = doc();
    let host = d.get_element_by_id("host").unwrap();
    let root = d
        .attach_shadow(host, crate::types::ShadowMode::Open)
        .expect("attachShadow returns the root");
    d.set_shadow_inner_html(host, "<p id=inner>x</p>");

    assert!(
        crate::dom::arena::is_shadow_node_id(root),
        "the shadow root's id comes from the shadow counter, not the arena"
    );
    assert!(
        root >= d.arena.len() as u32,
        "so it is outside the range `detached_candidates` scans"
    );
    let kids = d.shadow_children(host);
    assert!(
        !kids.is_empty(),
        "the shadow tree must have content to test"
    );
    for id in kids {
        assert!(
            !d.arena.is_alive(crate::dom::arena::NodeId(id)),
            "shadow child {id} is not an arena node"
        );
    }
}
