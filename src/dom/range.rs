//! `Range` — DOM §5.
//!
//! A pair of boundary points that the tree keeps up to date underneath them.
//! Every algorithm here was measured against Chrome (`/tmp/webcore-html/rg1`,
//! `rg2`, `rg3`), and two of the measurements contradicted the obvious reading:
//!
//!   * `insertNode` into a NON-collapsed range leaves the start in the
//!     shortened original text node — `("H", 1)` — not in the parent at the
//!     insertion index, which is what the offsets alone suggest.
//!   * `deleteContents` across two text nodes leaves TWO adjacent text nodes,
//!     not one merged node. Nothing normalizes; `"He"` and `"il"` render
//!     contiguously and are still two children.
//!
//! Ranges are handles into a store on `Document`, the same shape as the
//! traversals — which is what lets the tree tell every live range that it
//! moved. Unlike a traversal a range holds no callback, so the store DOES
//! survive a document clone.

use crate::types::Document;
use std::cmp::Ordering;
use std::collections::HashMap;

/// `compareBoundaryPoints` how-values (DOM §5.2).
pub const START_TO_START: u16 = 0;
pub const START_TO_END: u16 = 1;
pub const END_TO_END: u16 = 2;
pub const END_TO_START: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeState {
    pub start_container: u32,
    pub start_offset: usize,
    pub end_container: u32,
    pub end_offset: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RangeStore {
    map: HashMap<u32, RangeState>,
    next_id: u32,
}

impl RangeStore {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    /// Every NODE this store keeps a reference to.
    ///
    /// ⛔ Not the map's keys — those are RANGE handles. The node ids are the
    /// two containers inside each value. See `TraversalStore::node_ids`.
    pub fn node_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.map
            .values()
            .flat_map(|r| [r.start_container, r.end_container])
    }

    pub fn insert(&mut self, r: RangeState) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.map.insert(id, r);
        id
    }

    pub fn get(&self, id: u32) -> Option<RangeState> {
        self.map.get(&id).copied()
    }
    pub fn set(&mut self, id: u32, r: RangeState) {
        self.map.insert(id, r);
    }
    pub fn ids(&self) -> Vec<u32> {
        self.map.keys().copied().collect()
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Document {
    /// `document.createRange()`. A fresh range is collapsed on the document
    /// itself (measured: `["#document", 0, "#document", 0, true]`).
    pub fn create_range(&mut self) -> u32 {
        let doc = self.document_node();
        self.ranges.insert(RangeState {
            start_container: doc,
            start_offset: 0,
            end_container: doc,
            end_offset: 0,
        })
    }

    pub fn range_start_container(&self, r: u32) -> Option<u32> {
        self.ranges.get(r).map(|s| s.start_container)
    }
    pub fn range_start_offset(&self, r: u32) -> Option<usize> {
        self.ranges.get(r).map(|s| s.start_offset)
    }
    pub fn range_end_container(&self, r: u32) -> Option<u32> {
        self.ranges.get(r).map(|s| s.end_container)
    }
    pub fn range_end_offset(&self, r: u32) -> Option<usize> {
        self.ranges.get(r).map(|s| s.end_offset)
    }

    /// `range.collapsed` — both boundary points are the same point.
    pub fn range_collapsed(&self, r: u32) -> bool {
        self.ranges
            .get(r)
            .is_some_and(|s| s.start_container == s.end_container && s.start_offset == s.end_offset)
    }

    /// A node's **length** (DOM §4.4): character data counts its data in
    /// UTF-16 units, everything else counts its children.
    pub fn node_length(&self, id: u32) -> usize {
        match self.node_type(id) {
            3 | 4 | 8 => self.character_data_length(id),
            _ => self.child_nodes(id).len(),
        }
    }

    /// A node's index among its parent's children, or 0 when it has no parent.
    fn node_index(&self, id: u32) -> usize {
        let parent = self.parent_node(id);
        if parent == 0 {
            return 0;
        }
        self.child_nodes(parent)
            .iter()
            .position(|c| *c == id)
            .unwrap_or(0)
    }

    /// The root of `id`'s tree — the topmost node reachable by parents.
    fn tree_root(&self, id: u32) -> u32 {
        let mut cur = id;
        loop {
            let parent = self.parent_node(cur);
            if parent == 0 || parent == cur {
                return cur;
            }
            cur = parent;
        }
    }

    /// DOM §4.4's "position of boundary point A relative to boundary point B".
    ///
    /// `None` when the two are in different trees, which is the
    /// `WrongDocumentError` the comparing members throw.
    fn compare_points(&self, a: u32, a_off: usize, b: u32, b_off: usize) -> Option<Ordering> {
        if a == b {
            return Some(a_off.cmp(&b_off));
        }
        // Two roots have no order between them. `diverging_children` below
        // also answers `None` for that case, so this is an explicit statement
        // of the rule rather than the only thing enforcing it — which is why a
        // mutation removing it stays green while the BEHAVIOUR is still
        // pinned by `comparing_two_ranges_in_different_trees_...`.
        if self.tree_root(a) != self.tree_root(b) {
            return None;
        }
        // DOM §4.4 step 4: when A is an ANCESTOR of B, the answer turns on
        // whether the branch B sits in starts before A's offset — a child
        // index below the offset puts A after B, not before it.
        if self.is_ancestor_of(a, b) {
            let child = self.ancestor_child_of(b, a)?;
            return Some(if self.node_index(child) < a_off {
                Ordering::Greater
            } else {
                Ordering::Less
            });
        }
        // The mirror, inverted: ask where B sits relative to A and flip it.
        if self.is_ancestor_of(b, a) {
            let child = self.ancestor_child_of(a, b)?;
            return Some(if self.node_index(child) < b_off {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        // Siblings by tree order: walk up to the common ancestor and compare
        // the two branches' indices.
        let (ca, cb) = self.diverging_children(a, b)?;
        Some(self.node_index(ca).cmp(&self.node_index(cb)))
    }

    fn is_ancestor_of(&self, ancestor: u32, node: u32) -> bool {
        if ancestor == node {
            return false;
        }
        let mut cur = node;
        loop {
            let parent = self.parent_node(cur);
            if parent == 0 || parent == cur {
                return false;
            }
            if parent == ancestor {
                return true;
            }
            cur = parent;
        }
    }

    /// The ancestor of `node` that is a direct child of `ancestor`.
    fn ancestor_child_of(&self, node: u32, ancestor: u32) -> Option<u32> {
        let mut cur = node;
        loop {
            let parent = self.parent_node(cur);
            if parent == 0 {
                return None;
            }
            if parent == ancestor {
                return Some(cur);
            }
            cur = parent;
        }
    }

    /// The two children of the nearest common ancestor that `a` and `b` sit
    /// under — the pair whose indices decide tree order.
    fn diverging_children(&self, a: u32, b: u32) -> Option<(u32, u32)> {
        let chain_a = self.ancestor_chain(a);
        let chain_b = self.ancestor_chain(b);
        for (i, x) in chain_a.iter().enumerate() {
            if let Some(j) = chain_b.iter().position(|y| y == x) {
                // The children one step below the common ancestor.
                let ca = if i == 0 { a } else { chain_a[i - 1] };
                let cb = if j == 0 { b } else { chain_b[j - 1] };
                return Some((ca, cb));
            }
        }
        None
    }

    /// `node`'s ancestors, nearest first.
    fn ancestor_chain(&self, node: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = node;
        loop {
            let parent = self.parent_node(cur);
            if parent == 0 || parent == cur {
                break;
            }
            out.push(parent);
            cur = parent;
        }
        out
    }

    // ─── setting the boundary points ────────────────────────────────────────

    /// `range.setStart(node, offset)`. `false` is `IndexSizeError`.
    ///
    /// ⛔ Setting a boundary in a DIFFERENT tree moves the WHOLE range there,
    /// collapsed on the new point — it does not leave a range straddling two
    /// documents (measured).
    pub fn range_set_start(&mut self, r: u32, node: u32, offset: usize) -> bool {
        self.set_boundary(r, node, offset, true)
    }

    /// `range.setEnd(node, offset)`.
    pub fn range_set_end(&mut self, r: u32, node: u32, offset: usize) -> bool {
        self.set_boundary(r, node, offset, false)
    }

    fn set_boundary(&mut self, r: u32, node: u32, offset: usize, start: bool) -> bool {
        let Some(mut s) = self.ranges.get(r) else {
            return false;
        };
        if offset > self.node_length(node) {
            return false;
        }
        let other_root = self.tree_root(if start {
            s.end_container
        } else {
            s.start_container
        });
        let collapse = self.tree_root(node) != other_root
            || if start {
                self.compare_points(node, offset, s.end_container, s.end_offset)
                    == Some(Ordering::Greater)
            } else {
                self.compare_points(node, offset, s.start_container, s.start_offset)
                    == Some(Ordering::Less)
            };
        if start {
            s.start_container = node;
            s.start_offset = offset;
            if collapse {
                s.end_container = node;
                s.end_offset = offset;
            }
        } else {
            s.end_container = node;
            s.end_offset = offset;
            if collapse {
                s.start_container = node;
                s.start_offset = offset;
            }
        }
        self.ranges.set(r, s);
        true
    }

    /// `range.setStartBefore(node)`.
    pub fn range_set_start_before(&mut self, r: u32, node: u32) -> bool {
        let parent = self.parent_node(node);
        parent != 0 && self.range_set_start(r, parent, self.node_index(node))
    }

    /// `range.setStartAfter(node)`.
    pub fn range_set_start_after(&mut self, r: u32, node: u32) -> bool {
        let parent = self.parent_node(node);
        parent != 0 && self.range_set_start(r, parent, self.node_index(node) + 1)
    }

    /// `range.setEndBefore(node)`.
    pub fn range_set_end_before(&mut self, r: u32, node: u32) -> bool {
        let parent = self.parent_node(node);
        parent != 0 && self.range_set_end(r, parent, self.node_index(node))
    }

    /// `range.setEndAfter(node)`.
    pub fn range_set_end_after(&mut self, r: u32, node: u32) -> bool {
        let parent = self.parent_node(node);
        parent != 0 && self.range_set_end(r, parent, self.node_index(node) + 1)
    }

    /// `range.collapse(toStart)`.
    ///
    /// ⛔ The default is the END, not the start — `collapse()` with no
    /// argument is `collapse(false)` (measured: it lands on the end offset).
    pub fn range_collapse(&mut self, r: u32, to_start: bool) {
        let Some(mut s) = self.ranges.get(r) else {
            return;
        };
        if to_start {
            s.end_container = s.start_container;
            s.end_offset = s.start_offset;
        } else {
            s.start_container = s.end_container;
            s.start_offset = s.end_offset;
        }
        self.ranges.set(r, s);
    }

    /// `range.selectNode(node)` — the range spans the node itself, so both
    /// boundaries sit in its PARENT.
    pub fn range_select_node(&mut self, r: u32, node: u32) -> bool {
        let parent = self.parent_node(node);
        if parent == 0 {
            return false;
        }
        let index = self.node_index(node);
        self.ranges.set(
            r,
            RangeState {
                start_container: parent,
                start_offset: index,
                end_container: parent,
                end_offset: index + 1,
            },
        );
        true
    }

    /// `range.selectNodeContents(node)` — everything inside it.
    pub fn range_select_node_contents(&mut self, r: u32, node: u32) -> bool {
        if self.ranges.get(r).is_none() {
            return false;
        }
        let len = self.node_length(node);
        self.ranges.set(
            r,
            RangeState {
                start_container: node,
                start_offset: 0,
                end_container: node,
                end_offset: len,
            },
        );
        true
    }

    /// `range.cloneRange()` — an independent copy.
    pub fn range_clone(&mut self, r: u32) -> Option<u32> {
        let s = self.ranges.get(r)?;
        Some(self.ranges.insert(s))
    }

    /// `range.detach()` — defined as doing nothing (measured: the range keeps
    /// working afterwards).
    pub fn range_detach(&self, _r: u32) {}

    // ─── asking about the range ─────────────────────────────────────────────

    /// `range.commonAncestorContainer`.
    pub fn common_ancestor_container(&self, r: u32) -> Option<u32> {
        let s = self.ranges.get(r)?;
        if s.start_container == s.end_container {
            return Some(s.start_container);
        }
        let mut chain = self.ancestor_chain(s.start_container);
        chain.insert(0, s.start_container);
        let mut end_chain = self.ancestor_chain(s.end_container);
        end_chain.insert(0, s.end_container);
        chain.into_iter().find(|a| end_chain.contains(a))
    }

    /// `range.compareBoundaryPoints(how, other)`.
    ///
    /// ⛔ `START_TO_END` compares THIS range's END to the other's START, and
    /// `END_TO_START` compares this range's START to the other's END — the
    /// names read in the opposite order to the operands. `None` is the
    /// `NotSupportedError` for an unknown `how` and the `WrongDocumentError`
    /// for two different trees.
    pub fn compare_boundary_points(&self, r: u32, how: u16, other: u32) -> Option<i8> {
        let a = self.ranges.get(r)?;
        let b = self.ranges.get(other)?;
        let (an, ao, bn, bo) = match how {
            START_TO_START => (
                a.start_container,
                a.start_offset,
                b.start_container,
                b.start_offset,
            ),
            START_TO_END => (
                a.end_container,
                a.end_offset,
                b.start_container,
                b.start_offset,
            ),
            END_TO_END => (a.end_container, a.end_offset, b.end_container, b.end_offset),
            END_TO_START => (
                a.start_container,
                a.start_offset,
                b.end_container,
                b.end_offset,
            ),
            _ => return None,
        };
        Some(match self.compare_points(an, ao, bn, bo)? {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        })
    }

    /// `range.comparePoint(node, offset)` — `-1` before, `0` inside, `1` after.
    pub fn range_compare_point(&self, r: u32, node: u32, offset: usize) -> Option<i8> {
        let s = self.ranges.get(r)?;
        if self.tree_root(node) != self.tree_root(s.start_container) {
            return None;
        }
        if offset > self.node_length(node) {
            return None;
        }
        if self.compare_points(node, offset, s.start_container, s.start_offset)? == Ordering::Less {
            return Some(-1);
        }
        if self.compare_points(node, offset, s.end_container, s.end_offset)? == Ordering::Greater {
            return Some(1);
        }
        Some(0)
    }

    /// `range.isPointInRange(node, offset)`.
    ///
    /// Inclusive of BOTH boundaries — a point exactly on the start or the end
    /// is in the range (measured).
    pub fn is_point_in_range(&self, r: u32, node: u32, offset: usize) -> bool {
        self.range_compare_point(r, node, offset) == Some(0)
    }

    /// `range.intersectsNode(node)`.
    pub fn range_intersects_node(&self, r: u32, node: u32) -> bool {
        let Some(s) = self.ranges.get(r) else {
            return false;
        };
        if self.tree_root(node) != self.tree_root(s.start_container) {
            return false;
        }
        let parent = self.parent_node(node);
        if parent == 0 {
            return true;
        }
        let index = self.node_index(node);
        let before_end = self.compare_points(parent, index, s.end_container, s.end_offset)
            == Some(Ordering::Less);
        let after_start = self.compare_points(parent, index + 1, s.start_container, s.start_offset)
            == Some(Ordering::Greater);
        before_end && after_start
    }

    /// `range.toString()` — the text between the boundaries.
    pub fn range_to_string(&self, r: u32) -> String {
        let Some(s) = self.ranges.get(r) else {
            return String::new();
        };
        if self.node_type(s.start_container) == 3 && s.start_container == s.end_container {
            return substring16(
                &self.text_data(s.start_container),
                s.start_offset,
                s.end_offset,
            );
        }
        let mut out = String::new();
        if self.node_type(s.start_container) == 3 {
            let data = self.text_data(s.start_container);
            let len = data.encode_utf16().count();
            out.push_str(&substring16(&data, s.start_offset, len));
        }
        for node in self.contained_text_nodes(&s) {
            out.push_str(&self.text_data(node));
        }
        if self.node_type(s.end_container) == 3 {
            out.push_str(&substring16(
                &self.text_data(s.end_container),
                0,
                s.end_offset,
            ));
        }
        out
    }

    /// Every text node WHOLLY inside the range, in tree order — the two
    /// partially-covered ends are handled by the caller.
    fn contained_text_nodes(&self, s: &RangeState) -> Vec<u32> {
        self.nodes_in_tree_order(s)
            .into_iter()
            .filter(|n| {
                *n != s.start_container
                    && *n != s.end_container
                    && self.node_type(*n) == 3
                    && self.is_contained(*n, s)
            })
            .collect()
    }

    /// Every node between the boundary points, in tree order.
    fn nodes_in_tree_order(&self, s: &RangeState) -> Vec<u32> {
        let Some(root) = self.common_ancestor_of(s) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        self.collect_descendants(root, &mut out);
        out
    }

    fn common_ancestor_of(&self, s: &RangeState) -> Option<u32> {
        if s.start_container == s.end_container {
            return Some(s.start_container);
        }
        let mut chain = self.ancestor_chain(s.start_container);
        chain.insert(0, s.start_container);
        let mut end_chain = self.ancestor_chain(s.end_container);
        end_chain.insert(0, s.end_container);
        chain.into_iter().find(|a| end_chain.contains(a))
    }

    fn collect_descendants(&self, node: u32, out: &mut Vec<u32>) {
        for child in self.child_nodes(node) {
            out.push(child);
            self.collect_descendants(child, out);
        }
    }

    /// Is `node` wholly inside the range? DOM §5's "contained": its start
    /// point is after the range's start and its end point before the range's
    /// end.
    fn is_contained(&self, node: u32, s: &RangeState) -> bool {
        let parent = self.parent_node(node);
        if parent == 0 {
            return false;
        }
        let index = self.node_index(node);
        let after_start = matches!(
            self.compare_points(parent, index, s.start_container, s.start_offset),
            Some(Ordering::Greater) | Some(Ordering::Equal)
        );
        let before_end = matches!(
            self.compare_points(parent, index + 1, s.end_container, s.end_offset),
            Some(Ordering::Less) | Some(Ordering::Equal)
        );
        after_start && before_end
    }
}

/// A UTF-16 substring, which is what every DOM offset counts in.
fn substring16(s: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let start = start.min(units.len());
    let end = end.clamp(start, units.len());
    String::from_utf16_lossy(&units[start..end])
}

// ─── the mutating members (DOM §5.2) ────────────────────────────────────────

impl Document {
    /// `range.deleteContents()`.
    ///
    /// ⛔ Nothing is normalized afterwards. Deleting across two text nodes
    /// leaves TWO adjacent text nodes — `"He"` and `"il"` — which render
    /// contiguously and are still two children (measured: `childNodes.length`
    /// is 2, not 1). Merging them would pass every `toString` test and be
    /// wrong.
    pub fn delete_contents(&mut self, r: u32) {
        let Some(s) = self.ranges.get(r) else { return };
        if s.start_container == s.end_container && s.start_offset == s.end_offset {
            return;
        }
        // A range inside ONE text node is a data edit, not a tree edit.
        if s.start_container == s.end_container && self.node_type(s.start_container) == 3 {
            self.replace_data(
                s.start_container,
                s.start_offset,
                s.end_offset - s.start_offset,
                "",
            );
            return;
        }
        let doomed = self.contained_nodes_to_remove(&s);
        // Trim the two partially covered ends BEFORE removing anything, so the
        // offsets still mean what they meant when the range was set.
        if self.node_type(s.end_container) == 3 {
            self.replace_data(s.end_container, 0, s.end_offset, "");
        }
        if self.node_type(s.start_container) == 3 {
            let len = self.character_data_length(s.start_container);
            self.replace_data(s.start_container, s.start_offset, len - s.start_offset, "");
        }
        for node in doomed {
            self.remove_child(node);
        }
        self.collapse_to_start(r, &s);
    }

    /// `range.extractContents()` — remove the contents and return them as a
    /// `DocumentFragment`.
    pub fn extract_contents(&mut self, r: u32) -> Option<u32> {
        let frag = self.clone_contents(r)?;
        self.delete_contents(r);
        Some(frag)
    }

    /// `range.cloneContents()` — the same content, with the tree left alone.
    ///
    /// ⛔ A partially covered ancestor is CLONED into the fragment while the
    /// original stays put, attributes and all: extracting across two
    /// paragraphs yields `<p id="p1">llo<b id="b1">World</b>Tail</p><p
    /// id="p2">Sec</p>` — two elements carrying ids that also exist in the
    /// document.
    pub fn clone_contents(&mut self, r: u32) -> Option<u32> {
        let s = self.ranges.get(r)?;
        let frag = self.create_document_fragment();
        if s.start_container == s.end_container && s.start_offset == s.end_offset {
            return Some(frag);
        }
        if s.start_container == s.end_container && self.node_type(s.start_container) == 3 {
            let text = substring16(
                &self.text_data(s.start_container),
                s.start_offset,
                s.end_offset,
            );
            let t = self.create_text_node(&text);
            self.append_child(frag, t);
            return Some(frag);
        }
        let common = self.common_ancestor_of(&s)?;
        for child in self.child_nodes(common) {
            if let Some(piece) = self.clone_piece(child, &s) {
                self.append_child(frag, piece);
            }
        }
        Some(frag)
    }

    /// The part of `node` that falls inside the range, as a fresh subtree.
    ///
    /// `None` when the node is wholly outside. A node wholly inside is deep
    /// cloned; one that merely OVERLAPS is cloned shallowly and recursed into,
    /// which is what puts a partially covered `<p>` in the fragment with only
    /// the covered half of its children.
    fn clone_piece(&mut self, node: u32, s: &RangeState) -> Option<u32> {
        if self.is_contained(node, s) {
            return Some(self.clone_node(node, true));
        }
        let touches = self.range_touches(node, s);
        if !touches {
            return None;
        }
        if self.node_type(node) == 3 {
            let data = self.text_data(node);
            let len = data.encode_utf16().count();
            let from = if node == s.start_container {
                s.start_offset
            } else {
                0
            };
            let to = if node == s.end_container {
                s.end_offset
            } else {
                len
            };
            if from >= to {
                return None;
            }
            let text = substring16(&data, from, to);
            return Some(self.create_text_node(&text));
        }
        let shallow = self.clone_node(node, false);
        for child in self.child_nodes(node) {
            if let Some(piece) = self.clone_piece(child, s) {
                self.append_child(shallow, piece);
            }
        }
        Some(shallow)
    }

    /// Does any part of `node` fall between the boundary points?
    fn range_touches(&self, node: u32, s: &RangeState) -> bool {
        if node == s.start_container || node == s.end_container {
            return true;
        }
        if self.is_ancestor_of(node, s.start_container)
            || self.is_ancestor_of(node, s.end_container)
        {
            return true;
        }
        self.is_contained(node, s)
    }

    /// The nodes `deleteContents` should remove outright: wholly contained,
    /// and not inside another node already being removed.
    fn contained_nodes_to_remove(&self, s: &RangeState) -> Vec<u32> {
        let all = self.nodes_in_tree_order(s);
        let contained: Vec<u32> = all
            .into_iter()
            .filter(|n| {
                *n != s.start_container && *n != s.end_container && self.is_contained(*n, s)
            })
            .filter(|n| !self.is_ancestor_of(*n, s.start_container))
            .filter(|n| !self.is_ancestor_of(*n, s.end_container))
            .collect();
        // Only the topmost of each removed subtree — removing a parent takes
        // its children with it, and removing a child first would renumber the
        // indices the parent's own removal depends on.
        contained
            .iter()
            .copied()
            .filter(|n| !contained.iter().any(|o| self.is_ancestor_of(*o, *n)))
            .collect()
    }

    fn collapse_to_start(&mut self, r: u32, s: &RangeState) {
        // The start point survives the deletion; the end folds onto it. The
        // live-update hooks have already moved the offsets, so this reads the
        // CURRENT state rather than the one captured before the edit.
        let Some(now) = self.ranges.get(r) else {
            return;
        };
        let _ = s;
        self.ranges.set(
            r,
            RangeState {
                start_container: now.start_container,
                start_offset: now.start_offset,
                end_container: now.start_container,
                end_offset: now.start_offset,
            },
        );
    }

    /// `range.insertNode(node)`.
    ///
    /// ⛔ Into a NON-collapsed range whose start is mid-text, the start stays
    /// in the SHORTENED original text node — `("H", 1)` — rather than moving
    /// to the parent at the insertion index. The offsets alone (`[1, 2]`) read
    /// the other way; only printing the containers shows it.
    pub fn insert_node(&mut self, r: u32, node: u32) -> bool {
        let Some(s) = self.ranges.get(r) else {
            return false;
        };
        let container = s.start_container;
        let (parent, index) = if self.node_type(container) == 3 {
            // Split, and insert between the two halves.
            let parent = self.parent_node(container);
            if parent == 0 {
                return false;
            }
            if s.start_offset == 0 {
                (parent, self.node_index(container))
            } else if s.start_offset >= self.character_data_length(container) {
                (parent, self.node_index(container) + 1)
            } else {
                match self.split_text(container, s.start_offset) {
                    Some(_) => (parent, self.node_index(container) + 1),
                    None => return false,
                }
            }
        } else {
            (container, s.start_offset)
        };
        let children = self.child_nodes(parent);
        match children.get(index) {
            Some(reference) => self.insert_before(parent, node, *reference),
            None => self.append_child(parent, node),
        }
        true
    }

    /// `range.surroundContents(node)`.
    ///
    /// `false` is the `InvalidStateError` the spec throws when the range
    /// partially contains a non-text node — measured on a range running from
    /// inside one text node into a `<b>`'s text.
    pub fn surround_contents(&mut self, r: u32, node: u32) -> bool {
        let Some(s) = self.ranges.get(r) else {
            return false;
        };
        // Partially contained non-text node → InvalidStateError.
        for n in self.nodes_in_tree_order(&s) {
            if self.node_type(n) == 3 {
                continue;
            }
            let partially = (self.is_ancestor_of(n, s.start_container)
                || self.is_ancestor_of(n, s.end_container))
                && !self.is_contained(n, &s);
            if partially {
                return false;
            }
        }
        // "Replace all with null within node" — an existing child of the
        // wrapper is discarded (measured: a `<u>` carrying "old" loses it).
        for child in self.child_nodes(node) {
            self.remove_child(child);
        }
        let Some(frag) = self.extract_contents(r) else {
            return false;
        };
        if !self.insert_node(r, node) {
            return false;
        }
        for child in self.child_nodes(frag) {
            self.append_child(node, child);
        }
        self.range_select_node_contents(r, node);
        true
    }
}

// ─── live updating (DOM §4.2.3 and §4.10) ───────────────────────────────────
//
// Three hooks, because the tree changes underneath a range in three ways. Each
// moves the offsets differently, and only the removal case can change the
// CONTAINER — which is why all four measured fixtures are kept as tests.

impl Document {
    /// A node was inserted into `parent` at `index`.
    pub(crate) fn ranges_after_insert(&mut self, parent: u32, index: usize) {
        if self.suppress_range_updates {
            return;
        }
        for r in self.ranges.ids() {
            let Some(mut s) = self.ranges.get(r) else {
                continue;
            };
            if s.start_container == parent && s.start_offset > index {
                s.start_offset += 1;
            }
            if s.end_container == parent && s.end_offset > index {
                s.end_offset += 1;
            }
            self.ranges.set(r, s);
        }
    }

    /// `node` is about to be removed from `parent`, where it sits at `index`.
    ///
    /// ⛔ Runs BEFORE the removal, like the iterator's pre-removing steps: a
    /// range whose container is inside the doomed subtree has to be moved to
    /// `(parent, index)`, and neither is reachable once the links are cut.
    pub(crate) fn ranges_before_remove(&mut self, node: u32, parent: u32, index: usize) {
        if self.suppress_range_updates {
            return;
        }
        for r in self.ranges.ids() {
            let Some(mut s) = self.ranges.get(r) else {
                continue;
            };
            if s.start_container == node || self.is_ancestor_of(node, s.start_container) {
                s.start_container = parent;
                s.start_offset = index;
            } else if s.start_container == parent && s.start_offset > index {
                s.start_offset -= 1;
            }
            if s.end_container == node || self.is_ancestor_of(node, s.end_container) {
                s.end_container = parent;
                s.end_offset = index;
            } else if s.end_container == parent && s.end_offset > index {
                s.end_offset -= 1;
            }
            self.ranges.set(r, s);
        }
    }

    /// `replaceData(node, offset, count, data)` ran.
    ///
    /// A boundary PAST the replaced run slides by the size change; one INSIDE
    /// it collapses onto the run's start. Measured through all four mutators —
    /// `insertData`, `deleteData`, `appendData` and the `data` setter, which
    /// is `replaceData(0, length, …)` and is why assigning a whole new string
    /// drove a `(1, 4)` range to `(0, 0)`.
    pub(crate) fn ranges_after_replace_data(
        &mut self,
        node: u32,
        offset: usize,
        count: usize,
        data_len: usize,
    ) {
        if self.suppress_range_updates {
            return;
        }
        let shift = |v: usize| -> usize {
            if v > offset + count {
                (v + data_len).saturating_sub(count)
            } else if v > offset {
                offset
            } else {
                v
            }
        };
        for r in self.ranges.ids() {
            let Some(mut s) = self.ranges.get(r) else {
                continue;
            };
            if s.start_container == node {
                s.start_offset = shift(s.start_offset);
            }
            if s.end_container == node {
                s.end_offset = shift(s.end_offset);
            }
            self.ranges.set(r, s);
        }
    }

    /// `splitText(node, offset)` ran, producing `new_node`.
    pub(crate) fn ranges_after_split_text(&mut self, node: u32, offset: usize, new_node: u32) {
        for r in self.ranges.ids() {
            let Some(mut s) = self.ranges.get(r) else {
                continue;
            };
            if s.start_container == node && s.start_offset > offset {
                s.start_container = new_node;
                s.start_offset -= offset;
            }
            if s.end_container == node && s.end_offset > offset {
                s.end_container = new_node;
                s.end_offset -= offset;
            }
            self.ranges.set(r, s);
        }
    }
}
