//! Wiring a parsed `WebCore` tree into the arena.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use crate::css::*;

// ─── Arena wiring ────────────────────────────────────────────────────────────

/// Walk the WebCore tree and wire arena parent-child links to mirror it.
/// Called once after parsing is complete and the full WebCore tree is built.
pub(crate) fn wire_arena_children(arena: &mut crate::dom::arena::DomArena, root: &WebCore) {
    use crate::dom::arena::NodeId;
    let root_id = NodeId(root.node_id);
    if root_id.is_none() || !arena.is_alive(root_id) { return; }
    for child in &root.children {
        let child_id = NodeId(child.node_id);
        if child_id.is_none() || !arena.is_alive(child_id) { continue; }
        // Set data on arena character-data nodes — comments carry text too.
        if child.tag == "#text" || child.tag == "#comment" {
            arena.get_mut(child_id).text = child.text.clone();
        }
        // Copy attributes to arena node
        for (k, v) in &child.attributes {
            arena.get_mut(child_id).attributes.insert(k.clone(), v.clone());
        }
        arena.append_child(root_id, child_id);
        wire_arena_children(arena, child);
    }
}

/// Rebuild arena from an existing WebCore tree (e.g. after clone or DOM mutation).
/// Creates fresh arena nodes for every WebCore and wires parent-child links.
pub fn rebuild_arena_from_tree(arena: &mut crate::dom::arena::DomArena, root: &mut WebCore) {
    *arena = crate::dom::arena::DomArena::new();
    rebuild_arena_recursive(arena, root);
}

fn rebuild_arena_recursive(arena: &mut crate::dom::arena::DomArena, node: &mut WebCore) {
    use crate::dom::arena::NodeId;
    // Create arena node
    let arena_id = match node.tag.as_str() {
        "#text" => arena.create_text(&node.text),
        "#comment" => arena.create_comment(&node.text),
        _ => {
            let id = arena.create_element(&node.tag);
            for (k, v) in &node.attributes {
                arena.get_mut(id).attributes.insert(k.clone(), v.clone());
            }
            id
        }
    };
    node.node_id = arena_id.0;

    // Recurse children
    for child in &mut node.children {
        rebuild_arena_recursive(arena, child);
        let child_id = NodeId(child.node_id);
        arena.append_child(arena_id, child_id);
    }
    // Populate linked-list pointers on WebCore (second pass — all node_ids assigned)
    populate_sibling_links(node);
}

/// Populate parent/first_child/last_child/next_sibling/prev_sibling on a node
/// and all its Vec children. Called after node_ids are assigned.
pub fn populate_sibling_links(node: &mut WebCore) {
    let parent_id = node.node_id;
    let n = node.children.len();
    if n == 0 {
        node.first_child = 0;
        node.last_child = 0;
        return;
    }
    node.first_child = node.children[0].node_id;
    node.last_child = node.children[n - 1].node_id;
    for i in 0..n {
        node.children[i].parent = parent_id;
        node.children[i].prev_sibling = if i > 0 { node.children[i - 1].node_id } else { 0 };
        node.children[i].next_sibling = if i + 1 < n { node.children[i + 1].node_id } else { 0 };
    }
    // Recurse
    for child in &mut node.children {
        populate_sibling_links(child);
    }
}

/// Rebuild arena nodes for a subtree and append each child to `parent_arena_id`.
/// Used by `dom_set_inner_html` to wire new children into the existing arena.
pub fn rebuild_arena_recursive_pub(
    arena: &mut crate::dom::arena::DomArena,
    node: &mut WebCore,
    parent_arena_id: crate::dom::arena::NodeId,
) {
    rebuild_arena_recursive(arena, node);
    let child_id = crate::dom::arena::NodeId(node.node_id);
    arena.append_child(parent_arena_id, child_id);
}
