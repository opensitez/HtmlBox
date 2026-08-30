//! Form association — HTML §4.10.18.3.

use crate::types::Document;

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
    pub(crate) fn walk_tree(&self, root: u32, f: &mut dyn FnMut(&Document, u32)) {
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
