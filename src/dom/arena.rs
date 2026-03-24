//! Arena-based DOM tree with stable node identity.
//!
//! Every node gets a `NodeId` (u32 index) that never changes, even when
//! siblings are inserted/removed.  Parent–child–sibling links are indices,
//! not pointers, so no reference is ever invalidated.
//!
//! This module is the foundation for the engine redesign (see plan.md).
//! It coexists with the existing `HtmlBox` tree during migration.

use std::collections::HashMap;

// ─── Node Identity ──────────────────────────────────────────────────────────

/// Stable node identifier — an index into the arena.
/// `NodeId(0)` is reserved as "null / no node".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NONE: NodeId = NodeId(0);

    #[inline]
    pub fn is_none(self) -> bool { self.0 == 0 }
    #[inline]
    pub fn is_some(self) -> bool { self.0 != 0 }
    #[inline]
    pub fn index(self) -> usize { self.0 as usize }
}

impl From<u32> for NodeId {
    fn from(v: u32) -> Self { NodeId(v) }
}

impl From<NodeId> for u32 {
    fn from(v: NodeId) -> Self { v.0 }
}

// ─── Node Types ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    Element,
    Text,
    Comment,
    Document,
}

// ─── Node ───────────────────────────────────────────────────────────────────

/// A single DOM node.  Tree links are `NodeId` indices — never raw pointers.
#[derive(Clone, Debug)]
pub struct Node {
    // ── Identity ──
    pub node_type: NodeType,
    pub tag: String,

    // ── Tree structure (linked-list children for O(1) insert/remove) ──
    pub parent:       NodeId,
    pub first_child:  NodeId,
    pub last_child:   NodeId,
    pub next_sibling: NodeId,
    pub prev_sibling: NodeId,

    // ── Data ──
    pub attributes: HashMap<String, String>,
    pub text: String,

    // ── Flags ──
    pub dirty: DirtyFlags,

    /// Whether this slot is in use (false = on the free list).
    alive: bool,
}

impl Node {
    fn new_element(tag: impl Into<String>) -> Self {
        Node {
            node_type: NodeType::Element,
            tag: tag.into(),
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            next_sibling: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            attributes: HashMap::new(),
            text: String::new(),
            dirty: DirtyFlags::ALL,
            alive: true,
        }
    }

    fn new_text(text: impl Into<String>) -> Self {
        Node {
            node_type: NodeType::Text,
            tag: "#text".to_string(),
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            next_sibling: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            attributes: HashMap::new(),
            text: text.into(),
            dirty: DirtyFlags::ALL,
            alive: true,
        }
    }
}

// ─── Dirty Flags ────────────────────────────────────────────────────────────

/// Tracks what changed on a node since the last frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyFlags(pub u8);

impl DirtyFlags {
    pub const NONE:   DirtyFlags = DirtyFlags(0);
    pub const STYLE:  DirtyFlags = DirtyFlags(0b0001);
    pub const LAYOUT: DirtyFlags = DirtyFlags(0b0010);
    pub const PAINT:  DirtyFlags = DirtyFlags(0b0100);
    pub const ALL:    DirtyFlags = DirtyFlags(0b0111);

    #[inline] pub fn contains(self, other: DirtyFlags) -> bool { self.0 & other.0 == other.0 }
    #[inline] pub fn is_empty(self) -> bool { self.0 == 0 }
    #[inline] pub fn any(self) -> bool { self.0 != 0 }
}

impl std::ops::BitOr for DirtyFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { DirtyFlags(self.0 | rhs.0) }
}
impl std::ops::BitOrAssign for DirtyFlags {
    fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0; }
}
impl Default for DirtyFlags {
    fn default() -> Self { DirtyFlags::NONE }
}

// ─── Arena ──────────────────────────────────────────────────────────────────

/// Arena allocator for DOM nodes.
///
/// Slot 0 is reserved (NodeId::NONE).  Freed nodes go on a free list and are
/// reused by the next allocation — no memory grows without bound.
pub struct DomArena {
    nodes: Vec<Node>,
    free_list: Vec<NodeId>,
}

impl DomArena {
    pub fn new() -> Self {
        // Slot 0 = sentinel (NodeId::NONE)
        let sentinel = Node {
            node_type: NodeType::Document,
            tag: String::new(),
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            next_sibling: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            attributes: HashMap::new(),
            text: String::new(),
            dirty: DirtyFlags::NONE,
            alive: false,
        };
        DomArena {
            nodes: vec![sentinel],
            free_list: Vec::new(),
        }
    }

    // ── Allocation ──

    /// Create a new element node.  Returns its stable `NodeId`.
    pub fn create_element(&mut self, tag: &str) -> NodeId {
        self.alloc(Node::new_element(tag))
    }

    /// Create a new text node.
    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.alloc(Node::new_text(text))
    }

    fn alloc(&mut self, node: Node) -> NodeId {
        if let Some(id) = self.free_list.pop() {
            self.nodes[id.index()] = node;
            id
        } else {
            let id = NodeId(self.nodes.len() as u32);
            self.nodes.push(node);
            id
        }
    }

    // ── Access ──

    #[inline]
    pub fn get(&self, id: NodeId) -> &Node {
        debug_assert!(id.is_some() && (id.index()) < self.nodes.len());
        &self.nodes[id.index()]
    }

    #[inline]
    pub fn get_mut(&mut self, id: NodeId) -> &mut Node {
        debug_assert!(id.is_some() && (id.index()) < self.nodes.len());
        &mut self.nodes[id.index()]
    }

    pub fn is_alive(&self, id: NodeId) -> bool {
        id.is_some() && id.index() < self.nodes.len() && self.nodes[id.index()].alive
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    // ── Tree Mutations (O(1), never invalidate other NodeIds) ──

    /// Append `child` as the last child of `parent`.
    /// `child` must be detached (no parent).
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        debug_assert!(self.get(child).parent.is_none(), "child already has a parent");
        let last = self.get(parent).last_child;

        self.get_mut(child).parent = parent;
        self.get_mut(child).prev_sibling = last;
        self.get_mut(child).next_sibling = NodeId::NONE;

        if last.is_some() {
            self.get_mut(last).next_sibling = child;
        } else {
            self.get_mut(parent).first_child = child;
        }
        self.get_mut(parent).last_child = child;
        self.mark_dirty(parent, DirtyFlags::LAYOUT);
    }

    /// Insert `child` before `reference` (which must be a child of `parent`).
    pub fn insert_before(&mut self, parent: NodeId, child: NodeId, reference: NodeId) {
        debug_assert!(self.get(child).parent.is_none(), "child already has a parent");
        debug_assert_eq!(self.get(reference).parent, parent);

        let prev = self.get(reference).prev_sibling;
        self.get_mut(child).parent = parent;
        self.get_mut(child).prev_sibling = prev;
        self.get_mut(child).next_sibling = reference;
        self.get_mut(reference).prev_sibling = child;

        if prev.is_some() {
            self.get_mut(prev).next_sibling = child;
        } else {
            self.get_mut(parent).first_child = child;
        }
        self.mark_dirty(parent, DirtyFlags::LAYOUT);
    }

    /// Remove `child` from its parent.  The node stays in the arena (detached).
    pub fn remove_child(&mut self, child: NodeId) {
        let parent = self.get(child).parent;
        if parent.is_none() { return; }

        let prev = self.get(child).prev_sibling;
        let next = self.get(child).next_sibling;

        if prev.is_some() {
            self.get_mut(prev).next_sibling = next;
        } else {
            self.get_mut(parent).first_child = next;
        }

        if next.is_some() {
            self.get_mut(next).prev_sibling = prev;
        } else {
            self.get_mut(parent).last_child = prev;
        }

        self.get_mut(child).parent = NodeId::NONE;
        self.get_mut(child).prev_sibling = NodeId::NONE;
        self.get_mut(child).next_sibling = NodeId::NONE;
        self.mark_dirty(parent, DirtyFlags::LAYOUT);
    }

    /// Free a detached node (and all its descendants) for reuse.
    pub fn free(&mut self, id: NodeId) {
        if !self.is_alive(id) { return; }
        // Free children first
        let mut child = self.get(id).first_child;
        while child.is_some() {
            let next = self.get(child).next_sibling;
            self.free(child);
            child = next;
        }
        self.nodes[id.index()].alive = false;
        self.free_list.push(id);
    }

    // ── Dirty Flag Propagation ──

    fn mark_dirty(&mut self, id: NodeId, flags: DirtyFlags) {
        if id.is_none() { return; }
        self.get_mut(id).dirty |= flags;
        // Propagate up: parent needs to know a child changed
        let parent = self.get(id).parent;
        if parent.is_some() && !self.get(parent).dirty.contains(flags) {
            self.mark_dirty(parent, flags);
        }
    }

    // ── Attribute Mutation (sets dirty flags) ──

    pub fn set_attribute(&mut self, id: NodeId, key: &str, value: &str) {
        self.get_mut(id).attributes.insert(key.to_string(), value.to_string());
        // Class/style changes need re-style; others might too (presentational attrs)
        self.mark_dirty(id, DirtyFlags::STYLE);
    }

    pub fn remove_attribute(&mut self, id: NodeId, key: &str) {
        self.get_mut(id).attributes.remove(key);
        self.mark_dirty(id, DirtyFlags::STYLE);
    }

    pub fn set_text(&mut self, id: NodeId, text: &str) {
        self.get_mut(id).text = text.to_string();
        self.mark_dirty(id, DirtyFlags::LAYOUT);
    }

    // ── Traversal ──

    /// Iterate child NodeIds of `parent`.
    pub fn children(&self, parent: NodeId) -> ChildIter<'_> {
        ChildIter { arena: self, next: self.get(parent).first_child }
    }

    /// Count children.
    pub fn child_count(&self, parent: NodeId) -> usize {
        self.children(parent).count()
    }

    /// Collect the ancestor chain from `id` to the root (inclusive).
    pub fn ancestor_chain(&self, id: NodeId) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur.is_some() {
            chain.push(cur);
            cur = self.get(cur).parent;
        }
        chain
    }

    /// Check if `ancestor` is an ancestor of `descendant`.
    pub fn is_ancestor_of(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        let mut cur = self.get(descendant).parent;
        while cur.is_some() {
            if cur == ancestor { return true; }
            cur = self.get(cur).parent;
        }
        false
    }

    /// Find element by ID attribute.
    pub fn get_element_by_id(&self, root: NodeId, id: &str) -> Option<NodeId> {
        if self.get(root).attributes.get("id").map(|s| s.as_str()) == Some(id) {
            return Some(root);
        }
        let mut child = self.get(root).first_child;
        while child.is_some() {
            if let Some(found) = self.get_element_by_id(child, id) {
                return Some(found);
            }
            child = self.get(child).next_sibling;
        }
        None
    }
}

// ─── Iterators ──────────────────────────────────────────────────────────────

pub struct ChildIter<'a> {
    arena: &'a DomArena,
    next: NodeId,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        if self.next.is_none() { return None; }
        let id = self.next;
        self.next = self.arena.get(id).next_sibling;
        Some(id)
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_access() {
        let mut arena = DomArena::new();
        let div = arena.create_element("div");
        assert_eq!(arena.get(div).tag, "div");
        assert_eq!(arena.get(div).node_type, NodeType::Element);
        assert!(arena.is_alive(div));
    }

    #[test]
    fn append_children() {
        let mut arena = DomArena::new();
        let parent = arena.create_element("ul");
        let li1 = arena.create_element("li");
        let li2 = arena.create_element("li");
        let li3 = arena.create_element("li");

        arena.append_child(parent, li1);
        arena.append_child(parent, li2);
        arena.append_child(parent, li3);

        let children: Vec<NodeId> = arena.children(parent).collect();
        assert_eq!(children, vec![li1, li2, li3]);
        assert_eq!(arena.child_count(parent), 3);

        // Check parent links
        assert_eq!(arena.get(li1).parent, parent);
        assert_eq!(arena.get(li2).parent, parent);
        assert_eq!(arena.get(li3).parent, parent);

        // Check sibling links
        assert_eq!(arena.get(li1).next_sibling, li2);
        assert_eq!(arena.get(li2).next_sibling, li3);
        assert_eq!(arena.get(li3).next_sibling, NodeId::NONE);
        assert_eq!(arena.get(li3).prev_sibling, li2);
        assert_eq!(arena.get(li2).prev_sibling, li1);
        assert_eq!(arena.get(li1).prev_sibling, NodeId::NONE);
    }

    #[test]
    fn insert_before() {
        let mut arena = DomArena::new();
        let parent = arena.create_element("ul");
        let li1 = arena.create_element("li");
        let li3 = arena.create_element("li");
        arena.append_child(parent, li1);
        arena.append_child(parent, li3);

        // Insert li2 before li3
        let li2 = arena.create_element("li");
        arena.insert_before(parent, li2, li3);

        let children: Vec<NodeId> = arena.children(parent).collect();
        assert_eq!(children, vec![li1, li2, li3]);
    }

    #[test]
    fn insert_before_first() {
        let mut arena = DomArena::new();
        let parent = arena.create_element("ul");
        let li2 = arena.create_element("li");
        arena.append_child(parent, li2);

        // Insert li1 before li2 (becomes first child)
        let li1 = arena.create_element("li");
        arena.insert_before(parent, li1, li2);

        let children: Vec<NodeId> = arena.children(parent).collect();
        assert_eq!(children, vec![li1, li2]);
        assert_eq!(arena.get(parent).first_child, li1);
    }

    #[test]
    fn remove_child() {
        let mut arena = DomArena::new();
        let parent = arena.create_element("ul");
        let li1 = arena.create_element("li");
        let li2 = arena.create_element("li");
        let li3 = arena.create_element("li");
        arena.append_child(parent, li1);
        arena.append_child(parent, li2);
        arena.append_child(parent, li3);

        // Remove middle child
        arena.remove_child(li2);
        let children: Vec<NodeId> = arena.children(parent).collect();
        assert_eq!(children, vec![li1, li3]);
        assert_eq!(arena.get(li1).next_sibling, li3);
        assert_eq!(arena.get(li3).prev_sibling, li1);

        // li2 is detached
        assert_eq!(arena.get(li2).parent, NodeId::NONE);
    }

    #[test]
    fn remove_first_child() {
        let mut arena = DomArena::new();
        let parent = arena.create_element("div");
        let a = arena.create_element("a");
        let b = arena.create_element("b");
        arena.append_child(parent, a);
        arena.append_child(parent, b);

        arena.remove_child(a);
        assert_eq!(arena.get(parent).first_child, b);
        assert_eq!(arena.get(b).prev_sibling, NodeId::NONE);
    }

    #[test]
    fn remove_last_child() {
        let mut arena = DomArena::new();
        let parent = arena.create_element("div");
        let a = arena.create_element("a");
        let b = arena.create_element("b");
        arena.append_child(parent, a);
        arena.append_child(parent, b);

        arena.remove_child(b);
        assert_eq!(arena.get(parent).last_child, a);
        assert_eq!(arena.get(a).next_sibling, NodeId::NONE);
    }

    #[test]
    fn free_and_reuse() {
        let mut arena = DomArena::new();
        let a = arena.create_element("a");
        let b = arena.create_element("b");
        assert_eq!(arena.len(), 3); // sentinel + a + b

        arena.free(a);
        assert!(!arena.is_alive(a));

        // Next alloc reuses the freed slot
        let c = arena.create_element("c");
        assert_eq!(c, a); // same NodeId reused
        assert_eq!(arena.get(c).tag, "c");
        assert_eq!(arena.len(), 3); // no growth
    }

    #[test]
    fn node_id_stability_on_insert() {
        // The whole point: inserting a child doesn't change other nodes' IDs
        let mut arena = DomArena::new();
        let parent = arena.create_element("ul");
        let li1 = arena.create_element("li");
        let li2 = arena.create_element("li");
        arena.append_child(parent, li1);
        arena.append_child(parent, li2);

        let li1_id = li1;
        let li2_id = li2;

        // Insert a new element before li1 — this is what breaks Vec<HtmlBox>
        let li0 = arena.create_element("li");
        arena.insert_before(parent, li0, li1);

        // li1 and li2 still have the same NodeId
        assert_eq!(li1, li1_id);
        assert_eq!(li2, li2_id);
        assert_eq!(arena.get(li1).tag, "li");
        assert_eq!(arena.get(li2).tag, "li");
    }

    #[test]
    fn ancestor_chain() {
        let mut arena = DomArena::new();
        let html = arena.create_element("html");
        let body = arena.create_element("body");
        let div = arena.create_element("div");
        let p = arena.create_element("p");
        arena.append_child(html, body);
        arena.append_child(body, div);
        arena.append_child(div, p);

        let chain = arena.ancestor_chain(p);
        assert_eq!(chain, vec![p, div, body, html]);
    }

    #[test]
    fn is_ancestor_of() {
        let mut arena = DomArena::new();
        let html = arena.create_element("html");
        let body = arena.create_element("body");
        let div = arena.create_element("div");
        arena.append_child(html, body);
        arena.append_child(body, div);

        assert!(arena.is_ancestor_of(html, div));
        assert!(arena.is_ancestor_of(body, div));
        assert!(!arena.is_ancestor_of(div, html));
        assert!(!arena.is_ancestor_of(div, body));
    }

    #[test]
    fn get_element_by_id() {
        let mut arena = DomArena::new();
        let root = arena.create_element("div");
        let child = arena.create_element("span");
        arena.set_attribute(child, "id", "target");
        arena.append_child(root, child);

        assert_eq!(arena.get_element_by_id(root, "target"), Some(child));
        assert_eq!(arena.get_element_by_id(root, "missing"), None);
    }

    #[test]
    fn dirty_flags_propagate_up() {
        let mut arena = DomArena::new();
        let root = arena.create_element("div");
        let child = arena.create_element("p");
        arena.append_child(root, child);

        // Clear dirty flags
        arena.get_mut(root).dirty = DirtyFlags::NONE;
        arena.get_mut(child).dirty = DirtyFlags::NONE;

        // Mutate child — dirty should propagate to parent
        arena.set_attribute(child, "class", "active");
        assert!(arena.get(child).dirty.contains(DirtyFlags::STYLE));
        assert!(arena.get(root).dirty.contains(DirtyFlags::STYLE));
    }

    #[test]
    fn text_node() {
        let mut arena = DomArena::new();
        let t = arena.create_text("hello world");
        assert_eq!(arena.get(t).text, "hello world");
        assert_eq!(arena.get(t).node_type, NodeType::Text);
        assert_eq!(arena.get(t).tag, "#text");
    }

    #[test]
    fn set_text_marks_layout_dirty() {
        let mut arena = DomArena::new();
        let root = arena.create_element("div");
        let t = arena.create_text("old");
        arena.append_child(root, t);
        arena.get_mut(root).dirty = DirtyFlags::NONE;
        arena.get_mut(t).dirty = DirtyFlags::NONE;

        arena.set_text(t, "new content");
        assert!(arena.get(t).dirty.contains(DirtyFlags::LAYOUT));
        assert!(arena.get(root).dirty.contains(DirtyFlags::LAYOUT));
    }
}
