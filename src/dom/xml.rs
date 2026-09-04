//! XML in an HTML document — namespaces, CDATA sections and processing
//! instructions.

use crate::css::apply_property;
use crate::dom::arena::NodeId;
use crate::types::Document;
use crate::types::WebCore;

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
        apply_property(
            std::sync::Arc::make_mut(&mut b.style),
            "display",
            crate::html::default_display(qualified_name),
        );
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
        apply_property(std::sync::Arc::make_mut(&mut b.style), "display", "none");
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
        apply_property(std::sync::Arc::make_mut(&mut b.style), "display", "none");
        self.pending_nodes.insert(arena_id.0, b);
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        arena_id.0
    }

    /// `node.namespaceURI` — `None` for the null namespace, which is what
    /// every HTML element built by the parser has.
    pub fn namespace_uri(&self, id: u32) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return None;
        }
        self.arena.get(NodeId(id)).namespace.clone()
    }

    /// `node.prefix` — the part of the qualified name before the colon.
    pub fn prefix(&self, id: u32) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return None;
        }
        self.arena
            .get(NodeId(id))
            .tag
            .split_once(':')
            .map(|(prefix, _)| prefix.to_string())
    }

    /// `node.localName` — the qualified name without its prefix.
    pub fn local_name(&self, id: u32) -> String {
        if id == 0 {
            return String::new();
        }
        // ⛔ A shadow node is not an arena node, so the guard answered `""` for
        // every element in a shadow tree. The render tree holds the tag.
        if !self.arena.is_alive(NodeId(id)) {
            let Some(node) = self.find_webcore(id) else {
                return String::new();
            };
            let tag = node.tag.clone();
            return tag
                .split_once(':')
                .map(|(_, l)| l.to_string())
                .unwrap_or(tag);
        }
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
        if id == 0 {
            return;
        }
        self.set_attribute(id, qualified_name, value);
        if !self.arena.is_alive(NodeId(id)) {
            return;
        }
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
    pub fn get_attribute_ns(&self, id: u32, namespace: &str, local_name: &str) -> Option<String> {
        if id == 0 || !self.arena.is_alive(NodeId(id)) {
            return None;
        }
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
