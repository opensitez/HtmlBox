//! The WHATWG DOM, as this browser implements it.
//!
//! Method names and shapes are the IDL's — `createElement`, `getElementById`,
//! `cloneNode` — not a spelling of our own. That is the whole point: the same
//! surface is implemented by `vybe_widgets`, and later by a binding to a real
//! browser, so which engine is compiled in is a build-time choice and nothing
//! above this layer knows the difference.
//!
//! Node handles are `u64` here and `u32` in the arena. The public surface takes
//! the wider type because that is what the DOM interface uses everywhere;
//! narrowing happens at the boundary, so the arena is untouched.
//!
//! Every mutation goes through these methods, which update both the arena and the
//! WebCore tree (bridge period), and set dirty flags for incremental re-style/layout.

use crate::dom::attrs::Attr;
use crate::dom::token_list::{TokenList, TokenListMut};
use crate::html::validity::ValidityState;
use crate::types::{Document, WebCore, Rect};
use crate::dom::arena::NodeId;
use crate::css::apply_property;

// ─── Query ──────────────────────────────────────────────────────────────────

impl Document {
    /// Find element by its HTML `id` attribute. Returns stable node_id.
    pub fn get_element_by_id(&self, id: &str) -> Option<u32> {
        fn walk(node: &WebCore, id: &str) -> Option<u32> {
            if node.attributes.get("id").map(|s| s.as_str()) == Some(id) {
                return Some(node.node_id);
            }
            for child in &node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
        }
        walk(&self.root, id)
    }

    /// Query for the first element matching a CSS selector.
    pub fn query_selector(&self, selector: &str) -> Option<u32> {
        self.run_query(selector, true).first().copied()
    }

    /// Query for all elements matching a CSS selector.
    pub fn query_selector_all(&self, selector: &str) -> Vec<u32> {
        self.run_query(selector, false)
    }

    /// Shared body of `querySelector` / `querySelectorAll`.
    ///
    /// The document element is both a CANDIDATE and an ANCESTOR, and the walk
    /// used to treat it as neither — it started at the root's children with an
    /// empty ancestor chain, so `querySelector("html")` found nothing and
    /// `querySelector("html body")` had no `html` to match against.
    fn run_query(&self, selector: &str, first_only: bool) -> Vec<u32> {
        matching_ids_from(&self.root, selector, first_only)
    }
}

/// Ids of the elements in `root`'s tree (including `root`) that match
/// `selector`, in document order.
///
/// This is THE selector query for the DOM surface. `dom::query_selector` and
/// friends used to run a ten-line matcher that understood `#id`, `.class`,
/// `tag` and `*` and nothing else, silently answering "no match" for every
/// combinator, pseudo-class and attribute selector — so
/// `element.matches("div p")` was false on a `<p>` inside a `<div>`, and the
/// headless browser's `find` reported an empty result for `table tbody`.
/// One question, one answer.
pub fn matching_ids_from(root: &WebCore, selector: &str, first_only: bool) -> Vec<u32> {
    {
        let selectors = parse_comma_selectors(selector);
        let empty_hover = std::collections::HashSet::new();
        let mut results = Vec::new();
        if root.is_element() && root.node_id != 0 {
            let ctx = crate::css::MatchContext {
                focused_box: 0,
                keyboard_focus: false,
                type_child_index: 0,
                type_sibling_count: 1,
                html_box: Some(root),
                hover_chain: &empty_hover,
                element_id: root.node_id,
                prev_siblings: &[],
            };
            for sel in &selectors {
                if crate::css::matches_selector_with_ancestors(
                    &sel.parts, &root.tag, &root.attributes, 0, 1, &[], &ctx,
                ) {
                    results.push(root.node_id);
                    if first_only { return results; }
                    break;
                }
            }
        }
        let root_chain = [build_ancestor_entry(root, 0, 1, 0, 1)];
        query_walk(root, &root_chain, &selectors, &empty_hover, first_only, &mut results);
        results
    }
}

/// Split comma-separated selectors and parse each one.
fn parse_comma_selectors(selector: &str) -> Vec<crate::css::CssSelector> {
    selector.split(',')
        .map(|s| crate::css::parse_selector(s.trim()))
        .collect()
}

/// Per-child positional facts a selector can ask about.
///
/// The cascade computes all four for every element it styles; the query walk
/// used to hardcode `type_child_index: 0, type_sibling_count: 1` and pass an
/// empty `prev_siblings`, which silently broke `:nth-of-type`, `:first-of-type`
/// and BOTH sibling combinators — `querySelectorAll("p + p")` answered 0 on a
/// document with two adjacent paragraphs.
struct ChildPositions {
    /// Index among ELEMENT siblings (what `:nth-child` counts).
    elem_index: Vec<usize>,
    elem_count: usize,
    type_index: Vec<usize>,
    type_count: Vec<usize>,
}

fn child_positions(children: &[WebCore]) -> ChildPositions {
    let mut running: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let tags: Vec<String> = children.iter().map(|c| c.tag.to_ascii_lowercase()).collect();
    let type_index: Vec<usize> = tags.iter().map(|t| {
        let slot = running.entry(t.clone()).or_insert(0);
        let i = *slot; *slot += 1; i
    }).collect();
    let type_count: Vec<usize> = tags.iter().map(|t| *running.get(t).unwrap_or(&0)).collect();
    let mut pos = 0usize;
    let elem_index: Vec<usize> = children.iter()
        .map(|c| if c.is_element() { let p = pos; pos += 1; p } else { 0 })
        .collect();
    ChildPositions { elem_index, elem_count: pos, type_index, type_count }
}

/// Build ancestor info for the current node.
fn build_ancestor_entry(
    node: &WebCore,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
) -> crate::css::AncestorInfo {
    crate::css::AncestorInfo {
        tag: node.tag.clone(),
        attributes: node.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id: node.node_id,
    }
}

/// Walk `node`'s subtree testing every ELEMENT against `selectors`.
///
/// `parent_ancestors` must already contain `node` itself — `run_query` seeds it
/// with the document element so `html body` has an `html` to match against.
///
/// Document order, depth-first — `first_only` stops at the first hit, which is
/// what `querySelector` returns.
fn query_walk(
    node: &WebCore,
    parent_ancestors: &[crate::css::AncestorInfo],
    selectors: &[crate::css::CssSelector],
    hover_chain: &std::collections::HashSet<u32>,
    first_only: bool,
    results: &mut Vec<u32>,
) -> bool {
    let pos = child_positions(&node.children);
    // `+` and `~` look BACKWARDS, so this accumulates as the walk moves right.
    let mut prev_siblings: Vec<(String, String, String)> = Vec::new();

    for (i, child) in node.children.iter().enumerate() {
        if !child.is_element() || child.node_id == 0 { continue; }

        let ctx = crate::css::MatchContext {
            focused_box: 0,
            keyboard_focus: false,
            type_child_index: pos.type_index[i],
            type_sibling_count: pos.type_count[i],
            html_box: Some(child),
            hover_chain,
            element_id: child.node_id,
            prev_siblings: &prev_siblings,
        };

        for sel in selectors {
            if crate::css::matches_selector_with_ancestors(
                &sel.parts, &child.tag, &child.attributes,
                pos.elem_index[i], pos.elem_count, parent_ancestors, &ctx,
            ) {
                results.push(child.node_id);
                if first_only { return true; }
                break; // one hit per element, however many alternatives matched
            }
        }

        let mut child_ancestors = parent_ancestors.to_vec();
        child_ancestors.push(build_ancestor_entry(
            child, pos.elem_index[i], pos.elem_count,
            pos.type_index[i], pos.type_count[i],
        ));
        if query_walk(child, &child_ancestors, selectors, hover_chain, first_only, results) {
            return true;
        }

        prev_siblings.push((
            child.tag.to_ascii_lowercase(),
            child.attributes.get("id").cloned().unwrap_or_default(),
            child.attributes.get("class").cloned().unwrap_or_default(),
        ));
    }
    false
}

// ─── Read ───────────────────────────────────────────────────────────────────

impl Document {
    /// `element.tagName`.
    pub fn tag_name(&self, id: u32) -> Option<&str> {
        if id == 0 { return None; }
        if let Some(node) = self.arena.try_get(NodeId(id)) {
            if !node.tag.is_empty() { return Some(node.tag.as_str()); }
        }
        // Shadow nodes have no arena entry — the render tree answers for them.
        self.find_webcore(id).map(|n| n.tag.as_str())
    }

    /// Get an attribute value.
    pub fn get_attribute(&self, id: u32, key: &str) -> Option<String> {
        if id == 0 { return None; }
        let folded = self.fold_name(key);
        if let Some(v) = self.arena.try_get(NodeId(id)).and_then(|n| n.attributes.get(&folded)) {
            return Some(v.clone());
        }
        // Shadow-tree nodes are not mirrored into the arena, so the arena has
        // nothing for them and every attribute read came back empty — a
        // `<slot name=title>` reported no name at all. The render tree is the
        // authority for those, and `find_webcore` reaches them.
        self.find_webcore(id)
            .and_then(|n| n.attributes.get(&folded).cloned())
    }

    /// Get the text content of a node and all its descendants.
    pub fn text_content(&self, id: u32) -> String {
        if id == 0 { return String::new(); }
        let mut out = String::new();
        self.collect_text(NodeId(id), &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        let node = self.arena.get(id);
        // DOM §4.4: `textContent` concatenates the data of every descendant
        // EXCEPT comments and processing instructions. Those carry data too,
        // and including them would leak markup that is not content.
        //
        // Stated as an EXCLUSION rather than "text nodes only" on purpose:
        // `set_text_content` stores the string on the ELEMENT node instead of
        // building a child text node, so an allow-list of `NodeType::Text`
        // silently made `textContent` empty after every such write. Reading the
        // parser alone does not reveal that — it only ever puts text on
        // `#text` nodes.
        let is_markup_only = matches!(
            node.node_type,
            crate::dom::arena::NodeType::Comment
                | crate::dom::arena::NodeType::ProcessingInstruction
        );
        if !is_markup_only && !node.text.is_empty() {
            out.push_str(&node.text);
        }
        let mut child = node.first_child;
        while child.is_some() {
            self.collect_text(child, out);
            child = self.arena.get(child).next_sibling;
        }
    }

    /// `node.parentNode` — 0 when there is none.
    pub fn parent_node(&self, id: u32) -> u32 {
        if id == 0 || crate::dom::events::is_document_target(id) { return 0; }
        // `<html>`'s parent is the DOCUMENT, not nothing. It read as an orphan,
        // which is why `getRootNode()` stopped at the document element.
        if id == self.root.node_id { return crate::dom::events::DOCUMENT_TARGET; }
        // The doctype hangs off the document, not off any element.
        if id != 0 && id == self.doctype { return crate::dom::events::DOCUMENT_TARGET; }
        let p = self.arena.try_get(NodeId(id)).map_or(0, |n| n.parent.0);
        if p == 0 && self.find_webcore(id).is_some() && id != self.root.node_id {
            // A shadow-tree node has no arena parent; its parent is its host.
            if let Some(h) = self.shadow_host_of_child(id) { return h; }
        }
        p
    }

    /// The node that directly contains `id` in the render tree.
    ///
    /// ⛔ For a TOP-LEVEL shadow child the answer is the shadow ROOT, not the
    /// host. Chrome: `p.parentNode === root` is true and `p.parentElement` is
    /// **null**, because a `ShadowRoot` is a `DocumentFragment` and not an
    /// element. Answering the host here flattened the shadow boundary — the
    /// one boundary the whole feature exists to draw.
    fn shadow_host_of_child(&self, id: u32) -> Option<u32> {
        fn walk(n: &WebCore, id: u32) -> Option<u32> {
            if let Some(sr) = &n.shadow_root {
                if sr.children.iter().any(|c| c.node_id == id) { return Some(sr.node_id); }
                for c in &sr.children { if let Some(f) = walk(c, id) { return Some(f); } }
            }
            if n.children.iter().any(|c| c.node_id == id) { return Some(n.node_id); }
            n.children.iter().find_map(|c| walk(c, id))
        }
        walk(&self.root, id)
    }

    /// `document` — the id of the document node itself.
    pub fn document_node(&self) -> u32 { crate::dom::events::DOCUMENT_TARGET }


    /// `node.childNodes` — every child, of every kind, in tree order.
    pub fn child_nodes(&self, id: u32) -> Vec<u32> {
        if id == 0 { return Vec::new(); }
        // A shadow root's children are its tree's top level; a node INSIDE a
        // shadow tree has children of its own that the arena never saw. Both
        // are answered from the render tree, which is where a shadow tree
        // lives.
        if crate::dom::arena::is_shadow_node_id(id) {
            if let Some(root) = self.shadow_root_by_id(id) {
                return root.children.iter().map(|c| c.node_id).collect();
            }
            return self.find_webcore(id)
                .map(|n| n.children.iter().map(|c| c.node_id).collect())
                .unwrap_or_default();
        }
        // ⛔ The DOCUMENT is not an arena node — `arena.children` asserts on
        // its id and PANICKED here, the same unguarded-`get` shape as the
        // shadow ids. Its children are the doctype and the document element,
        // in that order (measured: `document.firstChild === document.doctype`).
        if crate::dom::events::is_document_target(id) {
            let mut kids = Vec::new();
            if self.doctype != 0 { kids.push(self.doctype); }
            if self.root.node_id != 0 { kids.push(self.root.node_id); }
            return kids;
        }
        if !self.arena.is_alive(NodeId(id)) { return Vec::new(); }
        self.arena.children(NodeId(id)).map(|c| c.0).collect()
    }

    /// Get the next sibling node_id (0 if none).
    pub fn next_sibling(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.try_get(NodeId(id)).map_or(0, |n| n.next_sibling.0)
    }

    /// `node.previousSibling` — 0 when there is none.
    pub fn previous_sibling(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.try_get(NodeId(id)).map_or(0, |n| n.prev_sibling.0)
    }
}

// ─── Interactive state reconciliation ───────────────────────────────────────

impl Document {
    /// Copy form state from the render tree back into the arena.
    ///
    /// WHY THIS EXISTS
    ///
    /// Everything in this file dual-writes: the arena first, then the WebCore
    /// tree. Interaction cannot follow that rule. `handle_form_click` and
    /// `process_form_input_key` are free functions over `&mut WebCore` — they
    /// have no `Document`, so they have no arena to write to, and they set
    /// `checked` / `value` on the render tree alone.
    ///
    /// That is invisible until something READS through the arena, which is
    /// exactly what the WHATWG accessors do. Without this, a user ticks a
    /// checkbox and `getAttribute("checked")` still answers the pre-click
    /// value — the interaction happened, and the DOM denies it.
    ///
    /// Reconciling here rather than fixing it at the write sites keeps the
    /// interaction code as pure render-tree logic and puts the sync at the one
    /// boundary that owns both stores.
    ///
    /// Only the three attributes interaction actually writes are copied, and
    /// only on form controls — a click must not cost a clone of every
    /// attribute map in the document.
    /// Not public: this is bookkeeping between two internal stores, not a web
    /// API. Nothing outside the browser should know the render tree and the
    /// arena are separate things.
    pub(crate) fn sync_form_state_to_arena(&mut self) {
        type Snapshot = (u32, Option<String>, Option<String>, Option<String>);
        fn walk(node: &WebCore, out: &mut Vec<Snapshot>) {
            if node.node_id != 0
                && matches!(node.tag.as_str(), "input" | "textarea" | "select" | "option")
            {
                out.push((
                    node.node_id,
                    // `checked` is NOT here any more: interaction writes
                    // CHECKEDNESS on the render-tree box, and the attribute it
                    // used to overwrite is the author's default, which no
                    // click may touch. Syncing it would put the old conflation
                    // back one level down.
                    None,
                    node.attributes.get("value").cloned(),
                    node.attributes.get("selected").cloned(),
                ));
            }
            for child in &node.children {
                walk(child, out);
            }
        }

        let mut updates: Vec<Snapshot> = Vec::new();
        walk(&self.root, &mut updates);

        for (id, checked, value, selected) in updates {
            if !self.arena.is_alive(NodeId(id)) {
                continue;
            }
            let attrs = &mut self.arena.get_mut(NodeId(id)).attributes;
            // Absent is meaningful: `checked` is a boolean attribute, so
            // REMOVING it is how "unticked" is spelled. A set-only sync would
            // make unticking a no-op.
            for (key, val) in [("checked", checked), ("value", value), ("selected", selected)] {
                match val {
                    Some(v) => {
                        attrs.insert(key.to_string(), v);
                    }
                    None => {
                        attrs.remove(key);
                    }
                }
            }
        }
    }
}

// ─── WHATWG tree operations ─────────────────────────────────────────────────

impl Document {
    /// `element.innerHTML` (getter) — the serialization of a node's CHILDREN,
    /// not of the node itself. `serialize_box` already knew how to write a
    /// subtree; the only thing missing was the DOM spelling in front of it.
    pub fn inner_html(&self, id: u32) -> String {
        if id == 0 { return String::new(); }
        let node = match self.find_webcore(id) {
            Some(n) => n,
            // A node created but never inserted still has an `innerHTML`.
            None => match self.pending_nodes.get(&id) {
                Some(n) => n,
                None => return String::new(),
            },
        };
        let mut out = String::new();
        for child in &node.children {
            crate::html::serializer::serialize_box(child, &mut out);
        }
        out
    }

    /// `node.cloneNode(deep)`. The clone is DETACHED — WHATWG gives it no
    /// parent — so it lands in `pending_nodes` exactly like a freshly created
    /// node, and the same `dom_append_child` attaches it.
    pub fn clone_node(&mut self, id: u32, deep: bool) -> u32 {
        if id == 0 { return 0; }
        // Cloned out of the tree FIRST so the recursion below owns its source.
        // `clone_subtree` needs `&mut self` for the arena, which it could not
        // have while still borrowing a box out of `self.root`.
        let src = match self.find_webcore(id).cloned() {
            Some(b) => b,
            None => match self.pending_nodes.get(&id).cloned() {
                Some(b) => b,
                None => return 0,
            },
        };
        let clone = self.clone_subtree(&src, deep);
        let new_id = clone.node_id;
        self.pending_nodes.insert(new_id, clone);
        new_id
    }

    /// Recursively rebuild `src` with fresh identities in BOTH stores.
    ///
    /// `src` must be owned by the caller, not borrowed out of `self` — the
    /// arena writes here need `&mut self`.
    fn clone_subtree(&mut self, src: &WebCore, deep: bool) -> WebCore {
        let new_id = match src.tag.as_str() {
            "#text" => self.arena.create_text(&src.text),
            "#comment" => self.arena.create_comment(&src.text),
            _ => {
                let a = self.arena.create_element(&src.tag);
                for (k, v) in &src.attributes {
                    self.arena.get_mut(a).attributes.insert(k.clone(), v.clone());
                }
                a
            }
        };

        let mut b = src.clone();
        b.node_id = new_id.0;
        b.children.clear();

        // A shallow clone copies the node and no descendants — DOM §4.4.
        if deep {
            for child in &src.children {
                let child_box = self.clone_subtree(child, true);
                self.arena.append_child(new_id, NodeId(child_box.node_id));
                b.children.push(child_box);
            }
        }

        self.next_node_id = self.next_node_id.max(new_id.0 + 1);
        b
    }

    /// `parent.replaceChild(new, old)`.
    ///
    /// Composed from the two operations that already dual-write correctly
    /// rather than open-coded: insert the replacement ahead of the outgoing
    /// node, then remove it. A hand-rolled version would be a third place that
    /// has to remember the arena, the render tree AND the dirty flags.
    pub fn replace_child(&mut self, parent: u32, new_child: u32, old_child: u32) -> bool {
        if parent == 0 || new_child == 0 || old_child == 0 { return false; }
        if !self.arena.is_alive(NodeId(old_child)) { return false; }
        // WHATWG throws NotFoundError when `old` is not a child of `parent`.
        if self.arena.get(NodeId(old_child)).parent.0 != parent { return false; }
        self.insert_before(parent, new_child, old_child);
        self.remove_child(old_child);
        true
    }
}

// ─── Node kind predicates, character data, and the rest of the IDL ──────────

impl Document {
    /// `document.body`.
    pub fn body(&self) -> Option<u32> {
        self.query_selector("body")
    }

    /// Which grammar this document was built from.
    pub fn kind(&self) -> crate::types::DocumentKind {
        self.kind
    }

    /// `node.nodeType === Node.ELEMENT_NODE`.
    pub fn is_element(&self, id: u32) -> bool {
        self.node_type(id) == 1
    }

    pub fn is_text_node(&self, id: u32) -> bool {
        self.node_type(id) == 3
    }

    pub fn is_comment_node(&self, id: u32) -> bool {
        self.node_type(id) == 8
    }

    /// DOM §4.10 `CharacterData` — text, CDATA, comment and processing
    /// instruction. The nodes that HAVE `data`, which is exactly the set whose
    /// `nodeValue` is non-null.
    pub fn is_character_data(&self, id: u32) -> bool {
        matches!(self.node_type(id), 3 | 4 | 7 | 8)
    }

    /// `CharacterData.data` — this node's OWN text, not its descendants'.
    ///
    /// Different question from `text_content`, which concatenates a subtree.
    /// On an element this is empty, because an element has no data of its own.
    pub fn text_data(&self, id: u32) -> String {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return String::new();
        }
        self.arena.get(NodeId(id)).text.clone()
    }

    /// `CharacterData.data = …`.
    pub fn set_text_data(&mut self, id: u32, data: &str) {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return;
        }
        self.arena.get_mut(NodeId(id)).text = data.to_string();
        if let Some(node) = self.find_webcore_mut(id) {
            node.text = data.to_string();
            node.layout.layout_dirty = true;
            node.layout.intrinsic_dirty = true;
        }
    }

    /// `CharacterData.length` — in UTF-16 code units, which is what every
    /// offset in this interface is counted in.
    ///
    /// Not `str::len` and not `chars().count()`: DOM offsets are UTF-16, so an
    /// emoji is TWO and `é` is one. A Rust byte offset would disagree with a
    /// script on any non-ASCII text.
    pub fn character_data_length(&self, id: u32) -> usize {
        self.text_data(id).encode_utf16().count()
    }

    /// `CharacterData.substringData(offset, count)`.
    ///
    /// `None` when `offset` is past the end — the spec throws `IndexSizeError`
    /// there, and this is the shape a caller can turn into one. A `count` that
    /// runs past the end is clamped, exactly as the spec says.
    pub fn substring_data(&self, id: u32, offset: usize, count: usize) -> Option<String> {
        let units: Vec<u16> = self.text_data(id).encode_utf16().collect();
        if offset > units.len() { return None; }
        let end = offset.saturating_add(count).min(units.len());
        Some(String::from_utf16_lossy(&units[offset..end]))
    }

    /// `CharacterData.replaceData(offset, count, data)` — the primitive the
    /// other four mutators are defined in terms of (DOM §4.10).
    pub fn replace_data(&mut self, id: u32, offset: usize, count: usize, data: &str) -> bool {
        let units: Vec<u16> = self.text_data(id).encode_utf16().collect();
        if offset > units.len() { return false; }
        let end = offset.saturating_add(count).min(units.len());
        let mut out: Vec<u16> = units[..offset].to_vec();
        out.extend(data.encode_utf16());
        out.extend_from_slice(&units[end..]);
        self.set_text_data(id, &String::from_utf16_lossy(&out));
        true
    }

    /// `CharacterData.appendData(data)`.
    pub fn append_data(&mut self, id: u32, data: &str) -> bool {
        let len = self.character_data_length(id);
        self.replace_data(id, len, 0, data)
    }

    /// `CharacterData.insertData(offset, data)`.
    pub fn insert_data(&mut self, id: u32, offset: usize, data: &str) -> bool {
        self.replace_data(id, offset, 0, data)
    }

    /// `CharacterData.deleteData(offset, count)`.
    pub fn delete_data(&mut self, id: u32, offset: usize, count: usize) -> bool {
        self.replace_data(id, offset, count, "")
    }

    /// `Text.splitText(offset)` — cut this node in two and return the NEW node,
    /// which holds everything from `offset` on and becomes the next sibling.
    ///
    /// Returns `None` for a bad offset or a non-text node; the spec throws
    /// `IndexSizeError` for the former.
    pub fn split_text(&mut self, id: u32, offset: usize) -> Option<u32> {
        if self.node_type(id) != 3 { return None; }
        let tail = self.substring_data(id, offset, usize::MAX)?;
        let new_id = self.create_text_node(&tail);
        self.delete_data(id, offset, usize::MAX);
        let parent = self.parent_node(id);
        if parent != 0 {
            // Insert directly after this node. A detached text node has no
            // parent and the new node simply stays detached, per the spec.
            let next = self.next_sibling(id);
            if next != 0 {
                self.insert_before(parent, new_id, next);
            } else {
                self.append_child(parent, new_id);
            }
        }
        Some(new_id)
    }

    /// `Text.wholeText` — the concatenated data of this node and the run of
    /// text-node siblings it sits in, with no element between them.
    pub fn whole_text(&self, id: u32) -> String {
        if self.node_type(id) != 3 { return String::new(); }
        let parent = self.parent_node(id);
        if parent == 0 { return self.text_data(id); }
        let sibs = self.child_nodes(parent);
        let Some(at) = sibs.iter().position(|n| *n == id) else { return self.text_data(id) };
        let mut start = at;
        while start > 0 && self.node_type(sibs[start - 1]) == 3 { start -= 1; }
        let mut end = at + 1;
        while end < sibs.len() && self.node_type(sibs[end]) == 3 { end += 1; }
        sibs[start..end].iter().map(|n| self.text_data(*n)).collect()
    }

    /// `node.getRootNode()` — the topmost ancestor.
    ///
    /// ⚠ DEVIATION: the spec's answer for a connected node is the DOCUMENT, and
    /// this returns the document ELEMENT (`<html>`), because webcore's tree has
    /// no document node — the only `NodeType::Document` in the arena is the
    /// dead sentinel in slot 0. Everything else about this is spec-correct;
    /// the missing node is tracked as its own item, and it is the same gap that
    /// keeps `ownerDocument` from ever answering null.
    ///
    /// `composed` follows a shadow root out to its host; without it the walk
    /// stops at the shadow boundary, which is the point of a shadow root.
    pub fn get_root_node(&self, id: u32, composed: bool) -> u32 {
        if crate::dom::events::is_document_target(id) { return id; }
        if id == 0 { return 0; }
        let mut cur = id;
        loop {
            // A shadow root is the root of its tree unless `composed` asks to
            // cross the boundary — that is the whole point of the option.
            if !composed {
                if let Some(host) = self.shadow_host(cur) {
                    let _ = host;
                    // The shadow tree's root is its topmost node; without a
                    // ShadowRoot node to name, the host stands in for it.
                    if let Some(h) = self.shadow_host(cur) { return h; }
                }
            }
            let parent = self.parent_node(cur);
            if parent == 0 { return cur; }
            cur = parent;
        }
    }

    /// `node.ownerDocument` — null for the document itself (DOM §4.4), which
    /// is why this is an `Option` rather than the document's own id.
    ///
    /// ⚠ DEVIATION, same cause as `get_root_node`: with no document node in the
    /// tree this hands back the document ELEMENT, and the `None` arm is
    /// unreachable — `node_type` never answers 9 for a live node. Chrome
    /// answers `null` for `document.ownerDocument`; we answer the `<html>`
    /// element for it.
    pub fn owner_document(&self, id: u32) -> Option<u32> {
        if id == 0 { return None; }
        // Null for the document itself (DOM §4.4) — now reachable, because
        // there is a document node to be.
        if crate::dom::events::is_document_target(id) { return None; }
        Some(crate::dom::events::DOCUMENT_TARGET)
    }

    /// `node.isSameNode(other)` — identity, not equality.
    pub fn is_same_node(&self, id: u32, other: u32) -> bool {
        id != 0 && id == other
    }

    /// `node.baseURI` — the document base URL.
    pub fn base_uri(&self, _id: u32) -> String {
        self.base_url.clone()
    }

    /// `element.hasAttribute(name)`.
    ///
    /// Not `getAttribute(..).is_some()` at the call site: an attribute present
    /// with an empty value is PRESENT, and `checked=""` is exactly that case.
    pub fn has_attribute(&self, id: u32, name: &str) -> bool {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return false;
        }
        self.arena
            .get(NodeId(id))
            .attributes
            .contains_key(&self.fold_name(name))
    }

    /// `input.checked` — a BOOLEAN ATTRIBUTE: present is true, absent is false.
    /// Its value is irrelevant, which is why this asks `has_attribute`.
    /// `input.checked` — CHECKEDNESS, the live state (HTML §4.10.5.3).
    ///
    /// Not `hasAttribute("checked")`: that is `defaultChecked`, the value a
    /// form reset restores to. They start equal and part company the moment
    /// anything ticks the box.
    pub fn checked(&self, id: u32) -> bool {
        self.find_webcore(id).map(|n| n.checkedness).unwrap_or(false)
    }

    /// `input.checked = b` — the IDL setter.
    ///
    /// Sets checkedness and raises the dirty checkedness flag, which is what
    /// stops a later `setAttribute("checked", …)` moving the box back. It does
    /// NOT touch the attribute: script ticking a box no more rewrites the
    /// document than a user clicking one does.
    ///
    /// This used to add and remove the `checked` ATTRIBUTE, which made the
    /// markup and the state one store — so `getAttribute("checked")` reported
    /// clicks, and the reset algorithm had nothing left to restore to.
    pub fn set_checked(&mut self, id: u32, checked: bool) {
        if let Some(node) = self.find_webcore_mut(id) {
            node.checkedness = checked;
            node.dirty_checked = true;
            // **Invalidate what this changed.** The old body wrote the
            // attribute through `set_attribute`, which marked the node dirty as
            // a side effect; writing the state field directly loses that, and a
            // cached cascade then keeps answering `:checked` from before the
            // change.
            node.layout.layout_dirty = true;
            node.layout.intrinsic_dirty = true;
        }
        // `:checked` is a SELECTOR, so checkedness is an input to the cascade,
        // not just to the painter — and the cascade is cached per element.
        self.style_dirty = true;
    }

    /// `element.focus()`.
    ///
    /// Focus is a field on the document here — `process_key_event` reads
    /// `focused_box` to decide which control receives typing, so setting it IS
    /// focusing.
    pub fn focus(&mut self, id: u32) {
        self.focused_box = id;
    }

    /// `getComputedStyle(element)` — the resolved property set after the
    /// cascade.
    ///
    /// Each engine answers with ITS OWN computed-style type (`ComputedStyle`
    /// here, `css::CssProperties` in the other), because the resolved form is
    /// an internal representation and no two engines share one. What crosses
    /// the seam is the string-keyed `computed_style_property` below, which both
    /// answer identically — the same split a browser draws between its internal
    /// style struct and the `CSSStyleDeclaration` it hands to script.
    pub fn get_computed_style(&self, id: u32) -> Option<&crate::types::ComputedStyle> {
        self.find_webcore(id).map(|node| &node.style)
    }

    /// `font-size` on the root element — what `rem` resolves against.
    fn root_font_px(&self) -> f32 {
        self.root.style.font_size.resolve(16.0, 16.0, 16.0)
    }

    /// The origin a POSITIONED box's insets are measured from: the nearest
    /// positioned ancestor, or the initial containing block at the page origin.
    ///
    /// CSS 2.1 §10.1. Two boxes with the same `top: 10px` sit at different page
    /// coordinates when their containing blocks differ, and `getComputedStyle`
    /// has to answer `10px` for both.
    fn containing_origin(&self, id: u32) -> (f32, f32) {
        let mut current = self.parent_node(id);
        while current != 0 {
            let positioned = self
                .get_computed_style(current)
                .map(|s| !matches!(s.position, crate::types::Position::Static))
                .unwrap_or(false);
            if positioned {
                if let Some(rect) = self.get_bounding_client_rect(current) {
                    return (rect.x, rect.y);
                }
            }
            current = self.parent_node(current);
        }
        (0.0, 0.0)
    }

    /// `getComputedStyle(element).getPropertyValue(property)` — the RESOLVED
    /// value, in used units.
    ///
    /// Reading this FORCES LAYOUT, as it does in a browser: a document built
    /// but never laid out has no geometry, and `0px` is the absence of an
    /// answer rather than an answer. Everything that is not geometry falls back
    /// to the declared value — an honest floor, since fully resolving every
    /// property against the cascade is a larger job than this.
    pub fn computed_style_property(&mut self, id: u32, property: &str) -> String {
        let width = self.viewport_w;
        crate::layout::LayoutEngine::new().layout(self, width);
        let rect = self.get_bounding_client_rect(id);
        let property = property.to_ascii_lowercase();
        // **An inset is the used value only when the element is POSITIONED**
        // (CSSOM §6.6.1). For a static box `top` has no effect at all, so its
        // resolved value is the COMPUTED one — `1em` on a 16px font is `16px`,
        // wherever the box happens to sit on the page.
        //
        // Answering the bounding rect for every element made `top` mean "how
        // far down the page you are", so a static box inside a body with the
        // UA's 8px margin reported `8px` for a declared `1em` — and moving the
        // box changed a value the box never set.
        let inset = matches!(property.as_str(), "left" | "top" | "right" | "bottom");
        let positioned = self
            .get_computed_style(id)
            .map(|s| !matches!(s.position, crate::types::Position::Static))
            .unwrap_or(false);
        if inset && !positioned {
            let style = match self.get_computed_style(id) {
                Some(style) => style,
                None => return String::new(),
            };
            let declared = match property.as_str() {
                "left" => style.left.clone(),
                "top" => style.top.clone(),
                "right" => style.right.clone(),
                _ => style.bottom.clone(),
            };
            let font_px = style.font_size.resolve(16.0, 16.0, 16.0);
            // `auto` is the initial value and stays the word — it is not a
            // length and serialising it as `0px` would claim the box was
            // placed. Percentages stay percentages, as the spec's computed
            // value for an inset does.
            return match declared {
                crate::types::CssLength::Auto => "auto".to_string(),
                crate::types::CssLength::Percent(p) => format!("{p}%"),
                other => format!(
                    "{}px",
                    other.resolve(font_px, self.viewport_w, self.root_font_px())
                ),
            };
        }
        let resolved = match (property.as_str(), rect) {
            // A positioned box's inset IS its used value — the offset from the
            // containing block, not from the page. Same number whenever that
            // block is the initial one, which is why this went unnoticed.
            ("left", Some(r)) => Some(format!("{}px", r.x - self.containing_origin(id).0)),
            ("top", Some(r)) => Some(format!("{}px", r.y - self.containing_origin(id).1)),
            ("width", Some(r)) => Some(format!("{}px", r.w)),
            ("height", Some(r)) => Some(format!("{}px", r.h)),
            _ => None,
        };
        match resolved {
            Some(v) => v,
            None => self.get_style_property(id, &property).unwrap_or_default(),
        }
    }
}

// ─── Node and Element traversal ─────────────────────────────────────────────
//
// All derived from `child_nodes` / `parent_node` / `node_type`, which is the
// point: the DOM offers the same tree through several vocabularies, and a
// browser is expected to answer every one of them. `childNodes` counts text
// and comments; `children` counts only elements — a page that walks the wrong
// one silently sees whitespace as a node.

impl Document {
    /// `node.parentElement` — the parent, but only if it IS an element.
    /// `None` at the document, whose parent is not an element.
    pub fn parent_element(&self, id: u32) -> Option<u32> {
        let parent = self.parent_node(id);
        (parent != 0 && self.is_element(parent)).then_some(parent)
    }

    /// `node.firstChild` / `lastChild` — of ANY kind.
    pub fn first_child(&self, id: u32) -> Option<u32> {
        self.child_nodes(id).first().copied()
    }

    pub fn last_child(&self, id: u32) -> Option<u32> {
        self.child_nodes(id).last().copied()
    }

    pub fn has_child_nodes(&self, id: u32) -> bool {
        !self.child_nodes(id).is_empty()
    }

    /// `node.contains(other)` — DOM §4.4. A node CONTAINS ITSELF, which is the
    /// part that surprises: `a.contains(a)` is true.
    pub fn contains(&self, id: u32, other: u32) -> bool {
        if id == 0 || other == 0 {
            return false;
        }
        let mut current = other;
        loop {
            if current == id {
                return true;
            }
            let parent = self.parent_node(current);
            if parent == 0 {
                return false;
            }
            current = parent;
        }
    }

    /// `element.children` — ELEMENT children only, unlike `childNodes`.
    pub fn children(&self, id: u32) -> Vec<u32> {
        self.child_nodes(id)
            .into_iter()
            .filter(|c| self.is_element(*c))
            .collect()
    }

    pub fn first_element_child(&self, id: u32) -> Option<u32> {
        self.children(id).first().copied()
    }

    pub fn last_element_child(&self, id: u32) -> Option<u32> {
        self.children(id).last().copied()
    }

    pub fn child_element_count(&self, id: u32) -> usize {
        self.children(id).len()
    }

    /// `element.nextElementSibling` — skips text and comments, which is the
    /// difference from `nextSibling` and the reason both exist.
    pub fn next_element_sibling(&self, id: u32) -> Option<u32> {
        let parent = self.parent_node(id);
        let siblings = self.children(parent);
        let at = siblings.iter().position(|n| *n == id)?;
        siblings.get(at + 1).copied()
    }

    pub fn previous_element_sibling(&self, id: u32) -> Option<u32> {
        let parent = self.parent_node(id);
        let siblings = self.children(parent);
        let at = siblings.iter().position(|n| *n == id)?;
        if at == 0 { None } else { siblings.get(at - 1).copied() }
    }

    // ─── EventTarget / event handler IDL (DOM §2.7, HTML §8.1.7.2) ──────────

    /// `target.addEventListener(type, callback, options)`.
    pub fn add_event_listener(
        &mut self,
        id: u32,
        event_type: &str,
        handler: crate::dom::events::EventHandler,
        options: crate::dom::events::ListenerOptions,
    ) -> u32 {
        self.event_targets.add_event_listener_with(id, event_type, handler, options)
    }

    /// `target.removeEventListener(...)`, by the id `add_event_listener` gave.
    pub fn remove_event_listener(&mut self, listener_id: u32) {
        self.event_targets.remove_event_listener(listener_id);
    }

    /// `target.dispatchEvent(event)` — returns false if a handler cancelled it,
    /// which is what the IDL says the boolean means. The event is NOT trusted:
    /// only the user agent creates trusted events (DOM §2.2).
    pub fn dispatch_event(&mut self, event: &mut crate::dom::events::DomEvent) -> bool {
        event.is_trusted = false;
        // `dispatch_on_tree` only READS the tree, but `event_targets` is a
        // field of the same struct — so the path is collected first and the
        // dispatch runs against it, keeping the borrows apart.
        // Routed through `dispatch_dom_event` so this path and the engine's own
        // get the SAME algorithm — shadow-aware propagation, retargeting, the
        // dispatch flag and the `once` sweep. A second path walker here is how
        // shadow dispatch came to work in one of them and not the other.
        self.dispatch_dom_event(event);
        !event.default_prevented()
    }


    /// `el.onclick = handler` and friends. `None` if `handler_name` is not an
    /// event handler attribute a browser has.
    pub fn set_event_handler(
        &mut self,
        id: u32,
        handler_name: &str,
        handler: crate::dom::events::EventHandler,
    ) -> Option<u32> {
        self.event_targets.set_event_handler(id, handler_name, handler)
    }

    /// `el.onclick = null`.
    pub fn remove_event_handler(&mut self, id: u32, handler_name: &str) -> bool {
        self.event_targets.remove_event_handler(id, handler_name)
    }

    /// `el.onclick !== null`.
    pub fn has_event_handler(&self, id: u32, handler_name: &str) -> bool {
        self.event_targets.has_event_handler(id, handler_name)
    }

    /// The handler attributes currently set on an element.
    pub fn event_handler_names(&self, id: u32) -> Vec<String> {
        self.event_targets.event_handler_names(id)
    }

    // ─── Shadow DOM (DOM §4.8, HTML §4.13) ──────────────────────────────────

    /// `element.attachShadow({ mode })` — attach a shadow root and return the
    /// HOST's id.
    ///
    /// A `ShadowRoot` is a node in the spec, and this tree has no node for it
    /// (the same shape as the missing Document node), so the host is what comes
    /// back and the shadow tree is reached through `shadow_children`. Returns
    /// `None` if the element already has one — `attachShadow` throws
    /// `NotSupportedError` there rather than replacing it.
    pub fn attach_shadow(&mut self, id: u32, mode: crate::types::ShadowMode) -> Option<u32> {
        if id == 0 { return None; }
        if self.has_shadow_root(id) { return None; }
        let node = self.find_webcore_mut(id)?;
        node.attach_shadow(mode, "");
        // Returns the SHADOW ROOT, as the IDL says — not the host.
        node.shadow_root.as_ref().map(|sr| sr.node_id)
    }

    /// Resolve a shadow root id to the element that hosts it.
    fn host_of_shadow_root(&self, shadow_id: u32) -> Option<u32> {
        fn walk(n: &WebCore, sid: u32) -> Option<u32> {
            if let Some(sr) = &n.shadow_root {
                if sr.node_id == sid { return Some(n.node_id); }
                for c in &sr.children { if let Some(f) = walk(c, sid) { return Some(f); } }
            }
            n.children.iter().find_map(|c| walk(c, sid))
        }
        walk(&self.root, shadow_id)
    }

    /// Borrow a shadow root by its own id.
    fn shadow_root_by_id(&self, shadow_id: u32) -> Option<&crate::types::ShadowRoot> {
        let host = self.host_of_shadow_root(shadow_id)?;
        self.find_webcore(host)?.shadow_root.as_deref()
    }

    fn shadow_root_by_id_mut(&mut self, shadow_id: u32) -> Option<&mut crate::types::ShadowRoot> {
        let host = self.host_of_shadow_root(shadow_id)?;
        self.find_webcore_mut(host)?.shadow_root.as_deref_mut()
    }

    /// `shadowRoot.host`.
    pub fn shadow_root_host(&self, shadow_id: u32) -> Option<u32> {
        self.host_of_shadow_root(shadow_id)
    }

    /// `shadowRoot.mode`.
    pub fn shadow_root_mode(&self, shadow_id: u32) -> Option<crate::types::ShadowMode> {
        self.shadow_root_by_id(shadow_id).map(|sr| sr.mode)
    }

    /// `shadowRoot.delegatesFocus`.
    pub fn shadow_delegates_focus(&self, shadow_id: u32) -> bool {
        self.shadow_root_by_id(shadow_id).map(|sr| sr.delegates_focus).unwrap_or(false)
    }
    pub fn set_shadow_delegates_focus(&mut self, shadow_id: u32, v: bool) {
        if let Some(sr) = self.shadow_root_by_id_mut(shadow_id) { sr.delegates_focus = v; }
    }

    /// `shadowRoot.slotAssignment`.
    pub fn shadow_slot_assignment(&self, shadow_id: u32) -> crate::types::SlotAssignment {
        self.shadow_root_by_id(shadow_id).map(|sr| sr.slot_assignment).unwrap_or_default()
    }
    pub fn set_shadow_slot_assignment(&mut self, shadow_id: u32, v: crate::types::SlotAssignment) {
        if let Some(sr) = self.shadow_root_by_id_mut(shadow_id) { sr.slot_assignment = v; }
    }

    /// `shadowRoot.clonable`.
    pub fn shadow_clonable(&self, shadow_id: u32) -> bool {
        self.shadow_root_by_id(shadow_id).map(|sr| sr.clonable).unwrap_or(false)
    }
    pub fn set_shadow_clonable(&mut self, shadow_id: u32, v: bool) {
        if let Some(sr) = self.shadow_root_by_id_mut(shadow_id) { sr.clonable = v; }
    }

    /// `shadowRoot.serializable`.
    pub fn shadow_serializable(&self, shadow_id: u32) -> bool {
        self.shadow_root_by_id(shadow_id).map(|sr| sr.serializable).unwrap_or(false)
    }
    pub fn set_shadow_serializable(&mut self, shadow_id: u32, v: bool) {
        if let Some(sr) = self.shadow_root_by_id_mut(shadow_id) { sr.serializable = v; }
    }

    /// `shadowRoot.adoptedStyleSheets` — constructed sheets applied to the
    /// tree, held as their source text.
    pub fn shadow_adopted_stylesheets(&self, shadow_id: u32) -> Vec<String> {
        self.shadow_root_by_id(shadow_id).map(|sr| sr.adopted_stylesheets.clone()).unwrap_or_default()
    }

    /// Replace `adoptedStyleSheets` and fold the rules into the scoped sheet,
    /// so adopting one actually STYLES the tree rather than only recording it.
    pub fn set_shadow_adopted_stylesheets(&mut self, shadow_id: u32, sheets: Vec<String>) {
        let Some(sr) = self.shadow_root_by_id_mut(shadow_id) else { return };
        sr.adopted_stylesheets = sheets.clone();
        for css in &sheets {
            sr.stylesheet.parse_and_add_author(css);
        }
        sr.stylesheet.rebuild_index();
    }

    /// `shadowRoot.activeElement` — the focused node, when it is inside THIS
    /// shadow tree. Focus outside it is not this root's business.
    pub fn shadow_active_element(&self, shadow_id: u32) -> Option<u32> {
        let focused = self.focused_box;
        if focused == 0 { return None; }
        let host = self.host_of_shadow_root(shadow_id)?;
        let sr = self.find_webcore(host)?.shadow_root.as_ref()?;
        fn contains(nodes: &[WebCore], id: u32) -> bool {
            nodes.iter().any(|n| n.node_id == id || contains(&n.children, id))
        }
        if contains(&sr.children, focused) { Some(focused) } else { None }
    }

    /// `element.shadowRoot` — present only for an OPEN root. A closed root is
    /// invisible from outside, which is the whole point of closed mode.
    pub fn shadow_root_of(&self, id: u32) -> Option<u32> {
        let node = self.find_webcore(id)?;
        let sr = node.shadow_root.as_ref()?;
        match sr.mode {
            crate::types::ShadowMode::Open => Some(sr.node_id),
            crate::types::ShadowMode::Closed => None,
        }
    }

    /// Does this element have a shadow root, open or closed?
    pub fn has_shadow_root(&self, id: u32) -> bool {
        self.find_webcore(id).map(|n| n.shadow_root.is_some()).unwrap_or(false)
    }

    /// `shadowRoot.host` — the element a shadow tree is attached to.
    pub fn shadow_host(&self, node_id: u32) -> Option<u32> {
        fn walk(n: &WebCore, target: u32) -> Option<u32> {
            if let Some(sr) = &n.shadow_root {
                for c in &sr.children {
                    if contains(c, target) { return Some(n.node_id); }
                }
            }
            for c in &n.children {
                if let Some(h) = walk(c, target) { return Some(h); }
            }
            if let Some(sr) = &n.shadow_root {
                for c in &sr.children {
                    if let Some(h) = walk(c, target) { return Some(h); }
                }
            }
            None
        }
        fn contains(n: &WebCore, target: u32) -> bool {
            if n.node_id == target { return true; }
            if let Some(sr) = &n.shadow_root {
                if sr.children.iter().any(|c| contains(c, target)) { return true; }
            }
            n.children.iter().any(|c| contains(c, target))
        }
        walk(&self.root, node_id)
    }

    /// `shadowRoot.children` — the top-level nodes of an element's shadow tree.
    pub fn shadow_children(&self, id: u32) -> Vec<u32> {
        self.find_webcore(id)
            .and_then(|n| n.shadow_root.as_ref())
            .map(|sr| sr.children.iter().filter(|c| c.is_element()).map(|c| c.node_id).collect())
            .unwrap_or_default()
    }

    /// `shadowRoot.querySelector(sel)` — scoped to the shadow tree.
    ///
    /// `document.querySelector` deliberately does NOT reach in here; this is
    /// the way in, and without it a shadow tree was unreachable through any
    /// API at all.
    pub fn shadow_query_selector(&self, host: u32, selector: &str) -> Option<u32> {
        self.shadow_query_selector_all(host, selector).first().copied()
    }

    /// `shadowRoot.querySelectorAll(sel)`.
    pub fn shadow_query_selector_all(&self, host: u32, selector: &str) -> Vec<u32> {
        let Some(node) = self.find_webcore(host) else { return Vec::new() };
        let Some(sr) = node.shadow_root.as_ref() else { return Vec::new() };
        let mut out = Vec::new();
        for child in &sr.children {
            out.extend(matching_ids_from(child, selector, false));
        }
        out
    }

    /// `shadowRoot.innerHTML = html` — replace a shadow tree's contents.
    pub fn set_shadow_inner_html(&mut self, host: u32, html: &str) -> bool {
        match self.find_webcore_mut(host) {
            Some(node) => node.set_shadow_content(html),
            None => false,
        }
    }

    // ─── HTMLSlotElement (HTML §4.13.4) ─────────────────────────────────────

    /// `slot.assignedNodes()` — the light-DOM nodes projected into this slot.
    ///
    /// Empty when nothing matches, which is when the slot's own children render
    /// as FALLBACK instead. `flatten` is not modelled separately: nested slots
    /// are projected during `resolve_slots`, so the answer is already flat.
    pub fn assigned_nodes(&self, slot_id: u32) -> Vec<u32> {
        let Some(host) = self.shadow_host(slot_id) else { return Vec::new() };
        let Some(slot) = self.find_webcore(slot_id) else { return Vec::new() };
        if slot.tag != "slot" { return Vec::new(); }
        let name = slot.attributes.get("name").cloned().unwrap_or_default();
        let Some(host_node) = self.find_webcore(host) else { return Vec::new() };
        host_node.children.iter()
            .filter(|c| {
                let assigned = c.attributes.get("slot").cloned().unwrap_or_default();
                if name.is_empty() {
                    assigned.is_empty() && (c.is_element()
                        || (c.is_text_node() && !c.text.trim().is_empty()))
                } else {
                    assigned == name
                }
            })
            .map(|c| c.node_id)
            .collect()
    }

    /// `slot.assignedElements()` — `assignedNodes` without the text nodes.
    pub fn assigned_elements(&self, slot_id: u32) -> Vec<u32> {
        self.assigned_nodes(slot_id).into_iter()
            .filter(|id| self.find_webcore(*id).map(|n| n.is_element()).unwrap_or(false))
            .collect()
    }

    /// `element.assignedSlot` — the slot a light-DOM child is projected into,
    /// or `None` when it is not slotted.
    pub fn assigned_slot(&self, id: u32) -> Option<u32> {
        let parent = self.parent_node(id);
        if parent == 0 || !self.has_shadow_root(parent) { return None; }
        let want = self.find_webcore(id)?.attributes.get("slot").cloned().unwrap_or_default();
        fn find_slot(nodes: &[WebCore], want: &str) -> Option<u32> {
            for n in nodes {
                if n.tag == "slot" {
                    let name = n.attributes.get("name").map(|s| s.as_str()).unwrap_or("");
                    if name == want { return Some(n.node_id); }
                }
                if let Some(f) = find_slot(&n.children, want) { return Some(f); }
            }
            None
        }
        let host = self.find_webcore(parent)?;
        find_slot(&host.shadow_root.as_ref()?.children, &want)
    }

    /// Every `<slot>` in an element's shadow tree, in tree order.
    pub fn shadow_slots(&self, host: u32) -> Vec<u32> {
        let Some(node) = self.find_webcore(host) else { return Vec::new() };
        let Some(sr) = node.shadow_root.as_ref() else { return Vec::new() };
        fn collect(nodes: &[WebCore], out: &mut Vec<u32>) {
            for n in nodes {
                if n.tag == "slot" { out.push(n.node_id); }
                collect(&n.children, out);
            }
        }
        let mut out = Vec::new();
        collect(&sr.children, &mut out);
        out
    }

    /// Fire `slotchange` on every slot of a host whose assignment changed.
    ///
    /// The event does not bubble past the shadow root in the spec's sense, but
    /// it IS a real event a component listens for to react to its light DOM
    /// changing — and nothing fired it before.
    pub fn fire_slot_change(&mut self, host: u32) {
        for slot in self.shadow_slots(host) {
            let mut e = crate::dom::events::DomEvent::new("slotchange", slot);
            self.dispatch_dom_event(&mut e);
        }
    }

    /// `element.matches(selectors)`.
    ///
    /// Answered against the whole document, because a selector can name
    /// ancestors and siblings: `matches("div p")` is a question about where the
    /// element SITS, not about the element alone. It used to run the
    /// simple-selector matcher, which returned false for any selector with a
    /// combinator — and `closest()`, which is built on this, inherited that.
    pub fn matches(&self, id: u32, selectors: &str) -> bool {
        id != 0 && self.query_selector_all(selectors).contains(&id)
    }

    /// `element.closest(selectors)` — the nearest ancestor-or-self that
    /// matches. Starts at the element ITSELF, per DOM §4.9.
    pub fn closest(&self, id: u32, selectors: &str) -> Option<u32> {
        // The match set is resolved ONCE and then walked up, rather than
        // re-querying the document at every ancestor — `matches` is a whole-tree
        // query now, so the naive loop was O(depth × tree).
        let hits: std::collections::HashSet<u32> =
            self.query_selector_all(selectors).into_iter().collect();
        let mut current = id;
        while current != 0 {
            if hits.contains(&current) {
                return Some(current);
            }
            current = self.parent_node(current);
        }
        None
    }

    /// `getElementsByClassName(names)` — every element carrying ALL the named
    /// classes, in tree order. Space-separated, and an empty list matches
    /// nothing.
    pub fn get_elements_by_class_name(&self, names: &str) -> Vec<u32> {
        let wanted: Vec<&str> = names.split_whitespace().collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        self.get_elements_by_tag_name("*")
            .into_iter()
            .filter(|id| wanted.iter().all(|c| self.class_list_contains(*id, c)))
            .collect()
    }

    /// `element.hasAttributes()`.
    pub fn has_attributes(&self, id: u32) -> bool {
        !self.get_attribute_names(id).is_empty()
    }

    /// `element.toggleAttribute(name)` — adds when absent, removes when
    /// present, and answers whether it is present afterwards.
    pub fn toggle_attribute(&mut self, id: u32, name: &str) -> bool {
        if self.has_attribute(id, name) {
            self.remove_attribute(id, name);
            false
        } else {
            self.set_attribute(id, name, "");
            true
        }
    }

    /// `element.className` — the `class` attribute as written.
    pub fn class_name(&self, id: u32) -> String {
        self.get_attribute(id, "class").unwrap_or_default()
    }

    pub fn set_class_name(&mut self, id: u32, value: &str) {
        self.set_attribute(id, "class", value);
    }

    /// `element.id`.
    pub fn id(&self, id: u32) -> String {
        self.get_attribute(id, "id").unwrap_or_default()
    }

    pub fn set_id(&mut self, id: u32, value: &str) {
        self.set_attribute(id, "id", value);
    }

    /// `element.remove()` — ChildNode §4.2.9. Detaches the node from wherever
    /// it is, without the caller naming the parent.
    pub fn remove(&mut self, id: u32) {
        self.remove_child(id);
    }

    /// `document.documentElement` — the root element, `<html>`.
    pub fn document_element(&self) -> Option<u32> {
        Some(self.root.node_id).filter(|id| *id != 0)
    }

    /// `document.head`.
    pub fn head(&self) -> Option<u32> {
        self.query_selector("head")
    }

    /// `document.activeElement` — what has focus.
    pub fn active_element(&self) -> Option<u32> {
        (self.focused_box != 0).then_some(self.focused_box)
    }

    /// `element.blur()`.
    pub fn blur(&mut self, id: u32) {
        if self.focused_box == id {
            self.focused_box = 0;
        }
    }
}

// ─── Viewport ───────────────────────────────────────────────────────────────

impl Document {
    /// The viewport this document is laid out against, as `(width, height)`.
    ///
    /// `window.innerWidth` / `window.innerHeight` read THIS — a window's size
    /// and its document's viewport are one measurement, not two.
    pub fn viewport(&self) -> (f32, f32) {
        (self.viewport_w, self.viewport_h)
    }

    /// `window.resizeTo`, or a real window resize.
    ///
    /// Relayouts immediately: every percentage length, every `vw`/`vh` and the
    /// whole flow depend on this width, so a viewport change that has not been
    /// laid out is a document that disagrees with itself.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_w = width;
        self.viewport_h = height;
        crate::layout::LayoutEngine::new().layout(self, width);
    }
}

// ─── Name folding ───────────────────────────────────────────────────────────

impl Document {
    /// Fold a tag or attribute name the way THIS document does.
    ///
    /// One function, so the places that take a name — creating an element,
    /// writing an attribute, reading one back, looking up by tag — cannot
    /// disagree about whether this document folds.
    ///
    /// HTML is ASCII-case-insensitive for these names, so `createElement("DIV")`
    /// makes a `div` and `getAttribute("HREF")` finds `href`. XML is
    /// case-sensitive: `<Rect>` and `<rect>` are two different elements, and
    /// folding one into the other would silently merge them.
    pub(crate) fn fold_name(&self, name: &str) -> String {
        match self.kind {
            crate::types::DocumentKind::Html => name.to_ascii_lowercase(),
            crate::types::DocumentKind::Xml => name.to_string(),
        }
    }
}

// ─── XML: namespaces, CDATA, processing instructions ────────────────────────
//
// webcore parses HTML, where every element is in the HTML namespace and
// `prefix` is always null — so none of this was reachable. It becomes
// reachable through `createElementNS` and friends, which is the only way an
// XML tree is built here.
//
// `prefix` and `localName` are DERIVED from the qualified name rather than
// stored. One name asked two ways cannot disagree with itself; three fields
// could.

impl Document {
    /// `document.createElementNS(namespace, qualifiedName)`.
    pub fn create_element_ns(&mut self, namespace: &str, qualified_name: &str) -> u32 {
        let arena_id = self.arena.create_element_ns(namespace, qualified_name);
        let mut b = WebCore::new(qualified_name);
        b.node_id = arena_id.0;
        apply_property(&mut b.style, "display", crate::html::default_display(qualified_name));
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// `document.createCDATASection(data)`.
    ///
    /// `display:none` for the same reason a comment gets it: CDATA is
    /// character data in the tree, and the render box must not paint it as if
    /// it were an element's own text.
    pub fn create_cdata_section(&mut self, data: &str) -> u32 {
        let arena_id = self.arena.create_cdata(data);
        let mut b = WebCore::new("#cdata-section");
        b.node_id = arena_id.0;
        b.text = data.to_string();
        apply_property(&mut b.style, "display", "none");
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// `document.createProcessingInstruction(target, data)`.
    pub fn create_processing_instruction(&mut self, target: &str, data: &str) -> u32 {
        let arena_id = self.arena.create_processing_instruction(target, data);
        let mut b = WebCore::new(target);
        b.node_id = arena_id.0;
        b.text = data.to_string();
        apply_property(&mut b.style, "display", "none");
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// `node.namespaceURI` — `None` for the null namespace, which is what
    /// every HTML element built by the parser has.
    pub fn namespace_uri(&self, id: u32) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return None; }
        self.arena.get(NodeId(id)).namespace.clone()
    }

    /// `node.prefix` — the part of the qualified name before the colon.
    pub fn prefix(&self, id: u32) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return None; }
        self.arena
            .get(NodeId(id))
            .tag
            .split_once(':')
            .map(|(prefix, _)| prefix.to_string())
    }

    /// `node.localName` — the qualified name without its prefix.
    pub fn local_name(&self, id: u32) -> String {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return String::new(); }
        let tag = &self.arena.get(NodeId(id)).tag;
        match tag.split_once(':') {
            Some((_, local)) => local.to_string(),
            None => tag.clone(),
        }
    }

    /// `element.setAttributeNS(namespace, qualifiedName, value)`.
    ///
    /// The value goes in under the QUALIFIED name, so `getAttribute` and
    /// serialization keep working unchanged; the namespace is recorded beside
    /// it so `getAttributeNS` can tell `xlink:href` from `href`.
    pub fn set_attribute_ns(
        &mut self,
        id: u32,
        namespace: &str,
        qualified_name: &str,
        value: &str,
    ) {
        if id == 0 { return; }
        self.set_attribute(id, qualified_name, value);
        if !self.arena.is_alive(NodeId(id)) { return; }
        let node = self.arena.get_mut(NodeId(id));
        if namespace.is_empty() {
            // An empty namespace is NULL, not the empty string — so the entry
            // is removed rather than stored blank.
            node.attribute_ns.remove(qualified_name);
        } else {
            node.attribute_ns
                .insert(qualified_name.to_string(), namespace.to_string());
        }
    }

    /// `element.getAttributeNS(namespace, localName)`.
    ///
    /// A different question from `getAttribute`, not a spelling of it: the
    /// match is on (namespace, localName), so an attribute with the right
    /// local name but the wrong namespace does NOT answer.
    pub fn get_attribute_ns(
        &self,
        id: u32,
        namespace: &str,
        local_name: &str,
    ) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return None; }
        let want = (!namespace.is_empty()).then_some(namespace);
        let node = self.arena.get(NodeId(id));
        for (name, value) in &node.attributes {
            let local = match name.split_once(':') {
                Some((_, local)) => local,
                None => name.as_str(),
            };
            if local != local_name {
                continue;
            }
            let have = node.attribute_ns.get(name).map(String::as_str);
            if have == want {
                return Some(value.clone());
            }
        }
        None
    }
}

// ─── HTMLSelectElement / HTMLOptionElement ──────────────────────────────────
//
// The items of a `<select>` are its `<option>` CHILDREN — HTML has no separate
// item list, and webcore's own renderer already reads them that way (it walks
// `sel.children` collecting `option` and flattening `optgroup`). So these are
// ordinary tree operations, and an item added here is one the renderer draws
// and the serializer round-trips, with nothing to keep in sync.
//
// Selection lives on the OPTIONS, as `selectedness` — the state HTML defines
// and the state webcore's mouse and keyboard paths write. Reading and writing
// THAT is what makes a programmatic selection and a user's click mean the same
// thing. `html::forms` holds the algorithms over it.

impl Document {
    /// Option node_ids in tree order, flattening `<optgroup>` — HTML counts
    /// options through a group, not around it, so `selectedIndex` does too.
    fn option_ids(&self, select: u32) -> Vec<u32> {
        // Delegates rather than walking again: two copies of "which options
        // does this select have" drift, and this one already differed from the
        // spec's — it descended into a NESTED `<optgroup>`, which the list of
        // options excludes.
        self.find_webcore(select)
            .map(crate::html::forms::option_ids)
            .unwrap_or_default()
    }

    /// `select.options.length`.
    pub fn item_count(&self, select: u32) -> usize {
        self.option_ids(select).len()
    }

    /// `select.add(new Option(text))` — append an `<option>` carrying `text`.
    pub fn add_item(&mut self, select: u32, text: &str) {
        if select == 0 { return; }
        let option = self.create_element("option");
        let label = self.create_text_node(text);
        self.append_child(option, label);
        self.append_child(select, option);
        self.notify_select_changed(select);
    }

    /// `select.remove(index)`. Out of range removes nothing, as the IDL says.
    pub fn remove_item(&mut self, select: u32, index: usize) {
        if let Some(&id) = self.option_ids(select).get(index) {
            self.remove_child(id);
            // Removing the selected option leaves a drop-down with none, and
            // the algorithm is what picks its replacement.
            self.notify_select_changed(select);
        }
    }

    /// Drop every option. `select.length = 0` in the IDL.
    pub fn clear_items(&mut self, select: u32) {
        for id in self.option_ids(select) {
            self.remove_child(id);
        }
        self.notify_select_changed(select);
    }

    /// The label of the option at `index` — its text content.
    pub fn item_text(&self, select: u32, index: usize) -> String {
        match self.option_ids(select).get(index) {
            Some(&id) => self.text_content(id).trim().to_string(),
            None => String::new(),
        }
    }

    pub fn set_item_text(&mut self, select: u32, index: usize, text: &str) {
        if let Some(&id) = self.option_ids(select).get(index) {
            self.set_text_content(id, text);
            // A closed drop-down shows the LABEL, so renaming the selected
            // option has to move the shown text with it.
            self.notify_select_changed(select);
        }
    }

    /// `select.selectedIndex` — "the index of the first option element in the
    /// list of options that has its selectedness set to true. If there isn't
    /// one, then it is −1."
    ///
    /// ⛔ −1 is not only the empty-select answer. A LIST BOX rests with nothing
    /// selected, because the selectedness setting algorithm auto-selects only
    /// at a display size of 1. This used to fall back to 0 whenever any option
    /// existed, so a fresh list box claimed a selection it did not have.
    pub fn selected_index(&self, select: u32) -> i32 {
        self.find_webcore(select)
            .map(crate::html::forms::selected_index)
            .unwrap_or(-1)
    }

    /// `select.selectedIndex = i`.
    ///
    /// Writes SELECTEDNESS — the same state a click and an arrow key write, so
    /// a programmatic selection and a user's are indistinguishable afterwards.
    ///
    /// "Set the selectedness of all the option elements ... to false, and then
    /// set the selectedness of the option element ... with index `index` to
    /// true", and the IDL setter raises DIRTINESS, which is what stops a
    /// subsequent form reset from being a no-op.
    ///
    /// ⛔ It does NOT touch the `selected` content attribute. That attribute is
    /// `defaultSelected` — the author's default and the reset target — and
    /// writing it here is why a reset used to restore the last programmatic
    /// selection instead of the markup's.
    pub fn set_selected_index(&mut self, select: u32, index: i32) {
        let options = crate::html::forms::option_ids(
            match self.find_webcore(select) {
                Some(n) => n,
                None => return,
            },
        );
        // A negative index — or any out-of-range one — means "nothing
        // selected", which selectedness can now actually express.
        let chosen = usize::try_from(index).ok().filter(|i| *i < options.len()).map(|i| options[i]);
        if let Some(sel) = self.find_webcore_mut(select) {
            crate::html::forms::for_each_option_mut(sel, &mut |option, _| {
                option.selectedness = Some(option.node_id) == chosen;
                option.dirty_selectedness = true;
            });
            sel.layout.layout_dirty = true;
        }
        // A closed drop-down shows a child text node rather than its options,
        // so the shown label has to follow the selection. `option:checked` is a
        // selector, so this is a style change too.
        if let Some(sel) = self.find_webcore_mut(select) {
            crate::html::forms::refresh_select_display_text(sel);
        }
        self.style_dirty = true;
    }

    /// `element.value` — which means three different things, exactly as HTML
    /// says it does.
    ///
    /// A `<textarea>`'s value is its text content; a `<select>`'s is the VALUE
    /// OF ITS SELECTED OPTION (falling back to that option's label, per
    /// `option.value`); everything else reads the `value` attribute. webcore's
    /// `input_value` covers the first and last but answers a `<select>` from a
    /// `value` attribute a select does not have.
    pub fn value(&self, id: u32) -> String {
        if id == 0 { return String::new(); }
        let Some(node) = self.find_webcore(id) else { return String::new() };
        let tag = node.tag.clone();
        let input_mode = crate::html::forms::value_mode(node);
        match tag.as_str() {
            // The VALUE, which is the raw value once anything has set it and
            // the child text (the default value) until then.
            "textarea" => self
                .find_webcore(id)
                .map(crate::types::input_value)
                .unwrap_or_default(),
            "select" => self
                .find_webcore(id)
                .map(crate::html::forms::select_value)
                .unwrap_or_default(),
            // ⛔ WHICH MODE the `value` IDL attribute is in decides where to
            // read from (HTML §4.10.5.4). A checkbox's value is its `value`
            // ATTRIBUTE, defaulting to `"on"` — its STATE is checkedness. Only
            // a mode-`Value` control holds a value of its own.
            "input" if input_mode != crate::html::forms::ValueMode::Value => {
                match self.get_attribute(id, "value") {
                    Some(v) => v,
                    None if input_mode == crate::html::forms::ValueMode::DefaultOn => {
                        "on".to_string()
                    }
                    None => String::new(),
                }
            }
            // Everything else answers with its VALUE. Reading the `value`
            // ATTRIBUTE here made `input.value` report the author's default
            // rather than what the control holds — so a field the user had
            // typed into read back empty.
            _ => self
                .find_webcore(id)
                .map(crate::types::input_value)
                .unwrap_or_default(),
        }
    }

    /// `element.value = v`, the setter half of the above.
    pub fn set_value(&mut self, id: u32, value: &str) {
        if id == 0 { return; }
        let tag = self.find_webcore(id).map(|n| n.tag.clone()).unwrap_or_default();
        match tag.as_str() {
            // The IDL setter sets the VALUE and raises the dirty value flag
            // (HTML §4.10.5.4) — it does not rewrite the markup, which is what
            // `set_text_content` and `set_attribute` were doing here. A reset
            // after a programmatic write used to restore the written value.
            "textarea" => self.set_control_value(id, value),
            // Assigning to `select.value` selects the option WITH that value —
            // it does not store a string. An unmatched value selects nothing.
            "select" => {
                let options = self.option_ids(id);
                let found = options.iter().position(|o| {
                    let v = self
                        .get_attribute(*o, "value")
                        .unwrap_or_else(|| self.text_content(*o).trim().to_string());
                    v == value
                });
                self.set_selected_index(id, found.map_or(-1, |i| i as i32));
            }
            _ => self.set_control_value(id, value),
        }
    }

    /// The `value` IDL setter, per the control's MODE (HTML §4.10.5.4).
    ///
    /// ⛔ Only mode `Value` writes the value state. A checkbox, a radio and a
    /// button write the `value` CONTENT ATTRIBUTE — that attribute IS their
    /// value, so routing them through the state made `checkbox.value = "true"`
    /// read back as `"on"`.
    fn set_control_value(&mut self, id: u32, value: &str) {
        let mode = match self.find_webcore(id) {
            Some(n) => crate::html::forms::value_mode(n),
            None => return,
        };
        // "If the new value is different from the old, move the text entry
        // cursor to the end and unselect" (HTML §4.10.5.4). Assigning the SAME
        // string leaves the selection where it was — measured both ways:
        // a (2,4) selection survives `value = "abcdef"` on a value that already
        // reads `"abcdef"`, and collapses to `[6,6]` when it does not.
        let value_changed = self.value(id) != value;
        if mode != crate::html::forms::ValueMode::Value {
            self.set_attribute(id, "value", value);
            return;
        }
        if let Some(node) = self.find_webcore_mut(id) {
            node.value_state = Some(value.to_string());
            node.dirty_value = true;
            node.layout.layout_dirty = true;
            // "Invoke the value sanitization algorithm, if the element's type
            // attribute's current state defines one" — a range assigned an
            // off-step number snaps, exactly as it does from the markup.
            let sanitized = {
                let is_range = node
                    .attributes
                    .get("type")
                    .map(|t| t.trim().eq_ignore_ascii_case("range"))
                    .unwrap_or(false);
                if is_range {
                    Some(crate::html::forms::best_representation(
                        crate::html::forms::sanitize_range_value(node, value),
                    ))
                } else {
                    None
                }
            };
            if let Some(v) = sanitized {
                node.value_state = Some(v);
            }
        }
        if value_changed {
            let end = self.value(id).chars().count();
            if let Some(node) = self.find_webcore_mut(id) {
                node.input_cursor = end;
                node.input_sel_anchor = end;
                node.input_sel_direction = crate::types::SelectionDirection::None;
            }
        }
    }
}

// ─── HTMLDialogElement ──────────────────────────────────────────────────────
//
// HTML §4.11.4. Openness is the `open` CONTENT ATTRIBUTE — the IDL property
// reflects it, so there is no separate "is it showing" flag to drift out of
// step, and markup that arrives with `<dialog open>` is already open without
// anyone calling `show()`.
//
// `display` and the non-modal `position` now come from the UA stylesheet
// (`dialog:not([open]) { display: none }` and the `dialog` block in
// `css::UA_CSS`), which is where the spec puts them. This file used to write
// both as INLINE styles because the sheet had no `dialog` entry — that made a
// dialog that had never been opened render in flow, and made an opened one
// immune to the author's own `display`, since an inline style beats every
// rule. Setting the attribute is the whole of `show()`; the cascade does the
// rest.
//
// `position` on a MODAL is the one thing still written here: a modal is laid
// out against the VIEWPORT and a non-modal stays with its containing block,
// and the difference is `dialog:modal`, a pseudo-class keyed on "is in the top
// layer" that the matcher has no state for yet. If both looked alike,
// `showModal()` would be `show()` under another name.

impl Document {
    /// `dialog.show()` / `dialog.showModal()`.
    pub fn show_dialog(&mut self, id: u32, modal: bool) {
        if id == 0 { return; }
        self.set_attribute(id, "open", "");
        if modal {
            self.set_style_property(id, "position", "fixed");
        }
    }

    /// `dialog.close()`.
    pub fn close_dialog(&mut self, id: u32) {
        if id == 0 { return; }
        self.remove_attribute(id, "open");
        // Drop the modal's `position` override with it, so a later `show()`
        // does not inherit `showModal()`'s layout.
        self.set_style_property(id, "position", "");
    }

    /// `dialog.open` — reflects the content attribute, per the IDL.
    pub fn dialog_open(&self, id: u32) -> bool {
        id != 0 && self.get_attribute(id, "open").is_some()
    }
}

// ─── WHATWG Node accessors ──────────────────────────────────────────────────
//
// The arena has carried the model for all of these from the start —
// `NodeType`, the attribute map, the parent chain. What it lacked was the IDL
// spelling on top. These are that spelling, and nothing more: no new state, no
// second store to keep in sync.
//
// The numbering is DOM §4.4 `nodeType`, which is also what `vybe_widgets`
// answers — the two engines are only interchangeable if they agree digit for
// digit, so these constants are copied from the spec table rather than
// re-derived.

impl Document {
    /// `node.nodeType` — DOM §4.4. `0` for a node that does not exist, which
    /// is not a spec value: the spec has no way to ask about a missing node.
    pub fn node_type(&self, id: u32) -> u16 {
        // DOCUMENT_NODE. The document is not in the arena — it is a reserved
        // id — so this answers before the liveness check, which would reject it.
        if crate::dom::events::is_document_target(id) { return 9; }
        // A `ShadowRoot` IS a `DocumentFragment` (DOM §4.8) — but only the
        // ROOT. Every node INSIDE a shadow tree is numbered from the same
        // descending id space, so "is a shadow id" would call a shadow `<p>` a
        // fragment; the question is whether this id names a shadow root.
        if crate::dom::arena::is_shadow_node_id(id) {
            if self.shadow_root_by_id(id).is_some() { return 11; }
            return match self.find_webcore(id) {
                Some(n) if n.tag == "#text" => 3,
                Some(n) if n.tag == "#comment" => 8,
                Some(_) => 1,
                None => 0,
            };
        }
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return 0; }
        match self.arena.get(NodeId(id)).node_type {
            crate::dom::arena::NodeType::Element  => 1,
            crate::dom::arena::NodeType::Text     => 3,
            crate::dom::arena::NodeType::CData    => 4,
            crate::dom::arena::NodeType::ProcessingInstruction => 7,
            crate::dom::arena::NodeType::Comment  => 8,
            crate::dom::arena::NodeType::Document => 9,
            crate::dom::arena::NodeType::DocumentType => 10,
            crate::dom::arena::NodeType::DocumentFragment => 11,
        }
    }

    /// `node.nodeName` — the tag for an element, `#text` / `#comment` /
    /// `#document` for the rest. The arena already stores exactly those
    /// strings in `tag`, so this is a read, not a mapping.
    ///
    /// NOT upper-cased. WHATWG upper-cases only for HTML elements in an HTML
    /// document, and `vybe_widgets::node_name` returns the tag verbatim; an
    /// engine that upper-cased here would answer differently from the engine
    /// it is meant to replace.
    pub fn node_name(&self, id: u32) -> String {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return String::new(); }
        let node = self.arena.get(NodeId(id));
        let name = node.tag.clone();
        // **An element's `nodeName` is its HTML-UPPERCASED qualified name**
        // (DOM §4.9): "Let qualifiedName be this's qualified name. If this is
        // in the HTML namespace and its node document is an HTML document,
        // then return qualifiedName in ASCII uppercase. Return qualifiedName."
        //
        // This answered the STORED tag, which HTML folds to lowercase on the
        // way in, so `el.nodeName == "DIV"` — the check a page actually writes
        // — was false for every element. `local_name` is the lowercase half.
        //
        // ⛔ BOTH conditions, not just the document. An `<svg:rect>` inside an
        // HTML page is in the SVG namespace and keeps its case; uppercasing on
        // the document kind alone renamed it to `SVG:RECT`, which the
        // namespace test here caught.
        let html_namespace = match node.namespace.as_deref() {
            None => true,
            Some(ns) => ns == crate::dom::HTML_NAMESPACE,
        };
        if matches!(self.kind, crate::types::DocumentKind::Html)
            && matches!(node.node_type, crate::dom::arena::NodeType::Element)
            && html_namespace
        {
            return name.to_ascii_uppercase();
        }
        name
    }

    /// `node.nodeValue` — the data of a text or comment node. `None` for an
    /// element and for the document, per DOM §4.4.
    pub fn node_value(&self, id: u32) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return None; }
        let node = self.arena.get(NodeId(id));
        // DOM §4.4: `nodeValue` is non-null for text, CDATA, comment and
        // processing-instruction nodes — everything that carries DATA. It is
        // null for an element and for the document.
        match node.node_type {
            crate::dom::arena::NodeType::Text
            | crate::dom::arena::NodeType::CData
            | crate::dom::arena::NodeType::Comment
            | crate::dom::arena::NodeType::ProcessingInstruction => Some(node.text.clone()),
            _ => None,
        }
    }

    /// `node.isConnected` — whether the node's root is the document.
    ///
    /// Walks to the root rather than asking "does it have a parent": a child
    /// of a DETACHED subtree has a parent and is still not connected. A node
    /// sitting in `pending_nodes` was created but never inserted, so it is
    /// disconnected by construction.
    pub fn is_connected(&self, id: u32) -> bool {
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return false; }
        if self.pending_nodes.contains_key(&id) { return false; }
        let root = self.root.node_id;
        if id == root { return true; }
        let mut cur = NodeId(id);
        while cur.is_some() {
            let parent = self.arena.get(cur).parent;
            if parent.0 == root { return true; }
            cur = parent;
        }
        false
    }

    /// `element.getAttributeNames()` — in insertion order is NOT guaranteed
    /// here: the arena stores attributes in a `HashMap`, so the order is
    /// arbitrary. WHATWG specifies document order; callers that care must
    /// sort. Named here rather than silently differing.
    pub fn get_attribute_names(&self, id: u32) -> Vec<String> {
        if id == 0 { return Vec::new(); }
        // DOM §4.9: "the qualified names of the attributes in this element's
        // attribute list, in order" — the list order, not a hash order.
        //
        // Through `attribute_entries`, so a SHADOW element answers too: it has
        // no arena entry, and reading the arena directly returned an empty
        // list for every node in a shadow tree.
        self.attribute_entries(id).into_iter().map(|(k, _)| k).collect()
    }

    /// `document.getElementsByTagName()` — tree order, ASCII case-insensitive
    /// as HTML requires (`<DIV>` and `<div>` are the same tag). Returns a
    /// SNAPSHOT, not a live `HTMLCollection`; liveness cannot survive a
    /// `Vec<u32>` return and is lost at this boundary either way.
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<u32> {
        // Folded, not blanket-lowercased: an HTML document matches
        // case-insensitively because BOTH sides fold, while an XML document
        // must distinguish `<Rect>` from `<rect>`. Hardcoding
        // `eq_ignore_ascii_case` here would have made XML lookups match the
        // wrong elements.
        let want = self.fold_name(tag);
        let all = want == "*";
        let mut out = Vec::new();
        fn walk(doc: &Document, node: &WebCore, want: &str, all: bool, out: &mut Vec<u32>) {
            if all || doc.fold_name(&node.tag) == want {
                // Text and comment boxes carry `#text` / `#comment` as their
                // tag and are not elements, so `*` must not collect them.
                if doc.node_type(node.node_id) == 1 {
                    out.push(node.node_id);
                }
            }
            for child in &node.children {
                walk(doc, child, want, all, out);
            }
        }
        walk(self, &self.root, &want, all, &mut out);
        out
    }

    /// `document.title`.
    ///
    /// Read from the document's own field, NOT by finding a `<title>` element:
    /// the parser lifts the title out of `<head>` as it goes and head content
    /// is not kept as a render box, so looking for the element answered `""`
    /// for every parsed document.
    pub fn title(&self) -> String {
        self.title.clone()
    }

    /// `document.title = …`.
    ///
    /// Writes the field, AND the `<title>` element when the document has one —
    /// a tree built through `createElement` keeps its element, and the two must
    /// not disagree about what the title is.
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
        if let Some(node) = self.query_selector("title") {
            self.set_text_content(node, title);
        }
    }
}

// ─── Mutate ─────────────────────────────────────────────────────────────────

impl Document {
    /// Create a new element node. Returns its stable node_id.
    /// The node is detached — use `dom_append_child` or `dom_insert_before` to attach it.
    pub fn create_element(&mut self, tag: &str) -> u32 {
        // `createElement("DIV")` makes a `div` in an HTML document — the name
        // is folded here so every later lookup by tag agrees with it.
        // `createElementNS` deliberately does NOT fold; see it below.
        let tag = &self.fold_name(tag);
        let arena_id = self.arena.create_element(tag);
        let mut b = WebCore::new(tag);
        b.node_id = arena_id.0;
        apply_property(&mut b.style, "display", crate::html::default_display(tag));
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// `document.createTextNode(data)`.
    pub fn create_text_node(&mut self, text: &str) -> u32 {
        let arena_id = self.arena.create_text(text);
        let mut b = WebCore::new("#text");
        b.node_id = arena_id.0;
        b.text = text.to_string();
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// Create a new comment node — `document.createComment()`. Detached, like
    /// its element and text siblings above.
    ///
    /// The render box is `display:none`: a comment is in the tree and in
    /// `childNodes`, and it draws nothing. Without that it would inherit the
    /// default display for an unknown tag and paint its own text onto the page.
    pub fn create_comment(&mut self, text: &str) -> u32 {
        let arena_id = self.arena.create_comment(text);
        let mut b = WebCore::new("#comment");
        b.node_id = arena_id.0;
        b.text = text.to_string();
        apply_property(&mut b.style, "display", "none");
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// Append a child node to a parent.
    /// If child is in pending_nodes (just created), it's moved into the tree.
    /// If child is already in the tree, it's detached from its current parent first.
    pub fn append_child(&mut self, parent_id: u32, child_id: u32) {
        if parent_id == 0 || child_id == 0 { return; }

        // **A fragment inserts its CHILDREN and not itself** (DOM §4.2.1).
        // Appending the fragment node would put a `#document-fragment` box in
        // the tree — something no selector matches and no layout knows — and
        // leave the caller's nodes one level deeper than they asked for.
        if self.is_document_fragment(child_id) {
            for node in self.child_nodes(child_id) {
                self.append_child(parent_id, node);
            }
            return;
        }

        // Update arena
        let arena_parent = self.arena.try_get(NodeId(child_id))
            .map_or(crate::dom::arena::NodeId::NONE, |n| n.parent);
        if arena_parent.is_some() {
            self.arena.remove_child(NodeId(child_id));
        }
        self.arena.append_child(NodeId(parent_id), NodeId(child_id));

        // Update WebCore tree
        let child_box = if let Some(b) = self.pending_nodes.remove(&child_id) {
            b
        } else {
            self.detach_webcore(child_id).unwrap_or_else(|| WebCore::new("#error"))
        };

        if let Some(parent) = self.find_webcore_mut(parent_id) {
            parent.children.push(child_box);
            parent.layout.layout_dirty = true;
            parent.layout.intrinsic_dirty = true;
            parent.has_dirty_layout_descendant = true;
        }
    }

    /// Insert a child before a reference node.
    pub fn insert_before(&mut self, parent_id: u32, child_id: u32, reference_id: u32) {
        if parent_id == 0 || child_id == 0 || reference_id == 0 { return; }

        // A fragment splices its children in — see `append_child`. Each goes
        // before the SAME reference, which is what keeps them in order.
        if self.is_document_fragment(child_id) {
            for node in self.child_nodes(child_id) {
                self.insert_before(parent_id, node, reference_id);
            }
            return;
        }

        // Update arena
        let arena_parent = self.arena.try_get(NodeId(child_id))
            .map_or(crate::dom::arena::NodeId::NONE, |n| n.parent);
        if arena_parent.is_some() {
            self.arena.remove_child(NodeId(child_id));
        }
        self.arena.insert_before(NodeId(parent_id), NodeId(child_id), NodeId(reference_id));

        // Update WebCore tree
        let child_box = if let Some(b) = self.pending_nodes.remove(&child_id) {
            b
        } else {
            self.detach_webcore(child_id).unwrap_or_else(|| WebCore::new("#error"))
        };

        if let Some(parent) = self.find_webcore_mut(parent_id) {
            let idx = parent.children.iter()
                .position(|c| c.node_id == reference_id)
                .unwrap_or(parent.children.len());
            parent.children.insert(idx, child_box);
            parent.layout.layout_dirty = true;
            parent.layout.intrinsic_dirty = true;
            parent.has_dirty_layout_descendant = true;
        }
    }

    /// Remove a child from its parent. The node is dropped from the WebCore tree
    /// and freed in the arena.
    pub fn remove_child(&mut self, child_id: u32) {
        if child_id == 0 { return; }
        // Get parent before removing
        let parent_id = self.arena.try_get(NodeId(child_id)).map_or(0, |n| n.parent.0);
        self.arena.remove_child(NodeId(child_id));
        // **A removed node is DETACHED, not destroyed** (DOM §4.2.3):
        // `removeChild` hands the node back and the caller may insert it
        // somewhere else, which is how every "move this node" is written.
        // Freeing the arena slot here made that idiom read a dead node —
        // `remove` then `appendChild` elsewhere lost the subtree silently, and
        // `replaceWith` left the replaced node unusable.
        //
        // It goes where a freshly CREATED node goes, because it is now in the
        // same state: detached, with an id, waiting to be attached. That is
        // the map `append_child` and `insert_before` already look in.
        if let Some(detached) = self.detach_webcore(child_id) {
            self.pending_nodes.insert(child_id, detached);
        }
        // Mark parent dirty for layout
        if parent_id != 0 {
            if let Some(parent) = self.find_webcore_mut(parent_id) {
                parent.layout.layout_dirty = true;
                parent.layout.intrinsic_dirty = true;
                parent.has_dirty_layout_descendant = true;
            }
        }
    }

    /// Set an attribute on an element. Sets STYLE dirty flag + layout dirty.
    pub fn set_attribute(&mut self, id: u32, key: &str, value: &str) {
        if id == 0 { return; }
        let key = &self.fold_name(key);
        self.arena.set_attribute(NodeId(id), key, value);
        let mut canvas_resized = None;
        if let Some(node) = self.find_webcore_mut(id) {
            node.attributes.insert(key, value);
            node.layout.layout_dirty = true;
            node.layout.intrinsic_dirty = true;
            // "When the `checked` content attribute is added, if the control
            // does not have dirty checkedness, the user agent must set the
            // checkedness of the element to true" (HTML §4.10.5.3). The
            // attribute is `defaultChecked`, so it only reaches the live state
            // while nothing has claimed that state yet.
            if key == "checked" && !node.dirty_checked {
                node.checkedness = true;
            }
            // The same rule for the other two states the markup seeds. A
            // content attribute reaches the live state only while nothing has
            // claimed that state — which is what makes it the DEFAULT.
            if key == "value" && !node.dirty_value {
                node.value_state = None;
                crate::html::forms::seed_input_value(node);
            }
            if key == "selected" && node.tag == "option" && !node.dirty_selectedness {
                node.selectedness = true;
            }
            // §4.12.5: `<canvas>`'s `width`/`height` IDL attributes REFLECT
            // these content attributes, and setting either one reinitialises
            // the bitmap and the drawing state. Reached this way — rather than
            // only through `set_canvas_size` — because `setAttribute` is a
            // route a page has to the same value, and two ways to set one
            // thing must not differ in what they do.
            if node.tag == "canvas" && (key == "width" || key == "height") {
                if let Ok(n) = value.trim().parse::<u32>() {
                    canvas_resized = Some(if key == "width" {
                        (n, node.image_height)
                    } else {
                        (node.image_width, n)
                    });
                }
            }
        }
        if let Some((w, h)) = canvas_resized {
            self.set_canvas_size(id, w, h);
        }
        // "If an option element in the list of options asks for a reset, then
        // run that select element's selectedness setting algorithm" — the
        // second half of the `selected` rule above, which only the PARENT can
        // do: it is what enforces one-selection-at-a-time and what refreshes
        // the label a closed drop-down shows.
        if key == "selected" {
            if let Some(select) = self.parent_select_of(id) {
                self.notify_select_changed(select);
            }
        }
    }

    /// The `<select>` an option belongs to, if any.
    pub(crate) fn parent_select_of(&self, option: u32) -> Option<u32> {
        fn walk(node: &WebCore, target: u32) -> Option<u32> {
            if node.tag == "select" && crate::html::forms::option_ids(node).contains(&target) {
                return Some(node.node_id);
            }
            node.children.iter().find_map(|c| walk(c, target))
        }
        walk(&self.root, option)
    }

    /// Re-settle a `<select>` after its options changed: run the selectedness
    /// setting algorithm, then move the shown label to match.
    ///
    /// ⛔ Needed on every route that adds or removes an option, not just the
    /// parser's. A drop-down built through the DOM had no selection at all,
    /// because only the parse path ever ran the algorithm — so a `ComboBox`
    /// filled by `AddItem` came up blank where the same markup came up on its
    /// first option.
    pub(crate) fn notify_select_changed(&mut self, select: u32) {
        if let Some(sel) = self.find_webcore_mut(select) {
            crate::html::forms::run_selectedness_setting_algorithm(sel);
            crate::html::forms::refresh_select_display_text(sel);
            sel.layout.layout_dirty = true;
        }
        self.style_dirty = true;
    }

    /// Remove an attribute from an element. Sets STYLE dirty flag.
    pub fn remove_attribute(&mut self, id: u32, key: &str) {
        if id == 0 { return; }
        let key = &self.fold_name(key);
        self.arena.remove_attribute(NodeId(id), key);
        if let Some(node) = self.find_webcore_mut(id) {
            node.attributes.remove(key);
            // The other half of the same sentence: "when the `checked` content
            // attribute is removed, if the control does not have dirty
            // checkedness, the user agent must set the checkedness of the
            // element to false."
            if key == "checked" && !node.dirty_checked {
                node.checkedness = false;
            }
        }
    }

    /// Set the text content of a node, replacing all children.
    pub fn set_text_content(&mut self, id: u32, text: &str) {
        if id == 0 { return; }

        // **A replaced child is DETACHED, not destroyed** — the rule
        // `remove_child` already states for DOM §4.2.3, and this is the same
        // removal. Freeing the slot here did more than lose a node a script
        // might re-insert: it RECYCLED the id, and everything keyed by node id
        // is then aliased. The listener map is: the next element created took a
        // dead node's id and inherited its handlers, so a control that cleared
        // and rebuilt itself ended up with two listeners on one button, one of
        // them belonging to an element that no longer existed.
        for child in self.child_nodes(id) {
            self.remove_child(child);
        }
        if let Some(node) = self.find_webcore_mut(id) {
            node.text.clear();
        }

        // **The replacement is a TEXT NODE, not a string on the element**
        // (DOM §4.4: "string replace all"). An element's own `text` field is
        // read by layout for a text node and for a pseudo-element and nowhere
        // else — `has_direct_text` in `layout/mod.rs` — so writing it here put
        // the words in the DOM, in `textContent` and in `outerHTML` while
        // laying out and painting NOTHING.
        //
        // That is what emptied the .NET forms: a label's caption, a button's
        // face and a form's title all arrive as `textContent`, so every control
        // rendered as a bare rectangle. Markup that came through the parser was
        // unaffected — it builds real `#text` boxes — which is exactly why a
        // page written in HTML showed its words and a page built through the
        // DOM did not.
        //
        // Empty text is the spec's null case: children are removed and NO node
        // is inserted, which is also what keeps `<p></p>` from gaining a child
        // the markup never had.
        if !text.is_empty() {
            let node = self.create_text_node(text);
            self.append_child(id, node);
            // A node that has just JOINED the tree has no computed style —
            // `append_child` marks layout dirty but nothing re-runs the
            // cascade, and a text run with no inherited font measures to
            // nothing. `:empty` also stops matching the parent, which is a
            // second reason this is a style change and not only a layout one.
            self.style_dirty = true;
        }
    }

    /// Parse HTML and replace the children of the given node.
    pub fn set_inner_html(&mut self, id: u32, html: &str) {
        if id == 0 { return; }

        // Parse HTML fragment
        let fragment = crate::html::parse_html(html);
        // The fragment comes back wrapped in `<html><head></head><body>…`, and
        // `innerHTML` means the CONTENT. Take the body's children by finding
        // body, not by indexing: this used to check `children[0]`, which the
        // synthesised `<head>` displaced.
        let new_children: Vec<WebCore> = match fragment
            .root
            .children
            .iter()
            .position(|c| c.tag == "body")
        {
            Some(at) => {
                let mut children = fragment.root.children;
                std::mem::take(&mut children[at].children)
            }
            None => fragment.root.children,
        };

        // Detached, not destroyed — see `set_text_content`, which this is the
        // markup-shaped twin of. A freed id gets RECYCLED, and every map keyed
        // by node id then aliases a dead node onto a live one.
        for child in self.child_nodes(id) {
            self.remove_child(child);
        }

        // Set new children on WebCore
        if let Some(node) = self.find_webcore_mut(id) {
            node.children = new_children;
            node.text.clear();
        }

        // Rebuild arena for new children — split borrow: &mut self.arena + &mut self.root
        if let Some(node) = crate::dom::find_box_mut(&mut self.root, id) {
            for child in &mut node.children {
                crate::html::rebuild_arena_recursive_pub(&mut self.arena, child, NodeId(id));
            }
        }
    }

    // ── classList ──

    /// Add a class to the element's class list.
    pub fn class_list_add(&mut self, id: u32, class: &str) {
        if id == 0 || class.is_empty() { return; }
        let current = self.get_attribute(id, "class").unwrap_or_default();
        if current.split_whitespace().any(|c| c == class) { return; }
        let new_val = if current.is_empty() {
            class.to_string()
        } else {
            format!("{} {}", current, class)
        };
        self.set_attribute(id, "class", &new_val);
    }

    /// Remove a class from the element's class list.
    pub fn class_list_remove(&mut self, id: u32, class: &str) {
        if id == 0 || class.is_empty() { return; }
        let current = self.get_attribute(id, "class").unwrap_or_default();
        let new_val: Vec<&str> = current.split_whitespace().filter(|&c| c != class).collect();
        let joined = new_val.join(" ");
        if joined.is_empty() {
            self.remove_attribute(id, "class");
        } else {
            self.set_attribute(id, "class", &joined);
        }
    }

    /// Toggle a class. Returns true if the class is now present.
    pub fn class_list_toggle(&mut self, id: u32, class: &str) -> bool {
        if self.class_list_contains(id, class) {
            self.class_list_remove(id, class);
            false
        } else {
            self.class_list_add(id, class);
            true
        }
    }

    /// Check if an element has a class.
    pub fn class_list_contains(&self, id: u32, class: &str) -> bool {
        if id == 0 { return false; }
        self.get_attribute(id, "class").as_deref()
            .map(|c| c.split_whitespace().any(|cl| cl == class))
            .unwrap_or(false)
    }

    // ── Inline style ──

    /// Set a single CSS property in the element's inline style.
    pub fn set_style_property(&mut self, id: u32, prop: &str, value: &str) {
        if id == 0 { return; }
        let current = self.get_attribute(id, "style").unwrap_or_default();
        let mut props = parse_inline_style(&current);
        let prop_lower = prop.to_ascii_lowercase();
        // CSSOM §6.7.2: `setProperty(prop, "")` REMOVES the declaration. It
        // used to store the empty string, which serialized as a malformed
        // `position: ` — a declaration the parser then dropped, so the property
        // looked removed while `style` carried a syntax error that any
        // round-trip through `cssText` would preserve.
        if value.trim().is_empty() {
            props.retain(|(k, _)| k != &prop_lower);
        } else if let Some(entry) = props.iter_mut().find(|(k, _)| k == &prop_lower) {
            entry.1 = value.to_string();
        } else {
            props.push((prop_lower, value.to_string()));
        }
        let new_style = props.iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("; ");
        self.set_attribute(id, "style", &new_style);
    }

    /// Get a single CSS property from the element's inline style.
    pub fn get_style_property(&self, id: u32, prop: &str) -> Option<String> {
        let style_attr = self.get_attribute(id, "style")?;
        let prop_lower = prop.to_ascii_lowercase();
        parse_inline_style(&style_attr)
            .into_iter()
            .find(|(k, _)| k == &prop_lower)
            .map(|(_, v)| v)
    }

    /// Remove a CSS property from the element's inline style.
    pub fn remove_style_property(&mut self, id: u32, prop: &str) {
        if id == 0 { return; }
        let current = self.get_attribute(id, "style").unwrap_or_default();
        let prop_lower = prop.to_ascii_lowercase();
        let props: Vec<(String, String)> = parse_inline_style(&current)
            .into_iter()
            .filter(|(k, _)| k != &prop_lower)
            .collect();
        if props.is_empty() {
            self.remove_attribute(id, "style");
        } else {
            let new_style = props.iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("; ");
            self.set_attribute(id, "style", &new_style);
        }
    }

    // ── Layout queries ──

    /// Get the bounding rect of a node (border box in document coordinates).
    pub fn get_bounding_client_rect(&self, id: u32) -> Option<Rect> {
        // `get_node` walks the LIGHT tree only. A shadow node is a real box
        // with a real rect — it is just not reachable from `root.children` —
        // so the shadow-aware lookup is the fallback rather than a second
        // answer for the same node.
        self.get_node(id)
            .or_else(|| self.find_webcore(id))
            .map(|node| node.layout.border_rect)
    }

    /// Get the offset width (border box width).
    pub fn offset_width(&self, id: u32) -> f32 {
        self.get_bounding_client_rect(id).map(|r| r.w).unwrap_or(0.0)
    }

    /// Get the offset height (border box height).
    pub fn offset_height(&self, id: u32) -> f32 {
        self.get_bounding_client_rect(id).map(|r| r.h).unwrap_or(0.0)
    }

    // ── Internal helpers ──

    /// Find a shared reference to an WebCore by node_id.
    ///
    /// A tree walk on purpose. `get_box_by_id` has an O(1) fast path through
    /// `node_index`, which is a `HashMap<u32, *const WebCore>` rebuilt only by
    /// `rebuild_node_index()` — that is, only at layout. Any DOM mutation in
    /// between moves boxes inside their parent's `Vec<WebCore>` and leaves
    /// those pointers dangling, and the fast path would hand one back. The DOM
    /// API mutates without laying out, so it must not use that index.
    pub(crate) fn find_webcore(&self, id: u32) -> Option<&WebCore> {
        // `pending_nodes` FIRST. A node created but not yet inserted is not in
        // the tree, and the ordinary DOM idiom writes to it before it ever is:
        //
        //     el = createElement(); el.setAttribute(..); parent.appendChild(el)
        //
        // Searching only `root` made every such write vanish from the render
        // box while still reaching the arena — so `getAttribute` (arena) agreed
        // and `getElementById` (render tree) did not.
        if self.pending_nodes.contains_key(&id) {
            return self.pending_nodes.get(&id);
        }
        // **A DETACHED SUBTREE IS STILL A TREE.**
        //
        // `pending_nodes` holds detached ROOTS. The moment a node is appended
        // into one it stops being a root and lives in that box's `children`,
        // so a map lookup no longer finds it — and neither does a walk from
        // `root`, because none of it is in the document yet.
        //
        // That is the ordinary idiom one level deeper, and it is what
        // `innerHTML` on a detached element does for every tag after the
        // first: build the subtree, then insert it. Without this the parser
        // could attach a control's direct children and nothing below them —
        // a `<table>` kept its `<thead>` nowhere, and a `<button>`'s own TEXT
        // was dropped, so composed chrome came out as empty boxes.
        fn find_pending<'a>(nodes: &'a [WebCore], id: u32) -> Option<&'a WebCore> {
            for node in nodes {
                if node.node_id == id {
                    return Some(node);
                }
                if let Some(found) = find_pending(&node.children, id) {
                    return Some(found);
                }
            }
            None
        }
        for pending in self.pending_nodes.values() {
            if let Some(found) = find_pending(std::slice::from_ref(pending), id) {
                return Some(found);
            }
        }
        // A SHADOW TREE IS PART OF THE TREE.
        //
        // The walk followed `children` only, so no node inside a shadow root
        // was findable — and every API that resolves an id through here
        // (`get_attribute`, `tag_name`, `text_content`, the slot interface)
        // silently answered nothing for shadow content. It is the same class of
        // miss as the detached-subtree case above: a node that is genuinely in
        // the tree, reached by a link the walk did not follow.
        fn walk(node: &WebCore, id: u32) -> Option<&WebCore> {
            if node.node_id == id { return Some(node); }
            if let Some(sr) = &node.shadow_root {
                for child in &sr.children {
                    if let Some(found) = walk(child, id) { return Some(found); }
                }
            }
            for child in &node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
        }
        walk(&self.root, id)
    }

    /// Find a mutable reference to an WebCore by node_id.
    ///
    /// Checks `pending_nodes` first, for the reason spelled out on
    /// `find_webcore` — a detached node is still a legal target for
    /// `setAttribute`, `setTextContent` and a style write.
    pub(crate) fn find_webcore_mut(&mut self, id: u32) -> Option<&mut WebCore> {
        if self.pending_nodes.contains_key(&id) {
            return self.pending_nodes.get_mut(&id);
        }
        // Shadow trees too — see the note in `find_webcore`.
        fn walk(node: &mut WebCore, id: u32) -> Option<&mut WebCore> {
            if node.node_id == id { return Some(node); }
            if let Some(sr) = node.shadow_root.as_mut() {
                for child in &mut sr.children {
                    if let Some(found) = walk(child, id) { return Some(found); }
                }
            }
            for child in &mut node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
        }
        // The same detached-subtree search `find_webcore` explains, and the
        // half that actually loses nodes: `append_child` takes the child OUT of
        // `pending_nodes` before asking for its parent, so a parent this cannot
        // find means the child is already gone and is dropped in silence.
        //
        // Found in two passes because the owning root has to be identified
        // before the map can be borrowed mutably to walk into it.
        fn contains(node: &WebCore, id: u32) -> bool {
            node.node_id == id
                || node.shadow_root.as_ref()
                    .map(|sr| sr.children.iter().any(|c| contains(c, id)))
                    .unwrap_or(false)
                || node.children.iter().any(|child| contains(child, id))
        }
        let owner = self
            .pending_nodes
            .iter()
            .find(|(_, pending)| contains(pending, id))
            .map(|(key, _)| *key);
        if let Some(owner) = owner {
            return self
                .pending_nodes
                .get_mut(&owner)
                .and_then(|pending| walk(pending, id));
        }
        walk(&mut self.root, id)
    }

    /// Detach an WebCore from its parent in the tree, returning the detached box.
    fn detach_webcore(&mut self, id: u32) -> Option<WebCore> {
        fn walk(node: &mut WebCore, id: u32) -> Option<WebCore> {
            if let Some(idx) = node.children.iter().position(|c| c.node_id == id) {
                return Some(node.children.remove(idx));
            }
            for child in &mut node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
        }
        walk(&mut self.root, id)
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn parse_inline_style(style: &str) -> Vec<(String, String)> {
    style.split(';')
        .filter_map(|decl| {
            let decl = decl.trim();
            if decl.is_empty() { return None; }
            let colon = decl.find(':')?;
            let key = decl[..colon].trim().to_ascii_lowercase();
            let val = decl[colon + 1..].trim().to_string();
            Some((key, val))
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// The DOM methods that had no home yet.
//
// Everything here is also on `vybe_widgets::dom::Document` with the same name
// and the same shape, because the two are meant to be interchangeable: see
// `_both_browsers_are_whatwg` in `platforms/web`, which calls every one of them
// through the `Browser` alias and so fails to compile the moment they drift.
// ─────────────────────────────────────────────────────────────────────────────

/// `dataset.fooBar` → `data-foo-bar` (HTML §3.2.6.6).
fn dataset_attribute(key: &str) -> String {
    let mut name = String::from("data-");
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            name.push('-');
            name.push(ch.to_ascii_lowercase());
        } else {
            name.push(ch);
        }
    }
    name
}

/// `data-foo-bar` → `fooBar`, or `None` for an attribute that is not `data-*`.
fn dataset_key(attribute: &str) -> Option<String> {
    let rest = attribute.strip_prefix("data-")?;
    let mut key = String::new();
    let mut upper = false;
    for ch in rest.chars() {
        if ch == '-' {
            upper = true;
            continue;
        }
        if upper {
            key.extend(ch.to_uppercase());
            upper = false;
        } else {
            key.push(ch);
        }
    }
    Some(key)
}

/// Walk a rendered subtree, gathering what a reader would actually see.
fn inner_text_into(node: &WebCore, out: &mut String) {
    use crate::types::Display;
    if node.tag == "#text" {
        out.push_str(&node.text);
        return;
    }
    if node.tag == "#comment" {
        return;
    }
    // The whole difference from `textContent`: what the cascade hid is not
    // text the user can read, so it contributes nothing.
    if node.style.display == Display::None {
        return;
    }
    let block = node.style.display != Display::Inline;
    if block && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for child in &node.children {
        inner_text_into(child, out);
    }
    if block && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

impl Document {
    // ─── DocumentFragment (DOM §4.2.1, §4.5) ───────────────────────────────

    /// `document.createDocumentFragment()` — a detached parent to build in.
    ///
    /// Detached like every other `create*`, so it lands in `pending_nodes` and
    /// the ordinary `append_child` attaches what goes into it.
    pub fn create_document_fragment(&mut self) -> u32 {
        let arena_id = self.arena.create_document_fragment();
        let mut b = WebCore::new("#document-fragment");
        b.node_id = arena_id.0;
        // It never renders: it exists to be emptied into something that does,
        // and if one is ever left in a tree it must not draw.
        apply_property(&mut b.style, "display", "none");
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// Whether this node is a `DocumentFragment` — the question `append_child`
    /// and `insert_before` ask before deciding what inserting it means.
    pub fn is_document_fragment(&self, id: u32) -> bool {
        id != 0
            && self.arena.is_alive(NodeId(id))
            && self.arena.get(NodeId(id)).node_type
                == crate::dom::arena::NodeType::DocumentFragment
    }

    // ─── Node comparison and normalisation (DOM §4.4, §4.5) ─────────────────

    /// `node.normalize()` — drop empty text nodes and merge adjacent ones.
    pub fn normalize(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        let children = self.child_nodes(id);
        for &child in &children {
            self.normalize(child);
        }
        // The run being merged INTO. Reset by any non-text child, because two
        // text nodes with an element between them are not adjacent.
        let mut run: u32 = 0;
        for child in children {
            if !self.is_text_node(child) {
                run = 0;
                continue;
            }
            let data = self.text_data(child);
            if data.is_empty() {
                // Removed outright, and does NOT break the run — so `a`, ``,
                // `b` merges to `ab`.
                self.remove_child(child);
                continue;
            }
            if run == 0 {
                run = child;
                continue;
            }
            let merged = format!("{}{}", self.text_data(run), data);
            self.set_text_data(run, &merged);
            self.remove_child(child);
        }
    }

    /// `node.isEqualNode(other)` — structural equality, not identity.
    ///
    /// Attributes live in a map, so they compare as the SET the spec says they
    /// are, with no order to get wrong.
    pub fn is_equal_node(&self, a: u32, b: u32) -> bool {
        if a == b {
            // Identity settles it — for a NODE. Here 0 is the null id and a
            // freed id names nothing, and nothing is not equal to itself.
            return a != 0 && self.arena.is_alive(NodeId(a));
        }
        if a == 0 || b == 0 {
            return false;
        }
        if !self.arena.is_alive(NodeId(a)) || !self.arena.is_alive(NodeId(b)) {
            return false;
        }
        if self.node_type(a) != self.node_type(b) {
            return false;
        }
        let (Some(x), Some(y)) = (self.arena.try_get(NodeId(a)), self.arena.try_get(NodeId(b)))
        else { return false };
        if x.tag != y.tag || x.namespace != y.namespace {
            return false;
        }
        if self.is_element(a) {
            // DOM §4.5 compares attributes as a SET — "each attribute in A
            // has an attribute in B with the same namespace, local name and
            // value". `AttrMap` is an ordered list and its `==` is
            // order-sensitive, which is right for the list and wrong here, so
            // the set comparison is spelled out rather than borrowed.
            let same_attrs = x.attributes.len() == y.attributes.len()
                && x.attributes.iter().all(|(k, v)| y.attributes.get(k) == Some(v));
            if !same_attrs || x.attribute_ns != y.attribute_ns {
                return false;
            }
        } else if x.text != y.text {
            return false;
        }
        let ca = self.child_nodes(a);
        let cb = self.child_nodes(b);
        ca.len() == cb.len()
            && ca
                .iter()
                .zip(cb.iter())
                .all(|(&p, &q)| self.is_equal_node(p, q))
    }

    /// The root-to-node path, used to put two nodes in tree order.
    fn ancestor_path(&self, id: u32) -> Vec<u32> {
        let mut path = vec![id];
        let mut current = id;
        loop {
            let parent = self.parent_node(current);
            if parent == 0 {
                break;
            }
            path.push(parent);
            current = parent;
        }
        path.reverse();
        path
    }

    /// `node.compareDocumentPosition(other)` — DOM §4.4's bitmask.
    ///
    /// `DISCONNECTED 0x01`, `PRECEDING 0x02`, `FOLLOWING 0x04`,
    /// `CONTAINS 0x08`, `CONTAINED_BY 0x10`, `IMPLEMENTATION_SPECIFIC 0x20`.
    /// Containment always arrives paired with a direction, which is why the
    /// answers below are two bits and not one.
    pub fn compare_document_position(&self, a: u32, b: u32) -> u16 {
        const DISCONNECTED: u16 = 0x01;
        const PRECEDING: u16 = 0x02;
        const FOLLOWING: u16 = 0x04;
        const CONTAINS: u16 = 0x08;
        const CONTAINED_BY: u16 = 0x10;
        const IMPLEMENTATION_SPECIFIC: u16 = 0x20;

        if a == b {
            return 0;
        }
        let pa = self.ancestor_path(a);
        let pb = self.ancestor_path(b);
        if pa.first() != pb.first() {
            // The spec lets an implementation pick the direction for two
            // detached trees, but requires it to be CONSISTENT. Ordering by
            // the node handle is the cheapest thing that is.
            let direction = if a < b { FOLLOWING } else { PRECEDING };
            return DISCONNECTED | IMPLEMENTATION_SPECIFIC | direction;
        }
        let split = pa
            .iter()
            .zip(pb.iter())
            .position(|(x, y)| x != y)
            .unwrap_or_else(|| pa.len().min(pb.len()));
        if split == pa.len() {
            return CONTAINED_BY | FOLLOWING;
        }
        if split == pb.len() {
            return CONTAINS | PRECEDING;
        }
        let siblings = self.child_nodes(pa[split - 1]);
        let ia = siblings.iter().position(|n| *n == pa[split]);
        let ib = siblings.iter().position(|n| *n == pb[split]);
        match (ia, ib) {
            (Some(ia), Some(ib)) if ia < ib => FOLLOWING,
            (Some(_), Some(_)) => PRECEDING,
            _ => DISCONNECTED | IMPLEMENTATION_SPECIFIC | FOLLOWING,
        }
    }

    // ─── The rest of ParentNode / ChildNode (DOM §4.2.6) ────────────────────

    /// Insert before `reference`, or append when there is no reference.
    ///
    /// `insert_before` rejects a reference of 0 rather than treating it as
    /// "at the end", so every caller below has to say which it means.
    fn insert_or_append(&mut self, parent: u32, child: u32, reference: u32) {
        if reference == 0 {
            self.append_child(parent, child);
        } else {
            self.insert_before(parent, child, reference);
        }
    }

    /// `parent.append(...nodes)`.
    pub fn append(&mut self, parent: u32, nodes: &[u32]) {
        for &node in nodes {
            self.append_child(parent, node);
        }
    }

    /// `parent.prepend(...nodes)`.
    ///
    /// Every node goes before the ORIGINAL first child, not before whatever is
    /// first at the time — inserting each before the running first child would
    /// hand the caller its nodes back reversed.
    pub fn prepend(&mut self, parent: u32, nodes: &[u32]) {
        let reference = self.first_child(parent).unwrap_or(0);
        for &node in nodes {
            self.insert_or_append(parent, node, reference);
        }
    }

    /// `node.before(...nodes)`.
    pub fn before(&mut self, id: u32, nodes: &[u32]) {
        let parent = self.parent_node(id);
        if parent == 0 {
            return;
        }
        for &new in nodes {
            self.insert_before(parent, new, id);
        }
    }

    /// `node.after(...nodes)`.
    pub fn after(&mut self, id: u32, nodes: &[u32]) {
        let parent = self.parent_node(id);
        if parent == 0 {
            return;
        }
        let reference = self.next_sibling(id);
        for &new in nodes {
            self.insert_or_append(parent, new, reference);
        }
    }

    /// `node.replaceWith(...nodes)`.
    pub fn replace_with(&mut self, id: u32, nodes: &[u32]) {
        let parent = self.parent_node(id);
        if parent == 0 {
            return;
        }
        for &new in nodes {
            self.insert_before(parent, new, id);
        }
        self.remove_child(id);
    }

    // ─── Serialisation and adjacent insertion (DOM Parsing §3) ─────────────

    /// `element.outerHTML` (getter) — the serialization of the node ITSELF,
    /// where [`inner_html`](Self::inner_html) writes only its children.
    pub fn outer_html(&self, id: u32) -> String {
        if id == 0 {
            return String::new();
        }
        let node = match self
            .find_webcore(id)
            .or_else(|| self.pending_nodes.get(&id))
        {
            Some(n) => n,
            None => return String::new(),
        };
        let mut out = String::new();
        crate::html::serializer::serialize_box(node, &mut out);
        out
    }

    /// `element.insertAdjacentElement(position, element)`.
    ///
    /// Returns the inserted element, or `None` for an unknown position or a
    /// placement with no parent to hold it — the IDL's `null`.
    pub fn insert_adjacent_element(&mut self, id: u32, position: &str, other: u32) -> Option<u32> {
        match position.to_ascii_lowercase().as_str() {
            "beforebegin" => {
                let parent = self.parent_node(id);
                if parent == 0 {
                    return None;
                }
                self.insert_before(parent, other, id);
            }
            "afterbegin" => {
                let reference = self.first_child(id).unwrap_or(0);
                self.insert_or_append(id, other, reference);
            }
            "beforeend" => self.append_child(id, other),
            "afterend" => {
                let parent = self.parent_node(id);
                if parent == 0 {
                    return None;
                }
                let reference = self.next_sibling(id);
                self.insert_or_append(parent, other, reference);
            }
            _ => return None,
        }
        Some(other)
    }

    /// `element.insertAdjacentText(position, data)`.
    pub fn insert_adjacent_text(&mut self, id: u32, position: &str, data: &str) {
        let text = self.create_text_node(data);
        self.insert_adjacent_element(id, position, text);
    }

    // ─── HTMLElement: dataset, innerText, tabIndex, click (HTML §3.2.6) ─────

    /// `element.dataset` — every `data-*` attribute, keyed the way the IDL
    /// names it rather than the way it is spelled in the markup.
    ///
    /// A snapshot, not the live `DOMStringMap`: Rust has nowhere to put an
    /// object whose property writes reach back into the element, so the pair
    /// [`set_dataset`](Self::set_dataset) / [`remove_dataset`](Self::remove_dataset)
    /// carries the write side.
    pub fn dataset(&self, id: u32) -> Vec<(String, String)> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return Vec::new();
        }
        let mut out: Vec<(String, String)> = self
            .arena
            .get(NodeId(id))
            .attributes
            .iter()
            .filter_map(|(name, value)| dataset_key(name).map(|k| (k, value.clone())))
            .collect();
        // The attributes live in a map, which has no order of its own. Sorting
        // means two reads of one element answer in the same order.
        out.sort();
        out
    }

    /// `element.dataset[key]`.
    pub fn dataset_get(&self, id: u32, key: &str) -> Option<String> {
        self.get_attribute(id, &dataset_attribute(key))
    }

    /// `element.dataset[key] = value`.
    pub fn set_dataset(&mut self, id: u32, key: &str, value: &str) {
        let name = dataset_attribute(key);
        self.set_attribute(id, &name, value);
    }

    /// `delete element.dataset[key]`.
    pub fn remove_dataset(&mut self, id: u32, key: &str) {
        let name = dataset_attribute(key);
        self.remove_attribute(id, &name);
    }

    /// `element.innerText` — the RENDERED text.
    ///
    /// The difference from `textContent` is the cascade: a subtree the styles
    /// hid contributes nothing, and a block boundary becomes a line break.
    /// Reading `textContent` where a page asked for `innerText` is how hidden
    /// markup leaks into a string a user is shown.
    pub fn inner_text(&self, id: u32) -> String {
        let mut out = String::new();
        if let Some(node) = self
            .find_webcore(id)
            .or_else(|| self.pending_nodes.get(&id))
        {
            inner_text_into(node, &mut out);
        }
        out.trim_matches('\n').to_string()
    }

    /// `element.tabIndex`.
    ///
    /// The default is not zero for everything: HTML §6.6.3 gives 0 to what is
    /// focusable without the attribute and −1 to the rest, so a `<div>` and a
    /// `<button>` answer differently with no `tabindex` on either.
    pub fn tab_index(&self, id: u32) -> i32 {
        if let Some(value) = self.get_attribute(id, "tabindex") {
            if let Ok(parsed) = value.trim().parse::<i32>() {
                return parsed;
            }
        }
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return -1;
        }
        match self.arena.get(NodeId(id)).tag.as_str() {
            // A link is only in the sequence once it has somewhere to go.
            "a" | "area" => {
                if self.get_attribute(id, "href").is_some() {
                    0
                } else {
                    -1
                }
            }
            "button" | "input" | "select" | "textarea" | "iframe" => 0,
            _ => -1,
        }
    }

    /// `element.tabIndex = index`.
    pub fn set_tab_index(&mut self, id: u32, index: i32) {
        self.set_attribute(id, "tabindex", &index.to_string());
    }

    /// `element.click()` — a synthetic click, dispatched like a real one.
    ///
    /// Both registries are notified, exactly as a real click does it: the
    /// `HtmlEvent` listeners bound to boxes, and the NodeId-keyed
    /// `EventTargetMap` that carries capture and bubbling. Notifying only one
    /// would make a synthetic click reach a different set of handlers from a
    /// user's, which is the one thing `click()` must not do.
    pub fn click(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        let mut dom_event = crate::dom::events::DomEvent::new("click", id);
        self.dispatch_dom_event(&mut dom_event);
    }

    // ─── CSSOM-View (§5) ───────────────────────────────────────────────────

    /// `element.getClientRects()`.
    ///
    /// One rect per box the element generates. A wrapped inline should report
    /// one per line; until the line boxes are addressable from here it reports
    /// the single border box, which is the correct answer for every element
    /// that generates one box and an under-count for the rest.
    pub fn get_client_rects(&self, id: u32) -> Vec<Rect> {
        self.get_bounding_client_rect(id).into_iter().collect()
    }

    /// `element.scrollIntoView()` — bring the element to the top of the view.
    ///
    /// The default alignment is `start`, which is what this does. The rect is
    /// viewport-relative, so its `y` IS the distance to scroll.
    pub fn scroll_into_view(&mut self, id: u32) {
        let Some(rect) = self.get_bounding_client_rect(id) else {
            return;
        };
        self.scroll_y = (self.scroll_y + rect.y).max(0.0);
    }

    /// `element.offsetParent` — the nearest POSITIONED ancestor, else the body.
    pub fn offset_parent(&mut self, id: u32) -> Option<u32> {
        let mut current = self.parent_node(id);
        while current != 0 {
            let tag = self.tag_name(current).unwrap_or("").to_string();
            if tag == "body" || tag == "html" {
                return self.body();
            }
            if self.computed_style_property(current, "position") != "static" {
                return Some(current);
            }
            current = self.parent_node(current);
        }
        None
    }

    /// `element.offsetTop` — the border box's top edge, relative to
    /// [`offset_parent`](Self::offset_parent) rather than to the viewport.
    pub fn offset_top(&mut self, id: u32) -> f32 {
        let own = self.get_bounding_client_rect(id).map(|r| r.y).unwrap_or(0.0);
        let origin = self
            .offset_parent(id)
            .and_then(|p| self.get_bounding_client_rect(p))
            .map(|r| r.y)
            .unwrap_or(0.0);
        own - origin
    }

    /// `element.offsetLeft` — see [`offset_top`](Self::offset_top).
    pub fn offset_left(&mut self, id: u32) -> f32 {
        let own = self.get_bounding_client_rect(id).map(|r| r.x).unwrap_or(0.0);
        let origin = self
            .offset_parent(id)
            .and_then(|p| self.get_bounding_client_rect(p))
            .map(|r| r.x)
            .unwrap_or(0.0);
        own - origin
    }

    // ─── The namespaced accessors that were missing their pair ─────────────

    /// `element.hasAttributeNS(namespace, localName)`.
    pub fn has_attribute_ns(&self, id: u32, namespace: &str, local_name: &str) -> bool {
        self.get_attribute_ns(id, namespace, local_name).is_some()
    }

    /// `element.removeAttributeNS(namespace, localName)`.
    ///
    /// Matched the same way `getAttributeNS` matches — by namespace and LOCAL
    /// name — so `xlink:href` goes and `href` stays.
    pub fn remove_attribute_ns(&mut self, id: u32, namespace: &str, local_name: &str) {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return;
        }
        let want = (!namespace.is_empty()).then_some(namespace);
        let doomed: Vec<String> = self
            .arena
            .get(NodeId(id))
            .attributes
            .keys()
            .filter(|name| {
                let local = match name.split_once(':') {
                    Some((_, local)) => local,
                    None => name.as_str(),
                };
                if local != local_name {
                    return false;
                }
                let have = self.arena.try_get(NodeId(id))
                    .and_then(|n| n.attribute_ns.get(*name));
                have.map(String::as_str) == want
            })
            .cloned()
            .collect();
        for name in doomed {
            self.remove_attribute(id, &name);
        }
    }
}

// ─── Canvas ─────────────────────────────────────────────────────────────────
//
// `canvas.getContext("2d")` — HTML §4.12.5.
//
// A page reaches a canvas the way it reaches anything else: it looks the
// element up, asks it for a context, and draws. Nothing above this layer names
// an engine, which is the same contract the rest of this file keeps — the
// identical surface exists on `vybe_widgets`, so which one is compiled in stays
// a build-time choice.

impl Document {
    /// `canvas.getContext("2d")` — HTML §4.12.5.1.
    ///
    /// Answers whether `id` is a `<canvas>` that now has a 2D context,
    /// allocating its bitmap if it does not have one yet. An element built by
    /// `createElement("canvas")` has never been through the parser, so this is
    /// where it gets the transparent-black bitmap the spec says a canvas
    /// starts with.
    ///
    /// There is no context OBJECT to return here on purpose. The context's
    /// identity is the element — every call arrives naming the node — so a
    /// handle would be a second name for something that already has one.
    pub fn get_context_2d(&mut self, id: u32) -> bool {
        self.ensure_canvas_bitmap(id)
    }

    /// Give `id` the bitmap a `<canvas>` element is defined to have, and say
    /// whether it is a canvas at all.
    ///
    /// §4.12.5 gives the ELEMENT the bitmap, not the context — a `<canvas>`
    /// has one from the moment it exists, and `getContext` hands out a way to
    /// draw on what is already there. So this is what `getContext` does and
    /// also what drawing does, rather than two paths that could disagree about
    /// whether a surface exists. The parser allocates the same buffer for a
    /// parsed `<canvas>`; an element from `createElement("canvas")` has never
    /// been through it, and gets its bitmap here.
    fn ensure_canvas_bitmap(&mut self, id: u32) -> bool {
        let Some(node) = self.find_webcore_mut(id) else { return false };
        if node.tag != "canvas" {
            return false;
        }
        // §4.12.5: a canvas with no `width`/`height` attribute is 300 × 150.
        if node.image_width == 0 || node.image_height == 0 {
            node.image_width = 300;
            node.image_height = 150;
        }
        let want = (node.image_width as usize) * (node.image_height as usize) * 4;
        match node.image_data {
            Some(ref data) if data.len() == want => {}
            // Transparent black, which is what the spec initialises the
            // bitmap to — and what a zeroed RGBA buffer already is.
            _ => node.image_data = Some(vec![0u8; want]),
        }
        true
    }

    /// Draw on the canvas `id` through the WHATWG 2D context.
    ///
    /// The context state persists across calls; see `canvas::CanvasSurfaces`.
    /// `None` when `id` is not a `<canvas>` — which is the only thing that can
    /// fail here, because the element owns its bitmap and
    /// [`ensure_canvas_bitmap`](Self::ensure_canvas_bitmap) is the same
    /// allocation `getContext` performs.
    pub fn with_canvas_2d<R>(
        &mut self,
        id: u32,
        f: impl FnOnce(&mut dyn crate::canvas::Canvas) -> R,
    ) -> Option<R> {
        if !self.ensure_canvas_bitmap(id) {
            return None;
        }
        // The bitmap is MOVED out of the element and back, so the element and
        // the surface store are never borrowed at the same time — and a canvas
        // is never copied to be drawn on.
        let (mut pixels, w, h) = {
            let node = self.find_webcore_mut(id)?;
            (
                node.image_data.take()?,
                node.image_width,
                node.image_height,
            )
        };
        let out = self.canvas_surfaces.with_context(id, &mut pixels, w, h, f);
        if let Some(node) = self.find_webcore_mut(id) {
            node.image_data = Some(pixels);
        }
        out
    }

    /// `canvas.width` / `canvas.height` — HTML §4.12.5.
    ///
    /// Assigning either one **reinitialises the bitmap to transparent black
    /// and resets the drawing state**, and the spec is explicit that this
    /// happens even when the value assigned is the one already there. So this
    /// is not a resize that preserves content: `canvas.width = canvas.width`
    /// is the documented way a page clears a canvas, and an implementation
    /// that kept the pixels would break it silently.
    pub fn set_canvas_size(&mut self, id: u32, width: u32, height: u32) {
        let Some(node) = self.find_webcore_mut(id) else { return };
        if node.tag != "canvas" {
            return;
        }
        node.image_width = width;
        node.image_height = height;
        node.image_data = Some(vec![0u8; (width as usize) * (height as usize) * 4]);
        node.attributes.insert("width", width.to_string());
        node.attributes.insert("height", height.to_string());
        self.canvas_surfaces.reset(id);
    }
}

// ─── NamedNodeMap and Attr — DOM §4.9.1 / §4.9.2 ────────────────────────────
//
// `element.attributes` is a live `NamedNodeMap` in a browser. It is a snapshot
// here, for the same reason `getElementsByTagName` is: liveness cannot survive
// a `Vec` return, and the caller is a Rust caller with no object identity to
// keep alive. Everything else about the interface — index order, the
// case-folding of a qualified name, the `NotFoundError` on removing an
// attribute that is not there, `ownerElement` going null on removal — behaves
// as the spec says and as Chrome demonstrates.
impl Document {
    /// `element.attributes` — the attribute list, in order.
    pub fn attributes(&self, id: u32) -> Vec<Attr> {
        let ns = self.attribute_ns_map(id);
        self.attribute_entries(id)
            .into_iter()
            .map(|(name, value)| Attr {
                owner_element: id,
                namespace: ns.get(&name).cloned(),
                name,
                value,
            })
            .collect()
    }

    /// `element.attributes.length`.
    pub fn attributes_length(&self, id: u32) -> usize {
        self.attribute_entries(id).len()
    }

    /// `element.attributes.item(index)` — positional, which is the whole
    /// reason the attribute list is a list.
    pub fn attributes_item(&self, id: u32, index: usize) -> Option<Attr> {
        self.attributes(id).into_iter().nth(index)
    }

    /// `element.attributes.getNamedItem(qualifiedName)`. The name is folded
    /// first, so `getNamedItem("TITLE")` finds `title` on an HTML element —
    /// verified against Chrome.
    pub fn get_named_item(&self, id: u32, qualified_name: &str) -> Option<Attr> {
        let folded = self.fold_name(qualified_name);
        self.attributes(id).into_iter().find(|a| a.name == folded)
    }

    /// `element.attributes.getNamedItemNS(namespace, localName)`.
    ///
    /// A `None` namespace means the NULL namespace, and matches only
    /// attributes that have none — it is not a wildcard.
    pub fn get_named_item_ns(
        &self,
        id: u32,
        namespace: Option<&str>,
        local_name: &str,
    ) -> Option<Attr> {
        self.attributes(id)
            .into_iter()
            .find(|a| a.local_name() == local_name && a.namespace.as_deref() == namespace)
    }

    /// `element.attributes.setNamedItem(attr)` — returns the attribute it
    /// replaced, if any.
    pub fn set_named_item(&mut self, id: u32, attr: Attr) -> Option<Attr> {
        let previous = self.get_named_item(id, &attr.name);
        match attr.namespace.clone() {
            Some(ns) => self.set_attribute_ns(id, &ns, &attr.name, &attr.value),
            None => self.set_attribute(id, &attr.name, &attr.value),
        }
        previous
    }

    /// `element.attributes.setNamedItemNS(attr)`. Same operation as
    /// `setNamedItem` — the spec defines both to run "set an attribute", and
    /// the namespace comes off the attribute either way.
    pub fn set_named_item_ns(&mut self, id: u32, attr: Attr) -> Option<Attr> {
        self.set_named_item(id, attr)
    }

    /// `element.attributes.removeNamedItem(qualifiedName)`.
    ///
    /// `None` is the `NotFoundError` the spec throws — this crate has no
    /// exception channel, so an absent attribute is reported by the absent
    /// return rather than by removing nothing and claiming success.
    pub fn remove_named_item(&mut self, id: u32, qualified_name: &str) -> Option<Attr> {
        let mut attr = self.get_named_item(id, qualified_name)?;
        self.remove_attribute(id, qualified_name);
        attr.owner_element = 0;
        Some(attr)
    }

    /// `element.attributes.removeNamedItemNS(namespace, localName)`.
    pub fn remove_named_item_ns(
        &mut self,
        id: u32,
        namespace: Option<&str>,
        local_name: &str,
    ) -> Option<Attr> {
        let mut attr = self.get_named_item_ns(id, namespace, local_name)?;
        match namespace {
            Some(ns) => self.remove_attribute_ns(id, ns, local_name),
            None => self.remove_attribute(id, &attr.name),
        }
        attr.owner_element = 0;
        Some(attr)
    }

    /// `element.getAttributeNode(qualifiedName)`.
    pub fn get_attribute_node(&self, id: u32, qualified_name: &str) -> Option<Attr> {
        self.get_named_item(id, qualified_name)
    }

    /// `element.getAttributeNodeNS(namespace, localName)`.
    pub fn get_attribute_node_ns(
        &self,
        id: u32,
        namespace: Option<&str>,
        local_name: &str,
    ) -> Option<Attr> {
        self.get_named_item_ns(id, namespace, local_name)
    }

    /// `element.setAttributeNode(attr)`.
    pub fn set_attribute_node(&mut self, id: u32, attr: Attr) -> Option<Attr> {
        self.set_named_item(id, attr)
    }

    /// `element.setAttributeNodeNS(attr)`.
    pub fn set_attribute_node_ns(&mut self, id: u32, attr: Attr) -> Option<Attr> {
        self.set_named_item(id, attr)
    }

    /// `element.removeAttributeNode(attr)` — removes the attribute this one
    /// names and hands it back detached.
    pub fn remove_attribute_node(&mut self, id: u32, attr: &Attr) -> Option<Attr> {
        match attr.namespace.as_deref() {
            Some(ns) => self.remove_named_item_ns(id, Some(ns), attr.local_name()),
            None => self.remove_named_item(id, &attr.name),
        }
    }

    /// `document.createAttribute(localName)` — an attribute with no owner,
    /// ready to be handed to `setAttributeNode`. The name is folded, so
    /// `createAttribute("Foo")` is `foo` in an HTML document, which is what
    /// Chrome does.
    pub fn create_attribute(&self, local_name: &str) -> Attr {
        Attr::new(self.fold_name(local_name), "")
    }

    /// `document.createAttributeNS(namespace, qualifiedName)`. The qualified
    /// name keeps its case: only HTML folds, and a namespaced attribute is by
    /// definition not an HTML one.
    pub fn create_attribute_ns(&self, namespace: Option<&str>, qualified_name: &str) -> Attr {
        Attr {
            owner_element: 0,
            namespace: namespace.map(|s| s.to_string()),
            name: qualified_name.to_string(),
            value: String::new(),
        }
    }

    /// The attribute list of an element, wherever it is stored. Shadow-tree
    /// nodes are not mirrored into the arena, so the render tree answers for
    /// them — the same fallback every other attribute read here uses.
    fn attribute_entries(&self, id: u32) -> Vec<(String, String)> {
        if id != 0 && self.arena.is_alive(NodeId(id)) {
            let node = self.arena.get(NodeId(id));
            if node.node_type == crate::dom::arena::NodeType::Element {
                return node.attributes.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            }
            return Vec::new();
        }
        self.find_webcore(id)
            .map(|n| n.attributes.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    /// Qualified name → namespace URI, for the attributes that have one.
    fn attribute_ns_map(&self, id: u32) -> std::collections::HashMap<String, String> {
        if id != 0 && self.arena.is_alive(NodeId(id)) {
            return self.arena.get(NodeId(id)).attribute_ns.clone();
        }
        std::collections::HashMap::new()
    }
}

// ─── The token-list attributes — DOM §7.1, HTML §2.6.7 ──────────────────────
//
// Each of these is `DOMTokenList` over one attribute. They are named
// accessors rather than one `token_list(id, "class")` because the IDL names
// them, and because the supported-token set is per attribute: `relList`
// answers `supports`, `classList` throws for it.
impl Document {
    /// `element.classList`.
    pub fn class_list(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "class" }
    }
    pub fn class_list_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "class" }
    }

    /// `relList` on `<a>`, `<area>`, `<link>` and `<form>`.
    pub fn rel_list(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "rel" }
    }
    pub fn rel_list_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "rel" }
    }

    /// `iframe.sandbox`.
    pub fn sandbox(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "sandbox" }
    }
    pub fn sandbox_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "sandbox" }
    }

    /// `element.part` — the shadow parts this element exposes to its host.
    pub fn part(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "part" }
    }
    pub fn part_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "part" }
    }

    /// `output.htmlFor` — a token list of the ids this output was computed
    /// from. NOT the same member as `label.htmlFor`, which is a single id
    /// string; see [`html_for`](Self::html_for).
    pub fn html_for_list(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "for" }
    }
    pub fn html_for_list_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "for" }
    }

    /// `label.htmlFor` / `script.htmlFor` — a single id, not a list.
    pub fn html_for(&self, id: u32) -> String {
        self.get_attribute(id, "for").unwrap_or_default()
    }
    pub fn set_html_for(&mut self, id: u32, value: &str) {
        self.set_attribute(id, "for", value);
    }
}

// ─── Form association — HTML §4.10.18.3 ─────────────────────────────────────
impl Document {
    /// `element.form` — the control's form owner.
    ///
    /// The `form` content attribute wins when it names a form; only when there
    /// is none does the nearest ancestor `<form>` apply. That order is the
    /// whole point of the attribute: it exists so a control can be associated
    /// with a form it is NOT inside.
    pub fn form_owner(&self, id: u32) -> Option<u32> {
        if !FORM_ASSOCIATED.contains(&self.tag_name(id)?) { return None; }
        if let Some(form_id) = self.get_attribute(id, "form") {
            let named = self.get_element_by_id(&form_id)?;
            return (self.tag_name(named) == Some("form")).then_some(named);
        }
        let mut cursor = self.parent_element(id);
        while let Some(node) = cursor {
            if self.tag_name(node) == Some("form") { return Some(node); }
            cursor = self.parent_element(node);
        }
        None
    }

    /// `form.elements` — the form's listed elements, in tree order.
    ///
    /// `<image>` inputs are excluded, as the spec says, and a control that
    /// points at this form with a `form` attribute is included even though it
    /// sits somewhere else in the tree.
    pub fn form_elements(&self, form: u32) -> Vec<u32> {
        let mut out = Vec::new();
        self.walk_tree(self.root.node_id, &mut |doc, node| {
            let Some(tag) = doc.tag_name(node) else { return };
            if !LISTED_ELEMENTS.contains(&tag) { return; }
            if tag == "input" && doc.input_type(node) == "image" { return; }
            if doc.form_owner(node) == Some(form) { out.push(node); }
        });
        out
    }

    /// `element.labels` — every `<label>` that labels this element, in tree
    /// order: the ones whose `for` names it, and any ancestor `<label>`.
    pub fn labels(&self, id: u32) -> Vec<u32> {
        let Some(tag) = self.tag_name(id) else { return Vec::new() };
        if !LABELABLE.contains(&tag) { return Vec::new(); }
        if tag == "input" && self.input_type(id) == "hidden" { return Vec::new(); }
        let own_id = self.get_attribute(id, "id").unwrap_or_default();
        let mut out = Vec::new();
        self.walk_tree(self.root.node_id, &mut |doc, node| {
            if doc.tag_name(node) != Some("label") { return; }
            let explicit = !own_id.is_empty()
                && doc.get_attribute(node, "for").as_deref() == Some(own_id.as_str());
            // An ancestor label labels its FIRST labelable descendant only.
            let implicit = doc.get_attribute(node, "for").is_none()
                && doc.first_labelable_descendant(node) == Some(id);
            if explicit || implicit { out.push(node); }
        });
        out
    }

    fn first_labelable_descendant(&self, label: u32) -> Option<u32> {
        let mut found = None;
        self.walk_tree(label, &mut |doc, node| {
            if found.is_some() || node == label { return; }
            if let Some(tag) = doc.tag_name(node) {
                if LABELABLE.contains(&tag) { found = Some(node); }
            }
        });
        found
    }

    /// `input.type` — the reflected keyword, lowercased, defaulting to `text`
    /// for an absent or unrecognised value, exactly as the IDL getter does.
    pub fn input_type(&self, id: u32) -> String {
        let raw = self.get_attribute(id, "type").unwrap_or_default().to_ascii_lowercase();
        if INPUT_TYPES.contains(&raw.as_str()) { raw } else { "text".to_string() }
    }

    /// `button.type` / `input.type` for a `<button>`, defaulting to `submit`.
    pub fn button_type(&self, id: u32) -> String {
        let raw = self.get_attribute(id, "type").unwrap_or_default().to_ascii_lowercase();
        match raw.as_str() {
            "button" | "reset" | "submit" => raw,
            _ => "submit".to_string(),
        }
    }

    /// Depth-first walk in tree order, shadow trees included.
    fn walk_tree(&self, root: u32, f: &mut dyn FnMut(&Document, u32)) {
        f(self, root);
        for child in self.child_nodes(root) {
            self.walk_tree(child, f);
        }
    }
}

/// The elements that can have a form owner (HTML §4.10.18.3).
const FORM_ASSOCIATED: &[&str] = &[
    "button", "fieldset", "input", "object", "output", "select", "textarea", "img",
];

/// `form.elements` lists these (HTML §4.10.3).
const LISTED_ELEMENTS: &[&str] = &[
    "button", "fieldset", "input", "object", "output", "select", "textarea",
];

/// The labelable elements (HTML §4.10.4).
const LABELABLE: &[&str] = &[
    "button", "input", "meter", "output", "progress", "select", "textarea",
];

const INPUT_TYPES: &[&str] = &[
    "hidden", "text", "search", "tel", "url", "email", "password", "date", "month",
    "week", "time", "datetime-local", "number", "range", "color", "checkbox",
    "radio", "file", "submit", "image", "reset", "button",
];

// ─── Constraint validation — HTML §4.10.19 ──────────────────────────────────
impl Document {
    /// `element.willValidate` — whether the element is a candidate for
    /// constraint validation at all.
    ///
    /// This is the member that decides everything else: a barred element is
    /// always `valid`, always has an empty `validationMessage`, and always
    /// passes `checkValidity()`, no matter what its attributes say. Chrome on
    /// `<input required readonly value="">` answers `willValidate=false` and
    /// `valid=true` — the `required` is simply not in force.
    pub fn will_validate(&self, id: u32) -> bool {
        let Some(tag) = self.tag_name(id) else { return false };
        // Listed but never candidates.
        if matches!(tag, "output" | "fieldset" | "object") { return false; }
        if !matches!(tag, "input" | "select" | "textarea" | "button") { return false; }

        if self.is_actually_disabled(id) { return false; }
        // A control inside a `<datalist>` is a suggestion, not an entry.
        let mut cursor = self.parent_element(id);
        while let Some(node) = cursor {
            if self.tag_name(node) == Some("datalist") { return false; }
            cursor = self.parent_element(node);
        }
        match tag {
            "input" => {
                if crate::html::validity::input_type_is_barred(&self.input_type(id)) {
                    return false;
                }
                // `readonly` bars only the types it applies to; a checkbox is
                // not made read-only by the attribute.
                !(self.has_attribute(id, "readonly") && self.readonly_applies(id))
            }
            "textarea" => !self.has_attribute(id, "readonly"),
            "button" => self.button_type(id) == "submit",
            _ => true,
        }
    }

    /// `element.validity`.
    pub fn validity(&self, id: u32) -> ValidityState {
        let mut v = ValidityState::default();
        if !self.will_validate(id) { return v; }
        let Some(tag) = self.tag_name(id) else { return v };

        v.custom_error = self.custom_validity.get(&id).is_some_and(|m| !m.is_empty());
        if tag == "button" { return v; }

        let value = self.value(id);
        let input_type = if tag == "input" { self.input_type(id) } else { String::new() };

        // ── valueMissing ──
        if self.has_attribute(id, "required") {
            v.value_missing = match tag {
                "select" => self.select_has_no_placeholder_selection(id),
                "textarea" => value.is_empty(),
                _ => match input_type.as_str() {
                    "checkbox" => !self.checked(id),
                    // A radio group is satisfied by ANY checked member.
                    "radio" => !self.radio_group_has_a_checked_member(id),
                    "file" => value.is_empty(),
                    _ => value.is_empty(),
                },
            };
        }

        // Every constraint below is on the VALUE, and an empty value has
        // nothing to violate — that is why `required` is the only one that
        // fires on an empty field.
        if value.is_empty() { return v; }

        // ── typeMismatch ──
        v.type_mismatch = match input_type.as_str() {
            "email" => {
                if self.has_attribute(id, "multiple") {
                    value.split(',').any(|a| !crate::html::validity::is_valid_email(a.trim()))
                } else {
                    !crate::html::validity::is_valid_email(&value)
                }
            }
            "url" => !crate::html::validity::is_valid_url(value.trim()),
            _ => false,
        };

        // ── patternMismatch ──
        if let Some(pattern) = self.get_attribute(id, "pattern") {
            if PATTERN_TYPES.contains(&input_type.as_str()) {
                // An unparseable pattern is not a violation: HTML says a
                // pattern that fails to compile is ignored.
                v.pattern_mismatch =
                    crate::html::validity::pattern_matches(&pattern, &value) == Some(false);
            }
        }

        // ── tooLong / tooShort ──
        //
        // ⛔ Both apply only to a value the USER edited. Chrome on
        // `<input maxlength=3 value="abcdef">` answers VALID — the dirty value
        // flag is part of the constraint, not an implementation shortcut.
        let length_constrained =
            tag == "textarea" || LENGTH_TYPES.contains(&input_type.as_str());
        if length_constrained && self.value_is_dirty(id) {
            let len = value.encode_utf16().count() as i64;
            if let Some(max) = self.numeric_attribute(id, "maxlength") {
                v.too_long = len > max as i64;
            }
            if let Some(min) = self.numeric_attribute(id, "minlength") {
                v.too_short = len < min as i64;
            }
        }

        // ── rangeUnderflow / rangeOverflow / stepMismatch / badInput ──
        if NUMERIC_TYPES.contains(&input_type.as_str()) {
            match crate::html::forms::parse_floating_point(&value) {
                None => v.bad_input = true,
                Some(n) => {
                    let min = self.get_attribute(id, "min")
                        .and_then(|s| crate::html::forms::parse_floating_point(&s));
                    let max = self.get_attribute(id, "max")
                        .and_then(|s| crate::html::forms::parse_floating_point(&s));
                    if let Some(min) = min { v.range_underflow = n < min; }
                    if let Some(max) = max { v.range_overflow = n > max; }

                    let step = match self.get_attribute(id, "step") {
                        Some(s) if s.trim().eq_ignore_ascii_case("any") => None,
                        Some(s) => crate::html::forms::parse_floating_point(&s)
                            .filter(|s| *s > 0.0)
                            .or(Some(1.0)),
                        None => Some(1.0),
                    };
                    if let Some(step) = step {
                        let base = min.unwrap_or(0.0);
                        let offset = (n - base) / step;
                        // A float division never lands exactly on an integer,
                        // so the test is "within a rounding error of one".
                        v.step_mismatch = (offset - offset.round()).abs() > 1e-9;
                    }
                }
            }
        }
        v
    }

    /// `element.validationMessage` — empty when the element is valid or barred.
    ///
    /// The wording is implementation-defined; the spec asks only for a
    /// suitably descriptive message. A custom message wins over every built-in
    /// one, which is what makes `setCustomValidity` useful.
    pub fn validation_message(&self, id: u32) -> String {
        if !self.will_validate(id) { return String::new(); }
        let v = self.validity(id);
        if v.valid() { return String::new(); }
        if v.custom_error {
            return self.custom_validity.get(&id).cloned().unwrap_or_default();
        }
        let tag = self.tag_name(id).unwrap_or("");
        if v.value_missing {
            return match tag {
                "select" => "Please select an item in the list.".into(),
                _ => "Please fill out this field.".into(),
            };
        }
        if v.type_mismatch {
            return match self.input_type(id).as_str() {
                "email" => "Please enter an email address.".into(),
                _ => "Please enter a URL.".into(),
            };
        }
        if v.pattern_mismatch { return "Please match the requested format.".into(); }
        if v.too_long { return "Please shorten this text.".into(); }
        if v.too_short { return "Please lengthen this text.".into(); }
        if v.range_underflow {
            let min = self.get_attribute(id, "min").unwrap_or_default();
            return format!("Value must be greater than or equal to {}.", min);
        }
        if v.range_overflow {
            let max = self.get_attribute(id, "max").unwrap_or_default();
            return format!("Value must be less than or equal to {}.", max);
        }
        if v.step_mismatch { return "Please enter a valid value.".into(); }
        if v.bad_input { return "Please enter a number.".into(); }
        String::new()
    }

    /// `element.setCustomValidity(message)`. An empty message clears it, which
    /// is the only way to clear it.
    pub fn set_custom_validity(&mut self, id: u32, message: &str) {
        if message.is_empty() {
            self.custom_validity.remove(&id);
        } else {
            self.custom_validity.insert(id, message.to_string());
        }
    }

    /// `element.checkValidity()` — and on a `<form>`, the static validation of
    /// every control it owns.
    ///
    /// Fires an `invalid` event at each element that fails, as the spec
    /// requires. That event is what a page listens for to render its own
    /// messages, so a `checkValidity` that only returned a bool would be
    /// answering the question and skipping the mechanism.
    pub fn check_validity(&mut self, id: u32) -> bool {
        if self.tag_name(id) == Some("form") {
            let mut all_valid = true;
            for control in self.form_elements(id) {
                if !self.check_validity(control) { all_valid = false; }
            }
            return all_valid;
        }
        if !self.will_validate(id) { return true; }
        if self.validity(id).valid() { return true; }
        self.fire_invalid_event(id);
        false
    }

    /// `element.reportValidity()`.
    ///
    /// Same answer as `checkValidity`, plus "report the problems to the user".
    /// Reporting is the host's job — there is no built-in bubble here — so the
    /// element is focused, which is the part of the spec's reporting steps a
    /// layout engine owns.
    pub fn report_validity(&mut self, id: u32) -> bool {
        let valid = self.check_validity(id);
        if !valid {
            let target = if self.tag_name(id) == Some("form") {
                self.form_elements(id).into_iter().find(|c| {
                    self.will_validate(*c) && !self.validity(*c).valid()
                })
            } else {
                Some(id)
            };
            if let Some(t) = target { self.focus(t); }
        }
        valid
    }

    fn fire_invalid_event(&mut self, id: u32) {
        // `invalid` does not bubble and IS cancelable (HTML §4.10.19.3).
        let mut event = crate::dom::events::DomEvent::new("invalid", id);
        self.dispatch_event(&mut event);
    }

    /// Disabled, or inside a disabled `<fieldset>` that is not shielding it
    /// through the fieldset's first `<legend>`.
    fn is_actually_disabled(&self, id: u32) -> bool {
        if self.has_attribute(id, "disabled") { return true; }
        let mut child = id;
        let mut cursor = self.parent_element(id);
        while let Some(node) = cursor {
            if self.tag_name(node) == Some("fieldset") && self.has_attribute(node, "disabled") {
                // The first `<legend>` of a disabled fieldset is NOT disabled.
                let first_legend = self.children(node).into_iter()
                    .find(|c| self.tag_name(*c) == Some("legend"));
                if first_legend != Some(child) { return true; }
            }
            child = node;
            cursor = self.parent_element(node);
        }
        false
    }

    /// Whether `readonly` has any effect on this input's type.
    fn readonly_applies(&self, id: u32) -> bool {
        matches!(
            self.input_type(id).as_str(),
            "text" | "search" | "url" | "tel" | "email" | "password" | "date" | "month"
                | "week" | "time" | "datetime-local" | "number"
        )
    }

    /// A `<select required>` is missing a value when its selected option is
    /// the placeholder — the first option, with an empty value, when the
    /// select is not `multiple` and has no `size` above one.
    fn select_has_no_placeholder_selection(&self, id: u32) -> bool {
        let value = self.value(id);
        value.is_empty()
    }

    fn radio_group_has_a_checked_member(&self, id: u32) -> bool {
        let name = self.get_attribute(id, "name").unwrap_or_default();
        if name.is_empty() { return self.checked(id); }
        let owner = self.form_owner(id);
        let mut any = false;
        self.walk_tree(self.root.node_id, &mut |doc, node| {
            if any { return; }
            if doc.tag_name(node) != Some("input") { return; }
            if doc.input_type(node) != "radio" { return; }
            if doc.get_attribute(node, "name").unwrap_or_default() != name { return; }
            if doc.form_owner(node) != owner { return; }
            if doc.checked(node) { any = true; }
        });
        any
    }

    fn numeric_attribute(&self, id: u32, name: &str) -> Option<u32> {
        self.get_attribute(id, name)
            .and_then(|v| crate::html::forms::parse_non_negative_integer(&v))
    }

    fn value_is_dirty(&self, id: u32) -> bool {
        self.find_webcore(id).is_some_and(|n| n.dirty_value)
    }
}

/// The input types `pattern` applies to (HTML §4.10.5.3.6).
const PATTERN_TYPES: &[&str] =
    &["text", "search", "url", "tel", "email", "password"];

/// The input types `maxlength`/`minlength` apply to (HTML §4.10.5.3.3).
const LENGTH_TYPES: &[&str] =
    &["text", "search", "url", "tel", "email", "password"];

/// The input types whose value is a NUMBER, for range and step constraints.
///
/// The date and time types also have `min`/`max`/`step`, and they belong here
/// — but their value is a date string, not a float, so putting them in this
/// list today would make every well-formed `2026-08-29` a `badInput`. They
/// join when there is a date parser to convert them; leaving them out costs a
/// missing constraint, putting them in would cost a wrong one.
const NUMERIC_TYPES: &[&str] = &["number", "range"];

// ─── The table interfaces — HTML §4.9.1–§4.9.11 ─────────────────────────────
//
// `rows` is the member that shows why these cannot be a plain child walk: it
// is thead's rows, then every tbody's rows in tree order, then tfoot's —
// whatever order the sections appear in the markup. Chrome on a table whose
// `<tfoot>` is written FIRST still answers `h1,r1,r2,r3,f1`.
//
// The index errors are `Option::None` here. Chrome throws `IndexSizeError`;
// this crate has no exception channel, and an out-of-range insert that
// silently appended would be the one outcome no caller can detect.
impl Document {
    /// `table.rows` — HTML §4.9.1, in the spec's order rather than the
    /// document's.
    pub fn table_rows(&self, table: u32) -> Vec<u32> {
        let mut out = Vec::new();
        if let Some(head) = self.t_head(table) { out.extend(self.section_rows(head)); }
        for body in self.t_bodies(table) { out.extend(self.section_rows(body)); }
        // A `<tr>` that is a direct child of the table — which the parser only
        // produces for a fragment — sits with the bodies.
        for child in self.children(table) {
            if self.tag_name(child) == Some("tr") { out.push(child); }
        }
        if let Some(foot) = self.t_foot(table) { out.extend(self.section_rows(foot)); }
        out
    }

    /// `table.tBodies`.
    pub fn t_bodies(&self, table: u32) -> Vec<u32> {
        self.children(table).into_iter()
            .filter(|c| self.tag_name(*c) == Some("tbody"))
            .collect()
    }

    /// `table.tHead` — the first `<thead>` child, or none.
    pub fn t_head(&self, table: u32) -> Option<u32> {
        self.children(table).into_iter().find(|c| self.tag_name(*c) == Some("thead"))
    }

    /// `table.tFoot`.
    pub fn t_foot(&self, table: u32) -> Option<u32> {
        self.children(table).into_iter().find(|c| self.tag_name(*c) == Some("tfoot"))
    }

    /// `table.caption`.
    pub fn caption(&self, table: u32) -> Option<u32> {
        self.children(table).into_iter().find(|c| self.tag_name(*c) == Some("caption"))
    }

    /// `section.rows` — the `<tr>` children of one `<thead>`/`<tbody>`/`<tfoot>`.
    pub fn section_rows(&self, section: u32) -> Vec<u32> {
        self.children(section).into_iter()
            .filter(|c| self.tag_name(*c) == Some("tr"))
            .collect()
    }

    /// `tr.cells` — the `<td>` and `<th>` children.
    pub fn row_cells(&self, row: u32) -> Vec<u32> {
        self.children(row).into_iter()
            .filter(|c| matches!(self.tag_name(*c), Some("td") | Some("th")))
            .collect()
    }

    /// `tr.rowIndex` — the row's position in its TABLE's `rows`, or −1 when it
    /// is not in a table.
    pub fn row_index(&self, row: u32) -> i32 {
        let Some(table) = self.owning_table(row) else { return -1 };
        self.table_rows(table).iter().position(|r| *r == row).map_or(-1, |i| i as i32)
    }

    /// `tr.sectionRowIndex` — the row's position within its own section, which
    /// is a different number from `rowIndex` for every row after the first
    /// section.
    pub fn section_row_index(&self, row: u32) -> i32 {
        let Some(parent) = self.parent_element(row) else { return -1 };
        self.section_rows(parent).iter().position(|r| *r == row).map_or(-1, |i| i as i32)
    }

    /// `td.cellIndex` / `th.cellIndex`.
    pub fn cell_index(&self, cell: u32) -> i32 {
        let Some(row) = self.parent_element(cell) else { return -1 };
        if self.tag_name(row) != Some("tr") { return -1; }
        self.row_cells(row).iter().position(|c| *c == cell).map_or(-1, |i| i as i32)
    }

    /// `td.colSpan` — at least 1, at most 1000 (HTML §4.9.11).
    pub fn col_span(&self, cell: u32) -> u32 {
        self.span_attribute(cell, "colspan", 1, 1000)
    }
    pub fn set_col_span(&mut self, cell: u32, span: u32) {
        self.set_attribute(cell, "colspan", &span.to_string());
    }

    /// `td.rowSpan` — at least 0 (0 means "to the end of the section"), at
    /// most 65534.
    pub fn row_span(&self, cell: u32) -> u32 {
        self.span_attribute(cell, "rowspan", 1, 65534)
    }
    pub fn set_row_span(&mut self, cell: u32, span: u32) {
        self.set_attribute(cell, "rowspan", &span.to_string());
    }

    fn span_attribute(&self, cell: u32, name: &str, default: u32, max: u32) -> u32 {
        match self.get_attribute(cell, name)
            .and_then(|v| crate::html::forms::parse_non_negative_integer(&v))
        {
            Some(n) if name == "rowspan" => n.min(max),
            Some(n) => n.clamp(1, max),
            None => default,
        }
    }

    /// `table.createCaption()` — returns the existing one if there is one,
    /// which is why it is not simply "create".
    pub fn create_caption(&mut self, table: u32) -> u32 {
        if let Some(existing) = self.caption(table) { return existing; }
        let caption = self.create_element("caption");
        let first = self.first_child(table).unwrap_or(0);
        if first == 0 { self.append_child(table, caption); }
        else { self.insert_before(table, caption, first); }
        caption
    }

    pub fn delete_caption(&mut self, table: u32) {
        if let Some(c) = self.caption(table) { self.remove_child(c); }
    }

    /// `table.createTHead()` — idempotent, and placed before every section.
    pub fn create_t_head(&mut self, table: u32) -> u32 {
        if let Some(existing) = self.t_head(table) { return existing; }
        let head = self.create_element("thead");
        // After any `<caption>` and `<colgroup>`, before the first section.
        let anchor = self.children(table).into_iter().find(|c| {
            !matches!(self.tag_name(*c), Some("caption") | Some("colgroup"))
        });
        match anchor {
            Some(a) => self.insert_before(table, head, a),
            None => self.append_child(table, head),
        }
        head
    }

    pub fn delete_t_head(&mut self, table: u32) {
        if let Some(h) = self.t_head(table) { self.remove_child(h); }
    }

    /// `table.createTFoot()` — idempotent, appended last.
    pub fn create_t_foot(&mut self, table: u32) -> u32 {
        if let Some(existing) = self.t_foot(table) { return existing; }
        let foot = self.create_element("tfoot");
        self.append_child(table, foot);
        foot
    }

    pub fn delete_t_foot(&mut self, table: u32) {
        if let Some(f) = self.t_foot(table) { self.remove_child(f); }
    }

    /// `table.createTBody()` — always a NEW one, unlike the other three.
    pub fn create_t_body(&mut self, table: u32) -> u32 {
        let body = self.create_element("tbody");
        match self.t_bodies(table).last() {
            Some(last) => {
                let after = self.next_sibling(*last);
                if after == 0 { self.append_child(table, body); }
                else { self.insert_before(table, body, after); }
            }
            None => self.append_child(table, body),
        }
        body
    }

    /// `table.insertRow(index)` — `None` is the spec's `IndexSizeError`.
    ///
    /// The placement rule is the interesting part: the new row goes into the
    /// PARENT of the row currently at `index`, so `insertRow(0)` on a table
    /// with a `<thead>` puts a row in the head, and `insertRow()` appends to
    /// whatever section holds the last row — a `<tfoot>` if there is one.
    /// Chrome demonstrates both.
    pub fn insert_row(&mut self, table: u32, index: i32) -> Option<u32> {
        let rows = self.table_rows(table);
        if index < -1 || index > rows.len() as i32 { return None; }
        let row = self.create_element("tr");

        if rows.is_empty() {
            let body = match self.t_bodies(table).last() {
                Some(b) => *b,
                None => {
                    let b = self.create_element("tbody");
                    self.append_child(table, b);
                    b
                }
            };
            self.append_child(body, row);
            return Some(row);
        }
        if index == -1 || index == rows.len() as i32 {
            let last = *rows.last().unwrap();
            let parent = self.parent_element(last).unwrap_or(table);
            self.append_child(parent, row);
        } else {
            let reference = rows[index as usize];
            let parent = self.parent_element(reference).unwrap_or(table);
            self.insert_before(parent, row, reference);
        }
        Some(row)
    }

    /// `table.deleteRow(index)` — `false` is `IndexSizeError`.
    pub fn delete_row(&mut self, table: u32, index: i32) -> bool {
        let rows = self.table_rows(table);
        let target = match index {
            -1 => match rows.last() { Some(r) => *r, None => return false },
            i if i < 0 || i >= rows.len() as i32 => return false,
            i => rows[i as usize],
        };
        self.remove_child(target);
        true
    }

    /// `tbody.insertRow(index)` / `thead.insertRow(index)`.
    pub fn section_insert_row(&mut self, section: u32, index: i32) -> Option<u32> {
        let rows = self.section_rows(section);
        if index < -1 || index > rows.len() as i32 { return None; }
        let row = self.create_element("tr");
        if index == -1 || index == rows.len() as i32 {
            self.append_child(section, row);
        } else {
            self.insert_before(section, row, rows[index as usize]);
        }
        Some(row)
    }

    /// `tbody.deleteRow(index)`.
    pub fn section_delete_row(&mut self, section: u32, index: i32) -> bool {
        let rows = self.section_rows(section);
        let target = match index {
            -1 => match rows.last() { Some(r) => *r, None => return false },
            i if i < 0 || i >= rows.len() as i32 => return false,
            i => rows[i as usize],
        };
        self.remove_child(target);
        true
    }

    /// `tr.insertCell(index)` — always a `<td>`, never a `<th>`.
    pub fn insert_cell(&mut self, row: u32, index: i32) -> Option<u32> {
        let cells = self.row_cells(row);
        if index < -1 || index > cells.len() as i32 { return None; }
        let cell = self.create_element("td");
        if index == -1 || index == cells.len() as i32 {
            self.append_child(row, cell);
        } else {
            self.insert_before(row, cell, cells[index as usize]);
        }
        Some(cell)
    }

    /// `tr.deleteCell(index)`.
    pub fn delete_cell(&mut self, row: u32, index: i32) -> bool {
        let cells = self.row_cells(row);
        let target = match index {
            -1 => match cells.last() { Some(c) => *c, None => return false },
            i if i < 0 || i >= cells.len() as i32 => return false,
            i => cells[i as usize],
        };
        self.remove_child(target);
        true
    }

    /// The `<table>` a row belongs to, through its section if it has one.
    fn owning_table(&self, row: u32) -> Option<u32> {
        let parent = self.parent_element(row)?;
        match self.tag_name(parent) {
            Some("table") => Some(parent),
            Some("thead") | Some("tbody") | Some("tfoot") => self.parent_element(parent),
            _ => None,
        }
    }
}

// ─── Text selection on form controls (HTML §4.10.19.3) ──────────────────────
//
// Offsets here are **UTF-16 code units**, which is what the IDL says and what
// `character_data_length` above already answers in. The internal cursor is a
// CHAR index, because that is what the key handler and the paint path want, so
// every entry point converts. The two agree on ASCII, which is every existing
// fixture — so the tests for this deliberately use a BMP non-ASCII string and
// an astral one, which fail differently.
//
// `None`/`false` is the thrown exception, following the rest of this file:
// `InvalidStateError` when the control does not support selection, and
// `IndexSizeError` when `setRangeText` gets a start past its end.

/// Char index → UTF-16 offset.
fn char_to_utf16(s: &str, chars: usize) -> usize {
    s.chars().take(chars).map(char::len_utf16).sum()
}

/// UTF-16 offset → char index, rounding **down** to a scalar boundary.
///
/// ⛔ An offset can land between the halves of a surrogate pair — Chrome
/// happily selects one half, and `setRangeText` there yields a lone surrogate.
/// A Rust `String` cannot hold one, so the boundary moves outward instead.
/// This is the single named deviation in this area.
fn utf16_to_char_floor(s: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (i, c) in s.chars().enumerate() {
        if seen >= units { return i; }
        // The offset falls INSIDE this character — round back to where it
        // starts. Advancing first and testing after would round the other way
        // and cut the pair off at its far edge.
        if seen + c.len_utf16() > units { return i; }
        seen += c.len_utf16();
    }
    s.chars().count()
}

/// UTF-16 offset → char index, rounding **up** to a scalar boundary.
fn utf16_to_char_ceil(s: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (i, c) in s.chars().enumerate() {
        if seen >= units { return i; }
        seen += c.len_utf16();
        if seen >= units { return i + 1; }
    }
    s.chars().count()
}

impl Document {
    /// The control's value as the selection API sees it, plus its length in
    /// UTF-16 code units.
    ///
    /// A `<textarea>`'s API value is LF-normalised, and the offsets index THAT
    /// — not the raw child text (measured: `setRangeText("X",0,5)` on
    /// `"line1\nline2"` yields `"X\nline2"`).
    fn selection_value(&self, id: u32) -> Option<String> {
        let node = self.find_webcore(id)?;
        if !crate::types::selection_api_applies(node) { return None; }
        Some(self.value(id))
    }

    /// Read (start, end) in UTF-16 units, clamped to the current value.
    fn selection_pair(&self, id: u32) -> Option<(usize, usize)> {
        let value = self.selection_value(id)?;
        let node = self.find_webcore(id)?;
        let len = value.chars().count();
        let cursor = node.input_cursor.min(len);
        let anchor = node.input_sel_anchor.min(len);
        Some((
            char_to_utf16(&value, cursor.min(anchor)),
            char_to_utf16(&value, cursor.max(anchor)),
        ))
    }

    /// Write (start, end) in UTF-16 units. `direction` of `None` leaves the
    /// existing direction alone — which is what the `selectionStart` and
    /// `selectionEnd` setters do, and what `setSelectionRange` does NOT.
    fn store_selection(
        &mut self,
        id: u32,
        start: usize,
        end: usize,
        direction: Option<crate::types::SelectionDirection>,
    ) {
        let Some(value) = self.selection_value(id) else { return };
        // No clamp to the value's length here: `utf16_to_char_floor` saturates,
        // so an offset past the end already lands on the last character. A
        // `.min(len)` in front of it was redundant, and a mutation run proved
        // no test could tell it from its absence.
        //
        // "If end is less than start then set start to end" — the range
        // collapses onto its END, not its start (measured: `(3,1)` → `[1,1]`).
        let start = start.min(end);
        let start_c = utf16_to_char_floor(&value, start);
        let end_c = utf16_to_char_floor(&value, end);
        let Some(node) = self.find_webcore_mut(id) else { return };
        if let Some(d) = direction { node.input_sel_direction = d; }
        // A backward selection's cursor sits at its START — the key handler
        // reads the pair as min/max either way, so only the direction field
        // depends on the ordering.
        if node.input_sel_direction == crate::types::SelectionDirection::Backward {
            node.input_cursor = start_c;
            node.input_sel_anchor = end_c;
        } else {
            node.input_cursor = end_c;
            node.input_sel_anchor = start_c;
        }
    }

    /// `input.selectionStart` / `textarea.selectionStart`.
    ///
    /// `None` is the spec's `null`, which every control the API does not apply
    /// to answers — including `number`, `date` and `email`, all of which hold
    /// a value and accept typing.
    pub fn selection_start(&self, id: u32) -> Option<u32> {
        self.selection_pair(id).map(|(s, _)| s as u32)
    }

    /// `element.selectionEnd`.
    pub fn selection_end(&self, id: u32) -> Option<u32> {
        self.selection_pair(id).map(|(_, e)| e as u32)
    }

    /// `element.selectionDirection` — `"none"`, `"forward"` or `"backward"`.
    pub fn selection_direction(&self, id: u32) -> Option<&'static str> {
        let node = self.find_webcore(id)?;
        if !crate::types::selection_api_applies(node) { return None; }
        Some(node.input_sel_direction.as_str())
    }

    /// `element.selectionStart = n`. `false` is `InvalidStateError`.
    ///
    /// ⛔ Not `set_selection_range` with the current end: a start past the end
    /// drags the END along (measured: start=8 over a (1,5) selection gives
    /// `[8,8]`, not `[5,5]`), and the direction is left untouched.
    pub fn set_selection_start(&mut self, id: u32, start: u32) -> bool {
        let Some((_, end)) = self.selection_pair(id) else { return false };
        let start = start as usize;
        self.store_selection(id, start, end.max(start), None);
        true
    }

    /// `element.selectionEnd = n`. An end before the start pulls the start
    /// back with it (measured: end=0 over a (3,5) selection gives `[0,0]`).
    pub fn set_selection_end(&mut self, id: u32, end: u32) -> bool {
        let Some((start, _)) = self.selection_pair(id) else { return false };
        self.store_selection(id, start, end as usize, None);
        true
    }

    /// `element.selectionDirection = s`.
    pub fn set_selection_direction(&mut self, id: u32, direction: &str) -> bool {
        let Some((start, end)) = self.selection_pair(id) else { return false };
        self.store_selection(id, start, end, Some(crate::types::SelectionDirection::parse(direction)));
        true
    }

    /// `element.setSelectionRange(start, end, direction)`.
    ///
    /// Passing `None` for `direction` is the two-argument call, which RESETS
    /// the direction to `"none"` — the one place it differs from the
    /// `selectionStart` setter.
    pub fn set_selection_range(
        &mut self,
        id: u32,
        start: u32,
        end: u32,
        direction: Option<&str>,
    ) -> bool {
        if self.selection_value(id).is_none() { return false; }
        let d = direction
            .map(crate::types::SelectionDirection::parse)
            .unwrap_or(crate::types::SelectionDirection::None);
        self.store_selection(id, start as usize, end as usize, Some(d));
        self.fire_select_event(id);
        true
    }

    /// `element.select()` — select everything the control holds.
    ///
    /// ⛔ Unlike every other member here, this one is NOT gated on the API
    /// applying: Chrome runs `checkbox.select()`, `number.select()` and
    /// `file.select()` without complaint. The spec's step is "if this has no
    /// selectable text, return" — a return, not a throw.
    pub fn select(&mut self, id: u32) {
        let Some(value) = self.selection_value(id) else { return };
        let len = value.encode_utf16().count();
        self.store_selection(id, 0, len, Some(crate::types::SelectionDirection::None));
        self.fire_select_event(id);
    }

    /// `element.setRangeText(replacement, start, end, selectMode)`.
    ///
    /// `range` of `None` is the one-argument call, which uses the current
    /// selection. `false` is `InvalidStateError` (the API does not apply) or
    /// `IndexSizeError` (`start` past `end`); an unrecognised `select_mode` is
    /// the IDL enum's `TypeError` and is likewise `false`.
    pub fn set_range_text(
        &mut self,
        id: u32,
        replacement: &str,
        range: Option<(u32, u32)>,
        select_mode: &str,
    ) -> bool {
        let Some(value) = self.selection_value(id) else { return false };
        if !matches!(select_mode, "select" | "start" | "end" | "preserve") { return false; }
        let (sel_start, sel_end) = match self.selection_pair(id) {
            Some(p) => p,
            None => return false,
        };
        let (mut start, mut end) = match range {
            Some((s, e)) => {
                // IndexSizeError. Checked BEFORE clamping, so `(10, 20)` on a
                // two-character value is legal and `(3, 1)` is not.
                if s > e { return false; }
                (s as usize, e as usize)
            }
            None => (sel_start, sel_end),
        };
        let max = value.encode_utf16().count();
        start = start.min(max);
        end = end.min(max);

        // Round the replaced range OUTWARD to whole scalar values. See
        // `utf16_to_char_floor`: a boundary inside a surrogate pair is
        // representable in Chrome and not in a Rust `String`.
        let start_c = utf16_to_char_floor(&value, start);
        let end_c = utf16_to_char_ceil(&value, end);
        let chars: Vec<char> = value.chars().collect();
        let mut next: String = chars[..start_c].iter().collect();
        next.push_str(replacement);
        next.extend(chars[end_c.min(chars.len())..].iter());

        // Recompute the offsets from what was actually cut, so the arithmetic
        // below stays consistent when a boundary moved.
        let start = char_to_utf16(&value, start_c);
        let end = char_to_utf16(&value, end_c);
        let new_length = replacement.encode_utf16().count();
        let new_end = start + new_length;

        // The plain `value` setter's "move the cursor to the end" is harmless
        // here: `store_selection` below positions the selection unconditionally
        // and clamps it to the value just written.
        self.set_value(id, &next);

        let (mut new_start, mut new_sel_end) = (sel_start, sel_end);
        match select_mode {
            "select" => { new_start = start; new_sel_end = new_end; }
            "start" => { new_start = start; new_sel_end = start; }
            "end" => { new_start = new_end; new_sel_end = new_end; }
            // The one that carries arithmetic. Verified against Chrome for all
            // seven positions the old selection can take relative to the
            // replaced range: before, after, straddling either edge, fully
            // inside, and with the replacement both growing and shrinking.
            _ => {
                let old_length = end.saturating_sub(start);
                let delta = new_length as i64 - old_length as i64;
                // An offset PAST the replaced range slides by the size change;
                // one INSIDE it collapses onto the edge it is nearest in role
                // — the start onto the range's start, the end onto its new end.
                if sel_start > end { new_start = (sel_start as i64 + delta).max(0) as usize; }
                else if sel_start > start { new_start = start; }
                if sel_end > end { new_sel_end = (sel_end as i64 + delta).max(0) as usize; }
                else if sel_end > start { new_sel_end = new_end; }
            }
        }
        // `setRangeText` never carries a direction — the range it leaves is
        // directionless whatever the selection it replaced was (measured).
        self.store_selection(id, new_start, new_sel_end, Some(crate::types::SelectionDirection::None));
        self.fire_select_event(id);
        true
    }

    /// `select` does not bubble and is not cancelable.
    ///
    /// ⛔ The spec QUEUES this task; there is no task queue in this crate, and
    /// `fire_invalid_event` already made the same trade. A listener runs — it
    /// just runs before the caller returns rather than after.
    fn fire_select_event(&mut self, id: u32) {
        let mut event = crate::dom::events::DomEvent::new("select", id);
        self.dispatch_event(&mut event);
    }
}

// ─── Document metadata and the document collections (HTML §3.1.1, DOM §4.5) ─
//
// Everything here was read off Chrome first (`/tmp/webcore-html/doc1.html`,
// `dt/`). Four of the collection rules are not what the names suggest, and each
// is a place a plausible implementation is wrong:
//
//   * `links` is `<a>` AND `<area>`, and only those WITH an `href`.
//   * `anchors` is `<a>` with a **name** attribute — an `<a name href>` is in
//     both lists, and a bare `<a>` is in neither.
//   * `getElementsByName` matches ANY element, not just form controls: a
//     `<div name=x>` comes back.
//   * `applets` is always empty, and `plugins` is the same list as `embeds`.
//
// These return `Vec<u32>` — a snapshot — where Chrome's are LIVE
// `HTMLCollection`s. Measured: appending an `<a href>` grows `document.links`
// from 2 to 3 without re-reading it. Noted in `architecture.md`; a live
// collection is a different object model, not a different filter.

impl Document {
    /// `document.doctype` — 0 when the document has none.
    pub fn doctype(&self) -> Option<u32> {
        (self.doctype != 0).then_some(self.doctype)
    }

    /// `doctype.name`, which is also its `nodeName`.
    pub fn doctype_name(&self) -> Option<String> {
        self.arena.try_get(NodeId(self.doctype)).map(|n| n.tag.clone())
    }

    /// `doctype.publicId` — the empty string when absent, never null.
    pub fn doctype_public_id(&self) -> Option<String> {
        self.doctype_ident("publicId")
    }

    /// `doctype.systemId`.
    pub fn doctype_system_id(&self) -> Option<String> {
        self.doctype_ident("systemId")
    }

    fn doctype_ident(&self, key: &str) -> Option<String> {
        let node = self.arena.try_get(NodeId(self.doctype))?;
        Some(node.attributes.get(key).cloned().unwrap_or_default())
    }

    /// `document.compatMode` — `"BackCompat"` in quirks, `"CSS1Compat"`
    /// otherwise.
    ///
    /// ⛔ This CANNOT distinguish limited-quirks from no-quirks; both answer
    /// `"CSS1Compat"`. Read `self.quirks` for the real mode.
    pub fn compat_mode(&self) -> &'static str {
        self.quirks.compat_mode()
    }

    /// `document.characterSet` / `.charset` / `.inputEncoding` — three names
    /// for one value (DOM §4.5).
    pub fn character_set(&self) -> &str {
        &self.character_set
    }

    /// `document.contentType`. This crate parses HTML and only HTML.
    pub fn content_type(&self) -> &'static str {
        "text/html"
    }

    /// `document.readyState`.
    ///
    /// Always `"complete"`: `parse_html` RETURNS a finished document, so there
    /// is no moment at which a caller holds one that is still loading. Chrome's
    /// probe answered `"loading"` only because an inline script ran mid-parse,
    /// which is a state this crate never exposes.
    pub fn ready_state(&self) -> &'static str {
        "complete"
    }

    /// `document.visibilityState` and `document.hidden`. No page-visibility
    /// machinery exists, and a document nobody has hidden is visible.
    pub fn visibility_state(&self) -> &'static str { "visible" }
    pub fn document_hidden(&self) -> bool { false }

    /// `document.referrer` — empty with no navigation history to draw on.
    pub fn referrer(&self) -> &'static str { "" }

    /// `document.URL` / `.documentURI` — the base URL the document was parsed
    /// against.
    pub fn document_uri(&self) -> &str { &self.base_url }

    /// `document.links` — `<a>` and `<area>` that HAVE an `href`.
    pub fn links(&self) -> Vec<u32> {
        self.collect_elements(|d, id| {
            matches!(d.tag_name(id), Some("a") | Some("area")) && d.has_attribute(id, "href")
        })
    }

    /// `document.anchors` — `<a>` with a `name`, whether or not it has an
    /// `href`.
    pub fn anchors(&self) -> Vec<u32> {
        self.collect_elements(|d, id| {
            d.tag_name(id) == Some("a") && d.has_attribute(id, "name")
        })
    }

    /// `document.images` — every `<img>`, with or without a `src`.
    pub fn images(&self) -> Vec<u32> {
        self.collect_elements(|d, id| d.tag_name(id) == Some("img"))
    }

    /// `document.forms`.
    pub fn forms(&self) -> Vec<u32> {
        self.collect_elements(|d, id| d.tag_name(id) == Some("form"))
    }

    /// `document.scripts`.
    pub fn scripts(&self) -> Vec<u32> {
        self.collect_elements(|d, id| d.tag_name(id) == Some("script"))
    }

    /// `document.embeds`, and `document.plugins`, which is the same list.
    pub fn embeds(&self) -> Vec<u32> {
        self.collect_elements(|d, id| d.tag_name(id) == Some("embed"))
    }

    /// `document.plugins` — an alias for `embeds` (measured: same length, and
    /// the spec says they return the same collection).
    pub fn plugins(&self) -> Vec<u32> { self.embeds() }

    /// `document.applets`, which HTML defines as always empty — `<applet>` was
    /// removed from the language and the collection kept for compatibility.
    pub fn applets(&self) -> Vec<u32> { Vec::new() }

    /// `document.getElementsByName(name)`.
    ///
    /// ⛔ Any element with that `name` attribute, not just form controls: a
    /// `<div name=x>` is in the list (measured).
    pub fn get_elements_by_name(&self, name: &str) -> Vec<u32> {
        self.collect_elements(|d, id| d.get_attribute(id, "name").as_deref() == Some(name))
    }

    /// `document.getElementsByTagNameNS(ns, local)`.
    ///
    /// `"*"` matches any namespace or any local name. An HTML element carries
    /// no explicit namespace in the arena, so it answers to the XHTML one —
    /// which is what it is in (measured: 3 `<a>` for the XHTML namespace).
    pub fn get_elements_by_tag_name_ns(&self, namespace: &str, local: &str) -> Vec<u32> {
        const XHTML: &str = "http://www.w3.org/1999/xhtml";
        self.collect_elements(|d, id| {
            let Some(node) = d.arena.try_get(NodeId(id)) else { return false };
            let ns = node.namespace.as_deref().unwrap_or(XHTML);
            let ns_ok = namespace == "*" || namespace == ns;
            let name_ok = local == "*"
                || d.local_name(id).eq_ignore_ascii_case(local);
            ns_ok && name_ok
        })
    }

    /// Every element in tree order that satisfies `pred`.
    fn collect_elements(&self, pred: impl Fn(&Document, u32) -> bool) -> Vec<u32> {
        self.get_elements_by_tag_name("*")
            .into_iter()
            .filter(|id| pred(self, *id))
            .collect()
    }

    /// `document.captureEvents()` / `document.releaseEvents()`.
    ///
    /// Defined by HTML as doing nothing at all — they exist so that old scripts
    /// calling them do not throw. Kept as real no-op methods for exactly that
    /// reason (measured: both return without error).
    pub fn capture_events(&self) {}
    pub fn release_events(&self) {}
}
