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
//! HtmlBox tree (bridge period), and set dirty flags for incremental re-style/layout.

use crate::types::{Document, HtmlBox, Rect};
use crate::dom::arena::NodeId;
use crate::css::apply_property;

// ─── Query ──────────────────────────────────────────────────────────────────

impl Document {
    /// Find element by its HTML `id` attribute. Returns stable node_id.
    pub fn get_element_by_id(&self, id: &str) -> Option<u32> {
        fn walk(node: &HtmlBox, id: &str) -> Option<u32> {
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
        let selectors = parse_comma_selectors(selector);
        let empty_hover = std::collections::HashSet::new();
        query_walk_first(&self.root, &[], &selectors, &empty_hover)
    }

    /// Query for all elements matching a CSS selector.
    pub fn query_selector_all(&self, selector: &str) -> Vec<u32> {
        let selectors = parse_comma_selectors(selector);
        let empty_hover = std::collections::HashSet::new();
        let mut results = Vec::new();
        query_walk_all(&self.root, &[], &selectors, &empty_hover, &mut results);
        results
    }
}

/// Split comma-separated selectors and parse each one.
fn parse_comma_selectors(selector: &str) -> Vec<crate::css::CssSelector> {
    selector.split(',')
        .map(|s| crate::css::parse_selector(s.trim()))
        .collect()
}

/// Build ancestor info for the current node's children.
fn build_ancestor_entry(node: &HtmlBox, child_index: usize, sibling_count: usize) -> crate::css::AncestorInfo {
    crate::css::AncestorInfo {
        tag: node.tag.clone(),
        attributes: node.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index: 0,
        type_sibling_count: 1,
        node_id: node.node_id,
    }
}

fn query_walk_first(
    node: &HtmlBox,
    parent_ancestors: &[crate::css::AncestorInfo],
    selectors: &[crate::css::CssSelector],
    hover_chain: &std::collections::HashSet<u32>,
) -> Option<u32> {
    let sibling_count = node.children.len();

    for (child_idx, child) in node.children.iter().enumerate() {
        if child.tag == "#text" || child.node_id == 0 { continue; }

        // Test this child against all selector alternatives
        let ctx = crate::css::MatchContext {
            focused_box: 0,
            keyboard_focus: false,
            type_child_index: 0,
            type_sibling_count: 1,
            html_box: Some(child),
            hover_chain,
            element_id: child.node_id,
            prev_siblings: &[],
        };

        for sel in selectors {
            if crate::css::matches_selector_with_ancestors(
                &sel.parts, &child.tag, &child.attributes,
                child_idx, sibling_count, parent_ancestors, &ctx,
            ) {
                return Some(child.node_id);
            }
        }

        // Recurse into this child's subtree
        let mut child_ancestors = parent_ancestors.to_vec();
        child_ancestors.push(build_ancestor_entry(child, child_idx, sibling_count));
        if let Some(found) = query_walk_first(child, &child_ancestors, selectors, hover_chain) {
            return Some(found);
        }
    }
    None
}

fn query_walk_all(
    node: &HtmlBox,
    parent_ancestors: &[crate::css::AncestorInfo],
    selectors: &[crate::css::CssSelector],
    hover_chain: &std::collections::HashSet<u32>,
    results: &mut Vec<u32>,
) {
    let sibling_count = node.children.len();

    for (child_idx, child) in node.children.iter().enumerate() {
        if child.tag == "#text" || child.node_id == 0 { continue; }

        let ctx = crate::css::MatchContext {
            focused_box: 0,
            keyboard_focus: false,
            type_child_index: 0,
            type_sibling_count: 1,
            html_box: Some(child),
            hover_chain,
            element_id: child.node_id,
            prev_siblings: &[],
        };

        for sel in selectors {
            if crate::css::matches_selector_with_ancestors(
                &sel.parts, &child.tag, &child.attributes,
                child_idx, sibling_count, parent_ancestors, &ctx,
            ) {
                results.push(child.node_id);
                break; // don't double-count from multiple selector alternatives
            }
        }

        // Recurse
        let mut child_ancestors = parent_ancestors.to_vec();
        child_ancestors.push(build_ancestor_entry(child, child_idx, sibling_count));
        query_walk_all(child, &child_ancestors, selectors, hover_chain, results);
    }
}

// ─── Read ───────────────────────────────────────────────────────────────────

impl Document {
    /// `element.tagName`.
    pub fn tag_name(&self, id: u32) -> Option<&str> {
        if id == 0 { return None; }
        Some(self.arena.get(NodeId(id)).tag.as_str())
    }

    /// Get an attribute value.
    pub fn get_attribute(&self, id: u32, key: &str) -> Option<String> {
        if id == 0 { return None; }
        self.arena
            .get(NodeId(id))
            .attributes
            .get(&self.fold_name(key))
            .cloned()
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
        if id == 0 { return 0; }
        self.arena.get(NodeId(id)).parent.0
    }

    /// `node.childNodes` — every child, of every kind, in tree order.
    pub fn child_nodes(&self, id: u32) -> Vec<u32> {
        if id == 0 { return Vec::new(); }
        self.arena.children(NodeId(id)).map(|c| c.0).collect()
    }

    /// Get the next sibling node_id (0 if none).
    pub fn next_sibling(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.get(NodeId(id)).next_sibling.0
    }

    /// `node.previousSibling` — 0 when there is none.
    pub fn previous_sibling(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.get(NodeId(id)).prev_sibling.0
    }
}

// ─── Interactive state reconciliation ───────────────────────────────────────

impl Document {
    /// Copy form state from the render tree back into the arena.
    ///
    /// WHY THIS EXISTS
    ///
    /// Everything in this file dual-writes: the arena first, then the HtmlBox
    /// tree. Interaction cannot follow that rule. `handle_form_click` and
    /// `process_form_input_key` are free functions over `&mut HtmlBox` — they
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
        fn walk(node: &HtmlBox, out: &mut Vec<Snapshot>) {
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
        let node = match self.find_htmlbox(id) {
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
        let src = match self.find_htmlbox(id).cloned() {
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
    fn clone_subtree(&mut self, src: &HtmlBox, deep: bool) -> HtmlBox {
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
        if let Some(node) = self.find_htmlbox_mut(id) {
            node.text = data.to_string();
            node.layout.layout_dirty = true;
            node.layout.intrinsic_dirty = true;
        }
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
        self.find_htmlbox(id).map(|n| n.checkedness).unwrap_or(false)
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
        if let Some(node) = self.find_htmlbox_mut(id) {
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
        self.find_htmlbox(id).map(|node| &node.style)
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

    /// `element.matches(selectors)`.
    pub fn matches(&self, id: u32, selectors: &str) -> bool {
        match self.find_htmlbox(id) {
            Some(node) => selectors
                .split(',')
                .any(|s| crate::dom::matches_simple_selector(node, s.trim())),
            None => false,
        }
    }

    /// `element.closest(selectors)` — the nearest ancestor-or-self that
    /// matches. Starts at the element ITSELF, per DOM §4.9.
    pub fn closest(&self, id: u32, selectors: &str) -> Option<u32> {
        let mut current = id;
        while current != 0 {
            if self.matches(current, selectors) {
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
// htmlbox parses HTML, where every element is in the HTML namespace and
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
        let mut b = HtmlBox::new(qualified_name);
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
        let mut b = HtmlBox::new("#cdata-section");
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
        let mut b = HtmlBox::new(target);
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
// item list, and htmlbox's own renderer already reads them that way (it walks
// `sel.children` collecting `option` and flattening `optgroup`). So these are
// ordinary tree operations, and an item added here is one the renderer draws
// and the serializer round-trips, with nothing to keep in sync.
//
// Selection is the exception: htmlbox keeps it in `node.data["_selected_idx"]`,
// which is where its mouse and keyboard paths already write. Reading and
// writing THAT is what makes a programmatic selection and a user's click mean
// the same thing.

impl Document {
    /// Option node_ids in tree order, flattening `<optgroup>` — HTML counts
    /// options through a group, not around it, so `selectedIndex` does too.
    fn option_ids(&self, select: u32) -> Vec<u32> {
        fn walk(node: &HtmlBox, out: &mut Vec<u32>) {
            for child in &node.children {
                match child.tag.as_str() {
                    "option" => out.push(child.node_id),
                    "optgroup" => walk(child, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        if let Some(sel) = self.find_htmlbox(select) {
            walk(sel, &mut out);
        }
        out
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
    }

    /// `select.remove(index)`. Out of range removes nothing, as the IDL says.
    pub fn remove_item(&mut self, select: u32, index: usize) {
        if let Some(&id) = self.option_ids(select).get(index) {
            self.remove_child(id);
        }
    }

    /// Drop every option. `select.length = 0` in the IDL.
    pub fn clear_items(&mut self, select: u32) {
        for id in self.option_ids(select) {
            self.remove_child(id);
        }
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
        }
    }

    /// `select.selectedIndex`. `-1` when there is nothing to select, which is
    /// the IDL's answer for an empty select; otherwise a dropdown always has a
    /// selection, defaulting to the first option.
    pub fn selected_index(&self, select: u32) -> i32 {
        let count = self.option_ids(select).len();
        if count == 0 {
            return -1;
        }
        let idx = self
            .find_htmlbox(select)
            .and_then(|n| n.data.get("_selected_idx"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        if idx < count { idx as i32 } else { 0 }
    }

    /// `select.selectedIndex = i`.
    ///
    /// Writes htmlbox's own `_selected_idx` — the same slot its click and
    /// arrow-key handling use, so a programmatic selection and a user's are
    /// indistinguishable afterwards — AND reflects `selected` onto the options
    /// so the DOM tells the same story as the widget.
    pub fn set_selected_index(&mut self, select: u32, index: i32) {
        let options = self.option_ids(select);
        if options.is_empty() {
            return;
        }
        // A negative assignment means "nothing selected" in the IDL. There is
        // no such state in `_selected_idx`, so it only clears `selected`.
        let chosen = usize::try_from(index).ok().filter(|i| *i < options.len());
        for (i, id) in options.iter().enumerate() {
            if Some(i) == chosen {
                self.set_attribute(*id, "selected", "");
            } else {
                self.remove_attribute(*id, "selected");
            }
        }
        // Nested rather than a let-chain: this crate is not on edition 2024.
        if let Some(i) = chosen {
            if let Some(sel) = self.find_htmlbox_mut(select) {
                sel.data.insert("_selected_idx".into(), i.to_string());
            }
        }
    }

    /// `element.value` — which means three different things, exactly as HTML
    /// says it does.
    ///
    /// A `<textarea>`'s value is its text content; a `<select>`'s is the VALUE
    /// OF ITS SELECTED OPTION (falling back to that option's label, per
    /// `option.value`); everything else reads the `value` attribute. htmlbox's
    /// `input_value` covers the first and last but answers a `<select>` from a
    /// `value` attribute a select does not have.
    pub fn value(&self, id: u32) -> String {
        if id == 0 { return String::new(); }
        let tag = self.find_htmlbox(id).map(|n| n.tag.clone()).unwrap_or_default();
        match tag.as_str() {
            "textarea" => self.text_content(id),
            "select" => {
                let idx = self.selected_index(id);
                match usize::try_from(idx).ok().and_then(|i| self.option_ids(id).get(i).copied()) {
                    Some(option) => self
                        .get_attribute(option, "value")
                        .unwrap_or_else(|| self.text_content(option).trim().to_string()),
                    None => String::new(),
                }
            }
            // A checkbox or radio with no `value` attribute submits `"on"`,
            // not the empty string — HTML §4.10.5.1.15 spells that default out
            // because a form needs SOMETHING to send for a ticked box. Every
            // other control's `value` does default to empty.
            "input"
                if matches!(
                    self.get_attribute(id, "type")
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .as_str(),
                    "checkbox" | "radio"
                ) =>
            {
                self.get_attribute(id, "value")
                    .unwrap_or_else(|| "on".to_string())
            }
            _ => self.get_attribute(id, "value").unwrap_or_default(),
        }
    }

    /// `element.value = v`, the setter half of the above.
    pub fn set_value(&mut self, id: u32, value: &str) {
        if id == 0 { return; }
        let tag = self.find_htmlbox(id).map(|n| n.tag.clone()).unwrap_or_default();
        match tag.as_str() {
            "textarea" => self.set_text_content(id, value),
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
            _ => self.set_attribute(id, "value", value),
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
// The user-agent stylesheet rules this file has to stand in for, since
// htmlbox's UA sheet has no `dialog` entry at all:
//
//     dialog:not([open]) { display: none }
//     dialog             { position: absolute }
//     dialog:modal       { position: fixed }
//
// `position` is what separates the two show methods: a modal is laid out
// against the VIEWPORT, a non-modal stays with its containing block. If both
// looked alike, `showModal()` would be `show()` under another name.

impl Document {
    /// `dialog.show()` / `dialog.showModal()`.
    pub fn show_dialog(&mut self, id: u32, modal: bool) {
        if id == 0 { return; }
        self.set_attribute(id, "open", "");
        self.set_style_property(id, "display", "block");
        self.set_style_property(id, "position", if modal { "fixed" } else { "absolute" });
    }

    /// `dialog.close()`.
    pub fn close_dialog(&mut self, id: u32) {
        if id == 0 { return; }
        self.remove_attribute(id, "open");
        // `dialog:not([open]) { display: none }` — a closed dialog is not
        // merely invisible, it is out of flow and takes up no space.
        self.set_style_property(id, "display", "none");
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
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return 0; }
        match self.arena.get(NodeId(id)).node_type {
            crate::dom::arena::NodeType::Element  => 1,
            crate::dom::arena::NodeType::Text     => 3,
            crate::dom::arena::NodeType::CData    => 4,
            crate::dom::arena::NodeType::ProcessingInstruction => 7,
            crate::dom::arena::NodeType::Comment  => 8,
            crate::dom::arena::NodeType::Document => 9,
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
        if id == 0 || !self.arena.is_alive(NodeId(id)) { return Vec::new(); }
        self.arena.get(NodeId(id)).attributes.keys().cloned().collect()
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
        fn walk(doc: &Document, node: &HtmlBox, want: &str, all: bool, out: &mut Vec<u32>) {
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
        let mut b = HtmlBox::new(tag);
        b.node_id = arena_id.0;
        apply_property(&mut b.style, "display", crate::html::default_display(tag));
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// `document.createTextNode(data)`.
    pub fn create_text_node(&mut self, text: &str) -> u32 {
        let arena_id = self.arena.create_text(text);
        let mut b = HtmlBox::new("#text");
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
        let mut b = HtmlBox::new("#comment");
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
        let arena_parent = self.arena.get(NodeId(child_id)).parent;
        if arena_parent.is_some() {
            self.arena.remove_child(NodeId(child_id));
        }
        self.arena.append_child(NodeId(parent_id), NodeId(child_id));

        // Update HtmlBox tree
        let child_box = if let Some(b) = self.pending_nodes.remove(&child_id) {
            b
        } else {
            self.detach_htmlbox(child_id).unwrap_or_else(|| HtmlBox::new("#error"))
        };

        if let Some(parent) = self.find_htmlbox_mut(parent_id) {
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
        let arena_parent = self.arena.get(NodeId(child_id)).parent;
        if arena_parent.is_some() {
            self.arena.remove_child(NodeId(child_id));
        }
        self.arena.insert_before(NodeId(parent_id), NodeId(child_id), NodeId(reference_id));

        // Update HtmlBox tree
        let child_box = if let Some(b) = self.pending_nodes.remove(&child_id) {
            b
        } else {
            self.detach_htmlbox(child_id).unwrap_or_else(|| HtmlBox::new("#error"))
        };

        if let Some(parent) = self.find_htmlbox_mut(parent_id) {
            let idx = parent.children.iter()
                .position(|c| c.node_id == reference_id)
                .unwrap_or(parent.children.len());
            parent.children.insert(idx, child_box);
            parent.layout.layout_dirty = true;
            parent.layout.intrinsic_dirty = true;
            parent.has_dirty_layout_descendant = true;
        }
    }

    /// Remove a child from its parent. The node is dropped from the HtmlBox tree
    /// and freed in the arena.
    pub fn remove_child(&mut self, child_id: u32) {
        if child_id == 0 { return; }
        // Get parent before removing
        let parent_id = self.arena.get(NodeId(child_id)).parent.0;
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
        if let Some(detached) = self.detach_htmlbox(child_id) {
            self.pending_nodes.insert(child_id, detached);
        }
        // Mark parent dirty for layout
        if parent_id != 0 {
            if let Some(parent) = self.find_htmlbox_mut(parent_id) {
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
        if let Some(node) = self.find_htmlbox_mut(id) {
            node.attributes.insert(key.to_string(), value.to_string());
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
    }

    /// Remove an attribute from an element. Sets STYLE dirty flag.
    pub fn remove_attribute(&mut self, id: u32, key: &str) {
        if id == 0 { return; }
        let key = &self.fold_name(key);
        self.arena.remove_attribute(NodeId(id), key);
        if let Some(node) = self.find_htmlbox_mut(id) {
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

        // Remove arena children
        let nid = NodeId(id);
        let mut child = self.arena.get(nid).first_child;
        while child.is_some() {
            let next = self.arena.get(child).next_sibling;
            self.arena.remove_child(child);
            self.arena.free(child);
            child = next;
        }
        // Update HtmlBox tree
        if let Some(node) = self.find_htmlbox_mut(id) {
            node.children.clear();
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
        let new_children: Vec<HtmlBox> = match fragment
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

        // Clear existing arena children
        let nid = NodeId(id);
        let mut child = self.arena.get(nid).first_child;
        while child.is_some() {
            let next = self.arena.get(child).next_sibling;
            self.arena.remove_child(child);
            self.arena.free(child);
            child = next;
        }

        // Set new children on HtmlBox
        if let Some(node) = self.find_htmlbox_mut(id) {
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
        self.arena.get(NodeId(id))
            .attributes.get("class")
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
        if let Some(entry) = props.iter_mut().find(|(k, _)| k == &prop_lower) {
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
        self.get_node(id).or_else(|| self.get_box_by_id(id)).map(|node| node.layout.border_rect)
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

    /// Find a shared reference to an HtmlBox by node_id.
    ///
    /// A tree walk on purpose. `get_box_by_id` has an O(1) fast path through
    /// `node_index`, which is a `HashMap<u32, *const HtmlBox>` rebuilt only by
    /// `rebuild_node_index()` — that is, only at layout. Any DOM mutation in
    /// between moves boxes inside their parent's `Vec<HtmlBox>` and leaves
    /// those pointers dangling, and the fast path would hand one back. The DOM
    /// API mutates without laying out, so it must not use that index.
    pub(crate) fn find_htmlbox(&self, id: u32) -> Option<&HtmlBox> {
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
        fn find_pending<'a>(nodes: &'a [HtmlBox], id: u32) -> Option<&'a HtmlBox> {
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
        fn walk(node: &HtmlBox, id: u32) -> Option<&HtmlBox> {
            if node.node_id == id { return Some(node); }
            for child in &node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
        }
        walk(&self.root, id)
    }

    /// Find a mutable reference to an HtmlBox by node_id.
    ///
    /// Checks `pending_nodes` first, for the reason spelled out on
    /// `find_htmlbox` — a detached node is still a legal target for
    /// `setAttribute`, `setTextContent` and a style write.
    fn find_htmlbox_mut(&mut self, id: u32) -> Option<&mut HtmlBox> {
        if self.pending_nodes.contains_key(&id) {
            return self.pending_nodes.get_mut(&id);
        }
        fn walk(node: &mut HtmlBox, id: u32) -> Option<&mut HtmlBox> {
            if node.node_id == id { return Some(node); }
            for child in &mut node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
        }
        // The same detached-subtree search `find_htmlbox` explains, and the
        // half that actually loses nodes: `append_child` takes the child OUT of
        // `pending_nodes` before asking for its parent, so a parent this cannot
        // find means the child is already gone and is dropped in silence.
        //
        // Found in two passes because the owning root has to be identified
        // before the map can be borrowed mutably to walk into it.
        fn contains(node: &HtmlBox, id: u32) -> bool {
            node.node_id == id || node.children.iter().any(|child| contains(child, id))
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

    /// Detach an HtmlBox from its parent in the tree, returning the detached box.
    fn detach_htmlbox(&mut self, id: u32) -> Option<HtmlBox> {
        fn walk(node: &mut HtmlBox, id: u32) -> Option<HtmlBox> {
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
fn inner_text_into(node: &HtmlBox, out: &mut String) {
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
        let mut b = HtmlBox::new("#document-fragment");
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
        let (x, y) = (self.arena.get(NodeId(a)), self.arena.get(NodeId(b)));
        if x.tag != y.tag || x.namespace != y.namespace {
            return false;
        }
        if self.is_element(a) {
            if x.attributes != y.attributes || x.attribute_ns != y.attribute_ns {
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
            .find_htmlbox(id)
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
            .find_htmlbox(id)
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
        let mut event = crate::dom::HtmlEvent::new(crate::dom::HtmlEventType::Click);
        event.target = id;
        event.current_target = id;
        self.events.dispatch(&mut self.root, event);

        let mut dom_event = crate::dom::events::DomEvent::new("click", id);
        self.event_targets.dispatch_event(&self.arena, &mut dom_event);
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
            let tag = self.arena.get(NodeId(current)).tag.clone();
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
                let have = self.arena.get(NodeId(id)).attribute_ns.get(*name);
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
        let Some(node) = self.find_htmlbox_mut(id) else { return false };
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
            let node = self.find_htmlbox_mut(id)?;
            (
                node.image_data.take()?,
                node.image_width,
                node.image_height,
            )
        };
        let out = self.canvas_surfaces.with_context(id, &mut pixels, w, h, f);
        if let Some(node) = self.find_htmlbox_mut(id) {
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
        let Some(node) = self.find_htmlbox_mut(id) else { return };
        if node.tag != "canvas" {
            return;
        }
        node.image_width = width;
        node.image_height = height;
        node.image_data = Some(vec![0u8; (width as usize) * (height as usize) * 4]);
        node.attributes.insert("width".to_string(), width.to_string());
        node.attributes.insert("height".to_string(), height.to_string());
        self.canvas_surfaces.reset(id);
    }
}
