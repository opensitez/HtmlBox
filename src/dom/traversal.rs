//! `TreeWalker` and `NodeIterator` — DOM §6.
//!
//! Two objects over the same filter vocabulary that disagree in four places,
//! every one of them measured against Chrome (`/tmp/webcore-html/tw*.html`)
//! rather than read off the prose:
//!
//! 1. A `TreeWalker`'s `nextNode()` NEVER returns the root and never runs the
//!    filter on it; a `NodeIterator`'s returns the root first.
//! 2. `FILTER_REJECT` prunes the whole subtree in a `TreeWalker` and is
//!    exactly `FILTER_SKIP` in a `NodeIterator`.
//! 3. `whatToShow` is consulted BEFORE the filter, and a node it excludes
//!    behaves like `SKIP` — children are still visited and **the filter is
//!    never called for it**.
//! 4. A `NodeIterator` tracks removals (§6.1's pre-removing steps) and a
//!    `TreeWalker` does not: remove a walker's `currentNode` and it keeps
//!    pointing at the detached node.
//!
//! Both are handles into a store on `Document`, the same shape as everything
//! else here — a `u32` naming state the document owns. That is what lets the
//! iterator's pre-removing steps run from inside `remove_child`.

use crate::types::Document;
use std::collections::HashMap;

// ─── NodeFilter constants (DOM §6.3) ────────────────────────────────────────

pub const SHOW_ALL: u32 = 0xFFFF_FFFF;
pub const SHOW_ELEMENT: u32 = 0x1;
pub const SHOW_ATTRIBUTE: u32 = 0x2;
pub const SHOW_TEXT: u32 = 0x4;
pub const SHOW_CDATA_SECTION: u32 = 0x8;
pub const SHOW_PROCESSING_INSTRUCTION: u32 = 0x40;
pub const SHOW_COMMENT: u32 = 0x80;
pub const SHOW_DOCUMENT: u32 = 0x100;
pub const SHOW_DOCUMENT_TYPE: u32 = 0x200;
pub const SHOW_DOCUMENT_FRAGMENT: u32 = 0x400;

pub const FILTER_ACCEPT: u16 = 1;
pub const FILTER_REJECT: u16 = 2;
pub const FILTER_SKIP: u16 = 3;

/// A `NodeFilter` callback. Takes the document so it can ask the node
/// anything — the JS form receives a live node, and here a node is an id plus
/// the document that gives it meaning.
pub type NodeFilterFn = Box<dyn FnMut(&Document, u32) -> u16 + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraversalKind {
    TreeWalker,
    NodeIterator,
}

pub struct Traversal {
    pub kind: TraversalKind,
    pub root: u32,
    pub what_to_show: u32,
    pub filter: Option<NodeFilterFn>,
    /// A walker's `currentNode`; an iterator's `referenceNode`.
    pub current: u32,
    /// `NodeIterator.pointerBeforeReferenceNode`.
    ///
    /// ⛔ Not derivable from `current`. It is why `previousNode()` from the end
    /// of an iterator returns the last node itself and a walker's does not —
    /// the iterator's pointer sits AFTER its reference, so stepping back lands
    /// on it (measured: `["i1", …]` against the walker's `["s1", …]`).
    pub pointer_before: bool,
}

/// The document's live traversals, keyed by handle.
#[derive(Default)]
pub struct TraversalStore {
    map: HashMap<u32, Traversal>,
    next_id: u32,
}

impl TraversalStore {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 1,
        }
    }

    /// Every NODE this store keeps a reference to.
    ///
    /// ⛔ Not the map's keys — those are TRAVERSAL handles from `next_id`. The
    /// node ids are `root` and `current` INSIDE each value. Reading the keys
    /// would both miss these and offer up handle integers as if they named
    /// nodes.
    pub fn node_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.map.values().flat_map(|t| [t.root, t.current])
    }

    pub fn insert(&mut self, t: Traversal) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.map.insert(id, t);
        id
    }

    pub fn get(&self, id: u32) -> Option<&Traversal> {
        self.map.get(&id)
    }
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Traversal> {
        self.map.get_mut(&id)
    }
    pub fn ids(&self) -> Vec<u32> {
        self.map.keys().copied().collect()
    }
}

impl std::fmt::Debug for TraversalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraversalStore")
            .field("live", &self.map.len())
            .finish()
    }
}

/// The `whatToShow` bit for a `nodeType`, per DOM §6.3: bit `1 << (type - 1)`.
pub fn show_bit(node_type: u16) -> u32 {
    if node_type == 0 {
        return 0;
    }
    1u32 << (node_type - 1)
}

impl Document {
    /// `document.createTreeWalker(root, whatToShow, filter)`.
    ///
    /// The walker's `currentNode` starts AT the root and the root is never
    /// filtered — `nextNode()` moves off it before asking anything.
    pub fn create_tree_walker(
        &mut self,
        root: u32,
        what_to_show: u32,
        filter: Option<NodeFilterFn>,
    ) -> u32 {
        self.traversals.insert(Traversal {
            kind: TraversalKind::TreeWalker,
            root,
            what_to_show,
            filter,
            current: root,
            pointer_before: true,
        })
    }

    /// `document.createNodeIterator(root, whatToShow, filter)`.
    ///
    /// The reference starts at the root with the pointer BEFORE it, which is
    /// why the first `nextNode()` answers the root itself.
    pub fn create_node_iterator(
        &mut self,
        root: u32,
        what_to_show: u32,
        filter: Option<NodeFilterFn>,
    ) -> u32 {
        self.traversals.insert(Traversal {
            kind: TraversalKind::NodeIterator,
            root,
            what_to_show,
            filter,
            current: root,
            pointer_before: true,
        })
    }

    /// `walker.root` / `iterator.root`.
    pub fn traversal_root(&self, t: u32) -> Option<u32> {
        self.traversals.get(t).map(|t| t.root)
    }

    /// `walker.whatToShow` / `iterator.whatToShow`.
    pub fn traversal_what_to_show(&self, t: u32) -> Option<u32> {
        self.traversals.get(t).map(|t| t.what_to_show)
    }

    /// Whether a `filter` was supplied — the callback itself is not readable
    /// back out, so this is the honest half of `walker.filter`.
    pub fn traversal_has_filter(&self, t: u32) -> bool {
        self.traversals.get(t).is_some_and(|t| t.filter.is_some())
    }

    /// `walker.currentNode`.
    pub fn current_node(&self, t: u32) -> Option<u32> {
        self.traversals.get(t).map(|t| t.current)
    }

    /// `walker.currentNode = node`.
    ///
    /// Settable to a node OUTSIDE the root — Chrome allows it and traversal
    /// carries on from there, stopping at the root going up.
    pub fn set_current_node(&mut self, t: u32, node: u32) {
        if let Some(tr) = self.traversals.get_mut(t) {
            tr.current = node;
        }
    }

    /// `iterator.referenceNode`.
    pub fn reference_node(&self, t: u32) -> Option<u32> {
        self.traversals.get(t).map(|t| t.current)
    }

    /// `iterator.pointerBeforeReferenceNode`.
    pub fn pointer_before_reference_node(&self, t: u32) -> Option<bool> {
        self.traversals.get(t).map(|t| t.pointer_before)
    }

    /// `iterator.detach()` — defined by DOM as doing nothing at all. It is
    /// kept so that old code calling it does not fail (measured: an iterator
    /// keeps working afterwards).
    pub fn traversal_detach(&self, _t: u32) {}

    // ─── filtering ──────────────────────────────────────────────────────────

    /// Run `whatToShow` and then the filter, per DOM §6.1's "filter" algorithm.
    ///
    /// ⛔ The ORDER is observable: a node excluded by `whatToShow` never
    /// reaches the callback at all. Measured by counting invocations — a
    /// walker over `SHOW_ELEMENT` called its filter for four elements and for
    /// none of the eleven text nodes between them.
    fn filter_node(&mut self, t: u32, node: u32) -> u16 {
        let Some(tr) = self.traversals.get(t) else {
            return FILTER_REJECT;
        };
        let what = tr.what_to_show;
        let has_filter = tr.filter.is_some();
        if what & show_bit(self.node_type(node)) == 0 {
            return FILTER_SKIP;
        }
        if !has_filter {
            return FILTER_ACCEPT;
        }
        // The filter needs `&Document` while it lives inside one. Lifting it
        // out for the call is the same move `dispatch_dom_event` makes with
        // `event_targets`; a filter that re-enters its own traversal sees no
        // filter, which is closer to Chrome's answer (it throws) than
        // recursing would be.
        let Some(mut f) = self.traversals.get_mut(t).and_then(|tr| tr.filter.take()) else {
            return FILTER_ACCEPT;
        };
        let verdict = f(self, node);
        if let Some(tr) = self.traversals.get_mut(t) {
            tr.filter = Some(f);
        }
        verdict
    }

    // ─── tree order ─────────────────────────────────────────────────────────

    /// The next node in tree order, not leaving `root`'s subtree.
    fn following_within(&self, node: u32, root: u32) -> Option<u32> {
        if let Some(first) = self.first_child(node) {
            return Some(first);
        }
        let mut cur = node;
        loop {
            if cur == root {
                return None;
            }
            let sib = self.next_sibling(cur);
            if sib != 0 {
                return Some(sib);
            }
            let parent = self.parent_node(cur);
            if parent == 0 || cur == root {
                return None;
            }
            cur = parent;
        }
    }

    /// The previous node in tree order, not leaving `root`'s subtree.
    fn preceding_within(&self, node: u32, root: u32) -> Option<u32> {
        if node == root {
            return None;
        }
        let prev = self.previous_sibling(node);
        if prev != 0 {
            return Some(self.last_inclusive_descendant(prev));
        }
        let parent = self.parent_node(node);
        (parent != 0).then_some(parent)
    }

    /// The node itself, or its deepest last descendant.
    fn last_inclusive_descendant(&self, node: u32) -> u32 {
        let mut cur = node;
        while let Some(last) = self.last_child(cur) {
            cur = last;
        }
        cur
    }

    /// Is `ancestor` `node` itself or one of its ancestors?
    fn is_inclusive_ancestor(&self, ancestor: u32, node: u32) -> bool {
        let mut cur = node;
        loop {
            if cur == ancestor {
                return true;
            }
            let parent = self.parent_node(cur);
            if parent == 0 || parent == cur {
                return false;
            }
            cur = parent;
        }
    }

    // ─── TreeWalker ─────────────────────────────────────────────────────────

    /// `walker.nextNode()`.
    pub fn tw_next_node(&mut self, t: u32) -> Option<u32> {
        let (mut node, root) = {
            let tr = self.traversals.get(t)?;
            (tr.current, tr.root)
        };
        let mut result = FILTER_ACCEPT;
        loop {
            // Descend while the last verdict did not prune the subtree.
            while result != FILTER_REJECT {
                let Some(child) = self.first_child(node) else {
                    break;
                };
                node = child;
                result = self.filter_node(t, node);
                if result == FILTER_ACCEPT {
                    self.set_current_node(t, node);
                    return Some(node);
                }
            }
            // Then the nearest following node, without leaving the root.
            let mut temp = node;
            let sibling = loop {
                if temp == root {
                    return None;
                }
                let sib = self.next_sibling(temp);
                if sib != 0 {
                    break sib;
                }
                let parent = self.parent_node(temp);
                if parent == 0 {
                    return None;
                }
                temp = parent;
            };
            node = sibling;
            result = self.filter_node(t, node);
            if result == FILTER_ACCEPT {
                self.set_current_node(t, node);
                return Some(node);
            }
        }
    }

    /// `walker.previousNode()`.
    pub fn tw_previous_node(&mut self, t: u32) -> Option<u32> {
        let (mut node, root) = {
            let tr = self.traversals.get(t)?;
            (tr.current, tr.root)
        };
        while node != root {
            let mut sibling = self.previous_sibling(node);
            while sibling != 0 {
                node = sibling;
                let mut result = self.filter_node(t, node);
                // Walk to the deepest last descendant that is not pruned.
                while result != FILTER_REJECT {
                    let Some(last) = self.last_child(node) else {
                        break;
                    };
                    node = last;
                    result = self.filter_node(t, node);
                }
                if result == FILTER_ACCEPT {
                    self.set_current_node(t, node);
                    return Some(node);
                }
                sibling = self.previous_sibling(node);
            }
            let parent = self.parent_node(node);
            if node == root || parent == 0 {
                return None;
            }
            node = parent;
            if self.filter_node(t, node) == FILTER_ACCEPT {
                self.set_current_node(t, node);
                return Some(node);
            }
        }
        None
    }

    /// `walker.parentNode()`. Leaves `currentNode` alone when it finds nothing.
    pub fn tw_parent_node(&mut self, t: u32) -> Option<u32> {
        let (mut node, root) = {
            let tr = self.traversals.get(t)?;
            (tr.current, tr.root)
        };
        while node != 0 && node != root {
            node = self.parent_node(node);
            if node != 0 && self.filter_node(t, node) == FILTER_ACCEPT {
                self.set_current_node(t, node);
                return Some(node);
            }
        }
        None
    }

    /// `walker.firstChild()`.
    pub fn tw_first_child(&mut self, t: u32) -> Option<u32> {
        self.traverse_children(t, true)
    }

    /// `walker.lastChild()`.
    pub fn tw_last_child(&mut self, t: u32) -> Option<u32> {
        self.traverse_children(t, false)
    }

    /// DOM §6.1 "traverse children".
    ///
    /// ⛔ `SKIP` descends into the skipped node's children and `REJECT` does
    /// not — measured: with `p1` skipped `firstChild()` answers `b1`, and with
    /// `p1` rejected it answers `p2`.
    fn traverse_children(&mut self, t: u32, first: bool) -> Option<u32> {
        let (start, root) = {
            let tr = self.traversals.get(t)?;
            (tr.current, tr.root)
        };
        let mut node = match if first {
            self.first_child(start)
        } else {
            self.last_child(start)
        } {
            Some(n) => n,
            None => return None,
        };
        loop {
            let result = self.filter_node(t, node);
            if result == FILTER_ACCEPT {
                self.set_current_node(t, node);
                return Some(node);
            }
            if result == FILTER_SKIP {
                let child = if first {
                    self.first_child(node)
                } else {
                    self.last_child(node)
                };
                if let Some(c) = child {
                    node = c;
                    continue;
                }
            }
            // Rejected, or skipped with nothing under it: move sideways, then
            // up — but never past the node the walk started from.
            loop {
                let sib = if first {
                    self.next_sibling(node)
                } else {
                    self.previous_sibling(node)
                };
                if sib != 0 {
                    node = sib;
                    break;
                }
                let parent = self.parent_node(node);
                if parent == 0 || parent == root || parent == start {
                    return None;
                }
                node = parent;
            }
        }
    }

    /// `walker.nextSibling()`.
    pub fn tw_next_sibling(&mut self, t: u32) -> Option<u32> {
        self.traverse_siblings(t, true)
    }

    /// `walker.previousSibling()`.
    pub fn tw_previous_sibling(&mut self, t: u32) -> Option<u32> {
        self.traverse_siblings(t, false)
    }

    /// DOM §6.1 "traverse siblings".
    fn traverse_siblings(&mut self, t: u32, next: bool) -> Option<u32> {
        let (mut node, root) = {
            let tr = self.traversals.get(t)?;
            (tr.current, tr.root)
        };
        if node == root {
            return None;
        }
        loop {
            let mut sibling = if next {
                self.next_sibling(node)
            } else {
                self.previous_sibling(node)
            };
            while sibling != 0 {
                node = sibling;
                let result = self.filter_node(t, node);
                if result == FILTER_ACCEPT {
                    self.set_current_node(t, node);
                    return Some(node);
                }
                // A skipped sibling's own children are candidates.
                let inner = if next {
                    self.first_child(node)
                } else {
                    self.last_child(node)
                };
                sibling = match inner {
                    Some(c) if result != FILTER_REJECT => c,
                    _ => {
                        if next {
                            self.next_sibling(node)
                        } else {
                            self.previous_sibling(node)
                        }
                    }
                };
            }
            let parent = self.parent_node(node);
            if parent == 0 || parent == root {
                return None;
            }
            node = parent;
            // Climbing past an ACCEPTED ancestor would return a node that is
            // not a sibling of anything the caller asked about.
            if self.filter_node(t, node) == FILTER_ACCEPT {
                return None;
            }
        }
    }

    // ─── NodeIterator ───────────────────────────────────────────────────────

    /// `iterator.nextNode()`.
    pub fn ni_next_node(&mut self, t: u32) -> Option<u32> {
        self.ni_step(t, true)
    }

    /// `iterator.previousNode()`.
    pub fn ni_previous_node(&mut self, t: u32) -> Option<u32> {
        self.ni_step(t, false)
    }

    /// DOM §6.1 "traverse", both directions.
    ///
    /// ⛔ `REJECT` is treated exactly as `SKIP` here. An iterator has no
    /// subtree to prune — it walks the flat tree order — so the two verdicts
    /// cannot differ (measured: rejecting `p1` still yields its child `b1`).
    fn ni_step(&mut self, t: u32, forward: bool) -> Option<u32> {
        let (mut node, mut before, root) = {
            let tr = self.traversals.get(t)?;
            (tr.current, tr.pointer_before, tr.root)
        };
        loop {
            if forward {
                if before {
                    before = false;
                } else {
                    node = self.following_within(node, root)?;
                }
            } else if !before {
                before = true;
            } else {
                node = self.preceding_within(node, root)?;
            }
            if self.filter_node(t, node) == FILTER_ACCEPT {
                break;
            }
        }
        if let Some(tr) = self.traversals.get_mut(t) {
            tr.current = node;
            tr.pointer_before = before;
        }
        Some(node)
    }

    /// DOM §6.1's **NodeIterator pre-removing steps**, run for every live
    /// iterator before `node` leaves the tree.
    ///
    /// ⛔ The rule is about `node` being an INCLUSIVE ANCESTOR of the
    /// reference, not about it being the reference. Measured across five
    /// shapes: removing an ancestor moves the reference (`b1` → `r`), removing
    /// a preceding sibling that is not an ancestor leaves it alone, and the
    /// branch taken depends on `pointerBeforeReferenceNode` — with the pointer
    /// before, the reference moves FORWARD past the removed subtree instead.
    pub fn run_pre_removing_steps(&mut self, node: u32) {
        for t in self.traversals.ids() {
            let Some(tr) = self.traversals.get(t) else {
                continue;
            };
            if tr.kind != TraversalKind::NodeIterator {
                continue;
            }
            let (root, reference, before) = (tr.root, tr.current, tr.pointer_before);
            if node == root || !self.is_inclusive_ancestor(node, reference) {
                continue;
            }
            if before {
                // The first node after the removed subtree that is still
                // inside the root.
                let mut next = self.following_within(node, root);
                while let Some(n) = next {
                    if !self.is_inclusive_ancestor(node, n) {
                        break;
                    }
                    next = self.following_within(n, root);
                }
                if let Some(n) = next {
                    if let Some(tr) = self.traversals.get_mut(t) {
                        tr.current = n;
                    }
                    continue;
                }
                if let Some(tr) = self.traversals.get_mut(t) {
                    tr.pointer_before = false;
                }
            }
            // The node immediately before the removed one.
            let prev = self.previous_sibling(node);
            let new_ref = if prev != 0 {
                self.last_inclusive_descendant(prev)
            } else {
                self.parent_node(node)
            };
            if let Some(tr) = self.traversals.get_mut(t) {
                tr.current = new_ref;
            }
        }
    }
}
