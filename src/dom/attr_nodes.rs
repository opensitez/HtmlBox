//! `NamedNodeMap` and `Attr` — DOM §4.9.1 / §4.9.2.

use crate::dom::arena::NodeId;
use crate::dom::attrs::Attr;
use crate::types::Document;

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
    pub(crate) fn attribute_entries(&self, id: u32) -> Vec<(String, String)> {
        if id != 0 && self.arena.is_alive(NodeId(id)) {
            let node = self.arena.get(NodeId(id));
            if node.node_type == crate::dom::arena::NodeType::Element {
                return node
                    .attributes
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
            }
            return Vec::new();
        }
        self.find_webcore(id)
            .map(|n| {
                n.attributes
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
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
