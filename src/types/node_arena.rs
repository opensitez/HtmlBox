//! The flat node arena — storage for every `WebCore` in a document.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

/// Flat storage for all WebCore nodes, indexed by node_id.
/// This is the source of truth for the DOM tree. Tree structure is encoded
/// via linked-list pointers (parent/first_child/last_child/next_sibling/prev_sibling)
/// on each WebCore.
pub struct NodeArena {
    nodes: HashMap<u32, WebCore>,
    pub root_id: u32,
}

impl NodeArena {
    pub fn new() -> Self {
        Self { nodes: HashMap::new(), root_id: 0 }
    }

    /// Insert a node. If a node with this ID already exists, it's replaced.
    pub fn insert(&mut self, node: WebCore) {
        self.nodes.insert(node.node_id, node);
    }

    /// Get an immutable reference to a node.
    #[inline]
    pub fn get(&self, id: u32) -> Option<&WebCore> {
        self.nodes.get(&id)
    }

    /// Get a mutable reference to a node.
    #[inline]
    pub fn get_mut(&mut self, id: u32) -> Option<&mut WebCore> {
        self.nodes.get_mut(&id)
    }

    /// Remove a node from the arena. Returns it if it existed.
    pub fn remove(&mut self, id: u32) -> Option<WebCore> {
        self.nodes.remove(&id)
    }

    /// Check if a node exists.
    pub fn contains(&self, id: u32) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Number of nodes in the arena.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Collect child node_ids of a parent (from linked-list pointers).
    pub fn child_ids(&self, parent_id: u32) -> Vec<u32> {
        let mut ids = Vec::new();
        if let Some(parent) = self.nodes.get(&parent_id) {
            let mut cur = parent.first_child;
            while cur != 0 {
                ids.push(cur);
                cur = self.nodes.get(&cur).map(|n| n.next_sibling).unwrap_or(0);
            }
        }
        ids
    }

    /// Iterate child node_ids without allocation (returns an iterator).
    pub fn children(&self, parent_id: u32) -> ChildIdIter<'_> {
        let first = self.nodes.get(&parent_id).map(|n| n.first_child).unwrap_or(0);
        ChildIdIter { arena: self, next: first }
    }

    /// Count children of a node.
    pub fn child_count(&self, parent_id: u32) -> usize {
        self.children(parent_id).count()
    }

    /// Get the root node.
    pub fn root(&self) -> Option<&WebCore> {
        self.nodes.get(&self.root_id)
    }

    /// Get the root node mutably.
    pub fn root_mut(&mut self) -> Option<&mut WebCore> {
        self.nodes.get_mut(&self.root_id)
    }

    /// Append a child to a parent. Updates linked-list pointers.
    pub fn append_child(&mut self, parent_id: u32, child_id: u32) {
        let old_last = self.nodes.get(&parent_id).map(|p| p.last_child).unwrap_or(0);

        if let Some(child) = self.nodes.get_mut(&child_id) {
            child.parent = parent_id;
            child.prev_sibling = old_last;
            child.next_sibling = 0;
        }

        if old_last != 0 {
            if let Some(prev) = self.nodes.get_mut(&old_last) {
                prev.next_sibling = child_id;
            }
        }

        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            if parent.first_child == 0 {
                parent.first_child = child_id;
            }
            parent.last_child = child_id;
        }
    }

    /// Remove a child from its parent. Updates linked-list pointers.
    /// The node stays in the arena (detached).
    pub fn detach(&mut self, node_id: u32) {
        let (parent_id, prev, next) = match self.nodes.get(&node_id) {
            Some(n) => (n.parent, n.prev_sibling, n.next_sibling),
            None => return,
        };
        if parent_id == 0 { return; }

        if prev != 0 {
            if let Some(p) = self.nodes.get_mut(&prev) { p.next_sibling = next; }
        } else if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.first_child = next;
        }

        if next != 0 {
            if let Some(n) = self.nodes.get_mut(&next) { n.prev_sibling = prev; }
        } else if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.last_child = prev;
        }

        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.parent = 0;
            node.prev_sibling = 0;
            node.next_sibling = 0;
        }
    }

    /// Build the arena from an existing WebCore tree (migration helper).
    /// Clones all nodes into the flat HashMap. Original tree unchanged.
    pub fn from_tree(root: &WebCore) -> Self {
        let mut arena = Self::new();
        arena.root_id = root.node_id;
        flatten_into_arena(root, &mut arena);
        arena
    }
}

/// Recursively flatten a Vec<WebCore> tree into the arena.
/// Clones each node (with empty children Vec) into the flat store.
/// The original tree is NOT modified.
fn flatten_into_arena(node: &WebCore, arena: &mut NodeArena) {
    // Clone the node with an empty children Vec (arena uses linked-list, not Vec)
    let mut flat_node = node.clone();
    flat_node.children.clear(); // arena nodes don't need Vec children
    arena.insert(flat_node);
    // Recurse into children
    for child in &node.children {
        flatten_into_arena(child, arena);
    }
}

/// Iterator over child node_ids using linked-list pointers.
pub struct ChildIdIter<'a> {
    arena: &'a NodeArena,
    next: u32,
}

impl std::fmt::Debug for NodeArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeArena")
            .field("len", &self.nodes.len())
            .field("root_id", &self.root_id)
            .finish()
    }
}

impl<'a> Iterator for ChildIdIter<'a> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.next == 0 { return None; }
        let id = self.next;
        self.next = self.arena.get(id).map(|n| n.next_sibling).unwrap_or(0);
        Some(id)
    }
}
