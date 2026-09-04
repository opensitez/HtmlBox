//! Wiring a parsed `WebCore` tree into the arena.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use crate::types::*;

// ─── Arena wiring ────────────────────────────────────────────────────────────

/// Walk the WebCore tree and wire arena parent-child links to mirror it.
/// Called once after parsing is complete and the full WebCore tree is built.
pub(crate) fn wire_arena_children(arena: &mut crate::dom::arena::DomArena, root: &mut WebCore) {
    use crate::dom::arena::NodeId;
    let root_id = NodeId(root.node_id);
    if root_id.is_none() || !arena.is_alive(root_id) {
        return;
    }
    // ⛔ THIS node's own attributes, before its children's. The loop below used
    // to be the only place attributes were copied, so the node it was first
    // called on — `<html>` — never got its own: `documentElement`'s `lang`,
    // `class` and everything else were missing from the arena on 10 of the 24
    // example pages.
    for (k, v) in root.attributes.iter() {
        arena
            .get_mut(root_id)
            .attributes
            .insert(k.clone(), v.clone());
    }
    for child in &mut root.children {
        let mut child_id = NodeId(child.node_id);
        if child_id.is_none() || !arena.is_alive(child_id) {
            // ⛔ A SYNTHESIZED node — `WebCore::new` hands out ids from its own
            // counter starting at 500,000, which the arena has never heard of.
            // Table normalization creates a `<tbody>` that way.
            //
            // This used to `continue` (and, for the subtree root, `return`),
            // so everything below such a node was never wired: a `<td>`'s text
            // node did not reach the arena at all, and `textContent` on a table
            // cell answered `""`. Give it an arena node instead.
            child_id = match child.tag.as_str() {
                "#text" => arena.create_text(&child.text),
                "#comment" => arena.create_comment(&child.text),
                tag => arena.create_element(tag),
            };
            child.node_id = child_id.0;
        }
        // Set data on arena character-data nodes — comments carry text too.
        if child.tag == "#text" || child.tag == "#comment" {
            arena.get_mut(child_id).text = child.text.clone();
        }
        // Copy attributes to arena node
        for (k, v) in &child.attributes {
            arena
                .get_mut(child_id)
                .attributes
                .insert(k.clone(), v.clone());
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

/// Make the arena's view of `root`'s subtree match the `WebCore` tree exactly.
///
/// ⛔ Needed because the EDITOR mutates the render tree with no arena in
/// scope: `Editor::insert_br` and its neighbours take `&mut WebCore`, create
/// nodes with `WebCore::new` (whose ids come from a private counter the arena
/// has never heard of), and there is no `Document` to dual-write to. After an
/// Enter keypress in a `contenteditable`, `textContent` still answered the
/// PRE-EDIT text.
///
/// This is a concrete instance of `arenaplan.md` item 4's rationale — two
/// structures that must be kept in sync, and a mutation path that cannot.
/// Folding `WebCore` into the arena removes the need for this function.
///
/// Unlike `wire_arena_children` it also DETACHES: an edit can delete a node,
/// and appending alone would leave the arena holding it.
pub(crate) fn resync_subtree(arena: &mut crate::dom::arena::DomArena, root: &mut WebCore) {
    use crate::dom::arena::NodeId;
    let root_id = NodeId(root.node_id);
    if root_id.is_none() || !arena.is_alive(root_id) {
        return;
    }

    let existing: Vec<NodeId> = arena.children(root_id).collect();
    for child in existing {
        arena.remove_child(child);
    }
    for child in &mut root.children {
        let mut child_id = NodeId(child.node_id);
        if child_id.is_none() || !arena.is_alive(child_id) {
            child_id = match child.tag.as_str() {
                "#text" => arena.create_text(&child.text),
                "#comment" => arena.create_comment(&child.text),
                tag => arena.create_element(tag),
            };
            child.node_id = child_id.0;
        }
        if child.tag == "#text" || child.tag == "#comment" {
            arena.get_mut(child_id).text = child.text.clone();
        }
        // ⛔ No attribute copy here. The only caller is the EDITOR, and no
        // editing operation changes an attribute on a node the arena already
        // has — it creates nodes (handled above) or moves them (which keeps
        // their attributes). A mutation proved the line was unfalsifiable.
        arena.append_child(root_id, child_id);
        resync_subtree(arena, child);
    }
}
