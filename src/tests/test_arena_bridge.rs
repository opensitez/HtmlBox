//! Tests for the arena-based DOM bridge integration.
//!
//! Verifies that:
//! - Every WebCore gets a unique non-zero node_id during parsing
//! - The arena mirrors the WebCore tree structure
//! - The node_map bridge lookup works correctly
//! - Arena node attributes/text match WebCore data

use crate::html::parse_html;
use crate::dom::arena::NodeId;
use crate::types::{Document, WebCore};

/// The `<body>` box, found by TAG.
///
/// Never `root.children[0]`. The parser synthesises the whole `html > head,
/// body` skeleton (HTML §13.2.6), so body is the *second* child — and any
/// fixed index goes wrong again the moment the tree gains anything else.
/// A test that means "the body" should say so.
fn body_of(doc: &Document) -> &WebCore {
    doc.root
        .children
        .iter()
        .find(|c| c.tag == "body")
        .expect("every parsed document has a body")
}

/// Helper: collect all node_ids from an WebCore tree into a Vec.
fn collect_node_ids(node: &crate::types::WebCore) -> Vec<u32> {
    let mut ids = vec![node.node_id];
    for child in &node.children {
        ids.extend(collect_node_ids(child));
    }
    ids
}

/// Helper: count total nodes in WebCore tree.
fn count_nodes(node: &crate::types::WebCore) -> usize {
    1 + node.children.iter().map(|c| count_nodes(c)).sum::<usize>()
}

#[test]
fn every_node_gets_unique_nonzero_id() {
    let doc = parse_html("<div><p>hello</p><span>world</span></div>");
    let ids = collect_node_ids(&doc.root);

    // All IDs should be non-zero
    for &id in &ids {
        assert!(id != 0, "node_id should not be 0, found 0 on a node");
    }

    // All IDs should be unique
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "duplicate node_ids found: {:?}", ids);
}

#[test]
fn arena_mirrors_tree_structure() {
    let doc = parse_html("<ul><li>one</li><li>two</li><li>three</li></ul>");
    let arena = &doc.arena;

    // Find the <html> node in arena
    let html_id = NodeId(doc.root.node_id);
    assert!(arena.is_alive(html_id), "html arena node should be alive");
    assert_eq!(arena.get(html_id).tag, "html");

    // html > body
    let body = body_of(&doc);
    let body_id = NodeId(body.node_id);
    assert!(arena.is_alive(body_id));
    assert_eq!(arena.get(body_id).tag, "body");
    assert_eq!(arena.get(body_id).parent, html_id);

    // body > ul
    let ul = &body.children[0];
    let ul_id = NodeId(ul.node_id);
    assert!(arena.is_alive(ul_id));
    assert_eq!(arena.get(ul_id).tag, "ul");
    assert_eq!(arena.get(ul_id).parent, body_id);

    // ul has 3 li children in arena
    let arena_children: Vec<NodeId> = arena.children(ul_id).collect();
    assert_eq!(arena_children.len(), 3, "ul should have 3 li children in arena");

    for (i, &li_arena_id) in arena_children.iter().enumerate() {
        assert_eq!(arena.get(li_arena_id).tag, "li");
        assert_eq!(arena.get(li_arena_id).parent, ul_id);
        // Each li should match the WebCore li's node_id
        assert_eq!(li_arena_id.0, ul.children[i].node_id);
    }
}

#[test]
fn arena_text_nodes_have_content() {
    let doc = parse_html("<p>Hello World</p>");
    let arena = &doc.arena;

    // Find the text node (html > body > p > #text)
    let p = &body_of(&doc).children[0]; // body > p
    assert_eq!(p.tag, "p");

    let text = &p.children[0];
    assert_eq!(text.tag, "#text");
    assert!(text.text.contains("Hello World"));

    let text_id = NodeId(text.node_id);
    assert!(arena.is_alive(text_id));
    assert_eq!(arena.get(text_id).tag, "#text");
    assert!(arena.get(text_id).text.contains("Hello World"));
}

#[test]
fn arena_attributes_copied() {
    let doc = parse_html(r#"<div id="main" class="container"><a href="/link">click</a></div>"#);
    let arena = &doc.arena;

    let div = &body_of(&doc).children[0]; // body > div
    assert_eq!(div.tag, "div");
    let div_id = NodeId(div.node_id);

    assert_eq!(arena.get(div_id).attributes.get("id").map(|s| s.as_str()), Some("main"));
    assert_eq!(arena.get(div_id).attributes.get("class").map(|s| s.as_str()), Some("container"));

    let a = &div.children[0];
    let a_id = NodeId(a.node_id);
    assert_eq!(arena.get(a_id).attributes.get("href").map(|s| s.as_str()), Some("/link"));
}

#[test]
fn node_map_lookup_works() {
    let mut doc = parse_html("<div><p>text</p></div>");
    doc.rebuild_node_map();

    let total = count_nodes(&doc.root);
    assert!(total >= 4, "should have at least html, body, div, p, #text");

    // Look up each node by its id
    let ids = collect_node_ids(&doc.root);
    for &id in &ids {
        let boxref = doc.get_box_by_id(id);
        assert!(boxref.is_some(), "node_id {} should be in node_map", id);
        assert_eq!(boxref.unwrap().node_id, id, "looked-up box should have matching node_id");
    }

    // Look up non-existent id
    assert!(doc.get_box_by_id(99999).is_none());
    assert!(doc.get_box_by_id(0).is_none());
}

#[test]
fn node_map_finds_deep_nodes() {
    let mut doc = parse_html(r#"<div><ul><li><a href="/l1">link1</a></li><li><a href="/l2">link2</a></li></ul></div>"#);
    doc.rebuild_node_map();

    // Find all <a> tags by walking the tree
    fn find_tags<'a>(node: &'a crate::types::WebCore, tag: &str) -> Vec<&'a crate::types::WebCore> {
        let mut result = Vec::new();
        if node.tag == tag { result.push(node); }
        for child in &node.children {
            result.extend(find_tags(child, tag));
        }
        result
    }

    let anchors = find_tags(&doc.root, "a");
    assert_eq!(anchors.len(), 2, "should find 2 anchor tags");

    for a in &anchors {
        let found = doc.get_box_by_id(a.node_id).unwrap();
        assert_eq!(found.tag, "a");
        assert_eq!(found.node_id, a.node_id);
    }
}

#[test]
fn arena_and_webcore_node_counts_match() {
    let doc = parse_html(r#"
        <html>
        <body>
            <h1>Title</h1>
            <p>Paragraph with <em>emphasis</em> and <strong>bold</strong>.</p>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
                <li>Item 3</li>
            </ul>
        </body>
        </html>
    "#);

    let webcore_count = count_nodes(&doc.root);
    // Arena has slot 0 (sentinel) + real nodes. Count alive nodes.
    let arena_alive = (1..doc.arena.len())
        .filter(|&i| doc.arena.is_alive(NodeId(i as u32)))
        .count();

    assert_eq!(webcore_count, arena_alive,
        "WebCore tree has {} nodes but arena has {} alive nodes",
        webcore_count, arena_alive);
}

#[test]
fn arena_parent_child_links_consistent() {
    let doc = parse_html("<div><span>a</span><span>b</span></div>");
    let arena = &doc.arena;

    // Walk entire tree and verify every child's parent link is correct
    fn verify_links(arena: &crate::dom::arena::DomArena, node: &crate::types::WebCore) {
        let node_id = NodeId(node.node_id);
        for child in &node.children {
            let child_id = NodeId(child.node_id);
            assert_eq!(
                arena.get(child_id).parent, node_id,
                "child '{}' (id={}) parent should be '{}' (id={}), got id={}",
                child.tag, child.node_id, node.tag, node.node_id,
                arena.get(child_id).parent.0
            );
            verify_links(arena, child);
        }
    }
    verify_links(arena, &doc.root);
}

#[test]
fn arena_sibling_links_consistent() {
    let doc = parse_html("<ul><li>a</li><li>b</li><li>c</li></ul>");
    let arena = &doc.arena;

    let ul = &body_of(&doc).children[0]; // body > ul
    let ul_id = NodeId(ul.node_id);

    let children: Vec<NodeId> = arena.children(ul_id).collect();
    assert_eq!(children.len(), 3);

    // Check next_sibling chain
    assert_eq!(arena.get(children[0]).next_sibling, children[1]);
    assert_eq!(arena.get(children[1]).next_sibling, children[2]);
    assert!(arena.get(children[2]).next_sibling.is_none());

    // Check prev_sibling chain
    assert!(arena.get(children[0]).prev_sibling.is_none());
    assert_eq!(arena.get(children[1]).prev_sibling, children[0]);
    assert_eq!(arena.get(children[2]).prev_sibling, children[1]);
}

#[test]
fn alloc_node_id_increments() {
    let mut doc = parse_html("<p>hi</p>");
    let id1 = doc.alloc_node_id();
    let id2 = doc.alloc_node_id();
    assert!(id1 > 0);
    assert_eq!(id2, id1 + 1);
    // Should not collide with any existing node
    let ids = collect_node_ids(&doc.root);
    assert!(!ids.contains(&id1));
    assert!(!ids.contains(&id2));
}

#[test]
fn arena_get_element_by_id_works() {
    let doc = parse_html(r#"<div id="outer"><span id="inner">text</span></div>"#);
    let arena = &doc.arena;
    let html_id = NodeId(doc.root.node_id);

    let found = arena.get_element_by_id(html_id, "inner");
    assert!(found.is_some());
    let inner_id = found.unwrap();
    assert_eq!(arena.get(inner_id).tag, "span");
    assert_eq!(arena.get(inner_id).attributes.get("id").map(|s| s.as_str()), Some("inner"));

    // Verify it matches the WebCore
    let span = &body_of(&doc).children[0].children[0]; // body > div > span
    assert_eq!(span.node_id, inner_id.0);
}

#[test]
fn rebuild_arena_from_tree_works() {
    let mut doc = parse_html("<div><p>hello</p></div>");

    // Trash the arena
    doc.arena = crate::dom::arena::DomArena::new();

    // Rebuild it
    crate::html::rebuild_arena_from_tree(&mut doc.arena, &mut doc.root);

    // Verify structure is correct
    let html_id = NodeId(doc.root.node_id);
    assert!(doc.arena.is_alive(html_id));
    assert_eq!(doc.arena.get(html_id).tag, "html");

    let body_id = NodeId(body_of(&doc).node_id);
    assert_eq!(doc.arena.get(body_id).parent, html_id);

    // Verify all parent-child links
    fn verify(arena: &crate::dom::arena::DomArena, node: &crate::types::WebCore) {
        let nid = NodeId(node.node_id);
        for child in &node.children {
            let cid = NodeId(child.node_id);
            assert_eq!(arena.get(cid).parent, nid);
            verify(arena, child);
        }
    }
    verify(&doc.arena, &doc.root);
}
