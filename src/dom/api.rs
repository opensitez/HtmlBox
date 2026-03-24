//! Public DOM API for Document — the interface for scripting and dynamic manipulation.
//!
//! Every mutation goes through these methods, which update both the arena and the
//! HtmlBox tree (bridge period), and set dirty flags for incremental re-style/layout.

use std::collections::HashMap;
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
    /// Get the tag name of a node.
    pub fn dom_tag(&self, id: u32) -> Option<&str> {
        if id == 0 { return None; }
        Some(self.arena.get(NodeId(id)).tag.as_str())
    }

    /// Get an attribute value.
    pub fn dom_get_attribute(&self, id: u32, key: &str) -> Option<String> {
        if id == 0 { return None; }
        self.arena.get(NodeId(id)).attributes.get(key).cloned()
    }

    /// Get the text content of a node and all its descendants.
    pub fn dom_text_content(&self, id: u32) -> String {
        if id == 0 { return String::new(); }
        let mut out = String::new();
        self.collect_text(NodeId(id), &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        let node = self.arena.get(id);
        if !node.text.is_empty() {
            out.push_str(&node.text);
        }
        let mut child = node.first_child;
        while child.is_some() {
            self.collect_text(child, out);
            child = self.arena.get(child).next_sibling;
        }
    }

    /// Get the parent node_id (0 if root or not found).
    pub fn dom_parent(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.get(NodeId(id)).parent.0
    }

    /// Get child node_ids.
    pub fn dom_children(&self, id: u32) -> Vec<u32> {
        if id == 0 { return Vec::new(); }
        self.arena.children(NodeId(id)).map(|c| c.0).collect()
    }

    /// Get the next sibling node_id (0 if none).
    pub fn dom_next_sibling(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.get(NodeId(id)).next_sibling.0
    }

    /// Get the previous sibling node_id (0 if none).
    pub fn dom_prev_sibling(&self, id: u32) -> u32 {
        if id == 0 { return 0; }
        self.arena.get(NodeId(id)).prev_sibling.0
    }
}

// ─── Mutate ─────────────────────────────────────────────────────────────────

impl Document {
    /// Create a new element node. Returns its stable node_id.
    /// The node is detached — use `dom_append_child` or `dom_insert_before` to attach it.
    pub fn dom_create_element(&mut self, tag: &str) -> u32 {
        let arena_id = self.arena.create_element(tag);
        let mut b = HtmlBox::new(tag);
        b.node_id = arena_id.0;
        apply_property(&mut b.style, "display", crate::html::default_display(tag));
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// Create a new text node. Returns its stable node_id.
    pub fn dom_create_text(&mut self, text: &str) -> u32 {
        let arena_id = self.arena.create_text(text);
        let mut b = HtmlBox::new("#text");
        b.node_id = arena_id.0;
        b.text = text.to_string();
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// Append a child node to a parent.
    /// If child is in pending_nodes (just created), it's moved into the tree.
    /// If child is already in the tree, it's detached from its current parent first.
    pub fn dom_append_child(&mut self, parent_id: u32, child_id: u32) {
        if parent_id == 0 || child_id == 0 { return; }

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
        }
    }

    /// Insert a child before a reference node.
    pub fn dom_insert_before(&mut self, parent_id: u32, child_id: u32, reference_id: u32) {
        if parent_id == 0 || child_id == 0 || reference_id == 0 { return; }

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
        }
    }

    /// Remove a child from its parent. The node is dropped from the HtmlBox tree
    /// and freed in the arena.
    pub fn dom_remove_child(&mut self, child_id: u32) {
        if child_id == 0 { return; }
        self.arena.remove_child(NodeId(child_id));
        self.arena.free(NodeId(child_id));
        self.detach_htmlbox(child_id);
    }

    /// Set an attribute on an element. Sets STYLE dirty flag.
    pub fn dom_set_attribute(&mut self, id: u32, key: &str, value: &str) {
        if id == 0 { return; }
        self.arena.set_attribute(NodeId(id), key, value);
        if let Some(node) = self.find_htmlbox_mut(id) {
            node.attributes.insert(key.to_string(), value.to_string());
        }
    }

    /// Remove an attribute from an element. Sets STYLE dirty flag.
    pub fn dom_remove_attribute(&mut self, id: u32, key: &str) {
        if id == 0 { return; }
        self.arena.remove_attribute(NodeId(id), key);
        if let Some(node) = self.find_htmlbox_mut(id) {
            node.attributes.remove(key);
        }
    }

    /// Set the text content of a node, replacing all children.
    pub fn dom_set_text_content(&mut self, id: u32, text: &str) {
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
        self.arena.set_text(nid, text);

        // Update HtmlBox tree
        if let Some(node) = self.find_htmlbox_mut(id) {
            node.children.clear();
            node.text = text.to_string();
        }
    }

    /// Parse HTML and replace the children of the given node.
    pub fn dom_set_inner_html(&mut self, id: u32, html: &str) {
        if id == 0 { return; }

        // Parse HTML fragment
        let fragment = crate::html::parse_html(html);
        let new_children: Vec<HtmlBox> = if !fragment.root.children.is_empty()
            && fragment.root.children[0].tag == "body"
        {
            fragment.root.children.into_iter()
                .flat_map(|body| body.children.into_iter())
                .collect()
        } else {
            fragment.root.children
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

        // Rebuild arena for new children
        // We need to use raw pointer to work around borrow checker
        let root_ptr = &mut self.root as *mut HtmlBox;
        fn find_mut_raw(node: &mut HtmlBox, id: u32) -> Option<&mut HtmlBox> {
            if node.node_id == id { return Some(node); }
            for child in &mut node.children {
                if let Some(found) = find_mut_raw(child, id) { return Some(found); }
            }
            None
        }
        unsafe {
            if let Some(node) = find_mut_raw(&mut *root_ptr, id) {
                for child in &mut node.children {
                    crate::html::rebuild_arena_recursive_pub(&mut self.arena, child, NodeId(id));
                }
            }
        }
    }

    // ── classList ──

    /// Add a class to the element's class list.
    pub fn class_list_add(&mut self, id: u32, class: &str) {
        if id == 0 || class.is_empty() { return; }
        let current = self.dom_get_attribute(id, "class").unwrap_or_default();
        if current.split_whitespace().any(|c| c == class) { return; }
        let new_val = if current.is_empty() {
            class.to_string()
        } else {
            format!("{} {}", current, class)
        };
        self.dom_set_attribute(id, "class", &new_val);
    }

    /// Remove a class from the element's class list.
    pub fn class_list_remove(&mut self, id: u32, class: &str) {
        if id == 0 || class.is_empty() { return; }
        let current = self.dom_get_attribute(id, "class").unwrap_or_default();
        let new_val: Vec<&str> = current.split_whitespace().filter(|&c| c != class).collect();
        let joined = new_val.join(" ");
        if joined.is_empty() {
            self.dom_remove_attribute(id, "class");
        } else {
            self.dom_set_attribute(id, "class", &joined);
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
        let current = self.dom_get_attribute(id, "style").unwrap_or_default();
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
        self.dom_set_attribute(id, "style", &new_style);
    }

    /// Get a single CSS property from the element's inline style.
    pub fn get_style_property(&self, id: u32, prop: &str) -> Option<String> {
        let style_attr = self.dom_get_attribute(id, "style")?;
        let prop_lower = prop.to_ascii_lowercase();
        parse_inline_style(&style_attr)
            .into_iter()
            .find(|(k, _)| k == &prop_lower)
            .map(|(_, v)| v)
    }

    /// Remove a CSS property from the element's inline style.
    pub fn remove_style_property(&mut self, id: u32, prop: &str) {
        if id == 0 { return; }
        let current = self.dom_get_attribute(id, "style").unwrap_or_default();
        let prop_lower = prop.to_ascii_lowercase();
        let props: Vec<(String, String)> = parse_inline_style(&current)
            .into_iter()
            .filter(|(k, _)| k != &prop_lower)
            .collect();
        if props.is_empty() {
            self.dom_remove_attribute(id, "style");
        } else {
            let new_style = props.iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("; ");
            self.dom_set_attribute(id, "style", &new_style);
        }
    }

    // ── Layout queries ──

    /// Get the bounding rect of a node (border box in document coordinates).
    pub fn dom_get_bounding_rect(&self, id: u32) -> Option<Rect> {
        let node = crate::types::find_by_node_id(&self.root, id);
        if node.is_null() { return None; }
        Some(unsafe { &*node }.border_rect)
    }

    /// Get the offset width (border box width).
    pub fn dom_offset_width(&self, id: u32) -> f32 {
        self.dom_get_bounding_rect(id).map(|r| r.w).unwrap_or(0.0)
    }

    /// Get the offset height (border box height).
    pub fn dom_offset_height(&self, id: u32) -> f32 {
        self.dom_get_bounding_rect(id).map(|r| r.h).unwrap_or(0.0)
    }

    // ── Internal helpers ──

    /// Find a mutable reference to an HtmlBox by node_id.
    fn find_htmlbox_mut(&mut self, id: u32) -> Option<&mut HtmlBox> {
        fn walk(node: &mut HtmlBox, id: u32) -> Option<&mut HtmlBox> {
            if node.node_id == id { return Some(node); }
            for child in &mut node.children {
                if let Some(found) = walk(child, id) { return Some(found); }
            }
            None
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
